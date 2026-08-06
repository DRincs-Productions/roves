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
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use headers::{ContentType, HeaderMapExt};
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
}

impl RovesProtocolHandler {
    pub fn new(close_proxy: Option<Arc<Mutex<EventLoopProxy<AppEvent>>>>) -> Self {
        Self { close_proxy }
    }
}

impl ProtocolHandler for RovesProtocolHandler {
    fn is_fetchable(&self) -> bool {
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
            "exit" | "close_window" => match &self.close_proxy {
                Some(proxy) => match proxy.lock().unwrap().send_event(AppEvent::CloseAllWindows) {
                    Ok(()) => Ok("true"),
                    Err(_) => Err("Failed to reach the main event loop".to_owned()),
                },
                None => Err("No window to close (headless mode)".to_owned()),
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
