//! `--window`: present the probe document in a real `blitz-shell` (winit) window.
//!
//! `RovesDocument` implements Blitz's `Document` trait, the integration point
//! `blitz-shell` offers embedders (the same one Dioxus Native uses):
//! - `poll` runs the JS frame step (rAF, microtasks, deno_core event loop) at
//!   60 Hz; a ticker thread wakes the shell's poll waker every frame, and the
//!   shell then resolves style/layout and paints with the Vello CPU renderer
//!   into the window through softbuffer.
//! - `handle_ui_event` receives real OS input and passes it to
//!   `dispatch_ui_event`, the input bridge the headless tests use too.
//!
//! The window is undecorated at a fixed logical position, so a test harness can
//! inject a real OS click at known screen coordinates.
//!
//! Input bridge: Blitz's `EventDriver` does hit-testing, hover, focus and the
//! default actions (text editing in its `TextInputData`, Tab focus moves,
//! checkbox toggles). `JsEventHandler` dispatches each DOM event it produces to
//! the trusted JS dispatcher synchronously, and a `preventDefault()` in JS
//! cancels the event in the driver, which then skips its default action (and,
//! for pointer events, the mouse compatibility events). The driver reaches the
//! document through `SharedDocument`, whose `RefCell` borrows last only for each
//! of its own steps, never across the handler: JS (and the DOM ops it calls)
//! runs while Blitz holds no borrow of the document.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use std::task::Context;
use std::time::{Duration, Instant};

use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocGuard, DocGuardMut, Document, EventDriver, EventHandler, NodeId};
use blitz_html::HtmlDocument;
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, DomEvent, DomEventData, EventState, KeyState,
    MouseEventButton, MouseEventButtons, UiEvent,
};
use deno_core::{JsRuntime, v8};
use keyboard_types::{Code, Key, Location, Modifiers};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::WindowAttributes;

/// Logical screen position of the undecorated window's top-left corner.
pub const WINDOW_ORIGIN: (f64, f64) = (100.0, 100.0);
/// Frames after which the window run gives up waiting for real input.
const TIMEOUT_FRAMES: u32 = 60 * 30;

/// The probe document as Blitz's `Document`, for the `EventDriver`: each
/// `inner`/`inner_mut` call borrows the shared `RefCell` only for as long as the
/// driver holds that guard.
pub(crate) struct SharedDocument(pub(crate) Rc<RefCell<HtmlDocument>>);

impl Document for SharedDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::RefCell(Ref::map(self.0.borrow(), |document| &**document))
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::RefCell(RefMut::map(self.0.borrow_mut(), |document| &mut **document))
    }
}

/// Dispatches each DOM event the `EventDriver` produces to JS at once; a
/// canceled JS event cancels the driver's event (no default action).
struct JsEventHandler<'a> {
    runtime: &'a mut JsRuntime,
    dispatcher: &'a v8::Global<v8::Function>,
}

impl EventHandler for JsEventHandler<'_> {
    fn handle_event(
        &mut self,
        _chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        event_state: &mut EventState,
    ) {
        let Some(init) = event_init(&event.data) else {
            return;
        };
        let not_canceled = crate::dispatch_trusted(
            self.runtime,
            self.dispatcher,
            event.target,
            event.data.name(),
            &init,
        );
        if !not_canceled {
            event_state.prevent_default();
        }
    }
}

/// Routes one UI event through Blitz's `EventDriver` with JS listeners (see the
/// module docs). The window's `handle_ui_event` and the headless tests both use it.
pub(crate) fn dispatch_ui_event(
    runtime: &mut JsRuntime,
    dispatcher: &v8::Global<v8::Function>,
    document: &Rc<RefCell<HtmlDocument>>,
    event: UiEvent,
) {
    let mut shared = SharedDocument(document.clone());
    let mut driver = EventDriver::new(
        &mut shared,
        JsEventHandler {
            runtime,
            dispatcher,
        },
    );
    driver.handle_ui_event(event);
}

