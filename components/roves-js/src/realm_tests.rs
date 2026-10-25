//! Lifetime regressions for the weak realm registry, independent of Servo page roots.

use std::ptr;

use crate::glue::ProxyTraps;
use crate::jsapi::*;
use crate::jsval::*;
use crate::proxy::{CreateWrapperProxyHandler, GetProxyPrivate, NewWindowProxy};
use crate::realm_impl::testing::*;
use crate::rust::{JSEngineHandle, Runtime};
use crate::tests::eval;

static GLOBAL: JSClass = JSClass {
    name: c"CollectibleRealm".as_ptr(),
    flags: crate::object::JSCLASS_IS_GLOBAL |
        ((crate::object::JSCLASS_GLOBAL_SLOT_COUNT + 1) << crate::object::JSCLASS_RESERVED_SLOTS_SHIFT),
    cOps: ptr::null(), spec: ptr::null(), ext: ptr::null(), oOps: ptr::null(),
};

fn new_global(cx: *mut JSContext, principals: *mut JSPrincipals) -> *mut JSObject {
    // SAFETY: tests pass a live context and either null or freshly created principals.
    unsafe { JS_NewGlobalObject(cx, &GLOBAL, principals, OnNewGlobalHookOption::DontFireOnNewGlobalHook, ptr::null()) }
}

fn collect(runtime: &mut Runtime) {
    for _ in 0..3 {
        runtime.gc_for_testing();
        // Flush lazy V8/cppgc finalization without depending on one collection's schedule.
        unsafe { &mut *(*runtime.raw_cx()).isolate }.low_memory_notification();
    }
}

#[test]
fn unreachable_context_holder_cycle_is_collected() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let weak = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        assert!(!global.get().is_null());
        let realm = unsafe { GetObjectRealmOrNull(global.get()) };
        realm_weak(realm)
    };
    collect(&mut runtime);
    assert!(weak.get().is_none(), "the context/holder/RealmData cycle must not root itself");
    assert_eq!(live_realm_count(unsafe { &*cx }), 1, "only the initial realm survives");
}

#[test]
fn rooted_objects_and_functions_keep_their_creation_realm() {
    for code in ["({ answer: 42 })", "(() => 42)"] {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        let weak;
        {
            crate::rooted!(in(cx) let mut kept = UndefinedValue());
            {
                crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
                weak = realm_weak(unsafe { GetObjectRealmOrNull(global.get()) });
                let old = unsafe { EnterRealm(cx, global.get()) };
                kept.set(eval(&runtime, code));
                unsafe { LeaveRealm(cx, old) };
            }
            collect(&mut runtime);
            assert!(weak.get().is_some(), "a rooted value retains its creation realm");
            if code.contains("=>") {
                crate::rooted!(in(cx) let this = UndefinedValue());
                crate::rooted!(in(cx) let mut result = UndefinedValue());
                let args = HandleValueArray { length_: 0, elements_: ptr::null() };
                assert!(unsafe { Call(cx, this.handle().into(), kept.handle().into(), &args, result.handle_mut().into()) });
                assert_eq!(result.get().to_int32(), 42);
            }
        }
        collect(&mut runtime);
        assert!(weak.get().is_none(), "dropping the last value releases its realm");
    }
}

#[test]
fn current_and_saved_realms_survive_nested_entries_without_object_roots() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    let (first, first_weak, restore_initial) = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        let realm = unsafe { GetObjectRealmOrNull(global.get()) };
        (realm, realm_weak(realm), unsafe { EnterRealm(cx, global.get()) })
    };
    let (second_weak, restore_first) = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        let realm = unsafe { GetObjectRealmOrNull(global.get()) };
        (realm_weak(realm), unsafe { EnterRealm(cx, global.get()) })
    };
    collect(&mut runtime);
    // Check weak handles before restoring raw pointers: even a broken trace fails safely.
    if first_weak.get().is_none() || second_weak.get().is_none() {
        reset_realms_for_test(unsafe { &*cx }, initial);
        panic!("current and saved realms must remain alive");
    }
    assert_eq!(saved_depth(unsafe { &*cx }), 2);
    assert_eq!(restore_first, first);
    unsafe { LeaveRealm(cx, restore_first) };
    collect(&mut runtime);
    assert!(first_weak.get().is_some());
    assert!(second_weak.get().is_none(), "the departed realm has no remaining owner");
    unsafe { LeaveRealm(cx, restore_initial) };
    collect(&mut runtime);
    assert!(first_weak.get().is_none());
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
}

