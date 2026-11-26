# Connect to a Roves game

The MCP bridge exists only in development builds. It never listens on a TCP port: the game
opens a private local endpoint (a named pipe on Windows, a user-only unix socket on macOS and
Linux) and writes a session record with a random token into a per-user directory.

## Get an MCP-enabled game

- Package it with the CLI: `roves bundle -c dist --enable-mcp -n my-game_mcp`. The package
  contains `mcp/server.mjs` with its dependencies and a marker that turns the bridge on.
  Never ship it to players.
- Windows only, without packaging: run any player as
  `play.exe --roves-game-dir=<absolute dist path> --roves-mcp`.

`mcp/` is next to the executable on Windows and Linux, and in
`<name>.app/Contents/Resources/mcp/` on macOS. The server needs Node.js 20 or newer.

## Register the server

```json
{
  "mcpServers": {
    "roves-game": { "command": "node", "args": ["/absolute/path/to/package/mcp/server.mjs"] }
  }
}
```

For Claude Code: `claude mcp add roves-game -- node /absolute/path/to/package/mcp/server.mjs`.

The server looks for running games on every tool call, so the game can start before or after
it. With several games open, add `"--pid", "<PID>"` to `args`. Closing the MCP client leaves
the game running; closing the game removes its session record.

## Tools

`get_state`, `evaluate_script`, `click`, `press_key`, `type_text`, `screenshot`, `watch`,
`page_snapshot`, `inspect_element`, `pixi_scene`. Coordinates are window points: CSS pixels
of the page at zoom 1, origin top-left. `press_key` takes ArrowLeft/Right/Up/Down, Enter,
Escape, Space, Tab, Backspace, Delete, Home, End, PageUp, PageDown.

## Game-side hook

The page sees only `window.__ROVES_MCP__ === true` while the bridge runs. Use
`isMcpEnabled()` from `@drincs/roves-api/core` (0.10.0 or newer) to switch on development
test hooks in an MCP build, for example Pixi'VN's `Game.testing.enable()`.
