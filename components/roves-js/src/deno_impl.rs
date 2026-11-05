/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! deno_core ownership of a roves-js runtime (single-document runtime, task 0385; see
//! ../../docs/V8_MIGRATION.md).
//!
//! Every [`crate::rust::Runtime`] owns one `deno_core::JsRuntime`: it creates the isolate and
//! the runtime's only script context (its "main realm"), owns the ES module graph (static and
//! dynamic imports, JSON modules, cycles, top-level await) and its event loop. roves-js keeps
//! the JSAPI surface (DOM objects, rooting, realms) on top of that isolate and context.
//!
//! - **No Deno capabilities.** No extension, op or JS namespace is given to scripts: the
//!   bootstrap globals (`Deno`, `__bootstrap`, `__infra`) and V8's flag-installed
//!   `queueMicrotask` are deleted before any script runs (the embedder defines its own). Module
//!   resolution and fetching go to the embedder's [`HostModuleLoader`] (Servo's import maps,
//!   fetch, CSP, SRI, credentials and referrer policy); nothing reads files or the network here.
//! - **Static graphs load stepwise.** [`start_module_graph`] creates a deno_core
//!   `SideModuleLoad` handle (Roves patch of the vendored deno_core, see
//!   `vendor/deno_core/ROVES_PATCHES.md`) and [`poll_module_graph`] advances it from embedder
//!   tasks: each poll registers at most one source that already arrived and returns `Pending`,
//!   waking itself once when a further step is ready, or while fetches are on the network. A
//!   poll that starts fetches may also wake once (futures' `FuturesUnordered` yield); then only
//!   those fetches wake the embedder's waker, so a pending network response never busy-loops.
//!   No borrow of the `JsRuntime` is held between polls, so script, timers, input and dynamic
//!   imports keep running while a graph is fetched (this replaces the provisional blocking
//!   decision of 2026-10-27). All graphs share the runtime's single module map. Polls log the
//!   time they held the thread (`[roves-module-pause]`). The handle does not pump the event
//!   loop: the adapter [`Loader`] never asks for it (`pump_event_loop_during_load` stays
//!   false), and the embedder polls the module event loop independently anyway.
//!   Shutdown (`ThreadSafeJSContext::request_shutdown`, from any thread) makes the next poll
//!   or start return [`ModuleGraphError::Cancelled`]; dropping the handle drops its pending
//!   fetches. [`load_module_graph`] is the blocking form (one call drives the same handle on
//!   the runtime's current-thread Tokio reactor; shutdown wakes that wait), kept for callers
//!   and tests that need it.
//! - **Evaluation** ([`evaluate_module`]) starts with `JsRuntime::mod_evaluate` at the caller's
//!   (HTML's) execution point and never waits for top-level await. A rejected evaluation
//!   reaches the embedder's promise rejection tracker through a promise internal to
//!   deno_core, so it is reported as `unhandledrejection` (with that internal promise), not as
//!   `error`/`window.onerror` (accepted by the user on 2026-10-28; fetch, parse, resolution
//!   and link failures keep their own contracts, see [`ModuleGraphError`]).
//! - **Dynamic imports** stay asynchronous: deno_core's event loop drives them, polled by
//!   [`poll_module_event_loop`] whenever the embedder's wake callback asks for it (a fetch
//!   completed, an evaluation settled, a new `import()` started).
//! - **Event loop lifetime.** The runtime holds one deno_core external op for its whole life:
//!   pending work (fetches, DOM events, timers awaited by top-level await) belongs to the host,
//!   so deno_core must not report a host-awaited top-level await as stalled. The op is released
//!   before the `JsRuntime` is dropped.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use deno_core::error::ModuleLoaderError;
use deno_core::{
    ImportAttributesContext, JsRuntime, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleResolveResponse, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, PollEventLoopOptions,
    RequestedModuleType, ResolutionKind, RuntimeOptions,
};

use crate::jsapi::JSContext;

/// deno_core's id of a loaded (instantiated) module.
pub type ModuleId = usize;

/// The module types the host fetches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostModuleKind {
    JavaScript,
    Json,
}

/// One fetched module: its response URL (the module's base URL), type and source text.
pub struct FetchedModule {
    pub url: String,
    pub kind: HostModuleKind,
    pub source: String,
}

/// A module fetch in progress. It runs on the runtime's thread (it may root DOM objects and
/// queue tasks, never run script); its result arrives from the host's network thread through
/// a [`oneshot`] channel. `Err` holds the message of the TypeError a failed fetch rejects with.
pub type HostModuleFetch = Pin<Box<dyn Future<Output = Result<FetchedModule, String>>>>;

