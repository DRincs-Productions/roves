# Netrender integration for Blitz

## First Claude assignment: shared-device proof

Use the local reference at C:/roves-media-probe/netrender-reference. Start with
merely-made/netrender d2f13ca5ecd20df89d148a7cebef28c0962b90ca (wgpu 29, Vello 0.9)
and the probe's existing deno_webgpu 0.227.0 / wgpu-core 29.0.1. Verify the resolved
locked graph and Rust toolchain, then inspect boot_on, device feature requirements,
and public APIs for sharing a wgpu Device/Queue and importing an existing texture.
Check lifetime and destruction order, and preserve the WebGPU feature surface
visible to the game. No private layout casts, second device, readback or implicit
copy. Evaluate upstream interop helpers only if they preserve these constraints.
Older compatible releases are allowed. Record each pin and its upgrade trigger in TODO.

Build the smallest opt-in fixture that draws a known WebGPU texture created through
Deno, places Netrender layers before and after it, and checks pixels. Readback of the
final fixture output for assertions is fine; the production canvas path must stay
on the shared GPU device without copies. Keep the existing renderer as fallback.
Use online CI for heavy builds. If there is no supported bridge, record the
specific API or feature blocker and stop this assignment before wiring the renderer
into the game. Claude delivers the diff and evidence without commit or push.

### Result of the first assignment (2026-11-15, uncommitted)

Stopped at the documented blocker, before wiring Netrender into the probe. Evidence and
repro: `support/netrender-shared-device-proof/README.md`.

- Graph: wgpu 29.0.4 + Netrender d2f13ca5 unify with deno_webgpu's `wgpu-core =29.0.1`
  (single wgpu-core/hal/types). Netrender needs no features; its 28 inter-stage limit is
  legacy and not needed with WebGPU default limits.
- Blocker: wgpu 29/30 cannot adopt an existing wgpu-core `Global`/device/texture, and
  wgpu-core ids are single-owner. Proposed upstream change (+153 lines) in
  `upstream/wgpu-29-core-interop.patch`; with it the fixture passes on GTX 1050 Vulkan
  and DX12 with one device, unchanged game features/limits and no canvas copy.
- Netrender bug: interleaved external textures blend later translucent ops twice;
  proposed fix in `upstream/netrender-d2f13ca5-external-texture-order.patch`.
- The texture comes from deno_webgpu's own wgpu-core calls, not from V8 JS: a real
  `navigator.gpu` run needs the patched wgpu in the probe graph (fork not authorized).
- Next: open the wgpu and Netrender issues/PRs (search existing ones first), then
  decide between waiting for a release or an authorized temporary fork.
- Netrender issue: creating it through the GitHub integration returned 403 (2026-11-15);
  not opened yet.

### Second assignment: V8 producer (2026-11-15, uncommitted)

- Fixture split into `src/lib.rs` (shared adoption, scene, assertions, hub counts) and two
  producers: the unchanged raw wgpu-core one (`src/main.rs`) and `--features deno --bin
  deno_js`, a real deno_core 0.413 `JsRuntime` + deno_webgpu 0.227 where unprivileged JS
  creates and draws the texture. A bootstrap-captured cppgc op registers the embedder's
  clone ids; V8 GC must release the guest ids before Netrender renders. Patches stay in
  the fixture (`[patch.crates-io]`); probe graph and production renderer untouched.
- Audit (clone safety, public APIs, drop order with JS handles, guest surface) and an
  isolated manual CI plan are in the fixture README; no workflow changed.
- Verified: `cargo check --features deno --bins` clean; raw producer still 6/6 on
  GTX 1050 Vulkan with the order fix. **`deno_js` not run yet** (heavy V8 build kept for
  CI): next step is that run, recording the GC pass count and the hub lines.

### Third assignment: isolated CI (2026-11-15, uncommitted, not pushed)

