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

  check(location.href === "game://content/index.html" && location.protocol === "game:", `initial URL ${location.href}`);
  check(history.length === 1 && history.state === null && history instanceof History, "initial history");

  const state = { scene: 2, flags: ["a"] };
  history.pushState(state, "", "/scene/2?x=1");
  check(location.pathname === "/scene/2" && location.search === "?x=1" && history.length === 2, `pushState URL ${location.href}`);
  check(history.state.scene === 2 && history.state !== state, "pushState stores a structured clone");

  history.replaceState({ scene: 3 }, "", "#top");
  check(location.hash === "#top" && location.pathname === "/scene/2" && history.length === 2, "replaceState");
  check(history.state.scene === 3, "replaceState state");

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
  history.back();
  check(location.hash === "#menu", "traversal is asynchronous");
})();
