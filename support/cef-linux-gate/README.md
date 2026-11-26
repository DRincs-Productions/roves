# CEF accelerated OSR -> wgpu-weld -> wgpu + SDL3: Linux feasibility gate

Status (2026-11-25): **fixture gate green in CI run 45618535853** (commit `292f1bee5c2`,
`ubuntu-24.04`, Xvfb, Mesa lavapipe): exit 3 under both layer-1 variants (setuid and user
namespace), i.e. build, flat layout, sandboxed renderers, SDL3 X11 + Vulkan presentation and
shutdown verified, **no DMA-BUF import device, so no CEF frame was imported or shown**.
**Software paint (below, not yet in CI):** where no frame can be imported the gate now uses
CEF's software output instead, so lavapipe is expected to pass with exit 0 and
`evidence=software`; exit 3 no longer exists.
**Packaged-game slice (below): implemented, type-checked locally only, not yet in CI.**
Nothing here claims that the Linux path shows CEF frames, and no CI result from a hosted
runner can claim it works on a GPU (see "GPU evidence versus CI").

Linux twin of `support/cef-macos-gate`, for the CEF + SDL3 + wgpu player
(`support/cef-windows-player`, Windows only). Same question with the same maintained
producer and renderer: does CEF's **accelerated off-screen** frame reach a wgpu device that
also presents to an **SDL3** window on Linux, with **Chromium's sandbox on**? There is no
CEF windowed mode. When no frame can be imported, the browser uses CEF's software output
instead (see "Software paint").

## Software paint

A CPU Vulkan adapter (lavapipe/llvmpipe), a GPU without a DMA-BUF import device (NVIDIA
proprietary, many VMs: `build_dmabuf_capable_device` fails) or `ROVES_FORCE_CPU_PAINT=1`
select CEF's standard software OSR path before the browser exists
(`game_rules::paint_path`, shared with the Windows player): `shared_texture_enabled = 0`,
`OnPaint` BGRA buffer, uploaded by the vendored welding (`support/vendor/wgpu-weld`,
`ROVES_PATCHES.md`) with `Queue::write_texture` into a texture presented like an imported
frame. Logged as `gate: paint path=Software("<reason>") evidence=software`,
`producer created mode=CpuPaint`; every PASS line then says `evidence=software`. That shows
the game and is checked by readback, but says nothing about the DMA-BUF import. A DMA-BUF
device that cannot be created on a GPU adapter is no longer a failure.

## Pins (same graph as the Windows player and the macOS gate)

| Component | Pin | Notes |
| --- | --- | --- |
| `welding` (merely-made/wgpu-weld) | `e991684`, crate 0.15.0, vendored in `support/vendor/wgpu-weld` (path dependency, Roves CPU-paint fallback) | `default-features = false`, `wgpu-29` + `cef-runtime` + `cpu-paint-fallback`. MPL-2.0 |
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
- **Close**: `close()` (`CloseBrowser(force)`), then CEF is pumped until welding's literal
  debug record `CEF on_before_close` (`linux_cef/cef_backed.rs`, the only close-completion
  signal of the pinned Linux producer) reaches the gate's logger, at most 10 s, before
  `cef::shutdown`; a close that does not complete fails the run (`gate: browser closed
  elapsed_ms=` / `FAIL browser not closed`). Same procedure as the macOS gate; it replaced the
  fixed 1.5 s pump of the first version.

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

Packaged game and profile (added with the packaged-game slice, same pinned revision):

- `content/public/browser/web_contents_observer.h` (`DidFinishLoad`,
  `DidFirstVisuallyNonEmptyPaint`), as used by the macOS and Windows players: the main frame's
  load-end is the load criterion; with an import device, the first accelerated frame after it
  is the OSR counterpart of the first visually non-empty paint.
