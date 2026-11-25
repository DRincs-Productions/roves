# Steamworks feasibility gate (experiment only)

A standalone Cargo workspace that answers one question for the CEF + SDL3 + wgpu player:
does `steamworks` 0.13.1 (the crate the legacy Servo shell shipped) still initialize,
dispatch a callback and shut down cleanly on Windows, macOS and Linux, and can CI tell the
possible outcomes apart honestly? It is not part of the Windows player
(`support/cef-netrender-probe`), of the CEF macOS/Linux gates, of their workflows, or of
any shipped target, and it does not integrate Steam into the player.

Out of scope, by design: store/depot upload, `steamcmd`, Steam credentials or partner
accounts, DRM wrappers, ownership checks for a real title, any stat or achievement change.

## Legacy implementation inspected (git `27ca0811d6d`)

- `ports/servoshell/desktop/protocols/steam.rs`: a `steam:` fetchable protocol. One
  `Client::init()` at registration; on success a detached thread calls `run_callbacks()`
  every 100 ms forever, on failure `client` stays `None` and every command answers a
  harmless default (`false`/`0`/`null`). Commands: `is_available`, `get_player_name`,
  `get_app_id`, achievements (unlock/is_unlocked/clear, each unlock/clear followed by
  `store_stats`), int/float stats, `store_stats`, `is_dlc_installed`, `open_overlay`,
  `open_store`. No explicit shutdown: `SteamAPI_Shutdown` only ran if the last `Client`
  clone was dropped, which the ticker thread prevented.
- `ports/servoshell/Cargo.toml`: optional `steam = ["dep:steamworks"]`, `steamworks = "0.13"`
  resolved to 0.13.1 / `steamworks-sys` 0.13.0 (same checksums as this lockfile).
- `ports/servoshell/build.rs` `copy_steam_lib`: copied `steam_api64.dll` /
  `libsteam_api.dylib` / `libsteam_api.so` from `steamworks-sys`'s `OUT_DIR` next to the
  binary, because the crate links the redistributable dynamically.
- `roves-api/src/steam.ts`: the JS contract over `fetch('steam:<command>')`; every call
  degrades to `null`/`false`/`0` without Steam, never throws.
- `.github/workflows/test.yml` `steam-emulator-smoke-test` (Linux only): swapped
  Goldberg/GSE (`Detanup01/gbe_fork`) in for `libsteam_api.so`, generated
  `steam_interfaces.txt` from the real redistributable, App ID 480, and checked the JS round
  trip. It downloaded the **latest** release unverified, so it was not reproducible, and it
  never exercised callbacks (all commands it checked are synchronous) or shutdown.

## Library choice

`steamworks` 0.13.1 (MIT/Apache-2.0, Noxime/steamworks-rs) is still the newest release on
crates.io (2026-05-05) and wraps the flat Steamworks API with manual callback dispatch, so it
is kept as is: no fork, no hand-written FFI. Its `Client` is `Send + Sync + Clone`; the last
drop calls `SteamAPI_Shutdown`. The Steamworks SDK redistributables come from
`steamworks-sys` 0.13.0 (Valve's SDK terms apply; this workflow uploads logs only, never the
binaries or the libraries).

## What the gate does

`roves-steamworks-gate [--expect any|unavailable|emulation|real] [--app-id N] [--timeout-secs N]`

1. `Client::init_app(480)` (Valve's public "Spacewar" test app; sets `SteamAppId` for this
   process only, writes no `steam_appid.txt`, never calls `RestartAppIfNecessary`).
2. On success: logs app id, whether the Steam ID is valid and the user logged on (never the
   persona name or the Steam ID itself), registers `UserStatsStored` and `UserStatsReceived`
   handlers, requests the user's stats (informative), and calls `StoreStats` with no pending
   change, retrying every 250 ms until accepted. `UserStatsStored` with an `Ok` result is the
   gated callback; callbacks are pumped on the main thread with `process_callbacks` every
   16 ms, as a player frame loop would.
3. Drops the callback handles, then the only `Client` (`SteamAPI_Shutdown`), and logs
   `gate: shutdown complete`.

Exit 0 when the observed outcome matches `--expect`, 1 otherwise (`gate: FAIL ...`).

| State | How it is produced | What it proves | What it does not prove |
|---|---|---|---|
| compile-only | `cargo build --locked --release` on `windows-2025`, `macos-15`, `ubuntu-24.04` | bindings and redistributable link on each OS | anything at run time |
| client unavailable | same runners, redistributable next to the binary, `--expect unavailable` | library loads, `SteamAPI_InitFlat` fails with a typed error, clean exit; `--expect real` in the same environment must fail (negative run) | callbacks, shutdown after a successful init |
| emulation | Linux only: pinned GSE `release-2026_09_27`, SHA-256 checked, offline, `--expect emulation` | init -> `UserStatsStored` callback -> `SteamAPI_Shutdown` through steamworks-rs | behaviour of the real Steam client, overlay, ownership, Steam Cloud, macOS/Windows |
| real Steam | manual opt-in on a developer machine (below) | the same sequence against the real client | nothing in CI; never run automatically |

Locally verified on Windows 11 (2026-11-25, no Steam client running): build, `--expect
unavailable` passes with `FailedGeneric("No SteamClient023")`, `--expect real` fails with
exit 1, unknown arguments fail. Nothing else has run yet; the workflow has no CI run.

### Why the emulator is acceptable here, and only on Linux

GSE is LGPL-3.0 and maintained; it is used offline, with Valve's own public test App ID, to
exercise Roves' open-source code path. It does not unlock or impersonate any commercial
title, uses no credentials, and the result is labelled `emulation`, never `real`. The
release is pinned by tag and GitHub's published SHA-256; update both together. Windows and
macOS emulation are not added: the legacy job was validated only on Linux, and the macOS
emulator build would need its own signing/notarization questions answered.

The emulator's own behaviour is an assumption until the first CI run: that `StoreStats`
posts `UserStatsStored` and that the release archive keeps `regular/x64/libsteam_api.so`
and a `generate_interfaces` x64 tool (the job prints the archive tree if not).

## Real Steam opt-in smoke (manual)

Run only on your own machine, with your own Steam client running and logged in. App 480 is
a free Valve test app; Steam will briefly show you as playing "Spacewar".

```sh
cd support/steamworks-gate
cargo build --locked --release
# Put the redistributable next to the binary (path printed by `cargo build -vv` or found
# under target/release/build/steamworks-sys-*/out/):
#   Windows: steam_api64.dll   macOS: libsteam_api.dylib   Linux: libsteam_api.so
target/release/roves-steamworks-gate --expect real
```

Linux also needs `LD_LIBRARY_PATH=target/release`. Expected: `gate: identity app_id=480
steam_id_valid=true logged_on=true`, `gate: callback UserStatsStored result=Ok(())`,
`gate: shutdown complete`, `gate: PASS outcome=initialized expect=real`, and Steam no longer
showing the app as running. Record OS, Steam client build and the `gate:` lines (they contain
no personal data) in the TODO entry. Inside the Steam Linux Runtime (pressure-vessel) this
is a separate, not yet attempted check.

## Next steps

First CI run; then the manual real-Steam smoke on Windows (and macOS/Linux when available);
then decide how the CEF player hosts the client (one `Client`, callbacks pumped from the SDL3
frame loop, explicit shutdown before CEF shutdown) and which `roves-api/steam` commands the
CEF bridge exposes. Not started.
