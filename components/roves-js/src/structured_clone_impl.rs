/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Structured clone on V8's `ValueSerializer`/`ValueDeserializer`.
//!
//! The stream is V8's header, then a transfer table, then the value:
//! - transferred `ArrayBuffer`s are copied into the table and detached (SpiderMonkey's
//!   behaviour for data that crosses processes), then handed to V8 by transfer id;
//! - other transferred objects go through the embedder's `writeTransfer`/`readTransfer`
//!   callbacks, and the value refers to them by index;
//! - embedder (DOM) objects are V8 API objects, which V8 treats as host objects: the
//!   embedder's `write`/`read` callbacks serialize them through a writer/reader that points
//!   at V8's serializer (`JS_WriteUint32Pair`, `JS_ReadBytes`, ...).
//!
//! As in SpiderMonkey, a failing embedder callback or an unclonable value reports through
//! `reportError` and leaves no pending exception.

use std::cell::{Cell, RefCell};
use std::ffi::{CString, c_void};

use crate::jsapi::{
    CloneDataPolicy, HandleValue, JSContext, JSObject, JSStructuredCloneCallbacks, JSStructuredCloneReader,
    JSStructuredCloneWriter, MutableHandleValue, StructuredCloneScope, TransferableOwnership,
};
use crate::jsval::{UndefinedValue, from_v8};
use v8::{ValueDeserializerHelper, ValueSerializerHelper};

/// Serialized structured-clone data (SpiderMonkey's `JSStructuredCloneData`).
#[derive(Default)]
pub struct JSStructuredCloneData {
    pub(crate) bytes: Vec<u8>,
}

/// SpiderMonkey's `JSAutoStructuredCloneBuffer`: data plus the version.
pub struct JSAutoStructuredCloneBuffer {
    pub data_: JSStructuredCloneData,
    pub version_: u32,
}

pub unsafe fn NewJSAutoStructuredCloneBuffer(_scope: StructuredCloneScope, _callbacks: *const JSStructuredCloneCallbacks) -> *mut JSAutoStructuredCloneBuffer {
    Box::into_raw(Box::new(JSAutoStructuredCloneBuffer { data_: JSStructuredCloneData::default(), version_: crate::jsapi::JS_STRUCTURED_CLONE_VERSION }))
}

pub unsafe fn DeleteJSAutoStructuredCloneBuffer(buffer: *mut JSAutoStructuredCloneBuffer) {
    // SAFETY: buffers come from `NewJSAutoStructuredCloneBuffer`.
    drop(unsafe { Box::from_raw(buffer) });
}

pub unsafe fn GetLengthOfJSStructuredCloneData(data: *mut JSStructuredCloneData) -> usize {
    // SAFETY: callers pass valid data.
    unsafe { &*data }.bytes.len()
}

pub unsafe fn CopyJSStructuredCloneData(src: *mut JSStructuredCloneData, dest: *mut u8) {
    // SAFETY: `dest` has room for the data (`GetLengthOfJSStructuredCloneData`).
    let bytes = unsafe { &(*src).bytes };
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest, bytes.len()) };
}

pub unsafe fn WriteBytesToJSStructuredCloneData(src: *const u8, len: usize, dest: *mut JSStructuredCloneData) -> bool {
    // SAFETY: callers pass `len` readable bytes and valid data.
    let bytes = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(src, len) } };
    unsafe { &mut *dest }.bytes.extend_from_slice(bytes);
    true
}

// Markers of embedder objects in the stream.
const HOST_OBJECT: u32 = 0;
const HOST_TRANSFERRED: u32 = 1;
// Kinds of transfer-table records.
const TRANSFER_ARRAY_BUFFER: u32 = 0;
const TRANSFER_HOST: u32 = 1;

/// What `JS_Write*` reach through a `*mut JSStructuredCloneWriter`.
struct Writer {
    helper: *const dyn v8::ValueSerializerHelper,
}

/// What `JS_Read*` reach through a `*mut JSStructuredCloneReader`.
struct Reader {
    helper: *const dyn v8::ValueDeserializerHelper,
}

