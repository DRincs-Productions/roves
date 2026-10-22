/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Realms: one V8 context each. A `*mut Realm` points at the realm's [`RealmData`], a cppgc
//! object that the context records in its embedder data so an object's realm is found from
//! its creation context.
//!
//! Lifetime: the context keeps a holder object (embedder data, strong) that owns the realm data
//! (`Object::wrap`); the realm data traces the context and its global. The cycle lives in the
//! unified heap and is collected when nothing reaches it: every JS object, function or promise
//! keeps its creation context, and the current realm and the realms saved by `EnterRealm` or
//! [`crate::jsapi::RealmSwitch`] are GC roots. The runtime's initial realm stays alive through
//! `JSContext::context`. The runtime's registry of realms is weak.
//!
//! A `*mut Realm` (and the global, principals or `CurrentGlobal` location derived from it) is
//! valid while at least one holds: the realm is current or saved; the caller holds a rooted
//! object or function of the realm (or a Window wrapper whose owner is in it); the caller is
//! inside a V8 scope with a local of the realm's context (including entered and microtask
//! contexts). Once the realm was unreachable at a GC, only a `WeakPersistent` may observe it.

use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Rc;

use v8::TracedReference;
use v8::cppgc::{GarbageCollected, Member, Persistent, UnsafePtr, Visitor, WeakPersistent};

use crate::jsapi::{JSContext, JSObject, JSPrincipals, Realm};

/// Embedder-data slot of a context holding its `RealmData` pointer (untraced: the holder in
/// [`HOLDER_SLOT`] keeps the data alive as long as the context).
const REALM_SLOT: i32 = 2;
/// Embedder-data slot of a context holding its cache object (see [`realm_cached`]).
const CACHE_SLOT: i32 = 3;
/// Embedder-data slot of a context holding the object that owns its `RealmData`.
const HOLDER_SLOT: i32 = 4;
/// `Object::wrap` tag of realm data (class boxes use 1, script privates 2).
const REALM_WRAP_TAG: u16 = 3;
/// Registry size before the first sweep of the entries of collected realms.
const REALM_SWEEP_MIN: usize = 16;

/// A realm's principals, shared by its data and its registry entry: whichever of the realm's
/// finalization and the runtime's shutdown comes first releases them, once.
type PrincipalsSlot = Rc<Cell<*mut JSPrincipals>>;

pub(crate) struct RealmData {
    context: TracedReference<v8::Context>,
    /// The realm's global object (its cell): the class object of `JS_NewGlobalObject`, or the
    /// plain global of the runtime's initial realm.
    pub(crate) global: Cell<*mut JSObject>,
    /// The realm's window proxy (`SetWindowProxy`), for a Window global.
    pub(crate) window_proxy: Cell<*mut JSObject>,
    /// The realm's promise-job drain function (see `runtime_impl`).
    pub(crate) drain_function: Cell<*mut JSObject>,
    principals: PrincipalsSlot,
}

#[cfg(test)]
thread_local! {
    /// Realm data finalized on this thread (tests observe collection through it).
    pub(crate) static REALMS_FINALIZED: Cell<usize> = const { Cell::new(0) };
    pub(crate) static STRONG_REALMS_FOR_TESTING: Cell<bool> = const { Cell::new(false) };
    pub(crate) static UNROOTED_SAVED_REALMS_FOR_TESTING: Cell<bool> = const { Cell::new(false) };
}

impl RealmData {
    /// The realm's context (alive while the realm data is).
    pub(crate) fn context<'s>(&self, scope: &v8::PinScope<'s, '_, ()>) -> v8::Local<'s, v8::Context> {
        self.context.get(scope).expect("realm data keeps its context")
    }

    /// Holds `principals` for the realm (once, after every fallible step of its creation).
    fn hold_principals(&self, principals: *mut JSPrincipals) {
        debug_assert!(self.principals.get().is_null(), "a realm holds its principals once");
        // SAFETY: principals come from `CreateRustJSPrincipals`.
        unsafe { crate::jsapi_impl::JS_HoldPrincipals(principals) };
        self.principals.set(principals);
    }
}

