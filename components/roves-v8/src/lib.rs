/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The process-wide V8 setup roves-js builds on: the Roves V8 flags (GC, JIT-less), the
//! custom foreground-task platform, and the public V8 APIs rusty_v8 150 does not bind
//! (`incumbent`, `codegen`, through `src/shim.cc`). deno_core initializes V8 itself, once:
//! roves-js applies [`set_flags`] and hands [`platform`] to `JsRuntime::init_platform`.
//! roves-js owns isolates (through deno_core), contexts and the JSAPI surface (see
//! ../../docs/V8_MIGRATION.md).

pub mod codegen;
pub mod compat;
pub mod foreground;
pub mod incumbent;

/// Applies the Roves V8 flags. Must run before V8 is initialized (deno_core's
/// `JsRuntime::init_platform` adds its own base flags after these; none negates them).
/// `extra` holds further flags (e.g. `--expose-gc` in tests).
pub fn set_flags(extra: &str) {
    // roves-js scans its root stack when cppgc traces its root set, like SpiderMonkey scans
    // rooted values at GC time. Without incremental/concurrent marking that scan happens in
    // one atomic pause and sees every root (correctness first; revisit with write barriers on
    // root updates).
    v8::V8::set_flags_from_string("--no-incremental-marking --no-concurrent-marking --no-parallel-marking");
    // V8 150 can reuse a Function constructor compilation from another classic script,
    // including its dynamic-import referrer (left/owner.js instead of right/owner.js).
    // Disable the maintained V8 compilation cache rather than rewriting source or keeping
    // a second module/referrer graph. Deno's module cache remains enabled. Revisit the
    // compilation/startup cost and this guard when upgrading the shared Deno/V8 version.
    v8::V8::set_flags_from_string("--no-compilation-cache");
    // See ../../docs/V8_MIGRATION.md's "Console-oriented requirement: JIT-less" — a
    // first-class supported configuration from the start. This must run before the platform
    // is initialized, not after.
    #[cfg(feature = "jitless")]
    v8::V8::set_flags_from_string("--jitless");
    if !extra.is_empty() {
        v8::V8::set_flags_from_string(extra);
    }
    // `ROVES_V8_FLAGS` appends V8 flags (e.g. `--max-semi-space-size=32` to tune the young
    // generation) for measurements on a shipped build. A diagnostic switch, like servoshell's
    // `ROVES_EXTRA_ARGS`, not a player option.
    if let Ok(flags) = std::env::var("ROVES_V8_FLAGS") {
        v8::V8::set_flags_from_string(&flags);
    }
}

/// The platform V8 is initialized with: foreground tasks go to the handler of their isolate
/// (see `foreground`), never to a thread of their own.
pub fn platform() -> v8::SharedRef<v8::Platform> {
    v8::new_custom_platform(0, false, false, foreground::Platform).make_shared()
}

/// Initializes V8 directly, for this crate's own tests (which do not use deno_core).
#[cfg(test)]
pub(crate) fn initialize_for_tests() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        set_flags("");
        v8::V8::initialize_platform(platform());
        v8::V8::initialize();
    });
}
