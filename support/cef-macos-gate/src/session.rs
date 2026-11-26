//! Platform-neutral glue of the macOS and Linux packaged-game loops (`player.rs`; the Linux
//! gate compiles this file through `#[path]`), built on the Windows player's shared modules:
//!
//! - page markers (README "Host API"): `window.__ROVES__` registered through welding's
//!   in-process DevTools channel before the game document, exactly as on Windows
//!   (`page_init.rs`, script `mcp_rules::page_marker_script`): the browser starts on
//!   `about:blank` and the game URL is loaded once the registration settled. The bootstrap
//!   document's navigation events are skipped by the loops (`bootstrap_event`);
//! - page input (README "Input"): SDL3 mouse, wheel, keyboard and committed text, focus,
//!   visibility and cursor shape through `input.rs`, with this platform's key codes
//!   (`keys.rs`);
//! - the optional local MCP bridge (Windows player README "Local MCP bridge",
//!   `support/cef-netrender-probe/mcp/README.md`): the Windows player's `mcp.rs`, started
//!   here before the browser when `--roves-mcp` or the package marker asked for it. Its
//!   requests run in this loop: `state` from this session, `evaluate` over the same DevTools
//!   channel as the page markers (which then also define `window.__ROVES_MCP__`), and
//!   `click`/`press_key`/`type_text` through the same `Input` as SDL events, once the game
//!   load-end was accepted (as real input);
//! - the test-only input hook `ROVES_GATE_TEST_INPUT_MS=<ms>` (CI): that long after the
//!   accepted game load-end, one synthetic left click at `TEST_CLICK_POINT`, one ArrowRight
//!   press and one wheel notch are pushed into SDL's queue and come back through the ordinary
//!   poll -> `Input` -> welding path; the page must report them with the Windows probe's
//!   console markers (`game_rules` click/key/wheel sequences, run side by side here).

use std::time::{Duration, Instant};

use sdl3::{
    EventSubsystem,
    event::Event,
    keyboard::{Keycode, Mod, Scancode},
    mouse::{MouseButton, MouseUtil, MouseWheelDirection},
    video::Window,
};
use serde_json::{Value, json};
use welding::CefSurfaceProducer;

use crate::game_rules::{
    ClickAction, ClickInput, ClickTest, KeyAction, KeyInput, KeyTest, TEST_CLICK_MOUSE_ID,
    TEST_CLICK_POINT, TEST_KEY_KEYBOARD_ID, TEST_KEY_MAX_DELAY_MS, TEST_WHEEL_LINES,
    TEST_WHEEL_MOUSE_ID, WheelAction, WheelInput, WheelMarker, WheelTest, is_click_marker,
    key_marker, on_origin, wheel_marker,
};
use crate::input::Input;
use crate::mcp::{Bridge, Call, Source};
use crate::page_init::PageInit;

/// Test-only environment variable of the synthetic input hook (milliseconds after load-end).
const TEST_INPUT_ENV: &str = "ROVES_GATE_TEST_INPUT_MS";

struct InputTest {
    delay: Duration,
    click: ClickTest,
    key: KeyTest,
    wheel: WheelTest,
}

pub(crate) struct Session {
    origin: String,
    game_url: String,
    page_init: Option<PageInit>,
    /// The game URL was requested (after the marker registration settled).
    navigated: bool,
    /// A navigation event on the game origin arrived: later `about:blank` events are not the
    /// bootstrap document's.
    game_started: bool,
    loaded_at: Option<Instant>,
    pub(crate) input: Input,
    test: Option<InputTest>,
    /// The local MCP bridge, when asked for and started.
    mcp: Option<Bridge>,
    pub(crate) failed: bool,
}

/// What the loop knows of the frames, for the MCP `state`.
pub(crate) struct Frames {
    /// Frames imported so far.
    pub(crate) imported: u32,
    /// The first game frame is on screen (startup screen released).
    pub(crate) shown: bool,
}

