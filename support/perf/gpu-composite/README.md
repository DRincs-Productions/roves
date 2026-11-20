# Roves vs Chrome GPU canvas compositing fixture

Companion of `../visual-matrix/` (DOM-only, CPU raster) for the case that matrix lists as
not covered: a WebGPU canvas composed with DOM under it, over it, clipped around it and
with `backdrop-filter` over it. Same bytes in both engines, fixed geometry, expected pixels
in fixed regions, explicit per-case classification. No engine, template or dependency
change; Python standard library only.

## Files

- `index.html`: the whole fixture (stylesheet, `#gc-cases` JSON, inline classic script; same
  reasons as the visual matrix: the probe's `--game` loader skips external classic scripts
  and Chrome refuses module scripts on `file://`).
- `check.py`: reads `#gc-cases` from the same `index.html`, the page report and a 640x360
  PNG. Region counting, rings, report decoding and the PNG reader are imported from
  `../visual-matrix/check.py` (which uses
  `../../blitz-v8-probe/fixtures/inline-opacity/check_png.py`).

## What the page does

- Two opaque WebGPU canvases (`alphaMode: "opaque"`, `getPreferredCanvasFormat()`, else
  `bgra8unorm`) on one device, redrawn every rAF frame through the same
  `getContext("webgpu")` / `configure` / `getCurrentTexture` / `queue.submit` path PixiJS
  8.19 uses in the probe (`dom.js` registers the canvas texture for the window compositor
  on `getCurrentTexture`). Pixi itself is not embedded: its renderer is already covered by
  the probe's `webgpu_pixi_8_19_0` scenario, and a raw WGSL draw keeps the canvas pixels
  exact (no antialiasing, no asset load, no Pixi version drift between engines).
- `#gpu-a` (400x240 at 20,20): a full-screen triangle whose fragment colour depends only on
  the canvas pixel position: red left of x 200, blue right, green band from y 180 (catches
  mirroring and vertical flips). `#gpu-b` (200x160): cleared to cyan, placed at -40,-40
  inside `#clipbox` (120x80 at 460,20, `overflow: hidden`).
- At rAF callback 30 after the device is ready the page reads `getBoundingClientRect()` of
  every element in `state`, the computed `backdrop-filter` (info only, Stylo and Chrome
  serialize differently), prints `ROVES_GPU_COMPOSITE_RESULT {json}`, stores it in
  `globalThis.__rovesGpuComposite` and writes `GPU_COMPOSITE_DONE <status>` into the hidden
  `#gc-report`. It keeps drawing the same frame afterwards. Status `ok`, `errors` (a thrown
  draw or a WebGPU `uncapturederror`), `no_webgpu`, `no_adapter`, `failed`.

## Cases

| case | gate (Roves) | what | expected (Chrome) | known Roves outcome |
| --- | --- | --- | --- | --- |
| `canvas-pixels` | yes | canvas content, orientation, white outside | red/blue/green, white | match |
| `dom-under-canvas` | yes | magenta box before the canvas | hidden under the canvas, visible outside | match |
| `dom-foreground` | yes | opaque yellow box after the canvas | yellow, red ring around it | match |
| `dom-foreground-alpha` | yes | 50% white box over blue | ~(128,128,255), blue ring | match |
| `backdrop-tint` | yes | `#glass`: `rgba(255,255,255,.25)` + `backdrop-filter: blur(8px)` across the red/blue edge (x 220) | tints (255,64,64) / (64,64,255) >= 26 px from every edge; canvas unchanged just outside | match (tint painted, no blur needed there) |
| `backdrop-blur-edge` | no, `known_no_blur` | columns x 218 and 221 (1.5 px from the edge) under `#glass` | red/blue mix (~174/145 for sigma 8) | sharp tints: `known_no_blur` |
| `canvas-clip` | no, `known_no_canvas_clip` | 200x160 canvas in a 120x80 `overflow: hidden` box, black DOM child over it | cyan only inside the box, white strips outside, black box above | before 2026-11-20 cyan in the strips: `known_no_canvas_clip`; with the rectangular scissor expected `match`, not yet verified on a GPU |

Classification: `match`, the declared `known_*` label (only when every `paint` region is
right, the `expect` regions are wrong and the declared `alternative` regions are right),
`unsupported_paint` (anything else), `unsupported_api` (page status not ok, errors, no
report). Element rects that differ from `state` fail the run as a problem. A Roves run is
`passed_with_known_divergences` when the gated cases match and the two diagnostic cases
show exactly their declared divergence; the output lists them under `known_divergences`,
and a known divergence that disappears is reported as a warning so the fixture and docs get
updated. Chrome must classify every case `match` (it validates the fixture).

