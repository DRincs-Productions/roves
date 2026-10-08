/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! JSAPI object, property, id, string and call operations implemented on V8, with the raw
//! (`*mut JSContext`, raw handle) signatures mozjs_sys exposes. `rust::wrappers2` wraps them
//! with the safe context and Rust handles, as mozjs does.
//!
//! Failures follow SpiderMonkey's convention: `false`/null with a pending exception.

use std::ffi::{CStr, c_char, c_void};

use crate::jsapi::{
    Handle, HandleId, HandleObject, HandleValue, HandleValueArray, JSClass, JSContext, JSFunction,
    JSLinearString, JSNative, JSONWriteCallback, JSObject, JSString, MutableHandle,
    MutableHandleId, MutableHandleObject, MutableHandleValue, ObjectOpResult, PropertyDescriptor,
    Symbol, SymbolCode, jsid,
};
use crate::jsid::{IntId, StringId, SymbolId, VoidId};
use crate::jsval::{JSVal, ObjectValue, UndefinedValue, from_v8, to_v8};

fn raw<'a>(cx: *mut JSContext) -> &'a JSContext {
    // SAFETY: JSAPI callers pass a live context of this thread's runtime.
    unsafe { &*cx }
}

fn object<'s>(scope: &mut v8::PinScope<'s, '_>, obj: *mut JSObject) -> Option<v8::Local<'s, v8::Object>> {
    if obj.is_null() {
        return None;
    }
    // SAFETY: JSAPI callers pass live (rooted) objects.
    let value = unsafe { crate::cell::cell_value(scope, obj as *mut c_void) };
    v8::Local::<v8::Object>::try_from(value).ok()
}

fn value<'s>(scope: &mut v8::PinScope<'s, '_>, value: JSVal) -> v8::Local<'s, v8::Value> {
    // SAFETY: JSAPI callers pass live (rooted) values.
    unsafe { to_v8(scope, value) }
}

fn cstr<'s>(scope: &mut v8::PinScope<'s, '_>, name: *const c_char) -> Option<v8::Local<'s, v8::String>> {
    // SAFETY: JSAPI callers pass NUL-terminated names.
    let name = unsafe { CStr::from_ptr(name) };
    v8::String::new(scope, &name.to_string_lossy())
}

/// The V8 property key of an id.
pub(crate) fn id_key<'s>(scope: &mut v8::PinScope<'s, '_>, id: jsid) -> Option<v8::Local<'s, v8::Name>> {
    if id.is_int() {
        return v8::String::new(scope, &id.to_int().to_string()).map(Into::into);
    }
    if id.is_string() || id.is_symbol() {
        let pointer = id.gcthing()?;
        // SAFETY: string and symbol ids hold live cells (rooted with the id).
        let key = unsafe { crate::cell::cell_value(scope, pointer) };
        return v8::Local::<v8::Name>::try_from(key).ok();
    }
    None
}

/// The id of a V8 property key (string keys that are array indices become int ids).
pub(crate) fn key_id(scope: &mut v8::PinScope, key: v8::Local<v8::Value>) -> jsid {
    if key.is_symbol() {
        let symbol = from_v8(scope, key);
        return SymbolId(symbol.to_symbol());
    }
    if let Some(string) = key.to_string(scope) {
        let text = string.to_rust_string_lossy(scope);
        if let Ok(index) = text.parse::<u32>() {
            if index <= i32::MAX as u32 && index.to_string() == text {
                return IntId(index as i32);
            }
        }
        let value = from_v8(scope, string.into());
        return StringId(value.to_string());
    }
    VoidId()
}

/// Writes V8's descriptor object into a SpiderMonkey `PropertyDescriptor`.
pub(crate) fn fill_descriptor(scope: &mut v8::PinScope, descriptor: v8::Local<v8::Object>, out: &mut PropertyDescriptor) {
    *out = PropertyDescriptor::default();
    fn field<'s>(scope: &mut v8::PinScope<'s, '_>, descriptor: v8::Local<v8::Object>, name: &str) -> Option<v8::Local<'s, v8::Value>> {
        let key = v8::String::new(scope, name)?;
        if !descriptor.has_own_property(scope, key.into())? {
            return None;
        }
        descriptor.get(scope, key.into())
    }
    if let Some(configurable) = field(scope, descriptor, "configurable") {
        out.set_hasConfigurable_(true);
        out.set_configurable_(configurable.boolean_value(scope));
    }
    if let Some(enumerable) = field(scope, descriptor, "enumerable") {
        out.set_hasEnumerable_(true);
        out.set_enumerable_(enumerable.boolean_value(scope));
    }
    if let Some(writable) = field(scope, descriptor, "writable") {
        out.set_hasWritable_(true);
        out.set_writable_(writable.boolean_value(scope));
    }
    if let Some(value) = field(scope, descriptor, "value") {
        out.set_hasValue_(true);
        out.value_ = from_v8(scope, value);
    }
    if let Some(getter) = field(scope, descriptor, "get") {
        out.set_hasGetter_(true);
        if getter.is_object() {
            out.getter_ = from_v8(scope, getter).to_object();
        }
    }
    if let Some(setter) = field(scope, descriptor, "set") {
        out.set_hasSetter_(true);
        if setter.is_object() {
            out.setter_ = from_v8(scope, setter).to_object();
        }
    }
}

/// A V8 descriptor for a SpiderMonkey `PropertyDescriptor`.
pub(crate) fn v8_descriptor(scope: &mut v8::PinScope, descriptor: &PropertyDescriptor) -> v8::PropertyDescriptor {
    let mut result = if descriptor.hasGetter_() || descriptor.hasSetter_() {
        let getter: v8::Local<v8::Value> = if descriptor.getter_.is_null() {
            v8::undefined(scope).into()
        } else {
            value(scope, ObjectValue(descriptor.getter_))
        };
        let setter: v8::Local<v8::Value> = if descriptor.setter_.is_null() {
            v8::undefined(scope).into()
        } else {
            value(scope, ObjectValue(descriptor.setter_))
        };
        v8::PropertyDescriptor::new_from_get_set(getter, setter)
    } else if descriptor.hasValue_() && descriptor.hasWritable_() {
        let data = value(scope, descriptor.value_);
        v8::PropertyDescriptor::new_from_value_writable(data, descriptor.writable_())
    } else if descriptor.hasValue_() {
        let data = value(scope, descriptor.value_);
        v8::PropertyDescriptor::new_from_value(data)
    } else {
        v8::PropertyDescriptor::new()
    };
    if descriptor.hasEnumerable_() {
        result.set_enumerable(descriptor.enumerable_());
    }
    if descriptor.hasConfigurable_() {
        result.set_configurable(descriptor.configurable_());
    }
    result
}

