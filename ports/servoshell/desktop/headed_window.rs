/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! An SDL3 window implementation.

#![deny(clippy::panic)]
#![deny(clippy::unwrap_used)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use euclid::{Angle, Length, Point2D, Rect, Rotation3D, Scale, Size2D, UnknownUnit, Vector3D};
use keyboard_types::ShortcutMatcher;
use log::{debug, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
use servo::{
    AuthenticationRequest, BluetoothDeviceSelectionRequest, Cursor, DeviceIndependentIntRect,
    DeviceIndependentPixel, DeviceIntPoint, DeviceIntRect, DeviceIntSize, DevicePixel, DevicePoint,
    EmbedderControl, EmbedderControlId, ImeEvent, InputEvent, InputEventId, InputEventResult,
    InputMethodControl, Key, KeyState, KeyboardEvent, Modifiers, MouseButton as ServoMouseButton,
    MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, MouseMoveEvent, NamedKey,
    OffscreenRenderingContext, PermissionRequest, RenderingContext, ScreenGeometry, Theme,
    TouchEvent, TouchEventType, TouchId, TouchPointerType, WebRenderDebugOption, WebView,
    WebViewId, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
    convert_rect_to_css_pixel,
};
use url::Url;
use dpi::PhysicalSize;
use sdl3::mouse::{Cursor as SdlCursor, MouseButton as SdlMouseButton, MouseUtil, SystemCursor};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use sdl3::{pixels::PixelFormat, surface::Surface};
use sdl3::video::WindowFlags;
#[cfg(target_os = "macos")]
use {
    objc2_app_kit::{NSColorSpace, NSView},
    objc2_foundation::MainThreadMarker,
};

use super::keyutils::keyboard_event_from_sdl;
use crate::desktop::accelerated_gl_media::setup_gl_accelerated_media;
use crate::desktop::dialog::Dialog;
use crate::desktop::event_loop::{
    ActiveEventLoop, AppEvent, EventLoopProxy, TouchPhase, WindowEvent, WindowId,
};
use crate::desktop::gui::Gui;
use crate::desktop::keyutils::CMD_OR_CONTROL;
use crate::desktop::logging;
use crate::prefs::ServoShellPreferences;
use crate::running_app_state::{RunningAppState, UserInterfaceCommand};
use crate::window::{
    LINE_HEIGHT, LINE_WIDTH, MIN_WINDOW_INNER_SIZE, PlatformWindow, ServoShellWindow,
    ServoShellWindowId,
};

pub(crate) const INITIAL_WINDOW_TITLE: &str = "Roves";

fn current_sdl_theme() -> Theme {
    match sdl3::VideoSubsystem::get_system_theme() {
        sdl3::video::SystemTheme::Dark => Theme::Dark,
        sdl3::video::SystemTheme::Light | sdl3::video::SystemTheme::Unknown => Theme::Light,
    }
}

/// How often the boot splash's indeterminate progress-bar animation (`gui.rs`'s
/// `draw_splash_progress_bar`) advances, whether it's holding for `app.rs`'s
/// `MIN_SPLASH_DURATION`/waiting on boot extraction, or covering a still-loading real
/// page (see `page_load_splash_since`, `splash_animation_wake_deadline`, and
/// `App::try_finish_booting`). ~30fps: smooth enough to read as animated, without waking
/// the event loop needlessly often during what's meant to be a brief, incidental wait.
pub(crate) const SPLASH_ANIMATION_TICK: Duration = Duration::from_millis(33);

/// How long the boot splash keeps covering the real page after `App::finish_init` opens
/// it (see `page_load_splash_since`), if the initial page's `LoadStatus::Complete` never
/// arrives -- a broken load, or a page that's still loading subresources it doesn't
/// consider blocking. Matches this codebase's existing "never leave the user stuck on a
/// splash forever" philosophy (see `bundle_launch.rs`'s boot-extraction-failure
/// handling) -- past this, showing the real page, even mid-load, beats an indefinite
/// Roves-branded hang.
const MAX_PAGE_LOAD_SPLASH_DURATION: Duration = Duration::from_secs(8);

pub struct HeadedWindow {
    /// The egui interface that is responsible for showing the user interface elements of
    /// this headed `Window`.
    gui: RefCell<Gui>,
    screen_size: Cell<Size2D<u32, DeviceIndependentPixel>>,
    last_theme: Cell<Theme>,
    webview_relative_mouse_point: Cell<Point2D<f32, DevicePixel>>,
    /// The inner size of the window in physical pixels which excludes OS decorations.
    /// It equals viewport size + (0, toolbar height).
    inner_size: Cell<PhysicalSize<u32>>,
    fullscreen: Cell<bool>,
    /// Where `set_fullscreen` persists fullscreen state across launches (a marker file
    /// named `fullscreen`, written when entering fullscreen and removed when leaving it —
    /// see `prefs.rs`'s `start_fullscreen`). `None` disables persistence entirely (matches
    /// `ServoShellPreferences::config_dir`, e.g. when the config dir couldn't be resolved).
    config_dir: Option<PathBuf>,
    device_pixel_ratio_override: Option<f32>,
    xr_window_poses: RefCell<Vec<Rc<XRWindowPose>>>,
    /// The `RenderingContext` of Servo itself. This is used to render Servo results
    /// temporarily until they can be blitted into the egui scene.
    rendering_context: Rc<OffscreenRenderingContext>,
    /// The RenderingContext that renders directly onto the Window. This is used as
    /// the target of egui rendering and also where Servo rendering results are finally
    /// blitted.
    window_rendering_context: Rc<WindowRenderingContext>,
    /// A helper that simulates touch events when the `--simulate-touch-events` flag
    /// is enabled.
    touch_event_simulator: Option<TouchEventSimulator>,
    /// Keyboard events that have been sent to Servo that have still not been handled yet.
    /// When these are handled, they will optionally be used to trigger keybindings that
    /// are overridable by web content.
    pending_keyboard_events: RefCell<HashMap<InputEventId, KeyboardEvent>>,
    // Keep this as the last field of the struct to ensure that the rendering context is
    // dropped first.
    // (https://github.com/servo/servo/issues/36711)
    sdl_window: sdl3::video::Window,
    /// Sends `AppEvent::RedrawRequested` from anywhere, without the caller needing its own
    /// `&ActiveEventLoop` handle — SDL3 has no push-based redraw-request event of its own (see
    /// `request_redraw`, and `event_loop.rs`'s own doc comment on that `AppEvent` variant).
    event_loop_proxy: EventLoopProxy,
    /// SDL3 text-input controller for the video subsystem that owns `sdl_window`.
    text_input: sdl3::keyboard::TextInputUtil,
    /// SDL's cursor visibility API is global to the mouse subsystem. The active cursor object
    /// must be retained for as long as SDL may use its native handle.
    mouse: MouseUtil,
    active_cursor: RefCell<Option<SdlCursor>>,
    /// The last title set on this window. We need to store this value here, as `winit::Window::title`
    /// is not supported very many platforms.
    last_title: RefCell<String>,
    /// A fixed title overriding the default of mirroring the active page's own title —
    /// see `ServoShellPreferences::window_title_override` and
    /// `update_user_interface_state`.
    window_title_override: Option<String>,
    /// The current set of open dialogs.
    dialogs: RefCell<HashMap<WebViewId, Vec<Dialog>>>,
    /// The [`EmbedderControlId`] of the currently showing [`InputMethod`] interfaces,
    /// if one is showing.
    visible_input_method: Cell<Option<EmbedderControlId>>,
    /// Whether a synthetic Servo composition Start has been emitted for the current SDL IME.
    ime_composing: Cell<bool>,
    /// The position of the mouse cursor after the most recent `MouseMove` event.
    last_mouse_position: Cell<Option<Point2D<f32, DeviceIndependentPixel>>>,
    /// Stable Servo identifiers for SDL3's 64-bit finger ids. Keeping an explicit map avoids
    /// truncating ids to Servo's i32 `TouchId` and releases entries on Up/Cancel.
    active_touch_ids: RefCell<HashMap<u64, TouchId>>,
    next_touch_id: Cell<i32>,
    /// Set by `App::finish_init` (see `app.rs`) the moment the real `WebView` opens, and
    /// cleared once its initial page reaches `LoadStatus::Complete` or
    /// `MAX_PAGE_LOAD_SPLASH_DURATION` elapses, whichever comes first -- see
    /// `begin_page_load_splash`, `splash_animation_wake_deadline`, and
    /// `RunningAppState::is_initial_page_loaded`. While set, `handle_winit_window_event`
    /// keeps painting the boot splash instead of compositing the real page: the page's
    /// own "nothing painted yet" clear color is black (see CUSTOMIZATIONS.md's boot-
    /// splash entries), and without this, that black frame is exactly what showed
    /// through for however long the page took to paint anything of its own -- read by a
    /// user as "splash, then a black screen, then the game".
    page_load_splash_since: Cell<Option<Instant>>,
}

impl HeadedWindow {
    /// SDL3-backed desktop window construction. Linux taskbar app-id naming is still a separate
    /// follow-up; icon selection and transparent/borderless `no_native_titlebar` windows are
    /// handled below, while macOS keeps its platform-specific sRGB correction.
    #[servo::servo_tracing::instrument(level = "debug", name = "HeadedWindow::new", skip_all)]
    pub(crate) fn new(
        servoshell_preferences: &ServoShellPreferences,
        event_loop: &ActiveEventLoop,
        event_loop_proxy: EventLoopProxy,
        initial_url: Url,
    ) -> Rc<Self> {
        let no_native_titlebar = servoshell_preferences.no_native_titlebar;
        let inner_size = servoshell_preferences.initial_window_size;

        let mut window_builder =
            event_loop
                .video()
                .window(INITIAL_WINDOW_TITLE, inner_size.width, inner_size.height);
        window_builder
            .opengl()
            .resizable()
            .high_pixel_density()
            // Must be invisible at startup, same reasoning the old winit code had for
            // `accesskit_winit` (now gone, see TODO.md) -- there is no content worth showing
            // before the very first splash frame is painted, further down this function.
            .hidden();
        if servoshell_preferences.start_fullscreen {
            // Reopen already in fullscreen if that's how the game was last left running (see
            // `prefs.rs`'s `start_fullscreen`/`set_fullscreen`'s persistence below) — avoids a
            // visible windowed-then-fullscreen transition on startup. See CUSTOMIZATIONS.md.
            window_builder.fullscreen();
        }
        if no_native_titlebar {
            let flags =
                window_builder.flags() | WindowFlags::BORDERLESS | WindowFlags::TRANSPARENT;
            window_builder.set_flags(flags);
        }

        let mut sdl_window = window_builder.build().expect("Failed to create window.");
        let _ = sdl_window.set_minimum_size(
            MIN_WINDOW_INNER_SIZE.width as u32,
            MIN_WINDOW_INNER_SIZE.height as u32,
        );

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        set_window_icon(&mut sdl_window);

        let window_handle = sdl_window
            .window_handle()
            .expect("SDL3 window did not have a window handle");
        HeadedWindow::force_srgb_color_space(window_handle.as_raw());

        let (screen_size, screen_scale): (Size2D<u32, DevicePixel>, f64) = servoshell_preferences
            .screen_size_override
            .map_or_else(
                || {
                    let bounds = event_loop
                        .video()
                        .get_primary_display()
                        .and_then(|display| display.get_bounds())
                        .unwrap_or(sdl3::rect::Rect::new(0, 0, inner_size.width as u32, inner_size.height as u32));
                    (
                        Size2D::new(bounds.width(), bounds.height()),
                        sdl_window.display_scale() as f64,
                    )
                },
                |size| (Size2D::new(size.width, size.height), 1.0),
            );
        let screen_scale: Scale<f64, DeviceIndependentPixel, DevicePixel> =
            Scale::new(screen_scale);
        let screen_size = (screen_size.to_f64() / screen_scale).to_u32();
        let (inner_width, inner_height) = sdl_window.size_in_pixels();
        let inner_size = PhysicalSize::new(inner_width, inner_height);

        let display_handle = sdl_window
            .display_handle()
            .expect("could not get display handle from window");
        let window_handle = sdl_window
            .window_handle()
            .expect("could not get window handle from window");
        let window_rendering_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, inner_size)
                .expect("Could not create RenderingContext for Window"),
        );

        // Setup for GL accelerated media handling. This is only active on certain Linux platforms
        // and Windows.
        {
            let details = window_rendering_context.surfman_details();
            setup_gl_accelerated_media(details.0, details.1);
        }

        // Make sure the gl context is made current.
        window_rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");

        let rendering_context = Rc::new(window_rendering_context.offscreen_context(inner_size));
        let gui = RefCell::new(Gui::new(
            &sdl_window,
            event_loop_proxy.clone(),
            rendering_context.clone(),
            initial_url,
        ));

        debug!("Created window {:?}", sdl_window.id());
        Rc::new(HeadedWindow {
            gui,
            sdl_window,
            event_loop_proxy,
            text_input: event_loop.video().text_input(),
            mouse: event_loop.sdl().mouse(),
            active_cursor: RefCell::new(None),
            webview_relative_mouse_point: Cell::new(Point2D::zero()),
            fullscreen: Cell::new(servoshell_preferences.start_fullscreen),
            config_dir: servoshell_preferences.config_dir.clone(),
            inner_size: Cell::new(inner_size),
            screen_size: Cell::new(screen_size),
            last_theme: Cell::new(current_sdl_theme()),
            device_pixel_ratio_override: servoshell_preferences.device_pixel_ratio_override,
            xr_window_poses: RefCell::new(vec![]),
            window_rendering_context,
            touch_event_simulator: servoshell_preferences
                .simulate_touch_events
                .then(Default::default),
            pending_keyboard_events: Default::default(),
            rendering_context,
            last_title: RefCell::new(String::from(INITIAL_WINDOW_TITLE)),
            window_title_override: servoshell_preferences.window_title_override.clone(),
            dialogs: Default::default(),
            visible_input_method: Default::default(),
            ime_composing: Cell::new(false),
            last_mouse_position: Default::default(),
            active_touch_ids: Default::default(),
            next_touch_id: Cell::new(0),
            page_load_splash_since: Cell::new(None),
        })
    }

    /// The raw SDL3 window id (`event_loop.rs`'s own `WindowId` type) — distinct from
    /// `PlatformWindow::id`'s `ServoShellWindowId` (this fork's own cross-platform window
    /// identity), named differently on purpose so `.id()` never becomes ambiguous between
    /// this inherent method and that trait one.
    pub(crate) fn sdl_window_id(&self) -> WindowId {
        self.sdl_window.id()
    }

    pub(crate) fn sdl_window(&self) -> &sdl3::video::Window {
        &self.sdl_window
    }

    /// SDL3 has no push-based "redraw requested" event of its own — see `event_loop.rs`'s own
    /// doc comment on `AppEvent::RedrawRequested`, which this sends.
    pub(crate) fn request_redraw(&self) {
        let _ = self
            .event_loop_proxy
            .send_event(AppEvent::RedrawRequested(self.sdl_window_id()));
    }

    /// Paints the boot splash instead of the normal browser UI — used both while a
    /// packed-content launch's boot extraction is still running on a background thread
    /// (`AppState::Booting` in `app.rs`, before there's a `RunningAppState`/webview to
    /// draw at all), and afterward, while `page_load_splash_since` is set, covering the
    /// real (still-loading) page. `elapsed` — time since whichever of those two waits
    /// began — drives the splash's indeterminate progress-bar animation (see
    /// `Gui::update_splash`/`draw_splash_progress_bar`); it isn't a completion fraction,
    /// since neither wait has one worth showing (a boot extraction is almost always one
    /// or two packs and finishes near-instantly once it starts; a page load has no
    /// fractional signal at all) — see `draw_splash_progress_bar`'s own doc comment.
    pub(crate) fn paint_splash(&self, elapsed: Duration) {
        let mut gui = self.gui.borrow_mut();
        gui.update_splash(&self.sdl_window, elapsed);
        gui.paint(&self.sdl_window);
    }

    /// Paints a visible error message instead of handing off to the (in this case
    /// broken) real page — used once the initial page has finished loading but a
    /// content-load error was recorded along the way (see `logging::content_load_error`)
    /// meaning the page's own script never actually ran. Without this, that case reads
    /// as a silent black window: `LoadStatus::Complete` still fires normally (navigation
    /// itself did complete), so the boot splash comes down same as any successful load,
    /// revealing WebRender's plain black default background since nothing the page
    /// would have painted ever ran.
    pub(crate) fn paint_content_load_error(&self, message: &str) {
        let mut gui = self.gui.borrow_mut();
        gui.update_content_load_error(&self.sdl_window, message);
        gui.paint(&self.sdl_window);
    }

    /// Called once by `App::finish_init`, right as the real `WebView` opens, to keep the
    /// boot splash up (see `page_load_splash_since`) instead of immediately handing off
    /// to a page that hasn't painted anything of its own yet.
    pub(crate) fn begin_page_load_splash(&self) {
        self.page_load_splash_since.set(Some(Instant::now()));
    }

    /// `Some(deadline)` while the boot splash is still covering this window's real
    /// (still-loading) page (see `page_load_splash_since`), so `App::window_event`/
    /// `new_events` can keep the winit event loop ticking at `SPLASH_ANIMATION_TICK`
    /// instead of going fully idle. Without this, once the last real `WindowEvent`/
    /// repaint request had been handled, nothing would wake the loop up again to
    /// animate the splash during a long, otherwise-silent load, or to promptly notice
    /// `LoadStatus::Complete` firing with no further page activity after it.
    pub(crate) fn splash_animation_wake_deadline(&self, state: &RunningAppState) -> Option<Instant> {
        let since = self.page_load_splash_since.get()?;
        if state.is_initial_page_loaded() || since.elapsed() >= MAX_PAGE_LOAD_SPLASH_DURATION {
            return None;
        }
        Some(Instant::now() + SPLASH_ANIMATION_TICK)
    }

    fn handle_keyboard_input(
        &self,
        state: Rc<RunningAppState>,
        window: &Rc<ServoShellWindow>,
        keyboard_event: KeyboardEvent,
    ) {
        for pose in self.xr_window_poses.borrow().iter() {
            pose.handle_xr_rotation(&keyboard_event);
            pose.handle_xr_translation(&keyboard_event);
        }

        // First, handle servoshell key bindings that are not overridable by, or visible to, the page.
        if self.handle_intercepted_key_bindings(state, window, &keyboard_event) {
            return;
        }

        // Then we deliver character and keyboard events to the page in the active webview.
        let Some(webview) = window.active_webview() else {
            return;
        };

        let id = webview.notify_input_event(InputEvent::Keyboard(keyboard_event.clone()));
        self.pending_keyboard_events
            .borrow_mut()
            .insert(id, keyboard_event);
    }

    /// Helper function to handle a click
    fn handle_mouse_button_event(&self, webview: &WebView, button: SdlMouseButton, pressed: bool) {
        // `point` can be outside viewport, such as at toolbar with negative y-coordinate.
        let point = self.webview_relative_mouse_point.get();
        let webview_rect: Rect<_, _> = webview.size().into();
        if !webview_rect.contains(point) {
            return;
        }

        if self
            .touch_event_simulator
            .as_ref()
            .is_some_and(|touch_event_simulator| {
                touch_event_simulator
                    .maybe_consume_move_button_event(webview, button, pressed, point)
            })
        {
            return;
        }

        let mouse_button = match button {
            SdlMouseButton::Left => ServoMouseButton::Left,
            SdlMouseButton::Right => ServoMouseButton::Right,
            SdlMouseButton::Middle => ServoMouseButton::Middle,
            SdlMouseButton::X1 => ServoMouseButton::Back,
            SdlMouseButton::X2 => ServoMouseButton::Forward,
            SdlMouseButton::Unknown => ServoMouseButton::Other(0),
        };

        let action = if pressed { MouseButtonAction::Down } else { MouseButtonAction::Up };

        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
            action,
            mouse_button,
            point.into(),
        )));
    }

    /// Helper function to handle mouse move events.
    fn handle_mouse_move_event(&self, webview: &WebView, x: f32, y: f32) {
        let mut point = Point2D::<f32, DevicePixel>::new(x, y);
        point.y -= (self.toolbar_height() * self.hidpi_scale_factor()).0;

        let previous_point = self.webview_relative_mouse_point.get();
        self.webview_relative_mouse_point.set(point);

        let webview_rect: Rect<_, _> = webview.size().into();
        if !webview_rect.contains(point) {
            if webview_rect.contains(previous_point) {
                webview.notify_input_event(InputEvent::MouseLeftViewport(
                    MouseLeftViewportEvent::default(),
                ));
            }
            return;
        }

        if self
            .touch_event_simulator
            .as_ref()
            .is_some_and(|touch_event_simulator| {
                touch_event_simulator.maybe_consume_mouse_move_event(webview, point)
            })
        {
            return;
        }

        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(point.into())));
    }

    fn handle_touch_event(
        &self,
        webview: &WebView,
        phase: TouchPhase,
        finger_id: u64,
        x: f32,
        y: f32,
    ) {
        let mut active_ids = self.active_touch_ids.borrow_mut();
        let touch_id = active_ids.get(&finger_id).copied().unwrap_or_else(|| {
            let touch_id = TouchId(self.next_touch_id.get());
            self.next_touch_id.set(self.next_touch_id.get().wrapping_add(1));
            active_ids.insert(finger_id, touch_id);
            touch_id
        });
        if matches!(phase, TouchPhase::Up | TouchPhase::Cancel) {
            active_ids.remove(&finger_id);
        }
        drop(active_ids);

        let (width, height) = self.sdl_window.size_in_pixels();
        let mut point = Point2D::<f32, DevicePixel>::new(x * width as f32, y * height as f32);
        point.y -= (self.toolbar_height() * self.hidpi_scale_factor()).0;
        let event_type = match phase {
            TouchPhase::Down => TouchEventType::Down,
            TouchPhase::Move => TouchEventType::Move,
            TouchPhase::Up => TouchEventType::Up,
            TouchPhase::Cancel => TouchEventType::Cancel,
        };
        webview.notify_input_event(InputEvent::Touch(TouchEvent::new(
            event_type,
            touch_id,
            point.into(),
            TouchPointerType::Touch,
        )));
    }

    /// Handle key events before sending them to Servo.
    fn handle_intercepted_key_bindings(
        &self,
        state: Rc<RunningAppState>,
        window: &Rc<ServoShellWindow>,
        key_event: &KeyboardEvent,
    ) -> bool {
        let Some(active_webview) = window.active_webview() else {
            return false;
        };

        let mut handled = true;
        ShortcutMatcher::from_event(key_event.event.clone())
            .shortcut(CMD_OR_CONTROL, 'W', || {
                window.close_webview(active_webview.id());
            })
            .shortcut(CMD_OR_CONTROL, 'P', || {
                let rate = env::var("SAMPLING_RATE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10);
                let duration = env::var("SAMPLING_DURATION")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10);
                active_webview.toggle_sampling_profiler(
                    Duration::from_millis(rate),
                    Duration::from_secs(duration),
                );
            })
            .shortcut(CMD_OR_CONTROL, 'X', || {
                active_webview
                    .notify_input_event(InputEvent::EditingAction(servo::EditingActionEvent::Cut));
            })
            .shortcut(CMD_OR_CONTROL, 'C', || {
                active_webview
                    .notify_input_event(InputEvent::EditingAction(servo::EditingActionEvent::Copy));
            })
            .shortcut(CMD_OR_CONTROL, 'V', || {
                active_webview.notify_input_event(InputEvent::EditingAction(
                    servo::EditingActionEvent::Paste,
                ));
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::F9), || {
                active_webview.capture_webrender();
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::F10), || {
                active_webview.toggle_webrender_debugging(WebRenderDebugOption::RenderTargetDebug);
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::F11), || {
                active_webview.toggle_webrender_debugging(WebRenderDebugOption::TextureCacheDebug);
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::F12), || {
                active_webview.toggle_webrender_debugging(WebRenderDebugOption::Profiler);
            })
            // Back/forward history navigation (Cmd/Ctrl+Alt+Left/Right, Cmd/Ctrl+[/]) is
            // intentionally not bound to a shortcut here: a player navigating "back" out of
            // the game's page can silently destroy in-game state.
            .optional_shortcut(
                self.get_fullscreen(),
                Modifiers::empty(),
                Key::Named(NamedKey::Escape),
                || active_webview.exit_fullscreen(),
            )
            // Select the first 8 tabs via shortcuts
            .shortcut(CMD_OR_CONTROL, '1', || window.activate_webview_by_index(0))
            .shortcut(CMD_OR_CONTROL, '2', || window.activate_webview_by_index(1))
            .shortcut(CMD_OR_CONTROL, '3', || window.activate_webview_by_index(2))
            .shortcut(CMD_OR_CONTROL, '4', || window.activate_webview_by_index(3))
            .shortcut(CMD_OR_CONTROL, '5', || window.activate_webview_by_index(4))
            .shortcut(CMD_OR_CONTROL, '6', || window.activate_webview_by_index(5))
            .shortcut(CMD_OR_CONTROL, '7', || window.activate_webview_by_index(6))
            .shortcut(CMD_OR_CONTROL, '8', || window.activate_webview_by_index(7))
            // Cmd/Ctrl 9 is a bit different in that it focuses the last tab instead of the 9th
            .shortcut(CMD_OR_CONTROL, '9', || {
                let len = window.webviews().len();
                if len > 0 {
                    window.activate_webview_by_index(len - 1)
                }
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::PageDown), || {
                if let Some(index) = window.get_active_webview_index() {
                    window.activate_webview_by_index((index + 1) % window.webviews().len())
                }
            })
            .shortcut(Modifiers::CONTROL, Key::Named(NamedKey::PageUp), || {
                if let Some(index) = window.get_active_webview_index() {
                    let len = window.webviews().len();
                    window.activate_webview_by_index((index + len - 1) % len);
                }
            })
            .shortcut(CMD_OR_CONTROL, 'T', || {
                window.create_and_activate_toplevel_webview(
                    state.clone(),
                    Url::parse("servo:newtab")
                        .expect("Should be able to unconditionally parse 'servo:newtab' as URL"),
                );
            })
            .shortcut(CMD_OR_CONTROL, 'Q', || state.schedule_exit())
            .otherwise(|| handled = false);
        handled
    }

    #[cfg_attr(not(target_os = "macos"), expect(unused_variables))]
    fn force_srgb_color_space(window_handle: RawWindowHandle) {
        #[cfg(target_os = "macos")]
        {
            if let RawWindowHandle::AppKit(handle) = window_handle {
                assert!(MainThreadMarker::new().is_some());
                unsafe {
                    let view = handle.ns_view.cast::<NSView>().as_ref();
                    view.window()
                        .expect("Should have a window")
                        .setColorSpace(Some(&NSColorSpace::sRGBColorSpace()));
                }
            }
        }
    }

    /// TODO(SDL3 windowing): IME positioning not ported yet — SDL3 exposes this through
    /// `SDL_SetTextInputArea`/`SDL_StartTextInput` (global, not a `Window` method the way
    /// winit's `set_ime_allowed`/`set_ime_cursor_area` were), not yet wired up here. The IME
    /// still works (the OS just doesn't get a cursor-area hint, so its candidate window may
    /// not appear right at the text caret) — `visible_input_method` bookkeeping (used by
    /// `WindowEvent::Ime(Disabled)` handling, itself not ported yet either) is kept for when
    /// that's ported.
    fn show_ime(&self, control_id: EmbedderControlId, input_method: InputMethodControl) {
        self.visible_input_method.set(Some(control_id));
        self.ime_composing.set(false);

        let position = input_method.position();
        let rect = sdl3::rect::Rect::new(
            position.min.x,
            position.min.y + self.toolbar_height().0 as i32,
            (position.max.x - position.min.x).max(1) as u32,
            (position.max.y - position.min.y).max(1) as u32,
        );
        self.text_input.set_rect(&self.sdl_window, rect, 0);
        self.text_input.start(&self.sdl_window);
    }

    pub(crate) fn for_each_active_dialog(
        &self,
        window: &ServoShellWindow,
        callback: impl Fn(&mut Dialog) -> bool,
    ) {
        let Some(active_webview) = window.active_webview() else {
            return;
        };
        let mut dialogs = self.dialogs.borrow_mut();
        let Some(dialogs) = dialogs.get_mut(&active_webview.id()) else {
            return;
        };
        if dialogs.is_empty() {
            return;
        }

        // If a dialog is open, clear any Servo cursor. TODO: This should restore the
        // cursor too, when all dialogs close. In general, we need a better cursor
        // management strategy.
        self.set_cursor(Cursor::Default);
        dialogs.retain_mut(callback);
    }

    fn add_dialog(&self, webview_id: WebViewId, dialog: Dialog) {
        self.dialogs
            .borrow_mut()
            .entry(webview_id)
            .or_default()
            .push(dialog)
    }

    /// The `AppEvent::SaveFileDialog` side of `App::user_event`'s handling for it — see that
    /// variant's own doc comment. Unlike every other `add_dialog` call site in this file,
    /// this one doesn't originate from an `EmbedderControl`/script-thread request tied to a
    /// specific `WebViewId` — it comes from `protocols/roves.rs`'s `save_file` command, which
    /// has no such id at all (a custom protocol handler isn't scoped to one webview the way a
    /// DOM `<input type="file">` is), so the caller resolves one itself (this fork's usual
    /// single-window/single-webview kiosk setup, see ../../CUSTOMIZATIONS.md, means there's
    /// normally exactly one sensible choice anyway).
    pub(crate) fn show_save_file_dialog(
        &self,
        webview_id: WebViewId,
        suggested_name: String,
        data: Vec<u8>,
        response: tokio::sync::oneshot::Sender<Result<(), String>>,
    ) {
        self.add_dialog(
            webview_id,
            Dialog::new_save_file_dialog(suggested_name, data, response),
        );
    }

    fn remove_dialog(&self, webview_id: WebViewId, embedder_control_id: EmbedderControlId) {
        let mut dialogs = self.dialogs.borrow_mut();
        if let Some(dialogs) = dialogs.get_mut(&webview_id) {
            dialogs.retain(|dialog| dialog.embedder_control_id() != Some(embedder_control_id));
        }
        dialogs.retain(|_, dialogs| !dialogs.is_empty());
    }

    fn has_active_dialog_for_webview(&self, webview_id: WebViewId) -> bool {
        // First lazily clean up any empty dialog vectors.
        let mut dialogs = self.dialogs.borrow_mut();
        dialogs.retain(|_, dialogs| !dialogs.is_empty());
        dialogs.contains_key(&webview_id)
    }

    fn toolbar_height(&self) -> Length<f32, DeviceIndependentPixel> {
        self.gui.borrow().toolbar_height()
    }

    /// TODO(SDL3 windowing, real progress not completion — see TODO.md): this used to be
    /// `handle_winit_window_event`, handling ~16 real winit `WindowEvent` variants — keyboard
    /// input, mouse buttons/motion/wheel, IME composition, modifiers, touch, pinch gesture,
    /// dropped files, theme/scale-factor changes, egui event forwarding and focus routing (see
    /// this file's own git history for that full body, or the pristine-vs-patched diff under
    /// `patches/servo-v0.5.0/0001-desktop-shell-core.patch` from before this migration branch).
    /// Only resize/close/redraw/focus are ported so far — a real window now opens, shows the
    /// boot splash, resizes, and closes cleanly, but does not yet accept any input at all.
    /// `handle_keyboard_input`/`handle_mouse_button_event`/`handle_mouse_move_event`/
    /// `handle_intercepted_key_bindings`/`show_ime` (further down this file) are all
    /// unreachable leftovers from that body, kept only because deleting carefully-written
    /// logic that will need re-deriving anyway seemed worse than a `#[expect(dead_code)]` —
    /// re-wire them once `WindowEvent` grows the variants they need.
    pub(crate) fn handle_window_event(
        &self,
        state: Rc<RunningAppState>,
        window: Rc<ServoShellWindow>,
        event: WindowEvent,
    ) {
        self.gui
            .borrow_mut()
            .handle_accessibility_window_event(&self.sdl_window, &event);
        if let Some(consumed) = self
            .gui
            .borrow_mut()
            .handle_pointer_event(&event, self.hidpi_scale_factor().0)
        {
            self.request_redraw();
            if consumed {
                return;
            }
        }
        if let Some(consumed) = self.gui.borrow_mut().handle_keyboard_event(&event) {
            self.request_redraw();
            if consumed {
                return;
            }
        }

        let theme = current_sdl_theme();
        if self.last_theme.replace(theme) != theme {
            if let Some(webview) = window.active_webview() {
                webview.notify_theme_change(theme);
            }
        }

        // Handle resize events first, so that any subsequent redrawing draws onto a buffer of the
        // correct size.
        let mut resized = false;
        if let WindowEvent::Resized(width, height) = &event {
            let new_inner_size = PhysicalSize::new(*width, *height);
            if self.inner_size.get() != new_inner_size {
                self.inner_size.set(new_inner_size);
                self.window_rendering_context.resize(new_inner_size);
                resized = true;
            }
        }

        // If requested to redraw or resized, repaint as soon as possible, so that new buffer
        // contents are available to the window manager. While `page_load_splash_since` is
        // still set (see its own doc comment), keep painting the boot splash over the real
        // page instead of compositing it, until the page reports itself ready or we give up
        // waiting.
        if event == WindowEvent::RedrawRequested || resized {
            let still_loading = self.page_load_splash_since.get().filter(|since| {
                !state.is_initial_page_loaded() && since.elapsed() < MAX_PAGE_LOAD_SPLASH_DURATION
            });
            match still_loading {
                Some(since) => self.paint_splash(since.elapsed()),
                None => {
                    self.page_load_splash_since.set(None);
                    match logging::content_load_error() {
                        Some(message) => self.paint_content_load_error(&message),
                        None => {
                            let mut gui = self.gui.borrow_mut();
                            gui.update(&state, &window, self);
                            gui.paint(&self.sdl_window);
                        },
                    }
                },
            }
        }

        match event {
            WindowEvent::Focused(true) => state.handle_focused(window.clone()),
            WindowEvent::CloseRequested => window.schedule_close(),
            // TODO(SDL3 windowing): no egui input is wired up yet on this branch (see
            // `gui.rs`'s own `SdlEguiGlow` doc comment) -- so unlike the old winit-based
            // dispatch, there's no "does egui have keyboard/mouse focus, forward there
            // instead" check to make yet. Every event below always goes straight to the
            // active `WebView`. Revisit once egui input exists and can plausibly claim focus
            // (a dialog, a text field in the toolbar, ...).
            WindowEvent::KeyDown { keycode, scancode, keymod, repeat } => {
                let keyboard_event =
                    keyboard_event_from_sdl(keycode, scancode, keymod, true, repeat);
                self.handle_keyboard_input(state, &window, keyboard_event);
            },
            WindowEvent::KeyUp { keycode, scancode, keymod } => {
                let keyboard_event =
                    keyboard_event_from_sdl(keycode, scancode, keymod, false, false);
                self.handle_keyboard_input(state, &window, keyboard_event);
            },
            WindowEvent::ImePreedit(text) => {
                if let Some(webview) = window.active_webview() {
                    if !self.ime_composing.replace(true) {
                        webview.notify_input_event(InputEvent::Ime(ImeEvent::Composition(
                            servo::CompositionEvent {
                                state: servo::CompositionState::Start,
                                data: String::new(),
                            },
                        )));
                    }
                    webview.notify_input_event(InputEvent::Ime(ImeEvent::Composition(
                        servo::CompositionEvent {
                            state: servo::CompositionState::Update,
                            data: text,
                        },
                    )));
                }
            },
            WindowEvent::ImeCommit(text) => {
                if let Some(webview) = window.active_webview() {
                    if !self.ime_composing.replace(false) {
                        webview.notify_input_event(InputEvent::Ime(ImeEvent::Composition(
                            servo::CompositionEvent {
                                state: servo::CompositionState::Start,
                                data: String::new(),
                            },
                        )));
                    }
                    webview.notify_input_event(InputEvent::Ime(ImeEvent::Composition(
                        servo::CompositionEvent {
                            state: servo::CompositionState::End,
                            data: text,
                        },
                    )));
                }
            },
            WindowEvent::MouseMotion { x, y } => {
                if let Some(webview) = window.active_webview() {
                    self.handle_mouse_move_event(&webview, x, y);
                }
            },
            WindowEvent::MouseButtonDown { button, .. } => {
                if let Some(webview) = window.active_webview() {
                    self.handle_mouse_button_event(&webview, button, true);
                }
            },
            WindowEvent::MouseButtonUp { button, .. } => {
                if let Some(webview) = window.active_webview() {
                    self.handle_mouse_button_event(&webview, button, false);
                }
            },
            // TODO(SDL3 windowing): scroll direction (whether `y` needs negating to match the
            // DOM's own deltaY convention) is unverified — no real device to check against.
            WindowEvent::MouseWheel { x, y } => {
                if let Some(webview) = window.active_webview() {
                    let delta = WheelDelta {
                        x: (x * LINE_WIDTH) as f64,
                        y: (y * LINE_HEIGHT) as f64,
                        z: 0.0,
                        mode: WheelMode::DeltaPixel,
                    };
                    let point = self.webview_relative_mouse_point.get();
                    webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                        delta,
                        point.into(),
                    )));
                }
            },
            WindowEvent::CursorLeft => {
                if let Some(webview) = window.active_webview() {
                    let webview_rect: Rect<_, _> = webview.size().into();
                    if webview_rect.contains(self.webview_relative_mouse_point.get()) {
                        webview.notify_input_event(InputEvent::MouseLeftViewport(
                            MouseLeftViewportEvent::default(),
                        ));
                    }
                }
            },
            WindowEvent::DroppedFile(path) => {
                if let Some(webview) = window.active_webview() {
                    if let Ok(url) = Url::from_file_path(&path) {
                        webview.load(url);
                    } else {
                        log::error!("Failed to create URL for dropped file ({path})");
                    }
                }
            },
            WindowEvent::Touch { phase, finger_id, x, y } => {
                if let Some(webview) = window.active_webview() {
                    self.handle_touch_event(&webview, phase, finger_id, x, y);
                }
            },
            WindowEvent::PinchGesture { delta } => {
                if let Some(webview) = window.active_webview() {
                    webview.adjust_pinch_zoom(
                        delta + 1.0,
                        self.webview_relative_mouse_point.get(),
                    );
                }
            },
            WindowEvent::DisplayChanged => {
                if let Ok(bounds) = self
                    .sdl_window
                    .get_display()
                    .and_then(|display| display.get_bounds())
                {
                    let scale = Scale::<f32, DeviceIndependentPixel, DevicePixel>::new(
                        self.sdl_window.display_scale(),
                    );
                    self.screen_size.set(
                        (Size2D::new(bounds.width(), bounds.height()).to_f32() / scale).to_u32(),
                    );
                }
                self.request_redraw();
            },
            // Resize/redraw are handled before this dispatch so the rendering context is
            // already up to date. Losing focus currently needs no additional Servo-side
            // action, but spelling these variants out keeps this match exhaustive: adding a
            // new translated SDL event must also add its real handling here instead of being
            // silently swallowed by a catch-all arm.
            WindowEvent::Resized(..) |
            WindowEvent::RedrawRequested |
            WindowEvent::Focused(false) => {},
        }
    }

}