pub unsafe fn JS_WriteUint32Pair(w: *mut JSStructuredCloneWriter, tag: u32, data: u32) -> bool {
    // SAFETY: writers are only handed out during a host-object write.
    let helper = unsafe { &*(*(w as *const Writer)).helper };
    helper.write_uint32(tag);
    helper.write_uint32(data);
    true
}

pub unsafe fn JS_WriteBytes(w: *mut JSStructuredCloneWriter, p: *const c_void, len: usize) -> bool {
    // SAFETY: as above; callers pass `len` readable bytes.
    let helper = unsafe { &*(*(w as *const Writer)).helper };
    helper.write_uint64(len as u64);
    if len > 0 {
        helper.write_raw_bytes(unsafe { std::slice::from_raw_parts(p as *const u8, len) });
    }
    true
}

pub unsafe fn JS_ReadUint32Pair(r: *mut JSStructuredCloneReader, p1: *mut u32, p2: *mut u32) -> bool {
    // SAFETY: readers are only handed out during a host-object read; valid out pointers.
    let helper = unsafe { &*(*(r as *const Reader)).helper };
    unsafe { helper.read_uint32(&mut *p1) && helper.read_uint32(&mut *p2) }
}

pub unsafe fn JS_ReadBytes(r: *mut JSStructuredCloneReader, p: *mut c_void, len: usize) -> bool {
    // SAFETY: as above; `p` has room for `len` bytes.
    let helper = unsafe { &*(*(r as *const Reader)).helper };
    let mut written = 0u64;
    if !helper.read_uint64(&mut written) || written as usize != len {
        return false;
    }
    if len == 0 {
        return true;
    }
    match helper.read_raw_bytes(len) {
        Some(bytes) => {
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, len) };
            true
        },
        None => false,
    }
}

fn raw_cx() -> *mut JSContext {
    JSContext::current() as *const JSContext as *mut JSContext
}

/// The serializer delegate: embedder objects through the embedder's callbacks.
struct SerializerDelegate {
    callbacks: *const JSStructuredCloneCallbacks,
    closure: *mut c_void,
    /// Host objects being transferred (by identity), with their table index.
    transferred: Vec<v8::Global<v8::Object>>,
    /// Set when a failure was reported to the embedder (no exception should remain).
    reported: Cell<bool>,
}

impl SerializerDelegate {
    fn report(&self, error: u32, message: &str) {
        // SAFETY: callers pass valid callbacks (or null).
        if let Some(report) = unsafe { self.callbacks.as_ref() }.and_then(|callbacks| callbacks.reportError) {
            let message = CString::new(message.replace('\0', "")).unwrap_or_default();
            // SAFETY: SpiderMonkey's error-callback contract.
            unsafe { report(raw_cx(), error, self.closure, message.as_ptr()) };
            self.reported.set(true);
        }
    }
}

fn throw_marker(scope: &mut v8::PinScope) {
    let message = v8::String::new(scope, "DataCloneError").expect("a short string");
    let error = v8::Exception::error(scope, message);
    scope.throw_exception(error);
}

