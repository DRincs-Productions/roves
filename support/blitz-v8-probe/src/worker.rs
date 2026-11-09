//! Dedicated workers (`new Worker(url)`) for the probe.
//!
//! No deno_core 0.413-paired extension provides `Worker`: Deno's lives in
//! `deno_runtime` (web worker bootstrap, permissions, `Deno.*`), which games must
//! not get. Each worker here runs its classic script in its own deno_core
//! `JsRuntime` (its own V8 isolate) on its own thread, with only deno_webidl and
//! deno_web loaded; `src/worker_scope.js` builds a DedicatedWorkerGlobalScope
//! from them and removes `Deno`. Workers get no DOM, fetch, createImageBitmap,
//! WebGPU, storage or Web Audio, so feature probes run inside one report what the
//! worker really has.
//!
//! Messages cross threads as V8 structured-serialization bytes (`core.serialize`)
//! over channels. The owner receives worker events through an async op that
//! resolves when the frame loop polls the main event loop, so no V8 call crosses
//! threads and nothing re-enters the main isolate. Uncaught worker errors not
//! cancelled inside the worker end the worker (as in Deno) and reach the owner as
//! an `error` event. `terminate()` stops the worker's isolate.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::Poll;

use deno_core::convert::Uint8Array;
use deno_core::error::CoreErrorKind;
use deno_core::futures::StreamExt;
use deno_core::futures::channel::mpsc;
use deno_core::{
    AsyncRefCell, CancelFuture, CancelHandle, JsBuffer, JsRuntime, OpState, RuntimeOptions, op2, v8,
};
use deno_error::JsErrorBox;

const WORKER_SCOPE: &str = include_str!("worker_scope.js");

const EVENT_CLOSED: u32 = 0;
const EVENT_MESSAGE: u32 = 1;
const EVENT_ERROR: u32 = 2;

/// From a worker to its owner.
enum ToOwner {
    /// Serialized message data.
    Message(Vec<u8>),
    /// Uncaught error as JSON `{message, filename, lineno, colno}`.
    Error(String),
}

/// Shared by the owner and the worker thread so `terminate()` can stop the
/// worker's isolate whether or not it has started yet.
#[derive(Default)]
struct Termination {
    terminated: bool,
    isolate: Option<v8::IsolateHandle>,
}

fn lock(termination: &Mutex<Termination>) -> MutexGuard<'_, Termination> {
    termination
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct OwnerSide {
    to_worker: mpsc::UnboundedSender<Vec<u8>>,
    from_worker: Rc<AsyncRefCell<mpsc::UnboundedReceiver<ToOwner>>>,
    termination: Arc<Mutex<Termination>>,
    cancel: Rc<CancelHandle>,
}

/// Owner-side state: live workers and the event last returned per worker.
#[derive(Default)]
struct Workers {
    next_id: u32,
    live: HashMap<u32, OwnerSide>,
    pending: HashMap<u32, ToOwner>,
}

/// `new Worker`: spawns the worker thread with the already fetched script.
#[op2(fast)]
fn op_worker_start(
    state: &mut OpState,
    #[string] source: String,
    #[string] url: String,
    #[string] name: String,
) -> Result<u32, JsErrorBox> {
    let (to_worker, inbox) = mpsc::unbounded();
    let (to_owner, from_worker) = mpsc::unbounded();
    let termination = Arc::new(Mutex::new(Termination::default()));
    let script = WorkerScript { source, url, name };
    let thread_termination = termination.clone();
    std::thread::Builder::new()
        .name("roves-probe-worker".into())
        .spawn(move || run_worker(script, inbox, to_owner, thread_termination))
        .map_err(|error| JsErrorBox::generic(format!("worker thread: {error}")))?;
    let workers = state.borrow_mut::<Workers>();
    workers.next_id += 1;
    let id = workers.next_id;
    workers.live.insert(
        id,
        OwnerSide {
            to_worker,
            from_worker: Rc::new(AsyncRefCell::new(from_worker)),
            termination,
            cancel: CancelHandle::new_rc(),
        },
    );
    Ok(id)
}

