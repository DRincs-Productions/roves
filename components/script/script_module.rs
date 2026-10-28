/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The script module mod contains common traits and structs
//! related to `type=module` for script thread or worker threads.
//!
//! Roves (task 0385, single-document runtime): deno_core owns the ES module graph, its
//! evaluation and dynamic `import()` (see `js::modules`). This module supplies HTML's part of
//! it as a [`HostModuleLoader`]: resolving module specifiers (import maps) and fetching module
//! scripts with Servo's fetch (destination, mode, credentials, referrer policy, CSP, SRI,
//! MIME checks, resource timing). It also keeps the entry points script elements, workers
//! and `modulepreload` call. A static graph is fetched and instantiated from a task, which
//! blocks this thread until the graph is linked (provisional decision of 2026-10-27, logged as
//! `[roves-module-pause]`); evaluation starts at HTML's execution point.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CStr;
use std::fmt::Debug;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use encoding_rs::UTF_8;
use headers::{HeaderMapExt, ReferrerPolicy as ReferrerPolicyHeader};
use hyper_serde::Serde;
use indexmap::IndexMap;
use js::context::JSContext;
use js::jsapi::{ExceptionStackBehavior, Handle as RawHandle, Heap, JSRuntime, JSTracer, SetScriptPrivateReferenceHooks, SetScriptPrivateTraceHook, Value};
use js::jsval::{JSVal, UndefinedValue};
use js::modules::{FetchedModule, HostModuleFetch, HostModuleKind, HostModuleLoader, ModuleGraphError, ModuleId};
use js::rust::wrappers2::{JS_ClearPendingException, JS_GetPendingException, JS_GetScriptedCallerPrivate, JS_SetPendingException};
use js::rust::Handle;
use mime::Mime;
use net_traits::blob_url_store::UrlWithBlobClaim;
use net_traits::http_status::HttpStatus;
use net_traits::mime_classifier::MimeClassifier;
use net_traits::policy_container::PolicyContainer;
use net_traits::request::{
    CredentialsMode, Destination, ParserMetadata, Referrer, RequestBuilder, RequestClient,
    RequestMode,
};
use net_traits::{
    BoxedFetchCallback, FetchMetadata, FetchResponseMsg, Metadata, NetworkError, ReferrerPolicy,
    ResourceFetchTiming, fetch_async,
};
use script_bindings::cell::DomRefCell;
use script_bindings::error::Fallible;
use script_bindings::settings_stack::run_a_script;
use script_bindings::trace::CustomTraceable;
use serde_json::{Map as JsonMap, Value as JsonValue};
use servo_url::ServoUrl;

use crate::DomTypeHolder;
use crate::dom::bindings::error::{
    Error, ErrorToJsval, report_pending_exception, throw_dom_exception,
};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::bindings::trace::{JSTraceable, RootedTraceableBox};
use crate::dom::csp::{GlobalCspReporting, Violation};
use crate::dom::globalscope::GlobalScope;
use crate::dom::html::htmlscriptelement::{SCRIPT_JS_MIMES, substitute_with_local_script};
use crate::dom::performance::performanceresourcetiming::InitiatorType;
use crate::dom::types::{
    Console, DedicatedWorkerGlobalScope, SharedWorkerGlobalScope, WorkerGlobalScope,
};
use crate::dom::window::Window;
use crate::network_listener::{self, ResourceTimingListener};
use crate::realms::enter_auto_realm;

pub(crate) fn gen_type_error(
    cx: &mut JSContext,
    global: &GlobalScope,
    error: Error,
) -> RethrowError {
    rooted!(&in(cx) let mut thrown = UndefinedValue());
    error.to_jsval(cx, global, thrown.handle_mut());

    RethrowError(RootedTraceableBox::from_box(Heap::boxed(thrown.get())))
}

#[derive(JSTraceable)]
pub(crate) struct RethrowError(RootedTraceableBox<Heap<JSVal>>);

impl RethrowError {
    pub(crate) fn new(val: Box<Heap<JSVal>>) -> Self {
        Self(RootedTraceableBox::from_box(val))
    }

    #[expect(unsafe_code)]
    pub(crate) fn from_pending_exception(cx: &mut JSContext) -> Self {
        rooted!(&in(cx) let mut exception = UndefinedValue());
        assert!(unsafe { JS_GetPendingException(cx, exception.handle_mut()) });
        unsafe { JS_ClearPendingException(cx) };

        Self::new(Heap::boxed(exception.get()))
    }

    pub(crate) fn handle(&self) -> Handle<'_, JSVal> {
        self.0.handle()
    }
}

impl Debug for RethrowError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        "RethrowError(...)".fmt(fmt)
    }
}

impl Clone for RethrowError {
    fn clone(&self) -> Self {
        Self(RootedTraceableBox::from_box(Heap::boxed(self.0.get())))
    }
}

/// A classic script's host-defined data, owned by the JS engine through the script private
/// (see `host_add_ref_top_level_script`).
///
/// # Safety
/// This can be shared inside an Rc because the engine traces it, through
/// `host_trace_top_level_script`, for as long as its script keeps the private: the owner
/// global is a traced edge, not a root.
#[derive(JSTraceable)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::allow_unrooted_in_rc)]
pub(crate) struct ModuleScript {
    #[no_trace]
    pub(crate) base_url: ServoUrl,
    pub(crate) options: ScriptFetchOptions,
    pub(crate) owner: Option<Dom<GlobalScope>>,
}

impl ModuleScript {
    pub(crate) fn new(
        base_url: ServoUrl,
        options: ScriptFetchOptions,
        owner: Option<Dom<GlobalScope>>,
    ) -> Self {
        ModuleScript {
            base_url,
            options,
            owner,
        }
    }
}

/// A module script: its URL and, once its graph is fetched and linked, deno_core's id of its
/// module record, or its error to rethrow (a parse, resolution or link error).
#[derive(JSTraceable, MallocSizeOf)]
pub(crate) struct ModuleTree {
    #[no_trace]
    url: ServoUrl,
    id: Option<ModuleId>,
    #[ignore_malloc_size_of = "mozjs"]
    rethrow_error: DomRefCell<Option<RethrowError>>,
}

impl ModuleTree {
    fn new(url: ServoUrl, id: Option<ModuleId>) -> Self {
        ModuleTree {
            url,
            id,
            rethrow_error: DomRefCell::new(None),
        }
    }

    pub(crate) fn get_url(&self) -> ServoUrl {
        self.url.clone()
    }

    pub(crate) fn get_rethrow_error(&self) -> &DomRefCell<Option<RethrowError>> {
        &self.rethrow_error
    }

    pub(crate) fn set_rethrow_error(&self, rethrow_error: RethrowError) {
        *self.rethrow_error.borrow_mut() = Some(rethrow_error);
    }
}

pub(crate) struct ModuleSource<'a> {
    pub source: Cow<'a, str>,
    pub unminified_dir: Option<String>,
    pub external: bool,
    pub url: ServoUrl,
}

impl<'a> crate::unminify::ScriptSource for ModuleSource<'a> {
    fn unminified_dir(&self) -> Option<String> {
        self.unminified_dir.clone()
    }

    fn extract_bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }

    fn rewrite_source(&mut self, source: String) {
        self.source = source.into();
    }

    fn url(&self) -> ServoUrl {
        self.url.clone()
    }

    fn is_external(&self) -> bool {
        self.external
    }
}

impl ModuleTree {
    /// Starts evaluating the module (HTML's `record.Evaluate()`): its graph's synchronous part
    /// runs now, top-level await continues on its own. A rejected evaluation is reported
    /// through the promise rejection tracker as `unhandledrejection` with deno_core's internal
    /// promise, not as `error`/`window.onerror` (accepted deviation, 2026-10-28).
    pub(crate) fn execute_module(
        &self,
        cx: &mut JSContext,
        global: &GlobalScope,
    ) -> Result<(), RethrowError> {
        let Some(id) = self.id else {
            return Ok(());
        };
        let mut realm = enter_auto_realm(cx, global);
        let cx = &mut *realm;
        if let Err(ModuleGraphError::Busy) = js::modules::evaluate_module(cx, id) {
            // Reached from script running inside deno_core's event loop: evaluate from the
            // next task instead.
            global
                .task_manager()
                .networking_task_source()
                .queue(task!(evaluate_module_later: move |cx| {
                    if js::modules::evaluate_module(cx, id).is_err() {
                        warn!("module {id} could not be evaluated");
                    }
                }));
        }
        Ok(())
    }

