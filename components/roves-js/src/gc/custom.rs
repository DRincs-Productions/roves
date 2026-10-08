/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Ported from mozjs 0.21.6 (same license) onto the roves-js root stack and tracer.

use std::ops::{Deref, DerefMut};

use crate::glue::{CallObjectRootTracer, CallValueRootTracer};
use crate::jsapi::{JSContext, JSObject, JSTracer};
use crate::jsval::Value;
use crate::rust::{Handle, MutableHandle};
use crate::gc::RootKind;

/// Similarly to `Traceable` trait, it's used to specify tracing of various types
/// that are used in conjunction with `CustomAutoRooter`.
pub unsafe trait CustomTrace {
    fn trace(&self, trc: *mut JSTracer);
}

unsafe impl CustomTrace for *mut JSObject {
    fn trace(&self, trc: *mut JSTracer) {
        let this = self as *const *mut _ as *mut *mut _;
        unsafe {
            CallObjectRootTracer(trc, this, c"object".as_ptr());
        }
    }
}

unsafe impl CustomTrace for Value {
    fn trace(&self, trc: *mut JSTracer) {
        let this = self as *const _ as *mut _;
        unsafe {
            CallValueRootTracer(trc, this, c"any".as_ptr());
        }
    }
}

unsafe impl<T: CustomTrace> CustomTrace for Option<T> {
    fn trace(&self, trc: *mut JSTracer) {
        if let Some(ref some) = *self {
            some.trace(trc);
        }
    }
}

unsafe impl<T: CustomTrace> CustomTrace for Vec<T> {
    fn trace(&self, trc: *mut JSTracer) {
        for elem in self {
            elem.trace(trc);
        }
    }
}

// This structure reimplements a C++ class that uses virtual dispatch, so
// use C layout to guarantee that vftable in CustomAutoRooter is in right place.
/// A rooter for a custom traceable value, registered on the root stack while its guard lives.
pub struct CustomAutoRooter<T> {
    data: T,
}

impl<T> CustomAutoRooter<T> {
    unsafe fn add_to_root_stack(&mut self, _cx: *mut JSContext)
    where
        T: CustomTrace,
    {
        crate::gc::register_custom_root(&self.data as *const T as *const std::ffi::c_void, trace_custom::<T>);
    }

    unsafe fn remove_from_root_stack(&mut self) {
        crate::gc::unregister_custom_root(&self.data as *const T as *const std::ffi::c_void);
    }
}

unsafe fn trace_custom<T: CustomTrace>(location: *const std::ffi::c_void, visitor: &mut v8::cppgc::Visitor) {
    // SAFETY: the guard unregisters the location before the rooter goes away.
    unsafe { &*(location as *const T) }.trace(crate::glue::tracer(visitor));
}

impl<T: CustomTrace> CustomAutoRooter<T> {
    pub fn new(data: T) -> Self {
        CustomAutoRooter { data }
    }

    pub fn root(&'_ mut self, cx: *mut JSContext) -> CustomAutoRooterGuard<'_, T> {
        CustomAutoRooterGuard::new(cx, self)
    }
}

#[cfg_attr(
    feature = "crown",
    crown::unrooted_must_root_lint::allow_unrooted_interior
)]
pub struct CustomAutoRooterGuard<'a, T: 'a + CustomTrace> {
    rooter: &'a mut CustomAutoRooter<T>,
}

impl<'a, T: 'a + CustomTrace> CustomAutoRooterGuard<'a, T> {
    pub fn new(cx: *mut JSContext, rooter: &'a mut CustomAutoRooter<T>) -> Self {
        unsafe {
            rooter.add_to_root_stack(cx);
        }
        CustomAutoRooterGuard { rooter }
    }

    pub fn handle(&'a self) -> Handle<'a, T>
    where
        T: RootKind,
    {
        // SAFETY: This root is a marked location.
        unsafe { Handle::from_marked_location(&raw const (self.rooter.data)) }
    }

    pub fn handle_mut(&'_ mut self) -> MutableHandle<'_, T>
    where
        T: RootKind,
    {
        // SAFETY: This root is a marked location.
        unsafe { MutableHandle::from_marked_location(&mut self.rooter.data) }
    }
}

impl<'a, T: 'a + CustomTrace> Deref for CustomAutoRooterGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.rooter.data
    }
}

impl<'a, T: 'a + CustomTrace> DerefMut for CustomAutoRooterGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.rooter.data
    }
}

impl<'a, T: 'a + CustomTrace> Drop for CustomAutoRooterGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            self.rooter.remove_from_root_stack();
        }
    }
}

pub type SequenceRooter<T> = CustomAutoRooter<Vec<T>>;
pub type SequenceRooterGuard<'a, T> = CustomAutoRooterGuard<'a, Vec<T>>;