impl PlatformWindow for HeadedWindow {
    fn as_headed_window(&self) -> Option<&Self> {
        Some(self)
    }

    fn screen_geometry(&self) -> ScreenGeometry {
        let hidpi_factor = self.hidpi_scale_factor();
        let toolbar_size = Size2D::new(0.0, (self.toolbar_height() * self.hidpi_scale_factor()).0);
        let screen_size = self.screen_size.get().to_f32() * hidpi_factor;

        // FIXME: In reality, this should subtract screen space used by the system interface
        // elements, but it is difficult to get this value with `winit` currently. See:
        // See https://github.com/rust-windowing/winit/issues/2494
        let available_screen_size = screen_size - toolbar_size;

        // TODO(SDL3 windowing): SDL3's `Window::position`/`size` report the client
        // (decoration-excluded) area, not winit's own `outer_position`/`outer_size` (which
        // include OS decorations) — same simplification noted on `PlatformWindow::window_rect`
        // further down. Real for windowed mode, an approximation with a native titlebar.
        let (x, y) = self.sdl_window.position();
        let (width, height) = self.sdl_window.size();
        let window_rect = DeviceIntRect::from_origin_and_size(
            DeviceIntPoint::new(x, y),
            DeviceIntSize::new(width as i32, height as i32),
        );

        ScreenGeometry {
            size: screen_size.to_i32(),
            available_size: available_screen_size.to_i32(),
            window_rect,
        }
    }

