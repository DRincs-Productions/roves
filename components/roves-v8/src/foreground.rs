/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! V8's foreground tasks: work V8 posts, often from its background threads, to run on an
//! isolate's own thread. Examples are finishing an asynchronous WebAssembly compilation,
//! `FinalizationRegistry` cleanup and `Atomics.waitAsync` timeouts.
//!
//! The default V8 platform queues these tasks until the embedder pumps its message loop. The
//! process-wide platform here hands each task to the handler registered for its isolate,
//! which brings it to the isolate's event loop. roves-js registers one per runtime.
//! - A task for an isolate without a handler is leaked, never run nor dropped: its isolate may
//!   be gone, and destroying a V8 task can touch it.
//! - Removing a handler waits for the deliveries already accepted for it, and none start
//!   afterwards: every task accepted while it was registered reaches it before removal returns.
//! - Delayed tasks wait on one timer thread and weakly capture their original registration;
//!   removing or replacing it never retargets them to a reused isolate address. Removing it
//!   destroys its waiting delayed tasks on the caller's thread, while the isolate is alive.

use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

/// Receives the tasks of one isolate, on any thread.
pub type Handler = Arc<dyn Fn(v8::Task) + Send + Sync>;

struct Registration {
    handler: Handler,
    deliveries: Mutex<Deliveries>,
    idle: Condvar,
}

struct Deliveries {
    active: bool,
    running: usize,
    /// A removal is waiting for the running deliveries (observed by tests).
    #[cfg(test)]
    draining: bool,
}

impl Registration {
    fn new(handler: Handler) -> Self {
        let deliveries = Deliveries { active: true, running: 0, #[cfg(test)] draining: false };
        Registration { handler, deliveries: Mutex::new(deliveries), idle: Condvar::new() }
    }

    fn deliveries(&self) -> MutexGuard<'_, Deliveries> {
        self.deliveries.lock().unwrap()
    }

    /// Accepts a delivery unless retired. Callers decide under the lock that also retires it.
    fn begin(&self) -> bool {
        let mut deliveries = self.deliveries();
        deliveries.running += usize::from(deliveries.active);
        deliveries.active
    }

    /// Runs the handler of a delivery accepted by `begin`, without holding any lock.
    fn deliver(&self, task: v8::Task) {
        struct Finish<'r>(&'r Registration);
        impl Drop for Finish<'_> {
            fn drop(&mut self) {
                let mut deliveries = self.0.deliveries();
                deliveries.running -= 1;
                if deliveries.running == 0 {
                    self.0.idle.notify_all();
                }
            }
        }
        let _finish = Finish(self);
        (self.handler)(task);
    }

    /// Waits until the deliveries accepted before retirement have returned. Holds no registry or
    /// timer lock: a handler may reenter them.
    fn drain(&self) {
        let mut deliveries = self.deliveries();
        #[cfg(test)]
        { deliveries.draining = true; }
        while deliveries.running > 0 {
            deliveries = self.idle.wait(deliveries).unwrap();
        }
    }
}

fn handlers() -> &'static Mutex<HashMap<usize, Arc<Registration>>> {
    static HANDLERS: OnceLock<Mutex<HashMap<usize, Arc<Registration>>>> = OnceLock::new();
    HANDLERS.get_or_init(Default::default)
}

/// The key of `isolate` in the handler registry (its address).
pub fn isolate_key(isolate: &v8::Isolate) -> usize {
    // SAFETY: only the address is used.
    let raw = unsafe { isolate.as_raw_isolate_ptr() };
    // SAFETY: `UnsafeRawIsolatePtr` is one pointer.
    unsafe { std::mem::transmute::<v8::UnsafeRawIsolatePtr, usize>(raw) }
}

/// Sets (or, with `None`, removes) the handler of the isolate with key `isolate`. Remove it
/// before the isolate is disposed. Retired registrations reject later deliveries, and this
/// returns only once the deliveries they already accepted have returned (so a handler must not
/// remove its own registration). Tasks handed to the embedder remain subject to its
/// foreground/shutdown ordering.
pub fn set_handler(isolate: usize, handler: Option<Handler>) {
    let (previous, reclaimed) = {
        let mut handlers = handlers().lock().unwrap();
        let previous = handlers.remove(&isolate);
        // Removal precedes disposal, so the isolate's delayed tasks can still be destroyed
        // instead of leaked when due. A replacement keeps the old policy: nothing says that
        // isolate is still alive.
        let reclaimed = match &previous {
            Some(previous) => retire(previous, handler.is_none()),
            None => Vec::new(),
        };
        if let Some(handler) = handler {
            handlers.insert(isolate, Arc::new(Registration::new(handler)));
        }
        (previous, reclaimed)
    };
    // A captured embedder value can reenter the registry from its destructor, and a running
    // delivery can reenter it too. Release the registry lock before waiting for deliveries,
    // dropping the retired callback (on both removal and replacement) and destroying native
    // tasks.
    if let Some(previous) = &previous {
        previous.drain();
    }
    drop(reclaimed);
    drop(previous);
}

