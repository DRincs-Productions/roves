/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Public V8 APIs missing from rusty_v8 150, bound by the compiler-checked shim.
use std::ffi::c_void;

unsafe extern "C" {
    fn roves_v8_script_id(script: *const c_void) -> i32;
    fn roves_v8_is_callable(object: *const c_void) -> bool;
    fn roves_v8_is_constructor(object: *const c_void) -> bool;
    fn roves_v8_set_call_handler(template: *const c_void, callback: v8::FunctionCallback, data: *const c_void);
}

// Locals are passed using the same single-pointer representation as rusty_v8's bindings;
// the C++ shim checks their size and invokes the exact pinned public declarations.
pub fn script_id(script: v8::Local<v8::UnboundScript>) -> i32 {
    unsafe { roves_v8_script_id(std::mem::transmute(script)) }
}
pub fn is_callable(object: v8::Local<v8::Object>) -> bool {
    unsafe { roves_v8_is_callable(std::mem::transmute(object)) }
}
pub fn is_constructor(object: v8::Local<v8::Object>) -> bool {
    unsafe { roves_v8_is_constructor(std::mem::transmute(object)) }
}
pub fn set_call_as_function_handler(
    template: v8::Local<v8::ObjectTemplate>,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<v8::Value>,
) {
    unsafe { roves_v8_set_call_handler(std::mem::transmute(template), callback.map_fn_to(), std::mem::transmute(data)) }
}
