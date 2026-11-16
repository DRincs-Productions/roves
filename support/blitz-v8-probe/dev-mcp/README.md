# Optional Roves development MCP

This MCP server starts one native Blitz probe using the existing V8 game
context. Build the probe with `--features dev-mcp` and install this directory's
dependencies with `npm ci` (Node 20 or newer). Pass two absolute paths when
registering the stdio server in your MCP client:

```json
{
  "command": "node",
  "args": [
    "C:/path/to/roves/support/blitz-v8-probe/dev-mcp/server.mjs",
    "C:/path/to/roves-blitz-v8-probe.exe",
    "C:/path/to/game/dist"
  ]
}
```

The game opens on the first call to `evaluate_script`. For example, pass
`{"source":"({ url: location.href, text: document.body.textContent.trim().slice(0, 200) })"}`.
The tool returns JSON from synchronous expressions, with a 16 KiB limit for
source and inspector responses. To observe asynchronous work, schedule it
with one call and read its state with a later call. Closing the MCP connection
closes this game instance and removes its temporary storage.

Call `performance_sample` with `{}` to read frame timings from the previous
five seconds. Durations are in milliseconds. Each series reports its sample
count, percentiles and lifetime count. Presentation and skip counters, when
available, cover the session. Displayed-frame counts and GPU execution times
are unavailable through this tool. Use a PresentMon capture on Windows to
measure displayed frames.

The normal probe build excludes the bridge. Even a feature-enabled probe
requires a per-process token on a game-window launch. The server generates that
token and supplies a fresh storage directory. Game bundles need no extra
scripts, UI elements or APIs. The bridge uses private process pipes and the
embedded Deno inspector, without opening an HTTP or WebSocket listener.
Treat calls as trusted development commands since they can change game state.

## Attach to an open probe window

Start the feature-enabled probe with `--game <dist> --window`. Set
`ROVES_PROBE_DEV_MCP_ATTACH_PORT` to a free port and
`ROVES_PROBE_DEV_MCP_TOKEN` to a fresh secret of 32 hexadecimal characters
before launch. Use a distinct port and token for each instance. Set
`ROVES_PROBE_FRAME_TIMINGS=1` to enable performance sampling. The endpoint
listens on IPv4 loopback once the game window starts polling.

Configure the MCP client to run `node server.mjs --attach <port>` with the
matching token in the server environment. Closing the client leaves the
game running. To keep saves separate, set `ROVES_PROBE_STORAGE_DIR` to a
dedicated directory when launching the game. This attach mode uses the
storage configured by the game launcher.

Run `node smoke.mjs --list-only` after `npm ci` to check MCP tool discovery.
Run `node smoke.mjs <probe executable>` to check evaluation, error handling, and
shutdown against the included HTML fixture in a real window.

## Chromium reference

Chromium `f2700724f59fb554eb19a4f827b1d256e2a13286` routes protocol messages
through `content/browser/devtools/devtools_session.cc` to a renderer agent.
This probe already embeds the page and the inspector in one process, so it
dispatches the same CDP `Runtime.evaluate` command through
`deno_core::JsRuntimeInspector`. The official MCP TypeScript SDK handles the
stdio protocol. Rust adds the pipe handoff to the V8 thread without adding
Cargo dependencies.
