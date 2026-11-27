# Roves CEF player for Windows (CEF accelerated OSR -> wgpu-weld -> wgpu)

The Windows desktop player of Roves. Chromium (through CEF 154, its own V8 running the game)
paints into a GPU texture that welding (merely-made/wgpu-weld) and grafting hand to a
Roves-owned wgpu 29 DX12 device, without a CEF fork, without a CPU copy on the presentation
path and with the Windows sandbox on. SDL3 owns the window, the event pump and input; SDL3
mouse, wheel, keyboard and text events are forwarded through welding. Without an importable
GPU the player falls back to CEF's software paint ("Software paint" below). The macOS and
Linux players (`support/cef-macos-gate`, `support/cef-linux-gate`) reuse its pure modules
through `#[path]`; `release.yml` publishes all three.

History: the prototype also composed the CEF frame with Netrender (Vello) to compare it with
a plain wgpu blit. That comparison ended on 2026-11-26 and the Netrender path, its
shared-device proof and their receipts were removed; the git history keeps them. The
`--roves-sustained` warmup-stall diagnostic it produced remains: when a sustained sample is
rejected, the player prints `stall-trace` lines (the last 128 redraws, `src/stall.rs`) and a
`stall-mailbox` line (one observational poll of welding's mailbox), and forwards welding and
grafting `warn`/`error` log records as `upstream-log` lines.

## Real-game mode (`--roves-game-dir`, opt-in, implemented; CI loading gate only, no real game run yet)

Purpose: play an **unmodified built game** through the same CEF accelerated OSR -> welding ->
wgpu path, and capture presentation with PresentMon next to Chrome. Without
`--roves-game-dir` nothing changes: fixture, checks, timings, sustained mode, defaults and the
CI gates are the same code paths. The only shared-path additions are a `popup refused` log
line (the fixture opens none; game mode logs `external link ...` instead, "External links"
below) and, on a winit event-loop error, running the normal cleanup before exit 1.

What it does (`src/game.rs`, `src/game_rules.rs`, hooks in `src/app.rs`; the SDL3 -> welding input path is `src/input.rs`, shared with the macOS and Linux players through `#[path]`, with the Win32 key codes in `src/keys.rs`; the window-icon decoding is `src/window_icon.rs`, also shared):

- **Loading.** The stage-1 window probe's loopback server
  (`support/cef-windows-player/src/server.rs`, compiled here with `#[path]`:
  axum + tower-http `ServeDir`, bound to `127.0.0.1` only, `..` rejected, MIME and Range from
  the library, plus the conservative cache policy in "Loopback cache policy" below) serves the
  directory, bound before CEF starts. Port 0 (default) gives each run
  its own origin; `--roves-port=<n>` fixes it. The producer opens `http://127.0.0.1:<port>/`
  with an opaque white background (Chrome's default; the fixture stays transparent) and is
  composed by the wgpu blit (this branch has no other composition). The directory must contain
  `index.html`. The server stops after `cef::shutdown`.
- **Profile.** CEF's root cache is `--roves-cache-dir` (default: a fresh directory under
  `%TEMP%` per run, made absolute) and the page gets welding's per-producer on-disk profile at
  `<cache-dir>\game-profile` (welding requires it inside the root cache). Every default run is
  therefore isolated and starts empty; reusing the same `--roves-cache-dir` **and**
  `--roves-port` keeps `localStorage`/IndexedDB saves (storage is keyed by origin). The
  packaged launch does this automatically ("Saves and the player profile" below).
- **Origin and security.** No script is injected, no `execute_script`, no host object or
  message bridge is exposed (welding's renderer handler only answers host-initiated
  evaluation requests, which game mode never sends); there is no Deno in this process. The
  sandbox path is the probe's (bootstrap.exe broker, `CefSandboxMode::Sandboxed`, exit 3
  without it). **Single document**: the producer is created with welding's main-frame
  navigation veto (`CefSurfaceConfig::main_frame_navigation_policy`, CEF `OnBeforeBrowse`;
  vendored welding change 2) and the policy `external_link::main_frame_policy`: main-frame
  navigations on `http://127.0.0.1:<port>` (exact origin match, unit-tested: the game URL,
  same-origin links and same-origin reloads) proceed, `about:blank` proceeds only as the
  page-marker bootstrap before the game started, everything else (another origin, `file:`,
  `data:`, `javascript:`, a server redirect off the origin, `about:blank` later) is cancelled
  **before its request is sent** and the game document stays. A main frame that still commits
  off the origin (a backstop; the veto should make it impossible) is logged as
  `FAIL game main frame left the loopback origin: <url>` and the browser is closed (exit 1).
  History API and fragment routes stay on the origin and are logged as `address <url>`. A
  main-frame `load-end` that is not 2xx/304 or not on the origin is a `FAIL`; the first
  accepted one marks the game loaded.
- **External links** (`src/external_link.rs`, shared with the macOS and Linux players through
  `#[path]`, unit-tested in `support/cef-host-api-tests`). Popups (`window.open`,
  `target=_blank`) stay refused by welding (`NewWindowRequested`) and vetoed off-origin
  navigations arrive as `NavigationBlocked`; for both the player decides: the URL is opened in
  the system browser (`open::that_detached`, the `open` crate: `ShellExecute`-equivalent
  `Start-Process` on Windows, `/usr/bin/open` on macOS, `xdg-open` on Linux) only when it is
  `http`/`https` (with a host) or `mailto`, not on the game origin, the navigation carries
  CEF's user-gesture flag (a real click on a link or a script running inside its user
  activation), and no other external open happened in the last second. Everything else
  (`file:`, `javascript:`, `data:`, custom schemes such as `steam://`, script-initiated opens
  without a gesture, a burst) is refused. Log lines carry the URL reduced to scheme and host,
  never its path or query: `external link opened url=https://example.com via=popup|navigation
  launched=true`, `external link refused url=... via=... reason=scheme|no-user-gesture|
  rate-limited|game-origin|invalid-url`, `external link open failed ... error=...`. Test-only
  environment `ROVES_TEST_EXTERNAL_OPEN_LOG=1` (CI): accepted links are logged with
  `launched=false` and no browser is started.
