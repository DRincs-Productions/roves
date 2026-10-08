/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rooting: `Rooted`/`rooted!`, `Handle`, `MutableHandle` and `Heap`.
//!
//! As in SpiderMonkey, a rooted location registers itself on this thread's root stack, and
//! the GC reads every registered location's *current* value when it traces the runtime's root
//! set. Writes through `as_ptr()` or a `MutableHandle` therefore need no bookkeeping. Marking
//! runs in one atomic pause (see `roves_v8::initialize_engine`), so the scan sees every root.

use std::cell::RefCell;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;

use v8::cppgc::Visitor;

use crate::cell::trace_cell;
use crate::jsapi::{BigInt, JSFunction, JSObject, JSScript, JSString, Symbol};
use crate::jsval::{JSVal, UndefinedValue};

/// A value that can live in a rooted location: it knows how to report its GC things.
pub trait RootKind: 'static {
    fn trace_root(&self, visitor: &mut Visitor);
}

/// The initial ("empty") value of a rooted location.
pub trait GCMethods {
    /// # Safety
    /// The value must be stored in a rooted or traced location before the next GC.
    unsafe fn initial() -> Self;
}

macro_rules! gc_thing_pointer {
    ($($ty:ty),*) => {
        $(
            impl RootKind for *mut $ty {
                fn trace_root(&self, visitor: &mut Visitor) {
                    trace_cell(*self as *mut c_void, visitor);
                }
            }

            impl GCMethods for *mut $ty {
                unsafe fn initial() -> Self {
                    std::ptr::null_mut()
                }
            }
        )*
    };
}

gc_thing_pointer!(JSObject, JSString, JSFunction, Symbol, BigInt, JSScript);

impl RootKind for JSVal {
    fn trace_root(&self, visitor: &mut Visitor) {
        if self.is_gcthing() {
            trace_cell(self.to_gcthing(), visitor);
        }
    }
}

impl GCMethods for JSVal {
    unsafe fn initial() -> Self {
        UndefinedValue()
    }
}

impl<T: RootKind> RootKind for Option<T> {
    fn trace_root(&self, visitor: &mut Visitor) {
        if let Some(value) = self {
            value.trace_root(visitor);
        }
    }
}

impl<T: RootKind> RootKind for Vec<T> {
    fn trace_root(&self, visitor: &mut Visitor) {
        for value in self {
            value.trace_root(visitor);
        }
    }
}

impl<T: RootKind, const N: usize> RootKind for [T; N] {
    fn trace_root(&self, visitor: &mut Visitor) {
        for value in self {
            value.trace_root(visitor);
        }
    }
}

struct RootEntry {
    location: *const c_void,
    trace: unsafe fn(*const c_void, &mut Visitor),
}

thread_local! {
    static ROOT_STACK: RefCell<Vec<RootEntry>> = const { RefCell::new(Vec::new()) };
}

unsafe fn trace_location<T: RootKind>(location: *const c_void, visitor: &mut Visitor) {
    // SAFETY: entries are removed before their location is dropped (RootedGuard::drop).
    unsafe { &*(location as *const T) }.trace_root(visitor);
}

/// Traces every rooted location of this thread (called from the runtime's root set).
pub(crate) fn trace_roots(visitor: &mut Visitor) {
    ROOT_STACK.with(|stack| {
        for entry in stack.borrow().iter() {
            // SAFETY: see `trace_location`.
            unsafe { (entry.trace)(entry.location, visitor) };
        }
    });
}

fn register_root<T: RootKind>(location: *const T) {
    ROOT_STACK.with(|stack| {
        stack.borrow_mut().push(RootEntry { location: location as *const c_void, trace: trace_location::<T> })
    });
}

fn unregister_root(location: *const c_void) {
    ROOT_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        // Roots are almost always released in LIFO order.
        let index = stack
            .iter()
            .rposition(|entry| entry.location == location)
            .expect("an unregistered root location");
        stack.remove(index);
    });
}

/// A rooted location's storage (`JS::Rooted<T>`).
pub struct Rooted<T> {
    pub data: T,
}

/// A rooted location registered on the root stack until dropped (`rooted!`).
pub struct RootedGuard<'a, T: 'a + RootKind> {
    root: *mut Rooted<T>,
    anchor: PhantomData<&'a mut Rooted<T>>,
}

impl<'a, T: 'a + RootKind> RootedGuard<'a, T> {
    pub fn new(_cx: *mut crate::jsapi::JSContext, root: &'a mut MaybeUninit<Rooted<T>>, initial: T) -> Self {
        let root: *mut Rooted<T> = root.write(Rooted { data: initial });
        // SAFETY: the location is pinned in the caller's `MaybeUninit` for `'a`.
        register_root(unsafe { &raw const (*root).data });
        RootedGuard { root, anchor: PhantomData }
    }

    pub fn handle(&self) -> Handle<'_, T> {
        // SAFETY: a root is a marked location.
        unsafe { Handle::from_marked_location(self.as_ptr()) }
    }

    pub fn handle_mut(&mut self) -> MutableHandle<'_, T> {
        // SAFETY: a root is a marked location.
        unsafe { MutableHandle::from_marked_location(self.as_ptr()) }
    }

    pub fn as_ptr(&self) -> *mut T {
        // SAFETY: `root` points into the caller's storage for `'a`.
        unsafe { &raw mut (*self.root).data }
    }

    pub fn get(&self) -> T
    where
        T: Copy,
    {
        **self
    }

    pub fn set(&mut self, value: T) {
        // SAFETY: as for `as_ptr`.
        unsafe { *self.as_ptr() = value };
    }
}

