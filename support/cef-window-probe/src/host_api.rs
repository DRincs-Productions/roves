//! Host API of the CEF players: same-origin routes `/__roves/<module>/<cmd>` on the game's
//! loopback server (`server.rs`), called by `@drincs/roves-api` when the page sees
//! `window.__ROVES__.transport === "http"`. It replaces the Servo shell's `roves:` and
//! `saves:` custom schemes: welding owns `CefApp` and the render-process handler, so the
//! players register no custom scheme. Platform-neutral; each player compiles this file
//! through `#[path]` and supplies its own [`HostConfig`].
//!
//! Request guard ([`check_request`]), modelled on Chromium's DevTools HTTP handler
//! (`content/browser/devtools/devtools_http_handler.cc` at f2700724f59f:
//! `RequestIsSafeToServe` refuses a `Host` that is not an IP literal or localhost, the
//! DNS-rebinding defence; `OnWebSocketRequest` refuses an `Origin` other than the server's
//! own). Stricter here, because only the game document is a legitimate caller:
//! - `Host` must be exactly `127.0.0.1:<port>` (a rebound DNS name never matches);
//! - the custom header `X-Roves-Api: 1` is required, so a cross-origin page needs a CORS
//!   preflight, and preflights (`OPTIONS`) are always refused without any CORS header;
//! - an `Origin` header, when present, must be the server's own origin, and
//!   `Sec-Fetch-Site`, when present, must be `same-origin`.
//!
//! No response carries CORS headers; every response is `Cache-Control: no-store`.
//! Commands that change state require `POST`. Modules are a name -> [`Module`] map: `core`,
//! `saves` and `steam` (Steamworks only through a player-supplied [`SteamApi`]; this file never
//! links Steamworks, so every player builds it).

use std::{
    collections::HashMap,
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::UNIX_EPOCH,
};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::Response,
    routing::any,
};
use base64::Engine as _;
use serde_json::{Value, json};

/// Path prefix of every host API route; never served from the game directory.
pub const PREFIX: &str = "/__roves/";
/// Required request header (value `1`).
pub const API_HEADER: &str = "x-roves-api";
/// Largest accepted request body (a save may hold a screenshot).
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Longest save key, in UTF-8 bytes (the Servo `saves:` limit).
pub const MAX_KEY_BYTES: usize = 200;
/// File suffix of a save; `list` reports only these.
const SAVE_SUFFIX: &str = ".save";
/// Suffix of in-flight temporary files (never a save).
const TEMP_SUFFIX: &str = ".tmp";

/// What a player hands to the host API.
pub struct HostConfig {
    /// `<persistent profile root>/saves`; `None` without a persistent profile (isolated or
    /// development runs), where `saves` answers "unavailable".
    pub saves_dir: Option<PathBuf>,
    /// `system_info`'s `engine_version`.
    pub engine_version: String,
    /// Asks the player's main loop to close the window through its ordinary close path.
    /// Called from the server thread with the command name; must not block.
    pub request_exit: Arc<dyn Fn(&'static str) + Send + Sync>,
    /// The player's Steamworks client (Windows player `steam` feature); `None`, or a handle
    /// already shut down, makes every `steam` command answer the legacy "unavailable" result.
    pub steam: Option<Arc<SteamHandle>>,
}

/// Answer of one command.
#[derive(Debug, PartialEq)]
pub enum Reply {
    Json(Value),
    Error(StatusCode, &'static str),
}

/// One call, after the request guard.
pub struct Call<'a> {
    pub method: &'a Method,
    /// Decoded query pairs.
    pub query: &'a [(String, String)],
    /// `POST` body (empty for other methods).
    pub body: &'a [u8],
}

impl Call<'_> {
    fn arg(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn post(&self) -> bool {
        self.method == Method::POST
    }
}

/// A host API module (`core`, `saves`, `steam`). Runs on a blocking thread.
pub trait Module: Send + Sync {
    fn call(&self, cmd: &str, call: &Call<'_>) -> Reply;
}

/// The modules of one player.
#[derive(Clone)]
pub struct HostApi {
    modules: Arc<HashMap<&'static str, Arc<dyn Module>>>,
}

impl HostApi {
    /// `core`, `saves` and `steam`. Also returns the saves location status for the player's log:
    /// the directory, or why saves are unavailable.
    pub fn new(config: HostConfig) -> (Self, Result<PathBuf, String>) {
        let saves = Saves::open(config.saves_dir);
        let status = saves.status();
        let mut modules: HashMap<&'static str, Arc<dyn Module>> = HashMap::new();
        modules.insert(
            "core",
            Arc::new(Core {
                engine_version: config.engine_version,
                request_exit: config.request_exit,
            }),
        );
        modules.insert("saves", Arc::new(saves));
        modules.insert(
            "steam",
            Arc::new(Steam {
                handle: config.steam,
            }),
        );
        (
            Self {
                modules: Arc::new(modules),
            },
            status,
        )
    }

    /// Routes under [`PREFIX`] for the server at `origin` (`http://127.0.0.1:<port>`).
    pub fn router(self, origin: &str) -> Router {
        let origin = Arc::<str>::from(origin);
        Router::new().route(
            "/__roves/{*path}",
            any(move |req: Request| {
                let api = self.clone();
                let origin = origin.clone();
                async move { api.handle(&origin, req).await }
            }),
        )
    }

