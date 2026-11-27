//! Loopback origin for the game directory: `http://127.0.0.1:<port>/`, bound to the IPv4
//! loopback interface only. tower-http's `ServeDir` does the file serving (rejects `..`
//! traversal, sets MIME types, answers Range requests, evaluates If-Match/If-None-Match);
//! this module owns the listener, the background runtime, the shutdown and the cache policy.
//!
//! Cache policy (conservative; see the probes' README, "Loopback cache policy"):
//! - Every response carries `Cache-Control: no-cache`, so Chromium revalidates before reusing
//!   a stored copy. Without it Chromium derives a heuristic freshness of 10 % of
//!   `Date - Last-Modified` (net/http/http_response_headers.cc, `GetFreshnessLifetimes`) and
//!   reuses old files without asking: with the packaged player's persistent profile and
//!   fixed origin, an updated package could then run stale scripts.
//! - `Last-Modified` is removed and date preconditions are ignored; the only validator is
//!   ServeDir's strong ETag (size + nanosecond mtime) prefixed with a token unique to this
//!   server instance. A tag from another launch never matches, so a package replaced
//!   between launches cannot be confirmed by a `304` even if a file kept its size and mtime
//!   (normalised archive timestamps). Inside one launch the metadata ETag still applies.
//! - `If-Range` (sent by Chromium's partial media cache) is evaluated here because ServeDir
//!   ignores it: a matching strong tag keeps the `206`, anything else gets the full `200`,
//!   so old and new bytes are never stitched together.
//! - No path is ever marked fresh or `immutable`; there is no opt-in for it (README).
//!
//! Packed game content (`content.rs`, a folder with `roves-content.json`): the same policy,
//! with responses built here from the archive entries instead of ServeDir, mirroring its
//! behavior (MIME from the path, one-range `206`/`416`, `307` from `dir` to `dir/`,
//! `index.html` for `/` and `dir/`). The strong ETag is the entry's CRC-32 and size, with
//! the same instance token.
//!
//! Mods and DLC (`mods.rs`): an active add-on's `overrides/<path>` is served for the game path
//! `<path>`, and `/__mods/<id>/<path>` serves the add-on's own files; both go through the same
//! cache policy, from the add-on's folder (ServeDir) or `.rovesmod` archive. Paths that no add-on
//! has fall through to the game, so a game without add-ons is served exactly as before.

use std::{
    convert::Infallible,
    io,
    net::{Ipv4Addr, TcpListener},
    path::Path,
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
    response::Response,
};
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::services::ServeDir;

/// Packed game content (memory-mapped ZIP archives), shared by the three players.
#[path = "content.rs"]
pub mod content;

/// Mods and DLC (folders and `.rovesmod` archives), shared by the three players.
#[path = "mods.rs"]
pub mod mods;

pub struct Server {
    pub origin: String,
    /// Boot report of packed content (`None` for a loose game folder).
    pub content: Option<String>,
    packed: Option<content::Content>,
    shutdown: oneshot::Sender<()>,
    thread: JoinHandle<io::Result<()>>,
}

/// Binds synchronously (so the origin is known before CEF starts) and serves on a dedicated
/// thread. Port 0 picks a free port, giving every run its own origin.
#[allow(dead_code)]
pub fn start(root: &Path, port: u16) -> io::Result<Server> {
    start_with(root, port, |_| Router::new())
}

/// [`start`] plus `routes`, built once the origin is known and matched before the game files
/// (the players' host API, `host_api.rs`, under `/__roves/`).
#[allow(dead_code)] // the players call `start_with_mods`; kept for the tests
pub fn start_with(
    root: &Path,
    port: u16,
    routes: impl FnOnce(&str) -> Router,
) -> io::Result<Server> {
    start_with_mods(root, port, None, routes)
}

