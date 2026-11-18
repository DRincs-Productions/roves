# Roves vs Chrome fixture matrix

Deterministic micro-scenes run with identical bytes in the Roves Blitz probe
(`--game <dir> --window`) and in an installed Chrome. Artifact-first: it runs an
existing probe executable from CI and the system Chrome; no build, no engine change,
no elevation, no ETW. It complements, and does not replace, the real game
(visual-novel template) as the independent acceptance scene.

## Files

- `index.html`: shared page, fixed 640x360 CSS-px `#stage` at the top-left corner.
- `bench.js`: cases, deterministic work and in-page measurement (ES module).
- `run.mjs`: runner (Node >= 20, built-ins only). It generates one directory per case
  under `<out>/fixtures/<case>/` (`index.html`, `bench.js`, `case.js`, and for Pixi cases
  `pixi.min.mjs`), runs each engine on it and writes `<out>/results.json`,
  `<out>/runs.csv` and per-run logs under `<out>/runs/<rep>-<case>-<engine>/`.

## Cases (one variable each, relative to `base`)

| case | base | change |
| --- | --- | --- |
| `blank` | - | empty stage; only the rAF measuring loop |
| `static-dom` | `blank` | 24 cards, `#mover`, glass panel (translucent, no filter), one text line; never changed |
| `text-incremental` | `static-dom` | the text line grows by one character per frame (typewriter, fixed passage, wraps) |
| `transform` | `static-dom` | `#mover` `transform: translate()` set every frame from the rAF callback |
| `opacity` | `static-dom` | `#mover` `opacity` set every frame from the rAF callback |
| `backdrop-filter` | `transform` | glass panel gets `backdrop-filter: blur(8px)`; the mover passes under it |
| `pixi-static` | `blank` | PixiJS 8.19.0 canvas (32 Graphics), rendered once |
| `pixi-animated` | `pixi-static` | scene posed and `app.render()` called every frame |
| `pixi-text-blur` | `pixi-animated` | DOM dialogue overlay over the canvas: glass panel with backdrop blur and static text |
| `pixi-static-hd` | `pixi-static` | Pixi backing 1920x1080 instead of 640x360; same CSS box and world geometry |
| `pixi-animated-hd` | `pixi-animated` | Pixi backing 1920x1080 instead of 640x360; same CSS box and world geometry |
| `pixi-text-incremental` | `pixi-animated` | incremental DOM dialogue over the animated canvas (see below), no backdrop filter |
| `pixi-text-incremental-hd` | `pixi-animated-hd` | the same incremental dialogue over the 1920x1080-backed canvas (pairs with `pixi-text-incremental`: only the backing differs) |

Backing cases (`-hd`): the 54 alternating runs at 640x360 backing (`fixture-matrix-combined-triples`)
did not reproduce the visual-novel template lag; the template's Pixi'VN canvas uses a
1920x1080 backing shown in a smaller CSS box (not re-measured for this change). The `-hd` cases keep the fixed 640x360 visible
stage (`#pixi` CSS rule, Pixi `autoDensity` left false so Pixi never sets the CSS size) and
`resolution: 1`, but call `app.init({ width: 1920, height: 1080 })` and scale `app.stage` by 3,
so the 32 shapes keep the same world coordinates, sizes and motion as the 640x360 cases; only
the number of rendered/composited pixels changes (9x). Pixi `Graphics` are tessellated in local
space, so circle segment counts are the same; at 3x they may look slightly more faceted.
The original cases are unchanged (scale 1).

Incremental dialogue (`pixi-text-incremental*`): the same `#glass` box and `#line` paragraph as
the overlay, without `backdrop-filter` (not rasterized by the probe, so not required). Every
frame appends one `<span>` holding the next character of the fixed passage; earlier spans are
never touched, mirroring typewriter renderers that split visible text across nodes. When the
passage (~370 characters) is complete the paragraph is emptied and starts again. The work
function runs the Pixi frame and then the dialogue step, so `work_ms` covers both.

All work is a pure function of the frame index (seeded PRNG for layout, no
wall-clock animation), so both engines do the same work per frame however fast frames
arrive. DOM changes are made from the rAF callback in both engines (Chrome: main-thread
style changes, not compositor CSS animations; see Chromium notes below). Pixi requests
`preference: "webgpu"` with `resolution: 1`, `antialias: false`, `autoStart: false` (no
Pixi ticker: the bench loop renders) and is loaded with `import()` from the pinned
`support/blitz-v8-probe/js/node_modules/pixi.js` (8.19.0, lockfile `js/package-lock.json`;
the runner checks the version and records the SHA-256). The template is not touched.
Pixi Text is not used (it needs a 2D canvas the probe does not provide).