    async fn handle(&self, origin: &str, req: Request) -> Response {
        if let Err(reason) = check_request(req.headers(), req.method(), origin) {
            return refused(StatusCode::FORBIDDEN, reason);
        }
        let Some((module, cmd)) = req
            .uri()
            .path()
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.split_once('/'))
            .filter(|(m, c)| !m.is_empty() && !c.is_empty() && !c.contains('/'))
            .map(|(m, c)| (m.to_owned(), c.to_owned()))
        else {
            return refused(StatusCode::NOT_FOUND, "unknown command");
        };
        let Some(handler) = self.modules.get(module.as_str()).cloned() else {
            return refused(StatusCode::NOT_FOUND, "unknown module");
        };
        let method = req.method().clone();
        if method != Method::GET && method != Method::POST {
            return refused(StatusCode::METHOD_NOT_ALLOWED, "GET or POST only");
        }
        let query: Vec<(String, String)> = req
            .uri()
            .query()
            .map(|q| form_urlencoded::parse(q.as_bytes()).into_owned().collect())
            .unwrap_or_default();
        let body = if method == Method::POST {
            match axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await {
                Ok(bytes) => bytes,
                Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body too large"),
            }
        } else {
            Default::default()
        };
        // File I/O and fsync off the server thread, so game files keep being served.
        let reply = tokio::task::spawn_blocking(move || {
            handler.call(
                &cmd,
                &Call {
                    method: &method,
                    query: &query,
                    body: &body,
                },
            )
        })
        .await
        .unwrap_or(Reply::Error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "handler failed",
        ));
        match reply {
            Reply::Json(value) => {
                let mut response = Response::new(Body::from(value.to_string()));
                let headers = response.headers_mut();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                no_store(headers);
                response
            },
            Reply::Error(status, reason) => refused(status, reason),
        }
    }
}

/// The request guard (module docs). `origin` is the server's `http://127.0.0.1:<port>`.
pub fn check_request(
    headers: &HeaderMap,
    method: &Method,
    origin: &str,
) -> Result<(), &'static str> {
    if method == Method::OPTIONS {
        return Err("preflight refused");
    }
    let host = origin.strip_prefix("http://").unwrap_or_default();
    let header = |name: &str| headers.get(name).map(HeaderValue::as_bytes);
    if host.is_empty() || headers.get_all(header::HOST).iter().count() != 1 {
        return Err("host refused");
    }
    if header(header::HOST.as_str()) != Some(host.as_bytes()) {
        return Err("host refused");
    }
    if header(API_HEADER) != Some(b"1".as_slice()) {
        return Err("missing X-Roves-Api header");
    }
    if headers.contains_key(header::ORIGIN)
        && header(header::ORIGIN.as_str()) != Some(origin.as_bytes())
    {
        return Err("origin refused");
    }
    if let Some(site) = header("sec-fetch-site")
        && site != b"same-origin"
    {
        return Err("cross-site request refused");
    }
    Ok(())
}

fn no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

fn refused(status: StatusCode, reason: &'static str) -> Response {
    let mut response = Response::new(Body::from(reason));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    no_store(headers);
    response
}

/// `core`: the Servo shell's `roves:` commands that make sense in the CEF players.
struct Core {
    engine_version: String,
    request_exit: Arc<dyn Fn(&'static str) + Send + Sync>,
}

impl Module for Core {
    fn call(&self, cmd: &str, call: &Call<'_>) -> Reply {
        match cmd {
            "is_available" => Reply::Json(Value::Bool(true)),
            "system_info" => Reply::Json(system_info(&self.engine_version)),
            // `clear_content_cache`: the CEF players serve the game straight from the package
            // (no extraction cache exists) and every loopback response is revalidated
            // (`server.rs` cache policy), so there is nothing stale to clear; the documented
            // contract "the game closes" is kept.
            "exit" | "close_window" | "clear_content_cache" if call.post() => {
                let cmd = match cmd {
                    "clear_content_cache" => "clear_content_cache",
                    _ => "exit",
                };
                (self.request_exit)(cmd);
                Reply::Json(Value::Bool(true))
            },
            "exit" | "close_window" | "clear_content_cache" => {
                Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only")
            },
            _ => Reply::Error(StatusCode::NOT_FOUND, "unknown command"),
        }
    }
}

/// The Servo shell's `system_info` shape (`os_type`, `os_version`, `bitness`, `architecture`,
/// `engine_version`). `os_type` is `windows`/`macos`, or the Linux distribution id from
/// `/etc/os-release` (the source the old `sysinfo::System::distribution_id` read);
/// `os_version` is the distribution's `VERSION_ID` on Linux, `ProductVersion` from
/// `SystemVersion.plist` on macOS, and `null` on Windows (no OS-version crate in the graph).
pub fn system_info(engine_version: &str) -> Value {
    let (os_type, os_version) = os_identity();
    json!({
        "os_type": os_type,
        "os_version": os_version,
        "bitness": if cfg!(target_pointer_width = "64") { "64-bit" } else { "32-bit" },
        "architecture": std::env::consts::ARCH,
        "engine_version": engine_version,
    })
}

#[cfg(target_os = "linux")]
fn os_identity() -> (String, Option<String>) {
    let text = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    };
    (
        field("ID").unwrap_or_else(|| "linux".into()),
        field("VERSION_ID"),
    )
}