#[test]
fn realm_switch_restores_the_previous_realm_on_unwinding() {
    let runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
    let realm = unsafe { GetObjectRealmOrNull(global.get()) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _switch = unsafe { &*cx }.switch_realm(realm);
        assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, realm);
        panic!("exercise RAII restoration");
    }));
    assert!(result.is_err());
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, initial);
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
}

#[test]
fn queued_microtasks_keep_their_realm_until_the_queue_drains() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let weak = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        let realm = unsafe { GetObjectRealmOrNull(global.get()) };
        let old = unsafe { EnterRealm(cx, global.get()) };
        eval(&runtime, "Promise.resolve().then(() => { globalThis.completed = true }); 0");
        unsafe { LeaveRealm(cx, old) };
        realm_weak(realm)
    };
    collect(&mut runtime);
    assert!(weak.get().is_some(), "the queued reaction owns its context");
    unsafe { &mut *(*cx).isolate }.perform_microtask_checkpoint();
    collect(&mut runtime);
    assert!(weak.get().is_none(), "a drained reaction no longer roots its realm");
}

unsafe extern "C" fn native_answer(_cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
    unsafe { *vp = Int32Value(42) };
    true
}

unsafe extern "C" fn native_has_calling_principals(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
    // Return a result instead of asserting inside an extern-C callback.
    let realm = unsafe { GetCurrentRealmOrNull(cx) };
    let callee_realm = unsafe { GetObjectRealmOrNull((*vp).to_object()) };
    let has_principals = realm == callee_realm && !realm.is_null()
        && !unsafe { GetRealmPrincipals(realm) }.is_null();
    unsafe { *vp = BooleanValue(has_principals) };
    true
}

unsafe extern "C" fn native_throws_in_calling_realm(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
    unsafe { native_has_calling_principals(cx, argc, vp) };
    let correct = unsafe { (*vp).to_boolean() };
    crate::rooted!(in(cx) let error = Int32Value(if correct { 7 } else { 9 }));
    unsafe { crate::api::JS_SetPendingException(cx, error.handle(), crate::api::ExceptionStackBehavior::Capture) };
    false
}

#[test]
fn native_callbacks_use_the_executing_realm_after_promise_jobs() {
    let runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    unsafe { JS_InitDestroyPrincipalsCallback(cx, Some(destroy_principals)) };
    let principals = unsafe { CreateRustJSPrincipals(ptr::null(), ptr::null_mut()) };
    crate::rooted!(in(cx) let global = new_global(cx, principals));
    let old = unsafe { EnterRealm(cx, global.get()) };
    crate::rooted!(in(cx) let native = ObjectValue(unsafe {
        JS_NewFunction(cx, Some(native_has_calling_principals), 0, 0, c"hasCallingPrincipals".as_ptr())
    }.cast()));
    assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"hasCallingPrincipals".as_ptr(), native.handle().into()) });
    crate::rooted!(in(cx) let throwing = ObjectValue(unsafe {
        JS_NewFunction(cx, Some(native_throws_in_calling_realm), 0, 0, c"throwInCallingRealm".as_ptr())
    }.cast()));
    assert!(unsafe { JS_SetProperty(cx, global.handle().into(), c"throwInCallingRealm".as_ptr(), throwing.handle().into()) });
    eval(&runtime, "globalThis.checked = false; globalThis.errorChecked = false; (async () => { await Promise.race([Promise.resolve(1)]); checked = hasCallingPrincipals(); try { throwInCallingRealm(); } catch (e) { errorChecked = e === 7; } checked = checked && hasCallingPrincipals(); })(); 0");
    unsafe { LeaveRealm(cx, old) };
    // As in Servo's job checkpoint, no explicit EnterRealm surrounds this reaction.
    unsafe { &mut *(*cx).isolate }.perform_microtask_checkpoint();
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, initial, "native entry restores the caller's realm");
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
    let old = unsafe { EnterRealm(cx, global.get()) };
    assert!(eval(&runtime, "checked").to_boolean(), "native callback lost the page's principals");
    assert!(eval(&runtime, "errorChecked").to_boolean(), "a throwing native callback preserves the realm too");
    unsafe { LeaveRealm(cx, old) };
    crate::rooted!(in(cx) let this = UndefinedValue());
    crate::rooted!(in(cx) let mut result = UndefinedValue());
    let args = HandleValueArray { length_: 0, elements_: ptr::null() };
    assert!(unsafe { Call(cx, this.handle().into(), native.handle().into(), &args, result.handle_mut().into()) });
    assert!(result.get().to_boolean(), "direct cross-realm native entry uses the callee realm too");
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, initial);
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
}

