/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{env, panic};

use crate::desktop::app::App;
use crate::desktop::bundle_launch::resolve_bundled_launch_args;
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::panic_hook;
use crate::prefs::{ArgumentParsingResult, parse_command_line_arguments};

pub fn main() {
    crate::crash_handler::install();
    crate::init_crypto();

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
    let (args, pending_boot_extraction) = match resolve_bundled_launch_args() {
        Some(bundled) => (bundled.args, bundled.pending_boot_extraction),
        None => (env::args().skip(1).collect(), None),
    };
    let (opts, preferences, servoshell_preferences) = match parse_command_line_arguments(&*args) {
        ArgumentParsingResult::ContentProcess(token) => return servo::run_content_process(token),
        ArgumentParsingResult::ChromeProcess(opts, preferences, servoshell_preferences) => {
            (opts, preferences, servoshell_preferences)
        },
        ArgumentParsingResult::Exit => {
            std::process::exit(0);
        },
        ArgumentParsingResult::ErrorParsing => {
            std::process::exit(1);
        },
    };

    crate::init_tracing(servoshell_preferences.tracing_filter.as_deref());

    let clean_shutdown = servoshell_preferences.clean_shutdown;
    let event_loop = match servoshell_preferences.headless {
        true => ServoShellEventLoop::headless(),
        false => ServoShellEventLoop::headed(),
    };

    {
        let mut app =
            App::new(opts, preferences, servoshell_preferences, &event_loop, pending_boot_extraction);
        event_loop.run_app(&mut app);
    }

    crate::platform::deinit(clean_shutdown)
}