// SAFETY: `trace` reports the context and every cell the realm keeps.
unsafe impl GarbageCollected for RealmData {
    fn trace(&self, visitor: &mut Visitor) {
        visitor.trace(&self.context);
        crate::cell::trace_cell(self.global.get() as *mut c_void, visitor);
        crate::cell::trace_cell(self.window_proxy.get() as *mut c_void, visitor);
        crate::cell::trace_cell(self.drain_function.get() as *mut c_void, visitor);
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesJsRealm"
    }
}

impl Drop for RealmData {
    fn drop(&mut self) {
        #[cfg(test)]
        REALMS_FINALIZED.with(|count| count.set(count.get() + 1));
        // Finalizers may run lazily, even after the runtime is gone: only the shared slot is
        // touched (null once the runtime's shutdown released it).
        let _gc = crate::object::GcScope::enter();
        release_principals(&self.principals);
    }
}

/// Drops the principals in `slot`, if still held (the destroy hook ignores the context).
fn release_principals(slot: &Cell<*mut JSPrincipals>) {
    let principals = slot.replace(std::ptr::null_mut());
    if !principals.is_null() {
        // SAFETY: the slot held one reference (`hold_principals`).
        unsafe { crate::jsapi_impl::JS_DropPrincipals(std::ptr::null_mut(), principals) };
    }
}

/// A realm in the runtime's weak registry. A collected realm whose finalizer has not run yet
/// keeps its entry while its principals are held, so shutdown can release them.
pub(crate) struct RealmEntry {
    data: WeakPersistent<RealmData>,
    principals: PrincipalsSlot,
}

// `UnsafePtr<RealmData>` is a single non-null pointer (see `cell`): realm pointers travel
// through mozjs-shaped APIs as `*mut Realm`.
const _: () = assert!(std::mem::size_of::<Option<UnsafePtr<RealmData>>>() == std::mem::size_of::<*mut Realm>());

fn to_raw(data: UnsafePtr<RealmData>) -> *mut Realm {
    // SAFETY: same size (asserted above); `UnsafePtr` only wraps the address.
    unsafe { std::mem::transmute::<UnsafePtr<RealmData>, *mut Realm>(data) }
}

fn from_raw(realm: *mut Realm) -> Option<UnsafePtr<RealmData>> {
    // SAFETY: inverse of `to_raw`; null maps to `None`.
    unsafe { std::mem::transmute::<*mut Realm, Option<UnsafePtr<RealmData>>>(realm) }
}

