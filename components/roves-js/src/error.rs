/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Throwing errors as the pending exception (mozjs's `error` module).

use std::ffi::CStr;

use crate::api::{ErrorKind, throw_error};
use crate::context::{JSContext, RawJSContext};

fn throw(cx: *mut RawJSContext, kind: ErrorKind, error: &CStr) {
    // SAFETY: callers pass a live context (JSAPI contract).
    throw_error(unsafe { &*cx }, kind, &error.to_string_lossy());
}

/// Throws a `TypeError` with `error` as its message.
///
/// # Safety
/// `cx` must be a live context.
pub unsafe fn throw_type_error(cx: *mut RawJSContext, error: &CStr) {
    throw(cx, ErrorKind::Type, error);
}

/// Throws a `RangeError` with `error` as its message.
///
/// # Safety
/// `cx` must be a live context.
pub unsafe fn throw_range_error(cx: *mut RawJSContext, error: &CStr) {
    throw(cx, ErrorKind::Range, error);
}

/// Throws an internal error (an `Error`) with `error` as its message.
///
/// # Safety
/// `cx` must be a live context.
pub unsafe fn throw_internal_error(cx: *mut RawJSContext, error: &CStr) {
    throw(cx, ErrorKind::Internal, error);
}

pub fn throw_type_error_safe(cx: &mut JSContext, error: &CStr) {
    throw(cx.ptr.as_ptr(), ErrorKind::Type, error);
}

pub fn throw_range_error_safe(cx: &mut JSContext, error: &CStr) {
    throw(cx.ptr.as_ptr(), ErrorKind::Range, error);
}

pub fn throw_internal_error_safe(cx: &mut JSContext, error: &CStr) {
    throw(cx.ptr.as_ptr(), ErrorKind::Internal, error);
}