/// The channel a module fetch receives its network result through.
pub use tokio::sync::oneshot;

/// Original location of an inline module. Only line metadata is kept; source is unchanged.
#[derive(Clone)]
pub struct ModuleSourcePosition {
    source_url: String,
    first_line: u32,
    lines: u32,
}
impl ModuleSourcePosition {
    /// `first_line` is one-based; zero leaves the generated position unchanged.
    pub fn new(source_url: String, first_line: u32, source: &str) -> Self {
        let mut lines = 1u32;
        let mut after_cr = false;
        for ch in source.chars() {
            if ch == '\n' && after_cr {
                after_cr = false;
                continue;
            }
            if matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
                lines = lines.saturating_add(1);
            }
            after_cr = ch == '\r';
        }
        Self { source_url, first_line, lines }
    }
    fn to_source_map(&self) -> Option<Vec<u8>> {
        let offset = self.first_line.checked_sub(1)?;
        let mut builder = deno_core::sourcemap::SourceMapBuilder::new(None);
        for line in 0..self.lines {
            let original = offset.checked_add(line)?;
            // Distinct valid anchors preserve columns through the maintained range codec.
            // A column-zero range alone loses later-line columns in sourcemap 9.3.2.
            builder.add(line, 0, original, 0, Some(&self.source_url), None, false);
            builder.add(line, 1, original, 1, Some(&self.source_url), None, true);
        }
        let mut bytes = Vec::new();
        builder.into_sourcemap().to_writer(&mut bytes).ok()?;
        Some(bytes)
    }
}

/// The embedder's module resolution and fetching (HTML's "resolve a module specifier" and
/// "fetch a single module script"). Called on the runtime's thread; callbacks must not run JS.
pub trait HostModuleLoader {
    /// Resolves `specifier` imported by the script or module named `referrer` (its URL, or the
    /// name an inline module was loaded under) to an absolute URL, or a TypeError message.
    fn resolve(&self, specifier: &str, referrer: &str) -> Result<String, String>;

    /// Optional original inline location. May run during stack formatting: metadata only,
    /// with no JS entry, network access or DOM rooting.
    fn source_position(&self, _specifier: &str) -> Option<ModuleSourcePosition> { None }

    /// Starts fetching the module at `url`, imported by `referrer` (none for a graph's root).
    fn fetch(&self, url: &str, referrer: Option<&str>, kind: HostModuleKind, dynamic_import: bool) -> HostModuleFetch;
}

/// Why a static module graph could not be loaded.
#[derive(Debug, PartialEq, Eq)]
pub enum ModuleGraphError {
    /// A module of the graph failed to fetch: HTML's null result (the script fires `error`).
    Fetch,
    /// Parsing, specifier resolution or linking failed: the pending exception holds the error
    /// to rethrow (a SyntaxError, or a TypeError for resolution).
    Rethrow,
    /// The runtime is already running deno_core work on this thread (a nested call from
    /// script); retry from a later task.
    Busy,
    /// The runtime is shutting down ([`crate::rust::ThreadSafeJSContext::request_shutdown`]):
    /// the load stopped waiting for the network and dropped its pending fetches, or never
    /// started. No exception is pending; embedders drop the load without reporting it to
    /// script (no `error` event at shutdown).
    Cancelled,
}

/// A failure seen by the loader during a static graph load (see [`take_static_failure`]).
enum StaticFailure {
    Fetch(String),
    Resolve(String),
}

#[derive(Default)]
struct LoaderState {
    host: RefCell<Option<Rc<dyn HostModuleLoader>>>,
    /// The first failure of the static load in progress.
    static_failure: RefCell<Option<StaticFailure>>,
    waker: RefCell<Option<Arc<HostWaker>>>,
}

impl LoaderState {
    fn note_static_failure(&self, failure: StaticFailure) {
        let mut slot = self.static_failure.borrow_mut();
        if slot.is_none() {
            *slot = Some(failure);
        }
    }

    fn request_poll(&self) {
        if let Some(waker) = self.waker.borrow().as_ref() {
            waker.wake_by_ref();
        }
    }

    fn host(&self) -> Result<Rc<dyn HostModuleLoader>, ModuleLoaderError> {
        self.host.borrow().clone().ok_or_else(|| ModuleLoaderError::type_error("ES modules are not available in this global".to_owned()))
    }
}

