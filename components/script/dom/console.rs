/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::convert::TryFrom;
use std::ptr::{self, NonNull};
use std::slice;

use embedder_traits::{ConsoleLogLevel, EmbedderMsg};
use js::context::JSContext;
use js::conversions::jsstr_to_string;
use js::jsapi::{self, ESClass, PropertyDescriptor};
use js::jsval::{Int32Value, UndefinedValue};
use js::realm::CurrentRealm;
use js::rust::wrappers2::{
    GetArrayLength, GetBuiltinClass, GetPropertyKeys, JS_ClearPendingException,
    JS_GetOwnPropertyDescriptorById, JS_GetPropertyById, JS_IdToValue, JS_Stringify,
    JS_ValueToSource,
};
use js::rust::{HandleObject, HandleValue, IdVector, ToNumber, ToString};
use script_bindings::conversions::get_dom_class;

use crate::dom::bindings::codegen::Bindings::ConsoleBinding::consoleMethods;
use crate::dom::bindings::error::report_pending_exception;
use crate::dom::bindings::str::DOMString;
use crate::dom::globalscope::GlobalScope;

/// The maximum object depth logged by console methods.
const MAX_LOG_DEPTH: usize = 10;
/// The maximum elements in an object logged by console methods.
const MAX_LOG_CHILDREN: usize = 15;

/// <https://developer.mozilla.org/en-US/docs/Web/API/Console>
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
pub(crate) struct Console;

impl Console {
    /// Helper to send a message that only consists of a single string
    fn send_string_message(global: &GlobalScope, level: ConsoleLogLevel, message: String) {
        let prefix = global.current_group_label().unwrap_or_default();
        Self::send_to_embedder(global, level, format!("{prefix}{message}"));
    }

    fn method(
        cx: &mut JSContext,
        global: &GlobalScope,
        level: ConsoleLogLevel,
        messages: Vec<HandleValue>,
    ) {
        // If the first argument is a string, apply sprintf-style substitutions per the
        // WHATWG Console spec formatter. The result is a single formatted string followed
        // by any arguments that were not consumed by a substitution specifier.
        let embedder_msg = if !messages.is_empty() && messages[0].is_string() {
            let (formatted, consumed) = apply_sprintf_substitutions(cx, &messages);
            let remaining = &messages[consumed..];
            if remaining.is_empty() {
                formatted
            } else {
                format!("{formatted} {}", stringify_handle_values(cx, remaining))
            }
        } else {
            stringify_handle_values(cx, &messages).into()
        };

        Self::send_string_message(global, level, embedder_msg);
    }

    fn send_to_embedder(global: &GlobalScope, level: ConsoleLogLevel, message: String) {
        global.send_to_embedder(EmbedderMsg::ShowConsoleApiMessage(
            global.webview_id(),
            level,
            message,
        ));
    }

    // Directly logs a string message, without processing the message
    pub(crate) fn internal_warn(_cx: &mut JSContext, global: &GlobalScope, message: String) {
        Console::send_string_message(global, ConsoleLogLevel::Warn, message);
    }
}

#[expect(unsafe_code)]
fn handle_value_to_string(cx: &mut JSContext, value: HandleValue) -> DOMString {
    match std::ptr::NonNull::new(unsafe { JS_ValueToSource(cx, value) }) {
        Some(js_str) => unsafe { jsstr_to_string(cx, js_str) }.into(),
        None => "<error converting value to string>".into(),
    }
}