- `.github/workflows/netrender-proof.yml` (job `windows-warp`, push on the fixture paths or
  the workflow file, plus `workflow_dispatch`) builds only the fixture on WARP DX12:
  exact pins and crates.io checksums, pristine-wgpu E0599 repro, patched tree = crates.io +
  interop patch only, negative run (published Netrender: exit 1 with exactly the two
  ordering pixels failing) and positive run (order fix by path override: 6/6, lock restored
  and diffed) for both producers; logs and executables uploaded. Checks in
  `support/netrender-shared-device-proof/ci/verify.py`.
- Pre-CI audit fix: the guest frame now includes the `GPUAdapter` and the op borrows it.
  `GPUDevice` keeps only the adapter id; a GC between the guest's completion and the op
  could vacate it and `adapter_clone_id` would panic.
- Next: read the first run (GC pass count, adapter line, the two negative pixels on WARP)
  and record it in the fixture README and `TODO.md`.

### Fourth assignment: CI Netrender #4 fixes (2026-11-15, uncommitted, not pushed)

- Job 132956577307: static link OK. `deno_js` panicked on the template-literal guard (the
  WGSL comments contain backticks): the shader is now a JSON string literal
  (`deno_core::serde_json`). Raw runs exited 139 on WARP after the adapter line.
- Local lldb diagnosis: access violation (write) in a `d3d10warp.dll` worker while Vello's
  compute frame runs; reproduced with a plain wgpu device and no bridge, so WARP cannot
  run Netrender/Vello. Details in the fixture README ("WARP").
- Workflow is now a matrix: `linux-lavapipe` runs negative and positive for both
  producers with unchanged gates; `windows-compile` only builds (static-CRT link).
  The pristine E0599 gate was tightened (job 132956577307 passed it with 0 x E0599).
- CI #6 (`43821714446`, `d85be3fb68f`) passed on both jobs. Linux lavapipe
  ran both the raw and real Deno JavaScript producers: the negative runs failed
  only the two expected ordering pixels and the patched runs passed all six
  pixel checks and the shared device and lifetime checks. Windows compiled both
  variants. The companion Servo run `43821714304` also passed.
- Netrender upstream branch `BlackRam-oss:codex/fix-external-texture-order`
  is published at `5a73b3c2`; GitHub returned 403 for the upstream issue and PR.
  The wgpu upstream proposal is in progress in the separate `wgpu-reference`
  checkout. The proof patches are still isolated from the Blitz renderer.

### Fifth assignment: opt-in Netrender window renderer (2026-11-16, uncommitted, not pushed)

Scope: minimal fixture and feature wiring only; no template run, no wiki, no commit/push/dispatch.

- **Isolated graph.** Cargo cannot scope `[patch.crates-io]` per feature, so
  `support/blitz-v8-probe/netrender-window/Cargo.toml` compiles the same probe sources
  (`[[bin]] path = "../src/main.rs"`) with feature `netrender` and its own `Cargo.lock`. Its
  patches: the probe's vendored Blitz and the proof's `patched/wgpu{,-core}`
  (`prepare-patched.sh`: crates.io + interop patch). Lock seeded from the probe's: 16 packages
  added, no version changed, one wgpu-core 29.0.1 / wgpu-hal 29.0.4 / wgpu-types 29.0.1,
  Netrender `d2f13ca5`, Vello 0.9.0, one peniko 0.6.1. The default probe manifest only gained
  a `check-cfg` lint for `feature = "netrender"`.
