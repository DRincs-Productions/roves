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
    let raw = unsafe { &*runtime.raw_cx() };
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
    let raw = unsafe { &*runtime.raw_cx() };
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
    let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
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
    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
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
    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
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
            self.slot.trace_visitor(visitor);
        }

        fn get_name(&self) -> &'static std::ffi::CStr {
            c"Owner"
        }
    }

    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    // SAFETY: the runtime owns its raw context.
    let raw = unsafe { &*runtime.raw_cx() };
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
        let raw = runtime.raw_cx();
        rooted!(in(raw) let value = value);
        let cx = runtime.cx();
        T::safe_from_jsval(cx, value.handle(), option)
    }
    fn ok<T>(result: Result<ConversionResult<T>, ()>) -> T {
        match result {
            Ok(ConversionResult::Success(value)) => value,
            _ => panic!("conversion failed"),
        }
    }

    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    // Integers: modular by default, EnforceRange throws, Clamp rounds half to even.
    assert_eq!(ok(from::<i32>(&mut runtime, "2 ** 32 + 5", ConversionBehavior::Default)), 5);
    assert_eq!(ok(from::<u8>(&mut runtime, "-1", ConversionBehavior::Default)), 255);
    assert_eq!(ok(from::<u8>(&mut runtime, "300.7", ConversionBehavior::Clamp)), 255);
    assert_eq!(ok(from::<u8>(&mut runtime, "2.5", ConversionBehavior::Clamp)), 2);
    assert_eq!(ok(from::<i64>(&mut runtime, "'-12'", ConversionBehavior::Default)), -12);
    assert!(from::<u8>(&mut runtime, "256", ConversionBehavior::EnforceRange).is_err());
    // SAFETY: a live context.
    assert!(unsafe { crate::jsapi::JS_IsExceptionPending(runtime.raw_cx()) });
    unsafe { crate::jsapi::JS_ClearPendingException(runtime.raw_cx()) };
    // ToNumber runs user code and propagates its exception as the pending one.
    assert!(from::<f64>(&mut runtime, "({ valueOf() { throw new RangeError('mine'); } })", ()).is_err());
    let raw = runtime.raw_cx();
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
    let raw = runtime.raw_cx();
    rooted!(in(raw) let mut value = UndefinedValue());
    {
        let cx = runtime.cx();
        vec![String::from("a"), String::from("é"), String::from("中")].safe_to_jsval(cx, value.handle_mut());
    }
    assert!(value.get().is_object());
    assert_eq!(describe(&runtime, value.get()), "a,é,中");
    {
        let cx = runtime.cx();
        u32::MAX.safe_to_jsval(cx, value.handle_mut());
    }
    assert_eq!(describe(&runtime, value.get()), "4294967295");
    runtime.gc_for_testing();
}


// --- CP98: object model, binding tables and the JSAPI object/property surface ---------------

mod object_model {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{IdVector, Runtime};

    thread_local! {
        static HOOK_FINALIZED: Cell<u32> = const { Cell::new(0) };
        static HOOK_TRACED: Cell<u32> = const { Cell::new(0) };
    }

    unsafe extern "C" fn finalize_hook(_gcx: *mut GCContext, obj: *mut JSObject) {
        let mut slot = UndefinedValue();
        // SAFETY: the finalize hook may read the reserved slots.
        unsafe { JS_GetReservedSlot(obj, 0, &mut slot) };
        if slot.is_int32() && slot.to_int32() == 42 {
            HOOK_FINALIZED.with(|count| count.set(count.get() + 1));
        }
    }

    unsafe extern "C" fn trace_hook(_trc: *mut JSTracer, _obj: *mut JSObject) {
        HOOK_TRACED.with(|count| count.set(count.get() + 1));
    }

    static OPS: JSClassOps = JSClassOps {
        addProperty: None,
        delProperty: None,
        enumerate: None,
        newEnumerate: None,
        resolve: None,
        mayResolve: None,
        finalize: Some(finalize_hook),
        call: None,
        construct: None,
        trace: Some(trace_hook),
    };

