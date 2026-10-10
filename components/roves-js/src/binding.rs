/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The binding tables of SpiderMonkey (`JSPropertySpec`, `JSFunctionSpec`) and the JIT
//! operations (`JSJitInfo` getters, setters and methods) generated bindings call through.
//!
//! Accessors and methods become V8 functions made from their natives
//! (`jsapi_impl::new_native_function`), carrying their `JSJitInfo` so the generic natives
//! of the bindings find it again through `RUST_FUNCTION_VALUE_TO_JITINFO`.

use std::ffi::{CStr, c_char, c_void};

use crate::jsapi::{
    HandleObject, JSContext, JSFunctionSpec, JSJitGetterCallArgs, JSJitInfo, JSJitMethodCallArgs,
    JSJitSetterCallArgs, JSNativeWrapper, JSObject, JSPropertySpec, JSPropertySpec_Kind,
    JSPropertySpec_ValueWrapper_Type, MutableHandle, SymbolCode,
};
use crate::jsval::{DoubleValue, Int32Value, JSVal, to_v8};

/// SpiderMonkey encodes a well-known symbol name as `SymbolCode + 1` in the name pointer.
const SYMBOL_NAME_LIMIT: usize = 0x1000;

/// The property key of a spec name (`JSPropertySpec::Name` / `JSFunctionSpec::Name`):
/// a C string, or an encoded well-known symbol. `None` ends the table.
fn spec_key<'s>(scope: &mut v8::PinScope<'s, '_>, name: usize) -> Option<(v8::Local<'s, v8::Name>, String)> {
    if name == 0 {
        return None;
    }
    if name < SYMBOL_NAME_LIMIT {
        // SAFETY: SymbolCode is a `u32` enum and encoded names come from its values.
        let code: SymbolCode = unsafe { std::mem::transmute((name - 1) as u32) };
        let symbol = symbol_for(scope, code);
        let description = symbol
            .description(scope)
            .to_string(scope)
            .map(|text| format!("[{}]", text.to_rust_string_lossy(scope)))
            .unwrap_or_default();
        return Some((symbol.into(), description));
    }
    // SAFETY: a non-symbol name is a NUL-terminated C string.
    let text = unsafe { CStr::from_ptr(name as *const c_char) }.to_string_lossy().into_owned();
    let key = v8::String::new(scope, &text)?;
    Some((key.into(), text))
}

pub(crate) fn symbol_for<'s>(scope: &mut v8::PinScope<'s, '_>, code: SymbolCode) -> v8::Local<'s, v8::Symbol> {
    match code {
        SymbolCode::asyncIterator => v8::Symbol::get_async_iterator(scope),
        SymbolCode::hasInstance => v8::Symbol::get_has_instance(scope),
        SymbolCode::isConcatSpreadable => v8::Symbol::get_is_concat_spreadable(scope),
        SymbolCode::match_ => v8::Symbol::get_match(scope),
        SymbolCode::replace => v8::Symbol::get_replace(scope),
        SymbolCode::search => v8::Symbol::get_search(scope),
        SymbolCode::split => v8::Symbol::get_split(scope),
        SymbolCode::toPrimitive => v8::Symbol::get_to_primitive(scope),
        SymbolCode::toStringTag => v8::Symbol::get_to_string_tag(scope),
        SymbolCode::unscopables => v8::Symbol::get_unscopables(scope),
        _ => v8::Symbol::get_iterator(scope),
    }
}

/// A function for a native wrapper (`None` for an absent accessor).
fn wrapper_function<'s>(
    cx: &JSContext,
    scope: &mut v8::PinScope<'s, '_>,
    wrapper: &JSNativeWrapper,
    nargs: u32,
    name: &str,
    constructor: bool,
) -> Option<Option<v8::Local<'s, v8::Value>>> {
    if wrapper.op.is_none() {
        return Some(None);
    }
    let function = crate::jsapi_impl::new_native_function(cx, wrapper.op, nargs, name, constructor, wrapper.info);
    if function.is_null() {
        return None;
    }
    // SAFETY: just created (and kept alive by the runtime).
    Some(Some(unsafe { crate::cell::cell_value(scope, function as *mut c_void) }))
}

