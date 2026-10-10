/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! SpiderMonkey proxies (mozjs's glue `ProxyTraps` handlers) as V8 `Proxy` objects.
//!
//! A proxy is a JS `Proxy` whose target is a class object of the proxy's `JSClass`. The
//! target's [`ClassBox`](crate::object::ClassBox) holds what SpiderMonkey keeps in the proxy:
//! the handler, the private value and the reserved slots. Its `cell` is the *proxy's* cell,
//! so the proxy keeps one identity, and the box's trace/finalize go to the handler's traps.
//! The target never reaches script: every operation goes through the V8 handler, whose
//! functions call the SpiderMonkey traps (or `BaseProxyHandler`'s defaults where a trap is
//! absent).
//!
//! V8 checks the ES proxy invariants on every trap result. SpiderMonkey's handlers report
//! non-configurable properties that the (empty) target does not have, which V8 rejects, so
//! such properties are mirrored onto the target when reported or defined.

use std::ffi::c_void;

use crate::glue::ProxyTraps;
use crate::jsapi::{
    HandleId, HandleObject, HandleValue, JSClass, JSContext, JSObject, MutableHandle,
    ObjectOpResult, PropertyDescriptor, jsid,
};
use crate::jsval::{JSVal, UndefinedValue, from_v8, to_v8};
use crate::object::ClassBox;

/// A proxy handler: mozjs's glue handler with its traps and the embedder's extra pointer.
pub(crate) struct ProxyHandler {
    pub(crate) traps: ProxyTraps,
    extra: *const c_void,
    family: *const c_void,
}

/// The family of every handler made by `CreateProxyHandler` (the DOM proxy family).
static HANDLER_FAMILY: u8 = 0;
/// The family of wrapper handlers (`CreateWrapperProxyHandler`): absent traps forward to the
/// wrapped object, the proxy's private.
static WRAPPER_FAMILY: u8 = 0;

/// The class of proxies created without one (SpiderMonkey's `js::ProxyClass`).
static PROXY_CLASS: JSClass = JSClass {
    name: c"Proxy".as_ptr(),
    flags: crate::object::JSCLASS_IS_PROXY | (1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
    cOps: std::ptr::null(),
    spec: std::ptr::null(),
    ext: std::ptr::null(),
    oOps: std::ptr::null(),
};

thread_local! {
    static DOM_PROXY_SHADOWS_CHECK: std::cell::Cell<crate::jsapi::DOMProxyShadowsCheck> = const { std::cell::Cell::new(None) };
}

pub unsafe fn CreateProxyHandler(traps: *const ProxyTraps, extra: *const c_void) -> *const c_void {
    // SAFETY: callers pass a valid trap table (copied: handlers outlive their tables).
    let handler = Box::new(ProxyHandler { traps: unsafe { *traps }, extra, family: GetProxyHandlerFamily() });
    // Handlers live for the whole process, as in mozjs (they are created once per binding).
    Box::into_raw(handler) as *const c_void
}

pub unsafe fn CreateWrapperProxyHandler(traps: *const ProxyTraps) -> *const c_void {
    // SAFETY: callers pass a valid trap table.
    let handler = Box::new(ProxyHandler { traps: unsafe { *traps }, extra: std::ptr::null(), family: &WRAPPER_FAMILY as *const u8 as *const c_void });
    Box::into_raw(handler) as *const c_void
}

pub unsafe fn DeleteWrapperProxyHandler(handler: *const c_void) {
    // SAFETY: handlers come from `CreateWrapperProxyHandler` and are no longer used.
    drop(unsafe { Box::from_raw(handler as *mut ProxyHandler) });
}

pub fn GetProxyHandlerFamily() -> *const c_void {
    &HANDLER_FAMILY as *const u8 as *const c_void
}

/// The box of a proxy (the target's box), if `obj` is a roves-js proxy.
fn proxy_box<'b>(obj: *mut JSObject) -> Option<&'b ClassBox> {
    let class_box = crate::object::class_box(obj)?;
    (!class_box.proxy.get().is_null()).then_some(class_box)
}

fn handler_of<'h>(obj: *mut JSObject) -> Option<&'h ProxyHandler> {
    // SAFETY: handlers live for the whole process.
    proxy_box(obj).map(|class_box| unsafe { &*class_box.proxy.get() })
}

pub unsafe fn IsProxyHandlerFamily(obj: *mut JSObject) -> bool {
    handler_of(obj).is_some_and(|handler| handler.family == GetProxyHandlerFamily())
}

pub unsafe fn GetProxyHandler(obj: *mut JSObject) -> *const c_void {
    proxy_box(obj).map_or(std::ptr::null(), |class_box| class_box.proxy.get() as *const c_void)
}

pub unsafe fn GetProxyHandlerExtra(obj: *mut JSObject) -> *const c_void {
    handler_of(obj).map_or(std::ptr::null(), |handler| handler.extra)
}

pub unsafe fn GetProxyPrivate(obj: *mut JSObject, dest: *mut JSVal) {
    let private = proxy_box(obj).map_or(UndefinedValue(), |class_box| class_box.private.get());
    // SAFETY: callers pass a valid out pointer.
    unsafe { *dest = private };
}

pub unsafe fn SetProxyPrivate(obj: *mut JSObject, value: *const JSVal) {
    if let Some(class_box) = proxy_box(obj) {
        // SAFETY: callers pass a valid value.
        class_box.private.set(unsafe { *value });
    }
}

pub unsafe fn GetProxyReservedSlot(obj: *mut JSObject, slot: u32, dest: *mut JSVal) {
    // SAFETY: forwarded (a proxy's reserved slots are its box's).
    unsafe { crate::jsapi_impl::JS_GetReservedSlot(obj, slot, dest) }
}

pub unsafe fn SetProxyReservedSlot(obj: *mut JSObject, slot: u32, value: *const JSVal) {
    // SAFETY: forwarded.
    unsafe { crate::jsapi_impl::JS_SetReservedSlot(obj, slot, value) }
}

/// SpiderMonkey's JIT hook for DOM proxies; V8 does not use it.
pub unsafe fn SetDOMProxyInformation(
    _dom_proxy_handler_family: *const c_void,
    shadows_check: crate::jsapi::DOMProxyShadowsCheck,
    _dom_remote_proxy_handler_family: *const c_void,
) {
    DOM_PROXY_SHADOWS_CHECK.with(|check| check.set(shadows_check));
}

/// A new proxy with `handler`, `private` and prototype `proto` (`lazy_proto`: the prototype
/// comes from the handler's `getPrototype` trap). `class` may be null (`PROXY_CLASS`).
pub unsafe fn NewProxyObject(
    cx: *mut JSContext,
    handler: *const c_void,
    private: HandleValue,
    proto: *mut JSObject,
    class: *const JSClass,
    lazy_proto: bool,
) -> *mut JSObject {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let class = if class.is_null() { &PROXY_CLASS as *const JSClass } else { class };
    let private = private.get();
    raw.catching(|scope| {
        let template = raw.class_templates.template(scope, class);
        let target = template.new_instance(scope)?;
        let prototype: v8::Local<v8::Value> = if proto.is_null() || lazy_proto {
            v8::null(scope).into()
        } else {
            // SAFETY: callers pass a live (rooted) prototype.
            unsafe { crate::cell::cell_value(scope, proto as *mut c_void) }
        };
        target.set_prototype(scope, prototype)?;
        let class_box = crate::object::make_class_box(scope, target, class);
        // SAFETY: just created, kept alive by the target.
        let box_ref = unsafe { class_box.as_ref() };
        box_ref.proxy.set(handler as *const ProxyHandler);
        box_ref.private.set(private);
        box_ref.lazy_proto.set(lazy_proto);
        let handler_object = handler_object(raw, scope, handler as *const ProxyHandler)?;
        let proxy = v8::Proxy::new(scope, target, handler_object)?;
        let member = v8::cppgc::Member::new(&class_box);
        let cell = crate::cell::new_cell_with_class(scope, proxy.into(), Some(member));
        box_ref.cell.set(cell);
        Some(cell as *mut JSObject)
    })
    .unwrap_or(std::ptr::null_mut())
}

/// The box behind a V8 proxy created by `NewProxyObject`.
pub(crate) fn proxy_box_of_v8<'b>(scope: &mut v8::PinScope, proxy: v8::Local<v8::Proxy>) -> Option<&'b ClassBox> {
    let target = v8::Local::<v8::Object>::try_from(proxy.get_target(scope)).ok()?;
    let class_box = crate::object::class_box_of_v8(target)?;
    (!class_box.proxy.get().is_null()).then_some(class_box)
}

/// Calls the handler's `getOwnPropertyDescriptor` trap (mozjs's glue helper).
pub unsafe fn InvokeGetOwnPropertyDescriptor(
    handler: *const c_void,
    cx: *mut JSContext,
    proxy: HandleObject,
    id: HandleId,
    desc: MutableHandle<PropertyDescriptor>,
    is_none: *mut bool,
) -> bool {
    // SAFETY: handlers come from `CreateProxyHandler`.
    let handler = unsafe { &*(handler as *const ProxyHandler) };
    match handler.traps.getOwnPropertyDescriptor {
        // SAFETY: SpiderMonkey's trap contract.
        Some(trap) => unsafe { trap(cx, proxy, id, desc, is_none) },
        None => {
            // SAFETY: callers pass a valid out pointer.
            unsafe { *is_none = true };
            true
        },
    }
}

// --- The V8 handler ------------------------------------------------------------------------

/// The V8 handler object of `handler` (one per handler and runtime).
fn handler_object<'s>(cx: &JSContext, scope: &mut v8::PinScope<'s, '_>, handler: *const ProxyHandler) -> Option<v8::Local<'s, v8::Object>> {
    if let Some(object) = cx.proxy_handlers.borrow().get(&(handler as usize)) {
        return Some(v8::Local::new(scope, object));
    }
    let object = v8::Object::new(scope);
    let data = v8::External::new(scope, handler as *mut c_void);
    // V8 callbacks must be function items (zero-sized), hence one call per trap.
    macro_rules! install {
        ($($name:literal => $callback:ident),* $(,)?) => {
            $(
                let function = v8::Function::builder($callback).data(data.into()).build(scope)?;
                let key = v8::String::new(scope, $name)?;
                object.set(scope, key.into(), function.into())?;
            )*
        };
    }
    install! {
        "getOwnPropertyDescriptor" => trap_get_own_property_descriptor,
        "defineProperty" => trap_define_property,
        "ownKeys" => trap_own_keys,
        "deleteProperty" => trap_delete_property,
        "has" => trap_has,
        "get" => trap_get,
        "set" => trap_set,
        "getPrototypeOf" => trap_get_prototype_of,
        "setPrototypeOf" => trap_set_prototype_of,
        "isExtensible" => trap_is_extensible,
        "preventExtensions" => trap_prevent_extensions,
    }
    cx.proxy_handlers.borrow_mut().insert(handler as usize, v8::Global::new(scope, object));
    Some(object)
}

/// What every trap callback needs: the handler, the proxy (its cell, rooted by the caller)
/// and the target.
struct TrapContext<'s, 'h> {
    handler: &'h ProxyHandler,
    proxy: *mut JSObject,
    target: v8::Local<'s, v8::Object>,
    cx: *mut JSContext,
}

fn trap_context<'s, 'h>(args: &v8::FunctionCallbackArguments<'s>) -> Option<TrapContext<'s, 'h>> {
    let target = v8::Local::<v8::Object>::try_from(args.get(0)).ok()?;
    let class_box = crate::object::class_box_of_v8(target)?;
    // The proxy's current handler (a transplant may have replaced the one the V8 handler
    // object was made for). SAFETY: handlers live for the whole process.
    let handler = unsafe { class_box.proxy.get().as_ref() }?;
    let cx = JSContext::current() as *const JSContext as *mut JSContext;
    Some(TrapContext { handler, proxy: class_box.cell.get() as *mut JSObject, target, cx })
}

const FORWARD_TRAP: &str = "(function (name, target, a, b, c) { return Reflect[name](target, a, b, c); })";

/// A wrapper handler's absent trap: `Reflect[name](wrapped, ...)` on the wrapped object.
/// Returns whether the trap was forwarded (its result is in `retval`, or an exception is
/// pending in V8).
fn forward<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context: &TrapContext<'s, '_>,
    name: &str,
    args: &v8::FunctionCallbackArguments<'s>,
    retval: &mut v8::ReturnValue<'s>,
) -> bool {
    if context.handler.family != &WRAPPER_FAMILY as *const u8 as *const c_void {
        return false;
    }
    let Some(class_box) = crate::object::class_box_of_v8(context.target) else { return false };
    let wrapped = class_box.private.get();
    if !wrapped.is_object() {
        return false;
    }
    // SAFETY: the private is traced by the box.
    let wrapped = unsafe { to_v8(scope, wrapped) };
    let Some(function) = crate::values_impl::helper(scope, "ForwardTrap", FORWARD_TRAP) else { return true };
    let Some(name) = v8::String::new(scope, name) else { return true };
    let undefined = v8::undefined(scope).into();
    let call_args = [name.into(), wrapped, args.get(1), args.get(2), args.get(3)];
    if let Some(result) = function.call(scope, undefined, &call_args) {
        retval.set(result);
    }
    true
}

/// Ends a trap: a SpiderMonkey failure rethrows the pending exception into V8.
fn fail(scope: &mut v8::PinScope) {
    crate::native::rethrow_pending(scope, JSContext::current());
}

/// A descriptor object (`{ value, writable, get, set, enumerable, configurable }`).
fn descriptor_object<'s>(scope: &mut v8::PinScope<'s, '_>, descriptor: &PropertyDescriptor) -> Option<v8::Local<'s, v8::Object>> {
    let object = v8::Object::new(scope);
    let mut put = |scope: &mut v8::PinScope<'s, '_>, name: &str, value: v8::Local<'s, v8::Value>| -> Option<()> {
        let key = v8::String::new(scope, name)?;
        object.create_data_property(scope, key.into(), value)?;
        Some(())
    };
    if descriptor.hasGetter_() || descriptor.hasSetter_() {
        let getter: v8::Local<v8::Value> =
            if descriptor.getter_.is_null() { v8::undefined(scope).into() } else { unsafe { to_v8(scope, crate::jsval::ObjectValue(descriptor.getter_)) } };
        let setter: v8::Local<v8::Value> =
            if descriptor.setter_.is_null() { v8::undefined(scope).into() } else { unsafe { to_v8(scope, crate::jsval::ObjectValue(descriptor.setter_)) } };
        put(scope, "get", getter)?;
        put(scope, "set", setter)?;
    } else {
        // SAFETY: descriptor values are rooted by the caller.
        let value = unsafe { to_v8(scope, descriptor.value_) };
        put(scope, "value", value)?;
        let writable = v8::Boolean::new(scope, descriptor.writable_());
        put(scope, "writable", writable.into())?;
    }
    let enumerable = v8::Boolean::new(scope, descriptor.enumerable_());
    put(scope, "enumerable", enumerable.into())?;
    let configurable = v8::Boolean::new(scope, descriptor.configurable_());
    put(scope, "configurable", configurable.into())?;
    Some(object)
}