fn define(cx: &JSContext, obj: *mut JSObject, key: impl for<'s, 'i> FnOnce(&mut v8::PinScope<'s, 'i>) -> Option<v8::Local<'s, v8::Name>>, value_of: JSVal, attrs: u32) -> bool {
    cx.catching(|scope| {
        let target = object(scope, obj)?;
        let key = key(scope)?;
        let data = value(scope, value_of);
        target.define_own_property(scope, key, data, crate::api::property_attributes(attrs))
    })
    .unwrap_or(false)
}

// --- Objects -------------------------------------------------------------------------------

/// A new object of `clasp` (a plain object when `clasp` is null) with `Object.prototype`.
pub unsafe fn JS_NewObject(cx: *mut JSContext, clasp: *const JSClass) -> *mut JSObject {
    if clasp.is_null() {
        // SAFETY: forwarded.
        return unsafe { JS_NewPlainObject(cx) };
    }
    crate::object::new_object(raw(cx), clasp, std::ptr::null_mut(), true)
}

pub unsafe fn JS_NewObjectWithGivenProto(cx: *mut JSContext, clasp: *const JSClass, proto: HandleObject) -> *mut JSObject {
    if clasp.is_null() {
        let cx = raw(cx);
        let proto = proto.get();
        return cx
            .catching(|scope| {
                let created = v8::Object::new(scope);
                let prototype: v8::Local<v8::Value> = match object(scope, proto) {
                    Some(proto) => proto.into(),
                    None => v8::null(scope).into(),
                };
                created.set_prototype(scope, prototype)?;
                Some(from_v8(scope, created.into()).to_object())
            })
            .unwrap_or(std::ptr::null_mut());
    }
    crate::object::new_object(raw(cx), clasp, proto.get(), false)
}

pub unsafe fn JS_NewObjectWithoutMetadata(cx: *mut JSContext, clasp: *const JSClass, proto: HandleObject) -> *mut JSObject {
    // SAFETY: forwarded.
    unsafe { JS_NewObjectWithGivenProto(cx, clasp, proto) }
}

pub unsafe fn JS_NewPlainObject(cx: *mut JSContext) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let created = v8::Object::new(scope);
            Some(from_v8(scope, created.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn JS_GetPrototype(cx: *mut JSContext, obj: HandleObject, mut result: MutableHandleObject) -> bool {
    let found = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let prototype = target.get_prototype(scope)?;
        Some(if prototype.is_object() { from_v8(scope, prototype).to_object() } else { std::ptr::null_mut() })
    });
    match found {
        Some(prototype) => {
            result.set(prototype);
            true
        },
        None => false,
    }
}

pub unsafe fn GetObjectProto(cx: *mut JSContext, obj: HandleObject, result: MutableHandleObject) -> bool {
    // SAFETY: forwarded.
    unsafe { JS_GetPrototype(cx, obj, result) }
}

/// The prototype without running proxy traps (the same on V8 for ordinary objects).
pub unsafe fn GetStaticPrototype(obj: *mut JSObject) -> *mut JSObject {
    JSContext::current()
        .catching(|scope| {
            let target = object(scope, obj)?;
            let prototype = target.get_prototype(scope)?;
            Some(if prototype.is_object() { from_v8(scope, prototype).to_object() } else { std::ptr::null_mut() })
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn JS_SetPrototype(cx: *mut JSContext, obj: HandleObject, proto: HandleObject) -> bool {
    raw(cx)
        .catching(|scope| {
            let target = object(scope, obj.get())?;
            let prototype: v8::Local<v8::Value> = match object(scope, proto.get()) {
                Some(proto) => proto.into(),
                None => v8::null(scope).into(),
            };
            target.set_prototype(scope, prototype)
        })
        .unwrap_or(false)
}

/// SpiderMonkey's immutable-prototype flag (WindowProxy, Location, Object.prototype). V8 has
/// no public API for it on ordinary objects: the prototype stays mutable (documented gap).
pub unsafe fn JS_SetImmutablePrototype(_cx: *mut JSContext, _obj: HandleObject, succeeded: *mut bool) -> bool {
    // SAFETY: JSAPI callers pass a valid out pointer.
    unsafe { *succeeded = true };
    true
}

pub unsafe fn JS_FreezeObject(cx: *mut JSContext, obj: HandleObject) -> bool {
    raw(cx)
        .catching(|scope| {
            let target = object(scope, obj.get())?;
            target.set_integrity_level(scope, v8::IntegrityLevel::Frozen)
        })
        .unwrap_or(false)
}

/// Copies `src`'s own properties onto `dst` (with their descriptors).
pub unsafe fn JS_InitializePropertiesFromCompatibleNativeObject(cx: *mut JSContext, dst: HandleObject, src: HandleObject) -> bool {
    raw(cx)
        .catching(|scope| {
            let target = object(scope, dst.get())?;
            let source = object(scope, src.get())?;
            let keys = source.get_own_property_names(
                scope,
                v8::GetPropertyNamesArgs {
                    mode: v8::KeyCollectionMode::OwnOnly,
                    property_filter: v8::PropertyFilter::ALL_PROPERTIES,
                    index_filter: v8::IndexFilter::IncludeIndices,
                    key_conversion: v8::KeyConversionMode::KeepNumbers,
                },
            )?;
            for index in 0..keys.length() {
                let key = keys.get_index(scope, index)?;
                let Ok(name) = v8::Local::<v8::Name>::try_from(key) else { continue };
                let descriptor = source.get_own_property_descriptor(scope, name)?;
                let Ok(descriptor) = v8::Local::<v8::Object>::try_from(descriptor) else { continue };
                let mut copy = PropertyDescriptor::default();
                fill_descriptor(scope, descriptor, &mut copy);
                let copy = v8_descriptor(scope, &copy);
                target.define_property(scope, name, &copy)?;
            }
            Some(true)
        })
        .unwrap_or(false)
}

pub unsafe fn JS_LinkConstructorAndPrototype(cx: *mut JSContext, ctor: HandleObject, proto: HandleObject) -> bool {
    raw(cx)
        .catching(|scope| {
            let constructor = object(scope, ctor.get())?;
            let prototype = object(scope, proto.get())?;
            let prototype_key = v8::String::new(scope, "prototype")?;
            let fixed = v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM | v8::PropertyAttribute::DONT_DELETE;
            constructor.define_own_property(scope, prototype_key.into(), prototype.into(), fixed)?;
            let constructor_key = v8::String::new(scope, "constructor")?;
            prototype.define_own_property(scope, constructor_key.into(), constructor.into(), v8::PropertyAttribute::DONT_ENUM)
        })
        .unwrap_or(false)
}

pub unsafe fn IsCallable(obj: *mut JSObject) -> bool {
    JSContext::current().with_scope(|scope| object(scope, obj).is_some_and(|target| target.is_callable()))
}

pub unsafe fn IsConstructor(obj: *mut JSObject) -> bool {
    JSContext::current().with_scope(|scope| {
        object(scope, obj).is_some_and(|target| v8::Local::<v8::Function>::try_from(target).is_ok_and(|f| f.is_constructor()))
    })
}

pub unsafe fn IsArrayObject(cx: *mut JSContext, v: HandleValue, is_array: *mut bool) -> bool {
    let result = raw(cx).with_scope(|scope| value(scope, v.get()).is_array());
    // SAFETY: JSAPI callers pass a valid out pointer.
    unsafe { *is_array = result };
    true
}

// --- Properties ----------------------------------------------------------------------------

pub unsafe fn JS_GetProperty(cx: *mut JSContext, obj: HandleObject, name: *const c_char, mut vp: MutableHandleValue) -> bool {
    let found = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let key = cstr(scope, name)?;
        let result = target.get(scope, key.into())?;
        Some(from_v8(scope, result))
    });
    match found {
        Some(result) => {
            vp.set(result);
            true
        },
        None => false,
    }
}

pub unsafe fn JS_GetPropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, mut vp: MutableHandleValue) -> bool {
    let found = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let key = id_key(scope, id.get())?;
        let result = target.get(scope, key.into())?;
        Some(from_v8(scope, result))
    });
    match found {
        Some(result) => {
            vp.set(result);
            true
        },
        None => false,
    }
}