    static CLASS: JSClass = JSClass {
        name: c"Thing".as_ptr(),
        flags: 2 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT | crate::object::JSCLASS_FOREGROUND_FINALIZE,
        cOps: &OPS,
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    fn global(runtime: &Runtime) -> JSVal {
        eval(runtime, "globalThis")
    }

    #[test]
    fn class_objects_keep_identity_reserved_slots_and_run_hooks() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let finalized_before = HOOK_FINALIZED.with(Cell::get);
        {
            rooted!(in(cx) let thing = unsafe { JS_NewObject(cx, &CLASS) });
            assert!(!thing.get().is_null());
            unsafe { JS_SetReservedSlot(thing.get(), 0, &Int32Value(42)) };
            let mut slot = UndefinedValue();
            unsafe { JS_GetReservedSlot(thing.get(), 0, &mut slot) };
            assert_eq!(slot.to_int32(), 42);
            assert_eq!(crate::object::object_class(thing.get()), &CLASS as *const JSClass);
            // Converting the V8 object back yields the same pointer (class objects are interned).
            rooted!(in(cx) let global = global(&runtime).to_object());
            rooted!(in(cx) let value = ObjectValue(thing.get()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"thing".as_ptr(), value.handle().into_handle()) });
            assert_eq!(eval(&runtime, "thing").to_object(), thing.get());
            assert_eq!(describe(&runtime, eval(&runtime, "Object.prototype.toString.call(thing)")), "[object Object]");
            runtime.gc_for_testing();
            assert!(HOOK_TRACED.with(Cell::get) > 0, "the class trace hook runs during GC");
            eval(&runtime, "delete globalThis.thing");
        }
        runtime.gc_for_testing();
        runtime.gc_for_testing();
        assert_eq!(HOOK_FINALIZED.with(Cell::get), finalized_before + 1, "finalize sees the reserved slots once");
    }

    #[test]
    fn properties_by_name_and_id_and_descriptors() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let object = unsafe { JS_NewPlainObject(cx) });
        rooted!(in(cx) let value = Int32Value(7));
        let attrs = (JSPROP_ENUMERATE | JSPROP_READONLY) as u32;
        assert!(unsafe { JS_DefineProperty(cx, object.handle().into_handle(), c"seven".as_ptr(), value.handle().into_handle(), attrs) });
        let mut found = false;
        assert!(unsafe { JS_HasOwnProperty(cx, object.handle().into_handle(), c"seven".as_ptr(), &mut found) } && found);
        rooted!(in(cx) let mut read = UndefinedValue());
        assert!(unsafe { JS_GetProperty(cx, object.handle().into_handle(), c"seven".as_ptr(), read.handle_mut().into_handle()) });
        assert_eq!(read.get().to_int32(), 7);

        // Index ids and the descriptor of a read-only data property.
        rooted!(in(cx) let mut id = crate::jsid::VoidId());
        unsafe { int_to_jsid(3, id.handle_mut().into_handle()) };
        assert!(unsafe { JS_DefinePropertyById2(cx, object.handle().into_handle(), id.handle().into_handle(), value.handle().into_handle(), 0) });
        assert!(unsafe { JS_HasPropertyById(cx, object.handle().into_handle(), id.handle().into_handle(), &mut found) } && found);
        rooted!(in(cx) let mut desc = PropertyDescriptor::default());
        let mut is_none = true;
        let seven = unsafe { JS_AtomizeAndPinString(cx, c"seven".as_ptr()) };
        rooted!(in(cx) let mut seven_id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, seven, seven_id.handle_mut().into_handle()) };
        assert!(seven_id.get().is_string());
        assert!(unsafe {
            JS_GetOwnPropertyDescriptorById(cx, object.handle().into_handle(), seven_id.handle().into_handle(), desc.handle_mut().into_handle(), &mut is_none)
        });
        assert!(!is_none);
        assert!(desc.hasWritable_() && !desc.writable_() && desc.enumerable_() && desc.configurable_());
        assert_eq!(desc.value_.to_int32(), 7);

        // An array-index string becomes an int id.
        let index = unsafe { JS_NewStringCopyN(cx, c"12".as_ptr(), 2) };
        rooted!(in(cx) let mut index_id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, index, index_id.handle_mut().into_handle()) };
        assert!(index_id.get().is_int() && index_id.get().to_int() == 12);

        // Own keys, then deletion.
        let mut keys = unsafe { IdVector::new(cx) };
        assert!(unsafe { GetPropertyKeys(cx, object.handle().into_handle(), JSITER_OWNONLY | JSITER_HIDDEN, keys.handle_mut()) });
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|id| id.is_int() && id.to_int() == 3));
        let mut result = ObjectOpResult { code_: 0 };
        assert!(unsafe { JS_DeletePropertyById(cx, object.handle().into_handle(), id.handle().into_handle(), &mut result) });
        assert!(result.ok());
        assert!(unsafe { JS_HasOwnPropertyById(cx, object.handle().into_handle(), id.handle().into_handle(), &mut found) } && !found);
    }

    unsafe extern "C" fn add_native(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        let mut sum = 0;
        for value in &args[2..] {
            sum += value.to_int32();
        }
        // A nested JSAPI call inside a native (nested V8 scopes on the same isolate).
        rooted!(in(cx) let this = args[1].to_object());
        rooted!(in(cx) let mut base = UndefinedValue());
        if !unsafe { JS_GetProperty(cx, this.handle().into_handle(), c"base".as_ptr(), base.handle_mut().into_handle()) } {
            return false;
        }
        if base.get().is_int32() {
            sum += base.get().to_int32();
        }
        args[0] = Int32Value(sum);
        true
    }

    unsafe extern "C" fn throwing_native(cx: *mut JSContext, _argc: u32, _vp: *mut JSVal) -> bool {
        rooted!(in(cx) let error = unsafe { JS_NewStringCopyN(cx, c"boom".as_ptr(), 4) });
        rooted!(in(cx) let value = StringValue(unsafe { &*error.get() }));
        unsafe { JS_SetPendingException(cx, value.handle(), ExceptionStackBehavior::Capture) };
        false
    }

    unsafe extern "C" fn reserved_getter(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, 2) };
        let callee = args[0].to_object();
        let reserved = unsafe { GetFunctionNativeReserved(callee, 0) };
        args[0] = if reserved.is_null() { UndefinedValue() } else { unsafe { *reserved } };
        true
    }

    static FUNCTIONS: [JSFunctionSpec; 3] = [
        JSFunctionSpec {
            name: JSFunctionSpec_Name { string_: c"add".as_ptr() },
            call: JSNativeWrapper { op: Some(add_native), info: ptr::null() },
            nargs: 2,
            flags: JSPROP_ENUMERATE as u16,
            selfHostedName: ptr::null(),
        },
        JSFunctionSpec {
            name: JSFunctionSpec_Name { string_: c"fail".as_ptr() },
            call: JSNativeWrapper { op: Some(throwing_native), info: ptr::null() },
            nargs: 0,
            flags: 0,
            selfHostedName: ptr::null(),
        },
        JSFunctionSpec {
            name: JSFunctionSpec_Name { string_: ptr::null() },
            call: JSNativeWrapper { op: None, info: ptr::null() },
            nargs: 0,
            flags: 0,
            selfHostedName: ptr::null(),
        },
    ];

    #[test]
    fn natives_from_function_specs_calls_and_exceptions() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let object = unsafe { JS_NewPlainObject(cx) });
        assert!(unsafe { JS_DefineFunctions(cx, object.handle().into_handle(), FUNCTIONS.as_ptr()) });
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let value = ObjectValue(object.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"api".as_ptr(), value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "api.base = 100; api.add(1, 2, 3)").to_int32(), 106);
        assert_eq!(describe(&runtime, eval(&runtime, "Object.keys(api).join()")), "add,base");
        assert_eq!(describe(&runtime, eval(&runtime, "try { api.fail(); 'no' } catch (e) { e }")), "boom");

        // JS::Call from Rust.
        rooted!(in(cx) let function = eval(&runtime, "(function (a, b) { return this.base * a + b; })"));
        let arguments = [Int32Value(2), Int32Value(5)];
        let array = HandleValueArray { length_: 2, elements_: arguments.as_ptr() };
        rooted!(in(cx) let mut result = UndefinedValue());
        assert!(unsafe { Call(cx, value.handle().into_handle(), function.handle().into_handle(), &array, result.handle_mut().into_handle()) });
        assert_eq!(result.get().to_int32(), 205);

        // A native function with reserved slots, identified again as its callee.
        let function = unsafe { JS_NewFunction(cx, Some(reserved_getter), 0, 0, c"reserved".as_ptr()) };
        rooted!(in(cx) let function = unsafe { JS_GetFunctionObject(function) });
        unsafe { SetFunctionNativeReserved(function.get(), 0, &Int32Value(9)) };
        rooted!(in(cx) let function_value = ObjectValue(function.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"reserved".as_ptr(), function_value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "reserved()").to_int32(), 9);
        assert_eq!(eval(&runtime, "reserved").to_object(), function.get(), "native functions keep their identity");
        let mut callable = false;
        assert!(unsafe { IsCallable(function.get()) });
        assert!(unsafe { IsArrayObject(cx, function_value.handle().into_handle(), &mut callable) } && !callable);
    }

    #[test]
    fn dynamic_native_functions_die_with_their_function() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // SAFETY: the runtime's live context.
        let states = || unsafe { (*cx).native_functions.borrow().len() };
        // A rooted function keeps a reserved slot's object alive through its private property.
        let function = unsafe { JS_NewFunction(cx, Some(reserved_getter), 0, 0, c"reserved".as_ptr()) };
        rooted!(in(cx) let function = unsafe { JS_GetFunctionObject(function) });
        let held = eval(&runtime, "({ toString() { return 'held by the slot'; } })");
        unsafe { SetFunctionNativeReserved(function.get(), 0, &held) };
        unsafe { SetFunctionNativeReserved(function.get(), 1, &Int32Value(3)) };
        runtime.gc_for_testing();
        let slot = unsafe { *GetFunctionNativeReserved(function.get(), 0) };
        assert_eq!(describe(&runtime, slot), "held by the slot");
        assert_eq!(unsafe { *GetFunctionNativeReserved(function.get(), 1) }.to_int32(), 3);
        // Unrooted dynamic functions are collected, and their states dropped at the next sweep.
        let before = states();
        for _ in 0..1000 {
            assert!(!unsafe { JS_NewFunction(cx, Some(reserved_getter), 0, 0, ptr::null()) }.is_null());
        }
        assert_eq!(states(), before + 1000);
        runtime.gc_for_testing();
        for _ in 0..100 {
            assert!(!unsafe { JS_NewFunction(cx, Some(reserved_getter), 0, 0, ptr::null()) }.is_null());
        }
        assert!(states() < 200, "dead states are swept: {}", states());
        // The rooted one still works.
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let function_value = ObjectValue(function.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"reserved".as_ptr(), function_value.handle().into_handle()) });
        assert_eq!(describe(&runtime, eval(&runtime, "String(reserved())")), "held by the slot");
    }

    unsafe extern "C" fn answer_getter(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        unsafe { *vp = Int32Value(42) };
        true
    }

    static PROPERTIES: [JSPropertySpec; 3] = [
        JSPropertySpec {
            name: JSPropertySpec_Name { string_: c"answer".as_ptr() },
            attributes_: JSPROP_ENUMERATE,
            kind_: JSPropertySpec_Kind::NativeAccessor,
            u: JSPropertySpec_AccessorsOrValue {
                accessors: JSPropertySpec_AccessorsOrValue_Accessors {
                    getter: JSPropertySpec_Accessor { native: JSNativeWrapper { op: Some(answer_getter), info: ptr::null() } },
                    setter: JSPropertySpec_Accessor { native: JSNativeWrapper { op: None, info: ptr::null() } },
                },
            },
        },
        JSPropertySpec {
            name: JSPropertySpec_Name { symbol_: crate::jsapi::SymbolCode::toStringTag as usize + 1 },
            attributes_: JSPROP_READONLY,
            kind_: JSPropertySpec_Kind::Value,
            u: JSPropertySpec_AccessorsOrValue {
                value: JSPropertySpec_ValueWrapper {
                    type_: JSPropertySpec_ValueWrapper_Type::String,
                    __bindgen_anon_1: JSPropertySpec_ValueWrapper__bindgen_ty_1 { string: c"Answer".as_ptr() },
                },
            },
        },
        JSPropertySpec {
            name: JSPropertySpec_Name { string_: ptr::null() },
            attributes_: 0,
            kind_: JSPropertySpec_Kind::Value,
            u: JSPropertySpec_AccessorsOrValue {
                value: JSPropertySpec_ValueWrapper {
                    type_: JSPropertySpec_ValueWrapper_Type::Int32,
                    __bindgen_anon_1: JSPropertySpec_ValueWrapper__bindgen_ty_1 { int32: 0 },
                },
            },
        },
    ];

    #[test]
    fn property_specs_define_accessors_and_symbol_values() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let proto = unsafe { GetRealmObjectPrototype(cx) });
        rooted!(in(cx) let object = unsafe { JS_NewObjectWithGivenProto(cx, ptr::null(), proto.handle().into_handle()) });
        assert!(unsafe { JS_DefineProperties(cx, object.handle().into_handle(), PROPERTIES.as_ptr()) });
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let value = ObjectValue(object.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"spec".as_ptr(), value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "spec.answer").to_int32(), 42);
        assert_eq!(describe(&runtime, eval(&runtime, "String(spec)")), "[object Answer]");
        assert_eq!(
            describe(&runtime, eval(&runtime, "Object.getOwnPropertyDescriptor(spec, 'answer').get.name")),
            "get answer"
        );
        rooted!(in(cx) let mut prototype = ptr::null_mut::<JSObject>());
        assert!(unsafe { JS_GetPrototype(cx, object.handle().into_handle(), prototype.handle_mut().into_handle()) });
        assert_eq!(prototype.get(), proto.get());

        // JSON through the streaming callback.
        unsafe extern "C" fn collect(buf: *const u16, len: u32, data: *mut c_void) -> bool {
            // SAFETY: the callback contract (chars, length, closure data).
            let units = unsafe { std::slice::from_raw_parts(buf, len as usize) };
            unsafe { &mut *(data as *mut std::string::String) }.push_str(&std::string::String::from_utf16_lossy(units));
            true
        }
        let mut json = std::string::String::new();
        rooted!(in(cx) let space = UndefinedValue());
        rooted!(in(cx) let replacer = ptr::null_mut::<JSObject>());
        assert!(unsafe {
            ToJSON(cx, value.handle().into_handle(), replacer.handle().into_handle(), space.handle().into_handle(), Some(collect), &mut json as *mut std::string::String as *mut c_void)
        });
        assert_eq!(json, r#"{"answer":42}"#);
    }
}

#[test]
fn plain_objects_and_symbols_have_one_cell_while_it_lives() {
    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    eval(&runtime, "globalThis.kept = {}; globalThis.sym = Symbol('s')");
    assert_eq!(eval(&runtime, "kept").to_object(), eval(&runtime, "kept").to_object());
    assert_eq!(eval(&runtime, "sym").to_symbol(), eval(&runtime, "sym").to_symbol());
    assert_ne!(eval(&runtime, "kept").to_object(), eval(&runtime, "({})").to_object());
    {
        rooted!(in(cx) let kept = eval(&runtime, "kept").to_object());
        runtime.gc_for_testing();
        assert_eq!(eval(&runtime, "kept").to_object(), kept.get(), "a rooted cell stays the object's cell");
    }
    // Once the cell is collected, the object gets a new, working cell.
    runtime.gc_for_testing();
    let again = eval(&runtime, "kept.marker = 'alive'; kept");
    assert_eq!(describe(&runtime, eval(&runtime, "kept.marker")), "alive");
    assert_eq!(again.to_object(), eval(&runtime, "kept").to_object());
}

mod realms {
    use std::ptr;

    use super::eval;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::Runtime;

    static GLOBAL_CLASS: JSClass = JSClass {
        name: c"TestGlobal".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL |
            ((crate::object::JSCLASS_GLOBAL_SLOT_COUNT + 1) << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: ptr::null(),
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    thread_local! {
        static GLOBAL_TRACED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    unsafe extern "C" fn trace_global(_trc: *mut JSTracer, _global: *mut JSObject) {
        GLOBAL_TRACED.with(|count| count.set(count.get() + 1));
    }

    #[test]
    fn new_globals_are_separate_realms_with_class_identity() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let first_realm = unsafe { GetCurrentRealmOrNull(cx) };
        let first_object_prototype = eval(&runtime, "Object.prototype").to_object();
        let mut options = crate::rust::RealmOptions::default();
        options.creationOptions_.traceGlobal_ = Some(trace_global);
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::FireOnNewGlobalHook, &*options)
        });
        assert!(!global.get().is_null());
        assert!(unsafe { JS_IsGlobalObject(global.get()) });
        unsafe { JS_SetReservedSlot(global.get(), crate::object::JSCLASS_GLOBAL_SLOT_COUNT, &Int32Value(5)) };
        let realm = unsafe { GetObjectRealmOrNull(global.get()) };
        assert!(!realm.is_null() && realm != first_realm);
        assert_eq!(unsafe { GetRealmGlobalOrNull(realm) }, global.get());

