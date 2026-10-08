/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `js::rust`: the runtime and the Rust-side handle types.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use v8::cppgc::{GarbageCollected, Persistent, Visitor};

pub use crate::gc::*;
pub use crate::gc::Traceable as Trace;
use crate::jsapi::JSContext as RawJSContext;
pub use crate::realm_impl::{get_context_realm, get_object_realm};
pub use crate::api::{
    ForOfIterationFailure, ToBoolean, ToInt32, ToInt64, ToNumber, ToString, ToUint16, ToUint32,
    ToUint64, for_of, maybe_wrap_object_or_null_value, maybe_wrap_object_value, maybe_wrap_value,
};

/// The `&mut JSContext` forms of the JSAPI (mozjs generates these from jsapi).
pub mod wrappers2 {
    // mozjs's `wrap!` macro: turns a raw JSAPI signature into a wrapper taking the safe
    // context and Rust handles. The list it runs over is generated (`wrappers2.in.rs`).
    macro_rules! wrap {
        // The invocation of @inner has the following form:
        // @inner (input args) <> (arg signture accumulator) <> (arg expr accumulator) <> unparsed tokens
        // when `unparsed tokens == \eps`, accumulator contains the final result
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: Handle<$gentype:ty>, $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: Handle<$gentype>) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandle<$gentype:ty>, $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandle<$gentype>) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: Handle, $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: Handle) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandle, $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandle) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleFunction , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleFunction) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleId , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleId) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleObject , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleObject) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleScript , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleScript) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleString , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleString) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleSymbol , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleSymbol) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: HandleValue , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: HandleValue) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleFunction , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleFunction) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleId , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleId) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleObject , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleObject) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleScript , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleScript) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleString , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleString) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleSymbol , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleSymbol) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: MutableHandleValue , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: MutableHandleValue) <> ($($arg_expr_acc,)* $arg.into(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: &mut JSContext , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: &mut JSContext) <> ($($arg_expr_acc,)* $arg.raw_cx(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: &JSContext , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: &JSContext) <> ($($arg_expr_acc,)* $arg.raw_cx_no_gc(),) <> $($rest)*);
        };
        // functions that take *const AutoRequireNoGC already have &JSContext, so we can remove this mareker argument
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: *const AutoRequireNoGC , $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)*) <> ($($arg_expr_acc,)* ::std::ptr::null(),) <> $($rest)*);
        };
        (@inner $saved:tt <> ($($arg_sig_acc:tt)*) <> ($($arg_expr_acc:expr,)*) <> $arg:ident: $type:ty, $($rest:tt)*) => {
            wrap!(@inner $saved <> ($($arg_sig_acc)* , $arg: $type) <> ($($arg_expr_acc,)* $arg,) <> $($rest)*);
        };
        (@inner ($module:tt: $func_name:ident -> $outtype:ty) <> (, $($args:tt)*) <> ($($argexprs:expr,)*) <> ) => {
            #[inline]
            pub unsafe fn $func_name($($args)*) -> $outtype {
                $module::$func_name($($argexprs),*)
            }
        };
        ($module:tt: pub fn $func_name:ident($($args:tt)*) -> $outtype:ty) => {
            wrap!(@inner ($module: $func_name -> $outtype) <> () <> () <> $($args)* ,);
        };
        ($module:tt: pub fn $func_name:ident($($args:tt)*)) => {
            wrap!($module: pub fn $func_name($($args)*) -> ());
        }
    }

    use crate::context::JSContext;
    use crate::gc::{
        Handle, HandleFunction, HandleId, HandleObject, HandleScript, HandleString, HandleSymbol,
        HandleValue, MutableHandle, MutableHandleFunction, MutableHandleId, MutableHandleObject,
        MutableHandleScript, MutableHandleString, MutableHandleSymbol, MutableHandleValue,
    };
    #[allow(unused_imports)]
    use crate::glue::*;
    #[allow(unused_imports)]
    use crate::jsapi::*;
    use crate::jsapi::{JSObject, JSString, UTF8Chars};
    use crate::{glue, jsapi};

    include!("wrappers2.in.rs");

    /// As in mozjs (written by hand there too: `ownDesc` is optional).
    ///
    /// # Safety
    /// As for `jsapi::SetPropertyIgnoringNamedGetter`.
    #[inline]
    pub unsafe fn SetPropertyIgnoringNamedGetter(
        cx: &mut JSContext,
        obj: HandleObject,
        id: HandleId,
        v: HandleValue,
        receiver: HandleValue,
        own_desc: Option<Handle<PropertyDescriptor>>,
        result: *mut ObjectOpResult,
    ) -> bool {
        let own_desc: Option<jsapi::Handle<PropertyDescriptor>> = own_desc.map(Into::into);
        let own_desc_ptr = own_desc.as_ref().map_or(std::ptr::null(), |desc| desc as *const _);
        // SAFETY: forwarded.
        unsafe { jsapi::SetPropertyIgnoringNamedGetter(cx.raw_cx(), obj.into(), id.into(), v.into(), receiver.into(), own_desc_ptr, result) }
    }

    pub fn JS_GetLatin1StringCharsAndLength(_cx: &JSContext, s: *mut JSString, length: &mut usize) -> *const u8 {
        crate::api::latin1_chars(s, length)
    }

    pub fn JS_GetTwoByteStringCharsAndLength(_cx: &JSContext, s: *mut JSString, length: &mut usize) -> *const u16 {
        crate::api::two_byte_chars(s, length)
    }

    /// # Safety
    /// `chars` must point at valid UTF-8 for the call.
    pub unsafe fn JS_NewStringCopyUTF8N(cx: &mut JSContext, chars: *const UTF8Chars) -> *mut JSString {
        // SAFETY: forwarded to the caller.
        crate::api::new_string_utf8(cx.raw_ref(), unsafe { &*chars })
    }

    pub fn JS_NewUCStringCopyN(cx: &mut JSContext, chars: *const u16, length: usize) -> *mut JSString {
        // SAFETY: JSAPI callers pass `length` valid code units.
        let units = unsafe { std::slice::from_raw_parts(chars, length) };
        crate::api::new_string_utf16(cx.raw_ref(), units)
    }

    /// # Safety
    /// As in mozjs.
    pub unsafe fn NewArrayObject1(cx: &mut JSContext, length: usize) -> *mut JSObject {
        crate::api::new_array(cx.raw_ref(), length)
    }

    /// # Safety
    /// As in mozjs.
    pub unsafe fn JS_DefineElement(cx: &mut JSContext, obj: HandleObject, index: u32, value: HandleValue, attrs: u32) -> bool {
        crate::api::define_element(cx.raw_ref(), obj, index, value, attrs)
    }

    /// # Safety
    /// As in mozjs.
    pub unsafe fn AssertSameCompartment(cx: &JSContext, obj: *mut JSObject) {
        crate::api::assert_same_compartment(cx, obj);
    }

    pub fn JS_IsExceptionPending(cx: &JSContext) -> bool {
        // SAFETY: a live context.
        unsafe { crate::api::JS_IsExceptionPending(cx.ptr.as_ptr()) }
    }

    pub fn JS_ClearPendingException(cx: &JSContext) {
        // SAFETY: a live context.
        unsafe { crate::api::JS_ClearPendingException(cx.ptr.as_ptr()) }
    }
}