/// Registers a new realm for `context` with its `global` cell (null: the context's plain
/// global). The realm lives as long as its context (see the module documentation); the
/// caller's `context` local keeps it alive meanwhile.
pub(crate) fn register_realm(cx: &JSContext, scope: &mut v8::PinScope, context: v8::Local<v8::Context>, global: *mut JSObject) -> *mut Realm {
    // Every holder/cache object must belong to this context. Creating one in the caller's
    // context would make the new realm retain that unrelated realm via its creation context.
    let scope = &mut v8::ContextScope::new(scope, context);
    let holder = crate::script_impl::empty_holder(cx, scope).expect("a realm holder");
    let context_reference = TracedReference::new(scope, context);
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    let data = RealmData {
        context: context_reference,
        global: Cell::new(std::ptr::null_mut()),
        window_proxy: Cell::new(std::ptr::null_mut()),
        drain_function: Cell::new(std::ptr::null_mut()),
        principals: Rc::new(Cell::new(std::ptr::null_mut())),
    };
    // SAFETY: wrapped right away: no allocation runs before the holder owns it.
    let data = unsafe { v8::cppgc::make_garbage_collected(heap, data) };
    // SAFETY: the holder is an API object with one internal field; the tag is unique.
    unsafe { v8::Object::wrap::<REALM_WRAP_TAG, RealmData>(scope, holder, &data) };
    context.set_embedder_data(HOLDER_SLOT, holder.into());
    let weak = WeakPersistent::new(&data);
    // SAFETY: the context local keeps the data alive for this function.
    let data_ref = unsafe { &*(data.as_ref() as *const RealmData) };
    let realm = to_raw(data);
    // SAFETY: the context owns the holder, which owns the data: the pointer is valid while the
    // context can be reached.
    unsafe { context.set_aligned_pointer_in_embedder_data(REALM_SLOT, realm as *mut c_void) };
    let global = if global.is_null() {
        let object = context.global(scope);
        crate::jsval::from_v8(scope, object.into()).to_object()
    } else {
        global
    };
    data_ref.global.set(global);
    // SpiderMonkey has no access checks between realms (Servo checks cross-origin access
    // itself), so all of a runtime's contexts share one security token.
    // Tokens compare by identity: an internalized string is one object per isolate.
    let token = v8::String::new_from_utf8(scope, b"roves-js runtime", v8::NewStringType::Internalized).expect("a short string");
    context.set_security_token(token.into());
    let cache = v8::Object::new(scope);
    let null = v8::null(scope).into();
    cache.set_prototype(scope, null);
    context.set_embedder_data(CACHE_SLOT, cache.into());
    // `eval`, `Function` and WebAssembly compilation then go through the embedder's CSP check
    // (see `runtime_impl::JS_SetSecurityCallbacks`).
    context.set_allow_generation_from_strings(false);
    {
        let mut realms = cx.realms.borrow_mut();
        // Drop the entries of collected realms now and then, at sizes doubling with what
        // survived the last sweep (a constant cost per registration).
        if realms.len() >= cx.realm_sweep.get() {
            realms.retain(|entry| entry.data.get().is_some() || !entry.principals.get().is_null());
            cx.realm_sweep.set(REALM_SWEEP_MIN.max(2 * realms.len()));
        }
        realms.push(RealmEntry { data: weak, principals: data_ref.principals.clone() });
    }
    assert!(crate::proxy::initialize_window_wrappers(scope).is_some(), "initialize Window wrapper cache");
    assert!(crate::script_impl::initialize_metadata(cx, scope).is_some(), "initialize script metadata cache");
    assert!(crate::dynamic_code::install(scope).is_some(), "install dynamic Function CSP adapters");
    realm
}

/// The data of a valid realm (see the module documentation for validity).
pub(crate) fn realm_data<'r>(realm: *mut Realm) -> Option<&'r RealmData> {
    // SAFETY: callers pass valid realms; the data does not move while alive.
    from_raw(realm).map(|data| unsafe { &*(data.as_ref() as *const RealmData) })
}

/// The value cached under `key` in the current realm, made by `make` on first use. The cache
/// belongs to the realm's V8 context: helpers written in JS and proxy handler objects are
/// per realm, so the objects they create get that realm's prototypes, and they live as long
/// as the realm.
pub(crate) fn realm_cached<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: &str,
    make: impl FnOnce(&mut v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::Value>>,
) -> Option<v8::Local<'s, v8::Value>> {
    let context = scope.get_current_context();
    let cache = v8::Local::<v8::Object>::try_from(context.get_embedder_data(scope, CACHE_SLOT)?).ok()?;
    let key = v8::String::new(scope, key)?;
    let cached = cache.get(scope, key.into())?;
    if !cached.is_undefined() {
        return Some(cached);
    }
    let value = make(scope)?;
    cache.set(scope, key.into(), value)?;
    Some(value)
}

/// The realm a context belongs to.
pub(crate) fn realm_of_context(context: v8::Local<v8::Context>) -> *mut Realm {
    context.get_aligned_pointer_from_embedder_data(REALM_SLOT) as *mut Realm
}

/// The realm an object was created in (null for objects without a creation context).
pub(crate) fn realm_of_object(cx: &JSContext, obj: *mut JSObject) -> *mut Realm {
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    if let Some(class_box) = crate::object::class_box(obj) {
        if !class_box.proxy_owner().is_null() {
            // A foreign Window wrapper belongs to its represented Window for
            // origin checks. The traced owner supplies its lifetime and realm;
            // avoid an extra persistent Context root on the wrapper.
            return realm_of_object(cx, class_box.proxy_owner());
        }
    }
    cx.with_scope(|scope| {
        // SAFETY: JSAPI callers pass live objects.
        let value = unsafe { crate::cell::cell_value(scope, obj as *mut c_void) };
        let Ok(object) = v8::Local::<v8::Object>::try_from(value) else { return std::ptr::null_mut() };
        match object.get_creation_context(scope) {
            Some(context) => realm_of_context(context),
            None => std::ptr::null_mut(),
        }
    })
}

