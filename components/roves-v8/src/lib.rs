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
}
