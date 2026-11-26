//! Single-document navigation and external links of the CEF players (Windows `app.rs`, macOS
//! and Linux `player.rs`, compiled there through `#[path]`; unit-tested by
//! `support/cef-host-api-tests`).
//!
//! - [`main_frame_policy`]: welding's main-frame navigation veto
//!   (`CefSurfaceConfig::main_frame_navigation_policy`, CEF `OnBeforeBrowse`). The game stays
//!   one document on its loopback origin: navigations on that origin (the game URL, same-origin
//!   links and reloads) proceed, the `about:blank` page-marker bootstrap proceeds until the game
//!   started, everything else is cancelled before its request is sent.
//! - [`ExternalLinks`]: what the player does with a refused popup (`window.open`,
//!   `target=_blank`) or a vetoed off-origin navigation: open it in the system browser only
//!   for http/https/mailto after a real user gesture, at most once per
//!   [`EXTERNAL_OPEN_INTERVAL`]; refuse everything else.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::game_rules::on_origin;

/// The page-marker bootstrap document (`mcp_rules::BOOTSTRAP_URL`).
const BOOTSTRAP_URL: &str = "about:blank";

/// Shortest interval between two external opens: a game cannot spam the system browser.
pub(crate) const EXTERNAL_OPEN_INTERVAL: Duration = Duration::from_secs(1);

/// Test-only environment variable (CI): log `external link opened` without launching the
/// system browser.
pub(crate) const TEST_EXTERNAL_OPEN_LOG_ENV: &str = "ROVES_TEST_EXTERNAL_OPEN_LOG";

/// The main-frame navigation policy of a game served on `origin` (`http://127.0.0.1:<port>`,
/// no trailing slash). Called by welding on CEF's UI thread; cheap and lock-free.
pub(crate) fn main_frame_policy(origin: &str) -> impl Fn(&str) -> bool + Send + Sync + 'static {
    let origin = origin.to_owned();
    let game_started = Arc::new(AtomicBool::new(false));
    move |url: &str| {
        if on_origin(&origin, url) {
            game_started.store(true, Ordering::Release);
            true
        } else {
            url == BOOTSTRAP_URL && !game_started.load(Ordering::Acquire)
        }
    }
}

/// The decision for one external link. `shown` is the URL reduced to scheme and host (no
/// user info, path, query or fragment), the only form that reaches the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalLink {
    /// Open `url` (the parsed, serialized URL) in the system browser.
    Open {
        url: String,
        shown: String,
    },
    Refuse {
        shown: String,
        reason: &'static str,
    },
}

/// The rate limiter of external opens (one per player).
#[derive(Debug, Default)]
pub(crate) struct ExternalLinks {
    last_open: Option<Instant>,
    /// [`TEST_EXTERNAL_OPEN_LOG_ENV`] was set: accepted links are logged, not launched.
    log_only: bool,
}

impl ExternalLinks {
    /// The player's limiter; [`TEST_EXTERNAL_OPEN_LOG_ENV`]`=1` selects the log-only mode.
    pub(crate) fn from_env() -> Self {
        Self {
            last_open: None,
            log_only: std::env::var_os(TEST_EXTERNAL_OPEN_LOG_ENV).is_some_and(|v| v == "1"),
        }
    }

    /// Decides `url` (see [`Self::decide`]) and, when accepted, launches it with `launch`
    /// (the player's `open::that_detached`) unless in the log-only mode. `via` is `popup` or
    /// `navigation`. Returns the log line, without the player's prefix.
    pub(crate) fn handle(
        &mut self,
        origin: &str,
        via: &str,
        url: &str,
        user_gesture: bool,
        launch: impl FnOnce(String) -> std::io::Result<()>,
    ) -> String {
        match self.decide(origin, url, user_gesture, Instant::now()) {
            ExternalLink::Open { shown, .. } if self.log_only => {
                format!("external link opened url={shown} via={via} launched=false")
            },
            ExternalLink::Open { url, shown } => match launch(url) {
                Ok(()) => format!("external link opened url={shown} via={via} launched=true"),
                Err(error) => {
                    format!("external link open failed url={shown} via={via} error={error}")
                },
            },
            ExternalLink::Refuse { shown, reason } => {
                format!("external link refused url={shown} via={via} reason={reason}")
            },
        }
    }

    /// Decides a refused popup or vetoed navigation to `url` of the game on `origin`.
    pub(crate) fn decide(
        &mut self,
        origin: &str,
        url: &str,
        user_gesture: bool,
        now: Instant,
    ) -> ExternalLink {
        let refuse = |shown: String, reason| ExternalLink::Refuse { shown, reason };
        let Ok(parsed) = url::Url::parse(url) else {
            return refuse("<invalid>".to_owned(), "invalid-url");
        };
        let shown = shown(&parsed);
        match parsed.scheme() {
            "http" | "https" if parsed.host_str().is_some_and(|h| !h.is_empty()) => {},
            "mailto" => {},
            _ => return refuse(shown, "scheme"),
        }
        if on_origin(origin, url) {
            return refuse(shown, "game-origin");
        }
        if !user_gesture {
            return refuse(shown, "no-user-gesture");
        }
        if self
            .last_open
            .is_some_and(|last| now.saturating_duration_since(last) < EXTERNAL_OPEN_INTERVAL)
        {
            return refuse(shown, "rate-limited");
        }
        self.last_open = Some(now);
        ExternalLink::Open {
            url: parsed.into(),
            shown,
        }
    }
}

