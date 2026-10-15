/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Converts a JavaScript value into an embedder [`JSValue`], following the WebDriver
//! JSON clone algorithm. Used to return the result of `WebView::evaluate_javascript`.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::ptr::NonNull;

use embedder_traits::{
    JSValue, JavaScriptEvaluationError, JavaScriptEvaluationResultSerializationError,
};
use js::context::JSContext;
use js::conversions::{FromJSValConvertible, jsstr_to_string};
use js::jsapi::{HandleValueArray, JSITER_OWNONLY, JSType, PropertyDescriptor};
use js::jsval::UndefinedValue;
use js::realm::CurrentRealm;
use js::rust::wrappers2::{
    GetPropertyKeys, JS_CallFunctionName, JS_GetOwnPropertyDescriptorById, JS_GetProperty,
    JS_GetPropertyById, JS_HasOwnProperty, JS_IsExceptionPending, JS_TypeOfValue,
};
use js::rust::{HandleObject, HandleValue, IdVector, ToString};
use script_bindings::codegen::GenericBindings::ShadowRootBinding::ShadowRootMethods;
use script_bindings::conversions::is_array_like;
use script_bindings::settings_stack::run_a_script;

use crate::DomTypeHolder;
use crate::dom::bindings::conversions::{
    ConversionBehavior, ConversionResult, get_property, get_property_jsval, jsid_to_string,
    root_from_object,
};
use crate::dom::bindings::error::{Error, report_pending_exception, throw_dom_exception};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::element::Element;
use crate::dom::globalscope::GlobalScope;
use crate::dom::node::{Node, NodeTraits};
use crate::dom::types::ShadowRoot;
use crate::dom::window::Window;

type JSValueResult = Result<JSValue, JavaScriptEvaluationError>;

/// <https://w3c.github.io/webdriver/#dfn-is-stale>
fn is_stale(element: &Element) -> bool {
    // An element is stale if its node document is not the active document
    // or if it is not connected.
    !element.owner_document().is_active() || !element.is_connected()
}

/// <https://w3c.github.io/webdriver/#dfn-is-detached>
fn is_detached(shadow_root: &ShadowRoot) -> bool {
    // A shadow root is detached if its node document is not the active document
    // or if the element node referred to as its host is stale.
    !shadow_root.owner_document().is_active() || is_stale(&shadow_root.Host())
}

#[expect(unsafe_code)]
fn object_has_to_json_property(
    cx: &mut JSContext,
    global_scope: &GlobalScope,
    object: HandleObject,
) -> bool {
    let name = CString::new("toJSON").unwrap();
    let mut found = false;
    if unsafe { JS_HasOwnProperty(cx, object, name.as_ptr(), &mut found) } && found {
        rooted!(&in(cx) let mut value = UndefinedValue());
        let result = unsafe { JS_GetProperty(cx, object, name.as_ptr(), value.handle_mut()) };
        if !result {
            throw_dom_exception(cx, global_scope, Error::JSFailed);
            false
        } else {
            result && unsafe { JS_TypeOfValue(cx, value.handle()) } == JSType::JSTYPE_FUNCTION
        }
    } else if JS_IsExceptionPending(cx) {
        throw_dom_exception(cx, global_scope, Error::JSFailed);
        false
    } else {
        false
    }
}

#[expect(unsafe_code)]
/// <https://w3c.github.io/webdriver/#dfn-collection>
fn is_arguments_object(cx: &mut JSContext, value: HandleValue) -> bool {
    rooted!(&in(cx) let class_name = unsafe { ToString(cx, value) });
    let Some(class_name) = NonNull::new(class_name.get()) else {
        return false;
    };
    let class_name = unsafe { jsstr_to_string(cx, class_name) };
    class_name == "[object Arguments]"
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct HashableJSVal(u64);

impl From<HandleValue<'_>> for HashableJSVal {
    fn from(v: HandleValue<'_>) -> HashableJSVal {
        HashableJSVal(v.get().asBits_)
    }
}

