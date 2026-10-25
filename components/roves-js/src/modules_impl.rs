/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! ES modules: SpiderMonkey's module records and hooks on V8 modules.
//!
//! - A module record (`*mut JSObject` in the JSAPI) is a placeholder object whose cell holds
//!   the V8 `Module`. The runtime keeps an entry per module (its cell and kind), found from a
//!   V8 module by identity hash. Identity hashes can collide: they only select a bucket, whose
//!   member is matched by module (or by record).
//! - A source module's entry is weak: its source metadata keeps the record while the source's
//!   host token is reachable (from the record, the V8 module graph, the module's functions or
//!   namespace, or a pending import), so token -> metadata -> record -> token collects as a
//!   cycle once nothing else reaches it. Dead entries are swept as insertions amortize.
//! - A JSON module has no V8 source to anchor it, and the installed V8 API cannot attach
//!   host data to a synthetic module: its entry is weak, and its record lives by its own
//!   reachability plus, while V8 may still run its evaluation steps (which read its parsed
//!   value from the record), by the module's. Before linking, V8 reaches the module only
//!   through the record (the resolve hook hands out records). A record a running
//!   `ModuleLink` resolved is rooted until that link returns; one V8 left linked is then
//!   kept by the module's namespace, through the source metadata's WeakMap (an ephemeron):
//!   V8 caches the namespace on the module and resets it only while a link is in progress,
//!   so whatever can still evaluate the module keeps its record, and an abandoned graph
//!   (record -> module -> namespace -> record) collects as a cycle. Once its steps ran (or
//!   failed: V8 never runs them again) the value lives in V8's export cell and the
//!   namespace stops keeping the record.
//! - A source module's private is source metadata, as a classic script's (see
//!   `script_metadata`): kept by the script id recorded at compilation and by the source's
//!   host token. The placeholder pins the token too: the V8 API no longer reports an errored
//!   module's source, so the private's lifetime does not depend on what V8 keeps for it.
//! - A JSON module's private is held by the same cppgc private data, owned by a holder on the
//!   record's placeholder: it lives exactly as long as the record. The entry only indexes it
//!   (weakly) for shutdown.
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
    cell: EntryCell,
    kind: ModuleKind,
}

enum EntryCell {
    /// Source modules until their metadata keeps them; JSON modules while they are being
    /// compiled.
    Strong(*mut c_void),
    Weak(crate::cell::WeakCell),
}

impl ModuleEntry {
    /// The record, unless it was collected.
    fn cell(&self) -> Option<*mut c_void> {
        match &self.cell {
            EntryCell::Strong(cell) => Some(*cell),
            EntryCell::Weak(cell) => cell.get(),
        }
    }
}

const SWEEP_MIN: usize = 256;

