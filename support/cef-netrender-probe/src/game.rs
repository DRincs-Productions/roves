//! Opt-in real-game mode (`--roves-game-dir=<built game>`): the probe opens an unmodified
//! game build from a per-run loopback origin instead of the deterministic fixture, and
//! forwards winit mouse, wheel and keyboard input to the off-screen browser through
//! welding's `CefSurfaceProducer` input API. Rendering, composition (`--roves-compose`),
//! sandbox and shutdown are the probe's unchanged path; nothing here injects script or
//! exposes any host API to the page.

use std::path::{Path, PathBuf};

use welding::{
    CefSurfaceProducer as _, CursorShape, EventModifiers, FocusDirection, KeyEvent, KeyEventKind,
    MouseAction, MouseButton, MouseEvent, windows_cef::WindowsCefProducer,
};
use winit::{
    event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent},
    keyboard::PhysicalKey,
    platform::scancode::PhysicalKeyExtScancode as _,
    window::{CursorIcon, Window},
};

use crate::game_rules::{
    KeyMessage, WHEEL_DELTA_PER_LINE, bmp_chars, fold_sided_modifier, native_key_code,
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

/// winit -> welding input state. Coordinates are physical pixels; welding converts them to
/// CEF's DIP with the producer's scale factor.
#[derive(Default)]
pub(crate) struct Input {
    modifiers: EventModifiers,
    cursor: (i32, i32),
    /// Set by `Focused(true)` and at startup; applied once a frame proved the browser exists.
    focus_pending: bool,
    /// Input calls welding refused (no browser yet, or closed). Only the first is logged.
    pub(crate) rejected: u32,
    pub(crate) forwarded: u64,
    cursor_hidden: bool,
}

impl Input {
    pub(crate) fn new() -> Self {
        Input {
            focus_pending: true,
            ..Default::default()
        }
    }

    /// Forwards one window event. Returns a log line for the first rejected call.
    pub(crate) fn window_event(
        &mut self,
        producer: &mut WindowsCefProducer,
        event: &WindowEvent,
    ) -> Option<String> {
        let result = match event {
            WindowEvent::Focused(true) => {
                self.focus_pending = true;
                return None;
            },
            WindowEvent::Occluded(occluded) => producer.set_visible(!occluded),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                producer.set_scale_factor(*scale_factor as f32)
            },
            WindowEvent::ModifiersChanged(m) => {
                let state = m.state();
                self.modifiers.shift = state.shift_key();
                self.modifiers.ctrl = state.control_key();
                self.modifiers.alt = state.alt_key();
                self.modifiers.meta = state.super_key();
                return None;
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                self.mouse(producer, MouseButton::Left, MouseAction::Moved)
            },
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Middle => MouseButton::Middle,
                    WinitMouseButton::Right => MouseButton::Right,
                    _ => return None, // Back/Forward: no welding equivalent.
                };
                let pressed = *state == ElementState::Pressed;
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
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        (x * WHEEL_DELTA_PER_LINE) as i32,
                        (y * WHEEL_DELTA_PER_LINE) as i32,
                    ),
                    MouseScrollDelta::PixelDelta(d) => (d.x as i32, d.y as i32),
                };
                self.mouse(
                    producer,
                    MouseButton::Left,
                    MouseAction::WheelScrolled { delta_x, delta_y },
                )
            },
            WindowEvent::KeyboardInput { event, .. } => self.key(producer, event),
            _ => return None,
        };
        self.count(result)
    }

    /// Per redraw: deferred focus and the page's cursor shape.
    pub(crate) fn per_frame(
        &mut self,
        producer: &mut WindowsCefProducer,
        window: &Window,
        has_frame: bool,
    ) -> Option<String> {
        if let Some(shape) = producer.poll_cursor_shape() {
            let hidden = matches!(&shape, CursorShape::Custom(name) if name == "none");
            if hidden != self.cursor_hidden {
                window.set_cursor_visible(!hidden);
                self.cursor_hidden = hidden;
            }
            window.set_cursor(cursor_icon(&shape));
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

    fn key(
        &self,
        producer: &mut WindowsCefProducer,
        event: &winit::event::KeyEvent,
    ) -> Result<(), welding::WeldError> {
        let scancode = event.physical_key.to_scancode();
        let vk = match event.physical_key {
            PhysicalKey::Code(_) => scancode.map_or(0, virtual_key),
            PhysicalKey::Unidentified(_) => 0,
        };
        let send = |producer: &mut WindowsCefProducer, kind, code, native, character| {
            producer.send_keyboard_input(KeyEvent {
                kind,
                windows_key_code: code,
                native_key_code: native,
                character,
                modifiers: self.modifiers,
            })
        };
        if event.state == ElementState::Released {
            let native = native_key_code(scancode, KeyMessage::Up);
            return send(producer, KeyEventKind::KeyUp, vk, native, None);
        }
        let native = native_key_code(
            scancode,
            KeyMessage::Down {
                repeat: event.repeat,
            },
        );
        // Win32 order: WM_KEYDOWN, then one WM_CHAR per produced character (wParam = the
        // character), which is what CEF's own Windows OSR sample forwards.
        send(producer, KeyEventKind::RawKeyDown, vk, native, None)?;
        if let Some(text) = event.text.as_deref() {
            for ch in bmp_chars(text) {
                send(producer, KeyEventKind::Char, ch as i32, native, Some(ch))?;
            }
        }
        Ok(())
    }
}

/// Layout-aware virtual key for a winit Windows scancode, from the OS's own mapping table.
fn virtual_key(scancode: u32) -> i32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK_EX, MapVirtualKeyW};
    // SAFETY: plain value-in/value-out Win32 call; extended scancodes carry 0xE0/0xE1 in the
    // high byte, which MAPVK_VSC_TO_VK_EX accepts since Windows Vista.
    let vk = unsafe { MapVirtualKeyW(scancode, MAPVK_VSC_TO_VK_EX) };
    fold_sided_modifier(vk) as i32
}

fn cursor_icon(shape: &CursorShape) -> CursorIcon {
    match shape {
        CursorShape::Default | CursorShape::Custom(_) => CursorIcon::Default,
        CursorShape::Pointer => CursorIcon::Pointer,
        CursorShape::Text => CursorIcon::Text,
        CursorShape::Wait => CursorIcon::Wait,
        CursorShape::Crosshair => CursorIcon::Crosshair,
        CursorShape::Move => CursorIcon::Move,
        CursorShape::NotAllowed => CursorIcon::NotAllowed,
        CursorShape::Help => CursorIcon::Help,
        CursorShape::Progress => CursorIcon::Progress,
        CursorShape::ResizeNs => CursorIcon::NsResize,
        CursorShape::ResizeEw => CursorIcon::EwResize,
        CursorShape::ResizeNeSw => CursorIcon::NeswResize,
        CursorShape::ResizeNwSe => CursorIcon::NwseResize,
        CursorShape::ResizeAll => CursorIcon::AllScroll,
        CursorShape::Grab => CursorIcon::Grab,
        CursorShape::Grabbing => CursorIcon::Grabbing,
        CursorShape::ZoomIn => CursorIcon::ZoomIn,
        CursorShape::ZoomOut => CursorIcon::ZoomOut,
        // `#[non_exhaustive]`: shapes added by a later welding fall back to the default.
        _ => CursorIcon::Default,
    }
}
