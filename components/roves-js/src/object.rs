/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! SpiderMonkey's class-based objects on V8.
//!
//! An object created with a `JSClass` is a V8 API object whose internal field 0 points at a
//! cppgc [`ClassBox`] (and which is `Object::wrap`ped to it, so the box lives with the object).
//! The box holds the class, the reserved slots and the object's own cell; the cell points back
//! at the box. Reserved slots are therefore read without touching V8, which the class trace
//! hook needs (it runs during the GC) and the finalize hook needs (it runs after the V8 object
//! is gone). The box's trace calls the class trace hook, its destruction the finalize hook.

use std::cell::{Cell as StdCell, RefCell};
use std::ffi::{CStr, c_void};

use v8::cppgc::{GarbageCollected, Member, UnsafePtr, Visitor};

use crate::cell::trace_cell;
use crate::jsapi::{JSClass, JSContext, JSObject};
use crate::jsval::{JSVal, UndefinedValue};

/// `JSCLASS_*` flags and masks (SpiderMonkey's values).
pub const JSCLASS_RESERVED_SLOTS_SHIFT: u32 = 0;
pub const JSCLASS_RESERVED_SLOTS_WIDTH: u32 = 8;
pub const JSCLASS_RESERVED_SLOTS_MASK: u32 = (1 << JSCLASS_RESERVED_SLOTS_WIDTH) - 1;
pub const JSCLASS_IS_DOMJSCLASS: u32 = 1 << 4;
pub const JSCLASS_USERBIT1: u32 = 1 << 7;
pub const JSCLASS_IS_PROXY: u32 = 1 << 9;
pub const JSCLASS_IS_GLOBAL: u32 = 1 << 17;
pub const JSCLASS_GLOBAL_SLOT_COUNT: u32 = 6;
pub const JSCLASS_FOREGROUND_FINALIZE: u32 = 1 << 19;

/// Internal-field tag of the class box pointer (shared with roves-v8's wrapper tag; the two
/// never coexist in one isolate).
const CLASS_BOX_TAG: u16 = 0;
/// `Object::wrap` tag of class boxes.
const CLASS_WRAP_TAG: u16 = 1;

/// The per-object state of a class-based object.
pub(crate) struct ClassBox {
    pub(crate) class: *const JSClass,
    /// Inline for the few slots DOM classes declare: a wrapper is created per DOM node, and a
    /// heap vector per wrapper was a `malloc`/`free` pair each.
    slots: RefCell<Slots>,
    /// The object's own cell (set right after allocation).
    pub(crate) cell: StdCell<*mut c_void>,
    /// For a proxy's target: the proxy handler (null for ordinary class objects). The box's
    /// `cell` is then the proxy's cell, and trace/finalize go to the handler's traps.
    pub(crate) proxy: StdCell<*const crate::proxy::ProxyHandler>,
    /// A borrowed Window wrapper: its owner is its identity's current window proxy, and it
    /// owns no DOM state.
    pub(crate) window_wrapper: StdCell<bool>,
    /// For window proxies and their wrappers: the browsing context's identity, shared across
    /// navigations (`JS_TransplantObject`). Isolate thread only (no concurrent marking).
    pub(crate) window_identity: StdCell<Option<UnsafePtr<WindowIdentity>>>,
    /// A proxy's private value (`GetProxyPrivate`).
    pub(crate) private: StdCell<JSVal>,
    /// Whether a proxy's prototype comes from its handler (`getPrototype` trap).
    pub(crate) lazy_proto: StdCell<bool>,
    /// For a global: its realm's `traceGlobal` hook (`RealmCreationOptions`), which traces the
    /// embedder's global state alongside the class hook.
    pub(crate) global_trace: StdCell<crate::jsapi::JSTraceOp>,
    /// The prototype SpiderMonkey's `JS_SetImmutablePrototype` fixed, if it was called. Traced
    /// by the box rather than rooted, so a class object and its prototype can still be
    /// collected together when a prototype property points back at the object.
    pub(crate) immutable_prototype: RefCell<Option<v8::TracedReference<v8::Value>>>,
}

thread_local! {
    /// How many class-box traces or finalizers are running (`RuntimeHeapState`).
    static IN_GC: StdCell<u32> = const { StdCell::new(0) };
    /// The box being finalized, and the stand-in object pointer its finalize hook receives.
    static FINALIZING: StdCell<Option<(*mut JSObject, *const ClassBox)>> = const { StdCell::new(None) };
}

// SAFETY: `trace` reports every slot, the own cell, and calls the class trace hook, which
// reports the native object's references (Servo's `JSTraceable`).
unsafe impl GarbageCollected for ClassBox {
    fn trace(&self, visitor: &mut Visitor) {
        let _gc = GcScope::enter();
        for slot in self.slots.borrow().iter() {
            if slot.is_gcthing() {
                trace_cell(slot.to_gcthing(), visitor);
            }
        }
        trace_cell(self.cell.get(), visitor);
        if let Some(identity) = self.identity() {
            visitor.trace(&Member::new(&identity));
        }
        let private = self.private.get();
        if private.is_gcthing() {
            trace_cell(private.to_gcthing(), visitor);
        }
        if let Some(prototype) = self.immutable_prototype.borrow().as_ref() {
            visitor.trace(prototype);
        }
        // Proxy handler creation may allocate after this box has been wrapped but before
        // its JSAPI cell exists. Trace owned values above, but never expose a null object
        // to native hooks that read reserved slots through that cell.
        if self.window_wrapper.get() || self.cell.get().is_null() {
            return;
        }
        // SAFETY: handlers live for the whole process (`CreateProxyHandler`).
        if let Some(handler) = unsafe { self.proxy.get().as_ref() } {
            if let Some(trace) = handler.traps.trace {
                // SAFETY: as for the class hook below.
                unsafe { trace(crate::glue::tracer(visitor), self.cell.get() as *mut JSObject) };
            }
            return;
        }
        // SAFETY: classes are static.
        let class = unsafe { &*self.class };
        if let Some(trace) = self.global_trace.get() {
            // SAFETY: as for the class hook below.
            unsafe { trace(crate::glue::tracer(visitor), self.cell.get() as *mut JSObject) };
        }
        if let Some(ops) = unsafe { class.cOps.as_ref() } {
            if let Some(trace) = ops.trace {
                // SAFETY: the hook receives this GC's tracer and the object (its cell).
                unsafe { trace(crate::glue::tracer(visitor), self.cell.get() as *mut JSObject) };
            }
        }
    }

