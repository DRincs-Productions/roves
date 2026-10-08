/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `JSVal`: a 64-bit value with SpiderMonkey's punboxing layout. Doubles are stored as their
//! bits; every other kind sets a 17-bit tag above a 47-bit payload. A GC thing's payload is its
//! cell pointer (see `cell`), so values stay `Copy` and pointer-comparable, as in mozjs.

use std::ffi::c_void;

use crate::jsapi::{BigInt, JSObject, JSString, Symbol};

const TAG_SHIFT: u64 = 47;
const PAYLOAD_MASK: u64 = (1 << TAG_SHIFT) - 1;
/// Tags above this (shifted) mark a non-double value; any tag at or below is a double.
const TAG_MAX_DOUBLE: u64 = 0x1FFF0;
const TAG_INT32: u64 = 0x1FFF1;
const TAG_UNDEFINED: u64 = 0x1FFF2;
const TAG_NULL: u64 = 0x1FFF3;
const TAG_BOOLEAN: u64 = 0x1FFF4;
const TAG_MAGIC: u64 = 0x1FFF5;
const TAG_STRING: u64 = 0x1FFF6;
const TAG_SYMBOL: u64 = 0x1FFF7;
const TAG_PRIVATE: u64 = 0x1FFF8;
const TAG_BIGINT: u64 = 0x1FFF9;
const TAG_OBJECT: u64 = 0x1FFFC;
const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

/// A JS value, `Copy` and 64 bits wide like SpiderMonkey's `JS::Value`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Value {
    bits: u64,
}

pub type JSVal = Value;

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSVal({:#x})", self.bits)
    }
}

fn tagged(tag: u64, payload: u64) -> JSVal {
    debug_assert!(payload <= PAYLOAD_MASK, "payload does not fit in 47 bits");
    JSVal { bits: (tag << TAG_SHIFT) | payload }
}

fn gc_thing(tag: u64, pointer: *const c_void) -> JSVal {
    let address = pointer as u64;
    assert!(address <= PAYLOAD_MASK, "GC-thing pointer above the 47-bit user address space");
    tagged(tag, address)
}

#[inline]
pub fn NullValue() -> JSVal {
    tagged(TAG_NULL, 0)
}

#[inline]
pub fn UndefinedValue() -> JSVal {
    tagged(TAG_UNDEFINED, 0)
}

#[inline]
pub fn Int32Value(i: i32) -> JSVal {
    tagged(TAG_INT32, i as u32 as u64)
}

#[inline]
pub fn DoubleValue(f: f64) -> JSVal {
    let bits = if f.is_nan() { CANONICAL_NAN } else { f.to_bits() };
    JSVal { bits }
}

#[inline]
pub fn UInt32Value(ui: u32) -> JSVal {
    if ui > i32::MAX as u32 { DoubleValue(ui as f64) } else { Int32Value(ui as i32) }
}

#[inline]
pub fn BooleanValue(b: bool) -> JSVal {
    tagged(TAG_BOOLEAN, b as u64)
}

#[inline]
pub fn StringValue(s: &JSString) -> JSVal {
    gc_thing(TAG_STRING, s as *const JSString as *const c_void)
}

#[inline]
pub fn ObjectValue(o: *mut JSObject) -> JSVal {
    assert!(!o.is_null(), "ObjectValue of a null object");
    gc_thing(TAG_OBJECT, o as *const c_void)
}

#[inline]
pub fn ObjectOrNullValue(o: *mut JSObject) -> JSVal {
    if o.is_null() { NullValue() } else { ObjectValue(o) }
}

#[inline]
pub fn SymbolValue(s: &Symbol) -> JSVal {
    gc_thing(TAG_SYMBOL, s as *const Symbol as *const c_void)
}

#[inline]
pub fn BigIntValue(b: &BigInt) -> JSVal {
    gc_thing(TAG_BIGINT, b as *const BigInt as *const c_void)
}

#[inline]
pub fn PrivateValue(o: *const c_void) -> JSVal {
    gc_thing(TAG_PRIVATE, o)
}

impl Default for JSVal {
    fn default() -> JSVal {
        UndefinedValue()
    }
}

impl JSVal {
    #[inline]
    fn tag(&self) -> u64 {
        self.bits >> TAG_SHIFT
    }

    #[inline]
    fn payload(&self) -> u64 {
        self.bits & PAYLOAD_MASK
    }

    /// The raw 64 bits (`asBits_` in SpiderMonkey).
    pub fn as_bits(&self) -> u64 {
        self.bits
    }

    pub fn is_undefined(&self) -> bool {
        self.tag() == TAG_UNDEFINED
    }

    pub fn is_null(&self) -> bool {
        self.tag() == TAG_NULL
    }

    pub fn is_null_or_undefined(&self) -> bool {
        self.is_null() || self.is_undefined()
    }