    fn device_hidpi_scale_factor(&self) -> Scale<f32, DeviceIndependentPixel, DevicePixel> {
        Scale::new(self.sdl_window.display_scale())
    }

    fn hidpi_scale_factor(&self) -> Scale<f32, DeviceIndependentPixel, DevicePixel> {
        self.device_pixel_ratio_override
            .map(Scale::new)
            .unwrap_or_else(|| self.device_hidpi_scale_factor())
    }

    fn update_user_interface_state(&self, _: &RunningAppState, window: &ServoShellWindow) -> bool {
        let title = self.window_title_override.clone().unwrap_or_else(|| {
            window
                .active_webview()
                .and_then(|webview| {
                    webview
                        .page_title()
                        .filter(|title| !title.is_empty())
                        .or_else(|| webview.url().map(|url| url.to_string()))
                })
                .unwrap_or_else(|| INITIAL_WINDOW_TITLE.to_string())
        });
        if title != *self.last_title.borrow() {
            // `Window`'s mutating methods take `&mut self` even though they just forward to a
            // plain FFI call on the shared, `Arc`-backed native window — `.clone()` (cheap: an
            // `Arc` bump, not a real duplicate window) gets an owned, independently-`&mut`-able
            // handle to call them through, same pattern used throughout this file wherever
            // `&self` needs to call one of these. See `Window`'s own `#[derive(Clone)]`.
            let _ = self.sdl_window.clone().set_title(&title);
            *self.last_title.borrow_mut() = title;
        }

        self.gui.borrow_mut().update_webview_data(window)
    }