        // Entering the realm switches where scripts run.
        let old = unsafe { EnterRealm(cx, global.get()) };
        assert_eq!(old, first_realm);
        assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, realm);
        assert_eq!(eval(&runtime, "globalThis").to_object(), global.get(), "the global proxy is the class object");
        assert_ne!(eval(&runtime, "Object.prototype").to_object(), first_object_prototype);
        assert_eq!(unsafe { CurrentGlobalOrNull(cx) }, global.get());
        assert!(eval(&runtime, "typeof console === 'undefined'").to_boolean(), "V8's console leaves room for the embedder's");
        eval(&runtime, "globalThis.inRealm = 1");
        unsafe { LeaveRealm(cx, old) };
        assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, first_realm);
        assert!(eval(&runtime, "typeof inRealm === 'undefined'").to_boolean());

        // The realm keeps its global (and its reserved slots) across GC, tracing it with the
        // realm's `traceGlobal` hook.
        runtime.gc_for_testing();
        assert!(GLOBAL_TRACED.with(std::cell::Cell::get) > 0);
        let mut slot = UndefinedValue();
        unsafe { JS_GetReservedSlot(global.get(), crate::object::JSCLASS_GLOBAL_SLOT_COUNT, &mut slot) };
        assert_eq!(slot.to_int32(), 5);
        assert_eq!(unsafe { crate::realm_impl::get_object_realm(eval(&runtime, "({})").to_object()) }, first_realm);
    }

    #[test]
    fn helpers_and_proxy_handlers_belong_to_the_current_realm() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // The initial realm uses the helper first.
        rooted!(in(cx) let map = eval(&runtime, "new Map([[1, 2]])").to_object());
        rooted!(in(cx) let mut iterator = UndefinedValue());
        assert!(unsafe { MapEntries(cx, map.handle().into_handle(), iterator.handle_mut().into_handle()) });
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, global.get()) };
        rooted!(in(cx) let other_map = eval(&runtime, "new Map([[3, 4]])").to_object());
        rooted!(in(cx) let mut other = UndefinedValue());
        assert!(unsafe { MapEntries(cx, other_map.handle().into_handle(), other.handle_mut().into_handle()) });
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"it".as_ptr(), other.handle().into_handle()) });
        assert!(
            eval(&runtime, "Object.getPrototypeOf(it) === Object.getPrototypeOf(new Map().entries()) && it.next().value[0] === 3").to_boolean(),
            "the iterator comes from the current realm"
        );
        unsafe { LeaveRealm(cx, old) };
        assert!(eval(&runtime, "typeof it === 'undefined'").to_boolean());
    }
}

mod proxies {
    use std::cell::Cell;
    use std::ptr;

    use super::{describe, eval};
    use crate::glue::*;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::Runtime;

    thread_local! {
        static PROXY_FINALIZED: Cell<u32> = const { Cell::new(0) };
        static PROXY_TRACED: Cell<u32> = const { Cell::new(0) };
    }

    /// The expando object (created on demand in the private slot, as Servo's DOM proxies do).
    unsafe fn expando(cx: *mut JSContext, proxy: HandleObject) -> *mut JSObject {
        let mut private = UndefinedValue();
        unsafe { GetProxyPrivate(proxy.get(), &mut private) };
        if private.is_object() {
            return private.to_object();
        }
        let object = unsafe { JS_NewPlainObject(cx) };
        unsafe { SetProxyPrivate(proxy.get(), &ObjectValue(object)) };
        object
    }

    unsafe extern "C" fn get_own_property_descriptor(
        cx: *mut JSContext,
        proxy: HandleObject,
        id: HandleId,
        desc: MutableHandle<PropertyDescriptor>,
        is_none: *mut bool,
    ) -> bool {
        let id_value = id.get();
        if id_value.is_string() {
            let mut length = 0;
            let chars = crate::api::latin1_chars(id_value.to_string(), &mut length);
            let name = if chars.is_null() { Vec::new() } else { unsafe { std::slice::from_raw_parts(chars, length) }.to_vec() };
            if name == b"foo" {
                let value = unsafe { JS_NewStringCopyN(cx, c"named-foo".as_ptr(), 9) };
                crate::rooted!(in(cx) let value = StringValue(unsafe { &*value }));
                unsafe { SetDataPropertyDescriptor(desc, value.handle().into_handle(), JSPROP_ENUMERATE as u32 | JSPROP_READONLY as u32) };
                unsafe { *is_none = false };
                return true;
            }
        }
        crate::rooted!(in(cx) let expando = unsafe { expando(cx, proxy) });
        unsafe { JS_GetOwnPropertyDescriptorById(cx, expando.handle().into_handle(), id, desc, is_none) }
    }

    unsafe extern "C" fn define_property(
        cx: *mut JSContext,
        proxy: HandleObject,
        id: HandleId,
        desc: Handle<PropertyDescriptor>,
        result: *mut ObjectOpResult,
    ) -> bool {
        crate::rooted!(in(cx) let expando = unsafe { expando(cx, proxy) });
        unsafe { JS_DefinePropertyById(cx, expando.handle().into_handle(), id, desc, result) }
    }

    unsafe extern "C" fn own_property_keys(cx: *mut JSContext, proxy: HandleObject, props: MutableHandleIdVector) -> bool {
        let foo = unsafe { JS_AtomizeAndPinString(cx, c"foo".as_ptr()) };
        crate::rooted!(in(cx) let mut id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, foo, id.handle_mut().into_handle()) };
        unsafe { AppendToIdVector(props, id.handle().into_handle()) };
        crate::rooted!(in(cx) let expando = unsafe { expando(cx, proxy) });
        unsafe { GetPropertyKeys(cx, expando.handle().into_handle(), JSITER_OWNONLY | JSITER_HIDDEN | JSITER_SYMBOLS, props) }
    }

    unsafe extern "C" fn delete(cx: *mut JSContext, proxy: HandleObject, id: HandleId, result: *mut ObjectOpResult) -> bool {
        crate::rooted!(in(cx) let expando = unsafe { expando(cx, proxy) });
        unsafe { JS_DeletePropertyById(cx, expando.handle().into_handle(), id, result) }
    }

    unsafe extern "C" fn trace(_trc: *mut JSTracer, _proxy: *mut JSObject) {
        PROXY_TRACED.with(|count| count.set(count.get() + 1));
    }

    unsafe extern "C" fn finalize(_gcx: *mut GCContext, proxy: *mut JSObject) {
        let mut slot = UndefinedValue();
        unsafe { GetProxyReservedSlot(proxy, 0, &mut slot) };
        if slot.is_int32() && slot.to_int32() == 77 {
            PROXY_FINALIZED.with(|count| count.set(count.get() + 1));
        }
    }

    fn traps() -> ProxyTraps {
        // SAFETY: an all-`None` trap table is valid.
        let mut traps: ProxyTraps = unsafe { std::mem::zeroed() };
        traps.getOwnPropertyDescriptor = Some(get_own_property_descriptor);
        traps.defineProperty = Some(define_property);
        traps.ownPropertyKeys = Some(own_property_keys);
        traps.delete_ = Some(delete);
        traps.trace = Some(trace);
        traps.finalize = Some(finalize);
        traps
    }

    static EXTRA: u8 = 0;

    #[test]
    fn dom_style_proxies_use_traps_defaults_and_identity() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps = traps();
        let handler = unsafe { CreateProxyHandler(&traps, &EXTRA as *const u8 as *const std::ffi::c_void) };
        let finalized_before = PROXY_FINALIZED.with(Cell::get);
        {
            rooted!(in(cx) let proto = eval(&runtime, "({ hello() { return 'hi ' + this.foo; } })").to_object());
            rooted!(in(cx) let private = UndefinedValue());
            rooted!(in(cx) let proxy = unsafe { NewProxyObject(cx, handler, private.handle().into_handle(), proto.get(), ptr::null(), false) });
            assert!(!proxy.get().is_null());
            unsafe { SetProxyReservedSlot(proxy.get(), 0, &Int32Value(77)) };
            assert!(unsafe { IsProxyHandlerFamily(proxy.get()) });
            rooted!(in(cx) let mut prototype = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_GetPrototype(cx, proxy.handle().into_handle(), prototype.handle_mut().into_handle()) });
            assert_eq!(prototype.get(), proto.get(), "a proxy's prototype comes from its getPrototypeOf trap");
            assert_eq!(unsafe { GetProxyHandler(proxy.get()) }, handler);
            assert_eq!(unsafe { GetProxyHandlerExtra(proxy.get()) }, &EXTRA as *const u8 as *const std::ffi::c_void);
            assert!(unsafe { crate::object::object_class(proxy.get()).as_ref() }.unwrap().flags & crate::object::JSCLASS_IS_PROXY != 0);

            rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
            rooted!(in(cx) let value = ObjectValue(proxy.get()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"p".as_ptr(), value.handle().into_handle()) });
            assert_eq!(eval(&runtime, "p").to_object(), proxy.get(), "a proxy keeps its identity");

            // Named property from the trap, prototype methods with the proxy as `this`.
            assert_eq!(describe(&runtime, eval(&runtime, "p.foo")), "named-foo");
            assert_eq!(describe(&runtime, eval(&runtime, "p.hello()")), "hi named-foo");
            assert!(eval(&runtime, "'foo' in p && 'hello' in p && !('nope' in p)").to_boolean());
            // Assignment through the default set trap lands on the expando.
            assert_eq!(eval(&runtime, "p.x = 5; p.x").to_int32(), 5);
            assert_eq!(describe(&runtime, eval(&runtime, "Object.keys(p).join()")), "foo,x");
            // A read-only named property rejects writes in strict mode.
            assert!(
                eval(&runtime, "(() => { 'use strict'; try { p.foo = 1; return false } catch (e) { return e instanceof TypeError } })()")
                    .to_boolean()
            );
            // Non-configurable expandos satisfy V8's invariants.
            assert_eq!(eval(&runtime, "Object.defineProperty(p, 'fixed', { value: 3, configurable: false }); p.fixed").to_int32(), 3);
            assert!(eval(&runtime, "Object.getOwnPropertyDescriptor(p, 'fixed').configurable === false").to_boolean());
            assert!(eval(&runtime, "delete p.x; !('x' in p)").to_boolean());
            assert!(eval(&runtime, "Object.getPrototypeOf(p) !== null && typeof p.hello === 'function'").to_boolean());

            runtime.gc_for_testing();
            assert!(PROXY_TRACED.with(Cell::get) > 0, "the trace trap runs during GC");
            eval(&runtime, "delete globalThis.p");
        }
        runtime.gc_for_testing();
        runtime.gc_for_testing();
        assert_eq!(PROXY_FINALIZED.with(Cell::get), finalized_before + 1, "the finalize trap runs once with the slots");
    }
}

mod typed_arrays {
    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::Runtime;
    use crate::typedarray::{ArrayBuffer, CreateWith, Float32Array, Uint8Array};

