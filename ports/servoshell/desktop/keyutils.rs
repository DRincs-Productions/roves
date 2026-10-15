/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::str::FromStr;

use keyboard_types::{Code, Key, KeyState, Location, Modifiers, NamedKey};
use sdl3::keyboard::{Keycode, Mod, Scancode};
use servo::KeyboardEvent;

// Some shortcuts use Cmd on Mac and Control on other systems.
#[cfg(target_os = "macos")]
pub const CMD_OR_CONTROL: Modifiers = Modifiers::META;
#[cfg(not(target_os = "macos"))]
pub const CMD_OR_CONTROL: Modifiers = Modifiers::CONTROL;

/// The USB HID usage page of keyboard keys, which SDL3 scancodes are usages of.
const USB_KEYBOARD_PAGE: u16 = 0x07;

/// SDL3 scancodes are USB HID usage IDs on the keyboard page (0x07), so Chromium's USB HID to
/// DOM `code` table (the `keycode` crate) gives the DOM [`Code`]. Keys without a DOM code, or
/// with one `keyboard-types` does not know, report `Code::Unidentified`.
fn code_from_scancode(scancode: Option<Scancode>) -> Code {
    scancode
        .and_then(|scancode| u16::try_from(scancode.to_i32()).ok())
        .and_then(|usage| keycode::KeyMap::from_usb_code(USB_KEYBOARD_PAGE, usage).ok())
        .and_then(|key_map| key_map.code)
        .and_then(|code| Code::from_str(&code.to_string()).ok())
        .unwrap_or(Code::Unidentified)
}

fn location_from_scancode(scancode: Option<Scancode>) -> Location {
    match scancode {
        Some(
            Scancode::LCtrl | Scancode::LShift | Scancode::LAlt | Scancode::LGui,
        ) => Location::Left,
        Some(
            Scancode::RCtrl | Scancode::RShift | Scancode::RAlt | Scancode::RGui,
        ) => Location::Right,
        Some(
            Scancode::Kp0 |
            Scancode::Kp1 |
            Scancode::Kp2 |
            Scancode::Kp3 |
            Scancode::Kp4 |
            Scancode::Kp5 |
            Scancode::Kp6 |
            Scancode::Kp7 |
            Scancode::Kp8 |
            Scancode::Kp9 |
            Scancode::KpDivide |
            Scancode::KpMultiply |
            Scancode::KpMinus |
            Scancode::KpPlus |
            Scancode::KpEnter |
            Scancode::KpPeriod |
            Scancode::KpEquals |
            Scancode::KpComma,
        ) => Location::Numpad,
        _ => Location::Standard,
    }
}