    fn request_repaint(&self, _: &ServoShellWindow) {
        self.request_redraw();
    }

    /// TODO(SDL3 windowing): unlike winit's `inner_size()`/`outer_size()`, SDL3's `Window::
    /// size()` doesn't distinguish the client area from OS decorations — treated as equal here
    /// (`decoration_size` always zero), same simplification as `screen_geometry`/`window_rect`.
    /// Loses window-decoration accounting on a native titlebar; harmless with
    /// `no_native_titlebar` (not ported either — see `HeadedWindow::new`'s own TODO).
    fn request_resize(&self, _: &WebView, new_outer_size: DeviceIntSize) -> Option<DeviceIntSize> {
        let (width, height) = self.sdl_window.size();
        let outer_size = PhysicalSize::new(width, height);
        let decoration_size = DeviceIntSize::zero();

        let screen_size =
            (self.screen_size.get().to_f32() * self.hidpi_scale_factor()).to_i32();
        let new_outer_size =
            new_outer_size.clamp(MIN_WINDOW_INNER_SIZE + decoration_size, screen_size * 2);

        if outer_size.width == new_outer_size.width as u32 &&
            outer_size.height == new_outer_size.height as u32
        {
            return Some(new_outer_size);
        }

        let new_inner_size = new_outer_size - decoration_size;
        let resulting_size = PhysicalSize::new(
            new_inner_size.width.max(0) as u32,
            new_inner_size.height.max(0) as u32,
        );
        match self.sdl_window.clone().set_size(resulting_size.width, resulting_size.height) {
            Ok(()) => {
                if self.inner_size.get() != resulting_size {
                    self.inner_size.set(resulting_size);
                    self.window_rendering_context.resize(resulting_size);
                }
                Some(DeviceIntSize::new(
                    resulting_size.width as i32 + decoration_size.width,
                    resulting_size.height as i32 + decoration_size.height,
                ))
            },
            Err(_) => None,
        }
    }