    #[test]
    fn typed_arrays_share_stable_data_with_script() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let mut array = std::ptr::null_mut::<JSObject>());
        unsafe { Uint8Array::create(cx, CreateWith::Slice(&[1, 2, 3]), array.handle_mut()) }.unwrap();
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(array.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"bytes".as_ptr(), value.handle().into_handle()) });
        assert_eq!(describe(&runtime, eval(&runtime, "bytes instanceof Uint8Array && bytes.join()")), "1,2,3");

        // Script writes are visible through the (stable) data pointer, also after a GC.
        eval(&runtime, "bytes[1] = 42");
        runtime.gc_for_testing();
        let typed = Uint8Array::from(array.get()).unwrap();
        assert_eq!(typed.to_vec(), Some(vec![1, 42, 3]));
        assert!(!typed.is_shared());
        assert_eq!(unsafe { JS_GetArrayBufferViewType(array.get()) }, Type::Uint8);
        assert!(Float32Array::from(array.get()).is_err(), "typed arrays do not reinterpret");

        // Float arrays and buffers.
        rooted!(in(cx) let mut floats = std::ptr::null_mut::<JSObject>());
        unsafe { Float32Array::create(cx, CreateWith::Slice(&[0.5, 1.5]), floats.handle_mut()) }.unwrap();
        rooted!(in(cx) let floats_value = ObjectValue(floats.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"floats".as_ptr(), floats_value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "floats[0] + floats[1]").to_number(), 2.0);
        rooted!(in(cx) let buffer = eval(&runtime, "new Uint8Array([9, 8, 7]).buffer").to_object());
        let buffer = ArrayBuffer::from(buffer.get()).unwrap();
        assert_eq!(buffer.to_vec(), Some(vec![9, 8, 7]));
        let mut length = 0;
        let mut shared = true;
        let mut data = std::ptr::null_mut();
        unsafe { GetArrayBufferViewLengthAndData(eval(&runtime, "bytes.subarray(1)").to_object(), &mut length, &mut shared, &mut data) };
        assert_eq!((length, shared, unsafe { *data }), (2, false, 42));
    }
}

mod lazy_globals {
    use std::cell::Cell;
    use std::ptr;

    use super::{describe, eval};
    use crate::glue::*;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::Runtime;

    thread_local! {
        static RESOLVE_CALLS: Cell<u32> = const { Cell::new(0) };
    }

    fn is_lazy_name(id: jsid) -> bool {
        if !id.is_string() {
            return false;
        }
        let mut length = 0;
        let chars = crate::api::latin1_chars(id.to_string(), &mut length);
        !chars.is_null() && unsafe { std::slice::from_raw_parts(chars, length) } == b"LazyThing"
    }

    fn is_phantom(id: jsid) -> bool {
        if !id.is_string() {
            return false;
        }
        let mut length = 0;
        let chars = crate::api::latin1_chars(id.to_string(), &mut length);
        !chars.is_null() && unsafe { std::slice::from_raw_parts(chars, length) } == b"Phantom"
    }

    unsafe extern "C" fn resolve(cx: *mut JSContext, obj: HandleObject, id: HandleId, resolved: *mut bool) -> bool {
        if is_phantom(id.get()) {
            // Reports success without defining anything (Servo does for disabled interfaces).
            unsafe { *resolved = true };
            return true;
        }
        if !is_lazy_name(id.get()) {
            unsafe { *resolved = false };
            return true;
        }
        RESOLVE_CALLS.with(|calls| calls.set(calls.get() + 1));
        // A lookup of the same property from inside the hook does not recurse.
        crate::rooted!(in(cx) let mut existing = UndefinedValue());
        if !unsafe { JS_GetPropertyById(cx, obj, id, existing.handle_mut().into_handle()) } {
            return false;
        }
        assert!(existing.get().is_undefined());
        crate::rooted!(in(cx) let value = Int32Value(42));
        if !unsafe { JS_DefinePropertyById2(cx, obj, id, value.handle().into_handle(), JSPROP_RESOLVING as u32) } {
            return false;
        }
        unsafe { *resolved = true };
        true
    }

    unsafe extern "C" fn may_resolve(_names: *const JSAtomState, id: jsid, _obj: *mut JSObject) -> bool {
        is_lazy_name(id) || is_phantom(id)
    }

    unsafe extern "C" fn enumerate(cx: *mut JSContext, _obj: HandleObject, props: MutableHandleIdVector, _enumerable_only: bool) -> bool {
        let name = unsafe { JS_AtomizeAndPinString(cx, c"LazyThing".as_ptr()) };
        crate::rooted!(in(cx) let mut id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, name, id.handle_mut().into_handle()) };
        unsafe { AppendToIdVector(props, id.handle().into_handle()) }
    }

    static OPS: JSClassOps = JSClassOps {
        addProperty: None,
        delProperty: None,
        enumerate: None,
        newEnumerate: Some(enumerate),
        resolve: Some(resolve),
        mayResolve: Some(may_resolve),
        finalize: None,
        call: None,
        construct: None,
        trace: Some(JS_GlobalObjectTraceHook),
    };

    static CLASS: JSClass = JSClass {
        name: c"LazyGlobal".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL |
            (crate::object::JSCLASS_GLOBAL_SLOT_COUNT << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: &OPS,
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    #[test]
    fn global_resolve_hooks_define_lazy_properties_once() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, global.get()) };
        let before = RESOLVE_CALLS.with(Cell::get);
        assert!(eval(&runtime, "typeof notLazy === 'undefined'").to_boolean(), "other names stay unresolved");
        assert!(eval(&runtime, "typeof Phantom === 'undefined' && !('Phantom' in globalThis)").to_boolean(), "a hook that defines nothing does not recurse");
        // A proxy on the prototype chain (like a Window's named properties object) must not
        // hide lazy properties.
        eval(&runtime, "Object.setPrototypeOf(Object.getPrototypeOf(globalThis), new Proxy({}, {}))");
        assert_eq!(eval(&runtime, "(function () { return LazyThing; })()").to_int32(), 42);
        assert_eq!(eval(&runtime, "LazyThing").to_int32(), 42);
        assert_eq!(eval(&runtime, "LazyThing + LazyThing").to_int32(), 84);
        assert!(eval(&runtime, "'LazyThing' in globalThis && globalThis.hasOwnProperty('LazyThing')").to_boolean());
        assert_eq!(RESOLVE_CALLS.with(Cell::get), before + 1, "the hook runs once; later lookups find the property");
        assert_eq!(describe(&runtime, eval(&runtime, "Object.getOwnPropertyNames(globalThis).includes('LazyThing')")), "true");
        unsafe { LeaveRealm(cx, old) };
    }
}

mod runtime_hooks {
    use std::cell::Cell;
    use std::ffi::c_void;

