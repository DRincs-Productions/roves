/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! ES modules: SpiderMonkey's module records and hooks on V8 modules.
//!
//! - A module record (`*mut JSObject` in the JSAPI) is a placeholder object whose cell holds
//!   the V8 `Module`. The runtime keeps an entry per module (its cell, its private value and,
//!   for JSON modules, the parsed value), found from a V8 module by identity hash and from a
//!   running script by script id. Entries live as long as the runtime.
//! - A module request (`ModuleRequestObject`) is an object holding the specifier and the
//!   module type as V8 privates.
//! - Linking calls the embedder's resolve hook from V8's resolve callback; dynamic `import()`
//!   and `import.meta` go through V8's host callbacks to the embedder's hooks.

use std::collections::HashMap;
use std::ffi::c_void;

use crate::jsapi::{
    Handle, JSContext, JSObject, JSRuntime, JSString, ModuleDynamicImportHook, ModuleErrorBehaviour,
    ModuleMetadataHook, ModuleResolveHook, ModuleType, MutableHandleValue, ReadOnlyCompileOptions, SourceText,
    Utf8Unit,
};
use crate::jsval::{JSVal, UndefinedValue, from_v8};

fn raw<'a>(cx: *mut JSContext) -> &'a JSContext {
    // SAFETY: JSAPI callers pass this thread's live context.
    unsafe { &*cx }
}

struct ModuleEntry {
    cell: *mut c_void,
    private: JSVal,
    json: Option<v8::Global<v8::Value>>,
}

/// The runtime's modules and module hooks.
#[derive(Default)]
pub(crate) struct Modules {
    entries: std::cell::RefCell<HashMap<i32, Vec<ModuleEntry>>>,
    by_script_id: std::cell::RefCell<HashMap<i32, i32>>,
    resolve_hook: std::cell::Cell<ModuleResolveHook>,
    dynamic_import_hook: std::cell::Cell<ModuleDynamicImportHook>,
    metadata_hook: std::cell::Cell<ModuleMetadataHook>,
}

/// Keeps the modules (and their privates) alive.
pub(crate) fn trace_modules(cx: &JSContext, visitor: &mut v8::cppgc::Visitor) {
    use crate::gc::RootKind;
    let Ok(entries) = cx.modules.entries.try_borrow() else { return };
    for entry in entries.values().flatten() {
        crate::cell::trace_cell(entry.cell, visitor);
        entry.private.trace_root(visitor);
    }
}

/// The V8 module of a module record.
fn module_of<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut JSObject) -> Option<v8::Local<'s, v8::Module>> {
    // SAFETY: callers pass live module records.
    unsafe { crate::cell::cell_module(scope, record as *mut c_void) }
}

/// The record (cell) of a V8 module, creating and registering it on first sight.
fn record_of(scope: &mut v8::PinScope, module: v8::Local<v8::Module>, json: Option<v8::Local<v8::Value>>) -> *mut JSObject {
    let cx = JSContext::current();
    let hash = module.get_identity_hash().get();
    if let Some(entries) = cx.modules.entries.borrow().get(&hash) {
        for entry in entries {
            // SAFETY: registered cells are alive (traced by the runtime).
            if unsafe { crate::cell::cell_module(scope, entry.cell) }.is_some_and(|known| known == module) {
                return entry.cell as *mut JSObject;
            }
        }
    }
    let cell = crate::cell::new_module_cell(scope, module);
    let json = json.map(|json| v8::Global::new(scope, json));
    cx.modules.entries.borrow_mut().entry(hash).or_default().push(ModuleEntry { cell, private: UndefinedValue(), json });
    if let Some(script_id) = module.script_id() {
        cx.modules.by_script_id.borrow_mut().insert(script_id, hash);
    }
    cell as *mut JSObject
}

fn with_entry<R>(scope: &mut v8::PinScope, module: v8::Local<v8::Module>, f: impl FnOnce(&mut ModuleEntry) -> R) -> Option<R> {
    let cx = JSContext::current();
    let hash = module.get_identity_hash().get();
    let mut entries = cx.modules.entries.borrow_mut();
    let entry = entries.get_mut(&hash)?.iter_mut().find(|entry| {
        // SAFETY: as above.
        unsafe { crate::cell::cell_module(scope, entry.cell) }.is_some_and(|known| known == module)
    })?;
    Some(f(entry))
}

