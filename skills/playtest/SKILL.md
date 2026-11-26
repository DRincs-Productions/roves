---
name: roves-playtest
description: Play and test a running Roves desktop game as an AI player through its local MCP bridge - see what the window shows (screenshots, short frame sequences), read the page as text, click, press keys and type, inspect the current HTML and CSS, and drive Pixi'VN games through Game.testing. Use for agent playtesting, reproducing a bug by playing, checking a UI change or a scene visually, or exploring a game. For PixiJS scene debugging read references/pixijs-devtools.md. For startup, console or performance problems use roves-diagnostics.
---

# Playtest a Roves game

You play through the tools of the game's local MCP bridge (an MCP-enabled build). To connect,
package with `roves bundle -c dist --enable-mcp` and register `node <package>/mcp/server.mjs`
as a stdio MCP server; on Windows `play.exe --roves-game-dir=<dist> --roves-mcp` also works.
The `roves-diagnostics` skill has the full connection guide. Check `get_state` first:
`input_ready` must be `true` before input is accepted.

## The loop: observe cheaply, act, verify

1. **Observe with text first.** `page_snapshot` returns visible text and every visible
   control with its center (`x`, `y`) in window points, ready for `click`. For a Pixi'VN game,
   `evaluate_script` with `window.pixiVN?.getState()` returns the dialogue, choices and what
   is allowed now. For canvas content, `pixi_scene` lists the scene nodes with their window
   bounds (see [PixiJS devtools](references/pixijs-devtools.md)). Text costs far fewer tokens
   than images.
2. **Look when it matters.** `screenshot` shows exactly what the player sees, canvas
   included. Its cost grows with the image area (about width x height / 750 tokens): keep
   `max_width` at 320-640 for an overview and capture a `region` at a larger size to read
   small text or check details. The result says how to map image pixels to window points.
3. **Act.** `click {x, y}`, `press_key {key}` (arrows, Enter, Escape, Space, Tab, Backspace,
   Delete, Home, End, PageUp, PageDown) and `type_text {text}` go through the player's real
   input path, so the game receives trusted events like a human's.
4. **Verify the effect** with the cheapest observation that proves it, then repeat.

## Seeing motion in (pseudo) real time

There is no video stream; a tool call is a snapshot. `watch` gets as close as possible: it
sends one optional input (`action: {click: {x, y}}`, `{key}` or `{text}`) and immediately
captures `count` frames `interval_ms` apart, with real timestamps. Frames identical to the
previous one come back only as `same_as`, so an idle screen costs nothing extra.

- Transition or animation after a click: `watch {action: {click: {x: 640, y: 600}}, count: 6, interval_ms: 150, max_width: 320}`.
- Typewriter text: `watch {count: 8, interval_ms: 250, region: {...dialogue box...}}`.
- Waiting for something to appear: poll `page_snapshot` or a small `evaluate_script` instead
  of repeated screenshots; take one screenshot when the text says it is there.

Each capture is a round trip to the game (tens of milliseconds), so `interval_ms` is a
minimum and `watch` is not a frame-timing measurement.

## Inspect HTML and CSS

- `inspect_element {selector}` or `{x, y}` (the topmost element at that point): truncated
  outerHTML, box, ancestors, what covers it (`covered_by`), computed styles (pass `styles` to
  choose properties) and the matching rules of the page's stylesheets with their source and
  `@media`/`@supports` conditions. Use `{x, y}` when a click "does nothing": it shows which
  element actually receives it.
- For anything else, `evaluate_script` runs a synchronous expression in the page and returns
  JSON (max 64 KiB): `document.querySelector('#hud').outerHTML.slice(0, 2000)`,
  `getComputedStyle(el).zIndex`, `[...document.querySelectorAll('[data-testid]')].map(e => e.dataset.testid)`.
  Promises are refused: start async work in one call, store the result on `globalThis` and
  read it in a later call.

## Pixi'VN games: play through `Game.testing`

Pixi'VN's testing bridge (`window.pixiVN`) is on only under `vite dev`. In an MCP build the
game can enable it itself:

```ts
import { isMcpEnabled } from "@drincs/roves-api/core"; // 0.10.0 or newer
if (import.meta.env.DEV || isMcpEnabled()) Game.testing.enable();
```

Then: `window.pixiVN.getState()` to decide, `window.pixiVN.selectChoice(i)` for a choice,
`setInput(value)` before continuing on an input prompt, `window.pixiVN.errors` after each
step. Their actions are async: call them without awaiting (for example
`void window.pixiVN.continue()`), then read `getState()` in the next call. Prefer clicking the
real UI when the test is about the UI; use `pixiVN` to reach a scene quickly or to assert
state. Snapshots: `globalThis.__save = window.pixiVN.Game.exportGameState()` and later
`void window.pixiVN.Game.restoreGameState(globalThis.__save)`.

## Report what you did

Record each action, what you observed and how (text, screenshot, scene), and whether a step
used `evaluate_script` to change state: a bug reached that way may not be reachable by a
player. Name the game build and the window size from `get_state`.
