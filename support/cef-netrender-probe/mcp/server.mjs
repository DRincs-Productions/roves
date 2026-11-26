// Roves local MCP server (stdio) for a running Roves CEF game on Windows, macOS or Linux.
// See README.md here and "Local MCP bridge" in support/cef-netrender-probe/README.md.
//
// The game publishes <session directory>/<pid>.json at launch when its bridge is on
// (--roves-mcp, or the package marker mcp/enabled). The session directory is per user:
//   Windows  %LOCALAPPDATA%\Roves\mcp
//   macOS    $HOME/Library/Application Support/Roves/mcp
//   Linux    $XDG_RUNTIME_DIR/roves/mcp, else $HOME/.local/state/roves/mcp
// (the same rule as the game's mcp_rules.rs `record_dir`). Every tool call rereads those
// records, so the game may start after this server; the call goes to the game's local
// endpoint (a named pipe on Windows, a unix-domain socket in the session directory on
// macOS/Linux) after the session token. No TCP port is used.
import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { createConnection } from "node:net";
import { isAbsolute, join } from "node:path";
import { createInterface } from "node:readline";
import { z } from "zod";

const args = process.argv.slice(2);
let pinned;
if (args.length === 2 && args[0] === "--pid" && /^[1-9]\d*$/.test(args[1])) {
  pinned = Number(args[1]);
} else if (args.length) {
  throw new Error("Usage: node server.mjs [--pid <game process id>]");
}

const PIPE = /^\\\\\.\\pipe\\roves-mcp-([1-9]\d*)-[0-9a-f]{32}$/;
const TOKEN = /^[0-9a-f]{64}$/;
const KEYS = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Enter", "Escape", "Space",
  "Tab", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown"];

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // EPERM: the process exists but belongs to someone else, so it is not this user's game.
    return false;
  }
}

/** The per-user session directory of this platform (mcp_rules.rs `record_dir`). */
function sessionDirectory(platform = process.platform, env = process.env) {
  const absolute = (name) => (env[name] && isAbsolute(env[name]) ? env[name] : undefined);
  if (platform === "win32") {
    if (!env.LOCALAPPDATA) throw new Error("Roves MCP needs a Windows user profile (LOCALAPPDATA)");
    return join(env.LOCALAPPDATA, "Roves", "mcp");
  }
  if (platform === "darwin") {
    if (!absolute("HOME")) throw new Error("Roves MCP needs HOME (an absolute path)");
    return join(env.HOME, "Library", "Application Support", "Roves", "mcp");
  }
  if (platform === "linux") {
    if (absolute("XDG_RUNTIME_DIR")) return join(env.XDG_RUNTIME_DIR, "roves", "mcp");
    if (!absolute("HOME")) throw new Error("Roves MCP needs XDG_RUNTIME_DIR or HOME (an absolute path)");
    return join(env.HOME, ".local", "state", "roves", "mcp");
  }
  throw new Error(`Roves MCP is not available on ${platform}`);
}

/** Whether `entry.pipe` is the endpoint a game with this PID publishes on this platform. */
function validEndpoint(entry, directory) {
  if (process.platform === "win32") {
    const pipe = PIPE.exec(entry.pipe ?? "");
    return Boolean(pipe) && Number(pipe[1]) === entry.pid;
  }
  // macOS/Linux: the socket <pid>.sock in the same private directory.
  return entry.pipe === join(directory, `${entry.pid}.sock`);
}