fn promise_of<'s>(scope: &mut v8::PinScope<'s, '_>, obj: *mut JSObject) -> Option<v8::Local<'s, v8::Promise>> {
    if obj.is_null() {
        return None;
    }
    // SAFETY: callers pass live objects.
    v8::Local::<v8::Promise>::try_from(unsafe { crate::cell::cell_value(scope, obj as *mut c_void) }).ok()
}

// --- Compilation ---------------------------------------------------------------------------

pub unsafe fn CompileModule1(cx: *mut JSContext, options: *const ReadOnlyCompileOptions, source: *mut SourceText<Utf8Unit>) -> *mut JSObject {
    // SAFETY: callers pass valid options and source.
    let options = unsafe { &*options };
    raw(cx)
        .catching(|scope| {
            let code = source_string(scope, source)?;
            let origin = crate::script_impl::script_origin(scope, options, true);
            let mut source = v8::script_compiler::Source::new(code, Some(&origin));
            let module = v8::script_compiler::compile_module(scope, &mut source)?;
            Some(record_of(scope, module, None))
        })
        .unwrap_or(std::ptr::null_mut())
}

fn source_string<'s>(scope: &mut v8::PinScope<'s, '_>, source: *mut SourceText<Utf8Unit>) -> Option<v8::Local<'s, v8::String>> {
    // SAFETY: callers pass a valid source text.
    let source = unsafe { &*source };
    let bytes = if source.length_ == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(source.units_ as *const u8, source.length_ as usize) } };
    v8::String::new_from_utf8(scope, bytes, v8::NewStringType::Normal)
}

fn json_module_steps<'s>(context: v8::Local<'s, v8::Context>, module: v8::Local<'s, v8::Module>) -> Option<v8::Local<'s, v8::Value>> {
    // SAFETY: V8 runs the steps with this context entered.
    v8::callback_scope!(unsafe scope, context);
    let json = with_entry(scope, module, |entry| entry.json.clone())??;
    let json = v8::Local::new(scope, &json);
    let name = v8::String::new(scope, "default")?;
    module.set_synthetic_module_export(scope, name, json)?;
    let resolver = v8::PromiseResolver::new(scope)?;
    let undefined = v8::undefined(scope).into();
    resolver.resolve(scope, undefined)?;
    Some(resolver.get_promise(scope).into())
}

/// A JSON module: a synthetic module whose `default` export is the parsed source.
pub unsafe fn CompileJsonModule1(cx: *mut JSContext, options: *const ReadOnlyCompileOptions, source: *mut SourceText<Utf8Unit>) -> *mut JSObject {
    // SAFETY: callers pass valid options.
    let options = unsafe { &*options };
    raw(cx)
        .catching(|scope| {
            let code = source_string(scope, source)?;
            let parsed = v8::json::parse(scope, code)?;
            let name = v8::String::new(scope, &crate::script_impl::options_filename(options))?;
            let export = v8::String::new(scope, "default")?;
            let module = v8::Module::create_synthetic_module(scope, name, &[export], json_module_steps);
            Some(record_of(scope, module, Some(parsed)))
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn IsCyclicModule(module: *mut JSObject) -> bool {
    JSContext::current().with_scope(|scope| module_of(scope, module).is_some_and(|module| module.is_source_text_module()))
}

// --- Privates and hooks ----------------------------------------------------------------------

pub unsafe fn SetModulePrivate(module: *mut JSObject, value: *const JSVal) {
    // SAFETY: callers pass a valid value.
    let value = unsafe { *value };
    JSContext::current().with_scope(|scope| {
        if let Some(module) = module_of(scope, module) {
            with_entry(scope, module, |entry| entry.private = value);
        }
    });
}

pub unsafe fn JS_GetModulePrivate(module: *mut JSObject, mut dest: MutableHandleValue) {
    let private = JSContext::current().with_scope(|scope| {
        let module = module_of(scope, module)?;
        with_entry(scope, module, |entry| entry.private)
    });
    dest.set(private.unwrap_or(UndefinedValue()));
}

fn modules<'m>(rt: *mut JSRuntime) -> &'m Modules {
    // A runtime pointer is its raw context (see `rust::Runtime::rt`).
    &raw(rt as *mut JSContext).modules
}

pub unsafe fn SetModuleResolveHook(rt: *mut JSRuntime, hook: ModuleResolveHook) {
    modules(rt).resolve_hook.set(hook);
}

pub unsafe fn GetModuleResolveHook(rt: *mut JSRuntime) -> ModuleResolveHook {
    modules(rt).resolve_hook.get()
}

pub unsafe fn SetModuleDynamicImportHook(rt: *mut JSRuntime, hook: ModuleDynamicImportHook) {
    modules(rt).dynamic_import_hook.set(hook);
}

pub unsafe fn SetModuleMetadataHook(rt: *mut JSRuntime, hook: ModuleMetadataHook) {
    modules(rt).metadata_hook.set(hook);
}

// --- Module requests -----------------------------------------------------------------------

fn request_key<'s>(scope: &mut v8::PinScope<'s, '_>, field: &str) -> v8::Local<'s, v8::Private> {
    let name = v8::String::new(scope, &format!("roves-js ModuleRequest.{field}")).expect("a short string");
    v8::Private::for_api(scope, Some(name))
}

/// The module type an import's attributes ask for (`with { type: "json" }`).
fn attributes_type(scope: &mut v8::PinScope, attributes: v8::Local<v8::FixedArray>, stride: usize) -> ModuleType {
    let mut index = 0;
    while index + 1 < attributes.length() {
        let key = attributes.get(scope, index).and_then(|key| v8::Local::<v8::Value>::try_from(key).ok());
        let value = attributes.get(scope, index + 1).and_then(|value| v8::Local::<v8::Value>::try_from(value).ok());
        if let (Some(key), Some(value)) = (key, value) {
            if key.to_rust_string_lossy(scope) == "type" {
                return if value.to_rust_string_lossy(scope) == "json" { ModuleType::JSON } else { ModuleType::Unknown };
            }
        }
        index += stride;
    }
    ModuleType::JavaScript
}

/// A module request object for `specifier` with `module_type`.
fn new_request<'s>(scope: &mut v8::PinScope<'s, '_>, specifier: v8::Local<'s, v8::String>, module_type: ModuleType) -> Option<v8::Local<'s, v8::Object>> {
    let request = v8::Object::new(scope);
    let key = request_key(scope, "specifier");
    request.set_private(scope, key, specifier.into())?;
    let key = request_key(scope, "type");
    let module_type = v8::Integer::new(scope, module_type as i32);
    request.set_private(scope, key, module_type.into())?;
    Some(request)
}

fn request_field<'s>(scope: &mut v8::PinScope<'s, '_>, request: *mut JSObject, field: &str) -> Option<v8::Local<'s, v8::Value>> {
    // SAFETY: callers pass live requests.
    let object = v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, request as *mut c_void) }).ok()?;
    let key = request_key(scope, field);
    object.get_private(scope, key)
}

