# Roves vs Chrome deterministic visual matrix

Correctness companion to `../fixture-matrix/` (which measures rAF cadence and does no
pixel check). Same bytes in both engines, actual pixels plus computed state, explicit
per-case classification. No build, no engine change, no new dependency: it runs a
published Roves Blitz probe headless with `ROVES_PROBE_SCREENSHOT` and Chrome through
Chrome DevTools (MCP). No full-screenshot equality: only fixed regions.

## Files

- `index.html`: the whole fixture (stylesheet, `#vm-cases` JSON, inline classic script).
  Inline because the probe's `--game` loader skips external classic scripts and Chrome
  refuses module scripts on `file://`; one file means no server and identical bytes.
- `check.py`: Python standard library only. Reads the `#vm-cases` table from the same
  `index.html` (single source of geometry and expectations), the page report and the PNG
  (decoded by `read_rgba` of `../../blitz-v8-probe/fixtures/inline-opacity/check_png.py`,
  640x360 RGBA from the probe or RGB from Chrome).
- Inline opacity reuses `support/blitz-v8-probe/fixtures/inline-opacity/` and its
  `check_png.py` unchanged in logic (the reader now also accepts RGB PNGs).

## Determinism and geometry

- Viewport 640x360, DPR 1 (probe `VIEWPORT`; Chrome `emulate` viewport `640x360x1`),
  white background, every element absolutely positioned from the top-left corner.
- Only solid colour boxes, no text, so fonts cannot move a region.
- The script changes `transform`, `opacity`, `scrollTop`/`scrollLeft` from the rAF
  callback as a pure function of the callback index `f` (1-based):
  `progress = min(f / drive_frames, stop)`, `drive_frames` 20. `stop` 0.5 is the
  midpoint (reached at f 10 and held), `stop` 1 the completion (f 20). Values are exact
  (`translateX` 4f px, opacity f/20, `scrollLeft` 3f). Nothing is written after f 20.
- At f `report_frame` (30) the page reads `getBoundingClientRect()`, computed `opacity`
  and `transform` and the scroll metrics, prints `ROVES_VISUAL_RESULT {json}` with
  `console.log`, stores it in `globalThis.__rovesVisualMatrix` and writes
  `VISUAL_MATRIX_DONE ok` into the hidden `#vm-report`.
- The probe's headless `--game` run drives at least 120 frames and captures after the
  last one, so the screenshot shows the state frozen since f 20. Chrome is captured after
  `VISUAL_MATRIX_DONE` appears.
- Pixel regions per case: `paint` rects inset by 3 px must be the expected colour
  (tolerance 6, fades 10), `blank` rects inset by 2 px must be background (tolerance 3),
  and the ring between 2 and 6 px outside `bounds` must be background (catches clipping
  leaks, wrong translation, unscaled boxes). Edges (antialiasing) are never sampled.

## Cases

| case | gate (Roves) | what | expected |
| --- | --- | --- | --- |
| `transform-mid` | yes | rAF `translateX` 0 -> 80 px, held at midpoint | box at x 60, origin vacated |
| `transform-done` | yes | same, completion | box at x 260 |
| `scale-mid` | yes | rAF `scale` 1/3 -> 1, midpoint 2/3, origin centre | 60 px box shown 40x40 at 370,30 |
| `css-anim-paused-mid` | no | CSS `animation` paused at `-500ms` of 1000 ms (time independent) | `translateX(40px)`, box at x 540 |
| `fade-mid` | yes | rAF opacity 0 -> 1, midpoint | blue at 0.5 over white ~ (128,128,255) |
| `fade-done` | yes | same, completion | opaque blue |
| `fade-out-done` | yes | rAF opacity 1 -> 0, completion | nothing painted, rect unchanged |
| `css-anim-forwards-done` | no | 1 ms CSS animation, `forwards` fill | opacity 1 |
| `clip-overflow` | yes | `overflow: hidden` 60x60 over a 120x120 child | green exactly inside, ring white |
| `clip-radius` | no | same with `border-radius: 30px` | centre green, corners white |
| `scroll-top` | yes | `scrollTop = 60` on stacked red/green/blue stripes | green, `scrollTop` 60, `scrollHeight` 180 |
| `scroll-left-mid` | yes | rAF `scrollLeft` 0 -> 60, midpoint 30 | left half red, right half green, `scrollWidth` 180 |

Gate: Chrome must classify every case `match` (validates the fixture). Roves must match
the gated cases; the others are diagnostic (CSS animation timeline and rounded overflow
clip are not yet evidenced in the probe). Classifications: `match`,
`unsupported_paint` (state right, pixels wrong), `unsupported_state` (pixels right,
state wrong, e.g. a rect that ignores `transform`), `unsupported_state_and_paint`,
`unsupported_api` (an API threw or returned nothing), `not_reported`. The computed
`transform` string is recorded as `transform_info` only: Chrome serializes a resolved
`matrix(...)`, the probe Stylo's computed value, so strings are not compared.

## Invocation

PowerShell, from the repository root; `<probe>` is a published `roves-blitz-v8-probe.exe`.

