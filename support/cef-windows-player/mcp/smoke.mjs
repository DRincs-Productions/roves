// Smoke test of the Roves local MCP bridge against a real game process (Windows, macOS,
// Linux).
//
//   node smoke.mjs list-only <server.mjs>
//   node smoke.mjs fixture <server.mjs> <game executable> <fixture dir> <output dir>
//   node smoke.mjs package <server.mjs> <package> <output dir>
//   node smoke.mjs disabled <server.mjs> <plain game executable> <fixture dir> <output dir>
//
// Players: Windows `play.exe` (game dir by switch, log by --roves-log); macOS
// `<app>.app/Contents/MacOS/<name>` and Linux `<dir>/roves-cef-linux-gate`, which play only
// their packaged game (Contents/Resources/game, game/ next to the executable), take their
// test settings from ROVES_GATE_* variables and log `gate:` lines on stdout (captured here as
// game.log; on Linux run this script under xvfb-run). On macOS/Linux, fixture and disabled
// need <fixture dir> already copied as the package's game (checked here); <package> is the
// `.app` on macOS and the package folder elsewhere.
//
// fixture: the fixture with --roves-mcp; discovery by PID, authentication refusal,
//          state, the isMcpEnabled() marker (true and read-only in the entry script of the
//          top-level document, absent in a same-origin child frame, no other bridge global),
//          evaluation (values, objects by value, errors, Promise refusal distinct
//          from a plain {}, non-JSON symbol refused, page CSP kept), click, key and
//          text delivered to the page, the page probes (page_snapshot, inspect_element,
//          pixi_scene on the fixture's duck-typed stage), screenshot and watch (PNG size
//          matching the reported geometry; required once the player imported a frame,
//          otherwise only a clean refusal), clean exit and session removal.
// disabled: the plain package's player (no mcp/enabled) on the fixture, twice: without
//          --roves-mcp, and with --roves-mcp while the bridge cannot start (its session
//          directory cannot be created: LOCALAPPDATA or XDG_RUNTIME_DIR points at a file, on
//          macOS a file stands at the directory's path). Both must load the game with the MCP
//          marker absent in the entry script and the child frame (the host API marker
//          __ROVES__ present), publish no session record and exit cleanly.
// package: the package's player launched directly (the package marker turns MCP on);
//          discovery without a PID, state and evaluation on the packaged game, the
//          sample game's Pixi scene through __PIXI_DEVTOOLS__, page_snapshot, a screenshot
//          (required once the player imported a frame); interaction
//          is required to succeed only if the game accepts input yet, otherwise to be refused
//          as not ready; clean exit and session removal.
//
// Exit codes: 0, or on macOS/Linux 2 when the runner produced no frame (the player logs
// NO_ACCELERATED_FRAME / NO_FRAME; the bridge checks still ran). On Windows, package mode
// expects 2 when the startup screen was never released (WARP), as play.exe reports it.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import {
  closeSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmdirSync, rmSync, statSync,
  writeFileSync,
} from "node:fs";
import { createConnection } from "node:net";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";

const [mode, serverPath, ...rest] = process.argv.slice(2);
const usage = "Usage: node smoke.mjs list-only <server.mjs> | fixture <server.mjs> <game executable> " +
  "<fixture dir> <output dir> | package <server.mjs> <package> <output dir> | " +
  "disabled <server.mjs> <plain game executable> <fixture dir> <output dir>";
