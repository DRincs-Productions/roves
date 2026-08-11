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
use winit::event_loop::EventLoopProxy;

use crate::desktop::event_loop::AppEvent;

pub struct RovesProtocolHandler {
    /// `None` in headless mode, where there's no window to close and no
    /// winit event loop to send this through in the first place (see
    /// `ServoShellEventLoop::event_loop_proxy`).
    close_proxy: Option<Arc<Mutex<EventLoopProxy<AppEvent>>>>,
    /// The directory packed game content was (or would be) extracted into —
    /// the same directory `FileProtocolHandler` serves from — if this launch
    /// is a packed-content one at all. `None` for a dev `--url` launch, which
    /// has no extraction cache to clear.
    content_cache_dir: Option<PathBuf>,
}

impl RovesProtocolHandler {
    pub fn new(
        close_proxy: Option<Arc<Mutex<EventLoopProxy<AppEvent>>>>,
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
        let command = request.current_url().path().to_owned();

        let result: Result<&'static str, String> = match command.as_str() {
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

fn json_response(request: &Request, body: String) -> Response {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(body.into_bytes());
    response
}
