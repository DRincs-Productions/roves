/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! JSAPI operations implemented on V8. `jsapi` re-exports the raw (`*mut JSContext`) forms,
//! `rust` the conversion helpers and `rust::wrappers2` the `&mut JSContext` forms, matching
//! where mozjs puts them.
//!
//! Every operation that can run script runs under `JSContext::catching`: an exception becomes
//! the context's pending exception and the operation reports failure (`false`, null or `Err`),
//! as SpiderMonkey's API does.

use std::borrow::Cow;
use std::ffi::CStr;
use std::ops::ControlFlow;

use crate::cell::{StringChars, string_chars};
use crate::context::JSContext as SafeJSContext;
use crate::gc::{HandleObject, HandleValue, MutableHandleValue};
use crate::jsapi::{JSContext, JSObject, JSString};
use crate::jsval::{from_v8, to_v8};

/// `JSPROP_*` attribute flags (SpiderMonkey's values).
pub const JSPROP_ENUMERATE: u32 = 0x01;
pub const JSPROP_READONLY: u32 = 0x02;
pub const JSPROP_PERMANENT: u32 = 0x04;

/// Whether `JS_SetPendingException` records a stack (V8 records one on error objects anyway).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExceptionStackBehavior {
    DoNotCapture = 0,
    Capture = 1,
}

/// UTF-8 characters to copy into a new JS string (mozjs passes a `mozilla::Range`).
pub struct UTF8Chars {
    pub(crate) start: *const u8,
    pub(crate) length: usize,
}

fn raw<'a>(cx: *mut JSContext) -> &'a JSContext {
    // SAFETY: callers pass a live context of this thread's runtime (JSAPI contract).
    unsafe { &*cx }
}

// --- Exceptions ---------------------------------------------------------------------------

pub unsafe fn JS_IsExceptionPending(cx: *mut JSContext) -> bool {
    raw(cx).pending_exception.borrow().is_some()
}

pub unsafe fn JS_ClearPendingException(cx: *mut JSContext) {
    raw(cx).pending_exception.borrow_mut().take();
}

/// Stores the pending exception in `rval` (without clearing it); `false` if none is pending.
pub unsafe fn JS_GetPendingException(cx: *mut JSContext, mut rval: MutableHandleValue) -> bool {
    let cx = raw(cx);
    let pending = cx.pending_exception.borrow().clone();
    let Some(exception) = pending else { return false };
    rval.set(cx.with_scope(|scope| {
        let local = v8::Local::new(scope, &exception);
        from_v8(scope, local)
    }));
    true
}

pub unsafe fn JS_SetPendingException(cx: *mut JSContext, value: HandleValue, _behavior: ExceptionStackBehavior) {
    let cx = raw(cx);
    cx.with_scope(|scope| {
        // SAFETY: a handle refers to a rooted location.
        let local = unsafe { to_v8(scope, value.get()) };
        cx.set_pending(scope, local);
    });
}

/// Throws a new `kind` error with `message` as the pending exception.
pub(crate) fn throw_error(cx: &JSContext, kind: ErrorKind, message: &str) {
    cx.with_scope(|scope| {
        let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
        let exception = match kind {
            ErrorKind::Type => v8::Exception::type_error(scope, message),
            ErrorKind::Range => v8::Exception::range_error(scope, message),
            ErrorKind::Internal => v8::Exception::error(scope, message),
        };
        cx.set_pending(scope, exception);
    });
}

#[derive(Clone, Copy)]
pub(crate) enum ErrorKind {
    Type,
    Range,
    Internal,
}

// --- Type conversions (`js::rust::To*`) -----------------------------------------------------

/// ECMAScript ToBoolean (never throws).
pub unsafe fn ToBoolean(v: HandleValue) -> bool {
    let value = v.get();
    if value.is_boolean() {
        return value.to_boolean();
    }
    if value.is_int32() {
        return value.to_int32() != 0;
    }
    if value.is_null_or_undefined() {
        return false;
    }
    if value.is_double() {
        let number = value.to_double();
        return !number.is_nan() && number != 0.0;
    }
    if value.is_object() || value.is_symbol() {
        return true;
    }
    JSContext::current().with_scope(|scope| {
        // SAFETY: a handle refers to a rooted location.
        let local = unsafe { to_v8(scope, value) };
        local.boolean_value(scope)
    })
}

