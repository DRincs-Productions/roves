#!/usr/bin/env node
// Roves vs Chrome fixture matrix runner (Windows, Node >= 20, built-ins only).
// Runs existing artifacts: a Roves Blitz probe executable (`--game <dir> --window`)
// and an installed Chrome. No build, no elevation, no engine change.
// Usage and metric definitions: README.md next to this file.
import { spawn, execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join, resolve, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const HERE = dirname(fileURLToPath(import.meta.url));
const ALL_CASES = ["blank", "static-dom", "text-incremental", "transform", "opacity", "backdrop-filter",
  "pixi-static", "pixi-animated", "pixi-text-blur"];
const DOM_CASES = ALL_CASES.filter((c) => !c.startsWith("pixi"));
const PIXI_DIR = resolve(HERE, "../../blitz-v8-probe/js/node_modules/pixi.js");
const PIXI_VERSION = "8.19.0";

function parseArgs(argv) {
  const opts = {
    engines: ["roves", "chrome"], cases: ALL_CASES, reps: 3, warmup: 120, measure: 600,
    out: null, rovesExe: null, chromeExe: "C:/Program Files/Google/Chrome/Application/chrome.exe",
    rovesBackend: "vulkan", jitless: false, chromeDsf: "1", timeoutS: 120, smoke: false, label: null,
  };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    const next = () => { if (i + 1 >= argv.length) throw new Error(`${a} needs a value`); return argv[++i]; };
    switch (a) {
      case "--engines": opts.engines = next().split(","); break;
      case "--cases": { const v = next(); opts.cases = v === "dom" ? DOM_CASES : v === "all" ? ALL_CASES : v.split(","); break; }
      case "--reps": opts.reps = Number(next()); break;
      case "--warmup-frames": opts.warmup = Number(next()); break;
      case "--measure-frames": opts.measure = Number(next()); break;
      case "--out": opts.out = resolve(next()); break;
      case "--roves-exe": opts.rovesExe = resolve(next()); break;
      case "--chrome-exe": opts.chromeExe = next(); break;
      case "--roves-backend": opts.rovesBackend = next(); break;
      case "--jitless": opts.jitless = true; break;
      case "--chrome-dsf": opts.chromeDsf = next(); break;
      case "--timeout-s": opts.timeoutS = Number(next()); break;
      case "--label": opts.label = next(); break;
      case "--smoke": opts.smoke = true; break;
      default: throw new Error(`unknown argument ${a}`);
    }
  }
  if (opts.smoke) { opts.reps = 1; opts.warmup = 10; opts.measure = 60; }
  for (const c of opts.cases) if (!ALL_CASES.includes(c)) throw new Error(`unknown case ${c}`);
  for (const e of opts.engines) if (!["roves", "chrome"].includes(e)) throw new Error(`unknown engine ${e}`);
  if (!opts.out) throw new Error("--out <dir> is required");
  if (opts.engines.includes("roves") && !opts.rovesExe) throw new Error("--roves-exe is required for engine roves");
  if (opts.engines.includes("roves") && !existsSync(opts.rovesExe)) throw new Error(`missing ${opts.rovesExe}`);
  if (opts.engines.includes("chrome") && !existsSync(opts.chromeExe)) throw new Error(`missing ${opts.chromeExe}`);
  return opts;
}

const sha256 = (file) => new Promise((ok, ko) => {
  const h = createHash("sha256");
  createReadStream(file).on("data", (d) => h.update(d)).on("end", () => ok(h.digest("hex"))).on("error", ko);
});
const log = (...m) => console.log(new Date().toISOString().slice(11, 19), ...m);
const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));

function prepareFixtures(opts) {
  const root = join(opts.out, "fixtures");
  const template = readFileSync(join(HERE, "index.html"), "utf8");
  const bench = readFileSync(join(HERE, "bench.js"));
  let pixi = null;
  if (opts.cases.some((c) => c.startsWith("pixi"))) {
    const pkg = JSON.parse(readFileSync(join(PIXI_DIR, "package.json"), "utf8"));
    if (pkg.version !== PIXI_VERSION) throw new Error(`pixi.js ${pkg.version} found, ${PIXI_VERSION} pinned`);
    pixi = join(PIXI_DIR, "dist", "pixi.min.mjs");
  }
  for (const name of opts.cases) {
    const dir = join(root, name);
    mkdirSync(dir, { recursive: true });
    if (name.startsWith("pixi")) copyFileSync(pixi, join(dir, "pixi.min.mjs"));
    writeFileSync(join(dir, "index.html"), template);
    writeFileSync(join(dir, "bench.js"), bench);
    writeFileSync(join(dir, "case.js"), `globalThis.ROVES_BENCH_CASE = ${JSON.stringify(name)};\n` +
      `globalThis.ROVES_BENCH_CONFIG = ${JSON.stringify({ warmupFrames: opts.warmup, measureFrames: opts.measure })};\n`);
  }
  return { root, pixi };
}

