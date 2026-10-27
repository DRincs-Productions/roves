//! Native async op fixture for the single-document probe (task 0384): one Rust `Future`, driven
//! by deno_core's own op driver and event loop, exposed to the game as exactly one function.
//! Glue is kept to this one fixture; it makes no claim about Servo tasks, DOM APIs or resource
//! cancellation in general.
//!
//! - Registration: `#[op2] async fn` + `extension!(ops = [...], js = [...])`. deno_core wraps the
//!   op in its `setUpAsyncStub` (`00_infra.js`), so a call returns a Promise that its event loop
//!   settles when the Rust future completes. The extension's trusted `js` source runs inside
//!   `JsRuntime::try_new`, while `Deno.core.ops` still exists, and captures the op into one
//!   arrow function on `globalThis.nativeFixture`; [`crate::new_runtime_with`] then removes the
//!   bootstrap namespace, so the game never sees `Deno` or the op by name.
//! - Driving: deno_core polls the future once eagerly on submit (`FuturesUnorderedDriver::
//!   submit_op_*`), then from its tokio-spawned driver task; `JsRuntime::run_event_loop` and
//!   `poll_event_loop` resolve the Promise. No executor, job queue, waker or timer of ours: the
//!   suspension points are tokio's `task::yield_now` and `sync::Notify`.
//! - State: per-runtime [`FixtureState`] lives in deno_core's public `OpState`; the op clones
//!   what it needs out of it before its first `await`, so no `RefCell` borrow spans a suspension.
//! - Observation: [`Counters`] (shared by `Rc`, outliving the runtime) count first polls,
//!   resumptions after the gate, completions and cancellations (an [`OpGuard`] dropped before or
//!   after the op finished) and the drops of each op's owned [`Payload`].

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use std::task::Poll;

use deno_core::JsRuntime;
use deno_core::OpState;
use deno_core::PollEventLoopOptions;
use deno_core::error::ResourceError;
use deno_core::op2;
use tokio::sync::Notify;

use crate::Log;
use crate::MemoryLoader;
use crate::err;
use crate::new_runtime_with;
use crate::privileged_ops;
use crate::read_log;
use crate::script;

/// Lifecycle counts of every fixture op run by one runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpCounts {
    /// Op bodies that reached their first poll (deno_core polls eagerly on submit).
    pub started: usize,
    /// `wait:` ops that ran past the gate.
    pub resumed_after_gate: usize,
    /// Guards dropped after their op produced a result (`Ok` or `Err`).
    pub completed: usize,
    /// Guards dropped while their op was still suspended: the future itself was dropped.
    pub cancelled: usize,
    /// Owned payloads freed.
    pub payloads_dropped: usize,
}

#[derive(Default)]
pub struct Counters {
    counts: Cell<OpCounts>,
    /// The most recent op's payload; upgrading it shows whether that op's owned data is alive.
    last_payload: RefCell<Weak<Payload>>,
}

impl Counters {
    fn bump(&self, update: impl FnOnce(&mut OpCounts)) {
        let mut counts = self.counts.get();
        update(&mut counts);
        self.counts.set(counts);
    }

    fn last_payload_alive(&self) -> bool {
        self.last_payload.borrow().upgrade().is_some()
    }
}

/// Owned per-op data: built from the op's owned `String` argument, freed with the future.
struct Payload {
    text: String,
    counters: Rc<Counters>,
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.counters.bump(|counts| counts.payloads_dropped += 1);
    }
}

/// Dropped exactly once per op: after the result (completion) or with the suspended future
/// (cancellation).
struct OpGuard {
    counters: Rc<Counters>,
    finished: bool,
}

impl Drop for OpGuard {
    fn drop(&mut self) {
        let finished = self.finished;
        self.counters.bump(|counts| {
            if finished {
                counts.completed += 1
            } else {
                counts.cancelled += 1
            }
        });
    }
}

/// Per-runtime fixture state in `OpState`.
struct FixtureState {
    counters: Rc<Counters>,
    gate: Rc<Notify>,
}