#[cfg(test)]
thread_local! {
    /// Negative fixtures: keep source records strong, or skip their metadata's record edge.
    pub(crate) static STRONG_SOURCES_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static UNANCHORED_SOURCES_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Negative fixtures for JSON records: keep them strong, never let a linked one's namespace
    /// keep it, never root the ones a link resolves, or keep their privates natively (traced
    /// as roots).
    pub(crate) static STRONG_JSON_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static UNANCHORED_JSON_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static UNROOTED_LINKING_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static NATIVE_JSON_PRIVATES_FOR_TESTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

enum ModuleKind {
    /// Recorded at compilation: the V8 API no longer reports the id once a module errors.
    Source { script_id: i32 },
    /// A synthetic module has no V8 source. Its parsed value and private live on the record's
    /// placeholder (see `CompileJsonModule1`, `SetModulePrivate`); `data` only indexes the
    /// private for shutdown.
    Json { data: Option<v8::cppgc::WeakPersistent<crate::script_metadata::PrivateData>> },
}

/// The runtime's modules and module hooks.
#[derive(Default)]
pub(crate) struct Modules {
    entries: std::cell::RefCell<HashMap<i32, Vec<ModuleEntry>>>,
    /// JSON records resolved by the running `ModuleLink` calls, innermost last (see
    /// `LinkSegment`).
    linking: std::cell::RefCell<Vec<*mut c_void>>,
    resolve_hook: std::cell::Cell<ModuleResolveHook>,
    dynamic_import_hook: std::cell::Cell<ModuleDynamicImportHook>,
    metadata_hook: std::cell::Cell<ModuleMetadataHook>,
    inserted: std::cell::Cell<usize>,
    next_sweep: std::cell::Cell<usize>,
    #[cfg(test)]
    native_privates: std::cell::RefCell<Vec<crate::script_impl::HostPrivate>>,
    /// Test resolve hooks' JSON records by specifier, weak (dropped with the runtime, like
    /// the entries' weak cells).
    #[cfg(test)]
    json_names: std::cell::RefCell<HashMap<String, crate::cell::WeakCell>>,
}

/// Keeps source modules not yet anchored to their metadata (see `CompileModule1`), JSON
/// modules being compiled, and the JSON records running links resolved. Everything else a
/// record owns (parsed value, private) hangs off its placeholder.
pub(crate) fn trace_modules(cx: &JSContext, visitor: &mut v8::cppgc::Visitor) {
    // Neither list is borrowed across an allocation.
    if let Ok(entries) = cx.modules.entries.try_borrow() {
        for entry in entries.values().flatten() {
            if let EntryCell::Strong(cell) = &entry.cell {
                crate::cell::trace_cell(*cell, visitor);
            }
        }
    }
    if let Ok(linking) = cx.modules.linking.try_borrow() {
        for record in linking.iter() {
            crate::cell::trace_cell(*record, visitor);
        }
    }
    #[cfg(test)]
    {
        let privates: Vec<_> = cx.modules.native_privates.try_borrow().map(|privates| privates.clone()).unwrap_or_default();
        for private in privates {
            private.trace(visitor);
        }
    }
}

/// The V8 module of a module record.
fn module_of<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut JSObject) -> Option<v8::Local<'s, v8::Module>> {
    // SAFETY: callers pass live module records.
    unsafe { crate::cell::cell_module(scope, record as *mut c_void) }
}

/// The record (cell) of a newly compiled V8 module, registered (strongly) with its kind.
fn new_record(scope: &mut v8::PinScope, module: v8::Local<v8::Module>, kind: ModuleKind) -> *mut JSObject {
    let cell = crate::cell::new_module_cell(scope, module);
    let hash = module.get_identity_hash().get();
    let modules = &JSContext::current().modules;
    let mut entries = modules.entries.borrow_mut();
    entries.entry(hash).or_default().push(ModuleEntry { cell: EntryCell::Strong(cell), kind });
    // Drop the entries of collected records now and then, after a number of insertions
    // proportional to what survived the last sweep (as interned cells do).
    let inserted = modules.inserted.get() + 1;
    modules.inserted.set(inserted);
    if inserted >= modules.next_sweep.get().max(SWEEP_MIN) {
        // Keep a JSON private's pending finalization discoverable for shutdown.
        entries.retain(|_, bucket| {
            bucket.retain(|entry| entry.cell().is_some() || matches!(&entry.kind, ModuleKind::Json { data: Some(data) } if data.get().is_some()));
            !bucket.is_empty()
        });
        modules.inserted.set(0);
        modules.next_sweep.set(entries.len());
    }
    cell as *mut JSObject
}

/// Ties a new source module's record to its source: the placeholder pins the host token
/// whatever V8 keeps once the module errors, and the source metadata (owned through the
/// token) keeps the record.
fn anchor_record(scope: &mut v8::PinScope, record: *mut JSObject, script_id: i32, token: v8::Local<v8::Symbol>) -> Option<()> {
    // SAFETY: the record is a new module cell, kept by its strong entry.
    let placeholder = unsafe { crate::cell::cell_value(scope, record as *mut c_void) };
    let placeholder = v8::Local::<v8::Object>::try_from(placeholder).ok()?;
    let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::SourceToken);
    placeholder.set_private(scope, key, token.into())?;
    #[cfg(test)]
    if UNANCHORED_SOURCES_FOR_TESTING.with(std::cell::Cell::get) {
        return Some(());
    }
    crate::script_impl::anchor_source_module(scope, script_id, record as *mut c_void)
}

