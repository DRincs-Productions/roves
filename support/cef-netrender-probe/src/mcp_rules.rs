//! Pure helpers of the optional local MCP bridge (README "Local MCP bridge"): limits, the
//! session token check, the Windows pipe name, the per-OS session directory and its private
//! files, the page marker script and the fixed key allow-list (std only, unit-tested
//! stand-alone on every OS: `rustc --edition 2024 --test src/mcp_rules.rs`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Random bytes of the per-session token (hex-encoded in the session record).
pub(crate) const TOKEN_BYTES: usize = 32;
/// Random bytes of the pipe-name nonce, so the name cannot be guessed from the PID alone.
pub(crate) const NONCE_BYTES: usize = 16;
/// Longest accepted request line, newline included (the token line too).
pub(crate) const MAX_REQUEST_LINE: usize = 20 * 1024;
/// Longest `evaluate` expression, in bytes.
pub(crate) const MAX_EXPRESSION: usize = 16 * 1024;
/// Longest serialized `evaluate` result, in bytes.
pub(crate) const MAX_RESULT: usize = 64 * 1024;
/// Longest `type_text` text, in characters.
pub(crate) const MAX_TEXT: usize = 256;
/// Console lines kept for `state` (newest last).
pub(crate) const CONSOLE_RING: usize = 50;
/// Longest console message kept for `state`, in characters.
pub(crate) const CONSOLE_MESSAGE_MAX: usize = 512;

/// Page-visible capability marker read by `@drincs/roves-api/core`'s `isMcpEnabled()`. Only
/// the boolean `true` is ever exposed: no pipe name, token or other session detail.
pub(crate) const PAGE_MARKER: &str = "__ROVES_MCP__";
/// Host marker read by `@drincs/roves-api/core`'s `isAvailable()` (README "Host API"): a
/// frozen `{ transport: "http", api: "/__roves/" }` telling roves-api to call the host API
/// routes of the loopback origin. Defined in every game-mode launch.
pub(crate) const HOST_MARKER: &str = "__ROVES__";
/// The browser's first document while the marker script is registered; the game URL is
/// loaded only after that registration settled.
pub(crate) const BOOTSTRAP_URL: &str = "about:blank";

/// Document-start script registered through CDP `Page.addScriptToEvaluateOnNewDocument`
/// (main world). Chromium runs it in every new document of the page target as soon as its
/// main-world context exists, before any of the document's own scripts; it defines the
/// markers only in the top-level frame and only on the game's own loopback origin, as
/// read-only, non-configurable properties: [`HOST_MARKER`] always, [`PAGE_MARKER`] (`true`)
/// only with `mcp`. `None` for anything but `http://127.0.0.1:<port>`, so the origin literal
/// never needs escaping.
pub(crate) fn page_marker_script(origin: &str, mcp: bool) -> Option<String> {
    let port = origin.strip_prefix("http://127.0.0.1:")?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let define = |name: &str, value: &str| {
        format!(
            "Object.defineProperty(window, \"{name}\", {{ value: {value}, writable: false,              enumerable: false, configurable: false }});"
        )
    };
    let host = define(
        HOST_MARKER,
        "Object.freeze({ transport: \"http\", api: \"/__roves/\" })",
    );
    let mcp = if mcp {
        define(PAGE_MARKER, "true")
    } else {
        String::new()
    };
    Some(format!(
        "(() => {{ if (window.top !== window || location.origin !== \"{origin}\") return;          {host} {mcp}}})();"
    ))
}

/// Lower-case hex of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// `\\.\pipe\roves-mcp-<pid>-<nonce>`: local named pipe of one game process.
pub(crate) fn pipe_name(pid: u32, nonce: &str) -> String {
    format!(r"\\.\pipe\roves-mcp-{pid}-{nonce}")
}

/// Operating systems with a session directory rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordOs {
    Windows,
    MacOs,
    Linux,
}

impl RecordOs {
    /// The OS this file is compiled for (`None` elsewhere: no bridge).
    pub(crate) const CURRENT: Option<RecordOs> = if cfg!(windows) {
        Some(RecordOs::Windows)
    } else if cfg!(target_os = "macos") {
        Some(RecordOs::MacOs)
    } else if cfg!(target_os = "linux") {
        Some(RecordOs::Linux)
    } else {
        None
    };
}