/// `Reflect.get(obj, id, receiver)`.
pub unsafe fn JS_ForwardGetPropertyTo(cx: *mut JSContext, obj: HandleObject, id: HandleId, receiver: HandleValue, mut vp: MutableHandleValue) -> bool {
    let found = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let key = id_key(scope, id.get())?;
        let receiver = value(scope, receiver.get());
        let global = scope.get_current_context().global(scope);
        let reflect_key = v8::String::new(scope, "Reflect")?;
        let reflect = v8::Local::<v8::Object>::try_from(global.get(scope, reflect_key.into())?).ok()?;
        let get_key = v8::String::new(scope, "get")?;
        let get = v8::Local::<v8::Function>::try_from(reflect.get(scope, get_key.into())?).ok()?;
        let result = get.call(scope, reflect.into(), &[target.into(), key.into(), receiver])?;
        Some(from_v8(scope, result))
    });
    match found {
        Some(result) => {
            vp.set(result);
            true
        },
        None => false,
    }
}

pub unsafe fn JS_SetProperty(cx: *mut JSContext, obj: HandleObject, name: *const c_char, v: HandleValue) -> bool {
    raw(cx)
        .catching(|scope| {
            let target = object(scope, obj.get())?;
            let key = cstr(scope, name)?;
            let data = value(scope, v.get());
            target.set(scope, key.into(), data)
        })
        .unwrap_or(false)
}

fn has(cx: &JSContext, obj: *mut JSObject, key: impl for<'s, 'i> FnOnce(&mut v8::PinScope<'s, 'i>) -> Option<v8::Local<'s, v8::Name>>, own: bool, found: *mut bool) -> bool {
    let result = cx.catching(|scope| {
        let target = object(scope, obj)?;
        let key = key(scope)?;
        if own { target.has_own_property(scope, key) } else { target.has(scope, key.into()) }
    });
    match result {
        Some(result) => {
            // SAFETY: JSAPI callers pass a valid out pointer.
            unsafe { *found = result };
            true
        },
        None => false,
    }
}

pub unsafe fn JS_HasProperty(cx: *mut JSContext, obj: HandleObject, name: *const c_char, foundp: *mut bool) -> bool {
    has(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), false, foundp)
}

pub unsafe fn JS_HasOwnProperty(cx: *mut JSContext, obj: HandleObject, name: *const c_char, foundp: *mut bool) -> bool {
    has(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), true, foundp)
}

pub unsafe fn JS_HasPropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, foundp: *mut bool) -> bool {
    has(raw(cx), obj.get(), |scope| id_key(scope, id.get()), false, foundp)
}

pub unsafe fn JS_HasOwnPropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, foundp: *mut bool) -> bool {
    has(raw(cx), obj.get(), |scope| id_key(scope, id.get()), true, foundp)
}

pub unsafe fn JS_AlreadyHasOwnPropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, foundp: *mut bool) -> bool {
    has(raw(cx), obj.get(), |scope| id_key(scope, id.get()), true, foundp)
}

pub unsafe fn JS_DeletePropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, result: *mut ObjectOpResult) -> bool {
    let deleted = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let key = id_key(scope, id.get())?;
        target.delete(scope, key.into())
    });
    match deleted {
        Some(deleted) => {
            // SAFETY: JSAPI callers pass a valid result.
            let result = unsafe { &mut *result };
            if deleted { result.succeed() } else { result.fail_cant_delete() };
            true
        },
        None => false,
    }
}

pub unsafe fn JS_DefineProperty(cx: *mut JSContext, obj: HandleObject, name: *const c_char, value: HandleValue, attrs: u32) -> bool {
    define(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), value.get(), attrs)
}

pub unsafe fn JS_DefineProperty3(cx: *mut JSContext, obj: HandleObject, name: *const c_char, value: HandleObject, attrs: u32) -> bool {
    define(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), crate::jsval::ObjectOrNullValue(value.get()), attrs)
}

pub unsafe fn JS_DefineProperty4(cx: *mut JSContext, obj: HandleObject, name: *const c_char, value: Handle<*mut JSString>, attrs: u32) -> bool {
    // SAFETY: a rooted, non-null string.
    let string = crate::jsval::StringValue(unsafe { &*value.get() });
    define(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), string, attrs)
}

pub unsafe fn JS_DefineProperty5(cx: *mut JSContext, obj: HandleObject, name: *const c_char, value: i32, attrs: u32) -> bool {
    define(raw(cx), obj.get(), |scope| cstr(scope, name).map(Into::into), crate::jsval::Int32Value(value), attrs)
}

pub unsafe fn JS_DefinePropertyById2(cx: *mut JSContext, obj: HandleObject, id: HandleId, value: HandleValue, attrs: u32) -> bool {
    define(raw(cx), obj.get(), |scope| id_key(scope, id.get()), value.get(), attrs)
}

pub unsafe fn JS_DefinePropertyById5(cx: *mut JSContext, obj: HandleObject, id: HandleId, value: HandleObject, attrs: u32) -> bool {
    define(raw(cx), obj.get(), |scope| id_key(scope, id.get()), crate::jsval::ObjectOrNullValue(value.get()), attrs)
}

