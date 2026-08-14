/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{env, panic};

use crate::desktop::app::App;
use crate::desktop::bundle_launch::{peek_game_name_for_logging, resolve_bundled_launch_args};
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::desktop::logging;
use crate::panic_hook;
use crate::prefs::{ArgumentParsingResult, parse_command_line_arguments};

pub fn main() {
    crate::crash_handler::install();
    crate::init_crypto();

    // Installed before anything else that could fail — including the panic
    // hook and `resolve_bundled_launch_args` below — so a failure that
    // early is actually diagnosable instead of vanishing silently (this app
    // is `#![windows_subsystem = "windows"]`, so there's no console to see
    // stderr in even if something did print to it). See `logging.rs`.
    //
    // Gated on the same "real argv is empty" check `resolve_bundled_launch_args`
    // itself uses (see `peek_game_name_for_logging`'s doc comment) — critically,
    // this excludes Servo's own multiprocess content-process children, which
    // re-exec *themselves* with `--content-process <token>` in argv. Each one
    // installing its own truncating file logger would race every other
    // process (including the main one) writing to that same file, each
    // wiping out whatever the others had already logged. Content processes
    // fall through to `Servo::setup_logging`'s content-process counterpart
    // (`set_logger`) exactly as before this module existed — unchanged, not
    // a regression, since the whole point of this early file logger is
    // diagnosing a launch that never gets that far in the first place.
    if env::args().nth(1).is_none() {
        let log_dir =
            roves_content_packer::extract::game_data_dir(peek_game_name_for_logging().as_deref());
        logging::init(&log_dir);
    }

    // TODO: once log-panics is released, can this be replaced by
    // log_panics::init()?
    panic::set_hook(Box::new(panic_hook::panic_hook));

    // A bundled build (see `python/servo/post_build_commands.py`'s `bundle`
    // command and CUSTOMIZATIONS.md) resolves its own launch args from a
    // `launch.json` sitting next to the running executable, in place of the
    // separate launcher process a bundle used to spawn. Falls back to the
    // real argv (skipping the binary name) for every other invocation.
    // `pending_boot_extraction` (packed-content builds only) is threaded
    // through to `App::new` unresolved — `resolve_bundled_launch_args`
    // never blocks on it; see `bundle_launch.rs` and `App::init`'s boot
    // splash handling.
    let (args, pending_boot_extraction, is_bundled_launch) = match resolve_bundled_launch_args() {
        Some(bundled) => (bundled.args, bundled.pending_boot_extraction, true),
        None => (env::args().skip(1).collect(), None, false),
    };
    // Startup milestones below: this app has no console on a double-clicked
    // Windows build (`#![windows_subsystem = "windows"]`), so a launch that
    // dies with no further output — a hang, or a hard native crash that
    // bypasses `panic_hook.rs` entirely (a GPU/driver issue in window/GL
    // context creation, a missing DLL, ...) — leaves nothing to go on
    // besides "which of these was the last one logged". Not meant to stay
    // this granular forever; remove once real-machine testing has actually
    // localized this class of failure.
    log::info!("resolved launch args: {args:?}");
    let (opts, preferences, servoshell_preferences) = match parse_command_line_arguments(&*args) {
        ArgumentParsingResult::ContentProcess(token) => return servo::run_content_process(token),
        ArgumentParsingResult::ChromeProcess(opts, preferences, servoshell_preferences) => {
            (opts, preferences, servoshell_preferences)
        },
        ArgumentParsingResult::Exit => {
            std::process::exit(0);
        },
        // `launch.json`'s "args" (see `bundle_launch.rs`) are build-tool-
        // generated config, not interactive user input — unlike a real CLI
        // typo, there is no user present to see the parse error and correct
        // it. A stray build-tool flag leaking in there (see
        // CUSTOMIZATIONS.md's launch-args-sanitization entry for a real
        // incident of exactly this) must never be able to silently kill
        // every future launch of an otherwise-working bundle (no window, no
        // console, on a double-clicked Windows build). Retry once with just
        // the content URL, dropping every extra arg, instead of exiting —
        // worst case is losing that launch.json's window-size/title/etc.
        // customization, not "the game never starts". Real (non-bundled)
        // invocations are unaffected: a developer's own CLI typo still
        // hard-errors exactly as before.
        ArgumentParsingResult::ErrorParsing if is_bundled_launch && args.len() > 1 => {
            log::error!(
                "bundled launch.json's args failed to parse ({args:?}); retrying with just the \
                 content URL, dropping every extra arg"
            );
            match parse_command_line_arguments(&args[..1]) {
                ArgumentParsingResult::ChromeProcess(opts, preferences, servoshell_preferences) => {
                    (opts, preferences, servoshell_preferences)
                },
                _ => std::process::exit(1),
            }
        },
        ArgumentParsingResult::ErrorParsing => {
            std::process::exit(1);
        },
    };
    log::info!("parsed command line arguments");

    crate::init_tracing(servoshell_preferences.tracing_filter.as_deref());

    let clean_shutdown = servoshell_preferences.clean_shutdown;
    let event_loop = match servoshell_preferences.headless {
        true => ServoShellEventLoop::headless(),
        false => ServoShellEventLoop::headed(),
    };
    log::info!(
        "created event loop, headless={}",
        servoshell_preferences.headless
    );

    {
        let mut app = App::new(
            opts,
            preferences,
            servoshell_preferences,
            &event_loop,
            pending_boot_extraction,
        );
        log::info!("running event loop");
        event_loop.run_app(&mut app);
    }

    crate::platform::deinit(clean_shutdown)
}