/// TODO(SDL3 windowing): `Key::Character` is derived from `Keycode`'s own fixed identity here,
/// not from SDL3's separate `TextInput` event (the real, layout/shift-aware composed text) —
/// simpler, but means a shifted symbol key (e.g. Shift+1 producing "!" on a US layout) reports
/// the unshifted character here; the actual DOM `keypress`/`input` text a page sees still comes
/// through correctly regardless, since that's driven by `TextInput`, not this. Named keys map
/// on a best-effort basis; unrecognized keycodes fall back to `NamedKey::Unidentified`.
fn key_from_keycode(keycode: Option<Keycode>) -> Key {
    let Some(keycode) = keycode else {
        return Key::Named(NamedKey::Unidentified);
    };
    match keycode {
        Keycode::A => Key::Character("a".to_owned()),
        Keycode::B => Key::Character("b".to_owned()),
        Keycode::C => Key::Character("c".to_owned()),
        Keycode::D => Key::Character("d".to_owned()),
        Keycode::E => Key::Character("e".to_owned()),
        Keycode::F => Key::Character("f".to_owned()),
        Keycode::G => Key::Character("g".to_owned()),
        Keycode::H => Key::Character("h".to_owned()),
        Keycode::I => Key::Character("i".to_owned()),
        Keycode::J => Key::Character("j".to_owned()),
        Keycode::K => Key::Character("k".to_owned()),
        Keycode::L => Key::Character("l".to_owned()),
        Keycode::M => Key::Character("m".to_owned()),
        Keycode::N => Key::Character("n".to_owned()),
        Keycode::O => Key::Character("o".to_owned()),
        Keycode::P => Key::Character("p".to_owned()),
        Keycode::Q => Key::Character("q".to_owned()),
        Keycode::R => Key::Character("r".to_owned()),
        Keycode::S => Key::Character("s".to_owned()),
        Keycode::T => Key::Character("t".to_owned()),
        Keycode::U => Key::Character("u".to_owned()),
        Keycode::V => Key::Character("v".to_owned()),
        Keycode::W => Key::Character("w".to_owned()),
        Keycode::X => Key::Character("x".to_owned()),
        Keycode::Y => Key::Character("y".to_owned()),
        Keycode::Z => Key::Character("z".to_owned()),
        Keycode::_0 => Key::Character("0".to_owned()),
        Keycode::_1 => Key::Character("1".to_owned()),
        Keycode::_2 => Key::Character("2".to_owned()),
        Keycode::_3 => Key::Character("3".to_owned()),
        Keycode::_4 => Key::Character("4".to_owned()),
        Keycode::_5 => Key::Character("5".to_owned()),
        Keycode::_6 => Key::Character("6".to_owned()),
        Keycode::_7 => Key::Character("7".to_owned()),
        Keycode::_8 => Key::Character("8".to_owned()),
        Keycode::_9 => Key::Character("9".to_owned()),
        Keycode::Minus => Key::Character("-".to_owned()),
        Keycode::Equals => Key::Character("=".to_owned()),
        Keycode::LeftBracket => Key::Character("[".to_owned()),
        Keycode::RightBracket => Key::Character("]".to_owned()),
        Keycode::Backslash => Key::Character("\\".to_owned()),
        Keycode::Semicolon => Key::Character(";".to_owned()),
        Keycode::Apostrophe => Key::Character("'".to_owned()),
        Keycode::Grave => Key::Character("`".to_owned()),
        Keycode::Comma => Key::Character(",".to_owned()),
        Keycode::Period => Key::Character(".".to_owned()),
        Keycode::Slash => Key::Character("/".to_owned()),
        Keycode::Space => Key::Character(" ".to_owned()),
        Keycode::Return | Keycode::KpEnter => Key::Named(NamedKey::Enter),
        Keycode::Escape => Key::Named(NamedKey::Escape),
        Keycode::Backspace => Key::Named(NamedKey::Backspace),
        Keycode::Tab => Key::Named(NamedKey::Tab),
        Keycode::CapsLock => Key::Named(NamedKey::CapsLock),
        Keycode::F1 => Key::Named(NamedKey::F1),
        Keycode::F2 => Key::Named(NamedKey::F2),
        Keycode::F3 => Key::Named(NamedKey::F3),
        Keycode::F4 => Key::Named(NamedKey::F4),
        Keycode::F5 => Key::Named(NamedKey::F5),
        Keycode::F6 => Key::Named(NamedKey::F6),
        Keycode::F7 => Key::Named(NamedKey::F7),
        Keycode::F8 => Key::Named(NamedKey::F8),
        Keycode::F9 => Key::Named(NamedKey::F9),
        Keycode::F10 => Key::Named(NamedKey::F10),
        Keycode::F11 => Key::Named(NamedKey::F11),
        Keycode::F12 => Key::Named(NamedKey::F12),
        Keycode::PrintScreen => Key::Named(NamedKey::PrintScreen),
        Keycode::ScrollLock => Key::Named(NamedKey::ScrollLock),
        Keycode::Pause => Key::Named(NamedKey::Pause),
        Keycode::Insert => Key::Named(NamedKey::Insert),
        Keycode::Home => Key::Named(NamedKey::Home),
        Keycode::PageUp => Key::Named(NamedKey::PageUp),
        Keycode::Delete => Key::Named(NamedKey::Delete),
        Keycode::End => Key::Named(NamedKey::End),
        Keycode::PageDown => Key::Named(NamedKey::PageDown),
        Keycode::Right => Key::Named(NamedKey::ArrowRight),
        Keycode::Left => Key::Named(NamedKey::ArrowLeft),
        Keycode::Down => Key::Named(NamedKey::ArrowDown),
        Keycode::Up => Key::Named(NamedKey::ArrowUp),
        Keycode::LCtrl | Keycode::RCtrl => Key::Named(NamedKey::Control),
        Keycode::LShift | Keycode::RShift => Key::Named(NamedKey::Shift),
        Keycode::LAlt | Keycode::RAlt => Key::Named(NamedKey::Alt),
        Keycode::LGui | Keycode::RGui => Key::Named(NamedKey::Meta),
        Keycode::Application => Key::Named(NamedKey::ContextMenu),
        Keycode::Cut => Key::Named(NamedKey::Cut),
        Keycode::Copy => Key::Named(NamedKey::Copy),
        Keycode::Paste => Key::Named(NamedKey::Paste),
        Keycode::Find => Key::Named(NamedKey::Find),
        Keycode::Mute => Key::Named(NamedKey::AudioVolumeMute),
        Keycode::VolumeUp => Key::Named(NamedKey::AudioVolumeUp),
        Keycode::VolumeDown => Key::Named(NamedKey::AudioVolumeDown),
        _ => Key::Named(NamedKey::Unidentified),
    }
}