// These namespaces belong to Deno/Node package or extension infrastructure, never to
// web game resources. Check before module-map lookup as well as after host resolution:
// deleting globals alone does not make already-registered internal modules unreachable.
fn web_module_url(url: &str) -> Result<ModuleSpecifier, ModuleLoaderError> {
    let parsed = ModuleSpecifier::parse(url).map_err(|error| ModuleLoaderError::type_error(error.to_string()))?;
    if matches!(parsed.scheme(), "ext" | "node" | "npm" | "jsr" | "deno") {
        return Err(ModuleLoaderError::type_error("runtime-internal module namespaces are not available to games"));
    }
    Ok(parsed)
}

/// The deno_core `ModuleLoader` of a runtime: an adapter over the embedder's loader.
struct Loader(Rc<LoaderState>);

impl ModuleLoader for Loader {
    fn get_source_map(&self, file_name: &str) -> Option<std::borrow::Cow<'_, [u8]>> {
        self.0.host().ok()?.source_position(file_name)?.to_source_map().map(std::borrow::Cow::Owned)
    }

    fn resolve(&self, specifier: &str, referrer: &str, kind: ResolutionKind) -> ModuleResolveResponse {
        let result = (|| {
            if ModuleSpecifier::parse(specifier).is_ok() {
                web_module_url(specifier)?;
            }
            // Absolute graph roots are not remapped through HTML's import map.
            if referrer == "." {
                if let Ok(url) = ModuleSpecifier::parse(specifier) {
                    return web_module_url(url.as_str());
                }
            }
            let host = self.0.host()?;
            if kind == ResolutionKind::DynamicImport {
                self.0.request_poll();
            }
            let url = host.resolve(specifier, referrer).map_err(ModuleLoaderError::type_error)?;
            web_module_url(&url)
        })();
        if kind != ResolutionKind::DynamicImport {
            if let Err(error) = &result {
                self.0.note_static_failure(StaticFailure::Resolve(error.to_string()));
            }
        }
        result
    }

    fn import_meta_resolve(&self, specifier: &str, referrer: &str) -> Result<ModuleSpecifier, ModuleLoaderError> {
        let url = self.0.host()?.resolve(specifier, referrer).map_err(|message| ModuleLoaderError::type_error(message))?;
        web_module_url(&url)
    }

    fn load(&self, specifier: &ModuleSpecifier, referrer: Option<&ModuleLoadReferrer>, options: ModuleLoadOptions) -> ModuleLoadResponse {
        let kind = match options.requested_module_type {
            RequestedModuleType::None => HostModuleKind::JavaScript,
            RequestedModuleType::Json => HostModuleKind::Json,
            ref other => {
                return ModuleLoadResponse::Sync(Err(ModuleLoaderError::type_error(format!("unsupported module type {other:?}"))));
            },
        };
        let host = match self.0.host() {
            Ok(host) => host,
            Err(error) => return ModuleLoadResponse::Sync(Err(error)),
        };
        let dynamic = options.is_dynamic_import;
        let referrer = referrer.map(|referrer| referrer.specifier.to_string());
        let fetch = host.fetch(specifier.as_str(), referrer.as_deref(), kind, dynamic);
        let state = self.0.clone();
        let requested = specifier.clone();
        ModuleLoadResponse::Async(Box::pin(async move {
            match fetch.await {
                Ok(module) => {
                    let found = match web_module_url(&module.url) {
                        Ok(url) => url,
                        Err(error) => {
                            if !dynamic { state.note_static_failure(StaticFailure::Resolve(error.to_string())); }
                            return Err(error);
                        },
                    };
                    let module_type = match module.kind {
                        HostModuleKind::JavaScript => ModuleType::JavaScript,
                        HostModuleKind::Json => ModuleType::Json,
                    };
                    Ok(ModuleSource::new_with_redirect(module_type, ModuleSourceCode::String(module.source.into()), &requested, &found, None))
                },
                Err(message) => {
                    if !dynamic {
                        state.note_static_failure(StaticFailure::Fetch(message.clone()));
                    }
                    Err(ModuleLoaderError::type_error(message))
                },
            }
        }))
    }
}

/// HTML's import attributes: only `type`, and only the module types the host fetches.
fn validate_import_attributes(scope: &mut v8::PinScope, attributes: &HashMap<String, String>, _context: &ImportAttributesContext) {
    for (key, value) in attributes {
        let message = if key != "type" {
            format!("unsupported import attribute \"{key}\"")
        } else if value != "json" {
            format!("unsupported module type \"{value}\"")
        } else {
            continue;
        };
        let Some(message) = v8::String::new(scope, &message) else { return };
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    }
}