/// Scheme and host only: `https://example.com`, `http://example.com:8080`, `mailto:`.
fn shown(url: &url::Url) -> String {
    match url.host_str() {
        Some(host) => match url.port() {
            Some(port) => format!("{}://{host}:{port}", url.scheme()),
            None => format!("{}://{host}", url.scheme()),
        },
        None => format!("{}:", url.scheme()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "http://127.0.0.1:47810";

    #[test]
    fn main_frame_policy_keeps_one_document() {
        let allows = main_frame_policy(ORIGIN);
        // Bootstrap, then the game and same-origin navigations (links, reloads, routes).
        assert!(allows("about:blank"));
        assert!(allows("http://127.0.0.1:47810/"));
        assert!(allows("http://127.0.0.1:47810/index.html?x#y"));
        // about:blank after the game started would replace the game document.
        assert!(!allows("about:blank"));
        for url in [
            "https://example.com/",
            "http://127.0.0.1:4781/",
            "http://127.0.0.1:478100/",
            "http://127.0.0.1:47810@evil.example/",
            "http://localhost:47810/",
            "file:///C:/Windows/",
            "javascript:alert(1)",
            "data:text/html,x",
            "chrome://version/",
        ] {
            assert!(!allows(url), "{url}");
        }
        // Each policy has its own bootstrap state.
        assert!(main_frame_policy(ORIGIN)("about:blank"));
    }

    #[test]
    fn external_links_need_a_web_scheme_and_a_gesture() {
        let now = Instant::now();
        let open = |url: &str| {
            let mut links = ExternalLinks::default();
            links.decide(ORIGIN, url, true, now)
        };
        assert_eq!(
            open("https://user:pw@Example.com/secret/path?token=1#f"),
            ExternalLink::Open {
                url: "https://user:pw@example.com/secret/path?token=1#f".to_owned(),
                shown: "https://example.com".to_owned(),
            }
        );
        assert_eq!(
            open("http://example.com:8080/a"),
            ExternalLink::Open {
                url: "http://example.com:8080/a".to_owned(),
                shown: "http://example.com:8080".to_owned(),
            }
        );
        assert_eq!(
            open("mailto:someone@example.com?subject=hi"),
            ExternalLink::Open {
                url: "mailto:someone@example.com?subject=hi".to_owned(),
                shown: "mailto:".to_owned(),
            }
        );
        for (url, shown, reason) in [
            ("file:///etc/passwd", "file:", "scheme"),
            ("javascript:alert(1)", "javascript:", "scheme"),
            ("data:text/html,x", "data:", "scheme"),
            ("steam://run/1", "steam://run", "scheme"),
            ("about:blank", "about:", "scheme"),
            ("not a url", "<invalid>", "invalid-url"),
            (
                "http://127.0.0.1:47810/other",
                "http://127.0.0.1:47810",
                "game-origin",
            ),
        ] {
            assert_eq!(
                open(url),
                ExternalLink::Refuse {
                    shown: shown.to_owned(),
                    reason
                },
                "{url}"
            );
        }
        let mut links = ExternalLinks::default();
        assert_eq!(
            links.decide(ORIGIN, "https://example.com/", false, now),
            ExternalLink::Refuse {
                shown: "https://example.com".to_owned(),
                reason: "no-user-gesture"
            }
        );
        // A refusal does not start the rate-limit window.
        assert!(matches!(
            links.decide(ORIGIN, "https://example.com/", true, now),
            ExternalLink::Open { .. }
        ));
    }

    #[test]
    fn handle_launches_only_accepted_links() {
        let mut links = ExternalLinks::default();
        let mut launched = Vec::new();
        let line = links.handle(ORIGIN, "popup", "https://example.com/p?q", true, |u| {
            launched.push(u);
            Ok(())
        });
        assert_eq!(
            line,
            "external link opened url=https://example.com via=popup launched=true"
        );
        assert_eq!(launched, ["https://example.com/p?q"]);
        let line = links.handle(ORIGIN, "navigation", "file:///x", true, |_| unreachable!());
        assert_eq!(
            line,
            "external link refused url=file: via=navigation reason=scheme"
        );
        // Log-only (CI): accepted, never launched.
        let mut links = ExternalLinks {
            log_only: true,
            ..ExternalLinks::default()
        };
        let line = links.handle(
            ORIGIN,
            "navigation",
            "https://example.com/",
            true,
            |_| unreachable!(),
        );
        assert_eq!(
            line,
            "external link opened url=https://example.com via=navigation launched=false"
        );
        // A launch error is reported, not hidden.
        let mut links = ExternalLinks::default();
        let line = links.handle(ORIGIN, "popup", "https://example.com/", true, |_| {
            Err(std::io::Error::other("no browser"))
        });
        assert_eq!(
            line,
            "external link open failed url=https://example.com via=popup error=no browser"
        );
    }

    #[test]
    fn external_opens_are_rate_limited() {
        let start = Instant::now();
        let mut links = ExternalLinks::default();
        let at = |links: &mut ExternalLinks, ms| {
            links.decide(
                ORIGIN,
                "https://example.com/",
                true,
                start + Duration::from_millis(ms),
            )
        };
        assert!(matches!(at(&mut links, 0), ExternalLink::Open { .. }));
        assert_eq!(
            at(&mut links, 999),
            ExternalLink::Refuse {
                shown: "https://example.com".to_owned(),
                reason: "rate-limited"
            }
        );
        assert!(matches!(at(&mut links, 1000), ExternalLink::Open { .. }));
        assert!(matches!(at(&mut links, 1500), ExternalLink::Refuse { .. }));
        assert!(matches!(at(&mut links, 2000), ExternalLink::Open { .. }));
    }
}