    use super::eval;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    thread_local! {
        static EXTRA_TRACED: Cell<u32> = const { Cell::new(0) };
        static REJECTIONS: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
        static CSP_CHECKS: std::cell::RefCell<Vec<std::string::String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn text(string: *mut JSString) -> std::string::String {
        if string.is_null() {
            return "null".into();
        }
        crate::jsapi::JSContext::current().with_scope(|scope| {
            // SAFETY: the checker's strings are rooted.
            unsafe { crate::cell::cell_value(scope, string as *mut c_void) }.to_rust_string_lossy(scope)
        })
    }

    unsafe extern "C" fn csp_check(
        _cx: *mut JSContext,
        kind: RuntimeCode,
        code: Handle<*mut JSString>,
        compilation_type: CompilationType,
        parameters: Handle<StackGCVector<*mut JSString>>,
        body: Handle<*mut JSString>,
        _arguments: Handle<StackGCVector<JSVal>>,
        _body_argument: Handle<JSVal>,
        allowed: *mut bool,
    ) -> bool {
        // SAFETY: the checker's arguments are valid handles.
        let (code, body) = unsafe { (text(*code.ptr), text(*body.ptr)) };
        let parameters = unsafe { &*(parameters.ptr as *const Vec<*mut JSString>) };
        let parameters: Vec<std::string::String> = parameters.iter().map(|parameter| text(*parameter)).collect();
        CSP_CHECKS.with(|checks| checks.borrow_mut().push(format!("{kind:?} {compilation_type:?} {body} [{}]", parameters.join(","))));
        // SAFETY: a valid out pointer.
        unsafe { *allowed = !code.contains("blocked") };
        true
    }

    static SECURITY_CALLBACKS: JSSecurityCallbacks =
        JSSecurityCallbacks { contentSecurityPolicyAllows: Some(csp_check), codeForEvalGets: None, subsumes: None };

    static DISPATCHED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

    /// An embedder's dispatch callback: may run on any thread.
    unsafe extern "C" fn dispatch(closure: *mut c_void, task: *mut crate::glue::DispatchablePointer) -> bool {
        assert_eq!(closure as usize, 11);
        DISPATCHED.lock().unwrap().push(task as usize);
        true
    }

    /// Runs foreground work until `condition` (a script) holds, for at most five seconds.
    fn wait_for(runtime: &Runtime, condition: &str, mut step: impl FnMut()) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            step();
            if eval(runtime, condition).to_boolean() {
                return true;
            }
            std::thread::yield_now();
        }
        false
    }

    #[test]
    fn foreground_tasks_finish_async_wasm_and_finalization() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let compile = "globalThis.done = 0; WebAssembly.compile(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])).then(m => done = m instanceof WebAssembly.Module ? 1 : 2); 0";
        // Without a dispatch callback, `RunJobs` runs the tasks.
        eval(&runtime, compile);
        assert!(wait_for(&runtime, "done === 1", || unsafe { RunJobs(cx) }), "WebAssembly.compile resolves");
        // With one, the tasks reach the embedder's event loop and run in `DispatchableRun`.
        unsafe { SetUpEventLoopDispatch(cx, Some(dispatch), 11 as *mut c_void) };
        eval(&runtime, compile);
        let run_dispatched = || {
            let tasks = std::mem::take(&mut *DISPATCHED.lock().unwrap());
            for task in tasks {
                unsafe { DispatchableRun(cx, task as *mut _, Dispatchable_MaybeShuttingDown::NotShuttingDown) };
            }
            unsafe { RunJobs(cx) };
        };
        assert!(wait_for(&runtime, "done === 1", run_dispatched), "WebAssembly.compile resolves through dispatch");
        // `FinalizationRegistry` callbacks run from a foreground task after a collection.
        eval(&runtime, "globalThis.cleaned = ''; globalThis.registry = new FinalizationRegistry(held => cleaned = held); (function () { registry.register({}, 'held'); })(); 0");
        let mut collected = false;
        let start = std::time::Instant::now();
        while !collected && start.elapsed() < std::time::Duration::from_secs(5) {
            runtime.gc_for_testing();
            run_dispatched();
            collected = eval(&runtime, "cleaned === 'held'").to_boolean();
        }
        assert!(collected, "the FinalizationRegistry callback ran");
    }

    thread_local! {
        static CONSUMER: Cell<usize> = const { Cell::new(0) };
    }

    unsafe extern "C" fn consume(_cx: *mut JSContext, _response: HandleObject, _mime_type: MimeType, consumer: *mut StreamConsumer) -> bool {
        CONSUMER.with(|slot| slot.set(consumer as usize));
        true
    }

    #[test]
    fn wasm_streaming_feeds_the_embedders_stream() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        unsafe { InitConsumeStreamCallback(cx, Some(consume), None) };
        eval(&runtime, "globalThis.done = 0; WebAssembly.compileStreaming({ response: true }).then(m => done = m instanceof WebAssembly.Module ? 1 : 2, e => done = String(e)); 0");
        assert!(wait_for(&runtime, "true", || unsafe { RunJobs(cx) }));
        let consumer = CONSUMER.with(Cell::get) as *mut StreamConsumer;
        assert!(!consumer.is_null(), "the response reached the consume-stream callback");
        let bytes = [0u8, 97, 115, 109, 1, 0, 0, 0];
        unsafe {
            StreamConsumerNoteResponseURLs(consumer, c"https://example.test/m.wasm".as_ptr(), std::ptr::null());
            assert!(StreamConsumerConsumeChunk(consumer, bytes.as_ptr(), 4));
            assert!(StreamConsumerConsumeChunk(consumer, bytes[4..].as_ptr(), 4));
            StreamConsumerStreamEnd(consumer);
        }
        assert!(wait_for(&runtime, "done === 1", || unsafe { RunJobs(cx) }), "{}", super::describe(&runtime, eval(&runtime, "String(done)")));
        // A source that is not a response is rejected.
        eval(&runtime, "done = 0; WebAssembly.compileStreaming(Promise.resolve(5)).then(() => done = 1, e => done = e instanceof TypeError ? 3 : 4); 0");
        assert!(wait_for(&runtime, "done === 3", || unsafe { RunJobs(cx) }));
    }

    #[test]
    fn csp_checks_eval_function_and_wasm() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        assert_eq!(eval(&runtime, "eval('1 + 1')").to_int32(), 2, "no check installed");
        unsafe { JS_SetSecurityCallbacks(cx, &SECURITY_CALLBACKS) };
        assert_eq!(eval(&runtime, "eval('2 + 2')").to_int32(), 4);
        assert!(eval(&runtime, "try { eval('\"blocked\"'); false } catch (e) { e instanceof EvalError }").to_boolean());
        assert_eq!(eval(&runtime, "new Function('a', 'b', 'return a + b')(2, 3)").to_int32(), 5);
        assert!(eval(&runtime, "try { Function('return \"blocked\"'); false } catch (e) { e instanceof EvalError }").to_boolean());
        assert!(eval(&runtime, "const o = {}; eval(o) === o").to_boolean(), "non-strings are returned unchanged");
        let wasm = "new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])) instanceof WebAssembly.Module";
        assert!(eval(&runtime, wasm).to_boolean());
        let checks = CSP_CHECKS.with(|checks| checks.take());
        assert_eq!(checks[0], "JS DirectEval null []");
        assert_eq!(checks[1], "JS DirectEval null []");
        assert_eq!(checks[2], "JS Function return a + b [a,b]");
        assert_eq!(checks[3], "JS Function return \"blocked\" []");
        assert_eq!(checks.last().map(std::string::String::as_str), Some("WASM DirectEval null []"));
    }

    unsafe extern "C" fn extra_roots(_trc: *mut JSTracer, data: *mut c_void) {
        assert_eq!(data as usize, 7);
        EXTRA_TRACED.with(|count| count.set(count.get() + 1));
    }

    unsafe extern "C" fn keep_going(_cx: *mut JSContext) -> bool {
        false
    }

    unsafe extern "C" fn track(_cx: *mut JSContext, _muted: bool, promise: HandleObject, state: PromiseRejectionHandlingState, _data: *mut c_void) {
        assert!(!promise.get().is_null());
        REJECTIONS.with(|counts| {
            let (unhandled, handled) = counts.get();
            counts.set(match state {
                PromiseRejectionHandlingState::Unhandled => (unhandled + 1, handled),
                PromiseRejectionHandlingState::Handled => (unhandled, handled + 1),
            });
        });
    }

    #[test]
    fn runtime_hooks_roots_interrupts_jobs_and_rejections() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        assert!(unsafe { JS_AddExtraGCRootsTracer(cx, Some(extra_roots), 7 as *mut c_void) });
        runtime.gc_for_testing();
        assert!(EXTRA_TRACED.with(Cell::get) > 0, "embedder root tracers run on every GC");

        // Without an embedder job queue, promise jobs run at an explicit checkpoint only.
        eval(&runtime, "globalThis.done = false; Promise.resolve().then(() => { globalThis.done = true; })");
        assert!(!eval(&runtime, "done").to_boolean());
        unsafe { RunJobs(cx) };
        assert!(eval(&runtime, "done").to_boolean());

        // Unhandled rejections, then a late handler.
        unsafe { SetPromiseRejectionTrackerCallback(cx, Some(track), std::ptr::null_mut()) };
        eval(&runtime, "globalThis.rejected = Promise.reject(1)");
        eval(&runtime, "rejected.catch(() => {})");
        unsafe { RunJobs(cx) };
        assert_eq!(REJECTIONS.with(Cell::get), (1, 1));

        // An interrupt requested from another thread stops a runaway script when a callback
        // returns false.
        assert!(unsafe { JS_AddInterruptCallback(cx, Some(keep_going)) });
        let thread_safe = runtime.thread_safe_js_context();
        let interrupter = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            thread_safe.request_interrupt_callback();
        });
        let finished = unsafe { &*cx }.with_scope(|scope| {
            let code = v8::String::new(scope, "for (;;) {}").unwrap();
            let script = v8::Script::compile(scope, code, None).unwrap();
            script.run(scope).is_some()
        });
        interrupter.join().unwrap();
        assert!(!finished, "the loop was terminated");
        unsafe { &mut *(*cx).isolate }.cancel_terminate_execution();

        // A child runtime lives on its own thread (its own isolate).
        let parent = runtime.prepare_for_new_child();
        std::thread::spawn(move || {
            let child = unsafe { Runtime::create_with_parent(parent) };
            assert_eq!(eval(&child, "6 * 7").to_int32(), 42);
        })
        .join()
        .unwrap();
    }
}

mod scripts {
    use std::cell::RefCell;
    use std::ffi::CString;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{CompileOptionsWrapper, EnvironmentChain, JSEngineHandle, Runtime, transform_str_to_source_text, transform_u16_to_source_text};

    thread_local! {
        static SEEN: RefCell<Vec<std::string::String>> = const { RefCell::new(Vec::new()) };
    }

    /// A native that records its scripted caller (private and location) and the stack.
    unsafe extern "C" fn whoami(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        crate::rooted!(in(cx) let mut private = UndefinedValue());
        unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
        let safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        let caller = crate::rust::describe_scripted_caller_safe(&safe).unwrap();
        crate::capture_stack!(in(cx) let stack);
        let stack = unsafe { stack }.unwrap().as_string(None, StackFormat::SpiderMonkey).unwrap();
        SEEN.with(|seen| {
            seen.borrow_mut().push(format!(
                "{}|{}:{}|{}",
                if private.get().is_int32() { private.get().to_int32() } else { -1 },
                caller.filename,
                caller.line,
                stack.lines().next().unwrap_or("")
            ))
        });
        unsafe { *vp = UndefinedValue() };
        true
    }

    #[test]
    fn scripts_compile_run_carry_privates_and_report_errors() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        let whoami_fn = unsafe { JS_NewFunction(cx, Some(whoami), 0, 0, c"whoami".as_ptr()) };
        rooted!(in(cx) let whoami_value = ObjectValue(whoami_fn as *mut JSObject));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"whoami".as_ptr(), whoami_value.handle().into_handle()) });

        // Compile, attach a private, run.
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("https://example.test/a.js").unwrap(), 10);
        let code = "function fromA() { whoami(); }\nvar ran = 'yes'; 6 * 7";
        let mut source = transform_str_to_source_text(code);
        rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
        assert!(!script.get().is_null());
        unsafe { SetScriptPrivate(script.get(), &Int32Value(11)) };
        rooted!(in(cx) let mut private = UndefinedValue());
        unsafe { JS_GetScriptPrivate(script.get(), private.handle_mut().into()) };
        assert_eq!(private.get().to_int32(), 11);
        rooted!(in(cx) let mut result = UndefinedValue());
        assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
        assert_eq!(result.get().to_int32(), 42);
        assert_eq!(describe(&runtime, eval(&runtime, "ran")), "yes");

        // A function from script A, called later from another script, reports A's private
        // and location.
        runtime.gc_for_testing();
        eval(&runtime, "fromA()");
        let seen = SEEN.with(|seen| seen.borrow().clone());
        assert_eq!(seen.len(), 1);
        assert!(seen[0].starts_with("11|https://example.test/a.js:10|fromA@https://example.test/a.js:10:"), "{}", seen[0]);

        // Event-handler style functions: arguments and an environment chain.
        let chain = EnvironmentChain::new(cx, SupportUnscopables::Yes);
        rooted!(in(cx) let scope_object = eval(&runtime, "({ fromScope: 5 })").to_object());
        chain.append(scope_object.get());
        let body: Vec<u16> = "return event + fromScope;".encode_utf16().collect();
        let mut body_source = transform_u16_to_source_text(&body);
        let argument = c"event".as_ptr();
        let function = unsafe { CompileFunction(cx, chain.get(), options.ptr, c"onclick".as_ptr(), 1, &argument, &mut body_source) };
        assert!(!function.is_null());
        rooted!(in(cx) let function_value = ObjectValue(function as *mut JSObject));
        let arguments = [Int32Value(2)];
        let array = HandleValueArray { length_: 1, elements_: arguments.as_ptr() };
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let mut call_result = UndefinedValue());
        assert!(unsafe { Call(cx, this.handle().into_handle(), function_value.handle().into_handle(), &array, call_result.handle_mut().into_handle()) });
        assert_eq!(call_result.get().to_int32(), 7);

        // Errors: the pending exception's message and location, and error reports.
        let failing = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("https://example.test/b.js").unwrap(), 1);
        let mut bad = transform_str_to_source_text("\nnull.boom");
        rooted!(in(cx) let mut ignored = UndefinedValue());
        assert!(!unsafe { Evaluate2(cx, failing.ptr, &mut bad, ignored.handle_mut().into()) });
        rooted!(in(cx) let mut exception = UndefinedValue());
        let info = crate::rust::error_info_from_exception_stack_safe(runtime.cx(), exception.handle_mut()).unwrap();
        assert!(info.message.starts_with("TypeError"), "{}", info.message);
        assert_eq!((info.filename.as_str(), info.line), ("https://example.test/b.js", 2));
        assert!(exception.get().is_object());
        rooted!(in(cx) let error = exception.get().to_object());
        let report = unsafe { JS_ErrorFromException(cx, error.handle().into_handle()) };
        assert!(!report.is_null());
        assert_eq!(unsafe { (*report)._base.lineno }, 2);
        let message = unsafe { std::ffi::CStr::from_ptr((*report)._base.message_.data_) }.to_string_lossy().into_owned();
        assert!(message.contains("null"), "{message}");

        // Syntax errors fail compilation with a pending exception.
        let mut syntax = transform_str_to_source_text("let = ;");
        assert!(unsafe { Compile1(cx, failing.ptr, &mut syntax) }.is_null());
        assert!(unsafe { crate::api::JS_IsExceptionPending(cx) });
        unsafe { crate::api::JS_ClearPendingException(cx) };
    }
}

