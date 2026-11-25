// Smoke test of the Roves local MCP bridge against a real game process (Windows).
//
//   node smoke.mjs list-only <server.mjs>
//   node smoke.mjs fixture <server.mjs> <play.exe> <fixture dir> <output dir>
//   node smoke.mjs package <server.mjs> <package dir> <output dir>
//   node smoke.mjs disabled <server.mjs> <plain play.exe> <fixture dir> <output dir>
//
// fixture: --roves-game-dir=<fixture> --roves-mcp; discovery by PID, authentication refusal,
//          state, the isMcpEnabled() marker (true and read-only in the entry script of the
//          top-level document, absent in a same-origin child frame, no other bridge global),
//          evaluation (values, objects by value, errors, Promise refusal distinct
//          from a plain {}, non-JSON symbol refused, page CSP kept), click, key and
//          text delivered to the page, clean exit and session removal.
// disabled: the plain package's play.exe (no mcp\enabled) on the fixture, twice: without
//          --roves-mcp, and with --roves-mcp while the bridge cannot start (LOCALAPPDATA
//          points at a file). Both must load the game with the MCP marker absent in the
//          entry script and the child frame (the host API marker __ROVES__ present), publish
//          no session record and exit 0.
// package: the package's play.exe launched directly (the package marker turns MCP on);
//          discovery without a PID, state and evaluation on the packaged game; interaction
//          is required to succeed only if the startup screen was released (real GPU),
//          otherwise to be refused as not ready (WARP); clean exit and session removal.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createConnection } from "node:net";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";

const [mode, serverPath, ...rest] = process.argv.slice(2);
const usage = "Usage: node smoke.mjs list-only <server.mjs> | fixture <server.mjs> <play.exe> " +
  "<fixture dir> <output dir> | package <server.mjs> <package dir> <output dir> | " +
  "disabled <server.mjs> <plain play.exe> <fixture dir> <output dir>";
const MARKER = "__ROVES_MCP__";
assert.ok(serverPath, usage);
const TOOLS = ["click", "evaluate_script", "get_state", "press_key", "type_text"];
const log = (line) => process.stdout.write(`roves-mcp-smoke ${line}\n`);

class Client {
  #lines;
  #nextId = 0;
  child;
  stderr = "";

