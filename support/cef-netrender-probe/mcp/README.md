# Roves local MCP server (development only)

Lets an MCP client (Claude Code, an IDE agent, ...) read the state of a **running** Roves
CEF game on Windows and interact with it. Off by default: a game starts its bridge only
with `--roves-mcp`, or when its package contains the file `mcp\enabled` (MCP-enabled
development packages). Never ship an MCP-enabled package to players.

Requirements: Windows, Node.js 20 or newer. In an MCP-enabled package the dependencies are
already installed in this folder; from the repository run `npm ci` here first.

Register the stdio server in the MCP client, for example:

```json
{
  "mcpServers": {
    "roves-game": {
      "command": "node",
      "args": ["C:/Games/my-game/mcp/server.mjs"]
    }
  }
}
```

Start the game (double-click `play.exe` of the MCP-enabled package, or
`play.exe --roves-game-dir=<dist> --roves-mcp`). The server looks for running games on every
tool call, so the order does not matter. With several games open, add `"--pid", "<PID>"` to
`args` (the error message lists the PIDs).

Tools:

| Tool | What it does |
| --- | --- |
| `get_state` | Host-side state: origin, URL, title, `loaded`, `startup` (`loading`/`released`/`failed`), `input_ready`, window size in points, frame and input counters, the last 50 console lines. Runs no page script. |
| `evaluate_script` | `{ "expression": "..." }` (max 16 KiB): synchronous JavaScript in the game page, JSON result (max 64 KiB). Promises are refused; start async work in one call, read it in a later one. The page's CSP still applies to `eval` inside the expression. It can change game state: treat it as a trusted development command. |
| `click` | `{ "x": 100, "y": 60 }`: left click at a window point (CSS pixels at page zoom 1, top-left origin), through the game's normal input path. |
| `press_key` | `{ "key": "ArrowRight" }`: keydown + keyup of one of ArrowLeft/Right/Up/Down, Enter, Escape, Space, Tab, Backspace, Delete, Home, End, PageUp, PageDown. No character is typed. |
| `type_text` | `{ "text": "hello" }`: up to 256 printable characters as text input to the focused element. |

`click`, `press_key` and `type_text` are refused until the game accepts input: after the
page loaded, and for a packaged game once the startup screen showed the first game frame.

How it is protected: the game listens only on a local Windows named pipe
(`\\.\pipe\roves-mcp-<pid>-<random>`, remote clients rejected), never on a TCP port, and has
no remote-debugging port. Each connection must first send a random 256-bit token that the
game writes, with the pipe name, to `%LOCALAPPDATA%\Roves\mcp\<pid>.json` in your own
profile; the file is removed when the game closes. Any program running as your Windows user
can read that file and control the game, exactly as it could already control the game
process itself. Nothing is added to the page: the game cannot see the bridge or the token.

`node smoke.mjs` is the CI smoke test (see its header).