/// Mirrors a non-configurable property onto the target (V8's proxy invariants).
fn mirror_non_configurable(scope: &mut v8::PinScope, target: v8::Local<v8::Object>, key: v8::Local<v8::Name>, descriptor: &PropertyDescriptor) {
    if descriptor.configurable_() {
        return;
    }
    let mirrored = crate::jsapi_impl::v8_descriptor(scope, descriptor);
    let _ = target.define_property(scope, key, &mirrored);
}

/// The own descriptor of `id` through the handler (`None` when absent, `Err` on failure).
fn own_descriptor(context: &TrapContext, id: jsid) -> Result<Option<PropertyDescriptor>, ()> {
    let Some(trap) = context.handler.traps.getOwnPropertyDescriptor else { return Ok(None) };
    let cx = context.cx;
    crate::rooted!(in(cx) let proxy = context.proxy);
    crate::rooted!(in(cx) let id = id);
    crate::rooted!(in(cx) let mut descriptor = PropertyDescriptor::default());
    let mut is_none = true;
    // SAFETY: SpiderMonkey's trap contract (rooted arguments).
    let ok = unsafe { trap(cx, proxy.handle().into_handle(), id.handle().into_handle(), descriptor.handle_mut().into_handle(), &mut is_none) };
    if !ok {
        return Err(());
    }
    Ok((!is_none).then(|| descriptor.get()))
}

