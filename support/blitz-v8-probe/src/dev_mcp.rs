//! Private stdio diagnostics for the opt-in development probe.

use deno_core::JsRuntime;
use deno_core::serde_json::{Value, json};
use deno_core::{InspectorMsgKind, InspectorSessionKind, JsRuntimeInspector};
use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

const MAX_SOURCE: usize = 16 * 1024;
const MAX_RESPONSE: usize = 16 * 1024;

thread_local! {
    static BRIDGE: Option<Bridge> = Bridge::start();
}

fn token() -> Option<String> {
    if !std::env::args().any(|arg| arg == "--game")
        || !std::env::args().any(|arg| arg == "--window")
    {
        return None;
    }
    let token = std::env::var("ROVES_PROBE_DEV_MCP_TOKEN").ok()?;
    (token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(token)
}

pub fn enabled() -> bool {
    token().is_some()
}

struct Bridge {
    token: String,
    requests: Receiver<String>,
}

impl Bridge {
    fn start() -> Option<Self> {
        let token = token()?;
        let (send, requests) = sync_channel(1);
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            loop {
                let mut line = String::new();
                // A JSON escape can use six bytes for one byte of source.
                let read = input
                    .by_ref()
                    .take((MAX_SOURCE * 6 + 3) as u64)
                    .read_line(&mut line);
                if !matches!(read, Ok(size) if size > 0)
                    || !line.ends_with('\n')
                    || send.send(line).is_err()
                {
                    break;
                }
            }
        });
        let bridge = Self { token, requests };
        bridge.reply(json!({ "ready": true }));
        Some(bridge)
    }

    fn reply(&self, response: Value) {
        let mut output = std::io::stdout().lock();
        let _ = writeln!(output, "ROVES_DEV_MCP_{} {response}", self.token);
        let _ = output.flush();
    }
}

/// Returns true when the controlling process has closed its input pipe.
pub fn poll(runtime: &mut JsRuntime) -> bool {
    BRIDGE.with(|bridge| {
        let Some(bridge) = bridge else { return false };
        let line = match bridge.requests.try_recv() {
            Ok(line) => line,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => return true,
        };
        let response = match deno_core::serde_json::from_str::<String>(&line) {
            Ok(source) if source.len() <= MAX_SOURCE => evaluate(runtime, &source),
            _ => Err("expected a JSON string of at most 16 KiB".into()),
        };
        bridge.reply(match response {
            Ok(value) => json!({ "value": value }),
            Err(error) => json!({ "error": error }),
        });
        false
    })
}

fn evaluate(runtime: &mut JsRuntime, source: &str) -> Result<Value, String> {
    let (send, receive) = std::sync::mpsc::channel();
    let mut session = JsRuntimeInspector::create_local_session(
        runtime.inspector(),
        Box::new(move |message| {
            if matches!(message.kind, InspectorMsgKind::Message(1)) {
                let _ = send.send(message.content);
            }
        }),
        InspectorSessionKind::NonBlocking {
            wait_for_disconnect: false,
        },
    );
    session.post_message(
        1,
        "Runtime.evaluate",
        Some(json!({
            "expression": source,
            "returnByValue": true,
            "awaitPromise": false,
            "timeout": 1000,
        })),
    );
    let message = receive
        .try_recv()
        .map_err(|_| "Inspector did not respond")?;
    if message.len() > MAX_RESPONSE {
        return Err("Inspector result exceeded 16 KiB".into());
    }
    let message: Value =
        deno_core::serde_json::from_str(&message).map_err(|_| "invalid Inspector result")?;
    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        return Err(error.into());
    }
    if message
        .pointer("/result/result/subtype")
        .and_then(Value::as_str)
        == Some("promise")
    {
        return Err("script must return a JSON value synchronously".into());
    }
    if let Some(exception) = message.pointer("/result/exceptionDetails") {
        return Err(exception
            .pointer("/exception/description")
            .or_else(|| exception.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("JavaScript exception")
            .into());
    }
    message
        .pointer("/result/result/value")
        .cloned()
        .ok_or_else(|| "script must return a JSON value synchronously".into())
}