- `chrome/common/chrome_paths_linux.cc:62-100` (`GetDefaultUserDataDirectory`) and
  `base/nix/xdg_util.cc:78-89` (`GetXDGDirectory`): Chromium's Linux profile lives in
  `$CHROME_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, under a product name. The
  player uses the same parent (`$XDG_CONFIG_HOME` or `~/.config`) with `Roves/Player/<name>`;
  differences: it ignores a relative `XDG_CONFIG_HOME` (XDG Base Directory rule; Chromium takes
  any non-empty value), does not read `CHROME_CONFIG_HOME`, and names the profile per install.
- `chrome/browser/process_singleton_posix.cc:12-30, 339-500`: Chromium's Linux singleton is a
  `SingletonLock` symlink to `<hostname>-<pid>` plus a socket that forwards the command line to
  the running instance; only the macOS build takes an `flock(2)` (`ReplaceOldSingletonLock`).
  The player keeps the Windows/macOS rule instead: an `flock` on `player.lock` (std
  `File::try_lock`), released by the kernel when the process dies, and a second launch exits
  with code 6 instead of forwarding anything. Trade-off: no stale-lock handling is needed, but
  `flock` on some network file systems is local-only (see "Open risks").

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

## Packaged game

When `game/index.html` exists next to the executable, the gate runs an unmodified static game
build instead of the fixture (`src/player.rs`), the Linux counterpart of the macOS player:

- `game/` is served by the shared loopback server (`support/cef-windows-player/src/server.rs`,
  compiled through `#[path]` as on Windows and macOS: axum + tower-http `ServeDir` on
  `127.0.0.1`, `Cache-Control: no-cache`, per-instance ETags) and opened at `<origin>/` in the
  same sandboxed accelerated OSR path, X11 only, window 1280x720.
- The origin, load-status and profile-name rules are the Windows player's
  `support/cef-windows-player/src/game_rules.rs`; the profile lock and stored-port glue is
  the macOS gate's `src/profile.rs`; both compiled unchanged through `#[path]`.
- Checked: load-end on the loopback origin with status 2xx/304, no main frame off the origin
  (welding's main-frame veto cancels off-origin navigations before they start; a commit off
  the origin is a backstop failure; refused popups and vetoed navigations go through the
  shared external-link rule, `external_link.rs`), no load error, no content
  process death, sandboxed renderers, the close path (SDL quit or window close request ->
  `close()` -> `OnBeforeClose` -> `cef::shutdown` -> server stop), no leftover processes.
- The first frame imported after the load-end (`gate: first game frame after load-end`; a
  `WasResized` at load-end asks a static page to paint again) and a last frame with more than
  one colour; `ROVES_GATE_FRAME_DUMP=<file.bmp>` writes it. Exit 0,
  `gate: PASS packaged game ... evidence=gpu|software`: `gpu` with a DMA-BUF import device,
  `software` on CEF's software output (the hosted runner's lavapipe, see "Software paint").
- Console messages are logged (first 200, 300 characters each) and counted by level; popups
  are refused by welding and reported. No remote-debugging endpoint is opened. Steam only in
  the `steam` feature build (section "Steam"). The host API, the `window.__ROVES__` marker,
  input, window and audio follow the macOS gate's README section "Host API marker, input,
  window and audio" (same `src/session.rs`, through `#[path]`); the project icon: section
  "Application icon". Startup screen: section "Startup screen".
- Test-only: `ROVES_GATE_AUTO_CLOSE_MS` pushes one SDL quit event that long after the first
  imported game frame, or after the accepted load-end when there is no import device (where
  no frame can ever be shown), so CI exercises the ordinary close path.

## Startup screen

The macOS gate's `src/startup_screen.rs` compiled through `#[path]` (README of that gate,
section "Startup screen", for content and rules): packaged-game mode only, the Windows
player's dark background, Roves logo, MetalMania wordmark (rasterized by this gate's
`build.rs` with the shared `support/cef-windows-player/build_wordmark.rs`; sparse checkouts
need `resources/fonts`) and indeterminate loading bar, from the first present until the
first game frame. The page-marker bootstrap document's frames stay behind it. Released by
the first game frame after the accepted load-end, or by the "settled" frame for static
pages: `gate: startup screen released`, just before `gate: first game frame ...`. On the
hosted runner the frames are CEF's software output (section "Software paint"), so the
release is exercised there too.

## Application icon

The window icon follows the players' project-icon contract (`window_icon.rs`,
`icon_rules.rs`): a valid `icon.png` next to the executable, set on the SDL3 X11 window
(`_NET_WM_ICON`), else the embedded Roves logo; one line `gate: window icon source=...`. The
package ships no `.desktop` entry or hicolor icon for desktop integration (launchers, dock
before start): that is packaging work for the distribution format still to choose (tar,
AppImage, Flatpak, Steam), tracked in `TODO.md`.

## Steam

