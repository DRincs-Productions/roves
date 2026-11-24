//! Optional local MCP bridge (README "Local MCP bridge"), off unless `--roves-mcp` is given
//! or the package contains `mcp\enabled`. Game mode only.
//!
//! Transport: one Windows named pipe per game process (`\\.\pipe\roves-mcp-<pid>-<nonce>`),
//! served by tokio's `named_pipe::ServerOptions` (remote clients rejected, first instance
//! required, so an existing pipe of that name cannot be squatted) on a dedicated thread.
//! Every connection must first send the 256-bit session token; nothing is read or answered
//! before it. Discovery: a session record `%LOCALAPPDATA%\Roves\mcp\<pid>.json` (pipe, token,
//! origin) written through a temporary file and a rename, inside the user's own profile, and
//! removed when the run starts closing. No TCP listener and no remote-debugging port is
//! opened; nothing is injected into, or exposed to, the page.
//!
//! Requests are newline-delimited JSON `{"method": ..., "params": {...}}`, handled on the
//! main loop: `state` (host-side state), `evaluate` (CDP `Runtime.evaluate` through welding's
//! in-process DevTools channel, `CefBrowserHost::SendDevToolsMessage`, with the page's CSP
//! kept for `eval`), and `click`/`press_key`/`type_text`, which go through the probe's
//! ordinary SDL -> welding input translation. Replies are one JSON line,
//! `{"value": ...}` or `{"error": "..."}`.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::oneshot;
use welding::{CefSurfaceProducer as _, windows_cef::WindowsCefProducer};

use crate::mcp_rules::{
    CONSOLE_MESSAGE_MAX, CONSOLE_RING, MAX_EXPRESSION, MAX_REQUEST_LINE, MAX_RESULT, McpKey,
    NONCE_BYTES, TOKEN_BYTES, hex, pipe_name, token_matches, truncate_chars, valid_text,
};

/// CDP message ids of the bridge start here, far from welding's own screenshot ids (from 1).
const CDP_ID_BASE: i64 = 0x5256_0000;
/// The first line of a connection must be the token within this time.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
/// An authenticated connection with no request for this long is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// A request not answered by the main loop within this time gets an error.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A pending `evaluate` with no CDP reply within this time gets an error.
const EVALUATE_TIMEOUT: Duration = Duration::from_secs(5);
/// Script time limit handed to `Runtime.evaluate` (milliseconds).
const SCRIPT_TIMEOUT_MS: u64 = 1000;
/// Requests queued between the pipe thread and the main loop.
const QUEUE: usize = 8;

/// One parsed request.
pub(crate) enum Call {
    State,
    Evaluate(String),
    Click { x: f64, y: f64 },
    PressKey(McpKey),
    TypeText(String),
}

impl Call {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Call::State => "state",
            Call::Evaluate(_) => "evaluate",
            Call::Click { .. } => "click",
            Call::PressKey(_) => "press_key",
            Call::TypeText(_) => "type_text",
        }
    }
}

/// The answer channel of one request.
pub(crate) struct Reply(oneshot::Sender<String>);

impl Reply {
    pub(crate) fn send(self, result: Result<Value, String>) {
        let line = match result {
            Ok(value) => json!({ "value": value }),
            Err(error) => json!({ "error": error }),
        };
        let _ = self.0.send(line.to_string());
    }
}

struct Request {
    line: String,
    reply: oneshot::Sender<String>,
}

/// Which CDP reply a pending evaluation waits for.
#[derive(Clone, Copy)]
enum Stage {
    /// `Runtime.evaluate` by reference: tells a Promise from other objects.
    Evaluate,
    /// `Runtime.callFunctionOn` returning the evaluated object by value.
    ByValue,
}

struct Pending {
    reply: Reply,
    deadline: Instant,
    stage: Stage,
    /// Object group holding this evaluation's remote objects; released when it ends.
    group: String,
}

/// Main-loop side of the bridge. Dropping it removes the session record, so discovery stops
/// finding the game; the pipe thread then answers nothing but "closed".
pub(crate) struct Bridge {
    requests: Receiver<Request>,
    pub(crate) pipe: String,
    pub(crate) record: PathBuf,
    pending: HashMap<i64, Pending>,
    next_id: i64,
    pub(crate) url: Option<String>,
    pub(crate) title: Option<String>,
    console: VecDeque<Value>,
    pub(crate) calls: u64,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.record);
    }
}

/// `%LOCALAPPDATA%\Roves\mcp`: session records, readable only through the user's profile ACL.
fn record_dir() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .ok_or("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local).join("Roves").join("mcp"))
}

