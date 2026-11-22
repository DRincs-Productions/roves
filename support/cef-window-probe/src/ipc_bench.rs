//! Stage 2, opt-in (`--features ipc-bench` build **and** `--roves-ipc-bench=<result.json>`):
//! benchmark of Deno -> Blink messages over CEF's public asynchronous process messages, with
//! checked results. See README.md, "Stage 2".
//!
//! Path of one message: `ipc_bench.js` in a deno_core 0.413.0 `JsRuntime` (own thread of the
//! browser process) -> async op -> `post_task(UI)` -> `CefFrame::SendProcessMessage(PID_RENDERER)`
//! -> [`BenchRenderHandler`] in the sandboxed renderer calls the page's own
//! `rovesIpcBenchFixture.handle(json)` -> reply message to `PID_BROWSER` -> the client's
//! `on_process_message_received` resolves the op's promise.
//!
//! Nothing is injected into any page: no `on_context_created` binding, no message router, no
//! `cefQuery`. The renderer only calls a function the fixture page defined itself, and only
//! when the browser process sends the request, which it does only in benchmark mode and only
//! to a main frame on the probe's loopback origin. This is not the template bridge.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};

use cef::*;
use deno_core::{
    JsRuntime, PollEventLoopOptions, RuntimeOptions, futures::channel::oneshot, op2, serde_json,
};

use crate::app::{close_window, log};

const REQUEST: &str = "roves-ipc-bench";
const REPLY: &str = "roves-ipc-bench-reply";
const FIXTURE_GLOBAL: &str = "rovesIpcBenchFixture";
const BENCH_JS: &str = include_str!("ipc_bench.js");

/// Replies awaited by the Deno side, keyed by request id.
static PENDING: LazyLock<Mutex<HashMap<i32, oneshot::Sender<String>>>> =
    LazyLock::new(Default::default);
static NEXT_ID: AtomicI32 = AtomicI32::new(1);
static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
/// JSON reported by `ipc_bench.js` through `op_roves_ipc_done`.
static RESULT: Mutex<Option<String>> = Mutex::new(None);
static PASSED: AtomicBool = AtomicBool::new(false);
/// Set once the browser starts closing: no new request is sent and none is left waiting.
static CLOSING: AtomicBool = AtomicBool::new(false);
static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

thread_local! {
    /// UI thread only: the browser whose main frame hosts the fixture and the URL prefix
    /// (loopback origin) that frame must still have when a request is sent.
    static TARGET: RefCell<Option<(Browser, String)>> = const { RefCell::new(None) };
}

fn error_json(error: impl Into<String>) -> String {
    serde_json::json!({ "ok": false, "error": error.into() }).to_string()
}

fn complete(id: i32, reply: String) {
    let sender = PENDING.lock().unwrap().remove(&id);
    if let Some(sender) = sender {
        let _ = sender.send(reply);
    }
}

// ---------------------------------------------------------------------------------------------
// Browser process

/// Called on the UI thread after the first successful main-frame load in benchmark mode.
pub fn start(browser: Browser, origin: &str, result_path: PathBuf) {
    LazyLock::force(&EPOCH);
    TARGET.with_borrow_mut(|target| *target = Some((browser, format!("{origin}/"))));
    log(format!("ipc-bench start deno_core=0.413.0 result={}", result_path.display()));
    let thread = std::thread::Builder::new()
        .name("roves-ipc-bench-deno".into())
        .spawn(move || finish(run_deno(), &result_path));
    match thread {
        Ok(thread) => *THREAD.lock().unwrap() = Some(thread),
        Err(error) => log(format!("ipc-bench error thread: {error}")),
    }
}

fn run_deno() -> Result<String, String> {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio: {error}"))?;
    tokio.block_on(async {
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![roves_cef_ipc_bench::init()],
            ..Default::default()
        });
        runtime
            .execute_script("roves-cef-ipc-bench.js", BENCH_JS)
            .map_err(|error| error.to_string())?;
        runtime
            .run_event_loop(PollEventLoopOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        RESULT
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| "the benchmark script ended without a result".to_string())
    })
}

/// Deno thread: records the outcome and closes the window through the normal close path.
fn finish(result: Result<String, String>, result_path: &Path) {
    let json = result.unwrap_or_else(error_json);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    let ok = parsed["ok"] == serde_json::Value::Bool(true);
    PASSED.store(ok, Ordering::SeqCst);
    if let Err(error) = std::fs::write(result_path, &json) {
        log(format!("ipc-bench error write {}: {error}", result_path.display()));
        PASSED.store(false, Ordering::SeqCst);
    }
    log(format!(
        "ipc-bench result ok={ok} mismatches={} json={json}",
        parsed["mismatches"]
    ));
    if !CLOSING.load(Ordering::SeqCst) {
        let mut task = FinishBench::new();
        post_task(ThreadId::UI, Some(&mut task));
    }
}

/// True only if the script reported `ok: true` and the result file was written.
pub fn passed() -> bool {
    PASSED.load(Ordering::SeqCst)
}

/// UI thread, `on_before_close`: unblock any request still waiting so the script can finish.
pub fn browser_closing() {
    CLOSING.store(true, Ordering::SeqCst);
    TARGET.with_borrow_mut(|target| *target = None);
    let pending: Vec<_> = PENDING.lock().unwrap().drain().collect();
    for (_, sender) in pending {
        let _ = sender.send(error_json("browser closed before the reply"));
    }
}

/// Browser thread after the message loop: wait for the script to record its outcome.
pub fn join() {
    let thread = THREAD.lock().unwrap().take();
    if let Some(thread) = thread
        && thread.join().is_err()
    {
        log("ipc-bench error deno thread panicked");
        PASSED.store(false, Ordering::SeqCst);
    }
}

