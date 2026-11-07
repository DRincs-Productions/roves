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

  // getComputedStyle reads Blitz/Stylo computed values, flushed on read.
  const app = document.getElementById("app");
  const box = document.getElementById("box");
  const computed = getComputedStyle(box);
  const fail = (message) => {
    throw new Error(message);
  };
  if (getComputedStyle(app).backgroundColor !== "rgb(24, 32, 48)") fail("stylesheet colour must be computed");
  if (getComputedStyle(app).getPropertyValue("padding-top") !== "12px") fail("getPropertyValue must read longhands");
  if (computed.width !== "60px") fail(`rAF style write must be computed: ${computed.width}`);
  box.style.width = "70px";
  if (computed.width !== "70px") fail("computed style must be live and flush pending writes");
  box.style.width = "60px";
  box.style.setProperty("--accent", "rgb(1, 2, 3)");
  if (computed.getPropertyValue("--accent") !== "rgb(1, 2, 3)") fail("custom properties must be readable");
  if (computed.getPropertyValue("margin") !== "") fail("shorthands are not serialized yet");
  if (getComputedStyle(box, "::before").width !== "") fail("pseudo-elements are not supported yet");
  let typeError = false;
  try {
    getComputedStyle({});
  } catch (error) {
    typeError = error instanceof TypeError;
  }
  if (!typeError) fail("getComputedStyle must reject non-elements");
})();
