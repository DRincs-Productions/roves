/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Runtime hooks and settings of the JSAPI (interrupts, extra GC roots, the job queue,
//! promise rejection tracking, GC and JIT settings) on V8.
//!
//! **Promise jobs.** SpiderMonkey hands every promise job to the embedder's job queue
//! (`JobQueueTraps::enqueuePromiseJob`), and the embedder runs it at its microtask
//! checkpoint. V8 keeps promise jobs in its own microtask queue (explicit policy here: V8
//! never runs them by itself). When V8 may have queued jobs (its promise hook reports a promise
//! created or resolved), roves-js enqueues one job in the embedder's queue, a native "drain"
//! function of the current realm, which performs V8's microtask checkpoint when the embedder
//! runs it. Promise jobs thus run at the embedder's checkpoints, in its order, as with
//! SpiderMonkey. Without an embedder job queue, [`RunJobs`] performs a checkpoint.
//!
//! **Foreground tasks.** V8 posts some work to run on the isolate's thread: finishing an
//! asynchronous WebAssembly compilation, `FinalizationRegistry` cleanup, `Atomics.waitAsync`
//! timeouts (see `roves_v8::foreground`). Like SpiderMonkey's off-thread work, each task goes
//! to the embedder's event loop through its dispatch callback (`SetUpEventLoopDispatch`) and
//! runs in `DispatchableRun`. Without a dispatch callback, tasks wait for [`RunJobs`].
//!
//! Settings with no V8 counterpart (SpiderMonkey's GC parameters, JIT options, build ids,
//! memory reporters) are recorded and otherwise ignored.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_void};

use crate::glue::{
    DispatchablePointer, GetSize, InvokeScriptPreparerHook, JobQueueTraps, RustDispatchToEventLoopCallback,
    WantToMeasure,
};
use crate::jsapi::{
    BuildIdCharVector, BuildIdOp, ConsumeStreamCallback, ContextOptions, DOMCallbacks, Dispatchable_MaybeShuttingDown,
    GCReason, GCSliceCallback, HasReleasedWrapperCallback, JSContext, JSGCCallback, JSGCParamKey, JSInterruptCallback,
    JSJitCompilerOption, JSReadPrincipalsOp, JSSecurityCallbacks, JSTraceDataOp, JobQueue, PreserveWrapperCallback,
    PromiseRejectionHandlingState, PromiseRejectionTrackerCallback, ReportStreamErrorCallback,
    ScriptEnvironmentPreparer_Closure, ServoSizes,
};
use crate::jsval::from_v8;

/// The hooks and settings an embedder installs on a runtime.
pub(crate) struct RuntimeHooks {
    interrupt_callbacks: RefCell<Vec<JSInterruptCallback>>,
    extra_roots: RefCell<Vec<(JSTraceDataOp, *mut c_void)>>,
    gc_callback: Cell<(JSGCCallback, *mut c_void)>,
    gc_slice_callback: Cell<GCSliceCallback>,
    gc_parameters: RefCell<HashMap<i32, u32>>,
    context_options: Box<ContextOptions>,
    security_callbacks: Cell<*const JSSecurityCallbacks>,
    dom_callbacks: Cell<*const DOMCallbacks>,
    preserve_wrapper: Cell<(PreserveWrapperCallback, HasReleasedWrapperCallback)>,
    read_principals: Cell<JSReadPrincipalsOp>,
    job_queue: Cell<*mut JobQueue>,
    promise_rejection_tracker: Cell<(PromiseRejectionTrackerCallback, *mut c_void)>,
    foreground: std::sync::Arc<Foreground>,
    consume_stream: Cell<(ConsumeStreamCallback, ReportStreamErrorCallback)>,
    script_environment_preparer: Cell<InvokeScriptPreparerHook>,
}

impl Default for RuntimeHooks {
    fn default() -> Self {
        RuntimeHooks {
            interrupt_callbacks: Default::default(),
            extra_roots: Default::default(),
            gc_callback: Cell::new((None, std::ptr::null_mut())),
            gc_slice_callback: Cell::new(None),
            gc_parameters: Default::default(),
            // SAFETY: SpiderMonkey's context options are plain flags (all off when zeroed).
            context_options: Box::new(unsafe { std::mem::zeroed() }),
            security_callbacks: Cell::new(std::ptr::null()),
            dom_callbacks: Cell::new(std::ptr::null()),
            preserve_wrapper: Cell::new((None, None)),
            read_principals: Cell::new(None),
            job_queue: Cell::new(std::ptr::null_mut()),
            promise_rejection_tracker: Cell::new((None, std::ptr::null_mut())),
            foreground: Default::default(),
            consume_stream: Cell::new((None, None)),
            script_environment_preparer: Cell::new(None),
        }
    }
}

/// The embedder's dispatch callback and its closure (`SetUpEventLoopDispatch`).
#[derive(Clone, Copy)]
struct Dispatch(RustDispatchToEventLoopCallback, *mut c_void);

// SAFETY: SpiderMonkey calls the dispatch callback from its helper threads: embedders make it
// thread-safe (Servo's sends a task to the script thread).
unsafe impl Send for Dispatch {}

/// A runtime's foreground tasks (see the module documentation).
#[derive(Default)]
pub(crate) struct Foreground {
    dispatch: std::sync::Mutex<Option<Dispatch>>,
    /// Tasks waiting for a dispatch callback or for [`RunJobs`].
    pending: std::sync::Mutex<Vec<v8::Task>>,
}

impl Foreground {
    /// Hands `task` to the embedder's event loop, or keeps it.
    fn post(&self, task: v8::Task) {
        let dispatch = *self.dispatch.lock().unwrap();
        let Some(Dispatch(Some(callback), closure)) = dispatch else {
            self.pending.lock().unwrap().push(task);
            return;
        };
        let pointer = Box::into_raw(Box::new(task)) as *mut DispatchablePointer;
        // SAFETY: the embedder's dispatch contract (the task comes back in `DispatchableRun`).
        if !unsafe { callback(closure, pointer) } {
            // The event loop is gone: so may be the isolate (see `roves_v8::foreground`).
            // SAFETY: the pointer was not taken.
            std::mem::forget(unsafe { Box::from_raw(pointer as *mut v8::Task) });
        }
    }
}

/// Registers `cx`'s runtime to receive its isolate's foreground tasks.
pub(crate) fn register_foreground(cx: &JSContext, isolate: &v8::Isolate) {
    let foreground = cx.hooks.foreground.clone();
    roves_v8::foreground::set_handler(roves_v8::foreground::isolate_key(isolate), Some(std::sync::Arc::new(move |task| foreground.post(task))));
}

/// Stops the foreground tasks of `cx`'s runtime and drops the waiting ones (before the isolate
/// is disposed).
pub(crate) fn unregister_foreground(cx: &JSContext, isolate: &v8::Isolate) {
    roves_v8::foreground::set_handler(roves_v8::foreground::isolate_key(isolate), None);
    cx.hooks.foreground.pending.lock().unwrap().clear();
}

/// Runs the foreground tasks waiting in `cx`'s runtime; returns whether any ran.
pub(crate) fn run_pending_tasks(cx: *mut JSContext) -> bool {
    let tasks = std::mem::take(&mut *hooks(cx).foreground.pending.lock().unwrap());
    let ran = !tasks.is_empty();
    for task in tasks {
        task.run();
    }
    ran
}

fn hooks<'h>(cx: *mut JSContext) -> &'h RuntimeHooks {
    // SAFETY: callers pass this thread's live context.
    unsafe { &(*cx).hooks }
}

fn isolate<'i>(cx: *mut JSContext) -> &'i mut v8::Isolate {
    // SAFETY: as above; the isolate is only used from its own thread.
    unsafe { &mut *(*cx).isolate }
}

/// Configures a new runtime's isolate for the JSAPI's model.
pub(crate) fn configure_isolate(isolate: &mut v8::Isolate) {
    isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
    isolate.set_promise_hook(promise_hook);
    isolate.set_promise_reject_callback(promise_reject_callback);
    let full = v8::GCType::kGCTypeMarkSweepCompact;
    isolate.add_gc_prologue_callback(gc_prologue, std::ptr::null_mut(), full);
    isolate.add_gc_epilogue_callback(gc_epilogue, std::ptr::null_mut(), full);
    crate::modules_impl::configure_isolate(isolate);
    isolate.set_modify_code_generation_from_strings_callback(code_generation_from_strings);
    isolate.set_allow_wasm_code_generation_callback(wasm_code_generation);
    configure_wasm_streaming(isolate);
}

// --- Interrupts ------------------------------------------------------------------------------

pub unsafe fn JS_AddInterruptCallback(cx: *mut JSContext, callback: JSInterruptCallback) -> bool {
    hooks(cx).interrupt_callbacks.borrow_mut().push(callback);
    true
}

pub unsafe fn JS_RequestInterruptCallback(cx: *mut JSContext) {
    let handle = isolate(cx).thread_safe_handle();
    handle.request_interrupt(run_interrupt_callbacks, std::ptr::null_mut());
}

pub unsafe fn JS_RequestInterruptCallbackCanWait(cx: *mut JSContext) {
    // SAFETY: forwarded.
    unsafe { JS_RequestInterruptCallback(cx) }
}

/// V8's interrupt: runs the interrupt callbacks; one returning `false` stops the script.
pub(crate) unsafe extern "C" fn run_interrupt_callbacks(_isolate: v8::UnsafeRawIsolatePtr, _data: *mut c_void) {
    let Some(cx) = crate::rust::Runtime::get() else { return };
    let cx = cx.as_ptr();
    let callbacks = hooks(cx).interrupt_callbacks.borrow().clone();
    let mut keep_running = true;
    for callback in callbacks.into_iter().flatten() {
        // SAFETY: SpiderMonkey's interrupt callback contract.
        keep_running &= unsafe { callback(cx) };
    }
    if !keep_running {
        isolate(cx).terminate_execution();
    }
}

// --- GC --------------------------------------------------------------------------------------

/// Registers an embedder root tracer, called on every GC (Servo traces its DOM roots here).
pub unsafe fn JS_AddExtraGCRootsTracer(cx: *mut JSContext, trace: JSTraceDataOp, data: *mut c_void) -> bool {
    hooks(cx).extra_roots.borrow_mut().push((trace, data));
    true
}

pub unsafe fn JS_RemoveExtraGCRootsTracer(cx: *mut JSContext, trace: JSTraceDataOp, data: *mut c_void) {
    hooks(cx).extra_roots.borrow_mut().retain(|entry| *entry != (trace, data));
}

/// Runs the embedder root tracers (from the runtime's root set).
pub(crate) fn trace_extra_roots(cx: &JSContext, visitor: &mut v8::cppgc::Visitor) {
    let Ok(roots) = cx.hooks.extra_roots.try_borrow() else { return };
    let trc = crate::glue::tracer(visitor);
    for (trace, data) in roots.iter() {
        if let Some(trace) = trace {
            // SAFETY: SpiderMonkey's trace-data contract.
            unsafe { trace(trc, *data) };
        }
    }
}

/// Sets the GC callback, told when each of V8's full collections (mark-compact, the ones that
/// trace the embedder's roots) begins and ends. Young-generation collections are not reported.
pub unsafe fn JS_SetGCCallback(cx: *mut JSContext, callback: JSGCCallback, data: *mut c_void) {
    hooks(cx).gc_callback.set((callback, data));
}

pub unsafe fn SetGCSliceCallback(cx: *mut JSContext, callback: GCSliceCallback) -> GCSliceCallback {
    hooks(cx).gc_slice_callback.replace(callback)
}

/// Reports a full collection's start or end to the embedder's GC and GC-slice callbacks (a V8
/// collection is reported as one cycle of one slice, without a description).
fn report_gc(status: crate::jsapi::JSGCStatus) {
    use crate::jsapi::{GCProgress, JSGCStatus};
    let Some(cx) = crate::rust::Runtime::get() else { return };
    let cx = cx.as_ptr();
    let (callback, data) = hooks(cx).gc_callback.get();
    let slice = hooks(cx).gc_slice_callback.get();
    let begin = status == JSGCStatus::JSGC_BEGIN;
    // SAFETY: SpiderMonkey's GC callback contracts; no JS runs in them.
    unsafe {
        if begin {
            if let Some(callback) = callback {
                callback(cx, status, GCReason::API, data);
            }
        }
        if let Some(slice) = slice {
            let steps = if begin {
                [GCProgress::GC_CYCLE_BEGIN, GCProgress::GC_SLICE_BEGIN]
            } else {
                [GCProgress::GC_SLICE_END, GCProgress::GC_CYCLE_END]
            };
            for progress in steps {
                slice(cx, progress, std::ptr::null());
            }
        }
        if !begin {
            if let Some(callback) = callback {
                callback(cx, status, GCReason::API, data);
            }
        }
    }
}

unsafe extern "C" fn gc_prologue(_: v8::UnsafeRawIsolatePtr, _: v8::GCType, _: v8::GCCallbackFlags, _: *mut c_void) {
    report_gc(crate::jsapi::JSGCStatus::JSGC_BEGIN);
}

unsafe extern "C" fn gc_epilogue(_: v8::UnsafeRawIsolatePtr, _: v8::GCType, _: v8::GCCallbackFlags, _: *mut c_void) {
    report_gc(crate::jsapi::JSGCStatus::JSGC_END);
}

pub unsafe fn JS_SetGCParameter(cx: *mut JSContext, key: JSGCParamKey, value: u32) {
    hooks(cx).gc_parameters.borrow_mut().insert(key as i32, value);
}

pub unsafe fn JS_GetGCParameter(cx: *mut JSContext, key: JSGCParamKey) -> u32 {
    hooks(cx).gc_parameters.borrow().get(&(key as i32)).copied().unwrap_or(0)
}

/// A full collection (V8's low-memory notification).
pub unsafe fn JS_GC(cx: *mut JSContext, _reason: GCReason) {
    isolate(cx).low_memory_notification();
}

// --- Job queue and promises ------------------------------------------------------------------

/// The embedder's job queue (mozjs's glue `RustJobQueue`): its traps, kept for reference.
pub struct RustJobQueue {
    _traps: JobQueueTraps,
    _queue: *const c_void,
    _interrupt_queues: *mut c_void,
}

pub unsafe fn CreateJobQueue(traps: *const JobQueueTraps, queue: *const c_void, interrupt_queues: *mut c_void) -> *mut JobQueue {
    // SAFETY: callers pass a valid trap table.
    let job_queue = Box::new(RustJobQueue { _traps: unsafe { *traps }, _queue: queue, _interrupt_queues: interrupt_queues });
    Box::into_raw(job_queue) as *mut JobQueue
}

pub unsafe fn DeleteJobQueue(queue: *mut JobQueue) {
    // SAFETY: queues come from `CreateJobQueue`.
    drop(unsafe { Box::from_raw(queue as *mut RustJobQueue) });
}

pub unsafe fn SetJobQueue(cx: *mut JSContext, queue: *mut JobQueue) {
    hooks(cx).job_queue.set(queue);
}

pub unsafe fn JobQueueIsEmpty(_cx: *mut JSContext) {}

thread_local! {
    /// Whether a drain job is in the embedder's queue (one is enough per checkpoint).
    static DRAIN_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// V8's promise hook: a promise created or resolved may have queued promise jobs.
unsafe extern "C" fn promise_hook(kind: v8::PromiseHookType, _promise: v8::Local<v8::Promise>, _parent: v8::Local<v8::Value>) {
    if matches!(kind, v8::PromiseHookType::Init | v8::PromiseHookType::Resolve) {
        request_drain();
    }
}

/// Enqueues the current realm's drain job in the embedder's job queue (once until it runs).
fn request_drain() {
    if DRAIN_PENDING.with(Cell::get) {
        return;
    }
    let Some(cx) = crate::rust::Runtime::get() else { return };
    let cx = cx.as_ptr();
    let queue = hooks(cx).job_queue.get();
    if queue.is_null() {
        return;
    }
    // SAFETY: embedder job queues come from `CreateJobQueue`.
    let queue = unsafe { &*(queue as *const RustJobQueue) };
    let Some(enqueue) = queue._traps.enqueuePromiseJob else { return };
    let Some(drain) = drain_function(cx) else { return };
    DRAIN_PENDING.with(|pending| pending.set(true));
    crate::rooted!(in(cx) let drain = drain);
    crate::rooted!(in(cx) let none = std::ptr::null_mut::<crate::jsapi::JSObject>());
    // SAFETY: SpiderMonkey's enqueue contract (no promise, allocation site or host data).
    let ok = unsafe {
        enqueue(queue._queue, cx, none.handle().into(), drain.handle().into(), none.handle().into(), none.handle().into())
    };
    if !ok {
        DRAIN_PENDING.with(|pending| pending.set(false));
    }
}

/// The current realm's drain function (made once per realm, kept by the realm).
fn drain_function(cx: *mut JSContext) -> Option<*mut crate::jsapi::JSObject> {
    let realm = crate::realm_impl::get_context_realm(cx);
    let data = crate::realm_impl::realm_data(realm)?;
    if data.drain_function.get().is_null() {
        // SAFETY: a live context; the function is stored in (and traced by) the realm.
        let function = unsafe { crate::jsapi_impl::JS_NewFunction(cx, Some(drain_microtasks), 0, 0, c"runPromiseJobs".as_ptr()) };
        if function.is_null() {
            return None;
        }
        data.drain_function.set(function as *mut crate::jsapi::JSObject);
    }
    Some(data.drain_function.get())
}

/// The drain job: runs V8's pending promise jobs (at the embedder's checkpoint).
unsafe extern "C" fn drain_microtasks(cx: *mut JSContext, _argc: u32, vp: *mut crate::jsval::JSVal) -> bool {
    DRAIN_PENDING.with(|pending| pending.set(false));
    isolate(cx).perform_microtask_checkpoint();
    // SAFETY: the JSNative contract.
    unsafe { *vp = crate::jsval::UndefinedValue() };
    true
}

pub unsafe fn JobQueueMayNotBeEmpty(_cx: *mut JSContext) {}

/// Runs the pending promise jobs (a V8 microtask checkpoint).
/// Runs the waiting foreground tasks and promise jobs (embedders without a job queue).
pub unsafe fn RunJobs(cx: *mut JSContext) {
    loop {
        isolate(cx).perform_microtask_checkpoint();
        if !run_pending_tasks(cx) {
            break;
        }
    }
}

pub unsafe fn SetPromiseRejectionTrackerCallback(cx: *mut JSContext, callback: PromiseRejectionTrackerCallback, data: *mut c_void) {
    hooks(cx).promise_rejection_tracker.set((callback, data));
}

/// V8's promise rejection events, reported to the embedder's tracker.
unsafe extern "C" fn promise_reject_callback(message: v8::PromiseRejectMessage) {
    let Some(cx) = crate::rust::Runtime::get() else { return };
    let cx = cx.as_ptr();
    let (Some(callback), data) = hooks(cx).promise_rejection_tracker.get() else { return };
    let state = match message.get_event() {
        v8::PromiseRejectEvent::PromiseRejectWithNoHandler => PromiseRejectionHandlingState::Unhandled,
        v8::PromiseRejectEvent::PromiseHandlerAddedAfterReject => PromiseRejectionHandlingState::Handled,
        _ => return,
    };
    // SAFETY: the callback runs on the isolate's thread, inside V8.
    let raw = unsafe { &*cx };
    let promise = raw.with_scope(|scope| {
        let promise = v8::Local::new(scope, message.get_promise());
        from_v8(scope, promise.into()).to_object()
    });
    crate::rooted!(in(cx) let promise = promise);
    // SAFETY: SpiderMonkey's tracker contract.
    unsafe { callback(cx, false, promise.handle().into_handle(), state, data) };
}

// --- Settings and callbacks kept for the embedder --------------------------------------------

pub unsafe fn ContextOptionsRef(cx: *mut JSContext) -> *mut ContextOptions {
    &*hooks(cx).context_options as *const ContextOptions as *mut ContextOptions
}

pub unsafe fn JS_SetGlobalJitCompilerOption(_cx: *mut JSContext, _option: JSJitCompilerOption, _value: u32) {}

pub unsafe fn JS_SetOffthreadIonCompilationEnabled(_cx: *mut JSContext, _enabled: bool) {}

pub unsafe fn DisableJitBackend() {}

/// Sets the security callbacks. Their CSP check (`contentSecurityPolicyAllows`) decides on
/// `eval`, `Function` and WebAssembly compilation: realms disallow code generation from
/// strings, so V8 asks [`code_generation_from_strings`] and [`wasm_code_generation`].
pub unsafe fn JS_SetSecurityCallbacks(cx: *mut JSContext, callbacks: *const JSSecurityCallbacks) {
    hooks(cx).security_callbacks.set(callbacks);
}

/// The source V8 builds for the `Function` constructor: `(function anonymous(<params>\n) {\n<body>\n})`
/// (or the async/generator forms). Returns the parameters and the body.
fn function_constructor_parts(source: &str) -> Option<(&str, &str)> {
    let rest = ["(function anonymous(", "(async function anonymous(", "(function* anonymous(", "(async function* anonymous("]
        .iter()
        .find_map(|prefix| source.strip_prefix(prefix))?;
    let (parameters, body) = rest.split_once("\n) {\n")?;
    Some((parameters, body.strip_suffix("\n})")?))
}

/// Asks the embedder's CSP check whether `kind` code may be compiled in the realm of `context`
/// (true without a check). `source` is the code (JS) or null (WebAssembly).
fn csp_allows(scope: &mut v8::PinScope, kind: crate::jsapi::RuntimeCode, source: Option<v8::Local<v8::String>>) -> bool {
    use crate::jsapi::{CompilationType, JSString, StackGCVector};
    let Some(cx) = crate::rust::Runtime::get() else { return true };
    let cx = cx.as_ptr();
    // SAFETY: security callbacks are static tables (JSAPI contract).
    let Some(check) = (unsafe { hooks(cx).security_callbacks.get().as_ref() }).and_then(|callbacks| callbacks.contentSecurityPolicyAllows) else {
        return true;
    };
    let text = source.map(|source| source.to_rust_string_lossy(scope));
    let function = text.as_deref().and_then(function_constructor_parts);
    let compilation_type = if function.is_some() { CompilationType::Function } else { CompilationType::DirectEval };
    let to_string = |scope: &mut v8::PinScope, text: Option<&str>| -> *mut JSString {
        match text.and_then(|text| v8::String::new(scope, text)) {
            Some(string) => crate::jsval::from_v8(scope, string.into()).to_string(),
            None => std::ptr::null_mut(),
        }
    };
    let code = to_string(scope, text.as_deref());
    let body = to_string(scope, function.map(|(_, body)| body));
    let parameter = to_string(scope, function.map(|(parameters, _)| parameters));
    crate::rooted!(in(cx) let code = code);
    crate::rooted!(in(cx) let body = body);
    // V8 joins the parameters into one list: they are reported as one string.
    let mut parameters: Vec<*mut JSString> = Vec::new();
    if !parameter.is_null() {
        parameters.push(parameter);
    }
    crate::rooted!(in(cx) let parameter = parameter);
    let arguments: Vec<crate::jsval::JSVal> = Vec::new();
    crate::rooted!(in(cx) let body_argument = crate::jsval::UndefinedValue());
    let parameter_handle = crate::jsapi::Handle { _phantom_0: std::marker::PhantomData, ptr: &parameters as *const Vec<*mut JSString> as *const StackGCVector<*mut JSString> };
    let argument_handle = crate::jsapi::Handle { _phantom_0: std::marker::PhantomData, ptr: &arguments as *const Vec<crate::jsval::JSVal> as *const StackGCVector<crate::jsval::JSVal> };
    // The check runs in the realm compiling the code.
    // SAFETY: the runtime's live context.
    let raw = unsafe { &*cx };
    let old = raw.current_realm.get();
    raw.set_current_realm(crate::realm_impl::realm_of_context(scope.get_current_context()));
    let mut allowed = false;
    // SAFETY: SpiderMonkey's CSP-check contract (rooted strings, valid vectors).
    let ok = unsafe {
        check(cx, kind, code.handle().into(), compilation_type, parameter_handle, body.handle().into(), argument_handle, body_argument.handle().into(), &mut allowed)
    };
    raw.set_current_realm(old);
    let _ = parameter;
    ok && allowed
}

fn code_generation_from_strings<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: v8::Local<'s, v8::Value>,
    _is_code_like: bool,
) -> v8::ModifyCodeGenerationFromStringsResult<'s> {
    // A non-string `eval` argument is returned unchanged (nothing is compiled).
    let Ok(string) = v8::Local::<v8::String>::try_from(source) else {
        return v8::ModifyCodeGenerationFromStringsResult { codegen_allowed: true, modified_source: None };
    };
    let allowed = csp_allows(scope, crate::jsapi::RuntimeCode::JS, Some(string));
    v8::ModifyCodeGenerationFromStringsResult { codegen_allowed: allowed, modified_source: allowed.then_some(string) }
}