impl Session {
    /// `origin`: the game's loopback origin (`http://127.0.0.1:<port>`); `mcp`: whether the
    /// local MCP bridge was asked for (it is started here, before the browser exists).
    pub(crate) fn new(origin: &str, mcp: Option<Source>) -> Result<Session, String> {
        let test = match std::env::var(TEST_INPUT_ENV) {
            Ok(value) => {
                let ms = value
                    .parse::<u64>()
                    .ok()
                    .filter(|ms| *ms <= TEST_KEY_MAX_DELAY_MS)
                    .ok_or_else(|| {
                        format!("{TEST_INPUT_ENV}={value}: not a whole number <= {TEST_KEY_MAX_DELAY_MS}")
                    })?;
                Some(InputTest {
                    delay: Duration::from_millis(ms),
                    click: ClickTest::Waiting,
                    key: KeyTest::Waiting,
                    wheel: WheelTest::Waiting,
                })
            },
            Err(_) => None,
        };
        let mcp = mcp.and_then(|source| match crate::mcp::start(origin) {
            Ok(bridge) => {
                // The token stays in the session record only; it is never logged.
                println!(
                    "gate: mcp enabled source={source:?} pipe={} record={}",
                    bridge.pipe,
                    bridge.record.display()
                );
                Some(bridge)
            },
            Err(error) => {
                println!("gate: mcp unavailable source={source:?} error={error}");
                None
            },
        });
        let page_init =
            crate::mcp_rules::page_marker_script(origin, mcp.is_some()).map(PageInit::new);
        if page_init.is_none() {
            println!("gate: page markers unavailable error=unexpected origin {origin}");
        }
        Ok(Session {
            origin: origin.to_owned(),
            game_url: format!("{origin}/"),
            page_init,
            navigated: false,
            game_started: false,
            loaded_at: None,
            input: Input::new(),
            test,
            mcp,
            failed: false,
        })
    }

    /// The browser's first URL: `about:blank` while the page markers are registered, the game
    /// otherwise.
    pub(crate) fn initial_url(&self) -> String {
        if self.page_init.is_some() {
            crate::mcp_rules::BOOTSTRAP_URL.to_owned()
        } else {
            self.game_url.clone()
        }
    }

    /// Whether the producer needs welding's DevTools channel (`devtools_protocol`).
    pub(crate) fn devtools(&self) -> bool {
        self.page_init.is_some()
    }

    /// One loop turn of the marker registration; loads the game URL once it settled.
    pub(crate) fn devtools_step<P: CefSurfaceProducer>(&mut self, producer: &mut P) {
        let Some(init) = self.page_init.as_mut() else {
            if !self.navigated {
                // No markers: the game URL was the initial URL.
                self.navigated = true;
            }
            return;
        };
        while let Some(message) = producer.poll_devtools_message() {
            let Ok(message) = serde_json::from_str::<Value>(&message) else {
                continue;
            };
            if !init.handle(&message)
                && let Some(bridge) = self.mcp.as_mut()
            {
                bridge.devtools_message(producer, &message);
            }
        }
        if let Some(bridge) = self.mcp.as_mut() {
            bridge.expire(producer);
        }
        if self.navigated {
            return;
        }
        let Some(outcome) = init.step(producer) else {
            return;
        };
        self.navigated = true;
        let mcp = self.mcp.is_some();
        match outcome {
            Ok(identifier) => {
                println!("gate: page markers registered identifier={identifier} mcp={mcp}")
            },
            Err(error) => println!("gate: page markers unavailable mcp={mcp} error={error}"),
        }
        match producer.navigate_to_url(&self.game_url) {
            Ok(()) => println!("gate: game navigation url={}", self.game_url),
            Err(error) => {
                println!(
                    "gate: FAIL game navigation url={} error={error}",
                    self.game_url
                );
                self.failed = true;
            },
        }
    }

    /// Whether a navigation event for `url` belongs to the `about:blank` bootstrap document
    /// (the loop skips it; the origin checks apply to everything else).
    pub(crate) fn bootstrap_event(&mut self, url: &str) -> bool {
        if self.page_init.is_some() && on_origin(&self.origin, url) {
            self.game_started = true;
        }
        let bootstrap = self.page_init.is_some()
            && !self.game_started
            && url == crate::mcp_rules::BOOTSTRAP_URL;
        if !bootstrap && let Some(bridge) = self.mcp.as_mut() {
            bridge.url = Some(url.to_owned());
        }
        bootstrap
    }

    /// Whether the page title may become the window title (the game document only).
    pub(crate) fn game_document(&self) -> bool {
        self.page_init.is_none() || self.game_started
    }

    /// A title of the game document (the MCP `state`).
    pub(crate) fn title(&mut self, title: &str) {
        if let Some(bridge) = self.mcp.as_mut() {
            bridge.title = Some(title.to_owned());
        }
    }

    /// The accepted game load-end: page input is forwarded from now on.
    pub(crate) fn loaded(&mut self) {
        self.loaded_at.get_or_insert_with(Instant::now);
    }