/// `"ok:<text>"` and `"fail:<text>"` yield once to tokio, then fulfil with an owned `String` or
/// reject with a `BadResource` error; `"wait:<text>"` stays suspended on the gate until it is
/// notified.
#[op2]
#[string]
async fn op_roves_fixture_native(
    state: Rc<RefCell<OpState>>,
    #[string] input: String,
) -> Result<String, ResourceError> {
    // Cloned out before the first await: no OpState borrow is held while suspended.
    let (counters, gate) = {
        let state = state.borrow();
        let fixture = state.borrow::<FixtureState>();
        (fixture.counters.clone(), fixture.gate.clone())
    };
    let (mode, text) = input.split_once(':').unwrap_or((input.as_str(), ""));
    let mode = mode.to_owned();
    let payload = Rc::new(Payload { text: text.to_owned(), counters: counters.clone() });
    *counters.last_payload.borrow_mut() = Rc::downgrade(&payload);
    counters.bump(|counts| counts.started += 1);
    let mut guard = OpGuard { counters: counters.clone(), finished: false };
    let result = match mode.as_str() {
        "ok" => {
            tokio::task::yield_now().await;
            Ok(format!("native:{}", payload.text))
        },
        "fail" => {
            tokio::task::yield_now().await;
            Err(ResourceError::Other(format!("fixture failed: {}", payload.text)))
        },
        "wait" => {
            gate.notified().await;
            counters.bump(|counts| counts.resumed_after_gate += 1);
            Ok(format!("released:{}", payload.text))
        },
        _ => Err(ResourceError::Other(format!("unknown fixture mode: {mode}"))),
    };
    guard.finished = true;
    result
}

deno_core::extension!(
    roves_native_async_probe,
    ops = [op_roves_fixture_native],
    js = ["ext:roves_native_async_probe/capture.js" = {
        source = r#"
"use strict";
(() => {
  const op = Deno.core.ops.op_roves_fixture_native;
  const nativeFixture = (input) => op(input);
  Object.defineProperty(globalThis, "nativeFixture", {
    value: nativeFixture, writable: false, enumerable: false, configurable: true,
  });
})();
"#
    }],
);

fn fixture_runtime(counters: &Rc<Counters>, gate: &Rc<Notify>) -> Result<JsRuntime, String> {
    let runtime = new_runtime_with(Rc::new(MemoryLoader::new(&[])), vec![roves_native_async_probe::init()])?;
    runtime
        .op_state()
        .borrow_mut()
        .put(FixtureState { counters: counters.clone(), gate: gate.clone() });
    Ok(runtime)
}

/// What `native_async_op_lifecycle` observed.
pub struct NativeAsyncReport {
    /// Completion runtime: the game log right after its classic script returned.
    pub completion_log_after_script: Log,
    pub completion_counts_after_script: OpCounts,
    /// The game log once `run_event_loop` settled both ops.
    pub completion_log: Log,
    pub completion_counts: OpCounts,
    /// `op_names()` matching the bootstrap check's privileged prefixes, and whether the fixture
    /// op itself is registered.
    pub privileged_ops: Vec<String>,
    pub fixture_op_registered: bool,
    /// `run_event_loop` result after a rejection nobody handled.
    pub unhandled_error: Option<String>,
    pub unhandled_counts: OpCounts,
    pub completion_payload_alive_after_teardown: bool,

    /// Cancellation runtime: the game log after its script and one `poll_event_loop`.
    pub pending_log: Log,
    pub pending_counts_before_poll: OpCounts,
    pub pending_poll_was_pending: bool,
    pub pending_counts_after_poll: OpCounts,
    pub pending_payload_alive_before_drop: bool,
    pub cancel_counts_after_drop: OpCounts,
    pub cancel_payload_alive_after_drop: bool,
    /// Strong references left once the runtime is gone: only the test's own.
    pub gate_strong_after_drop: usize,
    pub counters_strong_after_drop: usize,
    /// Counts after the gate was notified and tokio ran its other tasks.
    pub counts_after_gate_release: OpCounts,
}