fn keyboard_modifiers_from_sdl_mod(keymod: Mod) -> Modifiers {
    let mut modifiers = Modifiers::empty();
    modifiers.set(
        Modifiers::CONTROL,
        keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD),
    );
    modifiers.set(
        Modifiers::SHIFT,
        keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
    );
    modifiers.set(Modifiers::ALT, keymod.intersects(Mod::LALTMOD | Mod::RALTMOD));
    modifiers.set(Modifiers::META, keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD));
    modifiers
}

/// Builds a DOM-shaped [`KeyboardEvent`] from the raw fields of an `sdl3::event::Event::
/// KeyDown`/`KeyUp` — the SDL3-windowing equivalent of the old `keyboard_event_from_winit`.
pub fn keyboard_event_from_sdl(
    keycode: Option<Keycode>,
    scancode: Option<Scancode>,
    keymod: Mod,
    pressed: bool,
    repeat: bool,
) -> KeyboardEvent {
    KeyboardEvent::new_without_event(
        if pressed { KeyState::Down } else { KeyState::Up },
        key_from_keycode(keycode),
        code_from_scancode(scancode),
        location_from_scancode(scancode),
        keyboard_modifiers_from_sdl_mod(keymod),
        repeat,
        false,
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_character_code_modifiers_and_repeat() {
        let event = keyboard_event_from_sdl(
            Some(Keycode::A),
            Some(Scancode::A),
            Mod::LCTRLMOD | Mod::RSHIFTMOD,
            true,
            true,
        )
        .event;

        assert_eq!(event.state, KeyState::Down);
        assert_eq!(event.key, Key::Character("a".to_owned()));
        assert_eq!(event.code, Code::KeyA);
        assert_eq!(event.location, Location::Standard);
        assert!(event.modifiers.contains(Modifiers::CONTROL));
        assert!(event.modifiers.contains(Modifiers::SHIFT));
        assert!(event.repeat);
        assert!(!event.is_composing);
    }

    #[test]
    fn maps_keypad_location_and_key_release() {
        let event = keyboard_event_from_sdl(
            Some(Keycode::KpEnter),
            Some(Scancode::KpEnter),
            Mod::NOMOD,
            false,
            false,
        )
        .event;

        assert_eq!(event.state, KeyState::Up);
        assert_eq!(event.key, Key::Named(NamedKey::Enter));
        assert_eq!(event.code, Code::NumpadEnter);
        assert_eq!(event.location, Location::Numpad);
        assert!(event.modifiers.is_empty());
    }

    #[test]
    fn maps_scancodes_to_dom_codes() {
        for (scancode, code) in [
            (Scancode::A, Code::KeyA),
            (Scancode::Z, Code::KeyZ),
            (Scancode::_0, Code::Digit0),
            (Scancode::_9, Code::Digit9),
            (Scancode::Return, Code::Enter),
            (Scancode::Space, Code::Space),
            (Scancode::Grave, Code::Backquote),
            (Scancode::Up, Code::ArrowUp),
            (Scancode::Down, Code::ArrowDown),
            (Scancode::Left, Code::ArrowLeft),
            (Scancode::Right, Code::ArrowRight),
            (Scancode::F1, Code::F1),
            (Scancode::F12, Code::F12),
            (Scancode::F24, Code::F24),
            (Scancode::NumLockClear, Code::NumLock),
            (Scancode::Kp0, Code::Numpad0),
            (Scancode::KpEnter, Code::NumpadEnter),
            (Scancode::KpPlus, Code::NumpadAdd),
            (Scancode::KpPeriod, Code::NumpadDecimal),
            (Scancode::LCtrl, Code::ControlLeft),
            (Scancode::RShift, Code::ShiftRight),
            (Scancode::LAlt, Code::AltLeft),
            (Scancode::RGui, Code::MetaRight),
            (Scancode::Application, Code::ContextMenu),
            (Scancode::Mute, Code::AudioVolumeMute),
        ] {
            assert_eq!(code_from_scancode(Some(scancode)), code, "{scancode:?}");
        }
    }

    #[test]
    fn missing_sdl_codes_use_dom_unidentified_fallbacks() {
        let event = keyboard_event_from_sdl(None, None, Mod::NOMOD, true, false).event;

        assert_eq!(event.key, Key::Named(NamedKey::Unidentified));
        assert_eq!(event.code, Code::Unidentified);
        assert_eq!(event.location, Location::Standard);
    }
}
