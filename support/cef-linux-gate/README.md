# CEF accelerated OSR -> wgpu-weld -> wgpu + SDL3: Linux feasibility gate

Status (2026-11-25): **implemented, type-checked locally for `x86_64-unknown-linux-gnu`;
never compiled with CEF/SDL, never run.** Its CI (`.github/workflows/cef-linux-gate.yml`)
has no result yet. Nothing here claims that the Linux path works, and no future CI result
from a hosted runner can claim it works on a GPU (see "GPU evidence versus CI").

Linux twin of `support/cef-macos-gate`, for the CEF + SDL3 + wgpu player
(`support/cef-netrender-probe`, Windows only). Same question with the same maintained
producer and renderer: does CEF's **accelerated off-screen** frame reach a wgpu device that
also presents to an **SDL3** window on Linux, with **Chromium's sandbox on**? There is no
CEF windowed mode and no software (`OnPaint`) path. The Windows player, the macOS gate,
their lockfiles and workflows are untouched.

## Pins (same graph as the Windows player and the macOS gate)

| Component | Pin | Notes |
| --- | --- | --- |
| `welding` (merely-made/wgpu-weld) | git `e991684`, crate 0.15.0 | `default-features = false`, `wgpu-29` + `cef-runtime`. MPL-2.0 |
| `grafting` | git `d903cfa` | via the lockfile |
| `cef` / `cef-dll-sys` | `=154.5.0+154.0.34` | CEF 154.0.34, Chromium 154.0.8037.98; features `build-util` + `sandbox` |
| `wgpu` | `=29.0.4` | features `std`, `vulkan`, `wgsl` |
| `sdl3` | 0.20 (SDL 3.4.16) | `build-from-source-static` (feature `bundled-sdl`) + `raw-window-handle` |
| Rust | 1.99.1 (MSRV 1.97.1) | as the other CEF workflows |

`Cargo.lock` is the macOS gate's lockfile (seeded from the Windows player's): resolving
this crate offline changed only the root package (name, no `semver`), so every package,
Linux-only ones included (ash, libc), has the same version. CI repeats the graph check: one
wgpu 29.0.4 for the gate, welding and grafting; wgpu 30.0.2 only under `cef`
(`accelerated_osr`, never called).

## Linux path and exact APIs (read in welding e991684, grafting d903cfa, cef 154.5.0)

- **CEF settings** (`welding/src/runtime.rs` `build_settings`, `linux_cef/mod.rs`
  `LinuxCefProducer::new`): `windowless_rendering_enabled = 1`, `shared_texture_enabled = 1`,
  `external_begin_frame_enabled = 0`, `windowless_frame_rate = 60`;
  `external_message_pump = 1`, `multi_threaded_message_loop = 0`, so the gate calls
  `CefDoMessageLoopWork` (`CefRuntime::do_message_loop_work`) from its own loop.
- **Processes**: Linux CEF re-executes the main binary for every subprocess, so
  `CefRuntime::execute_process_from(dir, CefSandboxMode::Sandboxed)` is the first CEF call
  of `main` and there is no helper executable. `libcef.so` is linked by `cef-dll-sys`;
  `build.rs` adds an `$ORIGIN` run path (see "Sandbox").
- **Producer**: `CefRenderHandler::OnAcceleratedPaint`; `CefAcceleratedPaintInfo` carries
  `plane_count` DMA-BUF planes (fd, offset, size, stride), a colour type and a DRM
  modifier. welding `dup(2)`s every fd inside the callback (`linux_cef/cef_backed.rs`), maps
  `CEF_COLOR_TYPE_RGBA_8888` to `Rgba8UnormSrgb` and everything else to `Bgra8UnormSrgb`,
  and stores a `NativeFrame::DmaBufImage`.
- **Host device**: `welding::build_dmabuf_capable_device(&adapter, &desc)` ->
  `grafting::vulkan_dmabuf::create_dmabuf_host_context`, which opens the wgpu Vulkan device
  with `VK_EXT_external_memory_dma_buf`, `VK_EXT_image_drm_format_modifier`,
  `VK_EXT_queue_family_foreign` and `VK_KHR_external_memory_fd` added, and fails if the
  physical device lacks any. The same device presents to the window.
