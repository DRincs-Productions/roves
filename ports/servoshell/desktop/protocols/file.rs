/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `file:` protocol handler — replicates Servo's own upstream behavior
//! (plain filesystem reads, HTTP Range support) with one addition: if the
//! requested path doesn't exist yet *and* falls under a directory known to
//! hold lazily-extracted packed game content, it's extracted on demand —
//! from whichever `.pack` archive contains it — before the read proceeds.
//! See `support/content-packer` and CUSTOMIZATIONS.md's content-compression
//! entries for the full design.
//!
//! Registered for the `file` scheme in `app.rs`, taking over from the
//! engine's own internal default — see `components/servo/servo.rs`'s
//! protocol-registry merge-order change, without which an embedder can't
//! override `file` at all.
//!
//! Deliberately does **not** replicate the stock handler's directory-listing
//! fallback (`local_directory_listing`): Roves never opens more than one
//! `file://` document and never navigates to a bare directory (no address
//! bar, no tabs), so that upstream-only code path doesn't apply here and
//! isn't worth the extra `pub(crate)`-visibility patch to `components/net`
//! it would need. A directory request just becomes a network error instead.

use std::fs::File;
use std::future::{Future, ready};
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use headers::{ContentLength, ContentRange, ContentType, HeaderMapExt, Range};
use http::Method;
use servo::protocol_handler::{
    DoneChannel, FetchContext, FILE_CHUNK_SIZE, NetworkError, ProtocolHandler, Request,
    ResourceFetchTiming, Response, ResponseBody, get_range_request_bounds, partial_content,
    range_not_satisfiable_error,
};
use tokio::sync::mpsc::unbounded_channel;

use super::packed_content::PackedContent;

pub struct FileProtocolHandler {
    /// The directory containing the document Roves was launched with — the effective
    /// content "root" a root-absolute reference gets rebased onto when it doesn't
    /// resolve for real (see `rebase_to_content_root`). Independent of `packed`: this is
    /// set for every launch that has an initial path at all, packed content or not.
    initial_dir: Option<PathBuf>,
    packed: Option<PackedContent>,
}

impl FileProtocolHandler {
    /// `initial_file_path`: the local path the engine was told to open —
    /// i.e. the resolved initial `file:` URL's path. Its directory doubles as
    /// this session's content root (`initial_dir`, `rebase_to_content_root`)
    /// and as where to look for a `.roves-content-source` marker (written by
    /// `roves-content-packer extract`, see CUSTOMIZATIONS.md) — present only
    /// for a packed-content launch; absent for one opened via raw CLI args
    /// for local dev/testing, or a build with `--content-compress=none`, in
    /// which case `packed` alone (not `initial_dir`) behaves exactly like
    /// the stock handler.
    pub fn new(initial_file_path: Option<&Path>) -> Self {
        let initial_dir = initial_file_path.and_then(|p| p.parent()).map(Path::to_path_buf);
        let packed = initial_dir.as_deref().and_then(PackedContent::resolve);
        Self { initial_dir, packed }
    }

    /// See `PackedContent::ensure_available` — a no-op when this launch has no
    /// managed packed content at all (a dev `--url` run, or `--content-compress=none`).
    fn ensure_available(&self, file_path: &Path) {
        if let Some(packed) = &self.packed {
            packed.ensure_available(file_path);
        }
    }

    /// If `file_path` doesn't exist as given, tries it again as though it had been
    /// requested root-relative to this session's own content root (`initial_dir`)
    /// instead of the real OS filesystem root. Per the URL spec, `<script src="/foo.js">`
    /// on a document loaded from a bare `file:` URL (rather than served over http(s)
    /// from an actual domain root) resolves to `file:///foo.js` — `C:\foo.js` on Windows,
    /// `/foo.js` on Linux/macOS — exactly what any browser does for a `file://`
    /// document, and exactly the footgun this exists to route around: virtually every
    /// bundler (Vite, webpack, ...) emits root-absolute asset references by default, and
    /// a game's content root is never actually the OS filesystem root. Only ever
    /// consulted as a fallback, after the literal path already failed to resolve — a
    /// request that already resolves for real (a genuine OS-root file, vanishingly
    /// unlikely to collide with a game's own asset name) is left untouched.
    fn rebase_to_content_root(&self, file_path: &Path) -> Option<PathBuf> {
        let root = self.initial_dir.as_deref()?;
        let relative: PathBuf = file_path
            .components()
            .filter(|c| !matches!(c, Component::Prefix(_) | Component::RootDir))
            .collect();
        if relative.as_os_str().is_empty() {
            return None;
        }
        Some(root.join(relative))
    }
}

impl ProtocolHandler for FileProtocolHandler {
    // Both default to `false` on `ProtocolHandler` (see components/net/protocols/mod.rs) --
    // fine for a protocol handler nobody's meant to treat as the app's own trusted origin,
    // but this one *is* that origin: every Roves game is a `file:` document, and the engine
    // registers this handler in place of Servo's own built-in `file:` support specifically
    // so it's the one deciding this. Leaving both at their default `false` silently broke
    // real games: `is_secure() == false` fails Worker/`blob:` mixed-content checks (a page
    // loaded via `file:` was refusing to spawn its own Worker scripts, "Blocked as mixed
    // content", hanging any code waiting on a message back from one -- confirmed via a real
    // game whose loading screen never advanced past "loading" for exactly this reason), and
    // `is_fetchable() == false` would equally break a game's own `fetch()` calls for its own
    // bundled JSON/data assets.
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

        let response = if let Ok(mut file_path) = url.to_file_path() {
            self.ensure_available(&file_path);
            if !file_path.exists() &&
                let Some(rebased) = self.rebase_to_content_root(&file_path)
            {
                self.ensure_available(&rebased);
                if rebased.exists() {
                    file_path = rebased;
                }
            }

            if file_path.is_dir() {
                return Box::pin(ready(Response::network_error(NetworkError::ResourceLoadError(
                    "Directory listing is not supported".to_owned(),
                ))));
            }

            if let Ok(file) = File::open(file_path.clone()) {
                let file_size = file.metadata().ok().map(|metadata| metadata.len());

                let mut response =
                    Response::new(url, ResourceFetchTiming::new(request.timing_type()));

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
                    response
                        .headers
                        .typed_insert(ContentLength(end_byte - start_byte));
                    if is_range_request {
                        if let Ok(content_range) = ContentRange::bytes(start_byte..end_byte, file_size) {
                            response.headers.typed_insert(content_range);
                        }
                    }
                }

                let mime = mime_guess::from_path(file_path).first_or_octet_stream();
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
                Response::network_error(NetworkError::ResourceLoadError(
                    "Opening file failed".to_owned(),
                ))
            }
        } else {
            Response::network_error(NetworkError::ResourceLoadError(
                "Constructing file path failed".to_owned(),
            ))
        };

        Box::pin(ready(response))
    }
}
