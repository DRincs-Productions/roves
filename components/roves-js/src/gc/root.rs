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

    /// SpiderMonkey's post-write barrier for a heap location. V8's cppgc heap is not
    /// generational for cells, so there is nothing to record.
    ///
    /// # Safety
    /// `_location` must be a valid heap location.
    unsafe fn post_barrier(_location: *mut Self, _prev: Self, _next: Self)
    where
        Self: Sized,
    {
    }
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

impl RootKind for crate::jsid::jsid {
    fn trace_root(&self, visitor: &mut Visitor) {
        if let Some(pointer) = self.gcthing() {
            trace_cell(pointer, visitor);
        }
    }
}

impl GCMethods for crate::jsid::jsid {
    unsafe fn initial() -> Self {
        crate::jsid::VoidId()
    }
}

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

impl RootKind for crate::jsapi::PropertyDescriptor {
    fn trace_root(&self, visitor: &mut Visitor) {
        self.value_.trace_root(visitor);
        self.getter_.trace_root(visitor);
        self.setter_.trace_root(visitor);
    }
}

impl GCMethods for crate::jsapi::PropertyDescriptor {
    unsafe fn initial() -> Self {
        Self::default()
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

/// Registers a custom-traced root location (`CustomAutoRooter`).
pub(crate) fn register_custom_root(location: *const c_void, trace: unsafe fn(*const c_void, &mut Visitor)) {
    ROOT_STACK.with(|stack| stack.borrow_mut().push(RootEntry { location, trace }));
}

pub(crate) fn unregister_custom_root(location: *const c_void) {
    unregister_root(location);
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

    /// # Safety
    /// No GC may run while the reference is alive (the value is not re-read after a GC).
    pub unsafe fn as_mut(&mut self) -> &mut T {
        // SAFETY: as for `as_ptr`.
        unsafe { &mut *self.as_ptr() }
    }
}

impl<'a, T: 'a + RootKind> RootedGuard<'a, Vec<T>>
where
    Vec<T>: RootKind,
{
    pub fn set_index(&mut self, index: usize, value: T) {
        // SAFETY: no GC runs during the store.
        unsafe { self.as_mut()[index] = value };
    }

    pub fn handle_at(&'_ self, index: usize) -> Handle<'_, T> {
        assert!(index < self.len());
        // SAFETY: the elements of a rooted vector are traced.
        unsafe { Handle::from_marked_location(self.deref().as_ptr().add(index)) }
    }

    pub fn handle_mut_at(&'_ mut self, index: usize) -> MutableHandle<'_, T> {
        assert!(index < self.len());
        // SAFETY: as above.
        unsafe { MutableHandle::from_marked_location(self.as_mut().as_mut_ptr().add(index)) }
    }

    pub fn take(&'_ mut self) -> Vec<T> {
        // SAFETY: no GC runs during the swap.
        std::mem::take(unsafe { self.as_mut() })
    }
}

impl<'a, T: 'a + RootKind> Deref for RootedGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: as for `as_ptr`.
        unsafe { &*self.as_ptr() }
    }
}

impl<'a, T: 'a + RootKind> std::ops::DerefMut for RootedGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as for `as_ptr`; the location stays rooted while borrowed.
        unsafe { &mut *self.as_ptr() }
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

    /// The referenced value, borrowed while no GC can run (`_no_gc`).
    pub fn as_ref<'s: 'r, 'cx: 'r, 'r>(&'s self, _no_gc: &'cx crate::context::NoGC) -> &'r T
    where
        'a: 's,
    {
        self.ptr
    }

    /// From a raw (`jsapi`) handle.
    ///
    /// # Safety
    /// The raw handle must point at a marked location that outlives `'a`.
    pub unsafe fn from_raw(handle: crate::jsapi::Handle<T>) -> Self {
        // SAFETY: forwarded to the caller.
        unsafe { Handle::from_marked_location(handle.ptr) }
    }

    /// The raw (`jsapi`) handle.
    pub fn into_handle(self) -> crate::jsapi::Handle<T> {
        crate::jsapi::Handle { _phantom_0: PhantomData, ptr: self.ptr }
    }
}

impl<'a, T> From<Handle<'a, T>> for crate::jsapi::Handle<T> {
    fn from(handle: Handle<'a, T>) -> Self {
        handle.into_handle()
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

    /// From a raw (`jsapi`) mutable handle.
    ///
    /// # Safety
    /// The raw handle must point at a marked location that outlives `'a`.
    pub unsafe fn from_raw(handle: crate::jsapi::MutableHandle<T>) -> Self {
        // SAFETY: forwarded to the caller.
        unsafe { MutableHandle::from_marked_location(handle.ptr) }
    }

    /// The raw (`jsapi`) mutable handle.
    pub fn into_handle(self) -> crate::jsapi::MutableHandle<T> {
        crate::jsapi::MutableHandle { _phantom_0: PhantomData, ptr: self.ptr }
    }

    /// A shorter-lived mutable handle to the same location.
    pub fn reborrow<'b>(&'b mut self) -> MutableHandle<'b, T> {
        MutableHandle { ptr: self.ptr, anchor: PhantomData }
    }
}