/// An implementation of <https://console.spec.whatwg.org/#printer> for `console.dir`: the
/// value with generic JavaScript object formatting applied.
fn dir_string_from_handle_value(
    cx: &mut JSContext,
    handle_value: HandleValue,
    seen: &mut Vec<u64>,
) -> String {
    #[expect(unsafe_code)]
    fn inner(
        cx: &mut JSContext,
        handle_value: HandleValue,
        seen: &mut Vec<u64>,
    ) -> Result<String, ()> {
        if handle_value.is_string() {
            let js_string = ptr::NonNull::new(handle_value.to_string()).unwrap();
            return Ok(unsafe { jsstr_to_string(cx, js_string) });
        }

        if handle_value.is_number() {
            return Ok(handle_value.to_number().to_string());
        }

        if handle_value.is_boolean() {
            return Ok(handle_value.to_boolean().to_string());
        }

        if handle_value.is_object() {
            // JS objects can create circular reference, and we want to avoid recursing infinitely
            if seen.contains(&handle_value.asBits_) {
                // FIXME: Handle this properly
                return Ok("[circular]".into());
            }

            seen.push(handle_value.asBits_);
            let object_string = dir_string_from_object(cx, handle_value, seen);
            let js_value = seen.pop();
            debug_assert_eq!(js_value, Some(handle_value.asBits_));

            return object_string.ok_or(());
        }

        // FIXME: Handle more complex argument types here
        Ok(stringify_handle_value(cx, handle_value).into())
    }

    match inner(cx, handle_value, seen) {
        Ok(string) => string,
        Err(()) => {
            report_pending_exception(&mut CurrentRealm::assert(cx));
            "<error>".into()
        },
    }
}

fn accessor_string_from_property_descriptor(descriptor: &PropertyDescriptor) -> String {
    // https://console.spec.whatwg.org/#printer
    // Objects with either generic JavaScript object formatting or optimally useful formatting applied.
    match (
        descriptor.hasGetter_() && !descriptor.getter_.is_null(),
        descriptor.hasSetter_() && !descriptor.setter_.is_null(),
    ) {
        (true, true) => "Getter/Setter",
        (true, false) => "Getter",
        (false, true) => "Setter",
        (false, false) => "undefined",
    }
    .into()
}

/// Formats a plain object, array, function or map for `console.dir`: arrays as
/// `[item, ...]`, everything else as `Class {key: value, ...}`, with at most
/// [`MAX_LOG_CHILDREN`] entries. Returns `None` for other objects or on a JS error.
#[expect(unsafe_code)]
fn dir_string_from_object(
    cx: &mut JSContext,
    handle_value: HandleValue,
    seen: &mut Vec<u64>,
) -> Option<String> {
    rooted!(&in(cx) let object = handle_value.to_object());
    let mut object_class = ESClass::Other;
    if !unsafe { GetBuiltinClass(cx, object.handle(), &mut object_class as *mut _) } {
        return None;
    }
    let class = match object_class {
        ESClass::Object => "Object",
        ESClass::Array => "Array",
        ESClass::Function => "Function",
        // Map entries are not printed.
        ESClass::Map => return Some("Map {}".into()),
        _ => return None,
    };

    let mut ids = unsafe { IdVector::new(cx.raw_cx()) };
    // https://console.spec.whatwg.org/#printer
    // Objects with either generic JavaScript object formatting or optimally useful formatting applied.
    if !unsafe {
        GetPropertyKeys(
            cx,
            object.handle(),
            jsapi::JSITER_OWNONLY | jsapi::JSITER_SYMBOLS | jsapi::JSITER_HIDDEN,
            ids.handle_mut(),
        )
    } {
        return None;
    }

    // Own properties as (key, value) pairs and, for arrays, indexed items as (index, value).
    let mut properties = Vec::new();
    let mut items = Vec::new();
    for id in ids.iter() {
        rooted!(&in(cx) let id = *id);
        rooted!(&in(cx) let mut descriptor = PropertyDescriptor::default());

        let mut is_none = false;
        if !unsafe {
            JS_GetOwnPropertyDescriptorById(
                cx,
                object.handle(),
                id.handle(),
                descriptor.handle_mut(),
                &mut is_none,
            )
        } {
            return None;
        }
        if is_none {
            continue;
        }

        let is_accessor = (descriptor.hasGetter_() && !descriptor.getter_.is_null()) ||
            (descriptor.hasSetter_() && !descriptor.setter_.is_null());
        let value = if is_accessor {
            accessor_string_from_property_descriptor(&descriptor)
        } else {
            rooted!(&in(cx) let property = descriptor.value_);
            dir_string_from_handle_value(cx, property.handle(), seen)
        };

        if object_class == ESClass::Array && id.is_int() {
            items.push((id.to_int(), value));
            continue;
        }

        let key = if id.is_string() {
            rooted!(&in(cx) let mut key_value = UndefinedValue());
            if !unsafe { JS_IdToValue(cx, id.handle().get(), key_value.handle_mut()) } {
                continue;
            }
            rooted!(&in(cx) let js_string = key_value.to_string());
            let Some(js_string) = NonNull::new(js_string.get()) else {
                continue;
            };
            unsafe { jsstr_to_string(cx, js_string) }
        } else if id.is_symbol() || id.is_int() {
            rooted!(&in(cx) let mut key_value = UndefinedValue());
            if !unsafe { JS_IdToValue(cx, id.handle().get(), key_value.handle_mut()) } {
                continue;
            }
            handle_value_to_string(cx, key_value.handle()).to_string()
        } else {
            continue;
        };
        properties.push(format!("{key}: {value}"));
    }

    if object_class == ESClass::Array {
        let mut length = 0u32;
        if !unsafe { GetArrayLength(cx, object.handle(), &mut length) } {
            return None;
        }
        items.sort_by_key(|(index, _)| *index);
        let mut strings: Vec<String> = items
            .into_iter()
            .take(MAX_LOG_CHILDREN)
            .map(|(_, value)| value)
            .collect();
        if length as usize > strings.len() {
            strings.push("...".into());
        }
        return Some(format!("[{}]", itertools::join(strings, ", ")));
    }

    let truncated = properties.len() > MAX_LOG_CHILDREN;
    properties.truncate(MAX_LOG_CHILDREN);
    if truncated {
        properties.push("...".into());
    }
    Some(format!("{class} {{{}}}", itertools::join(properties, ", ")))
}

