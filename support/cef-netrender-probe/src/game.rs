//! Opt-in real-game mode (`--roves-game-dir=<built game>`): the probe opens an unmodified
//! game build from a per-run loopback origin instead of the deterministic fixture, and
//! forwards SDL3 mouse, wheel and keyboard input to the off-screen browser through
//! welding's `CefSurfaceProducer` input API. Rendering, composition (`--roves-compose`),
//! sandbox and shutdown are the probe's unchanged path; nothing here injects script or
//! exposes any host API to the page.

use std::path::{Path, PathBuf};

use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    mouse::{Cursor, MouseButton as SdlButton, MouseWheelDirection, SystemCursor},
    video::Window,
};
use welding::{
    CefSurfaceProducer as _, CursorShape, EventModifiers, FocusDirection, KeyEvent, KeyEventKind,
    MouseAction, MouseButton, MouseEvent, windows_cef::WindowsCefProducer,
};

use crate::game_rules::{
    InputEventKind, SDLK_SCANCODE_MASK, bmp_chars, fold_sided_modifier, forward_input,
    native_key_code, non_character_virtual_key, wheel_cef_delta,
};

/// The stage-1 window probe's loopback server (axum + tower-http `ServeDir` on 127.0.0.1),
/// reused unchanged so both probes serve the game the same way.
#[path = "../../cef-window-probe/src/server.rs"]
pub(crate) mod server;
pub(crate) use server::Server;

/// Console lines forwarded to the evidence log before only counting them.
const CONSOLE_LOG_CAP: u32 = 500;
/// Longest console message kept per line (characters).
const CONSOLE_MESSAGE_MAX: usize = 512;

pub(crate) struct GameOptions {
    /// Absolute game build directory (contains `index.html`).
    pub(crate) dir: PathBuf,
    /// `--roves-port` (0 = a free port per run; a fixed port keeps the origin, and therefore
    /// the page's storage in a reused `--roves-cache-dir`, stable across runs).
    pub(crate) port: u16,
}

impl GameOptions {
    pub(crate) fn new(dir: &str, port: u16) -> Result<Self, String> {
        // std::path::absolute, not canonicalize: no `\\?\` verbatim prefix (as the window probe).
        let dir = std::path::absolute(dir).map_err(|e| format!("--roves-game-dir={dir}: {e}"))?;
        if !dir.join("index.html").is_file() {
            return Err(format!("{} has no index.html", dir.display()));
        }
        Ok(GameOptions { dir, port })
    }
}

/// The per-producer persistent profile inside CEF's root cache (welding requires it to be
/// absolute and inside `CefRuntimeConfig::cache_path`). A fresh default `--roves-cache-dir`
/// gives every run an empty profile; reusing the directory and the port keeps saves.
pub(crate) fn profile_dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join("game-profile")
}

/// Console forwarding with a line cap.
#[derive(Default)]
pub(crate) struct Console {
    pub(crate) seen: u32,
}

impl Console {
    pub(crate) fn line(
        &mut self,
        level: i32,
        message: &str,
        source: &str,
        line: i32,
    ) -> Option<String> {
        self.seen += 1;
        if self.seen > CONSOLE_LOG_CAP {
            return None;
        }
        let message: String = message.chars().take(CONSOLE_MESSAGE_MAX).collect();
        Some(format!(
            "console level={level} source={source}:{line} message={message:?}"
        ))
    }
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
    cursor_hidden: bool,
    active_cursor: Option<Cursor>,
}

/// What one SDL key down/up became for CEF.
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyCodes {
    /// `SDL_KeyboardEvent.raw` (Windows scan code; 0 when SDL had none, e.g. a pushed event).
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
    pub(crate) fn window_event(
        &mut self,
        producer: &mut WindowsCefProducer,
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
                let vk = keycode.map_or(0, virtual_key);
                let kind = if matches!(event, Event::KeyDown { .. }) {
                    KeyEventKind::RawKeyDown
                } else {
                    KeyEventKind::KeyUp
                };
                // SDL scancodes are USB HID usages (non-negative).
                let usage = scancode.map(|s| s.to_i32() as u32);
                let windows_key_code = fold_sided_modifier(vk as u32) as i32;
                let native_key_code = native_key_code(*raw, usage, mapped_scan_code(vk));
                self.last_key = Some(KeyCodes {
                    raw: *raw,
                    usage: usage.unwrap_or(0),
                    windows_key_code,
                    native_key_code,
                });
                producer.send_keyboard_input(KeyEvent {
                    kind,
                    windows_key_code,
                    native_key_code,
                    character: None,
                    modifiers: self.modifiers,
                })
            },
            Event::TextInput { text, .. } => {
                let mut result = Ok(());
                for ch in bmp_chars(text) {
                    result = producer.send_keyboard_input(KeyEvent {
                        kind: KeyEventKind::Char,
                        windows_key_code: ch as i32,
                        // No scan code for SDL text: 0 is DomCode NONE (`code` empty). The
                        // former 1 was Escape's Windows scan code in Chromium's table.
                        native_key_code: 0,
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
    pub(crate) fn per_frame(
        &mut self,
        producer: &mut WindowsCefProducer,
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
            // welding's Windows `move_focus` is CEF `SetFocus(true)`; there is no blur call.
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

    fn mouse(
        &self,
        producer: &mut WindowsCefProducer,
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
