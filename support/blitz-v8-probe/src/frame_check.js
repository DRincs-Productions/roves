// Runs after the headless frame loop; see frame_test.js.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(`${message}: ${JSON.stringify(frameResults())}`);
  };
  const result = frameResults();
  check(result.unexpected.length === 0, "frame callbacks must not throw");
  check(result.frames === 5, "the rAF chain must run once per frame for 5 frames");
  check(result.first === result.second, "callbacks of one frame must share the timestamp");
  check(result.nested > result.first, "a callback requested during a frame must run in a later frame");
  check(!result.log.includes("cancelled ran"), "a cancelled handle must not run");
  check(result.reported, "a throwing callback must reach error listeners");
  check(result.log.includes("after boom"), "a throwing callback must not stop later callbacks");
  check(result.timerFired, "setTimeout must fire while frames are driven");
})();