    pub fn is_boolean(&self) -> bool {
        self.tag() == TAG_BOOLEAN
    }

    pub fn is_int32(&self) -> bool {
        self.tag() == TAG_INT32
    }

    pub fn is_double(&self) -> bool {
        self.tag() <= TAG_MAX_DOUBLE
    }

    pub fn is_number(&self) -> bool {
        self.is_int32() || self.is_double()
    }

    pub fn is_string(&self) -> bool {
        self.tag() == TAG_STRING
    }

    pub fn is_object(&self) -> bool {
        self.tag() == TAG_OBJECT
    }

    pub fn is_object_or_null(&self) -> bool {
        self.is_object() || self.is_null()
    }

    pub fn is_symbol(&self) -> bool {
        self.tag() == TAG_SYMBOL
    }

    pub fn is_bigint(&self) -> bool {
        self.tag() == TAG_BIGINT
    }

    pub fn is_magic(&self) -> bool {
        self.tag() == TAG_MAGIC
    }

    pub fn is_primitive(&self) -> bool {
        !self.is_object()
    }

    /// Whether the value refers to a GC thing (a cell).
    pub fn is_gcthing(&self) -> bool {
        matches!(self.tag(), TAG_STRING | TAG_SYMBOL | TAG_BIGINT | TAG_OBJECT)
    }

    pub fn is_markable(&self) -> bool {
        self.is_gcthing()
    }

    pub fn to_boolean(&self) -> bool {
        assert!(self.is_boolean());
        self.payload() != 0
    }

    pub fn to_int32(&self) -> i32 {
        assert!(self.is_int32());
        self.payload() as u32 as i32
    }

    pub fn to_double(&self) -> f64 {
        assert!(self.is_double());
        f64::from_bits(self.bits)
    }

    pub fn to_number(&self) -> f64 {
        if self.is_int32() { self.to_int32() as f64 } else { self.to_double() }
    }

    pub fn to_string(&self) -> *mut JSString {
        assert!(self.is_string());
        self.payload() as *mut JSString
    }

    pub fn to_object(&self) -> *mut JSObject {
        assert!(self.is_object());
        self.payload() as *mut JSObject
    }

    pub fn to_object_or_null(&self) -> *mut JSObject {
        if self.is_null() { std::ptr::null_mut() } else { self.to_object() }
    }

    pub fn to_symbol(&self) -> *mut Symbol {
        assert!(self.is_symbol());
        self.payload() as *mut Symbol
    }

    pub fn to_bigint(&self) -> *mut BigInt {
        assert!(self.is_bigint());
        self.payload() as *mut BigInt
    }

    pub fn to_private(&self) -> *const c_void {
        assert!(self.tag() == TAG_PRIVATE);
        self.payload() as *const c_void
    }

    /// The cell pointer of a GC-thing value.
    pub fn to_gcthing(&self) -> *mut c_void {
        assert!(self.is_gcthing());
        self.payload() as *mut c_void
    }
}

/// Converts a V8 value to a `JSVal`, allocating a cell for a GC thing. The result is
/// unrooted (see `cell`).
pub(crate) fn from_v8(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> JSVal {
    if value.is_undefined() {
        UndefinedValue()
    } else if value.is_null() {
        NullValue()
    } else if value.is_boolean() {
        BooleanValue(value.is_true())
    } else if value.is_int32() {
        Int32Value(value.int32_value(scope).unwrap_or(0))
    } else if value.is_number() {
        DoubleValue(value.number_value(scope).unwrap_or(f64::NAN))
    } else {
        let cell = crate::cell::new_cell(scope, value);
        let tag = if value.is_string() {
            TAG_STRING
        } else if value.is_symbol() {
            TAG_SYMBOL
        } else if value.is_big_int() {
            TAG_BIGINT
        } else {
            TAG_OBJECT
        };
        gc_thing(tag, cell)
    }
}

/// The V8 value of a `JSVal`.
///
/// # Safety
/// A GC-thing value's cell must be alive (rooted, traced, or created since the last GC).
pub(crate) unsafe fn to_v8<'s>(scope: &mut v8::PinScope<'s, '_>, value: JSVal) -> v8::Local<'s, v8::Value> {
    if value.is_gcthing() {
        // SAFETY: forwarded to the caller.
        return unsafe { crate::cell::cell_value(scope, value.to_gcthing()) };
    }
    if value.is_undefined() {
        v8::undefined(scope).into()
    } else if value.is_null() {
        v8::null(scope).into()
    } else if value.is_boolean() {
        v8::Boolean::new(scope, value.to_boolean()).into()
    } else if value.is_int32() {
        v8::Integer::new(scope, value.to_int32()).into()
    } else if value.is_double() {
        v8::Number::new(scope, value.to_double()).into()
    } else {
        // Magic and private values have no JS representation.
        v8::undefined(scope).into()
    }
}
