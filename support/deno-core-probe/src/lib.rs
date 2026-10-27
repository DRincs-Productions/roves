//! Isolated deno_core 0.413.0 feasibility probe for the single-document runtime
//! (docs/handoff/0380-single-document-runtime.md).
//!
//! One document means one `JsRuntime` and its main realm: no additional realms, frames or
//! WindowProxy reuse are modelled. Every scenario builds its own runtime with an in-memory
//! `ModuleLoader`: no file system, no network, no Deno extensions or permissions. Only
//! public deno_core APIs and its `v8` re-export are used. Each scenario returns the JS-side
//! log so `tests/probe.rs` can assert the observed order.
//!
//! [`new_runtime`] is the only constructor scenarios use for game code: it removes deno_core's
//! bootstrap namespace globals (`Deno`, `__bootstrap`, `__infra`) before any script or module
//! runs, so every scenario below also runs on the stripped global. The `JsRuntime` stays the
//! single owner of the module map, dynamic imports and the event loop.
//!
//! Scope of the evidence: these scenarios exercise deno_core's module map, loader, event
//! loop, the import-attributes callback and the context's string-compilation flag. They involve no Servo DOM, no History API and no HTML
//! origin security or CSP policy; where a scenario touches those areas it only shows that
//! a hook exists (see README.md).
//!
//! V8 is initialized once per process by [`v8_mode`], in the mode named by `V8_TEST_MODE`,
//! before the first `JsRuntime` exists.
//!
//! [`native_class`] holds the cppgc-backed native class fixture (owned metadata, traced native
//! child, wrapper identity); it runs on the same stripped document runtime.
//!
//! [`native_async`] holds the native async op fixture (one Rust future behind one captured
//! game function, completion and shutdown cancellation); same stripped document runtime.

mod native_async;
mod native_class;

pub use native_async::NativeAsyncReport;
pub use native_async::OpCounts;
pub use native_async::native_async_op_lifecycle;
pub use native_class::NativeClassReport;
pub use native_class::native_class_ownership;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use deno_core::Extension;
use deno_core::ImportAttributesContext;
use deno_core::JsRuntime;
use deno_core::ModuleLoadOptions;
use deno_core::ModuleLoadReferrer;
use deno_core::ModuleLoadResponse;
use deno_core::ModuleLoader;
use deno_core::ModuleResolveResponse;
use deno_core::ModuleSource;
use deno_core::ModuleSourceCode;
use deno_core::ModuleSpecifier;
use deno_core::ModuleType;
use deno_core::RequestedModuleType;
use deno_core::ResolutionKind;
use deno_core::RuntimeOptions;
use deno_core::error::ModuleLoaderError;
use deno_core::resolve_import;
use deno_core::v8;

/// Environment variable selecting the V8 mode for the whole test process.
pub const MODE_ENV: &str = "V8_TEST_MODE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Jitless,
}

/// Parses `V8_TEST_MODE`. Unset and unknown values are errors, so a CI typo cannot silently
/// run the normal suite twice.
pub fn parse_mode(value: Option<&str>) -> Result<Mode, String> {
    match value {
        Some("normal") => Ok(Mode::Normal),
        Some("jitless") => Ok(Mode::Jitless),
        Some(other) => Err(format!("{MODE_ENV}={other:?}: expected \"normal\" or \"jitless\"")),
        None => Err(format!("{MODE_ENV} must be set to \"normal\" or \"jitless\"")),
    }
}

static V8_MODE: OnceLock<Result<Mode, String>> = OnceLock::new();

/// Sets the V8 flags for the configured mode and initializes V8 through deno_core, exactly
/// once per process and before any `JsRuntime`. deno_core's own `init_v8` is guarded by a
/// `Once` (`runtime/setup.rs:229-243`) and adds its base flags with `set_flags_from_string`,
/// which does not undo `--jitless`. A bad mode leaves V8 uninitialized and fails every
/// scenario with the same message.
pub fn v8_mode() -> Result<Mode, String> {
    V8_MODE
        .get_or_init(|| {
            let value = match std::env::var(MODE_ENV) {
                Ok(value) => Some(value),
                Err(std::env::VarError::NotPresent) => None,
                Err(error) => return Err(format!("{MODE_ENV}: {error}")),
            };
            let mode = parse_mode(value.as_deref())?;
            if mode == Mode::Jitless {
                // The first element stands for the program name and is returned back.
                let unrecognized = deno_core::v8_set_flags(vec![String::new(), "--jitless".into()]);
                if unrecognized.len() > 1 {
                    return Err(format!("V8 did not recognize {:?}", &unrecognized[1..]));
                }
            }
            JsRuntime::init_platform(None);
            Ok(mode)
        })
        .clone()
}

