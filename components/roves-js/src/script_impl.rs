/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Script compilation and execution, script privates, scripted callers, error reports and
//! saved stacks (SpiderMonkey's `SavedFrame` objects) on V8.
//!
//! - A `*mut JSScript` is a cell holding V8's `UnboundScript`; it runs in the current realm.
//! - Every classic script and source module gets its own host token, which a native WeakMap
//!   ties to the source's private (see `script_metadata`), kept by script id too. The private
//!   of the source a running function (or its eval code) came from is found from the host
//!   options V8 reports for it: `JS_GetScriptedCallerPrivate` and dynamic `import()`.
//! - Saved frames are plain objects whose fields are V8 private properties, built from V8's
//!   current stack trace when captured.

use std::ffi::{CStr, CString, c_char, c_void};

use crate::glue::StringCallback;
use crate::jsapi::{
    EnvironmentChain, Handle, HandleObject, JSContext, JSErrorReport, JSErrorFormatString, JSFunction, JSObject,
    JSPrincipals, JSRuntime, JSScript, JSString, JSTracer, MutableHandle, MutableHandleObject, MutableHandleValue,
    ReadOnlyCompileOptions, SavedFrameResult, SavedFrameSelfHosted, ScriptPrivateReferenceHook, SourceText,
    StackCapture, StackFormat, SupportUnscopables, TaggedColumnNumberOneOrigin, Utf8Unit,
};
use crate::jsval::{JSVal, UndefinedValue, from_v8};

fn raw<'a>(cx: *mut JSContext) -> &'a JSContext {
    // SAFETY: JSAPI callers pass this thread's live context.
    unsafe { &*cx }
}

/// Roves extension (no SpiderMonkey counterpart): traces the GC edges an embedder-owned
/// script private holds (`value` is the private, never undefined). It runs during GC: it
/// must only report edges through the tracer: no allocation, JS, GC or engine table access.
pub type ScriptPrivateTraceHook = Option<unsafe extern "C" fn(trc: *mut JSTracer, value: *const JSVal)>;

/// The runtime's script state: privates by script id, the reference and trace hooks, and
/// the last error report handed out (with the strings it points at).
#[derive(Default)]
pub(crate) struct Scripts {
    metadata: crate::script_metadata::ScriptMetadata,
    reference_hooks: std::cell::Cell<(ScriptPrivateReferenceHook, ScriptPrivateReferenceHook)>,
    trace_hook: std::cell::Cell<ScriptPrivateTraceHook>,
    last_report: std::cell::RefCell<Option<(Box<JSErrorReport>, CString, CString)>>,
}

/// A script or module private, with the release and trace hooks current when it was set:
/// tracing and releasing it never query the context (GC may run them).
#[derive(Clone, Copy)]
pub(crate) struct HostPrivate {
    value: JSVal,
    release: ScriptPrivateReferenceHook,
    trace: ScriptPrivateTraceHook,
}

impl HostPrivate {
    pub(crate) const NONE: HostPrivate = HostPrivate { value: UndefinedValue(), release: None, trace: None };

    /// Takes the embedder's reference to `value` (none for undefined).
    pub(crate) fn retain(cx: &JSContext, value: JSVal) -> HostPrivate {
        if value.is_undefined() {
            return HostPrivate::NONE;
        }
        let (add_ref, release) = cx.scripts.reference_hooks.get();
        if let Some(add_ref) = add_ref {
            unsafe { add_ref(&value) };
        }
        HostPrivate { value, release, trace: cx.scripts.trace_hook.get() }
    }

    pub(crate) fn value(&self) -> JSVal {
        self.value
    }

    /// The private value itself and, through the embedder's hook, the edges it holds.
    pub(crate) fn trace(&self, visitor: &mut v8::cppgc::Visitor) {
        use crate::gc::RootKind;
        self.value.trace_root(visitor);
        if let (false, Some(trace)) = (self.value.is_undefined(), self.trace) {
            unsafe { trace(crate::glue::tracer(visitor), &self.value) };
        }
    }

    /// Drops the embedder's reference. Callers first unlink the private from what traces it.
    pub(crate) fn release(self) {
        if let (false, Some(release)) = (self.value.is_undefined(), self.release) {
            // Reference hooks follow SpiderMonkey's non-GC retain/release contract.
            unsafe { release(&self.value) };
        }
    }
}

