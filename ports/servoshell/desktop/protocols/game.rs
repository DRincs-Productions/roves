/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `game:` protocol handler — serves bundled game content under a fixed
//! virtual origin (`game://content/...`) instead of the raw absolute
//! `file:` path `file.rs` opens a game's `index.html` from.
//!
//! **Why this exists, when `file.rs` already serves the exact same files:**
//! a `file://` document's `window.location.pathname` is the real OS path
//! (e.g. `/C:/Users/.../dist/index.html`), not a root-relative one — so any
//! client-side history router (React Router, TanStack Router, Vue Router in
//! "history" mode, ...) can never match its own routes against it, and
//! falls back to its own "not found" page immediately. Root-absolute asset
//! references (`<script src="/assets/foo.js">`) have the exact same
//! underlying problem, which `file.rs`'s `rebase_to_content_root` works
//! around for that one case — but a router's own `location.pathname` isn't
//! an asset reference `rebase_to_content_root` ever sees; there is no
//! request to rebase.
//!
//! `game://content/index.html` sidesteps this at the root: it's a real,
//! distinct origin (see `ImmutableOrigin::new_opaque_for_game_content`),
//! with its own authority ("content") standing in for what an actual HTTP
//! deployment's domain would be — so `location.pathname` at boot is simply
//! `/index.html`, and `pushState("/about")` (any router's own client-side
//! navigation) stays same-origin and same-scheme, so the History API
//! allows it. This is the same idea Tauri itself uses (`tauri://localhost/`
//! / `https://tauri.localhost/`) to avoid this exact class of problem,
//! rather than something novel to this fork.
//!
//! A **direct navigation** to a sub-route (a hard reload while on
//! `game://content/about`, or `location.href = "/about"`) has no
//! `about.html` file to serve — same as any static host serving a
//! history-mode SPA needs a fallback rule (nginx's `try_files`, Vite's own
//! dev-server `historyApiFallback`, ...). `load` below serves the bundle's
//! entry HTML for exactly this case (`Destination::Document`, no matching
//! file) — deliberately gated to real navigations only, so a genuinely
//! missing *asset* (an actual 404) still surfaces as an error instead of
//! silently becoming HTML.
//!
//! Registered only for the fixed authority `content` — `mach bundle`/
//! Packmaster/`roves-action` are the only things that ever construct a
//! `game:` URL (see `bundle_launch.rs`), always with that exact host, so
//! anything else reaching this handler is a bug elsewhere, not a real
//! request. `file.rs`'s own on-demand packed-content extraction
//! (`packed_content::PackedContent`) is reused as-is — this handler serves
//! the same on-disk content, just addressed differently.

use std::fs::File;
use std::future::{Future, ready};
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use headers::{ContentLength, ContentRange, ContentType, HeaderMapExt, Range};
use http::Method;
use servo::protocol_handler::{
    Destination, DoneChannel, FetchContext, FILE_CHUNK_SIZE, NetworkError, ProtocolHandler,
    Request, ResourceFetchTiming, Response, ResponseBody, get_range_request_bounds,
    partial_content, range_not_satisfiable_error,
};
use tokio::sync::mpsc::unbounded_channel;

use super::packed_content::PackedContent;

/// The only authority this handler ever serves — see this module's own doc comment.
/// `pub(crate)` so `bundle_launch.rs` can build the exact same `game://<CONTENT_HOST>/...`
/// URLs this handler expects, from one shared source of truth instead of two copies of
/// the literal that could silently drift apart.
pub(crate) const CONTENT_HOST: &str = "content";

pub struct GameProtocolHandler {
    /// Where the bundled content actually lives on disk — what a `game://content/<path>`
    /// request's `<path>` resolves against.
    content_root: PathBuf,
    /// The bundle's entry HTML file, relative to `content_root` (e.g. `"index.html"`) —
    /// see this module's own doc comment on the SPA-fallback behavior this is for.
    entry_html: String,
    packed: Option<PackedContent>,
}

impl GameProtocolHandler {
    pub fn new(content_root: PathBuf, entry_html: String) -> Self {
        let packed = PackedContent::resolve(&content_root);
        Self { content_root, entry_html, packed }
    }

