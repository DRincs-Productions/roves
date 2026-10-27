/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::cell::FINALIZED;
use crate::jsapi::JSObject;
use crate::jsval::*;
use crate::rust::Runtime;

/// Evaluates `source` in the runtime's realm and returns the (unrooted) result.
pub(crate) fn eval(runtime: &Runtime, source: &str) -> JSVal {
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
    fn cross_origin_holders_use_ephemeron_ownership_and_captured_intrinsics() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let before = HOOK_FINALIZED.with(Cell::get);
        {
            rooted!(in(cx) let key = unsafe { JS_NewObject(cx, &CLASS) });
            unsafe { JS_SetReservedSlot(key.get(), 0, &Int32Value(42)) };
            {
                rooted!(in(cx) let holder = unsafe { JS_NewObject(cx, &CLASS) });
                unsafe { JS_SetReservedSlot(holder.get(), 0, &Int32Value(42)) };
                assert!(unsafe { crate::proxy::SetCrossOriginPropertyHolder(cx,
                    key.handle().into(), holder.handle().into()) });
            }
            eval(&runtime, "WeakMap.prototype.get = () => { throw 1 }; WeakMap.prototype.set = () => { throw 2 }; globalThis.WeakMap = () => { throw 3 }");
            runtime.gc_for_testing();
            rooted!(in(cx) let mut cached = ptr::null_mut::<JSObject>());
            assert!(unsafe { crate::proxy::GetCrossOriginPropertyHolder(cx,
                key.handle().into(), cached.handle_mut().into()) });
            assert!(!cached.get().is_null());
            let mut marker = UndefinedValue();
            unsafe { JS_GetReservedSlot(cached.get(), 0, &mut marker) };
            assert_eq!(marker.to_int32(), 42);
            assert_eq!(HOOK_FINALIZED.with(Cell::get), before);
        }
        for _ in 0..3 {
            unsafe { &mut *(*cx).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            runtime.gc_for_testing();
        }
        assert_eq!(HOOK_FINALIZED.with(Cell::get), before + 2,
            "the realm cache must not keep a dead key or its holder alive");
    }

    #[test]
    fn partially_constructed_class_boxes_trace_values_before_native_hooks() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let raw = unsafe { &*runtime.raw_cx() };
        let owner = raw.with_scope(|scope| {
            let owner = crate::object::new_class_box(scope, &CLASS);
            v8::cppgc::Persistent::new(&owner)
        });
        let owner_ref = owner.get().unwrap();
        owner_ref.set_reserved_slot(1, eval(&runtime, "({marker: 'slot'})"));
        owner_ref.private.set(eval(&runtime, "({marker: 'private'})"));
        let traced_before = HOOK_TRACED.with(Cell::get);
        runtime.gc_for_testing();
        assert_eq!(HOOK_TRACED.with(Cell::get), traced_before,
            "native hooks cannot inspect an object before its cell is initialized");
        assert_eq!(describe(&runtime, owner_ref.reserved_slot(1)), "[object Object]");
        assert_eq!(describe(&runtime, owner_ref.private.get()), "[object Object]");
        // Finish the construction stage: subsequent collections must invoke the hook.
        owner_ref.cell.set(eval(&runtime, "({})").to_object().cast());
        runtime.gc_for_testing();
        assert!(HOOK_TRACED.with(Cell::get) > traced_before);
        drop(owner);
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
    fn lifetime_stress_keeps_reachable_objects_and_finalizes_the_rest_once() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        const COUNT: usize = 4000;
        let finalized_before = HOOK_FINALIZED.with(Cell::get);
        eval(&runtime, "globalThis.kept = []");
        rooted!(in(cx) let kept = eval(&runtime, "kept").to_object());
        let mut kept_pointers = Vec::new();
        for i in 0..COUNT {
            rooted!(in(cx) let thing = unsafe { JS_NewObject(cx, &CLASS) });
            unsafe { JS_SetReservedSlot(thing.get(), 0, &Int32Value(42)) };
            unsafe { JS_SetReservedSlot(thing.get(), 1, &Int32Value(i as i32)) };
            if i % 2 == 0 {
                // Reachable only from JavaScript (an array on the global).
                rooted!(in(cx) let value = ObjectValue(thing.get()));
                let name = std::ffi::CString::new((i / 2).to_string()).unwrap();
                assert!(unsafe { JS_SetProperty(cx, kept.handle().into_handle(), name.as_ptr(), value.handle().into_handle()) });
                kept_pointers.push(thing.get() as usize);
            }
            if i % 500 == 0 {
                // Collections interleaved with allocation.
                runtime.gc_for_testing();
            }
        }
        for _ in 0..3 {
            eval(&runtime, "{ let garbage = []; for (let i = 0; i < 20000; i++) garbage.push({ i }); }");
            runtime.gc_for_testing();
        }
        assert_eq!(HOOK_FINALIZED.with(Cell::get), finalized_before + (COUNT / 2) as u32, "every unreachable object is finalized once");
        // The reachable half keeps its identity and its reserved slots.
        for (index, pointer) in kept_pointers.iter().enumerate().step_by(97) {
            let object = eval(&runtime, &format!("kept[{index}]")).to_object();
            assert_eq!(object as usize, *pointer, "identity of kept[{index}]");
            let mut slot = UndefinedValue();
            unsafe { JS_GetReservedSlot(object, 1, &mut slot) };
            assert_eq!(slot.to_int32(), (index * 2) as i32);
        }
        // Dropping them finalizes the rest, once.
        eval(&runtime, "kept.length = 0");
        runtime.gc_for_testing();
        runtime.gc_for_testing();
        assert_eq!(HOOK_FINALIZED.with(Cell::get), finalized_before + COUNT as u32);
    }

    thread_local! {
        static BENCH_PROTO: Cell<*mut JSObject> = const { Cell::new(ptr::null_mut()) };
    }

    /// Creates one class object the way Servo's `wrap` does (enter the realm, given prototype,
    /// private in a reserved slot) and returns it.
    unsafe extern "C" fn make_wrapper(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        rooted!(in(cx) let proto = BENCH_PROTO.with(Cell::get));
        rooted!(in(cx) let obj = unsafe { JS_NewObjectWithGivenProto(cx, &CLASS, proto.handle().into_handle()) });
        unsafe { JS_SetReservedSlot(obj.get(), 0, &PrivateValue(8 as *const c_void)) };
        // Servo's `init_reflector` accounts the native object's size.
        unsafe { AddAssociatedMemory(obj.get(), 400, MemoryUse::DOMBinding) };
        args[0] = ObjectValue(obj.get());
        true
    }

    /// Takes a string argument as Servo's DOMString setters do (`jsstr_to_string`).
    unsafe extern "C" fn take_string(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        use crate::conversions::{ConversionResult, FromJSValConvertible};
        let mut safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        let value = unsafe { crate::rust::HandleValue::from_marked_location(&args[2]) };
        let Ok(ConversionResult::Success(text)) = std::string::String::safe_from_jsval(&mut safe, value, ()) else { return false };
        args[0] = Int32Value(text.len() as i32);
        true
    }

    /// Cost of creating DOM-like wrappers (`cargo test --release --lib bench_wrapper_creation
    /// -- --ignored --nocapture`).
    #[test]
    #[ignore = "benchmark"]
    fn bench_wrapper_creation() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let proto = unsafe { JS_NewPlainObject(cx) });
        BENCH_PROTO.with(|cell| cell.set(proto.get()));
        const N: u32 = 100_000;
        for round in 0..3 {
            let start = std::time::Instant::now();
            for _ in 0..N {
                rooted!(in(cx) let obj = unsafe { JS_NewObjectWithGivenProto(cx, &CLASS, proto.handle().into_handle()) });
                unsafe { JS_SetReservedSlot(obj.get(), 0, &PrivateValue(8 as *const c_void)) };
            }
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH round {round} JS_NewObjectWithGivenProto + slot (Rust loop): {ns:.0} ns/object");
        }
        {
            let start = std::time::Instant::now();
            for _ in 0..N {
                rooted!(in(cx) let obj = unsafe { JS_NewObject(cx, &CLASS) });
                unsafe { JS_SetReservedSlot(obj.get(), 0, &PrivateValue(8 as *const c_void)) };
            }
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH JS_NewObject (default prototype, no SetPrototype) + slot: {ns:.0} ns/object");
            let start = std::time::Instant::now();
            for _ in 0..N {
                rooted!(in(cx) let obj = unsafe { JS_NewPlainObject(cx) });
            }
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH JS_NewPlainObject: {ns:.0} ns/object");
        }
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        for (name, native) in [(c"make", make_wrapper as unsafe extern "C" fn(*mut JSContext, u32, *mut JSVal) -> bool), (c"take", take_string)] {
            let function = unsafe { JS_NewFunction(cx, Some(native), 1, 0, name.as_ptr()) };
            rooted!(in(cx) let value = ObjectValue(unsafe { JS_GetFunctionObject(function) }));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), name.as_ptr(), value.handle().into_handle()) });
        }
        for (label, body) in [
            ("script calls native that wraps (createElement-like)", "for (let i = 0; i < N; i++) s += make() ? 1 : 0;"),
            ("script passes a string to a native (setter-like)", "for (let i = 0; i < N; i++) s += take('opacity 0.5');"),
            ("script passes a fresh string to a native", "for (let i = 0; i < N; i++) s += take('x' + i);"),
        ] {
            let source = format!("(() => {{ const N = {N}; let s = 0; {body} return s; }})()");
            eval(&runtime, &source); // warm-up
            let start = std::time::Instant::now();
            eval(&runtime, &source);
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH {label}: {ns:.0} ns/op");
        }
        // The same with a large live heap: a "DOM" of wrappers reachable from script, and
        // garbage JS objects, as on a real page.
        eval(&runtime, "globalThis.live = []; for (let i = 0; i < 50000; i++) live.push(make()); globalThis.junk = []; for (let i = 0; i < 200000; i++) junk.push({ i, s: 'x' + i });");
        for round in 0..3 {
            let source = format!("(() => {{ const N = {N}; let s = 0; for (let i = 0; i < N; i++) s += make() ? 1 : 0; return s; }})()");
            let start = std::time::Instant::now();
            eval(&runtime, &source);
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH round {round} createElement-like with 50k live wrappers: {ns:.0} ns/op");
        }
        for round in 0..2 {
            let source = format!("(() => {{ const N = {N}; let s = 0; for (let i = 0; i < N; i++) s += take('opacity 0.5'); return s; }})()");
            let start = std::time::Instant::now();
            eval(&runtime, &source);
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH round {round} setter-like with 50k live wrappers: {ns:.0} ns/op");
        }
        BENCH_PROTO.with(|cell| cell.set(ptr::null_mut()));
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

    /// `JS_GetPropertyDescriptorById` is the prototype walk of WebIDL named property visibility
    /// (script_bindings `has_property_on_prototype`): every object on the chain is asked for an
    /// own property ([[GetOwnProperty]]) and the walk moves on with [[GetPrototypeOf]], so a
    /// Proxy answers through `getOwnPropertyDescriptor`/`getPrototypeOf`, never `has`. The two
    /// traps disagree on purpose for `hasOnly`/`descriptorOnly`.
    #[test]
    fn property_descriptor_walk_uses_own_descriptor_and_prototype_traps() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        eval(&runtime, r#"
            globalThis.log = [];
            globalThis.getterCalls = 0;
            globalThis.deep = { deepKey: 'deep' };
            globalThis.middle = Object.create(deep, {
                undefinedKey: { configurable: true, writable: true, value: undefined },
                accessorKey: { configurable: true, get() { getterCalls++; return 'getter'; } },
            });
            globalThis.trapped = new Proxy(middle, {
                getOwnPropertyDescriptor(target, key) {
                    log.push('descriptor:' + String(key));
                    if (key === 'throwKey') throw new RangeError('descriptor trap');
                    if (key === 'descriptorOnly') return { configurable: true, writable: true, value: 'trap' };
                    return Reflect.getOwnPropertyDescriptor(target, key);
                },
                getPrototypeOf(target) { log.push('prototype'); return Reflect.getPrototypeOf(target); },
                has(target, key) { log.push('has:' + String(key)); return key === 'hasOnly' || Reflect.has(target, key); },
            });
            globalThis.nullEnded = new Proxy(Object.create(null), {
                getOwnPropertyDescriptor(target, key) { log.push('descriptor:' + String(key)); return undefined; },
                getPrototypeOf() { log.push('prototype'); return null; },
                has(target, key) { log.push('has:' + String(key)); return true; },
            });
        "#);
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let trapped_object = eval(&runtime, "trapped").to_object());
        rooted!(in(cx) let null_ended = eval(&runtime, "nullEnded").to_object());
        // Returns (succeeded, is_none, descriptor); the holder is stored as `globalThis.holder`.
        let lookup = |object: HandleObject, name: &std::ffi::CStr| {
            eval(&runtime, "log.length = 0; globalThis.holder = undefined");
            let atom = unsafe { JS_AtomizeAndPinString(cx, name.as_ptr()) };
            rooted!(in(cx) let mut id = crate::jsid::VoidId());
            unsafe { RUST_INTERNED_STRING_TO_JSID(cx, atom, id.handle_mut().into_handle()) };
            rooted!(in(cx) let mut desc = PropertyDescriptor::default());
            rooted!(in(cx) let mut holder = ptr::null_mut::<JSObject>());
            let mut is_none = true;
            let succeeded = unsafe {
                JS_GetPropertyDescriptorById(cx, object, id.handle().into_handle(), desc.handle_mut().into_handle(), holder.handle_mut().into(), &mut is_none)
            };
            if succeeded && !is_none {
                rooted!(in(cx) let value = ObjectValue(holder.get()));
                assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"holder".as_ptr(), value.handle().into_handle()) });
            }
            (succeeded, is_none, *desc)
        };
        let log = || describe(&runtime, eval(&runtime, "log.join()"));
        let trapped = trapped_object.handle().into_handle();

        // An own data property whose value is undefined is found on the Proxy itself.
        let (ok, is_none, desc) = lookup(trapped, c"undefinedKey");
        assert!(ok && !is_none && desc.hasValue_() && desc.value_.is_undefined());
        assert_eq!(log(), "descriptor:undefinedKey");
        assert_eq!(describe(&runtime, eval(&runtime, "holder === trapped")), "true");
        // An accessor is reported without calling its getter.
        let (ok, is_none, desc) = lookup(trapped, c"accessorKey");
        assert!(ok && !is_none && desc.hasGetter_() && !desc.getter_.is_null());
        assert_eq!(log(), "descriptor:accessorKey");
        assert_eq!(describe(&runtime, eval(&runtime, "getterCalls")), "0");
        // Inherited: the walk leaves the Proxy through its getPrototypeOf trap.
        let (ok, is_none, _) = lookup(trapped, c"deepKey");
        assert!(ok && !is_none);
        assert_eq!(log(), "descriptor:deepKey,prototype");
        assert_eq!(describe(&runtime, eval(&runtime, "holder === deep")), "true");
        // `has` says present, the descriptor says absent: not found, `has` never runs.
        let (ok, is_none, _) = lookup(trapped, c"hasOnly");
        assert!(ok && is_none);
        assert_eq!(log(), "descriptor:hasOnly,prototype");
        // The descriptor trap alone decides presence.
        let (ok, is_none, desc) = lookup(trapped, c"descriptorOnly");
        assert!(ok && !is_none && desc.hasValue_());
        assert_eq!(log(), "descriptor:descriptorOnly");
        // A null prototype ends the walk.
        let (ok, is_none, _) = lookup(null_ended.handle().into_handle(), c"missing");
        assert!(ok && is_none);
        assert_eq!(log(), "descriptor:missing,prototype");
        // A throwing descriptor trap fails the call with its exception pending, unchanged.
        let (ok, _, _) = lookup(trapped, c"throwKey");
        assert!(!ok);
        assert_eq!(log(), "descriptor:throwKey");
        rooted!(in(cx) let mut exception = UndefinedValue());
        assert!(unsafe { JS_GetPendingException(cx, exception.handle_mut()) });
        assert_eq!(describe(&runtime, exception.get()), "RangeError: descriptor trap");
        unsafe { JS_ClearPendingException(cx) };
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
    fn is_array_object_follows_the_builtin_class_contract() {
        // SpiderMonkey's JS::IsArrayObject compares the builtin class (ESClass::Array): scripted
        // proxies report ESClass::Other, so a proxied or revoked-proxy array is not an array and
        // nothing is thrown. It is not the ECMAScript IsArray abstract operation, and
        // script_bindings::is_array_like asserts the call never fails.
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        eval(&runtime, "Array.isArray = () => { throw new Error('poisoned'); };");
        for (source, expected) in [
            ("[1, 2]", true),
            ("new (class extends Array {})()", true),
            ("new Proxy([], {})", false),
            ("new Proxy(new Proxy([], {}), {})", false),
            ("(() => { const r = Proxy.revocable([], {}); r.revoke(); return r.proxy; })()", false),
            ("({ length: 0 })", false),
            ("(function () {})", false),
            ("'array'", false),
            ("undefined", false),
        ] {
            rooted!(in(cx) let value = eval(&runtime, source));
            let mut is_array = !expected;
            assert!(unsafe { IsArrayObject(cx, value.handle().into_handle(), &mut is_array) }, "{source}");
            assert_eq!(is_array, expected, "{source}");
            assert!(!unsafe { JS_IsExceptionPending(cx) }, "{source}");
        }
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

    static DEFINED_INFO_SEEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn defined_info_getter(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract: `vp[0]` is the callee.
        let info = unsafe { RUST_FUNCTION_VALUE_TO_JITINFO(*vp) };
        DEFINED_INFO_SEEN.store(info as usize, std::sync::atomic::Ordering::SeqCst);
        // SAFETY: the JSNative contract.
        unsafe { *vp = Int32Value(42) };
        true
    }

    #[test]
    fn defined_native_functions_are_owned_by_their_holders() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let seen = || DEFINED_INFO_SEEN.load(std::sync::atomic::Ordering::SeqCst);
        // SAFETY: the runtime's live context (for every closure below).
        let states = || unsafe { (*cx).native_functions.borrow().len() };
        let state_alive = |index: usize| unsafe { (*cx).native_functions.borrow()[index].alive() };
        let state_has_cell = |index: usize| unsafe { (*cx).native_functions.borrow()[index].weak_cell.get().is_some() };
        // A V8 weak handle on an object, to see it collected.
        let weak_of = |object: *mut JSObject| {
            unsafe { &*cx }.with_scope(|scope| {
                let value = unsafe { to_v8(scope, ObjectValue(object)) };
                v8::Weak::new(scope, v8::Local::<v8::Object>::try_from(value).unwrap())
            })
        };
        let full_gc = |runtime: &mut Runtime| {
            for _ in 0..3 {
                runtime.gc_for_testing();
                unsafe { &mut *(*cx).isolate }.low_memory_notification();
            }
        };
        // A spec-defined method, made as `JS_DefineFunctions`/`JS_DefineProperties` make one
        // (the JIT info is only compared, never read).
        let info = 0x5a50 as *const crate::jsapi::JSJitInfo;
        let define = || {
            crate::jsapi_impl::new_native_function(unsafe { &*cx }, Some(defined_info_getter), 1, "method", false, info)
                as *mut JSObject
        };
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let owner = eval(&runtime, "({})").to_object());
        rooted!(in(cx) let owner_value = ObjectValue(owner.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"owner".as_ptr(), owner_value.handle().into_handle()) });
        let method;
        let (weak_method, weak_held) = {
            rooted!(in(cx) let function = define());
            assert!(!function.get().is_null());
            method = states() - 1;
            rooted!(in(cx) let held = eval(&runtime, "({ toString() { return 'owned by the method'; } })"));
            unsafe { SetFunctionNativeReserved(function.get(), 0, &held.get()) };
            rooted!(in(cx) let function_value = ObjectValue(function.get()));
            assert!(unsafe { JS_SetProperty(cx, owner.handle().into_handle(), c"method".as_ptr(), function_value.handle().into_handle()) });
            (weak_of(function.get()), weak_of(held.get().to_object()))
        };
        // The rooted owner keeps the method and, through it, the reserved slot's object; the
        // method's cell, rooted by nobody, is collected and made again on the next call, with
        // the same callback, JIT info and reserved slot's object.
        full_gc(&mut runtime);
        assert!(state_alive(method) && !weak_method.is_empty(), "the owner keeps its method");
        assert!(!weak_held.is_empty(), "the method keeps its reserved slot's object");
        assert!(!state_has_cell(method), "the method's unrooted cell is collected");
        assert_eq!(eval(&runtime, "globalThis.saved = owner.method; owner.method(1)").to_int32(), 42);
        assert_eq!(seen(), info as usize);
        assert!(state_has_cell(method), "the call made a new cell");
        {
            rooted!(in(cx) let function = eval(&runtime, "owner.method").to_object());
            assert_eq!(unsafe { RUST_FUNCTION_VALUE_TO_JITINFO(ObjectValue(function.get())) }, info);
            let slot = unsafe { *GetFunctionNativeReserved(function.get(), 0) };
            assert_eq!(describe(&runtime, slot), "owned by the method");
        }
        full_gc(&mut runtime);
        let same = "owner.method === saved && owner.method.name === 'method' && owner.method.length === 1 ? 1 : 0";
        assert_eq!(eval(&runtime, same).to_int32(), 1, "the method keeps its identity, name and length");
        let throws = "try { new owner.method(); 0 } catch (error) { error instanceof TypeError ? 1 : 0 }";
        assert_eq!(eval(&runtime, throws).to_int32(), 1, "a method is not a constructor");
        DEFINED_INFO_SEEN.store(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(eval(&runtime, "saved(1)").to_int32(), 42);
        assert_eq!(seen(), info as usize);
        // Deleting the property and dropping every reference releases the method and the
        // reserved slot's object (a private property of it) before the runtime goes.
        eval(&runtime, "delete owner.method; delete globalThis.saved; 0");
        full_gc(&mut runtime);
        assert!(weak_method.is_empty(), "a released method is collected");
        assert!(weak_held.is_empty(), "a released method's reserved slot's object is collected");
        assert!(!state_alive(method), "a released method's state is dead");
        // Dead states are swept; later methods get their own callback data.
        for _ in 0..1000 {
            assert!(!define().is_null());
        }
        full_gc(&mut runtime);
        for _ in 0..100 {
            assert!(!define().is_null());
        }
        assert!(states() < 200, "dead states are swept: {}", states());
        rooted!(in(cx) let later = define());
        rooted!(in(cx) let later_value = ObjectValue(later.get()));
        assert!(unsafe { JS_SetProperty(cx, owner.handle().into_handle(), c"later".as_ptr(), later_value.handle().into_handle()) });
        full_gc(&mut runtime);
        DEFINED_INFO_SEEN.store(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(eval(&runtime, "owner.later(1)").to_int32(), 42);
        assert_eq!(seen(), info as usize);
    }

    unsafe extern "C" fn answer_getter(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        // Servo's generic getter reads its JIT info from the callee on every call.
        // SAFETY: the JSNative contract: `vp[0]` is the callee.
        let _info = unsafe { RUST_FUNCTION_VALUE_TO_JITINFO(*vp) };
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

    unsafe extern "C" fn noop_native(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        unsafe { *vp = Int32Value(1) };
        true
    }

    /// Calls its first argument (a function) once: a native calling back into script, the way
    /// Servo runs event listeners and timer callbacks.
    unsafe extern "C" fn invoke_native(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let function = args[2]);
        rooted!(in(cx) let mut rval = UndefinedValue());
        let no_args = HandleValueArray { length_: 0, elements_: std::ptr::null() };
        // Servo's callbacks enter the callee's realm around the call (AutoRealm).
        rooted!(in(cx) let callee = args[2].to_object());
        let old = unsafe { EnterRealm(cx, callee.get()) };
        let ok = unsafe { Call(cx, this.handle().into_handle(), function.handle().into_handle(), &no_args, rval.handle_mut().into_handle()) };
        unsafe { LeaveRealm(cx, old) };
        args[0] = rval.get();
        ok
    }

    /// Per-call cost of natives called from script (`cargo test --release --lib
    /// bench_native_calls -- --ignored --nocapture`): the binding hot path DOM code takes.
    #[test]
    #[ignore = "benchmark"]
    fn bench_native_calls() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let proto = unsafe { GetRealmObjectPrototype(cx) });
        rooted!(in(cx) let object = unsafe { JS_NewObjectWithGivenProto(cx, ptr::null(), proto.handle().into_handle()) });
        assert!(unsafe { JS_DefineProperties(cx, object.handle().into_handle(), PROPERTIES.as_ptr()) });
        let noop = unsafe { JS_NewFunction(cx, Some(noop_native), 0, 0, c"noop".as_ptr()) };
        rooted!(in(cx) let noop = ObjectValue(unsafe { JS_GetFunctionObject(noop) }));
        assert!(unsafe { JS_SetProperty(cx, object.handle().into_handle(), c"noop".as_ptr(), noop.handle().into_handle()) });
        let invoke = unsafe { JS_NewFunction(cx, Some(invoke_native), 1, 0, c"invoke".as_ptr()) };
        rooted!(in(cx) let invoke = ObjectValue(unsafe { JS_GetFunctionObject(invoke) }));
        assert!(unsafe { JS_SetProperty(cx, object.handle().into_handle(), c"invoke".as_ptr(), invoke.handle().into_handle()) });
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let value = ObjectValue(object.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"spec".as_ptr(), value.handle().into_handle()) });
        // A class object (like a DOM wrapper) with the same accessor.
        rooted!(in(cx) let thing = unsafe { JS_NewObject(cx, &CLASS) });
        assert!(unsafe { JS_DefineProperties(cx, thing.handle().into_handle(), PROPERTIES.as_ptr()) });
        rooted!(in(cx) let thing_value = ObjectValue(thing.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"thing".as_ptr(), thing_value.handle().into_handle()) });
        const N: u32 = 200_000;
        for (label, body) in [
            ("native getter on a class object (DOM-like)", "for (let i = 0; i < N; i++) s += thing.answer;"),
            ("plain JS property", "const o = { a: 1 }; for (let i = 0; i < N; i++) s += o.a;"),
            ("native getter (accessor spec)", "for (let i = 0; i < N; i++) s += spec.answer;"),
            ("native method (JS_NewFunction)", "for (let i = 0; i < N; i++) s += spec.noop();"),
            ("script -> native -> script callback (Call)", "const f = () => 1; for (let i = 0; i < N; i++) s += spec.invoke(f);"),
        ] {
            let source = format!("(() => {{ const N = {N}; let s = 0; {body} return s; }})()");
            eval(&runtime, &source); // warm-up
            let start = std::time::Instant::now();
            eval(&runtime, &source);
            let ns = start.elapsed().as_nanos() as f64 / N as f64;
            eprintln!("BENCH {label}: {ns:.0} ns/op");
        }
    }

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

    #[test]
    fn immutable_prototypes_reject_replacement_but_allow_properties() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let object = unsafe { JS_NewObject(cx, &CLASS) });
        rooted!(in(cx) let prototype = unsafe { GetRealmObjectPrototype(cx) });
        rooted!(in(cx) let mut replacement = unsafe { JS_NewPlainObject(cx) });
        assert!(unsafe { JS_SetPrototype(cx, object.handle().into_handle(), prototype.handle().into_handle()) });
        let mut succeeded = false;
        assert!(unsafe { JS_SetImmutablePrototype(cx, object.handle().into_handle(), &mut succeeded) });
        assert!(succeeded);
        assert!(!unsafe { JS_SetPrototype(cx, object.handle().into_handle(), replacement.handle().into_handle()) });
        rooted!(in(cx) let global = global(&runtime).to_object());
        rooted!(in(cx) let object_value = ObjectValue(object.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"immutableTarget".as_ptr(), object_value.handle().into_handle()) });
        assert_eq!(eval(&runtime, "(Object.defineProperty(immutableTarget, 'ordinary', { value: 7 }), immutableTarget.ordinary)").to_int32(), 7);
    }

    fn collect_fully(runtime: &mut Runtime) {
        let cx = runtime.raw_cx();
        for _ in 0..3 {
            unsafe { &mut *(*cx).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            runtime.gc_for_testing();
        }
    }

    #[test]
    fn immutable_prototypes_live_with_their_object_and_die_with_it() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let before = HOOK_FINALIZED.with(Cell::get);
        {
            rooted!(in(cx) let object = unsafe { JS_NewObject(cx, &CLASS) });
            unsafe { JS_SetReservedSlot(object.get(), 0, &Int32Value(42)) };
            {
                // The prototype points back at the object: with a strong root on the stored
                // prototype, neither could ever be collected.
                rooted!(in(cx) let prototype = unsafe { JS_NewObject(cx, &CLASS) });
                unsafe { JS_SetReservedSlot(prototype.get(), 0, &Int32Value(42)) };
                rooted!(in(cx) let back = ObjectValue(object.get()));
                assert!(unsafe { JS_SetProperty(cx, prototype.handle().into_handle(), c"owner".as_ptr(), back.handle().into_handle()) });
                assert!(unsafe { JS_SetPrototype(cx, object.handle().into_handle(), prototype.handle().into_handle()) });
                let mut succeeded = false;
                assert!(unsafe { JS_SetImmutablePrototype(cx, object.handle().into_handle(), &mut succeeded) });
                assert!(succeeded);
            }
            collect_fully(&mut runtime);
            assert_eq!(HOOK_FINALIZED.with(Cell::get), before, "a rooted object keeps its immutable prototype");
            rooted!(in(cx) let mut prototype = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_GetPrototype(cx, object.handle().into_handle(), prototype.handle_mut().into()) });
            let mut marker = UndefinedValue();
            unsafe { JS_GetReservedSlot(prototype.get(), 0, &mut marker) };
            assert_eq!(marker.to_int32(), 42);
            // The stored prototype survived the collection: assigning it again succeeds, any
            // other prototype is still refused.
            assert!(unsafe { JS_SetPrototype(cx, object.handle().into_handle(), prototype.handle().into_handle()) });
            rooted!(in(cx) let replacement = unsafe { JS_NewPlainObject(cx) });
            assert!(!unsafe { JS_SetPrototype(cx, object.handle().into_handle(), replacement.handle().into_handle()) });
            rooted!(in(cx) let mut current = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_GetPrototype(cx, object.handle().into_handle(), current.handle_mut().into()) });
            assert_eq!(current.get(), prototype.get());
        }
        collect_fully(&mut runtime);
        assert_eq!(HOOK_FINALIZED.with(Cell::get), before + 2,
            "an unrooted object and its immutable prototype must be collected together");
    }

    #[test]
    fn mutable_prototypes_stay_replaceable() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let plain = unsafe { JS_NewPlainObject(cx) });
        rooted!(in(cx) let thing = unsafe { JS_NewObject(cx, &CLASS) });
        rooted!(in(cx) let replacement = unsafe { JS_NewPlainObject(cx) });
        for object in [plain.get(), thing.get()] {
            rooted!(in(cx) let object = object);
            assert!(unsafe { JS_SetPrototype(cx, object.handle().into_handle(), replacement.handle().into_handle()) });
            collect_fully(&mut runtime);
            rooted!(in(cx) let mut current = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_GetPrototype(cx, object.handle().into_handle(), current.handle_mut().into()) });
            assert_eq!(current.get(), replacement.get());
            rooted!(in(cx) let null = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_SetPrototype(cx, object.handle().into_handle(), null.handle().into_handle()) });
        }
        assert!(eval(&runtime, "const o = {}; Object.setPrototypeOf(o, Array.prototype); Array.isArray(o) === false && o instanceof Array").to_boolean());
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

mod atoms {
    use super::{eval, finalized};
    use crate::gc::Heap;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{IdVector, Runtime};
    use v8::cppgc::{GarbageCollected, Persistent, Visitor};

    /// A string id's code units, read back through the cell's characters.
    fn units(id: jsid) -> Vec<u16> {
        assert!(id.is_string());
        let mut length = 0;
        let latin1 = crate::api::latin1_chars(id.to_string(), &mut length);
        if !latin1.is_null() {
            return unsafe { std::slice::from_raw_parts(latin1, length) }.iter().map(|byte| *byte as u16).collect();
        }
        let two_byte = crate::api::two_byte_chars(id.to_string(), &mut length);
        unsafe { std::slice::from_raw_parts(two_byte, length) }.to_vec()
    }

    /// The id of the string `source` evaluates to (atomized like a property key).
    fn string_id(runtime: &Runtime, source: &str, id: crate::gc::MutableHandleId) {
        let string = eval(runtime, source).to_string();
        unsafe { RUST_INTERNED_STRING_TO_JSID(runtime.raw_cx(), string, id.into_handle()) };
    }

    /// The atom of Latin-1 `text` (`JS_AtomizeStringN`, not pinned).
    fn atomize(runtime: &Runtime, text: &[u8]) -> *mut JSString {
        unsafe { JS_AtomizeStringN(runtime.raw_cx(), text.as_ptr() as *const std::ffi::c_char, text.len()) }
    }

    fn production_gc(runtime: &Runtime) {
        unsafe { &mut *(*runtime.raw_cx()).isolate }.low_memory_notification();
    }

    fn atom_count(runtime: &Runtime) -> usize {
        unsafe { &(*runtime.raw_cx()).interned }.len()
    }

