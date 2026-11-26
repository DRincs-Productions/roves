//! SDL3 -> welding page input shared by the three CEF players (Windows `game.rs`, and the
//! macOS and Linux gates through `#[path]`): mouse move/buttons, wheel, keyboard, committed
//! text, focus, visibility, scale and the page's cursor shape, forwarded through welding's
//! platform-neutral `CefSurfaceProducer` input API.
//!
//! Only the key codes are per platform: each crate provides a `keys` module with
//! `translate(&KeyFields) -> KeyTranslation` (CEF `windows_key_code`, `native_key_code` and
//! the key event's character) and `CHAR_NATIVE_FROM_KEY` (whether a `Char` event carries the
//! native code of the key that produced it). Windows: `cef-windows-player/src/keys.rs`
//! (Win32 layout lookups); macOS and Linux: `cef-macos-gate/src/keys.rs` (Chromium's
//! keycode table through the `keycode` crate).

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod, Scancode},
    mouse::{Cursor, MouseButton as SdlButton, MouseWheelDirection, SystemCursor},
    video::Window,
};
use welding::{
    CefSurfaceProducer, CursorShape, EventModifiers, FocusDirection, KeyEvent, KeyEventKind,
    MouseAction, MouseButton, MouseEvent,
};

use crate::game_rules::{InputEventKind, bmp_chars, forward_input, wheel_cef_delta};

/// The SDL fields of one key down/up that the platform translation reads.
pub(crate) struct KeyFields {
    pub(crate) keycode: Option<Keycode>,
    pub(crate) scancode: Option<Scancode>,
    /// Read by the macOS character lookup only (Windows and Linux CEF derive the character).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) keymod: Mod,
    /// `SDL_KeyboardEvent.raw` (platform scan/key code; 0 for a pushed event).
    pub(crate) raw: u16,
}

/// What the platform `keys::translate` made of one key down/up for CEF.
pub(crate) struct KeyTranslation {
    pub(crate) windows_key_code: i32,
    pub(crate) native_key_code: i32,
    /// `cef_key_event_t.character`/`unmodified_character` of the key down/up (`None`: 0).
    pub(crate) character: Option<char>,
}

/// SDL3 -> welding input state. Coordinates are physical pixels; welding converts them to
/// CEF's DIP with the producer's scale factor.
#[derive(Default)]
pub(crate) struct Input {
    modifiers: EventModifiers,
    cursor: (i32, i32),
    /// Set by `Focused(true)` and at startup; applied once a frame proved the browser exists.
    focus_pending: bool,
    /// Input calls welding refused after the game load-end (browser closed or failing). Only
    /// the first is logged.
    pub(crate) rejected: u32,
    /// User input dropped before the accepted game load-end (welding has no browser host
    /// yet). Only the first is logged.
    pub(crate) discarded: u32,
    pub(crate) forwarded: u64,
    /// Codes of the last key down/up handed to welding (log evidence of the key test).
    pub(crate) last_key: Option<KeyCodes>,
    /// `(delta_x, delta_y)` of the last wheel call handed to welding (log evidence of the
    /// wheel test).
    pub(crate) last_wheel: Option<(i32, i32)>,
    /// Native code of the last key down, for `Char` events where the platform wants it.
    last_down_native: i32,
    cursor_hidden: bool,
    active_cursor: Option<Cursor>,
}

/// What one SDL key down/up became for CEF.
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyCodes {
    /// `SDL_KeyboardEvent.raw` (platform scan code; 0 when SDL had none, e.g. a pushed event).
    pub(crate) raw: u16,
    /// SDL scancode (USB HID usage; 0 when unknown).
    pub(crate) usage: u32,
    pub(crate) windows_key_code: i32,
    pub(crate) native_key_code: i32,
}

impl std::fmt::Display for KeyCodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "raw={:#06X} usage={:#04X} windows_key_code={:#04X} native_key_code={:#06X}",
            self.raw, self.usage, self.windows_key_code, self.native_key_code
        )
    }
}

impl Input {
    pub(crate) fn new() -> Self {
        Input {
            focus_pending: true,
            ..Default::default()
        }
    }