/// <https://w3c.github.io/webdriver/#dfn-json-clone>
pub(crate) fn jsval_to_js_value(
    cx: &mut CurrentRealm,
    global_scope: &GlobalScope,
    val: HandleValue,
) -> JSValueResult {
    run_a_script::<DomTypeHolder, _, _>(cx, global_scope, |cx| {
        let mut seen = HashSet::new();
        let result = jsval_to_js_value_inner(cx, global_scope, val, &mut seen);

        if result.is_err() {
            report_pending_exception(cx);
        }
        result
    })
}

#[expect(unsafe_code)]
/// <https://w3c.github.io/webdriver/#dfn-internal-json-clone>
fn jsval_to_js_value_inner(
    cx: &mut CurrentRealm,
    global_scope: &GlobalScope,
    val: HandleValue,
    seen: &mut HashSet<HashableJSVal>,
) -> JSValueResult {
    if val.get().is_undefined() {
        Ok(JSValue::Undefined)
    } else if val.get().is_null() {
        Ok(JSValue::Null)
    } else if val.get().is_boolean() {
        Ok(JSValue::Boolean(val.get().to_boolean()))
    } else if val.get().is_number() {
        Ok(JSValue::Number(val.to_number()))
    } else if val.get().is_string() {
        let string = NonNull::new(val.to_string()).expect("Should have a non-Null String");
        let string = unsafe { jsstr_to_string(cx, string) };
        Ok(JSValue::String(string))
    } else if val.get().is_object() {
        rooted!(&in(cx) let object = match FromJSValConvertible::safe_from_jsval(cx, val, ()).unwrap() {
            ConversionResult::Success(object) => object,
            _ => unreachable!(),
        });

        if let Ok(element) = unsafe { root_from_object::<Element>(cx, *object) } {
            // If the element is stale, return error with error code stale element reference.
            if is_stale(&element) {
                Err(JavaScriptEvaluationError::SerializationError(
                    JavaScriptEvaluationResultSerializationError::StaleElementReference,
                ))
            } else {
                Ok(JSValue::Element(element.upcast::<Node>().unique_id()))
            }
        } else if let Ok(shadow_root) = unsafe { root_from_object::<ShadowRoot>(cx, *object) } {
            // If the shadow root is detached, return error with error code detached shadow root.
            if is_detached(&shadow_root) {
                Err(JavaScriptEvaluationError::SerializationError(
                    JavaScriptEvaluationResultSerializationError::DetachedShadowRoot,
                ))
            } else {
                Ok(JSValue::ShadowRoot(
                    shadow_root.upcast::<Node>().unique_id(),
                ))
            }
        } else if let Ok(window) = unsafe { root_from_object::<Window>(cx, *object) } {
            let window_proxy = window.window_proxy();
            if window_proxy.is_browsing_context_discarded() {
                Err(JavaScriptEvaluationError::SerializationError(
                    JavaScriptEvaluationResultSerializationError::StaleElementReference,
                ))
            } else if window_proxy.browsing_context_id() == window_proxy.webview_id() {
                Ok(JSValue::Window(window.webview_id().to_string()))
            } else {
                Ok(JSValue::Frame(
                    window_proxy.browsing_context_id().to_string(),
                ))
            }
        } else if object_has_to_json_property(cx, global_scope, object.handle()) {
            let name = CString::new("toJSON").unwrap();
            rooted!(&in(cx) let mut value = UndefinedValue());
            let call_result = unsafe {
                JS_CallFunctionName(
                    cx,
                    object.handle(),
                    name.as_ptr(),
                    &HandleValueArray::empty(),
                    value.handle_mut(),
                )
            };

            if call_result {
                Ok(jsval_to_js_value_inner(
                    cx,
                    global_scope,
                    value.handle(),
                    seen,
                )?)
            } else {
                throw_dom_exception(cx, global_scope, Error::JSFailed);
                Err(JavaScriptEvaluationError::SerializationError(
                    JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                ))
            }
        } else {
            clone_an_object(cx, global_scope, val, seen, object.handle())
        }
    } else {
        Err(JavaScriptEvaluationError::SerializationError(
            JavaScriptEvaluationResultSerializationError::UnknownType,
        ))
    }
}