/// Traced on every GC through a persistent root: reports this thread's root stack.
struct RootSet;

// SAFETY: `trace` reports every rooted location (see `gc::trace_roots`).
unsafe impl GarbageCollected for RootSet {
    fn trace(&self, visitor: &mut Visitor) {
        crate::gc::trace_roots(visitor);
        if let Some(cx) = Runtime::get() {
            // SAFETY: the runtime outlives its collections.
            crate::jsapi_impl::trace_native_functions(unsafe { cx.as_ref() }, visitor);
            // SAFETY: as above.
            crate::realm_impl::trace_realms(unsafe { cx.as_ref() }, visitor);
            // SAFETY: as above.
            crate::jsapi_impl::trace_atoms(unsafe { cx.as_ref() }, visitor);
            // SAFETY: as above. The embedder's roots (Servo's DOM root lists).
            crate::runtime_impl::trace_extra_roots(unsafe { cx.as_ref() }, visitor);
            // SAFETY: as above.
            crate::script_impl::trace_scripts(unsafe { cx.as_ref() }, visitor);
        }
        // RootedVec / RootedTraceableBox contents.
        // SAFETY: the tracer is this GC's visitor.
        unsafe { crate::gc::trace_traceables(crate::glue::tracer(visitor), std::ptr::null_mut()) };
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesJsRootSet"
    }
}