unsafe extern "C" fn wasm_code_generation(context: v8::Local<v8::Context>, _source: v8::Local<v8::String>) -> bool {
    // SAFETY: V8 calls this with the context entered.
    v8::callback_scope!(unsafe scope, context);
    csp_allows(scope, crate::jsapi::RuntimeCode::WASM, None)
}

pub unsafe fn SetDOMCallbacks(cx: *mut JSContext, callbacks: *const DOMCallbacks) {
    hooks(cx).dom_callbacks.set(callbacks);
}

pub unsafe fn SetPreserveWrapperCallbacks(cx: *mut JSContext, preserve: PreserveWrapperCallback, has_released: HasReleasedWrapperCallback) {
    hooks(cx).preserve_wrapper.set((preserve, has_released));
}

pub unsafe fn JS_InitReadPrincipalsCallback(cx: *mut JSContext, read: JSReadPrincipalsOp) {
    hooks(cx).read_principals.set(read);
}

thread_local! {
    static BUILD_ID_OP: Cell<BuildIdOp> = const { Cell::new(None) };
}

pub unsafe fn SetProcessBuildIdOp(op: BuildIdOp) {
    BUILD_ID_OP.with(|build_id| build_id.set(op));
}

/// SpiderMonkey's build id vector is a C++ container; V8 has its own cache keys.
pub unsafe fn SetBuildId(_build_id: *mut BuildIdCharVector, _chars: *const c_char, _len: usize) -> bool {
    true
}

