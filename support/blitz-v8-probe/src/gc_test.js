// GC lifecycle scenario. Rust counts live Blitz nodes before and after, forces
// collections through the embedder hook and runs gc_check.js.
(() => {
  "use strict";
  // 500 detached element + text pairs (1000 Blitz nodes) that nothing keeps.
  (() => {
    for (let index = 0; index < 500; index++) {
      const element = document.createElement("div");
      element.appendChild(document.createTextNode(`garbage ${index}`));
    }
  })();

  // A detached node the game still references must survive with its expando.
  const kept = document.createElement("div");
  kept.textContent = "kept";
  kept.expando = "kept";
  globalThis.gcKept = kept;

  // A connected node whose wrapper the game dropped: the node lives with the
  // document and its wrapper (with the expando) must be the one returned later.
  (() => {
    const attached = document.createElement("div");
    attached.setAttribute("id", "gc-attached");
    attached.expando = "attached";
    document.getElementById("app").appendChild(attached);
  })();
})();
