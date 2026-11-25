//! Windows key codes of the shared input path (`input.rs`): CEF 154 on Windows derives the
//! DOM `key`/`keyCode` from `windows_key_code` (the layout-aware Win32 virtual key, looked up
//! with `VkKeyScanW`) and the DOM `code` from `native_key_code` (the scan code in a
//! `WM_KEYDOWN` `lParam`). The macOS and Linux players use `cef-macos-gate/src/keys.rs`.

use sdl3::keyboard::Keycode;

use crate::game_rules::{
    SDLK_SCANCODE_MASK, fold_sided_modifier, native_key_code, non_character_virtual_key,
};
use crate::input::{KeyFields, KeyTranslation};

/// Windows `Char` events carry no scan code (0 is DomCode NONE).
pub(crate) const CHAR_NATIVE_FROM_KEY: bool = false;

pub(crate) fn translate(key: &KeyFields) -> KeyTranslation {
    let vk = key.keycode.map_or(0, virtual_key);
    // SDL scancodes are USB HID usages (non-negative).
    let usage = key.scancode.map(|s| s.to_i32() as u32);
    KeyTranslation {
        windows_key_code: fold_sided_modifier(vk as u32) as i32,
        native_key_code: native_key_code(key.raw, usage, mapped_scan_code(vk)),
        character: None,
    }
}

/// The scan code Windows associates with a virtual key (0 when none); only the fallback of
/// `native_key_code` when SDL reports no raw scan code. For VK_RIGHT and the other keys that
/// share a virtual key with the NumLock-off keypad it returns the keypad code (no `0xE0`).
fn mapped_scan_code(vk: i32) -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC_EX, MapVirtualKeyW};
    if vk <= 0 {
        return 0;
    }
    // SAFETY: value-in/value-out Windows keyboard mapping.
    unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) }
}

fn virtual_key(key: Keycode) -> i32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;
    match key {
        Keycode::Return => 0x0D,
        Keycode::Escape => 0x1B,
        Keycode::Backspace => 0x08,
        Keycode::Tab => 0x09,
        Keycode::Delete => 0x2E,
        // Keys without a character (`SDLK_SCANCODE_MASK | scancode`) are no UTF-16 unit for
        // `VkKeyScanW`: fixed table, unit-tested in `game_rules`.
        key if key as u32 & SDLK_SCANCODE_MASK != 0 => non_character_virtual_key(key as u32) as i32,
        key => {
            let Ok(unit) = u16::try_from(key as u32) else {
                return 0;
            };
            // SAFETY: value-in/value-out lookup in the current Windows keyboard layout.
            let mapped = unsafe { VkKeyScanW(unit) };
            if mapped == -1 {
                return 0;
            }
            (mapped & 0xFF) as i32
        },
    }
}
