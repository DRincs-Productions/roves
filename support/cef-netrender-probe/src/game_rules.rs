//! Pure helpers of the `--roves-game-dir` mode: origin policy, keyboard translation, the
//! input-readiness gate, the minimized/hidden suspension predicate and the test-only minimize, click, key and wheel sequences, the startup-screen layout and loading-bar animation (std only,
//! unit-tested stand-alone: `rustc --edition 2024 --test src/game_rules.rs`).
//!
//! CEF 154 on Windows (`CefBrowserPlatformDelegateNativeWin::TranslateUiKeyEvent`, CEF commit
//! 14c5a089e845) takes the DOM `code` from `cef_key_event_t.native_key_code` with
//! `ui::KeycodeConverter::NativeKeycodeToDomCode(native_key_code)`: an exact lookup in the
//! Windows column of Chromium's `ui/events/keycodes/dom/dom_code_data.inc` (pinned reference
//! f2700724f59f). That column holds scan codes in the form `GetScanCodeFromLParam`
//! (`ui/events/win/events_win_utils.cc`) produces: the make code, with `0xE0` in the high
//! byte for extended keys (ArrowRight `0xE04D`, Numpad6 `0x004D`). A Win32 `lParam` (repeat
//! count, scan code in bits 16-23, state bits) matches no entry, so `KeyboardEvent.code`
//! stays empty; a missing `0xE0` turns ArrowRight into Numpad6.

/// Whether a main-frame URL stays on the game's loopback origin. `origin` has no trailing
/// slash; `http://127.0.0.1:1234` must not match `http://127.0.0.1:12345/`, nor
/// `http://127.0.0.1:1234@evil.example/`.
pub(crate) fn on_origin(origin: &str, url: &str) -> bool {
    url.strip_prefix(origin)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// Main-frame document statuses accepted as a successful game load (304: revalidated from a
/// reused profile's HTTP cache).
pub(crate) fn load_status_ok(status: i32) -> bool {
    (200..300).contains(&status) || status == 304
}

/// SDL scancodes are USB HID keyboard-page usages. These are the keys whose Windows scan code
/// in Chromium's table carries the `0xE0` prefix and whose low byte `MapVirtualKeyW(..,
/// MAPVK_VK_TO_VSC_EX)` reproduces, but not always with the prefix: the navigation cluster
/// and arrows (0x49-0x52) share their virtual keys with the NumLock-off keypad, so Windows
/// returns the keypad scan code (VK_RIGHT -> `0x4D`, i.e. Numpad6); NumpadEnter shares
/// VK_RETURN. Also NumLock, NumpadDivide, ContextMenu, MetaLeft, ControlRight, AltRight and
/// MetaRight. PrintScreen (`0xE037`) is left out: its virtual key maps to a different low byte.
pub(crate) fn extended_usage(usage: u32) -> bool {
    matches!(usage, 0x49..=0x54 | 0x58 | 0x65 | 0xE3 | 0xE4 | 0xE6 | 0xE7)
}

/// USB HID usage of the Pause key; SDL's Windows backend rewrites its scan code.
const USAGE_PAUSE: u32 = 0x48;

/// `cef_key_event_t.native_key_code` for a key down/up on Windows: the scan code in Chromium's
/// `GetScanCodeFromLParam` form (see the module comment), 0 when unknown (`code` empty).
///
/// `raw` is SDL's `SDL_KeyboardEvent.raw`, which SDL's Windows backend fills with exactly that
/// form (`WindowsScanCodeToSDLScanCode`: make code, `0xE0` high byte when the message has
/// `KF_EXTENDED`), so a real key uses it unchanged. The one exception is Pause, which SDL
/// rewrites to `0xE046` (`0xE145` on the raw-input path) while Windows and Chromium use
/// `0x0045`. `raw` is 0 when SDL had no scan code (or for a pushed event); then the scan code
/// comes from `mapped` (`MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX)`) with the `0xE0` prefix
/// restored from the layout-independent SDL scancode `usage` (`extended_usage`).
pub(crate) fn native_key_code(raw: u16, usage: Option<u32>, mapped: u32) -> i32 {
    if usage == Some(USAGE_PAUSE) {
        return 0x0045;
    }
    if raw != 0 {
        return i32::from(raw);
    }
    let mut scan = mapped & 0xFFFF;
    if scan != 0 && scan & 0xFF00 == 0 && usage.is_some_and(extended_usage) {
        scan |= 0xE000;
    }
    scan as i32
}

/// SDL's keycode for a key without a character: `SDLK_SCANCODE_MASK | scancode`
/// (`SDL_keycode.h`). SDL 3.4.16's Windows keymap keeps that default for every such key.
pub(crate) const SDLK_SCANCODE_MASK: u32 = 1 << 30;

/// `cef_key_event_t.windows_key_code` for an SDL keycode that names a key without a character
/// (`SDLK_SCANCODE_MASK | usage`): the Win32 virtual key a real `WM_KEYDOWN` carries, 0 when
/// the key is unknown or `keycode` is a character (those go through `VkKeyScanW`). CEF 154
/// derives the DOM `key` only from this value (`PlatformKeyMap::DomKeyFromKeyboardCode`), so a
/// missing entry turns e.g. CapsLock or Pause into `key="Unidentified"`, `keyCode` 0. VK values:
/// Chromium's `ui/events/keycodes/keyboard_codes_win.h`. Keypad keys map to the NumLock-on
/// virtual keys; the virtual keys Windows reports with NumLock off are not reproduced.
pub(crate) fn non_character_virtual_key(keycode: u32) -> u32 {
    if keycode & SDLK_SCANCODE_MASK == 0 {
        return 0;
    }
    match keycode & !SDLK_SCANCODE_MASK {
        0x39 => 0x14,                               // CapsLock -> VK_CAPITAL
        usage @ 0x3A..=0x45 => 0x70 + usage - 0x3A, // F1..F12 -> VK_F1..VK_F12
        0x46 => 0x2C,                               // PrintScreen -> VK_SNAPSHOT
        0x47 => 0x91,                               // ScrollLock -> VK_SCROLL
        0x48 => 0x13,                               // Pause -> VK_PAUSE
        0x49 => 0x2D,                               // Insert
        0x4A => 0x24,                               // Home
        0x4B => 0x21,                               // PageUp -> VK_PRIOR
        0x4D => 0x23,                               // End
        0x4E => 0x22,                               // PageDown -> VK_NEXT
        0x4F => 0x27,                               // ArrowRight
        0x50 => 0x25,                               // ArrowLeft
        0x51 => 0x28,                               // ArrowDown
        0x52 => 0x26,                               // ArrowUp
        0x53 => 0x90,                               // NumLock
        0x54 => 0x6F,                               // NumpadDivide
        0x55 => 0x6A,                               // NumpadMultiply
        0x56 => 0x6D,                               // NumpadSubtract
        0x57 => 0x6B,                               // NumpadAdd
        0x58 => 0x0D,                               // NumpadEnter -> VK_RETURN
        usage @ 0x59..=0x61 => 0x61 + usage - 0x59, // Numpad1..9 -> VK_NUMPAD1..9
        0x62 => 0x60,                               // Numpad0
        0x63 => 0x6E,                               // NumpadDecimal
        0x65 => 0x5D,                               // ContextMenu -> VK_APPS
        usage @ 0x68..=0x73 => 0x7C + usage - 0x68, // F13..F24 -> VK_F13..VK_F24
        0x7F => 0xAD,                               // AudioVolumeMute
        0x80 => 0xAF,                               // AudioVolumeUp
        0x81 => 0xAE,                               // AudioVolumeDown
        0x8A => 0x1C,                               // Convert (INTERNATIONAL4) -> VK_CONVERT
        0x8B => 0x1D,                               // NonConvert (INTERNATIONAL5) -> VK_NONCONVERT
        0xE0 => 0xA2,                               // ControlLeft -> VK_LCONTROL
        0xE1 => 0xA0,                               // ShiftLeft -> VK_LSHIFT
        0xE2 => 0xA4,                               // AltLeft -> VK_LMENU
        0xE3 => 0x5B,                               // MetaLeft -> VK_LWIN
        0xE4 => 0xA3,                               // ControlRight -> VK_RCONTROL
        0xE5 => 0xA1,                               // ShiftRight -> VK_RSHIFT
        0xE6 => 0xA5,                               // AltRight -> VK_RMENU
        0xE7 => 0x5C,                               // MetaRight -> VK_RWIN
        0x102 => 0x5F,                              // Sleep
        0x10B => 0xB0,                              // MediaTrackNext
        0x10C => 0xB1,                              // MediaTrackPrevious
        0x10D => 0xB2,                              // MediaStop
        0x10F => 0xB3,                              // MediaPlayPause
        0x110 => 0xB5,                              // MediaSelect -> VK_LAUNCH_MEDIA_SELECT
        0x118 => 0xAA,                              // BrowserSearch
        0x119 => 0xAC,                              // BrowserHome
        0x11A => 0xA6,                              // BrowserBack
        0x11B => 0xA7,                              // BrowserForward
        0x11C => 0xA9,                              // BrowserStop
        0x11D => 0xA8,                              // BrowserRefresh
        0x11E => 0xAB,                              // BrowserFavorites (SDL AC_BOOKMARKS)
        _ => 0,
    }
}

/// `MapVirtualKeyW(.., MAPVK_VSC_TO_VK_EX)` distinguishes left/right modifiers, while the
/// `wParam` of a real `WM_KEYDOWN` (what Chromium sees natively and what DOM `keyCode`
/// reports) carries the generic VK_SHIFT/VK_CONTROL/VK_MENU.
pub(crate) fn fold_sided_modifier(vk: u32) -> u32 {
    match vk {
        0xA0 | 0xA1 => 0x10, // VK_LSHIFT / VK_RSHIFT -> VK_SHIFT
        0xA2 | 0xA3 => 0x11, // VK_LCONTROL / VK_RCONTROL -> VK_CONTROL
        0xA4 | 0xA5 => 0x12, // VK_LMENU / VK_RMENU -> VK_MENU
        other => other,
    }
}

/// CEF's `character` is one UTF-16 unit and welding casts `char as u16`, so only characters
/// of the Basic Multilingual Plane survive; others are skipped (a documented limitation).
pub(crate) fn bmp_chars(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars().filter(|c| u32::from(*c) <= 0xFFFF)
}

/// Pixels sent per winit `LineDelta` unit. `WHEEL_DELTA` (120) is what one notch of a
/// `WM_MOUSEWHEEL` carries and what CEF's own Windows OSR sample forwards; Chromium turns
/// a wheel offset into `wheel_ticks = offset / kWheelDelta`
/// (`ui/events/blink/web_input_event.cc`). Native Chrome scrolls 100/3 px per line, 3 lines
/// per notch (`web_input_event_builders_win.cc`), i.e. 100 px, so DOM `deltaY` magnitudes
/// differ from Chrome; only the direction and tick count are expected to match.
pub(crate) const WHEEL_DELTA_PER_LINE: f32 = 120.0;

/// The `(delta_x, delta_y)` handed to welding's `MouseAction::WheelScrolled` (and from there
/// unchanged to CEF `SendMouseWheelEvent`) for an SDL wheel event of `x`/`y` lines;
/// `flipped`: SDL reports `MouseWheelDirection::Flipped` (natural scrolling), whose values
/// are negated back to the normal direction.
pub(crate) fn wheel_cef_delta(x: f32, y: f32, flipped: bool) -> (i32, i32) {
    let factor = if flipped {
        -WHEEL_DELTA_PER_LINE
    } else {
        WHEEL_DELTA_PER_LINE
    };
    ((x * factor).round() as i32, (y * factor).round() as i32)
}

/// What an SDL event is for the game-mode input path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputEventKind {
    /// Mouse motion, buttons and wheel, key down/up and text input: input aimed at the page.
    User,
    /// Window lifecycle, visibility, focus, scale, resize, close: producer and window state.
    Window,
}

