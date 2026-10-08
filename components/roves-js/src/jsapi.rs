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
}
