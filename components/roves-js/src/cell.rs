/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! GC-thing cells: what a `*mut JSObject` (or `*mut JSString`, symbol, BigInt) points at.
//!
//! SpiderMonkey hands out raw pointers to its GC things. V8 cannot: its JS heap moves objects
//! and only exposes handles. A cell is a small object on V8's **cppgc** heap (which does not
//! move) holding a traced reference to the JS value. A pointer to a cell is a stable,
//! pointer-sized stand-in for the SpiderMonkey GC-thing pointer.
//!
//! A cell lives while something traces it: a root (`Rooted`/`rooted!`), a `Heap<T>` inside a
//! traced object, or another cell. An unrooted cell pointer is only valid until the next GC,
//! which is the same rule SpiderMonkey has for unrooted GC-thing pointers.
//!
//! Objects and symbols have **one cell each** while the cell lives, so pointer comparison is
//! identity as in SpiderMonkey: class objects and native functions find theirs through their
//! own storage, other objects through the runtime's weak [`Interned`] table.

use std::ffi::c_void;

use v8::TracedReference;
use v8::cppgc::{GarbageCollected, Member, UnsafePtr, Visitor, WeakPersistent};

/// The cells of objects and symbols by identity hash, held weakly (a cell lives while it is
/// traced; its V8 value does not keep it alive). Owned by the runtime's raw context.
#[derive(Default)]
pub(crate) struct Interned {
    cells: std::cell::RefCell<std::collections::HashMap<i32, Vec<WeakPersistent<Cell>>>>,
    inserted: std::cell::Cell<usize>,
}

impl Interned {
    fn find(&self, scope: &mut v8::PinScope, hash: i32, value: v8::Local<v8::Value>) -> Option<*mut c_void> {
        let mut cells = self.cells.borrow_mut();
        let entries = cells.get_mut(&hash)?;
        entries.retain(|entry| entry.get().is_some());
        for entry in entries.iter() {
            let cell = entry.get()?;
            if cell.value.get(scope).is_some_and(|held| held.strict_equals(value)) {
                // SAFETY: the entry is alive (checked above) and stays so for this call.
                let pointer = unsafe { UnsafePtr::new(entry) }?;
                return Some(to_raw(pointer));
            }
        }
        None
    }

    fn insert(&self, hash: i32, cell: *mut c_void) {
        let Some(pointer) = from_raw(cell) else { return };
        let mut cells = self.cells.borrow_mut();
        cells.entry(hash).or_default().push(WeakPersistent::new(&pointer));
        // Drop the entries of collected cells now and then.
        let inserted = self.inserted.get() + 1;
        self.inserted.set(inserted);
        if inserted % 4096 == 0 {
            cells.retain(|_, entries| {
                entries.retain(|entry| entry.get().is_some());
                !entries.is_empty()
            });
        }
    }
}

fn identity_hash(value: v8::Local<v8::Value>) -> Option<i32> {
    if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
        return Some(object.get_identity_hash().get());
    }
    if let Ok(symbol) = v8::Local::<v8::Symbol>::try_from(value) {
        return Some(symbol.get_identity_hash().get());
    }
    None
}

pub(crate) struct Cell {
    value: TracedReference<v8::Value>,
    /// A string cell's characters, copied out on first request: SpiderMonkey lends raw
    /// character pointers that stay valid while the string lives, and the cell gives the same
    /// guarantee (it does not move and is freed only with the string's last reference).
    chars: std::cell::OnceCell<StringChars>,
    /// A class-based object's box (see `object`): reserved slots and hooks.
    class: Option<Member<crate::object::ClassBox>>,
    /// A `JSScript` cell's compiled script (see `script_impl`).
    script: Option<TracedReference<v8::UnboundScript>>,
    /// A module record's module (see `modules_impl`).
    module: Option<TracedReference<v8::Module>>,
}

pub(crate) enum StringChars {
    Latin1(Vec<u8>),
    TwoByte(Vec<u16>),
}

