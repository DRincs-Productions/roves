// Tree navigation, selectors, classList and innerHTML/outerHTML on the Blitz tree.
// Runs in its own hidden <section> so later layout/paint checks are unaffected.
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

  const body = document.body;
  check(body !== null && body.localName === "body", "document.body must be found by selector");
  check(document.documentElement.localName === "html", "documentElement must be <html>");
  check(document.querySelector("#app") === document.getElementById("app"), "querySelector must share wrappers");
  check(body.ownerDocument === document && document.ownerDocument === null, "ownerDocument");

  const host = document.createElement("section");
  host.setAttribute("id", "api");
  host.style.display = "none";
  check(!host.isConnected, "a created node is not connected");
  body.appendChild(host);
  check(host.isConnected, "an appended node is connected");

  // innerHTML parsing (html5ever through Blitz) and selectors (Stylo matching).
  host.innerHTML = '<ul class="list"><li class="item a">one</li><li class="item b">two</li><li class="item">three</li></ul><p>tail &amp; end</p>';
  const list = host.querySelector("ul.list");
  check(list !== null && list.localName === "ul", "innerHTML must parse elements");
  const items = host.querySelectorAll("li.item");
  check(items instanceof NodeList && items.length === 3, "querySelectorAll must return a NodeList of 3");
  check(items[0].textContent === "one" && items.item(2).textContent === "three", "NodeList index and item()");
  check([...items].map((item) => item.textContent).join() === "one,two,three", "NodeList iteration");
  check(host.querySelectorAll("table").length === 0 && host.querySelector("table") === null, "no match");
  throwsDom(() => host.querySelector("<<"), "SyntaxError", "an invalid selector must throw");

  // Navigation and identity.
  check(list.firstChild === items[0] && list.lastChild === items[2], "first/lastChild identity");
  check(items[0].nextSibling === items[1] && items[1].previousSibling === items[0], "siblings");
  check(items[2].nextSibling === null && items[0].previousSibling === null, "sibling ends");
  check(list.childNodes.length === 3 && list.children.length === 3 && list.hasChildNodes(), "childNodes/children");
  check(list.firstElementChild === items[0], "firstElementChild");
  check(host.contains(items[1]) && !items[1].contains(host) && host.contains(host), "contains");
  check(!host.contains(null), "contains(null)");

  // compareDocumentPosition and the Node constants.
  check(Node.ELEMENT_NODE === 1 && Node.TEXT_NODE === 3 && Node.DOCUMENT_NODE === 9, "Node type constants");
  check(Node.DOCUMENT_FRAGMENT_NODE === 11 && list.COMMENT_NODE === 8, "Node constants on instances");
  check(Node.DOCUMENT_POSITION_DISCONNECTED === 1 && Node.DOCUMENT_POSITION_PRECEDING === 2
    && Node.DOCUMENT_POSITION_FOLLOWING === 4 && Node.DOCUMENT_POSITION_CONTAINS === 8
    && Node.DOCUMENT_POSITION_CONTAINED_BY === 16 && Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC === 32,
    "DOCUMENT_POSITION constants");
  const constant = Object.getOwnPropertyDescriptor(Node, "ELEMENT_NODE");
  check(!constant.writable && !constant.configurable && constant.enumerable, "Node constants are read-only");
  const { DOCUMENT_POSITION_PRECEDING: PRE, DOCUMENT_POSITION_FOLLOWING: FOL } = Node;
  const { DOCUMENT_POSITION_CONTAINS: HAS, DOCUMENT_POSITION_CONTAINED_BY: IN } = Node;
  const { DOCUMENT_POSITION_DISCONNECTED: OUT, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: IMPL } = Node;
  check(items[0].compareDocumentPosition(items[0]) === 0, "compareDocumentPosition(self)");
  check(items[0].compareDocumentPosition(items[2]) === FOL && items[2].compareDocumentPosition(items[0]) === PRE,
    "sibling document order");
  const tail = host.querySelector("p");
  check(items[1].compareDocumentPosition(tail) === FOL && tail.compareDocumentPosition(items[1]) === PRE,
    "document order across subtrees");
  check(items[1].compareDocumentPosition(host) === (HAS | PRE), "ancestor: CONTAINS | PRECEDING");
  check(host.compareDocumentPosition(items[1].firstChild) === (IN | FOL), "descendant: CONTAINED_BY | FOLLOWING");
  check(document.compareDocumentPosition(items[0]) === (IN | FOL) && items[0].compareDocumentPosition(document) === (HAS | PRE),
    "document is the root ancestor");
  const loose = document.createElement("div");
  const looseChild = loose.appendChild(document.createElement("span"));
  const looseToHost = loose.compareDocumentPosition(host);
  const hostToLoose = host.compareDocumentPosition(loose);
  check((looseToHost & (OUT | IMPL)) === (OUT | IMPL) && (hostToLoose & (OUT | IMPL)) === (OUT | IMPL),
    "disconnected: DISCONNECTED | IMPLEMENTATION_SPECIFIC");
  check(((looseToHost & PRE) === 0) !== ((looseToHost & FOL) === 0), "disconnected: exactly one direction");
  check(((looseToHost & PRE) !== 0) === ((hostToLoose & FOL) !== 0), "disconnected ordering is antisymmetric");
  check(loose.compareDocumentPosition(host) === looseToHost && looseChild.compareDocumentPosition(items[0]) === looseToHost,
    "disconnected ordering is stable across calls and per tree");
  check(loose.compareDocumentPosition(looseChild) === (IN | FOL), "tree order inside a disconnected tree");
  for (const bad of [undefined, null, {}, "node"]) {
    try {
      host.compareDocumentPosition(bad);
    } catch (error) {
      check(error instanceof TypeError, `compareDocumentPosition(${String(bad)}): got ${error}`);
      continue;
    }
    throw new Error(`compareDocumentPosition(${String(bad)}): no exception`);
  }

  // classList over the class attribute; selectors see the change.
  const first = items[0];
  check(first.classList.contains("a") && first.classList.length === 2, "classList reads the attribute");
  first.classList.add("x", "a");
  check(first.className === "item a x", `classList.add: ${first.className}`);
  first.classList.remove("item");
  check(first.className === "a x", `classList.remove: ${first.className}`);
  check(first.classList.toggle("y") === true && first.classList.contains("y"), "toggle on");
  check(first.classList.toggle("y") === false && !first.classList.contains("y"), "toggle off");
  check(first.classList.replace("x", "z") && first.className === "a z", "replace");
  check(host.querySelector("li.z") === first && host.querySelector("li.item.a") === null, "selectors follow class changes");
  throwsDom(() => first.classList.add(""), "SyntaxError", "empty token");
  throwsDom(() => first.classList.add("a b"), "InvalidCharacterError", "token with whitespace");

  // insertBefore / remove / append.
  const added = document.createElement("li");
  added.textContent = "zero";
  check(list.insertBefore(added, items[0]) === added && list.firstChild === added, "insertBefore");
  list.insertBefore(items[2], added);
  check(list.firstChild === items[2] && items[2].nextSibling === added, "insertBefore moves an attached node");
  list.insertBefore(added, added);
  check(added.previousSibling === items[2] && added.nextSibling === items[0], "insertBefore(node, node) keeps place");
  const appended = list.insertBefore(document.createElement("li"), null);
  check(appended.parentNode === list && list.lastChild === appended, "insertBefore(node, null) appends");
  throwsDom(() => list.insertBefore(document.createElement("li"), host), "NotFoundError", "reference outside parent");
  throwsDom(() => items[0].insertBefore(list, null), "HierarchyRequestError", "ancestor insertion");
  items[1].remove();
  check(items[1].parentNode === null && list.children.length === 4, "remove()");
  appended.append("text", document.createElement("b"));
  check(appended.childNodes.length === 2 && appended.lastChild.localName === "b", "append()");

  // Serialization and replacement.
  const paragraph = host.querySelector("p");
  check(paragraph.innerHTML === "tail &amp; end", `innerHTML getter: ${paragraph.innerHTML}`);
  check(paragraph.outerHTML === "<p>tail &amp; end</p>", `outerHTML getter: ${paragraph.outerHTML}`);
  host.innerHTML = "<span>new</span>";
  check(paragraph.parentNode === null && paragraph.textContent === "tail & end", "referenced replaced nodes survive detached");
  check(host.children.length === 1 && host.firstChild.localName === "span", "innerHTML replaces children");
  host.innerHTML = "";
  check(!host.hasChildNodes(), "empty innerHTML clears");

  // nodeName / namespaceURI / matches / closest / dataset / cloneNode / comments.
  const probe = document.createElement("div");
  probe.setAttribute("class", "outer");
  probe.innerHTML = '<p class="inner"><b>bold</b></p>';
  const bold = probe.querySelector("b");
  check(probe.nodeName === "DIV" && bold.firstChild.nodeName === "#text" && document.nodeName === "#document", "nodeName");
  check(probe.namespaceURI === "http://www.w3.org/1999/xhtml", "namespaceURI");
  check(bold.matches("p.inner > b") && !bold.matches("i"), "matches");
  throwsDom(() => bold.matches("<<"), "SyntaxError", "matches invalid selector");
  check(bold.closest(".inner").localName === "p" && bold.closest(".missing") === null, "closest");
  probe.dataset.userName = "ada";
  check(probe.getAttribute("data-user-name") === "ada" && probe.dataset.userName === "ada", "dataset");
  const shallow = probe.cloneNode(false);
  const deep = probe.cloneNode(true);
  check(shallow !== probe && shallow.className === "outer" && !shallow.hasChildNodes(), "shallow cloneNode");
  check(deep.querySelector("b").textContent === "bold" && deep.querySelector("b") !== bold, "deep cloneNode");
  const comment = document.createComment("note");
  check(comment instanceof Comment && comment.nodeType === 8 && comment.nodeName === "#comment", "createComment");
  check(document.readyState === "loading" && document.defaultView === globalThis, "readyState / defaultView");
  check(typeof MessageChannel === "function" && typeof requestIdleCallback === "function", "MessageChannel / requestIdleCallback");

  // createElementNS: React/Pixi create SVG through it; names keep their case.
  const SVG = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(SVG, "svg");
  check(svg instanceof SVGElement && !(svg instanceof HTMLElement) && svg.localName === "svg"
    && svg.namespaceURI === SVG && svg.prefix === null && svg.tagName === "svg" && svg.parentNode === null,
    `createElementNS(svg): ${svg.tagName} ${svg.namespaceURI}`);
  const gradient = document.createElementNS(SVG, "linearGradient");
  check(gradient.localName === "linearGradient" && gradient.tagName === "linearGradient" && gradient instanceof SVGElement,
    "createElementNS keeps the local name case");
  svg.appendChild(gradient);
  check(svg.firstChild === gradient && gradient.parentNode === svg, "SVG children attach");
  const svgRecords = [];
  const svgObserver = new MutationObserver(() => {});
  svgObserver.observe(svg, { attributes: true, attributeOldValue: true });
  svg.setAttribute("viewBox", "0 0 10 10");
  check(svg.getAttribute("viewBox") === "0 0 10 10" && svg.getAttribute("viewbox") === null && svg.hasAttribute("viewBox"),
    "SVG attributes preserve case");
  svg.setAttribute("viewBox", "0 0 20 20");
  svg.removeAttribute("viewBox");
  check(!svg.hasAttribute("viewBox"), "SVG removeAttribute preserves case");
  svgRecords.push(...svgObserver.takeRecords());
  svgObserver.disconnect();
  check(svgRecords.map((record) => `${record.attributeName}:${record.oldValue}`).join() === "viewBox:null,viewBox:0 0 10 10,viewBox:0 0 20 20",
    `SVG mutation records keep case: ${svgRecords.map((record) => record.attributeName).join()}`);
  const htmlCased = document.createElement("div");
  htmlCased.setAttribute("DATA-Mixed", "x");
  check(htmlCased.getAttribute("data-mixed") === "x" && htmlCased.getAttribute("DATA-MIXED") === "x"
    && htmlCased.outerHTML.includes("data-mixed="), "HTML attributes are still lowercased");
  const nsHtml = document.createElementNS("http://www.w3.org/1999/xhtml", "VIDEO");
  check(nsHtml instanceof HTMLElement && nsHtml.localName === "VIDEO" && !(nsHtml instanceof HTMLVideoElement)
    && document.createElementNS("http://www.w3.org/1999/xhtml", "video") instanceof HTMLVideoElement,
    "createElementNS(HTML) does not fold case and still picks HTML brands");
  const MATH = "http://www.w3.org/1998/Math/MathML";
  const math = document.createElementNS(MATH, "m:mRow");
  check(math.constructor === Element && math.namespaceURI === MATH && math.prefix === "m" && math.localName === "mRow"
    && math.tagName === "m:mRow" && math.nodeName === "m:mRow", `other namespace with prefix: ${math.tagName}`);
  math.setAttribute("mathVariant", "bold");
  check(math.getAttribute("mathVariant") === "bold" && math.getAttribute("mathvariant") === null, "other namespaces preserve attribute case");
  const nullNs = document.createElementNS(null, "Plain");
  check(nullNs.namespaceURI === null && nullNs.localName === "Plain" && nullNs.constructor === Element
    && document.createElementNS("", "plain").namespaceURI === null, "null/empty namespace");
  check(document.createElementNS("http://www.w3.org/XML/1998/namespace", "xml:lang").prefix === "xml"
    && document.createElementNS("http://www.w3.org/2000/xmlns/", "xmlns").localName === "xmlns", "xml/xmlns in their namespaces");
  for (const bad of ["", "1svg", "a b", "a:", ":a", "a:b:c", "a:1b", "<svg>"]) {
    throwsDom(() => document.createElementNS(SVG, bad), "InvalidCharacterError", `invalid qualified name '${bad}'`);
  }
  throwsDom(() => document.createElementNS(null, "svg:rect"), "NamespaceError", "prefix without namespace");
  throwsDom(() => document.createElementNS(SVG, "xml:rect"), "NamespaceError", "xml prefix outside the XML namespace");
  throwsDom(() => document.createElementNS(SVG, "xmlns"), "NamespaceError", "xmlns outside the XMLNS namespace");
  throwsDom(() => document.createElementNS(SVG, "xmlns:rect"), "NamespaceError", "xmlns prefix outside the XMLNS namespace");
  throwsDom(() => document.createElementNS("http://www.w3.org/2000/xmlns/", "rect"), "NamespaceError", "XMLNS namespace without xmlns");
  // WebIDL argument handling: two required arguments, DOMString conversion
  // (Symbol throws TypeError), nullable namespace (undefined -> null).
  const throwsType = (fn, message) => {
    try {
      fn();
    } catch (error) {
      check(error instanceof TypeError, `${message}: got ${error}`);
      return;
    }
    throw new Error(`${message}: no exception`);
  };
  throwsType(() => document.createElementNS(), "createElementNS with no arguments");
  throwsType(() => document.createElementNS(SVG), "createElementNS with one argument");
  throwsType(() => document.createElementNS(SVG, Symbol("svg")), "createElementNS rejects a Symbol qualified name");
  throwsType(() => document.createElementNS(Symbol("ns"), "svg"), "createElementNS rejects a Symbol namespace");
  const undefinedNs = document.createElementNS(undefined, "Plain");
  check(undefinedNs.namespaceURI === null && undefinedNs.localName === "Plain", "undefined namespace maps to null");
  check(document.createElementNS(SVG, undefined).localName === "undefined", "undefined qualified name converts to 'undefined'");

  // HTMLIFrameElement: nominal interface only (single-document policy, no nested
  // browsing context). React's getActiveElementDeep tests activeElement against it.
  check(typeof globalThis.HTMLIFrameElement === "function" && HTMLIFrameElement.prototype instanceof HTMLElement,
    "HTMLIFrameElement global extends HTMLElement");
  const created = document.createElement("IFRAME");
  check(created instanceof HTMLIFrameElement && created instanceof HTMLElement && created.tagName === "IFRAME"
    && created.localName === "iframe" && created.nodeName === "IFRAME", "createElement('IFRAME') is an HTMLIFrameElement");
  check(document.createElement("iframe") instanceof HTMLIFrameElement, "createElement('iframe') is an HTMLIFrameElement");
  // A visible container, since focus is exercised below; no src, so nothing is fetched.
  const frames = body.appendChild(document.createElement("div"));
  frames.innerHTML = '<iframe id="frame" tabindex="0"></iframe><div id="plain" tabindex="0"></div><canvas></canvas><svg><iframe></iframe></svg>';
  const parsedFrame = frames.querySelector("#frame");
  const plain = frames.querySelector("#plain");
  check(parsedFrame instanceof HTMLIFrameElement && parsedFrame.tagName === "IFRAME", "parsed <iframe> is an HTMLIFrameElement");
  check(!(plain instanceof HTMLIFrameElement) && !(frames instanceof HTMLIFrameElement)
    && !(frames.querySelector("canvas") instanceof HTMLIFrameElement) && !(body instanceof HTMLIFrameElement)
    && !(document.documentElement instanceof HTMLIFrameElement) && !(document instanceof HTMLIFrameElement)
    && !(document.createTextNode("iframe") instanceof HTMLIFrameElement), "other nodes are not HTMLIFrameElement");
  // <iframe> is not an HTML breakout tag, so inside <svg> it stays an SVG-namespace element.
  const svgFrame = frames.querySelector("svg").firstChild;
  check(svgFrame !== null && svgFrame.namespaceURI === "http://www.w3.org/2000/svg" && svgFrame.tagName === "iframe"
    && svgFrame instanceof SVGElement && !(svgFrame instanceof HTMLIFrameElement), "only HTML-namespace iframes get the brand");
  check(!("contentWindow" in parsedFrame) && !("contentDocument" in parsedFrame) && !("src" in parsedFrame),
    "no browsing-context API on HTMLIFrameElement");
  check(parsedFrame.ownerDocument === document && document.defaultView === globalThis, "the iframe belongs to the single document");
  let illegal = null;
  try {
    new HTMLIFrameElement();
  } catch (error) {
    illegal = error;
  }
  check(illegal instanceof TypeError, "HTMLIFrameElement is not constructible");
  // The template loop (React getActiveElementDeep, `zr` in the bundle) with
  // `Fr` = activeElement || body; `win` is the global, as `e` in the bundle.
  const activeDeep = () => {
    const getActive = (doc) => doc.activeElement || doc.body;
    let win = globalThis;
    let element = getActive(win.document);
    while (element instanceof win.HTMLIFrameElement) {
      let sameOrigin;
      try {
        sameOrigin = typeof element.contentWindow.location.href === "string";
      } catch {
        sameOrigin = false;
      }
      if (!sameOrigin) break;
      win = element.contentWindow;
      element = getActive(win.document);
    }
    return element;
  };
  check(document.activeElement === body && activeDeep() === body, "activeElement deep: body when nothing is focused");
  plain.focus();
  check(document.activeElement === plain && activeDeep() === plain, "activeElement deep: focused non-iframe element");
  parsedFrame.focus();
  check(document.activeElement === parsedFrame && activeDeep() === parsedFrame,
    "activeElement deep stops at a focused iframe (no nested document)");
  parsedFrame.blur();
  check(document.activeElement === body, "blur returns activeElement to body");
  frames.remove();

  // HTMLMediaElement: capability query only (no media backend). PixiJS format
  // detection calls document.createElement("video").canPlayType(...) at startup.
  check(typeof HTMLMediaElement === "function" && HTMLMediaElement.prototype instanceof HTMLElement
    && HTMLVideoElement.prototype instanceof HTMLMediaElement && HTMLAudioElement.prototype instanceof HTMLMediaElement,
    "HTMLMediaElement / HTMLVideoElement / HTMLAudioElement globals");
  const video = document.createElement("video");
  const audio = document.createElement("AUDIO");
  check(video instanceof HTMLVideoElement && video.tagName === "VIDEO" && audio instanceof HTMLAudioElement
    && audio.tagName === "AUDIO" && !(video instanceof HTMLAudioElement), "createElement video/audio brands");
  check(typeof video.canPlayType === "function" && typeof audio.canPlayType === "function", "canPlayType is a method");
  // The Pixi probe: every format answers "" (cannot play), never throws.
  check(["video/mp4", "video/ogg", "video/webm", 'video/mp4; codecs="avc1.42E01E"', "audio/mpeg", "audio/ogg", ""]
    .every((type) => video.canPlayType(type) === "" && audio.canPlayType(type) === ""), "canPlayType returns '' for unsupported formats");
  check(video.canPlayType(undefined) === "" && video.canPlayType(null) === "", "canPlayType converts its argument to DOMString");
  let missingArgument = null;
  try {
    video.canPlayType();
  } catch (error) {
    missingArgument = error;
  }
  check(missingArgument instanceof TypeError, "canPlayType requires an argument");
  let symbolArgument = null;
  try {
    video.canPlayType(Symbol("mp4"));
  } catch (error) {
    symbolArgument = error;
  }
  check(symbolArgument instanceof TypeError, "canPlayType rejects a Symbol argument");
  host.innerHTML = '<video id="parsed-video"></video><svg><video></video></svg>';
  const parsedVideo = host.querySelector("#parsed-video");
  check(parsedVideo instanceof HTMLVideoElement && parsedVideo.canPlayType("video/webm") === "", "parsed <video> is an HTMLVideoElement");
  check(!(host.querySelector("svg").firstChild instanceof HTMLMediaElement), "only HTML-namespace media elements get the brand");
  check(!(host instanceof HTMLMediaElement) && !("play" in video) && !("src" in video) && !("load" in video),
    "no playback API on HTMLMediaElement");
  let mediaIllegal = null;
  try {
    new HTMLVideoElement();
  } catch (error) {
    mediaIllegal = error;
  }
  check(mediaIllegal instanceof TypeError, "HTMLVideoElement is not constructible");
  host.innerHTML = "";

  // Form-control brands for keyboard guards (CI 42917654548: the template's
  // TanStack Hotkeys `ignoreInputs` threw ReferenceError on HTMLInputElement at
  // every keydown). `isTypingTarget` is the template's guard, de-minified.
  check(typeof HTMLInputElement === "function" && typeof HTMLTextAreaElement === "function"
    && typeof HTMLSelectElement === "function" && HTMLInputElement.prototype instanceof HTMLElement
    && HTMLTextAreaElement.prototype instanceof HTMLElement && HTMLSelectElement.prototype instanceof HTMLElement,
    "HTMLInputElement / HTMLTextAreaElement / HTMLSelectElement globals");
  host.innerHTML = '<form><input id="f-text"><input id="f-button" type="BUTTON"><input id="f-bogus" type="bogus">'
    + '<textarea id="f-area"></textarea><select id="f-select"></select><select id="f-multi" multiple></select>'
    + '<div id="f-editable" contenteditable=""><span id="f-inner">x</span><p id="f-off" contenteditable="false">y</p></div>'
    + '<div id="f-plain">z</div></form><svg><input id="f-svg-input"></input></svg>';
  const byId = (id) => host.querySelector(`#${id}`);
  const createdInput = document.createElement("INPUT");
  check(byId("f-text") instanceof HTMLInputElement && createdInput instanceof HTMLInputElement
    && byId("f-area") instanceof HTMLTextAreaElement && document.createElement("textarea") instanceof HTMLTextAreaElement
    && byId("f-select") instanceof HTMLSelectElement && document.createElement("select") instanceof HTMLSelectElement,
    "parsed and created form controls carry their brands");
  check(!(byId("f-svg-input") instanceof HTMLInputElement) && !(byId("f-plain") instanceof HTMLInputElement)
    && !(byId("f-area") instanceof HTMLInputElement) && !(byId("f-text") instanceof HTMLTextAreaElement)
    && !(document.createElementNS(null, "input") instanceof HTMLInputElement),
    "only HTML-namespace input/textarea/select get the brands");
  check(byId("f-text").type === "text" && byId("f-button").type === "button" && byId("f-bogus").type === "text"
    && byId("f-area").type === "textarea" && byId("f-select").type === "select-one"
    && byId("f-multi").type === "select-multiple", "type reflects the attribute (missing/invalid -> text)");
  createdInput.type = "Checkbox";
  check(createdInput.getAttribute("type") === "Checkbox" && createdInput.type === "checkbox",
    "type setter writes the attribute");
  check(!("value" in createdInput) && !("selectionStart" in createdInput) && !("value" in byId("f-area")),
    "no invented editing state on form controls");
  check(byId("f-editable").isContentEditable && byId("f-inner").isContentEditable && !byId("f-off").isContentEditable
    && !byId("f-plain").isContentEditable && !byId("f-text").isContentEditable,
    "isContentEditable follows contenteditable inheritance");
  byId("f-off").setAttribute("contenteditable", "PLAINTEXT-ONLY");
  check(byId("f-off").isContentEditable, "contenteditable is ASCII case-insensitive");
  const isTypingTarget = (element) => {
    if (!element) return false;
    if (element instanceof HTMLInputElement) {
      const type = element.type.toLowerCase();
      return type !== "button" && type !== "submit" && type !== "reset";
    }
    return !!(element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement
      || element instanceof HTMLElement && element.isContentEditable);
  };
  const guarded = [];
  const guardListener = (event) => {
    guarded.push(event.composedPath().some(isTypingTarget) || isTypingTarget(event.target));
  };
  document.addEventListener("keydown", guardListener);
  for (const id of ["f-text", "f-button", "f-area", "f-select", "f-inner", "f-plain"]) {
    byId(id).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  }
  document.removeEventListener("keydown", guardListener);
  check(JSON.stringify(guarded) === "[true,false,true,true,true,false]",
    `template keyboard guard runs without throwing and ignores typing targets (${JSON.stringify(guarded)})`);
  check([globalThis, document, null].every((target) => isTypingTarget(target) === false),
    "keyboard guard on window/document/null");
  let formIllegal = null;
  try {
    new HTMLInputElement();
  } catch (error) {
    formIllegal = error;
  }
  check(formIllegal instanceof TypeError, "HTMLInputElement is not constructible");
  host.innerHTML = "";

  // getElementsByTagName: live HTMLCollection (Motion 12 calls it during init).
  host.innerHTML = '<div id="tags"><p id="p1">a</p><span><p name="second">b</p></span><svg><foreignObject></foreignObject></svg></div>';
  const tags = host.querySelector("#tags");
  const paragraphs = tags.getElementsByTagName("p");
  check(paragraphs instanceof HTMLCollection && !(paragraphs instanceof NodeList), "getElementsByTagName returns an HTMLCollection");
  check(Object.prototype.toString.call(paragraphs) === "[object HTMLCollection]", "HTMLCollection toStringTag");
  check(paragraphs.length === 2 && paragraphs[0].id === "p1" && paragraphs.item(1).textContent === "b", "single tag in tree order");
  check(paragraphs[2] === undefined && paragraphs.item(2) === null && !(2 in paragraphs) && 1 in paragraphs, "out of range");
  check(paragraphs.namedItem("p1") === paragraphs[0] && paragraphs.namedItem("second") === paragraphs[1], "namedItem by id/name");
  check(paragraphs.namedItem("") === null && paragraphs.namedItem("missing") === null, "namedItem miss");
  check([...paragraphs].length === 2 && Object.keys(paragraphs).join() === "0,1", "iteration and indexed keys");
  check(tags.getElementsByTagName("P").length === 2, "HTML elements match the lowercased name");
  check(tags.getElementsByTagName("foreignObject").length === 1 && tags.getElementsByTagName("foreignobject").length === 0,
    "non-HTML elements match the exact name");
  check(tags.getElementsByTagName("table").length === 0, "no match is empty");
  const all = tags.getElementsByTagName("*");
  check(all.length === 5 && all[0].localName === "p" && all[1].localName === "span", `wildcard descendants in tree order: ${all.length}`);
  check(!Array.prototype.includes.call(all, tags), "the root itself is excluded");
  const heads = document.getElementsByTagName("head");
  check(heads.length === 1 && heads[0] === document.querySelector("head"), "document.getElementsByTagName");
  check(document.getElementsByTagName("*")[0] === document.documentElement, "document wildcard starts at <html>");
  // Liveness: later insertions and removals are reflected by the same object.
  const later = document.createElement("p");
  tags.appendChild(later);
  check(paragraphs.length === 3 && paragraphs[2] === later && all.length === 6, "live after appendChild");
  tags.firstChild.remove();
  check(paragraphs.length === 2 && paragraphs[0].textContent === "b", "live after remove");
  tags.querySelector("span").innerHTML = "<p>c</p><p>d</p>";
  check(paragraphs.length === 3 && paragraphs[1].textContent === "d", "live after innerHTML");
  const detached = document.createElement("div");
  const detachedParagraphs = detached.getElementsByTagName("p");
  check(detachedParagraphs.length === 0, "detached root starts empty");
  detached.appendChild(document.createElement("p"));
  check(detachedParagraphs.length === 1, "live on a detached root");
  tags.innerHTML = "";
  check(paragraphs.length === 0 && all.length === 0, "live after clearing the root");
  host.innerHTML = "";

  // Attributes.
  host.setAttribute("data-x", "1");
  check(host.hasAttribute("data-x"), "hasAttribute");
  host.removeAttribute("data-x");
  check(!host.hasAttribute("data-x") && host.getAttribute("data-x") === null, "removeAttribute");

  // AbortController/AbortSignal (deno_web 03_abort_signal.js). Pixi'VN narration
  // hooks create one at startup. Scoped checks, not full spec coverage.
  check(typeof AbortController === "function" && typeof AbortSignal === "function", "AbortController/AbortSignal globals");
  const controller = new AbortController();
  const signal = controller.signal;
  check(signal instanceof AbortSignal && signal instanceof EventTarget, "controller.signal is an AbortSignal");
  check(controller.signal === signal, "controller.signal identity");
  check(signal.aborted === false && signal.reason === undefined, "signal starts not aborted");
  signal.throwIfAborted();
  let abortEvents = 0;
  let handlerCalls = 0;
  let eventSeen = null;
  signal.addEventListener("abort", (event) => {
    abortEvents += 1;
    eventSeen = event;
  });
  signal.onabort = () => {
    handlerCalls += 1;
  };
  // An Element listener registered with { signal } is removed on abort.
  let clicks = 0;
  host.addEventListener("click", () => (clicks += 1), { signal });
  host.dispatchEvent(new Event("click"));
  check(clicks === 1, "listener with a live signal fires");
  const reason = new Error("stop");
  controller.abort(reason);
  check(signal.aborted === true && signal.reason === reason, "abort sets aborted and reason");
  check(abortEvents === 1 && handlerCalls === 1, `abort event fires once: ${abortEvents}/${handlerCalls}`);
  check(eventSeen instanceof Event && eventSeen.type === "abort" && eventSeen.target === signal, "abort event type/target");
  controller.abort(new Error("again"));
  check(abortEvents === 1 && signal.reason === reason, "a second abort is a no-op");
  try {
    signal.throwIfAborted();
    throw new Error("throwIfAborted must throw after abort");
  } catch (error) {
    check(error === reason, `throwIfAborted throws the reason: ${error}`);
  }
  host.dispatchEvent(new Event("click"));
  check(clicks === 1, "listener with an aborted signal is removed");
  const defaultReason = new AbortController();
  defaultReason.abort();
  check(defaultReason.signal.reason instanceof DOMException && defaultReason.signal.reason.name === "AbortError", "default reason is an AbortError");
  const preAborted = AbortSignal.abort("why");
  check(preAborted.aborted && preAborted.reason === "why", "AbortSignal.abort()");
  const combined = AbortSignal.any([new AbortController().signal, preAborted]);
  check(combined.aborted && combined.reason === "why", "AbortSignal.any() follows an aborted input");
  let threw = false;
  try {
    new AbortSignal();
  } catch (error) {
    threw = error instanceof TypeError;
  }
  check(threw, "AbortSignal is not constructible");
})();