/// Stops `registration` from accepting deliveries and, when `reclaim`, removes its waiting
/// delayed tasks. Called under the registry lock, which `post` and `post_delayed` also hold
/// while accepting, so none can be accepted for it afterwards. The timer lock is held as well:
/// a due task the timer has popped was either accepted before, or is still queued and taken.
fn retire(registration: &Arc<Registration>, reclaim: bool) -> Vec<Delayed> {
    let queue = TIMER.get().map(|timer| timer.queue.lock().unwrap());
    registration.deliveries().active = false;
    let Some(mut queue) = queue.filter(|_| reclaim) else { return Vec::new() };
    let (taken, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut queue.0)
        .into_iter()
        .partition(|delayed| std::ptr::eq(delayed.registration.as_ptr(), Arc::as_ptr(registration)));
    queue.0 = kept.into();
    taken
}

fn post(isolate: usize, task: v8::Task) {
    let registration = handlers().lock().unwrap().get(&isolate).filter(|registration| registration.begin()).cloned();
    match registration {
        Some(registration) => registration.deliver(task),
        None => std::mem::forget(task),
    }
}

struct Delayed {
    due: Instant,
    sequence: u64,
    registration: Weak<Registration>,
    task: v8::Task,
}

impl PartialEq for Delayed {
    fn eq(&self, other: &Self) -> bool {
        (self.due, self.sequence) == (other.due, other.sequence)
    }
}

impl Eq for Delayed {}

impl PartialOrd for Delayed {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Delayed {
    // `BinaryHeap` is a max-heap: the earliest task compares greatest.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (other.due, other.sequence).cmp(&(self.due, self.sequence))
    }
}

#[derive(Default)]
struct Timer {
    queue: Mutex<(BinaryHeap<Delayed>, u64)>,
    wake: Condvar,
}

static TIMER: OnceLock<Timer> = OnceLock::new();

fn timer() -> &'static Timer {
    TIMER.get_or_init(|| {
        std::thread::Builder::new()
            .name("roves-v8 timer".into())
            .spawn(run_timer)
            .expect("the V8 timer thread starts");
        Timer::default()
    })
}

fn post_delayed(isolate: usize, task: v8::Task, delay_in_seconds: f64) {
    // Capture ownership when queued, not when due. Isolate addresses may be reused by a
    // later runtime; a delayed task must never be sent to its handler. Queue under the
    // registry lock, so a concurrent removal reclaims the task (see `retire`).
    let handlers = handlers().lock().unwrap();
    let Some(registration) = handlers.get(&isolate).map(Arc::downgrade) else {
        // Same no-handler policy as an immediate task.
        std::mem::forget(task);
        return;
    };
    let delay = Duration::try_from_secs_f64(delay_in_seconds.max(0.0)).unwrap_or(Duration::MAX);
    let Some(due) = Instant::now().checked_add(delay) else {
        // Never due: like a task that never runs.
        std::mem::forget(task);
        return;
    };
    let timer = timer();
    let mut queue = timer.queue.lock().unwrap();
    queue.1 += 1;
    let sequence = queue.1;
    queue.0.push(Delayed { due, sequence, registration, task });
    timer.wake.notify_one();
    drop(queue);
    drop(handlers);
}

fn run_timer() {
    let timer = timer();
    let mut queue = timer.queue.lock().unwrap();
    loop {
        let now = Instant::now();
        match queue.0.peek().map(|next| next.due) {
            Some(due) if due <= now => {
                let Delayed { registration, task, .. } = queue.0.pop().expect("a due task");
                // Accept it under the timer lock, which a removal also holds (see `retire`).
                let registration = registration.upgrade().filter(|registration| registration.begin());
                drop(queue);
                match registration {
                    Some(registration) => registration.deliver(task),
                    None => std::mem::forget(task),
                }
                queue = timer.queue.lock().unwrap();
            },
            Some(due) => queue = timer.wake.wait_timeout(queue, due - now).unwrap().0,
            None => queue = timer.wake.wait(queue).unwrap(),
        }
    }
}

