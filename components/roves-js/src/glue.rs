/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! mozjs's C++ glue, implemented on V8. The tracer calls receive the cppgc visitor of the
//! running GC as their `*mut JSTracer` (roves-js passes it in every trace hook).

use std::ffi::{c_char, c_void};

use v8::cppgc::Visitor;

use crate::cell::trace_cell;
use crate::gc::Heap;
use crate::jsapi::{BigInt, JSFunction, JSObject, JSScript, JSString, JSTracer, PropertyDescriptor, Symbol, jsid};
use crate::jsval::Value;

pub use crate::binding::{CallJitGetterOp, CallJitMethodOp, CallJitSetterOp};
pub use crate::typedarray_impl::{
    GetFloat32ArrayLengthAndData, GetFloat64ArrayLengthAndData, GetInt16ArrayLengthAndData, GetInt32ArrayLengthAndData,
    GetInt8ArrayLengthAndData, GetUint16ArrayLengthAndData, GetUint32ArrayLengthAndData, GetUint8ArrayLengthAndData,
    GetUint8ClampedArrayLengthAndData,
};
pub use crate::proxy::{
    CreateProxyHandler, GetProxyHandler, GetProxyHandlerExtra, GetProxyHandlerFamily, GetProxyPrivate,
    GetProxyReservedSlot, InvokeGetOwnPropertyDescriptor, IsProxyHandlerFamily, NewProxyObject, SetProxyPrivate,
    SetProxyReservedSlot,
};
pub use crate::jsapi_impl::{
    AppendToIdVector, CheckedUnwrapStatic, CreateRustJSPrincipals, DestroyRustJSPrincipals,
    GetRustJSPrincipalsPrivate, IsWrapper, JS_GetReservedSlot, RUST_FUNCTION_VALUE_TO_JITINFO,
    RUST_INTERNED_STRING_TO_JSID, RUST_JSID_IS_VOID, RUST_SYMBOL_TO_JSID, SetDataPropertyDescriptor,
    UncheckedUnwrapObject, UnwrapObjectDynamic, UnwrapObjectStatic, int_to_jsid,
};

use crate::jsapi::{
    BaseProxyHandler_Action, CallArgs, ESClass, GCContext, HandleId, HandleObject, HandleValue, IsAcceptableThis,
    JSContext, JSType, MutableHandle, MutableHandleIdVector, MutableHandleObject, MutableHandleValue, NativeImpl,
    ObjectOpResult,
};
#[allow(unused_imports)]
use crate::jsapi::Handle;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
/// The traps of a proxy handler (mozjs's glue `ProxyTraps`), called by roves-js's V8 `Proxy`
/// handlers (see `proxy`).
pub struct ProxyTraps {
    pub enter: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            action: BaseProxyHandler_Action,
            bp: *mut bool,
        ) -> bool,
    >,
    pub getOwnPropertyDescriptor: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            desc: MutableHandle<PropertyDescriptor>,
            isNone: *mut bool,
        ) -> bool,
    >,
    pub defineProperty: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            desc: Handle<PropertyDescriptor>,
            result: *mut ObjectOpResult,
        ) -> bool,
    >,
    pub ownPropertyKeys: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            props: MutableHandleIdVector,
        ) -> bool,
    >,
    pub delete_: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            result: *mut ObjectOpResult,
        ) -> bool,
    >,
    pub enumerate: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            props: MutableHandleIdVector,
        ) -> bool,
    >,
    pub getPrototypeIfOrdinary: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            isOrdinary: *mut bool,
            protop: MutableHandleObject,
        ) -> bool,
    >,
    pub getPrototype: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            protop: MutableHandleObject,
        ) -> bool,
    >,
    pub setPrototype: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            proto: HandleObject,
            result: *mut ObjectOpResult,
        ) -> bool,
    >,
    pub setImmutablePrototype: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            succeeded: *mut bool,
        ) -> bool,
    >,
    pub preventExtensions: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            result: *mut ObjectOpResult,
        ) -> bool,
    >,
    pub isExtensible: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            succeeded: *mut bool,
        ) -> bool,
    >,
    pub has: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            bp: *mut bool,
        ) -> bool,
    >,
    pub get: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            receiver: HandleValue,
            id: HandleId,
            vp: MutableHandleValue,
        ) -> bool,
    >,
    pub set: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            v: HandleValue,
            receiver: HandleValue,
            result: *mut ObjectOpResult,
        ) -> bool,
    >,
    pub call: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            args: *const CallArgs,
        ) -> bool,
    >,
    pub construct: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            args: *const CallArgs,
        ) -> bool,
    >,
    pub hasOwn: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            id: HandleId,
            bp: *mut bool,
        ) -> bool,
    >,
    pub getOwnEnumerablePropertyKeys: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            props: MutableHandleIdVector,
        ) -> bool,
    >,
    pub nativeCall: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            test: IsAcceptableThis,
            impl_: NativeImpl,
            args: CallArgs,
        ) -> bool,
    >,
    pub objectClassIs: ::std::option::Option<
        unsafe extern "C" fn(
            obj: HandleObject,
            classValue: ESClass,
            cx: *mut JSContext,
        ) -> bool,
    >,
    pub className: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
        ) -> *const std::os::raw::c_char,
    >,
    pub fun_toString: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            isToString: bool,
        ) -> *mut JSString,
    >,
    pub boxedValue_unbox: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            proxy: HandleObject,
            vp: MutableHandleValue,
        ) -> bool,
    >,
    pub defaultValue: ::std::option::Option<
        unsafe extern "C" fn(
            cx: *mut JSContext,
            obj: HandleObject,
            hint: JSType,
            vp: MutableHandleValue,
        ) -> bool,
    >,
    pub trace: ::std::option::Option<
        unsafe extern "C" fn(trc: *mut JSTracer, proxy: *mut JSObject),
    >,
    pub finalize: ::std::option::Option<
        unsafe extern "C" fn(cx: *mut GCContext, proxy: *mut JSObject),
    >,
    pub objectMoved: ::std::option::Option<
        unsafe extern "C" fn(proxy: *mut JSObject, old: *mut JSObject) -> usize,
    >,
    pub isCallable:
        ::std::option::Option<unsafe extern "C" fn(obj: *mut JSObject) -> bool>,
    pub isConstructor:
        ::std::option::Option<unsafe extern "C" fn(obj: *mut JSObject) -> bool>,
}

