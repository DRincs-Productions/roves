// Runs after the Motion frames; see js/motion_scenario.js.
(() => {
  "use strict";
  if (motionResult.error !== null || !motionResult.started || !motionResult.finished) {
    throw new Error(`Motion animation did not finish: ${JSON.stringify(motionResult)}`);
  }
  for (const [stage, x, opacity] of [["midpoint", 50, 0.6], ["cancelled", 0, 0.2]]) {
    const actual = motionResult[stage];
    if (!actual || !(Math.abs(actual.x - x) <= 0.5 && Math.abs(actual.opacity - opacity) <= 0.01)) {
      throw new Error(`Motion ${stage}: expected x=${x}, opacity=${opacity}, got ${JSON.stringify(actual)}`);
    }
  }
  // The start value was read through getComputedStyle; the end value is computed.
  const opacity = getComputedStyle(document.getElementById("fading")).opacity;
  if (opacity !== "0.5") {
    throw new Error(`Motion must animate opacity from the computed value to 0.5, got ${opacity}`);
  }
})();
