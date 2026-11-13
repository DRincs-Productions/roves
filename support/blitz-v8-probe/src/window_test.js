// --window mode: waits for a real OS click on #button, routed by winit ->
// blitz-shell -> Blitz EventDriver (hit-test) -> trusted JS dispatch.
(() => {
  "use strict";
  const button = document.getElementById("button");
  const sequence = [];
  let click = null;
  for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
    button.addEventListener(type, (event) => {
      if (!event.isTrusted) return;
      sequence.push(type);
      if (type === "click") {
        click = { x: event.clientX, y: event.clientY, mouse: event instanceof MouseEvent };
        // A timer started by the real click must fire while the window runs
        // (the window loop has to drive Tokio's timer driver).
        const started = performance.now();
        setTimeout(() => {
          timerDelay = performance.now() - started;
        }, TIMER_MS);
      }
    });
  }
  const TIMER_MS = 250;
  let timerDelay = null;
  // Input-free counterpart: started by the first window frame's status check,
  // so a run that times out without a click still shows whether the window
  // loop fires timers ("waiting: click") or starves them ("waiting: window timer").
  let windowTimerFired = null;
  // Centre of the button in CSS pixels; the harness converts it to screen pixels.
  globalThis.windowTarget = () => {
    const rect = button.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  };
  // "waiting" until a trusted click arrives and the timer it started fires,
  // then "passed" or a failure detail (the Rust side times out a timer that
  // never fires).
  globalThis.windowStatus = () => {
    if (windowTimerFired === null) {
      windowTimerFired = false;
      setTimeout(() => {
        windowTimerFired = true;
      }, TIMER_MS);
    }
    if (!windowTimerFired) return "waiting: window timer";
    if (click === null) return "waiting: click";
    if (timerDelay === null) return "waiting: click timer";
    // Timers never fire early; a timer clamped to whole milliseconds may
    // report up to 1 ms less through performance.now().
    if (timerDelay < TIMER_MS - 1) return `failed: timer fired after ${timerDelay} ms`;
    const rect = button.getBoundingClientRect();
    const inside = click.x >= rect.left && click.x <= rect.right && click.y >= rect.top && click.y <= rect.bottom;
    const expected = ["pointerdown", "mousedown", "pointerup", "mouseup", "click"];
    let next = 0;
    for (const type of sequence) if (type === expected[next]) next++;
    if (inside && click.mouse && next === expected.length) return "passed";
    return `failed: ${JSON.stringify({ sequence, click, rect })}`;
  };
})();