/// Cryptographically random bytes from the system-preferred RNG (`BCryptGenRandom`).
fn random_hex(len: usize) -> Result<String, String> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut bytes = vec![0_u8; len];
    // SAFETY: the buffer is valid for `len` bytes; a null algorithm handle is required with
    // BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            u32::try_from(len).map_err(|_| "random length")?,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(format!("BCryptGenRandom failed status={status:#x}"));
    }
    Ok(hex(&bytes))
}

/// Opens the pipe, then publishes the session record. `origin`: the game's loopback origin.
pub(crate) fn start(origin: &str) -> Result<Bridge, String> {
    let token = random_hex(TOKEN_BYTES)?;
    let pid = std::process::id();
    let pipe = pipe_name(pid, &random_hex(NONCE_BYTES)?);
    let dir = record_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (send, requests) = sync_channel(QUEUE);
    let (ready_send, ready) = std::sync::mpsc::channel();
    let (thread_pipe, thread_token) = (pipe.clone(), token.clone());
    std::thread::Builder::new()
        .name("roves-mcp-pipe".into())
        .spawn(move || serve(&thread_pipe, &thread_token, send, ready_send))
        .map_err(|e| format!("pipe thread: {e}"))?;
    ready
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "the pipe thread did not start".to_owned())??;
    let record = dir.join(format!("{pid}.json"));
    let temporary = dir.join(format!("{pid}.json.tmp"));
    let body = json!({
        "version": 1,
        "pid": pid,
        "pipe": pipe,
        "token": token,
        "origin": origin,
    });
    std::fs::write(&temporary, body.to_string())
        .and_then(|()| std::fs::rename(&temporary, &record))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            format!("{}: {e}", record.display())
        })?;
    Ok(Bridge {
        requests,
        pipe,
        record,
        pending: HashMap::new(),
        next_id: CDP_ID_BASE,
        url: None,
        title: None,
        console: VecDeque::new(),
        calls: 0,
    })
}

/// Pipe thread: a current-thread tokio runtime accepting one connection per pipe instance.
fn serve(
    pipe: &str,
    token: &str,
    requests: SyncSender<Request>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = ready.send(Err(format!("tokio runtime: {e}")));
            return;
        },
    };
    runtime.block_on(async move {
        let options = |first: bool| {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create(pipe)
        };
        let mut server = match options(true) {
            Ok(server) => {
                let _ = ready.send(Ok(()));
                server
            },
            Err(e) => {
                let _ = ready.send(Err(format!("{pipe}: {e}")));
                return;
            },
        };
        loop {
            let connected = server.connect().await;
            // The next instance exists before this one is handed over, so a client never
            // finds the name missing between connections.
            let Ok(next) = options(false) else { return };
            let current = std::mem::replace(&mut server, next);
            if connected.is_ok() {
                tokio::spawn(connection(current, token.to_owned(), requests.clone()));
            }
        }
    });
}

async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Option<String> {
    let mut line = String::new();
    let read = (&mut *reader)
        .take(MAX_REQUEST_LINE as u64)
        .read_line(&mut line)
        .await
        .ok()?;
    (read > 0 && line.ends_with('\n')).then_some(line)
}

async fn connection(pipe: NamedPipeServer, token: String, requests: SyncSender<Request>) {
    let (read, mut write) = tokio::io::split(pipe);
    let mut read = BufReader::new(read);
    match tokio::time::timeout(AUTH_TIMEOUT, read_line(&mut read)).await {
        Ok(Some(line)) if token_matches(&token, &line) => {},
        _ => return,
    }
    while let Ok(Some(line)) = tokio::time::timeout(IDLE_TIMEOUT, read_line(&mut read)).await {
        let (reply, answer) = oneshot::channel();
        let (text, keep) = match requests.try_send(Request { line, reply }) {
            Ok(()) => match tokio::time::timeout(REQUEST_TIMEOUT, answer).await {
                Ok(Ok(text)) => (text, true),
                Ok(Err(_)) => (error_line("the game is closing"), false),
                Err(_) => (error_line("the game did not answer in time"), true),
            },
            Err(TrySendError::Full(_)) => (error_line("busy: too many queued requests"), true),
            Err(TrySendError::Disconnected(_)) => (error_line("the game is closing"), false),
        };
        if write
            .write_all(format!("{text}\n").as_bytes())
            .await
            .is_err()
            || !keep
        {
            return;
        }
    }
}

fn error_line(error: &str) -> String {
    json!({ "error": error }).to_string()
}

