//! Pure key-code rules of the macOS and Linux players (std + the `keycode` crate; compiled by
//! both gates through `#[path]` and unit-tested in `support/cef-host-api-tests`).
//!
//! What CEF 154 reads from `cef_key_event_t` per platform (Chromium 154.0.8037.98):
//! - macOS (`browser_platform_delegate_native_mac.mm` `TranslateWebKeyEvent`): an `NSEvent`
//!   is synthesized from `native_key_code` (the macOS virtual key code, `kVK_*`),
//!   `character`/`unmodified_character` and the modifiers, then translated by Chromium's own
//!   `WebKeyboardEventBuilder` exactly like a real key: DOM `code` from the key code
//!   (`ui/events/keycodes/dom/dom_code_data.inc`), `key` from the characters (function keys
//!   as the AppKit private-use characters, `ui/events/keycodes/keyboard_code_conversion_mac.mm`
//!   `DomKeyFromNSEvent`), `keyCode` from both. A key event with neither character is taken
//!   as `NSEventTypeFlagsChanged` (a modifier), so every non-modifier key needs one.
//! - Linux (`browser_platform_delegate_native_linux.cc`): DOM `keyCode` and `key` come from
//!   `windows_key_code` (through `XKeysymForWindowsKeyCode` with the Shift flag), DOM `code`
//!   from `native_key_code` as an XKB key code (`KeycodeConverter::NativeKeycodeToDomCode`).
//!
//! The native codes come from Chromium's own table (`dom_code_data.inc`) through the
//! `keycode` crate (MIT, generated from that file), indexed by SDL's scancode, which is the
//! USB HID usage of the physical key. The Windows virtual key of a character key follows the
//! layout character as Chromium's X11 path does (`KeyboardCodeFromXKeysym`); keys without a
//! character use the Windows player's table (`game_rules::non_character_virtual_key`).

use crate::game_rules::{SDLK_SCANCODE_MASK, fold_sided_modifier, non_character_virtual_key};

/// USB HID usage page of SDL's keyboard scancodes.
const USB_KEYBOARD_PAGE: u16 = 0x07;

fn key_map(usage: u32) -> Option<keycode::KeyMap> {
    let usage = u16::try_from(usage).ok().filter(|u| *u > 0 && *u <= 0xFF)?;
    keycode::KeyMap::from_usb_code(USB_KEYBOARD_PAGE, usage).ok()
}

/// macOS virtual key code (`kVK_*`) of an SDL scancode; `None` when Chromium has none.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn mac_key_code(usage: u32) -> Option<u16> {
    key_map(usage).map(|k| k.mac).filter(|code| *code != 0xFFFF)
}

/// XKB key code (evdev + 8, what X11 reports) of an SDL scancode; `None` when Chromium has
/// none.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn xkb_key_code(usage: u32) -> Option<u16> {
    key_map(usage).map(|k| k.xkb).filter(|code| *code != 0)
}

/// `windows_key_code` of an SDL key: `keycode` is SDL's layout keycode (the unshifted
/// character, or `SDLK_SCANCODE_MASK | usage` for keys without one; 0 when SDL gave none),
/// `usage` the scancode. Sided modifiers fold to VK_SHIFT/VK_CONTROL/VK_MENU as in a native
/// event; 0 when the key is unknown.
pub(crate) fn windows_key_code(keycode: u32, usage: u32) -> u32 {
    let vk = if keycode & SDLK_SCANCODE_MASK != 0 {
        non_character_virtual_key(keycode)
    } else {
        character_virtual_key(keycode).unwrap_or_else(|| position_virtual_key(usage))
    };
    fold_sided_modifier(vk)
}