```powershell
$exe = '<probe>'; $out = 'D:/roves-ci/visual-matrix-<date>'
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem Env: | Where-Object Name -match '^(ROVES_PROBE_|ROVES_EXTRA_ARGS|DENO_WEBGPU_|WGPU_)' |
  ForEach-Object { Remove-Item "Env:$($_.Name)" }
$repo = (Get-Location).Path
foreach ($run in @(@('visual', "$repo/support/perf/visual-matrix"),
                   @('inline', "$repo/support/blitz-v8-probe/fixtures/inline-opacity"))) {
  $env:ROVES_PROBE_SCREENSHOT = "$out/roves-$($run[0]).png"
  $env:ROVES_PROBE_STORAGE_DIR = "$out/storage-$($run[0])"
  # UTF-8 log: `*>` in Windows PowerShell 5.1 writes UTF-16, which check_png.py cannot read.
  Push-Location (Split-Path $exe)
  & $exe --game $run[1] 2>&1 | ForEach-Object { "$_" } | Out-File -Encoding utf8 "$out/roves-$($run[0]).log"
  Pop-Location
}
Remove-Item Env:ROVES_PROBE_SCREENSHOT, Env:ROVES_PROBE_STORAGE_DIR
python -I support/perf/visual-matrix/check.py --engine roves --png "$out/roves-visual.png" `
  --report "$out/roves-visual.log" --out "$out/roves-visual.json"
python -I support/blitz-v8-probe/fixtures/inline-opacity/check_png.py "$out/roves-inline.png" "$out/roves-inline.log"
```

Chrome DevTools (MCP), visible page, in this order:

1. `new_page` `about:blank`; `emulate` with `viewport: "640x360x1"`.
2. `navigate_page` to `file:///<repo>/support/perf/visual-matrix/index.html` (spaces as `%20`).
3. `wait_for` `["VISUAL_MATRIX_DONE"]`.
4. `evaluate_script` `() => "ROVES_VISUAL_RESULT " + JSON.stringify(globalThis.__rovesVisualMatrix)`
   with `filePath` `<out>/chrome-visual.json` (the checker finds the prefixed line whatever
   wrapper the tool writes around it).
5. `take_screenshot` with `filePath` `<out>/chrome-visual.png` (must be 640x360).
6. `navigate_page` to `file:///<repo>/support/blitz-v8-probe/fixtures/inline-opacity/index.html`;
   `evaluate_script` `() => document.getElementById("report").textContent` with `filePath`
   `<out>/chrome-inline.txt`; `take_screenshot` `<out>/chrome-inline.png`.

```powershell
python -I support/perf/visual-matrix/check.py --engine chrome --png "$out/chrome-visual.png" `
  --report "$out/chrome-visual.json" --out "$out/chrome-visual.json.check"
python -I support/perf/visual-matrix/check.py --compare "$out/roves-visual.json" "$out/chrome-visual.json.check"
python -I support/blitz-v8-probe/fixtures/inline-opacity/check_png.py "$out/chrome-inline.png" "$out/chrome-inline.txt"
```

A hidden or occluded Chrome page stops rAF (see `../fixture-matrix/README.md`), so the
wait in step 3 times out instead of reporting; record that as not run, not as a result.

## Chromium reference (f2700724, sparse checkout)

- `cc/trees/transform_node.h` (~L36): a node's transform is
  `T_post_translation * T_origin * T_scroll * M_local * -T_origin`; scroll offset is a
  translation in the same transform tree (`cc/trees/scroll_node.h` ~L65, "the transform
  node containing the scroll offset"). Hence translate, scale about the origin and
  scroll are checked by the same means: where the box lands, and the vacated area.
- `cc/trees/effect_node.h`: opacity is an effect node applied to the whole subtree once
  (group opacity), so a 0.5 box over white is one blend (~128), and opacity 0 paints
  nothing while layout (`getBoundingClientRect`) keeps the box. Inline-level group
  opacity is the existing inline-opacity fixture (Blink `paint_property_tree_builder.cc`
  reasoning recorded in TODO.md, Blitz #1170).
- `cc/trees/clip_node.h`: overflow clips are clip-tree rects in the transform space of the
  clipping box; the ring check is the observable effect (no leak outside the box).
  Rounded clips are a separate mask path in Chrome; kept diagnostic here.
- Main-thread driving and compositor animations: see `../fixture-matrix/README.md`
  (`cc/scheduler/scheduler.h`, `cc/animation/`). The rAF cases are main-thread style
  changes in both engines; the two CSS animation cases use timing that does not depend on
  frame rate (paused at a fixed local time, or finished long before the report).
- Roves differences: no layers or partial damage; the probe paints the whole scene on the
  CPU (`render_rgba`, Vello CPU), Chrome composites on the GPU. The matrix compares the
  observable result, not the mechanism.

## Not covered (next tranche)

WebGPU canvas compositing with DOM, clip and `backdrop-filter` is the separate
`../gpu-composite/` fixture (GPU window, native screen capture). The real template
typewriter (computed per-character visibility, timing, completion and cancel) is observed
by `../typewriter-probe/`, which relies on the inline-opacity gate above for pixels. Still
not covered: pixel timelines of the typewriter, CSS transitions driven by wall-clock time, `filter`, text rendering,
`scroll` events, smooth scrolling and DPR other than 1.