/// The platform's foreground task runner.
pub(crate) struct Platform;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    fn handler() -> Handler {
        // These tests inspect routing without constructing or running a native V8 Task.
        Arc::new(|_| panic!("routing test must not execute a V8 task"))
    }

    /// Whether this thread can take the registry lock. Other tests hold it only briefly, so a
    /// lock that stays taken detects a deadlock without hanging the suite.
    fn registry_unlocked() -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if handlers().try_lock().is_ok() { return true; }
            if Instant::now() >= deadline { return false; }
            std::thread::yield_now();
        }
    }

    #[test]
    fn removed_registration_cannot_deliver_a_delayed_task() {
        const KEY: usize = 1;
        let callback = handler();
        set_handler(KEY, Some(callback.clone()));
        let registration = handlers().lock().unwrap()[&KEY].clone();
        let delayed = Arc::downgrade(&registration);
        assert!(registration.deliveries().active);
        set_handler(KEY, None);
        // Even a dispatcher that still owns the old registration cannot use it once
        // retired. Keeping an external Arc to the callback does not keep it active.
        assert!(!delayed.upgrade().unwrap().begin());
        drop(registration);
        assert!(delayed.upgrade().is_none());
        assert_eq!(Arc::strong_count(&callback), 1);
    }

    #[test]
    fn reused_isolate_address_does_not_retarget_old_delayed_tasks() {
        const KEY: usize = 2;
        set_handler(KEY, Some(handler()));
        let old = handlers().lock().unwrap()[&KEY].clone();
        let delayed = Arc::downgrade(&old);
        // Replacing a registration also invalidates it without requiring a separate remove.
        set_handler(KEY, Some(handler()));
        let replacement = handlers().lock().unwrap()[&KEY].clone();
        assert!(!Arc::ptr_eq(&delayed.upgrade().unwrap(), &replacement));
        assert!(!delayed.upgrade().unwrap().begin());
        assert!(replacement.deliveries().active);
        set_handler(KEY, None);
    }

    #[test]
    fn retired_callback_is_destroyed_outside_registry_lock() {
        struct CheckDrop(Arc<AtomicBool>);
        impl Drop for CheckDrop {
            fn drop(&mut self) {
                self.0.store(registry_unlocked(), AtomicOrdering::Release);
            }
        }
        for (key, replace) in [(4, false), (5, true)] {
            let unlocked = Arc::new(AtomicBool::new(false));
            let guard = CheckDrop(unlocked.clone());
            let callback: Handler = Arc::new(move |_| { let _keep_capture = &guard; });
            set_handler(key, Some(callback));
            set_handler(key, replace.then(handler));
            assert!(unlocked.load(AtomicOrdering::Acquire));
            set_handler(key, None);
        }
    }

    fn delayed_count(registration: &Weak<Registration>) -> usize {
        let queue = timer().queue.lock().unwrap();
        queue.0.iter().filter(|delayed| delayed.registration.ptr_eq(registration)).count()
    }

    #[test]
    fn removed_registration_reclaims_its_delayed_tasks() {
        // A real delayed V8 task: the timeout of an `Atomics.waitAsync` that never resolves.
        crate::initialize_engine();
        let mut isolate = v8::Isolate::new(Default::default());
        let key = isolate_key(&isolate);
        set_handler(key, Some(handler()));
        let registration = Arc::downgrade(&handlers().lock().unwrap()[&key]);
        {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let source = v8::String::new(scope, "Atomics.waitAsync(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1e9).async").unwrap();
            assert!(v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap().is_true());
        }
        assert_eq!(delayed_count(&registration), 1);
        // Removal destroys it now, with the isolate alive, rather than leaking it when due.
        set_handler(key, None);
        assert_eq!(delayed_count(&registration), 0);
        assert!(registration.upgrade().is_none());
        drop(isolate);
    }

    /// A real foreground task of `isolate`: the timeout of a short `Atomics.waitAsync`, captured
    /// when due. Destroying it is safe while the isolate is alive.
    fn real_task(isolate: &mut v8::Isolate) -> v8::Task {
        let key = isolate_key(isolate);
        let (sender, receiver) = std::sync::mpsc::channel();
        let sender = Mutex::new(sender);
        set_handler(key, Some(Arc::new(move |task| {
            // Never destroy a task on the timer thread, not even after the test.
            if let Err(unsent) = sender.lock().unwrap().send(task) { std::mem::forget(unsent.0); }
        })));
        {
            v8::scope!(let scope, isolate);
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let source = v8::String::new(scope, "Atomics.waitAsync(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1).async").unwrap();
            assert!(v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap().is_true());
        }
        let task = receiver.recv_timeout(Duration::from_secs(10)).expect("V8 posts the timeout");
        set_handler(key, None);
        // Other tasks V8 posted meanwhile are destroyed here, with the isolate alive.
        drop(receiver);
        task
    }

    #[test]
    fn removal_waits_for_a_delivery_already_accepted() {
        use std::sync::mpsc::channel;
        use std::thread::{self, ThreadId};
        thread_local!(static RACING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
        /// The test's foreground queue entry: records the thread that destroys its task.
        struct Kept(#[allow(dead_code)] v8::Task, Arc<Mutex<Vec<ThreadId>>>);
        impl Drop for Kept {
            fn drop(&mut self) {
                self.1.lock().unwrap().push(thread::current().id());
            }
        }

        crate::initialize_engine();
        let mut isolate = v8::Isolate::new(Default::default());
        let key = isolate_key(&isolate);
        let task = real_task(&mut isolate);
        let destroyed = Arc::new(Mutex::new(Vec::new()));
        // Destroyed on this, the isolate's, thread before disposal, like `retire` in roves-js.
        let kept = Arc::new(Mutex::new(Vec::new()));
        let other = Arc::new(Mutex::new(Vec::new()));
        let reentered = Arc::new(AtomicBool::new(false));
        let (entered, on_entered) = channel();
        let (release, on_release) = channel::<()>();
        let (entered, on_release) = (Mutex::new(entered), Mutex::new(on_release));
        set_handler(key, Some({
            let (kept, other, destroyed, reentered) = (kept.clone(), other.clone(), destroyed.clone(), reentered.clone());
            Arc::new(move |task| {
                if !RACING.get() {
                    // Unrelated V8 work for this isolate.
                    other.lock().unwrap().push(task);
                    return;
                }
                entered.lock().unwrap().send(()).unwrap();
                on_release.lock().unwrap().recv().unwrap();
                // A removal waiting for this delivery holds no registry lock.
                reentered.store(registry_unlocked(), AtomicOrdering::Release);
                kept.lock().unwrap().push(Kept(task, destroyed.clone()));
            })
        }));
        let registration = handlers().lock().unwrap()[&key].clone();
        let poster = thread::spawn(move || {
            RACING.set(true);
            post(key, task);
        });
        // Accepted and running on the poster's thread.
        on_entered.recv().unwrap();
        let returned = Arc::new(AtomicBool::new(false));
        let releaser = thread::spawn({
            let (registration, returned) = (registration.clone(), returned.clone());
            move || {
                // Let the delivery go only while the removal waits for it or, without that
                // wait, after the removal has returned: the outcome does not depend on timing.
                loop {
                    let deliveries = registration.deliveries();
                    if (!deliveries.active && deliveries.draining) || returned.load(AtomicOrdering::Acquire) { break; }
                    drop(deliveries);
                    thread::yield_now();
                }
                release.send(()).unwrap();
            }
        });
        set_handler(key, None);
        let delivered_before_return = kept.lock().unwrap().len();
        returned.store(true, AtomicOrdering::Release);
        releaser.join().unwrap();
        poster.join().unwrap();
        assert_eq!(delivered_before_return, 1, "removal returned while an accepted delivery was still running");
        assert!(reentered.load(AtomicOrdering::Acquire), "a running delivery can reenter the registry");
        assert!(!registration.begin(), "a removed registration accepts nothing more");
        assert!(destroyed.lock().unwrap().is_empty(), "never run nor destroyed by the delivery");
        drop(std::mem::take(&mut *kept.lock().unwrap()));
        drop(std::mem::take(&mut *other.lock().unwrap()));
        assert_eq!(*destroyed.lock().unwrap(), [thread::current().id()], "destroyed once, on the isolate's thread");
        drop(registration);
        drop(isolate);
    }

    #[test]
    fn live_registration_keeps_its_original_handler() {
        const KEY: usize = 3;
        let callback = handler();
        set_handler(KEY, Some(callback.clone()));
        let delayed = Arc::downgrade(&handlers().lock().unwrap()[&KEY]);
        let selected = delayed.upgrade().unwrap();
        assert!(selected.deliveries().active);
        assert!(Arc::ptr_eq(&callback, &selected.handler));
        set_handler(KEY, None);
    }
}

impl v8::PlatformImpl for Platform {
    fn post_task(&self, isolate: *mut std::ffi::c_void, task: v8::Task) {
        post(isolate as usize, task);
    }

    fn post_non_nestable_task(&self, isolate: *mut std::ffi::c_void, task: v8::Task) {
        post(isolate as usize, task);
    }

    fn post_delayed_task(&self, isolate: *mut std::ffi::c_void, task: v8::Task, delay_in_seconds: f64) {
        post_delayed(isolate as usize, task, delay_in_seconds);
    }

    fn post_non_nestable_delayed_task(&self, isolate: *mut std::ffi::c_void, task: v8::Task, delay_in_seconds: f64) {
        post_delayed(isolate as usize, task, delay_in_seconds);
    }

    fn post_idle_task(&self, _isolate: *mut std::ffi::c_void, task: v8::IdleTask) {
        // Idle tasks are disabled (`idle_task_support` is off): V8 does not post any.
        std::mem::forget(task);
    }
}
