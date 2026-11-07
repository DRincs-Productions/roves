// Minimal DOM wrapper layer over one Blitz document (probe only).
//
// - Blitz owns the only node tree; wrappers hold a class-private node handle.
// - Each Blitz node id maps to exactly one wrapper (=== identity). The cache is
//   strong because this probe never drops Blitz nodes (Blitz reuses dropped ids);
//   production needs a GC-aware lifecycle instead.
// - Event and EventTarget come from deno_web. Its dispatch builds the event path
//   from `parentNode` of objects that have a `nodeType`, so capture/target/bubble
//   follow the Blitz tree without event code of our own.
//
// Evaluates to the trusted-event dispatcher, which Rust keeps; nothing privileged
// stays reachable from the guest global.
(() => {
  "use strict";
  const core = globalThis.Deno.core;
  const {
    op_blitz_get_element_by_id,
    op_blitz_node_type,
    op_blitz_parent,
    op_blitz_local_name,
    op_blitz_text_content,
    op_blitz_set_text_content,
    op_blitz_create_element,
    op_blitz_create_text_node,
    op_blitz_append_child,
    op_blitz_remove_child,
    op_blitz_get_attribute,
    op_blitz_set_attribute,
  } = core.ops;
  const { Event, EventTarget, CustomEvent, setIsTrusted } = core.loadExtScript(
    "ext:deno_web/02_event.js",
  );
  const { DOMException } = core.loadExtScript("ext:deno_web/01_dom_exception.js");

  const ELEMENT_NODE = 1;
  const TEXT_NODE = 3;
  const DOCUMENT_NODE = 9;
  const DOCUMENT_ID = 0;
  // XML Name production is broader; this ASCII subset covers HTML elements and
  // custom elements and rejects everything the probe does not handle.
  const VALID_NAME = /^[A-Za-z][A-Za-z0-9_.-]*$/;

  const wrappers = new Map();
  const handles = new WeakMap();
  let constructing = false;

  function checkStatus(status) {
    if (status === 1) {
      throw new DOMException("The new child would create an invalid tree", "HierarchyRequestError");
    }
    if (status === 2) {
      throw new DOMException("The node is not a child of this node", "NotFoundError");
    }
  }

  function handleOf(node, method) {
    const handle = handles.get(node);
    if (handle === undefined) {
      throw new TypeError(`Failed to execute '${method}': parameter is not of type 'Node'.`);
    }
    return handle;
  }

  class Node extends EventTarget {
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
      super();
    }
    get nodeType() {
      return op_blitz_node_type(handleOf(this, "nodeType"));
    }
    get parentNode() {
      const parent = op_blitz_parent(handleOf(this, "parentNode"));
      return parent < 0 ? null : wrap(parent);
    }
    // No shadow trees exist yet, so the `composed` option has no effect.
    getRootNode() {
      let node = this;
      for (let parent = node.parentNode; parent !== null; parent = node.parentNode) node = parent;
      return node;
    }
    get textContent() {
      return this.nodeType === DOCUMENT_NODE ? null : op_blitz_text_content(handleOf(this, "textContent"));
    }
    set textContent(value) {
      if (this.nodeType === DOCUMENT_NODE) return;
      op_blitz_set_text_content(handleOf(this, "textContent"), value === null ? "" : String(value));
    }
    appendChild(child) {
      checkStatus(op_blitz_append_child(handleOf(this, "appendChild"), handleOf(child, "appendChild")));
      return child;
    }
    removeChild(child) {
      checkStatus(op_blitz_remove_child(handleOf(this, "removeChild"), handleOf(child, "removeChild")));
      return child;
    }
  }

  class Element extends Node {
    get localName() {
      return op_blitz_local_name(handleOf(this, "localName"));
    }
    get tagName() {
      return this.localName.toUpperCase();
    }
    get id() {
      return this.getAttribute("id") ?? "";
    }
    set id(value) {
      this.setAttribute("id", value);
    }
    getAttribute(name) {
      return op_blitz_get_attribute(handleOf(this, "getAttribute"), String(name).toLowerCase());
    }
    setAttribute(name, value) {
      name = String(name);
      if (!VALID_NAME.test(name)) {
        throw new DOMException(`'${name}' is not a valid attribute name.`, "InvalidCharacterError");
      }
      op_blitz_set_attribute(handleOf(this, "setAttribute"), name.toLowerCase(), String(value));
    }
  }

  class Text extends Node {}

  class Document extends Node {
    getElementById(id) {
      const handle = op_blitz_get_element_by_id(String(id));
      return handle < 0 ? null : wrap(handle);
    }
    createElement(name) {
      name = String(name);
      if (!VALID_NAME.test(name)) {
        throw new DOMException(`'${name}' is not a valid element name.`, "InvalidCharacterError");
      }
      return wrap(op_blitz_create_element(name.toLowerCase()));
    }
    createTextNode(data) {
      return wrap(op_blitz_create_text_node(String(data)));
    }
  }

  function wrap(handle) {
    let wrapper = wrappers.get(handle);
    if (wrapper !== undefined) return wrapper;
    const type = op_blitz_node_type(handle);
    const Class = handle === DOCUMENT_ID
      ? Document
      : type === ELEMENT_NODE
      ? Element
      : type === TEXT_NODE
      ? Text
      : Node;
    constructing = true;
    try {
      wrapper = new Class();
    } finally {
      constructing = false;
    }
    handles.set(wrapper, handle);
    wrappers.set(handle, wrapper);
    return wrapper;
  }

  const document = wrap(DOCUMENT_ID);
  const globals = { Node, Element, Text, Document, Event, CustomEvent, EventTarget, DOMException };
  for (const [name, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
  }
  Object.defineProperty(globalThis, "document", { value: document, enumerable: true, configurable: true });

  // The guest only sees the DOM surface. deno_core keeps its own captured view
  // for lazily loaded extension scripts, so removing these is safe.
  delete globalThis.Deno;
  delete globalThis.__bootstrap;

  // Trusted UI event from the embedder (Blitz hit-test result).
  return function dispatchTrustedEvent(handle, type) {
    const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
    setIsTrusted(event, true);
    return wrap(handle).dispatchEvent(event);
  };
})();