/// Whether the game-mode input path handles an event now. User input is discarded until the
/// accepted game load-end has been handled: before it welding has no CEF browser host yet and
/// refuses page input, and the page the input was aimed at does not exist. Window events always
/// go through (welding stores visibility and scale for the browser it is still creating).
/// After load-end nothing is discarded, so later welding refusals still surface.
pub(crate) fn forward_input(kind: InputEventKind, game_loaded: bool) -> bool {
    game_loaded || kind == InputEventKind::Window
}

/// A frame-work suspension change decided from the SDL window flags (every mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Suspension {
    Suspend,
    Resume,
}

/// Frame work is suspended while the window is minimized or hidden and resumes only once it
/// is neither, so a `Shown` that arrives while still minimized changes nothing. Occlusion is
/// not an input: it never suspends.
pub(crate) fn suspension_change(
    suspended: bool,
    minimized: bool,
    hidden: bool,
) -> Option<Suspension> {
    match (suspended, minimized || hidden) {
        (false, true) => Some(Suspension::Suspend),
        (true, false) => Some(Suspension::Resume),
        _ => None,
    }
}

/// `--roves-test-minimize-ms` only: how long the scripted minimize keeps frame work suspended
/// before the restore is requested (about 90 suspended loop turns at the 16 ms tick).
pub(crate) const TEST_MINIMIZE_HOLD: std::time::Duration = std::time::Duration::from_millis(1500);
/// `--roves-test-minimize-ms` only: longest wait for the window to report the requested
/// minimize or restore before the run fails.
pub(crate) const TEST_TRANSITION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// `--roves-test-minimize-ms` (test only, game mode): one scripted minimize -> hold ->
/// restore cycle after the game's load-end. Driven from the loop's deadline handling, which
/// also runs on every suspended turn, so the restore request itself proves that deadlines
/// are serviced while frame work is suspended. The window manager decides the transitions;
/// the cycle only observes the probe's suspension state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MinimizeCycle {
    /// Waiting for load-end + the requested delay.
    Waiting,
    Minimizing(std::time::Instant),
    /// Suspended since this instant.
    Holding(std::time::Instant),
    Restoring(std::time::Instant),
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CycleAction {
    None,
    Minimize,
    Restore,
    Complete,
    Fail(&'static str),
}

impl MinimizeCycle {
    /// `due`: load-end + delay has passed. `suspended`: the probe's frame-work state.
    pub(crate) fn step(
        &mut self,
        now: std::time::Instant,
        due: bool,
        suspended: bool,
    ) -> CycleAction {
        let (next, action) = match *self {
            MinimizeCycle::Waiting if suspended => (
                MinimizeCycle::Failed,
                CycleAction::Fail("frame work suspended before the scripted minimize"),
            ),
            MinimizeCycle::Waiting if due => {
                (MinimizeCycle::Minimizing(now), CycleAction::Minimize)
            },
            MinimizeCycle::Minimizing(_) if suspended => {
                (MinimizeCycle::Holding(now), CycleAction::None)
            },
            MinimizeCycle::Minimizing(at) if now - at > TEST_TRANSITION_TIMEOUT => (
                MinimizeCycle::Failed,
                CycleAction::Fail("frame work not suspended after the minimize request"),
            ),
            MinimizeCycle::Holding(_) if !suspended => (
                MinimizeCycle::Failed,
                CycleAction::Fail("frame work resumed before the scripted restore"),
            ),
            MinimizeCycle::Holding(since) if now - since >= TEST_MINIMIZE_HOLD => {
                (MinimizeCycle::Restoring(now), CycleAction::Restore)
            },
            MinimizeCycle::Restoring(_) if !suspended => {
                (MinimizeCycle::Done, CycleAction::Complete)
            },
            MinimizeCycle::Restoring(at) if now - at > TEST_TRANSITION_TIMEOUT => (
                MinimizeCycle::Failed,
                CycleAction::Fail("frame work not resumed after the restore request"),
            ),
            state => (state, CycleAction::None),
        };
        *self = next;
        action
    }

    /// Done or failed: `--roves-auto-close-ms` may close the window.
    pub(crate) fn finished(self) -> bool {
        matches!(self, MinimizeCycle::Done | MinimizeCycle::Failed)
    }
}

/// `--roves-test-click-ms` only: SDL window point (logical coordinates) of the synthetic
/// left click. The CI fixture puts its target button over this point (a fixed 200x120 CSS px
/// box at the top-left corner), so a display scale of up to about 1.6 still lands inside it.
pub(crate) const TEST_CLICK_POINT: (f32, f32) = (100.0, 60.0);
/// `--roves-test-click-ms` only: SDL mouse id stamped on the pushed button events, so the
/// probe attributes exactly those events to the test (never a physical mouse).
pub(crate) const TEST_CLICK_MOUSE_ID: u32 = 0x524F_5645;
/// `--roves-test-click-ms` only: the console message (or its first word) the fixture's click
/// handler logs. Accepted only from a source on the game's loopback origin.
pub(crate) const TEST_CLICK_MARKER: &str = "roves-test-click-received";

/// Whether a console message is the fixture's click marker (`TEST_CLICK_MARKER`, alone or
/// followed by a space and details).
pub(crate) fn is_click_marker(message: &str) -> bool {
    message
        .strip_prefix(TEST_CLICK_MARKER)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
}

/// A synthetic button event of the test click after it went through the input path; `ok` is
/// whether welding accepted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickInput {
    Down { ok: bool },
    Up { ok: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickAction {
    None,
    /// Push the synthetic button down + up into the SDL queue.
    Push,
    Complete,
    Fail(&'static str),
}

/// `--roves-test-click-ms` (test only, game mode): one synthetic left click after the game's
/// load-end, observed in order: requested (pushed into the SDL queue), button down and up
/// forwarded through welding, then the page's click marker. Each pending state carries the
/// request instant; everything must happen within `TEST_TRANSITION_TIMEOUT` of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickTest {
    /// Waiting for load-end + the requested delay.
    Waiting,
    Requested(std::time::Instant),
    Pressed(std::time::Instant),
    Forwarded(std::time::Instant),
    Done,
    Failed,
}

impl ClickTest {
    /// Per deadline turn. `due`: load-end + delay has passed.
    pub(crate) fn tick(&mut self, now: std::time::Instant, due: bool) -> ClickAction {
        let (next, action) = match *self {
            ClickTest::Waiting if due => (ClickTest::Requested(now), ClickAction::Push),
            ClickTest::Requested(at) | ClickTest::Pressed(at) | ClickTest::Forwarded(at)
                if now - at > TEST_TRANSITION_TIMEOUT =>
            {
                (
                    ClickTest::Failed,
                    ClickAction::Fail(match *self {
                        ClickTest::Requested(_) => "synthetic button down never forwarded",
                        ClickTest::Pressed(_) => "synthetic button up never forwarded",
                        _ => "click marker never received",
                    }),
                )
            },
            state => (state, ClickAction::None),
        };
        *self = next;
        action
    }

    /// A synthetic button event came back from the SDL queue through the input path.
    pub(crate) fn input(&mut self, input: ClickInput) -> ClickAction {
        let (next, action) = match (*self, input) {
            (ClickTest::Failed, _) => (ClickTest::Failed, ClickAction::None),
            (ClickTest::Requested(_) | ClickTest::Pressed(_), ClickInput::Down { ok: false })
            | (ClickTest::Requested(_) | ClickTest::Pressed(_), ClickInput::Up { ok: false }) => (
                ClickTest::Failed,
                ClickAction::Fail("welding refused a synthetic button event"),
            ),
            (ClickTest::Requested(at), ClickInput::Down { ok: true }) => {
                (ClickTest::Pressed(at), ClickAction::None)
            },
            (ClickTest::Pressed(at), ClickInput::Up { ok: true }) => {
                (ClickTest::Forwarded(at), ClickAction::None)
            },
            (ClickTest::Requested(_), ClickInput::Up { .. }) => (
                ClickTest::Failed,
                ClickAction::Fail("synthetic button up before button down"),
            ),
            _ => (
                ClickTest::Failed,
                ClickAction::Fail("unexpected synthetic button event"),
            ),
        };
        *self = next;
        action
    }

    /// The page logged the click marker.
    pub(crate) fn marker(&mut self) -> ClickAction {
        let (next, action) = match *self {
            ClickTest::Failed => (ClickTest::Failed, ClickAction::None),
            ClickTest::Forwarded(_) => (ClickTest::Done, ClickAction::Complete),
            ClickTest::Done => (
                ClickTest::Failed,
                ClickAction::Fail("click marker received more than once"),
            ),
            _ => (
                ClickTest::Failed,
                ClickAction::Fail("click marker before the click was forwarded"),
            ),
        };
        *self = next;
        action
    }

    /// Done or failed: `--roves-auto-close-ms` may close the window.
    pub(crate) fn finished(self) -> bool {
        matches!(self, ClickTest::Done | ClickTest::Failed)
    }
}

/// `--roves-test-key-ms` only: longest accepted delay after load-end (10 minutes), so a
/// mistyped value fails at parse time instead of leaving the run waiting indefinitely.
pub(crate) const TEST_KEY_MAX_DELAY_MS: u64 = 600_000;
/// `--roves-test-key-ms` only: SDL keyboard id stamped on the pushed key events, so the probe
/// attributes exactly those events to the test (never a physical keyboard).
pub(crate) const TEST_KEY_KEYBOARD_ID: u32 = 0x524F_5645;
/// `--roves-test-key-ms` only: the console message prefix the fixture's keydown/keyup
/// handlers log, followed by ` type=keydown` or ` type=keyup` and optional details. Accepted
/// only from a source on the game's loopback origin.
pub(crate) const TEST_KEY_MARKER: &str = "roves-test-key-received";

/// Parses the `--roves-test-key-ms` delay: a whole number of milliseconds, at most
/// `TEST_KEY_MAX_DELAY_MS`.
pub(crate) fn parse_test_key_delay(value: &str) -> Result<std::time::Duration, String> {
    parse_bounded_test_delay("test-key-ms", value)
}

/// Parses the delay of a bounded test hook (`--roves-<name>=<ms>`): a whole number of
/// milliseconds, at most `TEST_KEY_MAX_DELAY_MS`.
fn parse_bounded_test_delay(name: &str, value: &str) -> Result<std::time::Duration, String> {
    let ms = value
        .parse::<u64>()
        .map_err(|e| format!("--roves-{name}={value}: {e}"))?;
    if ms > TEST_KEY_MAX_DELAY_MS {
        return Err(format!(
            "--roves-{name}={value}: must be <= {TEST_KEY_MAX_DELAY_MS}"
        ));
    }
    Ok(std::time::Duration::from_millis(ms))
}

/// `--roves-test-key=<name>` (only with `--roves-test-key-ms`): which key the synthetic press
/// uses. Two fixed regression keys, not a general key-injection API; ArrowRight is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TestKey {
    /// SDL `Right`: an extended (`0xE0`) navigation key.
    #[default]
    ArrowRight,
    /// SDL `CapsLock`: a non-character key that needs the explicit VK_CAPITAL mapping.
    CapsLock,
}

impl TestKey {
    /// Parses the `--roves-test-key` value (the DOM `code` of the key, exact case).
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "ArrowRight" => Ok(TestKey::ArrowRight),
            "CapsLock" => Ok(TestKey::CapsLock),
            _ => Err(format!(
                "--roves-test-key={value}: must be ArrowRight or CapsLock"
            )),
        }
    }

    /// SDL's name of the pushed keycode/scancode, as logged by `test key requested key=`.
    pub(crate) fn sdl_name(self) -> &'static str {
        match self {
            TestKey::ArrowRight => "Right",
            TestKey::CapsLock => "CapsLock",
        }
    }
}