`steam` cargo feature, off by default; the same code as the macOS gate (its README, section
"Steam"): the Windows player's `src/steam.rs`, the lifetime in the macOS gate's
`src/player_steam.rs` and the redistributable lookup `src/steam_lib.rs`, all through
`#[path]`. Packaged-game mode only, initialized in the browser process before CEF starts;
App ID from `SteamAppId`, else `steam_appid.txt` next to the executable; Steam missing is
never fatal (`gate: steam unavailable reason=...`, the host API answers "unavailable");
`gate: steam shutdown complete` before CEF shuts down. `target/release/bundle` built with the
feature copies steamworks-sys's `libsteam_api.so` next to the executable, where the gate's
`$ORIGIN` run path resolves it (no `LD_LIBRARY_PATH`, which the setuid `chrome-sandbox`
would drop). CEF re-executes the same binary for its subprocesses, so the dynamic loader
maps the library into them as well (before the sandbox engages); they never call it.
`chrome-sandbox` does not depend on it. Inside Steam the game runs in the Steam Linux
Runtime container: section "Steam Linux Runtime".

## Saves and the player profile

Ordinary launch (persistent, as on Windows and macOS): the root is
`$XDG_CONFIG_HOME/Roves/Player/<name>` when `XDG_CONFIG_HOME` is absolute, otherwise
`$HOME/.config/Roves/Player/<name>`, with `<name>` = executable stem + 64-bit FNV-1a of the
lower-cased canonical directory of the executable (`player_profile_name`). It holds
`player.lock` (held with `flock` for the whole browser-process run), `origin-port` (the port
the OS chose on the first launch, written atomically and bound again on every later launch,
so origin-keyed storage survives) and CEF's root cache `cef/` with welding's on-disk context
`cef/game-profile` (welding's Linux producer accepts it directly in `new`). Before CEF starts:
profile held by another instance -> exit 6; stored port taken -> exit 6; damaged
`origin-port` -> exit 1 (never a silent new origin). `--roves-isolated-profile` uses a per-run
cache under the temporary directory, removed at exit (CI's first packaged run).
Limitations: moving or renaming the directory starts a new profile; the lower-casing (a
Windows rule) makes two install paths that differ only in case share a profile.

## Local MCP bridge

Development only, off by default: the Windows player's local MCP bridge
(`support/cef-windows-player/src/mcp.rs` and `mcp_rules.rs` through `#[path]`; Windows
player README "Local MCP bridge"; client instructions in
`support/cef-windows-player/mcp/README.md`). A packaged-game launch starts it with
`--roves-mcp`, or when the package contains `mcp/enabled` next to the executable (only the
MCP-enabled development package has it); `--roves-mcp` without a packaged game is a usage
error. Off means no socket, no session record and no `gate: mcp` line.

- **Transport.** Before the browser is created `session.rs` starts the bridge: the per-user
  session directory `$XDG_RUNTIME_DIR/roves/mcp` (an absolute `XDG_RUNTIME_DIR` only), else
  `~/.local/state/roves/mcp` is created with mode 0700 (or must be a real directory owned by
  the user and is tightened to 0700), a unix-domain socket `<pid>.sock` (0600, tokio
  `UnixListener`) is bound there and the record `<pid>.json` (0600; `pipe` holds the socket
  path, plus the 256-bit `token` from `getrandom` and the game `origin`) is written through
  a temporary file and a rename. Connections whose peer credentials (`SO_PEERCRED`) name
  another uid are closed unread; every other connection must first send the token. Socket
  and record are removed at the end of the loop (`gate: mcp session removed calls=<n>`). Log
  lines: `gate: mcp enabled source=Switch|Package pipe=<socket> record=<path>` (never the
  token) or `gate: mcp unavailable source=... error=...` (the game still runs, without the
  marker).
- **Requests** run in the player loop, at most 4 per turn: `state` (origin, URL, title,
  `loaded`, `page_capability`, `startup` `loading|released` (first game frame shown),
  `input_ready`, window size/pixels/scale, imported frames, input counters, last 50 console
  lines); `evaluate` over the same in-process DevTools channel as the page markers (CDP
  `Runtime.evaluate`, CSP kept, Promise refused; refused before the game URL was requested);
  `click`/`press_key`/`type_text` as SDL events through the same `Input` as real input,
  refused before the accepted game load-end (as real input).
- **Page marker**: with the bridge running the page-marker script also defines the read-only
  `window.__ROVES_MCP__ === true` in the top-level game document (`gate: page markers
  registered ... mcp=true`); without it, `mcp=false` and the marker is absent.
- **MCP-enabled package**: the standard package plus `mcp/` next to the executable
  (`server.mjs`, `README.md`, `package.json`, `package-lock.json`, `node_modules/` from `npm
  ci --omit=dev`, and `enabled`); the executable `roves-cef-linux-gate` and the packaged
  template are byte-identical to the standard package's. CI uploads it as
  `cef-linux-gate-x64-mcp` (release asset `roves_shell_linux_mcp.zip`).

CI (`.github/workflows/cef-linux-gate.yml`, after the Steam variant): the stand-alone
`mcp_rules` tests (including the unix directory 0700/owner and record 0600 test), then
`smoke.mjs` (`support/cef-windows-player/mcp/smoke.mjs`) three times. **fixture**: the MCP
fixture temporarily as the MCP package's game, launched with `--roves-mcp`: record and
socket in the 0700 directory (both 0600), unauthenticated connections answered with nothing,
discovery by PID, state, the `isMcpEnabled()` marker (entry script, top level only),
evaluation, a trusted click, ArrowRight keydown/keyup, typed text, record and socket
removed, token not logged. **disabled**: the standard package on the same fixture without
the switch, and with `--roves-mcp` while the session directory cannot be created
(`XDG_RUNTIME_DIR` points at a file): marker absent, host marker present, no record.
**package**: the MCP package launched directly with the template, discovered without a PID.
Exit 0, or 2 when the runner gives no frame (the bridge checks still run). Not verified yet:
any run of these steps; a real MCP client session on a Mac/Linux desktop.

## Verdicts

The binary prints `gate:` lines on stdout (the last is `gate: exit code=<N>`) and exits with:

| Exit | Meaning |
| --- | --- |
| 0 | A shown frame's centre 64x64 block matched dodger blue (`[30,144,255]` in the frame's channel order, ±8) and every renderer was sandboxed. The PASS line carries `evidence=gpu` (DMA-BUF import) or `evidence=software` (CEF software output) |
| 2 | `NO_FRAME`: no frame before `ROVES_GATE_TIMEOUT_SECS` (default 60), nothing failed, renderers sandboxed. CI treats it as an error on a CPU adapter |
| 6 | Packaged game, ordinary launch, before CEF starts: profile held by another instance, or stored origin port taken |
| 1 | Any `gate: FAIL`: running as root, missing CEF files, CEF/SDL/wgpu setup, non-Vulkan interop, `acquire_frame` (incl. `IMPLICIT_MODIFIER`) or readback error, wgpu uncaptured error, content process terminated, frames imported but none matched, nothing presented, renderer missing or unsandboxed, unidentified sandboxed subprocess, browser not closed within 10 s; packaged game: load-end refused or missing, main frame off the origin, load error, last frame single-coloured, damaged `origin-port`, loopback stop failure |

Packaged game: 0 = load-end, imported first frame after it, multi-colour last frame, close,
sandbox (`evidence=gpu|software`); 2 = loaded but no frame.

The fixture is the macOS gate's `data:` page (blue, plus a 1 px corner element animated with
`requestAnimationFrame` so CEF keeps painting). It proves the frame's content arrived, not
ordering, tearing or synchronisation.

