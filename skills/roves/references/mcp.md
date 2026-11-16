# Connect to Roves

The tools described here currently ship with the Blitz development probe.
Build the probe with Cargo feature `dev-mcp` and run `npm ci` in
`support/blitz-v8-probe/dev-mcp` using Node 20 or newer. Configure a local
stdio server with command `node` and three absolute arguments in order:
`server.mjs`, the feature-enabled probe executable, and the game's dist
directory. The first tool call starts a native window and creates isolated
storage.

Call `evaluate_script` with
`{"source":"({url: location.href, text: document.body.textContent.trim().slice(0, 300)})"}`.
Return a synchronous JSON value. For asynchronous work, schedule the work
in one call and inspect its result in a later call. Find a control from the
current DOM before calling it and verify the new page state afterward.

Call `pixi_status` with `{}`. A registered scene returns a bounded tree and
renderer settings. A result of `not_detected` means no known PixiJS
references were visible to the probe. A bundle can register an existing
Pixi application through `globalThis.__PIXI_APP__` in its diagnostic build.
