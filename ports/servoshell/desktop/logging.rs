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
use std::sync::{Mutex, OnceLock};

/// Filename of the on-disk log, truncated fresh on every launch (`File::create`)
/// so it never grows unbounded across repeated runs — old runs' output isn't
/// meant to be kept, just this run's.
const LOG_FILE_NAME: &str = "roves.log";

/// Set the first time the game's own top-level document fails to fetch a `<script>` it
/// needs to actually run (a classic or module script) — checked every repaint by
/// `headed_window.rs`, once the page reports itself loaded, so a black canvas (nothing
/// ever painted, because the page's own JS never ran) shows a visible message instead of
/// silently doing nothing. Sticky for the rest of the session once set, deliberately: the
/// script already failed once, there's nothing to retry.
static CONTENT_LOAD_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// The message set by [`CONTENT_LOAD_ERROR`], if a content-load error was ever seen —
/// see `headed_window.rs`'s `handle_winit_window_event`.
pub(crate) fn content_load_error() -> Option<String> {
    CONTENT_LOAD_ERROR.get()?.lock().unwrap().clone()
}

/// Matches the exact two upstream Servo log call sites that mean "the page's own script
/// failed to load" — `script::script_module`'s `Fetching module script failed` and
/// `script::dom::html::htmlscriptelement`'s `Fetching classic script failed` (see
/// `components/script/script_module.rs`/`components/script/dom/htmlscriptelement.rs`).
/// Deliberately narrow: an ordinary page can 404 an image or a non-critical fetch without
/// the whole app being broken, but a `<script>` the page itself needed to run failing is
/// exactly the "renders nothing, looks like it hung" failure mode this exists to surface.
fn note_potential_content_load_error(record: &log::Record) {
    if record.level() != log::Level::Error {
        return;
    }
    if !matches!(
        record.target(),
        "script::script_module" | "script::dom::html::htmlscriptelement"
    ) {
        return;
    }
    let slot = CONTENT_LOAD_ERROR.get_or_init(|| Mutex::new(None));
    let mut slot = slot.lock().unwrap();
    if slot.is_none() {
        *slot = Some(record.args().to_string());
    }
}

/// Wraps `env_logger`'s own [`env_logger::Logger`] to additionally watch for the narrow
/// class of Servo-internal error logs handled by [`note_potential_content_load_error`] —
/// everything else about logging (destination, level filtering, formatting) is
/// unchanged, `env_logger`'s `Logger` still does all of it; this only adds a side effect
/// on top, purely so `headed_window.rs` has something to poll without needing its own
/// hook into Servo's script-loading internals.
struct RovesLogger {
    inner: env_logger::Logger,
}

impl log::Log for RovesLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        note_potential_content_load_error(record);
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

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
    let logger = RovesLogger { inner: logger };

    log::set_boxed_logger(Box::new(logger)).ok()?;
    log::set_max_level(filter);
    // First line in every fresh log, so anyone who finds the file
    // immediately knows what it is and where it's rooted — helpful given
    // this whole module exists for the case where nothing else visibly
    // happened.
    log::info!("Roves logging started, writing to {log_path:?}");
    Some(log_path)
}
