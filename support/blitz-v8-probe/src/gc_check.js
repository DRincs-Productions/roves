// Runs after the forced collections; see gc_test.js.
(() => {
  "use strict";
  const fail = (message) => {
    throw new Error(message);
  };
  if (gcKept.expando !== "kept") fail("a referenced detached wrapper must keep its expando");
  if (gcKept.textContent !== "kept") fail("a referenced detached node must not be freed");
  const attached = document.getElementById("gc-attached");
  if (attached === null) fail("a connected node must not be freed");
  if (attached.expando !== "attached") fail("a connected node's wrapper must survive GC");
  if (attached.parentNode !== document.getElementById("app")) fail("parent identity must hold after GC");
})();
