/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Promises, JSON, value operations, functions, dates, regular expressions and ArrayBuffer
//! operations of the JSAPI on V8.

use std::ffi::{CStr, c_char, c_void};

use crate::jsapi::{
    ClippedTime, ESClass, Handle, HandleObject, HandleValue, HandleValueArray, JSContext, JSFunction, JSNative,
    JSONWriteCallback, JSObject, JSString, JSType, MutableHandle, MutableHandleId, MutableHandleValue, ObjectOpResult,
    PromiseState, PromiseUserInputEventHandlingState, RegExpFlags, jsid,
};
use crate::jsval::{JSVal, from_v8, to_v8};

fn raw<'a>(cx: *mut JSContext) -> &'a JSContext {
    // SAFETY: JSAPI callers pass this thread's live context.
    unsafe { &*cx }
}

fn object<'s>(scope: &mut v8::PinScope<'s, '_>, obj: *mut JSObject) -> Option<v8::Local<'s, v8::Object>> {
    if obj.is_null() {
        return None;
    }
    // SAFETY: JSAPI callers pass live (rooted) objects.
    v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, obj as *mut c_void) }).ok()
}

fn value<'s>(scope: &mut v8::PinScope<'s, '_>, v: JSVal) -> v8::Local<'s, v8::Value> {
    // SAFETY: JSAPI callers pass live (rooted) values.
    unsafe { to_v8(scope, v) }
}

fn arguments<'s>(scope: &mut v8::PinScope<'s, '_>, args: *const HandleValueArray) -> Vec<v8::Local<'s, v8::Value>> {
    // SAFETY: callers pass a valid array of rooted values.
    let args = unsafe { &*args };
    if args.length_ == 0 {
        return Vec::new();
    }
    let values = unsafe { std::slice::from_raw_parts(args.elements_, args.length_) };
    values.iter().map(|v| value(scope, *v)).collect()
}

fn with_object<R>(obj: *mut JSObject, default: R, f: impl FnOnce(&mut v8::PinScope, v8::Local<v8::Object>) -> R) -> R {
    JSContext::current().with_scope(|scope| match object(scope, obj) {
        Some(target) => f(scope, target),
        None => default,
    })
}

/// Stores a JSAPI result: `Some(value)` into `out`, `None` reports failure.
fn set_value(result: Option<JSVal>, mut out: MutableHandleValue) -> bool {
    match result {
        Some(result) => {
            out.set(result);
            true
        },
        None => false,
    }
}

/// A function compiled once per runtime from `source` (helpers written in JS).
pub(crate) fn helper<'s>(scope: &mut v8::PinScope<'s, '_>, name: &'static str, source: &str) -> Option<v8::Local<'s, v8::Function>> {
    let cx = JSContext::current();
    if let Some(function) = cx.helpers.borrow().get(name) {
        return Some(v8::Local::new(scope, function));
    }
    let code = v8::String::new(scope, source)?;
    let script = v8::Script::compile(scope, code, None)?;
    let function = v8::Local::<v8::Function>::try_from(script.run(scope)?).ok()?;
    cx.helpers.borrow_mut().insert(name, v8::Global::new(scope, function));
    Some(function)
}

// --- Promises --------------------------------------------------------------------------------

fn promise<'s>(scope: &mut v8::PinScope<'s, '_>, obj: *mut JSObject) -> Option<v8::Local<'s, v8::Promise>> {
    object(scope, obj).and_then(|object| v8::Local::<v8::Promise>::try_from(object).ok())
}

pub unsafe fn IsPromiseObject(obj: HandleObject) -> bool {
    with_object(obj.get(), false, |_, object| object.is_promise())
}