fn trap_get_own_property_descriptor<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.getOwnPropertyDescriptor.is_none() && forward(scope, &context, "getOwnPropertyDescriptor", &args, &mut retval) {
        if let (Ok(key), Some(descriptor)) = (v8::Local::<v8::Name>::try_from(args.get(1)), v8::Local::<v8::Object>::try_from(retval.get(scope)).ok()) {
            let mut converted = PropertyDescriptor::default();
            crate::jsapi_impl::fill_descriptor(scope, descriptor, &mut converted);
            mirror_non_configurable(scope, context.target, key, &converted);
        }
        return;
    }
    let Ok(key) = v8::Local::<v8::Name>::try_from(args.get(1)) else { return };
    let id = crate::jsapi_impl::key_id(scope, key.into());
    match own_descriptor(&context, id) {
        Err(()) => fail(scope),
        Ok(None) => retval.set_undefined(),
        Ok(Some(descriptor)) => {
            mirror_non_configurable(scope, context.target, key, &descriptor);
            if let Some(object) = descriptor_object(scope, &descriptor) {
                retval.set(object.into());
            }
        },
    }
}

fn result_to_bool(ok: bool, result: &ObjectOpResult, retval: &mut v8::ReturnValue, scope: &mut v8::PinScope) {
    if ok {
        retval.set_bool(result.ok());
    } else {
        fail(scope);
    }
}