- **Import** (`native_frame/vulkan_dmabuf.rs` `import_vulkan`): requires
  `InteropBackend::Vulkan`; `grafting::vulkan_dmabuf::import_dmabuf` with the explicit DRM
  modifier and `VulkanDmaBufQueueOwnership::Foreign` -> `wgpu::Texture`
  (`TEXTURE_BINDING | COPY_SRC`). **On the wgpu-29 row an implicit modifier
  (`DRM_FORMAT_MOD_INVALID`) is refused**: the linear fallback needs
  `Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF`, which exists only from wgpu 30. wgpu-weld
  records that CEF gives `DRM_FORMAT_MOD_INVALID` on AMD/RADV; Intel/Mesa gives explicit
  modifiers. The gate reports this case as `gate: FAIL IMPLICIT_MODIFIER`.
- **Window**: SDL3 with `SDL_VIDEO_DRIVER=x11`, Xlib handles through `raw-window-handle`
  -> `wgpu::Instance::create_surface_unsafe` (Vulkan X11 surface) on the same device. The
  gate presents with `wgpu::util::TextureBlitter` to an sRGB surface format (as wgpu-weld's
  Linux demo); no shader of ours. The surface asks for `AutoVsync`, but FIFO does not block
  under Xvfb + lavapipe (CI run 45612105775 presented ~2450 times per second), so the drive
  loop is also paced to one iteration per 16 ms; `gate: loop end` reports `present_rate_hz`.
- **Order**: CEF initialises first, then SDL3 (same as the macOS gate).

## Chromium reference (pinned `f2700724f59f`, sparse paths added for this gate)