/// A new promise: with an `executor`, as `new Promise(executor)`; without one, a pending
/// promise to settle with `ResolvePromise`/`RejectPromise`.
pub unsafe fn NewPromiseObject(cx: *mut JSContext, executor: HandleObject) -> *mut JSObject {
    let executor = executor.get();
    raw(cx)
        .catching(|scope| {
            if executor.is_null() {
                let resolver = v8::PromiseResolver::new(scope)?;
                let promise = resolver.get_promise(scope);
                return Some(from_v8(scope, promise.into()).to_object());
            }
            let executor = object(scope, executor)?;
            let global = scope.get_current_context().global(scope);
            let key = v8::String::new(scope, "Promise")?;
            let constructor = v8::Local::<v8::Function>::try_from(global.get(scope, key.into())?).ok()?;
            let promise = constructor.new_instance(scope, &[executor.into()])?;
            Some(from_v8(scope, promise.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

/// A promise is its own resolver in V8 (`Promise::Resolver::GetPromise` returns it).
fn settle(cx: *mut JSContext, obj: *mut JSObject, v: JSVal, resolve: bool) -> bool {
    raw(cx)
        .catching(|scope| {
            let promise = promise(scope, obj)?;
            // SAFETY: V8's promise resolver is the promise object itself.
            let resolver = unsafe { std::mem::transmute::<v8::Local<v8::Promise>, v8::Local<v8::PromiseResolver>>(promise) };
            let v = value(scope, v);
            if resolve { resolver.resolve(scope, v) } else { resolver.reject(scope, v) }
        })
        .unwrap_or(false)
}

pub unsafe fn ResolvePromise(cx: *mut JSContext, promise: HandleObject, resolution: HandleValue) -> bool {
    settle(cx, promise.get(), resolution.get(), true)
}

pub unsafe fn RejectPromise(cx: *mut JSContext, promise: HandleObject, rejection: HandleValue) -> bool {
    settle(cx, promise.get(), rejection.get(), false)
}

pub unsafe fn CallOriginalPromiseResolve(cx: *mut JSContext, resolution: HandleValue) -> *mut JSObject {
    let resolution = resolution.get();
    raw(cx)
        .catching(|scope| {
            let resolution = value(scope, resolution);
            if resolution.is_promise() {
                return Some(from_v8(scope, resolution).to_object());
            }
            let resolver = v8::PromiseResolver::new(scope)?;
            resolver.resolve(scope, resolution)?;
            let promise = resolver.get_promise(scope);
            Some(from_v8(scope, promise.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn GetPromiseState(promise_obj: HandleObject) -> PromiseState {
    JSContext::current().with_scope(|scope| match promise(scope, promise_obj.get()).map(|promise| promise.state()) {
        Some(v8::PromiseState::Fulfilled) => PromiseState::Fulfilled,
        Some(v8::PromiseState::Rejected) => PromiseState::Rejected,
        _ => PromiseState::Pending,
    })
}

pub unsafe fn JS_GetPromiseResult(promise_obj: HandleObject, mut dest: MutableHandleValue) {
    let result = JSContext::current().with_scope(|scope| {
        let promise = promise(scope, promise_obj.get())?;
        let result = promise.result(scope);
        Some(from_v8(scope, result))
    });
    dest.set(result.unwrap_or(crate::jsval::UndefinedValue()));
}

pub unsafe fn GetPromiseIsHandled(promise_obj: HandleObject) -> bool {
    JSContext::current().with_scope(|scope| promise(scope, promise_obj.get()).is_some_and(|promise| promise.has_handler()))
}

pub unsafe fn SetAnyPromiseIsHandled(cx: *mut JSContext, promise_obj: HandleObject) -> bool {
    raw(cx).with_scope(|scope| {
        if let Some(promise) = promise(scope, promise_obj.get()) {
            promise.mark_as_handled();
        }
    });
    true
}

/// `promise.then(onFulfilled, onRejected)` without user-visible lookups.
pub unsafe fn AddPromiseReactions(cx: *mut JSContext, promise_obj: HandleObject, on_fulfilled: HandleObject, on_rejected: HandleObject) -> bool {
    raw(cx)
        .catching(|scope| {
            let promise = promise(scope, promise_obj.get())?;
            let fulfilled = object(scope, on_fulfilled.get()).and_then(|f| v8::Local::<v8::Function>::try_from(f).ok());
            let rejected = object(scope, on_rejected.get()).and_then(|f| v8::Local::<v8::Function>::try_from(f).ok());
            match (fulfilled, rejected) {
                (Some(fulfilled), Some(rejected)) => promise.then2(scope, fulfilled, rejected),
                (Some(fulfilled), None) => promise.then(scope, fulfilled),
                (None, Some(rejected)) => promise.catch(scope, rejected),
                (None, None) => Some(promise),
            }
            .map(|_| true)
        })
        .unwrap_or(false)
}

fn user_input_key<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Private> {
    let name = v8::String::new(scope, "roves-js promise user input").expect("a short string");
    v8::Private::for_api(scope, Some(name))
}

/// SpiderMonkey's per-promise user-input flag, kept as a V8 private on the promise.
pub unsafe fn SetPromiseUserInputEventHandlingState(promise_obj: HandleObject, state: PromiseUserInputEventHandlingState) -> bool {
    with_object(promise_obj.get(), false, |scope, object| {
        let key = user_input_key(scope);
        let state = v8::Integer::new(scope, state as i32);
        object.set_private(scope, key, state.into()).unwrap_or(false)
    })
}

pub unsafe fn GetPromiseUserInputEventHandlingState(promise_obj: HandleObject) -> PromiseUserInputEventHandlingState {
    with_object(promise_obj.get(), PromiseUserInputEventHandlingState::DontCare, |scope, object| {
        let key = user_input_key(scope);
        match object.get_private(scope, key).and_then(|state| state.int32_value(scope)) {
            Some(1) => PromiseUserInputEventHandlingState::HadUserInteractionAtCreation,
            Some(2) => PromiseUserInputEventHandlingState::DidntHaveUserInteractionAtCreation,
            _ => PromiseUserInputEventHandlingState::DontCare,
        }
    })
}

// --- JSON ------------------------------------------------------------------------------------

pub unsafe fn JS_ParseJSON(cx: *mut JSContext, chars: *const u16, len: u32, vp: MutableHandle<JSVal>) -> bool {
    // SAFETY: callers pass `len` code units.
    let units = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(chars, len as usize) } };
    let result = raw(cx).catching(|scope| {
        let text = v8::String::new_from_two_byte(scope, units, v8::NewStringType::Normal)?;
        let parsed = v8::json::parse(scope, text)?;
        Some(from_v8(scope, parsed))
    });
    set_value(result, vp)
}

/// `JSON.stringify`, streamed to `callback` (the value may be replaced by the result).
pub unsafe fn JS_Stringify(cx: *mut JSContext, v: MutableHandle<JSVal>, replacer: Handle<*mut JSObject>, space: Handle<JSVal>, callback: JSONWriteCallback, data: *mut c_void) -> bool {
    let value_handle = crate::jsapi::Handle { _phantom_0: std::marker::PhantomData, ptr: v.ptr as *const JSVal };
    // SAFETY: forwarded.
    unsafe { crate::jsapi_impl::ToJSON(cx, value_handle, replacer, space, callback, data) }
}

// --- Values ----------------------------------------------------------------------------------

pub unsafe fn SameValue(cx: *mut JSContext, v1: Handle<JSVal>, v2: Handle<JSVal>, same: *mut bool) -> bool {
    let result = raw(cx).with_scope(|scope| {
        let first = value(scope, v1.get());
        let second = value(scope, v2.get());
        first.same_value(second)
    });
    // SAFETY: callers pass a valid out pointer.
    unsafe { *same = result };
    true
}

pub unsafe fn JS_TypeOfValue(cx: *mut JSContext, v: Handle<JSVal>) -> JSType {
    let v = v.get();
    if v.is_undefined() {
        return JSType::JSTYPE_UNDEFINED;
    }
    if v.is_null() {
        return JSType::JSTYPE_OBJECT;
    }
    raw(cx).with_scope(|scope| {
        let local = value(scope, v);
        if local.is_function() {
            JSType::JSTYPE_FUNCTION
        } else if local.is_string() {
            JSType::JSTYPE_STRING
        } else if local.is_number() {
            JSType::JSTYPE_NUMBER
        } else if local.is_boolean() {
            JSType::JSTYPE_BOOLEAN
        } else if local.is_symbol() {
            JSType::JSTYPE_SYMBOL
        } else if local.is_big_int() {
            JSType::JSTYPE_BIGINT
        } else {
            JSType::JSTYPE_OBJECT
        }
    })
}

const TO_PRIMITIVE: &str = "(function (o, hint) {
    const exotic = o[Symbol.toPrimitive];
    if (exotic !== undefined && exotic !== null) {
        const result = exotic.call(o, hint);
        if (Object(result) !== result) return result;
        throw new TypeError('Cannot convert object to primitive value');
    }
    const order = hint === 'string' ? ['toString', 'valueOf'] : ['valueOf', 'toString'];
    for (const name of order) {
        const method = o[name];
        if (typeof method === 'function') {
            const result = method.call(o);
            if (Object(result) !== result) return result;
        }
    }
    throw new TypeError('Cannot convert object to primitive value');
})";

/// ECMAScript's `ToPrimitive(obj, hint)`.
pub unsafe fn ToPrimitive(cx: *mut JSContext, obj: HandleObject, hint: JSType, vp: MutableHandleValue) -> bool {
    let obj = obj.get();
    let result = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let to_primitive = helper(scope, "ToPrimitive", TO_PRIMITIVE)?;
        let hint = match hint {
            JSType::JSTYPE_STRING => "string",
            JSType::JSTYPE_NUMBER => "number",
            _ => "default",
        };
        let hint = v8::String::new(scope, hint)?;
        let undefined = v8::undefined(scope).into();
        let result = to_primitive.call(scope, undefined, &[target.into(), hint.into()])?;
        Some(from_v8(scope, result))
    });
    set_value(result, vp)
}

/// `new fun(...args)`.
pub unsafe fn Construct1(cx: *mut JSContext, fun: Handle<JSVal>, args: *const HandleValueArray, mut objp: MutableHandle<*mut JSObject>) -> bool {
    let fun = fun.get();
    let result = raw(cx).catching(|scope| {
        let constructor = value(scope, fun);
        let Ok(constructor) = v8::Local::<v8::Function>::try_from(constructor) else {
            crate::native::throw_type_error(scope, "value is not a constructor");
            return None;
        };
        let arguments = arguments(scope, args);
        let created = constructor.new_instance(scope, &arguments)?;
        Some(from_v8(scope, created.into()).to_object())
    });
    match result {
        Some(created) => {
            objp.set(created);
            true
        },
        None => false,
    }
}

/// `obj[name](...args)`.
pub unsafe fn JS_CallFunctionName(cx: *mut JSContext, obj: Handle<*mut JSObject>, name: *const c_char, args: *const HandleValueArray, rval: MutableHandle<JSVal>) -> bool {
    let obj = obj.get();
    // SAFETY: callers pass a NUL-terminated name.
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
    let result = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let key = v8::String::new(scope, &name)?;
        let method = target.get(scope, key.into())?;
        let Ok(method) = v8::Local::<v8::Function>::try_from(method) else {
            crate::native::throw_type_error(scope, &format!("{name} is not a function"));
            return None;
        };
        let arguments = arguments(scope, args);
        let result = method.call(scope, target.into(), &arguments)?;
        Some(from_v8(scope, result))
    });
    set_value(result, rval)
}

pub unsafe fn GetArrayLength(cx: *mut JSContext, obj: Handle<*mut JSObject>, lengthp: *mut u32) -> bool {
    let obj = obj.get();
    let length = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let key = v8::String::new(scope, "length")?;
        target.get(scope, key.into())?.uint32_value(scope)
    });
    match length {
        Some(length) => {
            // SAFETY: callers pass a valid out pointer.
            unsafe { *lengthp = length };
            true
        },
        None => false,
    }
}