    /// One SDL event: page input through `Input`, attributed to the test hook when it is one
    /// of its pushed events.
    pub(crate) fn sdl_event<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        window: &Window,
        event: &Event,
    ) {
        let testing = self.test.is_some();
        let click = match event {
            Event::MouseButtonDown { which, .. } if testing && *which == TEST_CLICK_MOUSE_ID => {
                Some(true)
            },
            Event::MouseButtonUp { which, .. } if testing && *which == TEST_CLICK_MOUSE_ID => {
                Some(false)
            },
            _ => None,
        };
        let key = match event {
            Event::KeyDown { which, .. } if testing && *which == TEST_KEY_KEYBOARD_ID => Some(true),
            Event::KeyUp { which, .. } if testing && *which == TEST_KEY_KEYBOARD_ID => Some(false),
            _ => None,
        };
        let wheel = testing
            && matches!(event, Event::MouseWheel { which, .. } if *which == TEST_WHEEL_MOUSE_ID);
        if key.is_some() {
            self.input.last_key = None;
        }
        if wheel {
            self.input.last_wheel = None;
        }
        let before = self.input.forwarded;
        if let Some(line) =
            self.input
                .window_event(producer, window, event, self.loaded_at.is_some())
        {
            println!("gate: {line}");
        }
        let ok = self.input.forwarded > before;
        let Some(test) = self.test.as_mut() else {
            return;
        };
        if let Some(down) = click {
            println!(
                "gate: test click forwarded event={} ok={ok}",
                if down { "down" } else { "up" }
            );
            let input = if down {
                ClickInput::Down { ok }
            } else {
                ClickInput::Up { ok }
            };
            let action = test.click.input(input);
            self.click_action(action);
        } else if let Some(down) = key {
            let codes = self
                .input
                .last_key
                .take()
                .map_or_else(|| "none".to_owned(), |codes| codes.to_string());
            println!(
                "gate: test key forwarded event={} ok={ok} {codes}",
                if down { "down" } else { "up" }
            );
            let input = if down {
                KeyInput::Down { ok }
            } else {
                KeyInput::Up { ok }
            };
            let action = test.key.input(input);
            self.key_action(action);
        } else if wheel {
            let delta = self.input.last_wheel.take();
            println!("gate: test wheel forwarded ok={ok} delta={delta:?}");
            let action = test.wheel.input(WheelInput { ok, delta });
            self.wheel_action(action);
        }
    }

    /// A console message of the page: kept for the MCP `state`; the test hook's markers,
    /// from the game origin only.
    pub(crate) fn console(&mut self, level: i32, source: &str, line: i32, message: &str) {
        if let Some(bridge) = self.mcp.as_mut() {
            bridge.console(level, message, source, line);
        }
        if self.test.is_none() || !on_origin(&self.origin, source) {
            return;
        }
        if is_click_marker(message) {
            println!("gate: test click marker received");
            let action = self
                .test
                .as_mut()
                .map_or(ClickAction::None, |t| t.click.marker());
            self.click_action(action);
        } else if let Some(marker) = key_marker(message) {
            println!("gate: test key marker received kind={marker:?}");
            let action = self
                .test
                .as_mut()
                .map_or(KeyAction::None, |t| t.key.marker(marker));
            self.key_action(action);
        } else if let Some(marker) = wheel_marker(message) {
            match marker {
                WheelMarker::Expected {
                    delta_y,
                    delta_mode,
                } => println!(
                    "gate: test wheel marker received delta_y={delta_y} delta_mode={delta_mode}"
                ),
                WheelMarker::Unexpected(why) => {
                    println!("gate: test wheel marker received unexpected={why:?}")
                },
            }
            let action = self
                .test
                .as_mut()
                .map_or(WheelAction::None, |t| t.wheel.marker(marker));
            self.wheel_action(action);
        }
    }

    /// Per loop turn: deferred focus, cursor shape and the test hook's schedule.
    pub(crate) fn per_turn<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        mouse: &MouseUtil,
        events: &EventSubsystem,
        window: &Window,
    ) {
        if let Some(line) = self
            .input
            .per_frame(producer, mouse, self.loaded_at.is_some())
        {
            println!("gate: {line}");
        }
        let (Some(test), Some(loaded)) = (self.test.as_mut(), self.loaded_at) else {
            return;
        };
        let now = Instant::now();
        let due = loaded.elapsed() >= test.delay;
        let click = test.click.tick(now, due);
        let key = test.key.tick(now, due);
        let wheel = test.wheel.tick(now, due);
        if click == ClickAction::Push {
            let pushed = push_click(events, window.id());
            let (x, y) = TEST_CLICK_POINT;
            println!(
                "gate: test click requested x={x} y={y} pushed={}",
                pushed.is_ok()
            );
            if let Err(error) = pushed {
                println!("gate: FAIL test click: SDL_PushEvent failed: {error}");
                self.failed = true;
            }
        } else {
            self.click_action(click);
        }
        if key == KeyAction::Push {
            let pushed = push_key(events, window.id());
            println!(
                "gate: test key requested key=Right pushed={}",
                pushed.is_ok()
            );
            if let Err(error) = pushed {
                println!("gate: FAIL test key: SDL_PushEvent failed: {error}");
                self.failed = true;
            }
        } else {
            self.key_action(key);
        }
        if wheel == WheelAction::Push {
            let pushed = push_wheel(events, window.id());
            println!("gate: test wheel requested pushed={}", pushed.is_ok());
            if let Err(error) = pushed {
                println!("gate: FAIL test wheel: SDL_PushEvent failed: {error}");
                self.failed = true;
            }
        } else {
            self.wheel_action(wheel);
        }
    }

    /// Whether the test hook (if any) finished, so a test auto-close may run.
    pub(crate) fn tests_finished(&self) -> bool {
        self.test
            .as_ref()
            .is_none_or(|t| t.click.finished() && t.key.finished() && t.wheel.finished())
    }

    /// One loop turn of the MCP bridge: a few queued requests.
    pub(crate) fn mcp_step<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        window: &Window,
        frames: &Frames,
    ) {
        let Some(bridge) = self.mcp.as_mut() else {
            return;
        };
        for (call, reply) in bridge.take(crate::mcp::CALLS_PER_TURN) {
            let method = call.method();
            let ready = self.loaded_at.is_some();
            let result = match call {
                Call::State => Ok(self.mcp_state(window, frames)),
                Call::Evaluate(_) | Call::Screenshot(_) if !self.navigated => {
                    Err("the game page is not loaded yet".to_owned())
                },
                Call::Evaluate(expression) => {
                    if let Some(bridge) = self.mcp.as_mut() {
                        bridge.evaluate(producer, &expression, reply);
                        println!("gate: mcp call method={method} queued=true");
                    }
                    continue;
                },
                Call::Screenshot(shot) => {
                    if let Some(bridge) = self.mcp.as_mut() {
                        let (width, height) = window.size();
                        let view = (f64::from(width), f64::from(height));
                        let dpr = f64::from(window.display_scale());
                        bridge.screenshot(producer, &shot, view, dpr, reply);
                        println!("gate: mcp call method={method} queued=true");
                    }
                    continue;
                },
                // Page input starts with the accepted game load-end, as for real input.
                Call::Click { .. } | Call::PressKey(_) | Call::TypeText(_) if !ready => {
                    Err("input is not ready: the game has not finished loading".to_owned())
                },
                Call::Click { x, y } => crate::mcp::click_events(window, x, y)
                    .and_then(|events| self.mcp_input(producer, window, &events)),
                Call::PressKey(key) => {
                    let events = crate::mcp::key_events(window.id(), key);
                    self.mcp_input(producer, window, &events)
                },
                Call::TypeText(text) => {
                    let events = [crate::mcp::text_event(window.id(), text)];
                    self.mcp_input(producer, window, &events)
                },
            };
            println!("gate: mcp call method={method} ok={}", result.is_ok());
            reply.send(result);
        }
    }

    fn mcp_input<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        window: &Window,
        events: &[Event],
    ) -> Result<Value, String> {
        crate::mcp::forward(&mut self.input, producer, window, events, |line| {
            println!("gate: {line}")
        })
    }

    /// Host-side state for `state` (the Windows player's fields that exist here): no page
    /// script runs for it.
    fn mcp_state(&self, window: &Window, frames: &Frames) -> Value {
        let mcp = self.mcp.as_ref();
        let (width, height) = window.size();
        let (pixel_width, pixel_height) = window.size_in_pixels();
        json!({
            "pid": std::process::id(),
            "launch": "package",
            "origin": self.origin,
            "url": mcp.and_then(|m| m.url.clone()),
            "title": mcp.and_then(|m| m.title.clone()),
            "loaded": self.loaded_at.is_some(),
            "page_capability": match self.page_init.as_ref().and_then(|p| p.registered) {
                None => "pending",
                Some(true) => "registered",
                Some(false) => "unavailable",
            },
            "startup": if frames.shown { "released" } else { "loading" },
            "input_ready": self.loaded_at.is_some(),
            "window": {
                "width": width,
                "height": height,
                "pixel_width": pixel_width,
                "pixel_height": pixel_height,
                "scale": window.display_scale(),
            },
            "frames": { "imported": frames.imported },
            "input": {
                "forwarded": self.input.forwarded,
                "rejected": self.input.rejected,
                "discarded": self.input.discarded,
            },
            "console": {
                "recent": mcp.map(Bridge::recent_console),
            },
        })
    }

    /// End of the loop: input counters, and a FAIL for an unfinished test hook. The MCP
    /// session record (and socket) is removed: discovery stops finding this game.
    pub(crate) fn finish(&mut self) {
        if let Some(bridge) = self.mcp.take() {
            let calls = bridge.calls;
            drop(bridge);
            println!("gate: mcp session removed calls={calls}");
        }
        println!(
            "gate: game input forwarded={} rejected={} discarded={}",
            self.input.forwarded, self.input.rejected, self.input.discarded
        );
        if let Some(test) = self.test.as_ref() {
            let done = test.click == ClickTest::Done
                && test.key == KeyTest::Done
                && test.wheel == WheelTest::Done;
            if done {
                println!("gate: PASS test input click, key and wheel delivered");
            } else if !self.failed {
                println!(
                    "gate: FAIL test input incomplete click={:?} key={:?} wheel={:?}",
                    test.click, test.key, test.wheel
                );
                self.failed = true;
            }
        }
    }

    fn click_action(&mut self, action: ClickAction) {
        match action {
            ClickAction::Complete => println!("gate: test click complete"),
            ClickAction::Fail(what) => {
                println!("gate: FAIL test click: {what}");
                self.failed = true;
            },
            ClickAction::None | ClickAction::Push => {},
        }
    }

    fn key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Complete => println!("gate: test key complete"),
            KeyAction::Fail(what) => {
                println!("gate: FAIL test key: {what}");
                self.failed = true;
            },
            KeyAction::None | KeyAction::Push => {},
        }
    }

    fn wheel_action(&mut self, action: WheelAction) {
        match action {
            WheelAction::Complete => println!("gate: test wheel complete"),
            WheelAction::Fail(what) => {
                println!("gate: FAIL test wheel: {what}");
                self.failed = true;
            },
            WheelAction::None | WheelAction::Push => {},
        }
    }
}

