/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Realms: one V8 context each. A `*mut Realm` points at the realm's [`RealmData`], which the
//! context records in its embedder data so an object's realm is found from its creation
//! context.

use std::cell::Cell;
use std::ffi::c_void;

use crate::jsapi::{JSContext, JSObject, JSPrincipals, Realm};

/// Embedder-data slot of a context holding its `RealmData` pointer.
const REALM_SLOT: i32 = 2;
/// Embedder-data slot of a context holding its cache object (see [`realm_cached`]).
const CACHE_SLOT: i32 = 3;

pub(crate) struct RealmData {
    pub(crate) context: v8::Global<v8::Context>,
    /// The realm's global object (its cell): the class object of `JS_NewGlobalObject`, or the
    /// plain global of the runtime's initial realm.
    pub(crate) global: Cell<*mut JSObject>,
    pub(crate) principals: Cell<*mut JSPrincipals>,
    /// The realm's window proxy (`SetWindowProxy`), for a Window global.
    pub(crate) window_proxy: Cell<*mut JSObject>,
    /// The realm's promise-job drain function (see `runtime_impl`).
    pub(crate) drain_function: Cell<*mut JSObject>,
}

/// Registers a new realm for `context` with its `global` cell (null: the context's plain
/// global); the runtime owns it until it is dropped.
pub(crate) fn register_realm(cx: &JSContext, scope: &mut v8::PinScope, context: v8::Local<v8::Context>, global: *mut JSObject) -> *mut Realm {
    let global = if global.is_null() { crate::jsval::from_v8(scope, context.global(scope).into()).to_object() } else { global };
    let data = Box::new(RealmData {
        context: v8::Global::new(scope, context),
        global: Cell::new(global),
        principals: Cell::new(std::ptr::null_mut()),
        window_proxy: Cell::new(std::ptr::null_mut()),
        drain_function: Cell::new(std::ptr::null_mut()),
    });
    let pointer = &*data as *const RealmData as *mut c_void;
    // SpiderMonkey has no access checks between realms (Servo checks cross-origin access
    // itself), so all of a runtime's contexts share one security token.
    // Tokens compare by identity: an internalized string is one object per isolate.
    let token = v8::String::new_from_utf8(scope, b"roves-js runtime", v8::NewStringType::Internalized).expect("a short string");
    context.set_security_token(token.into());
    // SAFETY: the realm data lives as long as the runtime (it owns the context).
    unsafe { context.set_aligned_pointer_in_embedder_data(REALM_SLOT, pointer) };
    let cache = v8::Object::new(scope);
    let null = v8::null(scope).into();
    cache.set_prototype(scope, null);
    context.set_embedder_data(CACHE_SLOT, cache.into());
    // `eval`, `Function` and WebAssembly compilation then go through the embedder's CSP check
    // (see `runtime_impl::JS_SetSecurityCallbacks`).
    context.set_allow_generation_from_strings(false);
    cx.realms.borrow_mut().push(data);
    pointer as *mut Realm
}

pub(crate) fn realm_data<'r>(realm: *mut Realm) -> Option<&'r RealmData> {
    // SAFETY: realm pointers come from `register_realm` and live as long as the runtime.
    (!realm.is_null()).then(|| unsafe { &*(realm as *const RealmData) })
}

/// The value cached under `key` in the current realm, made by `make` on first use. The cache
/// belongs to the realm's V8 context: helpers written in JS and proxy handler objects are
/// per realm, so the objects they create get that realm's prototypes, and they live as long
/// as the realm.
pub(crate) fn realm_cached<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: &str,
    make: impl FnOnce(&mut v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::Value>>,
) -> Option<v8::Local<'s, v8::Value>> {
    let context = scope.get_current_context();
    let cache = v8::Local::<v8::Object>::try_from(context.get_embedder_data(scope, CACHE_SLOT)?).ok()?;
    let key = v8::String::new(scope, key)?;
    let cached = cache.get(scope, key.into())?;
    if !cached.is_undefined() {
        return Some(cached);
    }
    let value = make(scope)?;
    cache.set(scope, key.into(), value)?;
    Some(value)
}

