import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const probe = process.argv[2];
assert.ok(probe, "Usage: node smoke.mjs <dev-mcp probe executable | --list-only>");
const listOnly = probe === "--list-only";
const discovery = process.argv[3] === "--discover";
const fixture = fileURLToPath(new URL("./fixture", import.meta.url));
const profile = discovery ? mkdtempSync(join(tmpdir(), "roves-mcp-discovery-")) : null;
const env = profile
  ? { ...process.env, LOCALAPPDATA: profile, ROVES_PROBE_STORAGE_DIR: join(profile, "saves") }
  : process.env;
const game = discovery
  ? spawn(resolve(probe), ["--game", fixture, "--window", "--mcp-discover"], {
      stdio: ["ignore", "ignore", "inherit"], env, windowsHide: true,
    })
  : null;
const gameClosed = game ? once(game, "close") : null;
const child = spawn(process.execPath, [
  fileURLToPath(new URL("./server.mjs", import.meta.url)),
  ...(discovery ? ["--discover"] : [listOnly ? "unused-probe" : probe, fixture]),
], { stdio: ["pipe", "pipe", "inherit"], env, windowsHide: true });
const closed = once(child, "close");
const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
let nextId = 0;
const deadline = setTimeout(() => child.kill(), 90_000);

function send(message) {
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", ...message }) + "\n");
}

async function call(method, params) {
  const id = ++nextId;
  send({ id, method, params });
  while (true) {
    const { done, value } = await lines.next();
    assert.ok(!done, `MCP closed while waiting for ${method}`);
    const reply = JSON.parse(value);
    if (reply.id !== id) continue;
    assert.equal(reply.error, undefined);
    return reply.result;
  }
}

function evaluate(source) {
  return call("tools/call", { name: "evaluate_script", arguments: { source } });
}

try {
  await call("initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "roves-smoke", version: "0.0.0" },
  });
  send({ method: "notifications/initialized" });
  const { tools } = await call("tools/list", {});
  assert.deepEqual(tools.map(({ name }) => name).sort(), ["evaluate_script", "performance_sample", "pixi_status"]);
  if (!listOnly) {
    if (game) {
      const record = join(profile, "Roves", "mcp", game.pid + ".json");
      const readyBy = Date.now() + 60_000;
      while (!existsSync(record) && game.exitCode === null && Date.now() < readyBy) {
        await delay(100);
      }
      assert.ok(existsSync(record), "The open game should publish its MCP connection record");
    }
    const result = await evaluate(`({
      text: document.getElementById("fixture").textContent,
      privileged: typeof globalThis.Deno,
      storage: localStorage.getItem("roves-smoke")
    })`);
    assert.ok(!result.isError, result.content[0].text);
    assert.deepEqual(JSON.parse(result.content[0].text), {
      text: "Roves MCP fixture", privileged: "undefined", storage: null,
    });
    const changed = await evaluate(`localStorage.setItem("roves-smoke", "set");
      document.getElementById("fixture").textContent = "updated";
      document.getElementById("fixture").textContent`);
    assert.equal(JSON.parse(changed.content[0].text), "updated");
    const failure = await evaluate("throw new Error('fixture exception')");
    assert.equal(failure.isError, true);
    assert.match(failure.content[0].text, /fixture exception/);
    assert.equal((await evaluate("Promise.resolve(42)")).isError, true);
    assert.equal(JSON.parse((await evaluate("40 + 2")).content[0].text), 42);
    const scheduled = await evaluate(`globalThis.rovesMcpTimerReady = false;
      setTimeout(() => { globalThis.rovesMcpTimerReady = true; }, 0);
      true`);
    assert.equal(JSON.parse(scheduled.content[0].text), true);
    let timerReady = false;
    for (let attempt = 0; attempt < 10 && !timerReady; attempt++) {
      const response = await evaluate("globalThis.rovesMcpTimerReady");
      timerReady = JSON.parse(response.content[0].text);
      if (!timerReady) await delay(50);
    }
    assert.equal(timerReady, true, "a timer scheduled through MCP should run on the game loop");
    const sample = await call("tools/call", { name: "performance_sample", arguments: {} });
    assert.ok(!sample.isError, sample.content[0].text);
    const metrics = JSON.parse(sample.content[0].text);
    assert.equal(metrics.window_ms, 5000);
    assert.ok(metrics.cpu_frame_step.count > 0);
    assert.equal(metrics.displayed_frames, null);
    async function pixi() {
      const reply = await call("tools/call", { name: "pixi_status", arguments: {} });
      assert.ok(!reply.isError, reply.content[0].text);
      return JSON.parse(reply.content[0].text);
    }
    assert.equal((await pixi()).status, "not_detected");
    await evaluate("globalThis.PIXI = {}; true");
    assert.equal((await pixi()).status, "detected_without_scene");
    await evaluate(`globalThis.__PIXI_APP__ = {
      stage: { label: "fixture-stage", visible: true, children: [] },
      renderer: { name: "webgpu", resolution: 2, canvas: { width: 640, height: 360 } }
    }; true`);
    const registered = await pixi();
    assert.equal(registered.status, "scene_available");
    assert.equal(registered.scene.label, "fixture-stage");
    assert.equal(registered.renderer.canvas_width, 640);
  }
  child.stdin.end();
  assert.deepEqual(await closed, [0, null]);
  if (game) {
    assert.equal(game.exitCode, null, "Disconnecting MCP should leave the game running");
    process.kill(game.pid, 0);
  }
  console.log(listOnly ? "MCP tool discovery passed" : "Roves MCP window smoke passed");
} finally {
  clearTimeout(deadline);
  child.stdin.end();
  if (game) {
    game.kill();
    try {
      await gameClosed;
    } finally {
      rmSync(profile, { recursive: true, force: true });
    }
  }
}