impl v8::ValueSerializerImpl for SerializerDelegate {
    fn throw_data_clone_error<'s>(&self, scope: &mut v8::PinScope<'s, '_>, message: v8::Local<'s, v8::String>) {
        let text = message.to_rust_string_lossy(scope);
        self.report(crate::jsapi::JS_SCERR_UNSUPPORTED_TYPE, &text);
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }

    fn write_host_object<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        object: v8::Local<'s, v8::Object>,
        value_serializer: &dyn v8::ValueSerializerHelper,
    ) -> Option<bool> {
        if let Some(index) = self.transferred.iter().position(|transferred| v8::Local::new(scope, transferred) == object) {
            value_serializer.write_uint32(HOST_TRANSFERRED);
            value_serializer.write_uint32(index as u32);
            return Some(true);
        }
        // SAFETY: callers pass valid callbacks (or null).
        let Some(write) = (unsafe { self.callbacks.as_ref() }).and_then(|callbacks| callbacks.write) else {
            self.report(crate::jsapi::JS_SCERR_UNSUPPORTED_TYPE, "object is not cloneable");
            throw_marker(scope);
            return None;
        };
        value_serializer.write_uint32(HOST_OBJECT);
        // The helper outlives this call; the writer is only used during it.
        // SAFETY: lifetime erasure of a reference that stays valid for the call.
        let helper: *const dyn v8::ValueSerializerHelper = unsafe { std::mem::transmute(value_serializer as *const dyn v8::ValueSerializerHelper) };
        let writer = Writer { helper };
        let cx = raw_cx();
        let obj = from_v8(scope, object.into()).to_object();
        crate::rooted!(in(cx) let obj = obj);
        let mut same_process = false;
        // SAFETY: SpiderMonkey's write-callback contract.
        let ok = unsafe { write(cx, &writer as *const Writer as *mut JSStructuredCloneWriter, obj.handle().into(), &mut same_process, self.closure) };
        if !ok {
            self.reported.set(true);
            throw_marker(scope);
            return None;
        }
        Some(true)
    }
}

/// Serializes `v` (transferring `transferable`'s objects) into `data`.
pub unsafe fn JS_WriteStructuredClone(
    cx: *mut JSContext,
    v: HandleValue,
    data: *mut JSStructuredCloneData,
    _scope: StructuredCloneScope,
    _policy: *const CloneDataPolicy,
    callbacks: *const JSStructuredCloneCallbacks,
    closure: *mut c_void,
    transferable: HandleValue,
) -> bool {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let (v, transferable) = (v.get(), transferable.get());
    let mut reported = false;
    let result = raw.catching(|scope| {
        // The transfer list: buffers and host objects.
        let mut buffers = Vec::new();
        let mut hosts = Vec::new();
        if transferable.is_object() {
            // SAFETY: a rooted value.
            let list = unsafe { crate::jsval::to_v8(scope, transferable) };
            let list = v8::Local::<v8::Object>::try_from(list).ok()?;
            let length_key = v8::String::new(scope, "length")?;
            let length = list.get(scope, length_key.into())?.uint32_value(scope)?;
            for index in 0..length {
                let item = list.get_index(scope, index)?;
                if let Ok(buffer) = v8::Local::<v8::ArrayBuffer>::try_from(item) {
                    buffers.push(buffer);
                } else if let Ok(object) = v8::Local::<v8::Object>::try_from(item) {
                    hosts.push(object);
                } else {
                    crate::native::throw_type_error(scope, "transferable is not an object");
                    return None;
                }
            }
        }
        let delegate = Box::new(SerializerDelegate {
            callbacks,
            closure,
            transferred: hosts.iter().map(|host| v8::Global::new(scope, *host)).collect(),
            reported: Cell::new(false),
        });
        let delegate_pointer = &*delegate as *const SerializerDelegate;
        let serializer = v8::ValueSerializer::new(scope, delegate);
        serializer.write_header();
        // The transfer table.
        serializer.write_uint32((buffers.len() + hosts.len()) as u32);
        for (index, buffer) in buffers.iter().enumerate() {
            serializer.write_uint32(TRANSFER_ARRAY_BUFFER);
            let length = buffer.byte_length();
            serializer.write_uint64(length as u64);
            if length > 0 {
                let bytes = buffer.data()?.as_ptr() as *const u8;
                // SAFETY: the buffer holds `length` bytes.
                serializer.write_raw_bytes(unsafe { std::slice::from_raw_parts(bytes, length) });
            }
            serializer.transfer_array_buffer(index as u32, *buffer);
        }
        // SAFETY: callers pass valid callbacks (or null).
        let callbacks_ref = unsafe { callbacks.as_ref() };
        for host in &hosts {
            let Some(write_transfer) = callbacks_ref.and_then(|callbacks| callbacks.writeTransfer) else {
                // SAFETY: the delegate lives inside the serializer.
                unsafe { &*delegate_pointer }.report(crate::jsapi::JS_SCERR_TRANSFERABLE, "object is not transferable");
                throw_marker(scope);
                return None;
            };
            let obj = from_v8(scope, (*host).into()).to_object();
            crate::rooted!(in(cx) let obj = obj);
            let mut tag = 0u32;
            let mut ownership = TransferableOwnership::SCTAG_TMO_UNFILLED;
            let mut content = std::ptr::null_mut();
            let mut extra = 0u64;
            // SAFETY: SpiderMonkey's transfer-callback contract.
            if !unsafe { write_transfer(cx, obj.handle().into(), closure, &mut tag, &mut ownership, &mut content, &mut extra) } {
                unsafe { &*delegate_pointer }.reported.set(true);
                throw_marker(scope);
                return None;
            }
            serializer.write_uint32(TRANSFER_HOST);
            serializer.write_uint32(tag);
            serializer.write_uint32(ownership as u32);
            serializer.write_uint64(content as u64);
            serializer.write_uint64(extra);
        }
        // SAFETY: a rooted value.
        let value = unsafe { crate::jsval::to_v8(scope, v) };
        let context = scope.get_current_context();
        let written = serializer.write_value(context, value);
        // SAFETY: as above.
        reported = unsafe { &*delegate_pointer }.reported.get();
        written?;
        for buffer in &buffers {
            buffer.detach(None)?;
        }
        Some(serializer.release())
    });
    match result {
        Some(bytes) => {
            // SAFETY: callers pass valid data.
            unsafe { &mut *data }.bytes = bytes;
            true
        },
        None => {
            if reported {
                // SpiderMonkey reports through the callback without a pending exception.
                raw.pending_exception.borrow_mut().take();
            }
            false
        },
    }
}

