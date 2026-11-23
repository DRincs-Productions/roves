//! Pure helpers of the `--roves-game-dir` mode: origin policy, keyboard translation, the
//! input-readiness gate, the minimized/hidden suspension predicate and the test-only minimize, click and key sequences (std only,
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
    let ms = value
        .parse::<u64>()
        .map_err(|e| format!("--roves-test-key-ms={value}: {e}"))?;
    if ms > TEST_KEY_MAX_DELAY_MS {
        return Err(format!(
            "--roves-test-key-ms={value}: must be <= {TEST_KEY_MAX_DELAY_MS}"
        ));
    }
    Ok(std::time::Duration::from_millis(ms))
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
        }
    }
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
        ];
        let mut fresh = Startup::new(t0);
        for (i, m) in all.iter().enumerate() {
            assert_eq!(fresh.reach(*m, t0 + ms(i as u64)), Some(i as u128));
            assert!(all[..i].iter().all(|other| other.name() != m.name()));
        }
        assert_eq!(Milestone::FirstPresent.name(), "first-present");
    }
}
