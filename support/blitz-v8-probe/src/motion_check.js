// Runs after the Motion frames; see js/motion_scenario.js.
(() => {
  "use strict";
  if (motionResult.error !== null || !motionResult.started || !motionResult.finished) {
    throw new Error(`Motion animation did not finish: ${JSON.stringify(motionResult)}`);
  }
  // The start value was read through getComputedStyle; the end value is computed.
  const opacity = getComputedStyle(document.getElementById("fading")).opacity;
  if (opacity !== "0.5") {
    throw new Error(`Motion must animate opacity from the computed value to 0.5, got ${opacity}`);
  }
})();