    fn window_rect(&self) -> DeviceIndependentIntRect {
        let (width, height) = self.sdl_window.size();
        let scale = self.hidpi_scale_factor();

        let outer_size = DeviceIntSize::new(width as i32, height as i32);

        let (x, y) = self.sdl_window.position();
        let origin = DeviceIntPoint::new(x, y);
        convert_rect_to_css_pixel(
            DeviceIntRect::from_origin_and_size(origin, outer_size),
            scale,
        )
    }

    fn set_position(&self, point: DeviceIntPoint) {
        let _ = self
            .sdl_window
            .clone()
            .set_position(sdl3::video::WindowPos::Positioned(point.x), sdl3::video::WindowPos::Positioned(point.y));
    }

    /// TODO(SDL3 windowing): `set_fullscreen`'s own monitor selection (winit's
    /// `current_monitor()`/`available_monitors()`) isn't ported — SDL3's `set_fullscreen`
    /// takes a plain `bool` and always uses the window's current display, so there's no
    /// monitor argument to plumb through in the first place. Simpler than before, not a gap.
    fn set_fullscreen(&self, state: bool) {
        if self.fullscreen.get() != state {
            let _ = self.sdl_window.clone().set_fullscreen(state);
            persist_fullscreen_state(self.config_dir.as_deref(), state);
        }
        self.fullscreen.set(state);
    }

