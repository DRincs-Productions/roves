---
name: roves-diagnostics
description: Diagnose a running Roves desktop game (CEF player) through its local MCP bridge - host state, startup and load status, input readiness, recent console errors, JavaScript evaluation in the page - and measure frame timing against Chrome. Use when a packaged Roves game shows a blank or stuck window, errors, refuses input or stutters. To play or visually test the game, use roves-playtest instead.
---

# Diagnose a Roves game

The Roves desktop player has an optional local MCP bridge (development builds only). Connect
it with the [connection guide](references/mcp.md); for stutter, follow the
[performance guide](references/performance.md).

## First look: `get_state`

Call `get_state` before anything else. It runs no page script, so it answers even when the
page is broken or busy.

| Field | What it tells you |
| --- | --- |
| `loaded` | The game document finished loading. `false` for long: a load error or a stuck server request. |
| `startup` | `loading` (startup screen still up), `released` (first game frame shown), `failed`. |
| `input_ready` | Whether `click`/`press_key`/`type_text` are accepted yet. |
| `page_capability` | `registered` when the page received `window.__ROVES_MCP__` (`isMcpEnabled()`). |
| `window` | Size in points and pixels, display `scale`, `suspended` (minimized). |
| `frames` | Frames imported (and presented on Windows). Not growing: nothing is being drawn. |
| `input` | Forwarded, rejected and discarded input events. |
| `console.recent` | The last 50 console lines with level, source and line. Read errors first. |

## Then narrow it down

- **Console errors**: open the cited source (`source`, `line`) in the game's build.
- **Page state**: `evaluate_script` with a synchronous expression returning JSON, for example
  `({ url: location.href, ready: document.readyState, errors: window.pixiVN?.errors?.length })`.
  Promises are refused: start async work in one call (store the result on `globalThis`) and
  read it in a later call. The page CSP still applies to `eval` inside the expression.
- **What is on screen**: one `screenshot` with a small `max_width` (320-640) or
  `page_snapshot` for DOM text and controls. The `roves-playtest` skill covers these tools,
  HTML/CSS inspection and the PixiJS scene in depth.

Bridge errors and what they mean:

| Error | Meaning |
| --- | --- |
| `No Roves game with MCP enabled is running` | Game not started, started without MCP, or already closed. |
| `Several Roves games are running` | Restart the server with `--pid <PID>` (the message lists them). |
| `the game page is not loaded yet` | Wait and call `get_state` again. |
| `input is not ready` | The startup screen has not shown the first game frame yet. |
| `... no DevTools reply in time` | The page is blocked (long script, debugger) or, for a screenshot, no frame was ever composited. |

Clicks, keys and text from the bridge travel the player's real input path, so the page sees
trusted events. `evaluate_script` changes state directly: record when a reproduction used it.
