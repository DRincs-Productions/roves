/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Source-owned metadata of classic scripts and source modules: V8 retains a host token
//! with the compiled source; a native WeakMap owns the cppgc metadata only while that token
//! is reachable. The metadata is found by script id or, from the host options V8 hands to
//! callbacks, by token. While reachable it traces its private's native edges through the
//! embedder's trace hook (see `ScriptPrivateTraceHook`). A source module's metadata also
//! keeps the module's record (see `modules_impl`), so the record lives exactly as long as
//! something reaches the source's token or the record itself. JSON module privates reuse the
//! same holder and data, kept by the module record instead of a token; a linked JSON module's
//! namespace keeps its record through the same WeakMap (see `keep_record`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;

use v8::cppgc::{GarbageCollected, Visitor, WeakPersistent};

use crate::jsapi::JSContext;
use crate::jsval::JSVal;
use crate::script_impl::HostPrivate;

const PRIVATE_WRAP_TAG: u16 = 2;
const SWEEP_MIN: usize = 256;

/// A private owned by a holder object, traced while the holder is reachable and released
/// once (replacement, explicit shutdown clearing, or finalization). Also used for JSON module
/// privates, whose holder lives on the record's placeholder (see `modules_impl`).
pub(crate) struct PrivateData {
    private: Cell<HostPrivate>,
    /// A source module's record (cell), or the JSON record a namespace keeps, fixed at
    /// creation; null otherwise.
    module: *mut c_void,
}

impl PrivateData {
    pub(crate) fn value(&self) -> JSVal {
        self.private.get().value()
    }

    /// Installs `private` (already retained), then releases the previous one: from then on
    /// only the new private and its own hooks are traced.
    pub(crate) fn replace(&self, private: HostPrivate) {
        self.private.replace(private).release();
    }

    pub(crate) fn clear(&self) {
        self.replace(HostPrivate::NONE);
    }
}

unsafe impl GarbageCollected for PrivateData {
    fn trace(&self, visitor: &mut Visitor) {
        self.private.get().trace(visitor);
        crate::cell::trace_cell(self.module, visitor);
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesScriptPrivate"
    }
}

impl Drop for PrivateData {
    fn drop(&mut self) {
        let _gc = crate::object::GcScope::enter();
        self.clear();
    }
}

/// The private data a holder (see `ScriptMetadata::new_holder`) wraps; valid while the
/// holder is.
pub(crate) fn holder_data(scope: &mut v8::PinScope, holder: v8::Local<v8::Object>) -> Option<*const PrivateData> {
    // SAFETY: only holders carry this wrap tag.
    let data = unsafe { v8::Object::unwrap::<PRIVATE_WRAP_TAG, PrivateData>(scope, holder) }?;
    Some(unsafe { data.as_ref() } as *const PrivateData)
}

struct ScriptEntry {
    token: v8::Weak<v8::Symbol>,
    data: Option<WeakPersistent<PrivateData>>,
}

#[derive(Default)]
pub(crate) struct ScriptMetadata {
    entries: RefCell<HashMap<i32, ScriptEntry>>,
    // Captured in the initial runtime realm, before user code can replace intrinsics.
    cache: RefCell<Option<v8::Global<v8::Array>>>,
    template: RefCell<Option<v8::Global<v8::ObjectTemplate>>>,
    insertions: Cell<usize>,
    next_sweep: Cell<usize>,
}

impl ScriptMetadata {
    pub(crate) fn initialize(&self, scope: &mut v8::PinScope) -> Option<()> {
        if self.cache.borrow().is_some() {
            return Some(());
        }
        let source = v8::String::new(scope, "[new WeakMap(), WeakMap.prototype.set, WeakMap.prototype.get]")?;
        let cache = v8::Script::compile(scope, source, None)?.run(scope)?;
        let cache = v8::Local::<v8::Array>::try_from(cache).ok()?;
        *self.cache.borrow_mut() = Some(v8::Global::new(scope, cache));
        Some(())
    }

