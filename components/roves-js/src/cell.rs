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

use std::ffi::c_void;

use v8::TracedReference;
use v8::cppgc::{GarbageCollected, Member, UnsafePtr, Visitor};

pub(crate) struct Cell {
    value: TracedReference<v8::Value>,
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
pub(crate) fn new_cell(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> *mut c_void {
    let reference = TracedReference::new(scope, value);
    let heap = scope.get_cpp_heap().expect("roves-js isolates carry a cppgc heap");
    // SAFETY: the returned pointer is handed straight to the caller, who roots or stores it
    // (the documented contract of an unrooted GC-thing pointer).
    let cell = unsafe { v8::cppgc::make_garbage_collected(heap, Cell { value: reference }) };
    to_raw(cell)
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
