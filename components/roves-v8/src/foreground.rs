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
//! - Delayed tasks wait on one timer thread.

use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Receives the tasks of one isolate, on any thread.
pub type Handler = Arc<dyn Fn(v8::Task) + Send + Sync>;

fn handlers() -> &'static Mutex<HashMap<usize, Handler>> {
    static HANDLERS: OnceLock<Mutex<HashMap<usize, Handler>>> = OnceLock::new();
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
/// before the isolate is disposed.
pub fn set_handler(isolate: usize, handler: Option<Handler>) {
    let mut handlers = handlers().lock().unwrap();
    match handler {
        Some(handler) => handlers.insert(isolate, handler),
        None => handlers.remove(&isolate),
    };
}

fn post(isolate: usize, task: v8::Task) {
    let handler = handlers().lock().unwrap().get(&isolate).cloned();
    match handler {
        Some(handler) => handler(task),
        None => std::mem::forget(task),
    }
}

struct Delayed {
    due: Instant,
    sequence: u64,
    isolate: usize,
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
    queue.0.push(Delayed { due, sequence, isolate, task });
    timer.wake.notify_one();
}

fn run_timer() {
    let timer = timer();
    let mut queue = timer.queue.lock().unwrap();
    loop {
        let now = Instant::now();
        match queue.0.peek().map(|next| next.due) {
            Some(due) if due <= now => {
                let Delayed { isolate, task, .. } = queue.0.pop().expect("a due task");
                drop(queue);
                post(isolate, task);
                queue = timer.queue.lock().unwrap();
            },
            Some(due) => queue = timer.wake.wait_timeout(queue, due - now).unwrap().0,
            None => queue = timer.wake.wait(queue).unwrap(),
        }
    }
}

/// The platform's foreground task runner.
pub(crate) struct Platform;

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
