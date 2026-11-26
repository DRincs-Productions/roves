//! Optional local MCP bridge (README "Local MCP bridge", `mcp/README.md`), off unless
//! `--roves-mcp` is given or the package contains the marker `mcp/enabled`. Game mode only.
//! Shared by the Windows player and, through `#[path]`, the macOS and Linux players
//! (`support/cef-macos-gate/src/session.rs`).
//!
//! Transport, one local endpoint per game process, served by tokio on a dedicated thread:
//!
//! - Windows: a named pipe `\\.\pipe\roves-mcp-<pid>-<nonce>` (`named_pipe::ServerOptions`:
//!   remote clients rejected, first instance required, so an existing pipe of that name
//!   cannot be squatted);
//! - macOS/Linux: a unix-domain socket `<pid>.sock` (`tokio::net::UnixListener`) in the
//!   per-user session directory (`mcp_rules::record_dir`), which is created or tightened to
//!   mode 0700 and must be owned by this user; the socket is 0600 and a connection whose
//!   peer credentials (`SO_PEERCRED`/`getpeereid`) name another uid is closed unread.
//!
//! Every connection must first send the 256-bit session token; nothing is read or answered
//! before it. Discovery: a session record `<session directory>/<pid>.json` (`pipe`: the pipe
//! name or the socket path, `token`, `origin`) written through a temporary file and a rename
//! (0600 on unix), and removed, with the socket, when the run starts closing. No TCP listener
//! and no remote-debugging port is opened.
//!
//! Page capability (`isMcpEnabled()` of `@drincs/roves-api/core`): the only thing the page
//! sees of the bridge is a read-only `window.__ROVES_MCP__ === true` in the top-level game
//! document, registered together with the host marker `window.__ROVES__` by `page_init.rs`
//! before the game URL loads (every game-mode launch has that registration; the bridge only
//! adds its marker to the script). Page domain events are drained and dropped; CEF keeps its
//! own JavaScript dialog handling, so dialogs are not delegated to the bridge.
//!
//! Requests are newline-delimited JSON `{"method": ..., "params": {...}}`, handled on the
//! main loop of each player: `state` (host-side state, built by the player), `evaluate` (CDP
//! `Runtime.evaluate` through welding's in-process DevTools channel,
//! `CefBrowserHost::SendDevToolsMessage`, with the page's CSP kept for `eval`), and
//! `click`/`press_key`/`type_text`, which go through the players' ordinary SDL -> welding
//! input translation (`input.rs`, [`click_events`], [`key_events`], [`forward`]). Replies
//! are one JSON line, `{"value": ...}` or `{"error": "..."}`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod, Scancode};
use sdl3::mouse::MouseButton;
use sdl3::video::Window;
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::sync::oneshot;
use welding::CefSurfaceProducer;

use crate::input::Input;
use crate::mcp_rules::{
    CONSOLE_MESSAGE_MAX, CONSOLE_RING, MAX_EXPRESSION, MAX_REQUEST_LINE, MAX_RESULT, McpKey,
    RecordOs, TOKEN_BYTES, hex, point_in_window, record_dir, token_matches, truncate_chars,
    valid_text, write_private,
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
/// Requests queued between the transport thread and the main loop.
const QUEUE: usize = 8;
/// Requests a player handles per loop turn at most (the rest wait for the next turn).
pub(crate) const CALLS_PER_TURN: usize = 4;
/// SDL device id of bridge-generated input events (they are never pushed into the SDL queue).
pub(crate) const INPUT_DEVICE_ID: u32 = 0x524F_4D43;
/// Package marker (path parts under the package's resource directory: next to the
/// executable on Windows and Linux, `Contents/Resources` on macOS) that turns the bridge on
/// for every game-mode launch; written only into MCP-enabled development packages.
pub(crate) const PACKAGE_MARKER: [&str; 2] = ["mcp", "enabled"];

/// What turned the local MCP bridge on.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Source {
    /// `--roves-mcp`.
    Switch,
    /// The package marker `mcp/enabled`.
    Package,
}

/// The bridge's source for a game-mode launch: the switch, else the package marker found
/// under `resources` (see [`PACKAGE_MARKER`]), else off.
pub(crate) fn source(switch: bool, resources: Option<&Path>) -> Option<Source> {
    let marker = resources.map(|dir| {
        PACKAGE_MARKER
            .iter()
            .fold(dir.to_path_buf(), |path, part| path.join(part))
    });
    if switch {
        Some(Source::Switch)
    } else if marker.is_some_and(|marker| marker.is_file()) {
        Some(Source::Package)
    } else {
        None
    }
}

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

/// Main-loop side of the bridge. Dropping it removes the session record (and the unix
/// socket), so discovery stops finding the game; the transport thread then answers nothing
/// but "closed".
pub(crate) struct Bridge {
    requests: Receiver<Request>,
    /// The endpoint: the pipe name on Windows, the socket path on macOS/Linux.
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
        transport::remove(&self.pipe);
    }
}

