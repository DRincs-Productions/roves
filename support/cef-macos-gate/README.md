# CEF accelerated OSR -> wgpu-weld -> wgpu + SDL3: macOS feasibility gate

Status (2026-11-25): the fixture gate is **green in CI run 45575923557** (commit
`fa9a52ce122`, `macos-15` arm64, macOS 15.8.2, Apple paravirtual GPU): exit 0, so CEF's
accelerated IOSurface frames reached the wgpu Metal device on that runner with a sandboxed
renderer. The **packaged-game slice** (section "Packaged game") was committed in
`ecd635fea9f`; its CI result is pending. The **persistent-profile slice** (section "Saves and
the player profile") is implemented and type-checked locally only, with no CI result yet. No
real Mac has run either mode.

First non-Windows step of the CEF + SDL3 + wgpu player (`support/cef-netrender-probe`,
Windows only). It answers one question with the same maintained producer and renderer as
Windows: does CEF's **accelerated off-screen** frame reach a wgpu device that also presents
to an **SDL3** window on macOS, with **Chromium's sandbox on**? There is no CEF windowed
mode and no software (`OnPaint`) path. The Windows player, its lockfile and its workflow are
untouched.

## Pins (same graph as the Windows player)

| Component | Pin | Notes |
| --- | --- | --- |
| `welding` (merely-made/wgpu-weld) | `e991684`, crate 0.15.0, vendored in `support/vendor/wgpu-weld` (path dependency; Roves CPU-paint fallback, `ROVES_PATCHES.md`) | `default-features = false`, `wgpu-29` + `cef-runtime` + `cpu-paint-fallback`. The gate keeps `prefer_accelerated = true` (Metal always imports IOSurfaces), so its behaviour is unchanged; the software `OnPaint` path is compiled in but not selected here. MPL-2.0 |
| `grafting` | git `d903cfa` | via the lockfile, as on Windows |
| `cef` / `cef-dll-sys` | `=154.5.0+154.0.34` | CEF 154.0.34, Chromium 154.0.8037.98; features `build-util` + `sandbox` |
| `wgpu` | `=29.0.4` | features `std`, `metal`, `wgsl` |
| `sdl3` | 0.20 (SDL 3.4.16) | `build-from-source-static` (feature `bundled-sdl`) + `raw-window-handle` |
| Rust | 1.99.1 (MSRV 1.97.1) | as the Windows workflow |

`Cargo.lock` was seeded from `support/cef-netrender-probe/Cargo.lock`; resolving this crate
only removed packages (ab_glyph, Netrender, ...) and added none, so every shared
package, macOS-only ones included (objc2, objc2-metal, objc2-io-surface, libloading), has the
Windows player's version. The packaged-game slice re-seeded it the same way: axum 0.8.9,
tower-http 0.7.1, tokio and tower (the loopback server's crates) came back at the Windows
player's versions, and the lock diff contains only additions. CI repeats the Windows graph check: one wgpu 29.0.4 for the gate,
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

Added for the packaged-game slice (same revision, sparse paths added):

- `content/public/browser/web_contents_observer.h:500-517`: `DidFinishLoad` fires when the
  document's load is done in the renderer (onload dispatched), per frame; `DidFailLoad` is
  its failure twin. CEF reports the former as `OnLoadEnd` and the latter as `OnLoadError`,
  and welding forwards only the main frame's. The player accepts the game on the main
  frame's load-end on the loopback origin with a 2xx/304 status and fails on a main-frame
  load error, like the Windows player.
- Same header, line 652 (`DidFirstVisuallyNonEmptyPaint`) and 1055
  (`OnFirstContentfulPaintInPrimaryMainFrame`): Chromium's own "first paint" signals live
  inside the browser and are not exposed by CEF's OSR API. The OSR counterpart used here is
  the first `OnAcceleratedPaint` frame imported after the accepted load-end; a `WasResized`
  at load-end makes an already static page paint once more (as the Windows player's startup
  gate). Content is not compared with a reference: the gate records the frame's distinct
  colours and requires the last frame before close not to be a single colour.
- `chrome/browser/app_controller_mac.mm:938-988` (`tryToTerminateApplication`): Chrome's
  quit path first closes every browser (`CloseAllBrowsersAndQuit`) and only then shuts down.
  The player follows the same order: SDL quit/window close -> `close_browser(force)` ->
  pump CEF until `OnBeforeClose` -> `cef::shutdown` -> stop the loopback server (after CEF,
  so in-flight responses finish) -> remove the per-run cache (isolated launch) or keep the
  persistent profile and release its lock. Unlike Chrome there is no
  `beforeunload` confirmation (forced close), as on Windows.

## Packaged game

The same binary becomes a minimal player when its bundle contains
`Contents/Resources/game/index.html` (`src/player.rs`); otherwise it runs the fixture gate.
Opening such an `.app` plays the game: no switch, no debug server.

- **Serving**: `support/cef-window-probe/src/server.rs`, compiled through `#[path]` exactly
  as the Windows player does (axum + tower-http `ServeDir` on `127.0.0.1`, port 0 = a new
  origin per run, `Cache-Control: no-cache`, per-instance ETags). It starts before CEF (the
  origin is the browser's initial URL) and stops after `cef::shutdown`. The game files are
  never modified or injected into.
- **Load**: the main frame's `LoadEnd` must be on the origin with 2xx/304; a main-frame
  `LoadError`, a main frame committed off the origin (a backstop: welding's main-frame veto,
  as on Windows, cancels off-origin navigations before they start), or a renderer death fails
  the run. Refused popups and vetoed navigations are external links: opened in the system
  browser only for http/https/mailto after a user gesture, at most once a second, logged as
  scheme + host (the Windows player's `external_link.rs`, README "External links" there).
  Console messages are printed (first 200, then counted per level).
- **First frame**: the first accelerated frame imported after the accepted load-end, read
  back once for the log. `ROVES_GATE_TIMEOUT_SECS` (default 60) bounds load-end plus that
  frame; no frame at all gives exit 2, frames only before the load-end give exit 1.
- **Close**: SDL `Quit` (Cmd+Q, last window closed) or `WindowEvent::CloseRequested` ends
  the loop; then `close_browser(force)` and a CEF pump until `OnBeforeClose`. welding
  e991684's macOS producer exposes no close-completion API (Windows has `is_closed`); its
  `LifeSpanHandler::on_before_close` logs the literal debug record `CEF on_before_close`
  (target `welding::macos_cef::cef_backed`), which the gate's logger turns into a flag
  (`fmt::Arguments::as_str`, no formatting of other debug records). Pinned revision: the
  signal must be rechecked when welding is bumped. No `OnBeforeClose` within 10 s fails the
  run. The fixture gate uses the same close (it used a fixed 1.5 s pump before).
- **Profile**: persistent per install with a fixed loopback origin in an ordinary launch;
  per run and removed at exit with `--roves-isolated-profile` (CI). See "Saves and the
  player profile".
- **Test-only environment** (CI): `ROVES_GATE_AUTO_CLOSE_MS` pushes one SDL quit event that
  long after the first game frame, so the ordinary close path runs unattended;
  `ROVES_GATE_FRAME_DUMP=<file.bmp>` writes the last frame before close (32-bit top-down
  BMP of the `Bgra8Unorm` readback).
- **Not in this slice**: native error dialogs, signing/notarisation. Page marker, input,
  window and audio: next section; startup screen, bundle icon and Steam: sections "Startup
  screen", "Application icon" and "Steam".

## Host API marker, input, window and audio

Shared with the Windows player through `#[path]`, so the packaged game behaves the same on
all three platforms (glue: `src/session.rs`, compiled by the Linux gate too); the project icon is
`Contents/Resources/icon.png` (`SDL_SetWindowIcon` sets the Dock icon at run time; Finder
shows the bundle's `.icns`, section "Application icon"):

- **Host API marker**: `window.__ROVES__` (read by `isAvailable()` of
  `@drincs/roves-api/core`) is registered before the game document exactly as on Windows:
  `support/cef-netrender-probe/src/page_init.rs` (now generic over welding's
  `CefSurfaceProducer`) enables the CDP Page domain and calls
  `Page.addScriptToEvaluateOnNewDocument` through welding's in-process DevTools channel
  (`devtools_protocol`, `CefBrowserHost::SendDevToolsMessage`; no remote-debugging port),
  with the script of `mcp_rules::page_marker_script`. The browser starts on `about:blank`;
  its navigation events are skipped (`gate: bootstrap document ...`) and the game URL is
  loaded once the registration settled (`gate: page markers registered`, or `unavailable`
  after 10 s, then the game loads without the marker). With the local MCP bridge running
  (section "Local MCP bridge") the same script also defines `window.__ROVES_MCP__`.
- **Input** (`support/cef-netrender-probe/src/input.rs`): SDL3 mouse move/buttons (click
  count from CEF), wheel (120 per line, as on Windows), key down/up, committed text
  (`SDL_StartTextInput`; IME composition is not forwarded, as on Windows), focus on the first
  turn after load-end, visibility on minimize/occlusion, display-scale changes and the page's
  cursor shape. Page input starts at the accepted load-end. Key codes per platform
  (`src/keys.rs`, `src/key_rules.rs`): `native_key_code` from Chromium's own key table
  (`ui/events/keycodes/dom/dom_code_data.inc`, through the `keycode` crate) indexed by SDL's
  scancode: the macOS virtual key code (`kVK_*`) on macOS, the XKB key code on Linux, SDL's
  `raw` only when the table has none. macOS CEF synthesizes an `NSEvent` from that code and
  the characters (`TranslateWebKeyEvent`), so key down/up carry the layout character with
  the modifiers applied (`SDL_GetKeyFromScancode`) or AppKit's function-key character
  (`NSUpArrowFunctionKey`...), and none for modifiers and CapsLock (a flags change). Linux CEF
  derives `key`/`keyCode` from `windows_key_code`, so it is the virtual key of the layout
  character (Chromium's X11 rule) or the Windows player's table for keys without one,
  falling back to the US key at the same position. Unit tests: `support/cef-host-api-tests`.
- **Window**: resizable; a pixel-size change reconfigures the wgpu surface and calls
  welding's `resize` (CEF `WasResized`), plus `set_scale_factor` when the display scale
  changed (HiDPI). The title follows the game document's `<title>`. The window icon follows
  the Windows project-icon contract (`window_icon.rs`, `icon_rules.rs`): a valid `icon.png`
  in the icon directory, else the embedded Roves logo (`gate: window icon source=...`).
- **Startup screen**: the Windows player's branding until the first game frame, section
  "Startup screen".
- **Audio**: Chromium plays page audio itself (CoreAudio on macOS, PulseAudio with an ALSA
  fallback on Linux, from its audio service). Neither gate passes a switch that mutes or
  disables it (no `--mute-audio`, no autoplay override): autoplay follows Chromium's default
  policy, so WebAudio starts after a user gesture, as in Chrome.
- **Test-only environment** (CI): `ROVES_GATE_TEST_INPUT_MS=<ms>` (at most 600000): that long
  after the accepted load-end, one left click at SDL point (100, 60), one ArrowRight press
  and one wheel notch are pushed into SDL's queue (sentinel device ids) and come back through
  the ordinary poll -> `Input` -> welding path; the page must report them with the Windows
  probe's console markers (`roves-test-click-received`, `roves-test-key-received`,
  `roves-test-wheel-received`), checked by the `game_rules` click/key/wheel sequences side
  by side. `gate: PASS test input click, key and wheel delivered`, or a `FAIL` (exit 1); the
  test auto-close waits for it. Fixture: `support/cef-macos-gate/player-fixture` (README
  "CI").

## Startup screen

Packaged-game mode only (the fixture gate keeps black): from the first present until the
first game frame the window shows the Windows player's startup screen, same content and
rules: dark background, the Roves logo and the MetalMania "Roves" wordmark of
`support/cef-netrender-probe/src/splash.rs` (`#[path]`) and the indeterminate loading bar,
placed by `game_rules::splash_layout`/`splash_indicator`. The wordmark is rasterized at build
time by the shared `support/cef-netrender-probe/build_wordmark.rs` (this gate's `build.rs`,
`ab_glyph`, font `resources/fonts/MetalMania-Regular.ttf`, SIL OFL 1.1), so sparse checkouts
need `resources/fonts`. `src/startup_screen.rs` composes the static column on the CPU once per
window size into one texture shown 1:1 by the frame path's `TextureBlitter`; only the bar's
rows are rewritten each frame. Frames before the game's first one (the `about:blank` page-marker
bootstrap) stay behind it. It is released by the first game frame after the accepted
load-end, or by the "settled" frame 1 s after load-end for static pages (`gate: startup
screen released`, just before `gate: first game frame ...`). Log lines: `gate: startup logo
decoded size=...`, `gate: wordmark rendered size=... font=MetalMania` (a failure only omits
that piece: `... unavailable error=...`). No failure screen: the gate ends the run instead.
The Linux gate compiles the same module.

## Application icon

Two icons, as on Windows (exe resource plus window icon):

- **Bundle icon** (Finder, Launchpad, the Dock before launch): `packaging/icns.sh
  <square.png> <out.icns>` converts a square PNG (the game's project icon, or the repository
  `icon.png` as the fallback) with the system's `sips` and `iconutil`; sizes above the source
  edge are left out, not upscaled. `target/release/bundle <out> --resources <dir>`: cef's
  bundler copies `<dir>/mac` into `Contents/Resources` and sets `CFBundleIconFile` when
  `<dir>/mac/roves-cef-macos-gate.icns` exists. Packaging replaces that file in place for a
  game's own icon.
- **Window/Dock icon at run time**: `Contents/Resources/icon.png` when valid, else the
  embedded Roves logo (`gate: window icon source=...`, section "Host API marker, input, window
  and audio").

CI builds the `.icns` from the repository `icon.png` and checks the file and the Info.plist
key (`PlistBuddy`).

## Steam

`steam` cargo feature, off by default (the standard `.app` has no Steam code and no
`libsteam_api.dylib`). Same client as the Windows player: its `src/steam.rs` through
`#[path]` (steamworks-rs `=0.13.1`, steamworks-sys 0.13.0 with the bundled SDK
redistributables) behind the host API's `steam` module; lifetime in `src/player_steam.rs`
(shared with the Linux gate):

- Packaged-game mode only, initialized once in the browser process before CEF starts, never
  in a helper. App ID from `SteamAppId` (Steam's launch environment), else
  `Contents/MacOS/steam_appid.txt`, else whatever `SteamAPI_Init` finds.
- Steam missing or not running is never fatal: one line `gate: steam unavailable reason=...`
  (no persona name or Steam ID) and the host API answers "unavailable"; the game plays as in
  the standard package. Success: `gate: steam initialized app_id=<id>`.
- Callbacks dispatched once per drive-loop turn; the client is dropped (`SteamAPI_Shutdown`,
  `gate: steam shutdown complete`) after the browser closed and before CEF shuts down.
- `target/release/bundle` built with the feature copies steamworks-sys's
  `libsteam_api.dylib` (found in `<target>/release/build/steamworks-sys-*/out`, byte-checked
  when several exist, `src/steam_lib.rs`) into `Contents/MacOS`, where its install name
  `@loader_path/libsteam_api.dylib` resolves from the main executable. The five CEF helpers
  are separate executables and do not link it.

CI builds the variant after the standard steps (`cargo build --release --bins --features
steam`, same target directory), bundles it with the same icon and template, and requires the
dylib, the main executable's `@loader_path/libsteam_api.dylib` reference and no helper
referencing it. The runner has no Steam client and macOS has no emulator step, so the run
requires `steam unavailable reason=`, an accepted load-end and the standard isolated run's
exit code. The real Steam client and the overlay (it must hook the presenting process, the
main one here) remain manual checks; signing/notarising the dylib with the `.app` is not
addressed.

## Saves and the player profile

The macOS counterpart of the Windows player's persistent profile (Windows README "Saves and
the player profile"), with the same rules: `support/cef-netrender-probe/src/game_rules.rs` is
compiled here through `#[path]`, unchanged (file names, `player_profile_name`, origin-port
parsing, plus the origin check and load statuses `src/player.rs` used to duplicate). Only the
platform glue is in `src/profile.rs`.

- **Root**: `~/Library/Application Support/Roves/Player/<exe stem>-<16 hex>`, where the hash
  is the 64-bit FNV-1a of the lower-cased canonical `.app` path (the Windows rule with the
  bundle as the install directory; APFS is case-insensitive by default, like NTFS). Every
  launch of one install finds the same profile; two packaged games never share storage;
  moving or renaming the `.app` starts a new profile (documented limitation, as on Windows).
  The working directory plays no part.
- **Files**: `player.lock` (held with `flock(LOCK_EX | LOCK_NB)` through std's
  `File::try_lock` for the whole browser-process run; released by the kernel when the
  process ends, crash included), `origin-port` (decimal port, written once through a
  temporary file and a rename), `cef/` (CEF's root cache) and `cef/game-profile` (welding's
  per-producer on-disk context: `persist_session_cookies`, Local Storage, IndexedDB, HTTP
  cache).
- **Order**: profile lock and stored port, then the loopback server on that port (port 0 on
  the first launch, then the bound port is stored), then CEF with `cache_path = <root>/cef`.
  The lock is released after `cef::shutdown` and the server stop.
- **Creation**: welding e991684's `MacosCefProducer::new` refuses `user_data_dir` because CEF
  initializes an on-disk request context asynchronously on macOS. The player calls
  `prepare_profile`, pumps `do_message_loop_work` (and SDL events) until
  `try_new_with_prepared_profile` returns the producer, then loads the game. The isolated
  launch takes the same path with `game-profile` under its per-run cache, as Windows does.
  A close request or `ROVES_GATE_TIMEOUT_SECS` before the context is ready fails the run.
- **Refusals** (before CEF starts, `gate: FAIL ...` then `gate: exit code=N`): profile held
  by another instance -> exit 6; stored port taken by another program -> exit 6 (another
  port would silently show an empty game); damaged `origin-port` -> exit 1 (never replaced
  by a new origin). No native dialog yet (Windows shows an SDL message box); log only.
- **Switch**: `--roves-isolated-profile` (packaged game only, otherwise exit 1) keeps the
  previous behaviour: a per-run cache under the temporary directory, removed at exit, no
  file under the persistent root. Any other `--roves-*` argument is rejected. The fixture
  gate always uses a per-run cache.
- **Keychain**: `--use-mock-keychain` stays (unattended runs). Whether cookies encrypted
  with the mock keychain in one launch decrypt in the next was **not checked** in the
  Chromium source or in CI; Local Storage and IndexedDB, which are not keychain-encrypted,
  do not depend on it. The distribution choice is still open ("Open risks").

Chromium reference (pinned `f2700724f59f`, sparse paths `chrome/common/chrome_paths_mac.mm`
and `chrome/browser/process_singleton_posix.cc` added):

- `chrome_paths_mac.mm:72-88`: the default user-data directory is
  `base::DIR_APP_DATA` (`~/Library/Application Support`) plus a product name from the outer
  bundle's `Info.plist`. The player uses the same parent; the name is per install instead of
  per product so different games stay apart, as on Windows.
- `process_singleton_posix.cc:480-512` (`ReplaceOldSingletonLock`, macOS): Chrome's
  profile lock relies on `flock(LOCK_EX | LOCK_NB)` being dropped by the kernel when the
  owner dies, which is the property `player.lock` uses. Chrome's current `SingletonLock`
  symlink (hostname-pid) and socket also forward the second launch's command line to the
  running instance; the player has nothing to forward and refuses the second launch.

## Local MCP bridge

Development only, off by default: the Windows player's local MCP bridge
(`support/cef-netrender-probe/src/mcp.rs` and `mcp_rules.rs` through `#[path]`; Windows
player README "Local MCP bridge"; client instructions in
`support/cef-netrender-probe/mcp/README.md`). A packaged-game launch starts it with
`--roves-mcp`, or when the package contains `Contents/Resources/mcp/enabled` inside the
`.app` (only the MCP-enabled development package has it); `--roves-mcp` without a packaged
game is a usage error. Off means no socket, no session record and no `gate: mcp` line.

- **Transport.** Before the browser is created `session.rs` starts the bridge: the per-user
  session directory `~/Library/Application Support/Roves/mcp` is created with mode 0700 (or
  must be a real directory owned by the user and is tightened to 0700), a unix-domain socket
  `<pid>.sock` (0600, tokio `UnixListener`) is bound there and the record `<pid>.json`
  (0600; `pipe` holds the socket path, plus the 256-bit `token` from `getrandom` and the
  game `origin`) is written through a temporary file and a rename. Connections whose peer
  credentials (`getpeereid`) name another uid are closed unread; every other connection must
  first send the token. Socket and record are removed at the end of the loop (`gate: mcp
  session removed calls=<n>`). Log lines: `gate: mcp enabled source=Switch|Package
  pipe=<socket> record=<path>` (never the token) or `gate: mcp unavailable source=...
  error=...` (the game still runs, without the marker).
- **Requests** run in the player loop, at most 4 per turn: `state` (origin, URL, title,
  `loaded`, `page_capability`, `startup` `loading|released` (first game frame shown),
  `input_ready`, window size/pixels/scale, imported frames, input counters, last 50 console
  lines); `evaluate` over the same in-process DevTools channel as the page markers (CDP
  `Runtime.evaluate`, CSP kept, Promise refused; refused before the game URL was requested);
  `click`/`press_key`/`type_text` as SDL events through the same `Input` as real input,
  refused before the accepted game load-end (as real input; the startup screen may still be
  up, and on a runner without accelerated frames the game never leaves it).
- **Page marker**: with the bridge running the page-marker script also defines the read-only
  `window.__ROVES_MCP__ === true` in the top-level game document (`gate: page markers
  registered ... mcp=true`); without it, `mcp=false` and the marker is absent.
- **MCP-enabled package**: the standard package plus `Contents/Resources/mcp/`
  (`server.mjs`, `README.md`, `package.json`, `package-lock.json`, `node_modules/` from `npm
  ci --omit=dev`, and `enabled`); the executable `Contents/MacOS/roves-cef-macos-gate` and
  the packaged template are byte-identical to the standard package's. CI uploads it as
  `cef-macos-gate-arm64-mcp` (release asset `roves_shell_macos_mcp.zip`).

CI (`.github/workflows/cef-macos-gate.yml`, after the Steam variant): the stand-alone
`mcp_rules` tests (including the unix directory 0700/owner and record 0600 test), then
`smoke.mjs` (`support/cef-netrender-probe/mcp/smoke.mjs`) three times. **fixture**: the MCP
fixture temporarily as the MCP package's game, launched with `--roves-mcp`: record and
socket in the 0700 directory (both 0600), unauthenticated connections answered with nothing,
discovery by PID, state, the `isMcpEnabled()` marker (entry script, top level only),
evaluation, a trusted click, ArrowRight keydown/keyup, typed text, record and socket
removed, token not logged. **disabled**: the standard package on the same fixture without
the switch, and with `--roves-mcp` while the session directory cannot be created (a file
stands at the session directory's path (only when it is absent or empty, removed
afterwards)): marker absent, host marker present, no record. **package**: the MCP package
launched directly with the template, discovered without a PID. Exit 0, or 2 when the runner
gives no frame (the bridge checks still run). Not verified yet: any run of these steps; a
real MCP client session on a Mac/Linux desktop.

## Verdicts

The main binary prints `gate:` lines on stdout and exits with:

| Exit | Fixture gate | Packaged game |
| --- | --- | --- |
| 0 | An imported frame's centre 64x64 block matched dodger blue `[255,144,30,255]` BGRA ±8, renderer sandboxed, browser closed | Accepted load-end, a frame after it, close requested, last frame not a single colour, renderer sandboxed, browser closed, server stopped |
| 2 | `NO_ACCELERATED_FRAME`: no `OnAcceleratedPaint` before the timeout, nothing failed | Game loaded but no accelerated frame at all before the timeout |
| 6 | - | Before CEF starts: persistent profile held by another instance, or its stored origin port taken |
| 1 | Any `gate: FAIL` (setup, non-Metal interop, `acquire_frame`/readback, wgpu error, content process terminated, no match, renderer missing or unsandboxed, no `OnBeforeClose`) | Any `gate: FAIL`, plus: load-end rejected or missing, load error, off-origin commit, close before the first game frame, server stop error, damaged `origin-port`, game profile not ready, unknown `--roves-*` switch |

Every run ends with `gate: exit code=<N>`, including the refusals that never start CEF.

The fixture is a `data:` page (blue, plus a 1 px corner element animated with
`requestAnimationFrame` so CEF keeps painting). The uniform colour proves the frame's
content arrived, not ordering or tearing.

## CI (`cef-macos-gate.yml`)

`macos-26` (Apple silicon; `macos-15` until 2026-11-26), push on this directory or the workflow, or manual. Steps: rustc
1.99.1 check and Ninja (`cef-dll-sys` uses CMake's Ninja generator); cargo/CEF cache;
dependency graph check; `cargo build --locked --release --bins`; `target/release/bundle`
and a structure check (main executable, framework, `libcef_sandbox.dylib`, five helpers);
one sandboxed run with a 150 s hard stop. The run step fails on any `gate: FAIL`, a missing
`mode=fixture`, `cef initialized sandbox=Sandboxed`, Metal adapter, `mode=AcceleratedPaint`,
`PASS sandbox`, `browser closed`, `cef shutdown complete` or `shutdown complete exit=<code>`
line, an exit other than 0/2, or a gate process alive after exit. Exit 2 is a notice:
GitHub's macOS runners are virtual machines with a paravirtual GPU, and Chromium may disable
GPU compositing there (as WARP does on Windows); run 45575923557 got exit 0.

Packaged game, on the same `.app` (no second build): checkout of
`DRincs-Productions/visual-novel-template` at `92e5115ac52de37202b2ebd657053da8322450e3` (the
Windows package's pin), Node 22, `npm ci` + `npm run build`, provenance in
`vnt-build-info.txt`; `dist/` copied with `ditto` into `Contents/Resources/game` and compared
with `diff -r` (byte-for-byte, SHA-256 list as an artifact). One run with
`ROVES_GATE_AUTO_CLOSE_MS=8000` and the 150 s hard stop. Required, in this order: CEF
sandboxed, Metal adapter, producer on `<origin>/`, `load-end status=200 url=<origin>/
ok=true`, game loaded, fresh frame requested, first game frame (`Bgra8Unorm`), auto-close
scheduled and pushed, loop end with `loaded=true game_frame=true close=quit`, close
requested, last game frame, frame dump, `PASS sandbox`, `PASS packaged game`, browser
closed, CEF shutdown, loopback stopped, cache removed, exit 0. Also: no `gate: FAIL`, no
`DevTools listening` line, cache directory gone, loopback port no longer accepting, no
process left. If the fixture run and the player both give exit 2, only load and close are
required (notice). Artifacts: `cef-macos-gate-logs` (both run logs, `player-last-frame.bmp`,
template build logs and provenance, packaged-file hashes) and `cef-macos-player-arm64` (the
zipped `.app`, with the game when packaging ran; unsigned apart from the linker's ad-hoc
signature). Never gates a release.

That packaged run passes `--roves-isolated-profile` and additionally requires
`player profile=isolated cache=`, `game profile ready dir=.../game-profile`, `cache removed`
and `exit code=0`, and no persistent-profile line. A following step mirrors the Windows
package's persistent-profile step on the same `.app`: two ordinary launches (auto-close
3000 ms) from an unrelated working directory, each exiting 0 (or 2 when the fixture gave 2)
with, in order, the profile root line, the origin, `profile=persistent cache=<root>/cef`,
CEF init on that cache, `game profile ready dir=<root>/cef/game-profile`, an accepted
load-end, browser closed, CEF shutdown, loopback stopped, `player profile kept`, exit lines;
no `FAIL`, `cache removed` or DevTools line. Across launches: the root is
`~/Library/Application Support/Roves/Player/roves-cef-macos-gate-<16 hex>`, the first starts
it (`origin-port=new`, port stored), the second reuses the same root and port without
rewriting it, both origins are equal, `origin-port` holds that port and `cef/` and
`cef/game-profile` exist after exit (entries listed as notices). Then, with CEF never
initialized: `player.lock` held by a perl `flock` -> exit 6 `FAIL player profile busy`; the
port held by a perl listener -> exit 6 `FAIL game loopback port busy port=<port>`; a damaged
`origin-port` -> exit 1. No process may remain; the profile is removed at the end. Logs in
`cef-macos-gate-logs` (`player-profile/*.log`). A game save written in one launch and read
in the next is not checked (no page injection; manual check, as on Windows).

Local checks done on Windows (light only, as required):
`cargo check`/`cargo clippy --target aarch64-apple-darwin --no-default-features --features dox --bins`
(clean, 1.99.1), `cargo fmt --check`, YAML parse. Packaged-game slice: the same clippy with
`-D warnings`, `cargo metadata --locked`, the CI graph check (wgpu 29 users, wgpu 30 only
under cef), `bash -n` of every run script, and the player step's ordered log check run on a
simulated log (passes; a removed `loopback stopped` line and a wrong exit code both fail).
Persistent-profile slice: the same clippy `-D warnings` and fmt, YAML parse, `bash -n` of every
run script, and the new step's root/origin/port extraction and root pattern on a synthetic
log (accepts a valid root, rejects a malformed one). They type-check the macOS code paths of
welding, objc2 and wgpu Metal; they do not build CEF or SDL, link or run anything.

**Player fixture** (after the persistent-profile step): the packaged template is swapped for
`player-fixture/` (restored afterwards) with the repository `icon.png` as
`Contents/Resources/icon.png`, and run once with a fresh persistent profile,
`ROVES_GATE_TEST_INPUT_MS=1000` and `ROVES_GATE_AUTO_CLOSE_MS=6000`. Required, each in log
order: project window icon; producer on `about:blank`, `page markers registered`, game
navigation, accepted load-end; `roves-ci-marker available=true` (the frozen
`window.__ROVES__`) and `roves-ci-save os_type=macos available=true write=true
read=hello-roves`; the document title; the click (forwarded down/up, trusted DOM click on the
button, `detail=1`), the ArrowRight press (`windows_key_code=0x27 native_key_code=0x007C`,
DOM `key=ArrowRight code=ArrowRight keyCode=39` for keydown and keyup, no keypress) and the
wheel notch (CEF delta `(0, 120)`, one trusted DOM wheel scrolling up); `PASS test input`;
clean close; exit 0, or 2 when the fixture gate had no accelerated frame. The click starts a
WebAudio oscillator whose `AudioContext` state is a notice only. External links (run with
`ROVES_TEST_EXTERNAL_OPEN_LOG=1`, so no browser starts): the click handler assigns
`location.href` to an https URL on example.com (vetoed, then `external link opened
url=https://example.com via=navigation launched=false`) and 2 s later, still inside the
click's user activation, calls `window.open` on example.org (`... url=https://example.org
via=popup launched=false`); exactly two opens, never one for the load-time `window.open` on
example.net without a gesture (refused as `no-user-gesture` when Chromium reports it), no
other refusal, a single load-end, the page's `roves-ci-external still-here same-href=true`,
and no path or query in the player's own lines.

**Icon, startup screen and Steam** (sections "Application icon", "Startup screen", "Steam"):
the bundle step builds `roves-cef-macos-gate.icns` from the repository `icon.png`
(`packaging/icns.sh`), bundles with `--resources`, and requires
`Contents/Resources/roves-cef-macos-gate.icns`, `CFBundleIconFile` set to it, and no
`libsteam_api` in the standard `.app`. The fixture gate must log no startup-screen line. The
isolated packaged run additionally requires `startup logo decoded`, `wordmark rendered ...
font=MetalMania`, no `gate: steam` line and, on exit 0, `startup screen released` between
`fresh frame requested` and the first game frame; the player fixture requires the release
after its load-end on exit 0. Then the Steam variant: `cargo build --release --bins
--features steam` in the same target directory (log `gate-steam-build.log`), its own `.app`
with the same icon and template, `Contents/MacOS/libsteam_api.dylib`, `otool -L` of the main
executable showing `@loader_path/libsteam_api.dylib` and of every helper showing no
`libsteam_api`. One isolated run (auto-close 3000 ms) must exit like the standard isolated run
with, in order, `steam unavailable reason=...`, the packaged-game origin, isolated profile,
sandboxed CEF, accepted load-end, browser closed, CEF shutdown, loopback stopped, exit line,
and no `FAIL`, `steam initialized`/`steam shutdown complete` or DevTools line
(`player-steam-run.log`). Artifact `cef-macos-gate-arm64-steam`: the zipped Steam `.app`.

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
- **Shutdown**: welding's macOS producer has no close-completion API (Windows has
  `is_closed`); the gate waits for its `CEF on_before_close` debug record (section
  "Packaged game"). A small upstream addition (`is_closed` on `MacosCefProducer`, as on
  Windows) would remove the log dependency; not proposed yet.
- **Real close buttons**: CI closes through a pushed SDL quit event. The red window button
  and Cmd+Q reach SDL as `NSEvent`s/`NSApp` delegate calls, and `CefDoMessageLoopWork` may
  drain the `NSApplication` queue itself (Chromium reference above), so whether SDL sees
  them is a manual check on a real Mac, not verified here.
- **Keychain**: the gate passes `--use-mock-keychain` (unattended runs would otherwise block
  on Chromium's Safe Storage prompt). The player needs a deliberate choice before
  distribution; with the persistent profile the choice also decides whether cookies stay
  readable across launches and across a later switch to the real keychain.
- **App Translocation**: Gatekeeper runs a quarantined app that the user did not move from a
  random read-only path. The profile name follows the `.app` path, so each such launch
  would get a new, empty profile. The player prints `gate: WARN app is translocated` and
  continues; signing/notarisation and installing into `/Applications` avoid it. A bundle-id
  based name would avoid it too but would let two games built from this gate share storage.
  Not exercised in CI (the runner's `.app` is not quarantined).
- **Physical input on a real Mac**: CI pushes SDL events (`SDL_PushEvent`), which never pass
  through AppKit. Whether real `NSEvent`s reach SDL while `CefDoMessageLoopWork` also pumps
  the `NSApplication` queue (Chromium reference above), and IME/dead-key behaviour, are
  manual checks on a Mac.
- **Audio on a real Mac**: the CI runner may have no output device; the fixture's
  `AudioContext` state is reported, not required.
- **Saves across launches**: CI proves the same profile and origin across launches, not that
  a real game's save survives; manual check on a Mac.
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

## Steam variant: open points

Implemented (section "Steam"). Still to verify, not assumed: the Steam overlay must hook the
process that presents, which here is the main process (wgpu Metal/Vulkan on the SDL3
window), not a CEF helper; on Linux the Steam Linux Runtime container (pressure-vessel) may
restrict the namespaces Chromium's sandbox needs. Each platform keeps a standard and a Steam
variant from the same build inputs, as the Windows release matrix does.
