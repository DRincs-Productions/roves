//! Lifetime regressions for the weak realm registry, independent of Servo page roots.

use std::ptr;

use crate::jsapi::*;
use crate::jsval::*;
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