    fn get_fullscreen(&self) -> bool {
        self.fullscreen.get()
    }

    fn id(&self) -> ServoShellWindowId {
        let id: u64 = self.sdl_window_id().into();
        id.into()
    }

    fn set_cursor(&self, cursor: Cursor) {
        let system_cursor = match cursor {
            Cursor::None => {
                self.mouse.show_cursor(false);
                return;
            },
            Cursor::Text | Cursor::VerticalText => SystemCursor::IBeam,
            Cursor::Wait => SystemCursor::Wait,
            Cursor::Progress => SystemCursor::WaitArrow,
            Cursor::Crosshair | Cursor::Cell => SystemCursor::Crosshair,
            Cursor::Pointer | Cursor::ContextMenu | Cursor::Help | Cursor::Alias |
            Cursor::Copy | Cursor::Grab | Cursor::Grabbing | Cursor::ZoomIn |
            Cursor::ZoomOut => SystemCursor::Hand,
            Cursor::EResize | Cursor::WResize | Cursor::EwResize | Cursor::ColResize => {
                SystemCursor::SizeWE
            },
            Cursor::NResize | Cursor::SResize | Cursor::NsResize | Cursor::RowResize => {
                SystemCursor::SizeNS
            },
            Cursor::NeResize | Cursor::SwResize | Cursor::NeswResize => SystemCursor::SizeNESW,
            Cursor::NwResize | Cursor::SeResize | Cursor::NwseResize => SystemCursor::SizeNWSE,
            Cursor::Move | Cursor::AllScroll => SystemCursor::SizeAll,
            Cursor::NoDrop | Cursor::NotAllowed => SystemCursor::No,
            Cursor::Default => SystemCursor::Arrow,
        };

        self.mouse.show_cursor(true);
        match SdlCursor::from_system(system_cursor) {
            Ok(native_cursor) => {
                native_cursor.set();
                *self.active_cursor.borrow_mut() = Some(native_cursor);
            },
            Err(error) => log::warn!("Could not create SDL3 system cursor: {error}"),
        }
    }