    /// Heap snapshots name a box after its class (`Window`, `HTMLDivElement`, ...); borrowed
    /// Window wrappers own no DOM state and are named apart from the windows they stand for.
    fn get_name(&self) -> &'static std::ffi::CStr {
        if self.window_wrapper.get() {
            return c"RovesJsWindowWrapper";
        }
        // SAFETY: classes are static, and so are their names (SpiderMonkey's `JSClass`
        // contract; every class here names itself with a C string literal), which is what V8
        // needs from a name returned outside snapshot generation.
        let name = unsafe { (*self.class).name };
        if name.is_null() { c"RovesJsClassBox" } else { unsafe { CStr::from_ptr(name) } }
    }
}

/// Reserved slot storage (DOM wrappers declare one to three slots; globals more).
type Slots = smallvec::SmallVec<[JSVal; 4]>;

/// Marks the extent of GC work run by roves-js (traces and finalizers).
pub(crate) struct GcScope;

impl GcScope {
    pub(crate) fn enter() -> GcScope {
        IN_GC.with(|depth| depth.set(depth.get() + 1));
        GcScope
    }
}

impl Drop for GcScope {
    fn drop(&mut self) {
        IN_GC.with(|depth| depth.set(depth.get() - 1));
    }
}

/// SpiderMonkey's heap state: collecting while roves-js runs GC hooks.
pub unsafe fn RuntimeHeapState() -> crate::jsapi::HeapState {
    if IN_GC.with(StdCell::get) > 0 { crate::jsapi::HeapState::MajorCollecting } else { crate::jsapi::HeapState::Idle }
}

impl Drop for ClassBox {
    fn drop(&mut self) {
        if self.window_wrapper.get() {
            return;
        }
        let _gc = GcScope::enter();
        // SAFETY: classes are static; handlers live for the whole process.
        let class = unsafe { &*self.class };
        let finalize = match unsafe { self.proxy.get().as_ref() } {
            Some(handler) => handler.traps.finalize,
            None => (unsafe { class.cOps.as_ref() }).and_then(|ops| ops.finalize),
        };
        let Some(finalize) = finalize else { return };
        // The V8 object (and possibly its cell) is gone: the hook gets a stand-in pointer that
        // the reserved-slot accessors resolve to this box.
        let stand_in = self as *const ClassBox as *mut JSObject;
        FINALIZING.with(|finalizing| finalizing.set(Some((stand_in, self as *const ClassBox))));
        // SAFETY: SpiderMonkey's finalize contract: the hook only reads the reserved slots.
        unsafe { finalize(std::ptr::null_mut(), stand_in) };
        FINALIZING.with(|finalizing| finalizing.set(None));
    }
}

fn box_from_raw<'b>(pointer: *mut c_void) -> Option<&'b ClassBox> {
    // SAFETY: `UnsafePtr` is a single non-null pointer (see `cell`); boxes live while their
    // object or cell does.
    let class_box = unsafe { std::mem::transmute::<*mut c_void, Option<UnsafePtr<ClassBox>>>(pointer) }?;
    Some(unsafe { &*(class_box.as_ref() as *const ClassBox) })
}

fn box_to_raw(class_box: &UnsafePtr<ClassBox>) -> *mut c_void {
    // SAFETY: see `box_from_raw` (`UnsafePtr` is a single pointer).
    unsafe { std::mem::transmute_copy::<UnsafePtr<ClassBox>, *mut c_void>(class_box) }
}

/// The class box of a class-based object (or of the object being finalized).
pub(crate) fn class_box<'b>(obj: *mut JSObject) -> Option<&'b ClassBox> {
    if obj.is_null() {
        return None;
    }
    if let Some((stand_in, finalizing)) = FINALIZING.with(StdCell::get) {
        if stand_in == obj {
            // SAFETY: the box is alive during its own finalize hook.
            return Some(unsafe { &*finalizing });
        }
    }
    let class_box = crate::cell::cell_class_box(obj as *mut c_void)?;
    // SAFETY: the cell's (traced) member points at a live box.
    Some(unsafe { &*(class_box as *const ClassBox) })
}

/// The class box behind a V8 object, if it is a class-based object.
pub(crate) fn class_box_of_v8<'b>(object: v8::Local<v8::Object>) -> Option<&'b ClassBox> {
    if object.internal_field_count() < 1 {
        return None;
    }
    // SAFETY: class-based objects store their box under this tag.
    let raw = unsafe { object.get_aligned_pointer_from_internal_field(0, CLASS_BOX_TAG) };
    if raw.is_null() { None } else { box_from_raw(raw as *mut c_void) }
}