/// Saves the current realm (a root until [`leave`]) and makes the valid `realm` current (a
/// null one keeps the current realm). Returns the saved realm. No allocation runs in between,
/// so both stay rooted throughout.
pub(crate) fn enter(cx: &JSContext, realm: *mut Realm) -> *mut Realm {
    let old = cx.current_realm.get();
    cx.saved_realms.borrow_mut().push(old);
    if !realm.is_null() {
        cx.current_realm.set(realm);
    }
    old
}

/// Restores `old` (the result of the matching [`enter`]) as the current realm and drops its
/// saved entry. Enter and leave must nest (the JSAPI contract): out of order, debug builds
/// assert; release builds drop the innermost entry for `old`, keeping the others rooted, and
/// restore `old` only if it is still a known live realm, never a pointer nothing roots.
pub(crate) fn leave(cx: &JSContext, old: *mut Realm) {
    let mut saved = cx.saved_realms.borrow_mut();
    let index = saved.iter().rposition(|realm| *realm == old);
    debug_assert!(
        index.is_some() && index == saved.len().checked_sub(1),
        "LeaveRealm must restore the realm of the innermost EnterRealm"
    );
    let Some(index) = index else {
        drop(saved);
        if is_live_realm(cx, old) {
            cx.current_realm.set(old);
        }
        return;
    };
    // `old` is rooted by its entry until it is current again.
    if !old.is_null() {
        cx.current_realm.set(old);
    }
    saved.remove(index);
}

/// Whether `realm` is a registered realm that was not collected (a scan, for misuse only).
fn is_live_realm(cx: &JSContext, realm: *mut Realm) -> bool {
    !realm.is_null() && find_live_realm(cx, |live| live == realm).is_some()
}

/// The first registered realm that was not collected and satisfies `accept` (which must not
/// allocate). The result is valid until the next allocation.
pub(crate) fn find_live_realm(cx: &JSContext, mut accept: impl FnMut(*mut Realm) -> bool) -> Option<*mut Realm> {
    let realms = cx.realms.borrow();
    realms.iter().find_map(|entry| {
        entry.data.get()?;
        // SAFETY: the entry is alive (checked above) and nothing collects before `accept`.
        let realm = to_raw(unsafe { UnsafePtr::new(&entry.data) }?);
        accept(realm).then_some(realm)
    })
}

/// Strong handles on every registered realm that was not collected (for an iteration whose
/// callbacks may allocate).
pub(crate) fn live_realms(cx: &JSContext) -> Vec<Persistent<RealmData>> {
    let realms = cx.realms.borrow();
    realms.iter().filter(|entry| entry.data.get().is_some()).map(|entry| Persistent::new(&entry.data)).collect()
}

/// The realm pointer of a strong handle from [`live_realms`].
pub(crate) fn persistent_realm(data: &Persistent<RealmData>) -> *mut Realm {
    // SAFETY: the persistent keeps the data alive.
    unsafe { UnsafePtr::new(data) }.map_or(std::ptr::null_mut(), to_raw)
}

/// Enters the realm of `target`'s global; returns the realm that was current.
pub unsafe fn EnterRealm(cx: *mut JSContext, target: *mut JSObject) -> *mut Realm {
    // SAFETY: callers pass a live context (JSAPI contract).
    let cx = unsafe { &*cx };
    // The caller roots `target`, which keeps its realm alive until it becomes current.
    let realm = realm_of_object(cx, target);
    enter(cx, realm)
}

/// Restores `old_realm` as the current realm (after `EnterRealm`, in nesting order).
pub unsafe fn LeaveRealm(cx: *mut JSContext, old_realm: *mut Realm) {
    // SAFETY: callers pass a live context (JSAPI contract).
    leave(unsafe { &*cx }, old_realm);
}

pub unsafe fn GetObjectRealmOrNull(obj: *mut JSObject) -> *mut Realm {
    realm_of_object(JSContext::current(), obj)
}

/// The global of a valid realm (see the module documentation).
pub unsafe fn GetRealmGlobalOrNull(realm: *mut Realm) -> *mut JSObject {
    realm_data(realm).map_or(std::ptr::null_mut(), |data| data.global.get())
}

