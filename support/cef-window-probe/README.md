# CEF window probe — stage 1 (Windows baseline)

Isolated experiment, separate Cargo workspace. Nothing in the engine, `roves-action`,
Packmaster or any release depends on it; other targets are untouched. Stage 1 only: one
sandboxed CEF window showing the unmodified game build. **Not implemented here:** Deno/V8
IPC, the Netrender stage, the single-document navigation policy beyond popup refusal,
`game://`, CSP, JITless, Steam, input/audio checks, macOS/Linux.

## What it does

1. CEF's `bootstrap.exe` (shipped as `roves_cef_probe.exe`) starts, initializes the Windows
   sandbox broker and calls `RunWinMain` in `roves_cef_probe.dll`. Subprocesses go through
   the same entry and return from `execute_process`. The browser process refuses to start
   without sandbox information (exit 3); the plain `roves_cef_probe` binary always refuses.
2. The browser process binds `127.0.0.1:<port>` (port 0 = a free port per run) and serves
   `--roves-game-dir` with tower-http `ServeDir` on a dedicated thread.
3. CEF is initialized with `root_cache_path == cache_path ==` a private per-run profile
   directory, so localStorage/IndexedDB/cookies persist on disk for that run in its own
   directory and the CEF process-singleton lock is taken there, never in the shared
   per-user `AppData\Local\CEF\User Data`.
4. One Views top-level window (Chrome runtime style, 1280x720) loads `http://127.0.0.1:<port>/`.
   `window.open`/`target=_blank` popups are refused and logged.
5. Close (OS close button or `--roves-auto-close-ms`): `Window::close` -> `can_close` ->
   `try_close_browser` (page unload handlers) -> `do_close` -> `on_before_close` ->
   `quit_message_loop` -> `shutdown()` -> loopback server graceful stop -> exit code.

Exit codes: 0 main document loaded (status 200 from the loopback origin) and clean close;
1 usage/server error; 2 never loaded, main-frame load error or a child with `--no-sandbox`;
3 no sandbox; 4 `cef::initialize` failed.

## Options (CEF command-line switches)

| Switch | Meaning |
| --- | --- |
| `--roves-game-dir=<dir>` | Required. Game build directory containing `index.html` (e.g. `visual-novel-template/dist`). |
| `--roves-profile-dir=<dir>` | Profile root. Default: `%TEMP%\roves-cef-probe\run-<unix-ms>-<pid>`. Reuse the same directory **and** `--roves-port` to continue with the same storage. |
| `--roves-port=<n>` | Loopback port, default 0 (random). The origin includes the port, so a different port means different storage. |
| `--roves-log=<file>` | Evidence log, one `roves-cef <event> ...` line per event (also on stderr). |
| `--roves-auto-close-ms=<ms>` | After the first successful main-frame load, close the window through the normal close path. |
| `--log-file=<file>` | CEF's own debug log (standard CEF switch). |

## Build and run (Windows x64)

Heavy builds run in CI (`.github/workflows/cef-window-probe.yml`). Locally, with the MSVC
toolchain and CMake on `PATH`:

```pwsh
cd support/cef-window-probe
# Optional shared CEF download (SHA-1 checked against the CEF CDN index by cef-dll-sys):
$env:CEF_PATH = "$env:USERPROFILE/.local/share/cef"
cargo run --locked --release --bin bundle -- ../../target/cef-bundle --release
../../target/cef-bundle/roves_cef_probe.exe --roves-game-dir=<path>/dist --roves-log=probe.log
```

`bundle` is a ~20-line wrapper over cef-rs' own Windows bundler
(`cef::build_util::win::build_bundle`, the code behind upstream `bundle-cef-app`): it
builds the cdylib and copies `bootstrap.exe` (renamed), the DLL, the manifest and the CEF
runtime files. `cargo check --no-default-features --features dox --all-targets` type-checks
the CEF glue without downloading CEF.

## Acceptance checks (CI job `windows`)

The job builds the release bundle, builds `DRincs-Productions/visual-novel-template`
(`npm install && npm run build`, commit printed as a notice, `template_ref` input on manual
dispatch), runs the probe with `--roves-auto-close-ms=10000` and requires:

- process exit code 0 within 120 s to load plus 90 s to exit, and no `roves_cef_probe`
  process alive 3 s after exit;