mod values {
    use std::ptr;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    fn set_global(runtime: &Runtime, name: &std::ffi::CStr, value: JSVal) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = eval(runtime, "globalThis").to_object());
        rooted!(in(cx) let value = value);
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), name.as_ptr(), value.handle().into_handle()) });
    }

    #[test]
    fn promises_json_and_value_operations() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();

        // Promises without an executor, settled from Rust, with reactions.
        rooted!(in(cx) let null_executor = ptr::null_mut::<JSObject>());
        rooted!(in(cx) let promise = unsafe { NewPromiseObject(cx, null_executor.handle().into_handle()) });
        assert!(unsafe { IsPromiseObject(promise.handle().into_handle()) });
        assert_eq!(unsafe { GetPromiseState(promise.handle().into_handle()) }, PromiseState::Pending);
        rooted!(in(cx) let on_fulfilled = eval(&runtime, "(v) => { globalThis.got = v; }").to_object());
        rooted!(in(cx) let none = ptr::null_mut::<JSObject>());
        assert!(unsafe { AddPromiseReactions(cx, promise.handle().into_handle(), on_fulfilled.handle().into_handle(), none.handle().into_handle()) });
        rooted!(in(cx) let five = Int32Value(5));
        assert!(unsafe { ResolvePromise(cx, promise.handle().into_handle(), five.handle().into_handle()) });
        assert_eq!(unsafe { GetPromiseState(promise.handle().into_handle()) }, PromiseState::Fulfilled);
        rooted!(in(cx) let mut result = UndefinedValue());
        unsafe { JS_GetPromiseResult(promise.handle().into_handle(), result.handle_mut().into()) };
        assert_eq!(result.get().to_int32(), 5);
        unsafe { RunJobs(cx) };
        assert_eq!(eval(&runtime, "got").to_int32(), 5);
        assert!(unsafe { SetPromiseUserInputEventHandlingState(promise.handle().into_handle(), PromiseUserInputEventHandlingState::HadUserInteractionAtCreation) });
        assert_eq!(
            unsafe { GetPromiseUserInputEventHandlingState(promise.handle().into_handle()) },
            PromiseUserInputEventHandlingState::HadUserInteractionAtCreation
        );

        // JSON.
        let text: Vec<u16> = r#"{"a":[1,2]}"#.encode_utf16().collect();
        rooted!(in(cx) let mut parsed = UndefinedValue());
        assert!(unsafe { JS_ParseJSON(cx, text.as_ptr(), text.len() as u32, parsed.handle_mut().into()) });
        set_global(&runtime, c"parsed", parsed.get());
        assert_eq!(eval(&runtime, "parsed.a[1]").to_int32(), 2);

        // Values.
        rooted!(in(cx) let nan = DoubleValue(f64::NAN));
        let mut same = false;
        assert!(unsafe { SameValue(cx, nan.handle().into_handle(), nan.handle().into_handle(), &mut same) } && same);
        rooted!(in(cx) let function = eval(&runtime, "(function named(a, b) { return this })"));
        assert_eq!(unsafe { JS_TypeOfValue(cx, function.handle().into_handle()) }, JSType::JSTYPE_FUNCTION);
        assert_eq!(unsafe { JS_GetFunctionArity(function.get().to_object() as *mut JSFunction) }, 2);
        rooted!(in(cx) let mut name = ptr::null_mut::<JSString>());
        rooted!(in(cx) let function_object = function.get().to_object() as *mut JSFunction);
        assert!(unsafe { JS_GetFunctionId(cx, function_object.handle().into_handle(), name.handle_mut().into()) });
        assert_eq!(describe(&runtime, StringValue(unsafe { &*name.get() })), "named");
        rooted!(in(cx) let date_like = eval(&runtime, "({ valueOf() { return 7 }, toString() { return 'seven' } })").to_object());
        rooted!(in(cx) let mut primitive = UndefinedValue());
        assert!(unsafe { ToPrimitive(cx, date_like.handle().into_handle(), JSType::JSTYPE_STRING, primitive.handle_mut().into()) });
        assert_eq!(describe(&runtime, primitive.get()), "seven");
        assert!(unsafe { ToPrimitive(cx, date_like.handle().into_handle(), JSType::JSTYPE_UNDEFINED, primitive.handle_mut().into()) });
        assert_eq!(primitive.get().to_int32(), 7);
        rooted!(in(cx) let map_constructor = eval(&runtime, "Map"));
        let no_arguments = HandleValueArray { length_: 0, elements_: ptr::null() };
        rooted!(in(cx) let mut map = ptr::null_mut::<JSObject>());
        assert!(unsafe { Construct1(cx, map_constructor.handle().into_handle(), &no_arguments, map.handle_mut().into()) });
        let entry = [StringValue(unsafe { &*JS_AtomizeAndPinString(cx, c"k".as_ptr()) }), Int32Value(1)];
        let entry_arguments = HandleValueArray { length_: 2, elements_: entry.as_ptr() };
        rooted!(in(cx) let mut ignored = UndefinedValue());
        assert!(unsafe { JS_CallFunctionName(cx, map.handle().into_handle(), c"set".as_ptr(), &entry_arguments, ignored.handle_mut().into()) });
        assert_eq!(unsafe { MapSize(cx, map.handle().into_handle()) }, 1);
        let mut class = ESClass::Other;
        assert!(unsafe { GetBuiltinClass(cx, map.handle().into_handle(), &mut class) } && class == ESClass::Map);
        let contents = [Int32Value(4), Int32Value(5)];
        let contents = HandleValueArray { length_: 2, elements_: contents.as_ptr() };
        rooted!(in(cx) let array = unsafe { NewArrayObject(cx, &contents) });
        let mut length = 0;
        assert!(unsafe { GetArrayLength(cx, array.handle().into_handle(), &mut length) } && length == 2);
        rooted!(in(cx) let mut element = UndefinedValue());
        assert!(unsafe { JS_GetElement(cx, array.handle().into_handle(), 1, element.handle_mut().into()) });
        assert_eq!(element.get().to_int32(), 5);
        rooted!(in(cx) let identifier = unsafe { JS_AtomizeAndPinString(cx, c"fooBar$1".as_ptr()) });
        let mut is_identifier = false;
        assert!(unsafe { JS_IsIdentifier(cx, identifier.handle().into(), &mut is_identifier) } && is_identifier);

        // Dates and regular expressions.
        rooted!(in(cx) let date = unsafe { NewDateObject(cx, ClippedTime { t: 86_400_000.0 }) });
        let mut msec = 0.0;
        assert!(unsafe { DateGetMsecSinceEpoch(cx, date.handle().into_handle(), &mut msec) } && msec == 86_400_000.0);
        let pattern: Vec<u16> = "b+".encode_utf16().collect();
        rooted!(in(cx) let regexp = unsafe { NewUCRegExpObject(cx, pattern.as_ptr(), pattern.len(), RegExpFlags { flags_: 0 }) });
        let input: Vec<u16> = "abbbc".encode_utf16().collect();
        let mut index = 0;
        rooted!(in(cx) let mut matched = UndefinedValue());
        assert!(unsafe { ExecuteRegExpNoStatics(cx, regexp.handle().into_handle(), input.as_ptr(), input.len(), &mut index, true, matched.handle_mut().into()) });
        assert!(matched.get().to_boolean());
        assert_eq!(index, 4);
        let bad: Vec<u16> = "(".encode_utf16().collect();
        rooted!(in(cx) let mut error = UndefinedValue());
        assert!(unsafe { CheckRegExpSyntax(cx, bad.as_ptr(), bad.len(), RegExpFlags { flags_: 0 }, error.handle_mut().into()) });
        assert!(error.get().is_object(), "an invalid pattern reports its SyntaxError");

        // ArrayBuffers.
        let bytes = unsafe { libc::malloc(4) } as *mut u8;
        unsafe { std::ptr::copy_nonoverlapping([1u8, 2, 3, 4].as_ptr(), bytes, 4) };
        rooted!(in(cx) let buffer = unsafe { NewArrayBufferWithContents(cx, 4, bytes as *mut std::ffi::c_void) });
        rooted!(in(cx) let clone = unsafe { ArrayBufferClone(cx, buffer.handle().into_handle(), 1, 2) });
        rooted!(in(cx) let view = unsafe { JS_NewUint8ArrayWithBuffer(cx, clone.handle().into_handle(), 0, -1) });
        set_global(&runtime, c"view", ObjectValue(view.get()));
        assert_eq!(describe(&runtime, eval(&runtime, "view.join()")), "2,3");
        assert!(unsafe { ArrayBufferCopyData(cx, clone.handle().into_handle(), 0, buffer.handle().into_handle(), 3, 1) });
        assert_eq!(describe(&runtime, eval(&runtime, "view.join()")), "4,3");
        rooted!(in(cx) let data_view = unsafe { JS_NewDataView(cx, buffer.handle().into_handle(), 0, 4) });
        set_global(&runtime, c"dv", ObjectValue(data_view.get()));
        assert_eq!(eval(&runtime, "dv.getUint8(2)").to_int32(), 3);
        let stolen = unsafe { StealArrayBufferContents(cx, buffer.handle().into_handle()) } as *mut u8;
        assert_eq!(unsafe { std::slice::from_raw_parts(stolen, 4) }, &[1, 2, 3, 4]);
        unsafe { libc::free(stolen as *mut std::ffi::c_void) };
        assert!(unsafe { IsDetachedArrayBufferObject(buffer.get()) });
    }
}

mod modules {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::CString;

    use super::{describe, eval};
    use crate::glue::JS_GetModulePrivate;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{CompileOptionsWrapper, JSEngineHandle, Runtime, transform_str_to_source_text};