/// Defines each property of a `JS_PS_END`-terminated table on `obj`.
pub unsafe fn JS_DefineProperties(cx: *mut JSContext, obj: HandleObject, ps: *const JSPropertySpec) -> bool {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let target = obj.get();
    raw.catching(|scope| {
        // SAFETY: callers pass a live object.
        let object = v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, target as *mut c_void) }).ok()?;
        let mut spec = ps;
        loop {
            // SAFETY: the table is terminated by an entry with a null name.
            let entry = unsafe { &*spec };
            // SAFETY: both name variants are pointer-sized.
            let name = unsafe { entry.name.symbol_ };
            let Some((key, text)) = spec_key(scope, name) else { break };
            let attributes = entry.attributes_ as u32;
            let enumerable = attributes & crate::jsapi::JSPROP_ENUMERATE as u32 != 0;
            let configurable = attributes & crate::jsapi::JSPROP_PERMANENT as u32 == 0;
            match entry.kind_ {
                JSPropertySpec_Kind::NativeAccessor => {
                    // SAFETY: the kind selects the accessor variant.
                    let accessors = unsafe { &entry.u.accessors };
                    let getter = wrapper_function(raw, scope, unsafe { &accessors.getter.native }, 0, &format!("get {text}"), false)?;
                    let setter = wrapper_function(raw, scope, unsafe { &accessors.setter.native }, 1, &format!("set {text}"), false)?;
                    let undefined: v8::Local<v8::Value> = v8::undefined(scope).into();
                    let mut descriptor = v8::PropertyDescriptor::new_from_get_set(getter.unwrap_or(undefined), setter.unwrap_or(undefined));
                    descriptor.set_enumerable(enumerable);
                    descriptor.set_configurable(configurable);
                    object.define_property(scope, key, &descriptor)?;
                },
                JSPropertySpec_Kind::Value => {
                    // SAFETY: the kind selects the value variant.
                    let wrapper = unsafe { &entry.u.value };
                    let data: v8::Local<v8::Value> = match wrapper.type_ {
                        JSPropertySpec_ValueWrapper_Type::String => {
                            // SAFETY: a string value is a C string.
                            let text = unsafe { CStr::from_ptr(wrapper.__bindgen_anon_1.string) }.to_string_lossy();
                            v8::String::new(scope, &text)?.into()
                        },
                        // SAFETY: the type selects the variant.
                        JSPropertySpec_ValueWrapper_Type::Int32 => unsafe { to_v8(scope, Int32Value(wrapper.__bindgen_anon_1.int32)) },
                        // SAFETY: as above.
                        JSPropertySpec_ValueWrapper_Type::Double => unsafe { to_v8(scope, DoubleValue(wrapper.__bindgen_anon_1.double_)) },
                    };
                    object.define_own_property(scope, key, data, crate::api::property_attributes(attributes))?;
                },
                JSPropertySpec_Kind::SelfHostedAccessor => {
                    crate::native::throw_type_error(scope, "self-hosted accessors are not supported on V8");
                    return None;
                },
            }
            // SAFETY: still inside the table (the terminator was not reached).
            spec = unsafe { spec.add(1) };
        }
        Some(true)
    })
    .unwrap_or(false)
}

/// The V8 counterpart of a SpiderMonkey self-hosted intrinsic used by binding tables
/// (WebIDL iterable declarations use the array iteration methods).
fn self_hosted<'s>(scope: &mut v8::PinScope<'s, '_>, name: &str) -> Option<v8::Local<'s, v8::Value>> {
    let method = match name.trim_start_matches('$') {
        "ArrayValues" => "values",
        "ArrayKeys" => "keys",
        "ArrayEntries" => "entries",
        "ArrayForEach" => "forEach",
        _ => return None,
    };
    let global = scope.get_current_context().global(scope);
    let array_key = v8::String::new(scope, "Array")?;
    let array = v8::Local::<v8::Object>::try_from(global.get(scope, array_key.into())?).ok()?;
    let prototype_key = v8::String::new(scope, "prototype")?;
    let prototype = v8::Local::<v8::Object>::try_from(array.get(scope, prototype_key.into())?).ok()?;
    let method_key = v8::String::new(scope, method)?;
    let function = prototype.get(scope, method_key.into())?;
    function.is_function().then_some(function)
}

