// CI fixture of the macOS and Linux players (README "Player fixture"), packaged as the game
// in place of the template for one persistent-profile run with ROVES_GATE_TEST_INPUT_MS.
// Every check is one console line the workflow looks for in the gate log:
//   roves-ci-marker ...         window.__ROVES__ (roves-api isAvailable()) before the game ran
//   roves-ci-save ...           host API system_info + a save written and read back
//   roves-test-click-received   the synthetic left click (game_rules TEST_CLICK_MARKER)
//   roves-test-key-received     keydown/keypress/keyup (TEST_KEY_MARKER; ArrowRight, no keypress)
//   roves-test-wheel-received   the synthetic wheel notch (TEST_WHEEL_MARKER)
//   roves-ci-audio ...          a WebAudio oscillator started by the click (user activation)
//   roves-ci-external ...       external links: the click navigates the main frame off-origin
//                               (vetoed by the player, handed to the system browser) and,
//                               within the activation window, opens a popup; a popup opened
//                               at load without a gesture must be refused. The document
//                               must survive (same href afterwards).
const host = window.__ROVES__;
const available = !!host && host.transport === "http" && host.api === "/__roves/" && Object.isFrozen(host);
console.log(`roves-ci-marker available=${available}`);

const status = (line) => {
  document.getElementById("status").textContent = line;
};

async function call(module, cmd, { key, body } = {}) {
  const query = key === undefined ? "" : `?${new URLSearchParams({ key })}`;
  const response = await fetch(`${host.api}${module}/${cmd}${query}`, {
    method: body !== undefined ? "POST" : "GET",
    headers: { "X-Roves-Api": "1" },
    body,
  });
  if (!response.ok) throw new Error(`${module}/${cmd}: HTTP ${response.status}`);
  return response.json();
}

const text = (base64) => (base64 === null ? null : new TextDecoder().decode(
  Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))));

async function saves() {
  if (!available) return;
  const info = await call("core", "system_info");
  const ok = await call("saves", "is_available");
  const wrote = ok ? await call("saves", "write", { key: "ci-player", body: "hello-roves" }) : false;
  const read = ok ? text(await call("saves", "read", { key: "ci-player" })) : null;
  console.log(`roves-ci-save os_type=${info.os_type} available=${ok} write=${wrote} read=${read}`);
}
saves().catch((error) => console.log(`roves-ci-save error ${error.message}`));

let audio = null;
function startAudio() {
  if (audio) return;
  audio = new AudioContext();
  const oscillator = audio.createOscillator();
  const gain = audio.createGain();
  gain.gain.value = 0.05;
  oscillator.connect(gain).connect(audio.destination);
  oscillator.start();
  audio.resume().then(
    () => console.log(`roves-ci-audio state=${audio.state} sampleRate=${audio.sampleRate}`),
    (error) => console.log(`roves-ci-audio error ${error.message}`));
  setTimeout(() => console.log(
    `roves-ci-audio later state=${audio.state} currentTime=${audio.currentTime.toFixed(2)}`), 1500);
}

// Distinct hosts, because the player logs external links as scheme + host only.
let externalDone = false;
function externalLinks() {
  if (externalDone) return;
  externalDone = true;
  const before = location.href;
  // Same-tab off-origin navigation with the click's user activation (not consumed by it).
  location.href = "https://example.com/roves-ci-navigation?token=secret";
  // Later than the player's 1 s external-open interval, inside Chromium's 5 s activation.
  setTimeout(() => {
    const popup = window.open("https://example.org/roves-ci-popup?token=secret", "_blank");
    console.log(`roves-ci-external popup-returned=${popup === null ? "null" : "window"}`);
  }, 2000);
  setTimeout(() => console.log(
    `roves-ci-external still-here same-href=${location.href === before}`), 2500);
}

document.getElementById("click-target").addEventListener("click", (e) => {
  console.log(`roves-test-click-received target=${e.target.id} trusted=${e.isTrusted} button=${e.button} detail=${e.detail} x=${e.clientX} y=${e.clientY}`);
  status("clicked");
  startAudio();
  externalLinks();
});
const key = (e) => {
  console.log(`roves-test-key-received type=${e.type} key=${e.key} code=${e.code} keyCode=${e.keyCode} repeat=${e.repeat} trusted=${e.isTrusted} target=${e.target.nodeName}`);
};
for (const type of ["keydown", "keypress", "keyup"]) {
  window.addEventListener(type, key, true);
}
window.addEventListener("wheel", (e) => {
  e.preventDefault();
  console.log(`roves-test-wheel-received type=${e.type} deltaX=${e.deltaX} deltaY=${e.deltaY} deltaZ=${e.deltaZ} deltaMode=${e.deltaMode} trusted=${e.isTrusted} target=${e.target.nodeName}`);
}, { passive: false });
// No user gesture: the player must refuse it (if Chromium reports it at all).
window.open("https://example.net/roves-ci-no-gesture", "_blank");
console.log("roves-ci-player-fixture module-ok");
