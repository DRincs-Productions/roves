// Guest-side checks: uses only the DOM surface; Deno and __bootstrap are gone.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const throwsDom = (fn, name, message) => {
    try {
      fn();
    } catch (error) {
      check(error instanceof DOMException && error.name === name, `${message}: got ${error}`);
      return;
    }
    throw new Error(`${message}: no exception`);
  };

  check(typeof globalThis.Deno === "undefined", "Deno must not be visible to the guest");
  check(typeof globalThis.__bootstrap === "undefined", "__bootstrap must not be visible to the guest");

  // Identity and text.
  const app = document.getElementById("app");
  check(app instanceof Element && app instanceof EventTarget, "getElementById must return an Element/EventTarget");
  check(document.getElementById("app") === app, "same node must return the same wrapper");
  check(document.getElementById("missing") === null, "unknown id must return null");
  check(document.getElementById("") === null, "empty id must return null");
  check(app.textContent === "Blitz layout fixture", "textContent must read the Blitz text");
  check(app.tagName === "MAIN" && app.nodeType === 1, "tagName/nodeType must come from Blitz");
  check(app.parentNode.parentNode.parentNode === document, "parentNode chain must reach the document wrapper");
  check(document.nodeType === 9 && document.parentNode === null, "document must be the tree root");

  app.expando = "kept";
  app.textContent = 42;
  check(app.textContent === "42", "textContent setter must coerce to string");
  app.textContent = null;
  check(app.textContent === "", "textContent = null must clear the element");
  app.textContent = "mutated from V8";
  const again = document.getElementById("app");
  check(again === app && again.expando === "kept", "wrapper identity must survive mutation");

  // Tree mutation and attributes.
  const button = document.createElement("BUTTON");
  check(button.localName === "button" && button.parentNode === null, "createElement must lowercase and be detached");
  button.setAttribute("id", "button");
  check(button.getAttribute("ID") === "button" && button.id === "button", "attributes must round-trip");
  check(button.getAttribute("missing") === null, "missing attribute must be null");
  check(document.getElementById("button") === null, "detached nodes must not be found by id");
  const label = document.createTextNode("press");
  check(button.appendChild(label) === label && label.parentNode === button, "appendChild must attach a text node");
  check(app.appendChild(button) === button, "appendChild must return the child");
  check(document.getElementById("button") === button, "inserted node must keep its wrapper");
  check(app.textContent === "mutated from V8press", "textContent must include descendants");

  const holder = document.createElement("div");
  holder.appendChild(label);
  check(label.parentNode === holder && button.textContent === "", "appendChild must move an attached node");
  check(holder.removeChild(label) === label && label.parentNode === null, "removeChild must detach");
  button.textContent = "press";

  throwsDom(() => button.appendChild(app), "HierarchyRequestError", "ancestor insertion must be rejected");
  throwsDom(() => app.appendChild(app), "HierarchyRequestError", "self insertion must be rejected");
  throwsDom(() => app.appendChild(document), "HierarchyRequestError", "document insertion must be rejected");
  throwsDom(() => holder.removeChild(button), "NotFoundError", "removing a non-child must be rejected");
  throwsDom(() => document.createElement("a b"), "InvalidCharacterError", "invalid element names must be rejected");
  let typeError = false;
  try {
    app.appendChild({});
  } catch (error) {
    typeError = error instanceof TypeError;
  }
  check(typeError, "non-node arguments must throw TypeError");
  let illegal = false;
  try {
    new Element();
  } catch (error) {
    illegal = error instanceof TypeError;
  }
  check(illegal, "wrappers must not be constructible by guest code");

  // Events: listeners in DOM order for the embedder's trusted click.
  const order = [];
  document.addEventListener("click", (event) => {
    if (event.isTrusted && event.eventPhase === Event.CAPTURING_PHASE) order.push("capture");
  }, { capture: true });
  button.addEventListener("click", (event) => {
    if (event.isTrusted && event.target === button) order.push("target");
  });
  app.addEventListener("click", (event) => {
    if (event.isTrusted && event.eventPhase === Event.BUBBLING_PHASE) {
      order.push("bubble");
      button.textContent = "clicked: " + order.join(",");
    }
  });

  // Ancestor listeners must run even when the target has no listener for the
  // type (deno_web's EventTarget.dispatchEvent skips them in that case).
  let reachedAncestor = false;
  const onAncestor = () => {
    reachedAncestor = true;
  };
  document.addEventListener("probe-bubble", onAncestor);
  button.dispatchEvent(new Event("probe-bubble", { bubbles: true }));
  document.removeEventListener("probe-bubble", onAncestor);
  check(reachedAncestor, "an event must bubble to ancestors when the target has no listener");

  // A guest-dispatched event is untrusted, so the listeners above ignore it.
  check(button.dispatchEvent(new Event("click", { bubbles: true })) === true, "guest dispatch must work");
  check(order.length === 0, "untrusted events must be distinguishable");
})();
