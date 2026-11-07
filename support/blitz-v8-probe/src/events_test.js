// Geometry, matchMedia, focus and UI event classes. Registers listeners for the
// trusted pointer/mouse/keyboard events Rust sends next; events_check.js verifies.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };

  // Geometry (forces style + layout like a browser).
  check(innerWidth === 640 && innerHeight === 360, `viewport: ${innerWidth}x${innerHeight}`);
  const app = document.getElementById("app");
  const appRect = app.getBoundingClientRect();
  check(appRect instanceof DOMRect, "getBoundingClientRect returns a DOMRect");
  check(
    appRect.x === 0 && appRect.y === 0 && appRect.width === 344 && appRect.height === 204,
    `#app is 320x180 plus 12px padding at the origin: ${JSON.stringify(appRect)}`,
  );
  check(appRect.right === 344 && appRect.bottom === 204, "DOMRect right/bottom");
  check(app.offsetWidth === 344 && app.offsetHeight === 204 && app.offsetLeft === 0 && app.offsetTop === 0, "offset*");
  const button = document.getElementById("button");
  const buttonRect = button.getBoundingClientRect();
  check(
    buttonRect.left === 12 && buttonRect.top > 12 && buttonRect.width >= 120 && buttonRect.height >= 40,
    `#button sits inside #app's padding below the text line: ${JSON.stringify(buttonRect)}`,
  );
  globalThis.buttonRect = buttonRect;
  const hidden = document.getElementById("api").getBoundingClientRect();
  check(hidden.width === 0 && hidden.height === 0, "display:none has an empty rect");

  // matchMedia against the 640x360 light viewport.
  check(matchMedia("(min-width: 600px)").matches && !matchMedia("(max-width: 600px)").matches, "width features");
  check(matchMedia("(orientation: landscape)").matches, "orientation");
  check(!matchMedia("print").matches && matchMedia("not print").matches, "media types and not");
  check(matchMedia("screen and (min-width: 100px) and (max-height: 400px)").matches, "and");
  check(matchMedia("(max-width: 10px), (min-height: 300px)").matches, "comma list");
  check(!matchMedia("(prefers-reduced-motion: reduce)").matches, "reduced motion off");
  check(matchMedia("(prefers-color-scheme: light)").matches, "light color scheme");
  check(!matchMedia("(unknown-feature: 1)").matches, "unknown features do not match");
  const list = matchMedia("(min-width: 600px)");
  check(list instanceof MediaQueryList && list instanceof EventTarget && list.media === "(min-width: 600px)", "MediaQueryList");

  // Focus: only focusable elements, trusted focus/blur/focusin/focusout in order.
  const focusLog = [];
  for (const type of ["focus", "blur", "focusin", "focusout"]) {
    app.addEventListener(type, (event) => {
      focusLog.push(`${type}:${event.target.localName}:${event.isTrusted}:${event instanceof FocusEvent}`);
    }, true);
  }
  // Diagnostics for the focus sequence: target-phase listeners, capture listeners
  // registered with an options object, and errors thrown by listeners.
  const focusDiagnostics = [];
  const listenerErrors = [];
  globalThis.__rovesListenerErrors = listenerErrors;
  addEventListener("error", (event) => {
    listenerErrors.push(String(event.error?.message ?? event.message));
    event.preventDefault();
  });
  for (const type of ["focus", "blur", "focusin", "focusout"]) {
    button.addEventListener(type, () => focusDiagnostics.push(`target:${type}`));
    app.addEventListener(type, () => focusDiagnostics.push(`capture-options:${type}`), { capture: true });
  }
  check(document.activeElement === document.body, `nothing focused: activeElement is body, got ${document.activeElement?.localName}`);
  app.focus();
  check(document.activeElement === document.body, "a non-focusable element does not take focus");
  button.focus();
  check(document.activeElement === button, "a button takes focus");
  button.blur();
  check(document.activeElement === document.body, "blur returns focus to body");
  button.focus();
  check(
    focusLog.join() === [
      "focus:button:true:true",
      "focusin:button:true:true",
      "blur:button:true:true",
      "focusout:button:true:true",
      "focus:button:true:true",
      "focusin:button:true:true",
    ].join(),
    `focus event sequence: ${focusLog.join()} | diagnostics: ${focusDiagnostics.join()} | listener errors: ${listenerErrors.join(" / ")} | fired: ${globalThis.__rovesFireLog?.join()}`,
  );

  // Guest-constructed events are untrusted and carry their init values.
  const synthetic = new MouseEvent("click", { clientX: 5, ctrlKey: true, bubbles: true });
  check(
    synthetic.clientX === 5 && synthetic.pageX === 5 && synthetic.ctrlKey && synthetic.getModifierState("Control") &&
      !synthetic.isTrusted && synthetic.bubbles && synthetic instanceof UIEvent && synthetic instanceof Event,
    "MouseEvent init values",
  );
  const key = new KeyboardEvent("keydown", { key: "a", code: "KeyA", repeat: true });
  check(key.key === "a" && key.code === "KeyA" && key.repeat && !key.isTrusted, "KeyboardEvent init values");
  const pointer = new PointerEvent("pointerdown", { pointerId: 7, pointerType: "pen", pressure: 0.5 });
  check(pointer.pointerId === 7 && pointer.pointerType === "pen" && pointer.pressure === 0.5 && pointer instanceof MouseEvent, "PointerEvent");

  // Listeners for the trusted input Rust sends.
  const received = [];
  globalThis.receivedInput = received;
  for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
    button.addEventListener(type, (event) => {
      received.push({
        type,
        trusted: event.isTrusted,
        pointer: event instanceof PointerEvent,
        mouse: event instanceof MouseEvent,
        x: event.clientX,
        y: event.clientY,
        buttons: event.buttons,
      });
    });
  }
  for (const type of ["keydown", "keyup"]) {
    document.addEventListener(type, (event) => {
      received.push({
        type,
        trusted: event.isTrusted,
        keyboard: event instanceof KeyboardEvent,
        key: event.key,
        code: event.code,
        onFocused: event.target === document.activeElement,
      });
    });
  }
})();
