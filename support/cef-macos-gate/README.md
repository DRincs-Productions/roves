# CEF accelerated OSR -> wgpu-weld -> wgpu + SDL3: macOS feasibility gate

Status (2026-11-25): **implemented, type-checked locally for `aarch64-apple-darwin`; never
compiled with CEF, never run.** Its CI (`.github/workflows/cef-macos-gate.yml`) has no
result yet. Nothing here claims that the macOS path works.

First non-Windows step of the CEF + SDL3 + wgpu player (`support/cef-netrender-probe`,
Windows only). It answers one question with the same maintained producer and renderer as
Windows: does CEF's **accelerated off-screen** frame reach a wgpu device that also presents
to an **SDL3** window on macOS, with **Chromium's sandbox on**? There is no CEF windowed
mode and no software (`OnPaint`) path. The Windows player, its lockfile and its workflow are
untouched.

## Pins (same graph as the Windows player)

| Component | Pin | Notes |
| --- | --- | --- |
| `welding` (merely-made/wgpu-weld) | git `e991684`, crate 0.15.0 | `default-features = false`, `wgpu-29` + `cef-runtime`. MPL-2.0 |
| `grafting` | git `d903cfa` | via the lockfile, as on Windows |
| `cef` / `cef-dll-sys` | `=154.5.0+154.0.34` | CEF 154.0.34, Chromium 154.0.8037.98; features `build-util` + `sandbox` |
| `wgpu` | `=29.0.4` | features `std`, `metal`, `wgsl` |
| `sdl3` | 0.20 (SDL 3.4.16) | `build-from-source-static` (feature `bundled-sdl`) + `raw-window-handle` |
| Rust | 1.99.1 (MSRV 1.97.1) | as the Windows workflow |

`Cargo.lock` was seeded from `support/cef-netrender-probe/Cargo.lock`; resolving this crate
only removed packages (axum, tokio extras, ab_glyph, ...) and added none, so every shared
package, macOS-only ones included (objc2, objc2-metal, objc2-io-surface, libloading), has the
Windows player's version. CI repeats the Windows graph check: one wgpu 29.0.4 for the gate,
welding and grafting; wgpu 30.0.2 only under `cef` (`accelerated_osr`, never called).

## macOS path and exact APIs (read in welding e991684 and cef 154.5.0)

- **CEF settings** (`welding/src/macos_cef/mod.rs` ~289, `runtime.rs` ~371):
  `windowless_rendering_enabled = 1`, `shared_texture_enabled = 1`,
  `windowless_frame_rate = 60`; `external_message_pump = 1` and
  `multi_threaded_message_loop = 0` off Windows, so the host calls `CefDoMessageLoopWork`
  (`CefRuntime::do_message_loop_work`) from its own loop.
- **Producer**: `CefRenderHandler::OnAcceleratedPaint`;
  `CefAcceleratedPaintInfo::shared_texture_handle` is an `IOSurfaceRef`. welding `CFRetain`s
  it inside the callback (`macos_cef/cef_backed.rs` ~82) and hands it out after the callback
  returns (no copy, `SyncMechanism::None`).
- **Import** (`native_frame/metal.rs`): `MTLTextureDescriptor` (`BGRA8Unorm`/`RGBA8Unorm`,
  `MTLStorageMode::Shared`, `MTLTextureUsage::ShaderRead`) ->
  `MTLDevice newTextureWithDescriptor:iosurface:plane:0` on the wgpu device's own
  `MTLDevice` -> `grafting::import_metal_texture_ref` -> `wgpu::Texture`
  (`TEXTURE_BINDING | COPY_SRC`) on the wgpu Metal HAL. The host device must be Metal
  (`InteropBackend::Metal`).
- **Window**: SDL3 `NSWindow`/`NSView` through `raw-window-handle` ->
  `wgpu::Instance::create_surface_unsafe` (a `CAMetalLayer` surface) on the same wgpu device.
  The gate presents the imported texture with `wgpu::util::TextureBlitter` (wgpu's own
  utility; no shader of ours).