/// The only origin the in-memory loader serves. `.invalid` never resolves on a network.
pub const ORIGIN: &str = "https://game.invalid/";
const HOST: &str = "game.invalid";

pub type Log = Vec<String>;

/// In-memory module source: what a fetch would have returned, with its MIME-derived type.
struct Served {
    module_type: ModuleType,
    code: &'static str,
}

/// Resolution allowlist + in-memory "fetch". Stands in for Servo's fetch/CSP/integrity
/// layer, which would be called from `resolve`/`load` in a real integration.
pub struct MemoryLoader {
    modules: HashMap<String, Served>,
    /// Specifiers mapped verbatim to a URL, bypassing the allowlist (see [`Self::with_alias`]).
    aliases: HashMap<String, String>,
    /// Every URL `load` was asked for, allowed or not.
    pub loads: RefCell<Vec<String>>,
    /// Every specifier `resolve` refused.
    pub refused: RefCell<Vec<String>>,
}

impl MemoryLoader {
    pub fn new(modules: &[(&str, ModuleType, &'static str)]) -> Self {
        Self {
            modules: modules
                .iter()
                .map(|(path, module_type, code)| {
                    (format!("{ORIGIN}{path}"), Served { module_type: module_type.clone(), code })
                })
                .collect(),
            aliases: HashMap::new(),
            loads: RefCell::default(),
            refused: RefCell::default(),
        }
    }

    /// Models a loader or import map that maps a game specifier onto deno_core's internal
    /// namespace (e.g. `ext:core/ops`). deno_core's own `ext:` guard in `ModuleMap::resolve`,
    /// not this allowlist, must then refuse it.
    pub fn with_alias(mut self, specifier: &str, url: &str) -> Self {
        self.aliases.insert(specifier.to_owned(), url.to_owned());
        self
    }
}

impl ModuleLoader for MemoryLoader {
    fn resolve(&self, specifier: &str, referrer: &str, _kind: ResolutionKind) -> ModuleResolveResponse {
        if let Some(url) = self.aliases.get(specifier) {
            return ModuleSpecifier::parse(url).map_err(|error| ModuleLoaderError::generic(error.to_string()));
        }
        let refuse = |why: &str| {
            self.refused.borrow_mut().push(specifier.to_owned());
            Err(ModuleLoaderError::type_error(format!("{why}: {specifier}")))
        };
        // Bare specifiers fail here (no import map in this probe).
        let url = match resolve_import(specifier, referrer) {
            Ok(url) => url,
            Err(_) => return refuse("unresolvable specifier"),
        };
        if url.scheme() != "https" || url.host_str() != Some(HOST) || url.port().is_some() {
            return refuse("blocked by the probe allowlist");
        }
        Ok(url)
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        _referrer: Option<&ModuleLoadReferrer>,
        options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        self.loads.borrow_mut().push(specifier.to_string());
        let Some(served) = self.modules.get(specifier.as_str()) else {
            return ModuleLoadResponse::Sync(Err(ModuleLoaderError::generic(format!("not found: {specifier}"))));
        };
        // HTML: "javascript-or-wasm" needs a JavaScript MIME type, "json" a JSON MIME type.
        // deno_core leaves this check to the loader (its FsModuleLoader does the same).
        let matches = matches!(
            (&options.requested_module_type, &served.module_type),
            (RequestedModuleType::None, ModuleType::JavaScript) | (RequestedModuleType::Json, ModuleType::Json)
        );
        if !matches {
            return ModuleLoadResponse::Sync(Err(ModuleLoaderError::type_error(format!(
                "module type mismatch for {specifier}: requested {:?}, served {}",
                options.requested_module_type, served.module_type
            ))));
        }
        ModuleLoadResponse::Sync(Ok(ModuleSource::new(
            served.module_type.clone(),
            ModuleSourceCode::String(served.code.to_owned().into()),
            specifier,
            None,
        )))
    }
}

/// HTML's import attribute rules: only the `type` key, and only module types the host
/// supports (`json` here; `css` would need `custom_module_evaluation_cb`).
fn validate_import_attributes(
    scope: &mut v8::PinScope,
    attributes: &HashMap<String, String>,
    _context: &ImportAttributesContext,
) {
    for (key, value) in attributes {
        let message = if key != "type" {
            format!("unsupported import attribute \"{key}\"")
        } else if value != "json" {
            format!("unsupported module type \"{value}\"")
        } else {
            continue;
        };
        let message = v8::String::new(scope, &message).unwrap();
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
        return;
    }
}

/// Creates the document's runtime: deno_core's bootstrap followed by
/// `remove_bootstrap_globals`, before any game script or module can run. This is the only
/// constructor for game code. Must run inside a tokio runtime: deno_core aborts the process
/// when V8 posts a delayed task for an isolate created outside one (`runtime/setup.rs:99-110`),
/// so the context is checked here instead of left to chance.
pub fn new_runtime(loader: Rc<MemoryLoader>) -> Result<JsRuntime, String> {
    new_runtime_with(loader, Vec::new())
}

/// [`new_runtime`] plus fixture extensions, initialized (ops, objects, trusted extension
/// scripts) before the bootstrap globals are removed: used by the [`native_class`] and
/// [`native_async`] fixtures.
fn new_runtime_with(loader: Rc<MemoryLoader>, extensions: Vec<Extension>) -> Result<JsRuntime, String> {
    let mut runtime = new_raw_runtime(loader, extensions)?;
    remove_bootstrap_globals(&mut runtime)?;
    Ok(runtime)
}

/// deno_core's runtime as `JsRuntime::try_new` leaves it, bootstrap globals included. Private:
/// used by [`new_runtime_with`] and by [`bootstrap_isolation`]'s before/after observation only,
/// never handed to game code.
fn new_raw_runtime(loader: Rc<MemoryLoader>, extensions: Vec<Extension>) -> Result<JsRuntime, String> {
    v8_mode()?;
    tokio::runtime::Handle::try_current().map_err(|error| format!("JsRuntime needs a tokio context: {error}"))?;
    JsRuntime::try_new(RuntimeOptions {
        module_loader: Some(loader),
        extensions,
        validate_import_attributes_cb: Some(Box::new(validate_import_attributes)),
        ..Default::default()
    })
    .map_err(err)
}

/// Global names deno_core 0.413.0's bootstrap puts on the main realm: `Deno` (`runtime/
/// bindings.rs` `initialize_deno_core_namespace`), `__bootstrap` (`00_primordials.js`) and
/// `__infra` (`00_infra.js`; `01_core.js` already deletes it, kept so a deno_core change that
/// stops doing so is caught). Op functions live under `Deno.core.ops`.
pub const BOOTSTRAP_GLOBALS: [&str; 3] = ["Deno", "__bootstrap", "__infra"];

/// Deletes [`BOOTSTRAP_GLOBALS`] from the global object with the native `v8::Object` API, after
/// `JsRuntime::try_new` returned. deno_core reads `Deno.core` from the global only while it is
/// being constructed (`store_js_callbacks`, `snapshotted_fast_op_refs`) and keeps `v8::Global`
/// handles afterwards; its own comments describe `Deno.core` being scrubbed after bootstrap.
/// A non-configurable property is reported as a blocker, never hidden behind a facade.
fn remove_bootstrap_globals(runtime: &mut JsRuntime) -> Result<Log, String> {
    deno_core::scope!(scope, runtime);
    let context = scope.get_current_context();
    let global = context.global(scope);
    let configurable_key = v8::String::new(scope, "configurable").ok_or("cannot allocate a V8 string")?;
    let mut report = Log::new();
    for name in BOOTSTRAP_GLOBALS {
        let key = v8::String::new(scope, name).ok_or("cannot allocate a V8 string")?;
        let descriptor = global
            .get_own_property_descriptor(scope, key.into())
            .ok_or_else(|| format!("reading the globalThis.{name} descriptor threw"))?;
        if descriptor.is_undefined() {
            report.push(format!("{name}:absent"));
            continue;
        }
        let descriptor: v8::Local<v8::Object> = descriptor.try_into().map_err(err)?;
        let configurable = descriptor.get(scope, configurable_key.into()).is_some_and(|value| value.is_true());
        if !configurable {
            return Err(format!(
                "blocker: globalThis.{name} is non-configurable and cannot be removed with public APIs"
            ));
        }
        if global.delete(scope, key.into()) != Some(true) || global.has_own_property(scope, key.into()) != Some(false) {
            return Err(format!("deleting globalThis.{name} did not remove it"));
        }
        report.push(format!("{name}:deleted"));
    }
    Ok(report)
}

/// A current-thread tokio runtime per scenario; `enable_all` because deno_core's delayed V8
/// tasks sleep on tokio's timer (tokio's "time" feature comes from deno_core's `tokio/full`).
fn block_on<T>(future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(err)?
        .block_on(future)
}

fn err(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

fn script(runtime: &mut JsRuntime, name: &str, code: &str) -> Result<v8::Global<v8::Value>, String> {
    runtime.execute_script(format!("{ORIGIN}{name}"), code.to_owned()).map_err(err)
}

/// Runs a main-realm script whose completion value is a string.
fn string_script(runtime: &mut JsRuntime, name: &str, code: &str) -> Result<String, String> {
    let value = script(runtime, name, code)?;
    deno_core::scope!(scope, runtime);
    let value = v8::Local::new(scope, value);
    let string: v8::Local<v8::String> = value.try_into().map_err(err)?;
    Ok(string.to_rust_string_lossy(scope))
}

/// Reads `globalThis.log` (an array of strings) from the main realm.
fn read_log(runtime: &mut JsRuntime) -> Result<Log, String> {
    let json = string_script(runtime, "read-log.js", "JSON.stringify(globalThis.log)")?;
    deno_core::serde_json::from_str(&json).map_err(err)
}

async fn run_main(runtime: &mut JsRuntime, path: &str) -> Result<(), String> {
    let url = ModuleSpecifier::parse(&format!("{ORIGIN}{path}")).map_err(err)?;
    let id = runtime.load_main_es_module(&url).await.map_err(err)?;
    let evaluation = runtime.mod_evaluate(id);
    runtime.run_event_loop(Default::default()).await.map_err(err)?;
    evaluation.await.map_err(err)
}

const SETUP: &str = "globalThis.log = [];";

/// Static imports, a JSON module with `with { type: "json" }`, and a two-module cycle.
pub fn static_json_and_cycle() -> Result<Log, String> {
    block_on(async {
        let loader = Rc::new(MemoryLoader::new(&[
            (
                "main.js",
                ModuleType::JavaScript,
                r#"import { a } from "./a.js";
                   import data from "./data.json" with { type: "json" };
                   log.push("main:" + a + ":" + data.answer);"#,
            ),
            ("a.js", ModuleType::JavaScript, r#"import { b } from "./b.js"; export const a = "A"; log.push("a:" + typeof b + ":" + b());"#),
            // `b` only touches `a` lazily: the cycle is legal and evaluates b before a.
            ("b.js", ModuleType::JavaScript, r#"import { a } from "./a.js"; export function b() { return a; } log.push("b");"#),
            ("data.json", ModuleType::Json, r#"{ "answer": 42 }"#),
        ]));
        let mut runtime = new_runtime(loader)?;
        script(&mut runtime, "setup.js", SETUP)?;
        run_main(&mut runtime, "main.js").await?;
        read_log(&mut runtime)
    })
}

/// Top-level await in a dependency, a synchronous sibling, and microtasks queued during
/// module evaluation.
pub fn top_level_await_and_microtasks() -> Result<Log, String> {
    block_on(async {
        let loader = Rc::new(MemoryLoader::new(&[
            (
                "main.js",
                ModuleType::JavaScript,
                r#"import "./slow.js";
                   import "./fast.js";
                   Promise.resolve().then(() => log.push("main:micro"));
                   queueMicrotask(() => log.push("main:queueMicrotask"));
                   log.push("main");"#,
            ),
            ("slow.js", ModuleType::JavaScript, r#"log.push("slow:start"); await Promise.resolve(); log.push("slow:end");"#),
            ("fast.js", ModuleType::JavaScript, r#"log.push("fast");"#),
        ]));
        let mut runtime = new_runtime(loader)?;
        script(&mut runtime, "setup.js", SETUP)?;
        run_main(&mut runtime, "main.js").await?;
        read_log(&mut runtime)
    })
}

/// Classic scripts do not drain microtasks by themselves (Explicit policy): the embedder
/// decides when to checkpoint, which is what HTML's "clean up after running script" needs.
pub fn classic_script_checkpoint() -> Result<(Log, Log), String> {
    block_on(async {
        let mut runtime = new_runtime(Rc::new(MemoryLoader::new(&[])))?;
        script(
            &mut runtime,
            "classic.js",
            r#"globalThis.log = [];
               Promise.resolve().then(() => log.push("micro"));
               log.push("sync");"#,
        )?;
        let before = read_log(&mut runtime)?;
        runtime.v8_isolate().perform_microtask_checkpoint();
        let after = read_log(&mut runtime)?;
        Ok((before, after))
    })
}

/// `import()` from a classic script, for JavaScript and JSON, resolved by the event loop.
pub fn dynamic_import() -> Result<Log, String> {
    block_on(async {
        let loader = Rc::new(MemoryLoader::new(&[
            ("dyn.js", ModuleType::JavaScript, r#"log.push("dyn:evaluated"); export const value = 7;"#),
            ("data.json", ModuleType::Json, r#"{ "answer": 42 }"#),
        ]));
        let mut runtime = new_runtime(loader)?;
        script(
            &mut runtime,
            "classic.js",
            r#"globalThis.log = [];
               import("./dyn.js").then(m => log.push("dyn:" + m.value));
               import("./data.json", { with: { type: "json" } }).then(m => log.push("json:" + m.default.answer));
               log.push("after-import-calls");"#,
        )?;
        runtime.run_event_loop(Default::default()).await.map_err(err)?;
        read_log(&mut runtime)
    })
}

const SETTLE_ALL: &str = r#"
  globalThis.settle = (label, promise) =>
    promise.then(
      () => log.push(label + ":fulfilled"),
      e => log.push(label + ":rejected:" + e.name),
    );
"#;

/// Import attributes and module types: everything here must reject.
pub fn import_attribute_policy() -> Result<Log, String> {
    block_on(async {
        let loader = Rc::new(MemoryLoader::new(&[
            ("a.js", ModuleType::JavaScript, "export default 1;"),
            ("data.json", ModuleType::Json, r#"{ "answer": 42 }"#),
        ]));
        let mut runtime = new_runtime(loader)?;
        script(&mut runtime, "setup.js", SETUP)?;
        script(&mut runtime, "settle.js", SETTLE_ALL)?;
        script(
            &mut runtime,
            "attributes.js",
            r#"settle("json-without-type", import("./data.json"));
               settle("js-as-json", import("./a.js", { with: { type: "json" } }));
               settle("text-type", import("./a.js", { with: { type: "text" } }));
               settle("bytes-type", import("./a.js", { with: { type: "bytes" } }));
               settle("unknown-key", import("./a.js", { with: { foo: "bar" } }));"#,
        )?;
        runtime.run_event_loop(Default::default()).await.map_err(err)?;
        let mut log = read_log(&mut runtime)?;
        log.sort();
        Ok(log)
    })
}

/// What the allowlist observed for `resolution_allowlist`.
pub struct AllowlistReport {
    pub log: Log,
    pub loads: Vec<String>,
    pub refused: Vec<String>,
    pub pwned: bool,
}

/// Off-origin, `data:`, `file:`, `ext:`, `node:` and bare specifiers must never reach `load`.
pub fn resolution_allowlist() -> Result<AllowlistReport, String> {
    block_on(async {
        let loader = Rc::new(MemoryLoader::new(&[("ok.js", ModuleType::JavaScript, "export default 1;")]));
        let mut runtime = new_runtime(loader.clone())?;
        script(&mut runtime, "setup.js", SETUP)?;
        script(&mut runtime, "settle.js", SETTLE_ALL)?;
        script(
            &mut runtime,
            "allowlist.js",
            r#"settle("same-origin", import("./ok.js"));
               settle("other-origin", import("https://evil.invalid/x.js"));
               settle("other-port", import("https://game.invalid:8443/ok.js"));
               settle("data-url", import("data:text/javascript,globalThis.pwned=true"));
               settle("file-url", import("file:///etc/passwd"));
               settle("ext-internal", import("ext:core/mod.js"));
               settle("node-builtin", import("node:fs"));
               settle("bare", import("lodash"));"#,
        )?;
        runtime.run_event_loop(Default::default()).await.map_err(err)?;
        let mut log = read_log(&mut runtime)?;
        log.sort();
        let pwned = string_script(&mut runtime, "pwned.js", "String(globalThis.pwned === true)")? == "true";
        Ok(AllowlistReport {
            log,
            loads: loader.loads.borrow().clone(),
            refused: loader.refused.borrow().clone(),
            pwned,
        })
    })
}

/// Sets the document context's string-compilation flag (`v8::Context::
/// set_allow_generation_from_strings`, public in v8 150.4.0) and returns what V8 reports.
fn allow_string_compilation(runtime: &mut JsRuntime, allow: bool) -> bool {
    deno_core::scope!(scope, runtime);
    let context = scope.get_current_context();
    context.set_allow_generation_from_strings(allow);
    context.is_code_generation_from_strings_allowed()
}

const TRY_STRING_COMPILATION: &str = r#"
  for (const [label, run] of [["eval", () => eval("1 + 1")], ["Function", () => new Function("return 1")()]]) {
    try { run(); log.push(label + ":allowed"); } catch (e) { log.push(label + ":" + e.name); }
  }
  log.push("script:ran");
"#;

/// Context-level switch only: v8 150.4.0 (rusty_v8 as re-exported by deno_v8 0.5.0) exposes
/// no per-string code-generation callback, so this is all-or-nothing for the document's
/// context. Scripts the embedder compiles (`execute_script`) are not affected by the flag.
/// No CSP or Trusted Types policy is evaluated here.
pub fn string_compilation_context_flag() -> Result<(bool, Log, bool, Log), String> {
    block_on(async {
        let mut runtime = new_runtime(Rc::new(MemoryLoader::new(&[])))?;
        let denied_flag = allow_string_compilation(&mut runtime, false);
        script(&mut runtime, "setup.js", SETUP)?;
        script(&mut runtime, "denied.js", TRY_STRING_COMPILATION)?;
        let denied = read_log(&mut runtime)?;
        let allowed_flag = allow_string_compilation(&mut runtime, true);
        script(&mut runtime, "setup.js", SETUP)?;
        script(&mut runtime, "allowed.js", TRY_STRING_COMPILATION)?;
        let allowed = read_log(&mut runtime)?;
        Ok((denied_flag, denied, allowed_flag, allowed))
    })
}

/// Whether a minimal WebAssembly module compiles: the observable difference between the
/// normal and JITless modes (Roves' own JITless tests rely on the same V8 behaviour).
pub fn webassembly_availability() -> Result<String, String> {
    block_on(async {
        let mut runtime = new_runtime(Rc::new(MemoryLoader::new(&[])))?;
        string_script(
            &mut runtime,
            "wasm.js",
            r#"(() => {
                 if (typeof WebAssembly === "undefined") return "absent";
                 try { new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])); return "compiled"; }
                 catch (e) { return "throws:" + e.name; }
               })()"#,
        )
    })
}

/// What `bootstrap_isolation` observed.
pub struct IsolationReport {
    /// Property descriptors of [`BOOTSTRAP_GLOBALS`] on a raw runtime, read by a trusted script.
    pub before: Log,
    /// What `remove_bootstrap_globals` reported on that same raw runtime.
    pub removal: Log,
    /// What the game's main module logged on a [`new_runtime`] document.
    pub log: Log,
    pub loads: Vec<String>,
    pub refused: Vec<String>,
    /// Registered op names that look like file system, network or process capabilities.
    pub privileged_ops: Vec<String>,
    /// Entries in the runtime's resource table after the game ran (no stdio, files, sockets).
    pub resources: usize,
}

const DESCRIBE_BOOTSTRAP_GLOBALS: &str = r#"
  JSON.stringify(["Deno", "__bootstrap", "__infra"].map(name => {
    const d = Object.getOwnPropertyDescriptor(globalThis, name);
    if (d === undefined) return name + ":absent";
    return name + ":" + ("value" in d ? "data" : "accessor") + ":" + (d.configurable ? "configurable" : "non-configurable");
  }))
"#;

/// Op-name fragments of Deno's privileged extensions (fs, net, fetch, process, ffi, node).
/// deno_core's builtin resource ops (`op_read`, `op_write`, `op_print`, ...) are registered
/// but reachable only through the removed `Deno.core` namespace and `ext:core/ops`.
const PRIVILEGED_OP_PREFIXES: &[&str] = &[
    // Not "op_run": the builtin op_run_microtasks would match.
    "op_fs_", "op_net_", "op_fetch", "op_http", "op_ws_", "op_spawn", "op_kill", "op_env", "op_open",
    "op_read_file", "op_write_file", "op_remove", "op_mkdir", "op_dlopen", "op_ffi", "op_node_", "op_process",
    "op_exit",
];

/// Registered op names matching [`PRIVILEGED_OP_PREFIXES`].
fn privileged_ops(runtime: &JsRuntime) -> Vec<String> {
    runtime
        .op_names()
        .into_iter()
        .filter(|name| PRIVILEGED_OP_PREFIXES.iter().any(|prefix| name.starts_with(prefix)))
        .map(str::to_owned)
        .collect()
}

/// Bootstrap globals before and after removal, then a game module on the default document
/// runtime: target globals absent, standard globals working, deno_core's internal modules
/// unreachable by direct specifier, by a loader alias, by a static import and by
/// `import.meta.resolve`. Proves the absence of this namespace only, not general privilege
/// isolation (see README.md).
pub fn bootstrap_isolation() -> Result<IsolationReport, String> {
    block_on(async {
        let (before, removal) = {
            let mut raw = new_raw_runtime(Rc::new(MemoryLoader::new(&[])), Vec::new())?;
            let json = string_script(&mut raw, "describe.js", DESCRIBE_BOOTSTRAP_GLOBALS)?;
            let before: Log = deno_core::serde_json::from_str(&json).map_err(err)?;
            (before, remove_bootstrap_globals(&mut raw)?)
        };

        let loader = Rc::new(
            MemoryLoader::new(&[
                (
                    "main.js",
                    ModuleType::JavaScript,
                    r#"for (const name of ["Deno", "__bootstrap", "__infra"]) {
                         log.push(name + ":" + typeof globalThis[name] + ":" + Object.hasOwn(globalThis, name));
                       }
                       log.push("Object:" + Object.keys({ a: 1 }).join());
                       const leaks = [];
                       for (const name of Object.getOwnPropertyNames(globalThis)) {
                         const value = globalThis[name];
                         if (value === globalThis || value === null || (typeof value !== "object" && typeof value !== "function")) continue;
                         for (const key of ["core", "ops", "primordials", "internals"]) {
                           if (Object.hasOwn(value, key)) leaks.push(name + "." + key);
                         }
                       }
                       log.push("namespace-leaks:" + leaks.join());
                       const privileged = ["fetch", "XMLHttpRequest", "WebSocket", "EventSource", "process", "require", "Buffer", "Bun", "open", "importScripts"];
                       log.push("privileged-globals:" + privileged.filter(name => typeof globalThis[name] !== "undefined").join());
                       log.push("import.meta:" + Object.keys(import.meta).sort().join());
                       try { import.meta.resolve("ext:core/ops"); log.push("meta-resolve:allowed"); }
                       catch (e) { log.push("meta-resolve:" + e.name); }
                       Promise.resolve("p").then(v => log.push("promise:" + v));
                       queueMicrotask(() => log.push("queueMicrotask:ran"));
                       const guard = (label, promise) => promise.then(
                         () => log.push(label + ":fulfilled"),
                         e => log.push(label + ":rejected:" + (String(e && e.message).includes("Importing ext: modules is only allowed") ? "core-guard" : "other")),
                       );
                       guard("ext-mod", import("ext:core/mod.js"));
                       guard("ext-ops", import("ext:core/ops"));
                       guard("alias-ops", import("internal-ops"));
                       guard("alias-mod", import("internal-mod"));
                       guard("static-alias", import("./static-alias.js"));
                       log.push("main:sync-done");"#,
                ),
                ("static-alias.js", ModuleType::JavaScript, r#"import * as ops from "internal-ops"; log.push("static-alias:evaluated:" + typeof ops);"#),
            ])
            .with_alias("internal-ops", "ext:core/ops")
            .with_alias("internal-mod", "ext:core/mod.js"),
        );
        let mut runtime = new_runtime(loader.clone())?;
        script(&mut runtime, "setup.js", SETUP)?;
        run_main(&mut runtime, "main.js").await?;
        let log = read_log(&mut runtime)?;
        let privileged_ops = privileged_ops(&runtime);
        let resources = runtime.op_state().borrow().resource_table.names().count();
        Ok(IsolationReport {
            before,
            removal,
            log,
            loads: loader.loads.borrow().clone(),
            refused: loader.refused.borrow().clone(),
            privileged_ops,
            resources,
        })
    })
}