impl<'a, T> From<MutableHandle<'a, T>> for crate::jsapi::MutableHandle<T> {
    fn from(handle: MutableHandle<'a, T>) -> Self {
        handle.into_handle()
    }
}

impl<T> Deref for MutableHandle<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the location outlives `'a`.
        unsafe { &*self.ptr }
    }
}

impl HandleValue<'static> {
    pub fn null() -> Self {
        // SAFETY: a static, immutable location.
        unsafe { Handle::from_marked_location(&NULL_VALUE) }
    }

    pub fn undefined() -> Self {
        // SAFETY: as above.
        unsafe { Handle::from_marked_location(&UNDEFINED_VALUE) }
    }
}

impl HandleObject<'static> {
    pub fn null() -> Self {
        // SAFETY: as above.
        unsafe { Handle::from_marked_location(&NULL_OBJECT.0) }
    }
}

struct SyncObject(*mut JSObject);
// SAFETY: a null pointer, never written.
unsafe impl Sync for SyncObject {}

static NULL_VALUE: JSVal = crate::jsval::NullValue();
static UNDEFINED_VALUE: JSVal = UndefinedValue();
static NULL_OBJECT: SyncObject = SyncObject(std::ptr::null_mut());

pub type HandleObject<'a> = Handle<'a, *mut JSObject>;
pub type HandleValue<'a> = Handle<'a, JSVal>;
pub type HandleString<'a> = Handle<'a, *mut JSString>;
pub type HandleFunction<'a> = Handle<'a, *mut JSFunction>;
pub type MutableHandleObject<'a> = MutableHandle<'a, *mut JSObject>;
pub type MutableHandleValue<'a> = MutableHandle<'a, JSVal>;
pub type MutableHandleString<'a> = MutableHandle<'a, *mut JSString>;
pub type HandleId<'a> = Handle<'a, crate::jsid::jsid>;
pub type HandleScript<'a> = Handle<'a, *mut crate::jsapi::JSScript>;
pub type HandleSymbol<'a> = Handle<'a, *mut crate::jsapi::Symbol>;
pub type MutableHandleFunction<'a> = MutableHandle<'a, *mut JSFunction>;
pub type MutableHandleId<'a> = MutableHandle<'a, crate::jsid::jsid>;
pub type MutableHandleScript<'a> = MutableHandle<'a, *mut crate::jsapi::JSScript>;
pub type MutableHandleSymbol<'a> = MutableHandle<'a, *mut crate::jsapi::Symbol>;

/// A GC-thing location inside a traced native object (`JS::Heap<T>`). Its owner reports it
/// from its trace method (`Traceable::trace`), so the referenced thing lives as long as the
/// owner.
pub struct Heap<T> {
    pub ptr: std::cell::UnsafeCell<T>,
}

impl<T: GCMethods> Default for Heap<T> {
    fn default() -> Self {
        // SAFETY: the owner traces the location.
        Heap { ptr: std::cell::UnsafeCell::new(unsafe { T::initial() }) }
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
        unsafe { *self.ptr.get() = value };
    }

    pub fn get(&self) -> T {
        // SAFETY: as for `set`.
        unsafe { *self.ptr.get() }
    }

    pub fn get_unsafe(&self) -> *mut T {
        self.ptr.get()
    }

    /// A raw (`jsapi`) handle to the location, as in mozjs.
    pub fn handle(&self) -> crate::jsapi::Handle<T> {
        crate::jsapi::Handle { _phantom_0: PhantomData, ptr: self.ptr.get() }
    }
}

impl<T: RootKind> Heap<T> {
    /// Reports the referenced GC thing to a cppgc visitor (roves-js's own trace methods; the
    /// `*mut JSTracer` form is `Traceable::trace`).
    pub fn trace_visitor(&self, visitor: &mut Visitor) {
        // SAFETY: tracing only reads the location, on the mutator thread.
        unsafe { &*self.ptr.get() }.trace_root(visitor);
    }
}

/// Conversion to a raw (`jsapi`) handle (mozjs_sys's `jsgc::IntoHandle`).
pub trait IntoHandle {
    type Target;
    fn into_handle(self) -> crate::jsapi::Handle<Self::Target>;
}

/// Conversion to a raw mutable handle (mozjs_sys's `jsgc::IntoMutableHandle`).
pub trait IntoMutableHandle: IntoHandle {
    fn into_handle_mut(self) -> crate::jsapi::MutableHandle<Self::Target>;
}

impl<'a, T> IntoHandle for Handle<'a, T> {
    type Target = T;
    fn into_handle(self) -> crate::jsapi::Handle<T> {
        Handle::into_handle(self)
    }
}

impl<'a, T> IntoHandle for MutableHandle<'a, T> {
    type Target = T;
    fn into_handle(self) -> crate::jsapi::Handle<T> {
        crate::jsapi::Handle { _phantom_0: PhantomData, ptr: self.as_ptr() }
    }
}

impl<'a, T> IntoMutableHandle for MutableHandle<'a, T> {
    fn into_handle_mut(self) -> crate::jsapi::MutableHandle<T> {
        MutableHandle::into_handle(self)
    }
}