/// Left button down + up at `TEST_CLICK_POINT`, stamped with `TEST_CLICK_MOUSE_ID`.
fn push_click(events: &EventSubsystem, window_id: u32) -> Result<(), String> {
    let (x, y) = TEST_CLICK_POINT;
    let (timestamp, which, mouse_btn, clicks) = (0, TEST_CLICK_MOUSE_ID, MouseButton::Left, 1);
    events
        .push_event(Event::MouseButtonDown {
            timestamp,
            window_id,
            which,
            mouse_btn,
            clicks,
            x,
            y,
        })
        .and_then(|()| {
            events.push_event(Event::MouseButtonUp {
                timestamp,
                window_id,
                which,
                mouse_btn,
                clicks,
                x,
                y,
            })
        })
        .map_err(|e| e.to_string())
}

/// ArrowRight down + up (no modifiers, not a repeat, `raw` 0), stamped with
/// `TEST_KEY_KEYBOARD_ID`.
fn push_key(events: &EventSubsystem, window_id: u32) -> Result<(), String> {
    let (timestamp, keycode, scancode, keymod, repeat, which, raw) = (
        0,
        Some(Keycode::Right),
        Some(Scancode::Right),
        Mod::empty(),
        false,
        TEST_KEY_KEYBOARD_ID,
        0,
    );
    events
        .push_event(Event::KeyDown {
            timestamp,
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            which,
            raw,
        })
        .and_then(|()| {
            events.push_event(Event::KeyUp {
                timestamp,
                window_id,
                keycode,
                scancode,
                keymod,
                repeat,
                which,
                raw,
            })
        })
        .map_err(|e| e.to_string())
}

/// One wheel notch away from the user (`TEST_WHEEL_LINES`), stamped with
/// `TEST_WHEEL_MOUSE_ID`.
fn push_wheel(events: &EventSubsystem, window_id: u32) -> Result<(), String> {
    let (x, y) = TEST_WHEEL_LINES;
    events
        .push_event(Event::MouseWheel {
            timestamp: 0,
            window_id,
            which: TEST_WHEEL_MOUSE_ID,
            x,
            y,
            direction: MouseWheelDirection::Normal,
            mouse_x: 0.0,
            mouse_y: 0.0,
            integer_x: x as i32,
            integer_y: y as i32,
        })
        .map_err(|e| e.to_string())
}