/// `worker.postMessage`: queues serialized data; dropped once terminated.
#[op2]
fn op_worker_post(state: &mut OpState, id: u32, #[buffer] data: JsBuffer) {
    if let Some(worker) = state.borrow::<Workers>().live.get(&id) {
        let _ = worker.to_worker.unbounded_send(data.to_vec());
    }
}

/// Next event from worker `id`: `EVENT_MESSAGE`/`EVENT_ERROR` (payload taken with
/// the ops below) or `EVENT_CLOSED` once the worker ended or was terminated.
#[op2]
async fn op_worker_next_event(state: Rc<RefCell<OpState>>, id: u32) -> u32 {
    let (receiver, cancel) = {
        let state = state.borrow();
        match state.borrow::<Workers>().live.get(&id) {
            Some(worker) => (worker.from_worker.clone(), worker.cancel.clone()),
            None => return EVENT_CLOSED,
        }
    };
    let next = async move { receiver.borrow_mut().await.next().await }
        .or_cancel(cancel)
        .await;
    let Ok(Some(event)) = next else {
        return EVENT_CLOSED;
    };
    let kind = match event {
        ToOwner::Message(_) => EVENT_MESSAGE,
        ToOwner::Error(_) => EVENT_ERROR,
    };
    state
        .borrow_mut()
        .borrow_mut::<Workers>()
        .pending
        .insert(id, event);
    kind
}

#[op2]
#[buffer]
fn op_worker_take_message(state: &mut OpState, id: u32) -> Vec<u8> {
    match state.borrow_mut::<Workers>().pending.remove(&id) {
        Some(ToOwner::Message(data)) => data,
        _ => Vec::new(),
    }
}

#[op2]
#[string]
fn op_worker_take_error(state: &mut OpState, id: u32) -> String {
    match state.borrow_mut::<Workers>().pending.remove(&id) {
        Some(ToOwner::Error(details)) => details,
        _ => "{}".into(),
    }
}

/// `worker.terminate()`: closes both channels and stops the isolate, also while
/// a script is running.
#[op2(fast)]
fn op_worker_terminate(state: &mut OpState, id: u32) {
    let workers = state.borrow_mut::<Workers>();
    workers.pending.remove(&id);
    let Some(worker) = workers.live.remove(&id) else {
        return;
    };
    worker.cancel.cancel();
    let mut termination = lock(&worker.termination);
    termination.terminated = true;
    if let Some(isolate) = &termination.isolate {
        isolate.terminate_execution();
    }
}

deno_core::extension!(
    roves_blitz_worker,
    ops = [
        op_worker_start,
        op_worker_post,
        op_worker_next_event,
        op_worker_take_message,
        op_worker_take_error,
        op_worker_terminate,
    ],
    state = |state| {
        state.put(Workers::default());
    }
);

// ---- Worker thread ----------------------------------------------------------

struct WorkerScript {
    source: String,
    url: String,
    name: String,
}

/// Worker-side state of the worker's own runtime.
struct WorkerScope {
    script: WorkerScript,
    to_owner: mpsc::UnboundedSender<ToOwner>,
    inbox: Rc<AsyncRefCell<mpsc::UnboundedReceiver<Vec<u8>>>>,
    cancel: Rc<CancelHandle>,
    closing: Rc<Cell<bool>>,
}

#[op2]
#[string]
fn op_worker_scope_source(state: &mut OpState) -> String {
    std::mem::take(&mut state.borrow_mut::<WorkerScope>().script.source)
}

#[op2]
#[string]
fn op_worker_scope_url(state: &mut OpState) -> String {
    state.borrow::<WorkerScope>().script.url.clone()
}

#[op2]
#[string]
fn op_worker_scope_name(state: &mut OpState) -> String {
    state.borrow::<WorkerScope>().script.name.clone()
}

/// `self.postMessage` inside the worker.
#[op2]
fn op_worker_scope_post(state: &mut OpState, #[buffer] data: JsBuffer) {
    let _ = state
        .borrow::<WorkerScope>()
        .to_owner
        .unbounded_send(ToOwner::Message(data.to_vec()));
}

