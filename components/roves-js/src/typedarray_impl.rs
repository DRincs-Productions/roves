/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The JSAPI typed-array and ArrayBuffer functions under mozjs's `typedarray` module.
//!
//! SpiderMonkey lends raw data pointers that stay valid while no GC runs. V8 keeps small
//! typed arrays' elements on its moving heap, so every data access first asks the view for
//! its buffer, which moves the elements to a stable off-heap backing store.

use std::ffi::c_void;

use crate::jsapi::{AutoRequireNoGC, JSContext, JSObject, Type};
use crate::jsval::from_v8;

fn with_value<R>(obj: *mut JSObject, f: impl FnOnce(&mut v8::PinScope, v8::Local<v8::Value>) -> R) -> R {
    JSContext::current().with_scope(|scope| {
        // SAFETY: callers pass live objects.
        let value = unsafe { crate::cell::cell_value(scope, obj as *mut c_void) };
        f(scope, value)
    })
}

/// A view's (or buffer's) stable data pointer, byte length and sharedness.
fn data_of(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> Option<(*mut u8, usize, bool)> {
    if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) {
        let shared = view.has_buffer() && view.buffer(scope).is_some_and(|buffer| buffer.is_shared_array_buffer());
        let buffer = view.buffer(scope)?;
        let base = buffer.data().map_or(std::ptr::null_mut(), |data| data.as_ptr() as *mut u8);
        // SAFETY: the view lies within its buffer.
        let data = if base.is_null() { base } else { unsafe { base.add(view.byte_offset()) } };
        return Some((data, view.byte_length(), shared));
    }
    if let Ok(buffer) = v8::Local::<v8::ArrayBuffer>::try_from(value) {
        let data = buffer.data().map_or(std::ptr::null_mut(), |data| data.as_ptr() as *mut u8);
        return Some((data, buffer.byte_length(), false));
    }
    if let Ok(buffer) = v8::Local::<v8::SharedArrayBuffer>::try_from(value) {
        // The backing store outlives this reference: the buffer keeps it.
        let data = buffer.get_backing_store().data().map_or(std::ptr::null_mut(), |data| data.as_ptr() as *mut u8);
        return Some((data, buffer.byte_length(), true));
    }
    None
}

fn element_size(kind: Type) -> usize {
    match kind {
        Type::Int8 | Type::Uint8 | Type::Uint8Clamped => 1,
        Type::Int16 | Type::Uint16 | Type::Float16 => 2,
        Type::Int32 | Type::Uint32 | Type::Float32 => 4,
        Type::Float64 | Type::BigInt64 | Type::BigUint64 | Type::Int64 => 8,
        Type::Simd128 => 16,
        Type::MaxTypedArrayViewType => 1,
    }
}

fn view_type(value: v8::Local<v8::Value>) -> Option<Type> {
    Some(if value.is_int8_array() {
        Type::Int8
    } else if value.is_uint8_array() {
        Type::Uint8
    } else if value.is_uint8_clamped_array() {
        Type::Uint8Clamped
    } else if value.is_int16_array() {
        Type::Int16
    } else if value.is_uint16_array() {
        Type::Uint16
    } else if value.is_int32_array() {
        Type::Int32
    } else if value.is_uint32_array() {
        Type::Uint32
    } else if value.is_float16_array() {
        Type::Float16
    } else if value.is_float32_array() {
        Type::Float32
    } else if value.is_float64_array() {
        Type::Float64
    } else if value.is_big_int64_array() {
        Type::BigInt64
    } else if value.is_big_uint64_array() {
        Type::BigUint64
    } else if value.is_data_view() {
        Type::MaxTypedArrayViewType
    } else {
        return None;
    })
}

pub unsafe fn JS_GetArrayBufferViewType(obj: *mut JSObject) -> Type {
    with_value(obj, |_, value| view_type(value).unwrap_or(Type::MaxTypedArrayViewType))
}

pub unsafe fn JS_IsArrayBufferViewObject(obj: *mut JSObject) -> bool {
    with_value(obj, |_, value| value.is_array_buffer_view())
}

pub unsafe fn JS_IsTypedArrayObject(obj: *mut JSObject) -> bool {
    with_value(obj, |_, value| value.is_typed_array())
}