#[cfg(target_os = "macos")]
fn os_identity() -> (String, Option<String>) {
    let plist =
        fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").unwrap_or_default();
    let version = plist
        .split_once("<key>ProductVersion</key>")
        .and_then(|(_, rest)| rest.split_once("<string>"))
        .and_then(|(_, rest)| rest.split_once("</string>"))
        .map(|(v, _)| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    ("macos".into(), version)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_identity() -> (String, Option<String>) {
    (std::env::consts::OS.into(), None)
}

/// Validates a save key and returns its file name. Kept from the Servo `saves:` protocol:
/// non-empty, at most [`MAX_KEY_BYTES`], not `.`/`..`, no `/`, `\`, `:` or NUL. Added for
/// portable file names: no other control character, none of `<>"|?*`, no trailing dot or
/// space, and no Windows device name (`CON`, `NUL`, `COM1`, ...) before the first dot.
pub fn save_file_name(key: &str) -> Option<String> {
    const DEVICES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    if key.is_empty()
        || key.len() > MAX_KEY_BYTES
        || key == "."
        || key == ".."
        || key.ends_with(['.', ' '])
        || key.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return None;
    }
    let stem = key.split('.').next().unwrap_or_default().trim_end();
    let upper = stem.to_ascii_uppercase();
    let numbered = upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.as_bytes()[3].is_ascii_digit();
    if DEVICES.contains(&upper.as_str()) || numbered {
        return None;
    }
    Some(format!("{key}{SAVE_SUFFIX}"))
}

/// `saves`: one file per key under the profile's `saves/` directory.
pub struct Saves {
    dir: Result<PathBuf, String>,
}

impl Saves {
    /// Creates the directory and removes temporary files left by an interrupted write.
    pub fn open(dir: Option<PathBuf>) -> Self {
        let dir = match dir {
            None => Err("no persistent profile".to_owned()),
            Some(dir) => match fs::create_dir_all(&dir) {
                Ok(()) => {
                    if let Ok(entries) = fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name();
                            let name = name.to_string_lossy();
                            if name.starts_with('.') && name.ends_with(TEMP_SUFFIX) {
                                let _ = fs::remove_file(entry.path());
                            }
                        }
                    }
                    Ok(dir)
                },
                Err(error) => Err(format!("{}: {error}", dir.display())),
            },
        };
        Self { dir }
    }

    pub fn status(&self) -> Result<PathBuf, String> {
        self.dir.clone()
    }

    fn keys(dir: &Path) -> Vec<(String, fs::Metadata)> {
        let mut keys: Vec<(String, fs::Metadata)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let key = name.strip_suffix(SAVE_SUFFIX)?;
                save_file_name(key)?;
                let meta = entry.metadata().ok().filter(fs::Metadata::is_file)?;
                Some((key.to_owned(), meta))
            })
            .collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));
        keys
    }
}

/// Writes `data` to `dir/name` through `tempfile`'s `NamedTempFile` in the same directory
/// (unique name, removed on any failure), flushed to disk, then persisted (renamed) over the
/// target, an atomic replace on NTFS, APFS and ext4: a crash leaves the old save or the new
/// one, never a truncated file.
pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> io::Result<()> {
    let mut file = tempfile::Builder::new()
        .prefix(".roves-save-")
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(dir.join(name)).map(drop).map_err(|e| e.error)
}

impl Module for Saves {
    fn call(&self, cmd: &str, call: &Call<'_>) -> Reply {
        let mutating = matches!(cmd, "write" | "delete" | "clear");
        let known = mutating || matches!(cmd, "is_available" | "list" | "read" | "most_recent");
        if !known {
            return Reply::Error(StatusCode::NOT_FOUND, "unknown command");
        }
        if mutating && !call.post() {
            return Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only");
        }
        let dir = match &self.dir {
            Ok(dir) => dir,
            // The Servo shell's "unavailable" answers.
            Err(_) => {
                return Reply::Json(match cmd {
                    "is_available" | "write" | "delete" | "clear" => Value::Bool(false),
                    "list" => Value::Array(Vec::new()),
                    _ => Value::Null,
                });
            },
        };
        let key = || {
            call.arg("key")
                .and_then(save_file_name)
                .ok_or(Reply::Error(StatusCode::BAD_REQUEST, "invalid key"))
        };
        let value = match cmd {
            "is_available" => Value::Bool(true),
            "list" => Value::Array(
                Saves::keys(dir)
                    .into_iter()
                    .map(|(key, _)| Value::String(key))
                    .collect(),
            ),
            "read" => match key() {
                Ok(name) => match fs::read(dir.join(name)) {
                    Ok(bytes) => {
                        Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
                    },
                    Err(_) => Value::Null,
                },
                Err(reply) => return reply,
            },
            "most_recent" => Saves::keys(dir)
                .into_iter()
                .filter_map(|(key, meta)| {
                    let ms = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
                    Some((key, i64::try_from(ms.as_millis()).unwrap_or(i64::MAX)))
                })
                .max_by_key(|(_, ms)| *ms)
                .map_or(
                    Value::Null,
                    |(key, ms)| json!({ "key": key, "modifiedMs": ms }),
                ),
            "write" => match key() {
                Ok(name) => Value::Bool(write_atomic(dir, &name, call.body).is_ok()),
                Err(reply) => return reply,
            },
            "delete" => match key() {
                Ok(name) => Value::Bool(match fs::remove_file(dir.join(name)) {
                    Ok(()) => true,
                    Err(e) => e.kind() == io::ErrorKind::NotFound,
                }),
                Err(reply) => return reply,
            },
            // "clear"
            _ => Value::Bool(
                Saves::keys(dir)
                    .into_iter()
                    .map(|(key, _)| fs::remove_file(dir.join(format!("{key}{SAVE_SUFFIX}"))))
                    .fold(true, |ok, r| ok & r.is_ok()),
            ),
        };
        Reply::Json(value)
    }
}