/// Stops the entry of `record` (alive: its strong entry keeps it) from keeping it: an
/// anchored source record, or a compiled JSON record.
fn weaken(module: v8::Local<v8::Module>, record: *mut c_void) {
    let hash = module.get_identity_hash().get();
    let mut entries = JSContext::current().modules.entries.borrow_mut();
    let entry = entries
        .get_mut(&hash)
        .and_then(|bucket| bucket.iter_mut().find(|entry| matches!(entry.cell, EntryCell::Strong(cell) if cell == record)));
    if let Some(entry) = entry {
        #[cfg(test)]
        if match entry.kind {
            ModuleKind::Source { .. } => STRONG_SOURCES_FOR_TESTING.with(std::cell::Cell::get),
            ModuleKind::Json { .. } => STRONG_JSON_FOR_TESTING.with(std::cell::Cell::get),
        } {
            return;
        }
        let weak = crate::cell::WeakCell::default();
        weak.set(record);
        entry.cell = EntryCell::Weak(weak);
    }
}

/// Makes the namespace of the linked JSON `module` keep its `record` (alive: rooted by the
/// caller or kept so far), or with `None` stop keeping it. A failure (only termination: the
/// WeakMap methods are captured intrinsics) leaves the record to its own reachability; the
/// steps then report the missing value instead of reading a collected record.
fn keep_json(scope: &mut v8::PinScope, module: v8::Local<v8::Module>, record: Option<*mut c_void>) {
    #[cfg(test)]
    if record.is_some() && UNANCHORED_JSON_FOR_TESTING.with(std::cell::Cell::get) {
        return;
    }
    v8::tc_scope!(let scope, scope);
    // Status at least linked (V8's API check): the namespace exists for good from here.
    let namespace = module.get_module_namespace();
    let _ = crate::script_impl::keep_module_record(scope, namespace, record);
}

/// Drops the strong entry of a new record that could not be completed: nothing else keeps it.
fn forget(module: v8::Local<v8::Module>, record: *mut c_void) {
    let hash = module.get_identity_hash().get();
    let mut entries = JSContext::current().modules.entries.borrow_mut();
    if let Some(bucket) = entries.get_mut(&hash) {
        bucket.retain(|entry| !matches!(entry.cell, EntryCell::Strong(cell) if cell == record));
        if bucket.is_empty() {
            entries.remove(&hash);
        }
    }
}

/// Test-only: (live source records, registered entries).
#[cfg(test)]
pub(crate) fn source_records_for_testing() -> (usize, usize) {
    let entries = JSContext::current().modules.entries.borrow();
    let live = entries.values().flatten().filter(|entry| matches!(entry.kind, ModuleKind::Source { .. }) && entry.cell().is_some()).count();
    (live, entries.values().map(Vec::len).sum())
}

/// Test-only: (live JSON records, those held for their steps: by a strong entry or by their
/// module's namespace).
#[cfg(test)]
pub(crate) fn json_records_for_testing() -> (usize, usize) {
    let records: Vec<_> = JSContext::current().modules.entries.borrow().values().flatten()
        .filter(|entry| matches!(entry.kind, ModuleKind::Json { .. }))
        .filter_map(|entry| entry.cell().map(|cell| (cell, matches!(entry.cell, EntryCell::Strong(_)))))
        .collect();
    // Rooted (as a link's records are) while their namespaces are read, which can allocate.
    let linking = &JSContext::current().modules.linking;
    let base = linking.borrow().len();
    linking.borrow_mut().extend(records.iter().map(|(record, _)| *record));
    let held = records.iter().filter(|(record, strong)| *strong || held_record_for_testing(*record as *mut JSObject)).count();
    linking.borrow_mut().truncate(base);
    (records.len(), held)
}

/// Test-only: whether the live, rooted JSON `record` is held for its steps (found by record,
/// in any bucket). Reading a linked module's namespace may create it: it keeps nothing itself.
#[cfg(test)]
pub(crate) fn held_record_for_testing(record: *mut JSObject) -> bool {
    let strong = JSContext::current().modules.entries.borrow().values().flatten()
        .find(|entry| entry.cell() == Some(record as *mut c_void))
        .map(|entry| matches!(entry.cell, EntryCell::Strong(_)))
        .expect("a live registered record");
    strong || JSContext::current().with_scope(|scope| {
        let module = module_of(scope, record).expect("a module record");
        if matches!(module.get_status(), v8::ModuleStatus::Uninstantiated | v8::ModuleStatus::Instantiating) {
            return false;
        }
        let namespace = module.get_module_namespace();
        crate::script_impl::kept_module_record(scope, namespace) == Some(record as *mut c_void)
    })
}