/// Wakes the runtime's thread through the embedder's callback, at most once per poll.
struct HostWaker {
    wake: Arc<dyn Fn() + Send + Sync>,
    scheduled: AtomicBool,
}

impl Wake for HostWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.scheduled.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }
}

/// The global object template a runtime's script context is created from (see `Runtime`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalKind {
    /// A Window: the template keeps V8's canonical, immutable Window prototype (see
    /// `object::ClassTemplates`).
    Document,
    /// Any other global (workers): deno_core's plain template.
    Plain,
}

thread_local! {
    /// The Window template made for the runtime being created on this thread, taken by the
    /// first Window global (`realm_impl::JS_NewGlobalObject`).
    static DOCUMENT_TEMPLATE: RefCell<Option<v8::Global<v8::ObjectTemplate>>> = const { RefCell::new(None) };
}

fn window_constructor(scope: &mut v8::PinScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    crate::native::throw_type_error(scope, "Illegal constructor");
}

/// deno_core's global template middleware for a document runtime: the Window template of
/// `object::ClassTemplates::template`, made before deno_core creates the context.
fn document_global_template<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    _plain: v8::Local<'s, v8::ObjectTemplate>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let constructor = v8::FunctionTemplate::new(scope, window_constructor);
    constructor.prototype_template(scope).set_internal_field_count(1);
    let template = constructor.instance_template(scope);
    template.set_immutable_proto();
    template.set_internal_field_count(1);
    let global = v8::Global::new(scope, template);
    DOCUMENT_TEMPLATE.with(|slot| *slot.borrow_mut() = Some(global));
    template
}

deno_core::extension!(roves_document_global, global_template_middleware = document_global_template);

/// The Window template of the runtime created last on this thread, once.
pub(crate) fn take_document_template() -> Option<v8::Global<v8::ObjectTemplate>> {
    DOCUMENT_TEMPLATE.with(|slot| slot.borrow_mut().take())
}

/// Names deno_core's bootstrap leaves on the main realm's global (`Deno`, `__bootstrap`,
/// `__infra`), plus V8's own `queueMicrotask` (deno_core's `--enable-queue-microtask` flag) and
/// `console` (silent without an inspector): the embedder defines the latter two.
const REMOVED_GLOBALS: [&str; 5] = ["Deno", "__bootstrap", "__infra", "queueMicrotask", "console"];

/// The deno_core side of a runtime. Owned by `rust::Runtime`, reached from the raw context.
pub(crate) struct DenoOwner {
    js: RefCell<ManuallyDrop<JsRuntime>>,
    tokio: ManuallyDrop<tokio::runtime::Runtime>,
    loader: Rc<LoaderState>,
    /// Evaluations started by [`evaluate_module`]; their futures are kept until they settle
    /// so deno_core always has a receiver for their results.
    evaluations: RefCell<Vec<Pin<Box<dyn Future<Output = Result<(), deno_core::error::CoreError>>>>>>,
    /// Modules [`evaluate_module`] already started (deno_core evaluates a module once).
    evaluated: RefCell<HashSet<ModuleId>>,
    /// Logged `poll_event_loop` errors, to bound repetitions.
    poll_errors: Cell<u32>,
    /// Set once, from any thread, when the runtime shuts down (see [`poll_module_graph`]).
    shutdown: Arc<tokio::sync::watch::Sender<bool>>,
}

