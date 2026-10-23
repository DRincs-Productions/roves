/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! V8's incumbent realm API (HTML's incumbent settings object): `Isolate::GetIncumbentContext`
//! and `Context::BackupIncumbentScope`. rusty_v8 152 does not bind them, but its prebuilt V8
//! library exports both, so these declare the exported C++ symbols directly. The crate pins
//! `v8 = "=152.2.0"`; a V8 update must recheck these names and the layout below.
//!
//! ABI notes: `Local<T>` is one pointer and `V8_TRIVIAL_ABI`, so it travels in a register.
//! The Microsoft C++ ABI returns any class from an instance method through a hidden pointer
//! passed after `this`, and returns that pointer.

use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

/// `v8::Context::BackupIncumbentScope`'s fields.
#[repr(C)]
struct RawScope {
    backup_incumbent_context: *const v8::Context,
    js_stack_comparable_address: usize,
    prev: *const c_void,
}

unsafe extern "C" {
    #[cfg_attr(not(target_env = "msvc"), link_name = "_ZN2v87Context20BackupIncumbentScopeC1ENS_5LocalIS0_EE")]
    #[cfg_attr(target_env = "msvc", link_name = "??0BackupIncumbentScope@Context@v8@@QEAA@V?$Local@VContext@v8@@@2@@Z")]
    fn backup_incumbent_scope_construct(this: *mut RawScope, context: *mut v8::Context);

    #[cfg_attr(not(target_env = "msvc"), link_name = "_ZN2v87Context20BackupIncumbentScopeD1Ev")]
    #[cfg_attr(target_env = "msvc", link_name = "??1BackupIncumbentScope@Context@v8@@QEAA@XZ")]
    fn backup_incumbent_scope_destruct(this: *mut RawScope);
}

#[cfg(not(target_env = "msvc"))]
unsafe extern "C" {
    #[link_name = "_ZN2v87Isolate19GetIncumbentContextEv"]
    fn isolate_get_incumbent_context(isolate: *mut c_void) -> *const v8::Context;
}

#[cfg(target_env = "msvc")]
unsafe extern "C" {
    #[link_name = "?GetIncumbentContext@Isolate@v8@@QEAA?AV?$Local@VContext@v8@@@2@XZ"]
    fn isolate_get_incumbent_context(isolate: *mut c_void, result: *mut *const v8::Context) -> *mut *const v8::Context;
}

fn raw_isolate(isolate: &v8::Isolate) -> *mut c_void {
    // SAFETY: `UnsafeRawIsolatePtr` is one pointer, the C++ `v8::Isolate*`.
    unsafe { std::mem::transmute::<v8::UnsafeRawIsolatePtr, *mut c_void>(isolate.as_raw_isolate_ptr()) }
}

/// The incumbent context: the context of the innermost JS frame when it is newer than the
/// innermost [`BackupIncumbentScope`], else that scope's context, else the entered context.
pub fn incumbent_context<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Context> {
    let isolate = raw_isolate(scope);
    // SAFETY: a live isolate, current on this thread; the handle lands in `scope`.
    let slot = unsafe {
        #[cfg(not(target_env = "msvc"))]
        let slot = isolate_get_incumbent_context(isolate);
        #[cfg(target_env = "msvc")]
        let slot = {
            let mut slot = std::ptr::null();
            isolate_get_incumbent_context(isolate, &mut slot);
            slot
        };
        slot
    };
    // SAFETY: `Local` is a non-null handle slot pointer, which V8 always returns here.
    unsafe { std::mem::transmute::<*const v8::Context, v8::Local<'s, v8::Context>>(slot) }
}

/// A `Context::BackupIncumbentScope`: while it is the innermost one, it supplies `context`
/// as the incumbent unless a newer JS frame runs. Scopes of an isolate must be exited in
/// reverse order of entry, with [`BackupIncumbentScope::exit`], before the isolate is disposed.
pub struct BackupIncumbentScope {
    // Boxed: V8 links scopes by address.
    raw: Box<MaybeUninit<RawScope>>,
    // The global handle V8 holds in the scope, as a local: a stable pointer to its slot.
    slot: NonNull<v8::Context>,
    exited: bool,
}

impl BackupIncumbentScope {
    pub fn enter(isolate: &v8::Isolate, context: v8::Global<v8::Context>) -> Self {
        let mut raw = Box::new(MaybeUninit::<RawScope>::uninit());
        let slot = context.into_raw();
        // SAFETY: the constructor reads `Isolate::Current()`; enter ours for the call.
        unsafe {
            isolate.enter();
            backup_incumbent_scope_construct(raw.as_mut_ptr(), slot.as_ptr());
            isolate.exit();
        }
        BackupIncumbentScope { raw, slot, exited: false }
    }

    /// Exits the scope. It must be `isolate`'s innermost one.
    pub fn exit(mut self, isolate: &mut v8::Isolate) {
        // SAFETY: constructed by `enter` on this isolate, innermost (the caller's contract).
        unsafe {
            isolate.enter();
            backup_incumbent_scope_destruct(self.raw.as_mut_ptr().cast());
            isolate.exit();
            drop(v8::Global::from_raw(isolate, self.slot));
        }
        self.exited = true;
    }
}

impl Drop for BackupIncumbentScope {
    fn drop(&mut self) {
        if !self.exited {
            // Still linked into the isolate: leak the memory and slot V8 may read.
            std::mem::forget(std::mem::replace(&mut self.raw, Box::new(MaybeUninit::uninit())));
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
        crate::initialize_engine();
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
