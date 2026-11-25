// CI fixture of the CEF player's host API (README "Host API"), packaged as the game of a
// throw-away package with a persistent profile and launched three times:
//   1. no save yet  -> write "ci-slot", read it back, core/exit (the game closes itself);
//   2. save found   -> log it, write "ci-stay", core/exit;
//   3. both found   -> log, stay open so the runner can probe the routes with curl.
// Every step is one console line `roves-host-api ...` that the workflow checks in the log.
// Every launch also logs `steam available=<bool>`; with Steam (the `steam` feature package
// under the CI's GSE emulator, App ID 480) it then exercises the achievement and stat
// commands on the ids declared in the emulator's steam_settings (README "Steam").
const host = window.__ROVES__;
const log = (line) => {
  console.log(`roves-host-api ${line}`);
  document.getElementById("status").textContent = line;
};

const MUTATING = ["write", "delete", "clear", "exit", "unlock_achievement", "set_stat_int", "store_stats"];

async function call(module, cmd, { key, body, args } = {}) {
  const params = new URLSearchParams(key === undefined ? args : { key });
  const query = params.size ? `?${params}` : "";
  const response = await fetch(`${host.api}${module}/${cmd}${query}`, {
    method: body !== undefined || MUTATING.includes(cmd) ? "POST" : "GET",
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

// Steam commands of roves-api's steam.ts; ids from the CI emulator's steam_settings.
async function steam() {
  const available = await call("steam", "is_available");
  if (!available) {
    log("steam available=false");
    return;
  }
  const appId = await call("steam", "get_app_id");
  const achievement_id = "ACH_WIN_ONE_GAME";
  const unlock = await call("steam", "unlock_achievement", { args: { achievement_id } });
  const unlocked = await call("steam", "is_achievement_unlocked", { args: { achievement_id } });
  const set = await call("steam", "set_stat_int", { args: { name: "NumGames", value: 3 } });
  const stat = await call("steam", "get_stat_int", { args: { name: "NumGames" } });
  const store = await call("steam", "store_stats");
  log(`steam available=true app_id=${appId} unlock=${unlock} unlocked=${unlocked} set=${set} stat=${stat} store=${store}`);
}

async function run() {
  if (!host || host.transport !== "http" || !Object.isFrozen(host)) {
    log("marker missing");
    return;
  }
  const info = await call("core", "system_info");
  log(`system_info os_type=${info.os_type} arch=${info.architecture} engine=${info.engine_version}`);
  await steam();
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