#[expect(unsafe_code)]
pub(crate) fn stringify_handle_value(cx: &mut JSContext, message: HandleValue) -> DOMString {
    if message.is_string() {
        let jsstr = std::ptr::NonNull::new(message.to_string()).unwrap();
        return unsafe { jsstr_to_string(cx, jsstr) }.into();
    }
    fn stringify_object_from_handle_value(
        cx: &mut JSContext,
        value: HandleValue,
        parents: Vec<u64>,
    ) -> DOMString {
        rooted!(&in(cx) let mut obj = value.to_object());
        let mut object_class = ESClass::Other;
        if !unsafe { GetBuiltinClass(cx, obj.handle(), &mut object_class as *mut _) } {
            return DOMString::from("/* invalid */");
        }
        let mut ids = unsafe { IdVector::new(cx.raw_cx()) };
        if !unsafe {
            GetPropertyKeys(
                cx,
                obj.handle(),
                jsapi::JSITER_OWNONLY | jsapi::JSITER_SYMBOLS,
                ids.handle_mut(),
            )
        } {
            return DOMString::from("/* invalid */");
        }
        let truncate = ids.len() > MAX_LOG_CHILDREN;
        if object_class != ESClass::Array && object_class != ESClass::Object {
            if truncate {
                return DOMString::from("…");
            } else {
                return handle_value_to_string(cx, value);
            }
        }

        let mut explicit_keys = object_class == ESClass::Object;
        let mut props = Vec::with_capacity(ids.len());
        for id in ids.iter().take(MAX_LOG_CHILDREN) {
            rooted!(&in(cx) let id = *id);
            rooted!(&in(cx) let mut desc = PropertyDescriptor::default());

            let mut is_none = false;
            if !unsafe {
                JS_GetOwnPropertyDescriptorById(
                    cx,
                    obj.handle(),
                    id.handle(),
                    desc.handle_mut(),
                    &mut is_none,
                )
            } {
                return DOMString::from("/* invalid */");
            }

            rooted!(&in(cx) let mut property = UndefinedValue());
            if !unsafe { JS_GetPropertyById(cx, obj.handle(), id.handle(), property.handle_mut()) }
            {
                return DOMString::from("/* invalid */");
            }

            if !explicit_keys {
                if id.is_int() {
                    if let Ok(id_int) = usize::try_from(id.to_int()) {
                        explicit_keys = props.len() != id_int;
                    } else {
                        explicit_keys = false;
                    }
                } else {
                    explicit_keys = false;
                }
            }
            let value_string = stringify_inner(cx, property.handle(), parents.clone());
            if explicit_keys {
                let key = if id.is_string() || id.is_symbol() || id.is_int() {
                    rooted!(&in(cx) let mut key_value = UndefinedValue());
                    if !unsafe { JS_IdToValue(cx, id.handle().get(), key_value.handle_mut()) } {
                        return DOMString::from("/* invalid */");
                    }
                    handle_value_to_string(cx, key_value.handle())
                } else {
                    return DOMString::from("/* invalid */");
                };
                props.push(format!("{}: {}", key, value_string,));
            } else {
                props.push(String::from(value_string));
            }
        }
        if truncate {
            props.push("…".to_string());
        }
        if object_class == ESClass::Array {
            DOMString::from(format!("[{}]", itertools::join(props, ", ")))
        } else {
            DOMString::from(format!("{{{}}}", itertools::join(props, ", ")))
        }
    }
    fn stringify_inner(cx: &mut JSContext, value: HandleValue, mut parents: Vec<u64>) -> DOMString {
        if parents.len() >= MAX_LOG_DEPTH {
            return DOMString::from("...");
        }
        let value_bits = value.asBits_;
        if parents.contains(&value_bits) {
            return DOMString::from("[circular]");
        }
        if value.is_undefined() {
            // This produces a better value than "(void 0)" from JS_ValueToSource.
            return DOMString::from("undefined");
        } else if !value.is_object() {
            return handle_value_to_string(cx, value);
        }
        parents.push(value_bits);

        if value.is_object() &&
            let Some(repr) = maybe_stringify_dom_object(cx, value)
        {
            return repr;
        }
        stringify_object_from_handle_value(cx, value, parents)
    }
    stringify_inner(cx, message, Vec::new())
}