pub unsafe fn JS_DefineUCProperty2(cx: *mut JSContext, obj: HandleObject, name: *const u16, namelen: usize, value: HandleValue, attrs: u32) -> bool {
    // SAFETY: JSAPI callers pass `namelen` code units.
    let units = unsafe { std::slice::from_raw_parts(name, namelen) };
    define(
        raw(cx),
        obj.get(),
        |scope| v8::String::new_from_two_byte(scope, units, v8::NewStringType::Normal).map(Into::into),
        value.get(),
        attrs,
    )
}

/// `Object.defineProperty` with a SpiderMonkey descriptor; `result` reports rejection.
pub unsafe fn JS_DefinePropertyById(cx: *mut JSContext, obj: HandleObject, id: HandleId, desc: Handle<PropertyDescriptor>, result: *mut ObjectOpResult) -> bool {
    // SAFETY: a handle refers to a live descriptor.
    let descriptor = unsafe { &*desc.ptr };
    let defined = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let key = id_key(scope, id.get())?;
        let descriptor = v8_descriptor(scope, descriptor);
        target.define_property(scope, key, &descriptor)
    });
    match defined {
        Some(defined) => {
            // SAFETY: JSAPI callers pass a valid result.
            let result = unsafe { &mut *result };
            if defined { result.succeed() } else { result.fail_cant_redefine_prop() };
            true
        },
        None => false,
    }
}

fn get_descriptor(cx: &JSContext, obj: *mut JSObject, id: jsid, own: bool, desc: MutableHandle<PropertyDescriptor>, holder: Option<MutableHandleObject>, is_none: *mut bool) -> bool {
    let found = cx.catching(|scope| {
        let mut target = object(scope, obj)?;
        let key = id_key(scope, id)?;
        loop {
            let descriptor = target.get_own_property_descriptor(scope, key)?;
            if let Ok(descriptor) = v8::Local::<v8::Object>::try_from(descriptor) {
                let mut out = PropertyDescriptor::default();
                fill_descriptor(scope, descriptor, &mut out);
                let holder = from_v8(scope, target.into()).to_object();
                return Some(Some((out, holder)));
            }
            if own {
                return Some(None);
            }
            let prototype = target.get_prototype(scope)?;
            match v8::Local::<v8::Object>::try_from(prototype) {
                Ok(prototype) => target = prototype,
                Err(_) => return Some(None),
            }
        }
    });
    match found {
        Some(Some((descriptor, found_holder))) => {
            // SAFETY: JSAPI out locations are valid and rooted.
            unsafe {
                *desc.ptr = descriptor;
                *is_none = false;
            }
            if let Some(mut holder) = holder {
                holder.set(found_holder);
            }
            true
        },
        Some(None) => {
            // SAFETY: as above.
            unsafe { *is_none = true };
            true
        },
        None => false,
    }
}

pub unsafe fn JS_GetOwnPropertyDescriptorById(cx: *mut JSContext, obj: HandleObject, id: HandleId, desc: MutableHandle<PropertyDescriptor>, is_none: *mut bool) -> bool {
    get_descriptor(raw(cx), obj.get(), id.get(), true, desc, None, is_none)
}

pub unsafe fn JS_GetPropertyDescriptorById(cx: *mut JSContext, obj: HandleObject, id: HandleId, desc: MutableHandle<PropertyDescriptor>, holder: MutableHandleObject, is_none: *mut bool) -> bool {
    get_descriptor(raw(cx), obj.get(), id.get(), false, desc, Some(holder), is_none)
}

// --- Ids and strings -----------------------------------------------------------------------

pub unsafe fn JS_IdToValue(cx: *mut JSContext, id: jsid, mut vp: MutableHandleValue) -> bool {
    if id.is_int() {
        vp.set(crate::jsval::Int32Value(id.to_int()));
        return true;
    }
    if id.is_string() {
        // SAFETY: a live string id.
        vp.set(crate::jsval::StringValue(unsafe { &*id.to_string() }));
        return true;
    }
    if id.is_symbol() {
        // SAFETY: a live symbol id.
        vp.set(crate::jsval::SymbolValue(unsafe { &*id.to_symbol() }));
        return true;
    }
    let _ = cx;
    vp.set(UndefinedValue());
    true
}

pub unsafe fn RUST_SYMBOL_TO_JSID(sym: *mut Symbol, mut id: MutableHandleId) {
    id.set(SymbolId(sym));
}

/// An id for a string (array-index strings become int ids, as SpiderMonkey atomizes them).
pub unsafe fn RUST_INTERNED_STRING_TO_JSID(cx: *mut JSContext, s: *mut JSString, mut id: MutableHandleId) {
    let result = raw(cx).with_scope(|scope| {
        let key = value(scope, crate::jsval::StringValue(unsafe { &*s }));
        key_id(scope, key)
    });
    id.set(result);
}

pub unsafe fn int_to_jsid(i: i32, mut id: MutableHandleId) {
    id.set(IntId(i));
}

pub unsafe fn RUST_JSID_IS_VOID(id: HandleId) -> bool {
    id.get().is_void()
}

fn new_string(cx: &JSContext, text: &str) -> *mut JSString {
    cx.catching(|scope| {
        let string = v8::String::new(scope, text)?;
        Some(from_v8(scope, string.into()).to_string())
    })
    .unwrap_or(std::ptr::null_mut())
}

/// An atomized string. V8 strings are not pinned atoms: the result is an ordinary string.
pub unsafe fn JS_AtomizeAndPinString(cx: *mut JSContext, s: *const c_char) -> *mut JSString {
    // SAFETY: JSAPI callers pass NUL-terminated strings.
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy();
    new_string(raw(cx), &text)
}

pub unsafe fn JS_AtomizeStringN(cx: *mut JSContext, s: *const c_char, length: usize) -> *mut JSString {
    // SAFETY: JSAPI callers pass `length` Latin-1 bytes.
    let bytes = unsafe { std::slice::from_raw_parts(s as *const u8, length) };
    let units: Vec<u16> = bytes.iter().map(|byte| *byte as u16).collect();
    crate::api::new_string_utf16(raw(cx), &units)
}

pub unsafe fn JS_NewStringCopyN(cx: *mut JSContext, s: *const c_char, n: usize) -> *mut JSString {
    // SAFETY: as for `JS_AtomizeStringN`.
    unsafe { JS_AtomizeStringN(cx, s, n) }
}

pub unsafe fn JS_GetLatin1StringCharsAndLength(_cx: *mut JSContext, _nogc: *const crate::jsapi::AutoRequireNoGC, s: *mut JSString, length: *mut usize) -> *const u8 {
    // SAFETY: JSAPI callers pass a valid out pointer.
    crate::api::latin1_chars(s, unsafe { &mut *length })
}