  constructor(args = []) {
    this.child = spawn(process.execPath, [resolve(serverPath), ...args],
      { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
    this.child.stderr.on("data", (chunk) => { this.stderr += chunk; });
    this.#lines = createInterface({ input: this.child.stdout })[Symbol.asyncIterator]();
  }

  async rpc(method, params) {
    const id = ++this.#nextId;
    this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    while (true) {
      const { done, value } = await this.#lines.next();
      assert.ok(!done, `MCP server closed while waiting for ${method}: ${this.stderr}`);
      const reply = JSON.parse(value);
      if (reply.id === id) return reply;
    }
  }

  async start() {
    const init = await this.rpc("initialize", {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "roves-mcp-smoke", version: "0.0.0" },
    });
    assert.equal(init.error, undefined, JSON.stringify(init.error));
    this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
    const list = await this.rpc("tools/list", {});
    assert.deepEqual(list.result.tools.map(({ name }) => name).sort(), TOOLS);
    return this;
  }

  /** { ok, value, text }: ok=false for a tool error or a protocol-level refusal. */
  async tool(name, args = {}) {
    const reply = await this.rpc("tools/call", { name, arguments: args });
    if (reply.error) return { ok: false, text: reply.error.message };
    const text = reply.result.content?.[0]?.text ?? "";
    if (reply.result.isError) return { ok: false, text };
    return { ok: true, value: JSON.parse(text), text };
  }

  async value(name, args) {
    const result = await this.tool(name, args);
    assert.ok(result.ok, `${name} ${JSON.stringify(args)} failed: ${result.text}`);
    return result.value;
  }

  async stop() {
    this.child.stdin.end();
    const timer = setTimeout(() => this.child.kill(), 5_000);
    await once(this.child, "close");
    clearTimeout(timer);
  }
}

async function until(what, ms, probe) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try {
      last = await probe();
      if (last.done) return last.value;
    } catch (error) {
      last = { error: error.message };
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for ${what}; last: ${JSON.stringify(last)}`);
}

function recordPath(pid) {
  assert.ok(process.env.LOCALAPPDATA, "LOCALAPPDATA is not set");
  return join(process.env.LOCALAPPDATA, "Roves", "mcp", `${pid}.json`);
}

async function waitRecord(pid) {
  return until(`session record of PID ${pid}`, 30_000, async () => {
    const path = recordPath(pid);
    if (!existsSync(path)) return { done: false };
    return { done: true, value: JSON.parse(readFileSync(path, "utf8")) };
  });
}

function checkRecord(record, pid) {
  assert.equal(record.version, 1);
  assert.equal(record.pid, pid);
  const prefix = String.raw`\\.\pipe\roves-mcp-` + pid + "-";
  assert.ok(record.pipe.startsWith(prefix) && /^[0-9a-f]{32}$/.test(record.pipe.slice(prefix.length)),
    `unexpected pipe name ${record.pipe}`);
  assert.match(record.token, /^[0-9a-f]{64}$/);
  assert.match(record.origin, /^http:\/\/127\.0\.0\.1:\d+$/);
}

/** Without the right token the pipe answers nothing and closes the connection. */
async function refusedWithout(record, firstLine) {
  const socket = createConnection(record.pipe);
  let received = "";
  socket.on("data", (chunk) => { received += chunk; });
  socket.on("error", () => {});
  await once(socket, "connect");
  socket.write(firstLine + "\n" + JSON.stringify({ method: "state" }) + "\n");
  const closed = once(socket, "close");
  const timer = setTimeout(() => socket.destroy(new Error("still open")), 10_000);
  await closed;
  clearTimeout(timer);
  assert.equal(received, "", `unauthenticated client received: ${received}`);
}

async function waitExit(game, ms) {
  if (game.exitCode !== null) return game.exitCode;
  const timer = setTimeout(() => game.kill(), ms);
  const [code] = await once(game, "exit");
  clearTimeout(timer);
  return code;
}

function launch(exe, args, out, env = process.env) {
  mkdirSync(out, { recursive: true });
  const game = spawn(exe, args, { cwd: out, stdio: "ignore", windowsHide: false, env });
  game.on("error", (error) => log(`launch error ${error.message}`));
  log(`launched pid=${game.pid} ${exe}`);
  return game;
}

function checkLog(path, record, source) {
  const text = readFileSync(path, "utf8");
  assert.ok(!text.includes(record.token), "the session token appears in the game log");
  assert.match(text, new RegExp(`mcp enabled source=${source} pipe=`));
  assert.match(text, /mcp session removed calls=\d+/);
  assert.match(text, /mcp call method=state ok=true/);
  return text;
}

async function fixture([exe, fixtureDir, out]) {
  assert.ok(exe && fixtureDir && out, usage);
  const gameLog = join(resolve(out), "game.log");
  const game = launch(resolve(exe), [
    `--roves-game-dir=${resolve(fixtureDir)}`, "--roves-mcp", "--roves-auto-close-ms=45000",
    "--roves-timeout-ms=90000", `--roves-log=${gameLog}`,
    `--roves-cache-dir=${join(resolve(out), "cache")}`,
    `--log-file=${join(resolve(out), "cef-debug.log")}`,
  ], resolve(out));
  let client;
  try {
    const record = await waitRecord(game.pid);
    checkRecord(record, game.pid);
    log(`record pipe=${record.pipe} origin=${record.origin}`);
    await refusedWithout(record, "0".repeat(64));
    await refusedWithout(record, "");
    log("unauthenticated connections refused");

    client = await new Client(["--pid", String(game.pid)]).start();
    const state = await until("fixture loaded with input", 60_000, async () => {
      const value = await client.value("get_state");
      return { done: value.loaded && value.input_ready, value };
    });
    assert.equal(state.pid, game.pid);
    assert.equal(state.origin, record.origin);
    assert.equal(state.launch, "game-dir");
    log(`state ${JSON.stringify({ ...state, console: undefined })}`);

    const evaluate = (expression) => client.tool("evaluate_script", { expression });
    const page = await until("fixture script", 20_000, async () => {
      const value = await client.value("evaluate_script", {
        expression: "({ origin: location.origin, title: document.title, ready: globalThis.rovesMcpFixture?.ready === true })",
      });
      return { done: value.ready, value };
    });
    assert.deepEqual(page, { origin: record.origin, title: "roves mcp fixture", ready: true });
    assert.equal(state.page_capability, "registered");

    // isMcpEnabled(): the entry script (first page code) already saw a read-only true marker.
    assert.deepEqual(await client.value("evaluate_script", { expression: "rovesMcpEntry" }), {
      enabled: true,
      descriptor: { value: true, writable: false, enumerable: false, configurable: false },
    });
    // Neither assignment nor delete changes it.
    assert.equal(await client.value("evaluate_script", {
      expression: `(() => { try { window.${MARKER} = false; } catch {} delete window.${MARKER}; ` +
        `return window.${MARKER}; })()`,
    }), true);
    // Top-level document only: a same-origin child frame has no marker.
    const frame = await until("child frame script", 20_000, async () => {
      const value = await client.value("evaluate_script", {
        expression: "document.getElementById('child-frame').contentWindow.rovesMcpFrame ?? null",
      });
      return { done: value !== null, value };
    });
    assert.deepEqual(frame, { enabled: false, own: false });
    log("page capability marker checks passed");
    assert.equal((await evaluate("40 + 2")).value, 42);
    assert.equal((await evaluate("undefined")).value, null);
    const thrown = await evaluate("throw new Error('fixture exception')");
    assert.ok(!thrown.ok && /fixture exception/.test(thrown.text), thrown.text);
    const promise = await evaluate("Promise.resolve(42)");
    assert.ok(!promise.ok && /Promise/.test(promise.text), promise.text);
    // A by-value Promise and a plain empty object have the same JSON ({}): the refusal must
    // come from the Promise's type, not from the value's shape.
    const asyncCall = await evaluate("(async () => 42)()");
    assert.ok(!asyncCall.ok && /Promise/.test(asyncCall.text), asyncCall.text);
    const empty = await evaluate("({})");
    assert.ok(empty.ok, empty.text);
    assert.deepEqual(empty.value, {});
    assert.deepEqual((await evaluate("({ list: [1, 'two', null], flag: true })")).value,
      { list: [1, "two", null], flag: true });
    const symbol = await evaluate("Symbol('roves')");
    assert.ok(!symbol.ok && /JSON-serializable/.test(symbol.text), symbol.text);
    // The page's CSP (script-src 'self') still blocks eval inside an evaluation.
    assert.equal((await evaluate(
      "(() => { try { eval('1'); return 'allowed'; } catch (e) { return e.name; } })()")).value, "EvalError");
    // Nothing of the bridge is visible to the page except the boolean marker (and the host
    // API marker every game-mode launch defines).
    assert.deepEqual((await evaluate(
      "Object.getOwnPropertyNames(globalThis).filter((k) => /roves|mcp|cef/i.test(k) && " +
      "!['rovesMcpFixture', 'rovesMcpEntry', 'rovesHostEntry'].includes(k)).sort()")).value,
      ["__ROVES_MCP__", "__ROVES__"]);
    log("evaluation checks passed");

    assert.equal((await client.value("click", { x: 100, y: 60 })).forwarded, 2);
    const clicks = await until("DOM click", 10_000, async () => {
      const value = (await evaluate("rovesMcpFixture.clicks")).value;
      return { done: value.length > 0, value };
    });
    assert.equal(clicks.length, 1);
    assert.equal(clicks[0].trusted, true);
    assert.equal(clicks[0].button, 0);
    const outside = await client.tool("click", { x: 100000, y: 10 });
    assert.ok(!outside.ok && /outside/.test(outside.text), outside.text);
    log(`click delivered ${JSON.stringify(clicks[0])}`);

    assert.equal((await client.value("press_key", { key: "ArrowRight" })).forwarded, 2);
    const keys = await until("DOM keydown/keyup", 10_000, async () => {
      const value = (await evaluate("rovesMcpFixture.keys")).value;
      return { done: value.includes("keyup:ArrowRight:ArrowRight:true"), value };
    });
    assert.deepEqual(keys, ["keydown:ArrowRight:ArrowRight:true", "keyup:ArrowRight:ArrowRight:true"]);
    const badKey = await client.tool("press_key", { key: "F13" });
    assert.ok(!badKey.ok, "an unlisted key was accepted");
    log("key delivered");

    assert.ok((await client.value("click", { x: 100, y: 180 })).forwarded >= 1);
    assert.ok((await client.value("type_text", { text: "roves" })).forwarded >= 1);
    const typed = await until("typed text", 10_000, async () => {
      const value = (await evaluate("({ typed: rovesMcpFixture.typed, input: document.getElementById('text-target').value })")).value;
      return { done: value.typed === "roves", value };
    });
    log(`text delivered ${JSON.stringify(typed)}`);
    const control = await client.tool("type_text", { text: "a\nb" });
    assert.ok(!control.ok, "control characters were accepted by type_text");

    const after = await client.value("get_state");
    assert.ok(after.input.forwarded >= 7, JSON.stringify(after.input));
    assert.equal(after.input.rejected, 0);
    assert.ok(after.console.recent.some(({ message }) => message === "roves-mcp-fixture ready"),
      JSON.stringify(after.console.recent));
    await client.stop();
    client = undefined;

    const code = await waitExit(game, 90_000);
    log(`game exit code=${code}`);
    assert.equal(code, 0);
    assert.ok(!existsSync(recordPath(game.pid)), "session record left behind");
    const text = checkLog(gameLog, record, "Switch");
    assert.ok(!/ FAIL /.test(text), "FAIL line in the game log");
    assert.match(text, /page markers registered identifier=\S+ mcp=true/);
    assert.match(text, /message="roves-host-entry transport=http api=\/__roves\/ frozen=true"/);
    assert.match(text, /game navigation url=http:\/\/127\.0\.0\.1:\d+\//);
    assert.match(text, /message="roves-mcp-entry enabled=true"/);
    assert.match(text, /message="roves-mcp-frame enabled=false"/);
    const late = await new Client(["--pid", String(game.pid)]).start();
    const gone = await late.tool("get_state");
    await late.stop();
    assert.ok(!gone.ok && /not running/.test(gone.text), gone.text);
    log("fixture smoke passed");
  } finally {
    if (client) await client.stop();
    if (game.exitCode === null) game.kill();
  }
}

async function packaged([pkg, out]) {
  assert.ok(pkg && out, usage);
  const marker = join(resolve(pkg), "mcp", "enabled");
  assert.ok(existsSync(marker), `missing package marker ${marker}`);
  const gameLog = join(resolve(out), "game.log");
  const game = launch(join(resolve(pkg), "play.exe"), [
    "--roves-isolated-profile", "--roves-no-error-dialog", "--roves-timeout-ms=60000",
    "--roves-auto-close-ms=5000", `--roves-log=${gameLog}`,
  ], resolve(out));
  let client;
  try {
    // Discovery without a PID: the server finds the only running game.
    client = await new Client().start();
    const state = await until("packaged game loaded", 60_000, async () => {
      const value = await client.value("get_state");
      return { done: value.loaded, value };
    });
    assert.equal(state.pid, game.pid);
    assert.equal(state.launch, "package");
    const record = JSON.parse(readFileSync(recordPath(game.pid), "utf8"));
    checkRecord(record, game.pid);
    assert.equal(state.origin, record.origin);
    log(`state ${JSON.stringify({ ...state, console: undefined })}`);
    const page = await client.value("evaluate_script", {
      expression: "({ origin: location.origin, title: document.title, readyState: document.readyState })",
    });
    assert.equal(page.origin, record.origin);
    log(`page ${JSON.stringify(page)}`);
    const now = await client.value("get_state");
    const click = await client.tool("click", { x: 10, y: 10 });
    if (now.input_ready) {
      assert.ok(click.ok, click.text);
      log("startup released: click forwarded");
    } else {
      assert.ok(!click.ok && /not ready/.test(click.text), click.text);
      log(`startup=${now.startup}: click refused as not ready (expected without accelerated frames)`);
    }
    await client.stop();
    client = undefined;
    const code = await waitExit(game, 120_000);
    const released = /startup released /.test(readFileSync(gameLog, "utf8"));
    log(`game exit code=${code} released=${released}`);
    assert.equal(code, released ? 0 : 2);
    assert.ok(!existsSync(recordPath(game.pid)), "session record left behind");
    checkLog(gameLog, record, "Package");
    log("package smoke passed");
  } finally {
    if (client) await client.stop();
    if (game.exitCode === null) game.kill();
  }
}

/**
 * Bridge off (no switch, plain package) and bridge unavailable (switch, but no session
 * directory): the game loads directly and the marker is absent everywhere.
 */
async function disabled([exe, fixtureDir, out]) {
  assert.ok(exe && fixtureDir && out, usage);
  const marker = join(dirname(resolve(exe)), "mcp", "enabled");
  assert.ok(!existsSync(marker), `${marker} exists: use the plain package's play.exe`);
  const blocker = join(resolve(out), "localappdata-is-a-file");
  mkdirSync(resolve(out), { recursive: true });
  writeFileSync(blocker, "not a directory\n");
  const cases = [
    { name: "off", args: [], env: process.env },
    { name: "unavailable", args: ["--roves-mcp"], env: { ...process.env, LOCALAPPDATA: blocker } },
  ];
  for (const { name, args, env } of cases) {
    const dir = join(resolve(out), name);
    const gameLog = join(dir, "game.log");
    const game = launch(resolve(exe), [
      `--roves-game-dir=${resolve(fixtureDir)}`, ...args, "--roves-auto-close-ms=8000",
      "--roves-timeout-ms=60000", `--roves-log=${gameLog}`,
      `--roves-cache-dir=${join(dir, "cache")}`, `--log-file=${join(dir, "cef-debug.log")}`,
    ], dir, env);
    try {
      const code = await waitExit(game, 90_000);
      const text = readFileSync(gameLog, "utf8");
      log(`${name}: game exit code=${code}`);
      assert.equal(code, 0);
      assert.ok(!/ FAIL /.test(text), `${name}: FAIL line in the game log`);
      assert.ok(!existsSync(recordPath(game.pid)), `${name}: session record published`);
      assert.match(text, /load-end status=200 url=http:\/\/127\.0\.0\.1:\d+\/ ok=true/);
      assert.match(text, /message="roves-mcp-entry enabled=false"/);
      assert.match(text, /message="roves-mcp-frame enabled=false"/);
      // The host API marker (`isAvailable()`) does not depend on the bridge.
      assert.match(text, /page markers registered identifier=\S+ mcp=false/);
      assert.match(text, /message="roves-host-entry transport=http api=\/__roves\/ frozen=true"/);
      // Host lines only (`roves-cef-nr <event>`): the fixture's own title, "roves mcp
      // fixture", is logged as `roves-cef-nr title roves mcp fixture` and is not a bridge line.
      assert.ok(!/^roves-cef-nr mcp (enabled|page capability|bootstrap)/m.test(text),
        `${name}: bridge lines in the game log`);
      const bridgeLines = text.split(/\r?\n/).filter((line) => line.startsWith("roves-cef-nr mcp "));
      if (name === "off") {
        assert.deepEqual(bridgeLines, [], "off: mcp line in the game log");
      } else {
        assert.equal(bridgeLines.length, 1, `unavailable: ${JSON.stringify(bridgeLines)}`);
        assert.match(bridgeLines[0], /^roves-cef-nr mcp unavailable source=Switch error=/);
      }
      log(`${name}: MCP marker absent, host marker present`);
    } finally {
      if (game.exitCode === null) game.kill();
    }
  }
  log("disabled smoke passed");
}

if (mode === "list-only") {
  const client = await new Client().start();
  const missing = await client.tool("get_state");
  await client.stop();
  assert.ok(!missing.ok, "get_state succeeded without a running game");
  log(`tools ${TOOLS.join(",")}; without a game: ${missing.text}`);
} else if (mode === "fixture") {
  await fixture(rest);
} else if (mode === "package") {
  await packaged(rest);
} else if (mode === "disabled") {
  await disabled(rest);
} else {
  throw new Error(usage);
}
