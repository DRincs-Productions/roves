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

> **Superseded (2026-08-08):** the per-platform launcher/hidden-core-binary shape this
> entry describes below (`bin/servoshell.exe` + `play.exe`, `<binary>-core` + `play.sh`/a
> bash script) is no longer current — see the "Single-executable bundle" entry near the
> end of this file. Kept as-written for its still-accurate motivation/history; don't use
> its per-platform bullet list as a description of what `mach bundle` produces today.

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
`Client`/`UserStats` surface, re-check `steam.rs`'s `handle_command` still matches.

**Follow-up (2026-08-07) — CI now actually builds `--features steam`:** the "not verified"
gap above is closed: `../.github/workflows/test.yml`'s `mach build` step now passes
`--features steam` on all 3 platforms, so every push exercises `steamworks-sys` actually
compiling/linking and `build.rs`'s `copy_steam_lib` finding and copying the Steamworks
redistributable (`steam_api64.dll`/`libsteam_api.*`) next to the binary — this workflow is
still dormant today (see its own header comment on why), so this only takes effect once
`servo/` is pushed as its own top-level repo. No Steam client is available on any CI runner
either way, so this still only proves the build/link step, not a real `Client::init()`
success — `handle_unavailable`'s degrade path is what runs there regardless.

**Follow-up (2026-08-06) — mixed-content blocking on `fetch('steam:...')`:** manual testing
surfaced `TypeError: Network error: Blocked as mixed content` on a `fetch("steam:is_available")`
call. Servo's mixed-content check (`components/net/fetch/methods.rs`'s
`should_request_be_blocked_as_mixed_content`, via
`components/net/protocols/mod.rs::is_url_potentially_trustworthy`) treats a custom scheme as
trustworthy only if its `ProtocolHandler::is_secure()` returns `true` — `SteamProtocolHandler`
only overrode `is_fetchable()` (needed for direct, non-`no-cors` `fetch()` access) and left
`is_secure()` at its default `false`, the same gap `protocols/urlinfo.rs`'s
`UrlInfoProtocolHander` already avoids by overriding both. Fixed by adding
`fn is_secure(&self) -> bool { true }` next to `is_fetchable` in `SteamProtocolHandler`,
folded into `patches/servo-v0.4.0/0005-add-steam-bridge.patch` (regenerated the whole
new-file hunk for `steam.rs` rather than hand-editing a diff-of-a-diff). The same gap existed
in `RovesProtocolHandler` (`roves:`, see the entry below) and was fixed there too, even
though it hadn't been exercised yet by manual testing — same trait, same missing override.

---

## 2026-08-06 — Roves' own general-purpose `invoke()` bridge (`roves:` protocol) + `@drincs/roves-api`

**Files:** `ports/servoshell/desktop/event_loop.rs`, `ports/servoshell/desktop/app.rs`,
`ports/servoshell/desktop/protocols/mod.rs`, new file
`ports/servoshell/desktop/protocols/roves.rs`, `ports/servoshell/desktop/tracing.rs` (added
2026-08-06, see CI-failure note below). Plus, outside this `servo/` directory (not
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
but re-check if a version bump changes `app.rs`'s event-loop structure materially.

**Follow-up (2026-08-06) — mixed-content blocking:** `RovesProtocolHandler` had the same
`is_secure()` gap as `SteamProtocolHandler` — see that entry's follow-up above for the root
cause. Fixed the same way (added `fn is_secure(&self) -> bool { true }`), folded into
`patches/servo-v0.4.0/0006-add-roves-invoke-bridge.patch`.

**Follow-up (2026-08-06) — CI-confirmed build break:** the "not verified against an actual
build" risk noted above materialized: CI failed with `error[E0004]: non-exhaustive patterns:
&winit::event::Event::UserEvent(AppEvent::CloseAllWindows) not covered` in
`ports/servoshell/desktop/tracing.rs`'s `LogTarget for winit::event::Event<AppEvent>` impl
(was line ~42-61 in the `v0.4.0` baseline) — an exhaustive `match self` over every
`AppEvent` variant that wasn't updated when `CloseAllWindows` was added above. Fixed by
adding `Self::UserEvent(AppEvent::CloseAllWindows) => target!("UserEvent(CloseAllWindows)"),`
alongside the existing `Waker`/`Accessibility` arms, folded into
`patches/servo-v0.4.0/0006-add-roves-invoke-bridge.patch` as an additional hunk rather than a
separate patch file, since it's part of the same logical change (this file was simply missed
the first time). No behavior change beyond making the match exhaustive again — `tracing.rs`
only affects `RUST_LOG` filtering granularity, never actual event handling.

---

## 2026-08-06 — Stable `file://` origin, so content opened from disk actually loads

**File:** `components/url/origin.rs`, `ImmutableOrigin::new_opaque_for_file` (was line
86-92 in the `v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0007-stable-file-origin-for-module-script-loading.patch`

**Upstream behavior:** `new_opaque_for_file()` mints a brand-new random `Uuid::new_v4()` on
every call, with no caching (`ServoUrl::origin()`, `components/url/lib.rs:90-92`, calls
`ImmutableOrigin::new()` — and therefore this — fresh every time). Two `.origin()` calls on
the *exact same* `file://` URL are therefore never equal to each other, let alone two
different `file://` URLs. Manual testing (see `TODO.md`'s "schermata bianca" writeup)
reproduced the consequence directly: a normal Vite build (external
`<script type="module" src="...">`, the default output shape for anything past a
single-file toy) opened from a `file://` URL renders a blank page. Root cause: the HTML
module-script spec always fetches external module scripts in CORS mode regardless of the
`crossorigin` attribute, and CORS mode's same-origin check
(`components/net/fetch/methods.rs:547-548`,
`*origin == request.current_url_with_blob_claim().origin()`) can never succeed for
`file://` given the above — the fetch falls through to `methods.rs:597`'s
`NetworkError::UnsupportedScheme`, the entry script never runs, and the page stays exactly as
the HTML parser left it. (`fetch()` to this fork's own `steam:`/`roves:` protocols is
unaffected — those are marked `is_fetchable()`, which explicitly bypasses this same-origin
check — but a plain `fetch()` of a `file://` URL, or any other CORS-mode `file://` request,
would hit the same wall.)

**Change:** `new_opaque_for_file()` now returns a fixed id (`Uuid::nil()`, via a new
`FILE_ORIGIN_ID` constant) instead of a fresh random one, so every `file://` origin this
build ever produces compares equal to every other one.

**Why:** this build only ever opens one `file://` document — its own bundled
`dist/index.html` — and exposes no way to navigate to any other `file://` URL (no address
bar, no tabs, see the 2026-08-05 toolbar-removal entry). "Are two `file://` origins the same
origin" has no real answer in the spec itself (opaque origins are supposed to be globally
unique, but see <https://github.com/whatwg/html/issues/3099> for the standing ambiguity
specifically about `file://`), and mainstream browsers already lean toward usability over
strict uniqueness here in practice. For this fork's one-document-only use case there is no
realistic downside to always treating `file://` as the same origin, and it's what makes an
ordinary Vite (or webpack, etc.) build actually load when opened from disk, matching how it
would behave served over `http(s)://`.

**Side effects to know about when upgrading:** this changes origin *equality* for `file://`
only — `is_potentially_trustworthy()` already special-cased `file://` as trustworthy
regardless of id (see the mixed-content follow-up above), so that behavior is unchanged.
Storage (`localStorage`, etc.) and any other same-origin-keyed state would now be shared
across *all* `file://` documents in the same process, which would be a real regression for
stock Servo (general-purpose browsing, multiple unrelated `file://` pages) but is inert here
given the single-document constraint above — revisit this reasoning if this fork ever grows
a way to open more than one `file://` document. Verified with `cargo check -p servo-url`
(compiles clean); not yet verified against an actual `./mach build` + `./mach bundle` +
manual click-through (see `TODO.md`) — treat that as the real verification of whether this
actually fixes the blank-page repro end to end.

---

## 2026-08-07 — Storage access for `file://` origins

**Files:** `components/url/origin.rs`, `components/script/dom/window/window.rs`,
`components/script/dom/globalscope/globalscope.rs`, `components/storage/client_storage.rs`.

**Patch:** `patches/servo-v0.4.0/0008-allow-storage-for-file-origin.patch`

**Upstream behavior:** manual testing of `../test-page/` on a real build (see `TODO.md`'s
now-removed storage findings) surfaced `localStorage`/`sessionStorage` throwing
`SecurityError: Cannot access ... from opaque origin.`, and `indexedDB.open()`/
`navigator.storage.{persist,persisted,estimate}()` rejecting with the equivalent for the
same reason: `Window::GetLocalStorage`/`GetSessionStorage`, `GlobalScope::
obtain_storage_key`, and `client_storage.rs`'s `obtain_a_local_storage_shelf` each reject any
origin that isn't `ImmutableOrigin::Tuple`, and every `file://` document in this fork is
`ImmutableOrigin::Opaque` (see the 2026-08-06 entry above). This is a faithful
implementation of the Storage Standard/HTML "obtain a local/session storage bottle map"
algorithms, not a Servo-specific bug — Firefox rejects `file://` storage the same way;
Chrome's own internal origin model just doesn't follow the spec here, which is why it looks
fine there.

**Change:** new `ImmutableOrigin::can_access_storage()` (mirrored on `MutableOrigin`) in
`origin.rs`: `self.is_tuple() || self.is_file_origin()`. The three call sites above
(`window.rs`'s two storage-bottle-map checks, `globalscope.rs`'s `obtain_storage_key()`, and
`client_storage.rs`'s `obtain_a_local_storage_shelf`) now call this instead of `is_tuple()`/a
blanket opaque-origin check, so `file://` documents specifically are exempted from the
Storage Standard's opaque-origin restriction — `localStorage`, `sessionStorage`,
`indexedDB`, and `navigator.storage` all go through one of these four checks. Every other
opaque origin (`data:`, `blob:`-without-origin, etc.) still gets rejected exactly as before.

**Why:** Roves ships games, and a web game with no way to persist save data is a hard
blocker, not a nice-to-have — the whole point of this fork is to make an ordinary web game's
existing code just work. The `can_access_storage()` exemption deliberately mirrors
`is_potentially_trustworthy()`'s existing `is_file_origin` carve-out (same file, added in the
2026-08-06 entry above) rather than making `file://` a tuple origin outright: that broader
change would also alter Cookie Store, CORS/same-origin, and mixed content behavior, none of
which were broken and none of which this change touches.

**Side effects to know about when upgrading:** `can_access_storage()`'s safety rests on the
exact same single-document assumption as `new_opaque_for_file()`'s origin-stability change
(all `file://` documents share one fixed origin, and this fork only ever has one open at a
time) — re-verify that assumption still holds before reusing this pattern if this fork ever
opens more than one `file://` document, or a second one concurrently (a devtools popup, a
future multi-window feature, etc.). Verified with `cargo check -p servo-url` and
`cargo check -p servo-storage` (both clean). `cargo check -p servo-script` (covers
`window.rs`/`globalscope.rs`) could **not** be run in the sandbox this was authored in — its
`mozjs_sys` build script needs `llvm-objdump`, which isn't installed there; this is a
toolchain gap in that environment, not a code issue, but it does mean the `window.rs`/
`globalscope.rs` hunks are unverified by any compiler here. Both edits are a single
`if`-condition swap using a method that already compiles correctly against the same type in
`servo-url`, so risk is low, but treat an actual `cargo check -p servo-script`/`./mach build`
as the real verification before considering this entry closed. Not yet re-verified
end-to-end against a real build either way (same caveat as the 2026-08-06 entry above).

**Future consoles:** none of PS4/PS5/Xbox/Switch/etc. (see `README.md`'s platform table) are
implemented yet, but when they are, `localStorage`/`indexedDB` are very unlikely to be the
right save-data backend there at all — consoles have their own native save-data APIs
(platform-specific save containers, cloud sync, storage quotas tied to the OS, etc.), and a
game shipped on Roves should use *those*, not an emulated Web Storage shim on top of
whatever generic filesystem access that console port ends up with. This entry's fix is
scoped to desktop `file://` specifically; it isn't meant to imply `localStorage`/`indexedDB`
should keep being the save-data path once console ports exist — that's a separate,
per-platform bridge (conceptually the same shape as the `steam:` protocol bridge above:
a native command surface web content calls into, backed by whatever the platform actually
offers) that hasn't been designed yet.

---

## 2026-08-07 — Default-on experimental web platform features

**Files:** `components/config/prefs.rs`.

**Patch:** `patches/servo-v0.4.0/0009-default-on-experimental-web-platform-prefs.patch`

**Upstream behavior:** upstream Servo curates its own bundle of off-by-default features in
`EXPERIMENTAL_PREFS` (`ports/servoshell/prefs.rs`), all flipped on together only when
launched with `--enable-experimental-web-platform-features`. Two of them
(`dom_async_clipboard_enabled`, `dom_indexeddb_enabled`) were exactly the clipboard/IndexedDB
gaps found while testing the storage fix above — `navigator.clipboard` was `undefined`, and
`indexedDB.open()` had a second, independent reason to fail beyond the opaque-origin one
(see the entry above). The other 16 entries in that same bundle
(`dom_exec_command_enabled`, `dom_fontface_enabled`, `dom_intersection_observer_enabled`,
`dom_navigator_protocol_handlers_enabled`, `dom_notification_enabled`,
`dom_offscreen_canvas_enabled`, `dom_permissions_enabled`, `dom_sanitizer_enabled`,
`dom_storage_manager_api_enabled`, `dom_webgl2_enabled`, `dom_webgpu_enabled`,
`layout_css_attr_enabled`, `layout_columns_enabled`, `layout_container_queries_enabled`,
`layout_grid_enabled`, `layout_variable_fonts_enabled`) were all still off too — notably
`dom_webgl2_enabled`, which meant `../test-page/`'s own `GpuInfoPanel`/PixiJS/Three.js
WebGL2 probes could never have seen a real WebGL2 context in this build, only WebGL1.

**Change:** all 18 `EXPERIMENTAL_PREFS` entries now default to `true` in
`Preferences::const_default()`, each commented at its own field. `dom_storage_manager_api_enabled`
additionally needed the `can_access_storage()` exemption from the entry above (same
opaque-origin gate, different call site) to actually work under `file://`, not just be
exposed.

**Why:** this fork exists to run real games' existing web code, not to browse the general
web — there's no "untrusted third-party site" threat model here that the
experimental/unstable split is protecting against (single bundled document, no navigation,
see the toolbar/tab-removal entries). Upstream's own curation is a reasonable, already-vetted
line to default this fork to, rather than either leaving real game APIs (WebGL2, WebGPU,
OffscreenCanvas, Notifications, CSS Grid/Container Queries, etc.) off by default or
re-deriving an equivalent list from scratch. Deliberately did **not** extend this to prefs
outside that bundle (e.g. WebRTC, Web Animations, Screen Wake Lock, Bluetooth, Geolocation,
Credential Management) despite some being plausibly game-relevant too — those aren't part of
upstream's own vetted experimental set, so enabling them here would be this fork's own
untested judgment call rather than reuse of an existing one; worth reconsidering
individually, not automatically, if a real game needs one.

**Side effects to know about when upgrading:** if a future Servo version changes
`EXPERIMENTAL_PREFS`'s membership (adds/removes entries, or an entry graduates to stable and
disappears from the list entirely), re-diff that list against this entry's 18 names rather
than assuming they still match. Verified with `cargo check -p servo-config` (clean, and this
crate has no heavy native deps so this check is fully trustworthy, unlike the `servo-script`
caveat above). Not yet verified end-to-end against a real build.

---

## 2026-08-07 — Disable the right-click context menu entirely

**Files:** `ports/servoshell/desktop/dialog.rs`, `ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0010-disable-context-menu-popup.patch`

**Upstream behavior:** right-clicking web content sends `EmbedderControl::ContextMenu` to the
embedder (`show_embedder_control` in `headed_window.rs`), which built a `Dialog::ContextMenu`
(`dialog.rs`) and rendered it as an `egui` popup — a browser-style menu (Back/Forward/Reload/
Copy Link/Open in New View/Cut/Copy/Paste/Select All, contextually filtered).

**Change:** `EmbedderControl::ContextMenu(prompt)` in `headed_window.rs` now just `drop(prompt)`
instead of building and showing a `Dialog`. `ContextMenu::drop` (`components/servo/
webview_delegate.rs`, unmodified) already sends the "no selection" response when a `ContextMenu`
is dropped without an explicit `select`/`dismiss` call, so this is a correct, immediate dismissal
from Servo's point of view — not a hang or a leaked request. With the only call site gone, the
entire `Dialog::ContextMenu` implementation in `dialog.rs` became dead code and was deleted
outright rather than left unreachable: the enum variant, its `update()` match arm (the `egui`
`Area`/`Frame` popup rendering and per-item button logic), its `embedder_control_id()` arm, and
the `new_context_menu` constructor. The now-unused bare `egui` imports (`Area`, `Button`,
`CornerRadius`, `Frame`, `Id`, `Order`, `Sense`, `Stroke`, `Vec2`, `pos2`) and `servo::{ContextMenu,
ContextMenuItem}` were removed from `dialog.rs`'s `use` statements accordingly — everything else
in that file still uses `Modal`/`RichText` and fully-qualified `egui::` paths for the pieces it
still needs, so those two stayed.

**Why:** [`TODO.md`](./TODO.md) point 2 — a videogame using this fork has no use for a
browser-style right-click menu, and entries like "View Source" or "Inspect" make no sense
outside an actual browser and would break immersion. Decided to disable it outright rather than
replace it with a custom menu (no game-relevant right-click actions were identified).

**Side effects to know about when upgrading:** if a future Servo version adds new
`ContextMenuAction`/`ContextMenuItem` variants, no code here needs updating — the menu is never
constructed at all, so nothing consumes those enums in this fork. **Not verified against an
actual `cargo check -p servoshell --bin servoshell`/`./mach build`**: this session's
`cargo check` got past a stale `webrender` build-script cache left over from before this
checkout was renamed/relocated (unrelated to this change — see the entry below), but then hit
`mozangle`'s (a `components/servo`/`components/script` dependency needed to build *any*
`servoshell` binary, on every platform, regardless of this change) `bindgen`-based build script
failing with "Unable to find libclang" — no `libclang.so` is installed in this sandbox and
`apt install libclang-dev` needs interactive `sudo` auth this session doesn't have. Same
category of gap as the `servo-script`/`llvm-objdump` caveat on the 2026-08-07 storage-access
entry above: a toolchain gap in *this* environment, not evidence of a code problem. What *was*
verified: all three edits (this entry and the two below) were applied with `patch -p1
--forward` to a fresh, unmodified extraction of the `v0.4.0` tag's `dialog.rs`/
`headed_window.rs`, applied cleanly with no fuzz/offset warnings, and the patched result was
byte-for-byte diffed against this fork's actual working copy of both files with zero
differences — so the patches are known to faithfully reproduce this exact change, even though
the change itself hasn't been compiler-checked here. Treat an actual build (with `libclang`
available) as the real verification before relying on this compiling.

---

## 2026-08-07 — Remove the page-reload keyboard shortcuts