## Metrics

Per run the page reports (`ROVES_BENCH_RESULT <json>` on the console, and POSTed to the
runner under Chrome):

- `raf_interval_ms`: intervals between consecutive rAF timestamps over the measured
  frames (P50/P95/P99/max/mean, counts over 16.67/25/33.34/50 ms) and `raf_fps`.
  These are rAF cadence, not displayed frames.
- `work_ms`: time spent in the per-frame work function (absent for static cases).
- `setup_ms`, `viewport` (inner size, `devicePixelRatio`), `supports` (`CSS.supports`
  for `backdrop-filter`, `navigator.gpu`), Pixi backend/resolution/canvas size,
  `pixi.requested_backing`, and `pixi.after_setup`/`pixi.at_end` with the actual
  `canvas` backing (`width`/`height`), `renderer_screen`, `resolution`, CSS
  `clientWidth/Height`, `getBoundingClientRect()` size and `app.stage` scale
  (`runs.csv` column `pixi_backing_css` = `<backing>/<css>` at end of run),
  `visibility` (`visibilityState` at measure start/end and changes).

Roves runs also store the probe's own `ROVES_PROBE_FRAME_TIMINGS` record
(`frame_step_interval`, `cpu_render_present`, `cpu_shell_redraw`, raster cache and
present causes; covers the whole window run, warm-up included). `results.json` records the
probe and Pixi SHA-256, fixture hashes, frame counts, backend and JIT mode.

## Invocation

```sh
# smoke: all 9 cases, both engines, 1 repetition, 10 warm-up + 60 measured frames
node support/perf/fixture-matrix/run.mjs --smoke --cases all \
  --roves-exe D:/roves-ci/44364448564-vello-ab/extracted/single/roves-blitz-v8-probe.exe \
  --out D:/roves-ci/fixture-matrix-smoke-all

# measurement: 3 alternating repetitions, 120 warm-up + 600 measured frames (defaults)
node support/perf/fixture-matrix/run.mjs --cases all --reps 3 \
  --roves-exe <probe.exe> --out D:/roves-ci/fixture-matrix-<date> --label <rev>
```

Options: `--engines roves,chrome`, `--cases all|dom|pixi|<a,b,...>`, `--reps N`,
`--warmup-frames N`, `--measure-frames N`, `--jitless` (Roves `ROVES_PROBE_JITLESS=1`),
`--roves-backend vulkan` (`DENO_WEBGPU_BACKEND`), `--chrome-exe <path>`,
`--chrome-dsf 1` (`--force-device-scale-factor`), `--timeout-s 120`, `--label`.
Exit code 0 only when every run reported `ok`.

Roves environment: inherited `ROVES_PROBE_*`/`DENO_WEBGPU_*`/`WGPU_*`/`ROVES_EXTRA_ARGS`
are cleared; `ROVES_PROBE_FRAME_TIMINGS=1`, fresh `ROVES_PROBE_STORAGE_DIR` per run. The
runner closes the window with `WM_CLOSE` once the page reports (`--window-frames` is only a
safety cap). Chrome: fresh `--user-data-dir` per run, `--app` window at 660x420,
extensions/sync/background networking off, and `--disable-features=CalculateNativeWinOcclusion
--disable-backgrounding-occluded-windows`. Without the last two Chrome marked the window
`hidden` and stopped rAF (1 callback in 3 s) on this machine whenever Windows considered it
covered, while Roves never throttles; with them both engines keep running, but timings from a
covered window or a locked session are not representative of presented frames. The runner
kills only processes of its own Chrome profile and its own probe. Roves run timeouts are
cancelled once the probe exits (before 2026-11-18 a pending timeout kept Node alive for up to
`--timeout-s` after the last run).

## Known differences and limits

- Viewport: Roves opens a 640x360 logical window; Chrome's `--app` inner area is slightly
  larger (644x381 on the GTX 1050 machine). The stage is fixed at 640x360 in both.
- DPR: the probe always reports `devicePixelRatio` 1 to JS, but its surface follows the
  Windows display scale. Run at 100% scaling or set `--chrome-dsf` to the same scale.
- `backdrop-filter`: the probe build does not rasterize CSS filters (see
  `support/blitz-v8-probe/README.md`), and has no `CSS.supports` (`supports` is `null`), so
  the `backdrop-filter` and `pixi-text-blur` Roves runs measure layout/paint without the blur.
  Compare them only together with a pixel check.
- Roves has no `visibilityState` (`null`).
- Chrome rAF timestamps are coarsened (0.1 ms without cross-origin isolation).
- rAF cadence is not presentation: for displayed-frame P95/P99 use PresentMon (Roves
  `ROVES_PROBE_FRAME_TRACE`, procedure in `docs/ROVES_PERFORMANCE.md`) or a Chrome DevTools
  trace; CPU/GPU cost per process is not collected yet.
