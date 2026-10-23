/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Preserve dynamic Function arguments until Servo has checked their TrustedScript brands.
//! V8's compilation callback alone only receives the already-composed source.

use crate::jsapi::{JSContext, JSString};
use crate::jsval::{JSVal, from_v8};

pub(crate) struct FunctionArguments<'a> {
    pub(crate) parameters: &'a [*mut JSString],
    pub(crate) arguments: &'a [JSVal],
    pub(crate) body: *mut JSString,
    pub(crate) body_argument: JSVal,
}

const INSTALL: &str = r#"(function(handlers) {
  const define = Object.defineProperty;
  const descriptor = Object.getOwnPropertyDescriptor;
  const getPrototype = Object.getPrototypeOf;
  const setPrototype = Object.setPrototypeOf;
  const constructors = [Function, getPrototype(function*(){}).constructor,
    getPrototype(async function(){}).constructor, getPrototype(async function*(){}).constructor];
  // Trap-less [[GetPrototypeOf]] and [[PreventExtensions]] forward to the targets,
  // so Proxy invariants hold whatever scripts later do to the constructors.
  // The traps are native: a JS trap frame would become the eval origin of the
  // compiled code (and so the referrer of its dynamic imports) instead of the caller.
  const wrappers = constructors.map((target, kind) => new Proxy(target, handlers[kind]));
  for (let kind = 0; kind < constructors.length; ++kind) {
    const target = constructors[kind];
    // %GeneratorFunction% and its siblings inherit from %Function% (ECMA-262
    // 27.3.2, 27.4.2, 27.7.2); relink them so the original never leaks.
    if (kind !== 0) {
      if (getPrototype(target) !== Function) throw new TypeError('unexpected constructor parent');
      setPrototype(target, wrappers[0]);
    }
    const property = descriptor(target.prototype, 'constructor');
    property.value = wrappers[kind];
    define(target.prototype, 'constructor', property);
  }
  globalThis.Function = wrappers[0];
  return true;
})"#;

pub(crate) fn install(scope: &mut v8::PinScope) -> Option<()> {
    crate::realm_impl::realm_cached(scope, "DynamicFunctionCsp", |scope| {
        let global = scope.get_current_context().global(scope);
        let reflect_key = v8::String::new(scope, "Reflect")?;
        let reflect = global.get(scope, reflect_key.into())?.to_object(scope)?;
        let construct_key = v8::String::new(scope, "construct")?;
        let construct = reflect.get(scope, construct_key.into())?;
        let error_key = v8::String::new(scope, "EvalError")?;
        let error = global.get(scope, error_key.into())?;
        let apply_key = v8::String::new(scope, "apply")?;
        let construct_key = v8::String::new(scope, "construct")?;
        let mut handlers = Vec::new();
        for kind in 0..4 {
            let kind = v8::Integer::new(scope, kind).into();
            let data = v8::Array::new_with_elements(scope, &[construct, error, kind]);
            let handler = v8::Object::new(scope);
            let apply = v8::Function::builder(apply_trap).data(data.into()).build(scope)?;
            handler.set(scope, apply_key.into(), apply.into())?;
            let construct = v8::Function::builder(construct_trap).data(data.into()).build(scope)?;
            handler.set(scope, construct_key.into(), construct.into())?;
            handlers.push(handler.into());
        }
        let handlers = v8::Array::new_with_elements(scope, &handlers);
        let source = v8::String::new(scope, INSTALL)?;
        let installer = v8::Script::compile(scope, source, None)?.run(scope)?;
        let installer = v8::Local::<v8::Function>::try_from(installer).ok()?;
        let undefined = v8::undefined(scope).into();
        installer.call(scope, undefined, &[handlers.into()])
    })?;
    Some(())
}

/// Proxy `apply(target, receiver, arguments)` trap.
fn apply_trap<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut result: v8::ReturnValue<'s>) {
    let undefined = v8::undefined(scope).into();
    if let Some(value) = compile(scope, &args, args.get(1), undefined, args.get(2)) {
        result.set(value);
    }
}

/// Proxy `construct(target, arguments, newTarget)` trap.
fn construct_trap<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, mut result: v8::ReturnValue<'s>) {
    let undefined = v8::undefined(scope).into();
    if let Some(value) = compile(scope, &args, undefined, args.get(2), args.get(1)) {
        result.set(value);
    }
}