**File:** `ports/servoshell/desktop/headed_window.rs`, `notify_input_event_handled` (was line
1061-1064 in the `v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0011-remove-page-reload-shortcuts.patch`

**Upstream behavior:** `Cmd`/`Ctrl+R` and `F5` both called `webview.reload()` unconditionally.

**Change:** both shortcut registrations removed from the `ShortcutMatcher` chain. Nothing else
in `notify_input_event_handled` references them; the zoom shortcuts immediately above are
untouched.

**Why:** [`TODO.md`](./TODO.md) point 3 — reloading resets whatever in-memory game state the
page has built up (this fork has no navigation history/session-restore concept to fall back on),
so an accidental `Ctrl+R`/`F5` is pure data loss for a player, with no equivalent "refresh the
page" use case a game needs. The right-click menu's own `Reload` entry is covered separately by
the 2026-08-07 context-menu removal above (the menu that would have offered it no longer exists
at all).

**Side effects to know about when upgrading:** `UserInterfaceCommand::Reload`/`ReloadAll` (
`running_app_state.rs`) and `WebView::reload()` itself are deliberately left alone — they're
still reachable from the `roves-api`/Android-embedding-style `egl::App::reload()` public API
(`ports/servoshell/egl/app.rs`) used by native host code on those targets, which is out of scope
for this entry (see `TODO.md` point 3's framing: keyboard/menu/gesture, not the embedding API
surface). Only the desktop keyboard entry points a player can trigger directly were removed.
**Not verified against an actual build** — same `libclang`/`mozangle` sandbox gap as the
context-menu entry above; verified the same way instead (patch applies cleanly to a pristine
`v0.4.0` `headed_window.rs` and reproduces this fork's actual file byte-for-byte).

---

## 2026-08-07 — Remove all back/forward history navigation

**File:** `ports/servoshell/desktop/headed_window.rs` (keyboard shortcuts, was line 384-405 in
the `v0.4.0` baseline; mouse side-button handling, was line 603-618).

**Patch:** `patches/servo-v0.4.0/0012-remove-back-forward-navigation.patch`

**Upstream behavior:** `Cmd/Ctrl+Alt+Right`/`Cmd/Ctrl+]` called `active_webview.go_forward(1)`,
`Cmd/Ctrl+Alt+Left`/`Cmd/Ctrl+[` called `active_webview.go_back(1)`, and pressing the mouse's
side "Forward"/"Back" buttons (`winit::event::MouseButton::Forward`/`Back`) queued
`UserInterfaceCommand::Forward`/`Back`, which `window.rs`'s `handle_interface_commands` resolves
to the same `go_forward`/`go_back` calls.

**Change:** the four keyboard shortcut registrations were removed from the `ShortcutMatcher`
chain (replaced with an explanatory comment, no replacement binding), which left the
`CMD_OR_ALT` import unused and it was removed too. The `MouseButton::Forward` and
`MouseButton::Back` match arms were merged into a single arm that still sets `consumed = true`
(so the button press doesn't fall through to `egui`/the page) but no longer queues a navigation
command — the side buttons are now inert rather than triggering history navigation.

**Why:** [`TODO.md`](./TODO.md) point 4 — for a game, navigating "back" out of the page's
current state can silently and completely break whatever the game was doing (no browser chrome
exists to explain what happened or offer "forward" as a recovery). This needed covering across
every input path a player could trigger it from: the keyboard shortcuts here, the mouse side
buttons here, and the context menu's own `GoBack`/`GoForward` entries, which are covered by the
2026-08-07 context-menu removal above (that menu no longer exists to offer them).

**Side effects to know about when upgrading:** same scope note as the reload entry above —
`UserInterfaceCommand::Back`/`Forward`, `WebView::go_back`/`go_forward`, and the `egl::App`
public API's `go_back()`/`go_forward()` (used by native host code embedding this engine, e.g. on
Android/OpenHarmony) are deliberately untouched; only the desktop player-facing input paths were
disabled. If a future Servo version adds another way to reach back/forward navigation from
player input (a new gesture, a new named key), re-apply the same reasoning to it. **Not
verified against an actual build** — same `libclang`/`mozangle` sandbox gap as the two entries
above; verified the same way instead (patch applies cleanly to a pristine `v0.4.0`
`headed_window.rs` and reproduces this fork's actual file byte-for-byte).

---

## 2026-08-07 — Aside: stale `webrender` build-script cache after this checkout was relocated

Not a patch, not a customization — a note for whoever next runs `cargo check`/`./mach build` in
this exact checkout. `webrender`'s build script had previously generated
`target/debug/build/webrender-*/out/shaders.rs` containing `include_str!("/home/simone/
template/pixi-vn-react-template/servo/target/...")` — an absolute path baked in from before this
directory was relocated/renamed to its current path. `cargo check -p servoshell` therefore failed
immediately with "No such file or directory" for every shader, unrelated to any Servo source
change. Fixed for this checkout with `cargo clean -p webrender` (forces the build script to
rerun and regenerate `shaders.rs` with the current, correct path). Not a code change, so no
patch/entry beyond this note — but worth knowing if a fresh `cargo check` mysteriously fails on
`webrender` shaders again after moving/copying this checkout. Clearing that cache unblocked
`webrender` but exposed a second, independent gap right behind it — see the `libclang`/
`mozangle` caveat repeated on all three entries above — so `cargo check -p servoshell --bin
servoshell` still doesn't currently complete in this particular sandbox even with this fixed.

---

## 2026-08-07 — Rename user/OS-facing labels from "Servo" to "Roves"

**Files:** `ports/servoshell/desktop/headed_window.rs`, `ports/servoshell/desktop/webxr.rs`,
`resources/org.servo.Servo.desktop` (renamed to `resources/org.roves.Roves.desktop`),
`python/servo/post_build_commands.py`. Also `.github/workflows/test.yml` (two comment/echo
strings, not patch-tracked — see caveat below) and `README.md` (not patch-tracked, see
`CLAUDE.md`'s scope note on what needs patches vs. plain doc updates).

**Patch:** `patches/servo-v0.4.0/0013-rename-servo-labels-to-roves.patch`

**Upstream behavior:** every label a player or the OS actually shows for this application
still said "Servo" (the upstream project's name), not "Roves" (this fork's actual product
name — see `README.md`'s intro): the window title (`INITIAL_WINDOW_TITLE`), the XR window
title, the Linux WM/taskbar app id (`with_name("org.servo.Servo", "Servo")`), the OpenXR
`AppInfo` application name, the `.desktop` launcher entry's `Name=`, and the custom `mach
bundle` command's (see the 2026-08-06 `mach bundle` entry above) generated macOS bundle name
(`Servo.app`) and `Info.plist` `CFBundleExecutable`/`CFBundleName`.

**Change:** each of the above now says "Roves" instead of "Servo":

- `headed_window.rs`: `INITIAL_WINDOW_TITLE = "Roves"`, XR window title `"Roves XR"`,
  `with_name("org.roves.Roves", "Roves")`.
- `webxr.rs`: `OpenXrAppInfo::new("Roves", 0, "Servo", 0)` — only the *application* name
  changed; the *engine* name argument deliberately still says "Servo", since the underlying
  engine genuinely still is Servo (see "Deliberately left alone" below).
- `resources/org.servo.Servo.desktop` renamed to `resources/org.roves.Roves.desktop`,
  `Name=Servo` → `Name=Roves`, and the file's own self-referential setup instructions
  (`cp org.servo.Servo.desktop ...`) updated to match the new filename.
- `post_build_commands.py`'s `_bundle_macos`/`bundle` methods: `Servo.app` → `Roves.app` (both
  the folder name and the docstring describing it), and the generated `Info.plist`'s
  `CFBundleExecutable`/`CFBundleName` (and the launcher script file they must match,
  `Contents/MacOS/Servo` → `Contents/MacOS/Roves`) → `"Roves"`.
- `.github/workflows/test.yml`: two comment/log strings referencing `Servo.app` updated to
  `Roves.app` to stay consistent with the rename above (this workflow only describes/exercises
  `mach bundle`'s output; see its own header comment on why it doesn't run anywhere yet).
- `README.md`: added a "Naming" section explaining the current state (Roves labels vs. the
  still-Servo-named engine/binary) so this doesn't read as an inconsistency to a new reader.

**Deliberately left alone (broader rename intentionally out of scope for this change):**

- The `servoshell` Cargo package/binary name itself (directory `ports/servoshell/`, `[package]
  name = "servoshell"` and everything derived from it) — considered, and **decided against**,
  not just deferred. A repo-wide `grep -rl servoshell` turns up ~50 files, including upstream
  Python build-system internals under `python/servo/` (`command_base.py`, `gstreamer.py`,
  `devtools_tests/*`, etc.) that have nothing to do with branding, plus `Cargo.lock`
  regeneration — too large and too unverifiable in a sandbox without `libclang` (see the
  caveats above) for no functional benefit. `servoshell` stays the binary/package name going
  forward; see `README.md`'s "Naming" section.
- `ContextMenu`/`CFBundleIdentifier` (`org.servo.servoshell.bundle`) and Android's
  `ANDROID_APP_NAME` (`org.servo.servoshell`, `post_build_commands.py`) — bundle/package
  identifiers, not display labels; changing an Android package name in particular makes the
  OS treat it as a completely different app (losing update continuity), so this needs an
  explicit decision, not a mechanical rename alongside display strings.
- `ports/servoshell/prefs.rs`'s on-disk config directory name (`config_dir.push("Servo")`) —
  changing this moves where preferences/persistent data are read from on an existing install;
  needs a migration decision, not a silent rename.
- `ports/servoshell/platform/macos/Info.plist` (upstream's own static bundle metadata, used by
  `./mach package`, a different mechanism than the `mach bundle` command above) and
  `etc/macos_sign.py`/`support/macos/Servo.entitlements` (codesigning/notarization) — a larger,
  more sensitive surface (signing identity, entitlements) not covered by this pass.
- `webxr.rs`'s OpenXR *engine* name argument (see above) and any other place that credits the
  actual Servo engine rather than naming the product — this fork doesn't rename Servo's
  internals, only the shell around it (see `README.md`'s "What this is").

**Why:** this fork's product is Roves, not Servo (see `README.md`), but several places a
player or the OS directly shows text still said "Servo" — inconsistent with the actual product
identity and confusing for anyone who notices (window title bar, Alt-Tab/taskbar, Finder/dock,
the Linux app menu). Scoped deliberately to *display labels only* (not the underlying
binary/package name, bundle identifiers, on-disk paths, or codesigning) after discussing the
size/risk of the full rename with the project owner — see the deliberately-left-alone list
above and `TODO.md` point 4 for what's still open.

**Side effects to know about when upgrading:** `.github/workflows/test.yml` and `README.md`
are this fork's own files with no upstream counterpart, so they aren't part of the
patch-against-pristine-tag mechanism the same way source files are (no prior patch in this
directory touches `.github/`, by existing convention — see e.g. patches 0001-0012, none of
which touch `.github/`); their changes are captured here in prose only, not replayed by
`0013`'s patch — if this repo is ever restructured so `.github/` *is* patch-tracked, fold
these two small string changes in then. If a future Servo version changes `AppInfo`'s
constructor signature/parameter order in `components/webxr/openxr/mod.rs`, double check which
argument is `application_name` vs `engine_name` before reapplying — swapping them would put
"Roves" in the wrong field.

---

## 2026-08-07 — Pack game content into compressed archives instead of shipping loose files

**Files:** `python/servo/post_build_commands.py` (`bundle`, `_place_bundle_content`,
`_bundle_windows`/`_bundle_macos`/`_bundle_linux`/`_bundle_linux_deb`), `Cargo.toml` (new
workspace member), and a brand-new crate, `support/content-packer/` (bin
`roves-content-packer`). Also `test-page/public/` — new test fixtures, not part of the patch
(see "Side effects" below).

**Patch:** `patches/servo-v0.4.0/0014-pack-and-compress-game-content.patch`

**Motivating problem:** since the 2026-08-06 `mach bundle` entry above, `--content-dir` (a
game's built `dist/`) was copied into the release bundle with a plain `shutil.copytree` —
every source file landed in the shipped zip exactly as built, trivially browsable/extractable
by anyone who unzips a release. Nothing about `mach bundle`'s job (produce something
double-click-runnable) required that; it was just the simplest thing `_place_bundle_content`
could do.

**Change:** `mach bundle` gained four new flags: `--content-compress {auto,none}` (default
`auto`), `--content-compression-level N` (default `1` — zstd, favoring speed over ratio),
`--content-max-pack-size SIZE` (default `500M`), `--content-exclude GLOB` (repeatable). With
the new default, `--content-dir` is no longer copied in as loose files. Instead:

- `_place_bundle_content` shells out to `roves-content-packer pack`, which walks
  `--content-dir` and splits it into a small, fixed number of `tar`+`zstd` archives
  (`.pack` files) plus a `manifest.json`, by depth: dist's own root-level files → one archive;
  each direct subfolder's own direct files → one archive per subfolder; everything deeper than
  that (grandchildren and beyond) → one more archive per top-level subfolder, with every
  descendant flattened into it (tar entries keep their full relative path, so unpacking
  reconstructs the original tree regardless of which archive a file ended up in). Each archive
  is capped at `--content-max-pack-size`, splitting into `.1.pack`/`.2.pack`/... past that.
  Files whose extension is already internally compressed (images, audio, video, fonts,
  archives — see `STORED_EXTENSIONS` in `support/content-packer/src/pack.rs`) go into a
  separate, *uncompressed* tar per bucket (`<bucket>.stored.pack`) instead of being fed through
  zstd a second time for no size benefit. `--content-exclude` globs (matched relative to
  `--content-dir`) are left as plain, unpacked files instead — e.g. a save-data/user-config
  subfolder a game ships inside `dist/` that shouldn't sit inside a read-only archive.
  Archiving order is fully deterministic (sorted paths throughout), so packing the same input
  twice produces byte-identical output — `manifest.json`'s `content_hash` (sha256 over every
  packed/excluded file's path+contents, in that same sorted order) changes iff the real output
  would.
- Each generated launcher (`play.exe`/`Roves`/`play.sh`/the `.deb`'s `/usr/bin/<pkg>` script)
  gets a copy of `roves-content-packer` alongside itself, and now runs its `extract`
  subcommand — synchronously, before starting the engine — to reconstruct plain files. Called
  with no `--dest`, `extract` picks (and prints) its own location under the OS temp directory
  (`std::env::temp_dir()`), keyed by a hash of the resolved `--content-dir` path, and each
  launcher captures that printed path (`CACHE_DIR="$(./roves-content-packer extract
  --content-dir ...)"` in bash; `Command::output()` — not `.status()` — on Windows) to build the
  engine's real html-file argument at *this* launch. Nothing is ever extracted next to the
  bundle itself, on any platform — see the same-day correction below for why that changed.
  `extract` skips the work entirely on a re-launch with unchanged content: it compares
  `manifest.json`'s `content_hash` against a `.content-hash` marker left in the destination
  from the previous run, so the OS temp directory being outside the bundle doesn't mean paying
  the decompression cost on every single launch.
- `--content-compress=none` restores the exact previous behavior (plain `copytree`, no
  `.content-cache` indirection, no packer binary shipped) — an escape hatch, not a special
  case sprinkled through the packing logic: `_place_bundle_content` branches on it right at the
  top and returns early.

**Why:** a fork whose stated purpose is embedding a game (see `README.md`) shouldn't make
that game's own source assets sit unprotected in every release by default — a curious end
user opening the zip finds a `tar`+`zstd` archive, not a folder of ready-to-copy JS/images.
"Very little compression, prioritize speed" (low zstd level, skip already-compressed
extensions entirely) was the explicit ask driving the tool choice: `tar`+`zstd` because zstd
is a widely-deployed, mature compression format with first-class Rust bindings already
resolvable in this workspace's `Cargo.lock` (pulled in transitively before this change), not
because of any exotic requirement. Splitting into a handful of archives by folder depth
(rather than either one giant archive or one archive per file/folder) balances two things a
single choice can't: fewer files to manage/ship than "one per folder" would produce on a
deeply-nested asset tree, while still keeping any *individual* archive small enough to
regenerate/re-download cheaply and to respect `--content-max-pack-size` without needing to
chunk a single archive mid-stream. The launch-time (not bundle-build-time) extraction step,
plus the content-hash cache, is what actually delivers on "not sitting in the clear on disk
by default": the shipped artifact itself never contains a plain copy, and a normal user run
only ever produces one in a temp/cache-style location, re-derived from the archives rather
than persisted as the source of truth.

**Deliberately left out of this pass (see the AskUserQuestion exchange that shaped this
entry, in the conversation that produced it, for the full list and rationale):** per-file
hashes in the manifest (no current consumer — nothing here does incremental
patching/integrity verification yet, so it would be dead weight); any actual encryption or
DRM-style obfuscation (the request was specifically for compression, and a fake-security XOR
scheme would be worse than no scheme — see `support/content-packer/`'s own lack of one). Real
protection against a motivated reverse-engineerer is a materially different, larger feature
and wasn't asked for.

**Correction (same day):** the first version of this extracted into a fixed `.content-cache/`
directory sitting right next to the bundle (baked into the html-file launch arg at
`mach bundle` time), and each launcher ran the extractor via a blocking call that, on Windows,
is a console-subsystem child process spawned from a `windows_subsystem = "windows"` parent —
which flashes a console window for an instant before servoshell's own window opens. Two
problems reported after trying an actual bundle: that window flash (read as "two windows, one
closing to open the other"), and not wanting a `.content-cache/` folder visibly sitting inside
the shipped game folder at all, even though it holds re-derived, re-creatable content rather
than anything load-bearing. Fixed by (1) making `--dest` optional in `extract`, defaulting to
a hash-keyed path under the OS temp directory instead of a caller-supplied one, which is what
let every launcher stop hardcoding a bundle-relative cache path and made the `.deb` launcher's
old `${XDG_CACHE_HOME:-$HOME/.cache}/<package_name>/content-cache` special case (needed only
because `/usr/lib/<package_name>/` isn't user-writable) unnecessary too — `temp_dir()` resolves
to something writable regardless of `--content-dir`'s own location; and (2) passing
`CREATE_NO_WINDOW` (`0x0800_0000`) via `CommandExt::creation_flags` on the Windows launcher's
child `Command`. A true zero-disk-writes design (a custom `content:` protocol handler
decompressing on demand, entirely in memory, never touching any filesystem path) was
considered and explicitly deferred rather than chosen — see "Deliberately left out" above and
this file's own note on why: it would need `components/url/origin.rs`'s `file://`-specific
opaque-origin/storage-access/trustworthy-origin carve-outs (2026-08-06 "Stable `file://`
origin" and 2026-08-07 "Storage access for `file://` origins" above) extended to a second
scheme, which is real spec-sensitive surface to get right and verify, not something to rush
through right before a release. OS-temp-directory extraction gets the actual complaint (no
visible artifact in the shipped game's own folder) without touching that surface at all.

**Side effects to know about when upgrading:** none of this touches Servo internals — it's
new Python (a fourth `_bundle_*` parameter plus a new helper) and a wholly new, dependency-thin
Rust crate (`tar`, `zstd`, `walkdir`, `glob`, `sha2`, `serde`/`serde_json`, `bpaf` — all either
already workspace dependencies or already resolvable in `Cargo.lock` at the versions pinned in
`support/content-packer/Cargo.toml`), so it should survive a version bump untouched as long as
`_place_bundle_content`'s and the four `_bundle_*` methods' call sites in `bundle()` aren't
restructured upstream (they're 100% this fork's own code, not upstream Servo's, so that risk
is really just "did a later patch in this same set change their signatures again" — check
`0004`/`0013` first). `test-page/public/`'s new image/audio/JSON/SVG fixtures (root files, two
direct subfolders each with their own files plus a nested sub-subfolder) exist purely to give
`test.yml`'s `npm run build` something realistic to exercise all three archive levels against
— they aren't part of the patch set since they're not derived from any upstream file at all,
just plain test fixtures tracked directly in this repo.

---

## 2026-08-08 — Split packed content into an eager "boot set" + lazy, on-demand extraction

> **Note (2026-08-08, later same day):** the "generated launcher calls `roves-content-packer
> extract`" description below (Windows/macOS/Linux launcher scripts/binaries) is superseded
> by the "Single-executable bundle" entry near the end of this file — that extraction call
> now happens in-process, inside the engine binary itself, via the same
> `roves_content_packer::extract` functions named below. Everything else in this entry (the
> boot/lazy split itself, the manifest format, the `file:` handler) is unaffected and still
> current.

**Files:** `components/servo/servo.rs` (protocol-registry merge order), `components/servo/lib.rs`
(new re-exports), a brand-new `ports/servoshell/desktop/protocols/file.rs`, plus
`ports/servoshell/desktop/protocols/mod.rs`/`desktop/app.rs` (registration),
`ports/servoshell/Cargo.toml` (new dependency), `python/servo/post_build_commands.py`, and
`support/content-packer/` (manifest format v2, `pack`/`extract` behavior, new `src/lib.rs` +
`tests/roundtrip.rs`).

**Patch:** `patches/servo-v0.4.0/0015-lazy-on-demand-content-extraction.patch`

**Motivating problem:** the previous two entries above extract *all* packed content eagerly
before the engine starts — fine for a small diagnostic page, but for a game whose assets reach
the multi-GB range, that means (a) a first-launch (or first-launch-after-a-content-update)
stall proportional to the *entire* game's size, even for content the player won't touch for
hours (a later level, an optional cosmetic pack), and (b) briefly needing disk space for both
the compressed archives and the full decompressed copy at once. Raised directly: "il
funzionamento... in un progetto di GB non rallenterà il sistema??"

**Change:** `roves-content-packer pack` now splits packed content into two tiers instead of
one:

- A small **boot set** — the html file itself, plus every local `src=`/`href=` it references
  directly (a lightweight attribute scan, not a full HTML parser; catches a bundler's entry
  `<script>`, `<link rel="modulepreload">` hints, a favicon, etc. — verified against
  `test-page/dist`'s real Vite output), plus anything matching the new `--boot-include`
  (`mach bundle --content-boot-include`) glob. These get their own dedicated archive(s)
  (`__boot__.pack`/`__boot__.stored.pack`), extracted eagerly by `roves-content-packer extract`
  (still called by every generated launcher exactly as before — its *contract* didn't change,
  only what it extracts) before the engine even starts.
- **Everything else** stays compressed. `manifest.json` (format v2) now also carries a
  `files: {path: pack}` map, so a specific path can be traced back to the one archive that
  holds it without touching any other. Nothing extracts it until something actually asks —
  which is the new part.

That "something asking" is a new `file:` protocol handler
(`ports/servoshell/desktop/protocols/file.rs`), replicating the stock handler's behavior
(plain reads, HTTP Range support for `<video>`/`<audio>` seeking) with one addition: if a
requested path doesn't exist yet and falls under the known content-cache directory, it
decompresses whichever pack contains it (a `roves_content_packer::extract::ensure_file_available`
call, guarded by a mutex so two near-simultaneous requests for the same not-yet-extracted pack
don't race) *before* the read proceeds — extraction is per-pack, not per-file (tar/zstd can't
cheaply seek to one member without processing everything before it), so a request "waits" at
most for its own bucket's archive, never the whole game. This is why `ensure_pack_extracted`'s
marker-file cache (see the previous entry) had to become *incremental*: a destination now
holds boot files plus whichever lazy packs have been touched so far, and only a genuine
content change (a mismatched `content_hash`) wipes it — an unchanged relaunch keeps everything
already extracted in earlier sessions, not just the boot set.

Registering a *custom* `file:` handler at all needed one real engine change:
`ProtocolRegistry::merge`'s `entry().or_insert()` only fills vacant slots, and
`components/servo/servo.rs` built the internal-defaults registry (which always has `file`)
first, then merged the embedder's registry into *that* — meaning an embedder's own `file`
registration was always silently discarded. Swapped the merge direction (embedder's registry
first, internal defaults merged in on top) so it isn't, for every scheme an embedder
explicitly claims — a no-op for any other embedder of the `servo` crate, since none of them
register `file`/`data`/`blob` today. `components/servo/lib.rs`'s `protocol_handler` facade
module also gained re-exports of three already-`pub` `net::protocols` functions
(`get_range_request_bounds`/`partial_content`/`range_not_satisfiable_error`) so the new
handler could reuse them instead of reimplementing Range-request math.

**Deliberately not replicated:** the stock handler's directory-listing fallback
(`local_directory_listing`) — Roves never opens more than one `file://` document and never
navigates to a bare directory (no address bar, no tabs), so that code path doesn't apply, and
reusing it would need a `pub(crate)` → `pub` visibility patch to `components/net` this doesn't
justify. A directory request now returns a network error instead.

**Why:** the boot set is deliberately *tiny by construction* (an entry chunk, a stylesheet, an
icon), so paying its extraction cost on every cold start is cheap regardless of the game's
total size — the multi-GB case only ever pays for what a session actually touches, exactly
once, and everything already touched survives a relaunch unless the content genuinely
changed. Note the honest limit: how small the boot set stays is downstream of the game's own
bundler code-splitting — a bundler that statically imports (or `modulepreload`-hints) most of
the app from the entry HTML will end up with most of the app in the boot set too, same as it
would eagerly fetch it in a browser regardless of this feature. Structuring lazy content
behind dynamic `import()` (standard web performance practice already) is what actually keeps
the boot set small for a large game — this tool respects exactly what the entry HTML declares
as immediately needed, it doesn't second-guess it.

**Explicitly considered and deferred:** a native "loading" splash shown by the launcher during
boot extraction. Not implemented — the boot set is small enough that this gap is typically
sub-second, and a real per-platform splash window (creation, synchronization with the
launcher's blocking extraction call, closing it at the right moment the engine's own window
appears) is a meaningfully sized, purely additive UI task on its own; standard web practice
(a page's own loading indicator while it fetches further lazy assets) already covers the more
common case of *waiting on gameplay assets*, which is the game's own responsibility, not
Roves'.

**Side effects to know about when upgrading:** the `components/servo/servo.rs` merge-order
swap and the `components/servo/lib.rs` re-exports are the first changes in this patch set that
touch genuinely shared engine code rather than code local to `ports/servoshell` or
`support/content-packer` — re-verify both still make sense if a future Servo version reshapes
`ProtocolRegistry`/`protocol_handler`'s module layout. **Not independently verified by a real
build in this environment** — `cargo check -p servoshell` (and `-p servo`, which
`ports/servoshell/desktop/protocols/file.rs` and the `servo.rs`/`lib.rs` changes are part of)
both hit the pre-existing `libclang`/`bindgen`/`mozangle` gap noted elsewhere in this file
before ever reaching this code; `roves-content-packer` itself (manifest v2, boot detection,
the incremental extraction cache, `ensure_file_available`) is fully covered by
`support/content-packer/tests/roundtrip.rs` and passes. Treat the servoshell-side pieces as
reviewed-but-not-compiled until a real `./mach build` confirms them.

**Correction (next day, after `.github/workflows/test.yml` actually ran this patch on real
Windows/macOS/Linux runners):** exactly one compile error, on every platform —
`ports/servoshell/desktop/protocols/file.rs`'s `use http::Method;` was an unresolved import
(E0432). `headers` (already a `servoshell` dependency, used for the rest of this file's Range
handling) doesn't re-export the `http` crate; `http` itself is a workspace dependency
(`Cargo.toml`'s `[workspace.dependencies]`) but hadn't been added to
`ports/servoshell/Cargo.toml`'s own `[dependencies]` — Rust's crate resolution needs it listed
on the crate that actually uses it, not merely present somewhere in the workspace lockfile.
Fixed by adding `http = { workspace = true }` there. Exactly the gap the note above flagged
("reviewed-but-not-compiled") — this is that real `./mach build` confirmation, and it caught a
real, if narrow, bug on the first try.

---

## 2026-08-08 — Accept absolute Windows paths on the command line (`Unsupported scheme` fix)

**File:** `ports/servoshell/parser.rs`

**Patch:** `patches/servo-v0.4.0/0016-accept-absolute-windows-paths-on-the-command-line.patch`

**Symptom:** on Windows, the bundled `play.exe` produced by `mach bundle` opened a window
showing nothing but Servo's network-error page — *"Could not load the requested page:
Unsupported scheme"* — instead of the game. Linux (`play.sh`) and macOS (`Roves.app`) were
unaffected.

**Cause:** `parse_url_or_filename` tries `ServoUrl::parse(input)` first and only falls back to
"treat this as a filename" on `ParseError::RelativeUrlWithoutBase`. An absolute Windows path
never produces that error: per the WHATWG URL spec, `C:\dir\index.html` parses *successfully*
as scheme `c` with the opaque path `\dir\index.html` (a single letter is a valid scheme). So
`get_default_url` sees a URL whose scheme isn't `file`, whose `to_file_path()` fails, and whose
scheme is neither localhost nor domain-like — every arm misses, it falls through to
`location_bar_input_to_url`, which re-parses the same string and hands back that same `c:` URL.
The engine then fetches it, `components/net/fetch/methods.rs`'s `scheme_fetch` finds no handler
registered for `c`, and returns `NetworkError::UnsupportedScheme`. POSIX absolute paths dodge
all of this because a leading `/` genuinely is not a scheme, so they do fail to parse and do
reach the filename fallback — which is why this only ever bit Windows.

The bug is latent upstream (`servo.exe C:\page.html` has presumably always behaved this way),
but only became *reachable for Roves* with the lazy-extraction entry above: `play.exe` now
passes an **absolute** html path — `roves-content-packer extract`'s printed cache directory,
which isn't known until launch time — where it previously passed a bundle-relative path that
parsed as a relative URL and worked fine.

**Change:** before the existing `ServoUrl::parse` attempt, detect the two absolute-Windows-path
shapes (`C:\…` / `C:/…` drive-absolute, and `\\server\share\…` UNC) with a new
`is_windows_absolute_path` helper, and for those hand the string straight to
`url::Url::from_file_path`. On Windows that yields the correct `file:///C:/…` URL with no host
(exactly what `get_default_url`'s `("file", None, Ok(path))` arm expects) and with each segment
percent-encoded properly. Everything else takes the original path unchanged.

**Why here and not in the launcher:** hand-assembling a `file:///` string inside the generated
`play.exe` (which is compiled by a bare `rustc` invocation with no dependencies available, so
no `url` crate) would mean hand-rolling percent-encoding for paths containing spaces, `#` or
`?` — and Windows temp directories live under the user's profile, so `C:\Users\Mario
Rossi\AppData\Local\Temp\…` is entirely ordinary, not an edge case. Fixing it in the parser
gets correct encoding from `Url::from_file_path` for free and makes *any* Windows path passed
to servoshell on the command line work, not just the launcher's.

**Deliberately not `#[cfg(windows)]`-gated:** the conversion only happens if
`Url::from_file_path` succeeds, and on a non-Windows build it rejects both shapes (neither is
an absolute path there), so behavior off Windows is provably unchanged — one code path to
reason about instead of two.

**Known limit, left alone:** the UNC shape now parses into a `file://server/share/…` URL, whose
host is `Some`, so `get_default_url`'s `("file", None, …)` arm still won't take it and a UNC
argument still falls through to the homepage. That's unchanged from before this fix (it
previously failed even earlier), Roves' launchers never generate one, and widening that arm is
a separate judgment call about whether an embedded game should load its content off a network
share at all.

**Verification:** the WHATWG parse behavior was confirmed directly against `url` 2.5.4 —
`Url::parse(r"C:\Users\me\AppData\Local\Temp\roves-content-ab12/index.html")` returns
`Ok`, scheme `"c"`, `to_file_path()` `Err`; the POSIX equivalent returns
`Err(RelativeUrlWithoutBase)`. The Windows side of `from_file_path`
(`path_to_file_url_segments_windows`) was read to confirm it maps a `Prefix::Disk` to a
host-less `file:///C:/…` serialization and percent-encodes each segment, and that mixed
separators (`C:\dir/index.html`, exactly the shape `play.exe`'s `format!` produces) are handled
— `Path::components()` splits on both on Windows. Not exercised by a compiled Windows build in
this environment; confirm with a real `mach bundle` + `play.exe` run.

---

## 2026-08-08 — Single-executable bundle: eliminate the separate launcher

**Files:** `support/content-packer/src/manifest.rs`/`pack.rs` (new `Manifest::entry_html`
field, `FORMAT_VERSION` bumped to 3), a brand-new `ports/servoshell/desktop/bundle_launch.rs`,
plus `ports/servoshell/desktop/mod.rs`/`desktop/cli.rs` (registration/call site),
`ports/servoshell/build.rs` (Linux rpath), `ports/servoshell/Cargo.toml` (winresource
metadata), `ports/servoshell/platform/windows/servoshell.exe.manifest` (assembly identity),
and `python/servo/post_build_commands.py` (every `_bundle_*` method, `bundle()` itself).

**Patch:** `patches/servo-v0.4.0/0017-single-executable-bundle.patch`

**Motivating problem:** every `mach bundle` output shipped **two** executables — a tiny
generated launcher (`play.exe` / `play.sh` / a bash script inside `Roves.app`) plus the real
engine binary, hidden in a `bin/` subdirectory (Windows, still literally named
`servoshell.exe` there) or under a `<binary>-core` suffix (macOS/Linux). The launcher's only
job was: run `roves-content-packer extract` (also shipped into the bundle, a *third*
executable), capture the cache directory it printed, and spawn/exec the real binary with the
resolved html path + `--window-size` + extra args. Noticed directly: a Windows user found
`servoshell.exe` sitting in `bin/` and asked for exactly one executable, named `play`,
everywhere — not just hidden better.

**Change:** the engine binary itself now does what the launcher used to do, in-process,
before opening any window — and is shipped as the single executable directly, under the
`play`/`play.exe`/`Roves` name. Concretely:

- `roves_content_packer::extract` was already linked into `servoshell` as a library (used by
  `desktop/protocols/file.rs` for on-demand lazy extraction — see the entry above) — the new
  `desktop/bundle_launch.rs` module calls the exact same `load_manifest`/`extract_boot`
  functions the launcher used to reach via a subprocess, so no `roves-content-packer` binary
  needs to ship to players at all anymore (still built and used locally, on the machine
  running `mach bundle`, to run `pack` — see `_build_content_packer`'s updated docstring).
- `mach bundle` writes a small `launch.json` next to the shipped binary instead of
  generating+compiling/writing a wrapper. Shape: `{"content_dir": "dist", "url": null,
  "args": ["--window-size", "1280x720", ...]}` for packed content, or `{"content_dir": null,
  "url": "dist/index.html", "args": [...]}` for `--content-compress=none`/loose builds. All
  paths relative to `launch.json`'s own directory.
- `bundle_launch::resolve_bundled_launch_args()`, called from `desktop/cli.rs` right where
  `env::args()` used to be read directly: looks for `launch.json` next to
  `env::current_exe()` (on macOS, content itself is resolved one level up into the sibling
  `Contents/Resources/`, matching the standard app-bundle layout — `launch.json` itself still
  sits next to the binary in `Contents/MacOS/`). If `content_dir` is present, resolves it,
  calls `extract_boot`, and uses `dest.join(&manifest.entry_html)` as the resolved URL — which
  is *why* `Manifest` gained `entry_html` (bumped to format v3): the actual entry file to open
  after extraction was previously known only because each generated launcher had it baked
  into its own source at bundle time; now the engine needs to look it up itself, and the
  manifest is the only thing both `pack` (dev machine) and the engine (player's machine)
  share. Falls back to the engine's normal preference-based default (rather than crashing) if
  extraction fails, and returns the resolved args in exactly the shape the old launchers
  already passed as argv (`[url, "--window-size", "WxH", ...]`), so nothing downstream
  (`prefs.rs`'s `bpaf` parsing, `parser.rs`'s `get_default_url`, `app.rs`) needed to change at
  all.
- **Critical safety rule:** `resolve_bundled_launch_args()` only ever consults `launch.json`
  when the process's real `argv` is completely empty. This isn't just conservatism — Servo's
  own multiprocess mode re-executes the running binary as content-process children with
  `--content-process <token>` in their argv (`prefs.rs`'s `content_process` field); once the
  launcher and the engine are the same binary, that child relaunch would otherwise find
  `launch.json` right next to itself too and silently discard its real startup args. Verified
  `multiprocess` defaults `false` and `mach bundle` never threads `-M`/`--multiprocess`
  through today, so this was latent rather than actively triggered — but it's a real
  correctness requirement for any future `mach bundle -- -M` (or a project that ever enables
  it), not just defensive style. An explicit invocation (a developer running the shipped
  binary from a terminal, a Steam launch-options override) hits the same rule and is likewise
  always left untouched.
- Linux never had a linker rpath (unlike the macOS `-rpath @executable_path/lib/` a previous
  entry added) — `.so` resolution was entirely `play.sh`'s job, setting `LD_LIBRARY_PATH`
  before exec. With that wrapper gone, `build.rs` now also emits
  `-Wl,-rpath,$ORIGIN` for Linux, so the single binary finds its sibling `.so` files itself.
- `_bundle_linux_deb`'s `/usr/bin/<package_name>` is now a plain symlink to the real binary
  under `/usr/lib/<package_name>/`, not a wrapper script — a symlink is a filesystem alias,
  not a second process, and `env::current_exe()` (what `bundle_launch.rs` looks next to)
  resolves through it via `/proc/self/exe` automatically. One real bug caught while writing
  this: `installed_size_kb`'s `os.walk` + `path.getsize` combination would have raised
  `FileNotFoundError` trying to follow that symlink's absolute target
  (`/usr/lib/<pkg>/<binary>`), which doesn't exist on the *build* machine — switched to
  `os.lstat(...).st_size` (the symlink's own tiny size, matching how `du`/real `dpkg-deb`
  account for symlinks) to actually verify this before it shipped.
- Two places baked a literal "ServoShell" string into the *artifact itself* (not just a
  filename, so renaming the shipped file alone wouldn't have erased them, unlike when the
  hidden `servoshell.exe` sat safely inside `bin/`): `Cargo.toml`'s
  `[package.metadata.winresource]` (`FileDescription`/`ProductName`/`OriginalFilename`, read
  by `winresource` into the `.exe`'s Windows version resource — visible via Explorer's
  Properties → Details) and `servoshell.exe.manifest`'s `assemblyIdentity name=`. Both now say
  "Roves"/`org.roves.Roves`, matching the WM-class string `headed_window.rs` already used.
  **Deliberately did *not* touch** `_bundle_macos`'s `Info.plist` `CFBundleIdentifier`
  (`org.servo.servoshell.bundle`) even though it's the same kind of leaked string — the
  2026-08-07 rename entry above already considered this exact string and deliberately
  deferred it (a bundle identifier affects macOS-level identity — defaults/prefs, TCC
  permission grants, code signing — unlike a display label, so it needs its own explicit
  decision). Caught and reverted a first pass at changing it anyway before finalizing this
  entry; left a comment at the call site pointing back at that reasoning. Also deliberately
  left `resources/org.roves.Roves.desktop` alone — it's dev-only instructions for pinning a
  local build to your own taskbar (mentions "Servo sources" accurately for that use), not
  anything `mach bundle` actually produces; the real `--deb` desktop-entry generation was
  already clean. Also changed `--deb-package-name`'s default from `servoshell` to `roves`.

**Why:** the user's ask was specifically "the only exe should be play, not servoshell,
anywhere" — hiding the second binary better (as macOS/Linux already did with the `-core`
suffix) wasn't enough; a real second binary still existed to be found. Making the engine
absorb the launcher's job removes it entirely rather than hiding it further, and was
free-of-new-dependencies since `roves_content_packer` was already linked in as a library for
an unrelated reason (on-demand lazy extraction).

**Verification:** `support/content-packer` (`entry_html`/`FORMAT_VERSION` bump) compiles and
tests cleanly in a plain sandbox — no `libclang`/`bindgen` dependency — confirmed with
`cargo test --manifest-path support/content-packer/Cargo.toml`: all pre-existing
`tests/roundtrip.rs` cases still pass, plus a new assertion that `load_manifest(...).entry_html
== "index.html"`. `bundle_launch.rs`'s pure logic (JSON parsing, the macOS `../Resources`
join, a real `extract_boot` call against a packed fixture) was additionally exercised in an
isolated standalone Rust program using this workspace's actual crate versions. **The
`servoshell`/`servo` crates themselves were not compiled** — same pre-existing
`libclang`/`bindgen` gap noted in the entry above and elsewhere in this file. `python/servo/post_build_commands.py`
was syntax-checked (`ast.parse`/`py_compile`) but not run end-to-end (no real `mach build` in
this environment). **Needs a real `./mach build && ./mach bundle` to confirm end-to-end** —
the previous entry's Windows path-parsing fix was already confirmed working on the user's own
Windows machine; this change should be verified there too (single `play.exe`, no `bin/`, game
actually launches), and ideally spot-checked by file listing on Linux/macOS if available,
though full runtime testing there isn't expected in this pass.

---

## 2026-08-09 — Native boot splash instead of a blank window during first-run extraction

**Files:** `support/content-packer/src/extract.rs`, `ports/servoshell/desktop/bundle_launch.rs`,
`ports/servoshell/desktop/cli.rs`, `ports/servoshell/desktop/event_loop.rs`,
`ports/servoshell/desktop/app.rs`, `ports/servoshell/desktop/gui.rs`,
`ports/servoshell/desktop/headed_window.rs`, `ports/servoshell/desktop/tracing.rs`.

**Patch:** `patches/servo-v0.4.0/0018-native-boot-splash-screen.patch`

**Upstream behavior:** for a packed-content bundle, `resolve_bundled_launch_args()`
(`bundle_launch.rs`, see the "Single-executable bundle" entry above) called
`extract::extract_boot` — the actual, potentially slow decompression of the boot pack set —
synchronously, in `cli.rs`, *before* the winit event loop or window even existed. On a slow
first launch (large boot set), there was nothing on screen at all during that wait; once the
window did appear, it showed a blank/uninitialized frame until Servo's compositor painted the
real page on top. No splash/loading-screen mechanism existed at all (a doc comment in
`pack.rs` floated the idea of an in-page HTML splash image via `--boot-include`, but nothing
implemented it, native or otherwise).

**Change:** the window now appears immediately, showing a minimal native splash (black
background, the Roves icon + "Roves" in white, and — after a ~300ms delay, so a fast/no-op
extraction never flashes one — a progress bar tracking real extraction progress) instead of
whatever was there before. Deliberately unstyled beyond that; styling is a follow-up.

- `extract.rs`: `resolve_dest` is `prepare_dest`'s canonicalize-and-pick-destination half,
  split out and made `pub` so a caller can learn *where* boot content will land (to build the
  `file:` URL it'll eventually load) without paying for decompression. `extract_boot`'s body
  is factored into a private `extract_boot_impl(opts, on_progress: Option<&mut dyn
  FnMut(f32)>)`; `extract_boot` (unchanged signature, all existing call sites — the CLI, the
  6 `roundtrip.rs` tests — untouched) calls it with `None`. New `pub fn
  extract_boot_with_progress(opts, on_progress: impl FnMut(f32))` calls it with `Some`,
  reporting `done/total` after each boot pack plus a final `1.0` — coarse (per-pack, not
  per-byte) but boot sets are deliberately just the html file and whatever it directly
  references, so usually only one or two packs. No manifest/`FORMAT_VERSION` changes needed.
- `bundle_launch.rs`: `resolve_packed_content_url` no longer calls `extract_boot` at all —
  only `extract::resolve_dest` (path/hash math) plus the already-loaded manifest's
  `entry_html`, both fast. `resolve_bundled_launch_args()` now returns `Option<BundledLaunch>`
  (`{ args, pending_boot_extraction: Option<ExtractOptions> }`) instead of
  `Option<Vec<String>>` — the caller decides when/how to actually run the still-pending
  extraction instead of it happening inline.
- `cli.rs`: destructures `BundledLaunch` and threads `pending_boot_extraction` through to a
  new `App::new` parameter, unresolved.
- `event_loop.rs`: two new `AppEvent` variants, `BootProgress(f32)` and `BootReady`, sent by
  the background extraction thread `App::init` spawns (see below) — mirrors the existing
  `Arc<Mutex<EventLoopProxy<AppEvent>>>` background-thread-to-main-thread pattern already used
  by `HeadedEventLoopWaker` and `protocols/roves.rs`'s `RovesProtocolHandler`, simplified to a
  single owned `EventLoopProxy` clone (no sharing needed for one thread).
- `tracing.rs`: its `LogTarget` impl for `winit::event::Event<AppEvent>` pattern-matches every
  `AppEvent` variant by name (for the `RUST_LOG='servoshell<winit@...'` trace filters this
  file documents) — adding the two variants above without a matching arm here is a compile
  error (non-exhaustive match), not just a missed log line. Added
  `UserEvent(BootProgress)`/`UserEvent(BootReady)` targets alongside the existing ones.
- `app.rs`: new `AppState::Booting { window, extraction_started, progress }` variant. `App`
  gained a `pending_extraction` field (the `ExtractOptions` from `cli.rs`, consumed by the
  first `init` call). `init` now always creates the platform window immediately (window
  creation never depended on the boot URL being ready — `HeadedWindow::new`/`Gui::new` don't
  touch it beyond an already-dead `_initial_url` parameter), then: with no pending extraction,
  proceeds exactly as before (moved into a new `finish_init` helper); headless with a pending
  extraction, runs `extract_boot` synchronously as before (no splash to show); headed with a
  pending extraction, spawns a background thread running `extract_boot_with_progress` (sending
  `BootProgress`/`BootReady` back), enters `Booting`, and defers `finish_init` (building the
  protocol registry, Servo, `RunningAppState`, and opening the real webview) until
  `AppEvent::BootReady` arrives. A new `new_events` hook forces one redraw when the splash's
  progress-bar delay elapses even without a fresh `BootProgress` tick. `window_event`/
  `user_event` both gained a `Booting` branch (paint the splash / update progress and hand off
  to `finish_init`) ahead of the existing `Running`-only logic.

  One correctness-critical ordering detail: `finish_init` (not `init`) is what constructs
  `protocols::file::FileProtocolHandler`, which looks for the `.roves-content-source` marker
  extraction writes — moving *all* of protocol-registry setup into `finish_init` (not just the
  Servo/webview part) means that marker always exists by the time the handler is built,
  whether extraction ran synchronously (headless) or finished on the background thread first
  (headed, `BootReady` fires only after `extract_boot_with_progress` returns). Building the
  handler any earlier would silently disable on-demand lazy extraction for the entire session.

  A second, easy-to-miss instance of the same ordering hazard: `self.initial_url` — previously
  computed once, eagerly, in `App::new` (via `get_default_url`, which only trusts a `file:`
  URL derived from a path argument if `fs::metadata` confirms that path *already exists*,
  `parser.rs`) — is now computed in `finish_init` instead, *after* any pending extraction has
  actually finished. Leaving it in `App::new` (as originally written) reintroduces exactly the
  bug `parser.rs`'s own "Accept absolute Windows paths" fix (see that entry above) was written
  to prevent, just one layer up: on a genuine first launch (empty cache), `App::new` runs
  *before* extraction has happened, so the boot html's absolute path doesn't exist on disk
  yet — `get_default_url` doesn't trust it, falls through to parsing the raw path as a URL
  directly, and on Windows that misparses the drive letter as the scheme. Net effect: first
  launch after a cache wipe shows "Could not load the requested page: Unsupported scheme";
  closing and relaunching works, because by then extraction has already happened once and the
  file genuinely exists. `create_platform_window`'s `url` argument (still computed early, in
  `init`) is unaffected by any of this — it's the same already-dead parameter `Gui::new` never
  uses, so a placeholder (`"about:blank"`, `App::new`'s initial value for the field) is fine
  there regardless of when the real URL is known.
- `gui.rs`: new `Gui::update_splash(winit_window, progress: Option<f32>)`, painted via the
  existing `EguiGlow`/`Gui::paint` pipeline `Gui::update` already uses — a black
  `CentralPanel`, the boot icon (decoded once in `Gui::new` from `resources/servo_64.png` via
  `image::load_from_memory`, independent of `headed_window.rs`'s Linux/Windows-only
  `load_icon`, since this must also work on macOS), "Roves" in white, and, when `progress` is
  `Some`, an `egui::ProgressBar`. Uses `egui::CentralPanel`'s deprecated top-level `show`
  (`#[expect(deprecated)]` — the non-deprecated replacement is hand-building a full-window
  `Ui` via egui's own internal-ish `Ui::new`/`UiBuilder` dance, not worth the extra surface
  for this deliberately simple splash) and clones the icon `Image` before handing it to
  `ui.add` — it's captured by the outer `EguiGlow::run` closure (which must be `FnMut`, since
  `run` can conceptually be invoked more than once), so moving it into the inner `show`
  closure without cloning is a compile error (`Image` isn't `Copy`, but is cheap to `Clone`
  — it just wraps a texture id/size, not pixel data).
- `headed_window.rs`: new `paint_splash(progress: Option<f32>)`, calling the above plus the
  existing `Gui::paint`.

**Why:** asked directly — a blank window (or nothing at all) during first-run extraction reads
as a hang, especially on a slow disk with a large boot set. Reusing the existing
`Gui`/`EguiGlow`/window machinery (rather than a second winit window/event loop, which winit
doesn't reliably support creating more than one of per process) kept this to one native window
throughout, with extraction genuinely running concurrently instead of blocking startup.

**Side effects to know about when upgrading:** the boot splash's icon is always
`resources/servo_64.png`, deliberately never the game-supplied icon from the entry below —
the splash is explicitly Roves' own branding moment, not the game's. `finish_init`'s
`active_event_loop` parameter is unused when compiled without the `webxr` feature (only
consulted inside a `#[cfg(feature = "webxr")]` block) — a pre-existing kind of harmless
`unused variable` warning this codebase already tolerates elsewhere (see the
toolbar-removal entry above).

**Verification:** `cargo test --manifest-path support/content-packer/Cargo.toml` — all 6
pre-existing `roundtrip.rs` cases plus the `size` unit test still pass unchanged, confirming
the `extract_boot`/`ExtractOptions` signature is untouched for every existing call site.
Every new/changed `egui`/`winit` API call (`egui::Image::from_texture`, `ColorImage::
from_rgba_unmultiplied`, `TextureHandle`/`SizedTexture`, `ProgressBar`, `CentralPanel`/`Frame`,
`ApplicationHandler::new_events`/`StartCause::ResumeTimeReached`) was checked directly against
this workspace's pinned versions' source (`egui`/`egui-winit` 0.34.3, `winit` 0.30.13) rather
than assumed. **`servoshell` itself could not be fully compiled in this environment** —
`cargo check -p servoshell` got past the usual `libclang`/`bindgen` gap (worked around with
`LIBCLANG_PATH` pointed at an Android NDK's `libclang.so`) and compiled several hundred
dependencies including `egui` 0.34.3 itself with no errors, but then hit an unrelated, sandbox
-specific wall: `libudev-sys`'s build script hard-requires a system `libudev.pc` via
`pkg-config`, which isn't installed here and can't be (no `sudo`) — this is a system-library
gap, not a code issue, and reproduces identically even with the `gamepad` feature disabled (
something else in the dependency graph also pulls it in). **Needs a real `./mach build`/
`./mach run` on a machine with a full toolchain to confirm end-to-end** — window appears
immediately, splash shows icon+text with no delay, progress bar appears only after the delay
and reflects real extraction progress, then the real page loads.

---

## 2026-08-09 — Game-supplied icon (with Roves fallback) for the window/taskbar/exe

**Files:** `ports/servoshell/build.rs`, `ports/servoshell/desktop/headed_window.rs`,
`.gitattributes`. Also `test-page/index.html` and two new fixtures,
`test-page/public/icon.png`/`icon.ico` — not part of the patch (same as `test-page/public/`'s
other fixtures, see the "Pack game content" entry above: plain test assets, not derived from
any upstream file).

**Patch:** `patches/servo-v0.4.0/0019-game-supplied-icon-fallback.patch`

**Upstream behavior:** the window/taskbar icon (`headed_window.rs`, Linux/Windows only) and
the compiled `.exe`'s own icon resource (`build.rs`, Windows only) were both unconditionally
`resources/servo_64.png`/`servo.ico` — Servo's own branding, not any particular game's.
Separately, `test-page/`'s `index.html` never referenced its own already-present (but unused)
`public/favicon.svg` placeholder, so a normal browser tab showing it had no icon at all.

**Change:** `test-page/index.html` now links `favicon.svg` like a normal website would. Two
new raster fixtures next to it, `test-page/public/icon.png` (window/taskbar) and `icon.ico`
(Windows exe resource, multi-size 16-256px), redraw `favicon.svg`'s same placeholder design
(solid `#6c5ce7` square, centered white "R") as bitmaps, since nothing in this sandbox can
rasterize SVG and the native side needs a raster format either way. `build.rs` now copies
whichever of `test-page/public/icon.png` or `resources/servo_64.png` exists into
`$OUT_DIR/window_icon.png` (with `cargo:rerun-if-changed` on both), and — on Windows — prefers
`test-page/public/icon.ico` over `resources/servo.ico` for `WindowsResource::set_icon`.
`headed_window.rs`'s window-icon `include_bytes!` now reads `$OUT_DIR/window_icon.png` instead
of the hardcoded `resources/servo_64.png` path. Net effect: once a real game supplies its own
`icon.png`/`icon.ico` in `test-page/public/`, the window, taskbar, and compiled executable all
show it automatically; absent that, everything falls back to today's Roves-branded assets
exactly as before. `.gitattributes` gained `*.ico binary` alongside the pre-existing
`*.png`/`*.jpg` rules — `icon.ico` is this repo's first tracked `.ico` file, and without an
explicit rule it falls under the blanket `* text=auto eol=lf` at the top of that file, which
lets Git's own (content-sniffing) heuristic decide whether to normalize line endings in it;
for a small binary file that's a real risk of silent corruption on a Windows checkout
(`core.autocrlf`), not just a theoretical one.

**Deliberately out of scope:** the boot splash's icon (see the entry above) is exempt from
this fallback on purpose — always Roves-branded, regardless of what the game supplies. Linux's
`.desktop` `Icon=` and macOS's `.icns`/`Info.plist` aren't wired to this fallback either —
different formats/tooling (an installed XDG icon-theme entry, `iconutil`) not worth the extra
surface in this pass; noted here as a follow-up, mirroring how the "Rename Servo to Roves"
entry above documents its own similarly-deferred branding work.

**Why:** asked directly — once this engine ships more than one game, a game should look like
itself (window/taskbar/exe icon) rather than the shell it happens to run on, without needing
to be told; falling back to Roves' own icon keeps today's behavior for any game that hasn't
supplied one yet.

**Side effects to know about when upgrading:** `build.rs` now does filesystem I/O
(`std::fs::copy`) unconditionally on every target, not just Windows — cheap, but note it if
ever auditing `build.rs` for why it touches paths outside its own crate.

**Verification:** same environment limitation as the entry above — `cargo check -p
servoshell` could not fully complete here (unrelated `libudev-sys`/`pkg-config` gap), so the
new `build.rs` logic and `headed_window.rs`'s `include_bytes!(concat!(env!("OUT_DIR"), ...))`
change weren't compiler-verified end-to-end. `icon.ico`'s multi-size embedding (16/32/48/64/
128/256) was confirmed with Pillow (`Image.open(...).info["sizes"]`) after generation. **Needs
a real `./mach build` on Windows to confirm the `.exe`'s Explorer icon and the window/taskbar
icon both actually change** when `test-page/public/icon.{png,ico}` are present, and fall back
correctly when they're removed.

**Follow-up (2026-08-10) — fixed a real bug this "Needs a real build" note caught:** tested via
`.github/workflows/test.yml`, which — unlike a normal in-place build — reconstructs Servo one
directory level away from this repo's real layout: it downloads+patches pristine Servo into a
nested `servo-src/` subdirectory, so `ports/servoshell/build.rs` ends up running from
`servo-src/ports/servoshell/`, not `<repo root>/ports/servoshell/`. Its `../../test-page/...`
paths (correct for this repo's real, flat layout — verified directly by hand from
`ports/servoshell/`) resolved to `servo-src/test-page/...` instead of the real
`<repo root>/test-page/...`, which doesn't exist — so `game_window_icon.exists()`/
`game_exe_icon.exists()` were always `false` in that workflow specifically, always silently
falling back to Roves' own icon. (The later `assemble test bundle` step's own `--content-dir
../test-page/dist` happened to still work, since that step runs one directory shallower,
directly inside `servo-src`, not from `servo-src/ports/servoshell`.) Root cause: `test-page/`
was never part of `servo-src/` at all — the patch-apply loop only recreates *patch-tracked*
files (see this entry's own "Files" note on `test-page/` being outside the patch set), so
nothing ever put a copy of it there. **Fixed in `test.yml` itself** (not `build.rs`, whose
paths are correct for how every real build actually lays out): the "download + patch Servo
source" step now also copies `test-page/public/` and the already-built `test-page/dist/` into
`servo-src/test-page/`, so that tree actually mirrors this repo's real layout the way patches
already assume; "assemble test bundle"'s `--content-dir` updated from `../test-page/dist` to
`test-page/dist` to match. Confirmed by re-tracing both steps' working directories and the
resulting relative paths by hand; not yet confirmed by an actual CI run.

**Second follow-up (2026-08-10) — zip a named subfolder, not `release/`'s contents at the
archive root:** unrelated to the icon bug above, but caught while re-checking the same
workflow — both "zip bundle" steps (`test.yml`) zipped `release/`'s *contents* directly
(`zip -r "../$ZIP" .` from inside `release/`; `Compress-Archive -Path release/*`), so
extracting the downloaded archive dumped `play`/`play.exe` plus 8+ loose DLLs/support files
straight into whatever folder you extracted into (e.g. `Downloads/`), not into one folder of
their own. Both steps now `mv`/`Rename-Item` `release` to `servo-test-page` first, then zip
*that folder* (`zip -r "$ZIP" servo-test-page`; `Compress-Archive -Path servo-test-page`) —
`Compress-Archive`/`zip -r` both preserve the given folder itself as the archive's one
top-level entry when the path doesn't end in a wildcard, which is what makes this work.
`servo-test-page` names it after this workflow's content (`test-page/`); a real bundle would
use the actual game's name instead. Not yet confirmed by an actual CI run.

---

## 2026-08-10 — Never show white before the game starts: default clear color + paint-before-show

**Files:** `components/config/prefs.rs`, `ports/servoshell/desktop/gui.rs`.

**Patch:** `patches/servo-v0.4.0/0020-never-show-white-on-startup.patch`

**Upstream behavior:** two independent gaps could still show a white (or otherwise
undefined) frame before the boot splash entry above's coverage kicks in, or after it hands
off to the real page:

1. `Preferences::const_default()`'s `shell_background_color_rgba` — the `glClearColor` used
   for *any* `WebView` that hasn't painted anything of its own yet, per `components/paint/
   painter.rs`'s own comment ("clear the entire RenderingContext... so WebView actually
   clears even before the first WebView is ready") — defaulted to opaque white
   (`[1.0, 1.0, 1.0, 1.0]`). This is the classic "blank white tab" every browser has, and it's
   exactly what showed through during the gap between the boot splash ending
   (`AppEvent::BootReady`) and the real page's first paint — plus, independently, on *any*
   launch with no pending boot extraction at all (a plain dev `--url` run, or a
   `--content-compress=none` build), which never enters `AppState::Booting` in the first
   place and so never got the boot-splash entry's coverage to begin with.
2. `Gui::new` (`gui.rs`) called `winit_window.set_visible(true)` without ever painting
   anything first — the window became visible with whatever undefined content its GL
   surface happened to have (surfman/the driver don't clear it on creation), for however long
   until winit delivered the first `RedrawRequested`. On some platforms/drivers that's a
   visible flash of garbage or white, not black.

**Change:**

- `shell_background_color_rgba` now defaults to opaque black (`[0.0, 0.0, 0.0, 1.0]`).
  Affects only the "nothing painted yet" state — once a page actually paints (including the
  game's own page, whatever its CSS background is), that content fully covers this color, so
  there's no visible effect once loading is done.
- `Gui::new` now calls `update_splash(winit_window, None)` + `paint(winit_window)` — the same
  boot-splash black screen the entry above added — *before* `set_visible(true)`, on every
  code path, not just a packed-content boot extraction. This guarantees the very first thing
  the OS ever displays for the window is the black splash frame, regardless of whether
  `AppState::Booting` is ever entered at all.

Combined, every startup path now goes: black splash frame (painted before the window is even
visible) → real page, whose own "not yet painted" background is now black instead of white →
the page's actual content. No code path shows white unless the game's own page explicitly
paints something white itself.

**Why:** asked directly — a white flash during startup on a black boot splash reads as
broken/janky regardless of how brief; fixing the *rendering-level* default (the clear color
every WebView uses before it has content) is far more robust than trying to synchronize
against the page's own load lifecycle from the embedder side (there's no first-paint signal
exposed to `ports/servoshell` today — the closest, `LoadStatus::Complete`
(`notify_load_status_changed`, `running_app_state.rs:783`), is DOM-readiness, not
"compositor has presented a frame" — see `components/metrics/lib.rs`'s `FirstPaint`, which
terminates inside the constellation for the Performance API and isn't forwarded to the
embedder at all). Changing the shared default clear color sidesteps needing that signal.

**Side effects to know about when upgrading:** `components/servo/tests/
performance_paint_timing.rs` already overrides this same pref for its own tests (to
gray/blue) — unaffected by this default change, but worth knowing it's a precedent for
per-test overrides if a future test relies on the *default* being white.

**Verification:** not compiled end-to-end in this environment (same `libudev-sys`/pkg-config
gap as the boot-splash entry above). `shell_background_color_rgba`'s new default and
`Gui::new`'s reordering were both read back against the surrounding code to confirm no other
call site assumes white (`components/paint/painter.rs`'s own doc comment for the pref, and
`Gui::new`'s existing accesskit-before-visible ordering, which this change preserves —
painting happens after accesskit init, still before `set_visible`).

---

## 2026-08-10 — Persist fullscreen state across launches

**Files:** `ports/servoshell/prefs.rs`, `ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0021-persist-fullscreen-across-launches.patch`

**Upstream behavior:** `HeadedWindow`'s `fullscreen: Cell<bool>` (and the actual OS-level
fullscreen state it tracks) was purely in-memory — always started `false`. A game closed
while its page was in fullscreen (via the Fullscreen API — `requestFullscreen()`/
`exitFullscreen()`, routed through `RunningAppState::notify_fullscreen_state_changed` →
`PlatformWindow::set_fullscreen`, the only call site that ever changes this state; there's no
native F11-style shortcut) always reopened windowed, then had to be told to go fullscreen
again by the page itself.

**Change:**

- `ServoShellPreferences` gained two fields: `start_fullscreen: bool` and `config_dir:
  Option<PathBuf>` (the latter just keeps the already-resolved `config_dir` local in
  `parse_command_line_arguments` around for `HeadedWindow` to reuse, instead of re-deriving
  it). `start_fullscreen` is read once at startup: `config_dir.join("fullscreen").exists()`.
- `HeadedWindow::new` (`headed_window.rs`) requests the window *already* fullscreen at
  creation time (`WindowAttributes::with_fullscreen`, resolving a monitor via
  `ActiveEventLoop::primary_monitor`/`available_monitors` — the window doesn't exist yet, so
  `winit_window.current_monitor()` isn't available the way `set_fullscreen` uses it) when
  `start_fullscreen` is set, and seeds `fullscreen: Cell::new(start_fullscreen)` to match —
  avoids a windowed-then-fullscreen transition on startup (and keeps that visible moment,
  whatever it is, off-white too, per the entry above).
- `set_fullscreen` (`headed_window.rs`) now calls a new `persist_fullscreen_state(config_dir,
  state)` helper whenever the state actually changes — writes an empty marker file named
  `fullscreen` under `config_dir` on entering fullscreen, removes it on leaving. Deliberately
  a marker file's mere existence, not JSON, matching `support/content-packer/src/
  extract.rs`'s own marker-file convention for simple booleans. `config_dir` being `None`
  (couldn't be resolved) just skips persistence rather than failing.

**Why:** asked directly — closing in fullscreen and reopening windowed is a jarring,
unexpected transition; a game's window state should survive a relaunch the way a player
left it.

**Side effects to know about when upgrading:** the marker lives directly under `config_dir`
(e.g. `~/.config/servo/default/fullscreen` on Linux by default) — the same directory
`prefs.json` lives in (see `get_preferences`). If a future change starts wiping/migrating
that directory's contents wholesale, this marker would be swept up too; worth a second
thought if that ever happens.

**Verification:** not compiled end-to-end in this environment (same `libudev-sys`/pkg-config
gap noted above). Confirmed `set_fullscreen` (`headed_window.rs:937-953`) is genuinely the
*only* place fullscreen state changes (no native keyboard shortcut, only the page's own
Fullscreen API via `running_app_state.rs`), so persisting there is complete, not partial
coverage. **Needs a real build to confirm end-to-end**: enter fullscreen via a page's
`requestFullscreen()`, close the app, relaunch, confirm it reopens fullscreen with no
windowed flash; then exit fullscreen and relaunch again to confirm it reopens windowed.

---

## 2026-08-10 — Fix: Windows taskbar/Alt-Tab icon not actually using the custom icon

**Files:** `ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0022-fix-windows-taskbar-icon.patch`

**Upstream behavior:** winit 0.30's cross-platform `Window::set_window_icon` only sets
`WM_SETICON`'s `ICON_SMALL` — the title bar icon. The taskbar/Alt-Tab icon is `ICON_BIG`, set
by a *separate*, Windows-only call (`WindowExtWindows::set_taskbar_icon`, in
`winit::platform::windows`) — `ports/servoshell` only ever called the former. Confirmed by a
real build: the title bar showed no icon at all, while the taskbar happened to show the
*right* icon anyway — Windows falls back to the `.exe`'s own embedded resource icon
(`build.rs`'s `winresource` step, itself already reading the game-supplied `icon.ico` — see
that entry above) for `ICON_BIG` when nothing has explicitly set it, which is why this went
unnoticed until specifically checking the title bar.

**Change:** `HeadedWindow::new` now also calls `winit_window.set_taskbar_icon(Some(icon))` (a
clone of the same `Icon` passed to `set_window_icon`), gated `#[cfg(target_os = "windows")]`
(the extension trait doesn't exist on other platforms — Linux/macOS have no small/big icon
split). Both calls now always agree, instead of one coming from the runtime icon and the
other happening to come from the `.exe` resource fallback.

**Why:** a real build showed the title bar icon missing entirely, which is what surfaced this
— the taskbar looking right was accidental (a different icon source entirely), not evidence
the runtime icon-setting code was correct.

**Side effects to know about when upgrading:** if a future winit version merges `ICON_SMALL`/
`ICON_BIG` back into one call (or renames `set_taskbar_icon`), this two-call pattern may
become redundant or need updating — check `winit::platform::windows::WindowExtWindows`'s docs
for the version in use.

**Verification:** not compiled end-to-end in this environment (same `libudev-sys`/pkg-config
gap as other entries above); `WindowExtWindows::set_taskbar_icon`'s signature and its
`ICON_BIG`/`ICON_SMALL` split were confirmed directly against winit 0.30.13's own source
(`platform_impl/windows/window.rs`, `platform/windows.rs`) in this workspace's registry cache,
not assumed. **Confirmed real bug** (title bar icon missing) via an actual Windows build/run;
the fix itself needs a further real build to confirm the title bar icon now actually appears.

---

## 2026-08-10 — Window title from the game's own `manifest.json`/`package.json` name

**Files:** `python/servo/post_build_commands.py`, `ports/servoshell/prefs.rs`,
`ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0023-window-title-from-manifest-or-package-json.patch`

**Upstream behavior:** the native window title always mirrored the active page's own
`document.title` (`HeadedWindow::update_user_interface_state`, falling back to the URL, then
to a hardcoded `"Roves"` if there's no webview at all) — there was no way to give a shipped
game a fixed, native-feeling title independent of whatever its page's `<title>` happens to
say (in `test-page`'s case, `<title>Servo test build</title>` — an internal diagnostic label,
not a real product name).

**Change:** scoped to `mach bundle` only (a plain `./mach run`/dev launch is unaffected —
deliberate, see "Why" below):

- `post_build_commands.py`'s new `_resolve_window_title(content_dir)` reads
  `<content_dir>/manifest.json`'s `name` field (a standard web-app-manifest field, and — since
  a bundler copies `public/` into the build output root — actually present inside
  `content_dir`, e.g. `dist/`, once built) or, failing that, `<content_dir>/../package.json`'s
  `name` (the common Vite/webpack layout: `package.json` next to the project, `content_dir`
  its built `dist/` one level below — a source file, so only available here, at bundle time,
  never inside the shipped `content_dir` itself). Used *verbatim* as the window title — this
  doesn't prepend "Roves" or reformat it at all; that's the content author's own call (e.g.
  `test-page/public/manifest.json` already had `"name": "Roves test-page"` from an unrelated
  earlier commit, which is exactly why that specific string was expected here). `bundle()`
  appends `["--window-title", <name>]` to `launch.json`'s `args` when a name was found;
  nothing changes when neither file has one.
- `prefs.rs`: new `--window-title TEXT` CLI flag → `ServoShellPreferences.window_title_override:
  Option<String>`.
- `headed_window.rs`: new `HeadedWindow.window_title_override` field (cloned from the
  preference at construction). `update_user_interface_state` now uses it as a fixed title when
  set, instead of ever computing one from the active webview's page title/URL — set once,
  never changed afterward, even if the page's own `document.title` changes later.

**Why:** asked directly, scoped to packaged builds only since `manifest.json`/`package.json`
naming a real product only makes sense for an actual shipped game — a dev run (`./mach run
some/path/index.html`) has no natural "content_dir" to read a manifest from in the first
place, and showing the raw page title there (as today) is arguably more useful for debugging
anyway.

**Side effects to know about when upgrading:** if `content_dir`'s bundler doesn't copy
`public/`-style files into the build root the way Vite does by default, `manifest.json` won't
be found there and this silently falls through to the `package.json` candidate (or to no
override at all) — not a bug, just worth knowing the first candidate path assumes that
convention.

**Verification:** `python3 -m py_compile python/servo/post_build_commands.py` — clean.
Confirmed `test-page/public/manifest.json` (`name: "Roves test-page"`) actually ends up at
`test-page/dist/manifest.json` after `npm run build`, since `test-page/vite.config.ts` doesn't
override Vite's default `publicDir`. Not compiled end-to-end on the Rust side in this
environment (same `libudev-sys` gap as other entries above). **Needs a real `mach bundle` +
run to confirm end-to-end**: title bar should read exactly `Roves test-page` for this repo's
own test bundle.

---

## 2026-08-11 — `roves:clear_content_cache` command, so a game can wipe its own extraction cache

**Files:** `ports/servoshell/desktop/protocols/roves.rs`, `ports/servoshell/desktop/app.rs`,
`support/content-packer/src/extract.rs`. Plus, outside this `servo/` directory (not
patch-tracked — see the "roves: protocol bridge" entry above for why): new module
`roves-api/src/cache.ts` (and its `roves-api/tsup.config.ts` entry point), and a new
"Clear extraction cache" button in `test-page` (`test-page/src/ClearCacheButton.tsx`, wired
into `App.tsx`).

**Patch:** `patches/servo-v0.4.0/0024-clear-content-cache-command.patch`

**Upstream behavior:** no equivalent — extends the `roves:` bridge (see the 2026-08-06 entry
above) with a second command, not a modification of existing upstream logic.

**Change:** a game's packed content (see the "Pack game content into compressed archives"
and "boot set" entries above) gets decompressed on first launch into a per-install directory
under the OS cache dir, and stays there — nothing ever clears it automatically. Asked for a
way for the game itself to force a fresh re-extraction (e.g. after shipping a content update
under an unexpectedly unchanged `content_hash`, or just for a support/troubleshooting reset),
*without* touching actual save data, which lives elsewhere entirely and this command never
touches.

- `support/content-packer/src/extract.rs`: new `is_managed_cache_dir(dir)` (checks for the
  `.roves-content-source` marker `prepare_dest` always writes) and `clear_cache(dir)` (refuses,
  rather than deletes, if that marker is missing, then `fs::remove_dir_all`s the whole
  directory). The marker check matters here specifically because this is a generic "delete
  this directory" operation reachable from web content — it must not be possible to point it
  at some unrelated path and have it delete that instead.
- `app.rs`: computes `content_cache_dir` the exact same way `FileProtocolHandler::new` derives
  its own `cache_dir` (the initial `file:` URL's parent directory) and passes it to
  `RovesProtocolHandler::new` alongside the existing `close_proxy`.
- `roves.rs`: new `"clear_content_cache"` match arm — calls `extract::clear_cache`, and, only
  if that succeeds, closes every window the same way `exit` does (factored the `exit` arm's
  window-closing logic out into a shared `close_all_windows` helper both arms now call). `None`
  `content_cache_dir` (a plain dev `--url` launch, not a packed-content one) answers with an
  error instead of doing nothing silently.
- `roves-api/src/cache.ts`: `clearContentCache()`, a thin `invoke("clear_content_cache")`
  wrapper, new `cache` entry in `tsup.config.ts` (mirrors `process`/`steam`'s existing pattern).
- `test-page/src/ClearCacheButton.tsx`: same shape as the existing quit button — a
  `window.confirm()`-guarded destructive action — dropped into the button row next to
  `IndexedDbButton`/the quit button in `App.tsx`.

**Why closing the window is not optional:** the destination directory this clears is the
*live* document root while the game is running (`FileProtocolHandler` serves the current
page and every future on-demand pack extraction out of it — see the "lazy on-demand content
extraction" entry above). Deleting it out from under a still-running page would silently break
any asset not yet extracted this session. A relaunch-instead-of-close option was considered
(and would need new code — nothing in this codebase currently spawns/replaces its own
process) but deliberately left out of this first version to keep the change small; closing and
letting the player start the game again is the safe default.

**Verification:** `cargo check -p roves-content-packer` and `cargo check -p servoshell` both
pass. `roves-api`'s `npm run build` (tsup) produces `dist/cache.{mjs,cjs,d.ts}` correctly.
`test-page`'s `tsc && vite build` verified against a locally `npm pack`-built copy of
`@drincs/roves-api` (the npm-published `0.1.0` doesn't have the `cache` module yet — see
below). Not run end-to-end against a real native build in this environment (same
`libudev-sys` gap as other entries above).

**Follow-up needed before this is actually usable from `test-page`:** `test-page/package.json`
depends on the *published* npm package, not the local workspace source (see the 2026-08-06
entry's "Outside `servo/`" note) — bumped here to `"@drincs/roves-api": "^0.2.0"` to match
`roves-api/package.json`'s version bump (`0.1.0` → `0.2.0`, done alongside this change, both
plain local edits). But the npm registry itself still only has `0.1.0` (`core`/`process`/`steam`
only, no `cache`) until someone actually publishes `0.2.0` — push a `v0.2.0`-style tag (see
`roves-api/.github/workflows/npm-publish.yml`) to trigger that. Until then, `test-page`'s own
`npm install`/`tsc` (including `.github/workflows/test.yml`'s CI) will fail to resolve
`@drincs/roves-api/cache`. Deliberately left un-triggered here since pushing a release tag is
a real publish action, not a local code change.

---

## 2026-08-12 — Hold the boot splash for a minimum duration on every launch

**File:** `ports/servoshell/desktop/app.rs`.

**Patch:** `patches/servo-v0.4.0/0025-hold-boot-splash-minimum-duration.patch`

**Upstream behavior:** no equivalent — refines the boot-splash entry above (2026-08-09) and
the never-show-white entry (2026-08-10).

**Problem:** asked directly — on startup, a brief black screen showed before the real page,
instead of the branded (icon + "Roves") splash. Root cause: `AppState::Booting` (the branded
splash's only code path with any actual visible duration) was gated entirely on there being a
pending packed-content boot extraction (`App::pending_extraction.is_some()`). A launch with
nothing to extract — a dev `--url` run, or (the common case after the very first launch) a
packed-content launch whose destination is already cached from a previous run — skipped
`Booting` entirely. The window's very first frame *is* the branded splash (`Gui::new` already
painted it, unconditionally, per the 2026-08-10 entry), but the very next frame immediately
swapped in the real, still-loading `WebView` — whose own clear color is black too (that same
entry's `shell_background_color_rgba` default) until it has content to paint. Net effect: the
branded splash flashed for a single frame, too brief to register — read by a user as "black
screen, then the app", not as a splash.

**Change:** new `MIN_SPLASH_DURATION` (500ms). Every headed launch now always enters
`AppState::Booting` and stays there for at least this long, regardless of whether there's a
pending extraction — if there is one, it still also has to finish (unchanged); if there isn't,
`MIN_SPLASH_DURATION` alone is what `finish_init` waits on.

- `AppState::Booting` gained a new field, `extraction_done: bool` — `true` from the start if
  `App::init` had no `pending_extraction` to begin with (nothing left to wait on but the
  timer), or flips to `true` once `AppEvent::BootReady` arrives (unchanged trigger, same as
  before this change).
- `App::init`: no longer branches on `self.pending_extraction` to decide *whether* to enter
  `Booting` for a headed launch — it always does now (headless is unaffected: no splash to
  show either way, so a pending extraction there still just runs synchronously, exactly as
  before). Only branches on it now to decide whether to also spawn the background extraction
  thread.
- New `App::try_finish_booting(event_loop)`: the one place that decides whether `Booting` is
  actually done — `extraction_done && extraction_started.elapsed() >= MIN_SPLASH_DURATION` —
  and, if not, re-arms `ControlFlow::WaitUntil` for whichever of `SPLASH_PROGRESS_BAR_DELAY`/
  `MIN_SPLASH_DURATION` hasn't elapsed yet. A no-op when not `Booting`, so every event handler
  can call it unconditionally instead of duplicating this decision. Replaces the old, simpler
  "just call `finish_init` directly from the `BootReady` handler" — that alone is no longer
  sufficient, since extraction finishing early (or not existing at all) must *not* skip the
  remaining minimum-duration wait.
- `new_events`/`window_event`/`user_event`'s `Booting` branches: still do their own thing
  first (force a progress-bar-delay redraw; paint the splash; update `progress`/
  `extraction_done`), then all defer to `try_finish_booting` instead of each having its own
  copy of the finish-or-reschedule logic (`window_event`'s old inline version) or directly
  calling `finish_init` (`user_event`'s old `BootReady` arm).

**Why 500ms specifically:** long enough to reliably register as "a splash appeared" (vs. a
single-frame flash, which is what the bug was), short enough that it doesn't read as an
artificial delay on a fast dev relaunch. Not derived from any measurement — a reasonable
starting point, adjustable if it feels off in practice.

**Verification:** `cargo check -p roves-content-packer` unaffected (this entry doesn't touch
that crate). `cargo check -p servoshell` could not be completed in this environment — same
`libudev-sys`/pkg-config gap as prior entries, this time hit before rustc ever reached
`servoshell`'s own source (a dependency lower in the graph fails first) — so this change was
*not* type-checked by rustc here. Reviewed by hand instead: every borrow-splitting pattern used
(cloning a `Rc<dyn PlatformWindow>` out of an `&mut self.state` match arm before calling back
into `&mut self`; ending an immutable `if let ... = &self.state` borrow at its last use before
a subsequent `&mut self` call) mirrors a pattern already present and compiling elsewhere in
this same file (e.g. the pre-existing `user_event`'s `boot_ready_window` extraction). **Needs a
real `./mach build`/`./mach run` to confirm end-to-end** — the splash should now visibly hold
for ~500ms on every launch, including a plain `./mach run --url` dev launch and a relaunch of
an already-extracted packed build, not just a genuine first-launch extraction.

---

## 2026-08-12 — Name the extraction cache directory after the game

**Files:** `support/content-packer/src/manifest.rs`, `support/content-packer/src/pack.rs`,
`support/content-packer/src/extract.rs`, `support/content-packer/src/main.rs`,
`support/content-packer/tests/roundtrip.rs`, `ports/servoshell/desktop/bundle_launch.rs`,
`python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0026-name-extraction-cache-dir-after-the-game.patch`

**Upstream behavior:** no equivalent — refines `extract::default_dest` (2026-08-09's
"lazy on-demand content extraction" entry, further along above).

**Problem:** asked directly — the on-disk extraction cache directory (see the appdata-cache
entries above) was a bare `<cache_dir>/roves-content-<hash8>/`, opaque and indistinguishable
from any other game's cache dir on the same machine at a glance.

**Change:** `<cache_dir>/<game_name>/cache/<hash8>/` — a top-level folder named after the game,
with the actual extracted content nested inside a `cache/<hash8>` subfolder (the hash — of the
resolved `content_dir` path, unchanged in meaning from before — is still what actually keeps
repeat launches of the *same* install pointed at the same destination while different
installs/games that happen to share a display name don't collide).

- `manifest.rs`: `Manifest` gained `name: Option<String>` (`#[serde(default)]`, so an older
  manifest without this key still deserializes) — the game's display name, written verbatim
  by `pack`. `None` for a dev/uncompressed build or an older manifest.
- `pack.rs`: `PackOptions` gained a matching `name: Option<String>`, threaded straight into the
  `Manifest` it writes.
- `main.rs`: new `--name NAME` (optional) on the `pack` subcommand, wired to
  `PackOptions::name`. `extract` is untouched — it never needs the name passed explicitly, see
  below.
- `extract.rs`: `default_dest` takes a new `game_name: Option<&str>` parameter and builds the
  new nested path shape; falls back to the literal string `"roves"` if `game_name` is `None` or
  sanitizes to nothing. New `sanitize_path_segment(name)` turns arbitrary manifest text into a
  single filesystem-safe path segment — strips path separators/Windows-reserved characters
  (`\/:*?"<>|`) and control characters to `-`, trims leading/trailing whitespace and `.`
  (Windows disallows a trailing dot/space; a lone leading dot reads as hidden on Unix), and
  caps length at 64 chars; returns `None` only if nothing survives (e.g. empty, or all
  whitespace/dots), in which case the caller falls back to `"roves"` — a result of all-`-`
  characters (e.g. sanitizing `"/\\\":"`) is still `Some`, since that's a valid, if unhelpful,
  directory name, not "nothing usable". `resolve_dest` gained the same new `game_name`
  parameter, forwarded straight to `default_dest`. `prepare_dest` now loads the manifest
  *before* calling `resolve_dest` (previously after) so `Manifest::name` is available in time —
  the same `content_dir.join("manifest.json")` read either way, just reordered; canonicalizing
  `content_dir` first was never a real dependency of that read. Added unit tests for
  `sanitize_path_segment` and `default_dest`'s new path shape (`#[cfg(test)] mod tests`,
  matching `size.rs`'s existing convention).
- `bundle_launch.rs`: `resolve_packed_content_url` already loaded the manifest before calling
  `resolve_dest` (unlike `prepare_dest`, no reordering needed here) — just passes
  `manifest.name.as_deref()` through now.
- `post_build_commands.py`: `_place_bundle_content` gained a `game_name: Optional[str]`
  parameter, passed as `--name` to the packer subprocess when set. Both call sites
  (`bundle()`'s own, and `_bundle_linux_deb`'s, which also gained the same new parameter to
  forward it along) pass the already-resolved `window_title` value (`_resolve_window_title`,
  the 2026-08-10 window-title entry) — the exact same "game's display name" source, reused
  rather than re-resolved separately.

**Why reuse `window_title` instead of a fresh CLI flag/resolution:** `_resolve_window_title`
already implements exactly the lookup this needed (`manifest.json`'s `name`, falling back to
`package.json`'s), and by the time `_place_bundle_content` runs, `bundle()` has already called
it. One resolution, two uses (window title, cache directory name), rather than two separate
(and potentially divergent) name sources.

**Verification:** `cargo test -p roves-content-packer` — all 6 pre-existing `roundtrip.rs`
cases (both `PackOptions` literals there updated with `name: None`) plus the `size` unit test
and the 4 new unit tests (`sanitize_path_segment`/`default_dest`) pass; a real, non-mocked run
through `pack`/`load_manifest`/`prepare_dest`, not just a type-check. `python3 -m py_compile
python/servo/post_build_commands.py` passes. `cargo check -p servoshell` could not be
completed in this environment for the `bundle_launch.rs` change specifically — same
`libudev-sys` gap noted in the entry above; reviewed by hand instead (a single-line call-site
change, `resolve_dest(&content_dir, None, manifest.name.as_deref())`, using a binding —
`manifest` — already in scope one line above it).

---

## 2026-08-12 — Installable packages on Windows/macOS too: `mach bundle --msi`/`--dmg`

**Files:** `python/servo/post_build_commands.py`, new file
`support/windows/roves-bundle.wxs.mako`, `../.github/workflows/test.yml`.

**Patch:** `patches/servo-v0.4.0/0027-add-msi-dmg-installer-support.patch`

**Upstream behavior:** no equivalent for `mach bundle` (a Roves-added command, see the
2026-08-06 entry above) — `--deb` was the only installable-package option `mach bundle` had,
and only on Linux. Windows and macOS only ever produced the portable `play.exe`/`Roves.app`.
Separately, upstream's own *unrelated* `./mach package` command (`package_commands.py`) does
build a WiX `.msi` (Windows) and a `.dmg` (macOS) — but for the bare stock `servoshell`
binary, not wired to `mach bundle`'s content-dir-bundling/packed-content machinery at all.

**Why now:** asked directly — this fork ships to end users as a real game distribution,
where "download, double-click, play, no install" (the portable bundle) and "download an
installer, install it like any other app" are both things a game's own release pipeline
might want, per platform, exactly like Tauri's own bundler offers both an unpacked binary and
platform installers (`msi`/`nsis` on Windows, `dmg`/`app` on macOS, `deb`/`rpm`/`appimage` on
Linux) as separate targets — see `README.md`'s "Embedding" section on Roves' general posture
of matching Tauri's shape where it makes sense. Only `msi` (Windows) and `dmg` (macOS) are
added here, matching what's actually reusable today (see below); `nsis`/`rpm`/`appimage`
aren't implemented and should follow the same shape if added later, not block on this entry.

**Change:**

- **`--msi` (Windows only, new):** wraps the same portable output `--content-dir`/
  `_bundle_windows` already produce — built into a throwaway `_stage` subdirectory of
  `--output` instead of `--output` directly — into an installable `.msi` via WiX's
  `candle`/`light` (the same toolset `./mach package`'s own Windows installer uses). New
  `_wrap_windows_msi` renders `support/windows/roves-bundle.wxs.mako`, a **generalized**
  version of `support/windows/servoshell.wxs.mako`'s recursive directory-harvest technique
  (`include_directory`): rather than that template's fixed "servoshell.exe + resources/"
  shape, it walks whatever actually ended up in the staging directory — play.exe, its DLLs,
  launch.json, and (only when content wasn't packed, or always for the packed-archive case)
  whichever subfolder the html file's own directory put game content in — so it stays
  correct regardless of a given game's `--content-dir`/`--content-compress` combination,
  unlike hand-listing files the way the upstream template does. `Product/@UpgradeCode` is a
  deterministic `uuid5` of `--package-name` (stable across builds of the *same* game, so WiX's
  `MajorUpgrade` recognizes successive installs as upgrades rather than unrelated products);
  `Product/@Version` must be a plain `a.b.c.d` numeric MSI version (new module-level
  `_sanitize_msi_version`, which strips a leading `v` but otherwise raises — rather than
  silently mangling — on anything that isn't, since `--deb-version`'s old free-text
  tolerance doesn't carry over to a format MSI itself enforces). Requires `candle`/`light` on
  `PATH`; raises `BuildNotFound` with a clear message if missing, same convention as `--deb`'s
  `dpkg-deb` check.
- **`--dmg` (macOS only, new):** wraps the `Roves.app` bundle `_bundle_macos` already
  produces into an installable `.dmg` via `hdiutil`, same approach `./mach package` uses for
  stock Servo's `Servo.app` (including reusing `package_commands.py`'s
  `check_call_with_randomized_backoff` for the same "Resource busy" flakiness `hdiutil` has
  on GitHub Actions) — new `_wrap_macos_dmg`, adding the usual `/Applications` symlink next to
  the `.app` inside the mounted volume for Finder's drag-to-install gesture.
- **`--deb-package-name`/`--deb-version` renamed to `--package-name`/`--package-version`**
  (same defaults, `roves`/`0.0.0`): now shared across `--deb`/`--msi`/`--dmg` instead of three
  separate per-format flag pairs, since all three are "give this package a name and a
  version," not something specific to `.deb`. No compatibility shim for the old flag names —
  no tagged Roves release exists yet that could depend on them, and `../roves-action` (which
  mirrors this exact CLI, see `../CLAUDE.md`'s "keep roves-action in sync" section) is updated
  in the same turn as this entry.
- **`bundle()`'s internal flow:** both new formats stage into `<output>/_stage` (an ordinary
  subdirectory, built via the *exact same* `_bundle_windows`/`_bundle_macos` +
  `_place_bundle_content` calls the portable path already used, unchanged), then get wrapped
  into the real installer file written into `--output` itself, after which `_stage` is
  deleted — mirroring `--deb`'s existing `pkgroot`-then-delete shape, so `--output` ends up
  containing only the final installer artifact either way, never a mix of staging files and
  the installer.
- **`../.github/workflows/test.yml`:** the matrix grew from 4 entries (windows, macos, linux,
  linux-deb) to 6 — every platform now gets both its portable job and its installer job
  (`windows`+`msi`, `macos`+`dmg`, `linux`+`deb`), via a new `package_mode` matrix axis, rather
  than only Linux having an installable variant. Added a step putting WiX's `bin/` (present
  but not on `PATH` by default on the `windows-latest` runner image, which ships WiX Toolset
  v3 pre-installed per `actions/runner-images`) onto `PATH`, gated on `package_mode == 'msi'`.
  Each OS's two zip artifacts are now named distinctly (`servoshell-test_<os>-<mode>.zip`) so
  the portable and installer jobs' uploads don't clobber each other under the same rolling
  "test" release.

**Side effects to know about when upgrading:** none of this depends on Servo internals beyond
what the 2026-08-06 `mach bundle` entry already doesn't — `get_binary_path()`/`self.target`,
stable low-level `CommandBase` API. If a future Servo version changes what
`copy_windows_dlls_to_build_directory` drops next to the Windows binary, the same DLL glob
`_bundle_windows` already relies on (and `_wrap_windows_msi` harvests via its generic
directory walk) needs rechecking — no new dependency introduced by this entry specifically.

**Verification:** `python3 -c "import ast; ast.parse(...)"` on `post_build_commands.py`
passes (syntax only — no Rust changed, so no `cargo check`/`cargo test` needed here).
`roves-bundle.wxs.mako` was rendered directly with `mako.template.Template` against a
synthetic staging directory (flat files + a nested subfolder, standing in for a real
`play.exe`+DLLs+`launch.json`+packed-content layout) and the output parsed with
`xml.etree.ElementTree` to confirm it's well-formed XML with the expected recursive
`<Directory>`/`<Component>`/`<ComponentRef>` structure — this confirms the *template logic*
is correct, not that WiX's own `candle`/`light` accept it, or that `hdiutil`/the deb path
still work: none of `--msi`/`--dmg`/`--deb` (nor even the pre-existing portable paths) were
exercised through a real `./mach build` + `./mach bundle` in this environment (no Windows/
Rust toolchain available here) — treat the next real CI run of `../.github/workflows/test.yml`
(once it's actually running somewhere — see its own header comment on why it's dormant today)
as the real verification, and fix up anything that doesn't survive contact with real WiX/
`hdiutil` before relying on this.

**Correction (same day, after a real CI run) — `output_dir` wasn't made absolute:** exactly
the kind of bug the caveat above was hedging against. A real `windows-latest` run of
`--msi` failed in `light` with `LGHT0103: The system cannot find the file
'../release\_stage\play.exe'` (and the same for every other file in the bundle). Root cause:
`output_dir` (from `--output`, e.g. `../release` — relative to `mach bundle`'s own cwd) was
never resolved to an absolute path, so `stage_dir`/`msi_build_dir` (both derived from it)
stayed relative too. `_wrap_windows_msi` then `cd`s into `msi_build_dir` before invoking
`candle`/`light` — and WiX resolves each `<File Source="...">` path (baked into the `.wxs`
from that same relative `stage_dir`) relative to *its own* cwd at that point, not the cwd the
path string was originally relative to. With cwd now one level deeper
(`release/_msi-build` instead of `servo-src`), resolving the relative `../release/_stage/...`
from there doubled the `release` segment (`release/release/_stage/...`), which doesn't exist.
Fixed with a one-line change: `output_dir = path.abspath(output or path.join(binary_dir,
"bundle"))` instead of using `output`/the joined path as-is — every path derived from it
(`stage_dir`, `msi_build_dir`, the `.wxs`'s `Source=` attributes, the final `.msi`/`.dmg`
path) is now absolute from the start, immune to whichever cwd `candle`/`light`/`hdiutil`
actually run from. `--dmg`/`--deb` never `cd()` elsewhere mid-command, so they likely weren't
actually broken by this — but the fix applies to `output_dir` itself, upstream of all three,
so it's not a Windows/`--msi`-specific patch. Folded into the same
`0027-add-msi-dmg-installer-support.patch` rather than a new one, since it corrects a bug
in that same not-yet-released change, not a change to already-shipped behavior.

---

## 2026-08-12 — Boot splash redesign: Metal Mania wordmark, squared white progress bar

**Files:** `ports/servoshell/desktop/gui.rs`. Also new files `resources/fonts/
MetalMania-Regular.ttf`, `resources/fonts/MetalMania-OFL.txt`, and `resources/
roves_wordmark.svg` — not part of the patch (same reasoning as `test-page/public/icon.png`/
`icon.ico` in the "Game-supplied icon" entry above: plain binary assets, not derived from any
upstream file, and a text-based unified diff can't represent new binary file content at all).
Unlike that icon precedent, `MetalMania-Regular.ttf` *is* something `./mach build` needs to
find on disk (`gui.rs`'s `include_bytes!`) — `../.github/workflows/test.yml`'s "download +
patch Servo source" step now also copies `resources/fonts/` into the reconstructed tree, the
same way it already mirrors `test-page/public`/`test-page/dist` in for the same reason.
`roves_wordmark.svg` isn't referenced by any Rust code, so it doesn't need that treatment —
it's a repo-only design asset.

**Patch:** `patches/servo-v0.4.0/0028-boot-splash-wordmark-font-and-progress-bar.patch`

**Upstream behavior:** no equivalent — refines the 2026-08-09 "Native boot splash" entry's
`Gui::update_splash` (further up this file).

**Asked directly:** the existing splash's "Roves" label rendered in egui's plain default
font, and the progress bar was a stock `egui::ProgressBar` (rounded corners, default theme
fill color) — visually generic, not an intentional design.

**Change:**

- **New asset, `resources/roves_wordmark.svg`:** a small, self-contained SVG — the existing
  splash icon (`resources/servo_64.png`, embedded as a base64 `<image>`) beside the "Roves"
  wordmark, set in **Metal Mania** (embedded as a base64 `@font-face` in an inline
  `<style>`, so the file renders correctly anywhere without the font installed system-wide —
  verified by rendering it with `cairosvg` both before and after installing the font locally:
  without it, text fell back to a generic sans-serif; with it, or in any real `@font-face`-
  respecting renderer/browser, which is what actually matters since the font travels with
  the file, it renders in Metal Mania). This is a standalone design asset, not something
  rendered live by the engine — see the next point for why.
- **`resources/fonts/MetalMania-Regular.ttf` + `MetalMania-OFL.txt`:** the actual font file
  (from Google Fonts, `fonts.gstatic.com/s/metalmania/v23/...`), bundled for the engine to
  embed at compile time, plus its SIL Open Font License 1.1 text (`Copyright (c) 2012 by Open
  Window ... Reserved Font Name "Metal Mania"`) — OFL permits embedding/redistribution
  royalty-free; this is the first third-party font actually bundled into the binary (the
  existing CJK fallback fonts `configure_fonts`/`load_cjk_fonts` load are read from the
  *player's own OS* at runtime, never shipped).
- **`gui.rs`, new `add_wordmark_font`:** registers `MetalMania-Regular.ttf`
  (`include_bytes!` + `FontData::from_static`, no per-launch disk read) under its own egui
  font family, `FontFamily::Name("Metal Mania")` — deliberately *not* pushed onto
  `FontFamily::Proportional`'s fallback chain the way `load_cjk_fonts` does for CJK, since
  this is a one-off display font for a single label, not something the rest of the UI should
  ever fall back to. Called from `Gui::new` right after `configure_fonts()`, unconditionally
  on every platform (unlike `configure_fonts` itself, which is platform-gated for CJK system-
  font probing) — required promoting the `FontData`/`FontFamily` imports from
  `#[cfg(any(windows, linux, freebsd))]`-gated to unconditional, since macOS now needs them
  too.
- **`update_splash`:** the "Roves" label now renders via `egui::RichText` with
  `FontId::new(34.0, FontFamily::Name("Metal Mania"))` instead of a plain `colored_label`.
  The gap between the icon+wordmark row and the progress bar grew from 12px to 20px (asked
  for explicitly — the bar sits "a bit further below" the wordmark, not immediately
  adjacent). The progress bar itself gained `.fill(Color32::WHITE)` (white, was the default
  theme accent color) and `.corner_radius(CornerRadius::ZERO)` (squared-off, was egui's
  default rounded-rect) plus an explicit `.desired_height(18.0)` for a consistent thin bar
  regardless of theme defaults. The manual vertical-centering fudge factor (`available_height
  / 2.0 - N`) was bumped from `40.0` to `51.0` to account for the larger gap — still an
  estimate, not computed from actual measured widget sizes (egui only knows a widget's real
  size after laying it out), same caveat the original value already carried.

**Why not render `roves_wordmark.svg` itself in the running splash:** there is no SVG
rasterization path anywhere in this engine or its dependencies (confirmed — no `resvg`/
`usvg` or equivalent; `favicon.svg`/`icon.svg`'s existing uses are all for the *unrelated*
game-icon-fallback feature, never rasterized at runtime either). Adding one just to redraw a
static icon+text lockup egui can already compose natively (an `egui::Image` for the icon,
`RichText` with a loaded font for the text — exactly what `update_splash` already did before
this change, now with the right font) would be a disproportionate new dependency for no
visual benefit: egui's own text rendering is already a vector/font rasterizer, not a bitmap
fallback. The SVG asset exists for uses *outside* the running engine (marketing, the wiki,
README embeds, etc.) where an actual SVG file is the right format.

**Side effects to know about when upgrading:** none of this touches Servo internals beyond
`egui`'s own stable, high-level widget API (`RichText`, `FontId`, `FontFamily`,
`ProgressBar`) — should survive a version bump untouched unless a future egui major version
renames `CornerRadius` (it was `Rounding` before egui ~0.32) or changes `FontData`'s
construction API.

**Verification:** `rustfmt --check --edition 2024 ports/servoshell/desktop/gui.rs` reports no
diff anywhere in the changed regions (the one pre-existing diff it does report, at an
unrelated `if let ... &&` chain further down the file, predates this change — confirms the
edit is both syntactically valid and already correctly formatted, without needing a full
build). A real `cargo check -p servoshell` still isn't completable in this environment — same
`libudev-dev`/`pkg-config` gap noted in earlier entries (`pkg-config --exists libudev` fails
here; only the runtime `libudev1` package, not `libudev-dev`, is installed), unrelated to
this change specifically. `resources/roves_wordmark.svg` was validated as well-formed XML
(`xml.etree.ElementTree`) and actually rendered to a PNG with `cairosvg` (see above) to
confirm the layout/embedding works, not just that the markup parses. Treat the next real
`./mach build` + manual run of this fork as the actual visual verification (exact pixel
centering in particular, given the fudge-factor caveat above) before considering this done.

---

## 2026-08-12 — Boot splash resize, manual centering, and readable progress bar

**Files:** `ports/servoshell/desktop/gui.rs`, `ports/servoshell/desktop/app.rs`,
`ports/servoshell/desktop/headed_window.rs`.

**Patch:**
`patches/servo-v0.4.0/0029-boot-splash-resize-recenter-and-progress-bar-redesign.patch`

**Upstream behavior:** no equivalent — refines the 2026-08-09 "Native boot splash" and
2026-08-12 "Boot splash redesign" entries' `Gui::update_splash` (further up this file).

**Asked directly, from a screenshot of a real launch:** four things wrong with the splash
from the previous entry — (1) the icon+wordmark row rendered pinned to the left edge instead
of centered; (2) the icon (`resources/servo_64.png`) and "Roves" wordmark read as too small,
with the wordmark not enough bigger than the icon; (3) the progress bar was too thick and, at
any fill level, visually indistinguishable from an empty bar; (4) no bar was visible at all
before extraction actually started.

**Root causes, investigated before changing anything:**

- **(3) is the real bug, not a perception issue.** `egui::ProgressBar` paints its track in
  `visuals.extreme_bg_color`, and paints its fill in whatever `.fill()` is given. This app
  sets `options.fallback_theme = egui::Theme::Light` (`Gui::new`) and doesn't otherwise force
  a dark theme, so on a system reporting (or defaulting to) a light theme,
  `extreme_bg_color` is `Color32::from_gray(255)` — pure white — which the previous entry's
  `.fill(Color32::WHITE)` exactly matches. Track and fill were the same color at every
  progress value, so the "bar" only ever read as a static white rectangle, never as a loading
  indicator. Confirmed by reading `egui`'s vendored `progress_bar.rs`/`style.rs` sources
  directly (this pinned version, 0.34.3, is present in the local Cargo registry cache), not
  guessed.
- **(4) was deliberate in the previous design** (`SPLASH_PROGRESS_BAR_DELAY`, "long enough
  that a fast/no-op extraction never shows a bar at all") but is exactly what was asked to
  change: the bar should be visible-but-empty from the very first frame.
- **(1) could not be conclusively root-caused against real rendering** (no display in this
  environment), but was investigated as far as static analysis allows: `ui.horizontal(...)`
  nested inside `ui.with_layout(Layout::top_down(Align::Center), ...)` is the standard egui
  idiom for centering a row and, per the egui source, *should* center correctly. Rather than
  keep trusting that against contrary empirical evidence (the screenshot), the row is now
  centered by explicit, *measured* horizontal padding instead — see below. This sidesteps the
  question of whether the old approach was actually buggy or something else was going on,
  since the new approach is correct either way.
- **On the icon itself:** `resources/servo_64.png` (loaded by
  `load_splash_icon_image`/`Gui::update_splash`) was checked pixel-by-pixel against
  `icon.svg` (repo root) and is a 64px rasterization of it — a generic, Recraft-AI-generated
  "three wolf heads in chains" clip-art image, not a Roves logo. No Roves-branded icon asset
  (matching the colorful lockup mark referenced in chat) exists anywhere in this repo, its
  three sibling checkouts (`roves-action`, `roves-api`, `roves-wiki`), or
  `resources/roves_wordmark.svg`'s own embedded icon (byte-identical to `servo_64.png`, i.e.
  the same wolf placeholder, not a different asset). **This entry does not change the icon
  file** — swapping in real Roves branding needs that asset supplied first; asked about
  separately in chat rather than guessed at here.

**Change:**

- **`gui.rs`, new constants:** `SPLASH_ICON_SIZE` (64.0 → 128.0) and
  `SPLASH_WORDMARK_FONT_SIZE` (34.0 → 88.0), the latter kept at the same icon-height-relative
  proportion (44/64 ≈ 0.69) as the reference lockup in `resources/roves_wordmark.svg`, just
  scaled to the new icon size — satisfies "both bigger, wordmark a bit bigger relative to the
  icon than before" without inventing a new ratio.
- **Manual horizontal centering:** `update_splash` now measures the wordmark's actual pixel
  width via `ctx.fonts_mut(|fonts| fonts.layout_no_wrap(...))` before building the row, then
  inserts a leading `ui.add_space(...)` inside the `ui.horizontal` sized to
  `(available_width - (icon_width + spacing + wordmark_width)) / 2`. This replaces reliance
  on `top_down(Align::Center)` centering the nested row on its own with an exact, measured
  centering that doesn't depend on that behavior at all — a stronger fix than re-guessing at
  whatever `Align::Center` was or wasn't doing.
- **New `draw_splash_progress_bar`:** replaces the `egui::ProgressBar` widget with a
  hand-painted track (`Ui::painter().rect_filled` + `rect_stroke`, a dim translucent-white
  fill with a brighter outline, `SPLASH_PROGRESS_BAR_HEIGHT = 6.0` thin, `CornerRadius::same(2)`
  — still visibly rectangular, per feedback, just softened at the corners rather than the
  previous hard square) and an opaque white fill rect sized to `progress`, drawn on top.
  Always draws the track, even at `progress == 0.0`, so the bar reads as "a loading indicator
  that's currently empty" rather than disappearing. Considered adding a crate for this
  (offered in chat) but a hand-painted rect is a few lines against an API already in use
  elsewhere in this file, with the same "not worth a new dependency for something this
  simple" reasoning as the previous entry's SVG-rendering call — no new dependency added.
- **`update_splash`'s signature, `progress: Option<f32> → f32`:** now that the bar is always
  drawn, `None` (meaning "don't draw it yet") no longer has a use — the type change makes
  that explicit instead of leaving a now-meaningless `Option` around. Propagated through
  `HeadedWindow::paint_splash` (`headed_window.rs`) and its one call site
  (`app.rs`'s `window_event`, now just `headed_window.paint_splash(*progress)`).
- **`app.rs`, removed `SPLASH_PROGRESS_BAR_DELAY`:** existed solely to gate *when* the bar
  started rendering (`(elapsed >= SPLASH_PROGRESS_BAR_DELAY).then_some(*progress)`); with the
  bar now unconditional, that gate was dead weight. `try_finish_booting`'s and `init`'s
  `ControlFlow::WaitUntil` re-arm logic — previously waking at whichever of
  `SPLASH_PROGRESS_BAR_DELAY`/`MIN_SPLASH_DURATION` hadn't yet passed — now just targets
  `MIN_SPLASH_DURATION` directly, since that's the only remaining thing `try_finish_booting`
  waits on. Behavior is unchanged in the case that matters (the busy-poll-until-extraction-
  done path once `MIN_SPLASH_DURATION` has already elapsed): both old and new code compute a
  zero wait there, verified by re-deriving the arithmetic, not just inspection.

**Side effects to know about when upgrading:** same as the previous entry — plain, stable
`egui` widget/painter API (`Ui::painter`, `rect_filled`, `rect_stroke`, `Fonts::layout_no_wrap`),
nothing touching Servo internals. `StrokeKind` (required by `rect_stroke` in this egui
version) is worth re-checking if painter signatures change in a future egui major version.

**Verification:** unlike every prior boot-splash entry, this one did *not* stop at
`rustfmt --check` and static reading — `cargo check -p servoshell` was actually attempted
(still blocked here on the documented `libudev-dev` gap, this time confirmed directly rather
than assumed, and additionally on this sandbox being too resource-constrained to finish
compiling `mozangle`'s bundled ANGLE via cargo check on the full workspace even with the
`gamepad` feature disabled to dodge libudev — a build-script `cc`/C++ compile got OOM-killed).
Given that, the actual new code (`draw_splash_progress_bar`, the measured-centering logic in
`update_splash`, and the `EguiGlow::run`/`CentralPanel::show` interaction the previous entry's
"deprecated `Panel::show`" comment alludes to) was instead verified by compiling it for real
in an isolated throwaway crate pinned to the exact same `egui = "=0.34.3"` / `egui_glow =
"=0.34.3"` (with the `winit` feature, matching `ports/servoshell/Cargo.toml`) versions this
workspace resolves to — confirmed the `EguiGlow::run` closure parameter is actually
`&mut egui::Ui` (not `&Context`, despite being named `ctx`), that `.show(ctx, ...)` only
type-checks via `Ui`'s `Deref<Target = Context>` impl, and that the new font-measurement and
painter calls compile against the real API. Additionally, `rustfmt --check --edition 2024`
was run on all three changed files (clean on every changed region — the several pre-existing
unrelated diffs it reports elsewhere in `app.rs`/`gui.rs`/`headed_window.rs`, all stale
`if let ... &&`-chain formatting, predate this change and were deliberately left alone). Most
importantly, `patches/servo-v0.4.0/0029-...patch` was verified end-to-end against a fresh
pristine `v0.4.0` download: extracted clean, applied patches `0001` through `0028` in order
(all applied without a reject, two with a harmless line-offset), confirmed the result was
byte-identical to this repo's own `HEAD` for all three files, then applied `0029` on top and
confirmed *that* result was byte-identical to the actual working tree — i.e. the patch is
proven mechanically reproducible from pristine upstream, not just "looked correct." Still
missing: an actual `./mach build` + manual run, same as every prior entry in this section —
in particular this doesn't prove the *visual* result (exact centering, whether 128px/88pt
reads as "much bigger" as intended) is right, only that it's what the code says it should be.

---

## 2026-08-13 — Regenerate Roves icon raster/format assets from `icon.svg`

**Files:** `resources/servo_64.png`, `resources/servo_1024.png`, `resources/servo.ico`,
`resources/servo.icns` — binary raster assets, not part of any patch (same reasoning as the
`test-page/public/icon.png`/`icon.ico` and `resources/fonts/` entries above: a text-based
unified diff can't represent new binary content). Also `.gitattributes` — see the last
paragraph below. `resources/servo.svg` and `support/openharmony/.../servo_{64,1024}.png` are
unaffected — the former was already byte-identical to `icon.svg` (confirmed via checksum),
and the latter are plain-text path placeholders pointing back at
`resources/servo_{64,1024}.png`, not actual copies, so they pick up this change automatically.

**Upstream behavior:** n/a — `icon.svg` (repo root) and all of the assets above are already a
Roves-specific replacement of upstream Servo's own icon (see the 2026-08-09 "Game-supplied
icon" entry's `resources/servo_64.png`/`servo.ico`), added whole-cloth in a prior commit
(`ca839e6`, "icon") that replaced the binaries directly without a documented generation
pipeline.

**Asked directly:** confirm the "Servo-branded, not any particular game's" icon used by the
boot splash (`Gui::update_splash`, previous entries above) and the window/taskbar/exe-icon
fallback (`headed_window.rs`/`build.rs`) is actually generated from `icon.svg`, and convert it
to every extension/size those consumers need. Investigated first: `resources/servo_64.png`
and `resources/servo_1024.png` were already pixel-identical rasterizations of `icon.svg` (not
upstream Servo's own logo — confirmed by comparing color histograms and a fresh
`cairosvg` render pixel-for-pixel), and `resources/servo.svg` was already byte-identical to
`icon.svg`. What was actually incomplete: `resources/servo.icns` had only a single `ic09`
(512×512) entry — no retina (`@2x`) variants and nothing below 512px, which macOS's Finder/
Dock render poorly at smaller sizes (upscaling a 512px source, or falling back to a generic
icon, depending on context) — and `resources/servo.ico` had 16/24/32/48/256px but was
missing the 64px and 128px sizes Windows uses for some Explorer view modes.

**Change:** rebuilt every derived asset from `icon.svg` through one pipeline (`cairosvg` to
rasterize the vector, Pillow for resizing/`.ico` packing, the `icnsutil` Python package for
`.icns` composition — all three already available in this environment, no new dependency):

1. Rendered `icon.svg` once at 2048×2048 via `cairosvg.svg2png` as a master raster (this repo
   has no SVG rasterization path at *runtime*, per the 2026-08-12 wordmark entry above, but
   that's about the running engine specifically — offline asset generation is a different
   question and unaffected by that constraint).
2. `resources/servo_1024.png`/`servo_64.png`: downsized from the master with Pillow's
   `Image.resize(..., Image.LANCZOS)` — content unchanged (still the same icon at the same
   two sizes), but a fresh render rather than a prior possibly-recompressed copy.
3. `resources/servo.ico`: regenerated via `Image.save(..., format="ICO", sizes=[16, 24, 32,
   48, 64, 128, 256])`, each size resampled individually from the 2048px master rather than
   letting the `.ico` encoder cascade-resize from a single frame — now has all 7 standard
   Windows icon sizes instead of 5.
4. `resources/servo.icns`: composed via `icnsutil compose` from a full Apple iconset (16, 32,
   128, 256, 512, plus each size's `@2x` retina variant, i.e. 16 through 1024px) — 10 entries
   (`icp4`, `ic11`, `icp5`, `ic12`, `ic07`, `ic13`, `ic08`, `ic14`, `ic09`, `ic10`) instead of
   the previous single `ic09`.

**Side effects to know about when upgrading:** none — these are pure design assets consumed
via `include_bytes!`/file-copy (`build.rs`, `headed_window.rs`, `gui.rs`), not upstream Servo
files, so there's nothing to reapply against a new tag; just re-run the same pipeline against
`icon.svg` if that source ever changes.

**Verification:** `PIL.Image.open` confirms `servo.ico` now reports all 7 requested sizes
`{16,24,32,48,64,128,256}`; `icnsutil info` confirms `servo.icns`'s 10 entries with the
expected type codes and pixel dimensions above; `file` confirms both are still recognized as
valid MS Windows icon resource / Mac OS X icon files respectively, not just well-formed by
their own tooling's say-so. `md5sum` reconfirmed `resources/servo.svg` is still byte-identical
to `icon.svg` after this change (untouched, as intended). Not done: an actual visual check on
Windows/macOS that Explorer/Finder pick the new sizes correctly — no such platform available
here.

**Also:** `.gitattributes` gained `*.icns binary`. `resources/servo.icns` was already tracked
without one (unlike `.ico`, which got its own rule in the 2026-08-09 "Game-supplied icon"
entry above for exactly this reason) — same silent-corruption-on-Windows-checkout risk from
falling under the blanket `* text=auto eol=lf` rule, just not caught until this file was
touched again.

---

## 2026-08-13 — Startup file logging, so a silently-failing `play.exe` is diagnosable

**Files:** `ports/servoshell/desktop/cli.rs`, `ports/servoshell/desktop/bundle_launch.rs`, new
`ports/servoshell/desktop/logging.rs`, `ports/servoshell/desktop/mod.rs`,
`ports/servoshell/panic_hook.rs`, `ports/servoshell/Cargo.toml`, `components/servo/servo.rs`,
`support/content-packer/src/extract.rs`. `Cargo.lock` picked up the matching `servoshell` →
`env_logger` dependency edge automatically — not part of the patch, same as every other
`Cargo.lock` change in this file's history (regenerated by Cargo itself, not hand-diffable).

**Patch:**
`patches/servo-v0.4.0/0030-startup-file-logging-for-diagnosing-silent-launches.patch`

**Upstream behavior:** no equivalent — a `log` backend (`env_logger`, straight to stderr)
wasn't installed at all until `Servo::setup_logging()` ran, deep inside `App::finish_init`,
itself only reached once a `Servo` instance exists. Every `log::error!`/`warn!` call before
that point — including, critically, the ones already inside
`bundle_launch::resolve_bundled_launch_args`/`resolve_packed_content_url` reporting a missing
or corrupt packed-content bundle, exactly the failure this entry exists to diagnose — was
silently discarded: `log`'s default no-op logger just drops anything logged before a real one
is installed.

**Asked directly:** "quando provo ad avviare play.exe non succede nulla" (nothing happens on
launch) — with no way to tell why. Investigated first, rather than guessed at: `main.rs` sets
`#![windows_subsystem = "windows"]`, so a double-clicked `play.exe` has no console at all
(stderr goes nowhere visible even for the logging that *does* happen after `setup_logging()`
runs); combined with the above gap, a failure anywhere in the first several hundred lines of
startup produced zero observable output, console or otherwise — genuinely "nothing happens,"
not just "something happens somewhere invisible."

**Also asked directly:** put the log file next to the extraction cache under
`%LOCALAPPDATA%\<game name>\` (`AppData\Local\Roves test-page\...` was the example given) —
not inside the game's own content/install directory — and have it start empty on every
launch (no accumulating history across runs) while capturing everything: Roves/Servo's own
logging and the game's own console output.

**Change:**

- **`support/content-packer/src/extract.rs`, new `game_data_dir(game_name)`:** the per-game
  top-level folder under the OS cache directory (`%LOCALAPPDATA%` on Windows,
  `~/Library/Caches` on macOS, `~/.cache`/`$XDG_CACHE_HOME` on Linux) that `default_dest`
  already nested the extraction cache under (`<this>/cache/<hash8>/`) — pulled out into its
  own function so a log file can sit at `<this>/roves.log`, a *sibling* of `cache/`, using the
  exact same name-sanitizing/`"roves"`-fallback logic (see `sanitize_path_segment`) instead of
  duplicating it. `default_dest` now calls this instead of inlining the same two lines.
  Existing behavior/tests unaffected — verified with `cargo test -p roves-content-packer`
  (all 6 pre-existing tests plus this entry's new `game_data_dir_is_default_dests_cache_grandparent`
  still pass), not just by inspection.
- **`bundle_launch.rs`, new `peek_game_name_for_logging`:** a cheap, side-effect-free,
  deliberately non-logging peek at `launch.json`/`manifest.json` — just enough to learn the
  game's name (or `None`) *before* `resolve_bundled_launch_args` itself runs, so `cli::main`
  can pick the right log directory in time to capture that very function's own diagnostics.
  Re-reads the same two small JSON files `resolve_bundled_launch_args` will read again
  properly shortly after, rather than threading a shared result across the gap — the two run
  at genuinely different times (this one strictly first), and duplicating a couple of cheap
  file reads is simpler and safer than the alternative.
- **New `ports/servoshell/desktop/logging.rs`:** `init(log_dir)` creates `log_dir` if needed,
  opens `log_dir.join("roves.log")` with `File::create` (truncates — a fresh, empty file every
  launch, exactly as asked), and installs an `env_logger`-style logger targeting that file
  (`env_logger::Target::Pipe`) as the process's global `log` backend. Defaults to `info`-level
  filtering (`env_logger`'s own default is `error`-only) since a private log file, unlike a
  terminal, isn't noisy for the user; still overridable via `RUST_LOG`. No new hook was needed
  to capture the game's own console output: `headed_window.rs`/`headless_window.rs`'s
  `show_console_message` already forwards every `console.log`/`warn`/`error` through
  `log::log!`, and `panic_hook.rs` already routes panics through `log::error!` — so a single
  logger installed here transparently captures Roves/Servo's own logging, the game's console
  output, and startup panics, all in one file, none of it requiring changes beyond installing
  the logger early enough.
- **`cli.rs`, `main()`:** calls the above — `logging::init(&extract::game_data_dir(peek_game_name_for_logging().as_deref()))`
  — as close to the top of `main` as possible (right after `crash_handler::install`/
  `init_crypto`, before `panic::set_hook` and everything else). Gated on
  `env::args().nth(1).is_none()`, the exact same "is this a genuine double-click/bundled
  launch" check `resolve_bundled_launch_args` already uses — **not** an arbitrary restriction,
  but a correctness requirement: Servo's own multiprocess content-process children re-exec
  *themselves* with `--content-process <token>` in argv, and each one installing its own
  *truncating* file logger would race every other process (including the main one) writing to
  that same path, each wiping out whatever the others had already logged. Content processes
  now behave exactly as before this entry (unaffected — they still get `Servo`'s own
  content-process `set_logger`, to stderr).
- **`components/servo/servo.rs`, `setup_logging`/`set_logger`:** `log::set_boxed_logger` only
  ever succeeds once per process, and now that `logging.rs` installs its own logger earlier
  (for the main/chrome process), Servo's later attempt was guaranteed to hit that and panic on
  the `.expect("Failed to set logger.")` that used to be here — checked this against the
  installed `log` crate's own docs/source, not assumed. Changed both call sites to a plain
  `if log::set_boxed_logger(...).is_ok() { log::set_max_level(filter); }`: whichever logger
  installs *first* wins silently, no panic. Trade-off, stated plainly rather than silently
  accepted: `FromEmbedderLogger`'s constellation-forwarding (an embedder-side crash/warning-UI
  hook) never runs once servoshell's own logger has already installed — this fork has no such
  UI to forward to (see this file's very first entries, removing the toolbar/tabs entirely),
  so that's an intentional no-op, not a quietly dropped feature something else still expects.
- **`panic_hook.rs`:** the panic message's file:line/thread detail (previously written to
  stderr only) is now built once and passed to *both* the stderr write and the final
  `log::error!` call — previously that call only logged the bare message, missing exactly the
  detail most useful for diagnosing *which* panic happened, in the one sink (the log file)
  actually likely to be visible on a windowed build.

**Side effects to know about when upgrading:** `setup_logging`/`set_logger`'s guard is a
one-line behavioral change against upstream `Log`/`env_logger` APIs (`log::set_boxed_logger`
returning `Result`) that have been stable for a long time — low risk on a version bump.
Everything else lives entirely in `ports/servoshell`/`support/content-packer`, untouched by
upstream Servo changes by construction.

**Verification:** `cargo test -p roves-content-packer` (all tests pass, including the new
one). `logging.rs`'s core logic (file truncation, `env_logger` target/filter wiring,
`log::set_boxed_logger` installation) was compiled and actually run — not just read — in an
isolated throwaway crate pinned to this workspace's exact `env_logger`/`log` versions,
confirming a real log file gets created, truncated, and populated with both an explicit test
log line and this module's own startup line. `peek_game_name_for_logging` +
`extract::game_data_dir` were likewise run together (not just inspected) against a real
fixture `launch.json`/`manifest.json` pair, confirming the resolved directory matches
`%LOCALAPPDATA%\Roves test-page\` exactly as asked. `rustfmt --check --edition 2024` is clean
on every changed region of every file except `extract.rs`'s two pre-existing, unrelated
`&&`-chain-formatting diffs (confirmed pre-existing by checking the *unmodified* `HEAD`
version of that file reports the identical two diffs — not something this change introduced).
`patches/servo-v0.4.0/0030-...patch` was verified end-to-end: applied cleanly on top of a
fresh pristine `v0.4.0` extraction with patches `0001`–`0029` already applied (itself
reconfirmed byte-identical to this repo's own `HEAD`), and the result after applying `0030` is
byte-identical to the actual working tree. Not done, same caveat as every entry in this
section: an actual `./mach build` + manual run on a real Windows machine, to confirm a
genuinely broken launch now actually produces a readable `roves.log` instead of nothing.

**Update, same day — milestone logging, after this alone still wasn't enough:** a real
Windows portable-bundle test came back with exactly one line in `roves.log` — this module's
own "Roves logging started" line — and nothing else, no matter what actually failed. That
still leaves a huge unlogged span (window/GL context creation, boot extraction, Servo
construction) where a hang or a hard native crash (GPU driver, ANGLE/GL context issue, a
missing DLL) can happen without ever reaching `panic_hook.rs` — a native crash of that kind
bypasses Rust's panic machinery entirely, so no amount of `.expect()`-to-`log::error!`
plumbing in Rust code would have caught it. Added bracketing `log::info!` calls (paired
before/after) at the remaining startup milestones most likely to hide such a crash:
`cli::main` (resolved launch args, parsed CLI args, event loop created, entering
`run_app`), `App::init` (immediately around `create_platform_window` — winit window + GL
surface creation, the single most GPU/driver-crash-prone step in this whole path), and
`App::finish_init` (immediately around `servo_builder.build()`). Also bracketed `Gui::new`'s
first `update_splash`/`paint` call (`gui.rs`) — the actual first GL draw/buffer-swap this
process makes, right after context creation, and therefore just as plausible a native-crash
site as context creation itself. None of this is meant to be permanent — it's deliberately
coarse-grained, commented as such, and should come back out once a real crash has actually
been localized this way; it's the diagnostic equivalent of `println!`-debugging, not a
lasting change to how this app logs.

**Update, same day — found and fixed the real bug the milestone logging was chasing:** the
milestone logging above worked immediately — a real Windows portable-bundle run logged
`resolved launch args: [..., "--package-name", "servoshell-test", "--package-version",
"0.4.0", ...]` and then nothing further, meaning the crash was between that line and
`parsed command line arguments`. `--package-name`/`--package-version` are `mach bundle`'s
*own* flags (`python/servo/post_build_commands.py`, used only to name a `--deb`/`--msi`/
`--dmg` output) — not anything `ports/servoshell/prefs.rs`'s CLI parser recognizes. Somehow
(root mechanism not fully pinned down — extensive attempts to reproduce it by faithfully
reconstructing `mach bundle`'s actual registered argparse arguments and re-running
`parse_known_args` with the exact CI invocation kept coming back clean, i.e. **not**
reproducing the leak; this remains an open question) they ended up forwarded into
`launch.json`'s `"args"`, which `bundle_launch.rs` feeds straight into
`prefs::parse_command_line_arguments`. That parser (`bpaf`) rejects unknown flags outright
— confirmed directly: running the real, freshly-built Linux `servoshell` binary with these
exact args reproduces `Error: --package-name is not expected in this context`, immediately,
before anything else runs. Combined with `ArgumentParsingResult::ErrorParsing` in `cli::main`
calling `std::process::exit(1)` with no logging on that path, and no console on a
double-clicked Windows build, this is the exact, complete explanation for "play.exe does
nothing."

**Change:** `post_build_commands.py`'s `bundle`, right before building `extra_args`, now
cross-checks every token in `params` against `self.__class__.bundle._mach_command.arguments`
— the same metadata `mach`'s own dispatcher (`python/mach/mach/dispatcher.py`) uses to build
the subcommand's parser — and drops any that match one of this command's *own* flag spellings
(along with the value that flag takes, looked up via that same metadata's `action`, so a
boolean flag like `--msi` doesn't wrongly eat the next legitimate passthrough token). Prints a
`warning:` line naming exactly what got dropped, so this is loud instead of silently
corrupting `launch.json` again in some future incident of the same shape. This is a defense
against the *symptom* (a reserved flag ending up in `params`) rather than a fix for the
underlying argparse mechanism, precisely because that mechanism wasn't fully pinned down —
see above.

**Not part of the `roves-action` sync** (see `CLAUDE.md`'s mirroring requirement): this
changes internal handling of an existing flag, not `mach bundle`'s CLI surface itself — no
flag was added, removed, or redefaulted, so `roves-action`'s `action.yml`/`README.md` need no
matching update.

**Patch:** `patches/servo-v0.4.0/0031-filter-mach-bundles-own-flags-out-of-game-launch-args.patch`

**Verification:** the exact filtering logic (reserved-flag detection + value-skipping) was
extracted and unit-tested standalone against four cases — the real observed leak (both
`--package-name` and `--package-version` correctly stripped along with their values), a
mix of a leaked boolean flag (`--msi`) and a legitimate passthrough flag+value (both handled
correctly), and two no-op cases (nothing leaked) — all four produced the expected
`extra_args`/`leaked_reserved_flags`. The file's own syntax was checked with `ast.parse`
(this sandbox's Python 3.10 can't actually run `mach` itself — `python/mach`/`python/tidy`
depend on Python 3.11+ stdlib additions (`contextlib.chdir`, `typing.LiteralString`) this
environment doesn't have, confirmed while trying; not a gap introduced by this change).
`patches/servo-v0.4.0/0031-...patch` was verified end-to-end the same way as every other
patch in this file: applied cleanly on top of a pristine `v0.4.0` extraction with patches
`0001`–`0030` already applied, producing a result byte-identical to the actual working tree.
Not done: an actual `mach bundle` run (blocked by the Python version gap above) confirming
the warning prints and `launch.json` ends up clean — the next real Windows/`mach`-capable
build should confirm this closes out the original report.

---

## 2026-08-14 — Bundled launches no longer die on a broken `launch.json`, and why the previous fix didn't actually close this out

**Update on the entry above:** the `mach bundle` run that entry's own "not done" note asked
for actually happened — a real Windows portable bundle was built by `.github/workflows/
test.yml` at the previous entry's commit (`6e3f1f5`, the one adding the `post_build_commands.py`
reserved-flag filter) and run for real. It still crashed, identically to before: `Error:
--package-name is not expected in this context`, no window, no `roves.log` line beyond
nothing at all. `launch.json` in that exact build's bundle still literally contains
`"--package-name", "servoshell-test", "--package-version", "0.4.0"` in its `"args"` array.
**The previous fix does not work.** The same crash was independently confirmed on the
`--msi`-mode bundle's `play.exe` too (its `launch.json` carries the same leaked flags plus
`--msi`) — not specific to the plain portable path.

**Root cause, still not fully pinned down — new evidence, same conclusion as before:** the
exact `bundle()` reserved-flag filter from the previous entry was extracted verbatim, along
with a byte-faithful reconstruction of `mach`'s own dispatcher (`python/mach/mach/
dispatcher.py`'s `_run_command_handler`, the code that separates the `params` REMAINDER
catch-all from every other registered flag) and the *complete* real argument list for
`bundle` (every `@CommandArgument` on it, plus every flag `common_command_arguments(binary_
selection=True)` adds — `--release`, `--dev`/`--debug`, `--prod`/`--production`, `--profile`,
`--with-asan`, `--with-tsan`, `--bin`, `--nightly`/`-n`, `--coverage`, not just the packaging-
specific ones). Fed the *exact* CI invocation (`--content-dir test-page/dist --output
../release --package-name servoshell-test --package-version 0.4.0`) through this reconstruction
in an isolated Python process (no `mach_bootstrap`, no venv): parsing comes back **completely
clean** — `package_name`/`package_version` land correctly in `command_namespace`, `params`
ends up empty, nothing for the filter to even do. This exactly reproduces the previous
entry's own "extensive attempts... kept coming back clean" finding, now with the complete
argument set rather than a partial one, closing off "an incomplete reconstruction was masking
it" as an explanation. Also directly ruled out: `mach`'s own polyglot shell wrapper re-execing
via `uv run --frozen python ${MACH_DIR}/mach "$@"` mangling argv before Python ever sees it —
tested directly (a throwaway script printing `sys.argv`, invoked both directly and through
`uv run --frozen python`, real `uv` binary, same argument list) — argv comes through
byte-identical either way. So the leak genuinely only manifests inside the full `mach`
process (global-argument parsing in `mach.run()`, command/provider registration, or something
else full-app-only) — not reproducible against the isolated pieces, and not re-investigated
further given the cost of standing up a complete local `mach` environment (blocked on this
sandbox lacking a Visual Studio install `pyyaml`'s C extension needs to build via `uv run`,
which is itself required before `mach` will even run) for what would be continued archaeology
of an already twice-inconclusive investigation.

**Change, this time targeting the actual user-visible failure instead of the upstream leak:**
given the leak's precise mechanism has now resisted two independent investigations, and a
post-hoc "filter known-reserved flags out of `params`" approach already failed once in
practice despite looking correct in isolation, this entry stops trying to guarantee
`launch.json` is always clean and instead makes a broken one non-fatal. `ports/servoshell/
desktop/cli.rs`'s `main()`: `resolve_bundled_launch_args()`'s `Some`/`None` match now also
threads through `is_bundled_launch: bool` (previously discarded). The `parse_command_line_
arguments` match gains one new arm: `ArgumentParsingResult::ErrorParsing if is_bundled_launch
&& args.len() > 1` — logs the failing `args` and the fact that a retry is happening, then
calls `parse_command_line_arguments` a second time with just `&args[..1]` (the content URL
alone, every extra arg dropped), and only exits(1) if even *that* somehow fails to parse.
Real (non-bundled) invocations — a developer running the shipped binary from a terminal with
their own typo'd flag — are completely unaffected: `is_bundled_launch` is `false` for those,
so the existing `ArgumentParsingResult::ErrorParsing => std::process::exit(1)` arm still
applies unchanged, still hard-erroring exactly as before. The distinction matters: a CLI typo
has a user present to see and fix it; a corrupt/poisoned `launch.json` does not — the person
who'll eventually see the failure is a player double-clicking `play.exe`, and "the game
silently never starts, forever, until someone rebuilds it" is a strictly worse failure mode
than "the game starts with default window size/title instead of whatever `launch.json` asked
for."

**Not part of the `roves-action` sync:** pure Rust-side behavior change to an existing
internal failure path, not a `mach bundle` CLI surface change — nothing in `action.yml`/
`roves-action`'s README describes this.

**Patch:** `patches/servo-v0.4.0/0032-dont-exit-on-broken-bundled-launch-args.patch`

**Verification:** applied cleanly on top of a pristine `v0.4.0` extraction with patches
`0001`–`0031` already applied, producing a result byte-identical to the actual working tree.
The change was pushed to trigger `.github/workflows/test.yml` for real (this repo is
`DRincs-Productions/roves`, a genuine top-level GitHub repo with Actions enabled — not the
dormant in-parent-project state `CLAUDE.md`'s workflow-location note describes), and the
resulting Windows portable + `--msi`-mode bundles were downloaded from the rolling `test`
release and actually run, exactly the way the previous entry's crash was first confirmed.

**Outcome:** confirmed fixed. The fresh Windows portable and `--msi`-mode bundles built by that
CI run (same leaked `launch.json` as ever — `--package-name`, `--package-version` still
present) were downloaded and actually launched: `roves.log` shows the parse failure, the
logged retry, then a full successful boot (`parsed command line arguments` → `created event
loop` → `creating platform window` → `built Servo instance` → the page's own content
rendering), and the window stays open and responsive. Both `play.exe` copies (plain portable
and the `--msi`-staged one) behave identically.

---

## 2026-08-14 — Fixing patch `0027` itself: a later, unrelated commit had silently truncated it

**Not a source change — a `patches/` integrity bug, caught while verifying the entry above.**
While re-deriving the exact byte-for-byte state `patches/servo-v0.4.0/0027-add-msi-dmg-
installer-support.patch` should produce (the same single-file pristine-extraction-plus-patch-
chain method every entry in this file already uses to verify a new patch), the chain came up
short: the real working tree's `post_build_commands.py` has `_wrap_windows_msi`,
`_wrap_macos_dmg`, `_sanitize_msi_version`, and the `--deb-package-name`/`--deb-version` →
`--package-name`/`--package-version` rename — none of which `0027`'s patch file, as
committed, actually contains. `git log` on that one patch file found two commits touching it:
`b5d2283` ("Add Windows installer template for roves-bundle using WiX" — the commit that
actually introduced this feature) and, much later, `9ad5b8d` ("Refactor boot splash screen:
resize, recenter, and redesign progress bar" — a commit with no business touching the msi/dmg
installer at all). `git diff b5d2283 9ad5b8d -- patches/servo-v0.4.0/0027-*.patch` confirms
it: `9ad5b8d` shrank the patch from 437 lines to 160, losing everything except a small later
`output_dir = path.abspath(...)` correction (referenced in the entry above this one). The
actual source (`post_build_commands.py` itself) was never affected — only the patch file
meant to reproduce it, almost certainly regenerated at the time with `git diff` against the
wrong base (e.g. the previous commit instead of the pre-`0027` state) and overwriting the
correct file instead of replacing just that one small hunk. This is exactly the silent-drift
failure mode `CLAUDE.md`'s "keep patches up to date" section warns about, and exactly why:
`.github/workflows/test.yml` downloads a pristine tag and applies every patch fresh on every
run — a truncated `0027` would have quietly built a bundle *missing* `--msi`/`--dmg` support
entirely the next time this project's Servo version gets bumped and this patch needs
reapplying, with nothing else here to catch it in the meantime (the working tree itself looks
completely correct; only the patch — the thing that matters for the *next* upgrade — was
wrong).

**Fix:** restored `patches/servo-v0.4.0/0027-add-msi-dmg-installer-support.patch` from
`b5d2283`'s (correct, complete) version, then re-applied the later `path.abspath` correction
on top by hand (a one-line change, easy to redo safely) and regenerated the patch from an
actual before/after diff rather than editing the unified-diff text directly. Net effect: same
437-ish lines as originally committed, plus the abspath fix folded in as part of the same
patch instead of silently replacing it.

**Verification:** rebuilt the full chain from a pristine `v0.4.0` extraction of just
`python/servo/post_build_commands.py` — `0004`, `0013`, `0014`, `0015`, `0016`, `0017`,
`0023`, `0026`, this corrected `0027`, `0031`, `0033` (the diagnostic-script entry below) — in
order, and the result is now byte-identical to the real working tree. Before the fix, the same
process reproducibly diverged (missing the msi/dmg methods entirely); after it, it matches.

---

## 2026-08-14 — CI actually launches the bundle it just built, instead of only building it

**File:** `.github/workflows/test.yml`. No upstream location — this workflow doesn't exist
upstream at all (see `CLAUDE.md`), so there's no `patches/` entry for it; it's edited directly
in this repo like any other Roves-only file.

**Why:** every job in this workflow, until now, only ever confirmed `mach bundle` *succeeds*.
That gap is exactly how the `--package-name`/`--package-version` launch-args leak (see the two
entries above) went unnoticed through *multiple* green runs of this same workflow: every real
double-click of the resulting `play.exe` crashed instantly, while CI stayed green throughout,
because nothing here had ever actually run the binary. A build succeeding and a build
launching are different claims, and only the first one was being tested.

**Change:** two new steps, inserted between "assemble test bundle" and the zip steps, run
*after* every matrix entry's bundle is assembled (portable, `--msi`, `--dmg`, `--deb` alike —
whichever binary the earlier "add-msi-dmg" bug above just confirmed still ships loose inside
`release/` for every mode, not only portable):

- **Linux/macOS** (one bash step, `if: matrix.os_name != 'windows'`): installs `xvfb` (Linux
  only — macOS runners already have a real window server even without a physical display),
  launches `release/play` or `release/Roves.app/Contents/MacOS/Roves` in the background,
  waits 10 seconds, and checks the process is *still running* — the same "did a window
  survive past argument parsing and GL/window setup" signal a human tester would look for.
  Captures stdout/stderr to files and prints them regardless of outcome, then searches
  `~/.cache` (or `~/Library/Caches` on macOS) for the newest `roves.log` and prints that too.
  Fails the job (`exit 1`) if the process exited on its own within the 10 seconds.
- **Windows** (one `pwsh` step): the same check via `Start-Process -PassThru` +
  `-RedirectStandardOutput`/`-RedirectStandardError`, searching `%LOCALAPPDATA%` for the
  newest `roves.log`.

Deliberately not a pixel-perfect check — no screenshot, no window-content assertion, just "is
the process still alive a few seconds in." That's intentional: it's exactly the granularity
needed to catch a crash-before-a-window-ever-appears (this bug's exact shape) without needing
a display-comparison harness, and it works identically whether or not the runner has a real
display attached.

**Not part of the `roves-action` sync:** CI-only tooling, not a `mach bundle` CLI surface
change.

**Verification:** `test.yml` is plain YAML + bash/pwsh, not part of the `patches/` mechanism
(see "File" above) — nothing to apply-check here. The change was pushed alongside the
diagnostic-script entry below and exercised for real by the resulting CI run; see that run's
outcome for whether the new steps themselves behave as intended (a smoke test that never
actually ran isn't verified by reading its own YAML).

**Correction (same day, after that real CI run):** the first version of this step failed on
4 of 6 matrix entries — worth recording in detail since each was a distinct bug, not one
underlying cause:

- **`set -e` was swallowing the diagnostics this step exists to produce.** GitHub Actions
  bash steps run with `errexit`. In the "already exited" branch, `wait "$PID"` returns that
  process's (non-zero, since it already exited) status as a bare statement — not inside an
  `if`/`while` condition or an `&&`/`||` chain — which is exactly the case bash's `-e` treats
  as fatal: the whole step aborted right there, before ever reaching the stdout/stderr dump,
  the `roves.log` search, or this step's own `::error::` message. Every failure showed up as
  a bare "Process completed with exit code N" with none of the diagnostics the step was
  written to produce — undermining the entire point of adding it. Fixed with an explicit
  `set +e` once the binary's existence is confirmed (kept `set -e` for the setup steps
  before that, where a hard stop on unexpected failure is still correct).
- **`--deb` doesn't produce a loose `release/play` at all.** Unlike `--msi`/`--dmg` (which
  wrap the same portable output), `_bundle_linux_deb` builds a real Debian package —
  `/usr/lib/<package_name>/`, `/usr/bin/<package_name>` symlink — that only exists once
  actually installed. The step assumed the portable layout unconditionally; fixed by
  `sudo dpkg -i release/*.deb` first, then testing `/usr/bin/servoshell-test` — the same
  binary a player actually installing the `.deb` would end up running.
- **`--msi`/`--dmg` *also* don't leave a loose binary in `release/`** — `bundle()` deletes
  `stage_dir` (where the portable `play.exe`/`Roves.app` was built) right after wrapping it,
  so `release/` ends up containing only the `.msi`/`.dmg` itself. This one is genuinely new
  behavior compared to every earlier CI run: those were unknowingly building from the
  truncated `0027` (see that fix entry above), which never actually defined `--msi`/`--dmg`/
  `--package-name` as recognized flags at all — meaning every prior "msi"/"dmg" CI job was
  silently producing a *plain portable build* (the unrecognized flags landing in `params`
  and `launch.json`, exactly the leak this whole investigation started from) while reporting
  success. With the corrected patch, `--msi`/`--dmg` now do what they were always supposed to,
  and this step needed updating to match: the Windows step now runs `msiexec /i ... /quiet
  INSTALLDIR=...` and looks for `play.exe` under the actual install directory; the macOS step
  `hdiutil attach`s the `.dmg` and points `BIN` at the mounted volume's `Roves.app` (detached
  again at the end, best-effort).

Net effect: this step wasn't just fixed, it went from silently never having tested a working
`--msi`/`--dmg` build to being the first thing that actually does.

---

## 2026-08-14 — Optional `diagnose.bat`/`diagnose.sh` shipped alongside the bundle

**File:** `python/servo/post_build_commands.py` — new `_DIAGNOSE_BAT`/`_DIAGNOSE_SH` string
constants and `_write_diagnostic_script` function (both module-level, next to
`_write_launch_config`), a new `--diagnostic-script` flag on `bundle`, and one new call site
in `bundle()` itself, right after `_place_bundle_content`.

**Why:** the same "no console on a double-clicked Windows build" problem the file-logging
entry above exists for has a second half: even with `roves.log` now capturing everything, a
non-technical tester asked to "try launching it and tell me what happens" still has no way to
*see* that log, or the process's exit code, without being walked through finding
`%LOCALAPPDATA%` by hand. A script that launches the game from a console that stays open
afterward — printing the exit code and the log's contents inline — turns "nothing happened"
into something a tester can screenshot or copy-paste directly into a bug report.

**Change:** `bundle`'s new `--diagnostic-script` flag (off by default — a real shipped release
has no reason to carry engine-internal debug tooling players never asked for) writes
`diagnose.bat` (Windows) or `diagnose.sh` (macOS/Linux, `chmod +x`'d) into `stage_dir` — the
same directory `play.exe`/`play`/`Roves.app` itself sits in, and, critically, the directory
that `--msi`/`--dmg` wrap wholesale into their installer (see the "single-executable-bundle"
and "add-msi-dmg" entries) — so the script ships inside those installed outputs too, not just
the plain portable one. Deliberately **not** written for `--deb`: a `.deb` install runs from
`/usr/bin` via a normal terminal that already shows stdout/stderr directly, so the script would
have nothing to add there. The script itself: runs the game binary directly (not backgrounded,
not killed after a timeout — a real tester should be able to actually play/close it normally),
then prints the exit code, a "this looks like a launch failure" callout if it was non-zero,
and the contents of whatever `roves.log` is newest under the platform's cache root (found by
`ls -t`/`Get-ChildItem | Sort LastWriteTime`, not a hardcoded path — robust to the game's own
name, which is what that directory is named after). Ends with `pause` (Windows) so
double-clicking doesn't instantly close the summary before anyone reads it.

**`.github/workflows/test.yml`:** `assemble test bundle`'s `BUNDLE_ARGS` now always includes
`--diagnostic-script`, so every CI-built test bundle exercises this path (and the new smoke-
test entry above incidentally proves the script itself gets written and is executable, since
it sits right next to the binary the smoke test launches).

**`roves-action` sync:** `--diagnostic-script` is a new, real `mach bundle` CLI flag (unlike
the CI-only smoke-test entry above), so per `CLAUDE.md`'s "keep `roves-action` in sync"
section, synced in this same turn (the sibling checkout was present): a matching
`diagnostic-script` input added to `action.yml` (same `[roves]`-tagged, `default: 'false'`
pattern as `deb`/`msi`/`dmg`), forwarded into the `mach bundle` invocation right after the
`deb`/`msi`/`dmg` block, and a matching row added to `README.md`'s input reference in the
same position.

**Patch:** `patches/servo-v0.4.0/0033-optional-diagnostic-launch-script.patch`

**Verification:** `ast.parse`-checked `post_build_commands.py`'s syntax (clean; no
`mach`-capable Python in this sandbox, same gap as earlier entries), and the patch was
verified the same way every other one in this file is: applied cleanly on top of a pristine
`v0.4.0` extraction with patches `0001`–`0031` already applied (see the "add-msi-dmg" fix
entry above for the corrected `0027` this depends on), producing a result byte-identical to
the actual working tree. `_DIAGNOSE_BAT`/`_DIAGNOSE_SH`'s literal string content was extracted
via `ast.literal_eval` and hand-inspected against the intended behavior. Actually wrote and ran
`diagnose.bat` against a real downloaded bundle (the fixed portable one from the entry above):
its own logic — banner, `%LOCALAPPDATA%` log search, exit-code branch, final messaging — all
executed correctly, but the bare `play.exe` line inside it failed to launch in this specific
sandbox (`ERRORLEVEL 9009`, "not recognized") even though the exact same `play.exe`, in the
exact same directory, launches fine when invoked directly (via `Start-Process`/a backgrounded
shell command) — reproduced across three different invocation methods (a raw `cmd /c`, a
PowerShell background job, and a plain `Start-Process` file-association launch mirroring a
real double-click), ruling out a mistake in any one test harness. Given `cd /d "%~dp0"` then a
bare sibling `.exe` is the single most standard, universally-supported batch pattern there is,
this reads as a sandbox-specific restriction on spawning an arbitrary named executable from a
`cmd.exe` child process specifically (as opposed to a directly-invoked `Start-Process`), not a
defect in the script — but stated plainly rather than silently assumed: **not confirmed
working end-to-end on a real, unrestricted Windows machine.** Not done: an actual
`mach bundle --diagnostic-script` run confirming `diagnose.bat`/`diagnose.sh` show up in a
real bundle and behave as written — the next real Windows/`mach`-capable build (the same CI
run testing the smoke-test entry above) should confirm this.

**Update:** the smoke-test entry above's real CI run confirmed this works — Windows portable
and `--msi` both ran `diagnose.bat` successfully once written, no separate fix needed.

---

## 2026-08-14 — macOS portable output renamed `Roves.app` → `play.app`, and a real Steam-dylib crash it exposed

**File:** `python/servo/post_build_commands.py` (`_bundle_macos`, `_wrap_macos_dmg`,
`bundle`'s docstring), `support/content-packer/src/main.rs` (one comment).

**Why the rename:** requested directly — `Roves.app` as the *portable bundle's own file
name* read as an odd, arbitrary choice next to Windows/Linux's neutral `play.exe`/`play`.
Worth being explicit about the tension this creates, surfaced and confirmed before making
this change: `README.md`'s own "Naming" section documents, as a *deliberate* decision from
the 2026-08-07 Servo→Roves rename, that every player/OS-facing label — window title,
taskbar/dock identity, the Linux `.desktop` entry, and (until now) the macOS `.app` bundle
name — should say "Roves". Renaming the bundle to `play.app` walks back that one piece of
it, trading "matches window-title/taskbar branding" for "matches the neutral `play`
placeholder every other platform already uses." Confirmed this trade-off explicitly rather
than assuming it — the answer was to proceed with `play.app` anyway. `README.md`'s Naming
section is updated to describe the new, narrower scope of that "says Roves everywhere"
claim (window title/taskbar/`.desktop`, not the portable binary/bundle name on any
platform).

**Change:** `_bundle_macos`: bundle folder `Roves.app` → `play.app`, the binary inside
`Contents/MacOS/Roves` → `Contents/MacOS/play`, and `Info.plist`'s
`CFBundleExecutable`/`CFBundleName` (must match the actual filename) → `"play"`.
`CFBundleIdentifier` untouched — still deliberately `org.servo.servoshell.bundle`, per the
existing comment above it explaining why that one needs its own separate decision.
`_wrap_macos_dmg`'s docstring and `bundle`'s own docstring updated to match. Also fixed:
`.github/workflows/test.yml`, `README.md`, `roves-action`'s `README.md`/`action.yml`, and
`roves-wiki`'s docs pages — every place describing the macOS output by name.

**A real bug this rename's own verification found:** re-deriving the patch for this change
meant actually running the real CI-built macOS bundles (both `portable` and `dmg`) end to
end for the first time (the smoke-test entry above only started doing this the same day) —
and both crashed instantly:

```text
dyld[81813]: Library not loaded: @loader_path/libsteam_api.dylib
  Referenced from: .../release/Roves.app/Contents/MacOS/Roves
  Reason: tried: '.../release/Roves.app/Contents/MacOS/libsteam_api.dylib' (no such file)
```

`ports/servoshell/build.rs` links every macOS dylib to be found via
`-rpath @executable_path/lib/`, and `_bundle_macos` accordingly copies every `.dylib` it
finds into a `lib/` subdirectory next to the binary — correct for Servo's own dependencies,
but `steamworks-sys` links `libsteam_api.dylib` with a hardcoded
`@loader_path/libsteam_api.dylib` install name instead (Valve's own SDK convention, not
something this fork's build script controls), and `@loader_path` for the main executable
means "flat, directly next to the binary" — not `lib/`. Every macOS `--features steam`
build has been broken this way since the feature was added; nothing had ever actually
launched one before now (see the smoke-test entry above for why that gap existed at all).

**Fix:** `_bundle_macos` now special-cases `libsteam_api.dylib` — copied flat into
`Contents/MacOS/` alongside the binary, removed from the list that goes into `lib/`. Every
other dylib is unaffected.

**Not part of the `roves-action` sync:** neither change affects `mach build`/`mach bundle`'s
CLI surface (no flag added/removed/renamed) — `action.yml`/README already just say
`play.app` generically enough not to need updating for the rename, and the dylib fix is
purely internal to what `_bundle_macos` copies where.

**Patch:** `patches/servo-v0.4.0/0034-rename-macos-bundle-to-play-app-and-fix-steam-dylib.patch`

**Verification:** `ast.parse`-checked `post_build_commands.py`'s syntax — clean. The patch
was verified the same way as every other one in this file: applied cleanly on top of a
pristine `v0.4.0` extraction with patches `0001`–`0033` already applied, producing a result
byte-identical to the actual working tree for both changed files. The libsteam_api fix
itself is confirmed by the failure this entry quotes above (the exact crash it's meant to
fix) — not yet re-confirmed working with a fresh CI run at the time of writing this entry;
that run is what will actually prove it (or not).

**Outcome:** confirmed fixed. The follow-up CI run came back green on all 6 matrix jobs —
including both macOS ones, which is the actual proof: the smoke-test step (this file's own
entry above) now genuinely runs a `--features steam` macOS build for the first time, and it
stayed up instead of crashing on the `libsteam_api.dylib` load failure this entry describes.

---

## 2026-08-14 — Boot splash was showing upstream Servo's own icon, not Roves' — CI never actually copied ours in

**Files:** `.github/workflows/test.yml` (one new `cp`), `ports/servoshell/desktop/gui.rs`
(`update_splash`, and the doc comment above `SPLASH_WORDMARK_FONT_SIZE`).

**Reported directly:** a real, freshly-built (same-day) Windows portable bundle's boot splash
showed a small green/teal/blue circular icon next to the "Roves" wordmark — not the
wolf-and-chains mark this project's actual branding uses everywhere else (the wiki, `icon.svg`
at this repo's root). Worth walking through how this got tracked down, since the actual
cause turned out to be nothing like the first hypothesis:

**First hypothesis (wrong): the committed icon file itself is stale/wrong.** Reading
`resources/servo_64.png` (what `gui.rs`'s `load_splash_icon_image` embeds via
`include_bytes!`) directly showed what looked, at a glance, like a dark, spiky, unfamiliar
creature — nothing like the wolf-chain mark. Concluded from this that the file itself must
still be some leftover upstream Servo asset, despite a 2026-08-13 entry above claiming it was
already regenerated from `icon.svg` and pixel-verified.

**Actually checking, rather than trusting a quick look:** upscaling `resources/servo_64.png`
with *nearest-neighbor* (no smoothing, so fine detail survives) showed it clearly *is* the
wolf-chain mark — the "unfamiliar creature" impression was just how illegible 209 individual
vector paths' worth of fine detail (teeth, fur, chain links) becomes once flattened to 64
pixels and viewed small. Same check against `resources/servo.ico`'s embedded 256px frame:
also correctly the wolf-chain mark. The 2026-08-13 entry's claim was right after all — these
*committed* files were never the problem.

**So why did a real build show something else entirely?** Asked the user directly whether
the screenshot came from a build made with current code (yes) — meaning the discrepancy was
real, not stale local state, and the committed files being correct meant the *build process*
had to be looking somewhere else. `.github/workflows/test.yml`'s "download + patch Servo
source" step downloads a **pristine** upstream Servo zip and applies this repo's text
patches on top — and, same as the `resources/fonts/` Metal Mania font before it, binary
assets like `resources/servo_64.png`/`servo.ico` can't be carried by a text patch at all.
Unlike `resources/fonts/`, which *does* get an explicit `cp` in that step, nobody had ever
added the equivalent copy for the icon rasters. Checked what fills that gap: extracted
`resources/servo_64.png` from the actual pristine `v0.4.0` zip this workflow downloads —
upstream Servo ships its own file at that exact same path (unsurprising; this repo's file
naming was never changed from Servo's own convention) — and it is, byte for byte, the exact
green/teal/blue circular mark from the screenshot. `include_bytes!`/`build.rs`'s icon
embedding happily compiled against *upstream's* file the whole time, silently baking in the
wrong icon instead of failing to build at all — exactly the kind of silent divergence between
the committed tree and what CI actually reconstructs that the `0027`-patch-truncation entry
above already surfaced once this same day.

**Fix:** one line added right after the existing `resources/fonts/` copy in `test.yml`:
`cp ../resources/servo_64.png ../resources/servo_1024.png ../resources/servo.ico
../resources/servo.icns resources/`. Of these, only `servo_64.png` (boot splash) and
`servo.ico` (Windows `.exe` icon, via `build.rs`) are actually referenced by any code path
today — `servo_1024.png` and `servo.icns` aren't wired up anywhere yet (the macOS `.app`'s
`Info.plist` has no `CFBundleIconFile` at all, a separate, pre-existing gap not addressed
here), but copying all four now means whichever of them *does* get wired up later won't
silently hit this exact same bug again.

**Separately, sizing:** also asked to make the icon and wordmark closer to the same visual
size and vertically centered against each other. The old code hardcoded
`SPLASH_ICON_SIZE = 128.0` against `SPLASH_WORDMARK_FONT_SIZE = 88.0` — a 128:88 (≈0.69)
ratio carried over from `resources/roves_wordmark.svg`'s own icon:font-size lockup, which
doesn't necessarily hold for how tall "Roves" actually renders in the Metal Mania font *at
this specific size*, since font em-size and rendered glyph height aren't the same thing.
`update_splash` now measures the wordmark's actual rendered size once (`egui::Context::
fonts_mut`'s `layout_no_wrap(...).size()`, the same technique the existing width measurement
already used, just also reading `.y` now) and sizes the icon to match that measured height
directly, instead of trusting an independently-guessed constant — removing
`SPLASH_ICON_SIZE` entirely. The half-height vertical-centering offset (previously a
hardcoded `87.0` fudge factor tied to the old 128px assumption) is now computed from the same
measurement too, so it stays correct regardless of exactly how tall the wordmark renders.

**Not part of the `roves-action` sync:** neither change touches `mach build`/`mach bundle`'s
CLI surface.

**Patch:** `patches/servo-v0.4.0/0035-fix-boot-splash-icon-and-size-icon-to-match-wordmark.patch`
(`gui.rs` only — the `test.yml` copy-step fix isn't part of the `patches/` mechanism, same
reasoning as every other `test.yml`-only entry in this file).

**Verification:** `rustfmt --edition 2024 --check` on `gui.rs` — clean except one pre-existing,
unrelated diff at line 504 (confirmed pre-existing by checking the unmodified `HEAD` version
of the file reports the identical diff). The new `self.context.egui_ctx.fonts_mut(...)` call
(made before `self.context.run(...)`, rather than via the closure's `ctx` argument as the
existing width measurement did, since `self.context.run` already holds `self.context`
mutably) mirrors an identical pattern already used elsewhere in this same file
(`self.context.egui_ctx.memory_mut(...)`, also called outside `.run()`) — not a novel,
unverified API usage. The patch was verified the same way as every other one in this file:
applied cleanly on top of a pristine `v0.4.0` extraction with patches `0001`–`0030` already
applied, producing a result byte-identical to the actual working tree. Not done: an actual
`cargo check`/`mach build` (this sandbox has no buildable local toolchain for the full
`servoshell` dependency graph in reasonable time) or a real screenshot of the rebuilt splash
confirming the icon now shows correctly and reads as proportionate — the next real
Windows/`mach`-capable CI run is what will actually confirm both fixes.

**Correction (same day, after that CI run): the sizing fix as first written crashed every
platform on launch.** 5 of 6 matrix jobs (everything except `linux-deb`, which happens not to
exercise this code path the same way) failed the smoke-test entry above's own check, with
`roves.log`/stderr showing:

```text
No fonts available until first call to Context::run() (thread main, at .../egui-0.34.3/src/context.rs:1103)
```

— an `egui` internal panic (SIGSEGV/abort downstream of it, exit code 139). The measurement
this entry moved to `self.context.egui_ctx.fonts_mut(...)`, called *before*
`self.context.run(...)`, turns out to be exactly the one thing `egui::Context::fonts_mut`
doesn't allow: fonts aren't initialized until the *first* `run()` call, ever, for that
context — measuring anything font-related has to happen *inside* the closure passed to
`run()`, not before it. The original code already knew this (its own width measurement was
always inside the closure); moving it out to dodge the `self` double-borrow (constructing
`icon`, which needs `&self.splash_icon_texture`, while `self.context.run(...)` already holds
`self.context` mutably) was the wrong way to resolve that borrow-checker problem.

**Fix:** resolved the *actual* borrow conflict instead — `self.splash_icon_texture.clone()`
(cheap; `TextureHandle` is a small ref-counted handle, confirmed via its actual docs, not
assumed) into a plain local variable *before* `self.context.run(...)`, which the closure can
freely capture without touching `self` at all. Both the wordmark measurement and `icon`'s
construction moved back inside the closure, using that cloned local instead of `self`
directly. Net result: same measure-don't-guess sizing this entry originally set out to add,
just with the measurement (and everything depending on it) happening at the only point
`egui` actually allows it.

**Patch:** regenerated `patches/servo-v0.4.0/0035-fix-boot-splash-icon-and-size-icon-to-match-wordmark.patch`
in place (from pristine `v0.4.0` + `0001`–`0030`, not as a second patch stacked on the
broken version — the broken version never should have shipped as-is, so there's no reason to
preserve it as a separate reviewable step). Re-verified the same way: applies cleanly,
byte-identical result. Still not done, same gap as above: an actual `mach build`/real launch
— but this time backed by a concrete, specific egui-internals reason the previous "not yet
confirmed" version was actually wrong, not just an unverified guess repeated twice.