/// The Steamworks calls behind the `steam` module, implemented by the player over
/// `steamworks::Client` (the legacy Servo `steam:` protocol's calls, one method each).
pub trait SteamApi: Send {
    fn player_name(&self) -> String;
    fn app_id(&self) -> u32;
    /// `SetAchievement` then `StoreStats`.
    fn unlock_achievement(&self, id: &str) -> bool;
    fn is_achievement_unlocked(&self, id: &str) -> bool;
    /// `ClearAchievement` then `StoreStats`.
    fn clear_achievement(&self, id: &str) -> bool;
    fn set_stat_int(&self, name: &str, value: i32) -> bool;
    fn get_stat_int(&self, name: &str) -> i32;
    fn set_stat_float(&self, name: &str, value: f32) -> bool;
    fn get_stat_float(&self, name: &str) -> f32;
    fn store_stats(&self) -> bool;
    fn is_dlc_installed(&self, app_id: u32) -> bool;
    fn open_overlay(&self, dialog: &str);
    /// `None`: the running app's own store page.
    fn open_store(&self, app_id: Option<u32>);
    /// Dispatches pending Steam callbacks; called from the Windows player's main loop.
    #[allow(dead_code)]
    fn run_callbacks(&self);
}

/// The player's one Steamworks client, shared by its main loop (callbacks) and the `steam`
/// module (server threads). Calls are serialized; [`SteamHandle::shutdown`] drops the client
/// (the player calls it before CEF shuts down), after which every command answers
/// "unavailable".
pub struct SteamHandle(Mutex<Option<Box<dyn SteamApi>>>);

// Driven by the Windows player only (`new` with its `steam` feature); the macOS/Linux gates
// compile this file without a Steamworks client.
#[allow(dead_code)]
impl SteamHandle {
    pub fn new(api: Box<dyn SteamApi>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(api))))
    }

    /// Runs `f` on the client, or returns `None` after shutdown.
    pub fn with<R>(&self, f: impl FnOnce(&dyn SteamApi) -> R) -> Option<R> {
        let guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        guard.as_deref().map(f)
    }

    /// Drops the client; `true` when it was still there.
    pub fn shutdown(&self) -> bool {
        let api = self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        api.is_some()
    }
}

/// Answer to a missing or malformed `steam` argument (the legacy protocol failed the fetch).
const BAD_ARGUMENT: &str = "missing or invalid argument";

/// `steam`: the legacy Servo `steam:` protocol (git 27ca0811d6d,
/// `ports/servoshell/desktop/protocols/steam.rs`): same commands, query names and JSON.
/// Commands that change state or open the overlay require `POST`.
struct Steam {
    handle: Option<Arc<SteamHandle>>,
}

impl Module for Steam {
    fn call(&self, cmd: &str, call: &Call<'_>) -> Reply {
        let mutating = matches!(
            cmd,
            "unlock_achievement"
                | "clear_achievement"
                | "set_stat_int"
                | "set_stat_float"
                | "store_stats"
                | "open_overlay"
                | "open_store"
        );
        let known = mutating
            || matches!(
                cmd,
                "is_available"
                    | "get_player_name"
                    | "get_app_id"
                    | "is_achievement_unlocked"
                    | "get_stat_int"
                    | "get_stat_float"
                    | "is_dlc_installed"
            );
        if !known {
            return Reply::Error(StatusCode::NOT_FOUND, "unknown command");
        }
        if mutating && !call.post() {
            return Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only");
        }
        let answer = self
            .handle
            .as_ref()
            .and_then(|handle| handle.with(|api| steam_command(api, cmd, call)));
        match answer {
            Some(Ok(value)) => Reply::Json(value),
            Some(Err(reason)) => Reply::Error(StatusCode::BAD_REQUEST, reason),
            // The legacy "unavailable" answers.
            None => Reply::Json(match cmd {
                "is_available"
                | "is_achievement_unlocked"
                | "is_dlc_installed"
                | "open_overlay"
                | "open_store" => Value::Bool(false),
                "get_stat_int" => Value::from(0i32),
                "get_stat_float" => Value::from(0f32),
                _ => Value::Null,
            }),
        }
    }
}