thread_local! {
    static CURRENT: Cell<Option<NonNull<RawJSContext>>> = const { Cell::new(None) };
}

#[derive(PartialEq)]
enum EngineState {
    Uninitialized,
    Initialized,
    ShutDown,
}

static ENGINE_STATE: std::sync::Mutex<EngineState> = std::sync::Mutex::new(EngineState::Uninitialized);

#[derive(Debug)]
pub enum JSEngineError {
    AlreadyInitialized,
    AlreadyShutDown,
    InitFailed,
}

/// The process-wide engine (mozjs's `JSEngine`): initializes V8 once. Runtimes hold
/// [`JSEngineHandle`]s, which must all be gone before the engine is dropped.
pub struct JSEngine {
    outstanding_handles: std::sync::Arc<std::sync::atomic::AtomicU32>,
    marker: std::marker::PhantomData<*mut ()>,
}

pub struct JSEngineHandle(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl Clone for JSEngineHandle {
    fn clone(&self) -> JSEngineHandle {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        JSEngineHandle(self.0.clone())
    }
}

impl Drop for JSEngineHandle {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn initialize_v8() {
    #[cfg(not(test))]
    roves_v8::initialize_engine();
    // Tests force collections, which V8 only allows with --expose-gc.
    #[cfg(test)]
    roves_v8::initialize_engine_with_flags("--expose-gc");
}

impl JSEngine {
    pub fn init() -> Result<JSEngine, JSEngineError> {
        let mut state = ENGINE_STATE.lock().unwrap();
        match *state {
            EngineState::Initialized => return Err(JSEngineError::AlreadyInitialized),
            EngineState::ShutDown => return Err(JSEngineError::AlreadyShutDown),
            EngineState::Uninitialized => (),
        }
        initialize_v8();
        *state = EngineState::Initialized;
        Ok(JSEngine { outstanding_handles: Default::default(), marker: std::marker::PhantomData })
    }

    pub fn can_shutdown(&self) -> bool {
        self.outstanding_handles.load(std::sync::atomic::Ordering::SeqCst) == 0
    }

    pub fn handle(&self) -> JSEngineHandle {
        self.outstanding_handles.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        JSEngineHandle(self.outstanding_handles.clone())
    }
}

impl Drop for JSEngine {
    fn drop(&mut self) {
        let mut state = ENGINE_STATE.lock().unwrap();
        if *state == EngineState::Initialized {
            assert!(self.can_shutdown(), "There are outstanding JS engine handles");
            // V8's platform cannot be initialized twice in a process, so it stays up.
            *state = EngineState::ShutDown;
        }
    }
}

#[cfg(test)]
impl JSEngineHandle {
    /// A handle for tests, which create runtimes on many threads without one `JSEngine`.
    pub(crate) fn for_tests() -> JSEngineHandle {
        initialize_v8();
        JSEngineHandle(Default::default())
    }
}

/// What a child runtime needs from its parent (mozjs's `ParentRuntime`). On V8 every runtime
/// is an independent isolate, so a child only shares the engine handle.
pub struct ParentRuntime {
    engine: JSEngineHandle,
    children_of_parent: std::sync::Arc<()>,
}

// SAFETY: it only carries reference counts.
unsafe impl Send for ParentRuntime {}

/// The thread-safe side of a runtime (mozjs's `ThreadSafeJSContext`): interrupts from other
/// threads, through V8's isolate handle.
#[derive(Clone)]
pub struct ThreadSafeJSContext(std::sync::Arc<std::sync::RwLock<Option<v8::IsolateHandle>>>);

// SAFETY: `IsolateHandle` is V8's thread-safe isolate reference.
unsafe impl Send for ThreadSafeJSContext {}
unsafe impl Sync for ThreadSafeJSContext {}

impl ThreadSafeJSContext {
    /// Requests the runtime's interrupt callbacks (`JS_AddInterruptCallback`) to run soon on
    /// the runtime's thread.
    pub fn request_interrupt_callback(&self) {
        if let Some(handle) = self.0.read().unwrap().as_ref() {
            handle.request_interrupt(crate::runtime_impl::run_interrupt_callbacks, std::ptr::null_mut());
        }
    }

    pub fn request_interrupt_callback_can_wait(&self) {
        self.request_interrupt_callback();
    }
}

/// One V8 isolate with its initial realm: the `JSRuntime`/`JSContext` pair of mozjs.
pub struct Runtime {
    // Field order matters: the root and context must drop before the isolate.
    cx: crate::context::JSContext,
    root_set: Persistent<RootSet>,
    raw: Box<RawJSContext>,
    /// Boxed: the raw context points at the `Isolate` value, which lives inside the
    /// `OwnedIsolate` and must not move.
    isolate: Box<v8::OwnedIsolate>,
    engine: JSEngineHandle,
    _parent_child_count: Option<std::sync::Arc<()>>,
    outstanding_children: std::sync::Arc<()>,
    thread_safe_handle: std::sync::Arc<std::sync::RwLock<Option<v8::IsolateHandle>>>,
}

impl Runtime {
    pub fn new(engine: JSEngineHandle) -> Runtime {
        Self::create(engine, None)
    }

    pub fn prepare_for_new_child(&self) -> ParentRuntime {
        ParentRuntime { engine: self.engine.clone(), children_of_parent: self.outstanding_children.clone() }
    }

    /// # Safety
    /// As in mozjs (the parent runtime must outlive this one).
    pub unsafe fn create_with_parent(parent: ParentRuntime) -> Runtime {
        Self::create(parent.engine.clone(), Some(parent))
    }

    fn create(engine: JSEngineHandle, parent: Option<ParentRuntime>) -> Runtime {
        let mut isolate = Box::new(v8::Isolate::new(v8::CreateParams::default()));
        crate::runtime_impl::configure_isolate(&mut isolate);
        let (context, root_set) = {
            v8::scope!(let scope, &mut **isolate);
            let context = v8::Context::new(scope, Default::default());
            let heap = scope.get_cpp_heap().expect("V8 isolates carry a cppgc heap");
            // SAFETY: moved straight into a Persistent.
            let root_set = unsafe { v8::cppgc::make_garbage_collected(heap, RootSet) };
            (v8::Global::new(scope, context), Persistent::new(&root_set))
        };
        let mut raw = Box::new(RawJSContext {
            isolate: std::ptr::null_mut(),
            context: RefCell::new(context),
            pending_exception: RefCell::new(None),
            class_templates: Default::default(),
            native_functions: RefCell::new(Vec::new()),
            interned: Default::default(),
            proxy_handlers: RefCell::new(Default::default()),
            atoms: Default::default(),
            hooks: Default::default(),
            scripts: Default::default(),
            current_realm: std::cell::Cell::new(std::ptr::null_mut()),
            realms: RefCell::new(Vec::new()),
        });
        raw.isolate = &mut **isolate as *mut v8::Isolate;
        CURRENT.with(|current| {
            assert!(current.get().is_none(), "one roves-js runtime per thread");
            current.set(Some(NonNull::from(&*raw)));
        });
        // The initial context is the runtime's first realm.
        let realm = raw.with_scope(|scope| {
            let context = scope.get_current_context();
            crate::realm_impl::register_realm(&raw, scope, context, std::ptr::null_mut())
        });
        raw.current_realm.set(realm);
        let thread_safe_handle = std::sync::Arc::new(std::sync::RwLock::new(Some(isolate.thread_safe_handle())));
        // SAFETY: the raw context lives (boxed) as long as the runtime.
        let cx = unsafe { crate::context::JSContext::from_ptr(NonNull::from(&*raw)) };
        Runtime {
            cx,
            root_set,
            raw,
            isolate,
            engine,
            _parent_child_count: parent.map(|parent| parent.children_of_parent),
            outstanding_children: std::sync::Arc::new(()),
            thread_safe_handle,
        }
    }

    pub fn thread_safe_js_context(&self) -> ThreadSafeJSContext {
        ThreadSafeJSContext(self.thread_safe_handle.clone())
    }

    /// The `JSRuntime` pointer (the raw context: one runtime per context on V8).
    pub fn rt(&self) -> *mut crate::jsapi::JSRuntime {
        NonNull::from(&*self.raw).as_ptr() as *mut crate::jsapi::JSRuntime
    }

    pub fn cx<'rt>(&'rt mut self) -> &'rt mut crate::context::JSContext {
        &mut self.cx
    }

    pub fn cx_no_gc<'rt>(&'rt self) -> &'rt crate::context::JSContext {
        &self.cx
    }

    /// The raw context pointer.
    pub fn raw_cx(&self) -> *mut RawJSContext {
        NonNull::from(&*self.raw).as_ptr()
    }

    /// The raw context of this thread's runtime.
    pub fn get() -> Option<NonNull<RawJSContext>> {
        CURRENT.with(Cell::get)
    }

    /// Forces a full, precise garbage collection (tests only: no stack scanning).
    #[doc(hidden)]
    pub fn gc_for_testing(&mut self) {
        let isolate: &mut v8::Isolate = &mut self.isolate;
        if let Some(heap) = isolate.get_cpp_heap() {
            // SAFETY: called outside any native frame that holds unrooted cell pointers it
            // still needs.
            unsafe { heap.collect_garbage_for_testing(v8::cppgc::EmbedderStackState::NoHeapPointers) };
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.thread_safe_handle.write().unwrap().take();
        assert!(std::sync::Arc::get_mut(&mut self.outstanding_children).is_some(), "This runtime still has live children.");
        CURRENT.with(|current| {
            if current.get() == Some(NonNull::from(&*self.raw)) {
                current.set(None);
            }
        });
        let _ = &self.root_set;
    }
}