## GPU evidence versus CI

| Source | What it can prove |
| --- | --- |
| Hosted CI (`ubuntu-24.04`, Xvfb, no `/dev/dri`, Mesa lavapipe) | Build, flat layout, `$ORIGIN`, CEF init with the sandbox, sandboxed renderers under **both** layer-1 variants, SDL3 X11 window + Vulkan presentation on a CPU adapter, shutdown without leftovers, and CEF's software output shown and read back (exit 0, `evidence=software` required). Not GPU evidence of any kind |
| Exit 0 with `evidence=software` | CEF's software frames shown through wgpu; says nothing about the DMA-BUF import |
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
fails on any `gate: FAIL`, a missing `mode=fixture`, `cef initialized sandbox=Sandboxed
ozone=x11`, `sdl video driver=x11`, Vulkan adapter, `mode=AcceleratedPaint|CpuPaint`,
`presented=` > 0, `PASS sandbox`, `browser closed`, `cef shutdown complete`, the expected
layer-1 line or `shutdown complete exit=<code>`, an exit other than 0/2, or a gate process
alive after exit. With a `Cpu` adapter it also requires `paint path=Software("cpu-adapter")
evidence=software`, `mode=CpuPaint`, exit 0 and a `PASS pixel ... evidence=software` line.

Packaged game (same job): `DRincs-Productions/visual-novel-template` at `92e5115ac52d`
(same pin and `npm ci` + `npm run build` as the Windows and macOS packages), `dist/` copied
byte for byte into `game/` (checked with `diff -r`, SHA-256 list in the logs); the packaged
runs use the setuid layer 1 (Ubuntu's default).

1. `--roves-isolated-profile` with `ROVES_GATE_AUTO_CLOSE_MS=8000`: the exit must equal the
   setuid fixture's (same adapter, so 0 with `evidence=software` on lavapipe) and the lines
   for that exit must appear in order: `producer created mode=AcceleratedPaint|CpuPaint ...
   url=<origin>/ profile=.../game-profile`, `load-end status=200 url=<origin>/ ok=true`,
   `game loaded`, `fresh frame requested ok=true`, the first game frame after load-end,
   `test auto-close scheduled delay_ms=8000 anchor=first-frame`, `pushed SDL quit`, the last
   game frame, the BMP, `PASS sandbox`, `PASS packaged game ... evidence=gpu|software`,
   `browser closed`, `cef shutdown complete`, `loopback stopped`, `cache removed`,
   `shutdown complete exit=0`, `exit code=0`. Always:
   no `DevTools listening`, no persistent-profile line, no cache left, loopback port closed,
   no process left.
2. With `XDG_CONFIG_HOME` set to a fresh directory and from an unrelated working directory:
   two ordinary launches must create and then reuse
   `$XDG_CONFIG_HOME/Roves/Player/roves-cef-linux-gate-<16 hex>` with the same origin and keep
   `origin-port`, `cef/` and `cef/game-profile`; then a held `flock` and a held port each give
   exit 6 and a damaged `origin-port` exit 1, all before `cef initialized`.

Artifacts: `cef-linux-gate-logs` (fixture, player and profile logs, BMP when exit 0, template
build info), `cef-linux-gate-x64` (tar of the gate directory, with `game/` when packaging ran).
Never gates a release.

Local checks done on Windows (light only, as required):
`cargo check`/`cargo clippy --locked --target x86_64-unknown-linux-gnu --no-default-features --features dox --bins`
(clean, 1.99.1, offline against the seeded lockfile), `cargo fmt --check`, YAML parse and
`bash -n` of the run script. They type-check the Linux code paths of welding, grafting and
wgpu Vulkan; they do not build CEF or SDL, link or run anything. Packaged-game slice: the same
`cargo clippy ... --features dox --bins` (no warnings) and `cargo fmt --check`; the four new
dependencies were resolved offline with `cargo update -w`, and every package of the lockfile
now has the macOS gate's version; YAML parse and `bash -n` of every `run:` block and of
`run-gate.sh`; the packaged run's ordered-pattern check executed against synthetic logs built
from the gate's own `println!` formats (exit 3 and exit 0 pass; an exit-3 log with a PASS line
fails).

