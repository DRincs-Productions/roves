// Guest side of the software-GPU frame-pacing regression (stage
// gpu_frame_pacing in main.rs). Rust drives one embedder tick at a time and
// feeds SCRIPTED poll results through the private gpuPacingTest hook: no GPU
// device exists in this run and none is claimed. The batch of successful
// submissions is simulated by the hook setting the counter.
//
// Ticks: T1 runs `first` (which queues A, B, C) and starts a deferral; T2-T4
// see a busy queue (deferred; between T2 and T3 Rust cancels B, queues D and
// dispatches a trusted click); T5 sees QueueEmpty and runs A, C, D in order
// with T5's timestamp. Then T6 runs E (queues F) and starts a second deferral;
// T7's poll errors, which disables pacing and runs F in the same tick; T8
// with a full batch no longer defers.
(() => {
  "use strict";
  const state = {
    log: [],
    firstTimestamp: null,
    resumeTimestamps: [],
    intervalTicks: 0,
    microtasks: 0,
    lastIntervalAt: null,
    clicks: 0,
    atFirst: null,
    atResume: null,
    loggedDuringDeferral: null,
  };
  const target = document.createElement("div");
  target.setAttribute("id", "pacing-target");
  document.getElementById("app").appendChild(target);
  target.addEventListener("click", (event) => {
    if (event.isTrusted) state.clicks++;
  });
  // A timer task that also queues a microtask, every tick.
  const interval = setInterval(() => {
    state.intervalTicks++;
    state.lastIntervalAt = performance.now();
    Promise.resolve().then(() => {
      state.microtasks++;
    });
  }, 0);
  const snapshot = () => ({
    intervalTicks: state.intervalTicks,
    microtasks: state.microtasks,
    clicks: state.clicks,
    lastIntervalAt: state.lastIntervalAt,
  });
  const resumed = (name) => (timestamp) => {
    state.log.push(name);
    state.resumeTimestamps.push(timestamp);
    if (state.atResume === null) state.atResume = snapshot();
  };
  let handleB;
  requestAnimationFrame((timestamp) => {
    state.log.push("first");
    state.firstTimestamp = timestamp;
    state.atFirst = snapshot();
    requestAnimationFrame(resumed("A"));
    handleB = requestAnimationFrame(() => state.log.push("B"));
    requestAnimationFrame(resumed("C"));
  });

  globalThis.gpuPacingTestState = {
    // Between T2 and T3, while the rAF queue is deferred.
    duringDeferral() {
      state.loggedDuringDeferral = state.log.slice();
      cancelAnimationFrame(handleB);
      requestAnimationFrame(resumed("D"));
    },
    // Before T6.
    errorPhase() {
      requestAnimationFrame(() => {
        state.log.push("E");
        requestAnimationFrame(() => state.log.push("F"));
      });
    },
    // Before T8.
    disabledPhase() {
      requestAnimationFrame(() => state.log.push("G"));
    },
    stop() {
      clearInterval(interval);
      target.remove();
    },
    check(phases) {
      const fail = (message) => {
        throw new Error(`gpu frame pacing: ${message} (log ${state.log.join(",")}; phases ${JSON.stringify(phases)})`);
      };
      const expect = (stats, name, fields) => {
        for (const [key, value] of Object.entries(fields)) {
          if (stats[key] !== value) fail(`${name}: ${key} is ${stats[key]}, expected ${value}`);
        }
      };
      const [afterFirstDeferred, afterResume, afterError, afterDisabled] = phases;
      if (state.log.join(",") !== "first,A,C,D,E,F,G") fail("callbacks did not run once each, in order, with B cancelled");
      if (state.loggedDuringDeferral?.join(",") !== "first") fail("rAF callbacks ran while the queue was busy");
      expect(afterFirstDeferred, "after T2", { ticks: 2, frames: 1, deferrals: 1, deferred_ticks: 1, polls: 1, saturated: true, active: true });
      expect(afterResume, "after T5", {
        ticks: 5, frames: 2, deferrals: 1, deferred_ticks: 3, polls: 4, max_deferred_run: 3,
        poll_errors: 0, saturated: false, disabled: false, script_left: 0,
      });
      expect(afterError, "after T7", { ticks: 7, frames: 4, deferrals: 2, deferred_ticks: 3, polls: 5, poll_errors: 1, disabled: true, saturated: false });
      expect(afterDisabled, "after T8", { ticks: 8, frames: 5, deferrals: 2, deferred_ticks: 3, polls: 5, saturated: false });
      if (!(afterResume.max_tick_gap_ms > 0)) fail("max_tick_gap_ms not recorded");
      // Host work continued during the three deferred ticks.
      const before = state.atFirst;
      const after = state.atResume;
      if (!(after.intervalTicks >= before.intervalTicks + 2)) fail(`timers did not run while deferred (${before.intervalTicks} -> ${after.intervalTicks})`);
      if (!(after.microtasks >= before.microtasks + 2)) fail(`microtasks did not run while deferred (${before.microtasks} -> ${after.microtasks})`);
      if (after.clicks !== 1) fail(`the trusted click during deferral was not delivered before resume (clicks ${after.clicks})`);
      // A, C and D share the resuming tick's timestamp: current, not the one of
      // the tick that deferred them.
      const [a, c, d] = state.resumeTimestamps;
      if (!(a === c && c === d)) fail(`resumed callbacks got different timestamps ${state.resumeTimestamps}`);
      if (!(a > state.firstTimestamp)) fail("resumed timestamp is not after the first frame");
      if (!(a >= after.lastIntervalAt)) fail(`resumed timestamp ${a} predates the last timer task ${after.lastIntervalAt}`);
      return JSON.stringify({ deferred_ticks: afterResume.deferred_ticks, timer_tasks_while_deferred: after.intervalTicks - before.intervalTicks });
    },
  };
})();