// --- Chrome: fixtures over loopback HTTP, result POSTed back by the page ---
const MIME = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8" };
function startServer(root) {
  let waiter = null;
  const server = createServer((req, res) => {
    if (req.method === "POST" && req.url === "/__bench_result") {
      let body = "";
      req.on("data", (d) => { body += d; });
      req.on("end", () => { res.end("ok"); if (waiter) { const w = waiter; waiter = null; try { w(JSON.parse(body)); } catch { w(null); } } });
      return;
    }
    const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    const file = resolve(root, "." + path);
    if (!file.startsWith(root) || !existsSync(file)) { res.statusCode = 404; res.end(); return; }
    res.setHeader("content-type", MIME[extname(file)] || "application/octet-stream");
    res.setHeader("cache-control", "no-store");
    createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok({
    port: server.address().port,
    next: () => new Promise((r) => { waiter = r; }),
    cancel: () => { if (waiter) { const w = waiter; waiter = null; w(null); } },
    close: () => server.close(),
  })));
}

const killTree = (pid) => new Promise((ok) => execFile("taskkill", ["/PID", String(pid), "/T", "/F"], () => ok()));
// Only the Chrome processes started with this run's own profile directory.
const killProfile = (profile) => new Promise((ok) => execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
  `Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${profile.replace(/'/g, "''")}') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`],
() => ok()));

