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