pub unsafe fn JS_GetTwoByteStringCharsAndLength(_cx: *mut JSContext, _nogc: *const crate::jsapi::AutoRequireNoGC, s: *mut JSString, length: *mut usize) -> *const u16 {
    // SAFETY: JSAPI callers pass a valid out pointer.
    crate::api::two_byte_chars(s, unsafe { &mut *length })
}

/// Atoms and linear strings are ordinary strings on V8.
pub unsafe fn AtomToLinearString(atom: *mut crate::jsapi::JSAtom) -> *mut JSLinearString {
    atom as *mut JSLinearString
}

pub unsafe fn GetLinearStringLength(s: *mut JSLinearString) -> usize {
    // SAFETY: a live string.
    unsafe { crate::api::JS_GetStringLength(s as *mut JSString) }
}

pub unsafe fn GetLinearStringCharAt(s: *mut JSLinearString, index: usize) -> u16 {
    let mut length = 0;
    let latin1 = crate::api::latin1_chars(s as *mut JSString, &mut length);
    if !latin1.is_null() {
        // SAFETY: `index < length` (JSAPI contract); the chars live with the string.
        return unsafe { *latin1.add(index) } as u16;
    }
    let units = crate::api::two_byte_chars(s as *mut JSString, &mut length);
    // SAFETY: as above.
    unsafe { *units.add(index) }
}

/// Whether the string is a canonical array index (`0` .. `2^32 - 2`).
pub unsafe fn StringIsArrayIndex(s: *const JSLinearString, indexp: *mut u32) -> bool {
    let mut length = 0;
    let mut text = String::new();
    let latin1 = crate::api::latin1_chars(s as *mut JSString, &mut length);
    if !latin1.is_null() {
        // SAFETY: the chars live with the string.
        text.extend(unsafe { std::slice::from_raw_parts(latin1, length) }.iter().map(|byte| *byte as char));
    } else {
        return false;
    }
    match text.parse::<u32>() {
        Ok(index) if index != u32::MAX && index.to_string() == text => {
            // SAFETY: JSAPI callers pass a valid out pointer.
            unsafe { *indexp = index };
            true
        },
        _ => false,
    }
}

pub unsafe fn GetWellKnownSymbol(cx: *mut JSContext, which: SymbolCode) -> *mut Symbol {
    raw(cx).with_scope(|scope| {
        let symbol = crate::binding::symbol_for(scope, which);
        from_v8(scope, symbol.into()).to_symbol()
    })
}

// --- Calls and functions -------------------------------------------------------------------

/// `fun.apply(thisv, args)` (`JS::Call`).
pub unsafe fn Call(cx: *mut JSContext, thisv: HandleValue, fun: HandleValue, args: *const HandleValueArray, mut rval: MutableHandleValue) -> bool {
    // SAFETY: JSAPI callers pass a valid argument array of rooted values.
    let args = unsafe { &*args };
    let arguments: Vec<JSVal> = if args.length_ == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(args.elements_, args.length_) }.to_vec()
    };
    let result = raw(cx).catching(|scope| {
        let function = value(scope, fun.get());
        let Ok(function) = v8::Local::<v8::Function>::try_from(function) else {
            let message = v8::String::new(scope, "value is not a function")?;
            let error = v8::Exception::type_error(scope, message);
            scope.throw_exception(error);
            return None;
        };
        let receiver = value(scope, thisv.get());
        let arguments: Vec<v8::Local<v8::Value>> = arguments.iter().map(|argument| value(scope, *argument)).collect();
        let result = function.call(scope, receiver, &arguments)?;
        Some(from_v8(scope, result))
    });
    match result {
        Some(result) => {
            rval.set(result);
            true
        },
        None => false,
    }
}

/// The per-function state of a function made from a native (`JS_NewFunction`,
/// `JS_DefineFunctions`): the native, the function's cell (its `callee`) and SpiderMonkey's
/// two extended "native reserved" slots. Owned by the runtime and traced by its root set
/// (such functions live as long as the runtime; DOM method functions are a bounded set).
pub(crate) struct NativeFunction {
    pub(crate) native: JSNative,
    pub(crate) cell: std::cell::Cell<*mut c_void>,
    pub(crate) reserved: std::cell::RefCell<[JSVal; 2]>,
    /// For a spec-defined method: its JIT info (`RUST_FUNCTION_VALUE_TO_JITINFO`).
    pub(crate) info: *const crate::jsapi::JSJitInfo,
}

fn native_function_callback(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue) {
    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    // SAFETY: the External points at this function's (runtime-owned) state.
    let function = unsafe { &*(external.value() as *const NativeFunction) };
    // SAFETY: the function's own cell is alive (traced by the runtime).
    let callee = unsafe { crate::cell::cell_value(scope, function.cell.get()) };
    let constructing = !args.new_target().is_undefined();
    crate::native::call_native(scope, &args, &mut retval, function.native, callee, constructing);
}

/// A function object calling `native` (null on failure).
pub(crate) fn new_native_function(cx: &JSContext, native: JSNative, nargs: u32, name: &str, constructor: bool, info: *const crate::jsapi::JSJitInfo) -> *mut JSFunction {
    let state = Box::new(NativeFunction {
        native,
        cell: std::cell::Cell::new(std::ptr::null_mut()),
        reserved: std::cell::RefCell::new([UndefinedValue(); 2]),
        info,
    });
    let state_pointer = &*state as *const NativeFunction as *mut c_void;
    let created = cx.catching(|scope| {
        let data = v8::External::new(scope, state_pointer);
        let builder = v8::Function::builder(native_function_callback).data(data.into()).length(nargs as i32);
        let builder = if constructor { builder } else { builder.constructor_behavior(v8::ConstructorBehavior::Throw) };
        let function = builder.build(scope)?;
        let name = v8::String::new(scope, name)?;
        function.set_name(name);
        Some(from_v8(scope, function.into()).to_object())
    });
    let Some(function) = created else { return std::ptr::null_mut() };
    state.cell.set(function as *mut c_void);
    cx.with_scope(|scope| {
        // SAFETY: the cell was just created (and is traced from here on).
        let object = unsafe { crate::cell::cell_value(scope, function as *mut c_void) };
        let object = v8::Local::<v8::Object>::try_from(object).expect("a function object");
        let key = native_function_key(scope);
        let data = v8::External::new(scope, state_pointer);
        object.set_private(scope, key, data.into());
    });
    cx.native_functions.borrow_mut().push(state);
    function as *mut JSFunction
}