/// Virtual key of a layout character (`KeyboardCodeFromXKeysym`, ASCII only).
fn character_virtual_key(keycode: u32) -> Option<u32> {
    let c = char::from_u32(keycode).filter(char::is_ascii)?;
    Some(match c {
        '\r' => 0x0D,
        '\x1B' => 0x1B,
        '\x08' => 0x08,
        '\t' => 0x09,
        ' ' => 0x20,
        '\x7F' => 0x2E, // SDLK_DELETE (forward delete)
        'a'..='z' => u32::from(c) - 0x20,
        'A'..='Z' | '0'..='9' => u32::from(c),
        ';' | ':' => 0xBA,  // VK_OEM_1
        '=' | '+' => 0xBB,  // VK_OEM_PLUS
        ',' | '<' => 0xBC,  // VK_OEM_COMMA
        '-' | '_' => 0xBD,  // VK_OEM_MINUS
        '.' | '>' => 0xBE,  // VK_OEM_PERIOD
        '/' | '?' => 0xBF,  // VK_OEM_2
        '`' | '~' => 0xC0,  // VK_OEM_3
        '[' | '{' => 0xDB,  // VK_OEM_4
        '\\' | '|' => 0xDC, // VK_OEM_5
        ']' | '}' => 0xDD,  // VK_OEM_6
        '\'' | '"' => 0xDE, // VK_OEM_7
        _ => return None,
    })
}

/// Virtual key of the US-layout key at the scancode's position, for layout characters
/// without an ASCII virtual key (e.g. `é`), as Chromium's `DomCodeToUsLayoutKeyboardCode`.
fn position_virtual_key(usage: u32) -> u32 {
    match usage {
        0x04..=0x1D => 0x41 + usage - 0x04, // KeyA..KeyZ
        0x1E..=0x26 => 0x31 + usage - 0x1E, // Digit1..Digit9
        0x27 => 0x30,                       // Digit0
        0x28 => 0x0D,                       // Enter
        0x29 => 0x1B,                       // Escape
        0x2A => 0x08,                       // Backspace
        0x2B => 0x09,                       // Tab
        0x2C => 0x20,                       // Space
        0x2D => 0xBD,                       // Minus
        0x2E => 0xBB,                       // Equal
        0x2F => 0xDB,                       // BracketLeft
        0x30 => 0xDD,                       // BracketRight
        0x31 | 0x32 => 0xDC,                // Backslash, IntlHash
        0x33 => 0xBA,                       // Semicolon
        0x34 => 0xDE,                       // Quote
        0x35 => 0xC0,                       // Backquote
        0x36 => 0xBC,                       // Comma
        0x37 => 0xBE,                       // Period
        0x38 => 0xBF,                       // Slash
        0x4C => 0x2E,                       // Delete
        0x64 => 0xE2,                       // IntlBackslash -> VK_OEM_102
        usage => non_character_virtual_key(SDLK_SCANCODE_MASK | usage),
    }
}

/// macOS: the `characters` AppKit reports for a key without a printable character (Return,
/// Backspace, arrows, function keys, ...; `NSEvent.h` `NS*FunctionKey`), by SDL scancode.
/// `None` for modifiers and CapsLock (their events carry no character) and printable keys.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn mac_control_character(usage: u32) -> Option<char> {
    let code = match usage {
        0x28 => 0x0D,                         // Return
        0x29 => 0x1B,                         // Escape
        0x2A => 0x7F,                         // Backspace (AppKit reports DEL)
        0x2B => 0x09,                         // Tab
        0x3A..=0x45 => 0xF704 + usage - 0x3A, // F1..F12 -> NSF1FunctionKey..
        0x46 => 0xF72E,                       // PrintScreen -> NSPrintScreenFunctionKey
        0x47 => 0xF72F,                       // ScrollLock
        0x48 => 0xF730,                       // Pause
        0x49 => 0xF727,                       // Insert (Help on Apple keyboards)
        0x4A => 0xF729,                       // Home
        0x4B => 0xF72C,                       // PageUp
        0x4C => 0xF728,                       // Delete (forward)
        0x4D => 0xF72B,                       // End
        0x4E => 0xF72D,                       // PageDown
        0x4F => 0xF703,                       // ArrowRight
        0x50 => 0xF702,                       // ArrowLeft
        0x51 => 0xF701,                       // ArrowDown
        0x52 => 0xF700,                       // ArrowUp
        0x53 => 0xF739,                       // NumLock (Clear) -> NSClearLineFunctionKey
        0x58 => 0x03,                         // NumpadEnter (AppKit reports ETX)
        0x65 => 0xF735,                       // ContextMenu -> NSMenuFunctionKey
        0x68..=0x73 => 0xF710 + usage - 0x68, // F13..F24
        _ => return None,
    };
    char::from_u32(code)
}

