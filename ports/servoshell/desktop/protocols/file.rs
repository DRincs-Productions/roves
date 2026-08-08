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
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;

use headers::{ContentLength, ContentRange, ContentType, HeaderMapExt, Range};
use http::Method;
use roves_content_packer::extract::CONTENT_SOURCE_MARKER;
use roves_content_packer::manifest::Manifest;
use servo::protocol_handler::{
    DoneChannel, FetchContext, FILE_CHUNK_SIZE, NetworkError, ProtocolHandler, Request,
    ResourceFetchTiming, Response, ResponseBody, get_range_request_bounds, partial_content,
    range_not_satisfiable_error,
};
use tokio::sync::mpsc::unbounded_channel;

struct PackedContent {
    /// Where `manifest.json` + `.pack` files live (read-only, shipped as-is).
    content_dir: PathBuf,
    /// Where boot files were already extracted, and where lazy ones land —
    /// the directory the initial `file:` URL's path lives in.
    cache_dir: PathBuf,
    manifest: Manifest,
    /// Serializes on-demand extraction: two near-simultaneous loads for
    /// files in the same not-yet-extracted pack must not both start
    /// unpacking it at once. Extraction is rare/one-time per pack per
    /// session, so coarse (one lock for all packs, not per-pack) is fine.
    extracting: Mutex<()>,
}

pub struct FileProtocolHandler {
    packed: Option<PackedContent>,
}

impl FileProtocolHandler {
    /// `initial_file_path`: the local path the engine was told to open —
    /// i.e. the resolved initial `file:` URL's path. Used only to look for a
    /// `.roves-content-source` marker possibly sitting next to it (written
    /// by `roves-content-packer extract`, see CUSTOMIZATIONS.md). Any other
    /// `file://` document (e.g. one opened via raw CLI args for local
    /// dev/testing, or a build with `--content-compress=none`) simply won't
    /// have that marker, and this handler behaves exactly like the stock one
    /// for it.
    pub fn new(initial_file_path: Option<&Path>) -> Self {
        let packed = initial_file_path.and_then(|p| p.parent()).and_then(Self::resolve_packed_content);
        Self { packed }
    }

    fn resolve_packed_content(cache_dir: &Path) -> Option<PackedContent> {
        let marker = cache_dir.join(CONTENT_SOURCE_MARKER);
        let content_dir = std::fs::read_to_string(&marker).ok()?;
        let content_dir = PathBuf::from(content_dir.trim());
        let manifest = roves_content_packer::extract::load_manifest(&content_dir).ok()?;
        Some(PackedContent {
            content_dir,
            cache_dir: cache_dir.to_path_buf(),
            manifest,
            extracting: Mutex::new(()),
        })
    }

    /// If `file_path` doesn't exist yet and falls under managed packed
    /// content, extracts whichever pack contains it — a synchronous,
    /// blocking call, same as the plain file I/O below it; this only ever
    /// happens the first time a given pack's contents are touched, including
    /// across relaunches (see `ensure_pack_extracted`'s own marker file).
    fn ensure_available(&self, file_path: &Path) {
        let Some(packed) = &self.packed else { return };
        if file_path.exists() {
            return;
        }
        let Ok(rel) = file_path.strip_prefix(&packed.cache_dir) else {
            return;
        };
        let rel_path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        let _guard = packed.extracting.lock().unwrap();
        // Re-check now that we hold the lock: another load() may have just
        // finished extracting the exact pack this path lives in.
        if file_path.exists() {
            return;
        }
        if let Err(e) = roves_content_packer::extract::ensure_file_available(
            &packed.content_dir,
            &packed.cache_dir,
            &packed.manifest,
            &rel_path,
        ) {
            log::warn!("on-demand extraction of {rel_path:?} failed: {e}");
        }
    }
}

impl ProtocolHandler for FileProtocolHandler {
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

        let response = if let Ok(file_path) = url.to_file_path() {
            self.ensure_available(&file_path);

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
