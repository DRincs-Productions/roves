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

thread_local! {
    /// Argument vectors of finished native calls, reused by later ones (a native call per DOM
    /// property access made a heap allocation each). Nested calls each take their own.
    static VP_POOL: std::cell::RefCell<Vec<Vec<JSVal>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The realm of the code calling into a callback: V8's incumbent context, i.e. the innermost
/// JS frame (the immediate JS caller, also in a promise reaction, where the JSAPI current
/// realm may be stale). Embedder JS helpers are JS frames of the realm that ran them, which
/// proxy traps enter for their accessing code. Without a JS frame newer than a hidden-caller
/// scope, the caller is embedder code: the context it entered (or the running microtask's).
pub(crate) fn caller_realm(cx: &JSContext, scope: &mut v8::PinScope) -> *mut crate::jsapi::Realm {
    let incumbent = roves_v8::incumbent::incumbent_context(scope);
    let context = if crate::runtime_impl::is_hidden_incumbent(cx, scope, incumbent) {
        scope.get_entered_or_microtask_context()
    } else {
        incumbent
    };
    crate::realm_impl::realm_of_context(context)
}

/// Calls `native` for a V8 call of `callee` with `args`, writing its result to `retval`.
pub(crate) fn call_native(
    scope: &mut v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    retval: &mut v8::ReturnValue,
    native: JSNative,
    callee: JSVal,
    constructing: bool,
) {
    let Some(native) = native else {
        throw_type_error(scope, "not a function");
        return;
    };
    let cx = JSContext::current();
    // V8 enters a function's context for native calls, including promise reactions
    // drained without a Servo AutoRealm. Keep the JSAPI realm (and its principals)
    // in sync, restoring the embedder's previous realm on every exit.
    let realm = crate::realm_impl::realm_of_context(scope.get_current_context());
    let _realm = cx.switch_realm(realm);
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let argc = args.length().max(0) as u32;
    let mut vp: Vec<JSVal> = VP_POOL.with(|pool| pool.borrow_mut().pop()).unwrap_or_default();
    vp.reserve(argc as usize + 3);
    vp.push(callee);
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
    let mut ok = unsafe { native(raw_cx, argc, vp_pointer) };
    // SAFETY: a rooted object result.
    if ok && rooted_vp[0].is_object() && unsafe { crate::proxy::IsWindowProxy(rooted_vp[0].to_object()) } {
        // SpiderMonkey wraps a value leaving the callee's realm. Window proxies are the only
        // per-realm objects here: a remote window's native converts its proxy for its own
        // realm, so the caller would see another identity than its own wrapper.
        let caller = caller_realm(cx, scope);
        if caller != realm && !caller.is_null() {
            let _caller = cx.switch_realm(caller);
            // SAFETY: the result is rooted in `vp`; the callee's realm stays alive meanwhile.
            let wrapped = unsafe { crate::proxy::wrap_window_for_realm(raw_cx, rooted_vp[0].to_object()) };
            if wrapped.is_null() {
                ok = false;
            } else {
                rooted_vp[0] = crate::jsval::ObjectValue(wrapped);
            }
        }
    }
    if ok {
        // SAFETY: the return value is rooted in `vp`.
        let result = unsafe { to_v8(scope, rooted_vp[0]) };
        retval.set(result);
    } else {
        rethrow_pending(scope, cx);
    }
    let mut used = std::mem::take(&mut *rooted_vp);
    used.clear();
    VP_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < 16 {
            pool.push(used);
        }
    });
}