/// The deserializer delegate: transferred and embedder objects.
struct DeserializerDelegate {
    callbacks: *const JSStructuredCloneCallbacks,
    policy: *const CloneDataPolicy,
    closure: *mut c_void,
    transferred: RefCell<Vec<v8::Global<v8::Object>>>,
    /// Set when an embedder callback failed (no exception should remain).
    embedder_failed: Cell<bool>,
}

impl v8::ValueDeserializerImpl for DeserializerDelegate {
    fn read_host_object<'s>(&self, scope: &mut v8::PinScope<'s, '_>, value_deserializer: &dyn v8::ValueDeserializerHelper) -> Option<v8::Local<'s, v8::Object>> {
        let mut kind = 0;
        if !value_deserializer.read_uint32(&mut kind) {
            return None;
        }
        if kind == HOST_TRANSFERRED {
            let mut index = 0;
            value_deserializer.read_uint32(&mut index).then_some(())?;
            let transferred = self.transferred.borrow();
            return transferred.get(index as usize).map(|object| v8::Local::new(scope, object));
        }
        let (mut tag, mut data) = (0, 0);
        (value_deserializer.read_uint32(&mut tag) && value_deserializer.read_uint32(&mut data)).then_some(())?;
        // SAFETY: callers pass valid callbacks (or null).
        let read = unsafe { self.callbacks.as_ref() }.and_then(|callbacks| callbacks.read)?;
        // SAFETY: lifetime erasure of a reference that stays valid for the call.
        let helper: *const dyn v8::ValueDeserializerHelper = unsafe { std::mem::transmute(value_deserializer as *const dyn v8::ValueDeserializerHelper) };
        let reader = Reader { helper };
        // SAFETY: SpiderMonkey's read-callback contract.
        let object = unsafe { read(raw_cx(), &reader as *const Reader as *mut JSStructuredCloneReader, self.policy, tag, data, self.closure) };
        if object.is_null() {
            self.embedder_failed.set(true);
            throw_marker(scope);
            return None;
        }
        // SAFETY: a live object returned by the embedder.
        let object = unsafe { crate::cell::cell_value(scope, object as *mut c_void) };
        v8::Local::<v8::Object>::try_from(object).ok()
    }
}