    /// Forwards SDL logical mouse coordinates as physical pixels to welding. `game_loaded`:
    /// the accepted game load-end has been handled; until then user input is discarded and
    /// only window events reach the producer.
    pub(crate) fn window_event<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        window: &Window,
        event: &Event,
        game_loaded: bool,
    ) -> Option<String> {
        let user_input = user_input_name(event);
        let kind = if user_input.is_some() {
            InputEventKind::User
        } else {
            InputEventKind::Window
        };
        if !forward_input(kind, game_loaded) {
            self.discarded += 1;
            return (self.discarded == 1).then(|| {
                format!(
                    "input discarded before the game load-end (first; later ones only counted): event={}",
                    user_input.unwrap_or_default()
                )
            });
        }
        let point = |x: f32, y: f32| {
            let (width, height) = window.size();
            let (pixels_wide, pixels_high) = window.size_in_pixels();
            (
                (x * pixels_wide as f32 / width.max(1) as f32).round() as i32,
                (y * pixels_high as f32 / height.max(1) as f32).round() as i32,
            )
        };
        let result = match event {
            Event::Window { win_event, .. } => match win_event {
                WindowEvent::FocusGained => {
                    self.focus_pending = true;
                    return None;
                },
                WindowEvent::Occluded | WindowEvent::Minimized | WindowEvent::Hidden => {
                    producer.set_visible(false)
                },
                // Maximized: a restore from minimized straight to maximized.
                WindowEvent::Restored
                | WindowEvent::Maximized
                | WindowEvent::Exposed
                | WindowEvent::Shown => producer.set_visible(true),
                WindowEvent::DisplayChanged(_) => producer.set_scale_factor(window.display_scale()),
                _ => return None,
            },
            Event::MouseMotion { x, y, .. } => {
                self.cursor = point(*x, *y);
                self.mouse(producer, MouseButton::Left, MouseAction::Moved)
            },
            Event::MouseButtonDown {
                mouse_btn, x, y, ..
            }
            | Event::MouseButtonUp {
                mouse_btn, x, y, ..
            } => {
                self.cursor = point(*x, *y);
                let button = match mouse_btn {
                    SdlButton::Left => MouseButton::Left,
                    SdlButton::Middle => MouseButton::Middle,
                    SdlButton::Right => MouseButton::Right,
                    _ => return None,
                };
                let pressed = matches!(event, Event::MouseButtonDown { .. });
                match button {
                    MouseButton::Left => self.modifiers.left_mouse_button = pressed,
                    MouseButton::Middle => self.modifiers.middle_mouse_button = pressed,
                    MouseButton::Right => self.modifiers.right_mouse_button = pressed,
                }
                let action = if pressed {
                    MouseAction::Pressed
                } else {
                    MouseAction::Released
                };
                self.mouse(producer, button, action)
            },
            Event::MouseWheel {
                x, y, direction, ..
            } => {
                let (delta_x, delta_y) =
                    wheel_cef_delta(*x, *y, *direction == MouseWheelDirection::Flipped);
                self.last_wheel = Some((delta_x, delta_y));
                self.mouse(
                    producer,
                    MouseButton::Left,
                    MouseAction::WheelScrolled { delta_x, delta_y },
                )
            },
            Event::KeyDown {
                keycode,
                scancode,
                keymod,
                raw,
                ..
            }
            | Event::KeyUp {
                keycode,
                scancode,
                keymod,
                raw,
                ..
            } => {
                self.modifiers.shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                self.modifiers.ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                self.modifiers.alt = keymod.intersects(Mod::LALTMOD | Mod::RALTMOD);
                self.modifiers.meta = keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD);
                let down = matches!(event, Event::KeyDown { .. });
                let kind = if down {
                    KeyEventKind::RawKeyDown
                } else {
                    KeyEventKind::KeyUp
                };
                let key = crate::keys::translate(&KeyFields {
                    keycode: *keycode,
                    scancode: *scancode,
                    keymod: *keymod,
                    raw: *raw,
                });
                if down {
                    self.last_down_native = key.native_key_code;
                }
                self.last_key = Some(KeyCodes {
                    raw: *raw,
                    // SDL scancodes are USB HID usages (non-negative).
                    usage: scancode.map_or(0, |s| s.to_i32() as u32),
                    windows_key_code: key.windows_key_code,
                    native_key_code: key.native_key_code,
                });
                producer.send_keyboard_input(KeyEvent {
                    kind,
                    windows_key_code: key.windows_key_code,
                    native_key_code: key.native_key_code,
                    character: key.character,
                    modifiers: self.modifiers,
                })
            },
            // Linux: CEF derives a char event's character from `windows_key_code` through an X
            // keysym (browser_platform_delegate_native_linux.cc), so a Unicode value there
            // types the wrong key ('r' = 0x72 is VKEY_F3). Text is committed through the IME
            // path instead, which inserts any Unicode text into the focused editable element
            // (`input`/`beforeinput` fire; no `keypress`).
            #[cfg(target_os = "linux")]
            Event::TextInput { text, .. } => {
                let text: String = bmp_chars(text).collect();
                if text.is_empty() {
                    Ok(())
                } else {
                    producer.ime_commit_text(&text)
                }
            },
            #[cfg(not(target_os = "linux"))]
            Event::TextInput { text, .. } => {
                // No scan code for SDL text on Windows/Linux: 0 is DomCode NONE (`code`
                // empty). macOS builds an NSEvent from it, where 0 is kVK_ANSI_A, so there the
                // key that produced the text is used (`keys::CHAR_NATIVE_FROM_KEY`).
                let native_key_code = if crate::keys::CHAR_NATIVE_FROM_KEY {
                    self.last_down_native
                } else {
                    0
                };
                let mut result = Ok(());
                for ch in bmp_chars(text) {
                    result = producer.send_keyboard_input(KeyEvent {
                        kind: KeyEventKind::Char,
                        windows_key_code: ch as i32,
                        native_key_code,
                        character: Some(ch),
                        modifiers: self.modifiers,
                    });
                    if result.is_err() {
                        break;
                    }
                }
                result
            },
            _ => return None,
        };
        self.count(result)
    }

    /// Per redraw: deferred focus and the page's cursor shape.
    pub(crate) fn per_frame<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        mouse: &sdl3::mouse::MouseUtil,
        has_frame: bool,
    ) -> Option<String> {
        if let Some(shape) = producer.poll_cursor_shape() {
            let hidden = matches!(&shape, CursorShape::Custom(name) if name == "none");
            if hidden != self.cursor_hidden {
                mouse.show_cursor(!hidden);
                self.cursor_hidden = hidden;
            }
            if let Ok(cursor) = Cursor::from_system(cursor_icon(&shape)) {
                cursor.set();
                self.active_cursor = Some(cursor);
            }
        }
        if self.focus_pending && has_frame {
            self.focus_pending = false;
            // welding's `move_focus` is CEF `SetFocus(true)` on every platform; no blur call.
            return self.count(producer.move_focus(FocusDirection::Forward));
        }
        None
    }

    fn count(&mut self, result: Result<(), welding::WeldError>) -> Option<String> {
        match result {
            Ok(()) => {
                self.forwarded += 1;
                None
            },
            Err(error) => {
                self.rejected += 1;
                (self.rejected == 1)
                    .then(|| format!("input rejected (first; later ones only counted): {error}"))
            },
        }
    }

    fn mouse<P: CefSurfaceProducer>(
        &self,
        producer: &mut P,
        button: MouseButton,
        action: MouseAction,
    ) -> Result<(), welding::WeldError> {
        producer.send_mouse_input(MouseEvent {
            x: self.cursor.0,
            y: self.cursor.1,
            button,
            action,
            modifiers: self.modifiers,
        })
    }
}

