/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! An event loop implementation that works in headless mode.
//!
//! SDL3-backed, replacing the previous winit-based implementation (see
//! CUSTOMIZATIONS.md's SDL3 windowing entry and TODO.md's "Finestra + event loop" section —
//! this is a work-in-progress migration, not a finished one). The public shape (`AppEvent`,
//! `ServoShellEventLoop`, `create_event_loop_waker`, `run_app`) is kept as close as possible to
//! the winit version so `app.rs`/`headed_window.rs`'s own logic needs minimal changes beyond
//! swapping which concrete types they name — only the actual event-pumping mechanism
//! underneath changes.

use std::cell::Cell;
use std::sync::Arc;
use std::time::{self, Duration, Instant};

use log::warn;
use servo::EventLoopWaker;

use super::app::App;

/// This module's own stand-in for `winit::window::WindowId` — SDL3 windows are identified by a
/// plain `u32` (`sdl3::video::Window::id()`, and every `sdl3::event::Event` variant that
/// targets a specific window carries the same `window_id: u32`), so there's no need for a
/// wrapping newtype the way winit's opaque `WindowId` has.
pub(crate) type WindowId = u32;

/// This module's own stand-in for `winit::event_loop::ControlFlow`. SDL3 has no equivalent
/// concept built in — `run_app`'s own loop (see below) reads this via `ActiveEventLoop` after
/// every dispatched event/tick and computes the actual `wait_event_timeout` duration from it,
/// the same job winit's own platform backend does internally for `ControlFlow::WaitUntil`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ControlFlow {
    Wait,
    WaitUntil(Instant),
}

/// This module's own stand-in for `winit::event_loop::ActiveEventLoop`. Holds just the two
/// pieces of live SDL3 state `app.rs`/`headed_window.rs` actually need a handle to: the
/// `VideoSubsystem` (window/monitor creation — see `HeadedWindow::new`) and the mutable
/// control-flow/exit state `App`'s dispatch methods set and `run_app`'s loop reads back.
pub(crate) struct ActiveEventLoop {
    sdl: sdl3::Sdl,
    video: sdl3::VideoSubsystem,
    control_flow: Cell<ControlFlow>,
    should_exit: Cell<bool>,
}

impl ActiveEventLoop {
    pub(crate) fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow);
    }

    pub(crate) fn exit(&self) {
        self.should_exit.set(true);
    }

    pub(crate) fn video(&self) -> &sdl3::VideoSubsystem {
        &self.video
    }

    pub(crate) fn sdl(&self) -> &sdl3::Sdl {
        &self.sdl
    }
}

/// This module's own stand-in for the subset of `winit::event::WindowEvent` that
/// `app.rs`/`headed_window.rs` actually consume — see `translate_sdl_event` for the mapping.
///
/// TODO(SDL3 windowing, real progress not completion): winit's `WindowEvent` has ~16 variants
/// in real use here (`grep -n "WindowEvent::" ports/servoshell/desktop/headed_window.rs`) —
/// mouse buttons/motion/wheel, keyboard input, IME composition, modifiers, touch, pinch
/// gesture, dropped files, theme/scale-factor changes. Only the handful needed to open a
/// window, show the boot splash, and close cleanly are ported so far. Porting the rest is
/// mechanical (`sdl3::event::Event` already carries equivalent data for nearly all of them —
/// see this module's own research notes in TODO.md) but is real, separate work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowEvent {
    Resized(u32, u32),
    CloseRequested,
    RedrawRequested,
    Focused(bool),
}

/// Translates one real (non-user) SDL3 event into this module's own `WindowEvent`, alongside
/// the `WindowId` it targets. Returns `None` for anything not ported yet (see `WindowEvent`'s
/// own doc comment) — `run_sdl3_app`'s loop simply skips those for now, the same as an app
/// event handler that doesn't match every variant.
fn translate_sdl_event(event: sdl3::event::Event) -> Option<(WindowId, WindowEvent)> {
    use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};
    match event {
        SdlEvent::Window { window_id, win_event, .. } => {
            let window_event = match win_event {
                SdlWindowEvent::Resized(width, height) => {
                    WindowEvent::Resized(width.max(0) as u32, height.max(0) as u32)
                },
                SdlWindowEvent::CloseRequested => WindowEvent::CloseRequested,
                SdlWindowEvent::Exposed => WindowEvent::RedrawRequested,
                SdlWindowEvent::FocusGained => WindowEvent::Focused(true),
                SdlWindowEvent::FocusLost => WindowEvent::Focused(false),
                _ => return None,
            };
            Some((window_id, window_event))
        },
        _ => None,
    }
}