fn trap_define_property<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.defineProperty.is_none() && forward(scope, &context, "defineProperty", &args, &mut retval) {
        return;
    }
    let Ok(key) = v8::Local::<v8::Name>::try_from(args.get(1)) else { return };
    let Ok(descriptor_value) = v8::Local::<v8::Object>::try_from(args.get(2)) else { return };
    let Some(trap) = context.handler.traps.defineProperty else {
        retval.set_bool(false);
        return;
    };
    let mut descriptor = PropertyDescriptor::default();
    crate::jsapi_impl::fill_descriptor(scope, descriptor_value, &mut descriptor);
    let cx = context.cx;
    crate::rooted!(in(cx) let proxy = context.proxy);
    crate::rooted!(in(cx) let id = crate::jsapi_impl::key_id(scope, key.into()));
    crate::rooted!(in(cx) let rooted_descriptor = descriptor);
    let mut result = ObjectOpResult { code_: 0 };
    // SAFETY: SpiderMonkey's trap contract.
    let ok = unsafe { trap(cx, proxy.handle().into_handle(), id.handle().into_handle(), rooted_descriptor.handle().into_handle(), &mut result) };
    if ok && result.ok() && descriptor.hasConfigurable_() {
        mirror_non_configurable(scope, context.target, key, &descriptor);
    }
    result_to_bool(ok, &result, &mut retval, scope);
}

fn trap_own_keys<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.ownPropertyKeys.is_none() && forward(scope, &context, "ownKeys", &args, &mut retval) {
        return;
    }
    let cx = context.cx;
    // SAFETY: `cx` is this thread's live context.
    let mut ids = unsafe { crate::rust::IdVector::new(cx) };
    if let Some(trap) = context.handler.traps.ownPropertyKeys {
        crate::rooted!(in(cx) let proxy = context.proxy);
        // SAFETY: SpiderMonkey's trap contract.
        if !unsafe { trap(cx, proxy.handle().into_handle(), ids.handle_mut()) } {
            fail(scope);
            return;
        }
    }
    let keys: Vec<v8::Local<v8::Value>> = ids.iter().filter_map(|id| crate::jsapi_impl::id_key(scope, *id).map(Into::into)).collect();
    let array = v8::Array::new_with_elements(scope, &keys);
    retval.set(array.into());
}

fn trap_delete_property<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.delete_.is_none() && forward(scope, &context, "deleteProperty", &args, &mut retval) {
        return;
    }
    let Some(trap) = context.handler.traps.delete_ else {
        retval.set_bool(true);
        return;
    };
    let cx = context.cx;
    crate::rooted!(in(cx) let proxy = context.proxy);
    crate::rooted!(in(cx) let id = crate::jsapi_impl::key_id(scope, args.get(1)));
    let mut result = ObjectOpResult { code_: 0 };
    // SAFETY: SpiderMonkey's trap contract.
    let ok = unsafe { trap(cx, proxy.handle().into_handle(), id.handle().into_handle(), &mut result) };
    result_to_bool(ok, &result, &mut retval, scope);
}