/// Balance embedder-owned private references before the live runtime is dismantled:
/// source metadata (classic scripts and source modules) once, then JSON module privates.
pub(crate) fn release_runtime_privates(cx: &JSContext) {
    cx.scripts.metadata.release_all();
    crate::modules_impl::release_json_privates(cx);
}

pub(crate) fn initialize_metadata(cx: &JSContext, scope: &mut v8::PinScope) -> Option<()> {
    cx.scripts.metadata.initialize(scope)
}

/// The private of the classic script or source module with V8 id `script_id`.
pub(crate) fn script_private(cx: &JSContext, script_id: i32) -> Option<JSVal> {
    cx.scripts.metadata.private(script_id)
}

/// Sets the private of the classic script or source module with V8 id `script_id`.
pub(crate) fn set_script_private(cx: &JSContext, script_id: i32, value: JSVal) {
    cx.catching(|scope| cx.scripts.metadata.set(cx, scope, script_id, value));
}

/// Ties a new source's host token to its metadata (see `source_origin`).
pub(crate) fn register_source(scope: &mut v8::PinScope, script_id: i32, token: v8::Local<v8::Symbol>) {
    JSContext::current().scripts.metadata.register(scope, script_id, token);
}

/// Anchors a new source module's record under its source's token (see `script_metadata`).
pub(crate) fn anchor_source_module(scope: &mut v8::PinScope, script_id: i32, record: *mut std::ffi::c_void) -> Option<()> {
    JSContext::current().scripts.metadata.anchor_module(scope, script_id, record)
}

/// The host token in a source's host options (none for sources compiled without one).
pub(crate) fn source_token<'s>(scope: &mut v8::PinScope<'s, '_>, options: v8::Local<'s, v8::Data>) -> Option<v8::Local<'s, v8::Symbol>> {
    if !options.is_fixed_array() {
        return None;
    }
    // ScriptOrigin's host options are a PrimitiveArray (V8's FixedArray storage).
    let options = unsafe { v8::Local::<v8::PrimitiveArray>::cast_unchecked(options) };
    if options.length() != 1 {
        return None;
    }
    let token: v8::Local<v8::Value> = options.get(scope, 0).into();
    v8::Local::<v8::Symbol>::try_from(token).ok()
}

/// A new private holder and a weak handle on its data (see `ScriptMetadata::new_holder`).
pub(crate) fn new_private_holder<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<(v8::Local<'s, v8::Object>, v8::cppgc::WeakPersistent<crate::script_metadata::PrivateData>)> {
    JSContext::current().scripts.metadata.new_holder(scope)
}

/// The private of the source whose host token is `token`; undefined if it has none.
pub(crate) fn token_private(scope: &mut v8::PinScope, token: v8::Local<v8::Symbol>) -> JSVal {
    JSContext::current().scripts.metadata.token_private(scope, token).unwrap_or(UndefinedValue())
}

// --- Compile options and sources -----------------------------------------------------------

/// New compile options for `file` (which the caller keeps alive) starting at `line`.
pub unsafe fn NewCompileOptions(_cx: *mut JSContext, file: *const c_char, line: u32) -> *mut ReadOnlyCompileOptions {
    // SAFETY: SpiderMonkey's compile options are plain data, valid when zeroed.
    let mut options: Box<ReadOnlyCompileOptions> = Box::new(unsafe { std::mem::zeroed() });
    options._base.filename_.data_ = file;
    options.lineno = line;
    options.column._base = 1;
    Box::into_raw(options)
}

pub unsafe fn DeleteCompileOptions(options: *mut ReadOnlyCompileOptions) {
    // SAFETY: options come from `NewCompileOptions`.
    drop(unsafe { Box::from_raw(options) });
}

pub(crate) fn options_filename(options: &ReadOnlyCompileOptions) -> String {
    let data = options._base.filename_.data_;
    if data.is_null() {
        return String::new();
    }
    // SAFETY: filenames are NUL-terminated (kept alive by the options' owner).
    unsafe { CStr::from_ptr(data) }.to_string_lossy().into_owned()
}

pub(crate) fn script_origin<'s>(scope: &mut v8::PinScope<'s, '_>, options: &ReadOnlyCompileOptions, is_module: bool) -> v8::ScriptOrigin<'s> {
    script_origin_with_data(scope, options, is_module, None)
}