    #[test]
    fn enumerated_transient_atoms_are_collected_once_their_ids_are_gone() {
        const COUNT: usize = 64;
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let object = eval(&runtime, "(() => { const o = {}; for (let i = 0; i < 64; i++) o['transient' + i] = i; return o; })()").to_object());
        runtime.gc_for_testing();
        let before = finalized();
        {
            let mut keys = unsafe { IdVector::new(cx) };
            assert!(unsafe { GetPropertyKeys(cx, object.handle().into_handle(), JSITER_OWNONLY, keys.handle_mut()) });
            assert_eq!(keys.len(), COUNT);
            let first = keys[0];
            runtime.gc_for_testing();
            assert_eq!(finalized(), before, "the id vector keeps its atoms");
            assert_eq!(units(first), "transient0".encode_utf16().collect::<Vec<_>>());
            // Re-enumeration while they live finds the same atoms.
            let mut again = unsafe { IdVector::new(cx) };
            assert!(unsafe { GetPropertyKeys(cx, object.handle().into_handle(), JSITER_OWNONLY, again.handle_mut()) });
            assert!(keys.iter().zip(again.iter()).all(|(a, b)| a == b), "one atom per contents while it lives");
        }
        runtime.gc_for_testing();
        assert_eq!(finalized(), before + COUNT, "transient atoms are collected once no id references them");
        // Re-atomization after collection gives working ids.
        let mut keys = unsafe { IdVector::new(cx) };
        assert!(unsafe { GetPropertyKeys(cx, object.handle().into_handle(), JSITER_OWNONLY, keys.handle_mut()) });
        rooted!(in(cx) let id = keys[5]);
        rooted!(in(cx) let mut value = UndefinedValue());
        assert!(unsafe { JS_GetPropertyById(cx, object.handle().into_handle(), id.handle().into_handle(), value.handle_mut().into_handle()) });
        assert_eq!(value.get().to_int32(), 5);
    }

    #[test]
    fn live_atoms_keep_identity_and_contents_across_gc() {
        struct Owner {
            id: Heap<jsid>,
        }

        unsafe impl GarbageCollected for Owner {
            fn trace(&self, visitor: &mut Visitor) {
                self.id.trace_visitor(visitor);
            }

            fn get_name(&self) -> &'static std::ffi::CStr {
                c"AtomOwner"
            }
        }

        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let mut kept = crate::jsid::VoidId());
        string_id(&runtime, "'kept' + 'Atom'", kept.handle_mut());
        // A traced native owner, like a DOM object holding a `Heap<jsid>`.
        let owner = unsafe { &*cx }.with_scope(|scope| {
            let heap = scope.get_cpp_heap().unwrap();
            let owner = unsafe { v8::cppgc::make_garbage_collected(heap, Owner { id: Heap::default() }) };
            Persistent::new(&owner)
        });
        rooted!(in(cx) let mut owned = crate::jsid::VoidId());
        string_id(&runtime, "'owned\\u00e9'", owned.handle_mut());
        owner.get().unwrap().id.set(owned.get());
        let owned_pointer = owned.get();
        owned.set(crate::jsid::VoidId());
        // Collects the evaluated temporaries first, so the counts below are exact.
        runtime.gc_for_testing();
        let before = finalized();
        runtime.gc_for_testing();
        production_gc(&runtime);
        runtime.gc_for_testing();
        assert_eq!(finalized(), before, "rooted and owned atoms stay alive");
        assert_eq!(units(kept.get()), "keptAtom".encode_utf16().collect::<Vec<_>>());
        assert_eq!(units(owner.get().unwrap().id.get()), "owned\u{e9}".encode_utf16().collect::<Vec<_>>());
        // Every atomization path finds the live atom.
        assert_eq!(atomize(&runtime, b"keptAtom"), kept.get().to_string(), "JS_AtomizeStringN finds the rooted atom");
        rooted!(in(cx) let mut again = crate::jsid::VoidId());
        string_id(&runtime, "['kept', 'Atom'].join('')", again.handle_mut());
        assert_eq!(again.get(), kept.get(), "a key with the same contents is the same id");
        string_id(&runtime, "'owned\\u00e9'", again.handle_mut());
        assert_eq!(again.get(), owned_pointer, "the owner's atom is found again");
        again.set(crate::jsid::VoidId());
        runtime.gc_for_testing();
        let before = finalized();
        // Dropping the owner's id releases its atom, exactly once.
        owner.get().unwrap().id.set(crate::jsid::VoidId());
        runtime.gc_for_testing();
        assert_eq!(finalized(), before + 1, "the released atom is collected");
        string_id(&runtime, "'owned\\u00e9'", again.handle_mut());
        assert_eq!(units(again.get()), "owned\u{e9}".encode_utf16().collect::<Vec<_>>());
        drop(owner);
    }

    #[test]
    fn atoms_preserve_utf16_contents_including_lone_surrogates() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let mut high = crate::jsid::VoidId());
        rooted!(in(cx) let mut low = crate::jsid::VoidId());
        rooted!(in(cx) let mut pair = crate::jsid::VoidId());
        string_id(&runtime, "'\\uD800x'", high.handle_mut());
        string_id(&runtime, "'\\uDC00x'", low.handle_mut());
        string_id(&runtime, "'\\uD83D\\uDE00'", pair.handle_mut());
        runtime.gc_for_testing();
        assert_eq!(units(high.get()), [0xD800, 0x78]);
        assert_eq!(units(low.get()), [0xDC00, 0x78]);
        assert_eq!(units(pair.get()), [0xD83D, 0xDE00]);
        assert_ne!(high.get(), low.get(), "lone surrogates are not replaced with U+FFFD");
        rooted!(in(cx) let mut again = crate::jsid::VoidId());
        string_id(&runtime, "String.fromCharCode(0xD800) + 'x'", again.handle_mut());
        assert_eq!(again.get(), high.get());
        // Latin-1 contents atomize to the same atom whatever the source.
        string_id(&runtime, "'caf\\u00e9'", again.handle_mut());
        assert_eq!(atomize(&runtime, b"caf\xe9"), again.get().to_string());
        // Array-index strings stay int ids.
        string_id(&runtime, "'42'", again.handle_mut());
        assert!(again.get().is_int() && again.get().to_int() == 42);
    }

    /// The id `JS_StringToId` gives the string `source` evaluates to.
    fn string_to_id(runtime: &Runtime, source: &str, id: crate::gc::MutableHandleId) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let string = eval(runtime, source).to_string());
        assert!(unsafe { JS_StringToId(cx, string.handle().into_handle(), id.into_handle()) });
    }

    #[test]
    fn string_to_id_keeps_exact_units_without_pinning() {
        const TRANSIENT: usize = 16;
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let mut nul = crate::jsid::VoidId());
        rooted!(in(cx) let mut unicode = crate::jsid::VoidId());
        rooted!(in(cx) let mut astral = crate::jsid::VoidId());
        rooted!(in(cx) let mut lone = crate::jsid::VoidId());
        string_to_id(&runtime, "'a\\u0000b'", nul.handle_mut());
        string_to_id(&runtime, "'n\\u00e4me\\u4e2d'", unicode.handle_mut());
        string_to_id(&runtime, "'\\uD83D\\uDE00'", astral.handle_mut());
        string_to_id(&runtime, "'x\\uDC00'", lone.handle_mut());
        // Collects the evaluated temporaries first, so the count below is exact.
        runtime.gc_for_testing();
        let before = finalized();
        runtime.gc_for_testing();
        production_gc(&runtime);
        runtime.gc_for_testing();
        assert_eq!(finalized(), before, "rooted ids keep their atoms");
        assert_eq!(units(nul.get()), [0x61, 0x00, 0x62], "embedded NUL is kept");
        assert_eq!(units(unicode.get()), "n\u{e4}me\u{4e2d}".encode_utf16().collect::<Vec<_>>());
        assert_eq!(units(astral.get()), [0xD83D, 0xDE00]);
        assert_eq!(units(lone.get()), [0x78, 0xDC00], "lone surrogates are not replaced");
        // The same atom as the property-key path.
        rooted!(in(cx) let mut again = crate::jsid::VoidId());
        string_id(&runtime, "'a' + String.fromCharCode(0) + 'b'", again.handle_mut());
        assert_eq!(again.get(), nul.get());
        // Canonical array indices become int ids; other numeric-looking names stay strings.
        string_to_id(&runtime, "'42'", again.handle_mut());
        assert!(again.get().is_int() && again.get().to_int() == 42);
        for name in ["042", "-1", "2147483648", "1.5"] {
            string_to_id(&runtime, &format!("'{name}'"), again.handle_mut());
            assert_eq!(units(again.get()), name.encode_utf16().collect::<Vec<_>>());
        }
        // A null string fails without producing an id.
        again.set(crate::jsid::VoidId());
        rooted!(in(cx) let null = std::ptr::null_mut::<JSString>());
        assert!(!unsafe { JS_StringToId(cx, null.handle().into_handle(), again.handle_mut().into_handle()) });
        assert!(again.get().is_void());
        // Unlike JS_AtomizeAndPinString, the atoms are collected once no id references them.
        // The finalized count also includes the evaluated strings, so a control round without
        // ids measures those first.
        let collected = |runtime: &mut Runtime, prefix: &str, make_ids: bool| {
            let cx = runtime.raw_cx();
            runtime.gc_for_testing();
            let before = finalized();
            {
                rooted!(in(cx) let mut transient = crate::jsid::VoidId());
                for i in 0..TRANSIENT {
                    let source = format!("['{prefix}', {i}].join('\\u0000\\uD800')");
                    if make_ids {
                        string_to_id(runtime, &source, transient.handle_mut());
                    } else {
                        rooted!(in(cx) let _string = eval(runtime, &source).to_string());
                    }
                }
            }
            runtime.gc_for_testing();
            production_gc(runtime);
            runtime.gc_for_testing();
            finalized() - before
        };
        let control = collected(&mut runtime, "control", false);
        let with_ids = collected(&mut runtime, "transient", true);
        assert!(with_ids >= control + TRANSIENT, "unpinned atoms are collected ({with_ids} vs {control})");
        assert_eq!(units(nul.get()), [0x61, 0x00, 0x62]);
    }

    #[test]
    fn pinned_atoms_live_for_the_runtime() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let pinned = unsafe { JS_AtomizeAndPinString(cx, c"pinnedAtom".as_ptr()) };
        // An ordinary atom pinned later is pinned too.
        let later = {
            rooted!(in(cx) let ordinary = atomize(&runtime, b"laterPinned"));
            let later = unsafe { JS_AtomizeAndPinString(cx, c"laterPinned".as_ptr()) };
            assert_eq!(later, ordinary.get(), "pinning keeps the live atom");
            later
        };
        let before = finalized();
        runtime.gc_for_testing();
        production_gc(&runtime);
        runtime.gc_for_testing();
        assert_eq!(finalized(), before, "pinned atoms are not collected");
        assert_eq!(unsafe { JS_AtomizeAndPinString(cx, c"pinnedAtom".as_ptr()) }, pinned);
        assert_eq!(atomize(&runtime, b"pinnedAtom"), pinned);
        assert_eq!(atomize(&runtime, b"laterPinned"), later);
        rooted!(in(cx) let mut id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, pinned, id.handle_mut().into_handle()) };
        assert_eq!(units(id.get()), "pinnedAtom".encode_utf16().collect::<Vec<_>>());
    }

    #[test]
    fn the_atom_table_stays_bounded_under_transient_keys() {
        const ROUNDS: usize = 8;
        const KEYS: usize = 2048;
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        runtime.gc_for_testing();
        let before = finalized();
        for round in 0..ROUNDS {
            {
                rooted!(in(cx) let object = eval(&runtime, &format!("(() => {{ const o = {{}}; for (let i = 0; i < {KEYS}; i++) o['r{round}_' + i] = i; return o; }})()")).to_object());
                let mut keys = unsafe { IdVector::new(cx) };
                assert!(unsafe { GetPropertyKeys(cx, object.handle().into_handle(), JSITER_OWNONLY, keys.handle_mut()) });
                assert_eq!(keys.len(), KEYS);
            }
            runtime.gc_for_testing();
        }
        assert!(finalized() >= before + ROUNDS * KEYS, "every round's atoms are collected");
        // Dead entries are swept after a number of insertions proportional to the survivors.
        assert!(atom_count(&runtime) <= 4096 + KEYS, "the table holds {} entries", atom_count(&runtime));
    }
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
    fn classic_source_metadata_survives_execution_in_another_realm() {
        unsafe extern "C" fn read_private(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
            rooted!(in(cx) let mut private = UndefinedValue());
            unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
            unsafe { *vp = private.get() };
            true
        }
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let options = crate::rust::CompileOptionsWrapper::new(runtime.cx_no_gc(),
            std::ffi::CString::new("cross-realm-source.js").unwrap(), 1);
        let mut source = crate::rust::transform_str_to_source_text("globalThis.crossPrivate = function(){ return readPrivate() }");
        rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, global.get()) };
        eval(&runtime, "WeakMap.prototype.set = () => { throw 1 }; globalThis.WeakMap = () => { throw 2 }");
        unsafe { SetScriptPrivate(script.get(), &Int32Value(43)) };
        let reader = unsafe { JS_NewFunction(cx, Some(read_private), 0, 0, c"readPrivate".as_ptr()) };
        rooted!(in(cx) let reader = ObjectValue(reader.cast()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"readPrivate".as_ptr(), reader.handle().into()) });
        rooted!(in(cx) let mut result = UndefinedValue());
        assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
        assert_eq!(eval(&runtime, "crossPrivate()").to_int32(), 43);
        unsafe { LeaveRealm(cx, old) };
        runtime.gc_for_testing();
        unsafe { &mut *(*cx).isolate }.low_memory_notification();
        let old = unsafe { EnterRealm(cx, global.get()) };
        assert_eq!(eval(&runtime, "crossPrivate()").to_int32(), 43);
        unsafe { LeaveRealm(cx, old) };
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

        let mut immutable = false;
        assert!(unsafe { JS_SetImmutablePrototype(cx, global.handle().into_handle(), &mut immutable) });
        assert!(immutable, "the WebIDL global class must retain its immutable prototype");
        rooted!(in(cx) let mut replacement_proto = unsafe { JS_NewPlainObject(cx) });
        assert!(!unsafe { JS_SetPrototype(cx, global.handle().into_handle(), replacement_proto.handle().into_handle()) });

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
        /// `own_property_keys` reports index 0 twice (an invalid native handler result).
        static DUPLICATE_INDEX: Cell<bool> = const { Cell::new(false) };
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
        if DUPLICATE_INDEX.with(Cell::get) {
            id.set(crate::jsid::IntId(0));
            unsafe { AppendToIdVector(props, id.handle().into_handle()) };
            unsafe { AppendToIdVector(props, id.handle().into_handle()) };
        }
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
            let mut immutable = false;
            assert!(unsafe { JS_SetImmutablePrototype(cx, proxy.handle().into_handle(), &mut immutable) });
            assert!(immutable, "proxy class boxes are reached through the JSAPI cell");
            rooted!(in(cx) let mut replacement_proto = unsafe { JS_NewPlainObject(cx) });
            assert!(!unsafe { JS_SetPrototype(cx, proxy.handle().into_handle(), replacement_proto.handle().into_handle()) });
            rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
            rooted!(in(cx) let value = ObjectValue(proxy.get()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"immutableProxy".as_ptr(), value.handle().into_handle()) });
            assert!(eval(&runtime, "Reflect.setPrototypeOf(immutableProxy, Object.getPrototypeOf(immutableProxy))").to_boolean());
            assert!(!eval(&runtime, "Reflect.setPrototypeOf(immutableProxy, {})").to_boolean());
            assert!(unsafe { IsProxyHandlerFamily(proxy.get()) });
            rooted!(in(cx) let mut prototype = ptr::null_mut::<JSObject>());
            assert!(unsafe { JS_GetPrototype(cx, proxy.handle().into_handle(), prototype.handle_mut().into_handle()) });
            assert_eq!(prototype.get(), proto.get(), "a proxy's prototype comes from its getPrototypeOf trap");
            assert_eq!(unsafe { GetProxyHandler(proxy.get()) }, handler);
            assert_eq!(unsafe { GetProxyHandlerExtra(proxy.get()) }, &EXTRA as *const u8 as *const std::ffi::c_void);
            assert!(unsafe { crate::object::object_class(proxy.get()).as_ref() }.unwrap().flags & crate::object::JSCLASS_IS_PROXY != 0);

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
            eval(&runtime, "delete globalThis.immutableProxy");
        }
        runtime.gc_for_testing();
        runtime.gc_for_testing();
        assert_eq!(PROXY_FINALIZED.with(Cell::get), finalized_before + 1, "the finalize trap runs once with the slots");
    }

    /// The ownKeys trap forwards native keys unchanged: duplicates from a handler (e.g. an
    /// index also listed as a named property) stay a visible TypeError, not a silent dedup.
    #[test]
    fn native_own_keys_duplicates_raise_type_error() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps = traps();
        let handler = unsafe { CreateProxyHandler(&traps, &EXTRA as *const u8 as *const std::ffi::c_void) };
        rooted!(in(cx) let proto = eval(&runtime, "({})").to_object());
        rooted!(in(cx) let private = UndefinedValue());
        rooted!(in(cx) let proxy = unsafe { NewProxyObject(cx, handler, private.handle().into_handle(), proto.get(), ptr::null(), false) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(proxy.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"dup".as_ptr(), value.handle().into_handle()) });
        assert_eq!(describe(&runtime, eval(&runtime, "Reflect.ownKeys(dup).join()")), "foo", "unique keys are listed");
        DUPLICATE_INDEX.with(|flag| flag.set(true));
        let duplicate = eval(&runtime, "(() => { try { Reflect.ownKeys(dup); return 'no error' } catch (e) { return e instanceof TypeError ? 'TypeError' : String(e) } })()");
        DUPLICATE_INDEX.with(|flag| flag.set(false));
        assert_eq!(describe(&runtime, duplicate), "TypeError", "duplicate native keys are a proxy invariant violation");
        assert_eq!(describe(&runtime, eval(&runtime, "Reflect.ownKeys(dup).join()")), "foo");
        eval(&runtime, "delete globalThis.dup");
    }

    #[test]
    fn partial_descriptors_through_dom_proxies_keep_absent_fields() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps = traps();
        let handler = unsafe { CreateProxyHandler(&traps, &EXTRA as *const u8 as *const std::ffi::c_void) };
        rooted!(in(cx) let proto = eval(&runtime, "({})").to_object());
        rooted!(in(cx) let private = UndefinedValue());
        rooted!(in(cx) let proxy = unsafe { NewProxyObject(cx, handler, private.handle().into_handle(), proto.get(), ptr::null(), false) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(proxy.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"q".as_ptr(), value.handle().into_handle()) });
        // Each check goes through V8's proxy trap, the native defineProperty trap and
        // `JS_DefinePropertyById` on the expando, as Servo's DOM proxies do.
        let checks = r#"(() => {
          const failed = [];
          const check = (name, ok) => { if (!ok) failed.push(name); };
          const own = key => Object.getOwnPropertyDescriptor(q, key);
          const g1 = () => 1, g2 = () => 2, s1 = function (v) {}, s2 = function (v) {};
          // Absent fields must not come from a polluted Object.prototype or a replaced Reflect,
          // so each definition runs polluted with a null-prototype descriptor of its own.
          const reflectDefine = Reflect.defineProperty, defineProperty = Object.defineProperty;
          const polluted = (operation, key, descriptor) => {
            const own = Object.assign(Object.create(null), descriptor);
            Object.prototype.get = g2; Object.prototype.set = s2; Object.prototype.value = 9; Object.prototype.writable = true;
            Reflect.defineProperty = () => { throw new Error("replaced Reflect.defineProperty used"); };
            try {
              return operation(q, key, own);
            } finally {
              delete Object.prototype.get; delete Object.prototype.set; delete Object.prototype.value; delete Object.prototype.writable;
              Reflect.defineProperty = reflectDefine;
            }
          };
          const def = (key, descriptor) => polluted(defineProperty, key, descriptor);
          const reflect = (key, descriptor) => polluted(reflectDefine, key, descriptor);
          const attempt = (name, steps) => { try { steps(); } catch (e) { failed.push(name + ": " + e); } };
          attempt("writable-only", () => {
            def("w", { value: 1, writable: true, enumerable: true, configurable: true });
            def("w", { writable: false });
            const d = own("w");
            check("writable-only", d.value === 1 && d.writable === false && d.enumerable === true && d.configurable === true);
          });
          attempt("value-only", () => {
            def("v", { value: 1, writable: false, enumerable: false, configurable: true });
            def("v", { value: 2 });
            const d = own("v");
            check("value-only", d.value === 2 && d.writable === false && d.enumerable === false && d.configurable === true);
          });
          attempt("accessors", () => {
            def("a", { get: g1, set: s1, enumerable: true, configurable: true });
            def("a", { get: g2 });
            let d = own("a");
            check("getter-only", d.get === g2 && d.set === s1 && d.enumerable === true && d.configurable === true);
            def("a", { set: s2 });
            d = own("a");
            check("setter-only", d.get === g2 && d.set === s2);
            def("a", { set: undefined });
            d = own("a");
            check("explicit undefined setter", d.get === g2 && d.set === undefined && q.a === 2);
            def("a", { get: undefined, set: s1 });
            d = own("a");
            check("explicit undefined getter", d.get === undefined && d.set === s1);
            def("n", { get: g1 });
            d = own("n");
            check("new getter-only", d.get === g1 && d.set === undefined && d.enumerable === false && d.configurable === false);
          });
          // A partial update that makes an accessor non-configurable mirrors it whole.
          attempt("non-configurable mirror", () => {
            def("m", { get: g1, set: s1, configurable: true });
            def("m", { get: g1, configurable: false });
            const d = own("m");
            check("non-configurable mirror", d.get === g1 && d.set === s1 && d.configurable === false);
          });
          // Non-configurable properties: allowed and rejected partial changes.
          attempt("non-configurable data", () => {
            def("f", { value: 1, writable: true, configurable: false });
            check("non-configurable writable-only allowed", reflect("f", { writable: false }) === true && own("f").writable === false);
            check("non-configurable writable-only rejected", reflect("f", { writable: true }) === false && own("f").writable === false);
            let threw = false;
            try { def("f", { writable: true }); } catch (e) { threw = e instanceof TypeError; }
            check("Object.defineProperty throws on rejection", threw);
          });
          attempt("non-configurable accessor", () => {
            check("non-configurable getter-only rejected", reflect("m", { get: g2 }) === false && own("m").get === g1);
            check("non-configurable setter-only same", reflect("m", { set: s1 }) === true && own("m").set === s1);
          });
          attempt("keys", () => {
            const sym = Symbol("k");
            def(sym, { value: 1, writable: true, configurable: true });
            def(sym, { writable: false });
            let d = own(sym);
            check("symbol writable-only", d.value === 1 && d.writable === false);
            def(0, { get: g1, set: s1, configurable: true });
            def(0, { set: s2 });
            d = own(0);
            check("integer setter-only", d.get === g1 && d.set === s2);
          });
          return failed.join();
        })()"#;
        assert_eq!(describe(&runtime, eval(&runtime, checks)), "");

        // Direct JSAPI calls: rejection is an `ObjectOpResult`, an exception stays pending.
        let key = unsafe { JS_AtomizeAndPinString(cx, c"k".as_ptr()) };
        rooted!(in(cx) let mut id = crate::jsid::VoidId());
        unsafe { RUST_INTERNED_STRING_TO_JSID(cx, key, id.handle_mut().into_handle()) };
        let mut writable_only = PropertyDescriptor::default();
        writable_only.set_hasWritable_(true);
        writable_only.set_writable_(false);
        rooted!(in(cx) let descriptor = writable_only);
        let define = |target: *mut JSObject, result: &mut ObjectOpResult| {
            rooted!(in(cx) let target = target);
            unsafe { JS_DefinePropertyById(cx, target.handle().into_handle(), id.handle().into_handle(), descriptor.handle().into_handle(), result) }
        };
        let mut result = ObjectOpResult { code_: 0 };
        assert!(define(eval(&runtime, "Object.preventExtensions({})").to_object(), &mut result));
        assert!(!result.ok(), "a non-extensible target rejects a new key");
        let mut result = ObjectOpResult { code_: 0 };
        rooted!(in(cx) let existing = eval(&runtime, "globalThis.existing = Object.preventExtensions({ k: 1 })").to_object());
        assert!(define(existing.get(), &mut result));
        assert!(result.ok());
        assert!(eval(&runtime, "const d = Object.getOwnPropertyDescriptor(existing, 'k'); d.value === 1 && !d.writable && d.enumerable && d.configurable").to_boolean());
        let mut result = ObjectOpResult { code_: 0 };
        eval(&runtime, "globalThis.k = 1");
        assert!(define(global.get(), &mut result));
        assert!(result.ok() && eval(&runtime, "const g = Object.getOwnPropertyDescriptor(globalThis, 'k'); g.value === 1 && !g.writable").to_boolean(), "a global target");
        let mut result = ObjectOpResult { code_: 0 };
        assert!(define(eval(&runtime, "new Proxy({}, { defineProperty() { return false; } })").to_object(), &mut result));
        assert!(!result.ok(), "a false trap result is a rejection");
        let mut result = ObjectOpResult { code_: 0 };
        assert!(!define(eval(&runtime, "new Proxy({}, { defineProperty() { throw new RangeError('trap'); } })").to_object(), &mut result));
        assert!(unsafe { JS_IsExceptionPending(cx) }, "a throwing trap leaves its exception pending");
        unsafe { JS_ClearPendingException(cx) };
        eval(&runtime, "delete globalThis.q; delete globalThis.existing; delete globalThis.k");
    }

    #[test]
    fn dom_proxy_descriptor_results_ignore_polluted_object_prototype() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps = traps();
        let handler = unsafe { CreateProxyHandler(&traps, &EXTRA as *const u8 as *const std::ffi::c_void) };
        rooted!(in(cx) let proto = eval(&runtime, "({})").to_object());
        rooted!(in(cx) let private = UndefinedValue());
        rooted!(in(cx) let proxy = unsafe { NewProxyObject(cx, handler, private.handle().into_handle(), proto.get(), ptr::null(), false) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(proxy.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"r".as_ptr(), value.handle().into_handle()) });
        // V8 runs ToPropertyDescriptor on the native getOwnPropertyDescriptor trap's result,
        // which observes inherited fields. The engine-made result must not see accessors on a
        // polluted Object.prototype; descriptors returned to script keep Object.prototype.
        let checks = r#"(() => { try { return (() => {
          const failed = [];
          const check = (name, ok) => { if (!ok) failed.push(name); };
          const g1 = () => 1, s1 = function (v) {}, sym = Symbol("k");
          Object.defineProperty(r, "d", { value: 1, writable: true, enumerable: true, configurable: true });
          Object.defineProperty(r, "a", { get: g1, set: s1, enumerable: false, configurable: true });
          Object.defineProperty(r, "u", { get: undefined, set: s1, configurable: true });
          Object.defineProperty(r, sym, { value: 2, writable: false, configurable: true });
          Object.defineProperty(r, 0, { get: g1, configurable: true });
          Object.defineProperty(r, "nc", { value: 5, writable: false, enumerable: true, configurable: false });
          const keys = ["foo", "d", "a", "u", sym, "0", "nc"];
          const calls = [];
          const poison = { value: 666, writable: true, enumerable: true, configurable: true, get: () => 666, set: function (v) {} };
          const results = new Map();
          let listed, failure = "";
          try {
            // Null-prototype descriptors: user descriptors do observe inherited fields.
            for (const name of Object.keys(poison)) {
              Object.defineProperty(Object.prototype, name, {
                __proto__: null,
                get() { calls.push("get " + name); return poison[name]; },
                set(v) { calls.push("set " + name); },
                configurable: true,
              });
            }
            for (const key of keys) results.set(key, Object.getOwnPropertyDescriptor(r, key));
            listed = Object.keys(r).join();
          } catch (e) {
            failure = String(e);
          } finally {
            for (const name of Object.keys(poison)) delete Object.prototype[name];
          }
          check("threw: " + failure, failure === "");
          check("Object.prototype accessors invoked: " + calls.join(), calls.length === 0);
          if (failure) return failed.join();
          const shape = (key, names) => {
            const d = results.get(key);
            return d !== undefined && Object.getPrototypeOf(d) === Object.prototype && Object.getOwnPropertyNames(d).join() === names;
          };
          const data = "value,writable,enumerable,configurable", accessor = "get,set,enumerable,configurable";
          const d = k => results.get(k);
          check("named trap property", shape("foo", data) && d("foo").value === "named-foo" && !d("foo").writable && d("foo").enumerable && d("foo").configurable);
          check("data", shape("d", data) && d("d").value === 1 && d("d").writable && d("d").enumerable && d("d").configurable);
          check("accessor", shape("a", accessor) && d("a").get === g1 && d("a").set === s1 && !d("a").enumerable && d("a").configurable);
          check("explicit undefined getter", shape("u", accessor) && d("u").get === undefined && d("u").set === s1);
          check("symbol", shape(sym, data) && d(sym).value === 2 && !d(sym).writable && !d(sym).enumerable);
          check("index getter-only", shape("0", accessor) && d("0").get === g1 && d("0").set === undefined);
          check("non-configurable", shape("nc", data) && d("nc").value === 5 && !d("nc").writable && !d("nc").configurable);
          check("keys: " + listed, listed === "foo,d,nc");
          // The non-configurable mirror is unchanged by the reads.
          check("mirror", Reflect.defineProperty(r, "nc", { value: 5 }) && !Reflect.defineProperty(r, "nc", { value: 6 }));
          return failed.join();
        })(); } catch (e) { return "uncaught: " + e; } })()"#;
        assert_eq!(describe(&runtime, eval(&runtime, checks)), "");
        // A user Proxy's own trap result still goes through ordinary ToPropertyDescriptor.
        let user = r#"(() => {
          const p = new Proxy({}, { getOwnPropertyDescriptor: () => ({ value: 1, configurable: true }) });
          Object.prototype.writable = true;
          try { return Object.getOwnPropertyDescriptor(p, "x").writable === true; } finally { delete Object.prototype.writable; }
        })()"#;
        assert!(eval(&runtime, user).to_boolean(), "user trap results keep inherited fields");
        eval(&runtime, "delete globalThis.r");
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
        static ALL_RESOLVE_CALLS: Cell<u32> = const { Cell::new(0) };
        static LAST_RESOLVE_REALM: Cell<*mut Realm> = const { Cell::new(ptr::null_mut()) };
        static FAIL_RESOLVE_ONCE: Cell<bool> = const { Cell::new(false) };
        static ENUMERATE_PHANTOM: Cell<bool> = const { Cell::new(false) };
        static DEFINE_ALIAS: Cell<bool> = const { Cell::new(false) };
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
        ALL_RESOLVE_CALLS.with(|calls| calls.set(calls.get() + 1));
        LAST_RESOLVE_REALM.set(unsafe { GetCurrentRealmOrNull(cx) });
        if is_phantom(id.get()) {
            // Reports success without defining anything (Servo does for disabled interfaces).
            unsafe { *resolved = true };
            return true;
        }
        if !is_lazy_name(id.get()) {
            unsafe { *resolved = false };
            return true;
        }
        if FAIL_RESOLVE_ONCE.replace(false) {
            crate::rooted!(in(cx) let error = Int32Value(13));
            unsafe { JS_SetPendingException(cx, error.handle(), ExceptionStackBehavior::DoNotCapture) };
            return false;
        }
        RESOLVE_CALLS.with(|calls| calls.set(calls.get() + 1));
        // A lookup of the same property from inside the hook does not recurse.
        crate::rooted!(in(cx) let mut existing = UndefinedValue());
        if !unsafe { JS_GetPropertyById(cx, obj, id, existing.handle_mut().into_handle()) } {
            return false;
        }
        assert!(existing.get().is_undefined() || (existing.get().is_int32() && existing.get().to_int32() == 99),
            "recursive resolution may read the inherited test value");
        crate::rooted!(in(cx) let value = Int32Value(42));
        if !unsafe { JS_DefinePropertyById2(cx, obj, id, value.handle().into_handle(), JSPROP_RESOLVING as u32) } {
            return false;
        }
        if DEFINE_ALIAS.with(Cell::get) && !unsafe { JS_DefineProperty(cx, obj, c"AliasThing".as_ptr(), value.handle().into_handle(), JSPROP_RESOLVING as u32) } {
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
        if !unsafe { AppendToIdVector(props, id.handle().into_handle()) } { return false }
        if DEFINE_ALIAS.with(Cell::get) {
            let alias = unsafe { JS_AtomizeAndPinString(cx, c"AliasThing".as_ptr()) };
            unsafe { RUST_INTERNED_STRING_TO_JSID(cx, alias, id.handle_mut().into_handle()) };
            if !unsafe { AppendToIdVector(props, id.handle().into_handle()) } { return false }
        }
        if ENUMERATE_PHANTOM.with(Cell::get) {
            let phantom = unsafe { JS_AtomizeAndPinString(cx, c"Phantom".as_ptr()) };
            unsafe { RUST_INTERNED_STRING_TO_JSID(cx, phantom, id.handle_mut().into_handle()) };
            return unsafe { AppendToIdVector(props, id.handle().into_handle()) };
        }
        true
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

    static DOM_CLASS: JSClass = JSClass {
        name: c"NativeLazyGlobal".as_ptr(),
        flags: CLASS.flags | crate::object::JSCLASS_USERBIT1,
        ..CLASS
    };

    #[test]
    fn dom_globals_use_native_lazy_properties_without_intrinsic_interceptors() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &DOM_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, global.get()) };
        let before = RESOLVE_CALLS.with(Cell::get);
        assert!(eval(&runtime, "!Object.hasOwn(globalThis, 'LazyThing')").to_boolean());
        assert!(unsafe { JS_InitGlobalInterfaceProperties(cx, global.handle().into_handle()) });
        assert_eq!(RESOLVE_CALLS.with(Cell::get), before, "registration must not create interfaces");
        assert!(eval(&runtime, "Object.getOwnPropertyNames(globalThis).includes('LazyThing')").to_boolean());
        assert_eq!(RESOLVE_CALLS.with(Cell::get), before, "enumeration must stay lazy");
        eval(&runtime, "Object.setPrototypeOf(Object.getPrototypeOf(globalThis), new Proxy({LazyThing: 99}, {}))");
        assert_eq!(eval(&runtime, "LazyThing").to_int32(), 42);
        assert_eq!(eval(&runtime, "LazyThing + LazyThing").to_int32(), 84);
        assert_eq!(RESOLVE_CALLS.with(Cell::get), before + 1);
        assert!(eval(&runtime, "const d = Object.getOwnPropertyDescriptor(globalThis, 'LazyThing'); d.value === 42 && d.writable && d.configurable && !d.enumerable").to_boolean());
        assert!(eval(&runtime, "Math.imul(3, 4) === 12 && Array.isArray([]) && typeof Phantom === 'undefined'").to_boolean());
        unsafe { LeaveRealm(cx, old) };
    }

    #[test]
    fn native_lazy_globals_preserve_assignment_deletion_and_gc() {
        let mut runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        for action in ["assign", "delete", "child"] {
            rooted!(in(cx) let global = unsafe {
                JS_NewGlobalObject(cx, &DOM_CLASS, ptr::null_mut(), OnNewGlobalHookOption::FireOnNewGlobalHook, ptr::null())
            });
            assert!(unsafe { JS_InitGlobalInterfaceProperties(cx, global.handle().into_handle()) });
            let old = unsafe { EnterRealm(cx, global.get()) };
            let before = RESOLVE_CALLS.with(Cell::get);
            runtime.gc_for_testing();
            match action {
                "assign" => assert!(eval(&runtime, "'use strict'; globalThis.LazyThing = 77; LazyThing === 77").to_boolean()),
                "delete" => assert!(eval(&runtime, "delete globalThis.LazyThing; typeof LazyThing === 'undefined' && !Object.hasOwn(globalThis, 'LazyThing')").to_boolean()),
                _ => assert!(eval(&runtime, "const child = Object.create(globalThis); child.LazyThing = 5; child.LazyThing === 5 && Object.hasOwn(child, 'LazyThing')").to_boolean()),
            }
            assert_eq!(RESOLVE_CALLS.with(Cell::get), before, "{action} must not instantiate interfaces");
            if action == "child" {
                assert_eq!(eval(&runtime, "LazyThing").to_int32(), 42);
                assert_eq!(RESOLVE_CALLS.with(Cell::get), before + 1);
            }
            unsafe { LeaveRealm(cx, old) };
        }
    }

    #[test]
    fn native_lazy_globals_retry_errors_and_discard_disabled_interfaces() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        ENUMERATE_PHANTOM.set(true);
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &DOM_CLASS, ptr::null_mut(), OnNewGlobalHookOption::FireOnNewGlobalHook, ptr::null())
        });
        ENUMERATE_PHANTOM.set(false);
        assert!(unsafe { JS_InitGlobalInterfaceProperties(cx, global.handle().into_handle()) });
        let old = unsafe { EnterRealm(cx, global.get()) };
        FAIL_RESOLVE_ONCE.set(true);
        assert!(eval(&runtime, "(() => { try { LazyThing; return false } catch(e) { return e === 13 } })()").to_boolean());
        assert_eq!(eval(&runtime, "LazyThing").to_int32(), 42, "failed resolution must remain retryable");
        assert!(eval(&runtime, "typeof Phantom === 'undefined' && !Object.hasOwn(globalThis, 'Phantom')").to_boolean());
        unsafe { LeaveRealm(cx, old) };
    }

    #[test]
    fn native_global_aliases_do_not_trigger_nested_initialization() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        DEFINE_ALIAS.set(true);
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &DOM_CLASS, ptr::null_mut(), OnNewGlobalHookOption::FireOnNewGlobalHook, ptr::null())
        });
        assert!(unsafe { JS_InitGlobalInterfaceProperties(cx, global.handle().into_handle()) });
        let old = unsafe { EnterRealm(cx, global.get()) };
        let before = RESOLVE_CALLS.with(Cell::get);
        let all_before = ALL_RESOLVE_CALLS.with(Cell::get);
        assert_eq!(eval(&runtime, "LazyThing + AliasThing").to_int32(), 84);
        assert_eq!(RESOLVE_CALLS.with(Cell::get), before + 1);
        assert_eq!(ALL_RESOLVE_CALLS.with(Cell::get), all_before + 1, "defining an alias must not invoke its placeholder");
        DEFINE_ALIAS.set(false);
        unsafe { LeaveRealm(cx, old) };
    }

    #[test]
    fn native_global_resolution_uses_the_owners_realm() {
        let runtime = Runtime::new(crate::rust::JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let caller_realm = unsafe { GetCurrentRealmOrNull(cx) };
        rooted!(in(cx) let owner = unsafe {
            JS_NewGlobalObject(cx, &DOM_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        assert!(unsafe { JS_InitGlobalInterfaceProperties(cx, owner.handle().into_handle()) });
        let owner_realm = unsafe { GetObjectRealmOrNull(owner.get()) };
        rooted!(in(cx) let caller = unsafe { CurrentGlobalOrNull(cx) });
        rooted!(in(cx) let value = ObjectValue(owner.get()));
        assert!(unsafe { JS_SetProperty(cx, caller.handle().into_handle(), c"otherGlobal".as_ptr(), value.handle().into_handle()) });
        FAIL_RESOLVE_ONCE.set(true);
        assert!(eval(&runtime, "(() => { try { otherGlobal.LazyThing; return false } catch(e) { return e === 13 } })()").to_boolean());
        assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, caller_realm);
        assert_eq!(eval(&runtime, "otherGlobal.LazyThing").to_int32(), 42);
        assert_eq!(LAST_RESOLVE_REALM.get(), owner_realm);
        assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, caller_realm);
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
        static CSP_SAW_TRUSTED_EVAL_OBJECT: Cell<bool> = const { Cell::new(false) };
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
        body_argument: Handle<JSVal>,
        allowed: *mut bool,
    ) -> bool {
        // SAFETY: the checker's arguments are valid handles.
        let (code, body) = unsafe { (text(*code.ptr), text(*body.ptr)) };
        if body_argument.get().is_object() {
            CSP_SAW_TRUSTED_EVAL_OBJECT.with(|saw| saw.set(true));
        }
        let parameters = unsafe { &*(parameters.ptr as *const Vec<*mut JSString>) };
        let parameters: Vec<std::string::String> = parameters.iter().map(|parameter| text(*parameter)).collect();
        CSP_CHECKS.with(|checks| checks.borrow_mut().push(format!("{kind:?} {compilation_type:?} {body} [{}]", parameters.join(","))));
        // SAFETY: a valid out pointer.
        unsafe { *allowed = !code.contains("blocked") };
        true
    }

    unsafe extern "C" fn code_for_eval_gets(
        _cx: *mut JSContext,
        code: HandleObject,
        out: crate::jsapi::MutableHandleString,
    ) -> bool {
        let is_trusted = JSContext::current().with_scope(|scope| {
            // SAFETY: V8 passed the live eval operand.
            let value = unsafe { crate::cell::cell_value(scope, code.get() as *mut c_void) };
            let Ok(object) = v8::Local::<v8::Object>::try_from(value) else { return false };
            let Some(key) = v8::String::new(scope, "__rovesTrustedScript") else { return false };
            object.get(scope, key.into()).is_some_and(|value| value.is_true())
        });
        if is_trusted {
            JSContext::current().with_scope(|scope| {
                let source = v8::String::new(scope, "40 + 2").unwrap();
                let value = crate::jsval::from_v8(scope, source.into());
                out.set(value.to_string());
            });
        }
        true
    }

    static SECURITY_CALLBACKS: JSSecurityCallbacks =
        JSSecurityCallbacks { contentSecurityPolicyAllows: Some(csp_check), codeForEvalGets: Some(code_for_eval_gets), subsumes: None };

    static TRUSTED_CLASS: JSClass = JSClass {
        name: c"TrustedScriptTest".as_ptr(),
        flags: 1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT,
        cOps: std::ptr::null(), spec: std::ptr::null(), ext: std::ptr::null(), oOps: std::ptr::null(),
    };

    unsafe extern "C" fn trusted_to_string(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        let receiver = unsafe { *vp.add(1) };
        assert!(receiver.is_object());
        assert_eq!(unsafe { crate::rust::get_object_class(receiver.to_object()) }, &TRUSTED_CLASS as *const _);
        unsafe { JS_GetReservedSlot(receiver.to_object(), 0, vp) };
        true
    }

    unsafe extern "C" fn make_trusted(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        let source = unsafe { *vp.add(2) };
        assert!(source.is_string());
        rooted!(in(cx) let object = unsafe { JS_NewObject(cx, &TRUSTED_CLASS) });
        unsafe { JS_SetReservedSlot(object.get(), 0, &source) };
        let stringify = unsafe { JS_NewFunction(cx, Some(trusted_to_string), 0, 0, c"toString".as_ptr()) };
        rooted!(in(cx) let stringify = ObjectValue(stringify as *mut JSObject));
        assert!(unsafe { JS_SetProperty(cx, object.handle().into(), c"toString".as_ptr(), stringify.handle().into()) });
        unsafe { *vp = ObjectValue(object.get()) };
        true
    }

    unsafe extern "C" fn trusted_function_csp(
        cx: *mut JSContext, _kind: RuntimeCode, _code: HandleString, compilation: CompilationType,
        parameters: Handle<StackGCVector<*mut JSString>>, body: HandleString,
        arguments: Handle<StackGCVector<JSVal>>, body_argument: HandleValue, allowed: *mut bool,
    ) -> bool {
        let parameters = unsafe { &*(parameters.ptr as *const Vec<*mut JSString>) };
        let arguments = unsafe { &*(arguments.ptr as *const Vec<JSVal>) };
        let matches = |argument: JSVal, string: *mut JSString| {
            if !argument.is_object() || unsafe { crate::rust::get_object_class(argument.to_object()) } != &TRUSTED_CLASS as *const _ { return false; }
            rooted!(in(cx) let mut data = UndefinedValue());
            unsafe { JS_GetReservedSlot(argument.to_object(), 0, &mut *data) };
            let raw = unsafe { &*cx };
            raw.with_scope(|scope| {
                let expected = unsafe { to_v8(scope, data.get()) };
                let actual = unsafe { to_v8(scope, StringValue(&*string)) };
                expected.strict_equals(actual)
            })
        };
        unsafe { *allowed = compilation == CompilationType::Function && parameters.len() == arguments.len() &&
            parameters.iter().zip(arguments).all(|(string, argument)| matches(*argument, *string)) && matches(body_argument.get(), body.get()) };
        true
    }

    unsafe extern "C" fn trusted_argument_source(_cx: *mut JSContext, object: HandleObject, source: MutableHandleString) -> bool {
        if unsafe { crate::rust::get_object_class(object.get()) } == &TRUSTED_CLASS as *const _ {
            let mut data = UndefinedValue();
            unsafe { JS_GetReservedSlot(object.get(), 0, &mut data) };
            source.set(data.to_string());
        }
        true
    }

    #[test]
    fn function_csp_preserves_brands_conversions_and_constructor_semantics() {
        static SECURITY: JSSecurityCallbacks = JSSecurityCallbacks {
            contentSecurityPolicyAllows: Some(trusted_function_csp), codeForEvalGets: Some(trusted_argument_source), subsumes: None,
        };
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        let trust = unsafe { JS_NewFunction(cx, Some(make_trusted), 1, 0, c"trust".as_ptr()) };
        rooted!(in(cx) let trust = ObjectValue(trust as *mut JSObject));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"trust".as_ptr(), trust.handle().into()) });
        unsafe { JS_SetSecurityCallbacks(cx, &SECURITY) };
        assert_eq!(eval(&runtime, "new Function(trust('value'), trust('return value + 1'))(41)").to_int32(), 42);
        assert!(eval(&runtime, "(() => { const p = trust('value'), b = trust('return value + 1'); let calls = 0; p.toString = () => { calls++; return 'wrongInput'; }; b.toString = () => { calls++; return 'return 0'; }; return Function(p, b)(41) === 42 && calls === 0; })()").to_boolean(), "trusted data bypasses forged ToString methods");
        for source in [
            "Function('value', trust('return value + 1'))",
            "Function(trust('value'), 'return value + 1')",
            "Function({toString() { return 'value'; }}, trust('return value + 1'))",
            "eval('(function anonymous(\\n) {\\nreturn 42\\n})')",
        ] {
            let source = format!("(() => {{ try {{ {source}; return false; }} catch (e) {{ return e instanceof EvalError; }} }})()");
            assert!(eval(&runtime, &source).to_boolean(), "untrusted source rejected: {source}");
        }
        assert!(eval(&runtime, "(() => { let order = '', blocked = false, denied = false; const p = {toString() { order += 'p'; try { eval('42'); } catch(e) { blocked = e instanceof EvalError; } return 'value'; }}, b = {toString() { order += 'b'; return 'return value + 1'; }}; try { Function(p,b); } catch(e) { denied = e instanceof EvalError; } return order === 'pb' && blocked && denied; })()").to_boolean(), "ordinary conversions run once, in order, without a CSP bypass");
        assert!(eval(&runtime, "(() => { class Derived extends Function {} const f = new Derived(trust('return 42')); return f() === 42 && f instanceof Derived && Function.prototype.constructor === Function; })()").to_boolean());
        assert!(eval(&runtime, r#"Function(trust('return "\uD800"'))().charCodeAt(0) === 0xD800"#).to_boolean(), "source stays UTF-16 without a lossy Rust conversion");
        assert!(eval(&runtime, r#"(() => {
            const constructors = [Function, (function*(){}).constructor,
                (async function(){}).constructor, (async function*(){}).constructor];
            for (const Constructor of constructors) {
                const probe = Constructor(trust('return 42'));
                const exactSource = '(' + Function.prototype.toString.call(probe) + ')';
                let blocked = false;
                const newTarget = new Proxy(function(){}, {
                    get(target, key) {
                        if (key === 'prototype') {
                            try { eval(exactSource); } catch (error) { blocked = error instanceof EvalError; }
                        }
                        return Reflect.get(target, key);
                    }
                });
                const compiled = Reflect.construct(Constructor, [trust('return 42')], newTarget);
                if (typeof compiled !== 'function' || !blocked) return false;
            }
            return true;
        })()"#).to_boolean(), "exact-source authorization cannot escape into a newTarget prototype getter");
        assert_eq!(super::describe(&runtime, eval(&runtime, r#"(() => {
            const constructors = [Function, (function*(){}).constructor,
                (async function(){}).constructor, (async function*(){}).constructor];
            const calls = [(C, ...args) => C(...args), (C, ...args) => new C(...args)];
            const rejected = make => { try { make(); return false; } catch (e) { return e instanceof EvalError; } };
            for (let kind = 0; kind < 4; kind++) {
                const C = constructors[kind];
                for (let how = 0; how < 2; how++) {
                    const make = (...args) => calls[how](C, ...args);
                    const where = kind + '/' + how;
                    if (typeof make(trust('x'), trust('return x')) !== 'function') return 'trusted ' + where;
                    if (!rejected(() => make('return 1'))) return 'untrusted body ' + where;
                    if (!rejected(() => make('x', trust('return x')))) return 'untrusted parameter ' + where;
                    if (!rejected(() => make(trust('x'), 'return x'))) return 'untrusted body with trusted parameter ' + where;
                }
            }
            return 'ok';
        })()"#)), "ok", "every dynamic constructor, called or constructed, checks every argument");
        assert!(eval(&runtime, "(() => { const G = Object.getPrototypeOf(function*(){}).constructor; return G(trust('yield 42'))().next().value === 42; })()").to_boolean());
        assert!(eval(&runtime, "(() => { const A = Object.getPrototypeOf(async function(){}).constructor; const AG = Object.getPrototypeOf(async function*(){}).constructor; return A(trust('return 42'))() instanceof Promise && AG(trust('yield 42'))().next() instanceof Promise; })()").to_boolean());
        assert!(eval(&runtime, "(() => { try { Function(Symbol()); return false; } catch(e) { return e instanceof TypeError; } })()").to_boolean());
        assert!(eval(&runtime, "(() => { globalThis.escapedFunctionBody = false; try { Function(trust('a) { globalThis.escapedFunctionBody = true; } ('), trust('return 42')); return false; } catch(e) { return e instanceof SyntaxError && !escapedFunctionBody; } })()").to_boolean(), "V8 still validates parameters separately from the body");
    }

    /// The four dynamic-code constructors and their prototypes, as Chrome links them
    /// (ECMA-262 20.2.2, 27.3.2, 27.4.2, 27.7.2): only the wrapped `Function` is reachable.
    const CONSTRUCTOR_GRAPH: &str = r#"(() => {
        const prototypes = [Function.prototype, Object.getPrototypeOf(function*(){}),
            Object.getPrototypeOf(async function(){}), Object.getPrototypeOf(async function*(){})];
        const constructors = prototypes.map(prototype => prototype.constructor);
        const names = ['Function', 'GeneratorFunction', 'AsyncFunction', 'AsyncGeneratorFunction'];
        const instances = [function(){}, function*(){}, async function(){}, async function*(){}];
        if (constructors[0] !== Function) return 'global Function';
        for (let kind = 0; kind < 4; kind++) {
            const C = constructors[kind], prototype = prototypes[kind];
            const name = Object.getOwnPropertyDescriptor(C, 'name'), length = Object.getOwnPropertyDescriptor(C, 'length');
            // Object.freeze only clears these properties' [[Configurable]].
            const configurable = !Object.isFrozen(C);
            if (name.value !== names[kind] || name.writable || name.enumerable || name.configurable !== configurable) return 'name ' + kind;
            if (length.value !== 1 || length.writable || length.enumerable || length.configurable !== configurable) return 'length ' + kind;
            const own = Object.getOwnPropertyDescriptor(C, 'prototype');
            if (own.value !== prototype || own.writable || own.enumerable || own.configurable) return 'prototype ' + kind;
            const back = Object.getOwnPropertyDescriptor(prototype, 'constructor');
            if (back.value !== C || back.enumerable || !back.configurable || back.writable !== (kind === 0)) return 'constructor ' + kind;
            if (instances[kind].constructor !== C || !(instances[kind] instanceof C)) return 'instance ' + kind;
            if (Object.getPrototypeOf(C) !== (kind === 0 ? Function.prototype : Function)) return 'parent ' + kind;
            if (kind !== 0 && Object.getPrototypeOf(prototype) !== Function.prototype) return 'prototype parent ' + kind;
            if (typeof C !== 'function' || !(C instanceof Function)) return 'instanceof ' + kind;
            // V8's own constructors print their name; a Proxy wrapper printed none.
            if (Function.prototype.toString.call(C) !== `function ${names[kind]}() { [native code] }`) return 'source ' + kind;
        }
        return 'ok';
    })()"#;

    #[test]
    fn dynamic_constructors_keep_the_intrinsic_graph() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        assert_eq!(super::describe(&runtime, eval(&runtime, CONSTRUCTOR_GRAPH)), "ok");
        assert!(eval(&runtime, r#"(() => {
            const [F, G, A, AG] = [Function, (function*(){}).constructor, (async function(){}).constructor, (async function*(){}).constructor];
            if (F('return 42')() !== 42 || new F('a', 'return a')(7) !== 7) return false;
            if (G('yield 42')().next().value !== 42 || Object.getPrototypeOf(G('')) !== G.prototype) return false;
            if (!(Reflect.construct(A, ['return 1'])() instanceof Promise)) return false;
            if (!(Reflect.construct(AG, ['yield 1'])().next() instanceof Promise)) return false;
            for (const C of [F, G, A, AG]) {
                class Derived extends C {}
                const derived = new Derived('return 1');
                const constructed = Reflect.construct(C, ['return 1'], Derived);
                if (Object.getPrototypeOf(Derived) !== C || !(derived instanceof Derived) || !(derived instanceof C)) return false;
                if (Object.getPrototypeOf(constructed) !== Derived.prototype) return false;
            }
            return true;
        })()"#).to_boolean(), "calls, construction and subclassing go through the wrappers");
        assert!(eval(&runtime, r#"(() => {
            const A = (async function(){}).constructor, other = function(){};
            if (Object.setPrototypeOf(A, other) !== A || Object.getPrototypeOf(A) !== other) return false;
            if (!Reflect.setPrototypeOf(A, null) || Object.getPrototypeOf(A) !== null) return false;
            Object.setPrototypeOf(A, Function);
            return Object.getPrototypeOf(A) === Function;
        })()"#).to_boolean(), "scripts may still relink the constructors");
        assert_eq!(super::describe(&runtime, eval(&runtime, CONSTRUCTOR_GRAPH)), "ok");
    }

    #[test]
    fn dynamic_constructors_keep_the_graph_when_frozen() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        assert!(eval(&runtime, r#"(() => {
            const constructors = [Function, (function*(){}).constructor, (async function(){}).constructor, (async function*(){}).constructor];
            constructors.forEach(Object.preventExtensions);
            Object.freeze(constructors[1]);
            if (constructors.some(Object.isExtensible) || !Object.isFrozen(constructors[1])) return false;
            if (Object.setPrototypeOf(constructors[2], Function) !== constructors[2]) return false;
            if (Reflect.setPrototypeOf(constructors[2], {})) return false;
            try { Object.setPrototypeOf(constructors[3], null); return false; } catch (error) { if (!(error instanceof TypeError)) return false; }
            return Function('return 42')() === 42 && constructors[1]('yield 1')().next().value === 1;
        })()"#).to_boolean(), "non-extensible constructors keep their parents and still work");
        assert_eq!(super::describe(&runtime, eval(&runtime, CONSTRUCTOR_GRAPH)), "ok");
    }

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
        // V8 has no WebAssembly without its JIT: then only finalization is checked.
        let wasm = cfg!(not(feature = "jitless"));
        // Without a dispatch callback, `RunJobs` runs the tasks.
        if wasm {
            eval(&runtime, compile);
            assert!(wait_for(&runtime, "done === 1", || unsafe { RunJobs(cx) }), "WebAssembly.compile resolves");
        }
        // With one, the tasks reach the embedder's event loop and run in `DispatchableRun`.
        unsafe { SetUpEventLoopDispatch(cx, Some(dispatch), 11 as *mut c_void) };
        let run_dispatched = || {
            let tasks = std::mem::take(&mut *DISPATCHED.lock().unwrap());
            for task in tasks {
                unsafe { DispatchableRun(cx, task as *mut _, Dispatchable_MaybeShuttingDown::NotShuttingDown) };
            }
            unsafe { RunJobs(cx) };
        };
        if wasm {
            eval(&runtime, compile);
            assert!(wait_for(&runtime, "done === 1", run_dispatched), "WebAssembly.compile resolves through dispatch");
        }
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
    #[cfg_attr(feature = "jitless", ignore = "V8 has no WebAssembly without its JIT")]
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
        assert_eq!(eval(&runtime, "eval({__rovesTrustedScript: true})").to_int32(), 42);
        assert!(CSP_SAW_TRUSTED_EVAL_OBJECT.with(Cell::get), "the original TrustedScript operand reaches CSP");
        // V8 has no WebAssembly without its JIT.
        let wasm = "new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])) instanceof WebAssembly.Module";
        if cfg!(not(feature = "jitless")) {
            assert!(eval(&runtime, wasm).to_boolean());
        }
        let checks = CSP_CHECKS.with(|checks| checks.take());
        assert_eq!(checks[0], "JS DirectEval 2 + 2 []");
        assert_eq!(checks[1], "JS DirectEval \"blocked\" []");
        assert_eq!(checks[2], "JS Function return a + b [a,b]");
        assert_eq!(checks[3], "JS Function return \"blocked\" []");
        assert_eq!(checks[4], "JS DirectEval 40 + 2 []");
        if cfg!(not(feature = "jitless")) {
            assert_eq!(checks.last().map(std::string::String::as_str), Some("WASM DirectEval null []"));
        }
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
    fn clear_kept_objects_releases_weak_ref_targets_without_promise_jobs() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // Creating or dereferencing a WeakRef keeps its target until ClearKeptObjects. No
        // promise job runs here, so V8's own clear (after promise jobs) never happens.
        eval(&runtime, "globalThis.held = {}; globalThis.strong = new WeakRef(held); globalThis.weak = new WeakRef({}); 0");
        runtime.gc_for_testing();
        assert!(eval(&runtime, "weak.deref() !== undefined").to_boolean(), "a kept target survives until a clear");
        runtime.gc_for_testing();
        assert!(eval(&runtime, "weak.deref() !== undefined").to_boolean(), "without a clear it stays kept");

        // The script crate's path (the HTML microtask checkpoint's step 6).
        let safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        unsafe { crate::rust::wrappers2::ClearKeptObjects(&safe) };
        runtime.gc_for_testing();
        assert!(eval(&runtime, "weak.deref() === undefined").to_boolean(), "a released, unreachable target is collected");
        assert!(eval(&runtime, "strong.deref() === held").to_boolean(), "a target held elsewhere survives");

        // A target dereferenced after the clear is kept again until the next one.
        eval(&runtime, "globalThis.again = new WeakRef(held); delete globalThis.held; again.deref(); 0");
        runtime.gc_for_testing();
        assert!(eval(&runtime, "again.deref() !== undefined").to_boolean(), "kept again until the next clear");
        unsafe { ClearKeptObjects(cx) };
        runtime.gc_for_testing();
        assert!(eval(&runtime, "again.deref() === undefined && strong.deref() === undefined").to_boolean());
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

    /// A native reached from a running script must still expose its scripted caller.
    unsafe extern "C" fn check_scripted_caller(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        let global = unsafe { GetScriptedCallerGlobal(cx) };
        assert!(!global.is_null());
        unsafe { *vp = BooleanValue(true) };
        true
    }

    #[test]
    fn incumbent_caller_requires_a_scripted_frame() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        assert!(unsafe { GetScriptedCallerGlobal(cx) }.is_null());
        rooted!(in(cx) let mut private = UndefinedValue());
        unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
        assert!(private.get().is_undefined());
        let mut buffer = [0; 32];
        let (mut line, mut column) = (0, 0);
        assert!(!unsafe { DescribeScriptedCaller(cx, buffer.as_mut_ptr(), buffer.len(), &mut line, &mut column) });
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        let native = unsafe { JS_NewFunction(cx, Some(check_scripted_caller), 0, 0, c"checkScriptedCaller".as_ptr()) };
        rooted!(in(cx) let callback = ObjectValue(native as *mut JSObject));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"checkScriptedCaller".as_ptr(), callback.handle().into_handle()) });
        assert!(eval(&runtime, "checkScriptedCaller()").to_boolean());
        assert!(unsafe { GetScriptedCallerGlobal(cx) }.is_null());
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
    fn native_errors_without_script_locations_report_unknown_column() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // V8 errors created outside script have no source column (-1).
        let value = unsafe { &*cx }.with_scope(|scope| {
            let text = v8::String::new(scope, "native error without a script").unwrap();
            let error = v8::Exception::error(scope, text);
            assert!(v8::Exception::create_message(scope, error).get_start_column() >= u32::MAX as usize);
            from_v8(scope, error)
        });
        rooted!(in(cx) let error = value.to_object());
        let report = unsafe { JS_ErrorFromException(cx, error.handle().into_handle()) };
        assert!(!report.is_null());
        assert_eq!(unsafe { (*report)._base.column._base }, 0);
    }

    #[test]
    fn pending_classic_import_retains_source_metadata_only_until_resolution() {
        use std::rc::Rc;
        thread_local! {
            static PENDING: RefCell<Option<(v8::Global<v8::Object>, v8::Global<v8::Object>)>> = const { RefCell::new(None) };
        }
        unsafe extern "C" fn retain(value: *const JSVal) {
            unsafe { Rc::increment_strong_count((*value).to_private() as *const i32) };
        }
        unsafe extern "C" fn release(value: *const JSVal) {
            unsafe { Rc::decrement_strong_count((*value).to_private() as *const i32) };
        }
        unsafe extern "C" fn queue_import(
            cx: *mut JSContext, _private: Handle<JSVal>, request: Handle<*mut JSObject>, promise: Handle<*mut JSObject>,
        ) -> bool {
            let pending = unsafe { &*cx }.with_scope(|scope| {
                let request = unsafe { to_v8(scope, ObjectValue(request.get())) }.to_object(scope).unwrap();
                let promise = unsafe { to_v8(scope, ObjectValue(promise.get())) }.to_object(scope).unwrap();
                (v8::Global::new(scope, request), v8::Global::new(scope, promise))
            });
            PENDING.with(|pending_slot| *pending_slot.borrow_mut() = Some(pending));
            true
        }
        let owner = Rc::new(42);
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        unsafe {
            SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release));
            SetModuleDynamicImportHook(runtime.rt(), Some(queue_import));
        }
        {
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("pending-source.js").unwrap(), 1);
            let mut source = transform_str_to_source_text("import('deferred.js')");
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
            unsafe { SetScriptPrivate(script.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            rooted!(in(cx) let mut result = UndefinedValue());
            assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
        }
        let (request, promise) = PENDING.with(|pending| pending.borrow_mut().take()).unwrap();
        fn collect(runtime: &mut Runtime) {
            runtime.gc_for_testing();
            for _ in 0..3 {
                unsafe { &mut *(*runtime.raw_cx()).isolate }.low_memory_notification();
                runtime.gc_for_testing();
            }
        }
        collect(&mut runtime);
        assert_eq!(Rc::strong_count(&owner), 2, "the host's pending import retains source metadata");
        let (request_object, promise_object) = unsafe { &*cx }.with_scope(|scope| {
            let request = v8::Local::new(scope, &request);
            let promise = v8::Local::new(scope, &promise);
            (from_v8(scope, request.into()).to_object(), from_v8(scope, promise.into()).to_object())
        });
        rooted!(in(cx) let request_object = request_object);
        rooted!(in(cx) let promise_object = promise_object);
        rooted!(in(cx) let no_evaluation = std::ptr::null_mut::<JSObject>());
        rooted!(in(cx) let private = PrivateValue(Rc::as_ptr(&owner).cast()));
        assert!(unsafe { FinishDynamicModuleImport(cx, no_evaluation.handle().into(), private.handle().into(),
            request_object.handle().into(), promise_object.handle().into()) });
        collect(&mut runtime);
        assert_eq!(Rc::strong_count(&owner), 1, "even a retained settled promise releases the source token");
    }

    #[test]
    fn classic_private_js_cycles_are_traced_and_collect_with_the_source() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let weak = {
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("private-cycle.js").unwrap(), 1);
            let mut source = transform_str_to_source_text("globalThis.cyclicFunction = function(){ return 42 }");
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
            rooted!(in(cx) let mut result = UndefinedValue());
            assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
            let weak = {
                rooted!(in(cx) let value = eval(&runtime, "({ callback: cyclicFunction })"));
                unsafe { SetScriptPrivate(script.get(), &value.get()) };
                unsafe { &*cx }.with_scope(|scope| {
                    let value = unsafe { to_v8(scope, value.get()) };
                    v8::Weak::new(scope, v8::Local::<v8::Object>::try_from(value).unwrap())
                })
            };
            eval(&runtime, "delete globalThis.cyclicFunction");
            runtime.gc_for_testing();
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
            assert!(!weak.is_empty(), "the rooted source retains its JS private value");
            rooted!(in(cx) let mut private = UndefinedValue());
            unsafe { JS_GetScriptPrivate(script.get(), private.handle_mut().into()) };
            assert!(private.get().is_object());
            rooted!(in(cx) let object = private.get().to_object());
            rooted!(in(cx) let mut callback = UndefinedValue());
            assert!(unsafe { JS_GetProperty(cx, object.handle().into(), c"callback".as_ptr(), callback.handle_mut().into()) });
            assert!(callback.get().is_object());
            weak
        };
        runtime.gc_for_testing();
        for _ in 0..3 {
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
            runtime.gc_for_testing();
        }
        assert!(weak.is_empty(), "an unrooted source/private/function cycle must not become a runtime root");
    }

    #[test]
    fn classic_private_metadata_follows_functions_and_releases_dead_sources() {
        use std::rc::Rc;
        thread_local! {
            static RELEASE_DURING_GC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        unsafe extern "C" fn retain(value: *const JSVal) {
            unsafe { Rc::increment_strong_count((*value).to_private() as *const i32) };
        }
        unsafe extern "C" fn release(value: *const JSVal) {
            if unsafe { RuntimeHeapState() } == HeapState::MajorCollecting {
                RELEASE_DURING_GC.with(|flag| flag.set(true));
            }
            unsafe { Rc::decrement_strong_count((*value).to_private() as *const i32) };
        }
        unsafe extern "C" fn read_private(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
            rooted!(in(cx) let mut private = UndefinedValue());
            unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
            let number = if private.get().is_undefined() { -1 } else {
                unsafe { *(private.get().to_private() as *const i32) }
            };
            unsafe { *vp = Int32Value(number) };
            true
        }
        let owners = [Rc::new(11), Rc::new(22)];
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        unsafe { SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release)) };
        {
            rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
            let reader = unsafe { JS_NewFunction(cx, Some(read_private), 0, 0, c"readPrivate".as_ptr()) };
            rooted!(in(cx) let reader = ObjectValue(reader.cast()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"readPrivate".as_ptr(), reader.handle().into()) });
        }
        eval(&runtime, "globalThis.keptFunctions = []; WeakMap.prototype.set = () => { throw 1 }; globalThis.WeakMap = () => { throw 2 }");
        // Identical code/name must still preserve each compilation's own source metadata.
        for owner in &owners {
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("shared-name.js").unwrap(), 1);
            let mut source = transform_str_to_source_text("keptFunctions.push(function(){ return readPrivate() })");
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
            assert!(!script.get().is_null());
            unsafe { SetScriptPrivate(script.get(), &PrivateValue(Rc::as_ptr(owner).cast())) };
            rooted!(in(cx) let mut result = UndefinedValue());
            assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
        }
        fn collect(runtime: &mut Runtime) {
            runtime.gc_for_testing();
            for _ in 0..3 {
                unsafe { &mut *(*runtime.raw_cx()).isolate }.low_memory_notification();
                runtime.gc_for_testing();
            }
        }
        collect(&mut runtime);
        assert_eq!(describe(&runtime, eval(&runtime, "keptFunctions.map(f => f()).join(',')")), "11,22");
        assert_eq!(Rc::strong_count(&owners[0]), 2);
        assert_eq!(Rc::strong_count(&owners[1]), 2);
        eval(&runtime, "keptFunctions.shift(); 0");
        collect(&mut runtime);
        assert_eq!(Rc::strong_count(&owners[0]), 1, "dead source releases its private before shutdown");
        assert!(RELEASE_DURING_GC.with(std::cell::Cell::get));
        assert_eq!(Rc::strong_count(&owners[1]), 2, "the remaining function retains its own source metadata");
        assert_eq!(eval(&runtime, "keptFunctions[0]()").to_int32(), 22);
        eval(&runtime, "delete globalThis.keptFunctions");
        collect(&mut runtime);
        assert_eq!(Rc::strong_count(&owners[1]), 1);
        drop(runtime);
        assert_eq!(Rc::strong_count(&owners[0]), 1, "no duplicate release at shutdown");
    }

    #[test]
    fn script_and_module_private_references_balance_replacement_and_shutdown() {
        use std::cell::Cell;
        use std::rc::Rc;
        thread_local! {
            static RETAINS: Cell<usize> = const { Cell::new(0) };
            static RELEASES: Cell<usize> = const { Cell::new(0) };
            static RELEASE_WITHOUT_RUNTIME: Cell<bool> = const { Cell::new(false) };
        }
        unsafe extern "C" fn retain(value: *const JSVal) {
            unsafe { Rc::increment_strong_count((*value).to_private() as *const ()) };
            RETAINS.with(|count| count.set(count.get() + 1));
        }
        unsafe extern "C" fn release(value: *const JSVal) {
            if Runtime::get().is_none() { RELEASE_WITHOUT_RUNTIME.with(|flag| flag.set(true)); }
            unsafe { Rc::decrement_strong_count((*value).to_private() as *const ()) };
            RELEASES.with(|count| count.set(count.get() + 1));
        }
        let owner = Rc::new(());
        let weak = Rc::downgrade(&owner);
        {
            let runtime = Runtime::new(JSEngineHandle::for_tests());
            let cx = runtime.raw_cx();
            unsafe { SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release)) };
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("private-owner.js").unwrap(), 1);
            let mut classic_source = transform_str_to_source_text("42");
            let mut module_source = transform_str_to_source_text("export const value = 42");
            let mut json_source = transform_str_to_source_text("42");
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut classic_source) });
            rooted!(in(cx) let module = unsafe { CompileModule1(cx, options.ptr, &mut module_source) });
            rooted!(in(cx) let json = unsafe { CompileJsonModule1(cx, options.ptr, &mut json_source) });
            assert!(!script.get().is_null() && !module.get().is_null() && !json.get().is_null());
            let private = PrivateValue(Rc::as_ptr(&owner).cast());
            unsafe {
                SetScriptPrivate(script.get(), &private);
                SetScriptPrivate(script.get(), &private);
                SetScriptPrivate(script.get(), &UndefinedValue());
                SetScriptPrivate(script.get(), &private);
                SetModulePrivate(module.get(), &private);
                SetModulePrivate(json.get(), &private);
                SetModulePrivate(module.get(), &private);
                SetModulePrivate(module.get(), &UndefinedValue());
                SetModulePrivate(module.get(), &private);
            }
            assert_eq!(Rc::strong_count(&owner), 4, "caller plus three engine-owned references");
            for record in [module.get(), json.get()] {
                rooted!(in(cx) let mut read = UndefinedValue());
                unsafe { crate::glue::JS_GetModulePrivate(record, read.handle_mut().into()) };
                assert_eq!(read.get().to_private(), private.to_private(), "source and JSON module privates");
            }
            // Ensure retain happens before release even when the engine is the only owner.
            drop(owner);
            let pointer = weak.as_ptr();
            assert_eq!(weak.strong_count(), 3);
            unsafe { SetModulePrivate(module.get(), &PrivateValue(pointer.cast())) };
            assert_eq!(weak.strong_count(), 3);
        }
        assert!(weak.upgrade().is_none(), "shutdown releases the remaining classic/source/JSON owners");
        assert_eq!(RETAINS.with(Cell::get), RELEASES.with(Cell::get));
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
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

        // A proxy in the chain, as every DOM proxy reflector (a Document) is in an element
        // handler's chain: V8's context extensions return no function for it without an
        // exception. Lookups go through the proxy's traps, the innermost scope wins, unscopables
        // hide a name, and syntax errors keep failing with a pending exception.
        let chain = EnvironmentChain::new(cx, SupportUnscopables::Yes);
        rooted!(in(cx) let outer_object = eval(&runtime, "new Proxy({ fromProxy: 3, shared: 100, hidden: 'outer' }, {})").to_object());
        rooted!(in(cx) let inner_object = eval(&runtime, "({ shared: 10, hidden: 'inner', [Symbol.unscopables]: { hidden: true } })").to_object());
        chain.append(outer_object.get());
        chain.append(inner_object.get());
        let body: Vec<u16> = "if (hidden !== 'outer') return -1;\nreturn event + fromProxy + shared;".encode_utf16().collect();
        let mut body_source = transform_u16_to_source_text(&body);
        let function = unsafe { CompileFunction(cx, chain.get(), options.ptr, c"onclick".as_ptr(), 1, &argument, &mut body_source) };
        assert!(!function.is_null());
        rooted!(in(cx) let function_value = ObjectValue(function as *mut JSObject));
        assert!(unsafe { Call(cx, this.handle().into_handle(), function_value.handle().into_handle(), &array, call_result.handle_mut().into_handle()) });
        assert_eq!(call_result.get().to_int32(), 15);
        let invalid: Vec<u16> = "}{".encode_utf16().collect();
        let mut invalid_source = transform_u16_to_source_text(&invalid);
        assert!(unsafe { CompileFunction(cx, chain.get(), options.ptr, c"onclick".as_ptr(), 1, &argument, &mut invalid_source) }.is_null());
        assert!(unsafe { crate::api::JS_IsExceptionPending(cx) });
        unsafe { crate::api::JS_ClearPendingException(cx) };

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

    type Outcome = std::result::Result<std::string::String, (std::string::String, std::string::String, u32, u32)>;

    /// Takes the pending exception: its message, filename, line and column.
    fn pending_location(runtime: &mut Runtime) -> (std::string::String, std::string::String, u32, u32) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let mut exception = UndefinedValue());
        let info = crate::rust::error_info_from_exception_stack_safe(runtime.cx(), exception.handle_mut()).unwrap();
        (info.message, info.filename, info.line, info.col)
    }

    /// Compiles `body` as a one-argument `onclick` handler scoped by `scopes` (none takes V8's
    /// own, unwrapped compilation); null with a pending exception when it does not compile.
    fn compile_handler(runtime: &Runtime, options: &CompileOptionsWrapper, scopes: &[*mut JSObject], body: &str) -> *mut JSFunction {
        let cx = runtime.raw_cx();
        let chain = EnvironmentChain::new(cx, SupportUnscopables::Yes);
        for object in scopes {
            chain.append(*object);
        }
        let units: Vec<u16> = body.encode_utf16().collect();
        let mut source = transform_u16_to_source_text(&units);
        let argument = c"event".as_ptr();
        unsafe { CompileFunction(cx, chain.get(), options.ptr, c"onclick".as_ptr(), 1, &argument, &mut source) }
    }

    /// Calls a handler with `2` as its event: its result, or its exception's location.
    fn call_handler(runtime: &mut Runtime, function: *mut JSFunction) -> Outcome {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let function_value = ObjectValue(function as *mut JSObject));
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let mut result = UndefinedValue());
        let arguments = [Int32Value(2)];
        let array = HandleValueArray { length_: 1, elements_: arguments.as_ptr() };
        if unsafe { Call(cx, this.handle().into_handle(), function_value.handle().into_handle(), &array, result.handle_mut().into_handle()) } {
            Ok(describe(runtime, result.get()))
        } else {
            Err(pending_location(runtime))
        }
    }

    fn run_handler(runtime: &mut Runtime, options: &CompileOptionsWrapper, scopes: &[*mut JSObject], body: &str) -> Outcome {
        let function = compile_handler(runtime, options, scopes, body);
        if function.is_null() { Err(pending_location(runtime)) } else { call_handler(runtime, function) }
    }

    /// The `with` wrapper of a scoped handler shares the body's first line with generated code:
    /// names, runtime error locations and stacks there and on later lines, syntax error
    /// locations, strictness and trailing comments must match V8's own compilation of the body.
    #[test]
    fn scoped_handlers_keep_body_names_locations_strictness_and_scopes() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        let whoami_fn = unsafe { JS_NewFunction(cx, Some(whoami), 0, 0, c"whoami".as_ptr()) };
        rooted!(in(cx) let whoami_value = ObjectValue(whoami_fn as *mut JSObject));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"whoami".as_ptr(), whoami_value.handle().into_handle()) });
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("https://example.test/handler.html").unwrap(), 7);
        rooted!(in(cx) let document = eval(&runtime, "new Proxy({ fromProxy: 3, shared: 100, hidden: 'outer' }, {})").to_object());
        rooted!(in(cx) let element = eval(&runtime, "({ shared: 10, hidden: 'inner', [Symbol.unscopables]: { hidden: true } })").to_object());
        let scopes = [document.get(), element.get()];

        for body in [
            "null.boom",
            "   null.boom; // first line",
            "event;\n  null.boom",
            "try { null.boom } catch (error) { return error.stack }",
            "return 1;\n  }{",
            "'use strict'; with ({}) {}",
        ] {
            let unscoped = run_handler(&mut runtime, &options, &[], body);
            assert!(unscoped.is_err() || body.starts_with("try"), "{body:?}: {unscoped:?}");
            assert_eq!(run_handler(&mut runtime, &options, &scopes, body), unscoped, "{body:?}");
        }

        // Absolute positions, so both paths cannot agree on a shifted location.
        let Err((message, filename, line, column)) = run_handler(&mut runtime, &options, &scopes, "   null.boom") else { panic!("null.boom threw") };
        assert!(message.starts_with("TypeError"), "{message}");
        assert_eq!((filename.as_str(), line), ("https://example.test/handler.html", 7));
        assert!((4..=12).contains(&column), "first-line column {column} is inside `null.boom`");
        let Err((_, _, line, column)) = run_handler(&mut runtime, &options, &scopes, "event;\n  null.boom") else { panic!("null.boom threw") };
        assert_eq!(line, 8);
        assert!((3..=11).contains(&column), "second-line column {column} is inside `null.boom`");
        let Err((message, _, line, _)) = run_handler(&mut runtime, &options, &scopes, "return 1;\n  }{") else { panic!("syntax error") };
        assert!(message.starts_with("SyntaxError"), "{message}");
        assert_eq!(line, 8);

        // The handler's own name and first-line location in a captured stack.
        SEEN.with(|seen| seen.borrow_mut().clear());
        assert_eq!(run_handler(&mut runtime, &options, &[], "  whoami();"), Ok("undefined".into()));
        assert_eq!(run_handler(&mut runtime, &options, &scopes, "  whoami();"), Ok("undefined".into()));
        let seen = SEEN.with(|seen| seen.borrow().clone());
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1], seen[0]);
        let frame = seen[1].split('|').nth(2).unwrap();
        let column: u32 = frame.strip_prefix("onclick@https://example.test/handler.html:7:").unwrap_or_else(|| panic!("{frame}")).parse().unwrap();
        assert!((3..=10).contains(&column), "{frame}");

        // Strict bodies stay strict (an undefined `this`) under the sloppy `with` wrapper,
        // sloppy ones see the global; scope order, unscopables and trailing comments hold.
        let strict = "'use strict'; return this === undefined ? fromProxy + shared : -1;";
        assert_eq!(run_handler(&mut runtime, &options, &scopes, strict), Ok("13".into()));
        assert_eq!(run_handler(&mut runtime, &options, &scopes, "return this === globalThis"), Ok("true".into()));
        assert_eq!(run_handler(&mut runtime, &options, &scopes, "return hidden + shared + event // trailing"), Ok("outer102".into()));
    }

    /// A scoped handler's closure keeps its scope objects once the environment chain is gone
    /// and a full production GC ran.
    #[test]
    fn scoped_handlers_keep_their_scopes_after_the_chain_and_a_full_gc() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("https://example.test/handler.html").unwrap(), 1);
        rooted!(in(cx) let mut handler = std::ptr::null_mut::<JSObject>());
        {
            rooted!(in(cx) let document = eval(&runtime, "new Proxy({ fromProxy: 3 }, {})").to_object());
            rooted!(in(cx) let element = eval(&runtime, "({ fromElement: 4 })").to_object());
            let function = compile_handler(&runtime, &options, &[document.get(), element.get()], "return event + fromProxy + fromElement;");
            assert!(!function.is_null());
            handler.set(function as *mut JSObject);
        }
        unsafe { JS_GC(cx, GCReason::API) };
        runtime.gc_for_testing();
        assert_eq!(call_handler(&mut runtime, handler.get() as *mut JSFunction), Ok("9".into()));
    }
}