/// The prototype the proxy reports (its handler's, for lazy prototypes).
fn prototype<'s>(scope: &mut v8::PinScope<'s, '_>, context: &TrapContext<'s, '_>) -> Result<v8::Local<'s, v8::Value>, ()> {
    let lazy = crate::object::class_box_of_v8(context.target).is_some_and(|class_box| class_box.lazy_proto.get());
    if lazy {
        if let Some(trap) = context.handler.traps.getPrototype {
            let cx = context.cx;
            crate::rooted!(in(cx) let proxy = context.proxy);
            crate::rooted!(in(cx) let mut proto = std::ptr::null_mut::<JSObject>());
            // SAFETY: SpiderMonkey's trap contract.
            if !unsafe { trap(cx, proxy.handle().into_handle(), proto.handle_mut().into_handle()) } {
                return Err(());
            }
            return Ok(if proto.get().is_null() {
                v8::null(scope).into()
            } else {
                // SAFETY: rooted above.
                unsafe { to_v8(scope, crate::jsval::ObjectValue(proto.get())) }
            });
        }
    }
    if lazy && context.handler.family == &WRAPPER_FAMILY as *const u8 as *const c_void {
        if let Some(class_box) = crate::object::class_box_of_v8(context.target) {
            let wrapped = class_box.private.get();
            if wrapped.is_object() {
                // SAFETY: the private is traced by the box.
                let wrapped = unsafe { to_v8(scope, wrapped) };
                if let Ok(wrapped) = v8::Local::<v8::Object>::try_from(wrapped) {
                    return wrapped.get_prototype(scope).ok_or(());
                }
            }
        }
    }
    context.target.get_prototype(scope).ok_or(())
}

fn trap_get_prototype_of<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.getPrototype.is_none() && forward(scope, &context, "getPrototypeOf", &args, &mut retval) {
        return;
    }
    match prototype(scope, &context) {
        Ok(prototype) => retval.set(prototype),
        Err(()) => fail(scope),
    }
}

fn trap_set_prototype_of<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.setPrototype.is_none() && forward(scope, &context, "setPrototypeOf", &args, &mut retval) {
        return;
    }
    match context.handler.traps.setPrototype {
        Some(trap) => {
            let cx = context.cx;
            crate::rooted!(in(cx) let proxy = context.proxy);
            let proto = args.get(1);
            crate::rooted!(in(cx) let proto = if proto.is_object() { from_v8(scope, proto).to_object() } else { std::ptr::null_mut() });
            let mut result = ObjectOpResult { code_: 0 };
            // SAFETY: SpiderMonkey's trap contract.
            let ok = unsafe { trap(cx, proxy.handle().into_handle(), proto.handle().into_handle(), &mut result) };
            result_to_bool(ok, &result, &mut retval, scope);
        },
        None => {
            let set = context.target.set_prototype(scope, args.get(1)).unwrap_or(false);
            retval.set_bool(set);
        },
    }
}

fn trap_is_extensible<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    // V8 requires the answer to match the target's; SpiderMonkey's DOM proxies are always
    // extensible, as is the target unless `preventExtensions` succeeded.
    if let Some(extensible) = reflect(scope, "isExtensible", &[context.target.into()]) {
        let extensible = extensible.boolean_value(scope);
        retval.set_bool(extensible);
    }
}

fn trap_prevent_extensions<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.preventExtensions.is_none() && forward(scope, &context, "preventExtensions", &args, &mut retval) {
        return;
    }
    match context.handler.traps.preventExtensions {
        Some(trap) => {
            let cx = context.cx;
            crate::rooted!(in(cx) let proxy = context.proxy);
            let mut result = ObjectOpResult { code_: 0 };
            // SAFETY: SpiderMonkey's trap contract.
            let ok = unsafe { trap(cx, proxy.handle().into_handle(), &mut result) };
            if ok && result.ok() {
                let _ = reflect(scope, "preventExtensions", &[context.target.into()]);
            }
            result_to_bool(ok, &result, &mut retval, scope);
        },
        None => {
            if let Some(prevented) = reflect(scope, "preventExtensions", &[context.target.into()]) {
                let prevented = prevented.boolean_value(scope);
                retval.set_bool(prevented);
            }
        },
    }
}

fn trap_has<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.has.is_none() && forward(scope, &context, "has", &args, &mut retval) {
        return;
    }
    let key = args.get(1);
    let id = crate::jsapi_impl::key_id(scope, key);
    if let Some(trap) = context.handler.traps.has {
        let cx = context.cx;
        crate::rooted!(in(cx) let proxy = context.proxy);
        crate::rooted!(in(cx) let id = id);
        let mut found = false;
        // SAFETY: SpiderMonkey's trap contract.
        if unsafe { trap(cx, proxy.handle().into_handle(), id.handle().into_handle(), &mut found) } {
            retval.set_bool(found);
        } else {
            fail(scope);
        }
        return;
    }
    // BaseProxyHandler::has: an own property, else the prototype chain.
    match own_descriptor(&context, id) {
        Err(()) => fail(scope),
        Ok(Some(_)) => retval.set_bool(true),
        Ok(None) => match prototype(scope, &context) {
            Err(()) => fail(scope),
            Ok(proto) => {
                let found = v8::Local::<v8::Object>::try_from(proto).ok().and_then(|proto| proto.has(scope, key));
                match found {
                    Some(found) => retval.set_bool(found),
                    None => retval.set_bool(false),
                }
            },
        },
    }
}