/** Running sessions of this user, newest record format only. */
function sessions() {
  const directory = sessionDirectory();
  let files;
  try {
    files = readdirSync(directory);
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  const found = [];
  for (const file of files) {
    const match = /^([1-9]\d*)\.json$/.exec(file);
    if (!match) continue;
    try {
      const path = join(directory, file);
      // macOS/Linux: the directory is private (0700); a record not owned by this user is
      // not one of this user's games.
      if (process.getuid && statSync(path).uid !== process.getuid()) continue;
      const entry = JSON.parse(readFileSync(path, "utf8"));
      if (entry.version !== 1 || entry.pid !== Number(match[1]) || !validEndpoint(entry, directory) ||
          !TOKEN.test(entry.token ?? "") || typeof entry.origin !== "string" || !alive(entry.pid)) continue;
      found.push(entry);
    } catch {
      // A record can disappear or be half-replaced while the directory is read.
    }
  }
  return found;
}

function select() {
  const running = sessions().filter(({ pid }) => pinned === undefined || pid === pinned);
  if (!running.length) {
    throw new Error(pinned === undefined
      ? "No Roves game with MCP enabled is running. Start it with --roves-mcp (or an MCP-enabled package)."
      : `The Roves game with PID ${pinned} is not running with MCP enabled.`);
  }
  if (running.length > 1) {
    throw new Error("Several Roves games are running. Restart this server with --pid <PID>, one of: " +
      running.map(({ pid }) => pid).join(", "));
  }
  return running[0];
}

/** Sends one request to the game and returns its value. */
async function request(method, params = {}) {
  const session = select();
  const socket = createConnection(session.pipe);
  socket.setTimeout(15_000, () => socket.destroy(new Error("The Roves game did not answer in time")));
  const lines = createInterface({ input: socket });
  try {
    socket.write(session.token + "\n" + JSON.stringify({ method, params }) + "\n");
    for await (const line of lines) {
      const reply = JSON.parse(line);
      if ("error" in reply) throw new Error(reply.error);
      return reply.value;
    }
    throw new Error("The Roves game closed the connection without an answer");
  } finally {
    lines.close();
    socket.destroy();
  }
}

// Discovery at launch: report what is already running (stderr; stdout carries MCP).
try {
  const running = sessions();
  process.stderr.write(running.length
    ? `roves-mcp: found ${running.map(({ pid, origin }) => `pid=${pid} origin=${origin}`).join("; ")}\n`
    : "roves-mcp: no running game yet; tools look again on every call\n");
} catch (error) {
  process.stderr.write(`roves-mcp: ${error.message}\n`);
}

await serveStdio(() => {
  const server = new McpServer({ name: "roves-cef", version: "0.0.0" });
  const tool = (name, description, inputSchema, call) => server.registerTool(name,
    { description, inputSchema },
    async (input) => {
      try {
        const value = await call(input);
        return { content: [{ type: "text", text: JSON.stringify(value) }] };
      } catch (error) {
        return { isError: true, content: [{ type: "text", text: error.message }] };
      }
    });
  tool("get_state",
    "Read the running Roves game's host-side state: origin, URL, title, load/startup state, " +
    "whether input is accepted, window size in points, frame counters and recent console lines.",
    z.object({}), () => request("state"));
  tool("evaluate_script",
    "Evaluate a synchronous JavaScript expression in the game page and return its JSON value. " +
    "Promises are refused: start asynchronous work in one call and read its result later. " +
    "The page's Content Security Policy still applies to eval inside the expression. " +
    "Trusted development command: it can change game state.",
    z.object({ expression: z.string().min(1).max(16 * 1024) }),
    ({ expression }) => request("evaluate", { expression }));
  tool("click",
    "Left-click at a window point (x, y in window points = CSS pixels at page zoom 1, origin top-left), " +
    "through the same input path as a real mouse click.",
    z.object({ x: z.number().nonnegative(), y: z.number().nonnegative() }),
    ({ x, y }) => request("click", { x, y }));
  tool("press_key",
    "Press and release one non-character key (keydown + keyup, no character input).",
    z.object({ key: z.enum(KEYS) }),
    ({ key }) => request("press_key", { key }));
  tool("type_text",
    "Type up to 256 printable characters as text input into the focused page element.",
    z.object({ text: z.string().min(1).max(256) }),
    ({ text }) => request("type_text", { text }));
  return server;
});