pub unsafe fn IsArrayBufferObject(obj: *mut JSObject) -> bool {
    with_value(obj, |_, value| value.is_array_buffer())
}

pub unsafe fn IsSharedArrayBufferObject(obj: *mut JSObject) -> bool {
    with_value(obj, |_, value| value.is_shared_array_buffer())
}

pub unsafe fn JS_GetTypedArraySharedness(obj: *mut JSObject) -> bool {
    with_value(obj, |scope, value| data_of(scope, value).is_some_and(|(_, _, shared)| shared))
}

pub unsafe fn UnwrapArrayBuffer(obj: *mut JSObject) -> *mut JSObject {
    with_value(obj, |_, value| if value.is_array_buffer() || value.is_shared_array_buffer() { obj } else { std::ptr::null_mut() })
}

pub unsafe fn UnwrapArrayBufferView(obj: *mut JSObject) -> *mut JSObject {
    with_value(obj, |_, value| if value.is_array_buffer_view() { obj } else { std::ptr::null_mut() })
}

pub unsafe fn UnwrapSharedArrayBuffer(obj: *mut JSObject) -> *mut JSObject {
    with_value(obj, |_, value| if value.is_shared_array_buffer() { obj } else { std::ptr::null_mut() })
}

/// Writes `obj`'s length (in elements of `kind`, or bytes), sharedness and data pointer.
unsafe fn length_and_data<T>(obj: *mut JSObject, kind: Option<Type>, length: *mut usize, is_shared: *mut bool, data: *mut *mut T) {
    let found = with_value(obj, |scope, value| data_of(scope, value));
    let (pointer, bytes, shared) = found.unwrap_or((std::ptr::null_mut(), 0, false));
    // SAFETY: callers pass valid out pointers.
    unsafe {
        *length = bytes / kind.map_or(1, element_size);
        *is_shared = shared;
        *data = pointer as *mut T;
    }
}

pub unsafe fn GetArrayBufferLengthAndData(obj: *mut JSObject, length: *mut usize, is_shared: *mut bool, data: *mut *mut u8) {
    // SAFETY: forwarded.
    unsafe { length_and_data(obj, None, length, is_shared, data) }
}

pub unsafe fn GetArrayBufferViewLengthAndData(obj: *mut JSObject, length: *mut usize, is_shared: *mut bool, data: *mut *mut u8) {
    // SAFETY: forwarded.
    unsafe { length_and_data(obj, None, length, is_shared, data) }
}

pub unsafe fn GetArrayBufferData(obj: *mut JSObject, is_shared: *mut bool, _nogc: *const AutoRequireNoGC) -> *mut u8 {
    let mut length = 0;
    let mut data = std::ptr::null_mut();
    // SAFETY: forwarded.
    unsafe { length_and_data(obj, None, &mut length, is_shared, &mut data) };
    data
}

pub unsafe fn GetArrayBufferByteLength(obj: *mut JSObject) -> usize {
    with_value(obj, |scope, value| data_of(scope, value).map_or(0, |(_, bytes, _)| bytes))
}

pub unsafe fn JS_GetArrayBufferViewByteLength(obj: *mut JSObject) -> usize {
    // SAFETY: forwarded.
    unsafe { GetArrayBufferByteLength(obj) }
}

pub unsafe fn JS_GetArrayBufferViewByteOffset(obj: *mut JSObject) -> usize {
    with_value(obj, |_, value| v8::Local::<v8::ArrayBufferView>::try_from(value).map_or(0, |view| view.byte_offset()))
}

pub unsafe fn JS_GetTypedArrayLength(obj: *mut JSObject) -> usize {
    with_value(obj, |_, value| v8::Local::<v8::TypedArray>::try_from(value).map_or(0, |array| array.length()))
}

/// The buffer of a view (materialized off-heap).
pub unsafe fn JS_GetArrayBufferViewBuffer(cx: *mut JSContext, obj: crate::jsapi::HandleObject, is_shared: *mut bool) -> *mut JSObject {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let result = raw.catching(|scope| {
        // SAFETY: callers pass live objects.
        let value = unsafe { crate::cell::cell_value(scope, obj.get() as *mut c_void) };
        let view = v8::Local::<v8::ArrayBufferView>::try_from(value).ok()?;
        let buffer = view.buffer(scope)?;
        Some((from_v8(scope, buffer.into()).to_object(), buffer.is_shared_array_buffer()))
    });
    match result {
        Some((buffer, shared)) => {
            // SAFETY: callers pass a valid out pointer.
            unsafe { *is_shared = shared };
            buffer
        },
        None => std::ptr::null_mut(),
    }
}