fn trap_get<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.get.is_none() && forward(scope, &context, "get", &args, &mut retval) {
        return;
    }
    let key = args.get(1);
    let receiver = args.get(2);
    let id = crate::jsapi_impl::key_id(scope, key);
    let cx = context.cx;
    if let Some(trap) = context.handler.traps.get {
        crate::rooted!(in(cx) let proxy = context.proxy);
        crate::rooted!(in(cx) let id = id);
        crate::rooted!(in(cx) let receiver = from_v8(scope, receiver));
        crate::rooted!(in(cx) let mut value = UndefinedValue());
        // SAFETY: SpiderMonkey's trap contract.
        let ok = unsafe { trap(cx, proxy.handle().into_handle(), receiver.handle().into_handle(), id.handle().into_handle(), value.handle_mut().into_handle()) };
        if ok {
            // SAFETY: rooted above.
            let value = unsafe { to_v8(scope, value.get()) };
            retval.set(value);
        } else {
            fail(scope);
        }
        return;
    }
    // BaseProxyHandler::get: an own property (value or getter), else the prototype chain.
    match own_descriptor(&context, id) {
        Err(()) => fail(scope),
        Ok(Some(descriptor)) => {
            crate::rooted!(in(cx) let descriptor = descriptor);
            if descriptor.hasGetter_() || descriptor.hasSetter_() {
                if descriptor.getter_.is_null() {
                    retval.set_undefined();
                    return;
                }
                // SAFETY: rooted above.
                let getter = unsafe { to_v8(scope, crate::jsval::ObjectValue(descriptor.getter_)) };
                let Ok(getter) = v8::Local::<v8::Function>::try_from(getter) else { return };
                if let Some(value) = getter.call(scope, receiver, &[]) {
                    retval.set(value);
                }
            } else {
                // SAFETY: rooted above.
                let value = unsafe { to_v8(scope, descriptor.value_) };
                retval.set(value);
            }
        },
        Ok(None) => match prototype(scope, &context) {
            Err(()) => fail(scope),
            Ok(proto) => {
                let Ok(proto) = v8::Local::<v8::Object>::try_from(proto) else {
                    retval.set_undefined();
                    return;
                };
                if let Some(value) = reflect(scope, "get", &[proto.into(), key, receiver]) {
                    retval.set(value);
                }
            },
        },
    }
}

/// Calls `Reflect[name](...args)`.
fn reflect<'s>(scope: &mut v8::PinScope<'s, '_>, name: &str, args: &[v8::Local<'s, v8::Value>]) -> Option<v8::Local<'s, v8::Value>> {
    let global = scope.get_current_context().global(scope);
    let reflect_key = v8::String::new(scope, "Reflect")?;
    let reflect = v8::Local::<v8::Object>::try_from(global.get(scope, reflect_key.into())?).ok()?;
    let key = v8::String::new(scope, name)?;
    let function = v8::Local::<v8::Function>::try_from(reflect.get(scope, key.into())?).ok()?;
    function.call(scope, reflect.into(), args)
}

fn trap_set<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut retval: v8::ReturnValue<'s>) {
    let Some(context) = trap_context(&args) else { return };
    if context.handler.traps.set.is_none() && forward(scope, &context, "set", &args, &mut retval) {
        return;
    }
    let key = args.get(1);
    let value = args.get(2);
    let receiver = args.get(3);
    let id = crate::jsapi_impl::key_id(scope, key);
    let cx = context.cx;
    crate::rooted!(in(cx) let proxy = context.proxy);
    crate::rooted!(in(cx) let rooted_id = id);
    crate::rooted!(in(cx) let rooted_value = from_v8(scope, value));
    crate::rooted!(in(cx) let rooted_receiver = from_v8(scope, receiver));
    let mut result = ObjectOpResult { code_: 0 };
    if let Some(trap) = context.handler.traps.set {
        // SAFETY: SpiderMonkey's trap contract.
        let ok = unsafe {
            trap(
                cx,
                proxy.handle().into_handle(),
                rooted_id.handle().into_handle(),
                rooted_value.handle().into_handle(),
                rooted_receiver.handle().into_handle(),
                &mut result,
            )
        };
        result_to_bool(ok, &result, &mut retval, scope);
        return;
    }
    // BaseProxyHandler::set: OrdinarySetWithOwnDescriptor with the handler's own descriptor.
    match own_descriptor(&context, id) {
        Err(()) => fail(scope),
        Ok(Some(descriptor)) => {
            crate::rooted!(in(cx) let descriptor = descriptor);
            let own = descriptor.handle().into_handle();
            // SAFETY: rooted arguments.
            let ok = unsafe {
                SetPropertyIgnoringNamedGetter(
                    cx,
                    proxy.handle().into_handle(),
                    rooted_id.handle().into_handle(),
                    rooted_value.handle().into_handle(),
                    rooted_receiver.handle().into_handle(),
                    &own,
                    &mut result,
                )
            };
            result_to_bool(ok, &result, &mut retval, scope);
        },
        Ok(None) => match prototype(scope, &context) {
            Err(()) => fail(scope),
            Ok(proto) => {
                let set = match v8::Local::<v8::Object>::try_from(proto) {
                    Ok(proto) => reflect(scope, "set", &[proto.into(), key, value, receiver]).map(|set| set.boolean_value(scope)),
                    // No prototype: a new data property on the receiver.
                    Err(_) => v8::Local::<v8::Object>::try_from(receiver)
                        .ok()
                        .and_then(|receiver| v8::Local::<v8::Name>::try_from(key).ok().and_then(|key| receiver.create_data_property(scope, key, value))),
                };
                if let Some(set) = set {
                    retval.set_bool(set);
                }
            },
        },
    }
}