impl DenoOwner {
    /// Creates the runtime's `JsRuntime` (its isolate and script context). V8 must be
    /// initialized (`rust::JSEngine`).
    pub(crate) fn new(kind: GlobalKind) -> Box<DenoOwner> {
        let tokio = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("a current-thread Tokio runtime");
        let loader = Rc::new(LoaderState::default());
        let js = {
            let _enter = tokio.enter();
            let mut extensions = Vec::new();
            if kind == GlobalKind::Document {
                extensions.push(roves_document_global::init());
            }
            JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(Loader(loader.clone()))),
                validate_import_attributes_cb: Some(Box::new(validate_import_attributes)),
                extensions,
                ..Default::default()
            })
        };
        // The host owns the pending async work (see the module documentation).
        js.op_state().borrow().external_ops_tracker.ref_op();
        Box::new(DenoOwner {
            js: RefCell::new(ManuallyDrop::new(js)),
            tokio: ManuallyDrop::new(tokio),
            loader,
            evaluations: Default::default(),
            evaluated: Default::default(),
            poll_errors: Cell::new(0),
            // No receiver is kept: loads subscribe while they wait, and the value persists.
            shutdown: Arc::new(tokio::sync::watch::channel(false).0),
        })
    }

    /// The shutdown flag shared with the runtime's `ThreadSafeJSContext`.
    pub(crate) fn shutdown_signal(&self) -> Arc<tokio::sync::watch::Sender<bool>> {
        self.shutdown.clone()
    }

    /// The `v8::Isolate` deno_core created (stable: it lives behind the `OwnedIsolate`).
    pub(crate) fn isolate(&self) -> *mut v8::Isolate {
        let mut js = self.js.borrow_mut();
        let isolate: &mut v8::Isolate = js.v8_isolate();
        isolate as *mut v8::Isolate
    }

    /// The runtime's only registered context.
    pub(crate) fn main_context(&self) -> v8::Global<v8::Context> {
        self.js.borrow().main_context()
    }

    /// Deletes [`REMOVED_GLOBALS`] from the main context's global, before any script runs.
    /// Panics if one cannot be removed: scripts must never see deno_core's namespace.
    pub(crate) fn remove_bootstrap_globals(&self, scope: &mut v8::PinScope) {
        let global = scope.get_current_context().global(scope);
        for name in REMOVED_GLOBALS {
            let key = v8::String::new(scope, name).expect("a short string");
            if global.has_own_property(scope, key.into()) == Some(true) {
                let deleted = global.delete(scope, key.into()) == Some(true) && global.has_own_property(scope, key.into()) == Some(false);
                assert!(deleted, "roves-js: cannot remove globalThis.{name} left by deno_core");
            }
        }
    }

    /// Drops the embedder's loader and wake callback (runtime shutdown).
    pub(crate) fn clear_host_loader(&self) {
        self.loader.host.borrow_mut().take();
        self.loader.waker.borrow_mut().take();
    }

    /// Runs `f` with exclusive use of the `JsRuntime`, inside its Tokio context; `None` if it
    /// is already in use on this thread (a nested call from script).
    fn with_js<R>(&self, f: impl FnOnce(&mut JsRuntime, &tokio::runtime::Runtime) -> R) -> Option<R> {
        let mut js = self.js.try_borrow_mut().ok()?;
        let _enter = self.tokio.enter();
        Some(f(&mut js, &self.tokio))
    }
}

impl Drop for DenoOwner {
    fn drop(&mut self) {
        self.loader.host.borrow_mut().take();
        self.loader.waker.borrow_mut().take();
        self.evaluations.get_mut().clear();
        let _enter = self.tokio.enter();
        let js = self.js.get_mut();
        js.op_state().borrow().external_ops_tracker.unref_op();
        // SAFETY: dropped once, here, inside the Tokio context; never used again.
        unsafe { ManuallyDrop::drop(js) };
        drop(_enter);
        // SAFETY: as above, after the runtime that used it.
        unsafe { ManuallyDrop::drop(&mut self.tokio) };
    }
}

fn owner<'o>(cx: &JSContext) -> &'o DenoOwner {
    let owner = cx.deno.get();
    assert!(!owner.is_null(), "a roves-js runtime owns its deno_core runtime");
    // SAFETY: set by `rust::Runtime` for its whole life; the runtime outlives its contexts.
    unsafe { &*owner }
}

/// Makes `loader` the runtime's module loader (resolution and fetching), with `wake` asked to
/// schedule [`poll_module_event_loop`] on the runtime's thread (it is called from any thread
/// and must only queue a task).
pub fn install_host_module_loader(cx: &crate::context::JSContext, loader: Rc<dyn HostModuleLoader>, wake: Arc<dyn Fn() + Send + Sync>) {
    let owner = owner(cx.raw_ref());
    *owner.loader.host.borrow_mut() = Some(loader);
    *owner.loader.waker.borrow_mut() = Some(Arc::new(HostWaker { wake, scheduled: AtomicBool::new(false) }));
}

/// Whether a host loader is installed.
pub fn has_host_module_loader(cx: &crate::context::JSContext) -> bool {
    owner(cx.raw_ref()).loader.host.borrow().is_some()
}

/// Drops the host loader (before the embedder's objects it refers to go away).
pub fn clear_host_module_loader(cx: &crate::context::JSContext) {
    owner(cx.raw_ref()).clear_host_loader();
}

/// Sets a new `kind` error with `message` as the pending exception, in the current realm.
fn set_pending_error(cx: &JSContext, syntax: bool, message: &str) {
    cx.with_scope(|scope| {
        let message = v8::String::new(scope, message).unwrap_or_else(|| v8::String::empty(scope));
        let error = if syntax { v8::Exception::syntax_error(scope, message) } else { v8::Exception::type_error(scope, message) };
        cx.set_pending(scope, error);
    });
}

