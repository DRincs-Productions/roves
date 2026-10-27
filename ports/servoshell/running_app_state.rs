/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared state and methods for desktop and EGL implementations.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use image::{DynamicImage, ImageFormat};
#[cfg(all(any(coverage, llvm_pgo), target_os = "android"))]
use libc::c_char;
use log::error;
use servo::{
    AuthenticationRequest, BluetoothDeviceSelectionRequest, ConsoleLogLevel,
    CreateNewWebViewRequest, DeviceIntPoint, DeviceIntSize, EmbedderControl, EmbedderControlId,
    InputEventId, InputEventResult, LoadStatus, MediaSessionEvent, PermissionRequest, Servo,
    ServoDelegate, ServoError, UserContentManager, WebView, WebViewDelegate, WebViewId,
};
use url::Url;

// On macOS the delegate is compiled but only created when `ROVES_MACOS_GAMEPAD=1` (see
// `desktop::gamepad::runtime_enabled`): SDL3's gamepad init once hung the shell there, cause
// unconfirmed on a real Mac. See CUSTOMIZATIONS.md's SDL3 gamepad entries.
#[cfg(all(feature = "gamepad", not(target_os = "android")))]
pub(crate) use crate::desktop::gamepad::ServoshellGamepadDelegate;
use crate::prefs::ServoShellPreferences;
use crate::window::{PlatformWindow, ServoShellWindow, ServoShellWindowId};

#[cfg(all(any(coverage, llvm_pgo), target_os = "android"))]
unsafe extern "C" {
    fn __llvm_profile_set_filename(file: *const c_char);
    fn __llvm_profile_write_file();
}

#[derive(Default)]
pub struct WebViewCollection {
    /// List of top-level browsing contexts.
    /// Modified by EmbedderMsg::WebViewOpened and EmbedderMsg::WebViewClosed,
    /// and we exit if it ever becomes empty.
    webviews: HashMap<WebViewId, WebView>,

    /// The order in which the webviews were created.
    pub(crate) creation_order: Vec<WebViewId>,

    /// The [`WebView`] that is currently active. This is the [`WebView`] that is shown and has
    /// input focus.
    active_webview_id: Option<WebViewId>,
}

impl WebViewCollection {
    pub fn add(&mut self, webview: WebView) {
        let id = webview.id();
        self.creation_order.push(id);
        self.webviews.insert(id, webview);
    }

    /// Removes a webview from the collection by [`WebViewId`]. If the removed [`WebView`] was the active
    /// [`WebView`] then the next newest [`WebView`] will be activated.
    pub fn remove(&mut self, id: WebViewId) -> Option<WebView> {
        self.creation_order.retain(|&webview_id| webview_id != id);
        let removed_webview = self.webviews.remove(&id);

        if self.active_webview_id == Some(id) {
            self.active_webview_id = None;
            if let Some(newest) = self.creation_order.last() {
                self.activate_webview(*newest);
            }
        }

        removed_webview
    }

    pub fn active(&self) -> Option<&WebView> {
        self.active_webview_id.and_then(|id| self.webviews.get(&id))
    }

    pub fn active_id(&self) -> Option<WebViewId> {
        self.active_webview_id
    }

    /// Gets a reference to the most recently created webview, if any.
    pub fn newest(&self) -> Option<&WebView> {
        self.creation_order
            .last()
            .and_then(|id| self.webviews.get(id))
    }

    pub fn all_in_creation_order(&self) -> impl Iterator<Item = (WebViewId, &WebView)> {
        self.creation_order
            .iter()
            .filter_map(move |id| self.webviews.get(id).map(|webview| (*id, webview)))
    }

    /// Returns true if the collection contains no webviews.
    pub fn is_empty(&self) -> bool {
        self.webviews.is_empty()
    }

    pub(crate) fn activate_webview(&mut self, id_to_activate: WebViewId) {
        assert!(self.creation_order.contains(&id_to_activate));

        self.active_webview_id = Some(id_to_activate);
        for (webview_id, webview) in self.all_in_creation_order() {
            if id_to_activate == webview_id {
                webview.show();
                webview.focus();
            } else {
                webview.hide();
                webview.blur();
            }
        }
    }

