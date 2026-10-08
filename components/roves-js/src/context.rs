/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The safe context wrapper (`js::context::JSContext`) Servo threads through script code. As
//! in mozjs, `&mut JSContext` marks code that may GC, and `&NoGC` (from `&JSContext`) code
//! that may not.

use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub use crate::jsapi::JSContext as RawJSContext;

pub struct JSContext {
    pub(crate) ptr: NonNull<RawJSContext>,
    no_gc: NoGC,
}

impl JSContext {
    /// # Safety
    /// `cx` must be a live context of this thread's runtime.
    pub unsafe fn from_ptr(cx: NonNull<RawJSContext>) -> JSContext {
        JSContext { ptr: cx, no_gc: NoGC(()) }
    }

    /// The context of this thread's runtime, if any.
    ///
    /// # Safety
    /// Only one `JSContext` may be used at a time on a thread (as with mozjs).
    pub unsafe fn get_from_thread() -> Option<JSContext> {
        // SAFETY: forwarded to the caller.
        crate::rust::Runtime::get().map(|raw_cx| unsafe { JSContext::from_ptr(raw_cx) })
    }

    #[inline]
    #[must_use]
    pub fn no_gc<'cx>(&'cx self) -> &'cx NoGC {
        &self.no_gc
    }

    #[inline]
    #[must_use]
    pub fn no_gc_mut<'cx>(&'cx mut self) -> &'cx mut NoGC {
        &mut self.no_gc
    }

    /// # Safety
    /// The raw context must not outlive this wrapper's use.
    pub unsafe fn raw_cx(&mut self) -> *mut RawJSContext {
        self.ptr.as_ptr()
    }

    /// # Safety
    /// As for [`JSContext::raw_cx`]; the caller must not GC through it.
    pub unsafe fn raw_cx_no_gc(&self) -> *mut RawJSContext {
        self.ptr.as_ptr()
    }

    pub(crate) fn raw(&self) -> &RawJSContext {
        // SAFETY: the pointer is live for the wrapper's use (constructor contract).
        unsafe { self.ptr.as_ref() }
    }
}

impl AsMut<JSContext> for JSContext {
    fn as_mut(&mut self) -> &mut JSContext {
        self
    }
}

impl Deref for JSContext {
    type Target = NoGC;

    fn deref<'cx>(&'cx self) -> &'cx NoGC {
        self.no_gc()
    }
}

impl DerefMut for JSContext {
    fn deref_mut<'cx>(&'cx mut self) -> &'cx mut NoGC {
        self.no_gc_mut()
    }
}

/// Proof that no GC can happen while it is borrowed.
pub struct NoGC(());

impl NoGC {
    /// # Safety
    /// The caller must guarantee that no GC happens while the token is used.
    pub unsafe fn new() -> Self {
        NoGC(())
    }
}