const WINDOWS = process.platform === "win32";
/** Prefix of the player's own log lines (not page console text). */
const HOST = WINDOWS ? "roves-cef-win " : "gate: ";
const MARKER = "__ROVES_MCP__";
assert.ok(serverPath, usage);
const TOOLS = ["click", "evaluate_script", "get_state", "inspect_element", "page_snapshot", "pixi_scene",
  "press_key", "screenshot", "type_text", "watch"];
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

  /** The raw tool result ({ content, isError }) or { error } for a protocol-level refusal. */
  async raw(name, args = {}) {
    const reply = await this.rpc("tools/call", { name, arguments: args });
    return reply.error ? { error: reply.error.message } : reply.result;
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

/** The per-user session directory (server.mjs and mcp_rules.rs `record_dir`). */
function sessionDirectory(env = process.env) {
  const absolute = (name) => (env[name] && isAbsolute(env[name]) ? env[name] : undefined);
  if (WINDOWS) {
    assert.ok(env.LOCALAPPDATA, "LOCALAPPDATA is not set");
    return join(env.LOCALAPPDATA, "Roves", "mcp");
  }
  if (process.platform === "darwin") {
    assert.ok(absolute("HOME"), "HOME is not an absolute path");
    return join(env.HOME, "Library", "Application Support", "Roves", "mcp");
  }
  if (absolute("XDG_RUNTIME_DIR")) return join(env.XDG_RUNTIME_DIR, "roves", "mcp");
  assert.ok(absolute("HOME"), "neither XDG_RUNTIME_DIR nor HOME is an absolute path");
  return join(env.HOME, ".local", "state", "roves", "mcp");
}

function recordPath(pid) {
  return join(sessionDirectory(), `${pid}.json`);
}

/**
 * Layout of a package from its player executable: { exe, marker, game }. Windows and Linux:
 * everything next to the executable; macOS: `<app>/Contents/{MacOS/<name>,Resources/...}`.
 */
function layout(exe) {
  exe = resolve(exe);
  const resources = process.platform === "darwin" ? join(dirname(dirname(exe)), "Resources") : dirname(exe);
  return { exe, marker: join(resources, "mcp", "enabled"), game: join(resources, "game") };
}

/** The player executable of a package (`.app` on macOS, folder elsewhere). */
function packageExe(pkg) {
  pkg = resolve(pkg);
  if (WINDOWS) return join(pkg, "play.exe");
  if (process.platform === "darwin") return join(pkg, "Contents", "MacOS", basename(pkg, ".app"));
  return join(pkg, "roves-cef-linux-gate");
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
  if (WINDOWS) {
    const prefix = String.raw`\\.\pipe\roves-mcp-` + pid + "-";
    assert.ok(record.pipe.startsWith(prefix) && /^[0-9a-f]{32}$/.test(record.pipe.slice(prefix.length)),
      `unexpected pipe name ${record.pipe}`);
  } else {
    // The socket next to the record, both private to this user (0600 in a 0700 directory).
    assert.equal(record.pipe, join(sessionDirectory(), `${pid}.sock`));
    const mode = (path) => statSync(path).mode & 0o777;
    assert.equal(mode(sessionDirectory()), 0o700, "session directory mode");
    assert.equal(mode(recordPath(pid)), 0o600, "session record mode");
    assert.equal(mode(record.pipe), 0o600, "socket mode");
  }
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

/**
 * Starts a player. Windows: every setting is a switch and the log is --roves-log. macOS/Linux:
 * an isolated profile, test settings in ROVES_GATE_* and stdout/stderr captured as the log.
 * `game` (Windows only): the game directory; `mcp`: add --roves-mcp.
 */
function launch(exe, { out, gameLog, game, mcp = false, autoCloseMs, timeoutMs, env = process.env }) {
  mkdirSync(out, { recursive: true });
  let args;
  let stdio = "ignore";
  let fd;
  if (WINDOWS) {
    args = [
      ...(game ? [`--roves-game-dir=${resolve(game)}`] : ["--roves-isolated-profile", "--roves-no-error-dialog"]),
      ...(mcp ? ["--roves-mcp"] : []), `--roves-auto-close-ms=${autoCloseMs}`,
      `--roves-timeout-ms=${timeoutMs}`, `--roves-log=${gameLog}`,
      ...(game ? [`--roves-cache-dir=${join(out, "cache")}`, `--log-file=${join(out, "cef-debug.log")}`] : []),
    ];
  } else {
    args = ["--roves-isolated-profile", ...(mcp ? ["--roves-mcp"] : [])];
    env = {
      ...env,
      ROVES_GATE_TIMEOUT_SECS: String(Math.ceil(timeoutMs / 1000)),
      ROVES_GATE_AUTO_CLOSE_MS: String(autoCloseMs),
    };
    fd = openSync(gameLog, "w");
    stdio = ["ignore", fd, fd];
  }
  const child = spawn(exe, args, { cwd: out, stdio, windowsHide: false, env });
  if (fd !== undefined) closeSync(fd);
  child.on("error", (error) => log(`launch error ${error.message}`));
  log(`launched pid=${child.pid} ${exe}`);
  return child;
}

/**
 * The exit code a clean run must end with: 0; on macOS/Linux 2 when the runner gave no
 * frame (the player says NO_ACCELERATED_FRAME or NO_FRAME).
 */
function cleanExit(text) {
  return !WINDOWS && /^gate: NO_(ACCELERATED_)?FRAME/m.test(text) ? 2 : 0;
}

/** macOS/Linux: the package's game must already be the fixture (the workflow copies it). */
function checkFixtureGame(exe, fixtureDir) {
  if (WINDOWS) return;
  const game = layout(exe).game;
  for (const file of readdirSync(resolve(fixtureDir))) {
    assert.deepEqual(readFileSync(join(game, file)), readFileSync(join(resolve(fixtureDir), file)),
      `${join(game, file)} is not the fixture's ${file}: copy ${fixtureDir} as the package's game`);
  }
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
  checkFixtureGame(exe, fixtureDir);
  const gameLog = join(resolve(out), "game.log");
  const game = launch(resolve(exe), {
    out: resolve(out), gameLog, game: fixtureDir, mcp: true, autoCloseMs: 45_000, timeoutMs: 90_000,
  });
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
    assert.equal(state.launch, WINDOWS ? "game-dir" : "package");
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
      // Linux commits text through CEF's IME path: the field gets the text and `input` fires,
      // but there is no `keypress` (the former CUSTOMIZATIONS.md (roves 791eff000a6), 2026-11-26).
      const keypress = process.platform === "linux" || value.typed === "roves";
      return { done: value.input === "roves" && keypress, value };
    });
    log(`text delivered ${JSON.stringify(typed)}`);
    const control = await client.tool("type_text", { text: "a\nb" });
    assert.ok(!control.ok, "control characters were accepted by type_text");

    // Page probes (one evaluate each).
    const snapshot = await client.value("page_snapshot");
    const button = snapshot.controls.find(({ id }) => id === "click-target");
    assert.ok(button, JSON.stringify(snapshot.controls));
    assert.deepEqual([button.tag, button.x, button.y, button.w, button.h], ["button", 100, 60, 200, 120]);
    assert.match(snapshot.text, /click target/);
    const inspected = await client.value("inspect_element", { selector: "#click-target", styles: ["width", "position"] });
    assert.deepEqual(inspected.computed, { width: "200px", position: "fixed" });
    assert.ok(inspected.rules.some(({ selector, source }) => selector === "#click-target" && source === "<style>"),
      JSON.stringify(inspected.rules));
    assert.equal((await client.value("inspect_element", { x: 20, y: 170 })).element, "input#text-target");
    const scene = await client.value("pixi_scene");
    assert.equal(scene.source, "__PIXI_STAGE__");
    assert.deepEqual(scene.nodes.map(({ path, label }) => [path, label ?? null]),
      [["/", "fixture-stage"], ["0", "fixture-sprite"]]);
    assert.deepEqual((await client.value("pixi_scene", { at: { x: 15, y: 25 } })).nodes.map(({ path }) => path), ["0", "/"]);
    assert.deepEqual((await client.value("pixi_scene", { find: "hidden" })).nodes.map(({ path }) => path), ["1"]);
    log("page probe checks passed");

    // Screenshots: Page.captureScreenshot needs a compositor frame.
    const pngSize = (data) => {
      const png = Buffer.from(data, "base64");
      assert.equal(png.subarray(1, 4).toString(), "PNG", "not a PNG");
      return [png.readUInt32BE(16), png.readUInt32BE(20)];
    };
    // Chromium rounds the scaled clip: the reported geometry may differ by one pixel.
    const near = (actual, reported) => {
      assert.ok(actual.every((v, i) => Math.abs(v - reported[i]) <= 1), `${actual} vs ${reported}`);
    };
    const framed = (await client.value("get_state")).frames?.imported > 0;
    const shot = await client.raw("screenshot", { format: "png", max_width: 100 });
    if (shot.isError || shot.error) {
      const why = shot.error ?? shot.content?.[0]?.text;
      assert.ok(!framed, `screenshot failed although the player imported frames: ${why}`);
      assert.match(why, /^screenshot: /);
      log(`screenshot refused without a frame: ${why}`);
    } else {
      const [image, text] = shot.content;
      assert.equal(image.type, "image");
      assert.equal(image.mimeType, "image/png");
      const meta = JSON.parse(text.text);
      near(pngSize(image.data), meta.size);
      assert.ok(meta.size[0] <= 100 && meta.size[0] >= 99, JSON.stringify(meta));
      const region = await client.raw("screenshot", { format: "png", max_width: 4096,
        region: { x: 0, y: 0, width: 200, height: 120 } });
      const regionMeta = JSON.parse(region.content[1].text);
      near(pngSize(region.content[0].data), regionMeta.size);
      assert.deepEqual(regionMeta.region, { x: 0, y: 0, width: 200, height: 120 });
      const watched = await client.raw("watch", { count: 3, interval_ms: 50, format: "png", max_width: 64 });
      const frames = JSON.parse(watched.content.at(-1).text).frames;
      assert.equal(frames.length, 3);
      assert.equal(watched.content.filter(({ type }) => type === "image").length,
        frames.filter(({ image }) => image !== undefined).length);
      const jpeg = await client.raw("screenshot", { max_width: 320 });
      assert.equal(jpeg.content[0].mimeType, "image/jpeg");
      log(`screenshot checks passed ${JSON.stringify({ full: meta.size, region: regionMeta.size, frames })}`);
    }
    const outsideShot = await client.tool("screenshot", { region: { x: 100000, y: 0, width: 10, height: 10 } });
    assert.ok(!outsideShot.ok && /outside/.test(outsideShot.text), outsideShot.text);

    const after = await client.value("get_state");
    assert.ok(after.input.forwarded >= 7, JSON.stringify(after.input));
    assert.equal(after.input.rejected, 0);
    assert.ok(after.console.recent.some(({ message }) => message === "roves-mcp-fixture ready"),
      JSON.stringify(after.console.recent));
    await client.stop();
    client = undefined;

    const code = await waitExit(game, 150_000);
    log(`game exit code=${code}`);
    assert.equal(code, cleanExit(readFileSync(gameLog, "utf8")));
    assert.ok(!existsSync(recordPath(game.pid)), "session record left behind");
    if (!WINDOWS) assert.ok(!existsSync(record.pipe), "socket left behind");
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
  const { exe, marker } = layout(packageExe(pkg));
  assert.ok(existsSync(marker), `missing package marker ${marker}`);
  const gameLog = join(resolve(out), "game.log");
  const game = launch(exe, { out: resolve(out), gameLog, autoCloseMs: 15_000, timeoutMs: 60_000 });
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
    // The packaged sample game (visual-novel-template) is a Pixi'VN game, and Pixi'VN's
    // Game.init always calls @pixi/devtools initDevtools({ app }): pixi_scene must find the
    // scene through __PIXI_DEVTOOLS__, as the PixiJS devtools extension would.
    const scene = await until("Pixi scene", 8_000, async () => {
      const value = await client.value("pixi_scene", { depth: 2, max_nodes: 40 });
      return { done: value.detected, value };
    });
    assert.equal(scene.source, "__PIXI_DEVTOOLS__");
    assert.ok(scene.nodes.length > 0 && scene.renderer, JSON.stringify(scene));
    log(`pixi scene ${JSON.stringify({ version: scene.version, renderer: scene.renderer, nodes: scene.nodes.length })}`);
    const snapshot = await client.value("page_snapshot", { max_items: 20, max_text: 200 });
    assert.ok(snapshot.canvases.length > 0, JSON.stringify(snapshot));
    log(`page snapshot controls=${snapshot.controls.length} canvases=${snapshot.canvases.length}`);
    const shot = await client.raw("screenshot", { max_width: 320 });
    const shotOk = !shot.error && !shot.isError && shot.content?.[0]?.type === "image";
    const shotText = shot.error ?? shot.content?.at(-1)?.text;
    if ((await client.value("get_state")).frames?.imported > 0) assert.ok(shotOk, shotText);
    log(`screenshot ok=${shotOk} ${shotText}`);
    // The game's own testing bridge (pixi-vn `Game.testing`, `window.pixiVN`) through MCP,
    // last so it cannot delay the checks above: when the game exposes it, `getState()` must
    // answer; a game without it, or an evaluation that does not answer in time, is reported.
    const pixiVN = await Promise.race([
      client.value("evaluate_script", {
        expression: "typeof window.pixiVN === 'object' && window.pixiVN !== null " +
          "? { present: true, state: window.pixiVN.getState() } : { present: false }",
      }),
      delay(4_000).then(() => ({ present: false, timeout: true })),
    ]).catch((error) => ({ present: false, error: error.message }));
    if (pixiVN.present) {
      assert.ok(pixiVN.state && typeof pixiVN.state === "object", "pixiVN.getState() returned no object");
      console.log(`::notice::pixiVN getState keys=${Object.keys(pixiVN.state).sort().join(",")}`);
    } else {
      console.log(`::notice::pixiVN not available through MCP ${JSON.stringify(pixiVN)}`);
    }
    await client.stop();
    client = undefined;
    const code = await waitExit(game, 120_000);
    const text = readFileSync(gameLog, "utf8");
    if (WINDOWS) {
      const released = /startup released /.test(text);
      log(`game exit code=${code} released=${released}`);
      assert.equal(code, released ? 0 : 2);
    } else {
      log(`game exit code=${code}`);
      assert.equal(code, cleanExit(text));
      assert.ok(!existsSync(record.pipe), "socket left behind");
    }
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
  const { marker } = layout(exe);
  assert.ok(!existsSync(marker), `${marker} exists: use the plain package's player`);
  checkFixtureGame(exe, fixtureDir);
  const blocker = join(resolve(out), "session-dir-is-a-file");
  mkdirSync(resolve(out), { recursive: true });
  writeFileSync(blocker, "not a directory\n");
  // Unavailable: the session directory cannot be created. Windows/Linux: its base variable
  // points at a file. macOS (HOME must stay usable): a file stands at the directory's own
  // path, only when nothing is there yet or it is an empty directory, and is removed after.
  let unavailable;
  let cleanup = () => {};
  if (WINDOWS) {
    unavailable = { ...process.env, LOCALAPPDATA: blocker };
  } else if (process.platform === "linux") {
    unavailable = { ...process.env, XDG_RUNTIME_DIR: blocker };
  } else {
    const dir = sessionDirectory();
    if (existsSync(dir) && statSync(dir).isDirectory() && readdirSync(dir).length === 0) rmdirSync(dir);
    if (!existsSync(dir)) {
      mkdirSync(dirname(dir), { recursive: true });
      writeFileSync(dir, "not a directory\n");
      cleanup = () => rmSync(dir, { force: true });
      unavailable = process.env;
    } else {
      console.log(`::notice::${dir} holds other sessions: the unavailable case is skipped`);
    }
  }
  const cases = [{ name: "off", mcp: false, env: process.env }];
  if (unavailable) cases.push({ name: "unavailable", mcp: true, env: unavailable });
  try {
    for (const { name, mcp, env } of cases) {
      const dir = join(resolve(out), name);
      const gameLog = join(dir, "game.log");
      const game = launch(resolve(exe), {
        out: dir, gameLog, game: fixtureDir, mcp, autoCloseMs: 8_000, timeoutMs: 60_000, env,
      });
      try {
        const code = await waitExit(game, 90_000);
        const text = readFileSync(gameLog, "utf8");
        log(`${name}: game exit code=${code}`);
        assert.equal(code, cleanExit(text));
        assert.ok(!/ FAIL /.test(text), `${name}: FAIL line in the game log`);
        assert.ok(!existsSync(recordPath(game.pid)), `${name}: session record published`);
        assert.match(text, /load-end status=200 url=http:\/\/127\.0\.0\.1:\d+\/ ok=true/);
        assert.match(text, /message="roves-mcp-entry enabled=false"/);
        assert.match(text, /message="roves-mcp-frame enabled=false"/);
        // The host API marker (`isAvailable()`) does not depend on the bridge.
        assert.match(text, /page markers registered identifier=\S+ mcp=false/);
        assert.match(text, /message="roves-host-entry transport=http api=\/__roves\/ frozen=true"/);
        // Host lines only (`<host prefix>mcp ...`): the fixture's own title, "roves mcp
        // fixture", is logged as a title line and is not a bridge line.
        const bridgeLines = text.split(/\r?\n/).filter((line) => line.startsWith(`${HOST}mcp `));
        if (name === "off") {
          assert.deepEqual(bridgeLines, [], "off: mcp line in the game log");
        } else {
          assert.equal(bridgeLines.length, 1, `unavailable: ${JSON.stringify(bridgeLines)}`);
          assert.ok(bridgeLines[0].startsWith(`${HOST}mcp unavailable source=Switch error=`), bridgeLines[0]);
        }
        log(`${name}: MCP marker absent, host marker present`);
      } finally {
        if (game.exitCode === null) game.kill();
      }
    }
  } finally {
    cleanup();
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