    #[cfg(feature = "webxr")]
    fn new_glwindow(&self, event_loop: &ActiveEventLoop) -> Rc<dyn servo::webxr::GlWindow> {
        let (width, height) = self.sdl_window.size();

        let sdl_window = event_loop
            .video()
            .window("Roves XR", width, height)
            .opengl()
            .hidden()
            .build()
            .expect("Failed to create window.");

        let pose = Rc::new(XRWindowPose {
            xr_rotation: Cell::new(Rotation3D::identity()),
            xr_translation: Cell::new(Vector3D::zero()),
        });
        self.xr_window_poses.borrow_mut().push(pose.clone());
        Rc::new(XRWindow { sdl_window, pose })
    }

    fn rendering_context(&self) -> Rc<dyn RenderingContext> {
        self.rendering_context.clone()
    }

    /// TODO(SDL3 windowing): approximated with the *system* theme (`SDL_GetSystemTheme`) —
    /// unlike winit's `Window::theme()`, SDL3 has no equivalent that accounts for a specific
    /// window's own forced/overridden theme (most windows just follow the system one anyway,
    /// so this is right in the common case).
    fn theme(&self) -> servo::Theme {
        current_sdl_theme()
    }

    fn maximize(&self, _webview: &WebView) {
        let _ = self.sdl_window.clone().maximize();
    }

    /// Handle servoshell key bindings that may have been prevented by the page in the active webview.
    fn notify_input_event_handled(
        &self,
        webview: &WebView,
        id: InputEventId,
        result: InputEventResult,
    ) {
        let Some(keyboard_event) = self.pending_keyboard_events.borrow_mut().remove(&id) else {
            return;
        };
        if result.intersects(InputEventResult::DefaultPrevented | InputEventResult::Consumed) {
            return;
        }

        ShortcutMatcher::from_event(keyboard_event.event)
            .shortcut(CMD_OR_CONTROL, '=', || {
                webview.set_page_zoom(webview.page_zoom() + 0.1);
            })
            .shortcut(CMD_OR_CONTROL, '+', || {
                webview.set_page_zoom(webview.page_zoom() + 0.1);
            })
            .shortcut(CMD_OR_CONTROL, '-', || {
                webview.set_page_zoom(webview.page_zoom() - 0.1);
            })
            .shortcut(CMD_OR_CONTROL, '0', || {
                webview.set_page_zoom(1.0);
            });
    }

    fn focus(&self) {
        let _ = self.sdl_window.clone().raise();
    }

    fn has_platform_focus(&self) -> bool {
        self.sdl_window.has_input_focus()
    }

    fn show_embedder_control(&self, webview_id: WebViewId, embedder_control: EmbedderControl) {
        let control_id = embedder_control.id();
        match embedder_control {
            EmbedderControl::SelectElement(prompt) => {
                // FIXME: Reading the toolbar height is needed here to properly position the select dialog.
                // But if the toolbar height changes while the dialog is open then the position won't be updated
                let offset = self.gui.borrow().toolbar_height();
                self.add_dialog(
                    webview_id,
                    Dialog::new_select_element_dialog(prompt, offset),
                );
            },
            EmbedderControl::ColorPicker(color_picker) => {
                // FIXME: Reading the toolbar height is needed here to properly position the select dialog.
                // But if the toolbar height changes while the dialog is open then the position won't be updated
                let offset = self.gui.borrow().toolbar_height();
                self.add_dialog(
                    webview_id,
                    Dialog::new_color_picker_dialog(color_picker, offset),
                );
            },
            EmbedderControl::InputMethod(input_method_control) => {
                self.show_ime(control_id, input_method_control);
            },
            EmbedderControl::FilePicker(file_picker) => {
                self.add_dialog(webview_id, Dialog::new_file_dialog(file_picker));
            },
            EmbedderControl::SimpleDialog(simple_dialog) => {
                self.add_dialog(webview_id, Dialog::new_simple_dialog(simple_dialog));
            },
            EmbedderControl::ContextMenu(prompt) => {
                // Right-click context menus are disabled entirely for this game embedding:
                // dismissing the request immediately is a no-op from the player's perspective,
                // since `ContextMenu::drop` sends the "no selection" response on our behalf.
                drop(prompt);
            },
        }
    }

    fn hide_embedder_control(&self, webview_id: WebViewId, embedder_control_id: EmbedderControlId) {
        if self.visible_input_method.get() == Some(embedder_control_id) {
            self.visible_input_method.set(None);
            self.ime_composing.set(false);
            self.text_input.stop(&self.sdl_window);
            return;
        }
        self.remove_dialog(webview_id, embedder_control_id);
    }

    fn show_bluetooth_device_dialog(
        &self,
        webview_id: WebViewId,
        request: BluetoothDeviceSelectionRequest,
    ) {
        self.add_dialog(webview_id, Dialog::new_device_selection_dialog(request));
    }

    fn show_permission_dialog(&self, webview_id: WebViewId, permission_request: PermissionRequest) {
        self.add_dialog(
            webview_id,
            Dialog::new_permission_request_dialog(permission_request),
        );
    }

    fn show_http_authentication_dialog(
        &self,
        webview_id: WebViewId,
        authentication_request: AuthenticationRequest,
    ) {
        self.add_dialog(
            webview_id,
            Dialog::new_authentication_dialog(authentication_request),
        );
    }

    fn dismiss_embedder_controls_for_webview(&self, webview_id: WebViewId) {
        self.dialogs.borrow_mut().remove(&webview_id);
    }

    fn show_console_message(&self, level: servo::ConsoleLogLevel, message: &str) {
        println!("{message}");
        log::log!(level.into(), "{message}");
    }

    fn notify_accessibility_tree_update(
        &self,
        _webview: WebView,
        tree_update: accesskit::TreeUpdate,
    ) {
        self.gui
            .borrow_mut()
            .notify_accessibility_tree_update(tree_update);
    }
}

