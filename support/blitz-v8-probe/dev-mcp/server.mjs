import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { createConnection } from "node:net";
import { z } from "zod";
import { pixiStatus } from "./pixi.mjs";

// After stdin closes, Roves may legitimately spend up to its Rust
// GPU_SHUTDOWN_TIMEOUT (120 s, src/main.rs) draining the WebGPU queue, plus
// wgpu-core's own ~6 s Queue Drop wait; a slow software adapter needs well
// over 8 s. Keep this above that sum and below playthrough.mjs's
// SERVER_EXIT_BUDGET_MS so each layer's own timeout reports the failure.
const CHILD_EXIT_BUDGET_MS = 150_000;

const [probe, game] = process.argv.slice(2);
const attach = probe === "--attach";
const discover = probe === "--discover";
if (discover ? process.argv.length > 4 : !probe || !game || process.argv.length !== 4) {
  throw new Error("Usage: node server.mjs <dev-mcp probe executable> <game dist> | --attach <port> | --discover [pid]");
}
const attachToken = process.env.ROVES_PROBE_DEV_MCP_TOKEN;
const attachPort = Number(game);
if (attach && (!Number.isInteger(attachPort) || attachPort < 1 || attachPort > 65535 ||
    !/^[0-9a-fA-F]{32}$/.test(attachToken ?? ""))) {
  throw new Error("Set a valid local attach port and ROVES_PROBE_DEV_MCP_TOKEN");
}

function discoverSession() {
  if (!process.env.LOCALAPPDATA) throw new Error("Discovery requires a Windows user profile");
  const directory = join(process.env.LOCALAPPDATA, "Roves", "mcp");
  let files;
  try {
    files = readdirSync(directory);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    files = [];
  }
  const sessions = [];
  for (const file of files) {
    if (!/^\d+\.json$/.test(file) || (game && file !== game + ".json")) continue;
    try {
      const entry = JSON.parse(readFileSync(join(directory, file), "utf8"));
      if (entry.version !== 1 || file !== entry.pid + ".json" ||
          !Number.isInteger(entry.pid) || entry.pid < 1 ||
          !Number.isInteger(entry.port) || entry.port < 1 || entry.port > 65535 ||
          typeof entry.token !== "string" || !/^[0-9a-f]{32}$/.test(entry.token)) continue;
      process.kill(entry.pid, 0);
      sessions.push(entry);
    } catch {
      // A session can disappear between listing the directory and checking its PID.
    }
  }
  if (!sessions.length) throw new Error("Open a Roves game with MCP discovery enabled");
  if (sessions.length > 1) {
    throw new Error("Multiple Roves games are open. Select a PID: " + sessions.map(({ pid }) => pid).join(", "));
  }
  return sessions[0];
}

async function requestAttached(payload, { port, token }) {
  const socket = createConnection({ host: "127.0.0.1", port });
  socket.setTimeout(5_000, () => socket.destroy(new Error("Roves attach timed out")));
  const lines = createInterface({ input: socket });
  try {
    socket.write(token + "\n" + JSON.stringify(payload) + "\n");
    for await (const line of lines) {
      const result = JSON.parse(line);
      if ("error" in result) throw new Error(result.error);
      return result.value;
    }
    throw new Error("Roves closed the connection before replying");
  } finally {
    lines.close();
    socket.destroy();
  }
}

class Probe {
  #pending;
  #closed = false;
  #child;
  #ready;
  #stopping = false;

  constructor() {
    const token = randomBytes(16).toString("hex");
    const prefix = "ROVES_DEV_MCP_" + token + " ";
    const storage = mkdtempSync(join(tmpdir(), "roves-dev-mcp-"));
    this.#child = spawn(resolve(probe), ["--game", resolve(game), "--window"], {
      env: { ...process.env, ROVES_PROBE_DEV_MCP_TOKEN: token, ROVES_PROBE_DEV_MCP_ATTACH_PORT: undefined, ROVES_PROBE_STORAGE_DIR: storage, ROVES_PROBE_FRAME_TIMINGS: "1" },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.#ready = this.#wait(60_000, "Roves did not become ready");
    const lines = createInterface({ input: this.#child.stdout });
    lines.on("line", (line) => {
      if (line.startsWith(prefix) && this.#pending) {
        try {
          this.#finish(null, JSON.parse(line.slice(prefix.length)));
        } catch (error) {
          this.#finish(error);
        }
      } else if (!line.startsWith(prefix)) {
        process.stderr.write(line.slice(0, 1024) + "\n");
      }
    });
    this.#child.stderr.pipe(process.stderr, { end: false });
    this.#child.stdin.on("error", (error) => this.#finish(error));
    this.#child.on("error", (error) => this.#finish(error));
    this.#child.once("close", (code) => {
      this.#closed = true;
      if (this.#stopping && code !== 0) {
        process.exitCode ||= 1;
        process.stderr.write("Roves did not exit cleanly (exit " + code + ")\n");
      }
      this.#finish(new Error("Roves closed (exit " + code + ")"));
      lines.close();
      try {
        rmSync(storage, { recursive: true, force: true });
      } catch (error) {
        process.stderr.write("Could not remove isolated test storage: " + error.message + "\n");
      }
    });
  }