#[test]
fn native_callbacks_restore_a_foreign_caller_after_success_and_throw() {
    let runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    unsafe { JS_InitDestroyPrincipalsCallback(cx, Some(destroy_principals)) };
    let owner_principals = unsafe { CreateRustJSPrincipals(ptr::null(), ptr::null_mut()) };
    let caller_principals = unsafe { CreateRustJSPrincipals(ptr::null(), ptr::null_mut()) };
    assert_ne!(owner_principals, caller_principals);
    crate::rooted!(in(cx) let owner = new_global(cx, owner_principals));
    crate::rooted!(in(cx) let caller = new_global(cx, caller_principals));
    let old = unsafe { EnterRealm(cx, owner.get()) };
    crate::rooted!(in(cx) let native = ObjectValue(unsafe {
        JS_NewFunction(cx, Some(native_has_calling_principals), 0, 0, c"checkOwner".as_ptr())
    }.cast()));
    crate::rooted!(in(cx) let throwing = ObjectValue(unsafe {
        JS_NewFunction(cx, Some(native_throws_in_calling_realm), 0, 0, c"throwInOwner".as_ptr())
    }.cast()));
    unsafe { LeaveRealm(cx, old) };
    let old = unsafe { EnterRealm(cx, caller.get()) };
    let caller_realm = unsafe { GetCurrentRealmOrNull(cx) };
    crate::rooted!(in(cx) let this = UndefinedValue());
    crate::rooted!(in(cx) let mut result = UndefinedValue());
    let args = HandleValueArray { length_: 0, elements_: ptr::null() };
    assert!(unsafe { Call(cx, this.handle().into(), native.handle().into(), &args, result.handle_mut().into()) });
    assert!(result.get().to_boolean(), "nonnull caller principals must not substitute for the callee's realm");
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, caller_realm);
    assert_eq!(unsafe { GetRealmPrincipals(caller_realm) }, caller_principals);
    assert!(!unsafe { Call(cx, this.handle().into(), throwing.handle().into(), &args, result.handle_mut().into()) });
    assert!(unsafe { crate::api::JS_GetPendingException(cx, result.handle_mut()) });
    assert_eq!(result.get().to_int32(), 7);
    unsafe { crate::api::JS_ClearPendingException(cx) };
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, caller_realm);
    assert_eq!(unsafe { GetRealmPrincipals(caller_realm) }, caller_principals);
    assert_eq!(saved_depth(unsafe { &*cx }), 1);
    unsafe { LeaveRealm(cx, old) };
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, initial);
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
}