pub unsafe fn InitializeMemoryReporter(_want_to_measure: WantToMeasure) {}

/// Fills in V8's heap statistics.
pub unsafe fn CollectServoSizes(cx: *mut JSContext, sizes: *mut ServoSizes, _get_size: GetSize) -> bool {
    let statistics = isolate(cx).get_heap_statistics();
    // SAFETY: callers pass a valid out pointer.
    let sizes = unsafe { &mut *sizes };
    sizes.gcHeapUsed = statistics.used_heap_size();
    sizes.gcHeapUnused = statistics.total_heap_size().saturating_sub(statistics.used_heap_size());
    sizes.mallocHeap = statistics.malloced_memory();
    sizes.nonHeap = statistics.external_memory();
    true
}

/// Sets the embedder's dispatch callback, which brings V8's foreground tasks to its event loop.
pub unsafe fn SetUpEventLoopDispatch(cx: *mut JSContext, callback: RustDispatchToEventLoopCallback, closure: *mut c_void) {
    let foreground = &hooks(cx).foreground;
    *foreground.dispatch.lock().unwrap() = Some(Dispatch(callback, closure));
    let waiting = std::mem::take(&mut *foreground.pending.lock().unwrap());
    for task in waiting {
        foreground.post(task);
    }
}

/// Runs a foreground task the embedder's event loop received (or drops it at shutdown).
pub unsafe fn DispatchableRun(_cx: *mut JSContext, ptr: *mut DispatchablePointer, maybe_shutting_down: Dispatchable_MaybeShuttingDown) {
    // SAFETY: dispatched pointers come from `Foreground::post`.
    let task = unsafe { *Box::from_raw(ptr as *mut v8::Task) };
    if maybe_shutting_down == Dispatchable_MaybeShuttingDown::NotShuttingDown {
        task.run();
    }
}