async function runChrome(opts, server, name, runDir) {
  const profile = join(runDir, "chrome-profile");
  const url = `http://127.0.0.1:${server.port}/${name}/index.html`;
  const args = [`--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", "--disable-extensions",
    "--disable-sync", "--disable-component-update", "--disable-background-networking", "--disable-default-apps",
    // Roves never throttles an occluded window; without these Chrome reports the
    // page hidden and stops rAF whenever Windows considers the window covered.
    "--disable-features=CalculateNativeWinOcclusion", "--disable-backgrounding-occluded-windows",
    `--force-device-scale-factor=${opts.chromeDsf}`, "--window-position=40,40", "--window-size=660,420", `--app=${url}`];
  const stderr = createWriteStream(join(runDir, "chrome.stderr.log"));
  const started = Date.now();
  const pending = server.next();
  const proc = spawn(opts.chromeExe, args, { stdio: ["ignore", "ignore", "pipe"] });
  proc.stderr.pipe(stderr);
  const timer = setTimeout(() => server.cancel(), opts.timeoutS * 1000);
  const page = await pending;
  clearTimeout(timer);
  await killTree(proc.pid);
  await killProfile(profile);
  await sleep(500);
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  return { wall_ms: Date.now() - started, page, error: page ? null : "timeout: no page result" };
}

// --- Roves: probe `--game <case dir> --window`; result read from stdout ---
function closeWindow(pid) {
  return new Promise((ok) => execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command",
    `$p = Get-Process -Id ${pid} -ErrorAction SilentlyContinue; if ($p) { [void]$p.CloseMainWindow() }`], () => ok()));
}

async function runRoves(opts, root, name, runDir) {
  const env = {};
  for (const [k, v] of Object.entries(process.env)) if (!/^(ROVES_PROBE_|ROVES_EXTRA_ARGS|DENO_WEBGPU_|WGPU_)/i.test(k)) env[k] = v;
  Object.assign(env, {
    ROVES_PROBE_FRAME_TIMINGS: "1", ROVES_PROBE_STORAGE_DIR: join(runDir, "storage"),
    DENO_WEBGPU_BACKEND: opts.rovesBackend, ROVES_PROBE_JITLESS: opts.jitless ? "1" : "0",
  });
  // Upper bound only; the runner closes the window (WM_CLOSE) as soon as the page reports.
  const cap = String(opts.warmup + opts.measure + 3600);
  const args = ["--game", join(root, name), "--window", "--window-frames", cap];
  const out = createWriteStream(join(runDir, "roves.stdout.log"));
  const err = createWriteStream(join(runDir, "roves.stderr.log"));
  const started = Date.now();
  const proc = spawn(opts.rovesExe, args, { cwd: dirname(opts.rovesExe), env, stdio: ["ignore", "pipe", "pipe"] });
  proc.stderr.pipe(err);
  let page = null;
  let timings = null;
  let closing = false;
  const exited = new Promise((ok) => proc.on("exit", (code) => ok(code)));
  createInterface({ input: proc.stdout }).on("line", (line) => {
    out.write(line + "\n");
    if (line.startsWith("ROVES_BENCH_RESULT ")) {
      try { page = JSON.parse(line.slice(19)); } catch { page = { status: "unparsable" }; }
      if (!closing) { closing = true; closeWindow(proc.pid); }
    } else if (line.startsWith("ROVES_PROBE_FRAME_TIMINGS ")) {
      try { timings = JSON.parse(line.slice(26)); } catch { timings = null; }
    }
  });
  const timeout = sleep(opts.timeoutS * 1000).then(() => "timeout");
  let code = await Promise.race([exited, timeout]);
  let error = null;
  if (code === "timeout") {
    error = page ? "window did not close after result" : "timeout: no page result";
    if (!closing) { closing = true; await closeWindow(proc.pid); }
    code = await Promise.race([exited, sleep(10000).then(() => "kill")]);
    if (code === "kill") { proc.kill(); code = await exited; }
  }
  await sleep(100);
  out.end(); err.end();
  return { wall_ms: Date.now() - started, exit_code: code, page, roves_frame_timings: timings, error };
}

function csvRow(r) {
  const p = r.page || {};
  const iv = p.raf_interval_ms || {};
  const w = p.work_ms || {};
  const ft = r.roves_frame_timings || {};
  const s = (k) => (ft[k] ? ft[k].p99 : "");
  return [r.engine, r.case, r.rep, p.status ?? "missing", r.error ?? "", p.raf_fps ?? "", iv.p50 ?? "", iv.p95 ?? "", iv.p99 ?? "",
    iv.max ?? "", iv.over_25ms ?? "", iv.over_33_34ms ?? "", w.p50 ?? "", w.p99 ?? "",
    p.viewport ? `${p.viewport.inner_width}x${p.viewport.inner_height}@${p.viewport.device_pixel_ratio}` : "",
    p.pixi ? p.pixi.backend : "", p.supports ? p.supports.backdrop_filter : "",
    s("cpu_render_present"), s("cpu_shell_redraw"), s("frame_step_interval"),
    p.visibility ? `${p.visibility.at_measure_start}/${p.visibility.at_end}/${p.visibility.changes_during_measure}` : ""].join(",");
}

async function main() {
  const opts = parseArgs(process.argv);
  mkdirSync(opts.out, { recursive: true });
  const { root, pixi } = prepareFixtures(opts);
  const meta = {
    schema: "roves-perf-fixture-matrix-run", version: 1, started: new Date().toISOString(), label: opts.label,
    engines: opts.engines, cases: opts.cases, reps: opts.reps, warmup_frames: opts.warmup, measure_frames: opts.measure,
    smoke: opts.smoke, roves_backend: opts.rovesBackend, jitless: opts.jitless, chrome_dsf: opts.chromeDsf,
    roves_exe: opts.rovesExe, roves_sha256: opts.rovesExe ? await sha256(opts.rovesExe) : null,
    chrome_exe: opts.engines.includes("chrome") ? opts.chromeExe : null,
    bench_js_sha256: await sha256(join(HERE, "bench.js")), index_html_sha256: await sha256(join(HERE, "index.html")),
    pixi_version: pixi ? PIXI_VERSION : null, pixi_sha256: pixi ? await sha256(pixi) : null,
  };
  const server = opts.engines.includes("chrome") ? await startServer(root) : null;
  const runs = [];
  try {
    for (let rep = 1; rep <= opts.reps; rep++) {
      // Alternate engine order between repetitions.
      const order = rep % 2 ? opts.engines : [...opts.engines].reverse();
      for (const name of opts.cases) {
        for (const engine of order) {
          const runDir = join(opts.out, "runs", `${rep}-${name}-${engine}`);
          mkdirSync(runDir, { recursive: true });
          log(`rep ${rep} ${name} ${engine}`);
          const r = engine === "roves" ? await runRoves(opts, root, name, runDir) : await runChrome(opts, server, name, runDir);
          const run = { engine, case: name, rep, ...r };
          writeFileSync(join(runDir, "result.json"), JSON.stringify(run, null, 2));
          runs.push(run);
          const p = run.page || {};
          log(`  ${p.status ?? "missing"} ${run.error ?? ""} fps=${p.raf_fps ?? "-"} p95=${p.raf_interval_ms?.p95 ?? "-"} p99=${p.raf_interval_ms?.p99 ?? "-"}`);
          await sleep(1000);
        }
      }
    }
  } finally {
    if (server) server.close();
  }
  meta.finished = new Date().toISOString();
  writeFileSync(join(opts.out, "results.json"), JSON.stringify({ meta, runs }, null, 2));
  const header = "engine,case,rep,status,error,raf_fps,raf_p50_ms,raf_p95_ms,raf_p99_ms,raf_max_ms,raf_over_25ms,raf_over_33ms," +
    "work_p50_ms,work_p99_ms,viewport,pixi_backend,supports_backdrop_filter,roves_render_present_p99_ms,roves_shell_redraw_p99_ms,roves_step_interval_p99_ms,visibility_start_end_changes";
  writeFileSync(join(opts.out, "runs.csv"), [header, ...runs.map(csvRow)].join("\n") + "\n");
  const failed = runs.filter((r) => !r.page || r.page.status !== "ok" || r.error);
  log(`done: ${runs.length} runs, ${failed.length} failed -> ${join(opts.out, "results.json")}`);
  process.exitCode = failed.length ? 1 : 0;
}

main().catch((e) => { console.error(e.stack || e); process.exitCode = 2; });