- No pixel/screenshot check is done by the runner; the probe's `present_causes.canvas`
  confirms Pixi canvas presents per frame. `present_causes` are exclusive per present: when
  the DOM also changes (`pixi-text-incremental*`) the present is counted as `scene`, not
  `canvas`. The `-hd` cases report a 1920x1080 backing and a 640x360 CSS box in both
  engines, but that the backing is actually downscaled into the box on screen is not
  pixel-verified.
- The `-hd` cases reproduce the backing size, not the template itself: no textures, sprites,
  Pixi filters, React tree, fonts or audio. The incremental dialogue has no backdrop blur,
  Markdown, per-character animation or React reconciliation.

## Chromium reference (f2700724, sparse checkout)

`cc/scheduler/scheduler.h` (one main-frame commit per BeginFrame; rAF runs in the main
frame), `cc/metrics/frame_sequence_metrics.h` (Chrome tracks smoothness separately for
`kRAF`, `kMainThreadAnimation`, `kCompositorAnimation`, `kCanvasAnimation`), `cc/animation/`
(transform/opacity/filter curves can run on the compositor without a main frame). The matrix
drives transform/opacity from rAF so both engines do main-thread work; a compositor CSS
animation variant is a separate case to add, not mixed into these.

WebGPU canvas backing and damage (consulted 2026-11-18 for the `-hd` and incremental cases):

- `third_party/blink/renderer/modules/webgpu/gpu_canvas_context.cc`: on canvas resize
  (`ReplaceDrawingBuffer`, ~L186-206) `texture_descriptor_.size` and
  `swap_texture_descriptor_.size` become the canvas `width`/`height`, i.e. the backing, not
  the CSS box; `configure()` (~L540-658) creates a `WebGPUSwapBufferProvider` and calls
  `SetNeedsCompositingUpdate()`; `getCurrentTexture()` (~L699-740) returns a new swap texture
  per frame and calls `DidDraw()` ("requesting a new canvas texture ... is enough to mark it
  as dirty"), whatever was actually drawn. Scaling to the CSS box is left to the compositor.
- `cc/layers/texture_layer.cc` (`Update`/`SetTransferableResourceInternal`, ~L87-179) and
  `cc/layers/texture_layer_impl.cc` (`SetTransferableResource`, ~L236): each new swap
  resource is pushed to the texture layer; `cc/trees/damage_tracker.cc` (~L425-454) unions
  the whole layer rect for a new/changed layer, otherwise `update_rect` plus layer damage
  clipped to its visible rect.

Consequences for the fixture: in Chrome a 1920x1080 WebGPU canvas costs a 9x larger swap
texture per frame plus a GPU downscale in the compositor, while the damaged area on screen
stays the 640x360 layer; DOM text changes are damaged as their own invalidated rect, not
the whole canvas (the paint-invalidation side lives in Blink paint code outside this sparse
checkout and was not re-read). So
the `-hd` cases isolate backing cost from screen damage, and the incremental dialogue tests
whether Roves, which repaints its whole scene for any DOM change, pays more when that change
overlaps an animated canvas. Chromium's per-layer partial damage is not something Roves has;
the comparison measures the effect, it does not reproduce Chrome's layering.

## Smoke result (2026-11-18, not a measurement)

GTX 1050 machine, probe `44364448564-vello-ab/single` (rev 0cdb0da9300), Chrome 157.0.8103.79,
`--smoke --cases all`: 18/18 runs `ok`, Pixi backend `webgpu` in both engines, Roves
`present_causes.canvas` 71 in the Pixi animated case. One repetition of 60 frames per case;
no performance conclusion.

Backing/dialogue smoke (2026-11-18, not a measurement): same probe and Chrome,
`--smoke --cases pixi`, output `D:/roves-ci/fixture-matrix-smoke-backing-dialogue-20261118`:
14/14 runs `ok` in 80 s wall, backend `webgpu` everywhere. Actual Pixi dimensions in both
engines: `-hd` cases canvas 1920x1080, renderer screen 1920x1080, resolution 1, CSS client
and rect 640x360, stage scale 3; other cases 640x360 / 640x360 / scale 1. Roves present
causes: `pixi-animated(-hd)` canvas 71; `pixi-text-incremental(-hd)` scene 58 + canvas 13,
with `cpu_render_present` P99 2.86/3.14 ms versus 1.12/1.10 ms without the dialogue. One
repetition of 60 frames; rAF cadence was ~58 fps (Roves) and ~56 fps (Chrome) in every case.
