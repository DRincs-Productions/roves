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
    op_blitz_create_element_ns,
    op_blitz_prefix,
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
    op_blitz_box_sizes,
    op_blitz_rect_component,
    op_blitz_viewport_size,
    op_blitz_scroll_metrics,
    op_blitz_scroll_component,
    op_blitz_scroll_to,
    op_blitz_focus,
    op_blitz_blur,
    op_blitz_focused,
    op_blitz_game_read,
    op_blitz_fetch_check,
    op_blitz_image_bitmap_rgba,
    op_blitz_gpu_wait_idle,
    op_blitz_gpu_poll,
    op_blitz_gpu_adapter_info,
    op_blitz_matches,
    op_blitz_create_comment,
    op_blitz_clone_node,
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
    PromiseRejectionEvent,
    MessageEvent,
    defineEventHandler,
  } = core.loadExtScript("ext:deno_web/02_event.js");
  const { Console } = core.loadExtScript("ext:deno_web/01_console.js");
  const { Blob, File, blobFromObjectUrl } = core.loadExtScript("ext:deno_web/09_file.js");
  const { TextDecoder, TextEncoder } = core.loadExtScript("ext:deno_web/08_text_encoding.js");
  const { atob, btoa } = core.loadExtScript("ext:deno_web/05_base64.js");
  const timers = core.loadExtScript("ext:deno_web/02_timers.js");
  // AbortController/AbortSignal from deno_web (needs 02_event and 02_timers,
  // loaded above). Pixi'VN narration hooks construct an AbortController at startup.
  const { AbortController, AbortSignal } = core.loadExtScript("ext:deno_web/03_abort_signal.js");
  const { MessageChannel, MessagePort } = core.loadExtScript("ext:deno_web/13_message_port.js");
  const webidl = core.loadExtScript("ext:deno_webidl/00_webidl.js");
  const { performance, setTimeOrigin } = core.loadExtScript("ext:deno_web/15_performance.js");
  setTimeOrigin();
  const HTML_NAMESPACE = "http://www.w3.org/1999/xhtml";
  const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
  const { DOMException } = core.loadExtScript("ext:deno_web/01_dom_exception.js");
  const { URL, URLSearchParams } = core.loadExtScript("ext:deno_web/00_url.js");
  // URL.createObjectURL/revokeObjectURL come from deno_web's blob URL script
  // (PixiJS starts its workers from blob: URLs).
  if (typeof URL.createObjectURL !== "function") core.loadExtScript("ext:deno_web/11_blob_url.js");
  const { structuredClone } = core.loadExtScript("ext:deno_web/02_structured_clone.js");
  const webStorage = core.loadExtScript("ext:deno_webstorage/01_webstorage.js");
  // WebGPU from deno_webgpu (lazy ESM, as Deno loads it); `gpu` exists after initGPU.
  const webgpu = core.createLazyLoader("ext:deno_webgpu/01_webgpu.js")();
  // createImageBitmap/ImageBitmap (PNG/JPEG decoding) from deno_image.
  const imageApi = core.createLazyLoader("ext:deno_image/01_image.js")();

  // deno_webgpu 0.227 has no GPUQueue.copyExternalImageToTexture, which PixiJS
  // uses for every image texture. Polyfill it over writeTexture for ImageBitmap
  // (pixels through op_blitz_image_bitmap_rgba) and ImageData sources, with
  // origin, flipY, premultipliedAlpha and bgra destinations.
  function installCopyExternalImage() {
    const proto = webgpu.GPUQueue?.prototype;
    if (!proto || typeof proto.copyExternalImageToTexture === "function") return;
    const xy = (value) => (Array.isArray(value) ? [value[0] ?? 0, value[1] ?? 0] : [value?.x ?? 0, value?.y ?? 0]);
    proto.copyExternalImageToTexture = function (source, destination, copySize) {
      const image = source.source;
      let width;
      let pixels;
      if (image instanceof imageApi.ImageBitmap) {
        width = image.width;
        pixels = op_blitz_image_bitmap_rgba(image);
      } else if (image?.data instanceof Uint8ClampedArray && typeof image.width === "number") {
        width = image.width;
        pixels = image.data;
      } else {
        throw new TypeError("copyExternalImageToTexture: only ImageBitmap and ImageData sources are supported");
      }
      const [copyWidth, copyHeight] = Array.isArray(copySize)
        ? [copySize[0], copySize[1] ?? 1]
        : [copySize.width, copySize.height ?? 1];
      const [originX, originY] = xy(source.origin);
      const out = new Uint8Array(copyWidth * copyHeight * 4);
      for (let row = 0; row < copyHeight; row++) {
        const sourceRow = originY + (source.flipY ? copyHeight - 1 - row : row);
        const from = (sourceRow * width + originX) * 4;
        out.set(pixels.subarray(from, from + copyWidth * 4), row * copyWidth * 4);
      }
      const bgra = String(destination.texture.format).startsWith("bgra");
      for (let index = 0; index < out.length; index += 4) {
        if (destination.premultipliedAlpha) {
          const alpha = out[index + 3] / 255;
          out[index] = Math.round(out[index] * alpha);
          out[index + 1] = Math.round(out[index + 1] * alpha);
          out[index + 2] = Math.round(out[index + 2] * alpha);
        }
        if (bgra) {
          const red = out[index];
          out[index] = out[index + 2];
          out[index + 2] = red;
        }
      }
      this.writeTexture(
        {
          texture: destination.texture,
          mipLevel: destination.mipLevel ?? 0,
          origin: destination.origin ?? [0, 0, 0],
          aspect: destination.aspect ?? "all",
        },
        out,
        { offset: 0, bytesPerRow: copyWidth * 4, rowsPerImage: copyHeight },
        [copyWidth, copyHeight, 1],
      );
    };
  }

  const ELEMENT_NODE = 1;
  const TEXT_NODE = 3;
  const DOCUMENT_NODE = 9;
  // Versioned Blitz node id of the document node, as a number handle.
  const DOCUMENT_ID = op_blitz_document_root();
  // XML Name production is broader; this ASCII subset covers HTML elements and
  // custom elements and rejects everything the probe does not handle.
  const VALID_NAME = /^[A-Za-z][A-Za-z0-9_.-]*$/;
  // XML QName ("prefix:local", both NCNames) for createElementNS. NCName chars
  // are approximated by ASCII letters/digits/_.- plus the non-ASCII BMP ranges
  // of the XML NameChar production; astral characters are rejected.
  const NC_START = "A-Za-z_\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u02FF\\u0370-\\u037D\\u037F-\\u1FFF"
    + "\\u200C\\u200D\\u2070-\\u218F\\u2C00-\\u2FEF\\u3001-\\uD7FF\\uF900-\\uFDCF\\uFDF0-\\uFFFD";
  const NC_NAME = `[${NC_START}][${NC_START}0-9.\\-\\u00B7\\u0300-\\u036F\\u203F-\\u2040]*`;
  const QUALIFIED_NAME = new RegExp(`^(?:(${NC_NAME}):)?(${NC_NAME})$`);
  const XML_NAMESPACE = "http://www.w3.org/XML/1998/namespace";
  const XMLNS_NAMESPACE = "http://www.w3.org/2000/xmlns/";

  // DOM "validate and extract": InvalidCharacterError for a non-QName,
  // NamespaceError for a prefix without namespace or misuse of xml/xmlns.
  // Arguments arrive already converted by the WebIDL layer (DOMString? / DOMString).
  function validateAndExtract(namespace, qualifiedName) {
    if (namespace === "") namespace = null;
    const match = QUALIFIED_NAME.exec(qualifiedName);
    if (match === null) {
      throw new DOMException(`'${qualifiedName}' is not a valid qualified name.`, "InvalidCharacterError");
    }
    const prefix = match[1] ?? null;
    const localName = match[2];
    const fail = (why) => {
      throw new DOMException(`'${qualifiedName}' in namespace '${namespace}': ${why}`, "NamespaceError");
    };
    if (prefix !== null && namespace === null) fail("a prefix requires a namespace");
    if (prefix === "xml" && namespace !== XML_NAMESPACE) fail("the xml prefix is reserved");
    const isXmlns = qualifiedName === "xmlns" || prefix === "xmlns";
    if (isXmlns !== (namespace === XMLNS_NAMESPACE)) fail("xmlns names and the XMLNS namespace go together");
    return { namespace, prefix, localName };
  }

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
    get nodeName() {
      switch (this.nodeType) {
        case ELEMENT_NODE:
          return this.tagName;
        case TEXT_NODE:
          return "#text";
        case 8:
          return "#comment";
        case DOCUMENT_NODE:
          return "#document";
        default:
          return "";
      }
    }
    cloneNode(deep = false) {
      return wrap(op_blitz_clone_node(handleOf(this, "cloneNode"), Boolean(deep)));
    }
    get parentNode() {
      const parent = op_blitz_parent(handleOf(this, "parentNode"));
      return parent < 0 ? null : wrap(parent);
    }
    get parentElement() {
      const parent = this.parentNode;
      return parent instanceof Element ? parent : null;
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
      const handle = handleOf(this, "textContent");
      const text = value === null ? "" : String(value);
      if (this.nodeType === TEXT_NODE) {
        const oldValue = mutationObserversActive() ? op_blitz_text_content(handle) : null;
        op_blitz_set_text_content(handle, text);
        queueMutation(this, "characterData", { oldValue });
        return;
      }
      const removed = mutationObserversActive() ? [...this.childNodes] : [];
      op_blitz_set_text_content(handle, text);
      const added = text === "" || !mutationObserversActive() ? [] : [...this.childNodes];
      queueMutation(this, "childList", { addedNodes: added, removedNodes: removed });
    }
    get data() {
      return this.nodeType === TEXT_NODE ? this.textContent : undefined;
    }
    set data(value) {
      if (this.nodeType === TEXT_NODE) this.textContent = value;
    }
    get nodeValue() {
      return this.nodeType === TEXT_NODE ? this.textContent : null;
    }
    set nodeValue(value) {
      if (this.nodeType === TEXT_NODE) this.textContent = value;
    }
    appendChild(child) {
      const childHandle = handleOf(child, "appendChild");
      const oldParent = mutationObserversActive() ? child.parentNode : null;
      checkStatus(op_blitz_append_child(handleOf(this, "appendChild"), childHandle));
      retainIfConnected(childHandle);
      if (oldParent !== null) queueMutation(oldParent, "childList", { removedNodes: [child] });
      queueMutation(this, "childList", { addedNodes: [child] });
      return child;
    }
    removeChild(child) {
      checkStatus(op_blitz_remove_child(handleOf(this, "removeChild"), handleOf(child, "removeChild")));
      queueMutation(this, "childList", { removedNodes: [child] });
      return child;
    }
    insertBefore(node, child) {
      const nodeHandle = handleOf(node, "insertBefore");
      const reference = child === null || child === undefined ? -1 : handleOf(child, "insertBefore");
      const oldParent = mutationObserversActive() ? node.parentNode : null;
      checkStatus(op_blitz_insert_before(handleOf(this, "insertBefore"), nodeHandle, reference));
      retainIfConnected(nodeHandle);
      if (oldParent !== null) queueMutation(oldParent, "childList", { removedNodes: [node] });
      queueMutation(this, "childList", { addedNodes: [node] });
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
  // DOMRectReadOnly/DOMRect per Geometry Interfaces: accessors on the
  // prototype, DOMRect adds the setters (ResizeObserverEntry.contentRect is
  // read-only).
  let setRectValue;
  class DOMRectReadOnly {
    #values;
    constructor(x = 0, y = 0, width = 0, height = 0) {
      this.#values = [Number(x), Number(y), Number(width), Number(height)];
    }
    static {
      setRectValue = (rect, index, value) => {
        rect.#values[index] = Number(value);
      };
    }
    static fromRect(other = {}) {
      return new this(other.x, other.y, other.width, other.height);
    }
    get x() {
      return this.#values[0];
    }
    get y() {
      return this.#values[1];
    }
    get width() {
      return this.#values[2];
    }
    get height() {
      return this.#values[3];
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
  class DOMRect extends DOMRectReadOnly {
    get x() {
      return super.x;
    }
    set x(value) {
      setRectValue(this, 0, value);
    }
    get y() {
      return super.y;
    }
    set y(value) {
      setRectValue(this, 1, value);
    }
    get width() {
      return super.width;
    }
    set width(value) {
      setRectValue(this, 2, value);
    }
    get height() {
      return super.height;
    }
    set height(value) {
      setRectValue(this, 3, value);
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

  // CSSOM View scrolling over Blitz's own scroll state (viewport scroll and per-node
  // scroll offsets, see op_blitz_scroll_*). The root element stands for the
  // viewport. Offsets are clamped by Rust to 0..max per axis; elements whose
  // overflow is visible/clip do not scroll. `behavior: "smooth"` is accepted and
  // applied immediately (no animation). No `scroll` events are dispatched yet.
  const SCROLL_LEFT = 0, SCROLL_TOP = 1, SCROLL_WIDTH = 2, SCROLL_HEIGHT = 3;
  const CLIENT_WIDTH = 4, CLIENT_HEIGHT = 5, CLIENT_LEFT = 6, CLIENT_TOP = 7;
  function scrollMetric(element, index, method) {
    op_blitz_scroll_metrics(handleOf(element, method));
    return op_blitz_scroll_component(index);
  }
  // WebIDL unrestricted double, then CSSOM "normalize non-finite values" (-> 0).
  function finiteOrZero(value) {
    const number = Number(value);
    return Number.isFinite(number) ? number : 0;
  }
  const SCROLL_BEHAVIORS = ["auto", "instant", "smooth"];
  function enumMember(value, allowed, what) {
    const text = String(value);
    if (!allowed.includes(text)) {
      throw new TypeError(`The provided value '${text}' is not a valid enum value of type ${what}.`);
    }
    return text;
  }
  // scroll()/scrollTo()/scrollBy() overloads: (ScrollToOptions?) or (x, y).
  // Returns [left, top] with NaN for an axis the options leave unchanged.
  function scrollTarget(args, method) {
    if (args.length >= 2) return [finiteOrZero(args[0]), finiteOrZero(args[1])];
    const options = args[0];
    if (options !== undefined && options !== null && typeof options !== "object" && typeof options !== "function") {
      throw new TypeError(`Failed to execute '${method}': The provided value is not of type 'ScrollToOptions'.`);
    }
    if (options?.behavior !== undefined) enumMember(options.behavior, SCROLL_BEHAVIORS, "ScrollBehavior");
    return [
      options?.left === undefined ? NaN : finiteOrZero(options.left),
      options?.top === undefined ? NaN : finiteOrZero(options.top),
    ];
  }
  function scrollElement(element, args, method, relative) {
    const handle = handleOf(element, method);
    let [left, top] = scrollTarget(args, method);
    if (relative) {
      op_blitz_scroll_metrics(handle);
      left = Number.isNaN(left) ? NaN : op_blitz_scroll_component(SCROLL_LEFT) + left;
      top = Number.isNaN(top) ? NaN : op_blitz_scroll_component(SCROLL_TOP) + top;
    }
    op_blitz_scroll_to(handle, left, top);
  }
  // scrollIntoView alignment of [start, start + size) inside [areaStart, areaStart
  // + areaSize): the scroll delta for "start" | "center" | "end" | "nearest".
  function alignDelta(start, size, areaStart, areaSize, mode) {
    const end = start + size;
    const areaEnd = areaStart + areaSize;
    switch (mode) {
      case "start": return start - areaStart;
      case "end": return end - areaEnd;
      case "center": return start + size / 2 - (areaStart + areaSize / 2);
      default:
        // "nearest": nothing when fully inside or covering the area; otherwise the
        // edge that needs the smaller move (CSSOM View's case analysis).
        if ((start >= areaStart && end <= areaEnd) || (start < areaStart && end > areaEnd)) return 0;
        if (start < areaStart) return size <= areaSize ? start - areaStart : end - areaEnd;
        return size <= areaSize ? end - areaEnd : start - areaStart;
    }
  }
  // Aligns `element` inside one scrolling box (the viewport for the root element),
  // measuring after the previous (inner) boxes have scrolled.
  function scrollIntoBox(element, box, isViewport, block, inline) {
    const handle = handleOf(box, "scrollIntoView");
    if (op_blitz_scroll_metrics(handle) === 0) return;
    const left = op_blitz_scroll_component(SCROLL_LEFT);
    const top = op_blitz_scroll_component(SCROLL_TOP);
    const borderLeft = op_blitz_scroll_component(CLIENT_LEFT);
    const borderTop = op_blitz_scroll_component(CLIENT_TOP);
    const areaWidth = op_blitz_scroll_component(CLIENT_WIDTH);
    const areaHeight = op_blitz_scroll_component(CLIENT_HEIGHT);
    const target = element.getBoundingClientRect();
    const boxRect = isViewport ? null : box.getBoundingClientRect();
    const areaLeft = isViewport ? 0 : boxRect.x + borderLeft;
    const areaTop = isViewport ? 0 : boxRect.y + borderTop;
    op_blitz_scroll_to(
      handle,
      left + alignDelta(target.x, target.width, areaLeft, areaWidth, inline),
      top + alignDelta(target.y, target.height, areaTop, areaHeight, block),
    );
  }
  const SCROLL_ALIGNMENTS = ["start", "center", "end", "nearest"];

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

  // Attribute names are ASCII-lowercased only on HTML-namespace elements (the
  // document is always an HTML document); SVG and other namespaces keep case,
  // e.g. React's `viewBox`.
  function attributeKey(handle, name) {
    name = String(name);
    return op_blitz_namespace(handle) === HTML_NAMESPACE ? name.replace(/[A-Z]+/g, (c) => c.toLowerCase()) : name;
  }

  class Element extends Node {
    get localName() {
      return op_blitz_local_name(handleOf(this, "localName"));
    }
    get tagName() {
      const prefix = this.prefix;
      const name = prefix === null ? this.localName : `${prefix}:${this.localName}`;
      return this.namespaceURI === HTML_NAMESPACE ? name.toUpperCase() : name;
    }
    get prefix() {
      return op_blitz_prefix(handleOf(this, "prefix")) || null;
    }
    get namespaceURI() {
      return op_blitz_namespace(handleOf(this, "namespaceURI")) || null;
    }
    matches(selectors) {
      const result = op_blitz_matches(handleOf(this, "matches"), String(selectors));
      if (result < 0) throw new DOMException(`'${selectors}' is not a valid selector.`, "SyntaxError");
      return result === 1;
    }
    webkitMatchesSelector(selectors) {
      return this.matches(selectors);
    }
    getElementsByTagName(qualifiedName) {
      handleOf(this, "getElementsByTagName");
      return elementsByTagName(this, qualifiedName);
    }
    closest(selectors) {
      for (let node = this; node !== null; node = node.parentNode) {
        if (node.nodeType === ELEMENT_NODE && node.matches(selectors)) return node;
      }
      return null;
    }
    get dataset() {
      const element = this;
      const toAttribute = (key) => "data-" + String(key).replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
      return new Proxy({}, {
        get: (_, key) => (typeof key === "string" ? element.getAttribute(toAttribute(key)) ?? undefined : undefined),
        set: (_, key, value) => {
          element.setAttribute(toAttribute(key), String(value));
          return true;
        },
        deleteProperty: (_, key) => {
          element.removeAttribute(toAttribute(key));
          return true;
        },
        has: (_, key) => element.hasAttribute(toAttribute(key)),
      });
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
      const handle = handleOf(this, "getAttribute");
      return op_blitz_get_attribute(handle, attributeKey(handle, name));
    }
    hasAttribute(name) {
      return this.getAttribute(name) !== null;
    }
    removeAttribute(name) {
      const handle = handleOf(this, "removeAttribute");
      const attribute = attributeKey(handle, name);
      const oldValue = op_blitz_get_attribute(handle, attribute);
      op_blitz_remove_attribute(handle, attribute);
      if (oldValue !== null) queueMutation(this, "attributes", { attributeName: attribute, oldValue });
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
      const removed = mutationObserversActive() ? [...this.childNodes] : [];
      op_blitz_set_inner_html(handleOf(this, "innerHTML"), value === null ? "" : String(value));
      const added = mutationObserversActive() ? [...this.childNodes] : [];
      queueMutation(this, "childList", { addedNodes: added, removedNodes: removed });
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
    get scrollLeft() {
      return scrollMetric(this, SCROLL_LEFT, "scrollLeft");
    }
    set scrollLeft(value) {
      op_blitz_scroll_to(handleOf(this, "scrollLeft"), finiteOrZero(value), NaN);
    }
    get scrollTop() {
      return scrollMetric(this, SCROLL_TOP, "scrollTop");
    }
    set scrollTop(value) {
      op_blitz_scroll_to(handleOf(this, "scrollTop"), NaN, finiteOrZero(value));
    }
    get scrollWidth() {
      return Math.round(scrollMetric(this, SCROLL_WIDTH, "scrollWidth"));
    }
    get scrollHeight() {
      return Math.round(scrollMetric(this, SCROLL_HEIGHT, "scrollHeight"));
    }
    get clientWidth() {
      return Math.round(scrollMetric(this, CLIENT_WIDTH, "clientWidth"));
    }
    get clientHeight() {
      return Math.round(scrollMetric(this, CLIENT_HEIGHT, "clientHeight"));
    }
    get clientLeft() {
      return Math.round(scrollMetric(this, CLIENT_LEFT, "clientLeft"));
    }
    get clientTop() {
      return Math.round(scrollMetric(this, CLIENT_TOP, "clientTop"));
    }
    scroll(...args) {
      scrollElement(this, args, "scroll", false);
    }
    scrollTo(...args) {
      scrollElement(this, args, "scrollTo", false);
    }
    scrollBy(...args) {
      scrollElement(this, args, "scrollBy", true);
    }
    // block/inline alignment through every scroll-container ancestor, inner to
    // outer, then the viewport. Detached elements and elements without a box do
    // not scroll anything.
    scrollIntoView(arg = undefined) {
      let block = "start";
      let inline = "nearest";
      if (arg !== null && (typeof arg === "object" || typeof arg === "function")) {
        if (arg.behavior !== undefined) enumMember(arg.behavior, SCROLL_BEHAVIORS, "ScrollBehavior");
        if (arg.block !== undefined) block = enumMember(arg.block, SCROLL_ALIGNMENTS, "ScrollLogicalPosition");
        if (arg.inline !== undefined) inline = enumMember(arg.inline, SCROLL_ALIGNMENTS, "ScrollLogicalPosition");
      } else if (arg !== undefined && arg !== null) {
        block = arg ? "start" : "end";
      }
      if (!this.isConnected || !op_blitz_measure(handleOf(this, "scrollIntoView"), false)) return;
      const root = document.documentElement;
      for (let box = this.parentElement; box !== null && box !== root; box = box.parentElement) {
        scrollIntoBox(this, box, false, block, inline);
      }
      if (root !== null) scrollIntoBox(this, root, true, block, inline);
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
      const handle = handleOf(this, "setAttribute");
      const attribute = attributeKey(handle, name);
      const oldValue = mutationObserversActive() ? op_blitz_get_attribute(handle, attribute) : null;
      op_blitz_set_attribute(handle, attribute, String(value));
      queueMutation(this, "attributes", { attributeName: attribute, oldValue });
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
  // MutationObserver: every DOM mutation the game can make goes through the
  // methods above, which queue records here. Records are delivered in a
  // microtask, per observer, as the DOM specifies (childList, attributes with
  // attributeFilter/oldValue, characterData with oldValue, subtree).
  const mutationObservers = new Set();
  let mutationDeliveryQueued = false;
  // Bumped on every childList change; invalidates live HTMLCollections.
  let treeVersion = 0;
  function mutationObserversActive() {
    return mutationObservers.size > 0;
  }
  function queueMutation(target, type, details) {
    // Every tree change in the probe goes through the wrapper methods that call
    // this, so the counter is a sound invalidation key for live HTMLCollections.
    if (type === "childList") treeVersion++;
    if (mutationObservers.size === 0) return;
    if (type === "childList" && !(details.addedNodes?.length || details.removedNodes?.length)) return;
    let queued = false;
    for (const observer of mutationObservers) {
      if (observer._consider(target, type, details)) queued = true;
    }
    if (queued && !mutationDeliveryQueued) {
      mutationDeliveryQueued = true;
      queueMicrotask(() => {
        mutationDeliveryQueued = false;
        for (const observer of [...mutationObservers]) observer._deliver();
      });
    }
  }
  class MutationRecord {
    constructor(init) {
      Object.assign(this, {
        type: init.type,
        target: init.target,
        addedNodes: createNodeList(init.addedNodes ?? []),
        removedNodes: createNodeList(init.removedNodes ?? []),
        previousSibling: null,
        nextSibling: null,
        attributeName: init.attributeName ?? null,
        attributeNamespace: null,
        oldValue: init.oldValue ?? null,
      });
    }
  }
  class MutationObserver {
    #callback;
    #targets = new Map();
    #records = [];
    constructor(callback) {
      if (typeof callback !== "function") {
        throw new TypeError("Failed to construct 'MutationObserver': parameter 1 is not of type 'Function'.");
      }
      this.#callback = callback;
    }
    observe(target, options = {}) {
      handleOf(target, "observe");
      const normalized = { ...options };
      if (normalized.attributeOldValue || normalized.attributeFilter) normalized.attributes ??= true;
      if (normalized.characterDataOldValue) normalized.characterData ??= true;
      if (!normalized.childList && !normalized.attributes && !normalized.characterData) {
        throw new TypeError("The options object must set at least one of 'attributes', 'characterData', or 'childList' to true.");
      }
      this.#targets.set(target, normalized);
      mutationObservers.add(this);
    }
    disconnect() {
      this.#targets.clear();
      this.#records = [];
      mutationObservers.delete(this);
    }
    takeRecords() {
      return this.#records.splice(0);
    }
    _consider(target, type, details) {
      for (const [observed, options] of this.#targets) {
        const matches = observed === target || (options.subtree && observed.contains(target));
        if (!matches) continue;
        if (type === "childList" && !options.childList) continue;
        if (type === "attributes") {
          if (!options.attributes) continue;
          if (options.attributeFilter && !options.attributeFilter.includes(details.attributeName)) continue;
        }
        if (type === "characterData" && !options.characterData) continue;
        const keepOld = (type === "attributes" && options.attributeOldValue) || (type === "characterData" && options.characterDataOldValue);
        this.#records.push(new MutationRecord({ ...details, type, target, oldValue: keepOld ? details.oldValue : null }));
        return true;
      }
      return false;
    }
    _deliver() {
      const records = this.takeRecords();
      if (records.length === 0) return;
      try {
        this.#callback.call(this, records, this);
      } catch (error) {
        reportException(error);
      }
    }
  }

  // ResizeObserver (CSSWG Resize Observer 1). Sizes come from the Taffy layout
  // Blitz already computes (op_blitz_box_sizes); nothing is measured in JS.
  // Observations are processed from runAnimationFrame, after the rAF callbacks,
  // as in HTML "update the rendering": callbacks always run asynchronously on
  // the V8 thread inside the embedder's frame entry, never from an op.
  // Horizontal writing mode only (inlineSize = width, blockSize = height); SVG
  // elements use their CSS layout box, not the SVG bounding box.
  const RESIZE_OBSERVER_BOXES = ["content-box", "border-box", "device-pixel-content-box"];
  const resizeObservers = new Set();
  class ResizeObserverSize {
    #inlineSize;
    #blockSize;
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
    }
    static _create(inlineSize, blockSize) {
      constructing = true;
      try {
        const size = new ResizeObserverSize();
        size.#inlineSize = inlineSize;
        size.#blockSize = blockSize;
        return Object.freeze(size);
      } finally {
        constructing = false;
      }
    }
    get inlineSize() {
      return this.#inlineSize;
    }
    get blockSize() {
      return this.#blockSize;
    }
  }
  const ZERO_BOXES = Object.freeze({ x: 0, y: 0, width: 0, height: 0, borderWidth: 0, borderHeight: 0 });
  function resizeBoxes(target, flush) {
    if (!target.isConnected) return ZERO_BOXES;
    op_blitz_box_sizes(handleOf(target, "observe"), flush);
    return {
      x: op_blitz_rect_component(0),
      y: op_blitz_rect_component(1),
      width: op_blitz_rect_component(2),
      height: op_blitz_rect_component(3),
      borderWidth: op_blitz_rect_component(4),
      borderHeight: op_blitz_rect_component(5),
    };
  }
  // "calculate box size" for the observed box; devicePixelRatio is 1 here.
  function observedSize(boxes, box) {
    if (box === "border-box") return [boxes.borderWidth, boxes.borderHeight];
    if (box === "device-pixel-content-box") {
      return [Math.round(boxes.width * devicePixelRatio), Math.round(boxes.height * devicePixelRatio)];
    }
    return [boxes.width, boxes.height];
  }
  class ResizeObserverEntry {
    #target;
    #contentRect;
    #borderBoxSize;
    #contentBoxSize;
    #devicePixelContentBoxSize;
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
    }
    static _create(target, boxes) {
      constructing = true;
      let entry;
      try {
        entry = new ResizeObserverEntry();
      } finally {
        constructing = false;
      }
      const sizes = (box) => Object.freeze([ResizeObserverSize._create(...observedSize(boxes, box))]);
      entry.#target = target;
      entry.#contentRect = new DOMRectReadOnly(boxes.x, boxes.y, boxes.width, boxes.height);
      entry.#borderBoxSize = sizes("border-box");
      entry.#contentBoxSize = sizes("content-box");
      entry.#devicePixelContentBoxSize = sizes("device-pixel-content-box");
      return entry;
    }
    get target() {
      return this.#target;
    }
    get contentRect() {
      return this.#contentRect;
    }
    get borderBoxSize() {
      return this.#borderBoxSize;
    }
    get contentBoxSize() {
      return this.#contentBoxSize;
    }
    get devicePixelContentBoxSize() {
      return this.#devicePixelContentBoxSize;
    }
  }
  // Spec "calculate depth for node": the number of nodes on the path from the
  // node up to the root of its tree, the node itself included, so every element
  // has depth >= 1. A disconnected target is the root of its own tree (depth 1)
  // and must stay deeper than the initial gather depth 0, otherwise its first
  // 0 x 0 report would be skipped as a loop instead of delivered.
  function nodeDepth(node) {
    let depth = 1;
    for (let parent = node.parentNode; parent !== null; parent = parent.parentNode) depth++;
    return depth;
  }
  class ResizeObserver {
    #callback;
    // target -> { box, lastReported: [inline, block] }, in observation order.
    #observations = new Map();
    #active = [];
    #skipped = false;
    constructor(callback) {
      if (typeof callback !== "function") {
        throw new TypeError("Failed to construct 'ResizeObserver': parameter 1 is not of type 'Function'.");
      }
      this.#callback = callback;
    }
    observe(target, options = {}) {
      if (!(target instanceof Element)) {
        throw new TypeError("Failed to execute 'observe' on 'ResizeObserver': parameter 1 is not of type 'Element'.");
      }
      const box = options?.box === undefined ? "content-box" : String(options.box);
      if (!RESIZE_OBSERVER_BOXES.includes(box)) {
        throw new TypeError(`Failed to execute 'observe' on 'ResizeObserver': The provided value '${box}' is not a valid enum value of type ResizeObserverBoxOptions.`);
      }
      // Re-observing replaces the observation (and re-arms the first report).
      this.#observations.delete(target);
      // -1 x -1 so the first pass reports every target, including 0 x 0 ones.
      this.#observations.set(target, { box, lastReported: [-1, -1] });
      resizeObservers.add(this);
    }
    unobserve(target) {
      if (!(target instanceof Element)) {
        throw new TypeError("Failed to execute 'unobserve' on 'ResizeObserver': parameter 1 is not of type 'Element'.");
      }
      this.#observations.delete(target);
      this.#active = this.#active.filter((observation) => observation.target !== target);
      if (this.#observations.size === 0) resizeObservers.delete(this);
    }
    disconnect() {
      this.#observations.clear();
      this.#active = [];
      resizeObservers.delete(this);
    }
    // "gather active observations at depth": `measure` flushes layout once per pass.
    _gather(depth, measure) {
      this.#active = [];
      this.#skipped = false;
      for (const [target, observation] of this.#observations) {
        const [inline, block] = observedSize(measure(target), observation.box);
        if (inline === observation.lastReported[0] && block === observation.lastReported[1]) continue;
        if (nodeDepth(target) > depth) {
          this.#active.push({ target, observation });
        } else {
          this.#skipped = true;
        }
      }
    }
    _hasActive() {
      return this.#active.length > 0;
    }
    _hasSkipped() {
      return this.#skipped;
    }
    // "broadcast active observations": returns the shallowest delivered depth.
    _broadcast() {
      let shallowest = Infinity;
      if (this.#active.length === 0) return shallowest;
      const entries = [];
      let flush = true;
      for (const { target, observation } of this.#active) {
        const boxes = resizeBoxes(target, flush);
        // Disconnected targets skip layout, so they must not consume the flush.
        if (target.isConnected) flush = false;
        entries.push(ResizeObserverEntry._create(target, boxes));
        observation.lastReported = observedSize(boxes, observation.box);
        shallowest = Math.min(shallowest, nodeDepth(target));
      }
      this.#active = [];
      try {
        this.#callback.call(this, entries, this);
      } catch (error) {
        reportException(error);
      }
      return shallowest;
    }
  }
  // Steps 14-16 of "update the rendering" for the document.
  function processResizeObservations() {
    if (resizeObservers.size === 0) return;
    const gather = (depth) => {
      // One style + layout flush per pass; callbacks may have changed the tree.
      const cache = new Map();
      let flush = true;
      const measure = (target) => {
        let boxes = cache.get(target);
        if (boxes === undefined) {
          boxes = resizeBoxes(target, flush);
          if (target.isConnected) flush = false;
          cache.set(target, boxes);
        }
        return boxes;
      };
      for (const observer of resizeObservers) observer._gather(depth, measure);
    };
    gather(0);
    while ([...resizeObservers].some((observer) => observer._hasActive())) {
      let depth = Infinity;
      for (const observer of [...resizeObservers]) depth = Math.min(depth, observer._broadcast());
      gather(depth);
    }
    if ([...resizeObservers].some((observer) => observer._hasSkipped())) {
      const message = "ResizeObserver loop completed with undelivered notifications.";
      const event = new ErrorEvent("error", { message, cancelable: true });
      if (globalThis.dispatchEvent(event)) core.print(`${message}\n`, true);
    }
  }

  // Live HTMLCollection (DOM §4.2.10.2): the element list is recomputed lazily
  // whenever the tree changed since the last read (treeVersion), so nodes
  // inserted or removed after the call are reflected like in browsers. A Proxy
  // provides the indexed properties; state lives in a WeakMap keyed by the proxy.
  const collectionState = new WeakMap();
  const isIndex = (key) => typeof key === "string" && /^(0|[1-9]\d*)$/.test(key);
  function collectionElements(collection) {
    const state = collectionState.get(collection);
    if (state === undefined) throw new TypeError("Illegal invocation");
    if (state.version !== treeVersion) {
      state.elements = state.compute();
      state.version = treeVersion;
    }
    return state.elements;
  }
  class HTMLCollection {
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
    }
    get length() {
      return collectionElements(this).length;
    }
    item(index) {
      return collectionElements(this)[index >>> 0] ?? null;
    }
    namedItem(name) {
      const key = String(name);
      if (key === "") return null;
      return collectionElements(this).find((element) =>
        element.getAttribute("id") === key ||
        (element.namespaceURI === HTML_NAMESPACE && element.getAttribute("name") === key)) ?? null;
    }
    [Symbol.iterator]() {
      return collectionElements(this)[Symbol.iterator]();
    }
    get [Symbol.toStringTag]() {
      return "HTMLCollection";
    }
  }
  function createHTMLCollection(compute) {
    constructing = true;
    let target;
    try {
      target = new HTMLCollection();
    } finally {
      constructing = false;
    }
    const proxy = new Proxy(target, {
      get(object, key, receiver) {
        if (isIndex(key)) return collectionElements(proxy)[key];
        return Reflect.get(object, key, receiver);
      },
      has(object, key) {
        if (isIndex(key)) return Number(key) < collectionElements(proxy).length;
        return Reflect.has(object, key);
      },
      set(object, key, value, receiver) {
        if (isIndex(key)) return false;
        return Reflect.set(object, key, value, receiver);
      },
      defineProperty(object, key, descriptor) {
        if (isIndex(key)) return false;
        return Reflect.defineProperty(object, key, descriptor);
      },
      deleteProperty(object, key) {
        if (isIndex(key)) return !(Number(key) < collectionElements(proxy).length);
        return Reflect.deleteProperty(object, key);
      },
      getOwnPropertyDescriptor(object, key) {
        if (isIndex(key)) {
          const element = collectionElements(proxy)[key];
          return element === undefined
            ? undefined
            : { value: element, writable: false, enumerable: true, configurable: true };
        }
        return Reflect.getOwnPropertyDescriptor(object, key);
      },
      ownKeys(object) {
        const indices = collectionElements(proxy).map((_, index) => String(index));
        return [...indices, ...Reflect.ownKeys(object)];
      },
    });
    collectionState.set(proxy, { compute, elements: [], version: -1 });
    return proxy;
  }
  // getElementsByTagName (DOM §4.4 "list of elements with qualified name"):
  // "*" matches every descendant element; otherwise HTML-namespace elements match
  // the ASCII-lowercased name and other elements match it exactly. For "*" and
  // plain lowercase names both rules reduce to an exact CSS type selector, so
  // those (the common "head"/"style"/"script" case) go through Blitz/Stylo
  // matching without depending on Stylo's HTML case folding; any other name
  // (uppercase, prefixed, needing escapes) is filtered by localName in JS.
  const PLAIN_LOWERCASE_TAG = /^[a-z_][a-z0-9_-]*$/;
  function elementsByTagName(root, qualifiedName) {
    const name = String(qualifiedName);
    if (name === "*" || PLAIN_LOWERCASE_TAG.test(name)) {
      return createHTMLCollection(() => [...root.querySelectorAll(name)]);
    }
    const lower = name.replace(/[A-Z]/g, (c) => c.toLowerCase());
    return createHTMLCollection(() =>
      [...root.querySelectorAll("*")].filter((element) =>
        element.localName === (element.namespaceURI === HTML_NAMESPACE ? lower : name)));
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

  // Offscreen GPUCanvasContext: the "current texture" is a texture of the canvas
  // size that the page renders into. Presenting it inside the Blitz window is a
  // later step (deno_webgpu uses wgpu 29, Blitz's GPU painter wgpu 30).
  const canvasContexts = new WeakMap();
  class OffscreenGPUCanvasContext {
    #canvas;
    #configuration = null;
    #texture = null;
    constructor(canvas) {
      this.#canvas = canvas;
    }
    get canvas() {
      return this.#canvas;
    }
    configure(configuration) {
      this.#configuration = { alphaMode: "opaque", ...configuration };
      this.#texture = null;
    }
    unconfigure() {
      this.#configuration = null;
      this.#texture = null;
    }
    getConfiguration() {
      return this.#configuration;
    }
    getCurrentTexture() {
      if (this.#configuration === null) {
        throw new DOMException("The context is not configured.", "InvalidStateError");
      }
      const { width, height } = this.#canvas;
      if (this.#texture === null || this.#texture.width !== width || this.#texture.height !== height) {
        const { device, format, usage = GPUTextureUsage.RENDER_ATTACHMENT } = this.#configuration;
        this.#texture = device.createTexture({
          size: [width, height],
          format,
          usage: usage | GPUTextureUsage.COPY_SRC | GPUTextureUsage.TEXTURE_BINDING,
        });
      }
      return this.#texture;
    }
  }
  class HTMLCanvasElement extends HTMLElement {
    get width() {
      return Number(this.getAttribute("width") ?? 300);
    }
    set width(value) {
      this.setAttribute("width", String(Math.max(0, Math.trunc(Number(value)) || 0)));
    }
    get height() {
      return Number(this.getAttribute("height") ?? 150);
    }
    set height(value) {
      this.setAttribute("height", String(Math.max(0, Math.trunc(Number(value)) || 0)));
    }
    // Only "webgpu" is available; WebGL and 2D return null (so libraries such as
    // PixiJS fall back to WebGPU).
    getContext(type) {
      if (String(type) !== "webgpu" || !navigator.gpu) return null;
      let context = canvasContexts.get(this);
      if (context === undefined) {
        context = new OffscreenGPUCanvasContext(this);
        canvasContexts.set(this, context);
      }
      return context;
    }
  }
  // Nominal interface only: the single-document policy creates no nested browsing
  // context, so there is no contentWindow/contentDocument/src navigation. It exists
  // so `x instanceof HTMLIFrameElement` works (React's getActiveElementDeep walks
  // document.activeElement while it is an iframe; with no contentWindow its
  // same-origin check throws, is caught, and the walk stops at the element).
  class HTMLIFrameElement extends HTMLElement {}
  // Capability query only: the probe has no HTML media playback backend (Deno
  // provides none), so canPlayType answers "" ("cannot play") for every type, which
  // is the spec answer for unsupported formats. Libraries such as PixiJS probe
  // formats through it; no load/play/src API is exposed.
  class HTMLMediaElement extends HTMLElement {
    canPlayType(type) {
      const prefix = "Failed to execute 'canPlayType' on 'HTMLMediaElement'";
      webidl.requiredArguments(arguments.length, 1, prefix);
      webidl.converters.DOMString(type, prefix, "Argument 1");
      return "";
    }
  }
  class HTMLAudioElement extends HTMLMediaElement {}
  class HTMLVideoElement extends HTMLMediaElement {}
  const HTML_ELEMENT_CLASSES = new Map([
    ["canvas", HTMLCanvasElement],
    ["iframe", HTMLIFrameElement],
    ["audio", HTMLAudioElement],
    ["video", HTMLVideoElement],
  ]);
  class Text extends Node {}
  class Comment extends Node {}

  class Document extends Node {
    getElementById(id) {
      const handle = op_blitz_get_element_by_id(String(id));
      return handle < 0 ? null : wrap(handle);
    }
    getElementsByTagName(qualifiedName) {
      handleOf(this, "getElementsByTagName");
      return elementsByTagName(this, qualifiedName);
    }
    createElement(name) {
      name = String(name);
      if (!VALID_NAME.test(name)) {
        throw new DOMException(`'${name}' is not a valid element name.`, "InvalidCharacterError");
      }
      return wrap(op_blitz_create_element(name.toLowerCase()));
    }
    // No case folding, even in the HTML namespace (as in browsers); the wrapper
    // class follows the namespace (HTML brands, SVGElement, plain Element).
    createElementNS(namespace, qualifiedName) {
      const prefix = "Failed to execute 'createElementNS' on 'Document'";
      webidl.requiredArguments(arguments.length, 2, prefix);
      // `DOMString? namespace`: null/undefined stay null, anything else converts.
      namespace = namespace === null || namespace === undefined
        ? null
        : webidl.converters.DOMString(namespace, prefix, "Argument 1");
      qualifiedName = webidl.converters.DOMString(qualifiedName, prefix, "Argument 2");
      const parts = validateAndExtract(namespace, qualifiedName);
      return wrap(op_blitz_create_element_ns(parts.namespace ?? "", parts.prefix ?? "", parts.localName));
    }
    get documentElement() {
      return this.firstElementChild;
    }
    // Standards mode: the root element scrolls the viewport.
    get scrollingElement() {
      return this.documentElement;
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
    createComment(data) {
      return wrap(op_blitz_create_comment(String(data)));
    }
    get readyState() {
      return documentReadyState;
    }
    get defaultView() {
      return globalThis;
    }
    get characterSet() {
      return "UTF-8";
    }
    get compatMode() {
      return "CSS1Compat";
    }
    hasFocus() {
      return true;
    }
    createTextNode(data) {
      return wrap(op_blitz_create_text_node(String(data)));
    }
  }

  function elementClass(namespace, localName) {
    if (namespace === HTML_NAMESPACE) {
      return HTML_ELEMENT_CLASSES.get(localName) ?? HTMLElement;
    }
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
      ? elementClass(op_blitz_namespace(handle), op_blitz_local_name(handle))
      : type === TEXT_NODE
      ? Text
      : type === 8
      ? Comment
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
  let documentReadyState = "loading";

  // Single-document History/Location (src/history.js).
  const createHistory = globalThis.__rovesCreateHistory;
  delete globalThis.__rovesCreateHistory;
  const historyApi = createHistory(globalThis, {
    URL,
    structuredClone,
    DOMException,
    Event,
    setTimeout: timers.setTimeout,
  });
  const createFetch = globalThis.__rovesCreateFetch;
  delete globalThis.__rovesCreateFetch;
  const fetchApi = createFetch({
    core,
    internals: globalThis.__bootstrap.internals,
    primordials: globalThis.__bootstrap.primordials,
    op_blitz_game_read,
    op_blitz_fetch_check,
    URL,
    location: historyApi.location,
  });
  // Dedicated workers (src/worker.js, src/worker.rs): own isolate and thread each.
  const createWorker = globalThis.__rovesCreateWorker;
  delete globalThis.__rovesCreateWorker;
  const workerApi = createWorker({
    core,
    webidl,
    EventTarget,
    ErrorEvent,
    MessageEvent,
    Event,
    DOMException,
    setIsTrusted,
    defineEventHandler,
    URL,
    blobFromObjectUrl,
    fetch: fetchApi.fetch,
    baseUrl: () => historyApi.location.href,
  });
  // Web Audio on web-audio-api (src/web_audio.js); its events are dispatched per frame.
  const createWebAudio = globalThis.__rovesCreateWebAudio;
  delete globalThis.__rovesCreateWebAudio;
  const webAudio = createWebAudio({
    ops: core.ops,
    webidl,
    EventTarget,
    Event,
    DOMException,
    setIsTrusted,
    setTimeout: timers.setTimeout,
    reportException,
  });
  // IndexedDB subset (src/indexeddb.js) persisted through deno_webstorage's
  // localStorage; V8 structured serialization stores the values.
  const createIndexedDB = globalThis.__rovesCreateIndexedDB;
  delete globalThis.__rovesCreateIndexedDB;
  const indexedDbApi = createIndexedDB({
    storage: () => webStorage.localStorage(),
    serialize: core.serialize,
    deserialize: core.deserialize,
    atob,
    btoa,
    Event,
    EventTarget,
    DOMException,
    setIsTrusted,
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
  // On a software GPU with a busy queue (see gpuPacingAllowsFrame) the rAF
  // queue is deferred for this tick: the callbacks stay queued in order (a
  // cancellation still removes them) and run in a later tick with that tick's
  // timestamp. Everything else in the tick still runs, and the embedder's
  // microtask checkpoint, timers and input are unaffected.
  function runAnimationFrame() {
    gpuPacingTick();
    webAudio.dispatchPendingEvents();
    if (gpuPacingAllowsFrame()) {
      gpuPacing.frames++;
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
    }
    // After the rAF callbacks, as in "update the rendering".
    processResizeObservations();
    sweepWrappers();
    gpuPacingAfterFrame();
  }

  // The global object is an EventTarget (like Window) so reported exceptions
  // reach `error` listeners.
  setEventTargetData(globalThis);
  globalThis[webidl.brand] = webidl.brand;
  Object.setPrototypeOf(globalThis, EventTarget.prototype);
  saveGlobalThisReference(globalThis);
  // deno_core's own queueMicrotask wrapper reports through this callback.
  core.setReportExceptionCallback(reportException);
  // Unhandled promise rejections become `unhandledrejection` events (as in
  // browsers) instead of terminating the runtime.
  core.setUnhandledPromiseRejectionHandler((promise, reason) => {
    const event = new PromiseRejectionEvent("unhandledrejection", { cancelable: true, promise, reason });
    if (globalThis.dispatchEvent(event)) {
      core.print(`Uncaught (in promise) ${reason?.stack ?? reason}\n`, true);
    }
    return true;
  });

  function viewportScroll(args, method, relative) {
    const root = document.documentElement;
    if (root !== null) scrollElement(root, args, method, relative);
    else scrollTarget(args, method);
  }
  function viewportOffset(index) {
    const root = document.documentElement;
    return root === null ? 0 : scrollMetric(root, index, "scrollX");
  }

  const globals = {
    Node,
    Element,
    HTMLElement,
    SVGElement,
    HTMLCanvasElement,
    HTMLIFrameElement,
    HTMLMediaElement,
    HTMLAudioElement,
    HTMLVideoElement,
    NodeList,
    HTMLCollection,
    DOMTokenList,
    MutationObserver,
    MutationRecord,
    Comment,
    MessageChannel,
    MessagePort,
    requestIdleCallback: (callback, options = {}) =>
      timers.setTimeout(() => {
        const start = performance.now();
        callback({ didTimeout: false, timeRemaining: () => Math.max(0, 16 - (performance.now() - start)) });
      }, Math.min(Number(options.timeout ?? 1), 1)),
    cancelIdleCallback: (handle) => timers.clearTimeout(handle),
    DOMRect,
    DOMRectReadOnly,
    ResizeObserver,
    ResizeObserverEntry,
    ResizeObserverSize,
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
    MessageEvent,
    EventTarget,
    AbortController,
    AbortSignal,
    DOMException,
    performance,
    getComputedStyle,
    // Window scrolling is the root element's (viewport) scrolling.
    scroll: (...args) => viewportScroll(args, "scroll", false),
    scrollTo: (...args) => viewportScroll(args, "scrollTo", false),
    scrollBy: (...args) => viewportScroll(args, "scrollBy", true),
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
    fetch: fetchApi.fetch,
    createImageBitmap: imageApi.createImageBitmap,
    ImageBitmap: imageApi.ImageBitmap,
    FormData: fetchApi.FormData,
    Headers: fetchApi.Headers,
    Request: fetchApi.Request,
    Response: fetchApi.Response,
    Blob,
    File,
    TextDecoder,
    TextEncoder,
    atob,
    btoa,
    PromiseRejectionEvent,
    History: historyApi.History,
    Location: historyApi.Location,
    PopStateEvent: historyApi.PopStateEvent,
    HashChangeEvent: historyApi.HashChangeEvent,
    ...webAudio.globals,
    ...indexedDbApi.globals,
    ...workerApi.globals,
  };
  for (const [name, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
  }
  Object.defineProperty(globalThis, "document", { value: document, enumerable: true, configurable: true });
  // Minimal navigator: WebGPU plus the fields libraries probe.
  // Workaround for deno_webgpu 0.227.0 (wgpu-core 29.0.1): as soon as a second
  // GPUDevice exists, texture creation fails with "not enough memory left" on
  // every device (reproduced on a GTX 1050). PixiJS always creates a probe device
  // in isWebGPUSupported before its real one, so each adapter hands out a single
  // shared device, created with every texture-compression feature it supports.
  let sharedDevice = null;
  // wgpu-core info of the adapter that created sharedDevice, queried while that
  // GPUAdapter was alive (see op_blitz_gpu_adapter_info) and read at teardown.
  let sharedAdapterInfo = null;
  let patchedGpu = null;
  // Teardown diagnostics: queue submissions made by the page (see shutdownGpu).
  let gpuSubmits = 0;
  let gpuCommandBuffers = 0;
  let gpuLastSubmitAt = null;
  // Software-adapter frame pacing. Run 42869197173 (forced DX12, Cpu adapter
  // "Microsoft Basic Render Driver"): the template's 238 submissions took
  // 35.7 s / 45.0 s (normal) and >120 s (JITless) to drain at teardown; run
  // 42874854967 then showed the blocking mid-frame drain that replaced it
  // stalling the host for 15 s. Now, on a Cpu adapter only, once
  // GPU_SUBMIT_BATCH submissions succeeded since the queue was last seen
  // empty, the next ticks defer the rAF queue until a NON-blocking
  // op_blitz_gpu_poll (wgpu-core PollType::Poll) reports QueueEmpty. Nothing
  // waits on the GPU mid-run: the host loop, timers, microtasks and input keep
  // running, and no rAF callback or submission is dropped. Hardware adapters
  // are never paced. A poll error is reported once and disables pacing (the
  // deferred callbacks run in that same tick); the teardown wait stays the gate.
  const GPU_SUBMIT_BATCH = 8;
  let gpuDevice = null; // the resolved shared GPUDevice
  let gpuSubmitsSinceEmpty = 0;
  let gpuPacingDeferring = false;
  let gpuPacingDisabled = false;
  let gpuPacingRun = 0; // consecutive deferred ticks of the current deferral
  let gpuPacingLastTickAt = null;
  // Fixture-only scripted poll results (gpuPacingTest hook): replaces the op,
  // so the pacing logic is tested without a GPU and without claiming one.
  let gpuPacingScript = null;
  let gpuPacing = newGpuPacingStats();
  let savedGpuPacing = null;
  function newGpuPacingStats() {
    return {
      batch: GPU_SUBMIT_BATCH,
      active: false, // pacing deferred the rAF queue at least once
      ticks: 0, // embedder frame ticks (runAnimationFrame calls)
      frames: 0, // ticks that ran the rAF queue
      deferred_ticks: 0, // ticks whose rAF queue was deferred
      deferrals: 0, // deferral episodes (busy queue after a batch)
      max_deferred_run: 0, // longest episode, in ticks
      max_tick_gap_ms: 0, // longest gap between two consecutive ticks
      polls: 0,
      poll_errors: 0,
      disabled: false,
    };
  }
  // Snapshot with the derived saturation fields: `saturated` = still deferring
  // (the queue never emptied since the last batch), `saturation` = share of
  // ticks deferred.
  function gpuPacingReport() {
    return {
      ...gpuPacing,
      saturated: gpuPacingDeferring,
      saturation: gpuPacing.ticks === 0 ? 0 : Math.round((gpuPacing.deferred_ticks / gpuPacing.ticks) * 1000) / 1000,
      pending_submits: gpuSubmitsSinceEmpty,
    };
  }
  function gpuPacingTick() {
    const now = performance.now();
    if (gpuPacingLastTickAt !== null) {
      gpuPacing.max_tick_gap_ms = Math.max(gpuPacing.max_tick_gap_ms, Math.round((now - gpuPacingLastTickAt) * 10) / 10);
    }
    gpuPacingLastTickAt = now;
    gpuPacing.ticks++;
  }
  function gpuPacingApplies() {
    if (gpuPacingDisabled) return false;
    if (gpuPacingScript !== null) return true;
    return gpuDevice !== null && sharedAdapterInfo?.device_type === "Cpu";
  }
  function pollGpuQueue() {
    if (gpuPacingScript !== null) {
      return gpuPacingScript.shift() ?? { ok: true, queue_empty: true, poll_status: "QueueEmpty" };
    }
    return JSON.parse(op_blitz_gpu_poll(gpuDevice));
  }
  // Whether this tick may run the rAF queue.
  function gpuPacingAllowsFrame() {
    if (!gpuPacingDeferring) return true;
    gpuPacing.polls++;
    const result = pollGpuQueue();
    if (!result.ok) {
      gpuPacing.poll_errors++;
      gpuPacing.disabled = true;
      gpuPacingDisabled = true;
      gpuPacingDeferring = false;
      gpuSubmitsSinceEmpty = 0;
      // `simulated`: a scripted error from the fixture regression, not a GPU.
      core.print(`ROVES_PROBE_RESULT ${JSON.stringify({ check: "gpu_backpressure", status: "poll_error", simulated: gpuPacingScript !== null, error: result.error, ...gpuPacingReport() })}\n`, false);
      return true;
    }
    if (result.queue_empty) {
      gpuPacingDeferring = false;
      gpuSubmitsSinceEmpty = 0;
      return true;
    }
    gpuPacing.deferred_ticks++;
    gpuPacingRun++;
    gpuPacing.max_deferred_run = Math.max(gpuPacing.max_deferred_run, gpuPacingRun);
    return false;
  }
  function gpuPacingAfterFrame() {
    if (gpuPacingDeferring || gpuSubmitsSinceEmpty < GPU_SUBMIT_BATCH || !gpuPacingApplies()) return;
    gpuPacingDeferring = true;
    gpuPacingRun = 0;
    gpuPacing.active = true;
    gpuPacing.deferrals++;
  }
  function countSubmits() {
    const proto = webgpu.GPUQueue?.prototype;
    const submit = proto?.submit;
    if (typeof submit !== "function") return;
    proto.submit = function (commandBuffers) {
      gpuSubmits++;
      gpuCommandBuffers += commandBuffers?.length ?? 0;
      gpuLastSubmitAt = performance.now();
      const result = submit.call(this, commandBuffers);
      // Counted for pacing only once the submission was accepted.
      gpuSubmitsSinceEmpty++;
      return result;
    };
  }
  function gpuWithSharedDevice() {
    webgpu.initGPU();
    const gpu = webgpu.gpu;
    if (patchedGpu !== null) return patchedGpu;
    installCopyExternalImage();
    countSubmits();
    const requestAdapter = gpu.requestAdapter.bind(gpu);
    gpu.requestAdapter = async (options) => {
      const adapter = await requestAdapter(options);
      if (adapter === null) return null;
      // Queried now: at teardown the adapter may already be dropped.
      const adapterInfo = JSON.parse(op_blitz_gpu_adapter_info(adapter));
      const requestDevice = adapter.requestDevice.bind(adapter);
      adapter.requestDevice = () => {
        if (sharedDevice === null) {
          sharedAdapterInfo = adapterInfo;
          const requiredFeatures = ["texture-compression-bc", "texture-compression-astc", "texture-compression-etc2"]
            .filter((feature) => adapter.features.has(feature));
          sharedDevice = requestDevice({ requiredFeatures });
          // A failed requestDevice stays the page's rejection; this branch ignores it.
          sharedDevice.then((device) => { gpuDevice = device; }, () => {});
        }
        return sharedDevice;
      };
      return adapter;
    };
    patchedGpu = gpu;
    return gpu;
  }
  const navigator = Object.freeze({
    get gpu() {
      return gpuWithSharedDevice();
    },
    userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Roves-Blitz-probe",
    platform: "Win32",
    language: "en",
    languages: ["en"],
    maxTouchPoints: 0,
    hardwareConcurrency: 4,
    // Browser-style connectivity hint, same as the Servo shell's Navigator.onLine
    // (always true): it only says the device is not known to be offline, never that
    // a remote request will succeed. Games (visual-novel-template) initialise their
    // "No internet connection" state from it, so it must not be undefined.
    onLine: true,
  });
  for (const name of ["GPUBufferUsage", "GPUTextureUsage", "GPUMapMode", "GPUShaderStage", "GPUColorWrite"]) {
    if (webgpu[name] !== undefined) {
      Object.defineProperty(globalThis, name, { value: webgpu[name], writable: true, configurable: true });
    }
  }
  Object.defineProperties(globalThis, {
    navigator: { value: navigator, enumerable: true, configurable: true },
    console: { value: new Console((message, level) => core.print(message, level > 1)), writable: true, configurable: true },
    history: { value: historyApi.history, enumerable: true, configurable: true },
    location: { get: () => historyApi.location, set: (url) => historyApi.location.assign(url), enumerable: true, configurable: true },
    localStorage: { get: () => webStorage.localStorage(), enumerable: true, configurable: true },
    sessionStorage: { get: () => webStorage.sessionStorage(), enumerable: true, configurable: true },
    indexedDB: { value: indexedDbApi.indexedDB, enumerable: true, configurable: true },
    innerWidth: { get: () => op_blitz_viewport_size(0), enumerable: true, configurable: true },
    innerHeight: { get: () => op_blitz_viewport_size(1), enumerable: true, configurable: true },
    scrollX: { get: () => viewportOffset(SCROLL_LEFT), enumerable: true, configurable: true },
    scrollY: { get: () => viewportOffset(SCROLL_TOP), enumerable: true, configurable: true },
    pageXOffset: { get: () => viewportOffset(SCROLL_LEFT), enumerable: true, configurable: true },
    pageYOffset: { get: () => viewportOffset(SCROLL_TOP), enumerable: true, configurable: true },
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
    // After the page's scripts: readyState interactive -> DOMContentLoaded ->
    // complete -> load (on the window), as a browser does.
    documentLoaded() {
      documentReadyState = "interactive";
      document.dispatchEvent(new Event("readystatechange"));
      document.dispatchEvent(new Event("DOMContentLoaded", { bubbles: true }));
      documentReadyState = "complete";
      document.dispatchEvent(new Event("readystatechange"));
      globalThis.dispatchEvent(new Event("load"));
    },
    // Navigations the single-document policy blocked (assign/replace/reload).
    blockedNavigations() {
      return JSON.stringify(historyApi.blockedNavigations);
    },
    // Frame-pacing statistics (game reports).
    gpuPacingStats() {
      return JSON.stringify(gpuPacingReport());
    },
    // Fixture-only regression of the pacing logic, driven tick by tick from
    // Rust (src/main.rs, stage gpu_frame_pacing). No GPU is involved or
    // claimed: the poll results are scripted and the batch of successful
    // submissions is simulated by setting the counter. "begin" saves the real
    // statistics, "end" restores them and returns the test's own.
    gpuPacingTest(command, argument = "[]") {
      switch (command) {
        case "begin":
          if (gpuDevice !== null) throw new Error("gpuPacingTest needs a run without a GPU device");
          savedGpuPacing = gpuPacing;
          gpuPacing = newGpuPacingStats();
          gpuPacingScript = [];
          gpuPacingLastTickAt = null;
          return "";
        case "batch": // scripted poll results for the next deferral + a full batch
          gpuPacingScript = JSON.parse(argument);
          gpuSubmitsSinceEmpty = GPU_SUBMIT_BATCH;
          return "";
        case "stats":
          return JSON.stringify({ ...gpuPacingReport(), script_left: gpuPacingScript?.length ?? 0 });
        case "end": {
          const stats = JSON.stringify(gpuPacingReport());
          gpuPacing = savedGpuPacing;
          savedGpuPacing = null;
          gpuPacingScript = null;
          gpuPacingDeferring = false;
          gpuPacingDisabled = false;
          gpuSubmitsSinceEmpty = 0;
          gpuPacingLastTickAt = null;
          return stats;
        }
        default:
          throw new Error(`unknown gpuPacingTest command ${command}`);
      }
    },
    // Orderly WebGPU teardown, awaited by the embedder before it drops the
    // runtime. wgpu-core 29's Queue Drop waits only ~6 s (100..3200 ms steps) for
    // the last successful submission and then panics, so the queue must already be
    // idle. deno_webgpu 0.227 has no GPUQueue.onSubmittedWorkDone, so the private
    // op_blitz_gpu_wait_idle polls wgpu-core natively (bounded, throws on error or
    // timeout); the shared device is destroyed only after it succeeds. Async only
    // to settle a still-pending requestDevice. Resolves to a JSON diagnostic; a
    // failure rejects and fails the run.
    async shutdownGpu() {
      if (sharedDevice === null) {
        return JSON.stringify({ device: false, submits: gpuSubmits });
      }
      let device;
      try {
        device = await sharedDevice;
      } catch (error) {
        // requestDevice failed: no queue exists, nothing was submitted.
        return JSON.stringify({ device: false, device_error: String(error), submits: gpuSubmits, adapter: sharedAdapterInfo });
      }
      // Printed before the blocking wait, so a timeout still records which
      // adapter/backend hung and what the page had submitted. The info was
      // cached when requestAdapter resolved, never re-queried here.
      const adapter = sharedAdapterInfo;
      const submissions = {
        submits: gpuSubmits,
        command_buffers: gpuCommandBuffers,
        last_submit_ms_ago: gpuLastSubmitAt === null ? null : Math.round(performance.now() - gpuLastSubmitAt),
        // Before `adapter` so CI's truncated notice keeps it.
        pacing: gpuPacingReport(),
      };
      core.print(`ROVES_PROBE_RESULT ${JSON.stringify({ check: "gpu_adapter", status: "ran", ...submissions, adapter })}\n`, false);
      const idle = JSON.parse(op_blitz_gpu_wait_idle(device));
      device.destroy();
      return JSON.stringify({ device: true, ...submissions, backend: adapter.backend, device_type: adapter.device_type, ...idle });
    },
  };
})();