#[expect(unsafe_code)]
fn maybe_stringify_dom_object(cx: &mut JSContext, value: HandleValue) -> Option<DOMString> {
    // The standard object serialization is not effective for DOM objects,
    // since their properties generally live on the prototype object.
    // Instead, fall back to the output of JSON.stringify combined
    // with the class name extracted from the output of toString().
    rooted!(&in(cx) let obj = value.to_object());
    let is_dom_class = unsafe { get_dom_class(obj.get()).is_ok() };
    if !is_dom_class {
        return None;
    }
    rooted!(&in(cx) let class_name = unsafe { ToString(cx, value) });
    let Some(class_name) = NonNull::new(class_name.get()) else {
        return Some("<error converting DOM object to string>".into());
    };
    let class_name = unsafe { jsstr_to_string(cx, class_name) }
        .replace("[object ", "")
        .replace("]", "");
    let mut repr = format!("{} ", class_name);
    rooted!(&in(cx) let mut value = value.get());

    #[expect(unsafe_code)]
    unsafe extern "C" fn stringified(
        string: *const u16,
        len: u32,
        data: *mut std::ffi::c_void,
    ) -> bool {
        let s = data as *mut String;
        let string_chars = unsafe { slice::from_raw_parts(string, len as usize) };
        unsafe { (*s).push_str(&String::from_utf16_lossy(string_chars)) };
        true
    }

    rooted!(&in(cx) let space = Int32Value(2));
    let stringify_result = unsafe {
        JS_Stringify(
            cx,
            value.handle_mut(),
            HandleObject::null(),
            space.handle(),
            Some(stringified),
            &mut repr as *mut String as *mut _,
        )
    };
    if !stringify_result {
        return Some("<error converting DOM object to string>".into());
    }
    Some(repr.into())
}