/// Whether a class object was marked with SpiderMonkey's immutable-prototype flag.
pub(crate) fn has_immutable_prototype(object: v8::Local<v8::Object>) -> bool {
    class_box_of_v8(object).is_some_and(|class_box| class_box.immutable_prototype.borrow().is_some())
}

/// Whether the JSAPI object cell was marked with an immutable prototype.
pub(crate) fn cell_has_immutable_prototype(object: *mut JSObject) -> bool {
    class_box(object).is_some_and(|class_box| class_box.immutable_prototype.borrow().is_some())
}

/// The original prototype stored when the immutable flag was set.
pub(crate) fn immutable_prototype<'s>(scope: &v8::PinScope<'s, '_>, object: v8::Local<v8::Object>) -> Option<v8::Local<'s, v8::Value>> {
    class_box_of_v8(object)?.immutable_prototype.borrow().as_ref()?.get(scope)
}

/// The immutable prototype stored on a JSAPI object cell.
pub(crate) fn immutable_prototype_for_cell<'s>(scope: &v8::PinScope<'s, '_>, object: *mut JSObject) -> Option<v8::Local<'s, v8::Value>> {
    class_box(object)?.immutable_prototype.borrow().as_ref()?.get(scope)
}

/// A browsing context's window proxy identity. Navigations replace the window proxy
/// (`JS_TransplantObject`); every proxy that stood for the context and every wrapper other
/// realms made for it share this record, which points only at the current proxy, so older
/// proxies and their windows are not kept alive by it.
pub(crate) struct WindowIdentity {
    /// The window proxy that now stands for the context (traced).
    current: StdCell<*mut JSObject>,
    /// A non-registered symbol: receiving realms' wrapper caches use it as WeakMap key, which
    /// stays the same across replacements and keeps no realm alive.
    token: v8::TracedReference<v8::Value>,
}

// SAFETY: `trace` reports both references.
unsafe impl GarbageCollected for WindowIdentity {
    fn trace(&self, visitor: &mut Visitor) {
        trace_cell(self.current.get() as *mut c_void, visitor);
        visitor.trace(&self.token);
    }

    fn get_name(&self) -> &'static CStr {
        c"RovesJsWindowIdentity"
    }
}

impl WindowIdentity {
    pub(crate) fn token<'s>(&self, scope: &v8::PinScope<'s, '_>) -> Option<v8::Local<'s, v8::Value>> {
        self.token.get(scope)
    }
}

/// The identity of the window proxy `class_box` (its own cell), made on first use.
pub(crate) fn window_identity(scope: &mut v8::PinScope, class_box: &ClassBox) -> UnsafePtr<WindowIdentity> {
    if let Some(identity) = class_box.identity() {
        return identity;
    }
    let token = v8::Symbol::new(scope, None);
    let token = v8::TracedReference::new(scope, token.into());
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: stored in the (live) box at once.
    let identity = unsafe {
        v8::cppgc::make_garbage_collected(heap, WindowIdentity { current: StdCell::new(class_box.cell.get() as *mut JSObject), token })
    };
    class_box.window_identity.set(Some(identity));
    class_box.identity().expect("just set")
}

/// `replacement` takes over the identity of the window proxy `original` (a navigation).
pub(crate) fn transfer_window_identity(scope: &mut v8::PinScope, original: &ClassBox, replacement: &ClassBox) {
    let identity = window_identity(scope, original);
    // SAFETY: just read from a live box, which traces it.
    unsafe { identity.as_ref() }.current.set(replacement.cell.get() as *mut JSObject);
    replacement.window_identity.set(Some(identity));
}

/// The window proxy that now stands for `proxy` (itself unless a navigation replaced it).
pub(crate) fn current_window_proxy(proxy: *mut JSObject) -> *mut JSObject {
    class_box(proxy).and_then(ClassBox::identity_proxy).unwrap_or(proxy)
}

impl ClassBox {
    fn identity(&self) -> Option<UnsafePtr<WindowIdentity>> {
        // SAFETY: `UnsafePtr` is a single pointer; its derived `Copy` only lacks `T: Copy`.
        unsafe { std::ptr::read(self.window_identity.as_ptr()) }
    }

    fn identity_proxy(&self) -> Option<*mut JSObject> {
        // SAFETY: the box traces its identity.
        self.identity().map(|identity| unsafe { identity.as_ref() }.current.get())
    }

    pub(crate) fn proxy_owner(&self) -> *mut JSObject {
        if self.window_wrapper.get() { self.identity_proxy().unwrap_or(std::ptr::null_mut()) } else { std::ptr::null_mut() }
    }

    pub(crate) fn proxy_private(&self) -> JSVal {
        match class_box(self.proxy_owner()) {
            Some(owner) => owner.proxy_private(),
            None => self.private.get(),
        }
    }

    /// Replaces the reserved slots with `other`'s (`JS_TransplantObject`).
    pub(crate) fn copy_slots_from(&self, other: &ClassBox) {
        let slots = other.slots.borrow().clone();
        *self.slots.borrow_mut() = slots;
    }

    pub(crate) fn reserved_slot(&self, index: u32) -> JSVal {
        if let Some(owner) = class_box(self.proxy_owner()) {
            return owner.reserved_slot(index);
        }
        self.slots.borrow().get(index as usize).copied().unwrap_or_else(UndefinedValue)
    }

    pub(crate) fn set_reserved_slot(&self, index: u32, value: JSVal) {
        if let Some(owner) = class_box(self.proxy_owner()) {
            owner.set_reserved_slot(index, value);
            return;
        }
        let mut slots = self.slots.borrow_mut();
        let index = index as usize;
        if index >= slots.len() {
            slots.resize(index + 1, UndefinedValue());
        }
        slots[index] = value;
    }
}