#[expect(unsafe_code)]
/// <https://w3c.github.io/webdriver/#dfn-clone-an-object>
fn clone_an_object(
    cx: &mut CurrentRealm,
    global_scope: &GlobalScope,
    val: HandleValue,
    seen: &mut HashSet<HashableJSVal>,
    object_handle: HandleObject,
) -> JSValueResult {
    let hashable = val.into();
    // Step 1. If value is in `seen`, return error with error code javascript error.
    if seen.contains(&hashable) {
        return Err(JavaScriptEvaluationError::SerializationError(
            JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
        ));
    }
    // Step 2. Append value to `seen`.
    seen.insert(hashable.clone());

    let return_val =
        if is_array_like::<crate::DomTypeHolder>(cx, val) || is_arguments_object(cx, val) {
            let mut result: Vec<JSValue> = Vec::new();

            let get_property_result =
                get_property::<u32>(cx, object_handle, c"length", ConversionBehavior::Default);
            let length = match get_property_result {
                Ok(length) => match length {
                    Some(length) => length,
                    _ => {
                        return Err(JavaScriptEvaluationError::SerializationError(
                            JavaScriptEvaluationResultSerializationError::UnknownType,
                        ));
                    },
                },
                Err(error) => {
                    throw_dom_exception(cx, global_scope, error);
                    return Err(JavaScriptEvaluationError::SerializationError(
                        JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                    ));
                },
            };
            // Step 4. For each enumerable property in value, run the following substeps:
            for i in 0..length {
                rooted!(&in(cx) let mut item = UndefinedValue());
                let cname = CString::new(i.to_string()).unwrap();
                let get_property_result =
                    get_property_jsval(cx, object_handle, &cname, item.handle_mut());
                match get_property_result {
                    Ok(_) => {
                        let converted_item =
                            jsval_to_js_value_inner(cx, global_scope, item.handle(), seen)?;

                        result.push(converted_item);
                    },
                    Err(error) => {
                        throw_dom_exception(cx, global_scope, error);
                        return Err(JavaScriptEvaluationError::SerializationError(
                            JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                        ));
                    },
                }
            }
            Ok(JSValue::Array(result))
        } else {
            let mut result = HashMap::new();

            let mut ids = unsafe { IdVector::new(cx.raw_cx()) };
            let succeeded =
                unsafe { GetPropertyKeys(cx, object_handle, JSITER_OWNONLY, ids.handle_mut()) };
            if !succeeded {
                return Err(JavaScriptEvaluationError::SerializationError(
                    JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                ));
            }
            for id in ids.iter() {
                rooted!(&in(cx) let id = *id);
                rooted!(&in(cx) let mut desc = PropertyDescriptor::default());

                let mut is_none = false;
                let succeeded = unsafe {
                    JS_GetOwnPropertyDescriptorById(
                        cx,
                        object_handle,
                        id.handle(),
                        desc.handle_mut(),
                        &mut is_none,
                    )
                };
                if !succeeded {
                    return Err(JavaScriptEvaluationError::SerializationError(
                        JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                    ));
                }

                rooted!(&in(cx) let mut property = UndefinedValue());
                let succeeded = unsafe {
                    JS_GetPropertyById(cx, object_handle, id.handle(), property.handle_mut())
                };
                if !succeeded {
                    return Err(JavaScriptEvaluationError::SerializationError(
                        JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                    ));
                }

                if !property.is_undefined() {
                    let name = jsid_to_string(cx, id.handle());
                    let Some(name) = name else {
                        return Err(JavaScriptEvaluationError::SerializationError(
                            JavaScriptEvaluationResultSerializationError::OtherJavaScriptError,
                        ));
                    };

                    let value = jsval_to_js_value_inner(cx, global_scope, property.handle(), seen)?;
                    result.insert(name.into(), value);
                }
            }
            Ok(JSValue::Object(result))
        };
    // Step 5. Remove the last element of `seen`.
    seen.remove(&hashable);
    // Step 6. Return success with data `result`.
    return_val
}