pub unsafe fn NewArrayObject(cx: *mut JSContext, contents: *const HandleValueArray) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let elements = arguments(scope, contents);
            let array = v8::Array::new_with_elements(scope, &elements);
            Some(from_v8(scope, array.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn NewArrayObject1(cx: *mut JSContext, length: usize) -> *mut JSObject {
    crate::api::new_array(raw(cx), length)
}

pub unsafe fn JS_GetElement(cx: *mut JSContext, obj: Handle<*mut JSObject>, index: u32, vp: MutableHandleValue) -> bool {
    let obj = obj.get();
    let result = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let element = target.get_index(scope, index)?;
        Some(from_v8(scope, element))
    });
    set_value(result, vp)
}

pub unsafe fn JS_IndexToId(cx: *mut JSContext, index: u32, mut id: MutableHandleId) -> bool {
    if index <= i32::MAX as u32 {
        id.set(crate::jsid::IntId(index as i32));
        return true;
    }
    let result = raw(cx).with_scope(|scope| {
        let key = v8::String::new(scope, &index.to_string())?;
        Some(crate::jsapi_impl::key_id(scope, key.into()))
    });
    match result {
        Some(result) => {
            id.set(result);
            true
        },
        None => false,
    }
}

const RESERVED_WORDS: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do", "else", "enum", "export",
    "extends", "false", "finally", "for", "function", "if", "import", "in", "instanceof", "new", "null", "return", "super",
    "switch", "this", "throw", "true", "try", "typeof", "var", "void", "while", "with",
];

