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

extern crate self as roves_v8;

#[cfg(feature = "webidl-pilot")]
pub mod webidl {
    pub mod screen {
        include!(concat!(env!("OUT_DIR"), "/ScreenV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod utf16_string_state {
        include!(concat!(env!("OUT_DIR"), "/Utf16StringStateV8Binding.rs"));
    }
    pub mod validity_state {
        include!(concat!(env!("OUT_DIR"), "/ValidityStateV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod mutable_primitives {
        include!(concat!(env!("OUT_DIR"), "/MutablePrimitivesV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod nullable_domstring {
        include!(concat!(env!("OUT_DIR"), "/NullableDomStringV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod usv_strings {
        include!(concat!(env!("OUT_DIR"), "/UsvStringsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod string_operations {
        include!(concat!(env!("OUT_DIR"), "/StringOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod nullable_operations {
        include!(concat!(env!("OUT_DIR"), "/NullableOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_operations {
        include!(concat!(env!("OUT_DIR"), "/OptionalOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod bytestring_operations {
        include!(concat!(env!("OUT_DIR"), "/ByteStringOperationsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_defaults {
        include!(concat!(env!("OUT_DIR"), "/OptionalDefaultsV8Binding.rs"));
    }
    #[cfg(test)]
    pub mod optional_string_defaults {
        include!(concat!(env!("OUT_DIR"), "/OptionalStringDefaultsV8Binding.rs"));
    }
}

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
    /// Internal WebIDL argument state for an omitted optional argument (including explicit
    /// `undefined`). This is never a JavaScript value and cannot be stored back into V8.
    Missing,
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    /// WebIDL ByteString bytes; conversion to JavaScript preserves each byte as a code unit.
    ByteString(Vec<u8>),
    /// A lossless UTF-16 string for JavaScript strings containing unpaired surrogates, which
    /// Rust's UTF-8 `String` cannot represent. Ordinary scalar-valid strings use `String`.
    Utf16String(Vec<u16>),
    /// Raw bytes — round-trips through a JS `Uint8Array` (see [`Runtime::eval_value`] and
    /// [`Runtime::store`]). Distinct from `Object` because this phase's own bindgen-primitives
    /// scope explicitly needs ArrayBuffer/TypedArray, not just "some object".
    Bytes(Vec<u8>),
    Object,
}

/// Engine-neutral representation of the WebIDL restricted `float` type.
/// Construction rejects NaN and infinities, keeping the IDL invariant across the V8 boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteF32(f32);

impl FiniteF32 {
    pub fn new(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

/// Engine-neutral representation of the WebIDL restricted `double` type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteF64(f64);

impl FiniteF64 {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
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
    // Materializing a child also freezes all ancestor templates. Shared flags track
    // that separately from whether each constructor has been exposed globally.
    materialized: std::rc::Rc<std::cell::Cell<bool>>,
    ancestors: Vec<std::rc::Rc<std::cell::Cell<bool>>>,
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
    /// Keeps callbacks armed until they have actually completed. An empty weak handle
    /// alone is insufficient: first-pass GC may clear it before second-pass finalization.
    wrapped_finalizers: Vec<WrappedFinalizer>,
    /// Per-method callback data stays alive as long as this isolate can invoke the callbacks.
    method_configs: Vec<Box<WebIdlMethodConfig>>,
}

struct WrappedFinalizer {
    _weak: v8::Weak<v8::Value>,
    completed: std::rc::Rc<std::cell::Cell<bool>>,
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

/// A WebIDL DOMString setter. JavaScript string conversion runs in the runtime and preserves
/// the resulting UTF-16 code units, including lone surrogates.
pub type DomStringSetter = fn(&mut dyn std::any::Any, Vec<u16>);

/// A nullable WebIDL DOMString setter. `None` represents the IDL `null` value.
pub type NullableDomStringSetter = fn(&mut dyn std::any::Any, Option<Vec<u16>>);

/// A setter for primitive WebIDL attributes after JavaScript coercion.
pub type WebIdlPrimitiveSetter = fn(&mut dyn std::any::Any, &Value);

/// JavaScript-to-WebIDL coercion requested by a generated primitive attribute setter.
#[derive(Clone, Copy)]
pub enum PrimitiveConversion {
    Boolean,
    Double,
    UnsignedLong,
    NullableBoolean,
    NullableDouble,
    NullableUnsignedLong,
    /// JavaScript ToString followed by USVString scalar-value conversion.
    UsvString,
    /// Nullable USVString: JavaScript null maps to IDL null; other values use ToString.
    NullableUsvString,
}

/// Required WebIDL operation argument coercions supported by the generated V8 pilot.
#[derive(Clone, Copy, Debug)]
pub enum WebIdlArgumentConversion {
    Boolean,
    Byte,
    Octet,
    Short,
    UnsignedShort,
    Long,
    LongLong,
    UnsignedLongLong,
    Float,
    UnrestrictedFloat,
    Double,
    UnrestrictedDouble,
    UnsignedLong,
    DomString,
    ByteString,
    UsvString,
}

/// WebIDL optional-argument state. `Missing` differs from a present nullable `None`.
#[derive(Clone, Debug, PartialEq)]
pub enum WebIdlOptionalArgument<T> {
    Missing,
    Present(T),
}

fn convert_webidl_integer(number: f64, bits: u32, signed: bool) -> f64 {
    if !number.is_finite() || number == 0.0 {
        return 0.0;
    }
    let modulus = 2.0_f64.powi(bits as i32);
    let mut value = number.trunc().rem_euclid(modulus);
    if signed && value >= modulus / 2.0 {
        value -= modulus;
    }
    value
}

struct WebIdlMethodConfig {
    method: NativeMethod,
    conversions: Vec<WebIdlArgumentConversion>,
    nullable_arguments: Vec<bool>,
    optional_arguments: Vec<bool>,
}

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
            method_configs: Vec::new(),
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
        // Reclaim only completed finalizers, never callbacks waiting for GC's second pass.
        self.wrapped_finalizers.retain(|entry| !entry.completed.get());
        let completed = std::rc::Rc::new(std::cell::Cell::new(false));
        let completion = completed.clone();
        let weak = v8::Weak::with_guaranteed_finalizer(
            &mut self.isolate,
            &global_value,
            Box::new(move || {
                // SAFETY: `raw` was created by `Box::into_raw` a few lines above and is reachable
                // from exactly one place afterward (this closure) -- V8 guarantees this finalizer
                // runs at most once, and only after nothing JS-reachable points at the wrapper
                // anymore, so nothing else can read `raw` concurrently or afterward.
                drop(unsafe { Box::from_raw(raw) });
                completion.set(true);
            }),
        );
        self.wrapped_finalizers.push(WrappedFinalizer { _weak: weak, completed });

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

        // Nonconstructible WebIDL interfaces throw on both calls and construction.
        // Native instances are created by create_instance without invoking this function.
        let template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             _args: v8::FunctionCallbackArguments,
             _retval: v8::ReturnValue| {
                throw_type_error(scope, "Illegal constructor");
             },
        )
        .build(scope);
        template.instance_template(scope).set_internal_field_count(1);

        if let Some(parent) = parent {
            let parent_template = v8::Local::new(scope, &parent.template);
            template.inherit(parent_template);
        }

        if let Some(class_name) = v8::String::new(scope, name) {
            template.set_class_name(class_name);
            let tag = v8::Symbol::get_to_string_tag(scope);
            template.prototype_template(scope).set_with_attr(
                tag.into(), class_name.into(),
                v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
            );
        }
        template.read_only_prototype();

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
            materialized: std::rc::Rc::new(std::cell::Cell::new(false)),
            ancestors: parent.map_or_else(Vec::new, |parent| {
                let mut ancestors = parent.ancestors.clone();
                ancestors.push(parent.materialized.clone());
                ancestors
            }),
        }
    }

    /// Finalizes registration and exposes a nonconstructible interface constructor even
    /// before any native instance exists. All members must be registered first.
    pub fn expose_interface(&mut self, interface: &Interface) -> Result<(), String> {
        if interface.constructor_exposed.get() { return Ok(()); }
        interface.materialized.set(true);
        for ancestor in &interface.ancestors { ancestor.set(true); }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let function = template.get_function(scope).ok_or("failed to materialize interface")?;
        let key = v8::String::new(scope, &interface.name).ok_or("invalid interface name")?;
        if context.global(scope).define_own_property(
            scope, key.into(), function.into(), v8::PropertyAttribute::DONT_ENUM,
        ) != Some(true) {
            return Err("failed to expose interface".into());
        }
        interface.constructor_exposed.set(true);
        Ok(())
    }

    /// Creates an instance of `interface`, reflecting `value` into it exactly like
    /// [`Runtime::create_wrapped`] (same ownership/finalization lifecycle, same
    /// [`Runtime::get_wrapped`] read-back) — but the instance's `[[Prototype]]` is
    /// `interface`'s prototype object, so `instanceof` and the prototype chain work from JS,
    /// which a plain `create_wrapped` object doesn't have.
    pub fn create_instance<T: 'static>(&mut self, interface: &Interface, value: T) -> Handle {
        self.expose_interface(interface).expect("failed to expose native interface");
        let boxed_any: Box<dyn std::any::Any> = Box::new(value);

        let (global_value, raw) = {
            let context_handle = &self.context;
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, context_handle);
            let scope = &mut v8::ContextScope::new(scope, context);

            let template = v8::Local::new(scope, &interface.template);

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

    /// Defines a read-only WebIDL-style attribute on the interface prototype.
    /// Function-template accessors receive the actual `this` and enforce the interface
    /// signature, unlike PropertyCallbackArguments-based instance accessors.
    pub fn define_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, None, None)
    }

    /// Defines a prototype attribute with both a getter and a setter. Callback functions
    /// each carry their own pointer data; no shared tuple allocation is needed.
    pub fn define_settable_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: PropertySetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, Some(setter), None, None, None)
    }

    /// Defines a WebIDL DOMString attribute with JavaScript ToString conversion.
    pub fn define_domstring_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: DomStringSetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, Some(setter), None, None)
    }

    /// Defines a nullable WebIDL DOMString attribute. JavaScript `null` maps to IDL null;
    /// other values use JavaScript ToString and preserve UTF-16 code units.
    pub fn define_nullable_domstring_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: NullableDomStringSetter,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, Some(setter), None)
    }

    /// Defines a settable primitive attribute whose conversion follows WebIDL's boolean or
    /// numeric conversion rules before native state is mutably borrowed.
    pub fn define_webidl_primitive_property(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: WebIdlPrimitiveSetter,
        conversion: PrimitiveConversion,
    ) -> Result<(), String> {
        self.define_attribute(interface, name, getter, None, None, None, Some((setter, conversion)))
    }

    fn define_attribute(
        &mut self,
        interface: &Interface,
        name: &str,
        getter: PropertyGetter,
        setter: Option<PropertySetter>,
        domstring_setter: Option<DomStringSetter>,
        nullable_domstring_setter: Option<NullableDomStringSetter>,
        primitive_setter: Option<(WebIdlPrimitiveSetter, PrimitiveConversion)>,
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);
        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let key = v8::String::new(scope, name).ok_or("invalid attribute name")?;
        let getter_data = v8::External::new(scope, getter as *mut std::ffi::c_void);
        let getter_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                // SAFETY: data is the PropertyGetter function pointer installed above.
                let getter: PropertyGetter = unsafe { std::mem::transmute(external.value()) };
                let this = args.this();
                // The signature rejects foreign receivers before this callback runs.
                // A valid receiver still needs initialized native data (JS constructors
                // for nonconstructible interfaces are rejected separately).
                if this.internal_field_count() < 1 {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                let raw = unsafe {
                    this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                } as *mut Box<dyn std::any::Any>;
                if raw.is_null() {
                    throw_type_error(scope, "Illegal invocation");
                    return;
                }
                // SAFETY: our instance owns this Box; its live local handle prevents GC.
                let value = getter(unsafe { (&*raw).as_ref() });
                retval.set(v8_value(scope, &value));
            },
        )
        .data(getter_data.into())
        .signature(signature)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
        let getter_name = v8::String::new(scope, &format!("get {name}")).unwrap();
        getter_template.set_class_name(getter_name);
        let setter_template = if let Some(setter) = setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            Some(v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                    let setter: PropertySetter = unsafe { std::mem::transmute(external.value()) };
                    let this = args.this();
                    if this.internal_field_count() < 1 {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let raw = unsafe {
                        this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                    } as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let value = native_value(scope, args.get(0));
                    // SAFETY: JavaScript exclusively borrows Runtime during this callback, so
                    // no shared native getter reference can overlap the mutable borrow.
                    setter(unsafe { (&mut *raw).as_mut() }, &value);
                },
            )
            .data(setter_data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope))
        } else if let Some((setter, conversion)) = primitive_setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            macro_rules! primitive_setter_template {
                ($conversion:expr) => {
                    v8::FunctionTemplate::builder(
                        |scope: &mut v8::PinScope,
                         args: v8::FunctionCallbackArguments,
                         _retval: v8::ReturnValue| {
                            let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                            let setter: WebIdlPrimitiveSetter = unsafe { std::mem::transmute(external.value()) };
                            let this = args.this();
                            if this.internal_field_count() < 1 {
                                throw_type_error(scope, "Illegal invocation");
                                return;
                            }
                            let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
                                as *mut Box<dyn std::any::Any>;
                            if raw.is_null() {
                                throw_type_error(scope, "Illegal invocation");
                                return;
                            }
                            let value = args.get(0);
                            let converted = match $conversion {
                                PrimitiveConversion::Boolean => Value::Bool(value.boolean_value(scope)),
                                PrimitiveConversion::Double => {
                                    let Some(number) = value.number_value(scope) else { return; };
                                    if !number.is_finite() {
                                        throw_type_error(scope, "double must be finite");
                                        return;
                                    }
                                    Value::Number(number)
                                }
                                PrimitiveConversion::UnsignedLong => {
                                    let Some(number) = value.number_value(scope) else { return; };
                                    if !number.is_finite() || number == 0.0 {
                                        Value::Number(0.0)
                                    } else {
                                        Value::Number(number.trunc().rem_euclid(4_294_967_296.0))
                                    }
                                }
                                PrimitiveConversion::NullableBoolean => {
                                    if value.is_null() { Value::Null } else { Value::Bool(value.boolean_value(scope)) }
                                }
                                PrimitiveConversion::NullableDouble => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(number) = value.number_value(scope) else { return; };
                                        if !number.is_finite() {
                                            throw_type_error(scope, "double must be finite");
                                            return;
                                        }
                                        Value::Number(number)
                                    }
                                }
                                PrimitiveConversion::NullableUnsignedLong => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(number) = value.number_value(scope) else { return; };
                                        if !number.is_finite() || number == 0.0 {
                                            Value::Number(0.0)
                                        } else {
                                            Value::Number(number.trunc().rem_euclid(4_294_967_296.0))
                                        }
                                    }
                                }
                                PrimitiveConversion::UsvString => {
                                    let Some(string) = value.to_string(scope) else { return; };
                                    Value::String(string.to_rust_string_lossy(scope))
                                }
                                PrimitiveConversion::NullableUsvString => {
                                    if value.is_null() {
                                        Value::Null
                                    } else {
                                        let Some(string) = value.to_string(scope) else { return; };
                                        Value::String(string.to_rust_string_lossy(scope))
                                    }
                                }
                            };
                            // Coercion may run user JavaScript; borrow native state only after it completes.
                            setter(unsafe { (&mut *raw).as_mut() }, &converted);
                        },
                    )
                    .data(setter_data.into())
                    .signature(signature)
                    .length(1)
                    .constructor_behavior(v8::ConstructorBehavior::Throw)
                    .build(scope)
                };
            }
            Some(match conversion {
                PrimitiveConversion::Boolean => primitive_setter_template!(PrimitiveConversion::Boolean),
                PrimitiveConversion::Double => primitive_setter_template!(PrimitiveConversion::Double),
                PrimitiveConversion::UnsignedLong => primitive_setter_template!(PrimitiveConversion::UnsignedLong),
                PrimitiveConversion::NullableBoolean => primitive_setter_template!(PrimitiveConversion::NullableBoolean),
                PrimitiveConversion::NullableDouble => primitive_setter_template!(PrimitiveConversion::NullableDouble),
                PrimitiveConversion::NullableUnsignedLong => primitive_setter_template!(PrimitiveConversion::NullableUnsignedLong),
                PrimitiveConversion::UsvString => primitive_setter_template!(PrimitiveConversion::UsvString),
                PrimitiveConversion::NullableUsvString => primitive_setter_template!(PrimitiveConversion::NullableUsvString),
            })
        } else if let Some(setter) = nullable_domstring_setter {
            let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
            Some(v8::FunctionTemplate::builder(
                |scope: &mut v8::PinScope,
                 args: v8::FunctionCallbackArguments,
                 _retval: v8::ReturnValue| {
                    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                    let setter: NullableDomStringSetter = unsafe { std::mem::transmute(external.value()) };
                    let this = args.this();
                    if this.internal_field_count() < 1 {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let raw = unsafe { this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG) }
                        as *mut Box<dyn std::any::Any>;
                    if raw.is_null() {
                        throw_type_error(scope, "Illegal invocation");
                        return;
                    }
                    let value = args.get(0);
                    let converted = if value.is_null() {
                        None
                    } else {
                        let Some(string) = value.to_string(scope) else { return; };
                        let mut units = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
                        Some(units)
                    };
                    // ToString can run user code or throw; borrow native state only afterwards.
                    setter(unsafe { (&mut *raw).as_mut() }, converted);
                },
            )
            .data(setter_data.into())
            .signature(signature)
            .length(1)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope))
        } else {
            domstring_setter.map(|setter| {
                let setter_data = v8::External::new(scope, setter as *mut std::ffi::c_void);
                v8::FunctionTemplate::builder(
                    |scope: &mut v8::PinScope,
                     args: v8::FunctionCallbackArguments,
                     _retval: v8::ReturnValue| {
                        let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
                        let setter: DomStringSetter =
                            unsafe { std::mem::transmute(external.value()) };
                        let this = args.this();
                        if this.internal_field_count() < 1 {
                            throw_type_error(scope, "Illegal invocation");
                            return;
                        }
                        let raw = unsafe {
                            this.get_aligned_pointer_from_internal_field(0, WRAPPED_POINTER_TAG)
                        } as *mut Box<dyn std::any::Any>;
                        if raw.is_null() {
                            throw_type_error(scope, "Illegal invocation");
                            return;
                        }
                        let Some(string) = args.get(0).to_string(scope) else { return; };
                        let mut units = vec![0; string.length()];
                        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
                        // The ToString operation above may execute user code, so take the
                        // mutable native borrow only after conversion has completed.
                        setter(unsafe { (&mut *raw).as_mut() }, units);
                    },
                )
                .data(setter_data.into())
                .signature(signature)
                .length(1)
                .constructor_behavior(v8::ConstructorBehavior::Throw)
                .build(scope)
            })
        };
        let setter_template = setter_template.map(|function| {
            let setter_name = v8::String::new(scope, &format!("set {name}")).unwrap();
            function.set_class_name(setter_name);
            function
        });
        template.prototype_template(scope).set_accessor_property(
            key.into(), Some(getter_template), setter_template, v8::PropertyAttribute::NONE,
        );
        Ok(())
    }

    /// Defines a nonconstructible method on the interface prototype, with a signature
    /// enforcing valid receivers (including interface inheritance).
    pub fn define_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
    ) -> Result<(), String> {
        self.define_webidl_method(interface, name, method, &[])
    }

    /// Defines a method whose arguments are converted using the listed required WebIDL types.
    /// Extra JavaScript arguments are ignored by the generated binding; omitted arguments are
    /// converted from `undefined` according to the declared type.
    pub fn define_webidl_method(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
    ) -> Result<(), String> {
        self.define_webidl_method_with_nullable_arguments(interface, name, method, conversions, &[])
    }

    /// Defines a method whose required arguments use the listed WebIDL conversions and
    /// nullable flags. For nullable types both `null` and `undefined` map to IDL null before
    /// applying the inner type's conversion.
    pub fn define_webidl_method_with_nullable_arguments(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
    ) -> Result<(), String> {
        self.define_webidl_method_with_argument_flags(
            interface, name, method, conversions, nullable_arguments, &[],
        )
    }

    /// Defines a method with per-argument nullable and optional WebIDL semantics. An optional
    /// argument whose value is omitted or `undefined` reaches the native callback as
    /// [`Value::Missing`], before nullable conversion; `null` for a nullable argument remains
    /// [`Value::Null`]. Explicit default values are handled by generated bindings separately.
    pub fn define_webidl_method_with_argument_flags(
        &mut self,
        interface: &Interface,
        name: &str,
        method: NativeMethod,
        conversions: &[WebIdlArgumentConversion],
        nullable_arguments: &[bool],
        optional_arguments: &[bool],
    ) -> Result<(), String> {
        if interface.materialized.get() {
            return Err("interface members must be defined before creating instances or descendants".into());
        }
        let context_handle = &self.context;
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Local::new(scope, context_handle);
        let scope = &mut v8::ContextScope::new(scope, context);

        let template = v8::Local::new(scope, &interface.template);
        let signature = v8::Signature::new(scope, template);
        let prototype_template = template.prototype_template(scope);

        let Some(key) = v8::String::new(scope, name) else {
            return Err(format!("{name:?} is not valid as a method name string"));
        };
        // Keep the immutable config alive for the isolate's lifetime. Box preserves its address
        // while the owning vector grows, and the isolate drops before these entries.
        let config = Box::new(WebIdlMethodConfig {
            method,
            conversions: conversions.to_vec(),
            nullable_arguments: (0..conversions.len())
                .map(|index| nullable_arguments.get(index).copied().unwrap_or(false))
                .collect(),
            optional_arguments: (0..conversions.len())
                .map(|index| optional_arguments.get(index).copied().unwrap_or(false))
                .collect(),
        });
        let config_pointer = (&*config) as *const WebIdlMethodConfig as *mut WebIdlMethodConfig;
        self.method_configs.push(config);
        let external_data = v8::External::new(scope, config_pointer as *mut std::ffi::c_void);

        let function_template = v8::FunctionTemplate::builder(
            |scope: &mut v8::PinScope,
             args: v8::FunctionCallbackArguments,
             mut retval: v8::ReturnValue| {
                let data = args.data();
                let Ok(external) = v8::Local::<v8::External>::try_from(data) else {
                    return;
                };
                // SAFETY: the External points to the config allocated for this method template.
                let config = unsafe { &*(external.value() as *const WebIdlMethodConfig) };

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

                // WebIDL's ToNumber/ToString conversions can run user code and throw. Preserve
                // that exact exception across the native callback boundary instead of returning
                // undefined when rusty_v8 reports the failed conversion as `None`.
                let argument_count = if config.conversions.is_empty() {
                    args.length() as usize
                } else {
                    config.conversions.len()
                };
                let mut arguments = Vec::with_capacity(argument_count);
                for i in 0..argument_count {
                    let argument = args.get(i as i32);
                    let converted = if config.optional_arguments.get(i).copied().unwrap_or(false)
                        && argument.is_undefined()
                    {
                        Value::Missing
                    } else if config.nullable_arguments.get(i).copied().unwrap_or(false)
                        && (argument.is_null() || argument.is_undefined())
                    {
                        Value::Null
                    } else {
                        match config.conversions.get(i) {
                        None => native_value(scope, argument),
                        Some(WebIdlArgumentConversion::Boolean) => Value::Bool(argument.boolean_value(scope)),
                        Some(WebIdlArgumentConversion::Byte) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 8, true))
                        }
                        Some(WebIdlArgumentConversion::Octet) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 8, false))
                        }
                        Some(WebIdlArgumentConversion::Short) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 16, true))
                        }
                        Some(WebIdlArgumentConversion::UnsignedShort) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 16, false))
                        }
                        Some(WebIdlArgumentConversion::Long) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 32, true))
                        }
                        Some(WebIdlArgumentConversion::LongLong) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 64, true))
                        }
                        Some(WebIdlArgumentConversion::UnsignedLongLong) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 64, false))
                        }
                        Some(WebIdlArgumentConversion::Float) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            let value = number as f32;
                            if !value.is_finite() {
                                throw_type_error(scope, "float argument must be finite");
                                return;
                            }
                            Value::Number(value as f64)
                        }
                        Some(WebIdlArgumentConversion::UnrestrictedFloat) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(number as f32 as f64)
                        }
                        Some(WebIdlArgumentConversion::Double) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            if !number.is_finite() {
                                throw_type_error(scope, "double argument must be finite");
                                return;
                            }
                            Value::Number(number)
                        }
                        Some(WebIdlArgumentConversion::UnrestrictedDouble) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(number)
                        }
                        Some(WebIdlArgumentConversion::UnsignedLong) => {
                            let Some(number) = argument.number_value(scope) else { return; };
                            Value::Number(convert_webidl_integer(number, 32, false))
                        }
                        Some(WebIdlArgumentConversion::DomString) => {
                            if argument.is_symbol() {
                                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                                return;
                            }
                            let result = {
                                v8::tc_scope!(let tc_scope, scope);
                                let scope = tc_scope;
                                match argument.to_string(scope) {
                                    Some(string) => {
                                        let mut utf16 = vec![0; string.length()];
                                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                                        Ok(Value::Utf16String(utf16))
                                    }
                                    None => Err(scope.exception()),
                                }
                            };
                            match result {
                                Ok(value) => value,
                                Err(Some(exception)) => { scope.throw_exception(exception); return; }
                                Err(None) => return,
                            }
                        }
                        Some(WebIdlArgumentConversion::UsvString) => {
                            if argument.is_symbol() {
                                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                                return;
                            }
                            let result = {
                                v8::tc_scope!(let tc_scope, scope);
                                let scope = tc_scope;
                                match argument.to_string(scope) {
                                    Some(string) => {
                                        let mut utf16 = vec![0; string.length()];
                                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                                        Ok(Value::String(String::from_utf16_lossy(&utf16)))
                                    }
                                    None => Err(scope.exception()),
                                }
                            };
                            match result {
                                Ok(value) => value,
                                Err(Some(exception)) => { scope.throw_exception(exception); return; }
                                Err(None) => return,
                            }
                        }
                        Some(WebIdlArgumentConversion::ByteString) => {
                            if argument.is_symbol() {
                                throw_type_error(scope, "Cannot convert a Symbol value to a string");
                                return;
                            }
                            let result = {
                                v8::tc_scope!(let tc_scope, scope);
                                let scope = tc_scope;
                                match argument.to_string(scope) {
                                    Some(string) => {
                                        let mut utf16 = vec![0; string.length()];
                                        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
                                        if utf16.iter().any(|unit| *unit > 0xFF) {
                                            let message = v8::String::new(scope, "ByteString contains a code unit greater than 255").unwrap();
                                            Err(Some(v8::Exception::type_error(scope, message).into()))
                                        } else {
                                            Ok(Value::ByteString(utf16.into_iter().map(|unit| unit as u8).collect()))
                                        }
                                    }
                                    None => Err(scope.exception()),
                                }
                            };
                            match result {
                                Ok(value) => value,
                                Err(Some(exception)) => { scope.throw_exception(exception); return; }
                                Err(None) => return,
                            }
                        }
                        }
                    };
                    arguments.push(converted);
                }
                let result = (config.method)(boxed_any.as_ref(), &arguments);
                retval.set(v8_value(scope, &result));
            },
        )
        .data(external_data.into())
        .signature(signature)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);

        let function_value: v8::Local<v8::Data> = function_template.into();
        prototype_template.set(key.into(), function_value);
        Ok(())
    }

    /// Installs an indexed read interceptor on the interface's instance template.
    /// Register before creating instances of this interface or any descendant; materialized
    /// V8 templates cannot be mutated. The guard also detects descendant instantiation.
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
        if interface.materialized.get() {
            return Err("indexed getter must be defined before creating instances or descendants".to_string());
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
    /// The returned reference borrows from both the runtime and `handle` — passing a `handle`
    /// that isn't actually keeping the object alive (there is no such way to construct one
    /// outside this crate) would be unsound; a live `Handle` argument is what guarantees the
    /// finalizer in `create_wrapped` hasn't run yet. Borrowing the runtime also prevents JS
    /// setters from mutating the data, or isolate disposal from freeing it, while it is read.
    ///
    /// ```compile_fail
    /// use roves_v8::Runtime;
    /// let mut runtime = Runtime::new();
    /// let handle = runtime.create_wrapped(String::from("native"));
    /// let reference = runtime.get_wrapped::<String>(&handle).unwrap();
    /// runtime.eval("1").unwrap(); // Cannot run JS while native data is borrowed.
    /// println!("{reference}");
    /// ```
    ///
    /// ```compile_fail
    /// use roves_v8::Runtime;
    /// let mut runtime = Runtime::new();
    /// let handle = runtime.create_wrapped(String::from("native"));
    /// let reference = runtime.get_wrapped::<String>(&handle).unwrap();
    /// drop(runtime); // Cannot dispose the isolate while native data is borrowed.
    /// println!("{reference}");
    /// ```
    pub fn get_wrapped<'h, T: 'static>(&'h mut self, handle: &'h Handle) -> Option<&'h T> {
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
        // The signature ties this read to both the live handle and an exclusive runtime
        // borrow. No JS evaluation/setter or isolate disposal can overlap the reference.
        boxed_any.downcast_ref::<T>()
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

fn throw_type_error(scope: &mut v8::PinScope, message: &str) {
    let message = v8::String::new(scope, message).unwrap();
    let exception = v8::Exception::type_error(scope, message);
    scope.throw_exception(exception);
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
        let string = v8::Local::<v8::String>::try_from(value).unwrap();
        let mut utf16 = vec![0; string.length()];
        string.write_v2(scope, 0, &mut utf16, v8::WriteFlags::empty());
        match std::string::String::from_utf16(&utf16) {
            Ok(string) => Value::String(string),
            Err(_) => Value::Utf16String(utf16),
        }
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
        Value::Missing | Value::Undefined | Value::Object => v8::undefined(scope).into(),
        Value::Null => v8::null(scope).into(),
        Value::Bool(b) => v8::Boolean::new(scope, *b).into(),
        Value::Number(n) => v8::Number::new(scope, *n).into(),
        Value::String(s) => v8::String::new(scope, s)
            .map(Into::into)
            .unwrap_or_else(|| v8::undefined(scope).into()),
        Value::ByteString(bytes) => {
            let units = bytes.iter().map(|byte| u16::from(*byte)).collect::<Vec<_>>();
            v8::String::new_from_two_byte(scope, &units, v8::NewStringType::Normal)
                .map(Into::into)
                .unwrap_or_else(|| v8::undefined(scope).into())
        }
        Value::Utf16String(units) => v8::String::new_from_two_byte(
            scope,
            units,
            v8::NewStringType::Normal,
        )
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
    fn unpaired_surrogates_round_trip_without_loss() {
        let mut runtime = Runtime::new();
        let units = vec![0xD800, b'A' as u16, 0xDC00];
        let handle = runtime.store(&Value::Utf16String(units.clone()));
        runtime.set_global_property("wide", &handle).unwrap();
        assert_eq!(runtime.eval_value("wide.length").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("wide.charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        assert_eq!(runtime.eval_value("wide.charCodeAt(1)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("wide.charCodeAt(2)").unwrap(), Value::Number(0xDC00 as f64));
        assert_eq!(runtime.eval_value("wide").unwrap(), Value::Utf16String(units.clone()));
        assert_eq!(runtime.load(&handle), Value::Utf16String(units));
    }

    #[test]
    #[cfg(feature = "webidl-pilot")]
    fn generated_domstring_binding_preserves_unpaired_surrogates() {
        use crate::webidl::utf16_string_state::{Utf16StringStateBinding, Utf16StringStateNative};

        struct NativeString(Vec<u16>);
        impl Utf16StringStateNative for NativeString {
            fn Value(&self) -> Vec<u16> {
                self.0.clone()
            }
            fn set_Value(&mut self, value: Vec<u16>) {
                self.0 = value;
            }
        }

        let mut runtime = Runtime::new();
        let binding = Utf16StringStateBinding::<NativeString>::install(&mut runtime).unwrap();
        let units = vec![0xD800, b'A' as u16, 0xDC00];
        let state = binding.create(&mut runtime, NativeString(units.clone()));
        runtime.set_global_property("state", &state).unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Utf16String(units));
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(0)").unwrap(),
            Value::Number(0xD800 as f64)
        );
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(1)").unwrap(),
            Value::Number(65.0)
        );
        assert_eq!(
            runtime.eval_value("state.value.charCodeAt(2)").unwrap(),
            Value::Number(0xDC00 as f64)
        );
        runtime.eval("state.value = 42").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("42".into()));
        runtime.eval("state.value = '\\ud800x\\udc00'").unwrap();
        assert_eq!(
            runtime.eval_value("state.value").unwrap(),
            Value::Utf16String(vec![0xD800, b'x' as u16, 0xDC00])
        );
        assert!(runtime
            .eval("state.value = Symbol('no string conversion')")
            .unwrap_err()
            .contains("TypeError"));
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

    #[test]
    fn materializing_a_descendant_rejects_late_members_on_all_ancestors() {
        fn getter(_: &dyn std::any::Any) -> Value { Value::Undefined }
        fn setter(_: &mut dyn std::any::Any, _: &Value) {}
        fn method(_: &dyn std::any::Any, _: &[Value]) -> Value { Value::Undefined }
        let mut runtime = Runtime::new();
        let parent = runtime.define_interface("Parent", None);
        let middle = runtime.define_interface("Middle", Some(&parent));
        let child = runtime.define_interface("Child", Some(&middle));
        let _instance = runtime.create_instance(&child, ());
        for interface in [&parent, &middle, &child] {
            assert!(runtime.define_property(interface, "read", getter).is_err());
            assert!(runtime.define_settable_property(interface, "write", getter, setter).is_err());
            assert!(runtime.define_method(interface, "method", method).is_err());
            assert!(runtime.define_indexed_property_getter(interface, collection_getter).is_err());
        }
        // Instantiating a sibling through a frozen parent is valid; defining its own
        // members remains possible until the sibling itself is materialized.
        let sibling = runtime.define_interface("Sibling", Some(&parent));
        runtime.define_property(&sibling, "read", getter).unwrap();
        let _sibling = runtime.create_instance(&sibling, ());
    }


    #[test]
    fn completed_finalizer_records_are_reclaimed_without_disarming_live_wrappers() {
        let mut runtime = Runtime::new();
        let live = runtime.create_wrapped(123_i32);
        for _ in 0..10 {
            for _ in 0..20 { drop(runtime.create_wrapped(())); }
            runtime.force_full_gc_for_testing();
            // The next allocation sweeps completed records, retaining the live one.
            let next = runtime.create_wrapped(());
            assert!(runtime.wrapped_finalizers.len() <= 2);
            assert_eq!(runtime.get_wrapped::<i32>(&live), Some(&123));
            drop(next);
            runtime.force_full_gc_for_testing();
        }
    }


    #[test]
    fn prototype_attributes_have_webidl_descriptors_and_check_receivers() {
        fn getter(this: &dyn std::any::Any) -> Value {
            Value::Number(*this.downcast_ref::<i32>().unwrap() as f64)
        }
        fn setter(this: &mut dyn std::any::Any, value: &Value) {
            if let Value::Number(value) = value { *this.downcast_mut::<i32>().unwrap() = *value as i32; }
        }
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Native", None);
        runtime.define_settable_property(&interface, "value", getter, setter).unwrap();
        let node = runtime.create_instance(&interface, 3_i32);
        runtime.set_global_property("node", &node).unwrap();
        assert_eq!(runtime.eval("Object.hasOwn(node, 'value')").unwrap(), "false");
        assert_eq!(runtime.eval("const d = Object.getOwnPropertyDescriptor(Native.prototype, 'value'); [d.get.name, d.get.length, d.set.name, d.set.length, d.enumerable, d.configurable].join(',')").unwrap(), "get value,0,set value,1,true,true");
        assert_eq!(runtime.eval("d.get.call(node)").unwrap(), "3");
        runtime.eval("d.set.call(node, 9)").unwrap();
        assert_eq!(runtime.get_wrapped::<i32>(&node), Some(&9));
        for expression in ["d.get.call({})", "d.set.call({}, 1)", "d.get.call(Object.create(node))", "new d.get()"] {
            assert!(runtime.eval(expression).unwrap_err().contains("TypeError"), "{expression}");
        }
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_mutable_primitive_attributes_follow_webidl_conversion() {
        use crate::webidl::mutable_primitives::{MutablePrimitivesBinding, MutablePrimitivesNative};
        use crate::{FiniteF32, FiniteF64};
        assert!(FiniteF32::new(f32::NAN).is_none());
        assert!(FiniteF32::new(f32::INFINITY).is_none());
        assert!(FiniteF64::new(f64::NAN).is_none());
        assert!(FiniteF64::new(f64::NEG_INFINITY).is_none());
        assert_eq!(FiniteF32::new(1.25).unwrap().get(), 1.25);
        assert_eq!(FiniteF64::new(-2.5).unwrap().get(), -2.5);
        #[derive(Default)]
        struct State {
            enabled: bool,
            ratio: f64,
            count: u32,
            optional_enabled: Option<bool>,
            optional_ratio: Option<f64>,
            optional_count: Option<u32>,
        }
        #[allow(non_snake_case)]
        impl MutablePrimitivesNative for State {
            fn Enabled(&self) -> bool { self.enabled }
            fn set_Enabled(&mut self, value: bool) { self.enabled = value; }
            fn Ratio(&self) -> FiniteF64 { FiniteF64::new(self.ratio).expect("test state stores finite double attributes") }
            fn set_Ratio(&mut self, value: FiniteF64) { self.ratio = value.get(); }
            fn Count(&self) -> u32 { self.count }
            fn set_Count(&mut self, value: u32) { self.count = value; }
            fn OptionalEnabled(&self) -> Option<bool> { self.optional_enabled }
            fn set_OptionalEnabled(&mut self, value: Option<bool>) { self.optional_enabled = value; }
            fn OptionalRatio(&self) -> Option<FiniteF64> { self.optional_ratio.map(|value| FiniteF64::new(value).expect("test state stores finite nullable doubles")) }
            fn set_OptionalRatio(&mut self, value: Option<FiniteF64>) { self.optional_ratio = value.map(FiniteF64::get); }
            fn OptionalCount(&self) -> Option<u32> { self.optional_count }
            fn set_OptionalCount(&mut self, value: Option<u32>) { self.optional_count = value; }
            fn Ping(&self) {}
            fn IsEnabled(&self) -> bool { self.enabled }
            fn CurrentRatio(&self) -> FiniteF64 { FiniteF64::new(self.ratio).unwrap_or_else(|| FiniteF64::new(0.0).unwrap()) }
            fn CurrentUnrestrictedRatio(&self) -> f64 { f64::INFINITY }
            fn OptionalUnrestrictedRatioResult(&self) -> Option<f64> { Some(f64::NAN) }
            fn CurrentUnrestrictedFloat(&self) -> f32 { f32::INFINITY }
            fn OptionalUnrestrictedFloatResult(&self) -> Option<f32> { Some(f32::NAN) }
            fn CurrentCount(&self) -> u32 { self.count }
            fn Accepts(&self, value: bool) -> bool { value }
            fn Add(&self, value: FiniteF64) -> FiniteF64 { FiniteF64::new(self.ratio + value.get()).unwrap() }
            fn EchoUnrestricted(&self, value: f64) -> f64 { value }
            fn Wrap(&self, value: u32) -> u32 { value }
            fn EchoByte(&self, value: i8) -> i8 { value }
            fn EchoOctet(&self, value: u8) -> u8 { value }
            fn EchoShort(&self, value: i16) -> i16 { value }
            fn EchoUnsignedShort(&self, value: u16) -> u16 { value }
            fn EchoLong(&self, value: i32) -> i32 { value }
            fn EchoLongLong(&self, value: i64) -> i64 { value }
            fn EchoUnsignedLongLong(&self, value: u64) -> u64 { value }
            fn EchoFloat(&self, value: FiniteF32) -> FiniteF32 { value }
            fn EchoUnrestrictedFloat(&self, value: f32) -> f32 { value }
            fn CurrentLabel(&self) -> Vec<u16> { vec![0xD800, 0x0041] }
            fn OptionalLabel(&self) -> Option<Vec<u16>> { None }
            fn CurrentUsvLabel(&self) -> String { "v8 ?".to_owned() }
            fn OptionalUsvLabel(&self) -> Option<String> { Some("game".to_owned()) }
            fn OptionalEnabledResult(&self) -> Option<bool> { Some(true) }
            fn OptionalRatioResult(&self) -> Option<FiniteF64> { None }
            fn OptionalCountResult(&self) -> Option<u32> { Some(u32::MAX) }
            fn SignedByteResult(&self) -> i8 { -7 }
            fn OctetResult(&self) -> u8 { 250 }
            fn ShortResult(&self) -> i16 { -300 }
            fn UnsignedShortResult(&self) -> u16 { 60_000 }
            fn LongResult(&self) -> i32 { -2_000_000 }
            fn LongLongResult(&self) -> i64 { i64::MAX }
            fn UnsignedLongLongResult(&self) -> u64 { u64::MAX }
            fn FloatResult(&self) -> FiniteF32 { FiniteF32::new(1.25).unwrap() }
            fn NullableFloatResult(&self) -> Option<FiniteF32> { Some(FiniteF32::new(-2.5).unwrap()) }
            fn NullableLongLongResult(&self) -> Option<i64> { None }
        }
        let mut runtime = Runtime::new();
        let binding = MutablePrimitivesBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        runtime.eval("state.enabled = 'false'; state.ratio = '2.5'; state.count = -1").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert!(native.enabled);
        assert_eq!(native.ratio, 2.5);
        assert_eq!(native.count, u32::MAX);
        runtime.eval("state.enabled = 0; state.count = 4294967297").unwrap();
        assert!(runtime.eval("state.ratio = {}").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("state.ratio = Infinity").unwrap_err().contains("TypeError"));
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert!(!native.enabled);
        assert_eq!(native.ratio, 2.5);
        assert_eq!(native.count, 1);
        assert!(runtime.eval("state.count = Symbol() ").is_err());

        assert_eq!(runtime.eval_value("state.add('2.5')").unwrap(), Value::Number(5.0));
        assert_eq!(runtime.eval_value("state.wrap(-1)").unwrap(), Value::Number(u32::MAX as f64));
        assert_eq!(runtime.eval_value("state.echoUnrestricted(Infinity)").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval_value("state.echoByte(128)").unwrap(), Value::Number(-128.0));
        assert_eq!(runtime.eval_value("state.echoOctet(-1)").unwrap(), Value::Number(255.0));
        assert_eq!(runtime.eval_value("state.echoOctet(NaN)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("state.echoShort(32768)").unwrap(), Value::Number(-32768.0));
        assert_eq!(runtime.eval_value("state.echoUnsignedShort(-1)").unwrap(), Value::Number(65535.0));
        assert_eq!(runtime.eval_value("state.echoLong(4294967295)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("state.echoLong(Infinity)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("state.echoLongLong(9223372036854775808)").unwrap(), Value::Number(i64::MIN as f64));
        assert_eq!(runtime.eval_value("state.echoUnsignedLongLong(-1)").unwrap(), Value::Number(u64::MAX as f64));
        assert_eq!(runtime.eval_value("state.echoFloat('1.5')").unwrap(), Value::Number(1.5));
        assert_eq!(runtime.eval_value("state.echoUnrestrictedFloat(Infinity)").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval("Number.isNaN(state.echoUnrestrictedFloat(NaN))").unwrap(), "true");
        for source in ["state.add(Infinity)", "state.add(Symbol())", "state.add()", "state.echoFloat(Infinity)", "state.echoFloat(1e300)"] {
            assert!(runtime.eval(source).unwrap_err().contains("TypeError"), "{source}");
        }

        assert_eq!(runtime.eval_value("state.optionalEnabled").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalRatio").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalCount").unwrap(), Value::Null);
        runtime.eval("state.optionalEnabled = null; state.optionalRatio = null; state.optionalCount = null;").unwrap();
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_enabled, None);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_ratio, None);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_count, None);

        runtime.eval("state.optionalEnabled = 'false'; state.optionalRatio = '3.25'; state.optionalCount = -2;").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert_eq!(native.optional_enabled, Some(true));
        assert_eq!(native.optional_ratio, Some(3.25));
        assert_eq!(native.optional_count, Some(u32::MAX - 1));

        runtime.eval("state.optionalRatio = Symbol();").unwrap_err();
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().optional_ratio, Some(3.25));
        assert!(runtime.eval("state.optionalRatio = undefined;").unwrap_err().contains("TypeError"));
        runtime.eval("state.optionalEnabled = undefined; state.optionalCount = undefined;").unwrap();
        let native = runtime.get_wrapped::<State>(&handle).unwrap();
        assert_eq!(native.optional_enabled, Some(false));
        assert_eq!(native.optional_ratio, Some(3.25));
        assert_eq!(native.optional_count, Some(0));
        assert_eq!(runtime.eval_value("state.ping()").unwrap(), Value::Undefined);
        assert_eq!(runtime.eval_value("state.isEnabled()").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.currentRatio()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("state.currentCount()").unwrap(), Value::Number(1.0));
        assert_eq!(runtime.eval_value("state.accepts(true)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.accepts(1)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.accepts(0)").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.accepts()").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("state.accepts(true, false)").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.currentLabel().charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        assert_eq!(runtime.eval_value("state.currentLabel().charCodeAt(1)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("state.optionalLabel()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.currentUsvLabel()").unwrap(), Value::String("v8 ?".to_owned()));
        assert_eq!(runtime.eval_value("state.optionalUsvLabel()").unwrap(), Value::String("game".to_owned()));
        assert_eq!(runtime.eval_value("state.optionalEnabledResult()").unwrap(), Value::Bool(true));
        assert_eq!(runtime.eval_value("state.optionalRatioResult()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.optionalCountResult()").unwrap(), Value::Number(u32::MAX as f64));
        for (expression, expected) in [
            ("state.signedByteResult()", -7.0), ("state.octetResult()", 250.0),
            ("state.shortResult()", -300.0), ("state.unsignedShortResult()", 60_000.0),
            ("state.longResult()", -2_000_000.0), ("state.longLongResult()", i64::MAX as f64),
            ("state.unsignedLongLongResult()", u64::MAX as f64), ("state.floatResult()", 1.25),
            ("state.nullableFloatResult()", -2.5),
        ] {
            assert_eq!(runtime.eval_value(expression).unwrap(), Value::Number(expected), "{expression}");
        }
        assert_eq!(runtime.eval_value("state.nullableLongLongResult()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.currentRatio()").unwrap(), Value::Number(2.5));
        assert!(runtime.eval("state.currentUnrestrictedRatio() === Infinity").unwrap() == "true");
        assert!(runtime.eval("Number.isNaN(state.optionalUnrestrictedRatioResult())").unwrap() == "true");
        assert!(runtime.eval("state.currentUnrestrictedFloat() === Infinity").unwrap() == "true");
        assert!(runtime.eval("Number.isNaN(state.optionalUnrestrictedFloatResult())").unwrap() == "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_required_string_operation_arguments_preserve_webidl_semantics() {
        use crate::webidl::string_operations::{StringOperationsBinding, StringOperationsNative};
        struct EchoStrings;
        #[allow(non_snake_case)]
        impl StringOperationsNative for EchoStrings {
            fn EchoDom(&self, value: Vec<u16>) -> Vec<u16> { value }
            fn EchoUsv(&self, value: String) -> String { value }
        }

        let mut runtime = Runtime::new();
        let binding = StringOperationsBinding::<EchoStrings>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, EchoStrings);
        runtime.set_global_property("strings", &handle).unwrap();
        assert_eq!(runtime.eval_value("strings.echoDom(123)").unwrap(), Value::String("123".into()));
        assert_eq!(runtime.eval_value("strings.echoDom({ toString() { return 'coerced'; } })").unwrap(), Value::String("coerced".into()));
        runtime.eval("globalThis.lone = '\\uD800';").unwrap();
        assert_eq!(runtime.eval_value("strings.echoDom(lone.charAt(0))").unwrap(), Value::Utf16String(vec![0xD800]));
        assert_eq!(runtime.eval_value("strings.echoUsv(lone.charAt(0))").unwrap(), Value::String("\u{FFFD}".into()));
        assert_eq!(runtime.eval_value("strings.echoDom()").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { strings.echoUsv(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval("(() => { try { strings.echoDom({ toString() { throw new RangeError('coercion'); } }); } catch (e) { return e instanceof RangeError && e.message === 'coercion'; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_nullable_operation_arguments_preserve_null_and_inner_conversions() {
        use crate::webidl::nullable_operations::{NullableOperationsBinding, NullableOperationsNative};
        struct NullableValues;
        #[allow(non_snake_case)]
        impl NullableOperationsNative for NullableValues {
            fn Flag(&self, value: Option<bool>) -> Option<bool> { value }
            fn Count(&self, value: Option<i32>) -> Option<i32> { value }
            fn Label(&self, value: Option<Vec<u16>>) -> Option<Vec<u16>> { value }
            fn Name(&self, value: Option<String>) -> Option<String> { value }
            fn Mix(&self, value: Option<i32>, addend: i32) -> Option<i32> { value.map(|value| value + addend) }
        }

        let mut runtime = Runtime::new();
        let binding = NullableOperationsBinding::<NullableValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, NullableValues);
        runtime.set_global_property("values", &handle).unwrap();

        assert_eq!(runtime.eval_value("values.flag(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag()").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.flag(0)").unwrap(), Value::Bool(false));
        assert_eq!(runtime.eval_value("values.count(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.count(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.count('4294967295')").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.label(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.label(undefined)").unwrap(), Value::Null);
        runtime.eval("globalThis.lone = '\\uD800';").unwrap();
        assert_eq!(runtime.eval_value("values.label(lone)").unwrap(), Value::Utf16String(vec![0xD800]));
        assert_eq!(runtime.eval_value("values.name(lone)").unwrap(), Value::String("\u{FFFD}".into()));
        assert_eq!(runtime.eval_value("values.name({ toString() { return 'game'; } })").unwrap(), Value::String("game".into()));
        assert_eq!(runtime.eval_value("values.mix(null, 5)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.mix(7, 5)").unwrap(), Value::Number(12.0));
        assert_eq!(runtime.eval("(() => { try { values.count(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_arguments_distinguish_missing_from_nullable_null() {
        use crate::webidl::optional_operations::{OptionalOperationsBinding, OptionalOperationsNative};
        use crate::WebIdlOptionalArgument;
        struct OptionalValues;
        #[allow(non_snake_case)]
        impl OptionalOperationsNative for OptionalValues {
            fn Classify(&self, value: WebIdlOptionalArgument<Option<i32>>) -> Option<i32> {
                match value {
                    WebIdlOptionalArgument::Missing => Some(-1),
                    WebIdlOptionalArgument::Present(None) => None,
                    WebIdlOptionalArgument::Present(Some(value)) => Some(value),
                }
            }
            fn Fallback(&self, value: WebIdlOptionalArgument<i32>) -> i32 {
                match value {
                    WebIdlOptionalArgument::Missing => -1,
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
        }

        let mut runtime = Runtime::new();
        let binding = OptionalOperationsBinding::<OptionalValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, OptionalValues);
        runtime.set_global_property("values", &handle).unwrap();

        assert_eq!(runtime.eval_value("values.classify()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.classify(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.classify(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.classify('7')").unwrap(), Value::Number(7.0));
        assert_eq!(runtime.eval_value("values.fallback()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.fallback(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("values.fallback(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval("(() => { try { values.fallback(Symbol()); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_bytestring_arguments_preserve_bytes_and_enforce_code_unit_range() {
        use crate::webidl::bytestring_operations::{ByteStringOperationsBinding, ByteStringOperationsNative};
        use crate::WebIdlOptionalArgument;
        struct ByteValues;
        #[allow(non_snake_case)]
        impl ByteStringOperationsNative for ByteValues {
            fn Echo(&self, value: Vec<u8>) -> Vec<u16> {
                value.into_iter().map(u16::from).collect()
            }
            fn NullableEcho(&self, value: Option<Vec<u8>>) -> Option<Vec<u16>> {
                value.map(|value| value.into_iter().map(u16::from).collect())
            }
            fn OptionalEcho(&self, value: WebIdlOptionalArgument<Option<Vec<u8>>>) -> Option<Vec<u16>> {
                match value {
                    WebIdlOptionalArgument::Missing => Some("missing".encode_utf16().collect()),
                    WebIdlOptionalArgument::Present(None) => None,
                    WebIdlOptionalArgument::Present(Some(value)) => Some(value.into_iter().map(u16::from).collect()),
                }
            }
        }

        let mut runtime = Runtime::new();
        let binding = ByteStringOperationsBinding::<ByteValues>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, ByteValues);
        runtime.set_global_property("values", &handle).unwrap();

        runtime.eval("globalThis.bytes = values.echo('A\\u00FF');").unwrap();
        assert_eq!(runtime.eval_value("bytes.charCodeAt(0)").unwrap(), Value::Number(65.0));
        assert_eq!(runtime.eval_value("bytes.charCodeAt(1)").unwrap(), Value::Number(255.0));
        assert_eq!(runtime.eval_value("values.nullableEcho(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.nullableEcho(undefined)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.optionalEcho()").unwrap(), Value::String("missing".into()));
        assert_eq!(runtime.eval_value("values.optionalEcho(undefined)").unwrap(), Value::String("missing".into()));
        assert_eq!(runtime.eval_value("values.optionalEcho(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("values.optionalEcho('\\u00FE')").unwrap(), Value::String("þ".into()));
        assert!(runtime.eval("values.echo('\\u0100')").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("values.echo('\\uD800')").unwrap_err().contains("TypeError"));
        assert!(runtime.eval("values.echo(Symbol())").unwrap_err().contains("TypeError"));
        assert_eq!(runtime.eval("(() => { try { values.echo({ toString() { throw new RangeError('bytes'); } }); } catch (e) { return e instanceof RangeError && e.message === 'bytes'; } })()").unwrap(), "true");
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_defaults_apply_only_to_missing_or_undefined_arguments() {
        use crate::webidl::optional_defaults::{OptionalDefaultsBinding, OptionalDefaultsNative};
        use crate::{FiniteF64, WebIdlOptionalArgument};
        struct Defaults;
        #[allow(non_snake_case)]
        impl OptionalDefaultsNative for Defaults {
            fn BooleanDefault(&self, value: WebIdlOptionalArgument<bool>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -1, WebIdlOptionalArgument::Present(value) => i32::from(value) }
            }
            fn IntegerDefault(&self, value: WebIdlOptionalArgument<i32>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -1, WebIdlOptionalArgument::Present(value) => value }
            }
            fn UnsignedDefault(&self, value: WebIdlOptionalArgument<u32>) -> u32 {
                match value { WebIdlOptionalArgument::Missing => u32::MAX, WebIdlOptionalArgument::Present(value) => value }
            }
            fn FiniteFloatDefault(&self, value: WebIdlOptionalArgument<crate::FiniteF32>) -> crate::FiniteF32 {
                match value { WebIdlOptionalArgument::Missing => crate::FiniteF32::new(-1.0).unwrap(), WebIdlOptionalArgument::Present(value) => value }
            }
            fn FiniteDefault(&self, value: WebIdlOptionalArgument<FiniteF64>) -> FiniteF64 {
                match value { WebIdlOptionalArgument::Missing => FiniteF64::new(-1.0).unwrap(), WebIdlOptionalArgument::Present(value) => value }
            }
            fn UnrestrictedFloatDefault(&self, value: WebIdlOptionalArgument<f32>) -> f32 {
                match value { WebIdlOptionalArgument::Missing => f32::NEG_INFINITY, WebIdlOptionalArgument::Present(value) => value }
            }
            fn InfinityDefault(&self, value: WebIdlOptionalArgument<f64>) -> f64 {
                match value { WebIdlOptionalArgument::Missing => f64::NEG_INFINITY, WebIdlOptionalArgument::Present(value) => value }
            }
            fn NullableDefault(&self, value: WebIdlOptionalArgument<Option<i32>>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -2, WebIdlOptionalArgument::Present(None) => -1, WebIdlOptionalArgument::Present(Some(value)) => value }
            }
            fn NullableValueDefault(&self, value: WebIdlOptionalArgument<Option<i32>>) -> i32 {
                match value { WebIdlOptionalArgument::Missing => -2, WebIdlOptionalArgument::Present(None) => -1, WebIdlOptionalArgument::Present(Some(value)) => value }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OptionalDefaultsBinding::<Defaults>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, Defaults);
        runtime.set_global_property("defaults", &handle).unwrap();
        assert_eq!(runtime.eval_value("defaults.booleanDefault()").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(undefined)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(true)").unwrap(), Value::Number(1.0));
        assert_eq!(runtime.eval_value("defaults.booleanDefault(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault()").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(undefined)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(9)").unwrap(), Value::Number(9.0));
        assert_eq!(runtime.eval_value("defaults.integerDefault(null)").unwrap(), Value::Number(0.0));
        assert_eq!(runtime.eval_value("defaults.unsignedDefault()").unwrap(), Value::Number(6.0));
        assert_eq!(runtime.eval_value("defaults.finiteFloatDefault()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("defaults.finiteDefault()").unwrap(), Value::Number(1.5));
        assert_eq!(runtime.eval_value("defaults.unrestrictedFloatDefault()").unwrap(), Value::Number(2.5));
        assert_eq!(runtime.eval_value("defaults.infinityDefault() ").unwrap(), Value::Number(f64::INFINITY));
        assert_eq!(runtime.eval_value("defaults.nullableDefault()").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(undefined)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(null)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableDefault(4)").unwrap(), Value::Number(4.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault()").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(undefined)").unwrap(), Value::Number(3.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(null)").unwrap(), Value::Number(-1.0));
        assert_eq!(runtime.eval_value("defaults.nullableValueDefault(4)").unwrap(), Value::Number(4.0));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_optional_string_defaults_preserve_domstring_usvstring_and_bytestring() {
        use crate::webidl::optional_string_defaults::{
            OptionalStringDefaultsBinding, OptionalStringDefaultsNative,
        };
        use crate::WebIdlOptionalArgument;
        struct StringDefaults;
        #[allow(non_snake_case)]
        impl OptionalStringDefaultsNative for StringDefaults {
            fn Dom(&self, value: WebIdlOptionalArgument<Vec<u16>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Usv(&self, value: WebIdlOptionalArgument<String>) -> String {
                match value {
                    WebIdlOptionalArgument::Missing => String::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Unicode(&self, value: WebIdlOptionalArgument<Vec<u16>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Nullable(&self, value: WebIdlOptionalArgument<Option<Vec<u16>>>) -> Option<Vec<u16>> {
                match value {
                    WebIdlOptionalArgument::Missing => None,
                    WebIdlOptionalArgument::Present(value) => value,
                }
            }
            fn Bytes(&self, value: WebIdlOptionalArgument<Vec<u8>>) -> Vec<u16> {
                match value {
                    WebIdlOptionalArgument::Missing => Vec::new(),
                    WebIdlOptionalArgument::Present(value) => value.into_iter().map(u16::from).collect(),
                }
            }
        }
        let mut runtime = Runtime::new();
        let binding = OptionalStringDefaultsBinding::<StringDefaults>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, StringDefaults);
        runtime.set_global_property("defaults", &handle).unwrap();
        let escaped_dom = Value::String("line\\nquote".into());
        assert_eq!(runtime.eval_value("defaults.dom()").unwrap(), escaped_dom);
        assert_eq!(runtime.eval_value("defaults.dom(undefined)").unwrap(), Value::String("line\\nquote".into()));
        assert_eq!(runtime.eval_value("defaults.dom('explicit')").unwrap(), Value::String("explicit".into()));
        assert_eq!(runtime.eval_value("defaults.usv()").unwrap(), Value::String("rocket \u{1f680}".into()));
        assert_eq!(runtime.eval_value("defaults.unicode()").unwrap(), Value::String("rocket \u{1f680}".into()));
        assert_eq!(runtime.eval_value("defaults.nullable()").unwrap(), Value::String("seed".into()));
        assert_eq!(runtime.eval_value("defaults.nullable(undefined)").unwrap(), Value::String("seed".into()));
        assert_eq!(runtime.eval_value("defaults.nullable(null)").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("defaults.nullable('explicit')").unwrap(), Value::String("explicit".into()));
        assert_eq!(runtime.eval_value("defaults.bytes()").unwrap(), Value::String("abc".into()));
        assert_eq!(runtime.eval_value("defaults.bytes(undefined)").unwrap(), Value::String("abc".into()));
        assert_eq!(runtime.eval_value("defaults.bytes('XYZ')").unwrap(), Value::String("XYZ".into()));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_nullable_domstring_preserves_null_and_utf16_semantics() {
        use crate::webidl::nullable_domstring::{NullableDomStringBinding, NullableDomStringNative};
        #[derive(Default)]
        struct State(Option<Vec<u16>>);
        #[allow(non_snake_case)]
        impl NullableDomStringNative for State {
            fn InitialValue(&self) -> Option<Vec<u16>> { None }
            fn Value(&self) -> Option<Vec<u16>> { self.0.clone() }
            fn set_Value(&mut self, value: Option<Vec<u16>>) { self.0 = value; }
        }

        let mut runtime = Runtime::new();
        let binding = NullableDomStringBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        assert_eq!(runtime.eval_value("state.initialValue").unwrap(), Value::Null);
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Null);

        runtime.eval("state.value = 'hello';").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("hello".into()));
        runtime.eval("state.value = '\\uD800x';").unwrap();
        assert_eq!(runtime.eval_value("state.value.charCodeAt(0)").unwrap(), Value::Number(0xD800 as f64));
        runtime.eval("state.value = null;").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::Null);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().0, None);

        runtime.eval("state.value = undefined;").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { state.value = Symbol(); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("undefined".into()));
    }

    #[cfg(feature = "webidl-pilot")]
    #[test]
    fn generated_usvstring_replaces_unpaired_surrogates_and_preserves_nullable_values() {
        use crate::webidl::usv_strings::{UsvStringsBinding, UsvStringsNative};
        #[derive(Default)]
        struct State { value: String, nullable: Option<String> }
        #[allow(non_snake_case)]
        impl UsvStringsNative for State {
            fn Value(&self) -> String { self.value.clone() }
            fn set_Value(&mut self, value: String) { self.value = value; }
            fn Nullable(&self) -> Option<String> { self.nullable.clone() }
            fn set_Nullable(&mut self, value: Option<String>) { self.nullable = value; }
            fn InitialValue(&self) -> Option<String> { None }
        }

        let mut runtime = Runtime::new();
        let binding = UsvStringsBinding::<State>::install(&mut runtime).unwrap();
        let handle = binding.create(&mut runtime, State::default());
        runtime.set_global_property("state", &handle).unwrap();
        assert_eq!(runtime.eval_value("state.initialValue").unwrap(), Value::Null);

        runtime.eval("state.value = '\\uD800x'; state.nullable = '\\uDC00y';").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("\u{FFFD}x".into()));
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("\u{FFFD}y".into()));
        runtime.eval("state.value = { toString() { return '\\uD800'; } };").unwrap();
        assert_eq!(runtime.eval_value("state.value").unwrap(), Value::String("\u{FFFD}".into()));

        runtime.eval("state.nullable = null;").unwrap();
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::Null);
        assert_eq!(runtime.get_wrapped::<State>(&handle).unwrap().nullable, None);
        runtime.eval("state.nullable = undefined;").unwrap();
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("undefined".into()));
        assert_eq!(runtime.eval("(() => { try { state.nullable = Symbol(); } catch (e) { return e instanceof TypeError; } })()").unwrap(), "true");
        assert_eq!(runtime.eval_value("state.nullable").unwrap(), Value::String("undefined".into()));
    }

    #[test]
    fn interfaces_without_constructors_and_foreign_method_receivers_throw() {
        fn method(_: &dyn std::any::Any, _: &[Value]) -> Value { Value::Undefined }
        let mut runtime = Runtime::new();
        let interface = runtime.define_interface("Native", None);
        runtime.define_method(&interface, "method", method).unwrap();
        let node = runtime.create_instance(&interface, ());
        runtime.set_global_property("node", &node).unwrap();
        for expression in ["Native()", "new Native()", "node.method.call({})", "new node.method()"] {
            assert!(runtime.eval(expression).unwrap_err().contains("TypeError"), "{expression}");
        }
    }

}