    /// See `PackedContent::ensure_available` — a no-op when this launch has no
    /// managed packed content at all (`--content-compress=none`).
    fn ensure_available(&self, file_path: &Path) {
        if let Some(packed) = &self.packed {
            packed.ensure_available(file_path);
        }
    }

    /// `game://content/foo/bar.js` → `<content_root>/foo/bar.js`. Filters out any
    /// `..`/root/prefix component defensively (a URL's path is already dot-segment-
    /// normalized by the `url` crate for a URL with an authority, which every
    /// `game:` URL this handler ever sees has — but there's no reason to trust that
    /// invariant here when a cheap, explicit filter closes off path traversal
    /// entirely regardless).
    fn resolve_path(&self, url_path: &str) -> PathBuf {
        let relative: PathBuf = Path::new(url_path)
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect();
        self.content_root.join(relative)
    }
}

impl ProtocolHandler for GameProtocolHandler {
    // Trusted first-party content origin, same reasoning as `file.rs`'s own overrides —
    // see that file's own comment.
    fn is_fetchable(&self) -> bool {
        true
    }

    fn is_secure(&self) -> bool {
        true
    }

    fn load<'a>(
        &'a self,
        request: &'a mut Request,
        done_chan: &mut DoneChannel,
        context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send + 'a>> {
        let url = request.current_url();

        if request.method != Method::GET {
            return Box::pin(ready(Response::network_error(NetworkError::InvalidMethod)));
        }

        if url.host_str() != Some(CONTENT_HOST) {
            return Box::pin(ready(Response::network_error(NetworkError::ResourceLoadError(
                format!("game: only serves the \"{CONTENT_HOST}\" host"),
            ))));
        }

        let mut file_path = self.resolve_path(url.path());
        self.ensure_available(&file_path);

        // SPA fallback: a real navigation (not an asset subresource) to a path with
        // no matching file falls back to the bundle's own entry HTML — see this
        // module's own doc comment for why. An asset request that genuinely doesn't
        // exist stays a real error below, not silently a page of HTML.
        if (!file_path.exists() || file_path.is_dir()) && request.destination == Destination::Document
        {
            file_path = self.content_root.join(&self.entry_html);
            self.ensure_available(&file_path);
        }

        if file_path.is_dir() {
            return Box::pin(ready(Response::network_error(NetworkError::ResourceLoadError(
                "Directory listing is not supported".to_owned(),
            ))));
        }

        let response = if let Ok(file) = File::open(&file_path) {
            let file_size = file.metadata().ok().map(|metadata| metadata.len());

            let mut response = Response::new(url, ResourceFetchTiming::new(request.timing_type()));

            let range_header = request.headers.typed_get::<Range>();
            let is_range_request = range_header.is_some();
            let Ok(range) = get_range_request_bounds(range_header, file_size.unwrap_or(0))
                .get_final(file_size)
            else {
                range_not_satisfiable_error(&mut response);
                return Box::pin(ready(response));
            };
            let mut reader = BufReader::with_capacity(FILE_CHUNK_SIZE, file);
            if reader.seek(SeekFrom::Start(range.start as u64)).is_err() {
                return Box::pin(ready(Response::network_error(NetworkError::InvalidMethod)));
            }

            if is_range_request {
                partial_content(&mut response);
            }

            let end_byte = range.end.map(|e| e as u64).or(file_size);
            if let Some(end_byte) = end_byte {
                let start_byte = range.start as u64;
                response.headers.typed_insert(ContentLength(end_byte - start_byte));
                if is_range_request {
                    if let Ok(content_range) = ContentRange::bytes(start_byte..end_byte, file_size) {
                        response.headers.typed_insert(content_range);
                    }
                }
            }

            let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
            response.headers.typed_insert(ContentType::from(mime));

            let (mut done_sender, done_receiver) = unbounded_channel();
            *done_chan = Some((done_sender.clone(), done_receiver));
            *response.body.lock() = ResponseBody::Receiving(vec![]);

            context.filemanager.fetch_file_in_chunks(
                &mut done_sender,
                reader,
                response.body.clone(),
                context.cancellation_listener.clone(),
                range,
            );

            response
        } else {
            Response::network_error(NetworkError::ResourceLoadError("Opening file failed".to_owned()))
        };

        Box::pin(ready(response))
    }
}
