# Debug the PixiJS scene

## What works in Roves

[PixiJS Devtools](https://github.com/pixijs/devtools) has two parts: `@pixi/devtools`
(`initDevtools`), which the game calls to publish its scene on `window.__PIXI_DEVTOOLS__`,
and a Chrome extension that reads it and shows a panel. The Roves player cannot load Chrome
extensions or open a DevTools window, so the panel never appears there. The `pixi_scene` MCP
tool reads the same globals the extension reads, in the same order, and gives the agent the
scene tree:

1. `window.__PIXI_DEVTOOLS__` (`app`, `stage`, `renderer`), set by `initDevtools({ app })`.
   Pixi'VN's `Game.init` calls it in every build, release included.
2. `window.__PIXI_APP__`, then `__PIXI_STAGE__`, `__PIXI_RENDERER__`, also searched in
   same-origin frames.

A plain PixiJS game exposes its scene with either of:

```ts
import { initDevtools } from "@pixi/devtools";
initDevtools({ app }); // or, without the package: globalThis.__PIXI_APP__ = app;
```

Gate it with `isMcpEnabled()` from `@drincs/roves-api/core` or a development flag if the
release build should not expose it. Without any of these globals `pixi_scene` returns
`detected: false` with this hint. To use the real extension panel, run the same web build in
Chrome (`npm run dev`) with the extension installed.

## Read the scene

`pixi_scene` returns `source` (which global it used), `version`, `renderer` (type, resolution,
screen size, canvas backing size and its position in the window), the ticker `fps` and a flat
list of nodes. Each node has `path` (child indexes from the stage, e.g. `"2/0/3"`), `depth`,
`type`, and only the non-default fields: `label`, `class`, `visible: false`, `renderable:
false`, `alpha`, `pos`, `scale`, `rotation`, `zIndex`, `text`, `texture`, `eventMode`,
`filters`, `mask`, `children` (count) and `window` (bounds in window points, usable for
`click`).

| Goal | Call |
| --- | --- |
| Overview | `pixi_scene {}` (visible nodes, depth 3, at most 120) |
| Expand a subtree | `pixi_scene {path: "2/0", depth: 2}` |
| Find by label, text, texture or type | `pixi_scene {find: "dialogue"}` (whole tree, hidden nodes too) |
| What is under a point | `pixi_scene {at: {x: 640, y: 360}}` (topmost first) |
| Hidden nodes as well | `pixi_scene {all: true}` |

Compare with a `screenshot` of the same moment when something is missing: a node that exists
but has `visible: false`, `alpha` 0, a `window` box outside the canvas or zero size explains
most "invisible sprite" bugs; a missing `texture` or a placeholder label points at asset
loading.

## Change the scene

Edit nodes with `evaluate_script`, resolving the `path` from the stage:

```js
(() => {
  const d = window.__PIXI_DEVTOOLS__ ?? {};
  const stage = d.stage ?? d.app?.stage ?? window.__PIXI_APP__?.stage ?? window.__PIXI_STAGE__;
  const node = "1/0".split("/").reduce((n, i) => n.children[+i], stage);
  node.visible = !node.visible;            // toggle
  return { label: node.label, visible: node.visible, x: node.x, y: node.y };
})()
```

Useful properties: `visible`, `alpha`, `x`/`y`, `scale.set(...)`, `rotation`, `zIndex`,
`tint`, `eventMode`, `text` (Text nodes); for a quick highlight, `node.alpha = 0.3` or add a
temporary `new PIXI.Graphics()` outline when `__PIXI_DEVTOOLS__.pixi` is set. Read the result
with `pixi_scene` or a small `screenshot {region}`. Changes are not saved: reload the scene
(or restore a Pixi'VN snapshot) before trusting a later test.

## Renderer and textures

```js
(() => {
  const r = (window.__PIXI_DEVTOOLS__ ?? {}).renderer ?? window.__PIXI_APP__?.renderer;
  return { name: r.name, resolution: r.resolution, size: [r.width, r.height],
    textures: r.texture?.managedTextures?.length, gl: r.gl?.getParameter?.(r.gl.VERSION) };
})()
```

`managedTextures` (PixiJS 8) growing between two visits of the same screen points at a
texture leak; check it before and after a scene change.