/// Keeps native functions (their cells) and their reserved slots alive.
pub(crate) fn trace_native_functions(cx: &JSContext, visitor: &mut v8::cppgc::Visitor) {
    use crate::gc::RootKind;
    let Ok(functions) = cx.native_functions.try_borrow() else { return };
    for function in functions.iter() {
        crate::cell::trace_cell(function.cell.get(), visitor);
        if let Ok(reserved) = function.reserved.try_borrow() {
            for slot in reserved.iter() {
                slot.trace_root(visitor);
            }
        }
    }
}

pub unsafe fn JS_NewFunction(cx: *mut JSContext, call: JSNative, nargs: u32, flags: u32, name: *const c_char) -> *mut JSFunction {
    let name = if name.is_null() { String::new() } else { unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned() };
    let constructor = flags & crate::jsapi::JSFUN_CONSTRUCTOR != 0;
    new_native_function(raw(cx), call, nargs, &name, constructor, std::ptr::null())
}

pub unsafe fn JS_GetFunctionObject(fun: *mut JSFunction) -> *mut JSObject {
    fun as *mut JSObject
}

fn native_function_key<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Private> {
    let name = v8::String::new(scope, "roves-js native function").expect("a short string");
    v8::Private::for_api(scope, Some(name))
}

/// The state of a function made by `new_native_function`.
pub(crate) fn native_function_of_v8<'f>(scope: &mut v8::PinScope, object: v8::Local<v8::Object>) -> Option<&'f NativeFunction> {
    let key = native_function_key(scope);
    let data = object.get_private(scope, key)?;
    let external = v8::Local::<v8::External>::try_from(data).ok()?;
    // SAFETY: only `new_native_function` stores this private, pointing at runtime-owned state.
    Some(unsafe { &*(external.value() as *const NativeFunction) })
}

fn native_function_of<'f>(cx: &JSContext, fun: *mut JSObject) -> Option<&'f NativeFunction> {
    cx.with_scope(|scope| {
        let target = object(scope, fun)?;
        native_function_of_v8(scope, target)
    })
}

pub unsafe fn GetFunctionNativeReserved(fun: *mut JSObject, which: usize) -> *const JSVal {
    match native_function_of(JSContext::current(), fun) {
        // SAFETY: the slots live with the function state.
        Some(function) => unsafe { (*function.reserved.as_ptr()).as_ptr().add(which) },
        None => std::ptr::null(),
    }
}

pub unsafe fn SetFunctionNativeReserved(fun: *mut JSObject, which: usize, val: *const JSVal) {
    if let Some(function) = native_function_of(JSContext::current(), fun) {
        // SAFETY: JSAPI callers pass a valid value.
        function.reserved.borrow_mut()[which] = unsafe { *val };
    }
}

/// The JIT info of a spec-defined method value (null for other functions).
pub unsafe fn RUST_FUNCTION_VALUE_TO_JITINFO(v: JSVal) -> *const crate::jsapi::JSJitInfo {
    if !v.is_object() {
        return std::ptr::null();
    }
    native_function_of(JSContext::current(), v.to_object()).map_or(std::ptr::null(), |function| function.info)
}

// --- Realm intrinsics ----------------------------------------------------------------------

fn intrinsic(cx: &JSContext, path: &[&str]) -> *mut JSObject {
    cx.catching(|scope| {
        let mut current: v8::Local<v8::Value> = scope.get_current_context().global(scope).into();
        for name in path {
            let object = v8::Local::<v8::Object>::try_from(current).ok()?;
            let key = v8::String::new(scope, name)?;
            current = object.get(scope, key.into())?;
        }
        current.is_object().then(|| from_v8(scope, current).to_object())
    })
    .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn GetRealmObjectPrototype(cx: *mut JSContext) -> *mut JSObject {
    intrinsic(raw(cx), &["Object", "prototype"])
}

pub unsafe fn GetRealmFunctionPrototype(cx: *mut JSContext) -> *mut JSObject {
    intrinsic(raw(cx), &["Function", "prototype"])
}

pub unsafe fn GetRealmErrorPrototype(cx: *mut JSContext) -> *mut JSObject {
    intrinsic(raw(cx), &["Error", "prototype"])
}

/// `%IteratorPrototype%` (the prototype of `%ArrayIteratorPrototype%`).
pub unsafe fn GetRealmIteratorPrototype(cx: *mut JSContext) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let array = v8::Array::new(scope, 0);
            let symbol = v8::Symbol::get_iterator(scope);
            let method = v8::Local::<v8::Function>::try_from(array.get(scope, symbol.into())?).ok()?;
            let iterator = v8::Local::<v8::Object>::try_from(method.call(scope, array.into(), &[])?).ok()?;
            let array_iterator_prototype = v8::Local::<v8::Object>::try_from(iterator.get_prototype(scope)?).ok()?;
            let prototype = array_iterator_prototype.get_prototype(scope)?;
            Some(from_v8(scope, prototype).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}

// --- JSON and source -----------------------------------------------------------------------

/// `JSON.stringify(value, replacer, space)`, streamed to `callback` (SpiderMonkey's `ToJSON`).
pub unsafe fn ToJSON(cx: *mut JSContext, value_handle: HandleValue, replacer: HandleObject, space: HandleValue, callback: JSONWriteCallback, data: *mut c_void) -> bool {
    let text = raw(cx).catching(|scope| {
        let global = scope.get_current_context().global(scope);
        let json_key = v8::String::new(scope, "JSON")?;
        let json = v8::Local::<v8::Object>::try_from(global.get(scope, json_key.into())?).ok()?;
        let stringify_key = v8::String::new(scope, "stringify")?;
        let stringify = v8::Local::<v8::Function>::try_from(json.get(scope, stringify_key.into())?).ok()?;
        let target = value(scope, value_handle.get());
        let replacer: v8::Local<v8::Value> = match object(scope, replacer.get()) {
            Some(replacer) => replacer.into(),
            None => v8::undefined(scope).into(),
        };
        let space = value(scope, space.get());
        let result = stringify.call(scope, json.into(), &[target, replacer, space])?;
        if result.is_undefined() {
            return Some(Vec::new());
        }
        let string = result.to_string(scope)?;
        let mut units = vec![0u16; string.length()];
        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
        Some(units)
    });
    let Some(units) = text else { return false };
    match callback {
        // SAFETY: SpiderMonkey's callback contract (chars, length, closure data).
        Some(callback) => unsafe { callback(units.as_ptr(), units.len() as u32, data) },
        None => true,
    }
}

/// An approximation of SpiderMonkey's `uneval` (V8 has no `toSource`): strings are quoted
/// as JSON, other values use `String(value)`.
pub unsafe fn JS_ValueToSource(cx: *mut JSContext, v: HandleValue) -> *mut JSString {
    raw(cx)
        .catching(|scope| {
            let target = value(scope, v.get());
            let text = if target.is_string() {
                v8::json::stringify(scope, target)?
            } else {
                target.to_string(scope)?
            };
            Some(from_v8(scope, text.into()).to_string())
        })
        .unwrap_or(std::ptr::null_mut())
}

// --- Reserved slots ------------------------------------------------------------------------

pub unsafe fn JS_GetReservedSlot(obj: *mut JSObject, index: u32, dest: *mut JSVal) {
    let slot = crate::object::class_box(obj).map_or(UndefinedValue(), |class_box| class_box.reserved_slot(index));
    // SAFETY: callers pass a valid out pointer.
    unsafe { *dest = slot };
}

pub unsafe fn JS_SetReservedSlot(obj: *mut JSObject, index: u32, v: *const JSVal) {
    if let Some(class_box) = crate::object::class_box(obj) {
        // SAFETY: callers pass a valid value.
        class_box.set_reserved_slot(index, unsafe { *v });
    }
}

pub unsafe fn JS_IsGlobalObject(obj: *mut JSObject) -> bool {
    let class = crate::object::object_class(obj);
    // SAFETY: class pointers are static binding tables.
    !class.is_null() && unsafe { (*class).flags } & crate::object::JSCLASS_IS_GLOBAL != 0
}

// --- Raw roots and memory accounting -------------------------------------------------------

unsafe fn trace_raw_value_root(location: *const c_void, visitor: &mut v8::cppgc::Visitor) {
    use crate::gc::RootKind;
    // SAFETY: the location stays valid until `RemoveRawValueRoot`.
    unsafe { &*(location as *const JSVal) }.trace_root(visitor);
}

pub unsafe fn AddRawValueRoot(_cx: *mut JSContext, vp: *mut JSVal, _name: *const c_char) -> bool {
    crate::gc::register_custom_root(vp as *const c_void, trace_raw_value_root);
    true
}

pub unsafe fn RemoveRawValueRoot(_cx: *mut JSContext, vp: *mut JSVal) {
    crate::gc::unregister_custom_root(vp as *const c_void);
}

/// Reports `nbytes` of memory owned by `obj` to the GC (V8's external memory counter).
pub unsafe fn AddAssociatedMemory(_obj: *mut JSObject, nbytes: usize, _use: crate::jsapi::MemoryUse) {
    // SAFETY: the current runtime's isolate, used from its own thread.
    let isolate = unsafe { &mut *JSContext::current().isolate };
    isolate.adjust_amount_of_external_allocated_memory(nbytes as i64);
}

pub unsafe fn RemoveAssociatedMemory(_obj: *mut JSObject, nbytes: usize, _use: crate::jsapi::MemoryUse) {
    // SAFETY: as above.
    let isolate = unsafe { &mut *JSContext::current().isolate };
    isolate.adjust_amount_of_external_allocated_memory(-(nbytes as i64));
}

pub unsafe fn GCTraceKindToAscii(kind: crate::jsapi::TraceKind) -> *const c_char {
    use crate::jsapi::TraceKind;
    let name: &'static CStr = match kind {
        TraceKind::Object => c"Object",
        TraceKind::BigInt => c"BigInt",
        TraceKind::String => c"String",
        TraceKind::Symbol => c"Symbol",
        TraceKind::Script => c"Script",
        TraceKind::Null => c"Null",
        _ => c"Other",
    };
    name.as_ptr()
}