- **Processes and sandbox**: macOS CEF never re-executes the main binary; it launches the
  five `Helper*.app` bundles. `cef::build_util::mac::bundle` builds the `.app`, copies
  `Chromium Embedded Framework.framework` and stamps `roves-cef-macos-gate-helper` into every
  helper. With `CefSandboxMode::Sandboxed` the browser passes `no_sandbox = 0`, and each
  helper loads `Chromium Embedded Framework.framework/Libraries/libcef_sandbox.dylib`
  through welding (`runtime.rs` `MacSandbox::initialize`) before entering CEF. There is no
  unsandboxed switch in the gate.
- **Order**: CEF initialises first, then SDL3 (the order of wgpu-weld's macOS demo).

## Chromium reference (pinned `f2700724f59f`, sparse paths added for this gate)

- `content/browser/child_process_launcher_helper_mac.cc:118-178`: unless `--no-sandbox` or an
  unsandboxed sandbox type applies, the launcher compiles the seatbelt policy and appends
  `--seatbelt-client=<fd>` to the child's command line (`sandbox/mac/seatbelt_exec.cc:79`).
  Hence the gate's sandbox check: every renderer helper of this bundle must carry
  `--seatbelt-client=` and none `--no-sandbox` (command lines read with `ps` while the
  browser is alive). GPU/utility helpers are only reported.
- `base/message_loop/message_pump_apple.mm:905-945`: if `NSApp` conforms to `CrAppProtocol`
  Chromium uses `MessagePumpCrApplication`; otherwise it falls back to
  `[NSApplication sharedApplication]`/`MessagePumpNSApplication`, and
  `IsHandlingSendEvent` only `DCHECK`s the protocol. Neither SDL3's `NSApplication` nor a
  plain one implements `CrAppProtocol`; this is the same situation as wgpu-weld's winit
  demo. Consequence to keep in mind, not tested here: `CefDoMessageLoopWork` drains the
  `NSApplication` event queue itself, so SDL may not see every `NSEvent` (wgpu-weld's macOS
  demo README reports less complete input forwarding for winit).
- `ui/gfx/mac/io_surface.cc:99`: `kCVPixelFormatType_32BGRA` <-> `kBGRA_8888`, which with the
  capturer's BGRA request (Windows README, `frame_sink_video_capturer_impl.cc`) gives the
  gate's `Bgra8Unorm` expectation.
- `gpu/command_buffer/service/shared_image/iosurface_image_backing.mm:1343`
  (`WaitForCommandsToBeScheduled`): Chromium waits for another `MTLDevice`'s commands on an
  IOSurface to be scheduled before a different device reads it. welding imports with no
  synchronisation and keeps the surface after the callback while Viz's capturer recycles a
  pool of 11 frames. See "Open risks".

Difference from Chromium: Chromium composites and presents in its own GPU process; here the
frame leaves Chromium as an IOSurface and the host presents it with wgpu, as on Windows
(where welding copies it inside the callback instead).

## Verdicts

The main binary prints `gate:` lines on stdout and exits with:

| Exit | Meaning |
| --- | --- |
| 0 | An imported frame's centre 64x64 block matched dodger blue `[255,144,30,255]` BGRA ±8, and the renderer was sandboxed |
| 2 | `NO_ACCELERATED_FRAME`: no `OnAcceleratedPaint` before `ROVES_GATE_TIMEOUT_SECS` (default 60), nothing failed, renderer sandboxed |
| 1 | Any `gate: FAIL`: CEF/SDL/wgpu setup, non-Metal interop, `acquire_frame` or readback error, wgpu uncaptured error, content process terminated, frames imported but none matched, renderer missing or unsandboxed |

The fixture is a `data:` page (blue, plus a 1 px corner element animated with
`requestAnimationFrame` so CEF keeps painting). The uniform colour proves the frame's
content arrived, not ordering or tearing.

## CI (`cef-macos-gate.yml`)

`macos-15` (Apple silicon), push on this directory or the workflow, or manual. Steps: rustc
1.99.1 check and Ninja (`cef-dll-sys` uses CMake's Ninja generator); cargo/CEF cache;
dependency graph check; `cargo build --locked --release --bins`; `target/release/bundle`
and a structure check (main executable, framework, `libcef_sandbox.dylib`, five helpers);
one sandboxed run with a 150 s hard stop. The run step fails on any `gate: FAIL`, a missing
`cef initialized sandbox=Sandboxed`, Metal adapter, `mode=AcceleratedPaint`,
`PASS sandbox` or `shutdown complete exit=<code>` line, an exit other than 0/2, or a gate
process alive after exit. Exit 2 is a notice: GitHub's macOS runners are virtual machines
with a paravirtual GPU, and Chromium may disable GPU compositing there (as WARP does on
Windows). Artifacts: `cef-macos-gate-logs`, `cef-macos-gate-arm64` (zipped `.app`, unsigned
apart from the linker's ad-hoc signature). Never gates a release.

Local checks done on Windows (light only, as required):
`cargo check`/`cargo clippy --target aarch64-apple-darwin --no-default-features --features dox --bins`
(clean, 1.99.1), `cargo fmt --check`, YAML parse. They type-check the macOS code paths of
welding, objc2 and wgpu Metal; they do not build CEF or SDL, link or run anything.

## Open risks and blockers (not verified)

- **Accelerated frames on real Macs**: welding's README records IOSurface -> Metal receipts
  (macOS 15.7.7 Intel, macOS 26.5.1 M4) on CEF 151 and the wgpu-30 row; the CEF 154 +
  wgpu-29 row on macOS has no receipt. Only a real Mac can give exit 0 for certain; the
  user has none, so this stays open unless the CI runner delivers frames.
- **Software fallback is not available in the pinned welding.** `cpu-paint-fallback`
  declares `CefSurfaceMode::CpuPaint` and `native_frame::CpuBitmap` with an upload path, but
  no producer implements `CefRenderHandler::OnPaint` at e991684 (no `on_paint` in
  `welding/src`). If accelerated OSR proves unavailable on target Macs, the supported CEF
  route is `OnPaint` (BGRA CPU buffer, one `queue.write_texture` per dirty frame); the
  narrow upstream route is an issue/PR to wgpu-weld wiring `OnPaint` into the producers
  behind that feature. Not opened and not implemented: there is no evidence yet that it is
  needed.
- **IOSurface synchronisation**: zero-copy import with no fence while Chromium may recycle
  the surface (see Chromium reference). Possible tearing or stale content under load; the
  gate cannot detect it. If observed, compare with welding's Windows copy-in-callback model.
- **Shutdown**: welding's macOS producer has no close-completion signal (Windows has
  `is_closed`), so the gate pumps CEF for 1.5 s after `close()` before `cef::shutdown`.
- **Keychain**: the gate passes `--use-mock-keychain` (unattended runs would otherwise block
  on Chromium's Safe Storage prompt). The player needs a deliberate choice before
  distribution.
- **Distribution**: code signing, hardened-runtime entitlements for the helpers and
  notarisation are not addressed here.

## How Linux follows

Same producer/renderer family, so the gate shape transfers. welding e991684 has
`linux_cef`: `OnAcceleratedPaint` gives native-pixmap/DMABUF planes, imported through
grafting's Vulkan DMABUF path; the host must create its wgpu device with
`welding::build_dmabuf_capable_device` (external-memory extensions), present to the SDL3
window on the same Vulkan device, and pump CEF (`external_message_pump`). The binary is
re-executed for subprocesses (`execute_process_from`), and `CefSandboxMode::Sandboxed` uses
CEF's namespace/setuid sandbox selection. To check before claiming anything: user-namespace
restrictions (e.g. Ubuntu's AppArmor policy) versus a setuid `chrome-sandbox`, X11/Wayland
through SDL3, and whether a GitHub Linux runner (no GPU; lavapipe for Vulkan) delivers any
accelerated frame. Reference: wgpu-weld's `demo-weld-linux`. Next bounded task: a
`cfg(target_os = "linux")` twin of this gate (same verdicts; sandbox evidence from
`/proc/<pid>/status` and the helper command lines) in its own job.

## How Steam follows

The CEF player has no Steam integration yet on any platform (the engine's `--features steam`
build was removed with Servo on this branch). Steam is a variant of the same bundle: link
Steamworks in the main (browser) process and ship the redistributable next to it,
`libsteam_api.dylib` inside the `.app` (signed with it) on macOS and `libsteam_api.so` on
Linux. Points to verify, not assumed: the Steam overlay must hook the process that presents,
which here is the main process (wgpu Metal/Vulkan on the SDL3 window), not a CEF helper;
on Linux the Steam Linux Runtime container (pressure-vessel) may restrict the namespaces
Chromium's sandbox needs. Each platform keeps a standard and a Steam variant from the same
build inputs, as the Windows release matrix does.
