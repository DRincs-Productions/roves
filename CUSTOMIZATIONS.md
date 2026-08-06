# Customizations over upstream Servo

Baseline: Servo `v0.4.0` (<https://github.com/servo/servo/archive/refs/tags/v0.4.0.zip>).

This file lists every deviation from that pristine upstream source, in the order they were
made. See `CLAUDE.md` for why this file exists and the protocol for keeping it current —
short version: **add an entry here in the same turn you change a file under `servo/`.**

Each entry: file path, upstream location the change replaces, what changed, why, and the
matching patch file under `patches/servo-v0.4.0/` that makes the change mechanically
reproducible (see `CLAUDE.md` — `.github/workflows/servo-test-build.yml` applies those
patches to a fresh pristine download on every run once it's actually running somewhere, so
they must stay in sync with reality).

---

## 2026-08-05 — Remove toolbar and tab strip UI entirely

**File:** `ports/servoshell/desktop/gui.rs`, in `Gui::update` (was line ~392-579 in the
`v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0001-remove-toolbar-and-tabs.patch`

**Upstream behavior:** the top toolbar (back/forward/reload/stop, address bar, experimental
prefs toggle) and the tab strip (tab list, new-tab button, new-window button) were drawn
inside `if winit_window.fullscreen().is_none() { ... } else { *toolbar_height = Length::default(); }`
— i.e. shown in windowed mode, hidden only when the OS window itself was in fullscreen.

**Change:** replaced the entire `if/else` block with a single unconditional statement:

```rust
// Kiosk/embedded fork: never draw the toolbar or tab strip, in windowed
// mode or fullscreen — this build is meant to look like a native app
// window, not a browser.
*toolbar_height = Length::default();
```

All of the removed block's code (toolbar buttons, address bar, tab strip, new-tab/new-window
buttons) was deleted, not just made unreachable — there is no dead code left behind.

**Why:** this build is meant to present as a normal native application window (like a Tauri
app), not as a browser with tabs — regardless of whether the window is fullscreen or not.
Upstream only hid this UI in fullscreen; the project's requirement is to hide it always.

**Side effects to know about when upgrading:** the `location` and `location_dirty` fields
destructured from `Self` at the top of `Gui::update` are no longer used anywhere in that
function (their only call sites were inside the removed block). This produces two harmless
`unused variable` warnings — not errors (`servoshell`'s `Cargo.toml` has no
`deny(warnings)`/`forbid(warnings)`). Not fixed further since it's cosmetic; revisit if this
crate ever turns warnings into errors.

---

## 2026-08-06 — Skip GStreamer DLL packaging on Windows when media is disabled

**File:** `python/servo/build_commands.py`, `run_post_build_tasks` (Windows branch) and
`copy_windows_dlls_to_build_directory` (was line ~197-347 in the `v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0002-skip-gstreamer-dll-copy-when-media-disabled.patch`

**Upstream behavior:** after a successful build, `run_post_build_tasks` packages
platform-specific runtime files. On macOS this is correctly gated on `self.enable_media`
(only copies GStreamer dylibs if the media stack is actually enabled). On Windows, the
equivalent call — `copy_windows_dlls_to_build_directory` → `package_gstreamer_dlls` —
runs unconditionally regardless of `self.enable_media`, and hard-fails the whole build if
`servo.platform.get().gstreamer_root(...)` can't find a GStreamer install.

**Change:** `copy_windows_dlls_to_build_directory` now takes an `enable_media: bool`
parameter (passed as `self.enable_media` from the call site) and only calls
`package_gstreamer_dlls` when it's true, mirroring the existing darwin branch. ANGLE and
MSVC DLL copying are unaffected — those aren't GStreamer-related and still run
unconditionally.

**Why:** `../.github/workflows/test.yml` builds with `--media-stack dummy` specifically to
avoid needing GStreamer installed at all on CI (see that file's comments — installing it on
Windows would require an interactive UAC prompt that hangs forever on a GH runner). But this
upstream inconsistency meant the ~25-minute Windows compile still failed at the very last
step, in the post-build DLL-copy phase, with "Could not find GStreamer installation
directory." — independent of `--media-stack dummy` and independent of `--skip-platform`
during bootstrap. Without this fix, Windows CI can never pass while GStreamer bootstrap is
intentionally skipped.

---

## 2026-08-06 — Strip dead browser-navigation state and the favicon pipeline from `Gui`

**File:** `ports/servoshell/desktop/gui.rs`.

**Patch:** `patches/servo-v0.4.0/0003-strip-dead-browser-navigation-state-and-favicon-pipeline.patch`

**Upstream behavior:** even after the 2026-08-05 toolbar/tab removal (above), `Gui` kept
computing full browser-chrome state every frame with no remaining consumer:
`update_location_in_toolbar` (address-bar text), `update_load_status` (spinner/dirty-flag
bookkeeping), `update_can_go_back_and_forward` (back/forward button state), and
`load_pending_favicons`/`embedder_image_to_egui_image` (decoding each `WebView`'s favicon and
uploading it to a GPU texture, cached in `favicon_textures`). All four were only ever read by
the toolbar/tab-strip drawing code that patch 0001 deleted — actual back/forward navigation
(`Alt`+arrow keys etc.) calls `WebView::go_back`/`go_forward` directly in
`headed_window.rs`, bypassing `Gui` entirely, so none of this tracking gates any real
behavior.

**Change:** removed `update_location_in_toolbar`, `update_load_status`,
`update_can_go_back_and_forward`, `load_pending_favicons`, and
`embedder_image_to_egui_image` entirely, along with the `load_status`, `can_go_back`,
`can_go_forward`, and `favicon_textures` fields on `Gui` and the `load_pending_favicons`
call site in `Gui::update`. `update_webview_data` (called once per frame from
`headed_window.rs`) now just calls the one function that still has a real consumer —
`update_status_text`, which feeds the hover-status tooltip — instead of OR-ing together four
functions' change-flags.

The `location: String` and `location_dirty: bool` fields were **left in place** rather than
removed: after this change they're written once in `Gui::new` from the `_initial_url`
constructor parameter and never read again, which the compiler fully accounts for (same
"harmless unused" category the toolbar-removal patch already documented for these two
fields — see 2026-08-05 above) — with no runtime loop touching them anymore, removing them
would require threading a signature change through `create_platform_window` →
`HeadedWindow::new` → `Gui::new` across `app.rs` and `headed_window.rs` for zero behavioral
or binary-size benefit. The constructor parameter was renamed to `_initial_url` to document
that it's now unused for this purpose, without changing the parameter list itself (so
`headed_window.rs`'s call site didn't need touching).

**Deliberately left alone:** `browser_tab` and `toolbar_button` (the tab-strip widget and
toolbar-button helper) are dead code with zero callers anywhere in the crate — unlike the
functions above, they don't run every frame, they simply never run at all, so the compiler
already drops them from the linked binary in release builds. Removing the source would be
pure cosmetics with no effect on the shipped game package, so they were left as-is.

**Why:** this fork's `Gui` no longer draws any browser chrome (see 2026-08-05 above), so this
is the second half of that same cleanup: the *state* that only existed to feed that chrome
was tracked here too, and kept running every frame — recomputing back/forward capability,
diffing load status, and (worst of all) decoding and uploading a GPU texture per `WebView`
favicon — for a UI element that no longer exists. None of it is "dead code" the compiler can
optimize away, since `Gui::update` genuinely calls it every frame; it's live, wasted CPU/GPU
work in every build of the game, embedded or otherwise.

**Side effects to know about when upgrading:** if a future upstream Servo version changes how
`WebView::status_text`/`load_status`/`can_go_back`/`can_go_forward`/`favicon` are exposed,
re-check this patch still applies cleanly — it's a bigger diff than 0001 and touches more of
`Gui`'s internals. Not verified against an actual `./mach build` (Servo's build is
multi-hour and wasn't run for this change) — treat the next real build of this fork as the
actual verification and fix up any compile errors this patch introduces before relying on it.

---

## 2026-08-06 — New `mach bundle` command: package a build into something runnable

**File:** `python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0004-add-mach-bundle-command.patch`

**Upstream behavior:** `./mach build` leaves the raw Cargo output in `target/<profile>/` —
just the engine binary plus (on Windows) ANGLE/MSVC DLLs dropped next to it by
`copy_windows_dlls_to_build_directory`. Running that binary bare opens Servo's own default
start page, not this fork's intended content, and on Windows it has no window-size/URL args
set, so it isn't something a non-technical person can be handed and told to double-click.

**Change:** added a new command, `./mach bundle [--html-file dist/index.html]
[--window-size 1280x720] [--output DIR] [--content-dir DIR] [--deb] [-- extra servoshell
args]`, category `post-build`. It does **not** touch `target/<profile>/` or move the binary
Cargo put there — `./mach run` and everything else that calls `get_binary_path()` keeps
working exactly as before. Instead, into a separate output directory (default:
`target/<profile>/bundle/`) it produces, per platform:

- **Windows:** the engine binary + its DLLs moved into a `bin/` subdirectory, plus a real
  `play.exe` at the top level — a tiny std-only Rust program, compiled on the fly with plain
  `rustc` (no Cargo project), built with `#![windows_subsystem = "windows"]` (the same
  attribute `ports/servoshell/main.rs` already uses on `servoshell.exe` itself — see the
  2026-08-05 entries) so double-clicking it never flashes a console. It just spawns
  `bin/servoshell.exe` with the configured args and exits.
- **macOS:** a minimal `Servo.app` bundle (`Contents/Info.plist` + `Contents/MacOS/Servo`, a
  small shell script that `exec`s the engine binary — renamed `<binary>-core` and tucked
  inside the bundle — with the configured args). Finder launches
  `Contents/MacOS/Servo` directly; no `Terminal.app` involved at all, unlike double-clicking
  a loose `.sh`.
- **Linux (default):** the engine binary renamed `<binary>-core` **without its executable
  bit**, plus a `play.sh` that `chmod +x`'s it, sets `LD_LIBRARY_PATH`, and execs it with the
  configured args. `play.sh` is the only supported entry point; a curious
  `./<binary>-core` fails with "Permission denied" instead of launching without the
  args/`LD_LIBRARY_PATH` it actually needs.
- **Linux with `--deb`:** a real, installable `.deb` instead (`<name>_<version>_<arch>.deb`,
  built via `dpkg-deb --build --root-owner-group`) — engine + content under
  `/usr/lib/<name>/`, a launcher at `/usr/bin/<name>`, and a `.desktop` entry so it shows up
  in application launchers. Requires `dpkg-deb` (from `dpkg-dev`) on `PATH`; raises
  `BuildNotFound` with a clear message if missing rather than a bare traceback. This is a
  functional package, not a lintian-clean one — no changelog, man page, or maintainer
  scripts.

`--content-dir` (e.g. a built `dist/`) is copied into the bundle at whatever relative
location `--html-file` expects it (default `dist/index.html` → a `dist/` next to the
launcher, or under `/usr/lib/<name>/` for `--deb`) — see `_place_bundle_content`. Passing it
is optional: a caller can instead place content into the output directory itself after the
command returns, which is how `../.github/workflows/test.yml`'s `assemble test bundle` step
originally worked before being simplified to just call this command directly.

**Why:** originally this exact logic (per-platform launcher, hidden/renamed core binary, no
console/Terminal) was hand-rolled as bash inside `../.github/workflows/test.yml`. That's
backwards: this fork's whole reason for existing is `git`-vendoring Servo instead of
depending on it as a black box (see `../CLAUDE.md`), specifically so *product* behavior like
"how does a build actually get handed to someone to run" lives in the product (`mach`), not
duplicated across whichever CI happens to build it. `../.github/workflows/embedded.yml` (the
*real* release pipeline) currently has its own near-identical hand-rolled
play.bat/play.sh generation, with the exact same "opens a terminal, no nice extension"
UX gap this command fixes — it doesn't consume this command yet because it doesn't build
from this fork at all today (it downloads upstream's official prebuilt binaries; see that
file's own comments), but the plan is for it to eventually point at this fork instead, at
which point it should switch to `./mach bundle` too instead of re-diverging.

**Side effects to know about when upgrading:** none of this depends on Servo internals
beyond `get_binary_path()`/`self.target` (stable, low-level `CommandBase` API), so it should
survive a version bump untouched. If a future Servo version changes what
`copy_windows_dlls_to_build_directory` drops next to the Windows binary, double check
`_bundle_windows`'s DLL glob still catches everything needed.

**Correction (same day):** `_bundle_macos` originally copied `.dylib` files flat into
`Contents/MacOS/`. `ports/servoshell/build.rs` links `servoshell` with
`-Wl,-rpath,@executable_path/lib/` unconditionally on macOS (see that file), so dyld only
ever looks in a `lib/` subdirectory next to the binary — flat placement meant any dylib
dependency (GStreamer today if `--media-stack gstreamer`, Steamworks below) would silently
fail to load at runtime despite bundling successfully. Fixed to copy into
`Contents/MacOS/lib/`, matching the same convention `run_post_build_tasks`'s own darwin
branch already uses for `package_gstreamer_dylibs` (`path.join(path.dirname(built_binary),
"lib/")`).

---

## 2026-08-06 — `steam:` protocol bridge, so web content can reach Steamworks

**Files:** `ports/servoshell/Cargo.toml`, `ports/servoshell/build.rs`,
`ports/servoshell/desktop/app.rs`, `ports/servoshell/desktop/protocols/mod.rs`, new file
`ports/servoshell/desktop/protocols/steam.rs`.

**Patch:** `patches/servo-v0.4.0/0005-add-steam-bridge.patch`

**Upstream behavior:** Servo has no notion of Steamworks; web content has no way to reach
any native SDK beyond what `ProtocolHandler`s already expose (the `servo:`/`resource:`/
`urlinfo:` schemes registered in `app.rs`, see `protocols/servo.rs`).

**Change:** added a `steam` Cargo feature (`dep:steamworks`, desktop-only — same
`not(any(target_os = "android", target_env = "ohos"))` dependency block `headers`/
`serde_json` already live in) that, when enabled, registers a new `steam:` custom protocol
handler alongside the existing ones in `app.rs`. `SteamProtocolHandler::new()` calls
`steamworks::Client::init()` once at registration time (spawning the same 100ms
`run_callbacks()` pump thread the Tauri build's `steam::try_init()` already does) and
degrades to "Steam unavailable" answers rather than failing when Steam isn't running —
mirrors the parent project's `src-tauri/src/steam.rs` **command-for-command** (achievements,
int/float stats, DLC check, overlay, store page), so `src/lib/steam.ts` in the parent
project can expose the *exact same* JS API regardless of which native shell (Tauri or
Roves) is running the game: `fetch('steam:unlock_achievement?achievement_id=ACH_X')` instead
of `invoke('steam_unlock_achievement', { achievementId: 'ACH_X' })`, both landing on the same
underlying Steamworks call. See `src/lib/steam.ts`'s own `callRoves()`/`isSteamSupported()`
for the JS-side half of this bridge (outside this `servo/` directory — that's the parent
project's own file, not part of this fork's patches). Notably there's no build-time "is
steam enabled" flag on the JS side: an earlier version baked one in via a `STEAM_ENABLED`
env var, but that could drift from whether *this particular binary* actually has the feature
compiled in — `isSteamSupported()` asks the real running binary instead (per-shell: `invoke`
on Tauri, `callRoves` on Roves), and caches the answer.

`build.rs` additionally copies the Steamworks redistributable (`steam_api64.dll` /
`libsteam_api.dylib` / `libsteam_api.so`) from steamworks-sys's own `OUT_DIR` into
`target/<profile>/`, right next to the built binary — mirrors `src-tauri/build.rs`'s
equivalent copy (which lands the same file in `src-tauri/` for Tauri's bundler instead).
Landing it in `target/<profile>/` means `./mach bundle` (see the 2026-08-06 entry above)
picks it up for free through the DLL/dylib/so glob it already has, no extra plumbing needed
there — only guarded by `CARGO_FEATURE_STEAM`, since build scripts don't get
`#[cfg(feature = ...)]` applied to their own compilation.

**Why:** shipping on Steam (achievements, cloud stats, overlay) is a stated goal for this
fork regardless of which native shell a given release uses — the Tauri build already had
this wired up; Roves had no equivalent path to any native API at all, since it's meant to
run untouched web content with zero JS-side awareness of which shell it's in beyond the
existing `__EMBEDDED_TARGET__` build flag.

**Side effects to know about when upgrading:** `ProtocolHandler` (`components/net/protocols/
mod.rs`) is a stable, low-level trait uninvolved in most of Servo's churn — this should
survive a version bump untouched. If a future Servo version changes `ServoUrl`'s API
(`as_url()`/`query_pairs()`), or `steamworks-rs` cuts a new major version with a different
`Client`/`UserStats` surface, re-check `steam.rs`'s `handle_command` still matches. Not
verified against an actual `./mach build --features steam` (multi-hour build, and this
sandbox has no Steam client to test against) — treat the next real `--features steam` build
as the actual verification, particularly whether `steamworks-sys` actually places
`libsteam_api.*` where `copy_steam_lib` expects it.

**Manual verification aid:** `../.github/workflows/test.yml`'s synthetic test page (in its
"assemble test bundle" step) has two buttons for exactly this: one hits `steam:is_available`
directly via `fetch()`, the other reproduces an `invoke(cmd, args)` →
`window.__TAURI_INTERNALS__.invoke` → `fetch('steam:...')` chain inline (no bundler, so no
real npm package import there) — click through both after a `--features steam` build lands
in the "test" release, instead of re-deriving this by hand. (`src/lib/steam.ts` itself no
longer goes through that `window.__TAURI_INTERNALS__` indirection — see the "Roves' own
general-purpose `invoke()` bridge" entry below for why — but the test page's chain still
exercises the same underlying `steam:` protocol either way.)

---

## 2026-08-06 — Roves' own general-purpose `invoke()` bridge (`roves:` protocol) + `@drincs/roves-api`

**Files:** `ports/servoshell/desktop/event_loop.rs`, `ports/servoshell/desktop/app.rs`,
`ports/servoshell/desktop/protocols/mod.rs`, new file
`ports/servoshell/desktop/protocols/roves.rs`. Plus, outside this `servo/` directory (not
patch-tracked — see the README.md/examples/ precedent above): the new `roves-api/` package
at the repo root, and `src/lib/hooks/quit-hooks.ts`/`src/lib/steam.ts` in the parent project.

**Patch:** `patches/servo-v0.4.0/0006-add-roves-invoke-bridge.patch`

**Upstream behavior:** no equivalent — this is new functionality, not a modification of
existing upstream logic.

**Change:** added a `roves:` custom protocol handler (`RovesProtocolHandler`, always
registered — unlike `steam:`, not feature-gated), the Roves equivalent of Tauri's IPC:
web content calls `fetch('roves:<command>?<args>')`, and gets a JSON result back. Today it
answers exactly one command, `exit`/`close_window`, which closes every open window
(`ServoShellWindow::schedule_close`) — in this fork's usual single-window setup, that's
equivalent to quitting the app, since `App::pump_servo_event_loop` returning `false` once no
windows remain makes the event loop exit on its own (`app.rs`).

The interesting part is *how* it reaches the window at all: `ProtocolHandler` implementors
must be `Send + Sync` and are invoked off the main thread (fetches run on network/IO
threads), but `RunningAppState`/`ServoShellWindow` are `Rc`-based — deliberately
single-threaded, main-thread-only types. A protocol handler can't touch them directly. The
fix reuses a mechanism this codebase already had for exactly this shape of problem:
`HeadedEventLoopWaker` (`event_loop.rs`) already wakes the main thread from other threads via
`Arc<Mutex<EventLoopProxy<AppEvent>>>` + `winit`'s own cross-thread-safe `EventLoopProxy`.
`RovesProtocolHandler` holds the same kind of handle (built from `App`'s own
`event_loop_proxy` field at registration time, `None` in headless mode) and sends a new
`AppEvent::CloseAllWindows` variant through it; `App::user_event` (already the place
`AppEvent`s get handled on the main thread) matches on it and calls `schedule_close()` on
every window in `RunningAppState::windows()`.

Outside `servo/`: a new npm package, **`@drincs/roves-api`** (repo root `roves-api/`,
registered as an npm workspace — see the root `package.json`), wraps this in JS with a
`core.invoke(cmd, args)` shaped exactly like `@tauri-apps/api/core`'s `invoke()`, plus a
`process.exit()` (the Roves equivalent of `@tauri-apps/plugin-process`'s `exit()`) and a full
`steam` module (talking to `steam:` directly, not through `roves:` — see the entry above for
why Steam gets its own dedicated protocol). It's a real, independent implementation, not a
shim over Tauri's runtime — deliberately shaped to feel familiar, nothing more. The parent
project's `src/lib/hooks/quit-hooks.ts` now calls `@drincs/roves-api/process`'s `exit()` on
Roves instead of `window.close()` (unreliable there: scripted `window.close()` is only
granted on windows the page itself opened via `window.open()`, not a shell-created top-level
window), and `src/lib/steam.ts` picks between a Tauri-`invoke()`-based `SteamApi`
implementation and `@drincs/roves-api/steam`'s own, once, at module load — both conform to
the exact same `SteamApi` interface exported from the package.

**Why:** closing/quitting the app is exactly the kind of basic native capability every
embedded shell needs, and `window.close()` doesn't reliably provide it (see above) — Roves
had no way to do this at all before. Building it as a small, generic `invoke()`-shaped
bridge (rather than a one-off `close:` protocol) gives Roves room to grow more "control this
app" commands the same way later, instead of accumulating one bespoke protocol per feature.

**Side effects to know about when upgrading:** the cross-thread handle pattern
(`Arc<Mutex<EventLoopProxy<AppEvent>>>`) depends on `winit`'s `EventLoopProxy` staying
`Send`/`Sync`-safe and on `RunningAppState::windows()`/`ServoShellWindow::schedule_close`
keeping their current signatures — all `pub(crate)`, low-level, and not upstream-churn-prone,
but re-check if a version bump changes `app.rs`'s event-loop structure materially. Not
verified against an actual `./mach build` (multi-hour; not run for this change) — treat the
next real build as the actual verification, in particular whether `AppEvent::CloseAllWindows`
compiles cleanly through the `matches!`/`if let` restructuring in `App::user_event`.