/// macOS: a printable layout character usable as `NSEvent.characters` (`None` for control
/// characters and SDL's scancode-masked keycodes).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn mac_printable_character(keycode: u32) -> Option<char> {
    if keycode & SDLK_SCANCODE_MASK != 0 || keycode > 0xFFFF {
        return None;
    }
    char::from_u32(keycode).filter(|c| !c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SDL scancodes (USB HID usages) of the keys checked below.
    const KEY_A: u32 = 0x04;
    const ARROW_RIGHT: u32 = 0x4F;
    const CAPS_LOCK: u32 = 0x39;
    const SHIFT_LEFT: u32 = 0xE1;

    #[test]
    fn native_codes_follow_chromium_dom_code_data() {
        // kVK_ANSI_A = 0x00, kVK_RightArrow = 0x7C, kVK_CapsLock = 0x39, kVK_Shift = 0x38.
        assert_eq!(mac_key_code(KEY_A), Some(0x00));
        assert_eq!(mac_key_code(ARROW_RIGHT), Some(0x7C));
        assert_eq!(mac_key_code(CAPS_LOCK), Some(0x39));
        assert_eq!(mac_key_code(SHIFT_LEFT), Some(0x38));
        // XKB = evdev + 8: KEY_A 30, KEY_RIGHT 106, KEY_CAPSLOCK 58, KEY_LEFTSHIFT 42.
        assert_eq!(xkb_key_code(KEY_A), Some(38));
        assert_eq!(xkb_key_code(ARROW_RIGHT), Some(114));
        assert_eq!(xkb_key_code(CAPS_LOCK), Some(66));
        assert_eq!(xkb_key_code(SHIFT_LEFT), Some(50));
        assert_eq!(mac_key_code(0), None);
        assert_eq!(xkb_key_code(0x1_0000), None);
    }

    #[test]
    fn windows_key_codes_match_native_dom_key_codes() {
        let masked = |usage: u32| SDLK_SCANCODE_MASK | usage;
        assert_eq!(windows_key_code(u32::from('a'), KEY_A), 0x41);
        assert_eq!(windows_key_code(masked(ARROW_RIGHT), ARROW_RIGHT), 0x27);
        assert_eq!(windows_key_code(masked(CAPS_LOCK), CAPS_LOCK), 0x14);
        assert_eq!(windows_key_code(masked(SHIFT_LEFT), SHIFT_LEFT), 0x10);
        assert_eq!(windows_key_code(u32::from(';'), 0x33), 0xBA);
        assert_eq!(windows_key_code(0x0D, 0x28), 0x0D);
        assert_eq!(windows_key_code(0x7F, 0x4C), 0x2E);
        // AZERTY: the layout says 'q' at the US A position, 'é' at the US Digit2 position.
        assert_eq!(windows_key_code(u32::from('q'), KEY_A), 0x51);
        assert_eq!(windows_key_code(u32::from('é'), 0x1F), 0x32);
        // No keycode from SDL: the US key at that position.
        assert_eq!(windows_key_code(0, KEY_A), 0x41);
        assert_eq!(windows_key_code(0, 0), 0);
    }

    #[test]
    fn mac_characters_name_function_keys_and_skip_modifiers() {
        assert_eq!(mac_control_character(ARROW_RIGHT), Some('\u{F703}'));
        assert_eq!(mac_control_character(0x3A), Some('\u{F704}'));
        assert_eq!(mac_control_character(0x68), Some('\u{F710}'));
        assert_eq!(mac_control_character(0x2A), Some('\u{7F}'));
        assert_eq!(mac_control_character(CAPS_LOCK), None);
        assert_eq!(mac_control_character(SHIFT_LEFT), None);
        assert_eq!(mac_control_character(KEY_A), None);
        assert_eq!(mac_printable_character(u32::from('A')), Some('A'));
        assert_eq!(mac_printable_character(u32::from('é')), Some('é'));
        assert_eq!(mac_printable_character(0x0D), None);
        assert_eq!(
            mac_printable_character(SDLK_SCANCODE_MASK | ARROW_RIGHT),
            None
        );
    }
}
