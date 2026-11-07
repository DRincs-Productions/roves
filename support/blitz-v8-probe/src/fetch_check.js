// Runs after the frame loop; see fetch_test.js.
(() => {
  "use strict";
  if (!fetchResult.done) throw new Error("fetch tests did not finish");
  if (fetchResult.failures.length > 0) throw new Error(`fetch: ${fetchResult.failures.join(" | ")}`);
})();
