//! Minimal, contained V8 runtime integration — Phases 1-2 of `../../docs/V8_MIGRATION.md`.
//!
//! This crate owns V8 platform/isolate/context lifecycle and exposes a small, engine-neutral
//! surface: no `v8::*` type appears in this crate's public API, matching the migration plan's
//! hard architectural rule that V8 types must not cross the scripting boundary. It is not wired
//! into Roves/Servo's production script engine yet; see the plan document for the full phase
//! sequence and this phase's own checkpoint.
//!
//! Scope note for anyone extending this file: this version of the `v8` crate uses `!Unpin`,
//! pinned scopes (`v8::scope!`/`v8::tc_scope!` macros, not `HandleScope::new`/`TryCatch::new`
//! called directly — see `docs/V8_MIGRATION.md`'s Phase 1 status note for why), and only one
//! isolate may be entered per OS thread at a time. Exception-reading and value-conversion logic
//! below is duplicated inline at each call site rather than factored into shared generic
//! functions: earlier attempts at that ran into the scope macros' anonymous, lifetime-heavy
//! generated types being awkward to name in a function signature. A handful of duplicated lines
//! is a fine trade against fighting that.

use std::sync::Once;

static V8_PLATFORM_INIT: Once = Once::new();

/// Initializes the V8 platform exactly once per process. Safe to call repeatedly; only the
/// first call has an effect. Must run before any [`Runtime`] is created.
fn ensure_platform_initialized() {
    V8_PLATFORM_INIT.call_once(|| {
        // See ../../docs/V8_MIGRATION.md's "Console-oriented requirement: JIT-less" — a
        // first-class supported configuration from the start. This must run before
        // `initialize_platform`/`initialize` below, not after.
        #[cfg(feature = "jitless")]
        v8::V8::set_flags_from_string("--jitless");

        // `Isolate::request_garbage_collection_for_testing` (used by this crate's own GC
        // stress tests, see docs/V8_MIGRATION.md's Phase 3 status note) only works if
        // `--expose-gc` was set here first — it "has a strong negative impact on garbage
        // collection performance" per its own doc comment, so this only applies to `cargo test`
        // builds, never a real embedding.
        #[cfg(test)]
        v8::V8::set_flags_from_string("--expose-gc");

        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

/// An engine-neutral snapshot of a JS value — see this module's own doc comment on why no
/// `v8::*` type crosses this boundary. `Object` is a one-way read-only marker: Phase 2 doesn't
/// need structural property access yet (that's Phase 4's WebIDL-bindings job), so a value that
/// isn't one of the primitives below just reports "it's some kind of object", and converting an
/// `Object` back into a JS value (e.g. via [`Runtime::store`]) produces `undefined` — there is
/// no data here to reconstruct the original object from.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// Raw bytes — round-trips through a JS `Uint8Array` (see [`Runtime::eval_value`] and
    /// [`Runtime::store`]). Distinct from `Object` because this phase's own bindgen-primitives
    /// scope explicitly needs ArrayBuffer/TypedArray, not just "some object".
    Bytes(Vec<u8>),
    Object,
}

/// A persistent reference to a JS value, outliving any single [`Runtime::eval`]/`eval_value`
/// call. Backed by a `v8::Global`, which is isolate-scoped rather than context-scoped, so it can
/// be read back (via [`Runtime::load`]) from a different fresh context than the one it was
/// created in — the migration plan's Phase 3 (GC/DOM ownership) will build the real reflector
/// identity/lifetime model on top of this same primitive; this phase only needs the handle
/// itself to work.
pub struct Handle(v8::Global<v8::Value>);

/// A named JS interface (constructor + prototype chain), created via
/// [`Runtime::define_interface`] — see that method's own doc comment.
pub struct Interface {
    template: v8::Global<v8::FunctionTemplate>,
    name: String,
    /// Set once the constructor has actually been exposed on the global object (lazily, at the
    /// first [`Runtime::create_instance`] call — see that method's own doc comment on why this
    /// can't happen eagerly in [`Runtime::define_interface`]).
    constructor_exposed: std::cell::Cell<bool>,
}

/// A single, isolated V8 execution environment: one [`v8::OwnedIsolate`] plus one persistent
/// JS realm (`context`), shared by every [`Runtime`] method — a global installed by
/// [`Runtime::define_native_function`] is only visible to a later [`Runtime::eval`] call because
/// both enter the *same* context. An earlier version of this crate created a brand-new,
/// throwaway context inside every method, which is why that sharing silently didn't work — each
/// call had its own global object, exactly like two different pages/realms in a real browser.
pub struct Runtime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
    /// Keeps every [`Runtime::create_wrapped`] finalizer armed — see the `v8` crate's own doc
    /// comment on `Weak`: "finalization callbacks are tied to the lifetime of a `Weak<T>`, and
    /// will not be called after the `Weak<T>` is dropped." A `Weak` that already fired stays in
    /// this list forever (a real bookkeeping cost this phase's own stress test measures but
    /// doesn't try to solve — see `docs/V8_MIGRATION.md`'s Phase 3 status note).
    wrapped_finalizers: Vec<v8::Weak<v8::Value>>,
}

/// Tag passed to `set_aligned_pointer_in_internal_field`/`get_aligned_pointer_from_internal_field`
/// — required by the API. V8's sandboxed external-pointer table only accepts a small range of
/// tag values (found by hitting "Fatal error in ToExternalPointerTag: the provided tag is
/// outside the allowed range" with an arbitrary `0xC0DE` on the first attempt); `0` is what the
/// `v8` crate's own `tests/test_api.rs` uses for its single-tag internal-field examples, and this
/// crate likewise only has one kind of tagged pointer so far, so there's no need for a distinct
/// value per type yet.
const WRAPPED_POINTER_TAG: u16 = 0;

/// The result of running a script: either its final expression's string representation, or the
/// message from an uncaught exception (syntax error or a thrown value). Deliberately a plain
/// `Result<String, String>` — see this module's own doc comment on why no `v8::*` type escapes.
pub type ScriptResult = Result<String, String>;

/// A native function callable from JS, registered via [`Runtime::define_native_function`].
/// A plain function pointer, not an arbitrary closure: this `v8` crate version's `Function::new`
/// requires its callback to be convertible to a bare `extern "C"` function pointer
/// (`impl MapFnTo<FunctionCallback>`), which only non-capturing closures/fn items satisfy.
/// Registering a callback that needs captured state (e.g. a reference to Roves-side data)
/// requires V8's `External`-data mechanism, deliberately not attempted in this phase — see
/// `docs/V8_MIGRATION.md`'s Phase 2 status note for why that's a distinct, harder follow-up.
pub type NativeFunction = fn(&[Value]) -> Value;

/// A read-only accessor's getter, registered via [`Runtime::define_property`]: reads whatever
/// Rust value an instance of the interface it's defined on actually wraps (already
/// downcast-checked the same way [`Runtime::get_wrapped`] is — see this crate's top doc comment
/// on why a raw pointer cast isn't used instead) and converts it to an engine-neutral [`Value`]
/// to return to JS. A plain function pointer, not an arbitrary closure, for the same reason
/// [`NativeFunction`] is: V8's accessor callback machinery requires a non-capturing closure/fn
/// under the hood.
pub type PropertyGetter = fn(&dyn std::any::Any) -> Value;

/// A settable accessor's setter, registered via [`Runtime::define_settable_property`] alongside
/// a [`PropertyGetter`] — mutates whatever Rust value the instance actually wraps, the same way
/// [`PropertyGetter`] reads it (downcast-checked, not a blind cast). Same plain-function-pointer
/// restriction as [`PropertyGetter`]/[`NativeFunction`], for the same reason.
pub type PropertySetter = fn(&mut dyn std::any::Any, &Value);

/// A callable method, registered via [`Runtime::define_method`]: receives the wrapped Rust value
/// of whichever instance it was called on (`node.someMethod()`) plus its JS arguments already
/// converted to [`Value`], and returns a [`Value`]. Same plain-function-pointer restriction as
/// this crate's other callback types, for the same reason.
pub type NativeMethod = fn(&dyn std::any::Any, &[Value]) -> Value;

/// An indexed-property read interceptor. `Some(value)` handles the index (including
/// `Some(Value::Undefined)`); `None` lets normal JS own/prototype lookup continue.
/// This is a primitive for future WebIDL collection bindings, not a complete implementation
/// of their query, enumeration, descriptor, assignment or deletion semantics.
pub type IndexedPropertyGetter = fn(&dyn std::any::Any, u32) -> Option<Value>;

impl Runtime {
    /// Creates a new isolate, initializing the V8 platform first if this is the first
    /// [`Runtime`] in the process.
    pub fn new() -> Self {
        ensure_platform_initialized();
        let mut isolate = v8::Isolate::new(v8::CreateParams::default());
        let context = {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, Default::default());
            v8::Global::new(scope, context)
        };
        Runtime {
            isolate,
            context,
            wrapped_finalizers: Vec::new(),
        }
    }

    /// Compiles and runs `source` as a classic (non-module) script in this [`Runtime`]'s one
    /// persistent context, and returns its final expression's value as a string, or the
    /// uncaught exception's message (covers both a syntax error, which fails at compile, and a
    /// thrown value, which fails at run).
    pub fn eval(&mut self, source: &str) -> ScriptResult {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let try_catch, scope);

        let Some(code) = v8::String::new(try_catch, source) else {
            return Err("source contained invalid UTF-16/UTF-8".to_string());
        };

        let Some(script) = v8::Script::compile(try_catch, code, None) else {
            let message = match try_catch.exception() {
                Some(exception) => exception.to_rust_string_lossy(try_catch),
                None => "unknown script error (no exception object captured)".to_string(),
            };
            return Err(message);
        };

        match script.run(try_catch) {
            Some(value) => Ok(value.to_rust_string_lossy(try_catch)),
            None => {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                Err(message)
            },
        }
    }

    /// Same as [`Runtime::eval`], but returns an engine-neutral [`Value`] instead of a string —
    /// Phase 2's primitive conversion layer.
    pub fn eval_value(&mut self, source: &str) -> Result<Value, String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let try_catch, scope);

        let Some(code) = v8::String::new(try_catch, source) else {
            return Err("source contained invalid UTF-16/UTF-8".to_string());
        };

        let Some(script) = v8::Script::compile(try_catch, code, None) else {
            let message = match try_catch.exception() {
                Some(exception) => exception.to_rust_string_lossy(try_catch),
                None => "unknown script error (no exception object captured)".to_string(),
            };
            return Err(message);
        };

        match script.run(try_catch) {
            Some(value) => Ok(native_value(try_catch, value)),
            None => {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                Err(message)
            },
        }
    }

    /// Same as [`Runtime::eval_value`], but if the result is a `Promise`, drives V8's microtask
    /// queue (`Isolate::perform_microtask_checkpoint`) until it settles and returns the resolved
    /// value, or the rejection's message. A non-`Promise` result returns immediately, same as
    /// `eval_value`. Bounded (see `MAX_MICROTASK_CHECKPOINTS`) so a promise this crate has no
    /// event loop to ever settle (e.g. one waiting on a timer) fails loudly instead of hanging.
    ///
    /// Microtask pumping happens *outside* any handle/context scope: `perform_microtask_checkpoint`
    /// needs `&mut self.isolate` directly, which a live scope already borrows. Each loop
    /// iteration opens and closes its own short-lived scope purely to read the promise's current
    /// state — see this module's own top doc comment on why this file favors small, repeated
    /// scope blocks over trying to share one across an operation like this.
    pub fn eval_resolved(&mut self, source: &str) -> Result<Value, String> {
        enum Outcome {
            Value(Value),
            Promise(v8::Global<v8::Promise>),
        }

        let outcome = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let try_catch, scope);

            let Some(code) = v8::String::new(try_catch, source) else {
                return Err("source contained invalid UTF-16/UTF-8".to_string());
            };
            let Some(script) = v8::Script::compile(try_catch, code, None) else {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown script error (no exception object captured)".to_string(),
                };
                return Err(message);
            };
            match script.run(try_catch) {
                Some(value) => match v8::Local::<v8::Promise>::try_from(value) {
                    Ok(promise) => Outcome::Promise(v8::Global::new(try_catch, promise)),
                    Err(_) => Outcome::Value(native_value(try_catch, value)),
                },
                None => {
                    let message = match try_catch.exception() {
                        Some(exception) => exception.to_rust_string_lossy(try_catch),
                        None => {
                            "unknown script error (no exception object captured)".to_string()
                        },
                    };
                    return Err(message);
                },
            }
        };

        let promise_global = match outcome {
            Outcome::Value(value) => return Ok(value),
            Outcome::Promise(promise) => promise,
        };

        const MAX_MICROTASK_CHECKPOINTS: u32 = 10_000;
        for _ in 0..MAX_MICROTASK_CHECKPOINTS {
            self.isolate.perform_microtask_checkpoint();

            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            let promise = v8::Local::new(scope, &promise_global);
            match promise.state() {
                v8::PromiseState::Pending => continue,
                v8::PromiseState::Fulfilled => {
                    let result = promise.result(scope);
                    return Ok(native_value(scope, result));
                },
                v8::PromiseState::Rejected => {
                    let result = promise.result(scope);
                    return Err(result.to_rust_string_lossy(scope));
                },
            }
        }
        Err("promise did not settle within the microtask pump budget".to_string())
    }

    /// Compiles, instantiates and evaluates `source` as an ES module (as opposed to [`eval`]/
    /// [`eval_value`]'s classic script), returning its completion value or an error.
    ///
    /// Only supports a **self-contained module with no imports** — `import`/dynamic `import()`
    /// resolution is deliberately out of scope for this phase (see
    /// `unreachable_resolve_module_callback`'s own doc comment). This still exercises the real
    /// primitives a later phase's module loader needs: compiling module source distinctly from
    /// a classic script, instantiation (which is where import resolution would normally happen),
    /// and evaluation. Module evaluation always produces a `Promise` under the hood (per the
    /// spec's top-level-await semantics) even when nothing in the module actually awaits
    /// anything, so this pumps microtasks exactly like [`Runtime::eval_resolved`] to get at the
    /// real completion value or propagate a rejection.
    ///
    /// [`eval`]: Runtime::eval
    /// [`eval_value`]: Runtime::eval_value
    pub fn eval_module(&mut self, source: &str) -> Result<Value, String> {
        let promise_value = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let try_catch, scope);

            let Some(code) = v8::String::new(try_catch, source) else {
                return Err("source contained invalid UTF-16/UTF-8".to_string());
            };
            // A module's ScriptOrigin must have `is_module = true` -- unlike a classic script
            // (eval/eval_value/eval_resolved above, where `None` is fine), V8 asserts this at
            // compile time and fatally aborts the whole process (not a catchable exception) if
            // it's missing, found by actually hitting that abort while getting this to work.
            let resource_name = v8::String::new(try_catch, "roves-v8-module")
                .map(Into::into)
                .unwrap_or_else(|| v8::undefined(try_catch).into());
            let origin = v8::ScriptOrigin::new(
                try_catch,
                resource_name,
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                true,
                None,
            );
            let mut compiler_source = v8::script_compiler::Source::new(code, Some(&origin));
            let Some(module) =
                v8::script_compiler::compile_module(try_catch, &mut compiler_source)
            else {
                let message = match try_catch.exception() {
                    Some(exception) => exception.to_rust_string_lossy(try_catch),
                    None => "unknown module compile error".to_string(),
                };
                return Err(message);
            };

            match module.instantiate_module(try_catch, unreachable_resolve_module_callback) {
                Some(true) => {},
                _ => {
                    let message = if module.get_status() == v8::ModuleStatus::Errored {
                        module.get_exception().to_rust_string_lossy(try_catch)
                    } else {
                        "module instantiation failed (does it have an unsupported import?)"
                            .to_string()
                    };
                    return Err(message);
                },
            }

            match module.evaluate(try_catch) {
                Some(value) => v8::Global::new(try_catch, value),
                None => {
                    let message = if module.get_status() == v8::ModuleStatus::Errored {
                        module.get_exception().to_rust_string_lossy(try_catch)
                    } else {
                        let message = match try_catch.exception() {
                            Some(exception) => exception.to_rust_string_lossy(try_catch),
                            None => "unknown module evaluation error".to_string(),
                        };
                        message
                    };
                    return Err(message);
                },
            }
        };

        const MAX_MICROTASK_CHECKPOINTS: u32 = 10_000;
        for _ in 0..MAX_MICROTASK_CHECKPOINTS {
            self.isolate.perform_microtask_checkpoint();

            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);
            let value = v8::Local::new(scope, &promise_value);

            let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) else {
                // Not a Promise at all -- some V8 configurations may complete module
                // evaluation synchronously without wrapping it. Return the value directly.
                return Ok(native_value(scope, value));
            };
            match promise.state() {
                v8::PromiseState::Pending => continue,
                v8::PromiseState::Fulfilled => {
                    let result = promise.result(scope);
                    return Ok(native_value(scope, result));
                },
                v8::PromiseState::Rejected => {
                    let result = promise.result(scope);
                    return Err(result.to_rust_string_lossy(scope));
                },
            }
        }
        Err("module evaluation did not settle within the microtask pump budget".to_string())
    }

    /// Reflects `value` into a fresh JS object, transferring ownership of it to that object's
    /// JS lifetime: `value` is dropped exactly once, when V8 collects the wrapper — never
    /// before, never after — via a *guaranteed* finalizer (`v8::Weak::with_guaranteed_finalizer`,
    /// which the crate itself documents as "guaranteed to be called before the isolate is
    /// destroyed," unlike a regular weak-handle finalizer that only fires "on a best effort
    /// basis"). This is Phase 3's ownership primitive per `docs/V8_MIGRATION.md`: a later
    /// WebIDL-generated DOM reflector (Phase 4) builds its property/method access layer on top
    /// of this same lifecycle; this phase only needs the lifecycle itself to be correct, which
    /// is what its own stress tests check.
    ///
    /// Returns a strong [`Handle`] keeping the object (and so `value`) alive. Drop every
    /// `Handle` (and any other JS-side reference — none exist yet since nothing stores this
    /// object anywhere JS code can reach) to make it eligible for collection.
    pub fn create_wrapped<T: 'static>(&mut self, value: T) -> Handle {
        // Boxed as `Box<dyn Any>` (double-boxed: the inner `Box<dyn Any>` is a fat pointer, and
        // an internal field can only hold a thin one, so `Box::into_raw` is taken of the *outer*
        // box instead — a plain, thin pointer to heap memory holding that fat pointer struct).
        // This is what lets `get_wrapped` check the requested type against the actual one at
        // read-back instead of trusting the caller — see that method's own doc comment.
        let boxed_any: Box<dyn std::any::Any> = Box::new(value);

        let (global_value, raw) = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);

            let template = v8::ObjectTemplate::new(scope);
            template.set_internal_field_count(1);
            let object = template
                .new_instance(scope)
                .expect("a freshly created ObjectTemplate instance should never fail");

            let raw = Box::into_raw(Box::new(boxed_any));
            object.set_aligned_pointer_in_internal_field(
                0,
                raw as *const std::ffi::c_void,
                WRAPPED_POINTER_TAG,
            );
            let object_value: v8::Local<v8::Value> = object.into();
            (v8::Global::new(scope, object_value), raw)
        };

        self.install_guaranteed_finalizer(global_value, raw)
    }

    /// Shared by [`create_wrapped`][Self::create_wrapped] and
    /// [`create_instance`][Self::create_instance]: installs the guaranteed finalizer that drops
    /// `raw` (a `Box<Box<dyn Any>>`, produced identically by both callers) exactly once, when V8
    /// collects `global_value`, and returns the strong [`Handle`] keeping it alive until then.
    fn install_guaranteed_finalizer(
        &mut self,
        global_value: v8::Global<v8::Value>,
        raw: *mut Box<dyn std::any::Any>,
    ) -> Handle {
        let weak = v8::Weak::with_guaranteed_finalizer(
            &mut self.isolate,
            &global_value,
            Box::new(move || {
                // SAFETY: `raw` was created by `Box::into_raw` a few lines above and is reachable
                // from exactly one place afterward (this closure) -- V8 guarantees this finalizer
                // runs at most once, and only after nothing JS-reachable points at the wrapper
                // anymore, so nothing else can read `raw` concurrently or afterward.
                drop(unsafe { Box::from_raw(raw) });
            }),
        );
        self.wrapped_finalizers.push(weak);

        Handle(global_value)
    }

    /// Defines a named JS interface — a constructor function exposed on the global object (e.g.
    /// `window.Node`), backed by a `v8::FunctionTemplate`, optionally inheriting from `parent`'s
    /// prototype chain via `FunctionTemplate::inherit` ("the function's prototype.__proto__ is
    /// set to the parent function's prototype", per that method's own doc comment — exactly the
    /// WebIDL interface-inheritance shape, e.g. `Element` inheriting from `Node`). Instances are
    /// created with [`Runtime::create_instance`], not by calling the constructor from JS (no
    /// constructor body is wired up in this prototype).
    ///
    /// This is the foundational primitive real DOM interfaces are built on that neither
    /// [`create_wrapped`][Self::create_wrapped] nor Phase 3's other primitives provide: JS-visible
    /// interface identity via the prototype chain (`instanceof`), not just Rust-side type
    /// checking via [`Runtime::get_wrapped`]. Prototyped here, inside `roves-v8`, before touching
    /// `components/script_bindings` for real — see `docs/V8_MIGRATION.md`'s Phase 4 status note
    /// on why: the production support modules a real interface needs
    /// (`interface.rs`/`proxyhandler.rs`/`finalize.rs`) are mutually interdependent around
    /// SpiderMonkey's `JSClass`-based object model, so no single one of them can be swapped for a
    /// V8 equivalent in isolation — this prototype is where that V8-side foundation gets designed
    /// and validated first.
    pub fn define_interface(&mut self, name: &str, parent: Option<&Interface>) -> Interface {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        // No real constructor behavior yet -- instances are made via `create_instance`, not by
        // calling this from JS. A no-op body is still required: `FunctionTemplate` has no
        // "callback-less" constructor.
        let template = v8::FunctionTemplate::builder(
            |_scope: &mut v8::PinScope,
             _args: v8::FunctionCallbackArguments,
             _retval: v8::ReturnValue| {},
        )
        .build(scope);
        template.instance_template(scope).set_internal_field_count(1);

        if let Some(parent) = parent {
            let parent_template = v8::Local::new(scope, &parent.template);
            template.inherit(parent_template);
        }

        if let Some(class_name) = v8::String::new(scope, name) {
            template.set_class_name(class_name);
        }

        // Deliberately NOT calling `template.get_function(scope)` here to expose the constructor
        // eagerly -- found by hitting a real bug: `get_function` materializes the actual
        // prototype JS object from `prototype_template`'s *current* contents immediately, and
        // later mutations to `prototype_template` (e.g. a `define_method` call after this
        // `define_interface` call returns) do NOT retroactively update that already-materialized
        // object. Every `define_method` test failed with "is not a function" until this was
        // deferred to `create_instance`, by which point real usage has already finished defining
        // the interface's members. `instance_template`-based accessors don't have this problem
        // (nothing forces an early snapshot of the instance template), which is why the earlier
        // `define_property` checkpoint never hit it.
        let _ = context;

        Interface {
            template: v8::Global::new(scope, template),
            name: name.to_string(),
            constructor_exposed: std::cell::Cell::new(false),
        }
    }

    /// Creates an instance of `interface`, reflecting `value` into it exactly like
    /// [`Runtime::create_wrapped`] (same ownership/finalization lifecycle, same
    /// [`Runtime::get_wrapped`] read-back) — but the instance's `[[Prototype]]` is
    /// `interface`'s prototype object, so `instanceof` and the prototype chain work from JS,
    /// which a plain `create_wrapped` object doesn't have.
    pub fn create_instance<T: 'static>(&mut self, interface: &Interface, value: T) -> Handle {
        let boxed_any: Box<dyn std::any::Any> = Box::new(value);

        let (global_value, raw) = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);

            let template = v8::Local::new(scope, &interface.template);

            // Exposes the constructor on the global object (e.g. `window.Node`), matching how a
            // real DOM interface is JS-visible -- deferred to here, rather than done eagerly in
            // `define_interface`, because `get_function` materializes the actual prototype
            // object from whatever `prototype_template` contains *right now*; doing this in
            // `define_interface` would freeze the prototype before later `define_method`/
            // `define_property` calls for this same interface ever ran. See `define_interface`'s
            // own doc comment for the real bug this was found by. Only done once per interface
            // (`constructor_exposed`) -- harmless to repeat, but pointless after the first time.
            if !interface.constructor_exposed.get() {
                if let (Some(function), Some(key)) =
                    (template.get_function(scope), v8::String::new(scope, &interface.name))
                {
                    let global = context.global(scope);
                    let _ = global.set(scope, key.into(), function.into());
                }
                interface.constructor_exposed.set(true);
            }

            let instance_template = template.instance_template(scope);
            let object = instance_template
                .new_instance(scope)
                .expect("a freshly created ObjectTemplate instance should never fail");

            let raw = Box::into_raw(Box::new(boxed_any));
            object.set_aligned_pointer_in_internal_field(
                0,
                raw as *const std::ffi::c_void,
                WRAPPED_POINTER_TAG,
            );
            let object_value: v8::Local<v8::Value> = object.into();
            (v8::Global::new(scope, object_value), raw)
        };

        self.install_guaranteed_finalizer(global_value, raw)
    }

    /// Defines a read-only accessor property named `name`, computed by calling `getter` on the
    /// wrapped Rust value of whichever instance JS reads it from (e.g. `node.nodeName`), not a
    /// value stored once and left static like [`Runtime::link`]'s plain property — the real
    /// WebIDL "attribute" shape a DOM interface needs.
    ///
    /// Installed on `interface`'s **instance template**, not its prototype template — found by
    /// hitting a real, silent bug: this `v8` crate version has no way to recover the actual
    /// receiver (`this`) inside a property accessor callback, only
    /// [`v8::PropertyCallbackArguments::holder`], which for an accessor defined on a *shared*
    /// prototype object returns that prototype object itself (with no internal field, since only
    /// instances have one), not the instance the property was actually read from — every read
    /// silently returned `undefined` instead of erroring, caught only by the getter tests
    /// expecting a real value and getting `Undefined`. Defining the accessor directly on the
    /// instance template sidesteps this: `holder()` for it is the instance itself, since the
    /// property lives directly on it rather than on a shared prototype object. Despite that, a
    /// property defined this way on a parent interface (via [`Runtime::define_interface`]'s
    /// `parent`) *is* still visible on a child interface's instances — V8's
    /// `FunctionTemplate::inherit` propagates instance-template accessors down the same
    /// inheritance relationship it wires the prototype chain through, confirmed by
    /// `define_property_is_inherited_from_a_parent_interface`'s own test (an earlier version of
    /// that test assumed the opposite and failed, which is how this was actually found rather
    /// than assumed).
    pub fn define_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
    ) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = v8::Local::new(scope, &interface.template);
        let instance_template = template.instance_template(scope);

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a property name string"));
        };
        let external_data = v8::External::new(scope, getter as *mut std::ffi::c_void);

        let configuration = v8::AccessorConfiguration::new(
            |scope: &mut v8::PinScope,
             _key: v8::Local<v8::Name>,
             args: v8::PropertyCallbackArguments,
             mut retval: v8::ReturnValue<v8::Value>| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: `external`'s value is exactly the `PropertyGetter` pointer
                // `define_property` stored below, cast back to its original type.
                let getter: PropertyGetter = unsafe { std::mem::transmute(external.value()) };

                let holder = args.holder();
                if holder.internal_field_count() < 1 {
                    return;
                }
                // SAFETY: same reasoning as `get_wrapped` -- this field is only ever set by
                // `create_wrapped`/`create_instance`, always via
                // `set_aligned_pointer_in_internal_field` with this exact `WRAPPED_POINTER_TAG`.
                let raw = unsafe {
                    holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    return;
                }
                // SAFETY: the instance is `holder` itself, alive for the duration of this
                // callback (V8 guarantees the receiver outlives its own property access), so the
                // guaranteed finalizer that would free `raw` cannot have run yet.
                let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };
                let value = getter(boxed_any.as_ref());
                retval.set(v8_value(scope, &value));
            },
        )
        .data(external_data.into());
        instance_template.set_accessor_with_configuration(key.into(), configuration);
        Ok(())
    }

    /// Same as [`Runtime::define_property`], but also settable from JS — `setter` mutates the
    /// wrapped Rust value in place when JS assigns to the property (e.g. `node.nodeValue = x`).
    /// Both callbacks receive the property's holder the same, already-fixed way (installed on
    /// the interface's instance template — see [`Runtime::define_property`]'s own doc comment on
    /// why a plain prototype-template accessor doesn't work in this `v8` crate version), and
    /// inherit across `define_interface`'s `parent` relationship the same way.
    pub fn define_settable_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: PropertySetter,
    ) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = v8::Local::new(scope, &interface.template);
        let instance_template = template.instance_template(scope);

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a property name string"));
        };
        // AccessorConfiguration has one shared `data` for both callbacks, but each needs its own
        // fn pointer -- bundled as a tuple behind one External instead of two separate ones.
        // Deliberately leaked (`Box::into_raw`, never freed): this allocation is one small,
        // 'static, fixed-size pair of fn pointers per *property definition* (not per instance),
        // so in practice a bounded, small number of these ever exist for the lifetime of the
        // process -- acceptable for this prototype phase, same tradeoff already made for
        // `wrapped_finalizers`' own unbounded `Vec` (see that field's doc comment). A real
        // production version would need a proper registry with the same lifetime as the
        // `Interface` itself instead.
        let callbacks = Box::new((getter, setter));
        let external_data = v8::External::new(scope, Box::into_raw(callbacks) as *mut std::ffi::c_void);

        let configuration = v8::AccessorConfiguration::new(
            |scope: &mut v8::PinScope,
             _key: v8::Local<v8::Name>,
             args: v8::PropertyCallbackArguments,
             mut retval: v8::ReturnValue<v8::Value>| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: `external`'s value is exactly the `Box<(PropertyGetter,
                // PropertySetter)>` pointer stored below, cast back to its original type; the
                // getter callback only ever reads through this shared reference.
                let (getter, _setter) =
                    unsafe { &*(external.value() as *const (PropertyGetter, PropertySetter)) };

                let holder = args.holder();
                if holder.internal_field_count() < 1 {
                    return;
                }
                let raw = unsafe {
                    holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    return;
                }
                let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };
                let value = getter(boxed_any.as_ref());
                retval.set(v8_value(scope, &value));
            },
        )
        .setter(
            |_scope: &mut v8::PinScope,
             _key: v8::Local<v8::Name>,
             new_value: v8::Local<v8::Value>,
             args: v8::PropertyCallbackArguments,
             _retval: v8::ReturnValue<()>| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: same reasoning as the getter closure above.
                let (_getter, setter) =
                    unsafe { &*(external.value() as *const (PropertyGetter, PropertySetter)) };

                let holder = args.holder();
                if holder.internal_field_count() < 1 {
                    return;
                }
                let raw = unsafe {
                    holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    return;
                }
                // SAFETY: `holder` is the sole owner of this internal field's data and this is
                // the only live reference to it for the duration of this callback -- V8 does not
                // re-enter a property setter for the same object concurrently.
                let boxed_any: &mut Box<dyn std::any::Any> = unsafe { &mut *raw };
                let new_value = native_value(_scope, new_value);
                setter(boxed_any.as_mut(), &new_value);
            },
        )
        .data(external_data.into());
        instance_template.set_accessor_with_configuration(key.into(), configuration);
        Ok(())
    }

    /// Defines a callable method named `name` on `interface`'s prototype (e.g.
    /// `node.someMethod(1, 2)`) — installed on the **prototype** template, unlike
    /// [`Runtime::define_property`]/[`Runtime::define_settable_property`]: a regular function
    /// callback's [`v8::FunctionCallbackArguments::this`] gives the actual receiver directly, so
    /// methods don't need the instance-template workaround accessors do (see
    /// [`Runtime::define_property`]'s own doc comment on why that workaround exists at all) — a
    /// real, structural difference between how V8 dispatches property accessors versus function
    /// calls, not a stylistic choice.
    pub fn define_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
    ) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = v8::Local::new(scope, &interface.template);
        let prototype_template = template.prototype_template(scope);

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a method name string"));
        };
        let external_data = v8::External::new(scope, method as *mut std::ffi::c_void);

        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: `external`'s value is exactly the `NativeMethod` pointer
                // `define_method` stored below, cast back to its original type.
                let method: NativeMethod = unsafe { std::mem::transmute(external.value()) };

                let this = args.this();
                if this.internal_field_count() < 1 {
                    return;
                }
                // SAFETY: same reasoning as `get_wrapped`/`define_property` -- this field is
                // only ever set by `create_wrapped`/`create_instance`, always via
                // `set_aligned_pointer_in_internal_field` with this exact `WRAPPED_POINTER_TAG`.
                let raw = unsafe {
                    this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    return;
                }
                let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };

                let mut arguments = Vec::with_capacity(args.length() as usize);
                for i in 0..args.length() {
                    arguments.push(native_value(scope, args.get(i)));
                }
                let result = method(boxed_any.as_ref(), &arguments);
                retval.set(v8_value(scope, &result));
            },
        )
        .data(external_data.into())
        .build(scope);

        let function_value: v8::Local<v8::Data> = function_template.into();
        prototype_template.set(key.into(), function_value);
        Ok(())
    }

    /// Installs an indexed read interceptor on the interface's instance template.
    /// Register before creating instances of this interface or any descendant; materialized
    /// V8 templates cannot be mutated. The guard detects direct instantiation only.
    /// V8 supplies canonical array indices as `u32`; named properties such as `"01"`,
    /// negative numbers and `2**32 - 1` remain normal JS properties. The holder is the
    /// wrapped instance even when lookup starts on an object inheriting from it.
    /// Unlike accessors, V8 does not propagate this handler through FunctionTemplate::inherit;
    /// register it explicitly on each derived interface that needs indexed reads.
    /// Only reads are intercepted: this intentionally does not promise full WebIDL
    /// legacy-platform-object semantics or return object wrappers through `Value::Object`.
    pub fn define_indexed_property_getter(
        &mut self,
        interface: &Interface,
        getter: IndexedPropertyGetter,
    ) -> Result<(), String> {
        if interface.constructor_exposed.get() {
            return Err("indexed getter must be defined before creating instances".to_string());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let external_data = v8::External::new(scope, getter as *mut std::ffi::c_void);
        let configuration = v8::IndexedPropertyHandlerConfiguration::new()
            .getter(
                |scope: &mut v8::PinScope,
                 index: u32,
                 args: v8::PropertyCallbackArguments,
                 mut retval: v8::ReturnValue<v8::Value>| {
                    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else {
                        return v8::Intercepted::kNo;
                    };
                    // SAFETY: this External contains exactly the function pointer supplied
                    // above, which remains valid for the lifetime of the template.
                    let getter: IndexedPropertyGetter =
                        unsafe { std::mem::transmute(external.value()) };
                    let holder = args.holder();
                    if holder.internal_field_count() < 1 {
                        return v8::Intercepted::kNo;
                    }
                    // SAFETY: only create_instance installs this field, using this tag and
                    // a Box<Box<dyn Any>>. The local holder keeps its finalizer from running.
                    let raw = unsafe {
                        holder.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                    } as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        return v8::Intercepted::kNo;
                    }
                    let boxed_any = unsafe { &*raw };
                    match getter(boxed_any.as_ref(), index) {
                        Some(value) => {
                            retval.set(v8_value(scope, &value));
                            v8::Intercepted::kYes
                        },
                        None => v8::Intercepted::kNo,
                    }
                },
            )
            .data(external_data.into());
        template
            .instance_template(scope)
            .set_indexed_property_handler(configuration);
        Ok(())
    }

    /// Reads back the Rust value a live [`Handle`] from [`Runtime::create_wrapped`] points at,
    /// checking that it's actually a `T` (not just trusting the caller) via `Any::downcast_ref`
    /// on the same `Box<dyn Any>` `create_wrapped` stored — returns `None` on a type mismatch
    /// instead of the memory-unsafe behavior a naive raw-pointer cast would have. Also `None` if
    /// `handle` doesn't point at a `create_wrapped`-created object at all (e.g. a plain `Value`
    /// from [`Runtime::store`]).
    ///
    /// This is only the JS-to-Rust half of wrapper identity — nothing here yet ensures wrapping
    /// the *same* conceptual value twice reuses the first wrapper instead of creating a second,
    /// independent one; see `docs/V8_MIGRATION.md`'s Phase 3 status note.
    ///
    /// # Safety requirement this relies on
    ///
    /// The returned reference borrows from `handle`, not from `self` alone — passing a `handle`
    /// that isn't actually keeping the object alive (there is no such way to construct one
    /// outside this crate) would be unsound; a live `Handle` argument is what guarantees the
    /// finalizer in `create_wrapped` hasn't run yet.
    pub fn get_wrapped<'h, T: 'static>(&mut self, handle: &'h Handle) -> Option<&'h T> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let value = v8::Local::new(scope, &handle.0);
        let object = v8::Local::<v8::Object>::try_from(value).ok()?;
        if object.internal_field_count() < 1 {
            return None;
        }
        // SAFETY: this field is only ever set by `create_wrapped`, always via
        // `set_aligned_pointer_in_internal_field` with this exact `WRAPPED_POINTER_TAG`, so a
        // non-null result always points at a live `Box<Box<dyn Any>>` this same crate allocated.
        let raw = unsafe {
            object.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
        } as *mut Box<dyn std::any::Any>;
        if raw.is_null() {
            return None;
        }
        // SAFETY: `raw` is non-null and was produced by `create_wrapped` as described above;
        // `handle` being alive (a live `Global`, per this method's own doc comment) guarantees
        // the guaranteed finalizer that would free it hasn't run.
        let boxed_any: &Box<dyn std::any::Any> = unsafe { &*raw };
        // Re-borrowed through a raw pointer to extend the lifetime to `'h`: `downcast_ref`'s
        // natural return type borrows from the local `&*raw` above, not from `handle`, but the
        // data it points to genuinely lives as long as `handle` does (same safety argument as
        // the `unsafe` block above), so this is sound, not just a way to dodge the borrow
        // checker.
        boxed_any
            .downcast_ref::<T>()
            .map(|reference| unsafe { &*(reference as *const T) })
    }

    /// Sets `property` on the JS object `on` points at to the JS value `other` points at.
    /// Exists to build real reference graphs between wrapped (or plain) JS values — in
    /// particular, this crate's own cycle stress test uses it to make two `create_wrapped`
    /// objects reference each other, the same shape a real DOM has everywhere (a parent
    /// referencing a child that references its parent back, an event listener closing over a
    /// node, ...). Deliberately a plain JS property, not an extra `v8::Global` cross-reference
    /// on the Rust side: V8's own tracing GC already collects cycles among ordinary JS object
    /// graphs correctly (that's the entire point of tracing over reference counting) — the only
    /// way this phase's ownership model could still leak a cycle is by adding a *Rust-side*
    /// strong reference on top, which this method doesn't.
    pub fn link(&mut self, on: &Handle, property: &str, other: &Handle) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let on_value = v8::Local::new(scope, &on.0);
        let Ok(object) = v8::Local::<v8::Object>::try_from(on_value) else {
            return Err("`on` does not point at a JS object".to_string());
        };
        let Some(key) = v8::String::new(scope, property) else {
            return Err(format!("{property:?} is not valid as a property name string"));
        };
        let other_value = v8::Local::new(scope, &other.0);
        if object.set(scope, key.into(), other_value) != Some(true) {
            return Err(format!("failed to set property {property:?}"));
        }
        Ok(())
    }

    /// Exposes `handle`'s value as a named property of this [`Runtime`]'s global object —
    /// e.g. `set_global_property("window", handle)` makes `handle`'s value reachable from
    /// script as `globalThis.window`/bare `window`. This is Phase 4's first validation
    /// milestone per `docs/V8_MIGRATION.md` ("global/window exposure"), prototyped here as a
    /// temporary experiment entirely within `roves-v8` — the plan document explicitly allows
    /// "manually implementing DOM APIs one by one... as temporary experiments used to validate
    /// the runtime" — rather than by touching `components/script_bindings/codegen.py` or any
    /// other production Servo file, which stays completely untouched by this and everything else
    /// in this crate so far.
    pub fn set_global_property(&mut self, name: &str, handle: &Handle) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let global = context.global(scope);
        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a global property name string"));
        };
        let value = v8::Local::new(scope, &handle.0);
        if global.set(scope, key.into(), value) != Some(true) {
            return Err(format!("failed to set global property {name:?}"));
        }
        Ok(())
    }

    /// Forces a full garbage collection cycle — test-only (see `ensure_platform_initialized`'s
    /// own comment on why `--expose-gc` isn't set outside `#[cfg(test)]`). Exists so this
    /// crate's own GC stress tests don't have to rely on GC happening to run on its own schedule
    /// within a test's short lifetime.
    #[cfg(test)]
    fn force_full_gc_for_testing(&mut self) {
        self.isolate
            .request_garbage_collection_for_testing(v8::GarbageCollectionType::Full);
    }

    /// Stores `value` as a JS value in this isolate and returns a [`Handle`] that outlives this
    /// call — see [`Handle`]'s own doc comment.
    pub fn store(&mut self, value: &Value) -> Handle {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let local = v8_value(scope, value);
        Handle(v8::Global::new(scope, local))
    }

    /// Reads a [`Handle`] back into an engine-neutral [`Value`]. A `Handle` is backed by a
    /// `v8::Global`, which is isolate- not context-scoped, so this works even though it enters
    /// this [`Runtime`]'s one persistent context rather than whatever context was active when
    /// [`Runtime::store`] created it — see [`Handle`]'s own doc comment.
    pub fn load(&mut self, handle: &Handle) -> Value {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let local = v8::Local::new(scope, &handle.0);
        native_value(scope, local)
    }

    /// Registers `f` as a global function named `name`, callable from a later [`Runtime::eval`]/
    /// `eval_value` call. Demonstrates this phase's "callbacks" primitive: arguments and the
    /// return value both round-trip through [`Value`], not raw `v8::*` types. See
    /// [`NativeFunction`]'s own doc comment for why `f` must be a plain function pointer, not an
    /// arbitrary closure.
    pub fn define_native_function(&mut self, name: &str, f: NativeFunction) -> Result<(), String> {
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        // `f` (a fn pointer) can't be captured by a `v8::Function::new` closure argument
        // directly and still satisfy `MapFnTo` (which requires the closure itself to carry no
        // captured state) unless it's threaded through as V8 "callback data" instead — a
        // `v8::External` wrapping the raw pointer, read back inside the (capture-free) closure.
        let external_data = v8::External::new(scope, f as *mut std::ffi::c_void);
        let Some(function) = v8::Function::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: `external`'s value is exactly the `NativeFunction` pointer this same
                // `define_native_function` call stored a few lines above, cast back to its
                // original type. Nothing else ever constructs this External.
                let f: NativeFunction =
                    unsafe { std::mem::transmute(external.value()) };

                let mut arguments = Vec::with_capacity(args.length() as usize);
                for i in 0..args.length() {
                    arguments.push(native_value(scope, args.get(i)));
                }
                let result = f(&arguments);
                retval.set(v8_value(scope, &result));
            },
        )
        .data(external_data.into())
        .build(scope) else {
            return Err(format!("failed to create native function {name:?}"));
        };

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a JS identifier string"));
        };
        let global = context.global(scope);
        if global.set(scope, key.into(), function.into()) != Some(true) {
            return Err(format!("failed to install {name:?} on the global object"));
        }
        Ok(())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Passed to `Module::instantiate_module` by [`Runtime::eval_module`], which only supports a