/// Test-only: registers the live JSON `record` (weakly) under `name`, or unregisters `name`.
#[cfg(test)]
pub(crate) fn register_json_for_testing(name: &str, record: Option<*mut JSObject>) {
    let mut names = JSContext::current().modules.json_names.borrow_mut();
    match record {
        Some(record) => {
            let weak = crate::cell::WeakCell::default();
            weak.set(record as *mut c_void);
            names.insert(name.to_owned(), weak);
        },
        None => {
            names.remove(name);
        },
    }
}

/// Test-only: `None` if `name` is no registered JSON record, else the record if it still
/// lives. `once` unregisters it.
#[cfg(test)]
pub(crate) fn json_record_for_testing(name: &str, once: bool) -> Option<Option<*mut JSObject>> {
    let mut names = JSContext::current().modules.json_names.borrow_mut();
    let record = if once { names.remove(name)?.get() } else { names.get(name)?.get() };
    Some(record.map(|record| record as *mut JSObject))
}

/// Test-only: JSON records rooted by running links.
#[cfg(test)]
pub(crate) fn linking_len_for_testing() -> usize {
    JSContext::current().modules.linking.borrow().len()
}

/// Test-only: the parsed value on a live JSON record's placeholder.
#[cfg(test)]
pub(crate) fn json_value_for_testing(record: *mut JSObject) -> JSVal {
    JSContext::current().with_scope(|scope| match json_value(scope, record as *mut c_void) {
        Some(value) => from_v8(scope, value),
        None => UndefinedValue(),
    })
}

/// Test-only: files `moved`'s entry first in `target`'s identity-hash bucket, as a native
/// hash collision would. Hash-based lookups of `moved` (`with_entry`) no longer find it.
/// Returns the size of the shared bucket.
#[cfg(test)]
pub(crate) fn collide_for_testing(scope: &mut v8::PinScope, target: *mut JSObject, moved: *mut JSObject) -> usize {
    let cx = JSContext::current();
    let target_hash = module_of(scope, target).expect("a module record").get_identity_hash().get();
    let moved_hash = module_of(scope, moved).expect("a module record").get_identity_hash().get();
    let mut entries = cx.modules.entries.borrow_mut();
    let bucket = entries.get_mut(&moved_hash).expect("a registered module");
    let index = bucket.iter().position(|entry| entry.cell() == Some(moved as *mut c_void)).expect("a registered module");
    let entry = bucket.remove(index);
    let bucket = entries.entry(target_hash).or_default();
    bucket.insert(0, entry);
    bucket.len()
}

fn with_entry<R>(scope: &mut v8::PinScope, module: v8::Local<v8::Module>, f: impl FnOnce(&mut ModuleEntry) -> R) -> Option<R> {
    let cx = JSContext::current();
    let hash = module.get_identity_hash().get();
    let mut entries = cx.modules.entries.borrow_mut();
    let entry = entries.get_mut(&hash)?.iter_mut().find(|entry| {
        // SAFETY: a strong entry's record is alive; a weak one's is checked.
        entry.cell().is_some_and(|cell| unsafe { crate::cell::cell_module(scope, cell) }.is_some_and(|known| known == module))
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
            // The host token keeps identical module sources from sharing one cached script.
            let (origin, token) = crate::script_impl::source_origin(scope, options, true);
            let mut source = v8::script_compiler::Source::new(code, Some(&origin));
            let module = v8::script_compiler::compile_module(scope, &mut source)?;
            let script_id = module.script_id()?;
            crate::script_impl::register_source(scope, script_id, token);
            // The strong entry keeps the record across the allocations of its anchoring.
            let record = new_record(scope, module, ModuleKind::Source { script_id });
            let anchored = anchor_record(scope, record, script_id, token);
            weaken(module, record as *mut c_void);
            anchored?;
            Some(record)
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
    // The record is copied out: no table borrow spans the V8 calls below.
    let record = with_entry(scope, module, |entry| match entry.kind {
        ModuleKind::Json { .. } => entry.cell(),
        ModuleKind::Source { .. } => None,
    })
    .flatten();
    // The record is alive: the namespace of the running module keeps it until `keep_json`.
    let json = record.and_then(|record| json_value(scope, record));
    // V8 runs the steps once: success evaluates the module, failure (exception or termination)
    // errors it for good (`SyntheticModule::Evaluate`, `Module::RecordError`). Either way its
    // value is no longer read from the record (the local holds it from here).
    if record.is_some() {
        keep_json(scope, module, None);
    }
    json_module_export(scope, module, json)
}

fn json_module_export<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    module: v8::Local<'s, v8::Module>,
    json: Option<v8::Local<'s, v8::Value>>,
) -> Option<v8::Local<'s, v8::Value>> {
    let Some(json) = json else {
        let message = v8::String::new(scope, "JSON module without its parsed value")?;
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return None;
    };
    let name = v8::String::new(scope, "default")?;
    module.set_synthetic_module_export(scope, name, json)?;
    let resolver = v8::PromiseResolver::new(scope)?;
    let undefined = v8::undefined(scope).into();
    resolver.resolve(scope, undefined)?;
    Some(resolver.get_promise(scope).into())
}

