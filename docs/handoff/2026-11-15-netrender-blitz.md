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
