# Netrender shared-device proof (opt-in, not shipped)

Standalone fixture for the first Netrender assignment of
`docs/handoff/2026-11-15-netrender-blitz.md`: can Netrender composite a WebGPU canvas
texture on the device the game created through deno_webgpu, with one wgpu-core
`Global`, one device and one queue, and no copy or readback of the canvas?

Nothing here is part of the Roves workspace, the Blitz probe or any release. The
existing probe renderer (`split_scene` + Vello CPU + `GpuCompositor`) is unchanged.

## Pins

| Component | Pin | Why | Upgrade trigger |
|---|---|---|---|
| wgpu-core | `=29.0.1` | exact pin of deno_webgpu 0.227.0 (deno_core 0.413) | a deno_webgpu paired with our deno_core on a newer wgpu-core |
| wgpu | `=29.0.4` | latest 29.x, requires wgpu-core `^29.0.1`, unifies with the pin above | wgpu release with the interop API below |
| netrender | git `d2f13ca5` (2026-08-13) | last revision on wgpu 29 / Vello 0.9; later ones use wgpu 30 (a second wgpu-core, objects not shareable) | Netrender on deno_webgpu's wgpu-core major, or deno_webgpu on Netrender's |
| vello | 0.9.0 (through netrender) | | as netrender |

Toolchain: stable 1.95.0 (local), no `rust-toolchain.toml` at `d2f13ca5`.
`cargo tree`: one `wgpu-core 29.0.1`, one `wgpu-hal 29.0.4`, one `wgpu-types 29.0.1`.

## Blocker 1: no public bridge in wgpu 29 (nor 30)

Netrender takes `WgpuHandles { wgpu::Instance, Adapter, Device, Queue }` and an
external texture as `&wgpu::TextureView`. deno_webgpu exposes only wgpu-core:
`Instance = Arc<wgpu_core::global::Global>` plus `DeviceId`/`QueueId`/`AdapterId`/`TextureId`
(`GPUDevice.instance/id/queue/adapter`, `GPUTexture.id`).

- `wgpu::Instance::from_core` takes a wgpu-core `Instance` **by value** and builds a new
  `Global`: it cannot share deno's registry.
- `ContextWgpuCore(Arc<Global>)`, `CoreDevice`/`CoreQueue`/`CoreTexture` fields are private;
  there is no `Device`/`Queue`/`Texture` constructor from core ids.
- wgpu-core ids are single-owner (`device_drop` removes the id): even with a constructor,
  wgpu and deno would double-drop one id. `Hub`/`Registry::prepare` are `pub(crate)`.
- `create_device_from_hal` / `create_texture_from_hal` create a *second* wgpu-core device
  with its own trackers and fences on the same native device: rejected (second device,
  unsound resource-state tracking).
- `Device::from_custom` (feature `custom`) would mean re-implementing the 29 dispatch
  interfaces that `wgpu/src/backend/wgpu_core.rs` (4,014 lines) already implements:
  rejected (duplicate of maintained code).

Compile repro: `PRISTINE=1 ./prepare-patched.sh && cargo build` fails with five `E0599`
(`adapter_clone_id`, `device_clone_id`, `queue_clone_id`, `texture_clone_id`,
`wgpu::Instance::from_core_global` not found).

### Narrowest upstream change: `upstream/wgpu-29-core-interop.patch` (+153 lines)

- wgpu-core: `Global::{adapter,device,queue,texture}_clone_id(id) -> id` register a new id
  for the same `Arc`, so each owner drops its own id in any order.
- wgpu (`cfg(wgpu_core)`): `Instance::from_core_global(Arc<Global>)` and
  `Instance::{adapter_from_core, device_from_core, texture_from_core}` adopting owned ids.
  The adopted device gets its own error sink; the embedder must not replace the
  device-lost callback deno_webgpu installed (Netrender never sets one).

## Blocker 2: Netrender interleaving paints later ops twice

