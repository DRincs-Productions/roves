// Records what the MCP smoke sends, for later evaluate_script reads (page-owned state).
const fixture = { ready: false, clicks: [], keys: [], typed: "" };
globalThis.rovesMcpFixture = fixture;
document.getElementById("click-target").addEventListener("click", (e) => {
  fixture.clicks.push({ trusted: e.isTrusted, button: e.button, x: e.clientX, y: e.clientY });
});
for (const type of ["keydown", "keyup"]) {
  addEventListener(type, (e) => {
    fixture.keys.push(`${e.type}:${e.key}:${e.code}:${e.isTrusted}`);
  }, true);
}
addEventListener("keypress", (e) => {
  if (e.isTrusted) fixture.typed += e.key;
}, true);
fixture.ready = true;
console.log("roves-mcp-fixture ready");
