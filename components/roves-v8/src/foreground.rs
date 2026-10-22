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
//! - Delayed tasks wait on one timer thread and weakly capture their original registration;
//!   removing or replacing it never retargets them to a reused isolate address.

use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

/// Receives the tasks of one isolate, on any thread.
pub type Handler = Arc<dyn Fn(v8::Task) + Send + Sync>;

struct Registration {
    handler: Handler,
    active: AtomicBool,
}

impl Registration {
    fn handler(&self) -> Option<Handler> {
        self.active.load(AtomicOrdering::Acquire).then(|| self.handler.clone())
    }

    fn post(&self, task: v8::Task) {
        match self.handler() {
            Some(handler) => handler(task),
            // Its isolate may have been disposed; even destruction is unsafe then.
            None => std::mem::forget(task),
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
/// before the isolate is disposed. Retired registrations reject later deliveries; tasks
/// already handed to the embedder remain subject to its foreground/shutdown ordering.
pub fn set_handler(isolate: usize, handler: Option<Handler>) {
    let previous = {
        let mut handlers = handlers().lock().unwrap();
        let previous = handlers.remove(&isolate);
        if let Some(previous) = &previous {
            previous.active.store(false, AtomicOrdering::Release);
        }
        if let Some(handler) = handler {
            handlers.insert(isolate, Arc::new(Registration { handler, active: AtomicBool::new(true) }));
        }
        previous
    };
    // A captured embedder value can reenter the registry from its destructor. Release the
    // registry lock before dropping the retired callback, on both removal and replacement.
    drop(previous);
}

fn post(isolate: usize, task: v8::Task) {
    let registration = handlers().lock().unwrap().get(&isolate).cloned();
    match registration {
        Some(registration) => registration.post(task),
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

fn timer() -> &'static Timer {
    static TIMER: OnceLock<Timer> = OnceLock::new();
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
    // later runtime; a delayed task must never be sent to its handler.
    let registration = handlers().lock().unwrap().get(&isolate).map(Arc::downgrade);
    let Some(registration) = registration else {
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
}

fn run_timer() {
    let timer = timer();
    let mut queue = timer.queue.lock().unwrap();
    loop {
        let now = Instant::now();
        match queue.0.peek().map(|next| next.due) {
            Some(due) if due <= now => {
                let Delayed { registration, task, .. } = queue.0.pop().expect("a due task");
                drop(queue);
                match registration.upgrade() {
                    Some(registration) => registration.post(task),
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

    fn handler() -> Handler {
        // These tests inspect routing without constructing or running a native V8 Task.
        Arc::new(|_| panic!("routing test must not execute a V8 task"))
    }

    #[test]
    fn removed_registration_cannot_deliver_a_delayed_task() {
        const KEY: usize = 1;
        let callback = handler();
        set_handler(KEY, Some(callback.clone()));
        let registration = handlers().lock().unwrap()[&KEY].clone();
        let delayed = Arc::downgrade(&registration);
        assert!(registration.handler().is_some());
        set_handler(KEY, None);
        // Even a dispatcher that still owns the old registration cannot use it once
        // retired. Keeping an external Arc to the callback does not keep it active.
        assert!(delayed.upgrade().unwrap().handler().is_none());
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
        assert!(delayed.upgrade().unwrap().handler().is_none());
        assert!(replacement.handler().is_some());
        set_handler(KEY, None);
    }

    #[test]
    fn retired_callback_is_destroyed_outside_registry_lock() {
        struct CheckDrop(Arc<AtomicBool>);
        impl Drop for CheckDrop {
            fn drop(&mut self) {
                // A failed try_lock detects the old deadlock without hanging the suite.
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    if let Ok(registry) = handlers().try_lock() {
                        drop(registry);
                        self.0.store(true, AtomicOrdering::Release);
                        break;
                    }
                    if Instant::now() >= deadline { break; }
                    std::thread::yield_now();
                }
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

    #[test]
    fn live_registration_keeps_its_original_handler() {
        const KEY: usize = 3;
        let callback = handler();
        set_handler(KEY, Some(callback.clone()));
        let delayed = Arc::downgrade(&handlers().lock().unwrap()[&KEY]);
        let selected = delayed.upgrade().unwrap().handler().unwrap();
        assert!(Arc::ptr_eq(&callback, &selected));
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
