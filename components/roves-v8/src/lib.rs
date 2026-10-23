/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The process-wide V8 setup roves-js builds on: platform initialization (flags, JIT-less,
//! the custom foreground-task platform) and the V8 APIs rusty_v8 does not bind (`incumbent`).
//! roves-js owns isolates, contexts and the JSAPI surface (see ../../docs/V8_MIGRATION.md).

pub mod foreground;
pub mod global_proxy;
pub mod incumbent;

use std::sync::Once;

static V8_PLATFORM_INIT: Once = Once::new();

/// Initializes the process-wide V8 platform once, before any isolate is created. Idempotent.
pub fn initialize_engine() {
    V8_PLATFORM_INIT.call_once(initialize_platform);
}

/// Like [`initialize_engine`], also applying `flags` (for example `--expose-gc` in another
/// crate's tests) if the platform is not initialized yet; later calls cannot change flags.
pub fn initialize_engine_with_flags(flags: &str) {
    V8_PLATFORM_INIT.call_once(|| {
        v8::V8::set_flags_from_string(flags);
        initialize_platform();
    });
}

fn initialize_platform() {
    {
        // roves-js scans its root stack when cppgc traces its root set, like SpiderMonkey
        // scans rooted values at GC time. Without incremental/concurrent marking that scan
        // happens in one atomic pause and sees every root (correctness first; revisit with
        // write barriers on root updates).
        v8::V8::set_flags_from_string("--no-incremental-marking --no-concurrent-marking --no-parallel-marking");
        // See ../../docs/V8_MIGRATION.md's "Console-oriented requirement: JIT-less" — a
        // first-class supported configuration from the start. This must run before
        // `initialize_platform`/`initialize` below, not after.
        #[cfg(feature = "jitless")]
        v8::V8::set_flags_from_string("--jitless");

        // `ROVES_V8_FLAGS` appends V8 flags (e.g. `--max-semi-space-size=32` to tune the young
        // generation) for measurements on a shipped build. A diagnostic switch, like
        // servoshell's `ROVES_EXTRA_ARGS`, not a player option.
        if let Ok(flags) = std::env::var("ROVES_V8_FLAGS") {
            v8::V8::set_flags_from_string(&flags);
        }

        // Foreground tasks go to the handler of their isolate (see `foreground`).
        let platform = v8::new_custom_platform(0, false, false, foreground::Platform).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    }
}
