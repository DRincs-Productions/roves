// Guest side of the software-GPU frame-pacing regression (stage
// gpu_frame_pacing in main.rs). Rust drives one embedder tick at a time and
// feeds SCRIPTED poll results through the private gpuPacingTest hook: no GPU
// device exists in this run and none is claimed. The batch of successful
// submissions is simulated by the hook setting the counter.
//
// Ticks: T1 runs `first` (which queues A, B, C) and starts a deferral; T2-T4
// see a busy queue (deferred; between T2 and T3 Rust cancels B, queues D and
// dispatches a trusted click); T5 sees the completed batch and runs A, C, D in order
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
  // Fake WebGPU classes for the opt-in render-work counters (gpuPacingTest
  // "work" wraps their prototypes). Each method checks its receiver's brand
  // like deno_webgpu's; the queue converts its argument with one iteration,
  // as deno_webgpu's sequence conversion does, and rejects a buffer submitted
  // twice.
  class GPUTextureView {}
  class GPURenderPassEncoder {
    #ended = false;
    draw(_vertexCount) {
      if (!(#ended in this)) throw new TypeError("Illegal invocation");
    }
    drawIndexed(_indexCount) {
      if (!(#ended in this)) throw new TypeError("Illegal invocation");
    }
    drawIndirect(_buffer, _offset) {
      if (!(#ended in this)) throw new TypeError("Illegal invocation");
    }
    drawIndexedIndirect(_buffer, _offset) {
      if (!(#ended in this)) throw new TypeError("Illegal invocation");
    }
    end() {
      if (!(#ended in this)) throw new TypeError("Illegal invocation");
      this.#ended = true;
    }
  }
  class GPUCommandBuffer {
    submitted = false;
  }
  class GPUCommandEncoder {
    #brand = true;
    beginRenderPass(descriptor) {
      if (!(#brand in this)) throw new TypeError("Illegal invocation");
      // Read once, like a WebIDL dictionary conversion.
      const colorAttachments = descriptor?.colorAttachments;
      if (!Array.isArray(colorAttachments)) throw new TypeError("colorAttachments is required");
      return new GPURenderPassEncoder();
    }
    finish(_descriptor) {
      if (!(#brand in this)) throw new TypeError("Illegal invocation");
      return new GPUCommandBuffer();
    }
  }
  class GPUQueue {
    submit(commandBuffers) {
      const buffers = [...commandBuffers];
      if (buffers.some((buffer) => buffer.submitted)) throw new TypeError("command buffer already submitted");
      buffers.forEach((buffer) => {
        buffer.submitted = true;
      });
    }
  }
  const fakeGpu = { GPUCommandEncoder, GPURenderPassEncoder, GPUQueue };
  const queue = new GPUQueue();
  const originalDrawLength = GPURenderPassEncoder.prototype.draw.length;
  const work = { brandErrors: 0, rejectedSubmit: false, observable: null };
  const pass = (encoder) => encoder.beginRenderPass({ colorAttachments: [{ view: new GPUTextureView() }, null] });
  // A finished buffer with one pass and one draw.
  const oneDrawBuffer = () => {
    const encoder = new GPUCommandEncoder();
    const single = pass(encoder);
    single.draw(3);
    single.end();
    return encoder.finish();
  };
  // T1, inside the batch window armed after that frame: two submitted
  // buffers, one abandoned encoder, one finished but never submitted buffer,
  // a rejected second submit and a draw after end().
  function recordBatchWork() {
    const first = new GPUCommandEncoder();
    const scene = pass(first);
    scene.draw(3);
    scene.draw(6);
    scene.drawIndexed(6);
    scene.drawIndexed(6);
    scene.drawIndexed(6);
    scene.drawIndirect({}, 0);
    scene.drawIndexedIndirect({}, 0);
    scene.end();
    scene.draw(3); // after end(): not counted
    const second = pass(first);
    second.draw(3);
    second.end();
    const firstBuffer = first.finish();
    const abandoned = new GPUCommandEncoder();
    const lost = pass(abandoned);
    for (let i = 0; i < 5; i++) lost.draw(3);
    const unsubmitted = new GPUCommandEncoder();
    const kept = pass(unsubmitted);
    for (let i = 0; i < 4; i++) kept.draw(3);
    kept.end();
    unsubmitted.finish();
    const otherBuffer = oneDrawBuffer();
    queue.submit([firstBuffer]);
    queue.submit([otherBuffer]);
    try {
      queue.submit([otherBuffer]);
    } catch {
      work.rejectedSubmit = true;
    }
    // The originals' brand checks still run first.
    for (const call of [
      () => GPUCommandEncoder.prototype.beginRenderPass.call({}, { colorAttachments: [] }),
      () => GPUCommandEncoder.prototype.finish.call({}),
      () => GPURenderPassEncoder.prototype.draw.call({}, 3),
    ]) {
      try {
        call();
      } catch (error) {
        if (error instanceof TypeError) work.brandErrors++;
      }
    }
  }
  // T6: outside the completed batch, counted in the totals only. One buffer
  // through a plain array (counted), then the observable-argument
  // regressions: the wrappers must not read a game-supplied descriptor or
  // re-read a submit sequence, so a one-shot argument behaves exactly as
  // without counters and its submission only increments uncounted_submits.
  function recordLaterWork() {
    const encoder = new GPUCommandEncoder();
    // Its getter throws on a second read: only the original may read it.
    let descriptorReads = 0;
    const oneShotDescriptor = {
      get colorAttachments() {
        if (++descriptorReads > 1) throw new Error("render pass descriptor read twice");
        return [{ view: new GPUTextureView() }];
      },
    };
    const later = encoder.beginRenderPass(oneShotDescriptor);
    later.draw(3);
    later.end();
    queue.submit([encoder.finish()]);
    // A one-shot iterable: a second iteration would throw.
    const generated = oneDrawBuffer();
    let generatorRuns = 0;
    queue.submit({
      *[Symbol.iterator]() {
        if (++generatorRuns > 1) throw new Error("submit sequence iterated twice");
        yield generated;
      },
    });
    // A Proxy whose traps throw once its buffer is submitted.
    const proxied = oneDrawBuffer();
    let proxyTrapsAfterSubmit = 0;
    const proxy = new Proxy([proxied], {
      get(target, key, receiver) {
        if (proxied.submitted) {
          proxyTrapsAfterSubmit++;
          throw new Error("submit sequence re-read after submission");
        }
        return Reflect.get(target, key, receiver);
      },
    });
    // An index getter that yields its buffer once.
    const accessed = oneDrawBuffer();
    let indexReads = 0;
    const accessorArray = [];
    Object.defineProperty(accessorArray, 0, {
      enumerable: true,
      get() {
        if (++indexReads > 1) throw new Error("submit element read twice");
        return accessed;
      },
    });
    const errors = [];
    const submitCatching = (sequence) => {
      try {
        queue.submit(sequence);
      } catch (error) {
        errors.push(String(error));
      }
    };
    submitCatching(proxy);
    submitCatching(accessorArray);
    // Patched array iteration: a plain array whose iteration runs page code
    // (only the fake queue's own conversion may run it).
    const patched = oneDrawBuffer();
    const values = Array.prototype[Symbol.iterator];
    let patchedIterations = 0;
    Array.prototype[Symbol.iterator] = function () {
      patchedIterations++;
      return values.call(this);
    };
    submitCatching([patched]);
    Array.prototype[Symbol.iterator] = values;
    work.observable = {
      descriptorReads,
      generatorRuns,
      proxyTrapsAfterSubmit,
      indexReads,
      patchedIterations,
      errors,
      submitted: [generated, proxied, accessed, patched].every((buffer) => buffer.submitted),
    };
  }
  let handleB;
  requestAnimationFrame((timestamp) => {
    recordBatchWork();
    state.log.push("first");
    state.firstTimestamp = timestamp;
    state.atFirst = snapshot();
    requestAnimationFrame(resumed("A"));
    handleB = requestAnimationFrame(() => state.log.push("B"));
    requestAnimationFrame(resumed("C"));
  });

  globalThis.gpuPacingTestState = {
    fakeGpu,
    // Between T2 and T3, while the rAF queue is deferred.
    duringDeferral() {
      state.loggedDuringDeferral = state.log.slice();
      cancelAnimationFrame(handleB);
      requestAnimationFrame(resumed("D"));
    },
    // Before T6.
    errorPhase() {
      requestAnimationFrame(() => {
        recordLaterWork();
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
      expect(afterFirstDeferred, "after T2", {
        ticks: 2, frames: 1, deferrals: 1, deferred_ticks: 1, polls: 1, saturated: true, active: true,
        completed_batches: 0, slowest_batch_submits: null, slowest_batch_frames: null,
      });
      // The completed batch is described by what was armed at T1: the
      // simulated 8 submits, the one page frame (T1) and no texture writes.
      expect(afterResume, "after T5", {
        ticks: 5, frames: 2, deferrals: 1, deferred_ticks: 3, polls: 4, max_deferred_run: 3,
        poll_errors: 0, saturated: false, disabled: false, script_left: 0,
        completed_batches: 1, slowest_batch_submits: 8, slowest_batch_frames: 1, slowest_batch_texture_bytes: 0,
        texture_writes: 0, copy_external_calls: 0,
      });
      if (!(afterResume.max_completed_batch_ms >= 0)) fail("max_completed_batch_ms not recorded");
      // Render work, at submitted command-buffer granularity.
      const batchWork = { render_command_buffers: 2, render_passes: 3, draws: 4, draw_indexed: 3, draw_indirect: 2 };
      if (afterFirstDeferred.slowest_batch_render_work !== null) fail("render work reported before the batch completed");
      expect(afterResume.slowest_batch_render_work ?? {}, "slowest batch render work", batchWork);
      const report = afterDisabled.render_work;
      expect(report?.submitted ?? {}, "submitted render work", {
        ...batchWork, render_command_buffers: 3, render_passes: 4, draws: 5,
      });
      expect(report.max_tick, "render work per tick", batchWork);
      // generator, Proxy, accessor index and patched iteration.
      if (report.uncounted_submits !== 4) fail(`uncounted_submits is ${report.uncounted_submits}, expected 4`);
      expect(work.observable ?? {}, "observable arguments", {
        descriptorReads: 1, generatorRuns: 1, proxyTrapsAfterSubmit: 0, indexReads: 1, patchedIterations: 1, submitted: true,
      });
      if (work.observable.errors.length !== 0) fail(`counters changed submit behaviour: ${work.observable.errors.join("; ")}`);
      if (!work.rejectedSubmit) fail("the fake queue accepted a buffer submitted twice");
      if (work.brandErrors !== 3) fail(`wrapped methods skipped brand checks (${work.brandErrors} of 3 TypeErrors)`);
      const drawDescriptor = Object.getOwnPropertyDescriptor(GPURenderPassEncoder.prototype, "draw");
      if (drawDescriptor.enumerable || !drawDescriptor.writable || drawDescriptor.value.name !== "draw" || drawDescriptor.value.length !== originalDrawLength) {
        fail("wrapped draw changed its property attributes, name or length");
      }
      if (Object.getPrototypeOf(new GPUCommandEncoder().beginRenderPass({ colorAttachments: [] })) !== GPURenderPassEncoder.prototype) {
        fail("beginRenderPass result lost its prototype");
      }
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
