/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Reading a request's body in an embedder protocol handler.
//!
//! A request body is not stored in the request: net pulls it from the script that sent it, one
//! chunk at a time, through the body's chunk-request channel (see `net::http_loader`'s
//! `obtain_response_setup_router_callback`, which does the same for HTTP). Roves's `saves:write`
//! uses this to receive save data in a POST body instead of a (multi-megabyte) URL.

use std::future::Future;
use std::pin::Pin;

use ipc_channel::ipc;
use net_traits::request::{BodyChunkRequest, BodyChunkResponse, Request};

/// Starts reading `request`'s whole body and returns a future of its bytes, or `None` when the
/// request has no body or its stream is already closed. The future resolves to `None` if the
/// stream fails. Call it from `ProtocolHandler::load` and await the future in the returned
/// response future: it does not borrow the request.
pub fn read_request_body(request: &Request) -> Option<Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send>>> {
    let stream = request.body.as_ref()?.clone_stream();
    let (sender, receiver) = ipc::channel::<BodyChunkResponse>().ok()?;
    {
        let mut lock = stream.lock();
        let requester = lock.as_mut()?;
        requester.send(BodyChunkRequest::Connect(sender)).ok()?;
        requester.send(BodyChunkRequest::Chunk).ok()?;
    }
    let (done, result) = tokio::sync::oneshot::channel();
    // The script sends the chunks; waiting for them must not block net's async workers.
    std::thread::Builder::new()
        .name("ProtocolRequestBody".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let outcome = loop {
                match receiver.recv() {
                    Ok(BodyChunkResponse::Chunk(chunk)) => {
                        bytes.extend_from_slice(&chunk);
                        let mut lock = stream.lock();
                        let Some(requester) = lock.as_mut() else { break None };
                        if requester.send(BodyChunkRequest::Chunk).is_err() {
                            break None;
                        }
                    },
                    Ok(BodyChunkResponse::Done) => break Some(bytes),
                    Ok(BodyChunkResponse::Error) | Err(_) => break None,
                }
            };
            let _ = done.send(outcome);
        })
        .ok()?;
    Some(Box::pin(async move { result.await.ok().flatten() }))
}
