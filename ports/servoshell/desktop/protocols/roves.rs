/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Roves' own general-purpose `invoke()` bridge — the `@drincs/roves-api`
//! `core` module (see servo/packages/roves-api) talks to this over a
//! `roves:` custom protocol, fetchable from ordinary page JS
//! (`fetch('roves:exit')`), the same way `protocols/steam.rs` exposes
//! Steamworks over `steam:`. This is the generic "control this app from JS"
//! surface (window/process lifecycle today); Steam has its own dedicated
//! scheme since it's a large, separate SDK surface, not something that
//! belongs generically here.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use headers::{ContentType, HeaderMapExt};
use roves_content_packer::extract;
use servo::protocol_handler::{
    DoneChannel, FetchContext, NetworkError, ProtocolHandler, Request, ResourceFetchTiming,
    Response, ResponseBody,
};
use crate::desktop::event_loop::{AppEvent, EventLoopProxy};

/// Test-only escape hatch used by the packaged-binary smoke tests. When set, a
/// `roves:save_file` request writes to this exact path instead of opening the
/// native picker. Normal launches never set it, so players always keep the
/// interactive "Save As" flow.
pub(crate) const SAVE_FILE_AUTOTEST_PATH_ENV: &str = "ROVES_SAVE_FILE_AUTOTEST_PATH";

pub struct RovesProtocolHandler {
    /// `None` in headless mode, where there's no window to close and no
    /// event loop to send this through in the first place (see
    /// `ServoShellEventLoop::event_loop_proxy`). The `Arc<Mutex<..>>` wrapping predates
    /// `EventLoopProxy` itself being cheaply `Clone`-able (it wraps an `Arc` internally
    /// now) -- kept as-is here to minimize this pass's blast radius.
    close_proxy: Option<Arc<Mutex<EventLoopProxy>>>,
    /// The directory packed game content was (or would be) extracted into —
    /// the same directory `FileProtocolHandler` serves from — if this launch
    /// is a packed-content one at all. `None` for a dev `--url` launch, which
    /// has no extraction cache to clear.
    content_cache_dir: Option<PathBuf>,
}

impl RovesProtocolHandler {
    pub fn new(
        close_proxy: Option<Arc<Mutex<EventLoopProxy>>>,
        content_cache_dir: Option<PathBuf>,
    ) -> Self {
        Self { close_proxy, content_cache_dir }
    }

    /// Sends `AppEvent::CloseAllWindows`, same as the `exit` command — used
    /// after clearing the content cache, since that cache is the *live*
    /// document root while the game runs (see `extract::clear_cache`'s
    /// caller here): leaving the app open afterwards risks a broken load for
    /// any asset not yet extracted this session.
    fn close_all_windows(&self) -> Result<(), String> {
        match &self.close_proxy {
            Some(proxy) => proxy
                .lock()
                .unwrap()
                .send_event(AppEvent::CloseAllWindows)
                .map_err(|_| "Failed to reach the main event loop".to_owned()),
            None => Err("No window to close (headless mode)".to_owned()),
        }
    }

    /// The desktop side of a game's save export — see `AppEvent::SaveFileDialog`'s own doc
    /// comment for the full chain (the injected userscript that intercepts `<a download>`
    /// clicks and lands here, and why this needs the main thread at all). Unlike every other
    /// command in this handler, the response genuinely waits on user interaction (picking a
    /// destination, or cancelling) — potentially a long time — hence a real `Future` here
    /// instead of the `future::ready` every other command returns.
    fn save_file(
        &self,
        suggested_name: String,
        data: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        if let Some(path) = std::env::var_os(SAVE_FILE_AUTOTEST_PATH_ENV) {
            return Box::pin(std::future::ready(
                std::fs::write(PathBuf::from(path), data)
                    .map_err(|error| format!("Failed to write save-file autotest output: {error}")),
            ));
        }

        let Some(proxy) = self.close_proxy.clone() else {
            return Box::pin(std::future::ready(Err(
                "No window to show a save dialog in (headless mode)".to_owned(),
            )));
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let send_result = proxy.lock().unwrap().send_event(AppEvent::SaveFileDialog {
            suggested_name,
            data,
            response: response_tx,
        });
        if send_result.is_err() {
            return Box::pin(std::future::ready(Err(
                "Failed to reach the main event loop".to_owned(),
            )));
        }
        Box::pin(async move {
            match response_rx.await {
                Ok(result) => result,
                // The main thread dropped `response_tx` without ever sending — shouldn't
                // happen (every path through `AppEvent::SaveFileDialog`'s handling in app.rs
                // sends exactly once) but a closed app/window mid-dialog is a real enough
                // possibility not to just unwrap.
                Err(_) => Err("The save dialog was closed without a result".to_owned()),
            }
        })
    }
}

impl ProtocolHandler for RovesProtocolHandler {
    fn is_fetchable(&self) -> bool {
        true
    }

    fn is_secure(&self) -> bool {
        true
    }

    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let command = url.path().to_owned();