/// A rooted vector of ids (`JS::RootedIdVector`), filled through its handle
/// (`GetPropertyKeys`, `AppendToIdVector`).
pub struct IdVector(Box<Vec<crate::jsid::jsid>>);

unsafe fn trace_id_vector(location: *const std::ffi::c_void, visitor: &mut Visitor) {
    use crate::gc::RootKind;
    // SAFETY: the vector is unregistered before it is dropped.
    unsafe { &*(location as *const Vec<crate::jsid::jsid>) }.trace_root(visitor);
}

impl IdVector {
    /// # Safety
    /// `cx` must be the live context of this thread (as for every mozjs constructor).
    pub unsafe fn new(_cx: *mut RawJSContext) -> IdVector {
        let vector = Box::new(Vec::new());
        crate::gc::register_custom_root(&*vector as *const Vec<_> as *const std::ffi::c_void, trace_id_vector);
        IdVector(vector)
    }

    pub fn handle_mut(&mut self) -> crate::jsapi::MutableHandleIdVector {
        crate::jsapi::MutableHandleIdVector { ptr: &mut *self.0 as *mut Vec<_> as *mut std::ffi::c_void }
    }
}

impl Drop for IdVector {
    fn drop(&mut self) {
        crate::gc::unregister_custom_root(&*self.0 as *const Vec<_> as *const std::ffi::c_void);
    }
}

