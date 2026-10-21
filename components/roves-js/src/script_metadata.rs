/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Script-owned metadata: V8 retains a host token with the compiled source;
//! a native WeakMap owns the cppgc metadata only while that token is reachable.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use v8::cppgc::{GarbageCollected, Visitor, WeakPersistent};

use crate::gc::RootKind;
use crate::jsapi::{JSContext, ScriptPrivateReferenceHook};
use crate::jsval::{JSVal, UndefinedValue};

const PRIVATE_WRAP_TAG: u16 = 2;
const SWEEP_MIN: usize = 256;

struct PrivateData {
    value: Cell<JSVal>,
    release: Cell<ScriptPrivateReferenceHook>,
}

impl PrivateData {
    fn replace(&self, value: JSVal, release: ScriptPrivateReferenceHook) {
        let old = self.value.replace(value);
        let old_release = self.release.replace(release);
        if !old.is_undefined() {
            if let Some(release) = old_release {
                // Reference hooks follow SpiderMonkey's non-GC retain/release contract.
                unsafe { release(&old) };
            }
        }
    }

    fn clear(&self) {
        self.replace(UndefinedValue(), None);
    }
}

unsafe impl GarbageCollected for PrivateData {
    fn trace(&self, visitor: &mut Visitor) {
        self.value.get().trace_root(visitor);
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
        let source = v8::String::new(scope, "[new WeakMap(), WeakMap.prototype.set]")?;
        let cache = v8::Script::compile(scope, source, None)?.run(scope)?;
        let cache = v8::Local::<v8::Array>::try_from(cache).ok()?;
        *self.cache.borrow_mut() = Some(v8::Global::new(scope, cache));
        let template = v8::ObjectTemplate::new(scope);
        // A field makes ObjectTemplate instances API wrappers for cppgc.
        template.set_internal_field_count(1);
        *self.template.borrow_mut() = Some(v8::Global::new(scope, template));
        Some(())
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
        Some(entries.get(&id)?.data.as_ref()?.get()?.value.get())
    }

    pub(crate) fn set(
        &self,
        cx: &JSContext,
        scope: &mut v8::PinScope,
        id: i32,
        value: JSVal,
    ) -> Option<()> {
        let token = self.entries.borrow().get(&id)?.token.to_local(scope)?;
        let existing = self
            .entries
            .borrow()
            .get(&id)?
            .data
            .as_ref()
            .and_then(|data| data.get())
            .map(|data| data as *const PrivateData);
        if existing.is_none() && value.is_undefined() {
            return Some(());
        }
        let data = if let Some(data) = existing {
            data
        } else {
            let template = v8::Local::new(scope, self.template.borrow().as_ref()?);
            let holder = template.new_instance(scope)?;
            // This is not a ClassBox; generic JSAPI class inspection must see null.
            holder.set_aligned_pointer_in_internal_field(0, std::ptr::null_mut(), 0);
            let heap = scope.get_cpp_heap()?;
            let data = unsafe {
                v8::cppgc::make_garbage_collected(
                    heap,
                    PrivateData {
                        value: Cell::new(UndefinedValue()),
                        release: Cell::new(None),
                    },
                )
            };
            // Unique tag; the local holder owns the cppgc object before another allocation.
            unsafe { v8::Object::wrap::<PRIVATE_WRAP_TAG, PrivateData>(scope, holder, &data) };
            let cache = v8::Local::new(scope, self.cache.borrow().as_ref()?);
            let map = cache.get_index(scope, 0)?;
            let set = v8::Local::<v8::Function>::try_from(cache.get_index(scope, 1)?).ok()?;
            set.call(scope, map, &[token.into(), holder.into()])?;
            self.entries.borrow_mut().get_mut(&id)?.data = Some(WeakPersistent::new(&data));
            (unsafe { data.as_ref() }) as *const PrivateData
        };
        // The script/token and WeakMap keep this data alive. Reference hooks cannot GC;
        // call them outside the index borrow and retain before releasing identical data.
        crate::script_impl::retain_private(cx, value);
        unsafe { &*data }.replace(value, crate::script_impl::private_release_hook(cx));
        Some(())
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
