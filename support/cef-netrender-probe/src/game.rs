//! Opt-in real-game mode (`--roves-game-dir=<built game>`): the probe opens an unmodified
//! game build from a per-run loopback origin instead of the deterministic fixture, and
//! forwards SDL3 mouse, wheel and keyboard input to the off-screen browser through
//! welding's `CefSurfaceProducer` input API (`input.rs`, shared with the macOS and Linux
//! players; Windows key codes in `keys.rs`). Rendering, composition (`--roves-compose`),
//! sandbox and shutdown are the probe's unchanged path. The host API (`host_api`, README
//! "Host API") is served next to the game files on the same origin.

use std::path::{Path, PathBuf};

/// The players' shared SDL3 -> welding input path (`input.rs`), with the Windows key codes
/// of `keys.rs`.
pub(crate) use crate::input::Input;

/// The stage-1 window probe's loopback server (axum + tower-http `ServeDir` on 127.0.0.1),
/// reused unchanged so both probes serve the game the same way.
#[path = "../../cef-window-probe/src/server.rs"]
pub(crate) mod server;
pub(crate) use server::Server;
/// The players' shared host API (`/__roves/<module>/<cmd>` on the same loopback origin;
/// README "Host API").
#[path = "../../cef-window-probe/src/host_api.rs"]
pub(crate) mod host_api;

/// Console lines forwarded to the evidence log before only counting them.
const CONSOLE_LOG_CAP: u32 = 500;
/// Longest console message kept per line (characters).
const CONSOLE_MESSAGE_MAX: usize = 512;

pub(crate) struct GameOptions {
    /// Absolute game build directory (contains `index.html`).
    pub(crate) dir: PathBuf,
    /// `--roves-port` (0 = a free port per run; a fixed port keeps the origin, and therefore
    /// the page's storage in a reused `--roves-cache-dir`, stable across runs).
    pub(crate) port: u16,
}

impl GameOptions {
    pub(crate) fn new(dir: &str, port: u16) -> Result<Self, String> {
        // std::path::absolute, not canonicalize: no `\\?\` verbatim prefix (as the window probe).
        let dir = std::path::absolute(dir).map_err(|e| format!("--roves-game-dir={dir}: {e}"))?;
        if !dir.join("index.html").is_file() {
            return Err(format!("{} has no index.html", dir.display()));
        }
        Ok(GameOptions { dir, port })
    }
}

/// The per-producer persistent profile inside CEF's root cache (welding requires it to be
/// absolute and inside `CefRuntimeConfig::cache_path`). A fresh default `--roves-cache-dir`
/// gives every run an empty profile; reusing the directory and the port keeps saves.
pub(crate) fn profile_dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join("game-profile")
}

/// Console forwarding with a line cap.
#[derive(Default)]
pub(crate) struct Console {
    pub(crate) seen: u32,
}

impl Console {
    pub(crate) fn line(
        &mut self,
        level: i32,
        message: &str,
        source: &str,
        line: i32,
    ) -> Option<String> {
        self.seen += 1;
        if self.seen > CONSOLE_LOG_CAP {
            return None;
        }
        let message: String = message.chars().take(CONSOLE_MESSAGE_MAX).collect();
        Some(format!(
            "console level={level} source={source}:{line} message={message:?}"
        ))
    }
}
