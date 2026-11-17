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

On Windows, add `--mcp-discover` to the feature-enabled probe's
`--game <dist> --window` command. The probe selects a loopback port and
publishes a session record under `%LOCALAPPDATA%\Roves\mcp`. Configure
your client with `node server.mjs --discover`. If multiple games are open,
add the PID indicated by the server after `--discover`. The server rereads
session records on each tool call. Frame timing collection is enabled in
this mode.

An explicit endpoint is also available for development:

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

## Downloadable Windows bundle (experimental)

A `blitz-probe.yml` run dispatched with `dev_mcp`, or a push whose head
commit message contains `[dev-mcp]`, publishes the `roves-dev-mcp-windows`
artifact from the normal leg. `package-windows.ps1` stages it. Ordinary
probe artifacts do not change. The artifact contains:

| Path | Content |
| --- | --- |
| `roves-blitz-v8-probe.exe` | debug probe built with `--features dev-mcp` |
| `node/node.exe`, `node/LICENSE` | the Node runtime used by the CI smoke test (Node 22) |
| `mcp/` | this sidecar, its README, `smoke.mjs`, the demo `fixture/` and its locked production `node_modules/` |
| `deno-extension-sources/` | JS/TS sources and licences of the locked `deno_*` crates |
| `bundle-info.json` | commit, Node version and crate list |

Both legs smoke-test the staged copy with its own `node.exe`, in launch mode
and in `--discover` mode, after moving it to a folder whose path contains
spaces. During that test, `C:\roves-cargo\registry\src` is renamed away, so
the executable cannot read the build machine's registry. Only the normal leg uploads the
bundle. The executable is the same in both legs, so set
`ROVES_PROBE_JITLESS=1` to run it JIT-less.

The bundle is experimental, not a product package. Node SEA was not used
because a single-file executable would need a bundler, a CommonJS entry point
and `postject` in addition to the sidecar. Instead, the bundle includes the
runtime that the smoke test used. A system Node 20 or newer also works with
`mcp/server.mjs`.

### Install contract

Extract the artifact to any folder on any drive, for example `C:\roves-mcp`;
no administrator rights, junction or fixed path are needed. The probe has no
deno_core startup snapshot yet (see `TODO.md`), so it reads the Deno extension
JS at start-up from the `deno-extension-sources` folder next to the
executable. Keep that folder beside `roves-blitz-v8-probe.exe`: when it is
present, the executable never reads the build machine's Cargo registry, and a
missing file stops the start-up with an error naming the expected path.
The executable also needs the Microsoft Visual C++ runtime that Windows
usually provides. To check the extracted bundle, run
`C:\roves-mcp\node\node.exe C:\roves-mcp\mcp\smoke.mjs C:\roves-mcp\roves-blitz-v8-probe.exe`,
then run the same command with `--discover` added.

Register the MCP server with absolute paths. Launch mode opens the demo
game or any `dist` directory:

```json
{
  "command": "C:/roves-mcp/node/node.exe",
  "args": [
    "C:/roves-mcp/mcp/server.mjs",
    "C:/roves-mcp/roves-blitz-v8-probe.exe",
    "C:/roves-mcp/mcp/fixture"
  ]
}
```

To use discover mode, start the game yourself with
`C:\roves-mcp\roves-blitz-v8-probe.exe --game <dist> --window --mcp-discover`,
then register `["C:/roves-mcp/mcp/server.mjs", "--discover"]` as the
arguments.

## Template playthrough (CI, opt-in)

The same `[dev-mcp]` / `dev_mcp` runs also play the start of
visual-novel-template through this MCP server, in both legs. The template
already passes its live props and narration actions to Pixi'VN's
`Game.testing` (`useTestingBridge`: `setProps` and `setActions`), but only in
`vite dev`. CI builds a disposable copy where `template-testing-optin.mjs`
removes that DEV gate and adds the `Game.testing.enable()` call the Vite plugin
makes in dev. The template repository, the ordinary build and its native-UI
Save/Load/Skip gates are unchanged, and Roves gets no new capability: the
bridge is ordinary page JavaScript.

`playthrough.mjs <probe> <dist> [--out result.json]` launches the game in a
window with isolated storage, waits up to 240 s for `window.pixiVN` with live
`navigate`/`t`/`toast` props, a registered `navigate` action and the `start`
label. It then calls `start("start")`, `continue()`, `goBack()`,
`selectChoice()` and `setInput()`. Since `evaluate_script` rejects promises,
every async action is scheduled in the page and polled for at most 60 s. The
run requires the narration route, at least four distinct dialogue states,
`goBack` returning to the previous line and replaying the next one, the name
prompt answered with `RovesMcp`, that name in later dialogue, no errors in
`pixiVN.errors` and a clean exit after the client disconnects. It prints one
`ROVES_MCP_PLAYTHROUGH` JSON line with timed milestones; CI annotates it and
uploads `blitz-mcp-playthrough-<mode>`. Each MCP request still has the
server's 5 s limit, so a longer stall of the game thread fails the run.

## Chromium reference

Chromium `f2700724f59fb554eb19a4f827b1d256e2a13286` routes protocol messages
through `content/browser/devtools/devtools_session.cc` to a renderer agent.
This probe already embeds the page and the inspector in one process, so it
dispatches the same CDP `Runtime.evaluate` command through
`deno_core::JsRuntimeInspector`. The official MCP TypeScript SDK handles the
stdio protocol. Rust adds the pipe handoff to the V8 thread without adding
Cargo dependencies.