/// This module's own stand-in for `winit::event_loop::EventLoopProxy<AppEvent>` — lets any
/// thread push an `AppEvent` onto the main SDL3 event queue, woken up promptly even if the
/// main thread is parked in `wait_event_timeout` (`SDL_PushEvent`, which `EventSender` wraps,
/// is documented thread-safe, same guarantee winit's own proxy makes). `sdl3::event::
/// EventSender` has no public constructor besides `EventSubsystem::event_sender()`/
/// `EventPump::event_sender()`, neither of which is itself `Send` — so a single `EventSender`
/// is created once on the main thread (`ServoShellEventLoop::headed`) and shared via `Arc`
/// (its own `push_event`/`push_custom_event` only need `&self`) rather than re-obtained per
/// clone the way winit's own, actually-`Clone`, proxy is.
#[derive(Clone)]
pub(crate) struct EventLoopProxy(Arc<sdl3::event::EventSender>);

impl EventLoopProxy {
    pub(crate) fn send_event(&self, event: AppEvent) -> Result<(), String> {
        self.0
            .push_custom_event(event)
            .map_err(|error| error.to_string())
    }
}

pub enum AppEvent {
    /// Another process or thread has kicked the OS event loop with EventLoopWaker.
    Waker,
    /// SDL3 has no push-based "redraw requested" event the way winit does — this is `App`'s
    /// own stand-in, requested via `HeadedWindow::request_redraw` (which owns an
    /// `EventLoopProxy` clone precisely so it can send this from anywhere, without every
    /// caller needing its own `&ActiveEventLoop` handle — several of the call sites this
    /// replaces are `HeadedWindow`'s own methods, not `App`'s). Dispatched by routing straight
    /// into the same window-event handling a real `WindowEvent::RedrawRequested` would have —
    /// see `App::dispatch_user_event`.
    RedrawRequested(WindowId),
    // TODO(SDL3 windowing): this still names `egui_winit`'s `accesskit_winit::Event` type —
    // the *type* is deliberately kept for now (see TODO.md's AccessKit de-risking note: it's
    // just a plain action-request/tree-request payload, not something that itself needs a
    // live winit window), but the `accesskit_winit::Adapter` that actually *produces* these by
    // watching a winit window is gone along with the rest of winit — nothing constructs this
    // variant anywhere yet. A real SDL3-backed AccessKit adapter (see TODO.md) needs to send
    // an equivalent event here before accessibility support works again.
    Accessibility(egui_winit::accesskit_winit::Event),
    /// Requested by `protocols::roves::RovesProtocolHandler` (the `roves:` scheme's
    /// `exit`/`close_window` command — see `@drincs/roves-api`'s `process.exit()`) from
    /// whatever thread is servicing that `fetch()`. `RunningAppState`/`ServoShellWindow`
    /// are `Rc`-based (main-thread-only), so a `ProtocolHandler` (`Send + Sync`, and run
    /// off-thread) can't call `schedule_close()` directly — it has to ask the main thread
    /// to do it instead, the same way `HeadedEventLoopWaker` already asks the main thread
    /// to wake up, via this same `EventLoopProxy`.
    CloseAllWindows,
    /// Sent by the background boot-extraction thread `App::init` spawns for a
    /// packed-content launch (see `bundle_launch.rs`'s `BundledLaunch`) as extraction
    /// proceeds, so the boot splash's progress bar can be repainted with real progress
    /// (0.0-1.0) instead of just spinning.
    BootProgress(f32),
    /// Sent once by that same background thread when boot extraction finishes (or fails —
    /// either way the boot URL's files are as ready as they'll get), so `App` can finish
    /// building Servo/`RunningAppState` and open the real webview. See `AppState::Booting`.
    BootReady,
    /// Requested by `protocols::roves::RovesProtocolHandler`'s `save_file` command (the
    /// desktop side of a game's save export — see `@drincs/roves-api`'s doc comments, and
    /// the injected `download`-interception userscript `app.rs` registers alongside
    /// `window.__ROVES__ = true;`) — same reasoning as `CloseAllWindows` above: the native
    /// "Save As" dialog this shows (`Dialog::SaveFile` in `dialog.rs`) has to run on the
    /// main thread, which a `ProtocolHandler` can't touch directly. Unlike `CloseAllWindows`
    /// this needs a reply: `response` carries the write result back to the still-pending
    /// `fetch()` once the user actually picks a destination (or cancels).
    SaveFileDialog {
        suggested_name: String,
        data: Vec<u8>,
        response: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
}

impl std::fmt::Debug for AppEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppEvent::Waker => write!(f, "Waker"),
            AppEvent::RedrawRequested(window_id) => {
                f.debug_tuple("RedrawRequested").field(window_id).finish()
            },
            AppEvent::Accessibility(event) => f.debug_tuple("Accessibility").field(event).finish(),
            AppEvent::CloseAllWindows => write!(f, "CloseAllWindows"),
            AppEvent::BootProgress(progress) => f.debug_tuple("BootProgress").field(progress).finish(),
            AppEvent::BootReady => write!(f, "BootReady"),
            // `tokio::sync::oneshot::Sender` isn't `Debug`, so this can't be derived —
            // `data`'s length stands in for its (likely uninteresting, possibly large)
            // bytes, same idea as not dumping a whole file's contents into a log line.
            AppEvent::SaveFileDialog { suggested_name, data, .. } => f
                .debug_struct("SaveFileDialog")
                .field("suggested_name", suggested_name)
                .field("data_len", &data.len())
                .finish(),
        }
    }
}