/// Cryptographically random bytes from the OS (`getrandom`: `ProcessPrng` on Windows,
/// `getentropy` on macOS, `getrandom(2)` on Linux), hex-encoded.
fn random_hex(len: usize) -> Result<String, String> {
    let mut bytes = vec![0_u8; len];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("getrandom: {e}"))?;
    Ok(hex(&bytes))
}

/// Opens the endpoint, then publishes the session record. `origin`: the game's loopback
/// origin.
pub(crate) fn start(origin: &str) -> Result<Bridge, String> {
    let os = RecordOs::CURRENT.ok_or("no MCP bridge on this platform")?;
    let token = random_hex(TOKEN_BYTES)?;
    let pid = std::process::id();
    let dir = record_dir(os, |name| std::env::var_os(name))?;
    transport::prepare(&dir)?;
    let pipe = transport::endpoint(&dir, pid)?;
    let (send, requests) = sync_channel(QUEUE);
    let (ready_send, ready) = std::sync::mpsc::channel();
    let (thread_pipe, thread_token) = (pipe.clone(), token.clone());
    std::thread::Builder::new()
        .name("roves-mcp".into())
        .spawn(move || serve(&thread_pipe, &thread_token, send, ready_send))
        .map_err(|e| format!("transport thread: {e}"))?;
    ready
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "the transport thread did not start".to_owned())??;
    let record = dir.join(format!("{pid}.json"));
    let body = json!({
        "version": 1,
        "pid": pid,
        "pipe": pipe,
        "token": token,
        "origin": origin,
    });
    if let Err(error) = write_private(
        &record,
        &dir.join(format!("{pid}.json.tmp")),
        body.to_string().as_bytes(),
    ) {
        transport::remove(&pipe);
        return Err(error);
    }
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

/// Transport thread: a current-thread tokio runtime serving the endpoint.
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
    runtime.block_on(transport::listen(pipe, token, requests, ready));
}

#[cfg(windows)]
mod transport {
    //! Windows named pipe, one instance per connection.
    use std::path::Path;
    use std::sync::mpsc::{Sender, SyncSender};

    use tokio::net::windows::named_pipe::ServerOptions;

    use super::{Request, connection, random_hex};
    use crate::mcp_rules::{NONCE_BYTES, pipe_name};

    /// The session directory lives in the user's profile (its ACL): only created here.
    pub(super) fn prepare(dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
    }

    /// `\\.\pipe\roves-mcp-<pid>-<nonce>`: the nonce keeps the name unguessable from the PID.
    pub(super) fn endpoint(_dir: &Path, pid: u32) -> Result<String, String> {
        Ok(pipe_name(pid, &random_hex(NONCE_BYTES)?))
    }

    /// A pipe disappears with its last handle.
    pub(super) fn remove(_pipe: &str) {}

    pub(super) async fn listen(
        pipe: &str,
        token: &str,
        requests: SyncSender<Request>,
        ready: Sender<Result<(), String>>,
    ) {
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
    }
}

#[cfg(unix)]
mod transport {
    //! Unix-domain socket in the private session directory, peer uid checked.
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::mpsc::{Sender, SyncSender};
    use std::time::Duration;

    use tokio::net::UnixListener;

    use super::{Request, connection};

    fn uid() -> u32 {
        // SAFETY: geteuid has no preconditions and cannot fail.
        unsafe { libc::geteuid() }
    }

    /// Creates or tightens the session directory: 0700, owned by this user, not a symlink.
    pub(super) fn prepare(dir: &Path) -> Result<(), String> {
        crate::mcp_rules::private_dir(dir, uid())
    }

    /// `<session directory>/<pid>.sock`. The directory is private, so the name needs no
    /// nonce; a stale socket of an earlier process with the same PID is replaced at bind.
    pub(super) fn endpoint(dir: &Path, pid: u32) -> Result<String, String> {
        let path = dir.join(format!("{pid}.sock"));
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{}: not valid UTF-8", path.display()))
    }

    pub(super) fn remove(path: &str) {
        let _ = std::fs::remove_file(path);
    }

