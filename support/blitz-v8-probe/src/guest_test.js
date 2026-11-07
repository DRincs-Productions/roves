// Guest-side checks: uses only the DOM surface, never Deno.core.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };

  const first = document.getElementById("app");
  check(first instanceof Element, "getElementById must return an Element wrapper");
  check(document.getElementById("app") === first, "same node must return the same wrapper");
  check(document.getElementById("missing") === null, "unknown id must return null");
  check(document.getElementById("") === null, "empty id must return null");
  check(first.textContent === "Blitz layout fixture", "textContent must read the Blitz text");

  first.expando = "kept";
  first.textContent = 42;
  check(first.textContent === "42", "textContent setter must coerce to string");
  first.textContent = null;
  check(first.textContent === "", "textContent = null must clear the element");
  first.textContent = "mutated from V8";

  const again = document.getElementById("app");
  check(again === first && again.expando === "kept", "wrapper identity must survive mutation");
  check(again.textContent === "mutated from V8", "write must be visible through a new lookup");

  let threw = false;
  try {
    new Element(0);
  } catch (error) {
    threw = error instanceof TypeError;
  }
  check(threw, "wrappers must not be constructible by guest code");
})();