Why these divergences are expected in Roves (not hidden, not gated):

- No blur: the probe graph does not enable anyrender_vello_cpu `filters` (see
  `support/blitz-v8-probe/Cargo.toml` and the `vello-mt` TODO entry), and even with it the
  backdrop under `#glass` is a separate GPU canvas quad (`presentation.rs` `split_scene`),
  not part of the raster segment that the CPU filter would read. The element's own
  background is still painted, which `backdrop-tint` gates.
- No canvas clip (expected to disappear, not yet verified): since 2026-11-20 `split_scene`
  intersects the open explicit clip layers that are exact axis-aligned rectangles into
  `CanvasDraw::clip` and the compositor applies it as the render pass scissor of the canvas
  draw (wgpu-core 29.0.4 `render_pass_set_scissor_rect`), so `#clipbox` (sharp
  `overflow: hidden`, integer coordinates) should clip `#gpu-b` exactly and classify
  `canvas-clip` as `match`. The checker already accepts `match` for this non-gated case and
  warns "known divergence no longer observed". **After one positive real-GPU run** of the
  recipe below (Roves `match`, Chrome `match`, rects equal), set `canvas-clip` to
  `"gate": true`, drop `known`/`expect`/`alternative`, bump `fixture_version` and update
  this table; until then keep `known_no_canvas_clip` accepted. Rounded, rotated or path
  clips and opacity-layer clips are still not applied (probe README). The netrender
  renderer still falls back for clipped canvases (unchanged classification).

## Capture: no deterministic readback in the GPU window path

Explored, no production code added:

- `ROVES_PROBE_SCREENSHOT` in `--game` runs (`src/game.rs` `run`) renders with
  `render_rgba` (Vello CPU): `Paint::Resource` canvas placeholders are transparent there, so
  the PNG shows the DOM over a white hole. It also forces the headless path before the
  window. Do not set it for this fixture.
- `GpuWindowRenderer` (`src/presentation.rs`) never reads a texture back (`readback:false`).
- The netrender fixture readback (`ROVES_PROBE_NETRENDER_FIXTURE`) exists only in the opt-in
  netrender build, is tied to that fixture's six pixels, and netrender falls back on
  filters/clipped canvases, i.e. on exactly what this fixture tests.

So the Roves pixels come from the screen, like the existing `window mode with a real OS
click` step of `.github/workflows/blitz-probe.yml` (`CopyFromScreen` of the window). A game
window is undecorated, non-resizable, at logical (100,100), 640x360 (`window.rs`
`WINDOW_ORIGIN`, `VIEWPORT`). It needs a real GPU (GitHub runners have none), an unlocked
desktop, 100 % display scaling (the checker requires a 640x360 PNG and DPR 1) and the
window unoccluded and not minimized during the capture.

## Recipe

Roves, PowerShell from the repository root, `<probe>` a published
`roves-blitz-v8-probe.exe` (normal or JITless build):

