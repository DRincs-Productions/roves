// Verifies the trusted input Rust dispatched; see events_test.js.
(() => {
  "use strict";
  const fail = (message) => {
    // Diagnostics: parent chain identity and an untrusted JS-dispatched keydown.
    const chain = [];
    for (let node = document.getElementById("button"); node !== null; node = node.parentNode) {
      chain.push(node === document ? "#document(global)" : node.nodeType === 9 ? "#document(other)" : node.localName);
    }
    const phases = [];
    const probe = (event) => phases.push(`${event.eventPhase}:${event.bubbles}`);
    document.addEventListener("keydown", probe);
    document.getElementById("button").dispatchEvent(new KeyboardEvent("keydown", { bubbles: true }));
    document.removeEventListener("keydown", probe);
    // Where does bubbling stop? Path seen at the target and phases at ancestors.
    const seen = [];
    const name = (node) => (node === document ? "#document" : node === globalThis ? "#window" : node.localName);
    const button = document.getElementById("button");
    const atTarget = (event) => seen.push(`path=${event.composedPath().map(name).join(">")}`);
    button.addEventListener("keydown", atTarget);
    const watchers = [];
    for (const node of [button.parentNode, document.body, document.documentElement, document]) {
      for (const capture of [true, false]) {
        const watcher = (event) => seen.push(`${name(node)}:${capture ? "capture" : "bubble"}:${event.eventPhase}`);
        node.addEventListener("keydown", watcher, capture);
        watchers.push([node, watcher, capture]);
      }
    }
    button.dispatchEvent(new Event("keydown", { bubbles: true }));
    seen.push("--- KeyboardEvent:");
    button.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "x" }));
    seen.push("--- FocusEvent focusin:");
    for (const [node, watcher, capture] of watchers) node.addEventListener("focusin", watcher, capture);
    button.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    for (const [node, watcher, capture] of watchers) node.removeEventListener("focusin", watcher, capture);
    button.removeEventListener("keydown", atTarget);
    for (const [node, watcher, capture] of watchers) node.removeEventListener("keydown", watcher, capture);
    const plain = [];
    const probePlain = (event) => plain.push(`${event.eventPhase}`);
    document.addEventListener("keydown", probePlain);
    document.getElementById("button").dispatchEvent(new Event("keydown", { bubbles: true }));
    document.removeEventListener("keydown", probePlain);
    throw new Error(`${message}: ${JSON.stringify(receivedInput)} | fired: ${globalThis.__rovesFireLog?.join(" ; ")} | listener errors: ${globalThis.__rovesListenerErrors?.join(" / ")} | chain: ${chain.join(">")} | js KeyboardEvent at document: ${phases.join()} | js Event at document: ${plain.join()} | dispatch trace: ${seen.join(" ; ")}`);
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
