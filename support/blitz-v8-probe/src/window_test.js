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
      if (type === "click") click = { x: event.clientX, y: event.clientY, mouse: event instanceof MouseEvent };
    });
  }
  // Centre of the button in CSS pixels; the harness converts it to screen pixels.
  globalThis.windowTarget = () => {
    const rect = button.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  };
  // "waiting" until a trusted click arrives, then "passed" or a failure detail.
  globalThis.windowStatus = () => {
    if (click === null) return "waiting";
    const rect = button.getBoundingClientRect();
    const inside = click.x >= rect.left && click.x <= rect.right && click.y >= rect.top && click.y <= rect.bottom;
    const expected = ["pointerdown", "mousedown", "pointerup", "mouseup", "click"];
    let next = 0;
    for (const type of sequence) if (type === expected[next]) next++;
    if (inside && click.mouse && next === expected.length) return "passed";
    return `failed: ${JSON.stringify({ sequence, click, rect })}`;
  };
})();