/// A fixture key marker: which DOM event type it reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyMarker {
    Down,
    Up,
    /// The marker word with any other or no `type=` (always a failure).
    Other,
}

/// Classifies a console message as a key marker (`None`: not one).
pub(crate) fn key_marker(message: &str) -> Option<KeyMarker> {
    let rest = message.strip_prefix(TEST_KEY_MARKER)?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let kind = |word: &str| {
        rest.strip_prefix(word)
            .is_some_and(|tail| tail.is_empty() || tail.starts_with(' '))
    };
    Some(if kind(" type=keydown") {
        KeyMarker::Down
    } else if kind(" type=keyup") {
        KeyMarker::Up
    } else {
        KeyMarker::Other
    })
}

/// A synthetic key event of the test after it went through the input path; `ok` is whether
/// welding accepted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyInput {
    Down { ok: bool },
    Up { ok: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyAction {
    None,
    /// Push the synthetic key down + up into the SDL queue.
    Push,
    Complete,
    Fail(&'static str),
}

/// `--roves-test-key-ms` (test only, game mode): one synthetic key press after the game's
/// load-end, observed in order: requested (pushed into the SDL queue), key down and up
/// forwarded through welding, then the page's keydown marker and keyup marker. Each pending
/// state carries the request instant; everything must happen within
/// `TEST_TRANSITION_TIMEOUT` of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyTest {
    /// Waiting for load-end + the requested delay.
    Waiting,
    Requested(std::time::Instant),
    Pressed(std::time::Instant),
    Forwarded(std::time::Instant),
    /// The page reported keydown; keyup pending.
    DomDown(std::time::Instant),
    Done,
    Failed,
}

impl KeyTest {
    /// Per deadline turn. `due`: load-end + delay has passed.
    pub(crate) fn tick(&mut self, now: std::time::Instant, due: bool) -> KeyAction {
        let (next, action) = match *self {
            KeyTest::Waiting if due => (KeyTest::Requested(now), KeyAction::Push),
            KeyTest::Requested(at)
            | KeyTest::Pressed(at)
            | KeyTest::Forwarded(at)
            | KeyTest::DomDown(at)
                if now - at > TEST_TRANSITION_TIMEOUT =>
            {
                (
                    KeyTest::Failed,
                    KeyAction::Fail(match *self {
                        KeyTest::Requested(_) => "synthetic key down never forwarded",
                        KeyTest::Pressed(_) => "synthetic key up never forwarded",
                        KeyTest::Forwarded(_) => "keydown marker never received",
                        _ => "keyup marker never received",
                    }),
                )
            },
            state => (state, KeyAction::None),
        };
        *self = next;
        action
    }

    /// A synthetic key event came back from the SDL queue through the input path.
    pub(crate) fn input(&mut self, input: KeyInput) -> KeyAction {
        let (next, action) = match (*self, input) {
            (KeyTest::Failed, _) => (KeyTest::Failed, KeyAction::None),
            (KeyTest::Requested(_) | KeyTest::Pressed(_), KeyInput::Down { ok: false })
            | (KeyTest::Requested(_) | KeyTest::Pressed(_), KeyInput::Up { ok: false }) => (
                KeyTest::Failed,
                KeyAction::Fail("welding refused a synthetic key event"),
            ),
            (KeyTest::Requested(at), KeyInput::Down { ok: true }) => {
                (KeyTest::Pressed(at), KeyAction::None)
            },
            (KeyTest::Pressed(at), KeyInput::Up { ok: true }) => {
                (KeyTest::Forwarded(at), KeyAction::None)
            },
            (KeyTest::Requested(_), KeyInput::Up { .. }) => (
                KeyTest::Failed,
                KeyAction::Fail("synthetic key up before key down"),
            ),
            _ => (
                KeyTest::Failed,
                KeyAction::Fail("unexpected synthetic key event"),
            ),
        };
        *self = next;
        action
    }

    /// The page logged a key marker.
    pub(crate) fn marker(&mut self, marker: KeyMarker) -> KeyAction {
        let (next, action) = match (*self, marker) {
            (KeyTest::Failed, _) => (KeyTest::Failed, KeyAction::None),
            (KeyTest::Forwarded(at), KeyMarker::Down) => (KeyTest::DomDown(at), KeyAction::None),
            (KeyTest::DomDown(_), KeyMarker::Up) => (KeyTest::Done, KeyAction::Complete),
            (_, KeyMarker::Other) => (
                KeyTest::Failed,
                KeyAction::Fail("key marker with an unexpected event type"),
            ),
            (KeyTest::DomDown(_) | KeyTest::Done, KeyMarker::Down)
            | (KeyTest::Done, KeyMarker::Up) => (
                KeyTest::Failed,
                KeyAction::Fail("key marker received more than once"),
            ),
            (KeyTest::Forwarded(_), KeyMarker::Up) => (
                KeyTest::Failed,
                KeyAction::Fail("keyup marker before the keydown marker"),
            ),
            _ => (
                KeyTest::Failed,
                KeyAction::Fail("key marker before the key was forwarded"),
            ),
        };
        *self = next;
        action
    }

    /// Done or failed: `--roves-auto-close-ms` may close the window.
    pub(crate) fn finished(self) -> bool {
        matches!(self, KeyTest::Done | KeyTest::Failed)
    }
}

/// `--roves-test-wheel-ms` only: SDL mouse id stamped on the pushed wheel event, so the probe
/// attributes exactly that event to the test (never a physical mouse).
pub(crate) const TEST_WHEEL_MOUSE_ID: u32 = 0x524F_5645;
/// `--roves-test-wheel-ms` only: the pushed SDL wheel amount `(x, y)` in lines, normal
/// direction: one notch away from the user (scroll up).
pub(crate) const TEST_WHEEL_LINES: (f32, f32) = (0.0, 1.0);
/// `--roves-test-wheel-ms` only: the `(delta_x, delta_y)` the current conversion
/// (`wheel_cef_delta`) must hand to welding/CEF for `TEST_WHEEL_LINES`.
pub(crate) const TEST_WHEEL_CEF_DELTA: (i32, i32) = (0, 120);
/// `--roves-test-wheel-ms` only: the console message prefix the fixture's wheel listener
/// logs, followed by space-separated `name=value` fields. Accepted only from a source on the
/// game's loopback origin.
pub(crate) const TEST_WHEEL_MARKER: &str = "roves-test-wheel-received";

/// Parses the `--roves-test-wheel-ms` delay: a whole number of milliseconds, at most
/// `TEST_KEY_MAX_DELAY_MS`.
pub(crate) fn parse_test_wheel_delay(value: &str) -> Result<std::time::Duration, String> {
    parse_bounded_test_delay("test-wheel-ms", value)
}

/// A fixture wheel marker, checked against the pushed event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WheelMarker {
    /// A trusted DOM `wheel` with the expected direction: `deltaX` 0 and `deltaY` < 0 (Blink
    /// negates the CEF delta, so a positive CEF `delta_y` scrolls up). The magnitude and
    /// `deltaMode` are reported, not checked.
    Expected { delta_y: f64, delta_mode: u32 },
    /// The marker word with a wrong or missing field (always a failure).
    Unexpected(&'static str),
}

/// Classifies a console message as a wheel marker (`None`: not one). Fields:
/// `type=wheel deltaX=<n> deltaY=<n> deltaMode=<0|1|2> trusted=true`, others ignored.
pub(crate) fn wheel_marker(message: &str) -> Option<WheelMarker> {
    let rest = message.strip_prefix(TEST_WHEEL_MARKER)?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let field = |name: &str| {
        rest.split(' ')
            .filter_map(|word| word.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    let number = |name: &str| {
        field(name)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    Some(if field("type") != Some("wheel") {
        WheelMarker::Unexpected("wheel marker with an unexpected event type")
    } else if field("trusted") != Some("true") {
        WheelMarker::Unexpected("wheel marker for an untrusted event")
    } else if number("deltaX") != Some(0.0) {
        WheelMarker::Unexpected("wheel marker with a horizontal delta")
    } else {
        let delta_mode = field("deltaMode").and_then(|v| v.parse::<u32>().ok());
        match (number("deltaY"), delta_mode) {
            (Some(delta_y), Some(delta_mode @ 0..=2)) if delta_y < 0.0 => WheelMarker::Expected {
                delta_y,
                delta_mode,
            },
            (Some(_), Some(0..=2)) => {
                WheelMarker::Unexpected("wheel marker with the wrong vertical direction")
            },
            _ => WheelMarker::Unexpected("wheel marker without a valid deltaY/deltaMode"),
        }
    })
}

/// The synthetic wheel event of the test after it went through the input path: `ok` is
/// whether welding accepted it, `delta` what the probe handed to welding (`None`: no wheel
/// call was made).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WheelInput {
    pub(crate) ok: bool,
    pub(crate) delta: Option<(i32, i32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelAction {
    None,
    /// Push the synthetic wheel event into the SDL queue.
    Push,
    Complete,
    Fail(&'static str),
}

/// `--roves-test-wheel-ms` (test only, game mode): one synthetic wheel notch after the game's
/// load-end, observed in order: requested (pushed into the SDL queue), forwarded through
/// welding with `TEST_WHEEL_CEF_DELTA`, then exactly one expected page marker. Each pending
/// state carries the request instant; everything must happen within
/// `TEST_TRANSITION_TIMEOUT` of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelTest {
    /// Waiting for load-end + the requested delay.
    Waiting,
    Requested(std::time::Instant),
    Forwarded(std::time::Instant),
    Done,
    Failed,
}

impl WheelTest {
    /// Per deadline turn. `due`: load-end + delay has passed.
    pub(crate) fn tick(&mut self, now: std::time::Instant, due: bool) -> WheelAction {
        let (next, action) = match *self {
            WheelTest::Waiting if due => (WheelTest::Requested(now), WheelAction::Push),
            WheelTest::Requested(at) | WheelTest::Forwarded(at)
                if now - at > TEST_TRANSITION_TIMEOUT =>
            {
                (
                    WheelTest::Failed,
                    WheelAction::Fail(match *self {
                        WheelTest::Requested(_) => "synthetic wheel event never forwarded",
                        _ => "wheel marker never received",
                    }),
                )
            },
            state => (state, WheelAction::None),
        };
        *self = next;
        action
    }

    /// The synthetic wheel event came back from the SDL queue through the input path.
    pub(crate) fn input(&mut self, input: WheelInput) -> WheelAction {
        let (next, action) = match (*self, input) {
            (WheelTest::Failed, _) => (WheelTest::Failed, WheelAction::None),
            (WheelTest::Requested(_), WheelInput { ok: false, .. }) => (
                WheelTest::Failed,
                WheelAction::Fail("welding refused the synthetic wheel event"),
            ),
            (WheelTest::Requested(at), WheelInput { ok: true, delta })
                if delta == Some(TEST_WHEEL_CEF_DELTA) =>
            {
                (WheelTest::Forwarded(at), WheelAction::None)
            },
            (WheelTest::Requested(_), _) => (
                WheelTest::Failed,
                WheelAction::Fail("synthetic wheel event forwarded with an unexpected CEF delta"),
            ),
            _ => (
                WheelTest::Failed,
                WheelAction::Fail("unexpected synthetic wheel event"),
            ),
        };
        *self = next;
        action
    }

    /// The page logged a wheel marker.
    pub(crate) fn marker(&mut self, marker: WheelMarker) -> WheelAction {
        let (next, action) = match (*self, marker) {
            (WheelTest::Failed, _) => (WheelTest::Failed, WheelAction::None),
            (_, WheelMarker::Unexpected(why)) => (WheelTest::Failed, WheelAction::Fail(why)),
            (WheelTest::Forwarded(_), WheelMarker::Expected { .. }) => {
                (WheelTest::Done, WheelAction::Complete)
            },
            (WheelTest::Done, _) => (
                WheelTest::Failed,
                WheelAction::Fail("wheel marker received more than once"),
            ),
            _ => (
                WheelTest::Failed,
                WheelAction::Fail("wheel marker before the wheel event was forwarded"),
            ),
        };
        *self = next;
        action
    }

    /// Done or failed: `--roves-auto-close-ms` may close the window.
    pub(crate) fn finished(self) -> bool {
        matches!(self, WheelTest::Done | WheelTest::Failed)
    }
}

/// One startup milestone of the browser process (every mode; `LoopbackReady` in game mode
/// only). Logged as `startup milestone=<name> elapsed_ms=<n>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Milestone {
    OptionsLogReady,
    LoopbackReady,
    CefInitialized,
    SdlReady,
    WindowSurfaceProducerReady,
    LoadStart,
    LoadEnd,
    FirstImport,
    FirstPresent,
    /// Game mode: the startup screen gave way to the game (`StartupGate::Released`).
    StartupReleased,
}

impl Milestone {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Milestone::OptionsLogReady => "options-log-ready",
            Milestone::LoopbackReady => "loopback-ready",
            Milestone::CefInitialized => "cef-initialized",
            Milestone::SdlReady => "sdl-ready",
            Milestone::WindowSurfaceProducerReady => "window-surface-producer-ready",
            Milestone::LoadStart => "load-start",
            Milestone::LoadEnd => "load-end",
            Milestone::FirstImport => "first-import",
            Milestone::FirstPresent => "first-present",
            Milestone::StartupReleased => "startup-released",
        }
    }
}

/// Packaged launch: the game build directory next to the executable, used when the probe
/// starts without `--roves-game-dir` and `<exe dir>\game\index.html` exists.
pub(crate) const PLAYER_GAME_DIR: &str = "game";

/// Packaged launch, persistent profile (README "Saves and the player profile"): files of the
/// profile root. The lock is held for the whole browser-process run (one instance per
/// profile); the origin port file keeps the loopback origin, and so the origin-keyed page
/// storage, identical across launches; CEF's root cache (with welding's `game-profile`) is
/// the `cef` subdirectory.
pub(crate) const PLAYER_LOCK_FILE: &str = "player.lock";
pub(crate) const PLAYER_ORIGIN_PORT_FILE: &str = "origin-port";
pub(crate) const PLAYER_CEF_DIR: &str = "cef";

/// Name of the default persistent profile directory (under `%LOCALAPPDATA%\Roves\Player`):
/// the executable stem (ASCII letters, digits, `-` and `_` kept, other characters replaced by
/// `_`, at most 32, `game` when empty) and the 64-bit FNV-1a of the lower-cased canonical
/// install directory. Every launch of one install finds the same profile; two packages
/// that both ship a `play.exe` never share storage. Moving or renaming the install directory
/// starts a new profile (documented limitation).
pub(crate) fn player_profile_name(exe_stem: &str, install_dir: &str) -> String {
    let mut stem: String = exe_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    if stem.is_empty() {
        stem.push_str("game");
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in install_dir.to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{stem}-{hash:016x}")
}

/// The stored origin port: decimal 1..=65535, surrounding whitespace allowed. `None` for any
/// other content (a damaged file is reported, never silently replaced by a new origin).
pub(crate) fn parse_origin_port(text: &str) -> Option<u16> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u16>().ok().filter(|port| *port != 0)
}

/// Port of a loopback origin `http://127.0.0.1:<port>` (no trailing slash).
pub(crate) fn origin_port(origin: &str) -> Option<u16> {
    origin
        .strip_prefix("http://127.0.0.1:")
        .and_then(parse_origin_port)
}

/// Why the startup screen did not give way to the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupFailure {
    /// The main-frame load-end had a non-accepted status or left the loopback origin.
    LoadRejected,
    /// CEF reported a main-frame load error.
    LoadError,
    /// The renderer process ended (before or after the release).
    RendererTerminated,
    /// No accepted main-frame load-end within `--roves-timeout-ms`.
    LoadTimeout,
    /// Loaded, but no CEF frame painted after the load arrived within `--roves-timeout-ms`
    /// (for example accelerated off-screen rendering unavailable on the adapter).
    NoFreshFrame,
}

impl StartupFailure {
    /// Log token (`startup failed reason=<token>`).
    pub(crate) fn token(self) -> &'static str {
        match self {
            StartupFailure::LoadRejected => "load-rejected",
            StartupFailure::LoadError => "load-error",
            StartupFailure::RendererTerminated => "renderer-terminated",
            StartupFailure::LoadTimeout => "load-timeout",
            StartupFailure::NoFreshFrame => "no-fresh-frame",
        }
    }

    /// The sentence shown to the player in the error dialog.
    pub(crate) fn message(self) -> &'static str {
        match self {
            StartupFailure::LoadRejected | StartupFailure::LoadError => {
                "The game files could not be loaded. Check that the \"game\" folder next to \
                 the program is complete, then start the game again."
            },
            StartupFailure::RendererTerminated => {
                "The game stopped unexpectedly. Start the game again; if it keeps happening, \
                 update the graphics driver."
            },
            StartupFailure::LoadTimeout => "The game took too long to load. Start the game again.",
            StartupFailure::NoFreshFrame => {
                "The game loaded but could not be displayed: this graphics adapter or driver \
                 does not support the accelerated rendering the game needs. Update the \
                 graphics driver or use another GPU."
            },
        }
    }
}