/// A static module graph load in progress, advanced by [`poll_module_graph`] from embedder
/// tasks (see the module documentation). It holds no borrow of the `JsRuntime` or of a
/// context: between polls script, timers, input, dynamic imports and other loads run normally,
/// all on the runtime's single module map. Dropping it cancels the fetches still pending (the
/// modules it already registered stay in the map, so a later load reuses them); it may be
/// dropped at any time, also after the runtime.
pub struct ModuleGraphLoad {
    load: deno_core::SideModuleLoad,
    url: String,
    /// The first failure the loader noted while this load was polled (see
    /// [`take_static_failure`]).
    failure: Option<StaticFailure>,
    started: std::time::Instant,
    /// Time spent inside polls: the part of the load that holds the thread.
    polling: std::time::Duration,
    polls: u32,
}

impl ModuleGraphLoad {
    /// The name the graph's root is loaded under.
    pub fn url(&self) -> &str {
        &self.url
    }
}

/// Starts loading the static module graph rooted at `url`, without fetching anything yet:
/// [`poll_module_graph`] does the work. `inline_source`, an inline module's text loaded under
/// `url`, is taken only when the load starts (it is left in place on `Busy`).
///
/// Errors: `Rethrow` for an invalid URL (pending exception set), `Cancelled` once the runtime
/// is shutting down (nothing is fetched), `Busy` when called while the runtime runs deno_core
/// work on this thread (retry from a later task).
pub fn start_module_graph(
    cx: &mut crate::context::JSContext,
    url: &str,
    inline_source: &mut Option<String>,
) -> Result<ModuleGraphLoad, ModuleGraphError> {
    let raw = cx.raw_ref();
    let owner = owner(raw);
    let specifier = match web_module_url(url) {
        Ok(specifier) => specifier,
        Err(error) => {
            set_pending_error(raw, false, &format!("invalid module URL {url}: {error}"));
            return Err(ModuleGraphError::Rethrow);
        },
    };
    if *owner.shutdown.borrow() {
        log::debug!("[roves-modules] static module graph {url} not loaded: the runtime is shutting down");
        return Err(ModuleGraphError::Cancelled);
    }
    let js = owner.js.try_borrow().map_err(|_| ModuleGraphError::Busy)?;
    let code = inline_source.take().map(deno_core::ModuleCodeString::from);
    let load = js.start_side_es_module_load(&specifier, code);
    Ok(ModuleGraphLoad {
        load,
        url: url.to_owned(),
        failure: None,
        started: std::time::Instant::now(),
        polling: std::time::Duration::ZERO,
        polls: 0,
    })
}

/// Clears the loader's failure slot before a poll of `graph`.
///
/// A static load's fetch futures, root resolution, inline registration and import
/// resolution only run inside a poll of that load's own handle (deno_core's
/// `RecursiveModuleLoad` owns its fetches, and dynamic imports never note failures), so what
/// the slot holds after the poll belongs to `graph`, even with several graphs in flight.
fn take_static_failure(owner: &DenoOwner, graph: &mut ModuleGraphLoad) {
    if let Some(failure) = owner.loader.static_failure.borrow_mut().take() {
        graph.failure.get_or_insert(failure);
    }
}

