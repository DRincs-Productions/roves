//! Pure helpers of the optional local MCP bridge (README "Local MCP bridge"): limits, the
//! session token check, the pipe name, the page capability script and the fixed key
//! allow-list (std only, unit-tested
//! stand-alone: `rustc --edition 2024 --test src/mcp_rules.rs`).

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
/// The browser's first document while the marker script is registered; the game URL is
/// loaded only after that registration settled.
pub(crate) const BOOTSTRAP_URL: &str = "about:blank";

/// Document-start script registered through CDP `Page.addScriptToEvaluateOnNewDocument`
/// (main world). Chromium runs it in every new document of the page target as soon as its
/// main-world context exists, before any of the document's own scripts; it defines the marker
/// only in the top-level frame and only on the game's own loopback origin, as a read-only,
/// non-configurable `true`. `None` for anything but `http://127.0.0.1:<port>`, so the origin
/// literal never needs escaping.
pub(crate) fn page_capability_script(origin: &str) -> Option<String> {
    let port = origin.strip_prefix("http://127.0.0.1:")?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!(
        "(() => {{ if (window.top !== window || location.origin !== \"{origin}\") return; \
         Object.defineProperty(window, \"{PAGE_MARKER}\", {{ value: true, writable: false, \
         enumerable: false, configurable: false }}); }})();"
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
        let script = page_capability_script("http://127.0.0.1:49152").unwrap();
        assert!(script.contains("window.top !== window"));
        assert!(script.contains("location.origin !== \"http://127.0.0.1:49152\""));
        assert!(script.contains(&format!("\"{PAGE_MARKER}\", {{ value: true,")));
        assert!(script.contains("writable: false"));
        assert!(script.contains("configurable: false"));
        // Nothing but the origin literal and the marker: no session detail can be embedded.
        assert!(!script.contains("pipe") && !script.contains("token"));
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
            assert_eq!(page_capability_script(origin), None, "{origin:?}");
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