const COMPLETION_GAME: &str = r#"(() => {
  globalThis.log = [];
  log.push("bootstrap-globals:" + ["Deno", "__bootstrap", "__infra"].filter(name => name in globalThis).join());
  log.push("fixture:" + typeof nativeFixture + ":" + Object.getOwnPropertyNames(globalThis).filter(name => name.startsWith("op_") || name === "nativeFixture").join());
  const settle = (label, promise) => promise.then(
    value => { log.push(label + ":fulfilled:" + value); Promise.resolve().then(() => log.push(label + ":chained")); },
    error => log.push(label + ":rejected:" + error.name + ":" + error.message),
  );
  const ok = nativeFixture("ok:alpha");
  log.push("returns-promise:" + (ok instanceof Promise));
  log.push("internal-id-symbol:" + (Symbol.for("Deno.core.internalPromiseId") in ok));
  settle("ok", ok);
  settle("fail", nativeFixture("fail:beta"));
  Promise.resolve().then(() => log.push("promise-job"));
  log.push("script:returned");
})()"#;

const PENDING_GAME: &str = r#"(() => {
  globalThis.log = [];
  nativeFixture("wait:gamma").then(
    value => log.push("wait:fulfilled:" + value),
    error => log.push("wait:rejected:" + error.name),
  );
  Promise.resolve().then(() => log.push("promise-job"));
  log.push("script:returned");
})()"#;

/// A real Rust future behind one game-visible function: completion with owned input and
/// result, a completion error, an unhandled rejection surfacing from the event loop, and a
/// pending op that runtime shutdown drops without ever calling back into the game.
pub fn native_async_op_lifecycle() -> Result<NativeAsyncReport, String> {
    crate::block_on(async {
        // Completion.
        let counters = Rc::new(Counters::default());
        let gate = Rc::new(Notify::new());
        let mut runtime = fixture_runtime(&counters, &gate)?;
        let privileged_ops = privileged_ops(&runtime);
        let fixture_op_registered = runtime.op_names().contains(&"op_roves_fixture_native");
        script(&mut runtime, "completion.js", COMPLETION_GAME)?;
        let completion_log_after_script = read_log(&mut runtime)?;
        let completion_counts_after_script = counters.counts.get();
        runtime.run_event_loop(Default::default()).await.map_err(err)?;
        let completion_log = read_log(&mut runtime)?;
        let completion_counts = counters.counts.get();

        script(&mut runtime, "unhandled.js", r#"nativeFixture("fail:unhandled");"#)?;
        let unhandled_error = runtime.run_event_loop(Default::default()).await.err().map(err);
        let unhandled_counts = counters.counts.get();
        drop(runtime);
        let completion_payload_alive_after_teardown = counters.last_payload_alive();

        // Cancellation.
        let counters = Rc::new(Counters::default());
        let gate = Rc::new(Notify::new());
        let mut runtime = fixture_runtime(&counters, &gate)?;
        script(&mut runtime, "pending.js", PENDING_GAME)?;
        let pending_counts_before_poll = counters.counts.get();
        // One tick of deno_core's public event loop: Promise jobs run, the op stays pending.
        let poll = std::future::poll_fn(|cx| Poll::Ready(runtime.poll_event_loop(cx, PollEventLoopOptions::default()))).await;
        let pending_poll_was_pending = match poll {
            Poll::Pending => true,
            Poll::Ready(result) => {
                result.map_err(err)?;
                false
            },
        };
        let pending_log = read_log(&mut runtime)?;
        let pending_counts_after_poll = counters.counts.get();
        let pending_payload_alive_before_drop = counters.last_payload_alive();

        drop(runtime);
        let cancel_counts_after_drop = counters.counts.get();
        let cancel_payload_alive_after_drop = counters.last_payload_alive();
        let gate_strong_after_drop = Rc::strong_count(&gate);
        let counters_strong_after_drop = Rc::strong_count(&counters);

        // Releasing the gate now must not resume anything: the future is gone.
        gate.notify_waiters();
        gate.notify_one();
        tokio::task::yield_now().await;
        let counts_after_gate_release = counters.counts.get();

        Ok(NativeAsyncReport {
            completion_log_after_script,
            completion_counts_after_script,
            completion_log,
            completion_counts,
            privileged_ops,
            fixture_op_registered,
            unhandled_error,
            unhandled_counts,
            completion_payload_alive_after_teardown,
            pending_log,
            pending_counts_before_poll,
            pending_poll_was_pending,
            pending_counts_after_poll,
            pending_payload_alive_before_drop,
            cancel_counts_after_drop,
            cancel_payload_alive_after_drop,
            gate_strong_after_drop,
            counters_strong_after_drop,
            counts_after_gate_release,
        })
    })
}
