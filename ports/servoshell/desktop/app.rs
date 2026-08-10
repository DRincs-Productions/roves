/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Application entry point, runs the event loop.

use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

use roves_content_packer::extract;
use servo::protocol_handler::ProtocolRegistry;
use servo::{
    EventLoopWaker, Opts, Preferences, ServoBuilder, ServoUrl, UserContentManager, UserScript,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::WindowId;

use super::event_loop::AppEvent;
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::desktop::headed_window::HeadedWindow;
use crate::desktop::headless_window::HeadlessWindow;
use crate::desktop::protocols;
use crate::desktop::tracing::trace_winit_event;
use crate::parser::get_default_url;
use crate::prefs::ServoShellPreferences;
use crate::running_app_state::RunningAppState;
#[cfg(feature = "gamepad")]
use crate::running_app_state::ServoshellGamepadDelegate;
use crate::window::{PlatformWindow, ServoShellWindowId};

/// How long the boot splash shows before its progress bar appears — see
/// `AppState::Booting` and `Gui::update_splash`. Short enough to feel
/// immediate on a slow first launch, long enough that a fast/no-op
/// extraction (content already cached from a previous run) never shows a
/// bar at all.
const SPLASH_PROGRESS_BAR_DELAY: Duration = Duration::from_millis(300);

pub(crate) enum AppState {
    Initializing,
    /// A headed, packed-content launch whose boot extraction hasn't finished
    /// yet — `window` is already created and showing the boot splash (see
    /// `HeadedWindow::paint_splash`) while a background thread (spawned from
    /// `App::init`) decompresses the boot set. `extraction_started` drives
    /// the progress bar's appear-after-a-delay behavior; `progress` is
    /// updated by incoming `AppEvent::BootProgress`.
    Booting {
        window: Rc<dyn PlatformWindow>,
        extraction_started: Instant,
        progress: f32,
    },
    Running(Rc<RunningAppState>),
    ShuttingDown,
}

pub struct App {
    opts: Opts,
    preferences: Preferences,
    servoshell_preferences: ServoShellPreferences,
    waker: Box<dyn EventLoopWaker>,
    event_loop_proxy: Option<EventLoopProxy<AppEvent>>,
    initial_url: ServoUrl,
    /// A packed-content launch's still-to-run boot extraction (see
    /// `bundle_launch.rs`'s `BundledLaunch`), taken (and consumed) by the
    /// first call to `init`. `None` for a dev run, a plain `--url` launch,
    /// or a packed-content launch whose destination was already up to date.
    pending_extraction: Option<extract::ExtractOptions>,
    t_start: Instant,
    t: Instant,
    state: AppState,
}

impl App {
    pub fn new(
        opts: Opts,
        preferences: Preferences,
        servo_shell_preferences: ServoShellPreferences,
        event_loop: &ServoShellEventLoop,
        pending_boot_extraction: Option<extract::ExtractOptions>,
    ) -> Self {
        let t = Instant::now();
        App {
            opts,
            preferences,
            servoshell_preferences: servo_shell_preferences,
            waker: event_loop.create_event_loop_waker(),
            event_loop_proxy: event_loop.event_loop_proxy(),
            // Placeholder — recomputed in `finish_init`, once any pending boot
            // extraction has actually finished. See that function's doc
            // comment for why this can't be computed here, at construction
            // time, for a packed-content launch.
            initial_url: ServoUrl::parse("about:blank").expect("\"about:blank\" is a valid URL"),
            pending_extraction: pending_boot_extraction,
            t_start: t,
            t,
            state: AppState::Initializing,
        }
    }

    /// Initialize Application once event loop start running. Always creates
    /// the window immediately. If a boot extraction is still pending for a
    /// headed launch, defers the rest of startup (`finish_init`) until it
    /// completes — extraction runs on a background thread while a splash
    /// shows in the meantime (see `AppState::Booting`) instead of blocking
    /// window creation on it, as a plain synchronous call used to. Headless
    /// launches have no splash to show, so a pending extraction there just
    /// runs synchronously, exactly as before.
    pub fn init(&mut self, active_event_loop: Option<&ActiveEventLoop>) {
        // `self.initial_url` is still just the `finish_init`-pending placeholder
        // at this point — harmless here, since `create_platform_window`'s `url`
        // param is only ever forwarded to `Gui::new`'s own dead `_initial_url`
        // parameter (see CUSTOMIZATIONS.md), never actually used.
        let url = self.initial_url.as_url().clone();
        let platform_window = self.create_platform_window(url, active_event_loop);

        let Some(opts) = self.pending_extraction.take() else {
            self.finish_init(platform_window, active_event_loop);
            return;
        };

        let Some(active_event_loop) = active_event_loop else {
            if let Err(error) = extract::extract_boot(&opts) {
                log::error!("extracting boot content: {error}");
            }
            self.finish_init(platform_window, None);
            return;
        };

        // Show the splash immediately, then make sure we wake up again once
        // the progress bar's appear delay has passed even if no
        // `BootProgress` tick arrives before then — see `new_events`.
        if let Some(headed_window) = platform_window.as_headed_window() {
            headed_window.winit_window().request_redraw();
        }
        active_event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + SPLASH_PROGRESS_BAR_DELAY));

        let proxy = self
            .event_loop_proxy
            .clone()
            .expect("Should always have event loop proxy in headed mode.");
        thread::spawn(move || {
            let result = extract::extract_boot_with_progress(&opts, |progress| {
                let _ = proxy.send_event(AppEvent::BootProgress(progress));
            });
            if let Err(error) = result {
                log::error!("extracting boot content: {error}");
            }
            // Sent even on failure: the page will simply fail to load (same
            // failure philosophy as elsewhere for missing/corrupt content)
            // rather than leaving the app stuck on the splash forever.
            let _ = proxy.send_event(AppEvent::BootReady);
        });
        self.state = AppState::Booting {
            window: platform_window,
            extraction_started: Instant::now(),
            progress: 0.0,
        };
    }

    /// The rest of startup, deferred behind boot extraction when there is
    /// one (see `init`): builds the protocol registry/Servo instance,
    /// constructs `RunningAppState`, and opens the real webview into
    /// `platform_window` (already created by `init`).
    fn finish_init(&mut self, platform_window: Rc<dyn PlatformWindow>, active_event_loop: Option<&ActiveEventLoop>) {
        // Computed here rather than at `App::new` time: for a packed-content
        // launch, `init` only ever calls `finish_init` once any pending boot
        // extraction has actually finished (synchronously for headless, or
        // via `AppEvent::BootReady` for headed — see `init`), so the target
        // file genuinely exists on disk by now. Computing this any earlier
        // (as used to happen, in `App::new`) races extraction on a true
        // first launch: `get_default_url` only trusts a `file:` URL whose
        // target already exists (`parser.rs`'s `exists(path)` check), so on
        // a cache-miss first launch it would silently fall through to
        // parsing the raw path as a URL directly — which, for a Windows
        // absolute path, misparses the drive letter as the scheme (see
        // `parser.rs`'s own comment on this) — surfacing as "Could not load
        // the requested page: Unsupported scheme" until the *next* launch,
        // once extraction has already happened once.
        self.initial_url = get_default_url(
            self.servoshell_preferences.url.as_deref(),
            env::current_dir().unwrap(),
            |path| fs::metadata(path).is_ok(),
            &self.servoshell_preferences,
        );

        let mut protocol_registry = ProtocolRegistry::default();
        let _ = protocol_registry.register(
            "urlinfo",
            protocols::urlinfo::UrlInfoProtocolHander::default(),
        );
        let _ =
            protocol_registry.register("servo", protocols::servo::ServoProtocolHandler::default());
        let _ = protocol_registry.register(
            "resource",
            protocols::resource::ResourceProtocolHandler::default(),
        );
        // Takes over from the engine's own internal `file:` handler — see
        // `components/servo/servo.rs`'s protocol-registry merge-order change,
        // without which this registration would silently be discarded.
        let initial_file_path = self.initial_url.as_url().to_file_path().ok();
        let _ = protocol_registry.register(
            "file",
            protocols::file::FileProtocolHandler::new(initial_file_path.as_deref()),
        );
        // `@drincs/roves-api`'s `core`/`process` modules talk to this — see
        // protocols/roves.rs. `None` in headless mode (no window, no winit
        // event loop to send AppEvent::CloseAllWindows through).
        let close_proxy = self
            .event_loop_proxy
            .clone()
            .map(|proxy| Arc::new(Mutex::new(proxy)));
        let _ = protocol_registry.register(
            "roves",
            protocols::roves::RovesProtocolHandler::new(close_proxy),
        );
        // Only registered with `--features steam` (see ports/servoshell/Cargo.toml)
        // — mirrors the parent project's Tauri `steam` feature. `SteamProtocolHandler::new()`
        // tries to init Steam once here; it degrades to "unavailable" answers rather
        // than failing when Steam isn't running (e.g. outside Steam, in CI).
        #[cfg(feature = "steam")]
        let _ =
            protocol_registry.register("steam", protocols::steam::SteamProtocolHandler::new());

        let servo_builder = ServoBuilder::default()
            .opts(self.opts.clone())
            .preferences(self.preferences.clone())
            .protocol_registry(protocol_registry)
            .event_loop_waker(self.waker.clone());

        #[cfg(feature = "webxr")]
        let servo_builder =
            servo_builder.webxr_registry(super::webxr::XrDiscoveryWebXrRegistry::new_boxed(
                platform_window.clone(),
                active_event_loop,
                &self.preferences,
            ));

        let servo = servo_builder.build();
        servo.setup_logging();

        let user_content_manager = Rc::new(UserContentManager::new(&servo));
        for script in load_userscripts(self.servoshell_preferences.userscripts_directory.as_deref())
            .expect("Loading userscripts failed")
        {
            user_content_manager.add_script(Rc::new(script));
        }

        for user_stylesheet in &self.servoshell_preferences.user_stylesheets {
            user_content_manager.add_stylesheet(user_stylesheet.clone());
        }

        let running_state = Rc::new(RunningAppState::new(
            servo,
            self.servoshell_preferences.clone(),
            self.waker.clone(),
            user_content_manager,
            self.preferences.clone(),
            #[cfg(feature = "gamepad")]
            ServoshellGamepadDelegate::maybe_new().map(Rc::new),
        ));
        running_state.open_window(platform_window, self.initial_url.as_url().clone());

        self.state = AppState::Running(running_state);
    }

    #[servo::servo_tracing::instrument(level = "debug", skip_all)]
    fn create_platform_window(
        &self,
        url: Url,
        active_event_loop: Option<&ActiveEventLoop>,
    ) -> Rc<dyn PlatformWindow> {
        assert_eq!(
            self.servoshell_preferences.headless,
            active_event_loop.is_none()
        );

        let Some(active_event_loop) = active_event_loop else {
            return HeadlessWindow::new(&self.servoshell_preferences);
        };

        HeadedWindow::new(
            &self.servoshell_preferences,
            active_event_loop,
            self.event_loop_proxy
                .clone()
                .expect("Should always have event loop proxy in headed mode."),
            url,
        )
    }

    pub fn pump_servo_event_loop(&mut self, active_event_loop: Option<&ActiveEventLoop>) -> bool {
        let AppState::Running(state) = &self.state else {
            return false;
        };

        let create_platform_window = |url: Url| self.create_platform_window(url, active_event_loop);
        if !state.spin_event_loop(Some(&create_platform_window)) {
            self.state = AppState::ShuttingDown;
            return false;
        }
        true
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(Some(event_loop));
    }

    /// Only used while `Booting`, to force one redraw once the splash's
    /// progress-bar delay (`SPLASH_PROGRESS_BAR_DELAY`) elapses even if no
    /// `AppEvent::BootProgress` tick has arrived yet — see `init`'s
    /// `ControlFlow::WaitUntil` and `window_event`'s Booting branch, which
    /// re-arms it until the bar is actually shown.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) &&
            let AppState::Booting { window, .. } = &self.state &&
            let Some(headed_window) = window.as_headed_window()
        {
            headed_window.winit_window().request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let now = Instant::now();
        trace_winit_event!(
            window_event,
            "@{:?} (+{:?}) {window_event:?}",
            now - self.t_start,
            now - self.t
        );
        self.t = now;

        if let AppState::Booting { window, extraction_started, progress } = &self.state {
            let elapsed = extraction_started.elapsed();
            if let Some(headed_window) = window.as_headed_window() &&
                headed_window.winit_window().id() == window_id &&
                matches!(window_event, WindowEvent::RedrawRequested | WindowEvent::Resized(_))
            {
                headed_window.paint_splash((elapsed >= SPLASH_PROGRESS_BAR_DELAY).then_some(*progress));
            }
            event_loop.set_control_flow(if elapsed >= SPLASH_PROGRESS_BAR_DELAY {
                ControlFlow::Wait
            } else {
                ControlFlow::WaitUntil(Instant::now() + (SPLASH_PROGRESS_BAR_DELAY - elapsed))
            });
            return;
        }

        let AppState::Running(state) = &self.state else {
            return;
        };

        if let Some(window) = state.window(ServoShellWindowId::from(u64::from(window_id))) &&
            let Some(headed_window) = window.platform_window().as_headed_window()
        {
            headed_window.handle_winit_window_event(state.clone(), window, window_event);
        }

        if !self.pump_servo_event_loop(event_loop.into()) {
            event_loop.exit();
        }
        // Block until the window gets an event
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, app_event: AppEvent) {
        let boot_ready_window = if let AppState::Booting { window, progress, .. } = &mut self.state {
            match app_event {
                AppEvent::BootProgress(new_progress) => {
                    *progress = new_progress;
                    if let Some(headed_window) = window.as_headed_window() {
                        headed_window.winit_window().request_redraw();
                    }
                    None
                },
                AppEvent::BootReady => Some(window.clone()),
                _ => None,
            }
        } else {
            None
        };
        if let Some(window) = boot_ready_window {
            self.finish_init(window, Some(event_loop));
            return;
        }
        if matches!(self.state, AppState::Booting { .. }) {
            return;
        }

        let AppState::Running(state) = &self.state else {
            return;
        };

        if matches!(app_event, AppEvent::CloseAllWindows) {
            // See protocols/roves.rs and event_loop.rs's own doc comment on
            // this variant: this is the only way a `ProtocolHandler` (which
            // must be `Send + Sync`, and runs off the main thread) can ask
            // the main thread to close windows, since `ServoShellWindow`
            // itself is `Rc`-based and can't be touched from there directly.
            for window in state.windows().values() {
                window.schedule_close();
            }
        } else if let Some(window) = app_event
            .window_id()
            .and_then(|window_id| state.window(ServoShellWindowId::from(u64::from(window_id)))) &&
            let Some(headed_window) = window.platform_window().as_headed_window()
        {
            headed_window.handle_winit_app_event(state.clone(), app_event);
        }

        if !self.pump_servo_event_loop(event_loop.into()) {
            event_loop.exit();
        }

        // Block until the window gets an event
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

fn load_userscripts(userscripts_directory: Option<&Path>) -> std::io::Result<Vec<UserScript>> {
    let mut userscripts = Vec::new();
    if let Some(userscripts_directory) = &userscripts_directory {
        let mut files = std::fs::read_dir(userscripts_directory)?
            .map(|e| e.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort_unstable();
        for file in files {
            let script = std::fs::read_to_string(&file)?;
            userscripts.push(UserScript::new(script, Some(file)));
        }
    }
    Ok(userscripts)
}