/// ECMAScript ToNumber (may run `valueOf`/`toString` and throw).
pub unsafe fn ToNumber(cx: *mut JSContext, v: HandleValue) -> Result<f64, ()> {
    let value = v.get();
    if value.is_number() {
        return Ok(value.to_number());
    }
    raw(cx)
        .catching(|scope| {
            // SAFETY: a handle refers to a rooted location.
            let local = unsafe { to_v8(scope, value) };
            local.number_value(scope)
        })
        .ok_or(())
}

/// ECMAScript's modular integer conversion (ToInt32 and friends) of a number.
pub(crate) fn to_integer_modulo(number: f64, bits: u32, signed: bool) -> i128 {
    if !number.is_finite() || number == 0.0 {
        return 0;
    }
    let modulus = 1i128 << bits;
    let truncated = number.trunc();
    // `truncated % 2^bits` computed exactly: f64 → i128 is exact below 2^127, and larger
    // magnitudes are multiples of 2^bits for bits <= 64.
    let wrapped = if truncated.abs() >= 2f64.powi(127) { 0 } else { (truncated as i128).rem_euclid(modulus) };
    if signed && wrapped >= modulus / 2 { wrapped - modulus } else { wrapped }
}

macro_rules! integer_conversion {
    ($name:ident, $ty:ty, $bits:expr, $signed:expr) => {
        /// ECMAScript integer conversion (may run `valueOf` and throw).
        pub unsafe fn $name(cx: *mut JSContext, v: HandleValue) -> Result<$ty, ()> {
            let value = v.get();
            if value.is_int32() {
                return Ok(to_integer_modulo(value.to_int32() as f64, $bits, $signed) as $ty);
            }
            // SAFETY: forwarded.
            let number = unsafe { ToNumber(cx, v) }?;
            Ok(to_integer_modulo(number, $bits, $signed) as $ty)
        }
    };
}

integer_conversion!(ToInt32, i32, 32, true);
integer_conversion!(ToUint32, u32, 32, false);
integer_conversion!(ToUint16, u16, 16, false);
integer_conversion!(ToInt64, i64, 64, true);
integer_conversion!(ToUint64, u64, 64, false);

/// ECMAScript ToString; null (with a pending exception) on failure.
pub unsafe fn ToString(cx: &mut SafeJSContext, v: HandleValue) -> *mut JSString {
    let value = v.get();
    if value.is_string() {
        return value.to_string();
    }
    raw(cx.ptr.as_ptr())
        .catching(|scope| {
            // SAFETY: a handle refers to a rooted location.
            let local = unsafe { to_v8(scope, value) };
            let string = local.to_string(scope)?;
            Some(from_v8(scope, string.into()).to_string())
        })
        .unwrap_or(std::ptr::null_mut())
}

// --- Strings -----------------------------------------------------------------------------

fn chars_of<'c>(s: *mut JSString) -> &'c StringChars {
    JSContext::current().with_scope(|scope| {
        // SAFETY: JSAPI callers pass live strings.
        unsafe { string_chars(scope, s as *mut std::ffi::c_void) }
    })
}

/// Whether the string's characters are all Latin-1 (`JS_DeprecatedStringHasLatin1Chars`).
pub unsafe fn JS_DeprecatedStringHasLatin1Chars(s: *mut JSString) -> bool {
    matches!(chars_of(s), StringChars::Latin1(_))
}

pub unsafe fn JS_GetStringLength(s: *mut JSString) -> usize {
    match chars_of(s) {
        StringChars::Latin1(chars) => chars.len(),
        StringChars::TwoByte(chars) => chars.len(),
    }
}

/// The string's Latin-1 characters, valid while the string lives; null if it is two-byte.
pub fn latin1_chars(s: *mut JSString, length: &mut usize) -> *const u8 {
    match chars_of(s) {
        StringChars::Latin1(chars) => {
            *length = chars.len();
            chars.as_ptr()
        },
        StringChars::TwoByte(_) => std::ptr::null(),
    }
}