/// `OrdinarySetWithOwnDescriptor` with `ownDesc` (SpiderMonkey's helper for proxy `set`
/// traps whose own lookup must skip named getters).
pub unsafe fn SetPropertyIgnoringNamedGetter(
    cx: *mut JSContext,
    obj: HandleObject,
    id: HandleId,
    v: HandleValue,
    receiver: HandleValue,
    own_desc: *const crate::jsapi::Handle<PropertyDescriptor>,
    result: *mut ObjectOpResult,
) -> bool {
    // SAFETY: callers pass valid, rooted arguments.
    let result = unsafe { &mut *result };
    let raw = unsafe { &*cx };
    if own_desc.is_null() {
        // No own property: OrdinarySet continues on the prototype (or defines on the receiver).
        let outcome = raw.catching(|scope| {
            let target = unsafe { crate::cell::cell_value(scope, obj.get() as *mut c_void) };
            let target = v8::Local::<v8::Object>::try_from(target).ok()?;
            let key: v8::Local<v8::Value> = crate::jsapi_impl::id_key(scope, id.get())?.into();
            let value = unsafe { to_v8(scope, v.get()) };
            let receiver = unsafe { to_v8(scope, receiver.get()) };
            let proto = crate::jsapi_impl::prototype_of(scope, target)?;
            if proto.is_object() {
                return reflect(scope, "set", &[proto, key, value, receiver]).map(|set| set.boolean_value(scope));
            }
            let receiver = v8::Local::<v8::Object>::try_from(receiver).ok()?;
            receiver.create_data_property(scope, v8::Local::<v8::Name>::try_from(key).ok()?, value)
        });
        return match outcome {
            Some(true) => result.succeed(),
            Some(false) => result.fail_read_only(),
            None => false,
        };
    }
    let descriptor = unsafe { *(*own_desc).ptr };
    let outcome = raw.catching(|scope| {
        if descriptor.hasGetter_() || descriptor.hasSetter_() {
            if descriptor.setter_.is_null() {
                return Some(false);
            }
            // SAFETY: rooted by the caller's descriptor.
            let setter = unsafe { to_v8(scope, crate::jsval::ObjectValue(descriptor.setter_)) };
            let setter = v8::Local::<v8::Function>::try_from(setter).ok()?;
            let receiver = unsafe { to_v8(scope, receiver.get()) };
            let value = unsafe { to_v8(scope, v.get()) };
            setter.call(scope, receiver, &[value])?;
            return Some(true);
        }
        if descriptor.hasWritable_() && !descriptor.writable_() {
            return Some(false);
        }
        let receiver = unsafe { to_v8(scope, receiver.get()) };
        let Ok(receiver) = v8::Local::<v8::Object>::try_from(receiver) else { return Some(false) };
        let key = crate::jsapi_impl::id_key(scope, id.get())?;
        let value = unsafe { to_v8(scope, v.get()) };
        // An existing own property of the receiver keeps its attributes.
        let existing = receiver.get_own_property_descriptor(scope, key)?;
        if let Ok(existing) = v8::Local::<v8::Object>::try_from(existing) {
            let mut current = PropertyDescriptor::default();
            crate::jsapi_impl::fill_descriptor(scope, existing, &mut current);
            if current.hasGetter_() || current.hasSetter_() || (current.hasWritable_() && !current.writable_()) {
                return Some(false);
            }
            let update = v8::PropertyDescriptor::new_from_value(value);
            return receiver.define_property(scope, key, &update);
        }
        receiver.create_data_property(scope, key, value)
    });
    match outcome {
        Some(true) => {
            result.succeed();
            true
        },
        Some(false) => {
            result.fail_read_only();
            true
        },
        None => false,
    }
}

// --- Window proxies --------------------------------------------------------------------------

thread_local! {
    static WINDOW_PROXY_CLASS: std::cell::Cell<*const JSClass> = const { std::cell::Cell::new(std::ptr::null()) };
}