thread_local! {
    static REMOTE_GETTER_REALMS: std::cell::RefCell<Vec<*mut Realm>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A remote window's `window` getter as Servo generates it: unwraps `this` to the window and
/// converts its window proxy for the current realm.
unsafe extern "C" fn remote_window_getter(cx: *mut JSContext, _argc: u32, vp: *mut JSVal) -> bool {
    REMOTE_GETTER_REALMS.with(|realms| realms.borrow_mut().push(unsafe { GetCurrentRealmOrNull(cx) }));
    let window = unsafe { UnwrapObjectDynamic((*vp.add(1)).to_object(), cx, false) };
    crate::rooted!(in(cx) let mut proxy = ObjectValue(unsafe { ToWindowProxyIfWindowSlow(window) }));
    if !unsafe { JS_WrapValue(cx, proxy.handle_mut().into()) } {
        return false;
    }
    unsafe { *vp = proxy.get() };
    true
}

/// The same-origin branch of Servo's window proxy `get`: forwards to the window's property.
unsafe extern "C" fn forward_window_get(cx: *mut JSContext, proxy: HandleObject, receiver: HandleValue, id: HandleId, vp: MutableHandleValue) -> bool {
    let mut window = UndefinedValue();
    unsafe { GetProxyPrivate(proxy.get(), &mut window) };
    crate::rooted!(in(cx) let window = window.to_object());
    unsafe { JS_ForwardGetPropertyTo(cx, window.handle().into(), id, receiver, vp) }
}

/// Servo's DissimilarOriginWindow clones its parent's origin, so the parent's window proxy
/// `get` forwards to the remote window's own getter, a native of the remote window's realm.
/// It runs there, but the proxy it returns must be the immediate JS caller's wrapper, also
/// from a promise job without a JSAPI realm and from a nested frame of a third realm.
#[test]
fn natives_of_a_remote_window_realm_return_the_callers_window_wrapper() {
    let runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    crate::rooted!(in(cx) let page = new_global(cx, ptr::null_mut()));
    crate::rooted!(in(cx) let remote = new_global(cx, ptr::null_mut()));
    let remote_realm = unsafe { GetObjectRealmOrNull(remote.get()) };
    let mut traps: ProxyTraps = unsafe { std::mem::zeroed() };
    traps.get = Some(forward_window_get);
    let handler = unsafe { CreateWrapperProxyHandler(&traps) };
    let old = unsafe { EnterRealm(cx, remote.get()) };
    crate::rooted!(in(cx) let getter = ObjectValue(unsafe {
        JS_NewFunction(cx, Some(remote_window_getter), 0, 0, c"get window".as_ptr())
    }.cast()));
    assert!(unsafe { JS_SetProperty(cx, remote.handle().into(), c"remoteGetter".as_ptr(), getter.handle().into()) });
    eval(&runtime, "Object.defineProperty(globalThis, 'window', { get: remoteGetter, configurable: true }); 0");
    crate::rooted!(in(cx) let proxy = unsafe { NewWindowProxy(cx, remote.handle().into(), handler) });
    unsafe { SetWindowProxy(cx, remote.handle().into(), proxy.handle().into()) };
    unsafe { LeaveRealm(cx, old) };

    let old = unsafe { EnterRealm(cx, page.get()) };
    crate::rooted!(in(cx) let mut held = ObjectValue(proxy.get()));
    assert!(unsafe { JS_WrapValue(cx, held.handle_mut().into()) });
    assert_ne!(held.get().to_object(), proxy.get(), "the page holds its own wrapper");
    assert!(unsafe { JS_SetProperty(cx, page.handle().into(), c"held".as_ptr(), held.handle().into()) });
    assert!(unsafe { JS_SetProperty(cx, page.handle().into(), c"remoteGetter".as_ptr(), getter.handle().into()) });
    assert!(eval(&runtime, "held.window === held").to_boolean(), "the forwarded getter returned the remote realm's wrapper");
    assert!(eval(&runtime, "remoteGetter.call(held) === held").to_boolean(), "a direct call returned the remote realm's wrapper");
    eval(&runtime, "globalThis.asyncSame = false; globalThis.asyncTrap = false; (async () => { await Promise.race([Promise.resolve(0)]); asyncSame = remoteGetter.call(held) === held; asyncTrap = held.window === held; })(); 0");
    unsafe { LeaveRealm(cx, old) };
    // As in Servo's job checkpoint, no explicit EnterRealm surrounds this reaction: the trap's
    // embedder code then runs in the stale initial realm, but the JS caller is the page.
    unsafe { &mut *(*cx).isolate }.perform_microtask_checkpoint();
    assert_eq!(unsafe { GetCurrentRealmOrNull(cx) }, initial);
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
    let old = unsafe { EnterRealm(cx, page.get()) };
    assert!(eval(&runtime, "asyncSame").to_boolean(), "a promise job got a wrapper of another realm");
    assert!(eval(&runtime, "asyncTrap").to_boolean(), "a trap in a promise job got a wrapper of another realm");
    unsafe { LeaveRealm(cx, old) };

    // Nested JS frames: the page (the entered realm) calls a function of a third realm, which
    // calls the getter. The immediate JS caller is that realm, so it gets its own wrapper.
    crate::rooted!(in(cx) let frame = new_global(cx, ptr::null_mut()));
    let old = unsafe { EnterRealm(cx, frame.get()) };
    crate::rooted!(in(cx) let mut frame_held = ObjectValue(proxy.get()));
    assert!(unsafe { JS_WrapValue(cx, frame_held.handle_mut().into()) });
    assert_ne!(frame_held.get().to_object(), held.get().to_object(), "each realm has its own wrapper");
    assert!(unsafe { JS_SetProperty(cx, frame.handle().into(), c"held".as_ptr(), frame_held.handle().into()) });
    assert!(unsafe { JS_SetProperty(cx, frame.handle().into(), c"remoteGetter".as_ptr(), getter.handle().into()) });
    eval(&runtime, "globalThis.viaFrame = () => remoteGetter.call(held) === held; 0");
    unsafe { LeaveRealm(cx, old) };
    let old = unsafe { EnterRealm(cx, page.get()) };
    crate::rooted!(in(cx) let frame_value = ObjectValue(frame.get()));
    assert!(unsafe { JS_SetProperty(cx, page.handle().into(), c"frameGlobal".as_ptr(), frame_value.handle().into()) });
    assert!(eval(&runtime, "frameGlobal.viaFrame()").to_boolean(), "a nested caller got the entered realm's wrapper");
    unsafe { LeaveRealm(cx, old) };
    let realms = REMOTE_GETTER_REALMS.with(|realms| std::mem::take(&mut *realms.borrow_mut()));
    assert_eq!(realms, vec![remote_realm; 5], "the getter keeps running in its own realm");
    // The remote realm itself still gets its own wrapper back (no caller conversion there).
    let old = unsafe { EnterRealm(cx, remote.get()) };
    crate::rooted!(in(cx) let mut own = ObjectValue(proxy.get()));
    assert!(unsafe { JS_WrapValue(cx, own.handle_mut().into()) });
    assert!(unsafe { JS_SetProperty(cx, remote.handle().into(), c"ownProxy".as_ptr(), own.handle().into()) });
    assert!(eval(&runtime, "remoteGetter.call(ownProxy) === ownProxy").to_boolean());
    unsafe { LeaveRealm(cx, old) };
    REMOTE_GETTER_REALMS.with(|realms| realms.borrow_mut().clear());
    assert_eq!(saved_depth(unsafe { &*cx }), 0);
}

#[test]
fn escaped_native_function_keeps_its_realm_without_a_registry_root() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let weak;
    {
        crate::rooted!(in(cx) let mut function = UndefinedValue());
        {
            crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
            weak = realm_weak(unsafe { GetObjectRealmOrNull(global.get()) });
            let old = unsafe { EnterRealm(cx, global.get()) };
            function.set(ObjectValue(unsafe { JS_NewFunction(cx, Some(native_answer), 0, 0, c"answer".as_ptr()) }.cast()));
            unsafe { LeaveRealm(cx, old) };
        }
        collect(&mut runtime);
        assert!(weak.get().is_some());
        crate::rooted!(in(cx) let this = UndefinedValue());
        crate::rooted!(in(cx) let mut result = UndefinedValue());
        let args = HandleValueArray { length_: 0, elements_: ptr::null() };
        assert!(unsafe { Call(cx, this.handle().into(), function.handle().into(), &args, result.handle_mut().into()) });
        assert_eq!(result.get().to_int32(), 42);
    }
    collect(&mut runtime);
    assert!(weak.get().is_none());
}

