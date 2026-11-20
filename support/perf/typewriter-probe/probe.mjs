// Cross-engine typewriter probe for visual-novel-template (see README.md).
// Node >= 20; built-ins only, except the Roves leg, which drives the existing
// dev-mcp server (support/blitz-v8-probe/dev-mcp, after `npm ci` there).
//
//   node probe.mjs roves <dev-mcp probe.exe> <dist> --out <roves.txt> [--min-visible 8] [--server <server.mjs>]
//   node probe.mjs serve <dist> [--port 0]
//   node probe.mjs compare <roves.txt> <chrome.txt>
//
// `roves` runs observer.js in the game through the dev-mcp evaluate_script
// tool and writes the `ROVES_TYPEWRITER_RESULT {...}` line. `serve` serves the
// same dist read-only on 127.0.0.1 for the Chrome DevTools (MCP) leg.
// `compare` prints both records side by side and checks the shared contract.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createReadStream, existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, resolve, sep } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const PREFIX = "ROVES_TYPEWRITER_RESULT ";
const OBSERVER = readFileSync(new URL("./observer.js", import.meta.url), "utf8").trim();
// The dev-mcp evaluate_script tool rejects sources over 16 KiB. The Roves leg
// sends observer.js without full-line comments, blank lines and indentation;
// newlines are kept, so automatic semicolon insertion is unchanged. This is
// only safe while observer.js has no trailing or block comments and no
// multi-line strings, which the assertion below enforces. Chrome evaluates the
// readable file itself through `sourcePath`.
const EVALUATE_LIMIT = 16 * 1024;
const OBSERVER_CALL = "(" + OBSERVER.split(/\r?\n/).map((line) => line.trim())
  .filter((line) => line !== "" && !line.startsWith("//")).join("\n") + ")()";
const [mode, ...rest] = process.argv.slice(2);

function option(name, fallback) {
  const at = rest.indexOf(name);
  if (at < 0) return fallback;
  const value = rest[at + 1];
  assert.ok(value !== undefined, `${name} needs a value`);
  rest.splice(at, 2);
  return value;
}

// Finds the result line in a raw log, in a file holding the line, or in a
// JSON string literal written by a DevTools evaluate_script `filePath`.
function loadResult(file) {
  const text = readFileSync(file, "utf8");
  const quoted = text.match(/"ROVES_TYPEWRITER_RESULT (?:[^"\\]|\\.)*"/);
  const line = quoted ? JSON.parse(quoted[0]) : text.split(/\r?\n/).find((l) => l.includes(PREFIX));
  assert.ok(line, `${file}: no ${PREFIX.trim()} line`);
  return JSON.parse(line.slice(line.indexOf(PREFIX) + PREFIX.length));
}