/// Game-mode startup screen. The branded screen stays up until the accepted main-frame
/// load-end has been handled AND a CEF frame painted after it has been imported, as
/// Chromium keeps the previous surface and holds input until the first frame of the new
/// content (`RenderWidgetHostImpl::OnRenderFrameMetadataChangedAfterActivation`). A frame
/// imported in the same loop turn as the load-end may predate it, so the gate records the
/// newest generation at the end of that turn and requires a strictly newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupGate {
    /// Waiting for the accepted main-frame load-end.
    Loading,
    /// Load-end handled in the current turn; the baseline is taken at its end.
    LoadEnded,
    /// Waiting for a frame newer than this generation (`None`: none imported yet).
    WaitingFrame(Option<u64>),
    Released,
    Failed(StartupFailure),
}

impl StartupGate {
    /// The accepted main-frame load-end. `true` when it moved the gate.
    pub(crate) fn load_end(&mut self) -> bool {
        if *self != StartupGate::Loading {
            return false;
        }
        *self = StartupGate::LoadEnded;
        true
    }

    /// A producer frame of welding paint `generation` was imported. `true` on release.
    pub(crate) fn imported(&mut self, generation: u64) -> bool {
        match *self {
            StartupGate::WaitingFrame(baseline) if baseline.is_none_or(|b| generation > b) => {
                *self = StartupGate::Released;
                true
            },
            _ => false,
        }
    }

    /// End of a loop turn; `newest` is the generation of the newest imported frame so far.
    pub(crate) fn end_turn(&mut self, newest: Option<u64>) {
        if *self == StartupGate::LoadEnded {
            *self = StartupGate::WaitingFrame(newest);
        }
    }

    /// Records a failure. Only the first one counts; a renderer that ends after the release
    /// still replaces the game with the failure screen. `true` when it moved the gate.
    pub(crate) fn fail(&mut self, failure: StartupFailure) -> bool {
        let applies = match *self {
            StartupGate::Failed(_) => false,
            StartupGate::Released => failure == StartupFailure::RendererTerminated,
            _ => true,
        };
        if applies {
            *self = StartupGate::Failed(failure);
        }
        applies
    }

    /// The failure a passed startup deadline means in the current state (`None`: released or
    /// already failed).
    pub(crate) fn timed_out(self) -> Option<StartupFailure> {
        match self {
            StartupGate::Loading => Some(StartupFailure::LoadTimeout),
            StartupGate::LoadEnded | StartupGate::WaitingFrame(_) => {
                Some(StartupFailure::NoFreshFrame)
            },
            StartupGate::Released | StartupGate::Failed(_) => None,
        }
    }

    pub(crate) fn released(self) -> bool {
        self == StartupGate::Released
    }

    pub(crate) fn failure(self) -> Option<StartupFailure> {
        match self {
            StartupGate::Failed(failure) => Some(failure),
            _ => None,
        }
    }
}

/// Startup screen: top-left corners of a column of `items` (`(width, height)` in physical
/// pixels, top to bottom, `None` = not drawn) centred in a `w` x `h` target, `gap` pixels
/// apart. While the column does not fit, its leading item is dropped (logo first, then the
/// wordmark), so a small window keeps the loading bar; nothing is scaled or drawn outside
/// the target. Recomputed every frame, so resizes and DPI changes re-centre it.
pub(crate) fn splash_layout<const N: usize>(
    w: u32,
    h: u32,
    items: [Option<(u32, u32)>; N],
    gap: u32,
) -> [Option<(u32, u32)>; N] {
    let mut out = [None; N];
    for first in 0..N {
        let present = || items[first..].iter().flatten();
        let count = present().count() as u32;
        if count == 0 {
            break;
        }
        let width = present().map(|&(iw, _)| iw).max().unwrap_or(0);
        let height = present().map(|&(_, ih)| ih).sum::<u32>() + gap * (count - 1);
        if width <= w && height <= h {
            let mut y = (h - height) / 2;
            for (slot, item) in out[first..].iter_mut().zip(&items[first..]) {
                if let Some((iw, ih)) = *item {
                    *slot = Some(((w - iw) / 2, y));
                    y += ih + gap;
                }
            }
            break;
        }
    }
    out
}

/// One sweep of the indeterminate loading bar, in milliseconds.
pub(crate) const SPLASH_BAR_PERIOD_MS: u128 = 1400;

/// Indeterminate loading bar: `(offset, width)` of the moving segment inside a `track`
/// pixels wide track, `elapsed_ms` after the startup screen appeared, or `None` while it is
/// outside the track. A segment a third of the track wide sweeps (eased in and out) from
/// fully left of the track to fully right of it once per period, clipped to the track. It
/// signals activity, not progress: no fraction of the load is known or claimed.
pub(crate) fn splash_indicator(track: u32, elapsed_ms: u128) -> Option<(u32, u32)> {
    let segment = f64::from((track / 3).max(1));
    let t = (elapsed_ms % SPLASH_BAR_PERIOD_MS) as f64 / SPLASH_BAR_PERIOD_MS as f64;
    let eased = t * t * (3.0 - 2.0 * t);
    let start = -segment + (f64::from(track) + segment) * eased;
    let (from, to) = (
        start.round().max(0.0),
        (start + segment).round().min(f64::from(track)),
    );
    (to > from).then_some((from as u32, (to - from) as u32))
}

/// Startup timeline: milliseconds since entry to the main probe run (after CEF subprocess
/// dispatch), each milestone reported at most once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Startup {
    entered: std::time::Instant,
    reported: u16,
}

impl Startup {
    pub(crate) fn new(entered: std::time::Instant) -> Self {
        Startup {
            entered,
            reported: 0,
        }
    }