pub unsafe fn NewArrayBuffer(cx: *mut JSContext, nbytes: usize) -> *mut JSObject {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    raw.catching(|scope| {
        let buffer = v8::ArrayBuffer::new(scope, nbytes);
        Some(from_v8(scope, buffer.into()).to_object())
    })
    .unwrap_or(std::ptr::null_mut())
}

macro_rules! typed_array {
    ($v8:ident, $kind:ident, $element:ty, $is:ident, $new:ident, $get_data:ident, $length_and_data:ident, $unwrap:ident) => {
        pub unsafe fn $new(cx: *mut JSContext, length: usize) -> *mut JSObject {
            // SAFETY: callers pass a live context.
            let raw = unsafe { &*cx };
            raw.catching(|scope| {
                let buffer = v8::ArrayBuffer::new(scope, length * std::mem::size_of::<$element>());
                let array = v8::$v8::new(scope, buffer, 0, length)?;
                Some(from_v8(scope, array.into()).to_object())
            })
            .unwrap_or(std::ptr::null_mut())
        }

        pub unsafe fn $get_data(obj: *mut JSObject, is_shared: *mut bool, _nogc: *const AutoRequireNoGC) -> *mut $element {
            let mut length = 0;
            let mut data = std::ptr::null_mut();
            // SAFETY: forwarded.
            unsafe { length_and_data(obj, Some(Type::$kind), &mut length, is_shared, &mut data) };
            data
        }

        pub unsafe fn $length_and_data(obj: *mut JSObject, length: *mut usize, is_shared: *mut bool, data: *mut *mut $element) {
            // SAFETY: forwarded.
            unsafe { length_and_data(obj, Some(Type::$kind), length, is_shared, data) }
        }

        pub unsafe fn $unwrap(obj: *mut JSObject) -> *mut JSObject {
            with_value(obj, |_, value| if value.$is() { obj } else { std::ptr::null_mut() })
        }
    };
}

typed_array!(Int8Array, Int8, i8, is_int8_array, JS_NewInt8Array, JS_GetInt8ArrayData, GetInt8ArrayLengthAndData, UnwrapInt8Array);
typed_array!(Uint8Array, Uint8, u8, is_uint8_array, JS_NewUint8Array, JS_GetUint8ArrayData, GetUint8ArrayLengthAndData, UnwrapUint8Array);
typed_array!(
    Uint8ClampedArray,
    Uint8Clamped,
    u8,
    is_uint8_clamped_array,
    JS_NewUint8ClampedArray,
    JS_GetUint8ClampedArrayData,
    GetUint8ClampedArrayLengthAndData,
    UnwrapUint8ClampedArray
);
typed_array!(Int16Array, Int16, i16, is_int16_array, JS_NewInt16Array, JS_GetInt16ArrayData, GetInt16ArrayLengthAndData, UnwrapInt16Array);
typed_array!(Uint16Array, Uint16, u16, is_uint16_array, JS_NewUint16Array, JS_GetUint16ArrayData, GetUint16ArrayLengthAndData, UnwrapUint16Array);
typed_array!(Int32Array, Int32, i32, is_int32_array, JS_NewInt32Array, JS_GetInt32ArrayData, GetInt32ArrayLengthAndData, UnwrapInt32Array);
typed_array!(Uint32Array, Uint32, u32, is_uint32_array, JS_NewUint32Array, JS_GetUint32ArrayData, GetUint32ArrayLengthAndData, UnwrapUint32Array);
typed_array!(Float32Array, Float32, f32, is_float32_array, JS_NewFloat32Array, JS_GetFloat32ArrayData, GetFloat32ArrayLengthAndData, UnwrapFloat32Array);
typed_array!(Float64Array, Float64, f64, is_float64_array, JS_NewFloat64Array, JS_GetFloat64ArrayData, GetFloat64ArrayLengthAndData, UnwrapFloat64Array);
