// CI-only opt-in for Pixi'VN's Game.testing bridge in a disposable copy of
// visual-novel-template. The template already registers its live props and
// narration actions (useTestingBridge: setProps/setActions), but only under
// import.meta.env.DEV, and vitePluginPixivn calls Game.testing.enable() only
// for `vite serve`. A static `vite build` therefore ships neither. This script
// removes the DEV gate around the template's own <TestingBridge /> and appends
// the same Game.testing.enable() call the Vite plugin injects in dev. It edits
// only the copy passed as argument and fails if the template layout changed.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const copy = process.argv[2];
assert.ok(copy && process.argv.length === 3, "Usage: node template-testing-optin.mjs <template copy>");
assert.ok(process.env.GITHUB_WORKSPACE === undefined ||
  !resolve(copy).startsWith(resolve(process.env.GITHUB_WORKSPACE)),
"Refusing to edit a template inside the workspace: pass the isolated copy");

function edit(relative, change) {
  const file = join(copy, relative);
  const before = readFileSync(file, "utf8");
  const after = change(before);
  assert.notEqual(after, before, `${relative} was not changed`);
  writeFileSync(file, after);
}

edit("src/components/providers/root-provider.tsx", (source) => {
  const gated = /\{\s*import\.meta\.env\.DEV\s*&&\s*<TestingBridge\s*\/>\s*\}/g;
  const matches = source.match(gated) ?? [];
  assert.equal(matches.length, 1, "expected exactly one DEV-gated <TestingBridge /> in root-provider.tsx");
  return source.replace(gated, "<TestingBridge />");
});

edit("src/main.tsx", (source) => {
  assert.match(source, /import\s*\{[^}]*\bGame\b[^}]*\}\s*from\s*["']@drincs\/pixi-vn["']/,
    "main.tsx must import Game from @drincs/pixi-vn");
  assert.doesNotMatch(source, /Game\.testing\.enable\(/, "main.tsx already enables Game.testing");
  return source.replace(/\s*$/, "\n\n// CI copy only (roves dev-mcp playthrough).\nGame.testing.enable();\n");
});

console.log("Game.testing opt-in applied to " + resolve(copy));