/// Parses one request line; `Err` is the error to send back.
fn parse(line: &str) -> Result<Call, String> {
    let request: Value = serde_json::from_str(line).map_err(|_| "invalid JSON request")?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or("missing method")?;
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let number = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("{method}: missing number {name}"))
    };
    let string = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{method}: missing string {name}"))
    };
    Ok(match method {
        "state" => Call::State,
        "evaluate" => {
            let expression = string("expression")?;
            if expression.is_empty() || expression.len() > MAX_EXPRESSION {
                return Err(format!(
                    "evaluate: expression must be 1..={MAX_EXPRESSION} bytes"
                ));
            }
            Call::Evaluate(expression)
        },
        "click" => Call::Click {
            x: number("x")?,
            y: number("y")?,
        },
        "press_key" => {
            let key = string("key")?;
            Call::PressKey(McpKey::parse(&key).ok_or_else(|| {
                let names: Vec<_> = McpKey::ALL.iter().map(|k| k.name()).collect();
                format!(
                    "press_key: unsupported key {key:?} (one of {})",
                    names.join(", ")
                )
            })?)
        },
        "type_text" => {
            let text = string("text")?;
            if !valid_text(&text) {
                return Err(
                    "type_text: 1..=256 printable characters (use press_key for keys)".into(),
                );
            }
            Call::TypeText(text)
        },
        other => return Err(format!("unknown method {other:?}")),
    })
}

impl Bridge {
    /// Up to `max` requests; malformed ones are answered here and not returned.
    pub(crate) fn take(&mut self, max: usize) -> Vec<(Call, Reply)> {
        let mut calls = Vec::new();
        while calls.len() < max {
            let request = match self.requests.try_recv() {
                Ok(request) => request,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            self.calls += 1;
            let reply = Reply(request.reply);
            match parse(&request.line) {
                Ok(call) => calls.push((call, reply)),
                Err(error) => reply.send(Err(error)),
            }
        }
        calls
    }

    /// Sends one CDP command of the bridge and returns its message id.
    fn send_cdp(
        &mut self,
        producer: &mut WindowsCefProducer,
        method: &str,
        params: Value,
    ) -> Result<i64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "id": id, "method": method, "params": params });
        producer
            .send_devtools_message(&message.to_string())
            .map(|()| id)
            .map_err(|error| format!("evaluate: {error}"))
    }

    /// Ends an evaluation: answers it and releases its object group (the reply to the
    /// release carries no pending id and is dropped by `poll_devtools`).
    fn finish(
        &mut self,
        producer: &mut WindowsCefProducer,
        pending: Pending,
        result: Result<Value, String>,
    ) {
        pending.reply.send(result);
        let _ = self.send_cdp(
            producer,
            "Runtime.releaseObjectGroup",
            json!({ "objectGroup": pending.group }),
        );
    }

    /// Starts `Runtime.evaluate` in the page's main world: synchronous only
    /// (`awaitPromise: false`), no user gesture, and `allowUnsafeEvalBlockedByCSP: false` so
    /// `eval`/`new Function` inside the expression stay subject to the page's CSP.
    ///
    /// The result is requested by reference, not with `returnByValue`: V8's inspector
    /// serializes a by-value object as plain JSON without its `subtype`, so a Promise would
    /// arrive as `{"type":"object","value":{}}`, indistinguishable from `({})`. A reference
    /// keeps `subtype: "promise"` for the refusal; any other object is then fetched by value
    /// with `Runtime.callFunctionOn` on the same reference (same JSON serialization as
    /// `returnByValue`). Each evaluation has its own object group, released at its end.
    pub(crate) fn evaluate(
        &mut self,
        producer: &mut WindowsCefProducer,
        expression: &str,
        reply: Reply,
    ) {
        let group = format!("roves-mcp-{}", self.next_id);
        let params = json!({
            "expression": expression,
            "objectGroup": group,
            "returnByValue": false,
            "generatePreview": false,
            "awaitPromise": false,
            "userGesture": false,
            "allowUnsafeEvalBlockedByCSP": false,
            "timeout": SCRIPT_TIMEOUT_MS,
        });
        let pending = Pending {
            reply,
            deadline: Instant::now() + EVALUATE_TIMEOUT,
            stage: Stage::Evaluate,
            group,
        };
        match self.send_cdp(producer, "Runtime.evaluate", params) {
            Ok(id) => {
                self.pending.insert(id, pending);
            },
            Err(error) => self.finish(producer, pending, Err(error)),
        }
    }

    /// Drains welding's DevTools queue (it is bounded, so it is polled every loop turn while
    /// the bridge is on), advances matching evaluations and expires late ones. Messages that
    /// are not replies to a pending evaluation (events, `releaseObjectGroup` replies) are
    /// dropped: no CDP domain is enabled, so no events are expected.
    pub(crate) fn poll_devtools(&mut self, producer: &mut WindowsCefProducer) {
        while let Some(message) = producer.poll_devtools_message() {
            let Ok(message) = serde_json::from_str::<Value>(&message) else {
                continue;
            };
            let Some(pending) = message
                .get("id")
                .and_then(Value::as_i64)
                .and_then(|id| self.pending.remove(&id))
            else {
                continue;
            };
            match (pending.stage, evaluation_step(&message)) {
                (Stage::Evaluate, Step::Fetch(object)) => {
                    let params = json!({
                        "objectId": object,
                        "functionDeclaration": "function () { return this; }",
                        "objectGroup": pending.group,
                        "returnByValue": true,
                        "awaitPromise": false,
                        "userGesture": false,
                    });
                    match self.send_cdp(producer, "Runtime.callFunctionOn", params) {
                        Ok(id) => {
                            self.pending.insert(
                                id,
                                Pending {
                                    stage: Stage::ByValue,
                                    ..pending
                                },
                            );
                        },
                        Err(error) => self.finish(producer, pending, Err(error)),
                    }
                },
                (Stage::Evaluate, Step::Done(result)) => self.finish(producer, pending, result),
                // By value there is no reference left to fetch: a by-value object without
                // `value` is not JSON-serializable.
                (Stage::ByValue, Step::Fetch(_)) => self.finish(
                    producer,
                    pending,
                    Err("evaluate: the result is not JSON-serializable".into()),
                ),
                (Stage::ByValue, Step::Done(result)) => self.finish(producer, pending, result),
            }
        }
        let now = Instant::now();
        let expired: Vec<i64> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(pending) = self.pending.remove(&id) {
                self.finish(
                    producer,
                    pending,
                    Err("evaluate: no DevTools reply in time".into()),
                );
            }
        }
    }

    pub(crate) fn console(
        &mut self,
        level: impl std::fmt::Debug,
        message: &str,
        source: &str,
        line: i32,
    ) {
        if self.console.len() == CONSOLE_RING {
            self.console.pop_front();
        }
        self.console.push_back(json!({
            "level": format!("{level:?}"),
            "message": truncate_chars(message, CONSOLE_MESSAGE_MAX),
            "source": truncate_chars(source, CONSOLE_MESSAGE_MAX),
            "line": line,
        }));
    }

    pub(crate) fn recent_console(&self) -> Value {
        Value::Array(self.console.iter().cloned().collect())
    }
}

