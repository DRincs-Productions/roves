// Verifies the trusted input Rust dispatched; see events_test.js.
(() => {
  "use strict";
  const fail = (message) => {
    throw new Error(`${message}: ${JSON.stringify(receivedInput)} | fired: ${globalThis.__rovesFireLog?.join(" ; ")} | listener errors: ${globalThis.__rovesListenerErrors?.join(" / ")}`);
  };
  const mouse = receivedInput.filter((event) => "mouse" in event);
  const keys = receivedInput.filter((event) => "keyboard" in event);
  if (mouse.map((event) => event.type).join() !== "pointerdown,mousedown,pointerup,mouseup,click") {
    fail("pointer/mouse sequence");
  }
  for (const event of mouse) {
    if (!event.trusted || !event.mouse) fail(`${event.type} must be a trusted MouseEvent`);
    if (event.type.startsWith("pointer") !== event.pointer) fail(`${event.type} PointerEvent class`);
    const inside = event.x >= buttonRect.left && event.x <= buttonRect.right &&
      event.y >= buttonRect.top && event.y <= buttonRect.bottom;
    if (!inside) fail(`${event.type} coordinates must fall inside the button rect ${JSON.stringify(buttonRect)}`);
    const pressed = event.type === "pointerdown" || event.type === "mousedown";
    if (event.buttons !== (pressed ? 1 : 0)) fail(`${event.type} buttons`);
  }
  if (keys.map((event) => event.type).join() !== "keydown,keyup") fail("keyboard sequence");
  for (const event of keys) {
    if (!event.trusted || !event.keyboard || event.key !== "Enter" || event.code !== "Enter") {
      fail(`${event.type} must be a trusted KeyboardEvent for Enter`);
    }
    if (!event.onFocused) fail(`${event.type} must target the focused element`);
  }
})();
