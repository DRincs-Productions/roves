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

  // Attributes.
  host.setAttribute("data-x", "1");
  check(host.hasAttribute("data-x"), "hasAttribute");
  host.removeAttribute("data-x");
  check(!host.hasAttribute("data-x") && host.getAttribute("data-x") === null, "removeAttribute");
})();
