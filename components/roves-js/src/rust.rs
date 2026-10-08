/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `js::rust`: the runtime and the Rust-side handle types.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use v8::cppgc::{GarbageCollected, Persistent, Visitor};

pub use crate::gc::{
    GCMethods, Handle, HandleFunction, HandleObject, HandleString, HandleValue, MutableHandle,
    MutableHandleObject, MutableHandleString, MutableHandleValue, RootKind, RootedGuard,
};
use crate::jsapi::JSContext as RawJSContext;

/// Traced on every GC through a persistent root: reports this thread's root stack.
struct RootSet;

// SAFETY: `trace` reports every rooted location (see `gc::trace_roots`).
unsafe impl GarbageCollected for RootSet {
    fn trace(&self, visitor: &mut Visitor) {
        crate::gc::trace_roots(visitor);
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesJsRootSet"
    }
}

thread_local! {
    static CURRENT: Cell<Option<NonNull<RawJSContext>>> = const { Cell::new(None) };
}

/// One V8 isolate with its initial realm: the `JSRuntime`/`JSContext` pair of mozjs.
pub struct Runtime {
    // Field order matters: the root and context must drop before the isolate.
    root_set: Persistent<RootSet>,
    raw: Box<RawJSContext>,
    /// Boxed: the raw context points at the `Isolate` value, which lives inside the
    /// `OwnedIsolate` and must not move.
    isolate: Box<v8::OwnedIsolate>,
}

impl Runtime {
    pub fn new() -> Runtime {
        #[cfg(not(test))]
        roves_v8::initialize_engine();
        // Tests force collections, which V8 only allows with --expose-gc.
        #[cfg(test)]
        roves_v8::initialize_engine_with_flags("--expose-gc");
        let mut isolate = Box::new(v8::Isolate::new(v8::CreateParams::default()));
        let (context, root_set) = {
            v8::scope!(let scope, &mut **isolate);
            let context = v8::Context::new(scope, Default::default());
            let heap = scope.get_cpp_heap().expect("V8 isolates carry a cppgc heap");
            // SAFETY: moved straight into a Persistent.
            let root_set = unsafe { v8::cppgc::make_garbage_collected(heap, RootSet) };
            (v8::Global::new(scope, context), Persistent::new(&root_set))
        };
        let mut raw = Box::new(RawJSContext { isolate: std::ptr::null_mut(), context: RefCell::new(context) });
        raw.isolate = &mut **isolate as *mut v8::Isolate;
        let runtime = Runtime { root_set, raw, isolate };
        CURRENT.with(|current| current.set(Some(runtime.cx_ptr())));
        runtime
    }

    fn cx_ptr(&self) -> NonNull<RawJSContext> {
        NonNull::from(&*self.raw)
    }

    /// The raw context (`Runtime::cx` in mozjs).
    pub fn cx(&self) -> *mut RawJSContext {
        self.cx_ptr().as_ptr()
    }

    /// The safe context wrapper for this runtime.
    pub fn cx_mut(&mut self) -> crate::context::JSContext {
        // SAFETY: the raw context lives as long as the runtime.
        unsafe { crate::context::JSContext::from_ptr(self.cx_ptr()) }
    }

    /// The raw context of this thread's runtime.
    pub fn get() -> Option<NonNull<RawJSContext>> {
        CURRENT.with(Cell::get)
    }

    /// Forces a full, precise garbage collection (tests only: no stack scanning).
    #[doc(hidden)]
    pub fn gc_for_testing(&mut self) {
        let isolate: &mut v8::Isolate = &mut self.isolate;
        if let Some(heap) = isolate.get_cpp_heap() {
            // SAFETY: called outside any native frame that holds unrooted cell pointers it
            // still needs.
            unsafe { heap.collect_garbage_for_testing(v8::cppgc::EmbedderStackState::NoHeapPointers) };
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            if current.get() == Some(self.cx_ptr()) {
                current.set(None);
            }
        });
        let _ = &self.root_set;
    }
}
