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
use std::collections::HashMap;
use std::ffi::c_void;

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
    slots: RefCell<Vec<JSVal>>,
    /// The object's own cell (set right after allocation).
    pub(crate) cell: StdCell<*mut c_void>,
    /// For a global object: its realm.
    pub(crate) realm: RefCell<Option<v8::Global<v8::Context>>>,
    /// For a proxy's target: the proxy handler (null for ordinary class objects). The box's
    /// `cell` is then the proxy's cell, and trace/finalize go to the handler's traps.
    pub(crate) proxy: StdCell<*const crate::proxy::ProxyHandler>,
    /// A proxy's private value (`GetProxyPrivate`).
    pub(crate) private: StdCell<JSVal>,
    /// Whether a proxy's prototype comes from its handler (`getPrototype` trap).
    pub(crate) lazy_proto: StdCell<bool>,
    /// For a global: its realm's `traceGlobal` hook (`RealmCreationOptions`), which traces the
    /// embedder's global state alongside the class hook.
    pub(crate) global_trace: StdCell<crate::jsapi::JSTraceOp>,
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
        let private = self.private.get();
        if private.is_gcthing() {
            trace_cell(private.to_gcthing(), visitor);
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

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesJsClassBox"
    }
}

/// Marks the extent of GC work run by roves-js (traces and finalizers).
struct GcScope;

impl GcScope {
    fn enter() -> GcScope {
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

impl ClassBox {
    pub(crate) fn reserved_slot(&self, index: u32) -> JSVal {
        self.slots.borrow().get(index as usize).copied().unwrap_or_else(UndefinedValue)
    }

    pub(crate) fn set_reserved_slot(&self, index: u32, value: JSVal) {
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
    templates: RefCell<HashMap<usize, v8::Global<v8::ObjectTemplate>>>,
}

impl ClassTemplates {
    pub(crate) fn template<'s>(&self, scope: &mut v8::PinScope<'s, '_>, class: *const JSClass) -> v8::Local<'s, v8::ObjectTemplate> {
        if let Some(template) = self.templates.borrow().get(&(class as usize)) {
            return v8::Local::new(scope, template);
        }
        let template = v8::ObjectTemplate::new(scope);
        template.set_internal_field_count(1);
        // SAFETY: classes are static.
        let ops = unsafe { (*class).cOps.as_ref() };
        if ops.is_some_and(|ops| ops.call.is_some() || ops.construct.is_some()) {
            let data = v8::External::new(scope, class as *mut c_void);
            template.set_call_as_function_handler(call_class_hook, Some(data.into()));
        }
        self.templates.borrow_mut().insert(class as usize, v8::Global::new(scope, template));
        template
    }
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
    let callee = args.this();
    crate::native::call_native(scope, &args, &mut retval, Some(native), callee.into(), constructing);
}

/// Allocates a class-based object with `proto` (or the realm's `Object.prototype` when
/// `default_proto` and `proto` is null). Returns its cell, or null with a pending exception.
pub(crate) fn new_object(cx: &JSContext, class: *const JSClass, proto: *mut JSObject, default_proto: bool) -> *mut JSObject {
    cx.catching(|scope| {
        let template = cx.class_templates.template(scope, class);
        let object = template.new_instance(scope)?;
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

/// Creates `object`'s class box (stored in its internal field, kept alive by the object).
pub(crate) fn make_class_box(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, class: *const JSClass) -> UnsafePtr<ClassBox> {
    // SAFETY: classes are static.
    let slots = reserved_slot_count(unsafe { &*class });
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: the box is attached to the object (wrap) right away, before any GC can run.
    let class_box = unsafe {
        v8::cppgc::make_garbage_collected(
            heap,
            ClassBox {
                class,
                slots: RefCell::new(vec![UndefinedValue(); slots]),
                cell: StdCell::new(std::ptr::null_mut()),
                realm: RefCell::new(None),
                proxy: StdCell::new(std::ptr::null()),
                private: StdCell::new(UndefinedValue()),
                lazy_proto: StdCell::new(false),
                global_trace: StdCell::new(None),
            },
        )
    };
    let raw = box_to_raw(&class_box);
    object.set_aligned_pointer_in_internal_field(0, raw, CLASS_BOX_TAG);
    let root = v8::cppgc::Persistent::new(&class_box);
    // SAFETY: the object is an API object from a template with one internal field.
    unsafe { v8::Object::wrap::<CLASS_WRAP_TAG, ClassBox>(scope, object, &root) };
    class_box
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
