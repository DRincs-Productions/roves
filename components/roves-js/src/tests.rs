/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::cell::FINALIZED;
use crate::jsapi::JSObject;
use crate::jsval::*;
use crate::rust::Runtime;

/// Evaluates `source` in the runtime's realm and returns the (unrooted) result.
fn eval(runtime: &Runtime, source: &str) -> JSVal {
    // SAFETY: the runtime owns its raw context.
    let raw = unsafe { &*runtime.cx() };
    raw.with_scope(|scope| {
        let code = v8::String::new(scope, source).unwrap();
        let script = v8::Script::compile(scope, code, None).unwrap();
        let result = script.run(scope).unwrap();
        from_v8(scope, result)
    })
}

/// Reads `value` back as a string through V8 (`String(value)`).
fn describe(runtime: &Runtime, value: JSVal) -> String {
    // SAFETY: the runtime owns its raw context.
    let raw = unsafe { &*runtime.cx() };
    raw.with_scope(|scope| {
        // SAFETY: tests only describe live (rooted or fresh) values.
        let local = unsafe { to_v8(scope, value) };
        local.to_rust_string_lossy(scope)
    })
}

fn finalized() -> usize {
    FINALIZED.with(|count| count.get())
}

#[test]
fn immediates_round_trip_with_spidermonkey_semantics() {
    assert!(UndefinedValue().is_undefined() && NullValue().is_null() && NullValue().is_null_or_undefined());
    assert_eq!(Int32Value(-7).to_int32(), -7);
    assert!(Int32Value(5).is_number() && !Int32Value(5).is_double());
    assert_eq!(DoubleValue(1.5).to_double(), 1.5);
    assert!(DoubleValue(f64::NAN).to_double().is_nan());
    assert_eq!(DoubleValue(-f64::NAN).as_bits(), DoubleValue(f64::NAN).as_bits(), "NaNs are canonical");
    assert_eq!(DoubleValue(f64::NEG_INFINITY).to_number(), f64::NEG_INFINITY);
    assert!(UInt32Value(u32::MAX).is_double() && UInt32Value(7).is_int32());
    assert!(BooleanValue(true).to_boolean() && !BooleanValue(false).to_boolean());
    assert!(ObjectOrNullValue(std::ptr::null_mut()).is_null());
    assert_eq!(JSVal::default(), UndefinedValue());
    assert!(!Int32Value(0).is_gcthing() && UndefinedValue().is_primitive());
}

#[test]
fn v8_values_convert_to_jsvals_and_back() {
    let runtime = Runtime::new();
    for (source, expected) in [("undefined", "undefined"), ("null", "null"), ("true", "true"), ("42", "42"), ("0.25", "0.25")] {
        assert_eq!(describe(&runtime, eval(&runtime, source)), expected);
    }
    let text = eval(&runtime, "'hello'");
    assert!(text.is_string() && text.is_gcthing());
    assert_eq!(describe(&runtime, text), "hello");
    let symbol = eval(&runtime, "Symbol('s')");
    assert!(symbol.is_symbol());
    let big = eval(&runtime, "10n");
    assert!(big.is_bigint());
    assert_eq!(describe(&runtime, big), "10");
    let object = eval(&runtime, "({ toString() { return 'obj'; } })");
    assert!(object.is_object() && !object.is_primitive());
    assert_eq!(describe(&runtime, object), "obj");
    assert_eq!(ObjectValue(object.to_object()), object);
}

#[test]
fn rooted_values_survive_gc_and_unrooted_cells_are_collected() {
    let mut runtime = Runtime::new();
    let cx = runtime.cx();
    let before = finalized();
    {
        rooted!(in(cx) let kept = eval(&runtime, "({ marker: 'kept' })"));
        rooted!(in(cx) let mut object = std::ptr::null_mut::<JSObject>());
        object.set(eval(&runtime, "({ toString() { return 'object root'; } })").to_object());
        // An unrooted value: its cell is unreachable at the next GC.
        let _unrooted = eval(&runtime, "({})");
        runtime.gc_for_testing();
        assert!(finalized() >= before + 1, "the unrooted cell is collected");
        let after_first = finalized();
        // The rooted cells are alive and still hold their objects.
        assert_eq!(describe(&runtime, ObjectValue(kept.to_object())), "[object Object]");
        assert_eq!(describe(&runtime, ObjectValue(object.get())), "object root");
        runtime.gc_for_testing();
        assert_eq!(finalized(), after_first, "rooted cells are not collected");
    }
    // Leaving the scope unroots both cells.
    let before_unroot = finalized();
    runtime.gc_for_testing();
    assert_eq!(finalized(), before_unroot + 2, "unrooted cells are collected once");
}

#[test]
fn handles_read_and_write_rooted_locations() {
    let mut runtime = Runtime::new();
    let cx = runtime.cx();
    rooted!(in(cx) let mut value = UndefinedValue());
    {
        let mut handle = value.handle_mut();
        handle.set(eval(&runtime, "'via handle'"));
    }
    let read = value.handle();
    assert!(read.get().is_string());
    runtime.gc_for_testing();
    assert_eq!(describe(&runtime, value.get()), "via handle");
    // Roots of compound types report every GC thing they hold.
    rooted!(in(cx) let list = vec![eval(&runtime, "'a'"), Int32Value(1), eval(&runtime, "'b'")]);
    runtime.gc_for_testing();
    assert_eq!(describe(&runtime, list[0]), "a");
    assert_eq!(describe(&runtime, list[2]), "b");
}

