/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Isolate::SetModifyCodeGenerationFromStringsCallback`, which rusty_v8 150 does not bind
//! (152 does). The callback is installed through `src/shim.cc`, compiled against the pinned
//! public header; V8 then asks the process-wide [`Callback`] on every `eval`/`Function`
//! compilation of a context that disallows code generation from strings.

use std::ffi::c_void;
use std::sync::OnceLock;

/// The decision on one string compilation: `None` blocks it, `Some(None)` compiles the
/// original source, `Some(Some(source))` compiles `source` instead.
pub type Decision<'s> = Option<Option<v8::Local<'s, v8::String>>>;

/// Decides on a string compilation in `context`. Runs inside V8's handle scope, with the
/// context entered; handles it returns must live in that scope.
pub type Callback = for<'s> fn(&mut v8::PinScope<'s, '_>, v8::Local<'s, v8::Value>, bool) -> Decision<'s>;

static CALLBACK: OnceLock<Callback> = OnceLock::new();

unsafe extern "C" {
    fn roves_v8_set_codegen_callback(isolate: *mut c_void);
}

/// Routes `isolate`'s string compilations to `callback`. One callback per process: every
/// roves-js runtime installs the same function.
pub fn set_callback(isolate: &mut v8::Isolate, callback: Callback) {
    let installed = *CALLBACK.get_or_init(|| callback);
    assert!(installed as usize == callback as usize, "one code generation callback per process");
    // SAFETY: a live isolate on its own thread.
    unsafe { roves_v8_set_codegen_callback(crate::incumbent::raw_isolate(isolate)) };
}

/// Called by `shim.cc`'s trampoline.
#[unsafe(no_mangle)]
unsafe extern "C" fn roves_v8_codegen_from_strings(
    context: *const v8::Context,
    source: *const v8::Value,
    is_code_like: bool,
    modified: *mut *const v8::String,
) -> bool {
    let Some(callback) = CALLBACK.get() else { return false };
    // SAFETY: V8 passes non-null handles of its current handle scope (a `Local` is the
    // pointer to its slot, the same in C++ and rusty_v8).
    let (context, source) = unsafe {
        (
            std::mem::transmute::<*const v8::Context, v8::Local<v8::Context>>(context),
            std::mem::transmute::<*const v8::Value, v8::Local<v8::Value>>(source),
        )
    };
    // SAFETY: V8 calls this with `context` entered, inside a handle scope.
    v8::callback_scope!(unsafe scope, context);
    match callback(scope, source, is_code_like) {
        None => false,
        Some(replacement) => {
            let replacement = replacement.map_or(std::ptr::null(), |string| &*string as *const v8::String);
            // SAFETY: the trampoline passes a valid out pointer.
            unsafe { *modified = replacement };
            true
        },
    }
}