/// Native privates whose JS edges only the embedder's trace hook reports, as Servo's
/// `ModuleScript` holds its owner global.
mod traced_privates {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::ptr;
    use std::rc::{Rc, Weak};

    use super::{describe, eval};
    use crate::gc::Heap;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{CompileOptionsWrapper, JSEngineHandle, Runtime, transform_str_to_source_text};

    struct NativeOwner {
        owner: Heap<*mut JSObject>,
        /// A module record the owner also traces, when set.
        record: Heap<*mut JSObject>,
    }

    thread_local! {
        /// Engine-owned references per private, as the reference hooks report them.
        static ENGINE_REFS: RefCell<HashMap<usize, isize>> = RefCell::new(HashMap::new());
        static RETAINS: Cell<usize> = const { Cell::new(0) };
        static RELEASES: Cell<usize> = const { Cell::new(0) };
        static TRACES: Cell<usize> = const { Cell::new(0) };
        static STALE_TRACE: Cell<bool> = const { Cell::new(false) };
        static RELEASE_WITHOUT_RUNTIME: Cell<bool> = const { Cell::new(false) };
        static OWNERS_FINALIZED: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    }

    fn reset() {
        ENGINE_REFS.with(|refs| refs.borrow_mut().clear());
        for counter in [&RETAINS, &RELEASES, &TRACES] {
            counter.with(|count| count.set(0));
        }
        STALE_TRACE.with(|flag| flag.set(false));
        RELEASE_WITHOUT_RUNTIME.with(|flag| flag.set(false));
        OWNERS_FINALIZED.with(|finalized| finalized.borrow_mut().clear());
    }