/// Reserved slots a class declares.
fn reserved_slot_count(class: &JSClass) -> usize {
    let mut count = ((class.flags >> JSCLASS_RESERVED_SLOTS_SHIFT) & JSCLASS_RESERVED_SLOTS_MASK) as usize;
    if class.flags & JSCLASS_IS_GLOBAL != 0 {
        count += JSCLASS_GLOBAL_SLOT_COUNT as usize;
    }
    count
}

/// Per-runtime cache of the object template of each class.
#[derive(Default)]
pub(crate) struct ClassTemplates {
    /// By class address (one lookup per class object created: a cheap hasher).
    templates: RefCell<rustc_hash::FxHashMap<usize, v8::Global<v8::ObjectTemplate>>>,
}

impl ClassTemplates {
    pub(crate) fn template<'s>(&self, scope: &mut v8::PinScope<'s, '_>, class: *const JSClass) -> v8::Local<'s, v8::ObjectTemplate> {
        if let Some(template) = self.templates.borrow().get(&(class as usize)) {
            return v8::Local::new(scope, template);
        }
        let template = if is_window_global_class(class) {
            // Give V8 its own canonical Window prototype before freezing the global's
            // prototype. Servo initializes that object later instead of replacing it.
            let constructor = v8::FunctionTemplate::new(scope, window_constructor);
            constructor.prototype_template(scope).set_internal_field_count(1);
            let template = constructor.instance_template(scope);
            template.set_immutable_proto();
            template
        } else {
            v8::ObjectTemplate::new(scope)
        };
        template.set_internal_field_count(1);
        // SAFETY: classes are static.
        let ops = unsafe { (*class).cOps.as_ref() };
        if ops.is_some_and(|ops| ops.call.is_some() || ops.construct.is_some()) {
            let data = v8::External::new(scope, class as *mut c_void);
            template.set_call_as_function_handler(call_class_hook, Some(data.into()));
        }
        if ops.is_some_and(|ops| ops.resolve.is_some() || ops.newEnumerate.is_some()) &&
            !uses_native_global_properties(class) {
            // Fallback for class hooks that cannot enumerate DOM interface names.
            // Resolution precedes the prototype chain, then steps aside for existing
            // own properties. DOM globals use per-property V8 callbacks instead.
            let data = v8::External::new(scope, class as *mut c_void);
            let configuration = v8::NamedPropertyHandlerConfiguration::new()
                .getter(resolve_getter)
                .query(resolve_query)
                .enumerator(resolve_enumerator)
                .data(data.into())
                .flags(v8::PropertyHandlerFlags::ONLY_INTERCEPT_STRINGS);
            template.set_named_property_handler(configuration);
        }
        self.templates.borrow_mut().insert(class as usize, v8::Global::new(scope, template));
        template
    }
}

pub(crate) fn is_global_class(class: *const JSClass) -> bool {
    // SAFETY: all class pointers passed to the object adapter refer to static classes.
    unsafe { &*class }.flags & JSCLASS_IS_GLOBAL != 0
}

pub(crate) fn is_window_global_class(class: *const JSClass) -> bool {
    // SAFETY: all class pointers passed to the object adapter refer to static classes.
    let class = unsafe { &*class };
    class.flags & JSCLASS_IS_GLOBAL != 0 && !class.name.is_null() &&
        unsafe { CStr::from_ptr(class.name) }.to_bytes() == b"Window"
}

fn window_constructor(scope: &mut v8::PinScope, _: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    crate::native::throw_type_error(scope, "Illegal constructor");
}

/// V8 initializes API-object fields as JavaScript values, not tagged native pointers.
/// Prepare the still-unbound prototype before any conversion tries to read its class box.
pub(crate) fn initialize_window_prototype(scope: &mut v8::PinScope, global: v8::Local<v8::Object>, class: *const JSClass) -> Option<()> {
    if is_window_global_class(class) {
        let prototype = global.get_prototype(scope)?;
        let prototype = v8::Local::<v8::Object>::try_from(prototype).ok()?;
        // The Window template reserves this field and no class box is attached yet.
        prototype.set_aligned_pointer_in_internal_field(0, std::ptr::null_mut(), CLASS_BOX_TAG);
    }
    Some(())
}

/// Servo's DOM globals enumerate their lazy interface data properties. V8 can own
/// their initialization without intercepting every access to standard intrinsics.
pub(crate) fn uses_native_global_properties(class: *const JSClass) -> bool {
    // SAFETY: classes are static.
    let class = unsafe { &*class };
    class.flags & (JSCLASS_IS_GLOBAL | JSCLASS_USERBIT1) == (JSCLASS_IS_GLOBAL | JSCLASS_USERBIT1) &&
        unsafe { class.cOps.as_ref() }.is_some_and(|ops| ops.resolve.is_some() && ops.newEnumerate.is_some())
}

/// Called only after the embedder initializes a DOM global's reserved slots.
pub(crate) fn initialize_global_properties(cx: &JSContext, obj: *mut JSObject) -> bool {
    let Some(class_box) = class_box(obj) else { return true };
    let class = class_box.class;
    if !uses_native_global_properties(class) {
        return true;
    }
    cx.catching(|scope| {
        // SAFETY: the class owns static callbacks and `obj` is its live global.
        let enumerate = unsafe { (*(*class).cOps).newEnumerate }?;
        let raw_cx = cx as *const JSContext as *mut JSContext;
        crate::rooted!(in(raw_cx) let object = obj);
        let mut ids = unsafe { crate::rust::IdVector::new(raw_cx) };
        if !unsafe { enumerate(raw_cx, object.handle().into_handle(), ids.handle_mut(), false) } {
            crate::native::rethrow_pending(scope, cx);
            return None;
        }
        let global = v8::Local::<v8::Object>::try_from(unsafe { crate::cell::cell_value(scope, obj as *mut c_void) }).ok()?;
        let data = v8::External::new(scope, class as *mut c_void);
        for id in ids.iter() {
            let key = v8::Local::<v8::Name>::try_from(crate::jsapi_impl::id_key(scope, *id)?).ok()?;
            if global.has_real_named_property(scope, key)? {
                continue;
            }
            install_global_property(scope, global, key, data.into())?.then_some(())?;
        }
        Some(())
    }).is_some()
}

