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
}

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
        Runtime { isolate, context }
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
    use super::{NativeFunction, Runtime, Value};

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
}
