/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Runtime hooks and settings of the JSAPI (interrupts, extra GC roots, the job queue,
//! promise rejection tracking, GC and JIT settings) on V8.
//!
//! **Promise jobs.** SpiderMonkey hands every promise job to the embedder's job queue
//! (`JobQueueTraps::enqueuePromiseJob`), and the embedder runs it. V8 has no such hook: its
//! microtask queue is the only queue. Runtimes use V8's explicit microtask policy, and
//! [`RunJobs`] performs a microtask checkpoint; the embedder's job-queue traps are kept but
//! not called for promise jobs.
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
    event_loop_dispatch: Cell<(RustDispatchToEventLoopCallback, *mut c_void)>,
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
            event_loop_dispatch: Cell::new((None, std::ptr::null_mut())),
            consume_stream: Cell::new((None, None)),
            script_environment_preparer: Cell::new(None),
        }
    }
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
    isolate.set_promise_reject_callback(promise_reject_callback);
    crate::modules_impl::configure_isolate(isolate);
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

/// Records the GC callback (V8's collections are not reported to it yet).
pub unsafe fn JS_SetGCCallback(cx: *mut JSContext, callback: JSGCCallback, data: *mut c_void) {
    hooks(cx).gc_callback.set((callback, data));
}

pub unsafe fn SetGCSliceCallback(cx: *mut JSContext, callback: GCSliceCallback) -> GCSliceCallback {
    hooks(cx).gc_slice_callback.replace(callback)
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

pub unsafe fn JobQueueMayNotBeEmpty(_cx: *mut JSContext) {}

/// Runs the pending promise jobs (a V8 microtask checkpoint).
pub unsafe fn RunJobs(cx: *mut JSContext) {
    isolate(cx).perform_microtask_checkpoint();
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

/// Records the security callbacks (CSP checks of `eval` are not wired to V8 yet).
pub unsafe fn JS_SetSecurityCallbacks(cx: *mut JSContext, callbacks: *const JSSecurityCallbacks) {
    hooks(cx).security_callbacks.set(callbacks);
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

pub unsafe fn SetUpEventLoopDispatch(cx: *mut JSContext, callback: RustDispatchToEventLoopCallback, closure: *mut c_void) {
    hooks(cx).event_loop_dispatch.set((callback, closure));
}

/// Runs a dispatched task (SpiderMonkey's off-thread work); none are dispatched on V8.
pub unsafe fn DispatchableRun(_cx: *mut JSContext, _ptr: *mut DispatchablePointer, _maybe_shutting_down: Dispatchable_MaybeShuttingDown) {}

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