    /// `Some(elapsed_ms)` the first time `milestone` is reached, `None` afterwards.
    pub(crate) fn reach(&mut self, milestone: Milestone, now: std::time::Instant) -> Option<u128> {
        let bit = 1u16 << milestone as u16;
        if self.reported & bit != 0 {
            return None;
        }
        self.reported |= bit;
        Some(now.saturating_duration_since(self.entered).as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Logo, wordmark and bar as in `src/splash.rs` (`LOGO_PX`, `WORDMARK_SIZE`, `BAR_SIZE`,
    // `SPLASH_GAP`).
    const LOGO: Option<(u32, u32)> = Some((256, 256));
    const WORDMARK: Option<(u32, u32)> = Some((240, 48));
    const BAR: Option<(u32, u32)> = Some((240, 6));

    #[test]
    fn splash_layout_centres_the_column() {
        assert_eq!(
            splash_layout(800, 600, [LOGO, WORDMARK, BAR], 24),
            [Some((272, 121)), Some((280, 401)), Some((280, 473))]
        );
        // HiDPI-sized target: same physical sizes, re-centred.
        assert_eq!(
            splash_layout(1600, 1200, [LOGO, WORDMARK, BAR], 24),
            [Some((672, 421)), Some((680, 701)), Some((680, 773))]
        );
        // Logo unavailable (branding decode failed): wordmark and bar only.
        assert_eq!(
            splash_layout(800, 600, [None, WORDMARK, BAR], 24),
            [None, Some((280, 261)), Some((280, 333))]
        );
        // Failure screen: no bar.
        assert_eq!(
            splash_layout(800, 600, [LOGO, WORDMARK, None], 24),
            [Some((272, 136)), Some((280, 416)), None]
        );
    }

    #[test]
    fn splash_layout_drops_leading_items_that_do_not_fit() {
        // Too short for the logo: wordmark and bar.
        assert_eq!(
            splash_layout(300, 200, [LOGO, WORDMARK, BAR], 24),
            [None, Some((30, 61)), Some((30, 133))]
        );
        // Only the bar fits.
        assert_eq!(
            splash_layout(250, 40, [LOGO, WORDMARK, BAR], 24),
            [None, None, Some((5, 17))]
        );
        // Too narrow for everything; empty and degenerate targets.
        assert_eq!(
            splash_layout(200, 600, [LOGO, WORDMARK, BAR], 24),
            [None; 3]
        );
        assert_eq!(splash_layout(0, 0, [LOGO, WORDMARK, BAR], 24), [None; 3]);
        assert_eq!(splash_layout(800, 600, [None, None, None], 24), [None; 3]);
        // Every placed item lies inside the target.
        for (w, h) in [
            (256, 358),
            (257, 359),
            (241, 79),
            (240, 6),
            (1, 1),
            (4000, 9),
        ] {
            let items = [LOGO, WORDMARK, BAR];
            for (placed, item) in splash_layout(w, h, items, 24).iter().zip(items) {
                if let (Some((x, y)), Some((iw, ih))) = (placed, item) {
                    assert!(x + iw <= w && y + ih <= h, "{w}x{h}: {x},{y} {iw}x{ih}");
                }
            }
        }
    }

    #[test]
    fn splash_indicator_sweeps_inside_the_track() {
        // Starts and ends outside the track; mid-period it is fully inside and centred.
        assert_eq!(splash_indicator(240, 0), None);
        assert_eq!(
            splash_indicator(240, SPLASH_BAR_PERIOD_MS / 2),
            Some((80, 80))
        );
        // Periodic.
        assert_eq!(
            splash_indicator(240, 5 * SPLASH_BAR_PERIOD_MS + 300),
            splash_indicator(240, 300)
        );
        let mut entered_clipped = false;
        let mut last_end = 0;
        for ms in 0..SPLASH_BAR_PERIOD_MS {
            if let Some((x, w)) = splash_indicator(240, ms) {
                assert!(w > 0 && w <= 80 && x + w <= 240, "{ms} ms: {x}+{w}");
                entered_clipped |= x == 0 && w < 80;
                // Left to right within one period.
                assert!(x + w >= last_end, "{ms} ms moved back");
                last_end = x + w;
            }
        }
        assert!(entered_clipped);
        // Degenerate tracks never escape them.
        for track in [0, 1, 2] {
            for ms in (0..SPLASH_BAR_PERIOD_MS).step_by(10) {
                if let Some((x, w)) = splash_indicator(track, ms) {
                    assert!(x + w <= track);
                }
            }
        }
    }

    #[test]
    fn origin_check_is_exact() {
        let origin = "http://127.0.0.1:1234";
        assert!(on_origin(origin, "http://127.0.0.1:1234/"));
        assert!(on_origin(origin, "http://127.0.0.1:1234/#/route"));
        assert!(on_origin(
            origin,
            "http://127.0.0.1:1234/game/index.html?x=1"
        ));
        assert!(on_origin(origin, "http://127.0.0.1:1234"));
        assert!(!on_origin(origin, "http://127.0.0.1:12345/"));
        assert!(!on_origin(origin, "http://127.0.0.1:1234@evil.example/"));
        assert!(!on_origin(origin, "https://example.com/"));
        assert!(!on_origin(origin, "about:blank"));
        assert!(!on_origin(origin, "data:text/html,x"));
    }

    #[test]
    fn load_statuses() {
        assert!(load_status_ok(200));
        assert!(load_status_ok(304));
        assert!(!load_status_ok(404));
        assert!(!load_status_ok(0));
    }

    #[test]
    fn user_input_waits_for_the_game_load_end() {
        // Before load-end (welding has no browser host): only window events go through.
        assert!(!forward_input(InputEventKind::User, false));
        assert!(forward_input(InputEventKind::Window, false));
        // After load-end everything is handled, so welding refusals are not hidden.
        assert!(forward_input(InputEventKind::User, true));
        assert!(forward_input(InputEventKind::Window, true));
    }

    // SDL scancodes (USB HID usages) used below.
    const KEY_A: u32 = 0x04;
    const RIGHT: u32 = 0x4F;
    const KP_ENTER: u32 = 0x58;
    const KP_6: u32 = 0x5E;
    const RCTRL: u32 = 0xE4;

    /// Chromium's `GetScanCodeFromLParam` (`ui/events/win/events_win_utils.cc`), to check that
    /// a real key message and the probe's value agree.
    fn chromium_scan_code_from_lparam(l_param: u32) -> u32 {
        let mut scan_code = (l_param >> 16) & 0x00FF;
        if l_param & (1 << 24) != 0 {
            scan_code |= 0xE000;
        }
        scan_code
    }

    #[test]
    fn real_keys_use_sdl_raw_scan_code_unchanged() {
        // SDL's Windows `raw` = what Chromium derives from the same WM_KEYDOWN lParam.
        for (l_param, usage, raw) in [
            (0x014D_0001u32, RIGHT, 0xE04Du16), // ArrowRight: extended E0 4D
            (0x004D_0001, KP_6, 0x004D),        // Numpad6 (NumLock on or off)
            (0x001E_0001, KEY_A, 0x001E),       // KeyA
            (0x011C_0001, KP_ENTER, 0xE01C),    // NumpadEnter
            (0x011D_0001, RCTRL, 0xE01D),       // ControlRight
        ] {
            let native = native_key_code(raw, Some(usage), 0);
            assert_eq!(native as u32, chromium_scan_code_from_lparam(l_param));
        }
        // `mapped` is ignored when SDL had a scan code (VK_RIGHT maps to 0x4D).
        assert_eq!(native_key_code(0xE04D, Some(RIGHT), 0x4D), 0xE04D);
        assert_eq!(native_key_code(0x004D, Some(KP_6), 0x4D), 0x004D);
    }

    #[test]
    fn native_key_code_is_never_an_lparam() {
        // The old value (lParam 0x004D0001 / 0xC04D0001) matched no Chromium table entry.
        for raw in [0u16, 0x1E, 0xE04D] {
            for mapped in [0u32, 0x4D, 0xE04D] {
                let native = native_key_code(raw, Some(RIGHT), mapped) as u32;
                assert_eq!(native & 0xFFFF_0000, 0, "{native:#x}");
            }
        }
    }

    #[test]
    fn missing_raw_restores_the_extended_prefix_from_the_sdl_scancode() {
        // MapVirtualKeyW(VK_RIGHT, MAPVK_VK_TO_VSC_EX) = 0x4D (the CI runner and a local
        // P/Invoke): ArrowRight must still be E04D, the keypad key keeps 0x4D.
        assert_eq!(native_key_code(0, Some(RIGHT), 0x4D), 0xE04D);
        assert_eq!(native_key_code(0, Some(KP_6), 0x4D), 0x004D);
        // Already prefixed by Windows: unchanged.
        assert_eq!(native_key_code(0, Some(RIGHT), 0xE04D), 0xE04D);
        assert_eq!(native_key_code(0, Some(RCTRL), 0xE01D), 0xE01D);
        // VK_RETURN maps to 0x1C: Enter vs NumpadEnter by the SDL scancode.
        assert_eq!(native_key_code(0, Some(0x28), 0x1C), 0x001C);
        assert_eq!(native_key_code(0, Some(KP_ENTER), 0x1C), 0xE01C);
        // Ordinary keys untouched; nothing known: 0 (DomCode NONE, not a wrong key).
        assert_eq!(native_key_code(0, Some(KEY_A), 0x1E), 0x001E);
        assert_eq!(native_key_code(0, None, 0x1E), 0x001E);
        assert_eq!(native_key_code(0, Some(RIGHT), 0), 0);
        assert_eq!(native_key_code(0, None, 0), 0);
    }

    #[test]
    fn extended_usages_match_chromium_windows_column() {
        // Usages whose `dom_code_data.inc` Windows scan code is 0xE0xx (PrintScreen excluded).
        for usage in [
            0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52, 0x53, 0x54, 0x58, 0x65,
            0xE3, 0xE4, 0xE6, 0xE7,
        ] {
            assert!(extended_usage(usage), "{usage:#x}");
        }
        // Keypad digits/operators, Enter, letters, left modifiers: not extended.
        for usage in [
            0x04, 0x28, 0x46, 0x48, 0x55, 0x56, 0x57, 0x59, 0x5E, 0x62, 0x63, 0xE0, 0xE1, 0xE2,
        ] {
            assert!(!extended_usage(usage), "{usage:#x}");
        }
    }

    #[test]
    fn pause_uses_the_windows_scan_code() {
        // SDL rewrites Pause to 0xE046 (WM_KEYDOWN) or 0xE145 (raw input); Chromium: 0x0045.
        assert_eq!(native_key_code(0xE046, Some(0x48), 0), 0x0045);
        assert_eq!(native_key_code(0xE145, Some(0x48), 0), 0x0045);
        assert_eq!(native_key_code(0, Some(0x48), 0), 0x0045);
        // NumLock keeps its extended code.
        assert_eq!(native_key_code(0xE045, Some(0x53), 0), 0xE045);
        assert_eq!(native_key_code(0, Some(0x53), 0x45), 0xE045);
    }

    /// Every mapped usage, spelled out (no ranges, so the implementation's arithmetic is not
    /// mirrored). Each row was derived independently from the pinned sources: SDL 3.4.16
    /// `SDL_scancode.h` usage -> HID usage (page 0x07 below 0x100; Sleep 0x010082 and the
    /// consumer page 0x0C for the media/AC keys) -> Chromium f2700724f59f
    /// `dom/dom_code_data.inc` DomCode -> `dom_us_layout_data.h` `kDomCodeToKeyboardCodeMap`
    /// VKEY -> its value in `keyboard_codes_posix.h` (the Win32 VK numbering).
    const NON_CHARACTER_VIRTUAL_KEYS: &[(u32, u32, &str)] = &[
        (0x39, 0x14, "CAPSLOCK"),
        (0x3A, 0x70, "F1"),
        (0x3B, 0x71, "F2"),
        (0x3C, 0x72, "F3"),
        (0x3D, 0x73, "F4"),
        (0x3E, 0x74, "F5"),
        (0x3F, 0x75, "F6"),
        (0x40, 0x76, "F7"),
        (0x41, 0x77, "F8"),
        (0x42, 0x78, "F9"),
        (0x43, 0x79, "F10"),
        (0x44, 0x7A, "F11"),
        (0x45, 0x7B, "F12"),
        (0x46, 0x2C, "PRINTSCREEN"),
        (0x47, 0x91, "SCROLLLOCK"),
        (0x48, 0x13, "PAUSE"),
        (0x49, 0x2D, "INSERT"),
        (0x4A, 0x24, "HOME"),
        (0x4B, 0x21, "PAGEUP"),
        (0x4D, 0x23, "END"),
        (0x4E, 0x22, "PAGEDOWN"),
        (0x4F, 0x27, "RIGHT"),
        (0x50, 0x25, "LEFT"),
        (0x51, 0x28, "DOWN"),
        (0x52, 0x26, "UP"),
        (0x53, 0x90, "NUMLOCKCLEAR"),
        (0x54, 0x6F, "KP_DIVIDE"),
        (0x55, 0x6A, "KP_MULTIPLY"),
        (0x56, 0x6D, "KP_MINUS"),
        (0x57, 0x6B, "KP_PLUS"),
        (0x58, 0x0D, "KP_ENTER"),
        (0x59, 0x61, "KP_1"),
        (0x5A, 0x62, "KP_2"),
        (0x5B, 0x63, "KP_3"),
        (0x5C, 0x64, "KP_4"),
        (0x5D, 0x65, "KP_5"),
        (0x5E, 0x66, "KP_6"),
        (0x5F, 0x67, "KP_7"),
        (0x60, 0x68, "KP_8"),
        (0x61, 0x69, "KP_9"),
        (0x62, 0x60, "KP_0"),
        (0x63, 0x6E, "KP_PERIOD"),
        (0x65, 0x5D, "APPLICATION"),
        (0x68, 0x7C, "F13"),
        (0x69, 0x7D, "F14"),
        (0x6A, 0x7E, "F15"),
        (0x6B, 0x7F, "F16"),
        (0x6C, 0x80, "F17"),
        (0x6D, 0x81, "F18"),
        (0x6E, 0x82, "F19"),
        (0x6F, 0x83, "F20"),
        (0x70, 0x84, "F21"),
        (0x71, 0x85, "F22"),
        (0x72, 0x86, "F23"),
        (0x73, 0x87, "F24"),
        (0x7F, 0xAD, "MUTE"),
        (0x80, 0xAF, "VOLUMEUP"),
        (0x81, 0xAE, "VOLUMEDOWN"),
        (0x8A, 0x1C, "INTERNATIONAL4"), // Convert -> VK_CONVERT
        (0x8B, 0x1D, "INTERNATIONAL5"), // NonConvert -> VK_NONCONVERT
        (0xE0, 0xA2, "LCTRL"),
        (0xE1, 0xA0, "LSHIFT"),
        (0xE2, 0xA4, "LALT"),
        (0xE3, 0x5B, "LGUI"),
        (0xE4, 0xA3, "RCTRL"),
        (0xE5, 0xA1, "RSHIFT"),
        (0xE6, 0xA5, "RALT"),
        (0xE7, 0x5C, "RGUI"),
        (0x102, 0x5F, "SLEEP"),
        (0x10B, 0xB0, "MEDIA_NEXT_TRACK"),
        (0x10C, 0xB1, "MEDIA_PREVIOUS_TRACK"),
        (0x10D, 0xB2, "MEDIA_STOP"),
        (0x10F, 0xB3, "MEDIA_PLAY_PAUSE"),
        (0x110, 0xB5, "MEDIA_SELECT"),
        (0x118, 0xAA, "AC_SEARCH"),
        (0x119, 0xAC, "AC_HOME"),
        (0x11A, 0xA6, "AC_BACK"),
        (0x11B, 0xA7, "AC_FORWARD"),
        (0x11C, 0xA9, "AC_STOP"),
        (0x11D, 0xA8, "AC_REFRESH"),
        (0x11E, 0xAB, "AC_BOOKMARKS"),
    ];

    #[test]
    fn non_character_keys_get_their_virtual_key() {
        assert!(
            NON_CHARACTER_VIRTUAL_KEYS
                .windows(2)
                .all(|w| w[0].0 < w[1].0),
            "table rows must be unique and sorted"
        );
        // Every SDL scancode (`SDL_SCANCODE_COUNT` = 512): the row's VK, or 0 when unmapped
        // (Help 0x75, KanaMode/Lang1/Lang2, IntlRo/IntlYen, NumpadComma, ...).
        for usage in 0..0x200 {
            let expected = NON_CHARACTER_VIRTUAL_KEYS
                .iter()
                .find(|row| row.0 == usage)
                .map_or(0, |row| row.1);
            assert_eq!(
                non_character_virtual_key(SDLK_SCANCODE_MASK | usage),
                expected,
                "usage {usage:#x}"
            );
        }
        // Keycodes without SDLK_SCANCODE_MASK are characters (`VkKeyScanW` path): 0 here, even
        // when the value equals a mapped usage ('9' is CapsLock's 0x39, 'a' is Numpad9's 0x61).
        for keycode in [0x00, 0x0D, 0x1B, 0x39, 0x61, 0x7F, 0xE1, 0x102] {
            assert_eq!(
                non_character_virtual_key(keycode),
                0,
                "keycode {keycode:#x}"
            );
        }
    }

    #[test]
    fn japanese_conversion_keys_get_their_virtual_key() {
        // SDL 3.4.16 `scancodes_windows.h`: scan 0x79/0x7B -> SDL_SCANCODE_INTERNATIONAL4/5.
        // Chromium f2700724f59f: the same Windows scan codes are Convert/NonConvert
        // (`dom_code_data.inc`), mapped to VKEY_CONVERT/VKEY_NONCONVERT
        // (`dom_us_layout_data.h`), which `platform_key_map_win.cc` turns into
        // DomKey::CONVERT/NON_CONVERT for every layout.
        let key = |usage: u32| non_character_virtual_key(SDLK_SCANCODE_MASK | usage);
        assert_eq!(key(0x8A), 0x1C); // Convert -> VK_CONVERT
        assert_eq!(key(0x8B), 0x1D); // NonConvert -> VK_NONCONVERT
        // Layout-dependent neighbours stay unmapped: KanaMode/Lang1/Lang2 (DomKey depends on
        // the Korean/Japanese layout), IntlRo/IntlYen/NumpadComma (printable VKs).
        for usage in [0x85, 0x87, 0x88, 0x89, 0x90, 0x91] {
            assert_eq!(key(usage), 0, "usage {usage:#x}");
        }
    }

    #[test]
    fn sided_modifiers_fold_to_generic() {
        assert_eq!(fold_sided_modifier(0xA1), 0x10);
        assert_eq!(fold_sided_modifier(0xA2), 0x11);
        assert_eq!(fold_sided_modifier(0xA5), 0x12);
        assert_eq!(fold_sided_modifier(0x41), 0x41);
    }

    #[test]
    fn non_bmp_characters_are_skipped() {
        assert_eq!(bmp_chars("a\u{1F600}é").collect::<String>(), "aé");
    }

    #[test]
    fn suspension_follows_minimized_or_hidden_flags() {
        use Suspension::*;
        assert_eq!(suspension_change(false, true, false), Some(Suspend));
        assert_eq!(suspension_change(false, false, true), Some(Suspend));
        assert_eq!(suspension_change(false, false, false), None);
        // Already suspended: a second minimize/hide is no new transition.
        assert_eq!(suspension_change(true, true, true), None);
        // `Shown` while still minimized (or restored while still hidden) does not resume.
        assert_eq!(suspension_change(true, true, false), None);
        assert_eq!(suspension_change(true, false, true), None);
        assert_eq!(suspension_change(true, false, false), Some(Resume));
    }

    fn ms(n: u64) -> std::time::Duration {
        std::time::Duration::from_millis(n)
    }

    #[test]
    fn minimize_cycle_runs_in_order() {
        let t0 = std::time::Instant::now();
        let mut cycle = MinimizeCycle::Waiting;
        assert_eq!(cycle.step(t0, false, false), CycleAction::None);
        assert_eq!(cycle.step(t0, true, false), CycleAction::Minimize);
        // The window manager has not applied it yet.
        assert_eq!(cycle.step(t0 + ms(5), true, false), CycleAction::None);
        assert_eq!(cycle.step(t0 + ms(20), true, true), CycleAction::None);
        assert!(!cycle.finished());
        // Held suspended until TEST_MINIMIZE_HOLD, then the restore is requested once.
        let hold_end = t0 + ms(20) + TEST_MINIMIZE_HOLD;
        assert_eq!(cycle.step(hold_end - ms(1), true, true), CycleAction::None);
        assert_eq!(cycle.step(hold_end, true, true), CycleAction::Restore);
        assert_eq!(cycle.step(hold_end + ms(5), true, true), CycleAction::None);
        assert_eq!(
            cycle.step(hold_end + ms(30), true, false),
            CycleAction::Complete
        );
        assert!(cycle.finished());
        assert_eq!(cycle.step(hold_end + ms(40), true, true), CycleAction::None);
        assert_eq!(cycle, MinimizeCycle::Done);
    }

    #[test]
    fn minimize_cycle_fails_on_missing_or_out_of_order_transitions() {
        let t0 = std::time::Instant::now();
        let late = TEST_TRANSITION_TIMEOUT + ms(1);

        // Suspended before the scripted minimize: not attributable to the test.
        let mut cycle = MinimizeCycle::Waiting;
        assert!(matches!(cycle.step(t0, false, true), CycleAction::Fail(_)));
        assert!(cycle.finished());

        // The minimize never suspends frame work.
        let mut cycle = MinimizeCycle::Waiting;
        assert_eq!(cycle.step(t0, true, false), CycleAction::Minimize);
        assert_eq!(
            cycle.step(t0 + TEST_TRANSITION_TIMEOUT, true, false),
            CycleAction::None
        );
        assert!(matches!(
            cycle.step(t0 + late, true, false),
            CycleAction::Fail(_)
        ));
        assert_eq!(cycle, MinimizeCycle::Failed);

        // Frame work resumes on its own during the hold.
        let mut cycle = MinimizeCycle::Waiting;
        cycle.step(t0, true, false);
        cycle.step(t0 + ms(10), true, true);
        assert!(matches!(
            cycle.step(t0 + ms(20), true, false),
            CycleAction::Fail(_)
        ));

        // The restore never resumes frame work.
        let mut cycle = MinimizeCycle::Waiting;
        cycle.step(t0, true, false);
        cycle.step(t0, true, true);
        let restore_at = t0 + TEST_MINIMIZE_HOLD;
        assert_eq!(cycle.step(restore_at, true, true), CycleAction::Restore);
        assert!(matches!(
            cycle.step(restore_at + late, true, true),
            CycleAction::Fail(_)
        ));
        assert!(cycle.finished());
    }

    #[test]
    fn click_marker_is_exact_word() {
        assert!(is_click_marker("roves-test-click-received"));
        assert!(is_click_marker(
            "roves-test-click-received target=click-target trusted=true"
        ));
        assert!(!is_click_marker("roves-test-click-receivedx"));
        assert!(!is_click_marker("x roves-test-click-received"));
        assert!(!is_click_marker(""));
    }

    #[test]
    fn click_test_runs_in_order() {
        let t0 = std::time::Instant::now();
        let mut click = ClickTest::Waiting;
        assert_eq!(click.tick(t0, false), ClickAction::None);
        assert_eq!(click.tick(t0, true), ClickAction::Push);
        // Pushed once only.
        assert_eq!(click.tick(t0 + ms(1), true), ClickAction::None);
        assert_eq!(
            click.input(ClickInput::Down { ok: true }),
            ClickAction::None
        );
        assert_eq!(click.input(ClickInput::Up { ok: true }), ClickAction::None);
        assert!(!click.finished());
        assert_eq!(click.tick(t0 + ms(50), true), ClickAction::None);
        assert_eq!(click.marker(), ClickAction::Complete);
        assert!(click.finished());
        // Done is terminal for ticks (no timeout).
        assert_eq!(
            click.tick(t0 + TEST_TRANSITION_TIMEOUT * 2, true),
            ClickAction::None
        );
        assert_eq!(click, ClickTest::Done);
    }

    #[test]
    fn click_test_fails_on_refusal_order_or_timeout() {
        let t0 = std::time::Instant::now();
        let late = t0 + TEST_TRANSITION_TIMEOUT + ms(1);
        let requested = || {
            let mut click = ClickTest::Waiting;
            click.tick(t0, true);
            click
        };

        // welding refuses the button down or the button up.
        let mut click = requested();
        assert!(matches!(
            click.input(ClickInput::Down { ok: false }),
            ClickAction::Fail(_)
        ));
        assert!(click.finished());
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        assert!(matches!(
            click.input(ClickInput::Up { ok: false }),
            ClickAction::Fail(_)
        ));

        // Out of order: up before down, a second down, an event before the request.
        let mut click = requested();
        assert!(matches!(
            click.input(ClickInput::Up { ok: true }),
            ClickAction::Fail(_)
        ));
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        assert!(matches!(
            click.input(ClickInput::Down { ok: true }),
            ClickAction::Fail(_)
        ));
        let mut click = ClickTest::Waiting;
        assert!(matches!(
            click.input(ClickInput::Down { ok: true }),
            ClickAction::Fail(_)
        ));

        // The marker before the click was forwarded, or twice.
        let mut click = ClickTest::Waiting;
        assert!(matches!(click.marker(), ClickAction::Fail(_)));
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        assert!(matches!(click.marker(), ClickAction::Fail(_)));
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        click.input(ClickInput::Up { ok: true });
        assert_eq!(click.marker(), ClickAction::Complete);
        assert!(matches!(click.marker(), ClickAction::Fail(_)));

        // Each pending state times out relative to the request.
        let mut click = requested();
        assert_eq!(
            click.tick(t0 + TEST_TRANSITION_TIMEOUT, true),
            ClickAction::None
        );
        assert!(matches!(click.tick(late, true), ClickAction::Fail(_)));
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        assert!(matches!(click.tick(late, true), ClickAction::Fail(_)));
        let mut click = requested();
        click.input(ClickInput::Down { ok: true });
        click.input(ClickInput::Up { ok: true });
        assert!(matches!(click.tick(late, true), ClickAction::Fail(_)));
        assert_eq!(click, ClickTest::Failed);

        // Failed is terminal and silent.
        assert_eq!(
            click.input(ClickInput::Down { ok: true }),
            ClickAction::None
        );
        assert_eq!(click.marker(), ClickAction::None);
        assert_eq!(click.tick(late, true), ClickAction::None);
    }

    #[test]
    fn test_key_selector_accepts_only_the_regression_keys() {
        assert_eq!(TestKey::default(), TestKey::ArrowRight);
        assert_eq!(TestKey::parse("ArrowRight"), Ok(TestKey::ArrowRight));
        assert_eq!(TestKey::parse("CapsLock"), Ok(TestKey::CapsLock));
        assert_eq!(TestKey::ArrowRight.sdl_name(), "Right");
        assert_eq!(TestKey::CapsLock.sdl_name(), "CapsLock");
        // The pushed CapsLock (raw 0) resolves like a physical one: VK_CAPITAL, and
        // `MapVirtualKeyW`'s 0x3A without an 0xE0 prefix (Chromium dom_code_data.inc 0x003a).
        assert_eq!(non_character_virtual_key(SDLK_SCANCODE_MASK | 0x39), 0x14);
        assert_eq!(native_key_code(0, Some(0x39), 0x3A), 0x3A);
        for bad in ["", "capslock", "Right", "KeyA", "ArrowLeft", " CapsLock"] {
            assert!(TestKey::parse(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn test_key_delay_is_bounded() {
        assert_eq!(parse_test_key_delay("0"), Ok(ms(0)));
        assert_eq!(parse_test_key_delay("1000"), Ok(ms(1000)));
        assert_eq!(
            parse_test_key_delay("600000"),
            Ok(ms(TEST_KEY_MAX_DELAY_MS))
        );
        for bad in ["600001", "18446744073709551615", "", "-1", "1.5", "1000ms"] {
            assert!(parse_test_key_delay(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn key_markers_are_classified() {
        let m = "roves-test-key-received type=keydown key=ArrowRight code=ArrowRight trusted=true";
        assert_eq!(key_marker(m), Some(KeyMarker::Down));
        assert_eq!(
            key_marker("roves-test-key-received type=keyup"),
            Some(KeyMarker::Up)
        );
        assert_eq!(
            key_marker("roves-test-key-received type=keypress key=x"),
            Some(KeyMarker::Other)
        );
        assert_eq!(
            key_marker("roves-test-key-received type=keydownx"),
            Some(KeyMarker::Other)
        );
        assert_eq!(
            key_marker("roves-test-key-received"),
            Some(KeyMarker::Other)
        );
        assert_eq!(key_marker("roves-test-key-receivedx type=keydown"), None);
        assert_eq!(key_marker("x roves-test-key-received type=keydown"), None);
        assert_eq!(key_marker(""), None);
    }

    #[test]
    fn key_test_runs_in_order() {
        let t0 = std::time::Instant::now();
        let mut key = KeyTest::Waiting;
        assert_eq!(key.tick(t0, false), KeyAction::None);
        assert_eq!(key.tick(t0, true), KeyAction::Push);
        // Pushed once only.
        assert_eq!(key.tick(t0 + ms(1), true), KeyAction::None);
        assert_eq!(key.input(KeyInput::Down { ok: true }), KeyAction::None);
        assert_eq!(key.input(KeyInput::Up { ok: true }), KeyAction::None);
        assert_eq!(key.marker(KeyMarker::Down), KeyAction::None);
        assert!(!key.finished());
        assert_eq!(key.tick(t0 + ms(50), true), KeyAction::None);
        assert_eq!(key.marker(KeyMarker::Up), KeyAction::Complete);
        assert!(key.finished());
        // Done is terminal for ticks (no timeout).
        assert_eq!(
            key.tick(t0 + TEST_TRANSITION_TIMEOUT * 2, true),
            KeyAction::None
        );
        assert_eq!(key, KeyTest::Done);
    }

    #[test]
    fn key_test_fails_on_refusal_order_or_timeout() {
        let t0 = std::time::Instant::now();
        let late = t0 + TEST_TRANSITION_TIMEOUT + ms(1);
        let requested = || {
            let mut key = KeyTest::Waiting;
            key.tick(t0, true);
            key
        };
        let forwarded = || {
            let mut key = requested();
            key.input(KeyInput::Down { ok: true });
            key.input(KeyInput::Up { ok: true });
            key
        };
        let fails = |action: KeyAction| matches!(action, KeyAction::Fail(_));

        // welding refuses the key down or the key up.
        let mut key = requested();
        assert!(fails(key.input(KeyInput::Down { ok: false })));
        assert!(key.finished());
        let mut key = requested();
        key.input(KeyInput::Down { ok: true });
        assert!(fails(key.input(KeyInput::Up { ok: false })));

        // Out of order SDL events: up before down, a second down, an event before the
        // request, an extra event after both were forwarded.
        assert!(fails(requested().input(KeyInput::Up { ok: true })));
        let mut key = requested();
        key.input(KeyInput::Down { ok: true });
        assert!(fails(key.input(KeyInput::Down { ok: true })));
        assert!(fails(KeyTest::Waiting.input(KeyInput::Down { ok: true })));
        assert!(fails(forwarded().input(KeyInput::Down { ok: true })));

        // DOM markers: before forwarding, keyup first, keydown twice, keyup twice, an
        // unexpected type, anything after done.
        assert!(fails(KeyTest::Waiting.marker(KeyMarker::Down)));
        let mut key = requested();
        key.input(KeyInput::Down { ok: true });
        assert!(fails(key.marker(KeyMarker::Down)));
        assert!(fails(forwarded().marker(KeyMarker::Up)));
        let mut key = forwarded();
        key.marker(KeyMarker::Down);
        assert!(fails(key.marker(KeyMarker::Down)));
        assert!(fails(forwarded().marker(KeyMarker::Other)));
        let mut key = forwarded();
        key.marker(KeyMarker::Down);
        assert!(fails(key.marker(KeyMarker::Other)));
        let mut key = forwarded();
        key.marker(KeyMarker::Down);
        assert_eq!(key.marker(KeyMarker::Up), KeyAction::Complete);
        assert!(fails(key.marker(KeyMarker::Up)));
        let mut key = forwarded();
        key.marker(KeyMarker::Down);
        key.marker(KeyMarker::Up);
        assert!(fails(key.marker(KeyMarker::Down)));

        // Each pending state times out relative to the request.
        let mut key = requested();
        assert_eq!(
            key.tick(t0 + TEST_TRANSITION_TIMEOUT, true),
            KeyAction::None
        );
        assert!(fails(key.tick(late, true)));
        let mut key = requested();
        key.input(KeyInput::Down { ok: true });
        assert!(fails(key.tick(late, true)));
        assert!(fails(forwarded().tick(late, true)));
        let mut key = forwarded();
        key.marker(KeyMarker::Down);
        assert!(fails(key.tick(late, true)));
        assert_eq!(key, KeyTest::Failed);

        // Failed is terminal and silent.
        assert_eq!(key.input(KeyInput::Down { ok: true }), KeyAction::None);
        assert_eq!(key.marker(KeyMarker::Up), KeyAction::None);
        assert_eq!(key.tick(late, true), KeyAction::None);
    }

    #[test]
    fn wheel_conversion_gives_the_expected_cef_delta() {
        // The test notch, through the same conversion the input path uses.
        let (x, y) = TEST_WHEEL_LINES;
        assert_eq!(wheel_cef_delta(x, y, false), TEST_WHEEL_CEF_DELTA);
        assert_eq!(TEST_WHEEL_CEF_DELTA, (0, 120));
        // Flipped (natural scrolling) values are negated back; fractions round.
        assert_eq!(wheel_cef_delta(0.0, -1.0, true), (0, 120));
        assert_eq!(wheel_cef_delta(-1.0, 0.0, false), (-120, 0));
        assert_eq!(wheel_cef_delta(0.0, 0.5, false), (0, 60));
        assert_eq!(wheel_cef_delta(0.0, -2.0, false), (0, -240));
    }

    #[test]
    fn test_wheel_delay_is_bounded() {
        assert_eq!(parse_test_wheel_delay("0"), Ok(ms(0)));
        assert_eq!(parse_test_wheel_delay("1000"), Ok(ms(1000)));
        assert_eq!(
            parse_test_wheel_delay("600000"),
            Ok(ms(TEST_KEY_MAX_DELAY_MS))
        );
        for bad in ["600001", "", "-1", "1.5", "1s"] {
            let error = parse_test_wheel_delay(bad).expect_err(bad);
            assert!(error.starts_with("--roves-test-wheel-ms="), "{error}");
        }
        // The key flag keeps its own name in the message.
        assert!(
            parse_test_key_delay("x")
                .unwrap_err()
                .starts_with("--roves-test-key-ms=")
        );
    }

    #[test]
    fn wheel_markers_are_classified() {
        let good = "roves-test-wheel-received type=wheel deltaX=0 deltaY=-120 deltaZ=0 \
                    deltaMode=0 wheelDeltaY=120 trusted=true cancelable=true \
                    defaultPrevented=true";
        assert_eq!(
            wheel_marker(good),
            Some(WheelMarker::Expected {
                delta_y: -120.0,
                delta_mode: 0
            })
        );
        // Magnitude and deltaMode are reported, not checked (only direction).
        assert_eq!(
            wheel_marker(
                "roves-test-wheel-received type=wheel deltaX=-0 deltaY=-3 deltaMode=1 trusted=true"
            ),
            Some(WheelMarker::Expected {
                delta_y: -3.0,
                delta_mode: 1
            })
        );
        let unexpected = |m: &str| matches!(wheel_marker(m), Some(WheelMarker::Unexpected(_)));
        let base = "roves-test-wheel-received";
        for bad in [
            "",
            " type=mousewheel deltaX=0 deltaY=-120 deltaMode=0 trusted=true",
            " type=wheel deltaX=0 deltaY=-120 deltaMode=0 trusted=false",
            " type=wheel deltaX=0 deltaY=-120 deltaMode=0",
            " type=wheel deltaX=5 deltaY=-120 deltaMode=0 trusted=true",
            " type=wheel deltaY=-120 deltaMode=0 trusted=true",
            " type=wheel deltaX=0 deltaY=120 deltaMode=0 trusted=true",
            " type=wheel deltaX=0 deltaY=0 deltaMode=0 trusted=true",
            " type=wheel deltaX=0 deltaY=NaN deltaMode=0 trusted=true",
            " type=wheel deltaX=0 deltaY=-120 deltaMode=3 trusted=true",
            " type=wheel deltaX=0 deltaY=-120 trusted=true",
        ] {
            assert!(unexpected(&format!("{base}{bad}")), "{bad:?}");
        }
        assert_eq!(wheel_marker("roves-test-wheel-receivedx type=wheel"), None);
        assert_eq!(wheel_marker("x roves-test-wheel-received type=wheel"), None);
        assert_eq!(wheel_marker("roves-test-key-received type=keydown"), None);
    }

    const WHEEL_OK: WheelInput = WheelInput {
        ok: true,
        delta: Some(TEST_WHEEL_CEF_DELTA),
    };
    const WHEEL_DOM: WheelMarker = WheelMarker::Expected {
        delta_y: -120.0,
        delta_mode: 0,
    };

    #[test]
    fn wheel_test_runs_in_order() {
        let t0 = std::time::Instant::now();
        let mut wheel = WheelTest::Waiting;
        assert_eq!(wheel.tick(t0, false), WheelAction::None);
        assert_eq!(wheel.tick(t0, true), WheelAction::Push);
        // Pushed once only.
        assert_eq!(wheel.tick(t0 + ms(1), true), WheelAction::None);
        assert_eq!(wheel.input(WHEEL_OK), WheelAction::None);
        assert!(!wheel.finished());
        assert_eq!(wheel.tick(t0 + ms(50), true), WheelAction::None);
        assert_eq!(wheel.marker(WHEEL_DOM), WheelAction::Complete);
        assert!(wheel.finished());
        // Done is terminal for ticks (no timeout).
        assert_eq!(
            wheel.tick(t0 + TEST_TRANSITION_TIMEOUT * 2, true),
            WheelAction::None
        );
        assert_eq!(wheel, WheelTest::Done);
    }

    #[test]
    fn wheel_test_fails_on_refusal_delta_order_or_timeout() {
        let t0 = std::time::Instant::now();
        let late = t0 + TEST_TRANSITION_TIMEOUT + ms(1);
        let requested = || {
            let mut wheel = WheelTest::Waiting;
            wheel.tick(t0, true);
            wheel
        };
        let forwarded = || {
            let mut wheel = requested();
            wheel.input(WHEEL_OK);
            wheel
        };
        let fails = |action: WheelAction| matches!(action, WheelAction::Fail(_));

        // welding refuses the call; no wheel call; another delta (flipped sign, wrong axis).
        let mut wheel = requested();
        assert!(fails(wheel.input(WheelInput {
            ok: false,
            delta: Some(TEST_WHEEL_CEF_DELTA)
        })));
        assert!(wheel.finished());
        for delta in [None, Some((0, -120)), Some((120, 0)), Some((0, 360))] {
            assert!(
                fails(requested().input(WheelInput { ok: true, delta })),
                "{delta:?}"
            );
        }

        // Synthetic events out of order: before the request, a second one after forwarding,
        // one after completion.
        assert!(fails(WheelTest::Waiting.input(WHEEL_OK)));
        assert!(fails(forwarded().input(WHEEL_OK)));
        let mut wheel = forwarded();
        wheel.marker(WHEEL_DOM);
        assert!(fails(wheel.input(WHEEL_OK)));

        // DOM markers: before the request or forwarding, twice, unexpected fields.
        assert!(fails(WheelTest::Waiting.marker(WHEEL_DOM)));
        assert!(fails(requested().marker(WHEEL_DOM)));
        let mut wheel = forwarded();
        assert_eq!(wheel.marker(WHEEL_DOM), WheelAction::Complete);
        assert!(fails(wheel.marker(WHEEL_DOM)));
        assert!(fails(
            forwarded().marker(WheelMarker::Unexpected("wrong direction"))
        ));
        let mut wheel = forwarded();
        wheel.marker(WHEEL_DOM);
        assert!(fails(wheel.marker(WheelMarker::Unexpected("extra"))));

        // Each pending state times out relative to the request.
        let mut wheel = requested();
        assert_eq!(
            wheel.tick(t0 + TEST_TRANSITION_TIMEOUT, true),
            WheelAction::None
        );
        assert!(fails(wheel.tick(late, true)));
        let mut wheel = forwarded();
        assert!(fails(wheel.tick(late, true)));
        assert_eq!(wheel, WheelTest::Failed);

        // Failed is terminal and silent.
        assert_eq!(wheel.input(WHEEL_OK), WheelAction::None);
        assert_eq!(wheel.marker(WHEEL_DOM), WheelAction::None);
        assert_eq!(wheel.tick(late, true), WheelAction::None);
    }

    #[test]
    fn startup_milestones_report_once_relative_to_entry() {
        let t0 = std::time::Instant::now();
        let mut startup = Startup::new(t0);
        assert_eq!(
            startup.reach(Milestone::OptionsLogReady, t0 + ms(3)),
            Some(3)
        );
        assert_eq!(
            startup.reach(Milestone::FirstImport, t0 + ms(900)),
            Some(900)
        );
        // Every later frame: no second report.
        assert_eq!(startup.reach(Milestone::FirstImport, t0 + ms(916)), None);
        assert_eq!(
            startup.reach(Milestone::FirstPresent, t0 + ms(901)),
            Some(901)
        );
        assert_eq!(startup.reach(Milestone::FirstPresent, t0 + ms(917)), None);
        // Milestones are independent; an instant before entry saturates to 0.
        assert_eq!(startup.reach(Milestone::LoadStart, t0), Some(0));
        let mut early = Startup::new(t0 + ms(10));
        assert_eq!(early.reach(Milestone::SdlReady, t0), Some(0));
        let all = [
            Milestone::OptionsLogReady,
            Milestone::LoopbackReady,
            Milestone::CefInitialized,
            Milestone::SdlReady,
            Milestone::WindowSurfaceProducerReady,
            Milestone::LoadStart,
            Milestone::LoadEnd,
            Milestone::FirstImport,
            Milestone::FirstPresent,
            Milestone::StartupReleased,
        ];
        let mut fresh = Startup::new(t0);
        for (i, m) in all.iter().enumerate() {
            assert_eq!(fresh.reach(*m, t0 + ms(i as u64)), Some(i as u128));
            assert!(all[..i].iter().all(|other| other.name() != m.name()));
        }
        assert_eq!(Milestone::FirstPresent.name(), "first-present");
        assert_eq!(Milestone::StartupReleased.name(), "startup-released");
    }

    #[test]
    fn startup_gate_needs_load_end_then_a_newer_frame() {
        let mut gate = StartupGate::Loading;
        // Frames before load-end never release.
        assert!(!gate.imported(1));
        gate.end_turn(Some(1));
        assert_eq!(gate, StartupGate::Loading);
        assert_eq!(gate.timed_out(), Some(StartupFailure::LoadTimeout));
        // Load-end turn: the frame imported in the same turn (generation 2) may predate it.
        assert!(gate.load_end());
        assert!(!gate.load_end());
        assert!(!gate.imported(2));
        gate.end_turn(Some(2));
        assert_eq!(gate, StartupGate::WaitingFrame(Some(2)));
        assert_eq!(gate.timed_out(), Some(StartupFailure::NoFreshFrame));
        // The same generation imported again is not fresh; a newer one releases.
        assert!(!gate.imported(2));
        assert!(gate.imported(3));
        assert!(gate.released());
        assert_eq!(gate.timed_out(), None);
        assert!(!gate.imported(4));
        gate.end_turn(Some(4));
        assert!(gate.released());
    }

    #[test]
    fn startup_gate_without_earlier_frames_releases_on_the_first_import() {
        let mut gate = StartupGate::Loading;
        assert!(gate.load_end());
        gate.end_turn(None);
        assert_eq!(gate, StartupGate::WaitingFrame(None));
        assert!(gate.imported(1));
    }

    #[test]
    fn startup_gate_failures() {
        let mut gate = StartupGate::Loading;
        assert!(gate.fail(StartupFailure::LoadError));
        assert_eq!(gate.failure(), Some(StartupFailure::LoadError));
        // The first failure stays; nothing releases a failed gate.
        assert!(!gate.fail(StartupFailure::LoadTimeout));
        assert!(!gate.load_end());
        gate.end_turn(Some(9));
        assert!(!gate.imported(10));
        assert_eq!(gate.timed_out(), None);
        assert_eq!(gate.failure(), Some(StartupFailure::LoadError));

        // After the release only a renderer termination replaces the game.
        let mut released = StartupGate::Released;
        assert!(!released.fail(StartupFailure::LoadError));
        assert!(!released.fail(StartupFailure::NoFreshFrame));
        assert!(released.released());
        assert!(released.fail(StartupFailure::RendererTerminated));
        assert_eq!(released.failure(), Some(StartupFailure::RendererTerminated));
    }

    #[test]
    fn startup_failure_texts_are_distinct() {
        let all = [
            StartupFailure::LoadRejected,
            StartupFailure::LoadError,
            StartupFailure::RendererTerminated,
            StartupFailure::LoadTimeout,
            StartupFailure::NoFreshFrame,
        ];
        for (i, f) in all.iter().enumerate() {
            assert!(!f.message().is_empty());
            assert!(all[..i].iter().all(|other| other.token() != f.token()));
        }
        assert_eq!(StartupFailure::NoFreshFrame.token(), "no-fresh-frame");
    }

    #[test]
    fn player_profile_name_is_stable_and_install_specific() {
        let a = player_profile_name("play", r"\\?\C:\Games\Visual Novel");
        // Same install, any case: same profile.
        assert_eq!(a, player_profile_name("play", r"\\?\c:\games\visual novel"));
        assert!(a.starts_with("play-") && a.len() == "play-".len() + 16);
        // Another install of a package with the same executable name: another profile.
        assert_ne!(a, player_profile_name("play", r"\\?\C:\Games\Other Novel"));
        // FNV-1a 64 reference value: the empty input hashes to the offset basis.
        assert_eq!(player_profile_name("play", ""), "play-cbf29ce484222325");
        assert_eq!(
            player_profile_name("", "x"),
            player_profile_name("game", "x")
        );
        assert!(player_profile_name("my game.é/..", "x").starts_with("my_game_____-"));
        assert_eq!(
            player_profile_name(&"a".repeat(40), "x")
                .split('-')
                .next()
                .map(str::len),
            Some(32)
        );
    }

    #[test]
    fn origin_port_file_and_origin_parsing() {
        assert_eq!(parse_origin_port("47810"), Some(47810));
        assert_eq!(parse_origin_port(" 47810\r\n"), Some(47810));
        for bad in ["", "0", "65536", "-1", "+5", "47810x", "4 7810", "0x10"] {
            assert_eq!(parse_origin_port(bad), None, "{bad:?}");
        }
        assert_eq!(origin_port("http://127.0.0.1:51234"), Some(51234));
        assert_eq!(origin_port("http://127.0.0.1:51234/"), None);
        assert_eq!(origin_port("http://localhost:51234"), None);
    }
}
