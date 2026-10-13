/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(missing_docs)]

//! A persistent disk tier for the HTTP cache, so that remote resources (e.g. game assets)
//! survive restarts. Storability and freshness are decided by `http-cache-semantics`
//! (RFC 9111), storage is a content-addressable `cacache` directory.
//!
//! Only complete `200` responses to plain `GET` requests over http(s) without credentials
//! are stored. Entries are only ever served while fresh: stale entries are left to the
//! network path, which overwrites them with the new response.

use std::path::PathBuf;
use std::time::SystemTime;

use http::header;
use http_cache_semantics::{BeforeRequest, CacheOptions, CachePolicy};
use log::{debug, warn};
use net_traits::request::{CacheMode, Request};
use net_traits::response::{Response, ResponseBody};
use net_traits::{ResourceFetchTiming, ResourceTimingType};
use servo_url::ServoUrl;

/// Responses larger than this are not written to disk.
const MAX_ENTRY_SIZE: usize = 256 * 1024 * 1024;

/// A persistent HTTP cache stored in a directory.
#[derive(Clone)]
pub struct HttpDiskCache {
    dir: PathBuf,
}

/// A response that is allowed to be stored, waiting for its body to complete.
pub struct PendingDiskStore {
    dir: PathBuf,
    key: String,
    policy: CachePolicy,
}

/// Whether `request` may be answered from, or stored in, the disk cache.
fn is_candidate(request: &Request) -> bool {
    let url = request.current_url();
    request.method == http::Method::GET &&
        matches!(url.scheme(), "http" | "https") &&
        !matches!(request.cache_mode, CacheMode::NoStore | CacheMode::Reload) &&
        !request.headers.contains_key(header::AUTHORIZATION) &&
        !request.headers.contains_key(header::COOKIE)
}

/// The request as seen by `http-cache-semantics`; it only looks at the URI, method and headers.
fn policy_request(request: &Request) -> Option<http::Request<()>> {
    let mut url = request.current_url().into_url();
    url.set_fragment(None);
    let mut policy_request = http::Request::get(url.as_str()).body(()).ok()?;
    *policy_request.headers_mut() = request.headers.clone();
    Some(policy_request)
}

/// The cache options of a private (single-user) cache, as opposed to a shared proxy cache.
fn cache_options() -> CacheOptions {
    CacheOptions {
        shared: false,
        ..Default::default()
    }
}

impl HttpDiskCache {
    /// Create a disk cache stored in `dir`, which is created on the first write.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Decide, once the response headers are known, whether `response` should be written to
    /// disk once its body is complete.
    pub fn prepare_store(
        &self,
        request: &Request,
        response: &Response,
    ) -> Option<PendingDiskStore> {
        // Range requests and their partial responses are left to the memory cache.
        if !is_candidate(request) ||
            request.headers.contains_key(header::RANGE) ||
            response.status != http::StatusCode::OK
        {
            return None;
        }
        let mut policy_response = http::Response::new(());
        *policy_response.status_mut() = http::StatusCode::OK;
        *policy_response.headers_mut() = response.headers.clone();
        let now = SystemTime::now();
        let policy = CachePolicy::new_options(
            &policy_request(request)?,
            &policy_response,
            now,
            cache_options(),
        );
        // Responses that would never be served fresh are not worth a disk write.
        if !policy.is_storable() || policy.is_stale(now) {
            return None;
        }
        Some(PendingDiskStore {
            dir: self.dir.clone(),
            key: request.current_url().into_string(),
            policy,
        })
    }

    /// Load a fresh stored response for `request`, if there is one.
    pub async fn load(&self, request: &Request) -> Option<Response> {
        if !is_candidate(request) {
            return None;
        }
        let url = request.current_url();
        let entry = cacache::metadata(&self.dir, url.as_str()).await.ok()??;
        let policy: CachePolicy = serde_json::from_value(entry.metadata).ok()?;
        let BeforeRequest::Fresh(parts) =
            policy.before_request(&policy_request(request)?, SystemTime::now())
        else {
            return None;
        };
        // `read_hash` verifies the content integrity, so a corrupted file is a miss.
        let body = cacache::read_hash(&self.dir, &entry.integrity).await.ok()?;
        debug!("serving {url} from the disk cache");
        Some(build_response(url, request.timing_type(), parts, body))
    }

    /// Remove every stored response.
    pub fn clear(&self) {
        if self.dir.exists() &&
            let Err(error) = cacache::clear_sync(&self.dir)
        {
            warn!("Failed to clear the HTTP disk cache: {error}");
        }
    }
}

fn build_response(
    url: ServoUrl,
    timing_type: ResourceTimingType,
    parts: http::response::Parts,
    body: Vec<u8>,
) -> Response {
    let mut response = Response::new(url, ResourceFetchTiming::new(timing_type));
    response.status = parts.status.into();
    // These headers carry an up-to-date `Age`, so the memory cache computes the remaining
    // freshness correctly.
    response.headers = parts.headers;
    *response.body.lock() = ResponseBody::Done(body);
    response
}

impl PendingDiskStore {
    /// Write the complete `body` to disk, replacing any previous entry for the same URL.
    pub async fn write(self, body: Vec<u8>) {
        if body.len() > MAX_ENTRY_SIZE {
            return;
        }
        let Ok(metadata) = serde_json::to_value(&self.policy) else {
            return;
        };
        let previous = cacache::metadata(&self.dir, &self.key).await.ok().flatten();
        let result = async {
            let integrity = cacache::write_hash(&self.dir, &body).await?;
            let options = cacache::WriteOpts::new()
                .integrity(integrity.clone())
                .size(body.len())
                .metadata(metadata);
            cacache::index::insert_async(&self.dir, &self.key, options).await?;
            // Drop the content the previous entry pointed to, so that updated resources
            // don't accumulate on disk.
            if let Some(previous) = previous &&
                previous.integrity != integrity
            {
                cacache::remove_hash(&self.dir, &previous.integrity).await?;
            }
            Ok::<_, cacache::Error>(())
        }
        .await;
        if let Err(error) = result {
            warn!(
                "Failed to write {} to the HTTP disk cache: {error}",
                self.key
            );
        }
    }
}