/// Whether the string is an IdentifierName that is not a reserved word.
pub unsafe fn JS_IsIdentifier(cx: *mut JSContext, s: Handle<*mut JSString>, is_identifier: *mut bool) -> bool {
    let text = raw(cx).with_scope(|scope| value(scope, crate::jsval::StringValue(unsafe { &*s.get() })).to_rust_string_lossy(scope));
    let mut chars = text.chars();
    let valid = match chars.next() {
        Some(first) => {
            (first.is_alphabetic() || first == '$' || first == '_') &&
                chars.all(|c| c.is_alphanumeric() || c == '$' || c == '_' || c == '\u{200C}' || c == '\u{200D}') &&
                !RESERVED_WORDS.contains(&text.as_str())
        },
        None => false,
    };
    // SAFETY: callers pass a valid out pointer.
    unsafe { *is_identifier = valid };
    true
}

// --- Functions -------------------------------------------------------------------------------

fn function_name(cx: *mut JSContext, fun: *mut JSFunction, mut name: MutableHandle<*mut JSString>) -> bool {
    let result = raw(cx).with_scope(|scope| {
        let function = v8::Local::<v8::Function>::try_from(object(scope, fun as *mut JSObject)?).ok()?;
        let function_name = function.get_name(scope);
        (function_name.length() > 0).then(|| from_v8(scope, function_name.into()).to_string())
    });
    name.set(result.unwrap_or(std::ptr::null_mut()));
    true
}

pub unsafe fn JS_GetFunctionId(cx: *mut JSContext, fun: Handle<*mut JSFunction>, name: MutableHandle<*mut JSString>) -> bool {
    function_name(cx, fun.get(), name)
}

pub unsafe fn JS_GetFunctionDisplayId(cx: *mut JSContext, fun: Handle<*mut JSFunction>, name: MutableHandle<*mut JSString>) -> bool {
    function_name(cx, fun.get(), name)
}

/// The function's `length`.
pub unsafe fn JS_GetFunctionArity(fun: *mut JSFunction) -> u16 {
    with_object(fun as *mut JSObject, 0, |scope, function| {
        let key = v8::String::new(scope, "length").expect("a short string");
        function.get(scope, key.into()).and_then(|length| length.uint32_value(scope)).unwrap_or(0) as u16
    })
}