/// self-contained module with no imports (see that method's own doc comment on why) — so this
/// should never actually run. Written as an ordinary Rust fn with the "logical" signature the
/// `v8` crate's own doc comment on `ResolveModuleCallback` describes; despite that type's raw
/// form being an `unsafe extern "C" fn` with a platform-specific ABI (an extra leading out-param
/// on Windows), the crate's `MapFnTo`/`MapFnFrom` machinery generates that wrapper automatically
/// from a plain closure/fn like this one — writing the raw ABI by hand isn't necessary and (as
/// found while getting this to compile) isn't even accepted where a `MapFnTo` bound is expected.
fn unreachable_resolve_module_callback<'s>(
    _context: v8::Local<'s, v8::Context>,
    _specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    unreachable!(
        "roves-v8's eval_module only supports import-free modules in this phase; \
         see docs/V8_MIGRATION.md's Phase 2 status note"
    )
}

/// Converts a `v8::Local<Value>` into this crate's engine-neutral [`Value`] — see [`Value`]'s
/// own doc comment on the `Object`/`Bytes` variants' scope.
fn native_value<'s>(scope: &v8::PinScope<'s, '_>, value: v8::Local<'s, v8::Value>) -> Value {
    if value.is_undefined() {
        Value::Undefined
    } else if value.is_null() {
        Value::Null
    } else if value.is_boolean() {
        Value::Bool(value.boolean_value(scope))
    } else if value.is_number() {
        Value::Number(value.number_value(scope).unwrap_or(f64::NAN))
    } else if value.is_string() {
        Value::String(value.to_rust_string_lossy(scope))
    } else if value.is_uint8_array() {
        let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(value) else {
            return Value::Object;
        };
        let len = view.byte_length();
        let mut bytes = vec![0u8; len];
        let copied = view.copy_contents(&mut bytes);
        bytes.truncate(copied);
        Value::Bytes(bytes)
    } else {
        Value::Object
    }
}