```powershell
$exe = '<probe>'; $out = 'D:/roves-ci/gpu-composite-<date>'
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem Env: | Where-Object Name -match '^(ROVES_PROBE_|ROVES_EXTRA_ARGS|DENO_WEBGPU_|WGPU_)' |
  ForEach-Object { Remove-Item "Env:$($_.Name)" }
$env:ROVES_PROBE_STORAGE_DIR = "$out/storage"
$log = "$out/roves-gpu.log"; $err = "$out/roves-gpu.err"
$fixture = (Resolve-Path support/perf/gpu-composite).Path
$p = Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) `
  -ArgumentList @('--game', "`"$fixture`"", '--window', '--window-frames', '900') `
  -RedirectStandardOutput $log -RedirectStandardError $err -PassThru -NoNewWindow
$done = $null
for ($i = 0; $i -lt 60 -and -not $done -and -not $p.HasExited; $i++) {
  Start-Sleep -Milliseconds 250
  if (Test-Path $log) { $done = Select-String -Path $log -Pattern 'ROVES_GPU_COMPOSITE_RESULT' | Select-Object -First 1 }
}
if (-not $done) { throw 'no ROVES_GPU_COMPOSITE_RESULT: record as not run' }
Start-Sleep -Milliseconds 500   # a few more presented frames of the same content
Add-Type -AssemblyName System.Drawing
$bitmap = New-Object System.Drawing.Bitmap 640, 360
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen(100, 100, 0, 0, $bitmap.Size)
$bitmap.Save("$out/roves-gpu.png", [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose(); $bitmap.Dispose()
$p.WaitForExit(); Remove-Item Env:ROVES_PROBE_STORAGE_DIR
python -I support/perf/gpu-composite/check.py --engine roves --png "$out/roves-gpu.png" `
  --report $log --out "$out/roves-gpu.json"
```

Also keep from the log `gpu_window_prepare` (must not be `cpu_fallback`, otherwise the
softbuffer window painted transparent canvases: record as not run), the final
`gpu_window_presentation` (`canvas_layers` > 0, no wgpu errors) and `game_window_closed`.
If the display scaling is not 100 % the PNG is not 640x360 and the checker exits 2.

Chrome DevTools (MCP), visible page, in this order:

1. `new_page` `about:blank`; `emulate` with `viewport: "640x360x1"`.
2. `navigate_page` to `file:///<repo>/support/perf/gpu-composite/index.html` (spaces as `%20`).
3. `wait_for` `["GPU_COMPOSITE_DONE"]`.
4. `evaluate_script` `() => "ROVES_GPU_COMPOSITE_RESULT " + JSON.stringify(globalThis.__rovesGpuComposite)`
   with `filePath` `<out>/chrome-gpu.json`.
5. `take_screenshot` with `filePath` `<out>/chrome-gpu.png` (must be 640x360).

```powershell
python -I support/perf/gpu-composite/check.py --engine chrome --png "$out/chrome-gpu.png" `
  --report "$out/chrome-gpu.json" --out "$out/chrome-gpu.json.check"
python -I support/perf/gpu-composite/check.py --compare "$out/roves-gpu.json" "$out/chrome-gpu.json.check"
```

`GPU_COMPOSITE_DONE no_adapter`/`no_webgpu` in Chrome (WebGPU blocked or headless without a
GPU) or a hidden page that stops rAF is "not run", not a result. Record the Chrome version,
GPU, backend (`chrome://gpu`) and whether the page was visible.

## Chromium reference (f2700724, sparse checkout)

- `cc/trees/effect_node.h` (L84-90): `backdrop_filters`, `backdrop_filter_bounds` and
  `backdrop_filter_quality` live on the effect node of the element; backdrop-filter is a
  render-surface reason (`kBackdropFilter`). The filter reads whatever was composited
  below, canvas layers included, so a WebGPU canvas under `#glass` is blurred.
- `components/viz/service/display/skia_renderer.cc` (~L3294-3329): the backdrop is cropped
  to the (rounded-out) backdrop filter bounds with `SkImageFilters::Crop(...,
  SkTileMode::kMirror)` when the blur would sample outside. Hence `backdrop-tint` samples
  >= 26 px (more than 3 sigma for blur(8px)) from the glass edges, where mirror vs clamp vs
  transparent edge handling cannot change a pixel, and `backdrop-blur-edge` samples only
  the vertical canvas edge in the middle of the glass.
- `cc/trees/clip_node.h`: overflow clips are clip-tree rects applied to every layer in the
  subtree, canvas layers included (see the visual matrix README); `canvas-clip` checks the
  observable effect.
- `components/viz/common/quads/shared_quad_state.h` (L71-72): each quad's clip is an
  optional integer `gfx::Rect clip_rect` in target space, separate from the quad rect;
  rounded corners travel separately in `mask_filter_info` (L67-70, applied by
  `skia_renderer.cc` as an antialiased `SkRRect` clip, ~L1737-1753).
  `components/viz/service/display/direct_renderer.cc` `SetScissorStateForQuad`
  (~L532-547) intersects the render pass scissor with `clip_rect` and sets it as the
  scissor test for that quad, or disables the scissor when there is no clip; quads stay
  untouched. Roves follows the same split: the exact rectangular part becomes a per-draw
  wgpu scissor (integer, intersected with the viewport, reset for every layer), the quad
  and UVs are unchanged. Differences: Roves has no rounded/mask stage (rounded clips are
  ignored for canvases, not approximated), rounds a fractional clip outwards because
  scissors are integer, and does not apply the clip of opacity layers.
- Roves differences: Chrome composites the canvas as a texture layer in viz with clips and
  filters on its property trees; the probe composites Vello CPU raster segments and
  canvas quads with one textured-quad pipeline (premultiplied source-over), a
  rectangular-scissor-only clip stage and no filter stage. The fixture compares the
  observable result only.

## Not covered

Rounded or transformed clips of a canvas, `filter:` on the canvas itself, `opacity` groups
spanning a canvas (applied per side, see the probe README), non-opaque canvases,
blur quality/edge pixels near the glass border, DPR other than 1, resize, Pixi's own
renderer output in this paired run.