pub unsafe fn InitConsumeStreamCallback(cx: *mut JSContext, consume: ConsumeStreamCallback, report: ReportStreamErrorCallback) {
    hooks(cx).consume_stream.set((consume, report));
}

pub unsafe fn RegisterScriptEnvironmentPreparer(cx: *mut JSContext, hook: InvokeScriptPreparerHook) {
    hooks(cx).script_environment_preparer.set(hook);
}

/// SpiderMonkey hands its C++ closure to the preparer hook; V8 never calls the hook.
pub unsafe fn RunScriptEnvironmentPreparerClosure(_cx: *mut JSContext, _closure: *mut ScriptEnvironmentPreparer_Closure) -> bool {
    false
}

// --- WebAssembly streaming ------------------------------------------------------------------
//
// `WebAssembly.compileStreaming`/`instantiateStreaming`: V8 hands the source (a `Response` or a
// promise of one) to [`wasm_streaming`]. Once the source settles, the response goes to the
// embedder's consume-stream callback (SpiderMonkey's model), which checks it and later feeds
// its body back through the `StreamConsumer*` functions below into V8's streaming compiler.

/// The `StreamConsumer` of one streaming compilation.
struct WasmConsumer {
    streaming: std::cell::RefCell<Option<v8::WasmStreaming<false>>>,
}