/// Apply sprintf-style substitutions to console format arguments per the WHATWG Console spec.
///
/// If the first argument is a string, it is treated as a format string where `%s`, `%d`, `%i`,
/// `%f`, `%o`, `%O`, and `%c` are replaced by subsequent arguments. Returns the formatted string
/// and the index of the first argument that was not consumed by a substitution.
///
/// <https://console.spec.whatwg.org/#formatter>
#[expect(unsafe_code)]
fn apply_sprintf_substitutions(cx: &mut JSContext, messages: &[HandleValue]) -> (String, usize) {
    debug_assert!(!messages.is_empty() && messages[0].is_string());

    let js_string = ptr::NonNull::new(messages[0].to_string()).unwrap();
    let format_string = unsafe { jsstr_to_string(cx, js_string) };

    let mut result = String::new();
    let mut arg_index = 1usize;
    let mut chars = format_string.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }

        match chars.peek().copied() {
            Some('s') => {
                chars.next();
                if arg_index < messages.len() {
                    result.push_str(&stringify_handle_value(cx, messages[arg_index]).str());
                    arg_index += 1;
                } else {
                    result.push_str("%s");
                }
            },
            Some('d') | Some('i') => {
                let spec = chars.next().unwrap();
                if arg_index < messages.len() {
                    let num = unsafe { ToNumber(cx.raw_cx(), messages[arg_index]) };
                    if num.is_err() {
                        unsafe { JS_ClearPendingException(cx) };
                    }
                    arg_index += 1;
                    format_integer_substitution(&mut result, num);
                } else {
                    result.push('%');
                    result.push(spec);
                }
            },
            Some('f') => {
                chars.next();
                if arg_index < messages.len() {
                    let num = unsafe { ToNumber(cx.raw_cx(), messages[arg_index]) };
                    if num.is_err() {
                        unsafe { JS_ClearPendingException(cx) };
                    }
                    arg_index += 1;
                    format_float_substitution(&mut result, num);
                } else {
                    result.push_str("%f");
                }
            },
            Some('o') | Some('O') => {
                let spec = chars.next().unwrap();
                if arg_index < messages.len() {
                    result.push_str(&stringify_handle_value(cx, messages[arg_index]).str());
                    arg_index += 1;
                } else {
                    result.push('%');
                    result.push(spec);
                }
            },
            Some('c') => {
                chars.next();
                if arg_index < messages.len() {
                    arg_index += 1; // consume but ignore CSS styling
                }
            },
            Some('%') => {
                chars.next();
                result.push('%');
            },
            _ => {
                result.push('%');
            },
        }
    }

    (result, arg_index)
}

fn format_integer_substitution(result: &mut String, num: Result<f64, ()>) {
    match num {
        Ok(n) if n.is_nan() => result.push_str("NaN"),
        Ok(n) if n == f64::INFINITY => result.push_str("Infinity"),
        Ok(n) if n == f64::NEG_INFINITY => result.push_str("-Infinity"),
        Ok(n) => result.push_str(&(n.trunc() as i64).to_string()),
        Err(_) => result.push_str("NaN"),
    }
}

fn format_float_substitution(result: &mut String, num: Result<f64, ()>) {
    match num {
        Ok(n) if n.is_nan() => result.push_str("NaN"),
        Ok(n) if n == f64::INFINITY => result.push_str("Infinity"),
        Ok(n) if n == f64::NEG_INFINITY => result.push_str("-Infinity"),
        Ok(n) => result.push_str(&n.to_string()),
        Err(_) => result.push_str("NaN"),
    }
}