fn install_global_property(
    scope: &v8::PinScope, global: v8::Local<v8::Object>, key: v8::Local<v8::Name>, data: v8::Local<v8::Value>,
) -> Option<bool> {
    global.set_accessor_with_configuration(scope, key,
        v8::AccessorConfiguration::new(global_property_getter)
            .setter(global_property_setter).data(data)
            .property_attribute(v8::PropertyAttribute::DONT_ENUM))
}

fn global_property_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>, key: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>, mut retval: v8::ReturnValue<v8::Value>,
) {
    let Some(external) = v8::Local::<v8::External>::try_from(args.data()).ok() else { return };
    let class = external.value() as *const JSClass;
    // SAFETY: registration only uses DOM classes with a static resolve hook.
    let Some(resolve) = (unsafe { (*(*class).cOps).resolve }) else { return };
    let holder = args.holder();
    let Some(obj) = interceptor_object(scope, holder) else { return };
    // Existing resolve callbacks define the final descriptor themselves. A native
    // data property permits this; V8's auto-replacing lazy property does not.
    if holder.delete(scope, key.into()) != Some(true) { return }
    let cx = JSContext::current() as *const JSContext as *mut JSContext;
    let id = crate::jsapi_impl::key_id(scope, key.into());
    crate::rooted!(in(cx) let object = obj);
    crate::rooted!(in(cx) let id = id);
    // Access through another same-origin global must create interface objects in
    // their owner's realm, then restore the caller even when resolution fails.
    let old_realm = unsafe { crate::realm_impl::EnterRealm(cx, obj) };
    let mut resolved = false;
    let ok = unsafe { resolve(cx, object.handle().into_handle(), id.handle().into_handle(), &mut resolved) };
    let value = if ok && resolved { resolved_value(scope, holder, key, holder) } else { None };
    unsafe { crate::realm_impl::LeaveRealm(cx, old_realm) };
    if !ok {
        if holder.has_real_named_property(scope, key) == Some(false) {
            install_global_property(scope, holder, key, args.data());
        }
        crate::native::rethrow_pending(scope, JSContext::current());
        return;
    }
    if let Some(value) = value {
        retval.set(value);
    }
}

fn global_property_setter<'s>(
    scope: &mut v8::PinScope<'s, '_>, key: v8::Local<'s, v8::Name>, value: v8::Local<'s, v8::Value>,
    args: v8::PropertyCallbackArguments<'s>, _retval: v8::ReturnValue<()>,
) {
    if args.holder().delete(scope, key.into()) != Some(true) { return }
    if args.holder().define_own_property(scope, key, value, v8::PropertyAttribute::DONT_ENUM) == Some(false) &&
        args.should_throw_on_error() {
        crate::native::throw_type_error(scope, "cannot replace lazy global property");
    }
}

thread_local! {
    /// The (object, key) pairs whose resolve hook is running: lookups of the same property
    /// from inside the hook must not resolve it again (SpiderMonkey's `JSPROP_RESOLVING`).
    static RESOLVING: RefCell<Vec<(*mut JSObject, crate::jsid::jsid)>> = const { RefCell::new(Vec::new()) };
}

/// Keeps the ids of the guards alive: after a hook resolved, its id is no longer rooted while
/// the property is read back (a getter may GC), and a recreated atom would not match.
pub(crate) fn trace_resolving(visitor: &mut v8::cppgc::Visitor) {
    use crate::gc::RootKind;
    RESOLVING.with(|resolving| {
        let Ok(resolving) = resolving.try_borrow() else { return };
        for (_, id) in resolving.iter() {
            id.trace_root(visitor);
        }
    });
}

/// The class object behind an interceptor's holder. A global's holder may be its proxy (which
/// carries the box) or, for unqualified name lookups, the inner global object: that one maps
/// to its realm's global.
fn interceptor_object(scope: &mut v8::PinScope, holder: v8::Local<v8::Object>) -> Option<*mut JSObject> {
    if let Some(class_box) = class_box_of_v8(holder) {
        let cell = class_box.cell.get();
        return (!cell.is_null()).then_some(cell as *mut JSObject);
    }
    let context = holder.get_creation_context(scope)?;
    let realm = crate::realm_impl::realm_of_context(context);
    // SAFETY: realms come from `register_realm`.
    let global = unsafe { crate::realm_impl::GetRealmGlobalOrNull(realm) };
    (!global.is_null()).then_some(global)
}

