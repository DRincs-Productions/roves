// Motion 12.38.0 (the visual-novel-template version) on the probe DOM.
// Bundled by esbuild into dist/motion-scenario.js and run by the probe after the
// basic frame checks; the probe then drives more frames and checks the pixels.
import { animate } from "motion";

(() => {
  "use strict";
  const app = document.getElementById("app");
  const element = document.createElement("div");
  element.setAttribute("id", "motion");
  element.style.width = "40px";
  element.style.height = "40px";
  element.style.backgroundColor = "rgb(90, 120, 220)";
  app.appendChild(element);

  // No start value: Motion reads the current opacity (1) through getComputedStyle.
  const fading = document.createElement("div");
  fading.setAttribute("id", "fading");
  fading.style.width = "40px";
  fading.style.height = "40px";
  fading.style.backgroundColor = "rgb(255, 255, 255)";
  app.appendChild(fading);

  const result = { started: false, finished: false, error: null };
  globalThis.motionResult = result;
  try {
    const controls = Promise.all([
      animate(element, { x: [0, 100], opacity: [0.2, 1] }, { duration: 0.1, ease: "linear" }),
      animate(fading, { opacity: 0.5 }, { duration: 0.1, ease: "linear" }),
    ].map((animation) => animation.finished));
    result.started = true;
    controls.then(
      () => {
        result.finished = true;
      },
      (error) => {
        result.error = String(error?.stack ?? error);
      },
    );
  } catch (error) {
    result.error = String(error?.stack ?? error);
  }
})();
