/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `jsapi` names Servo uses: opaque GC-thing types, the raw context and the raw API.
//!
//! GC-thing pointers (`*mut JSObject`, ...) point at cells (see `cell`); the types themselves
//! are opaque and never dereferenced from Rust, as with SpiderMonkey.

use std::cell::RefCell;

pub use crate::gc::Heap;
pub use crate::realm_impl::{
    CurrentGlobal, CurrentGlobalOrNull, EnterRealm, GetCurrentRealmOrNull, GetObjectRealmOrNull,
    GetRealmGlobalOrNull, GetRealmPrincipals, JS_FireOnNewGlobalObject, JS_GlobalObjectTraceHook,
    JS_MayResolveStandardClass, JS_NewEnumerateStandardClasses, JS_NewGlobalObject,
    JS_ResolveStandardClass, LeaveRealm,
};
pub use crate::binding::{CallJitGetterOp, CallJitMethodOp, CallJitSetterOp, JS_DefineFunctions, JS_DefineProperties};
pub use crate::jsapi_impl::*;
pub use crate::typedarray_impl::*;
pub use crate::runtime_impl::*;
pub use crate::script_impl::*;
pub use crate::values_impl::*;
pub use crate::modules_impl::*;
pub use crate::structured_clone_impl::{
    JS_ReadBytes, JS_ReadStructuredClone, JS_ReadUint32Pair, JS_WriteBytes, JS_WriteStructuredClone,
    JS_WriteUint32Pair, JSAutoStructuredCloneBuffer, JSStructuredCloneData,
};
pub use crate::proxy::{ProxyClassExtension, ProxyClassOps, ProxyObjectOps};
pub use crate::proxy::{
    IsWindowProxy, IsWindowSlow, JS_TransplantObject, SetDOMProxyInformation, SetPropertyIgnoringNamedGetter,
    SetWindowProxy, SetWindowProxyClass, ToWindowIfWindowProxy, ToWindowProxyIfWindowSlow,
};
pub use crate::object::RuntimeHeapState;

/// The bindgen types Servo uses (layouts copied from mozjs_sys by
/// `support/roves_js/extract_jsapi_types.py`); roves-js gives them their behaviour.
mod types {
    #![allow(dead_code, non_camel_case_types, non_snake_case, non_upper_case_globals, clippy::all)]
    use super::{BigInt, JSContext, JSFunction, JSObject, JSScript, JSString, Symbol};
    use crate::jsid::{PropertyKey, jsid};
    #[allow(unused_imports)]
    use crate::gc::Rooted;
    #[allow(unused_imports)]
    use crate::structured_clone_impl::{JSAutoStructuredCloneBuffer, JSStructuredCloneData};
    use crate::jsval::Value;

    /// SpiderMonkey's stack-rooted GC vector (opaque here; its uses are emulated).
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct StackGCVector<T, AllocPolicy = TempAllocPolicy> {
        _marker: std::marker::PhantomData<(T, AllocPolicy)>,
    }

    include!("jsapi_types.rs");

    // As in mozjs_sys's bindgen output: the static binding tables are shared read-only.
    unsafe impl Sync for JSClass {}
    unsafe impl Sync for JSFunctionSpec {}
    unsafe impl Sync for JSNativeWrapper {}
    unsafe impl Sync for JSPropertySpec {}
    unsafe impl Sync for JSTypedMethodJitInfo {}
    // Raw handles are plain pointers; the static handles below are shared read-only.
    unsafe impl<T> Sync for Handle<T> {}
}
pub use types::*;

/// SpiderMonkey's `JS::` namespace (bindgen's `jsapi::JS` module): the same flat items.
#[allow(non_snake_case)]
pub mod JS {
    pub use super::*;
}

pub type MutableHandleValue = MutableHandle<Value>;
pub type MutableHandleObject = MutableHandle<*mut JSObject>;
pub type MutableHandleString = MutableHandle<*mut JSString>;
pub type MutableHandleId = MutableHandle<jsid>;

static NULL_VALUE: Value = crate::jsval::NULL_BITS;
static UNDEFINED_VALUE: Value = crate::jsval::UNDEFINED_BITS;
static TRUE_VALUE: Value = crate::jsval::TRUE_BITS;
static FALSE_VALUE: Value = crate::jsval::FALSE_BITS;

macro_rules! static_handle {
    ($name:ident, $value:ident) => {
        pub static $name: Handle<Value> = Handle { _phantom_0: std::marker::PhantomData, ptr: &$value };
    };
}

static_handle!(NullHandleValue, NULL_VALUE);
static_handle!(UndefinedHandleValue, UNDEFINED_VALUE);
static_handle!(TrueHandleValue, TRUE_VALUE);
static_handle!(FalseHandleValue, FALSE_VALUE);
pub use crate::jsid::{PropertyKey, jsid};
pub use crate::jsval::{JSVal, Value};
pub use crate::api::{
    ExceptionStackBehavior, JS_ClearPendingException, JS_DeprecatedStringHasLatin1Chars,
    JS_GetPendingException, JS_GetStringLength, JS_IsExceptionPending, JS_SetPendingException,
    UTF8Chars,
};

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            /// Opaque GC thing; pointers to it point at a roves-js cell.
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(JSObject, JSString, JSFunction, Symbol, BigInt, JSScript);

