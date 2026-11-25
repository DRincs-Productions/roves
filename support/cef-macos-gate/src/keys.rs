//! macOS and Linux key codes of the players' shared input path
//! (`support/cef-netrender-probe/src/input.rs`); the Linux gate compiles this file through
//! `#[path]`. The rules are in `key_rules.rs` (README "Input").

use crate::input::{KeyFields, KeyTranslation};
use crate::key_rules::windows_key_code;

/// macOS: CEF builds an `NSEvent` for a `Char` event too, where native code 0 would be
/// kVK_ANSI_A, so the text carries the key that produced it. Linux: 0 (DomCode NONE).
pub(crate) const CHAR_NATIVE_FROM_KEY: bool = cfg!(target_os = "macos");

pub(crate) fn translate(key: &KeyFields) -> KeyTranslation {
    let keycode = key.keycode.map_or(0, |k| k as u32);
    // SDL scancodes are USB HID usages (non-negative).
    let usage = key.scancode.map_or(0, |s| s.to_i32() as u32);
    KeyTranslation {
        windows_key_code: windows_key_code(keycode, usage) as i32,
        native_key_code: native_key_code(usage, key.raw),
        character: character(key, usage),
    }
}

/// Chromium's native code of the physical key; SDL's `raw` (the AppKit key code on macOS,
/// the X11 key code on X11) only when the table has none. A pushed event has `raw` 0.
fn native_key_code(usage: u32, raw: u16) -> i32 {
    #[cfg(target_os = "macos")]
    let code = crate::key_rules::mac_key_code(usage);
    #[cfg(not(target_os = "macos"))]
    let code = crate::key_rules::xkb_key_code(usage);
    i32::from(code.unwrap_or(raw))
}

/// macOS: `NSEvent.characters` of the key down/up, the layout character with the current
/// modifiers applied by SDL (`SDL_GetKeyFromScancode`), or AppKit's character of a function
/// key; `None` (a modifier change) for modifiers and CapsLock.
#[cfg(target_os = "macos")]
fn character(key: &KeyFields, usage: u32) -> Option<char> {
    use crate::key_rules::{mac_control_character, mac_printable_character};
    mac_control_character(usage).or_else(|| {
        let scancode = key.scancode?;
        // SAFETY: value-in/value-out lookup in SDL's current keymap (main thread).
        let shifted = unsafe {
            sdl3::sys::keyboard::SDL_GetKeyFromScancode(
                sdl3::sys::scancode::SDL_Scancode(scancode.to_i32()),
                sdl3::sys::keycode::SDL_Keymod(key.keymod.bits()),
                false,
            )
        };
        mac_printable_character(shifted.0)
            .or_else(|| mac_printable_character(key.keycode.map_or(0, |k| k as u32)))
    })
}

/// Linux: CEF derives the character from `windows_key_code` and Shift.
#[cfg(not(target_os = "macos"))]
fn character(_: &KeyFields, _: u32) -> Option<char> {
    None
}
