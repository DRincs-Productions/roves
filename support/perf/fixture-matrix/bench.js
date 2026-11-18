// Shared Roves/Chrome fixture matrix: one page, one case per run.
// The runner writes case.js (globalThis.ROVES_BENCH_CASE/ROVES_BENCH_CONFIG)
// next to this file; the bytes served to both engines are identical.
// Loaded as a module: the Roves probe's `--game` loader runs module and
// inline scripts only (external classic scripts are skipped).
//
// Every case runs the same requestAnimationFrame loop. Work is a pure function
// of the frame index (no wall clock, no Math.random), so both engines do the
// same work per frame regardless of how fast frames arrive.
//
// Case chain (each case changes exactly one thing relative to `base`):
//   blank            empty 640x360 stage, rAF loop only
//   static-dom       blank + fixed DOM (cards, glass panel without filter, text)
//   text-incremental static-dom + one text node changed every frame
//   transform        static-dom + #mover transform changed every frame
//   opacity          static-dom + #mover opacity changed every frame
//   backdrop-filter  transform + backdrop-filter: blur(8px) on the glass panel
//   pixi-static      blank + PixiJS canvas rendered once
//   pixi-animated    pixi-static + Pixi scene updated and rendered every frame
//   pixi-text-blur   pixi-animated + DOM dialogue overlay (static text, backdrop blur)
//   pixi-static-hd   pixi-static with a 1920x1080 backing (same 640x360 world and CSS box)
//   pixi-animated-hd pixi-animated with a 1920x1080 backing (same 640x360 world and CSS box)
//   pixi-text-incremental    pixi-animated + DOM dialogue growing by one <span> per frame
//   pixi-text-incremental-hd pixi-animated-hd + the same incremental DOM dialogue
(() => {
  "use strict";
  // `backing`: Pixi backing-store scale relative to the 640x360 stage. The canvas CSS box
  // stays 640x360 (#pixi rule) and app.stage is scaled by the same factor, so world
  // coordinates and scene geometry are identical to the scale-1 cases.
  const CASES = {
    "blank": { base: null, dom: false, pixi: null },
    "static-dom": { base: "blank", dom: true, pixi: null },
    "text-incremental": { base: "static-dom", dom: true, pixi: null, animate: "text" },
    "transform": { base: "static-dom", dom: true, pixi: null, animate: "transform" },
    "opacity": { base: "static-dom", dom: true, pixi: null, animate: "opacity" },
    "backdrop-filter": { base: "transform", dom: true, pixi: null, animate: "transform", blur: true },
    "pixi-static": { base: "blank", dom: false, pixi: "static" },
    "pixi-animated": { base: "pixi-static", dom: false, pixi: "animated" },
    "pixi-text-blur": { base: "pixi-animated", dom: false, pixi: "animated", overlay: true },
    "pixi-static-hd": { base: "pixi-static", dom: false, pixi: "static", backing: 3 },
    "pixi-animated-hd": { base: "pixi-animated", dom: false, pixi: "animated", backing: 3 },
    "pixi-text-incremental": { base: "pixi-animated", dom: false, pixi: "animated", dialogue: true },
    "pixi-text-incremental-hd": { base: "pixi-animated-hd", dom: false, pixi: "animated", backing: 3, dialogue: true },
  };
  const name = globalThis.ROVES_BENCH_CASE;
  const config = Object.assign({ warmupFrames: 120, measureFrames: 600 }, globalThis.ROVES_BENCH_CONFIG);
  const spec = CASES[name];
  const STAGE_W = 640;
  const STAGE_H = 360;
  const PASSAGE =
    "The lantern swung once in the draught and the shadows of the shelves leaned across the floor. " +
    "Mara counted the steps back to the door, nine of them, then turned the page again and read the " +
    "same line a third time without taking in a single word of it. Outside the rain had settled into " +
    "the patient rhythm it kept all night, and somewhere below the stairs a kettle began to tick as it cooled.";

  const report = (result) => {
    const line = "ROVES_BENCH_RESULT " + JSON.stringify(result);
    console.log(line);
    if (location.protocol === "http:") {
      fetch("/__bench_result", { method: "POST", body: JSON.stringify(result) }).catch(() => {});
    }
  };
  const fail = (step, error) => report({
    schema: "roves-perf-fixture-matrix", version: 1, case: name, status: "failed", step,
    error: String(error && error.stack || error).slice(0, 2000),
  });
  if (!spec) { fail("case", "unknown case " + name); return; }

  // Deterministic PRNG (mulberry32) for layout and scene data.
  function prng(seed) {
    return () => {
      seed |= 0; seed = (seed + 0x6d2b79f5) | 0;
      let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  const PALETTE = ["#e07a5f", "#3d405b", "#81b29a", "#f2cc8f", "#6d597a", "#b56576", "#457b9d", "#e9c46a"];
  const stage = document.getElementById("stage");
  let pixiInfo = null;
  let pixiApp = null;

  function buildDom(withBlur) {
    const rand = prng(1234);
    for (let i = 0; i < 24; i++) {
      const card = document.createElement("div");
      card.className = "card";
      const col = i % 8;
      const row = Math.floor(i / 8);
      card.style.left = (16 + col * 77) + "px";
      card.style.top = (16 + row * 70) + "px";
      card.style.background = PALETTE[Math.floor(rand() * PALETTE.length)];
      card.textContent = "Item " + (i + 1);
      stage.appendChild(card);
    }
    const mover = document.createElement("div");
    mover.id = "mover";
    stage.appendChild(mover);
    const glass = document.createElement("div");
    glass.id = "glass";
    if (withBlur) glass.classList.add("blur");
    const text = document.createElement("p");
    text.id = "line";
    text.textContent = PASSAGE.slice(0, 120);
    glass.appendChild(text);
    stage.appendChild(glass);
    return { mover, text };
  }

  function buildOverlay() {
    const glass = document.createElement("div");
    glass.id = "glass";
    glass.classList.add("blur");
    const text = document.createElement("p");
    text.id = "line";
    text.textContent = PASSAGE.slice(0, 120);
    glass.appendChild(text);
    stage.appendChild(glass);
  }

  // Incremental dialogue: same glass box as the overlay but without backdrop-filter
  // (the probe does not rasterize CSS filters). One <span> holding one character is
  // appended per frame; earlier spans are never touched (typewriter renderers split text
  // across nodes the same way). The paragraph is emptied when the passage completes.
  function buildDialogue() {
    const glass = document.createElement("div");
    glass.id = "glass";
    const text = document.createElement("p");
    text.id = "line";
    glass.appendChild(text);
    stage.appendChild(glass);
    let shown = 0;
    return () => {
      if (shown === PASSAGE.length) { text.replaceChildren(); shown = 0; return; }
      const span = document.createElement("span");
      span.textContent = PASSAGE[shown++];
      text.appendChild(span);
    };
  }

  function pixiDimensions(app) {
    const r = app.renderer;
    const c = app.canvas;
    let rect = null;
    try { const b = c.getBoundingClientRect(); rect = [round(b.width), round(b.height)]; } catch { rect = null; }
    return {
      canvas: [c.width, c.height], renderer_screen: [r.width, r.height], resolution: r.resolution,
      css_client: [c.clientWidth ?? null, c.clientHeight ?? null], css_rect: rect,
      stage_scale: [app.stage.scale.x, app.stage.scale.y],
    };
  }

  async function buildPixi(animated, backing) {
    // Pinned PixiJS ESM build copied next to this file by the runner.
    const PIXI = await import("./pixi.min.mjs");
    const app = new PIXI.Application();
    // autoDensity stays false: Pixi does not touch the canvas CSS size, which the #pixi
    // rule fixes at 640x360 for every backing size.
    await app.init({
      width: STAGE_W * backing, height: STAGE_H * backing, resolution: 1, preference: "webgpu",
      autoStart: false, sharedTicker: false, antialias: false, background: "#1d2a3a",
    });
    app.stage.scale.set(backing);
    const r = app.renderer;
    pixiInfo = {
      version: PIXI.VERSION || null,
      backend: r.name || ({ 1: "webgl", 2: "webgpu", 4: "canvas" })[r.type] || null,
      resolution: r.resolution, canvas: [r.canvas.width, r.canvas.height],
      requested_backing: [STAGE_W * backing, STAGE_H * backing],
    };
    app.canvas.id = "pixi";
    stage.insertBefore(app.canvas, stage.firstChild);
    pixiInfo.after_setup = pixiDimensions(app);
    pixiApp = app;
    const rand = prng(5678);
    const shapes = [];
    for (let i = 0; i < 32; i++) {
      const g = new PIXI.Graphics();
      const color = parseInt(PALETTE[i % PALETTE.length].slice(1), 16);
      const size = 18 + Math.floor(rand() * 30);
      if (i % 2) g.rect(-size / 2, -size / 2, size, size).fill(color);
      else g.circle(0, 0, size / 2).fill(color);
      g.x = 20 + rand() * (STAGE_W - 40);
      g.y = 20 + rand() * (STAGE_H - 40);
      shapes.push({ g, x: g.x, y: g.y, phase: rand() * Math.PI * 2 });
      app.stage.addChild(g);
    }
    const pose = (frame) => {
      for (const s of shapes) {
        s.g.rotation = frame * 0.03 + s.phase;
        s.g.x = s.x + Math.sin(frame * 0.05 + s.phase) * 12;
      }
    };
    pose(0);
    app.render();
    return animated ? (frame) => { pose(frame); app.render(); } : null;
  }

  function animator(dom) {
    switch (spec.animate) {
      case "text": return (frame) => {
        const n = frame % (PASSAGE.length + 1);
        dom.text.textContent = PASSAGE.slice(0, n);
      };
      case "transform": return (frame) => {
        const t = frame % 240;
        const x = t < 120 ? t * 4 : (240 - t) * 4;
        dom.mover.style.transform = "translate(" + x + "px, 0px)";
      };
      case "opacity": return (frame) => {
        const t = frame % 120;
        dom.mover.style.opacity = String((t < 60 ? t : 120 - t) / 60);
      };
      default: return null;
    }
  }

  function percentile(sorted, p) {
    if (!sorted.length) return null;
    const rank = Math.max(1, Math.ceil((p / 100) * sorted.length));
    return round(sorted[rank - 1]);
  }
  function round(v) { return Math.round(v * 1000) / 1000; }
  function summarize(values, thresholds) {
    const sorted = values.slice().sort((a, b) => a - b);
    const out = {
      count: values.length,
      p50: percentile(sorted, 50), p95: percentile(sorted, 95), p99: percentile(sorted, 99),
      max: sorted.length ? round(sorted[sorted.length - 1]) : null,
      mean: values.length ? round(values.reduce((a, b) => a + b, 0) / values.length) : null,
    };
    for (const t of thresholds || []) out["over_" + String(t).replace(".", "_") + "ms"] = values.filter((v) => v > t).length;
    return out;
  }

  async function main() {
    const setupStart = performance.now();
    let work = null;
    let step = "dom";
    try {
      if (spec.dom) {
        const dom = buildDom(Boolean(spec.blur));
        work = animator(dom);
      }
      if (spec.pixi) {
        step = "pixi";
        work = await buildPixi(spec.pixi === "animated", spec.backing || 1);
        if (spec.overlay) buildOverlay();
        if (spec.dialogue) {
          step = "dialogue";
          const pixiWork = work;
          const dialogue = buildDialogue();
          work = pixiWork ? (frame) => { pixiWork(frame); dialogue(); } : () => dialogue();
        }
      }
    } catch (error) { fail(step, error); return; }
    const setupMs = performance.now() - setupStart;
    const total = config.warmupFrames + config.measureFrames;
    const intervals = [];
    const workMs = [];
    let frame = 0;
    let last = null;
    let measureStart = null;
    const visibility = { at_measure_start: null, at_end: null, changes_during_measure: 0 };
    addEventListener("visibilitychange", () => { if (measureStart !== null) visibility.changes_during_measure++; });
    function tick(now) {
      if (frame === config.warmupFrames) { measureStart = now; visibility.at_measure_start = document.visibilityState ?? null; }
      if (frame > config.warmupFrames && last !== null) intervals.push(now - last);
      last = now;
      if (work) {
        const begin = performance.now();
        work(frame);
        if (frame >= config.warmupFrames) workMs.push(performance.now() - begin);
      }
      frame++;
      if (frame <= total) { requestAnimationFrame(tick); return; }
      const duration = now - measureStart;
      visibility.at_end = document.visibilityState ?? null;
      if (pixiApp) pixiInfo.at_end = pixiDimensions(pixiApp);
      report({
        schema: "roves-perf-fixture-matrix", version: 1, case: name, base: spec.base, status: "ok",
        user_agent: navigator.userAgent,
        viewport: { inner_width: innerWidth, inner_height: innerHeight, device_pixel_ratio: devicePixelRatio, stage: [STAGE_W, STAGE_H] },
        supports: {
          backdrop_filter: typeof CSS !== "undefined" && CSS.supports ? CSS.supports("backdrop-filter", "blur(8px)") : null,
          webgpu: typeof navigator.gpu !== "undefined",
        },
        pixi: pixiInfo,
        visibility,
        frames: { warmup: config.warmupFrames, measured: config.measureFrames },
        setup_ms: round(setupMs),
        measure_ms: round(duration),
        raf_fps: round((intervals.length * 1000) / duration),
        raf_interval_ms: summarize(intervals, [16.67, 25, 33.34, 50]),
        work_ms: work ? summarize(workMs, [4, 8, 16.67]) : null,
      });
    }
    requestAnimationFrame(tick);
  }

  if (document.readyState === "complete") main();
  else addEventListener("load", () => main());
})();