/// The placeholder of a live module record.
fn placeholder_of<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut c_void) -> Option<v8::Local<'s, v8::Object>> {
    // SAFETY: callers pass live records.
    v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, record) }).ok()
}

/// The parsed value of a live JSON module record: a private property of its placeholder, so
/// the record keeps it with no root of its own.
fn json_value<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut c_void) -> Option<v8::Local<'s, v8::Value>> {
    let placeholder = placeholder_of(scope, record)?;
    let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::JsonModuleValue);
    // `JSON.parse` never yields undefined: an absent property is a missing value.
    if !placeholder.has_private(scope, key)? {
        return None;
    }
    placeholder.get_private(scope, key)
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
            // The strong entry keeps the record while its placeholder takes the parsed value
            // (still held by this local); a record without it is never returned.
            let record = new_record(scope, module, ModuleKind::Json { data: None });
            let placeholder = placeholder_of(scope, record as *mut c_void);
            let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::JsonModuleValue);
            if placeholder.and_then(|placeholder| placeholder.set_private(scope, key, parsed)) != Some(true) {
                forget(module, record as *mut c_void);
                return None;
            }
            // Unlinked, only the record reaches the module: its own reachability decides.
            weaken(module, record as *mut c_void);
            Some(record)
        })
        .unwrap_or(std::ptr::null_mut())
}

pub unsafe fn IsCyclicModule(module: *mut JSObject) -> bool {
    JSContext::current().with_scope(|scope| module_of(scope, module).is_some_and(|module| module.is_source_text_module()))
}

// --- Privates and hooks ----------------------------------------------------------------------

/// A module's private: its source metadata's or, for a JSON module, its record's.
fn private_of(scope: &mut v8::PinScope, module: v8::Local<v8::Module>) -> JSVal {
    let private = with_entry(scope, module, |entry| match &entry.kind {
        ModuleKind::Source { script_id } => Err(*script_id),
        ModuleKind::Json { .. } => Ok(entry.cell()),
    });
    match private {
        Some(Ok(Some(record))) => json_private(scope, record),
        Some(Err(script_id)) => crate::script_impl::script_private(JSContext::current(), script_id).unwrap_or(UndefinedValue()),
        _ => UndefinedValue(),
    }
}

/// The private holder on a live JSON record's placeholder, if it has one.
fn json_private_holder<'s>(scope: &mut v8::PinScope<'s, '_>, record: *mut c_void) -> Option<v8::Local<'s, v8::Object>> {
    let placeholder = placeholder_of(scope, record)?;
    let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::JsonModulePrivate);
    if !placeholder.has_private(scope, key)? {
        return None;
    }
    v8::Local::<v8::Object>::try_from(placeholder.get_private(scope, key)?).ok()
}

