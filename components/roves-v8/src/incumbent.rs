/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! V8's incumbent realm API (HTML's incumbent settings object): `Isolate::GetIncumbentContext`
//! and `Context::BackupIncumbentScope`. rusty_v8 150 does not bind them; `src/shim.cc` calls
//! them through the pinned public headers, so no symbol name or layout is written by hand.

use std::alloc::Layout;
use std::ffi::c_void;
use std::ptr::NonNull;

unsafe extern "C" {
    fn roves_v8_incumbent_context(isolate: *mut c_void) -> *const v8::Context;
    fn roves_v8_backup_incumbent_scope_size() -> usize;
    fn roves_v8_backup_incumbent_scope_align() -> usize;
    fn roves_v8_backup_incumbent_scope_enter(storage: *mut c_void, context: *const v8::Context);
    fn roves_v8_backup_incumbent_scope_exit(storage: *mut c_void);
}

/// The C++ `v8::Isolate*` of `isolate`.
pub(crate) fn raw_isolate(isolate: &v8::Isolate) -> *mut c_void {
    // SAFETY: `UnsafeRawIsolatePtr` is one pointer, the C++ `v8::Isolate*`.
    unsafe { std::mem::transmute::<v8::UnsafeRawIsolatePtr, *mut c_void>(isolate.as_raw_isolate_ptr()) }
}

/// The incumbent context: the context of the innermost JS frame when it is newer than the
/// innermost [`BackupIncumbentScope`], else that scope's context, else the entered context.
pub fn incumbent_context<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Context> {
    // SAFETY: a live isolate, current on this thread; the handle lands in `scope`.
    let slot = unsafe { roves_v8_incumbent_context(raw_isolate(scope)) };
    // SAFETY: `Local` is a non-null handle slot pointer, which V8 always returns here.
    unsafe { std::mem::transmute::<*const v8::Context, v8::Local<'s, v8::Context>>(slot) }
}

fn scope_layout() -> Layout {
    // SAFETY: plain queries of the compiled header's type.
    let (size, align) = unsafe { (roves_v8_backup_incumbent_scope_size(), roves_v8_backup_incumbent_scope_align()) };
    Layout::from_size_align(size, align).expect("BackupIncumbentScope's layout")
}

/// A `Context::BackupIncumbentScope`: while it is the innermost one, it supplies `context`
/// as the incumbent unless a newer JS frame runs. Scopes of an isolate must be exited in
/// reverse order of entry, with [`BackupIncumbentScope::exit`], before the isolate is disposed.
/// A scope dropped without `exit` is still linked into the isolate: its storage and slot leak.
pub struct BackupIncumbentScope {
    // Heap storage: V8 links scopes by address.
    storage: NonNull<u8>,
    // The global handle V8 holds in the scope, as a local: a stable pointer to its slot.
    slot: NonNull<v8::Context>,
}

impl BackupIncumbentScope {
    pub fn enter(isolate: &v8::Isolate, context: v8::Global<v8::Context>) -> Self {
        let layout = scope_layout();
        // SAFETY: a non-zero size (the scope has fields).
        let storage = NonNull::new(unsafe { std::alloc::alloc(layout) }).expect("BackupIncumbentScope storage");
        let slot = context.into_raw();
        // SAFETY: the constructor reads `Isolate::Current()`; enter ours for the call.
        unsafe {
            isolate.enter();
            roves_v8_backup_incumbent_scope_enter(storage.as_ptr().cast(), slot.as_ptr());
            isolate.exit();
        }
        BackupIncumbentScope { storage, slot }
    }

    /// Exits the scope. It must be `isolate`'s innermost one.
    pub fn exit(self, isolate: &mut v8::Isolate) {
        // SAFETY: constructed by `enter` on this isolate, innermost (the caller's contract).
        unsafe {
            isolate.enter();
            roves_v8_backup_incumbent_scope_exit(self.storage.as_ptr().cast());
            isolate.exit();
            drop(v8::Global::from_raw(isolate, self.slot));
            std::alloc::dealloc(self.storage.as_ptr(), scope_layout());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `incumbent()`: the global of the incumbent context.
    fn incumbent(scope: &mut v8::PinScope, _args: v8::FunctionCallbackArguments, mut result: v8::ReturnValue) {
        let global = incumbent_context(scope).global(scope);
        result.set(global.into());
    }

    fn run<'s>(scope: &mut v8::PinScope<'s, '_>, source: &str) -> v8::Local<'s, v8::Value> {
        let source = v8::String::new(scope, source).unwrap();
        v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap()
    }

    #[test]
    fn incumbent_follows_js_frames_and_backup_scopes() {
        crate::initialize_for_tests();
        let mut isolate = v8::Isolate::new(Default::default());
        let (a, b, sentinel) = {
            v8::scope!(let scope, &mut isolate);
            let token = v8::String::new(scope, "token").unwrap().into();
            let contexts: Vec<_> = (0..3).map(|_| {
                let context = v8::Context::new(scope, Default::default());
                context.set_security_token(token);
                v8::Global::new(scope, context)
            }).collect();
            let [a, b, sentinel] = <[_; 3]>::try_from(contexts).ok().unwrap();
            (a, b, sentinel)
        };
        v8::scope!(let scope, &mut isolate);
        let (a, b) = (v8::Local::new(scope, &a), v8::Local::new(scope, &b));
        let (a_global, b_global) = (a.global(scope), b.global(scope));
        for (context, other) in [(a, b_global), (b, a_global)] {
            let scope = &mut v8::ContextScope::new(scope, context);
            let native = v8::Function::new(scope, incumbent).unwrap();
            let global = context.global(scope);
            let (key, other_key) = (v8::String::new(scope, "incumbent").unwrap(), v8::String::new(scope, "other").unwrap());
            global.set(scope, key.into(), native.into());
            global.set(scope, other_key.into(), other.into());
            run(scope, "globalThis.viaHere = () => incumbent();");
        }
        let scope = &mut v8::ContextScope::new(scope, b);
        assert!(run(scope, "incumbent() === globalThis").is_true(), "the calling frame's realm");
        assert!(run(scope, "other.viaHere() === other").is_true(), "the innermost frame's realm, not the entered one");
        assert!(incumbent_context(scope) == b, "no JS frame and no backup: the entered context");
        let backup = BackupIncumbentScope::enter(scope, sentinel.clone());
        assert!(incumbent_context(scope) == v8::Local::new(scope, &sentinel), "no newer JS frame: the backup");
        assert!(run(scope, "incumbent() === globalThis").is_true(), "a newer JS frame wins over the backup");
        backup.exit(scope);
        assert!(incumbent_context(scope) == b);
    }
}