    fn count(counter: &'static std::thread::LocalKey<Cell<usize>>) -> usize {
        counter.with(Cell::get)
    }

    fn owners_finalized() -> Vec<i32> {
        OWNERS_FINALIZED.with(|finalized| finalized.borrow().clone())
    }

    fn native_owner(value: *const JSVal) -> *const NativeOwner {
        unsafe { (*value).to_private() as *const NativeOwner }
    }

    fn engine_refs(pointer: *const NativeOwner) -> isize {
        ENGINE_REFS.with(|refs| refs.borrow().get(&(pointer as usize)).copied().unwrap_or(0))
    }

    unsafe extern "C" fn retain(value: *const JSVal) {
        let pointer = native_owner(value);
        unsafe { Rc::increment_strong_count(pointer) };
        ENGINE_REFS.with(|refs| *refs.borrow_mut().entry(pointer as usize).or_default() += 1);
        RETAINS.with(|count| count.set(count.get() + 1));
    }

    unsafe extern "C" fn release(value: *const JSVal) {
        let pointer = native_owner(value);
        if Runtime::get().is_none() {
            RELEASE_WITHOUT_RUNTIME.with(|flag| flag.set(true));
        }
        ENGINE_REFS.with(|refs| *refs.borrow_mut().entry(pointer as usize).or_default() -= 1);
        RELEASES.with(|count| count.set(count.get() + 1));
        unsafe { Rc::decrement_strong_count(pointer) };
    }

    /// Reports the owner edge, as Servo's hook traces `ModuleScript` (no assertions: an
    /// unwinding extern callback aborts).
    unsafe extern "C" fn trace(trc: *mut JSTracer, value: *const JSVal) {
        let pointer = native_owner(value);
        TRACES.with(|count| count.set(count.get() + 1));
        if engine_refs(pointer) <= 0 {
            STALE_TRACE.with(|flag| flag.set(true));
            return;
        }
        unsafe { crate::glue::CallObjectTracer(trc, &(*pointer).owner as *const _ as *mut _, c"owner".as_ptr()) };
        if !unsafe { (*pointer).record.get() }.is_null() {
            unsafe { crate::glue::CallObjectTracer(trc, &(*pointer).record as *const _ as *mut _, c"record".as_ptr()) };
        }
    }

    unsafe extern "C" fn finalize(_gcx: *mut GCContext, obj: *mut JSObject) {
        let mut slot = UndefinedValue();
        // SAFETY: the finalize hook may read the reserved slots.
        unsafe { JS_GetReservedSlot(obj, 0, &mut slot) };
        if slot.is_int32() {
            OWNERS_FINALIZED.with(|finalized| finalized.borrow_mut().push(slot.to_int32()));
        }
    }

    static OPS: JSClassOps = JSClassOps {
        addProperty: None,
        delProperty: None,
        enumerate: None,
        newEnumerate: None,
        resolve: None,
        mayResolve: None,
        finalize: Some(finalize),
        call: None,
        construct: None,
        trace: None,
    };

    static CLASS: JSClass = JSClass {
        name: c"PrivateOwner".as_ptr(),
        flags: 1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT | crate::object::JSCLASS_FOREGROUND_FINALIZE,
        cOps: &OPS,
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    /// A native private whose owner object (id `id`) has a `marker` expando and, when given,
    /// a `callback` property set to the value of `callback`.
    fn new_owner(runtime: &Runtime, id: i32, callback: Option<&str>) -> Rc<NativeOwner> {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let object = unsafe { JS_NewObject(cx, &CLASS) });
        unsafe { JS_SetReservedSlot(object.get(), 0, &Int32Value(id)) };
        rooted!(in(cx) let marker = eval(runtime, &format!("'owner {id}'")));
        assert!(unsafe { JS_SetProperty(cx, object.handle().into(), c"marker".as_ptr(), marker.handle().into()) });
        if let Some(callback) = callback {
            rooted!(in(cx) let callback = eval(runtime, callback));
            assert!(unsafe { JS_SetProperty(cx, object.handle().into(), c"callback".as_ptr(), callback.handle().into()) });
        }
        let owner = Rc::new(NativeOwner { owner: Heap::default(), record: Heap::default() });
        owner.owner.set(object.get());
        owner
    }

    fn marker(runtime: &Runtime, owner: &Weak<NativeOwner>) -> std::string::String {
        let cx = runtime.raw_cx();
        let owner = owner.upgrade().expect("a live native owner");
        rooted!(in(cx) let object = owner.owner.get());
        rooted!(in(cx) let mut marker = UndefinedValue());
        assert!(unsafe { JS_GetProperty(cx, object.handle().into(), c"marker".as_ptr(), marker.handle_mut().into()) });
        describe(runtime, marker.get())
    }

    fn private_of(owner: Option<&Weak<NativeOwner>>) -> JSVal {
        owner.map_or(UndefinedValue(), |owner| PrivateValue(owner.as_ptr().cast()))
    }

    /// Full collections of both heaps.
    fn collect(runtime: &mut Runtime) {
        for _ in 0..3 {
            let isolate = unsafe { &mut *(*runtime.raw_cx()).isolate };
            isolate.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            isolate.low_memory_notification();
            runtime.gc_for_testing();
        }
    }

