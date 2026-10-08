/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `jsapi` names Servo uses: opaque GC-thing types, the raw context and the raw API.
//!
//! GC-thing pointers (`*mut JSObject`, ...) point at cells (see `cell`); the types themselves
//! are opaque and never dereferenced from Rust, as with SpiderMonkey.

use std::cell::RefCell;

pub use crate::gc::{Handle, HandleObject, HandleValue, Heap, MutableHandle, MutableHandleObject, MutableHandleValue};
pub use crate::jsval::{JSVal, Value};
pub use crate::api::{
    ExceptionStackBehavior, JS_ClearPendingException, JS_DeprecatedStringHasLatin1Chars,
    JS_GetPendingException, JS_GetStringLength, JS_IsExceptionPending, JS_SetPendingException,
    JSPROP_ENUMERATE, JSPROP_PERMANENT, JSPROP_READONLY, UTF8Chars,
};

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            /// Opaque GC thing; pointers to it point at a roves-js cell.
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(JSObject, JSString, JSFunction, Symbol, BigInt, JSScript);

/// The raw context behind [`crate::context::JSContext`]: one V8 isolate entered in one realm,
/// plus this thread's root stack (see `gc`).
pub struct JSContext {
    pub(crate) isolate: *mut v8::Isolate,
    /// The current realm's V8 context.
    pub(crate) context: RefCell<v8::Global<v8::Context>>,
    /// SpiderMonkey's pending exception: set when an API call catches a JS exception (or a
    /// native throws one), read and cleared through `JS_GetPendingException` and friends, and
    /// rethrown into V8 when control returns to script.
    pub(crate) pending_exception: RefCell<Option<v8::Global<v8::Value>>>,
}

impl JSContext {
    /// Runs `f` inside a handle scope entered in the current realm.
    pub(crate) fn with_scope<R>(&self, f: impl FnOnce(&mut v8::PinScope) -> R) -> R {
        // SAFETY: the runtime that owns the isolate outlives every context pointer it hands
        // out, and the isolate is only used from its own thread.
        let isolate = unsafe { &mut *self.isolate };
        v8::scope!(let scope, isolate);
        let context = v8::Local::new(scope, &*self.context.borrow());
        let scope = &mut v8::ContextScope::new(scope, context);
        f(scope)
    }

    /// Runs `f` like [`JSContext::with_scope`] under a `TryCatch`: a JS exception thrown
    /// inside becomes the pending exception, and the result is `None`.
    pub(crate) fn catching<R>(&self, f: impl FnOnce(&mut v8::PinScope) -> Option<R>) -> Option<R> {
        self.with_scope(|scope| {
            v8::tc_scope!(let try_catch, scope);
            let result = f(try_catch);
            if try_catch.has_caught() {
                let exception = try_catch.exception().unwrap_or_else(|| v8::undefined(try_catch).into());
                *self.pending_exception.borrow_mut() = Some(v8::Global::new(try_catch, exception));
                try_catch.reset();
                return None;
            }
            result
        })
    }

    /// Makes `exception` the pending exception.
    pub(crate) fn set_pending(&self, scope: &mut v8::PinScope, exception: v8::Local<v8::Value>) {
        *self.pending_exception.borrow_mut() = Some(v8::Global::new(scope, exception));
    }

    /// The context of this thread's runtime, for APIs that take no context argument.
    pub(crate) fn current<'a>() -> &'a JSContext {
        let raw = crate::rust::Runtime::get().expect("a roves-js runtime on this thread");
        // SAFETY: the runtime outlives the API calls made on its thread.
        unsafe { &*raw.as_ptr() }
    }
}