    thread_local! {
        static REGISTRY: RefCell<HashMap<std::string::String, usize>> = RefCell::new(HashMap::new());
        static DYNAMIC: RefCell<Option<(JSVal, usize, usize)>> = const { RefCell::new(None) };
    }

    fn specifier(cx: *mut JSContext, request: Handle<*mut JSObject>) -> std::string::String {
        let string = unsafe { GetModuleRequestSpecifier(cx, request) };
        let safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        unsafe { crate::conversions::jsstr_to_string(&safe, std::ptr::NonNull::new(string).unwrap()) }
    }

    unsafe extern "C" fn resolve(cx: *mut JSContext, _private: Handle<JSVal>, request: Handle<*mut JSObject>) -> *mut JSObject {
        let name = specifier(cx, request);
        REGISTRY.with(|registry| registry.borrow().get(&name).copied()).map_or(std::ptr::null_mut(), |record| record as *mut JSObject)
    }

    unsafe extern "C" fn metadata(cx: *mut JSContext, private: Handle<JSVal>, meta: Handle<*mut JSObject>) -> bool {
        crate::rooted!(in(cx) let url = private.get());
        unsafe { JS_SetProperty(cx, meta, c"privateId".as_ptr(), url.handle().into_handle()) }
    }

    unsafe extern "C" fn dynamic_import(_cx: *mut JSContext, private: Handle<JSVal>, request: Handle<*mut JSObject>, promise: Handle<*mut JSObject>) -> bool {
        DYNAMIC.with(|dynamic| *dynamic.borrow_mut() = Some((private.get(), request.get() as usize, promise.get() as usize)));
        true
    }

    fn compile(runtime: &Runtime, name: &str, source: &str, json: bool) -> *mut JSObject {
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new(name).unwrap(), 1);
        let mut text = transform_str_to_source_text(source);
        let record = unsafe { if json { CompileJsonModule1(cx, options.ptr, &mut text) } else { CompileModule1(cx, options.ptr, &mut text) } };
        assert!(!record.is_null(), "{name} compiles");
        REGISTRY.with(|registry| registry.borrow_mut().insert(name.to_owned(), record as usize));
        record
    }

    #[test]
    fn module_graphs_link_evaluate_and_import_dynamically() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let rt = runtime.rt();
        unsafe {
            SetModuleResolveHook(rt, Some(resolve));
            SetModuleMetadataHook(rt, Some(metadata));
            SetModuleDynamicImportHook(rt, Some(dynamic_import));
        }
        compile(&runtime, "dep.js", "export const answer = 40;", false);
        compile(&runtime, "data.json", r#"{"extra": 2}"#, true);
        rooted!(in(cx) let main = compile(
            &runtime,
            "main.js",
            "import { answer } from 'dep.js';\nimport data from 'data.json' with { type: 'json' };\nexport const total = answer + data.extra;\nexport const meta = import.meta.privateId;\nexport function later() { return import('dep.js'); }",
            false,
        ));
        unsafe { SetModulePrivate(main.get(), &Int32Value(77)) };
        rooted!(in(cx) let mut private = UndefinedValue());
        unsafe { JS_GetModulePrivate(main.get(), private.handle_mut().into()) };
        assert_eq!(private.get().to_int32(), 77);
        assert!(unsafe { IsCyclicModule(main.get()) });
        assert_eq!(unsafe { GetRequestedModulesCount(cx, main.handle().into()) }, 2);
        assert_eq!(unsafe { GetRequestedModuleType(cx, main.handle().into(), 1) }, ModuleType::JSON);

        assert!(unsafe { ModuleLink(cx, main.handle().into()) });
        rooted!(in(cx) let mut evaluation = UndefinedValue());
        assert!(unsafe { ModuleEvaluate(cx, main.handle().into(), evaluation.handle_mut().into()) });
        unsafe { RunJobs(cx) };
        rooted!(in(cx) let evaluation_promise = evaluation.get().to_object());
        assert!(unsafe { ThrowOnModuleEvaluationFailure(cx, evaluation_promise.handle().into(), ModuleErrorBehaviour::ThrowModuleErrorsSync) });
        rooted!(in(cx) let namespace = unsafe { GetModuleNamespace(cx, main.handle().into()) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let namespace_value = ObjectValue(namespace.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"ns".as_ptr(), namespace_value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "ns.total").to_int32(), 42);
        assert_eq!(eval(&runtime, "ns.meta").to_int32(), 77, "import.meta comes from the metadata hook with the module's private");

        // Dynamic import: the hook gets the importer's private, a request and a promise.
        eval(&runtime, "globalThis.pending = ns.later(); pending.then((m) => { globalThis.dynamic = m.answer; });");
        let (importer_private, request, promise) = DYNAMIC.with(|dynamic| dynamic.borrow_mut().take()).expect("the dynamic import hook ran");
        assert_eq!(importer_private.to_int32(), 77);
        rooted!(in(cx) let request = request as *mut JSObject);
        assert_eq!(specifier(cx, request.handle().into()), "dep.js");
        rooted!(in(cx) let promise = promise as *mut JSObject);
        rooted!(in(cx) let dep = REGISTRY.with(|registry| registry.borrow()["dep.js"]) as *mut JSObject);
        rooted!(in(cx) let mut dep_evaluation = UndefinedValue());
        assert!(unsafe { ModuleEvaluate(cx, dep.handle().into(), dep_evaluation.handle_mut().into()) });
        rooted!(in(cx) let dep_evaluation = dep_evaluation.get().to_object());
        rooted!(in(cx) let importer = importer_private);
        assert!(unsafe {
            FinishDynamicModuleImport(cx, dep_evaluation.handle().into(), importer.handle().into(), request.handle().into(), promise.handle().into())
        });
        unsafe { RunJobs(cx) };
        assert_eq!(eval(&runtime, "dynamic").to_int32(), 40);
        assert_eq!(describe(&runtime, eval(&runtime, "typeof pending.then")), "function");
    }
}

mod structured_clone {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::glue::{CopyJSStructuredCloneData, GetLengthOfJSStructuredCloneData, WriteBytesToJSStructuredCloneData};
    use crate::rust::{JSAutoStructuredCloneBufferWrapper, JSEngineHandle, Runtime};

    static CLASS: JSClass = JSClass {
        name: c"Thing".as_ptr(),
        flags: 1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT,
        cOps: ptr::null(),
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    thread_local! {
        static REPORTED: Cell<u32> = const { Cell::new(0) };
    }

    unsafe extern "C" fn write(_cx: *mut JSContext, w: *mut JSStructuredCloneWriter, obj: HandleObject, _same: *mut bool, _closure: *mut c_void) -> bool {
        let mut slot = UndefinedValue();
        unsafe { JS_GetReservedSlot(obj.get(), 0, &mut slot) };
        unsafe { JS_WriteUint32Pair(w, 0xBEEF, slot.to_int32() as u32) && JS_WriteBytes(w, b"abc".as_ptr() as *const c_void, 3) }
    }

    unsafe extern "C" fn read(cx: *mut JSContext, r: *mut JSStructuredCloneReader, _policy: *const CloneDataPolicy, tag: u32, data: u32, _closure: *mut c_void) -> *mut JSObject {
        assert_eq!(tag, 0xBEEF);
        let mut bytes = [0u8; 3];
        assert!(unsafe { JS_ReadBytes(r, bytes.as_mut_ptr() as *mut c_void, 3) });
        assert_eq!(&bytes, b"abc");
        let object = unsafe { JS_NewObject(cx, &CLASS) };
        unsafe { JS_SetReservedSlot(object, 0, &Int32Value(data as i32 + 1)) };
        object
    }

    unsafe extern "C" fn report(_cx: *mut JSContext, _error: u32, _closure: *mut c_void, _message: *const std::ffi::c_char) {
        REPORTED.with(|count| count.set(count.get() + 1));
    }

    unsafe extern "C" fn write_transfer(
        _cx: *mut JSContext,
        _obj: Handle<*mut JSObject>,
        _closure: *mut c_void,
        tag: *mut u32,
        ownership: *mut TransferableOwnership,
        content: *mut *mut c_void,
        extra: *mut u64,
    ) -> bool {
        unsafe {
            *tag = 9;
            *ownership = TransferableOwnership::SCTAG_TMO_CUSTOM;
            *content = 0x1000 as *mut c_void;
            *extra = 5;
        }
        true
    }

    unsafe extern "C" fn read_transfer(
        cx: *mut JSContext,
        _r: *mut JSStructuredCloneReader,
        _policy: *const CloneDataPolicy,
        tag: u32,
        content: *mut c_void,
        extra: u64,
        _closure: *mut c_void,
        mut out: MutableHandleObject,
    ) -> bool {
        assert_eq!((tag, content as usize, extra), (9, 0x1000, 5));
        let object = unsafe { JS_NewObject(cx, &CLASS) };
        unsafe { JS_SetReservedSlot(object, 0, &Int32Value(100)) };
        out.set(object);
        true
    }

    static CALLBACKS: JSStructuredCloneCallbacks = JSStructuredCloneCallbacks {
        read: Some(read),
        write: Some(write),
        reportError: Some(report),
        readTransfer: Some(read_transfer),
        writeTransfer: Some(write_transfer),
        freeTransfer: None,
        canTransfer: None,
        sabCloned: None,
    };

    fn slot(object: *mut JSObject) -> i32 {
        let mut value = UndefinedValue();
        unsafe { JS_GetReservedSlot(object, 0, &mut value) };
        value.to_int32()
    }

    #[test]
    fn structured_clone_round_trips_values_host_objects_and_transfers() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let thing = unsafe { JS_NewObject(cx, &CLASS) });
        unsafe { JS_SetReservedSlot(thing.get(), 0, &Int32Value(41)) };
        rooted!(in(cx) let port = unsafe { JS_NewObject(cx, &CLASS) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        for (name, object) in [(c"thing", thing.get()), (c"port", port.get())] {
            rooted!(in(cx) let value = ObjectValue(object));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), name.as_ptr(), value.handle().into_handle()) });
        }
        rooted!(in(cx) let message = eval(
            &runtime,
            "globalThis.buffer = new Uint8Array([1, 2, 3]).buffer; \
             ({ text: 'hi', list: [1, { deep: true }], map: new Map([['k', 2]]), when: new Date(5), thing, port, buffer })",
        ));
        rooted!(in(cx) let transfer = eval(&runtime, "[buffer, port]"));
        let policy = CloneDataPolicy { allowIntraClusterClonableSharedObjects_: false, allowSharedMemoryObjects_: false };
        let buffer = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
        let data = unsafe { &mut (*buffer.as_raw_ptr()).data_ };
        assert!(unsafe {
            JS_WriteStructuredClone(cx, message.handle().into(), data, StructuredCloneScope::DifferentProcess, &policy, &CALLBACKS, ptr::null_mut(), transfer.handle().into())
        });
        assert!(eval(&runtime, "buffer.byteLength === 0").to_boolean(), "transferred buffers are detached");

        // Bytes only: as if they crossed a process boundary.
        let mut bytes = vec![0u8; unsafe { GetLengthOfJSStructuredCloneData(data) }];
        unsafe { CopyJSStructuredCloneData(data, bytes.as_mut_ptr()) };
        let received = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
        let received_data = unsafe { &mut (*received.as_raw_ptr()).data_ };
        assert!(unsafe { WriteBytesToJSStructuredCloneData(bytes.as_ptr(), bytes.len(), received_data) });
        rooted!(in(cx) let mut copy = UndefinedValue());
        assert!(unsafe {
            JS_ReadStructuredClone(cx, received_data, JS_STRUCTURED_CLONE_VERSION, StructuredCloneScope::DifferentProcess, copy.handle_mut().into(), &policy, &CALLBACKS, ptr::null_mut())
        });
        rooted!(in(cx) let copy_value = copy.get());
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"copy".as_ptr(), copy_value.handle().into_handle()) });
        assert_eq!(describe(&runtime, eval(&runtime, "copy.text + copy.list[1].deep + copy.map.get('k') + copy.when.getTime()")), "hitrue25");
        assert_eq!(describe(&runtime, eval(&runtime, "Array.from(new Uint8Array(copy.buffer)).join()")), "1,2,3");
        assert_eq!(slot(eval(&runtime, "copy.thing").to_object()), 42, "host objects go through the read/write callbacks");
        assert_eq!(slot(eval(&runtime, "copy.port").to_object()), 100, "transferred host objects come from readTransfer");
        assert!(eval(&runtime, "copy.thing !== thing").to_boolean());

        // Unclonable values report through the callback without a pending exception.
        rooted!(in(cx) let bad = eval(&runtime, "({ f() {} })"));
        rooted!(in(cx) let no_transfer = UndefinedValue());
        let failing = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
        let failing_data = unsafe { &mut (*failing.as_raw_ptr()).data_ };
        assert!(!unsafe {
            JS_WriteStructuredClone(cx, bad.handle().into(), failing_data, StructuredCloneScope::DifferentProcess, &policy, &CALLBACKS, ptr::null_mut(), no_transfer.handle().into())
        });
        assert_eq!(REPORTED.with(Cell::get), 1);
        assert!(!unsafe { crate::api::JS_IsExceptionPending(cx) });
    }
}