#[test]
fn heap_locations_are_traced_by_their_owner() {
    use crate::gc::Heap;
    use v8::cppgc::{GarbageCollected, Persistent, Visitor};

    /// A traced owner (like a DOM object) holding a `Heap<JSVal>`.
    struct Owner {
        slot: Heap<JSVal>,
    }

    unsafe impl GarbageCollected for Owner {
        fn trace(&self, visitor: &mut Visitor) {
            self.slot.trace(visitor);
        }

        fn get_name(&self) -> &'static std::ffi::CStr {
            c"Owner"
        }
    }

    let mut runtime = Runtime::new();
    // SAFETY: the runtime owns its raw context.
    let raw = unsafe { &*runtime.cx() };
    let owner = raw.with_scope(|scope| {
        let heap = scope.get_cpp_heap().unwrap();
        // SAFETY: moved straight into a Persistent.
        let owner = unsafe { v8::cppgc::make_garbage_collected(heap, Owner { slot: Heap::default() }) };
        Persistent::new(&owner)
    });
    owner.get().unwrap().slot.set(eval(&runtime, "'held by the owner'"));
    let before = finalized();
    runtime.gc_for_testing();
    assert_eq!(finalized(), before, "the owner keeps its heap location's cell alive");
    assert_eq!(describe(&runtime, owner.get().unwrap().slot.get()), "held by the owner");
    owner.get().unwrap().slot.set(UndefinedValue());
    runtime.gc_for_testing();
    assert_eq!(finalized(), before + 1, "an overwritten heap location releases its cell");
    drop(owner);
}

#[test]
fn conversions_follow_webidl_and_mozjs() {
    use crate::conversions::{ConversionBehavior, ConversionResult, FromJSValConvertible, ToJSValConvertible};

    fn from<T: FromJSValConvertible>(runtime: &mut Runtime, source: &str, option: T::Config) -> Result<ConversionResult<T>, ()> {
        let value = eval(runtime, source);
        let raw = runtime.cx();
        rooted!(in(raw) let value = value);
        let mut cx = runtime.cx_mut();
        T::safe_from_jsval(&mut cx, value.handle(), option)
    }
    fn ok<T>(result: Result<ConversionResult<T>, ()>) -> T {
        match result {
            Ok(ConversionResult::Success(value)) => value,
            _ => panic!("conversion failed"),
        }
    }

    let mut runtime = Runtime::new();
    // Integers: modular by default, EnforceRange throws, Clamp rounds half to even.
    assert_eq!(ok(from::<i32>(&mut runtime, "2 ** 32 + 5", ConversionBehavior::Default)), 5);
    assert_eq!(ok(from::<u8>(&mut runtime, "-1", ConversionBehavior::Default)), 255);
    assert_eq!(ok(from::<u8>(&mut runtime, "300.7", ConversionBehavior::Clamp)), 255);
    assert_eq!(ok(from::<u8>(&mut runtime, "2.5", ConversionBehavior::Clamp)), 2);
    assert_eq!(ok(from::<i64>(&mut runtime, "'-12'", ConversionBehavior::Default)), -12);
    assert!(from::<u8>(&mut runtime, "256", ConversionBehavior::EnforceRange).is_err());
    // SAFETY: a live context.
    assert!(unsafe { crate::jsapi::JS_IsExceptionPending(runtime.cx()) });
    unsafe { crate::jsapi::JS_ClearPendingException(runtime.cx()) };
    // ToNumber runs user code and propagates its exception as the pending one.
    assert!(from::<f64>(&mut runtime, "({ valueOf() { throw new RangeError('mine'); } })", ()).is_err());
    let raw = runtime.cx();
    rooted!(in(raw) let mut exception = UndefinedValue());
    assert!(unsafe { crate::jsapi::JS_GetPendingException(raw, exception.handle_mut()) });
    assert_eq!(describe(&runtime, exception.get()), "RangeError: mine");
    unsafe { crate::jsapi::JS_ClearPendingException(raw) };
    assert!(ok(from::<bool>(&mut runtime, "'x'", ())) && !ok(from::<bool>(&mut runtime, "''", ())));
    assert_eq!(ok(from::<f64>(&mut runtime, "'1.5'", ())), 1.5);
    // Strings: Latin-1 and two-byte, through ToString.
    assert_eq!(ok(from::<String>(&mut runtime, "'caf\\u00e9'", ())), "café");
    assert_eq!(ok(from::<String>(&mut runtime, "'\\u4e2d\\u6587'", ())), "中文");
    assert_eq!(ok(from::<String>(&mut runtime, "({ toString() { return 'custom'; } })", ())), "custom");
    // Option and sequences (any iterable).
    assert_eq!(ok(from::<Option<i32>>(&mut runtime, "null", ConversionBehavior::Default)), None);
    assert_eq!(ok(from::<Vec<i32>>(&mut runtime, "new Set([3, 1, 2])", ConversionBehavior::Default)), vec![3, 1, 2]);
    assert!(matches!(from::<Vec<i32>>(&mut runtime, "5", ConversionBehavior::Default), Ok(ConversionResult::Failure(_))));
    // Non-objects are rejected as objects, with a pending TypeError.
    assert!(from::<*mut JSObject>(&mut runtime, "1", ()).is_err());
    unsafe { crate::jsapi::JS_ClearPendingException(raw) };

    // Rust → JS: numbers, strings and arrays.
    let raw = runtime.cx();
    rooted!(in(raw) let mut value = UndefinedValue());
    {
        let mut cx = runtime.cx_mut();
        vec![String::from("a"), String::from("é"), String::from("中")].safe_to_jsval(&mut cx, value.handle_mut());
    }
    assert!(value.get().is_object());
    assert_eq!(describe(&runtime, value.get()), "a,é,中");
    {
        let mut cx = runtime.cx_mut();
        u32::MAX.safe_to_jsval(&mut cx, value.handle_mut());
    }
    assert_eq!(describe(&runtime, value.get()), "4294967295");
    runtime.gc_for_testing();
}