/// The realm a context belongs to.
pub(crate) fn realm_of_context(context: v8::Local<v8::Context>) -> *mut Realm {
    context.get_aligned_pointer_from_embedder_data(REALM_SLOT) as *mut Realm
}

/// The realm an object was created in (null for objects without a creation context).
pub(crate) fn realm_of_object(cx: &JSContext, obj: *mut JSObject) -> *mut Realm {
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    if let Some(class_box) = crate::object::class_box(obj) {
        if let Some(realm) = class_box.realm.borrow().as_ref() {
            return cx.with_scope(|scope| {
                let context = v8::Local::new(scope, realm);
                realm_of_context(context)
            });
        }
    }
    cx.with_scope(|scope| {
        // SAFETY: JSAPI callers pass live objects.
        let value = unsafe { crate::cell::cell_value(scope, obj as *mut c_void) };
        let Ok(object) = v8::Local::<v8::Object>::try_from(value) else { return std::ptr::null_mut() };
        match object.get_creation_context(scope) {
            Some(context) => realm_of_context(context),
            None => std::ptr::null_mut(),
        }
    })
}

/// Enters the realm of `target`'s global; returns the realm that was current.
pub unsafe fn EnterRealm(cx: *mut JSContext, target: *mut JSObject) -> *mut Realm {
    // SAFETY: callers pass a live context (JSAPI contract).
    let cx = unsafe { &*cx };
    let old = cx.current_realm.get();
    let realm = realm_of_object(cx, target);
    if !realm.is_null() {
        cx.set_current_realm(realm);
    }
    old
}

/// Restores `old_realm` as the current realm (after `EnterRealm`).
pub unsafe fn LeaveRealm(cx: *mut JSContext, old_realm: *mut Realm) {
    // SAFETY: callers pass a live context (JSAPI contract).
    let cx = unsafe { &*cx };
    if !old_realm.is_null() {
        cx.set_current_realm(old_realm);
    }
}

pub unsafe fn GetObjectRealmOrNull(obj: *mut JSObject) -> *mut Realm {
    realm_of_object(JSContext::current(), obj)
}

pub unsafe fn GetRealmGlobalOrNull(realm: *mut Realm) -> *mut JSObject {
    realm_data(realm).map_or(std::ptr::null_mut(), |data| data.global.get())
}

pub unsafe fn GetRealmPrincipals(realm: *mut Realm) -> *mut JSPrincipals {
    realm_data(realm).map_or(std::ptr::null_mut(), |data| data.principals.get())
}

/// The current realm of `cx` (mozjs's `get_context_realm`).
pub fn get_context_realm(cx: *mut JSContext) -> *mut Realm {
    // SAFETY: callers pass a live context.
    unsafe { &*cx }.current_realm.get()
}

/// The realm of `obj` (mozjs's `get_object_realm`).
pub fn get_object_realm(obj: *mut JSObject) -> *mut Realm {
    realm_of_object(JSContext::current(), obj)
}

/// Keeps every realm's global alive (SpiderMonkey's realms root their globals).
pub(crate) fn trace_realms(cx: &JSContext, visitor: &mut v8::cppgc::Visitor) {
    let Ok(realms) = cx.realms.try_borrow() else { return };
    for realm in realms.iter() {
        crate::cell::trace_cell(realm.global.get() as *mut c_void, visitor);
        crate::cell::trace_cell(realm.window_proxy.get() as *mut c_void, visitor);
        crate::cell::trace_cell(realm.drain_function.get() as *mut c_void, visitor);
    }
}

pub unsafe fn GetCurrentRealmOrNull(cx: *mut JSContext) -> *mut Realm {
    get_context_realm(cx)
}

pub unsafe fn CurrentGlobalOrNull(cx: *mut JSContext) -> *mut JSObject {
    // SAFETY: forwarded.
    unsafe { GetRealmGlobalOrNull(get_context_realm(cx)) }
}