/// The principals of a valid realm, borrowed: valid as long as the realm (no reference taken).
pub unsafe fn GetRealmPrincipals(realm: *mut Realm) -> *mut JSPrincipals {
    realm_data(realm).map_or(std::ptr::null_mut(), |data| data.principals.get())
}

/// The current realm of `cx` (mozjs's `get_context_realm`).
pub fn get_context_realm(cx: *mut JSContext) -> *mut Realm {
    // SAFETY: callers pass a live context.
    unsafe { &*cx }.current_realm.get()
}

/// The realm of `obj` (mozjs's `get_object_realm`).
pub fn get_object_realm(obj: *mut JSObject) -> *mut Realm {
    realm_of_object(JSContext::current(), obj)
}

/// Keeps the current realm and the saved ones alive (SpiderMonkey keeps the realms of the
/// active stack). Other realms live only while something reaches their context.
pub(crate) fn trace_realm_roots(cx: &JSContext, visitor: &mut Visitor) {
    trace_realm(cx.current_realm.get(), visitor);
    #[cfg(test)]
    {
        if STRONG_REALMS_FOR_TESTING.with(Cell::get) {
            for entry in cx.realms.borrow().iter() {
                visitor.trace(&Member::new(&entry.data));
            }
        }
        if UNROOTED_SAVED_REALMS_FOR_TESTING.with(Cell::get) {
            return;
        }
    }
    // A borrow failure would mean a GC inside a push or pop: abort rather than skip roots.
    for realm in cx.saved_realms.borrow().iter() {
        trace_realm(*realm, visitor);
    }
}

fn trace_realm(realm: *mut Realm, visitor: &mut Visitor) {
    if let Some(data) = from_raw(realm) {
        visitor.trace(&Member::new(&data));
    }
}

/// Releases the principals of every realm, collected or not, at runtime shutdown (before the
/// isolate goes): realm data finalized later finds them released.
pub(crate) fn release_realm_principals(cx: &JSContext) {
    let slots: Vec<PrincipalsSlot> = cx.realms.borrow().iter().map(|entry| entry.principals.clone()).collect();
    for slot in slots {
        release_principals(&slot);
    }
}

pub unsafe fn GetCurrentRealmOrNull(cx: *mut JSContext) -> *mut Realm {
    get_context_realm(cx)
}

pub unsafe fn CurrentGlobalOrNull(cx: *mut JSContext) -> *mut JSObject {
    // SAFETY: forwarded.
    unsafe { GetRealmGlobalOrNull(get_context_realm(cx)) }
}

/// The current realm's global, as a location in the realm's data: valid while that realm is
/// current or saved.
pub unsafe fn CurrentGlobal(cx: *mut JSContext) -> *const *mut JSObject {
    realm_data(get_context_realm(cx)).map_or(std::ptr::null(), |data| data.global.as_ptr() as *const *mut JSObject)
}

/// A new global object of `clasp` in a new realm (a new V8 context whose global proxy is the
/// class object). The new realm is not entered.
pub unsafe fn JS_NewGlobalObject(
    cx: *mut JSContext,
    clasp: *const crate::jsapi::JSClass,
    principals: *mut JSPrincipals,
    _hook_option: crate::jsapi::OnNewGlobalHookOption,
    options: *const crate::jsapi::RealmOptions,
) -> *mut JSObject {
    // SAFETY: callers pass a live context.
    let raw = unsafe { &*cx };
    let created = raw.catching(|scope| {
        let template = raw.class_templates.template(scope, clasp);
        let context = v8::Context::new(scope, v8::ContextOptions { global_template: Some(template), ..Default::default() });
        // The class box goes on the global proxy (what scripts see as `globalThis`) before
        // anything converts it.
        let global = context.global(scope);
        crate::object::initialize_window_prototype(scope, global, clasp)?;
        let cell = crate::object::attach_class_box(scope, global, clasp);
        // SAFETY: callers pass valid options (or null).
        if let Some(options) = unsafe { options.as_ref() } {
            if let Some(class_box) = crate::object::class_box(cell) {
                class_box.global_trace.set(options.creationOptions_.traceGlobal_);
            }
        }
        // The `context` local keeps the realm alive until the caller roots the global.
        let realm = register_realm(raw, scope, context, cell);
        // V8 gives every context a built-in `console` (silent without an inspector), which
        // would shadow the embedder's (Servo defines `console` lazily, like its interfaces).
        // After registration: the realm's shared security token allows the access.
        let console = v8::String::new(scope, "console")?;
        global.delete(scope, console.into())?;
        // Last: a failed creation holds no principals (its realm is simply collected).
        if !principals.is_null() {
            realm_data(realm)?.hold_principals(principals);
        }
        Some(cell)
    });
    created.unwrap_or(std::ptr::null_mut())
}