impl<'a, T: 'a + RootKind> Deref for RootedGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: as for `as_ptr`.
        unsafe { &*self.as_ptr() }
    }
}

impl<'a, T: 'a + RootKind> Drop for RootedGuard<'a, T> {
    fn drop(&mut self) {
        unregister_root(self.as_ptr() as *const c_void);
        // SAFETY: the value was initialized in `new`, and the location is no longer rooted.
        unsafe { std::ptr::drop_in_place(self.as_ptr()) };
    }
}

/// A read-only reference to a rooted (or otherwise traced) location.
pub struct Handle<'a, T: 'a> {
    ptr: &'a T,
}

impl<T> Clone for Handle<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<'_, T> {}

impl<'a, T> Handle<'a, T> {
    /// # Safety
    /// `ptr` must be a rooted or traced location that outlives `'a`.
    pub unsafe fn from_marked_location(ptr: *const T) -> Self {
        // SAFETY: forwarded to the caller.
        Handle { ptr: unsafe { &*ptr } }
    }

    pub fn get(&self) -> T
    where
        T: Copy,
    {
        *self.ptr
    }

    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }
}

impl<T> Deref for Handle<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.ptr
    }
}

/// A writable reference to a rooted (or otherwise traced) location.
pub struct MutableHandle<'a, T: 'a> {
    ptr: *mut T,
    anchor: PhantomData<&'a mut T>,
}

impl<'a, T> MutableHandle<'a, T> {
    /// # Safety
    /// `ptr` must be a rooted or traced location that outlives `'a`.
    pub unsafe fn from_marked_location(ptr: *mut T) -> Self {
        MutableHandle { ptr, anchor: PhantomData }
    }

    pub fn get(&self) -> T
    where
        T: Copy,
    {
        // SAFETY: the location outlives `'a`.
        unsafe { *self.ptr }
    }

    pub fn set(&mut self, value: T) {
        // SAFETY: the location outlives `'a`.
        unsafe { *self.ptr = value };
    }

    pub fn handle(&self) -> Handle<'_, T> {
        // SAFETY: the location is marked.
        unsafe { Handle::from_marked_location(self.ptr) }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }
}

impl<T> Deref for MutableHandle<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the location outlives `'a`.
        unsafe { &*self.ptr }
    }
}

pub type HandleObject<'a> = Handle<'a, *mut JSObject>;
pub type HandleValue<'a> = Handle<'a, JSVal>;
pub type HandleString<'a> = Handle<'a, *mut JSString>;
pub type HandleFunction<'a> = Handle<'a, *mut JSFunction>;
pub type MutableHandleObject<'a> = MutableHandle<'a, *mut JSObject>;
pub type MutableHandleValue<'a> = MutableHandle<'a, JSVal>;
pub type MutableHandleString<'a> = MutableHandle<'a, *mut JSString>;

/// A GC-thing location inside a traced native object (`JS::Heap<T>`). Its owner reports it
/// from its trace method (`Heap::trace`), so the referenced thing lives as long as the owner.
pub struct Heap<T> {
    value: std::cell::UnsafeCell<T>,
}

impl<T: GCMethods> Default for Heap<T> {
    fn default() -> Self {
        // SAFETY: the owner traces the location.
        Heap { value: std::cell::UnsafeCell::new(unsafe { T::initial() }) }
    }
}

impl<T: Copy + GCMethods> Heap<T> {
    pub fn boxed(value: T) -> Box<Heap<T>> {
        let heap = Box::<Heap<T>>::default();
        heap.set(value);
        heap
    }

    pub fn set(&self, value: T) {
        // SAFETY: single-threaded; no reference into the cell escapes.
        unsafe { *self.value.get() = value };
    }

    pub fn get(&self) -> T {
        // SAFETY: as for `set`.
        unsafe { *self.value.get() }
    }

    pub fn get_unsafe(&self) -> *mut T {
        self.value.get()
    }

    pub fn handle(&self) -> Handle<'_, T> {
        // SAFETY: a traced heap location.
        unsafe { Handle::from_marked_location(self.value.get()) }
    }
}

impl<T: RootKind> Heap<T> {
    /// Reports the referenced GC thing, from the owner's trace method.
    pub fn trace(&self, visitor: &mut Visitor) {
        // SAFETY: tracing only reads the location, on the mutator thread.
        unsafe { &*self.value.get() }.trace_root(visitor);
    }
}