- **Published-branch blockers (exact).** wgpu: no 29.x branch or release has a core interop API;
  the upstream seam (#10211, `from_core`/`as_core` on plain `Arc`s) is only on `trunk` and
  `firefox-161`, unreleased (expected wgpu 31, and deno_webgpu 0.227 pins wgpu-core `=29.0.1`).
  `BlackRam-oss/wgpu codex/core-interop-seam` is a local trunk-based test branch and is not used.
  Netrender: the fix branch `codex/fix-external-texture-order` (`5a73b3c2`) descends from
  `main` on wgpu 30, so it cannot share deno_webgpu's wgpu-core; `d2f13ca5` plus the proof's
  order patch (CI `--config` path override) is used instead.
- **Bridge reuse.** The proof's `src/lib.rs` now exports `adopt_device`/`adopt_texture`
  (behaviour unchanged for its producers); the probe depends on it by path. Ids:
  `op_blitz_present_device` clones adapter/device/queue ids while the page's `GPUDevice` and
  the `GPUAdapter` that created it (dom.js `sharedAdapter`, kept alive with the device) are
  borrowed; `op_blitz_present_canvas` clones a texture id next to the existing owned view.
- **Renderer** (`src/netrender_window.rs`, `NetrenderWindowRenderer`): translates the recorded
  Blitz scene (solid axis-aligned rects, rectangular clips, group opacity over one draw,
  canvases as `ExternalTextureComposite` at their scene-op boundary; `alphaMode: "opaque"`
  canvases get a black underlay, equal to forcing alpha 1); renders with
  `render_with_compositor_and_external_textures` (master `Rgba8Unorm`, storage-capable without
  device features), blits the master to the surface (`Bgra8Unorm` or `Rgba8Unorm`, non-sRGB)
  with `wgpu::util::TextureBlitter`, presents. Anything else, an error on the embedder's
  error sink, a lost/validation surface or a Netrender panic (`catch_unwind`) switches once to
  the existing `GpuWindowRenderer`, which re-renders that frame from the same recording.
  Resize reconfigures the surface (frames are skipped until the configured size matches);
  `Outdated` reconfigures; `Timeout`/`Occluded` skip. Resource drop: canvases are read from
  `Presentation` each frame (pruned first), the adopted texture lives one frame, dropping a
  `CanvasSource` drops only the embedder's id. Device loss: adopted device has its own error
  sink (patch), no device-lost callback is installed, the page's handlers are untouched.
- **Chromium comparison** (f2700724, `components/viz/service/display/display_resource_provider.h`):
  Viz reads child resources only inside `ScopedReadLockSharedImage`, which keeps the shared
  image alive for the draw and carries `gpu::SyncToken`s ordering the display compositor after
  the producer. Roves keeps the scoped-read idea (a texture id cloned and adopted for one frame)
  but needs no interprocess or cross-context token: producer and consumer are one process,
  one wgpu-core device and one queue, the page's submissions precede the compositor's on that
  queue, and wgpu-core's trackers insert the barriers. The page keeps device ownership.
- **Fixture** `support/blitz-v8-probe/fixtures/netrender-window/index.html` (a minimal
  `--game` page): black page, opaque red `#before`, a 160x120 WebGPU canvas cleared to 50%
  premultiplied blue, 50% white `#after` over canvas, `#before` and page. Six pixels
  (`FIXTURE_CHECKS`); readback of the final output only, after 30 Netrender frames. Expected
  negative (published Netrender): exactly the three `#after` pixels fail (double blending).
- **Evidence (local, light).** `cargo fmt --check` (probe, proof); `cargo check --locked
  --bins --tests` clean for the default probe, the isolated manifest (shared target dir, only
  pre-existing warnings) and `cargo check --locked --features deno --bins` for the proof;
  `check_log.py` accepts/rejects synthetic negative/positive logs as intended; workflow YAML
  parses. **Not run:** unit tests (`netrender_window::` translation of the real fixture page
  painted by blitz-paint, untranslated constructs), any window, any pixel.
- **CI-ready plan** `.github/workflows/netrender-window.yml` (isolated, informational; paths:
  the manifest dir, `src/netrender_window.rs`, the fixture, the proof's lib/script/patches,
  the workflow): `windows-test` builds the opt-in executable and runs
  `cargo test -- netrender_window:: presentation::` (no window: WARP crashes Vello);
  `linux-lavapipe-window` (first Linux build of the probe: Mesa lavapipe pinned via
  `VK_DRIVER_FILES`, Xvfb, ALSA/fontconfig/xkbcommon/wayland dev packages) runs the fixture
  negative (`--locked`) and positive (order fix by path override, lock diff checked and
  restored) with `check_log.py`. Shared-source changes (`main.rs`, `presentation.rs`,
  `dom.js`) are covered by `blitz-probe.yml` for the default build.
- **First CI (run 43850232485, commit `9b32a08e52f`) and fix (2026-11-16, uncommitted).**
  Linux job 133240562560: negative and positive both fell back at frame 0 with
  `non-rectangular or transformed fill`, so no `netrender_window_fixture` record; Windows job
  133240562980: `fixture_scene_translates_in_paint_order` panicked on the same `Err`. Exact
  command, from a local dump of the fixture painted by blitz-paint (temporary example on the
  default probe's cache, deleted): the second `Fill`, the black background of `html`, whose box
  is 640x0 because every child is absolutely positioned, recorded with identity transform as
  `MoveTo(0,0) LineTo(640,0) LineTo(640,0) LineTo(0,0)` (blitz-paint's `CssBox::shape`: one
  point per corner, no `ClosePath`). The edge test rejected the zero-length edge. The other
  fills are the root background (kurbo `Rect`: 4 points + `ClosePath`, accepted) and the
  `#before`/`#after` boxes (4 points, no `ClosePath`, accepted). Fix in `axis_aligned_rect`:
  consecutive repeated points and the return to the start are dropped; `ClosePath` only as
  the last element (one subpath); with non-degenerate bounds the polygon must still be exactly
  the four distinct bounds corners joined by axis-aligned edges (a repeated corner such as
  TL, TR, BR, TR is rejected); with zero-width or zero-height bounds the straight-line polygon
  has no area, so it is translated as the empty rectangle (the fill paints nothing, a clip
  hides everything; a zero-area canvas box is skipped). Transforms, curves, several subpaths,
  triangles, L shapes and bow ties stay rejected. Pixel oracles (`FIXTURE_CHECKS`, tolerance
  2), `check_log.py` gates and the fallback switch are unchanged. New unit tests:
  `blitz_rect_encodings_are_accepted`, `non_rectangular_polygons_are_rejected`. Local evidence
  (light): `rustfmt --check`; the new function copied into a temporary example of the default
  probe passed every accept/reject case (deleted). **Not run locally:** the `netrender` feature
  build and its unit tests, any window or pixel (CI).
- **Next:** push with the next allowed batch and read both jobs (expect Linux system-package
  iterations); then resize in a resizable window, an injected device loss
  (`GPUDevice.destroy()` mid-run must produce one fallback and no panic), translation of text,
  paths and images, and the unmodified template.

### Sixth assignment: surface lifecycle of the opt-in renderer (2026-11-16, uncommitted, not pushed)

Input: CI 43865296074 (`05459415ddb`) passed after the empty CSS box fix; standard Blitz CI
43865296192 was still running. Scope: opt-in Netrender only; no new dependency; the default probe
renderer, original pixel oracles and negative/positive gates are unchanged.

- **Scenarios.** `ROVES_PROBE_NETRENDER_SCENARIO=resize|device-destroy` (only with
  `ROVES_PROBE_NETRENDER_FIXTURE=1`) starts once, right after the six `FIXTURE_CHECKS` pixels are
  read back (first verified frame), on separate runs of the positive executable. The renderer sets
  a request in `window::WindowScenario` (shared `Rc`), and the window run carries it out.
- **Native resize through existing facilities.** `window::ScenarioApplication` wraps blitz-shell's
  unchanged `BlitzApplication`: every winit `ApplicationHandler` method is forwarded, then a pending
  resize calls `Window::request_surface_size(LogicalSize 800x450)` on the shell's window (`View.window`
  is public). X11 without a window manager (Xvfb) applies it with `XConfigureWindow`, also for a
  non-resizable window (winit moves the min/max hints first). The resulting `SurfaceResized` goes
  through blitz-shell exactly as a user resize: viewport, `set_size`, redraw. The renderer
  reconfigures at once and skips frames until the configured size matches. After ten frames at
  the new size, `netrender_window_resize` reads back eight pixels. The page clears its canvas to
  50% green once `innerWidth` changes, so a stale blue texture fails both canvas pixels. One pixel
  checks the added area (black page, Netrender clears to white). One near `#before`'s right edge
  would sample `#after` if the 640x360 frame were stretched. The CSS-to-device scale is now fixed
  at the first resume (the same value as before for the original read back).
- **Guest `GPUDevice.destroy()`.** The page defines `netrenderFixtureDestroyDevice()` (destroys its
  own device and stops drawing) and its own `device.lost` handler. `RovesDocument` invokes the
  function once at the next frame step. `op_blitz_present_device` also clones the page device's
  deno_webgpu `ErrorHandler` (`Rc`, public), and the renderer reads its public `is_lost` before
  each frame. `destroy()` sets it synchronously on this thread, so there is no race with
  rendering. Errors are not used to detect the loss, because wgpu 29 drops `DeviceLost` errors
  (`handle_error_inner`). No device-lost callback is installed: wgpu-core's slot is unused,
  deno_webgpu resolves `lost` from its own handler, and promise reactions are additive, so the
  page's handler is preserved.
- **Semantics on a lost device.** `GpuWindowRenderer` presents on the same device, so falling back
  to it cannot work. Its `configure` would hit `panic!("window surface configuration failed")`, or
  every frame would hit an acquire error; this was read in code, not run. Instead the window stops
  once (`netrender_window_device_lost`) and drops the `Gpu` (surface, Netrender, adopted ids), or
  suspends the fallback's surface if already in fallback. `is_active()` is then false, so
  blitz-shell stops requesting redraws. `render`, `set_size` and `resume` do no GPU work, and
  `complete_resume` returns `true` because blitz-shell `debug_assert`s it. JS keeps running. Close
  (frame limit or user) and the WebGPU teardown are unchanged: `device_poll` has no validity
  check, and the second `destroy()` is a no-op.
- **Chromium f2700724.** Read in `components/viz/service/display/`: `display_resource_provider.h`,
  `direct_renderer.cc` and `display.cc`. In `display_resource_provider.h`,
  `ScopedReadLockSharedImage` keeps a child resource alive for one draw.
  `DeclareUsedResourcesFromChild` returns the resources that are no longer used to the child.
  Roves adapts this as an adopted texture id per frame, dropped after submission; `Presentation`
  keeps the page's ids until the canvas is replaced, forgotten or torn down. The return path has
  no interprocess token: one device, one queue. In `direct_renderer.cc`, `DrawFrame` reshapes the
  output surface lazily, only when about to draw ("otherwise the reshape can leave the window at
  the wrong size"), then forces a full redraw. In `display.cc`, `Resize` only records
  `current_surface_size_`, and `DrawAndSwap` skips frames whose root pass size does not match.
  Roves reconfigures eagerly in `set_size`; frames are skipped until the configured size matches,
  and Netrender always redraws the full frame. The outcome is the same: no stretched or partially
  sized frame. `Display::OnContextLost` lets the client recreate the output surface on a new
  context. A page's WebGPU `destroy()` does not reach Viz, whose context is separate, so Chrome
  keeps compositing the page. Roves shares the page device with the compositor and the page owns
  it, so it cannot recreate one: the controlled stop is the adaptation, and recovery is a TODO.
- **Gates.** `check_log.py resize|device-destroy`. Both first pass every positive gate (six pixels,
  `gpu_window_presentation`, `gpu_teardown`, exit 0). `resize` also requires one resize request,
  one page `resized` event, one passed `netrender_window_resize` larger in both directions, and no
  fallback. `device-destroy` also requires one page call, exactly one stop on Netrender after the
  verified frame, the page's own `lost` handler with `destroyed`, no `panicked at` and a
  frame-limit close. Plain negative/positive runs must not contain scenario records. Twenty
  synthetic logs were accepted/rejected as intended. New headless unit tests:
  `scenarios_parse_only_known_names`, `resize_checks_are_discriminating`. Workflow: two steps after
  the positive run, with `--window-frames 180`; `src/window.rs` was added to the trigger paths.
- **Evidence (local, light).** `cargo fmt --check` (probe) passed. `cargo check --locked --bins
  --tests` is clean for the default probe and for the isolated manifest (shared target dir, only
  pre-existing warnings; the one new dead-code warning in the default build is silenced by
  `cfg_attr`). The synthetic gates passed. **Not run:** the opt-in unit tests, any window or
  pixel, and both scenarios (CI).
- **Next:** push with the next allowed batch and read `linux-lavapipe-window` (the resize timing,
  the drop on a destroyed device, the teardown on it); then translation of text, paths and images.

### Seventh assignment: solid-color glyph runs (2026-11-16, uncommitted, not pushed)

Scope: opt-in Netrender only, while standard Blitz CI was running; no new dependency, no
template edit, the default probe renderer and the six-pixel, resize and device-loss gates
unchanged.

- **AnyRender semantics (read in anyrender 0.14.0, vello 0.9.0, anyrender_vello_cpu 0.18.0,
  vendored blitz-paint `src/text.rs`).** `GlyphRunCommand` = `font_data` (`peniko::FontData`:
  `Blob<u8>` + collection index), `font_size` (px per em in run space), `hint`,
  `normalized_coords` (i16 F2Dot14), `embolden`, `style`, `brush`, `brush_alpha`, `transform`
  (run space -> device), `glyph_transform` (per-glyph, before positioning) and positioned glyphs.
  blitz-paint sends `Fill::NonZero`, a solid brush, `brush_alpha` 1, a skew `glyph_transform` only
  for synthetic oblique, `embolden` only with the `font-embolden` features, and `hint = true`
  otherwise. The fallback (vello_cpu) honors `hint`, but not `brush_alpha`. Vello GPU hints only
  vertically, and only under uniform scale plus translation.
- **Netrender d2f13ca5 API.** `Scene::push_glyph_run_full(font_id, size, glyphs, color, transform_id,
  clip_rect, clip_corner_radii)`, `push_font`/`FontRegistry::intern` (key `(Blob::id(), index)`),
  `push_transform`. The emitter calls `vscene.draw_glyphs(font).font_size().transform().brush()`
  and adds `normalized_coords` only from user-space axis values. Colors of `SceneRect` and
  `SceneGlyphRun` are **premultiplied** (`unpremultiply_color`). There is no field for hint,
  glyph transform, embolden, style, or normalized coordinates.
- **Translation (`translate_glyph_run`).** Accepted: `Fill::NonZero`, a solid brush (brush alpha
  folded), identity or absent `glyph_transform`, zero `embolden`, all-zero or empty coordinates,
  a finite transform and size, group alpha 1. The clip stack's device rect is `clip_rect`
  (`NO_CLIP` when it is the viewport). A fully clipped or empty run paints nothing. The transform
  is the inverse of Netrender's `transform_to_affine`. `hint` is dropped, so only AA edges can
  differ from the fallback. Each other field returns a named reason, which triggers the existing
  single switch to `GpuWindowRenderer`. Text under group opacity falls back because Vello fills
  glyphs separately, so a folded alpha would double-blend overlaps. Rect colors are now
  premultiplied too: the six pixels were right only because unpremultiply clamped (#after `[1,1,1,0.5]` -> `[2,2,2]`).
- **Font identity and lifetimes.** The plan keeps the recorded `FontData`; the scene gets
  `FontBlob { data: blob.clone(), index }` (an `Arc` bump, same `Blob::id()`), interned per frame
  with `FontRegistry`, so Vello's glyph cache keys on the same id across frames. No bytes are read,
  copied or re-uploaded by Roves. The bytes live while parley's collection, a recording or a
  Netrender scene holds the `Blob`.
- **Chromium f2700724** (`cc/paint/paint_op.h` `DrawTextBlobOp`, `paint_op.cc`
  `DrawTextBlobOp::RasterWithFlags`, `paint_op_buffer_serializer.cc`, `paint_op_writer.cc`):
  shaped text is a ref-counted `SkTextBlob` (glyphs, positions, `SkFont` with a ref-counted
  `SkTypeface`) plus `PaintFlags`, drawn under the canvas's current matrix and clip. For OOP
  raster, `DrawTextBlobOp::Serialize` is `NOTREACHED`: text goes through slugs (`DrawTextSlugsOp`),
  and glyph data goes once per typeface through `SkStrikeServer` (text ops are skipped without
  it). Roves translates the same way, as shaped run plus paint plus matrix plus clip. It needs no
  strike transfer: one process and one device, and the `Blob` id plays the role of the typeface
  unique id for the glyph cache. Difference: Skia's `SkFont` carries hinting and subpixel flags;
  Netrender drops hinting (above).
- **Text check (scenario `text`).** The fixture page has a hidden `#text` (`class="hidden"`, two
  U+2588 FULL BLOCK glyphs, opaque yellow, 48px `"DejaVu Sans Mono", monospace`, at 130,96, over
  the canvas and `#before`, left of `#after`). It cannot change the six original pixels: the unit
  test asserts no glyph run while hidden. After the verified frame the renderer calls
  `netrenderFixtureShowText()`. Once 10 presented frames contained a glyph run,
  `netrender_window_text` reads back the final output. It checks four pixels derived from the
  translated run itself, not from font metrics. Two are cell centers (origin + advance/2) at
  0.3 em above the baseline, where FULL BLOCK spans descender to ascender: yellow. Two controls,
  0.15 em left of the cell and 0.5 em below the baseline, show the canvas over `#before`
  (`[128,0,128]`), so no text box was painted. Each point must lie inside the canvas box and left
  of `#after`, otherwise the record fails with the reason; glyph ids are reported and `.notdef` is
  rejected. `check_log.py text`: positive gates, one page call, one page `text` event, a passed
  record with 4 pixels, `glyph_runs > 0`, no fallback, no panic. Every kind now rejects other
  scenarios' records. Workflow: `fonts-dejavu-core` added to the apt list, `fc-match` logged, and
  a new step after `device-destroy`.
- **Evidence (local, light).** `cargo fmt --check` (probe). `cargo check --locked --bins --tests`
  is clean for the default probe and the isolated manifest (pre-existing warnings only).
  `check_log.py` passed 17 synthetic logs, each accepted or rejected as intended. A temporary
  example on the default probe's cache, deleted afterwards, painted the fixture with blitz-paint.
  Hidden: 4 commands, no glyph run. Shown: one run, with font blob 340712 bytes (Windows monospace
  fallback), glyph id 2527 twice, `hint` true, no coordinates, zero embolden, `Fill(NonZero)`,
  solid `[1,1,0,1]`, brush alpha 1, transform `translate(130,96)`, glyphs at x 0 and 28.9, y 41.
  Derived pixels: (144.4,122.6), (173.3,122.6), (122.8,122.6) and (144.4,161.0), all inside the
  canvas and left of 200. **Not run:** the opt-in unit tests (new: `text_fixture_translates_over_the_canvas`,
  `glyph_runs_keep_font_identity_transform_and_clip`, `unsupported_glyph_runs_are_named`,
  `netrender_transform_and_color_encoding`, `text_checks_reject_pixels_outside_the_canvas`), any
  window or pixel.
- **Next:** with the next allowed push, read the `text` step (`fc-match`, glyph ids, the four
  pixels) and the Windows unit tests (monospace fallback). Then text under group opacity through
  Netrender layers, paths and images. Propose upstream the missing glyph fields (hint,
  glyph transform/embolden, normalized coordinates).

## Follow-up assignments after the proof

1. Translate Blitz paint commands directly where practical, reusing Netrender's
   scene and resource API. Map clips, transforms, fonts, alpha, formats and blur.
   Verify backdrop blur with Pixi underneath. Keep the game template unchanged.
2. Test tile reuse when a dialogue letter changes and when only transform or
   opacity changes. Verify resize, restore, close, and the CPU fallback. Replace
   split_scene, RasterCache and composition glue only where parity is established.
3. Compare release builds on GTX 1050 with menu, at least three dialogues and Skip.
   Repeat three times per variant. Collect PresentMon by scene, P95/P99 displayed
   intervals, raster per miss and CPU/GPU usage. Check normal/JITless in CI.
   Dialogues also need the separate Deno/Blitz timing split for work between renders.