/// Runs the class resolve hook for `key` on the holder; `Some(true)` when it defined the
/// property, `None` when it threw (the exception is rethrown into V8).
fn run_resolve_hook(scope: &mut v8::PinScope, data: v8::Local<v8::Value>, holder: v8::Local<v8::Object>, key: v8::Local<v8::Name>) -> Option<bool> {
    // An existing own property is found by the normal lookup.
    if holder.has_real_named_property(scope, key) == Some(true) {
        return Some(false);
    }
    let external = v8::Local::<v8::External>::try_from(data).ok()?;
    let class = external.value() as *const JSClass;
    // SAFETY: only classes with a resolve or enumerate hook get these interceptors.
    let ops = unsafe { &*(*class).cOps };
    let resolve = ops.resolve?;
    let obj = interceptor_object(scope, holder)?;
    let id = crate::jsapi_impl::key_id(scope, key.into());
    if RESOLVING.with(|resolving| resolving.borrow().contains(&(obj, id))) {
        return Some(false);
    }
    let cx = JSContext::current() as *const JSContext as *mut JSContext;
    if let Some(may_resolve) = ops.mayResolve {
        // SAFETY: SpiderMonkey's hook contract (the atom state is not used by Servo's hooks).
        if !unsafe { may_resolve(std::ptr::null(), id, obj) } {
            return Some(false);
        }
    }
    crate::rooted!(in(cx) let object = obj);
    crate::rooted!(in(cx) let rooted_id = id);
    let mut resolved = false;
    RESOLVING.with(|resolving| resolving.borrow_mut().push((obj, id)));
    // SAFETY: SpiderMonkey's resolve contract (rooted arguments).
    let ok = unsafe { resolve(cx, object.handle().into_handle(), rooted_id.handle().into_handle(), &mut resolved) };
    RESOLVING.with(|resolving| resolving.borrow_mut().pop());
    if ok && resolved {
        // The caller reads the property back; lookups of it from there must not resolve it
        // again (a hook may report success without defining anything, as Servo's does for
        // interfaces disabled by preferences).
        RESOLVING.with(|resolving| resolving.borrow_mut().push((obj, id)));
    }
    if !ok {
        crate::native::rethrow_pending(scope, JSContext::current());
        return None;
    }
    Some(resolved)
}

fn resolve_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>,
    mut retval: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let holder = args.holder();
    match run_resolve_hook(scope, args.data(), holder, key) {
        None => v8::Intercepted::kYes,
        Some(false) => v8::Intercepted::kNo,
        Some(true) => match finish_resolved(|| resolved_value(scope, holder, key, holder)) {
            Some(value) => {
                retval.set(value);
                v8::Intercepted::kYes
            },
            None => v8::Intercepted::kNo,
        },
    }
}

/// Runs the read-back of a resolved property, then drops the guard `run_resolve_hook` left.
fn finish_resolved<R>(read: impl FnOnce() -> R) -> R {
    let result = read();
    RESOLVING.with(|resolving| resolving.borrow_mut().pop());
    result
}

/// A field of a resolved property's own descriptor.
fn descriptor_field<'s>(scope: &mut v8::PinScope<'s, '_>, descriptor: v8::Local<'s, v8::Object>, name: &str) -> Option<v8::Local<'s, v8::Value>> {
    let key = v8::String::new(scope, name)?;
    descriptor.get(scope, key.into())
}

/// The value of the property the resolve hook just defined on the holder. Only the own
/// property is read: V8's "real named property" lookups walk the prototype chain, which can
/// reach named-property proxies (a Window's) that must not run here.
fn resolved_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    holder: v8::Local<'s, v8::Object>,
    key: v8::Local<'s, v8::Name>,
    receiver: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Value>> {
    let descriptor = v8::Local::<v8::Object>::try_from(holder.get_own_property_descriptor(scope, key)?).ok()?;
    let getter = descriptor_field(scope, descriptor, "get")?;
    if let Ok(getter) = v8::Local::<v8::Function>::try_from(getter) {
        return getter.call(scope, receiver.into(), &[]);
    }
    descriptor_field(scope, descriptor, "value")
}

/// The V8 attributes of the property the resolve hook just defined (own property only).
fn resolved_attributes(scope: &mut v8::PinScope, holder: v8::Local<v8::Object>, key: v8::Local<v8::Name>) -> Option<v8::PropertyAttribute> {
    let descriptor = v8::Local::<v8::Object>::try_from(holder.get_own_property_descriptor(scope, key)?).ok()?;
    let mut attributes = v8::PropertyAttribute::NONE;
    let writable = descriptor_field(scope, descriptor, "writable")?;
    if !writable.is_undefined() && !writable.boolean_value(scope) {
        attributes = attributes | v8::PropertyAttribute::READ_ONLY;
    }
    if !descriptor_field(scope, descriptor, "enumerable")?.boolean_value(scope) {
        attributes = attributes | v8::PropertyAttribute::DONT_ENUM;
    }
    if !descriptor_field(scope, descriptor, "configurable")?.boolean_value(scope) {
        attributes = attributes | v8::PropertyAttribute::DONT_DELETE;
    }
    Some(attributes)
}

fn resolve_query<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>,
    mut retval: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    let holder = args.holder();
    match run_resolve_hook(scope, args.data(), holder, key) {
        None => v8::Intercepted::kYes,
        Some(false) => v8::Intercepted::kNo,
        Some(true) => match finish_resolved(|| resolved_attributes(scope, holder, key)) {
            Some(attributes) => {
                retval.set_int32(attributes.as_u32() as i32);
                v8::Intercepted::kYes
            },
            None => v8::Intercepted::kNo,
        },
    }
}