fn module_type_of(code: i32) -> ModuleType {
    match code {
        1 => ModuleType::JavaScript,
        2 => ModuleType::JSON,
        _ => ModuleType::Unknown,
    }
}

pub unsafe fn GetModuleRequestSpecifier(cx: *mut JSContext, request: Handle<*mut JSObject>) -> *mut JSString {
    raw(cx).with_scope(|scope| {
        let specifier = request_field(scope, request.get(), "specifier").filter(|value| value.is_string());
        specifier.map_or(std::ptr::null_mut(), |specifier| from_v8(scope, specifier).to_string())
    })
}

pub unsafe fn GetModuleRequestType(cx: *mut JSContext, request: Handle<*mut JSObject>) -> ModuleType {
    raw(cx).with_scope(|scope| {
        let code = request_field(scope, request.get(), "type").and_then(|value| value.int32_value(scope));
        module_type_of(code.unwrap_or(0))
    })
}

fn requested<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut JSObject, index: u32) -> Option<v8::Local<'s, v8::ModuleRequest>> {
    let module = module_of(scope, record)?;
    let requests = module.get_module_requests();
    let request = requests.get(scope, index as usize)?;
    v8::Local::<v8::ModuleRequest>::try_from(request).ok()
}

pub unsafe fn GetRequestedModulesCount(cx: *mut JSContext, record: Handle<*mut JSObject>) -> u32 {
    raw(cx).with_scope(|scope| module_of(scope, record.get()).map_or(0, |module| module.get_module_requests().length() as u32))
}

pub unsafe fn GetRequestedModuleSpecifier(cx: *mut JSContext, record: Handle<*mut JSObject>, index: u32) -> *mut JSString {
    raw(cx).with_scope(|scope| match requested(scope, record.get(), index) {
        Some(request) => {
            let specifier = request.get_specifier();
            from_v8(scope, specifier.into()).to_string()
        },
        None => std::ptr::null_mut(),
    })
}