#[cfg(test)]
thread_local! {
    /// Cells finalized on this thread (tests observe collection through it).
    pub(crate) static FINALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Drop for Cell {
    fn drop(&mut self) {
        FINALIZED.with(|count| count.set(count.get() + 1));
    }
}

// SAFETY: `trace` reports the cell's only reference.
unsafe impl GarbageCollected for Cell {
    fn trace(&self, visitor: &mut Visitor) {
        visitor.trace(&self.value);
        if let Some(class) = &self.class {
            visitor.trace(class);
        }
        if let Some(script) = &self.script {
            visitor.trace(script);
        }
        if let Some(module) = &self.module {
            visitor.trace(module);
        }
    }

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesJsCell"
    }
}

// `UnsafePtr<Cell>` is a single non-null pointer (plus a zero-sized marker); cell pointers
// travel through mozjs-shaped APIs as `*mut c_void`-sized raw pointers.
const _: () = assert!(std::mem::size_of::<UnsafePtr<Cell>>() == std::mem::size_of::<*mut c_void>());
const _: () = assert!(std::mem::size_of::<Option<UnsafePtr<Cell>>>() == std::mem::size_of::<*mut c_void>());

fn to_raw(cell: UnsafePtr<Cell>) -> *mut c_void {
    // SAFETY: same size (asserted above); `UnsafePtr` only wraps the cell's address.
    unsafe { std::mem::transmute::<UnsafePtr<Cell>, *mut c_void>(cell) }
}

fn from_raw(pointer: *mut c_void) -> Option<UnsafePtr<Cell>> {
    // SAFETY: inverse of `to_raw`; a null pointer maps to `None` (niche of the NonNull field).
    unsafe { std::mem::transmute::<*mut c_void, Option<UnsafePtr<Cell>>>(pointer) }
}

/// Allocates a cell for `value`. The cell is unrooted: root or store it before the next GC.
/// A class-based object keeps one cell (its identity), which this returns instead.
pub(crate) fn new_cell(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> *mut c_void {
    if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
        if let Some(class_box) = crate::object::class_box_of_v8(object) {
            let cell = class_box.cell.get();
            if !cell.is_null() {
                // Scripts see a Window global as its window proxy.
                return crate::realm_impl::window_proxy_of_global(cell).unwrap_or(cell);
            }
        }
        if let Ok(proxy) = v8::Local::<v8::Proxy>::try_from(value) {
            if let Some(class_box) = crate::proxy::proxy_box_of_v8(scope, proxy) {
                let cell = class_box.cell.get();
                if !cell.is_null() {
                    return cell;
                }
            }
        }
        if object.is_function() {
            if let Some(function) = crate::jsapi_impl::native_function_of_v8(scope, object) {
                let cell = function.cell.get();
                if !cell.is_null() {
                    return cell;
                }
            }
        }
    }
    let Some(hash) = identity_hash(value) else { return new_cell_with_class(scope, value, None) };
    let Some(cx) = crate::rust::Runtime::get() else { return new_cell_with_class(scope, value, None) };
    // SAFETY: the runtime's raw context outlives its scopes.
    let interned = unsafe { &cx.as_ref().interned };
    if let Some(cell) = interned.find(scope, hash, value) {
        return cell;
    }
    let cell = new_cell_with_class(scope, value, None);
    interned.insert(hash, cell);
    cell
}

/// Allocates a cell, optionally tied to a class box.
pub(crate) fn new_cell_with_class(
    scope: &mut v8::PinScope,
    value: v8::Local<v8::Value>,
    class: Option<Member<crate::object::ClassBox>>,
) -> *mut c_void {
    let reference = TracedReference::new(scope, value);
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: the returned pointer is handed straight to the caller, who roots or stores it
    // (the documented contract of an unrooted GC-thing pointer).
    let cell = unsafe {
        v8::cppgc::make_garbage_collected(heap, Cell { value: reference, chars: std::cell::OnceCell::new(), class, script: None, module: None })
    };
    to_raw(cell)
}

/// A cell for a compiled script (`*mut JSScript`); its value is the script's id.
pub(crate) fn new_script_cell(scope: &mut v8::PinScope, script: v8::Local<v8::UnboundScript>) -> *mut c_void {
    let id = v8::Integer::new(scope, script.script_id());
    let reference = TracedReference::new(scope, id.into());
    let script = TracedReference::new(scope, script);
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: as for `new_cell_with_class`.
    let cell = unsafe {
        v8::cppgc::make_garbage_collected(
            heap,
            Cell { value: reference, chars: std::cell::OnceCell::new(), class: None, script: Some(script), module: None },
        )
    };
    to_raw(cell)
}