/// Per-user directory of the session records (and, on macOS/Linux, of the sockets), from the
/// environment read through `var`:
///
/// - Windows: `%LOCALAPPDATA%\Roves\mcp` (inside the user's profile and its ACL);
/// - macOS: `$HOME/Library/Application Support/Roves/mcp`;
/// - Linux: `$XDG_RUNTIME_DIR/roves/mcp` (the per-user runtime directory), else
///   `$HOME/.local/state/roves/mcp`. As in the XDG Base Directory specification, a relative
///   value counts as unset.
///
/// `mcp/server.mjs` applies the same rule.
pub(crate) fn record_dir(
    os: RecordOs,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    match os {
        RecordOs::Windows => {
            let local = var("LOCALAPPDATA")
                .filter(|value| !value.is_empty())
                .ok_or("LOCALAPPDATA is not set")?;
            Ok(PathBuf::from(local).join("Roves").join("mcp"))
        },
        RecordOs::MacOs => Ok(absolute("HOME")
            .ok_or("HOME is not an absolute path")?
            .join("Library")
            .join("Application Support")
            .join("Roves")
            .join("mcp")),
        RecordOs::Linux => match absolute("XDG_RUNTIME_DIR") {
            Some(runtime) => Ok(runtime.join("roves").join("mcp")),
            None => Ok(absolute("HOME")
                .ok_or("neither XDG_RUNTIME_DIR nor HOME is an absolute path")?
                .join(".local")
                .join("state")
                .join("roves")
                .join("mcp")),
        },
    }
}

/// Creates (every missing part with mode 0700) or checks the session directory: a real
/// directory (not a symlink) owned by `uid`, with group and other access removed. Only
/// processes of that user (and root) can then reach the socket and the record inside.
#[cfg(unix)]
pub(crate) fn private_dir(dir: &Path, uid: u32) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let fail = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(fail)?;
    let meta = std::fs::symlink_metadata(dir).map_err(fail)?;
    if !meta.file_type().is_dir() {
        return Err(format!("{}: not a directory", dir.display()));
    }
    if meta.uid() != uid {
        return Err(format!(
            "{}: owned by uid {}, not by this user (uid {uid})",
            dir.display(),
            meta.uid()
        ));
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(fail)?;
    }
    Ok(())
}

/// Writes `body` to `path` through `temporary` and a rename, so a reader never sees a partial
/// record. The temporary file is created fresh (never through an existing file or link) and,
/// on unix, with mode 0600.
pub(crate) fn write_private(path: &Path, temporary: &Path, body: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let _ = std::fs::remove_file(temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(temporary)
        .and_then(|mut file| file.write_all(body))
        .and_then(|()| std::fs::rename(temporary, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(temporary);
            format!("{}: {e}", path.display())
        })
}

/// Compares the client's first line (trailing CR/LF removed) with the session token without
/// an early exit on the first differing byte.
pub(crate) fn token_matches(expected: &str, line: &str) -> bool {
    let given = line.trim_end_matches(['\r', '\n']).as_bytes();
    let expected = expected.as_bytes();
    if expected.is_empty() || given.len() != expected.len() {
        return false;
    }
    expected
        .iter()
        .zip(given)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

/// Keys `press_key` accepts: DOM `key` names of non-character keys, mapped by the bridge to
/// the SDL keycode/scancode the probe's ordinary keyboard path translates. A fixed list, not a
/// general key-injection API; characters go through `type_text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum McpKey {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

impl McpKey {
    pub(crate) const ALL: [McpKey; 14] = [
        McpKey::ArrowLeft,
        McpKey::ArrowRight,
        McpKey::ArrowUp,
        McpKey::ArrowDown,
        McpKey::Enter,
        McpKey::Escape,
        McpKey::Space,
        McpKey::Tab,
        McpKey::Backspace,
        McpKey::Delete,
        McpKey::Home,
        McpKey::End,
        McpKey::PageUp,
        McpKey::PageDown,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            McpKey::ArrowLeft => "ArrowLeft",
            McpKey::ArrowRight => "ArrowRight",
            McpKey::ArrowUp => "ArrowUp",
            McpKey::ArrowDown => "ArrowDown",
            McpKey::Enter => "Enter",
            McpKey::Escape => "Escape",
            McpKey::Space => "Space",
            McpKey::Tab => "Tab",
            McpKey::Backspace => "Backspace",
            McpKey::Delete => "Delete",
            McpKey::Home => "Home",
            McpKey::End => "End",
            McpKey::PageUp => "PageUp",
            McpKey::PageDown => "PageDown",
        }
    }

    /// Exact, case-sensitive name match.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        McpKey::ALL.into_iter().find(|key| key.name() == name)
    }
}