mod window_proxies {
    use std::ptr;

    use super::{describe, eval};
    use crate::glue::*;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    static WINDOW_CLASS: JSClass = JSClass {
        name: c"Window".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL | (crate::object::JSCLASS_GLOBAL_SLOT_COUNT << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: ptr::null(),
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    #[test]
    fn window_proxies_forward_outerize_and_transplant() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let window = unsafe {
            JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, window.get()) };
        eval(&runtime, "globalThis.answer = 42; globalThis.greet = function () { return 'hi'; }");
        unsafe { LeaveRealm(cx, old) };

        // SAFETY: an all-`None` trap table: every operation forwards to the window.
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        rooted!(in(cx) let proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        assert!(!proxy.get().is_null());
        assert!(unsafe { IsWindowProxy(proxy.get()) });
        assert_eq!(unsafe { ToWindowIfWindowProxy(proxy.get()) }, window.get());
        unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
        assert!(unsafe { IsWindowSlow(window.get()) });
        assert_eq!(unsafe { ToWindowProxyIfWindowSlow(window.get()) }, proxy.get());
        // Scripts see the window proxy as `globalThis`: one object (V8's global proxy).
        let old = unsafe { EnterRealm(cx, window.get()) };
        assert_eq!(eval(&runtime, "globalThis").to_object(), proxy.get(), "window === globalThis");
        unsafe { LeaveRealm(cx, old) };

        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let proxy_value = ObjectValue(proxy.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"w".as_ptr(), proxy_value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "w.answer").to_int32(), 42);
        assert_eq!(describe(&runtime, eval(&runtime, "w.greet()")), "hi");
        assert_eq!(eval(&runtime, "w.added = 7; w.added").to_int32(), 7);
        assert!(eval(&runtime, "'answer' in w && Object.keys(w).includes('added')").to_boolean());

        // Transplanting keeps the proxy's identity and takes the new wrapped window.
        rooted!(in(cx) let other_window = unsafe {
            JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, other_window.get()) };
        eval(&runtime, "globalThis.answer = 1");
        unsafe { LeaveRealm(cx, old) };
        rooted!(in(cx) let replacement = unsafe { NewWindowProxy(cx, other_window.handle().into(), handler) });
        // Window proxies on V8 global proxies: the replacement becomes the window proxy.
        let transplanted = unsafe { JS_TransplantObject(cx, proxy.handle().into(), replacement.handle().into()) };
        assert_eq!(transplanted, replacement.get());
        assert_eq!(unsafe { ToWindowIfWindowProxy(transplanted) }, other_window.get());
    }
}

mod callable_classes {
    use std::ptr;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    /// A constructor native that returns `{ newTarget }` (`vp` holds new.target after the args).
    unsafe extern "C" fn construct(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 3) };
        let new_target = args[argc as usize + 2];
        crate::rooted!(in(cx) let result = unsafe { JS_NewPlainObject(cx) });
        crate::rooted!(in(cx) let new_target = new_target);
        unsafe { JS_SetProperty(cx, result.handle().into_handle(), c"newTarget".as_ptr(), new_target.handle().into_handle()) };
        args[0] = ObjectValue(result.get());
        true
    }

    static OPS: JSClassOps = JSClassOps {
        addProperty: None,
        delProperty: None,
        enumerate: None,
        newEnumerate: None,
        resolve: None,
        mayResolve: None,
        finalize: None,
        call: Some(construct),
        construct: Some(construct),
        trace: None,
    };

    static CLASS: JSClass = JSClass {
        name: c"Iface".as_ptr(),
        flags: 0,
        cOps: &OPS,
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    #[test]
    fn callable_class_objects_are_functions_with_new_target() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let iface = unsafe { JS_NewObject(cx, &CLASS) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(iface.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"Iface".as_ptr(), value.handle().into_handle()) });
        assert_eq!(describe(&runtime, eval(&runtime, "typeof Iface")), "function");
        assert_eq!(eval(&runtime, "Iface").to_object(), iface.get(), "the function keeps its identity");
        assert!(eval(&runtime, "new Iface().newTarget === Iface").to_boolean());
        assert!(eval(&runtime, "class Sub extends Iface {}; new Sub().newTarget === Sub").to_boolean(), "super() passes the subclass as new.target");
        assert!(eval(&runtime, "Reflect.construct(Iface, [], Array).newTarget === Array").to_boolean());
    }
}

mod embedder_job_queue {
    use std::cell::RefCell;
    use std::ffi::c_void;

    use super::eval;
    use crate::glue::{CreateJobQueue, JobQueueTraps};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    thread_local! {
        static JOBS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn enqueue(
        _queue: *const c_void,
        _cx: *mut JSContext,
        _promise: HandleObject,
        job: HandleObject,
        _site: HandleObject,
        _data: HandleObject,
    ) -> bool {
        JOBS.with(|jobs| jobs.borrow_mut().push(job.get() as usize));
        true
    }

    #[test]
    fn promise_jobs_run_at_the_embedders_checkpoint() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // SAFETY: an all-`None` trap table, plus the enqueue trap.
        let mut traps: JobQueueTraps = unsafe { std::mem::zeroed() };
        traps.enqueuePromiseJob = Some(enqueue);
        let queue = unsafe { CreateJobQueue(&traps, std::ptr::null(), std::ptr::null_mut()) };
        unsafe { SetJobQueue(cx, queue) };

        eval(&runtime, "globalThis.log = []; Promise.resolve(1).then(v => log.push(v)); Promise.resolve(2).then(v => log.push(v));");
        assert_eq!(eval(&runtime, "log.length").to_int32(), 0, "V8 does not run promise jobs by itself");
        let jobs = JOBS.with(|jobs| jobs.borrow_mut().split_off(0));
        assert_eq!(jobs.len(), 1, "one drain job per checkpoint");

        // The embedder's checkpoint runs the job: V8's pending reactions run, in order.
        rooted!(in(cx) let job = ObjectValue(jobs[0] as *mut JSObject));
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let mut rval = UndefinedValue());
        let no_args = HandleValueArray { length_: 0, elements_: std::ptr::null() };
        assert!(unsafe { Call(cx, this.handle().into_handle(), job.handle().into_handle(), &no_args, rval.handle_mut().into_handle()) });
        assert_eq!(super::describe(&runtime, eval(&runtime, "log.join()")), "1,2");
        // Later promises request a new drain job.
        eval(&runtime, "Promise.resolve(3).then(v => log.push(v))");
        assert_eq!(JOBS.with(|jobs| jobs.borrow().len()), 1);
    }
}

thread_local! {
    static GC_EVENTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

unsafe extern "C" fn record_gc(
    _: *mut crate::jsapi::JSContext,
    status: crate::jsapi::JSGCStatus,
    _: crate::jsapi::GCReason,
    data: *mut std::os::raw::c_void,
) {
    GC_EVENTS.with(|events| events.borrow_mut().push(format!("{status:?} {}", data as usize)));
}

unsafe extern "C" fn record_gc_slice(
    _: *mut crate::jsapi::JSContext,
    progress: crate::jsapi::GCProgress,
    _: *const crate::jsapi::GCDescription,
) {
    GC_EVENTS.with(|events| events.borrow_mut().push(format!("{progress:?}")));
}

#[test]
fn gc_callbacks_report_full_collections() {
    let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    // SAFETY: the runtime's live context.
    unsafe {
        crate::jsapi::JS_SetGCCallback(cx, Some(record_gc), 7 as *mut _);
        crate::jsapi::SetGCSliceCallback(cx, Some(record_gc_slice));
    }
    runtime.gc_for_testing();
    let events = GC_EVENTS.with(|events| events.take());
    let cycle = ["JSGC_BEGIN 7", "GC_CYCLE_BEGIN", "GC_SLICE_BEGIN", "GC_SLICE_END", "GC_CYCLE_END", "JSGC_END 7"];
    assert!(events.len() >= cycle.len() && events.len() % cycle.len() == 0, "{events:?}");
    for chunk in events.chunks(cycle.len()) {
        assert_eq!(chunk, cycle);
    }
    // SAFETY: as above.
    unsafe {
        crate::jsapi::JS_SetGCCallback(cx, None, std::ptr::null_mut());
        crate::jsapi::SetGCSliceCallback(cx, None);
    }
    runtime.gc_for_testing();
    assert!(GC_EVENTS.with(|events| events.borrow().is_empty()));
}