/// The current realm's global, as a location rooted by the realm.
pub unsafe fn CurrentGlobal(cx: *mut JSContext) -> *const *mut JSObject {
    realm_data(get_context_realm(cx)).map_or(std::ptr::null(), |data| data.global.as_ptr() as *const *mut JSObject)
}

/// A new global object of `clasp` in a new realm (a new V8 context whose global proxy is the
/// class object). The new realm is not entered.
pub unsafe fn JS_NewGlobalObject(
    cx: *mut JSContext,
    clasp: *const crate::jsapi::JSClass,
    principals: *mut JSPrincipals,
    _hook_option: crate::jsapi::OnNewGlobalHookOption,
    options: *const crate::jsapi::RealmOptions,
) -> *mut JSObject {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let created = raw.catching(|scope| {
        let template = raw.class_templates.template(scope, clasp);
        let context = v8::Context::new(scope, v8::ContextOptions { global_template: Some(template), ..Default::default() });
        // The class box goes on the global proxy (what scripts see as `globalThis`) before
        // anything converts it.
        let global = context.global(scope);
        let cell = crate::object::attach_class_box(scope, global, clasp);
        // SAFETY: callers pass valid options (or null).
        if let Some(options) = unsafe { options.as_ref() } {
            if let Some(class_box) = crate::object::class_box(cell) {
                class_box.global_trace.set(options.creationOptions_.traceGlobal_);
            }
        }
        let realm = register_realm(raw, scope, context, cell);
        // V8 gives every context a built-in `console` (silent without an inspector), which
        // would shadow the embedder's (Servo defines `console` lazily, like its interfaces).
        // After registration: the realm's shared security token allows the access.
        let console = v8::String::new(scope, "console")?;
        global.delete(scope, console.into())?;
        let data = realm_data(realm)?;
        if !principals.is_null() {
            // SAFETY: principals come from `CreateRustJSPrincipals`.
            unsafe { crate::jsapi_impl::JS_HoldPrincipals(principals) };
            data.principals.set(principals);
        }
        Some(cell)
    });
    created.unwrap_or(std::ptr::null_mut())
}

/// SpiderMonkey traces a global's intrinsics here; V8 contexts keep their own alive.
pub unsafe extern "C" fn JS_GlobalObjectTraceHook(_trc: *mut crate::jsapi::JSTracer, _global: *mut JSObject) {}

pub unsafe fn JS_FireOnNewGlobalObject(_cx: *mut JSContext, _global: crate::jsapi::HandleObject) {}

/// V8 contexts are created with every standard class already defined: nothing to resolve.
pub unsafe extern "C" fn JS_ResolveStandardClass(_cx: *mut JSContext, _obj: crate::jsapi::HandleObject, _id: crate::jsapi::HandleId, resolved: *mut bool) -> bool {
    // SAFETY: callers pass a valid out pointer.
    unsafe { *resolved = false };
    true
}

pub unsafe extern "C" fn JS_MayResolveStandardClass(_names: *const crate::jsapi::JSAtomState, _id: crate::jsid::jsid, _maybe_obj: *mut JSObject) -> bool {
    false
}

pub unsafe extern "C" fn JS_NewEnumerateStandardClasses(_cx: *mut JSContext, _obj: crate::jsapi::HandleObject, _properties: crate::jsapi::MutableHandleIdVector, _enumerable_only: bool) -> bool {
    true
}

/// The window proxy registered for the realm whose global is `global` (a cell), if any.
pub(crate) fn window_proxy_of_global(global: *mut c_void) -> Option<*mut c_void> {
    let cx = crate::rust::Runtime::get()?;
    // SAFETY: the runtime outlives its scopes.
    let realms = unsafe { cx.as_ref() }.realms.try_borrow().ok()?;
    realms
        .iter()
        .find(|realm| realm.global.get() as *mut c_void == global)
        .map(|realm| realm.window_proxy.get() as *mut c_void)
        .filter(|proxy| !proxy.is_null())
}