/// [`start_with`] plus the add-ons of [`mods::Mods::discover`].
pub fn start_with_mods(
    root: &Path,
    port: u16,
    mods: Option<mods::Mods>,
    routes: impl FnOnce(&str) -> Router,
) -> io::Result<Server> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let packed = content::Content::open(root)?;
    let summary = packed.as_ref().map(|c| c.summary.clone());
    let files = GameFiles {
        source: match packed.clone() {
            Some(content) => Source::Packed(content),
            None => Source::Dir(ServeDir::new(root)),
        },
        token: instance_token(),
    };
    let addons = mods.map(|mods| {
        let sources = mods
            .sources()
            .into_iter()
            .map(|source| GameFiles {
                source: match source {
                    mods::Files::Dir(dir) => Source::Dir(ServeDir::new(dir)),
                    mods::Files::Packed(content) => Source::Packed(content),
                },
                token: files.token.clone(),
            })
            .collect::<Vec<_>>();
        (mods, std::sync::Arc::new(sources))
    });
    let app = routes(&origin).fallback(move |req: Request| {
        let files = files.clone();
        let addons = addons.clone();
        async move {
            let routed = addons.and_then(|(mods, sources)| {
                addon_route(&mods, req.uri()).map(|(index, uri)| (sources, index, uri))
            });
            match routed {
                Some((sources, index, uri)) => {
                    let (mut parts, body) = req.into_parts();
                    parts.uri = uri;
                    sources[index]
                        .clone()
                        .serve(Request::from_parts(parts, body))
                        .await
                },
                None if req.uri().path().starts_with(mods::URL_PREFIX) => {
                    Ok(status(StatusCode::NOT_FOUND))
                },
                None => files.serve(req).await,
            }
        }
    });
    let (shutdown, stopped) = oneshot::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("roves-cef-loopback".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()?;
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
            })
        })?;
    Ok(Server {
        origin,
        content: summary,
        packed,
        shutdown,
        thread,
    })
}

impl Server {
    /// Packed content counters for the player's log (requests, inflation, prediction).
    pub fn content_report(&self) -> Option<String> {
        self.packed.as_ref().map(|c| c.stats().report())
    }

    /// Stops accepting, lets in-flight responses finish and joins the thread.
    pub fn stop(self) -> io::Result<()> {
        let _ = self.shutdown.send(());
        self.thread
            .join()
            .map_err(|_| io::Error::other("loopback server thread panicked"))?
    }
}

/// Unique per server start: wall-clock nanoseconds and process id, hex (ETag-safe).
fn instance_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("r{nanos:x}-{:x}", std::process::id())
}

/// Never matches a ServeDir tag: stands in for a validator issued by another instance.
const FOREIGN_TAG: &str = "\"roves-other-instance\"";

#[derive(Clone)]
enum Source {
    Dir(ServeDir),
    Packed(content::Content),
}

#[derive(Clone)]
struct GameFiles {
    source: Source,
    token: String,
}

/// Add-on request -> (source index, URI inside that source): `/__mods/<id>/<path>`, or a game
/// path that an active add-on overrides. `None`: serve the game (or 404 under `/__mods/`).
fn addon_route(mods: &mods::Mods, uri: &axum::http::Uri) -> Option<(usize, axum::http::Uri)> {
    let (index, inner) = match uri.path().strip_prefix(mods::URL_PREFIX) {
        Some(rest) => {
            let decoded = String::from_utf8(percent_decode(rest)?).ok()?;
            mods.addon_file(&decoded)?
        },
        None => mods.overlay(&entry_path(uri.path())?)?,
    };
    let mut target = format!("/{}", mods::encode_path(&inner));
    if let Some(query) = uri.query() {
        target.push('?');
        target.push_str(query);
    }
    Some((index, target.parse().ok()?))
}