thread_local! {
    static PRINCIPALS_DESTROYED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

unsafe extern "C" fn destroy_principals(principals: *mut JSPrincipals) {
    PRINCIPALS_DESTROYED.with(|count| count.set(count.get() + 1));
    unsafe { DestroyRustJSPrincipals(principals) };
}

#[test]
fn realm_principals_are_released_once_at_gc_and_once_at_shutdown() {
    PRINCIPALS_DESTROYED.with(|count| count.set(0));
    {
        let mut runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        unsafe { JS_InitDestroyPrincipalsCallback(cx, Some(destroy_principals)) };
        {
            let principals = unsafe { CreateRustJSPrincipals(ptr::null(), ptr::null_mut()) };
            crate::rooted!(in(cx) let global = new_global(cx, principals));
            let realm = unsafe { GetObjectRealmOrNull(global.get()) };
            assert_eq!(unsafe { GetRealmPrincipals(realm) }, principals);
            collect(&mut runtime);
            assert_eq!(PRINCIPALS_DESTROYED.with(std::cell::Cell::get), 0);
        }
        collect(&mut runtime);
        assert_eq!(PRINCIPALS_DESTROYED.with(std::cell::Cell::get), 1);
        // Keep another realm current until runtime teardown. Its principal is released by
        // shutdown; any later lazy RealmData finalizer must see an empty shared slot.
        let principals = unsafe { CreateRustJSPrincipals(ptr::null(), ptr::null_mut()) };
        crate::rooted!(in(cx) let global = new_global(cx, principals));
        unsafe { EnterRealm(cx, global.get()) };
    }
    assert_eq!(PRINCIPALS_DESTROYED.with(std::cell::Cell::get), 2);
    unsafe { JS_InitDestroyPrincipalsCallback(ptr::null_mut(), None) };
}

#[test]
fn weak_realm_registry_sweeps_dead_entries() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    for _ in 0..48 {
        {
            crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
            assert!(!global.get().is_null());
        }
        collect(&mut runtime);
    }
    assert_eq!(live_realm_count(unsafe { &*cx }), 1);
    assert!(registry_len(unsafe { &*cx }) < 20, "dead entries are pruned on registration");
}

