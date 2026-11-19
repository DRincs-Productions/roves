// Guest-side frame loop scenario, checked by frame_check.js after the frames ran.
(() => {
  "use strict";
  const app = document.getElementById("app");
  const box = document.createElement("div");
  box.setAttribute("id", "box");
  box.style.backgroundColor = "rgb(40, 160, 90)";
  box.style.height = "20px";
  box.style.width = "10px";
  app.appendChild(box);

  const log = [];
  let frames = 0;
  let lastTimestamp = -1;
  const step = (timestamp) => {
    if (!(timestamp > lastTimestamp)) throw new Error("rAF timestamps must increase");
    if (frames > 0 && !log.includes(`micro ${frames}`)) {
      throw new Error("microtasks must drain before the next frame");
    }
    lastTimestamp = timestamp;
    frames++;
    box.style.width = `${10 + frames * 10}px`;
    const queued = frames;
    Promise.resolve().then(() => log.push(`micro ${queued}`));
    if (frames < 5) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);

  // Same frame: one timestamp; a handle cancelled by an earlier callback is
  // skipped; a callback requested during the frame runs in the next one.
  let first;
  let second;
  let nested;
  requestAnimationFrame((timestamp) => {
    first = timestamp;
    requestAnimationFrame((next) => {
      nested = next;
    });
    cancelAnimationFrame(cancelled);
  });
  const cancelled = requestAnimationFrame(() => log.push("cancelled ran"));
  requestAnimationFrame((timestamp) => {
    second = timestamp;
  });

  // Errors thrown by callbacks follow the window error contract (Chromium
  // ExecutionContext::DispatchErrorEvent): a cancelable ErrorEvent reaches the
  // window listeners and window.onerror(message, filename, lineno, colno, error),
  // whose `true` return cancels it. An uncancelled error, or one thrown by an
  // error listener (not dispatched again), is only logged ("Uncaught ...") and
  // stops neither the later callbacks of the frame nor the next frames (the step
  // chain above). A cancelled callback that would throw is never reported.
  // Other errors are collected so frame_check.js reports them by message.
  const errors = [];
  const unexpected = [];
  const expected = new Set(["boom", "uncancelled", "via onerror", "recursive"]);
  addEventListener("error", (event) => {
    const message = event.error?.message;
    if (!expected.has(message)) {
      unexpected.push(String(message ?? event.message));
      event.preventDefault();
      return;
    }
    errors.push(`listener ${message} ${event.cancelable}`);
    if (message === "boom") event.preventDefault();
    if (message === "recursive") throw new Error("thrown by an error listener");
  });
  window.onerror = (message, filename, lineno, colno, error) => {
    const args = typeof message === "string" && message.includes(error?.message)
      && typeof filename === "string" && lineno > 0 && colno > 0;
    errors.push(`onerror ${error?.message} ${args}`);
    return error?.message === "via onerror";
  };
  addEventListener("error", (event) => {
    const message = event.error?.message;
    if (expected.has(message)) errors.push(`after ${message} ${event.defaultPrevented}`);
  });
  requestAnimationFrame(() => {
    throw new Error("boom");
  });
  requestAnimationFrame(() => log.push("after boom"));
  requestAnimationFrame(() => {
    throw new Error("uncancelled");
  });
  requestAnimationFrame(() => log.push("after uncancelled"));
  requestAnimationFrame(() => {
    throw new Error("via onerror");
  });
  requestAnimationFrame(() => {
    throw new Error("recursive");
  });
  requestAnimationFrame(() => log.push("after recursive"));
  const cancelledThrow = requestAnimationFrame(() => {
    throw new Error("cancelled callback threw");
  });
  cancelAnimationFrame(cancelledThrow);

  let timerFired = false;
  setTimeout(() => {
    timerFired = true;
  }, 0);

  // requestIdleCallback/cancelIdleCallback through every receiver a game uses
  // (bare, window., globalThis.) must not throw "Illegal invocation" (deno_web
  // timers check `this`; CI 42907305234), the idle callback must run with an
  // IdleDeadline, and a cancelled handle must never run.
  let idleDeadline = null;
  const idleHandles = [
    requestIdleCallback(() => log.push("idle bare")),
    window.requestIdleCallback((deadline) => {
      idleDeadline = { didTimeout: deadline.didTimeout, remaining: typeof deadline.timeRemaining() };
      log.push("idle window");
    }, { timeout: 50 }),
    globalThis.requestIdleCallback(() => log.push("idle global")),
  ];
  const cancelledIdle = window.requestIdleCallback(() => log.push("idle cancelled ran"));
  window.cancelIdleCallback(cancelledIdle);
  const cancelledIdleBare = requestIdleCallback(() => log.push("idle cancelled bare ran"));
  cancelIdleCallback(cancelledIdleBare);
  const idleHandlesValid = idleHandles.every((handle) => typeof handle === "number" && handle > 0);

  globalThis.frameResults = () => ({
    frames, first, second, nested, errors, unexpected, timerFired, idleDeadline, idleHandlesValid, log,
  });
})();