/// SpiderMonkey traces a global's intrinsics here; V8 contexts keep their own alive.
pub unsafe extern "C" fn JS_GlobalObjectTraceHook(_trc: *mut crate::jsapi::JSTracer, _global: *mut JSObject) {}

pub unsafe fn JS_FireOnNewGlobalObject(_cx: *mut JSContext, _global: crate::jsapi::HandleObject) {}

/// Install DOM interface placeholders only once the embedder's global and document
/// are ready for exposure checks (the new-global notification is too early).
pub unsafe fn JS_InitGlobalInterfaceProperties(cx: *mut JSContext, global: crate::jsapi::HandleObject) -> bool {
    // SAFETY: callers initialize the global before requesting its interface names.
    let old = unsafe { EnterRealm(cx, global.get()) };
    let initialized = crate::object::initialize_global_properties(unsafe { &*cx }, global.get());
    unsafe { LeaveRealm(cx, old) };
    initialized
}

/// V8 contexts are created with every standard class already defined: nothing to resolve.
pub unsafe extern "C" fn JS_ResolveStandardClass(_cx: *mut JSContext, _obj: crate::jsapi::HandleObject, _id: crate::jsapi::HandleId, resolved: *mut bool) -> bool {
    // SAFETY: callers pass a valid out pointer.
    unsafe { *resolved = false };
    true
}

pub unsafe extern "C" fn JS_MayResolveStandardClass(_names: *const crate::jsapi::JSAtomState, _id: crate::jsid::jsid, _maybe_obj: *mut JSObject) -> bool {
    false
}

pub unsafe extern "C" fn JS_NewEnumerateStandardClasses(_cx: *mut JSContext, _obj: crate::jsapi::HandleObject, _properties: crate::jsapi::MutableHandleIdVector, _enumerable_only: bool) -> bool {
    true
}

/// The window proxy registered for the realm whose global is `global`, the cell of the class
/// object `object` (from the object's own realm, no registry scan).
pub(crate) fn window_proxy_of_global(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, global: *mut c_void) -> Option<*mut c_void> {
    let context = object.get_creation_context(scope)?;
    let data = realm_data(realm_of_context(context))?;
    if data.global.get() as *mut c_void != global {
        return None;
    }
    let proxy = data.window_proxy.get() as *mut c_void;
    (!proxy.is_null()).then_some(proxy)
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// How many registered realms were not collected.
    pub(crate) fn live_realm_count(cx: &JSContext) -> usize {
        cx.realms.borrow().iter().filter(|entry| entry.data.get().is_some()).count()
    }

    /// The registry size (live and collected entries not yet swept).
    pub(crate) fn registry_len(cx: &JSContext) -> usize {
        cx.realms.borrow().len()
    }

    /// A weak handle on a valid realm's data.
    pub(crate) fn realm_weak(realm: *mut Realm) -> WeakPersistent<RealmData> {
        WeakPersistent::new(&from_raw(realm).expect("a realm"))
    }

    /// Makes the initial realm current and forgets the saved realms, so a test that found a
    /// realm collected never restores a dangling pointer.
    pub(crate) fn reset_realms_for_test(cx: &JSContext, initial: *mut Realm) {
        cx.saved_realms.borrow_mut().clear();
        cx.current_realm.set(initial);
    }

    /// The saved-realm stack depth.
    pub(crate) fn saved_depth(cx: &JSContext) -> usize {
        cx.saved_realms.borrow().len()
    }
}