/// One `steam` command on a live client.
fn steam_command(api: &dyn SteamApi, cmd: &str, call: &Call<'_>) -> Result<Value, &'static str> {
    let text = |name: &str| call.arg(name).ok_or(BAD_ARGUMENT);
    let id = || text("achievement_id");
    let name = || text("name");
    Ok(match cmd {
        "is_available" => Value::Bool(true),
        "get_player_name" => Value::String(api.player_name()),
        "get_app_id" => Value::from(api.app_id()),
        "unlock_achievement" => Value::Bool(api.unlock_achievement(id()?)),
        "is_achievement_unlocked" => Value::Bool(api.is_achievement_unlocked(id()?)),
        "clear_achievement" => Value::Bool(api.clear_achievement(id()?)),
        "set_stat_int" => {
            let value = text("value")?.parse().map_err(|_| BAD_ARGUMENT)?;
            Value::Bool(api.set_stat_int(name()?, value))
        },
        "get_stat_int" => Value::from(api.get_stat_int(name()?)),
        "set_stat_float" => {
            let value = text("value")?.parse().map_err(|_| BAD_ARGUMENT)?;
            Value::Bool(api.set_stat_float(name()?, value))
        },
        "get_stat_float" => Value::from(api.get_stat_float(name()?)),
        "store_stats" => Value::Bool(api.store_stats()),
        "is_dlc_installed" => {
            let app = text("app_id")?.parse().map_err(|_| BAD_ARGUMENT)?;
            Value::Bool(api.is_dlc_installed(app))
        },
        "open_overlay" => {
            api.open_overlay(text("dialog")?);
            Value::Bool(true)
        },
        // "open_store": an absent or malformed `app_id` opens the running app's page (legacy).
        _ => {
            api.open_store(call.arg("app_id").and_then(|v| v.parse().ok()));
            Value::Bool(true)
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    const ORIGIN: &str = "http://127.0.0.1:49152";

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (n, v) in pairs {
            map.append(
                header::HeaderName::from_bytes(n.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn guard_accepts_the_game_document() {
        let ok = [("host", "127.0.0.1:49152"), ("x-roves-api", "1")];
        assert_eq!(check_request(&headers(&ok), &Method::GET, ORIGIN), Ok(()));
        let post = [
            ("host", "127.0.0.1:49152"),
            ("x-roves-api", "1"),
            ("origin", ORIGIN),
            ("sec-fetch-site", "same-origin"),
        ];
        assert_eq!(
            check_request(&headers(&post), &Method::POST, ORIGIN),
            Ok(())
        );
    }

    #[test]
    fn guard_refusals() {
        let cases: &[(&[(&str, &str)], Method)] = &[
            // DNS rebinding: a name resolving to 127.0.0.1, or localhost.
            (
                &[("host", "evil.example:49152"), ("x-roves-api", "1")],
                Method::GET,
            ),
            (
                &[("host", "localhost:49152"), ("x-roves-api", "1")],
                Method::GET,
            ),
            (
                &[("host", "127.0.0.1:49153"), ("x-roves-api", "1")],
                Method::GET,
            ),
            (&[("host", "127.0.0.1"), ("x-roves-api", "1")], Method::GET),
            (&[("x-roves-api", "1")], Method::GET),
            (
                &[
                    ("host", "127.0.0.1:49152"),
                    ("host", "127.0.0.1:49152"),
                    ("x-roves-api", "1"),
                ],
                Method::GET,
            ),
            // Missing or wrong custom header (a simple cross-origin request has none).
            (&[("host", "127.0.0.1:49152")], Method::GET),
            (
                &[("host", "127.0.0.1:49152"), ("x-roves-api", "true")],
                Method::POST,
            ),
            // Foreign or opaque origin.
            (
                &[
                    ("host", "127.0.0.1:49152"),
                    ("x-roves-api", "1"),
                    ("origin", "http://evil.example"),
                ],
                Method::POST,
            ),
            (
                &[
                    ("host", "127.0.0.1:49152"),
                    ("x-roves-api", "1"),
                    ("origin", "null"),
                ],
                Method::POST,
            ),
            (
                &[
                    ("host", "127.0.0.1:49152"),
                    ("x-roves-api", "1"),
                    ("sec-fetch-site", "cross-site"),
                ],
                Method::GET,
            ),
            // Preflight, even fully formed.
            (
                &[
                    ("host", "127.0.0.1:49152"),
                    ("x-roves-api", "1"),
                    ("origin", ORIGIN),
                ],
                Method::OPTIONS,
            ),
        ];
        for (pairs, method) in cases {
            assert!(
                check_request(&headers(pairs), method, ORIGIN).is_err(),
                "{pairs:?} {method}"
            );
        }
    }

    #[test]
    fn save_keys() {
        for good in [
            "slot-1",
            "autosave",
            "1",
            "Slot 2",
            "sav\u{e8}",
            ".hidden",
            "a.b",
            "x".repeat(200).as_str(),
        ] {
            assert_eq!(
                save_file_name(good),
                Some(format!("{good}.save")),
                "{good:?}"
            );
        }
        for bad in [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:x",
            "a\0b",
            "a\nb",
            "a?",
            "a*",
            "a|b",
            "a<b",
            "a\"b",
            "trail.",
            "trail ",
            "CON",
            "con.txt",
            "nul",
            "COM1",
            "lpt9.x",
            "x".repeat(201).as_str(),
        ] {
            assert_eq!(save_file_name(bad), None, "{bad:?}");
        }
        assert!(save_file_name("COM").is_some());
        assert!(save_file_name("console").is_some());
    }

    struct TempDir(PathBuf, #[allow(dead_code)] tempfile::TempDir);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = tempfile::Builder::new()
                .prefix(&format!("roves-host-api-{tag}-"))
                .tempdir()
                .unwrap();
            Self(dir.path().to_path_buf(), dir)
        }
    }

    fn call(
        module: &dyn Module,
        method: Method,
        cmd: &str,
        key: Option<&str>,
        body: &[u8],
    ) -> Reply {
        let query: Vec<(String, String)> = key
            .map(|k| vec![("key".to_owned(), k.to_owned())])
            .unwrap_or_default();
        module.call(
            cmd,
            &Call {
                method: &method,
                query: &query,
                body,
            },
        )
    }

    #[test]
    fn saves_round_trip() {
        let tmp = TempDir::new("saves");
        let dir = tmp.0.join("saves");
        // A temporary file left by an interrupted write is removed on open.
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".roves-save-AbC123.tmp"), b"partial").unwrap();
        let saves = Saves::open(Some(dir.clone()));
        assert_eq!(saves.status().unwrap(), dir);
        assert!(!dir.join(".roves-save-AbC123.tmp").exists());
        let json = |r: Reply| match r {
            Reply::Json(v) => v,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            json(call(&saves, Method::GET, "is_available", None, b"")),
            json!(true)
        );
        assert_eq!(
            json(call(&saves, Method::GET, "list", None, b"")),
            json!([])
        );
        assert_eq!(
            json(call(&saves, Method::GET, "read", Some("b"), b"")),
            Value::Null
        );
        assert_eq!(
            json(call(&saves, Method::GET, "most_recent", None, b"")),
            Value::Null
        );
        assert_eq!(
            json(call(&saves, Method::POST, "write", Some("b"), b"{\"n\":1}")),
            json!(true)
        );
        assert_eq!(
            json(call(&saves, Method::POST, "write", Some("a"), b"\x00\xff")),
            json!(true)
        );
        // Overwrite through the temporary file: no temporary file stays behind.
        assert_eq!(
            json(call(&saves, Method::POST, "write", Some("b"), b"{\"n\":2}")),
            json!(true)
        );
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        fs::write(dir.join("junk.txt"), b"x").unwrap();
        assert_eq!(
            json(call(&saves, Method::GET, "list", None, b"")),
            json!(["a", "b"])
        );
        assert_eq!(
            json(call(&saves, Method::GET, "read", Some("b"), b"")),
            json!(base64::engine::general_purpose::STANDARD.encode(b"{\"n\":2}"))
        );
        assert_eq!(
            json(call(&saves, Method::GET, "read", Some("a"), b"")),
            json!("AP8=")
        );
        let recent = json(call(&saves, Method::GET, "most_recent", None, b""));
        assert!(recent["modifiedMs"].as_i64().unwrap() > 0);
        // Mutations need POST; invalid keys are refused.
        assert_eq!(
            call(&saves, Method::GET, "write", Some("a"), b"x"),
            Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only")
        );
        assert_eq!(
            call(&saves, Method::POST, "write", Some("../escape"), b"x"),
            Reply::Error(StatusCode::BAD_REQUEST, "invalid key")
        );
        assert_eq!(
            call(&saves, Method::GET, "read", None, b""),
            Reply::Error(StatusCode::BAD_REQUEST, "invalid key")
        );
        assert!(!tmp.0.join("escape.save").exists());
        assert_eq!(
            json(call(&saves, Method::POST, "delete", Some("a"), b"")),
            json!(true)
        );
        assert_eq!(
            json(call(&saves, Method::POST, "delete", Some("a"), b"")),
            json!(true)
        );
        assert_eq!(
            json(call(&saves, Method::GET, "list", None, b"")),
            json!(["b"])
        );
        assert_eq!(
            json(call(&saves, Method::POST, "clear", None, b"")),
            json!(true)
        );
        assert_eq!(
            json(call(&saves, Method::GET, "list", None, b"")),
            json!([])
        );
        assert!(dir.join("junk.txt").exists());
        assert_eq!(
            call(&saves, Method::GET, "nope", None, b""),
            Reply::Error(StatusCode::NOT_FOUND, "unknown command")
        );
    }

    #[test]
    fn saves_unavailable_without_profile() {
        let saves = Saves::open(None);
        assert!(saves.status().is_err());
        for (method, cmd, expected) in [
            (Method::GET, "is_available", json!(false)),
            (Method::GET, "list", json!([])),
            (Method::GET, "read", Value::Null),
            (Method::GET, "most_recent", Value::Null),
            (Method::POST, "write", json!(false)),
            (Method::POST, "delete", json!(false)),
            (Method::POST, "clear", json!(false)),
        ] {
            assert_eq!(
                call(&saves, method, cmd, Some("k"), b"x"),
                Reply::Json(expected),
                "{cmd}"
            );
        }
    }

    #[test]
    fn core_commands() {
        let asked = Arc::new(AtomicBool::new(false));
        let flag = asked.clone();
        let core = Core {
            engine_version: "test-engine".into(),
            request_exit: Arc::new(move |_| flag.store(true, Ordering::SeqCst)),
        };
        let Reply::Json(info) = call(&core, Method::GET, "system_info", None, b"") else {
            panic!("system_info");
        };
        assert_eq!(info["engine_version"], "test-engine");
        assert_eq!(info["architecture"], std::env::consts::ARCH);
        assert!(info["os_type"].is_string());
        assert_eq!(
            call(&core, Method::GET, "exit", None, b""),
            Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only")
        );
        assert!(!asked.load(Ordering::SeqCst));
        assert_eq!(
            call(&core, Method::POST, "exit", None, b""),
            Reply::Json(json!(true))
        );
        assert!(asked.load(Ordering::SeqCst));
    }

    /// A recording stand-in for the player's Steamworks client.
    #[derive(Default)]
    struct FakeSteam(Arc<Mutex<Vec<String>>>);

    impl FakeSteam {
        fn log(&self, line: String) -> bool {
            self.0.lock().unwrap().push(line);
            true
        }
    }

    impl SteamApi for FakeSteam {
        fn player_name(&self) -> String {
            "player".into()
        }
        fn app_id(&self) -> u32 {
            480
        }
        fn unlock_achievement(&self, id: &str) -> bool {
            self.log(format!("unlock {id}"))
        }
        fn is_achievement_unlocked(&self, id: &str) -> bool {
            id == "ACH_DONE"
        }
        fn clear_achievement(&self, id: &str) -> bool {
            self.log(format!("clear {id}"))
        }
        fn set_stat_int(&self, name: &str, value: i32) -> bool {
            self.log(format!("int {name}={value}"))
        }
        fn get_stat_int(&self, _: &str) -> i32 {
            7
        }
        fn set_stat_float(&self, name: &str, value: f32) -> bool {
            self.log(format!("float {name}={value}"))
        }
        fn get_stat_float(&self, _: &str) -> f32 {
            1.5
        }
        fn store_stats(&self) -> bool {
            self.log("store".into())
        }
        fn is_dlc_installed(&self, app_id: u32) -> bool {
            app_id == 10
        }
        fn open_overlay(&self, dialog: &str) {
            self.log(format!("overlay {dialog}"));
        }
        fn open_store(&self, app_id: Option<u32>) {
            self.log(format!("store page {app_id:?}"));
        }
        fn run_callbacks(&self) {
            self.log("callbacks".into());
        }
    }

    fn steam_call(module: &dyn Module, method: Method, cmd: &str, query: &[(&str, &str)]) -> Reply {
        let query: Vec<(String, String)> = query
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect();
        module.call(
            cmd,
            &Call {
                method: &method,
                query: &query,
                body: b"",
            },
        )
    }

    #[test]
    fn steam_commands_on_a_client() {
        let fake = FakeSteam::default();
        let calls = fake.0.clone();
        let handle = SteamHandle::new(Box::new(fake));
        let steam = Steam {
            handle: Some(handle.clone()),
        };
        let get = |cmd: &str, query: &[(&str, &str)]| steam_call(&steam, Method::GET, cmd, query);
        let post = |cmd: &str, query: &[(&str, &str)]| steam_call(&steam, Method::POST, cmd, query);
        let json = |v: Value| Reply::Json(v);
        assert_eq!(get("is_available", &[]), json(json!(true)));
        assert_eq!(get("get_player_name", &[]), json(json!("player")));
        assert_eq!(get("get_app_id", &[]), json(json!(480)));
        let ach = [("achievement_id", "ACH_DONE")];
        assert_eq!(get("is_achievement_unlocked", &ach), json(json!(true)));
        assert_eq!(get("get_stat_int", &[("name", "wins")]), json(json!(7)));
        assert_eq!(get("get_stat_float", &[("name", "t")]), json(json!(1.5)));
        assert_eq!(
            get("is_dlc_installed", &[("app_id", "10")]),
            json(json!(true))
        );
        assert_eq!(post("unlock_achievement", &ach), json(json!(true)));
        assert_eq!(post("clear_achievement", &ach), json(json!(true)));
        let int = [("name", "wins"), ("value", "-3")];
        assert_eq!(post("set_stat_int", &int), json(json!(true)));
        let float = [("name", "t"), ("value", "2.25")];
        assert_eq!(post("set_stat_float", &float), json(json!(true)));
        assert_eq!(post("store_stats", &[]), json(json!(true)));
        assert_eq!(
            post("open_overlay", &[("dialog", "achievements")]),
            json(json!(true))
        );
        assert_eq!(post("open_store", &[]), json(json!(true)));
        assert_eq!(post("open_store", &[("app_id", "20")]), json(json!(true)));
        assert_eq!(handle.with(|api| api.run_callbacks()), Some(()));
        assert_eq!(
            *calls.lock().unwrap(),
            [
                "unlock ACH_DONE",
                "clear ACH_DONE",
                "int wins=-3",
                "float t=2.25",
                "store",
                "overlay achievements",
                "store page None",
                "store page Some(20)",
                "callbacks",
            ]
        );
        // Mutations need POST; missing or malformed arguments are refused before Steamworks.
        assert_eq!(
            get("unlock_achievement", &ach),
            Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only")
        );
        for (cmd, query) in [
            ("unlock_achievement", &[][..]),
            ("set_stat_int", &[("name", "wins"), ("value", "1.5")][..]),
            ("set_stat_float", &[("value", "1")][..]),
            ("open_overlay", &[][..]),
        ] {
            assert_eq!(
                post(cmd, query),
                Reply::Error(StatusCode::BAD_REQUEST, BAD_ARGUMENT),
                "{cmd}"
            );
        }
        assert_eq!(
            get("is_dlc_installed", &[("app_id", "x")]),
            Reply::Error(StatusCode::BAD_REQUEST, BAD_ARGUMENT)
        );
        assert_eq!(
            get("nope", &[]),
            Reply::Error(StatusCode::NOT_FOUND, "unknown command")
        );
        assert_eq!(calls.lock().unwrap().len(), 9);
        // After the player's shutdown: the "unavailable" answers, nothing reaches the client.
        assert!(handle.shutdown());
        assert!(!handle.shutdown());
        assert_eq!(get("is_available", &[]), json(json!(false)));
        assert_eq!(handle.with(|api| api.run_callbacks()), None);
    }

    #[test]
    fn steam_unavailable_answers() {
        let steam = Steam { handle: None };
        for (method, cmd, expected) in [
            (Method::GET, "is_available", json!(false)),
            (Method::GET, "get_player_name", Value::Null),
            (Method::GET, "get_app_id", Value::Null),
            (Method::GET, "is_achievement_unlocked", json!(false)),
            (Method::GET, "get_stat_int", json!(0)),
            (Method::GET, "get_stat_float", json!(0.0)),
            (Method::GET, "is_dlc_installed", json!(false)),
            (Method::POST, "unlock_achievement", Value::Null),
            (Method::POST, "clear_achievement", Value::Null),
            (Method::POST, "set_stat_int", Value::Null),
            (Method::POST, "set_stat_float", Value::Null),
            (Method::POST, "store_stats", Value::Null),
            (Method::POST, "open_overlay", json!(false)),
            (Method::POST, "open_store", json!(false)),
        ] {
            assert_eq!(
                steam_call(&steam, method, cmd, &[]),
                Reply::Json(expected),
                "{cmd}"
            );
        }
        assert_eq!(
            steam_call(&steam, Method::GET, "store_stats", &[]),
            Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only")
        );
        assert_eq!(Value::from(0f32).to_string(), "0.0");
    }

    #[test]
    fn routes_over_http() {
        use tower::ServiceExt as _;
        let tmp = TempDir::new("routes");
        let exits = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter = exits.clone();
        let (api, status) = HostApi::new(HostConfig {
            saves_dir: Some(tmp.0.join("saves")),
            engine_version: "test-engine".into(),
            request_exit: Arc::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            steam: None,
        });
        assert!(status.is_ok());
        let router = api.router(ORIGIN);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let send = |method: Method, uri: &str, pairs: &[(&str, &str)], body: &'static [u8]| {
            let mut req = axum::http::Request::builder().method(method).uri(uri);
            for (n, v) in pairs {
                req = req.header(*n, *v);
            }
            let req = req.body(Body::from(body)).unwrap();
            runtime.block_on(async {
                let response = router.clone().oneshot(req).await.unwrap();
                let (parts, body) = response.into_parts();
                let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
                (parts, String::from_utf8(bytes.to_vec()).unwrap())
            })
        };
        let ok: &[(&str, &str)] = &[("host", "127.0.0.1:49152"), ("x-roves-api", "1")];
        let (parts, body) = send(
            Method::POST,
            "/__roves/saves/write?key=slot%201",
            ok,
            b"hello",
        );
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(body, "true");
        assert_eq!(parts.headers["cache-control"], "no-store");
        assert_eq!(parts.headers["content-type"], "application/json");
        assert!(tmp.0.join("saves").join("slot 1.save").is_file());
        let (parts, body) = send(Method::GET, "/__roves/saves/read?key=slot%201", ok, b"");
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(body, "\"aGVsbG8=\"");
        let (_, body) = send(Method::GET, "/__roves/saves/list", ok, b"");
        assert_eq!(body, "[\"slot 1\"]");
        let (parts, _) = send(Method::GET, "/__roves/core/system_info", ok, b"");
        assert_eq!(parts.status, StatusCode::OK);
        // Refusals: no CORS header ever, no-store everywhere.
        for (method, pairs) in [
            (
                Method::GET,
                &[("host", "evil.example:49152"), ("x-roves-api", "1")][..],
            ),
            (Method::GET, &[("host", "127.0.0.1:49152")][..]),
            (
                Method::OPTIONS,
                &[
                    ("host", "127.0.0.1:49152"),
                    ("origin", "http://evil.example"),
                    ("access-control-request-method", "POST"),
                    ("access-control-request-headers", "x-roves-api"),
                ][..],
            ),
        ] {
            let (parts, _) = send(method.clone(), "/__roves/saves/list", pairs, b"");
            assert_eq!(parts.status, StatusCode::FORBIDDEN, "{method} {pairs:?}");
            assert_eq!(parts.headers["cache-control"], "no-store");
            assert!(
                !parts
                    .headers
                    .keys()
                    .any(|n| n.as_str().starts_with("access-control-")),
                "{method}"
            );
        }
        let (parts, _) = send(Method::GET, "/__roves/steam/x", ok, b"");
        assert_eq!(parts.status, StatusCode::NOT_FOUND);
        let (parts, body) = send(Method::GET, "/__roves/steam/is_available", ok, b"");
        assert_eq!((parts.status, body.as_str()), (StatusCode::OK, "false"));
        let (parts, _) = send(Method::GET, "/__roves/nope/x", ok, b"");
        assert_eq!(parts.status, StatusCode::NOT_FOUND);
        let (parts, _) = send(Method::GET, "/__roves/saves/a/b", ok, b"");
        assert_eq!(parts.status, StatusCode::NOT_FOUND);
        let (parts, _) = send(Method::PUT, "/__roves/saves/list", ok, b"");
        assert_eq!(parts.status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        let (parts, body) = send(Method::POST, "/__roves/core/exit", ok, b"");
        assert_eq!((parts.status, body.as_str()), (StatusCode::OK, "true"));
        assert_eq!(exits.load(Ordering::SeqCst), 1);
    }
}
