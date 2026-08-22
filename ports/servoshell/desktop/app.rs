/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Application entry point, runs the event loop.

use std::path::{Path, PathBuf};
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
use crate::desktop::headed_window::{HeadedWindow, SPLASH_ANIMATION_TICK};
use crate::desktop::headless_window::HeadlessWindow;
use crate::desktop::protocols;
use crate::desktop::tracing::trace_winit_event;
use crate::parser::get_default_url;
use crate::prefs::ServoShellPreferences;
use crate::running_app_state::RunningAppState;
#[cfg(feature = "gamepad")]
use crate::running_app_state::ServoshellGamepadDelegate;
use crate::window::{PlatformWindow, ServoShellWindowId};

/// Minimum time every headed launch spends on the boot splash before
/// `finish_init` runs (building Servo, opening the real `WebView`),
/// regardless of whether there's any boot extraction to wait on. Without
/// this, a launch with nothing to extract — a dev `--url` run, or (the
/// common case after the very first launch) a packed-content launch whose
/// destination is already cached from a previous run — skipped
/// `AppState::Booting` entirely: the window's very first frame *is* the
/// branded splash (see `Gui::new`), but the next frame immediately swapped
/// in the real, still-loading `WebView`. Holding the splash up for a fixed
/// minimum makes it long enough to actually register on every launch, not
/// just a slow first one. This is only half of what keeps the splash from
/// flashing away too early, though — see `HeadedWindow::begin_page_load_splash`
/// for the other half: once `finish_init` *does* open the real `WebView`, the
/// splash stays up still further, until that page is actually ready.
const MIN_SPLASH_DURATION: Duration = Duration::from_millis(500);