struct IterationState {
    realms: Vec<v8::cppgc::WeakPersistent<crate::realm_impl::RealmData>>,
    visited: usize,
    all_alive: bool,
}

unsafe extern "C" fn iterate_with_gc(cx: *mut JSContext, data: *mut std::ffi::c_void, compartment: *mut Compartment) -> CompartmentIterResult {
    let state = unsafe { &mut *data.cast::<IterationState>() };
    unsafe { JS_GC(cx, GCReason::API) };
    state.all_alive &= state.realms.iter().all(|realm| realm.get().is_some());
    if !state.all_alive {
        // A broken snapshot must fail without accessing a collected compartment pointer.
        return CompartmentIterResult::Stop;
    }
    assert!(!unsafe { GetRealmGlobalOrNull(compartment.cast()) }.is_null());
    state.visited += 1;
    CompartmentIterResult::KeepGoing
}

#[test]
fn compartment_iteration_roots_its_snapshot_across_callback_gc() {
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let mut state = IterationState { realms: Vec::new(), visited: 0, all_alive: true };
    // The first realm must remain rooted while allocating the second. Drop both only
    // immediately before iteration (whose snapshot performs no V8 allocation).
    {
        crate::rooted!(in(cx) let first = new_global(cx, ptr::null_mut()));
        crate::rooted!(in(cx) let second = new_global(cx, ptr::null_mut()));
        for global in [first.get(), second.get()] {
            state.realms.push(realm_weak(unsafe { GetObjectRealmOrNull(global) }));
        }
    }
    unsafe { JS_IterateCompartments(cx, (&mut state as *mut IterationState).cast(), Some(iterate_with_gc)) };
    assert!(state.all_alive);
    assert_eq!(state.visited, 3);
    collect(&mut runtime);
    assert!(state.realms.iter().all(|realm| realm.get().is_none()));
}

#[test]
fn negative_fixtures_detect_old_registry_roots_and_missing_saved_roots() {
    use crate::realm_impl::{STRONG_REALMS_FOR_TESTING, UNROOTED_SAVED_REALMS_FOR_TESTING};
    let mut runtime = Runtime::new(JSEngineHandle::for_tests());
    let cx = runtime.raw_cx();
    let initial = unsafe { GetCurrentRealmOrNull(cx) };
    let weak = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        realm_weak(unsafe { GetObjectRealmOrNull(global.get()) })
    };
    STRONG_REALMS_FOR_TESTING.with(|flag| flag.set(true));
    collect(&mut runtime);
    let retained = weak.get().is_some();
    STRONG_REALMS_FOR_TESTING.with(|flag| flag.set(false));
    collect(&mut runtime);
    assert!(retained, "the old unconditional registry roots defeat collection");
    assert!(weak.get().is_none());

    let saved_weak = {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        let weak = realm_weak(unsafe { GetObjectRealmOrNull(global.get()) });
        unsafe { EnterRealm(cx, global.get()) };
        weak
    };
    {
        crate::rooted!(in(cx) let global = new_global(cx, ptr::null_mut()));
        unsafe { EnterRealm(cx, global.get()) };
    }
    UNROOTED_SAVED_REALMS_FOR_TESTING.with(|flag| flag.set(true));
    collect(&mut runtime);
    let lost_saved_realm = saved_weak.get().is_none();
    UNROOTED_SAVED_REALMS_FOR_TESTING.with(|flag| flag.set(false));
    // Never restore or trace the now-dangling saved pointer.
    reset_realms_for_test(unsafe { &*cx }, initial);
    assert!(lost_saved_realm, "the fixture detects a missing saved-realm edge");
}