/// Persists fullscreen state across launches as a marker file's mere existence — no JSON,
/// matching `support/content-packer/src/extract.rs`'s own marker-file convention — so
/// `prefs.rs`'s `start_fullscreen` can read it back with a plain `Path::exists()` check next
/// launch. `config_dir` is `None` when it couldn't be resolved at all (see
/// `ServoShellPreferences::config_dir`); persistence is simply skipped in that case, same
/// failure philosophy as elsewhere in this codebase (missing state degrades to "off" rather
/// than panicking).
fn persist_fullscreen_state(config_dir: Option<&Path>, fullscreen: bool) {
    let Some(config_dir) = config_dir else { return };
    let marker = config_dir.join("fullscreen");
    let result = if fullscreen {
        fs::write(&marker, b"")
    } else {
        fs::remove_file(&marker).or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        })
    };
    if let Err(error) = result {
        log::warn!("persisting fullscreen state to {marker:?}: {error}");
    }
}

/// A game-supplied window icon, if `mach bundle --icon-png` copied one next to this exact
/// binary (see `python/servo/post_build_commands.py`) — checked at every launch, not baked
/// in at compile time, specifically so a *prebuilt* shell (downloaded, never compiled
/// per-game — every `roves-action` base-mode consumer, and Packmaster) can still show a
/// game's own branding instead of whatever `build.rs` happened to bake in when that shell
/// itself was built (Roves' own icon, absent a `--icon-png` passed to that build). `None`
/// when no such file exists, or `current_exe()` itself fails — never a hard error, since the
/// compile-time default is always a safe fallback.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn runtime_window_icon_bytes() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    std::fs::read(dir.join("icon.png")).ok()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_window_icon(window: &mut sdl3::video::Window) {
    let bytes = runtime_window_icon_bytes()
        .unwrap_or_else(|| include_bytes!(concat!(env!("OUT_DIR"), "/window_icon.png")).to_vec());
    let image = match image::load_from_memory(&bytes) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            log::warn!("Could not decode the SDL3 window icon: {error}");
            return;
        },
    };
    let (width, height) = image.dimensions();
    let mut pixels = image.into_raw();
    match Surface::from_data(
        pixels.as_mut_slice(),
        width,
        height,
        width.saturating_mul(4),
        PixelFormat::RGBA32,
    ) {
        Ok(surface) => {
            if !window.set_icon(surface) {
                log::warn!("SDL3 rejected the decoded window icon");
            }
        },
        Err(error) => log::warn!("Could not create the SDL3 window icon surface: {error}"),
    }
}

#[cfg(feature = "webxr")]
struct XRWindow {
    sdl_window: sdl3::video::Window,
    pose: Rc<XRWindowPose>,
}

struct XRWindowPose {
    xr_rotation: Cell<Rotation3D<f32, UnknownUnit, UnknownUnit>>,
    xr_translation: Cell<Vector3D<f32, UnknownUnit>>,
}

#[cfg(feature = "webxr")]
impl servo::webxr::GlWindow for XRWindow {
    fn get_render_target(
        &self,
        device: &mut surfman::Device,
        _context: &mut surfman::Context,
    ) -> servo::webxr::GlWindowRenderTarget {
        let _ = self.sdl_window.clone().show();
        let window_handle = self
            .sdl_window
            .window_handle()
            .expect("could not get window handle from window");
        let (width, height) = self.sdl_window.size();
        let size = Size2D::new(width as i32, height as i32);
        let native_widget = device
            .connection()
            .create_native_widget_from_window_handle(window_handle, size)
            .expect("Failed to create native widget");
        servo::webxr::GlWindowRenderTarget::NativeWidget(native_widget)
    }

    fn get_rotation(&self) -> Rotation3D<f32, UnknownUnit, UnknownUnit> {
        self.pose.xr_rotation.get()
    }

    fn get_translation(&self) -> Vector3D<f32, UnknownUnit> {
        self.pose.xr_translation.get()
    }

    fn get_mode(&self) -> servo::webxr::GlWindowMode {
        use servo::pref;
        if pref!(dom_webxr_glwindow_red_cyan) {
            servo::webxr::GlWindowMode::StereoRedCyan
        } else if pref!(dom_webxr_glwindow_left_right) {
            servo::webxr::GlWindowMode::StereoLeftRight
        } else if pref!(dom_webxr_glwindow_spherical) {
            servo::webxr::GlWindowMode::Spherical
        } else if pref!(dom_webxr_glwindow_cubemap) {
            servo::webxr::GlWindowMode::Cubemap
        } else {
            servo::webxr::GlWindowMode::Blit
        }
    }

    fn display_handle(&self) -> raw_window_handle::DisplayHandle<'_> {
        self.sdl_window
            .display_handle()
            .expect("Every window should have a display handle")
    }
}

impl XRWindowPose {
    fn handle_xr_translation(&self, input: &KeyboardEvent) {
        if input.event.state != KeyState::Down {
            return;
        }
        const NORMAL_TRANSLATE: f32 = 0.1;
        const QUICK_TRANSLATE: f32 = 1.0;
        let mut x = 0.0;
        let mut z = 0.0;
        match input.event.key {
            Key::Character(ref k) => match &**k {
                "w" => z = -NORMAL_TRANSLATE,
                "W" => z = -QUICK_TRANSLATE,
                "s" => z = NORMAL_TRANSLATE,
                "S" => z = QUICK_TRANSLATE,
                "a" => x = -NORMAL_TRANSLATE,
                "A" => x = -QUICK_TRANSLATE,
                "d" => x = NORMAL_TRANSLATE,
                "D" => x = QUICK_TRANSLATE,
                _ => return,
            },
            _ => return,
        };
        let (old_x, old_y, old_z) = self.xr_translation.get().to_tuple();
        let vec = Vector3D::new(x + old_x, old_y, z + old_z);
        self.xr_translation.set(vec);
    }

    fn handle_xr_rotation(&self, input: &KeyboardEvent) {
        if input.event.state != KeyState::Down {
            return;
        }
        let mut x = 0.0;
        let mut y = 0.0;
        match input.event.key {
            Key::Named(NamedKey::ArrowUp) => x = 1.0,
            Key::Named(NamedKey::ArrowDown) => x = -1.0,
            Key::Named(NamedKey::ArrowLeft) => y = 1.0,
            Key::Named(NamedKey::ArrowRight) => y = -1.0,
            _ => return,
        }
        if input.event.modifiers.contains(Modifiers::SHIFT) {
            x *= 10.0;
            y *= 10.0;
        }
        let x: Rotation3D<_, UnknownUnit, UnknownUnit> = Rotation3D::around_x(Angle::degrees(x));
        let y: Rotation3D<_, UnknownUnit, UnknownUnit> = Rotation3D::around_y(Angle::degrees(y));
        let rotation = self.xr_rotation.get().then(&x).then(&y);
        self.xr_rotation.set(rotation);
    }
}

#[derive(Default)]
pub struct TouchEventSimulator {
    pub left_mouse_button_down: Cell<bool>,
}

impl TouchEventSimulator {
    fn maybe_consume_move_button_event(
        &self,
        webview: &WebView,
        button: SdlMouseButton,
        pressed: bool,
        point: DevicePoint,
    ) -> bool {
        if button != SdlMouseButton::Left {
            return false;
        }

        if pressed && !self.left_mouse_button_down.get() {
            webview.notify_input_event(InputEvent::Touch(TouchEvent::new(
                TouchEventType::Down,
                TouchId(0),
                point.into(),
                TouchPointerType::Touch,
            )));
            self.left_mouse_button_down.set(true);
        } else if !pressed {
            webview.notify_input_event(InputEvent::Touch(TouchEvent::new(
                TouchEventType::Up,
                TouchId(0),
                point.into(),
                TouchPointerType::Touch,
            )));
            self.left_mouse_button_down.set(false);
        }

        true
    }

    fn maybe_consume_mouse_move_event(
        &self,
        webview: &WebView,
        point: Point2D<f32, DevicePixel>,
    ) -> bool {
        if !self.left_mouse_button_down.get() {
            return false;
        }

        webview.notify_input_event(InputEvent::Touch(TouchEvent::new(
            TouchEventType::Move,
            TouchId(0),
            point.into(),
            TouchPointerType::Touch,
        )));
        true
    }
}