    pub(crate) fn activate_webview_by_index(&mut self, index: usize) {
        let Some(webview_id) = self.creation_order.get(index) else {
            // Just ignore requests to activate uknown WebViews. This can happen by pressing
            // keyboard shortcuts in the interface.
            return;
        };
        self.activate_webview(*webview_id);
    }
}

/// A command received via the user interacting with the user interface.
#[cfg(target_os = "android")]
pub(crate) enum UserInterfaceCommand {
    Go(String),
    Back,
    Forward,
    Reload,
}

pub(crate) struct RunningAppState {
    /// The gamepad provider, used for handling gamepad events and set on each WebView.
    /// May be `None` if gamepad support is disabled or failed to initialize, or on macOS
    /// without the runtime opt-in (see the `use` above).
    #[cfg(all(feature = "gamepad", not(target_os = "android")))]
    gamepad_delegate: Option<Rc<ServoshellGamepadDelegate>>,

    /// servoshell specific preferences created during startup of the application.
    pub(crate) servoshell_preferences: ServoShellPreferences,

    /// A handle to the Servo instance.
    pub(crate) servo: Servo,

    /// Whether or not the application has achieved stable image output. This is used
    /// for the `exit_after_stable_image` option.
    pub(crate) achieved_stable_image: Rc<Cell<bool>>,

    /// The [`UserContentManager`] for all `WebView`s created.
    pub(crate) user_content_manager: Rc<UserContentManager>,

    /// Whether or not program exit has been triggered. This means that all windows
    /// will be destroyed and shutdown will start at the end of the current event loop.
    exit_scheduled: Cell<bool>,

    /// The set of [`ServoShellWindow`]s that currently exist for this instance of servoshell.
    // This is the last field of the struct to ensure that windows are dropped *after* all
    // other references to the relevant rendering contexts have been destroyed.
    // See https://github.com/servo/servo/issues/36711.
    windows: RefCell<HashMap<ServoShellWindowId, Rc<ServoShellWindow>>>,

    /// The currently focused [`ServoShellWindow`], if one is focused.
    focused_window: RefCell<Option<Rc<ServoShellWindow>>>,

    /// Whether accessibility is active in servoshell.
    ///
    /// Set by the platform via AccessKit, and forwarded to existing and new WebViews via
    /// [`WebView::set_accessibility_active()`], in [`Self::set_accessibility_active()`] and
    /// and [`ServoShellWindow::create_toplevel_webview()`].
    accessibility_active: Cell<bool>,

    /// The id of the first (and, outside multi-window/tab flows, only) [`WebView`] this
    /// instance ever opens — set once, in [`Self::open_window`], and never cleared. Used
    /// by [`Self::is_initial_page_loaded`] to know which `WebView`'s `LoadStatus::Complete`
    /// actually marks "the game's own page is ready", as opposed to some later tab/popup.
    initial_webview_id: Cell<Option<WebViewId>>,

    /// Set by [`Self::notify_load_status_changed`] the first time `initial_webview_id`'s
    /// `WebView` reaches `LoadStatus::Complete`. See [`Self::is_initial_page_loaded`].
    initial_load_complete: Cell<bool>,

    /// Emit the auxiliary-window policy warning at most once per application lifetime.
    auxiliary_window_warning_emitted: Cell<bool>,
}

impl RunningAppState {
    pub(crate) fn new(
        servo: Servo,
        servoshell_preferences: ServoShellPreferences,
        user_content_manager: Rc<UserContentManager>,
        #[cfg(all(feature = "gamepad", not(target_os = "android")))]
        gamepad_delegate: Option<Rc<ServoshellGamepadDelegate>>,
    ) -> Self {
        servo.set_delegate(Rc::new(ServoShellServoDelegate));

        Self {
            windows: Default::default(),
            focused_window: Default::default(),
            #[cfg(all(feature = "gamepad", not(target_os = "android")))]
            gamepad_delegate,
            servoshell_preferences,
            servo,
            achieved_stable_image: Default::default(),
            exit_scheduled: Default::default(),
            user_content_manager,
            accessibility_active: Cell::new(false),
            initial_webview_id: Cell::new(None),
            initial_load_complete: Cell::new(false),
            auxiliary_window_warning_emitted: Cell::new(false),
        }
    }