pub unsafe fn GetRequestedModuleType(cx: *mut JSContext, record: Handle<*mut JSObject>, index: u32) -> ModuleType {
    raw(cx).with_scope(|scope| match requested(scope, record.get(), index) {
        // Request attributes are (key, value, source offset) triples.
        Some(request) => {
            let attributes = request.get_import_attributes();
            attributes_type(scope, attributes, 3)
        },
        None => ModuleType::Unknown,
    })
}

// --- Linking and evaluation ------------------------------------------------------------------

/// V8's resolve callback: asks the embedder's resolve hook for the requested module.
fn resolve_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    // SAFETY: V8 calls this with the context entered.
    v8::callback_scope!(unsafe scope, context);
    let cx = JSContext::current();
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let Some(hook) = cx.modules.resolve_hook.get() else {
        crate::native::throw_type_error(scope, "no module resolve hook");
        return None;
    };
    let private = with_entry(scope, referrer, |entry| entry.private).unwrap_or(UndefinedValue());
    let module_type = attributes_type(scope, attributes, 3);
    let request = new_request(scope, specifier, module_type)?;
    let request = from_v8(scope, request.into()).to_object();
    crate::rooted!(in(raw_cx) let private = private);
    crate::rooted!(in(raw_cx) let request = request);
    // SAFETY: SpiderMonkey's resolve-hook contract (rooted arguments).
    let resolved = unsafe { hook(raw_cx, private.handle().into(), request.handle().into()) };
    if resolved.is_null() {
        crate::native::rethrow_pending(scope, cx);
        return None;
    }
    module_of(scope, resolved)
}

pub unsafe fn ModuleLink(cx: *mut JSContext, record: Handle<*mut JSObject>) -> bool {
    let record = record.get();
    raw(cx)
        .catching(|scope| {
            let module = module_of(scope, record)?;
            module.instantiate_module(scope, resolve_callback)
        })
        .unwrap_or(false)
}