`Renderer::render_with_compositor_and_external_textures` renders the **whole** scene into
the master, composites the external texture, then redraws the tail above it. Every
translucent op after the boundary is therefore blended twice (also outside the canvas),
and content meant to be above a translucent canvas also shows through it. Same code at
Netrender HEAD (`scene_tail_fragment`). Fix proposed in
`upstream/netrender-d2f13ca5-external-texture-order.patch` (+41/-5): the master renders only
`ops[..first boundary]`, each external texture is followed by `ops[b_i..b_{i+1}]`. The CSS
filter preprocessing path keeps the old behavior (Netrender already marks filters plus
interleaved externals as a follow-up).

## Fixture

`src/lib.rs` also exports the bridge itself, `adopt_device` (wrap the shared `Global`, adopt
embedder-owned adapter/device/queue ids) and `adopt_texture`, so the Blitz probe's opt-in
Netrender window renderer (`support/blitz-v8-probe/netrender-window`) reuses it instead of a
copy; its CI also reuses `prepare-patched.sh`, `ci/cargo-retry.sh` and `ci/verify.py lock-diff`.

Shared part in `src/lib.rs` (adoption, scene, pixel assertions, hub id counts); the WGSL
canvas shader is `src/canvas.wgsl`. Two producers feed it.

### Raw wgpu-core producer (`src/main.rs`, default binary)