- **Input** (welding's maintained `CefSurfaceProducer` API; physical pixels, welding converts
  to DIP): cursor moves, left/middle/right buttons with held-button modifiers, wheel
  (`LineDelta` x 120 = `WHEEL_DELTA` per notch, `PixelDelta` as is), Shift/Ctrl/Alt/Meta.
  Keyboard: `RawKeyDown`, one `Char` per typed BMP character (`wParam` = the character), and
  `KeyUp`; `windows_key_code` is the virtual key of the SDL keycode (`VkKeyScanW` for
  characters; for keycodes without a character, `SDLK_SCANCODE_MASK | scancode`, a fixed
  table unit-tested in `src/game_rules.rs`) with left/right modifiers folded to
  VK_SHIFT/VK_CONTROL/VK_MENU (what a real `WM_KEYDOWN` carries). CEF 154 derives the DOM
  `key` only from this value (`PlatformKeyMap::DomKeyFromKeyboardCode`), so a key missing
  from the table reaches the page as `key="Unidentified"`, `keyCode` 0; the table covers the
  keys SDL 3.4.16's Windows scan-code table produces that have a fixed Win32 virtual key
  (navigation, arrows, keypad, F1-F24, CapsLock, ScrollLock, Pause, PrintScreen,
  ContextMenu, modifiers, volume/media/browser keys, the Japanese Convert/NonConvert keys:
  scan `0079`/`007B`, VK_CONVERT/VK_NONCONVERT, `key` `Convert`/`NonConvert` on every
  layout in Chromium's `platform_key_map_win.cc`). Left unmapped because Chromium's `key`
  for them is layout-dependent or printable: KanaMode, Lang1/Lang2, IntlRo, IntlYen,
  NumpadComma, NumpadEqual; Help stays unmapped too. Not reproduced (unverified on hardware):
  the keypad's NumLock-off virtual keys (SDL reports `SDLK_KP_*` either way, so Numpad6 stays
  VK_NUMPAD6), Ctrl+Pause as anything but Pause (SDL's table maps both scan codes `E046` to
  `SDL_SCANCODE_PAUSE`), and AltGr as `AltGraph` (welding's `EventModifiers` has no CEF
  extended-key flag).
  `native_key_code`
  is a **Windows scan code**, not an `lParam`: CEF 154
  (`CefBrowserPlatformDelegateNativeWin::TranslateUiKeyEvent`, CEF commit 14c5a089e845,
  `libcef/browser/native/browser_platform_delegate_native_win.cc`) passes it unchanged to
  `ui::KeycodeConverter::NativeKeycodeToDomCode`, an exact lookup in the Windows column of
  Chromium's `dom_code_data.inc`: make code with `0xE0` in the high byte for extended keys
  (ArrowRight `E04D`, Numpad6 `004D`, PrintScreen `E037`, Help `E03B`, Pause `0045`). A value
  matching no entry leaves `KeyboardEvent.code` empty (CI run 45075636992 on `34cb3726e95`:
  the former `lParam` value gave `key=ArrowRight keyCode=39 code=""`). Sources, in order:
  Pause is always `0045` (SDL 3.4.16's Windows backend rewrites it to `E046`/`E145`);
  otherwise SDL's `SDL_KeyboardEvent.raw`, which that backend fills with exactly this form, is
  used unchanged (welding forwards it to CEF unchanged too). Only when `raw` is 0 (no scan
  code, e.g. an event pushed into the queue) the code falls back to
  `MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX)` and, if Windows returned no prefix, adds `0xE0`
  for a fixed list of SDL scancodes validated against Chromium's table (Insert..ArrowUp,
  NumLock, NumpadDivide, NumpadEnter, ContextMenu, MetaLeft/Right, ControlRight, AltRight).
  The fallback is limited: Windows maps VK_RIGHT to the keypad's `4D`, so the prefix comes
  from the SDL scancode; keys whose virtual key maps to a different low byte cannot be
  rebuilt (VK_HELP gives `0063`, not `E03B`; PrintScreen likewise) and keep whatever Windows
  returns, 0 = `code` empty. `Char` events carry `native_key_code` 0 (SDL text has no scan
  code; the former 1 was Escape's). Focus is given to the browser after the first imported
  frame and on every `Focused(true)`; `Occluded` maps to `set_visible`; `ScaleFactorChanged` to
  `set_scale_factor`; the page's cursor shape is applied to the window (hidden for `none`).
  Resize is the probe's existing path (surface reconfigure + `producer.resize`). User input
  (mouse motion, buttons, wheel, key down/up, text input) is **discarded until the accepted
  game load-end has been handled**: before CEF's `on_after_created` welding has no browser
  host and refuses page input, and there is no page to receive it. Evidence: CI run
  45073519738 logged `input rejected` for a welding mouse-input operation before
  `load-start`; that log does not say which SDL mouse event produced it. The exact event is
  unknown — most likely the initial cursor motion SDL queues for the window under the cursor —
  and the new `event=<kind>` log below will confirm it on the next rerun. Window lifecycle, visibility, focus, scale, resize
  and close events are always handled (welding stores visibility/scale for the browser it is
  creating); the test hooks push their events only after load-end. The rule is unit-tested in
  `src/game_rules.rs` (`forward_input`). The first discarded event is logged as `input
  discarded before the game load-end ... event=<kind>`; after load-end nothing is discarded
  and any welding refusal is logged once as `input rejected` (a CI failure). Both are counted
  at close: `game input forwarded=<n> rejected=<n> discarded_before_load_end=<n>
  console_messages=<n>`.
- **Minimized/hidden window (every mode).** While SDL reports the window minimized or
  hidden the host does no wgpu frame work (no import, compose, acquire or present); the loop
  waits on SDL events for at most 16 ms per turn and still handles CEF events, the load
  timeout, auto-close and OS close. In game mode `Minimized`/`Hidden`/`Occluded` also call
  `set_visible(false)` (CEF `WasHidden`), and `Restored`/`Maximized`/`Shown`/`Exposed`
  `set_visible(true)`. Resume is decided from the window flags, re-reads the pixel size
  (changes while suspended are deferred) and re-presents the last imported frame at once;
  welding's latest-frame slot then delivers the newest paint. Log lines:
  `frame work suspended reason=... phase=... imported=<n> presented=<n>`,
  `frame work resumed reason=... suspended_ms=<n> idle_ticks=<n> ...`,
  `resume first-present after_ms=<n> fresh_import=<bool> generation=...` and
  `resume first-import after_ms=<n> generation=<n>`. Occlusion alone keeps presenting.
  A suspension during the `--roves-sustained` warmup/sample rejects it (as a resize does).
  Chromium f2700724 reference: `ui/compositor/compositor.cc` `Compositor::SetVisible` and
  `components/viz/service/display/display_scheduler.cc` `SetVisible`/`ShouldDraw` stop
  drawing while the display is invisible but keep the message loop running;
  `ui/views/win/hwnd_message_handler.cc` `OnSize(SIZE_MINIMIZED)` drives it on Windows;
  native occlusion throttling (`ui/aura/window_tree_host.cc`) is a separate, feature-gated
  path not reproduced here.
- **Scripted minimize/restore (`--roves-test-minimize-ms=<ms>`, tests only).** Game mode
  only. `<ms>` after the accepted load-end the probe calls `SDL_MinimizeWindow`, waits until
  frame work is suspended, holds it suspended for 1500 ms, then calls `SDL_RestoreWindow`
  and waits for the resume; the window manager reports both transitions through ordinary
  SDL window events, so suspension and resume go through the same path as a user's
  minimize. The cycle is advanced from the loop's deadline handling, which also runs on
  suspended turns: the restore request is itself evidence that deadlines are serviced while
  suspended. It fails (`FAIL test minimize ...`, then close) if frame work is suspended
  before the scripted minimize, is not suspended within 10 s of the request, resumes during
  the hold, is not resumed within 10 s of the restore request, or if either SDL call fails;
  an unfinished cycle at exit is also a `FAIL`. `--roves-auto-close-ms` waits for the cycle
  to finish. Log lines: `test minimize requested ok=<bool> hold_ms=1500`,
  `test restore requested ok=<bool> idle_ticks=<n>`, `test minimize cycle complete`. Not
  meant for players; the flag only exists to make the suspension path testable in CI.
- **Synthetic click (`--roves-test-click-ms=<ms>`, tests only).** Game mode only, and not
  together with `--roves-test-minimize-ms` or `--roves-test-key-ms` (one test hook per run).
  `<ms>` after the accepted
  load-end the probe pushes one left `MouseButtonDown` + `MouseButtonUp` at SDL window point
  (100, 60) (logical coordinates) into the SDL event queue with `SDL_PushEvent` (sdl3 0.20
  `EventSubsystem::push_event`), stamped with a sentinel mouse id (`0x524F5645`). They come
  back through the loop's ordinary `poll_iter` -> `window_event` -> `Input` -> welding
  `send_mouse_input` path (CEF `SendMouseClickEvent`, click count 1); only events carrying
  the sentinel id count for the test. The page must log `roves-test-click-received` (alone or
  followed by a space) from a click handler on the loopback origin. Required, in order:
  request pushed, button down forwarded, button up forwarded (welding accepted each call),
  then the marker; anything else fails (`FAIL test click ...`, then close): a push error, a
  welding refusal, up before down, extra synthetic events, the marker before the click was
  forwarded or more than once, or any step not done within 10 s of the request; an
  unfinished sequence at exit is also a `FAIL`. `--roves-auto-close-ms` waits for it. Log
  lines: `test click requested x=100 y=60 window_id=<n> pushed=<bool>`,
  `test click forwarded event=down|up ok=<bool>`, `test click marker received`,
  `test click complete`. **Coverage and limits:** this exercises the SDL queue, the probe's
  SDL -> welding mouse translation and CEF/Blink delivery of a trusted DOM `click` to the
  element under the point. It is a synthetic SDL event, not a physical mouse or the Win32
  message path SDL builds events from; it proves no gameplay, no visual result and no
  presented frame. The point maps to CSS pixels only through the window/display scale (the
  CI fixture's 200x120 px target tolerates scales up to about 1.6). Mouse motion, the other
  buttons, keyboard, text input, wheel and browser focus are **not** covered by this hook.
- **Synthetic key (`--roves-test-key-ms=<ms>`, tests only).** Game mode only, and not
  together with `--roves-test-minimize-ms` or `--roves-test-click-ms` (one test hook per run,
  so a result is attributable to one hook). `<ms>` must be a whole number from 0 to 600000
  (anything else is a usage error). `<ms>` after the accepted load-end the probe pushes one
  `KeyDown` + `KeyUp` with SDL keycode and scancode `Right` (ArrowRight: layout-independent,
  no character), no modifiers, `repeat=false` and a sentinel keyboard id (`0x524F5645`) into
  the SDL event queue with `SDL_PushEvent` (sdl3 0.20 `EventSubsystem::push_event`, whose
  `to_ll`/`from_ll` round-trip key events). They come back through the loop's ordinary
  `poll_iter` -> `window_event` -> `Input` -> welding `send_keyboard_input` path, the same
  translation as a real key (`RawKeyDown`/`KeyUp`, VK_RIGHT). The pushed events have `raw` 0,
  so `native_key_code` takes the fallback: `MapVirtualKeyW` gives `4D` (Numpad6) and the SDL
  scancode `Right` restores `E04D`; physical keys use SDL's `raw` instead. Only events
  carrying the sentinel id count
  for the test. The page must log `roves-test-key-received type=keydown ...` and then
  `roves-test-key-received type=keyup ...` from the loopback origin. Required, in order:
  request pushed, key down forwarded, key up forwarded (welding accepted each call), the
  keydown marker, the keyup marker; anything else fails (`FAIL test key ...`, then close): a
  push error, a welding refusal, up before down, extra synthetic events, a marker before the
  key was forwarded, keyup before keydown, a repeated marker, a marker with any other `type=`
  (for example `keypress`), or any step not done within 10 s of the request; an unfinished
  sequence at exit is also a `FAIL`. `--roves-auto-close-ms` waits for it.
  `--roves-test-key=ArrowRight|CapsLock` (exact case, only together with
  `--roves-test-key-ms`, otherwise a usage error; default `ArrowRight`) selects the key: two
  fixed regression keys, not a general key-injection API. `CapsLock` pushes SDL keycode and
  scancode `CapsLock` with `raw` 0, the same way: `non_character_virtual_key` gives
  VK_CAPITAL (`0x14`) and `MapVirtualKeyW` gives scan code `3A` with no `0xE0` prefix, which
  Chromium f2700724 maps to DOM `code` `CapsLock` (`ui/events/keycodes/dom/dom_code_data.inc`
  Windows column `0x003a`) and `key` `CapsLock` (`ui/events/keycodes/platform_key_map_win.cc`
  `{VKEY_CAPITAL, DomKey::CAPS_LOCK}`), `keyCode` 20. Log lines:
  `test key requested key=Right|CapsLock window_id=<n> pushed=<bool>` (the SDL key name),
  `test key codes event=down|up raw=<hex> usage=<hex> windows_key_code=<hex>
  native_key_code=<hex>` (diagnostic, not gated), `test key forwarded event=down|up ok=<bool>`,
  `test key marker received kind=Down|Up`,
  `test key complete`. The DOM fields (`key`, `code`, `keyCode`, `repeat`, `isTrusted`) are
  asserted by the CI fixture, not by the probe. **Coverage and limits:** this exercises the
  SDL queue, the probe's SDL -> welding key translation and CEF/Blink delivery of one trusted
  `keydown`/`keyup` pair. It is a synthetic SDL event, not a physical keyboard or the Win32
  message path SDL builds key events from. It proves no text input, `Char`/`keypress`, IME,
  modifier, lock-state toggle (the CapsLock press sends no modifier flags and the probe
  tracks no lock state), auto-repeat or other-layout behaviour. Nor does it prove browser focus: Chromium
  f2700724 forwards key events without a focus check
  (`RenderWidgetHostImpl::ForwardKeyboardEventWithCommands`) and Blink falls back to the main
  frame when no frame is focused (`WebFrameWidgetImpl::HandleKeyEvent` ->
  `FocusController::FocusedOrMainFrame`), and on WARP the probe never gives focus (no
  imported frame). No gameplay, visual result or presented frame is proved.
- **Synthetic wheel (`--roves-test-wheel-ms=<ms>`, tests only).** Game mode only, and not
  together with `--roves-test-minimize-ms`, `--roves-test-click-ms` or `--roves-test-key-ms`
  (one test hook per run). `<ms>` must be a whole number from 0 to 600000 (anything else is
  a usage error). `<ms>` after the accepted load-end the probe pushes one `MouseWheel` with
  `x=0 y=1` lines (one notch away from the user, i.e. up), `direction=Normal` and a sentinel
  mouse id (`0x524F5645`) into the SDL event queue with `SDL_PushEvent` (sdl3 0.20
  `EventSubsystem::push_event`). It comes back through the loop's ordinary `poll_iter` ->
  `window_event` -> `Input` -> welding `send_mouse_input` (`MouseAction::WheelScrolled`)
  path, the same conversion as a real wheel (`wheel_cef_delta`: lines x
  `WHEEL_DELTA_PER_LINE` = 120, flipped direction negated), which welding hands unchanged to
  CEF `SendMouseWheelEvent`. `Input` places the wheel at its tracked cursor (the last
  forwarded motion, initially 0,0), not at the event's `mouse_x`/`mouse_y`. Only an event
  carrying the sentinel id counts for the test. The page must log
  `roves-test-wheel-received` followed by space-separated `name=value` fields from the
  loopback origin. Required, in order: request pushed; the event forwarded with welding
  accepting the call and exactly `delta_x=0 delta_y=120`; then one marker with `type=wheel`,
  `trusted=true`, `deltaX=0`, a negative `deltaY` (Blink negates the CEF delta, so DOM
  "up" is negative) and a `deltaMode` of 0, 1 or 2. Anything else fails (`FAIL test wheel
  ...`, then close): a push error, a welding refusal, another delta or no wheel call, a second
  synthetic wheel event, a marker before forwarding, a second marker, a marker with another
  type, untrusted, a horizontal delta, the wrong direction or a missing/invalid
  `deltaY`/`deltaMode`, or any step not done within 10 s of the request; an unfinished
  sequence at exit is also a `FAIL`. `--roves-auto-close-ms` waits for it. Log lines:
  `test wheel requested x=0 y=1 direction=normal window_id=<n> pushed=<bool>`,
  `test wheel forwarded delta_x=<n> delta_y=<n> ok=<bool>` (`none` instead of the deltas when
  no wheel call was made), `test wheel marker received delta_y=<v> delta_mode=<n>` (or
  `unexpected="<reason>"`), `test wheel complete`. The DOM `deltaY` magnitude and
  `deltaMode` are **reported, not checked**. **Coverage and limits:** this exercises the
  SDL queue, the probe's SDL -> welding wheel conversion and CEF/Blink delivery of one trusted
  DOM `wheel` event with the expected direction. It is a synthetic SDL event, not a physical
  wheel or the Win32 `WM_MOUSEWHEEL` path SDL builds wheel events from; it proves no input
  latency, scrolling, visible rendering or presented frame, no horizontal wheel, no
  high-resolution/precision touchpad deltas and no parity of the delta magnitude with Chrome
  (see the Chromium reference below: a native Chrome notch is about 100 CSS px, while this
  test only checks the CEF API's delivered event and direction).
- **Lifecycle.** `--roves-timeout-ms` (default 120000) bounds only the load (exit 2 if no
  accepted load-end). Afterwards the run lasts until the OS closes the window, or
  `--roves-auto-close-ms=<ms>` after the load-end (for repeatable captures). Exit 0 = loaded,
  no `FAIL`; 1 = a `FAIL` (load error, renderer terminated, off-origin navigation, close
  failure); 2 = never loaded / close timeout. Console messages are logged as
  `console level=... source=...:line message=...` (first 500, 512 characters each).
  `--roves-sustained` and `--roves-keep-open` are refused with `--roves-game-dir` (no fixture
  checks); `--roves-port`, `--roves-auto-close-ms`, `--roves-test-minimize-ms`,
  `--roves-test-click-ms`, `--roves-test-key-ms` and `--roves-test-wheel-ms` are refused
  without it (usage error, exit 1, before the log opens).
- **Timings.** With `--roves-timings`, every redraw after the accepted load-end is recorded
  and a `timings phase=game ...` summary (same stages and format as above) is emitted when the
  window starts closing, before the close sequence. Resizes break the intervals but are not
  excluded (their frames are in the sample). Each stage keeps 65536 samples (about 18 min at
  60 presents/s); more are counted in `dropped_samples`.

Chromium reference consulted (pinned `f2700724f59f`, sparse dirs `ui/events/blink` and
`ui/events/win` added to the main checkout for this): `ui/events/win/events_win_utils.cc`
`GetScanCodeFromLParam`/`GetLParamFromScanCode` (~486-504: scan code in `lParam` bits 16-23,
extended flag bit 24 <-> `0xE0xx`): the scan-code form of the Windows column in
`ui/events/keycodes/dom/dom_code_data.inc`, which CEF 154 looks `native_key_code` up in
directly (no `lParam` decoding; see Input above). `ui/events/blink/web_input_event_builders_win.cc`
(~297-345): native Chrome scrolls `WHEEL_DELTA` notches x lines-per-notch (default 3) x
`kScrollbarPixelsPerLine = 100/3` px, i.e. 100 px per notch; `ui/events/blink/web_input_event.cc`
(~122) derives `wheel_ticks = offset / kWheelDelta` from a `ui::MouseWheelEvent` offset. CEF's
own OSR glue that turns `SendMouseWheelEvent` deltas into that offset is not in the checkout,
so how far one notch scrolls under CEF versus Chrome is **not verified**: the tick count and
direction are expected to match, DOM `deltaY` magnitudes may not. The synthetic wheel gate
(`--roves-test-wheel-ms`) used no further Chromium source (the support/chromium-reference
checkout was not fetched for it): it checks the delta the probe hands to CEF
(`delta_y=120`) and the direction of the delivered DOM event, and only reports the DOM
`deltaY` magnitude and `deltaMode`, so it is no evidence of parity with Chrome's ~100 px
native notch. For the synthetic click
gate no new Chromium source was checked out (the same pinned `f2700724` sparse dirs only):
`ui/events/blink`/`ui/events/win` build `WebMouseEvent`s from native Win32 messages in
Chrome, a path the probe never takes, because CEF's `SendMouseClickEvent` constructs the
Blink event itself; the gate therefore says nothing about parity with Chrome's native mouse
construction.

### Running a real game

```pwsh
# 1. Build the game once, unmodified (example: a Vite template; any directory with index.html).
cd D:\games\pixi-vn-react-template; npm ci; npm run build   # -> dist\index.html

# 2. Run it from the probe bundle (artifact cef-windows-player-windows-x64 of a CI run of
#    this branch). Accelerated frames need a real GPU (WARP uses software paint).
$dir = 'D:\roves-ci\<run id>-cef-sdl3'
$exe = "$dir\bundle\roves_cef_windows_player.exe"
& $exe --roves-game-dir=D:\games\pixi-vn-react-template\dist `
  "--roves-log=$dir\game.log" --roves-timings
# Play normally (click, type, scroll, resize), close the window.
# Keep saves across runs: add "--roves-cache-dir=$dir\game-cache" --roves-port=47810.
```

Read the log for `game loopback origin=... browser_pid=<pid>`, `load-end ... ok=true`,
`console ...`, `popup refused ...`, `FAIL ...` and, with `--roves-timings`, `timings
phase=game`. A manual test is valid only on the real window (screenshot it): a clean log does
not prove the page rendered or reacted.

### PresentMon comparison with Chrome

Same machine, monitor, refresh rate (record it), window size (1280x720 client area at
scale 1, or record what was used), game build, scene and input script; plugged in, nothing
else running, window visible and unobstructed; alternate the targets (probe, Chrome, then
reverse) at least twice. Use one PresentMon release for
everything and confirm its flags with `PresentMon.exe --help` (the names below are those of
the PresentMon 2.x console tool):

```pwsh
$pm = 'C:\tools\PresentMon.exe'   # PresentMon 2.x console build; record its version
$game = 'D:\games\pixi-vn-react-template\dist'
$out = 'D:\roves-ci\presentmon'

# Probe: fixed port so the scene URL is identical; close 70 s after load-end.
Start-Process $exe -ArgumentList "--roves-game-dir=$game","--roves-port=47810",
  "--roves-auto-close-ms=70000","--roves-log=$out\probe-nr.log"
& $pm --process_name roves_cef_windows_player.exe --delay 10 --timed 60 `
  --terminate_after_timed --output_file "$out\probe-nr.csv"

# Chrome on the same build and the same kind of loopback origin, fresh profile, app window.
# (Any 127.0.0.1-only static server works, e.g. Python's: python -m http.server 47811
#  --bind 127.0.0.1 --directory $game.)
Start-Process chrome.exe -ArgumentList "--user-data-dir=$out\chrome-profile-1",
  "--app=http://127.0.0.1:47811/","--window-size=1280,720"
& $pm --process_name chrome.exe --delay 10 --timed 60 --terminate_after_timed `
  --output_file "$out\chrome.csv"
```

Reading the CSVs: the probe's own swap chain lives in the **browser process** (filter rows
to the `browser_pid` from the log; CEF subprocesses share the executable name). Chrome
presents from its GPU process (DirectComposition): keep the rows of the process whose
`PresentMode`/swap chain belongs to the game window and note it. Compare the distribution of
the displayed-frame interval (`MsBetweenDisplayChange`, or the equivalent column of the
installed version) P50/P95/P99/max and dropped/not-displayed frames, plus `MsBetweenPresents`
and `PresentMode`; record per-process CPU and GPU (Task Manager or PresentMon's GPU/CPU
telemetry if the version provides it). The probe's `timings phase=game` adds host-thread
stages for the probe only; Chrome has no equivalent here. Not covered by this procedure:
input-to-photon latency (no instrumented input here), and the probe always pays the capture
copy, welding's copy and two CPU waits per frame that Chrome does not (see "Differences from
Chromium's own model"). Same-target spread across trials must be smaller than the gap before
calling a difference real; no result is claimed until such runs exist.

## Loopback cache policy and static compatibility fixture (2026-11-24)

**Problem fixed.** `ServeDir` sent `Last-Modified` and an ETag but no `Cache-Control`. For such
a 200/206 Chromium computes a heuristic freshness of `(Date - Last-Modified) / 10` and reuses
the stored copy *without contacting the server* while it is fresh. With the packaged player's
persistent profile and fixed origin, a file built 30 days before a launch stayed fresh for
3 days, so a package updated in that window could run old scripts against new ones. Second
gap: `ServeDir` ignores `If-Range`, which Chromium's partial (media) cache sends to fetch a
missing range of an entry it already holds; a stale validator still got a `206`, letting the
cache stitch old and new bytes. Negative repro on the previous server (same fixture, raw
HTTP): no `cache-control`, `last-modified` present, `Range` + foreign `If-Range` -> `206`.

**Policy** (`src/server.rs`, shared with the macOS and Linux players; still a thin
wrapper that calls `ServeDir` through `tower::ServiceExt::oneshot`):

- Every response (also 304/404/206) has `Cache-Control: no-cache`: Chromium stores it but
  always revalidates first (`HasCacheRestriction` -> zero freshness).
- `Last-Modified` is removed and `If-Modified-Since`/`If-Unmodified-Since` are ignored. The
  only validator is `ServeDir`'s strong ETag (size + nanosecond mtime) prefixed with a token
  unique to the server instance (`"r<nanos>-<pid>:<servedir tag>"`). Inside one launch a
  revalidation yields `304`; a tag from another launch never matches (`If-None-Match` -> full
  `200`, `If-Match` -> `412`), so a package replaced between launches is never confirmed, even
  when a changed file kept its size and mtime (normalised archive timestamps). Within one
  launch the residual case is a file rewritten in place with identical size and nanosecond
  mtime.
- `If-Range`: a strong tag of this instance is checked by `ServeDir` as `If-Match`; a match
  keeps the `206`, a `412` (or a date, weak or foreign validator) is answered with the full
  `200`.
- **No path is fresh or `immutable`; there is no opt-in.** Evaluated and not enabled: a
  freshness lifetime can only be safe across package updates if the URL itself is bound to
  the bytes, because a fresh entry is never re-asked. Lists in a build manifest (for example
  Vite's `.vite/manifest.json`) do not prove that: file names are configurable and Rollup's
  `[hash]` is not a digest of the final bytes, so the server cannot verify it; a pattern such
  as `-[hash].js` is a heuristic. The only verifiable rule found (the file name embeds the
  file's full SHA-256, checked by the server before granting `immutable`) needs a hashing
  dependency and matches no mainstream bundler's output, and there is no measurement showing
  that loopback `304`s cost anything. Revisit only with such a measurement.
- Cost: a new launch re-reads each requested file once into the HTTP cache (`200` instead of
  `304`), which can also make V8 drop a code cache tied to the earlier response. Not measured;
  no prefetch or other optimisation was added.

**Chromium reference** (pinned `f2700724f59fb554eb19a4f827b1d256e2a13286`; the submodule was
empty, so files were read through GitHub raw): `net/http/http_response_headers.cc`
(`GetFreshnessLifetimes` heuristic `/ 10`, `HasCacheRestriction`, `immutable` only parsed to
override `Pragma`; crbug 41253661: it does not override `LOAD_VALIDATE_CACHE`),
`net/http/http_cache_transaction.cc` (`ConditionalizeRequest`: ETag preferred, `If-Range` for
uncached ranges of a partial entry, a `206` to `If-Range` treated as "another range" of the same
object). `v8_code_cache.cc` and `preload_helper.cc` could not be fetched (HTTP 429) and were
not consulted.

**Fixture** `support/cef-windows-player/fixtures/static-compat` (no framework, no build step):
`index.html` with `<link rel=modulepreload>`, `rel=preload as=style`, `rel=preload as=fetch
crossorigin` for a 39-byte `tiny.wasm`, a module `main.js` and `range.txt`. It logs one
`roves-static-fixture check=<name> ok=<bool>` line per check (modulepreload load + import,
style preload + applied, wasm preload + `instantiateStreaming`, cache headers, each preload
fetched once by Resource Timing, `content-type` of JS/CSS/WASM, `Range: bytes=4-9` -> `206`)
and `roves-static-fixture done ok=<bool> checks=9`.

- Rust tests in `server.rs` (`#[cfg(test)]`, raw HTTP/1.1 over loopback against the fixture):
  MIME types, headers, 304 only for the same instance, Range/`If-Range`, foreign `If-Match`.
  Run locally (4/4 passed) through a throw-away crate that includes `server.rs` with `#[path]`
  and the same pins, so CEF is not built; `ROVES_STATIC_FIXTURE` overrides the fixture path.
- Chrome 157 headless (CLI, not Chrome DevTools MCP, whose use was not permitted here) on the
  new server: 9/9 checks, title `roves static fixture ok`. On the previous server the same page
  reports `check=cache-headers ok=false cache-control=null ... last-modified=true`.
- CI: `cef-windows-player.yml` step "run the probe on the static compatibility fixture
  (WARP)" runs the bundled probe with `--roves-game-dir` on the fixture and requires the
  `done ok=true` console line before auto-close, no `ok=false`, exit 0. Passed on
  `22f0c5a6914` together with the stage-1 probe's server tests: runs `45455778222` (this
  workflow) and `45455777871` (`cef-window-probe.yml`), both green on WARP; no real-GPU or
  real-game cache claim.

## Packaged player (directly playable Windows folder; implemented 2026-11-23, not yet run in CI)

`cef-windows-player.yml` also produces the artifact
`visual-novel-template-roves-cef-windows-x64`: the same Release bundle with the unmodified
production web build of
[DRincs-Productions/visual-novel-template](https://github.com/DRincs-Productions/visual-novel-template)
at commit `92e5115ac52de37202b2ebd657053da8322450e3`, laid out to be started with a double
click. It is a workflow artifact only, never attached to a GitHub Release.

```text
visual-novel-template-roves-cef\
  play.exe            CEF 154 bootstrap.exe (renamed with play.dll and play.exe.manifest),
                      Windows icon = the game's icon, else the Roves icon ("Project icon")
  play.dll            this probe (Release)
  icon.png            the game's window icon, only when the game has one ("Project icon")
  roves.ico           the official Roves icon, the fallback for tools that replace game\
  libcef.dll, *.pak, locales\, ...   CEF runtime, as bundled by cef-rs (the bundle binary
                      removes the build-only CMakeLists.txt, archive.json and libcef.lib)
  vcruntime140*.dll   only if a module imports the MSVC runtime (app-local copy)
  game\               the template's dist\, byte for byte
  PROVENANCE.txt      engine/template commits, pins, build commands, licenses
  provenance\         Cargo.lock, rust-dependencies.txt (crate + license), template
                      package-lock.json and npm ls, imports.txt, licenses\ (Roves, Metal
                      Mania OFL, CEF-LICENSE.txt, CEF-CREDITS.html moved from the bundle)
  SHA256SUMS.txt
```

**Packaged launch.** Without `--roves-game-dir`, if `<directory of play.exe>\game\index.html`
exists (and neither `--roves-sustained` nor `--roves-keep-open` is given), the probe runs that
game in game mode. The directory comes from `std::env::current_exe`, never from the working
directory, so a shortcut or another current directory open the same game. An ordinary launch
uses the **persistent player profile** (next section), so saves survive between launches;
`--roves-isolated-profile` is the explicit CI/test mode with a throw-away cache. The test hooks
(`--roves-test-*-ms`) are refused in the packaged launch (usage error, exit 1): they need an
explicit `--roves-game-dir`. `--roves-no-error-dialog` (packaged launch only, refused
otherwise) turns the failure dialogs off for unattended runs. Log line: `player launch
game-dir=<dir> profile=<persistent|isolated> cache=<dir> clean-cache=<bool>
error-dialog=<bool>`.

### Project icon (implemented 2026-11-25, not yet run in CI)

The game's own icon comes first in both places Windows shows one; the official Roves icon is
the fallback. The startup and failure screens always show the Roves logo and wordmark.

- **Window (title bar, taskbar while running).** At startup in game mode the probe looks for
  `icon.png` next to `play.exe` (`std::env::current_exe`). A valid one (at most 8 MiB, PNG
  signature with `IHDR` first, square, 16..=2048 px, decoded by SDL3 with the header's size)
  is downscaled by halvings to at most 256 px and set with `SDL_SetWindowIcon`; otherwise the
  embedded Roves logo is set as before. Pure rules in `src/icon_rules.rs` (unit-tested
  stand-alone in CI). Log: `window icon source=project size=<n>x<n> set=<bool>` or `window
  icon source=roves reason=<absent|invalid> [error=...]`, then the unchanged `branding decoded
  ... window-icon=<bool>`.
- **`play.exe` (Explorer, shortcuts, taskbar pin).** Its icon resource is patched after link
  time with electron/rcedit, as before, with the ICO chosen by
  `packaging/project_icon.py` (Pillow 11.3.0 pinned, contract tests in
  `packaging/test_project_icon.py`, run in CI before packaging).

Source order (first valid wins; PNG and ICO resolved independently): window PNG =
`--icon-png`, then `<game>/icon.png`, then the largest frame of the project ICO;
`play.exe` = `--icon-ico`, then an ICO generated from the resolved PNG (16..256 px frames),
then `<game>/icon.ico`, `<game>/favicon.ico`, then the Roves `roves.ico`. An explicit file
that breaks the rules fails the packaging; a file found in the game folder that breaks them is
a warning and the next source is tried. The package ships `roves.ico` so consumers that
replace `game\` (roves-action `cef-windows`, Roves Packmaster) re-run the same contract from
the base instead of inheriting the sample game's icon. Those consumers carry an identical copy
of the script (roves-action) or the same rules in Rust (Packmaster); keep the three in step.

Libraries: SDL3 (already linked) decodes and scales the PNG at runtime, as for the logo;
Pillow and rcedit were already pinned in the workflow. Not added: the `image` or `ico` crates
in the player (SDL3 covers the runtime PNG path; the ICO only exists at packaging time).
Real-hardware check pending: title bar/taskbar/Explorer icon of a packaged game with and
without its own icon, also at 150 %.

### Saves and the player profile (fixed 2026-11-23)

Regression fixed: the first packaged launch used a fresh per-run CEF cache under `%TEMP%`,
deleted on exit, and a free loopback port per run. Page storage (`localStorage`, IndexedDB,
Cache Storage) is keyed by origin *and* lives in the profile, so every launch started with an
empty game: saves were lost twice over.

- **Ordinary launch (default).** Profile root
  `%LOCALAPPDATA%\Roves\Player\<exe stem>-<16 hex>`, where the hex is the 64-bit FNV-1a of the
  lower-cased canonical install directory (`player_profile_name` in `src/game_rules.rs`,
  unit-tested): every launch of one install finds the same profile, and two packages that both
  ship `play.exe` never share storage. `--roves-cache-dir=<dir>` replaces the root. Contents:
  `player.lock` (held for the whole browser-process run), `origin-port` and `cef\` (CEF's root
  cache, with welding's per-producer `game-profile` inside). The first launch binds a free port
  and stores it (`origin-port.tmp` then rename; `player origin-port stored port=<n>`); every
  later launch binds the stored port, so the origin `http://127.0.0.1:<port>` and the saves stay
  identical. Nothing is deleted on exit. Log line: `player profile root=<dir> lock=acquired
  origin-port=<n|new>`. `--roves-port` is refused here (the profile owns the port).
- **Contention fails cleanly (exit 6), before the loopback server and CEF start.** A second
  copy of the same install finds the lock held (`File::try_lock`, i.e. `LockFileEx`; released
  by the OS when the process ends, even on a crash, so no stale lock): `FAIL player profile
  busy root=<dir> ...` and the dialog "The game is already running ...". If another program
  holds the stored port (bind `AddrInUse`/`PermissionDenied`): `FAIL game loopback port busy
  port=<n> ...` and a dialog asking to close that program; the probe never falls back to another
  port, which would silently show an empty game. A damaged `origin-port` file or an unwritable
  profile fails with exit 1 and a dialog naming the folder (`FAIL player profile root=...`).
  With `--roves-no-error-dialog` the dialogs are replaced by `player error dialog skipped
  (--roves-no-error-dialog)`. Windows lets a listener rebind a port at once even with
  connections in `TIME_WAIT` (checked locally), so a quick restart is not contention.
- **Isolated mode (`--roves-isolated-profile`, packaged launch only).** The previous behaviour,
  now opt-in for CI and tests: CEF's root cache is a fresh `%TEMP%\roves-cef-player\run-<ms>-<pid>`
  with a free port per run, removed after CEF shut down and the loopback server stopped, on every
  exit path after it is created (`player cache removed dir=... attempts=<n>`; retried for 2 s
  while child processes exit; `player cache not removed ...` otherwise). Refused with
  `--roves-cache-dir` or outside the packaged launch (usage error, exit 1).
- **Temporary probe mode unchanged.** `--roves-game-dir` keeps a per-run
  `%TEMP%\roves-cef-windows-player\run-...` cache and a free port unless `--roves-cache-dir`
  and `--roves-port` are given (see "Real-game mode").

Limits: moving or renaming the install directory starts a new, empty profile (the old one
stays under `%LOCALAPPDATA%`); no uninstaller removes profiles; a different program
permanently holding the stored port makes the game refuse to start until it is closed.
Saves surviving a real game's relaunch on real hardware are a manual check (below).
Libraries: no new dependency. Known-folder lookup uses `%LOCALAPPDATA%` directly (a crate such
as `directories`/`dirs` would add a dependency for one Windows-only path); locking uses the
standard library's `File::try_lock` (stable since Rust 1.89); the 64-bit FNV-1a is a few lines
because `std`'s `DefaultHasher` is not guaranteed stable across Rust releases.

**Startup screen (every game mode, packaged or `--roves-game-dir`).** The window is created
hidden; the official logo (root `icon.png`, embedded with `include_bytes!`) is decoded and
halved twice to 256 px by SDL3 itself (`SDL_LoadPNG_IO`, SDL 3.4's bundled stb_image, and
`SDL_ScaleSurface` linear; `SDL_PremultiplySurfaceAlpha`), set as the window icon
(`SDL_SetWindowIcon`, unless the game ships its own icon, "Project icon") and uploaded once as a premultiplied `Rgba8Unorm` texture. The
"Roves" wordmark uses the official font Metal Mania (`resources/fonts/MetalMania-Regular.ttf`,
SIL OFL 1.1, license copied to `provenance\licenses\`): `build.rs` rasterizes it at 112 px
with ab_glyph (a build-dependency only) into an 8-bit coverage bitmap (196x79 px, tight
bounds, kerning applied) embedded in `play.dll`; at runtime it is tinted `#F2F2F5`,
premultiplied and uploaded the same way. No font file and no rasterizer ship in the player.
A sparse checkout must include `resources/fonts` or the build stops with a clear error. Before
the CEF browser is created, the window is shown with a first frame: the dark background
`#141418` with a centred column of the logo, the wordmark (both 1:1) and an indeterminate
loading bar (196x6 px track, as wide as the wordmark, `#34343C` with a `#F2F2F5` segment a third as wide that sweeps
left to right, eased, every 1.4 s, clipped to the track), composed by the existing
premultiplied wgpu blit (now several draws per pass, each with a destination offset; the bar
is two 1x1 textures stretched to rectangles) and presented by the existing present path. The
bar animates from the screen's first frame because the loop already recomposes and presents
the startup screen every turn (vsync-paced); it shows activity only and claims no progress
fraction. Layout and animation are pure functions (`splash_layout`, `splash_indicator` in
`src/game_rules.rs`, unit-tested): sizes are physical pixels, the column is re-centred on
every frame (so resizes and DPI changes follow the surface size), and while it does not fit
the logo is dropped first, then the wordmark, so nothing is drawn outside the target. That
screen stays until
**both** the accepted main-frame load-end has been handled **and** a CEF frame painted after
it has been imported: the gate (`StartupGate` in `src/game_rules.rs`, unit-tested) records the
newest welding paint generation at the end of the loop turn that handled load-end and
releases on a strictly newer one, because a frame imported in that same turn may predate the
load. At load-end the probe calls welding's `resize` with the current size, which re-sends
`was_resized` and `Invalidate(PET_VIEW)`, so a page that is already static still paints
once. Only then is the composed CEF frame shown. The clear colour around the game frame (for
example right after a resize) is the same dark colour in game mode; the fixture keeps its red
background and visible window, unchanged. In the packaged launch, page input is forwarded only
after the release (explicit `--roves-game-dir` keeps forwarding from load-end, as its test
hooks expect), and `--roves-auto-close-ms` counts from the release instead of load-end. Log
lines: `branding decoded logo=256x256 window-icon=<bool> elapsed_ms=<n>` (or
`branding unavailable error=...`, then the screens omit the logo), `wordmark rendered
size=196x79 font=MetalMania` (or `wordmark unavailable error=...`, also when the embedded
bitmap has no opaque pixel; then
the screens omit the wordmark; the bar needs no decoding and is always drawn),
`startup screen shown size=<w>x<h> window-shown=<bool> presented=<bool>`, `startup
fresh-frame requested ok=<bool>`, `startup released generation=<n> imported=<n>
after_load_end_ms=<n>` and the `startup milestone=startup-released` timeline entry. Note that
`first-present` now marks the first redraw of the startup screen in game mode, not the first
CEF frame.

**Readable failure.** A main-frame load error, a rejected load-end (status or origin), a
renderer termination (also after the release) or, in the packaged launch, no release within
`--roves-timeout-ms` (`load-timeout` if no accepted load-end, `no-fresh-frame` if loaded but no
newer frame, e.g. accelerated off-screen rendering unavailable) turns the screen into the
failure screen: the same dark background with the logo and wordmark at about 30 % opacity,
no loading bar (nothing is loading any more), and the window
title `Roves - the game could not start` (`startup failed reason=<token>`, then `startup
failure screen presented reason=<token>`). The packaged launch then shows a native SDL message
box over the window (`SDL_ShowSimpleMessageBox`, error icon) with a sentence for the player,
the reason token and the log path, and closes with exit code 2 after it is dismissed (`startup
failure dialog closed ok=<bool>`, or `startup failure dialog skipped
(--roves-no-error-dialog)`). If the window itself cannot be created (for example no DX12
adapter) the packaged launch shows the same kind of dialog without a parent window (exit 5).
No red or black placeholder is shown on any of these paths. Beyond the pre-rasterized wordmark,
no text is drawn on the wgpu screen: sentences would need a real font rasterizer and text
layout (for example glyphon, whose releases track wgpu versions independently of the wgpu 29
pin here, or SDL3_ttf plus a bundled font); the native dialog gives readable,
localisable-by-OS text with code already in the graph. The wordmark is fixed at build
time, so it needs neither of those.

**Chromium reference.** The `support/chromium-reference` submodule was empty in this
checkout, so the pinned revision `f2700724f59fb554eb19a4f827b1d256e2a13286` was read through
GitHub raw: `content/browser/renderer_host/render_widget_host_impl.cc` and `.h`
(`new_content_rendering_timeout_`, `ClearDisplayedGraphics`,
`ForceFirstFrameAfterNavigationTimeout`, `OnRenderFrameMetadataChangedAfterActivation`),
`third_party/blink/common/widget/constants.cc` (`kNewContentRenderingDelay` = 4 s),
`content/public/browser/web_contents_observer.h` (`DocumentOnLoadCompletedInPrimaryMainFrame`,
`DidFinishLoad`) and `chrome/app/chrome_exe.rc` (`IDR_MAINFRAME` first icon resource).
Chromium keeps showing the previous surface and keeps input inactive after a cross-document
navigation until the renderer's first frame metadata for the new content arrives
(`first_content_metadata_received_`, then `input_router()->MakeActive()`), and after 4 s
clears to the background and releases input anyway. Roves maps the "previous surface" to its
startup screen and holds input the same way in the packaged launch; it requires load-end in
addition (a game's first paint is often an empty shell before its scripts finish), and it
does not reveal an empty page after a timeout: it shows the failure screen instead, because a
blank white or black window is exactly what the screen replaces. The Windows icon follows
Chrome's model of an icon resource in the executable (`IDR_MAINFRAME` first), applied after
link time because the executable is CEF's prebuilt bootstrap.exe.

**Libraries evaluated.** Reused: SDL3 3.4 (already linked) for PNG decode, downscale,
premultiply, window icon and the message box; ab_glyph 0.2.32 (Apache-2.0, pure Rust,
build-dependency only) to rasterize the Metal Mania wordmark at build time; the probe's wgpu blit for composition, the wordmark and the loading bar
(no new pipeline or shader); not added for the wordmark: SDL3_ttf (a second native library
and a font file to ship), glyphon (above) and a runtime rasterizer (the text never changes);
welding's `resize` for the fresh paint (it exposes no `invalidate`); for the executable icon,
the tool `mach bundle --icon-ico` already uses (electron/rcedit v2.0.0, MIT, SHA-256 pinned
in the workflow) and Pillow 11.3.0 (pinned) to turn `icon.png` into a multi-size `.ico`. Not
added: the `image` crate (decode/resize would duplicate SDL 3.4), `winresource`/`embed-resource`
(they embed resources at link time of a Rust binary; `play.exe` is CEF's prebuilt
bootstrap.exe), a text renderer (above). No new runtime Rust dependency; CEF is not forked.

**CI gate for the package (WARP).** After the probe steps, the workflow checks out the
template at the pinned commit (`persist-credentials: false`), verifies `HEAD`, builds it with
Node 22 `npm ci` + `npm run build` (the template's own `prebuild` PWA icon step runs as
part of it; the resulting checkout changes are only recorded), converts the icon, verifies
rcedit's SHA-256 and assembles the folder above. Gated there: the three renamed files exist and
no `roves_cef_windows_player*`/`.pdb` file remains, the icon extracted from `play.exe`
changed after rcedit, `game\` has exactly the same files and SHA-256 as `dist\`, every DLL
imported by a top-level `.exe`/`.dll` is in the package, a Windows API set or in System32,
except the MSVC runtime, which is copied app-local from the runner's Visual Studio
redistributable directory when imported (delay-loaded imports are reported only), and
`cargo tree` succeeds. A missing CEF `LICENSE.txt` is a warning. Then `play.exe` is started
**directly**: no `--roves-game-dir`/`--roves-cache-dir`, from an unrelated working directory,
with `--roves-isolated-profile --roves-no-error-dialog --roves-auto-close-ms=5000
--roves-timeout-ms=60000`. Required in order: sandbox info, `player launch` with
`<package>\game`, `profile=isolated` and `clean-cache=true`, the
loopback origin, `window icon source=project size=<n>x<n>
set=true` when the packaging resolved a project window icon (otherwise `window icon
source=roves reason=absent`; logged before the branding is decoded),
`branding decoded logo=256x256 window-icon=true`, `wordmark rendered size=196x79 font=MetalMania`, `startup screen shown ...
window-shown=true presented=true`, the producer, `load-end status=200 url=<origin>/ ok=true`,
`startup fresh-frame requested ok=true`; then, since WARP delivers no accelerated frame, the
failure path (`FAIL startup not released within 60000 ms (no-fresh-frame)`, `startup failed
reason=no-fresh-frame`, `startup failure screen presented`, `startup failure dialog skipped`,
the clean close/shutdown lines, `player cache removed`, `exit code=2`); if frames do arrive,
the release path is required instead (`startup released`, `auto-close scheduled delay_ms=5000
from=startup-release`, `auto-close window`, clean shutdown, `exit code=0`). Also gated: the
expected exit code, no other `FAIL` line, no `player cache not removed`/`branding
unavailable`/`wordmark unavailable`/test-hook line, the cache directory gone afterwards, a `--type=renderer` child,
no child with `--no-sandbox`, no `play.exe` left, and five refused launches (exit 1, no log):
`--roves-no-error-dialog` and `--roves-isolated-profile` on the plain bundle, a test hook,
`--roves-isolated-profile` with `--roves-cache-dir`, and `--roves-port` in the persistent
packaged launch. A
desktop capture 3 s after the main load (`startup-screen.png` in the evidence) must contain at
least 200000 pixels of the background colour (±3), at most 20000 saturated red pixels (the
logo has a red ring; the old red clear colour would fill the window), at least 1000
near-white logo/wordmark pixels and at least 500 loading-bar track pixels (`#34343C` ±3; the
track is 1440 px, of which the segment covers at most 480); an all-black capture (no visible
desktop) only warns. **Not proved by CI:** the bar's motion (one capture), the release onto a
real game frame, gameplay, input, the failure dialog itself and the
visual failure screen (both need a real or interactive desktop), audio, and anything on a
real GPU.

**CI gate for the persistent profile (WARP).** A second step launches the package twice with
no profile switch (default root under the runner's `%LOCALAPPDATA%`, `--roves-timeout-ms=15000`,
exit 2 on WARP or 0 if released). Required: `profile=persistent ... clean-cache=false`, a root
named `play-<16 hex>` under `%LOCALAPPDATA%\Roves\Player`, `origin-port=new` and `player
origin-port stored port=<n>` on the first launch, `origin-port=<n>` and no new store on the
second, the **same loopback origin** in both, `origin-port` holding `<n>` and `cef\` plus
`cef\game-profile` present after exit, no `player cache removed`. Then three refusals that must
end before `cef initialized`: the profile lock held by the step (`FileStream.Lock`) gives exit 6
and `FAIL player profile busy`; the stored port held by a `TcpListener` gives exit 6 and `FAIL
game loopback port busy port=<n>`; a damaged `origin-port` gives exit 1. This proves the profile
and origin are stable, not that a game's own save survives (that needs a frame and input).

Manual check: run `play.exe` from the artifact on the GTX 1050 and confirm the startup screen
(logo, readable wordmark, a smoothly moving bar, also after a resize and on a 150 % display),
the release onto the template's menu, input, closing, the icon in Explorer and the taskbar;
save in the template, close, start again and load the save; start a second copy while the first
runs (dialog "already running", exit 6, first copy unaffected).

## Host API (`/__roves/`, implemented 2026-11-25, not yet run in CI)

Games call the player through `@drincs/roves-api` (`core`, `saves`, `steam`). The Servo shell served
those calls on the custom schemes `roves:` and `saves:`; welding owns `CefApp` and the
render-process handler, so registering schemes would need a welding fork (rejected). The
player instead serves same-origin routes on the game's loopback server:
`http://127.0.0.1:<port>/__roves/<module>/<cmd>` (`support/cef-windows-player/src/host_api.rs`,
shared through `#[path]` with the macOS and Linux gates; modules are a name -> handler map:
`core`, `saves`, `steam`, section "Steam"). A game directory must not contain a `__roves` folder: the prefix is
never served from the game files.

- **Discovery:** every game-mode launch defines `window.__ROVES__ = Object.freeze({ transport:
  "http", api: "/__roves/" })` (read-only, top-level game document only, before any page
  script) through the same CDP document-start registration as the MCP marker (section "Local
  MCP bridge", `page_init.rs`). roves-api's `isAvailable()` accepts it, and `core`/`saves`
  then call the routes; older shells keep `__ROVES__ === true` and the schemes.
- **Request guard** (after Chromium's DevTools HTTP handler,
  `content/browser/devtools/devtools_http_handler.cc` at `f2700724f59f`:
  `RequestIsSafeToServe` ~134-142 refuses a `Host` that is not an IP or localhost, the
  DNS-rebinding defence; `OnWebSocketRequest` ~866-890 refuses a foreign `Origin`). Stricter
  because only the game document may call: `Host` exactly `127.0.0.1:<port>`; header
  `X-Roves-Api: 1` required (a cross-origin page needs a CORS preflight for it); `OPTIONS` is
  always refused and no response carries a CORS header; an `Origin`, when sent, must be the
  server origin; `Sec-Fetch-Site`, when sent, must be `same-origin`. Refusals are `403`.
  Every reply is `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`.
- **core:** `system_info` (the Servo shape: `os_type` `windows`/`macos`/Linux `ID`,
  `os_version` (`null` on Windows: no OS-version crate in the graph; Linux `VERSION_ID`,
  macOS `ProductVersion`), `bitness`, `architecture`, `engine_version` =
  `roves-cef-player cef-<CEF version>`); `exit` (`POST`) closes through the same path as an
  OS close request (log `close requested by the game command=exit`); `clear_content_cache`
  (`POST`) has no extraction cache to clear (the game is served from the package and every
  response is revalidated, section "Loopback cache policy") and keeps its documented
  contract: the game closes. `is_available` answers `true`. Servo's `save_file` is not ported.
- **saves:** files `<profile root>\saves\<key>.save` of the persistent player profile;
  without one (`--roves-game-dir`, `--roves-isolated-profile`) every command answers the
  Servo "unavailable" values (`is_available` false). Commands and JSON replies are the
  Servo ones (`read` returns base64 or `null`, `most_recent` `{key, modifiedMs}`); `write`,
  `delete`, `clear` need `POST`, `write` takes the raw body (at most 32 MiB). Keys keep the
  Servo rules (non-empty, at most 200 bytes, not `.`/`..`, no `/ \ :` or NUL) plus portable
  file names (no other control character, none of `< > " | ? *`, no trailing dot or space, no
  Windows device name such as `CON` or `COM1`); invalid keys answer `400`. Writes go through
  `tempfile`'s `NamedTempFile` in the same directory, `sync_all`, then `persist` (rename over
  the target); leftover temporary files are removed when the server starts. File I/O runs on
  tokio's blocking pool, so game files keep being served.
- **Tests:** `support/cef-host-api-tests` runs `server.rs` and `host_api.rs` under plain
  `cargo test` (guard, keys, saves round trip, routes, a save surviving a server restart);
  it is a separate CEF-free crate because the players' test binaries must link CEF, which
  their `dox` type-check feature does not provide. CI also runs the packaged player on
  `host-api-fixture/` three times with one persistent profile (write + `exit`, read back after
  a restart + `exit`, then `curl` refusals for a foreign Host, a missing header, a foreign
  Origin and a preflight). The fixture is a static page (style in `style.css`, as its CSP
  `default-src 'self'` refuses inline styles; a CSP violation fails the step). On WARP no
  accelerated frame arrives at all, so launch 3, which stays open past `--roves-timeout-ms`,
  ends on the `no-fresh-frame` path (exit 2); the step accepts that only with the WARP
  adapter and `closing imported=0`, and fails a launch that imported frames without
  releasing the startup screen. A static first screen does not need an animation to release
  it: on load-end the player calls welding's `resize` at the same size, whose
  `Invalidate(PET_VIEW)` makes CEF request a refresh frame from Chromium's frame-sink video
  capturer (`FrameSinkVideoCapturerImpl::RequestRefreshFrame` -> `kRefreshDemand`, which
  captures the current surface without new damage). Release of a static page on a real GPU
  is a manual check (CI has no GPU).

## Steam (`steam` feature, off by default; implemented 2026-11-25, not yet run in CI)

`cargo run --bin bundle -- <dir> --release --features steam` builds the player with
`steamworks` =0.13.1 (steamworks-sys 0.13.0, the crates of the legacy Servo `steam` feature;
MIT/Apache-2.0, Steamworks SDK redistributables under
Valve's terms). Without `--features`, the bundle binary keeps cef-rs' `build_bundle`; with
it, it builds the cdylib with those features itself (cef-rs builds it without features),
lays it out with cef-rs' `bundle` and copies steamworks-sys's `steam_api64.dll` (found
through cargo's `build-script-executed` message) next to the executable. A Steam package
ships that DLL next to `play.exe`.

- **One client** (`src/steam.rs`): game mode only, before CEF and the wgpu device exist (the
  Steam overlay hooks the renderer's device). App ID as Steam itself resolves it: the Steam
  launch environment (`SteamAppId`), else a development `steam_appid.txt` next to the
  executable (`Client::init_app`), else whatever `SteamAPI_Init` finds. Failure is never fatal:
  one log line `steam unavailable reason=<kind> <detail>` (or `steam initialized
  app_id=<id>`); no persona name or Steam ID is ever logged. `SteamAPI_RunCallbacks` runs
  once per SDL main-loop turn; the client is dropped (`SteamAPI_Shutdown`, log `steam
  shutdown complete`) before CEF shuts down, on every path where CEF was initialized.
- **Delay-loaded DLL:** build.rs links `steam_api64.dll` with `/DELAYLOAD` (plus
  `delayimp.lib`), so CEF's sandboxed child processes, which run the same `play.dll`, never
  load it, and the player checks the file exists before the first Steamworks call.
- **Host API `steam` module** (`host_api.rs`, platform-neutral: the Steamworks calls sit
  behind its `SteamApi` trait, so the macOS/Linux gates build it without steamworks and answer
  "unavailable"): the legacy `steam:` protocol's commands, query names and JSON
  (`ports/servoshell/desktop/protocols/steam.rs` at git `27ca0811d6d`): `is_available`,
  `get_player_name`, `get_app_id`, `is_achievement_unlocked{achievement_id}`,
  `get_stat_int{name}`, `get_stat_float{name}`, `is_dlc_installed{app_id}` (`GET`), and
  `unlock_achievement`/`clear_achievement` (each followed by `StoreStats`), `set_stat_int`,
  `set_stat_float{name,value}`, `store_stats`, `open_overlay{dialog}`, `open_store{app_id?}`
  (`POST`). Without the feature, without Steam or after shutdown every command answers the
  legacy "unavailable" value (`false`, `0`, `0.0` or `null`); a missing or malformed argument
  answers `400` (the legacy protocol failed the fetch). Steamworks calls run on the server's
  blocking threads (`Client` is `Send + Sync`), serialized with the callback pump by one
  mutex. roves-api `steam.ts` uses these routes when `__ROVES__` is the HTTP marker.
- **CI evidence (emulation only):** the workflow builds the Steam variant and runs it with the
  pinned, SHA-256-checked Goldberg/GSE Windows release (`emu-win-release-vs22.7z`,
  `release-2026_09_27`, LGPL-3.0) in place of `steam_api64.dll`, offline, App ID 480, with
  `ACH_WIN_ONE_GAME` and the int stat `NumGames` declared in its `steam_settings`: the fixture
  must see `is_available` true, `get_app_id` 480, unlock/unlocked/set/stat/store true/3. It
  also checks the delay-load import and the shutdown order. The plain package must answer
  `steam available=false`. None of this is evidence about the real Steam client, the overlay,
  ownership or Steam Cloud: the real-Steam smoke stays manual.
- **Steam playable package:** `packaging/steam_package.ps1` derives it from the plain package
  (same `play.exe`, CEF runtime, `game\`, icons) with the Steam `play.dll` and the
  redistributable `steam_api64.dll`, after checking that every other file of the Steam bundle
  is byte-identical in the plain bundle; `provenance\imports.txt` and
  `rust-dependencies.txt` describe the Steam build, `PROVENANCE.txt` gains a "Steam variant"
  section and `SHA256SUMS.txt` is rebuilt. CI launches it directly with no Steam client
  (`steam unavailable`, software frame on WARP, startup released, exit 0) and uploads it as
  `visual-novel-template-roves-cef-steam-windows-x64`.

## Local MCP bridge (optional, off by default; implemented 2026-11-24, fixture gate passed in CI)

Lets an MCP client (an AI agent, an IDE) read the state of a running game and interact with
it, for development and playtests only. Game mode only (packaged launch or
`--roves-game-dir`). **Off unless** the launch has `--roves-mcp`, or the executable's folder
contains the marker file `mcp\enabled` (written only into the MCP-enabled development
package, below). Off means: no pipe, no session record, welding's DevTools channel not
subscribed, and no `mcp` log line (gated in CI on the plain package). The same `play.dll`
serves both packages; `--roves-mcp` without a game is a usage error.

The macOS and Linux players (`support/cef-macos-gate`, `support/cef-linux-gate`, section
"Local MCP bridge" of their READMEs) compile the same `src/mcp.rs` and `src/mcp_rules.rs`
through `#[path]` since 2026-11-26: same switch, same package marker (macOS
`Contents/Resources/mcp/enabled`, Linux `mcp/enabled` next to the executable), same record
format, requests and page marker, with a unix-domain socket instead of the named pipe (below).
The SDL event builders and the `Input` hand-off of `click`/`press_key`/`type_text`
(`mcp::click_events`, `key_events`, `text_event`, `forward`) are shared by the three players.

Pieces (`src/mcp.rs`, pure rules in `src/mcp_rules.rs`, stdio server in `mcp/`):

- **Transport and discovery.** At launch, before the window and the browser exist, the probe
  opens the Windows named pipe `\\.\pipe\roves-mcp-<pid>-<128-bit random hex>` with tokio's
  `named_pipe::ServerOptions` (`first_pipe_instance`, so an existing pipe of that name cannot
  be squatted; `reject_remote_clients`) on its own thread, then writes the session record
  `%LOCALAPPDATA%\Roves\mcp\<pid>.json` (`version`, `pid`, `pipe`, 256-bit `token` from
  the OS RNG through `getrandom`, game `origin`) through a temporary file and a rename. The
  record is removed when the run starts closing (and at the end of every exit path). Log
  lines: `mcp enabled source=Switch|Package pipe=<name> record=<path>` (the token is never
  logged), `mcp call method=<m> ok=<bool>` (or `queued=true` for `evaluate`), `mcp session
  removed calls=<n>`, or `mcp unavailable source=... error=...` (the game still runs).
- **macOS and Linux transport.** The session directory is `~/Library/Application
  Support/Roves/mcp` (macOS) or `$XDG_RUNTIME_DIR/roves/mcp`, else
  `~/.local/state/roves/mcp` (Linux; a relative value counts as unset, as in the XDG Base
  Directory specification): `mcp_rules::record_dir`, also implemented by `server.mjs`. It is
  created with every missing part 0700, or, when it exists, must be a real directory (not a
  symlink) owned by the effective uid and is tightened to 0700 (`mcp_rules::private_dir`).
  The endpoint is the socket `<dir>/<pid>.sock` (tokio `UnixListener`, a stale socket of the
  same PID replaced, chmod 0600 after bind); the record field `pipe` holds its path. Every
  accepted connection's peer credentials (`UnixStream::peer_cred`: `SO_PEERCRED` on Linux,
  `getpeereid` on macOS) must name the player's effective uid, otherwise it is closed unread.
  The record is written 0600 (`mcp_rules::write_private`, fresh temporary file + rename, as
  on Windows). Socket and record are removed when the run starts closing. The gates log the
  same lines with the `gate: ` prefix.
- **Authentication.** The first line of every connection must equal the token (compared
  without an early exit) within 5 s; otherwise the pipe closes without answering anything.
  Then one JSON request per line (at most 20 KiB), one JSON reply per line (`{"value": ...}`
  or `{"error": "..."}`); 8 queued requests at most, 10 s per answer, idle connections closed
  after 120 s.
- **Requests**, handled on the main loop (at most 4 per turn):
  `state` (host-side: origin, last on-origin URL, title, `loaded`, `startup`
  `none|loading|released|failed`, `input_ready`, window size in points and pixels, scale,
  suspension, imported/presented frames, input counters, console count and last 50 lines; no
  page script); `evaluate` (`{"expression"}` up to 16 KiB; CDP `Runtime.evaluate` by
  reference in a per-call `objectGroup`, `awaitPromise: false`, `userGesture: false`, a
  1000 ms script `timeout` and `allowUnsafeEvalBlockedByCSP: false`, sent through welding's
  in-process `send_devtools_message` = `CefBrowserHost::SendDevToolsMessage`; the reply is
  matched by id in welding's bounded DevTools queue, drained every loop turn while the bridge
  is on; a result with `subtype: "promise"` is refused, any other object or function is then
  fetched by value with `Runtime.callFunctionOn` (`function () { return this; }`,
  `returnByValue`) on the same reference, and the group is released with
  `Runtime.releaseObjectGroup` when the call ends; exceptions, non-JSON values and results
  over 64 KiB are errors; 5 s without a reply for the whole call is an error; only a
  top-level Promise is detected, one nested in a returned object is serialized as `{}`);
  `click` (`{"x","y"}` window points = SDL logical coordinates, CSS pixels at page zoom 1;
  left down + up), `press_key` (`{"key"}`, a fixed list: ArrowLeft/Right/Up/Down,
  Enter, Escape, Space, Tab, Backspace, Delete, Home, End, PageUp, PageDown; down + up, no
  `Char`) and `type_text` (`{"text"}`, 1..256 printable characters, one `Char` each). The
  three input requests build ordinary SDL events (device id `0x524F4D43`, never pushed into
  the SDL queue) and hand them to the game's `Input`, the same translation and welding calls
  as real input, counted in `game input forwarded=...`; they are refused with "input is not
  ready" exactly when real input would be discarded (before the accepted load-end; in the
  packaged launch, before the startup screen is released).
- **Why `evaluate` is by reference.** A single `returnByValue` evaluation cannot refuse a
  Promise: V8's inspector serializes a by-value object as plain JSON without its `subtype`
  (`ObjectMirror::buildRemoteObject`, `WrapMode::kJson`, `src/inspector/value-mirror.cc`), so
  `Promise.resolve(42)` arrived as `{"type":"object","value":{}}` and passed as `{}` (local
  GTX 1050 fixture smoke of `9d691517300`, `smoke.mjs` Promise assertion). A thrown value is
  wrapped by reference in either mode (`InjectedScript::wrapEvaluateResult`) and is reported
  from `exceptionDetails.exception.description`, so exceptions were never affected.
- **Server** (`mcp/server.mjs`, Node.js 20+, the official MCP TypeScript SDK
  `@modelcontextprotocol/server` 2.4.0 and zod 4.4.3, the same pins and lockfile the retired
  Blitz dev-mcp used, commit `d587feb97f6`). Stdio MCP tools `get_state`, `evaluate_script`,
  `click`, `press_key`, `type_text`. On start it reports the running sessions on stderr; every
  tool call rereads the records (valid shape, PID alive), so the game may start later; with
  several games it asks for `--pid <PID>`. Consumer instructions: `mcp/README.md`.
- **Page capability `isMcpEnabled()`** (2026-11-25, implemented, not yet built or run in
  CI). `@drincs/roves-api/core`'s `isMcpEnabled()` reads `window.__ROVES_MCP__ === true`
  synchronously; `isAvailable()` reads the host marker `__ROVES__` (section "Host API").
  Delivery (since 2026-11-25 in `page_init.rs`, shared with `__ROVES__`, so every game-mode
  launch has it; the bridge only adds `__ROVES_MCP__` to the script when `mcp::start`
  succeeded): the producer is created on `about:blank` instead of the game URL. Every loop
  turn the host tries CDP
  `Page.enable` through the same in-process DevTools channel until the browser exists; after
  its successful reply it sends `Page.addScriptToEvaluateOnNewDocument` (`source` only, main
  world, `runImmediately: false`); when that reply carries an `identifier` (log `page markers
  registered identifier=<id> mcp=<bool>`) the
  host calls welding's `navigate_to_url(<origin>/)` (log `game navigation url=...`). The
  script (`mcp_rules::page_marker_script`, unit-tested) returns early unless `window.top
  === window` and `location.origin` equals the game's `http://127.0.0.1:<port>` literal, then
  defines `__ROVES__` (frozen host marker) and, with the bridge, `__ROVES_MCP__` as `{ value:
  true, writable: false, enumerable: false, configurable: false }`. It contains nothing else:
  no pipe, no token, no record path. A CDP error, or no browser/reply within 10 s, logs `page
  markers unavailable mcp=<bool> error=...` and loads the game anyway without the markers
  (fail-closed; `state.page_capability`
  `pending|registered|unavailable`). `evaluate` is refused until the registration settled, so
  it never runs in the bootstrap document. Navigation events of `about:blank` are skipped
  (log `page bootstrap event skipped ...`) only until the game commits on its origin
  (load-end: until the accepted game load-end); afterwards `about:blank` is an off-origin
  navigation as before. Bridge off or unavailable: the same bootstrap, without the MCP marker.
  **Why this guarantees the entry script:** Chromium stores the script in the DevTools
  session's browser-originating state before answering
  (`content/browser/devtools/protocol/page_handler.cc`
  `AddScriptToEvaluateOnNewDocumentInternal`, ~793-821, `browser_originating_session_state()
  ->scripts_to_evaluate_on_new_document`), which is re-sent to every renderer the session
  attaches to, so it survives the cross-site `about:blank` -> loopback renderer swap. In the
  renderer, `InspectorPageAgent::DidCreateMainWorldContext`
  (`third_party/blink/renderer/core/inspector/inspector_page_agent.cc` ~1076-1101) injects
  the stored scripts when each frame's main-world context is created, before the parser runs
  any page script. That probe reaches the page agent only after `Page.enable`
  (`InspectorPageAgent::enable` ~557-564 calls `instrumenting_agents_->
  AddInspectorPageAgent(this)`; `Restore` ~526-529 re-enables it in each new renderer), and
  `InspectorInjectedScriptManager::InjectScripts`
  (`inspector_injected_script_manager.cc` ~61-73) has no other caller: without `Page.enable`
  the registration is answered with an identifier but never runs (CI run 45575923510: the
  entry script saw no `__ROVES_MCP__` descriptor). The Page domain therefore stays enabled
  for the MCP session; its events are drained and dropped, and since CEF installs its own
  JavaScript dialog manager, dialogs are only reported to the session, not delegated to it
  (`page_handler.cc` `DidRunJavaScriptDialog` ~647-671, `Enable` ~703-731). Same-origin child
  frames get the script too and return at the `window.top` check; cross-site frames are
  separate targets (no auto-attach) and never get it. Revision `f2700724f59f`, sparse
  checkout of those files and `content/browser/devtools/protocol/`.
  **Missing welding API (no upstream request opened):** welding e991684 owns the
  `CefApp`/`CefRenderProcessHandler` (`WeldApp::build` and `WeldRenderProcessHandler` in
  `welding/src/app.rs` are `pub(crate)`; the handler implements only
  `on_process_message_received` for `weld.eval`) and ignores `extra_info` at browser
  creation, so a host cannot add CEF's own document-start hook `OnContextCreated` (or pass
  per-browser data to the renderer) without forking welding. The minimal upstream path would
  be a `CefSurfaceConfig` field for document-start main-world scripts, applied by welding's
  render-process handler in `OnContextCreated` (with the frame's top-level/origin known
  there) and delivered through `extra_info`. The CDP route above needs no fork and uses
  only welding's public DevTools channel, so it was chosen; it costs two CDP round trips and
  an `about:blank` commit before the game URL (every game-mode launch since the host API
  marker; the cost is visible in the startup timeline, not measured separately yet). Rejected:
  `execute_script`/`request_script_result` after load-start (races the page's own scripts),
  a `<script>` rewritten into the served HTML (changes the game's bytes and CSP), a URL
  parameter or cookie (page-controlled, not top-level-only).

**Security model.** Same user only. Windows: the record lives in the user's profile (default
ACL: the user, SYSTEM, Administrators), the pipe accepts no remote client, and nothing works
without the token. macOS/Linux: the socket and the record live in a 0700 directory owned by
the user (0600 each), connections from another uid are closed by the peer-credential check,
and nothing works without the token. Any process of the same user can read the record and
drive the game; such a process could already open or debug the game process itself, so this
adds no privilege.
No TCP listener and no `--remote-debugging-port`/`--remote-debugging-pipe` switch is used; the
loopback HTTP server is unchanged. The page receives only the read-only boolean
`__ROVES_MCP__` (top-level document on the game origin, bridge running; see above): no
binding, no message router, no other script, the CDP session is browser-side only, and only
the Page domain is enabled (MCP-enabled launches only, for the document-start marker; its
events are dropped). The sandbox (bootstrap broker, sandboxed renderers) and the origin policy
(off-origin commit = FAIL and close; popups refused) are unchanged: an expression that
navigates away still trips the origin check. `evaluate_script` can change game state: it is
a trusted development command. Never ship an MCP-enabled package to players.

**Chromium reference** (`support/chromium-reference` at `f2700724f59f`, sparse checkout of
the three files below for this change). `content/browser/devtools/devtools_pipe_handler.cc`
(`DevToolsPipeHandler` constructor, ~396-412): `--remote-debugging-pipe` attaches a client to
`DevToolsAgentHost::CreateForBrowser` over two inherited file descriptors (on Windows
`_get_osfhandle`, ~144/228) with NUL-delimited JSON or CBOR framing and no authentication: a
port-less transport whose access control is "only the parent process holds the handles".
`chrome/browser/devtools/remote_debugging_server.cc` (~90/133: bound to `127.0.0.1`;
`IsRemoteDebuggingAllowed` ~162-182: policy pref and, in Chrome branding, refusal with the
default user-data dir): `--remote-debugging-port` has no authentication either, only the
loopback bind. Roves needs a client that is **not** its parent (the MCP client starts after
the game), so it keeps Chromium's port-less idea (no socket) but replaces "inherited handles"
with a per-session token over a local named pipe, and it uses CEF's in-process
`SendDevToolsMessage` (the `AttachClient` path of `content/public/browser/devtools_agent_host.h`
~120/172) instead of either switch, so CEF never exposes the whole browser target. The
`Runtime.evaluate` options are CDP's documented parameters. The result shape was then checked
in V8's inspector sources as vendored by the `v8` crate 152.2.0 (`src/inspector/value-mirror.cc`
`toProtocolValue`/`ObjectMirror`/`SymbolMirror`, `injected-script.cc` `wrapEvaluateResult`,
`v8-runtime-agent-impl.cc` wrap modes), not CEF 154's own V8 build.

**Libraries evaluated.** Reused: welding's DevTools channel (`devtools_protocol`,
`send_devtools_message`, `poll_devtools_message`) instead of an own observer; the probe's
`Input` for interaction; tokio (`named_pipe` on Windows, `UnixListener` + `peer_cred` on
macOS/Linux, `io-util`, `time`, `sync`, all already compiled in the graph) for the
transport; serde_json (already in the graph through cef and welding) for JSON; `getrandom`
0.2.17 (already in the three lockfiles; since 2026-11-26 a direct dependency of the three
players, replacing the Windows-only `BCryptGenRandom` call and its windows-sys feature) for
the token and the pipe nonce; `libc` 0.2.191 (already locked) for `geteuid` in the macOS/Linux
peer and directory-owner checks; the official MCP TypeScript SDK for the protocol. Not used: `rmcp` (Rust MCP SDK; a self-contained `.exe` would avoid Node, but it
adds a crate graph to review and a second binary, while the TypeScript server already ran in
this repository), CEF's `--remote-debugging-port` (unauthenticated TCP),
`--remote-debugging-pipe` (needs the client as parent process), welding's
`request_script_result` (renderer-side evaluation, no CSP option, no time limit), CDP
`Input.dispatch*` (would bypass the probe's input translation that the real game uses). No
CEF fork, no new Cargo crate (one lock line: `serde_json` as a direct dependency).

**Verification of the macOS/Linux bridge (2026-11-26).** Local only: `mcp_rules` stand-alone
tests (10 on Windows; the unix directory/permission test compiles only on unix and runs in
the gate workflows' assembly steps), `cef-host-api-tests` (now including `mcp_rules`, 71
tests) and its `cargo check --tests --target x86_64-unknown-linux-gnu`, clippy `-D warnings`
`dox`/`dox,steam` for this player and cross for both gates, `cargo fmt --check`, `node
--check` and `smoke.mjs list-only` on Windows, YAML and `bash -n` of the workflows. CI gates
added, **not run yet**: on macOS and Linux an MCP-enabled package (artifacts
`cef-macos-gate-arm64-mcp`, `cef-linux-gate-x64-mcp`), the fixture smoke on it with the MCP
fixture as the packaged game (directory 0700, socket and record 0600, interaction), the
disabled smoke on the standard package (bridge off; bridge unavailable: Linux
`XDG_RUNTIME_DIR` points at a file, macOS a file at the session directory's path) and the
packaged launch smoke; release assets `roves_shell_{macos,linux}_mcp.zip`.

**Verification of `isMcpEnabled()` (2026-11-25).** Local only: `mcp_rules` tests (8,
including the page script bounds), `cargo check`/`clippy -D warnings --features dox
--all-targets` and `cargo fmt --check` on Rust 1.99.1, `node --check mcp/smoke.mjs`,
roves-api `npm test` (5 cases, no window/regular browser/bridge/no bridge/non-boolean). CI
gates added, **not run yet**: the fixture smoke now also requires the marker as
`{ value: true, writable: false, enumerable: false, configurable: false }` in the entry
script `mcp/fixture/entry.js` (classic, parser-blocking, first page code), unchanged after
assignment and `delete`, absent in the same-origin child frame `frame.html`, the only
bridge-named global, and the `registered`/`game navigation` log lines; the new step "MCP
disabled smoke on the fixture (WARP)" (`smoke.mjs disabled`) runs the plain package's
`play.exe` on the same fixture without `--roves-mcp` and with `--roves-mcp` while
`LOCALAPPDATA` points at a file (bridge cannot start): direct load, marker absent in entry
script and child frame, no record, no bootstrap line, exit 0. Regular browser: the same
`roves-api` unit tests only; the Chrome DevTools (MCP) check of the fixture served over HTTP was not run (tool permission not granted in this session).

**Verification.** Local: `rustc --edition 2024 --test src/mcp_rules.rs` (7 tests), `cargo
check/clippy --features dox --all-targets`, `node mcp/smoke.mjs list-only mcp/server.mjs`, and
the server against a scripted fake pipe (discovery, token, errors). CI (fixture gate on every
run; packaged smoke and MCP package upload only with the MCP candidate opt-in,
`enable-mcp=true` or the `[cef-mcp-candidate]` commit marker): see "CI
(`cef-windows-player.yml`...)", MCP steps. On `9d9f43f5d97` the fixture smoke passed in CI
(run `45468335122`, WARP) and in one local run on a GTX 1050: evaluation (values, exception,
Promise refusal), trusted click, ArrowRight key, typed text, exit 0. (An earlier local run of
`9d691517300` failed the Promise refusal, fixed in `9d9f43f5d97`.) **Not verified yet:** the
packaged-launch MCP smoke and the MCP-enabled artifact (no candidate run yet), any session on
a real game.

**Limitations.** Windows only; Node.js 20+ needed by the server; one request in flight per
connection; `evaluate` is synchronous only and reads the main frame's main world; `click` is
left button only, no hover/drag/wheel; `press_key` has no modifiers and no character (use
`type_text`); text without browser focus (WARP) may not reach an `<input>` (the CI fixture
records `keypress`); no screenshot tool (welding's `request_snapshot_png` exists, not wired);
records of a crashed game stay until the PID is reused or the file is removed (the server
ignores dead PIDs).

**Integration contract for `roves-action` and Packmaster (not implemented there; see
`TODO.md`).** Input `enable-mcp` (Action) / "Enable MCP for AI assistants" (Packmaster),
default **false**. False: the package is produced exactly as today (no `mcp\` folder). True:
copy `support/cef-windows-player/mcp/{server.mjs,package.json,package-lock.json,README.md}`
into `<package>\mcp\`, run `npm ci --omit=dev --ignore-scripts` there (Node 20+), create the
empty file `<package>\mcp\enabled`, add `mcp-package-lock.json`/`mcp-npm-ls.txt` to the
provenance, and document the client entry `node <package>\mcp\server.mjs`. No different
engine binary is required. A target without this support (any non-Windows target, or a shell
release older than the one that ships the bridge) must fail with a clear error instead of
silently producing a package without MCP.

## Software paint (WARP and PCs without a GPU; implemented 2026-11-25, not yet run in CI)

Accelerated OSR delivers no importable frame on WARP (`Microsoft Basic Render Driver`,
`DeviceType::Cpu`: the CI runner, and what Windows uses on a PC without a GPU or driver), so
the window stayed black and the packaged game never left its startup screen. The player now
chooses the paint path once, before the browser exists (`game_rules::paint_path`, shared with
the Linux gate): on a CPU adapter, or with `ROVES_FORCE_CPU_PAINT=1` (tests), it creates the
browser with `prefer_accelerated = false`. The vendored welding (`support/vendor/wgpu-weld`,
changes and upstream texts in its `ROVES_PATCHES.md`) then uses CEF's standard software OSR
path: `CefWindowInfo::shared_texture_enabled = 0`, `CefRenderHandler::OnPaint` with a
`width*height*4` BGRA buffer and pixel dirty rects, at the same `windowless_frame_rate` (60).
Only the dirty rows are copied inside the callback; the union of the dirty rectangles is
uploaded with `Queue::write_texture` into one persistent `Bgra8Unorm` texture (recreated on
resize) that `acquire_frame` returns like an imported frame, so composition, the startup
gate, checks and readback are unchanged. Chromium's own software compositor works the same
way (`components/viz/service/display_embedder/software_output_device_win.cc`, damage rect
per frame, reference `f2700724f59f`).

Log lines: `paint path=Software("cpu-adapter") evidence=software`, `producer created
accelerated=false background=<white|transparent> evidence=software`, `import frame=1 ...
path=cpu-paint-upload evidence=software` (accelerated: `paint path=Accelerated
evidence=accelerated`, `path=dx12-shared-handle`). A software frame shows the game; it is no
evidence about the DX12 shared-handle import, which remains a real-GPU check. CI (WARP) now
requires the software path: 16 `PASS pixel` lines in the fixture run, the template package
released from its startup screen with a software frame (exit 0), the persistent-profile and
host API launches exit 0. Cost: one CPU copy of the dirty rows plus one texture upload per
painted frame (not measured; performance flow). In `--roves-sustained` the stall mailbox
re-poll is observational and sees no DX12 frame in software mode.

## Pipeline

1. CEF's `bootstrap.exe` (shipped as `roves_cef_windows_player.exe`) initializes the
   Windows sandbox broker and calls `RunWinMain` in `roves_cef_windows_player.dll`.
   welding's `CefWindowsSandboxContext::from_raw` borrows the instance and sandbox info,
   `execute_process` returns for subprocesses and `initialize` starts CEF with
   `CefSandboxMode::Sandboxed`. Without sandbox information the browser process exits 3;
   there is no unsandboxed path.
2. An SDL3 window and a wgpu 29 DX12 instance/surface with a single adapter/device/queue.
   The same `wgpu::Device` and `Queue` go to welding's `HostWgpuContext` (backend must be
   `Dx12`). One device: no second device, no wgpu-core id adoption, no wgpu patch.
3. `WindowsCefProducer` (welding) creates a windowless browser with
   `shared_texture_enabled` (accelerated OSR), `windowless_frame_rate = 60`, transparent
   background, on the fixture `src/fixture.html` loaded as a `data:` URL (no server).
4. Per paint, CEF calls `OnAcceleratedPaint` with a callback-scoped D3D11 shared NT handle.
   welding duplicates it, takes the keyed mutex and `CopyResource`s it on the GPU into a
   welding-owned shared texture, waits for that copy with a D3D11 event query, creates an
   NT handle, and grafting (`import_dx12_shared_texture`, d903cfa) opens it on the host's
   D3D12 device and wraps it as a `wgpu::Texture` (`TEXTURE_BINDING | COPY_SRC`). welding
   then submits a 1-texel copy into an unmapped buffer and waits for it (its documented
   cache-visibility step for the D3D11 -> D3D12 handoff).
5. One premultiplied-alpha blit composes the imported texture over a cleared background
   (red in the fixture checks) into an `Rgba8Unorm` target of the surface size.
6. A nearest-sampling blit draws the composed texture onto a non-sRGB surface
   (`Bgra8Unorm`/`Rgba8Unorm`, `Opaque`), viewport = source size. Present.

No step maps a buffer or copies pixels to the CPU to present. The only CPU readbacks are
the verification samples of the two checks below, issued after the frame was presented and
logged as `verification-readback ... (not on the presentation path)`.

## Checks (automatic mode)

After `load-end` and 20 imported frames whose size equals the surface:

| Point (fraction of the viewport) | Page content | Composed RGBA | CEF texel, premultiplied RGBA |
| --- | --- | --- | --- |
| (0.25, 0.25) | opaque `rgb(0,255,0)` | `[0,255,0,255]` | `[0,255,0,255]` |
| (0.75, 0.25) | `rgba(0,0,255,0.5)` | `[128,0,128,255]` (50% blue over red) | `[0,0,128,128]` |
| (0.25, 0.75) | nothing (transparent) | `[255,0,0,255]` (background) | `[0,0,0,0]` |
| (0.75, 0.75) | nothing (transparent) | `[255,0,0,255]` (background) | `[0,0,0,0]` |

Tolerance 3/255 per channel. The CEF texel row proves premultiplied alpha and real
transparency in the imported texture itself; the composed row proves the blend over the
background. Then the window is resized from 1280x720 to 960x540 (surface reconfigured, `producer.resize`), the probe waits for 20
frames at the new size (the imported frame must match the surface; it is never stretched)
and repeats the 4x2 samples. Then `producer.close()`, wait for CEF's close, drain the
queue, drop producer and renderer, then `CefRuntime` (`cef::shutdown`).

Exit codes: 0 both checks passed; 1 a pixel/resize/close failure or usage error; 2 timeout
(no load, no frames at the expected size, or close not completed) or fewer checks than
expected; 3 no sandbox; 4 `cef initialize` failed; 5 window/wgpu/producer startup
failed. `FAIL ...` lines name each failure.

Options: `--roves-log=<file>`, `--roves-cache-dir=<dir>` (default a per-run directory under `%TEMP%`),
`--roves-timeout-ms=<ms>` (default 120000), `--roves-keep-open` (one check, then the
window stays until the OS closes it), `--roves-timings` (below; off by default, CI does not
pass it), `--roves-sustained` with `--roves-warmup-ms=<ms>` (default 10000) and
`--roves-sample-ms=<ms>` (default 30000) (below; off by default, CI does not pass it,
exclusive with `--roves-keep-open`), `--roves-game-dir=<dir>` with `--roves-port=<n>` and
`--roves-auto-close-ms=<ms>` and one of the test-only `--roves-test-minimize-ms=<ms>`,
`--roves-test-click-ms=<ms>`, `--roves-test-key-ms=<ms>` (optionally with
`--roves-test-key=ArrowRight|CapsLock`) or `--roves-test-wheel-ms=<ms>` (opt-in
real-game mode, above; no fixture checks), and `--roves-no-error-dialog` and
`--roves-isolated-profile` (packaged launch only, "Packaged player" above). Without
`--roves-game-dir`, a `game\index.html` next to the executable selects the packaged launch
instead of the fixture; there `--roves-cache-dir` names the persistent profile root, not
CEF's root cache. Exit codes: 0 pass, 1 usage/setup error or failed check, 2 timeout,
incomplete run or failed startup, 3 sandbox, 4 CEF initialisation, 5 window/surface startup, 6 packaged-launch
contention (profile in use or stored origin port taken). CEF switches such as
`--log-file` pass through.

## Optional timings (`--roves-timings`)

**Smoke only.** Each smoke phase ends after about 20 imported frames at its size (well under
a second on a real GPU), so these summaries hold a few dozen samples that include the
first frames after load or resize. They show where the time goes, not steady-state pacing:
use `--roves-sustained` (next section) for that. With the option, each phase (`initial`, `resized`; `*-timeout` if it timed out) ends with a
header line and one line per stage, emitted **before** that phase's verification readback:

```text
roves-cef-win timings phase=initial clock=wall-monotonic unit=us window_ms=... presented=... imported=... superseded_producer_frames=... empty_acquire_polls=... dropped_samples=... excluded=loading,verification-readback,closing percentile=nearest-rank
roves-cef-win timings phase=initial stage=import count=... min=... p50=... p95=... p99=... max=... mean=...
```

Clock and meaning: every sample is **wall-clock time on the SDL3 thread** around one host
call, read with `std::time::Instant` (monotonic), in microseconds. It includes blocking
waits inside the call; it is not process CPU time and not GPU execution time (no GPU
timestamp queries).

| Stage | Covers |
| --- | --- |
| `events` | draining CEF navigation/title events (`poll_web_event` until empty) |
| `import` | `acquire_frame` calls that returned a frame: mailbox take, grafting's D3D12 open/wrap, welding's 1-texel copy and CPU wait. Calls that found nothing are only counted (`empty_acquire_polls`) |
| `compose` | encoding the composition blit |
| `compose-submit` | `queue.submit` of the composition blit |
| `acquire` | `surface.get_current_texture` (blocks on the 2-frame latency limit / vsync with `AutoVsync`); failed acquisitions are sampled too |
| `present-encode` | encoding the composed -> surface blit |
| `submit` | `queue.submit` of the surface blit |
| `present` | `pre_present_notify` + `present` |
| `frame` | whole redraw from its start to just after `present` |
| `present-interval` | between consecutive successful presents |
| `import-interval` | between consecutive imported producer frames |

`superseded_producer_frames` sums the gaps in welding's paint generation counter between
imports: paints replaced in its single-slot mailbox before the host took them (a failed
welding copy also leaves a gap, and welding logs it).

Exclusions: no sample before `load-end` (the frame that receives it included), after the
check of the last phase, or while closing. The summary is emitted and cleared before the
verification readback, and the next interval starts fresh, so readback time never enters a
sample. Surface reconfiguration (`Resized`) also breaks the intervals. The `resized` phase
deliberately includes frames that arrive at the old size until CEF catches up, since that is
what a resize costs. Samples are kept exactly, up to 65536 per stage and phase (more are
counted in `dropped_samples`), and summarised with nearest-rank percentiles; std only, no
new dependency. The aggregation is unit-tested stand-alone:
`rustc --edition 2024 --test src/timings.rs` (and the stall ring of the sustained mode:
`rustc --edition 2024 --test src/stall.rs`).

Not covered: welding's D3D11 copy and event-query wait run inside CEF's `OnAcceleratedPaint`
on CEF's UI thread (welding sets `multi_threaded_message_loop` on Windows), not on the SDL3
thread, so they are not in `import`; Chromium's own raster/capture cost is not visible.
Numbers are to be recorded only from real runs; none are claimed here.

## Startup timeline (always on)

Every run (fixture and game mode) logs one line per startup milestone, each at most once:

```text
roves-cef-win startup milestone=<name> elapsed_ms=<n>
```

`elapsed_ms` is wall-clock time (`std::time::Instant`, monotonic, whole milliseconds) since
**entry to the main probe run**: the instant in `app::run` right after CEF's subprocess
dispatch (`execute_process`) returned to the browser process. It therefore **excludes** OS
process creation, bootstrap.exe/DLL loading and the startup of CEF's child processes before
that point (child processes return earlier and never log milestones). It is **not**
`Probe.started`: that older instant is set later, after CEF and SDL initialization and just
before window/surface/producer creation, and remains the origin of `--roves-timeout-ms` (fixture
phases and game load) and of the `closing ... elapsed_ms=` value. Neither origin nor any
timeout changed.

| Milestone | Reached when |
| --- | --- |
| `options-log-ready` | options parsed, evidence log open, version lines written |
| `loopback-ready` | game mode only: loopback server bound and serving |
| `cef-initialized` | `CefRuntime` initialized (`cef initialized` line) |
| `sdl-ready` | SDL3 video subsystem and event pump created |
| `window-surface-producer-ready` | SDL window, DX12 adapter/device, surface and `WindowsCefProducer` (initial navigation requested) created |
| `load-start` | first `NavigationEvent::LoadStart` drained by the host loop |
| `load-end` | first **accepted** load-end (game: status OK on the loopback origin; fixture: any load-end) as processed by the host loop |
| `first-import` | first accelerated producer frame imported by `acquire_frame` |
| `first-present` | first surface presentation by the host, of any frame: it may be the host's background/blank frame and does **not** prove that a CEF frame or game content was imported or visually rendered |

Navigation milestones are stamped when the host loop drains the event, so they can trail
CEF's own callback by up to one loop turn. On WARP (CI) no accelerated frame arrives:
`first-import` is absent, but `first-present` is still logged, because the host presents its
background/blank frame without any import. Evidence: run 45073519738 (commit `ed4f4c374fc`)
logs `startup milestone=first-present` at roughly 299–475 ms on WARP, never logs
`first-import`, and its closing lines report `imported=0` (for example after 362, 366 and
5742 presents). Only `first-import` marks the first CEF frame reaching the host; neither
milestone proves visual correctness. A failure before a milestone (for example window or
producer creation) leaves it and every later one absent.

First recorded timeline, same run 45073519738 (commit `ed4f4c374fc`). That run has three
WARP game-mode runs; two are summarized here: the ordinary loading fixture and the
minimized-window fixture. The click-gate run is excluded (it was failing on the unrelated
pre-load input rejection described above), as is the blank initial fixture. Common to both:
`loopback-ready` 1 ms, `cef-initialized` 113–120 ms, `sdl-ready` 147–176 ms,
`first-present` 299–360 ms, `imported=0`. Per-run navigation pairs:

- ordinary loading fixture: `window-surface-producer-ready` 355 ms, `load-start` 1073 ms,
  `load-end` 1130 ms (producer-ready to `load-start` 718 ms; `load-start` to `load-end` 57 ms);
- minimized-window fixture: `window-surface-producer-ready` 295 ms, `load-start` 990 ms,
  `load-end` 1060 ms (producer-ready to `load-start` 695 ms; `load-start` to `load-end` 70 ms).

The largest observed gap is producer-ready (initial navigation requested) to `load-start`,
695–718 ms; `load-start` to `load-end` took 57–70 ms. These are two single CI fixtures on
WARP, not averages, not a game startup benchmark and not a user-hardware result.
The trace locates that delay; it does not identify its cause.

The timeline only locates **where** startup wall-clock time goes. It is not CPU time, not GPU
time, not a launch-click-to-ready figure and not an optimization result; one run proves
nothing about averages. The once-only rule is unit-tested in `src/game_rules.rs`
(`rustc --edition 2024 --test src/game_rules.rs`).

## Optional sustained sample (`--roves-sustained`)

Same stages, clock and summary format as above, over a steady-state window instead of the
smoke phases. The correctness smoke and its defaults are unchanged without the option.

1. The `initial` phase runs as usual (load, 20 imported frames at 1280x720, pixel check;
   timed only if `--roves-timings` is also given). The sustained part starts **only if that
   check passed** and the surface is exactly 1280x720; otherwise `FAIL sustained not
   started: ...`.
2. Warmup (`--roves-warmup-ms`, default 10 s): frames are presented but not recorded. The
   window is created non-resizable and there is no resize phase.
3. Sample (`--roves-sample-ms`, default 30 s): every redraw is recorded; no readback runs.
   At the end the log has `timings phase=sustained ...` (header with `window_ms`, presented,
   imported, superseded and dropped counts, then one line per stage with its `count`) and
   `sustained result=healthy phase=Sampling span_ms=<actual sample span> target_sample_ms=...
   warmup_ms=... presented=... imported=... superseded_producer_frames=... dropped_samples=...`.
4. The probe then closes the browser and shuts CEF down as in the smoke run; exit 0 only
   with one passed check and a healthy sample.

A sample is **rejected** (`sustained result=unhealthy`, `FAIL sustained sample rejected:
<reason>`, summary labelled `phase=sustained-unhealthy` when sampling had started, exit 2) if, during warmup or sample, no producer frame is imported for more than 500 ms (the
fixture repaints every 16 ms; checked after every redraw), the window size changes (the
resize is not sampled: recording stops before it is reported), the sample has no imported
or presented frame, any stage overflowed the 65536-sample cap, or the OS closes the window
first (`FAIL no healthy sustained sample completed`). `--roves-timeout-ms` bounds only the
load and initial phase; warmup and sample are bounded by their own durations and the stall
check. A rejection after warmup started also prints the `stall-trace` lines (and, for a
stall, one `stall-mailbox` line) described in "Attributing the warmup stalls"; a healthy
sample prints neither. The summary header still lists `excluded=loading,verification-readback,closing`;
in sustained mode warmup and resize are excluded too, as described here.

## Build and run

Heavy builds run in CI (`.github/workflows/cef-windows-player.yml`). Locally only:

```pwsh
cd support/cef-windows-player
cargo +1.99.1 check --locked --features dox --all-targets   # no CEF download
```

A real GPU run uses the `cef-windows-player-windows-x64` artifact of this branch:
`roves_cef_windows_player.exe --roves-log=probe.log` (SDL3 window, wgpu composition);
`--roves-timings` adds smoke-phase stage summaries and `--roves-sustained` a steady-state
sample. Building it
locally needs MSVC, CMake and
`cargo run --locked --release --bin bundle -- <dir> --release` (downloads CEF 154).

## Releases (`release.yml`, tags `v*.*.*`)

Pushing a tag `v*.*.*` runs this workflow, `cef-macos-gate.yml` and `cef-linux-gate.yml` as
reusable workflows on the tag, with all their checks (their push triggers are limited to
branches, so the tag does not run them a second time); this one gets `release: true`, which
always produces and smokes the MCP-enabled package and titles `PROVENANCE.txt` as a release.
A publish job (only when all three succeed) turns their artifacts into the release assets
`roves_shell_windows.zip`, `roves_shell_windows_steam.zip`, `roves_shell_windows_mcp.zip`
(package contents at the zip root), `roves_shell_macos.zip`, `roves_shell_macos_steam.zip`,
`roves_shell_macos_mcp.zip` (the gate's `ditto` zip of the `.app`), `roves_shell_linux.zip`,
`roves_shell_linux_steam.zip`, `roves_shell_linux_mcp.zip` (Info-ZIP with unix modes and
symlinks; `chrome-sandbox` ships 0755, see the Linux gate README "Running elsewhere") and
`SHA256SUMS.txt`. The three `_mcp` packages are the development-only MCP-enabled packages
("Local MCP bridge"): `mcp/enabled`, `mcp/server.mjs` and its installed SDK must be present
in them (macOS: under `Contents/Resources`) and `mcp` absent from every other zip. Before
publishing it checks each zip's layout and the Windows packages against their own
`SHA256SUMS.txt`; afterwards it requires all ten assets on the release. Each zip carries the visual-novel-template sample game and is the base
Roves Packmaster and roves-action repack with a developer's game. Re-cut a failed release by
deleting and pushing the tag again (the job replaces a release of the same name).

## CI (`cef-windows-player.yml`, push on this directory or manual)

Rust 1.99.1, separate cache key and `CEF_PATH` (CEF 154 never mixes with the stage-1 CEF
156 download). Steps: dependency-graph gate (`cargo tree`: welding, grafting and the
probe on wgpu 29.0.4; wgpu 30.0.2 only under `cef`), release bundle, run on WARP. Gated: sandbox information, DX12 adapter, surface and
producer lines, close/shutdown/exit lines, at least one `--type=renderer` child, no child
command line with `--no-sandbox` (sampled with `Win32_Process` while running), no process
left. If frames were imported, exit 0 and 16 `PASS pixel` lines are required; if WARP
delivers no accelerated frame, a notice says so and exit 2 is required (the GPU path is
then verified only on real hardware). The
workflow also triggers on the repository root `icon.png` (embedded by the startup screen),
and its last steps build, check, run and upload the directly playable package ("Packaged
player" above). On this branch only, the Servo + V8 Debug CI entrypoints were removed
(2026-11-23): `test.yml` no longer has `ensure-test-release`, `build-and-publish` and
`steam-emulator-smoke-test` (it keeps the two SDL3 checks), and `v8.yml` (roves-js/roves-v8
runtime tests and the Debug `servoshell-v8` build) is deleted, so a push here neither
rebuilds Servo nor overwrites the rolling `test` release. Other branches keep both files.

A second step reuses the same bundle (no second build) for an opt-in `--roves-game-dir`
**loading gate**: the step writes a two-file fixture (`index.html` loading the ES module
`game.js`, which logs `roves-ci-game-fixture module-ok same-origin=true`) and runs
`--roves-auto-close-ms=5000` on WARP. Gated, in log order: sandbox
information, DX12 adapter, `producer created ... background=white`, `load-end status=200` on
the logged loopback root with `ok=true`, the console marker with source `<origin>/game.js`,
`auto-close window`, close, `browser closed`, `cef shutdown complete`, `game loopback
stopped`, `exit code=0`; plus process exit 0, no `FAIL`/`close requested by the OS`/`popup
refused` line, a sandboxed renderer child and no leftover process. Imported frames are only
reported (WARP delivers none), so this step proves serving, navigation, script execution
and lifecycle, **not** accelerated frames, presentation, input or gameplay; there is no
window automation in CI. This step also fails on any `test minimize`, `test click` or
`test key` line:
without their flags the test hooks must stay off.

A third step reuses the same bundle for the **minimized-window suspension gate**: a fixture
whose `game.js` logs `roves-ci-lifecycle-fixture visibility=<state>` at start and on every
`visibilitychange`, run with `--roves-test-minimize-ms=1000
--roves-auto-close-ms=8000` on WARP. Gated, each line after the previous one: `producer
created accelerated=true background=white`, `load-end status=200` on the loopback root,
`auto-close scheduled delay_ms=8000`, `test minimize requested ok=true hold_ms=1500`,
`frame work suspended reason=minimized phase=Game`, the fixture's `visibility=hidden`
console marker (CEF events still drained while suspended), `test restore requested ok=true`,
`frame work resumed` with `suspended_ms >= 1500` and `idle_ticks >= 20`, `test minimize
cycle complete`, the `visibility=visible` marker, `auto-close window`, close, `browser
closed`, `producer and renderer dropped`, `cef shutdown complete`, `game loopback stopped`,
`exit code=0`; plus exactly one `frame work suspended` line, process exit 0, no
`FAIL`/`close requested by the OS`/`popup refused` line and no leftover process. The
`resume first-present`/`first-import` lines are only reported. This proves, on the runner's
window manager, that an SDL minimize suspends frame work while the loop keeps servicing
CEF events and deadlines, that CEF/the page see the window hidden and visible again, that
restore resumes from the window flags after the hold, and that close and shutdown stay
clean afterwards. It does **not** prove accelerated frames, GPU presentation or the
re-present on resume (WARP delivers no accelerated frame), input, visual correctness,
user-driven minimize/hide from the taskbar or other window managers, or the absence of GPU
work on real hardware while suspended.

A fourth step reuses the same bundle for the **synthetic click gate**. It first checks that
`--roves-test-click-ms` is refused without `--roves-game-dir` and together with
`--roves-test-minimize-ms` (exit 1, no evidence log written, a `usage error` on stderr if
any was captured). Then a fresh-cache run on a fixture with a fixed 200x120 px button at the
top-left, whose click handler logs `roves-test-click-received target=<id> trusted=<bool>
button=<n> detail=<n> x=<n> y=<n>`, with `--roves-test-click-ms=1000
--roves-auto-close-ms=5000` on WARP. Gated, each line after the previous one: `producer
created accelerated=true background=white`, the fixture's `module-ok` marker (listener
installed), `test click requested x=100 y=60 ... pushed=true`, `test click forwarded
event=down ok=true`, `test click forwarded event=up ok=true`, the click marker from
`<origin>/game.js` with `target=click-target trusted=true button=0 detail=1`, `test click
marker received`, `test click complete`, `auto-close window`, close, `game input
forwarded=`, `browser closed`, `producer and renderer dropped`, `cef shutdown complete`,
`game loopback stopped`, `exit code=0`; plus the accepted `load-end` before the request,
exactly one request, one forwarded down, one forwarded up and one click marker, process exit
0, no `FAIL`/`input rejected`/`close requested by the OS`/`popup refused`/`test minimize`/
`test key` line and no leftover process. Imported frames and presents are only reported. This proves
that an SDL-queued left click reaches welding and arrives in the page as one trusted DOM
click on the expected element; it does **not** prove physical mouse input, the Win32 ->
SDL translation, hover/motion, keyboard, wheel or focus delivery, gameplay, accelerated
frames or any visual result. If Blink were to drop input before a first paint on WARP, this
gate would fail at the marker rather than pass falsely.

A fifth step reuses the same bundle for the **synthetic key gate**. It first checks that
`--roves-test-key-ms` is refused without `--roves-game-dir`, together with
`--roves-test-click-ms`, together with `--roves-test-minimize-ms`, above 600000 ms and when
not a whole number (`1s`) (exit 1, no evidence log written, a `usage error` on stderr if any
was captured). Then a fresh-cache run on a fixture whose capture listeners on `window` log
`roves-test-key-received type=<type> key=<key> code=<code> keyCode=<n> repeat=<bool>
trusted=<bool> target=<node>` for `keydown`, `keypress` and `keyup`, with
`--roves-test-key-ms=1000 --roves-auto-close-ms=5000` on WARP. Gated,
each line after the previous one: `producer created accelerated=true background=white`, the
fixture's `module-ok` marker (listeners installed), `test key requested key=Right ...
pushed=true`, `test key forwarded event=down ok=true`, `test key forwarded event=up ok=true`,
the `type=keydown` marker from `<origin>/game.js` with `key=ArrowRight code=ArrowRight
keyCode=39 repeat=false trusted=true`, `test key marker received kind=Down`, the `type=keyup`
marker with the same fields, `test key marker received kind=Up`, `test key complete`,
`auto-close window`, close, `game input forwarded=`, `browser closed`, `producer and renderer
dropped`, `cef shutdown complete`, `game loopback stopped`, `exit code=0`; plus the accepted
`load-end` before the request, exactly one request, one forwarded down, one forwarded up, one
keydown and one keyup marker (two markers in total, so no `keypress`), process exit 0, no
`FAIL`/`input rejected`/`close requested by the OS`/`popup refused`/`test minimize`/
`test click` line and no leftover process. The click step likewise fails on any `test key`
line. Imported frames and presents are only reported; the `target` is logged, not gated.
This proves that one SDL-queued ArrowRight press reaches welding and arrives in the page as
one trusted `keydown` followed by one trusted `keyup` with the expected `key`, `code` and
`keyCode`. It does **not** prove a physical keyboard, the Win32 -> SDL translation, text
input/`Char` events, IME, modifiers, auto-repeat, other keyboard layouts, browser focus (the
probe never sets it on WARP; see the flag's limits above), gameplay, accelerated frames or any
visual result.

A sixth step, separate so the ArrowRight step stays as it is, repeats the key gate for the **synthetic CapsLock
regression** (the non-character VK_CAPITAL mapping). It first checks that `--roves-test-key`
is refused without `--roves-test-key-ms`, with `KeyA` and with `capslock` (exit 1, no
evidence log, `usage error` on stderr if captured). Then a fresh-cache run on the same
fixture with `--roves-test-key-ms=1000 --roves-test-key=CapsLock` gates, in order and with
the same counts, exit code and forbidden lines as the ArrowRight step: `test key requested
key=CapsLock ... pushed=true`, both `forwarded ... ok=true` lines, the `type=keydown` and
`type=keyup` markers with `key=CapsLock code=CapsLock keyCode=20 repeat=false trusted=true`,
their receipts, `test key complete` and the clean close/shutdown lines; additionally no
`key=Right`/`key=ArrowRight` line. Same limits as the ArrowRight gate; it also proves no
CapsLock lock-state toggle or `getModifierState('CapsLock')` result.

A seventh step reuses the same bundle for the **synthetic wheel gate** (not yet run in CI at
the time of writing). It first checks that `--roves-test-wheel-ms` is refused without
`--roves-game-dir`, together with the click, key or minimize hook, above 600000 ms and when
not a whole number (`1s`) (exit 1, no evidence log, `usage error` on stderr if captured).
Then a fresh-cache run on a fixture whose non-passive capture `wheel` listener on `window`
calls `preventDefault()` (so no scrolling perturbs the fixture) and logs
`roves-test-wheel-received type=<type> deltaX=<n> deltaY=<n> deltaZ=<n> deltaMode=<n>
wheelDeltaY=<n> trusted=<bool> cancelable=<bool> defaultPrevented=<bool> scrollY=<n>
target=<node>`, with `--roves-test-wheel-ms=1000
--roves-auto-close-ms=5000` on WARP. Gated, each line after the previous one: `producer
created accelerated=true background=white`, the fixture's `module-ok` marker, `test wheel
requested x=0 y=1 direction=normal ... pushed=true`, `test wheel forwarded delta_x=0
delta_y=120 ok=true`, the marker from `<origin>/game.js` with `type=wheel deltaX=0`, a
negative `deltaY`, `deltaZ=0`, `deltaMode` 0-2 and `trusted=true`, `test wheel marker
received delta_y=<negative> delta_mode=<0-2>`, `test wheel complete`, `auto-close window`,
close, `game input forwarded=`, `browser closed`, `producer and renderer dropped`, `cef
shutdown complete`, `game loopback stopped`, `exit code=0`; plus the accepted `load-end`
before the request, exactly one request, one forwarded line, one DOM wheel marker and one
receipt, process exit 0, no `FAIL`/`input rejected`/`close requested by the OS`/`popup
refused`/`test minimize`/`test click`/`test key` line and no leftover process. The other
synthetic steps now also fail on any `test wheel` line. The `deltaY` magnitude, `deltaMode`,
`wheelDeltaY`, `cancelable`, `defaultPrevented`, `scrollY` and `target` are logged as
notices, not gated. This proves that one SDL-queued wheel notch reaches welding with the
current conversion's CEF delta and arrives in the page as exactly one trusted DOM `wheel`
event scrolling up. It does **not** prove a physical wheel, the Win32 -> SDL translation,
input latency, actual scrolling, visible rendering, accelerated frames, the horizontal axis,
precision/touchpad deltas or parity of the delta magnitude with Chrome.

**Local real-GPU correctness checks (2026-11-23), separate from the CI/WARP gates above.**
The `cef-windows-player-windows-x64` artifact (ID 14852559736) of
[run 45245179096](https://github.com/DRincs-Productions/roves/actions/runs/45245179096), built
from commit `d9443fc5e0a5828cb6d1f341061412f740f77f4b`, was downloaded and run locally twice
on Windows with an NVIDIA GeForce GTX 1050 (DX12, driver 32.0.15.8163), CEF 154.0.34, wgpu
29.0.4, 1280x720: once per keyboard regression key, with the same artifact and fixture.

*Configured preference vs delivered frames (applies to both runs).* The local logs and the
WARP log of the same CI run all contain `producer created accelerated=true`, but that line is
printed after building `WindowsCefConfig` with `prefer_accelerated: true`: it reports the
requested preference, and `prefer_accelerated` itself is not evidence that CEF delivered an
accelerated frame. The local evidence of an actual accelerated import, in each run, is the
adapter line `type=DiscreteGpu` for the GTX 1050 together with
`import frame=1 ... path=dx12-shared-handle`, i.e. a shared-handle frame imported on the
discrete adapter. The CI WARP run used adapter `Microsoft Basic Render Driver` with
`type=Cpu` and has no `import frame=` lines, so it delivered no imported accelerated frame;
the CI gates are WARP evidence only, and the accelerated-import evidence below is local only.

- **CapsLock** (`--roves-test-key-ms=1000 --roves-test-key=CapsLock`): adapter GTX 1050
  `type=DiscreteGpu`, `import frame=1 ... path=dx12-shared-handle`; the forwarded key down/up
  had raw=0, usage=0x39, windows_key_code=0x14, native_key_code=0x003A. Both trusted DOM
  markers (`keydown` and `keyup`) had
  `key=CapsLock code=CapsLock keyCode=20 repeat=false trusted=true`. Exit code 0, clean
  browser/renderer/CEF shutdown, no leftover probe process; 742 frames were presented in
  14364 ms, a count that includes startup and teardown.
- **ArrowRight** (`--roves-test-key-ms=1000`, default key selector): adapter GTX 1050
  `type=DiscreteGpu`, `import frame=1 ... path=dx12-shared-handle`; the forwarded key down/up
  had raw=0, usage=0x4F, windows_key_code=0x27, native_key_code=0xE04D. Both trusted DOM
  markers (`keydown` and `keyup`) had
  `key=ArrowRight code=ArrowRight keyCode=39 repeat=false trusted=true`. Exit code 0, clean
  browser/renderer/CEF shutdown, no leftover probe process.

These are functional smoke tests on one machine, **not a performance result** (no
frame-interval, latency or CPU/GPU measurement, no Chrome comparison; the CapsLock frame count
is not a frame-pacing figure), and **not proof** of physical keyboard input, the Win32 -> SDL
translation, or CapsLock modifier/lock-state behavior; the limits of the gates above still
apply.

### MCP steps (fixture gate on every run; MCP candidate opt-in, default off)

Always (every run, pushes included): the plain package's direct launch must log no `mcp`
line and the package must have no `mcp\` folder (bridge off by default); steps (1) and (2)
below run in their own temporary folder and never change the uploaded plain package. Only
with the MCP candidate opt-in: step (3) and the upload of the MCP-enabled package.

**MCP candidate opt-in.** One job-level value, `ROVES_CEF_MCP_CANDIDATE`, gates both step (3)
and the upload; the assembly step logs a notice saying whether it is active. It is `true`
when either:

- the workflow is dispatched manually with `enable-mcp=true` (Actions -> "CEF -> wgpu-weld ->
  wgpu + SDL3 probe" -> Run workflow on `codex/cef-sdl3-wgpu-probe`; the input defaults to
  false), or
- a push to `codex/cef-sdl3-wgpu-probe` has `[cef-mcp-candidate]` in the message of its
  **head** commit (only the last commit of the push is read). The push must touch one of the
  workflow's `paths`, otherwise the workflow does not run; with no code change, an empty
  commit does not trigger it, so touch the workflow or the probe deliberately or use the
  manual input. The marker is ignored on every other branch, on tags and on manual runs.

Without either, no MCP-enabled artifact is produced and the plain package remains the only
playable upload. The plain package never changes with the opt-in. (1) `rustc
--test src/mcp_rules.rs`; the MCP-enabled package = the plain package (same `play.dll`) plus
`mcp\` (server, `npm ci --omit=dev` dependencies, README, marker `enabled`), provenance and
`SHA256SUMS.txt` regenerated; `smoke.mjs list-only` on the packaged server. (2) Fixture smoke
(`mcp/fixture`, CSP `script-src 'self'`, launched from the MCP package with
`--roves-game-dir --roves-mcp`, WARP): session record published at launch with the expected
shape, two unauthenticated connections answered with nothing, discovery by PID, state,
`40 + 2` = 42, `undefined` = null, an exception refused, `Promise.resolve(42)` and an async
call refused as Promises while `({})` = `{}` and a nested object come back by value, a
`Symbol` refused as not JSON-serializable, `eval` inside an
evaluation still an `EvalError` (page CSP kept), no bridge global in the page, a trusted DOM
click at (100, 60), ArrowRight keydown/keyup (`key`/`code` ArrowRight, trusted), typed text
`roves` seen by a `keypress` listener, out-of-window click, unlisted key and control
characters refused, exit 0 after auto-close, record removed, token absent from the log, no
`FAIL`, a later call reporting the game not running. (3) Packaged smoke: `play.exe` started
directly (marker on, `--roves-isolated-profile --roves-no-error-dialog`), discovered
without a PID; state and evaluation on the package origin; `click` forwarded if the startup
screen was released, otherwise refused as not ready (WARP, exit 2); record removed. Artifacts:
`cef-windows-player-mcp-evidence` (every run) and, with the MCP candidate opt-in only,
`visual-novel-template-roves-cef-mcp-windows-x64`. Steps (1) and (2) passed on `9d9f43f5d97`
(run `45468335122`); step (3) and the MCP-enabled upload have not run yet.

## Versions evaluated (2026-11-22)

| Component | Pin | Notes |
| --- | --- | --- |
| wgpu-weld / `welding` | `e991684` (main, 2026-11-20, crate 0.15.0), vendored since 2026-11-25 in `support/vendor/wgpu-weld` as a path dependency with the Roves CPU-paint fallback ("Software paint", `ROVES_PATCHES.md`) | `default-features = false`, `wgpu-29` + `cef-runtime` + `cpu-paint-fallback`. `welding/src` is unchanged since the 0.15.0 release-notes commit `5e7ce9f`; later commits change only the manifest (grafting from git main, wgpu 30.0.2) and repository tooling. Not compared byte-for-byte with the crates.io 0.15.0 package. MSRV 1.97.1; its CI toolchain 1.99.1. MPL-2.0 |
| `grafting` / `grafting-frame` | git d903cfa (0.6.1 / 0.1.1) | Pinned with `cargo update --precise` to the revision wgpu-weld's own `Cargo.lock` reviewed at e991684 (grafting main is at df803e0, 0.6.3). MPL-2.0 |
| `cef` / `cef-dll-sys` | `=154.5.0+154.0.34` (Chromium 154.0.8037.98) | sha256 `495579dc...930b` / `a52b7101...5f69`, equal to wgpu-weld's lock |
| `wgpu` | `=29.0.4` (wgpu-core/hal/types 29.0.4, naga 29.0.4) | welding's `wgpu-29` row |
| `sdl3` | `0.20` (SDL3 built from source, static) | window, events, input |

Compatibility findings:

- **CEF 154 vs the baseline's CEF 156: not unifiable today.** welding e991684 requires
  `cef ^154.5.0`, and every `cef-dll-sys` declares `links = "cef_dll_wrapper"`, so 154 and
  156 cannot coexist in one Cargo graph; moving welding to 156 would need welding's CEF
  handler code updated (its 151 -> 154 migration recorded a handler-signature diff). Hence
  a separate workspace, a separate CEF download and the baseline untouched. Unification
  trigger: a welding release on the CEF the baseline uses (or the baseline moving to
  welding's).
- **wgpu-29 row: one wgpu 29.0.4.** welding/grafting declare `wgpu ^29.0.1`; one wgpu
  29.0.4 serves them (`cargo tree -d`). The `wgpu-29` row has DX12 and Metal receipts from earlier releases; wgpu-weld's CEF 154
  Windows qualification (`docs/receipts/cef154_windows_20261007`, 4096 pixel samples,
  sandboxed DX12) was taken on the `wgpu-30` row, so the 29 row on CEF 154 is unverified
  until this probe runs.
- **Graph cost, not a soundness issue:** welding enables `cef`'s `accelerated_osr`
  feature, which pulls wgpu 30.0.2 (+ core/hal/naga 30) for `cef::osr_texture_import`;
  welding does not call that module (it only needs `AcceleratedPaintInfo`, which is not
  feature-gated). The wgpu 30 copy is compiled, never instantiated. Downstream cannot turn
  off a dependency's feature; the fix is upstream in welding (drop `accelerated_osr` or
  gate it per row). Not filed yet.

## Chromium reference (pinned `f2700724f59f`, Chromium 160.0.8162.0)

Consulted before writing code (main checkout's sparse `support/chromium-reference`,
`components/viz` and `cc`):

- `components/viz/service/frame_sinks/video_capture/frame_sink_video_capturer_impl.cc`
  (pixel-format note at line ~86, `GetVideoFramePoolForFormat`): with
  `kPreferSharedImageWithNativeHandle` + ARGB the capturer requests **BGRA** shared images;
  `kFramePoolCapacity = kDesignLimitMaxFrames + 1 = 11` frames in flight
  (`frame_sink_video_capturer_impl.h:185`), and the minimum capture period is clamped to
  `media::limits::kMaxFramesPerSecond`. Hence: expect `Bgra8Unorm`, and copy/release each
  CEF frame promptly (welding copies inside the callback) so the producer's pool is never
  starved.
- `components/viz/common/frame_sinks/copy_output_result.h:39`: Viz copy results are
  "N32Premul" (premultiplied); RGBA texture results are `kRGBA_8888` shared images.
  Hence `SourceAlpha::Premultiplied` and the premultiplied CEF texel expectations.
- `components/viz/service/display/direct_renderer.cc:250-270`: transparency of the root
  render pass is an explicit, costlier mode (`has_transparent_background`); a transparent
  page background is a deliberate request (CEF `background_color = 0`), not the default.

Scheduling and timing reference, consulted for `--roves-timings` (same revision):

- `components/viz/service/display/display_scheduler.cc` (`DisplayScheduler::DrawAndSwap`
  ~322, `AttemptDrawAndSwap` ~801, "Swap throttled" instant ~717): Viz draws on a
  BeginFrame deadline and throttles on pending swaps. The probe has no BeginFrame source: it
  redraws continuously (`ControlFlow::Poll`) and is throttled only by the swap chain
  (`desired_maximum_frame_latency = 2`, `AutoVsync`), which surfaces as time in `acquire`.
  That is why `acquire` is its own stage and `frame` is reported next to `present-interval`.
- `components/viz/service/display/display.cc` (`Display::DrawAndSwap` ~751,
  `base::ElapsedTimer aggregate_timer` ~829, `draw_timer` ~935, `DidReceiveSwapBuffersAck`
  ~1162 with `Swap`/`WaitForPresentation` instants from `gfx::SwapTimings`): Chromium times
  the CPU-side stages (aggregate, draw) with monotonic `base::TimeTicks` in microseconds and
  takes swap start/end from the GPU side separately. The probe mirrors the CPU-side split
  (`import` ~ surface aggregation of the producer frame, `compose` ~ draw, `submit` +
  `present` ~ swap issue) with `std::time::Instant` in microseconds, and does not claim the
  GPU side: there is no equivalent of `SwapTimings` or presentation feedback here.
- `cc/metrics/compositor_frame_reporter.h:127` (`VizBreakdown`: submit-to-receive,
  received-to-start-draw, start-draw-to-swap-start, swap-start-to-swap-end,
  swap-end-to-presentation): stage names follow that order. Chromium aggregates in bucketed
  UMA histograms (e.g. `Compositing.SurfaceAggregator.AggregateUs`) and per-frame Perfetto
  trace events; the probe keeps exact bounded samples and prints percentiles in its log
  instead (no trace or histogram dependency for a one-shot comparison).

Not verified here: CEF's own OSR glue (`CefVideoConsumerOSR`, which drives Viz's
`FrameSinkVideoCapturer` for `OnAcceleratedPaint`) is not in the Chromium checkout; the
claim that OSR frames come from that capturer is CEF's documented design, not something
this probe read. CEF 154 embeds Chromium 154.0.8037.98, older than the reference; the
behaviours above are long-standing.

Differences from Chromium's own model: Chromium presents through its own compositor and
DirectComposition; here the frame leaves Chromium as a captured texture and is composed by
an external renderer, so every frame pays the capture copy inside Chromium, welding's GPU
copy and two CPU waits (below). That is the cost of keeping Chromium unmodified.

## Limitations and open questions

- Verified on one real GPU (GTX 1050) only; WARP delivers no accelerated OSR frame
  (renderers get `--disable-gpu-compositing`), so CI cannot exercise the import path.
- Per frame welding creates a D3D11 texture and an NT handle, the host opens it on D3D12,
  and the CPU waits twice (D3D11 event query in CEF's paint callback on CEF's UI thread; the
  1-texel D3D12 submission in `acquire_frame` on the host thread). GPU-only data movement,
  but synchronous. `--roves-timings` measures the host-thread part; real-scene frame pacing
  remains with the performance flow.
- The fixture smoke exercises no input. Real-game mode forwards mouse, wheel, keyboard,
  focus, visibility, scale factor and cursor shape. The ordinary CI game smoke covers only
  loading and lifecycle on WARP. The synthetic CI input gates passed on Windows/WARP in
  [run 45245179096](https://github.com/DRincs-Productions/roves/actions/runs/45245179096)
  (commit `d9443fc5e0a5828cb6d1f341061412f740f77f4b`): the SDL click gate (SDL event queue
  -> welding -> CEF/Blink DOM click path), the ArrowRight key gate (SDL keydown/up -> welding
  -> CEF, trusted DOM `key=ArrowRight code=ArrowRight keyCode=39`) and the CapsLock key gate
  (same path, trusted DOM `key=CapsLock code=CapsLock keyCode=20`). These are WARP tests
  only: they exercise no accelerated OSR import, no physical input, and no text, wheel,
  focus, gameplay or visual behavior. The local real-GPU key checks are a separate record
  ("Local real-GPU correctness checks" in the CI section above). A synthetic wheel gate
  (SDL wheel queue -> welding -> CEF/Blink DOM `wheel`, direction only) has been added to
  the workflow but has no CI result yet. Physical mouse input,
  physical keyboard, text input, wheel, focus, gameplay and visuals remain
  **unverified**, and input has
  these known gaps (welding e991684 API or scope): no IME (composition events, CJK input),
  no `mouseleave` (welding always sends `mouse_leave = 0`, hover can stick when the cursor
  leaves the window), no blur (welding's Windows focus call is `SetFocus(true)` only), no
  `is_system_key` for Alt combinations, Back/Forward mouse buttons dropped, non-BMP
  characters (emoji) dropped, numpad keys with NumLock off may map to their NumLock-on VK,
  horizontal wheel sign not verified, no touch/pen/drag-and-drop, `<select>` popups not
  drawn (welding's `acquire_popup` is not composed), no gamepad, no audio verification, no
  DevTools window (the optional local MCP bridge uses CDP in-process only, "Local MCP
  bridge"). Off-origin main-frame navigations are vetoed before they start and external
  links open in the system browser after a user gesture ("External links"); verified by the
  macOS/Linux player-fixture CI steps in log-only mode, not yet on Windows CI and not with a
  real system browser.
- Local real-game smoke (CI artifact of commit `1565935d74a`, run 45066625026; template SHA
  `e88e9ac29e7`; Windows, GTX 1050 / DX12, 1280x720): main route
  returned 200, CEF imported frame 1, clean exit (code 0) after 581 imports and 2452
  presents. Console: two PixiJS Assets "don't know how to parse it" warnings for the remote
  `sfx_whoosh.wav` and `bgm_cheerful.wav`, and Tone.js WebAudio autoplay blocked before a user
  gesture. Startup observation (derived from the log, not measured directly): the log
  records `auto-close scheduled delay_ms=15000` at the accepted main-frame load-end and
  later `closing ... elapsed_ms=48987`; the close was caused by auto-close, so the host
  processed load-end about 33,987 ms after `Probe.started` (48,987 - 15,000; the real
  value can be lower by at most one event-loop turn of auto-close overshoot). This is a
  probe-relative interval, **not** launch-click-to-ready: `Probe.started` is set after the
  CEF runtime and SDL are initialized and before `probe.start()` creates the window,
  surface and CEF producer, so the interval includes window/surface creation and CEF
  navigation startup, and excludes process launch and CEF/SDL initialization. It is a
  single local Windows GTX 1050 run, not an average or a benchmark, with no Chrome
  comparator, and it does not identify which stage takes the time. The derived ~33.987 s
  is only probe-relative (from `Probe.started`); that run predates the startup timeline
  ("Startup timeline" above), which future runs log with its own origin (main probe run
  entry) to break the interval down by phase (runtime/SDL init, window/surface/producer,
  load-start, load-end, first import/present). Not done: Chrome comparison, audible-output
  check, gameplay verification. Follow-up: compare the same warnings in Chrome and check
  audio after a real gesture; record the startup timeline on a real-GPU run and compare
  load time of the same template SHA in Chrome on the same machine.
- A `GPUDevice`/context loss inside CEF, or `device.destroy()` on the host, is not handled
  beyond logging uncaptured wgpu errors as `FAIL`.
- Windows x64 only; no code signing; the bundle DLL and bootstrap.exe must stay unsigned
  or be signed together (CEF bootstrap rule).