/// A key press and release as a keyboard would send them (no modifiers), through
/// `dispatch_ui_event`: keydown/keyup reach JS, and an uncanceled keydown runs
/// Blitz's default action on the focused element (text editing, Tab).
pub(crate) fn press_key(
    runtime: &mut JsRuntime,
    dispatcher: &v8::Global<v8::Function>,
    document: &Rc<RefCell<HtmlDocument>>,
    key: Key,
) {
    for pressed in [true, false] {
        let event = BlitzKeyEvent {
            key: key.clone(),
            code: Code::Unidentified,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: if pressed {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            text: match &key {
                Key::Character(text) => Some(text.as_str().into()),
                _ => None,
            },
        };
        let event = if pressed {
            UiEvent::KeyDown(event)
        } else {
            UiEvent::KeyUp(event)
        };
        dispatch_ui_event(runtime, dispatcher, document, event);
    }
}

/// A primary-button press and release at viewport point (`x`, `y`), as a mouse
/// sends them, through `dispatch_ui_event`: Blitz hit-tests, runs its pointer
/// default actions (focus and caret placement in a text input) and produces
/// the pointer/mouse/click events JS receives. Returns `false`, sending
/// nothing, when no node is at the point.
pub(crate) fn pointer_click(
    runtime: &mut JsRuntime,
    dispatcher: &v8::Global<v8::Function>,
    document: &Rc<RefCell<HtmlDocument>>,
    x: f32,
    y: f32,
) -> bool {
    let Some(target) = document.borrow().element_from_point(x, y) else {
        return false;
    };
    for pressed in [true, false] {
        let Some(mut event) = document
            .borrow()
            .get_node(target)
            .map(|node| node.synthetic_click_event_data(Modifiers::empty()))
        else {
            return false;
        };
        event.coords.page_x = x;
        event.coords.page_y = y;
        event.coords.client_x = x;
        event.coords.client_y = y;
        event.coords.screen_x = x;
        event.coords.screen_y = y;
        event.button = MouseEventButton::Main;
        event.buttons = MouseEventButtons::from_bits_truncate(u8::from(pressed));
        let event = if pressed {
            UiEvent::PointerDown(event)
        } else {
            UiEvent::PointerUp(event)
        };
        dispatch_ui_event(runtime, dispatcher, document, event);
    }
    true
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            },
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn modifiers_json(mods: Modifiers) -> String {
    format!(
        r#""ctrlKey":{},"shiftKey":{},"altKey":{},"metaKey":{}"#,
        mods.contains(Modifiers::CONTROL),
        mods.contains(Modifiers::SHIFT),
        mods.contains(Modifiers::ALT),
        mods.contains(Modifiers::META),
    )
}

fn pointer_init(event: &BlitzPointerEvent) -> String {
    let (pointer_id, pointer_type) = match event.id {
        BlitzPointerId::Mouse => (1, "mouse"),
        BlitzPointerId::Pen => (2, "pen"),
        BlitzPointerId::Finger(id) => (id.saturating_add(3), "touch"),
    };
    format!(
        r#"{{"clientX":{},"clientY":{},"screenX":{},"screenY":{},"button":{},"buttons":{},"pointerId":{},"pointerType":"{}","isPrimary":{},{}}}"#,
        event.coords.client_x,
        event.coords.client_y,
        event.coords.screen_x,
        event.coords.screen_y,
        event.button as u8,
        event.buttons.bits(),
        pointer_id,
        pointer_type,
        event.is_primary,
        modifiers_json(event.mods),
    )
}

/// JSON init dictionary for the JS event, or `None` for events the probe does
/// not forward yet (scroll, wheel, IME composition, touch). Blitz's `input`
/// carries only the new value, which JS reads from the element itself, so the
/// InputEvent has no `inputType`/`data`; it is not cancelable (as in browsers).
fn event_init(data: &DomEventData) -> Option<String> {
    use DomEventData::*;
    match data {
        PointerMove(event) | PointerDown(event) | PointerUp(event) | PointerCancel(event)
        | PointerEnter(event) | PointerLeave(event) | PointerOver(event) | PointerOut(event)
        | MouseMove(event) | MouseDown(event) | MouseUp(event) | MouseEnter(event)
        | MouseLeave(event) | MouseOver(event) | MouseOut(event) | Click(event)
        | ContextMenu(event) | DoubleClick(event) => Some(pointer_init(event)),
        KeyPress(event) | KeyDown(event) | KeyUp(event) => Some(format!(
            r#"{{"key":{},"code":{},"repeat":{},"isComposing":{},{}}}"#,
            json_string(&event.key.to_string()),
            json_string(&event.code.to_string()),
            event.is_auto_repeating,
            event.is_composing,
            modifiers_json(event.modifiers),
        )),
        Focus(_) | Blur(_) => Some(r#"{"bubbles":false}"#.to_string()),
        FocusIn(_) | FocusOut(_) => Some("{}".to_string()),
        Input(_) => Some(r#"{"cancelable":false}"#.to_string()),
        _ => None,
    }
}

/// The probe document plus the JS runtime that owns its scripts.
pub struct RovesDocument {
    pub document: Rc<RefCell<HtmlDocument>>,
    pub runtime: JsRuntime,
    pub dispatcher: v8::Global<v8::Function>,
    pub run_animation_frame: v8::Global<v8::Function>,
    frames: u32,
    last_frame: Option<Instant>,
    ticker_started: bool,
    timeline_started: bool,
}

impl RovesDocument {
    pub fn new(
        document: Rc<RefCell<HtmlDocument>>,
        runtime: JsRuntime,
        dispatcher: v8::Global<v8::Function>,
        run_animation_frame: v8::Global<v8::Function>,
    ) -> Self {
        Self {
            document,
            runtime,
            dispatcher,
            run_animation_frame,
            frames: 0,
            last_frame: None,
            ticker_started: false,
            timeline_started: false,
        }
    }

    /// blitz-shell resolves with its own animation time (seconds since its first
    /// redraw, a private `Instant`). On the shell's first call into the document,
    /// restart the page clock so flushes from JS ops use the shell's origin
    /// instead of the headless one (see `animation_clock`).
    fn start_shell_timeline(&mut self) {
        if !self.timeline_started {
            self.timeline_started = true;
            crate::animation_clock::page_clock().restart();
        }
    }

    /// Evaluates a script returning a string (the window test's status).
    fn evaluate(&mut self, source: &'static str) -> String {
        let value = self
            .runtime
            .execute_script("roves-blitz-window-status.js", source)
            .expect("window status script should run");
        let runtime = &mut self.runtime;
        deno_core::scope!(scope, runtime);
        let value = v8::Local::new(scope, value);
        value.to_rust_string_lossy(scope)
    }

    fn after_frame(&mut self) {
        self.frames += 1;
        if self.frames == 2 {
            // Layout has run at least once (the shell resolves before painting).
            let status = self.evaluate("JSON.stringify(windowTarget())");
            let scale = f64::from(self.document.borrow().viewport().hidpi_scale);
            println!(
                "ROVES_PROBE_WINDOW_READY {{\"scale\":{scale},\"target\":{status},\"origin\":[{},{}]}}",
                WINDOW_ORIGIN.0, WINDOW_ORIGIN.1
            );
            flush();
        }
        let status = self.evaluate("windowStatus()");
        let finished = status != "waiting";
        if finished || self.frames >= TIMEOUT_FRAMES {
            let passed = status == "passed";
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"window_presentation_real_input\",\"status\":\"{}\",\"frames\":{},\"detail\":{}}}",
                if passed { "passed" } else { "failed" },
                self.frames,
                json_string(&status),
            );
            flush();
            std::process::exit(if passed { 0 } else { 1 });
        }
    }
}

fn flush() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

impl Document for RovesDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::RefCell(Ref::map(self.document.borrow(), |document| &**document))
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        self.start_shell_timeline();
        DocGuardMut::RefCell(RefMut::map(self.document.borrow_mut(), |document| {
            &mut **document
        }))
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        self.start_shell_timeline();
        dispatch_ui_event(&mut self.runtime, &self.dispatcher, &self.document, event);
    }

    fn poll(&mut self, task_context: Option<Context<'_>>) -> bool {
        self.start_shell_timeline();
        if !self.ticker_started {
            if let Some(context) = &task_context {
                // The shell polls when its waker fires; tick it every frame.
                let waker = context.waker().clone();
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_micros(16_667));
                        waker.wake_by_ref();
                    }
                });
                self.ticker_started = true;
            }
        }
        let due = self
            .last_frame
            .is_none_or(|last| last.elapsed() >= Duration::from_micros(16_000));
        if !due {
            return false;
        }
        self.last_frame = Some(Instant::now());
        crate::frame_step(&mut self.runtime, &self.run_animation_frame);
        self.after_frame();
        true
    }
}

/// Runs the window until the window test passes, fails or times out (the
/// process exits from `RovesDocument::after_frame`).
pub fn run(document: RovesDocument) {
    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let mut application = BlitzApplication::new(proxy, receiver);
    let (width, height) = crate::VIEWPORT;
    let attributes = WindowAttributes::default()
        .with_title("Roves Blitz probe")
        .with_position(LogicalPosition::new(WINDOW_ORIGIN.0, WINDOW_ORIGIN.1))
        .with_surface_size(LogicalSize::new(f64::from(width), f64::from(height)))
        .with_decorations(false)
        .with_resizable(false);
    application.add_window(WindowConfig::with_attributes(
        Box::new(document),
        VelloCpuWindowRenderer::new(),
        attributes,
    ));
    event_loop
        .run_app(application)
        .expect("window event loop should run");
}