/// Lists the lazy properties through the class `newEnumerate` hook.
fn resolve_enumerator<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::PropertyCallbackArguments<'s>, mut retval: v8::ReturnValue<v8::Array>) {
    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    let class = external.value() as *const JSClass;
    // SAFETY: as for `run_resolve_hook`.
    let ops = unsafe { &*(*class).cOps };
    let Some(enumerate) = ops.newEnumerate else { return };
    let Some(obj) = interceptor_object(scope, args.holder()) else { return };
    let cx = JSContext::current() as *const JSContext as *mut JSContext;
    crate::rooted!(in(cx) let object = obj);
    // SAFETY: `cx` is this thread's live context.
    let mut ids = unsafe { crate::rust::IdVector::new(cx) };
    // SAFETY: SpiderMonkey's enumerate contract.
    if !unsafe { enumerate(cx, object.handle().into_handle(), ids.handle_mut(), false) } {
        crate::native::rethrow_pending(scope, JSContext::current());
        return;
    }
    let keys: Vec<v8::Local<v8::Value>> = ids.iter().filter_map(|id| crate::jsapi_impl::id_key(scope, *id).map(Into::into)).collect();
    let array = v8::Array::new_with_elements(scope, &keys);
    retval.set(array);
}

fn is_callable_class(class: *const JSClass) -> bool {
    // SAFETY: classes are static.
    unsafe { (*class).cOps.as_ref() }.is_some_and(|ops| ops.call.is_some() || ops.construct.is_some())
}

fn callable_class_key<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Private> {
    crate::jsapi_impl::private_key(scope, crate::jsapi_impl::PrivateKey::CallableClassBox)
}

/// An object of a class with `call`/`construct` hooks (Servo's interface objects): a real V8
/// function, so constructing it through `super()` or `Reflect.construct` passes the actual
/// `new.target` (an API object's call-as-function handler does not get it) and `typeof` is
/// "function". A private API-object holder lets V8/cppgc trace the function's box;
/// unreachable functions and their native state are collected together.
fn new_callable_class_object(cx: &JSContext, class: *const JSClass, proto: *mut JSObject, default_proto: bool) -> *mut JSObject {
    cx.catching(|scope| {
        let template = cx.class_templates.template(scope, &PLAIN_OBJECT_CLASS);
        let data = template.new_instance(scope)?;
        let class_box = make_class_box(scope, data, class);
        let function = v8::Function::builder(callable_class_callback).data(data.into()).build(scope)?;
        if !proto.is_null() {
            // SAFETY: JSAPI callers pass live (rooted) prototypes.
            let proto = unsafe { crate::cell::cell_value(scope, proto as *mut c_void) };
            function.set_prototype(scope, proto)?;
        } else if !default_proto {
            let null = v8::null(scope).into();
            function.set_prototype(scope, null)?;
        }
        let key = callable_class_key(scope);
        function.set_private(scope, key, data.into())?;
        let cell = crate::cell::new_cell_with_class(scope, function.into(), Some(Member::new(&class_box)));
        // SAFETY: just created; the function's holder traces the box.
        unsafe { class_box.as_ref() }.cell.set(cell);
        Some(cell as *mut JSObject)
    })
    .unwrap_or(std::ptr::null_mut())
}

/// The box of a function made by `new_callable_class_object`.
pub(crate) fn callable_class_box_of<'b>(scope: &mut v8::PinScope, function: v8::Local<v8::Object>) -> Option<&'b ClassBox> {
    let key = callable_class_key(scope);
    let holder = v8::Local::<v8::Object>::try_from(function.get_private(scope, key)?).ok()?;
    class_box_of_v8(holder)
}

fn callable_class_callback(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue) {
    let Ok(holder) = v8::Local::<v8::Object>::try_from(args.data()) else { return };
    let Some(class_box) = class_box_of_v8(holder) else { return };
    // SAFETY: only callable classes get this callback.
    let ops = unsafe { &*(*class_box.class).cOps };
    let constructing = !args.new_target().is_undefined();
    let native = if constructing { ops.construct.or(ops.call) } else { ops.call };
    let Some(native) = native else {
        crate::native::throw_type_error(scope, "object is not callable");
        return;
    };
    // The box's cell is the function's (alive while it is called).
    let callee = crate::jsval::ObjectValue(class_box.cell.get() as *mut JSObject);
    crate::native::call_native(scope, &args, &mut retval, Some(native), callee, constructing);
}

/// A callable class object was called (or constructed): run its `call`/`construct` hook.
fn call_class_hook(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, mut retval: v8::ReturnValue) {
    let Ok(external) = v8::Local::<v8::External>::try_from(args.data()) else { return };
    let class = external.value() as *const JSClass;
    // SAFETY: classes are static, and only classes with a hook get this handler.
    let ops = unsafe { &*(*class).cOps };
    let constructing = !args.new_target().is_undefined();
    let native = if constructing { ops.construct.or(ops.call) } else { ops.call };
    let Some(native) = native else {
        crate::native::throw_type_error(scope, "object is not callable");
        return;
    };
    // V8 passes the called object as `This()` to call-as-function handlers (measured: also for
    // `o.f()` and `new obj()`), so it is the callee; the call's own receiver is not available.
    // Servo's callable classes are interface objects, called as constructors or plain
    // functions, where the receiver does not matter.
    let callee = crate::jsval::from_v8(scope, args.this().into());
    crate::native::call_native(scope, &args, &mut retval, Some(native), callee, constructing);
}