/// Advances `graph`: registers at most one module whose source already arrived, starts the
/// fetches of its imports and, once the whole graph is registered, links it. Nothing is evaluated
/// (see [`evaluate_module`]) and the module event loop is not pumped (the adapter loader
/// never asks for it, see the module documentation).
///
/// `Pending` either means a fetch is pending and registered `waker`, or that one source was
/// registered and another immediately-ready step was deferred; in the latter case the waker
/// was scheduled once to yield to the embedder's task queue. A poll that starts fetches may
/// also wake `waker` once (futures' `FuturesUnordered` yields after polling every newly
/// started fetch); the poll that answers finds nothing ready and does not wake again, so a
/// real network wait is never a busy loop. `Ready` gives the same results as
/// [`load_module_graph`]; the handle is then finished and must be dropped. `Ready(Err(Busy))`
/// leaves the handle untouched: poll it again from a later task. `Ready(Err(Cancelled))` once
/// the runtime is shutting down: drop the handle (and with it the pending fetches) without
/// reporting anything to script.
///
/// Must not be called while script runs on this thread (embedders call it from a task).
pub fn poll_module_graph(
    cx: &mut crate::context::JSContext,
    graph: &mut ModuleGraphLoad,
    waker: &Waker,
) -> Poll<Result<ModuleId, ModuleGraphError>> {
    let raw = cx.raw_ref();
    let owner = owner(raw);
    if *owner.shutdown.borrow() {
        log::debug!("[roves-modules] static module graph {} dropped: the runtime is shutting down", graph.url);
        return Poll::Ready(Err(ModuleGraphError::Cancelled));
    }
    if graph.load.is_finished() {
        log::warn!("[roves-modules] static module graph {} polled after completion", graph.url);
        return Poll::Ready(Err(ModuleGraphError::Cancelled));
    }
    owner.loader.static_failure.borrow_mut().take();
    let mut context = Context::from_waker(waker);
    let polled = std::time::Instant::now();
    let Some(poll) = owner.with_js(|js, _| js.poll_side_es_module_load(&mut graph.load, &mut context)) else {
        return Poll::Ready(Err(ModuleGraphError::Busy));
    };
    graph.polling += polled.elapsed();
    graph.polls += 1;
    take_static_failure(owner, graph);
    let Poll::Ready(result) = poll else {
        return Poll::Pending;
    };
    // Measured for the review of the former blocking decision; no threshold is assumed.
    log::info!(
        "[roves-module-pause] static module graph {} ({}) held the thread {:.1} ms over {} polls, {:.1} ms after it started",
        graph.url,
        if result.is_ok() { "loaded" } else { "failed" },
        graph.polling.as_secs_f64() * 1000.0,
        graph.polls,
        graph.started.elapsed().as_secs_f64() * 1000.0
    );
    Poll::Ready(finish_graph_load(raw, &graph.url, result, graph.failure.take()))
}

/// Fetches and instantiates the static module graph rooted at `url` (with `inline_source`, an
/// inline module's text, loaded under that name), blocking this thread until every module of
/// the graph is fetched and linked. Nothing is evaluated (see [`evaluate_module`]). A shutdown
/// requested before or during the wait returns [`ModuleGraphError::Cancelled`] instead.
///
/// The blocking form of [`start_module_graph`]/[`poll_module_graph`], kept for callers that
/// need the whole graph within one call; Servo's script elements use the stepwise form.
///
/// Must not be called while script runs on this thread (embedders call it from a task).
pub fn load_module_graph(cx: &mut crate::context::JSContext, url: &str, inline_source: Option<String>) -> Result<ModuleId, ModuleGraphError> {
    let mut inline_source = inline_source;
    let mut graph = start_module_graph(cx, url, &mut inline_source)?;
    let raw = cx.raw_ref();
    let owner = owner(raw);
    owner.loader.static_failure.borrow_mut().take();
    // Subscribed after `start_module_graph` checked the flag: the watch value persists and
    // `wait_for` checks it first, so a shutdown requested in between is still seen.
    let mut shutdown = owner.shutdown.subscribe();
    let result = owner
        .with_js(|js, tokio| {
            tokio.block_on(async {
                let mut cancelled = std::pin::pin!(shutdown.wait_for(|closing| *closing));
                // Shutdown first, then the load; both register the reactor's waker, so a
                // signal from another thread wakes this wait without any polling. Returning
                // drops the load between steps (deno_core registers modules only
                // synchronously), and with it the host fetches still waiting on the network.
                // `Err` (sender gone) cannot happen while the owner lives; treat it as shutdown.
                std::future::poll_fn(|cx| {
                    if cancelled.as_mut().poll(cx).is_ready() {
                        return Poll::Ready(None);
                    }
                    js.poll_side_es_module_load(&mut graph.load, cx).map(Some)
                })
                .await
            })
        })
        .ok_or(ModuleGraphError::Busy)?;
    take_static_failure(owner, &mut graph);
    // The provisional static-load pause (see the module documentation): measured, not judged.
    log::info!(
        "[roves-module-pause] static module graph {url} ({}) paused the thread {:.1} ms",
        match &result {
            None => "cancelled by shutdown",
            Some(Ok(_)) => "loaded",
            Some(Err(_)) => "failed",
        },
        graph.started.elapsed().as_secs_f64() * 1000.0
    );
    let Some(result) = result else {
        // Failures noted by the dropped fetches belong to no one: no exception, no report.
        return Err(ModuleGraphError::Cancelled);
    };
    finish_graph_load(raw, url, result, graph.failure.take())
}