impl std::ops::Deref for IdVector {
    type Target = [crate::jsid::jsid];

    fn deref(&self) -> &[crate::jsid::jsid] {
        &self.0
    }
}

/// Appends to the vector behind an `IdVector` handle.
///
/// # Safety
/// `handle` must come from [`IdVector::handle_mut`] of a live vector.
pub(crate) unsafe fn append_to_id_vector(handle: crate::jsapi::MutableHandleIdVector, id: crate::jsid::jsid) {
    // SAFETY: see above.
    unsafe { &mut *(handle.ptr as *mut Vec<crate::jsid::jsid>) }.push(id);
}

/// Defines `methods` (a `JS_FS_END`-terminated table) on `obj`.
///
/// # Safety
/// `cx` must be the live context of this thread.
pub unsafe fn define_methods(cx: *mut RawJSContext, obj: HandleObject, methods: &'static [crate::jsapi::JSFunctionSpec]) -> Result<(), ()> {
    // SAFETY: both name variants are pointer-sized; the terminator's is null.
    assert!(methods.last().is_some_and(|spec| unsafe { spec.name.string_ }.is_null()));
    // SAFETY: forwarded.
    if unsafe { crate::jsapi::JS_DefineFunctions(cx, obj.into(), methods.as_ptr()) } { Ok(()) } else { Err(()) }
}

/// Defines `properties` (a `JS_PS_END`-terminated table) on `obj`.
///
/// # Safety
/// `cx` must be the live context of this thread.
pub unsafe fn define_properties(cx: *mut RawJSContext, obj: HandleObject, properties: &'static [crate::jsapi::JSPropertySpec]) -> Result<(), ()> {
    // SAFETY: as above.
    assert!(properties.last().is_some_and(|spec| unsafe { spec.name.string_ }.is_null()));
    // SAFETY: forwarded.
    if unsafe { crate::jsapi::JS_DefineProperties(cx, obj.into(), properties.as_ptr()) } { Ok(()) } else { Err(()) }
}