/// The compiled script of a `JSScript` cell.
///
/// # Safety
/// `pointer` must be a live script cell.
pub(crate) unsafe fn cell_script<'s>(scope: &mut v8::PinScope<'s, '_>, pointer: *mut c_void) -> Option<v8::Local<'s, v8::UnboundScript>> {
    let cell = from_raw(pointer)?;
    // SAFETY: the caller guarantees the cell is alive.
    unsafe { cell.as_ref() }.script.as_ref()?.get(scope)
}

/// The JS value a live cell holds.
///
/// # Safety
/// `pointer` must come from [`new_cell`] and the cell must not have been collected.
pub(crate) unsafe fn cell_value<'s>(scope: &mut v8::PinScope<'s, '_>, pointer: *mut c_void) -> v8::Local<'s, v8::Value> {
    let cell = from_raw(pointer).expect("a non-null GC-thing pointer");
    // SAFETY: the caller guarantees the cell is alive.
    let cell = unsafe { cell.as_ref() };
    cell.value.get(scope).expect("a live cell holds its value")
}

/// Marks the cell `pointer` (if any) as reachable, from a trace method.
pub(crate) fn trace_cell(pointer: *mut c_void, visitor: &mut Visitor) {
    if let Some(cell) = from_raw(pointer) {
        visitor.trace(&Member::new(&cell));
    }
}

/// A string cell's characters (Latin-1 when every code unit fits, else UTF-16), valid while
/// the cell lives.
///
/// # Safety
/// `pointer` must be a live string cell.
pub(crate) unsafe fn string_chars<'c>(scope: &mut v8::PinScope, pointer: *mut c_void) -> &'c StringChars {
    let cell = from_raw(pointer).expect("a non-null string pointer");
    // SAFETY: the caller guarantees the cell is alive; the returned reference is tied to it.
    let cell: &'c Cell = unsafe { &*(cell.as_ref() as *const Cell) };
    cell.chars.get_or_init(|| {
        let value = cell.value.get(scope).expect("a live cell holds its value");
        let string = v8::Local::<v8::String>::try_from(value).expect("a string cell");
        let mut units = vec![0u16; string.length()];
        string.write_v2(scope, 0, &mut units, v8::WriteFlags::empty());
        if units.iter().all(|unit| *unit <= 0xFF) {
            StringChars::Latin1(units.into_iter().map(|unit| unit as u8).collect())
        } else {
            StringChars::TwoByte(units)
        }
    })
}

/// The raw class box pointer of a class-based object's cell (no V8 access: safe during GC).
pub(crate) fn cell_class_box(pointer: *mut c_void) -> Option<*mut c_void> {
    let cell = from_raw(pointer)?;
    // SAFETY: JSAPI callers pass live object pointers.
    let cell = unsafe { cell.as_ref() };
    let member = cell.class.as_ref()?;
    // SAFETY: the member is traced by the (live) cell.
    let class_box = unsafe { member.get() }?;
    Some(class_box as *const crate::object::ClassBox as *mut c_void)
}

/// A module record: a placeholder object whose cell holds the V8 module.
pub(crate) fn new_module_cell(scope: &mut v8::PinScope, module: v8::Local<v8::Module>) -> *mut c_void {
    let placeholder = v8::Object::new(scope);
    let reference = TracedReference::new(scope, placeholder.into());
    let module = TracedReference::new(scope, module);
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: as for `new_cell_with_class`.
    let cell = unsafe {
        v8::cppgc::make_garbage_collected(
            heap,
            Cell { value: reference, chars: std::cell::OnceCell::new(), class: None, script: None, module: Some(module) },
        )
    };
    to_raw(cell)
}

/// The module of a module record cell.
///
/// # Safety
/// `pointer` must be a live cell.
pub(crate) unsafe fn cell_module<'s>(scope: &mut v8::PinScope<'s, '_>, pointer: *mut c_void) -> Option<v8::Local<'s, v8::Module>> {
    let cell = from_raw(pointer)?;
    // SAFETY: the caller guarantees the cell is alive.
    unsafe { cell.as_ref() }.module.as_ref()?.get(scope)
}