fn consumer<'c>(sc: *mut crate::jsapi::StreamConsumer) -> Option<&'c WasmConsumer> {
    // SAFETY: consumers come from `wasm_streaming` and live until their stream ends.
    unsafe { (sc as *const WasmConsumer).as_ref() }
}

/// Ends a streaming compilation: frees its consumer and returns its V8 stream.
fn finish_consumer(sc: *mut crate::jsapi::StreamConsumer) -> Option<v8::WasmStreaming<false>> {
    if sc.is_null() {
        return None;
    }
    // SAFETY: as above; the stream has ended, so the embedder no longer uses the consumer.
    let consumer = unsafe { Box::from_raw(sc as *mut WasmConsumer) };
    consumer.streaming.take()
}

pub(crate) fn configure_wasm_streaming(isolate: &mut v8::Isolate) {
    isolate.set_wasm_streaming_callback(wasm_streaming);
}

fn wasm_streaming<'s>(scope: &mut v8::PinScope<'s, '_>, source: v8::Local<'s, v8::Value>, streaming: v8::WasmStreaming<false>) {
    let consumer = Box::into_raw(Box::new(WasmConsumer { streaming: std::cell::RefCell::new(Some(streaming)) }));
    let settle = |scope: &mut v8::PinScope<'s, '_>| -> Option<()> {
        let resolver = v8::PromiseResolver::new(scope)?;
        resolver.resolve(scope, source)?;
        let promise = resolver.get_promise(scope);
        let data = v8::External::new(scope, consumer as *mut c_void);
        let fulfilled = v8::Function::builder(wasm_source_fulfilled).data(data.into()).build(scope)?;
        let rejected = v8::Function::builder(wasm_source_rejected).data(data.into()).build(scope)?;
        promise.then2(scope, fulfilled, rejected)?;
        Some(())
    };
    if settle(scope).is_none() {
        if let Some(streaming) = finish_consumer(consumer as *mut crate::jsapi::StreamConsumer) {
            streaming.abort(None);
        }
    }
}