/// Allocates a class-based object with `proto` (or the realm's `Object.prototype` when
/// `default_proto` and `proto` is null). Returns its cell, or null with a pending exception.
pub(crate) fn new_object(cx: &JSContext, class: *const JSClass, proto: *mut JSObject, default_proto: bool) -> *mut JSObject {
    if is_callable_class(class) {
        return new_callable_class_object(cx, class, proto, default_proto);
    }
    cx.catching(|scope| {
        let template = cx.class_templates.template(scope, class);
        let object = if !default_proto && !unsafe { (*class).name }.is_null() &&
            unsafe { CStr::from_ptr((*class).name) }.to_bytes() == b"WindowPrototype" {
            let realm = crate::realm_impl::realm_data(cx.current_realm.get())?;
            let global = realm.global.get();
            if class_box(global).is_some_and(|global| is_window_global_class(global.class)) {
                let value = unsafe { crate::cell::cell_value(scope, global as *mut c_void) };
                let global = v8::Local::<v8::Object>::try_from(value).ok()?;
                let prototype = global.get_prototype(scope)?;
                let prototype = v8::Local::<v8::Object>::try_from(prototype).ok()?;
                if prototype.internal_field_count() == 1 && class_box_of_v8(prototype).is_none() {
                    prototype
                } else {
                    template.new_instance(scope)?
                }
            } else {
                template.new_instance(scope)?
            }
        } else {
            template.new_instance(scope)?
        };
        if !proto.is_null() {
            // SAFETY: JSAPI callers pass live (rooted) prototypes.
            let proto = unsafe { crate::cell::cell_value(scope, proto as *mut c_void) };
            object.set_prototype(scope, proto)?;
        } else if !default_proto {
            let null = v8::null(scope).into();
            object.set_prototype(scope, null)?;
        }
        Some(attach_class_box(scope, object, class))
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Gives `object` (from a class template) its class box and cell; returns the cell.
pub(crate) fn attach_class_box(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, class: *const JSClass) -> *mut JSObject {
    let class_box = make_class_box(scope, object, class);
    let cell = crate::cell::new_cell_with_class(scope, object.into(), Some(Member::new(&class_box)));
    // SAFETY: the box was just created and is alive (wrapped).
    unsafe { class_box.as_ref() }.cell.set(cell);
    cell as *mut JSObject
}

/// Moves the class box of the global `cell` onto a new hidden object, which becomes the cell's
/// value: another realm took over the global proxy the cell was on (see
/// `realm_impl::RovesNewWindowGlobalReusingProxy`). `context` is the global's own context. Native access through the cell keeps its
/// box and slots; script cannot reach the hidden object.
pub(crate) fn rehome_global_cell(scope: &mut v8::PinScope, cell: *mut JSObject, context: v8::Local<v8::Context>) -> Option<()> {
    // SAFETY: a live (rooted) global cell.
    let member = unsafe { crate::cell::cell_class_member(cell as *mut c_void) }?;
    let cx = JSContext::current();
    let template = cx.class_templates.template(scope, &PLAIN_OBJECT_CLASS);
    // Created in the global's own context: objects map to their realm through their creation
    // context, and the cell must still resolve to its realm.
    let scope = &mut v8::ContextScope::new(scope, context);
    let hidden = template.new_instance(scope)?;
    // The field holds the box's cppgc object pointer, as `box_to_raw` stores it.
    let raw = v8::cppgc::GetRustObj::<ClassBox>::get_rust_obj(member) as *mut c_void;
    hidden.set_aligned_pointer_in_internal_field(0, raw, CLASS_BOX_TAG);
    // SAFETY: as in `make_class_box`; the cell keeps the hidden object alive.
    unsafe { v8::Object::wrap::<CLASS_WRAP_TAG, ClassBox>(scope, hidden, member) };
    // SAFETY: a live cell, its value not borrowed.
    unsafe { crate::cell::set_cell_value(scope, cell as *mut c_void, hidden.into()) };
    Some(())
}

/// Creates `object`'s class box (stored in its internal field, kept alive by the object).
pub(crate) fn make_class_box(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, class: *const JSClass) -> UnsafePtr<ClassBox> {
    let class_box = new_class_box(scope, class);
    let raw = box_to_raw(&class_box);
    object.set_aligned_pointer_in_internal_field(0, raw, CLASS_BOX_TAG);
    // SAFETY: the object is an API object from a template with one internal field; the box
    // was just allocated, and no GC runs before the wrap makes the object keep it alive.
    unsafe { v8::Object::wrap::<CLASS_WRAP_TAG, ClassBox>(scope, object, &class_box) };
    class_box
}

/// A new class box. The caller attaches it to an object or a cell before the next GC.
pub(crate) fn new_class_box(scope: &mut v8::PinScope, class: *const JSClass) -> UnsafePtr<ClassBox> {
    // SAFETY: classes are static.
    let slots = reserved_slot_count(unsafe { &*class });
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: the caller stores the box (in an object's internal field or a cell) at once.
    unsafe {
        v8::cppgc::make_garbage_collected(
            heap,
            ClassBox {
                class,
                slots: RefCell::new(smallvec::smallvec![UndefinedValue(); slots]),
                cell: StdCell::new(std::ptr::null_mut()),
                proxy: StdCell::new(std::ptr::null()),
                window_wrapper: StdCell::new(false),
                window_identity: StdCell::new(None),
                private: StdCell::new(UndefinedValue()),
                lazy_proto: StdCell::new(false),
                global_trace: StdCell::new(None),
                immutable_prototype: RefCell::new(None),
            },
        )
    }
}

/// `JS::GetClass` / mozjs's `get_object_class`: the class of a class-based object, or the
/// shared class of ordinary objects.
pub(crate) fn object_class(obj: *mut JSObject) -> *const JSClass {
    match class_box(obj) {
        Some(class_box) => class_box.class,
        None => &PLAIN_OBJECT_CLASS,
    }
}

/// The class reported for ordinary (non class-based) objects.
pub(crate) static PLAIN_OBJECT_CLASS: JSClass = JSClass {
    name: c"Object".as_ptr(),
    flags: 0,
    cOps: std::ptr::null(),
    spec: std::ptr::null(),
    ext: std::ptr::null(),
    oOps: std::ptr::null(),
};