/// The class of window proxies (SpiderMonkey's default when the embedder sets none).
static DEFAULT_WINDOW_PROXY_CLASS: JSClass = JSClass {
    name: c"WindowProxy".as_ptr(),
    flags: crate::object::JSCLASS_IS_PROXY | (1 << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
    cOps: std::ptr::null(),
    spec: std::ptr::null(),
    ext: std::ptr::null(),
    oOps: std::ptr::null(),
};

pub unsafe fn SetWindowProxyClass(_cx: *mut JSContext, clasp: *const JSClass) {
    WINDOW_PROXY_CLASS.with(|class| class.set(clasp));
}

pub fn GetWindowProxyClass() -> *const JSClass {
    let class = WINDOW_PROXY_CLASS.with(std::cell::Cell::get);
    if class.is_null() { &DEFAULT_WINDOW_PROXY_CLASS } else { class }
}

/// A window proxy for `obj`.
///
/// For a global (a V8 context's global, as a Window is), the window proxy is V8's own global
/// proxy, the object scripts see as `globalThis`: a second cell on it carries the proxy state
/// (handler, private = the window, reserved slots), and conversions of the global proxy yield
/// that cell once `SetWindowProxy` registers it, so `window === globalThis`. Script property
/// accesses use V8's global object semantics; the handler's traps are not run. For other
/// objects it is a wrapper proxy whose private is `obj`.
pub unsafe fn NewWindowProxy(cx: *mut JSContext, obj: HandleObject, handler: *const c_void) -> *mut JSObject {
    let window = obj.get();
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let realm = crate::realm_impl::realm_of_object(raw, window);
    let is_global = crate::realm_impl::realm_data(realm).is_some_and(|data| data.global.get() == window);
    if !is_global {
        crate::rooted!(in(cx) let window = crate::jsval::ObjectValue(window));
        // SAFETY: forwarded; the prototype comes from the window (lazy, forwarded).
        return unsafe { NewProxyObject(cx, handler, window.handle().into(), std::ptr::null_mut(), GetWindowProxyClass(), true) };
    }
    raw.with_scope(|scope| {
        // SAFETY: a live global cell; its value is the V8 global proxy.
        let global_proxy = unsafe { crate::cell::cell_value(scope, window as *mut c_void) };
        let class_box = crate::object::new_class_box(scope, GetWindowProxyClass());
        // SAFETY: just created; the cell below keeps it alive.
        let box_ref = unsafe { class_box.as_ref() };
        box_ref.proxy.set(handler as *const ProxyHandler);
        box_ref.private.set(crate::jsval::ObjectValue(window));
        box_ref.lazy_proto.set(true);
        let cell = crate::cell::new_cell_with_class(scope, global_proxy, Some(v8::cppgc::Member::new(&class_box)));
        box_ref.cell.set(cell);
        cell as *mut JSObject
    })
}

pub unsafe fn IsWindowProxy(obj: *mut JSObject) -> bool {
    proxy_box(obj).is_some_and(|class_box| class_box.class == GetWindowProxyClass())
}

/// Records `window_proxy` as the proxy of the window `global`.
pub unsafe fn SetWindowProxy(cx: *mut JSContext, global: crate::jsapi::Handle<*mut JSObject>, window_proxy: crate::jsapi::Handle<*mut JSObject>) {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let realm = crate::realm_impl::realm_of_object(raw, global.get());
    if let Some(data) = crate::realm_impl::realm_data(realm) {
        data.window_proxy.set(window_proxy.get());
    }
}

/// Whether `obj` is a global with a window proxy.
pub unsafe fn IsWindowSlow(obj: *mut JSObject) -> bool {
    // SAFETY: forwarded.
    let outer = unsafe { ToWindowProxyIfWindowSlow(obj) };
    outer != obj
}

pub unsafe fn ToWindowProxyIfWindowSlow(obj: *mut JSObject) -> *mut JSObject {
    let cx = JSContext::current();
    let realm = crate::realm_impl::realm_of_object(cx, obj);
    match crate::realm_impl::realm_data(realm) {
        Some(data) if data.global.get() == obj && !data.window_proxy.get().is_null() => data.window_proxy.get(),
        _ => obj,
    }
}

pub unsafe fn ToWindowIfWindowProxy(obj: *mut JSObject) -> *mut JSObject {
    // SAFETY: forwarded.
    if unsafe { IsWindowProxy(obj) } {
        if let Some(class_box) = proxy_box(obj) {
            let window = class_box.private.get();
            if window.is_object() {
                return window.to_object();
            }
        }
    }
    obj
}

/// SpiderMonkey swaps `origobj`'s identity onto `target`. V8 objects cannot swap identity,
/// so `origobj` takes `target`'s proxy state (handler, private, slots) and keeps its own
/// identity, which is what callers observe; `origobj` is returned.
pub unsafe fn JS_TransplantObject(_cx: *mut JSContext, origobj: HandleObject, target: HandleObject) -> *mut JSObject {
    let (original, replacement) = (origobj.get(), target.get());
    let (Some(original_box), Some(replacement_box)) = (proxy_box(original), proxy_box(replacement)) else {
        return std::ptr::null_mut();
    };
    original_box.proxy.set(replacement_box.proxy.get());
    original_box.private.set(replacement_box.private.get());
    original_box.lazy_proto.set(replacement_box.lazy_proto.get());
    original_box.copy_slots_from(replacement_box);
    original
}

// --- Proxy class statics -------------------------------------------------------------------
//
// SpiderMonkey's proxy classes use these; roves-js proxies take their behaviour from the
// handler, so they are empty.

pub static ProxyClassOps: crate::jsapi::JSClassOps = crate::jsapi::JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: None,
    mayResolve: None,
    finalize: None,
    call: None,
    construct: None,
    trace: None,
};

pub static ProxyClassExtension: crate::jsapi::ClassExtension = crate::jsapi::ClassExtension { _private: [] };

pub static ProxyObjectOps: crate::jsapi::ObjectOps = crate::jsapi::ObjectOps {
    lookupProperty: None,
    defineProperty: None,
    hasProperty: None,
    getProperty: None,
    setProperty: None,
    getOwnPropertyDescriptor: None,
    deleteProperty: None,
    getElements: None,
    funToString: None,
};