/// Client `on_process_message_received` (UI thread). Returns 1 when the message was ours.
pub fn on_browser_message(
    frame: Option<&mut Frame>,
    source: ProcessId,
    message: Option<&mut ProcessMessage>,
) -> i32 {
    let Some(message) = message else {
        return 0;
    };
    if source != ProcessId::RENDERER || CefString::from(&message.name()).to_string() != REPLY {
        return 0;
    }
    let Some(args) = message.argument_list() else {
        return 1;
    };
    let id = args.int(0);
    let reply = if frame.is_some_and(|frame| frame.is_main() != 0) {
        CefString::from(&args.string(1)).to_string()
    } else {
        error_json("reply from a frame other than the main frame")
    };
    complete(id, reply);
    1
}

wrap_task! {
    struct SendToRenderer {
        id: i32,
        payload: String,
    }

    impl Task {
        fn execute(&self) {
            let sent = TARGET.with_borrow(|target| {
                let (browser, prefix) = target.as_ref()?;
                let frame = browser.main_frame()?;
                if !CefString::from(&frame.url()).to_string().starts_with(prefix.as_str()) {
                    return None;
                }
                let mut message = process_message_create(Some(&CefString::from(REQUEST)))?;
                let args = message.argument_list()?;
                args.set_int(0, self.id);
                args.set_string(1, Some(&CefString::from(self.payload.as_str())));
                frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
                Some(())
            });
            if sent.is_none() {
                complete(self.id, error_json("no fixture main frame on the loopback origin"));
            }
        }
    }
}

wrap_task! {
    struct FinishBench {}

    impl Task {
        fn execute(&self) {
            log("ipc-bench close window");
            let browser = TARGET.with_borrow(|target| target.as_ref().map(|(browser, _)| browser.clone()));
            if let Some(mut browser) = browser {
                close_window(&mut browser);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Deno side

#[op2]
#[string]
async fn op_roves_ipc_send(#[string] payload: String) -> String {
    if CLOSING.load(Ordering::SeqCst) {
        return error_json("browser closing");
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (sender, receiver) = oneshot::channel();
    PENDING.lock().unwrap().insert(id, sender);
    let mut task = SendToRenderer::new(id, payload);
    if post_task(ThreadId::UI, Some(&mut task)) == 0 {
        complete(id, error_json("post_task(UI) failed"));
    }
    receiver
        .await
        .unwrap_or_else(|_| error_json("reply channel dropped"))
}

/// Microseconds on the browser process' monotonic clock (std `Instant`).
#[op2(fast)]
fn op_roves_ipc_now_us() -> f64 {
    EPOCH.elapsed().as_secs_f64() * 1e6
}

#[op2]
fn op_roves_ipc_done(#[string] result: String) {
    *RESULT.lock().unwrap() = Some(result);
}

deno_core::extension!(
    roves_cef_ipc_bench,
    ops = [op_roves_ipc_send, op_roves_ipc_now_us, op_roves_ipc_done],
);

// ---------------------------------------------------------------------------------------------
// Renderer process

// App given to `execute_process` for CEF subprocesses in `ipc-bench` builds; only the renderer
// uses its handler.
wrap_app! {
    pub struct SubprocessApp {}

    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(BenchRenderHandler::new())
        }
    }
}

wrap_render_process_handler! {
    struct BenchRenderHandler {}

    impl RenderProcessHandler {
        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(message)) = (frame, message) else {
                return 0;
            };
            if source_process != ProcessId::BROWSER
                || CefString::from(&message.name()).to_string() != REQUEST
            {
                return 0;
            }
            let Some(args) = message.argument_list() else {
                return 1;
            };
            let id = args.int(0);
            let reply = if frame.is_main() == 0 {
                error_json("request for a frame other than the main frame")
            } else {
                call_fixture(frame, &CefString::from(&args.string(1))).unwrap_or_else(error_json)
            };
            if let Some(mut out) = process_message_create(Some(&CefString::from(REPLY)))
                && let Some(out_args) = out.argument_list()
            {
                out_args.set_int(0, id);
                out_args.set_string(1, Some(&CefString::from(reply.as_str())));
                frame.send_process_message(ProcessId::BROWSER, Some(&mut out));
            }
            1
        }
    }
}

/// Renderer main thread: `rovesIpcBenchFixture.handle(payload)` in the frame's own context.
fn call_fixture(frame: &Frame, payload: &CefString) -> Result<String, String> {
    let context = frame.v8_context().ok_or("frame has no V8 context")?;
    if context.enter() == 0 {
        return Err("cannot enter the frame's V8 context".into());
    }
    let result = (|| {
        let fixture = context
            .global()
            .and_then(|global| global.value_bykey(Some(&CefString::from(FIXTURE_GLOBAL))))
            .filter(|value| value.is_object() != 0)
            .ok_or("the page does not define rovesIpcBenchFixture")?;
        let handle = fixture
            .value_bykey(Some(&CefString::from("handle")))
            .filter(|value| value.is_function() != 0)
            .ok_or("rovesIpcBenchFixture.handle is not a function")?;
        let mut this = fixture.clone();
        let argument = v8_value_create_string(Some(payload));
        match handle.execute_function(Some(&mut this), Some(&[argument])) {
            Some(value) if value.is_string() != 0 => Ok(CefString::from(&value.string_value()).to_string()),
            _ if handle.has_exception() != 0 => Err(handle
                .exception()
                .map(|exception| CefString::from(&exception.message()).to_string())
                .unwrap_or_else(|| "exception in handle".into())),
            _ => Err("handle did not return a string".to_string()),
        }
    })();
    context.exit();
    result
}