- `content/browser/zygote_host/zygote_host_impl_linux.cc:103-158` (`ZygoteHostImpl::Init`):
  as root without `--no-sandbox` Chromium exits; otherwise layer 1 is the user-namespace
  sandbox when `Credentials::CanCreateProcessInNewUserNS()` succeeds, else the setuid
  sandbox when `chrome-sandbox` exists, else `LOG(FATAL) "No usable sandbox!"` (pointing to
  Ubuntu 23.10+'s AppArmor user-namespace restriction). So a running renderer already
  implies a layer 1; the gate still checks it per process.
- `sandbox/linux/services/credentials.cc:267-312`: the user-namespace test forks with
  `CLONE_NEWUSER`, writes the uid/gid maps, drops capabilities and must succeed in a nested
  `unshare(CLONE_NEWUSER)`. The gate's `unshare --user --map-current-user unshare --user
  true` probe mirrors it only to *explain* the expected layer (`sandbox-layer1-expected=`).
- `sandbox/linux/suid/client/setuid_sandbox_host.cc:134-178`: `chrome-sandbox` is looked up
  next to the executable (`DIR_EXE`); if used it must be owned by root with mode 4755, or
  Chromium aborts rather than run unsandboxed. The gate directory therefore keeps the flat
  CEF layout with `chrome-sandbox` beside the gate.
- `sandbox/policy/linux/sandbox_debug_handling_linux.cc:67-76`: sandboxed children set
  `PR_SET_DUMPABLE 0`, so `/proc/<pid>/ns/*` and `environ` are not readable by the browser's
  user. The gate reads only world-readable `/proc/<pid>/status` and `cmdline`.
- `content/zygote/zygote_linux.cc:530-612` (`Zygote::ReadArgsAndFork`): a zygote-forked
  child gets the command line the browser sent in the fork request and calls
  `SetProcessTitleFromCommandLine(nullptr)`; `base/process/set_process_title.cc:49-106` and
  `set_process_title_linux.cc` (`setproctitle_init`) only rewrite the title when the argv
  handed to `ContentMain` (`content/app/content_main.cc:252`) is the kernel's contiguous
  argv followed by `environ`. The title is one string `"<exe> <arg> ..."`.
- `content/browser/renderer_host/renderer_sandboxed_process_launcher_delegate.cc:38-46`,
  `content/browser/service_host/utility_sandbox_delegate.cc:140-197`,
  `content/common/zygote/zygote_communication_linux.cc:266-269`: renderers fork from the
  generic (pre-sandboxed) zygote; utilities fork from the generic zygote, from the
  unsandboxed zygote (`--no-zygote-sandbox`) or are launched directly; the GPU process is
  launched directly. `content/zygote/zygote_main_linux.cc:74-160`: the zygote enters layer 1
  and becomes or forks the PID-namespace init, but seccomp-BPF is installed by each child.
- `ui/ozone/platform/x11/x11_surface_factory.cc:231-258` and
  `ui/gfx/linux/gbm_support_x11.cc:32-66`: under Ozone/X11, native pixmaps (the DMA-BUF
  planes OSR hands out) are GBM buffers on a DRM fd obtained from the X server through
  **DRI3**; without DRI3 or a connection there is no GBM device and no native pixmap. The
  gate passes `--ozone-platform=x11` to stay on this path.
- `ui/gfx/linux/gbm_wrapper.cc:281-309, 349-356`: buffers created without a modifier list
  report `gbm_bo_get_modifier`, which can be `DRM_FORMAT_MOD_INVALID` (the implicit case
  welding's wgpu-29 row refuses).
- `components/viz/service/frame_sinks/video_capture/frame_sink_video_capturer_impl.cc:86-97`:
  the ARGB capture format may yield RGBA or BGRA depending on platform, hence the gate
  accepts both `Rgba8UnormSrgb` and `Bgra8UnormSrgb` and compares in the frame's order.

Difference from Chromium: Chromium composites and presents in its own GPU process; here the
frame leaves Chromium as DMA-BUF planes and the host imports and presents it with wgpu, as
on macOS (zero copy) and unlike Windows (copy inside the callback).

## Sandbox

Always `CefSandboxMode::Sandboxed` (`CefSettings.no_sandbox = 0`, null `sandbox_info`, CEF's
native namespace/setuid selection); there is no unsandboxed switch. Evidence, taken from
`/proc` while the browser is alive, for every descendant whose argv[0] is the gate:

| Field | Meaning | Required for renderers |
| --- | --- | --- |
| `Seccomp:` | 2 = seccomp-BPF filter (Chromium's layer 2) | `2` |
| `NoNewPrivs:` | set before the BPF policy is installed | `1` |
| `NSpid:` entries | 2+ = nested PID namespace, created by both layer-1 sandboxes | `>= 2` |
| `--no-sandbox` in `cmdline` | sandbox disabled | absent |

At least one renderer is required and all renderers must pass; zygote, GPU and utility
processes are reported only.

**Renderer identity.** Seccomp, `NoNewPrivs` and `NSpid` are identical for renderers and for
sandboxed utilities forked from the same zygote, so they cannot identify a renderer. A
process counts as a renderer only when its `--type=renderer` comes from the process title
Chromium writes from the zygote fork request, **and** its parent is the generic zygote
(`--type=zygote`, no `--no-zygote-sandbox`, no seccomp). CEF subprocesses therefore receive
the kernel's original argv (captured through glibc's `.init_array` arguments) instead of
cef-rs' `Args::new()` copy, which made `setproctitle_init` give up and left every forked
child showing `--type=zygote` (CI run 45612105775: `renderers=0` with three seccomp-BPF
descendants). The browser process keeps welding's copy. Any seccomp process still titled
`--type=zygote`, or a titled renderer not forked by the generic zygote, is
`role=unidentified` and fails the gate: it is never counted as a renderer nor ignored.
Each `gate: child` line reports `ppid`, `role` and `title=chromium|exec`. The title is
written by the child itself; this is identity reporting for the gate's trusted fixture, not
a security boundary (the sandbox facts come from the kernel). Cross-checking the identity
through CEF itself (`CefBrowserProcessHandler::OnBeforeChildProcessLaunch` gives no PID; a
`CefRenderProcessHandler` report would need welding's `CefApp`) is not available with the
pinned welding. The two layer-1 variants cannot be told apart from `/proc`
of a non-dumpable process; the run states which one Chromium's own order implies
(`gate: host userns-probe=... sandbox-layer1-expected=namespace|setuid|none`) and CI
enforces that expectation by setting `kernel.apparmor_restrict_unprivileged_userns`.

`$ORIGIN` run path: with the setuid sandbox the zygote is started through the set-user-ID
`chrome-sandbox`, and the dynamic loader drops `LD_LIBRARY_PATH` for set-user-ID programs,
so libcef must resolve through the gate's own run path (CI checks `readelf`/`ldd`).

## Verdicts

The binary prints `gate:` lines on stdout and exits with:

| Exit | Meaning |
| --- | --- |
| 0 | An imported frame's centre 64x64 block matched dodger blue (`[30,144,255]` in the frame's channel order, ±8) and every renderer was sandboxed. The PASS line carries `evidence=gpu` or `evidence=software-adapter` |
| 2 | `NO_ACCELERATED_FRAME`: no `OnAcceleratedPaint` before `ROVES_GATE_TIMEOUT_SECS` (default 60), nothing failed, renderers sandboxed |
| 3 | `NO_DMABUF_DEVICE`: the Vulkan adapter is a CPU implementation without the import extensions; a plain device presents instead, delivered frames are counted and dropped; nothing failed, renderers sandboxed. Never returned for a GPU adapter |
| 1 | Any `gate: FAIL`: running as root, missing CEF files, CEF/SDL/wgpu setup, DMA-BUF device on a GPU adapter, non-Vulkan interop, `acquire_frame` (incl. `IMPLICIT_MODIFIER`) or readback error, wgpu uncaptured error, content process terminated, frames imported but none matched, nothing presented, renderer missing or unsandboxed, unidentified sandboxed subprocess |

The fixture is the macOS gate's `data:` page (blue, plus a 1 px corner element animated with
`requestAnimationFrame` so CEF keeps painting). It proves the frame's content arrived, not
ordering, tearing or synchronisation.

## GPU evidence versus CI

| Source | What it can prove |
| --- | --- |
| Hosted CI (`ubuntu-24.04`, Xvfb, no `/dev/dri`, Mesa lavapipe) | Build, flat layout, `$ORIGIN`, CEF init with the sandbox, sandboxed renderers under **both** layer-1 variants, SDL3 X11 window + Vulkan presentation on a CPU adapter, shutdown without leftovers. Exit 2 or 3 expected. Not GPU evidence of any kind |
| Exit 0 with `evidence=software-adapter` | Only that CEF frames reached a CPU Vulkan device; reported as software evidence |
| Exit 0 with `evidence=gpu` on a real Linux desktop (X11 or Xwayland, Mesa, DRI3) | The accelerated DMA-BUF -> Vulkan path on that GPU/driver. Record GPU, driver, kernel, session type and modifier |

Nobody has run it on a GPU yet. Expected per wgpu-weld's own receipts: Intel/Mesa should be
able to pass; AMD/RADV should fail with `IMPLICIT_MODIFIER` on this wgpu-29 row (wgpu-weld
validated AMD only on the wgpu-30 row); NVIDIA proprietary is listed by wgpu-weld as not
supported by the CEF Linux DMA-BUF path.

## CI (`cef-linux-gate.yml`)

`ubuntu-24.04`, push on this directory or the workflow, or manual. Steps: apt packages
(CMake/Ninja for `cef-dll-sys`, SDL3 X11+Wayland build dependencies, CEF runtime libraries,
Xvfb, `mesa-vulkan-drivers`), rustc 1.99.1 check, host notice (`/dev/dri`, `vulkaninfo`),
cargo/CEF cache, dependency graph check, `cargo build --locked --release --bins`,
`target/release/bundle` and a layout check (`libcef.so`, `chrome-sandbox`, `icudtl.dat`,
`resources.pak`, `v8_context_snapshot.bin`, `locales/en-US.pak`, `$ORIGIN`, `ldd`), then
`chrome-sandbox` made root 4755. Two runs under `xvfb-run` with a 150 s hard stop: setuid
layer 1 (`apparmor_restrict_unprivileged_userns=1`) and user-namespace layer 1 (`=0`). Each
fails on any `gate: FAIL`, a missing `cef initialized sandbox=Sandboxed ozone=x11`,
`sdl video driver=x11`, Vulkan adapter, `mode=AcceleratedPaint`, `presented=` > 0,
`PASS sandbox`, the expected layer-1 line or `shutdown complete exit=<code>`, an exit other
than 0/2/3 (3 only with a `Cpu` adapter), or a gate process alive after exit. Artifacts:
`cef-linux-gate-logs`, `cef-linux-gate-x64` (tar of the gate directory). Never gates a release.

Local checks done on Windows (light only, as required):
`cargo check`/`cargo clippy --locked --target x86_64-unknown-linux-gnu --no-default-features --features dox --bins`
(clean, 1.99.1, offline against the seeded lockfile), `cargo fmt --check`, YAML parse and
`bash -n` of the run script. They type-check the Linux code paths of welding, grafting and
wgpu Vulkan; they do not build CEF or SDL, link or run anything.

## Running elsewhere

Extracting the artifact as a normal user drops the root owner and the setuid bit:
re-apply `sudo chown root:root chrome-sandbox && sudo chmod 4755 chrome-sandbox` if the
host restricts unprivileged user namespaces (Ubuntu 23.10+ default), or Chromium stops
with "No usable sandbox!". Then run `./roves-cef-linux-gate` from an X11 session or under
Xwayland (`DISPLAY` set). Never as root.

## Open risks and blockers (not verified)

- **Accelerated frames on real Linux GPUs**: not run anywhere. wgpu-weld's receipts cover
  Intel/Mesa + X11 and, on the wgpu-30 row only, AMD/RADV.
- **Implicit modifiers on the wgpu-29 row**: AMD (and any driver whose GBM buffers report
  `DRM_FORMAT_MOD_INVALID`) cannot be imported with the player's current pins. Options, none
  taken: move the CEF player to welding's `wgpu-30` row (a Windows player change, out of
  scope here), or nothing. Not an upstream bug: welding documents and refuses it on purpose.
- **Software fallback is not available in the pinned welding** (same as macOS:
  `cpu-paint-fallback` declares `CpuPaint` but no producer implements `OnPaint`). On a
  machine without DRI3/GBM (e.g. NVIDIA proprietary, some VMs) the player would show
  nothing; the supported CEF route would be `OnPaint` with one upload per dirty frame,
  through an upstream wgpu-weld issue/PR. Not opened: no evidence yet that it is needed.
- **DMA-BUF synchronisation**: zero-copy import with `Foreign` queue ownership and no
  explicit fence from CEF; possible tearing or stale content under load, not detectable by
  this gate.
- **Shutdown**: welding's Linux producer has no close-completion signal, so the gate pumps
  CEF for 1.5 s after `close()` before `cef::shutdown`.
- **`--password-store=basic`**: unattended choice for the gate (no Secret Service/KWallet
  wait); the player needs a deliberate one before distribution.
- **SDL3 and CEF on one X display**: SDL3 and Chromium each open their own X connection; no
  input path is exercised here.

## Wayland

Not a gate target. The gate forces SDL3's X11 driver and Chromium's Ozone/X11; on a
Wayland desktop both run through Xwayland, which provides DRI3, so the X11 gate applies.
Native Wayland is a gap on both sides: wgpu-weld states Wayland is not supported by its CEF
Linux DMA-BUF path (`demo-weld-linux` docs), and Chromium's Ozone/Wayland gets its buffers
through a different path (Wayland `zwp_linux_dmabuf` with the compositor) than the X11 DRI3
GBM device the gate relies on. `ROVES_GATE_SDL_VIDEO_DRIVER=wayland` only switches the SDL3
window (Chromium stays on X11) and the run prints that it is outside the gate; it exists to
observe the gap, not to claim support. Screen captures over `x11grab` on a Wayland session
are black whatever is shown (wgpu-weld's note), so use the gate's pixel readback.

## Steam Linux Runtime

Not covered. The CEF player has no Steam integration on any platform yet. Under Steam,
games run inside the Steam Linux Runtime container (pressure-vessel, bubblewrap-based):
bubblewrap sets `NO_NEW_PRIVS`, so a set-user-ID `chrome-sandbox` cannot work there, and
Chromium's layer 1 then depends on unprivileged user namespaces being available *inside*
the container (nested namespaces). Whether CEF 154's sandbox starts in the current runtime
(sniper/steamrt3), and whether `/dev/dri`, DRI3 via the host X server/Xwayland and the
Vulkan ICDs are exposed so the DMA-BUF path works, is unverified. The gate reports
`container-env=` (`PRESSURE_VESSEL_RUNTIME`, `STEAM_RUNTIME`, `SteamAppId`, `FLATPAK_ID`,
`container`) so a run inside such a container is identifiable, and never treats it as the
plain-host result. Shipping `libsteam_api.so` and the overlay (it must hook the presenting
process, the main process here) remain as described in the macOS gate's README.
