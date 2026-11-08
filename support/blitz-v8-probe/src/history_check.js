// Runs after the frame loop (timers ran); see history_test.js.
(() => {
  "use strict";
  const expected = ['hashchange:#menu', 'popstate:{"scene":3}:/scene/2#top', "hashchange:#top"].join();
  if (historyEvents.join() !== expected) {
    throw new Error(`history events: ${historyEvents.join()} (expected ${expected})`);
  }
  if (location.hash !== "#top" || history.length !== 3 || history.state.scene !== 3) {
    throw new Error(`after back(): ${location.href}, length ${history.length}`);
  }
  // Each entry keeps its own mode: #menu was set to manual, #top stays auto.
  if (history.scrollRestoration !== "auto") {
    throw new Error(`after back(): scrollRestoration ${history.scrollRestoration} (expected auto)`);
  }
})();