fn wasm_source_rejected(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    let Ok(data) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    if let Some(streaming) = finish_consumer(data.value() as *mut crate::jsapi::StreamConsumer) {
        let reason = args.get(0);
        streaming.abort(Some(reason));
    }
    let _ = scope;
}

fn wasm_source_fulfilled(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    let Ok(data) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    let sc = data.value() as *mut crate::jsapi::StreamConsumer;
    let abort = |scope: &mut v8::PinScope, message: &str| {
        let message = v8::String::new(scope, message).expect("a short string");
        let error = v8::Exception::type_error(scope, message);
        if let Some(streaming) = finish_consumer(sc) {
            streaming.abort(Some(error));
        }
    };
    let Some(cx) = crate::rust::Runtime::get() else { return };
    let cx = cx.as_ptr();
    let (Some(consume), _) = hooks(cx).consume_stream.get() else {
        abort(scope, "WebAssembly streaming is not supported");
        return;
    };
    let response = args.get(0);
    if !response.is_object() {
        abort(scope, "expected Response or Promise resolving to Response");
        return;
    }
    let response = crate::jsval::from_v8(scope, response).to_object();
    crate::rooted!(in(cx) let response = response);
    // The embedder checks the response in the realm compiling it.
    // SAFETY: the runtime's live context.
    let raw = unsafe { &*cx };
    let old = raw.current_realm.get();
    raw.set_current_realm(crate::realm_impl::realm_of_context(scope.get_current_context()));
    // SAFETY: SpiderMonkey's consume-stream contract (rooted response, live consumer).
    let accepted = unsafe { consume(cx, response.handle().into(), crate::jsapi::MimeType::Wasm, sc) };
    raw.set_current_realm(old);
    if !accepted {
        let exception = raw.pending_exception.borrow_mut().take().map(|exception| v8::Local::new(scope, exception));
        match exception {
            Some(exception) => {
                if let Some(streaming) = finish_consumer(sc) {
                    streaming.abort(Some(exception));
                }
            },
            None => abort(scope, "the Response could not be consumed"),
        }
    }
}

