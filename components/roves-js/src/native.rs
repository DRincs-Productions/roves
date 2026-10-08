/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Calling SpiderMonkey-style natives (`JSNative`) from V8 callbacks.
//!
//! A `JSNative` receives `vp`: `[callee, this, args..., (new.target)]` with the return value
//! written to `vp[0]`, and reports failure by returning `false` with a pending exception.
//! When constructing, `this` is a magic value (`CallArgs::is_constructing` checks it).

use crate::jsapi::{JSContext, JSNative};
use crate::jsval::{JSVal, from_v8, magic_value, to_v8};

pub(crate) fn throw_type_error(scope: &mut v8::PinScope, message: &str) {
    let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let error = v8::Exception::type_error(scope, message);
    scope.throw_exception(error);
}

/// Rethrows the context's pending exception into V8 (after a native or hook failed).
pub(crate) fn rethrow_pending(scope: &mut v8::PinScope, cx: &JSContext) {
    let pending = cx.pending_exception.borrow_mut().take();
    match pending {
        Some(exception) => {
            let exception = v8::Local::new(scope, &exception);
            scope.throw_exception(exception);
        },
        // SpiderMonkey's uncatchable failure (no pending exception): stop the script.
        None => {
            scope.terminate_execution();
        },
    }
}

/// Calls `native` for a V8 call of `callee` with `args`, writing its result to `retval`.
pub(crate) fn call_native(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    retval: &mut v8::ReturnValue,
    native: JSNative,
    callee: v8::Local<v8::Value>,
    constructing: bool,
) {
    let Some(native) = native else {
        throw_type_error(scope, "not a function");
        return;
    };
    let cx = JSContext::current();
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let argc = args.length().max(0) as u32;
    let mut vp: Vec<JSVal> = Vec::with_capacity(argc as usize + 3);
    vp.push(from_v8(scope, callee));
    vp.push(if constructing { magic_value() } else { from_v8(scope, args.this().into()) });
    for index in 0..argc as i32 {
        vp.push(from_v8(scope, args.get(index)));
    }
    if constructing {
        vp.push(from_v8(scope, args.new_target()));
    }
    // Root the array for the call; the native reads and writes it in place.
    crate::rooted!(in(raw_cx) let mut rooted_vp = vp);
    // SAFETY: the rooted vector is not resized during the call.
    let vp_pointer = unsafe { (*rooted_vp.as_ptr()).as_mut_ptr() };
    // SAFETY: SpiderMonkey's JSNative contract: `vp` holds callee, this and `argc` arguments.
    let ok = unsafe { native(raw_cx, argc, vp_pointer) };
    if ok {
        // SAFETY: the return value is rooted in `vp`.
        let result = unsafe { to_v8(scope, rooted_vp[0]) };
        retval.set(result);
    } else {
        rethrow_pending(scope, cx);
    }
    rooted_vp.set(Vec::new());
}