/// Maps the outcome of a static graph load to HTML's results: a fetch failure has no error to
/// rethrow; resolution, parse and link failures leave the error to rethrow pending.
fn finish_graph_load(
    raw: &JSContext,
    url: &str,
    result: Result<ModuleId, deno_core::error::CoreError>,
    failure: Option<StaticFailure>,
) -> Result<ModuleId, ModuleGraphError> {
    match result {
        Ok(id) => Ok(id),
        Err(error) => match failure {
            Some(StaticFailure::Fetch(message)) => {
                log::warn!("[roves-modules] fetching the module graph of {url} failed: {message}");
                Err(ModuleGraphError::Fetch)
            },
            Some(StaticFailure::Resolve(message)) => {
                set_pending_error(raw, false, &message);
                Err(ModuleGraphError::Rethrow)
            },
            None => {
                log::warn!("[roves-modules] loading the module graph of {url} failed: {error}");
                // Deno already classifies parse/link and import-attribute failures. Its
                // public conversion preserves that class instead of turning every error
                // into SyntaxError (in particular, invalid attributes are TypeError).
                raw.with_scope(|scope| {
                    let exception = deno_core::error::to_v8_error(scope, &error);
                    if let deno_core::error::CoreErrorKind::Js(original) = error.as_kind() {
                        crate::script_impl::retain_module_error_info(scope, exception, original);
                    }
                    raw.set_pending(scope, exception);
                });
                Err(ModuleGraphError::Rethrow)
            },
        },
    }
}

/// Starts evaluating the loaded module `id` (HTML's `record.Evaluate()`): runs its graph's
/// synchronous part now and returns without waiting for top-level await. A rejected
/// evaluation goes to the promise rejection tracker (see the module documentation).
pub fn evaluate_module(cx: &mut crate::context::JSContext, id: ModuleId) -> Result<(), ModuleGraphError> {
    let owner = owner(cx.raw_ref());
    if owner.evaluated.borrow().contains(&id) {
        return Ok(());
    }
    let evaluation = owner
        .with_js(|js, _| {
            // The public namespace API reports an already-errored dependency without
            // entering mod_evaluate's assertion. Do not catch arbitrary runtime panics.
            match js.get_module_namespace(id) {
                Ok(_) => Some(js.mod_evaluate(id)),
                Err(error) => {
                    log::warn!("[roves-modules] module {id} already failed evaluation: {error}");
                    None
                },
            }
        })
        .ok_or(ModuleGraphError::Busy)?;
    owner.evaluated.borrow_mut().insert(id);
    match evaluation {
        Some(evaluation) => owner.evaluations.borrow_mut().push(Box::pin(evaluation)),
        None => log::warn!("[roves-modules] module {id} cannot be evaluated again (errored)"),
    }
    // Its settlement wakes the event loop, which drops the finished future.
    owner.loader.request_poll();
    Ok(())
}

/// Polls deno_core's event loop once: advances dynamic imports and settles finished module
/// evaluations. The embedder calls it from a task when its wake callback asked for it, with
/// the runtime's realm entered, then performs its microtask checkpoint. Script may run.
pub fn poll_module_event_loop(cx: &mut crate::context::JSContext) {
    let owner = owner(cx.raw_ref());
    let Some(waker) = owner.loader.waker.borrow().clone() else { return };
    waker.scheduled.store(false, Ordering::Release);
    let std_waker = Waker::from(waker.clone());
    let mut context = Context::from_waker(&std_waker);
    let result = owner.with_js(|js, _| {
        owner.evaluations.borrow_mut().retain_mut(|evaluation| evaluation.as_mut().poll(&mut context).is_pending());
        js.poll_event_loop(&mut context, PollEventLoopOptions::default())
    });
    match result {
        // In use by an outer call: try again from a later task.
        None => waker.wake_by_ref(),
        Some(Poll::Ready(Err(error))) => {
            let count = owner.poll_errors.get().saturating_add(1);
            owner.poll_errors.set(count);
            if count.is_power_of_two() {
                log::warn!("[roves-modules] module event loop error (#{count}): {error}");
            }
        },
        Some(_) => {},
    }
}

/// The namespace object of the evaluated module `id`, in the main realm.
pub fn module_namespace(cx: &mut crate::context::JSContext, id: ModuleId) -> Option<*mut crate::jsapi::JSObject> {
    let raw = cx.raw_ref();
    let namespace = owner(raw).with_js(|js, _| js.get_module_namespace(id).ok())??;
    Some(raw.with_scope(|scope| {
        let namespace = v8::Local::new(scope, namespace);
        crate::jsval::from_v8(scope, namespace.into()).to_object()
    }))
}