/// Request path -> game-relative entry path (`/` and `dir/` -> `index.html`), or `None` for
/// paths that can never name an entry (`..`, `.`, empty segments, backslashes, bad escapes).
fn entry_path(uri_path: &str) -> Option<String> {
    let decoded: Vec<u8> = percent_decode(uri_path.trim_start_matches('/'))?;
    let mut path = String::from_utf8(decoded).ok()?;
    if path.is_empty() || path.ends_with('/') {
        path.push_str("index.html");
    }
    let valid = !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    valid.then_some(path)
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Does an If-Match/If-None-Match list (already mapped by `map_client_tags`) name `tag`?
/// `strong`: weak tags never match (If-Match); otherwise `W/` is ignored (If-None-Match).
fn tag_listed(list: &HeaderValue, tag: &str, strong: bool) -> bool {
    let Ok(list) = list.to_str() else {
        return false;
    };
    list.trim() == "*"
        || list
            .split(',')
            .map(str::trim)
            .any(|t| match t.strip_prefix("W/") {
                Some(weak) => !strong && weak == tag,
                None => t == tag,
            })
}

fn status(code: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = code;
    response
}

/// ServeDir's answers for one packed entry (see the module docs).
fn packed_response(content: &content::Content, parts: &Parts) -> Response {
    if parts.method != Method::GET && parts.method != Method::HEAD {
        let mut response = status(StatusCode::METHOD_NOT_ALLOWED);
        response
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    let Some(path) = entry_path(parts.uri.path()) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(hit) = content.get(&path) else {
        if !parts.uri.path().ends_with('/') && content.contains(&format!("{path}/index.html")) {
            let mut response = status(StatusCode::TEMPORARY_REDIRECT);
            if let Ok(location) = HeaderValue::from_str(&format!("{}/", parts.uri.path())) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            return response;
        }
        return status(StatusCode::NOT_FOUND);
    };
    let size = hit.bytes.len() as u64;
    let tag = format!("\"{:08x}-{size:x}\"", hit.crc);
    let headers: &HeaderMap = &parts.headers;
    if let Some(list) = headers.get(header::IF_MATCH)
        && !tag_listed(list, &tag, true)
    {
        return status(StatusCode::PRECONDITION_FAILED);
    }
    let etag =
        HeaderValue::from_str(&tag).unwrap_or_else(|_| HeaderValue::from_static(FOREIGN_TAG));
    if let Some(list) = headers.get(header::IF_NONE_MATCH)
        && tag_listed(list, &tag, false)
    {
        let mut response = status(StatusCode::NOT_MODIFIED);
        response.headers_mut().insert(header::ETAG, etag);
        return response;
    }
    let mime = mime_guess::from_path(&path)
        .first_raw()
        .unwrap_or("application/octet-stream");
    let mut response = Response::new(Body::empty());
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    out.insert(header::ETAG, etag);
    let mut body = hit.bytes;
    if let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        let single = http_range_header::parse_range_header(range)
            .ok()
            .filter(|parsed| parsed.ranges.len() == 1)
            .and_then(|parsed| parsed.validate(size).ok())
            .and_then(|mut ranges| ranges.pop());
        let Some(range) = single else {
            let out = response.headers_mut();
            out.remove(header::CONTENT_TYPE);
            out.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{size}")).unwrap(),
            );
            *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            return response;
        };
        let (first, last) = (*range.start(), *range.end());
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {first}-{last}/{size}")).unwrap(),
        );
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        body = body.slice(first as usize..=last as usize);
    }
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
    if parts.method == Method::GET {
        *response.body_mut() = Body::from(body);
    }
    response
}

impl GameFiles {
    async fn serve(self, req: Request) -> Result<Response, Infallible> {
        let (mut parts, _body) = req.into_parts();
        let headers = &mut parts.headers;
        headers.remove(header::IF_MODIFIED_SINCE);
        headers.remove(header::IF_UNMODIFIED_SINCE);
        for name in [header::IF_NONE_MATCH, header::IF_MATCH] {
            if let Some(value) = headers.get(&name).and_then(|v| v.to_str().ok()) {
                let mapped = self.map_client_tags(value);
                headers.insert(name, mapped);
            }
        }
        // If-Range: keep the range only for a strong tag of this instance, evaluated as an
        // If-Match by ServeDir; a 412 then means "changed" and the full file is sent.
        let mut guarded_range = false;
        if let Some(value) = headers.remove(header::IF_RANGE) {
            let inner = value
                .to_str()
                .ok()
                .and_then(|v| self.strip_token(v.trim()))
                .filter(|_| headers.contains_key(header::RANGE));
            match inner {
                Some(tag) if !headers.contains_key(header::IF_MATCH) => {
                    headers.insert(header::IF_MATCH, tag);
                    guarded_range = true;
                },
                _ => {
                    headers.remove(header::RANGE);
                },
            }
        }
        let first = self
            .call(Request::from_parts(parts.clone(), Body::empty()))
            .await;
        let response = if guarded_range && first.status() == StatusCode::PRECONDITION_FAILED {
            parts.headers.remove(header::IF_MATCH);
            parts.headers.remove(header::RANGE);
            self.call(Request::from_parts(parts, Body::empty())).await
        } else {
            first
        };
        Ok(self.finish(response))
    }

    async fn call(&self, req: Request) -> Response {
        match &self.source {
            Source::Dir(dir) => match dir.clone().oneshot(req).await {
                Ok(response) => response.map(Body::new),
                Err(never) => match never {},
            },
            Source::Packed(content) => packed_response(content, &req.into_parts().0),
        }
    }

    fn finish(&self, mut response: Response) -> Response {
        let headers = response.headers_mut();
        headers.remove(header::LAST_MODIFIED);
        if let Some(tag) = headers.get(header::ETAG).and_then(|v| v.to_str().ok()) {
            let ours = self.add_token(tag);
            headers.insert(header::ETAG, ours);
        }
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        response
    }

    /// `"<inner>"` -> `"<token>:<inner>"`; ServeDir's tags are strong and quoted.
    fn add_token(&self, tag: &str) -> HeaderValue {
        let inner = tag.trim_matches('"');
        HeaderValue::from_str(&format!("\"{}:{inner}\"", self.token))
            .unwrap_or_else(|_| HeaderValue::from_static(FOREIGN_TAG))
    }