/// Next step of an evaluation after one CDP reply.
enum Step {
    /// The bridge's answer.
    Done(Result<Value, String>),
    /// A non-Promise object by reference (`objectId`), to be fetched by value.
    Fetch(String),
}

/// Maps a CDP `Runtime.evaluate` (by reference) or `Runtime.callFunctionOn` (by value) reply.
/// Exceptions arrive in `exceptionDetails` in both modes (V8 wraps a thrown value by
/// reference regardless of `returnByValue`).
fn evaluation_step(message: &Value) -> Step {
    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        return Step::Done(Err(format!("evaluate: {error}")));
    }
    if let Some(exception) = message.pointer("/result/exceptionDetails") {
        return Step::Done(Err(exception
            .pointer("/exception/description")
            .or_else(|| exception.get("text"))
            .and_then(Value::as_str)
            .map_or_else(
                || "JavaScript exception".to_owned(),
                |text| truncate_chars(text, 2048).to_owned(),
            )));
    }
    let Some(result) = message.pointer("/result/result") else {
        return Step::Done(Err("evaluate: invalid DevTools reply".into()));
    };
    if result.get("subtype").and_then(Value::as_str) == Some("promise") {
        return Step::Done(Err(
            "evaluate: the expression returned a Promise; return a JSON value \
                    synchronously and read asynchronous results with a later call"
                .into(),
        ));
    }
    let value = if let Some(value) = result.get("value") {
        value.clone()
    } else if let Some(text) = result.get("unserializableValue").and_then(Value::as_str) {
        // NaN, Infinity, -0 and BigInt literals.
        Value::String(text.to_owned())
    } else if result.get("type").and_then(Value::as_str) == Some("undefined") {
        Value::Null
    } else if let (Some("object" | "function"), Some(object)) = (
        result.get("type").and_then(Value::as_str),
        result.get("objectId").and_then(Value::as_str),
    ) {
        // Objects and functions only: a symbol is not serializable by value either (V8
        // refuses it in returnByValue), so it keeps the error below.
        return Step::Fetch(object.to_owned());
    } else {
        return Step::Done(Err("evaluate: the result is not JSON-serializable".into()));
    };
    if value.to_string().len() > MAX_RESULT {
        return Step::Done(Err(format!(
            "evaluate: the result exceeds {MAX_RESULT} bytes"
        )));
    }
    Step::Done(Ok(value))
}