    pub(super) async fn listen(
        path: &str,
        token: &str,
        requests: SyncSender<Request>,
        ready: Sender<Result<(), String>>,
    ) {
        let _ = std::fs::remove_file(path);
        let bound = UnixListener::bind(path).and_then(|listener| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map(|()| listener)
        });
        let listener = match bound {
            Ok(listener) => {
                let _ = ready.send(Ok(()));
                listener
            },
            Err(e) => {
                remove(path);
                let _ = ready.send(Err(format!("{path}: {e}")));
                return;
            },
        };
        let uid = uid();
        loop {
            match listener.accept().await {
                // Same user only (the 0700 directory already keeps others out; root is not
                // refused by the directory, so the credentials are checked too).
                Ok((stream, _)) => {
                    if stream.peer_cred().is_ok_and(|peer| peer.uid() == uid) {
                        tokio::spawn(connection(stream, token.to_owned(), requests.clone()));
                    }
                },
                // Out of descriptors and the like: try again shortly instead of spinning.
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
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

async fn connection<S: AsyncRead + AsyncWrite + Send + 'static>(
    stream: S,
    token: String,
    requests: SyncSender<Request>,
) {
    let (read, mut write) = tokio::io::split(stream);
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
    fn send_cdp<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        method: &str,
        params: Value,
    ) -> Result<i64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "id": id, "method": method, "params": params });
        producer
            .send_devtools_message(&message.to_string())
            .map(|()| id)
            .map_err(|error| format!("{method}: {error}"))
    }

    /// Ends an evaluation: answers it and releases its object group (the reply to the
    /// release carries no pending id and is dropped by `poll_devtools`).
    fn finish<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
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
    pub(crate) fn evaluate<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
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

    /// One DevTools message not consumed by the page marker registration: advances the
    /// matching evaluation. Other messages (Page domain events, `releaseObjectGroup` replies)
    /// are dropped.
    pub(crate) fn devtools_message<P: CefSurfaceProducer>(
        &mut self,
        producer: &mut P,
        message: &Value,
    ) {
        let Some(pending) = message
            .get("id")
            .and_then(Value::as_i64)
            .and_then(|id| self.pending.remove(&id))
        else {
            return;
        };
        match (pending.stage, evaluation_step(message)) {
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

    /// Answers evaluations with no CDP reply in time.
    pub(crate) fn expire<P: CefSurfaceProducer>(&mut self, producer: &mut P) {
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

/// `click`: left button down + up at window point (`x`, `y`) (SDL logical coordinates, CSS
/// pixels at page zoom 1), as the SDL events a real click produces.
pub(crate) fn click_events(window: &Window, x: f64, y: f64) -> Result<[Event; 2], String> {
    let (width, height) = window.size();
    if !point_in_window(x, y, width, height) {
        return Err(format!(
            "click: ({x}, {y}) is outside the {width}x{height} window (window points)"
        ));
    }
    let (timestamp, window_id, which, mouse_btn, clicks) =
        (0, window.id(), INPUT_DEVICE_ID, MouseButton::Left, 1);
    let (x, y) = (x as f32, y as f32);
    Ok([
        Event::MouseButtonDown {
            timestamp,
            window_id,
            which,
            mouse_btn,
            clicks,
            x,
            y,
        },
        Event::MouseButtonUp {
            timestamp,
            window_id,
            which,
            mouse_btn,
            clicks,
            x,
            y,
        },
    ])
}

/// `press_key`: one key down + up of a fixed non-character key, as the SDL events of a
/// physical key (no text event: SDL sends text separately).
pub(crate) fn key_events(window_id: u32, key: McpKey) -> [Event; 2] {
    let (keycode, scancode) = match key {
        McpKey::ArrowLeft => (Keycode::Left, Scancode::Left),
        McpKey::ArrowRight => (Keycode::Right, Scancode::Right),
        McpKey::ArrowUp => (Keycode::Up, Scancode::Up),
        McpKey::ArrowDown => (Keycode::Down, Scancode::Down),
        McpKey::Enter => (Keycode::Return, Scancode::Return),
        McpKey::Escape => (Keycode::Escape, Scancode::Escape),
        McpKey::Space => (Keycode::Space, Scancode::Space),
        McpKey::Tab => (Keycode::Tab, Scancode::Tab),
        McpKey::Backspace => (Keycode::Backspace, Scancode::Backspace),
        McpKey::Delete => (Keycode::Delete, Scancode::Delete),
        McpKey::Home => (Keycode::Home, Scancode::Home),
        McpKey::End => (Keycode::End, Scancode::End),
        McpKey::PageUp => (Keycode::PageUp, Scancode::PageUp),
        McpKey::PageDown => (Keycode::PageDown, Scancode::PageDown),
    };
    let (timestamp, keycode, scancode, keymod, repeat, which, raw) = (
        0,
        Some(keycode),
        Some(scancode),
        Mod::empty(),
        false,
        INPUT_DEVICE_ID,
        0,
    );
    [
        Event::KeyDown {
            timestamp,
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            which,
            raw,
        },
        Event::KeyUp {
            timestamp,
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            which,
            raw,
        },
    ]
}

/// `type_text`: committed text, as SDL's text input event.
pub(crate) fn text_event(window_id: u32, text: String) -> Event {
    Event::TextInput {
        timestamp: 0,
        window_id,
        text,
    }
}

/// Feeds bridge-generated SDL events to the game's `Input`, exactly as the loop feeds real
/// ones; the caller checks first that real input is forwarded too. `log` receives `Input`'s
/// own log lines. The value is `{ "forwarded": <welding input calls> }`.
pub(crate) fn forward<P: CefSurfaceProducer>(
    input: &mut Input,
    producer: &mut P,
    window: &Window,
    events: &[Event],
    mut log: impl FnMut(String),
) -> Result<Value, String> {
    let mut forwarded = 0;
    for event in events {
        let (before, rejected) = (input.forwarded, input.rejected);
        if let Some(line) = input.window_event(producer, window, event, true) {
            log(line);
        }
        if input.rejected > rejected {
            return Err("the browser refused the input event".into());
        }
        forwarded += input.forwarded - before;
    }
    Ok(json!({ "forwarded": forwarded }))
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