/// Deserializes `data` into `vp` in the current realm.
pub unsafe fn JS_ReadStructuredClone(
    cx: *mut JSContext,
    data: *const JSStructuredCloneData,
    _version: u32,
    _scope: StructuredCloneScope,
    mut vp: MutableHandleValue,
    policy: *const CloneDataPolicy,
    callbacks: *const JSStructuredCloneCallbacks,
    closure: *mut c_void,
) -> bool {
    // SAFETY: callers pass a live context and valid data.
    let raw = unsafe { &*cx };
    let bytes = unsafe { &(*data).bytes };
    let embedder_failed = Cell::new(false);
    let result = raw.catching(|scope| {
        let delegate = Box::new(DeserializerDelegate { callbacks, policy, closure, transferred: RefCell::new(Vec::new()), embedder_failed: Cell::new(false) });
        let delegate_pointer = &*delegate as *const DeserializerDelegate;
        let deserializer = v8::ValueDeserializer::new(scope, delegate, bytes);
        let context = scope.get_current_context();
        deserializer.read_header(context)?.then_some(())?;
        let mut count = 0;
        deserializer.read_uint32(&mut count).then_some(())?;
        let mut buffer_index = 0u32;
        for _ in 0..count {
            let mut kind = 0;
            deserializer.read_uint32(&mut kind).then_some(())?;
            if kind == TRANSFER_ARRAY_BUFFER {
                let mut length = 0u64;
                deserializer.read_uint64(&mut length).then_some(())?;
                let buffer = v8::ArrayBuffer::new(scope, length as usize);
                if length > 0 {
                    let source = deserializer.read_raw_bytes(length as usize)?;
                    let target = buffer.data()?.as_ptr() as *mut u8;
                    // SAFETY: both hold `length` bytes.
                    unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), target, length as usize) };
                }
                deserializer.transfer_array_buffer(buffer_index, buffer);
                buffer_index += 1;
                continue;
            }
            let (mut tag, mut ownership) = (0u32, 0u32);
            let (mut content, mut extra) = (0u64, 0u64);
            (deserializer.read_uint32(&mut tag) &&
                deserializer.read_uint32(&mut ownership) &&
                deserializer.read_uint64(&mut content) &&
                deserializer.read_uint64(&mut extra))
                .then_some(())?;
            // SAFETY: callers pass valid callbacks (or null).
            let read_transfer = unsafe { callbacks.as_ref() }.and_then(|callbacks| callbacks.readTransfer)?;
            crate::rooted!(in(cx) let mut object = std::ptr::null_mut::<JSObject>());
            let helper: &dyn v8::ValueDeserializerHelper = &deserializer;
            // SAFETY: lifetime erasure of a reference that stays valid for the call.
            let reader = Reader { helper: unsafe { std::mem::transmute::<&dyn v8::ValueDeserializerHelper, *const dyn v8::ValueDeserializerHelper>(helper) } };
            // SAFETY: SpiderMonkey's transfer-read contract.
            let ok = unsafe {
                read_transfer(
                    cx,
                    &reader as *const Reader as *mut JSStructuredCloneReader,
                    policy,
                    tag,
                    content as *mut c_void,
                    extra,
                    closure,
                    object.handle_mut().into(),
                )
            };
            if !ok || object.get().is_null() {
                embedder_failed.set(true);
                throw_marker(scope);
                return None;
            }
            // SAFETY: a live, rooted object.
            let local = unsafe { crate::cell::cell_value(scope, object.get() as *mut c_void) };
            let local = v8::Local::<v8::Object>::try_from(local).ok()?;
            // SAFETY: the delegate lives inside the deserializer.
            unsafe { &*delegate_pointer }.transferred.borrow_mut().push(v8::Global::new(scope, local));
        }
        let value = deserializer.read_value(context);
        // SAFETY: the delegate lives inside the deserializer.
        if unsafe { &*delegate_pointer }.embedder_failed.get() {
            embedder_failed.set(true);
        }
        let value = value?;
        Some(from_v8(scope, value))
    });
    match result {
        Some(value) => {
            vp.set(value);
            true
        },
        None => {
            if embedder_failed.get() {
                // SpiderMonkey leaves no exception when an embedder callback fails.
                raw.pending_exception.borrow_mut().take();
            }
            vp.set(UndefinedValue());
            false
        },
    }
}