  #finish(error, result) {
    const pending = this.#pending;
    if (!pending) return;
    this.#pending = undefined;
    clearTimeout(pending.timer);
    if (error) pending.reject(error);
    else pending.resolve(result);
  }

  #wait(milliseconds, message) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#finish(new Error(message));
        this.stop();
      }, milliseconds);
      this.#pending = { resolve, reject, timer };
    });
  }

  async request(payload) {
    const ready = await this.#ready;
    if (!ready.ready || this.#closed) throw new Error("Roves is not ready");
    if (this.#pending) throw new Error("Another diagnostic request is in progress");
    if (typeof payload === "string" && Buffer.byteLength(payload) > 16 * 1024) throw new Error("Source exceeds 16 KiB");
    const response = this.#wait(5_000, "Roves diagnostic request timed out");
    this.#child.stdin.write(JSON.stringify(payload) + "\n");
    const result = await response;
    if ("error" in result) throw new Error(result.error);
    return result.value;
  }

  stop() {
    if (this.#closed) return;
    this.#closed = true;
    this.#stopping = true;
    this.#child.stdin.end();
    // Last-resort guard for a hung exit, never the expected path: Roves closes
    // the window on stdin EOF and then drains the WebGPU queue, failing loudly
    // itself (non-zero exit) after GPU_SHUTDOWN_TIMEOUT. A kill still fails the
    // run through the `close` handler above.
    const deadline = setTimeout(() => {
      process.stderr.write("Roves did not exit within " + CHILD_EXIT_BUDGET_MS / 1000 + " s after stdin closed; killing it\n");
      this.#child.kill();
    }, CHILD_EXIT_BUDGET_MS);
    deadline.unref();
    this.#child.once("close", () => clearTimeout(deadline));
  }
}

let child;
const shutdown = () => child?.stop();
process.stdin.once("end", shutdown);
for (const [signal, code] of [["SIGINT", 130], ["SIGTERM", 143]]) {
  process.once(signal, () => {
    shutdown();
    process.stdin.destroy();
    process.exitCode = code;
  });
}

await serveStdio(() => {
  const server = new McpServer({ name: "roves-dev", version: "0.0.0" });
  async function run(payload) {
    try {
      if (typeof payload === "string" && Buffer.byteLength(payload) > 16 * 1024) {
        throw new Error("Source exceeds 16 KiB");
      }
      if (!attach && !discover) child ??= new Probe();
      const value = discover
        ? await requestAttached(payload, discoverSession())
        : attach
          ? await requestAttached(payload, { port: attachPort, token: attachToken })
          : await child.request(payload);
      return { content: [{ type: "text", text: JSON.stringify(value) }] };
    } catch (error) {
      return { isError: true, content: [{ type: "text", text: error.message }] };
    }
  }
  server.registerTool("evaluate_script", {
    description: "Evaluate synchronous JavaScript in the isolated Roves game page and return JSON.",
    inputSchema: z.object({ source: z.string().max(16 * 1024) }),
  }, ({ source }) => run(source));
  server.registerTool("performance_sample", {
    description: "Summarize game ticks and CPU rendering phases sampled in the last five seconds.",
    inputSchema: z.object({}),
  }, () => run({ method: "performance_sample" }));
  server.registerTool("pixi_status", {
    description: "Inspect a registered PixiJS scene and renderer without modifying the game.",
    inputSchema: z.object({}),
  }, () => run(pixiStatus));
  return server;
});
