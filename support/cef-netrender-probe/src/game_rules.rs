//! Pure helpers of the `--roves-game-dir` mode: origin policy, keyboard translation, the
//! minimized/hidden suspension predicate and the test-only minimize cycle (std only,
//! unit-tested stand-alone: `rustc --edition 2024 --test src/game_rules.rs`).
//!
//! CEF on Windows rebuilds a Win32 key message from `cef_key_event_t`: `windows_key_code` is
//! the message's virtual key and `native_key_code` its `lParam`, from which Chromium takes the
//! scan code that becomes `KeyboardEvent.code` (`ui/events/win/events_win_utils.cc`,
//! `GetScanCodeFromLParam`, pinned reference f2700724f59f). A zero `native_key_code` leaves
//! `KeyboardEvent.code` empty, which breaks games that bind keys by `code`.

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

/// winit 0.30's Windows scancode is "a 16bit extended scancode": the make code in the low
/// byte, `0xE0`/`0xE1` in the high byte for extended keys. This is the inverse of Chromium's
/// `GetScanCodeFromLParam` (same as its `GetLParamFromScanCode`): scan code in bits 16-23,
/// extended flag in bit 24.
pub(crate) fn scancode_lparam_bits(scancode: u32) -> u32 {
    let mut lparam = (scancode & 0x00FF) << 16;
    if scancode & 0xE000 == 0xE000 {
        lparam |= 1 << 24;
    }
    lparam
}

/// Which Win32 key message a CEF key event stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyMessage {
    /// `WM_KEYDOWN` (and the `WM_CHAR` it produces); `repeat` sets the previous-state bit.
    Down { repeat: bool },
    /// `WM_KEYUP`: previous-state and transition bits set.
    Up,
}

/// Full `lParam` as Windows would deliver it: repeat count 1, scan code and extended flag,
/// bit 30 (previous key state) for auto-repeats and key-ups, bit 31 (transition) for key-ups.
pub(crate) fn native_key_code(scancode: Option<u32>, message: KeyMessage) -> i32 {
    let mut lparam = 1 | scancode.map_or(0, scancode_lparam_bits);
    match message {
        KeyMessage::Down { repeat: true } => lparam |= 1 << 30,
        KeyMessage::Down { repeat: false } => {},
        KeyMessage::Up => lparam |= (1 << 30) | (1 << 31),
    }
    lparam as i32
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
    fn plain_scancode_goes_to_bits_16_to_23() {
        // 'A' make code 0x1E.
        assert_eq!(scancode_lparam_bits(0x1E), 0x001E_0000);
    }

    #[test]
    fn extended_scancode_sets_bit_24() {
        // Right arrow: E0 4D.
        assert_eq!(scancode_lparam_bits(0xE04D), 0x014D_0000);
        // Round trip through Chromium's GetScanCodeFromLParam.
        let lparam = scancode_lparam_bits(0xE04D);
        let mut back = (lparam >> 16) & 0xFF;
        if lparam & (1 << 24) != 0 {
            back |= 0xE000;
        }
        assert_eq!(back, 0xE04D);
    }

    #[test]
    fn key_messages_set_win32_state_bits() {
        assert_eq!(
            native_key_code(Some(0x1E), KeyMessage::Down { repeat: false }),
            0x001E_0001
        );
        assert_eq!(
            native_key_code(Some(0x1E), KeyMessage::Down { repeat: true }),
            0x401E_0001
        );
        assert_eq!(
            native_key_code(Some(0x1E), KeyMessage::Up) as u32,
            0xC01E_0001
        );
        assert_eq!(native_key_code(None, KeyMessage::Down { repeat: false }), 1);
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
}