    /// Strong tag of this instance -> ServeDir's tag; `None` for weak or foreign tags.
    fn strip_token(&self, tag: &str) -> Option<HeaderValue> {
        let rest = tag.strip_prefix('"')?.strip_prefix(self.token.as_str())?;
        let inner = rest.strip_prefix(':')?;
        HeaderValue::from_str(&format!("\"{inner}")).ok()
    }

    /// Maps an If-None-Match/If-Match list: this instance's tags lose the token (weak marker
    /// kept), any other tag becomes one that never matches, `*` is kept.
    fn map_client_tags(&self, list: &str) -> HeaderValue {
        if list.trim() == "*" {
            return HeaderValue::from_static("*");
        }
        let mapped: Vec<String> = list
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(|tag| {
                let (weak, strong) = match tag.strip_prefix("W/") {
                    Some(rest) => ("W/", rest),
                    None => ("", tag),
                };
                match self.strip_token(strong) {
                    Some(inner) => format!("{weak}{}", inner.to_str().unwrap_or(FOREIGN_TAG)),
                    None => FOREIGN_TAG.to_owned(),
                }
            })
            .collect();
        HeaderValue::from_str(&mapped.join(", "))
            .unwrap_or_else(|_| HeaderValue::from_static(FOREIGN_TAG))
    }
}