    pub(crate) fn open_window(
        self: &Rc<Self>,
        platform_window: Rc<dyn PlatformWindow>,
        initial_url: Url,
    ) -> Rc<ServoShellWindow> {
        let window = Rc::new(ServoShellWindow::new(platform_window.clone()));
        self.windows
            .borrow_mut()
            .insert(window.id(), window.clone());
        window.create_and_activate_toplevel_webview(self.clone(), initial_url);

        if self.initial_webview_id.get().is_none() &&
            let Some(webview) = window.active_webview()
        {
            self.initial_webview_id.set(Some(webview.id()));
        }

        // If the window already has platform focus, mark it as focused in our application state.
        if platform_window.has_platform_focus() {
            self.focus_window(window.clone());
        }

        window
    }

    pub(crate) fn windows<'a>(
        &'a self,
    ) -> Ref<'a, HashMap<ServoShellWindowId, Rc<ServoShellWindow>>> {
        self.windows.borrow()
    }

    /// Whether the very first `WebView` this instance ever opened (`open_window`'s
    /// initial navigation — practically always the game's own page) has reached
    /// `LoadStatus::Complete`. Used by the boot splash (`headed_window.rs`'s
    /// `page_load_splash_since`) as the least-bad available proxy for "the real page is
    /// ready to be shown" — see `notify_load_status_changed` above. Not a true first-
    /// *paint* signal (no such thing is exposed to the embedder today — see
    /// CUSTOMIZATIONS.md's boot-splash entries), just DOM readiness, but far closer than
    /// showing the page the instant its `WebView` is created.
    pub(crate) fn is_initial_page_loaded(&self) -> bool {
        self.initial_load_complete.get()
    }

    pub(crate) fn focused_window(&self) -> Option<Rc<ServoShellWindow>> {
        self.focused_window.borrow().clone()
    }

    pub(crate) fn focus_window(&self, window: Rc<ServoShellWindow>) {
        window.focus();
        *self.focused_window.borrow_mut() = Some(window);
    }

    #[cfg_attr(target_os = "android", expect(dead_code))]
    pub(crate) fn window(&self, id: ServoShellWindowId) -> Option<Rc<ServoShellWindow>> {
        self.windows.borrow().get(&id).cloned()
    }

    pub(crate) fn servo(&self) -> &Servo {
        &self.servo
    }

    #[cfg(all(feature = "gamepad", not(target_os = "android")))]
    pub(crate) fn gamepad_delegate(&self) -> Option<Rc<ServoshellGamepadDelegate>> {
        self.gamepad_delegate.clone()
    }

    pub(crate) fn schedule_exit(&self) {
        self.exit_scheduled.set(true);

        #[cfg(all(any(coverage, llvm_pgo), target_os = "android"))]
        {
            use std::ffi::CString;

            use crate::prefs::default_config_dir;

            let mut profile_path = default_config_dir().expect("Need a config dir");
            profile_path.push("profiles/");
            let filename = format!(
                "{}/profile-%h-%p.profraw",
                profile_path.to_str().expect("Should be unicode")
            );
            let c_filename = CString::new(filename).expect("Need a valid cstring");
            unsafe {
                __llvm_profile_set_filename(c_filename.as_ptr() as *const c_char);
                __llvm_profile_write_file()
            }
        }
    }

    /// Close any [`ServoShellWindow`] that doesn't have an open [`WebView`].
    fn close_empty_windows(&self) {
        self.windows.borrow_mut().retain(|_, window| {
            if !self.exit_scheduled.get() && !window.should_close() {
                return true;
            }

            if let Some(focused_window) = self.focused_window() &&
                Rc::ptr_eq(window, &focused_window)
            {
                *self.focused_window.borrow_mut() = None;
            }
            false
        });
    }

    /// Spins the internal application event loop.
    ///
    /// - Notifies Servo about incoming gamepad events
    /// - Spin the Servo event loop, which will update Servo's embedding layer and trigger
    ///   delegate methods.
    ///
    /// Returns true if the event loop should continue spinning and false if it should exit.
    pub(crate) fn spin_event_loop(self: &Rc<Self>) -> bool {
        // We clone here to avoid a double borrow. User interface commands can update the list of windows.
        #[cfg(target_os = "android")]
        {
            let windows: Vec<_> = self.windows.borrow().values().cloned().collect();
            for window in windows {
                window.handle_interface_commands(self);
            }
        }

        self.servo.spin_event_loop();

        for window in self.windows.borrow().values() {
            window.update_and_request_repaint_if_necessary(self);
        }

        if self.servoshell_preferences.exit_after_stable_image && self.achieved_stable_image.get() {
            self.schedule_exit();
        }

        self.close_empty_windows();

        // When no more windows are open, exit the application.
        if self.windows.borrow().is_empty() {
            self.schedule_exit()
        }

        !self.exit_scheduled.get()
    }

    fn maybe_window_for_webview(&self, webview: &WebView) -> Option<Rc<ServoShellWindow>> {
        // Look up the ServoShellWindow by RenderingContext. This method can be called while a
        // WebView is being constructed, which means that it may not fully be associated with a
        // ServoShellWindow yet.
        let rendering_context = webview.rendering_context();
        self.windows()
            .values()
            .find(|window| {
                Rc::ptr_eq(
                    &window.platform_window().rendering_context(),
                    &rendering_context,
                )
            })
            .cloned()
    }

    pub(crate) fn window_for_webview(&self, webview: &WebView) -> Rc<ServoShellWindow> {
        self.maybe_window_for_webview(webview)
            .unwrap_or_else(|| panic!("Looking for unexpected WebView: {:?}", webview.id()))
    }

    pub(crate) fn platform_window_for_webview(&self, webview: &WebView) -> Rc<dyn PlatformWindow> {
        self.window_for_webview(webview).platform_window()
    }

    /// If we are exiting after achieving a stable image or we want to save the display of the
    /// [`WebView`] to an image file, request a screenshot of the [`WebView`].
    fn maybe_request_screenshot(&self, webview: WebView) {
        let output_path = self.servoshell_preferences.output_image_path.clone();
        if !self.servoshell_preferences.exit_after_stable_image && output_path.is_none() {
            return;
        }

        // Never request more than a single screenshot for now.
        let achieved_stable_image = self.achieved_stable_image.clone();
        if achieved_stable_image.get() {
            return;
        }

        webview.take_screenshot(None, move |image| {
            achieved_stable_image.set(true);

            let Some(output_path) = output_path else {
                return;
            };

            let image = match image {
                Ok(image) => image,
                Err(error) => {
                    error!("Could not take screenshot: {error:?}");
                    return;
                },
            };

            let image_format = ImageFormat::from_path(&output_path).unwrap_or(ImageFormat::Png);
            if let Err(error) =
                DynamicImage::ImageRgba8(image).save_with_format(output_path, image_format)
            {
                error!("Failed to save screenshot: {error}.");
            }
        });
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn handle_focused(&self, window: Rc<ServoShellWindow>) {
        *self.focused_window.borrow_mut() = Some(window);
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn set_accessibility_active(&self, active: bool) {
        let was_active = self.accessibility_active.replace(active);
        if active == was_active {
            return;
        }

        for window in self.windows().values() {
            for (_, webview) in window.webviews() {
                // Activate accessibility in the WebView.
                // There are two sites like this; this is the a11y activation site.
                webview.set_accessibility_active(active);
            }
        }
    }

    pub(crate) fn accessibility_active(&self) -> bool {
        self.accessibility_active.get()
    }
}

impl WebViewDelegate for RunningAppState {
    fn screen_geometry(&self, webview: WebView) -> Option<servo::ScreenGeometry> {
        Some(self.platform_window_for_webview(&webview).screen_geometry())
    }

    fn notify_status_text_changed(&self, webview: WebView, _status: Option<String>) {
        self.window_for_webview(&webview).set_needs_update();
    }

    fn notify_history_changed(&self, webview: WebView, _entries: Vec<Url>, _current: usize) {
        self.window_for_webview(&webview).set_needs_update();
    }

    fn notify_page_title_changed(&self, webview: WebView, _: Option<String>) {
        self.window_for_webview(&webview).set_needs_update();
    }

    fn request_move_to(&self, webview: WebView, new_position: DeviceIntPoint) {
        self.platform_window_for_webview(&webview)
            .set_position(new_position);
    }

    fn request_resize_to(&self, webview: WebView, requested_outer_size: DeviceIntSize) {
        self.platform_window_for_webview(&webview)
            .request_resize(&webview, requested_outer_size);
    }

    fn request_authentication(
        &self,
        webview: WebView,
        authentication_request: AuthenticationRequest,
    ) {
        self.platform_window_for_webview(&webview)
            .show_http_authentication_dialog(webview.id(), authentication_request);
    }

    fn request_create_new(&self, _parent_webview: WebView, request: CreateNewWebViewRequest) {
        // Servo's request responder sends None on drop. Reject before allocating a
        // WebView, preserving the existing document and the native shell window.
        drop(request);
        if !self.auxiliary_window_warning_emitted.replace(true) {
            log::warn!(
                "Roves blocked auxiliary-window creation: games run in a single window; \
                 repeated requests will be suppressed"
            );
        }
    }

    fn notify_closed(&self, webview: WebView) {
        self.window_for_webview(&webview)
            .close_webview(webview.id())
    }

    fn notify_input_event_handled(
        &self,
        webview: WebView,
        id: InputEventId,
        result: InputEventResult,
    ) {
        self.platform_window_for_webview(&webview)
            .notify_input_event_handled(&webview, id, result);
    }

    fn notify_cursor_changed(&self, webview: WebView, cursor: servo::Cursor) {
        self.platform_window_for_webview(&webview)
            .set_cursor(cursor);
    }

    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        self.window_for_webview(&webview).set_needs_update();

        if status == LoadStatus::Complete {
            // A new document knows no gamepads yet; see `announce_connected_gamepads`.
            #[cfg(all(feature = "gamepad", not(target_os = "android")))]
            if let Some(gamepad_delegate) = self.gamepad_delegate() {
                gamepad_delegate.announce_connected_gamepads(self);
            }
            if self.initial_webview_id.get() == Some(webview.id()) {
                self.initial_load_complete.set(true);
            }
            self.maybe_request_screenshot(webview);
        }
    }

    fn notify_fullscreen_state_changed(&self, webview: WebView, fullscreen_state: bool) {
        self.platform_window_for_webview(&webview)
            .set_fullscreen(fullscreen_state);
    }

    fn show_bluetooth_device_dialog(
        &self,
        webview: WebView,
        request: BluetoothDeviceSelectionRequest,
    ) {
        self.platform_window_for_webview(&webview)
            .show_bluetooth_device_dialog(webview.id(), request);
    }

    fn request_permission(&self, webview: WebView, permission_request: PermissionRequest) {
        self.platform_window_for_webview(&webview)
            .show_permission_dialog(webview.id(), permission_request);
    }

    fn notify_new_frame_ready(&self, webview: WebView) {
        self.window_for_webview(&webview).set_needs_repaint();
    }

    fn show_embedder_control(&self, webview: WebView, embedder_control: EmbedderControl) {
        self.window_for_webview(&webview)
            .show_embedder_control(webview, embedder_control);
    }

    fn hide_embedder_control(&self, webview: WebView, embedder_control_id: EmbedderControlId) {
        self.window_for_webview(&webview)
            .hide_embedder_control(webview, embedder_control_id);
    }

    fn notify_favicon_changed(&self, webview: WebView) {
        self.window_for_webview(&webview)
            .notify_favicon_changed(webview);
    }

    fn notify_media_session_event(&self, webview: WebView, event: MediaSessionEvent) {
        self.platform_window_for_webview(&webview)
            .notify_media_session_event(event);
    }

    fn notify_crashed(&self, webview: WebView, reason: String, backtrace: Option<String>) {
        self.platform_window_for_webview(&webview)
            .notify_crashed(webview, reason, backtrace);
    }

    fn show_console_message(&self, webview: WebView, level: ConsoleLogLevel, message: String) {
        self.platform_window_for_webview(&webview)
            .show_console_message(level, &message);
    }

    fn notify_accessibility_tree_update(
        &self,
        webview: WebView,
        tree_update: accesskit::TreeUpdate,
    ) {
        self.platform_window_for_webview(&webview)
            .notify_accessibility_tree_update(webview, tree_update);
    }
}

struct ServoShellServoDelegate;
impl ServoDelegate for ServoShellServoDelegate {
    fn notify_error(&self, error: ServoError) {
        error!("Saw Servo error: {error:?}!");
    }

    fn show_console_message(&self, level: ConsoleLogLevel, message: String) {
        // For messages without a WebView context, apply platform-specific behavior
        #[cfg(not(target_os = "android"))]
        println!("{message}");
        log::log!(level.into(), "{message}");
    }
}