/// The user-input SDL events `Input` forwards as page input (named for the log); `None` for
/// window and every other event.
fn user_input_name(event: &Event) -> Option<&'static str> {
    Some(match event {
        Event::MouseMotion { .. } => "mouse-motion",
        Event::MouseButtonDown { .. } => "mouse-button-down",
        Event::MouseButtonUp { .. } => "mouse-button-up",
        Event::MouseWheel { .. } => "mouse-wheel",
        Event::KeyDown { .. } => "key-down",
        Event::KeyUp { .. } => "key-up",
        Event::TextInput { .. } => "text-input",
        _ => return None,
    })
}

fn cursor_icon(shape: &CursorShape) -> SystemCursor {
    match shape {
        CursorShape::Pointer | CursorShape::Grab | CursorShape::Grabbing => SystemCursor::Hand,
        CursorShape::Text => SystemCursor::IBeam,
        CursorShape::Wait => SystemCursor::Wait,
        CursorShape::Crosshair => SystemCursor::Crosshair,
        CursorShape::Move | CursorShape::ResizeAll => SystemCursor::SizeAll,
        CursorShape::NotAllowed => SystemCursor::No,
        CursorShape::Progress => SystemCursor::WaitArrow,
        CursorShape::ResizeNs => SystemCursor::SizeNS,
        CursorShape::ResizeEw => SystemCursor::SizeWE,
        CursorShape::ResizeNeSw => SystemCursor::SizeNESW,
        CursorShape::ResizeNwSe => SystemCursor::SizeNWSE,
        _ => SystemCursor::Arrow,
    }
}