/// A live JSON record's private.
fn json_private(scope: &mut v8::PinScope, record: *mut c_void) -> JSVal {
    #[cfg(test)]
    if NATIVE_JSON_PRIVATES_FOR_TESTING.with(std::cell::Cell::get) {
        return JSContext::current().modules.native_privates.borrow().last().map_or(UndefinedValue(), |private| private.value());
    }
    let Some(holder) = json_private_holder(scope, record) else { return UndefinedValue() };
    // SAFETY: the holder (a local) keeps its data alive.
    crate::script_metadata::holder_data(scope, holder).map_or(UndefinedValue(), |data| unsafe { &*data }.value())
}

/// Sets a live JSON record's private: the first one creates the holder on the record's
/// placeholder, indexed (weakly) by the record's entry for shutdown. `record` is rooted by
/// the caller (see `SetModulePrivate`): it is read again after the holder's allocation.
fn set_json_private(cx: &JSContext, scope: &mut v8::PinScope, module: v8::Local<v8::Module>, record: *mut c_void, value: JSVal) -> Option<()> {
    #[cfg(test)]
    if NATIVE_JSON_PRIVATES_FOR_TESTING.with(std::cell::Cell::get) {
        let private = crate::script_impl::HostPrivate::retain(cx, value);
        cx.modules.native_privates.borrow_mut().push(private);
        return Some(());
    }
    let holder = match json_private_holder(scope, record) {
        Some(holder) => holder,
        None if value.is_undefined() => return Some(()),
        None => {
            let (holder, data) = crate::script_impl::new_private_holder(scope)?;
            // Only an indexed private is created: shutdown must find it.
            with_entry(scope, module, |entry| match &mut entry.kind {
                ModuleKind::Json { data: slot } => Some(*slot = Some(data)),
                ModuleKind::Source { .. } => None,
            })??;
            let placeholder = placeholder_of(scope, record)?;
            let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::JsonModulePrivate);
            placeholder.set_private(scope, key, holder.into()).filter(|set| *set)?;
            holder
        },
    };
    let data = crate::script_metadata::holder_data(scope, holder)?;
    // The holder (a local, so a root) keeps its wrapped data alive on the non-moving cppgc
    // heap even if a reference hook did GC (SpiderMonkey's contract says they do not); the
    // new private is retained before the old one is unlinked and released.
    unsafe { &*data }.replace(crate::script_impl::HostPrivate::retain(cx, value));
    Some(())
}

/// Unlike SpiderMonkey's, this can GC (the first private allocates a holder): callers keep
/// `module` rooted across the call, as Servo's `rooted!` record does.
pub unsafe fn SetModulePrivate(module: *mut JSObject, value: *const JSVal) {
    // SAFETY: callers pass a valid value.
    let value = unsafe { *value };
    let record = module as *mut c_void;
    let cx = JSContext::current();
    let kind = cx.with_scope(|scope| {
        let module = module_of(scope, module)?;
        with_entry(scope, module, |entry| match &entry.kind {
            ModuleKind::Source { script_id } => Some(*script_id),
            ModuleKind::Json { .. } => None,
        })
    });
    // Hooks run outside the module table borrow, under the script-source ownership
    // contract shared with classic scripts, as SpiderMonkey does: the new private is
    // retained before the old one is unlinked and released.
    match kind {
        Some(Some(script_id)) => crate::script_impl::set_script_private(cx, script_id, value),
        Some(None) => {
            cx.catching(|scope| {
                let module = module_of(scope, module)?;
                set_json_private(cx, scope, module, record, value)
            });
        },
        None => {},
    }
}

/// Releases JSON module privates at shutdown (source modules' are source metadata). A
/// private whose record was collected was released by its data's finalizer; one released
/// here leaves nothing for that finalizer to release.
pub(crate) fn release_json_privates(cx: &JSContext) {
    // Hooks can drop embedder objects: invoke them without borrowing the module table.
    let data: Vec<_> = cx.modules.entries.borrow_mut().values_mut().flatten()
        .filter_map(|entry| match &mut entry.kind {
            ModuleKind::Json { data } => data.take(),
            ModuleKind::Source { .. } => None,
        })
        .collect();
    for data in data {
        if let Some(data) = data.get() {
            data.clear();
        }
    }
    #[cfg(test)]
    for private in std::mem::take(&mut *cx.modules.native_privates.borrow_mut()) {
        private.release();
    }
}