#[cfg(test)]
mod tests {
    //! Raw HTTP/1.1 over loopback against the real server and the framework-free fixture
    //! (`support/cef-windows-player/fixtures/static-compat`); no client crate.
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpStream,
        path::PathBuf,
    };

    fn fixture() -> PathBuf {
        // Same relative path from both crates that compile this file.
        let default = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cef-windows-player/fixtures/static-compat"
        );
        PathBuf::from(option_env!("ROVES_STATIC_FIXTURE").unwrap_or(default))
    }

    /// The fixture loose, packed stored and packed compressed (`content.rs`; `range.txt` is
    /// an asset, the rest code): every test below runs on all three.
    fn roots() -> (Vec<PathBuf>, Vec<tempfile::TempDir>) {
        let base = fixture();
        let code = [
            "index.html",
            "main.js",
            "modules/preloaded.js",
            "style.css",
            "tiny.wasm",
        ];
        let files: Vec<(String, Vec<u8>)> = code
            .iter()
            .chain(&["range.txt"])
            .map(|rel| (rel.to_string(), std::fs::read(base.join(rel)).unwrap()))
            .collect();
        let mut roots = vec![base];
        let mut dirs = Vec::new();
        for level in [0, 6] {
            let dir = tempfile::tempdir().unwrap();
            content::tests::pack(dir.path(), &files, &code, level);
            roots.push(dir.path().to_path_buf());
            dirs.push(dir);
        }
        (roots, dirs)
    }

    struct Reply {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl Reply {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    fn get(origin: &str, path: &str, extra: &[(&str, &str)]) -> Reply {
        let addr = origin.trim_start_matches("http://");
        let mut stream = TcpStream::connect(addr).unwrap();
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
        for (n, v) in extra {
            req.push_str(&format!("{n}: {v}\r\n"));
        }
        req.push_str("\r\n");
        stream.write_all(req.as_bytes()).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8(raw[..split].to_vec()).unwrap();
        let mut lines = head.split("\r\n");
        let status = lines
            .next()
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let headers = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
            .collect();
        Reply {
            status,
            headers,
            body: raw[split + 4..].to_vec(),
        }
    }

    #[test]
    fn mime_types_and_cache_headers() {
        let (roots, _dirs) = roots();
        for root in &roots {
            let server = start(root, 0).unwrap();
            for (path, mime) in [
                ("/", "text/html"),
                ("/main.js", "text/javascript"),
                ("/modules/preloaded.js", "text/javascript"),
                ("/style.css", "text/css"),
                ("/tiny.wasm", "application/wasm"),
            ] {
                let r = get(&server.origin, path, &[]);
                assert_eq!(r.status, 200, "{path}");
                assert!(
                    r.header("content-type").unwrap().starts_with(mime),
                    "{path}"
                );
                assert_eq!(r.header("cache-control"), Some("no-cache"), "{path}");
                assert!(r.header("last-modified").is_none(), "{path}");
                assert!(r.header("etag").unwrap().starts_with("\"r"), "{path}");
            }
            let missing = get(&server.origin, "/missing.js", &[]);
            assert_eq!(missing.status, 404);
            assert_eq!(missing.header("cache-control"), Some("no-cache"));
            server.stop().unwrap();
        }
    }

    #[test]
    fn revalidation_is_scoped_to_the_instance() {
        let (roots, _dirs) = roots();
        for root in &roots {
            let first = start(root, 0).unwrap();
            let tag = get(&first.origin, "/main.js", &[])
                .header("etag")
                .unwrap()
                .to_owned();
            let same = get(&first.origin, "/main.js", &[("If-None-Match", &tag)]);
            assert_eq!(same.status, 304);
            assert_eq!(same.header("etag"), Some(tag.as_str()));
            assert_eq!(same.header("cache-control"), Some("no-cache"));
            let weak = get(
                &first.origin,
                "/main.js",
                &[("If-None-Match", &format!("W/{tag}"))],
            );
            assert_eq!(weak.status, 304);
            // Date validators are ignored: only the instance tag can produce a 304.
            let dated = get(
                &first.origin,
                "/main.js",
                &[("If-Modified-Since", "Fri, 01 Jan 2100 00:00:00 GMT")],
            );
            assert_eq!(dated.status, 200);
            first.stop().unwrap();

            // A later launch (same files, same size and mtime) must not confirm the old tag.
            let second = start(root, 0).unwrap();
            let again = get(&second.origin, "/main.js", &[("If-None-Match", &tag)]);
            assert_eq!(again.status, 200);
            assert_ne!(again.header("etag"), Some(tag.as_str()));
            assert!(!again.body.is_empty());
            second.stop().unwrap();
        }
    }

    #[test]
    fn ranges_and_if_range() {
        let (roots, _dirs) = roots();
        for root in &roots {
            let server = start(root, 0).unwrap();
            let full = get(&server.origin, "/range.txt", &[]);
            assert_eq!(full.header("accept-ranges"), Some("bytes"));
            let total = full.body.len();
            let tag = full.header("etag").unwrap().to_owned();

            let part = get(&server.origin, "/range.txt", &[("Range", "bytes=4-9")]);
            assert_eq!(part.status, 206);
            assert_eq!(part.body, b"456789");
            assert_eq!(
                part.header("content-range"),
                Some(format!("bytes 4-9/{total}").as_str())
            );
            assert_eq!(part.header("cache-control"), Some("no-cache"));

            let kept = get(
                &server.origin,
                "/range.txt",
                &[("Range", "bytes=4-9"), ("If-Range", &tag)],
            );
            assert_eq!(kept.status, 206);
            assert_eq!(kept.body, b"456789");

            for stale in [
                "\"r0-0:1.00000000-10\"",
                "Fri, 01 Jan 2100 00:00:00 GMT",
                &format!("W/{tag}"),
            ] {
                let whole = get(
                    &server.origin,
                    "/range.txt",
                    &[("Range", "bytes=4-9"), ("If-Range", stale)],
                );
                assert_eq!(whole.status, 200, "{stale}");
                assert_eq!(whole.body.len(), total, "{stale}");
            }
            server.stop().unwrap();
        }
    }

    #[test]
    fn foreign_if_match_fails() {
        let (roots, _dirs) = roots();
        for root in &roots {
            let server = start(root, 0).unwrap();
            let r = get(
                &server.origin,
                "/main.js",
                &[("If-Match", "\"r1-1:0.0-0\"")],
            );
            assert_eq!(r.status, 412);
            server.stop().unwrap();
        }
    }

    #[test]
    fn packed_paths_and_redirects() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            ("index.html".to_owned(), b"root".to_vec()),
            ("docs/index.html".to_owned(), b"docs".to_vec()),
            ("img/a b.txt".to_owned(), b"space".to_vec()),
        ];
        content::tests::pack(dir.path(), &files, &["index.html", "docs/index.html"], 0);
        let server = start(dir.path(), 0).unwrap();
        assert!(server.content.as_deref().unwrap().contains("entries=3"));
        assert_eq!(get(&server.origin, "/", &[]).body, b"root");
        assert_eq!(get(&server.origin, "/docs/", &[]).body, b"docs");
        let redirect = get(&server.origin, "/docs", &[]);
        assert_eq!(redirect.status, 307);
        assert_eq!(redirect.header("location"), Some("/docs/"));
        assert_eq!(redirect.header("cache-control"), Some("no-cache"));
        assert_eq!(get(&server.origin, "/img/a%20b.txt", &[]).body, b"space");
        for bad in [
            "/../index.html",
            "/%2e%2e/index.html",
            "/img//a%20b.txt",
            "/%zz",
            "/img",
        ] {
            assert_eq!(get(&server.origin, bad, &[]).status, 404, "{bad}");
        }
        server.stop().unwrap();
    }
}