pub unsafe fn StreamConsumerConsumeChunk(sc: *mut crate::jsapi::StreamConsumer, begin: *const u8, length: usize) -> bool {
    let Some(consumer) = consumer(sc) else { return false };
    let mut streaming = consumer.streaming.borrow_mut();
    let Some(streaming) = streaming.as_mut() else { return false };
    // SAFETY: callers pass a valid chunk.
    let chunk = if length == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(begin, length) } };
    JSContext::current().with_scope(|_| streaming.on_bytes_received(chunk));
    true
}

pub unsafe fn StreamConsumerStreamEnd(sc: *mut crate::jsapi::StreamConsumer) {
    if let Some(streaming) = finish_consumer(sc) {
        JSContext::current().with_scope(|_| streaming.finish());
    }
}

pub unsafe fn StreamConsumerStreamError(sc: *mut crate::jsapi::StreamConsumer, _error_code: usize) {
    if let Some(streaming) = finish_consumer(sc) {
        JSContext::current().with_scope(|scope| {
            let message = v8::String::new(scope, "WebAssembly streaming failed").expect("a short string");
            let error = v8::Exception::type_error(scope, message);
            streaming.abort(Some(error));
        });
    }
}

pub unsafe fn StreamConsumerNoteResponseURLs(sc: *mut crate::jsapi::StreamConsumer, maybe_url: *const c_char, _maybe_source_map_url: *const c_char) {
    let Some(consumer) = consumer(sc) else { return };
    if maybe_url.is_null() {
        return;
    }
    // SAFETY: callers pass a NUL-terminated URL.
    let url = unsafe { std::ffi::CStr::from_ptr(maybe_url) }.to_string_lossy();
    if let Some(streaming) = consumer.streaming.borrow_mut().as_mut() {
        streaming.set_url(&url);
    }
}