pub unsafe fn JS_GetModulePrivate(module: *mut JSObject, mut dest: MutableHandleValue) {
    let private = JSContext::current().with_scope(|scope| {
        let record = module as *mut c_void;
        let module = module_of(scope, module)?;
        Some(if module.is_synthetic_module() { json_private(scope, record) } else { private_of(scope, module) })
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
    let private = private_of(scope, referrer);
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
    let module = module_of(scope, resolved)?;
    // From here V8 holds the module, not the record: root a JSON record until the running
    // link returns (no allocation since the hook returned it).
    #[cfg(test)]
    let unrooted = UNROOTED_LINKING_FOR_TESTING.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let unrooted = false;
    if module.is_synthetic_module() && !unrooted {
        cx.modules.linking.borrow_mut().push(resolved as *mut c_void);
    }
    Some(module)
}

/// The JSON records one `ModuleLink` resolved: `linking[base..]` when it returns, on every
/// path. Nested links (from a resolve hook) close their own segment first; a record two
/// segments share is handled by each.
struct LinkSegment {
    base: usize,
    /// The linked record, rooted by the caller.
    root: *mut c_void,
}

impl LinkSegment {
    fn open(cx: &JSContext, root: *mut JSObject) -> LinkSegment {
        LinkSegment { base: cx.modules.linking.borrow().len(), root: root as *mut c_void }
    }
}

impl Drop for LinkSegment {
    /// Lets the namespaces of the JSON modules V8 left linked (even when the link failed: a
    /// dependency of a strongly connected component that completed stays linked) keep their
    /// records until their steps run. V8 reset the others to unlinked: only their records
    /// reach them again.
    fn drop(&mut self) {
        let cx = JSContext::current();
        // Copied out: the segment stays rooted, and unborrowed, during the V8 calls below.
        let records: Vec<_> = cx.modules.linking.borrow().get(self.base..).map(<[_]>::to_vec).unwrap_or_default();
        if !std::thread::panicking() {
            cx.with_scope(|scope| {
                for record in records.into_iter().chain(Some(self.root)) {
                    // SAFETY: the segment's records are rooted by `linking`, the root by its caller.
                    let Some(module) = (unsafe { crate::cell::cell_module(scope, record) }) else { continue };
                    if module.is_synthetic_module() && matches!(module.get_status(), v8::ModuleStatus::Instantiated) {
                        keep_json(scope, module, Some(record));
                    }
                }
            });
        }
        cx.modules.linking.borrow_mut().truncate(self.base);
    }
}

pub unsafe fn ModuleLink(cx: *mut JSContext, record: Handle<*mut JSObject>) -> bool {
    let record = record.get();
    let cx = raw(cx);
    let _segment = LinkSegment::open(cx, record);
    cx.catching(|scope| {
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

/// V8's dynamic `import()`: hands the request and a new promise to the embedder's hook,
/// which settles the promise later through `FinishDynamicModuleImport`.
fn dynamic_import<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_defined_options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let cx = JSContext::current();
    let raw_cx = cx as *const JSContext as *mut JSContext;
    let resolver = v8::PromiseResolver::new(scope)?;
    let promise = resolver.get_promise(scope);
    // V8 passes the referrer's host options: the importing source's, or for eval code the
    // source containing it (`Script::GetEvalOrigin`). Keep its metadata available to the
    // asynchronous host until the promise is resolved/rejected or abandoned. The token is
    // not exposed to JS.
    let token = crate::script_impl::source_token(scope, host_defined_options);
    if let Some(token) = token {
        let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::SourceToken);
        promise.set_private(scope, key, token.into())?;
    }
    let Some(hook) = cx.modules.dynamic_import_hook.get() else {
        let message = v8::String::new(scope, "dynamic import is not supported here")?;
        let error = v8::Exception::type_error(scope, message);
        resolver.reject(scope, error)?;
        return Some(promise);
    };
    // A token without metadata means no private, never another source's.
    let private = match token {
        Some(token) => crate::script_impl::token_private(scope, token),
        None => crate::script_impl::caller_private(scope),
    };
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
                let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::SourceToken);
                let _source_token = promise.get_private(scope, key)?;
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
        let key = crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::SourceToken);
        let _source_token = promise.get_private(scope, key)?;
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
    let private = private_of(scope, module);
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