- log lines: `sandbox-info present=true`, `loopback origin=http://127.0.0.1:`, `window shown`,
  `load-end main-frame status=200 url=http://127.0.0.1:<port>/ ok=true`,
  `child-launch type=renderer no-sandbox=false`, `auto-close window`,
  `browser before-close`, `message-loop exited`, `cef shutdown complete`,
  `loopback stopped`, `exit code=0`; no `no-sandbox=true` anywhere;
- `Local State` present in the per-run profile directory after exit.

Console warnings/errors of the game are re-emitted as notices, not gated. A full-screen PNG
is taken about 5 s after the load; the log, CEF debug log, PNG and build log are uploaded as
`cef-window-probe-evidence`, the bundle as `cef-window-probe-windows-x64`.

What the checks do **not** prove: that the game reached its menu or rendered correctly (the
PNG must be inspected; the runner has no GPU, so WebGL may fall back or fail), input,
audio, save/load across restarts, OS-initiated close while a fetch is pending, behaviour on
real hardware, or the exact sandbox policy of each child (only that no child received
`--no-sandbox` and that the browser received sandbox information from bootstrap.exe;
`chrome://sandbox` was not inspected).

## Chromium reference (pinned `f2700724f59f`, Chromium 160.0.8162.0)

Consulted before writing code, from `support/chromium-reference`:

- `net/base/is_potentially_trustworthy.cc` (`IsLocalhost` branch, ~line 318): loopback
  origins are potentially trustworthy, so `http://127.0.0.1:<port>` is a secure context
  (WebCrypto, Service Workers, storage APIs) without TLS. A custom scheme would need extra
  CEF scheme registration to get the same treatment; the loopback origin keeps Chromium's
  standard network, cache and Range-request paths.
- `url/scheme_host_port.h`: an origin is the (scheme, host, port) tuple, so the port chosen
  per run is also the storage partition key; `--roves-port` exists to reproduce storage.
- `docs/design/sandbox.md`: the browser is the broker, renderers are sandboxed targets
  unless `--no-sandbox` is passed; hence the per-child `--no-sandbox` check and keeping
  bootstrap.exe (which initializes the broker before loading our DLL).

CEF 156.0.31 embeds Chromium 156.0.8078.100, older than the reference revision; the files
above describe long-standing behaviour, not something introduced after 156. CEF's
`libcef_dll/bootstrap/bootstrap_win.cc` at the CEF commit `e603d97` confirms that
`RunWinMain`'s return value is the process exit code and that the client DLL and
bootstrap.exe must be both unsigned or signed with the same certificates.

## Dependency choice

- `cef = "=156.7.0"` (tauri-apps/cef-rs, Apache-2.0 OR MIT, official Rust bindings
  maintained under the Tauri organization; the version suffix `+156.0.31` is the CEF binary
  distribution it downloads from `cef-builds.spotifycdn.com`). Exact pin plus `Cargo.lock`
  (`cef-dll-sys` 156.7.0+156.0.31, `download-cef` 3.0.1). The glue follows the upstream
  `examples/cefsimple` at tag `cef-v156.7.0+156.0.31` (Windows sandbox entry, Views window
  and close sequence). Integrity of the CEF archive is a SHA-1 check against the CDN's own
  index, not a signature: it protects against corruption, not a compromised CDN.
- `axum` 0.8.9 + `tower-http` 0.7.1 `ServeDir` (MIT) for the loopback file server. CEF's
  own `CefServer` would avoid the dependency but needs hand-written file serving (MIME,
  traversal, Range); `ServeDir` already does it. `tower` 0.5 (`util`, already in the graph
  through axum) lets `server.rs` wrap it with the conservative cache policy (`no-cache`,
  per-instance ETag, no `Last-Modified`, `If-Range`), documented with its fixture
  (`fixtures/static-compat`) in `../cef-netrender-probe/README.md`, "Loopback cache policy".
- No Deno, V8 or Netrender crate is linked in this stage.

## Known limits of stage 1

- The loopback port is reachable by any local process for the lifetime of the run.
  `ServeDir` rejects `..` but follows symlinks inside the game directory.
- Chrome-style CEF may contact Google services (component updates, safe-browsing lists);
  no network isolation beyond the game origin is configured yet.
- Client-side routes are served only from `/`; a full reload on a deep route would 404
  (Roves' single-document policy blocks reloads; not implemented here).
- Windows x64 only; no code signing.