        // The one command here that genuinely needs to await a main-thread round trip (the
        // save dialog can stay open for as long as the user takes to decide) — doesn't fit
        // the uniform `Result<&'static str, String>`/`future::ready` shape every other
        // command below uses, so it's handled separately, before that match.
        if command == "save_file" {
            let query: std::collections::HashMap<String, String> =
                url.as_url().query_pairs().into_owned().collect();

            // Captured by value (not `&Request`, which doesn't outlive this call) so the
            // success response can still be built inside the async block below — a closure
            // rather than a second top-level function so the exact `ServoUrl`/
            // `ResourceTimingType` types never need spelling out (they're not otherwise
            // imported into this file; `json_response`'s existing signature only needs
            // `&Request`, which type inference threads through for it already).
            let (response_url, timing_type) = (request.current_url(), request.timing_type());
            let make_json_response = move |body: String| -> Response {
                let mut response = Response::new(response_url, ResourceFetchTiming::new(timing_type));
                response.headers.typed_insert(ContentType::json());
                *response.body.lock() = ResponseBody::Done(body.into_bytes());
                response
            };

            // Resolved synchronously, before the async block below, so that block never
            // needs to borrow `self` — it only needs to `.await` the already-independent
            // future `save_file` hands back (see that method's own doc comment). The
            // explicit type annotation on `outcome_future` (rather than relying on each
            // match arm's own inferred type to line up) is what makes every arm below
            // coerce to the same boxed trait object, since they're otherwise three
            // different concrete `Future` implementations (two `std::future::Ready`s and
            // `save_file`'s own already-boxed one).
            let filename = query.get("filename").cloned();
            let data = query.get("data").and_then(|data| base64_decode(data));
            let outcome_future: Pin<Box<dyn Future<Output = Result<(), String>> + Send>> =
                match (filename, data) {
                    (Some(filename), Some(data)) => self.save_file(filename, data),
                    (None, _) => Box::pin(std::future::ready(Err(
                        "Missing 'filename' parameter".to_owned(),
                    ))),
                    (Some(_), None) => Box::pin(std::future::ready(Err(
                        "Missing or invalid base64 'data' parameter".to_owned(),
                    ))),
                };

            return Box::pin(async move {
                match outcome_future.await {
                    Ok(()) => make_json_response("true".to_owned()),
                    Err(error) => Response::network_error(NetworkError::ResourceLoadError(error)),
                }
            });
        }

        let result: Result<&'static str, String> = match command.as_str() {
            // The generic "is this page actually running inside Roves"
            // check — see `@drincs/roves-api/core`'s `isAvailable()`. Always
            // `true` here: reaching this match arm at all means a `roves:`
            // fetch resolved, which only happens when this protocol handler
            // is registered, i.e. only inside Roves. A regular browser (or
            // any other embedder that never registered `roves:`) fails the
            // `fetch()` itself before ever reaching Rust code, which is
            // exactly what `core.invoke`'s caller sees as "unavailable".
            "is_available" => Ok("true"),
            // Host/engine diagnostics for bug reports and graphics-compatibility triage --
            // see `@drincs/roves-api/core`'s `systemInfo()`. Field names deliberately mirror
            // `@tauri-apps/plugin-os`'s (`type()`/`version()`/`arch()`) and the `os_info`
            // crate's (`os_type`/`version`/`bitness`/`architecture`) conventions, so code
            // that already knows either of those feels at home here too -- `engine_version`
            // is the one addition neither has an equivalent for, since neither wraps an
            // engine a page could plausibly want the *version of*, the way Roves' own
            // `servoshell::VERSION` matters for graphics/compatibility debugging the same way
            // a "webview version" would elsewhere.
            "system_info" => return Box::pin(std::future::ready(json_response(
                request,
                serde_json::json!({
                    "os_type": sysinfo::System::distribution_id(),
                    "os_version": sysinfo::System::os_version(),
                    "bitness": if cfg!(target_pointer_width = "64") { "64-bit" } else { "32-bit" },
                    "architecture": std::env::consts::ARCH,
                    "engine_version": crate::VERSION,
                })
                .to_string(),
            ))),
            // Closes every open window. In this fork's usual single-window
            // kiosk setup (see ../../CUSTOMIZATIONS.md's toolbar/tab removal
            // entries) that's equivalent to quitting the app: once no
            // windows remain, `App::pump_servo_event_loop` returns `false`
            // and the event loop exits on its own — see app.rs.
            "exit" | "close_window" => self.close_all_windows().map(|()| "true"),
            // Wipes the startup extraction cache — not save data, see
            // `support/content-packer`'s doc comments — and then closes the
            // app the same way `exit` does, since that cache is the live
            // document root while running (see `close_all_windows`'s doc
            // comment). The next launch re-extracts fresh from the shipped
            // bundle.
            "clear_content_cache" => match &self.content_cache_dir {
                Some(dir) => extract::clear_cache(dir).and_then(|()| self.close_all_windows()).map(|()| "true"),
                None => Err("No extraction cache to clear (not a packed-content launch)".to_owned()),
            },
            _ => {
                return Box::pin(std::future::ready(Response::network_error(
                    NetworkError::ResourceLoadError(format!("Unknown roves: command '{command}'")),
                )));
            },
        };

        Box::pin(std::future::ready(match result {
            Ok(body) => json_response(request, body.to_owned()),
            Err(error) => Response::network_error(NetworkError::ResourceLoadError(error)),
        }))
    }
}

/// Same base64 alphabet/decoding `protocols/saves.rs` uses for its own `write` command's
/// `data` parameter — this command's `data` parameter is shaped identically (a base64
/// payload in a query string), produced by the same `FileReader.readAsDataURL` pattern on
/// the JS side either way (see the userscript `app.rs` registers).
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

fn json_response(request: &Request, body: String) -> Response {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(body.into_bytes());
    response
}
