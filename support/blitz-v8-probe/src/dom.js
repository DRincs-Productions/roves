// Minimal DOM wrapper layer over one Blitz document (probe only).
// Each Blitz node id maps to exactly one wrapper, so repeated lookups return the
// same object (===). The cache is strong: Blitz node ids are never dropped by
// this probe (see op_blitz_set_text_content), so a wrapper cannot alias a
// different node. A production version needs a GC-aware lifecycle instead.
((globalThis) => {
  "use strict";
  const { op_blitz_get_element_by_id, op_blitz_text_content, op_blitz_set_text_content } =
    globalThis.Deno.core.ops;

  const wrappers = new Map();
  let constructing = false;

  class Node {
    #handle;
    constructor(handle) {
      if (!constructing) throw new TypeError("Illegal constructor");
      this.#handle = handle;
    }
    get textContent() {
      return op_blitz_text_content(this.#handle);
    }
    set textContent(value) {
      op_blitz_set_text_content(this.#handle, value === null ? "" : String(value));
    }
  }
  class Element extends Node {}

  function wrap(handle) {
    let wrapper = wrappers.get(handle);
    if (wrapper === undefined) {
      constructing = true;
      try {
        wrapper = new Element(handle);
      } finally {
        constructing = false;
      }
      wrappers.set(handle, wrapper);
    }
    return wrapper;
  }

  class Document {
    getElementById(id) {
      const handle = op_blitz_get_element_by_id(String(id));
      return handle < 0 ? null : wrap(handle);
    }
  }

  Object.defineProperties(globalThis, {
    Node: { value: Node, writable: true, configurable: true },
    Element: { value: Element, writable: true, configurable: true },
    document: { value: new Document(), enumerable: true, configurable: true },
  });
})(globalThis);