**Player fixture** (after the persistent-profile step; the macOS step's counterpart): the
packaged template is swapped for `support/cef-macos-gate/player-fixture` (restored
afterwards) with the repository `icon.png` next to the executable, and run once with a fresh
persistent profile (own `XDG_CONFIG_HOME`), `ROVES_GATE_TEST_INPUT_MS=1000` and
`ROVES_GATE_AUTO_CLOSE_MS=6000`. Same required lines as on macOS, with the XKB native code
(`native_key_code=0x0072`, ArrowRight) and the setuid fixture's exit code. Audio: a
PulseAudio daemon with a null sink (`pulseaudio`, `pactl`) runs during the step; the click
starts a WebAudio oscillator, the page must report `roves-ci-audio state=running` twice
(the second time 1.5 s later, with an advancing clock), and `pactl list short sink-inputs`,
sampled twice a second while the player runs, must show Chromium's output stream. That is
evidence that audio reaches the sound server, not that it is audible. External links: the
same checks as on macOS (`ROVES_TEST_EXTERNAL_OPEN_LOG=1`; the vetoed example.com navigation
and the example.org popup opened in log-only mode, nothing for the gesture-less example.net
popup, the document kept).

**Icon, startup screen and Steam** (sections "Application icon", "Startup screen", "Steam"):
the fixture gate must log no startup-screen line; the isolated packaged run additionally
requires `startup logo decoded`, `wordmark rendered ... font=MetalMania`, a `window icon
source=(project|roves) ... set=true` line, no `gate: steam` line and, on exit 0, `startup
screen released` between `fresh frame requested` and the first game frame; the player
fixture requires the release after its load-end on exit 0. The standard package must contain
no `libsteam_api.so` and its executable must not need it. Steam variant: `cargo build
--release --bins --features steam` in the same target directory (`gate-steam-build.log`),
bundled into its own directory with the template, `chrome-sandbox` made root 4755; required:
`libsteam_api.so` next to the gate and byte-identical to steamworks-sys's redistributable, a
`NEEDED [libsteam_api.so]` entry, the `$ORIGIN` run path, `ldd` resolving `libsteam_api.so`
and `libcef.so` from the package directory with nothing missing, and `chrome-sandbox` not
depending on it. Then one launch under **emulation**, the Windows player's design: the
pinned Goldberg/GSE Linux release (gbe_fork `release-2026_09_27`,
`emu-linux-release.tar.bz2`, SHA-256 checked) replaces
`libsteam_api.so` offline (`steam_interfaces.txt` generated from the redistributable,
`steam_settings` with App ID 480, one achievement and one stat), the game is
`support/cef-windows-player/host-api-fixture` with a fresh persistent profile; the package is
restored afterwards. Required: exit 0; in order `steam initialized app_id=480`, origin,
persistent profile, sandboxed CEF, accepted load-end, `startup screen released`, `close
requested by the game command=exit`, browser closed, `steam shutdown complete`, `cef shutdown
complete`, loopback stopped, exit line; the page's `roves-host-api steam available=true
app_id=480 unlock=true unlocked=true set=true stat=3 store=true` and `phase=1 write=true
read=hello-roves`; no `FAIL`, `steam unavailable`, host API error or CSP violation
(`steam-emulation/steam-emulation.log`). This is emulator evidence, not the real Steam client.
Artifact `cef-linux-gate-x64-steam`: the Steam package's tar (redistributable and template).

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
- **Shutdown**: welding's Linux producer has no public close-completion signal; the gate
  relies on its literal debug record `CEF on_before_close` (pinned revision). A welding update
  that rewords it fails every run with `browser not closed`, never silently.
- **Packaged game on a GPU**: not run anywhere; on the hosted runner the game's frames are
  never imported. A real desktop run (and a look at `player-last-frame.bmp`) is still needed.
- **Profile lock on network home directories**: `flock` on NFS/CIFS may be local to the
  client, so two machines sharing one home could both acquire it. Not handled.
- **`--password-store=basic`**: unattended choice for the gate (no Secret Service/KWallet
  wait); the player needs a deliberate one before distribution.
- **SDL3 and CEF on one X display**: SDL3 and Chromium each open their own X connection. The
  packaged game forwards SDL3 input (CI: synthetic SDL events only); physical keyboard
  layouts, IME and dead keys are manual checks on a desktop.
- **Audio on desktops**: CI proves an output stream reaches a PulseAudio null sink. PipeWire
  desktops serve the same PulseAudio protocol (`pipewire-pulse`); a desktop without either
  falls back to ALSA in Chromium. Audible output stays a manual check.
- **Duplication with the macOS player**: `player.rs`, the readback/BMP helpers and the close
  procedure mirror the macOS gate's code instead of sharing it, because sharing would mean
  editing the macOS gate (out of scope for this slice). A shared module over
  `CefSurfaceProducer` is a follow-up.

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

Not covered. The `steam` feature (section "Steam") is checked on the plain host only, under
the GSE emulator in CI. Under Steam,
games run inside the Steam Linux Runtime container (pressure-vessel, bubblewrap-based):
bubblewrap sets `NO_NEW_PRIVS`, so a set-user-ID `chrome-sandbox` cannot work there, and
Chromium's layer 1 then depends on unprivileged user namespaces being available *inside*
the container (nested namespaces). Whether CEF 154's sandbox starts in the current runtime
(sniper/steamrt3), and whether `/dev/dri`, DRI3 via the host X server/Xwayland and the
Vulkan ICDs are exposed so the DMA-BUF path works, is unverified. The gate reports
`container-env=` (`PRESSURE_VESSEL_RUNTIME`, `STEAM_RUNTIME`, `SteamAppId`, `FLATPAK_ID`,
`container`) so a run inside such a container is identifiable, and never treats it as the
plain-host result. The overlay (it must hook the presenting process, the main process here)
remains a manual check with the real Steam client (macOS gate's README, "Steam variant: open
points").
