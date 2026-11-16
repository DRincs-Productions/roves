//! Private stdio diagnostics for the opt-in development probe.

use deno_core::JsRuntime;
use deno_core::serde_json::{Value, json};
use deno_core::{InspectorMsgKind, InspectorSessionKind, JsRuntimeInspector};
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::Duration;

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
    requests: Receiver<Request>,
    attached: bool,
}

struct Request {
    line: String,
    reply: Option<SyncSender<Value>>,
}

fn read_request(input: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    input
        .take((MAX_SOURCE * 6 + 3) as u64)
        .read_line(&mut line)
        .ok()?;
    line.ends_with('\n').then_some(line)
}

impl Bridge {
    fn start() -> Option<Self> {
        let token = token()?;
        let (send, requests) = sync_channel(1);
        if let Ok(port) = std::env::var("ROVES_PROBE_DEV_MCP_ATTACH_PORT") {
            let port = port.parse::<u16>().expect("invalid diagnostic port");
            assert_ne!(port, 0, "configure an explicit diagnostic port");
            let listener = TcpListener::bind(("127.0.0.1", port))
                .expect("could not listen on the diagnostic port");
            let auth = token.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().map_while(Result::ok) {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                    let mut input = std::io::BufReader::new(stream);
                    if read_request(&mut input).as_deref().map(str::trim_end) != Some(auth.as_str())
                    {
                        continue;
                    }
                    while let Some(line) = read_request(&mut input) {
                        let (reply, response) = sync_channel(1);
                        if send
                            .send(Request {
                                line,
                                reply: Some(reply),
                            })
                            .is_err()
                        {
                            return;
                        }
                        let Ok(value) = response.recv_timeout(Duration::from_secs(5)) else {
                            break;
                        };
                        if writeln!(input.get_mut(), "{value}").is_err() {
                            break;
                        }
                    }
                }
            });
            return Some(Self {
                token,
                requests,
                attached: true,
            });
        }
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Some(line) = read_request(&mut input) {
                if send.send(Request { line, reply: None }).is_err() {
                    break;
                }
            }
        });
        let bridge = Self {
            token,
            requests,
            attached: false,
        };
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
pub fn poll(
    runtime: &mut JsRuntime,
    timings: Option<&crate::frame_timings::SharedFrameTimings>,
    stats: Option<&std::cell::RefCell<crate::presentation::PresentStats>>,
) -> bool {
    BRIDGE.with(|bridge| {
        let Some(bridge) = bridge else { return false };
        let request = match bridge.requests.try_recv() {
            Ok(request) => request,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => return !bridge.attached,
        };
        let response = match deno_core::serde_json::from_str::<Value>(&request.line) {
            Ok(Value::Object(request))
                if request.get("method").and_then(Value::as_str) == Some("performance_sample") =>
            {
                timings
                    .map(|timings| {
                        let mut sample = timings.borrow().live_sample();
                        let stats = stats.map(std::cell::RefCell::borrow);
                        sample["session_present_calls"] = json!(stats.as_ref().map(|s| s.frames));
                        sample["session_skipped_frames"] =
                            json!(stats.as_ref().map(|s| s.skipped_frames));
                        sample
                    })
                    .ok_or_else(|| "frame timings are unavailable".into())
            },
            Ok(Value::String(source)) if source.len() <= MAX_SOURCE => evaluate(runtime, &source),
            _ => Err("expected a JSON string of at most 16 KiB".into()),
        };
        let response = match response {
            Ok(value) => json!({ "value": value }),
            Err(error) => json!({ "error": error }),
        };
        if let Some(reply) = request.reply {
            let _ = reply.send(response);
        } else {
            bridge.reply(response);
        }
        false
    })
}

fn evaluate(runtime: &mut JsRuntime, source: &str) -> Result<Value, String> {
    let (send, receive) = std::sync::mpsc::channel();
    let mut session = JsRuntimeInspector::create_local_session(
        runtime.inspector(),
        Box::new(move |message| {
            if matches!(message.kind, InspectorMsgKind::Message(_)) {
                let _ = send.send(message.content);
            }
        }),
        InspectorSessionKind::NonBlocking {
            wait_for_disconnect: false,
        },
    );
    let mut next_id = 0;
    let mut request = |method, params| {
        next_id += 1;
        session.post_message(next_id, method, Some(params));
        inspector_result(
            receive
                .try_recv()
                .map_err(|_| "Inspector did not respond")?,
        )
    };
    let result = request(
        "Runtime.evaluate",
        json!({
            "expression": source,
            "awaitPromise": false,
            "allowUnsafeEvalBlockedByCSP": false,
            "timeout": 1000,
        }),
    )?;
    if let Some(value) = result.get("value") {
        return Ok(value.clone());
    }
    // Returning by value directly would turn a Promise into an empty object
    // and discard its subtype. Inspect the remote object first.
    if result["type"] == "object"
        && result["subtype"] != "promise"
        && let Some(object_id) = result["objectId"].as_str()
    {
        let value = request(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": "function () { return this; }",
                "returnByValue": true,
                "awaitPromise": false,
            }),
        )?;
        if let Some(value) = value.get("value") {
            return Ok(value.clone());
        }
    }
    Err("script must return a JSON value synchronously".into())
}

fn inspector_result(message: String) -> Result<Value, String> {
    if message.len() > MAX_RESPONSE {
        return Err("Inspector result exceeded 16 KiB".into());
    }
    let message: Value =
        deno_core::serde_json::from_str(&message).map_err(|_| "invalid Inspector result")?;
    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        return Err(error.into());
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
        .pointer("/result/result")
        .cloned()
        .ok_or_else(|| "invalid Inspector result".into())
}