fn compile<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    receiver: v8::Local<'s, v8::Value>,
    new_target: v8::Local<'s, v8::Value>,
    inputs: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Value>> {
    let raw = JSContext::current();
    let cx = raw as *const JSContext as *mut JSContext;
    let target = v8::Local::<v8::Function>::try_from(args.get(0)).ok()?;
    let inputs = v8::Local::<v8::Array>::try_from(inputs).ok()?;
    let data = v8::Local::<v8::Array>::try_from(args.data()).ok()?;
    crate::rooted!(in(cx) let mut originals = Vec::<JSVal>::new());
    crate::rooted!(in(cx) let mut strings = Vec::<*mut JSString>::new());
    let mut converted = Vec::<v8::Local<v8::Value>>::new();
    // HostGetCodeForEval reads a TrustedScript's real data before ToString. This
    // ignores forged coercion methods on branded objects, matching direct eval.
    for index in 0..inputs.length() {
        let input = inputs.get_index(scope, index)?;
        let original = from_v8(scope, input);
        originals.push(original);
        crate::rooted!(in(cx) let mut extracted = std::ptr::null_mut::<JSString>());
        if input.is_object() {
            crate::rooted!(in(cx) let object = original.to_object());
            if !crate::runtime_impl::function_argument_source(cx, object.handle().into(), extracted.handle_mut().into()) {
                crate::native::rethrow_pending(scope, raw);
                return None;
            }
        }
        let string = if extracted.get().is_null() {
            input.to_string(scope)?
        } else {
            let value = unsafe { crate::jsval::to_v8(scope, crate::jsval::StringValue(&*extracted.get())) };
            v8::Local::<v8::String>::try_from(value).ok()?
        };
        let string_value = from_v8(scope, string.into());
        strings.push(string_value.to_string());
        // Servo's TrustedScriptOrString arguments retain objects, while untrusted
        // primitive arguments are represented by their already-converted strings.
        if !input.is_object() && !input.is_string() {
            originals[index as usize] = string_value;
        }
        converted.push(string.into());
    }
    let prefix = match data.get_index(scope, 2)?.int32_value(scope)? {
        0 => "(function anonymous(",
        1 => "(function* anonymous(",
        2 => "(async function anonymous(",
        3 => "(async function* anonymous(",
        _ => return None,
    };
    let count = converted.len();
    let parameters = count.saturating_sub(1);
    let mut source = v8::String::new(scope, prefix)?;
    let comma = v8::String::new(scope, ",")?;
    for (index, parameter) in converted.iter().take(parameters).enumerate() {
        if index != 0 { source = v8::String::concat(scope, source, comma)?; }
        let parameter = v8::Local::<v8::String>::try_from(*parameter).ok()?;
        source = v8::String::concat(scope, source, parameter)?;
    }
    let separator = v8::String::new(scope, "\n) {\n")?;
    source = v8::String::concat(scope, source, separator)?;
    let body = match converted.last() {
        Some(body) => v8::Local::<v8::String>::try_from(*body).ok()?,
        None => v8::String::empty(scope),
    };
    source = v8::String::concat(scope, source, body)?;
    let end = v8::String::new(scope, "\n})")?;
    source = v8::String::concat(scope, source, end)?;
    crate::rooted!(in(cx) let body_value = from_v8(scope, body.into()));
    let function = FunctionArguments {
        parameters: &strings[..parameters],
        arguments: &originals[..parameters],
        body: body_value.get().to_string(),
        body_argument: originals.last().copied().unwrap_or(body_value.get()),
    };
    if !crate::runtime_impl::function_csp_allows(scope, source, &function) {
        if raw.pending_exception.borrow().is_some() {
            crate::native::rethrow_pending(scope, raw);
        } else {
            let message = v8::String::new(scope, "Code generation from strings disallowed for this context")?;
            let constructor = data.get_index(scope, 1)?;
            let constructor = v8::Local::<v8::Function>::try_from(constructor).ok()?;
            let error = constructor.new_instance(scope, &[message.into()])?;
            scope.throw_exception(error.into());
        }
        return None;
    }
    // Only the exact source just checked is authorized, once, during this native
    // constructor call. All ToString callbacks have finished before authorization.
    let _authorization = crate::runtime_impl::authorize_function(raw, scope, source);
    if new_target.is_undefined() {
        target.call(scope, receiver, &converted)
    } else {
        let construct = data.get_index(scope, 0)?;
        let construct = v8::Local::<v8::Function>::try_from(construct).ok()?;
        let arguments = v8::Array::new_with_elements(scope, &converted);
        let undefined = v8::undefined(scope).into();
        construct.call(scope, undefined, &[target.into(), arguments.into(), new_target])
    }
}