/// Converts this crate's engine-neutral [`Value`] into a `v8::Local<Value>` — the inverse of
/// [`native_value`]. `Value::Object` has no data to reconstruct an object from and becomes
/// `undefined`; see [`Value`]'s own doc comment.
fn v8_value<'s>(scope: &v8::PinScope<'s, '_>, value: &Value) -> v8::Local<'s, v8::Value> {
    match value {
        Value::Undefined | Value::Object => v8::undefined(scope).into(),
        Value::Null => v8::null(scope).into(),
        Value::Bool(b) => v8::Boolean::new(scope, *b).into(),
        Value::Number(n) => v8::Number::new(scope, *n).into(),
        Value::String(s) => v8::String::new(scope, s)
            .map(Into::into)
            .unwrap_or_else(|| v8::undefined(scope).into()),
        Value::Bytes(bytes) => {
            let buffer = v8::ArrayBuffer::new(scope, bytes.len());
            // SAFETY: `buffer` was just created above with exactly `bytes.len()` bytes backing
            // it, and nothing else holds a reference to it yet.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    buffer.data().unwrap().as_ptr() as *mut u8,
                    bytes.len(),
                );
            }
            match v8::Uint8Array::new(scope, buffer, 0, bytes.len()) {
                Some(array) => array.into(),
                None => v8::undefined(scope).into(),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{Handle, NativeFunction, Runtime, Value};

    /// Increments a shared counter when dropped — used by the `create_wrapped` GC stress tests
    /// below to observe exactly how many wrapped values actually got dropped, and how many
    /// times each (a double-drop would show up as a count too high, not just "ran").
    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn evaluates_a_basic_expression() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.eval("1 + 2").unwrap(), "3");
    }

    #[test]
    fn evaluates_string_concatenation() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval("'hello' + ' ' + 'world'").unwrap(),
            "hello world"
        );
    }

    #[test]
    fn reports_a_thrown_exception() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval("throw new Error('roves-v8 smoke test')")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 smoke test"),
            "expected the thrown message in the error, got: {err}"
        );
    }

    #[test]
    fn reports_a_syntax_error() {
        let mut runtime = Runtime::new();
        assert!(runtime.eval("this is not valid javascript (((").is_err());
    }

    #[test]
    fn a_second_runtime_after_the_first_is_dropped_still_works() {
        // Exercises `ensure_platform_initialized`'s Once guard: a second Runtime later in the
        // same process must not re-initialize the platform, and must still work correctly.
        //
        // Deliberately sequential, not concurrent: V8 only allows one isolate "entered" on a
        // given thread at a time (see this crate's own `isolate.rs` doc comment on
        // `OwnedIsolate` — the Locker/Unlocker API is required to hold more than one alive
        // across threads, which this crate doesn't attempt in Phase 1). Two `Runtime`s alive at
        // once on one thread is not a real Roves usage pattern anyway: production Roves runs
        // exactly one game/isolate per process.
        let mut a = Runtime::new();
        assert_eq!(a.eval("21 * 2").unwrap(), "42");
        drop(a);

        let mut b = Runtime::new();
        assert_eq!(b.eval("'still' + 'works'").unwrap(), "stillworks");
    }

    #[test]
    fn eval_value_converts_primitives() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.eval_value("undefined").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval_value("null").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("true").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("1 + 2").unwrap(), Value::Number(3.0));
        assert_eq!(
            runtime.eval_value("'hi'").unwrap(),
            Value::String("hi".to_string())
        );
        assert_eq!(runtime.eval_value("({})").unwrap(), Value::Object);
    }

    #[test]
    fn eval_value_converts_uint8_array_to_bytes() {
        let mut runtime = Runtime::new();
        let value = runtime
            .eval_value("new Uint8Array([1, 2, 3, 255])")
            .unwrap();
        assert_eq!(value, Value::Bytes(vec![1, 2, 3, 255]));
    }

    #[test]
    fn handle_round_trips_a_value_across_calls() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Number(42.0));
        // A fresh, unrelated eval call happens between store and load, proving the handle
        // doesn't depend on the context it was created in.
        assert_eq!(runtime.eval("1 + 1").unwrap(), "2");
        assert_eq!(runtime.load(&handle), Value::Number(42.0));
    }

    #[test]
    fn handle_round_trips_bytes() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Bytes(vec![9, 8, 7]));
        assert_eq!(runtime.load(&handle), Value::Bytes(vec![9, 8, 7]));
    }

    #[test]
    fn native_function_is_callable_from_js() {
        let double: NativeFunction = |args| match args.first() {
            Some(Value::Number(n)) => Value::Number(n * 2.0),
            _ => Value::Undefined,
        };

        let mut runtime = Runtime::new();
        runtime.define_native_function("double", double).unwrap();
        assert_eq!(runtime.eval("double(21)").unwrap(), "42");
    }

    #[test]
    fn eval_resolved_passes_through_a_non_promise_value() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval_resolved("1 + 2").unwrap(),
            Value::Number(3.0)
        );
    }

    #[test]
    fn eval_resolved_returns_an_already_resolved_promise() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.eval_resolved("Promise.resolve(42)").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn eval_resolved_pumps_microtasks_for_a_chained_then() {
        // Unlike Promise.resolve(42) (already fulfilled the instant it's created), this
        // promise only settles once its .then callback actually runs as a microtask -- this
        // is the case that needs the perform_microtask_checkpoint loop, not just a state read.
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime
                .eval_resolved("Promise.resolve(1).then(v => v + 1)")
                .unwrap(),
            Value::Number(2.0)
        );
    }

    #[test]
    fn eval_resolved_reports_a_rejected_promise() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval_resolved("Promise.reject(new Error('roves-v8 promise rejection'))")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 promise rejection"),
            "expected the rejection message in the error, got: {err}"
        );
    }

    #[test]
    fn eval_module_runs_a_self_contained_module() {
        let mut runtime = Runtime::new();
        runtime
            .eval_module("globalThis.moduleRan = 42;")
            .unwrap();
        assert_eq!(
            runtime.eval_value("globalThis.moduleRan").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn eval_module_reports_a_thrown_exception() {
        let mut runtime = Runtime::new();
        let err = runtime
            .eval_module("throw new Error('roves-v8 module error')")
            .unwrap_err();
        assert!(
            err.contains("roves-v8 module error"),
            "expected the thrown message in the error, got: {err}"
        );
    }

    #[test]
    fn eval_module_reports_a_syntax_error() {
        let mut runtime = Runtime::new();
        assert!(
            runtime
                .eval_module("this is not valid javascript (((")
                .is_err()
        );
    }

    #[test]
    fn create_wrapped_keeps_the_value_alive_while_the_handle_lives() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = runtime.create_wrapped(DropCounter(counter.clone()));
        // A GC right now must not collect it -- the returned Handle is still a live, strong
        // reference.
        runtime.force_full_gc_for_testing();
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        drop(handle);
    }

    #[test]
    fn create_wrapped_drops_the_value_exactly_once_when_collected() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = runtime.create_wrapped(DropCounter(counter.clone()));
        drop(handle); // the only strong reference -- now eligible for collection

        let mut collected = false;
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) > 0 {
                collected = true;
                break;
            }
        }
        assert!(collected, "value was never collected within 20 full GC cycles");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "value must be dropped exactly once, not zero or more than once"
        );
    }

    #[test]
    fn create_wrapped_stress_many_objects_all_get_collected_exactly_once() {
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        const COUNT: usize = 200;
        {
            let handles: Vec<Handle> = (0..COUNT)
                .map(|_| runtime.create_wrapped(DropCounter(counter.clone())))
                .collect();
            assert_eq!(counter.load(Ordering::SeqCst), 0);
            drop(handles);
        }

        for _ in 0..30 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) as usize == COUNT {
                break;
            }
        }
        assert_eq!(
            counter.load(Ordering::SeqCst) as usize,
            COUNT,
            "every one of {COUNT} wrapped objects should eventually be collected exactly once"
        );
    }

    #[test]
    fn create_wrapped_of_different_types_coexist_independently() {
        // No read-back API exists yet (deliberately Phase 4/WebIDL-bindings scope, see
        // create_wrapped's own doc comment) -- this only exercises that wrapping unrelated
        // types in the same Runtime doesn't panic or corrupt each other's finalizer.
        let mut runtime = Runtime::new();
        let int_counter = Arc::new(AtomicUsize::new(0));
        let string_counter = Arc::new(AtomicUsize::new(0));

        struct TaggedDrop(Arc<AtomicUsize>);
        impl Drop for TaggedDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let a = runtime.create_wrapped((42i32, TaggedDrop(int_counter.clone())));
        let b = runtime.create_wrapped(("hello".to_string(), TaggedDrop(string_counter.clone())));
        drop(a);
        drop(b);

        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if int_counter.load(Ordering::SeqCst) == 1 && string_counter.load(Ordering::SeqCst) == 1
            {
                break;
            }
        }
        assert_eq!(int_counter.load(Ordering::SeqCst), 1);
        assert_eq!(string_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn get_wrapped_reads_back_the_right_value() {
        let mut runtime = Runtime::new();
        let handle = runtime.create_wrapped(42i32);
        assert_eq!(runtime.get_wrapped::<i32>(&handle), Some(&42));
    }

    #[test]
    fn get_wrapped_rejects_a_type_mismatch() {
        let mut runtime = Runtime::new();
        let handle = runtime.create_wrapped(42i32);
        // Asking for the wrong type must return None, not read 4 bytes of an i32 as if they
        // were something else -- this is exactly what the Box<dyn Any> + downcast_ref check
        // in get_wrapped exists to prevent.
        assert_eq!(runtime.get_wrapped::<String>(&handle), None);
    }

    #[test]
    fn get_wrapped_returns_none_for_a_non_wrapped_handle() {
        let mut runtime = Runtime::new();
        let handle = runtime.store(&Value::Number(42.0));
        assert_eq!(runtime.get_wrapped::<i32>(&handle), None);
    }

    #[test]
    fn get_wrapped_distinguishes_two_different_wrapped_objects() {
        let mut runtime = Runtime::new();
        let a = runtime.create_wrapped(1i32);
        let b = runtime.create_wrapped(2i32);
        assert_eq!(runtime.get_wrapped::<i32>(&a), Some(&1));
        assert_eq!(runtime.get_wrapped::<i32>(&b), Some(&2));
    }

    #[test]
    fn wrapped_objects_referencing_each_other_do_not_leak_the_cycle() {
        // A naive design where cross-references between wrapped objects are held as extra
        // v8::Global handles on the Rust side would leak this forever (each side's strong
        // reference keeps the other alive, even with no external reference at all). `link`
        // deliberately uses a plain JS property instead -- this test proves that choice
        // actually avoids the leak, not just that it's the design.
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let a = runtime.create_wrapped(DropCounter(counter.clone()));
        let b = runtime.create_wrapped(DropCounter(counter.clone()));
        runtime.link(&a, "other", &b).unwrap();
        runtime.link(&b, "other", &a).unwrap();
        drop(a);
        drop(b);

        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) == 2 {
                break;
            }
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "both sides of the reference cycle should be collected, not leaked"
        );
    }

    #[test]
    fn set_global_property_exposes_a_wrapped_object_as_window() {
        // Phase 4's first validation milestone ("global/window exposure", see
        // docs/V8_MIGRATION.md) -- a Rust-backed object reachable from script as `window`, with
        // a property on it also readable, built entirely from this crate's own Phase 2/3
        // primitives (create_wrapped, link, set_global_property), no codegen involved.
        struct FakeWindow;

        let mut runtime = Runtime::new();
        let window = runtime.create_wrapped(FakeWindow);
        let name = runtime.store(&Value::String("Roves".to_string()));
        runtime.link(&window, "name", &name).unwrap();
        runtime.set_global_property("window", &window).unwrap();

        assert_eq!(
            runtime.eval_value("window.name").unwrap(),
            Value::String("Roves".to_string())
        );
        assert_eq!(
            runtime.eval_value("typeof window").unwrap(),
            Value::String("object".to_string())
        );
    }

    #[test]
    fn define_interface_exposes_a_constructor_on_the_global_object() {
        // The constructor is only actually exposed on the global object once the first
        // instance is created (see create_instance's own doc comment on why: exposing it
        // eagerly in define_interface would freeze the prototype before any later
        // define_method/define_property calls for this interface ever ran) -- so this test
        // creates one, even though it doesn't otherwise need it, purely to trigger that.
        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime.create_instance(&node_interface, Node);
        assert_eq!(
            runtime.eval_value("typeof Node").unwrap(),
            Value::String("function".to_string())
        );
    }

    #[test]
    fn create_instance_has_the_interfaces_prototype() {
        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let node = runtime.create_instance(&node_interface, Node);
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node instanceof Node").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn create_instance_participates_in_prototype_inheritance() {
        // The real WebIDL shape this prototypes: `Element` inheriting from `Node`, e.g.
        // `document.createElement(...) instanceof Node` must be true, not just
        // `instanceof Element`.
        struct Node;
        struct Element;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        // `Node`'s own constructor is only exposed on the global object at its own first
        // create_instance call (see create_instance's own doc comment on why that's deferred) --
        // this throwaway instance exists purely so the JS-side `instanceof Node` check below has
        // a `Node` identifier to resolve at all.
        runtime.create_instance(&node_interface, Node);
        let element = runtime.create_instance(&element_interface, Element);
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element instanceof Element").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true),
            "an Element instance must also be a Node instance, via the prototype chain"
        );
    }

    #[test]
    fn create_instance_is_not_an_instance_of_an_unrelated_interface() {
        struct Node;
        struct Event;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let event_interface = runtime.define_interface("Event", None);
        let node = runtime.create_instance(&node_interface, Node);
        runtime.set_global_property("node", &node).unwrap();
        // Throwaway instance purely to expose `Event` on the global object -- see the sibling
        // test's own comment on why.
        runtime.create_instance(&event_interface, Event);

        assert_eq!(
            runtime.eval_value("node instanceof Event").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn create_instance_still_supports_typed_read_back_and_gc() {
        // create_instance must keep everything create_wrapped already provides: typed
        // get_wrapped read-back and guaranteed-finalizer GC cleanup -- gaining a prototype
        // chain shouldn't cost either.
        let mut runtime = Runtime::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let node_interface = runtime.define_interface("Node", None);

        let node = runtime.create_instance(&node_interface, (7i32, DropCounter(counter.clone())));
        assert_eq!(runtime.get_wrapped::<(i32, DropCounter)>(&node).unwrap().0, 7);

        drop(node);
        for _ in 0..20 {
            runtime.force_full_gc_for_testing();
            if counter.load(Ordering::SeqCst) == 1 {
                break;
            }
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn define_property_computes_a_live_getter_from_wrapped_data() {
        // The real WebIDL "attribute" shape: node.nodeName reads Rust-side state live, not a
        // snapshot stored once like `link`'s plain property.
        struct Node {
            name: String,
        }

        fn node_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "nodeName", node_name)
            .unwrap();

        let node = runtime.create_instance(
            &node_interface,
            Node {
                name: "DIV".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.nodeName").unwrap(),
            Value::String("DIV".to_string())
        );
    }

    #[test]
    fn define_property_is_shared_by_every_instance_of_the_interface() {
        struct Node {
            name: String,
        }
        fn node_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "nodeName", node_name)
            .unwrap();

        let a = runtime.create_instance(
            &node_interface,
            Node {
                name: "DIV".to_string(),
            },
        );
        let b = runtime.create_instance(
            &node_interface,
            Node {
                name: "SPAN".to_string(),
            },
        );
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();

        assert_eq!(
            runtime.eval_value("a.nodeName").unwrap(),
            Value::String("DIV".to_string())
        );
        assert_eq!(
            runtime.eval_value("b.nodeName").unwrap(),
            Value::String("SPAN".to_string())
        );
    }

    #[test]
    fn define_property_on_its_own_interface_works_alongside_instanceof() {
        struct Node;
        struct Element {
            tag: String,
        }
        fn tag_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(element.tag.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        runtime
            .define_property(&element_interface, "tagName", tag_name)
            .unwrap();
        // Throwaway instance purely to expose `Node` on the global object for the `instanceof`
        // check below -- see create_instance's own doc comment on why exposure is per-interface
        // and deferred to first instantiation.
        runtime.create_instance(&node_interface, Node);

        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element.tagName").unwrap(),
            Value::String("DIV".to_string())
        );
        // instanceof still sees Element's inheritance from Node (that's prototype-chain-based,
        // unaffected by the instance-template limitation below).
        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn define_property_is_inherited_from_a_parent_interface() {
        // A genuine surprise found by testing rather than assuming (the first version of this
        // test asserted the opposite and failed): despite accessors being installed on the
        // *instance* template rather than the prototype template (see define_property's own doc
        // comment on why), V8's `FunctionTemplate::inherit` propagates instance-template
        // accessors down to child interfaces too, not just the prototype chain `instanceof`
        // relies on. A property defined on a parent interface (`Node`) is visible on a child
        // interface's instances (`Element`) -- and, checked here with a getter that actually
        // downcasts rather than a constant, reads that specific instance's own wrapped data
        // correctly, not stale or wrong data from elsewhere.
        struct Element {
            tag: String,
        }
        fn tag_name(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(element.tag.clone()),
                None => Value::Undefined,
            }
        }

        struct Node;
        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_property(&node_interface, "tagName", tag_name)
            .unwrap();
        // Throwaway instance purely to expose `Node` on the global object for the `instanceof`
        // check below -- see create_instance's own doc comment on why exposure is per-interface
        // and deferred to first instantiation.
        runtime.create_instance(&node_interface, Node);
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        assert_eq!(
            runtime.eval_value("element instanceof Node").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            runtime.eval_value("element.tagName").unwrap(),
            Value::String("DIV".to_string())
        );
    }

    #[test]
    fn define_settable_property_reads_the_initial_value() {
        struct Node {
            value: String,
        }
        fn get_value(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.value.clone()),
                None => Value::Undefined,
            }
        }
        fn set_value(this: &mut dyn std::any::Any, new_value: &Value) {
            if let (Some(node), Value::String(s)) = (this.downcast_mut::<Node>(), new_value) {
                node.value = s.clone();
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_settable_property(&node_interface, "nodeValue", get_value, set_value)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                value: "hello".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.nodeValue").unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn define_settable_property_mutates_the_wrapped_value() {
        struct Node {
            value: String,
        }
        fn get_value(this: &dyn std::any::Any) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.value.clone()),
                None => Value::Undefined,
            }
        }
        fn set_value(this: &mut dyn std::any::Any, new_value: &Value) {
            if let (Some(node), Value::String(s)) = (this.downcast_mut::<Node>(), new_value) {
                node.value = s.clone();
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_settable_property(&node_interface, "nodeValue", get_value, set_value)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                value: "hello".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        // Assigning from JS must both take effect (read back via a later access) and be
        // reflected on the Rust side (read back via get_wrapped), proving the setter actually
        // mutated the same wrapped value the getter/get_wrapped read from, not a copy.
        runtime.eval("node.nodeValue = 'world'").unwrap();
        assert_eq!(
            runtime.eval_value("node.nodeValue").unwrap(),
            Value::String("world".to_string())
        );
        assert_eq!(runtime.get_wrapped::<Node>(&node).unwrap().value, "world");
    }

    #[test]
    fn define_method_is_callable_with_arguments_from_js() {
        struct Node {
            name: String,
        }
        fn greet(this: &dyn std::any::Any, args: &[Value]) -> Value {
            let node = match this.downcast_ref::<Node>() {
                Some(node) => node,
                None => return Value::Undefined,
            };
            let greeting = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => "Hello".to_string(),
            };
            Value::String(format!("{greeting}, {}!", node.name))
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "greet", greet)
            .unwrap();
        let node = runtime.create_instance(
            &node_interface,
            Node {
                name: "World".to_string(),
            },
        );
        runtime.set_global_property("node", &node).unwrap();

        assert_eq!(
            runtime.eval_value("node.greet('Hi')").unwrap(),
            Value::String("Hi, World!".to_string())
        );
        assert_eq!(
            runtime.eval_value("node.greet()").unwrap(),
            Value::String("Hello, World!".to_string())
        );
    }

    #[test]
    fn define_method_is_inherited_from_a_parent_interface() {
        struct Element {
            tag: String,
        }
        fn describe(this: &dyn std::any::Any, _args: &[Value]) -> Value {
            match this.downcast_ref::<Element>() {
                Some(element) => Value::String(format!("<{}>", element.tag)),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "describe", describe)
            .unwrap();
        let element_interface = runtime.define_interface("Element", Some(&node_interface));
        let element = runtime.create_instance(
            &element_interface,
            Element {
                tag: "DIV".to_string(),
            },
        );
        runtime.set_global_property("element", &element).unwrap();

        // Methods live on the prototype template, so they inherit via the normal JS prototype
        // chain -- no surprise here the way there was for instance-template-based accessors.
        assert_eq!(
            runtime.eval_value("element.describe()").unwrap(),
            Value::String("<DIV>".to_string())
        );
    }

    #[test]
    fn define_method_is_shared_by_every_instance_of_the_interface() {
        struct Node {
            name: String,
        }
        fn get_name(this: &dyn std::any::Any, _args: &[Value]) -> Value {
            match this.downcast_ref::<Node>() {
                Some(node) => Value::String(node.name.clone()),
                None => Value::Undefined,
            }
        }

        let mut runtime = Runtime::new();
        let node_interface = runtime.define_interface("Node", None);
        runtime
            .define_method(&node_interface, "getName", get_name)
            .unwrap();
        let a = runtime.create_instance(
            &node_interface,
            Node {
                name: "A".to_string(),
            },
        );
        let b = runtime.create_instance(
            &node_interface,
            Node {
                name: "B".to_string(),
            },
        );
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();

        assert_eq!(
            runtime.eval_value("a.getName()").unwrap(),
            Value::String("A".to_string())
        );
        assert_eq!(
            runtime.eval_value("b.getName()").unwrap(),
            Value::String("B".to_string())
        );
    }

    fn collection_getter(this: &dyn std::any::Any, index: u32) -> Option<Value> {
        this.downcast_ref::<Vec<Value>>()?
            .get(index as usize)
            .cloned()
    }

    #[test]
    fn indexed_getter_reads_each_instances_native_data() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&interface, collection_getter)
            .unwrap();
        let a = runtime.create_instance(&interface, vec![Value::String("A".into())]);
        let b = runtime.create_instance(&interface, vec![Value::String("B".into())]);
        runtime.set_global_property("a", &a).unwrap();
        runtime.set_global_property("b", &b).unwrap();
        assert_eq!(runtime.eval("a[0] + b['0']").unwrap(), "AB");
        assert_eq!(runtime.eval_value("a[1]").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval("a instanceof Collection").unwrap(), "true");
    }

    #[test]
    fn indexed_getter_distinguishes_undefined_from_unhandled_lookup() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&interface, collection_getter)
            .unwrap();
        let collection = runtime.create_instance(&interface, vec![Value::Undefined]);
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        runtime
            .eval("Collection.prototype[0] = 'masked'; Collection.prototype[1] = 'fallback'")
            .unwrap();
        assert_eq!(
            runtime.eval_value("collection[0]").unwrap(),
            Value::Undefined
        );
        assert_eq!(runtime.eval("collection[1]").unwrap(), "fallback");
        assert_eq!(
            runtime.eval("Object.create(collection)[1]").unwrap(),
            "fallback"
        );
        runtime.eval("collection[2] = 'own'").unwrap();
        assert_eq!(runtime.eval("collection[2]").unwrap(), "own");
    }

    #[test]
    fn indexed_getter_leaves_noncanonical_indices_as_named_properties() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        fn getter(_: &dyn std::any::Any, index: u32) -> Option<Value> {
            Some(Value::Number(index as f64))
        }
        runtime
            .define_indexed_property_getter(&interface, getter)
            .unwrap();
        let collection = runtime.create_instance(&interface, ());
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        assert_eq!(
            runtime.eval("collection[4294967294]").unwrap(),
            "4294967294"
        );
        assert_eq!(runtime.eval(
            "['01', '-1', '1.5', '4294967295', 'label'].every(k => { collection[k] = 'named'; return collection[k] === 'named'; })"
        ).unwrap(), "true");
    }

    #[test]
    fn indexed_getter_on_derived_interface_keeps_wrapper_finalization() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Collection(Arc<AtomicUsize>);
        impl Drop for Collection {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn getter(this: &dyn std::any::Any, index: u32) -> Option<Value> {
            this.downcast_ref::<Collection>()?;
            (index == 0).then_some(Value::String("inherited".into()))
        }
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&parent, getter)
            .unwrap();
        let child = runtime.define_interface("ChildCollection", Some(&parent));
        // FunctionTemplate::inherit does not copy indexed interceptors, unlike accessors.
        runtime
            .define_indexed_property_getter(&child, getter)
            .unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let collection = runtime.create_instance(&child, Collection(drops.clone()));
        runtime
            .set_global_property("collection", &collection)
            .unwrap();
        assert_eq!(runtime.eval("collection[0]").unwrap(), "inherited");
        assert_eq!(
            runtime.eval("Object.create(collection)[0]").unwrap(),
            "inherited"
        );
        drop(collection);
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        runtime.eval("delete globalThis.collection").unwrap();
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        runtime.force_full_gc_for_testing();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn indexed_getter_rejects_registration_after_instance_creation() {
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Collection", None);
        let _collection = runtime.create_instance(&interface, ());
        assert!(
            runtime
                .define_indexed_property_getter(&interface, collection_getter)
                .is_err()
        );
    }

    #[test]
    fn indexed_getter_requires_explicit_registration_on_derived_interfaces() {
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Collection", None);
        runtime
            .define_indexed_property_getter(&parent, collection_getter)
            .unwrap();
        let child = runtime.define_interface("ChildCollection", Some(&parent));
        let base = runtime.create_instance(&parent, vec![Value::String("base".into())]);
        let derived = runtime.create_instance(&child, vec![Value::String("derived".into())]);
        runtime.set_global_property("base", &base).unwrap();
        runtime.set_global_property("derived", &derived).unwrap();
        assert_eq!(runtime.eval("base[0]").unwrap(), "base");
        assert_eq!(runtime.eval_value("derived[0]").unwrap(), Value::Undefined);
    }
}
