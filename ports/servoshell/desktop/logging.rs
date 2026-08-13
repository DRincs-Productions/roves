/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Installs a single process-wide file logger as early in `cli::main` as
//! possible — before `panic::set_hook`, before `resolve_bundled_launch_args`,
//! before anything else that can fail silently. This exists because, until
//! now, nothing installed a `log` backend at all until deep inside
//! `App::finish_init` (`Servo::setup_logging`, called only once a `Servo`
//! instance exists), so every `log::error!`/`warn!` call before that point —
//! including inside `bundle_launch::resolve_bundled_launch_args` itself,
//! exactly where a broken/missing packed-content bundle would report the
//! problem — was silently discarded. Combined with `ports/servoshell/main.rs`
//! setting `#![windows_subsystem = "windows"]` (no console unless launched
//! from one), a double-clicked `play.exe` that failed early looked like
//! nothing happened at all, with no way to tell why.
//!
//! `log::set_boxed_logger` only ever succeeds once per process — see
//! `components/servo/servo.rs`'s `setup_logging`/`set_logger`, both patched
//! (`patches/servo-v0.4.0/0030-...patch`) to no-op instead of panicking on
//! the second call this now causes, since this module's logger always
//! installs first. The trade-off: `FromEmbedderLogger`'s
//! constellation-forwarding (an embedder-side crash/warning-UI hook) never
//! runs — this fork has no such UI to forward to (see CUSTOMIZATIONS.md's
//! earliest entries removing the toolbar/tabs entirely), so that's a
//! deliberate no-op, not a silently swallowed feature.

use std::fs::File;
use std::path::Path;

/// Filename of the on-disk log, truncated fresh on every launch (`File::create`)
/// so it never grows unbounded across repeated runs — old runs' output isn't
/// meant to be kept, just this run's.
const LOG_FILE_NAME: &str = "roves.log";

/// Installs the file logger, rooted at `log_dir` (see
/// `roves_content_packer::extract::game_data_dir` — a sibling of the
/// extraction cache, not inside the game's own content directory). Returns
/// the log file's path on success, purely for callers that want to mention
/// it in a startup message; installation failing (directory/file couldn't
/// be created — e.g. a read-only install location) is not fatal and is not
/// itself logged anywhere (nothing to log it *to*) — `Servo::setup_logging`
/// simply ends up being the first successful `log::set_boxed_logger` call
/// instead, same as before this module existed.
///
/// Captures everything: normal `env_logger`-style filtering, defaulting to
/// `info` (not `env_logger`'s own default of `error`-only) since a private
/// log file, unlike a terminal, isn't noisy for the user — override with
/// `RUST_LOG` as usual. Because the game's own `console.log`/`warn`/`error`
/// already route through `log::log!` (see `headed_window.rs`/
/// `headless_window.rs`'s `show_console_message`), and every panic already
/// routes through `log::error!` (`panic_hook.rs`), a single logger installed
/// here is enough to capture Roves/Servo's own logging, the game's console
/// output, and startup panics, all in one file.
pub(crate) fn init(log_dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::create_dir_all(log_dir).ok()?;
    let log_path = log_dir.join(LOG_FILE_NAME);
    let file = File::create(&log_path).ok()?;

    let env = env_logger::Env::default().default_filter_or("info");
    let mut builder = env_logger::Builder::from_env(env);
    builder.target(env_logger::Target::Pipe(Box::new(file)));
    builder.format_timestamp_millis();
    let logger = builder.build();
    let filter = logger.filter();

    log::set_boxed_logger(Box::new(logger)).ok()?;
    log::set_max_level(filter);
    // First line in every fresh log, so anyone who finds the file
    // immediately knows what it is and where it's rooted — helpful given
    // this whole module exists for the case where nothing else visibly
    // happened.
    log::info!("Roves logging started, writing to {log_path:?}");
    Some(log_path)
}