The "Deno side" repeats deno_webgpu 0.227.0's wgpu-core calls (`Global::new` descriptor,
`request_adapter`, `adapter_request_device` with WebGPU default limits and no features)
and draws a 16x16 canvas texture on the GPU (`RENDER_ATTACHMENT | TEXTURE_BINDING`,
**no `COPY_SRC`**: any copy would be a validation error; top half opaque green, bottom
half 50% premultiplied blue), submitted without waiting. The embedder clones the ids,
adopts them in wgpu, then Deno drops its own ids (the page's GC) before Netrender
renders. Netrender gets the game device as-is (features and limits asserted equal).
Scene: opaque red before the canvas, canvas at boundary 1 (`SourceAlpha::Premultiplied`),
50% white after it. Only the final master is read back for six pixel assertions; the hub
report asserts 2 device ids, then 1, and no new device or queue during Netrender's frame.

### V8 producer (`src/bin/deno_js.rs`, feature `deno`)

A real deno_core 0.413.0 `JsRuntime` with deno_webidl 0.260.0, deno_web 0.291.0 and
deno_webgpu 0.227.0 (the Blitz probe's releases; v8 150.4.0). deno_webgpu's own
`wgpu-core =29.0.1` resolves to the same patched crate through this fixture's
`[patch.crates-io]`, so `cargo tree` still shows one wgpu-core/hal/types. Nothing outside
this directory sees the patch: the Blitz probe graph and the production renderer are
unchanged, and no fork is published.

1. `src/deno_bootstrap.js` (privileged, before guest code) loads
   `ext:deno_webgpu/01_webgpu.js` lazily as the probe's dom.js does, exposes only a frozen
   `navigator.gpu` and the WebGPU flag namespaces, captures `op_netrender_proof_adopt` and
   deletes `Deno` and `__bootstrap`. Its completion value is the embedder hook, held only
   as a Rust `v8::Global`.
2. `src/guest.js` (unprivileged, the "game") fails if any `Deno`/`__bootstrap`/`op_*`
   global is visible, then calls `requestAdapter()`, `requestDevice()`, `createTexture()`
   without `COPY_SRC`, compiles the shared WGSL (prepended as `const CANVAS_WGSL = <JSON
   string>`, encoded with deno_core's re-exported `serde_json`: the shader's comments
   contain backticks, so a template literal is not safe), records one
   render pass, submits without awaiting and checks validation/OOM error scopes. It
   resolves to `{ adapter, device, texture }` and keeps no other reference.
3. The embedder calls the hook with that frame. The op takes `#[cppgc] &GPUAdapter`,
   `&GPUDevice` and `&GPUTexture` (deno_core rejects anything else), checks the same
   `Global`, the device's adapter, the texture's device and queue, refuses `COPY_SRC`,
   records the hub ids (asserted: the
   guest made exactly one device and one queue) and registers the embedder's clone ids.
4. Every JS handle is released (Rust drops the hook and frame handles) and
   `low_memory_notification()` plus the event loop run up to 8 times until deno_webgpu's
   `Drop` impls have removed the guest's ids: the hub must go from guest+embedder ids to
   exactly one adapter/device/queue/texture id. `GPUQueue` is freed only after the
   `GPUDevice` that holds it as a `v8::Global`, hence possibly several passes.
5. Only then wgpu adopts the ids and Netrender renders, with the runtime alive as in the
   probe. Teardown: queue drained (`PollType::wait_indefinitely`), wgpu objects dropped
   (hub must be empty), runtime dropped; the test's `Arc<Global>` must be the last one.

## Audit (2026-11-15, from the pinned sources)

- **Clone safety.** In wgpu-core 29.0.1 `adapter_drop`, `device_drop`, `queue_drop` and
  `texture_drop` are a plain `hub.*.remove(id)`: no device-lost callback, no pending-write
  deactivation, no destruction. The resource dies with its last `Arc`, so the guest and
  the embedder drop their ids in any order. `Registry::get` panics on a vacant id; the
  clone happens inside the op while the cppgc objects are borrowed, so the guest ids are
  live. **Fixed before CI:** `GPUDevice` keeps only the adapter *id* (no handle to the
  `GPUAdapter`, whose finalizer drops it), so the first version, which cloned
  `device.adapter` with only device and texture in the frame, panicked
  (`Adapter[..] does not exist`) whenever V8 collected the guest's adapter between the
  guest's completion and the op. The frame now carries the adapter and the op borrows it
  (`adapter.id == device.adapter`). A product bridge has the same constraint: clone the
  adapter id while the game's `GPUAdapter` is alive, or an upstream API must expose the
  device's adapter. Clones have no trace record (`trace` feature only). Not per id, by design:
  `GPUTexture.destroy()` and `GPUDevice.destroy()` from the guest destroy the shared
  object for the embedder too, and wgpu's default uncaptured-error handler panics, so a
  product integration must install `Device::on_uncaptured_error` and treat destroy or
  loss as "canvas gone". deno_webgpu 0.227 installs no device-lost closure; the embedder
  must not install one either (single slot, `replace`).
- **Publishable APIs.** Besides the proposed wgpu/wgpu-core interop (+153 lines) the V8
  path uses only published API: public fields of `deno_webgpu::device::GPUDevice`
  (`instance`, `id`, `adapter`, `queue`) and `deno_webgpu::texture::GPUTexture`
  (`instance`, `id`, `device_id`, `queue_id`, `size`, `mip_level_count`, `sample_count`,
  `dimension`, `format`, `usage`), the public `From` conversions, and deno_core's
  `extension!`, `op2` cppgc arguments, `execute_script`, `resolve`, `call_with_args`,
  `with_event_loop_promise`, `v8_isolate().low_memory_notification()` and
  `OpState::try_take`. No private layout casts.
- **Lifetime and drop order with JS handles.** deno_webgpu frees ids only from cppgc
  finalizers (`Drop` of `GPUTexture`, `GPUDevice`, `GPUQueue`, `GPUAdapter`). `GPUDevice`
  holds its `GPUQueue` through a `v8::Global` released when the device is finalized; no
  `v8::Global` points back at the device (the error handler holds a `v8::Weak`), so no
  root cycle keeps it alive. The guest's ids are released by GC, not by script, and the
  embedder's ids keep the canvas valid. wgpu-core 29's `Queue` `Drop` waits only a few
  seconds for the last submission, so the last owner drains it first. Other guest objects
  (view, shader, pipeline, encoder, command buffer) only hold `Arc<Global>` clones and are
  dropped with the runtime, before the final `Arc::strong_count == 1` check. Residual
  risk, not observed yet: `low_memory_notification` is called from Rust with V8's
  default "stack may contain heap pointers" state, so a stale cppgc pointer on the native
  stack could keep a guest object alive across all 8 passes; the GC assertion then fails
  with a panic, which CI reports as a lifetime failure, never as a pixel result.
- **Guest surface.** No privileged bridge on the global (checked by the guest), one
  device (hub count in the op), no input copy or readback (`COPY_SRC` refused by the op
  and absent from the texture, so any copy is a validation error; Netrender only samples
  the view; only the final master is read back for the assertions).
- deno_webgpu's own `GPUCanvasContext` (OffscreenCanvas path, `canvas.rs`) adds `COPY_SRC`
  to its textures. The probe does not use it (dom.js has its own
  `OffscreenGPUCanvasContext` over `device.createTexture`) and this fixture does the same.

## Running

```sh
./prepare-patched.sh   # downloads wgpu 29.0.4 / wgpu-core 29.0.1, checks sha256, applies the patch
cargo run --bin roves-netrender-shared-device-proof   # Netrender d2f13ca5 as published: exit 1 (blocker 2)
cargo run --bin roves-netrender-shared-device-proof --config 'patch."https://github.com/merely-made/netrender".netrender.path="<netrender checkout with upstream/netrender-d2f13ca5-external-texture-order.patch>/netrender"'
cargo run --features deno --bin deno_js   # V8 + deno_webgpu producer; same --config override for 6/6
                                          # Windows: with RUSTFLAGS='-C target-feature=+crt-static' (see Windows link)
```

The `--config` path override rewrites the `netrender` entries of `Cargo.lock`. Restore
them afterwards (`cargo metadata --format-version 1 >/dev/null` without the override) and
never keep a lock that points at a local path.

## Evidence (2026-11-15, local, GTX 1050, driver 581.63, debug build)

| Run | Backend | Result |
|---|---|---|
| wgpu/wgpu-core pristine | - | compile error, 5x E0599 |
| patched wgpu, Netrender d2f13ca5 | Vulkan | bridge OK (2 -> 1 device ids, same features/limits); 4/6 pixels pass; `(50,10)` `[255,192,192]` instead of `[255,128,128]`, `(40,40)` `[192,160,223]` instead of `[192,128,192]` |
| patched wgpu + Netrender order fix | Vulkan | 6/6 pass, exit 0 |
| patched wgpu + Netrender order fix | DX12 (`DENO_WEBGPU_BACKEND=dx12`) | 6/6 pass, exit 0 |
| raw producer after the `src/lib.rs` refactor + order fix | Vulkan | 6/6 pass, exit 0; hub 1/1/1/1 ids before Netrender, no new device or queue during its frame |
| V8 producer (`--features deno`) | - | `cargo check --features deno --bins`: 0 errors, 0 warnings (also after the adapter fix and the WGSL encoding fix). **Not run yet**: the V8 build, link and run are in the CI job below |
| raw producer, WARP forced locally (temporary `force_fallback_adapter`, reverted) | DX12 WARP 10.0.26100.9818 | **exit 139**, see WARP below |
| pristine wgpu compile repro (2026-11-15, after the CI gate fix) | - | 5 lines `error[E0599]`, one per interop symbol, each naming it in backticks |

The `deno` feature only adds 208 locked packages; no previously locked version changed
(v8 150.4.0 and deno_core 0.413.0 as in the probe). The `PRISTINE=1` compile repro was not
re-run after the refactor (the same calls moved unchanged into `src/lib.rs`).

## WARP: Vello's compute frame crashes the driver

CI Netrender #4 (job 132956577307) linked both executables for the first time. Both raw
runs exited 139 right after the `device ids` and `adapter:` lines (WARP,
"Microsoft Basic Render Driver", `Dx12`, `Cpu`), before any pixel line. Reproduced
locally on WARP 10.0.26100.9818 (raw producer with `force_fallback_adapter` forced by a
temporary edit, reverted) and isolated with `lldb`:

- The canvas pass is not the cause: a `device_poll(wait_indefinitely)` right after its
  submission completes (temporary edit, reverted), then the run still dies at the same point.
- The faulting thread is a WARP worker: `0xC0000005`, access violation **writing**, in
  `d3d10warp.dll` (`orl %eax,(%rdx,%r8,4)`, an OR into a buffer, as in an emulated UAV
  atomic). At that moment the main thread is already inside
  `render_with_compositor_and_external_textures` → `compose_external_texture` → FXC
  compiling the external-texture pipeline, i.e. after `render_to_internal_master` submitted
  Vello's compute frame. The renderer itself was created (Vello pipelines compiled).
- Bridge excluded: a throwaway example (deleted) with a plain `wgpu::Instance` (DX12, FXC
  and the default compiler), `force_fallback_adapter`, a fresh device and
  `create_netrender_instance` + `render_vello` of one opaque rect also exits 139 at the
  first `device.poll`. So Netrender d2f13ca5 / Vello 0.9 compute on WARP crashes with no
  shared device, no deno_webgpu and no external texture involved.

Raw step logs need repository admin rights; the run's annotations do not. The V8
producer's panic was the template-literal guard (`src/bin/deno_js.rs`), fixed above; it
never reached the GPU. Upstream report: not filed yet (it is a WARP/Vello interaction,
the right tracker is not established; no web access in this session).

## Isolated CI (`.github/workflows/netrender-proof.yml`)

One matrix job `proof`, separate from the engine workflows and the Blitz probe workflow
(neither changed), so nothing else builds the patched wgpu. Triggers: a push touching
`support/netrender-shared-device-proof/**` or the workflow file (any branch), or
`workflow_dispatch`. Informational: it never gates releases. Rust 1.95.0, debug builds,
`CARGO_HOME` = `<runner>/roves-cargo`; the checks live in `ci/verify.py` (run it locally with
`python -I ci/verify.py ...`).

- **`linux-lavapipe`** (`ubuntu-24.04`): installs `mesa-vulkan-drivers` (lavapipe,
  llvmpipe Vulkan ICD), takes the single `lvp_icd[.x86_64].json` manifest from the
  package's own file list (job 132973982729 had no `lvp_icd.x86_64.json`, so the name is not
  hard-coded; zero or several matches fail with the Vulkan file listing annotated), pins
  the loader to it with `VK_DRIVER_FILES`/`VK_ICD_FILENAMES`, annotates the Mesa version and `vulkaninfo --summary`, runs with
  `DENO_WEBGPU_BACKEND=vulkan`. Runs every step below, both producers, negative and positive.
- **`windows-compile`** (`windows-latest`, `RUSTFLAGS=-C target-feature=+crt-static`, see
  Windows link): the same pins, repro, patched-tree, negative and positive builds (so the
  static-CRT `deno_js` link stays covered) and artifacts, but **no runs**: WARP, the
  runner's only adapter, crashes as described above.

1. **Pins and provenance.** `verify.py pins`: exact `Cargo.toml` pins, `[patch.crates-io]`
   limited to `patched/wgpu{,-core}`, exactly one locked version of wgpu 29.0.4,
   wgpu-core 29.0.1, wgpu-hal 29.0.4, wgpu-types 29.0.1, naga, vello 0.9.0, Netrender at
   `d2f13ca5` (git source), v8 150.4.0, deno_core 0.413.0, deno_web/webidl/webgpu, and no
   other path package. `verify.py crates-io`: the sha256 values in `prepare-patched.sh`
   equal the crates.io index `cksum`. Patch sha256 values are logged.
2. **Compile repro.** `PRISTINE=1 ./prepare-patched.sh` + `cargo check --locked` must fail
   only with `E0599`, and each of the five interop symbols must be named (in backticks) by
   an `E0599` diagnostic. The gate reads rustc's JSON diagnostics (`--message-format json
   --color never`: error code and plain message), not the human output, and re-emits the
   errors as annotations. Job 132956577307 annotated "0 x E0599" yet passed, because the
   former check only searched the symbols anywhere in the log; job 132973982981 then failed
   the stricter human-output gate although all five errors were present, because
   `dtolnay/rust-toolchain` exports `CARGO_TERM_COLOR=always` and every line began with an
   ANSI escape, so `^error\[` matched nothing. The gate is as strict as before.
   Step dependencies: a failed gate skips the patched tree and everything built on it (the
   tree would still be pristine); a failed negative build or negative run does not stop the
   positive steps, which require only the patched tree.
3. **Patched tree.** `verify.py patched-diff`: the patched sources differ from the
   pristine crates.io ones exactly in the three files the interop patch names.
4. **Negative** (Netrender as published, `--locked`; Linux): both executables must exit 1
   with no panic, the lavapipe adapter line (`llvmpipe`, `Vulkan`, `device_type: Cpu`), the
   shared-device hub line, the producer's lifetime lines (raw: 2 -> 1 device ids; V8:
   guest privilege check, hub ids, GC pass count) and exactly the two ordering pixels
   failing. Any other failure (build, device init, panic, crash or timeout exit, other
   pixels) fails the job; `verify.py log` then annotates the last 8 log lines.
5. **Positive**: Netrender fetched at the exact SHA, `git apply` of the order patch
   (numstat must be +41/-5), both executables built with the `--config` path override.
   `verify.py lock-diff` accepts only the Netrender git packages moving to the local path;
   the original `Cargo.lock` is then restored and `git diff --exit-code` checks it. On
   Linux both must exit 0 with 6/6 and the same lifetime lines. The teardown assertions
   (hub empty, last `Arc<Global>`) run before the exit code, so exit 0/1 without a panic
   proves them.
6. Artifacts `netrender-shared-device-proof-<job>`: every log, `negative/` and `positive/`
   executables and `NOTES.txt`. `deno_js` reads extension JS from
   `<CARGO_HOME>/registry/src` at run time (no startup snapshot), so mirror that path to
   run it elsewhere.

`verify.py log KIND PRODUCER ADAPTER LOG STATUS` takes `lavapipe` or `warp` (the latter
for local runs once WARP works). Software rasterizers may round differently: the
tolerance stays 2/255 and failures are reported, not tuned. Not yet known: whether Vello
0.9's compute frame runs correctly on the runner's lavapipe; the first Linux run decides,
and a lavapipe failure is reported as such, never relaxed.

Windows link (static CRT): run 43744319003 failed to link both `deno_js` builds under the
repository's `lld-link` with `undefined symbol: __declspec(dllimport) remove`, `_wassert`
and `_time64`, referenced by lcms2-sys 4.0.7 (deno_webgpu -> deno_image -> lcms2). The
prebuilt `rusty_v8.lib` uses the static CRT (`/DEFAULTLIB:libcmt.lib`), so the CRT startup
and its static `libucrt.lib` came from libcmt, while `cc` had compiled lcms2 with `/MD`
(imports from `ucrt.lib`). Naming `ucrt.lib` as well (a `build.rs` tried in the next run)
only moved the failure: both UCRT flavours then define `_invalid_parameter_noinfo` and
`_wctype` (duplicate symbols). Mixing the two CRTs is the defect, so the CI job now sets
`RUSTFLAGS=-C target-feature=+crt-static` for every build, as Deno builds on Windows:
`cc` 1.11 then compiles lcms2 with `-MT` and v8 150.4.0's `build.rs` links `libcpmt`
instead of `msvcprt`, matching the `/MT` V8 library. The `build.rs` workaround was removed;
no `/FORCE:MULTIPLE`, no global linker or repository flag. A local Windows build of
`deno_js` needs the same variable (PowerShell: `$env:RUSTFLAGS='-C target-feature=+crt-static'`).

## Upstream reports

- Vello/Netrender compute on WARP (access violation in `d3d10warp.dll`, see WARP): not
  filed; first find the right tracker (linebender/vello vs. Microsoft WARP) and search
  existing reports, with a minimal plain-wgpu reproduction.

- wgpu interop API: not opened yet (search existing gfx-rs/wgpu issues first).
- Netrender blocker 2: the GitHub integration's create-issue call for
  merely-made/netrender returned an integration **403** (2026-11-15); no issue was
  created. Retry with an account or token that can open issues
  there, then link it in `TODO.md`.
