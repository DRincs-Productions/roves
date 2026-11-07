// localStorage (deno_webstorage, SQLite under the origin directory) and
// sessionStorage (memory). `storageRuns` counts processes that used the same
// directory, which CI uses to check persistence across runs.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const runs = Number(localStorage.getItem("roves-probe-runs") ?? "0") + 1;
  localStorage.setItem("roves-probe-runs", runs);
  check(localStorage.getItem("roves-probe-runs") === String(runs), "setItem coerces values to strings");
  globalThis.storageRuns = runs;

  // A save game round trip.
  const save = { scene: "chapter-1", flags: { met: true }, at: 42 };
  localStorage.setItem("save-1", JSON.stringify(save));
  check(JSON.parse(localStorage.getItem("save-1")).flags.met === true, "JSON save round trip");
  check(localStorage["save-1"] === localStorage.getItem("save-1"), "named property access");
  const keys = [];
  for (let index = 0; index < localStorage.length; index++) keys.push(localStorage.key(index));
  check(keys.includes("save-1") && keys.includes("roves-probe-runs"), `key()/length: ${keys}`);
  localStorage.removeItem("save-1");
  check(localStorage.getItem("save-1") === null, "removeItem");
  check(localStorage instanceof Storage, "Storage interface");

  sessionStorage.setItem("transient", "1");
  check(sessionStorage.getItem("transient") === "1" && sessionStorage.length === 1, "sessionStorage");
  sessionStorage.clear();
  check(sessionStorage.length === 0, "sessionStorage.clear");
})();
