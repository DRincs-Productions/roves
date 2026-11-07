// Minimal DOM wrapper layer over one Blitz document (probe only).
//
// - Blitz owns the only node tree; wrappers hold a class-private node handle.
// - Each live Blitz node maps to at most one wrapper (=== identity). Wrappers of
//   nodes connected to the document are held strongly (expandos survive, as in
//   browsers); wrappers of detached nodes only weakly. When V8 collects a wrapper,
//   a FinalizationRegistry callback releases its handle and Rust frees the detached
//   Blitz subtree once no wrapper references any node in it.
// - Event and EventTarget come from deno_web. Its dispatch builds the event path
//   from `parentNode` of objects that have a `nodeType`, so capture/target/bubble
//   follow the Blitz tree without event code of our own.
//
// Evaluates to the embedder hooks (trusted-event dispatch, animation frame), which
// Rust keeps; nothing privileged stays reachable from the guest global.
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
    op_blitz_set_style_property,
    op_blitz_namespace,
    op_blitz_computed_style,
    op_blitz_document_root,
    op_blitz_retain,
    op_blitz_release,
    op_blitz_is_connected,
    op_blitz_insert_before,
    op_blitz_child_count,
    op_blitz_child_at,
    op_blitz_sibling,
    op_blitz_query,
    op_blitz_query_result,
    op_blitz_outer_html,
    op_blitz_inner_html,
    op_blitz_set_inner_html,
    op_blitz_remove_attribute,
    op_blitz_measure,
    op_blitz_rect_component,
    op_blitz_viewport_size,
    op_blitz_focus,
    op_blitz_blur,
    op_blitz_focused,
  } = core.ops;
  const {
    Event,
    EventTarget,
    CustomEvent,
    ErrorEvent,
    setIsTrusted,
    setEventTargetData,
    saveGlobalThisReference,
    reportException,
    dispatch,
  } = core.loadExtScript("ext:deno_web/02_event.js");
  const timers = core.loadExtScript("ext:deno_web/02_timers.js");
  const webidl = core.loadExtScript("ext:deno_webidl/00_webidl.js");
  const { performance, setTimeOrigin } = core.loadExtScript("ext:deno_web/15_performance.js");
  setTimeOrigin();
  const HTML_NAMESPACE = "http://www.w3.org/1999/xhtml";
  const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
  const { DOMException } = core.loadExtScript("ext:deno_web/01_dom_exception.js");
  const { URL, URLSearchParams } = core.loadExtScript("ext:deno_web/00_url.js");
  const { structuredClone } = core.loadExtScript("ext:deno_web/02_structured_clone.js");
  const webStorage = core.loadExtScript("ext:deno_webstorage/01_webstorage.js");

  const ELEMENT_NODE = 1;
  const TEXT_NODE = 3;
  const DOCUMENT_NODE = 9;
  // Versioned Blitz node id of the document node, as a number handle.
  const DOCUMENT_ID = op_blitz_document_root();
  // XML Name production is broader; this ASCII subset covers HTML elements and
  // custom elements and rejects everything the probe does not handle.
  const VALID_NAME = /^[A-Za-z][A-Za-z0-9_.-]*$/;

  // handle -> { ref: WeakRef<wrapper>, strong: wrapper | null, token }
  const cache = new Map();
  const handles = new WeakMap();
  const finalizers = new FinalizationRegistry(({ handle, token }) => {
    const entry = cache.get(handle);
    if (entry !== undefined && entry.token === token) cache.delete(handle);
    op_blitz_release(handle);
  });
  function cached(handle) {
    const entry = cache.get(handle);
    return entry === undefined ? undefined : entry.strong ?? entry.ref.deref();
  }
  // Re-evaluates strong vs weak retention for one cached wrapper.
  function retainIfConnected(handle) {
    const entry = cache.get(handle);
    if (entry === undefined) return;
    const wrapper = entry.strong ?? entry.ref.deref();
    if (wrapper === undefined) return;
    entry.strong = handle === DOCUMENT_ID || op_blitz_is_connected(handle) ? wrapper : null;
  }
  // Once per frame: wrappers of nodes that left the document become collectable,
  // wrappers of nodes that joined it become strong.
  function sweepWrappers() {
    for (const handle of cache.keys()) retainIfConnected(handle);
  }
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
    // deno_web's EventTarget.dispatchEvent returns early when the target itself
    // has no listener for the type, which skips every ancestor; correct for
    // Deno's tree-less targets, wrong for DOM nodes. Nodes call deno_web's
    // spec dispatch (the path is built from parentNode) directly.
    dispatchEvent(event) {
      if (!(event instanceof Event)) {
        throw new TypeError("Failed to execute 'dispatchEvent': parameter 1 is not of type 'Event'.");
      }
      if (event.eventPhase !== Event.NONE) {
        throw new DOMException("Invalid event state", "InvalidStateError");
      }
      return dispatch(this, event);
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
      const childHandle = handleOf(child, "appendChild");
      checkStatus(op_blitz_append_child(handleOf(this, "appendChild"), childHandle));
      retainIfConnected(childHandle);
      return child;
    }
    removeChild(child) {
      checkStatus(op_blitz_remove_child(handleOf(this, "removeChild"), handleOf(child, "removeChild")));
      return child;
    }
    insertBefore(node, child) {
      const nodeHandle = handleOf(node, "insertBefore");
      const reference = child === null || child === undefined ? -1 : handleOf(child, "insertBefore");
      checkStatus(op_blitz_insert_before(handleOf(this, "insertBefore"), nodeHandle, reference));
      retainIfConnected(nodeHandle);
      return node;
    }
    get ownerDocument() {
      return this.nodeType === DOCUMENT_NODE ? null : document;
    }
    get isConnected() {
      return op_blitz_is_connected(handleOf(this, "isConnected"));
    }
    get childNodes() {
      const handle = handleOf(this, "childNodes");
      const count = op_blitz_child_count(handle);
      const nodes = [];
      for (let index = 0; index < count; index++) nodes.push(wrap(op_blitz_child_at(handle, index)));
      return createNodeList(nodes);
    }
    hasChildNodes() {
      return op_blitz_child_count(handleOf(this, "hasChildNodes")) > 0;
    }
    get firstChild() {
      return wrapOrNull(op_blitz_child_at(handleOf(this, "firstChild"), 0));
    }
    get lastChild() {
      const handle = handleOf(this, "lastChild");
      const count = op_blitz_child_count(handle);
      return count === 0 ? null : wrap(op_blitz_child_at(handle, count - 1));
    }
    get nextSibling() {
      return wrapOrNull(op_blitz_sibling(handleOf(this, "nextSibling"), true));
    }
    get previousSibling() {
      return wrapOrNull(op_blitz_sibling(handleOf(this, "previousSibling"), false));
    }
    contains(other) {
      if (other === null || other === undefined) return false;
      handleOf(other, "contains");
      for (let node = other; node !== null; node = node.parentNode) if (node === this) return true;
      return false;
    }
    // ParentNode mixin (Element and Document).
    get children() {
      return createNodeList(this.childNodes.filter((node) => node.nodeType === ELEMENT_NODE));
    }
    get firstElementChild() {
      return this.children[0] ?? null;
    }
    querySelector(selectors) {
      const count = query(this, selectors, false);
      return count === 0 ? null : wrap(op_blitz_query_result(0));
    }
    querySelectorAll(selectors) {
      const count = query(this, selectors, true);
      const nodes = [];
      for (let index = 0; index < count; index++) nodes.push(wrap(op_blitz_query_result(index)));
      return createNodeList(nodes);
    }
    append(...nodes) {
      for (const node of nodes) {
        this.appendChild(typeof node === "string" ? document.createTextNode(node) : node);
      }
    }
    // ChildNode mixin.
    remove() {
      const parent = this.parentNode;
      if (parent !== null) parent.removeChild(this);
    }
  }

  // Geometry reads force style + layout in Rust, like browsers.
  class DOMRect {
    constructor(x = 0, y = 0, width = 0, height = 0) {
      Object.assign(this, { x, y, width, height });
    }
    get left() {
      return Math.min(this.x, this.x + this.width);
    }
    get top() {
      return Math.min(this.y, this.y + this.height);
    }
    get right() {
      return Math.max(this.x, this.x + this.width);
    }
    get bottom() {
      return Math.max(this.y, this.y + this.height);
    }
    toJSON() {
      const { x, y, width, height, left, top, right, bottom } = this;
      return { x, y, width, height, left, top, right, bottom };
    }
  }
  function measure(element, offset) {
    if (!op_blitz_measure(handleOf(element, "getBoundingClientRect"), offset)) return new DOMRect();
    return new DOMRect(
      op_blitz_rect_component(0),
      op_blitz_rect_component(1),
      op_blitz_rect_component(2),
      op_blitz_rect_component(3),
    );
  }

  // HTML focusability, reduced to the cases the probe handles.
  const FORM_CONTROLS = ["button", "input", "select", "textarea"];
  function isFocusable(element) {
    const name = element.localName;
    if (FORM_CONTROLS.includes(name)) return !element.hasAttribute("disabled");
    return (name === "a" && element.hasAttribute("href")) || element.hasAttribute("tabindex");
  }
  // Blitz may report the document, <html> or <body> as focused before anything
  // focusable took focus; for the DOM that is "no focused element".
  function focusedElement() {
    const focused = wrapOrNull(op_blitz_focused());
    if (!(focused instanceof Element)) return null;
    const name = focused.localName;
    return name === "html" || name === "body" ? null : focused;
  }

  // UI event classes on top of deno_web's Event. Values come from the init
  // dictionary, with the DOM defaults.
  function defineInitGetters(Class, defaults) {
    for (const [name, fallback] of Object.entries(defaults)) {
      Object.defineProperty(Class.prototype, name, {
        get() {
          return eventInits.get(this)?.[name] ?? fallback;
        },
        configurable: true,
        enumerable: true,
      });
    }
  }
  const eventInits = new WeakMap();
  class UIEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      eventInits.set(this, { ...init });
    }
  }
  defineInitGetters(UIEvent, { detail: 0, view: null });
  const MODIFIERS = { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };
  function getModifierState(key) {
    return { Control: this.ctrlKey, Shift: this.shiftKey, Alt: this.altKey, Meta: this.metaKey }[key] ?? false;
  }
  class MouseEvent extends UIEvent {}
  defineInitGetters(MouseEvent, {
    ...MODIFIERS,
    screenX: 0,
    screenY: 0,
    clientX: 0,
    clientY: 0,
    button: 0,
    buttons: 0,
    relatedTarget: null,
  });
  Object.defineProperties(MouseEvent.prototype, {
    pageX: { get() { return this.clientX; } },
    pageY: { get() { return this.clientY; } },
    x: { get() { return this.clientX; } },
    y: { get() { return this.clientY; } },
    getModifierState: { value: getModifierState },
  });
  class PointerEvent extends MouseEvent {}
  defineInitGetters(PointerEvent, {
    pointerId: 0,
    width: 1,
    height: 1,
    pressure: 0,
    tangentialPressure: 0,
    tiltX: 0,
    tiltY: 0,
    twist: 0,
    pointerType: "",
    isPrimary: false,
  });
  class WheelEvent extends MouseEvent {}
  defineInitGetters(WheelEvent, { deltaX: 0, deltaY: 0, deltaZ: 0, deltaMode: 0 });
  class KeyboardEvent extends UIEvent {}
  defineInitGetters(KeyboardEvent, { ...MODIFIERS, key: "", code: "", location: 0, repeat: false, isComposing: false });
  Object.defineProperty(KeyboardEvent.prototype, "getModifierState", { value: getModifierState });
  class FocusEvent extends UIEvent {}
  defineInitGetters(FocusEvent, { relatedTarget: null });

  const POINTER_EVENTS = new Set(["pointerdown", "pointerup", "pointermove", "pointerover", "pointerout", "pointerenter", "pointerleave", "pointercancel"]);
  const MOUSE_EVENTS = new Set(["click", "dblclick", "auxclick", "contextmenu", "mousedown", "mouseup", "mousemove", "mouseover", "mouseout", "mouseenter", "mouseleave"]);
  const KEYBOARD_EVENTS = new Set(["keydown", "keyup", "keypress"]);
  const FOCUS_EVENTS = new Set(["focus", "blur", "focusin", "focusout"]);
  function eventClassFor(type) {
    if (POINTER_EVENTS.has(type)) return PointerEvent;
    if (MOUSE_EVENTS.has(type)) return MouseEvent;
    if (type === "wheel") return WheelEvent;
    if (KEYBOARD_EVENTS.has(type)) return KeyboardEvent;
    if (FOCUS_EVENTS.has(type)) return FocusEvent;
    return Event;
  }
  function fireTrusted(target, type, Class, init) {
    const event = new Class(type, { cancelable: false, composed: true, view: globalThis, ...init });
    setIsTrusted(event, true);
    return target.dispatchEvent(event);
  }

  // matchMedia over the probe viewport. Evaluates media types (all/screen/print),
  // `not`, `and`, comma lists and the features width/height (min/max), orientation,
  // prefers-reduced-motion, prefers-color-scheme, hover and pointer. Unknown
  // features never match. Not live: `change` never fires (no resize yet).
  function evaluateFeature(feature) {
    const match = /^\(\s*([a-z-]+)\s*(?::\s*([^)]+?))?\s*\)$/.exec(feature.trim());
    if (match === null) return false;
    const [, name, rawValue] = match;
    const value = rawValue?.trim();
    const width = op_blitz_viewport_size(0);
    const height = op_blitz_viewport_size(1);
    const pixels = (text) => {
      const parsed = /^(-?[\d.]+)(px)?$/.exec(text ?? "");
      return parsed === null ? NaN : Number(parsed[1]);
    };
    switch (name) {
      case "min-width": return width >= pixels(value);
      case "max-width": return width <= pixels(value);
      case "width": return width === pixels(value);
      case "min-height": return height >= pixels(value);
      case "max-height": return height <= pixels(value);
      case "height": return height === pixels(value);
      case "orientation": return value === (width >= height ? "landscape" : "portrait");
      case "prefers-reduced-motion": return value === undefined ? false : value === "no-preference";
      case "prefers-color-scheme": return value === "light";
      case "hover": return value === undefined || value === "hover";
      case "pointer": return value === undefined || value === "fine";
      default: return false;
    }
  }
  function evaluateMediaQuery(query) {
    let text = query.trim().toLowerCase();
    let negate = false;
    if (text.startsWith("not ")) {
      negate = true;
      text = text.slice(4).trim();
    } else if (text.startsWith("only ")) {
      text = text.slice(5).trim();
    }
    const parts = text.split(/\s+and\s+/);
    let result = true;
    for (const part of parts) {
      if (part === "all" || part === "screen") continue;
      if (part === "print" || part === "speech") {
        result = false;
        continue;
      }
      if (!evaluateFeature(part)) result = false;
    }
    return negate ? !result : result;
  }
  class MediaQueryList extends EventTarget {
    #media;
    constructor(media) {
      if (!constructing) throw new TypeError("Illegal constructor");
      super();
      this.#media = media;
    }
    get media() {
      return this.#media;
    }
    get matches() {
      return this.#media.split(",").some(evaluateMediaQuery);
    }
    addListener(listener) {
      this.addEventListener("change", listener);
    }
    removeListener(listener) {
      this.removeEventListener("change", listener);
    }
  }
  function matchMedia(query) {
    constructing = true;
    try {
      return new MediaQueryList(String(query));
    } finally {
      constructing = false;
    }
  }

  function wrapOrNull(handle) {
    return handle < 0 ? null : wrap(handle);
  }

  function query(node, selectors, all) {
    const count = op_blitz_query(handleOf(node, "querySelector"), String(selectors), all);
    if (count < 0) {
      throw new DOMException(`'${selectors}' is not a valid selector.`, "SyntaxError");
    }
    return count;
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
    get style() {
      return styleFor(this);
    }
    getAttribute(name) {
      return op_blitz_get_attribute(handleOf(this, "getAttribute"), String(name).toLowerCase());
    }
    hasAttribute(name) {
      return this.getAttribute(name) !== null;
    }
    removeAttribute(name) {
      op_blitz_remove_attribute(handleOf(this, "removeAttribute"), String(name).toLowerCase());
    }
    get className() {
      return this.getAttribute("class") ?? "";
    }
    set className(value) {
      this.setAttribute("class", value);
    }
    get classList() {
      let list = classLists.get(this);
      if (list === undefined) {
        constructing = true;
        try {
          list = new DOMTokenList(this);
        } finally {
          constructing = false;
        }
        classLists.set(this, list);
      }
      return list;
    }
    get innerHTML() {
      return op_blitz_inner_html(handleOf(this, "innerHTML"));
    }
    set innerHTML(value) {
      op_blitz_set_inner_html(handleOf(this, "innerHTML"), value === null ? "" : String(value));
    }
    get outerHTML() {
      return op_blitz_outer_html(handleOf(this, "outerHTML"));
    }
    getBoundingClientRect() {
      return measure(this, false);
    }
    get offsetLeft() {
      return Math.round(measure(this, true).x);
    }
    get offsetTop() {
      return Math.round(measure(this, true).y);
    }
    get offsetWidth() {
      return Math.round(measure(this, true).width);
    }
    get offsetHeight() {
      return Math.round(measure(this, true).height);
    }
    focus() {
      if (!this.isConnected || !isFocusable(this)) return;
      const previous = focusedElement();
      if (previous === this) return;
      if (!op_blitz_focus(handleOf(this, "focus"))) return;
      if (previous !== null) {
        fireTrusted(previous, "blur", FocusEvent, { relatedTarget: this });
        fireTrusted(previous, "focusout", FocusEvent, { bubbles: true, relatedTarget: this });
      }
      fireTrusted(this, "focus", FocusEvent, { relatedTarget: previous });
      fireTrusted(this, "focusin", FocusEvent, { bubbles: true, relatedTarget: previous });
    }
    blur() {
      if (focusedElement() !== this) return;
      op_blitz_blur();
      fireTrusted(this, "blur", FocusEvent, {});
      fireTrusted(this, "focusout", FocusEvent, { bubbles: true });
    }
    setAttribute(name, value) {
      name = String(name);
      if (!VALID_NAME.test(name)) {
        throw new DOMException(`'${name}' is not a valid attribute name.`, "InvalidCharacterError");
      }
      op_blitz_set_attribute(handleOf(this, "setAttribute"), name.toLowerCase(), String(value));
    }
  }

  // Inline style writes go to Blitz's style attribute declarations. Reads are not
  // implemented yet (they need the declaration block or computed style).
  const styles = new WeakMap();
  const toKebab = (name) => name.startsWith("--") ? name : name.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  function styleFor(element) {
    let style = styles.get(element);
    if (style !== undefined) return style;
    const handle = handleOf(element, "style");
    const methods = {
      setProperty(name, value) {
        op_blitz_set_style_property(handle, toKebab(String(name)), value === null ? "" : String(value));
      },
      removeProperty(name) {
        op_blitz_set_style_property(handle, toKebab(String(name)), "");
      },
    };
    style = new Proxy(methods, {
      set(target, name, value) {
        if (typeof name !== "string") return false;
        target.setProperty(name, value);
        return true;
      },
    });
    styles.set(element, style);
    return style;
  }

  // Static NodeList snapshot (querySelectorAll is static per spec; childNodes and
  // children are live in browsers, a known simplification here). Supports index
  // access, length, item(), iteration and forEach; filter is internal.
  class NodeList {
    #nodes;
    constructor(nodes) {
      if (!constructing) throw new TypeError("Illegal constructor");
      this.#nodes = nodes;
      nodes.forEach((node, index) => {
        Object.defineProperty(this, index, { value: node, enumerable: true });
      });
    }
    get length() {
      return this.#nodes.length;
    }
    item(index) {
      return this.#nodes[index] ?? null;
    }
    forEach(callback, thisArg) {
      this.#nodes.forEach((node, index) => callback.call(thisArg, node, index, this));
    }
    filter(predicate) {
      return this.#nodes.filter(predicate);
    }
    [Symbol.iterator]() {
      return this.#nodes[Symbol.iterator]();
    }
  }
  function createNodeList(nodes) {
    constructing = true;
    try {
      return new NodeList(nodes);
    } finally {
      constructing = false;
    }
  }

  // classList over the `class` attribute (Blitz restyles on attribute changes).
  const classLists = new WeakMap();
  class DOMTokenList {
    #element;
    constructor(element) {
      if (!constructing) throw new TypeError("Illegal constructor");
      this.#element = element;
    }
    #tokens() {
      return this.#element.className.split(/[\t\n\f\r ]+/).filter(Boolean);
    }
    #write(tokens) {
      this.#element.setAttribute("class", [...new Set(tokens)].join(" "));
    }
    #check(token) {
      token = String(token);
      if (token === "") throw new DOMException("The token must not be empty.", "SyntaxError");
      if (/[\t\n\f\r ]/.test(token)) {
        throw new DOMException("The token must not contain whitespace.", "InvalidCharacterError");
      }
      return token;
    }
    get length() {
      return this.#tokens().length;
    }
    get value() {
      return this.#element.className;
    }
    item(index) {
      return this.#tokens()[index] ?? null;
    }
    contains(token) {
      return this.#tokens().includes(String(token));
    }
    add(...tokens) {
      const list = this.#tokens();
      for (const token of tokens) {
        const checked = this.#check(token);
        if (!list.includes(checked)) list.push(checked);
      }
      this.#write(list);
    }
    remove(...tokens) {
      const checked = tokens.map((token) => this.#check(token));
      this.#write(this.#tokens().filter((token) => !checked.includes(token)));
    }
    toggle(token, force) {
      const checked = this.#check(token);
      const present = this.contains(checked);
      const wanted = force === undefined ? !present : Boolean(force);
      if (wanted && !present) this.add(checked);
      if (!wanted && present) this.remove(checked);
      return wanted;
    }
    replace(token, replacement) {
      const from = this.#check(token);
      const to = this.#check(replacement);
      const list = this.#tokens();
      const index = list.indexOf(from);
      if (index < 0) return false;
      list[index] = to;
      this.#write(list);
      return true;
    }
    [Symbol.iterator]() {
      return this.#tokens()[Symbol.iterator]();
    }
    toString() {
      return this.value;
    }
  }
  // Live computed style: every read flushes Blitz styles and serializes the
  // current computed value. Pseudo-elements are not supported (empty values).
  function getComputedStyle(element, pseudoElement = null) {
    if (!(element instanceof Element)) {
      throw new TypeError("Failed to execute 'getComputedStyle': parameter 1 is not of type 'Element'.");
    }
    const handle = handleOf(element, "getComputedStyle");
    const pseudo = pseudoElement !== null && pseudoElement !== undefined && String(pseudoElement) !== "";
    const read = (name) => (pseudo ? "" : op_blitz_computed_style(handle, name));
    const declaration = {
      getPropertyValue(name) {
        name = String(name);
        return read(name.startsWith("--") ? name : name.toLowerCase());
      },
    };
    return new Proxy(declaration, {
      get(target, name) {
        if (name in target) return target[name];
        return typeof name === "string" ? read(toKebab(name)) : undefined;
      },
      set() {
        return false;
      },
    });
  }

  class HTMLElement extends Element {}
  class SVGElement extends Element {}
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
    get documentElement() {
      return this.firstElementChild;
    }
    get body() {
      return this.querySelector("body");
    }
    get activeElement() {
      return focusedElement() ?? this.body;
    }
    get head() {
      return this.querySelector("head");
    }
    createTextNode(data) {
      return wrap(op_blitz_create_text_node(String(data)));
    }
  }

  function elementClass(namespace) {
    if (namespace === HTML_NAMESPACE) return HTMLElement;
    if (namespace === SVG_NAMESPACE) return SVGElement;
    return Element;
  }

  function wrap(handle) {
    let wrapper = cached(handle);
    if (wrapper !== undefined) return wrapper;
    const type = op_blitz_node_type(handle);
    const Class = handle === DOCUMENT_ID
      ? Document
      : type === ELEMENT_NODE
      ? elementClass(op_blitz_namespace(handle))
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
    const token = {};
    cache.set(handle, { ref: new WeakRef(wrapper), strong: null, token });
    finalizers.register(wrapper, { handle, token });
    op_blitz_retain(handle);
    retainIfConnected(handle);
    return wrapper;
  }

  const document = wrap(DOCUMENT_ID);

  // Single-document History/Location (src/history.js) and deno_webstorage.
  const createHistory = globalThis.__rovesCreateHistory;
  delete globalThis.__rovesCreateHistory;
  const historyApi = createHistory(globalThis, {
    URL,
    structuredClone,
    DOMException,
    Event,
    setTimeout: timers.setTimeout,
  });
  // requestAnimationFrame per the HTML "run the animation frame callbacks" steps:
  // the handles are snapshotted at frame start, so callbacks requested during the
  // frame run in the next one, and a callback cancelled by an earlier one is skipped.
  let animationFrameCallbacks = new Map();
  let nextAnimationFrameHandle = 0;
  function requestAnimationFrame(callback) {
    if (typeof callback !== "function") {
      throw new TypeError("Failed to execute 'requestAnimationFrame': parameter 1 is not of type 'Function'.");
    }
    const handle = ++nextAnimationFrameHandle;
    animationFrameCallbacks.set(handle, callback);
    return handle;
  }
  function cancelAnimationFrame(handle) {
    animationFrameCallbacks.delete(handle);
  }
  // The timestamp is the frame start on the performance timeline, as in browsers.
  function runAnimationFrame() {
    const timestamp = performance.now();
    for (const handle of [...animationFrameCallbacks.keys()]) {
      const callback = animationFrameCallbacks.get(handle);
      if (callback === undefined) continue;
      animationFrameCallbacks.delete(handle);
      try {
        callback(timestamp);
      } catch (error) {
        reportException(error);
      }
    }
    sweepWrappers();
  }

  // The global object is an EventTarget (like Window) so reported exceptions
  // reach `error` listeners.
  setEventTargetData(globalThis);
  globalThis[webidl.brand] = webidl.brand;
  Object.setPrototypeOf(globalThis, EventTarget.prototype);
  saveGlobalThisReference(globalThis);
  // deno_core's own queueMicrotask wrapper reports through this callback.
  core.setReportExceptionCallback(reportException);

  const globals = {
    Node,
    Element,
    HTMLElement,
    SVGElement,
    NodeList,
    DOMTokenList,
    DOMRect,
    UIEvent,
    MouseEvent,
    PointerEvent,
    WheelEvent,
    KeyboardEvent,
    FocusEvent,
    MediaQueryList,
    matchMedia,
    Text,
    Document,
    Event,
    CustomEvent,
    ErrorEvent,
    EventTarget,
    DOMException,
    performance,
    getComputedStyle,
    requestAnimationFrame,
    cancelAnimationFrame,
    setTimeout: timers.setTimeout,
    clearTimeout: timers.clearTimeout,
    setInterval: timers.setInterval,
    clearInterval: timers.clearInterval,
    URL,
    URLSearchParams,
    structuredClone,
    Storage: webStorage.Storage,
    History: historyApi.History,
    Location: historyApi.Location,
    PopStateEvent: historyApi.PopStateEvent,
    HashChangeEvent: historyApi.HashChangeEvent,
  };
  for (const [name, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
  }
  Object.defineProperty(globalThis, "document", { value: document, enumerable: true, configurable: true });
  Object.defineProperties(globalThis, {
    history: { value: historyApi.history, enumerable: true, configurable: true },
    location: { get: () => historyApi.location, set: (url) => historyApi.location.assign(url), enumerable: true, configurable: true },
    localStorage: { get: () => webStorage.localStorage(), enumerable: true, configurable: true },
    sessionStorage: { get: () => webStorage.sessionStorage(), enumerable: true, configurable: true },
    innerWidth: { get: () => op_blitz_viewport_size(0), enumerable: true, configurable: true },
    innerHeight: { get: () => op_blitz_viewport_size(1), enumerable: true, configurable: true },
    devicePixelRatio: { value: 1, enumerable: true, configurable: true },
  });
  for (const name of ["window", "self"]) {
    Object.defineProperty(globalThis, name, { value: globalThis, enumerable: true, configurable: true });
  }

  // The guest only sees the DOM surface. deno_core keeps its own captured view
  // for lazily loaded extension scripts, so removing these is safe.
  delete globalThis.Deno;
  delete globalThis.__bootstrap;

  return {
    // Trusted UI event from the embedder (Blitz hit-test result).
    dispatchTrustedEvent(handle, type, init = "{}") {
      const Class = eventClassFor(type);
      const event = new Class(type, {
        bubbles: true,
        cancelable: true,
        composed: true,
        view: globalThis,
        ...JSON.parse(init),
      });
      setIsTrusted(event, true);
      return wrap(handle).dispatchEvent(event);
    },
    runAnimationFrame,
    // Navigations the single-document policy blocked (assign/replace/reload).
    blockedNavigations() {
      return JSON.stringify(historyApi.blockedNavigations);
    },
  };
})();