/// The string's UTF-16 characters, valid while the string lives; null if it is Latin-1.
pub fn two_byte_chars(s: *mut JSString, length: &mut usize) -> *const u16 {
    match chars_of(s) {
        StringChars::TwoByte(chars) => {
            *length = chars.len();
            chars.as_ptr()
        },
        StringChars::Latin1(_) => std::ptr::null(),
    }
}

/// A new string from UTF-8 (`JS_NewStringCopyUTF8N`); null on failure.
pub fn new_string_utf8(cx: &JSContext, chars: &UTF8Chars) -> *mut JSString {
    // SAFETY: `UTF8Chars` borrows valid UTF-8 for its lifetime (see `Utf8Chars`).
    let bytes = unsafe { std::slice::from_raw_parts(chars.start, chars.length) };
    let Ok(text) = std::str::from_utf8(bytes) else { return std::ptr::null_mut() };
    cx.catching(|scope| {
        let string = v8::String::new(scope, text)?;
        Some(from_v8(scope, string.into()).to_string())
    })
    .unwrap_or(std::ptr::null_mut())
}

/// A new string from UTF-16 code units (`JS_NewUCStringCopyN`); null on failure.
pub fn new_string_utf16(cx: &JSContext, units: &[u16]) -> *mut JSString {
    cx.catching(|scope| {
        let string = v8::String::new_from_two_byte(scope, units, v8::NewStringType::Normal)?;
        Some(from_v8(scope, string.into()).to_string())
    })
    .unwrap_or(std::ptr::null_mut())
}

// --- Arrays and iteration ------------------------------------------------------------------

/// A new array of `length` holes (`JS::NewArrayObject`).
pub fn new_array(cx: &JSContext, length: usize) -> *mut JSObject {
    cx.catching(|scope| {
        let array = v8::Array::new(scope, length as i32);
        Some(from_v8(scope, array.into()).to_object())
    })
    .unwrap_or(std::ptr::null_mut())
}

/// `obj[index] = value` as a data property (`JS_DefineElement`).
pub fn define_element(cx: &JSContext, obj: HandleObject, index: u32, value: HandleValue, attrs: u32) -> bool {
    cx.catching(|scope| {
        // SAFETY: handles refer to rooted locations.
        let target = unsafe { to_v8(scope, crate::jsval::ObjectValue(obj.get())) };
        let target = v8::Local::<v8::Object>::try_from(target).ok()?;
        // SAFETY: as above.
        let value = unsafe { to_v8(scope, value.get()) };
        let key = v8::String::new(scope, &index.to_string())?;
        let attributes = property_attributes(attrs);
        target.define_own_property(scope, key.into(), value, attributes)
    })
    .unwrap_or(false)
}

/// V8 attributes for `JSPROP_*` flags.
pub(crate) fn property_attributes(flags: u32) -> v8::PropertyAttribute {
    let mut attributes = v8::PropertyAttribute::NONE;
    if flags & JSPROP_ENUMERATE == 0 {
        attributes = attributes | v8::PropertyAttribute::DONT_ENUM;
    }
    if flags & JSPROP_READONLY != 0 {
        attributes = attributes | v8::PropertyAttribute::READ_ONLY;
    }
    if flags & JSPROP_PERMANENT != 0 {
        attributes = attributes | v8::PropertyAttribute::DONT_DELETE;
    }
    attributes
}

/// Why `for_of` stopped early.
pub enum ForOfIterationFailure<OtherError> {
    ValueIsNotIterable,
    /// There is a pending exception.
    JSFailed,
    Other(OtherError),
}

