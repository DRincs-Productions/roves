// Single-document History/Location; history_check.js verifies the async events.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const throws = (fn, name, message) => {
    try {
      fn();
    } catch (error) {
      check(name === undefined || error.name === name, `${message}: got ${error}`);
      return;
    }
    throw new Error(`${message}: no exception`);
  };

  // Regression (CI 42672941726): the document URL is the content root, so SPA routers
  // match their "/" route instead of rendering "Not Found" for "/index.html".
  check(location.href === "game://content/" && location.protocol === "game:", `initial URL ${location.href}`);
  check(location.pathname === "/" && location.search === "" && location.hash === "", `initial pathname ${location.pathname}`);
  check(new URL("./assets/a.js", location.href).href === "game://content/assets/a.js", "relative URLs resolve against the content root");
  // Regression: the template reads navigator.onLine to decide whether to show its
  // "No internet connection" notice; it was undefined (falsy).
  check(navigator.onLine === true, `navigator.onLine ${navigator.onLine}`);
  check(history.length === 1 && history.state === null && history instanceof History, "initial history");

  // Regression: the original template assigns scrollRestoration in strict-mode code,
  // which threw "only a getter" before the setter existed.
  const restoration = Object.getOwnPropertyDescriptor(History.prototype, "scrollRestoration");
  check(typeof restoration?.get === "function" && typeof restoration.set === "function", "scrollRestoration accessor");
  check(history.scrollRestoration === "auto", `initial scrollRestoration ${history.scrollRestoration}`);
  history.scrollRestoration = "manual";
  check(history.scrollRestoration === "manual", "scrollRestoration = manual");
  for (const invalid of ["smooth", "", "AUTO", null, undefined]) history.scrollRestoration = invalid;
  check(history.scrollRestoration === "manual", "invalid scrollRestoration values are ignored");
  throws(() => (history.scrollRestoration = Symbol("auto")), "TypeError", "Symbol scrollRestoration");

  const state = { scene: 2, flags: ["a"] };
  history.pushState(state, "", "/scene/2?x=1");
  check(history.scrollRestoration === "manual", "pushState copies scrollRestoration");
  history.scrollRestoration = "auto";
  check(location.pathname === "/scene/2" && location.search === "?x=1" && history.length === 2, `pushState URL ${location.href}`);
  check(history.state.scene === 2 && history.state !== state, "pushState stores a structured clone");

  history.replaceState({ scene: 3 }, "", "#top");
  check(location.hash === "#top" && location.pathname === "/scene/2" && history.length === 2, "replaceState");
  check(history.state.scene === 3 && history.scrollRestoration === "auto", "replaceState state and scrollRestoration");

  throws(() => history.pushState(null, "", "https://example.com/"), "SecurityError", "cross-origin pushState");
  throws(() => history.pushState({ callback() {} }, ""), undefined, "uncloneable state");

  const before = location.href;
  location.assign("/other.html");
  location.reload();
  history.go(0);
  check(location.href === before, "loading another document is blocked");

  const events = [];
  globalThis.historyEvents = events;
  addEventListener("popstate", (event) => {
    events.push(`popstate:${JSON.stringify(event.state)}:${location.pathname}${location.hash}`);
  });
  addEventListener("hashchange", (event) => {
    events.push(`hashchange:${new URL(event.newURL).hash}`);
  });

  location.hash = "menu";
  check(history.length === 3 && location.hash === "#menu", "a fragment change adds an entry");
  check(history.scrollRestoration === "auto", "a fragment entry copies scrollRestoration");
  history.scrollRestoration = "manual";
  history.back();
  check(location.hash === "#menu", "traversal is asynchronous");
})();