/// Whether `(x, y)` (window points, i.e. SDL logical coordinates) lies inside a window of
/// `width` x `height` points.
pub(crate) fn point_in_window(x: f64, y: f64, width: u32, height: u32) -> bool {
    x.is_finite()
        && y.is_finite()
        && x >= 0.0
        && y >= 0.0
        && x < f64::from(width)
        && y < f64::from(height)
}

/// `type_text` input: 1..=`MAX_TEXT` characters, no control characters (keys such as Enter
/// or Tab go through `press_key`).
pub(crate) fn valid_text(text: &str) -> bool {
    let count = text.chars().count();
    (1..=MAX_TEXT).contains(&count) && !text.chars().any(char::is_control)
}

/// At most `max` characters of `text` (no allocation when it already fits).
pub(crate) fn truncate_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encodes_every_byte_as_two_lower_case_digits() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn pipe_names_are_local_and_per_process() {
        assert_eq!(pipe_name(42, "00ff"), r"\\.\pipe\roves-mcp-42-00ff");
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn session_directories_follow_each_platform_rule() {
        let local = r"C:\Users\a\AppData\Local";
        assert_eq!(
            record_dir(RecordOs::Windows, env(&[("LOCALAPPDATA", local)])).unwrap(),
            PathBuf::from(local).join("Roves").join("mcp")
        );
        assert!(record_dir(RecordOs::Windows, env(&[("LOCALAPPDATA", "")])).is_err());
        assert!(record_dir(RecordOs::Windows, env(&[("HOME", "/home/a")])).is_err());

        // Absolute-path checks are the host's (`Path::is_absolute`), so the unix rules are
        // exercised with unix paths on unix only.
        if cfg!(unix) {
            assert_eq!(
                record_dir(RecordOs::MacOs, env(&[("HOME", "/Users/a")])).unwrap(),
                Path::new("/Users/a/Library/Application Support/Roves/mcp")
            );
            let linux = |pairs: &[(&str, &str)]| record_dir(RecordOs::Linux, env(pairs));
            assert_eq!(
                linux(&[("XDG_RUNTIME_DIR", "/run/user/1000"), ("HOME", "/home/a")]).unwrap(),
                Path::new("/run/user/1000/roves/mcp")
            );
            // A relative XDG_RUNTIME_DIR counts as unset (XDG Base Directory specification).
            assert_eq!(
                linux(&[("XDG_RUNTIME_DIR", "run"), ("HOME", "/home/a")]).unwrap(),
                Path::new("/home/a/.local/state/roves/mcp")
            );
            assert_eq!(
                linux(&[("HOME", "/home/a")]).unwrap(),
                Path::new("/home/a/.local/state/roves/mcp")
            );
        }
        assert!(record_dir(RecordOs::MacOs, env(&[("HOME", "relative")])).is_err());
        assert!(record_dir(RecordOs::MacOs, env(&[])).is_err());
        assert!(record_dir(RecordOs::Linux, env(&[("HOME", "home")])).is_err());
        assert!(record_dir(RecordOs::Linux, env(&[])).is_err());
    }

    /// A fresh, empty scratch directory under the system temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "roves-mcp-rules-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn records_are_replaced_whole_and_private() {
        let dir = scratch("record");
        let (path, temporary) = (dir.join("42.json"), dir.join("42.json.tmp"));
        // A stale temporary file (an earlier crash) does not block the write.
        std::fs::write(&temporary, "stale").unwrap();
        write_private(&path, &temporary, br#"{"version":1}"#).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"version":1}"#);
        assert!(!temporary.exists());
        write_private(&path, &temporary, b"{}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
        // A missing directory fails and leaves nothing behind.
        let missing = dir.join("missing");
        assert!(
            write_private(&missing.join("1.json"), &missing.join("1.json.tmp"), b"{}").is_err()
        );
        assert!(!missing.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn session_directories_are_private_to_their_owner() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = scratch("dir");
        let uid = std::fs::metadata(&root).unwrap().uid();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        // Created with every missing part 0700.
        let dir = root.join("roves").join("mcp");
        private_dir(&dir, uid).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&root.join("roves")), 0o700);
        // An existing directory open to others is tightened.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        private_dir(&dir, uid).unwrap();
        assert_eq!(mode(&dir), 0o700);
        // Another owner, a file and a symlink are refused.
        assert!(
            private_dir(&dir, uid.wrapping_add(1))
                .unwrap_err()
                .contains("owned by uid")
        );
        let file = root.join("file");
        std::fs::write(&file, "").unwrap();
        assert!(private_dir(&file, uid).is_err());
        let link = root.join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(
            private_dir(&link, uid)
                .unwrap_err()
                .contains("not a directory")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_token_must_match_exactly() {
        let token = "ab".repeat(TOKEN_BYTES);
        assert!(token_matches(&token, &format!("{token}\n")));
        assert!(token_matches(&token, &format!("{token}\r\n")));
        assert!(token_matches(&token, &token));
        assert!(!token_matches(&token, ""));
        assert!(!token_matches(&token, "\n"));
        assert!(!token_matches(&token, &token[1..]));
        assert!(!token_matches(&token, &format!("{token}0\n")));
        assert!(!token_matches(&token, &token.to_uppercase()));
        assert!(!token_matches(&token, &format!(" {token}")));
        // An empty expected token never authenticates anything.
        assert!(!token_matches("", ""));
        assert!(!token_matches("", "\n"));
    }

    #[test]
    fn the_page_script_is_bounded_to_the_top_level_loopback_origin() {
        for mcp in [false, true] {
            let script = page_marker_script("http://127.0.0.1:49152", mcp).unwrap();
            assert!(script.contains("window.top !== window"));
            assert!(script.contains("location.origin !== \"http://127.0.0.1:49152\""));
            assert!(script.contains(&format!(
                "\"{HOST_MARKER}\", {{ value: Object.freeze({{ transport: \"http\", api: \"/__roves/\" }}),"
            )));
            assert_eq!(
                script.contains(&format!("\"{PAGE_MARKER}\", {{ value: true,")),
                mcp,
                "mcp={mcp}"
            );
            assert_eq!(
                script.matches("writable: false").count(),
                1 + usize::from(mcp)
            );
            assert_eq!(
                script.matches("configurable: false").count(),
                1 + usize::from(mcp)
            );
            // Nothing but the origin literal and the markers: no session detail can be
            // embedded.
            assert!(!script.contains("pipe") && !script.contains("token"));
        }
        for origin in [
            "",
            "http://127.0.0.1:",
            "http://127.0.0.1:123456",
            "http://127.0.0.1:80/",
            "http://localhost:80",
            "https://127.0.0.1:80",
            "http://127.0.0.1:80\"",
            "http://127.0.0.1:8a",
        ] {
            assert_eq!(page_marker_script(origin, true), None, "{origin:?}");
            assert_eq!(page_marker_script(origin, false), None, "{origin:?}");
        }
    }

    #[test]
    fn keys_round_trip_and_unknown_names_are_refused() {
        for key in McpKey::ALL {
            assert_eq!(McpKey::parse(key.name()), Some(key));
        }
        for name in ["arrowright", "Right", "a", "F5", "", " Enter", "Enter "] {
            assert_eq!(McpKey::parse(name), None, "{name:?}");
        }
    }

    #[test]
    fn points_must_be_finite_and_inside_the_window() {
        assert!(point_in_window(0.0, 0.0, 1280, 720));
        assert!(point_in_window(1279.5, 719.5, 1280, 720));
        assert!(!point_in_window(1280.0, 10.0, 1280, 720));
        assert!(!point_in_window(10.0, 720.0, 1280, 720));
        assert!(!point_in_window(-0.5, 10.0, 1280, 720));
        assert!(!point_in_window(f64::NAN, 10.0, 1280, 720));
        assert!(!point_in_window(10.0, f64::INFINITY, 1280, 720));
        assert!(!point_in_window(0.0, 0.0, 0, 0));
    }

    #[test]
    fn text_is_bounded_and_printable() {
        assert!(valid_text("hello"));
        assert!(valid_text("città ✓"));
        assert!(valid_text(&"x".repeat(MAX_TEXT)));
        assert!(!valid_text(&"x".repeat(MAX_TEXT + 1)));
        assert!(!valid_text(""));
        assert!(!valid_text("a\nb"));
        assert!(!valid_text("\t"));
        assert!(!valid_text("\u{7f}"));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        assert_eq!(truncate_chars("àbc", 2), "àb");
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_chars("abc", 10), "abc");
        assert_eq!(truncate_chars("abc", 0), "");
    }
}