// --- Property keys -------------------------------------------------------------------------

/// The ids of `obj`'s properties (`JSITER_OWNONLY`, `JSITER_HIDDEN`, `JSITER_SYMBOLS`).
pub unsafe fn GetPropertyKeys(cx: *mut JSContext, obj: HandleObject, flags: u32, props: crate::jsapi::MutableHandleIdVector) -> bool {
    use crate::jsapi::{JSITER_HIDDEN, JSITER_OWNONLY, JSITER_SYMBOLS};
    let ids = raw(cx).catching(|scope| {
        let target = object(scope, obj.get())?;
        let mut filter = if flags & JSITER_HIDDEN != 0 { v8::PropertyFilter::ALL_PROPERTIES } else { v8::PropertyFilter::ONLY_ENUMERABLE };
        if flags & JSITER_SYMBOLS == 0 {
            filter = filter | v8::PropertyFilter::SKIP_SYMBOLS;
        }
        let mode = if flags & JSITER_OWNONLY != 0 { v8::KeyCollectionMode::OwnOnly } else { v8::KeyCollectionMode::IncludePrototypes };
        let keys = target.get_property_names(
            scope,
            v8::GetPropertyNamesArgs {
                mode,
                property_filter: filter,
                index_filter: v8::IndexFilter::IncludeIndices,
                key_conversion: v8::KeyConversionMode::KeepNumbers,
            },
        )?;
        let mut ids = Vec::with_capacity(keys.length() as usize);
        for index in 0..keys.length() {
            let key = keys.get_index(scope, index)?;
            ids.push(if key.is_number() {
                let number = key.uint32_value(scope)?;
                if number <= i32::MAX as u32 { IntId(number as i32) } else { key_id(scope, key) }
            } else {
                key_id(scope, key)
            });
        }
        Some(ids)
    });
    let Some(ids) = ids else { return false };
    for id in ids {
        // SAFETY: forwarded to the vector behind the handle.
        unsafe { crate::rust::append_to_id_vector(props, id) };
    }
    true
}

pub unsafe fn AppendToIdVector(v: crate::jsapi::MutableHandleIdVector, id: HandleId) -> bool {
    // SAFETY: the handle points at a live `IdVector`.
    unsafe { crate::rust::append_to_id_vector(v, id.get()) };
    true
}

// --- Principals ----------------------------------------------------------------------------

/// The principals behind a `*mut JSPrincipals` (mozjs's `RustJSPrincipals`).
pub(crate) struct RustPrincipals {
    refcount: std::cell::Cell<i32>,
    _callbacks: *const crate::glue::JSPrincipalsCallbacks,
    private: *mut c_void,
}

thread_local! {
    static DESTROY_PRINCIPALS: std::cell::Cell<crate::jsapi::JSDestroyPrincipalsOp> = const { std::cell::Cell::new(None) };
    static TRUSTED_PRINCIPALS: std::cell::Cell<*mut crate::jsapi::JSPrincipals> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}

pub unsafe fn CreateRustJSPrincipals(callbacks: *const crate::glue::JSPrincipalsCallbacks, private_data: *mut c_void) -> *mut crate::jsapi::JSPrincipals {
    let principals = Box::new(RustPrincipals { refcount: std::cell::Cell::new(0), _callbacks: callbacks, private: private_data });
    Box::into_raw(principals) as *mut crate::jsapi::JSPrincipals
}