impl From<egui_winit::accesskit_winit::Event> for AppEvent {
    fn from(event: egui_winit::accesskit_winit::Event) -> AppEvent {
        AppEvent::Accessibility(event)
    }
}

/// A headed or headless event loop. Headless event loops are necessary for environments without a
/// display server. Ideally, we could use the headed SDL3 event loop in both modes, but on Linux,
/// the event loop requires a display server, which prevents running servoshell in a console.
pub(crate) enum ServoShellEventLoop {
    /// A real SDL3 windowing event loop.
    Sdl3 {
        sdl: sdl3::Sdl,
        video: sdl3::VideoSubsystem,
        event_subsystem: sdl3::EventSubsystem,
        proxy: EventLoopProxy,
    },
    /// A fake event loop which contains a signalling flag used to ensure
    /// that pending events get processed in a timely fashion, and a condition
    /// variable to allow waiting on that flag changing state.
    Headless(Arc<HeadlessEventLoop>),
}

impl ServoShellEventLoop {
    pub(crate) fn headless() -> ServoShellEventLoop {
        ServoShellEventLoop::Headless(Default::default())
    }

    pub(crate) fn headed() -> ServoShellEventLoop {
        let sdl = sdl3::init().expect("Could not initialize SDL3");
        let video = sdl.video().expect("Could not initialize SDL3 video subsystem");
        let event_subsystem = sdl.event().expect("Could not initialize SDL3 event subsystem");
        event_subsystem
            .register_custom_event::<AppEvent>()
            .expect("Could not register AppEvent as a custom SDL3 event type");
        let proxy = EventLoopProxy(Arc::new(event_subsystem.event_sender()));
        ServoShellEventLoop::Sdl3 { sdl, video, event_subsystem, proxy }
    }
}

impl ServoShellEventLoop {
    pub(crate) fn event_loop_proxy(&self) -> Option<EventLoopProxy> {
        match self {
            ServoShellEventLoop::Sdl3 { proxy, .. } => Some(proxy.clone()),
            ServoShellEventLoop::Headless(..) => None,
        }
    }

    pub fn create_event_loop_waker(&self) -> Box<dyn EventLoopWaker> {
        match self {
            ServoShellEventLoop::Sdl3 { proxy, .. } => Box::new(HeadedEventLoopWaker::new(proxy.clone())),
            ServoShellEventLoop::Headless(data) => Box::new(HeadlessEventLoopWaker(data.clone())),
        }
    }

    pub fn run_app(self, app: &mut App) {
        match self {
            ServoShellEventLoop::Sdl3 { sdl, video, .. } => {
                run_sdl3_app(sdl, video, app);
            },
            ServoShellEventLoop::Headless(event_loop) => event_loop.run_app(app),
        }
    }
}