/// The raw context behind [`crate::context::JSContext`]: one V8 isolate entered in one realm,
/// plus this thread's root stack (see `gc`).
pub struct JSContext {
    pub(crate) isolate: *mut v8::Isolate,
    /// The current realm's V8 context (kept in sync with `current_realm`).
    pub(crate) context: RefCell<v8::Global<v8::Context>>,
    /// The current realm (see `realm_impl`).
    pub(crate) current_realm: std::cell::Cell<*mut Realm>,
    /// Every realm of this runtime.
    pub(crate) realms: RefCell<Vec<Box<crate::realm_impl::RealmData>>>,
    /// SpiderMonkey's pending exception: set when an API call catches a JS exception (or a
    /// native throws one), read and cleared through `JS_GetPendingException` and friends, and
    /// rethrown into V8 when control returns to script.
    pub(crate) pending_exception: RefCell<Option<v8::Global<v8::Value>>>,
    /// Object templates of the `JSClass`es used in this runtime.
    pub(crate) class_templates: crate::object::ClassTemplates,
    /// Functions made from natives (`JS_NewFunction`); see `jsapi_impl::NativeFunction`.
    pub(crate) native_functions: RefCell<Vec<Box<crate::jsapi_impl::NativeFunction>>>,
    /// The size of `native_functions` at which the states of dead functions are dropped.
    pub(crate) native_function_sweep: std::cell::Cell<usize>,
    /// The cells of plain objects and symbols (see `cell`).
    pub(crate) interned: crate::cell::Interned,
    /// The V8 handler object of each proxy handler (see `proxy`).
    pub(crate) proxy_handlers: RefCell<std::collections::HashMap<usize, v8::Global<v8::Object>>>,
    /// String atoms (see `jsapi_impl::atomize`).
    pub(crate) atoms: crate::jsapi_impl::Atoms,
    /// The embedder's runtime hooks and settings (see `runtime_impl`).
    pub(crate) hooks: crate::runtime_impl::RuntimeHooks,
    /// Script privates and error reports (see `script_impl`).
    pub(crate) scripts: crate::script_impl::Scripts,
    /// Helper functions written in JS, compiled once (see `values_impl::helper`).
    pub(crate) helpers: RefCell<std::collections::HashMap<&'static str, v8::Global<v8::Function>>>,
    /// Module records and module hooks (see `modules_impl`).
    pub(crate) modules: crate::modules_impl::Modules,
    /// The boxes of callable class objects (see `object::new_callable_class_object`).
    pub(crate) callable_class_boxes: RefCell<Vec<v8::cppgc::Persistent<crate::object::ClassBox>>>,
}

impl JSContext {
    /// Runs `f` inside a handle scope entered in the current realm.
    pub(crate) fn with_scope<R>(&self, f: impl FnOnce(&mut v8::PinScope) -> R) -> R {
        // SAFETY: the runtime that owns the isolate outlives every context pointer it hands
        // out, and the isolate is only used from its own thread.
        let isolate = unsafe { &mut *self.isolate };
        v8::scope!(let scope, isolate);
        let context = v8::Local::new(scope, &*self.context.borrow());
        let scope = &mut v8::ContextScope::new(scope, context);
        f(scope)
    }

    /// Runs `f` like [`JSContext::with_scope`] under a `TryCatch`: a JS exception thrown
    /// inside becomes the pending exception, and the result is `None`.
    pub(crate) fn catching<R>(&self, f: impl FnOnce(&mut v8::PinScope) -> Option<R>) -> Option<R> {
        self.with_scope(|scope| {
            v8::tc_scope!(let try_catch, scope);
            let result = f(try_catch);
            if try_catch.has_caught() {
                let exception = try_catch.exception().unwrap_or_else(|| v8::undefined(try_catch).into());
                *self.pending_exception.borrow_mut() = Some(v8::Global::new(try_catch, exception));
                try_catch.reset();
                return None;
            }
            result
        })
    }

    /// Makes `realm` the current realm.
    pub(crate) fn set_current_realm(&self, realm: *mut Realm) {
        if let Some(data) = crate::realm_impl::realm_data(realm) {
            *self.context.borrow_mut() = data.context.clone();
            self.current_realm.set(realm);
        }
    }

    /// Makes `exception` the pending exception.
    pub(crate) fn set_pending(&self, scope: &mut v8::PinScope, exception: v8::Local<v8::Value>) {
        *self.pending_exception.borrow_mut() = Some(v8::Global::new(scope, exception));
    }

    /// The context of this thread's runtime, for APIs that take no context argument.
    pub(crate) fn current<'a>() -> &'a JSContext {
        let raw = crate::rust::Runtime::get().expect("a roves-js runtime on this thread");
        // SAFETY: the runtime outlives the API calls made on its thread.
        unsafe { &*raw.as_ptr() }
    }
}