pub(crate) enum AppState {
    Initializing,
    /// A headed launch that hasn't finished starting up yet — `window` is
    /// already created and showing the boot splash (see
    /// `HeadedWindow::paint_splash`) while, if there's a pending
    /// packed-content boot extraction, a background thread (spawned from
    /// `App::init`) decompresses the boot set. `extraction_started` drives
    /// both `MIN_SPLASH_DURATION` and the splash's animation clock (passed
    /// to `paint_splash` as elapsed time — see that function's doc comment
    /// for why it's no longer a completion fraction). `extraction_done`
    /// starts `true` when there was no pending extraction to begin with
    /// (nothing to wait on but `MIN_SPLASH_DURATION`), or flips to `true`
    /// once `AppEvent::BootReady` arrives. `App::try_finish_booting` is what
    /// actually decides, from these fields, when to hand off to
    /// `finish_init` — which, for a headed launch, keeps the splash up
    /// still further (see `MIN_SPLASH_DURATION`'s own doc comment) rather
    /// than transitioning straight to `Running`.
    Booting {
        window: Rc<dyn PlatformWindow>,
        extraction_started: Instant,
        extraction_done: bool,
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
    /// The managed extraction-cache directory a packed-content launch's boot
    /// content lives (or will live) in — captured from `pending_extraction`
    /// before `init` takes and consumes it, so `finish_init` still has it
    /// available for `roves:clear_content_cache` (see `content_cache_dir`
    /// below). `None` for anything that isn't a packed-content launch (a dev
    /// run, or an uncompressed `--content-compress=none` bundle), so that
    /// case correctly reports "not a packed-content launch" instead of
    /// pointing `clear_content_cache` at the bundle's own loose content
    /// folder — which is real, on-disk content, not a cache, and must never
    /// be deleted.
    packed_content_dest: Option<PathBuf>,
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
            packed_content_dest: pending_boot_extraction.as_ref().and_then(|opts| opts.dest.clone()),
            pending_extraction: pending_boot_extraction,
            t_start: t,
            t,
            state: AppState::Initializing,
        }
    }

    /// Initialize Application once event loop start running. Always creates
    /// the window immediately. For a headed launch, always shows the boot
    /// splash for at least `MIN_SPLASH_DURATION` before deferring the rest
    /// of startup (`finish_init`) — if there's also a pending packed-content
    /// boot extraction, that runs concurrently on a background thread and
    /// must finish too (see `AppState::Booting`, `try_finish_booting`)
    /// instead of blocking window creation on it, as a plain synchronous
    /// call used to. Headless launches have no splash to show, so a pending
    /// extraction there just runs synchronously, exactly as before.
    pub fn init(&mut self, active_event_loop: Option<&ActiveEventLoop>) {
        // `self.initial_url` is still just the `finish_init`-pending placeholder
        // at this point — harmless here, since `create_platform_window`'s `url`
        // param is only ever forwarded to `Gui::new`'s own dead `_initial_url`
        // parameter (see CUSTOMIZATIONS.md), never actually used.
        let url = self.initial_url.as_url().clone();
        // See the milestone-logging comment in `cli::main` — window/GL
        // surface creation (winit + ANGLE/GL context setup) is the single
        // most likely place for a silent, unloggable native crash on
        // Windows, so this is bracketed specifically rather than just
        // logged once before/after everything in `init`.
        log::info!("creating platform window");
        let platform_window = self.create_platform_window(url, active_event_loop);
        log::info!("created platform window");

        let opts = self.pending_extraction.take();

        let Some(active_event_loop) = active_event_loop else {
            // Headless: no splash to show either way, so just run any
            // pending extraction synchronously and move straight on, same
            // as before this function also started gating headed launches
            // on `MIN_SPLASH_DURATION`.
            if let Some(opts) = opts {
                if let Err(error) = extract::extract_boot(&opts) {
                    log::error!("extracting boot content: {error}");
                }
            }
            self.finish_init(platform_window, None);
            return;
        };

        // Show the splash immediately, then make sure we wake up again at the next
        // animation tick even if no `BootProgress`/`BootReady` arrives before then — see
        // `new_events`/`try_finish_booting`, which take over re-arming this on every
        // subsequent tick.
        if let Some(headed_window) = platform_window.as_headed_window() {
            headed_window.winit_window().request_redraw();
        }
        active_event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + SPLASH_ANIMATION_TICK));

        let extraction_done = match opts {
            Some(opts) => {
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
                    // Sent even on failure: the page will simply fail to load
                    // (same failure philosophy as elsewhere for missing/
                    // corrupt content) rather than leaving the app stuck on
                    // the splash forever.
                    let _ = proxy.send_event(AppEvent::BootReady);
                });
                false
            },
            // Nothing to extract — the only thing `try_finish_booting` still
            // needs to wait on is `MIN_SPLASH_DURATION` itself.
            None => true,
        };
        self.state = AppState::Booting {
            window: platform_window,
            extraction_started: Instant::now(),
            extraction_done,
        };
    }

    /// If `Booting`, check whether both extraction (if any was pending) and
    /// `MIN_SPLASH_DURATION` have finished; if so, hand off to
    /// `finish_init`. Otherwise (re)arm `control_flow` to wake up at the next
    /// `SPLASH_ANIMATION_TICK` (so the splash's indeterminate animation keeps
    /// moving the whole time, not just once at the `MIN_SPLASH_DURATION`
    /// mark), capped so it never overshoots that mark on the first tick. A
    /// no-op when not `Booting`, so every event handler below can call this
    /// unconditionally.
    fn try_finish_booting(&mut self, event_loop: &ActiveEventLoop) {
        let AppState::Booting { window, extraction_started, extraction_done, .. } = &self.state
        else {
            return;
        };
        let elapsed = extraction_started.elapsed();
        if !*extraction_done || elapsed < MIN_SPLASH_DURATION {
            // Once past `MIN_SPLASH_DURATION` itself, `saturating_sub` alone would
            // return zero and busy-loop while still waiting on `extraction_done` — tick
            // at the animation rate instead in that case, same as while still within
            // `MIN_SPLASH_DURATION` but capped so the first tick doesn't overshoot it.
            let remaining = MIN_SPLASH_DURATION.saturating_sub(elapsed);
            let tick = if remaining.is_zero() {
                SPLASH_ANIMATION_TICK
            } else {
                remaining.min(SPLASH_ANIMATION_TICK)
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + tick));
            return;
        }
        let window = window.clone();
        self.finish_init(window, Some(event_loop));
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
        // `roves:clear_content_cache` needs to know what to delete —
        // `self.packed_content_dest` (see its own doc comment), not
        // `initial_file_path`'s parent: for an uncompressed bundle, that
        // parent is the bundle's own loose content folder, not a cache Roves
        // ever created, and must never be a candidate for deletion.
        let _ = protocol_registry.register(
            "roves",
            protocols::roves::RovesProtocolHandler::new(close_proxy, self.packed_content_dest.clone()),
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

        log::info!("building Servo instance");
        let servo = servo_builder.build();
        log::info!("built Servo instance");
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

        // Keep the boot splash up (see its own doc comment) instead of immediately
        // handing off to a page that hasn't painted anything of its own yet — a no-op
        // for headless, which has no splash/window to begin with.
        if let Some(headed_window) = platform_window.as_headed_window() {
            headed_window.begin_page_load_splash();
        }
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

    /// Drives the two periodic ticks the boot splash's animation relies on, neither of
    /// which any real `WindowEvent`/`AppEvent` is guaranteed to otherwise deliver
    /// promptly (or at all, during an otherwise-silent wait):
    /// - While `Booting`: forces a redraw at each `SPLASH_ANIMATION_TICK` (see
    ///   `try_finish_booting`, which arms this), then defers to `try_finish_booting`
    ///   itself — also what transitions to `finish_init` once both extraction and
    ///   `MIN_SPLASH_DURATION` are done.
    /// - While `Running` with a window whose `page_load_splash_since` is still set (see
    ///   `HeadedWindow::splash_animation_wake_deadline`, armed by `window_event`/
    ///   `user_event`'s tail): forces that window's next redraw too, both to keep the
    ///   splash animating and to promptly notice `LoadStatus::Complete` firing with no
    ///   further page activity after it.
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if !matches!(cause, StartCause::ResumeTimeReached { .. }) {
            return;
        }
        match &self.state {
            AppState::Booting { window, .. } => {
                if let Some(headed_window) = window.as_headed_window() {
                    headed_window.winit_window().request_redraw();
                }
                self.try_finish_booting(event_loop);
            },
            AppState::Running(state) => {
                for window in state.windows().values() {
                    if let Some(headed_window) = window.platform_window().as_headed_window() &&
                        headed_window.splash_animation_wake_deadline(state).is_some()
                    {
                        headed_window.winit_window().request_redraw();
                    }
                }
            },
            _ => {},
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

        if matches!(self.state, AppState::Booting { .. }) {
            if let AppState::Booting { window, extraction_started, .. } = &self.state &&
                let Some(headed_window) = window.as_headed_window() &&
                headed_window.winit_window().id() == window_id &&
                matches!(window_event, WindowEvent::RedrawRequested | WindowEvent::Resized(_))
            {
                headed_window.paint_splash(extraction_started.elapsed());
            }
            self.try_finish_booting(event_loop);
            return;
        }

        let AppState::Running(state) = &self.state else {
            return;
        };
        // Cloned out of `self.state`'s borrow (cheap — `Rc` clone) so it's still usable
        // below, after `self.pump_servo_event_loop` needs `&mut self`.
        let state = state.clone();

        if let Some(window) = state.window(ServoShellWindowId::from(u64::from(window_id))) &&
            let Some(headed_window) = window.platform_window().as_headed_window()
        {
            headed_window.handle_winit_window_event(state.clone(), window, window_event);
        }

        if !self.pump_servo_event_loop(event_loop.into()) {
            event_loop.exit();
        }
        set_running_control_flow(event_loop, &state);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, app_event: AppEvent) {
        let mut redraw_window = None;
        if let AppState::Booting { window, extraction_done, .. } = &mut self.state {
            match app_event {
                AppEvent::BootProgress(_) => {
                    redraw_window = Some(window.clone());
                },
                AppEvent::BootReady => *extraction_done = true,
                _ => {},
            }
        }
        if let Some(window) = redraw_window &&
            let Some(headed_window) = window.as_headed_window()
        {
            headed_window.winit_window().request_redraw();
        }
        if matches!(self.state, AppState::Booting { .. }) {
            self.try_finish_booting(event_loop);
            return;
        }

        let AppState::Running(state) = &self.state else {
            return;
        };
        // Cloned out of `self.state`'s borrow (cheap — `Rc` clone) so it's still usable
        // below, after `self.pump_servo_event_loop` needs `&mut self`.
        let state = state.clone();

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

        set_running_control_flow(event_loop, &state);
    }
}

/// Sets `control_flow` to keep the event loop ticking at `SPLASH_ANIMATION_TICK` if any
/// window's boot splash is still covering its still-loading real page (see
/// `HeadedWindow::splash_animation_wake_deadline`) — otherwise, fully idle
/// (`ControlFlow::Wait`) until the next real event, same as before this splash-overlay
/// mechanism existed. Shared by `window_event`/`user_event`'s `Running` tails.
fn set_running_control_flow(event_loop: &ActiveEventLoop, state: &RunningAppState) {
    let next_wake = state
        .windows()
        .values()
        .filter_map(|window| {
            window
                .platform_window()
                .as_headed_window()?
                .splash_animation_wake_deadline(state)
        })
        .min();
    event_loop.set_control_flow(next_wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
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