/// Iterates `iterable` with the JS iterator protocol, calling `callback` with each element.
pub fn for_of<Callback, OtherError>(
    cx: *mut JSContext,
    iterable: HandleValue<'_>,
    mut callback: Callback,
) -> Result<(), ForOfIterationFailure<OtherError>>
where
    Callback: FnMut(HandleValue<'_>) -> Result<ControlFlow<()>, ForOfIterationFailure<OtherError>>,
{
    let cx = raw(cx);
    // Get the iterator and its `next` method.
    let iterator = cx.catching(|scope| {
        // SAFETY: a handle refers to a rooted location.
        let value = unsafe { to_v8(scope, iterable.get()) };
        let Ok(object) = v8::Local::<v8::Object>::try_from(value) else { return Some(None) };
        let symbol = v8::Symbol::get_iterator(scope);
        let method = object.get(scope, symbol.into())?;
        let Ok(method) = v8::Local::<v8::Function>::try_from(method) else { return Some(None) };
        let iterator = method.call(scope, object.into(), &[])?;
        let Ok(iterator) = v8::Local::<v8::Object>::try_from(iterator) else {
            let message = v8::String::new(scope, "iterator is not an object")?;
            let error = v8::Exception::type_error(scope, message);
            scope.throw_exception(error);
            return None;
        };
        Some(Some((v8::Global::new(scope, iterator), from_v8(scope, iterator.into()))))
    });
    let (iterator, _) = match iterator {
        None => return Err(ForOfIterationFailure::JSFailed),
        Some(None) => return Err(ForOfIterationFailure::ValueIsNotIterable),
        Some(Some(iterator)) => iterator,
    };
    loop {
        let step = cx.catching(|scope| {
            let iterator = v8::Local::new(scope, &iterator);
            let next_key = v8::String::new(scope, "next")?;
            let next = iterator.get(scope, next_key.into())?;
            let Ok(next) = v8::Local::<v8::Function>::try_from(next) else {
                let message = v8::String::new(scope, "iterator has no next method")?;
                let error = v8::Exception::type_error(scope, message);
                scope.throw_exception(error);
                return None;
            };
            let result = next.call(scope, iterator.into(), &[])?;
            let Ok(result) = v8::Local::<v8::Object>::try_from(result) else {
                let message = v8::String::new(scope, "iterator result is not an object")?;
                let error = v8::Exception::type_error(scope, message);
                scope.throw_exception(error);
                return None;
            };
            let done_key = v8::String::new(scope, "done")?;
            if result.get(scope, done_key.into())?.boolean_value(scope) {
                return Some(None);
            }
            let value_key = v8::String::new(scope, "value")?;
            let value = result.get(scope, value_key.into())?;
            Some(Some(from_v8(scope, value)))
        });
        let element = match step {
            None => return Err(ForOfIterationFailure::JSFailed),
            Some(None) => return Ok(()),
            Some(Some(element)) => element,
        };
        let raw_cx = cx as *const JSContext as *mut JSContext;
        crate::rooted!(in(raw_cx) let element = element);
        match callback(element.handle())? {
            ControlFlow::Continue(()) => {},
            ControlFlow::Break(()) => {
                // Close the iterator (IteratorClose), ignoring its result.
                cx.catching(|scope| {
                    let iterator = v8::Local::new(scope, &iterator);
                    let return_key = v8::String::new(scope, "return")?;
                    let method = iterator.get(scope, return_key.into())?;
                    if let Ok(method) = v8::Local::<v8::Function>::try_from(method) {
                        method.call(scope, iterator.into(), &[])?;
                    }
                    Some(())
                });
                return Ok(());
            },
        }
    }
}

/// Cross-compartment wrapping does not exist on V8: objects are usable in any realm of the
/// isolate. These keep mozjs's call sites compiling.
pub fn maybe_wrap_value(_cx: &mut SafeJSContext, _rval: MutableHandleValue) {}
pub fn maybe_wrap_object_or_null_value(_cx: &mut SafeJSContext, _rval: MutableHandleValue) {}
/// # Safety
/// As in mozjs (a live object value).
pub unsafe fn maybe_wrap_object_value(_cx: &mut SafeJSContext, _rval: MutableHandleValue) {}

/// No compartments on V8 (see `maybe_wrap_value`).
pub fn assert_same_compartment(_cx: &SafeJSContext, _obj: *mut JSObject) {}

/// A `Cow` message for conversion failures.
pub type FailureMessage = Cow<'static, CStr>;