    #[expect(unsafe_code)]
    pub(crate) fn report_error(&self, cx: &mut JSContext, global: &GlobalScope) {
        let module_error = self.rethrow_error.borrow();

        if let Some(exception) = &*module_error {
            let mut realm = enter_auto_realm(cx, global);
            let cx = &mut realm.current_realm();

            unsafe {
                JS_SetPendingException(cx, exception.handle(), ExceptionStackBehavior::Capture);
            }
            report_pending_exception(cx);
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#resolve-a-module-specifier>
    pub(crate) fn resolve_module_specifier(
        global: &GlobalScope,
        script: Option<&ModuleScript>,
        specifier: DOMString,
    ) -> Fallible<ServoUrl> {
        // Step 1~3 to get settingsObject and baseURL
        let script_global = script.and_then(|s| s.owner.as_ref().map(Dom::as_rooted));
        // Step 1. Let settingsObject and baseURL be null.
        let (global, base_url): (&GlobalScope, &ServoUrl) = match script {
            // Step 2. If referringScript is not null, then:
            // Set settingsObject to referringScript's settings object.
            // Set baseURL to referringScript's base URL.
            Some(s) => (script_global.as_ref().map_or(global, |g| g), &s.base_url),
            // Step 3. Otherwise:
            // Set settingsObject to the current settings object.
            // Set baseURL to settingsObject's API base URL.
            // FIXME(#37553): Is this the correct current settings object?
            None => (global, &global.api_base_url()),
        };

        // Step 4. Let importMap be an empty import map.
        // Step 5. If settingsObject's global object implements Window, then set importMap to settingsObject's
        // global object's import map.
        let import_map = if global.is::<Window>() {
            Some(global.import_map())
        } else {
            None
        };
        let specifier = &specifier.str();

        // Step 6. Let serializedBaseURL be baseURL, serialized.
        let serialized_base_url = base_url.as_str();
        // Step 7. Let asURL be the result of resolving a URL-like module specifier given specifier and baseURL.
        let as_url = Self::resolve_url_like_module_specifier(specifier, base_url);
        // Step 8. Let normalizedSpecifier be the serialization of asURL, if asURL is non-null;
        // otherwise, specifier.
        let normalized_specifier = match &as_url {
            Some(url) => url.as_str(),
            None => specifier,
        };

        // Step 9. Let result be a URL-or-null, initially null.
        let mut result = None;
        if let Some(map) = import_map {
            // Step 10. For each scopePrefix → scopeImports of importMap's scopes:
            for (prefix, imports) in &map.scopes {
                // Step 10.1 If scopePrefix is serializedBaseURL, or if scopePrefix ends with U+002F (/)
                // and scopePrefix is a code unit prefix of serializedBaseURL, then:
                let prefix = prefix.as_str();
                if prefix == serialized_base_url ||
                    (serialized_base_url.starts_with(prefix) && prefix.ends_with('\u{002f}'))
                {
                    // Step 10.1.1 Let scopeImportsMatch be the result of resolving an imports match
                    // given normalizedSpecifier, asURL, and scopeImports.
                    let scope_imports_match =
                        resolve_imports_match(normalized_specifier, as_url.as_ref(), imports)?;

                    // Step 10.1.2 If scopeImportsMatch is not null, then set result to scopeImportsMatch, and break.
                    if scope_imports_match.is_some() {
                        result = scope_imports_match;
                        break;
                    }
                }
            }

            // Step 11. If result is null, set result to the result of resolving an imports match given
            // normalizedSpecifier, asURL, and importMap's imports.
            if result.is_none() {
                result =
                    resolve_imports_match(normalized_specifier, as_url.as_ref(), &map.imports)?;
            }
        }

        // Step 12. If result is null, set it to asURL.
        if result.is_none() {
            result = as_url.clone();
        }

        // Step 13. If result is not null, then:
        match result {
            Some(result) => {
                // Step 13.1 Add module to resolved module set given settingsObject, serializedBaseURL,
                // normalizedSpecifier, and asURL.
                global.add_module_to_resolved_module_set(
                    serialized_base_url,
                    normalized_specifier,
                    as_url.clone(),
                );
                // Step 13.2 Return result.
                Ok(result)
            },
            // Step 14. Throw a TypeError indicating that specifier was a bare specifier,
            // but was not remapped to anything by importMap.
            None => Err(Error::Type(
                c"Specifier was a bare specifier, but was not remapped to anything by importMap."
                    .to_owned(),
            )),
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#resolving-a-url-like-module-specifier>
    fn resolve_url_like_module_specifier(specifier: &str, base_url: &ServoUrl) -> Option<ServoUrl> {
        // Step 1. If specifier starts with "/", "./", or "../", then:
        if specifier.starts_with('/') || specifier.starts_with("./") || specifier.starts_with("../")
        {
            // Step 1.1. Let url be the result of URL parsing specifier with baseURL.
            return ServoUrl::parse_with_base(Some(base_url), specifier).ok();
        }
        // Step 2. Let url be the result of URL parsing specifier (with no base URL).
        ServoUrl::parse(specifier).ok()
    }
}

#[expect(unsafe_code)]
#[expect(non_snake_case)]
/// Registers the hooks that keep classic scripts' privates (`ModuleScript`) alive and traced.
/// Modules and dynamic imports belong to deno_core (see [`install_module_loader`]).
pub(crate) unsafe fn EnsureScriptPrivateHooksInitialized(rt: *mut JSRuntime) {
    unsafe {
        SetScriptPrivateReferenceHooks(
            rt,
            Some(host_add_ref_top_level_script),
            Some(host_release_top_level_script),
        );
        SetScriptPrivateTraceHook(rt, Some(host_trace_top_level_script));
    }
}

#[expect(unsafe_code)]
unsafe extern "C" fn host_add_ref_top_level_script(value: *const Value) {
    unsafe { Rc::increment_strong_count((*value).to_private() as *const ModuleScript) };
}

#[expect(unsafe_code)]
unsafe extern "C" fn host_release_top_level_script(value: *const Value) {
    unsafe { Rc::decrement_strong_count((*value).to_private() as *const ModuleScript) };
}

/// Traces a script private's edges (its owner global) during GC; the engine calls it only
/// while it holds a reference to the private.
#[expect(unsafe_code)]
unsafe extern "C" fn host_trace_top_level_script(trc: *mut JSTracer, value: *const Value) {
    unsafe { (*((*value).to_private() as *const ModuleScript)).trace(trc) };
}

#[derive(Clone, Debug, JSTraceable, MallocSizeOf)]
/// <https://html.spec.whatwg.org/multipage/#script-fetch-options>
pub(crate) struct ScriptFetchOptions {
    pub(crate) integrity_metadata: String,
    #[no_trace]
    pub(crate) credentials_mode: CredentialsMode,
    pub(crate) cryptographic_nonce: String,
    #[no_trace]
    pub(crate) parser_metadata: ParserMetadata,
    #[no_trace]
    pub(crate) referrer_policy: ReferrerPolicy,
    /// <https://html.spec.whatwg.org/multipage/#concept-script-fetch-options-render-blocking>
    /// The boolean value of render-blocking used for the initial fetch and for fetching any imported modules.
    /// Unless otherwise stated, its value is false.
    pub(crate) render_blocking: bool,
}

impl ScriptFetchOptions {
    /// <https://html.spec.whatwg.org/multipage/#default-classic-script-fetch-options>
    pub(crate) fn default_classic_script() -> ScriptFetchOptions {
        Self {
            cryptographic_nonce: String::new(),
            integrity_metadata: String::new(),
            parser_metadata: ParserMetadata::NotParserInserted,
            credentials_mode: CredentialsMode::CredentialsSameOrigin,
            referrer_policy: ReferrerPolicy::EmptyString,
            render_blocking: false,
        }
    }

    /// <https://html.spec.whatwg.org/multipage/#descendant-script-fetch-options>
    pub(crate) fn descendant_fetch_options(
        &self,
        url: &ServoUrl,
        global: &GlobalScope,
    ) -> ScriptFetchOptions {
        // Step 2. Let integrity be the result of resolving a module integrity metadata with url and settingsObject.
        let integrity = global.import_map().resolve_a_module_integrity_metadata(url);

        // Step 1. Let newOptions be a copy of originalOptions.
        // TODO Step 4. Set newOptions's fetch priority to "auto".
        Self {
            // Step 3. Set newOptions's integrity metadata to integrity.
            integrity_metadata: integrity,
            cryptographic_nonce: self.cryptographic_nonce.clone(),
            credentials_mode: self.credentials_mode,
            parser_metadata: self.parser_metadata,
            referrer_policy: self.referrer_policy,
            render_blocking: self.render_blocking,
        }
    }
}

#[expect(unsafe_code)]
pub(crate) unsafe fn module_script_from_reference_private(
    reference_private: &RawHandle<JSVal>,
) -> Option<&ModuleScript> {
    if reference_private.get().is_undefined() {
        return None;
    }
    unsafe { (reference_private.get().to_private() as *const ModuleScript).as_ref() }
}

// --- The host module loader -------------------------------------------------------------------

/// What one module fetch received on the network thread.
#[derive(Default)]
struct ModuleResponse {
    metadata: Option<Metadata>,
    status: Option<Result<(), NetworkError>>,
    data: Vec<u8>,
    violations: Vec<Violation>,
    timing: Option<ResourceFetchTiming>,
}

/// The fetch callback of one module fetch: it runs on the fetch thread and hands the complete
/// response to the module loader's future (which runs on this thread) at end of body. No task
/// of this thread is involved, so the response also arrives while a static graph load blocks
/// the thread (see the module documentation).
fn collect_module_response(sender: js::modules::oneshot::Sender<ModuleResponse>) -> BoxedFetchCallback {
    let mut pending = Some((sender, ModuleResponse::default()));
    Box::new(move |message| {
        let Some((_, response)) = pending.as_mut() else {
            return;
        };
        match message {
            FetchResponseMsg::ProcessResponse(_, metadata) => {
                response.metadata = metadata.ok().map(|metadata| match metadata {
                    FetchMetadata::Unfiltered(metadata) => metadata,
                    FetchMetadata::Filtered { unsafe_, .. } => unsafe_,
                });
                let status = response
                    .metadata
                    .as_ref()
                    .map(|metadata| metadata.status.clone())
                    .unwrap_or_else(HttpStatus::new_error);
                response.status = Some(if status.is_error() {
                    Err(NetworkError::ResourceLoadError(
                        "No http status code received".to_owned(),
                    ))
                } else if status.is_success() {
                    Ok(())
                } else {
                    Err(NetworkError::ResourceLoadError(format!(
                        "HTTP error code {}",
                        status.code()
                    )))
                });
            },
            FetchResponseMsg::ProcessResponseChunk(_, chunk) => {
                // SRI makes fetch consume/verify the body before ProcessResponse sends
                // metadata. Keep those early bytes; finish_module_fetch still requires
                // successful metadata/EOF before any source can reach deno_core.
                if !matches!(response.status, Some(Err(_))) {
                    response.data.extend_from_slice(&chunk.0);
                }
            },
            FetchResponseMsg::ProcessCspViolations(_, violations) => {
                response.violations.extend(violations)
            },
            FetchResponseMsg::ProcessResponseEOF(_, result, timing) => {
                let Some((sender, mut response)) = pending.take() else {
                    return;
                };
                if let Err(error) = result {
                    response.status = Some(Err(error));
                }
                response.timing = Some(timing);
                let _ = sender.send(response);
            },
            FetchResponseMsg::ProcessRequestBody(_) | FetchResponseMsg::ProcessContentLength(..) => {},
        }
    })
}

/// Resource timing of a module fetch.
struct ModuleTiming {
    url: ServoUrl,
    global: DomRoot<GlobalScope>,
}

impl ResourceTimingListener for ModuleTiming {
    fn resource_timing_information(&self) -> (InitiatorType, ServoUrl) {
        (InitiatorType::LocalName("module".to_string()), self.url.clone())
    }

    fn resource_timing_global(&self) -> DomRoot<GlobalScope> {
        self.global.clone()
    }
}

/// Reports a module fetch's CSP violations and resource timing from a later task: they may
/// run script (event listeners, performance observers), which must not happen while a module
/// load holds the JS runtime.
fn queue_fetch_reports(
    global: &GlobalScope,
    url: ServoUrl,
    violations: Vec<Violation>,
    timing: Option<ResourceFetchTiming>,
    status: Result<(), NetworkError>,
) {
    let trusted = Trusted::new(global);
    global
        .task_manager()
        .networking_task_source()
        .queue(task!(report_module_fetch: move |cx| {
            let global = trusted.root();
            if !violations.is_empty() {
                if let Some(scope) = global.downcast::<DedicatedWorkerGlobalScope>() {
                    scope.report_csp_violations(violations);
                } else if let Some(scope) = global.downcast::<SharedWorkerGlobalScope>() {
                    scope.report_csp_violations(violations);
                } else {
                    global.report_csp_violations(cx, violations, None, None);
                }
            }
            if let Some(timing) = timing {
                let listener = ModuleTiming { url, global: global.clone() };
                network_listener::submit_timing(cx, &listener, &status, &timing);
            }
        }));
}

/// Servo's half of deno_core's module loading for one global (one per runtime).
pub(crate) struct ServoModuleLoader {
    global: Trusted<GlobalScope>,
    /// The fetch destination of JavaScript modules: "script", or a module worker's.
    destination: Cell<Destination>,
    /// The fetch options of each module, by its URL: given by its script for a graph's root,
    /// else derived from its referrer's (HTML's descendant script fetch options).
    options: Rc<RefCell<HashMap<String, ScriptFetchOptions>>>,
    /// The base URL of each inline module script, by the name it was loaded under.
    inline_bases: RefCell<HashMap<String, ServoUrl>>,
    /// The root of a module worker's graph and its fetch client's policy container: fetched
    /// same-origin, its response initializes the worker's policies.
    worker_root: RefCell<Option<(String, RequestClient, Referrer)>>,
    next_inline: Cell<u64>,
}

thread_local! {
    static LOADER: RefCell<Option<Rc<ServoModuleLoader>>> = const { RefCell::new(None) };
}

fn loader() -> Option<Rc<ServoModuleLoader>> {
    LOADER.with(|loader| loader.borrow().clone())
}

/// A bare message of a resolution error (deno_core turns it into a TypeError).
fn error_message(error: Error) -> String {
    match error {
        Error::Type(message) => message.to_string_lossy().into_owned(),
        other => format!("{other:?}"),
    }
}

impl ServoModuleLoader {
    /// The script that imports, as HTML's referencing script: its base URL (an inline
    /// module's base URL, else the referrer's URL) and fetch options.
    fn referencing_script(&self, referrer: &str) -> Option<ModuleScript> {
        let base_url = self
            .inline_bases
            .borrow()
            .get(referrer)
            .cloned()
            .or_else(|| ServoUrl::parse(referrer).ok())?;
        let options = self
            .options
            .borrow()
            .get(referrer)
            .cloned()
            .unwrap_or_else(ScriptFetchOptions::default_classic_script);
        Some(ModuleScript::new(base_url, options, None))
    }
}

// Deno supplies a URL to its resolver. Classic scripts also carry HTML fetch options
// on their source private, which differ even for two inline scripts with the same URL.
// Read that existing source metadata while import() is on the stack, before async fetch.
#[expect(unsafe_code)]
fn active_classic_module_script() -> Option<ModuleScript> {
    let raw = js::rust::Runtime::get()?;
    let mut context = unsafe { JSContext::from_ptr(raw) };
    let cx = &mut context;
    rooted!(&in(cx) let mut value = UndefinedValue());
    unsafe { JS_GetScriptedCallerPrivate(cx, value.handle_mut()) };
    let reference = value.handle().into_handle();
    let script = unsafe { module_script_from_reference_private(&reference) }?;
    Some(ModuleScript::new(script.base_url.clone(), script.options.clone(), None))
}

impl HostModuleLoader for ServoModuleLoader {
    fn resolve(&self, specifier: &str, referrer: &str) -> Result<String, String> {
        let global = self.global.root();
        let classic = active_classic_module_script();
        let script = classic.or_else(|| self.referencing_script(referrer));
        let url = ModuleTree::resolve_module_specifier(
            &global,
            script.as_ref(),
            DOMString::from(specifier.to_owned()),
        ).map_err(error_message)?;
        if let Some(script) = script {
            self.options.borrow_mut().entry(url.as_str().to_owned())
                .or_insert_with(|| script.options.descendant_fetch_options(&url, &global));
        }
        Ok(url.into_string())
    }

    /// <https://html.spec.whatwg.org/multipage/#fetch-a-single-module-script>
    fn fetch(
        &self,
        url: &str,
        referrer: Option<&str>,
        kind: HostModuleKind,
        dynamic_import: bool,
    ) -> HostModuleFetch {
        let global = self.global.root();
        let Ok(request_url) = ServoUrl::parse(url) else {
            let message = format!("invalid module URL {url}");
            return Box::pin(async move { Err(message) });
        };
        let options = self.options.borrow().get(url).cloned().unwrap_or_else(|| {
            referrer.and_then(|referrer| self.options.borrow().get(referrer).cloned())
                .unwrap_or_else(ScriptFetchOptions::default_classic_script)
                .descendant_fetch_options(&request_url, &global)
        });
        let worker_root = self
            .worker_root
            .borrow()
            .as_ref()
            .filter(|(root, _, _)| referrer.is_none() && root == url)
            .map(|(_, client, referrer)| (client.clone(), referrer.clone()));

        // Step 8. Let request be a new request whose URL is url, mode is "cors", referrer is
        // referrer, and client is fetchClient.
        // Step 10. If destination is "worker", "sharedworker", or "serviceworker", and
        // isTopLevel is true, then set request's mode to "same-origin".
        let mode = if worker_root.is_some() {
            RequestMode::SameOrigin
        } else {
            RequestMode::CorsMode
        };
        // Step 9. Set request's destination to the result of running the fetch destination
        // from module type steps given destination and moduleType.
        let destination = match kind {
            HostModuleKind::Json => Destination::Json,
            HostModuleKind::JavaScript => if dynamic_import {
                Destination::Script
            } else {
                self.destination.get()
            },
        };
        // Step 12. Set up the module script request given request and options.
        // Descendants use the importing script's base URL, not the document URL.
        // Inline records have an internal identity fragment; referencing_script maps
        // them back to the HTML base. Fetch still applies the inherited referrer policy.
        let fetch_referrer = worker_root.as_ref().map(|(_, referrer)| referrer.clone())
            .or_else(|| referrer.and_then(|url| self.referencing_script(url))
                .map(|script| Referrer::ReferrerUrl(script.base_url)))
            .unwrap_or_else(|| global.get_referrer());
        let fetch_client = worker_root.as_ref().map(|(client, _)| client.clone())
            .unwrap_or_else(|| global.request_client(None));
        let request = RequestBuilder::new(
            global.webview_id(),
            crate::url::ensure_blob_referenced_by_url_is_kept_alive(&global, request_url.clone()),
            fetch_referrer,
        )
        .destination(destination)
        .parser_metadata(options.parser_metadata)
        .integrity_metadata(options.integrity_metadata.clone())
        .credentials_mode(options.credentials_mode)
        .referrer_policy(options.referrer_policy)
        .mode(mode)
        .cryptographic_nonce_metadata(options.cryptographic_nonce.clone())
        .client(fetch_client)
        .pipeline_id(Some(global.pipeline_id()));

        let (sender, receiver) = js::modules::oneshot::channel();
        fetch_async(
            &global.core_resource_thread(),
            request,
            None,
            collect_module_response(sender),
        );

        let trusted = self.global.clone();
        let options_map = self.options.clone();
        Box::pin(async move {
            let response = receiver
                .await
                .map_err(|_| format!("fetching module {request_url} was cancelled"))?;
            let global = trusted.root();
            finish_module_fetch(
                &global,
                request_url,
                kind,
                options,
                worker_root.map(|(client, _)| client.policy_container),
                response,
                &options_map,
            )
        })
    }
}

/// <https://html.spec.whatwg.org/multipage/#fetch-a-single-module-script>, step 13: checks a
/// module response and makes its source. Runs on this thread outside script.
fn finish_module_fetch(
    global: &GlobalScope,
    url: ServoUrl,
    kind: HostModuleKind,
    mut options: ScriptFetchOptions,
    worker_policy_container: Option<PolicyContainer>,
    response: ModuleResponse,
    options_map: &RefCell<HashMap<String, ScriptFetchOptions>>,
) -> Result<FetchedModule, String> {
    let status = response.status.unwrap_or_else(|| {
        Err(NetworkError::ResourceLoadError("No response received".to_owned()))
    });
    queue_fetch_reports(
        global,
        url.clone(),
        response.violations,
        response.timing,
        status.clone(),
    );

    // Step 1. If any of the following are true: bodyBytes is null or failure; or response's
    // status is not an ok status, then: run onComplete given null.
    if let Err(error) = status {
        return Err(format!("fetching module {url} failed: {error:?}"));
    }
    let Some(metadata) = response.metadata else {
        return Err(format!("fetching module {url} failed: no response"));
    };

    // The processResponseConsumeBody steps defined inside
    // [run a worker](https://html.spec.whatwg.org/multipage/#run-a-worker)
    if let Some(policy_container) = worker_policy_container {
        if let Some(scope) = global.downcast::<WorkerGlobalScope>() {
            scope.process_response_for_workerscope(&metadata, &policy_container);
        }
    }

    let final_url = metadata.final_url.clone();

    // Step 2. Let mimeType be the result of extracting a MIME type from response's header list.
    let mime_type: Option<Mime> = metadata.content_type.map(Serde::into_inner).map(Into::into);

    // Step 4. Let referrerPolicy be the result of parsing the `Referrer-Policy` header given response.
    let referrer_policy = metadata
        .headers
        .and_then(|headers| headers.typed_get::<ReferrerPolicyHeader>())
        .into();

    // Step 5. If referrerPolicy is not the empty string, set options's referrer policy to referrerPolicy.
    if referrer_policy != ReferrerPolicy::EmptyString {
        options.referrer_policy = referrer_policy;
    }

    let Some(mime) = mime_type else {
        return Err(format!("module {url} has no MIME type"));
    };
    // Step 7.2/7.4: a JavaScript MIME type for "javascript-or-wasm", a JSON MIME type for "json".
    let accepted = match kind {
        HostModuleKind::JavaScript => SCRIPT_JS_MIMES.contains(&mime.essence_str()),
        HostModuleKind::Json => MimeClassifier::is_json(&mime),
    };
    if !accepted {
        return Err(format!(
            "module {url} was served as {}, which is not allowed for this module type",
            mime.essence_str()
        ));
    }

    // Step 7.1 Let sourceText be the result of UTF-8 decoding bodyBytes.
    let (mut source, _) = UTF_8.decode_with_bom_removal(&response.data);
    if kind == HostModuleKind::JavaScript {
        if let Some(window) = global.downcast::<Window>() &&
            let Some(script_source) = window.local_script_source()
        {
            substitute_with_local_script(script_source, &mut source, final_url.clone());
        }
        if global.unminified_js_dir().is_some() {
            let mut module_source = ModuleSource {
                source,
                unminified_dir: global.unminified_js_dir(),
                external: true,
                url: final_url.clone(),
            };
            crate::unminify::unminify_js(&mut module_source);
            source = module_source.source;
        }
    }

    // Its descendants' fetch options derive from these (deno_core names the module by its
    // response URL).
    options_map
        .borrow_mut()
        .insert(final_url.as_str().to_owned(), options);
    Ok(FetchedModule {
        url: final_url.into_string(),
        kind,
        source: source.into_owned(),
    })
}

/// Makes Servo resolve and fetch the modules of `global`'s runtime (once per runtime), and
/// lets deno_core's module event loop wake this thread through a networking task.
pub(crate) fn install_module_loader(cx: &JSContext, global: &GlobalScope) {
    if js::modules::has_host_module_loader(cx) {
        return;
    }
    let destination = if global.is::<SharedWorkerGlobalScope>() {
        Destination::SharedWorker
    } else if global.is::<WorkerGlobalScope>() {
        Destination::Worker
    } else {
        Destination::Script
    };
    let loader = Rc::new(ServoModuleLoader {
        global: Trusted::new(global),
        destination: Cell::new(destination),
        options: Default::default(),
        inline_bases: Default::default(),
        worker_root: Default::default(),
        next_inline: Cell::new(0),
    });
    LOADER.with(|slot| *slot.borrow_mut() = Some(loader.clone()));
    let wake_target = Mutex::new((
        global.task_manager().networking_task_source().to_sendable(),
        Trusted::new(global),
    ));
    js::modules::install_host_module_loader(
        cx,
        loader,
        Arc::new(move || {
            let target = wake_target.lock().unwrap();
            let global = target.1.clone();
            target.0.queue(task!(poll_module_event_loop: move |cx| {
                let global = global.root();
                crate::script_module::poll_module_event_loop(cx, &global);
            }));
        }),
    );
}

/// Drops the module loader of this thread's runtime (before its DOM references go away).
pub(crate) fn clear_module_loader(cx: &JSContext) {
    // Cancel queued graph completions before the runtime/DOM references go away.
    // Their already-queued tasks become no-ops because load_pending_graph removes by key.
    PENDING_GRAPHS.with(|graphs| graphs.borrow_mut().clear());
    js::modules::clear_host_module_loader(cx);
    LOADER.with(|slot| slot.borrow_mut().take());
}

/// Runs deno_core's module event loop once (dynamic imports, top-level await completions) as
/// script of `global`, followed by HTML's cleanup after running script.
fn poll_module_event_loop(cx: &mut JSContext, global: &GlobalScope) {
    let mut realm = enter_auto_realm(cx, global);
    let cx = &mut realm.current_realm();
    run_a_script::<DomTypeHolder, _, _>(cx, global, |cx| {
        js::modules::poll_module_event_loop(cx);
    });
}

// --- Static module graphs -----------------------------------------------------------------------

type OnComplete = Box<dyn FnOnce(&mut JSContext, Option<Rc<ModuleTree>>)>;

/// A static graph waiting for its loading task.
struct PendingGraph {
    global: DomRoot<GlobalScope>,
    /// The script's URL, or an inline script's base URL.
    url: ServoUrl,
    /// The name deno_core loads the root under: its URL, or a unique name for inline source.
    name: String,
    inline_source: Option<String>,
    on_complete: OnComplete,
}

thread_local! {
    static PENDING_GRAPHS: RefCell<HashMap<u64, PendingGraph>> = RefCell::new(HashMap::new());
    static NEXT_GRAPH: Cell<u64> = const { Cell::new(0) };
}

/// Fetches and links the static graph of a module script from a networking task (never from
/// the script element's preparation, which may run inside script), then runs `on_complete`
/// with the module script (`None` when a fetch failed).
fn queue_graph_load(global: &GlobalScope, pending: PendingGraph) {
    let key = NEXT_GRAPH.with(|next| {
        let key = next.get();
        next.set(key + 1);
        key
    });
    PENDING_GRAPHS.with(|graphs| graphs.borrow_mut().insert(key, pending));
    global
        .task_manager()
        .networking_task_source()
        .queue(task!(load_module_graph: move |cx| {
            load_pending_graph(cx, key);
        }));
}

fn load_pending_graph(cx: &mut JSContext, key: u64) {
    let Some(pending) = PENDING_GRAPHS.with(|graphs| graphs.borrow_mut().remove(&key)) else {
        return;
    };
    let global = pending.global.clone();
    install_module_loader(cx, &global);
    let mut realm = enter_auto_realm(cx, &*global);
    let cx = &mut realm.current_realm();
    let result = js::modules::load_module_graph(cx, &pending.name, pending.inline_source.clone());
    let module = match result {
        Ok(id) => Some(Rc::new(ModuleTree::new(pending.url.clone(), Some(id)))),
        // HTML: a fetch failure completes with null.
        Err(ModuleGraphError::Fetch) => None,
        // HTML: a parse, resolution or link failure is the module script's error to rethrow.
        Err(ModuleGraphError::Rethrow) => {
            let module = ModuleTree::new(pending.url.clone(), None);
            module.set_rethrow_error(RethrowError::from_pending_exception(cx));
            Some(Rc::new(module))
        },
        Err(ModuleGraphError::Busy) => {
            // Script runs inside deno_core's event loop: try again from the next task.
            let url = pending.url.clone();
            queue_graph_load(&global, pending);
            return debug!("module graph of {url} deferred: the JS runtime is busy");
        },
    };
    (pending.on_complete)(cx, module);
}

/// Registers a graph root's fetch options and queues its load.
fn load_graph(
    global: &GlobalScope,
    url: ServoUrl,
    name: String,
    inline_source: Option<String>,
    options: ScriptFetchOptions,
    on_complete: impl FnOnce(&mut JSContext, Option<Rc<ModuleTree>>) + 'static,
) {
    // Callers install the loader first (`install_module_loader`).
    if let Some(loader) = loader() {
        loader.options.borrow_mut().insert(name.clone(), options);
    }
    queue_graph_load(
        global,
        PendingGraph {
            global: DomRoot::from_ref(global),
            url,
            name,
            inline_source,
            on_complete: Box::new(on_complete),
        },
    );
}

#[expect(clippy::too_many_arguments)]
/// <https://html.spec.whatwg.org/multipage/#fetch-a-module-worker-script-tree>
/// <https://html.spec.whatwg.org/multipage/#fetch-a-worklet/module-worker-script-graph>
pub(crate) fn fetch_a_module_worker_script_graph(
    cx: &mut JSContext,
    global: &GlobalScope,
    url: UrlWithBlobClaim,
    fetch_client: RequestClient,
    destination: Destination,
    referrer: Referrer,
    credentials_mode: CredentialsMode,
    on_complete: impl FnOnce(&mut JSContext, Option<Rc<ModuleTree>>) + Clone + 'static,
) {
    // Step 1. Let options be a script fetch options whose cryptographic nonce
    // is the empty string, integrity metadata is the empty string, parser
    // metadata is "not-parser-inserted", credentials mode is credentialsMode,
    // referrer policy is the empty string, and fetch priority is "auto".
    let options = ScriptFetchOptions {
        integrity_metadata: "".into(),
        credentials_mode,
        cryptographic_nonce: "".into(),
        parser_metadata: ParserMetadata::NotParserInserted,
        referrer_policy: ReferrerPolicy::EmptyString,
        render_blocking: false,
    };

    install_module_loader(cx, global);
    let url = url.url();
    if let Some(loader) = loader() {
        loader.destination.set(destination);
        *loader.worker_root.borrow_mut() =
            Some((url.as_str().to_owned(), fetch_client, referrer));
    }

    // Step 2. Fetch a single module script given url, fetchClient, destination, options,
    // settingsObject, "client", true, and onSingleFetchComplete; then fetch its descendants
    // and link it (deno_core loads the whole graph).
    let name = url.as_str().to_owned();
    load_graph(global, url, name, None, options, on_complete);
}

/// <https://html.spec.whatwg.org/multipage/#fetch-a-module-script-tree>
pub(crate) fn fetch_an_external_module_script(
    cx: &mut JSContext,
    url: UrlWithBlobClaim,
    global: &GlobalScope,
    options: ScriptFetchOptions,
    on_complete: impl FnOnce(&mut JSContext, Option<Rc<ModuleTree>>) + Clone + 'static,
) {
    install_module_loader(cx, global);
    let url = url.url();
    let name = url.as_str().to_owned();
    load_graph(global, url, name, None, options, on_complete);
}

/// <https://html.spec.whatwg.org/multipage/#fetch-a-modulepreload-module-script-graph>
///
/// Roves: the preload fetches and links the graph through deno_core (so its modules are in
/// the module map when a script imports them). JSON preload is temporarily unavailable:
/// report an error rather than claiming a successful load (user decision, 2026-10-28).
pub(crate) fn fetch_a_modulepreload_module(
    cx: &mut JSContext,
    url: UrlWithBlobClaim,
    destination: Destination,
    global: &GlobalScope,
    options: ScriptFetchOptions,
    on_complete: impl FnOnce(&mut JSContext, bool) + 'static,
) {
    if let Destination::Json = destination {
        static WARNING: std::sync::Once = std::sync::Once::new();
        WARNING.call_once(|| warn!("Roves: JSON modulepreload is temporarily unavailable; use import with type json"));
        return on_complete(cx, true);
    }
    install_module_loader(cx, global);
    let url = url.url();
    let name = url.as_str().to_owned();
    load_graph(global, url, name, None, options, move |cx, result| {
        // Run onComplete given whether the fetch failed.
        on_complete(cx, result.is_none());
    });
}

#[expect(clippy::too_many_arguments)]
/// <https://html.spec.whatwg.org/multipage/#fetch-an-inline-module-script-graph>
///
/// Roves: deno_core loads the inline source under a unique name derived from `url` (its
/// fragment replaced), which is also what `import.meta.url` reports; source positions start
/// at the module's first line.
pub(crate) fn fetch_inline_module_script(
    cx: &mut JSContext,
    global: &GlobalScope,
    module_script_text: Cow<'_, str>,
    url: ServoUrl,
    options: ScriptFetchOptions,
    _line_number: u32,
    _introduction_type: Option<&'static CStr>,
    on_complete: impl FnOnce(&mut JSContext, Option<Rc<ModuleTree>>) + Clone + 'static,
) {
    install_module_loader(cx, global);
    let Some(loader) = loader() else {
        return on_complete(cx, None);
    };
    let index = loader.next_inline.get();
    loader.next_inline.set(index + 1);
    let base = url.as_str().split('#').next().unwrap_or_default();
    let name = format!("{base}#roves-inline-module-{index}");
    loader
        .inline_bases
        .borrow_mut()
        .insert(name.clone(), url.clone());
    let source = if global.unminified_js_dir().is_some() {
        let mut module_source = ModuleSource {
            source: module_script_text,
            unminified_dir: global.unminified_js_dir(),
            external: false,
            url: url.clone(),
        };
        crate::unminify::unminify_js(&mut module_source);
        module_source.source.into_owned()
    } else {
        module_script_text.into_owned()
    };
    load_graph(global, url, name, Some(source), options, on_complete);
}

pub(crate) type ModuleSpecifierMap = IndexMap<String, Option<ServoUrl>>;
pub(crate) type ModuleIntegrityMap = IndexMap<ServoUrl, String>;

/// <https://html.spec.whatwg.org/multipage/#specifier-resolution-record>
#[derive(Default, Eq, Hash, JSTraceable, MallocSizeOf, PartialEq)]
pub(crate) struct ResolvedModule {
    /// <https://html.spec.whatwg.org/multipage/#specifier-resolution-record-serialized-base-url>
    base_url: String,
    /// <https://html.spec.whatwg.org/multipage/#specifier-resolution-record-specifier>
    specifier: String,
    /// <https://html.spec.whatwg.org/multipage/#specifier-resolution-record-as-url>
    #[no_trace]
    specifier_url: Option<ServoUrl>,
}

impl ResolvedModule {
    pub(crate) fn new(
        base_url: String,
        specifier: String,
        specifier_url: Option<ServoUrl>,
    ) -> Self {
        Self {
            base_url,
            specifier,
            specifier_url,
        }
    }
}

/// <https://html.spec.whatwg.org/multipage/#import-map-processing-model>
#[derive(Default, JSTraceable, MallocSizeOf)]
pub(crate) struct ImportMap {
    #[no_trace]
    imports: ModuleSpecifierMap,
    #[no_trace]
    scopes: IndexMap<ServoUrl, ModuleSpecifierMap>,
    #[no_trace]
    integrity: ModuleIntegrityMap,
}

impl ImportMap {
    /// <https://html.spec.whatwg.org/multipage/#resolving-a-module-integrity-metadata>
    pub(crate) fn resolve_a_module_integrity_metadata(&self, url: &ServoUrl) -> String {
        // Step 1. Let map be settingsObject's global object's import map.

        // Step 2. If map's integrity[url] does not exist, then return the empty string.
        // Step 3. Return map's integrity[url].
        self.integrity.get(url).cloned().unwrap_or_default()
    }
}

/// <https://html.spec.whatwg.org/multipage/#register-an-import-map>
pub(crate) fn register_import_map(
    cx: &mut JSContext,
    global: &GlobalScope,
    result: Fallible<ImportMap>,
) {
    match result {
        Ok(new_import_map) => {
            // Step 2. Merge existing and new import maps, given global and result's import map.
            merge_existing_and_new_import_maps(cx, global, new_import_map);
        },
        Err(exception) => {
            let mut realm = enter_auto_realm(cx, global);
            let cx = &mut realm.current_realm();

            // Step 1. If result's error to rethrow is not null, then report
            // an exception given by result's error to rethrow for global and return.
            throw_dom_exception(cx, global, exception);
            report_pending_exception(cx);
        },
    }
}

/// <https://html.spec.whatwg.org/multipage/#merge-existing-and-new-import-maps>
fn merge_existing_and_new_import_maps(
    cx: &mut JSContext,
    global: &GlobalScope,
    new_import_map: ImportMap,
) {
    // Step 1. Let newImportMapScopes be a deep copy of newImportMap's scopes.
    let new_import_map_scopes = new_import_map.scopes;

    // Step 2. Let oldImportMap be global's import map.
    let mut old_import_map = global.import_map_mut();

    // Step 3. Let newImportMapImports be a deep copy of newImportMap's imports.
    let mut new_import_map_imports = new_import_map.imports;

    let resolved_module_set = global.resolved_module_set();
    // Step 4. For each scopePrefix → scopeImports of newImportMapScopes:
    for (scope_prefix, mut scope_imports) in new_import_map_scopes {
        // Step 4.1. For each record of global's resolved module set:
        for record in resolved_module_set.iter() {
            // If scopePrefix is record's serialized base URL, or if scopePrefix ends with
            // U+002F (/) and scopePrefix is a code unit prefix of record's serialized base URL, then:
            let prefix = scope_prefix.as_str();
            if prefix == record.base_url ||
                (record.base_url.starts_with(prefix) && prefix.ends_with('\u{002f}'))
            {
                // For each specifierKey → resolutionResult of scopeImports:
                scope_imports.retain(|key, val| {
                    // If specifierKey is record's specifier, or if all of the following conditions are true:
                    // specifierKey ends with U+002F (/);
                    // specifierKey is a code unit prefix of record's specifier;
                    // either record's specifier as a URL is null or is special,
                    if *key == record.specifier ||
                        (key.ends_with('\u{002f}') &&
                            record.specifier.starts_with(key) &&
                            (record.specifier_url.is_none() ||
                                record
                                    .specifier_url
                                    .as_ref()
                                    .is_some_and(|u| u.is_special_scheme())))
                    {
                        // The user agent may report a warning to the console indicating the ignored rule.
                        // They may choose to avoid reporting if the rule is identical to an existing one.
                        Console::internal_warn(
                            cx,
                            global,
                            format!("Ignored rule: {key} -> {val:?}."),
                        );
                        // Remove scopeImports[specifierKey].
                        false
                    } else {
                        true
                    }
                })
            }
        }

        // Step 4.2 If scopePrefix exists in oldImportMap's scopes
        if old_import_map.scopes.contains_key(&scope_prefix) {
            // set oldImportMap's scopes[scopePrefix] to the result of
            // merging module specifier maps, given scopeImports and oldImportMap's scopes[scopePrefix].
            let merged_module_specifier_map = merge_module_specifier_maps(
                cx,
                global,
                scope_imports,
                &old_import_map.scopes[&scope_prefix],
            );
            old_import_map
                .scopes
                .insert(scope_prefix, merged_module_specifier_map);
        } else {
            // Step 4.3 Otherwise, set oldImportMap's scopes[scopePrefix] to scopeImports.
            old_import_map.scopes.insert(scope_prefix, scope_imports);
        }
    }

    // Step 5. For each url → integrity of newImportMap's integrity:
    for (url, integrity) in &new_import_map.integrity {
        // Step 5.1 If url exists in oldImportMap's integrity, then:
        if old_import_map.integrity.contains_key(url) {
            // Step 5.1.1 The user agent may report a warning to the console indicating the ignored rule.
            // They may choose to avoid reporting if the rule is identical to an existing one.
            Console::internal_warn(cx, global, format!("Ignored rule: {url} -> {integrity}."));
            // Step 5.1.2 Continue.
            continue;
        }

        // Step 5.2 Set oldImportMap's integrity[url] to integrity.
        old_import_map
            .integrity
            .insert(url.clone(), integrity.clone());
    }

    // Step 6. For each record of global's resolved module set:
    for record in resolved_module_set.iter() {
        // For each specifier → url of newImportMapImports:
        new_import_map_imports.retain(|specifier, val| {
            // If specifier starts with record's specifier, then:
            //
            // Note: Spec is wrong, we need to check if record's specifier starts with specifier
            // See: https://github.com/whatwg/html/issues/11875
            if record.specifier.starts_with(specifier) {
                // The user agent may report a warning to the console indicating the ignored rule.
                // They may choose to avoid reporting if the rule is identical to an existing one.
                Console::internal_warn(
                    cx,
                    global,
                    format!("Ignored rule: {specifier} -> {val:?}."),
                );
                // Remove newImportMapImports[specifier].
                false
            } else {
                true
            }
        });
    }

    // Step 7. Set oldImportMap's imports to the result of merge module specifier maps,
    // given newImportMapImports and oldImportMap's imports.
    let merged_module_specifier_map =
        merge_module_specifier_maps(cx, global, new_import_map_imports, &old_import_map.imports);
    old_import_map.imports = merged_module_specifier_map;

    // https://html.spec.whatwg.org/multipage/#the-resolution-algorithm
    // Sort scopes to ensure entries are visited from most-specific to least-specific.
    old_import_map
        .scopes
        .sort_by(|a_key, _, b_key, _| b_key.cmp(a_key));
}

/// <https://html.spec.whatwg.org/multipage/#merge-module-specifier-maps>
fn merge_module_specifier_maps(
    cx: &mut JSContext,
    global: &GlobalScope,
    new_map: ModuleSpecifierMap,
    old_map: &ModuleSpecifierMap,
) -> ModuleSpecifierMap {
    // Step 1. Let mergedMap be a deep copy of oldMap.
    let mut merged_map = old_map.clone();

    // Step 2. For each specifier → url of newMap:
    for (specifier, url) in new_map {
        // Step 2.1 If specifier exists in oldMap, then:
        if old_map.contains_key(&specifier) {
            // Step 2.1.1 The user agent may report a warning to the console indicating the ignored rule.
            // They may choose to avoid reporting if the rule is identical to an existing one.
            Console::internal_warn(cx, global, format!("Ignored rule: {specifier} -> {url:?}."));

            // Step 2.1.2 Continue.
            continue;
        }

        // Step 2.2 Set mergedMap[specifier] to url.
        merged_map.insert(specifier, url);
    }

    merged_map
}

/// <https://html.spec.whatwg.org/multipage/#parse-an-import-map-string>
pub(crate) fn parse_an_import_map_string(
    cx: &mut JSContext,
    global: &GlobalScope,
    input: &str,
    base_url: ServoUrl,
) -> Fallible<ImportMap> {
    // Step 1. Let parsed be the result of parsing a JSON string to an Infra value given input.
    let parsed: JsonValue = serde_json::from_str(input)
        .map_err(|_| Error::Type(c"The value needs to be a JSON object.".to_owned()))?;
    // Step 2. If parsed is not an ordered map, then throw a TypeError indicating that the
    // top-level value needs to be a JSON object.
    let JsonValue::Object(mut parsed) = parsed else {
        return Err(Error::Type(
            c"The top-level value needs to be a JSON object.".to_owned(),
        ));
    };

    // Step 3. Let sortedAndNormalizedImports be an empty ordered map.
    let mut sorted_and_normalized_imports = ModuleSpecifierMap::new();
    // Step 4. If parsed["imports"] exists, then:
    if let Some(imports) = parsed.get("imports") {
        // Step 4.1 If parsed["imports"] is not an ordered map, then throw a TypeError
        // indicating that the value for the "imports" top-level key needs to be a JSON object.
        let JsonValue::Object(imports) = imports else {
            return Err(Error::Type(
                c"The \"imports\" top-level value needs to be a JSON object.".to_owned(),
            ));
        };
        // Step 4.2 Set sortedAndNormalizedImports to the result of sorting and
        // normalizing a module specifier map given parsed["imports"] and baseURL.
        sorted_and_normalized_imports =
            sort_and_normalize_module_specifier_map(cx, global, imports, &base_url);
    }

    // Step 5. Let sortedAndNormalizedScopes be an empty ordered map.
    let mut sorted_and_normalized_scopes: IndexMap<ServoUrl, ModuleSpecifierMap> = IndexMap::new();
    // Step 6. If parsed["scopes"] exists, then:
    if let Some(scopes) = parsed.get("scopes") {
        // Step 6.1 If parsed["scopes"] is not an ordered map, then throw a TypeError
        // indicating that the value for the "scopes" top-level key needs to be a JSON object.
        let JsonValue::Object(scopes) = scopes else {
            return Err(Error::Type(
                c"The \"scopes\" top-level value needs to be a JSON object.".to_owned(),
            ));
        };
        // Step 6.2 Set sortedAndNormalizedScopes to the result of sorting and
        // normalizing scopes given parsed["scopes"] and baseURL.
        sorted_and_normalized_scopes = sort_and_normalize_scopes(cx, global, scopes, &base_url)?;
    }

    // Step 7. Let normalizedIntegrity be an empty ordered map.
    let mut normalized_integrity = ModuleIntegrityMap::new();
    // Step 8. If parsed["integrity"] exists, then:
    if let Some(integrity) = parsed.get("integrity") {
        // Step 8.1 If parsed["integrity"] is not an ordered map, then throw a TypeError
        // indicating that the value for the "integrity" top-level key needs to be a JSON object.
        let JsonValue::Object(integrity) = integrity else {
            return Err(Error::Type(
                c"The \"integrity\" top-level value needs to be a JSON object.".to_owned(),
            ));
        };
        // Step 8.2 Set normalizedIntegrity to the result of normalizing
        // a module integrity map given parsed["integrity"] and baseURL.
        normalized_integrity = normalize_module_integrity_map(cx, global, integrity, &base_url);
    }

    // Step 9. If parsed's keys contains any items besides "imports", "scopes", or "integrity",
    // then the user agent should report a warning to the console indicating that an invalid
    // top-level key was present in the import map.
    parsed.retain(|k, _| !matches!(k.as_str(), "imports" | "scopes" | "integrity"));
    if !parsed.is_empty() {
        Console::internal_warn(
            cx,
            global,
            "Invalid top-level key was present in the import map.
                Only \"imports\", \"scopes\", and \"integrity\" are allowed."
                .to_string(),
        );
    }

    // Step 10. Return an import map
    Ok(ImportMap {
        imports: sorted_and_normalized_imports,
        scopes: sorted_and_normalized_scopes,
        integrity: normalized_integrity,
    })
}

/// <https://html.spec.whatwg.org/multipage/#sorting-and-normalizing-a-module-specifier-map>
fn sort_and_normalize_module_specifier_map(
    cx: &mut JSContext,
    global: &GlobalScope,
    original_map: &JsonMap<String, JsonValue>,
    base_url: &ServoUrl,
) -> ModuleSpecifierMap {
    // Step 1. Let normalized be an empty ordered map.
    let mut normalized = ModuleSpecifierMap::new();

    // Step 2. For each specifier_key -> value in originalMap
    for (specifier_key, value) in original_map {
        // Step 2.1 Let normalized_specifier_key be the result of
        // normalizing a specifier key given specifier_key and base_url.
        let Some(normalized_specifier_key) =
            normalize_specifier_key(cx, global, specifier_key, base_url)
        else {
            // Step 2.2 If normalized_specifier_key is null, then continue.
            continue;
        };

        // Step 2.3 If value is not a string, then:
        let JsonValue::String(value) = value else {
            // Step 2.3.1 The user agent may report a warning to the console
            // indicating that addresses need to be strings.
            Console::internal_warn(cx, global, "Addresses need to be strings.".to_string());

            // Step 2.3.2 Set normalized[normalized_specifier_key] to null.
            normalized.insert(normalized_specifier_key, None);
            // Step 2.3.3 Continue.
            continue;
        };

        // Step 2.4. Let address_url be the result of resolving a URL-like module specifier given value and baseURL.
        let Some(address_url) =
            ModuleTree::resolve_url_like_module_specifier(value.as_str(), base_url)
        else {
            // Step 2.5 If address_url is null, then:
            // Step 2.5.1. The user agent may report a warning to the console
            // indicating that the address was invalid.
            Console::internal_warn(
                cx,
                global,
                format!("Value failed to resolve to module specifier: {value}"),
            );

            // Step 2.5.2 Set normalized[normalized_specifier_key] to null.
            normalized.insert(normalized_specifier_key, None);
            // Step 2.5.3 Continue.
            continue;
        };

        // Step 2.6 If specifier_key ends with U+002F (/), and the serialization of
        // address_url does not end with U+002F (/), then:
        if specifier_key.ends_with('\u{002f}') && !address_url.as_str().ends_with('\u{002f}') {
            // step 2.6.1. The user agent may report a warning to the console
            // indicating that an invalid address was given for the specifier key specifierKey;
            // since specifierKey ends with a slash, the address needs to as well.
            Console::internal_warn(
                cx,
                global,
                format!(
                    "Invalid address for specifier key '{specifier_key}': {address_url}.
                    Since specifierKey ends with a slash, the address needs to as well."
                ),
            );

            // Step 2.6.2 Set normalized[normalized_specifier_key] to null.
            normalized.insert(normalized_specifier_key, None);
            // Step 2.6.3 Continue.
            continue;
        }

        // Step 2.7 Set normalized[normalized_specifier_key] to address_url.
        normalized.insert(normalized_specifier_key, Some(address_url));
    }

    // Step 3. Return the result of sorting in descending order normalized
    // with an entry a being less than an entry b if a's key is code unit less than b's key.
    normalized.sort_by(|a_key, _, b_key, _| b_key.cmp(a_key));
    normalized
}

/// <https://html.spec.whatwg.org/multipage/#sorting-and-normalizing-scopes>
fn sort_and_normalize_scopes(
    cx: &mut JSContext,
    global: &GlobalScope,
    original_map: &JsonMap<String, JsonValue>,
    base_url: &ServoUrl,
) -> Fallible<IndexMap<ServoUrl, ModuleSpecifierMap>> {
    // Step 1. Let normalized be an empty ordered map.
    let mut normalized: IndexMap<ServoUrl, ModuleSpecifierMap> = IndexMap::new();

    // Step 2. For each scopePrefix → potentialSpecifierMap of originalMap:
    for (scope_prefix, potential_specifier_map) in original_map {
        // Step 2.1 If potentialSpecifierMap is not an ordered map, then throw a TypeError indicating
        // that the value of the scope with prefix scopePrefix needs to be a JSON object.
        let JsonValue::Object(potential_specifier_map) = potential_specifier_map else {
            return Err(Error::Type(
                c"The value of the scope with prefix scopePrefix needs to be a JSON object."
                    .to_owned(),
            ));
        };

        // Step 2.2 Let scopePrefixURL be the result of URL parsing scopePrefix with baseURL.
        let Ok(scope_prefix_url) = ServoUrl::parse_with_base(Some(base_url), scope_prefix) else {
            // Step 2.3 If scopePrefixURL is failure, then:
            // Step 2.3.1 The user agent may report a warning
            // to the console that the scope prefix URL was not parseable.
            Console::internal_warn(
                cx,
                global,
                format!("Scope prefix URL was not parseable: {scope_prefix}"),
            );
            // Step 2.3.2 Continue.
            continue;
        };

        // Step 2.4 Let normalizedScopePrefix be the serialization of scopePrefixURL.
        let normalized_scope_prefix = scope_prefix_url;

        // Step 2.5 Set normalized[normalizedScopePrefix] to the result of sorting and
        // normalizing a module specifier map given potentialSpecifierMap and baseURL.
        let normalized_specifier_map =
            sort_and_normalize_module_specifier_map(cx, global, potential_specifier_map, base_url);
        normalized.insert(normalized_scope_prefix, normalized_specifier_map);
    }

    // Step 3. Return the result of sorting in descending order normalized,
    // with an entry a being less than an entry b if a's key is code unit less than b's key.
    normalized.sort_by(|a_key, _, b_key, _| b_key.cmp(a_key));
    Ok(normalized)
}

/// <https://html.spec.whatwg.org/multipage/#normalizing-a-module-integrity-map>
fn normalize_module_integrity_map(
    cx: &mut JSContext,
    global: &GlobalScope,
    original_map: &JsonMap<String, JsonValue>,
    base_url: &ServoUrl,
) -> ModuleIntegrityMap {
    // Step 1. Let normalized be an empty ordered map.
    let mut normalized = ModuleIntegrityMap::new();

    // Step 2. For each key → value of originalMap:
    for (key, value) in original_map {
        // Step 2.1 Let resolvedURL be the result of
        // resolving a URL-like module specifier given key and baseURL.
        let Some(resolved_url) =
            ModuleTree::resolve_url_like_module_specifier(key.as_str(), base_url)
        else {
            // Step 2.2 If resolvedURL is null, then:
            // Step 2.2.1 The user agent may report a warning
            // to the console indicating that the key failed to resolve.
            Console::internal_warn(
                cx,
                global,
                format!("Key failed to resolve to module specifier: {key}"),
            );
            // Step 2.2.2 Continue.
            continue;
        };

        // Step 2.3 If value is not a string, then:
        let JsonValue::String(value) = value else {
            // Step 2.3.1 The user agent may report a warning
            // to the console indicating that integrity metadata values need to be strings.
            Console::internal_warn(
                cx,
                global,
                "Integrity metadata values need to be strings.".to_string(),
            );
            // Step 2.3.2 Continue.
            continue;
        };

        // Step 2.4 Set normalized[resolvedURL] to value.
        normalized.insert(resolved_url, value.clone());
    }

    // Step 3. Return normalized.
    normalized
}

/// <https://html.spec.whatwg.org/multipage/#normalizing-a-specifier-key>
fn normalize_specifier_key(
    cx: &mut JSContext,
    global: &GlobalScope,
    specifier_key: &str,
    base_url: &ServoUrl,
) -> Option<String> {
    // step 1. If specifierKey is the empty string, then:
    if specifier_key.is_empty() {
        // step 1.1 The user agent may report a warning to the console
        // indicating that specifier keys may not be the empty string.
        Console::internal_warn(
            cx,
            global,
            "Specifier keys may not be the empty string.".to_string(),
        );
        // step 1.2 Return null.
        return None;
    }
    // step 2. Let url be the result of resolving a URL-like module specifier, given specifierKey and baseURL.
    let url = ModuleTree::resolve_url_like_module_specifier(specifier_key, base_url);

    // step 3. If url is not null, then return the serialization of url.
    if let Some(url) = url {
        return Some(url.into_string());
    }

    // step 4. Return specifierKey.
    Some(specifier_key.to_string())
}

/// <https://html.spec.whatwg.org/multipage/#resolving-an-imports-match>
///
/// When the error is thrown, it will terminate the entire resolve a module specifier algorithm
/// without any further fallbacks.
fn resolve_imports_match(
    normalized_specifier: &str,
    as_url: Option<&ServoUrl>,
    specifier_map: &ModuleSpecifierMap,
) -> Fallible<Option<ServoUrl>> {
    // Step 1. For each specifierKey → resolutionResult of specifierMap:
    for (specifier_key, resolution_result) in specifier_map {
        // Step 1.1 If specifierKey is normalizedSpecifier, then:
        if specifier_key == normalized_specifier {
            if let Some(resolution_result) = resolution_result {
                // Step 1.1.2 Assert: resolutionResult is a URL.
                // This is checked by Url type already.
                // Step 1.1.3 Return resolutionResult.
                return Ok(Some(resolution_result.clone()));
            } else {
                // Step 1.1.1 If resolutionResult is null, then throw a TypeError.
                return Err(Error::Type(
                    c"Resolution of specifierKey was blocked by a null entry.".to_owned(),
                ));
            }
        }

        // Step 1.2 If all of the following are true:
        // - specifierKey ends with U+002F (/)
        // - specifierKey is a code unit prefix of normalizedSpecifier
        // - either asURL is null, or asURL is special, then:
        if specifier_key.ends_with('\u{002f}') &&
            normalized_specifier.starts_with(specifier_key) &&
            (as_url.is_none() || as_url.is_some_and(|u| u.is_special_scheme()))
        {
            // Step 1.2.1 If resolutionResult is null, then throw a TypeError.
            // Step 1.2.2 Assert: resolutionResult is a URL.
            let Some(resolution_result) = resolution_result else {
                return Err(Error::Type(
                    c"Resolution of specifierKey was blocked by a null entry.".to_owned(),
                ));
            };

            // Step 1.2.3 Let afterPrefix be the portion of normalizedSpecifier after the initial specifierKey prefix.
            let after_prefix = normalized_specifier
                .strip_prefix(specifier_key)
                .expect("specifier_key should be the prefix of normalized_specifier");

            // Step 1.2.4 Assert: resolutionResult, serialized, ends with U+002F (/), as enforced during parsing.
            debug_assert!(resolution_result.as_str().ends_with('\u{002f}'));

            // Step 1.2.5 Let url be the result of URL parsing afterPrefix with resolutionResult.
            let url = ServoUrl::parse_with_base(Some(resolution_result), after_prefix);

            // Step 1.2.6 If url is failure, then throw a TypeError
            // Step 1.2.7 Assert: url is a URL.
            let Ok(url) = url else {
                return Err(Error::Type(
                    c"Resolution of normalizedSpecifier was blocked since
                    the afterPrefix portion could not be URL-parsed relative to
                    the resolutionResult mapped to by the specifierKey prefix."
                        .to_owned(),
                ));
            };

            // Step 1.2.8 If the serialization of resolutionResult is not
            // a code unit prefix of the serialization of url, then throw a TypeError
            if !url.as_str().starts_with(resolution_result.as_str()) {
                return Err(Error::Type(
                    c"Resolution of normalizedSpecifier was blocked due to
                    it backtracking above its prefix specifierKey."
                        .to_owned(),
                ));
            }

            // Step 1.2.9 Return url.
            return Ok(Some(url));
        }
    }

    // Step 2. Return null.
    Ok(None)
}