fn script_origin_with_data<'s>(
    scope: &mut v8::PinScope<'s, '_>, options: &ReadOnlyCompileOptions, is_module: bool,
    host_data: Option<v8::Local<'s, v8::Data>>,
) -> v8::ScriptOrigin<'s> {
    let name = v8::String::new(scope, &options_filename(options)).unwrap_or_else(|| v8::String::empty(scope));
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        options.lineno.saturating_sub(1) as i32,
        options.column._base.saturating_sub(1) as i32,
        !options._base.mutedErrors_,
        -1,
        None,
        options._base.mutedErrors_,
        false,
        is_module,
        host_data,
    )
}

/// The origin of a script or module source with its own host token. V8's compilation cache
/// only shares a script between sources with strictly equal host options, so each source
/// keeps its own script id (and private) even when code and name are identical.
pub(crate) fn source_origin<'s>(scope: &mut v8::PinScope<'s, '_>, options: &ReadOnlyCompileOptions, is_module: bool)
    -> (v8::ScriptOrigin<'s>, v8::Local<'s, v8::Symbol>)
{
    let token = v8::Symbol::new(scope, None);
    let data = v8::PrimitiveArray::new(scope, 1);
    data.set(scope, 0, token.into());
    (script_origin_with_data(scope, options, is_module, Some(data.into())), token)
}

fn utf8_source<'s>(scope: &mut v8::PinScope<'s, '_>, source: *mut SourceText<Utf8Unit>) -> Option<v8::Local<'s, v8::String>> {
    // SAFETY: callers pass a valid source text whose units outlive the call.
    let source = unsafe { &*source };
    let bytes = if source.length_ == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(source.units_ as *const u8, source.length_ as usize) }
    };
    v8::String::new_from_utf8(scope, bytes, v8::NewStringType::Normal)
}

fn utf16_source<'s>(scope: &mut v8::PinScope<'s, '_>, source: *mut SourceText<u16>) -> Option<v8::Local<'s, v8::String>> {
    // SAFETY: as above.
    let source = unsafe { &*source };
    let units = if source.length_ == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(source.units_, source.length_ as usize) } };
    v8::String::new_from_two_byte(scope, units, v8::NewStringType::Normal)
}

/// Compiles a classic script (not yet bound to a realm).
pub(crate) fn compile_script<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    options: &ReadOnlyCompileOptions,
    code: v8::Local<'s, v8::String>,
) -> Option<v8::Local<'s, v8::UnboundScript>> {
    let (origin, token) = source_origin(scope, options, false);
    let mut source = v8::script_compiler::Source::new(code, Some(&origin));
    let script = v8::script_compiler::compile_unbound_script(
        scope,
        &mut source,
        v8::script_compiler::CompileOptions::NoCompileOptions,
        v8::script_compiler::NoCacheReason::NoReason,
    )?;
    register_source(scope, script.script_id(), token);
    Some(script)
}

