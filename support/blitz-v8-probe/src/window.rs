//! `--window`: present the probe document in a real `blitz-shell` (winit) window.
//!
//! `RovesDocument` implements Blitz's `Document` trait, the integration point
//! `blitz-shell` offers embedders (the same one Dioxus Native uses):
//! - `poll` runs the JS frame step (rAF, microtasks, deno_core event loop) at
//!   60 Hz; a ticker thread wakes the shell's poll waker every frame, and the
//!   shell then resolves style/layout and paints with the Vello CPU renderer
//!   into the window through softbuffer.
//! - `handle_ui_event` receives real OS input. Blitz's `EventDriver` does
//!   hit-testing, hover and focus, and a collecting `EventHandler` forwards the
//!   resulting DOM events to the trusted JS dispatcher after the driver returns,
//!   so JS never runs while Blitz holds the document.
//!
//! The window is undecorated at a fixed logical position, so a test harness can
//! inject a real OS click at known screen coordinates.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use std::task::Context;
use std::time::{Duration, Instant};

use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocGuard, DocGuardMut, Document, EventDriver, EventHandler, NodeId};
use blitz_html::HtmlDocument;
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, DomEvent, DomEventData, EventState, UiEvent,
};
use deno_core::{JsRuntime, v8};
use keyboard_types::Modifiers;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::WindowAttributes;

/// Logical screen position of the undecorated window's top-left corner.
pub const WINDOW_ORIGIN: (f64, f64) = (100.0, 100.0);
/// Frames after which the window run gives up waiting for real input.
const TIMEOUT_FRAMES: u32 = 60 * 30;

type Collected = Rc<RefCell<Vec<(NodeId, &'static str, String)>>>;

/// Collects the DOM events Blitz's `EventDriver` produces; they are dispatched
/// to JS once the driver has released the document.
struct Collector(Collected);

impl EventHandler for Collector {
    fn handle_event(
        &mut self,
        _chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        _event_state: &mut EventState,
    ) {
        if let Some(init) = event_init(&event.data) {
            self.0
                .borrow_mut()
                .push((event.target, event.data.name(), init));
        }
    }
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
/// not forward yet (scroll, wheel, input, IME, touch).
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
        DocGuardMut::RefCell(RefMut::map(self.document.borrow_mut(), |document| {
            &mut **document
        }))
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let collected: Collected = Rc::new(RefCell::new(Vec::new()));
        {
            let mut driver = EventDriver::new(self, Collector(collected.clone()));
            driver.handle_ui_event(event);
        }
        for (target, kind, init) in collected.take() {
            crate::dispatch_trusted(&mut self.runtime, &self.dispatcher, target, kind, &init);
        }
    }

    fn poll(&mut self, task_context: Option<Context<'_>>) -> bool {
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
