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
// A duck-typed PixiJS stage for the pixi_scene probe (no renderer: bounds are window points).
const pixiNode = (label, x, y, width, height, extra = {}) => ({
  label, x, y, visible: true, alpha: 1, children: [], ...extra,
  getBounds: () => ({ x, y, width, height }),
});
globalThis.__PIXI_STAGE__ = pixiNode("fixture-stage", 0, 0, 200, 120, { children: [
  pixiNode("fixture-sprite", 10, 20, 50, 40, { texture: { label: "fixture-texture" } }),
  pixiNode("hidden-layer", 0, 0, 200, 120, { visible: false }),
] });
fixture.ready = true;
console.log("roves-mcp-fixture ready");
