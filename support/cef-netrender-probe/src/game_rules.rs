//! Pure helpers of the `--roves-game-dir` mode: origin policy and keyboard translation (std
//! only, unit-tested stand-alone: `rustc --edition 2024 --test src/game_rules.rs`).
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
}
