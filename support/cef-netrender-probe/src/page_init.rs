//! Page markers of every game-mode launch (README "Host API"): `window.__ROVES__` (the host
//! API marker read by `isAvailable()` of `@drincs/roves-api/core`) and, while the local MCP
//! bridge runs, `window.__ROVES_MCP__` (`isMcpEnabled()`); script in
//! `mcp_rules::page_marker_script`.
//!
//! The browser starts on `about:blank`; this module enables the CDP Page domain (Blink
//! injects document-start scripts only from the enabled page agent's main-world-context
//! probe) and registers the script with `Page.addScriptToEvaluateOnNewDocument`, both through
//! welding's in-process DevTools channel (`CefBrowserHost::SendDevToolsMessage`, no port).
//! Chromium stores both in the DevTools session and re-applies them in every later renderer,
//! across process swaps. The host loads the game URL only after the registration settled; a
//! refused or late registration loads the game without the markers.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use welding::{CefSurfaceProducer as _, windows_cef::WindowsCefProducer};

use crate::mcp_rules::truncate_chars;

/// CDP message ids of the registration, apart from the bridge's (`0x5256_0000..`) and
/// welding's own screenshot ids (from 1).
const CDP_ID_BASE: i64 = 0x5256_8000;
/// The registration (browser ready, CDP replies) must settle within this time of the first
/// attempt; otherwise the game loads without the markers.
const TIMEOUT: Duration = Duration::from_secs(10);

enum Stage {
    /// Not sent yet (the browser may not exist); `deadline` starts at the first attempt.
    Unsent { deadline: Option<Instant> },
    /// `Page.enable` sent as CDP message `id`. Without it the registration succeeds but
    /// nothing ever runs (`InspectorPageAgent::DidCreateMainWorldContext`).
    Enabling { id: i64, deadline: Instant },
    /// Page domain enabled; the script is sent next turn.
    Enabled { deadline: Instant },
    /// `Page.addScriptToEvaluateOnNewDocument` sent as CDP message `id`.
    Sent { id: i64, deadline: Instant },
    /// Reply or failure, not handed to the host yet.
    Ready(Result<String, String>),
    /// Handed to the host; `registered` holds the outcome.
    Settled,
}

pub(crate) struct PageInit {
    script: String,
    stage: Stage,
    next_id: i64,
    /// `None` until the registration settled, then whether the markers are registered.
    pub(crate) registered: Option<bool>,
}

impl PageInit {
    pub(crate) fn new(script: String) -> Self {
        Self {
            script,
            stage: Stage::Unsent { deadline: None },
            next_id: CDP_ID_BASE,
            registered: None,
        }
    }

    fn send(
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
            .map_err(|error| format!("{method}: {error}"))
    }

    /// Consumes `message` when it is a reply to one of the registration's commands.
    pub(crate) fn handle(&mut self, message: &Value) -> bool {
        let id = message.get("id").and_then(Value::as_i64);
        match self.stage {
            Stage::Enabling { id: sent, deadline } if id == Some(sent) => {
                self.stage = match enable_reply(message) {
                    Ok(()) => Stage::Enabled { deadline },
                    Err(error) => Stage::Ready(Err(error)),
                };
                true
            },
            Stage::Sent { id: sent, .. } if id == Some(sent) => {
                self.stage = Stage::Ready(register_reply(message));
                true
            },
            _ => false,
        }
    }

    /// Advances the registration; returns its outcome once, when it settled
    /// (`Ok(identifier)` or the reason the markers are unavailable).
    pub(crate) fn step(
        &mut self,
        producer: &mut WindowsCefProducer,
    ) -> Option<Result<String, String>> {
        let now = Instant::now();
        match self.stage {
            Stage::Unsent { deadline } => {
                let deadline = deadline.unwrap_or(now + TIMEOUT);
                // The Page domain stays enabled for the whole session: Chromium keeps it in
                // the DevTools session state and re-enables it in every later renderer, so
                // the script also runs after the about:blank -> game process swap.
                self.stage = match self.send(producer, "Page.enable", json!({})) {
                    Ok(id) => Stage::Enabling { id, deadline },
                    // Typically "the browser is not ready yet": retried next turn.
                    Err(error) if now >= deadline => Stage::Ready(Err(error)),
                    Err(_) => Stage::Unsent {
                        deadline: Some(deadline),
                    },
                };
            },
            Stage::Enabled { deadline } => {
                let params = json!({ "source": self.script.as_str(), "runImmediately": false });
                match self.send(producer, "Page.addScriptToEvaluateOnNewDocument", params) {
                    Ok(id) => self.stage = Stage::Sent { id, deadline },
                    Err(error) if now >= deadline => self.stage = Stage::Ready(Err(error)),
                    Err(_) => {},
                }
            },
            Stage::Enabling { deadline, .. } | Stage::Sent { deadline, .. } if now >= deadline => {
                self.stage = Stage::Ready(Err("no DevTools reply in time".into()));
            },
            _ => {},
        }
        if !matches!(self.stage, Stage::Ready(_)) {
            return None;
        }
        let Stage::Ready(outcome) = std::mem::replace(&mut self.stage, Stage::Settled) else {
            unreachable!("checked above");
        };
        self.registered = Some(outcome.is_ok());
        Some(outcome)
    }
}

/// Maps the CDP reply to `Page.enable`: success, or the reason the markers are unavailable.
fn enable_reply(message: &Value) -> Result<(), String> {
    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        return Err(format!("Page.enable: {}", truncate_chars(error, 512)));
    }
    if message.get("result").is_some_and(Value::is_object) {
        Ok(())
    } else {
        Err("Page.enable: invalid DevTools reply".to_owned())
    }
}

/// Maps the CDP reply to `Page.addScriptToEvaluateOnNewDocument`: its script identifier, or
/// the reason the markers are unavailable.
fn register_reply(message: &Value) -> Result<String, String> {
    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        return Err(truncate_chars(error, 512).to_owned());
    }
    message
        .pointer("/result/identifier")
        .and_then(Value::as_str)
        .map(|id| truncate_chars(id, 64).to_owned())
        .ok_or_else(|| "invalid DevTools reply".to_owned())
}
