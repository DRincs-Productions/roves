// Single-document History API and Location for the probe (loaded by dom.js).
//
// The game owns one document: pushState/replaceState/back/forward/go and hash
// changes are same-document entries; anything that would load another document
// (assign/replace/reload or setting href to another path) is blocked and recorded,
// like the Roves single-document policy. URLs are resolved with deno_web's URL and
// must stay same-origin (SecurityError otherwise); states use structuredClone.
// Traversal is asynchronous: popstate (and hashchange) fire from a task.
// Evaluated before dom.js, which takes the factory and removes the global.
globalThis.__rovesCreateHistory = (globalThis, dependencies) => {
  "use strict";
  const { URL, structuredClone, DOMException, Event, setTimeout } = dependencies;
  const DOCUMENT_URL = "game://content/index.html";

  class PopStateEvent extends Event {
    #state;
    constructor(type, init = {}) {
      super(type, init);
      this.#state = init.state ?? null;
    }
    get state() {
      return this.#state;
    }
  }
  class HashChangeEvent extends Event {
    #oldURL;
    #newURL;
    constructor(type, init = {}) {
      super(type, init);
      this.#oldURL = init.oldURL ?? "";
      this.#newURL = init.newURL ?? "";
    }
    get oldURL() {
      return this.#oldURL;
    }
    get newURL() {
      return this.#newURL;
    }
  }

  // Each entry keeps its scroll restoration mode (HTML: initial "auto"; pushState and
  // fragment navigations copy it from the current entry, replaceState keeps it).
  const entries = [{ url: new URL(DOCUMENT_URL), state: null, scrollRestoration: "auto" }];
  let index = 0;
  const blockedNavigations = [];
  const current = () => entries[index];

  function resolveSameOrigin(url, method) {
    const resolved = new URL(String(url), current().url);
    if (resolved.origin !== current().url.origin || resolved.protocol !== current().url.protocol) {
      throw new DOMException(`${method}: '${resolved.href}' is not same-origin with the document.`, "SecurityError");
    }
    return resolved;
  }
  function differsOnlyInHash(a, b) {
    const left = new URL(a.href);
    const right = new URL(b.href);
    left.hash = "";
    right.hash = "";
    return left.href === right.href && a.hash !== b.hash;
  }

  let constructing = false;
  class History {
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
    }
    get length() {
      return entries.length;
    }
    get state() {
      return current().state;
    }
    get scrollRestoration() {
      return current().scrollRestoration;
    }
    // WebIDL enum attribute: values other than "auto"/"manual" are ignored, not thrown.
    set scrollRestoration(value) {
      const mode = `${value}`;
      if (mode === "auto" || mode === "manual") current().scrollRestoration = mode;
    }
    pushState(state, _unused, url = null) {
      const target = url === null || url === undefined ? new URL(current().url.href) : resolveSameOrigin(url, "pushState");
      const entry = { url: target, state: structuredClone(state), scrollRestoration: current().scrollRestoration };
      entries.splice(index + 1);
      entries.push(entry);
      index = entries.length - 1;
    }
    replaceState(state, _unused, url = null) {
      const target = url === null || url === undefined ? current().url : resolveSameOrigin(url, "replaceState");
      entries[index] = { url: target, state: structuredClone(state), scrollRestoration: current().scrollRestoration };
    }
    go(delta = 0) {
      delta = Math.trunc(Number(delta)) || 0;
      if (delta === 0) {
        blockedNavigations.push({ kind: "reload", url: current().url.href });
        return;
      }
      const target = index + delta;
      if (target < 0 || target >= entries.length) return;
      const from = current();
      setTimeout(() => {
        index = target;
        const to = current();
        globalThis.dispatchEvent(new PopStateEvent("popstate", { state: to.state }));
        if (differsOnlyInHash(from.url, to.url)) {
          globalThis.dispatchEvent(new HashChangeEvent("hashchange", { oldURL: from.url.href, newURL: to.url.href }));
        }
      }, 0);
    }
    back() {
      this.go(-1);
    }
    forward() {
      this.go(1);
    }
  }

  // Same-document fragment navigation: a new entry plus an async hashchange.
  function navigateToFragment(target) {
    const { url: from, scrollRestoration } = current();
    entries.splice(index + 1);
    entries.push({ url: target, state: null, scrollRestoration });
    index = entries.length - 1;
    setTimeout(() => {
      globalThis.dispatchEvent(new HashChangeEvent("hashchange", { oldURL: from.href, newURL: target.href }));
    }, 0);
  }
  function navigate(url, kind) {
    const target = new URL(String(url), current().url);
    if (differsOnlyInHash(current().url, target)) {
      navigateToFragment(target);
      return;
    }
    // Loading another document is not available to the game.
    blockedNavigations.push({ kind, url: target.href });
  }

  class Location {
    constructor() {
      if (!constructing) throw new TypeError("Illegal constructor");
    }
    get href() {
      return current().url.href;
    }
    set href(value) {
      navigate(value, "assign");
    }
    get origin() {
      return current().url.origin;
    }
    get protocol() {
      return current().url.protocol;
    }
    get host() {
      return current().url.host;
    }
    get hostname() {
      return current().url.hostname;
    }
    get port() {
      return current().url.port;
    }
    get pathname() {
      return current().url.pathname;
    }
    get search() {
      return current().url.search;
    }
    get hash() {
      return current().url.hash;
    }
    set hash(value) {
      const target = new URL(current().url.href);
      target.hash = String(value);
      if (target.href !== current().url.href) navigateToFragment(target);
    }
    assign(url) {
      navigate(url, "assign");
    }
    replace(url) {
      navigate(url, "replace");
    }
    reload() {
      blockedNavigations.push({ kind: "reload", url: current().url.href });
    }
    toString() {
      return this.href;
    }
  }

  constructing = true;
  const history = new History();
  const location = new Location();
  constructing = false;

  return { History, Location, PopStateEvent, HashChangeEvent, history, location, blockedNavigations };
};