pub unsafe fn JS_ValueToFunction(cx: *mut JSContext, v: HandleValue) -> *mut JSFunction {
    let v = v.get();
    raw(cx)
        .catching(|scope| {
            let function = value(scope, v);
            if !function.is_function() {
                crate::native::throw_type_error(scope, "value is not a function");
                return None;
            }
            Some(from_v8(scope, function).to_object() as *mut JSFunction)
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn NewFunctionWithReserved(cx: *mut JSContext, call: JSNative, nargs: u32, flags: u32, name: *const c_char) -> *mut JSFunction {
    // SAFETY: forwarded.
    unsafe { crate::jsapi_impl::JS_NewFunction(cx, call, nargs, flags, name) }
}

pub unsafe fn DefineFunctionWithReserved(cx: *mut JSContext, obj: *mut JSObject, name: *const c_char, call: JSNative, nargs: u32, attrs: u32) -> *mut JSFunction {
    // SAFETY: forwarded.
    let function = unsafe { NewFunctionWithReserved(cx, call, nargs, 0, name) };
    if function.is_null() {
        return function;
    }
    crate::rooted!(in(cx) let target = obj);
    crate::rooted!(in(cx) let function_value = crate::jsval::ObjectValue(function as *mut JSObject));
    // SAFETY: rooted arguments.
    let defined = unsafe { crate::jsapi_impl::JS_DefineProperty(cx, target.handle().into(), name, function_value.handle().into(), attrs) };
    if defined { function } else { std::ptr::null_mut() }
}

/// `Reflect.set(obj, id, v, receiver)`.
pub unsafe fn JS_ForwardSetPropertyTo(cx: *mut JSContext, obj: Handle<*mut JSObject>, id: Handle<jsid>, v: Handle<JSVal>, receiver: Handle<JSVal>, result: *mut ObjectOpResult) -> bool {
    let (obj, id, v, receiver) = (obj.get(), id.get(), v.get(), receiver.get());
    let set = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let key = crate::jsapi_impl::id_key(scope, id)?;
        let v = value(scope, v);
        let receiver = value(scope, receiver);
        let reflect_set = helper(scope, "ReflectSet", "(function (t, k, v, r) { return Reflect.set(t, k, v, r); })")?;
        let undefined = v8::undefined(scope).into();
        let set = reflect_set.call(scope, undefined, &[target.into(), key.into(), v, receiver])?;
        Some(set.boolean_value(scope))
    });
    match set {
        Some(set) => {
            // SAFETY: callers pass a valid result.
            let result = unsafe { &mut *result };
            if set { result.succeed() } else { result.fail_read_only() }
        },
        None => false,
    }
}

pub unsafe fn GetBuiltinClass(cx: *mut JSContext, obj: Handle<*mut JSObject>, cls: *mut ESClass) -> bool {
    let class = raw(cx).with_scope(|scope| {
        let Some(target) = object(scope, obj.get()) else { return ESClass::Other };
        let v: v8::Local<v8::Value> = target.into();
        if v.is_array() {
            ESClass::Array
        } else if v.is_number_object() {
            ESClass::Number
        } else if v.is_string_object() {
            ESClass::String
        } else if v.is_boolean_object() {
            ESClass::Boolean
        } else if v.is_reg_exp() {
            ESClass::RegExp
        } else if v.is_array_buffer() {
            ESClass::ArrayBuffer
        } else if v.is_shared_array_buffer() {
            ESClass::SharedArrayBuffer
        } else if v.is_date() {
            ESClass::Date
        } else if v.is_set() {
            ESClass::Set
        } else if v.is_map() {
            ESClass::Map
        } else if v.is_promise() {
            ESClass::Promise
        } else if v.is_map_iterator() {
            ESClass::MapIterator
        } else if v.is_set_iterator() {
            ESClass::SetIterator
        } else if v.is_arguments_object() {
            ESClass::Arguments
        } else if v.is_native_error() {
            ESClass::Error
        } else if v.is_big_int_object() {
            ESClass::BigInt
        } else if v.is_function() {
            ESClass::Function
        } else if v.is_proxy() || crate::object::class_box_of_v8(target).is_some() {
            ESClass::Other
        } else {
            ESClass::Object
        }
    });
    // SAFETY: callers pass a valid out pointer.
    unsafe { *cls = class };
    true
}

/// `Map.prototype.entries.call(map)`.
pub unsafe fn MapEntries(cx: *mut JSContext, obj: HandleObject, rval: MutableHandleValue) -> bool {
    let obj = obj.get();
    let result = raw(cx).catching(|scope| {
        let target = object(scope, obj)?;
        let entries = helper(scope, "MapEntries", "(function (m) { return Map.prototype.entries.call(m); })")?;
        let undefined = v8::undefined(scope).into();
        let iterator = entries.call(scope, undefined, &[target.into()])?;
        Some(from_v8(scope, iterator))
    });
    set_value(result, rval)
}

pub unsafe fn MapSize(cx: *mut JSContext, obj: HandleObject) -> u32 {
    raw(cx).with_scope(|scope| {
        object(scope, obj.get()).and_then(|map| v8::Local::<v8::Map>::try_from(map).ok()).map_or(0, |map| map.size() as u32)
    })
}

// --- Dates and regular expressions -----------------------------------------------------------

pub unsafe fn NewDateObject(cx: *mut JSContext, time: ClippedTime) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let date = v8::Date::new(scope, time.t)?;
            Some(from_v8(scope, date.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn ObjectIsDate(cx: *mut JSContext, obj: Handle<*mut JSObject>, is_date: *mut bool) -> bool {
    let result = raw(cx).with_scope(|scope| object(scope, obj.get()).is_some_and(|object| object.is_date()));
    // SAFETY: callers pass a valid out pointer.
    unsafe { *is_date = result };
    true
}

pub unsafe fn DateGetMsecSinceEpoch(cx: *mut JSContext, obj: HandleObject, msec: *mut f64) -> bool {
    let time = raw(cx).with_scope(|scope| object(scope, obj.get()).and_then(|date| v8::Local::<v8::Date>::try_from(date).ok()).map(|date| date.value_of()));
    match time {
        Some(time) => {
            // SAFETY: callers pass a valid out pointer.
            unsafe { *msec = time };
            true
        },
        None => false,
    }
}

fn regexp_flags(flags: RegExpFlags) -> v8::RegExpCreationFlags {
    use crate::jsapi::{
        RegExpFlag_DotAll, RegExpFlag_Global, RegExpFlag_HasIndices, RegExpFlag_IgnoreCase, RegExpFlag_Multiline,
        RegExpFlag_Sticky, RegExpFlag_Unicode, RegExpFlag_UnicodeSets,
    };
    let bits = flags.flags_;
    let mut result = v8::RegExpCreationFlags::empty();
    for (bit, flag) in [
        (RegExpFlag_Global, v8::RegExpCreationFlags::GLOBAL),
        (RegExpFlag_IgnoreCase, v8::RegExpCreationFlags::IGNORE_CASE),
        (RegExpFlag_Multiline, v8::RegExpCreationFlags::MULTILINE),
        (RegExpFlag_Sticky, v8::RegExpCreationFlags::STICKY),
        (RegExpFlag_Unicode, v8::RegExpCreationFlags::UNICODE),
        (RegExpFlag_DotAll, v8::RegExpCreationFlags::DOT_ALL),
        (RegExpFlag_HasIndices, v8::RegExpCreationFlags::HAS_INDICES),
        (RegExpFlag_UnicodeSets, v8::RegExpCreationFlags::UNICODE_SETS),
    ] {
        if bits & bit != 0 {
            result |= flag;
        }
    }
    result
}

fn utf16<'s>(scope: &mut v8::PinScope<'s, '_>, chars: *const u16, length: usize) -> Option<v8::Local<'s, v8::String>> {
    // SAFETY: callers pass `length` code units.
    let units = if length == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(chars, length) } };
    v8::String::new_from_two_byte(scope, units, v8::NewStringType::Normal)
}

pub unsafe fn NewUCRegExpObject(cx: *mut JSContext, chars: *const u16, length: usize, flags: RegExpFlags) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let pattern = utf16(scope, chars, length)?;
            let regexp = v8::RegExp::new(scope, pattern, regexp_flags(flags))?;
            Some(from_v8(scope, regexp.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn ObjectIsRegExp(cx: *mut JSContext, obj: Handle<*mut JSObject>, is_regexp: *mut bool) -> bool {
    let result = raw(cx).with_scope(|scope| object(scope, obj.get()).is_some_and(|object| object.is_reg_exp()));
    // SAFETY: callers pass a valid out pointer.
    unsafe { *is_regexp = result };
    true
}

/// Checks a pattern: an invalid one stores its `SyntaxError` in `error` (still `true`).
pub unsafe fn CheckRegExpSyntax(cx: *mut JSContext, chars: *const u16, length: usize, flags: RegExpFlags, mut error: MutableHandle<JSVal>) -> bool {
    let raw = raw(cx);
    let created = raw.catching(|scope| {
        let pattern = utf16(scope, chars, length)?;
        v8::RegExp::new(scope, pattern, regexp_flags(flags)).map(|_| ())
    });
    if created.is_none() {
        let exception = raw.pending_exception.borrow_mut().take();
        let exception = raw.with_scope(|scope| exception.map(|exception| from_v8(scope, v8::Local::new(scope, &exception).into())));
        error.set(exception.unwrap_or(crate::jsval::UndefinedValue()));
    } else {
        error.set(crate::jsval::UndefinedValue());
    }
    true
}

const EXECUTE_REGEXP: &str = "(function (re, input, index, test) {
    const flags = re.flags.includes('g') || re.flags.includes('y') ? re.flags : re.flags + 'g';
    const copy = new RegExp(re.source, flags);
    copy.lastIndex = index;
    const match = copy.exec(input);
    return [test ? match !== null : match, copy.lastIndex];
})";

/// Runs `reobj` on `chars` from `*indexp` without touching the regexp's state; `*indexp`
/// becomes the end of the match.
pub unsafe fn ExecuteRegExpNoStatics(
    cx: *mut JSContext,
    reobj: Handle<*mut JSObject>,
    chars: *const u16,
    length: usize,
    indexp: *mut usize,
    test: bool,
    rval: MutableHandle<JSVal>,
) -> bool {
    let reobj = reobj.get();
    // SAFETY: callers pass a valid index.
    let index = unsafe { *indexp };
    let result = raw(cx).catching(|scope| {
        let regexp = object(scope, reobj)?;
        let input = utf16(scope, chars, length)?;
        let execute = helper(scope, "ExecuteRegExp", EXECUTE_REGEXP)?;
        let undefined = v8::undefined(scope).into();
        let index_value = v8::Number::new(scope, index as f64);
        let test_value = v8::Boolean::new(scope, test);
        let pair = execute.call(scope, undefined, &[regexp.into(), input.into(), index_value.into(), test_value.into()])?;
        let pair = v8::Local::<v8::Array>::try_from(pair).ok()?;
        let outcome = pair.get_index(scope, 0)?;
        let last_index = pair.get_index(scope, 1)?.uint32_value(scope)?;
        Some((from_v8(scope, outcome), last_index as usize))
    });
    match result {
        Some((outcome, last_index)) => {
            // SAFETY: as above.
            unsafe { *indexp = last_index };
            set_value(Some(outcome), rval)
        },
        None => false,
    }
}

// --- ArrayBuffers ----------------------------------------------------------------------------

fn buffer<'s>(scope: &mut v8::PinScope<'s, '_>, obj: *mut JSObject) -> Option<v8::Local<'s, v8::ArrayBuffer>> {
    object(scope, obj).and_then(|object| v8::Local::<v8::ArrayBuffer>::try_from(object).ok())
}

fn buffer_bytes(scope: &mut v8::PinScope, obj: *mut JSObject) -> Option<(*mut u8, usize)> {
    let buffer = buffer(scope, obj)?;
    let data = buffer.data().map_or(std::ptr::null_mut(), |data| data.as_ptr() as *mut u8);
    Some((data, buffer.byte_length()))
}

pub unsafe fn ArrayBufferClone(cx: *mut JSContext, src: Handle<*mut JSObject>, offset: usize, length: usize) -> *mut JSObject {
    let src = src.get();
    raw(cx)
        .catching(|scope| {
            let (data, available) = buffer_bytes(scope, src)?;
            if offset.checked_add(length)? > available {
                crate::native::throw_type_error(scope, "ArrayBuffer clone out of range");
                return None;
            }
            let clone = v8::ArrayBuffer::new(scope, length);
            if length > 0 {
                let target = clone.data()?.as_ptr() as *mut u8;
                // SAFETY: both ranges are in bounds and distinct buffers.
                unsafe { std::ptr::copy_nonoverlapping(data.add(offset), target, length) };
            }
            Some(from_v8(scope, clone.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn ArrayBufferCopyData(cx: *mut JSContext, to: Handle<*mut JSObject>, to_index: usize, from: Handle<*mut JSObject>, from_index: usize, count: usize) -> bool {
    let (to, from) = (to.get(), from.get());
    raw(cx)
        .catching(|scope| {
            let (target, target_length) = buffer_bytes(scope, to)?;
            let (source, source_length) = buffer_bytes(scope, from)?;
            if to_index.checked_add(count)? > target_length || from_index.checked_add(count)? > source_length {
                crate::native::throw_type_error(scope, "ArrayBuffer copy out of range");
                return None;
            }
            if count > 0 {
                // SAFETY: in-bounds ranges (possibly of the same buffer).
                unsafe { std::ptr::copy(source.add(from_index), target.add(to_index), count) };
            }
            Some(true)
        })
        .unwrap_or(false)
}

pub unsafe fn DetachArrayBuffer(cx: *mut JSContext, obj: Handle<*mut JSObject>) -> bool {
    let obj = obj.get();
    raw(cx).catching(|scope| buffer(scope, obj)?.detach(None)).unwrap_or(false)
}

pub unsafe fn HasDefinedArrayBufferDetachKey(_cx: *mut JSContext, _obj: Handle<*mut JSObject>, is_defined: *mut bool) -> bool {
    // SAFETY: callers pass a valid out pointer.
    unsafe { *is_defined = false };
    true
}

pub unsafe fn IsDetachedArrayBufferObject(obj: *mut JSObject) -> bool {
    JSContext::current().with_scope(|scope| buffer(scope, obj).is_some_and(|buffer| buffer.was_detached()))
}

unsafe extern "C" fn free_contents(data: *mut c_void, _length: usize, _deleter_data: *mut c_void) {
    // SAFETY: the contents were allocated with `malloc` (the JSAPI contract).
    unsafe { libc::free(data) };
}

/// A buffer taking ownership of `malloc`ed `contents`.
pub unsafe fn NewArrayBufferWithContents(cx: *mut JSContext, nbytes: usize, contents: *mut c_void) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            // SAFETY: the buffer takes the allocation and frees it with `free`.
            let store = unsafe { v8::ArrayBuffer::new_backing_store_from_ptr(contents, nbytes, free_contents, std::ptr::null_mut()) };
            let buffer = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
            Some(from_v8(scope, buffer.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Detaches the buffer, handing its contents to the caller (a `malloc`ed copy).
pub unsafe fn StealArrayBufferContents(cx: *mut JSContext, obj: Handle<*mut JSObject>) -> *mut c_void {
    let obj = obj.get();
    raw(cx)
        .catching(|scope| {
            let target = buffer(scope, obj)?;
            let length = target.byte_length();
            // SAFETY: a fresh allocation of `length` bytes (at least one).
            let copy = unsafe { libc::malloc(length.max(1)) };
            if copy.is_null() {
                return None;
            }
            if length > 0 {
                let data = target.data()?.as_ptr() as *const u8;
                // SAFETY: both ranges hold `length` bytes.
                unsafe { std::ptr::copy_nonoverlapping(data, copy as *mut u8, length) };
            }
            target.detach(None)?;
            Some(copy)
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn JS_NewDataView(cx: *mut JSContext, buffer_obj: Handle<*mut JSObject>, byte_offset: usize, byte_length: usize) -> *mut JSObject {
    let buffer_obj = buffer_obj.get();
    raw(cx)
        .catching(|scope| {
            let target = buffer(scope, buffer_obj)?;
            let view = v8::DataView::new(scope, target, byte_offset, byte_length);
            Some(from_v8(scope, view.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

macro_rules! typed_array_with_buffer {
    ($($name:ident => $v8:ident, $size:expr;)*) => {
        $(
            /// A typed array on `buffer` (`length` -1: the rest of the buffer).
            pub unsafe fn $name(cx: *mut JSContext, buffer_obj: Handle<*mut JSObject>, byte_offset: usize, length: i64) -> *mut JSObject {
                let buffer_obj = buffer_obj.get();
                raw(cx)
                    .catching(|scope| {
                        let target = buffer(scope, buffer_obj)?;
                        let length = if length < 0 { target.byte_length().saturating_sub(byte_offset) / $size } else { length as usize };
                        let array = v8::$v8::new(scope, target, byte_offset, length)?;
                        // SAFETY: every typed array is a value (rusty_v8 lacks some upcasts).
                        let array = unsafe { std::mem::transmute::<v8::Local<v8::$v8>, v8::Local<v8::Value>>(array) };
                        Some(from_v8(scope, array).to_object())
                    })
                    .unwrap_or(std::ptr::null_mut())
            }
        )*
    };
}

typed_array_with_buffer! {
    JS_NewInt8ArrayWithBuffer => Int8Array, 1;
    JS_NewUint8ArrayWithBuffer => Uint8Array, 1;
    JS_NewUint8ClampedArrayWithBuffer => Uint8ClampedArray, 1;
    JS_NewInt16ArrayWithBuffer => Int16Array, 2;
    JS_NewUint16ArrayWithBuffer => Uint16Array, 2;
    JS_NewInt32ArrayWithBuffer => Int32Array, 4;
    JS_NewUint32ArrayWithBuffer => Uint32Array, 4;
    JS_NewFloat16ArrayWithBuffer => Float16Array, 2;
    JS_NewFloat32ArrayWithBuffer => Float32Array, 4;
    JS_NewFloat64ArrayWithBuffer => Float64Array, 8;
    JS_NewBigInt64ArrayWithBuffer => BigInt64Array, 8;
    JS_NewBigUint64ArrayWithBuffer => BigUint64Array, 8;
}

/// The embedder's free function for external buffer contents.
struct ExternalFree {
    free: crate::jsapi::BufferContentsFreeFunc,
    user_data: *mut c_void,
}

unsafe extern "C" fn free_external_contents(data: *mut c_void, _length: usize, deleter_data: *mut c_void) {
    // SAFETY: the deleter data is the boxed `ExternalFree` made for this buffer.
    let external = unsafe { Box::from_raw(deleter_data as *mut ExternalFree) };
    if let Some(free) = external.free {
        // SAFETY: SpiderMonkey's free-function contract.
        unsafe { free(data, external.user_data) };
    }
}

/// A buffer on embedder-owned `contents`, released with `free_func(contents, user_data)`.
pub unsafe fn NewExternalArrayBuffer(
    cx: *mut JSContext,
    nbytes: usize,
    contents: *mut c_void,
    free_func: crate::jsapi::BufferContentsFreeFunc,
    free_user_data: *mut c_void,
) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let external = Box::into_raw(Box::new(ExternalFree { free: free_func, user_data: free_user_data }));
            // SAFETY: the backing store releases the contents through the deleter.
            let store = unsafe { v8::ArrayBuffer::new_backing_store_from_ptr(contents, nbytes, free_external_contents, external as *mut c_void) };
            let buffer = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
            Some(from_v8(scope, buffer.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}