    /// A classic source whose native private owns an object holding a function of that
    /// source: a cycle through native code. Returns the private and whether its owner was
    /// finalized while the source was still reachable.
    fn classic_owner_cycle(runtime: &mut Runtime) -> (Weak<NativeOwner>, bool) {
        let cx = runtime.raw_cx();
        let weak = {
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("traced-owner.js").unwrap(), 1);
            let mut source = transform_str_to_source_text("globalThis.ownedFunction = function(){ return 7 }");
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
            rooted!(in(cx) let mut result = UndefinedValue());
            assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
            let owner = new_owner(runtime, 1, Some("ownedFunction"));
            unsafe { SetScriptPrivate(script.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            Rc::downgrade(&owner)
        };
        assert_eq!(weak.strong_count(), 1, "the engine owns the only reference");
        collect(runtime);
        assert_eq!(eval(runtime, "ownedFunction()").to_int32(), 7);
        (weak, owners_finalized().contains(&1))
    }

    #[test]
    fn traced_native_private_keeps_its_owner_and_collects_with_its_source() {
        reset();
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        unsafe {
            SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release));
            SetScriptPrivateTraceHook(runtime.rt(), Some(trace));
        }
        let (weak, finalized_while_reachable) = classic_owner_cycle(&mut runtime);
        assert!(!finalized_while_reachable, "the trace hook keeps the owner of a reachable source");
        assert!(count(&TRACES) > 0);
        assert_eq!(marker(&runtime, &weak), "owner 1", "the owner keeps its expando");
        eval(&runtime, "delete globalThis.ownedFunction");
        collect(&mut runtime);
        assert!(weak.upgrade().is_none(), "the dead source releases its private");
        assert_eq!(count(&RELEASES), 1);
        assert_eq!(owners_finalized(), vec![1], "the source/private/owner cycle collects");
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![1], "the owner is finalized once");
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1), "no release at shutdown after the GC one");
        assert!(!STALE_TRACE.with(Cell::get), "no trace after release");
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    #[test]
    fn untraced_native_private_edges_do_not_keep_their_owner() {
        reset();
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        unsafe { SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release)) };
        let (weak, finalized_while_reachable) = classic_owner_cycle(&mut runtime);
        assert!(finalized_while_reachable, "without a trace hook nothing reports the owner edge");
        assert!(weak.upgrade().is_some(), "the private itself is still owned by the source");
        assert_eq!(count(&TRACES), 0);
        eval(&runtime, "delete globalThis.ownedFunction");
        collect(&mut runtime);
        assert!(weak.upgrade().is_none());
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1));
    }

    #[test]
    fn module_and_json_privates_trace_only_their_current_owner() {
        reset();
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        unsafe {
            SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release));
            SetScriptPrivateTraceHook(runtime.rt(), Some(trace));
        }
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("traced-module.js").unwrap(), 1);
        let mut module_source = transform_str_to_source_text("export const value = 42");
        let mut json_source = transform_str_to_source_text("42");
        rooted!(in(cx) let module = unsafe { CompileModule1(cx, options.ptr, &mut module_source) });
        rooted!(in(cx) let json = unsafe { CompileJsonModule1(cx, options.ptr, &mut json_source) });
        assert!(!module.get().is_null() && !json.get().is_null());
        let set = |record: *mut JSObject, owner: Option<&Weak<NativeOwner>>| unsafe { SetModulePrivate(record, &private_of(owner)) };
        let read = |record: *mut JSObject| {
            rooted!(in(cx) let mut read = UndefinedValue());
            unsafe { crate::glue::JS_GetModulePrivate(record, read.handle_mut().into()) };
            read.get()
        };
        // Sets a new owner `id` as `record`'s private, the engine being its only owner.
        fn give(runtime: &Runtime, record: *mut JSObject, id: i32) -> Weak<NativeOwner> {
            let owner = new_owner(runtime, id, None);
            unsafe { SetModulePrivate(record, &PrivateValue(Rc::as_ptr(&owner).cast())) };
            Rc::downgrade(&owner)
        }
        let first = give(&runtime, module.get(), 1);
        let first_json = give(&runtime, json.get(), 4);
        collect(&mut runtime);
        assert!(owners_finalized().is_empty(), "the source module and JSON privates trace their owners");
        assert_eq!((marker(&runtime, &first), marker(&runtime, &first_json)), ("owner 1".to_owned(), "owner 4".to_owned()));
        assert_eq!(read(module.get()), private_of(Some(&first)));
        assert_eq!(read(json.get()), private_of(Some(&first_json)));

        // Replacement releases the old owner at once; only the new one is traced from then on.
        let second = give(&runtime, module.get(), 2);
        assert!(first.upgrade().is_none());
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![1]);
        assert_eq!(marker(&runtime, &second), "owner 2");
        // Undefined clears the private.
        set(module.get(), None);
        assert!(second.upgrade().is_none() && read(module.get()).is_undefined());
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![1, 2]);
        // Re-setting the same private retains before it releases.
        let third = give(&runtime, module.get(), 3);
        set(module.get(), Some(&third));
        assert_eq!(third.strong_count(), 1);
        let second_json = give(&runtime, json.get(), 5);
        assert!(first_json.upgrade().is_none(), "the replaced JSON private is released");
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![1, 2, 4]);
        assert_eq!((marker(&runtime, &third), marker(&runtime, &second_json)), ("owner 3".to_owned(), "owner 5".to_owned()));
        assert_eq!(read(json.get()), private_of(Some(&second_json)));
        let owners = [first, second, third, first_json, second_json];
        drop(runtime);
        assert!(owners.iter().all(|owner| owner.upgrade().is_none()), "shutdown releases the remaining owners");
        assert_eq!(count(&RETAINS), count(&RELEASES));
        assert_eq!(count(&RETAINS), 6);
        assert!(!STALE_TRACE.with(Cell::get), "no trace after release");
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    thread_local! {
        /// Source module records by specifier, and the importer private of the last import.
        static RECORDS: RefCell<HashMap<std::string::String, usize>> = RefCell::new(HashMap::new());
        static IMPORTER: Cell<Option<JSVal>> = const { Cell::new(None) };
    }

    thread_local! {
        /// JSON record switches of `resolve_source`: names handed out once (their records are
        /// not rooted), names whose resolution first runs a full precise collection, and the
        /// name whose resolution first links another record (a nested `ModuleLink`).
        static ONCE: RefCell<Vec<std::string::String>> = const { RefCell::new(Vec::new()) };
        static GC_ON: RefCell<Vec<std::string::String>> = const { RefCell::new(Vec::new()) };
        static LINK_ON: RefCell<Option<(std::string::String, usize)>> = const { RefCell::new(None) };
        static NESTED_LINKED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    /// Full collections of both heaps from inside a hook, scanning no native stack: no frame
    /// below holds an unrooted cell pointer it still needs.
    fn precise_gc(cx: *mut JSContext) {
        let isolate = unsafe { &mut *(*cx).isolate };
        if let Some(heap) = isolate.get_cpp_heap() {
            unsafe { heap.collect_garbage_for_testing(v8::cppgc::EmbedderStackState::NoHeapPointers) };
        }
    }

    unsafe extern "C" fn resolve_source(cx: *mut JSContext, _private: Handle<JSVal>, request: Handle<*mut JSObject>) -> *mut JSObject {
        let string = unsafe { GetModuleRequestSpecifier(cx, request) };
        let safe = unsafe { crate::context::JSContext::from_ptr(ptr::NonNull::new(cx).unwrap()) };
        let name = unsafe { crate::conversions::jsstr_to_string(&safe, ptr::NonNull::new(string).unwrap()) };
        if let Some((_, record)) = LINK_ON.with(|on| on.borrow_mut().take_if(|(on, _)| *on == name)) {
            rooted!(in(cx) let record = record as *mut JSObject);
            let linked = unsafe { ModuleLink(cx, record.handle().into()) };
            NESTED_LINKED.with(|nested| nested.set(Some(linked)));
        }
        if GC_ON.with(|names| names.borrow().contains(&name)) {
            precise_gc(cx);
        }
        let once = ONCE.with(|once| once.borrow().contains(&name));
        // JSON records are registered weakly: one a test left unrooted is handed out only
        // while it lives, and fails resolution once collected instead of dangling.
        let record = match crate::modules_impl::json_record_for_testing(&name, once) {
            Some(live) => live.map(|record| record as usize),
            None => RECORDS.with(|records| if once { records.borrow_mut().remove(&name) } else { records.borrow().get(&name).copied() }),
        };
        match record {
            Some(record) => record as *mut JSObject,
            None => {
                rooted!(in(cx) let error = Int32Value(404));
                unsafe { JS_SetPendingException(cx, error.handle(), ExceptionStackBehavior::DoNotCapture) };
                ptr::null_mut()
            },
        }
    }

    unsafe extern "C" fn note_importer(_cx: *mut JSContext, private: Handle<JSVal>, _request: Handle<*mut JSObject>, _promise: Handle<*mut JSObject>) -> bool {
        IMPORTER.with(|importer| importer.set(Some(private.get())));
        true
    }

    /// A runtime with module hooks and, when `hooks`, the native private hooks above.
    fn source_runtime(hooks: bool) -> Runtime {
        reset();
        RECORDS.with(|records| records.borrow_mut().clear());
        IMPORTER.with(|importer| importer.set(None));
        ONCE.with(|once| once.borrow_mut().clear());
        GC_ON.with(|names| names.borrow_mut().clear());
        LINK_ON.with(|on| on.borrow_mut().take());
        NESTED_LINKED.with(|nested| nested.set(None));
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        unsafe {
            if hooks {
                SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release));
                SetScriptPrivateTraceHook(runtime.rt(), Some(trace));
            }
            SetModuleResolveHook(runtime.rt(), Some(resolve_source));
            SetModuleDynamicImportHook(runtime.rt(), Some(note_importer));
        }
        runtime
    }

    /// Compiles a source module; its record is not rooted.
    fn compile_source(runtime: &Runtime, name: &str, source: &str) -> *mut JSObject {
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new(name).unwrap(), 1);
        let mut text = transform_str_to_source_text(source);
        let record = unsafe { CompileModule1(cx, options.ptr, &mut text) };
        assert!(!record.is_null(), "{name} compiles");
        RECORDS.with(|records| records.borrow_mut().insert(name.to_owned(), record as usize));
        record
    }

    /// Links and evaluates `record` (rooted by the caller); publishes its namespace as `ns`.
    fn evaluate_into_ns(runtime: &Runtime, record: *mut JSObject) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let record = record);
        assert!(unsafe { ModuleLink(cx, record.handle().into()) });
        rooted!(in(cx) let mut evaluation = UndefinedValue());
        assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
        unsafe { RunJobs(cx) };
        rooted!(in(cx) let namespace = ObjectValue(unsafe { GetModuleNamespace(cx, record.handle().into()) }));
        rooted!(in(cx) let global = eval(runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"ns".as_ptr(), namespace.handle().into_handle()) });
    }

    fn live_sources() -> usize {
        crate::modules_impl::source_records_for_testing().0
    }

    fn read_private(runtime: &Runtime, record: *mut JSObject) -> JSVal {
        rooted!(in(runtime.raw_cx()) let mut read = UndefinedValue());
        unsafe { crate::glue::JS_GetModulePrivate(record, read.handle_mut().into()) };
        read.get()
    }

    /// The importer private the dynamic import hook sees for `call`.
    fn importer_of(runtime: &Runtime, call: &str) -> JSVal {
        IMPORTER.with(|importer| importer.set(None));
        eval(runtime, call);
        IMPORTER.with(Cell::get).expect("the dynamic import hook ran")
    }

    #[test]
    fn source_module_records_live_with_their_graph_and_collect_with_their_private() {
        let mut runtime = source_runtime(true);
        let cx = runtime.raw_cx();
        let weak = {
            rooted!(in(cx) let dep = compile_source(&runtime, "dep.js", "export const answer = 40;"));
            rooted!(in(cx) let main = compile_source(&runtime, "main.js",
                "import { answer } from 'dep.js';\nexport function owned() { return answer + 2; }\nexport function later() { return import('dep.js'); }"));
            evaluate_into_ns(&runtime, main.get());
            // The private owns an object holding `owned`: owner -> function -> source -> private.
            let owner = new_owner(&runtime, 1, Some("ns.owned"));
            unsafe { SetModulePrivate(main.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            assert!(!dep.get().is_null());
            Rc::downgrade(&owner)
        };
        RECORDS.with(|records| records.borrow_mut().clear());
        collect(&mut runtime);
        assert_eq!(live_sources(), 2, "the namespace keeps its record, its static graph the dependency's");
        assert!(owners_finalized().is_empty());
        assert_eq!(marker(&runtime, &weak), "owner 1");
        assert_eq!(importer_of(&runtime, "ns.later()"), private_of(Some(&weak)), "dynamic import after GC sees the private");
        eval(&runtime, "globalThis.later = ns.later; delete globalThis.ns;");
        collect(&mut runtime);
        assert_eq!(live_sources(), 2, "exported functions keep their module");
        assert_eq!(importer_of(&runtime, "globalThis.pending = later()"), private_of(Some(&weak)));
        eval(&runtime, "delete globalThis.later;");
        collect(&mut runtime);
        assert_eq!(live_sources(), 2, "a pending import keeps its importer");
        assert!(weak.upgrade().is_some());
        eval(&runtime, "delete globalThis.pending;");
        IMPORTER.with(|importer| importer.set(None));
        collect(&mut runtime);
        assert!(weak.upgrade().is_none(), "the unreachable source module releases its private");
        assert_eq!(count(&RELEASES), 1);
        assert_eq!(owners_finalized(), vec![1], "the record/source/private/owner cycle collects");
        assert_eq!(live_sources(), 0, "both records collect");
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![1], "the owner is finalized once");
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1), "no release at shutdown after the GC one");
        assert!(!STALE_TRACE.with(Cell::get), "no trace after release");
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    #[test]
    fn rooted_errored_source_record_survives_collection_without_a_private() {
        let mut runtime = source_runtime(true);
        let cx = runtime.raw_cx();
        let weak = {
            rooted!(in(cx) let record = compile_source(&runtime, "errored.js", "export {}; throw new Error('boom');"));
            assert!(unsafe { ModuleLink(cx, record.handle().into()) });
            {
                rooted!(in(cx) let mut evaluation = UndefinedValue());
                assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
                unsafe { RunJobs(cx) };
                rooted!(in(cx) let promise = evaluation.get().to_object());
                assert!(!unsafe { ThrowOnModuleEvaluationFailure(cx, promise.handle().into(), ModuleErrorBehaviour::ThrowModuleErrorsSync) });
                unsafe { JS_ClearPendingException(cx) };
            }
            RECORDS.with(|records| records.borrow_mut().clear());
            collect(&mut runtime);
            assert_eq!(live_sources(), 1, "a rooted errored record outlives collections without a private");
            assert!(read_private(&runtime, record.get()).is_undefined());
            let owner = new_owner(&runtime, 3, None);
            unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            let weak = Rc::downgrade(&owner);
            drop(owner);
            collect(&mut runtime);
            assert_eq!(read_private(&runtime, record.get()), private_of(Some(&weak)), "the errored record keeps its script id and metadata");
            assert!(unsafe { IsCyclicModule(record.get()) });
            weak
        };
        collect(&mut runtime);
        assert!(weak.upgrade().is_none(), "the unrooted record releases its private");
        assert_eq!(live_sources(), 0);
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1));
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    #[test]
    fn weak_source_index_sweeps_collected_records_as_insertions_amortize() {
        let mut runtime = source_runtime(false);
        let cx = runtime.raw_cx();
        rooted!(in(cx) let kept = compile_source(&runtime, "kept.js", "export default 1;"));
        unsafe { SetModulePrivate(kept.get(), &Int32Value(5)) };
        for round in 0..2 {
            for index in 0..300 {
                compile_source(&runtime, &format!("dead-{round}-{index}.js"), "export default 0;");
            }
            RECORDS.with(|records| records.borrow_mut().clear());
            collect(&mut runtime);
        }
        let (live, entries) = crate::modules_impl::source_records_for_testing();
        assert_eq!(live, 1, "only the rooted record survives");
        // 601 registrations; a sweep at the 256th insertion of the second round dropped the
        // first round's dead entries.
        assert!(entries <= 301, "collected records are swept ({entries} entries left)");
        assert_eq!(read_private(&runtime, kept.get()).to_int32(), 5, "the survivor is still found");
    }

    /// One module whose private owns (through `ns.owned`) an object holding its function.
    /// Returns the private and the live source records once only the namespace keeps the
    /// module, then once nothing does.
    fn namespace_lifetime(runtime: &mut Runtime) -> (Weak<NativeOwner>, usize, usize) {
        let cx = runtime.raw_cx();
        let weak = {
            rooted!(in(cx) let record = compile_source(runtime, "lone.js", "export function owned() { return 1; }"));
            evaluate_into_ns(runtime, record.get());
            let owner = new_owner(runtime, 9, Some("ns.owned"));
            unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            Rc::downgrade(&owner)
        };
        RECORDS.with(|records| records.borrow_mut().clear());
        collect(runtime);
        let with_namespace = live_sources();
        eval(runtime, "delete globalThis.ns;");
        collect(runtime);
        (weak, with_namespace, live_sources())
    }

    #[test]
    fn negative_fixtures_detect_strong_source_roots_and_missing_record_edges() {
        use crate::modules_impl::{STRONG_SOURCES_FOR_TESTING as STRONG, UNANCHORED_SOURCES_FOR_TESTING as UNANCHORED};
        {
            let mut runtime = source_runtime(true);
            let (weak, with_namespace, after) = namespace_lifetime(&mut runtime);
            assert_eq!((with_namespace, after), (1, 0), "control: the namespace keeps the record, then it collects");
            assert!(weak.upgrade().is_none());
        }
        // The pre-0275 strong source root: the record and its private never collect.
        STRONG.with(|flag| flag.set(true));
        {
            let mut runtime = source_runtime(true);
            let (weak, with_namespace, after) = namespace_lifetime(&mut runtime);
            STRONG.with(|flag| flag.set(false));
            assert_eq!((with_namespace, after), (1, 1), "a strong root keeps the unreachable record");
            assert!(weak.upgrade().is_some(), "and its private, until shutdown");
            drop(runtime);
            assert!(weak.upgrade().is_none());
        }
        // Without the metadata's record edge the record dies while its module still lives.
        UNANCHORED.with(|flag| flag.set(true));
        {
            let mut runtime = source_runtime(true);
            let (weak, with_namespace, _) = namespace_lifetime(&mut runtime);
            UNANCHORED.with(|flag| flag.set(false));
            assert_eq!(with_namespace, 0, "a missing record edge loses a record whose namespace is live");
            assert!(weak.upgrade().is_none());
            drop(runtime);
        }
        assert_eq!(count(&RETAINS), count(&RELEASES));
    }

    // --- JSON module records -----------------------------------------------------------------

    fn json_records() -> (usize, usize) {
        crate::modules_impl::json_records_for_testing()
    }

    fn linking_len() -> usize {
        crate::modules_impl::linking_len_for_testing()
    }

    fn held(record: *mut JSObject) -> bool {
        crate::modules_impl::held_record_for_testing(record)
    }

    fn check(runtime: &Runtime, source: &str) {
        assert_eq!(describe(runtime, eval(runtime, source)), "true", "{source}");
    }

    /// Compiles a JSON module for the resolve hook (registered weakly); callers root the
    /// record before the next allocation.
    fn compile_json(runtime: &Runtime, name: &str, source: &str) -> *mut JSObject {
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new(name).unwrap(), 1);
        let mut text = transform_str_to_source_text(source);
        let record = unsafe { CompileJsonModule1(cx, options.ptr, &mut text) };
        assert!(!record.is_null(), "{name} compiles");
        crate::modules_impl::register_json_for_testing(name, Some(record));
        record
    }

    /// Stops the resolve hook from handing out `name`.
    fn unregister(name: &str) {
        RECORDS.with(|records| records.borrow_mut().remove(name));
        crate::modules_impl::register_json_for_testing(name, None);
    }

    /// Hands `name` out once: the hook forgets it as it resolves it.
    fn once(name: &str) {
        ONCE.with(|once| once.borrow_mut().push(name.to_owned()));
    }

    fn link(runtime: &Runtime, record: *mut JSObject) -> bool {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let record = record);
        let linked = unsafe { ModuleLink(cx, record.handle().into()) };
        if !linked {
            unsafe { JS_ClearPendingException(cx) };
        }
        linked
    }

    /// Evaluates the linked `record` and publishes its namespace as `globalThis[name]`; the
    /// evaluation error as a string on failure.
    fn evaluate(runtime: &Runtime, record: *mut JSObject, name: &std::ffi::CStr) -> Outcome {
        let cx = runtime.raw_cx();
        let error = |runtime: &Runtime| {
            rooted!(in(cx) let mut error = UndefinedValue());
            assert!(unsafe { JS_GetPendingException(cx, error.handle_mut().into()) });
            unsafe { JS_ClearPendingException(cx) };
            Err(describe(runtime, error.get()))
        };
        rooted!(in(cx) let record = record);
        rooted!(in(cx) let mut evaluation = UndefinedValue());
        if !unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) } {
            return error(runtime);
        }
        unsafe { RunJobs(cx) };
        rooted!(in(cx) let promise = evaluation.get().to_object());
        if !unsafe { ThrowOnModuleEvaluationFailure(cx, promise.handle().into(), ModuleErrorBehaviour::ThrowModuleErrorsSync) } {
            return error(runtime);
        }
        rooted!(in(cx) let namespace = ObjectValue(unsafe { GetModuleNamespace(cx, record.handle().into()) }));
        rooted!(in(cx) let global = eval(runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), name.as_ptr(), namespace.handle().into_handle()) });
        Ok(())
    }

    /// A module's evaluation: Ok, or its error as a string.
    type Outcome = std::result::Result<(), std::string::String>;

    const MISSING_VALUE: &str = "TypeError: JSON module without its parsed value";

    /// An unrooted JSON record with a private and `kept` (rooted by the caller), both
    /// unlinked, then a full collection. Returns the live JSON records after it, the unrooted
    /// record's owner and `kept`'s.
    fn unlinked_lifetime(runtime: &mut Runtime, kept: *mut JSObject) -> (usize, Weak<NativeOwner>, Weak<NativeOwner>) {
        let cx = runtime.raw_cx();
        let owners = {
            rooted!(in(cx) let dropped = compile_json(runtime, "dropped.json", "[1]"));
            unregister("dropped.json");
            [(dropped.get(), 1), (kept, 2)].map(|(record, id)| {
                let owner = new_owner(runtime, id, None);
                unsafe { SetModulePrivate(record, &PrivateValue(Rc::as_ptr(&owner).cast())) };
                Rc::downgrade(&owner)
            })
        };
        collect(runtime);
        let [dropped, kept_owner] = owners;
        assert_eq!(read_private(runtime, kept), private_of(Some(&kept_owner)), "a rooted record keeps its private");
        (json_records().0, dropped, kept_owner)
    }

    #[test]
    fn unlinked_json_records_collect_with_their_private() {
        let mut runtime = source_runtime(true);
        let cx = runtime.raw_cx();
        let kept_owner = {
            // Rooted until its evaluation published its value.
            rooted!(in(cx) let kept = compile_json(&runtime, "kept.json", r#"{"n": 1}"#));
            let (live, dropped, kept_owner) = unlinked_lifetime(&mut runtime, kept.get());
            assert_eq!(live, 1, "only the rooted unlinked record survives");
            assert!(dropped.upgrade().is_none(), "the collected record released its private");
            assert_eq!((count(&RELEASES), owners_finalized()), (1, vec![1]));
            assert_eq!(marker(&runtime, &kept_owner), "owner 2");
            assert_eq!(json_records(), (1, 0), "unlinked JSON entries are weak");
            // The surviving record still links and evaluates with its value.
            rooted!(in(cx) let user = compile_source(&runtime, "kept-user.js", "import data from 'kept.json' with { type: 'json' };\nexport const n = data.n;"));
            assert!(link(&runtime, user.get()));
            assert_eq!(evaluate(&runtime, user.get(), c"keptNs"), Ok(()));
            check(&runtime, "keptNs.n === 1");
            kept_owner
        };
        // Evaluated, its value lives in V8: the now unrooted record collects with its private.
        collect(&mut runtime);
        check(&runtime, "keptNs.n === 1");
        assert!(kept_owner.upgrade().is_none());
        assert_eq!(json_records(), (0, 0));
        assert_eq!(owners_finalized(), vec![1, 2], "each owner is finalized once");
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (2, 2), "both released by GC, none at shutdown");
        assert!(!STALE_TRACE.with(Cell::get) && !RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    /// `json-main.js` imports `data.json`, links, then `data.json` is unrooted and a full
    /// collection runs. Returns the (live, strong) JSON records after it and the evaluation
    /// outcome.
    fn linked_lifetime(runtime: &mut Runtime) -> ((usize, usize), Outcome) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let data = compile_json(runtime, "data.json", r#"{"n": 1}"#));
        rooted!(in(cx) let main = compile_source(runtime, "json-main.js",
            "import data from 'data.json' with { type: 'json' };\nexport default data;\nexport function again() { return data; }"));
        assert!(link(runtime, main.get()));
        assert_eq!(linking_len(), 0, "the link released its temporary roots");
        // From here only what the link left keeps the record.
        drop(data);
        unregister("data.json");
        collect(runtime);
        let linked = json_records();
        (linked, evaluate(runtime, main.get(), c"ns"))
    }

    #[test]
    fn linked_json_value_survives_until_evaluation() {
        let mut runtime = source_runtime(false);
        let (linked, evaluated) = linked_lifetime(&mut runtime);
        assert_eq!(linked, (1, 1), "a linked, unevaluated record is held for its steps");
        assert_eq!(evaluated, Ok(()));
        check(&runtime, "ns.default.n === 1 && ns.again() === ns.default");
        eval(&runtime, "globalThis.kept = ns.default; kept.mark = 7;");
        assert_eq!(json_records(), (1, 0), "evaluated: the entry is weak");
        collect(&mut runtime);
        assert_eq!(json_records(), (0, 0), "the evaluated record collects");
        check(&runtime, "ns.default === kept && ns.again() === kept && kept.mark === 7 && kept.n === 1");
    }

    #[test]
    fn failed_links_release_temporary_roots_and_keep_linked_dependencies() {
        let mut runtime = source_runtime(false);
        let cx = runtime.raw_cx();
        // V8 resets both JSON modules when a later request cannot be resolved.
        rooted!(in(cx) let rooted_json = compile_json(&runtime, "rooted.json", "1"));
        rooted!(in(cx) let loose = compile_json(&runtime, "loose.json", "2"));
        rooted!(in(cx) let main = compile_source(&runtime, "missing-main.js",
            "import a from 'rooted.json' with { type: 'json' };\nimport b from 'loose.json' with { type: 'json' };\nimport c from 'missing.js';"));
        assert!(!link(&runtime, main.get()), "a missing module fails the link");
        assert_eq!(linking_len(), 0);
        assert_eq!(json_records(), (2, 0), "reset JSON modules are reachable only through their records");
        drop(loose);
        unregister("loose.json");
        collect(&mut runtime);
        assert_eq!(json_records(), (1, 0), "the unrooted reset record collects, the rooted one survives");
        assert!(!rooted_json.get().is_null());
        rooted!(in(cx) let user = compile_source(&runtime, "rooted-user.js", "import a from 'rooted.json' with { type: 'json' };\nexport const value = a;"));
        assert!(link(&runtime, user.get()));
        assert_eq!(evaluate(&runtime, user.get(), c"rootedNs"), Ok(()));
        check(&runtime, "rootedNs.value === 1");

        // `b.js` completes its component (with `data.json`) before `bad.js` fails: V8 keeps
        // both linked, and `b.js` still needs the value.
        rooted!(in(cx) let data = compile_json(&runtime, "data.json", r#"{"n": 5}"#));
        rooted!(in(cx) let b = compile_source(&runtime, "b.js", "import data from 'data.json' with { type: 'json' };\nexport const value = data;"));
        rooted!(in(cx) let bad = compile_source(&runtime, "bad.js", "import { nope } from 'data.json' with { type: 'json' };"));
        rooted!(in(cx) let main = compile_source(&runtime, "partial-main.js", "import 'b.js';\nimport 'bad.js';"));
        assert!(!link(&runtime, main.get()), "a missing export fails the link");
        assert_eq!(linking_len(), 0);
        drop(data);
        drop(bad);
        unregister("data.json");
        unregister("bad.js");
        collect(&mut runtime);
        assert_eq!(json_records(), (2, 1), "the dependency left linked is held for its steps");
        assert!(link(&runtime, b.get()));
        assert_eq!(evaluate(&runtime, b.get(), c"bNs"), Ok(()));
        check(&runtime, "bNs.value.n === 5");
        assert_eq!(json_records(), (2, 0));
    }

    /// `gc-main.js` imports `data.json` (unrooted as the link starts, handed out once), then
    /// `gc.js`, whose resolution collects. Returns the (live, strong) JSON records after the
    /// link and the evaluation outcome.
    fn gc_during_resolve(runtime: &mut Runtime) -> ((usize, usize), Outcome) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let data = compile_json(runtime, "data.json", r#"{"n": 3}"#));
        once("data.json");
        rooted!(in(cx) let gc = compile_source(runtime, "gc.js", "export {};"));
        GC_ON.with(|names| names.borrow_mut().push("gc.js".to_owned()));
        rooted!(in(cx) let main = compile_source(runtime, "gc-main.js",
            "import data from 'data.json' with { type: 'json' };\nimport 'gc.js';\nexport default data;"));
        // Only the weak registry reaches it until the link's resolution roots it.
        drop(data);
        assert!(link(runtime, main.get()));
        assert_eq!(linking_len(), 0);
        assert!(!gc.get().is_null());
        let linked = json_records();
        collect(runtime);
        (linked, evaluate(runtime, main.get(), c"gcNs"))
    }

    #[test]
    fn resolved_json_records_survive_collection_and_nested_links() {
        let mut runtime = source_runtime(false);
        let (linked, evaluated) = gc_during_resolve(&mut runtime);
        assert_eq!(linked, (1, 1), "the link's temporary root kept the record across the collection");
        assert_eq!(evaluated, Ok(()));
        check(&runtime, "gcNs.default.n === 3");
        drop(runtime);

        // The outer link resolves `outer.json` (once) and `shared.json`, then `nested.js`,
        // whose resolution links `inner-main.js` (resolving `shared.json`, `inner.json` once and
        // `inner-gc.js`, which collects) and collects again once that link returned.
        let mut runtime = source_runtime(false);
        let cx = runtime.raw_cx();
        rooted!(in(cx) let outer = compile_json(&runtime, "outer.json", r#"{"outer": 1}"#));
        rooted!(in(cx) let shared = compile_json(&runtime, "shared.json", r#"{"shared": 2}"#));
        rooted!(in(cx) let inner_json = compile_json(&runtime, "inner.json", r#"{"inner": 3}"#));
        once("outer.json");
        once("inner.json");
        rooted!(in(cx) let nested = compile_source(&runtime, "nested.js", "export const nested = 4;"));
        rooted!(in(cx) let inner_gc = compile_source(&runtime, "inner-gc.js", "export {};"));
        rooted!(in(cx) let inner = compile_source(&runtime, "inner-main.js",
            "import shared from 'shared.json' with { type: 'json' };\nimport inner from 'inner.json' with { type: 'json' };\nimport 'inner-gc.js';\nexport const value = shared.shared + inner.inner;"));
        rooted!(in(cx) let main = compile_source(&runtime, "outer-main.js",
            "import outer from 'outer.json' with { type: 'json' };\nimport shared from 'shared.json' with { type: 'json' };\nimport { nested } from 'nested.js';\nexport const value = outer.outer + shared.shared + nested;"));
        GC_ON.with(|names| names.borrow_mut().extend(["inner-gc.js".to_owned(), "nested.js".to_owned()]));
        LINK_ON.with(|on| *on.borrow_mut() = Some(("nested.js".to_owned(), inner.get() as usize)));
        // Only the weak registry reaches the JSON records until the links' resolutions root them.
        drop(outer);
        drop(shared);
        drop(inner_json);
        assert!(link(&runtime, main.get()));
        assert_eq!(NESTED_LINKED.with(Cell::get), Some(true), "the nested link succeeded");
        assert_eq!(linking_len(), 0, "both links released their segments");
        assert_eq!(json_records(), (3, 3), "the outer segment kept its records across the nested link's collections");
        unregister("shared.json");
        collect(&mut runtime);
        assert_eq!(json_records(), (3, 3));
        assert_eq!(evaluate(&runtime, inner.get(), c"innerNs"), Ok(()));
        assert_eq!(evaluate(&runtime, main.get(), c"outerNs"), Ok(()));
        check(&runtime, "innerNs.value === 5 && outerNs.value === 7");
        assert!(!nested.get().is_null() && !inner_gc.get().is_null());
        collect(&mut runtime);
        assert_eq!(json_records(), (0, 0), "evaluated, unrooted records collect");
    }

    #[test]
    fn json_evaluation_failure_before_the_steps_keeps_the_record_linked() {
        let mut runtime = source_runtime(false);
        let cx = runtime.raw_cx();
        rooted!(in(cx) let main = compile_source(&runtime, "throw-main.js",
            "import 'throw.js';\nimport data from 'data.json' with { type: 'json' };"));
        {
            rooted!(in(cx) let data = compile_json(&runtime, "data.json", r#"{"n": 8}"#));
            rooted!(in(cx) let thrower = compile_source(&runtime, "throw.js", "throw new Error('boom');"));
            assert!(link(&runtime, main.get()));
            assert_eq!(evaluate(&runtime, main.get(), c"throwNs"), Err("Error: boom".to_owned()));
            assert!(!data.get().is_null() && !thrower.get().is_null());
        }
        // Only the errored importer reaches `data.json` from here: the weak registry hands
        // it out only while it lives.
        unregister("throw.js");
        collect(&mut runtime);
        assert_eq!(json_records(), (1, 1), "the unevaluated record is still held for its steps");
        rooted!(in(cx) let again = compile_source(&runtime, "again.js", "import data from 'data.json' with { type: 'json' };\nexport const n = data.n;"));
        assert!(link(&runtime, again.get()));
        assert_eq!(evaluate(&runtime, again.get(), c"againNs"), Ok(()));
        check(&runtime, "againNs.n === 8");
        unregister("data.json");
        collect(&mut runtime);
        assert_eq!(json_records(), (0, 0));
        assert!(!main.get().is_null());
    }

    #[test]
    fn abandoned_linked_json_graphs_collect_and_held_ones_evaluate_later() {
        fn give(runtime: &Runtime, record: *mut JSObject, id: i32) -> Weak<NativeOwner> {
            let owner = new_owner(runtime, id, None);
            unsafe { SetModulePrivate(record, &PrivateValue(Rc::as_ptr(&owner).cast())) };
            Rc::downgrade(&owner)
        }
        let mut runtime = source_runtime(true);
        let cx = runtime.raw_cx();
        // Each graph links but never runs the JSON steps; nothing reaches it once its block
        // ends and the resolve hook forgets its names.
        let linked = {
            rooted!(in(cx) let data = compile_json(&runtime, "linked.json", r#"{"id": 1}"#));
            let owner = give(&runtime, data.get(), 1);
            rooted!(in(cx) let main = compile_source(&runtime, "linked-main.js", "import data from 'linked.json' with { type: 'json' };\nexport default data;"));
            assert!(link(&runtime, main.get()));
            owner
        };
        let root = {
            rooted!(in(cx) let data = compile_json(&runtime, "root.json", r#"{"id": 2}"#));
            let owner = give(&runtime, data.get(), 2);
            assert!(link(&runtime, data.get()), "a JSON module linked as the root");
            owner
        };
        let errored = {
            rooted!(in(cx) let data = compile_json(&runtime, "errored.json", r#"{"id": 3}"#));
            let owner = give(&runtime, data.get(), 3);
            rooted!(in(cx) let thrower = compile_source(&runtime, "errored-throw.js", "throw new Error('boom');"));
            rooted!(in(cx) let main = compile_source(&runtime, "errored-main.js",
                "import 'errored-throw.js';\nimport data from 'errored.json' with { type: 'json' };"));
            assert!(link(&runtime, main.get()));
            assert_eq!(evaluate(&runtime, main.get(), c"erroredNs"), Err("Error: boom".to_owned()));
            assert!(!thrower.get().is_null());
            owner
        };
        let partial = {
            // `partial-b.js` completes its component (with `partial.json`) before
            // `partial-bad.js` fails: V8 leaves both linked.
            rooted!(in(cx) let data = compile_json(&runtime, "partial.json", r#"{"id": 4}"#));
            let owner = give(&runtime, data.get(), 4);
            rooted!(in(cx) let b = compile_source(&runtime, "partial-b.js", "import data from 'partial.json' with { type: 'json' };\nexport const value = data;"));
            rooted!(in(cx) let bad = compile_source(&runtime, "partial-bad.js", "import { nope } from 'partial.json' with { type: 'json' };"));
            rooted!(in(cx) let main = compile_source(&runtime, "partial-main.js", "import 'partial-b.js';\nimport 'partial-bad.js';"));
            assert!(!link(&runtime, main.get()), "a missing export fails the link");
            assert!(!b.get().is_null() && !bad.get().is_null());
            owner
        };
        // A held graph: only its rooted importer reaches the JSON module.
        rooted!(in(cx) let held_main = compile_source(&runtime, "held-main.js",
            "import data from 'held.json' with { type: 'json' };\nexport default data;\nexport function again() { return data; }"));
        let held = {
            rooted!(in(cx) let data = compile_json(&runtime, "held.json", r#"{"id": 5}"#));
            let owner = give(&runtime, data.get(), 5);
            assert!(link(&runtime, held_main.get()));
            owner
        };
        for name in ["linked.json", "linked-main.js", "root.json", "errored.json", "errored-throw.js", "errored-main.js", "partial.json",
            "partial-b.js", "partial-bad.js", "partial-main.js", "held.json"]
        {
            unregister(name);
        }
        collect(&mut runtime);
        assert_eq!(json_records(), (1, 1), "abandoned linked JSON records collect, the held one survives for its steps");
        for owner in [&linked, &root, &errored, &partial] {
            assert!(owner.upgrade().is_none(), "an abandoned record released its private");
        }
        let mut finalized = owners_finalized();
        finalized.sort();
        assert_eq!((finalized, count(&RELEASES)), (vec![1, 2, 3, 4], 4), "each abandoned owner once");
        assert_eq!(marker(&runtime, &held), "owner 5");
        // The held graph evaluates later with its value, read from the surviving record.
        assert_eq!(evaluate(&runtime, held_main.get(), c"heldNs"), Ok(()));
        check(&runtime, "heldNs.default.id === 5 && heldNs.again() === heldNs.default");
        eval(&runtime, "globalThis.heldValue = heldNs.default; delete globalThis.heldNs; 0");
        drop(held_main);
        collect(&mut runtime);
        assert_eq!(json_records(), (0, 0), "the evaluated record collects with its graph");
        assert!(held.upgrade().is_none());
        check(&runtime, "heldValue.id === 5");
        assert_eq!((owners_finalized().len(), count(&RELEASES)), (5, 5));
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (5, 5), "every private released once, none at shutdown");
        assert!(!STALE_TRACE.with(Cell::get) && !RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    /// An unrooted JSON record whose private's owner traces the record: a cycle through
    /// native code that nothing else reaches.
    fn json_owner_cycle(runtime: &Runtime, id: i32) -> Weak<NativeOwner> {
        let name = format!("cycle-{id}.json");
        rooted!(in(runtime.raw_cx()) let record = compile_json(runtime, &name, "{}"));
        unregister(&name);
        let owner = new_owner(runtime, id, None);
        owner.record.set(record.get());
        unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
        Rc::downgrade(&owner)
    }

    #[test]
    fn json_private_owner_cycles_collect_and_release_once() {
        let mut runtime = source_runtime(true);
        let cx = runtime.raw_cx();
        let cycle = json_owner_cycle(&runtime, 1);
        assert_eq!(cycle.strong_count(), 1, "the engine owns the only reference");
        let kept = {
            rooted!(in(cx) let kept = compile_json(&runtime, "kept-cycle.json", "{}"));
            let owner = new_owner(&runtime, 2, None);
            owner.record.set(kept.get());
            unsafe { SetModulePrivate(kept.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            let kept_owner = Rc::downgrade(&owner);
            drop(owner);
            collect(&mut runtime);
            assert!(cycle.upgrade().is_none(), "the record/private/owner cycle collects");
            assert_eq!((count(&RELEASES), owners_finalized()), (1, vec![1]));
            assert_eq!(json_records(), (1, 0));
            assert_eq!(read_private(&runtime, kept.get()), private_of(Some(&kept_owner)));
            assert_eq!(marker(&runtime, &kept_owner), "owner 2");
            kept_owner
        };
        assert_eq!(owners_finalized(), vec![1], "the collected owner is finalized once");
        drop(runtime);
        assert!(kept.upgrade().is_none(), "shutdown releases the live record's private");
        assert_eq!((count(&RETAINS), count(&RELEASES)), (2, 2), "exactly one release each");
        assert!(!STALE_TRACE.with(Cell::get), "no trace after release");
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    #[test]
    fn colliding_json_identity_hashes_keep_each_records_state() {
        let mut runtime = source_runtime(false);
        let cx = runtime.raw_cx();
        rooted!(in(cx) let first = compile_json(&runtime, "first.json", r#"{"first": true}"#));
        rooted!(in(cx) let second = compile_json(&runtime, "second.json", r#"{"second": true}"#));
        // `first`'s entry now sits first in `second`'s bucket.
        let shared = unsafe { &*cx }.with_scope(|scope| crate::modules_impl::collide_for_testing(scope, second.get(), first.get()));
        assert!(shared >= 2);
        rooted!(in(cx) let importer = compile_source(&runtime, "collide.js", "import second from 'second.json' with { type: 'json' };\nexport default second;"));
        assert!(link(&runtime, importer.get()));
        assert!(held(second.get()) && !held(first.get()), "linking holds only the linked record");
        collect(&mut runtime);
        assert_eq!(evaluate(&runtime, importer.get(), c"collideNs"), Ok(()));
        check(&runtime, "collideNs.default.second === true");
        assert!(!held(second.get()) && !held(first.get()));
        assert_eq!(json_records(), (2, 0));
        rooted!(in(cx) let value = crate::modules_impl::json_value_for_testing(first.get()));
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), c"firstValue".as_ptr(), value.handle().into_handle()) });
        check(&runtime, "firstValue.first === true");
    }

    #[test]
    fn negative_fixtures_detect_missing_json_roots_and_native_privates() {
        use crate::modules_impl::{
            NATIVE_JSON_PRIVATES_FOR_TESTING as NATIVE, STRONG_JSON_FOR_TESTING as STRONG, UNANCHORED_JSON_FOR_TESTING as UNANCHORED,
            UNROOTED_LINKING_FOR_TESTING as UNROOTED,
        };
        let with = |flag: &'static std::thread::LocalKey<Cell<bool>>, f: &mut dyn FnMut()| {
            flag.with(|flag| flag.set(true));
            f();
            flag.with(|flag| flag.set(false));
        };
        // Without weak entries (the pre-0279 strong JSON root) unlinked records never collect.
        with(&STRONG, &mut || {
            let mut runtime = source_runtime(true);
            let (live, dropped) = {
                rooted!(in(runtime.raw_cx()) let kept = compile_json(&runtime, "kept.json", r#"{"n": 1}"#));
                let (live, dropped, _) = unlinked_lifetime(&mut runtime, kept.get());
                (live, dropped)
            };
            assert_eq!(live, 2, "a strong entry keeps the unreachable record");
            assert!(dropped.upgrade().is_some(), "and its private, until shutdown");
            drop(runtime);
            assert!(dropped.upgrade().is_none());
            assert_eq!(count(&RETAINS), count(&RELEASES));
        });
        // Without their namespaces keeping linked records their value is lost before the steps.
        with(&UNANCHORED, &mut || {
            let mut runtime = source_runtime(false);
            let (linked, evaluated) = linked_lifetime(&mut runtime);
            assert_eq!(linked, (0, 0), "nothing held the linked record");
            assert_eq!(evaluated, Err(MISSING_VALUE.to_owned()));
        });
        // Without the link's temporary roots a collection during resolution loses the record.
        with(&UNROOTED, &mut || {
            let mut runtime = source_runtime(false);
            let (linked, evaluated) = gc_during_resolve(&mut runtime);
            assert_eq!(linked, (0, 0), "the record died during the link");
            assert_eq!(evaluated, Err(MISSING_VALUE.to_owned()));
        });
        // A private kept natively (traced as a root) keeps its owner/record cycle alive.
        with(&NATIVE, &mut || {
            let mut runtime = source_runtime(true);
            let cycle = json_owner_cycle(&runtime, 1);
            collect(&mut runtime);
            assert!(cycle.upgrade().is_some(), "the native root keeps the cycle");
            assert_eq!(json_records(), (1, 0), "through the owner's record edge");
            assert!(owners_finalized().is_empty() && count(&RELEASES) == 0);
            drop(runtime);
            assert!(cycle.upgrade().is_none());
            assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1));
        });
    }

    thread_local! {
        /// Scratch switches for the negative holder fixtures.
        static HOLDER_RETAINS: Cell<bool> = const { Cell::new(true) };
        static HOLDER_TRACES: Cell<bool> = const { Cell::new(true) };
        static STALE_RESOLVE: Cell<bool> = const { Cell::new(false) };
    }

    fn holder_private(holder: *mut JSObject) -> JSVal {
        let mut private = UndefinedValue();
        unsafe { JS_GetReservedSlot(holder, 0, &mut private) };
        private
    }

    /// Mirrors Servo's `ModuleScriptHolder` (script_module.rs), the hooks above standing in
    /// for the `ModuleScript` ones. Engine reference counts guard every use of the private,
    /// so the negative fixtures never dereference a released pointer.
    unsafe extern "C" fn trace_holder(trc: *mut JSTracer, holder: *mut JSObject) {
        let private = holder_private(holder);
        if !private.is_undefined() && HOLDER_TRACES.with(Cell::get) {
            unsafe { trace(trc, &private) };
        }
    }

    unsafe extern "C" fn finalize_holder(_gcx: *mut GCContext, holder: *mut JSObject) {
        let private = holder_private(holder);
        if !private.is_undefined() {
            unsafe { JS_SetReservedSlot(holder, 0, &UndefinedValue()) };
            if engine_refs(native_owner(&private)) > 0 {
                unsafe { release(&private) };
            }
        }
    }

    static HOLDER_OPS: JSClassOps = JSClassOps {
        addProperty: None,
        delProperty: None,
        enumerate: None,
        newEnumerate: None,
        resolve: None,
        mayResolve: None,
        finalize: Some(finalize_holder),
        call: None,
        construct: None,
        trace: Some(trace_holder),
    };

    static HOLDER_CLASS: JSClass = JSClass {
        name: c"ModuleScriptHolder".as_ptr(),
        flags: 1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT | crate::object::JSCLASS_FOREGROUND_FINALIZE,
        cOps: &HOLDER_OPS,
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    /// `import.meta.resolve`: reads the holder's private through rooted handles, converts its
    /// argument (reentrant, may collect) and returns owner id * 100 + specifier length, minus
    /// the length without a private, -1000 for a released private.
    unsafe extern "C" fn resolve(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        // vp[0] holds the callee until the return value replaces it.
        rooted!(in(cx) let holder = unsafe { *GetFunctionNativeReserved(args[0].to_object(), 0) });
        rooted!(in(cx) let private = if holder.is_object() { holder_private(holder.to_object()) } else { UndefinedValue() });
        use crate::conversions::{ConversionResult, FromJSValConvertible};
        let mut safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        let value = unsafe { crate::rust::HandleValue::from_marked_location(&args[2]) };
        let Ok(ConversionResult::Success(text)) = std::string::String::safe_from_jsval(&mut safe, value, ()) else { return false };
        let length = text.len() as i32;
        if private.is_undefined() {
            args[0] = Int32Value(-length);
            return true;
        }
        let pointer = native_owner(&private.get());
        if engine_refs(pointer) <= 0 {
            STALE_RESOLVE.with(|flag| flag.set(true));
            args[0] = Int32Value(-1000);
            return true;
        }
        let mut id = UndefinedValue();
        unsafe { JS_GetReservedSlot((*pointer).owner.get(), 0, &mut id) };
        args[0] = Int32Value(if id.is_int32() { id.to_int32() * 100 + length } else { -2000 });
        true
    }

    unsafe extern "C" fn gc_now(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        // SAFETY: the JSNative contract.
        let args = unsafe { std::slice::from_raw_parts_mut(vp, argc as usize + 2) };
        unsafe { &mut *(*cx).isolate }.low_memory_notification();
        args[0] = UndefinedValue();
        true
    }

    /// Defines `meta.resolve` as Servo's HostPopulateImportMeta does. With an owner id, the
    /// owner object (holding `meta.resolve`, a cycle through native code) is reachable only
    /// through the function's holder once the source's reference is dropped here.
    fn escaped_resolve(runtime: &Runtime, id: Option<i32>) -> Option<Weak<NativeOwner>> {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe { CurrentGlobalOrNull(cx) });
        unsafe { DefineFunctionWithReserved(cx, global.get(), c"gcNow".as_ptr(), Some(gc_now), 0, 0) };
        rooted!(in(cx) let meta = eval(runtime, "globalThis.meta = {}").to_object());
        let function = unsafe { DefineFunctionWithReserved(cx, meta.get(), c"resolve".as_ptr(), Some(resolve), 1, JSPROP_ENUMERATE.into()) };
        rooted!(in(cx) let function = unsafe { JS_GetFunctionObject(function) });
        let owner = new_owner(runtime, id?, Some("meta.resolve"));
        rooted!(in(cx) let holder = unsafe { JS_NewObject(cx, &HOLDER_CLASS) });
        let private = PrivateValue(Rc::as_ptr(&owner).cast());
        if HOLDER_RETAINS.with(Cell::get) {
            unsafe { retain(&private) };
        }
        unsafe { JS_SetReservedSlot(holder.get(), 0, &private) };
        unsafe { SetFunctionNativeReserved(function.get(), 0, &ObjectValue(holder.get())) };
        Some(Rc::downgrade(&owner))
    }

    fn holder_switches(retains: bool, traces: bool) {
        reset();
        HOLDER_RETAINS.with(|flag| flag.set(retains));
        HOLDER_TRACES.with(|flag| flag.set(traces));
        STALE_RESOLVE.with(|flag| flag.set(false));
    }

    #[test]
    fn escaped_import_meta_resolve_owns_and_traces_its_module_script() {
        holder_switches(true, true);
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        assert!(escaped_resolve(&runtime, None).is_none());
        assert_eq!(eval(&runtime, "meta.resolve('ab')").to_int32(), -2, "an undefined private falls back");
        let weak = escaped_resolve(&runtime, Some(9)).unwrap();
        assert_eq!(weak.strong_count(), 1, "the holder owns the only reference");
        collect(&mut runtime);
        assert!(owners_finalized().is_empty(), "the holder traces its owner");
        assert_eq!(marker(&runtime, &weak), "owner 9");
        let reentrant = "meta.resolve({ toString() { gcNow(); return 'abc' } })";
        assert_eq!(eval(&runtime, reentrant).to_int32(), 903, "GC in the conversion keeps the private");
        assert_eq!(eval(&runtime, "globalThis.escaped = meta.resolve; delete globalThis.meta; escaped('x')").to_int32(), 901);
        collect(&mut runtime);
        assert_eq!(eval(&runtime, "escaped('xy')").to_int32(), 902, "the escaped function keeps its metadata");
        assert!(owners_finalized().is_empty() && weak.upgrade().is_some());
        eval(&runtime, "delete globalThis.escaped");
        collect(&mut runtime);
        assert!(weak.upgrade().is_none(), "the dead function releases its module script");
        assert_eq!(owners_finalized(), vec![9], "the function/holder/owner cycle collects");
        collect(&mut runtime);
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1), "released once, none at shutdown");
        assert_eq!(owners_finalized(), vec![9]);
        assert!(!STALE_TRACE.with(Cell::get) && !STALE_RESOLVE.with(Cell::get));
        assert!(!RELEASE_WITHOUT_RUNTIME.with(Cell::get));
    }

    #[test]
    fn untraced_import_meta_holder_loses_its_owner() {
        holder_switches(true, false);
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let weak = escaped_resolve(&runtime, Some(9)).unwrap();
        collect(&mut runtime);
        assert_eq!(owners_finalized(), vec![9], "without the trace nothing reports the owner edge");
        assert!(weak.upgrade().is_some(), "the holder still owns the private");
        eval(&runtime, "delete globalThis.meta");
        collect(&mut runtime);
        assert!(weak.upgrade().is_none());
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (1, 1));
    }

    #[test]
    fn unretained_import_meta_holder_fails_safely() {
        holder_switches(false, true);
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let weak = escaped_resolve(&runtime, Some(9)).unwrap();
        assert!(weak.upgrade().is_none(), "nothing owns the private once the source drops it");
        assert_eq!(eval(&runtime, "meta.resolve('x')").to_int32(), -1000);
        assert!(STALE_RESOLVE.with(Cell::get), "the guarded resolve saw the released private");
        collect(&mut runtime);
        assert!(STALE_TRACE.with(Cell::get), "the guarded trace saw the released private");
        drop(runtime);
        assert_eq!((count(&RETAINS), count(&RELEASES)), (0, 0), "nothing released that was never retained");
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

    /// JSAPI operations keep the original built-ins when page scripts replace them.
    #[test]
    fn jsapi_operations_ignore_replaced_builtins() {
        unsafe extern "C" fn collect(buf: *const u16, len: u32, data: *mut std::ffi::c_void) -> bool {
            // SAFETY: the callback contract (chars, length, closure data).
            let units = unsafe { std::slice::from_raw_parts(buf, len as usize) };
            unsafe { &mut *(data as *mut std::string::String) }.push_str(&std::string::String::from_utf16_lossy(units));
            true
        }
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let value = eval(&runtime, "({ answer: 42, list: [1, 2] })"));
        rooted!(in(cx) let date_like = eval(&runtime, "({ valueOf() { return 7 }, toString() { return 'seven' } })").to_object());
        rooted!(in(cx) let executor = eval(&runtime, "(resolve) => resolve(5)").to_object());
        eval(&runtime, "JSON.stringify = () => 'replaced'; Reflect.get = () => 'replaced'; \
            Function.prototype.call = () => 'replaced'; Function.prototype.apply = () => 'replaced'; \
            Array.prototype[Symbol.iterator] = function* () { yield 'replaced'; }; \
            globalThis.Promise = function () { throw new Error('replaced'); }; 0");

        let mut json = std::string::String::new();
        rooted!(in(cx) let space = UndefinedValue());
        rooted!(in(cx) let replacer = ptr::null_mut::<JSObject>());
        assert!(unsafe { ToJSON(cx, value.handle().into_handle(), replacer.handle().into_handle(), space.handle().into_handle(), Some(collect), &mut json as *mut std::string::String as *mut std::ffi::c_void) });
        assert_eq!(json, r#"{"answer":42,"list":[1,2]}"#);

        rooted!(in(cx) let mut primitive = UndefinedValue());
        assert!(unsafe { ToPrimitive(cx, date_like.handle().into_handle(), JSType::JSTYPE_STRING, primitive.handle_mut().into()) });
        assert_eq!(describe(&runtime, primitive.get()), "seven");
        assert!(unsafe { ToPrimitive(cx, date_like.handle().into_handle(), JSType::JSTYPE_UNDEFINED, primitive.handle_mut().into()) });
        assert_eq!(primitive.get().to_int32(), 7);

        rooted!(in(cx) let promise = unsafe { NewPromiseObject(cx, executor.handle().into_handle()) });
        assert!(!promise.get().is_null());
        assert_eq!(unsafe { GetPromiseState(promise.handle().into_handle()) }, PromiseState::Fulfilled);
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

        // Detach keys: ordinary buffers have none; WebAssembly.Memory buffers (non-detachable
        // in V8) have one; a view is not a buffer.
        let detach_key = |object: *mut JSObject| {
            rooted!(in(cx) let object = object);
            let mut defined = false;
            unsafe { HasDefinedArrayBufferDetachKey(cx, object.handle().into_handle(), &mut defined) }.then_some(defined)
        };
        assert_eq!(detach_key(clone.get()), Some(false));
        assert_eq!(detach_key(view.get()), None);
        if eval(&runtime, "typeof WebAssembly === 'object'").to_boolean() {
            assert_eq!(detach_key(eval(&runtime, "new WebAssembly.Memory({ initial: 1 }).buffer").to_object()), Some(true));
        }
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
        // Rooted: JSON records no longer live by their entries (0279).
        rooted!(in(cx) let data = compile(&runtime, "data.json", r#"{"extra": 2}"#, true));
        assert!(!data.get().is_null());
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

    /// Links, evaluates and exposes `record`'s namespace as `globalThis[name]`.
    fn run_module(runtime: &Runtime, record: *mut JSObject, name: &std::ffi::CStr) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let record = record);
        assert!(unsafe { ModuleLink(cx, record.handle().into()) });
        rooted!(in(cx) let mut evaluation = UndefinedValue());
        assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
        unsafe { RunJobs(cx) };
        rooted!(in(cx) let evaluation_promise = evaluation.get().to_object());
        assert!(
            unsafe { ThrowOnModuleEvaluationFailure(cx, evaluation_promise.handle().into(), ModuleErrorBehaviour::ThrowModuleErrorsSync) },
            "the module graph evaluates"
        );
        rooted!(in(cx) let namespace = unsafe { GetModuleNamespace(cx, record.handle().into()) });
        rooted!(in(cx) let global = eval(runtime, "globalThis").to_object());
        rooted!(in(cx) let namespace_value = ObjectValue(namespace.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into_handle(), name.as_ptr(), namespace_value.handle().into_handle()) });
    }

    #[test]
    fn json_module_values_live_on_their_records() {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let rt = runtime.rt();
        unsafe {
            SetModuleResolveHook(rt, Some(resolve));
            SetModuleMetadataHook(rt, Some(metadata));
            SetModuleDynamicImportHook(rt, Some(dynamic_import));
        }
        // SAFETY: the runtime's live context.
        let gc = || unsafe { &mut *(*cx).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
        let check = |source: &str| assert_eq!(describe(&runtime, eval(&runtime, source)), "true", "{source}");
        rooted!(in(cx) let object = compile(&runtime, "object.json", r#"{"n": 1, "list": [1, 2]}"#, true));
        rooted!(in(cx) let null = compile(&runtime, "null.json", "null", true));
        rooted!(in(cx) let no = compile(&runtime, "false.json", "false", true));
        rooted!(in(cx) let zero = compile(&runtime, "zero.json", "0", true));
        rooted!(in(cx) let string = compile(&runtime, "string.json", r#""text""#, true));
        rooted!(in(cx) let array = compile(&runtime, "array.json", r#"[1, "a"]"#, true));
        assert!([object.get(), null.get(), no.get(), zero.get(), string.get(), array.get()].iter().all(|record| !record.is_null()));
        // Nothing but the records keeps the parsed values before instantiation.
        gc();
        rooted!(in(cx) let main = compile(
            &runtime,
            "json-main.js",
            "import object from 'object.json' with { type: 'json' };\n\
             import nul from 'null.json' with { type: 'json' };\n\
             import no from 'false.json' with { type: 'json' };\n\
             import zero from 'zero.json' with { type: 'json' };\n\
             import string from 'string.json' with { type: 'json' };\n\
             import array from 'array.json' with { type: 'json' };\n\
             export { object, nul, no, zero, string, array };\n\
             export function again() { return object; }",
            false,
        ));
        gc();
        run_module(&runtime, main.get(), c"jsonNs");
        check("jsonNs.nul === null && jsonNs.no === false && Object.is(jsonNs.zero, 0) && jsonNs.string === 'text'");
        check("Array.isArray(jsonNs.array) && jsonNs.array.length === 2 && jsonNs.array[1] === 'a'");
        check("jsonNs.object.n === 1 && jsonNs.object.list[1] === 2");
        eval(&runtime, "jsonNs.object.mark = 7; globalThis.keptJson = jsonNs.object;");
        gc();
        check("jsonNs.object === keptJson && jsonNs.object.mark === 7 && jsonNs.again() === keptJson");
        // A later importer of the evaluated JSON module sees the same, mutated value.
        rooted!(in(cx) let second = compile(
            &runtime,
            "json-second.js",
            "import object from 'object.json' with { type: 'json' };\nexport const same = object;",
            false,
        ));
        gc();
        run_module(&runtime, second.get(), c"jsonNs2");
        check("jsonNs2.same === keptJson && jsonNs2.same.mark === 7");
    }

    #[test]
    fn json_dynamic_import_survives_collection_while_pending() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let rt = runtime.rt();
        unsafe {
            SetModuleResolveHook(rt, Some(resolve));
            SetModuleDynamicImportHook(rt, Some(dynamic_import));
        }
        let collect = |runtime: &mut Runtime| {
            // SAFETY: the runtime's live context.
            unsafe { &mut *(*runtime.raw_cx()).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            runtime.gc_for_testing();
        };
        let check = |runtime: &Runtime, source: &str| assert_eq!(describe(runtime, eval(runtime, source)), "true", "{source}");
        eval(&runtime, "globalThis.pending = import('dynamic.json', { with: { type: 'json' } }); pending.then((m) => { globalThis.loaded = m; });");
        let (importer, request, promise) = DYNAMIC.with(|dynamic| dynamic.borrow_mut().take()).expect("the dynamic import hook ran");
        rooted!(in(cx) let importer = importer);
        rooted!(in(cx) let request = request as *mut JSObject);
        rooted!(in(cx) let promise = promise as *mut JSObject);
        assert_eq!(unsafe { GetModuleRequestType(cx, request.handle().into()) }, ModuleType::JSON);
        collect(&mut runtime);
        {
            // The embedder keeps the fetched record (Servo's module map) until the import ends.
            rooted!(in(cx) let record = compile(&runtime, "dynamic.json", r#"{"k": [3]}"#, true));
            assert!(unsafe { ModuleLink(cx, record.handle().into()) });
            rooted!(in(cx) let mut evaluation = UndefinedValue());
            assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
            rooted!(in(cx) let evaluation = evaluation.get().to_object());
            collect(&mut runtime);
            assert!(unsafe {
                FinishDynamicModuleImport(cx, evaluation.handle().into(), importer.handle().into(), request.handle().into(), promise.handle().into())
            });
            unsafe { RunJobs(cx) };
        }
        REGISTRY.with(|registry| registry.borrow_mut().remove("dynamic.json"));
        check(&runtime, "loaded.default.k[0] === 3");
        eval(&runtime, "globalThis.first = loaded.default;");
        collect(&mut runtime);
        assert_eq!(crate::modules_impl::json_records_for_testing(), (0, 0), "the evaluated record collected");
        check(&runtime, "loaded.default === first && first.k[0] === 3");
    }

    /// Host completion of `import()` waits on the evaluation promise without user-patchable
    /// `Promise.prototype.then`; a namespace's own `then` export is still assimilated.
    #[test]
    fn dynamic_import_completion_ignores_patched_promise_then() {
        let patches = [
            "function () { hijacked++; throw new Error('hijacked'); }",
            "function (f) { hijacked++; if (typeof f === 'function') f(); return 'hostile'; }",
        ];
        for patch in patches {
            let mut runtime = Runtime::new(JSEngineHandle::for_tests());
            let cx = runtime.raw_cx();
            let rt = runtime.rt();
            unsafe {
                SetModuleResolveHook(rt, Some(resolve));
                SetModuleDynamicImportHook(rt, Some(dynamic_import));
            }
            let check = |runtime: &Runtime, source: &str| assert_eq!(describe(runtime, eval(runtime, source)), "true", "{patch}: {source}");
            eval(&runtime, "globalThis.results = {}; globalThis.hijacked = 0; globalThis.gate = new Promise(r => globalThis.open = r); 0");
            rooted!(in(cx) let tla = compile(&runtime, "tla.js", "export const v = 1; await gate;", false));
            rooted!(in(cx) let fails = compile(&runtime, "fails.js", "throw new Error('boom');", false));
            rooted!(in(cx) let thenable = compile(&runtime, "thenable.js", "export function then(resolve) { resolve('assimilated'); }", false));
            eval(&runtime, &format!("Promise.prototype.then = {patch}; 0"));
            for (name, record) in [("tla.js", tla.get()), ("fails.js", fails.get()), ("thenable.js", thenable.get())] {
                // `await` on a native promise performs PromiseThen without looking up `then`.
                eval(&runtime, &format!("(async () => {{ try {{ results['{name}'] = await import('{name}'); }} catch (e) {{ results['{name}'] = e; }} }})(); 0"));
                let (importer, request, promise) = DYNAMIC.with(|dynamic| dynamic.borrow_mut().take()).expect("the dynamic import hook ran");
                rooted!(in(cx) let importer = importer);
                rooted!(in(cx) let request = request as *mut JSObject);
                rooted!(in(cx) let promise = promise as *mut JSObject);
                rooted!(in(cx) let record = record);
                assert!(unsafe { ModuleLink(cx, record.handle().into()) });
                rooted!(in(cx) let mut evaluation = UndefinedValue());
                assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
                rooted!(in(cx) let evaluation = evaluation.get().to_object());
                assert!(
                    unsafe { FinishDynamicModuleImport(cx, evaluation.handle().into(), importer.handle().into(), request.handle().into(), promise.handle().into()) },
                    "{patch}: {name} completion is registered"
                );
            }
            // SAFETY: the runtime's live context.
            unsafe { &mut *(*cx).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            runtime.gc_for_testing();
            unsafe { RunJobs(cx) };
            check(&runtime, "!('tla.js' in results)");
            check(&runtime, "results['fails.js'] instanceof Error && results['fails.js'].message === 'boom'");
            check(&runtime, "results['thenable.js'] === 'assimilated'");
            eval(&runtime, "open(); 0");
            unsafe { RunJobs(cx) };
            check(&runtime, "results['tla.js'].v === 1 && Object.prototype.toString.call(results['tla.js']) === '[object Module]'");
            check(&runtime, "hijacked === 0");
        }
    }
}

/// The private of the running classic script or source module (SpiderMonkey's source-object
/// private), as seen by natives and by dynamic `import()`.
mod caller_privates {
    use std::cell::RefCell;
    use std::ffi::{CStr, CString};
    use std::ptr;
    use std::rc::Rc;

    use super::eval;
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{CompileOptionsWrapper, JSEngineHandle, Runtime, transform_str_to_source_text};

    static GLOBAL_CLASS: JSClass = JSClass {
        name: c"CallerPrivateGlobal".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL |
            ((crate::object::JSCLASS_GLOBAL_SLOT_COUNT + 1) << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: ptr::null(),
        spec: ptr::null(),
        ext: ptr::null(),
        oOps: ptr::null(),
    };

    /// Identical code and name for every module of a test.
    const SHARED_SOURCE: &str = "export function owner() { return readPrivate(); }\nexport function load() { return import('owner.js'); }";

    thread_local! {
        /// Owners the dynamic-import hook saw; asserted outside the callback.
        static IMPORTERS: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn retain(value: *const JSVal) {
        unsafe { Rc::increment_strong_count((*value).to_private() as *const i32) };
    }

    unsafe extern "C" fn release(value: *const JSVal) {
        unsafe { Rc::decrement_strong_count((*value).to_private() as *const i32) };
    }

    fn owner(private: JSVal) -> i32 {
        if private.is_undefined() { -1 } else { unsafe { *(private.to_private() as *const i32) } }
    }

    unsafe extern "C" fn read_private(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        rooted!(in(cx) let mut private = UndefinedValue());
        unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
        unsafe { *vp = Int32Value(owner(private.get())) };
        true
    }

    /// SpiderMonkey's caller private ignores `HideScriptedCaller`.
    unsafe extern "C" fn read_hidden(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        rooted!(in(cx) let mut private = UndefinedValue());
        unsafe {
            HideScriptedCaller(cx);
            JS_GetScriptedCallerPrivate(cx, private.handle_mut().into());
            UnhideScriptedCaller(cx);
            *vp = Int32Value(owner(private.get()));
        }
        true
    }

    /// Re-enters script from a native: returns the global `classicOwner()`.
    unsafe extern "C" fn reenter(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        rooted!(in(cx) let global = unsafe { CurrentGlobalOrNull(cx) });
        let no_arguments = HandleValueArray { length_: 0, elements_: ptr::null() };
        rooted!(in(cx) let mut result = UndefinedValue());
        let ok = unsafe { JS_CallFunctionName(cx, global.handle().into(), c"classicOwner".as_ptr(), &no_arguments, result.handle_mut().into()) };
        unsafe { *vp = result.get() };
        ok
    }

    unsafe extern "C" fn record_import(_cx: *mut JSContext, private: Handle<JSVal>, _request: Handle<*mut JSObject>, _promise: Handle<*mut JSObject>) -> bool {
        IMPORTERS.with(|importers| importers.borrow_mut().push(owner(private.get())));
        true
    }

    /// Re-enters script from a native: returns the global `classicEvalLoad()`.
    unsafe extern "C" fn reenter_import(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        rooted!(in(cx) let global = unsafe { CurrentGlobalOrNull(cx) });
        let no_arguments = HandleValueArray { length_: 0, elements_: ptr::null() };
        rooted!(in(cx) let mut result = UndefinedValue());
        let ok = unsafe { JS_CallFunctionName(cx, global.handle().into(), c"classicEvalLoad".as_ptr(), &no_arguments, result.handle_mut().into()) };
        unsafe { *vp = result.get() };
        ok
    }

    fn runtime() -> Runtime {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        unsafe {
            SetScriptPrivateReferenceHooks(runtime.rt(), Some(retain), Some(release));
            SetModuleDynamicImportHook(runtime.rt(), Some(record_import));
        }
        runtime
    }

    /// Sets `name` on the current realm's global.
    fn set_global(runtime: &Runtime, name: &CStr, value: JSVal) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe { CurrentGlobalOrNull(cx) });
        rooted!(in(cx) let value = value);
        assert!(unsafe { JS_SetProperty(cx, global.handle().into(), name.as_ptr(), value.handle().into()) });
    }

    fn install(runtime: &Runtime, name: &CStr, native: JSNative) {
        let function = unsafe { JS_NewFunction(runtime.raw_cx(), native, 0, 0, name.as_ptr()) };
        set_global(runtime, name, ObjectValue(function.cast()));
    }

    /// Compiles, links and evaluates a module owned by `private` in the current realm and
    /// exposes its namespace as the global `name`. Module records live as long as the runtime.
    fn load_module(runtime: &Runtime, source: &str, private: &Rc<i32>, name: &CStr) -> *mut JSObject {
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("same-module.js").unwrap(), 1);
        let mut text = transform_str_to_source_text(source);
        rooted!(in(cx) let record = unsafe { CompileModule1(cx, options.ptr, &mut text) });
        assert!(!record.get().is_null());
        unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(private).cast())) };
        assert!(unsafe { ModuleLink(cx, record.handle().into()) });
        rooted!(in(cx) let mut evaluation = UndefinedValue());
        assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
        unsafe { RunJobs(cx) };
        let namespace = unsafe { GetModuleNamespace(cx, record.handle().into()) };
        set_global(runtime, name, ObjectValue(namespace));
        record.get()
    }

    /// Compiles and runs a classic script owned by `private` (none: no private) in the current realm.
    fn run_classic(runtime: &Runtime, name: &str, source: &str, private: Option<&Rc<i32>>) -> *mut JSScript {
        let cx = runtime.raw_cx();
        let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new(name).unwrap(), 1);
        let mut text = transform_str_to_source_text(source);
        rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut text) });
        assert!(!script.get().is_null());
        if let Some(private) = private {
            unsafe { SetScriptPrivate(script.get(), &PrivateValue(Rc::as_ptr(private).cast())) };
        }
        rooted!(in(cx) let mut result = UndefinedValue());
        assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
        script.get()
    }

    fn script_id(runtime: &Runtime, record: *mut JSObject) -> Option<i32> {
        unsafe { &*runtime.raw_cx() }.with_scope(|scope| unsafe { crate::cell::cell_module(scope, record.cast()) }?.script_id())
    }

    fn collect(runtime: &mut Runtime) {
        runtime.gc_for_testing();
        for _ in 0..3 {
            unsafe { &mut *(*runtime.raw_cx()).isolate }.low_memory_notification();
            runtime.gc_for_testing();
        }
    }

    fn take_importers() -> Vec<i32> {
        IMPORTERS.with(|importers| std::mem::take(&mut *importers.borrow_mut()))
    }

    /// Each realm's `ns.owner()` and the importer of `ns.load()`.
    fn observe(runtime: &Runtime, globals: [*mut JSObject; 2]) -> (Vec<i32>, Vec<i32>) {
        let cx = runtime.raw_cx();
        let mut owners = Vec::new();
        for global in globals {
            let old = unsafe { EnterRealm(cx, global) };
            owners.push(eval(runtime, "ns.owner()").to_int32());
            eval(runtime, "ns.load(); 0");
            unsafe { LeaveRealm(cx, old) };
        }
        (owners, take_importers())
    }

    #[test]
    fn identical_module_sources_keep_their_own_private_across_realms() {
        let owners = [Rc::new(11), Rc::new(22)];
        {
            let mut runtime = runtime();
            let cx = runtime.raw_cx();
            let new_global = || unsafe {
                JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
            };
            rooted!(in(cx) let left = new_global());
            rooted!(in(cx) let right = new_global());
            let globals = [left.get(), right.get()];
            let mut ids = Vec::new();
            for (global, owner) in globals.into_iter().zip(&owners) {
                let old = unsafe { EnterRealm(cx, global) };
                install(&runtime, c"readPrivate", Some(read_private));
                let record = load_module(&runtime, SHARED_SOURCE, owner, c"ns");
                ids.push(script_id(&runtime, record));
                unsafe { LeaveRealm(cx, old) };
            }
            assert!(ids[0].is_some() && ids[0] != ids[1], "identical sources must not share a cached script: {ids:?}");
            assert_eq!(observe(&runtime, globals), (vec![11, 22], vec![11, 22]));
            collect(&mut runtime);
            assert_eq!(observe(&runtime, globals), (vec![11, 22], vec![11, 22]), "after GC");
            assert_eq!((Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1])), (2, 2));
        }
        assert_eq!((Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1])), (1, 1), "shutdown releases both");
    }

    #[test]
    fn colliding_identity_hashes_select_the_module_by_script_id() {
        let owners = [Rc::new(11), Rc::new(22)];
        {
            let mut runtime = runtime();
            install(&runtime, c"readPrivate", Some(read_private));
            let first = load_module(&runtime, SHARED_SOURCE, &owners[0], c"first");
            let second = load_module(&runtime, SHARED_SOURCE, &owners[1], c"second");
            // The second module now precedes the first in the first's bucket; hash-based
            // `with_entry` lookups (privates, import.meta) are not used for it afterwards.
            let bucket = unsafe { &*runtime.raw_cx() }.with_scope(|scope| crate::modules_impl::collide_for_testing(scope, first, second));
            assert_eq!(bucket, 2);
            let check = |runtime: &Runtime| {
                let owners = [eval(runtime, "first.owner()").to_int32(), eval(runtime, "second.owner()").to_int32()];
                eval(runtime, "first.load(); second.load(); 0");
                (owners, take_importers())
            };
            assert_eq!(check(&runtime), ([11, 22], vec![11, 22]));
            collect(&mut runtime);
            assert_eq!(check(&runtime), ([11, 22], vec![11, 22]), "after GC");
        }
        assert_eq!((Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1])), (1, 1));
    }

    #[test]
    fn caller_private_is_the_innermost_classic_or_module_source() {
        let (classic, module) = (Rc::new(33), Rc::new(44));
        {
            let runtime = runtime();
            let cx = runtime.raw_cx();
            rooted!(in(cx) let mut private = UndefinedValue());
            unsafe { JS_GetScriptedCallerPrivate(cx, private.handle_mut().into()) };
            assert!(private.get().is_undefined(), "no script is running");
            install(&runtime, c"readPrivate", Some(read_private));
            install(&runtime, c"readHidden", Some(read_hidden));
            install(&runtime, c"reenter", Some(reenter));
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("classic-caller.js").unwrap(), 1);
            let mut source = transform_str_to_source_text(
                "globalThis.classicOwner = function() { return readPrivate() };\
                 globalThis.classicLoad = function() { return import('owner.js') };\
                 globalThis.classicToModule = function() { return mod.owner() };",
            );
            rooted!(in(cx) let script = unsafe { Compile1(cx, options.ptr, &mut source) });
            unsafe { SetScriptPrivate(script.get(), &PrivateValue(Rc::as_ptr(&classic).cast())) };
            rooted!(in(cx) let mut result = UndefinedValue());
            assert!(unsafe { JS_ExecuteScript(cx, script.handle().into(), result.handle_mut().into()) });
            load_module(
                &runtime,
                "export function owner() { return readPrivate(); }\n\
                 export function hidden() { return readHidden(); }\n\
                 export function viaClassic() { return classicOwner(); }\n\
                 export function viaNative() { return reenter() * 100 + readPrivate(); }\n\
                 export function load() { return import('owner.js'); }",
                &module,
                c"mod",
            );
            let read = |source: &str| eval(&runtime, source).to_int32();
            assert_eq!(read("readPrivate()"), -1, "a script without a private");
            assert_eq!(read("classicOwner()"), 33);
            assert_eq!(read("mod.owner()"), 44);
            assert_eq!(read("mod.hidden()"), 44, "HideScriptedCaller does not hide the private");
            assert_eq!(read("mod.viaClassic()"), 33);
            assert_eq!(read("classicToModule()"), 44);
            assert_eq!(read("mod.viaNative()"), 3344, "native re-entry sees the innermost source");
            eval(&runtime, "mod.load(); classicLoad(); 0");
            assert_eq!(take_importers(), vec![44, 33]);
        }
        assert_eq!((Rc::strong_count(&classic), Rc::strong_count(&module)), (1, 1));
    }

    /// `callerGlobal()`: `GetScriptedCallerGlobal`, or null.
    unsafe extern "C" fn caller_global(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
        let global = unsafe { GetScriptedCallerGlobal(cx) };
        unsafe { *vp = if global.is_null() { NullValue() } else { ObjectValue(global) } };
        true
    }

    /// `hiddenThen(f)`: under `HideScriptedCaller`, checks that the caller is hidden, then
    /// returns `f()`.
    unsafe extern "C" fn hidden_then(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        let args = unsafe { CallArgs::from_vp(vp, argc) };
        rooted!(in(cx) let callee = args.get(0).get());
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let mut result = UndefinedValue());
        let no_arguments = HandleValueArray { length_: 0, elements_: ptr::null() };
        let ok = unsafe {
            HideScriptedCaller(cx);
            assert!(GetScriptedCallerGlobal(cx).is_null(), "the calling script is hidden");
            let ok = Call(cx, this.handle().into(), callee.handle().into(), &no_arguments, result.handle_mut().into());
            UnhideScriptedCaller(cx);
            ok
        };
        args.rval().set(result.get());
        ok
    }

    #[test]
    fn scripted_caller_global_is_the_innermost_frame_realm() {
        let runtime = runtime();
        let cx = runtime.raw_cx();
        let new_global = || unsafe {
            JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        };
        rooted!(in(cx) let left = new_global());
        rooted!(in(cx) let right = new_global());
        for (global, other) in [(left.get(), right.get()), (right.get(), left.get())] {
            let old = unsafe { EnterRealm(cx, global) };
            install(&runtime, c"callerGlobal", Some(caller_global));
            install(&runtime, c"hiddenThen", Some(hidden_then));
            set_global(&runtime, c"other", ObjectValue(other));
            eval(&runtime, "globalThis.fromHere = () => callerGlobal(); 0");
            unsafe { LeaveRealm(cx, old) };
        }
        let old = unsafe { EnterRealm(cx, right.get()) };
        let check = |source: &str| assert!(eval(&runtime, source).to_boolean(), "{source}");
        check("callerGlobal() === globalThis");
        // The entered realm is this one; the innermost frame runs in the other.
        check("other.fromHere() === other");
        // Script started under a hidden caller is visible again.
        check("hiddenThen(() => callerGlobal() === globalThis)");
        check("hiddenThen(() => other.fromHere() === other)");
        check("hiddenThen(() => hiddenThen(() => callerGlobal() === globalThis))");
        check("callerGlobal() === globalThis");
        unsafe { LeaveRealm(cx, old) };
        assert!(unsafe { GetScriptedCallerGlobal(cx) }.is_null());
    }

    #[test]
    fn dynamic_functions_import_from_their_calling_module() {
        let owners = [Rc::new(11), Rc::new(22)];
        {
            let runtime = runtime();
            // Each body names its module: V8's eval cache keys on source, so an identical body
            // reuses the first compilation's origin, exactly as V8's own constructors do.
            let source = |tag: &str| format!(
                "const proto = Object.getPrototypeOf;\n\
                 export function load() {{\n\
                   Function(\"return import('{tag}0.js')\")();\n\
                   new Function('{tag}', \"return import({tag})\")('{tag}1.js');\n\
                   proto(function*(){{}}).constructor(\"yield import('{tag}2.js')\")().next();\n\
                   proto(async function(){{}}).constructor(\"return import('{tag}3.js')\")();\n\
                   new (class extends Function {{}})(\"return import('{tag}4.js')\")();\n\
                 }}"
            );
            load_module(&runtime, &source("a"), &owners[0], c"first");
            load_module(&runtime, &source("b"), &owners[1], c"second");
            eval(&runtime, "first.load(); second.load(); first.load(); 0");
            assert_eq!(take_importers(), [[11; 5], [22; 5], [11; 5]].concat());
        }
        assert_eq!((Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1])), (1, 1));
    }

    /// Identical code and name for every module of the eval tests. Eval code gets its own
    /// V8 script: only its origin's host options identify the owner.
    const EVAL_SOURCE: &str = "export function evalOwner() { return eval('readPrivate()'); }\n\
         export function evalLoad() { return eval(\"import('leaf.js')\"); }\n\
         export function nestedLoad() { return eval(\"eval(\\\"import('leaf.js')\\\")\"); }\n\
         export function evalArrow() { return eval(\"() => import('leaf.js')\"); }\n\
         export async function asyncLoad() { await null; return eval(\"import('leaf.js')\"); }\n\
         export function thenLoad() { return Promise.resolve().then(() => eval(\"import('leaf.js')\")); }\n\
         export function viaNative() { return reenterImport(); }";

    /// Every eval case of `EVAL_SOURCE` in the current realm: the eval's owner and the
    /// importers, in order: eval, nested eval, escaped arrow, async, microtask, native re-entry.
    fn observe_eval(runtime: &Runtime) -> (i32, Vec<i32>) {
        let cx = runtime.raw_cx();
        let owner = eval(runtime, "ns.evalOwner()").to_int32();
        eval(runtime, "ns.evalLoad(); ns.nestedLoad(); ns.evalArrow()(); ns.asyncLoad(); ns.thenLoad(); 0");
        unsafe { RunJobs(cx) };
        eval(runtime, "ns.viaNative(); 0");
        (owner, take_importers())
    }

    #[test]
    fn direct_eval_imports_keep_their_source_owner() {
        let owners = [Rc::new(11), Rc::new(22)];
        let classic = Rc::new(33);
        {
            let mut runtime = runtime();
            let cx = runtime.raw_cx();
            let new_global = || unsafe {
                JS_NewGlobalObject(cx, &GLOBAL_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
            };
            rooted!(in(cx) let left = new_global());
            rooted!(in(cx) let right = new_global());
            let globals = [left.get(), right.get()];
            for (global, owner) in globals.into_iter().zip(&owners) {
                let old = unsafe { EnterRealm(cx, global) };
                install(&runtime, c"readPrivate", Some(read_private));
                install(&runtime, c"reenterImport", Some(reenter_import));
                run_classic(
                    &runtime,
                    "classic-eval.js",
                    "globalThis.classicEvalOwner = function() { return eval('readPrivate()') };\
                     globalThis.classicEvalLoad = function() { return eval(\"import('leaf.js')\") };\
                     globalThis.classicEvalArrow = eval(\"() => import('leaf.js')\");\
                     globalThis.classicToModule = function() { return ns.evalLoad() };",
                    Some(&classic),
                );
                load_module(&runtime, EVAL_SOURCE, owner, c"ns");
                // Only the module itself may keep an eval arrow alive across the collection.
                eval(&runtime, "globalThis.keptArrow = ns.evalArrow(); 0");
                unsafe { LeaveRealm(cx, old) };
            }
            let check = |runtime: &Runtime| {
                let mut seen = Vec::new();
                for global in globals {
                    let old = unsafe { EnterRealm(cx, global) };
                    let module = observe_eval(runtime);
                    let classic_owner = eval(runtime, "classicEvalOwner()").to_int32();
                    eval(runtime, "classicEvalLoad(); classicEvalArrow(); classicToModule(); keptArrow(); 0");
                    seen.push((module, classic_owner, take_importers()));
                    unsafe { LeaveRealm(cx, old) };
                }
                seen
            };
            let expected = vec![
                ((11, vec![11, 11, 11, 11, 11, 33]), 33, vec![33, 33, 11, 11]),
                ((22, vec![22, 22, 22, 22, 22, 33]), 33, vec![33, 33, 22, 22]),
            ];
            assert_eq!(check(&runtime), expected);
            collect(&mut runtime);
            assert_eq!(check(&runtime), expected, "after GC");
            assert_eq!(
                (Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1]), Rc::strong_count(&classic)),
                (2, 2, 3),
                "one reference per source; eval code adds none"
            );
        }
        assert_eq!((Rc::strong_count(&owners[0]), Rc::strong_count(&owners[1]), Rc::strong_count(&classic)), (1, 1, 1));
    }

    /// A metadata lookup by host token runs no user code; a source with a token but no
    /// private never takes another source's.
    #[test]
    fn host_metadata_ignores_poisoned_weakmaps_and_unowned_sources() {
        let (classic, module) = (Rc::new(33), Rc::new(44));
        {
            let mut runtime = runtime();
            install(&runtime, c"readPrivate", Some(read_private));
            install(&runtime, c"reenterImport", Some(reenter_import));
            eval(
                &runtime,
                "const poison = () => { throw new Error('poisoned') };\
                 WeakMap.prototype.get = poison; WeakMap.prototype.set = poison; WeakMap.prototype.has = poison;\
                 globalThis.WeakMap = poison; 0",
            );
            run_classic(
                &runtime,
                "classic-eval.js",
                "globalThis.classicEvalOwner = function() { return eval('readPrivate()') };\
                 globalThis.classicEvalLoad = function() { return eval(\"import('leaf.js')\") };",
                Some(&classic),
            );
            // Explicitly cleared: the token exists, its private is undefined.
            let unowned = run_classic(
                &runtime,
                "unowned.js",
                "globalThis.unownedOwner = function() { return eval('readPrivate()') };\
                 globalThis.unownedLoad = function() { return eval(\"import('leaf.js')\") };",
                Some(&classic),
            );
            unsafe { SetScriptPrivate(unowned, &UndefinedValue()) };
            load_module(
                &runtime,
                "export function owner() { return eval('readPrivate()'); }\n\
                 export function load() { return eval(\"import('leaf.js')\"); }\n\
                 export function viaNative() { return reenterImport(); }\n\
                 export function viaUnowned() { return unownedOwner() * 100 + readPrivate(); }\n\
                 export function viaUnownedLoad() { return unownedLoad(); }",
                &module,
                c"mod",
            );
            let check = |runtime: &Runtime| {
                let owners = ["classicEvalOwner()", "mod.owner()", "unownedOwner()", "mod.viaUnowned()"].map(|code| eval(runtime, code).to_int32());
                eval(runtime, "classicEvalLoad(); mod.load(); mod.viaNative(); unownedLoad(); mod.viaUnownedLoad(); 0");
                (owners, take_importers())
            };
            let expected = ([33, 44, -1, -56], vec![33, 44, 33, -1, -1]);
            assert_eq!(check(&runtime), expected);
            collect(&mut runtime);
            assert_eq!(check(&runtime), expected, "after GC");
            assert_eq!((Rc::strong_count(&classic), Rc::strong_count(&module)), (2, 2), "the cleared source released its reference");
        }
        assert_eq!((Rc::strong_count(&classic), Rc::strong_count(&module)), (1, 1));
    }

    /// The V8 API no longer reports an errored module's source; its record keeps the private.
    #[test]
    fn errored_module_records_keep_their_private() {
        let owner = Rc::new(55);
        {
            let mut runtime = runtime();
            let cx = runtime.raw_cx();
            let options = CompileOptionsWrapper::new(runtime.cx_no_gc(), CString::new("errored.js").unwrap(), 1);
            let mut text = transform_str_to_source_text("export function lost() { return 1; }\nthrow new Error('boom');");
            rooted!(in(cx) let record = unsafe { CompileModule1(cx, options.ptr, &mut text) });
            assert!(!record.get().is_null());
            unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            assert!(unsafe { ModuleLink(cx, record.handle().into()) });
            rooted!(in(cx) let mut evaluation = UndefinedValue());
            assert!(unsafe { ModuleEvaluate(cx, record.handle().into(), evaluation.handle_mut().into()) });
            unsafe { RunJobs(cx) };
            rooted!(in(cx) let evaluation = evaluation.get().to_object());
            assert!(!unsafe { ThrowOnModuleEvaluationFailure(cx, evaluation.handle().into(), ModuleErrorBehaviour::ThrowModuleErrorsSync) });
            unsafe { crate::api::JS_ClearPendingException(cx) };
            assert_eq!(script_id(&runtime, record.get()), None, "no script id for an errored module");
            let read = |record: *mut JSObject| {
                rooted!(in(cx) let mut private = UndefinedValue());
                unsafe { crate::glue::JS_GetModulePrivate(record, private.handle_mut().into()) };
                self::owner(private.get())
            };
            collect(&mut runtime);
            assert_eq!(read(record.get()), 55, "the record keeps the private through GC");
            assert_eq!(Rc::strong_count(&owner), 2);
            unsafe { SetModulePrivate(record.get(), &UndefinedValue()) };
            assert_eq!((read(record.get()), Rc::strong_count(&owner)), (-1, 1));
            unsafe { SetModulePrivate(record.get(), &PrivateValue(Rc::as_ptr(&owner).cast())) };
            collect(&mut runtime);
            assert_eq!((read(record.get()), Rc::strong_count(&owner)), (55, 2));
        }
        assert_eq!(Rc::strong_count(&owner), 1, "shutdown releases the errored module's private once");
    }
}

mod structured_clone {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;

    use super::{describe, eval};
    use crate::jsapi::*;
    use crate::jsval::*;
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
    fn structured_clone_storage_moves_without_copying() {
        let bytes = vec![1, 2, 3, 4];
        let pointer = bytes.as_ptr();
        let capacity = bytes.capacity();
        let mut data: JSStructuredCloneData = bytes.into();
        let bytes: Vec<u8> = std::mem::take(&mut data).into();
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(bytes.capacity(), capacity);
        assert!(Vec::<u8>::from(data).is_empty());
        let received: JSStructuredCloneData = bytes.into();
        let bytes: Vec<u8> = received.into();
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(bytes, [1, 2, 3, 4]);
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
        let bytes: Vec<u8> = std::mem::take(data).into();
        let received = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
        let received_data = unsafe { &mut (*received.as_raw_ptr()).data_ };
        *received_data = bytes.into();
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

        // Shared memory never crosses, as Servo's policy (no shared memory objects) requires,
        // and the refusal is reported like any other.
        rooted!(in(cx) let shared = eval(&runtime, "({ shared: new SharedArrayBuffer(8) })"));
        let refused = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
        let refused_data = unsafe { &mut (*refused.as_raw_ptr()).data_ };
        assert!(!unsafe {
            JS_WriteStructuredClone(cx, shared.handle().into(), refused_data, StructuredCloneScope::DifferentProcess, &policy, &CALLBACKS, ptr::null_mut(), no_transfer.handle().into())
        });
        assert_eq!(REPORTED.with(Cell::get), 2);
        assert!(!unsafe { crate::api::JS_IsExceptionPending(cx) });

        // Likewise for a WebAssembly.Module (unavailable in JIT-less builds).
        if eval(&runtime, "typeof WebAssembly === 'object'").to_boolean() {
            rooted!(in(cx) let module = eval(&runtime, "new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]))"));
            let module_buffer = unsafe { JSAutoStructuredCloneBufferWrapper::new(StructuredCloneScope::DifferentProcess, &CALLBACKS) };
            let module_data = unsafe { &mut (*module_buffer.as_raw_ptr()).data_ };
            assert!(!unsafe {
                JS_WriteStructuredClone(cx, module.handle().into(), module_data, StructuredCloneScope::DifferentProcess, &policy, &CALLBACKS, ptr::null_mut(), no_transfer.handle().into())
            });
            assert_eq!(REPORTED.with(Cell::get), 3);
            assert!(!unsafe { crate::api::JS_IsExceptionPending(cx) });
        }
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

    thread_local! {
        static WINDOW_OWNERS_FINALIZED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    /// Servo's navigation sequence on top of the proxy takeover (`WindowProxy::set_window`):
    /// a new window proxy, the transplant, `SetWindowProxy`, then the conversions it relies on.
    #[test]
    fn servo_navigation_sequence_keeps_window_proxy_conversions_consistent() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let new_window = || unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        let window_proxy_for = |window: HandleObject| {
            let old = unsafe { EnterRealm(cx, window.get()) };
            let proxy = unsafe { NewWindowProxy(cx, window, handler) };
            unsafe { LeaveRealm(cx, old) };
            proxy
        };
        rooted!(in(cx) let first = new_window());
        rooted!(in(cx) let first_proxy = window_proxy_for(first.handle().into()));
        unsafe { SetWindowProxy(cx, first.handle().into(), first_proxy.handle().into()) };
        rooted!(in(cx) let mut held = ObjectValue(first_proxy.get()));
        assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });

        unsafe { RovesReuseWindowProxyForNextGlobal(cx, first.get()) };
        rooted!(in(cx) let second = new_window());
        rooted!(in(cx) let second_proxy = window_proxy_for(second.handle().into()));
        rooted!(in(cx) let transplanted = unsafe { JS_TransplantObject(cx, first_proxy.handle().into(), second_proxy.handle().into()) });
        assert_eq!(transplanted.get(), second_proxy.get(), "Servo adopts the replacement as its reflector");
        unsafe { SetWindowProxy(cx, second.handle().into(), transplanted.handle().into()) };

        let old = unsafe { EnterRealm(cx, second.get()) };
        eval(&runtime, "globalThis.page = 'second'; 0");
        rooted!(in(cx) let inside = eval(&runtime, "globalThis").to_object());
        unsafe { LeaveRealm(cx, old) };
        assert_eq!(inside.get(), transplanted.get(), "scripts see the window proxy as globalThis");
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "second");
        // Another realm sees one wrapper per window proxy, the same before and after.
        rooted!(in(cx) let seen = eval(&runtime, "held").to_object());
        rooted!(in(cx) let mut current = ObjectValue(transplanted.get()));
        assert!(unsafe { JS_WrapValue(cx, current.handle_mut().into()) });
        assert_eq!(seen.get(), current.get().to_object(), "the held wrapper is the current window proxy's wrapper");
        runtime.gc_for_testing();
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "second", "after a collection too");
        unsafe {
            assert_eq!(ToWindowIfWindowProxy(transplanted.get()), second.get());
            assert_eq!(ToWindowProxyIfWindowSlow(second.get()), transplanted.get());
            assert_eq!(GetNonCCWObjectGlobal(second.get()), second.get());
            assert_eq!(GetNonCCWObjectGlobal(first.get()), first.get());
            assert!(IsWindowProxy(transplanted.get()));
        }
    }

    /// Repeated navigations (A, B, C) with a collection between them: references held in another
    /// realm follow the whole chain of replaced window proxies.
    #[test]
    fn held_window_wrappers_follow_repeated_navigations() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        rooted!(in(cx) let mut window = unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) });
        let old = unsafe { EnterRealm(cx, window.get()) };
        rooted!(in(cx) let mut proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        unsafe { LeaveRealm(cx, old) };
        unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
        rooted!(in(cx) let mut held = ObjectValue(proxy.get()));
        assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });
        for page in ["b", "c"] {
            unsafe { RovesReuseWindowProxyForNextGlobal(cx, window.get()) };
            window.set(unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) });
            let old = unsafe { EnterRealm(cx, window.get()) };
            rooted!(in(cx) let next = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
            eval(&runtime, &format!("globalThis.page = '{page}'; 0"));
            unsafe { LeaveRealm(cx, old) };
            proxy.set(unsafe { JS_TransplantObject(cx, proxy.handle().into(), next.handle().into()) });
            unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
            runtime.gc_for_testing();
            assert_eq!(describe(&runtime, eval(&runtime, "held.page")), page);
        }
    }

    /// A navigation to another origin (another script thread) replaces the window proxy with a
    /// real proxy over Servo's DissimilarOriginWindow: wrappers other realms hold switch to it,
    /// so they get its cross-origin traps instead of the old same-origin window.
    #[test]
    fn held_window_wrappers_take_the_cross_origin_replacement() {
        unsafe extern "C" fn cross_origin_get(_cx: *mut JSContext, _proxy: HandleObject, _receiver: HandleValue, _id: HandleId, mut vp: MutableHandleValue) -> bool {
            vp.set(Int32Value(99));
            true
        }
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let mut cross_origin_traps: ProxyTraps = unsafe { std::mem::zeroed() };
        cross_origin_traps.get = Some(cross_origin_get);
        let cross_origin_handler = unsafe { CreateWrapperProxyHandler(&cross_origin_traps) };
        rooted!(in(cx) let window = unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) });
        let old = unsafe { EnterRealm(cx, window.get()) };
        rooted!(in(cx) let window_proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        unsafe { LeaveRealm(cx, old) };
        unsafe { SetWindowProxy(cx, window.handle().into(), window_proxy.handle().into()) };
        let old = unsafe { EnterRealm(cx, window.get()) };
        eval(&runtime, "globalThis.page = 'same-origin'; 0");
        unsafe { LeaveRealm(cx, old) };
        rooted!(in(cx) let mut held = ObjectValue(window_proxy.get()));
        assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "same-origin");

        // Servo's `unset_currently_active`: a DissimilarOriginWindow behind a real proxy.
        rooted!(in(cx) let dissimilar = unsafe { JS_NewPlainObject(cx) });
        rooted!(in(cx) let replacement = unsafe { NewWindowProxy(cx, dissimilar.handle().into(), cross_origin_handler) });
        rooted!(in(cx) let transplanted = unsafe { JS_TransplantObject(cx, window_proxy.handle().into(), replacement.handle().into()) });
        assert_eq!(transplanted.get(), replacement.get());
        assert_eq!(eval(&runtime, "held.page").to_int32(), 99, "the held wrapper uses the cross-origin traps");
    }

    /// Servo's `set_window` in a window's realm: a new window proxy, the transplant of the
    /// previous one (if any), then `SetWindowProxy`. Returns the browsing context's new proxy.
    unsafe fn navigate(cx: *mut JSContext, previous: HandleObject, window: HandleObject, handler: *const std::ffi::c_void) -> *mut JSObject {
        unsafe { navigate_in(cx, window.get(), previous, window, handler) }
    }

    /// [`navigate`] with the new window proxy made in the realm of `realm`.
    unsafe fn navigate_in(cx: *mut JSContext, realm: *mut JSObject, previous: HandleObject, window: HandleObject, handler: *const std::ffi::c_void) -> *mut JSObject {
        let old = unsafe { EnterRealm(cx, realm) };
        rooted!(in(cx) let next = unsafe { NewWindowProxy(cx, window, handler) });
        rooted!(in(cx) let current = unsafe { JS_TransplantObject(cx, previous, next.handle().into()) });
        unsafe { SetWindowProxy(cx, window, current.handle().into()) };
        unsafe { LeaveRealm(cx, old) };
        current.get()
    }

    static REMOTE: JSClass = JSClass { name: c"DissimilarOriginWindow".as_ptr(), ..WINDOW_CLASS };

    /// The cross-origin `get`: `self`/`window` are the window proxy and `parent` the window
    /// in reserved slot 0, converted for the caller's realm; anything else is denied.
    unsafe extern "C" fn remote_get(cx: *mut JSContext, proxy: HandleObject, _receiver: HandleValue, id: HandleId, mut vp: MutableHandleValue) -> bool {
        let id_value = id.get();
        let mut length = 0;
        let chars = if id_value.is_string() { crate::api::latin1_chars(id_value.to_string(), &mut length) } else { ptr::null() };
        let name = if chars.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(chars, length) } };
        let mut window = UndefinedValue();
        unsafe { GetProxyPrivate(proxy.get(), &mut window) };
        match name {
            b"self" | b"window" => vp.set(ObjectValue(unsafe { ToWindowProxyIfWindowSlow(window.to_object()) })),
            b"parent" => {
                let mut parent = UndefinedValue();
                unsafe { JS_GetReservedSlot(window.to_object(), 0, &mut parent) };
                vp.set(parent);
            },
            _ => {
                let message = unsafe { JS_NewStringCopyN(cx, c"SecurityError".as_ptr(), 13) };
                rooted!(in(cx) let error = StringValue(unsafe { &*message }));
                unsafe { crate::api::JS_SetPendingException(cx, error.handle().into(), crate::api::ExceptionStackBehavior::DoNotCapture) };
                return false;
            },
        }
        unsafe { JS_WrapValue(cx, vp) }
    }

    /// Servo's local → cross-origin → local sequence (`unset_currently_active`, then
    /// `set_currently_active` with a fresh Window): another realm keeps one wrapper for the
    /// browsing context. `contentWindow`/`event.source`-like conversions and the cross-origin
    /// allowlist getters return it in every phase, and the replaced windows' realms collect
    /// while it is held.
    #[test]
    fn foreign_window_wrappers_keep_identity_across_cross_origin_round_trips() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let mut remote_traps: ProxyTraps = unsafe { std::mem::zeroed() };
        remote_traps.get = Some(remote_get);
        let remote_handler = unsafe { CreateWrapperProxyHandler(&remote_traps) };
        let new_window = |class: &JSClass| unsafe { JS_NewGlobalObject(cx, class, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        let wrapped = move |object: *mut JSObject| {
            rooted!(in(cx) let mut value = ObjectValue(object));
            assert!(unsafe { JS_WrapValue(cx, value.handle_mut().into()) });
            value.get().to_object()
        };
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let mut window = new_window(&WINDOW_CLASS));
        let old = unsafe { EnterRealm(cx, window.get()) };
        rooted!(in(cx) let mut proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        eval(&runtime, "globalThis.page = 'first'; 0");
        unsafe { LeaveRealm(cx, old) };
        unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
        rooted!(in(cx) let held = ObjectValue(wrapped(proxy.get())));
        assert_ne!(held.get().to_object(), proxy.get(), "another realm sees a wrapper");
        assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });
        let mut replaced = vec![crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) })];
        for round in 0..2 {
            {
                // The previous window proxy, as a stale native reference would keep it.
                rooted!(in(cx) let previous = proxy.get());
                window.set(new_window(&REMOTE));
                unsafe { JS_SetReservedSlot(window.get(), 0, &ObjectValue(observer.get())) };
                proxy.set(unsafe { navigate(cx, previous.handle().into(), window.handle().into(), remote_handler) });
                assert_ne!(proxy.get(), previous.get(), "Servo adopts the cross-origin replacement");
                assert_eq!(wrapped(proxy.get()), held.get().to_object(), "round {round}: contentWindow while cross-origin");
                assert_eq!(wrapped(window.get()), held.get().to_object(), "round {round}: a remote message source");
                assert_eq!(wrapped(previous.get()), held.get().to_object(), "round {round}: a stale reference");
            }
            replaced.push(crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) }));
            assert!(eval(&runtime, "held.self === held && held.window === held && held.parent === globalThis").to_boolean(), "round {round}: the allowlist");
            assert!(
                eval(&runtime, "(() => { try { held.document; return false; } catch (e) { return e === 'SecurityError'; } })()").to_boolean(),
                "round {round}: other properties stay denied",
            );
            runtime.gc_for_testing();
            {
                rooted!(in(cx) let previous = proxy.get());
                window.set(new_window(&WINDOW_CLASS));
                let old = unsafe { EnterRealm(cx, window.get()) };
                eval(&runtime, &format!("globalThis.page = 'local {round}'; 0"));
                unsafe { LeaveRealm(cx, old) };
                proxy.set(unsafe { navigate(cx, previous.handle().into(), window.handle().into(), handler) });
                assert_eq!(wrapped(proxy.get()), held.get().to_object(), "round {round}: contentWindow back on the same origin");
                assert_eq!(wrapped(window.get()), held.get().to_object(), "round {round}: a local message source");
                assert_eq!(wrapped(previous.get()), held.get().to_object(), "round {round}: a stale remote reference");
            }
            replaced.push(crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) }));
            assert_eq!(describe(&runtime, eval(&runtime, "held.page")), format!("local {round}"));
            let old = unsafe { EnterRealm(cx, window.get()) };
            assert_eq!(eval(&runtime, "globalThis").to_object(), proxy.get(), "round {round}: window === globalThis in its own realm");
            unsafe { LeaveRealm(cx, old) };
            runtime.gc_for_testing();
        }
        for _ in 0..3 {
            runtime.gc_for_testing();
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
        }
        let current = replaced.pop().unwrap();
        assert!(current.get().is_some(), "the current window lives");
        for (index, realm) in replaced.iter().enumerate() {
            assert!(realm.get().is_none(), "replaced window {index} collects while the wrapper is held");
        }
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "local 1", "the held wrapper roots the current window");
        assert_eq!(wrapped(proxy.get()), held.get().to_object());
    }

    /// A remote window proxy made in the realm that receives it (JSAPI allows any realm; Servo
    /// enters the DissimilarOriginWindow's own). Only a local window's own realm sees the proxy
    /// itself (`window === globalThis`): the receiving realm keeps one wrapper whether it saw
    /// the context first as remote or as local, across remote ↔ local and remote → remote.
    fn remote_proxy_made_in_the_receiving_realm(remote_first: bool) {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let mut remote_traps: ProxyTraps = unsafe { std::mem::zeroed() };
        remote_traps.get = Some(remote_get);
        let remote_handler = unsafe { CreateWrapperProxyHandler(&remote_traps) };
        let wrapped = move |object: *mut JSObject| {
            rooted!(in(cx) let mut value = ObjectValue(object));
            assert!(unsafe { JS_WrapValue(cx, value.handle_mut().into()) });
            value.get().to_object()
        };
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let mut window = ptr::null_mut::<JSObject>());
        rooted!(in(cx) let mut proxy = ptr::null_mut::<JSObject>());
        rooted!(in(cx) let mut held = UndefinedValue());
        let phases: [bool; 4] = if remote_first { [true, false, true, true] } else { [false, true, true, false] };
        let mut replaced = Vec::new();
        for (phase, remote) in phases.into_iter().enumerate() {
            rooted!(in(cx) let previous = proxy.get());
            window.set(unsafe { JS_NewGlobalObject(cx, if remote { &REMOTE } else { &WINDOW_CLASS }, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) });
            if remote {
                unsafe { JS_SetReservedSlot(window.get(), 0, &ObjectValue(observer.get())) };
            } else {
                let old = unsafe { EnterRealm(cx, window.get()) };
                eval(&runtime, &format!("globalThis.page = 'local {phase}'; 0"));
                unsafe { LeaveRealm(cx, old) };
            }
            let (realm, phase_handler) = if remote { (observer.get(), remote_handler) } else { (window.get(), handler) };
            if previous.get().is_null() {
                let old = unsafe { EnterRealm(cx, realm) };
                proxy.set(unsafe { NewWindowProxy(cx, window.handle().into(), phase_handler) });
                unsafe { LeaveRealm(cx, old) };
                unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
                held.set(ObjectValue(wrapped(proxy.get())));
                assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });
            } else {
                proxy.set(unsafe { navigate_in(cx, realm, previous.handle().into(), window.handle().into(), phase_handler) });
                assert_ne!(proxy.get(), previous.get(), "phase {phase}: Servo adopts the replacement");
                assert_eq!(wrapped(previous.get()), held.get().to_object(), "phase {phase}: a stale reference");
            }
            if remote {
                assert_eq!(unsafe { crate::realm_impl::realm_of_object(&*cx, proxy.get()) }, unsafe { GetObjectRealmOrNull(observer.get()) }, "phase {phase}: the remote proxy is the receiving realm's");
            }
            assert_eq!(wrapped(proxy.get()), held.get().to_object(), "phase {phase}: contentWindow");
            assert_eq!(wrapped(window.get()), held.get().to_object(), "phase {phase}: a message source");
            if remote {
                assert!(eval(&runtime, "held.self === held && held.window === held && held.parent === globalThis").to_boolean(), "phase {phase}: the allowlist getters");
            } else {
                assert_eq!(describe(&runtime, eval(&runtime, "held.page")), format!("local {phase}"));
                let old = unsafe { EnterRealm(cx, window.get()) };
                assert_eq!(eval(&runtime, "globalThis").to_object(), proxy.get(), "phase {phase}: window === globalThis in its own realm");
                unsafe { LeaveRealm(cx, old) };
            }
            replaced.push(crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) }));
            runtime.gc_for_testing();
        }
        for _ in 0..3 {
            runtime.gc_for_testing();
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
        }
        assert!(replaced.pop().unwrap().get().is_some(), "the current window lives");
        for (index, realm) in replaced.iter().enumerate() {
            assert!(realm.get().is_none(), "replaced window {index} collects while the wrapper is held");
        }
        assert_eq!(wrapped(proxy.get()), held.get().to_object());
    }

    #[test]
    fn a_remote_first_proxy_made_in_the_receiving_realm_keeps_one_identity() {
        remote_proxy_made_in_the_receiving_realm(true);
    }

    #[test]
    fn a_local_first_context_with_remote_proxies_made_in_the_receiving_realm_keeps_one_identity() {
        remote_proxy_made_in_the_receiving_realm(false);
    }

    /// The current realm's cross-origin holder for `proxy` (null if none).
    fn holder_of(cx: *mut JSContext, proxy: *mut JSObject) -> *mut JSObject {
        rooted!(in(cx) let proxy = proxy);
        rooted!(in(cx) let mut holder = ptr::null_mut::<JSObject>());
        assert!(unsafe { crate::proxy::GetCrossOriginPropertyHolder(cx, proxy.handle().into(), holder.handle_mut().into()) });
        holder.get()
    }

    /// Makes and stores a holder in the current realm, as Servo's
    /// `ensure_cross_origin_property_holder` does; returns it with a weak handle on it.
    fn new_holder(runtime: &Runtime, proxy: *mut JSObject) -> (*mut JSObject, v8::Weak<v8::Object>) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let proxy = proxy);
        rooted!(in(cx) let holder = eval(runtime, "({})").to_object());
        assert!(unsafe { crate::proxy::SetCrossOriginPropertyHolder(cx, proxy.handle().into(), holder.handle().into()) });
        let weak = unsafe { &*cx }.with_scope(|scope| {
            let value = unsafe { crate::cell::cell_value(scope, holder.get() as *mut std::ffi::c_void) };
            v8::Weak::new(scope, v8::Local::<v8::Object>::try_from(value).unwrap())
        });
        (holder.get(), weak)
    }

    /// HTML's [[CrossOriginPropertyDescriptorMap]] is keyed by (current global, object global,
    /// property): a held Window wrapper keeps its identity across navigations, but each new
    /// Window (one that took over the previous V8 global proxy, then a remote one) gets fresh
    /// holders, one per receiving realm. Holders of replaced Windows and their realms collect;
    /// other proxies (Location) keep theirs.
    #[test]
    fn cross_origin_holders_follow_the_current_window() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let new_window = |class: &JSClass| unsafe { JS_NewGlobalObject(cx, class, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        let wrapped = move |object: *mut JSObject| {
            rooted!(in(cx) let mut value = ObjectValue(object));
            assert!(unsafe { JS_WrapValue(cx, value.handle_mut().into()) });
            value.get().to_object()
        };
        rooted!(in(cx) let other_realm = new_window(&WINDOW_CLASS));
        rooted!(in(cx) let location = unsafe { NewProxyObject(cx, handler, HandleValue::undefined(), ptr::null_mut(), ptr::null(), false) });
        let (location_holder, _) = new_holder(&runtime, location.get());
        rooted!(in(cx) let location_holder = location_holder);
        rooted!(in(cx) let mut window = new_window(&WINDOW_CLASS));
        let old = unsafe { EnterRealm(cx, window.get()) };
        rooted!(in(cx) let mut proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        unsafe { LeaveRealm(cx, old) };
        unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
        rooted!(in(cx) let held = wrapped(proxy.get()));
        let old = unsafe { EnterRealm(cx, other_realm.get()) };
        rooted!(in(cx) let held_elsewhere = wrapped(proxy.get()));
        unsafe { LeaveRealm(cx, old) };
        let mut weak_holders = Vec::new();
        let mut replaced = Vec::new();
        for phase in ["first", "reused global proxy", "remote"] {
            if phase != "first" {
                rooted!(in(cx) let previous = proxy.get());
                replaced.push(crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) }));
                let remote = phase == "remote";
                if !remote {
                    unsafe { RovesReuseWindowProxyForNextGlobal(cx, window.get()) };
                }
                window.set(new_window(if remote { &REMOTE } else { &WINDOW_CLASS }));
                proxy.set(unsafe { navigate(cx, previous.handle().into(), window.handle().into(), handler) });
                if !remote {
                    let reused = unsafe { &*cx }.with_scope(|scope| unsafe {
                        let previous = crate::cell::cell_value(scope, previous.get() as *mut std::ffi::c_void);
                        let current = crate::cell::cell_value(scope, proxy.get() as *mut std::ffi::c_void);
                        previous.strict_equals(current)
                    });
                    assert!(reused, "the new owner is the same raw V8 global proxy");
                }
                assert_eq!(wrapped(proxy.get()), held.get(), "{phase}: the wrapper keeps its identity");
                assert!(holder_of(cx, held.get()).is_null(), "{phase}: a new Window gets no stale holder");
            }
            let (holder, weak) = new_holder(&runtime, held.get());
            assert_eq!(holder_of(cx, held.get()), holder, "{phase}: same Window, same realm, same holder");
            let old = unsafe { EnterRealm(cx, other_realm.get()) };
            assert!(holder_of(cx, held_elsewhere.get()).is_null(), "{phase}: another receiving realm has its own holder");
            let (elsewhere, weak_elsewhere) = new_holder(&runtime, held_elsewhere.get());
            assert_ne!(elsewhere, holder);
            assert_eq!(holder_of(cx, held_elsewhere.get()), elsewhere);
            unsafe { LeaveRealm(cx, old) };
            assert_eq!(holder_of(cx, held.get()), holder, "{phase}: unaffected by the other realm");
            weak_holders.push((weak, weak_elsewhere));
        }
        assert_eq!(holder_of(cx, location.get()), location_holder.get(), "other proxies keep their holder");
        for _ in 0..3 {
            runtime.gc_for_testing();
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
        }
        let (current, current_elsewhere) = weak_holders.pop().unwrap();
        assert!(!current.is_empty() && !current_elsewhere.is_empty(), "the current Window's holders live");
        for (index, (weak, weak_elsewhere)) in weak_holders.iter().enumerate() {
            assert!(weak.is_empty() && weak_elsewhere.is_empty(), "holders of replaced Window {index} collect");
        }
        for (index, realm) in replaced.iter().enumerate() {
            assert!(realm.get().is_none(), "replaced window {index} collects");
        }
        assert_eq!(holder_of(cx, location.get()), location_holder.get(), "after a collection too");
    }

    /// A remote → remote navigation (a new DissimilarOriginWindow): the wrapper keeps its
    /// identity and gets a fresh holder; the replaced Window, its proxy's creation realm, and
    /// its holder collect.
    #[test]
    fn cross_origin_holders_follow_a_remote_to_remote_transplant() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let traps: ProxyTraps = unsafe { std::mem::zeroed() };
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        let new_remote = || unsafe { JS_NewGlobalObject(cx, &REMOTE, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        rooted!(in(cx) let mut window = new_remote());
        let old = unsafe { EnterRealm(cx, window.get()) };
        rooted!(in(cx) let mut proxy = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
        unsafe { LeaveRealm(cx, old) };
        unsafe { SetWindowProxy(cx, window.handle().into(), proxy.handle().into()) };
        rooted!(in(cx) let mut held = ObjectValue(proxy.get()));
        assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
        let (first, weak_first) = new_holder(&runtime, held.get().to_object());
        assert_eq!(holder_of(cx, held.get().to_object()), first);
        let replaced = crate::realm_impl::testing::realm_weak(unsafe { GetObjectRealmOrNull(window.get()) });
        window.set(new_remote());
        rooted!(in(cx) let replacement = unsafe { navigate(cx, proxy.handle().into(), window.handle().into(), handler) });
        assert_ne!(replacement.get(), proxy.get(), "Servo adopts the replacement");
        assert_eq!(unsafe { crate::realm_impl::realm_of_object(&*cx, held.get().to_object()) }, unsafe { GetObjectRealmOrNull(window.get()) }, "the wrapper is the new Window's");
        rooted!(in(cx) let mut again = ObjectValue(replacement.get()));
        assert!(unsafe { JS_WrapValue(cx, again.handle_mut().into()) });
        assert_eq!(again.get().to_object(), held.get().to_object(), "the wrapper keeps its identity");
        assert!(holder_of(cx, held.get().to_object()).is_null(), "the new Window gets no stale holder");
        let (second, _) = new_holder(&runtime, held.get().to_object());
        rooted!(in(cx) let second = second);
        assert_eq!(holder_of(cx, held.get().to_object()), second.get());
        proxy.set(replacement.get());
        for _ in 0..3 {
            runtime.gc_for_testing();
            unsafe { &mut *(*cx).isolate }.low_memory_notification();
        }
        assert!(replaced.get().is_none(), "the replaced Window collects");
        assert!(weak_first.is_empty(), "the replaced Window's holder collects");
        assert_eq!(holder_of(cx, held.get().to_object()), second.get(), "the current Window's holder stays cached");
    }

    #[test]
    fn a_window_that_handed_over_its_proxy_is_still_collected() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let new_window = || unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        let before = crate::realm_impl::testing::live_realm_count(unsafe { &*cx });
        rooted!(in(cx) let mut second = ptr::null_mut::<JSObject>());
        {
            rooted!(in(cx) let first = new_window());
            unsafe { RovesReuseWindowProxyForNextGlobal(cx, first.get()) };
            second.set(new_window());
        }
        runtime.gc_for_testing();
        assert_eq!(crate::realm_impl::testing::live_realm_count(unsafe { &*cx }), before + 1, "only the new window's realm survives");
        let old = unsafe { EnterRealm(cx, second.get()) };
        assert_eq!(describe(&runtime, eval(&runtime, "typeof globalThis")), "object");
        unsafe { LeaveRealm(cx, old) };
    }

    #[test]
    fn a_navigated_window_takes_over_the_previous_global_proxy() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let new_window = || unsafe { JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) };
        rooted!(in(cx) let first = new_window());
        let old = unsafe { EnterRealm(cx, first.get()) };
        eval(&runtime, "globalThis.page = 'first'; 0");
        unsafe { JS_SetReservedSlot(first.get(), 0, &Int32Value(7)) };
        unsafe { LeaveRealm(cx, old) };
        // Another realm (a parent page) keeps a reference to the first window.
        rooted!(in(cx) let mut held = ObjectValue(first.get()));
        assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
        rooted!(in(cx) let observer = eval(&runtime, "globalThis").to_object());
        assert!(unsafe { JS_SetProperty(cx, observer.handle().into(), c"held".as_ptr(), held.handle().into()) });
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "first");

        rooted!(in(cx) let second = unsafe { RovesNewWindowGlobalReusingProxy(cx, &WINDOW_CLASS, ptr::null_mut(), ptr::null(), first.get()) });
        assert!(!second.get().is_null());
        assert_ne!(second.get(), first.get(), "a new Window global");
        let old = unsafe { EnterRealm(cx, second.get()) };
        assert_eq!(describe(&runtime, eval(&runtime, "typeof page")), "undefined", "a fresh global");
        eval(&runtime, "globalThis.page = 'second'; 0");
        rooted!(in(cx) let seen = eval(&runtime, "globalThis").to_object());
        assert_eq!(seen.get(), second.get(), "the new realm's script sees its own global");
        unsafe { LeaveRealm(cx, old) };
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "second", "old references reach the new window");

        runtime.gc_for_testing();
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "second", "after a collection too");
        // Servo then builds the new window's interfaces from its realm's intrinsics.
        for window in [second.get(), first.get()] {
            let old = unsafe { EnterRealm(cx, window) };
            assert!(!unsafe { GetRealmObjectPrototype(cx) }.is_null(), "Object.prototype of {}", if window == second.get() { "the new window" } else { "the old window" });
            assert!(!unsafe { GetRealmFunctionPrototype(cx) }.is_null(), "Function.prototype");
            assert!(!unsafe { GetRealmErrorPrototype(cx) }.is_null(), "Error.prototype");
            assert!(!unsafe { GetRealmIteratorPrototype(cx) }.is_null(), "%IteratorPrototype%");
            unsafe { LeaveRealm(cx, old) };
        }
        // The previous Window keeps its realm: Servo maps objects back to their global.
        assert_eq!(unsafe { GetNonCCWObjectGlobal(first.get()) }, first.get(), "the previous Window is still its realm's global");
        assert_eq!(unsafe { GetNonCCWObjectGlobal(second.get()) }, second.get());
        // The previous Window keeps its native state, off the proxy.
        let mut slot = UndefinedValue();
        unsafe { JS_GetReservedSlot(first.get(), 0, &mut slot) };
        assert_eq!(slot.to_int32(), 7);
        let mut fresh = UndefinedValue();
        unsafe { JS_GetReservedSlot(second.get(), 0, &mut fresh) };
        assert!(fresh.is_undefined());
        let aliases = unsafe { &*cx }.with_scope(|scope| unsafe {
            let first = crate::cell::cell_value(scope, first.get() as *mut std::ffi::c_void);
            let second = crate::cell::cell_value(scope, second.get() as *mut std::ffi::c_void);
            first.strict_equals(second)
        });
        assert!(!aliases, "the previous Window no longer aliases the proxy");
        // Only a registered global of the same class can hand over its proxy.
        rooted!(in(cx) let plain = eval(&runtime, "({})").to_object());
        assert!(unsafe { RovesNewWindowGlobalReusingProxy(cx, &WINDOW_CLASS, ptr::null_mut(), ptr::null(), plain.get()) }.is_null());

        // The side channel bindings use: the next Window global takes over the proxy.
        unsafe { RovesReuseWindowProxyForNextGlobal(cx, second.get()) };
        rooted!(in(cx) let third = new_window());
        let old = unsafe { EnterRealm(cx, third.get()) };
        eval(&runtime, "globalThis.page = 'third'; 0");
        unsafe { LeaveRealm(cx, old) };
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "third");
        // An invalid previous global falls back to a fresh one.
        unsafe { RovesReuseWindowProxyForNextGlobal(cx, plain.get()) };
        rooted!(in(cx) let fourth = new_window());
        assert!(!fourth.get().is_null());
        assert_eq!(describe(&runtime, eval(&runtime, "held.page")), "third", "no takeover");
    }

    #[test]
    fn foreign_window_wrappers_keep_identity_realm_and_single_native_owner() {
        unsafe extern "C" fn get_prototype(cx: *mut JSContext, proxy: HandleObject, prototype: MutableHandleObject) -> bool {
            if unsafe { GetObjectRealmOrNull(proxy.get()) } == unsafe { GetCurrentRealmOrNull(cx) } {
                let mut window = UndefinedValue();
                unsafe { GetProxyPrivate(proxy.get(), &mut window) };
                prototype.set(unsafe { GetStaticPrototype(window.to_object()) });
            } else {
                prototype.set(ptr::null_mut());
            }
            true
        }
        unsafe extern "C" fn set_prototype(cx: *mut JSContext, proxy: HandleObject, prototype: HandleObject, result: *mut ObjectOpResult) -> bool {
            rooted!(in(cx) let mut current = ptr::null_mut::<JSObject>());
            if !unsafe { GetObjectProto(cx, proxy, current.handle_mut().into()) } { return false; }
            if current.get() == prototype.get() { unsafe { (*result).succeed() }; }
            else { unsafe { (*result).fail_cant_set_proto() }; }
            true
        }
        unsafe extern "C" fn finalize(_gc: *mut GCContext, _proxy: *mut JSObject) {
            WINDOW_OWNERS_FINALIZED.with(|count| count.set(count.get() + 1));
        }
        let before = WINDOW_OWNERS_FINALIZED.with(std::cell::Cell::get);
        {
            let mut runtime = Runtime::new(JSEngineHandle::for_tests());
            let cx = runtime.raw_cx();
            rooted!(in(cx) let window = unsafe {
                JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
            });
            let parent = unsafe { EnterRealm(cx, window.get()) };
            let owner_realm = unsafe { GetCurrentRealmOrNull(cx) };
            let mut traps: ProxyTraps = unsafe { std::mem::zeroed() };
            traps.getPrototype = Some(get_prototype);
            traps.setPrototype = Some(set_prototype);
            traps.finalize = Some(finalize);
            let handler = unsafe { CreateWrapperProxyHandler(&traps) };
            rooted!(in(cx) let owner = unsafe { NewWindowProxy(cx, window.handle().into(), handler) });
            unsafe { SetWindowProxy(cx, window.handle().into(), owner.handle().into()) };
            unsafe { LeaveRealm(cx, parent) };
            eval(&runtime, "globalThis.WeakMap = function () { throw 13; }");
            rooted!(in(cx) let mut first = ObjectValue(owner.get()));
            rooted!(in(cx) let mut second = ObjectValue(owner.get()));
            assert!(unsafe { JS_WrapValue(cx, first.handle_mut().into()) });
            assert!(unsafe { JS_WrapValue(cx, second.handle_mut().into()) });
            assert_ne!(first.get().to_object(), owner.get());
            assert_eq!(first.get().to_object(), second.get().to_object());
            assert_eq!(unsafe { GetObjectRealmOrNull(first.get().to_object()) }, owner_realm);
            unsafe { SetProxyReservedSlot(owner.get(), 0, &Int32Value(17)) };
            let mut slot = UndefinedValue();
            unsafe { GetProxyReservedSlot(first.get().to_object(), 0, &mut slot) };
            assert_eq!(slot.to_int32(), 17, "reserved slots follow the owner instead of stale copies");
            rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
            assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"foreignWindow".as_ptr(), first.handle().into()) });
            runtime.gc_for_testing();
            assert!(eval(&runtime, "Object.getPrototypeOf(foreignWindow) === null && !Reflect.setPrototypeOf(foreignWindow, {}) && Reflect.setPrototypeOf(foreignWindow, null)").to_boolean());
            let parent = unsafe { EnterRealm(cx, window.get()) };
            assert!(unsafe { JS_WrapValue(cx, first.handle_mut().into()) });
            assert_eq!(first.get().to_object(), owner.get(), "returning to the owner realm unwraps the adapter");
            unsafe { LeaveRealm(cx, parent) };
            let weak_realm = crate::realm_impl::testing::realm_weak(owner_realm);
            drop(owner);
            drop(window);
            drop(first);
            drop(second);
            runtime.gc_for_testing();
            assert!(weak_realm.get().is_some(), "the foreign wrapper keeps its owner's realm");
            eval(&runtime, "delete globalThis.foreignWindow; 0");
            for _ in 0..3 {
                runtime.gc_for_testing();
                unsafe { &mut *(*cx).isolate }.low_memory_notification();
            }
            assert!(weak_realm.get().is_none(), "the dead wrapper/owner/context cycle collects");
        }
        assert_eq!(WINDOW_OWNERS_FINALIZED.with(std::cell::Cell::get), before + 1, "only the native owner finalizes DOM state");
    }

    #[test]
    fn window_global_initializes_its_canonical_prototype_without_replacing_it() {
        static PROTOTYPE: JSClass = JSClass { name: c"WindowPrototype".as_ptr(), flags: 0, ..WINDOW_CLASS };
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let window = unsafe {
            JS_NewGlobalObject(cx, &WINDOW_CLASS, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, window.get()) };
        eval(&runtime, "globalThis.initialPrototype = Object.getPrototypeOf(globalThis)");
        rooted!(in(cx) let parent = unsafe { JS_NewPlainObject(cx) });
        rooted!(in(cx) let canonical = unsafe { JS_NewObjectWithGivenProto(cx, &PROTOTYPE, parent.handle().into()) });
        assert!(!canonical.get().is_null());
        assert!(unsafe { JS_SetPrototype(cx, window.handle().into(), canonical.handle().into()) });
        assert_eq!(unsafe { GetStaticPrototype(canonical.get()) }, parent.get());
        rooted!(in(cx) let second = unsafe { JS_NewObjectWithGivenProto(cx, &PROTOTYPE, parent.handle().into()) });
        assert_ne!(second.get(), canonical.get(), "only the initial binding adopts the template prototype");
        let mut immutable = false;
        assert!(unsafe { JS_SetImmutablePrototype(cx, window.handle().into(), &mut immutable) });
        assert!(immutable);
        rooted!(in(cx) let value = ObjectValue(canonical.get()));
        assert!(unsafe { JS_SetProperty(cx, window.handle().into(), c"canonicalPrototype".as_ptr(), value.handle().into()) });
        assert!(eval(&runtime, "initialPrototype === canonicalPrototype && Object.getPrototypeOf(globalThis) === canonicalPrototype").to_boolean());
        assert!(eval(&runtime, "Reflect.setPrototypeOf(globalThis, canonicalPrototype) && !Reflect.setPrototypeOf(globalThis, {})").to_boolean());
        assert!(eval(&runtime, "(() => { try { Object.setPrototypeOf(globalThis, {}); return false; } catch (e) { return e instanceof TypeError; } })()").to_boolean());
        assert_eq!(eval(&runtime, "globalThis.mutableProperty = 42; mutableProperty").to_int32(), 42);
        unsafe { LeaveRealm(cx, old) };
    }

    #[test]
    fn remote_window_globals_dispatch_their_prototype_security_traps() {
        static REMOTE: JSClass = JSClass { name: c"DissimilarOriginWindow".as_ptr(), ..WINDOW_CLASS };
        unsafe extern "C" fn get_prototype(_cx: *mut JSContext, _proxy: HandleObject, proto: MutableHandleObject) -> bool {
            proto.set(ptr::null_mut());
            true
        }
        unsafe extern "C" fn set_prototype(_cx: *mut JSContext, _proxy: HandleObject, proto: HandleObject, result: *mut ObjectOpResult) -> bool {
            if proto.get().is_null() { unsafe { (*result).succeed() }; } else { unsafe { (*result).fail_cant_set_proto() }; }
            true
        }
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        rooted!(in(cx) let remote = unsafe {
            JS_NewGlobalObject(cx, &REMOTE, ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null())
        });
        let old = unsafe { EnterRealm(cx, remote.get()) };
        let mut traps: ProxyTraps = unsafe { std::mem::zeroed() };
        traps.getPrototype = Some(get_prototype);
        traps.setPrototype = Some(set_prototype);
        let handler = unsafe { CreateWrapperProxyHandler(&traps) };
        rooted!(in(cx) let proxy = unsafe { NewWindowProxy(cx, remote.handle().into(), handler) });
        assert!(!proxy.get().is_null());
        assert!(unsafe { IsWindowProxy(proxy.get()) });
        assert_eq!(unsafe { ToWindowIfWindowProxy(proxy.get()) }, remote.get());
        unsafe { SetWindowProxy(cx, remote.handle().into(), proxy.handle().into()) };
        unsafe { LeaveRealm(cx, old) };
        rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
        rooted!(in(cx) let value = ObjectValue(proxy.get()));
        assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"remoteWindow".as_ptr(), value.handle().into()) });
        assert!(eval(&runtime, "Object.getPrototypeOf(remoteWindow) === null").to_boolean());
        assert!(eval(&runtime, "Reflect.setPrototypeOf(remoteWindow, null) && !Reflect.setPrototypeOf(remoteWindow, {})").to_boolean());
    }

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

    thread_local! {
        static COLLECTED_CALLABLES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    unsafe extern "C" fn finalize_callable(_gc: *mut GCContext, _object: *mut JSObject) {
        COLLECTED_CALLABLES.with(|count| count.set(count.get() + 1));
    }

    static COLLECTIBLE_OPS: JSClassOps = JSClassOps {
        finalize: Some(finalize_callable),
        ..OPS
    };

    static COLLECTIBLE_CLASS: JSClass = JSClass {
        cOps: &COLLECTIBLE_OPS,
        ..CLASS
    };

    #[test]
    fn callable_classes_follow_js_reachability_instead_of_runtime_lifetime() {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        COLLECTED_CALLABLES.with(|count| count.set(0));
        {
            rooted!(in(cx) let iface = unsafe { JS_NewObject(cx, &COLLECTIBLE_CLASS) });
            rooted!(in(cx) let global = eval(&runtime, "globalThis").to_object());
            rooted!(in(cx) let value = ObjectValue(iface.get()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"KeptIface".as_ptr(), value.handle().into()) });
        }
        runtime.gc_for_testing();
        assert_eq!(COLLECTED_CALLABLES.with(std::cell::Cell::get), 0);
        assert_eq!(describe(&runtime, eval(&runtime, "typeof KeptIface")), "function");
        eval(&runtime, "delete globalThis.KeptIface");
        for _ in 0..3 {
            // Collect both heaps: the JS function owns the cppgc holder.
            unsafe { &mut *(*cx).isolate }.request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
            runtime.gc_for_testing();
        }
        assert_eq!(COLLECTED_CALLABLES.with(std::cell::Cell::get), 1, "a dead constructor must not be a runtime root");
        runtime.gc_for_testing();
        assert_eq!(COLLECTED_CALLABLES.with(std::cell::Cell::get), 1, "finalize exactly once");
    }

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
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::ffi::c_void;

    use super::eval;
    use crate::glue::{CreateJobQueue, JobQueueTraps};
    use crate::jsapi::*;
    use crate::jsval::*;
    use crate::rust::{JSEngineHandle, Runtime};

    thread_local! {
        /// The embedder's microtask queue (Servo's `MicrotaskQueue`): JS callbacks, traced as
        /// roots (`trace_host`), as Servo traces its queue.
        static HOST: RefCell<VecDeque<*mut JSObject>> = const { RefCell::new(VecDeque::new()) };
        static PROMISE_JOBS_ENQUEUED: Cell<usize> = const { Cell::new(0) };
        /// The checkpoint's re-entrancy flag (HTML's "performing a microtask checkpoint").
        static PERFORMING: Cell<bool> = const { Cell::new(false) };
        /// Turns V8 gave the embedder (`runJobs` calls) and exceptions the embedder reported.
        static TURNS: Cell<usize> = const { Cell::new(0) };
        static REPORTED: Cell<usize> = const { Cell::new(0) };
    }

    /// SpiderMonkey's promise-job trap: V8 keeps its promise jobs, so it is never called.
    unsafe extern "C" fn enqueue(
        _queue: *const c_void,
        _cx: *mut JSContext,
        _promise: HandleObject,
        _job: HandleObject,
        _site: HandleObject,
        _data: HandleObject,
    ) -> bool {
        PROMISE_JOBS_ENQUEUED.with(|count| count.set(count.get() + 1));
        true
    }

    /// Servo's `run_jobs`: runs the oldest embedder microtask on its turn.
    unsafe extern "C" fn run_jobs(_queue: *const c_void, cx: *mut JSContext) {
        TURNS.with(|turns| turns.set(turns.get() + 1));
        run_next(cx);
    }

    /// Servo's `MicrotaskQueue::run_next`: its callback wrapper reports a thrown exception (a
    /// terminated callback leaves none).
    fn run_next(cx: *mut JSContext) {
        let Some(job) = HOST.with(|host| host.borrow_mut().pop_front()) else { return };
        rooted!(in(cx) let job = ObjectValue(job));
        rooted!(in(cx) let this = UndefinedValue());
        rooted!(in(cx) let mut rval = UndefinedValue());
        let no_args = HandleValueArray { length_: 0, elements_: std::ptr::null() };
        if !unsafe { Call(cx, this.handle().into_handle(), job.handle().into_handle(), &no_args, rval.handle_mut().into_handle()) } &&
            unsafe { JS_IsExceptionPending(cx) }
        {
            unsafe { JS_ClearPendingException(cx) };
            REPORTED.with(|reported| reported.set(reported.get() + 1));
        }
    }

    /// An uncatchable failure (a native returning `false` without an exception), as when an
    /// interrupt stops the script: V8 terminates execution.
    unsafe extern "C" fn terminate(_cx: *mut JSContext, _argc: u32, _vp: *mut JSVal) -> bool {
        false
    }

    unsafe extern "C" fn host_checkpoint(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        checkpoint(cx);
        unsafe { CallArgs::from_vp(vp, argc) }.rval().set(UndefinedValue());
        true
    }

    /// Servo's `queueMicrotask`: the callback goes to the embedder's queue, which tells the
    /// runtime through the production bridge.
    unsafe extern "C" fn queue_microtask(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
        let args = unsafe { CallArgs::from_vp(vp, argc) };
        HOST.with(|host| host.borrow_mut().push_back(args.get(0).get().to_object()));
        let safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        unsafe { crate::rust::wrappers2::JobQueueMayNotBeEmpty(&safe) };
        args.rval().set(UndefinedValue());
        true
    }

    unsafe extern "C" fn trace_host(trc: *mut JSTracer, _data: *mut c_void) {
        HOST.with(|host| {
            for job in host.borrow_mut().iter_mut() {
                unsafe { crate::glue::CallObjectRootTracer(trc, job, c"microtask".as_ptr()) };
            }
        });
    }

    /// Resets the host, then defines it on the current global.
    fn install_host(runtime: &Runtime) {
        HOST.with(|host| host.borrow_mut().clear());
        PROMISE_JOBS_ENQUEUED.with(|count| count.set(0));
        TURNS.with(|turns| turns.set(0));
        REPORTED.with(|reported| reported.set(0));
        define_host(runtime);
    }

    /// Defines the host's natives and `queueMicrotask` on the current global.
    fn define_host(runtime: &Runtime) {
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe { CurrentGlobalOrNull(cx) });
        let natives: [(JSNative, &std::ffi::CStr); 3] = [
            (Some(queue_microtask), c"hostQueueMicrotask"),
            (Some(terminate), c"terminate"),
            (Some(host_checkpoint), c"hostCheckpoint"),
        ];
        for (native, name) in natives {
            let function = unsafe { JS_NewFunction(cx, native, 1, 0, name.as_ptr()) };
            rooted!(in(cx) let function = ObjectValue(function.cast()));
            assert!(unsafe { JS_SetProperty(cx, global.handle().into(), name.as_ptr(), function.handle().into()) });
        }
        eval(runtime, "globalThis.queueMicrotask = hostQueueMicrotask; 0");
    }

    /// Servo's `MicrotaskQueue::checkpoint` (steps 1 to 3 and 7).
    fn checkpoint(cx: *mut JSContext) {
        if PERFORMING.with(|performing| performing.replace(true)) {
            return;
        }
        let mut safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        loop {
            unsafe { crate::rust::wrappers2::PerformMicrotaskCheckpoint(&mut safe) };
            if HOST.with(|host| host.borrow().is_empty()) {
                break;
            }
            run_next(cx);
        }
        PERFORMING.with(|performing| performing.set(false));
    }

    fn log(runtime: &Runtime) -> std::string::String {
        super::describe(runtime, eval(runtime, "log.splice(0).join()"))
    }

    fn new_runtime_with_job_queue() -> (Runtime, *mut JobQueue) {
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        // SAFETY: an all-`None` trap table, plus the traps Servo installs.
        let mut traps: JobQueueTraps = unsafe { std::mem::zeroed() };
        traps.enqueuePromiseJob = Some(enqueue);
        traps.runJobs = Some(run_jobs);
        let queue = unsafe { CreateJobQueue(&traps, std::ptr::null(), std::ptr::null_mut()) };
        unsafe { SetJobQueue(runtime.raw_cx(), queue) };
        assert!(unsafe { JS_AddExtraGCRootsTracer(runtime.raw_cx(), Some(trace_host), std::ptr::null_mut()) });
        install_host(&runtime);
        (runtime, queue)
    }

    /// The orders Chrome 155 shows for these cases (task 0329's Chrome DevTools reference
    /// covers the first two).
    const CASES: [(&str, &str); 3] = [
        (
            "Promise.resolve().then(() => { log.push('p1'); Promise.resolve().then(() => log.push('p2')) }); \
             queueMicrotask(() => log.push('q'))",
            "p1,q,p2",
        ),
        (
            "queueMicrotask(() => { log.push('q1'); Promise.resolve().then(() => log.push('p2')); \
             queueMicrotask(() => log.push('q2')) }); Promise.resolve().then(() => log.push('p1'))",
            "q1,p1,p2,q2",
        ),
        (
            "Promise.resolve().then(() => log.push('p1')); queueMicrotask(() => log.push('q')); \
             Promise.resolve().then(() => log.push('p2'))",
            "p1,q,p2",
        ),
    ];

    #[test]
    fn promise_jobs_and_embedder_microtasks_share_one_fifo() {
        let (runtime, queue) = new_runtime_with_job_queue();
        for (source, expected) in CASES {
            eval(&runtime, &format!("globalThis.log = []; {source}; 0"));
            assert_eq!(eval(&runtime, "log.length").to_int32(), 0, "V8 runs no microtask by itself: {source}");
            checkpoint(runtime.raw_cx());
            assert_eq!(super::describe(&runtime, eval(&runtime, "log.join()")), expected, "{source}");
            assert!(HOST.with(|host| host.borrow().is_empty()), "every embedder microtask had its turn");
        }
        assert_eq!(PROMISE_JOBS_ENQUEUED.with(Cell::get), 0, "promise jobs stay in V8's queue");
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
    }

    static WINDOW_LIKE: JSClass = JSClass {
        name: c"WindowLike".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL | (crate::object::JSCLASS_GLOBAL_SLOT_COUNT << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: std::ptr::null(),
        spec: std::ptr::null(),
        ext: std::ptr::null(),
        oOps: std::ptr::null(),
    };

    #[test]
    fn embedder_microtasks_of_a_realm_keep_their_turn_after_it_is_left() {
        let (runtime, queue) = new_runtime_with_job_queue();
        let cx = runtime.raw_cx();
        rooted!(in(cx) let global = unsafe {
            JS_NewGlobalObject(cx, &WINDOW_LIKE, std::ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, std::ptr::null())
        });
        let old = unsafe { EnterRealm(cx, global.get()) };
        install_host(&runtime);
        eval(&runtime, &format!("globalThis.log = []; {}; 0", CASES[0].0));
        unsafe { LeaveRealm(cx, old) };
        // The checkpoint runs outside the realm (as after a task); the turns belong to the
        // runtime's initial realm.
        checkpoint(runtime.raw_cx());
        let old = unsafe { EnterRealm(cx, global.get()) };
        assert_eq!(super::describe(&runtime, eval(&runtime, "log.join()")), CASES[0].1);
        unsafe { LeaveRealm(cx, old) };
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
    }

    #[test]
    fn promise_jobs_without_embedder_microtasks_wait_for_the_checkpoint() {
        let (runtime, queue) = new_runtime_with_job_queue();
        eval(&runtime, "globalThis.log = []; Promise.resolve(1).then(v => log.push(v)); Promise.resolve(2).then(v => log.push(v)); 0");
        assert_eq!(eval(&runtime, "log.length").to_int32(), 0, "V8 does not run promise jobs by itself");
        checkpoint(runtime.raw_cx());
        assert_eq!(super::describe(&runtime, eval(&runtime, "log.join()")), "1,2");
        checkpoint(runtime.raw_cx());
        assert_eq!(super::describe(&runtime, eval(&runtime, "log.join()")), "1,2", "a checkpoint runs each job once");
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
    }

    /// V8 empties its queue when a microtask's execution is terminated, so the embedder runs
    /// the microtasks whose turn it dropped. The promise jobs and microtasks those queue rejoin
    /// V8's queue within the same checkpoint, and no turn is left to run a later microtask early.
    #[test]
    fn microtasks_after_a_termination_rejoin_the_fifo_in_the_same_checkpoint() {
        let (runtime, queue) = new_runtime_with_job_queue();
        let cx = runtime.raw_cx();
        eval(&runtime, "globalThis.log = []; \
            queueMicrotask(() => { log.push('a'); terminate(); log.push('unreachable') }); \
            queueMicrotask(() => { log.push('b'); Promise.resolve().then(() => log.push('pb')); queueMicrotask(() => log.push('c')) }); \
            Promise.resolve().then(() => log.push('dropped')); 0");
        checkpoint(cx);
        let isolate = unsafe { &*(*cx).isolate };
        assert!(!isolate.is_execution_terminating(), "the termination ended with the stack");
        assert_eq!(log(&runtime), "a,b,pb,c", "V8 dropped the promise job queued before the termination");
        assert!(HOST.with(|host| host.borrow().is_empty()));
        // Nothing is left in V8's queue: neither a promise job nor a turn.
        let turns = TURNS.with(Cell::get);
        let mut safe = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx).unwrap()) };
        unsafe { crate::rust::wrappers2::PerformMicrotaskCheckpoint(&mut safe) };
        assert_eq!((log(&runtime), TURNS.with(Cell::get)), (std::string::String::new(), turns));
        // A later task keeps the FIFO order: no stale turn runs its microtask first.
        eval(&runtime, "Promise.resolve().then(() => log.push('e')); queueMicrotask(() => log.push('d')); 0");
        checkpoint(cx);
        assert_eq!(log(&runtime), "e,d");
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
    }

    #[test]
    fn a_throwing_microtask_does_not_stop_later_work_and_nested_checkpoints_do_nothing() {
        let (runtime, queue) = new_runtime_with_job_queue();
        let cx = runtime.raw_cx();
        eval(&runtime, "globalThis.log = []; \
            queueMicrotask(() => { log.push('t'); Promise.resolve().then(() => log.push('pt')); throw new Error('reported') }); \
            Promise.resolve().then(() => { log.push('p'); throw new Error('rejects its promise') }); \
            queueMicrotask(() => { hostCheckpoint(); log.push('n') }); \
            queueMicrotask(() => log.push('q')); 0");
        checkpoint(cx);
        assert_eq!(log(&runtime), "t,p,n,q,pt", "the nested checkpoint ran nothing out of order");
        assert_eq!(REPORTED.with(Cell::get), 1, "the embedder reported the thrown exception");
        assert!(!unsafe { JS_IsExceptionPending(cx) });
        eval(&runtime, "queueMicrotask(() => log.push('later')); 0");
        checkpoint(cx);
        assert_eq!(log(&runtime), "later");
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
    }

    /// Servo's `Runtime::drop` clears its microtask queue and deletes the job queue before the
    /// runtime; the turns and promise jobs still in the old isolate's queue die with it.
    #[test]
    fn an_abandoned_runtimes_turns_never_reach_the_next_runtime_on_the_thread() {
        for round in 0..2 {
            let (runtime, queue) = new_runtime_with_job_queue();
            eval(&runtime, "globalThis.log = []; queueMicrotask(() => log.push('turn')); Promise.resolve().then(() => log.push('job')); 0");
            if round == 1 {
                // The previous runtime's turn never ran: this one runs only its own.
                checkpoint(runtime.raw_cx());
                assert_eq!((log(&runtime), TURNS.with(Cell::get)), ("turn,job".into(), 1));
            }
            HOST.with(|host| host.borrow_mut().clear());
            unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
            drop(runtime);
        }
    }

    static WINDOW: JSClass = JSClass {
        name: c"Window".as_ptr(),
        flags: crate::object::JSCLASS_IS_GLOBAL | (crate::object::JSCLASS_GLOBAL_SLOT_COUNT << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
        cOps: std::ptr::null(),
        spec: std::ptr::null(),
        ext: std::ptr::null(),
        oOps: std::ptr::null(),
    };

    /// A navigation shuts the old Window's context down (V8 detaches its global): V8 skips its
    /// promise jobs, but the turns of its embedder microtasks belong to the initial realm, so
    /// none is lost and the embedder still decides about each queued callback.
    #[test]
    fn a_shut_down_realms_microtasks_keep_their_turns() {
        let (mut runtime, queue) = new_runtime_with_job_queue();
        let cx = runtime.raw_cx();
        let new_window = || unsafe { JS_NewGlobalObject(cx, &WINDOW, std::ptr::null_mut(), OnNewGlobalHookOption::DontFireOnNewGlobalHook, std::ptr::null()) };
        rooted!(in(cx) let first = new_window());
        let old = unsafe { EnterRealm(cx, first.get()) };
        install_host(&runtime);
        rooted!(in(cx) let first_log = eval(&runtime, "globalThis.log = []; \
            queueMicrotask(() => log.push('q1')); Promise.resolve().then(() => log.push('p1')); \
            queueMicrotask(() => log.push('q2')); log"));
        unsafe { LeaveRealm(cx, old) };
        unsafe { RovesReuseWindowProxyForNextGlobal(cx, first.get()) };
        rooted!(in(cx) let second = new_window());
        let old = unsafe { EnterRealm(cx, second.get()) };
        define_host(&runtime);
        eval(&runtime, "globalThis.log = []; Promise.resolve().then(() => log.push('p3')); queueMicrotask(() => log.push('q3')); 0");
        unsafe { LeaveRealm(cx, old) };
        runtime.gc_for_testing();
        checkpoint(cx);
        assert_eq!(TURNS.with(Cell::get), 3, "every embedder microtask had its turn");
        assert!(HOST.with(|host| host.borrow().is_empty()));
        assert_eq!(super::describe(&runtime, first_log.get()), "q1,q2", "the old context's promise job was skipped");
        let old = unsafe { EnterRealm(cx, second.get()) };
        assert_eq!(log(&runtime), "p3,q3");
        unsafe { LeaveRealm(cx, old) };
        drop(runtime);
        unsafe { crate::runtime_impl::DeleteJobQueue(queue) };
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

