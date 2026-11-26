//! Packaged game, persistent per-install profile (README "Saves and the player profile"):
//! the macOS counterpart of the Windows player's `PlayerProfile`
//! (`support/cef-windows-player/src/app.rs`). The rules (file names, profile name, port
//! parsing) are the Windows player's own `game_rules.rs`, compiled here through `#[path]`;
//! only the platform glue lives in this file.
//!
//! Layout of `<root>` (default `~/Library/Application Support/Roves/Player/<name>`):
//! - `player.lock`: held with `flock(LOCK_EX | LOCK_NB)` (`File::try_lock`) for the whole
//!   browser-process run. One instance per profile; the kernel drops the lock when the
//!   process ends, crash included, so a stale file never blocks the next launch (Chromium's
//!   macOS `ReplaceOldSingletonLock` relies on the same `flock` property).
//! - `origin-port`: the loopback port chosen by the OS on the first launch. Page storage is
//!   keyed by origin, so every later launch must bind the same port; a taken port or a
//!   damaged file ends the run instead of silently opening an empty origin.
//! - `cef/`: CEF's root cache, with welding's per-producer persistent context in
//!   `cef/game-profile` (welding requires it absolute and inside the root cache).

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::game_rules::{
    PLAYER_CEF_DIR, PLAYER_LOCK_FILE, PLAYER_ORIGIN_PORT_FILE, parse_origin_port,
    player_profile_name,
};

/// welding's per-producer context inside the root cache (the Windows player's
/// `game::profile_dir`).
pub(crate) const GAME_PROFILE_DIR: &str = "game-profile";

/// Exit code of a packaged launch that found its profile in use by another instance, or the
/// stored origin port taken by another program (same value as on Windows).
pub(crate) const EXIT_CONTENTION: i32 = 6;

/// Switch of the CI/test mode: a per-run cache removed at exit, nothing persists (the Windows
/// player's switch of the same name).
pub(crate) const ISOLATED_PROFILE_SWITCH: &str = "--roves-isolated-profile";

/// CEF's root cache of a profile root.
pub(crate) fn cef_dir(root: &Path) -> PathBuf {
    root.join(PLAYER_CEF_DIR)
}

/// Default profile root: `$HOME/Library/Application Support/Roves/Player/<name>`, where
/// `<name>` is the Windows rule (`player_profile_name`) applied to the executable stem and
/// the canonical `.app` path. Chromium's default user-data directory has the same parent
/// (`base::DIR_APP_DATA`, `chrome/common/chrome_paths_mac.mm`); unlike Chromium the name is
/// per install, so two packaged games never share storage.
pub(crate) fn default_root(exe: &Path, app: &Path) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .ok_or("HOME is not set")?;
    let app =
        std::fs::canonicalize(app).map_err(|e| format!("app bundle {}: {e}", app.display()))?;
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Roves")
        .join("Player")
        .join(player_profile_name(&stem, &app.to_string_lossy())))
}

/// Gatekeeper's App Translocation runs a quarantined app that was not moved by the user
/// from a random read-only path; the install-path rule then gives every such launch a new
/// profile. Reported, not refused (README "Open risks").
pub(crate) fn translocated(app: &Path) -> bool {
    std::fs::canonicalize(app)
        .map(|p| p.to_string_lossy().contains("/AppTranslocation/"))
        .unwrap_or(false)
}

/// The held profile lock and the stored origin port.
pub(crate) struct PlayerProfile {
    pub(crate) root: PathBuf,
    _lock: File,
    /// `None` on the first launch: the port the OS picks is stored after the bind.
    pub(crate) stored_port: Option<u16>,
}

pub(crate) enum ProfileError {
    /// Another process holds the profile lock.
    Busy,
    Failed(String),
}

impl PlayerProfile {
    pub(crate) fn open(root: &Path) -> Result<Self, ProfileError> {
        let failed = |what: &str, e: std::io::Error| ProfileError::Failed(format!("{what}: {e}"));
        std::fs::create_dir_all(root).map_err(|e| failed("create the profile directory", e))?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(PLAYER_LOCK_FILE))
            .map_err(|e| failed("open the profile lock", e))?;
        match lock.try_lock() {
            Ok(()) => {},
            Err(std::fs::TryLockError::WouldBlock) => return Err(ProfileError::Busy),
            Err(std::fs::TryLockError::Error(e)) => return Err(failed("lock the profile", e)),
        }
        let port_file = root.join(PLAYER_ORIGIN_PORT_FILE);
        let stored_port = match std::fs::read_to_string(&port_file) {
            Ok(text) => Some(parse_origin_port(&text).ok_or_else(|| {
                ProfileError::Failed(format!(
                    "{} is damaged (expected a port number); the saves belong to that port",
                    port_file.display()
                ))
            })?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(failed("read the origin port", e)),
        };
        Ok(PlayerProfile {
            root: root.to_path_buf(),
            _lock: lock,
            stored_port,
        })
    }

    /// Writes the origin port through a temporary file and a rename (atomic replace), so an
    /// interrupted first launch never leaves a partial port file.
    pub(crate) fn store_port(&self, port: u16) -> Result<(), String> {
        let target = self.root.join(PLAYER_ORIGIN_PORT_FILE);
        let temporary = self.root.join(format!("{PLAYER_ORIGIN_PORT_FILE}.tmp"));
        std::fs::write(&temporary, format!("{port}\n"))
            .and_then(|()| std::fs::rename(&temporary, &target))
            .map_err(|e| format!("{}: {e}", target.display()))
    }
}