/// Evaluates a linked module; `rval` is its evaluation promise.
pub unsafe fn ModuleEvaluate(cx: *mut JSContext, record: Handle<*mut JSObject>, mut rval: MutableHandleValue) -> bool {
    let record = record.get();
    let result = raw(cx).catching(|scope| {
        let module = module_of(scope, record)?;
        let result = module.evaluate(scope)?;
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

/// Throws the reason of a rejected evaluation promise.
pub unsafe fn ThrowOnModuleEvaluationFailure(cx: *mut JSContext, promise: Handle<*mut JSObject>, _behaviour: ModuleErrorBehaviour) -> bool {
    let raw = raw(cx);
    let reason = raw.with_scope(|scope| {
        let promise = promise_of(scope, promise.get())?;
        (promise.state() == v8::PromiseState::Rejected).then(|| v8::Global::new(scope, promise.result(scope)))
    });
    match reason {
        Some(reason) => {
            *raw.pending_exception.borrow_mut() = Some(reason);
            false
        },
        None => true,
    }
}

pub unsafe fn GetModuleNamespace(cx: *mut JSContext, record: Handle<*mut JSObject>) -> *mut JSObject {
    raw(cx).with_scope(|scope| {
        module_of(scope, record.get()).map_or(std::ptr::null_mut(), |module| {
            let namespace = module.get_module_namespace();
            from_v8(scope, namespace).to_object()
        })
    })
}

// --- Dynamic import and import.meta ----------------------------------------------------------

/// The private of the script or module whose code is running.
fn caller_private(scope: &mut v8::PinScope) -> JSVal {
    let cx = JSContext::current();
    let Some(trace) = v8::StackTrace::current_stack_trace(scope, 1) else { return UndefinedValue() };
    let Some(frame) = trace.get_frame(scope, 0) else { return UndefinedValue() };
    let script_id = frame.get_script_id() as i32;
    let hash = cx.modules.by_script_id.borrow().get(&script_id).copied();
    if let Some(entry) = hash.and_then(|hash| cx.modules.entries.borrow().get(&hash).and_then(|entries| entries.first().map(|entry| entry.private))) {
        return entry;
    }
    crate::script_impl::script_private(cx, script_id).unwrap_or(UndefinedValue())
}

/// V8's dynamic `import()`: hands the request and a new promise to the embedder's hook,
/// which settles the promise later through `FinishDynamicModuleImport`.
fn dynamic_import<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let cx = JSContext::current();
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let resolver = v8::PromiseResolver::new(scope)?;
    let promise = resolver.get_promise(scope);
    let Some(hook) = cx.modules.dynamic_import_hook.get() else {
        let message = v8::String::new(scope, "dynamic import is not supported here")?;
        let error = v8::Exception::type_error(scope, message);
        resolver.reject(scope, error)?;
        return Some(promise);
    };
    let private = caller_private(scope);
    // Dynamic-import attributes are (key, value) pairs.
    let module_type = attributes_type(scope, attributes, 2);
    let request = new_request(scope, specifier, module_type)?;
    let request = from_v8(scope, request.into()).to_object();
    let promise_object = from_v8(scope, promise.into()).to_object();
    crate::rooted!(in(raw_cx) let private = private);
    crate::rooted!(in(raw_cx) let request = request);
    crate::rooted!(in(raw_cx) let promise_object = promise_object);
    // SAFETY: SpiderMonkey's dynamic-import hook contract.
    let ok = unsafe { hook(raw_cx, private.handle().into(), request.handle().into(), promise_object.handle().into()) };
    if !ok {
        crate::native::rethrow_pending(scope, cx);
        return None;
    }
    Some(promise)
}

/// Settles a dynamic import's promise with the module's namespace once `evaluation_promise`
/// fulfils (or with the pending exception when there is no evaluation promise).
pub unsafe fn FinishDynamicModuleImport(
    cx: *mut JSContext,
    evaluation_promise: Handle<*mut JSObject>,
    referencing_private: Handle<JSVal>,
    module_request: Handle<*mut JSObject>,
    promise: Handle<*mut JSObject>,
) -> bool {
    let raw = raw(cx);
    let (evaluation, promise_obj) = (evaluation_promise.get(), promise.get());
    if evaluation.is_null() {
        let exception = raw.pending_exception.borrow_mut().take();
        return raw
            .catching(|scope| {
                let promise = promise_of(scope, promise_obj)?;
                // SAFETY: a V8 promise is its own resolver.
                let resolver = unsafe { std::mem::transmute::<v8::Local<v8::Promise>, v8::Local<v8::PromiseResolver>>(promise) };
                let reason: v8::Local<v8::Value> = match &exception {
                    Some(exception) => v8::Local::new(scope, exception),
                    None => v8::undefined(scope).into(),
                };
                resolver.reject(scope, reason)
            })
            .unwrap_or(false);
    }
    let Some(hook) = raw.modules.resolve_hook.get() else { return false };
    // SAFETY: SpiderMonkey's resolve-hook contract.
    let record = unsafe { hook(cx, referencing_private, module_request) };
    if record.is_null() {
        return false;
    }
    crate::rooted!(in(cx) let record = record);
    let record = record.get();
    raw.catching(|scope| {
        let module = module_of(scope, record)?;
        let namespace = module.get_module_namespace();
        let evaluation = promise_of(scope, evaluation)?;
        let promise = promise_of(scope, promise_obj)?;
        // SAFETY: as above.
        let resolver = unsafe { std::mem::transmute::<v8::Local<v8::Promise>, v8::Local<v8::PromiseResolver>>(promise) };
        let source = "(function (evaluation, namespace) { return evaluation.then(() => namespace); })";
        let code = v8::String::new(scope, source)?;
        let then = v8::Local::<v8::Function>::try_from(v8::Script::compile(scope, code, None)?.run(scope)?).ok()?;
        let undefined = v8::undefined(scope).into();
        let chained = then.call(scope, undefined, &[evaluation.into(), namespace])?;
        resolver.resolve(scope, chained)
    })
    .unwrap_or(false)
}

/// V8's `import.meta` initialization: the embedder's metadata hook fills the object.
unsafe extern "C" fn import_meta(context: v8::Local<v8::Context>, module: v8::Local<v8::Module>, meta: v8::Local<v8::Object>) {
    // SAFETY: V8 calls this with the context entered.
    v8::callback_scope!(unsafe scope, context);
    let cx = JSContext::current();
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let Some(hook) = cx.modules.metadata_hook.get() else { return };
    let private = with_entry(scope, module, |entry| entry.private).unwrap_or(UndefinedValue());
    let meta = from_v8(scope, meta.into()).to_object();
    crate::rooted!(in(raw_cx) let private = private);
    crate::rooted!(in(raw_cx) let meta = meta);
    // SAFETY: SpiderMonkey's metadata-hook contract.
    if !unsafe { hook(raw_cx, private.handle().into(), meta.handle().into()) } {
        crate::native::rethrow_pending(scope, cx);
    }
}

/// Installs V8's module host callbacks on a new runtime's isolate.
pub(crate) fn configure_isolate(isolate: &mut v8::Isolate) {
    isolate.set_host_import_module_dynamically_callback(dynamic_import);
    isolate.set_host_initialize_import_meta_object_callback(import_meta);
}
