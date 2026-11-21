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

  // A throwing callback is reported as an error event and does not stop others.
  // Other errors are collected so frame_check.js reports them by message.
  let reported = false;
  const unexpected = [];
  addEventListener("error", (event) => {
    if (event.error?.message === "boom") {
      reported = true;
    } else {
      unexpected.push(String(event.error?.message ?? event.message));
    }
    event.preventDefault();
  });
  requestAnimationFrame(() => {
    throw new Error("boom");
  });
  requestAnimationFrame(() => log.push("after boom"));

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
    frames, first, second, nested, reported, unexpected, timerFired, idleDeadline, idleHandlesValid, log,
  });
})();
