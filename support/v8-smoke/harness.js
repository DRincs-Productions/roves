// Shared harness of the V8 smoke pages: `check(name, fn)` runs a synchronous or asynchronous
// check (returning true to pass, "skip" when the environment lacks the feature, anything else
// as the failure detail), after the previous one; `finish()` waits for every check and prints
// the lines run.sh reads.
"use strict";
const results = [];
// Checks run one after another, so a check installing global handlers (onerror,
// unhandledrejection) never sees another check's events.
let chain = Promise.resolve();

function check(name, fn, timeoutMs = 10000) {
  chain = chain.then(() => {
    const run = (async () => fn())();
    return Promise.race([run, new Promise(resolve => setTimeout(() => resolve("timeout"), timeoutMs))])
      .then(outcome => results.push([name, outcome]), error => results.push([name, String((error && error.stack) || error)]));
  });
}

async function finish() {
  await chain;
  let passed = 0, skipped = 0;
  for (const [name, outcome] of results) {
    if (outcome === "skip") { skipped++; console.log("SUITE SKIP " + name); continue; }
    if (outcome === true) passed++;
    console.log((outcome === true ? "SUITE PASS " : "SUITE FAIL ") + name + (outcome === true ? "" : " :: " + outcome));
  }
  console.log("SUITE TOTAL " + passed + "/" + (results.length - skipped) + (skipped ? " (" + skipped + " skipped)" : ""));
}