/// Drives `app` from a real SDL3 event queue — the SDL3 equivalent of
/// `winit::event_loop::EventLoop::run_app`. Unlike winit (whose platform backend hides its own
/// equivalent of this loop), this is plain, explicit code: `wait_event_timeout` blocks the
/// main thread until either a real event arrives or `control_flow`'s deadline passes (mirroring
/// `ControlFlow::Wait`/`WaitUntil`), each event is translated and dispatched to `app`, and the
/// loop exits once `ActiveEventLoop::exit()` has been called.
///
/// TODO(SDL3 windowing, real progress not completion — see TODO.md): only enough of SDL3's own
/// `Event` enum is translated here to drive `App`'s own state machine (window creation,
/// booting, shutdown) — the *rest* of each event's payload (mouse/keyboard/IME/gesture data
/// `headed_window.rs::handle_winit_window_event` used to consume) is not ported yet. A real
/// window currently opens and can be closed, but does not yet accept input.
fn run_sdl3_app(sdl: sdl3::Sdl, video: sdl3::VideoSubsystem, app: &mut App) {
    let event_loop = ActiveEventLoop {
        sdl,
        video,
        control_flow: Cell::new(ControlFlow::Wait),
        should_exit: Cell::new(false),
    };
    let mut event_pump = event_loop
        .sdl()
        .event_pump()
        .expect("Could not create SDL3 event pump");

    app.init(Some(&event_loop));
    app.dispatch_new_events(&event_loop);

    loop {
        if event_loop.should_exit.get() {
            return;
        }

        let timeout = match event_loop.control_flow.get() {
            ControlFlow::Wait => None,
            ControlFlow::WaitUntil(deadline) => Some(deadline.saturating_duration_since(Instant::now())),
        };
        let event = match timeout {
            Some(timeout) => event_pump.wait_event_timeout(timeout),
            None => Some(event_pump.wait_event()),
        };

        let Some(event) = event else {
            // Timed out waiting for `ControlFlow::WaitUntil`'s deadline -- same as winit's own
            // `StartCause::ResumeTimeReached`.
            app.dispatch_new_events(&event_loop);
            continue;
        };

        if event.is_user_event() {
            if let Some(app_event) = event.as_user_event_type::<AppEvent>() {
                app.dispatch_user_event(&event_loop, app_event);
            }
            continue;
        }

        if let Some((window_id, window_event)) = translate_sdl_event(event) {
            app.dispatch_window_event(&event_loop, window_id, window_event);
        }

        if event_loop.should_exit.get() {
            return;
        }
    }
}

#[derive(Clone)]
struct HeadedEventLoopWaker {
    proxy: EventLoopProxy,
}

impl HeadedEventLoopWaker {
    fn new(proxy: EventLoopProxy) -> HeadedEventLoopWaker {
        HeadedEventLoopWaker { proxy }
    }
}

impl EventLoopWaker for HeadedEventLoopWaker {
    fn wake(&self) {
        // Kick the OS event loop awake.
        if let Err(err) = self.proxy.send_event(AppEvent::Waker) {
            warn!("Failed to wake up event loop ({}).", err);
        }
    }

    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
}

/// The [`HeadlessEventLoop`] is used when running in headless mode. The event
/// loop just loops over a condvar to simulate a real windowing system event loop.
#[derive(Default)]
pub(crate) struct HeadlessEventLoop {
    guard: Arc<std::sync::Mutex<bool>>,
    condvar: std::sync::Condvar,
}

impl HeadlessEventLoop {
    fn run_app(&self, app: &mut App) {
        app.init(None);

        loop {
            self.sleep();
            if !app.pump_servo_event_loop(None) {
                break;
            }
            *self.guard.lock().unwrap() = false;
        }
    }

    fn sleep(&self) {
        // To avoid sleeping when we should be processing events, do two things:
        // * before sleeping, check whether our signalling flag has been set
        // * wait on a condition variable with a maximum timeout, to allow
        //   being woken up by any signals that occur while sleeping.
        let guard = self.guard.lock().unwrap();
        if *guard {
            return;
        }
        let _ = self
            .condvar
            .wait_timeout(guard, time::Duration::from_millis(5))
            .unwrap();
    }
}

#[derive(Clone)]
struct HeadlessEventLoopWaker(Arc<HeadlessEventLoop>);

impl EventLoopWaker for HeadlessEventLoopWaker {
    fn wake(&self) {
        // Set the signalling flag and notify the condition variable.
        // This ensures that any sleep operation is interrupted,
        // and any non-sleeping operation will have a change to check
        // the flag before going to sleep.
        let mut flag = self.0.guard.lock().unwrap();
        *flag = true;
        self.0.condvar.notify_all();
    }

    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
}
