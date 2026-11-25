// CI fixture of the CEF player's host API (README "Host API"), packaged as the game of a
// throw-away package with a persistent profile and launched three times:
//   1. no save yet  -> write "ci-slot", read it back, core/exit (the game closes itself);
//   2. save found   -> log it, write "ci-stay", core/exit;
//   3. both found   -> log, stay open so the runner can probe the routes with curl.
// Every step is one console line `roves-host-api ...` that the workflow checks in the log.
const host = window.__ROVES__;
const log = (line) => {
  console.log(`roves-host-api ${line}`);
  document.getElementById("status").textContent = line;
};

async function call(module, cmd, { key, body } = {}) {
  const query = key === undefined ? "" : `?key=${encodeURIComponent(key)}`;
  const response = await fetch(`${host.api}${module}/${cmd}${query}`, {
    method: body !== undefined || ["write", "delete", "clear", "exit"].includes(cmd) ? "POST" : "GET",
    headers: { "X-Roves-Api": "1" },
    body,
  });
  if (!response.ok) throw new Error(`${module}/${cmd}: HTTP ${response.status}`);
  return response.json();
}

const text = (base64) => (base64 === null ? null : new TextDecoder().decode(
  Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))));

// The player checks the game's load-end before a clean close: exit only after `load`, plus
// a margin for CEF's load-end notification to reach the host.
const loaded = new Promise((resolve) => {
  if (document.readyState === "complete") resolve();
  else window.addEventListener("load", resolve, { once: true });
}).then(() => new Promise((resolve) => setTimeout(resolve, 1500)));

async function exit() {
  await loaded;
  await call("core", "exit");
}

async function run() {
  if (!host || host.transport !== "http" || !Object.isFrozen(host)) {
    log("marker missing");
    return;
  }
  const info = await call("core", "system_info");
  log(`system_info os_type=${info.os_type} arch=${info.architecture} engine=${info.engine_version}`);
  if (!(await call("saves", "is_available"))) {
    log("saves unavailable");
    return;
  }
  const saved = text(await call("saves", "read", { key: "ci-slot" }));
  const stay = text(await call("saves", "read", { key: "ci-stay" }));
  if (saved === null) {
    const wrote = await call("saves", "write", { key: "ci-slot", body: "hello-roves" });
    const back = text(await call("saves", "read", { key: "ci-slot" }));
    log(`phase=1 write=${wrote} read=${back}`);
    await exit();
  } else if (stay === null) {
    const list = await call("saves", "list");
    log(`phase=2 read=${saved} list=${list.join(",")}`);
    await call("saves", "write", { key: "ci-stay", body: "1" });
    await exit();
  } else {
    log(`phase=3 read=${saved} staying`);
  }
}

run().catch((error) => log(`error ${error.message}`));