fn stringify_handle_values(cx: &mut JSContext, messages: &[HandleValue]) -> DOMString {
    DOMString::from(itertools::join(
        messages
            .iter()
            .copied()
            .map(|msg| stringify_handle_value(cx, msg)),
        " ",
    ))
}

impl consoleMethods<crate::DomTypeHolder> for Console {
    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/log>
    fn Log(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Log, messages);
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/clear>
    fn Clear(_global: &GlobalScope) {
        // A log cannot be cleared.
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console>
    fn Debug(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Debug, messages);
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/info>
    fn Info(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Info, messages);
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/warn>
    fn Warn(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Warn, messages);
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/error>
    fn Error(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Error, messages);
    }

    /// <https://console.spec.whatwg.org/#trace>
    fn Trace(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        Console::method(cx, global, ConsoleLogLevel::Trace, messages);
    }

    /// <https://console.spec.whatwg.org/#dir>
    fn Dir(
        cx: &mut js::context::JSContext,
        global: &GlobalScope,
        item: HandleValue,
        _options: Option<*mut jsapi::JSObject>,
    ) {
        // Step 1. Let object be item with generic JavaScript object formatting applied.
        let object = dir_string_from_handle_value(cx, item, &mut Vec::new());
        // Step 2. Perform Printer("dir", « object », options).
        Console::send_string_message(global, ConsoleLogLevel::Dir, object);
    }

    /// <https://developer.mozilla.org/en-US/docs/Web/API/Console/assert>
    fn Assert(
        cx: &mut JSContext,
        global: &GlobalScope,
        condition: bool,
        messages: Vec<HandleValue>,
    ) {
        if !condition {
            let message = format!(
                "Assertion failed: {}",
                stringify_handle_values(cx, &messages)
            );

            Console::send_string_message(global, ConsoleLogLevel::Log, message);
        }
    }

    /// <https://console.spec.whatwg.org/#time>
    fn Time(global: &GlobalScope, label: DOMString) {
        if let Ok(()) = global.time(label.clone()) {
            let message = format!("{label}: timer started");
            Console::send_string_message(global, ConsoleLogLevel::Log, message);
        }
    }

    /// <https://console.spec.whatwg.org/#timelog>
    fn TimeLog(cx: &mut JSContext, global: &GlobalScope, label: DOMString, data: Vec<HandleValue>) {
        if let Ok(delta) = global.time_log(&label) {
            let message = format!("{label}: {delta}ms {}", stringify_handle_values(cx, &data));

            Console::send_string_message(global, ConsoleLogLevel::Log, message);
        }
    }

    /// <https://console.spec.whatwg.org/#timeend>
    fn TimeEnd(global: &GlobalScope, label: DOMString) {
        if let Ok(delta) = global.time_end(&label) {
            let message = format!("{label}: {delta}ms");

            Console::send_string_message(global, ConsoleLogLevel::Log, message);
        }
    }

    /// <https://console.spec.whatwg.org/#group>
    fn Group(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        global.push_console_group(stringify_handle_values(cx, &messages));
    }

    /// <https://console.spec.whatwg.org/#groupcollapsed>
    fn GroupCollapsed(cx: &mut JSContext, global: &GlobalScope, messages: Vec<HandleValue>) {
        global.push_console_group(stringify_handle_values(cx, &messages));
    }

    /// <https://console.spec.whatwg.org/#groupend>
    fn GroupEnd(global: &GlobalScope) {
        global.pop_console_group();
    }

    /// <https://console.spec.whatwg.org/#count>
    fn Count(global: &GlobalScope, label: DOMString) {
        let count = global.increment_console_count(&label);
        let message = format!("{label}: {count}");

        Console::send_string_message(global, ConsoleLogLevel::Log, message);
    }

    /// <https://console.spec.whatwg.org/#countreset>
    fn CountReset(cx: &mut JSContext, global: &GlobalScope, label: DOMString) {
        if global.reset_console_count(&label).is_err() {
            Self::internal_warn(cx, global, format!("Counter “{label}” doesn’t exist."))
        }
    }
}
