# Roves local MCP server (development only)

Lets an MCP client (Claude Code, an IDE agent, ...) read the state of a **running** Roves
CEF game on Windows, macOS or Linux and interact with it. Off by default: a game starts its
bridge only with `--roves-mcp`, or when its package contains the marker file `mcp/enabled`
(MCP-enabled development packages). Never ship an MCP-enabled package to players.

Requirements: Node.js 20 or newer. In an MCP-enabled package the dependencies are already
installed in the `mcp` folder; from the repository run `npm ci` here first.

| Platform | MCP-enabled package | This folder (`server.mjs`) | Player |
| --- | --- | --- | --- |
| Windows | `roves_shell_windows_mcp.zip` | `mcp\` next to `play.exe` | `play.exe` |
| macOS | `roves_shell_macos_mcp.zip` | `<name>.app/Contents/Resources/mcp/` | the `.app` |
| Linux | `roves_shell_linux_mcp.zip` | `mcp/` next to the executable | `roves-cef-linux-gate` |

Register the stdio server in the MCP client, for example:

```json
{
  "mcpServers": {
    "roves-game": {
      "command": "node",
      "args": ["/path/to/the/package/mcp/server.mjs"]
    }
  }
}
```

Start the game (open the MCP-enabled package's player, or start any packaged player with
`--roves-mcp`; on Windows also `play.exe --roves-game-dir=<dist> --roves-mcp`). The server
looks for running games on every tool call, so the order does not matter. With several games
open, add `"--pid", "<PID>"` to `args` (the error message lists the PIDs).

Tools:

| Tool | What it does |
| --- | --- |
| `get_state` | Host-side state: origin, URL, title, `loaded`, `startup` (`loading`/`released`/`failed`), `input_ready`, window size in points, frame and input counters, the last 50 console lines. Runs no page script. |
| `evaluate_script` | `{ "expression": "..." }` (max 16 KiB): synchronous JavaScript in the game page, JSON result (max 64 KiB). Promises are refused; start async work in one call, read it in a later one. The page's CSP still applies to `eval` inside the expression. It can change game state: treat it as a trusted development command. |
| `click` | `{ "x": 100, "y": 60 }`: left click at a window point (CSS pixels at page zoom 1, top-left origin), through the game's normal input path. |
| `press_key` | `{ "key": "ArrowRight" }`: keydown + keyup of one of ArrowLeft/Right/Up/Down, Enter, Escape, Space, Tab, Backspace, Delete, Home, End, PageUp, PageDown. No character is typed. |
| `type_text` | `{ "text": "hello" }`: up to 256 printable characters as text input to the focused element. |
| `screenshot` | `{ "max_width": 640, "format": "jpeg", "quality": 60, "region": {x, y, width, height} }` (all optional): what the window shows, through CDP `Page.captureScreenshot` on the in-process DevTools channel (canvas included), scaled down to `max_width` image pixels. Returns the image plus its size, the captured region and the formula from image pixels to window points. |
| `watch` | `{ "count": 6, "interval_ms": 200, "max_width": 320, "action": {"click": {x, y}} \| {"key": "Enter"} \| {"text": "..."} }`: an optional input, then a short frame sequence (at most 20 frames, 30 s); frames identical to the previous one are listed but not returned. |
| `page_snapshot` | Visible page text plus the visible controls (buttons, links, inputs, ARIA roles, outermost `cursor: pointer` elements) with their center in window points, size, disabled state and what covers them, and the canvases. One `evaluate`. |
| `inspect_element` | `{ "selector": "..." }` or `{ "x": 10, "y": 20 }`: truncated outerHTML, box, ancestors, computed styles (a default set or `styles`), and the matching rules of same-origin stylesheets with source and `@media`/`@supports` conditions. One `evaluate`. |
| `pixi_scene` | The PixiJS scene, found like the @pixi/devtools backend does (`__PIXI_DEVTOOLS__` from `initDevtools`, then `__PIXI_APP__`, `__PIXI_STAGE__`, `__PIXI_RENDERER__`): renderer, FPS and a bounded node list (path, type, label, text, texture, transform, visibility, event mode, bounds in window points). `{ "path", "depth", "find", "at": {x, y}, "all" }`. One `evaluate`. |

The page probes (`page_snapshot`, `inspect_element`, `pixi_scene`) are fixed functions in
`server.mjs` sent through `evaluate`: they run with the page's privileges, install nothing in
the page and need nothing from the game except, for `pixi_scene`, one of the globals above.
A screenshot needs a compositor frame: a window that never produced one (no GPU frame on a
headless runner) answers `screenshot: no DevTools reply in time`.

`click`, `press_key` and `type_text` are refused until the game accepts real input: on
Windows after the page loaded, and for a packaged game once the startup screen showed the
first game frame; on macOS and Linux after the page loaded.

How it is protected: the game never listens on a TCP port and has no remote-debugging port.
It listens on one local endpoint per game process and writes a session record `<pid>.json`
(endpoint, random 256-bit token, game origin) into a per-user session directory; both are
removed when the game closes. Each connection must first send the token, otherwise it is
closed without an answer.

| Platform | Endpoint | Session directory |
| --- | --- | --- |
| Windows | named pipe `\\.\pipe\roves-mcp-<pid>-<random>` (remote clients rejected) | `%LOCALAPPDATA%\Roves\mcp` (your profile's ACL) |
| macOS | unix socket `<pid>.sock` in the session directory | `~/Library/Application Support/Roves/mcp` |
| Linux | unix socket `<pid>.sock` in the session directory | `$XDG_RUNTIME_DIR/roves/mcp`, else `~/.local/state/roves/mcp` |

On macOS and Linux the session directory is created (or tightened) to mode 0700 and must be
owned by you, the socket and the record are 0600, and the game also closes any connection
whose peer credentials name another user. On every platform any program running as your
user can read the record and control the game, exactly as it could already control the game
process itself.

The page never sees the bridge's endpoint or token. It only gets the read-only boolean
`window.__ROVES_MCP__ === true` in the top-level game document while the bridge is running,
set before the game's first script, so a game can call `isMcpEnabled()` of
`@drincs/roves-api/core` (roves-api 0.10.0 or newer) to switch on development-only test hooks. It is
`false` in a regular browser, without the bridge, if the bridge failed to start, and in
iframes. It is a hint, not a security boundary.

`node smoke.mjs` is the CI smoke test (see its header).