    /// A new, empty holder object for `Object::wrap` (script privates and realm data). The
    /// template is isolate wide, made on first use.
    pub(crate) fn empty_holder<'s>(&self, scope: &mut v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::Object>> {
        let existing = self.template.borrow().as_ref().map(|template| v8::Local::new(scope, template));
        let template = match existing {
            Some(template) => template,
            None => {
                let template = v8::ObjectTemplate::new(scope);
                // A field makes ObjectTemplate instances API wrappers for cppgc.
                template.set_internal_field_count(1);
                *self.template.borrow_mut() = Some(v8::Global::new(scope, template));
                template
            },
        };
        let holder = template.new_instance(scope)?;
        // This is not a ClassBox; generic JSAPI class inspection must see null.
        holder.set_aligned_pointer_in_internal_field(0, std::ptr::null_mut(), 0);
        Some(holder)
    }

    pub(crate) fn register(&self, scope: &mut v8::PinScope, id: i32, token: v8::Local<v8::Symbol>) {
        let token = v8::Weak::new(scope, token);
        let mut entries = self.entries.borrow_mut();
        entries.insert(id, ScriptEntry { token, data: None });
        let insertions = self.insertions.get() + 1;
        self.insertions.set(insertions);
        if insertions >= self.next_sweep.get().max(SWEEP_MIN) {
            // Keep a pending cppgc finalization discoverable for explicit shutdown cleanup.
            entries.retain(|_, entry| {
                !entry.token.is_empty()
                    || entry.data.as_ref().is_some_and(|data| data.get().is_some())
            });
            self.insertions.set(0);
            self.next_sweep.set(entries.len());
        }
    }

    pub(crate) fn private(&self, id: i32) -> Option<JSVal> {
        let entries = self.entries.borrow();
        Some(entries.get(&id)?.data.as_ref()?.get()?.private.get().value())
    }

    /// The private of the source whose host token is `token` (`None`: the token has no
    /// metadata). One lookup in the captured WeakMap, whose holder wraps the metadata.
    pub(crate) fn token_private<'s>(&self, scope: &mut v8::PinScope<'s, '_>, token: v8::Local<'s, v8::Symbol>) -> Option<JSVal> {
        let data = self.data_under(scope, token.into())?;
        // SAFETY: `data_under`'s holder was a local of this scope, keeping the data alive.
        Some(unsafe { &*data }.value())
    }

    /// Calls the captured WeakMap method at `method` (1: set, 2: get) on the captured map.
    fn call_map<'s>(&self, scope: &mut v8::PinScope<'s, '_>, method: u32, args: &[v8::Local<'s, v8::Value>]) -> Option<v8::Local<'s, v8::Value>> {
        let cache = v8::Local::new(scope, self.cache.borrow().as_ref()?);
        let map = cache.get_index(scope, 0)?;
        let method = v8::Local::<v8::Function>::try_from(cache.get_index(scope, method)?).ok()?;
        method.call(scope, map, args)
    }

    /// The data of the holder the captured WeakMap keeps under `key`, valid while this scope
    /// lives (its local holder keeps it).
    fn data_under<'s>(&self, scope: &mut v8::PinScope<'s, '_>, key: v8::Local<'s, v8::Value>) -> Option<*const PrivateData> {
        let holder = v8::Local::<v8::Object>::try_from(self.call_map(scope, 2, &[key])?).ok()?;
        holder_data(scope, holder)
    }

    /// The record the captured WeakMap keeps under `key` (see `keep_record`), if any.
    pub(crate) fn kept_record<'s>(&self, scope: &mut v8::PinScope<'s, '_>, key: v8::Local<'s, v8::Value>) -> Option<*mut c_void> {
        let data = self.data_under(scope, key)?;
        // SAFETY: as in `token_private`.
        Some(unsafe { &*data }.module).filter(|record| !record.is_null())
    }

    /// Makes the captured WeakMap keep `record` (alive: rooted or kept by the caller) while
    /// `key` is reachable, through a holder of new data with no private; `None` stops keeping
    /// the record kept so far. Nothing changes when `key` already keeps `record`.
    pub(crate) fn keep_record<'s>(&self, scope: &mut v8::PinScope<'s, '_>, key: v8::Local<'s, v8::Value>, record: Option<*mut c_void>) -> Option<()> {
        if self.kept_record(scope, key) == record {
            return Some(());
        }
        let value = match record {
            Some(record) => self.new_holder_of(scope, record)?.0.into(),
            None => v8::undefined(scope).into(),
        };
        self.call_map(scope, 1, &[key, value]).map(drop)
    }

    /// A new holder object owning new, empty private data (not a source module's), and a
    /// weak handle on that data. The local holder is the data's only owner.
    pub(crate) fn new_holder<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> Option<(v8::Local<'s, v8::Object>, WeakPersistent<PrivateData>)> {
        self.new_holder_of(scope, std::ptr::null_mut())
    }

    fn new_holder_of<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        module: *mut c_void,
    ) -> Option<(v8::Local<'s, v8::Object>, WeakPersistent<PrivateData>)> {
        let holder = self.empty_holder(scope)?;
        let heap = scope.get_cpp_heap()?;
        let data = unsafe {
            v8::cppgc::make_garbage_collected(
                heap,
                PrivateData { private: Cell::new(HostPrivate::NONE), module },
            )
        };
        // Unique tag; the local holder owns the cppgc object before another allocation.
        unsafe { v8::Object::wrap::<PRIVATE_WRAP_TAG, PrivateData>(scope, holder, &data) };
        Some((holder, WeakPersistent::new(&data)))
    }

    fn existing(&self, id: i32) -> Option<*const PrivateData> {
        let entries = self.entries.borrow();
        entries.get(&id)?.data.as_ref()?.get().map(|data| data as *const PrivateData)
    }

    /// Creates source `id`'s metadata (keeping `module`, if any), owned by a holder that the
    /// captured WeakMap keeps under the source's token.
    fn create(&self, scope: &mut v8::PinScope, id: i32, module: *mut c_void) -> Option<*const PrivateData> {
        let token = self.entries.borrow().get(&id)?.token.to_local(scope)?;
        let (holder, data) = self.new_holder_of(scope, module)?;
        self.call_map(scope, 1, &[token.into(), holder.into()])?;
        let pointer = data.get()? as *const PrivateData;
        self.entries.borrow_mut().get_mut(&id)?.data = Some(data);
        Some(pointer)
    }

    pub(crate) fn set(
        &self,
        cx: &JSContext,
        scope: &mut v8::PinScope,
        id: i32,
        value: JSVal,
    ) -> Option<()> {
        let data = match self.existing(id) {
            Some(data) => data,
            None if value.is_undefined() => return Some(()),
            None => self.create(scope, id, std::ptr::null_mut())?,
        };
        // The script/token and WeakMap keep this data alive. Reference hooks cannot GC;
        // call them outside the index borrow and retain before releasing identical data.
        unsafe { &*data }.replace(HostPrivate::retain(cx, value));
        Some(())
    }

    /// Hands a new source module's record to its source's metadata, created now (a new
    /// source has none yet) even without a private.
    pub(crate) fn anchor_module(&self, scope: &mut v8::PinScope, id: i32, record: *mut c_void) -> Option<()> {
        debug_assert!(self.existing(id).is_none(), "a new source has no metadata yet");
        self.create(scope, id, record).map(drop)
    }

    pub(crate) fn release_all(&self) {
        let entries = std::mem::take(&mut *self.entries.borrow_mut());
        for entry in entries.into_values() {
            if let Some(data) = entry.data.as_ref().and_then(|data| data.get()) {
                data.clear();
            }
        }
    }
}