async function roves() {
  const minVisible = Number(option("--min-visible", "8"));
  const out = option("--out");
  const serverPath = option("--server",
    fileURLToPath(new URL("../../blitz-v8-probe/dev-mcp/server.mjs", import.meta.url)));
  const [probe, dist] = rest;
  assert.ok(probe && dist && out, "Usage: node probe.mjs roves <probe.exe> <dist> --out <file>");
  assert.ok(!/\/\/|\/\*/.test(OBSERVER_CALL) &&
    OBSERVER_CALL.split("\n").every((line) => (line.match(/`/g) ?? []).length % 2 === 0),
    "observer.js: trailing/block comment or multi-line template literal; keep comments on their own lines");
  const observerBytes = Buffer.byteLength(OBSERVER_CALL);
  assert.ok(observerBytes <= EVALUATE_LIMIT,
    `compacted observer is ${observerBytes} bytes, over the ${EVALUATE_LIMIT}-byte evaluate_script limit`);
  const server = spawn(process.execPath, [serverPath, resolve(probe), resolve(dist)],
    { stdio: ["pipe", "pipe", "inherit"], windowsHide: true });
  const closed = once(server, "close");
  const lines = createInterface({ input: server.stdout })[Symbol.asyncIterator]();
  const deadline = setTimeout(() => server.kill(), 420_000);
  let nextId = 0;
  const send = (message) => server.stdin.write(JSON.stringify({ jsonrpc: "2.0", ...message }) + "\n");
  async function rpc(method, params) {
    const id = ++nextId;
    send({ id, method, params });
    for (;;) {
      const { done, value } = await lines.next();
      if (done) throw new Error(`MCP server closed while waiting for ${method}`);
      const reply = JSON.parse(value);
      if (reply.id !== id) continue;
      if (reply.error) throw new Error(`${method}: ${JSON.stringify(reply.error).slice(0, 300)}`);
      return reply.result;
    }
  }
  async function evaluate(source) {
    const reply = await rpc("tools/call", { name: "evaluate_script", arguments: { source } });
    const text = reply.content?.[0]?.text ?? "";
    if (reply.isError) throw new Error("evaluate_script: " + text.slice(0, 500));
    return JSON.parse(text);
  }
  async function poll(what, source, accept, timeoutMs) {
    const until = Date.now() + timeoutMs;
    for (;;) {
      const last = await evaluate(source);
      if (accept(last)) return last;
      if (Date.now() >= until) throw new Error(`${what} not reached; last ${JSON.stringify(last).slice(0, 400)}`);
      await delay(200);
    }
  }
  let record = null;
  let failure = null;
  let ready = null;
  try {
    await rpc("initialize", { protocolVersion: "2025-11-25", capabilities: {},
      clientInfo: { name: "roves-typewriter-probe", version: "0.0.0" } });
    send({ method: "notifications/initialized" });
    // Same as Chrome recipe step 3: evaluating the observer installs it once
    // and returns status(), whose `ready` is the playthrough's readiness.
    const status = await poll("Game.testing bridge with live props and actions", OBSERVER_CALL,
      (s) => s.ready.ready === true || (s.ready.deno !== undefined && s.ready.deno !== "undefined"), 240_000);
    ready = status.ready;
    if (ready.deno !== "undefined") throw new Error(`privileged Deno API exposed to the game (${ready.deno})`);
    await evaluate("globalThis.__rovesTw.start()");
    // A line first seen already static is finalized at once (missed_no_stagger),
    // so these waits end on every observed line, typed or not.
    await poll("first line finalized", "globalThis.__rovesTw.status()", (s) => s.finalized[0] === true, 60_000);
    await evaluate(`globalThis.__rovesTw.next(${minVisible})`);
    await poll("second line finalized", "globalThis.__rovesTw.status()", (s) => s.finalized[1] === true, 30_000);
  } catch (error) {
    failure = String(error?.message ?? error).slice(0, 800);
  }
  try {
    record = await evaluate("globalThis.__rovesTw?.report() ?? null");
  } catch (error) {
    failure ??= String(error?.message ?? error).slice(0, 800);
  }
  clearTimeout(deadline);
  server.stdin.end();
  const [code] = await Promise.race([closed, delay(180_000).then(() => ["timeout"])]);
  if (code === "timeout") server.kill();
  const header = { engine: "roves", probe: resolve(probe), dist: resolve(dist), jitless: process.env.ROVES_PROBE_JITLESS === "1",
    min_visible: minVisible, ready, failure, server_exit: code };
  writeFileSync(out, JSON.stringify(header) + "\n" + (record ?? "") + "\n");
  console.log(JSON.stringify(header));
  if (record) console.log(record);
  process.exitCode = failure || !record ? 1 : 0;
}

const MIME = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8", ".json": "application/json", ".webmanifest": "application/manifest+json",
  ".svg": "image/svg+xml", ".png": "image/png", ".jpg": "image/jpeg", ".webp": "image/webp", ".ico": "image/x-icon",
  ".wasm": "application/wasm", ".woff2": "font/woff2", ".mp3": "audio/mpeg", ".ogg": "audio/ogg", ".txt": "text/plain",
};

function serve() {
  const port = Number(option("--port", "0"));
  const root = resolve(rest[0] ?? "");
  assert.ok(rest[0] && existsSync(resolve(root, "index.html")), "Usage: node probe.mjs serve <dist> [--port N]");
  const server = createServer((req, res) => {
    let file = resolve(root, "." + decodeURIComponent(new URL(req.url, "http://x").pathname));
    if (file !== root && !file.startsWith(root + sep)) { res.statusCode = 403; res.end(); return; }
    // History API routes (/game/narration) fall back to the SPA entry.
    if (!existsSync(file) || statSync(file).isDirectory()) {
      if (extname(file)) { res.statusCode = 404; res.end(); return; }
      file = resolve(root, "index.html");
    }
    res.setHeader("content-type", MIME[extname(file)] ?? "application/octet-stream");
    res.setHeader("cache-control", "no-store");
    createReadStream(file).pipe(res);
  });
  server.listen(port, "127.0.0.1", () =>
    console.log(`serving ${root} at http://127.0.0.1:${server.address().port}/ (Ctrl+C to stop)`));
}

function compare() {
  const [a, b] = rest;
  assert.ok(a && b, "Usage: node probe.mjs compare <roves.txt> <chrome.txt>");
  const runs = { roves: loadResult(a), chrome: loadResult(b) };
  const problems = [];
  const flowAt = (run, what) => run.flow?.find((f) => f.what === what) ?? null;
  const dom = (d) => (d ? JSON.stringify(d) : "null");
  for (const [engine, run] of Object.entries(runs)) {
    if (run.schema !== 2) { problems.push(`${engine}: result schema ${run.schema}, expected 2 (re-run with this observer)`); continue; }
    const { env, lines } = run;
    console.log(`${engine}: ` + (env ? `waapi ${env.waapi}, viewport ${env.viewport}, dpr ${env.dpr}, delay ${env.delay_setting}, ` +
      `bundle ${env.bundle}, hidden_seen ${env.hidden_seen}` : "start() never called") +
      `, notes ${JSON.stringify(run.notes)}, observer ${JSON.stringify(run.observer)}`);
    // Readiness and flow: the same gates as the dev-mcp playthrough.
    if (!run.ready) problems.push(`${engine}: playthrough readiness (navigate, props.navigate/t/toast, label start) never reached`);
    else {
      if (run.ready.deno !== "undefined") problems.push(`${engine}: privileged Deno API exposed (${run.ready.deno})`);
      if (run.ready.boot_errors?.length) problems.push(`${engine}: boot errors ${JSON.stringify(run.ready.boot_errors)}`);
    }
    if (!env) { problems.push(`${engine}: no run (start never called)`); continue; }
    if (env.hidden_seen) problems.push(`${engine}: page was hidden during the run (rAF throttled): not a result`);
    if (run.notes.length) problems.push(`${engine}: observer notes ${JSON.stringify(run.notes)}`);
    for (const f of run.flow) if (/_rejected$/.test(f.what)) problems.push(`${engine}: ${f.what}: ${f.error}`);
    const started = flowAt(run, "start_fulfilled");
    if (!started) problems.push(`${engine}: Game.testing start("start") never settled`);
    else {
      if (started.game.label !== "start") problems.push(`${engine}: start opened label ${started.game.label}, expected start`);
      if (!/\/game\/narration/.test(started.game.path ?? "")) problems.push(`${engine}: start did not navigate to /game/narration (${started.game.path})`);
    }
    if (run.game_end?.error_count) problems.push(`${engine}: Game.testing captured errors ${JSON.stringify(run.game_end.errors)}`);
    for (let index = 0; index < 2; index++) {
      const line = lines[index];
      if (!line) { problems.push(`${engine}: line ${index} not observed; last DOM ${dom(run.dom_last)}`); continue; }
      if (line.outcome === "missed_no_stagger") {
        problems.push(`${engine}: line ${index} missed_no_stagger: first observed with the whole text already static, ` +
          `no per-character typing seen (DOM ${dom(line.dom_mount)}); timings not comparable, no parity claimed`);
        continue;
      }
      const expected = index === 0 ? "completed" : "cancelled";
      if (line.outcome !== expected) {
        problems.push(`${engine}: line ${index} outcome ${line.outcome}, expected ${expected}` +
          (line.outcome === "pending" ? `; static mismatch ${dom(line.static_mismatch)}; last DOM ${dom(run.dom_last)}` : ""));
      }
      if (!line.in_order) problems.push(`${engine}: line ${index} characters became visible out of order`);
      if (line.unreadable_opacity) problems.push(`${engine}: line ${index} computed opacity unreadable ${line.unreadable_opacity}x`);
    }
    if (run.cancel_arm?.not_exercised) problems.push(`${engine}: cancel not exercised: ${run.cancel_arm.not_exercised}`);
  }
  const [r, c] = [runs.roves, runs.chrome];
  if (r.schema !== 2 || c.schema !== 2 || !r.env || !c.env) {
    console.log("\nTYPEWRITER_PROBE contract problems:\n- " + problems.join("\n- "));
    process.exitCode = 1;
    return;
  }
  if (r.env.bundle !== c.env.bundle) problems.push(`different bundles: ${r.env.bundle} vs ${c.env.bundle}`);
  if (r.env.delay_setting !== c.env.delay_setting) problems.push(`different typewriter delay settings`);
  // Navigation/state differences at the same checkpoints, reported even when
  // the cancel could not be exercised.
  for (const what of ["ready", "start_fulfilled", "continue_fulfilled"]) {
    const [fr, fc] = [flowAt(r, what), flowAt(c, what)];
    if (!fr || !fc) { if (fr || fc) problems.push(`${what}: reached only in ${fr ? "roves" : "chrome"}`); continue; }
    for (const key of ["label", "step", "path", "text"]) {
      if (fr.game[key] !== fc.game[key]) {
        problems.push(`${what}: ${key} differs: roves ${JSON.stringify(fr.game[key])} vs chrome ${JSON.stringify(fc.game[key])}`);
      }
    }
  }
  const rows = [
    ["n", (l) => l.n], ["outcome", (l) => l.outcome], ["first_visible_ms", (l) => l.first_visible_ms],
    ["q25_ms", (l) => l.quartiles_ms[25]], ["q50_ms", (l) => l.quartiles_ms[50]], ["q75_ms", (l) => l.quartiles_ms[75]],
    ["q100_ms", (l) => l.quartiles_ms[100]], ["finalized_ms", (l) => l.finalized_ms],
    ["visible_before_final", (l) => l.visible_before_final], ["char_gap_p50", (l) => l.char_gap_ms.p50],
    ["char_gap_p95", (l) => l.char_gap_ms.p95], ["char_gap_max", (l) => l.char_gap_ms.max],
    ["max_progress_gap_ms", (l) => l.max_progress_gap_ms], ["frames", (l) => l.frames],
    ["max_frame_gap_ms", (l) => l.max_frame_gap_ms], ["inline_opacity_max", (l) => l.inline_opacity_max],
    ["cancel_at_ms", (l) => l.cancel?.t_ms ?? null], ["cancel_visible", (l) => l.cancel?.visible ?? null],
    ["cancel_to_final_ms", (l) => (l.cancel && l.finalized_ms != null ? l.finalized_ms - l.cancel.t_ms : null)],
  ];
  for (let index = 0; index < Math.min(r.lines.length, c.lines.length, 2); index++) {
    const [lr, lc] = [r.lines[index], c.lines[index]];
    // Normalized (whitespace-free) text: a missed line has no character spans.
    if (lr.text_norm !== lc.text_norm) problems.push(`line ${index}: different text (${lr.text_norm_len} vs ${lc.text_norm_len} glyphs)`);
    if (lr.path === "stagger" && lc.path === "stagger" && lr.n !== lc.n) problems.push(`line ${index}: different glyph spans (${lr.n} vs ${lc.n})`);
    if (lr.path !== lc.path) problems.push(`line ${index}: path differs: roves ${lr.path} vs chrome ${lc.path}`);
    for (const key of ["label", "step", "path"]) {
      if (lr.game_mount?.[key] !== lc.game_mount?.[key]) {
        problems.push(`line ${index}: game ${key} at mount differs: roves ${JSON.stringify(lr.game_mount?.[key])} vs chrome ${JSON.stringify(lc.game_mount?.[key])}`);
      }
    }
    console.log(`\nline ${index}: ${JSON.stringify(lr.text_norm.slice(0, 60))}\n${"metric".padEnd(22)}${"roves".padStart(12)}${"chrome".padStart(12)}`);
    console.log(`${"path".padEnd(22)}${lr.path.slice(0, 11).padStart(12)}${lc.path.slice(0, 11).padStart(12)}`);
    // Timings exist only for a line observed typing; a missed line prints its DOM instead.
    for (const [name, get] of rows) {
      const show = (l) => (l.path === "stagger" || name === "outcome" ? String(get(l)) : "-");
      console.log(`${name.padEnd(22)}${show(lr).padStart(12)}${show(lc).padStart(12)}`);
    }
    for (const [engine, l] of [["roves", lr], ["chrome", lc]]) {
      console.log(`${engine} DOM mount ${dom(l.dom_mount)}\n${engine} DOM final ${dom(l.dom_final)}`);
    }
  }
  console.log("\n" + (problems.length ? "TYPEWRITER_PROBE contract problems (no parity claimed):\n- " + problems.join("\n- ") :
    "TYPEWRITER_PROBE contract ok (timings are reported, not gated)"));
  process.exitCode = problems.length ? 1 : 0;
}

if (mode === "roves") await roves();
else if (mode === "serve") serve();
else if (mode === "compare") compare();
else {
  console.error("Usage: node probe.mjs roves|serve|compare ... (see README.md)");
  process.exitCode = 2;
}