/// The `JSClass` of an object (a shared plain class for ordinary objects).
///
/// # Safety
/// `obj` must be a live object.
pub unsafe fn get_object_class(obj: *mut crate::jsapi::JSObject) -> *const crate::jsapi::JSClass {
    crate::object::object_class(obj)
}

pub fn is_dom_class(class: &crate::jsapi::JSClass) -> bool {
    class.flags & crate::object::JSCLASS_IS_DOMJSCLASS != 0
}

/// # Safety
/// `obj` must be a live object.
pub unsafe fn is_dom_object(obj: *mut crate::jsapi::JSObject) -> bool {
    // SAFETY: classes are static binding tables.
    is_dom_class(unsafe { &*get_object_class(obj) })
}

/// Wraps `obj` for the current realm. V8 shares objects across realms, so this is the
/// identity (WindowProxy outerization is not emulated yet).
///
/// # Safety
/// `cx` must be the live context of this thread.
pub unsafe fn maybe_wrap_object(_cx: *mut RawJSContext, _obj: MutableHandleObject) {}

/// The global class mozjs offers for simple embeddings and tests.
pub static SIMPLE_GLOBAL_CLASS: crate::jsapi::JSClass = crate::jsapi::JSClass {
    name: c"Global".as_ptr(),
    flags: crate::object::JSCLASS_IS_GLOBAL |
        ((crate::object::JSCLASS_GLOBAL_SLOT_COUNT & crate::object::JSCLASS_RESERVED_SLOTS_MASK) <<
            crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
    cOps: std::ptr::null(),
    spec: std::ptr::null(),
    ext: std::ptr::null(),
    oOps: std::ptr::null(),
};

/// Realm creation options (mozjs's owning wrapper of `JS::RealmOptions`).
pub struct RealmOptions(Box<crate::jsapi::RealmOptions>);

impl Default for RealmOptions {
    fn default() -> RealmOptions {
        // SAFETY: bindgen's plain option structs are valid when zeroed (null hooks and
        // pointers, false flags, the first enum variants), as `JS_NewRealmOptions` makes them.
        RealmOptions(Box::new(unsafe { std::mem::zeroed() }))
    }
}

impl std::ops::Deref for RealmOptions {
    type Target = crate::jsapi::RealmOptions;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for RealmOptions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// --- Scripts, errors and stacks (mozjs's `rust` helpers) ---------------------------------------

pub fn transform_str_to_source_text(source: &str) -> crate::jsapi::SourceText<crate::jsapi::Utf8Unit> {
    crate::jsapi::SourceText {
        units_: source.as_ptr() as *const _,
        length_: source.len() as u32,
        ownsUnits_: false,
        _phantom_0: std::marker::PhantomData,
    }
}

pub fn transform_u16_to_source_text(source: &[u16]) -> crate::jsapi::SourceText<u16> {
    crate::jsapi::SourceText {
        units_: source.as_ptr() as *const _,
        length_: source.len() as u32,
        ownsUnits_: false,
        _phantom_0: std::marker::PhantomData,
    }
}

/// Owned compile options (mozjs's `CompileOptionsWrapper`).
pub struct CompileOptionsWrapper {
    pub ptr: *mut crate::jsapi::ReadOnlyCompileOptions,
    filename: std::ffi::CString,
}

impl CompileOptionsWrapper {
    pub fn new(cx: &crate::context::JSContext, filename: std::ffi::CString, line: u32) -> Self {
        // SAFETY: the options keep a pointer to `filename`, which this wrapper owns.
        let ptr = unsafe { crate::script_impl::NewCompileOptions(cx.raw_ref() as *const _ as *mut _, filename.as_ptr(), line) };
        assert!(!ptr.is_null());
        Self { ptr, filename }
    }

    /// # Safety
    /// `cx` must point to a live context.
    #[deprecated(note = "Use CompileOptionsWrapper::new instead")]
    pub unsafe fn new_raw(cx: *mut RawJSContext, filename: std::ffi::CString, line: u32) -> Self {
        // SAFETY: as above.
        let ptr = unsafe { crate::script_impl::NewCompileOptions(cx, filename.as_ptr(), line) };
        Self { ptr, filename }
    }

    pub fn filename(&self) -> &str {
        self.filename.to_str().expect("Guaranteed by new")
    }

    pub fn set_introduction_type(&mut self, introduction_type: &'static std::ffi::CStr) {
        // SAFETY: the options are owned by this wrapper.
        unsafe { (*self.ptr)._base.introductionType = introduction_type.as_ptr() };
    }

    pub fn set_muted_errors(&mut self, muted_errors: bool) {
        // SAFETY: as above.
        unsafe { (*self.ptr)._base.mutedErrors_ = muted_errors };
    }

    pub fn set_is_run_once(&mut self, is_run_once: bool) {
        // SAFETY: as above.
        unsafe { (*self.ptr).isRunOnce = is_run_once };
    }

    pub fn set_no_script_rval(&mut self, no_script_rval: bool) {
        // SAFETY: as above.
        unsafe { (*self.ptr).noScriptRval = no_script_rval };
    }
}

impl Drop for CompileOptionsWrapper {
    fn drop(&mut self) {
        // SAFETY: created by `NewCompileOptions`.
        unsafe { crate::script_impl::DeleteCompileOptions(self.ptr) }
    }
}

/// Compiles and runs `script` in `glob`'s realm (mozjs's `evaluate_script`).
pub fn evaluate_script(
    cx: &mut crate::context::JSContext,
    glob: HandleObject,
    script: &str,
    rval: MutableHandleValue,
    options: CompileOptionsWrapper,
) -> Result<(), ()> {
    let mut realm = crate::realm::AutoRealm::new_from_handle(cx, glob);
    let mut source = transform_str_to_source_text(script);
    // SAFETY: the realm's context is live; the options and source outlive the call.
    let ok = unsafe { crate::script_impl::Evaluate2(realm.raw_cx(), options.ptr, &mut source, rval.into()) };
    if ok { Ok(()) } else { Err(()) }
}

/// An environment chain for function compilation (mozjs's `EnvironmentChain`).
pub struct EnvironmentChain {
    chain: *mut crate::jsapi::EnvironmentChain,
}

impl EnvironmentChain {
    pub fn new(cx: *mut RawJSContext, support_unscopables: crate::jsapi::SupportUnscopables) -> Self {
        // SAFETY: a new, owned chain.
        Self { chain: unsafe { crate::script_impl::NewEnvironmentChain(cx, support_unscopables) } }
    }

    pub fn append(&self, obj: *mut crate::jsapi::JSObject) {
        // SAFETY: the chain is owned by this wrapper.
        assert!(unsafe { crate::script_impl::AppendToEnvironmentChain(self.chain, obj) });
    }

    pub fn get(&self) -> *mut crate::jsapi::EnvironmentChain {
        self.chain
    }
}

impl Drop for EnvironmentChain {
    fn drop(&mut self) {
        // SAFETY: as above.
        unsafe { crate::script_impl::DeleteEnvironmentChain(self.chain) }
    }
}

#[derive(Debug, Default)]
pub struct ScriptedCaller {
    pub filename: String,
    pub line: u32,
    pub col: u32,
}

pub fn describe_scripted_caller_safe(cx: &crate::context::JSContext) -> Result<ScriptedCaller, ()> {
    let mut buf = [0 as std::ffi::c_char; 1024];
    let mut line = 0;
    let mut col = 0;
    // SAFETY: a live context and a buffer of the given length.
    if !unsafe { crate::script_impl::DescribeScriptedCaller(cx.raw_ref() as *const _ as *mut _, buf.as_mut_ptr(), buf.len(), &mut line, &mut col) } {
        return Err(());
    }
    // SAFETY: NUL-terminated by `DescribeScriptedCaller`.
    let filename = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    Ok(ScriptedCaller { filename: String::from_utf8_lossy(filename.to_bytes()).into_owned(), line, col })
}

pub struct ErrorInfo {
    pub message: String,
    pub filename: String,
    pub line: u32,
    pub col: u32,
}

unsafe extern "C" fn fill_string_callback(ptr: *const std::ffi::c_char, len: usize, target: *mut std::ffi::c_void) {
    // SAFETY: `target` is a `String` and the bytes are UTF-8 (`PendingExceptionStackInfo`).
    let target = unsafe { &mut *(target as *mut String) };
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    target.push_str(&String::from_utf8_lossy(slice));
}

/// Takes the pending exception into `rval` and describes it (mozjs's helper).
pub fn error_info_from_exception_stack_safe(cx: &mut crate::context::JSContext, rval: MutableHandleValue) -> Option<ErrorInfo> {
    let mut message = String::new();
    let mut filename = String::new();
    let mut line = 0;
    let mut col = 0;
    // SAFETY: a live context; the targets are `String`s for `fill_string_callback`.
    let found = unsafe {
        crate::script_impl::PendingExceptionStackInfo(
            cx.raw_cx(),
            Some(fill_string_callback),
            &raw mut message as *mut std::ffi::c_void,
            &raw mut filename as *mut std::ffi::c_void,
            &mut line,
            &mut col,
            rval.into(),
        )
    };
    found.then_some(ErrorInfo { message, filename, line, col })
}

/// A captured JS stack (mozjs's `CapturedJSStack`).
pub struct CapturedJSStack<'a> {
    cx: *mut RawJSContext,
    stack: RootedGuard<'a, *mut crate::jsapi::JSObject>,
}

impl<'a> CapturedJSStack<'a> {
    /// # Safety
    /// `cx` must be this thread's live context.
    pub unsafe fn new(cx: *mut RawJSContext, mut guard: RootedGuard<'a, *mut crate::jsapi::JSObject>, max_frame_count: Option<u32>) -> Option<Self> {
        // SAFETY: zeroed storage is a valid capture before initialization.
        let mut capture: crate::jsapi::StackCapture = unsafe { std::mem::zeroed() };
        // SAFETY: initializes the capture.
        unsafe {
            match max_frame_count {
                None => crate::script_impl::JS_StackCapture_AllFrames(&mut capture),
                Some(count) => crate::script_impl::JS_StackCapture_MaxFrames(count, &mut capture),
            }
        }
        let start_after = std::ptr::null_mut::<crate::jsapi::JSObject>();
        let start_after = crate::jsapi::Handle { _phantom_0: std::marker::PhantomData, ptr: &start_after };
        // SAFETY: a rooted out location.
        if !unsafe { crate::script_impl::CaptureCurrentStack(cx, guard.handle_mut().into(), &mut capture, start_after) } {
            None
        } else {
            Some(CapturedJSStack { cx, stack: guard })
        }
    }

    pub fn as_string(&self, indent: Option<usize>, format: crate::jsapi::StackFormat) -> Option<String> {
        let cx = self.cx;
        crate::rooted!(in(cx) let mut js_string = std::ptr::null_mut::<crate::jsapi::JSString>());
        // SAFETY: rooted arguments.
        if !unsafe {
            crate::script_impl::BuildStackString(cx, std::ptr::null_mut(), self.stack.handle().into(), js_string.handle_mut().into(), indent.unwrap_or(0), format)
        } {
            return None;
        }
        let string = std::ptr::NonNull::new(js_string.get())?;
        // SAFETY: this thread's live context and a live, rooted string.
        let safe_cx = unsafe { crate::context::JSContext::from_ptr(std::ptr::NonNull::new(cx)?) };
        Some(unsafe { crate::conversions::jsstr_to_string(&safe_cx, string) })
    }

    /// Calls `f` for each frame, innermost first.
    pub fn for_each_stack_frame<F>(&self, mut f: F)
    where
        F: FnMut(Handle<*mut crate::jsapi::JSObject>),
    {
        let cx = self.cx;
        crate::rooted!(in(cx) let mut current = self.stack.get());
        crate::rooted!(in(cx) let mut next = std::ptr::null_mut::<crate::jsapi::JSObject>());
        loop {
            f(current.handle());
            // SAFETY: rooted arguments.
            let result = unsafe {
                crate::script_impl::GetSavedFrameParent(
                    cx,
                    std::ptr::null_mut(),
                    current.handle().into(),
                    next.handle_mut().into(),
                    crate::jsapi::SavedFrameSelfHosted::Include,
                )
            };
            if result != crate::jsapi::SavedFrameResult::Ok || next.get().is_null() {
                return;
            }
            current.set(next.get());
        }
    }
}

#[macro_export]
macro_rules! capture_stack {
    (&in($cx:expr) $($t:tt)*) => {
        $crate::capture_stack!(in(unsafe { $cx.raw_cx() }) $($t)*);
    };
    (in($cx:expr) let $name:ident = with max depth($max_frame_count:expr)) => {
        $crate::rooted!(in($cx) let mut __obj = ::std::ptr::null_mut());
        let $name = $crate::rust::CapturedJSStack::new($cx, __obj, Some($max_frame_count));
    };
    (in($cx:expr) let $name:ident ) => {
        $crate::rooted!(in($cx) let mut __obj = ::std::ptr::null_mut());
        let $name = $crate::rust::CapturedJSStack::new($cx, __obj, None);
    }
}