/// Frees principals whose last reference was dropped (`JSDestroyPrincipalsOp` callers).
pub unsafe fn DestroyRustJSPrincipals(principals: *mut crate::jsapi::JSPrincipals) {
    // SAFETY: principals come from `CreateRustJSPrincipals`.
    drop(unsafe { Box::from_raw(principals as *mut RustPrincipals) });
}

pub unsafe fn GetRustJSPrincipalsPrivate(principals: *mut crate::jsapi::JSPrincipals) -> *mut c_void {
    // SAFETY: as above.
    unsafe { &*(principals as *const RustPrincipals) }.private
}

pub unsafe fn JS_HoldPrincipals(principals: *mut crate::jsapi::JSPrincipals) {
    // SAFETY: as above.
    let principals = unsafe { &*(principals as *const RustPrincipals) };
    principals.refcount.set(principals.refcount.get() + 1);
}

pub unsafe fn JS_DropPrincipals(_cx: *mut JSContext, principals: *mut crate::jsapi::JSPrincipals) {
    // SAFETY: as above.
    let rust = unsafe { &*(principals as *const RustPrincipals) };
    let count = rust.refcount.get() - 1;
    rust.refcount.set(count);
    if count == 0 {
        if let Some(destroy) = DESTROY_PRINCIPALS.with(std::cell::Cell::get) {
            // SAFETY: the embedder's destroy hook takes ownership of the last reference.
            unsafe { destroy(principals) };
        }
    }
}

pub unsafe fn JS_InitDestroyPrincipalsCallback(_cx: *mut JSContext, destroy: crate::jsapi::JSDestroyPrincipalsOp) {
    DESTROY_PRINCIPALS.with(|hook| hook.set(destroy));
}

pub unsafe fn JS_SetTrustedPrincipals(_cx: *mut JSContext, principals: *mut crate::jsapi::JSPrincipals) {
    TRUSTED_PRINCIPALS.with(|trusted| trusted.set(principals));
}

// --- Compartments, wrappers and scripted callers -------------------------------------------
//
// V8 has no compartments or cross-compartment wrappers: objects of every realm are used
// directly, so wrapping is the identity and each realm stands for its own compartment.

pub unsafe fn IsSharableCompartment(_comp: *mut crate::jsapi::Compartment) -> bool {
    false
}

pub unsafe fn IsSystemCompartment(_comp: *mut crate::jsapi::Compartment) -> bool {
    false
}

pub unsafe fn JS_IterateCompartments(cx: *mut JSContext, data: *mut c_void, callback: crate::jsapi::JSIterateCompartmentCallback) {
    let Some(callback) = callback else { return };
    let realms: Vec<*mut c_void> = raw(cx).realms.borrow().iter().map(|realm| &**realm as *const _ as *mut c_void).collect();
    for realm in realms {
        // SAFETY: SpiderMonkey's iteration callback contract.
        if unsafe { callback(cx, data, realm as *mut crate::jsapi::Compartment) } == crate::jsapi::CompartmentIterResult::Stop {
            break;
        }
    }
}

pub unsafe fn IsWrapper(_obj: *mut JSObject) -> bool {
    false
}

pub unsafe fn IsWindowProxy(_obj: *mut JSObject) -> bool {
    false
}

pub unsafe fn CheckedUnwrapStatic(obj: *mut JSObject) -> *mut JSObject {
    obj
}

pub unsafe fn UnwrapObjectStatic(obj: *mut JSObject) -> *mut JSObject {
    obj
}

pub unsafe fn UncheckedUnwrapObject(obj: *mut JSObject, _stop_at_window_proxy: bool) -> *mut JSObject {
    obj
}

pub unsafe fn UnwrapObjectDynamic(obj: *mut JSObject, _cx: *mut JSContext, _stop_at_window_proxy: bool) -> *mut JSObject {
    obj
}

pub unsafe fn JS_WrapObject(_cx: *mut JSContext, _objp: MutableHandleObject) -> bool {
    true
}

pub unsafe fn JS_WrapValue(_cx: *mut JSContext, _vp: MutableHandleValue) -> bool {
    true
}

/// The global of `obj`'s realm.
pub unsafe fn GetNonCCWObjectGlobal(obj: *mut JSObject) -> *mut JSObject {
    let realm = crate::realm_impl::realm_of_object(JSContext::current(), obj);
    // SAFETY: forwarded.
    unsafe { crate::realm_impl::GetRealmGlobalOrNull(realm) }
}

pub unsafe fn GetFunctionRealm(cx: *mut JSContext, obj: HandleObject) -> *mut crate::jsapi::Realm {
    crate::realm_impl::realm_of_object(raw(cx), obj.get())
}

thread_local! {
    static HIDDEN_CALLERS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub unsafe fn HideScriptedCaller(_cx: *mut JSContext) {
    HIDDEN_CALLERS.with(|count| count.set(count.get() + 1));
}

pub unsafe fn UnhideScriptedCaller(_cx: *mut JSContext) {
    HIDDEN_CALLERS.with(|count| count.set(count.get().saturating_sub(1)));
}

// --- Descriptors and promises --------------------------------------------------------------

pub unsafe fn SetDataPropertyDescriptor(desc: MutableHandle<PropertyDescriptor>, value: HandleValue, attrs: u32) {
    use crate::jsapi::{JSPROP_ENUMERATE, JSPROP_PERMANENT, JSPROP_READONLY};
    let mut descriptor = PropertyDescriptor::default();
    descriptor.set_hasValue_(true);
    descriptor.value_ = value.get();
    descriptor.set_hasWritable_(true);
    descriptor.set_writable_(attrs & JSPROP_READONLY as u32 == 0);
    descriptor.set_hasEnumerable_(true);
    descriptor.set_enumerable_(attrs & JSPROP_ENUMERATE as u32 != 0);
    descriptor.set_hasConfigurable_(true);
    descriptor.set_configurable_(attrs & JSPROP_PERMANENT as u32 == 0);
    // SAFETY: a handle refers to a live, rooted descriptor.
    unsafe { *desc.ptr = descriptor };
}

/// A promise rejected with `rejection_value` (`Promise.reject` without user overrides).
pub unsafe fn CallOriginalPromiseReject(cx: *mut JSContext, rejection_value: HandleValue) -> *mut JSObject {
    raw(cx)
        .catching(|scope| {
            let resolver = v8::PromiseResolver::new(scope)?;
            let reason = value(scope, rejection_value.get());
            resolver.reject(scope, reason)?;
            let promise = resolver.get_promise(scope);
            Some(from_v8(scope, promise.into()).to_object())
        })
        .unwrap_or(std::ptr::null_mut())
}