pub unsafe fn Compile1(cx: *mut JSContext, options: *const ReadOnlyCompileOptions, source: *mut SourceText<Utf8Unit>) -> *mut JSScript {
    // SAFETY: callers pass valid options.
    let options = unsafe { &*options };
    raw(cx)
        .catching(|scope| {
            let code = utf8_source(scope, source)?;
            let script = compile_script(scope, options, code)?;
            Some(crate::cell::new_script_cell(scope, script) as *mut JSScript)
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Runs a compiled script in the current realm.
pub unsafe fn JS_ExecuteScript(cx: *mut JSContext, script: Handle<*mut JSScript>, mut rval: MutableHandle<JSVal>) -> bool {
    let script = script.get();
    let result = raw(cx).catching(|scope| {
        // SAFETY: a rooted script cell.
        let unbound = unsafe { crate::cell::cell_script(scope, script as *mut c_void) }?;
        let bound = unbound.bind_to_current_context(scope);
        let value = bound.run(scope)?;
        Some(from_v8(scope, value))
    });
    match result {
        Some(value) => {
            rval.set(value);
            true
        },
        None => false,
    }
}

/// Compiles and runs a classic script in the current realm.
pub unsafe fn Evaluate2(cx: *mut JSContext, options: *const ReadOnlyCompileOptions, source: *mut SourceText<Utf8Unit>, mut rval: MutableHandle<JSVal>) -> bool {
    // SAFETY: callers pass valid options.
    let options = unsafe { &*options };
    let result = raw(cx).catching(|scope| {
        let code = utf8_source(scope, source)?;
        let script = compile_script(scope, options, code)?;
        let value = script.bind_to_current_context(scope).run(scope)?;
        Some(from_v8(scope, value))
    });
    match result {
        Some(value) => {
            rval.set(value);
            true
        },
        None => false,
    }
}

/// The objects of an environment chain (`with`-like scopes for event handler bodies).
pub(crate) struct EnvChain {
    objects: Vec<*mut JSObject>,
}

unsafe fn trace_env_chain(location: *const c_void, visitor: &mut v8::cppgc::Visitor) {
    // SAFETY: the chain is unregistered before it is freed.
    let chain = unsafe { &*(location as *const EnvChain) };
    for object in &chain.objects {
        crate::cell::trace_cell(*object as *mut c_void, visitor);
    }
}

pub unsafe fn NewEnvironmentChain(_cx: *mut JSContext, _support_unscopables: SupportUnscopables) -> *mut EnvironmentChain {
    let chain = Box::new(EnvChain { objects: Vec::new() });
    let pointer = Box::into_raw(chain);
    crate::gc::register_custom_root(pointer as *const c_void, trace_env_chain);
    pointer as *mut EnvironmentChain
}

pub unsafe fn AppendToEnvironmentChain(chain: *mut EnvironmentChain, obj: *mut JSObject) -> bool {
    // SAFETY: chains come from `NewEnvironmentChain`.
    unsafe { &mut *(chain as *mut EnvChain) }.objects.push(obj);
    true
}

pub unsafe fn DeleteEnvironmentChain(chain: *mut EnvironmentChain) {
    crate::gc::unregister_custom_root(chain as *const c_void);
    // SAFETY: as above.
    drop(unsafe { Box::from_raw(chain as *mut EnvChain) });
}

/// Compiles a function body with `nargs` named arguments, scoped by the environment chain's
/// objects (V8's context extensions: name lookups go through them first, innermost last).
pub unsafe fn CompileFunction(
    cx: *mut JSContext,
    env_chain: *const EnvironmentChain,
    options: *const ReadOnlyCompileOptions,
    name: *const c_char,
    nargs: u32,
    argnames: *const *const c_char,
    source: *mut SourceText<u16>,
) -> *mut JSFunction {
    // SAFETY: callers pass valid options, names and chain.
    let options = unsafe { &*options };
    let objects: Vec<*mut JSObject> = unsafe { env_chain.as_ref() }.map_or(Vec::new(), |chain| unsafe { &*(chain as *const EnvironmentChain as *const EnvChain) }.objects.clone());
    raw(cx)
        .catching(|scope| {
            let code = utf16_source(scope, source)?;
            let mut arguments = Vec::with_capacity(nargs as usize);
            for index in 0..nargs as usize {
                // SAFETY: `argnames` holds `nargs` NUL-terminated names.
                let argument = unsafe { CStr::from_ptr(*argnames.add(index)) }.to_string_lossy();
                arguments.push(v8::String::new(scope, &argument)?);
            }
            let mut extensions = Vec::with_capacity(objects.len());
            for object in &objects {
                // SAFETY: the chain roots its objects.
                let value = unsafe { crate::cell::cell_value(scope, *object as *mut c_void) };
                extensions.push(v8::Local::<v8::Object>::try_from(value).ok()?);
            }
            // SpiderMonkey's chain lists the innermost scope last; V8 looks up the extensions
            // from the last one too.
            let origin = script_origin(scope, options, false);
            let mut source = v8::script_compiler::Source::new(code, Some(&origin));
            let function = v8::script_compiler::compile_function(
                scope,
                &mut source,
                &arguments,
                &extensions,
                v8::script_compiler::CompileOptions::NoCompileOptions,
                v8::script_compiler::NoCacheReason::NoReason,
            )?;
            if !name.is_null() {
                // SAFETY: a NUL-terminated name.
                let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
                let name = v8::String::new(scope, &name)?;
                function.set_name(name);
            }
            Some(from_v8(scope, function.into()).to_object() as *mut JSFunction)
        })
        .unwrap_or(std::ptr::null_mut())
}

// --- Script privates and the scripted caller -------------------------------------------------

fn script_id(scope: &mut v8::PinScope, script: *mut JSScript) -> Option<i32> {
    // SAFETY: callers pass live script cells.
    unsafe { crate::cell::cell_script(scope, script as *mut c_void) }.map(|script| script.script_id())
}

pub unsafe fn SetScriptPrivate(script: *mut JSScript, value: *const JSVal) {
    let cx = JSContext::current();
    let Some(id) = cx.with_scope(|scope| script_id(scope, script)) else { return };
    // SAFETY: callers pass a valid value.
    set_script_private(cx, id, unsafe { *value });
}

pub unsafe fn JS_GetScriptPrivate(script: *mut JSScript, mut dest: MutableHandleValue) {
    let cx = JSContext::current();
    let id = cx.with_scope(|scope| script_id(scope, script));
    let private = id.and_then(|id| script_private(cx, id)).unwrap_or(UndefinedValue());
    dest.set(private);
}

pub unsafe fn SetScriptPrivateReferenceHooks(rt: *mut JSRuntime, add_ref: ScriptPrivateReferenceHook, release: ScriptPrivateReferenceHook) {
    // A runtime pointer is its raw context (see `rust::Runtime::rt`).
    raw(rt as *mut JSContext).scripts.reference_hooks.set((add_ref, release));
}

/// Sets the hook tracing script and module privates set from now on (see
/// `ScriptPrivateTraceHook`); privates already set keep the hook they were set with.
pub unsafe fn SetScriptPrivateTraceHook(rt: *mut JSRuntime, trace: ScriptPrivateTraceHook) {
    // A runtime pointer is its raw context (see `rust::Runtime::rt`).
    raw(rt as *mut JSContext).scripts.trace_hook.set(trace);
}

/// The innermost scripted frame (none when no script is running).
fn innermost_frame<'s>(scope: &mut v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::StackFrame>> {
    let trace = v8::StackTrace::current_stack_trace(scope, 1)?;
    if trace.get_frame_count() == 0 {
        return None;
    }
    trace.get_frame(scope, 0)
}

/// The innermost scripted frame: name, line and column.
fn caller_frame(scope: &mut v8::PinScope) -> Option<(String, u32, u32)> {
    let frame = innermost_frame(scope)?;
    let name = frame.get_script_name_or_source_url(scope).map(|name| name.to_rust_string_lossy(scope)).unwrap_or_default();
    Some((name, frame.get_line_number() as u32, frame.get_column() as u32))
}

/// The private of the classic script or source module whose code is running, as
/// SpiderMonkey's source-object private: V8 reports the host options of the innermost
/// frame's source, or of the source containing it for eval code (whose source object
/// SpiderMonkey gives the caller's private). `HideScriptedCaller` does not hide it.
pub(crate) fn caller_private(scope: &mut v8::PinScope) -> JSVal {
    let options = scope.get_current_host_defined_options();
    match options.and_then(|options| source_token(scope, options)) {
        Some(token) => token_private(scope, token),
        None => UndefinedValue(),
    }
}

pub unsafe fn JS_GetScriptedCallerPrivate(cx: *mut JSContext, mut dest: MutableHandleValue) {
    dest.set(raw(cx).with_scope(caller_private));
}

/// The global of the realm whose script is running (the entered realm).
pub unsafe fn GetScriptedCallerGlobal(cx: *mut JSContext) -> *mut JSObject {
    raw(cx).with_scope(|scope| {
        // An entered context alone does not imply a script is on the stack. Servo's
        // incumbent-settings algorithm must fall back to its explicit callback stack.
        if v8::StackTrace::current_stack_trace(scope, 1).is_none_or(|trace| trace.get_frame_count() == 0) {
            return std::ptr::null_mut();
        }
        let context = scope.get_entered_or_microtask_context();
        let realm = crate::realm_impl::realm_of_context(context);
        // SAFETY: realms come from `register_realm`.
        unsafe { crate::realm_impl::GetRealmGlobalOrNull(realm) }
    })
}

pub unsafe fn DescribeScriptedCaller(cx: *mut JSContext, buffer: *mut c_char, buflen: usize, line: *mut u32, col: *mut u32) -> bool {
    let Some((name, frame_line, frame_column)) = raw(cx).with_scope(caller_frame) else { return false };
    let bytes = name.as_bytes();
    let length = bytes.len().min(buflen.saturating_sub(1));
    // SAFETY: callers pass a buffer of `buflen` bytes and valid out pointers.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer as *mut u8, length);
        *buffer.add(length) = 0;
        *line = frame_line;
        *col = frame_column;
    }
    true
}

// --- Errors ----------------------------------------------------------------------------------

/// The message, file, line and column V8 reports for an exception value.
fn exception_info(scope: &mut v8::PinScope, exception: v8::Local<v8::Value>) -> (String, String, u32, u32) {
    let message = v8::Exception::create_message(scope, exception);
    let mut text = message.get(scope).to_rust_string_lossy(scope);
    if let Some(stripped) = text.strip_prefix("Uncaught ") {
        text = stripped.to_owned();
    }
    let file = message.get_script_resource_name(scope).and_then(|name| name.to_string(scope)).map(|name| name.to_rust_string_lossy(scope)).unwrap_or_default();
    let line = message.get_line_number(scope).unwrap_or(0) as u32;
    // V8 uses -1 for missing locations; rusty_v8 exposes that sentinel as usize.
    // Preserve the embedder's zero (unknown) instead of truncating and overflowing.
    let column = u32::try_from(message.get_start_column()).ok().and_then(|column| column.checked_add(1)).unwrap_or(0);
    (text, file, line, column)
}

/// Takes the pending exception into `dest` and reports its message and location.
pub unsafe fn PendingExceptionStackInfo(
    cx: *mut JSContext,
    callback: StringCallback,
    message_target: *mut c_void,
    filename_target: *mut c_void,
    line: *mut u32,
    col: *mut u32,
    mut dest: MutableHandleValue,
) -> bool {
    let raw = raw(cx);
    let Some(exception) = raw.pending_exception.borrow_mut().take() else { return false };
    let (value, (message, file, line_number, column)) = raw.with_scope(|scope| {
        let exception = v8::Local::new(scope, &exception);
        (from_v8(scope, exception), exception_info(scope, exception))
    });
    dest.set(value);
    if let Some(callback) = callback {
        // SAFETY: the callback copies the bytes into its target.
        unsafe {
            callback(message.as_ptr() as *const c_char, message.len(), message_target);
            callback(file.as_ptr() as *const c_char, file.len(), filename_target);
        }
    }
    // SAFETY: callers pass valid out pointers.
    unsafe {
        *line = line_number;
        *col = column;
    }
    true
}

/// The error report of an error object (null for other values). The report and its strings
/// stay valid until the next call, as SpiderMonkey's live as long as the error.
pub unsafe fn JS_ErrorFromException(cx: *mut JSContext, obj: HandleObject) -> *mut JSErrorReport {
    let raw = raw(cx);
    let info = raw.with_scope(|scope| {
        // SAFETY: callers pass a live object.
        let value = unsafe { crate::cell::cell_value(scope, obj.get() as *mut c_void) };
        value.is_native_error().then(|| exception_info(scope, value))
    });
    let Some((message, file, line, column)) = info else { return std::ptr::null_mut() };
    let message = CString::new(message.replace('\0', "")).unwrap_or_default();
    let file = CString::new(file.replace('\0', "")).unwrap_or_default();
    // SAFETY: SpiderMonkey's error report is plain data, valid when zeroed.
    let mut report: Box<JSErrorReport> = Box::new(unsafe { std::mem::zeroed() });
    report._base.message_.data_ = message.as_ptr();
    report._base.filename.data_ = file.as_ptr();
    report._base.lineno = line;
    report._base.column._base = column;
    let pointer = &mut *report as *mut JSErrorReport;
    *raw.scripts.last_report.borrow_mut() = Some((report, message, file));
    pointer
}

/// SpiderMonkey's error-number table is not available on V8.
pub unsafe fn RUST_js_GetErrorMessage(_user_ref: *mut c_void, _error_number: u32) -> *const JSErrorFormatString {
    std::ptr::null()
}

pub unsafe fn DumpJSStack(cx: *mut JSContext, _show_args: bool, _show_locals: bool, _show_this_props: bool) {
    let text = raw(cx).with_scope(|scope| stack_text(scope, usize::MAX));
    eprintln!("{text}");
}

// --- Saved stacks ----------------------------------------------------------------------------

pub unsafe fn JS_StackCapture_AllFrames(capture: *mut StackCapture) {
    // SAFETY: callers pass storage for a capture; word 0 holds the frame limit (0: all).
    unsafe { std::ptr::write_bytes(capture, 0, 1) };
}

pub unsafe fn JS_StackCapture_MaxFrames(max_frames: u32, capture: *mut StackCapture) {
    // SAFETY: as above.
    unsafe {
        std::ptr::write_bytes(capture, 0, 1);
        *(capture as *mut u64) = max_frames as u64;
    }
}

const FRAME_FIELDS: [&str; 5] = ["source", "line", "column", "functionDisplayName", "parent"];

fn frame_key<'s>(scope: &mut v8::PinScope<'s, '_>, field: &str) -> v8::Local<'s, v8::Private> {
    let name = v8::String::new(scope, &format!("roves-js SavedFrame.{field}")).expect("a short string");
    v8::Private::for_api(scope, Some(name))
}

fn frame_field<'s>(scope: &mut v8::PinScope<'s, '_>, frame: *mut JSObject, field: &str) -> Option<v8::Local<'s, v8::Value>> {
    // SAFETY: callers pass live frames.
    let object = unsafe { crate::cell::cell_value(scope, frame as *mut c_void) };
    let object = v8::Local::<v8::Object>::try_from(object).ok()?;
    let key = frame_key(scope, field);
    object.get_private(scope, key)
}

/// Captures the current stack as a chain of saved-frame objects.
pub unsafe fn CaptureCurrentStack(cx: *mut JSContext, mut stackp: MutableHandleObject, capture: *mut StackCapture, _start_after: HandleObject) -> bool {
    // SAFETY: callers pass an initialized capture.
    let limit = match unsafe { *(capture as *const u64) } {
        0 => usize::MAX,
        limit => limit as usize,
    };
    let stack = raw(cx).catching(|scope| {
        let trace = v8::StackTrace::current_stack_trace(scope, limit.min(1000))?;
        let mut parent: v8::Local<v8::Value> = v8::null(scope).into();
        for index in (0..trace.get_frame_count()).rev() {
            let frame = trace.get_frame(scope, index)?;
            let object = v8::Object::new(scope);
            let source: v8::Local<v8::Value> = match frame.get_script_name_or_source_url(scope) {
                Some(name) => name.into(),
                None => v8::String::empty(scope).into(),
            };
            let line = v8::Integer::new_from_unsigned(scope, frame.get_line_number() as u32);
            let column = v8::Integer::new_from_unsigned(scope, frame.get_column() as u32);
            let name: v8::Local<v8::Value> = match frame.get_function_name(scope) {
                Some(name) if name.length() > 0 => name.into(),
                _ => v8::null(scope).into(),
            };
            let values = [source, line.into(), column.into(), name, parent];
            for (field, value) in FRAME_FIELDS.iter().zip(values) {
                let key = frame_key(scope, field);
                object.set_private(scope, key, value)?;
            }
            parent = object.into();
        }
        Some(if parent.is_object() { from_v8(scope, parent).to_object() } else { std::ptr::null_mut() })
    });
    match stack {
        Some(stack) => {
            stackp.set(stack);
            true
        },
        None => false,
    }
}

pub unsafe fn GetSavedFrameParent(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    saved_frame: Handle<*mut JSObject>,
    mut parentp: MutableHandle<*mut JSObject>,
    _self_hosted: SavedFrameSelfHosted,
) -> SavedFrameResult {
    let parent = raw(cx).with_scope(|scope| {
        let parent = frame_field(scope, saved_frame.get(), "parent")?;
        parent.is_object().then(|| from_v8(scope, parent).to_object())
    });
    parentp.set(parent.unwrap_or(std::ptr::null_mut()));
    SavedFrameResult::Ok
}

pub unsafe fn GetSavedFrameSource(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    saved_frame: Handle<*mut JSObject>,
    mut sourcep: MutableHandle<*mut JSString>,
    _self_hosted: SavedFrameSelfHosted,
) -> SavedFrameResult {
    let source = raw(cx).with_scope(|scope| {
        let source = frame_field(scope, saved_frame.get(), "source")?;
        source.is_string().then(|| from_v8(scope, source).to_string())
    });
    sourcep.set(source.unwrap_or(std::ptr::null_mut()));
    SavedFrameResult::Ok
}

pub unsafe fn GetSavedFrameLine(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    saved_frame: Handle<*mut JSObject>,
    linep: *mut u32,
    _self_hosted: SavedFrameSelfHosted,
) -> SavedFrameResult {
    let line = raw(cx).with_scope(|scope| frame_field(scope, saved_frame.get(), "line").and_then(|line| line.uint32_value(scope)));
    // SAFETY: callers pass a valid out pointer.
    unsafe { *linep = line.unwrap_or(0) };
    SavedFrameResult::Ok
}

pub unsafe fn GetSavedFrameColumn(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    saved_frame: Handle<*mut JSObject>,
    columnp: *mut TaggedColumnNumberOneOrigin,
    _self_hosted: SavedFrameSelfHosted,
) -> SavedFrameResult {
    let column = raw(cx).with_scope(|scope| frame_field(scope, saved_frame.get(), "column").and_then(|column| column.uint32_value(scope)));
    // SAFETY: callers pass a valid out pointer.
    unsafe { (*columnp).value_ = column.unwrap_or(0) };
    SavedFrameResult::Ok
}

pub unsafe fn GetSavedFrameFunctionDisplayName(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    saved_frame: Handle<*mut JSObject>,
    mut namep: MutableHandle<*mut JSString>,
    _self_hosted: SavedFrameSelfHosted,
) -> SavedFrameResult {
    let name = raw(cx).with_scope(|scope| {
        let name = frame_field(scope, saved_frame.get(), "functionDisplayName")?;
        name.is_string().then(|| from_v8(scope, name).to_string())
    });
    namep.set(name.unwrap_or(std::ptr::null_mut()));
    SavedFrameResult::Ok
}

/// The current stack in SpiderMonkey's format (`name@source:line:column`, one frame a line).
fn stack_text(scope: &mut v8::PinScope, limit: usize) -> String {
    let Some(trace) = v8::StackTrace::current_stack_trace(scope, limit.min(1000)) else { return String::new() };
    let mut text = String::new();
    for index in 0..trace.get_frame_count() {
        let Some(frame) = trace.get_frame(scope, index) else { continue };
        let name = frame.get_function_name(scope).map(|name| name.to_rust_string_lossy(scope)).unwrap_or_default();
        let source = frame.get_script_name_or_source_url(scope).map(|name| name.to_rust_string_lossy(scope)).unwrap_or_default();
        text.push_str(&format!("{name}@{source}:{}:{}\n", frame.get_line_number(), frame.get_column()));
    }
    text
}

/// Formats a saved stack (SpiderMonkey's format; V8's when asked).
pub unsafe fn BuildStackString(
    cx: *mut JSContext,
    _principals: *mut JSPrincipals,
    stack: HandleObject,
    mut stringp: MutableHandle<*mut JSString>,
    indent: usize,
    format: StackFormat,
) -> bool {
    let mut frame = stack.get();
    let raw = raw(cx);
    let text = raw.with_scope(|scope| {
        let mut text = String::new();
        while !frame.is_null() {
            let source = frame_field(scope, frame, "source").map(|value| value.to_rust_string_lossy(scope)).unwrap_or_default();
            let line = frame_field(scope, frame, "line").and_then(|value| value.uint32_value(scope)).unwrap_or(0);
            let column = frame_field(scope, frame, "column").and_then(|value| value.uint32_value(scope)).unwrap_or(0);
            let name = frame_field(scope, frame, "functionDisplayName").filter(|value| value.is_string()).map(|value| value.to_rust_string_lossy(scope)).unwrap_or_default();
            text.push_str(&" ".repeat(indent));
            match format {
                StackFormat::V8 => text.push_str(&format!("    at {name} ({source}:{line}:{column})\n")),
                _ => text.push_str(&format!("{name}@{source}:{line}:{column}\n")),
            }
            frame = match frame_field(scope, frame, "parent") {
                Some(parent) if parent.is_object() => from_v8(scope, parent).to_object(),
                _ => std::ptr::null_mut(),
            };
        }
        text
    });
    let units: Vec<u16> = text.encode_utf16().collect();
    let string = crate::api::new_string_utf16(raw, &units);
    if string.is_null() {
        return false;
    }
    stringp.set(string);
    true
}