/// Next message from the owner; empty once the worker is closing or the owner
/// terminated it (V8 serialized data always has a header, so never empty).
#[op2]
async fn op_worker_scope_next(state: Rc<RefCell<OpState>>) -> Uint8Array {
    let (inbox, cancel) = {
        let state = state.borrow();
        let scope = state.borrow::<WorkerScope>();
        (scope.inbox.clone(), scope.cancel.clone())
    };
    let next = async move { inbox.borrow_mut().await.next().await }
        .or_cancel(cancel)
        .await;
    next.ok().flatten().unwrap_or_default().into()
}

/// `self.close()`: no further messages; the worker ends after the current task.
#[op2(fast)]
fn op_worker_scope_close(state: &mut OpState) {
    let scope = state.borrow::<WorkerScope>();
    scope.closing.set(true);
    scope.cancel.cancel();
}

deno_core::extension!(
    roves_blitz_worker_scope,
    ops = [
        op_worker_scope_source,
        op_worker_scope_url,
        op_worker_scope_name,
        op_worker_scope_post,
        op_worker_scope_next,
        op_worker_scope_close,
    ]
);

/// `{message, filename, lineno, colno}` of an error that ended the worker.
fn error_details(error: &deno_core::error::CoreError, url: &str) -> String {
    let (message, filename, lineno, colno) = match error.0.as_ref() {
        CoreErrorKind::Js(js) => {
            let frame = js.frames.first();
            (
                js.exception_message.clone(),
                frame
                    .and_then(|frame| frame.file_name.clone())
                    .unwrap_or_else(|| url.to_string()),
                frame.and_then(|frame| frame.line_number).unwrap_or(0),
                frame.and_then(|frame| frame.column_number).unwrap_or(0),
            )
        },
        other => (other.to_string(), url.to_string(), 0, 0),
    };
    deno_core::serde_json::json!({
        "message": message,
        "filename": filename,
        "lineno": lineno,
        "colno": colno,
    })
    .to_string()
}

fn run_worker(
    script: WorkerScript,
    inbox: mpsc::UnboundedReceiver<Vec<u8>>,
    to_owner: mpsc::UnboundedSender<ToOwner>,
    termination: Arc<Mutex<Termination>>,
) {
    let url = script.url.clone();
    let report = |details: String| {
        let _ = to_owner.unbounded_send(ToOwner::Error(details));
    };
    let tokio = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(tokio) => tokio,
        Err(error) => {
            let message = format!("worker runtime: {error}");
            let details = deno_core::serde_json::json!({
                "message": message,
                "filename": url,
                "lineno": 0,
                "colno": 0,
            });
            report(details.to_string());
            return;
        },
    };
    tokio.block_on(async {
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![
                deno_webidl::deno_webidl::init(),
                deno_web::deno_web::init(
                    Arc::new(deno_web::BlobStore::default()),
                    None,
                    false,
                    deno_web::InMemoryBroadcastChannel::default(),
                ),
                roves_blitz_worker_scope::init(),
            ],
            ..Default::default()
        });
        {
            let mut termination = lock(&termination);
            if termination.terminated {
                return;
            }
            termination.isolate = Some(runtime.v8_isolate().thread_safe_handle());
        }
        let closing = Rc::new(Cell::new(false));
        runtime.op_state().borrow_mut().put(WorkerScope {
            script,
            to_owner: to_owner.clone(),
            inbox: Rc::new(AsyncRefCell::new(inbox)),
            cancel: CancelHandle::new_rc(),
            closing: closing.clone(),
        });
        // The scope script evaluates the worker's script and starts its message
        // loop; errors it does not cancel end the worker.
        let result = match runtime.execute_script("roves-blitz-worker-scope.js", WORKER_SCOPE) {
            Ok(_) => {
                std::future::poll_fn(|cx| {
                    if closing.get() {
                        return Poll::Ready(Ok(()));
                    }
                    match runtime.poll_event_loop(cx, Default::default()) {
                        Poll::Pending if closing.get() => Poll::Ready(Ok(())),
                        poll => poll,
                    }
                })
                .await
            },
            Err(error) => Err(CoreErrorKind::Js(error).into_box()),
        };
        if let Err(error) = result {
            if !lock(&termination).terminated {
                report(error_details(&error, &url));
            }
        }
    });
}