/// The principal callbacks of mozjs's `RustJSPrincipals` (structured-clone writing and the
/// system-principal check).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct JSPrincipalsCallbacks {
    pub write: Option<
        unsafe extern "C" fn(
            principals: *mut crate::jsapi::JSPrincipals,
            cx: *mut crate::jsapi::JSContext,
            writer: *mut crate::jsapi::JSStructuredCloneWriter,
        ) -> bool,
    >,
    pub isSystemOrAddonPrincipal: Option<unsafe extern "C" fn(principals: *mut crate::jsapi::JSPrincipals) -> bool>,
}

/// The cppgc visitor behind a tracer pointer.
///
/// # Safety
/// `trc` must be a tracer handed out by roves-js during a GC (a cppgc `Visitor`).
pub(crate) unsafe fn visitor<'v>(trc: *mut JSTracer) -> &'v mut Visitor {
    // SAFETY: forwarded to the caller.
    unsafe { &mut *(trc as *mut Visitor) }
}

/// The tracer pointer roves-js passes to trace hooks for `visitor`.
pub(crate) fn tracer(visitor: &mut Visitor) -> *mut JSTracer {
    visitor as *mut Visitor as *mut JSTracer
}

fn trace_value(trc: *mut JSTracer, value: Value) {
    if value.is_gcthing() {
        // SAFETY: tracer contract.
        trace_cell(value.to_gcthing(), unsafe { visitor(trc) });
    }
}

fn trace_pointer(trc: *mut JSTracer, pointer: *mut c_void) {
    // SAFETY: tracer contract.
    trace_cell(pointer, unsafe { visitor(trc) });
}

macro_rules! heap_tracer {
    ($name:ident, $ty:ty) => {
        /// # Safety
        /// `trc` is a roves-js tracer and `location` a live heap location.
        pub unsafe fn $name(trc: *mut JSTracer, location: *mut Heap<*mut $ty>, _name: *const c_char) {
            // SAFETY: forwarded to the caller.
            trace_pointer(trc, unsafe { (*location).get() } as *mut c_void);
        }
    };
}

heap_tracer!(CallObjectTracer, JSObject);
heap_tracer!(CallStringTracer, JSString);
heap_tracer!(CallFunctionTracer, JSFunction);
heap_tracer!(CallSymbolTracer, Symbol);
heap_tracer!(CallBigIntTracer, BigInt);
heap_tracer!(CallScriptTracer, JSScript);

/// # Safety
/// `trc` is a roves-js tracer and `location` a live heap location.
pub unsafe fn CallValueTracer(trc: *mut JSTracer, location: *mut Heap<Value>, _name: *const c_char) {
    // SAFETY: forwarded to the caller.
    trace_value(trc, unsafe { (*location).get() });
}

/// # Safety
/// `trc` is a roves-js tracer and `location` a live heap location.
pub unsafe fn CallIdTracer(trc: *mut JSTracer, location: *mut Heap<jsid>, _name: *const c_char) {
    // SAFETY: forwarded to the caller.
    let id = unsafe { (*location).get() };
    if let Some(pointer) = id.gcthing() {
        trace_pointer(trc, pointer);
    }
}

/// # Safety
/// `trc` is a roves-js tracer and `location` a live value.
pub unsafe fn CallValueRootTracer(trc: *mut JSTracer, location: *mut Value, _name: *const c_char) {
    // SAFETY: forwarded to the caller.
    trace_value(trc, unsafe { *location });
}

/// # Safety
/// `trc` is a roves-js tracer and `location` a live object pointer.
pub unsafe fn CallObjectRootTracer(trc: *mut JSTracer, location: *mut *mut JSObject, _name: *const c_char) {
    // SAFETY: forwarded to the caller.
    trace_pointer(trc, unsafe { *location } as *mut c_void);
}

/// # Safety
/// `trc` is a roves-js tracer and `descriptor` a live descriptor.
pub unsafe fn CallPropertyDescriptorTracer(trc: *mut JSTracer, descriptor: *mut PropertyDescriptor) {
    // SAFETY: forwarded to the caller.
    let descriptor = unsafe { &*descriptor };
    trace_value(trc, descriptor.value_);
    trace_pointer(trc, descriptor.getter_ as *mut c_void);
    trace_pointer(trc, descriptor.setter_ as *mut c_void);
}
