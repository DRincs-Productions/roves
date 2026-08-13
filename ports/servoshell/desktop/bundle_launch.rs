/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Resolves the CLI args a bundled build should launch with, in-process,
//! instead of relying on a separate launcher executable to compute and pass
//! them in. `mach bundle` (see `python/servo/post_build_commands.py`) writes
//! a small `launch.json` next to the shipped binary; this reads it back so
//! the shipped bundle can contain exactly one executable (`play`/`play.exe`/
//! `Roves`) instead of a thin launcher plus a separately-named engine
//! binary. See CUSTOMIZATIONS.md's single-executable-bundle entry.
//!
//! `launch.json` shape (all paths relative to the config file's own
//! directory):
//! ```json
//! {
//!   "content_dir": "dist",   // present only for --content-compress=auto builds
//!   "url": "dist/index.html", // used only when content_dir is absent
//!   "args": ["--window-size", "1280x720"]
//! }
//! ```

use std::env;
use std::path::{Path, PathBuf};

use roves_content_packer::extract;

const LAUNCH_CONFIG_FILE: &str = "launch.json";

/// Result of [`resolve_bundled_launch_args`]: the args to launch with,
/// exactly as if they'd been passed on the command line (a positional URL
/// followed by flags), plus — for a packed-content build — the boot
/// extraction that still needs to actually run before that URL's files
/// exist on disk. Resolving the args never blocks on extraction itself (see
/// `resolve_packed_content_url`); the caller (`App`, see `app.rs`) is what
/// runs `pending_boot_extraction` in the background and shows a splash
/// while it does, instead of extraction happening synchronously before the
/// window even exists.
pub(crate) struct BundledLaunch {
    pub(crate) args: Vec<String>,
    pub(crate) pending_boot_extraction: Option<extract::ExtractOptions>,
}

/// Cheaply figures out which game (if any) this is a bundled launch of,
/// *before* anything else has run — specifically, before
/// `resolve_bundled_launch_args` below, or logging (`desktop/logging.rs`)
/// is even installed — so `cli::main` can pick the right per-game log
/// directory (`extract::game_data_dir`) and have every log line from the
/// rest of startup, including whatever `resolve_bundled_launch_args` itself
/// logs, actually land somewhere instead of being silently dropped (no
/// logger exists yet at that point otherwise). Deliberately does not log
/// anything itself, for the same reason, and deliberately re-reads
/// `launch.json`/`manifest.json` rather than sharing state with
/// `resolve_bundled_launch_args` — the two run at genuinely different
/// times (this one first), and duplicating a couple of small, fast file
/// reads is simpler and safer than threading a shared result across that
/// gap. Returns `None` for every case that isn't "a packed-content bundled
/// launch with a named manifest" (no `launch.json`, a `url`-based bundle, a
/// dev/explicit-argv invocation, or a manifest predating the `name` field)
/// — callers should treat that the same as [`extract::game_data_dir`]'s own
/// `None` fallback.
pub(crate) fn peek_game_name_for_logging() -> Option<String> {
    if env::args().nth(1).is_some() {
        return None;
    }
    let exe_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    let text = std::fs::read_to_string(exe_dir.join(LAUNCH_CONFIG_FILE)).ok()?;
    let config: serde_json::Value = serde_json::from_str(&text).ok()?;
    let content_rel_dir = config.get("content_dir")?.as_str()?;
    let content_dir = content_root(&exe_dir).join(content_rel_dir);
    extract::load_manifest(&content_dir).ok()?.name
}

/// Returns the args to launch with, or `None` to fall back to the process's
/// real `argv` unchanged.
///
/// Deliberately only consulted when the real `argv` is completely empty —
/// this is what a genuine double-click launch looks like, and it's the only
/// case it's safe to override: an explicit invocation (a developer running
/// the shipped binary from a terminal, a Steam launch-options override, and
/// critically Servo's own multiprocess content-process children
/// re-executing *themselves* with `--content-process <token>` in argv, see
/// `ports/servoshell/prefs.rs`'s `content_process` field) must always be
/// left untouched, or a bundled multiprocess build would have every child
/// process silently discard its real startup args and try to open the game
/// window all over again.
pub(crate) fn resolve_bundled_launch_args() -> Option<BundledLaunch> {
    if env::args().nth(1).is_some() {
        return None;
    }

    let exe_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    let config_path = exe_dir.join(LAUNCH_CONFIG_FILE);
    // Absence is the common case: a plain `./mach run`/dev invocation never
    // has a launch.json sitting next to it. Not an error.
    let text = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&text)
        .inspect_err(|e| log::error!("parsing {config_path:?}: {e}"))
        .ok()?;

    let extra_args: Vec<String> = config
        .get("args")
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let (url, pending_boot_extraction) =
        if let Some(content_rel_dir) = config.get("content_dir").and_then(|v| v.as_str()) {
            let (url, opts) = resolve_packed_content_url(&exe_dir, content_rel_dir)?;
            (url, Some(opts))
        } else {
            let rel_url = config.get("url").and_then(|v| v.as_str())?;
            (exe_dir.join(rel_url).to_string_lossy().into_owned(), None)
        };

    let mut args = vec![url];
    args.extend(extra_args);
    Some(BundledLaunch {
        args,
        pending_boot_extraction,
    })
}

/// macOS app bundles keep the executable in `Contents/MacOS/` and bundled
/// resources — here, packed game content — in the sibling
/// `Contents/Resources/`, the standard layout `_bundle_macos` (see
/// `python/servo/post_build_commands.py`) already places content into.
/// Windows/Linux keep content flat, next to the binary.
#[cfg(target_os = "macos")]
fn content_root(exe_dir: &Path) -> PathBuf {
    exe_dir.join("..").join("Resources")
}

#[cfg(not(target_os = "macos"))]
fn content_root(exe_dir: &Path) -> PathBuf {
    exe_dir.to_path_buf()
}

/// Resolves the absolute `file:` URL a packed-content build will open, and
/// the [`extract::ExtractOptions`] that must still be run to actually make
/// that URL's files exist. Deliberately does **not** extract anything
/// itself — only `extract::resolve_dest` (a path/hash computation) and a
/// `manifest.json` read, both fast — so this never blocks the caller. The
/// actual (slow) decompression is the caller's job: `App::init` (`app.rs`)
/// runs it on a background thread while showing a boot splash, and
/// `desktop/protocols/file.rs` separately extracts individual lazy packs
/// on demand once the engine is running. Any failure (missing/corrupt
/// content) is logged and treated as "no bundled launch config", leaving
/// the engine to fall back to its normal preference-based default rather
/// than hard-crashing on a broken install.
fn resolve_packed_content_url(
    exe_dir: &Path,
    content_rel_dir: &str,
) -> Option<(String, extract::ExtractOptions)> {
    let content_dir = content_root(exe_dir).join(content_rel_dir);
    let manifest = extract::load_manifest(&content_dir)
        .inspect_err(|e| log::error!("loading packed-content manifest at {content_dir:?}: {e}"))
        .ok()?;
    let (content_dir, dest) = extract::resolve_dest(&content_dir, None, manifest.name.as_deref())
        .inspect_err(|e| log::error!("resolving boot content destination: {e}"))
        .ok()?;
    let url = dest
        .join(&manifest.entry_html)
        .to_string_lossy()
        .into_owned();
    let opts = extract::ExtractOptions {
        content_dir,
        dest: Some(dest),
        force: false,
    };
    Some((url, opts))
}