/// Defines each method of a `JS_FS_END`-terminated table on `obj`.
pub unsafe fn JS_DefineFunctions(cx: *mut JSContext, obj: HandleObject, fs: *const JSFunctionSpec) -> bool {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let target = obj.get();
    raw.catching(|scope| {
        // SAFETY: callers pass a live object.
        let object = v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, target as *mut c_void) }).ok()?;
        let mut spec = fs;
        loop {
            // SAFETY: the table is terminated by an entry with a null name.
            let entry = unsafe { &*spec };
            // SAFETY: both name variants are pointer-sized.
            let name = unsafe { entry.name.symbol_ };
            let Some((key, text)) = spec_key(scope, name) else { break };
            if !entry.selfHostedName.is_null() {
                // SAFETY: a NUL-terminated name.
                let intrinsic = unsafe { CStr::from_ptr(entry.selfHostedName) }.to_string_lossy();
                let Some(function) = self_hosted(scope, &intrinsic) else {
                    crate::native::throw_type_error(scope, &format!("self-hosted function {intrinsic} is not available on V8"));
                    return None;
                };
                let flags = entry.flags as u32;
                object.define_own_property(scope, key, function, crate::api::property_attributes(flags))?;
                // SAFETY: still inside the table.
                spec = unsafe { spec.add(1) };
                continue;
            }
            let flags = entry.flags as u32;
            let constructor = flags & crate::jsapi::JSFUN_CONSTRUCTOR != 0;
            let function = wrapper_function(raw, scope, &entry.call, entry.nargs as u32, &text, constructor)??;
            object.define_own_property(scope, key, function, crate::api::property_attributes(flags))?;
            // SAFETY: still inside the table.
            spec = unsafe { spec.add(1) };
        }
        Some(true)
    })
    .unwrap_or(false)
}

/// `info->getter(cx, thisObj, specializedThis, JSJitGetterCallArgs(args))`.
pub unsafe fn CallJitGetterOp(info: *const JSJitInfo, cx: *mut JSContext, thisObj: HandleObject, specializedThis: *mut c_void, _argc: u32, vp: *mut JSVal) -> bool {
    // SAFETY: a getter info's first union member is its getter.
    let Some(getter) = (unsafe { (*info).__bindgen_anon_1.getter }) else { return false };
    let args = JSJitGetterCallArgs { _base: MutableHandle { _phantom_0: std::marker::PhantomData, ptr: vp } };
    // SAFETY: SpiderMonkey's JIT getter contract.
    unsafe { getter(cx, thisObj, specializedThis, args) }
}

/// `info->setter(cx, thisObj, specializedThis, JSJitSetterCallArgs(args))` (the value is `args[0]`).
pub unsafe fn CallJitSetterOp(info: *const JSJitInfo, cx: *mut JSContext, thisObj: HandleObject, specializedThis: *mut c_void, _argc: u32, vp: *mut JSVal) -> bool {
    // SAFETY: a setter info's first union member is its setter.
    let Some(setter) = (unsafe { (*info).__bindgen_anon_1.setter }) else { return false };
    // SAFETY: `vp` holds callee, this and at least one argument.
    let args = JSJitSetterCallArgs { _base: MutableHandle { _phantom_0: std::marker::PhantomData, ptr: unsafe { vp.add(2) } } };
    // SAFETY: SpiderMonkey's JIT setter contract.
    unsafe { setter(cx, thisObj, specializedThis, args) }
}

/// `info->method(cx, thisObj, specializedThis, JSJitMethodCallArgs(args))`.
pub unsafe fn CallJitMethodOp(info: *const JSJitInfo, cx: *mut JSContext, thisObj: HandleObject, specializedThis: *mut c_void, argc: u32, vp: *mut JSVal) -> bool {
    // SAFETY: a method info's first union member is its method.
    let Some(method) = (unsafe { (*info).__bindgen_anon_1.method }) else { return false };
    // SAFETY: zero is a valid bit pattern (no flags set).
    let mut args: JSJitMethodCallArgs = unsafe { std::mem::zeroed() };
    // SAFETY: `vp` holds callee, this and `argc` arguments.
    args.argv_ = unsafe { vp.add(2) };
    args.argc_ = argc;
    // SAFETY: SpiderMonkey's JIT method contract.
    unsafe { method(cx, thisObj, specializedThis, &args) }
}
