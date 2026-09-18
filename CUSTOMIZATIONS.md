# Customizations over upstream Servo

Baseline: Servo `v0.5.0` (<https://github.com/servo/servo/archive/refs/tags/v0.5.0.zip>) â€” see
the 2026-09-04 migration entry below for the upgrade from the previous `v0.4.0` baseline, and
for why the entries below it still say `patches/servo-v0.4.0/...`: those are the historical
record of *why* and *when* each change was made, not stale documentation to fix â€” only the
`Patch:` file each one names has moved (see the migration entry's mapping table).

This file lists every deviation from that pristine upstream source, in the order they were
made. See `CLAUDE.md` for why this file exists and the protocol for keeping it current â€”
short version: **add an entry here in the same turn you change a file under `servo/`.**

Each entry: file path, upstream location the change replaces, what changed, why, and the
matching patch file under `patches/servo-v<tag>/` that makes the change mechanically
reproducible (see `CLAUDE.md` â€” `.github/workflows/servo-test-build.yml` applies those
patches to a fresh pristine download on every run once it's actually running somewhere, so
they must stay in sync with reality).

---

## 2026-09-04 â€” Migrate baseline from Servo v0.4.0 to v0.5.0

**Files:** effectively the whole tree â€” see the mapping table below for exactly which files
landed in which new patch.

**Patch:** all of `patches/servo-v0.5.0/` (new). `patches/servo-v0.4.0/` is left in place as a
historical record of the v0.4.0-era patch boundaries, but is no longer applied by anything â€”
`test.yml`/`android.yml`/`release.yml` all now read `SERVO_TAG=0.5.0`.

**Why re-derive instead of replaying the old 61 patches:** replaying `patches/servo-v0.4.0/`
in sequence against a fresh v0.5.0 checkout cascades â€” once one patch fails to apply (upstream
moved the code it targets), every later patch touching the same file fails too, even when its
own hunk would've applied fine on its own. Used a per-file 3-way merge instead
(`git merge-file`, base = pristine v0.4.0, ours = this repo's fully-patched HEAD, theirs =
pristine v0.5.0), which reconciles upstream's evolution against our full accumulated
customization set for a file in one step, regardless of how many old patches touched it. Of
68 originally-patched files: 38 merged with zero conflicts (including
`python/servo/post_build_commands.py`, despite it being touched by ~15 different old
patches), 18 are ours outright (no upstream counterpart to merge against), 1 was a rename
(`org.servo.Servo.desktop` â†’ `org.roves.Roves.desktop`) already reflected identically on both
sides, and 12 had genuine conflicts (upstream and our own patches touching the exact same
region) resolved by hand â€” most notably the Android app's full View-XML-to-Jetpack-Compose UI
rewrite (`MainActivity.kt`), `IDBIndex` gaining `SetName`/an internal representation change
that had to be reconciled with our own client-side cursor support, and `app.rs`'s
Servo-builder/webxr-registration reordering merged into this fork's own much larger
boot-splash/packed-content rewrite of the same function.

**New patch numbering â€” consolidated by subsystem, not preserved 1:1:** the old 61 patches
split by *logical change*, which meant many of them touched the same few files
(`post_build_commands.py`, `app.rs`, `headed_window.rs`, `gui.rs`) repeatedly and
cumulatively â€” exactly what made sequential replay so fragile. The new 14 patches split by
*subsystem/directory* instead: a file only ever appears in one patch, so a future upgrade only
ever needs the per-file 3-way merge above, never a sequential replay. Mapping from old patch
numbers (prefix only, see `patches/servo-v0.4.0/` for exact filenames) to new:

Built by re-checking, for each of the 61 old patches, exactly which files its own diff
touched (`grep "^diff --git" patches/servo-v0.4.0/*.patch`) and which new group each of those
files landed in â€” not a rough guess, an exhaustive cross-reference:

| New patch | Old patch #s (v0.4.0) | Covers |
| --- | --- | --- |
| `0001-desktop-shell-core` | 0001,0003,0005,0006,0010,0011,0012,0013,0015,0016,0017,0018,0019,0020,0021,0022,0023,0024,0025,0026,0028,0029,0030,0032,0035,0037,0043,0044,0046,0047,0049,0053,0055,0058 | `ports/servoshell/desktop/{app,bundle_launch,cli,dialog,event_loop,gui,headed_window,logging,mod,tracing,webxr}.rs`, `ports/servoshell/{build,main,panic_hook,parser,prefs,Cargo.toml}`, Windows exe manifest |
| `0002-desktop-protocols` | 0005,0006,0015,0024,0045,0047,0048,0053,0056,0058 | `ports/servoshell/desktop/protocols/*.rs` |
| `0003-content-packer` | 0014,0015,0017,0018,0024,0026,0030,0034 | `support/content-packer/*` (new crate) |
| `0004-android` | 0060,0061 | `support/android/apk/**` (Gradle, manifest, `MainActivity.kt`) |
| `0005-windows-packaging` | 0027,0041 | `mach.bat`, `support/windows/roves-bundle.wxs.mako` |
| `0006-media-gstreamer` | 0038,0043 | `python/servo/gstreamer.py` |
| `0007-build-tooling` | 0002,0004,0013,0014,0015,0017,0023,0026,0027,0031,0033,0034,0036,0039,0040,0042,0043,0047,0049,0051,0052,0060,0061 | `python/servo/{build_commands,post_build_commands}.py` |
| `0008-indexeddb` | 0054 | `components/script/dom/indexeddb/*`, `IDBIndex.webidl`, codegen `Bindings.conf` |
| `0009-svg-currentcolor-and-layout` | 0057 | `svgsvgelement.rs`, `components/layout/{context,replaced,traversal}.rs`, `components/shared/layout/lib.rs` |
| `0010-canvas-offscreencanvas-font` | 0059 | `canvas_state.rs`, `canvasrenderingcontext2d.rs` |
| `0011-storage-and-origin` | 0007,0008,0009,0020,0050,0053,0057 | `origin.rs`, `client_storage.rs`, `globalscope.rs`, `window/{history,window}.rs`, `components/config/prefs.rs`, `net/fetch/methods.rs` |
| `0012-servo-core` | 0015,0030,0039,0053 | `components/servo/{lib,servo}.rs` |
| `0013-resources-and-branding` | 0013,0019 | `.gitattributes`, the `.desktop` rename (text-diffable part only â€” see below) |
| `0014-root-workspace` | 0014,0058 | root `Cargo.toml` (workspace members, `sysinfo` dependency) |

(Several old patches appear against multiple new ones â€” e.g. `0015` touched files across
`servo/`, `servoshell/`, `protocols/`, `post_build_commands.py`, and `content-packer/` all in
one patch, so it shows up in five rows above. That fan-out is exactly why file-level, not
change-level, grouping was chosen this time: a file only ever needs reconciling once, no
matter how many old logical changes had accumulated in it.)

**Two real bugs the migration itself surfaced** (both already fixed on `main`/whichever branch
carries this, see the branch's own commit history for the exact fix commits â€” noted here so a
*future* upgrade doesn't have to rediscover the same failure modes):

1. **A clean 3-way merge can still silently drop an import your own code still needs.** The
   auto-merge for `components/layout/traversal.rs` dropped `use style::data::ElementData;`
   without any conflict â€” upstream removed it because *its own* code in that file stopped
   needing it, and our side never touched that particular line, so the removal applied
   cleanly. But our `svg_color_is_stale` function (added by the old `0057` patch, now folded
   into `0009-svg-currentcolor-and-layout`) still needs that exact type, in a different hunk
   the merge tool has no way to connect to the import line. Caught by an actual `cargo build`
   failure (`error[E0425]: cannot find type 'ElementData' in this scope`), not by anything
   inspectable from the diff alone â€” **a per-file 3-way merge reduces how much manual
   patch-reapplication is needed, it doesn't replace an actual build as the real
   correctness check.**
2. **`cp -r` on Windows loses non-Windows file properties `git diff`/`patch` still care
   about.** Doing the file-tree swap with a plain recursive copy (there's no `rsync` on this
   Windows machine) lost the executable bit on ~39 files (git tracks it; NTFS has no concept
   of it) and dereferenced 3 OpenHarmony icon "symlinks" (git tracks them as
   `120000`/symlink-type blobs whose content is just the relative target path; a Windows
   `cp` instead copies the *referenced file's actual bytes* as a plain `100644` file) â€” both
   invisible until diffed against pristine with `git diff`, which reports them as a mode/type
   change respectively, not as content changes `cargo build` would ever catch. Fixed via
   `git update-index --chmod=+x` for the former and `git checkout <pre-migration-ref> --`
   for the latter (git already had the correct, small text-placeholder content on file from
   before the migration touched it).

**Binary/text-placeholder assets â€” still not part of any patch, same as every prior entry
below that says so:** a unified diff can't carry new binary content, so these have never gone
through the patch mechanism at all, migration or not â€” carry them over by hand (`cp`, same as
`test.yml`'s/`android.yml`'s own copy steps already do) whenever they don't already exist
byte-for-byte in the new checkout:

- `resources/fonts/{MetalMania-OFL.txt,MetalMania-Regular.ttf}` â€” boot-splash wordmark font.
- `resources/{servo_64.png,servo_1024.png,servo.ico,servo.icns,servo.svg}` â€” the
  Roves-branded icon, replacing upstream's own file at each of these exact paths.
- `resources/roves_wordmark.svg` â€” boot-splash wordmark asset (not read by any Rust code, so
  its absence doesn't break a build, only what the splash actually displays).
- `support/openharmony/{AppScope,entry/src/main}/resources/base/media/servo_{64,1024}.png` â€”
  plain-text files containing a relative path back to the two `resources/servo_*.png` above
  (Roves' Windows-compatible stand-in for what upstream tracks as a real git symlink); **also
  a type change, not just new binary content**, so it hits the exact same "can't be a text
  patch" wall even though the content itself is a short text string.

**Verification:** `mach build --features steam` succeeds from a clean checkout of this
branch (confirmed via a throwaway CI workflow building directly from the branch, not through
the reconstruction path above â€” see that workflow's own comment for why). The reconstruction
path itself (this entry's actual patches, applied to a fresh pristine v0.5.0 download the way
`test.yml`/`android.yml` do) has not yet been separately verified end-to-end as of this
entry â€” do that before considering this migration fully done, not just "the tree we hand-built
compiles."

**Correction (2026-09-10):** this caveat was justified â€” the reconstruction/patch set silently
lost real content the direct-branch-build verification above could never have caught. See the
2026-09-10 entries near the end of this file for two concrete cases found so far: `0011-storage-
and-origin.patch` was missing 14 of 18 "default-on experimental prefs" (shipped broken in
v0.4.5â€“v0.4.11), and `0004-android.patch` is missing `servoapp/build.gradle.kts` entirely (only
surfaced by `android.yml`'s own patch-reconstruction build, not by `roves-action`'s
direct-checkout build). Treat every file this migration touched as suspect until independently
re-diffed against pristine, not just re-compiled.

---

## 2026-08-05 â€” Remove toolbar and tab strip UI entirely

**File:** `ports/servoshell/desktop/gui.rs`, in `Gui::update` (was line ~392-579 in the
`v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0001-remove-toolbar-and-tabs.patch`

**Upstream behavior:** the top toolbar (back/forward/reload/stop, address bar, experimental
prefs toggle) and the tab strip (tab list, new-tab button, new-window button) were drawn
inside `if winit_window.fullscreen().is_none() { ... } else { *toolbar_height = Length::default(); }`
â€” i.e. shown in windowed mode, hidden only when the OS window itself was in fullscreen.

**Change:** replaced the entire `if/else` block with a single unconditional statement:

```rust
// Kiosk/embedded fork: never draw the toolbar or tab strip, in windowed
// mode or fullscreen â€” this build is meant to look like a native app
// window, not a browser.
*toolbar_height = Length::default();
```

All of the removed block's code (toolbar buttons, address bar, tab strip, new-tab/new-window
buttons) was deleted, not just made unreachable â€” there is no dead code left behind.

**Why:** this build is meant to present as a normal native application window (like a Tauri
app), not as a browser with tabs â€” regardless of whether the window is fullscreen or not.
Upstream only hid this UI in fullscreen; the project's requirement is to hide it always.

**Side effects to know about when upgrading:** the `location` and `location_dirty` fields
destructured from `Self` at the top of `Gui::update` are no longer used anywhere in that
function (their only call sites were inside the removed block). This produces two harmless
`unused variable` warnings â€” not errors (`servoshell`'s `Cargo.toml` has no
`deny(warnings)`/`forbid(warnings)`). Not fixed further since it's cosmetic; revisit if this
crate ever turns warnings into errors.

---

## 2026-08-06 â€” Skip GStreamer DLL packaging on Windows when media is disabled

**File:** `python/servo/build_commands.py`, `run_post_build_tasks` (Windows branch) and
`copy_windows_dlls_to_build_directory` (was line ~197-347 in the `v0.4.0` baseline).

**Patch:** `patches/servo-v0.4.0/0002-skip-gstreamer-dll-copy-when-media-disabled.patch`

**Upstream behavior:** after a successful build, `run_post_build_tasks` packages
platform-specific runtime files. On macOS this is correctly gated on `self.enable_media`
(only copies GStreamer dylibs if the media stack is actually enabled). On Windows, the
equivalent call â€” `copy_windows_dlls_to_build_directory` â†’ `package_gstreamer_dlls` â€”
runs unconditionally regardless of `self.enable_media`, and hard-fails the whole build if
`servo.platform.get().gstreamer_root(...)` can't find a GStreamer install.

**Change:** `copy_windows_dlls_to_build_directory` now takes an `enable_media: bool`
parameter (passed as `self.enable_media` from the call site) and only calls
`package_gstreamer_dlls` when it's true, mirroring the existing darwin branch. ANGLE and
MSVC DLL copying are unaffected â€” those aren't GStreamer-related and still run
unconditionally.

**Why:** `../.github/workflows/test.yml` builds with `--media-stack dummy` specifically to
avoid needing GStreamer installed at all on CI (see that file's comments â€” installing it on
Windows would require an interactive UAC prompt that hangs forever on a GH runner). But this
upstream inconsistency meant the ~25-minute Windows compile still failed at the very last
step, in the post-build DLL-copy phase, with "Could not find GStreamer installation
directory." â€” independent of `--media-stack dummy` and independent of `--skip-platform`
during bootstrap. Without this fix, Windows CI can never pass while GStreamer bootstrap is
intentionally skipped.

---

## 2026-08-06 â€” Strip dead browser-navigation state and the favicon pipeline from `Gui`

**File:** `ports/servoshell/desktop/gui.rs`.

**Patch:** `patches/servo-v0.4.0/0003-strip-dead-browser-navigation-state-and-favicon-pipeline.patch`

**Upstream behavior:** even after the 2026-08-05 toolbar/tab removal (above), `Gui` kept
computing full browser-chrome state every frame with no remaining consumer:
`update_location_in_toolbar` (address-bar text), `update_load_status` (spinner/dirty-flag
bookkeeping), `update_can_go_back_and_forward` (back/forward button state), and
`load_pending_favicons`/`embedder_image_to_egui_image` (decoding each `WebView`'s favicon and
uploading it to a GPU texture, cached in `favicon_textures`). All four were only ever read by
the toolbar/tab-strip drawing code that patch 0001 deleted â€” actual back/forward navigation
(`Alt`+arrow keys etc.) calls `WebView::go_back`/`go_forward` directly in
`headed_window.rs`, bypassing `Gui` entirely, so none of this tracking gates any real
behavior.

**Change:** removed `update_location_in_toolbar`, `update_load_status`,
`update_can_go_back_and_forward`, `load_pending_favicons`, and
`embedder_image_to_egui_image` entirely, along with the `load_status`, `can_go_back`,
`can_go_forward`, and `favicon_textures` fields on `Gui` and the `load_pending_favicons`
call site in `Gui::update`. `update_webview_data` (called once per frame from
`headed_window.rs`) now just calls the one function that still has a real consumer â€”
`update_status_text`, which feeds the hover-status tooltip â€” instead of OR-ing together four
functions' change-flags.

The `location: String` and `location_dirty: bool` fields were **left in place** rather than
removed: after this change they're written once in `Gui::new` from the `_initial_url`
constructor parameter and never read again, which the compiler fully accounts for (same
"harmless unused" category the toolbar-removal patch already documented for these two
fields â€” see 2026-08-05 above) â€” with no runtime loop touching them anymore, removing them
would require threading a signature change through `create_platform_window` â†’
`HeadedWindow::new` â†’ `Gui::new` across `app.rs` and `headed_window.rs` for zero behavioral
or binary-size benefit. The constructor parameter was renamed to `_initial_url` to document
that it's now unused for this purpose, without changing the parameter list itself (so
`headed_window.rs`'s call site didn't need touching).

**Deliberately left alone:** `browser_tab` and `toolbar_button` (the tab-strip widget and
toolbar-button helper) are dead code with zero callers anywhere in the crate â€” unlike the
functions above, they don't run every frame, they simply never run at all, so the compiler
already drops them from the linked binary in release builds. Removing the source would be
pure cosmetics with no effect on the shipped game package, so they were left as-is.

**Why:** this fork's `Gui` no longer draws any browser chrome (see 2026-08-05 above), so this
is the second half of that same cleanup: the *state* that only existed to feed that chrome
was tracked here too, and kept running every frame â€” recomputing back/forward capability,
diffing load status, and (worst of all) decoding and uploading a GPU texture per `WebView`
favicon â€” for a UI element that no longer exists. None of it is "dead code" the compiler can
optimize away, since `Gui::update` genuinely calls it every frame; it's live, wasted CPU/GPU
work in every build of the game, embedded or otherwise.

**Side effects to know about when upgrading:** if a future upstream Servo version changes how
`WebView::status_text`/`load_status`/`can_go_back`/`can_go_forward`/`favicon` are exposed,
re-check this patch still applies cleanly â€” it's a bigger diff than 0001 and touches more of
`Gui`'s internals. Not verified against an actual `./mach build` (Servo's build is
multi-hour and wasn't run for this change) â€” treat the next real build of this fork as the
actual verification and fix up any compile errors this patch introduces before relying on it.

---

## 2026-08-06 â€” New `mach bundle` command: package a build into something runnable

> **Superseded (2026-08-08):** the per-platform launcher/hidden-core-binary shape this
> entry describes below (`bin/servoshell.exe` + `play.exe`, `<binary>-core` + `play.sh`/a
> bash script) is no longer current â€” see the "Single-executable bundle" entry near the
> end of this file. Kept as-written for its still-accurate motivation/history; don't use
> its per-platform bullet list as a description of what `mach bundle` produces today.

**File:** `python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0004-add-mach-bundle-command.patch`

**Upstream behavior:** `./mach build` leaves the raw Cargo output in `target/<profile>/` â€”
just the engine binary plus (on Windows) ANGLE/MSVC DLLs dropped next to it by
`copy_windows_dlls_to_build_directory`. Running that binary bare opens Servo's own default
start page, not this fork's intended content, and on Windows it has no window-size/URL args
set, so it isn't something a non-technical person can be handed and told to double-click.

**Change:** added a new command, `./mach bundle [--html-file dist/index.html]
[--window-size 1280x720] [--output DIR] [--content-dir DIR] [--deb] [-- extra servoshell
args]`, category `post-build`. It does **not** touch `target/<profile>/` or move the binary
Cargo put there â€” `./mach run` and everything else that calls `get_binary_path()` keeps
working exactly as before. Instead, into a separate output directory (default:
`target/<profile>/bundle/`) it produces, per platform:

- **Windows:** the engine binary + its DLLs moved into a `bin/` subdirectory, plus a real
  `play.exe` at the top level â€” a tiny std-only Rust program, compiled on the fly with plain
  `rustc` (no Cargo project), built with `#![windows_subsystem = "windows"]` (the same
  attribute `ports/servoshell/main.rs` *itself* sets, as of the 2026-09-10 fix below â€” this
  entry previously and incorrectly claimed that was already true from the start; it wasn't,
  see that entry) so double-clicking it never flashes a console. It just spawns
  `bin/servoshell.exe` with the configured args and exits.
- **macOS:** a minimal `Servo.app` bundle (`Contents/Info.plist` + `Contents/MacOS/Servo`, a
  small shell script that `exec`s the engine binary â€” renamed `<binary>-core` and tucked
  inside the bundle â€” with the configured args). Finder launches
  `Contents/MacOS/Servo` directly; no `Terminal.app` involved at all, unlike double-clicking
  a loose `.sh`.
- **Linux (default):** the engine binary renamed `<binary>-core` **without its executable
  bit**, plus a `play.sh` that `chmod +x`'s it, sets `LD_LIBRARY_PATH`, and execs it with the
  configured args. `play.sh` is the only supported entry point; a curious
  `./<binary>-core` fails with "Permission denied" instead of launching without the
  args/`LD_LIBRARY_PATH` it actually needs.
- **Linux with `--deb`:** a real, installable `.deb` instead (`<name>_<version>_<arch>.deb`,
  built via `dpkg-deb --build --root-owner-group`) â€” engine + content under
  `/usr/lib/<name>/`, a launcher at `/usr/bin/<name>`, and a `.desktop` entry so it shows up
  in application launchers. Requires `dpkg-deb` (from `dpkg-dev`) on `PATH`; raises
  `BuildNotFound` with a clear message if missing rather than a bare traceback. This is a
  functional package, not a lintian-clean one â€” no changelog, man page, or maintainer
  scripts.

`--content-dir` (e.g. a built `dist/`) is copied into the bundle at whatever relative
location `--html-file` expects it (default `dist/index.html` â†’ a `dist/` next to the
launcher, or under `/usr/lib/<name>/` for `--deb`) â€” see `_place_bundle_content`. Passing it
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
UX gap this command fixes â€” it doesn't consume this command yet because it doesn't build
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
ever looks in a `lib/` subdirectory next to the binary â€” flat placement meant any dylib
dependency (GStreamer today if `--media-stack gstreamer`, Steamworks below) would silently
fail to load at runtime despite bundling successfully. Fixed to copy into
`Contents/MacOS/lib/`, matching the same convention `run_post_build_tasks`'s own darwin
branch already uses for `package_gstreamer_dylibs` (`path.join(path.dirname(built_binary),
"lib/")`).

---

## 2026-08-06 â€” `steam:` protocol bridge, so web content can reach Steamworks

**Files:** `ports/servoshell/Cargo.toml`, `ports/servoshell/build.rs`,
`ports/servoshell/desktop/app.rs`, `ports/servoshell/desktop/protocols/mod.rs`, new file
`ports/servoshell/desktop/protocols/steam.rs`.

**Patch:** `patches/servo-v0.4.0/0005-add-steam-bridge.patch`

**Upstream behavior:** Servo has no notion of Steamworks; web content has no way to reach
any native SDK beyond what `ProtocolHandler`s already expose (the `servo:`/`resource:`/
`urlinfo:` schemes registered in `app.rs`, see `protocols/servo.rs`).

**Change:** added a `steam` Cargo feature (`dep:steamworks`, desktop-only â€” same
`not(any(target_os = "android", target_env = "ohos"))` dependency block `headers`/
`serde_json` already live in) that, when enabled, registers a new `steam:` custom protocol
handler alongside the existing ones in `app.rs`. `SteamProtocolHandler::new()` calls
`steamworks::Client::init()` once at registration time (spawning the same 100ms
`run_callbacks()` pump thread the Tauri build's `steam::try_init()` already does) and
degrades to "Steam unavailable" answers rather than failing when Steam isn't running â€”
mirrors the parent project's `src-tauri/src/steam.rs` **command-for-command** (achievements,
int/float stats, DLC check, overlay, store page), so `src/lib/steam.ts` in the parent
project can expose the *exact same* JS API regardless of which native shell (Tauri or
Roves) is running the game: `fetch('steam:unlock_achievement?achievement_id=ACH_X')` instead
of `invoke('steam_unlock_achievement', { achievementId: 'ACH_X' })`, both landing on the same
underlying Steamworks call. See `src/lib/steam.ts`'s own `callRoves()`/`isSteamSupported()`
for the JS-side half of this bridge (outside this `servo/` directory â€” that's the parent
project's own file, not part of this fork's patches). Notably there's no build-time "is
steam enabled" flag on the JS side: an earlier version baked one in via a `STEAM_ENABLED`
env var, but that could drift from whether *this particular binary* actually has the feature
compiled in â€” `isSteamSupported()` asks the real running binary instead (per-shell: `invoke`
on Tauri, `callRoves` on Roves), and caches the answer.

`build.rs` additionally copies the Steamworks redistributable (`steam_api64.dll` /
`libsteam_api.dylib` / `libsteam_api.so`) from steamworks-sys's own `OUT_DIR` into
`target/<profile>/`, right next to the built binary â€” mirrors `src-tauri/build.rs`'s
equivalent copy (which lands the same file in `src-tauri/` for Tauri's bundler instead).
Landing it in `target/<profile>/` means `./mach bundle` (see the 2026-08-06 entry above)
picks it up for free through the DLL/dylib/so glob it already has, no extra plumbing needed
there â€” only guarded by `CARGO_FEATURE_STEAM`, since build scripts don't get
`#[cfg(feature = ...)]` applied to their own compilation.

**Why:** shipping on Steam (achievements, cloud stats, overlay) is a stated goal for this
fork regardless of which native shell a given release uses â€” the Tauri build already had
this wired up; Roves had no equivalent path to any native API at all, since it's meant to
run untouched web content with zero JS-side awareness of which shell it's in beyond the
existing `__EMBEDDED_TARGET__` build flag.

**Side effects to know about when upgrading:** `ProtocolHandler` (`components/net/protocols/
mod.rs`) is a stable, low-level trait uninvolved in most of Servo's churn â€” this should
survive a version bump untouched. If a future Servo version changes `ServoUrl`'s API
(`as_url()`/`query_pairs()`), or `steamworks-rs` cuts a new major version with a different
`Client`/`UserStats` surface, re-check `steam.rs`'s `handle_command` still matches.

**Follow-up (2026-08-07) â€” CI now actually builds `--features steam`:** the "not verified"
gap above is closed: `../.github/workflows/test.yml`'s `mach build` step now passes
`--features steam` on all 3 platforms, so every push exercises `steamworks-sys` actually
compiling/linking and `build.rs`'s `copy_steam_lib` finding and copying the Steamworks
redistributable (`steam_api64.dll`/`libsteam_api.*`) next to the binary â€” this workflow is
still dormant today (see its own header comment on why), so this only takes effect once
`servo/` is pushed as its own top-level repo. No Steam client is available on any CI runner
either way, so this still only proves the build/link step, not a real `Client::init()`
success â€” `handle_unavailable`'s degrade path is what runs there regardless.

**Follow-up (2026-08-06) â€” mixed-content blocking on `fetch('steam:...')`:** manual testing
surfaced `TypeError: Network error: Blocked as mixed content` on a `fetch("steam:is_available")`
call. Servo's mixed-content check (`components/net/fetch/methods.rs`'s
`should_request_be_blocked_as_mixed_content`, via
`components/net/protocols/mod.rs::is_url_potenµ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥çF–ÆÇ•÷G'W7Gv÷'F‡–’G&VG27W7FöÒ66†VÖR0§G'W7Gv÷'F‡’öæÇ’–b—G2&÷Fö6öÄ†æFÆW#£¦—5÷6V7W&R‚–&WGW&ç2G'VV(	B7FVÕ&÷Fö6öÄ†æFÆW& ¦öæÇ’÷fW'&öFR—5öfWF6†&ÆR‚–†æVVFVBf÷"F—&V7BÂæöâÖæòÖ6÷'6fWF6‚‚–66W72’æBÆVg@¦—5÷6V7W&R‚–B—G2FVfVÇBfÇ6VÂF†R6ÖRv&÷Fö6öÇ2÷W&Æ–æfòç'6w0¦W&Ä–æfõ&÷Fö6öÄ†æFW&Ç&VG’fö–G2'’÷fW'&–F–ær&÷F‚âf—†VB'’FF–æp¦fâ—5÷6V7W&R‚g6VÆb’Óâ&ööÂ²G'VRÖæW‡BFò—5öfWF6†&ÆV–â7FVÕ&÷Fö6öÄ†æFÆW&À¦föÆFVB–çFòF6†W2÷6W'fò×cãBãóRÖFB×7FVÒÖ'&–FvRçF6†‡&VvVæW&FVBF†Rv†öÆP¦æWrÖf–ÆR‡Væ²f÷"7FVÒç'6&F†W"F†â†æBÖVF—F–ærF–fbÖöbÖÖF–fb’âF†R6ÖRvW†—7FV@¦–â&÷fW5&÷Fö6öÄ†æFÆW&†&÷fW3¦Â6VRF†RVçG'’&VÆ÷r’æBv2f—†VBF†W&RFöòÂWfVà§F†÷Vv‚—B†FâwB&VVâW†W&6—6VB–WB'’ÖçVÂFW7F–ær(	B6ÖRG&—BÂ6ÖRÖ—76–ær÷fW'&–FRà ¢ÒÒĞ ¢22##bÓ‚Ób(	B&÷fW2r÷vâvVæW&Â×W'÷6R–çfö¶R‚–'&–FvR†&÷fW3¦&÷Fö6öÂ’²G&–æ72÷&÷fW2Ö–  ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öWfVçEöÆö÷ç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2öÖöBç'6ÂæWrf–ÆP¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2÷&÷fW2ç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷G&6–ærç'6†FFV@£##bÓ‚ÓbÂ6VR4’Öf–ÇW&Ræ÷FR&VÆ÷r’âÇW2Â÷WG6–FRF†—26W'fòöF—&V7F÷'’†æ÷@§F6‚×G&6¶VB(	B6VRF†R$TDÔRæÖBöW†×ÆW2ò&V6VFVçB&÷fR“¢F†RæWr&÷fW2Ö’ö6¶vP¦BF†R&Wò&ö÷BÂæB7&2öÆ–"ö†öö·2÷V—BÖ†öö·2çG6ö7&2öÆ–"÷7FVÒçG6–âF†R&VçB&ö¦V7Bà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóbÖFB×&÷fW2Ö–çfö¶RÖ'&–FvRçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢æòWV—fÆVçB(	BF†—2—2æWrgVæ7F–öæÆ—G’Âæ÷BÖöF–f–6F–öâö`¦W†—7F–ærW7G&VÒÆöv–2à ¢¢¤6†ævS¢¢¢FFVB&÷fW3¦7W7FöÒ&÷Fö6öÂ†æFÆW"†&÷fW5&÷Fö6öÄ†æFÆW&ÂÇv—0§&Vv—7FW&VB(	BVæÆ–¶R7FVÓ¦Âæ÷BfVGW&RÖvFVB’ÂF†R&÷fW2WV—fÆVçBöbFW&’w2•3 §vV"6öçFVçB6ÆÇ2fWF6‚‚w&÷fW3£Æ6öÖÖæCãóÆ&w3âr–ÂæBvWG2¥4ôâ&W7VÇB&6²âFöF’—@¦ç7vW'2W†7FÇ’öæR6öÖÖæBÂW†—Fö6Æ÷6U÷v–æF÷vÂv†–6‚6Æ÷6W2WfW'’÷Vâv–æF÷p¢†6W'fõ6†VÆÅv–æF÷s£§66†VGVÆUö6Æ÷6V’(	B–âF†—2f÷&²w2W7VÂ6–ævÆR×v–æF÷r6WGWÂF†Bw0¦WV—fÆVçBFòV—GF–ærF†RÂ6–æ6R£§V×÷6W'fõöWfVçEöÆö÷&WGW&æ–ærfÇ6Vöæ6Ræğ§v–æF÷w2&VÖ–âÖ¶W2F†RWfVçBÆö÷W†—Böâ—G2÷vâ†ç'6’à ¥F†R–çFW&W7F–ær'B—2¦†÷r¢—B&V6†W2F†Rv–æF÷rBÆÃ¢&÷Fö6öÄ†æFÆW&–×ÆVÖVçF÷'0¦×W7B&R6VæB²7–æ6æB&R–çfö¶VBöfbF†RÖ–âF‡&VB†fWF6†W2'VâöâæWGv÷&²ô”ğ§F‡&VG2’Â'WB'Vææ–æt7FFVö6W'fõ6†VÆÅv–æF÷v&R&6Ö&6VB(	BFVÆ–&W&FVÇ§6–ævÆR×F‡&VFVBÂÖ–â×F‡&VBÖöæÇ’G—W2â&÷Fö6öÂ†æFÆW"6âwBF÷V6‚F†VÒF—&V7FÇ’âF†P¦f—‚&WW6W2ÖV6†æ—6ÒF†—26öFV&6RÇ&VG’†Bf÷"W†7FÇ’F†—26†Röb&ö&ÆVÓ ¦†VFVDWfVçDÆö÷v¶W&†WfVçEöÆö÷ç'6’Ç&VG’v¶W2F†RÖ–âF‡&VBg&öÒ÷F†W"F‡&VG2f–¦&3Ä×WFWƒÄWfVçDÆö÷&÷‡“ÄWfVçCããæ²v–æ—Fw2÷vâ7&÷72×F‡&VB×6fRWfVçDÆö÷&÷‡–à¦&÷fW5&÷Fö6öÄ†æFÆW&†öÆG2F†R6ÖR¶–æBöb†æFÆR†'V–ÇBg&öÒw2÷và¦WfVçEöÆö÷÷&÷‡–f–VÆBB&Vv—7G&F–öâF–ÖRÂæöæV–â†VFÆW72ÖöFR’æB6VæG2æWp¦WfVçC£¤6Æ÷6TÆÅv–æF÷w6f&–çBF‡&÷Vv‚—C²£§W6W%öWfVçF†Ç&VG’F†RÆ6P¦WfVçF2vWB†æFÆVBöâF†RÖ–âF‡&VB’ÖF6†W2öâ—BæB6ÆÇ266†VGVÆUö6Æ÷6R‚–öà¦WfW'’v–æF÷r–â'Vææ–æt7FFS£§v–æF÷w2‚–à ¤÷WG6–FR6W'fòö¢æWrçÒ6¶vRÂ¢¦G&–æ72÷&÷fW2Ö–¢¢‡&Wò&ö÷B&÷fW2Ö’öÀ§&Vv—7FW&VB2âçÒv÷&·76R(	B6VRF†R&ö÷B6¶vRæ§6öæ’Âw&2F†—2–â¥2v—F‚¦6÷&Ræ–çfö¶R†6ÖBÂ&w2–6†VBW†7FÇ’Æ–¶RFW&’Ö2ö’ö6÷&Vw2–çfö¶R‚–ÂÇW2¦&ö6W72æW†—B‚–‡F†R&÷fW2WV—fÆVçBöbFW&’Ö2÷ÇVv–â×&ö6W76w2W†—B‚–’æBgVÆÀ¦7FVÖÖöGVÆR‡FÆ¶–ærFò7FVÓ¦F—&V7FÇ’Âæ÷BF‡&÷Vv‚&÷fW3¦(	B6VRF†RVçG'’&÷fRf÷ §v‡’7FVÒvWG2—G2÷vâFVF–6FVB&÷Fö6öÂ’â—Bw2&VÂÂ–æFWVæFVçB–×ÆVÖVçFF–öâÂæ÷B§6†–Ò÷fW"FW&’w2'VçF–ÖR(	BFVÆ–&W&FVÇ’6†VBFòfVVÂfÖ–Æ–"Âæ÷F†–ærÖ÷&RâF†R&Vç@§&ö¦V7Bw27&2öÆ–"ö†öö·2÷V—BÖ†öö·2çG6æ÷r6ÆÇ2G&–æ72÷&÷fW2Ö’÷&ö6W76w2W†—B‚–öà¥&÷fW2–ç7FVBöbv–æF÷ræ6Æ÷6R‚–‡Vç&VÆ–&ÆRF†W&S¢67&—FVBv–æF÷ræ6Æ÷6R‚–—2öæÇ¦w&çFVBöâv–æF÷w2F†RvR—G6VÆb÷VæVBf–v–æF÷ræ÷Vâ‚–Âæ÷B6†VÆÂÖ7&VFVBF÷ÖÆWfVÀ§v–æF÷r’ÂæB7&2öÆ–"÷7FVÒçG6–6·2&WGvVVâFW&’Ö–çfö¶R‚–Ö&6VB7FVÔ– ¦–×ÆVÖVçFF–öâæBG&–æ72÷&÷fW2Ö’÷7FVÖw2÷vâÂöæ6RÂBÖöGVÆRÆöB(	B&÷F‚6öæf÷&ÒFğ§F†RW†7B6ÖR7FVÔ––çFW&f6RW‡÷'FVBg&öÒF†R6¶vRà ¢¢¥v‡“¢¢¢6Æ÷6–ær÷V—GF–ærF†R—2W†7FÇ’F†R¶–æBöb&6–2æF—fR6&–Æ—G’WfW'¦VÖ&VFFVB6†VÆÂæVVG2ÂæBv–æF÷ræ6Æ÷6R‚–FöW6âwB&VÆ–&Ç’&÷f–FR—B‡6VR&÷fR’(	B&÷fW0¦†Bæòv’FòFòF†—2BÆÂ&Vf÷&Râ'V–ÆF–ær—B26ÖÆÂÂvVæW&–2–çfö¶R‚–×6†V@¦'&–FvR‡&F†W"F†âöæRÖöfb6Æ÷6S¦&÷Fö6öÂ’v—fW2&÷fW2&ööÒFòw&÷rÖ÷&R&6öçG&öÂF†—0¦"6öÖÖæG2F†R6ÖRv’ÆFW"Â–ç7FVBöb67V×VÆF–æröæR&W7ö¶R&÷Fö6öÂW"fVGW&Rà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢F†R7&÷72×F‡&VB†æFÆRGFW&à¢†&3Ä×WFWƒÄWfVçDÆö÷&÷‡“ÄWfVçCããæ’FWVæG2öâv–æ—Fw2WfVçDÆö÷&÷‡–7F––æp¦6VæFö7–æ6×6fRæBöâ'Vææ–æt7FFS£§v–æF÷w2‚–ö6W'fõ6†VÆÅv–æF÷s£§66†VGVÆUö6Æ÷6V ¦¶VW–ærF†V—"7W'&VçB6–væGW&W2(	BÆÂV"†7&FR–ÂÆ÷rÖÆWfVÂÂæBæ÷BW7G&VÒÖ6‡W&â×&öæRÀ¦'WB&RÖ6†V6²–bfW'6–öâ'V×6†ævW2ç'6w2WfVçBÖÆö÷7G'V7GW&RÖFW&–ÆÇ’à ¢¢¤föÆÆ÷r×Wƒ##bÓ‚Ób’(	BÖ—†VBÖ6öçFVçB&Æö6¶–æs¢¢¢&÷fW5&÷Fö6öÄ†æFÆW&†BF†R6ÖP¦—5÷6V7W&R‚–v27FVÕ&÷Fö6öÄ†æFÆW&(	B6VRF†BVçG'’w2föÆÆ÷r×W&÷fRf÷"F†R&ö÷@¦6W6Râf—†VBF†R6ÖRv’†FFVBfâ—5÷6V7W&R‚g6VÆb’Óâ&ööÂ²G'VRÖ’ÂföÆFVB–çFğ¦F6†W2÷6W'fò×cãBãóbÖFB×&÷fW2Ö–çfö¶RÖ'&–FvRçF6†à ¢¢¤föÆÆ÷r×Wƒ##bÓ‚Ób’(	B4’Ö6öæf—&ÖVB'V–ÆB'&V³¢¢¢F†R&æ÷BfW&–f–VBv–ç7Bâ7GVÀ¦'V–ÆB"&—6²æ÷FVB&÷fRÖFW&–Æ—¦VC¢4’f–ÆVBv—F‚W'&÷%´SEÓ¢æöâÖW††W7F—fRGFW&ç3 ¢gv–æ—C£¦WfVçC£¤WfVçC£¥W6W$WfVçB„WfVçC£¤6Æ÷6TÆÅv–æF÷w2’æ÷B6÷fW&VF–à¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷G&6–ærç'6w2ÆöuF&vWBf÷"v–æ—C£¦WfVçC£¤WfVçCÄWfVçCæ–×À¢‡v2Æ–æRãC"Óc–âF†RcãBã&6VÆ–æR’(	BâW††W7F—fRÖF6‚6VÆf÷fW"WfW'¦WfVçFf&–çBF†Bv6âwBWFFVBv†Vâ6Æ÷6TÆÅv–æF÷w6v2FFVB&÷fRâf—†VB'¦FF–ær6VÆc£¥W6W$WfVçB„WfVçC£¤6Æ÷6TÆÅv–æF÷w2’ÓâF&vWB‚%W6W$WfVçB„6Æ÷6TÆÅv–æF÷w2’"’Æ ¦Æöæw6–FRF†RW†—7F–ærv¶W&ö66W76–&–Æ—G–&×2ÂföÆFVB–çFğ¦F6†W2÷6W'fò×cãBãóbÖFB×&÷fW2Ö–çfö¶RÖ'&–FvRçF6†2âFF—F–öæÂ‡Væ²&F†W"F†â§6W&FRF6‚f–ÆRÂ6–æ6R—Bw2'BöbF†R6ÖRÆöv–6Â6†ævR‡F†—2f–ÆRv26–×Ç’Ö—76V@§F†Rf—'7BF–ÖR’âæò&V†f–÷"6†ævR&W–öæBÖ¶–ærF†RÖF6‚W††W7F—fRv–â(	BG&6–ærç'6 ¦öæÇ’ffV7G2%U5EôÄôvf–ÇFW&–ærw&çVÆ&—G’ÂæWfW"7GVÂWfVçB†æFÆ–ærà ¢ÒÒĞ ¢22##bÓ‚Ób(	B7F&ÆRf–ÆS¢òö÷&–v–âÂ6ò6öçFVçB÷VæVBg&öÒF—6²7GVÆÇ’ÆöG0 ¢¢¤f–ÆS¢¢¢6ö×öæVçG2÷W&Âö÷&–v–âç'6Â–Ö×WF&ÆT÷&–v–ã£¦æWuö÷VUöf÷%öf–ÆV‡v2Æ–æP£ƒbÓ“"–âF†RcãBã&6VÆ–æR’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãór×7F&ÆRÖf–ÆRÖ÷&–v–âÖf÷"ÖÖöGVÆR×67&—BÖÆöF–ærçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢æWuö÷VUöf÷%öf–ÆR‚–Ö–çG2'&æBÖæWr&æFöÒWV–C£¦æWu÷cB‚–öà¦WfW'’6ÆÂÂv—F‚æò66†–ær†6W'fõW&Ã£¦÷&–v–â‚–Â6ö×öæVçG2÷W&ÂöÆ–"ç'3£“Ó“&Â6ÆÇ0¦–Ö×WF&ÆT÷&–v–ã£¦æWr‚–(	BæBF†W&Vf÷&RF†—2(	Bg&W6‚WfW'’F–ÖR’âGvòæ÷&–v–â‚–6ÆÇ2öà§F†R¦W†7B6ÖR¢f–ÆS¢òöU$Â&RF†W&Vf÷&RæWfW"WVÂFòV6‚÷F†W"ÂÆWBÆöæRGvğ¦F–ffW&VçBf–ÆS¢òöU$Ç2âÖçVÂFW7F–ær‡6VRDôDòæÖFw2'66†W&ÖF&–æ6"w&—FWW§&W&öGV6VBF†R6öç6WVVæ6RF—&V7FÇ“¢æ÷&ÖÂf—FR'V–ÆB†W‡FW&æÀ¦Ç67&—BG—SÒ&ÖöGVÆR"7&3Ò"âââ#æÂF†RFVfVÇB÷WGWB6†Rf÷"ç—F†–ær7B§6–ævÆRÖf–ÆRF÷’’÷VæVBg&öÒf–ÆS¢òöU$Â&VæFW'2&Ææ²vRâ&ö÷B6W6S¢F†R…DÔÀ¦ÖöGVÆR×67&—B7V2Çv—2fWF6†W2W‡FW&æÂÖöGVÆR67&—G2–â4õ%2ÖöFR&Vv&FÆW72öbF†P¦7&÷76÷&–v–æGG&–'WFRÂæB4õ%2ÖöFRw26ÖRÖ÷&–v–â6†V6°¢†6ö×öæVçG2öæWBöfWF6‚öÖWF†öG2ç'3£SCrÓSC†À¦¦÷&–v–âÓÒ&WVW7Bæ7W'&VçE÷W&Å÷v—F…ö&Æö%ö6Æ–Ò‚’æ÷&–v–â‚–’6âæWfW"7V66VVBf÷ ¦f–ÆS¢òöv—fVâF†R&÷fR(	BF†RfWF6‚fÆÇ2F‡&÷Vv‚FòÖWF†öG2ç'3£S“vw0¦æWGv÷&´W'&÷#£¥Vç7W÷'FVE66†VÖVÂF†RVçG'’67&—BæWfW"'Vç2ÂæBF†RvR7F—2W†7FÇ’0§F†R…DÔÂ'6W"ÆVgB—Bâ†fWF6‚‚–FòF†—2f÷&²w2÷vâ7FVÓ¦ö&÷fW3¦&÷Fö6öÇ2—0§VæffV7FVB(	BF†÷6R&RÖ&¶VB—5öfWF6†&ÆR‚–Âv†–6‚W‡Æ–6—FÇ’'—76W2F†—26ÖRÖ÷&–v–à¦6†V6²(	B'WBÆ–âfWF6‚‚–öbf–ÆS¢òöU$ÂÂ÷"ç’÷F†W"4õ%2ÖÖöFRf–ÆS¢òö&WVW7BÀ§v÷VÆB†—BF†R6ÖRvÆÂâ ¢¢¤6†ævS¢¢¢æWuö÷VUöf÷%öf–ÆR‚–æ÷r&WGW&ç2f—†VB–B†WV–C£¦æ–Â‚–Âf–æWp¦d”ÄUôõ$”t”åô”F6öç7FçB’–ç7FVBöbg&W6‚&æFöÒöæRÂ6òWfW'’f–ÆS¢òö÷&–v–âF†—0¦'V–ÆBWfW"&öGV6W26ö×&W2WVÂFòWfW'’÷F†W"öæRà ¢¢¥v‡“¢¢¢F†—2'V–ÆBöæÇ’WfW"÷Vç2öæRf–ÆS¢òöFö7VÖVçB(	B—G2÷vâ'VæFÆV@¦F—7Bö–æFW‚æ‡FÖÆ(	BæBW‡÷6W2æòv’Fòæf–vFRFòç’÷F†W"f–ÆS¢òöU$Â†æòFG&W70¦&"ÂæòF'2Â6VRF†R##bÓ‚ÓRFööÆ&"×&VÖ÷fÂVçG'’’â$&RGvòf–ÆS¢òö÷&–v–ç2F†R6ÖP¦÷&–v–â"†2æò&VÂç7vW"–âF†R7V2—G6VÆb†÷VR÷&–v–ç2&R7W÷6VBFò&RvÆö&ÆÇ§Væ—VRÂ'WB6VRÆ‡GG3¢òöv—F‡V"æ6öÒ÷v†Gvrö‡FÖÂö—77VW2ó3““âf÷"F†R7FæF–ærÖ&–wV—G§7V6–f–6ÆÇ’&÷WBf–ÆS¢òö’ÂæBÖ–ç7G&VÒ'&÷w6W'2Ç&VG’ÆVâF÷v&BW6&–Æ—G’÷fW §7G&–7BVæ—VVæW72†W&R–â&7F–6Râf÷"F†—2f÷&²w2öæRÖFö7VÖVçBÖöæÇ’W6R66RF†W&R—2æğ§&VÆ—7F–2F÷vç6–FRFòÇv—2G&VF–ærf–ÆS¢òö2F†R6ÖR÷&–v–âÂæB—Bw2v†BÖ¶W2à¦÷&F–æ'’f—FR†÷"vV'6²ÂWF2â’'V–ÆB7GVÆÇ’ÆöBv†Vâ÷VæVBg&öÒF—6²ÂÖF6†–ær†÷r—@§v÷VÆB&V†fR6W'fVB÷fW"‡GG‡2“¢òöà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢F†—26†ævW2÷&–v–â¦WVÆ—G’¢f÷"f–ÆS¢òö ¦öæÇ’(	B—5÷÷FVçF–ÆÇ•÷G'W7Gv÷'F‡’‚–Ç&VG’7V6–ÂÖ66VBf–ÆS¢òö2G'W7Gv÷'F‡§&Vv&FÆW72öb–B‡6VRF†RÖ—†VBÖ6öçFVçBföÆÆ÷r×W&÷fR’Â6òF†B&V†f–÷"—2Væ6†ævVBà¥7F÷&vR†Æö6Å7F÷&vVÂWF2â’æBç’÷F†W"6ÖRÖ÷&–v–âÖ¶W–VB7FFRv÷VÆBæ÷r&R6†&V@¦7&÷72¦ÆÂ¢f–ÆS¢òöFö7VÖVçG2–âF†R6ÖR&ö6W72Âv†–6‚v÷VÆB&R&VÂ&Vw&W76–öâf÷ §7Fö6²6W'fò†vVæW&Â×W'÷6R'&÷w6–ærÂ×VÇF—ÆRVç&VÆFVBf–ÆS¢òövW2’'WB—2–æW'B†W&P¦v—fVâF†R6–ævÆRÖFö7VÖVçB6öç7G&–çB&÷fR(	B&Wf—6—BF†—2&V6öæ–ær–bF†—2f÷&²WfW"w&÷w0¦v’Fò÷VâÖ÷&RF†âöæRf–ÆS¢òöFö7VÖVçBâfW&–f–VBv—F‚6&vò6†V6²×6W'fò×W&Æ ¢†6ö×–ÆW26ÆVâ“²æ÷B–WBfW&–f–VBv–ç7Bâ7GVÂâöÖ6‚'V–ÆF²âöÖ6‚'VæFÆV°¦ÖçVÂ6Æ–6²×F‡&÷Vv‚‡6VRDôDòæÖF’(	BG&VBF†B2F†R&VÂfW&–f–6F–öâöbv†WF†W"F†—0¦7GVÆÇ’f—†W2F†R&Ææ²×vR&W&òVæBFòVæBà ¢ÒÒĞ ¢22##bÓ‚Ór(	B7F÷&vR66W72f÷"f–ÆS¢òö÷&–v–ç0 ¢¢¤f–ÆW3¢¢¢6ö×öæVçG2÷W&Âö÷&–v–âç'6Â6ö×öæVçG2÷67&—BöFöÒ÷v–æF÷r÷v–æF÷rç'6À¦6ö×öæVçG2÷67&—BöFöÒövÆö&Ç66÷RövÆö&Ç66÷Rç'6Â6ö×öæVçG2÷7F÷&vRö6Æ–VçE÷7F÷&vRç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó‚ÖÆÆ÷r×7F÷&vRÖf÷"Öf–ÆRÖ÷&–v–âçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢ÖçVÂFW7F–æröbââ÷FW7B×vRööâ&VÂ'V–ÆB‡6VRDôDòæÖFw0¦æ÷r×&VÖ÷fVB7F÷&vRf–æF–æw2’7W&f6VBÆö6Å7F÷&vVö6W76–öå7F÷&vVF‡&÷v–æp¦6V7W&—G”W'&÷#¢6ææ÷B66W72âââg&öÒ÷VR÷&–v–âæÂæB–æFW†VDD"æ÷Vâ‚–ğ¦æf–vF÷"ç7F÷&vRç·W'6—7BÇW'6—7FVBÆW7F–ÖFWÒ‚–&V¦V7F–ærv—F‚F†RWV—fÆVçBf÷"F†P§6ÖR&V6öã¢v–æF÷s£¤vWDÆö6Å7F÷&vVövWE6W76–öå7F÷&vVÂvÆö&Å66÷S£ ¦ö'F–å÷7F÷&vUö¶W–ÂæB6Æ–VçE÷7F÷&vRç'6w2ö'F–åööÆö6Å÷7F÷&vU÷6†VÆfV6‚&V¦V7Bç¦÷&–v–âF†B—6âwB–Ö×WF&ÆT÷&–v–ã£¥GWÆVÂæBWfW'’f–ÆS¢òöFö7VÖVçB–âF†—2f÷&²—0¦–Ö×WF&ÆT÷&–v–ã£¤÷VV‡6VRF†R##bÓ‚ÓbVçG'’&÷fR’âF†—2—2f—F†gVÀ¦–×ÆVÖVçFF–öâöbF†R7F÷&vR7FæF&Bô…DÔÂ&ö'F–âÆö6Â÷6W76–öâ7F÷&vR&÷GFÆRÖ ¦Æv÷&—F†×2Âæ÷B6W'fò×7V6–f–2'Vr(	Bf—&Vf÷‚&V¦V7G2f–ÆS¢òö7F÷&vRF†R6ÖRv“°¤6‡&öÖRw2÷vâ–çFW&æÂ÷&–v–âÖöFVÂ§W7BFöW6âwBföÆÆ÷rF†R7V2†W&RÂv†–6‚—2v‡’—BÆöö·0¦f–æRF†W&Rà ¢¢¤6†ævS¢¢¢æWr–Ö×WF&ÆT÷&–v–ã£¦6åö66W75÷7F÷&vR‚–†Ö—'&÷&VBöâ×WF&ÆT÷&–v–æ’–à¦÷&–v–âç'6¢6VÆbæ—5÷GWÆR‚’ÇÂ6VÆbæ—5öf–ÆUö÷&–v–â‚–âF†RF‡&VR6ÆÂ6—FW2&÷fP¢†v–æF÷rç'6w2Gvò7F÷&vRÖ&÷GFÆRÖÖ6†V6·2ÂvÆö&Ç66÷Rç'6w2ö'F–å÷7F÷&vUö¶W’‚–Âæ@¦6Æ–VçE÷7F÷&vRç'6w2ö'F–åööÆö6Å÷7F÷&vU÷6†VÆf’æ÷r6ÆÂF†—2–ç7FVBöb—5÷GWÆR‚–ö¦&Ææ¶WB÷VRÖ÷&–v–â6†V6²Â6òf–ÆS¢òöFö7VÖVçG27V6–f–6ÆÇ’&RW†V×FVBg&öÒF†P¥7F÷&vR7FæF&Bw2÷VRÖ÷&–v–â&W7G&–7F–öâ(	BÆö6Å7F÷&vVÂ6W76–öå7F÷&vVÀ¦–æFW†VDD&ÂæBæf–vF÷"ç7F÷&vVÆÂvòF‡&÷Vv‚öæRöbF†W6Rf÷W"6†V6·2âWfW'’÷F†W ¦÷VR÷&–v–â†FF¦Â&Æö#¦×v—F†÷WBÖ÷&–v–âÂWF2â’7F–ÆÂvWG2&V¦V7FVBW†7FÇ’2&Vf÷&Rà ¢¢¥v‡“¢¢¢&÷fW26†—2vÖW2ÂæBvV"vÖRv—F‚æòv’FòW'6—7B6fRFF—2†&@¦&Æö6¶W"Âæ÷Bæ–6R×FòÖ†fR(	BF†Rv†öÆRö–çBöbF†—2f÷&²—2FòÖ¶Râ÷&F–æ'’vV"vÖRw0¦W†—7F–ær6öFR§W7Bv÷&²âF†R6åö66W75÷7F÷&vR‚–W†V×F–öâFVÆ–&W&FVÇ’Ö—'&÷'0¦—5÷÷FVçF–ÆÇ•÷G'W7Gv÷'F‡’‚–w2W†—7F–ær—5öf–ÆUö÷&–v–æ6'fRÖ÷WB‡6ÖRf–ÆRÂFFVB–âF†P£##bÓ‚ÓbVçG'’&÷fR’&F†W"F†âÖ¶–ærf–ÆS¢òöGWÆR÷&–v–â÷WG&–v‡C¢F†B'&öFW ¦6†ævRv÷VÆBÇ6òÇFW"6öö¶–R7F÷&RÂ4õ%2÷6ÖRÖ÷&–v–âÂæBÖ—†VB6öçFVçB&V†f–÷"ÂæöæRö`§v†–6‚vW&R'&ö¶VâæBæöæRöbv†–6‚F†—26†ævRF÷V6†W2à ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢6åö66W75÷7F÷&vR‚–w26fWG’&W7G2öâF†P¦W†7B6ÖR6–ævÆRÖFö7VÖVçB77V×F–öâ2æWuö÷VUöf÷%öf–ÆR‚–w2÷&–v–â×7F&–Æ—G’6†ævP¢†ÆÂf–ÆS¢òöFö7VÖVçG26†&RöæRf—†VB÷&–v–âÂæBF†—2f÷&²öæÇ’WfW"†2öæR÷VâB§F–ÖR’(	B&R×fW&–g’F†B77V×F–öâ7F–ÆÂ†öÆG2&Vf÷&R&WW6–ærF†—2GFW&â–bF†—2f÷&²WfW ¦÷Vç2Ö÷&RF†âöæRf–ÆS¢òöFö7VÖVçBÂ÷"6V6öæBöæR6öæ7W'&VçFÇ’†FWgFööÇ2÷WÂ¦gWGW&R×VÇF’×v–æF÷rfVGW&RÂWF2â’âfW&–f–VBv—F‚6&vò6†V6²×6W'fò×W&Ææ@¦6&vò6†V6²×6W'fò×7F÷&vV†&÷F‚6ÆVâ’â6&vò6†V6²×6W'fò×67&—F†6÷fW'0¦v–æF÷rç'6övÆö&Ç66÷Rç'6’6÷VÆB¢¦æ÷B¢¢&R'Vâ–âF†R6æF&÷‚F†—2v2WF†÷&VB–â(	B—G0¦Ö÷¦§5÷7—6'V–ÆB67&—BæVVG2ÆÇfÒÖö&¦GV×Âv†–6‚—6âwB–ç7FÆÆVBF†W&S²F†—2—2§FööÆ6†–âv–âF†BVçf—&öæÖVçBÂæ÷B6öFR—77VRÂ'WB—BFöW2ÖVâF†Rv–æF÷rç'6ğ¦vÆö&Ç66÷Rç'6‡Væ·2&RVçfW&–f–VB'’ç’6ö×–ÆW"†W&Râ&÷F‚VF—G2&R6–ævÆP¦–fÖ6öæF—F–öâ7vW6–ærÖWF†öBF†BÇ&VG’6ö×–ÆW26÷'&V7FÇ’v–ç7BF†R6ÖRG—R–à¦6W'fò×W&ÆÂ6ò&—6²—2Æ÷rÂ'WBG&VBâ7GVÂ6&vò6†V6²×6W'fò×67&—FöâöÖ6‚'V–ÆF ¦2F†R&VÂfW&–f–6F–öâ&Vf÷&R6öç6–FW&–ærF†—2VçG'’6Æ÷6VBâæ÷B–WB&R×fW&–f–V@¦VæB×FòÖVæBv–ç7B&VÂ'V–ÆBV—F†W"v’‡6ÖR6fVB2F†R##bÓ‚ÓbVçG'’&÷fR’à ¢¢¤gWGW&R6öç6öÆW3¢¢¢æöæRöb3Bõ3Rõ†&÷‚õ7v—F6‚öWF2â‡6VR$TDÔRæÖFw2ÆFf÷&ÒF&ÆR’&P¦–×ÆVÖVçFVB–WBÂ'WBv†VâF†W’&RÂÆö6Å7F÷&vVö–æFW†VDD&&RfW'’VæÆ–¶VÇ’Fò&RF†P§&–v‡B6fRÖFF&6¶VæBF†W&RBÆÂ(	B6öç6öÆW2†fRF†V—"÷vâæF—fR6fRÖFF—0¢‡ÆFf÷&Ò×7V6–f–26fR6öçF–æW'2Â6Æ÷VB7–æ2Â7F÷&vRV÷F2F–VBFòF†Rõ2ÂWF2â’ÂæB¦vÖR6†—VBöâ&÷fW26†÷VÆBW6R§F†÷6R¢Âæ÷BâV×VÆFVBvV"7F÷&vR6†–ÒöâF÷ö`§v†FWfW"vVæW&–2f–ÆW7—7FVÒ66W72F†B6öç6öÆR÷'BVæG2Wv—F‚âF†—2VçG'’w2f—‚—0§66÷VBFòFW6·F÷f–ÆS¢òö7V6–f–6ÆÇ“²—B—6âwBÖVçBFò–×Ç’Æö6Å7F÷&vVö–æFW†VDD& §6†÷VÆB¶VW&V–ærF†R6fRÖFFF‚öæ6R6öç6öÆR÷'G2W†—7B(	BF†Bw26W&FRÀ§W"×ÆFf÷&Ò'&–FvR†6öæ6WGVÆÇ’F†R6ÖR6†R2F†R7FVÓ¦&÷Fö6öÂ'&–FvR&÷fS ¦æF—fR6öÖÖæB7W&f6RvV"6öçFVçB6ÆÇ2–çFòÂ&6¶VB'’v†FWfW"F†RÆFf÷&Ò7GVÆÇ¦öffW'2’F†B†6âwB&VVâFW6–væVB–WBà ¢ÒÒĞ ¢22##bÓ‚Ór(	BFVfVÇBÖöâW‡W&–ÖVçFÂvV"ÆFf÷&ÒfVGW&W0 ¢¢¤f–ÆW3¢¢¢6ö×öæVçG2ö6öæf–r÷&Vg2ç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó’ÖFVfVÇBÖöâÖW‡W&–ÖVçFÂ×vV"×ÆFf÷&Ò×&Vg2çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢W7G&VÒ6W'fò7W&FW2—G2÷vâ'VæFÆRöböfbÖ'’ÖFVfVÇBfVGW&W2–à¦U…U$”ÔTåDÅõ$Te6†÷'G2÷6W'f÷6†VÆÂ÷&Vg2ç'6’ÂÆÂfÆ—VBöâFövWF†W"öæÇ’v†Và¦ÆVæ6†VBv—F‚ÒÖVæ&ÆRÖW‡W&–ÖVçFÂ×vV"×ÆFf÷&ÒÖfVGW&W6âGvòöbF†VĞ¢†FöÕö7–æ5ö6Æ—&ö&EöVæ&ÆVFÂFöÕö–æFW†VFF%öVæ&ÆVF’vW&RW†7FÇ’F†R6Æ—&ö&Bô–æFW†VDD ¦v2f÷VæBv†–ÆRFW7F–ærF†R7F÷&vRf—‚&÷fR(	Bæf–vF÷"æ6Æ—&ö&Fv2VæFVf–æVFÂæ@¦–æFW†VDD"æ÷Vâ‚–†B6V6öæBÂ–æFWVæFVçB&V6öâFòf–Â&W–öæBF†R÷VRÖ÷&–v–âöæP¢‡6VRF†RVçG'’&÷fR’âF†R÷F†W"bVçG&–W2–âF†B6ÖR'VæFÆP¢†FöÕöW†V5ö6öÖÖæEöVæ&ÆVFÂFöÕöföçFf6UöVæ&ÆVFÂFöÕö–çFW'6V7F–öåöö'6W'fW%öVæ&ÆVFÀ¦FöÕöæf–vF÷%÷&÷Fö6öÅö†æFÆW'5öVæ&ÆVFÂFöÕöæ÷F–f–6F–öåöVæ&ÆVFÀ¦FöÕööfg67&VVåö6çf5öVæ&ÆVFÂFöÕ÷W&Ö—76–öç5öVæ&ÆVFÂFöÕ÷6æ—F—¦W%öVæ&ÆVFÀ¦FöÕ÷7F÷&vUöÖævW%ö•öVæ&ÆVFÂFöÕ÷vV&vÃ%öVæ&ÆVFÂFöÕ÷vV&wUöVæ&ÆVFÀ¦Æ–÷WEö775öGG%öVæ&ÆVFÂÆ–÷WEö6öÇVÖç5öVæ&ÆVFÂÆ–÷WEö6öçF–æW%÷VW&–W5öVæ&ÆVFÀ¦Æ–÷WEöw&–EöVæ&ÆVFÂÆ–÷WE÷f&–&ÆUöföçG5öVæ&ÆVF’vW&RÆÂ7F–ÆÂöfbFöò(	Bæ÷F&Ç¦FöÕ÷vV&vÃ%öVæ&ÆVFÂv†–6‚ÖVçBââ÷FW7B×vRöw2÷vâwT–æfõæVÆõ—†”¥2õF‡&VRæ§0¥vV$tÃ"&ö&W26÷VÆBæWfW"†fR6VVâ&VÂvV$tÃ"6öçFW‡B–âF†—2'V–ÆBÂöæÇ’vV$tÃà ¢¢¤6†ævS¢¢¢ÆÂ‚U…U$”ÔTåDÅõ$Te6VçG&–W2æ÷rFVfVÇBFòG'VV–à¦&VfW&Væ6W3£¦6öç7EöFVfVÇB‚–ÂV6‚6öÖÖVçFVBB—G2÷vâf–VÆBâFöÕ÷7F÷&vUöÖævW%ö•öVæ&ÆVF ¦FF—F–öæÆÇ’æVVFVBF†R6åö66W75÷7F÷&vR‚–W†V×F–öâg&öÒF†RVçG'’&÷fR‡6ÖP¦÷VRÖ÷&–v–âvFRÂF–ffW&VçB6ÆÂ6—FR’Fò7GVÆÇ’v÷&²VæFW"f–ÆS¢òöÂæ÷B§W7B&P¦W‡÷6VBà ¢¢¥v‡“¢¢¢F†—2f÷&²W†—7G2Fò'Vâ&VÂvÖW2rW†—7F–ærvV"6öFRÂæ÷BFò'&÷w6RF†RvVæW&À§vV"(	BF†W&Rw2æò'VçG'W7FVBF†—&B×'G’6—FR"F‡&VBÖöFVÂ†W&RF†BF†P¦W‡W&–ÖVçFÂ÷Vç7F&ÆR7Æ—B—2&÷FV7F–ærv–ç7B‡6–ævÆR'VæFÆVBFö7VÖVçBÂæòæf–vF–öâÀ§6VRF†RFööÆ&"÷F"×&VÖ÷fÂVçG&–W2’âW7G&VÒw2÷vâ7W&F–öâ—2&V6öæ&ÆRÂÇ&VG’×fWGFV@¦Æ–æRFòFVfVÇBF†—2f÷&²FòÂ&F†W"F†âV—F†W"ÆVf–ær&VÂvÖR—2…vV$tÃ"ÂvV$uRÀ¤öfg67&VVä6çf2Âæ÷F–f–6F–öç2Â552w&–Bô6öçF–æW"VW&–W2ÂWF2â’öfb'’FVfVÇB÷ §&RÖFW&—f–ærâWV—fÆVçBÆ—7Bg&öÒ67&F6‚âFVÆ–&W&FVÇ’F–B¢¦æ÷B¢¢W‡FVæBF†—2Fò&Vg0¦÷WG6–FRF†B'VæFÆR†RærâvV%%D2ÂvV"æ–ÖF–öç2Â67&VVâv¶RÆö6²Â&ÇVWFö÷F‚ÂvVöÆö6F–öâÀ¤7&VFVçF–ÂÖævVÖVçB’FW7—FR6öÖR&V–ærÆW6–&Ç’vÖR×&VÆWfçBFöò(	BF†÷6R&VâwB'Bö`§W7G&VÒw2÷vâfWGFVBW‡W&–ÖVçFÂ6WBÂ6òVæ&Æ–ærF†VÒ†W&Rv÷VÆB&RF†—2f÷&²w2÷và§VçFW7FVB§VFvÖVçB6ÆÂ&F†W"F†â&WW6RöbâW†—7F–æröæS²v÷'F‚&V6öç6–FW&–æp¦–æF—f–GVÆÇ’Âæ÷BWFöÖF–6ÆÇ’Â–b&VÂvÖRæVVG2öæRà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢–bgWGW&R6W'fòfW'6–öâ6†ævW0¦U…U$”ÔTåDÅõ$Te6w2ÖVÖ&W'6†—†FG2÷&VÖ÷fW2VçG&–W2Â÷"âVçG'’w&GVFW2Fò7F&ÆRæ@¦F—6V'2g&öÒF†RÆ—7BVçF—&VÇ’’Â&RÖF–fbF†BÆ—7Bv–ç7BF†—2VçG'’w2‚æÖW2&F†W §F†â77VÖ–ærF†W’7F–ÆÂÖF6‚âfW&–f–VBv—F‚6&vò6†V6²×6W'fòÖ6öæf–v†6ÆVâÂæBF†—0¦7&FR†2æò†Vg’æF—fRFW26òF†—26†V6²—2gVÆÇ’G'W7Gv÷'F‡’ÂVæÆ–¶RF†R6W'fò×67&—F ¦6fVB&÷fR’âæ÷B–WBfW&–f–VBVæB×FòÖVæBv–ç7B&VÂ'V–ÆBà ¢ÒÒĞ ¢22##bÓ‚Ór(	BF—6&ÆRF†R&–v‡BÖ6Æ–6²6öçFW‡BÖVçRVçF—&VÇ ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öF–Æörç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóÖF—6&ÆRÖ6öçFW‡BÖÖVçR×÷WçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢&–v‡BÖ6Æ–6¶–ærvV"6öçFVçB6VæG2VÖ&VFFW$6öçG&öÃ£¤6öçFW‡DÖVçVFòF†P¦VÖ&VFFW"†6†÷uöVÖ&VFFW%ö6öçG&öÆ–â†VFVE÷v–æF÷rç'6’Âv†–6‚'V–ÇBF–Æös£¤6öçFW‡DÖVçV ¢†F–Æörç'6’æB&VæFW&VB—B2âVwV–÷W(	B'&÷w6W"×7G–ÆRÖVçR„&6²ôf÷'v&Bõ&VÆöBğ¤6÷’Æ–æ²ô÷Vâ–âæWrf–Wrô7WBô6÷’õ7FRõ6VÆV7BÆÂÂ6öçFW‡GVÆÇ’f–ÇFW&VB’à ¢¢¤6†ævS¢¢¢VÖ&VFFW$6öçG&öÃ£¤6öçFW‡DÖVçR‡&ö×B––â†VFVE÷v–æF÷rç'6æ÷r§W7BG&÷‡&ö×B– ¦–ç7FVBöb'V–ÆF–æræB6†÷v–ærF–Æövâ6öçFW‡DÖVçS£¦G&÷†6ö×öæVçG2÷6W'fòğ§vV'f–WuöFVÆVvFRç'6ÂVæÖöF–f–VB’Ç&VG’6VæG2F†R&æò6VÆV7F–öâ"&W7öç6Rv†Vâ6öçFW‡DÖVçV ¦—2G&÷VBv—F†÷WBâW‡Æ–6—B6VÆV7FöF—6Ö—766ÆÂÂ6òF†—2—26÷'&V7BÂ–ÖÖVF–FRF—6Ö—76À¦g&öÒ6W'fòw2ö–çBöbf–Wr(	Bæ÷B†ær÷"ÆV¶VB&WVW7Bâv—F‚F†RöæÇ’6ÆÂ6—FRvöæRÂF†P¦VçF—&RF–Æös£¤6öçFW‡DÖVçV–×ÆVÖVçFF–öâ–âF–Æörç'6&V6ÖRFVB6öFRæBv2FVÆWFV@¦÷WG&–v‡B&F†W"F†âÆVgBVç&V6†&ÆS¢F†RVçVÒf&–çBÂ—G2WFFR‚–ÖF6‚&Ò‡F†RVwV– ¦&Vög&ÖV÷W&VæFW&–æræBW"Ö—FVÒ'WGFöâÆöv–2’Â—G2VÖ&VFFW%ö6öçG&öÅö–B‚–&ÒÂæ@§F†RæWuö6öçFW‡EöÖVçV6öç7G'V7F÷"âF†Ræ÷r×VçW6VB&&RVwV––×÷'G2†&VÂ'WGFöæÀ¦6÷&æW%&F—W6Âg&ÖVÂ–FÂ÷&FW&Â6Vç6VÂ7G&ö¶VÂfV3&Â÷3&’æB6W'fó£§´6öçFW‡DÖVçRÀ¤6öçFW‡DÖVçT—FV×ÖvW&R&VÖ÷fVBg&öÒF–Æörç'6w2W6V7FFVÖVçG266÷&F–ævÇ’(	BWfW'—F†–ærVÇ6P¦–âF†Bf–ÆR7F–ÆÂW6W2ÖöFÆö&–6…FW‡FæBgVÆÇ’×VÆ–f–VBVwV“£¦F‡2f÷"F†R–V6W2—@§7F–ÆÂæVVG2Â6òF†÷6RGvò7F–VBà ¢¢¥v‡“¢¢¢¶DôDòæÖFÒ‚âõDôDòæÖB’ö–çB"(	Bf–FVövÖRW6–ærF†—2f÷&²†2æòW6Rf÷"¦'&÷w6W"×7G–ÆR&–v‡BÖ6Æ–6²ÖVçRÂæBVçG&–W2Æ–¶R%f–Wr6÷W&6R"÷"$–ç7V7B"Ö¶Ræò6Vç6P¦÷WG6–FRâ7GVÂ'&÷w6W"æBv÷VÆB'&V²–ÖÖW'6–öââFV6–FVBFòF—6&ÆR—B÷WG&–v‡B&F†W"F†à§&WÆ6R—Bv—F‚7W7FöÒÖVçR†æòvÖR×&VÆWfçB&–v‡BÖ6Æ–6²7F–öç2vW&R–FVçF–f–VB’à ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢–bgWGW&R6W'fòfW'6–öâFG2æWp¦6öçFW‡DÖVçT7F–öæö6öçFW‡DÖVçT—FVÖf&–çG2Âæò6öFR†W&RæVVG2WFF–ær(	BF†RÖVçR—2æWfW ¦6öç7G'V7FVBBÆÂÂ6òæ÷F†–ær6öç7VÖW2F†÷6RVçV×2–âF†—2f÷&²â¢¤æ÷BfW&–f–VBv–ç7Bà¦7GVÂ6&vò6†V6²×6W'f÷6†VÆÂÒÖ&–â6W'f÷6†VÆÆöâöÖ6‚'V–ÆF¢£¢F†—26W76–öâw0¦6&vò6†V6¶v÷B7B7FÆRvV'&VæFW&'V–ÆB×67&—B66†RÆVgB÷fW"g&öÒ&Vf÷&RF†—0¦6†V6¶÷WBv2&VæÖVB÷&VÆö6FVB‡Vç&VÆFVBFòF†—26†ævR(	B6VRF†RVçG'’&VÆ÷r’Â'WBF†Vâ†—@¦Ö÷¦ævÆVw2†6ö×öæVçG2÷6W'föö6ö×öæVçG2÷67&—FFWVæFVæ7’æVVFVBFò'V–ÆB¦ç’ ¦6W'f÷6†VÆÆ&–æ'’ÂöâWfW'’ÆFf÷&ÒÂ&Vv&FÆW72öbF†—26†ævR’&–æFvVæÖ&6VB'V–ÆB67&—@¦f–Æ–ærv—F‚%Væ&ÆRFòf–æBÆ–&6Æær"(	BæòÆ–&6Æærç6ö—2–ç7FÆÆVB–âF†—26æF&÷‚æ@¦B–ç7FÆÂÆ–&6ÆærÖFWfæVVG2–çFW&7F—fR7VFöWF‚F†—26W76–öâFöW6âwB†fRâ6ÖP¦6FVv÷'’öbv2F†R6W'fò×67&—FöÆÇfÒÖö&¦GV×6fVBöâF†R##bÓ‚Ór7F÷&vRÖ66W70¦VçG'’&÷fS¢FööÆ6†–âv–â§F†—2¢Vçf—&öæÖVçBÂæ÷BWf–FVæ6Röb6öFR&ö&ÆVÒâv†B§v2 §fW&–f–VC¢ÆÂF‡&VRVF—G2‡F†—2VçG'’æBF†RGvò&VÆ÷r’vW&RÆ–VBv—F‚F6‚×¢ÒÖf÷'v&FFòg&W6‚ÂVæÖöF–f–VBW‡G&7F–öâöbF†RcãBãFrw2F–Æörç'6ğ¦†VFVE÷v–æF÷rç'6ÂÆ–VB6ÆVæÇ’v—F‚æògW§¢ööfg6WBv&æ–æw2ÂæBF†RF6†VB&W7VÇBv0¦'—FRÖf÷"Ö'—FRF–ffVBv–ç7BF†—2f÷&²w27GVÂv÷&¶–ær6÷’öb&÷F‚f–ÆW2v—F‚¦W&ğ¦F–ffW&Væ6W2(	B6òF†RF6†W2&R¶æ÷vâFòf—F†gVÆÇ’&W&öGV6RF†—2W†7B6†ævRÂWfVâF†÷Vv€§F†R6†ævR—G6VÆb†6âwB&VVâ6ö×–ÆW"Ö6†V6¶VB†W&RâG&VBâ7GVÂ'V–ÆB‡v—F‚Æ–&6Ææv ¦f–Æ&ÆR’2F†R&VÂfW&–f–6F–öâ&Vf÷&R&VÇ––æröâF†—26ö×–Æ–ærà ¢ÒÒĞ ¢22##bÓ‚Ór(	B&VÖ÷fRF†RvR×&VÆöB¶W–&ö&B6†÷'F7WG0 ¢¢¤f–ÆS¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6Âæ÷F–g•ö–çWEöWfVçEö†æFÆVF‡v2Æ–æP£cÓcB–âF†RcãBã&6VÆ–æR’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó×&VÖ÷fR×vR×&VÆöB×6†÷'F7WG2çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢6ÖFö7G&Âµ&æBcV&÷F‚6ÆÆVBvV'f–Wrç&VÆöB‚–Væ6öæF—F–öæÆÇ’à ¢¢¤6†ævS¢¢¢&÷F‚6†÷'F7WB&Vv—7G&F–öç2&VÖ÷fVBg&öÒF†R6†÷'F7WDÖF6†W&6†–ââæ÷F†–ærVÇ6P¦–âæ÷F–g•ö–çWEöWfVçEö†æFÆVF&VfW&Væ6W2F†VÓ²F†R¦ööÒ6†÷'F7WG2–ÖÖVF–FVÇ’&÷fR&P§VçF÷V6†VBà ¢¢¥v‡“¢¢¢¶DôDòæÖFÒ‚âõDôDòæÖB’ö–çB2(	B&VÆöF–ær&W6WG2v†FWfW"–âÖÖVÖ÷'’vÖR7FFRF†P§vR†2'V–ÇBW‡F†—2f÷&²†2æòæf–vF–öâ†—7F÷'’÷6W76–öâ×&W7F÷&R6öæ6WBFòfÆÂ&6²öâ’À§6òâ66–FVçFÂ7G&Âµ&öcV—2W&RFFÆ÷72f÷"Æ–W"Âv—F‚æòWV—fÆVçB'&Vg&W6‚F†P§vR"W6R66RvÖRæVVG2âF†R&–v‡BÖ6Æ–6²ÖVçRw2÷vâ&VÆöFVçG'’—26÷fW&VB6W&FVÇ’'§F†R##bÓ‚Ór6öçFW‡BÖÖVçR&VÖ÷fÂ&÷fR‡F†RÖVçRF†Bv÷VÆB†fRöffW&VB—BæòÆöævW"W†—7G0¦BÆÂ’à ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢W6W$–çFW&f6T6öÖÖæC£¥&VÆöFö&VÆöDÆÆ€¦'Vææ–æuö÷7FFRç'6’æBvV%f–Ws£§&VÆöB‚–—G6VÆb&RFVÆ–&W&FVÇ’ÆVgBÆöæR(	BF†W’w&P§7F–ÆÂ&V6†&ÆRg&öÒF†R&÷fW2Ö–ôæG&ö–BÖVÖ&VFF–ær×7G–ÆRVvÃ£¤£§&VÆöB‚–V&Æ–2¢†÷'G2÷6W'f÷6†VÆÂöVvÂöç'6’W6VB'’æF—fR†÷7B6öFRöâF†÷6RF&vWG2Âv†–6‚—2÷WBöb66÷P¦f÷"F†—2VçG'’‡6VRDôDòæÖFö–çB2w2g&Ö–æs¢¶W–&ö&BöÖVçRövW7GW&RÂæ÷BF†RVÖ&VFF–ær§7W&f6R’âöæÇ’F†RFW6·F÷¶W–&ö&BVçG'’ö–çG2Æ–W"6âG&–vvW"F—&V7FÇ’vW&R&VÖ÷fVBà¢¢¤æ÷BfW&–f–VBv–ç7Bâ7GVÂ'V–ÆB¢¢(	B6ÖRÆ–&6ÆævöÖ÷¦ævÆV6æF&÷‚v2F†P¦6öçFW‡BÖÖVçRVçG'’&÷fS²fW&–f–VBF†R6ÖRv’–ç7FVB‡F6‚Æ–W26ÆVæÇ’Fò&—7F–æP¦cãBã†VFVE÷v–æF÷rç'6æB&W&öGV6W2F†—2f÷&²w27GVÂf–ÆR'—FRÖf÷"Ö'—FR’à ¢ÒÒĞ ¢22##bÓ‚Ór(	B&VÖ÷fRÆÂ&6²öf÷'v&B†—7F÷'’æf–vF–öà ¢¢¤f–ÆS¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6†¶W–&ö&B6†÷'F7WG2Âv2Æ–æR3ƒBÓCR–à§F†RcãBã&6VÆ–æS²Ö÷W6R6–FRÖ'WGFöâ†æFÆ–ærÂv2Æ–æRc2Óc‚’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó"×&VÖ÷fRÖ&6²Öf÷'v&BÖæf–vF–öâçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢6ÖBô7G&Â´ÇBµ&–v‡Fö6ÖBô7G&ÂµÖ6ÆÆVB7F—fU÷vV'f–Wrævõöf÷'v&Bƒ–À¦6ÖBô7G&Â´ÇB´ÆVgFö6ÖBô7G&Âµ¶6ÆÆVB7F—fU÷vV'f–Wrævõö&6²ƒ–ÂæB&W76–ærF†RÖ÷W6Rw0§6–FR$f÷'v&B"ò$&6²"'WGFöç2†v–æ—C£¦WfVçC£¤Ö÷W6T'WGFöã£¤f÷'v&Fö&6¶’VWVV@¦W6W$–çFW&f6T6öÖÖæC£¤f÷'v&Fö&6¶Âv†–6‚v–æF÷rç'6w2†æFÆUö–çFW&f6Uö6öÖÖæG6&W6öÇfW0§FòF†R6ÖRvõöf÷'v&Fövõö&6¶6ÆÇ2à ¢¢¤6†ævS¢¢¢F†Rf÷W"¶W–&ö&B6†÷'F7WB&Vv—7G&F–öç2vW&R&VÖ÷fVBg&öÒF†R6†÷'F7WDÖF6†W& ¦6†–â‡&WÆ6VBv—F‚âW‡ÆæF÷'’6öÖÖVçBÂæò&WÆ6VÖVçB&–æF–ær’Âv†–6‚ÆVgBF†P¦4ÔEôõ%ôÅF–×÷'BVçW6VBæB—Bv2&VÖ÷fVBFöòâF†RÖ÷W6T'WGFöã£¤f÷'v&Fæ@¦Ö÷W6T'WGFöã£¤&6¶ÖF6‚&×2vW&RÖW&vVB–çFò6–ævÆR&ÒF†B7F–ÆÂ6WG26öç7VÖVBÒG'VV ¢‡6òF†R'WGFöâ&W72FöW6âwBfÆÂF‡&÷Vv‚FòVwV–÷F†RvR’'WBæòÆöævW"VWVW2æf–vF–öà¦6öÖÖæB(	BF†R6–FR'WGFöç2&Ræ÷r–æW'B&F†W"F†âG&–vvW&–ær†—7F÷'’æf–vF–öâà ¢¢¥v‡“¢¢¢¶DôDòæÖFÒ‚âõDôDòæÖB’ö–çBB(	Bf÷"vÖRÂæf–vF–ær&&6²"÷WBöbF†RvRw0¦7W'&VçB7FFR6â6–ÆVçFÇ’æB6ö×ÆWFVÇ’'&V²v†FWfW"F†RvÖRv2Fö–ær†æò'&÷w6W"6‡&öÖP¦W†—7G2FòW‡Æ–âv†B†VæVB÷"öffW"&f÷'v&B"2&V6÷fW'’’âF†—2æVVFVB6÷fW&–ær7&÷70¦WfW'’–çWBF‚Æ–W"6÷VÆBG&–vvW"—Bg&öÓ¢F†R¶W–&ö&B6†÷'F7WG2†W&RÂF†RÖ÷W6R6–FP¦'WGFöç2†W&RÂæBF†R6öçFW‡BÖVçRw2÷vâvô&6¶övôf÷'v&FVçG&–W2Âv†–6‚&R6÷fW&VB'’F†P£##bÓ‚Ór6öçFW‡BÖÖVçR&VÖ÷fÂ&÷fR‡F†BÖVçRæòÆöævW"W†—7G2FòöffW"F†VÒ’à ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢6ÖR66÷Ræ÷FR2F†R&VÆöBVçG'’&÷fR(	@¦W6W$–çFW&f6T6öÖÖæC£¤&6¶öf÷'v&FÂvV%f–Ws£¦võö&6¶övõöf÷'v&FÂæBF†RVvÃ£¤ §V&Æ–2’w2võö&6²‚–övõöf÷'v&B‚–‡W6VB'’æF—fR†÷7B6öFRVÖ&VFF–ærF†—2Væv–æRÂRærâöà¤æG&ö–Bô÷Vä†&Ööç’’&RFVÆ–&W&FVÇ’VçF÷V6†VC²öæÇ’F†RFW6·F÷Æ–W"Öf6–ær–çWBF‡2vW&P¦F—6&ÆVBâ–bgWGW&R6W'fòfW'6–öâFG2æ÷F†W"v’Fò&V6‚&6²öf÷'v&Bæf–vF–öâg&öĞ§Æ–W"–çWB†æWrvW7GW&RÂæWræÖVB¶W’’Â&RÖÇ’F†R6ÖR&V6öæ–ærFò—Bâ¢¤æ÷@§fW&–f–VBv–ç7Bâ7GVÂ'V–ÆB¢¢(	B6ÖRÆ–&6ÆævöÖ÷¦ævÆV6æF&÷‚v2F†RGvòVçG&–W0¦&÷fS²fW&–f–VBF†R6ÖRv’–ç7FVB‡F6‚Æ–W26ÆVæÇ’Fò&—7F–æRcãBã ¦†VFVE÷v–æF÷rç'6æB&W&öGV6W2F†—2f÷&²w27GVÂf–ÆR'—FRÖf÷"Ö'—FR’à ¢ÒÒĞ ¢22##bÓ‚Ór(	B6–FS¢7FÆRvV'&VæFW&'V–ÆB×67&—B66†RgFW"F†—26†V6¶÷WBv2&VÆö6FV@ ¤æ÷BF6‚Âæ÷B7W7FöÖ—¦F–öâ(	Bæ÷FRf÷"v†öWfW"æW‡B'Vç26&vò6†V6¶öâöÖ6‚'V–ÆF–à§F†—2W†7B6†V6¶÷WBâvV'&VæFW&w2'V–ÆB67&—B†B&Wf–÷W6Ç’vVæW&FV@¦F&vWBöFV'Vrö'V–ÆB÷vV'&VæFW"Ò¢ö÷WB÷6†FW'2ç'66öçF–æ–ær–æ6ÇVFU÷7G"‚"ö†öÖR÷6–ÖöæRğ§FV×ÆFR÷—†’×fâ×&V7B×FV×ÆFR÷6W'fò÷F&vWBòâââ"–(	Bâ'6öÇWFRF‚&¶VB–âg&öÒ&Vf÷&RF†—0¦F—&V7F÷'’v2&VÆö6FVB÷&VæÖVBFò—G27W'&VçBF‚â6&vò6†V6²×6W'f÷6†VÆÆF†W&Vf÷&Rf–ÆV@¦–ÖÖVF–FVÇ’v—F‚$æò7V6‚f–ÆR÷"F—&V7F÷'’"f÷"WfW'’6†FW"ÂVç&VÆFVBFòç’6W'fò6÷W&6P¦6†ævRâf—†VBf÷"F†—26†V6¶÷WBv—F‚6&vò6ÆVâ×vV'&VæFW&†f÷&6W2F†R'V–ÆB67&—BFğ§&W'VâæB&VvVæW&FR6†FW'2ç'6v—F‚F†R7W'&VçBÂ6÷'&V7BF‚’âæ÷B6öFR6†ævRÂ6òæğ§F6‚öVçG'’&W–öæBF†—2æ÷FR(	B'WBv÷'F‚¶æ÷v–ær–bg&W6‚6&vò6†V6¶×—7FW&–÷W6Ç’f–Ç2öà¦vV'&VæFW&6†FW'2v–âgFW"Ö÷f–ærö6÷––ærF†—26†V6¶÷WBâ6ÆV&–ærF†B66†RVæ&Æö6¶V@¦vV'&VæFW&'WBW‡÷6VB6V6öæBÂ–æFWVæFVçBv&–v‡B&V†–æB—B(	B6VRF†RÆ–&6Æævğ¦Ö÷¦ævÆV6fVB&WVFVBöâÆÂF‡&VRVçG&–W2&÷fR(	B6ò6&vò6†V6²×6W'f÷6†VÆÂÒÖ&–à§6W'f÷6†VÆÆ7F–ÆÂFöW6âwB7W'&VçFÇ’6ö×ÆWFR–âF†—2'F–7VÆ"6æF&÷‚WfVâv—F‚F†—2f—†VBà ¢ÒÒĞ ¢22##bÓ‚Ór(	B&VæÖRW6W"ôõ2Öf6–ærÆ&VÇ2g&öÒ%6W'fò"Fò%&÷fW2  ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷vV'‡"ç'6À¦&W6÷W&6W2ö÷&rç6W'fòå6W'fòæFW6·F÷‡&VæÖVBFò&W6÷W&6W2ö÷&rç&÷fW2å&÷fW2æFW6·F÷’À¦—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–âÇ6òæv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆ‡Gvò6öÖÖVçBöV6†ğ§7G&–æw2Âæ÷BF6‚×G&6¶VB(	B6VR6fVB&VÆ÷r’æB$TDÔRæÖF†æ÷BF6‚×G&6¶VBÂ6VP¦4ÄTDRæÖFw266÷Ræ÷FRöâv†BæVVG2F6†W2g2âÆ–âFö2WFFW2’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó2×&VæÖR×6W'fòÖÆ&VÇ2×Fò×&÷fW2çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢WfW'’Æ&VÂÆ–W"÷"F†Rõ27GVÆÇ’6†÷w2f÷"F†—2Æ–6F–öà§7F–ÆÂ6–B%6W'fò"‡F†RW7G&VÒ&ö¦V7Bw2æÖR’Âæ÷B%&÷fW2"‡F†—2f÷&²w27GVÂ&öGV7@¦æÖR(	B6VR$TDÔRæÖFw2–çG&ò“¢F†Rv–æF÷rF—FÆR†”ä•D”Åõt”äDõuõD•DÄV’ÂF†R…"v–æF÷p§F—FÆRÂF†RÆ–çW‚tÒ÷F6¶&"–B†v—F…öæÖR‚&÷&rç6W'fòå6W'fò"Â%6W'fò"–’ÂF†R÷Vå… ¦–æföÆ–6F–öâæÖRÂF†RæFW6·F÷ÆVæ6†W"VçG'’w2æÖSÖÂæBF†R7W7FöÒÖ6€¦'VæFÆV6öÖÖæBw2‡6VRF†R##bÓ‚ÓbÖ6‚'VæFÆVVçG'’&÷fR’vVæW&FVBÖ4õ2'VæFÆRæÖP¢†6W'fòæ’æB–æfòçÆ—7F4d'VæFÆTW†V7WF&ÆVö4d'VæFÆTæÖVà ¢¢¤6†ævS¢¢¢V6‚öbF†R&÷fRæ÷r6—2%&÷fW2"–ç7FVBöb%6W'fò#  ¢Ò†VFVE÷v–æF÷rç'6¢”ä•D”Åõt”äDõuõD•DÄRÒ%&÷fW2&Â…"v–æF÷rF—FÆR%&÷fW2…"&À¢v—F…öæÖR‚&÷&rç&÷fW2å&÷fW2"Â%&÷fW2"–à¢ÒvV'‡"ç'6¢÷Vå‡$–æfó£¦æWr‚%&÷fW2"ÂÂ%6W'fò"Â–(	BöæÇ’F†R¦Æ–6F–öâ¢æÖP¢6†ævVC²F†R¦Væv–æR¢æÖR&wVÖVçBFVÆ–&W&FVÇ’7F–ÆÂ6—2%6W'fò"Â6–æ6RF†RVæFW&Ç––æp¢Væv–æRvVçV–æVÇ’7F–ÆÂ—26W'fò‡6VR$FVÆ–&W&FVÇ’ÆVgBÆöæR"&VÆ÷r’à¢Ò&W6÷W&6W2ö÷&rç6W'fòå6W'fòæFW6·F÷&VæÖVBFò&W6÷W&6W2ö÷&rç+ZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æÚ±î¸Â¸­yêë¢°k¢G§¦*^oves.Roves.desktop`,
  `Name=Servo` â†’ `Name=Roves`, and the file's own self-referential setup instructions
  (`cp org.servo.Servo.desktop ...`) updated to match the new filename.
- `post_build_commands.py`'s `_bundle_macos`/`bundle` methods: `Servo.app` â†’ `Roves.app` (both
  the folder name and the docstring describing it), and the generated `Info.plist`'s
  `CFBundleExecutable`/`CFBundleName` (and the launcher script file they must match,
  `Contents/MacOS/Servo` â†’ `Contents/MacOS/Roves`) â†’ `"Roves"`.
- `.github/workflows/test.yml`: two comment/log strings referencing `Servo.app` updated to
  `Roves.app` to stay consistent with the rename above (this workflow only describes/exercises
  `mach bundle`'s output; see its own header comment on why it doesn't run anywhere yet).
- `README.md`: added a "Naming" section explaining the current state (Roves labels vs. the
  still-Servo-named engine/binary) so this doesn't read as an inconsistency to a new reader.

**Deliberately left alone (broader rename intentionally out of scope for this change):**

- The `servoshell` Cargo package/binary name itself (directory `ports/servoshell/`, `[package]
  name = "servoshell"` and everything derived from it) â€” considered, and **decided against**,
  not just deferred. A repo-wide `grep -rl servoshell` turns up ~50 files, including upstream
  Python build-system internals under `python/servo/` (`command_base.py`, `gstreamer.py`,
  `devtools_tests/*`, etc.) that have nothing to do with branding, plus `Cargo.lock`
  regeneration â€” too large and too unverifiable in a sandbox without `libclang` (see the
  caveats above) for no functional benefit. `servoshell` stays the binary/package name going
  forward; see `README.md`'s "Naming" section.
- `ContextMenu`/`CFBundleIdentifier` (`org.servo.servoshell.bundle`) and Android's
  `ANDROID_APP_NAME` (`org.servo.servoshell`, `post_build_commands.py`) â€” bundle/package
  identifiers, not display labels; changing an Android package name in particular makes the
  OS treat it as a completely different app (losing update continuity), so this needs an
  explicit decision, not a mechanical rename alongside display strings.
- `ports/servoshell/prefs.rs`'s on-disk config directory name (`config_dir.push("Servo")`) â€”
  changing this moves where preferences/persistent data are read from on an existing install;
  needs a migration decision, not a silent rename.
- `ports/servoshell/platform/macos/Info.plist` (upstream's own static bundle metadata, used by
  `./mach package`, a different mechanism than the `mach bundle` command above) and
  `etc/macos_sign.py`/`support/macos/Servo.entitlements` (codesigning/notarization) â€” a larger,
  more sensitive surface (signing identity, entitlements) not covered by this pass.
- `webxr.rs`'s OpenXR *engine* name argument (see above) and any other place that credits the
  actual Servo engine rather than naming the product â€” this fork doesn't rename Servo's
  internals, only the shell around it (see `README.md`'s "What this is").

**Why:** this fork's product is Roves, not Servo (see `README.md`), but several places a
player or the OS directly shows text still said "Servo" â€” inconsistent with the actual product
identity and confusing for anyone who notices (window title bar, Alt-Tab/taskbar, Finder/dock,
the Linux app menu). Scoped deliberately to *display labels only* (not the underlying
binary/package name, bundle identifiers, on-disk paths, or codesigning) after discussing the
size/risk of the full rename with the project owner â€” see the deliberately-left-alone list
above and `TODO.md` point 4 for what's still open.

**Side effects to know about when upgrading:** `.github/workflows/test.yml` and `README.md`
are this fork's own files with no upstream counterpart, so they aren't part of the
patch-against-pristine-tag mechanism the same way source files are (no prior patch in this
directory touches `.github/`, by existing convention â€” see e.g. patches 0001-0012, none of
which touch `.github/`); their changes are captured here in prose only, not replayed by
`0013`'s patch â€” if this repo is ever restructured so `.github/` *is* patch-tracked, fold
these two small string changes in then. If a future Servo version changes `AppInfo`'s
constructor signature/parameter order in `components/webxr/openxr/mod.rs`, double check which
argument is `application_name` vs `engine_name` before reapplying â€” swapping them would put
"Roves" in the wrong field.

---

## 2026-08-07 â€” Pack game content into compressed archives instead of shipping loose files

**Files:** `python/servo/post_build_commands.py` (`bundle`, `_place_bundle_content`,
`_bundle_windows`/`_bundle_macos`/`_bundle_linux`/`_bundle_linux_deb`), `Cargo.toml` (new
workspace member), and a brand-new crate, `support/content-packer/` (bin
`roves-content-packer`). Also `test-page/public/` â€” new test fixtures, not part of the patch
(see "Side effects" below).

**Patch:** `patches/servo-v0.4.0/0014-pack-and-compress-game-content.patch`

**Motivating problem:** since the 2026-08-06 `mach bundle` entry above, `--content-dir` (a
game's built `dist/`) was copied into the release bundle with a plain `shutil.copytree` â€”
every source file landed in the shipped zip exactly as built, trivially browsable/extractable
by anyone who unzips a release. Nothing about `mach bundle`'s job (produce something
double-click-runnable) required that; it was just the simplest thing `_place_bundle_content`
could do.

**Change:** `mach bundle` gained four new flags: `--content-compress {auto,none}` (default
`auto`), `--content-compression-level N` (default `1` â€” zstd, favoring speed over ratio),
`--content-max-pack-size SIZE` (default `500M`), `--content-exclude GLOB` (repeatable). With
the new default, `--content-dir` is no longer copied in as loose files. Instead:

- `_place_bundle_content` shells out to `roves-content-packer pack`, which walks
  `--content-dir` and splits it into a small, fixed number of `tar`+`zstd` archives
  (`.pack` files) plus a `manifest.json`, by depth: dist's own root-level files â†’ one archive;
  each direct subfolder's own direct files â†’ one archive per subfolder; everything deeper than
  that (grandchildren and beyond) â†’ one more archive per top-level subfolder, with every
  descendant flattened into it (tar entries keep their full relative path, so unpacking
  reconstructs the original tree regardless of which archive a file ended up in). Each archive
  is capped at `--content-max-pack-size`, splitting into `.1.pack`/`.2.pack`/... past that.
  Files whose extension is already internally compressed (images, audio, video, fonts,
  archives â€” see `STORED_EXTENSIONS` in `support/content-packer/src/pack.rs`) go into a
  separate, *uncompressed* tar per bucket (`<bucket>.stored.pack`) instead of being fed through
  zstd a second time for no size benefit. `--content-exclude` globs (matched relative to
  `--content-dir`) are left as plain, unpacked files instead â€” e.g. a save-data/user-config
  subfolder a game ships inside `dist/` that shouldn't sit inside a read-only archive.
  Archiving order is fully deterministic (sorted paths throughout), so packing the same input
  twice produces byte-identical output â€” `manifest.json`'s `content_hash` (sha256 over every
  packed/excluded file's path+contents, in that same sorted order) changes iff the real output
  would.
- Each generated launcher (`play.exe`/`Roves`/`play.sh`/the `.deb`'s `/usr/bin/<pkg>` script)
  gets a copy of `roves-content-packer` alongside itself, and now runs its `extract`
  subcommand â€” synchronously, before starting the engine â€” to reconstruct plain files. Called
  with no `--dest`, `extract` picks (and prints) its own location under the OS temp directory
  (`std::env::temp_dir()`), keyed by a hash of the resolved `--content-dir` path, and each
  launcher captures that printed path (`CACHE_DIR="$(./roves-content-packer extract
  --content-dir ...)"` in bash; `Command::output()` â€” not `.status()` â€” on Windows) to build the
  engine's real html-file argument at *this* launch. Nothing is ever extracted next to the
  bundle itself, on any platform â€” see the same-day correction below for why that changed.
  `extract` skips the work entirely on a re-launch with unchanged content: it compares
  `manifest.json`'s `content_hash` against a `.content-hash` marker left in the destination
  from the previous run, so the OS temp directory being outside the bundle doesn't mean paying
  the decompression cost on every single launch.
- `--content-compress=none` restores the exact previous behavior (plain `copytree`, no
  `.content-cache` indirection, no packer binary shipped) â€” an escape hatch, not a special
  case sprinkled through the packing logic: `_place_bundle_content` branches on it right at the
  top and returns early.

**Why:** a fork whose stated purpose is embedding a game (see `README.md`) shouldn't make
that game's own source assets sit unprotected in every release by default â€” a curious end
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
hashes in the manifest (no current consumer â€” nothing here does incremental
patching/integrity verification yet, so it would be dead weight); any actual encryption or
DRM-style obfuscation (the request was specifically for compression, and a fake-security XOR
scheme would be worse than no scheme â€” see `support/content-packer/`'s own lack of one). Real
protection against a motivated reverse-engineerer is a materially different, larger feature
and wasn't asked for.

**Correction (same day):** the first version of this extracted into a fixed `.content-cache/`
directory sitting right next to the bundle (baked into the html-file launch arg at
`mach bundle` time), and each launcher ran the extractor via a blocking call that, on Windows,
is a console-subsystem child process spawned from a `windows_subsystem = "windows"` parent â€”
which flashes a console window for an instant before servoshell's own window opens. Two
problems reported after trying an actual bundle: that window flash (read as "two windows, one
closing to open the other"), and not wanting a `.content-cache/` folder visibly sitting inside
the shipped game folder at all, even though it holds re-derived, re-creatable content rather
than anything load-bearing. Fixed by (1) making `--dest` optional in `extract`, defaulting to
a hash-keyed path under the OS temp directory instead of a caller-supplied one, which is what
let every launcher stop hardcoding a bundle-relative cache path and made the `.deb` launcher's
old `${XDG_CACHE_HOME:-$HOME/.cache}/<package_name>/content-cache` special case (needed only
because `/usr/lib/<package_name>/` isn't user-writable) unnecessary too â€” `temp_dir()` resolves
to something writable regardless of `--content-dir`'s own location; and (2) passing
`CREATE_NO_WINDOW` (`0x0800_0000`) via `CommandExt::creation_flags` on the Windows launcher's
child `Command`. A true zero-disk-writes design (a custom `content:` protocol handler
decompressing on demand, entirely in memory, never touching any filesystem path) was
considered and explicitly deferred rather than chosen â€” see "Deliberately left out" above and
this file's own note on why: it would need `components/url/origin.rs`'s `file://`-specific
opaque-origin/storage-access/trustworthy-origin carve-outs (2026-08-06 "Stable `file://`
origin" and 2026-08-07 "Storage access for `file://` origins" above) extended to a second
scheme, which is real spec-sensitive surface to get right and verify, not something to rush
through right before a release. OS-temp-directory extraction gets the actual complaint (no
visible artifact in the shipped game's own folder) without touching that surface at all.

**Side effects to know about when upgrading:** none of this touches Servo internals â€” it's
new Python (a fourth `_bundle_*` parameter plus a new helper) and a wholly new, dependency-thin
Rust crate (`tar`, `zstd`, `walkdir`, `glob`, `sha2`, `serde`/`serde_json`, `bpaf` â€” all either
already workspace dependencies or already resolvable in `Cargo.lock` at the versions pinned in
`support/content-packer/Cargo.toml`), so it should survive a version bump untouched as long as
`_place_bundle_content`'s and the four `_bundle_*` methods' call sites in `bundle()` aren't
restructured upstream (they're 100% this fork's own code, not upstream Servo's, so that risk
is really just "did a later patch in this same set change their signatures again" â€” check
`0004`/`0013` first). `test-page/public/`'s new image/audio/JSON/SVG fixtures (root files, two
direct subfolders each with their own files plus a nested sub-subfolder) exist purely to give
`test.yml`'s `npm run build` something realistic to exercise all three archive levels against
â€” they aren't part of the patch set since they're not derived from any upstream file at all,
just plain test fixtures tracked directly in this repo.

---

## 2026-08-08 â€” Split packed content into an eager "boot set" + lazy, on-demand extraction

> **Note (2026-08-08, later same day):** the "generated launcher calls `roves-content-packer
> extract`" description below (Windows/macOS/Linux launcher scripts/binaries) is superseded
> by the "Single-executable bundle" entry near the end of this file â€” that extraction call
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
before the engine starts â€” fine for a small diagnostic page, but for a game whose assets reach
the multi-GB range, that means (a) a first-launch (or first-launch-after-a-content-update)
stall proportional to the *entire* game's size, even for content the player won't touch for
hours (a later level, an optional cosmetic pack), and (b) briefly needing disk space for both
the compressed archives and the full decompressed copy at once. Raised directly: "il
funzionamento... in un progetto di GB non rallenterÃ  il sistema??"

**Change:** `roves-content-packer pack` now splits packed content into two tiers instead of
one:

- A small **boot set** â€” the html file itself, plus every local `src=`/`href=` it references
  directly (a lightweight attribute scan, not a full HTML parser; catches a bundler's entry
  `<script>`, `<link rel="modulepreload">` hints, a favicon, etc. â€” verified against
  `test-page/dist`'s real Vite output), plus anything matching the new `--boot-include`
  (`mach bundle --content-boot-include`) glob. These get their own dedicated archive(s)
  (`__boot__.pack`/`__boot__.stored.pack`), extracted eagerly by `roves-content-packer extract`
  (still called by every generated launcher exactly as before â€” its *contract* didn't change,
  only what it extracts) before the engine even starts.
- **Everything else** stays compressed. `manifest.json` (format v2) now also carries a
  `files: {path: pack}` map, so a specific path can be traced back to the one archive that
  holds it without touching any other. Nothing extracts it until something actually asks â€”
  which is the new part.

That "something asking" is a new `file:` protocol handler
(`ports/servoshell/desktop/protocols/file.rs`), replicating the stock handler's behavior
(plain reads, HTTP Range support for `<video>`/`<audio>` seeking) with one addition: if a
requested path doesn't exist yet and falls under the known content-cache directory, it
decompresses whichever pack contains it (a `roves_content_packer::extract::ensure_file_available`
call, guarded by a mutex so two near-simultaneous requests for the same not-yet-extracted pack
don't race) *before* the read proceeds â€” extraction is per-pack, not per-file (tar/zstd can't
cheaply seek to one member without processing everything before it), so a request "waits" at
most for its own bucket's archive, never the whole game. This is why `ensure_pack_extracted`'s
marker-file cache (see the previous entry) had to become *incremental*: a destination now
holds boot files plus whichever lazy packs have been touched so far, and only a genuine
content change (a mismatched `content_hash`) wipes it â€” an unchanged relaunch keeps everything
already extracted in earlier sessions, not just the boot set.

Registering a *custom* `file:` handler at all needed one real engine change:
`ProtocolRegistry::merge`'s `entry().or_insert()` only fills vacant slots, and
`components/servo/servo.rs` built the internal-defaults registry (which always has `file`)
first, then merged the embedder's registry into *that* â€” meaning an embedder's own `file`
registration was always silently discarded. Swapped the merge direction (embedder's registry
first, internal defaults merged in on top) so it isn't, for every scheme an embedder
explicitly claims â€” a no-op for any other embedder of the `servo` crate, since none of them
register `file`/`data`/`blob` today. `components/servo/lib.rs`'s `protocol_handler` facade
module also gained re-exports of three already-`pub` `net::protocols` functions
(`get_range_request_bounds`/`partial_content`/`range_not_satisfiable_error`) so the new
handler could reuse them instead of reimplementing Range-request math.

**Deliberately not replicated:** the stock handler's directory-listing fallback
(`local_directory_listing`) â€” Roves never opens more than one `file://` document and never
navigates to a bare directory (no address bar, no tabs), so that code path doesn't apply, and
reusing it would need a `pub(crate)` â†’ `pub` visibility patch to `components/net` this doesn't
justify. A directory request now returns a network error instead.

**Why:** the boot set is deliberately *tiny by construction* (an entry chunk, a stylesheet, an
icon), so paying its extraction cost on every cold start is cheap regardless of the game's
total size â€” the multi-GB case only ever pays for what a session actually touches, exactly
once, and everything already touched survives a relaunch unless the content genuinely
changed. Note the honest limit: how small the boot set stays is downstream of the game's own
bundler code-splitting â€” a bundler that statically imports (or `modulepreload`-hints) most of
the app from the entry HTML will end up with most of the app in the boot set too, same as it
would eagerly fetch it in a browser regardless of this feature. Structuring lazy content
behind dynamic `import()` (standard web performance practice already) is what actually keeps
the boot set small for a large game â€” this tool respects exactly what the entry HTML declares
as immediately needed, it doesn't second-guess it.

**Explicitly considered and deferred:** a native "loading" splash shown by the launcher during
boot extraction. Not implemented â€” the boot set is small enough that this gap is typically
sub-second, and a real per-platform splash window (creation, synchronization with the
launcher's blocking extraction call, closing it at the right moment the engine's own window
appears) is a meaningfully sized, purely additive UI task on its own; standard web practice
(a page's own loading indicator while it fetches further lazy assets) already covers the more
common case of *waiting on gameplay assets*, which is the game's own responsibility, not
Roves'.

**Side effects to know about when upgrading:** the `components/servo/servo.rs` merge-order
swap and the `components/servo/lib.rs` re-exports are the first changes in this patch set that
touch genuinely shared engine code rather than code local to `ports/servoshell` or
`support/content-packer` â€” re-verify both still make sense if a future Servo version reshapes
`ProtocolRegistry`/`protocol_handler`'s module layout. **Not independently verified by a real
build in this environment** â€” `cargo check -p servoshell` (and `-p servo`, which
`ports/servoshell/desktop/protocols/file.rs` and the `servo.rs`/`lib.rs` changes are part of)
both hit the pre-existing `libclang`/`bindgen`/`mozangle` gap noted elsewhere in this file
before ever reaching this code; `roves-content-packer` itself (manifest v2, boot detection,
the incremental extraction cache, `ensure_file_available`) is fully covered by
`support/content-packer/tests/roundtrip.rs` and passes. Treat the servoshell-side pieces as
reviewed-but-not-compiled until a real `./mach build` confirms them.

**Correction (next day, after `.github/workflows/test.yml` actually ran this patch on real
Windows/macOS/Linux runners):** exactly one compile error, on every platform â€”
`ports/servoshell/desktop/protocols/file.rs`'s `use http::Method;` was an unresolved import
(E0432). `headers` (already a `servoshell` dependency, used for the rest of this file's Range
handling) doesn't re-export the `http` crate; `http` itself is a workspace dependency
(`Cargo.toml`'s `[workspace.dependencies]`) but hadn't been added to
`ports/servoshell/Cargo.toml`'s own `[dependencies]` â€” Rust's crate resolution needs it listed
on the crate that actually uses it, not merely present somewhere in the workspace lockfile.
Fixed by adding `http = { workspace = true }` there. Exactly the gap the note above flagged
("reviewed-but-not-compiled") â€” this is that real `./mach build` confirmation, and it caught a
real, if narrow, bug on the first try.

---

## 2026-08-08 â€” Accept absolute Windows paths on the command line (`Unsupported scheme` fix)

**File:** `ports/servoshell/parser.rs`

**Patch:** `patches/servo-v0.4.0/0016-accept-absolute-windows-paths-on-the-command-line.patch`

**Symptom:** on Windows, the bundled `play.exe` produced by `mach bundle` opened a window
showing nothing but Servo's network-error page â€” *"Could not load the requested page:
Unsupported scheme"* â€” instead of the game. Linux (`play.sh`) and macOS (`Roves.app`) were
unaffected.

**Cause:** `parse_url_or_filename` tries `ServoUrl::parse(input)` first and only falls back to
"treat this as a filename" on `ParseError::RelativeUrlWithoutBase`. An absolute Windows path
never produces that error: per the WHATWG URL spec, `C:\dir\index.html` parses *successfully*
as scheme `c` with the opaque path `\dir\index.html` (a single letter is a valid scheme). So
`get_default_url` sees a URL whose scheme isn't `file`, whose `to_file_path()` fails, and whose
scheme is neither localhost nor domain-like â€” every arm misses, it falls through to
`location_bar_input_to_url`, which re-parses the same string and hands back that same `c:` URL.
The engine then fetches it, `components/net/fetch/methods.rs`'s `scheme_fetch` finds no handler
registered for `c`, and returns `NetworkError::UnsupportedScheme`. POSIX absolute paths dodge
all of this because a leading `/` genuinely is not a scheme, so they do fail to parse and do
reach the filename fallback â€” which is why this only ever bit Windows.

The bug is latent upstream (`servo.exe C:\page.html` has presumably always behaved this way),
but only became *reachable for Roves* with the lazy-extraction entry above: `play.exe` now
passes an **absolute** html path â€” `roves-content-packer extract`'s printed cache directory,
which isn't known until launch time â€” where it previously passed a bundle-relative path that
parsed as a relative URL and worked fine.

**Change:** before the existing `ServoUrl::parse` attempt, detect the two absolute-Windows-path
shapes (`C:\â€¦` / `C:/â€¦` drive-absolute, and `\\server\share\â€¦` UNC) with a new
`is_windows_absolute_path` helper, and for those hand the string straight to
`url::Url::from_file_path`. On Windows that yields the correct `file:///C:/â€¦` URL with no host
(exactly what `get_default_url`'s `("file", None, Ok(path))` arm expects) and with each segment
percent-encoded properly. Everything else takes the original path unchanged.

**Why here and not in the launcher:** hand-assembling a `file:///` string inside the generated
`play.exe` (which is compiled by a bare `rustc` invocation with no dependencies available, so
no `url` crate) would mean hand-rolling percent-encoding for paths containing spaces, `#` or
`?` â€” and Windows temp directories live under the user's profile, so `C:\Users\Mario
Rossi\AppData\Local\Temp\â€¦` is entirely ordinary, not an edge case. Fixing it in the parser
gets correct encoding from `Url::from_file_path` for free and makes *any* Windows path passed
to servoshell on the command line work, not just the launcher's.

**Deliberately not `#[cfg(windows)]`-gated:** the conversion only happens if
`Url::from_file_path` succeeds, and on a non-Windows build it rejects both shapes (neither is
an absolute path there), so behavior off Windows is provably unchanged â€” one code path to
reason about instead of two.

**Known limit, left alone:** the UNC shape now parses into a `file://server/share/â€¦` URL, whose
host is `Some`, so `get_default_url`'s `("file", None, â€¦)` arm still won't take it and a UNC
argument still falls through to the homepage. That's unchanged from before this fix (it
previously failed even earlier), Roves' launchers never generate one, and widening that arm is
a separate judgment call about whether an embedded game should load its content off a network
share at all.

**Verification:** the WHATWG parse behavior was confirmed directly against `url` 2.5.4 â€”
`Url::parse(r"C:\Users\me\AppData\Local\Temp\roves-content-ab12/index.html")` returns
`Ok`, scheme `"c"`, `to_file_path()` `Err`; the POSIX equivalent returns
`Err(RelativeUrlWithoutBase)`. The Windows side of `from_file_path`
(`path_to_file_url_segments_windows`) was read to confirm it maps a `Prefix::Disk` to a
host-less `file:///C:/â€¦` serialization and percent-encodes each segment, and that mixed
separators (`C:\dir/index.html`, exactly the shape `play.exe`'s `format!` produces) are handled
â€” `Path::components()` splits on both on Windows. Not exercised by a compiled Windows build in
this environment; confirm with a real `mach bundle` + `play.exe` run.

---

## 2026-08-08 â€” Single-executable bundle: eliminate the separate launcher

**Files:** `support/content-packer/src/manifest.rs`/`pack.rs` (new `Manifest::entry_html`
field, `FORMAT_VERSION` bumped to 3), a brand-new `ports/servoshell/desktop/bundle_launch.rs`,
plus `ports/servoshell/desktop/mod.rs`/`desktop/cli.rs` (registration/call site),
`ports/servoshell/build.rs` (Linux rpath), `ports/servoshell/Cargo.toml` (winresource
metadata), `ports/servoshell/platform/windows/servoshell.exe.manifest` (assembly identity),
and `python/servo/post_build_commands.py` (every `_bundle_*` method, `bundle()` itself).

**Patch:** `patches/servo-v0.4.0/0017-single-executable-bundle.patch`

**Motivating problem:** every `mach bundle` output shipped **two** executables â€” a tiny
generated launcher (`play.exe` / `play.sh` / a bash script inside `Roves.app`) plus the real
engine binary, hidden in a `bin/` subdirectory (Windows, still literally named
`servoshell.exe` there) or under a `<binary>-core` suffix (macOS/Linux). The launcher's only
job was: run `roves-content-packer extract` (also shipped into the bundle, a *third*
executable), capture the cache directory it printed, and spawn/exec the real binary with the
resolved html path + `--window-size` + extra args. Noticed directly: a Windows user found
`servoshell.exe` sitting in `bin/` and asked for exactly one executable, named `play`,
everywhere â€” not just hidden better.

**Change:** the engine binary itself now does what the launcher used to do, in-process,
before opening any window â€” and is shipped as the single executable directly, under the
`play`/`play.exe`/`Roves` name. Concretely:

- `roves_content_packer::extract` was already linked into `servoshell` as a library (used by
  `desktop/protocols/file.rs` for on-demand lazy extraction â€” see the entry above) â€” the new
  `desktop/bundle_launch.rs` module calls the exact same `load_manifest`µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥âöW‡G&7Eö&ö÷F ¢gVæ7F–öç2F†RÆVæ6†W"W6VBFò&V6‚f–7V'&ö6W72Â6òæò&÷fW2Ö6öçFVçB×6¶W&&–æ'¢æVVG2Fò6†—FòÆ–W'2BÆÂç–Ö÷&R‡7F–ÆÂ'V–ÇBæBW6VBÆö6ÆÇ’ÂöâF†RÖ6†–æP¢'Vææ–ærÖ6‚'VæFÆVÂFò'Vâ6¶(	B6VRö'V–ÆEö6öçFVçE÷6¶W&w2WFFVBFö77G&–ær’à¢ÒÖ6‚'VæFÆVw&—FW26ÖÆÂÆVæ6‚æ§6öææW‡BFòF†R6†—VB&–æ'’–ç7FVBö`¢vVæW&F–ær¶6ö×–Æ–ær÷w&—F–ærw&W"â6†S¢²&6öçFVçEöF—"#¢&F—7B"Â'W&Â#¢çVÆÂÀ¢&&w2#¢²"Ò×v–æF÷r×6—¦R"Â##ƒƒs#"Âââå×Öf÷"6¶VB6öçFVçBÂ÷"²&6öçFVçEöF—"#¢çVÆÂÀ¢'W&Â#¢&F—7Bö–æFW‚æ‡FÖÂ"Â&&w2#¢²ââå×Öf÷"ÒÖ6öçFVçBÖ6ö×&W73ÖæöæVöÆö÷6R'V–ÆG2âÆÀ¢F‡2&VÆF—fRFòÆVæ6‚æ§6öæw2÷vâF—&V7F÷'’à¢Ò'VæFÆUöÆVæ6ƒ£§&W6öÇfUö'VæFÆVEöÆVæ6…ö&w2‚–Â6ÆÆVBg&öÒFW6·F÷ö6Æ’ç'6&–v‡Bv†W&P¢Vçc£¦&w2‚–W6VBFò&R&VBF—&V7FÇ“¢Æöö·2f÷"ÆVæ6‚æ§6öææW‡BFğ¢Vçc£¦7W'&VçEöW†R‚–†öâÖ4õ2Â6öçFVçB—G6VÆb—2&W6öÇfVBöæRÆWfVÂW–çFòF†R6–&Æ–æp¢6öçFVçG2õ&W6÷W&6W2öÂÖF6†–ærF†R7FæF&BÖ'VæFÆRÆ–÷WB(	BÆVæ6‚æ§6öæ—G6VÆb7F–ÆÀ¢6—G2æW‡BFòF†R&–æ'’–â6öçFVçG2ôÖ4õ2ö’â–b6öçFVçEöF—&—2&W6VçBÂ&W6öÇfW2—BÀ¢6ÆÇ2W‡G&7Eö&ö÷FÂæBW6W2FW7Bæ¦ö–â‚fÖæ–fW7BæVçG'•ö‡FÖÂ–2F†R&W6öÇfVBU$Â(	Bv†–6€¢—2§v‡’¢Öæ–fW7Fv–æVBVçG'•ö‡FÖÆ†'V×VBFòf÷&ÖBc2“¢F†R7GVÂVçG'’f–ÆRFò÷Và¢gFW"W‡G&7F–öâv2&Wf–÷W6Ç’¶æ÷vâöæÇ’&V6W6RV6‚vVæW&FVBÆVæ6†W"†B—B&¶V@¢–çFò—G2÷vâ6÷W&6RB'VæFÆRF–ÖS²æ÷rF†RVæv–æRæVVG2FòÆöö²—BW—G6VÆbÂæBF†P¢Öæ–fW7B—2F†RöæÇ’F†–ær&÷F‚6¶†FWbÖ6†–æR’æBF†RVæv–æR‡Æ–W"w2Ö6†–æR¢6†&RâfÆÇ2&6²FòF†RVæv–æRw2æ÷&ÖÂ&VfW&Væ6RÖ&6VBFVfVÇB‡&F†W"F†â7&6†–ær’–`¢W‡G&7F–öâf–Ç2ÂæB&WGW&ç2F†R&W6öÇfVB&w2–âW†7FÇ’F†R6†RF†RöÆBÆVæ6†W'0¢Ç&VG’76VB2&wb†·W&ÂÂ"Ò×v–æF÷r×6—¦R"Â%w„‚"ÂââåÖ’Â6òæ÷F†–ærF÷vç7G&VĞ¢†&Vg2ç'6w2'f'6–ærÂ'6W"ç'6w2vWEöFVfVÇE÷W&ÆÂç'6’æVVFVBFò6†ævR@¢ÆÂà¢Ò¢¤7&—F–6Â6fWG’'VÆS¢¢¢&W6öÇfUö'VæFÆVEöÆVæ6…ö&w2‚–öæÇ’WfW"6öç7VÇG2ÆVæ6‚æ§6öæ ¢v†VâF†R&ö6W72w2&VÂ&wf—26ö×ÆWFVÇ’V×G’âF†—2—6âwB§W7B6öç6W'fF—6Ò(	B6W'fòw0¢÷vâ×VÇF—&ö6W72ÖöFR&RÖW†V7WFW2F†R'Vææ–ær&–æ'’26öçFVçB×&ö6W726†–ÆG&Vâv—F€¢ÒÖ6öçFVçB×&ö6W72ÇFö¶Vãæ–âF†V—"&wb†&Vg2ç'6w26öçFVçE÷&ö6W76f–VÆB“²öæ6RF†P¢ÆVæ6†W"æBF†RVæv–æR&RF†R6ÖR&–æ'’ÂF†B6†–ÆB&VÆVæ6‚v÷VÆB÷F†W'v—6Rf–æ@¢ÆVæ6‚æ§6öæ&–v‡BæW‡BFò—G6VÆbFöòæB6–ÆVçFÇ’F—66&B—G2&VÂ7F'GW&w2âfW&–f–V@¢×VÇF—&ö6W76FVfVÇG2fÇ6VæBÖ6‚'VæFÆVæWfW"F‡&VG2ÔÖöÒÖ×VÇF—&ö6W76 ¢F‡&÷Vv‚FöF’Â6òF†—2v2ÆFVçB&F†W"F†â7F—fVÇ’G&–vvW&VB(	B'WB—Bw2&VÀ¢6÷'&V7FæW72&WV—&VÖVçBf÷"ç’gWGW&RÖ6‚'VæFÆRÒÒÔÖ†÷"&ö¦V7BF†BWfW"Væ&ÆW0¢—B’Âæ÷B§W7BFVfVç6—fR7G–ÆRââW‡Æ–6—B–çfö6F–öâ†FWfVÆ÷W"'Vææ–ærF†R6†—V@¢&–æ'’g&öÒFW&Ö–æÂÂ7FVÒÆVæ6‚Ö÷F–öç2÷fW'&–FR’†—G2F†R6ÖR'VÆRæB—2Æ–¶Wv—6P¢Çv—2ÆVgBVçF÷V6†VBà¢ÒÆ–çW‚æWfW"†BÆ–æ¶W"'F‚‡VæÆ–¶RF†RÖ4õ2×'F‚W†V7WF&ÆU÷F‚öÆ–"ö&Wf–÷W0¢VçG'’FFVB’(	Bç6ö&W6öÇWF–öâv2VçF—&VÇ’Æ’ç6†w2¦ö"Â6WGF–ærÄEôÄ”%$%•õD† ¢&Vf÷&RW†V2âv—F‚F†Bw&W"vöæRÂ'V–ÆBç'6æ÷rÇ6òVÖ—G0¢ÕvÂÂ×'F‚ÂDõ$”t”æf÷"Æ–çW‚Â6òF†R6–ævÆR&–æ'’f–æG2—G26–&Æ–ærç6öf–ÆW2—G6VÆbà¢Òö'VæFÆUöÆ–çW…öFV&w2÷W7"ö&–âóÇ6¶vUöæÖSæ—2æ÷rÆ–â7–ÖÆ–æ²FòF†R&VÂ&–æ'¢VæFW"÷W7"öÆ–"óÇ6¶vUöæÖSâöÂæ÷Bw&W"67&—B(	B7–ÖÆ–æ²—2f–ÆW7—7FVÒÆ–2À¢æ÷B6V6öæB&ö6W72ÂæBVçc£¦7W'&VçEöW†R‚–‡v†B'VæFÆUöÆVæ6‚ç'6Æöö·2æW‡BFò¢&W6öÇfW2F‡&÷Vv‚—Bf–÷&ö2÷6VÆböW†VWFöÖF–6ÆÇ’âöæR&VÂ'Vr6Vv‡Bv†–ÆRw&—F–æp¢F†—3¢–ç7FÆÆVE÷6—¦Uö¶&w2÷2çvÆ¶²F‚ævWG6—¦V6öÖ&–æF–öâv÷VÆB†fR&—6V@¢f–ÆTæ÷Df÷VæDW'&÷&G'––ærFòföÆÆ÷rF†B7–ÖÆ–æ²w2'6öÇWFRF&vW@¢†÷W7"öÆ–"óÇ¶sâóÆ&–æ'“æ’Âv†–6‚FöW6âwBW†—7BöâF†R¦'V–ÆB¢Ö6†–æR(	B7v—F6†VBFğ¢÷2æÇ7FB‚âââ’ç7E÷6—¦V‡F†R7–ÖÆ–æ²w2÷vâF–ç’6—¦RÂÖF6†–ær†÷rGV÷&VÂG¶rÖFV& ¢66÷VçBf÷"7–ÖÆ–æ·2’Fò7GVÆÇ’fW&–g’F†—2&Vf÷&R—B6†—VBà¢ÒGvòÆ6W2&¶VBÆ—FW&Â%6W'fõ6†VÆÂ"7G&–ær–çFòF†R¦'F–f7B—G6VÆb¢†æ÷B§W7B¢f–ÆVæÖRÂ6ò&VæÖ–ærF†R6†—VBf–ÆRÆöæRv÷VÆFâwB†fRW&6VBF†VÒÂVæÆ–¶Rv†VâF†P¢†–FFVâ6W'f÷6†VÆÂæW†V6B6fVÇ’–ç6–FR&–âö“¢6&vòçFöÖÆw0¢·6¶vRæÖWFFFçv–ç&W6÷W&6UÖ†f–ÆTFW67&—F–öæö&öGV7DæÖVö÷&–v–æÄf–ÆVæÖVÂ&V@¢'’v–ç&W6÷W&6V–çFòF†RæW†Vw2v–æF÷w2fW'6–öâ&W6÷W&6R(	Bf—6–&ÆRf–W‡Æ÷&W"w0¢&÷W'F–W2(i"FWF–Ç2’æB6W'f÷6†VÆÂæW†RæÖæ–fW7Fw276VÖ&Ç”–FVçF—G’æÖSÖâ&÷F‚æ÷r6¢%&÷fW2"ö÷&rç&÷fW2å&÷fW6ÂÖF6†–ærF†RtÒÖ6Æ727G&–ær†VFVE÷v–æF÷rç'6Ç&VG’W6VBà¢¢¤FVÆ–&W&FVÇ’F–B¦æ÷B¢F÷V6‚¢¢ö'VæFÆUöÖ6÷6w2–æfòçÆ—7F4d'VæFÆT–FVçF–f–W& ¢†÷&rç6W'fòç6W'f÷6†VÆÂæ'VæFÆV’WfVâF†÷Vv‚—Bw2F†R6ÖR¶–æBöbÆV¶VB7G&–ær(	BF†P¢##bÓ‚Ór&VæÖRVçG'’&÷fRÇ&VG’6öç6–FW&VBF†—2W†7B7G&–æræBFVÆ–&W&FVÇ¢FVfW'&VB—B†'VæFÆR–FVçF–f–W"ffV7G2Ö4õ2ÖÆWfVÂ–FVçF—G’(	BFVfVÇG2÷&Vg2ÂD40¢W&Ö—76–öâw&çG2Â6öFR6–væ–ær(	BVæÆ–¶RF—7Æ’Æ&VÂÂ6ò—BæVVG2—G2÷vâW‡Æ–6—@¢FV6—6–öâ’â6Vv‡BæB&WfW'FVBf—'7B72B6†æv–ær—Bç—v’&Vf÷&Rf–æÆ—¦–ærF†—0¢VçG'“²ÆVgB6öÖÖVçBBF†R6ÆÂ6—FRö–çF–ær&6²BF†B&V6öæ–ærâÇ6òFVÆ–&W&FVÇ¢ÆVgB&W6÷W&6W2ö÷&rç&÷fW2å&÷fW2æFW6·F÷ÆöæR(	B—Bw2FWbÖöæÇ’–ç7G'V7F–öç2f÷"–ææ–ær¢Æö6Â'V–ÆBFò–÷W"÷vâF6¶&"†ÖVçF–öç2%6W'fò6÷W&6W2"67W&FVÇ’f÷"F†BW6R’Âæ÷@¢ç—F†–ærÖ6‚'VæFÆV7GVÆÇ’&öGV6W3²F†R&VÂÒÖFV&FW6·F÷ÖVçG'’vVæW&F–öâv0¢Ç&VG’6ÆVââÇ6ò6†ævVBÒÖFV"×6¶vRÖæÖVw2FVfVÇBg&öÒ6W'f÷6†VÆÆFò&÷fW6à ¢¢¥v‡“¢¢¢F†RW6W"w26²v27V6–f–6ÆÇ’'F†RöæÇ’W†R6†÷VÆB&RÆ’Âæ÷B6W'f÷6†VÆÂÀ¦ç—v†W&R"(	B†–F–ærF†R6V6öæB&–æ'’&WGFW"†2Ö4õ2ôÆ–çW‚Ç&VG’F–Bv—F‚F†RÖ6÷&V §7Vff—‚’v6âwBVæ÷Vvƒ²&VÂ6V6öæB&–æ'’7F–ÆÂW†—7FVBFò&Rf÷VæBâÖ¶–ærF†RVæv–æP¦'6÷&"F†RÆVæ6†W"w2¦ö"&VÖ÷fW2—BVçF—&VÇ’&F†W"F†â†–F–ær—BgW'F†W"ÂæBv0¦g&VRÖöbÖæWrÖFWVæFVæ6–W26–æ6R&÷fW5ö6öçFVçE÷6¶W&v2Ç&VG’Æ–æ¶VB–â2Æ–'&'’f÷ ¦âVç&VÆFVB&V6öâ†öâÖFVÖæBÆ§’W‡G&7F–öâ’à ¢¢¥fW&–f–6F–öã¢¢¢7W÷'Bö6öçFVçB×6¶W&†VçG'•ö‡FÖÆödõ$ÔEõdU%4”ôæ'V×’6ö×–ÆW2æ@§FW7G26ÆVæÇ’–âÆ–â6æF&÷‚(	BæòÆ–&6Æævö&–æFvVæFWVæFVæ7’(	B6öæf—&ÖVBv—F€¦6&vòFW7BÒÖÖæ–fW7B×F‚7W÷'Bö6öçFVçB×6¶W"ô6&vòçFöÖÆ¢ÆÂ&RÖW†—7F–æp¦FW7G2÷&÷VæGG&—ç'666W27F–ÆÂ72ÂÇW2æWr76W'F–öâF†BÆöEöÖæ–fW7B‚âââ’æVçG'•ö‡FÖÀ£ÓÒ&–æFW‚æ‡FÖÂ&â'VæFÆUöÆVæ6‚ç'6w2W&RÆöv–2„¥4ôâ'6–ærÂF†RÖ4õ2ââõ&W6÷W&6W6 ¦¦ö–âÂ&VÂW‡G&7Eö&ö÷F6ÆÂv–ç7B6¶VBf—‡GW&R’v2FF—F–öæÆÇ’W†W&6—6VB–âà¦—6öÆFVB7FæFÆöæR'W7B&öw&ÒW6–ærF†—2v÷&·76Rw27GVÂ7&FRfW'6–öç2â¢¥F†P¦6W'f÷6†VÆÆö6W'fö7&FW2F†V×6VÇfW2vW&Ræ÷B6ö×–ÆVB¢¢(	B6ÖR&RÖW†—7F–æp¦Æ–&6Æævö&–æFvVævæ÷FVB–âF†RVçG'’&÷fRæBVÇ6Wv†W&R–âF†—2f–ÆRâ—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç– §v27–çF‚Ö6†V6¶VB†7Bç'6Vö•ö6ö×–ÆV’'WBæ÷B'VâVæB×FòÖVæB†æò&VÂÖ6‚'V–ÆF–à§F†—2Vçf—&öæÖVçB’â¢¤æVVG2&VÂâöÖ6‚'V–ÆBbbâöÖ6‚'VæFÆVFò6öæf—&ÒVæB×FòÖVæB¢¢(	@§F†R&Wf–÷W2VçG'’w2v–æF÷w2F‚×'6–ærf—‚v2Ç&VG’6öæf—&ÖVBv÷&¶–æröâF†RW6W"w2÷và¥v–æF÷w2Ö6†–æS²F†—26†ævR6†÷VÆB&RfW&–f–VBF†W&RFöò‡6–ævÆRÆ’æW†VÂæò&–âöÂvÖP¦7GVÆÇ’ÆVæ6†W2’ÂæB–FVÆÇ’7÷BÖ6†V6¶VB'’f–ÆRÆ—7F–æröâÆ–çW‚öÖ4õ2–bf–Æ&ÆRÀ§F†÷Vv‚gVÆÂ'VçF–ÖRFW7F–ærF†W&R—6âwBW‡V7FVB–âF†—272à ¢ÒÒĞ ¢22##bÓ‚Ó’(	BæF—fR&ö÷B7Æ6‚–ç7FVBöb&Ææ²v–æF÷rGW&–ærf—'7B×'VâW‡G&7F–öà ¢¢¤f–ÆW3¢¢¢7W÷'Bö6öçFVçB×6¶W"÷7&2öW‡G&7Bç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö'VæFÆUöÆVæ6‚ç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö6Æ’ç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öWfVçEöÆö÷ç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷öç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷G&6–ærç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó‚ÖæF—fRÖ&ö÷B×7Æ6‚×67&VVâçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢f÷"6¶VBÖ6öçFVçB'VæFÆRÂ&W6öÇfUö'VæFÆVEöÆVæ6…ö&w2‚– ¢†'VæFÆUöÆVæ6‚ç'6Â6VRF†R%6–ævÆRÖW†V7WF&ÆR'VæFÆR"VçG'’&÷fR’6ÆÆV@¦W‡G&7C£¦W‡G&7Eö&ö÷F(	BF†R7GVÂÂ÷FVçF–ÆÇ’6Æ÷rFV6ö×&W76–öâöbF†R&ö÷B6²6WB(	@§7–æ6‡&öæ÷W6Ç’Â–â6Æ’ç'6Â¦&Vf÷&R¢F†Rv–æ—BWfVçBÆö÷÷"v–æF÷rWfVâW†—7FVBâöâ6Æ÷p¦f—'7BÆVæ6‚†Æ&vR&ö÷B6WB’ÂF†W&Rv2æ÷F†–æröâ67&VVâBÆÂGW&–ærF†Bv—C²öæ6RF†P§v–æF÷rF–BV"Â—B6†÷vVB&Ææ²÷Væ–æ—F–Æ—¦VBg&ÖRVçF–Â6W'fòw26ö×÷6—F÷"–çFVBF†P§&VÂvRöâF÷âæò7Æ6‚öÆöF–ær×67&VVâÖV6†æ—6ÒW†—7FVBBÆÂ†Fö26öÖÖVçB–à¦6²ç'6fÆöFVBF†R–FVöbâ–â×vR…DÔÂ7Æ6‚–ÖvRf–ÒÖ&ö÷BÖ–æ6ÇVFVÂ'WBæ÷F†–æp¦–×ÆVÖVçFVB—BÂæF—fR÷"÷F†W'v—6R’à ¢¢¤6†ævS¢¢¢F†Rv–æF÷ræ÷rV'2–ÖÖVF–FVÇ’Â6†÷v–ærÖ–æ–ÖÂæF—fR7Æ6‚†&Æ6°¦&6¶w&÷VæBÂF†R&÷fW2–6öâ²%&÷fW2"–âv†—FRÂæB(	BgFW"ã3×2FVÆ’Â6òf7BöæòÖ÷ ¦W‡G&7F–öâæWfW"fÆ6†W2öæR(	B&öw&W72&"G&6¶–ær&VÂW‡G&7F–öâ&öw&W72’–ç7FVBö`§v†FWfW"v2F†W&R&Vf÷&RâFVÆ–&W&FVÇ’Vç7G–ÆVB&W–öæBF†C²7G–Æ–ær—2föÆÆ÷r×Wà ¢ÒW‡G&7Bç'6¢&W6öÇfUöFW7F—2&W&UöFW7Fw26æöæ–6Æ—¦RÖæB×–6²ÖFW7F–æF–öâ†ÆbÀ¢7Æ—B÷WBæBÖFRV&6ò6ÆÆW"6âÆV&â§v†W&R¢&ö÷B6öçFVçBv–ÆÂÆæB‡Fò'V–ÆBF†P¢f–ÆS¦U$Â—BvÆÂWfVçGVÆÇ’ÆöB’v—F†÷WB––ærf÷"FV6ö×&W76–öââW‡G&7Eö&ö÷Fw2&öG¢—2f7F÷&VB–çFò&—fFRW‡G&7Eö&ö÷Eö–×Â†÷G2Âöå÷&öw&W73¢÷F–öãÂf×WBG–à¢fä×WB†c3"“â–²W‡G&7Eö&ö÷F‡Væ6†ævVB6–væGW&RÂÆÂW†—7F–ær6ÆÂ6—FW2(	BF†R4Ä’ÂF†P¢b&÷VæGG&—ç'6FW7G2(	BVçF÷V6†VB’6ÆÇ2—Bv—F‚æöæVâæWrV"fà¢W‡G&7Eö&ö÷E÷v—F…÷&öw&W72†÷G2Âöå÷&öw&W73¢–×Âfä×WB†c3"’–6ÆÇ2—Bv—F‚6öÖVÀ¢&W÷'F–ærFöæR÷F÷FÆgFW"V6‚&ö÷B6²ÇW2f–æÂã(	B6ö'6R‡W"×6²Âæ÷@¢W"Ö'—FR’'WB&ö÷B6WG2&RFVÆ–&W&FVÇ’§W7BF†R‡FÖÂf–ÆRæBv†FWfW"—BF—&V7FÇ¢&VfW&Væ6W2Â6òW7VÆÇ’öæÇ’öæR÷"Gvò6·2âæòÖæ–fW7Bödõ$ÔEõdU%4”ôæ6†ævW2æVVFVBà¢Ò'VæFÆUöÆVæ6‚ç'6¢&W6öÇfU÷6¶VEö6öçFVçE÷W&ÆæòÆöævW"6ÆÇ2W‡G&7Eö&ö÷FBÆÂ(	@¢öæÇ’W‡G&7C£§&W6öÇfUöFW7F‡F‚ö†6‚ÖF‚’ÇW2F†RÇ&VG’ÖÆöFVBÖæ–fW7Bw0¢VçG'•ö‡FÖÆÂ&÷F‚f7Bâ&W6öÇfUö'VæFÆVEöÆVæ6…ö&w2‚–æ÷r&WGW&ç2÷F–öãÄ'VæFÆVDÆVæ6ƒæ ¢†²&w2ÂVæF–æuö&ö÷EöW‡G&7F–öã¢÷F–öãÄW‡G&7D÷F–öç3âÖ’–ç7FVBö`¢÷F–öãÅfV3Å7G&–æsãæ(	BF†R6ÆÆW"FV6–FW2v†Vâö†÷rFò7GVÆÇ’'VâF†R7F–ÆÂ×VæF–æp¢W‡G&7F–öâ–ç7FVBöb—B†Væ–ær–æÆ–æRà¢Ò6Æ’ç'6¢FW7G'V7GW&W2'VæFÆVDÆVæ6†æBF‡&VG2VæF–æuö&ö÷EöW‡G&7F–öæF‡&÷Vv‚Fò¢æWr£¦æWv&ÖWFW"ÂVç&W6öÇfVBà¢ÒWfVçEöÆö÷ç'6¢GvòæWrWfVçFf&–çG2Â&ö÷E&öw&W72†c3"–æB&ö÷E&VG–Â6VçB'¢F†R&6¶w&÷VæBW‡G&7F–öâF‡&VB£¦–æ—F7vç2‡6VR&VÆ÷r’(	BÖ—'&÷'2F†RW†—7F–æp¢&3Ä×WFWƒÄWfVçDÆö÷&÷‡“ÄWfVçCããæ&6¶w&÷VæB×F‡&VB×FòÖÖ–â×F‡&VBGFW&âÇ&VG’W6V@¢'’†VFVDWfVçDÆö÷v¶W&æB&÷Fö6öÇ2÷&÷fW2ç'6w2&÷fW5&÷Fö6öÄ†æFÆW&Â6–×Æ–f–VBFò¢6–ævÆR÷væVBWfVçDÆö÷&÷‡–6ÆöæR†æò6†&–æræVVFVBf÷"öæRF‡&VB’à¢ÒG&6–ærç'6¢—G2ÆöuF&vWF–×Âf÷"v–æ—C£¦WfVçC£¤WfVçCÄWfVçCæGFW&âÖÖF6†W2WfW'¢WfVçFf&–çB'’æÖR†f÷"F†R%U5EôÄôsÒw6W'f÷6†VÆÃÇv–æ—DâââvG&6Rf–ÇFW'2F†—0¢f–ÆRFö7VÖVçG2’(	BFF–ærF†RGvòf&–çG2&÷fRv—F†÷WBÖF6†–ær&Ò†W&R—26ö×–ÆP¢W'&÷"†æöâÖW††W7F—fRÖF6‚’Âæ÷B§W7BÖ—76VBÆörÆ–æRâFFV@¢W6W$WfVçB„&ö÷E&öw&W72–öW6W$WfVçB„&ö÷E&VG’–F&vWG2Æöæw6–FRF†RW†—7F–æröæW2à¢Òç'6¢æWr7FFS£¤&ö÷F–ær²v–æF÷rÂW‡G&7F–öå÷7F'FVBÂ&öw&W72Öf&–çBâ ¢v–æVBVæF–æuöW‡G&7F–öæf–VÆB‡F†RW‡G&7D÷F–öç6g&öÒ6Æ’ç'6Â6öç7VÖVB'’F†P¢f—'7B–æ—F6ÆÂ’â–æ—Fæ÷rÇv—27&VFW2F†RÆFf÷&Òv–æF÷r–ÖÖVF–FVÇ’‡v–æF÷p¢7&VF–öâæWfW"FWVæFVBöâF†R&ö÷BU$Â&V–ær&VG’(	B†VFVEv–æF÷s£¦æWvöwV“£¦æWvFöâw@¢F÷V6‚—B&W–öæBâÇ&VG’ÖFVBö–æ—F–Å÷W&Æ&ÖWFW"’ÂF†Vã¢v—F‚æòVæF–ærW‡G&7F–öâÀ¢&ö6VVG2W†7FÇ’2&Vf÷&R†Ö÷fVB–çFòæWrf–æ—6…ö–æ—F†VÇW"“²†VFÆW72v—F‚VæF–æp¢W‡G&7F–öâÂ'Vç2W‡G&7Eö&ö÷F7–æ6‡&öæ÷W6Ç’2&Vf÷&R†æò7Æ6‚Fò6†÷r“²†VFVBv—F‚¢VæF–ærW‡G&7F–öâÂ7vç2&6¶w&÷VæBF‡&VB'Vææ–ærW‡G&7Eö&ö÷E÷v—F…÷&öw&W76‡6VæF–æp¢&ö÷E&öw&W76ö&ö÷E&VG–&6²’ÂVçFW'2&ö÷F–ævÂæBFVfW'2f–æ—6…ö–æ—F†'V–ÆF–ærF†P¢&÷Fö6öÂ&Vv—7G'’Â6W'fòÂ'Vææ–æt7FFVÂæB÷Væ–ærF†R&VÂvV'f–Wr’VçF–À¢WfVçC£¤&ö÷E&VG–'&—fW2âæWræWuöWfVçG6†öö²f÷&6W2öæR&VG&rv†VâF†R7Æ6‚w0¢&öw&W72Ö&"FVÆ’VÆ6W2WfVâv—F†÷WBg&W6‚&ö÷E&öw&W76F–6²âv–æF÷uöWfVçFğ¢W6W%öWfVçF&÷F‚v–æVB&ö÷F–æv'&æ6‚‡–çBF†R7Æ6‚òWFFR&öw&W72æB†æBöf`¢Fòf–æ—6…ö–æ—F’†VBöbF†RW†—7F–ær'Vææ–ævÖöæÇ’Æöv–2à ¢öæR6÷'&V7FæW72Ö7&—F–6Â÷&FW&–ærFWF–Ã¢f–æ—6…ö–æ—F†æ÷B–æ—F’—2v†B6öç7G'V7G0¢&÷Fö6öÇ3£¦f–ÆS£¤f–ÆU&÷Fö6öÄ†æFÆW&Âv†–6‚Æöö·2f÷"F†Rç&÷fW2Ö6öçFVçB×6÷W&6VÖ&¶W ¢W‡G&7F–öâw&—FW2(	BÖ÷f–ær¦ÆÂ¢öb&÷Fö6öÂ×&Vv—7G'’6WGW–çFòf–æ—6…ö–æ—F†æ÷B§W7BF†P¢6W'fò÷vV'f–Wr'B’ÖVç2F†BÖ&¶W"Çv—2W†—7G2'’F†RF–ÖRF†R†æFÆW"—2'V–ÇBÀ¢v†WF†W"W‡G&7F–öâ&â7–æ6‡&öæ÷W6Ç’††VFÆW72’÷"f–æ—6†VBöâF†R&6¶w&÷VæBF‡&VBf—'7@¢††VFVBÂ&ö÷E&VG–f—&W2öæÇ’gFW"W‡G&7Eö&ö÷E÷v—F…÷&öw&W76&WGW&ç2’â'V–ÆF–ærF†P¢†æFÆW"ç’V&Æ–W"v÷VÆB6–ÆVçFÇ’F—6&ÆRöâÖFVÖæBÆ§’W‡G&7F–öâf÷"F†RVçF—&R6W76–öâà ¢6V6öæBÂV7’×FòÖÖ—72–ç7Fæ6RöbF†R6ÖR÷&FW&–ær†¦&C¢6VÆbæ–æ—F–Å÷W&Æ(	B&Wf–÷W6Ç¢6ö×WFVBöæ6RÂVvW&Ç’Â–â£¦æWv‡f–vWEöFVfVÇE÷W&ÆÂv†–6‚öæÇ’G'W7G2f–ÆS¦ ¢U$ÂFW&—fVBg&öÒF‚&wVÖVçB–bg3£¦ÖWFFF6öæf—&×2F†BF‚¦Ç&VG’W†—7G2¢À¢'6W"ç'6’(	B—2æ÷r6ö×WFVB–âf–æ—6…ö–æ—F–ç7FVBÂ¦gFW"¢ç’VæF–ærW‡G&7F–öâ†0¢7GVÆÇ’f–æ—6†VBâÆVf–ær—B–â£¦æWv†2÷&–v–æÆÇ’w&—GFVâ’&V–çG&öGV6W2W†7FÇ’F†P¢'Vr'6W"ç'6w2÷vâ$66WB'6öÇWFRv–æF÷w2F‡2"f—‚‡6VRF†BVçG'’&÷fR’v2w&—GFVà¢Fò&WfVçBÂ§W7BöæRÆ–W"W¢öâvVçV–æRf—'7BÆVæ6‚†V×G’66†R’Â£¦æWv'Vç0¢¦&Vf÷&R¢W‡G&7F–öâ†2†VæVBÂ6òF†R&ö÷B‡FÖÂw2'6öÇWFRF‚FöW6âwBW†—7BöâF—6°¢–WB(	BvWEöFVfVÇE÷W&ÆFöW6âwBG'W7B—BÂfÆÇ2F‡&÷Vv‚Fò'6–ærF†R&rF‚2U$À¢F—&V7FÇ’ÂæBöâv–æF÷w2F†BÖ—7'6W2F†RG&—fRÆWGFW"2F†R66†VÖRâæWBVffV7C¢f—'7@¢ÆVæ6‚gFW"66†Rv—R6†÷w2$6÷VÆBæ÷BÆöBF†R&WVW7FVBvS¢Vç7W÷'FVB66†VÖR#°¢6Æ÷6–æræB&VÆVæ6†–ærv÷&·2Â&V6W6R'’F†VâW‡G&7F–öâ†2Ç&VG’†VæVBöæ6RæBF†P¢f–ÆRvVçV–æVÇ’W†—7G2â7&VFU÷ÆFf÷&Õ÷v–æF÷vw2W&Æ&wVÖVçB‡7F–ÆÂ6ö×WFVBV&Ç’Â–à¢–æ—F’—2VæffV7FVB'’ç’öbF†—2(	B—Bw2F†R6ÖRÇ&VG’ÖFVB&ÖWFW"wV“£¦æWvæWfW ¢W6W2Â6òÆ6V†öÆFW"†&&÷WC¦&Ææ²&Â£¦æWvw2–æ—F–ÂfÇVRf÷"F†Rf–VÆB’—2f–æP¢F†W&R&Vv&FÆW72öbv†VâF†R&VÂU$Â—2¶æ÷vâà¢ÒwV’ç'6¢æWrwV“£§WFFU÷7Æ6‚‡v–æ—E÷v–æF÷rÂ&öw&W73¢÷F–öãÆc3#â–Â–çFVBf–F†P¢W†—7F–ærVwV”vÆ÷vöwV“£§–çF—VÆ–æRwV“£§WFFVÇ&VG’W6W2(	B&Æ6°¢6VçG&ÅæVÆÂF†R&ö÷B–6öâ†FV6öFVBöæ6R–âwV“£¦æWvg&öÒ&W6÷W&6W2÷6W'fõócBçævf–¢–ÖvS£¦ÆöEög&öÕöÖVÖ÷'–Â–æFWVæFVçBöb†VFVE÷v–æF÷rç'6w2Æ–çW‚õv–æF÷w2ÖöæÇ¢ÆöEö–6öæÂ6–æ6RF†—2×W7BÇ6òv÷&²öâÖ4õ2’Â%&÷fW2"–âv†—FRÂæBÂv†Vâ&öw&W76—0¢6öÖVÂâVwV“£¥&öw&W74&&âW6W2VwV“£¤6VçG&ÅæVÆw2FW&V6FVBF÷ÖÆWfVÂ6†÷v ¢†5¶W‡V7B†FW&V6FVB•Ö(	BF†RæöâÖFW&V6FVB&WÆ6VÖVçB—2†æBÖ'V–ÆF–ærgVÆÂ×v–æF÷p¢V–f–VwV’w2÷vâ–çFW&æÂÖ—6‚V“£¦æWvöV”'V–ÆFW&Fæ6RÂæ÷Bv÷'F‚F†RW‡G&7W&f6P¢f÷"F†—2FVÆ–&W&FVÇ’6–×ÆR7Æ6‚’æB6ÆöæW2F†R–6öâ–ÖvV&Vf÷&R†æF–ær—BFğ¢V’æFF(	B—Bw26GW&VB'’F†R÷WFW"VwV”vÆ÷s£§'Væ6Æ÷7W&R‡v†–6‚×W7B&Rfä×WFÂ6–æ6P¢'Væ6â6öæ6WGVÆÇ’&R–çfö¶VBÖ÷&RF†âöæ6R’Â6òÖ÷f–ær—B–çFòF†R–ææW"6†÷v ¢6Æ÷7W&Rv—F†÷WB6Æöæ–ær—26ö×–ÆRW'&÷"†–ÖvV—6âwB6÷–Â'WB—26†VFò6ÆöæV ¢(	B—B§W7Bw&2FW‡GW&R–B÷6—¦RÂæ÷B—†VÂFF’à¢Ò†VFVE÷v–æF÷rç'6¢æWr–çE÷7Æ6‚‡&öw&W73¢÷F–öãÆc3#â–Â6ÆÆ–ærF†R&÷fRÇW2F†P¢W†—7F–ærwV“£§–çFà ¢¢¥v‡“¢¢¢6¶VBF—&V7FÇ’(	B&Ææ²v–æF÷r†÷"æ÷F†–ærBÆÂ’GW&–ærf—'7B×'VâW‡G&7F–öâ&VG0¦2†ærÂW7V6–ÆÇ’öâ6Æ÷rF—6²v—F‚Æ&vR&ö÷B6WBâ&WW6–ærF†RW†—7F–æp¦wV–öVwV”vÆ÷v÷v–æF÷rÖ6†–æW'’‡&F†W"F†â6V6öæBv–æ—Bv–æF÷röWfVçBÆö÷Âv†–6‚v–æ—@¦FöW6âwB&VÆ–&Ç’7W÷'B7&VF–ærÖ÷&RF†âöæRöbW"&ö6W72’¶WBF†—2FòöæRæF—fRv–æF÷p§F‡&÷Vv†÷WBÂv—F‚W‡G&7F–öâvVçV–æVÇ’'Vææ–ær6öæ7W'&VçFÇ’–ç7FVBöb&Æö6¶–ær7F'GWà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢F†R&ö÷B7Æ6‚w2–6öâ—2Çv—0¦&W6÷W&6W2÷6W'fõócBçævÂFVÆ–&W&FVÇ’æWfW"F†RvÖR×7WÆ–VB–6öâg&öÒF†RVçG'’&VÆ÷r(	@§F†R7Æ6‚—2W‡Æ–6—FÇ’&÷fW2r÷vâ'&æF–ærÖöÖVçBÂæ÷BF†RvÖRw2âf–æ—6…ö–æ—Fw0¦7F—fUöWfVçEöÆö÷&ÖWFW"—2VçW6VBv†Vâ6ö×–ÆVBv—F†÷WBF†RvV'‡&fVGW&R†öæÇ¦6öç7VÇFVB–ç6–FR5¶6fr†fVGW&RÒ'vV'‡""•Ö&Æö6²’(	B&RÖW†—7F–ær¶–æBöb†&ÖÆW70¦VçW6VBf&–&ÆVv&æ–ærF†—26öFV&6RÇ&VG’FöÆW&FW2VÇ6Wv†W&R‡6VRF†P§FööÆ&"×&VÖ÷fÂVçG'’&÷fR’à ¢¢¥fW&–f–6F–öã¢¢¢6&vòFW7BÒÖÖæ–fW7B×F‚7W÷'Bö6öçFVçB×6¶W"ô6&vòçFöÖÆ(	BÆÂ`§&RÖW†—7F–ær&÷VæGG&—ç'666W2ÇW2F†R6—¦VVæ—BFW7B7F–ÆÂ72Væ6†ævVBÂ6öæf—&Ö–æp§F†RW‡G&7Eö&ö÷FöW‡G&7D÷F–öç66–væGW&R—2VçF÷V6†VBf÷"WfW'’W†—7F–ær6ÆÂ6—FRà¤WfW'’æWrö6†ævVBVwV–öv–æ—F’6ÆÂ†VwV“£¤–ÖvS£¦g&öÕ÷FW‡GW&VÂ6öÆ÷$–ÖvS£ ¦g&öÕ÷&v&÷Væ×VÇF—Æ–VFÂFW‡GW&T†æFÆVö6—¦VEFW‡GW&VÂ&öw&W74&&Â6VçG&ÅæVÆög&ÖVÀ¦Æ–6F–öä†æFÆW#£¦æWuöWfVçG6ö7F'D6W6S£¥&W7VÖUF–ÖU&V6†VF’v26†V6¶VBF—&V7FÇ’v–ç7@§F†—2v÷&·76Rw2–ææVBfW'6–öç2r6÷W&6R†VwV–öVwV’×v–æ—Fã3Bã2Âv–æ—Fã3ã2’&F†W §F†â77VÖVBâ¢¦6W'f÷6†VÆÆ—G6VÆb6÷VÆBæ÷B&RgVÆÇ’6ö×–ÆVB–âF†—2Vçf—&öæÖVçB¢¢(	@¦6&vò6†V6²×6W'f÷6†VÆÆv÷B7BF†RW7VÂÆ–&6Æævö&–æFvVæv‡v÷&¶VB&÷VæBv—F€¦Ä”$4ÄäuõD†ö–çFVBBâæG&ö–BäD²w2Æ–&6Æærç6ö’æB6ö×–ÆVB6WfW&Â‡VæG&V@¦FWVæFVæ6–W2–æ6ÇVF–ærVwV–ã3Bã2—G6VÆbv—F‚æòW'&÷'2Â'WBF†Vâ†—BâVç&VÆFVBÂ6æF&÷€¢×7V6–f–2vÆÃ¢Æ–'VFWb×7—6w2'V–ÆB67&—B†&B×&WV—&W27—7FVÒÆ–'VFWbç6f–¦¶rÖ6öæf–vÂv†–6‚—6âwB–ç7FÆÆVB†W&RæB6âwB&R†æò7VFö’(	BF†—2—27—7FVÒÖÆ–'&'¦vÂæ÷B6öFR—77VRÂæB&W&öGV6W2–FVçF–6ÆÇ’WfVâv—F‚F†RvÖWFfVGW&RF—6&ÆVB€§6öÖWF†–ærVÇ6R–âF†RFWVæFVæ7’w&‚Ç6òVÆÇ2—B–â’â¢¤æVVG2&VÂâöÖ6‚'V–ÆFğ¦âöÖ6‚'VæöâÖ6†–æRv—F‚gVÆÂFööÆ6†–âFò6öæf—&ÒVæB×FòÖVæB¢¢(	Bv–æF÷rV'0¦–ÖÖVF–FVÇ’Â7Æ6‚6†÷w2–6öâ·FW‡Bv—F‚æòFVÆ’Â&öw&W72&"V'2öæÇ’gFW"F†RFVÆ¦æB&VfÆV7G2&VÂW‡G&7F–öâ&öw&W72ÂF†VâF†R&VÂvRÆöG2à ¢ÒÒĞ ¢22##bÓ‚Ó’(	BvÖR×7WÆ–VB–6öâ‡v—F‚&÷fW2fÆÆ&6²’f÷"F†Rv–æF÷r÷F6¶&"öW†P ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂö'V–ÆBç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6À¦æv—FGG&–'WFW6âÇ6òFW7B×vRö–æFW‚æ‡FÖÆæBGvòæWrf—‡GW&W2À¦FW7B×vR÷V&Æ–2ö–6öâçævö–6öâæ–6ö(	Bæ÷B'BöbF†RF6‚‡6ÖR2FW7B×vR÷V&Æ–2öw0¦÷F†W"f—‡GW&W2Â6VRF†R%6²vÖR6öçFVçB"VçG'’&÷fS¢Æ–âFW7B76WG2Âæ÷BFW&—fVBg&öĞ¦ç’W7G&VÒf–ÆR’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó’ÖvÖR×7WÆ–VBÖ–6öâÖfÆÆ&6²çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢F†Rv–æF÷r÷F6¶&"–6öâ††VFVE÷v–æF÷rç'6ÂÆ–çW‚õv–æF÷w2öæÇ’’æ@§F†R6ö×–ÆVBæW†Vw2÷vâ–6öâ&W6÷W&6R†'V–ÆBç'6Âv–æF÷w2öæÇ’’vW&R&÷F‚Væ6öæF—F–öæÆÇ¦&W6÷W&6W2÷6W'fõócBçævö6W'fòæ–6ö(	B6W'fòw2÷vâ'&æF–ærÂæ÷Bç’'F–7VÆ"vÖRw2à¥6W&FVÇ’ÂFW7B×vRöw2–æFW‚æ‡FÖÆæWfW"&VfW&Væ6VB—G2÷vâÇ&VG’×&W6VçB†'WBVçW6VB¦V&Æ–2öff–6öâç7fvÆ6V†öÆFW"Â6òæ÷&ÖÂ'&÷w6W"F"6†÷v–ær—B†Bæò–6öâBÆÂà ¢¢¤6†ævS¢¢¢FW7B×vRö–æFW‚æ‡FÖÆæ÷rÆ–æ·2ff–6öâç7fvÆ–¶Ræ÷&ÖÂvV'6—FRv÷VÆBâGvğ¦æWr&7FW"f—‡GW&W2æW‡BFò—BÂFW7B×vR÷V&Æ–2ö–6öâçæv‡v–æF÷r÷F6¶&"’æB–6öâæ–6ö ¢…v–æF÷w2W†R&W6÷W&6RÂ×VÇF’×6—¦RbÓ#Sg‚’Â&VG&rff–6öâç7fvw26ÖRÆ6V†öÆFW"FW6–và¢‡6öÆ–B3f3V6Sv7V&RÂ6VçFW&VBv†—FR%""’2&—FÖ2Â6–æ6Ræ÷F†–ær–âF†—26æF&÷‚6à§&7FW&—¦R5dræBF†RæF—fR6–FRæVVG2&7FW"f÷&ÖBV—F†W"v’â'V–ÆBç'6æ÷r6÷–W0§v†–6†WfW"öbFW7B×vR÷V&Æ–2ö–6öâçæv÷"&W6÷W&6W2÷6W'fõócBçævW†—7G2–çFğ¦DõUEôD•"÷v–æF÷uö–6öâçæv‡v—F‚6&vó§&W'VâÖ–bÖ6†ævVFöâ&÷F‚’ÂæB(	Böâv–æF÷w2(	B&VfW'0¦FW7B×vR÷V&Æ–2ö–6öâæ–6ö÷fW"&W6÷W&6W2÷6W'fòæ–6öf÷"v–æF÷w5&W6÷W&6S£§6WEö–6öæà¦†VFVE÷v–æF÷rç'6w2v–æF÷rÖ–6öâ–æ6ÇVFUö'—FW2æ÷r&VG2DõUEôD•"÷v–æF÷uö–6öâçæv–ç7FV@¦öbF†R†&F6öFVB&W6÷W&6W2÷6W'fõócBçævF‚âæWBVffV7C¢öæ6R&VÂvÖR7WÆ–W2—G2÷và¦–6öâçævö–6öâæ–6ö–âFW7B×vR÷V&Æ–2öÂF†Rv–æF÷rÂF6¶&"ÂæB6ö×–ÆVBW†V7WF&ÆRÆÀ§6†÷r—BWFöÖF–6ÆÇ“²'6VçBF†BÂWfW'—F†–ærfÆÇ2&6²FòFöF’w2&÷fW2Ö'&æFVB76WG0¦W†7FÇ’2&Vf÷&Râæv—FGG&–'WFW6v–æVB¢æ–6ò&–æ'–Æöæw6–FRF†R&RÖW†—7F–æp¦¢çævö¢æ§v'VÆW2(	B–6öâæ–6ö—2F†—2&Wòw2f—'7BG&6¶VBæ–6öf–ÆRÂæBv—F†÷WBà¦W‡Æ–6—B'VÆR—BfÆÇ2VæFW"F†R&Ææ¶WB¢FW‡CÖWFòVöÃÖÆfBF†RF÷öbF†Bf–ÆRÂv†–6€¦ÆWG2v—Bw2÷vâ†6öçFVçB×6æ–ff–ær’†WW&—7F–2FV6–FRv†WF†W"Fòæ÷&ÖÆ—¦RÆ–æRVæF–æw2–â—C°¦f÷"6ÖÆÂ&–æ'’f–ÆRF†Bw2&VÂ&—6²öb6–ÆVçB6÷''WF–öâöâv–æF÷w26†V6¶÷W@¢†6÷&RæWFö7&Æf’Âæ÷B§W7BF†V÷&WF–6ÂöæRà ¢¢¤FVÆ–&W&FVÇ’÷WBöb66÷S¢¢¢F†R&ö÷B7Æ6‚w2–6öâ‡6VRF†RVçG'’&÷fR’—2W†V×Bg&öĞ§F†—2fÆÆ&6²öâW'÷6R(	BÇv—2&÷fW2Ö'&æFVBÂ&Vv&FÆW72öbv†BF†RvÖR7WÆ–W2âÆ–çW‚w0¦æFW6·F÷–6öãÖæBÖ4õ2w2æ–6ç6ö–æfòçÆ—7F&VâwBv—&VBFòF†—2fÆÆ&6²V—F†W"(	@¦F–ffW&VçBf÷&ÖG2÷FööÆ–ær†â–ç7FÆÆVB„Dr–6öâ×F†VÖRVçG'’Â–6öçWF–Æ’æ÷Bv÷'F‚F†RW‡G&§7W&f6R–âF†—273²æ÷FVB†W&R2föÆÆ÷r×WÂÖ—'&÷&–ær†÷rF†R%&VæÖR6W'fòFò&÷fW2 ¦VçG'’&÷fRFö7VÖVçG2—G2÷vâ6–Ö–Æ&Ç’ÖFVfW'&VB'&æF–ærv÷&²à ¢¢¥v‡“¢¢¢6¶VBF—&V7FÇ’(	Böæ6RF†—2Væv–æR6†—2Ö÷&RF†âöæRvÖRÂvÖR6†÷VÆBÆöö²Æ–¶P¦—G6VÆb‡v–æF÷r÷F6¶&"öW†R–6öâ’&F†W"F†âF†R6†VÆÂ—B†Vç2Fò'VâöâÂv—F†÷WBæVVF–æp§Fò&RFöÆC²fÆÆ–ær&6²Fò&÷fW2r÷vâ–6öâ¶VW2FöF’w2&V†f–÷"f÷"ç’vÖRF†B†6âw@§7WÆ–VBöæR–WBà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢'V–ÆBç'6æ÷rFöW2f–ÆW7—7FVÒ’ôğ¢†7FC£¦g3£¦6÷–’Væ6öæF—F–öæÆÇ’öâWfW'’F&vWBÂæ÷B§W7Bv–æF÷w2(	B6†VÂ'WBæ÷FR—B–`¦WfW"VF—F–ær'V–ÆBç'6f÷"v‡’—BF÷V6†W2F‡2÷WG6–FR—G2÷vâ7&FRà ¢¢¥fW&–f–6F–öã¢¢¢6ÖRVçf—&öæÖVçBÆ–Ö—FF–öâ2F†RVçG'’&÷fR(	B6&vò6†V6²× §6W'f÷6†VÆÆ6÷VÆBæ÷BgVÆÇ’6ö×ÆWFR†W&R‡Vç&VÆFVBÆ–'VFWb×7—6ö¶rÖ6öæf–vv’Â6òF†P¦æWr'V–ÆBç'6Æöv–2æB†VFVE÷v–æF÷rç'6w2–æ6ÇVFUö'—FW2†6öæ6B†Vçb‚$õUEôD•""’Ââââ’– ¦6†ævRvW&VâwB6ö×–ÆW"×fW&–f–VBVæB×FòÖVæBâ–6öâæ–6öw2×VÇF’×6—¦RVÖ&VFF–ærƒbó3"óC‚ócBğ£#‚ó#Sb’v26öæf—&ÖVBv—F‚–ÆÆ÷r†–ÖvRæ÷Vâ‚âââ’æ–æfõ²'6—¦W2%Ö’gFW"vVæW&F–öââ¢¤æVVG0¦&VÂâöÖ6‚'V–ÆFöâv–æF÷w2Fò6öæf—&ÒF†RæW†Vw2W‡Æ÷&W"–6öâæBF†Rv–æF÷r÷F6¶& ¦–6öâ&÷F‚7GVÆÇ’6†ævR¢¢v†VâFW7B×vR÷V&Æ–2ö–6öâç·ærÆ–6÷Ö&R&W6VçBÂæBfÆÂ&6°¦6÷'&V7FÇ’v†VâF†W’w&R&VÖ÷fVBà ¢¢¤föÆÆ÷r×Wƒ##bÓ‚Ó’(	Bf—†VB&VÂ'VrF†—2$æVVG2&VÂ'V–ÆB"æ÷FR6Vv‡C¢¢¢FW7FVBf–¦æv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆÂv†–6‚(	BVæÆ–¶Ræ÷&ÖÂ–â×Æ6R'V–ÆB(	B&V6öç7G'V7G26W'fòöæP¦F—&V7F÷'’ÆWfVÂv’g&öÒF†—2&Wòw2&VÂÆ–÷WC¢—BF÷væÆöG2·F6†W2&—7F–æR6W'fò–çFò¦æW7FVB6W'fò×7&2ö7V&F—&V7F÷'’Â6ò÷'G2÷6W'f÷6†VÆÂö'V–ÆBç'6VæG2W'Vææ–ærg&öĞ¦6W'fò×7&2÷÷'G2÷6W'f÷6†VÆÂöÂæ÷BÇ&Wò&ö÷Câ÷÷'G2÷6W'f÷6†VÆÂöâ—G2ââòââ÷FW7B×vRòââæ §F‡2†6÷'&V7Bf÷"F†—2&Wòw2&VÂÂfÆBÆ–÷WB(	BfW&–f–VBF—&V7FÇ’'’†æBg&öĞ¦÷'G2÷6W'f÷6†VÆÂö’&W6öÇfVBFò6W'fò×7&2÷FW7B×vRòââæ–ç7FVBöbF†R&VÀ¦Ç&Wò&ö÷Câ÷FW7B×vRòââæÂv†–6‚FöW6âwBW†—7B(	B6òvÖU÷v–æF÷uö–6öâæW†—7G2‚–ğ¦vÖUöW†Uö–6öâæW†—7G2‚–vW&RÇv—2fÇ6V–âF†Bv÷&¶fÆ÷r7V6–f–6ÆÇ’ÂÇv—26–ÆVçFÇ¦fÆÆ–ær&6²Fò&÷fW2r÷vâ–6öââ…F†RÆFW"76VÖ&ÆRFW7B'VæFÆV7FWw2÷vâÒÖ6öçFVçBÖF— ¢ââ÷FW7B×vRöF—7F†VæVBFò7F–ÆÂv÷&²Â6–æ6RF†B7FW'Vç2öæRF—&V7F÷'’6†ÆÆ÷vW"À¦F—&V7FÇ’–ç6–FR6W'fò×7&6Âæ÷Bg&öÒ6W'fò×7&2÷÷'G2÷6W'f÷6†VÆÆâ’&ö÷B6W6S¢FW7B×vRö §v2æWfW"'Böb6W'fò×7&2öBÆÂ(	BF†RF6‚ÖÇ’Æö÷öæÇ’&V7&VFW2§F6‚×G&6¶VB ¦f–ÆW2‡6VRF†—2VçG'’w2÷vâ$f–ÆW2"æ÷FRöâFW7B×vRö&V–ær÷WG6–FRF†RF6‚6WB’Â6ğ¦æ÷F†–ærWfW"WB6÷’öb—BF†W&Râ¢¤f—†VB–âFW7Bç–ÖÆ—G6VÆb¢¢†æ÷B'V–ÆBç'6Âv†÷6P§F‡2&R6÷'&V7Bf÷"†÷rWfW'’&VÂ'V–ÆB7GVÆÇ’Æ—2÷WB“¢F†R&F÷væÆöB²F6‚6W'fğ§6÷W&6R"7FWæ÷rÇ6ò6÷–W2FW7B×vR÷V&Æ–2öæBF†RÇ&VG’Ö'V–ÇBFW7B×vRöF—7Bö–çFğ¦6W'fò×7&2÷FW7B×vRöÂ6òF†BG&VR7GVÆÇ’Ö—'&÷'2F†—2&Wòw2&VÂÆ–÷WBF†Rv’F6†W0¦Ç&VG’77VÖS²&76VÖ&ÆRFW7B'VæFÆR"w2ÒÖ6öçFVçBÖF—&WFFVBg&öÒââ÷FW7B×vRöF—7FFğ¦FW7B×vRöF—7FFòÖF6‚â6öæf—&ÖVB'’&R×G&6–ær&÷F‚7FW2rv÷&¶–ærF—&V7F÷&–W2æBF†P§&W7VÇF–ær&VÆF—fRF‡2'’†æC²æ÷B–WB6öæf—&ÖVB'’â7GVÂ4’'Vâà ¢¢¥6V6öæBföÆÆ÷r×Wƒ##bÓ‚Ó’(	B¦—æÖVB7V&föÆFW"Âæ÷B&VÆV6Röw26öçFVçG2BF†P¦&6†—fR&ö÷C¢¢¢Vç&VÆFVBFòF†R–6öâ'Vr&÷fRÂ'WB6Vv‡Bv†–ÆR&RÖ6†V6¶–ærF†R6ÖP§v÷&¶fÆ÷r(	B&÷F‚'¦—'VæFÆR"7FW2†FW7Bç–ÖÆ’¦—VB&VÆV6Röw2¦6öçFVçG2¢F—&V7FÇ¢†¦—×""ââòE¤•"æg&öÒ–ç6–FR&VÆV6Rö²6ö×&W72Ô&6†—fRÕF‚&VÆV6Rò¦’Â6ğ¦W‡G&7F–ærF†RF÷væÆöFVB&6†—fRGV×VBÆ–öÆ’æW†VÇW2‚²Æö÷6RDÄÇ2÷7W÷'Bf–ÆW0§7G&–v‡B–çFòv†FWfW"föÆFW"–÷RW‡G&7FVB–çFò†RærâF÷væÆöG2ö’Âæ÷B–çFòöæRföÆFW"ö`§F†V—"÷vââ&÷F‚7FW2æ÷r×fö&VæÖRÔ—FVÖ&VÆV6VFò6W'fò×FW7B×vVf—'7BÂF†Vâ¦— ¢§F†BföÆFW"¢†¦—×""E¤•"6W'fò×FW7B×vV²6ö×&W72Ô&6†—fRÕF‚6W'fò×FW7B×vV’(	@¦6ö×&W72Ô&6†—fVö¦—×&&÷F‚&W6W'fRF†Rv—fVâföÆFW"—G6VÆb2F†R&6†—fRw2öæP§F÷ÖÆWfVÂVçG'’v†VâF†RF‚FöW6âwBVæB–âv–ÆF6&BÂv†–6‚—2v†BÖ¶W2F†—2v÷&²à¦6W'fò×FW7B×vVæÖW2—BgFW"F†—2v÷&¶fÆ÷rw26öçFVçB†FW7B×vRö“²&VÂ'VæFÆRv÷VÆ@§W6RF†R7GVÂvÖRw2æÖR–ç7FVBâæ÷B–WB6öæf—&ÖVB'’â7GVÂ4’'Vâà ¢ÒÒĞ ¢22##bÓ‚Ó(	BæWfW"6†÷rv†—FR&Vf÷&RF†RvÖR7F'G3¢FVfVÇB6ÆV"6öÆ÷"²–çBÖ&Vf÷&R×6†÷p ¢¢¤f–ÆW3¢¢¢6ö×öæVçG2ö6öæf–r÷&Vg2ç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó#ÖæWfW"×6†÷r×v†—FRÖöâ×7F'GWçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢Gvò–æFWVæFVçBv26÷VÆB7F–ÆÂ6†÷rv†—FR†÷"÷F†W'v—6P§VæFVf–æVB’g&ÖR&Vf÷&RF†R&ö÷B7Æ6‚VçG'’&÷fRw26÷fW&vR¶–6·2–âÂ÷"gFW"—B†æG0¦öfbFòF†R&VÂvS  £â&VfW&Væ6W3£¦6öç7EöFVfVÇB‚–w26†VÆÅö&6¶w&÷VæEö6öÆ÷%÷&v&(	BF†RvÄ6ÆV$6öÆ÷&W6V@¢f÷"¦ç’¢vV%f–WvF†B†6âwB–çFVBç—F†–æröb—G2÷vâ–WBÂW"6ö×öæVçG2÷–çBğ¢–çFW"ç'6w2÷vâ6öÖÖVçB‚&6ÆV"F†RVçF—&R&VæFW&–æt6öçFW‡Bâââ6òvV%f–Wr7GVÆÇ¢6ÆV'2WfVâ&Vf÷&RF†Rf—'7BvV%f–Wr—2&VG’"’(	BFVfVÇFVBFò÷VRv†—FP¢†³ãÂãÂãÂãÖ’âF†—2—2F†R6Æ76–2&&Ææ²v†—FRF""WfW'’'&÷w6W"†2ÂæB—Bw0¢W†7FÇ’v†B6†÷vVBF‡&÷Vv‚GW&–ærF†Rv&WGvVVâF†R&ö÷B7Æ6‚VæF–æp¢†WfVçC£¤&ö÷E&VG–’æBF†R&VÂvRw2f—'7B–çB(	BÇW2Â–æFWVæFVçFÇ’Âöâ¦ç’ ¢ÆVæ6‚v—F‚æòVæF–ær&ö÷BW‡G&7F–öâBÆÂ†Æ–âFWbÒ×W&Æ'VâÂ÷"¢ÒÖ6öçFVçBÖ6ö×&W73ÖæöæV'V–ÆB’Âv†–6‚æWfW"VçFW'27FFS£¤&ö÷F–æv–âF†Rf—'7@¢Æ6RæB6òæWfW"v÷BF†R&ö÷B×7Æ6‚VçG'’w26÷fW&vRFò&Vv–âv—F‚à£"âwV“£¦æWv†wV’ç'6’6ÆÆVBv–æ—E÷v–æF÷rç6WE÷f—6–&ÆR‡G'VR–v—F†÷WBWfW"–çF–æp¢ç—F†–ærf—'7B(	BF†Rv–æF÷r&V6ÖRf—6–&ÆRv—F‚v†FWfW"VæFVf–æVB6öçFVçB—G2tÀ¢7W&f6R†VæVBFò†fR‡7W&fÖâ÷F†RG&—fW"FöâwB6ÆV"—Böâ7&VF–öâ’Âf÷"†÷vWfW"Æöæp¢VçF–Âv–æ—BFVÆ—fW&VBF†Rf—'7B&VG&u&WVW7FVFâöâ6öÖRÆFf÷&×2öG&—fW'2F†Bw2¢f—6–&ÆRfÆ6‚öbv&&vR÷"v†—FRÂæ÷B&Æ6²à ¢¢¤6†ævS¢¢  ¢Ò6†VÆÅö&6¶w&÷VæEö6öÆ÷%÷&v&æ÷rFVfVÇG2Fò÷VR&Æ6²†³ãÂãÂãÂãÖ’à¢ffV7G2öæÇ’F†R&æ÷F†–ær–çFVB–WB"7FFR(	Böæ6RvR7GVÆÇ’–çG2†–æ6ÇVF–ærF†P¢vÖRw2÷vâvRÂv†FWfW"—G2552&6¶w&÷VæB—2’ÂF†B6öçFVçBgVÆÇ’6÷fW'2F†—26öÆ÷"Â6ğ¢F†W&Rw2æòf—6–&ÆRVffV7Böæ6RÆöF–ær—2FöæRà¢ÒwV“£¦æWvæ÷r6ÆÇ2WFFU÷7Æ6‚‡v–æ—E÷v–æF÷rÂæöæR–²–çB‡v–æ—E÷v–æF÷r–(	BF†R6ÖP¢&ö÷B×7Æ6‚&Æ6²67&VVâF†RVçG'’&÷fRFFVB(	B¦&Vf÷&R¢6WE÷f—6–&ÆR‡G'VR–ÂöâWfW'¢6öFRF‚Âæ÷B§W7B6¶VBÖ6öçFVçB&ö÷BW‡G&7F–öââF†—2wV&çFVW2F†RfW'’f—'7BF†–æp¢F†Rõ2WfW"F—7Æ—2f÷"F†Rv–æF÷r—2F†R&Æ6²7Æ6‚g&ÖRÂ&Vv&FÆW72öbv†WF†W ¢7FFS£¤&ö÷F–æv—2WfW"VçFW&VBBÆÂà ¤6öÖ&–æVBÂWfW'’7F'GWF‚æ÷rvöW3¢&Æ6²7Æ6‚g&ÖR‡–çFVB&Vf÷&RF†Rv–æF÷r—2WfVà§f—6–&ÆR’(i"&VÂvRÂv†÷6R÷vâ&æ÷B–WB–çFVB"&6¶w&÷VæB—2æ÷r&Æ6²–ç7FVBöbv†—FR(i §F†RvRw27GVÂ6öçFVçBâæò6öFRF‚6†÷w2v†—FRVæÆW72F†RvÖRw2÷vâvRW‡Æ–6—FÇ§–çG26öÖWF†–ærv†—FR—G6VÆbà ¢¢¥v‡“¢¢¢6¶VBF—&V7FÇ’(	Bv†—FRfÆ6‚GW&–ær7F'GWöâ&Æ6²&ö÷B7Æ6‚&VG20¦'&ö¶Vâö¦æ·’&Vv&FÆW72öb†÷r'&–Vc²f—†–ærF†R§&VæFW&–ærÖÆWfVÂ¢FVfVÇB‡F†R6ÆV"6öÆ÷ ¦WfW'’vV%f–WrW6W2&Vf÷&R—B†26öçFVçB’—2f"Ö÷&R&ö'W7BF†âG'––ærFò7–æ6‡&öæ—¦P¦v–ç7BF†RvRw2÷vâÆöBÆ–fV7–6ÆRg&öÒF†RVÖ&VFFW"6–FR‡F†W&Rw2æòf—'7B×–çB6–væÀ¦W‡÷6VBFò÷'G2÷6W'f÷6†VÆÆFöF’(	BF†R6Æ÷6W7BÂÆöE7FGW3£¤6ö×ÆWFV ¢†æ÷F–g•öÆöE÷7FGW5ö6†ævVFÂ'Vææ–æuö÷7FFRç'3£sƒ6’Â—2DôÒ×&VF–æW72Âæ÷@¢&6ö×÷6—F÷"†2&W6VçFVBg&ÖR"(	B6VR6ö×öæVçG2öÖWG&–72öÆ–"ç'6w2f—'7E–çFÂv†–6€§FW&Ö–æFW2–ç6–FRF†R6öç7FVÆÆF–öâf÷"F†RW&f÷&Öæ6R’æB—6âwBf÷'v&FVBFòF†P¦VÖ&VFFW"BÆÂ’â6†æv–ærF†R6†&VBFVfVÇB6ÆV"6öÆ÷"6–FW7FW2æVVF–ærF†B6–væÂà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢6ö×öæVçG2÷6W'fò÷FW7G2ğ§W&f÷&Öæ6U÷–çE÷F–Ö–ærç'6Ç&VG’÷fW'&–FW2F†—26ÖR&Vbf÷"—G2÷vâFW7G2‡Fğ¦w&’ö&ÇVR’(	BVæffV7FVB'’F†—2FVfVÇB6†ævRÂ'WBv÷'F‚¶æ÷v–ær—Bw2&V6VFVçBf÷ §W"×FW7B÷fW'&–FW2–bgWGW&RFW7B&VÆ–W2öâF†R¦FVfVÇB¢&V–ærZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æÚ±î¸Â¸­yêë¢°k¢G§¦*^white.

**Verification:** not compiled end-to-end in this environment (same `libudev-sys`/pkg-config
gap as the boot-splash entry above). `shell_background_color_rgba`'s new default and
`Gui::new`'s reordering were both read back against the surrounding code to confirm no other
call site assumes white (`components/paint/painter.rs`'s own doc comment for the pref, and
`Gui::new`'s existing accesskit-before-visible ordering, which this change preserves â€”
painting happens after accesskit init, still before `set_visible`).

---

## 2026-08-10 â€” Persist fullscreen state across launches

**Files:** `ports/servoshell/prefs.rs`, `ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0021-persist-fullscreen-across-launches.patch`

**Upstream behavior:** `HeadedWindow`'s `fullscreen: Cell<bool>` (and the actual OS-level
fullscreen state it tracks) was purely in-memory â€” always started `false`. A game closed
while its page was in fullscreen (via the Fullscreen API â€” `requestFullscreen()`/
`exitFullscreen()`, routed through `RunningAppState::notify_fullscreen_state_changed` â†’
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
  `ActiveEventLoop::primary_monitor`/`available_monitors` â€” the window doesn't exist yet, so
  `winit_window.current_monitor()` isn't available the way `set_fullscreen` uses it) when
  `start_fullscreen` is set, and seeds `fullscreen: Cell::new(start_fullscreen)` to match â€”
  avoids a windowed-then-fullscreen transition on startup (and keeps that visible moment,
  whatever it is, off-white too, per the entry above).
- `set_fullscreen` (`headed_window.rs`) now calls a new `persist_fullscreen_state(config_dir,
  state)` helper whenever the state actually changes â€” writes an empty marker file named
  `fullscreen` under `config_dir` on entering fullscreen, removes it on leaving. Deliberately
  a marker file's mere existence, not JSON, matching `support/content-packer/src/
  extract.rs`'s own marker-file convention for simple booleans. `config_dir` being `None`
  (couldn't be resolved) just skips persistence rather than failing.

**Why:** asked directly â€” closing in fullscreen and reopening windowed is a jarring,
unexpected transition; a game's window state should survive a relaunch the way a player
left it.

**Side effects to know about when upgrading:** the marker lives directly under `config_dir`
(e.g. `~/.config/servo/default/fullscreen` on Linux by default) â€” the same directory
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

## 2026-08-10 â€” Fix: Windows taskbar/Alt-Tab icon not actually using the custom icon

**Files:** `ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0022-fix-windows-taskbar-icon.patch`

**Upstream behavior:** winit 0.30's cross-platform `Window::set_window_icon` only sets
`WM_SETICON`'s `ICON_SMALL` â€” the title bar icon. The taskbar/Alt-Tab icon is `ICON_BIG`, set
by a *separate*, Windows-only call (`WindowExtWindows::set_taskbar_icon`, in
`winit::platform::windows`) â€” `ports/servoshell` only ever called the former. Confirmed by a
real build: the title bar showed no icon at all, while the taskbar happened to show the
*right* icon anyway â€” Windows falls back to the `.exe`'s own embedded resource icon
(`build.rs`'s `winresource` step, itself already reading the game-supplied `icon.ico` â€” see
that entry above) for `ICON_BIG` when nothing has explicitly set it, which is why this went
unnoticed until specifically checking the title bar.

**Change:** `HeadedWindow::new` now also calls `winit_window.set_taskbar_icon(Some(icon))` (a
clone of the same `Icon` passed to `set_window_icon`), gated `#[cfg(target_os = "windows")]`
(the extension trait doesn't exist on other platforms â€” Linux/macOS have no small/big icon
split). Both calls now always agree, instead of one coming from the runtime icon and the
other happening to come from the `.exe` resource fallback.

**Why:** a real build showed the title bar icon missing entirely, which is what surfaced this
â€” the taskbar looking right was accidental (a different icon source entirely), not evidence
the runtime icon-setting code was correct.

**Side effects to know about when upgrading:** if a future winit version merges `ICON_SMALL`/
`ICON_BIG` back into one call (or renames `set_taskbar_icon`), this two-call pattern may
become redundant or need updating â€” check `winit::platform::windows::WindowExtWindows`'s docs
for the version in use.

**Verification:** not compiled end-to-end in this environment (same `libudev-sys`/pkg-config
gap as other entries above); `WindowExtWindows::set_taskbar_icon`'s signature and its
`ICON_BIG`/`ICON_SMALL` split were confirmed directly against winit 0.30.13's own source
(`platform_impl/windows/window.rs`, `platform/windows.rs`) in this workspace's registry cache,
not assumed. **Confirmed real bug** (title bar icon missing) via an actual Windows build/run;
the fix itself needs a further real build to confirm the title bar icon now actually appears.

---

## 2026-08-10 â€” Window title from the game's own `manifest.json`/`package.json` name

**Files:** `python/servo/post_build_commands.py`, `ports/servoshell/prefs.rs`,
`ports/servoshell/desktop/headed_window.rs`.

**Patch:** `patches/servo-v0.4.0/0023-window-title-from-manifest-or-package-json.patch`

**Upstream behavior:** the native window title always mirrored the active page's own
`document.title` (`HeadedWindow::update_user_interface_state`, falling back to the URL, then
to a hardcoded `"Roves"` if there's no webview at all) â€” there was no way to give a shipped
game a fixed, native-feeling title independent of whatever its page's `<title>` happens to
say (in `test-page`'s case, `<title>Servo test build</title>` â€” an internal diagnostic label,
not a real product name).

**Change:** scoped to `mach bundle` only (a plain `./mach run`/dev launch is unaffected â€”
deliberate, see "Why" below):

- `post_build_commands.py`'s new `_resolve_window_title(content_dir)` reads
  `<content_dir>/manifest.json`'s `name` field (a standard web-app-manifest field, and â€” since
  a bundler copies `public/` into the build output root â€” actually present inside
  `content_dir`, e.g. `dist/`, once built) or, failing that, `<content_dir>/../package.json`'s
  `name` (the common Vite/webpack layout: `package.json` next to the project, `content_dir`
  its built `dist/` one level below â€” a source file, so only available here, at bundle time,
  never inside the shipped `content_dir` itself). Used *verbatim* as the window title â€” this
  doesn't prepend "Roves" or reformat it at all; that's the content author's own call (e.g.
  `test-page/public/manifest.json` already had `"name": "Roves test-page"` from an unrelated
  earlier commit, which is exactly why that specific string was expected here). `bundle()`
  appends `["--window-title", <name>]` to `launch.json`'s `args` when a name was found;
  nothing changes when neither file has one.
- `prefs.rs`: new `--window-title TEXT` CLI flag â†’ `ServoShellPreferences.window_title_override:
  Option<String>`.
- `headed_window.rs`: new `HeadedWindow.window_title_override` field (cloned from the
  preference at construction). `update_user_interface_state` now uses it as a fixed title when
  set, instead of ever computing one from the active webview's page title/URL â€” set once,
  never changed afterward, even if the page's own `document.title` changes later.

**Why:** asked directly, scoped to packaged builds only since `manifest.json`/`package.json`
naming a real product only makes sense for an actual shipped game â€” a dev run (`./mach run
some/path/index.html`) has no natural "content_dir" to read a manifest from in the first
place, and showing the raw page title there (as today) is arguably more useful for debugging
anyway.

**Side effects to know about when upgrading:** if `content_dir`'s bundler doesn't copy
`public/`-style files into the build root the way Vite does by default, `manifest.json` won't
be found there and this silently falls through to the `package.json` candidate (or to no
override at all) â€” not a bug, just worth knowing the first candidate path assumes that
convention.

**Verification:** `python3 -m py_compile python/servo/post_build_commands.py` â€” clean.
Confirmed `test-page/public/manifest.json` (`name: "Roves test-page"`) actually ends up at
`test-page/dist/manifest.json` after `npm run build`, since `test-page/vite.config.ts` doesn't
override Vite's default `publicDir`. Not compiled end-to-end on the Rust side in this
environment (same `libudev-sys` gap as other entries above). **Needs a real `mach bundle` +
run to confirm end-to-end**: title bar should read exactly `Roves test-page` for this repo's
own test bundle.

---

## 2026-08-11 â€” `roves:clear_content_cache` command, so a game can wipe its own extraction cache

**Files:** `ports/servoshell/desktop/protocols/roves.rs`, `ports/servoshell/desktop/app.rs`,
`support/content-packer/src/extract.rs`. Plus, outside this `servo/` directory (not
patch-tracked â€” see the "roves: protocol bridge" entry above for why): new module
`roves-api/src/cache.ts` (and its `roves-api/tsup.config.ts` entry point), and a new
"Clear extraction cache" button in `test-page` (`test-page/src/ClearCacheButton.tsx`, wired
into `App.tsx`).

**Patch:** `patches/servo-v0.4.0/0024-clear-content-cache-command.patch`

**Upstream behavior:** no equivalent â€” extends the `roves:` bridge (see the 2026-08-06 entry
above) with a second command, not a modification of existing upstream logic.

**Change:** a game's packed content (see the "Pack game content into compressed archives"
and "boot set" entries above) gets decompressed on first launch into a per-install directory
under the OS cache dir, and stays there â€” nothing ever clears it automatically. Asked for a
way for the game itself to force a fresh re-extraction (e.g. after shipping a content update
under an unexpectedly unchanged `content_hash`, or just for a support/troubleshooting reset),
*without* touching actual save data, which lives elsewhere entirely and this command never
touches.

- `support/content-packer/src/extract.rs`: new `is_managed_cache_dir(dir)` (checks for the
  `.roves-content-source` marker `prepare_dest` always writes) and `clear_cache(dir)` (refuses,
  rather than deletes, if that marker is missing, then `fs::remove_dir_all`s the whole
  directory). The marker check matters here specifically because this is a generic "delete
  this directory" operation reachable from web content â€” it must not be possible to point it
  at some unrelated path and have it delete that instead.
- `app.rs`: computes `content_cache_dir` the exact same way `FileProtocolHandler::new` derives
  its own `cache_dir` (the initial `file:` URL's parent directory) and passes it to
  `RovesProtocolHandler::new` alongside the existing `close_proxy`.
- `roves.rs`: new `"clear_content_cache"` match arm â€” calls `extract::clear_cache`, and, only
  if that succeeds, closes every window the same way `exit` does (factored the `exit` arm's
  window-closing logic out into a shared `close_all_windows` helper both arms now call). `None`
  `content_cache_dir` (a plain dev `--url` launch, not a packed-content one) answers with an
  error instead of doing nothing silently.
- `roves-api/src/cache.ts`: `clearContentCache()`, a thin `invoke("clear_content_cache")`
  wrapper, new `cache` entry in `tsup.config.ts` (mirrors `process`/`steam`'s existing pattern).
- `test-page/src/ClearCacheButton.tsx`: same shape as the existing quit button â€” a
  `window.confirm()`-guarded destructive action â€” dropped into the button row next to
  `IndexedDbButton`/the quit button in `App.tsx`.

**Why closing the window is not optional:** the destination directory this clears is the
*live* document root while the game is running (`FileProtocolHandler` serves the current
page and every future on-demand pack extraction out of it â€” see the "lazy on-demand content
extraction" entry above). Deleting it out from under a still-running page would silently break
any asset not yet extracted this session. A relaunch-instead-of-close option was considered
(and would need new code â€” nothing in this codebase currently spawns/replaces its own
process) but deliberately left out of this first version to keep the change small; closing and
letting the player start the game again is the safe default.

**Verification:** `cargo check -p roves-content-packer` and `cargo check -p servoshell` both
pass. `roves-api`'s `npm run build` (tsup) produces `dist/cache.{mjs,cjs,d.ts}` correctly.
`test-page`'s `tsc && vite build` verified against a locally `npm pack`-built copy of
`@drincs/roves-api` (the npm-published `0.1.0` doesn't have the `cache` module yet â€” see
below). Not run end-to-end against a real native build in this environment (same
`libudev-sys` gap as other entries above).

**Follow-up needed before this is actually usable from `test-page`:** `test-page/package.json`
depends on the *published* npm package, not the local workspace source (see the 2026-08-06
entry's "Outside `servo/`" note) â€” bumped here to `"@drincs/roves-api": "^0.2.0"` to match
`roves-api/package.json`'s version bump (`0.1.0` â†’ `0.2.0`, done alongside this change, both
plain local edits). But the npm registry itself still only has `0.1.0` (`core`/`process`/`steam`
only, no `cache`) until someone actually publishes `0.2.0` â€” push a `v0.2.0`-style tag (see
`roves-api/.github/workflows/npm-publish.yml`) to trigger that. Until then, `test-page`'s own
`npm install`/`tsc` (including `.github/workflows/test.yml`'s CI) will fail to resolve
`@drincs/roves-api/cache`. Deliberately left un-triggered here since pushing a release tag is
a real publish action, not a local code change.

---

## 2026-08-12 â€” Hold the boot splash for a minimum duration on every launch

**File:** `ports/servoshell/desktop/app.rs`.

**Patch:** `patches/servo-v0.4.0/0025-hold-boot-splash-minimum-duration.patch`

**Upstream behavior:** no equivalent â€” refines the boot-splash entry above (2026-08-09) and
the never-show-white entry (2026-08-10).

**Problem:** asked directly â€” on startup, a brief black screen showed before the real page,
instead of the branded (icon + "Roves") splash. Root cause: `AppState::Booting` (the branded
splash's only code path with any actual visible duration) was gated entirely on there being a
pending packed-content boot extraction (`App::pending_extraction.is_some()`). A launch with
nothing to extract â€” a dev `--url` run, or (the common case after the very first launch) a
packed-content launch whose destination is already cached from a previous run â€” skipped
`Booting` entirely. The window's very first frame *is* the branded splash (`Gui::new` already
painted it, unconditionally, per the 2026-08-10 entry), but the very next frame immediately
swapped in the real, still-loading `WebView` â€” whose own clear color is black too (that same
entry's `shell_background_color_rgba` default) until it has content to paint. Net effect: the
branded splash flashed for a single frame, too brief to register â€” read by a user as "black
screen, then the app", not as a splash.

**Change:** new `MIN_SPLASH_DURATION` (500ms). Every headed launch now always enters
`AppState::Booting` and stays there for at least this long, regardless of whether there's a
pending extraction â€” if there is one, it still also has to finish (unchanged); if there isn't,
`MIN_SPLASH_DURATION` alone is what `finish_init` waits on.

- `AppState::Booting` gained a new field, `extraction_done: bool` â€” `true` from the start if
  `App::init` had no `pending_extraction` to begin with (nothing left to wait on but the
  timer), or flips to `true` once `AppEvent::BootReady` arrives (unchanged trigger, same as
  before this change).
- `App::init`: no longer branches on `self.pending_extraction` to decide *whether* to enter
  `Booting` for a headed launch â€” it always does now (headless is unaffected: no splash to
  show either way, so a pending extraction there still just runs synchronously, exactly as
  before). Only branches on it now to decide whether to also spawn the background extraction
  thread.
- New `App::try_finish_booting(event_loop)`: the one place that decides whether `Booting` is
  actually done â€” `extraction_done && extraction_started.elapsed() >= MIN_SPLASH_DURATION` â€”
  and, if not, re-arms `ControlFlow::WaitUntil` for whichever of `SPLASH_PROGRESS_BAR_DELAY`/
  `MIN_SPLASH_DURATION` hasn't elapsed yet. A no-op when not `Booting`, so every event handler
  can call it unconditionally instead of duplicating this decision. Replaces the old, simpler
  "just call `finish_init` directly from the `BootReady` handler" â€” that alone is no longer
  sufficient, since extraction finishing early (or not existing at all) must *not* skip the
  remaining minimum-duration wait.
- `new_events`/`window_event`/`user_event`'s `Booting` branches: still do their own thing
  first (force a progress-bar-delay redraw; paint the splash; update `progress`/
  `extraction_done`), then all defer to `try_finish_booting` instead of each having its own
  copy of the finish-or-reschedule logic (`window_event`'s old inline version) or directly
  calling `finish_init` (`user_event`'s old `BootReady` arm).

**Why 500ms specifically:** long enough to reliably register as "a splash appeared" (vs. a
single-frame flash, which is what the bug was), short enough that it doesn't read as an
artificial delay on a fast dev relaunch. Not derived from any measurement â€” a reasonable
starting point, adjustable if it feels off in practice.

**Verification:** `cargo check -p roves-content-packer` unaffected (this entry doesn't touch
that crate). `cargo check -p servoshell` could not be completed in this environment â€” same
`libudev-sys`/pkg-config gap as prior entries, this time hit before rustc ever reached
`servoshell`'s own source (a dependency lower in the graph fails first) â€” so this change was
*not* type-checked by rustc here. Reviewed by hand instead: every borrow-splitting pattern used
(cloning a `Rc<dyn PlatformWindow>` out of an `&mut self.state` match arm before calling back
into `&mut self`; ending an immutable `if let ... = &self.state` borrow at its last use before
a subsequent `&mut self` call) mirrors a pattern already present and compiling elsewhere in
this same file (e.g. the pre-existing `user_event`'s `boot_ready_window` extraction). **Needs a
real `./mach build`/`./mach run` to confirm end-to-end** â€” the splash should now visibly hold
for ~500ms on every launch, including a plain `./mach run --url` dev launch and a relaunch of
an already-extracted packed build, not just a genuine first-launch extraction.

---

## 2026-08-12 â€” Name the extraction cache directory after the game

**Files:** `support/content-packer/src/manifest.rs`, `support/content-packer/src/pack.rs`,
`support/content-packer/src/extract.rs`, `support/content-packer/src/main.rs`,
`support/content-packer/tests/roundtrip.rs`, `ports/servoshell/desktop/bundle_launch.rs`,
`python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0026-name-extraction-cache-dir-after-the-game.patch`

**Upstream behavior:** no equivalent â€” refines `extract::default_dest` (2026-08-09's
"lazy on-demand content extraction" entry, further along above).

**Problem:** asked directly â€” the on-disk extraction cache directory (see the appdata-cache
entries above) was a bare `<cache_dir>/roves-content-<hash8>/`, opaque and indistinguishable
from any other game's cache dir on the same machine at a glance.

**Change:** `<cache_dir>/<game_name>/cache/<hash8>/` â€” a top-level folder named after the game,
with the actual extracted content nested inside a `cache/<hash8>` subfolder (the hash â€” of the
resolved `content_dir` path, unchanged in meaning from before â€” is still what actually keeps
repeat launches of the *same* install pointed at the same destination while different
installs/games that happen to share a display name don't collide).

- `manifest.rs`: `Manifest` gained `name: Option<String>` (`#[serde(default)]`, so an older
  manifest without this key still deserializes) â€” the game's display name, written verbatim
  by `pack`. `None` for a dev/uncompressed build or an older manifest.
- `pack.rs`: `PackOptions` gained a matching `name: Option<String>`, threaded straight into the
  `Manifest` it writes.
- `main.rs`: new `--name NAME` (optional) on the `pack` subcommand, wired to
  `PackOptions::name`. `extract` is untouched â€” it never needs the name passed explicitly, see
  below.
- `extract.rs`: `default_dest` takes a new `game_name: Option<&str>` parameter and builds the
  new nested path shape; falls back to the literal string `"roves"` if `game_name` is `None` or
  sanitizes to nothing. New `sanitize_path_segment(name)` turns arbitrary manifest text into a
  single filesystem-safe path segment â€” strips path separators/Windows-reserved characters
  (`\/:*?"<>|`) and control characters to `-`, trims leading/trailing whitespace and `.`
  (Windows disallows a trailing dot/space; a lone leading dot reads as hidden on Unix), and
  caps length at 64 chars; returns `None` only if nothing survives (e.g. empty, or all
  whitespace/dots), in which case the caller falls back to `"roves"` â€” a result of all-`-`
  characters (e.g. sanitizing `"/\\\":"`) is still `Some`, since that's a valid, if unhelpful,
  directory name, not "nothing usable". `resolve_dest` gained the same new `game_name`
  parameter, forwarded straight to `default_dest`. `prepare_dest` now loads the manifest
  *before* calling `resolve_dest` (previously after) so `Manifest::name` is available in time â€”
  the same `content_dir.join("manifest.json")` read either way, just reordered; canonicalizing
  `content_dir` first was never a real dependency of that read. Added unit tests for
  `sanitize_path_segment` and `default_dest`'s new path shape (`#[cfg(test)] mod tests`,
  matching `size.rs`'s existing convention).
- `bundle_launch.rs`: `resolve_packed_content_url` already loaded the manifest before calling
  `resolve_dest` (unlike `prepare_dest`, no reordering needed here) â€” just passes
  `manifest.name.as_deref()` through now.
- `post_build_commands.py`: `_place_bundle_content` gained a `game_name: Optional[str]`
  parameter, passed as `--name` to the packer subprocess when set. Both call sites
  (`bundle()`'s own, and `_bundle_linux_deb`'s, which also gained the same new parameter to
  forward it along) pass the already-resolved `window_title` value (`_resolve_window_title`,
  the 2026-08-10 window-title entry) â€” the exact same "game's display name" source, reused
  rather than re-resolved separately.

**Why reuse `window_title` instead of a fresh CLI flag/resolution:** `_resolve_window_title`
already implements exactly the lookup this needed (`manifest.json`'s `name`, falling back to
`package.json`'s), and by the time `_place_bundle_content` runs, `bundle()` has already called
it. One resolution, two uses (window title, cache directory name), rather than two separate
(and potentially divergent) name sources.

**Verification:** `cargo test -p roves-content-packer` â€” all 6 pre-existing `roundtrip.rs`
cases (both `PackOptions` literals there updated with `name: None`) plus the `size` unit test
and the 4 new unit tests (`sanitize_path_segment`/`default_dest`) pass; a real, non-mocked run
through `pack`/`load_manifest`/`prepare_dest`, not just a type-check. `python3 -m py_compile
python/servo/post_build_commands.py` passes. `cargo check -p servoshell` could not be
completed in this environment for the `bundle_launch.rs` change specifically â€” same
`libudev-sys` gap noted in the entry above; reviewed by hand instead (a single-line call-site
change, `resolve_dest(&content_dir, None, manifest.name.as_deref())`, using a binding â€”
`manifest` â€” already in scope one line above it).

---

## 2026-08-12 â€” Installable packages on Windows/macOS too: `mach bundle --msi`/`--dmg`

**Files:** `python/servo/post_build_commands.py`, new file
`support/windows/roves-bundle.wxs.mako`, `../.github/workflows/test.yml`.

**Patch:** `patches/servo-v0.4.0/0027-add-msi-dmg-installer-support.patch`

**Upstream behavior:** no equivalent for `mach bundle` (a Roves-added command, see the
2026-08-06 entry above) â€” `--deb` was the only installable-package option `mach bundle` had,
and only on Linux. Windows and macOS only ever produced the portable `play.exe`/`Roves.app`.
Separately, upstream's own *unrelated* `./mach package` command (`package_commands.py`) does
build a WiX `.msi` (Windows) and a `.dmg` (macOS) â€” but for the bare stock `servoshell`
binary, not wired to `mach bundle`'s content-dir-bundling/packed-content machinery at all.

**Why now:** asked directly â€” this fork ships to end users as a real game distribution,
where "download, double-click, play, no install" (the portable bundle) and "download an
installer, install it like any other app" are both things a game's own release pipeline
might want, per platform, exactly like Tauri's own bundler offers both an unpacked binary and
platform installers (`msi`/`nsis` on Windows, `dmg`/`app` on macOS, `deb`/`rpm`/`appimage` on
Linux) as separate targets â€” see `README.md`'s "Embedding" section on Roves' general posture
of matching Tauri's shape where it makes sense. Only `msi` (Windows) and `dmg` (macOS) are
added here, matching what's actually reusable today (see below); `nsis`/`rpm`/`appimage`
aren't implemented and should follow the same shape if added later, not block on this entry.

**Change:**

- **`--msi` (Windows only, new):** wraps the same portable output `--content-dir`/
  `_bundle_windows` already produce â€” built into a throwaway `_stage` subdirectory of
  `--output` instead of `--output` directly â€” into an installable `.msi` via WiX's
  `candle`/`light` (the same toolset `./mach package`'s own Windows installer uses). New
  `_wrap_windows_msi` renders `support/windows/roves-bundle.wxs.mako`, a **generalized**
  version of `support/windows/servoshell.wxs.mako`'s recursive directory-harvest technique
  (`include_directory`): rather than that template's fixed "servoshell.exe + resources/"
  shape, it walks whatever actually ended up in the staging directory â€” play.exe, its DLLs,
  launch.json, and (only when content wasn't packed, or always for the packed-archive case)
  whichever subfolder the html file's own directory put game content in â€” so it stays
  correct regardless of a given game's `--content-dir`/`--content-compress` combination,
  unlike hand-listing files the way the upstream template does. `Product/@UpgradeCode` is a
  deterministic `uuid5` of `--package-name` (stable across builds of the *same* game, so WiX's
  `MajorUpgrade` recognizes successive installs as upgrades rather than unrelated products);
  `Product/@Version` must be a plain `a.b.c.d` numeric MSI version (new module-level
  `_sanitize_msi_version`, which strips a leading `v` but otherwise raises â€” rather than
  silently mangling â€” on anything that isn't, since `--deb-version`'s old free-text
  tolerance doesn't carry over to a format MSI itself enforces). Requires `candle`/`light` on
  `PATH`; raises `BuildNotFound` with a clear message if missing, same convention as `--deb`'s
  `dpkg-deb` check.
- **`--dmg` (macOS only, new):** wraps the `Roves.app` bundle `_bundle_macos` already
  produces into an installable `.dmg` via `hdiutil`, same approach `./mach package` uses for
  stock Servo's `Servo.app` (including reusing `package_commands.py`'s
  `check_call_with_randomized_backoff` for the same "Resource busy" flakiness `hdiutil` has
  on GitHub Actions) â€” new `_wrap_macos_dmg`, adding the usual `/Applications` symlink next to
  the `.app` inside the mounted volume for Finder's drag-to-install gesture.
- **`--deb-package-name`/`--deb-version` renamed to `--package-name`/`--package-version`**
  (same defaults, `roves`/`0.0.0`): now shared across `--deb`/`--msi`/`--dmg` instead of three
  separate per-format flag pairs, since all three are "give this package a name and a
  version," not something specific to `.deb`. No compatibility shim for the old flag names â€”
  no tagged Roves release exists yet that could depend on them, and `../roves-action` (which
  mirrors this exact CLI, see `../CLAUDE.md`'s "keep roves-action in sync" section) is updated
  in the same turn as this entry.µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥ëZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥à¢Ò¢¦'VæFÆR‚–w2–çFW&æÂfÆ÷s¢¢¢&÷F‚æWrf÷&ÖG27FvR–çFòÆ÷WGWCâõ÷7FvV†â÷&F–æ'¢7V&F—&V7F÷'’Â'V–ÇBf–F†R¦W†7B6ÖR¢ö'VæFÆU÷v–æF÷w6öö'VæFÆUöÖ6÷6°¢÷Æ6Uö'VæFÆUö6öçFVçF6ÆÇ2F†R÷'F&ÆRF‚Ç&VG’W6VBÂVæ6†ævVB’ÂF†VâvWBw&V@¢–çFòF†R&VÂ–ç7FÆÆW"f–ÆRw&—GFVâ–çFòÒÖ÷WGWF—G6VÆbÂgFW"v†–6‚÷7FvV—0¢FVÆWFVB(	BÖ—'&÷&–ærÒÖFV&w2W†—7F–ær¶w&ö÷F×F†VâÖFVÆWFR6†RÂ6òÒÖ÷WGWFVæG2W ¢6öçF–æ–æröæÇ’F†Rf–æÂ–ç7FÆÆW"'F–f7BV—F†W"v’ÂæWfW"Ö—‚öb7Fv–ærf–ÆW2æ@¢F†R–ç7FÆÆW"à¢Ò¢¦ââòæv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆ¢¢¢F†RÖG&—‚w&Wrg&öÒBVçG&–W2‡v–æF÷w2ÂÖ6÷2ÂÆ–çW‚À¢Æ–çW‚ÖFV"’Fòb(	BWfW'’ÆFf÷&Òæ÷rvWG2&÷F‚—G2÷'F&ÆR¦ö"æB—G2–ç7FÆÆW"¦ö ¢†v–æF÷w6¶×6–ÂÖ6÷6¶FÖvÂÆ–çW†¶FV&’Âf–æWr6¶vUöÖöFVÖG&—‚†—2Â&F†W ¢F†âöæÇ’Æ–çW‚†f–ærâ–ç7FÆÆ&ÆRf&–çBâFFVB7FWWGF–ærv•‚w2&–âö‡&W6Vç@¢'WBæ÷BöâD†'’FVfVÇBöâF†Rv–æF÷w2ÖÆFW7F'VææW"–ÖvRÂv†–6‚6†—2v•‚FööÇ6W@¢c2&RÖ–ç7FÆÆVBW"7F–öç2÷'VææW"Ö–ÖvW6’öçFòD†ÂvFVBöâ6¶vUöÖöFRÓÒv×6’và¢V6‚õ2w2Gvò¦—'F–f7G2&Ræ÷ræÖVBF—7F–æ7FÇ’†6W'f÷6†VÆÂ×FW7EóÆ÷3âÓÆÖöFSâç¦—’6ğ¢F†R÷'F&ÆRæB–ç7FÆÆW"¦ö'2rWÆöG2FöâwB6Æö&&W"V6‚÷F†W"VæFW"F†R6ÖR&öÆÆ–æp¢'FW7B"&VÆV6Rà ¢¢¥6–FRVffV7G2Fò¶æ÷r&÷WBv†VâWw&F–æs¢¢¢æöæRöbF†—2FWVæG2öâ6W'fò–çFW&æÇ2&W–öæ@§v†BF†R##bÓ‚ÓbÖ6‚'VæFÆVVçG'’Ç&VG’FöW6âwB(	BvWEö&–æ'•÷F‚‚–ö6VÆbçF&vWFÀ§7F&ÆRÆ÷rÖÆWfVÂ6öÖÖæD&6V’â–bgWGW&R6W'fòfW'6–öâ6†ævW2v†@¦6÷•÷v–æF÷w5öFÆÇ5÷Fõö'V–ÆEöF—&V7F÷'–G&÷2æW‡BFòF†Rv–æF÷w2&–æ'’ÂF†R6ÖRDÄÂvÆö ¦ö'VæFÆU÷v–æF÷w6Ç&VG’&VÆ–W2öâ†æB÷w&÷v–æF÷w5ö×6–†'fW7G2f–—G2vVæW&–0¦F—&V7F÷'’vÆ²’æVVG2&V6†V6¶–ær(	BæòæWrFWVæFVæ7’–çG&öGV6VB'’F†—2VçG'’7V6–f–6ÆÇ’à ¢¢¥fW&–f–6F–öã¢¢¢—F†öã2Ö2&–×÷'B7C²7Bç'6R‚âââ’&öâ÷7Eö'V–ÆEö6öÖÖæG2ç– §76W2‡7–çF‚öæÇ’(	Bæò'W7B6†ævVBÂ6òæò6&vò6†V6¶ö6&vòFW7FæVVFVB†W&R’à¦&÷fW2Ö'VæFÆRçw‡2æÖ¶öv2&VæFW&VBF—&V7FÇ’v—F‚Ö¶òçFV×ÆFRåFV×ÆFVv–ç7B§7–çF†WF–27Fv–ærF—&V7F÷'’†fÆBf–ÆW2²æW7FVB7V&föÆFW"Â7FæF–ær–âf÷"&VÀ¦Æ’æW†V´DÄÇ2¶ÆVæ6‚æ§6öæ·6¶VBÖ6öçFVçBÆ–÷WB’æBF†R÷WGWB'6VBv—F€¦†ÖÂæWG&VRäVÆVÖVçEG&VVFò6öæf—&Ò—Bw2vVÆÂÖf÷&ÖVB„ÔÂv—F‚F†RW‡V7FVB&V7W'6—fP¦ÄF—&V7F÷'“æöÄ6ö×öæVçCæöÄ6ö×öæVçE&Vcæ7G'V7GW&R(	BF†—26öæf—&×2F†R§FV×ÆFRÆöv–2 ¦—26÷'&V7BÂæ÷BF†Bv•‚w2÷vâ6æFÆVöÆ–v‡F66WB—BÂ÷"F†B†F—WF–Æ÷F†RFV"F€§7F–ÆÂv÷&³¢æöæRöbÒÖ×6–öÒÖFÖvöÒÖFV&†æ÷"WfVâF†R&RÖW†—7F–ær÷'F&ÆRF‡2’vW&P¦W†W&6—6VBF‡&÷Vv‚&VÂâöÖ6‚'V–ÆF²âöÖ6‚'VæFÆV–âF†—2Vçf—&öæÖVçB†æòv–æF÷w2ğ¥'W7BFööÆ6†–âf–Æ&ÆR†W&R’(	BG&VBF†RæW‡B&VÂ4’'Vâöbââòæv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆ ¢†öæ6R—Bw27GVÆÇ’'Vææ–ær6öÖWv†W&R(	B6VR—G2÷vâ†VFW"6öÖÖVçBöâv‡’—Bw2F÷&ÖçBFöF’¦2F†R&VÂfW&–f–6F–öâÂæBf—‚Wç—F†–ærF†BFöW6âwB7W'f—fR6öçF7Bv—F‚&VÂv•‚ğ¦†F—WF–Æ&Vf÷&R&VÇ––æröâF†—2à ¢¢¤6÷'&V7F–öâ‡6ÖRF’ÂgFW"&VÂ4’'Vâ’(	B÷WGWEöF—&v6âwBÖFR'6öÇWFS¢¢¢W†7FÇ§F†R¶–æBöb'VrF†R6fVB&÷fRv2†VFv–ærv–ç7Bâ&VÂv–æF÷w2ÖÆFW7F'Vâö`¦ÒÖ×6–f–ÆVB–âÆ–v‡Fv—F‚Ät…C3¢F†R7—7FVÒ6ææ÷Bf–æBF†Rf–ÆP¢rââ÷&VÆV6UÅ÷7FvUÇÆ’æW†Rv†æBF†R6ÖRf÷"WfW'’÷F†W"f–ÆR–âF†R'VæFÆR’â&ö÷B6W6S ¦÷WGWEöF—&†g&öÒÒÖ÷WGWFÂRærâââ÷&VÆV6V(	B&VÆF—fRFòÖ6‚'VæFÆVw2÷vâ7vB’v0¦æWfW"&W6öÇfVBFòâ'6öÇWFRF‚Â6ò7FvUöF—&ö×6•ö'V–ÆEöF—&†&÷F‚FW&—fVBg&öÒ—B§7F–VB&VÆF—fRFöòâ÷w&÷v–æF÷w5ö×6–F†Vâ6F2–çFò×6•ö'V–ÆEöF—&&Vf÷&R–çfö¶–æp¦6æFÆVöÆ–v‡F(	BæBv•‚&W6öÇfW2V6‚Äf–ÆR6÷W&6SÒ"âââ#æF‚†&¶VB–çFòF†Rçw‡6 ¦g&öÒF†B6ÖR&VÆF—fR7FvUöF—&’&VÆF—fRFò¦—G2÷vâ¢7vBBF†Bö–çBÂæ÷BF†R7vBF†P§F‚7G&–ærv2÷&–v–æÆÇ’&VÆF—fRFòâv—F‚7vBæ÷röæRÆWfVÂFVWW ¢†&VÆV6Rõö×6’Ö'V–ÆF–ç7FVBöb6W'fò×7&6’Â&W6öÇf–ærF†R&VÆF—fRââ÷&VÆV6Rõ÷7FvRòââæ ¦g&öÒF†W&RF÷V&ÆVBF†R&VÆV6V6VvÖVçB†&VÆV6R÷&VÆV6Rõ÷7FvRòââæ’Âv†–6‚FöW6âwBW†—7Bà¤f—†VBv—F‚öæRÖÆ–æR6†ævS¢÷WGWEöF—"ÒF‚æ'7F‚†÷WGWB÷"F‚æ¦ö–â†&–æ'•öF—"À¢&'VæFÆR"’––ç7FVBöbW6–ær÷WGWF÷F†R¦ö–æVBF‚2Ö—2(	BWfW'’F‚FW&—fVBg&öÒ—@¢†7FvUöF—&Â×6•ö'V–ÆEöF—&ÂF†Rçw‡6w26÷W&6SÖGG&–'WFW2ÂF†Rf–æÂæ×6–öæFÖv §F‚’—2æ÷r'6öÇWFRg&öÒF†R7F'BÂ–Ö×VæRFòv†–6†WfW"7vB6æFÆVöÆ–v‡Fö†F—WF–Æ ¦7GVÆÇ’'Vâg&öÒâÒÖFÖvöÒÖFV&æWfW"6B‚–VÇ6Wv†W&RÖ–BÖ6öÖÖæBÂ6òF†W’Æ–¶VÇ’vW&Vâw@¦7GVÆÇ’'&ö¶Vâ'’F†—2(	B'WBF†Rf—‚Æ–W2Fò÷WGWEöF—&—G6VÆbÂW7G&VÒöbÆÂF‡&VRÀ§6ò—Bw2æ÷Bv–æF÷w2öÒÖ×6–×7V6–f–2F6‚âföÆFVB–çFòF†R6ÖP¦#rÖFBÖ×6’ÖFÖrÖ–ç7FÆÆW"×7W÷'BçF6†&F†W"F†âæWröæRÂ6–æ6R—B6÷'&V7G2'Vp¦–âF†B6ÖRæ÷B×–WB×&VÆV6VB6†ævRÂæ÷B6†ævRFòÇ&VG’×6†—VB&V†f–÷"à ¢ÒÒĞ ¢22##bÓ‚Ó"(	B&ö÷B7Æ6‚&VFW6–vã¢ÖWFÂÖæ–v÷&FÖ&²Â7V&VBv†—FR&öw&W72&  ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6âÇ6òæWrf–ÆW2&W6÷W&6W2öföçG2ğ¤ÖWFÄÖæ–Õ&VwVÆ"çGFfÂ&W6÷W&6W2öföçG2ôÖWFÄÖæ–ÔôdÂçG‡FÂæB&W6÷W&6W2ğ§&÷fW5÷v÷&FÖ&²ç7fv(	Bæ÷B'BöbF†RF6‚‡6ÖR&V6öæ–ær2FW7B×vR÷V&Æ–2ö–6öâçævğ¦–6öâæ–6ö–âF†R$vÖR×7WÆ–VB–6öâ"VçG'’&÷fS¢Æ–â&–æ'’76WG2Âæ÷BFW&—fVBg&öÒç§W7G&VÒf–ÆRÂæBFW‡BÖ&6VBVæ–f–VBF–fb6âwB&W&W6VçBæWr&–æ'’f–ÆR6öçFVçBBÆÂ’à¥VæÆ–¶RF†B–6öâ&V6VFVçBÂÖWFÄÖæ–Õ&VwVÆ"çGFf¦—2¢6öÖWF†–ærâöÖ6‚'V–ÆFæVVG2Fğ¦f–æBöâF—6²†wV’ç'6w2–æ6ÇVFUö'—FW2’(	Bââòæv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆw2&F÷væÆöB°§F6‚6W'fò6÷W&6R"7FWæ÷rÇ6ò6÷–W2&W6÷W&6W2öföçG2ö–çFòF†R&V6öç7G'V7FVBG&VRÂF†P§6ÖRv’—BÇ&VG’Ö—'&÷'2FW7B×vR÷V&Æ–6öFW7B×vRöF—7F–âf÷"F†R6ÖR&V6öâà¦&÷fW5÷v÷&FÖ&²ç7fv—6âwB&VfW&Væ6VB'’ç’'W7B6öFRÂ6ò—BFöW6âwBæVVBF†BG&VFÖVçB(	@¦—Bw2&WòÖöæÇ’FW6–vâ76WBà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó#‚Ö&ö÷B×7Æ6‚×v÷&FÖ&²ÖföçBÖæB×&öw&W72Ö&"çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢æòWV—fÆVçB(	B&Vf–æW2F†R##bÓ‚Ó’$æF—fR&ö÷B7Æ6‚"VçG'’w0¦wV“£§WFFU÷7Æ6††gW'F†W"WÚ±î¸Â¸­yêë¢°k¢G§¦*^this file).

**Asked directly:** the existing splash's "Roves" label rendered in egui's plain default
font, and the progress bar was a stock `egui::ProgressBar` (rounded corners, default theme
fill color) â€” visually generic, not an intentional design.

**Change:**

- **New asset, `resources/roves_wordmark.svg`:** a small, self-contained SVG â€” the existing
  splash icon (`resources/servo_64.png`, embedded as a base64 `<image>`) beside the "Roves"
  wordmark, set in **Metal Mania** (embedded as a base64 `@font-face` in an inline
  `<style>`, so the file renders correctly anywhere without the font installed system-wide â€”
  verified by rendering it with `cairosvg` both before and after installing the font locally:
  without it, text fell back to a generic sans-serif; with it, or in any real `@font-face`-
  respecting renderer/browser, which is what actually matters since the font travels with
  the file, it renders in Metal Mania). This is a standalone design asset, not something
  rendered live by the engine â€” see the next point for why.
- **`resources/fonts/MetalMania-Regular.ttf` + `MetalMania-OFL.txt`:** the actual font file
  (from Google Fonts, `fonts.gstatic.com/s/metalmania/v23/...`), bundled for the engine to
  embed at compile time, plus its SIL Open Font License 1.1 text (`Copyright (c) 2012 by Open
  Window ... Reserved Font Name "Metal Mania"`) â€” OFL permits embedding/redistribution
  royalty-free; this is the first third-party font actually bundled into the binary (the
  existing CJK fallback fonts `configure_fonts`/`load_cjk_fonts` load are read from the
  *player's own OS* at runtime, never shipped).
- **`gui.rs`, new `add_wordmark_font`:** registers `MetalMania-Regular.ttf`
  (`include_bytes!` + `FontData::from_static`, no per-launch disk read) under its own egui
  font family, `FontFamily::Name("Metal Mania")` â€” deliberately *not* pushed onto
  `FontFamily::Proportional`'s fallback chain the way `load_cjk_fonts` does for CJK, since
  this is a one-off display font for a single label, not something the rest of the UI should
  ever fall back to. Called from `Gui::new` right after `configure_fonts()`, unconditionally
  on every platform (unlike `configure_fonts` itself, which is platform-gated for CJK system-
  font probing) â€” required promoting the `FontData`/`FontFamily` imports from
  `#[cfg(any(windows, linux, freebsd))]`-gated to unconditional, since macOS now needs them
  too.
- **`update_splash`:** the "Roves" label now renders via `egui::RichText` with
  `FontId::new(34.0, FontFamily::Name("Metal Mania"))` instead of a plain `colored_label`.
  The gap between the icon+wordmark row and the progress bar grew from 12px to 20px (asked
  for explicitly â€” the bar sits "a bit further below" the wordmark, not immediately
  adjacent). The progress bar itself gained `.fill(Color32::WHITE)` (white, was the default
  theme accent color) and `.corner_radius(CornerRadius::ZERO)` (squared-off, was egui's
  default rounded-rect) plus an explicit `.desired_height(18.0)` for a consistent thin bar
  regardless of theme defaults. The manual vertical-centering fudge factor (`available_height
  / 2.0 - N`) was bumped from `40.0` to `51.0` to account for the larger gap â€” still an
  estimate, not computed from actual measured widget sizes (egui only knows a widget's real
  size after laying it out), same caveat the original value already carried.

**Why not render `roves_wordmark.svg` itself in the running splash:** there is no SVG
rasterization path anywhere in this engine or its dependencies (confirmed â€” no `resvg`/
`usvg` or equivalent; `favicon.svg`/`icon.svg`'s existing uses are all for the *unrelated*
game-icon-fallback feature, never rasterized at runtime either). Adding one just to redraw a
static icon+text lockup egui can already compose natively (an `egui::Image` for the icon,
`RichText` with a loaded font for the text â€” exactly what `update_splash` already did before
this change, now with the right font) would be a disproportionate new dependency for no
visual benefit: egui's own text rendering is already a vector/font rasterizer, not a bitmap
fallback. The SVG asset exists for uses *outside* the running engine (marketing, the wiki,
README embeds, etc.) where an actual SVG file is the right format.

**Side effects to know about when upgrading:** none of this touches Servo internals beyond
`egui`'s own stable, high-level widget API (`RichText`, `FontId`, `FontFamily`,
`ProgressBar`) â€” should survive a version bump untouched unless a future egui major version
renames `CornerRadius` (it was `Rounding` before egui ~0.32) or changes `FontData`'s
construction API.

**Verification:** `rustfmt --check --edition 2024 ports/servoshell/desktop/gui.rs` reports no
diff anywhere in the changed regions (the one pre-existing diff it does report, at an
unrelated `if let ... &&` chain further down the file, predates this change â€” confirms the
edit is both syntactically valid and already correctly formatted, without needing a full
build). A real `cargo check -p servoshell` still isn't completable in this environment â€” same
`libudev-dev`/`pkg-config` gap noted in earlier entries (`pkg-config --exists libudev` fails
here; only the runtime `libudev1` package, not `libudev-dev`, is installed), unrelated to
this change specifically. `resources/roves_wordmark.svg` was validated as well-formed XML
(`xml.etree.ElementTree`) and actually rendered to a PNG with `cairosvg` (see above) to
confirm the layout/embedding works, not just that the markup parses. Treat the next real
`./mach build` + manual run of this fork as the actual visual verification (exact pixel
centering in particular, given the fudge-factor caveat above) before considering this done.

---

## 2026-08-12 â€” Boot splash resize, manual centering, and readable progress bar

**Files:** `ports/servoshell/desktop/gui.rs`, `ports/servoshell/desktop/app.rs`,
`ports/servoshell/desktop/headed_window.rs`.

**Patch:**
`patches/servo-v0.4.0/0029-boot-splash-resize-recenter-and-progress-bar-redesign.patch`

**Upstream behavior:** no equivalent â€” refines the 2026-08-09 "Native boot splash" and
2026-08-12 "Boot splash redesign" entries' `Gui::update_splash` (further up this file).

**Asked directly, from a screenshot of a real launch:** four things wrong with the splash
from the previous entry â€” (1) the icon+wordmark row rendered pinned to the left edge instead
of centered; (2) the icon (`resources/servo_64.png`) and "Roves" wordmark read as too small,
with the wordmark not enough bigger than the icon; (3) the progress bar was too thick and, at
any fill level, visually indistinguishable from an empty bar; (4) no bar was visible at all
before extraction actually started.

**Root causes, investigated before changing anything:**

- **(3) is the real bug, not a perception issue.** `egui::ProgressBar` paints its track in
  `visuals.extreme_bg_color`, and paints its fill in whatever `.fill()` is given. This app
  sets `options.fallback_theme = egui::Theme::Light` (`Gui::new`) and doesn't otherwise force
  a dark theme, so on a system reporting (or defaulting to) a light theme,
  `extreme_bg_color` is `Color32::from_gray(255)` â€” pure white â€” which the previous entry's
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
  centered by explicit, *measured* horizontal padding instead â€” see below. This sidesteps the
  question of whether the old approach was actually buggy or something else was going on,
  since the new approach is correct either way.
- **On the icon itself:** `resources/servo_64.png` (loaded by
  `load_splash_icon_image`/`Gui::update_splash`) was checked pixel-by-pixel against
  `icon.svg` (repo root) and is a 64px rasterization of it â€” a generic, Recraft-AI-generated
  "three wolf heads in chains" clip-art image, not a Roves logo. No Roves-branded icon asset
  (matching the colorful lockup mark referenced in chat) exists anywhere in this repo, its
  three sibling checkouts (`roves-action`, `roves-api`, `roves-wiki`), or
  `resources/roves_wordmark.svg`'s own embedded icon (byte-identical to `servo_64.png`, i.e.
  the same wolf placeholder, not a different asset). **This entry does not change the icon
  file** â€” swapping in real Roves branding needs that asset supplied first; asked about
  separately in chat rather than guessed at here.

**Change:**

- **`gui.rs`, new constants:** `SPLASH_ICON_SIZE` (64.0 â†’ 128.0) and
  `SPLASH_WORDMARK_FONT_SIZE` (34.0 â†’ 88.0), the latter kept at the same icon-height-relative
  proportion (44/64 â‰ˆ 0.69) as the reference lockup in `resources/roves_wordmark.svg`, just
  scaled to the new icon size â€” satisfies "both bigger, wordmark a bit bigger relative to the
  icon than before" without inventing a new ratio.
- **Manual horizontal centering:** `update_splash` now measures the wordmark's actual pixel
  width via `ctx.fonts_mut(|fonts| fonts.layout_no_wrap(...))` before building the row, then
  inserts a leading `ui.add_space(...)` inside the `ui.horizontal` sized to
  `(available_width - (icon_width + spacing + wordmark_width)) / 2`. This replaces reliance
  on `top_down(Align::Center)` centering the nested row on its own with an exact, measured
  centering that doesn't depend on that behavior at all â€” a stronger fix than re-guessing at
  whatever `Align::Center` was or wasn't doing.
- **New `draw_splash_progress_bar`:** replaces the `egui::ProgressBar` widget with a
  hand-painted track (`Ui::painter().rect_filled` + `rect_stroke`, a dim translucent-white
  fill with a brighter outline, `SPLASH_PROGRESS_BAR_HEIGHT = 6.0` thin, `CornerRadius::same(2)`
  â€” still visibly rectangular, per feedback, just softened at the corners rather than the
  previous hard square) and an opaque white fill rect sized to `progress`, drawn on top.
  Always draws the track, even at `progress == 0.0`, so the bar reads as "a loading indicator
  that's currently empty" rather than disappearing. Considered adding a crate for this
  (offered in chat) but a hand-painted rect is a few lines against an API already in use
  elsewhere in this file, with the same "not worth a new dependency for something this
  simple" reasoning as the previous entry's SVG-rendering call â€” no new dependency added.
- **`update_splash`'s signature, `progress: Option<f32> â†’ f32`:** now that the bar is always
  drawn, `None` (meaning "don't draw it yet") no longer has a use â€” the type change makes
  that explicit instead of leaving a now-meaningless `Option` around. Propagated through
  `HeadedWindow::paint_splash` (`headed_window.rs`) and its one call site
  (`app.rs`'s `window_event`, now just `headed_window.paint_splash(*progress)`).
- **`app.rs`, removed `SPLASH_PROGRESS_BAR_DELAY`:** existed solely to gate *when* the bar
  started rendering (`(elapsed >= SPLASH_PROGRESS_BAR_DELAY).then_some(*progress)`); with the
  bar now unconditional, that gate was dead weight. `try_finish_booting`'s and `init`'s
  `ControlFlow::WaitUntil` re-arm logic â€” previously waking at whichever of
  `SPLASH_PROGRESS_BAR_DELAY`/`MIN_SPLASH_DURATION` hadn't yet passed â€” now just targets
  `MIN_SPLASH_DURATION` directly, since that's the only remaining thing `try_finish_booting`
  waits on. Behavior is unchanged in the case that matters (the busy-poll-until-extraction-
  done path once `MIN_SPLASH_DURATION` has already elapsed): both old and new code compute a
  zero wait there, verified by re-deriving the arithmetic, not just inspection.

**Side effects to know about when upgrading:** same as the previous entry â€” plain, stable
`egui` widget/painter API (`Ui::painter`, `rect_filled`, `rect_stroke`, `Fonts::layout_no_wrap`),
nothing touching Servo internals. `StrokeKind` (required by `rect_stroke` in this egui
version) is worth re-checking if painter signatures change in a future egui major version.

**Verification:** unlike every prior boot-splash entry, this one did *not* stop at
`rustfmt --check` and static reading â€” `cargo check -p servoshell` was actually attempted
(still blocked here on the documented `libudev-dev` gap, this time confirmed directly rather
than assumed, and additionally on this sandbox being too resource-constrained to finish
compiling `mozangle`'s bundled ANGLE via cargo check on the full workspace even with the
`gamepad` feature disabled to dodge libudev â€” a build-script `cc`/C++ compile got OOM-killed).
Given that, the actual new code (`draw_splash_progress_bar`, the measured-centering logic in
`update_splash`, and the `EguiGlow::run`/`CentralPanel::show` interaction the previous entry's
"deprecated `Panel::show`" comment alludes to) was instead verified by compiling it for real
in an isolated throwaway crate pinned to the exact same `egui = "=0.34.3"` / `egui_glow =
"=0.34.3"` (with the `winit` feature, matching `ports/servoshell/Cargo.toml`) versions this
workspace resolves to â€” confirmed the `EguiGlow::run` closure parameter is actually
`&mut egui::Ui` (not `&Context`, despite being named `ctx`), that `.show(ctx, ...)` only
type-checks via `Ui`'s `Deref<Target = Context>` impl, and that the new font-measurement and
painter calls compile against the real API. Additionally, `rustfmt --check --edition 2024`
was run on all three changed files (clean on every changed region â€” the several pre-existing
unrelated diffs it reports elsewhere in `app.rs`/`gui.rs`/`headed_window.rs`, all stale
`if let ... &&`-chain formatting, predate this change and were deliberately left alone). Most
importantly, `patches/servo-v0.4.0/0029-...patch` was verified end-to-end against a fresh
pristine `v0.4.0` download: extracted clean, applied patches `0001` through `0028` in order
(all applied without a reject, two with a harmless line-offset), confirmed the result was
byte-identical to this repo's own `HEAD` for all three files, then applied `0029` on top and
confirmed *that* result was byte-identical to the actual working tree â€” i.e. the patch is
proven mechanically reproducible from pristine upstream, not just "looked correct." Still
missing: an actual `./mach build` + manual run, same as every prior entry in this section â€”
in particular this doesn't prove the *visual* result (exact centering, whether 128px/88pt
reads as "much bigger" as intended) is right, only that it's what the code says it should be.

---

## 2026-08-13 â€” Regenerate Roves icon raster/format assets from `icon.svg`

**Files:** `resources/servo_64.png`, `resources/servo_1024.png`, `resources/servo.ico`,
`resources/servo.icns` â€” binary raster assets, not part of any patch (same reasoning as the
`test-page/public/icon.png`/`icon.ico` and `resources/fonts/` entries above: a text-based
unified diff can't represent new binary content). Also `.gitattributes` â€” see the last
paragraph below. `resources/servo.svg` and `support/openharmony/.../servo_{64,1024}.png` are
unaffected â€” the former was already byte-identical to `icon.svg` (confirmed via checksum),
and the latter are plain-text path placeholders pointing back at
`resources/servo_{64,1024}.png`, not actual copies, so they pick up this change automatically.

**Upstream behavior:** n/a â€” `icon.svg` (repo root) and all of the assets above are already a
Roves-specific replacement of upstream Servo's own icon (see the 2026-08-09 "Game-supplied
icon" entry's `resources/servo_64.png`/`servo.ico`), added whole-cloth in a prior commit
(`ca839e6`, "icon") that replaced the binaries directly without a documented generation
pipeline.

**Asked directly:** confirm the "Servo-branded, not any particular game's" icon used by the
boot splash (`Gui::update_splash`, previous entries above) and the window/taskbar/exe-icon
fallback (`headed_window.rs`/`build.rs`) is actually generated from `icon.svg`, and convert it
to every extension/size those consumers need. Investigated first: `resources/servo_64.png`
and `resources/servo_1024.png` were already pixel-identical rasterizations of `icon.svg` (not
upstream Servo's own logo â€” confirmed by comparing color histograms and a fresh
`cairosvg` render pixel-for-pixel), and `resources/servo.svg` was already byte-identical to
`icon.svg`. What was actually incomplete: `resources/servo.icns` had only a single `ic09`
(512Ã—512) entry â€” no retina (`@2x`) variants and nothing below 512px, which macOS's Finder/
Dock render poorly at smaller sizes (upscaling a 512px source, or falling back to a generic
icon, depending on context) â€” and `resources/servo.ico` had 16/24/32/48/256px but was
missing the 64px and 128px sizes Windows uses for some Explorer view modes.

**Change:** rebuilt every derived asset from `icon.svg` through one pipeline (`cairosvg` to
rasterize the vector, Pillow for resizing/`.ico` packing, the `icnsutil` Python package for
`.icns` composition â€” all three already available in this environment, no new dependency):

1. Rendered `icon.svg` once at 2048Ã—2048 via `cairosvg.svg2png` as a master raster (this repo
   has no SVG rasterization path at *runtime*, per the 2026-08-12 wordmark entry above, but
   that's about the running engine specifically â€” offline asset generation is a different
   question and unaffected by that constraint).
2. `resources/servo_1024.png`/`servo_64.png`: downsized from the master with Pillow's
   `Image.resize(..., Image.LANCZOS)` â€” content unchanged (still the same icon at the same
   two sizes), but a fresh render rather than a prior possibly-recompressed copy.
3. `resources/servo.ico`: regenerated via `Image.save(..., format="ICO", sizes=[16, 24, 32,
   48, 64, 128, 256])`, each size resampled individually from the 2048px master rather than
   letting the `.ico` encoder cascade-resize from a single frame â€” now has all 7 standard
   Windows icon sizes instead of 5.
4. `resources/servo.icns`: composed via `icnsutil compose` from a full Apple iconset (16, 32,
   128, 256, 512, plus each size's `@2x` retina variant, i.e. 16 through 1024px) â€” 10 entries
   (`icp4`, `ic11`, `icp5`, `ic12`, `ic07`, `ic13`, `ic08`, `ic14`, `ic09`, `ic10`) instead of
   the previous single `ic09`.

**Side effects to know about when upgrading:** none â€” these are pure design assets consumed
via `include_bytes!`/file-copy (`build.rs`, `headed_window.rs`, `gui.rs`), not upstream Servo
files, so there's nothing to reapply against a new tag; just re-run the same pipeline against
`icon.svg` if that source ever changes.

**Verification:** `PIL.Image.open` confirms `servo.ico` now reports all 7 requested sizes
`{16,24,32,48,64,128,256}`; `icnsutil info` confirms `servo.icns`'s 10 entries with the
expected type codes and pixel dimensions above; `file` confirms both are still recognized as
valid MS Windows icon resource / Mac OS X icon files respectively, not just well-formed by
their own tooling's say-so. `md5sum` reconfirmed `resources/servo.svg` is still byte-identical
to `icon.svg` after this change (untouched, as intended). Not done: an actual visual check on
Windows/macOS that Explorer/Finder pick the new sizes correctly â€” no such platform available
here.

**Also:** `.gitattributes` gained `*.icns binary`. `resources/servo.icns` was already tracked
without one (unlike `.ico`, which got its own rule in the 2026-08-09 "Game-supplied icon"
entry above for exactly this reason) â€” same silent-corruption-on-Windows-checkout risk from
falling under the blanket `* text=auto eol=lf` rule, just not caught until this file was
touched again.

---

## 2026-08-13 â€” Startup file logging, so a silently-failing `play.exe` is diagnosable

**Files:** `ports/servoshell/desktop/cli.rs`, `ports/servoshell/desktop/bundle_launch.rs`, new
`ports/servoshell/desktop/logging.rs`, `ports/servoshell/desktop/mod.rs`,
`ports/servoshell/panic_hook.rs`, `ports/servoshell/Cargo.toml`, `components/servo/servo.rs`,
`support/content-packer/src/extract.rs`. `Cargo.lock` picked up the matching `servoshell` â†’
`env_logger` dependency edge automatically â€” not part of the patch, same as every other
`Cargo.lock` change in this file's history (regenerated by Cargo itself, not hand-diffable).

**Patch:**
`patches/servo-v0.4.0/0030-startup-file-logging-for-diagnosing-silent-launches.patch`

**Upstream behavior:** no equivalent â€” a `log` backend (`env_logger`, straight to stderr)
wasn't installed at all until `Servo::setup_logging()` ran, deep inside `App::finish_init`,
itself only reached once a `Servo` instance exists. Every `log::error!`/`warn!` call before
that point â€” including, critically, the ones already inside
`bundle_launch::resolve_bundled_launch_args`/`resolve_packed_content_url` reporting a missing
or corrupt packed-content bundle, exactly the failure this entry exists to diagnose â€” was
silently discarded: `log`'s default no-op logger just drops anything logged before a real one
is installed.

**Asked directly:** "quando provo ad avviare play.exe non succede nulla" (nothing happens on
launch) â€” with no way to tell why. Investigated first, rather than guessed at: `main.rs` sets
`#![windows_subsystem = "windows"]`, so a double-clicked `play.exe` has no console at all
(stderr goes nowhere visible even for the logging that *does* happen after `setup_logging()`
runs); combined with the above gap, a failure anywhere in the first several hundred lines of
startup produced zero observable output, console or otherwise â€” genuinely "nothing happens,"
not just "something happens somewhere invisible."

**Also asked directly:** put the log file next to the extraction cache under
`%LOCALAPPDATA%\<game name>\` (`AppData\Local\Roves test-page\...` was the example given) â€”
not inside the game's own content/install directory â€” and have it start empty on every
launch (no accumulating history across runs) while capturing everything: Roves/Servo's own
logging and the game's own console output.

**Change:**

- **`support/content-packer/src/extract.rs`, new `game_data_dir(game_name)`:** the per-game
  top-level folder under the OS cache directory (`%LOCALAPPDATA%` on Windows,
  `~/Library/Caches` on macOS, `~/.cache`/`$XDG_CACHE_HOME` on Linux) that `default_dest`
  already nested the extraction cache under (`<this>/cache/<hash8>/`) â€” pulled out into its
  own function so a log file can sit at `<this>/roves.log`, a *sibling* of `cache/`, using the
  exact same name-sanitizing/`"roves"`-fallback logic (see `sanitize_path_segment`) instead of
  duplicating it. `default_dest` now calls this instead of inlining the same two lines.
  Existing behavior/tests unaffected â€” verified with `cargo test -p roves-content-packer`
  (all 6 pre-existing tests plus this entry's new `game_data_dir_is_default_dests_cache_grandparent`
  still pass), not just by inspection.
- **`bundle_launch.rs`, new `peek_game_name_for_logging`:** a cheap, side-effect-free,
  deliberately non-logging peek at `launch.json`/`manifest.json` â€” just enough to learn the
  game's name (or `None`) *before* `resolve_bundled_launch_args` itself runs, so `cli::main`
  can pick the right log directory in time to capture that very function's own diagnostics.
  Re-reads the same two small JSON files `resolve_bundled_launch_args` will read again
  properly shortly after, rather than threading a shared result across the gap â€” the two run
  at genuinely different times (this one strictly first), and duplicating a couple of cheap
  file reads is simpler and safer than the alternative.
- **New `ports/servoshell/desktop/logging.rs`:** `init(log_dir)` creates `log_dir` if needed,
  opens `log_dir.join("roves.log")` with `File::create` (truncates â€” a fresh, empty file every
  launch, exactly as asked), and installs an `env_logger`-style logger targeting that file
  (`env_logger::Target::Pipe`) as the process's global `log` backend. Defaults to `info`-level
  filtering (`env_logger`'s own default is `error`-only) since a private log file, unlike a
  terminal, isn't noisy for the user; still overridable via `RUST_LOG`. No new hook was needed
  to capture the game's own console output: `headed_window.rs`/`headless_window.rs`'s
  `show_console_message` already forwards every `console.log`/`warn`/`error` through
  `log::log!`, and `panic_hook.rs` already routes panics through `log::error!` â€” so a single
  logger installed here transparently captures Roves/Servo's own logging, the game's console
  output, and startup panics, all in one file, none of it requiring changes beyond installing
  the logger early enough.
- **`cli.rs`, `main()`:** calls the above â€” `logging::init(&extract::game_data_dir(peek_game_name_for_logging().as_deref()))`
  â€” as close to the top of `main` as possible (right after `crash_handler::install`/
  `init_crypto`, before `panic::set_hook` and everything else). Gated on
  `env::args().nth(1).is_none()`, the exact same "is this a genuine double-click/bundled
  launch" check `resolve_bundled_launch_args` already uses â€” **not** an arbitrary restriction,
  but a correctness requirement: Servo's own multiprocess content-process children re-exec
  *themselves* with `--content-process <token>` in argv, and each one installing its own
  *truncating* file logger would race every other process (including the main one) writing to
  that same path, each wiping out whatever the others had already logged. Content processes
  now behave exactly as before this entry (unaffected â€” they still get `Servo`'s own
  content-process `set_logger`, to stderr).
- **`components/servo/servo.rs`, `setup_logging`/`set_logger`:** `log::set_boxed_logger` only
  ever succeeds once per process, and now that `logging.rs` installs its own logger earlier
  (for the main/chrome process), Servo's later attempt was guaranteed to hit that and panic on
  the `.expect("Failed to set logger.")` that used to be here â€” checked this against the
  installed `log` crate's own docs/source, not assumed. Changed both call sites to a plain
  `if log::set_boxed_logger(...).is_ok() { log::set_max_level(filter); }`: whichever logger
  installs *first* wins silently, no panic. Trade-off, stated plainly rather than silently
  accepted: `FromEmbedderLogger`'s constellation-forwarding (an embedder-side crash/warning-UI
  hook) never runs once servoshell's own logger has already installed â€” this fork has no such
  UI to forward to (see this file's very first entries, removing the toolbar/tabs entirely),
  so that's an intentional no-op, not a quietly dropped feature something else still expects.
- **`panic_hook.rs`:** the panic message's file:line/thread detail (previously written to
  stderr only) is now built once and passed to *both* the stderr write and the final
  `log::error!` call â€” previously that call only logged the bare message, missing exactly the
  detail most useful for diagnosing *which* panic happened, in the one sink (the log file)
  actually likely to be visible on a windowed build.

**Side effects to know about when upgrading:** `setup_logging`/`set_logger`'s guard is a
one-line behavioral change against upstream `Log`/`env_logger` APIs (`log::set_boxed_logger`
returning `Result`) that have been stable for a long time â€” low risk on a version bump.
Everything else lives entirely in `ports/servoshell`/`support/content-packer`, untouched by
upstream Servo changes by construction.

**Verification:** `cargo test -p roves-content-packer` (all tests pass, including the new
one). `logging.rs`'s core logic (file truncation, `env_logger` target/filter wiring,
`log::set_boxed_logger` installation) was compiled and actually run â€” not just read â€” in an
isolated throwaway crate pinned to this workspace's exact `env_logger`/`log` versions,
confirming a real log file gets created, truncated, and populated with both an explicit test
log line and this module's own startup line. `peek_game_name_for_logging` +
`extract::game_data_dir` were likewise run together (not just inspected) against a real
fixture `launch.json`/`manifest.json` pair, confirming the resolved directory matches
`%LOCALAPPDATA%\Roves test-page\` exactly as asked. `rustfmt --check --edition 2024` is clean
on every changed region of every file except `extract.rs`'s two pre-existing, unrelated
`&&`-chain-formatting diffs (confirmed pre-existing by checking the *unmodified* `HEAD`
version of that file reports the identical two diffs â€” not something this change introduced).
`patches/servo-v0.4.0/0030-...patch` was verified end-to-end: applied cleanly on top of a
fresh pristine `v0.4.0` extraction with patches `0001`â€“`0029` already applied (itself
reconfirmed byte-identical to this repo's own `HEAD`), and the result after applying `0030` is
byte-identical to the actual working tree. Not done, same caveat as every entry in this
section: an actual `./mach build` + manual run on a real Windows machine, to confirm a
genuinely broken launch now actually produces a readable `roves.log` instead of nothing.

**Update, same day â€” milestone logging, after this alone still wasn't enough:** a real
Windows portable-bundle test came back with exactly one line in `roves.log` â€” this module's
own "Roves logging started" line â€” and nothing else, no matter what actually failed. That
still leaves a huge unlogged span (window/GL context creation, boot extraction, Servo
construction) where a hang or a hard native crash (GPU driver, ANGLE/GL context issue, a
missing DLL) can happen without ever reaching `panic_hook.rs` â€” a native crash of that kind
bypasses Rust's panic machinery entirely, so no amount of `.expect()`-to-`log::error!`
plumbing in Rust code would have caught it. Added bracketing `log::info!` calls (paired
before/after) at the remaining startup milestones most likely to hide such a crash:
`cli::main` (resolved launch args, parsed CLI args, event loop created, entering
`run_app`), `App::init` (immediately around `create_platform_window` â€” winit window + GL
surface creation, the single most GPU/driver-crash-prone step in this whole path), and
`App::finish_init` (immediately around `servo_builder.build()`). Also bracketed `Gui::new`'s
first `update_splash`/`paint` call (`gui.rs`) â€” the actual first GL draw/buffer-swap this
process makes, right after context creation, and therefore just as plausible a native-crash
site as context creation itself. None of this is meant to be permanent â€” it's deliberately
coarse-grained, commented as such, and should come back out once a real crash has actually
been localized this way; it's the diagnostic equivalent of `println!`-debugging, not a
lasting change to how this app logs.

**Update, same day â€” found and fixed the real bug the milestone logging was chasing:** the
milestone logging above worked immediately â€” a real Windows portable-bundle run logged
`resolved launch args: [..., "--package-name", "servoshell-test", "--package-version",
"0.4.0", ...]` and then nothing further, meaning the crash was between that line and
`parsed command line arguments`. `--package-name`/`--package-version` are `mach bundle`'s
*own* flags (`python/servo/post_build_commands.py`, used only to name a `--deb`/`--msi`/
`--dmg` output) â€” not anything `ports/servoshell/prefs.rs`'s CLI parser recognizes. Somehow
(root mechanism not fully pinned down â€” extensive attempts to reproduce it by faithfully
reconstructing `mach bundle`'s actual registered argparse arguments and re-running
`parse_known_args` with the exact CI invocation kept coming back clean, i.e. **not**
reproducing the leak; this remains an open question) they ended up forwarded into
`launch.json`'s `"args"`, which `bundle_launch.rs` feeds straight into
`prefs::parse_command_line_arguments`. That parser (`bpaf`) rejects unknown flags outright
â€” confirmed directly: running the real, freshly-built Linux `servoshell` binary with these
exact args reproduces `Error: --package-name is not expected in this context`, immediately,
before anything else runs. Combined with `ArgumentParsingResult::ErrorParsing` in `cli::main`
calling `std::process::exit(1)` with no logging on that path, and no console on a
double-clicked Windows build, this is the exact, complete explanation for "play.exe does
nothing."

**Change:** `post_build_commands.py`'s `bundle`, right before building `extra_args`, now
cross-checks every token in `params` against `self.__class__.bundle._mach_command.arguments`
â€” the same metadata `mach`'s own dispatcher (`python/mach/mach/dispatcher.py`) uses to build
the subcommand's parser â€” and drops any that match one of this command's *own* flag spellings
(along with the value that flag takes, looked up via that same metadata's `action`, so a
boolean flag like `--msi` doesn't wrongly eat the next legitimate passthrough token). Prints a
`warning:` line naming exactly what got dropped, so this is loud instead of silently
corrupting `launch.json` again in some future incident of the same shape. This is a defense
against the *symptom* (a reserved flag ending up in `params`) rather than a fix for the
underlying argparse mechanism, precisely because that mechanism wasn't fully pinned down â€”
see above.

**Not part of the `roves-action` sync** (see `CLAUDE.md`'s mirroring requirement): this
changes internal handling of an existing flag, not `mach bundle`'s CLI surface itself â€” no
flag was added, removed, or redefaulted, so `roves-action`'s `action.yml`/`README.md` need no
matching update.

**Patch:** `patches/servo-v0.4.0/0031-filter-mach-bundles-own-flags-out-of-game-launch-args.patch`

**Verification:** the exact filtering logic (reserved-flag detection + value-skipping) was
extracted and unit-tested standalone against four cases â€” the real observed leak (both
`--package-name` and `--package-version` correctly stripped along with their values), a
mix of a leaked boolean flag (`--msi`) and a legitimate passthrough flag+value (both handled
correctly), and two no-op cases (nothing leaked) â€” all four produced the expected
`extra_args`/`leaked_reserved_flags`. The file's own syntax was checked with `ast.parse`
(this sandbox's Python 3.10 can't actually run `mach` itself â€” `python/mach`/`python/tidy`
depend on Python 3.11+ stdlib additions (`contextlib.chdir`, `typing.LiteralString`) this
environment doesn't have, confirmed while trying; not a gap introduced by this change).
`patches/servo-v0.4.0/0031-...patch` was verified end-to-end the same way as every other
patch in this file: applied cleanly on top of a pristine `v0.4.0` extraction with patches
`0001`â€“`0030` already applied, producing a result byte-identical to the actual working tree.
Not done: an actual `mach bundle` run (blocked by the Python version gap above) confirming
the warning prints and `launch.json` ends up clean â€” the next real Windows/`mach`-capable
build should confirm this closes out the original report.

---

## 2026-08-14 â€” Bundled launches no longer die on a broken `launch.json`, and why the previous fix didn't actually close this out

**Update on the entry above:** the `mach bundle` run that entry's own "not done" note asked
for actually happened â€” a real Windows portable bundle was built by `.github/workflows/
test.yml` at the previous entry's commit (`6e3f1f5`, the one adding the `post_build_commands.py`
reserved-flag filter) and run for real. It still crashed, identically to before: `Error:
--package-name is not expected in this context`, no window, no `roves.log` line beyond
nothing at all. `launch.json` in that exact build's bundle still literally contains
`"--package-name", "servoshell-test", "--package-version", "0.4.0"` in its `"args"` array.
**The previous fix does not work.** The same crash was independently confirmed on the
`--msi`-mode bundle's `play.exe` too (its `launch.json` carries the same leaked flags plus
`--msi`) â€” not specific to the plain portable path.

**Root cause, still not fully pinned down â€” new evidence, same conclusion as before:** the
exact `bundle()` reserved-flag filter from the previous entry was extracted verbatim, along
with a byte-faithful reconstruction of `mach`'s own dispatcher (`python/mach/mach/
dispatcher.py`'s `_run_command_handler`, the code that separates the `params` REMAINDER
catch-all from every other registered flag) and the *complete* real argument list for
`bundle` (every `@CommandArgument` on it, plus every flag `common_command_arguments(binary_
selection=True)` adds â€” `--release`, `--dev`/`--debug`, `--prod`/`--production`, `--profile`,
`--with-asan`, `--with-tsan`, `--bin`, `--nightly`/`-n`, `--coverage`, not just the packaging-
specific ones). Fed the *exact* CI invocation (`--content-dir test-page/dist --output
../release --package-name servoshell-test --package-version 0.4.0`) through this reconstruction
in an isolated Python process (no `mach_bootstrap`, no venv): parsing comes back **completely
clean** â€” `package_name`/`package_version` land correctly in `command_namespace`, `params`
ends up empty, nothing for the filter to even do. This exactly reproduces the previous
entry's own "extensive attempts... kept coming back clean" finding, now with the complete
argument set rather than a partial one, closing off "an incomplete reconstruction was masking
it" as an explanation. Also directly ruled out: `mach`'s own polyglot shell wrapper re-execing
via `uv run --frozen python ${MACH_DIR}/mach "$@"` mangling argv before Python ever sees it â€”
tested directly (a throwaway script printing `sys.argv`, invoked both directly and through
`uv run --frozen python`, real `uv` binary, same argument list) â€” argv comes through
byte-identical either way. So the leak genuinely only manifests inside the full `mach`
process (global-argument parsing in `mach.run()`, command/provider registration, or something
else full-app-only) â€” not reproducible against the isolated pieces, and not re-investigated
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
&& args.len() > 1` â€” logs the failing `args` and the fact that a retry is happening, then
calls `parse_command_line_arguments` a second time with just `&args[..1]` (the content URL
alone, every extra arg dropped), and only exits(1) if even *that* somehow fails to parse.
Real (non-bundled) invocations â€” a developer running the shipped binary from a terminal with
their own typo'd flag â€” are completely unaffected: `is_bundled_launch` is `false` for those,
so the existing `ArgumentParsingResult::ErrorParsing => std::process::exit(1)` arm still
applies unchanged, still hard-erroring exactly as before. The distinction matters: a CLI typo
has a user present to see and fix it; a corrupt/poisoned `launch.json` does not â€” the person
who'll eventually see the failure is a player double-clicking `play.exe`, and "the game
silently never starts, forever, until someone rebuilds it" is a strictly worse failure mode
than "the game starts with default window size/title instead of whatever `launch.json` asked
for."

**Not part of the `roves-action` sync:** pure Rust-side behavior change to an existing
internal failure path, not a `mach bundle` CLI surface change â€” nothing in `action.yml`/
`roves-action`'s README describes this.

**Patch:** `patches/servo-v0.4.0/0032-dont-exit-on-broken-bundled-launch-args.patch`

**Verification:** applied cleanly on top of a pristine `v0.4.0` extraction with patches
`0001`â€“`0031` already applied, producing a result byte-identical to the actual working tree.
The change was pushed to trigger `.github/workflows/test.yml` for real (this repo is
`DRincs-Productions/roves`, a genuine top-level GitHub repo with Actions enabled â€” not the
dormant in-parent-project state `CLAUDE.md`'s workflow-location note describes), and the
resulting Windows portable + `--msi`-mode bundles were downloaded from the rolling `test`
release and actually run, exactly the way the previous entry's crash was first confirmed.

**Outcome:** confirmed fixed. The fresh Windows portable and `--msi`-mode bundles built by that
CI run (same leaked `launch.json` as ever â€” `--package-name`, `--package-version` still
present) were downloaded and actually launched: `roves.log` shows the parse failure, the
logged retry, then a full successful boot (`parsed command line arguments` â†’ `created event
loop` â†’ `creating platform window` â†’ `built Servo instance` â†’ the page's own content
rendering), and the window stays open and responsive. Both `play.exe` copies (plain portable
and the `--msi`-staged one) behave identically.

---

## 2026-08-14 â€” Fixing patch `0027` itself: a later, unrelated commit had silently truncated it

**Not a source change â€” a `patches/` integrity bug, caught while verifying the entry above.**
While re-deriving the exact byte-for-byte state `patches/servo-v0.4.0/0027-add-msi-dmg-
installer-support.patch` should produce (the same single-file pristine-extraction-plus-patch-
chain method every entry in this file already uses to verify a new patch), the chain came up
short: the real working tree's `post_build_commands.py` has `_wrap_windows_msi`,
`_wrap_macos_dmg`, `_sanitize_msi_version`, and the `--deb-package-name`/`--deb-version` â†’
`--package-name`/`--package-version` rename â€” none of which `0027`'s patch file, as
committed, actually contains. `git log` on that one patch file found two commits touching it:
`b5d2283` ("Add Windows installer template for roves-bundle using WiX" â€” the commit that
actually introduced this feature) and, much later, `9ad5b8d` ("Refactor boot splash screen:
resize, recenter, and redesign progress bar" â€” a commit with no business touching the msi/dmg
installer at all). `git diff b5d2283 9ad5b8d -- patches/servo-v0.4.0/0027-*.patch` confirms
it: `9ad5b8d` shrank the patch from 437 lines to 160, losing everything except a small later
`output_dir = path.abspath(...)` correction (referenced in the entry above this one). The
actual source (`post_build_commands.py` itself) was never affected â€” only the patch file
meant to reproduce it, almost certainly regenerated at the time with `git diff` against the
wrong base (e.g. the previous commit instead of the pre-`0027` state) and overwriting the
correct file instead of replacing just that one small hunk. This is exactly the silent-drift
failure mode `CLAUDE.md`'s "keep patches up to date" section warns about, and exactly why:
`.github/workflows/test.yml` downloads a pristine tag and applies every patch fresh on every
run â€” a truncated `0027` would have quietly built a bundle *missing* `--msi`/`--dmg` support
entirely the next time this project's Servo version gets bumped and this patch needs
reapplying, with nothing else here to catch it in the meantime (the working tree itself looks
completely correct; only the patch â€” the thing that matters for the *next* upgrade â€” was
wrong).

**Fix:** restored `patches/servo-v0.4.0/0027-add-msi-dmg-installer-support.patch` from
`b5d2283`'s (correct, complete) version, then re-applied the later `path.abspath` correction
on top by hand (a one-line change, easy to redo safely) and regenerated the patch from an
actual before/after diff rather than editing the unified-diff text directly. Net effect: same
437-ish lines as originally committed, plus the abspath fix folded in as part of the same
patch instead of silently replacing it.

**Verification:** rebuilt the full chain from a pristine `v0.4.0` extraction of just
`python/servo/post_build_commands.py` â€” `0004`, `0013`, `0014`, `0015`, `0016`, `0017`,
`0023`, `0026`, this corrected `0027`, `0031`, `0033` (the diagnostic-script entry below) â€” in
order, and the result is now byte-identical to the real working tree. Before the fix, the same
process reproducibly diverged (missing the msi/dmg methods entirely); after it, it matches.

---

## 2026-08-14 â€” CI actually launches the bundle it just built, instead of only building it

**File:** `.github/workflows/test.yml`. No upstream location â€” this workflow doesn't exist
upstream at all (see `CLAUDE.md`), so there's no `patches/` entry for it; it's edited directly
in this repo like any other Roves-only file.

**Why:** every job in this workflow, until now, only ever confirmed `mach bundle` *succeeds*.
That gap is exactly how the `--package-name`/`--package-version` launch-args leak (see the two
entries above) went unnoticed through *multiple* green runs of this same workflow: every real
double-click of the resulting `play.exe` crashed instantly, while CI stayed green throughout,
because nothing here had ever actually run the binary. A build succeeding and a build
launching are different claims, and only the first one was being tested.

**Change:** two new steps, inserted between "assemble test bundle" and the zip steps, run
*after* every matrix entry's bundle is assembled (portable, `--msi`, `--dmg`, `--deb` alike â€”
whichever binary the earlier "add-msi-dmg" bug above just confirmed still ships loose inside
`release/` for every mode, not only portable):

- **Linux/macOS** (one bash step, `if: matrix.os_name != 'windows'`): installs `xvfb` (Linux
  only â€” macOS runners already have a real window server even without a physical display),
  launches `release/play` or `release/Roves.app/Contents/MacOS/Roves` in the background,
  waits 10 seconds, and checks the process is *still running* â€” the same "did a window
  survive past argument parsing and GL/window setup" signal a human tester would look for.
  Captures stdout/stderr to files and prints them regardless of outcome, then searches
  `~/.cache` (or `~/Library/Caches` on macOS) for the newest `roves.log` and prints that too.
  Fails the job (`exit 1`) if the process exited on its own within the 10 seconds.
- **Windows** (one `pwsh` step): the same check via `Start-Process -PassThru` +
  `-RedirectStandardOutput`/`-RedirectStandardError`, searching `%LOCALAPPDATA%` for the
  newest `roves.log`.

Deliberately not a pixel-perfect check â€” no screenshot, no window-content assertion, just "is
the process still alive a few seconds in." That's intentional: it's exactly the granularity
needed to catch a crash-before-a-window-ever-appears (this bug's exact shape) without needing
a display-comparison harness, and it works identically whether or not the runner has a real
display attached.

**Not part of the `roves-action` sync:** CI-only tooling, not a `mach bundle` CLI surface
change.

**Verification:** `test.yml` is plain YAML + bash/pwsh, not part of the `patches/` mechanism
(see "File" above) â€” nothing to apply-check here. The change was pushed alongside the
diagnostic-script entry below and exercised for real by the resulting CI run; see that run's
outcome for whether the new steps themselves behave as intended (a smoke test that never
actually ran isn't verified by reading its own YAML).

**Correction (same day, after that real CI run):** the first version of this step failed on
4 of 6 matrix entries â€” worth recording in detail since each was a distinct bug, not one
underlying cause:

- **`set -e` was swallowing the diagnostics this step exists to produce.** GitHub Actions
  bash steps run with `errexit`. In the "already exited" branch, `wait "$PID"` returns that
  process's (non-zero, since it already exited) status as a bare statement â€” not inside an
  `if`/`while` condition or an `&&`/`||` chain â€” which is exactly the case bash's `-e` treats
  as fatal: the whole step aborted right there, before ever reaching the stdout/stderr dump,
  the `roves.log` search, or this step's own `::error::` message. Every failure showed up as
  a bare "Process completed with exit code N" with none of the diagnostics the step was
  written to produce â€” undermining the entire point of adding it. Fixed with an explicit
  `set +e` once the binary's existence is confirmed (kept `set -e` for the setup steps
  before that, where a hard stop on unexpected failure is still correct).
- **`--deb` doesn't produce a loose `release/play` at all.** Unlike `--msi`/`--dmg` (which
  wrap the same portable output), `_bundle_linux_deb` builds a real Debian package â€”
  `/usr/lib/<package_name>/`, `/usr/bin/<package_name>` symlink â€” that only exists once
  actually installed. The step assumed the portable layout unconditionally; fixed by
  `sudo dpkg -i release/*.deb` first, then testing `/usr/bin/servoshell-test` â€” the same
  binary a player actually installing the `.deb` would end up running.
- **`--msi`/`--dmg` *also* don't leave a loose binary in `release/`** â€” `bundle()` deletes
  `stage_dir` (where the portable `play.exe`/`Roves.app` was built) right after wrapping it,
  so `release/` ends up containing only the `.msi`/`.dmg` itself. This one is genuinely new
  behavior compared to every earlier CI run: those were unknowingly building from the
  truncated `0027` (see that fix entry above), which never actually defined `--msi`/`--dmg`/
  `--package-name` as recognized flags at all â€” meaning every prior "msi"/"dmg" CI job was
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

## 2026-08-14 â€” Optional `diagnose.bat`/`diagnose.sh` shipped alongside the bundle

**File:** `python/servo/post_build_commands.py` â€” new `_DIAGNOSE_BAT`/`_DIAGNOSE_SH` string
constants and `_write_diagnostic_script` function (both module-level, next to
`_write_launch_config`), a new `--diagnostic-script` flag on `bundle`, and one new call site
in `bundle()` itself, right after `_place_bundle_content`.

**Why:** the same "no console on a double-clicked Windows build" problem the file-logging
entry above exists for has a second half: even with `roves.log` now capturing everything, a
non-technical tester asked to "try launching it and tell me what happens" still has no way to
*see* that log, or the process's exit code, without being walked through finding
`%LOCALAPPDATA%` by hand. A script that launches the game from a console that stays open
afterward â€” printing the exit code and the log's contents inline â€” turns "nothing happened"
into something a tester can screenshot or copy-paste directly into a bug report.

**Change:** `bundle`'s new `--diagnostic-script` flag (off by default â€” a real shipped release
has no reason to carry engine-internal debug tooling players never asked for) writes
`diagnose.bat` (Windows) or `diagnose.sh` (macOS/Linux, `chmod +x`'d) into `stage_dir` â€” the
same directory `play.exe`/`play`/`Roves.app` itself sits in, and, critically, the directory
that `--msi`/`--dmg` wrap wholesale into their installer (see the "single-executable-bundle"
and "add-msi-dmg" entries) â€” so the script ships inside those installed outputs too, not just
the plain portable one. Deliberately **not** written for `--deb`: a `.deb` install runs from
`/usr/bin` via a normal terminal that already shows stdout/stderr directly, so the script would
have nothing to add there. The script itself: runs the game binary directly (not backgrounded,
not killed after a timeout â€” a real tester should be able to actually play/close it normally),
then prints the exit code, a "this looks like a launch failure" callout if it was non-zero,
and the contents of whatever `roves.log` is newest under the platform's cache root (found by
`ls -t`/`Get-ChildItem | Sort LastWriteTime`, not a hardcoded path â€” robust to the game's own
name, µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥çv†–6‚—2v†BF†BF—&V7F÷'’—2æÖVBgFW"’âVæG2v—F‚W6V…v–æF÷w2’6ğ¦F÷V&ÆRÖ6Æ–6¶–ærFöW6âwB–ç7FçFÇ’6Æ÷6RF†R7VÖÖ'’&Vf÷&Rç–öæR&VG2—Bà ¢¢¦æv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆ¢¢¢76VÖ&ÆRFW7B'VæFÆVw2%TäDÄUô$u6æ÷rÇv—2–æ6ÇVFW0¦ÒÖF–væ÷7F–2×67&—FÂ6òWfW'’4’Ö'V–ÇBFW7B'VæFÆRW†W&6—6W2F†—2F‚†æBF†RæWr6Öö¶RĞ§FW7BVçG'’&÷fR–æ6–FVçFÆÇ’&÷fW2F†R67&—B—G6VÆbvWG2w&—GFVâæB—2W†V7WF&ÆRÂ6–æ6P¦—B6—G2&–v‡BæW‡BFòF†R&–æ'’F†R6Öö¶RFW7BÆVæ6†W2’à ¢¢¦&÷fW2Ö7F–öæ7–æ3¢¢¢ÒÖF–væ÷7F–2×67&—F—2æWrÂ&VÂÖ6‚'VæFÆV4Ä’fÆr‡VæÆ–¶P§F†R4’ÖöæÇ’6Öö¶R×FW7BVçG'’&÷fR’Â6òW"4ÄTDRæÖFw2&¶VW&÷fW2Ö7F–öæ–â7–æ2 §6V7F–öâÂ7–æ6VB–âF†—26ÖRGW&â‡F†R6–&Æ–ær6†V6¶÷WBv2&W6VçB“¢ÖF6†–æp¦F–væ÷7F–2×67&—F–çWBFFVBFò7F–öâç–ÖÆ‡6ÖR·&÷fW5Ö×FvvVBÂFVfVÇC¢vfÇ6Rv §GFW&â2FV&ö×6–öFÖv’Âf÷'v&FVB–çFòF†RÖ6‚'VæFÆV–çfö6F–öâ&–v‡BgFW"F†P¦FV&ö×6–öFÖv&Æö6²ÂæBÖF6†–ær&÷rFFVBFò$TDÔRæÖFw2–çWB&VfW&Væ6R–âF†P§6ÖR÷6—F–öâà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó32Ö÷F–öæÂÖF–væ÷7F–2ÖÆVæ6‚×67&—BçF6†  ¢¢¥fW&–f–6F–öã¢¢¢7Bç'6VÖ6†V6¶VB÷7Eö'V–ÆEö6öÖÖæG2ç–w27–çF‚†6ÆVã²æğ¦Ö6†Ö6&ÆR—F†öâ–âF†—26æF&÷‚Â6ÖRv2V&Æ–W"VçG&–W2’ÂæBF†RF6‚v0§fW&–f–VBF†R6ÖRv’WfW'’÷F†W"öæR–âF†—2f–ÆR—3¢Æ–VB6ÆVæÇ’öâF÷öb&—7F–æP¦cãBãW‡G&7F–öâv—F‚F6†W2(	63Ç&VG’Æ–VB‡6VRF†R&FBÖ×6’ÖFÖr"f—€¦VçG'’&÷fRf÷"F†R6÷'&V7FVB#vF†—2FWVæG2öâ’Â&öGV6–ær&W7VÇB'—FRÖ–FVçF–6ÂFğ§F†R7GVÂv÷&¶–ærG&VRâôD”täõ4Uô$FöôD”täõ4Uõ4†w2Æ—FW&Â7G&–ær6öçFVçBv2W‡G&7FV@§f–7BæÆ—FW&ÅöWfÆæB†æBÖ–ç7V7FVBv–ç7BF†R–çFVæFVB&V†f–÷"â7GVÆÇ’w&÷FRæB&à¦F–væ÷6Ræ&Fv–ç7B&VÂF÷væÆöFVB'VæFÆR‡F†Rf—†VB÷'F&ÆRöæRg&öÒF†RVçG'’&÷fR“ ¦—G2÷vâÆöv–2(	B&ææW"ÂTÄô4ÄDDVÆör6V&6‚ÂW†—BÖ6öFR'&æ6‚Âf–æÂÖW76v–ær(	BÆÀ¦W†V7WFVB6÷'&V7FÇ’Â'WBF†R&&RÆ’æW†VÆ–æR–ç6–FR—Bf–ÆVBFòÆVæ6‚–âF†—27V6–f–0§6æF&÷‚†U%$õ$ÄUdTÂ“–Â&æ÷B&V6övæ—¦VB"’WfVâF†÷Vv‚F†RW†7B6ÖRÆ’æW†VÂ–âF†P¦W†7B6ÖRF—&V7F÷'’ÂÆVæ6†W2f–æRv†Vâ–çfö¶VBF—&V7FÇ’‡f–7F'BÕ&ö6W76ö&6¶w&÷VæFV@§6†VÆÂ6öÖÖæB’(	B&W&öGV6VB7&÷72F‡&VRF–ffW&VçB–çfö6F–öâÖWF†öG2†&r6ÖBö6Â¥÷vW%6†VÆÂ&6¶w&÷VæB¦ö"ÂæBÆ–â7F'BÕ&ö6W76f–ÆRÖ76ö6–F–öâÆVæ6‚Ö—'&÷&–ær§&VÂF÷V&ÆRÖ6Æ–6²’Â'VÆ–ær÷WBÖ—7F¶R–âç’öæRFW7B†&æW72âv—fVâ6BöB"WæG&F†Vâ¦&&R6–&Æ–æræW†V—2F†R6–ævÆRÖ÷7B7FæF&BÂVæ—fW'6ÆÇ’×7W÷'FVB&F6‚GFW&âF†W&R—2À§F†—2&VG226æF&÷‚×7V6–f–2&W7G&–7F–öâöâ7væ–ærâ&&—G&'’æÖVBW†V7WF&ÆRg&öÒ¦6ÖBæW†V6†–ÆB&ö6W727V6–f–6ÆÇ’†2÷÷6VBFòF—&V7FÇ’Ö–çfö¶VB7F'BÕ&ö6W76’Âæ÷B¦FVfV7B–âF†R67&—B(	B'WB7FFVBÆ–æÇ’&F†W"F†â6–ÆVçFÇ’77VÖVC¢¢¦æ÷B6öæf—&ÖV@§v÷&¶–ærVæB×FòÖVæBöâ&VÂÂVç&W7G&–7FVBv–æF÷w2Ö6†–æRâ¢¢æ÷BFöæS¢â7GVÀ¦Ö6‚'VæFÆRÒÖF–væ÷7F–2×67&—F'Vâ6öæf—&Ö–ærF–væ÷6Ræ&FöF–væ÷6Rç6†6†÷rW–â§&VÂ'VæFÆRæB&V†fR2w&—GFVâ(	BF†RæW‡B&VÂv–æF÷w2öÖ6†Ö6&ÆR'V–ÆB‡F†R6ÖR4§'VâFW7F–ærF†R6Öö¶R×FW7BVçG'’&÷fR’6†÷VÆB6öæf—&ÒF†—2à ¢¢¥WFFS¢¢¢F†R6Öö¶R×FW7BVçG'’&÷fRw2&VÂ4’'Vâ6öæf—&ÖVBF†—2v÷&·2(	Bv–æF÷w2÷'F&ÆP¦æBÒÖ×6–&÷F‚&âF–væ÷6Ræ&F7V66W76gVÆÇ’öæ6Rw&—GFVâÂæò6W&FRf—‚æVVFVBà ¢ÒÒĞ ¢22##bÓ‚ÓB(	BÖ4õ2÷'F&ÆR÷WGWB&VæÖVB&÷fW2æ(i"Æ’æÂæB&VÂ7FVÒÖG–Æ–"7&6‚—BW‡÷6V@ ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†ö'VæFÆUöÖ6÷6Â÷w&öÖ6÷5öFÖvÀ¦'VæFÆVw2Fö77G&–ær’Â7W÷'Bö6öçFVçB×6¶W"÷7&2öÖ–âç'6†öæR6öÖÖVçB’à ¢¢¥v‡’F†R&VæÖS¢¢¢&WVW7FVBF—&V7FÇ’(	B&÷fW2æ2F†R§÷'F&ÆR'VæFÆRw2÷vâf–ÆP¦æÖR¢&VB2âöFBÂ&&—G&'’6†ö–6RæW‡BFòv–æF÷w2ôÆ–çW‚w2æWWG&ÂÆ’æW†VöÆ–à¥v÷'F‚&V–ærW‡Æ–6—B&÷WBF†RFVç6–öâF†—27&VFW2Â7W&f6VBæB6öæf—&ÖVB&Vf÷&RÖ¶–æp§F†—26†ævS¢$TDÔRæÖFw2÷vâ$æÖ–ær"6V7F–öâFö7VÖVçG2Â2¦FVÆ–&W&FR¢FV6—6–öâg&öĞ§F†R##bÓ‚Ór6W'fş(i%&÷fW2&VæÖRÂF†BWfW'’Æ–W"ôõ2Öf6–ærÆ&VÂ(	Bv–æF÷rF—FÆRÀ§F6¶&"öFö6²–FVçF—G’ÂF†RÆ–çW‚æFW6·F÷VçG'’ÂæB‡VçF–Âæ÷r’F†RÖ4õ2æ'VæFÆP¦æÖR(	B6†÷VÆB6’%&÷fW2"â&VæÖ–ærF†R'VæFÆRFòÆ’ævÆ·2&6²F†BöæR–V6Rö`¦—BÂG&F–ær&ÖF6†W2v–æF÷r×F—FÆR÷F6¶&"'&æF–ær"f÷"&ÖF6†W2F†RæWWG&ÂÆ– §Æ6V†öÆFW"WfW'’÷F†W"ÆFf÷&ÒÇ&VG’W6W2â"6öæf—&ÖVBF†—2G&FRÖöfbW‡Æ–6—FÇ’&F†W §F†â77VÖ–ær—B(	BF†Rç7vW"v2Fò&ö6VVBv—F‚Æ’æç—v’â$TDÔRæÖFw2æÖ–æp§6V7F–öâ—2WFFVBFòFW67&–&RF†RæWrÂæ'&÷vW"66÷RöbF†B'6—2&÷fW2WfW'—v†W&R ¦6Æ–Ò‡v–æF÷rF—FÆR÷F6¶&"öæFW6·F÷Âæ÷BF†R÷'F&ÆR&–æ'’ö'VæFÆRæÖRöâç§ÆFf÷&Ò’à ¢¢¤6†ævS¢¢¢ö'VæFÆUöÖ6÷6¢'VæFÆRföÆFW"&÷fW2æ(i"Æ’æÂF†R&–æ'’–ç6–FP¦6öçFVçG2ôÖ4õ2õ&÷fW6(i"6öçFVçG2ôÖ4õ2÷Æ–ÂæB–æfòçÆ—7Fw0¦4d'VæFÆTW†V7WF&ÆVö4d'VæFÆTæÖV†×W7BÖF6‚F†R7GVÂf–ÆVæÖR’(i"'Æ’&à¦4d'VæFÆT–FVçF–f–W&VçF÷V6†VB(	B7F–ÆÂFVÆ–&W&FVÇ’÷&rç6W'fòç6W'f÷6†VÆÂæ'VæFÆVÂW"F†P¦W†—7F–ær6öÖÖVçB&÷fR—BW‡Æ–æ–ærv‡’F†BöæRæVVG2—G2÷vâ6W&FRFV6—6–öâà¦÷w&öÖ6÷5öFÖvw2Fö77G&–æræB'VæFÆVw2÷vâFö77G&–ærWFFVBFòÖF6‚âÇ6òf—†VC ¦æv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆÂ$TDÔRæÖFÂ&÷fW2Ö7F–öæw2$TDÔRæÖFö7F–öâç–ÖÆÂæ@¦&÷fW2×v–¶–w2Fö72vW2(	BWfW'’Æ6RFW67&–&–ærF†RÖ4õ2÷WGWB'’æÖRà ¢¢¤&VÂ'VrF†—2&VæÖRw2÷vâfW&–f–6F–öâf÷VæC¢¢¢&RÖFW&—f–ærF†RF6‚f÷"F†—26†ævP¦ÖVçB7GVÆÇ’'Vææ–ærF†R&VÂ4’Ö'V–ÇBÖ4õ2'VæFÆW2†&÷F‚÷'F&ÆVæBFÖv’VæBFğ¦VæBf÷"F†Rf—'7BF–ÖR‡F†R6Öö¶R×FW7BVçG'’&÷fRöæÇ’7F'FVBFö–ærF†—2F†R6ÖRF’’(	@¦æB&÷F‚7&6†VB–ç7FçFÇ“  ¦FW‡@¦G–ÆE³ƒƒ5Ó¢Æ–'&'’æ÷BÆöFVC¢ÆöFW%÷F‚öÆ–'7FVÕö’æG–Æ– ¢&VfW&Væ6VBg&öÓ¢âââ÷&VÆV6Rõ&÷fW2æô6öçFVçG2ôÖ4õ2õ&÷fW0¢&V6öã¢G&–VC¢râââ÷&VÆV6Rõ&÷fW2æô6öçFVçG2ôÖ4õ2öÆ–'7FVÕö’æG–Æ–"r†æò7V6‚f–ÆR¦  ¦÷'G2÷6W'f÷6†VÆÂö'V–ÆBç'6Æ–æ·2WfW'’Ö4õ2G–Æ–"Fò&Rf÷VæBf–¦×'F‚W†V7WF&ÆU÷F‚öÆ–"öÂæBö'VæFÆUöÖ6÷666÷&F–ævÇ’6÷–W2WfW'’æG–Æ–&—@¦f–æG2–çFòÆ–"ö7V&F—&V7F÷'’æW‡BFòF†R&–æ'’(	B6÷'&V7Bf÷"6W'fòw2÷vâFWVæFVæ6–W2À¦'WB7FV×v÷&·2×7—6Æ–æ·2Æ–'7FVÕö’æG–Æ–&v—F‚†&F6öFV@¦ÆöFW%÷F‚öÆ–'7FVÕö’æG–Æ–&–ç7FÆÂæÖR–ç7FVB…fÇfRw2÷vâ4D²6öçfVçF–öâÂæ÷@§6öÖWF†–ærF†—2f÷&²w2'V–ÆB67&—B6öçG&öÇ2’ÂæBÆöFW%÷F†f÷"F†RÖ–âW†V7WF&ÆP¦ÖVç2&fÆBÂF—&V7FÇ’æW‡BFòF†R&–æ'’"(	Bæ÷BÆ–"öâWfW'’Ö4õ2ÒÖfVGW&W27FVÖ ¦'V–ÆB†2&VVâ'&ö¶VâF†—2v’6–æ6RF†RfVGW&Rv2FFVC²æ÷F†–ær†BWfW"7GVÆÇ¦ÆVæ6†VBöæR&Vf÷&Ræ÷r‡6VRF†R6Öö¶R×FW7BVçG'’&÷fRf÷"v‡’F†BvW†—7FVBBÆÂ’à ¢¢¤f—ƒ¢¢¢ö'VæFÆUöÖ6÷6æ÷r7V6–ÂÖ66W2Æ–'7FVÕö’æG–Æ–&(	B6÷–VBfÆB–çFğ¦6öçFVçG2ôÖ4õ2öÆöæw6–FRF†R&–æ'’Â&VÖ÷fVBg&öÒF†RÆ—7BF†BvöW2–çFòÆ–"öâWfW'¦÷F†W"G–Æ–"—2VæffV7FVBà ¢¢¤æ÷B'BöbF†R&÷fW2Ö7F–öæ7–æ3¢¢¢æV—F†W"6†ævRffV7G2Ö6‚'V–ÆFöÖ6‚'VæFÆVw0¤4Ä’7W&f6R†æòfÆrFFVB÷&VÖ÷fVB÷&VæÖVB’(	B7F–öâç–ÖÆõ$TDÔRÇ&VG’§W7B6¦Æ’ævVæW&–6ÆÇ’Væ÷Vv‚æ÷BFòæVVBWFF–ærf÷"F†R&VæÖRÂæBF†RG–Æ–"f—‚—0§W&VÇ’–çFW&æÂFòv†Bö'VæFÆUöÖ6÷66÷–W2v†W&Rà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó3B×&VæÖRÖÖ6÷2Ö'VæFÆR×Fò×Æ’ÖÖæBÖf—‚×7FVÒÖG–Æ–"çF6†  ¢¢¥fW&–f–6F–öã¢¢¢7Bç'6VÖ6†V6¶VB÷7Eö'V–ÆEö6öÖÖæG2ç–w27–çF‚(	B6ÆVââF†RF6€§v2fW&–f–VBF†R6ÖRv’2WfW'’÷F†W"öæR–âF†—2f–ÆS¢Æ–VB6ÆVæÇ’öâF÷öb§&—7F–æRcãBãW‡G&7F–öâv—F‚F6†W2(	636Ç&VG’Æ–VBÂ&öGV6–ær&W7VÇ@¦'—FRÖ–FVçF–6ÂFòF†R7GVÂv÷&¶–ærG&VRf÷"&÷F‚6†ævVBf–ÆW2âF†RÆ–'7FVÕö’f—€¦—G6VÆb—26öæf—&ÖVB'’F†Rf–ÇW&RF†—2VçG'’V÷FW2&÷fR‡F†RW†7B7&6‚—Bw2ÖVçBFğ¦f—‚’(	Bæ÷B–WB&RÖ6öæf—&ÖVBv÷&¶–ærv—F‚g&W6‚4’'VâBF†RF–ÖRöbw&—F–ærF†—2VçG'“°§F†B'Vâ—2v†Bv–ÆÂ7GVÆÇ’&÷fR—B†÷"æ÷B’à ¢¢¤÷WF6öÖS¢¢¢6öæf—&ÖVBf—†VBâF†RföÆÆ÷r×W4’'Vâ6ÖR&6²w&VVâöâÆÂbÖG&—‚¦ö'2(	@¦–æ6ÇVF–ær&÷F‚Ö4õ2öæW2Âv†–6‚—2F†R7GVÂ&ööc¢F†R6Öö¶R×FW7B7FW‡F†—2f–ÆRw2÷và¦VçG'’&÷fR’æ÷rvVçV–æVÇ’'Vç2ÒÖfVGW&W27FVÖÖ4õ2'V–ÆBf÷"F†Rf—'7BF–ÖRÂæB—@§7F–VBW–ç7FVBöb7&6†–æröâF†RÆ–'7FVÕö’æG–Æ–&ÆöBf–ÇW&RF†—2VçG'’FW67&–&W2à ¢ÒÒĞ ¢22##bÓ‚ÓB(	B&ö÷B7Æ6‚v26†÷v–ærW7G&VÒ6W'fòw2÷vâ–6öâÂæ÷B&÷fW2r(	B4’æWfW"7GVÆÇ’6÷–VB÷W'2–à ¢¢¤f–ÆW3¢¢¢æv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆ†öæRæWr7’Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6 ¢†WFFU÷7Æ6†ÂæBF†RFö26öÖÖVçB&÷fR5Ä4…õtõ$DÔ$µôdôåEõ4•¤V’à ¢¢¥&W÷'FVBF—&V7FÇ“¢¢¢&VÂÂg&W6†Ç’Ö'V–ÇB‡6ÖRÖF’’v–æF÷w2÷'F&ÆR'VæFÆRw2&ö÷B7Æ6€§6†÷vVB6ÖÆÂw&VVâ÷FVÂö&ÇVR6—&7VÆ"–6öâæW‡BFòF†R%&÷fW2"v÷&FÖ&²(	Bæ÷BF†P§vöÆbÖæBÖ6†–ç2Ö&²F†—2&ö¦V7Bw27GVÂ'&æF–ærW6W2WfW'—v†W&RVÇ6R‡F†Rv–¶’Â–6öâç7fv ¦BF†—2&Wòw2&ö÷B’âv÷'F‚vÆ¶–ærF‡&÷Vv‚†÷rF†—2v÷BG&6¶VBF÷vâÂ6–æ6RF†R7GVÀ¦6W6RGW&æVB÷WBFò&Ræ÷F†–ærÆ–¶RF†Rf—'7B‡—÷F†W6—3  ¢¢¤f—'7B‡—÷F†W6—2‡w&öær“¢F†R6öÖÖ—GFVB–6öâf–ÆR—G6VÆb—27FÆR÷w&öærâ¢¢&VF–æp¦&W6÷W&6W2÷6W'fõócBçæv‡v†BwV’ç'6w2ÆöE÷7Æ6…ö–6öåö–ÖvVVÖ&VG2f–¦–æ6ÇVFUö'—FW2’F—&V7FÇ’6†÷vVBv†BÆöö¶VBÂBvÆæ6RÂÆ–¶RF&²Â7–·’ÂVæfÖ–Æ– ¦7&VGW&R(	Bæ÷F†–ærÆ–¶RF†RvöÆbÖ6†–âÖ&²â6öæ6ÇVFVBg&öÒF†—2F†BF†Rf–ÆR—G6VÆb×W7@§7F–ÆÂ&R6öÖRÆVgF÷fW"W7G&VÒ6W'fò76WBÂFW7—FR##bÓ‚Ó2VçG'’&÷fR6Æ–Ö–ær—Bv0¦Ç&VG’&VvVæW&FVBg&öÒ–6öâç7fvæB—†VÂ×fW&–f–VBà ¢¢¤7GVÆÇ’6†V6¶–ærÂ&F†W"F†âG'W7F–ærV–6²Æöö³¢¢¢W66Æ–ær&W6÷W&6W2÷6W'fõócBçæv §v—F‚¦æV&W7BÖæV–v†&÷"¢†æò6Öö÷F†–ærÂ6òf–æRFWF–Â7W'f—fW2’6†÷vVB—B6ÆV&Ç’¦—2¢F†P§vöÆbÖ6†–âÖ&²(	BF†R'VæfÖ–Æ–"7&VGW&R"–×&W76–öâv2§W7B†÷r–ÆÆVv–&ÆR#’–æF—f–GVÀ§fV7F÷"F‡2rv÷'F‚öbf–æRFWF–Â‡FVWF‚ÂgW"Â6†–âÆ–æ·2’&V6öÖW2öæ6RfÆGFVæVBFòc@§—†VÇ2æBf–WvVB6ÖÆÂâ6ÖR6†V6²v–ç7B&W6÷W&6W2÷6W'fòæ–6öw2VÖ&VFFVB#Sg‚g&ÖS ¦Ç6ò6÷'&V7FÇ’F†RvöÆbÖ6†–âÖ&²âF†R##bÓ‚Ó2VçG'’w26Æ–Òv2&–v‡BgFW"ÆÂ(	BF†W6P¢¦6öÖÖ—GFVB¢f–ÆW2vW&RæWfW"F†R&ö&ÆVÒà ¢¢¥6òv‡’F–B&VÂ'V–ÆB6†÷r6öÖWF†–ærVÇ6RVçF—&VÇ“ò¢¢6¶VBF†RW6W"F—&V7FÇ’v†WF†W §F†R67&VVç6†÷B6ÖRg&öÒ'V–ÆBÖFRv—F‚7W'&VçB6öFR‡–W2’(	BÖVæ–ærF†RF—67&Wæ7’v0§&VÂÂæ÷B7FÆRÆö6Â7FFRÂæBF†R6öÖÖ—GFVBf–ÆW2&V–ær6÷'&V7BÖVçBF†R¦'V–ÆB&ö6W72 ¦†BFò&RÆöö¶–ær6öÖWv†W&RVÇ6Râæv—F‡V"÷v÷&¶fÆ÷w2÷FW7Bç–ÖÆw2&F÷væÆöB²F6‚6W'fğ§6÷W&6R"7FWF÷væÆöG2¢§&—7F–æR¢¢W7G&VÒ6W'fò¦—æBÆ–W2F†—2&Wòw2FW‡@§F6†W2öâF÷(	BæBÂ6ÖR2F†R&W6÷W&6W2öföçG2öÖWFÂÖæ–föçB&Vf÷&R—BÂ&–æ'¦76WG2Æ–¶R&W6÷W&6W2÷6W'fõócBçævö6W'fòæ–6ö6âwB&R6'&–VB'’FW‡BF6‚BÆÂà¥VæÆ–¶R&W6÷W&6W2öföçG2öÂv†–6‚¦FöW2¢vWBâW‡Æ–6—B7–âF†B7FWÂæö&öG’†BWfW ¦FFVBF†RWV—fÆVçB6÷’f÷"F†R–6öâ&7FW'2â6†V6¶VBv†Bf–ÆÇ2F†Bv¢W‡G&7FV@¦&W6÷W&6W2÷6W'fõócBçævg&öÒF†R7GVÂ&—7F–æRcãBã¦—F†—2v÷&¶fÆ÷rF÷væÆöG2(	@§W7G&VÒ6W'fò6†—2—G2÷vâf–ÆRBF†BW†7B6ÖRF‚‡Vç7W'&—6–æs²F†—2&Wòw2f–ÆP¦æÖ–ærv2æWfW"6†ævVBg&öÒ6W'fòw2÷vâ6öçfVçF–öâ’(	BæB—B—2Â'—FRf÷"'—FRÂF†RW†7@¦w&VVâ÷FVÂö&ÇVR6—&7VÆ"Ö&²g&öÒF†R67&VVç6†÷Bâ–æ6ÇVFUö'—FW2ö'V–ÆBç'6w2–6öà¦VÖ&VFF–ær†–Ç’6ö×–ÆVBv–ç7B§W7G&VÒw2¢f–ÆRF†Rv†öÆRF–ÖRÂ6–ÆVçFÇ’&¶–ær–âF†P§w&öær–6öâ–ç7FVBöbf–Æ–ærFò'V–ÆBBÆÂ(	BW†7FÇ’F†R¶–æBöb6–ÆVçBF—fW&vVæ6R&WGvVVà§F†R6öÖÖ—GFVBG&VRæBv†B4’7GVÆÇ’&V6öç7G'V7G2F†BF†R#v×F6‚×G'Væ6F–öâVçG'¦&÷fRÇ&VG’7W&f6VBöæ6RF†—26ÖRF’à ¢¢¤f—ƒ¢¢¢öæRÆ–æRFFVB&–v‡BgFW"F†RW†—7F–ær&W6÷W&6W2öföçG2ö6÷’–âFW7Bç–ÖÆ ¦7ââ÷&W6÷W&6W2÷6W'fõócBçærââ÷&W6÷W&6W2÷6W'fõó#Bçærââ÷&W6÷W&6W2÷6W'fòæ–6ğ¢ââ÷&W6÷W&6W2÷6W'fòæ–6ç2&W6÷W&6W2öâöbF†W6RÂöæÇ’6W'fõócBçæv†&ö÷B7Æ6‚’æ@¦6W'fòæ–6ö…v–æF÷w2æW†V–6öâÂf–'V–ÆBç'6’&R7GVÆÇ’&VfW&Væ6VB'’ç’6öFRF€§FöF’(	B6W'fõó#BçævæB6W'fòæ–6ç6&VâwBv—&VBWç—v†W&R–WB‡F†RÖ4õ2æw0¦–æfòçÆ—7F†2æò4d'VæFÆT–6öäf–ÆVBÆÂÂ6W&FRÂ&RÖW†—7F–ærvæ÷BFG&W76V@¦†W&R’Â'WB6÷––ærÆÂf÷W"æ÷rÖVç2v†–6†WfW"öbF†VÒ¦FöW2¢vWBv—&VBWÆFW"vöâw@§6–ÆVçFÇ’†—BF†—2W†7B6ÖR'Vrv–âà ¢¢¥6W&FVÇ’Â6—¦–æs¢¢¢Ç6ò6¶VBFòÖ¶RF†R–6öâæBv÷&FÖ&²6Æ÷6W"FòF†R6ÖRf—7VÀ§6—¦RæBfW'F–6ÆÇ’6VçFW&VBv–ç7BV6‚÷F†W"âF†RöÆB6öFR†&F6öFV@¦5Ä4…ô”4ôåõ4•¤RÒ#‚ãv–ç7B5Ä4…õtõ$DÔ$µôdôåEõ4•¤RÒƒ‚ã(	B#ƒ£ƒ‚(˜ƒãc’§&F–ò6'&–VB÷fW"g&öÒ&W6÷W&6W2÷&÷fW5÷v÷&FÖ&²ç7fvw2÷vâ–6öã¦föçB×6—¦RÆö6·WÂv†–6€¦FöW6âwBæV6W76&–Ç’†öÆBf÷"†÷rFÆÂ%&÷fW2"7GVÆÇ’&VæFW'2–âF†RÖWFÂÖæ–föçB¦@§F†—27V6–f–26—¦R¢Â6–æ6RföçBVÒ×6—¦RæB&VæFW&VBvÇ—‚†V–v‡B&VâwBF†R6ÖRF†–ærà¦WFFU÷7Æ6†æ÷rÖV7W&W2F†Rv÷&FÖ&²w27GVÂ&VæFW&VB6—¦Röæ6R†VwV“£¤6öçFW‡C£ ¦föçG5ö×WFw2Æ–÷WEöæõ÷w&‚âââ’ç6—¦R‚–ÂF†R6ÖRFV6†æ—VRF†RW†—7F–ærv–GF‚ÖV7W&VÖVç@¦Ç&VG’W6VBÂ§W7BÇ6ò&VF–ærç–æ÷r’æB6—¦W2F†R–6öâFòÖF6‚F†BÖV7W&VB†V–v‡@¦F—&V7FÇ’Â–ç7FVBöbG'W7F–ærâ–æFWVæFVçFÇ’ÖwVW76VB6öç7FçB(	B&VÖ÷f–æp¦5Ä4…ô”4ôåõ4•¤VVçF—&VÇ’âF†R†ÆbÖ†V–v‡BfW'F–6ÂÖ6VçFW&–æröfg6WB‡&Wf–÷W6Ç’¦†&F6öFVBƒrãgVFvRf7F÷"F–VBFòF†RöÆB#‡‚77V×F–öâ’—2æ÷r6ö×WFVBg&öÒF†R6ÖP¦ÖV7W&VÖVçBFöòÂ6ò—B7F—26÷'&V7B&Vv&FÆW72öbW†7FÇ’†÷rFÆÂF†Rv÷&FÖ&²&VæFW'2à ¢¢¤æ÷B'BöbF†R&÷fW2Ö7F–öæ7–æ3¢¢¢æV—F†W"6†ævRF÷V6†W2Ö6‚'V–ÆFöÖ6‚'VæFÆVw0¤4Ä’7W&f6Rà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¢†wV’ç'6öæÇ’(	BF†RFW7Bç–ÖÆ6÷’×7FWf—‚—6âwB'BöbF†RF6†W2öÖV6†æ—6ÒÂ6ÖP§&V6öæ–ær2WfW'’÷F†W"FW7Bç–ÖÆÖöæÇ’VçG'’–âF†—2f–ÆR’à ¢¢¥fW&–f–6F–öã¢¢¢'W7Ff×BÒÖVF—F–öâ##BÒÖ6†V6¶öâwV’ç'6(	B6ÆVâW†6WBöæR&RÖW†—7F–ærÀ§Vç&VÆFVBF–fbBÆ–æRSB†6öæf—&ÖVB&RÖW†—7F–ær'’6†V6¶–ærF†RVæÖöF–f–VB„TFfW'6–öà¦öbF†Rf–ÆR&W÷'G2F†R–FVçF–6ÂF–fb’âF†RæWr6VÆbæ6öçFW‡BæVwV•ö7G‚æföçG5ö×WB‚âââ–6ÆÀ¢†ÖFR&Vf÷&R6VÆbæ6öçFW‡Bç'Vâ‚âââ–Â&F†W"F†âf–F†R6Æ÷7W&Rw27G†&wVÖVçB2F†P¦W†—7F–ærv–GF‚ÖV7W&VÖVçBF–BÂ6–æ6R6VÆbæ6öçFW‡Bç'VæÇ&VG’†öÆG26VÆbæ6öçFW‡F ¦×WF&Ç’’Ö—'&÷'2â–FVçF–6ÂGFW&âÇ&VG’W6VBVÇ6Wv†W&R–âF†—26ÖRf–ÆP¢†6VÆbæ6öçFW‡BæVwV•ö7G‚æÖVÖ÷'•ö×WB‚âââ–ÂÇ6ò6ÆÆVB÷WG6–FRç'Vâ‚–’(	Bæ÷Bæ÷fVÂÀ§VçfW&–f–VB’W6vRâF†RF6‚v2fW&–f–VBF†R6ÖRv’2WfW'’÷F†W"öæR–âF†—2f–ÆS ¦Æ–VB6ÆVæÇ’öâF÷öb&—7F–æRcãBãW‡G&7F–öâv—F‚F6†W2(	63Ç&VG¦Æ–VBÂ&öGV6–ær&W7VÇB'—FRÖ–FVçF–6ÂFòF†R7GVÂv÷&¶–ærG&VRâæ÷BFöæS¢â7GVÀ¦6&vò6†V6¶öÖ6‚'V–ÆF‡F†—26æF&÷‚†2æò'V–ÆF&ÆRÆö6ÂFööÆ6†–âf÷"F†RgVÆÀ¦6W'f÷6†VÆÆFWVæFVæ7’w&‚–â&V6öæ&ÆRF–ÖR’÷"&VÂ67&VVç6†÷BöbF†R&V'V–ÇB7Æ6€¦6öæf—&Ö–ærF†R–6öâæ÷r6†÷w26÷'&V7FÇ’æB&VG22&÷÷'F–öæFR(	BF†RæW‡B&VÀ¥v–æF÷w2öÖ6†Ö6&ÆR4’'Vâ—2v†Bv–ÆÂ7GVÆÇ’6öæf—&Ò&÷F‚f—†W2à ¢¢¤6÷'&V7F–öâ‡6ÖRF’ÂgFW"F†B4’'Vâ“¢F†R6—¦–ærf—‚2f—'7Bw&—GFVâ7&6†VBWfW'§ÆFf÷&ÒöâÆVæ6‚â¢¢RöbbÖG&—‚¦ö'2†WfW'—F†–ærW†6WBÆ–çW‚ÖFV&Âv†–6‚†Vç2æ÷BFğ¦W†W&6—6RF†—26öFRF‚F†R6ÖRv’’f–ÆVBF†R6Öö¶R×FW7BVçG'’&÷fRw2÷vâ6†V6²Âv—F€¦&÷fW2æÆöv÷7FFW'"6†÷v–æs  ¦FW‡@¤æòföçG2f–Æ&ÆRVçF–Âf—'7B6ÆÂFò6öçFW‡C£§'Vâ‚’‡F‡&VBÖ–âÂBâââöVwV’Óã3Bã2÷7&2ö6öçFW‡Bç'3£2¦  ®(	BâVwV––çFW&æÂæ–2…4”u4Tubö&÷'BF÷vç7G&VÒöb—BÂW†—B6öFR3’’âF†RÖV7W&VÖVç@§F†—2VçG'’Ö÷fVBFò6VÆbæ6öçFW‡BæVwV•ö7G‚æföçG5ö×WB‚âââ–Â6ÆÆVB¦&Vf÷&R ¦6VÆbæ6öçFW‡Bç'Vâ‚âââ–ÂGW&ç2÷WBFò&RW†7FÇ’F†RöæRF†–ærVwV“£¤6öçFW‡C£¦föçG5ö×WF ¦FöW6âwBÆÆ÷s¢föçG2&VâwB–æ—F–Æ—¦VBVçF–ÂF†R¦f—'7B¢'Vâ‚–6ÆÂÂWfW"Âf÷"F†@¦6öçFW‡B(	BÖV7W&–ærç—F†–ærföçB×&VÆFVB†2Fò†Vâ¦–ç6–FR¢F†R6Æ÷7W&R76VBFğ¦'Vâ‚–Âæ÷B&Vf÷&R—BâF†R÷&–v–æÂ6öFRÇ&VG’¶æWrF†—2†—G2÷vâv–GF‚ÖV7W&VÖVçBv0¦Çv—2–ç6–FRF†R6Æ÷7W&R“²Ö÷f–ær—B÷WBFòFöFvRF†R6VÆfF÷V&ÆRÖ&÷'&÷r†6öç7G'V7F–æp¦–6öæÂv†–6‚æVVG2g6VÆbç7Æ6…ö–6öå÷FW‡GW&VÂv†–ÆR6VÆbæ6öçFW‡Bç'Vâ‚âââ–Ç&VG’†öÆG0¦6VÆbæ6öçFW‡F×WF&Ç’’v2F†Rw&öærv’Fò&W6öÇfRF†B&÷'&÷rÖ6†V6¶W"&ö&ÆVÒà ¢¢¤f—ƒ¢¢¢&W6öÇfVBF†R¦7GVÂ¢&÷'&÷r6öæfÆ–7B–ç7FVB(	B6VÆbç7Æ6…ö–6öå÷FW‡GW&Ræ6ÆöæR‚– ¢†6†V²FW‡GW&T†æFÆV—26ÖÆÂ&VbÖ6÷VçFVB†æFÆRÂ6öæf—&ÖVBf–—G27GVÂFö72Âæ÷@¦77VÖVB’–çFòÆ–âÆö6Âf&–&ÆR¦&Vf÷&R¢6VÆbæ6öçFW‡Bç'Vâ‚âââ–Âv†–6‚F†R6Æ÷7W&R6à¦g&VVÇ’6GW&Rv—F†÷WBF÷V6†–ær6VÆfBÆÂâ&÷F‚F†Rv÷&FÖ&²ÖV7W&VÖVçBæB–6öæw0¦6öç7G'V7F–öâÖ÷fVB&6²–ç6–FRF†R6Æ÷7W&RÂW6–ærF†B6ÆöæVBÆö6Â–ç7FVBöb6VÆf ¦F—&V7FÇ’âæWB&W7VÇC¢6ÖRÖV7W&RÖFöâwBÖwVW726—¦–ærF†—2VçG'’÷&–v–æÆÇ’6WB÷WBFòFBÀ¦§W7Bv—F‚F†RÖV7W&VÖVçB†æBWfW'—F†–ærFWVæF–æröâ—B’†Væ–ærBF†RöæÇ’ö–ç@¦VwV–7GVÆÇ’ÆÆ÷w2—Bà ¢¢¥F6ƒ¢¢¢&VvVæW&FVBF6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¦–âÆ6R†g&öÒ&—7F–æRcãBã²(	63Âæ÷B26V6öæBF6‚7F6¶VBöâF†P¦'&ö¶VâfW'6–öâ(	BF†R'&ö¶VâfW'6–öâæWfW"6†÷VÆB†fR6†—VB2Ö—2Â6òF†W&Rw2æò&V6öâFğ§&W6W'fR—B26W&FR&Wf–Wv&ÆR7FW’â&R×fW&–f–VBF†R6ÖRv“¢Æ–W26ÆVæÇ’À¦'—FRÖ–FVçF–6Â&W7VÇBâ7F–ÆÂæ÷BFöæRÂ6ÖRv2&÷fS¢â7GVÂÖ6‚'V–ÆF÷&VÂÆVæ6€®(	B'WBF†—2F–ÖR&6¶VB'’6öæ7&WFRÂ7V6–f–2VwV’Ö–çFW&æÇ2&V6öâF†R&Wf–÷W2&æ÷B–W@¦6öæf—&ÖVB"fW'6–öâv27GVÆÇ’w&öærÂæ÷B§W7BâVçfW&–f–VBwVW72&WVFVBGv–6Rà ¢¢¤÷WF6öÖS¢¢¢6öæf—&ÖVBf—†VB(	BF†—24’'Vâ6ÖR&6²w&VVâöâÆÂbÖG&—‚¦ö'2ÂæB‡W"F†P¦VçG'’&÷fRw2÷vâ&æ÷BFöæR"v’&VÂ67&VVç6†÷Bg&öÒÆ—fRv–æF÷w2÷'F&ÆR'V–Æ@¦6öæf—&ÖVBF†R–6öâæ÷r6†÷w2F†R7GVÂvöÆbÖæBÖ6†–ç2Ö&²Âæ÷BW7G&VÒ6W'fòw2à ¢¢¤föÆÆ÷r×WÂ6ÖRF’‡&W÷'FVBF—&V7FÇ’v–ç7BF†B67&VVç6†÷B“¢F†R–6öâ7F–ÆÂ&V@§6ÖÆÆW"F†âF†Rv÷&FÖ&²Â6—¦RÖÖF6†–ærf—‚æ÷Gv—F‡7FæF–ærâ¢¢ÖV7W&–æp¦&W6÷W&6W2÷6W'fõócBçævw2÷vâ6öçFVçB‡&VæFW&–ær–6öâç7fvg&W6‚æBG&–ÖÖ–ærFò—G0¦æöâ×G&ç7&VçB&÷VæF–ær&÷‚ÂF†R6ÖR6†'FööÆ–ærW6VBV&Æ–W"–âF†—2f–ÆRFğ§&VvVæW&FRF†W6R76WG2’W‡Æ–ç2v‡“¢F†R'Gv÷&²—2v–FR÷fÂ&FvRF†BöæÇ’f–ÆÇ0¦&÷WB¢£s‚R¢¢öb—G2÷vâ7V&R6çf2w2†V–v‡BÂF†R&W7B&V–ærG&ç7&VçBF÷ö&÷GFöĞ§FF–ær†6öæf—&ÖVBB&÷F‚#G‚æBF†R7GVÂ6†—VBcG‚Â6öç6—7FVçFÇ’’â6—¦–ærF†P¢¦–ÖvR¢‡FFVB6çf2–æ6ÇVFVB’FòÖF6‚F†Rv÷&FÖ&²w2ÖV7W&VB†V–v‡BÂ2F†R&Wf–÷W0§fW'6–öâöbF†—2VçG'’F–BÂv2F†W&Vf÷&RÇv—2vö–ærFòVæFW'6—¦RF†R§f—6–&ÆR&FvR¢'§F†B6ÖRã#"R(	BF†Rf—‚v÷&¶VBW†7FÇ’2ÖV7W&VBÂF†RÖV7W&VÖVçB§W7Bv6âwB66÷VçF–æp¦f÷"FF–ær&¶VB–çFòF†R76WB—G6VÆbà ¢¢¤f—ƒ¢¢¢æWr6öç7FçB5Ä4…ô”4ôåô4ôåDTåEô„T”t…Eõ$D”òÒãsƒFÂFö7VÖVçFVBv—F‚v†W&RF†P¦çVÖ&W"6ÖRg&öÒæBv‡’—Bw27Æ6‚ÖöæÇ’6÷'&V7F–öâ&F†W"F†â&RÖ7&÷öbF†R6†&V@¦–6öâ76WG2‡F†÷6RÇ6ò6W'fR2F†Rv–æF÷w2æW†V÷F6¶&"–6öâf–'V–ÆBç'6Âæ@¦WfVçGVÆÇ’F†RÖ4õ2æ–6öâ(	B&÷F‚6öçFW‡G2v†W&R7V&RÂ6VçFW&VBFF–ær—2F†P¢¦6÷'&V7B¢Æöö²Âæ÷B'Vr’âWFFU÷7Æ6†æ÷rF—f–FW2F†RÖV7W&VBv÷&FÖ&²†V–v‡B'’F†—0§&F–òFòvWBF†R–6öâw27GVÂF&vWB†V–v‡B†–6öå÷6—¦V’Â6òF†Rf—6–&ÆR&FvR(	Bæ÷B—G0§FF–ær(	BVæG2WÖF6†–ærF†Rv÷&FÖ&²âWfW'’÷F†W"6Æ7VÆF–öâF†BW6VBFòG&V@¦v÷&FÖ&µ÷6—¦Rç–27FæBÖ–âf÷"'F†R–6öâw2†V–v‡B"‡F†RÆö6·Ww2F÷FÂv–GF‚ÂF†P§fW'F–6ÂÖ6VçFW&–ær†ÆbÖ†V–v‡Böfg6WB’æ÷rW6W2–6öå÷6—¦V–ç7FVBÂ6–æ6RF†RGvò&Ræğ¦ÆöævW"WVÂ'’FW6–vâà ¢¢¥F6ƒ¢¢¢&VvVæW&FVBF6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¦–âÆ6Rv–âÂ6ÖR&V6öæ–ær2F†R&Wf–÷W26÷'&V7F–öâ–âF†—2VçG'’(	BöæR6ö†W&Vç@¢&&ö÷B7Æ6‚–6öâ6—¦–ær"6†ævRÂæ÷B7F6²öbF6†W2Fö7VÖVçF–ærWfW'’–çFW&ÖVF–FP¦Ö—77FWâ&R×fW&–f–VBF†R6ÖRv“¢Æ–W26ÆVæÇ’öâ(	63Â'—FRÖ–FVçF–6Â&W7VÇBà ¢¢¥fW&–f–6F–öã¢¢¢'W7Ff×BÒÖVF—F–öâ##BÒÖ6†V6¶6ÆVâ‡6ÖRöæR&RÖW†—7F–ærÂVç&VÆFV@¦Æ–æRÓSBF–fb2&Vf÷&R’âæ÷BFöæS¢â7GVÂ&V'V–ÇB67&VVç6†÷B6öæf—&Ö–ærF†RæWr&F–ğ§&VG226÷'&V7FÇ’×6—¦VB&F†W"F†â÷fW"Ò÷"VæFW"Ö6÷'&V7FVB(	BãsƒF6ÖRg&öÒÖV7W&–æp§F†R7GVÂ6†—VB76WBÂæ÷BwVW72Â'WB&FöW2—BÆöö²&–v‡B"—2VÇF–ÖFVÇ’f—7VÀ¦§VFvÖVçBF†RæW‡B&VÂ'V–ÆBw267&VVç6†÷B6†÷VÆB6öæf—&Òà ¢¢¤÷WF6öÖS¢¢¢4’6ÖR&6²w&VVâÂæBF†—2F–ÖR&VÂ67&VVç6†÷Bg&öÒF†B'V–ÆB§v2 ¦6†V6¶VB(	BF†R6öçFVçB×FF–ær6ö×Vç6F–öâv26÷'&V7B2f"2—BvVçBÂ'WBF†R–6öâ7F–ÆÀ§&VB2Föò6ÖÆÂæW‡BFòF†Rv÷&FÖ&²âæ÷BÖV7W&VÖVçB'VrF†—2F–ÖS¢&W÷'FVBF—&V7FÇ¦2FVÆ–&W&FR6—¦–ær&VfW&Væ6RÂæ÷BF–VBFòF†RFF–ærÖF‚&÷fRà ¢¢¤föÆÆ÷r×WÂ6ÖRF“¢Ö¶RF†R–6öâF—7F–æ7FÇ’&–vvW"Âæ÷B§W7B†V–v‡BÖÖF6†VBâ¢¢æWp¦5Ä4…ô”4ôåõ44ÄRÒ"ã6öç7FçBÂÆ–VBöâF÷öb†æ÷B–ç7FVBöb¦5Ä4…ô”4ôåô4ôåDTåEô„T”t…Eõ$D”ö(	BWFFU÷7Æ6†w2–6öå÷6—¦V—2æ÷p¦v÷&FÖ&µ÷6—¦Rç’ò5Ä4…ô”4ôåô4ôåDTåEô„T”t…Eõ$D”ò¢5Ä4…ô”4ôåõ44ÄVâ¶WB2Gvğ§6W&FR6öç7FçG2FVÆ–&W&FVÇ“¢öæR—2ÖV7W&VB6÷'&V7F–öâf÷"F†R76WBw2÷và§G&ç7&VçBFF–ærÂF†R÷F†W"—2Æ–âFW6–vâ&VfW&Væ6R‡F†R–6öâ6†÷VÆBf—7VÆÇ¦FöÖ–æFRÂæ÷B§W7BÖF6‚ÂF†Rv÷&FÖ&²’(	B6öÆÆ6–ærF†VÒ–çFòöæRçVÖ&W"v÷VÆBÆ÷6Rv†–6€§'B—2&FW&—fVBg&öÒF†R7GVÂ76WB"fW'7W2'6öÖV&öG’w2W7F†WF–26ÆÂÂ"v†–6‚ÖGFW'2–`¦V—F†W"öæRæVVG2&Wf—6—F–ær–æFWVæFVçFÇ’ÆFW"†Rærâ–bF†R–6öâ76WB—G6VÆb6†ævW2À¦öæÇ’F†R&F–ò6öç7FçB6†÷VÆBæVVBWFF–ær’âWfW'’F÷vç7G&VÒ6Æ7VÆF–öâÇ&VG¦6öç7VÖVB–6öå÷6—¦V&F†W"F†â&RÖFW&—f–ær–6öâ†V–v‡B–æÆ–æR‡6VRF†R&Wf–÷W2VçG'’w0¦÷vâ&Vf7F÷"f÷"W†7FÇ’F†—2&V6öâ’Â6òF†—2v2öæRÖÆ–æR6†ævRBF†R6ö×WFF–öà¦—G6VÆbÂæ÷F†–ærVÇ6R–âWFFU÷7Æ6†æVVFVBF÷V6†–ærà ¢¢¥F6ƒ¢¢¢&VvVæW&FVBF6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¦–âÆ6Rv–â(	B6ÖR&öæR6ö†W&VçB–6öâ×6—¦–ær6†ævR"&V6öæ–ær2&÷F‚&–÷"6÷'&V7F–öç0¦–âF†—2VçG'’â&R×fW&–f–VBF†R6ÖRv“¢Æ–W26ÆVæÇ’öâ(	63Â'—FRÖ–FVçF–6À§&W7VÇBà ¢¢¥fW&–f–6F–öã¢¢¢'W7Ff×BÒÖVF—F–öâ##BÒÖ6†V6¶6ÆVâ‡6ÖR&RÖW†—7F–ærÂVç&VÆFV@¦Æ–æRÓSBF–fb’âæ÷BFöæS¢æ÷F†W"&VÂ67&VVç6†÷B6öæf—&Ö–ær"ã—2F†R&–v‡B×VÇF—Æ–W §&F†W"F†ââ÷fW"Ò÷"VæFW"×6†ö÷B(	BF†—2—27V&¦V7F—fR6—¦–ær&VfW&Væ6RÂæ÷B6öÖWF†–æp¦ÖV7W&VÖVçB6â6WGFÆRÂ6òF†RæW‡B'V–ÆB—2v†B7GVÆÇ’6öæf—&×2—Bà ¢¢¤÷WF6öÖRÂæBF†R7GVÂ&ö÷B6W6RöbWfW'’6—¦–ærGFV×B–âF†—2VçG'’6òf#¢¢¢&VÀ§67&VVç6†÷Bg&öÒF†B'V–ÆB6†÷vVBF†R–6öâ6ö×ÆWFVÇ’Væ6†ævVB–â6—¦RÂ¦æB¢F†P§v÷&FÖ&²6†–gFVBæ÷F–6V&Ç’ÆVgBöbv†W&R—BW6VBFò6—B(	B&Vw&W76–öâÂæ÷B§W7B'7F–ÆÂFöğ§6ÖÆÂâ"VwV“£¤–ÖvS£¦Ö…ö†V–v‡B‚–(	BW6VB'’WfW'’fW'6–öâöbF†—2f—‚6òf"(	BöæÇ’WfW ¦62Ö†–×VÓ²6öæf—&ÖVBF—&V7FÇ’v–ç7BVwV’w2÷vâFö72F†B—BFöW2æ÷B66ÆRâ–ÖvP§W7B—G2FVfVÇBöæF—fR6—¦Rv†VâF†BFVfVÇB—2Ç&VG’6ÖÆÆW"â5Ä4…ô”4ôåõ44ÄV ¦w&÷v–ær–6öå÷6—¦VF†W&Vf÷&R†B¦W&òVffV7BöâF†R7GVÆÇ’×&VæFW&VB–6öâÂv†–ÆRF†P¦Æ–÷WBÖF‚F÷vç7G&VÒ†Æö6·Wv–GF‚Â†÷&—¦öçFÂ6VçFW&–ær’¦F–B¢W6RF†Rw&÷vâ–6öå÷6—¦V §fÇVR(	B6†–gF–ærF†Rv÷&FÖ&²Fò6ö×Vç6FRf÷"6—¦R6†ævRF†Bv2æWfW"7GVÆÇ§f—6–&ÆRÂv†–6‚—2W†7FÇ’F†RÆVgGv&BÖ—6Æ–væÖVçB&W÷'FVBâWfW'’V&Æ–W"&ÖV7W&RÂFöâw@¦wVW72"6—¦–ær6†ævR–âF†—2VçG'’v26ö×WF–ærF†R&–v‡BçVÖ&W"æBF†Vâ†æF–ær—BFòà¤’F†B6–ÆVçFÇ’–væ÷&VB—Bv†VæWfW"F†BçVÖ&W"v2¦–æ7&V6R¢÷fW"F†RFW‡GW&Rw0¦FVfVÇB6—¦Rà ¢¢¤f—ƒ¢¢¢7v—F6†VBg&öÒæÖ…ö†V–v‡B†–6öå÷6—¦R–Fğ¦æf—E÷FõöW†7E÷6—¦R†VwV“£¥fV3#£§7ÆB†–6öå÷6—¦R’–Âv†–6‚7GVÆÇ’f÷&6W2F†R&VæFW&VB6—¦P¢†6öæf—&ÖVBv–ç7BVwV’w2Fö73¢f—E÷FõöW†7E÷6—¦V&f÷&6W2F†R–ÖvRFòö67W’7V6–f–0§6—¦RÂ"VæÆ–¶RF†RÖ…ò×&Vf—†VBÖWF†öG2Âv†–6‚öæÇ’6’âfV3#£§7ÆF†WVÂv–GF‚æ@¦†V–v‡B’—26÷'&V7B†W&R7V6–f–6ÆÇ’&V6W6R&W6÷W&6W2÷6W'fõócBçævw26çf2—27V&R(	@§F†—2v÷VÆBæVVBFò66÷VçBf÷"7V7B&F–ò–bF†BWfW"6†ævW2âF†—2öæR6†ævRf—†W2&÷F€¦6ö×Æ–çG2Böæ6S¢F†R–6öâ7GVÆÇ’w&÷w2Fò–6öå÷6—¦Væ÷rÂæBF†RÆ–÷WBÖF‚†Ç&VG¦6ö×WF–ærF†R&–v‡BÆö6·W÷v–GF†ö6VçFW&–ærW6–ær–6öå÷6—¦V’f–æÆÇ’ÖF6†W2v†Bw0¦7GVÆÇ’&VæFW&VBà ¢¢¥F6ƒ¢¢¢&VvVæW&FVBF6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¦–âÆ6Rv–ââ&R×fW&–f–VBF†R6ÖRv“¢Æ–W26ÆVæÇ’öâ(	63Â'—FRÖ–FVçF–6À§&W7VÇBà ¢¢¥fW&–f–6F–öã¢¢¢'W7Ff×BÒÖVF—F–öâ##BÒÖ6†V6¶6ÆVâ‡6ÖR&RÖW†—7F–ærÂVç&VÆFV@¦Æ–æRÓSBF–fb’âæ÷BFöæS¢&VÂ67&VVç6†÷B6öæf—&Ö–ærF†R–6öâæ÷r7GVÆÇ’&VæFW'2@£'‚F†Rv÷&FÖ&²w26öçFVçBÖ6ö×Vç6FVB†V–v‡BæBF†Rv÷&FÖ&²—2&6²Fò6VçFW"(	BF†—2—0§F†Rf÷W'F‚—FW&F–öâöbF†—26ÖRVçG'’FòÖ¶RF†B6Æ–ÒÂ6òG&VB&æ÷B–WB67&VVç6†÷GFVB ¦2F†R÷W&F—fR6fVBVçF–ÂöæR7GVÆÇ’ÆæG2à ¢¢¤÷WF6öÖR(	BF†Rf–gF‚æBÂW"â7GVÂ67&VVç6†÷BF†—2F–ÖRÂ6÷'&V7BöæS¢¢¢4’6ÖR&6°¦w&VVâÂæB&VÂ67&VVç6†÷Bg&öÒF†B'V–ÆB6öæf—&ÖVB&÷F‚f—†W2Böæ6S¢F†RvöÆbÖæBĞ¦6†–ç2–6öâ&VæFW&–ær6÷'&V7FÇ’‡F†—2VçG'’w2÷&–v–æÂ'Vr’Â&÷Vv†Ç’ÖF6†–ærF†P§v÷&FÖ&²w2†V–v‡BæB&÷W&Ç’6VçFW&VBv–ç7B—B†f—E÷FõöW†7E÷6—¦V7GVÆÇ’F¶–æp¦VffV7BÂVæÆ–¶RWfW'’Ö…ö†V–v‡FÖ&6VBGFV×B&Vf÷&R—B’âGvò&Vf–æVÖVçG2föÆÆ÷vVBÀ§&W÷'FVBF—&V7FÇ’v–ç7BF†B6ÖR67&VVç6†÷C  ¢Ò¢¥Föò&–râ¢¢v—F‚f—E÷FõöW†7E÷6—¦Vf–æÆÇ’Ö¶–ær5Ä4…ô”4ôåõ44ÄVf—6–&ÆRf÷"F†P¢f—'7BF–ÖRÂ"ã†6†÷6Vâ&6²v†Vâ—B†Bæòf—6–&ÆRVffV7BBÆÂ’&VB2FöòÆ&vRà¢†ÇfVBFòãà¢Ò¢¥—†VÆFVBâ¢¢&W6÷W&6W2÷6W'fõócBçæv(	BF†R6ÖRcL9scB76WB'V–ÆBç'6VÖ&VG22F†P¢v–æF÷w2æW†V–6öâ(	B—2f–æRB—G2æF—fR6ÖÆÂ6—¦RÂ'WBF†R7Æ6‚æ÷rF—7Æ—2—@¢66ÆVBvVÆÂ7BcG‚‡Fò&÷Vv†Ç’ÖF6‚F†Rv÷&FÖ&²w2†V–v‡BÂF–ÖW0¢5Ä4…ô”4ôåõ44ÄV’ÂæBW66Æ–ærcG‚6÷W&6RF†Bf"—2W†7FÇ’v†B&öGV6VBF†P¢f—6–&ÆR—†VÆF–öââ7v—F6†VBÆöE÷7Æ6…ö–6öåö–ÖvVö7Æ6…ö–6öå÷FW‡GW&VFòVÖ&V@¢&W6÷W&6W2÷6W'fõó#Bçæv–ç7FVB(	BÇ&VG’&VÂ76WB–âF†—2&Wò‡6VRF†RV&Æ–W ¢&&ö÷B7Æ6‚7F–ÆÂ6†÷w26W'fòw2–6öâ"VçG'’Âv†–6‚Ç&VG’FFVB—BFòFW7Bç–ÖÆw0¢–6öâÖ6÷’7FWf÷"Vç&VÆFVB&V6öç2Â6òæò4’6†ævRæVVFVB†W&R’(	BF÷vç66Æ–ær¢#G‚6÷W&6RFòv†FWfW"F†R7Æ6‚7GVÆÇ’æVVG27F—27&—7Bç’6—¦RF†—27Æ6€¢v–ÆÂÆW6–&Ç’W6Rà ¢¢¥F6ƒ¢¢¢&VvVæW&FVBF6†W2÷6W'fò×cãBãó3RÖf—‚Ö&ö÷B×7Æ6‚Ö–6öâÖæB×6—¦RÖ–6öâ×FòÖÖF6‚×v÷&FÖ&²çF6† ¦–âÆ6Rv–ââ&R×fW&–f–VBF†R6ÖRv“¢Æ–W26ÆVæÇ’öâ(	63Â'—FRÖ–FVçF–6À§&W7VÇBà ¢¢¥fW&–f–6F–öã¢¢¢'W7Ff×BÒÖVF—F–öâ##BÒÖ6†V6¶6ÆVâ‡6ÖR&RÖW†—7F–ærÂVç&VÆFV@¦Æ–æRÓSBF–fb’âæ÷BFöæS¢g&W6‚67&VVç6†÷B6öæf—&Ö–ærãæBF†RÆ&vW"6÷W&6R&V@§&–v‡B(	B&V6öæ&ÆRFòW‡V7B6òÂv—fVâF†R&Wf–÷W267&VVç6†÷BÇ&VG’6öæf—&ÖVBF†P§VæFW&Ç––ær6—¦–ærÖV6†æ—6Òv÷&·26÷'&V7FÇ’B"ãócG‚Â'WBæ÷B–æFWVæFVçFÇ’6öæf—&ÖV@¦BF†W6RW†7BæWrfÇVW2–WBà ¢ÒÒĞ ¢22##bÓ‚ÓR(	BÖ4õ2'VæFÆRv2Ö—76–æru7G&VÖW"w2÷vâG–Æ–'2Â7&6†–ærWfW'’&VÂ†æöâÖGVÖ×’ÖÖVF–’ÆVæ6€ ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†ö'VæFÆUöÖ6÷6’à ¢¢¥v‡“¢¢¢f÷VæBv†–ÆR7WGF–ærF†Rf—'7B&VÂÂfW'6–öæVB&VÆV6R†cããÂ6VP¦æv—F‡V"÷v÷&¶fÆ÷w2÷&VÆV6Rç–ÖÆæB4ÄTDRæÖFw2$7WGF–ærfW'6–öæVB&VÆV6R"6V7F–öâ’(	@§F†Rf—'7BF–ÖRF†—2f÷&²w2÷vâ4’WfW"'V–ÇBÖ4õ2'VæFÆRv—F‚F†R§&VÂ¢u7G&VÖW"ÖVF–§7F6²–ç7FVBöbÒÖÖVF–×7F6²GVÖ×–†FW7Bç–ÖÆÇv—2W6W2GVÖ×–Â6òF†—26Æ72öb'Vp¦†Bæòv’Fò7W&f6RF†W&R’â&÷F‚F†R÷'F&ÆVæBFÖv¦ö'27&6†VBöâÆVæ6ƒ  ¦FW‡@¦G–ÆE³sS#EÓ¢Æ–'&'’æ÷BÆöFVC¢'F‚öÆ–&w7GÆ’ÓããæG–Æ– ¢&VfW&Væ6VBg&öÓ¢âââ÷&VÆV6R÷Æ’æô6öçFVçG2ôÖ4õ2÷Æ¢&V6öã¢G&–VC¢râââ÷&VÆV6R÷Æ’æô6öçFVçG2ôÖ4õ2öÆ–"öÆ–&w7GÆ’ÓããæG–Æ–"r†æò7V6‚f–ÆR’Âââà¦  ¤F–væ÷7F–27FWFFVBFò&VÆV6Rç–ÖÆ†÷FööÂÔÆöâÆ–ÂÇ6öà¦6öçFVçG2ôÖ4õ2öÆ–"ö’6öæf—&ÖVBÆ–Æ–æ·2Æ–&w7GÆ–öÆ–&w7Gf–FVööÆ–&w7F&6Vğ¦Æ–&w7G&VÖW&öWF2âF—&V7FÇ’f–'F†ÂæBF†B6öçFVçG2ôÖ4õ2öÆ–"öF–FâwBW†—7B–à§F†R'VæFÆRBÆÂà ¢¢¥&ö÷B6W6S¢¢¢Ö6‚'V–ÆFw2÷vâ÷7BÖ'V–ÆB7FW†'V–ÆEö6öÖÖæG2ç–w0¦'Vå÷÷7Eö'V–ÆE÷F6·6’Ç&VG’6÷–W2u7G&VÖW"w2G–Æ–'2öâÖ4õ2f–¦w7G&VÖW"ç–w26¶vUöw7G&VÖW%öG–Æ–'2†'V–ÇEö&–æ'’Â#Æ&–æ'•öF—#âöÆ–"ò"ÂF&vWB–(	@¦–çFòÆ–"ö§7V&F—&V7F÷'’¢öbF†R'V–ÆB÷WGWBÂæ÷BfÆBÆöæw6–FRF†R&–æ'’â'W@¦ö'VæFÆUöÖ6÷6‡F†R6öFRÖ6‚'VæFÆV7GVÆÇ’W6W2Fò76VÖ&ÆRÆ’æ’öæÇ’WfW ¦F–B¶bf÷"b–â÷2æÆ—7FF—"†&–æ'•öF—"’–bbæVæG7v—F‚‚"æG–Æ–""•Ö(	BfÆB66âö`¦&–æ'•öF—&—G6VÆbÂv†–6‚æWfW"Æöö·2öæRÆWfVÂF÷vâ–çFò&–æ'•öF—"öÆ–"öâ6òWfW'¤u7G&VÖW"Æ–'&'’6–ÆVçFÇ’æWfW"ÖFR—B–çFòF†Rf–æÂ'VæFÆRÂöâWfW'’Ö4õ2'V–ÆBv—F€§&VÂÖVF–Væ&ÆVBÂ6–æ6RF†RF’6¶vUöw7G&VÖW%öG–Æ–'67F'FVBæW7F–ær—G2÷WGWB–à¦Æ–"öâF†—2†Bæòv’Fò&R6Vv‡B&Vf÷&Ræ÷s¢Æ–âÖ6‚'V–ÆFöÖ6‚'VæFÆV7V66W70¦FöW6âwBÆVæ6‚ç—F†–ær‡F†R6ÖR6Æ72öbvF†R$4’7GVÆÇ’ÆVæ6†W2F†R'VæFÆR ¦VçG'’&÷fRFW67&–&W2’ÂæBFW7Bç–ÖÆw2÷vâ6Öö¶RFW7BÇv—2&âv—F‚ÒÖÖVF–×7F6°¦GVÖ×–Âv†–6‚æVVG2æöæRöbF†—2à ¢¢¤f—ƒ¢¢¢ö'VæFÆUöÖ6÷6æ÷rÇ6ò6÷–W2&–æ'•öF—"öÆ–"ö†–b—BW†—7G2(	BöæÇ’&W6Vç@§v†VâF†R&VÂu7G&VÖW"ÖVF–7F6²v2Væ&ÆVB’–çFò6öçFVçG2ôÖ4õ2öÆ–"öf–¦6‡WF–Âæ6÷—G&VR‚âââÂF—'5öW†—7Eöö³ÕG'VR–ÂÖW&v–ærv—F‚v†FWfW"F†R&RÖW†—7F–ærÆö÷6RĞ¦æG–Æ–&Æö÷Ç&VG’Æ6VBF†W&R†Rærâ7FVÒw2G–Æ–"†æFÆ–ærÂVæffV7FVB’âÇ6òFFV@¦W†—7Eöö³ÕG'VVFòF†BÆö÷w2÷vâ÷2æÖ¶VF—'66ÆÂÂ6–æ6R&÷F‚F‡26âæ÷rG'’Fğ¦7&VFRF†R6ÖRÆ–"öF—&V7F÷'’à ¢¢¤æ÷B'BöbF†R&÷fW2Ö7F–öæ7–æ3¢¢¢FöW6âwBF÷V6‚Ö6‚'V–ÆFöÖ6‚'VæFÆVw24Ä§7W&f6R†æòfÆrFFVB÷&VÖ÷fVB÷&VæÖVB’(	BW&VÇ’–çFW&æÂFòv†Bö'VæFÆUöÖ6÷66÷–W0§v†W&Rà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãó3bÖf—‚ÖÖ6÷2Ö'VæFÆRÖÖ—76–ærÖw7G&VÖW"ÖG–Æ–'2çF6†  ¢¢¥fW&–f–6F–öã¢¢¢Æ–VB6ÆVæÇ’öâF÷öb(	63V†F6‚×ÒÖG'’×'VæÂæğ§&V¦V;ZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æÚ±î¸Â¸­yêë¢°k¢G§¦*^ts).

**Outcome:** confirmed fixed. The retriggered `v0.1.0` release run came back green on all 3
jobs, including macOS â€” its smoke test now genuinely launches a real (non-dummy-media)
bundle and stays up, instead of crashing on the `libgstplay` failure this entry describes.
`roves_shell_macos.zip` published successfully alongside the Windows/Linux artifacts. The
one-off diagnostic step added to `release.yml` to gather the `otool`/`ls` evidence above has
been removed now that the fix is confirmed working.

## 2026-08-17 â€” Fix: `roves:clear_content_cache` pointed at loose bundle content on an uncompressed build, instead of reporting "not a packed-content launch"

**Files:** `ports/servoshell/desktop/app.rs`.

**Patch:** `patches/servo-v0.4.0/0037-fix-clear-content-cache-uncompressed-bundle.patch`

**Reported as:** a real Packmaster-generated release, bundled with content compression
turned off (`--content-compress=none`/Packmaster's own "Compressione" toggle unchecked),
clicking the diagnostic "Clear extraction cache" button (`test-page`'s `ClearCacheButton.tsx`,
via `@drincs/roves-api/cache`'s `clearContentCache()`) failed with `TypeError: Network
error: "<bundle>\game" is not a managed content cache directory` â€” not the friendlier "No
extraction cache to clear (not a packed-content launch)" message the 2026-08-11 entry's own
`roves.rs` `None` arm already exists to produce for exactly this case.

**Root cause:** the 2026-08-11 entry (`roves:clear_content_cache` command) computed
`content_cache_dir` in `finish_init` as `initial_file_path`'s parent directory â€” "the exact
same directory `FileProtocolHandler` resolves content into", per that entry's own words. That
equivalence holds for a *packed* launch (the parent of the extracted boot HTML file's path
genuinely is the managed cache directory `prepare_dest` created, marker file and all), but
not for an *uncompressed* bundled launch: there, `bundle_launch.rs`'s `resolve_bundled_launch_args`
takes the `"url"` branch (no `content_dir` in `launch.json`), so `initial_file_path`'s parent
is just the bundle's own loose content folder â€” real, on-disk game content Packmaster placed
there directly, never anything `extract::prepare_dest` wrote a `.roves-content-source` marker
into. `is_managed_cache_dir` correctly refuses to delete it (see 2026-08-11 entry), but the
resulting error is confusing: it reads like a filesystem/permissions problem, not "there's
nothing to clear, compression was off for this build."

**Fix:** added `App::packed_content_dest: Option<PathBuf>`, captured in `App::new` from
`pending_boot_extraction.as_ref().and_then(|opts| opts.dest.clone())` â€” i.e. `Some` only when
`bundle_launch.rs` actually resolved a packed-content launch (`content_dir` present in
`launch.json`), regardless of whether the boot extraction it describes ends up actually
running (a cache-hit skip inside `prepare_dest` doesn't change this: `pending_boot_extraction`
is `Some` any time `content_dir` was in `launch.json` at all, whether or not extraction turns
out to be needed). `finish_init` now passes `self.packed_content_dest.clone()` to
`RovesProtocolHandler::new` instead of re-deriving a directory from `initial_file_path`. An
uncompressed bundle now correctly gets `None` there, so `clear_content_cache` takes the
existing `None` arm in `roves.rs` and reports "No extraction cache to clear (not a
packed-content launch)" â€” accurate, and matches what that arm's own doc comment already
claimed happened for "a plain dev `--url` launch," which a `--content-compress=none` bundled
launch effectively also is from this command's point of view.

**Not part of the `roves-action` sync:** doesn't touch `mach build`/`mach bundle`'s CLI
surface (no flag added/removed/renamed) â€” purely internal to how an already-existing command
picks its target directory.

**Verification:** `cargo check -p servoshell` could not be run in this environment (no MSVC
linker/Visual Studio Build Tools available on this machine) â€” reviewed by hand instead;
`packed_content_dest`'s type and the `.and_then(|opts| opts.dest.clone())` call match
`extract::ExtractOptions`'s `dest: Option<PathBuf>` field exactly, and `Path`'s existing
import in `app.rs` is still used elsewhere (`load_userscripts`), so removing its one other use
site doesn't orphan the import. Needs a real `mach build`/`mach bundle` + launch, with
compression both on and off, to confirm end-to-end before considering this closed.

## 2026-08-18 â€” Fix: macOS `--features steam` build crashed while packaging GStreamer dylibs

**Files:** `python/servo/gstreamer.py`.

**Patch:** `patches/servo-v0.4.0/0038-fix-macos-steam-gstreamer-dylib-packaging-crash.patch`

**Reported as:** `release.yml`'s first-ever run building every platform twice (plain +
`--features steam`, see the 2026-08-17 "Every platform builds twice" entry) had 5 of 6 jobs
succeed â€” only `macos, steam` failed, at the `mach build (release)` step, after ~24 minutes
(i.e. well into the build, not an early config error). The published `v0.2.0` tag/release
were deleted per this file's own "delete-and-republish loop" procedure once this was
confirmed a real bug, not the CI upload flake from the same day's other entry.

**Root cause:** `package_gstreamer_dylibs` (called from `build_commands.py` whenever real
media + `darwin` are both true â€” i.e. every non-`--media-stack dummy` macOS build)
walks every non-system dependency line `otool -L` reports on the built binary, resolving
each one to a real path and copying it. It only knows how to resolve two shapes: an absolute
path, or an `@rpath/...` line (via `make_rpath_path_absolute`). `steamworks-sys` links
`libsteam_api.dylib` with a hardcoded `@loader_path/libsteam_api.dylib` install name instead
of `@rpath/...` (Valve's own SDK convention â€” already noted in the 2026-08-14 "macOS portable
output renamed" entry, which special-cased this exact string at the *bundle* step). Nothing
before now had ever exercised real GStreamer (`media-gstreamer`) and `--features steam`
*together* on macOS: `test.yml`'s own steam build always uses `--media-stack dummy`, so
`package_gstreamer_dylibs` never runs there at all, and `release.yml` never built with
`--features steam` before this same day's "Every platform builds twice" entry. First real
combination, first time this path got exercised â€” `make_rpath_path_absolute` returned the
`@loader_path/...` string unresolved (its own early-return for anything not starting with
`@rpath/`), and the code then tried to run `otool -L` and `shutil.copyfile` directly against
that literal, non-existent path:
```
error: otool-classic: can't open file: @loader_path/libsteam_api.dylib (No such file or directory)
ERROR: could not package required dylibs: [Errno 2] No such file or directory: '@loader_path/libsteam_api.dylib'
```

**Fix:** new `is_separately_packaged_dylib()` in `gstreamer.py`, checked alongside the
existing system-library filter in `find_non_system_dependencies_with_otool` â€” skips
`libsteam_api.dylib` by filename before it ever enters the dependency-walking set. This is
the correct fix, not a workaround: that dylib is already placed correctly by two other,
pre-existing mechanisms (`build.rs`'s `copy_steam_lib` at build time, `post_build_commands.py`'s
`_bundle_macos` at bundle time), so this generic GStreamer-dependency walker had no business
trying to resolve/copy it itself in the first place â€” it just happened to never have been
asked to before.

**Verification:** could not run `mach build` locally in this environment (no Python/MSVC
toolchain available on this machine at all â€” see the 2026-08-17 `app.rs` entry's own
verification note for the same limitation). Reviewed by hand: `os.path.basename(...) ==
"libsteam_api.dylib"` correctly matches the exact dependency line `otool -L` reports
(confirmed against the real failing log's own error text, which names that exact file), and
the new check sits in the same boolean chain as the existing `is_macos_system_library`/
`librustc-stable_rt` filters, so it's applied identically at both call sites of
`find_non_system_dependencies_with_otool` (the top-level binary scan and the transitive
per-dependency scan inside the walking loop). Needs a real re-run of `release.yml`'s
`macos, steam` job to confirm fixed end-to-end before re-tagging a release.

## 2026-08-18 â€” Windows portable output: move GStreamer plugin DLLs into a `lib/` subfolder

**Files:** `components/servo/servo.rs`, `python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0039-windows-move-gstreamer-plugins-into-lib-subfolder.patch`

**Reported as:** a real Packmaster-generated Windows release (`roves-packmaster/release/
servo-test-page/`) had ~103 files sitting flat in the game's root folder â€” almost all of them
DLLs a player has no reason to ever see. The only two files that actually matter to a player
browsing that folder are `play.exe` and `diagnose.bat`; everything else is implementation
detail that belonged in a subfolder from the start.

**Root cause:** `copy_windows_dlls_to_build_directory` (`build_commands.py`, at `mach build`
time) already copies every GStreamer DLL flat into `target/release/` â€” both the ~35 *plugin*
element libraries (`windows_plugins()`, e.g. `gstplayback.dll`, `gstlibav.dll`) and their own
private codec/runtime dependencies (`windows_dlls()`'s `GSTREAMER_WIN_DEPENDENCY_LIBS` â€”
ffmpeg's `avcodec-59.dll` and friends, OpenSSL, glib, ...). `_bundle_windows` then blindly
copied every `.dll` it found there straight into the bundle root, preserving that flatness
into every published release.

**Why this couldn't just move everything into a subfolder naively:** Windows' *implicit*
DLL search (resolving `play.exe`'s own load-time dependencies, before any of our code runs)
only ever checks `play.exe`'s own directory, system dirs, and `PATH` â€” never a subfolder,
and there's no supported way to redirect that for a statically/implicitly linked dependency
short of a delay-load trick or a separate launcher stub. The GStreamer *plugin* files are
different: they're never linked by `play.exe` at all -- `components/servo/servo.rs`'s
`media_platform::init` (Windows/macOS branch) loads each one by explicit path via
`gstreamer::Plugin::load_file` (see `components/media/backends/gstreamer/lib.rs`'s
`init_with_plugins`), which on Windows uses `LOAD_WITH_ALTERED_SEARCH_PATH` â€” a search order
that checks *the plugin file's own directory* first (and does not fall back to `play.exe`'s
directory at all). That's exactly the mechanism macOS's own `plugin_dir.push("lib")` (a few
lines above, already existing) already relies on; Windows just wasn't using it.

**Fix:**

- `servo.rs`: Windows now pushes `"lib"` onto `plugin_dir` too, exactly like macOS already
  did â€” one boolean condition change (`cfg!(any(target_os = "macos", windows))`).
- `post_build_commands.py`'s `_bundle_windows`: every DLL in `windows_plugins()` (the plugin
  files themselves) now goes *only* into a new `output_dir/lib/`. Every DLL in `windows_dlls()`
  (GStreamer's own core shared libs, e.g. `gstreamer-1.0-0.dll`, plus the plugins' private
  codec/runtime deps) goes into *both* `output_dir` (flat, since some of those core libs really
  are real load-time dependencies of `play.exe` itself) *and* `lib/` (since
  `LOAD_WITH_ALTERED_SEARCH_PATH` needs a plugin's own dependencies sitting right next to it,
  not next to `play.exe`) â€” duplicating a handful of small DLLs is far cheaper than guessing
  wrong about which ones `play.exe` needs flat. Anything not in either list (unexpected/future
  DLLs this list doesn't know about) keeps its old flat-only placement, so nothing regresses
  for a file this change doesn't recognize. Net result: the bundle root drops from ~103 files
  to `play.exe`, `diagnose.bat`, `launch.json`, `manifest.json`, the packed `.pack` content
  archives, and a `lib/` folder â€” everything a player would actually care about, front and
  center.
- The existing `--msi` WiX template (`support/windows/roves-bundle.wxs.mako`) needed no
  changes: its harvesting is already fully recursive over whatever subfolders `stage_dir`
  happens to contain, so `lib/` gets picked up automatically.
- macOS and Linux are untouched by this entry. macOS already had this exact `lib/` split
  (this fix is a direct port of that existing pattern to Windows). Linux's own flat `.so` dump
  next to `play` has the same surface-level appearance but a different, not-yet-fully-
  understood plugin-loading mechanism (Linux's `media_platform::init` never calls
  `init_with_plugins` at all â€” see the `#[cfg(not(any(windows, target_os = "macos")))]` branch
  a few lines below in `servo.rs` â€” so it isn't clear yet whether GStreamer's default registry
  scan on Linux even uses these bundled `.so` files, or falls back to a system-wide install).
  Left alone deliberately rather than guessed at; worth its own dedicated investigation later.

**Not part of the `roves-action` sync:** doesn't touch `mach build`/`mach bundle`'s CLI
surface (no flag added/removed/renamed, no new bundle output format) â€” the portable output's
*internal* folder layout changed, but `mach bundle --output <dir>` still produces exactly one
output folder per platform the same way it always has, which is all `roves-action`'s own
`action.yml`/README ever describe.

**Verification:** could not run `mach build`/`mach bundle` locally in this environment (see
this file's own recurring note on missing MSVC/Python toolchain access here). Instead:
confirmed against a real Packmaster-generated release folder's actual file listing (the exact
~103 files this entry describes) that every plugin name in `windows_plugins()` and every
dependency name in `windows_dlls()` is present and accounted for in that real listing, with
nothing left over unclassified; confirmed `windows_dlls()`/`windows_plugins()` are the same
already-authoritative lists `build_commands.py` uses to decide what to copy into
`target/release/` in the first place (not a new, independently-guessed heuristic); confirmed
via `git log -p`/reading `components/media/backends/gstreamer/lib.rs` that `Plugin::load_file`
is the actual runtime loading mechanism (not GStreamer's default registry scan) for the
Windows/macOS branch; and dry-ran `patch -p1` for the new patch file against a from-scratch
pristine v0.4.0 checkout with patches `0001`â€“`0038` already applied in order, confirming it
applies with zero fuzz. Still needs a real CI build + a real launch with sound/video to
confirm every moved plugin actually loads correctly from `lib/` before this is considered
fully closed.

## 2026-08-18 â€” Fix: `mach bundle --bin` on Windows missed plugins when re-bundling an already-bundled shell

**Files:** `python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0040-windows-bundle-preexisting-lib-when-rebundling-a-prebuilt-shell.patch`

**Reported as:** found while designing `roves-action`'s new `use-prebuilt-shell` mode (see
that repo's own `CLAUDE.md`), which downloads a previously-published `roves_shell_<platform>
.zip` and runs `mach bundle --bin <extracted play.exe>` against it to add game content â€”
without ever running `mach build` itself. Not yet exercised by a real CI run at the time of
this entry (that feature isn't merged in `roves-action` yet); caught by re-reading this same
day's earlier `_bundle_windows` entry with this exact usage in mind, not by a failure report.

**Root cause:** the 2026-08-18 "move GStreamer plugin DLLs into a `lib/` subfolder" entry
just above changed `_bundle_windows` to scan `binary_dir` for flat `.dll` files and sort them
into `output_dir` and/or `output_dir/lib/`. That's correct when `binary_dir` is a fresh
`target/release/` build (still everything flat, per `build_commands.py`'s own
`copy_windows_dlls_to_build_directory`) â€” but `--bin`/`--nightly` can instead point
`servo_binary` at a binary living in an *already-bundled* directory (exactly what
`roves-action`'s new mode does), where `binary_dir` already has its own `lib/` from a
*previous* `mach bundle` run. The flat-only scan never looks one level down, so every plugin
sitting in that pre-existing `lib/` would silently go missing from the new bundle.

**Fix:** after the existing flat-DLL scan, also check for `binary_dir/lib/` and copy it
wholesale into the new `lib/` if present (`shutil.copytree(..., dirs_exist_ok=True)`) â€” the
exact same pattern `_bundle_macos`'s own `gstreamer_lib_dir` handling (a few lines below,
pre-existing, added by the 2026-08-15 "macOS bundle was missing GStreamer's own dylibs"
entry) already uses for the identical situation on macOS. `_bundle_macos` needed no changes
at all here â€” it already handled this correctly, which is what made the gap on the Windows
side, added only hours earlier in the same day, easy to spot by direct comparison.

**Not part of the `roves-action` sync:** same reasoning as the entry above â€” no `mach build`/
`mach bundle` CLI surface changed.

**Verification:** could not run `mach build`/`mach bundle` locally (see this file's recurring
note on missing toolchain access here). Dry-ran `patch -p1` against a from-scratch pristine
v0.4.0 checkout with patches `0001`â€“`0039` already applied in order, confirming it applies
with zero fuzz. Needs a real end-to-end run of `roves-action`'s `use-prebuilt-shell` mode (or
any other real `--bin`-against-an-already-bundled-shell usage) to confirm the copied `lib/`
plugins actually load correctly before this is considered fully closed.

## 2026-08-19 â€” Fix: `mach.bat` broke on this exact checkout's own path (a space in it)

**Files:** `mach.bat`.

**Patch:** `patches/servo-v0.4.0/0041-fix-mach-bat-quoting-for-paths-with-spaces.patch`

**Reported as:** hit directly while trying to run a real `mach build --release` on Windows
from this checkout, at `C:\Users\<user>\3D Objects\roves` â€” a space in "3D Objects" is enough
to trigger this, and that's this actual machine's real folder name, not a contrived
reproduction. Likely to bite any Windows user whose checkout lives under a path with a space
anywhere in it (a shared "OneDrive - Company Name" sync folder, "Program Files", a username
with a space, etc.) â€” not specific to this one path.

**Root cause:** `uv run --frozen python %workdir%mach %*` expands `%workdir%` unquoted. cmd.exe
splits unquoted variable expansions on whitespace before handing arguments to the child
process, so a space anywhere in the checkout path splits `%workdir%mach` into two separate
argv entries at that space â€” `python` then tries to open the first fragment as the script
file and fails with `can't open file 'C:\\Users\\<user>\\3D': [Errno 2] No such file or
directory`, never reaching `mach` itself. `mach.ps1` has no such bug â€” PowerShell keeps
`(Join-Path $workdir "mach")`'s result as a single argument regardless of embedded spaces
when passed to a native command, so this is `mach.bat`-specific.

**Fix:** quote the expanded path: `uv run --frozen python "%workdir%mach" %*`.

**Verification:** confirmed the failure reproduces before the fix (`can't open file
'C:\\Users\\...\\3D'`) and disappears after it, on this exact checkout path, by actually
running `mach.bat` before and after â€” not just a patch dry-run. (`mach.bat --help` then hits
an unrelated, pre-existing `argparse` error â€” `ValueError: action 'store_true' is not valid
for positional arguments` â€” identically on `mach.ps1` too, confirming that part is a separate,
already-existing issue unaffected by this fix, not something this change introduced.) Patch
also dry-run-applies cleanly against a from-scratch pristine v0.4.0 checkout with patches
`0001`â€“`0040` already applied in order.

**Not part of the `roves-action`/`roves-packmaster` sync:** no `mach build`/`mach bundle` CLI surface
changed â€” this only fixes `mach.bat` even being invocable from a path with a space in it.

## 2026-08-19 â€” Windows portable output: attempted to shrink the root further, reverted

**Files:** `python/servo/post_build_commands.py` (comment only â€” `gstreamer.py` ends up
unchanged, see below).

**Patch:** `patches/servo-v0.4.0/0042-windows-document-why-gstreamer-dll-duplication-is-required.patch`

**Reported as:** the 2026-08-18 "move GStreamer plugin DLLs into a `lib/` subfolder" entry
got the bundle root from ~103 files down to ~58 by moving out every *plugin* DLL â€” but it
kept duplicating all 48 `windows_dlls()` entries (GStreamer's own core shared libs plus their
private codec/runtime deps) into *both* `output_dir` and `lib/`, reasoning that "some of
those really are load-time dependencies of `play.exe` itself" without pinning down exactly
which ones. Asked to actually narrow that down instead of guessing conservatively.

**What was tried:** curated a 20-entry subset of `windows_dlls()` believed to be `play.exe`'s
own real load-time dependencies via `dumpbin /dependents` on a real, published `play.exe`
(`v0.2.0`'s `roves_shell_windows.zip`), recursively, until the closure stopped growing â€” the
same method `GSTREAMER_WIN_DEPENDENCY_LIBS`/`GSTREAMER_BASE_LIBS` themselves were curated
with. Changed `_bundle_windows` to give a flat `output_dir` copy only to that 20-item subset,
sending the other 28 `windows_dlls()` entries into `lib/` only. Reasoning at the time: a
plugin's own dependencies resolve via `LOAD_WITH_ALTERED_SEARCH_PATH` relative to the
plugin's own directory (`lib/`), so those 28 wouldn't need a `play.exe`-side copy at all.

**Why that reasoning was wrong:** `LOAD_WITH_ALTERED_SEARCH_PATH` only changes how the
*specified* module itself â€” the plugin file GStreamer explicitly hands to
`gst_plugin_load_file` â€” gets found. It does **not** change how the OS loader later resolves
*that plugin's own* import table (its static/implicit dependencies, e.g. `gstnice.dll`
needing `nice-10.dll`) â€” those still go through the normal, process-wide DLL search order,
which checks `play.exe`'s directory and system dirs, never `lib/`. So a dependency DLL that
only exists in `lib/` (because this change removed its `output_dir` copy) is invisible to
every plugin that needs it, even though the plugin file *itself* loads fine.

**Caught by:** the very next real CI run â€” `.github/workflows/test.yml` had just been changed
(a CI-only change, no patch needed â€” see that file's own history/comments) to build with the
real GStreamer media stack instead of `--media-stack dummy`, exercising this exact code path
with real audio/video for the first time ever. The Windows job failed immediately with
`Error initializing GStreamer: ErrorLoadingPlugins([...])`, and a matching explicit
`stderr`/`roves.log` annotation added in that same test.yml change showed exactly why:
`GStreamer-WARNING: Failed to load plugin '...\lib\gstnice.dll': The specified module could
not be found` â€” and identically for `gstogg.dll`, `gstopengl.dll`, `gstopus.dll`,
`gsttheora.dll`, `gstvorbis.dll`, `gstaudiofx.dll`, `gstisomp4.dll`, `gstmatroska.dll`. Ran
`dumpbin /dependents` again, this time on those specific plugin files (using the same
GStreamer 1.22.8 install pulled locally for the build attempt below) â€” every single missing
dependency (`nice-10.dll`, `libogg-0.dll`, `graphene-1.0-0.dll`, `libpng16-16.dll`,
`libjpeg-8.dll`, `opus-0.dll`, `theoradec-1.dll`, `theoraenc-1.dll`, `libvorbis-0.dll`,
`libvorbisenc-2.dll`, `gstcontroller-1.0-0.dll`, `gstriff-1.0-0.dll`, `gstgl-1.0-0.dll`,
`bz2.dll`) was exactly one of the 28 entries this change had removed from `output_dir`. Not a
theoretical concern â€” a real, reproduced failure.

**Fix:** reverted `_bundle_windows` back to the 2026-08-18 entry's original behavior (every
`windows_dlls()` entry duplicated into both `output_dir` and `lib/`, no exceptions) and
removed the now-wrong `GSTREAMER_BASE_LIBS_NEEDED_BY_SERVO_DIRECTLY`/
`GSTREAMER_WIN_DEPENDENCY_LIBS_NEEDED_BY_SERVO_DIRECTLY`/`windows_dlls_needed_flat()` from
`gstreamer.py` entirely â€” net result, `gstreamer.py` is now byte-identical to before this
whole attempt; only `post_build_commands.py`'s docstring keeps a permanent note of why this
was tried and why it doesn't work, so nobody re-attempts the same narrowing without rediscovering this. The bundle root stays at the 2026-08-18 entry's ~58 files â€” see that
entry's own "why this couldn't just move everything into a subfolder naively" for the
already-identified, still-open, bigger option (a launcher-stub binary) if a future attempt
wants to go lower than that.

**Not part of the `roves-action`/`roves-packmaster` sync:** no `mach build`/`mach bundle` CLI surface
changed at any point in this attempt-then-revert.

**Verification:** the failure and the fix are both empirically confirmed against a real CI
run, not just static reasoning â€” the first time this vendoring setup has had that for a
Windows GStreamer/DLL-layout change (every prior entry on this topic notes it *couldn't* run
`mach build` or a real launch). A parallel attempt to reproduce this locally (a real `mach
build --release` on this same dev machine) got all the way through several genuine local
toolchain gaps (missing GStreamer dev libs, a missing/mismatched `lld-link`, missing
`clang-cl`, then an MSVC-STL/LLVM version mismatch compiling `mozjs_sys`) before being
abandoned as a dead end unrelated to this change â€” CI ended up being both faster and more
representative than continuing to fight this machine's own environment. Dry-run-applies
cleanly against a from-scratch pristine v0.4.0 checkout with patches `0001`â€“`0041` already
applied in order.

## 2026-08-20 â€” Windows: shrink the bundle root further, for real this time

**Files:** `ports/servoshell/main.rs`, `ports/servoshell/Cargo.toml`, `python/servo/gstreamer.py`,
`python/servo/post_build_commands.py`.

**Patch:** `patches/servo-v0.4.0/0043-windows-setdlldirectory-shrink-bundle-root.patch`

**Reported as:** even after the 2026-08-18/19 entries got the Windows portable root down to
~58-69 files (engine DLLs at ~58, plus Packmaster's own packed-content files pushing a real
generated bundle to ~69 before that got its own fix), that's still far more than hoped for â€”
the real target was "under 20, ideally closer to 10." The 2026-08-19 revert entry's own "why
this couldn't just move everything into a subfolder naively" section had already identified
the actual fix for this, just deferred as a bigger, riskier change at the time.

**Root cause, precisely:** `LOAD_WITH_ALTERED_SEARCH_PATH` (what GStreamer's own
`gst_plugin_load_file` uses) only changes how *that one specified file* is found â€” it says
nothing about how the OS loader resolves *that file's own* implicit imports once found. A
plugin's dependencies are resolved through whatever the *process-wide* DLL search order
happens to be at that moment, and by default that never includes `lib/`. The 2026-08-19
revert worked around this by duplicating everything into both places instead of fixing the
actual gap.

**The real fix:** `ports/servoshell/main.rs` now calls `SetDllDirectoryW` on Windows, very
early in `main()` (before anything else runs), pointing it at the `lib/` folder next to the
running executable. Per its own documented behavior, this *adds* `lib/` to the front of the
process-wide DLL search order without removing the application directory from it â€” purely
additive, doesn't change how anything already flat next to the binary resolves. Needs
`Win32_System_LibraryLoader` added to `windows-sys`'s feature list in
`ports/servoshell/Cargo.toml`.

Critically, this only helps *plugin* dependencies (resolved at runtime, after `main()` has
already run) â€” it does nothing for `play.exe`'s *own* static/implicit imports, which the OS
loader resolves as part of ordinary PE loading, before `main()` (and therefore before
`SetDllDirectoryW`) ever executes. `python/servo/gstreamer.py`'s
`GSTREAMER_BASE_LIBS_NEEDED_BY_SERVO_DIRECTLY`/
`GSTREAMER_WIN_DEPENDENCY_LIBS_NEEDED_BY_SERVO_DIRECTLY`/`windows_dlls_needed_flat()` (the
same 20-item set curated via `dumpbin /dependents` for the 2026-08-19 attempt â€” recovered
from that commit's history, since the reasoning for exactly which files these are hadn't
changed) are reinstated for exactly that reason: `_bundle_windows` gives a flat copy to a
`windows_dlls()` entry only when it's in that set (or unrecognized by either list); every
other plugin-only dependency now lives in `lib/` only, safely, because of the
`SetDllDirectoryW` call. Net effect on a real bundle: root drops from ~58 (engine files
alone) to ~28 â€” `play.exe`, `diagnose.bat`/`.sh`, `launch.json`, the 20 curated DLLs,
`msvcp140.dll`/`vcruntime140.dll`/`api-ms-win-crt-runtime-l1-1-0.dll` (real `play.exe`
dependencies from the MSVC CRT, unrelated to GStreamer), `libEGL.dll`/`libGLESv2.dll`
(ANGLE) â€” plus `steam_appid.txt`/`steam_api64.dll` when Steam is enabled, both of which
correctly stay flat (Valve's own convention; `steam_api64.dll` is itself a real load-time
dependency of `play.exe`).

**Why not lower still:** the remaining ~28 are a genuine floor with this approach â€” every one
of them resolves before `main()` runs, so `SetDllDirectoryW` structurally can't reach them.
Getting lower would mean either delay-loading them (a linker-level `/DELAYLOAD` change,
resolving them on first *use* instead of at process start) or a thin launcher-stub binary
that calls `SetDllDirectoryW` and re-execs the real engine binary from `lib/` â€” both
meaningfully bigger and riskier than this entry, and both would need real local build
verification to attempt safely, which this machine still can't do (see the 2026-08-19
entry's own toolchain-gap list). Deliberately not attempted here; flagged as a possible
follow-up if the ~28-file floor ever needs to come down further.

**Verification:** compile-checked locally first (`cargo check -p servoshell --bin
servoshell`, with `RUSTFLAGS=-Clinker=link.exe` and a `PYTHON3` pointed at a `uv`-installed
interpreter to work aroundµ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥ëZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥âF†—26ÖRÖ6†–æRw2÷vâFööÆ6†–âv2’ÂF†Vâ6öæf—&ÖVBf÷"&VÀ§f–FW7Bç–ÖÆw2æW‡B4’'Vâ(	BÆÂbÖG&—‚¦ö'2w&VVâ‡&VÂu7G&VÖW"ÂÆÂ2ÆFf÷&×2’À¦æBg&W6‚F÷væÆöBöbF†RV&Æ—6†VBv–æF÷w2FW7B76WBÆæFVBW†7FÇ’3f–ÆW2–âF†P¦'VæFÆR&ö÷Bƒ#’v—F†÷WBF†—2'V–ÆBw2÷vâW‡G&äõDRçG‡F’(	B&–v‡B–âF†Rã#‚Ó3&ævRF†—0¦VçG'’&VF–7FVBÂv—F‚&VÂVF–ò÷f–FVò6öæf—&ÖVB7F–ÆÂv÷&¶–ærà ¢22##bÓ‚Ó#(	Bu7G&VÖW"VF–ò6–æ³¢GFV×FVBÄ””är×v—BÂ&WfW'FVBÒÒ—BFVFÆö6¶V@ ¢¢¤f–ÆW3¢¢¢6ö×öæVçG2öÖVF–ö&6¶VæG2öw7G&VÖW"öVF–õ÷6–æ²ç'6  ¢¢¤æ÷B'BöbF†RF6‚6WB¢¢‡6VR&VÆ÷r’(	BF†—2VçG'’Fö7VÖVçG2f—‚F†Bv2G&–VBæ@§&WfW'FVBF†R6ÖRF’Â6òF†RæW‡BW'6öâ–çfW7F–vF–ærF†—27–×FöÒFöW6âwB&WG'’F†R6ÖP¦'&ö¶Vâ&ö6‚à ¢¢¥&W÷'FVB3¢¢¢&VÂW6W"'V–ÇBFW7B×vVv—F‚6¶Ö7FW"‡6†VÆÂcã"ã"Â&VÂu7G&VÖW"À¦æ÷BÒÖÖVF–×7F6²GVÖ×–’æB&W÷'FVBF†RvV$VF–ò'Æ’FW7B&VW"'WGFöâ6†÷vVBö²(	@§Æ–VBCC‡¢&VWf÷"#×6†æòW†6WF–öâÂ6öçFW‡Bç7FFS¢'Vææ–æv’'WBæ÷F†–ærv0¦VF–&ÆRâ&÷fW2æÆöv6†÷vVBæòW'&÷'2„u7G&VÖW"öÖVF–÷ÆFf÷&Ó£¦–æ—F(	B6VR6W'fòç'6(	@¦Æöw2æ÷F†–æröâ7V66W72ÂöæÇ’Æös£¦W'&÷"öâf–ÇW&R’ÂæBF†RW6W"w2v–æF÷w2÷WGWBFWf–6Rğ§föÇVÖRvW&R6öæf—&ÖVB6÷'&V7BâFFVB6V6öæBÂÆöævW"ƒ'2’FW7BFöæRFòF†R6ÖR'WGFöà¢†ââ÷FW7B×vR÷7&2ôVF–ô'WGFöâçG7†(	Bæ÷B'BöbF†RF6‚6WBÂ6VRF†—2f–ÆRw2÷và¦FW7B×vRöæ÷FW3²F†—2'BöbF†R6†ævRv2¶WBÂ6VR—G2÷vâæ÷FR&VÆ÷r’Fò—6öÆFRF†P§f&–&ÆS¢F†R'2FöæRv2VF–&ÆRÂF†R#×2öæRv6âwB(	Bö–çF–ærBGW&F–öâ÷F–Ö–ær&6P§&F†W"F†âgVæFÖVçFÆÇ’'&ö¶Vâ—VÆ–æRà ¢¢¤F–væ÷6—2‡7F–ÆÂ&VÆ–WfVB6÷'&V7B“¢¢¢u7G&VÖW$VF–õ6–æ³£§Æ’‚–6ÆÇ0¦6VÆbç—VÆ–æRç6WE÷7FFR†w7G&VÖW#£¥7FFS£¥Æ––ær–æBG&VG2ç’æöâÖW'&÷"&W7VÇF2'F†P§6–æ²—2æ÷rÆ––ærâ"u7G&VÖW"w2÷vâ6WE÷7FFV6âÆVv—F–ÖFVÇ’&WGW&â7V66W72v†–ÆRF†P§G&ç6—F–öâ—27F–ÆÂöæÇ’§VæF–ær¢†u5Eõ5DDUô4„ätUô5”ä6’(	BW†7FÇ’v†B†Vç2v†Vâ¦F÷vç7G&VÒVÆVÖVçBæVVG2&VÂ6WGWF–ÖRÂÆ–¶RWFöVF–÷6–æ¶(i"v6—6–æ¶öâv–æF÷w2¦÷Væ–ærF†R7GVÂ÷WGWBFWf–6RÂv†–6‚6öÖÖöæÇ’F¶W2fWr‡VæG&VB×2öâ6öÆB7F'BâF†P¥vV$VF–ò6–FR†2æò–FVç’öbF†—2—2†Væ–æs¢F†R÷66–ÆÆF÷"w27F÷F–ÖR—266†VGVÆV@¦v–ç7BF†RvV"VF–òw&‚w2÷vâ–çFW&æÂ6Æö6²Â6ö×ÆWFVÇ’FV6÷WÆVBg&öÒv†WF†W"F†P¤u7G&VÖW"—VÆ–æR†27GVÆÇ’7F'FVBVÖ—GF–ær6×ÆW2–WBâ6†÷'BöæR×6†÷B6÷VæB†fW'¦6öÖÖöâ&VÂ66R(	BT’6Æ–6·2Âfö÷G7FW2Âç’6†÷'B4e‚Âæ÷B§W7BF†—2F–væ÷7F–2&VW’6à¦f–æ—6‚æBG&–vvW"7G‚æ6Æ÷6R‚–‡v†–6‚FV'2F†R—VÆ–æRF÷vâ’&Vf÷&RF†RFWf–6R†2WfVà¦f–æ—6†VB÷Væ–ærÂ&öGV6–ærF÷FÂ6–ÆVæ6Rv—F‚æòW'&÷"ç—v†W&R–âF†R6†–âFò&W÷'B—Bà ¢¢¤GFV×FVBf—‚‡&WfW'FVB“¢¢¢gFW"6WE÷7FFR…Æ––ær–7V66VVFVBÂÆ’‚–FF—F–öæÆÇ¦6ÆÆVB6VÆbç—VÆ–æRç7FFR†w7G&VÖW#£¤6Æö6µF–ÖS£¦g&öÕ÷6V6öæG2ƒR’–†VÆVÖVçC£§7FFVÂF†P¦&Æö6¶–ærw7EöVÆVÖVçEövWE÷7FFVWV—fÆVçB’æB&÷vFVB—G2&W7VÇFFöòÂ–çFVæF–ærFğ¦Ö¶RÆ’‚–æ÷B&WGW&âö¶VçF–ÂF†R—VÆ–æR†B7GVÆÇ’f–æ—6†VBG&ç6—F–öæ–ærFğ¦Ä””ävà ¢¢¥v‡’—Bv2w&öærÂ6öæf—&ÖVB'’F†R6ÖRW6W"&R×FW7F–ær&VÂ'V–ÆC¢¢¢–ç7FVBöbf—†–æp§F†R6–ÆVæ6RÂF†—2ÖFRF†–æw27G&–7FÇ’v÷'6R(	B7F–ÆÂæòVF–&ÆR6÷VæBÂVF–òæ÷r7F'G2ÆFRÀ¦æB§F†Rv†öÆRvRg&VW¦W2¢f÷"6WfW&Â6V6öæG2â&ö÷B6W6RöbF†R&Vw&W76–öã¢Æ’‚–'Vç0¦öâF†RVF–ò&VæFW"F‡&VB†&VæFW%÷F‡&VBç'6w2VF–õ&VæFW%F‡&VC£¦WfVçEöÆö÷’ÂF†R6ÖP§6–ævÆRF‡&VBF†BÇ6ò§6W'f–6W2¢VF–õ&VæFW%F‡&VD×6s£¥6–æ´æVVDFF(	BF†RÖW76vRF†@¦×W7B&R&ö6W76VBFòW6‚F†R7&2w2f—'7B'VffW"Âv†–6‚—2—G6VÆb&V6öæF—F–öâf÷"F†P§—VÆ–æRWfW"6ö×ÆWF–ær&W&öÆÂæB&V6†–ærÄ””ävâ&Æö6¶–ærF†BF‡&VB–ç6–FP¦VÆVÖVçC£§7FFR‚–v—F–ærf÷"Ä””äv7&VFW2F—&V7BFVFÆö6³¢F†R6öæF—F–öâ&V–ærv—FV@¦öâ6âöæÇ’&R6F—6f–VB'’F†RfW'’F‡&VBF†Bw2&Æö6¶VBv—F–ærf÷"—BâF†Rö'6W'fV@§7–×Fö×2ÖF6‚W†7FÇ“¢F÷FÂg&VW¦Rf÷"F†Rv—B†æ÷F†–æröâF†R&VæFW"F‡&VB&öw&W76W2À¦–æ6ÇVF–ærF†RvV$VF–òw&‚w2÷vâ6Æö6²’ÂF†Vâ&ÆFR"VF–òöæÇ’öæ6RF†RW2F–ÖV÷WBW‡—&W2À¦Æ’‚–&WGW&ç2â†–væ÷&VBF÷vç7G&VÒ’W'&÷"ÂæBF†RF‡&VBf–æÆÇ’G&–ç2—G2&6¶Æör(	@¦'’v†–6‚ö–çBF†R÷&–v–æÆÇ’×66†VGVÆVBFöæRw2F–Ö–ær—2ÖVæ–ævÆW72à ¢¢¥7FGW3¢¢¢&WfW'FVBVF–õ÷6–æ²ç'6&6²FòF†R÷&–v–æÂVæ6öæF—F–öæÀ¦æÖ‡Å÷Â‚’–öæÖöW'"‚âââ–öâ6WE÷7FFVÆöæS²FVÆWFVBF†RF6‚f–ÆRF†—2VçG'¦÷&–v–æÆÇ’ö–çFVBB†CBÖw7G&VÖW"ÖVF–ò×6–æ²×v—BÖf÷"×&VÂ×Æ––ær×7FFRçF6†(	BæWfW ¦W†—7FVBW7G&VÒÂ6òæ÷F†–ærFò&VÇ’’âF†RVæFW&Ç––ær6–ÆVæ6R'Vr—27F–ÆÂ&VÂæB7F–ÆÀ§Væf—†VBâ6÷'&V7Bf—‚v÷VÆBæVVBFò6–væÂ&FWf–6R7GVÆÇ’÷Vâ"v—F†÷WB&Æö6¶–ærF†RF‡&V@§F†B×W7B6W'f–6R6–æ´æVVDFFFòvWBF†W&R(	BRærâu7G&VÖW"'W2vF6‚öâ6W&FP§F‡&VB&V7F–ærFò5”ä5ôDôäVÂ÷"&W7G'V7GW&–ær6ò7FFRÖ6†ævRv—F–ær†Vç2öfbF†P§&VæFW"F‡&VBVçF—&VÇ’âÆVgBVæf—†VB&F†W"F†âGFV×F–æræ÷F†W"VçfW&–f–VB6†ævRFòF†—0§6ÖRf–ÆR&Æ–æB(	BF†—2Ö6†–æR7F–ÆÂ6âwB6ö×–ÆR6W'fòÖÖVF–Öw7G&VÖW&Æö6ÆÇ’†Ö—76–æp¦¶rÖ6öæf–vôu7G&VÖW"FWbÆ–'&&–W2(	B6ÖRv2F†R##bÓ‚Ó#VçG'’&÷fR’Â6òç—F†–æp¦†W&R6â7W'&VçFÇ’öæÇ’&RfW&–f–VB'’&VÂW6W"&V'V–ÆF–æræBFW7F–ærÂv†–6‚—2W‡Vç6—fP§Fò—FW&FRöâ7V7VÆF—fVÇ’à ¢¢¤¶WBÂæ÷B&WfW'FVC¢¢¢F†RFW7B×vR÷7&2ôVF–ô'WGFöâçG7†Æöær×FöæRF–væ÷7F–2'WGFöâƒ'0§FW7BFöæRÆöæw6–FRF†R÷&–v–æÂ#×2&VW’(	BF†B'B6÷'&V7FÇ’F–B—G2¦ö"†—6öÆF–ærF†P§7–×FöÒFòF–Ö–ær&6R’æB&VÖ–ç2W6VgVÂf÷"v†öWfW"–6·2F†—2WæW‡Bâæ÷B'BöbF†P§F6‚6WBÂW"F†—2f–ÆRw2÷vâFW7B×vRöæ÷FW2à ¢22##bÓ‚Ó#(	Bu7G&VÖW"VF–ò6–æ³¢F–væ÷7F–2Æövv–ærf÷"F†RvV$VF–ò6–ÆVæ6R&W÷'@ ¢¢¤f–ÆW3¢¢¢6ö×öæVçG2öÖVF–ö&6¶VæG2öw7G&VÖW"öVF–õ÷6–æ²ç'6  ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóCBÖw7G&VÖW"ÖVF–ò×6–æ²ÖF–væ÷7F–2ÖÆövv–ærçF6†  ¢¢¤föÆÆ÷r×WFòF†RGvòVçG&–W2F—&V7FÇ’&÷fRâ¢¢F†—2Ö6†–æRw2FööÆ6†–âv—2æ÷p§&W6öÇfVC¢Ö6‚&ö÷G7G&ÒÖf÷&6RÒ×–W6‡v–ævWBÖ–ç7FÆÆVB4Ö¶RôÄÅdÒôæ–æ¦õv•‚ÂöâF÷ö`§F†Ru7G&VÖW"Õ5d2FWfVÂ4D²æBÕ5d2Æ–æ¶W"Ç&VG’&W6VçBg&öÒâV&Æ–W"GFV×B’v÷Bf ¦Væ÷Vv‚Fò6ö×–ÆR6W'fòÖÖVF–Öw7G&VÖW&F—&V7FÇ’†6&vò6†V6²×6W'fòÖÖVF–Öw7G&VÖW& §7V66VVG2v—F‚D†ö´uô4ôäd”uõD†ö–çFVBBF&vWBöFWVæFVæ6–W2öw7G&VÖW"óãğ¦×7f5÷ƒƒeócFæB•D„ôã6ö–çFVBBWfÖ–ç7FÆÆVB–çFW'&WFW"(	B6ÖRVçb6†R2F†P£##bÓ‚Ó#VçG'’&VF–7FVBv÷VÆB&RæVVFVB’âgVÆÂÖ6‚'V–ÆF†æVVFVBf÷"F†R&VÂDôÒğ§67&—BVF–ô6öçFW‡FF‚Âæ÷B§W7BF†—2&6¶VæB7&FR–â—6öÆF–öâ’7F–ÆÂæVVG2–ç7FÆÆ–æp§F†BFööÆ6†–â&÷W&Ç’7—7FVÒ×v–FRf—'7C²æ÷BFöæR–âF†—26W76–öâÂFVfW'&VB&6²Fò4’à ¢¢¥v†BF—&V7BÆö6ÂFW7F–æröbF†—27&FRf÷VæBÂæBv‡’—B6†ævW2F†RF–væ÷6—3¢¢¢w&÷FR§F‡&÷vv’W†×ÆR†6ö×öæVçG2öÖVF–öW†×ÆW2öW†×ÆW2ö&VW#×2ç'6Âæ÷B6öÖÖ—GFVB’F†@§&W&öGV6W2VF–ô'WGFöâçG7†w2W†7B66Væ&–òv–ç7BF†R&VÂu7G&VÖW"&6¶VæB(	B6ÖP¦VF–ô6öçFW‡Fö÷66–ÆÆF÷"öv–âw&‚ÂæB7&—F–6ÆÇ’Â6Æ÷6–ærF†R6öçFW‡BöæÇ’öæ6P¦6öçFW‡Bæ7W'&VçE÷F–ÖR‚–‡fW&–f–VB–â6ö×öæVçG2÷67&—BöFöÒöVF–òöVF–÷66†VGVÆVG6÷W&6VæöFRç'6 ¦æB&VæFW%÷F‡&VBç'6Fò&R¦W†7FÇ’¢F†R6Æö6²öæVæFVF7GVÆÇ’f—&W2g&öÒ(	B—BöæÇ¦Gfæ6W2öæ6R&Æö6²†27GVÆÇ’&VVâ&VæFW&VBæB†æFVBFòF†R6–æ²Âf–¦VF–õ&VæFW%F‡&VD×6s£¥6–æ´æVVDFF’&V6†W2F†R66†VGVÆVB7F÷F–ÖRÂF†R6ÖR6öæF—F–öâF†P§&VÂöæVæFVF6ÆÆ&6²v—G2f÷"âÖV7W&VBöâF†—2Ö6†–æS¢WFöVF–÷6–æ¶Föö²ãsf×2Fğ¦7GVÆÇ’&V6‚Ä””äv†FWf–6R÷Vâ6÷7BÂ6öæf—&Ö–ærF†B'BöbF†R÷&–v–æÂF†V÷'’’Â'W@¦g&öÒF†W&RF†R&VæFW"F‡&VB6VBWfW'’7V'6WVVçB&Æö6²FòvVçV–æR&VÂ×F–ÖR6öç7V×F–öà¢†6öæf—&ÖVB'’W6…öFFF–ÖW7F×2G&6¶–ærvÆÂÖ6Æö6²Âæ÷B&6–ær†VB’(	BÖVæ–æp¦öæVæFVFF–FâwBf—&RVçF–Â¢£#SF×2¢¢ÂvVÆÂgFW"ÆÂ#×2öb66†VGVÆVBVF–ò†B7GVÆÇ¦&VVâvVæW&FVBæB†æFVBFòF†RFWf–6Râ–â÷F†W"v÷&G3¢F†R&6¶VæBÂ–â—6öÆF–öâÂFöW2æ÷@§FV"F÷vâF†R—VÆ–æR&Vf÷&RF†R6÷VæB†27GVÆÇ’Æ–VB(	B—B§W7BFVÆ—2F†R7F'B'’F†P¦FWf–6Rw2÷Vâ6÷7BâF†B6öçG&F–7G2F÷FÂ6–ÆVæ6RæBfÇ6–f–W2F†R&GW&F–öâ&6R"F†V÷'¦&÷fR2¦6ö×ÆWFR¢W‡ÆæF–öâÂF†÷Vv‚F†Rãsf×2÷VâÖ6÷7BÖV7W&VÖVçB—G6VÆb7F–ÆÂ7FæG2à¤Ç6ò6W&FVÇ’6öæf—&ÖVBw7G&VÖW%÷ÇVv–åöÆ—7G2÷v–æF÷w2ç'2æ–æ–æ6ÇVFW2w7Gv6–æBF†P¦6öÖÖöâÆ—7B–æ6ÇVFW2w7FWFöFWFV7F(	Bu7G&VÖW$&6¶VæC£¦–æ—E÷v—F…÷ÇVv–ç6†6ö×öæVçG2ğ¦ÖVF–ö&6¶VæG2öw7G&VÖW"öÆ–"ç'6’†&BÖW†—G2†Æös£¦W'&÷"²&ö6W73£¦W†—Bƒ–’–bç¦7W&FVBÇVv–âf–Ç2FòÆöBÂæBF†R&W÷'F–ærW6W"w2&÷fW2æÆöv6†÷w2æ÷&ÖÂ7F'GWv—F€¦æò7V6‚W'&÷"Â6òÖ—76–æröf–ÆVBÇVv–âÆöF–ær—2Ç6ò'VÆVB÷WBà ¢¢¤v—fVâF†R&6¶VæB6†V6·2÷WB–â—6öÆF–öâÂF†R&VÖ–æ–ær6æF–FFW2&R÷WG6–FRv†B§7FæFÆöæR7&FRFW7B6â&V6ƒ¢¢¢6öÖWF†–ær–âF†RDôÒ÷67&—B&–æF–ærÆ–W"&WGvVVâ¥0¦VF–ô6öçFW‡Fö÷66–ÆÆF÷$æöFVæBF†—2&6¶VæB‡VçFW7FVB(	BæVVG2gVÆÂÖ6‚'V–ÆF’Â÷ §6öÖWF†–ær7V6–f–2FòF†R&VÂ'VæFÆVBDÄÂÆ–÷WB‡G&–ÖÖVBÆ–"ö7V'6WBg2âF†RgVÆÂFWfVÀ¥4D²W6VBf÷"F†RÆö6ÂFW7B’÷"F†R&W÷'F–ærW6W"w27GVÂVF–ò†&Gv&R÷&÷WF–ærÂæV—F†W"ö`§v†–6‚Æö6Â—6öÆFVB7&FRFW7B6â6VRà ¢¢¥F†—2F6ƒ¢¢¢FG2Æös£¦–æfòÖ&6VBF–væ÷7F–2Æövv–ær†6VBFòF†Rf—'7B ¦W6…öFF6ÆÇ2W"6–æ²–ç7Fæ6RÂ6ò&VÂÆöær×Æ––ær6÷VæBFöW6âwB7Ò&÷fW2æÆöv’@§F†RW†7Bö–çG2F†RÆö6ÂFW7B&÷fR–ç7G'VÖVçFVBv—F‚W&–çFÆâ(	B—VÆ–æR7FFP§G&ç6—F–öç2æB5”ä5ôDôäVf–'W2×vF6†–ærF‡&VBÂÇW2Æ’‚–ö7F÷‚–w26WE÷7FFV §&W7VÇG2âW&Rö'6W'f&–Æ—G’Âæò&V†f–÷"6†ævR‡6ÖR6WE÷7FFV6ÆÇ22W7G&VÒ’âFVfVÇ@¦ÆörÆWfVÂ—2–æfö†÷'G2÷6W'f÷6†VÆÂöFW6·F÷öÆövv–ærç'6’Â6òF†W6RÆ–æW2ÆæB–à¦&÷fW2æÆövöâ&VÂW6W"w2Ö6†–æRv—F‚æòW‡G&6öæf–wW&F–öâæVVFVBâ–çFVçC¢vWBF†—2–çFğ§F†RæW‡BFW7F4’'V–ÆB†FW7Bç–ÖÆG&–vvW'2öâF6†W2ò¢¦6†ævW2’Â†fRF†R&W÷'F–æp§W6W"F÷væÆöB—BæB&RÖ6Æ–6²F†R&VWÂæB&VBF†R&VÂF–Ö–æröfbF†V—"7GVÂ†&Gv&Ræ@§F†R7GVÂ'VæFÆVBDÄÂÆ–÷WB(	B6Æ÷6–ærW†7FÇ’F†RGvòv2F†RÆö6ÂFW7B&÷fR6÷VÆFâw@§&V6‚à ¢¢¥&W6öÇWF–öâÂæBv‡’F†—2F6‚—2&WfW'FVC¢¢¢F‡&VR&÷VæG2öb&VÂÖÖ6†–æRFW7F–ærf–F†—0¦Æövv–ærGW&æVBW6öÖWF†–æröFFW"F†âf—†VBF–Ö–ær&6S¢öâF†R&W÷'F–ærW6W"w2Ö6†–æRÀ¦VF–ô'WGFöâçG7†w2GFW&âöbæWrVF–ô6öçFW‡B‚–²7G‚æ6Æ÷6R‚–öâWfW'’6Æ–6²&öGV6V@§v–ÆFÇ’–æ6öç6—7FVçB6ö×ÆWF–öâF–ÖW2W"6Æ–6²ƒC†×2Fò÷fW"#&VÂ6V6öæG2Âf÷"FöæW0§&öw&ÖÖVBB#×2ó#×2’ÂæBBÆV7BöæR6öçFW‡BW"6W76–öâæWfW"&V6V—fVB—G266†VGVÆV@§7F÷BÆÂ(	B—G27F÷‚–öæÇ’f—&VBBvRFV&F÷vâÂÆöæw6–FRv–æF÷rÖ6Æ÷6R6ÆVçWâF†@§GFW&âF–FâwB&W&öGV6Rv—F‚ââ÷FW7B×vR÷7&2õFöæT'WGFöâçG7†…FöæRæ§2’Â6öæf—&ÖVBVF–&ÆR'§F†R6ÖRW6W"öâF†R6ÖRÖ6†–æRâFöæRæ§2¶VW26–ævÆRÂÆ¦–Ç’Ö7&VFVBvÆö&À¦VF–ô6öçFW‡F&WW6VB7&÷726ÆÇ2†FöæRævWD6öçFW‡B‚–öFöæRç7F'B‚––âF†RFöæV6¶vRÀ§fW&–f–VB'’&VF–ær—G26÷W&6R(	B—BFöW2¦æ÷B¢VvW&Ç’6öç7G'V7B&VÂ6öçFW‡Böâ–×÷'BÀ§'VÆ–ær÷WBöæRV&Ç’F†V÷'’’&F†W"F†â6öç7G'V7F–æræBFV&–ærF÷vâg&W6‚6öçFW‡BW §6÷VæBF†Rv’VF–ô'WGFöâçG7†F–BâF†BF–ffW&Væ6R(	BW'6—7FVçB÷&WW6VB6öçFW‡Bg2à¦7&VFRÖæBÖ6Æ÷6R×W"×6÷VæB(	B—2F†RöæR6ÆV"f&–&ÆRF†B6†ævVB&WGvVVâ&FöW6âwBv÷&²"æ@¢'v÷&·2"–âF†—2–çfW7F–vF–öâÂF†÷Vv‚F†R&V6—6RÖV6†æ—6Ò‡v‡’F†RW"Ö6Æ–6²GFW&à§7V6–f–6ÆÇ’7F'fW2ö†æw2&VæFW"F‡&VBöâF†—2Ö6†–æR’v2æWfW"&ö÷BÖ6W6VBBF†P¤u7G&VÖW"ôDôÒÆWfVÃ²—Bv÷VÆBæVVBV—F†W"gVÆÂÆö6ÂÖ6‚'V–ÆFv—F‚DôÒÖÆ–W ¦–ç7G'VÖVçFF–öâÂ÷"F†R6ÖRFW7B&W&öGV6VBöâ÷F†W"†&Gv&RÂæV—F†W"FöæR†W&Rà ¥&7F–6ÂW6†÷C¢F†—2f÷&²w2÷vâF–væ÷7F–2vRæ÷röæÇ’FW7G2VF–òF‡&÷Vv‚FöæRæ§2‡6VP¦ââ÷FW7B×vR÷7&2õFöæT'WGFöâçG7†w2÷vâ6öÖÖ—B’ÂÖF6†–ær†÷r&VÂvÖR—2Æ–¶VÇ’Fò&öGV6P§6÷VæBç—v’†Æ–'&'’v—F‚ÖævVBW'6—7FVçB6öçFW‡B’&F†W"F†â&p¦VF–ô6öçFW‡Fö÷66–ÆÆF÷$æöFV6ÆÇ2W"öæR×6†÷BVffV7Bâ6–æ6RF†Rö'6W'fVB&ö&ÆVÒ—0§7V6–f–2FòW6vRGFW&âF†—2f÷&²w2÷vâF–væ÷7F–72æòÆöævW"W†W&6—6RÂæB&W&öGV6–ær—@¦gW'F†W"v÷VÆBæVVB&VÂ†&Gv&RF†—26W76–öâFöW6âwB†fRgWGW&R66W72FòÂF†RF–væ÷7F–0¦Æövv–ærFFVB†W&R—2&WfW'FVB(	BVF–õ÷6–æ²ç'6—2&6²Fò'—FRÖ–FVçF–6Âv—F‚&—7F–æP§W7G&VÒÂæBCBÖw7G&VÖW"ÖVF–ò×6–æ²ÖF–væ÷7F–2ÖÆövv–ærçF6†—2FVÆWFVBâ–bgWGW&P§&W÷'B7W&f6W2F†R6ÖR'&rVF–ô6öçFW‡BW"öæR×6†÷B6÷VæB&öGV6W2æòöW'&F–2VF–ò §7–×FöÒÂ7F'Bg&öÒF†—2VçG'’–ç7FVBöb&RÖFW&—f–ærF†R–çfW7F–vF–öâg&öÒ67&F6‚(	Bæ@¦6öç6–FW"F†BF†R&VÂf—‚Â–böæRW†—7G2Â—2Ö÷&RÆ–¶VÇ’6W'fò÷6W'fòÖÖVF–6öæ7W'&Væ7’—77VP¦&÷VæB&–BVF–ô6öçFW‡F7&VF–öâ÷FV&F÷vâF†âç—F†–ær–âF†—2f–ÆR7V6–f–6ÆÇ’Â6–æ6P¦VF–õ÷6–æ²ç'6–â—6öÆF–öâ‡6VRF†RF‡&÷vv’W†×ÆR&÷fR’&V†fVB6÷'&V7FÇ’à ¢ÒÒĞ ¢22##bÓ‚Ó#"(	B&ö÷B7Æ6ƒ¢7F’WF‡&÷Vv‚F†RvRÖÆöBv—BFöòÂæB7F÷f¶–ær&öw&W70 ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6Â÷'G2÷6W'f÷6†VÆÂ÷'Vææ–æuö÷7FFRç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóCBÖ&ö÷B×7Æ6‚Ö6÷fW"×vRÖÆöBÖæBÖ–æFWFW&Ö–æFR×&öw&W72çF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢æòWV—fÆVçB(	B&Vf–æW2F†R##bÓ‚Ó’$æF—fR&ö÷B7Æ6‚"Â##bÓ‚Ó ¢$æWfW"6†÷rv†—FR&Vf÷&RF†RvÖR7F'G2"ÂæB##bÓ‚Ó"&ö÷B×7Æ6‚VçG&–W2†ÆÂgW'F†W"W §F†—2f–ÆR’à ¢¢¥&W÷'FVBF—&V7FÇ’Âg&öÒ&VÂÆVæ6ƒ¢¢¢F†R&÷fW2Ö'&æFVB7Æ6‚V&VBÆÖ÷7@¦–ÖÖVF–FVÇ’Â'WBv2F†VâföÆÆ÷vVB'’Æ–â&Æ6²67&VVâf÷"æ÷F–6V&ÆR7G&WF6‚&Vf÷&RF†P¦vÖR—G6VÆbV&VB(	B&VB2'7Æ6‚ÂF†Vâ&Æ6²67&VVâÂF†VâF†RvÖR"Âæ÷B2öæP¦6öçF–çV÷W2ÆöBâ6W&FVÇ’ÂF†R7Æ6‚w2÷vâ&öw&W72&"Æöö¶VBf¶S¢Çv—2gVÆÇ’f–ÆÆVBÀ¦æWfW"f—6–&Ç’æ–ÖF–ærà ¢¢¥&ö÷B6W6RöbF†R&Æ6²67&VVã¢¢¢F†R##bÓ‚Ó"&†öÆBF†R&ö÷B7Æ6‚f÷"Ö–æ–×VĞ¦GW&F–öâ"VçG'’w2Ô”åõ5Ä4…ôEU$D”ôæöæÇ’6÷fW'2£¦f–æ—6…ö–æ—F—G6VÆb†'V–ÆF–ærF†P¥6W'fò–ç7Fæ6RÂ÷Væ–ærF†R&VÂvV%f–Wv’(	BF†RÖöÖVçBf–æ—6…ö–æ—F&WGW&ç2Â7FFV ¦&V6ÖR'Vææ–ævæBF†R7Æ6‚7F÷VB&V–ær–çFVBBÆÂâWfW'—F†–ærg&öÒF†W&R(	BF†P§vRw2÷vâ…DÔÂô¥2'6–ærÂ76WBÆöF–ærÂæBf—'7B&VæFW"(	B†Vç2¦gFW"¢F†B†æFöfbÀ¦æBf÷"†÷vWfW"ÆöærF†BF¶W2ÂF†R&VÂvV%f–Wvw2÷vâ&æ÷F†–ær–çFVB–WB"6ÆV"6öÆ÷ ¢†÷VR&Æ6²Â6VRF†R##bÓ‚ÓVçG'’’—2W†7FÇ’v†B6†÷vVBF‡&÷Vv‚âÔ”åõ5Ä4…ôEU$D”ôæ §v2æWfW"ÖVçBFòÂæBFöW6âwBÂ6÷fW"F†—2†6RBÆÂ(	B—BöæÇ’wV&çFVW2F†R7Æ6‚v0§f—6–&ÆRf÷"¦BÆV7B¢†Æb6V6öæB&Vf÷&Rf–æ—6…ö–æ—F'Vç2Â–æFWVæFVçBöb†÷rÆöærF†P§vRF¶W2Fò7GVÆÇ’–çB6öÖWF†–ærgFW'v&Bà ¢¢¥&ö÷B6W6RöbF†Rf¶RÖÆöö¶–ær&öw&W72&#¢¢¢G&u÷7Æ6…÷&öw&W75ö&&f–ÆÆVBFòF†P¦g&7F–öâ&W÷'FVB'’W‡G&7Eö&ö÷E÷v—F…÷&öw&W76†7W÷'Bö6öçFVçB×6¶W"÷7&2öW‡G&7Bç'6’À§v†–6‚öæÇ’&W÷'G2&öw&W72W"§v†öÆR&ö÷B6²¢(	BæBF†R&ö÷B6WB—2FVÆ–&W&FVÇ’§W7BF†P§vRw2÷vâ…DÔÂÇW2v†FWfW"—BF—&V7FÇ’&VfW&Væ6W2Â'W7VÆÇ’öæÇ’öæR÷"Gvò6·2"W"F†@¦gVæ7F–öâw2÷vâFö26öÖÖVçBâöâF†R÷fW'v†VÆÖ–ævÇ’6öÖÖöâ66R†66†R†—Bg&öÒ&Wf–÷W0¦ÆVæ6‚Â÷"ç’6ÖÆÂ&ö÷B6WB’ÂF†RfW'’f—'7B&öw&W72&W÷'BÇ&VG’&VG2ãÂ&Vf÷&R§6–ævÆRg&ÖRBç’÷F†W"fÇVRWfW"vWG2–çFVB(	B6ò–â&7F–6RF†R&"v2Çv—2gVÆÂæ@¦æWfW"f—6–&Ç’Ö÷fVBÂ&VF–ær2'&ö¶Vâ&F†W"F†â2&Ç&VG’FöæR"à ¢¢¤6†ævS¢¢  ¢Ò¢¥F†R7Æ6‚æ÷r7F—2WF‡&÷Vv‚F†RvRÖÆöBv—BÂæ÷B§W7BF‡&÷Vv€¢f–æ—6…ö–æ—Fâ¢¢†VFVEv–æF÷vv–æVBvUöÆöE÷7Æ6…÷6–æ6S¢6VÆÃÄ÷F–öãÄ–ç7FçCãæÂ6W@¢'’æWr&Vv–å÷vUöÆöE÷7Æ6‚‚–(	B6ÆÆVBg&öÒf–æ—6…ö–æ—F&–v‡B&Vf÷&P¢'Vææ–æu÷7FFRæ÷Vå÷v–æF÷r‚âââ–÷Vç2F†R&VÂvV%f–Wvâv†–ÆR—Bw26WBÀ¢†æFÆU÷v–æ—E÷v–æF÷uöWfVçFw2&W–çB'&æ6‚¶VW2–çF–ærF†R&ö÷B7Æ6‚–ç7FVBö`¢6ö×÷6—F–ærF†R&VÂvRÂ6†V6¶VBv–ç7BæWr'Vææ–æt7FFS£¦—5ö–æ—F–Å÷vUöÆöFVB‚– ¢†&6¶VB'’æWr–æ—F–Å÷vV'f–Wuö–Fö–æ—F–ÅöÆöEö6ö×ÆWFVf–VÆG2Â6WBg&öĞ¢æ÷F–g•öÆöE÷7FGW5ö6†ævVFF†Rf—'7BF–ÖRF†R¦–æ—F–Â¢vV%f–Wv(	BG&6¶VB'’–BÂ6WBöæ6P¢–â÷Vå÷v–æF÷v(	B&V6†W2ÆöE7FGW3£¤6ö×ÆWFV’æBæWrÔ…õtUôÄôEõ5Ä4…ôEU$D”ôæ ¢ƒ‡2’6fWG’F–ÖV÷WBÂ6òvRF†BæWfW"6–væÇ2&VG’FöW6âwB†ærF†R7Æ6‚f÷&WfW ¢†ÖF6†W2F†—26öFV&6Rw2W†—7F–ær&æWfW"ÆVfRF†RW6W"7GV6²öâ7Æ6‚f÷&WfW" ¢†–Æ÷6÷‡’(	B6VRF†R&ö÷BÖW‡G&7F–öâÖf–ÇW&R†æFÆ–ær–â'VæFÆUöÆVæ6‚ç'6’âF†—2—2¢ÆV7BÖ&B&÷‡’Âæ÷BG'VRf—'7B×–çB6–væÃ¢ÆöE7FGW3£¤6ö×ÆWFV—2DôÒ&VF–æW70¢†Fö7VÖVçBç&VG•7FFRÓÒ&6ö×ÆWFR&’Âæ÷B'F†R6ö×÷6—F÷"†2&W6VçFVBg&ÖR"(	BF†P¢##bÓ‚ÓVçG'’Ç&VG’Fö7VÖVçFVBF†Bæò7V6‚6–væÂ—2W‡÷6VBFòF†RVÖ&VFFW"FöF’à¢7F–ÆÂÆ&vRÂ†öæW7B–×&÷fVÖVçB÷fW"6†÷v–ærF†R&VÂvRF†R–ç7FçB—G2vV%f–Wv ¢W†—7G2à¢Ò¢¤æWr†VFVEv–æF÷s£§7Æ6…öæ–ÖF–öå÷v¶UöFVFÆ–æR‚e'Vææ–æt7FFR–¢¢&WGW&ç2F†RæW‡@¢æ–ÖF–öâ×F–6²FVFÆ–æRv†–ÆRF†R7Æ6‚—26÷fW&–ærF†RvRÂ÷"æöæVöæ6R—Bw2FöæR(	@¢W6VB'’GvòæWr6ÆÂ6—FW26òF†Rv–æ—BWfVçBÆö÷¶VW2F–6¶–ærWfVâGW&–ærâ÷F†W'v—6P¢gVÆÇ’–FÆRv—B†æVVFVB&÷F‚Fòæ–ÖFRF†R7Æ6‚æBFò&ö×FÇ’æ÷F–6P¢ÆöE7FGW3£¤6ö×ÆWFVf—&–ærv—F‚æògW'F†W"vR7F—f—G’gFW"—B“¢ç'6w0¢æWuöWfVçG6æ÷rÇ6ò†æFÆW27FFS£¥'Vææ–æv‡&Wf–÷W6Ç’öæÇ’&ö÷F–æv’ÂæBæWp¢g&VRgVæ7F–öâ6WE÷'Vææ–æuö6öçG&öÅöfÆ÷v†6ÆÆVBg&öÒ&÷F‚v–æF÷uöWfVçFw2æ@¢W6W%öWfVçFw2'Vææ–ævF–Ç2Â&WÆ6–ærF†V—"&Wf–÷W2Væ6öæF—F–öæÀ¢6öçG&öÄfÆ÷s£¥v—F’&×26öçG&öÄfÆ÷s£¥v—EVçF–Æ–ç7FVBv†VæWfW"ç’v–æF÷rw27Æ6‚—0¢7F–ÆÂ7F—fRà¢Ò¢¥F†R&öw&W72&"—2æ÷r–æFWFW&Ö–æFRÂæ÷BFWFW&Ö–æFRÂf÷"F†R¦VçF—&R¢7Æ6‚GW&F–öâ¢ ¢†&÷F‚F†RW‡G&7F–öâöÔ”åõ5Ä4…ôEU$D”ôæv—BæBF†RæWrvRÖÆöBv—B’(	@¢G&u÷7Æ6…÷&öw&W75ö&&†wV’ç'6’æòÆöævW"F¶W2³ÂÖg&7F–öã²—BF¶W2VÆ6VF ¢‡vÆÂÖ6Æö6²F–ÖR6–æ6Rv†–6†WfW"v—B—27W'&VçFÇ’7F—fR’æBG&w2f—†VB×v–GF‚v†—FP¢†–v†Æ–v‡BF†B–ær×öæw2&6²æBf÷'F‚7&÷72F†RG&6²ÂG&—fVâW&VÇ’'’F†BVÆ6V@¢F–ÖRâ†öæW7BÂ6öçF–çV÷W2Ö÷F–öâ–ç7FVBöb7V6–f–2†æBÂW"F†R&ö÷BÖ6W6R&÷fRÀ¢g&WVVçFÇ’w&öær’6ö×ÆWF–öâW&6VçFvRâwV“£§WFFU÷7Æ6†ö†VFVEv–æF÷s£§–çE÷7Æ6† ¢vW&R&WG—VBg&öÒ&öw&W73¢c3&FòVÆ6VC¢GW&F–öæFòÖF6‚à¢ÒWfVçC£¤&ö÷E&öw&W76w2–ÆöB†W‡G&7Eö&ö÷E÷v—F…÷&öw&W76w2&VÂW"×6²g&7F–öâ’—0¢æòÆöævW"W6VBFòG&—fRF†R&"w2f–ÆÂ(	BF†RWfVçB—G6VÆbÂæBF†R&6¶w&÷VæB×F‡&V@¢W‡G&7F–öâ×&öw&W72ÇVÖ&–ærF†B6VæG2—BÂ&R&÷F‚ÆVgB2Ö—2‡7F–ÆÂ&VÂÂ7F–ÆÀ¢÷FVçF–ÆÇ’W6VgVÂ6–væÂf÷"vVçV–æVÇ’Æ&vR&ö÷B6WB“²W6W%öWfVçFw2†æFÆW"f÷"—Bæ÷p¢§W7B&WVW7G2&VG&ræB–væ÷&W2F†RfÇVRÂ&F†W"F†â7F÷&–ær—Bà¢Ò7FFS£¤&ö÷F–ævw2&öw&W73¢c3&f–VÆB—2&VÖ÷fVB(	BæòÆöævW"æVVFVBÂ6–æ6P¢–çE÷7Æ6†æ÷rFW&—fW2—G2æ–ÖF–öâ6Æö6²F—&V7FÇ’g&öÒW‡G&7F–öå÷7F'FVBæVÆ6VB‚–à¢ÒÖ–æ÷"Vff–6–Væ7’f—‚æ÷F–6VBv†–ÆRF÷V6†–ærF†—3¢G'•öf–æ—6…ö&ö÷F–ævw2&Wf–÷W0¢v—EVçF–Æ66†VGVÆ–ærÂöæ6R7BÔ”åõ5Ä4…ôEU$D”ôæ'WB7F–ÆÂv—F–æröà¢W‡G&7F–öåöFöæVÂ6ö×WFVB¦W&òÖGW&F–öâv—B†6GW&F–æu÷7V&&÷GFöÖ–ær÷WBB¦W&ò’À¢v†–6‚'W7’ÖÆö÷2F†RWfVçBÆö÷BgVÆÂF–ÇBf÷"†÷vWfW"Æöær6Æ÷rW‡G&7F–öâF¶W2âæ÷p¢F–6·2BF†R6ÖRf—†VB5Ä4…ôä”ÔD”ôåõD”4¶‡ã36×2Â3g2’W6VBWfW'—v†W&RVÇ6R–âF†—0¢VçG'’–ç7FVBà ¢¢¥v‡’‚6V6öæG2f÷"Ô…õtUôÄôEõ5Ä4…ôEU$D”ôæ¢¢¢ÆöærVæ÷Vv‚F†BW76VçF–ÆÇ’ç’&VÀ¦vÖRw2vR&V6†W2Fö7VÖVçBç&VG•7FFRÓÒ&6ö×ÆWFR&vVÆÂ&Vf÷&R—Bf—&W2Â6†÷'BVæ÷Vv€§F†BvVçV–æVÇ’'&ö¶Vâö‡VærÆöBFöW6âwBÆVfRF†RW6W"7F&–ærB&÷fW2Ö'&æFVB7Æ6‚F†@§&VG22g&÷¦Vââæ÷B6¶VBF—&V7FÇ’(	B§VFvÖVçB6ÆÂ–âF†R6ÖR7—&—B2Ô”åõ5Ä4…ôEU$D”ôæ ¦æBF†RW‡G&7F–öâÖf–ÇW&R&FöâwB†ærf÷&WfW""†–Æ÷6÷‡’VÇ6Wv†W&R–âF†—2f–ÆS²&Wf—6—B–b§&VÂÆVæ6‚6†÷w2—Bw2Föò6†÷'B†Æ&vRvÖRv†÷6RvRvVçV–æVÇ’F¶W2ÆöævW"Fò&V6‚DôĞ§&VF–æW72’÷"FöòÆöær†'&ö¶VâÆöBÆVf–ærF†R7Æ6‚Wæ÷F–6V&Ç’&Vf÷&RF†RF–ÖV÷W@¦ÖW&6–gVÆÇ’VæG2—B’à ¢¢¥fW&–f–6F–öã¢¢¢æ÷B6ö×–ÆVBVæB×FòÖVæB–âF†—2Vçf—&öæÖVçB(	B6&vò6†V6²×6W'f÷6†VÆÆ ¦f–Ç2–ÖÖVF–FVÇ’öâÆÆBÖÆ–æ²æW†VöÆ–æ²æW†Væ÷B&V–ærf÷VæB†æòÕ5d2'V–ÆBFööÇ2–ç7FÆÆV@¦†W&R’ÂF†R6ÖR¶–æBöbVçf—&öæÖVçBvV&Æ–W"VçG&–W2†—Bv—F‚Æ–'VFWb×7—6÷¶rÖ6öæf–röà¤Æ–çW‚â&Wf–WvVB6&VgVÆÇ’'’†æB–ç7FVC¢WfW'’æWrö6†ævVB6ÆÂ6—FRw2G—W2Â&÷'&÷p¦Æ–fWF–ÖW2†–â'F–7VÆ"Âv–æF÷uöWfVçFöW6W%öWfVçFæ÷r6ÆöæR&3Å'Vææ–æt7FFSæ÷WBö`¦6VÆbç7FFVw2&÷'&÷r¦&Vf÷&R¢6ÆÆ–ær6VÆbçV×÷6W'fõöWfVçEöÆö÷Âv†–6‚æVVG2f×WB6VÆf(	@§F†R÷&–v–æÂ6öFRv÷Bv’v—F†÷WBF†—2&V6W6Ræ÷F†–ærgFW"F†Bö–çBW6VB7FFV’Âæ@¦VwV–öv–æ—F’W6vRvW&R6†V6¶VBv–ç7B†÷rF†W’w&RW6VBVÇ6Wv†W&R–âF†—26ÖR6öFV&6Rà¢¢¥v†öWfW"'V–ÆG2F†—2æW‡B6†÷VÆBFò&VÂâöÖ6‚'V–ÆFöâöÖ6‚'Væv–ç7B&VÂ'VæFÆV@¦vÖRæB6öæf—&ÒÂöââ7GVÂ6Æ÷rÖ—6‚ÆöBÂF†BF†R7Æ6‚æ÷rf—6–&Ç’W'6—7G2‡v—F‚¦Ö÷f–ær&öw&W72–æF–6F÷"’ÆÂF†Rv’F‡&÷Vv‚FòF†RvÖRw2÷vâf—'7Bg&ÖRÂv—F‚æò&Æ6°¦v(	BæBF†BFVÆ–&W&FVÇ’'&ö¶VâöæWfW"×&W6öÇf–ærvR7F–ÆÂ&V6÷fW'2gFW ¦Ô…õtUôÄôEõ5Ä4…ôEU$D”ôæ–ç7FVBöb†æv–ærâ¢  ¢22##bÓ‚Ó#B(	Bf—ƒ¢&ö÷BÖ'6öÇWFR76WB&VfW&Væ6W2†'VæFÆW"w2FVfVÇB’&W6öÇfVBv–ç7BF†Rõ2f–ÆW7—7FVÒ&ö÷B–ç7FVBöbF†RvÖRw26öçFVçB&ö÷@ ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2öf–ÆRç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóCR×&V&6R×&ö÷BÖ'6öÇWFRÖf–ÆR×F‡2×FòÖ6öçFVçB×&ö÷BçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢Væ6†ævVBf÷"ç—F†–ærF†BÇ&VG’&W6öÇfW2(	BF†—2öæÇ’FG2¦fÆÆ&6²f÷"v†Bv÷VÆB÷F†W'v—6R&R†&Bf–ÇW&Râf–ÆU&÷Fö6öÄ†æFÆW&—G6VÆb†2æğ§W7G&VÒWV—fÆVçBBÆÂ‡6VRF†R%7Æ—B6¶VB6öçFVçBâââ"VçG'’&÷fRf÷"—G2÷&–v–â’à ¢¢¥&W÷'FVBF—&V7FÇ’Âg&öÒ&VÂÆVæ6‚öb&VÂvÖR†—†’×fâ×&V7B×FV×ÆFVÂf–¦&÷fW2Ö7F–öæw2æWrVæB×FòÖVæBFW7B(	B6VRF†B&Wòw2÷vâFW7Bç–ÖÆ“¢¢¢Æ–â&Æ6°§v–æF÷rÂæò6öçFVçBWfW"&VæFW&VBâ&÷fW2æÆöv6†÷vVBF†R7GVÂ6W6Röæ6R6öÖVöæRÆöö¶VC  ¦FW‡@¤U%$õ"67&—C£§67&—EöÖöGVÆUÒfWF6†–ærÖöGVÆR67&—Bf–ÆVB÷Væ–ærf–ÆRf–ÆV@¤U%$õ"67&—C£¦FöÓ£¦‡FÖÃ£¦‡FÖÇ67&—FVÆVÖVçEÒfWF6†–ær6Æ76–267&—Bf–ÆVB÷Væ–ærf–ÆRf–ÆVB†f–ÆS¢òòô3¢÷&Vv—7FW%5ræ§2¦  ¢¢¥&ö÷B6W6S¢¢¢ÒÖ6öçFVçBÖF—"F—7Böw2÷vâ–æFW‚æ‡FÖÆ&VfW&Væ6W2—G267&—G0§&ö÷B×&VÆF—fR†ö76WG2ö–æFW‚Õ………‚æ§6Â÷&Vv—7FW%5ræ§6’(	Bf—FRw2FVfVÇB&6S¢ròvÂæ@¦WfW'’÷F†W"Ö¦÷"'VæFÆW"w2FVfVÇBFöòÂ6–æ6Ræ÷&ÖÆÇ’F†B6öçFVçB—26W'fVB÷fW"‡GG‡2¦g&öÒâ7GVÂFöÖ–â&ö÷Bâ&÷fW2÷Vç2F†B–æFW‚æ‡FÖÆf–&&Rf–ÆS¦U$Â–ç7FV@¢†æWfW"â‡GG‡2’6W'fW"’(	BW"F†RU$Â7V2Â&ö÷B×&VÆF—fR&VfW&Væ6R&W6öÇfVBv–ç7B¦f–ÆS¦Fö7VÖVçB&V6öÖW2f–ÆS¢òòö76WG2ö–æFW‚Õ………‚æ§6Â’æRâ3¥Æ76WG5Âââæöâv–æF÷w2÷ ¦ö76WG2òââæöâÆ–çW‚öÖ4õ3¢F†R&VÂõ2f–ÆW7—7FVÒ&ö÷BÂæ÷BF†RvÖRw2÷vâ6öçFVç@¦F—&V7F÷'’âW†7FÇ’v†Bç’'&÷w6W"FöW2f÷"f–ÆS¢òöFö7VÖVçB÷VæVBF—&V7FÇ’–ç7FVBö`§6W'fVB(	Bæ÷B'Vr–âF†RU$Â&W6öÇWF–öâ—G6VÆbÂ§W7BÖ—6ÖF6‚æö&öG’†—G2VçF–Â&VÀ¦'VæFÆW"w2FVfVÇB÷WGWBÖVWG2f–ÆS¦ÖÆöFVBVæv–æRâf–ÆU&÷Fö6öÄ†æFÆW&†FFVB'’F†P¢%7Æ—B6¶VB6öçFVçBâââ"VçG'’&÷fR’†BæòÆöv–2Fòæ÷F–6R÷"6÷'&V7Bf÷"F†—2BÆÂ(	@¦—B§W7BG&–VBF†RÆ—FW&ÂÂÇ&VG’×w&öærF‚æB&W÷'FVBæWGv÷&´W'&÷#£¥&W6÷W&6TÆöDW'&÷& ¢‚$÷Væ–ærf–ÆRf–ÆVB"’Æ–¶RF†R7Fö6²†æFÆW"v÷VÆBf÷"ç’÷F†W"Ö—76–ærf–ÆRà ¢¢¤6†ævS¢¢¢f–ÆU&÷Fö6öÄ†æFÆW&v–æVB–æ—F–ÅöF—&‡F†RF—&V7F÷'’6öçF–æ–ærF†P¦Fö7VÖVçB&÷fW2v2ÆVæ6†VBv—F‚(	B6WBVæ6öæF—F–öæÆÇ’Â–æFWVæFVçBöb6¶VFÂ6òF†—0¦Ç6ò6÷fW'2ÒÖ6öçFVçBÖ6ö×&W72æöæVæB&rFWbÒ×W&ÆÆVæ6‚Âæ÷B§W7B6¶V@¦6öçFVçB’æB&V&6U÷Fõö6öçFVçE÷&ö÷FÂ6öç7VÇFVBöæÇ’2fÆÆ&6²öæ6RF†RÆ—FW&ÂF€¦Ç&VG’f–ÆVBFò&W6öÇfS¢7G&—2ç’6ö×öæVçC£¥&Vf—†ö6ö×öæVçC£¥&ö÷DF—&öfbF†P¦f–ÆVBF‚æB&RÖ¦ö–ç2F†R&VÖ–æFW"öçFò–æ—F–ÅöF—&ÂF†Vâ&WG&–W2†Vç7W&Uöf–Æ&ÆV ¦–æ6ÇVFVBÂ6ò&V&6VBF‚VæFW"6¶VB6öçFVçB7F–ÆÂG&–vvW'2öâÖFVÖæBW‡G&7F–öà¦6÷'&V7FÇ’’â&WVW7BF†BÇ&VG’&W6öÇfW2f÷"&VÂ(	B–æ6ÇVF–ærF†Rfæ—6†–ævÇ’VæÆ–¶VÇ¦66RöbvVçV–æRõ2×&ö÷Bf–ÆR†Væ–ærFò6†&RvÖR76WBw2æÖR(	B—2ÆVgBVçF÷V6†VBà ¢¢¥fW&–f–6F–öã¢¢¢6ö×–ÆVB6ÆVâ(	BâöÖ6‚'V–ÆF†FV'VrÂÕ5d2FööÆ6†–â6÷W&6VBÖçVÆÇ§f–f7f'3cBæ&F²ÄÅdÒw2ÆÆBÖÆ–æ²æW†VFFVBFòD†Â6–æ6RF†—2Vçf—&öæÖVçBFöW6âw@¦†fRV—F†W"öâD†'’FVfVÇB’v÷BÆÂF†Rv’F‡&÷Vv‚6ö×–Æ–ær6W'f÷6†VÆÆ—G6VÆbv—F€§¦W&òW'&÷'2ÂöæÇ’f–Æ–ærBF†Rf–æÂÆ–æ²7FWöââVç&VÆFVBÂ&RÖW†—7F–ær7–Ö&öÀ¦Ö—6ÖF6‚–âÖ÷¦§2w2'VæFÆVB”5R2²²6öFR†VæFVf–æVB7–Ö&öÃ¢õ÷7FEöf–æEöf—'7Eööe÷G&—f–Å÷÷5ó ¢òõ÷7FE÷6V&6…óÂ&VfW&Væ6VBg&öÒVÆö2æ7öçVÖ&W$f÷&ÖGFW%6¶VÆWFöâæ7’(	BâÕ5d0¥5DÂ÷FööÇ6WBfW'6–öâÖ—6ÖF6‚öâF†—2Ö6†–æR7V6–f–6ÆÇ’Âæ÷F†–ærF÷V6†VB'’F†—26†ævRà¢¢¥v†öWfW"'V–ÆG2F†—2æW‡B6†÷VÆB6öæf—&Ò&VÂÆVæ6‚öb—†’×fâ×&V7B×FV×ÆFV†÷"ç¦÷F†W"f—FRÖFVfVÇBÖ÷WGWBvÖR’æ÷r7GVÆÇ’&VæFW'2¢¢ÂæBF†B&÷fW2Ö7F–öæw2÷và¦FW7Bç–ÖÆ‡v†–6‚—2v†B7W&f6VBF†—2–âF†Rf—'7BÆ6R’7F—2w&VVâà ¢22##bÓ‚Ó#B(	BFBf—6–&ÆRW'&÷"67&VVâf÷"6öçFVçBÖÆöBf–ÇW&RÂ–ç7FVBöb6–ÆVçB&Æ6²v–æF÷p ¢¢¤f–ÆW3¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öÆövv–ærç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷öwV’ç'6À¦÷'G2÷6W'f÷6†VÆÂöFW6·F÷ö†VFVE÷v–æF÷rç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóCb×f—6–&ÆRÖW'&÷"×67&VVâÖöâÖ6öçFVçBÖÆöBÖf–ÇW&RçF6†  ¢¢¥W7G&VÒ&V†f–÷#¢¢¢W7G&VÒ6W'fò†2g&öÔVÖ&VFFW$ÆövvW&ÖV6†æ—6Ò–çFVæFVBFğ¦f÷'v&B7&6‚÷v&æ–ærÖ6Æ72Æör&V6÷&G2FòâVÖ&VFFW"×6–FRT’(	BÇ&VG’F—6&ÆVB†W&R‡6VP§F†R##bÓ‚Ó2%7F'GWf–ÆRÆövv–ærÂ6ò6–ÆVçFÇ’Öf–Æ–ærÆ’æW†V—2F–væ÷6&ÆR"VçG'’À§F6‚3Òââæ’&V6W6RF†—2f÷&²†2æòFööÆ&"÷v&æ–ærT’Fòf÷'v&BFòBÆÂâF†—0¦VçG'’FG2æ'&÷rÂW'÷6RÖ'V–ÇB&WÆ6VÖVçBf÷"W†7FÇ’öæRf–ÇW&R6Æ72Â&F†W"F†à§&RÖVæ&Æ–ærF†BvVæW&ÂÖV6†æ—6Òà ¢¢¤Ö÷F—fF–öã¢¢¢F†Rf—‚&÷fR‡F6‚CR’6Æ÷6W2F†RÖ÷7B6öÖÖöâ6W6RÂ'WBç’¦÷F†W" §&V6öâ7&—F–6ÂÇ67&—Cæf–Ç2FòfWF6‚†vVçV–æVÇ’Ö—76–ærf–ÆRÂ&VÂ'Vr–â¦gWGW&R'VæFÆW"w2÷WGWBÂF—6²6÷''WF–öâÂâââ’v÷VÆB7F–ÆÂ&VB26–ÆVçB&Æ6²v–æF÷r(	@¦ÆöE7FGW3£¤6ö×ÆWFV7F–ÆÂf—&W2æ÷&ÖÆÇ’†æf–vF–öâ—G6VÆb6ö×ÆWFW2f–æRWfVâF†÷Vv‚F†P§vRw2÷vâ¥2æWfW"&â’Â6òF†R&ö÷B7Æ6‚6öÖW2F÷vâF†R6ÖR2ç’7V66W76gVÂÆöBÀ§&WfVÆ–ærvV%&VæFW"w2Æ–â&Æ6²FVfVÇB&6¶w&÷VæBv—F‚æò–æF–6F–öâç—F†–ærvVçBw&öærà¥&—6VBF—&V7FÇ’GW&–ær&Wf–WröbF6‚CS¢%&÷fW26†÷VÆB6öÖ×Væ–6FRF†RW'&÷"6öÖV†÷rÂ ¦æ÷B§W7BÆVfR—BF—66÷fW&&ÆRöæÇ’'’6öÖVöæRv†ò¶æ÷w2FòvòÆöö²B&÷fW2æÆövà ¢¢¤6†ævS¢¢  ¢Ò¢¦Æövv–ærç'6¢¢æ÷rw&2VçeöÆövvW&w2÷vâÆövvW&–â6ÖÆÂ&÷fW4ÆövvW&F†@¢FF—F–öæÆÇ’vF6†W2WfW'’Æör&V6÷&Bf÷"F†RW†7BGvòW7G&VÒ6ÆÂ6—FW2F†BÖVâ'F†P¢vRw2÷vâ67&—Bf–ÆVBFòÆöB"(	B67&—C£§67&—EöÖöGVÆVw2fWF6†–ærÖöGVÆR67&—@¢f–ÆVFæB67&—C£¦FöÓ£¦‡FÖÃ£¦‡FÖÇ67&—FVÆVÖVçFw2fWF6†–ær6Æ76–267&—Bf–ÆVF(	BæBÀ¢F†Rf—'7BF–ÖRV—F†W"f—&W2Â&V6÷&G2F†RÖW76vR–âæWr&ö6W72×v–FP¢4ôåDTåEôÄôEôU%$õ&6Æ÷B†V"†7&FR’fâ6öçFVçEöÆöEöW'&÷"‚–Fò&VB—B&6²’à¢FVÆ–&W&FVÇ’æ'&÷s¢â÷&F–æ'’vRCBÖ–æræöâÖ7&—F–6Â–ÖvR6†÷VÆFâwBF¶R÷fW"F†P¢v†öÆRv–æF÷rÂ'WBÇ67&—CæF†RvR—G6VÆbæVVFVBFò'Vâf–Æ–ær—2W†7FÇ’F†P¢'&VæFW'2æ÷F†–ærÂÆöö·2‡Vær"66Rv÷'F‚7W&f6–ærâWfW'—F†–ærVÇ6R&÷WBÆövv–ær†f–ÆP¢FW7F–æF–öâÂÆWfVÂf–ÇFW&–ærÂf÷&ÖGF–ær’—2Væ6†ævVB(	BF†—2öæÇ’FG26–FRVffV7Bà¢Ò¢¦wV’ç'6¢¢v–æVBwV“£§WFFUö6öçFVçEöÆöEöW'&÷&ÂæWr7FF–267&VVâ‡&WW6–ærF†R&ö÷@¢7Æ6‚w2–6öâö&Æ6²×æVÂ7G–Æ–ærf÷"f—7VÂ6öçF–çV—G’Âæò&öw&W72&"6–æ6Ræ÷F†–ær—0¢7F–ÆÂ–âfÆ–v‡B’6†÷v–ær%F†—2vÖRw26öçFVçBf–ÆVBFòÆöB"ÂF†R&V6÷&FVBÖW76vRÂæB¢ö–çFW"Fò&÷fW2æÆövà¢Ò¢¦†VFVE÷v–æF÷rç'6¢¢w2†æFÆU÷v–æ—E÷v–æF÷uöWfVçF&W–çB'&æ6‚æ÷r6†V6·0¢Æövv–æs£¦6öçFVçEöÆöEöW'&÷"‚–BF†RW†7Bö–çBF†BW6VBFòVæ6öæF—F–öæÆÇ’†æBöf`¢g&öÒF†R&ö÷B7Æ6‚FòF†R&VÂvR†öæ6R—5ö–æ—F–Å÷vUöÆöFVB‚–—2G'VR’(	B–bà¢W'&÷"v2&V6÷&FVBÂ—B–çG2F†RæWrW'&÷"67&VVâ–ç7FVB‡f–æWp¢†VFVEv–æF÷s£§–çEö6öçFVçEöÆöEöW'&÷&’æB¶VW2Fö–ær6òöâWfW'’7V'6WVVçB&W–çBÀ¢FVÆ–&W&FVÇ’7F–6·’f÷"F†R&W7BöbF†R6W76–öã¢F†R67&—BÇ&VG’f–ÆVBöæ6RÂF†W&Rw0¢æ÷F†–ærFò&WG'’à ¢¢¥fW&–f–6F–öã¢¢¢6ÖR2F6‚CR&÷fR(	B6ö×–ÆVB6ÆVâF‡&÷Vv‚6W'f÷6†VÆÆ—G6VÆbÀ¦&Æö6¶VBöæÇ’'’F†—2Ö6†–æRw2÷vâVç&VÆFVBÕ5d2ô”5RÆ–æ²W'&÷"â¢¥v†öWfW"'V–ÆG2F†—2æW‡@§6†÷VÆBFVÆ–&W&FVÇ’'&V²'VæFÆVBvÖRw26öçFVçB†RærâFVÆWFR÷&VæÖRöæR&VfW&Væ6VB67&—@¦gFW"'VæFÆ–ær’æB6öæf—&ÒF†RW'&÷"67&VVâV'2–ç7FVBöb&Æ6²v–æF÷rÂv—F‚§6Vç6–&ÆRÖW76vRâ¢  ¢ÒÒĞ ¢22##bÓ‚Ó#R(	B6fRÖvÖR7F÷&vR ¢¢¤f–ÆW3¢¢ ¢Ò÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2÷6fW2ç'6†æWrf–ÆR¢Ò÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2öÖöBç'6Â÷'G2÷6W'f÷6†VÆÂöFW6·F÷÷&÷Fö6öÇ2÷&÷fW2ç'6À¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öç'6‡v—&–ær¢Ò÷'G2÷6W'f÷6†VÆÂô6&vòçFöÖÆ†æWr&6ScFFWVæFVæ7’¢Ò—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†”å5DÄÄTEôÔ$´U& ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóCr×6fRÖvÖR×7F÷&vRÖ’çF6†  ¢¢¥v†C¢¢¢æWr6fW3¦7W7FöÒ&÷Fö6öÂ†Ö—'&÷&–ær&÷fW3¦ö7FVÓ¦w2W†—7F–ær&Æ&vRÀ§6W&FR7W&f6RvWG2—G2÷vâ66†VÖR"GFW&â(	B6VRF†÷6RVçG&–W2&÷fR’ÂW‡÷6–ærâ7–æ2À¦÷&–v–â×66÷VB¶W’÷fÇVR7F÷&RFòvV"6öçFVçBÂ6†VBÆ–¶R–æFW†VDD"–â7—&—B'WB&6¶VB'§&VÂf–ÆW2â6öÖÖæG3¢—5öf–Æ&ÆVÂw&—FVö&VF†6fRw2'—FW2Â&6ScBÖVæ6öFVB÷fW"F†P§VW'’7G&–ær(	B6ÖRG&ç7÷'B–F–öÒ&÷fW3¦ö7FVÓ¦Ç&VG’W6Rf÷"WfW'—F†–ærÂ6†÷6Vâ÷fW ¦fWF6‚‚–&WVW7B&öG’7V6–f–6ÆÇ’Fòfö–BæWE÷G&—G3£§&WVW7C£¥&WVW7D&öG–w2•0¦6‡Væ²Ö6†ææVÂÇVÖ&–ærÂv†–6‚v÷VÆB†fRæVVFVB&VÂ†&Gv&RFò—FW&FRöâ6fVÇ’æBF†—0¦Ö6†–æRw2÷vâ'&ö¶VâÆ–æ¶W"ÖFR–×÷76–&ÆRFòFW7BVæB×FòÖVæB’ÂFVÆWFVÂÆ—7FÂÚ±î¸Â¸­yêë¢°k¢G§¦*^µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^clear`.
`@drincs/roves-api/saves` (new module, `roves-api/src/saves.ts`) is the JS-facing wrapper â€”
see that package's own README.md.

**Where saves land** (`saves.rs`'s `resolve_saves_dir`) depends on how *this exact binary*
was shipped, which nothing previously exposed a way to detect at runtime:
- **Portable** (plain `mach bundle` output): a `saves/` folder next to the binary â€” on macOS
  specifically, next to the `.app` bundle itself (walking up past `Contents/MacOS/`), not
  inside it, since writing inside a bundle that's conventionally treated as a read-only,
  signed artifact (and sometimes literally is, mounted from a `.dmg`) is the wrong default.
- **Installed** (`--msi`/`--dmg`/`--deb`): under `roves_content_packer::extract::game_data_dir`
  (the OS cache dir, a sibling of the content-extraction cache and `roves.log`).
- New `INSTALLED_MARKER = ".roves-installed"` (`post_build_commands.py`) is the signal: an
  empty file written into the installer's staging directory â€” right next to wherever the
  binary itself ends up (`stage_dir` on Windows, `Contents/MacOS/` inside `play.app` on
  macOS, `pkg_root/usr/lib/<package_name>/` for `.deb`) â€” only when one of those three flags
  is set. `resolve_saves_dir` checks for this marker next to `std::env::current_exe()`; its
  absence means portable. If the portable location genuinely isn't writable (e.g. a zip
  extracted into `Program Files` without admin rights), falls back to the installed-style
  cache location rather than failing outright.
- `game_data_dir`'s own `game_name` argument is threaded from `App::packed_content_dest`'s
  grandparent directory (`app.rs`'s new registration code) â€” `None` for a launch with no
  packed-content boot extraction at all (a dev `--url` run, or a `--content-compress=none`
  bundle), which falls back to the generic, ungamed `game_data_dir(None)` bucket. Not ideal
  for two different loose-content games installed side by side on the same machine, but no
  worse than `roves.log`'s own existing, already-accepted limitation
  (`bundle_launch.rs`'s `peek_game_name_for_logging`) â€” not a new gap introduced here.

**Steam Cloud sync:** when compiled with `--features steam` and a Steam client is running,
every `write`/`delete` also mirrors to `ISteamRemoteStorage` (via the `steamworks` crate's
`Client::remote_storage().file(key)`, `.write()`/`.delete()`) under the same key as the local
file. A **separate** `Client::init()` call from `protocols/steam.rs`'s own â€” deliberate:
`steamworks::Client` is a cheap handle onto the already-running Steam client process, not a
second connection, and keeping the two protocol handlers independent avoids `app.rs` having
to thread a shared handle through a registration order that has no other dependency between
them today. Local disk stays the source of truth for reads â€” no conflict resolution to get
wrong â€” except a `read` for a key with no local file present but an existing Cloud copy pulls
that copy down first (`steam_try_pull`), so a fresh install on a second machine still sees
existing cloud saves. `list`/`clear` are local-only (don't enumerate Cloud-only files that
have never been read/pulled locally on this machine) â€” a known, documented gap, not an
oversight; see this file's own note in the wiki write-up for the same caveat surfaced to game
developers.

**Why:** requested as a first-class save-data story for games running under Roves â€” until
now nothing existed beyond ad-hoc use of the engine's own (upstream, unmodified) IndexedDB
implementation (`components/storage/`), which CUSTOMIZATIONS.md's `dom_indexeddb_enabled`
entry already flagged as a stop-gap, not the intended long-term path, once a real save API
existed. Also fills a real, adjacent gap: there was no runtime-detectable "is this page
actually running inside Roves" signal at all â€” `roves:is_available` (new command on the
existing `roves:` protocol, exposed as `@drincs/roves-api/core`'s `isAvailable()`) is a small,
independent addition alongside this feature, not specific to saves, but added here since it's
exactly what a game should check before calling into `saves` (or any other Roves-only API) at
all.

**Verification:** compiled clean through `servoshell` itself, including with `--features
steam` (exercising every `steamworks` API call this patch adds â€” `remote_storage()`,
`.file()`, `.write()`/`.read()`/`.delete()`/`.exists()`), blocked only by this same machine's
own unrelated MSVC/ICU link error (see patch 0045's entry). **None of the runtime behavior
described above â€” install-type detection, the portable/installed path split, or actual Steam
Cloud read/write/pull-on-miss â€” has been exercised on a real, linked binary.** Whoever builds
this next should: launch a portable build and confirm `saves/` appears next to it; build with
`--msi`/`--dmg`/`--deb`, confirm `.roves-installed` ends up next to the installed binary, and
that saves instead land under the OS cache dir; and, with `--features steam` and a real Steam
client running, confirm a save written on one machine actually appears under that app's Steam
Cloud files (Steamworks' own `steamctl`/the Steam client's own "Manage Game" â†’ cloud-save UI),
and that reading an unfetched key on a second machine pulls it down correctly.

---

## 2026-08-26 â€” Fix `file:` protocol handler blocking Workers as mixed content

**File:** `ports/servoshell/desktop/protocols/file.rs`, `impl ProtocolHandler for
FileProtocolHandler`.

**Patch:** `patches/servo-v0.4.0/0048-fix-file-protocol-mixed-content-blocking-workers.patch`

**Upstream/prior behavior:** `ProtocolHandler`'s own trait (`components/net/protocols/mod.rs`)
defaults both `is_fetchable()` and `is_secure()` to `false` â€” the latter's doc comment says
outright "this only works for bypassing mixed content checks right now". `FileProtocolHandler`
never overrode either, despite `roves.rs`/`steam.rs`/`saves.rs` (this fork's *other* three
custom protocol handlers) all overriding both to `true` since the day each was added.

**Change:** added both overrides, `true` in each case, to `FileProtocolHandler`.

**Why:** confirmed via a real game (built with the `pixi-vn-react-template`, shipped through
`roves-action`'s base mode) whose loading screen never advanced past "loading" â€” `roves.log`
showed `ERROR script::dom::workers::workerglobalscope] error loading script blob:.../...
(Blocked as mixed content)` for two separate Worker scripts, and nothing past that point ever
ran, since whatever the page's own code was waiting to hear back from those workers never
arrived. A Roves game's `file:` document *is* the app's own single, fully-trusted origin â€”
nothing else is ever loaded alongside it â€” so treating it as insecure/non-fetchable was never
intentional, just an oversight from the day `FileProtocolHandler` was first added (patch
0015): the other three handlers got both overrides from the start because their authors
happened to write `is_secure`/`is_fetchable` themselves; this one just copies stock Servo's own
upstream `file:` handling almost verbatim (see this file's own module doc comment) and
inherited the trait's defaults along with it, silently, without anyone noticing until a real
game exercised Workers.

**Verification:** compiled clean through `servoshell` itself, blocked only by this same
machine's own unrelated MSVC/ICU link error (see patch 0045's entry) â€” **not yet re-tested
against the actual failing game/build that surfaced this** (that reproduction lives on
whoever reported it, not on this machine). Whoever builds this next should re-run that exact
game's bundle and confirm the loading screen now reaches the main menu, with no more "Blocked
as mixed content" lines in `roves.log`.

---

## 2026-08-26 â€” Runtime + post-build game icon (replaces the `test-page/` compile-time hack)

**Files:**
- `ports/servoshell/build.rs`
- `ports/servoshell/desktop/headed_window.rs`
- `python/servo/post_build_commands.py` (`mach bundle --icon-png`/`--icon-ico`)

**Patch:** `patches/servo-v0.4.0/0049-runtime-post-build-game-icon.patch`

**Upstream/prior behavior:** `build.rs` looked for `test-page/public/icon.png`/`icon.ico`
*inside the engine checkout* at compile time, falling back to Roves' own
`resources/servo_64.png`/`servo.ico` only if that file didn't exist. `test-page/` is this
repo's own permanent, checked-in test fixture (see its own CUSTOMIZATIONS.md entries) â€” it
always exists in a full checkout, so the "fallback" branch never actually fired for a normal
build. `roves-action`'s `icon-png`/`icon-ico` inputs worked around this, in `advanced-mode`
only, by copying the consumer's file into that exact path before `mach build` compiled it in.

**Why this was a real, shipped bug, not just an edge case:** `.github/workflows/release.yml`
builds the *officially published* shell exactly this way â€” a plain compile, no icon override
â€” so every published Roves release has always shipped with `test-page`'s icon baked in
instead of Roves' own branding. Confirmed on a real game bundled through `roves-action`'s
base mode (which downloads that exact published shell): its `play.exe` showed `test-page`'s
icon, not Roves', despite the game never asking for `test-page`'s icon at all.

**Change:**
- **`build.rs`** now always uses `resources/servo_64.png`/`servo.ico` at compile time â€” no
  `test-page/` lookup, no per-game icon concept at compile time at all any more.
- **`headed_window.rs`** gained `runtime_window_icon_bytes()`: at every launch (Windows/Linux
  only â€” see below), checks for an `icon.png` next to the running binary
  (`std::env::current_exe().parent()`) *before* falling back to the compiled-in default from
  `build.rs`. This is the actual fix for base mode: a prebuilt shell is never compiled
  per-game, so only a runtime check can show a game's own icon without a custom compile.
- **`post_build_commands.py`**'s `bundle()` gained `--icon-png <path>` (copies the file next
  to the bundled binary as `icon.png` â€” Windows/Linux `stage_dir`/`output_dir`, macOS
  unsupported for now, `.deb`'s `lib_dir`, also referenced from the generated `.desktop`
  entry's new `Icon=` line) and `--icon-ico <path>` (Windows only: patches the already-staged
  `play.exe`'s own icon resource in place via `rcedit`, downloaded once and cached under
  `target/dependencies/rcedit/` â€” see new `_ensure_rcedit`/`_patch_windows_exe_icon`). Both
  apply identically whether the binary being bundled was just compiled (`advanced-mode`) or
  extracted from a prebuilt shell (base mode) â€” the whole point, since base mode is what
  every real `roves-action`/Packmaster consumer actually uses.
- **macOS is a known, deliberate gap, not an oversight**: its Dock/app icon comes from the
  `.app` bundle's own `Info.plist`/`.icns`, a completely different mechanism this repo has
  never had any code for (confirmed: no `.icns`/`CFBundleIconFile` reference anywhere in this
  tree before this patch either) â€” scoped out rather than attempting a blind, untested icns
  generation/embedding feature with no way to verify it on this (non-macOS) machine. `mach
  bundle --icon-png` on macOS prints a warning and ignores the input rather than silently
  doing nothing or failing the run.

**Why not fix this purely in `roves-action`:** the old workaround only worked in
`advanced-mode` (a real compile) â€” base mode downloads an *already-built* shell, so there was
never a file for `roves-action` to copy anything into before compilation, because no
compilation happens. The fix had to move the icon mechanism from compile-time to
runtime/post-build, which only the engine itself can do.

**Verification:** `post_build_commands.py`'s changes syntax-checked (`ast.parse`, via WSL
Python since this machine's own `python3` is a non-functional Windows Store alias) â€” **not
run**, no real `mach bundle` invocation attempted (would need a real build first, blocked by
this machine's own unrelated linker gap â€” see patch 0045's entry). The Rust side compiled
clean through `servoshell` itself. **None of the following has been exercised for real**:
`runtime_window_icon_bytes()` actually finding and loading a real `icon.png`; `--icon-png`
actually landing at the right path for each of Windows/Linux-portable/`.deb`; `rcedit`
actually downloading and successfully patching a real `play.exe`'s icon (this machine has
never invoked `rcedit` at all); the `.desktop` entry's new `Icon=` line actually showing the
right icon in a real Linux app launcher. Whoever builds this next should verify all of the
above against a real game, on Windows and Linux at minimum.

---

## 2026-08-27 â€” Fix: `blob:` Worker scripts still blocked as mixed content after patch 0048

**File:** `components/net/fetch/methods.rs`, `should_request_be_blocked_as_mixed_content`,
`should_response_be_blocked_as_mixed_content` (new helper `is_request_url_potentially_
trustworthy` added alongside them).

**Patch:** `patches/servo-v0.4.0/0050-fix-blob-worker-mixed-content-false-block.patch`

**This is a follow-up to patch 0048, which did not actually fix the problem it targeted.**
0048's own "Verification" section flagged this explicitly: it wasn't re-tested against the
real failing build. A fresh `roves.log` from that same `pixi-vn-react-template` build (now
running the released engine v0.4.0, with 0048 in it) showed the *identical* `error loading
script blob:null/... (Blocked as mixed content)` lines, at two different Worker scripts, in
the same place the loading screen hangs â€” proving `FileProtocolHandler::is_secure()`/
`is_fetchable()` alone didn't resolve it.

**Root cause (the actual one):** mixed-content blocking runs in two steps â€”
`do_settings_prohibit_mixed_security_contexts` first asks "is the *requesting* origin (the
page that spawned the Worker) itself potentially trustworthy?", and only if yes does it go
on to check "is the *target* URL (the Worker's own script URL) also potentially
trustworthy?". Patch 0045 (`ImmutableOrigin::new_opaque_for_file`) already made this fork's
`file://` origin answer "yes" to the first question â€” correctly, that's the whole point of
that patch. But that flips mixed-content checking **on** for `file://` pages for the first
time (before 0045, `file://` was never trustworthy, so `do_settings_prohibit_mixed_security_
contexts` always short-circuited to "not prohibited" and this second check never ran at
all â€” which is why nobody had hit this before). The second question is answered by
`is_url_potentially_trustworthy`, which for a `blob:` URL falls through to
`ImmutableOrigin::new`/`url::Url::origin()` â€” a *generic*, text-based re-derivation that
cannot recover an opaque creator origin, because opaque origins serialize to the literal
string `"null"` (visible directly in the failing log: `blob:null/<uuid>`) and re-parsing
`"null"` as a URL always fails, falling back to a brand-new, unrelated (and therefore
untrustworthy) opaque origin. So *any* Worker spawned via a `blob:` URL from this fork's
`file://` page fails the second check even though the page itself passed the first â€” a
real, pre-existing Servo-wide gap in `is_url_potentially_trustworthy` that stayed invisible
until 0045 made an opaque origin trustworthy for the first time. (Bundlers like Vite/webpack
ship Web/module workers as `new Worker(URL.createObjectURL(workerScriptBlob))`, so this
class of app hits it directly.)

**Change:** added `is_request_url_potentially_trustworthy(request, protocol_registry)`,
used in place of the raw `is_url_potentially_trustworthy(protocol_registry,
&request.current_url())` call in both functions above. For a `blob:` URL specifically, it
consults `request.current_url_with_blob_claim().origin()` instead â€” the same real, tracked
creator-origin lookup (`BlobToken::origin`, set from `BlobResolver::origin` at the point the
blob was claimed, see `components/shared/net/blob_url_store.rs`) that `BlobProtocolHander`
itself already relies on to authorize the fetch in the first place. Every other scheme's
behavior is completely unchanged â€” this only replaces the *derivation* of a blob: URL's
origin with the one the engine already tracks correctly elsewhere, it doesn't change how
trustworthiness is decided once an origin is known.

**Why not fix `is_url_potentially_trustworthy` itself:** that function only takes a raw
`&ServoUrl`, with no access to the owning `Request` (and therefore no way to reach its blob
claim/token) â€” widening its signature would touch every one of its several other call sites
in this file for no benefit, since none of the others deal with `blob:` requests carrying a
live claim. Scoping the fix to a new, blob-aware wrapper used only at the two call sites that
actually gate `NetworkError::MixedContent` is the smaller, more targeted change.

**`should_upgrade_mixed_content_request` was deliberately left untouched:** it has the same
theoretical blob: false-positive, but its only effect is scheme-swapping `http`â†’`https` or
`ws`â†’`wss` on the request; for a `blob:` scheme this swap is already a no-op (see the match
arm's `_ => None`), so a wrong "should upgrade" verdict there causes no observable behavior
change. Not worth the risk of touching a third call site for zero effect.

**Verification:** `cargo check -p servo-net` (this machine's usual environment gap â€” MSVC
`vcvars64.bat` + `LLVM\bin` on `PATH` for `lld-link.exe` â€” plus a *new* one hit for the first
time on this exact crate: `stylo`'s own `build.rs` shells out to a `python.exe` on `PATH`,
which resolves to the non-functional Windows Store alias by default; fixed for this check by
also prepending this repo's own `.venv\Scripts` on `PATH`, which has a real interpreter)
compiled clean, zero warnings, in isolation. **Not yet re-tested against the actual failing
game/build** â€” same caveat 0048's own entry carried, now doubly important since 0048 alone
was already shown, by real testing, not to be sufficient. Whoever builds this next should
re-run `pixi-vn-react-template`'s bundle and confirm both that the loading screen now reaches
the main menu *and* that `roves.log` has no more "Blocked as mixed content" lines at all.

---

## 2026-08-27 â€” Auto-detect `icon.png`/`icon.ico` in `--content-dir` when neither icon flag is given

**File:** `python/servo/post_build_commands.py`, `bundle()`.

**Patch:** `patches/servo-v0.4.0/0051-icon-auto-detect-from-content-dir.patch`

**Change:** before `bundle()`'s existing `--icon-png`/`--icon-ico` handling (patch 0049) runs,
if neither flag was passed and `--content-dir` was, look for `icon.png`/`icon.ico` sitting
directly in that content directory and use it as the default. An explicitly passed
`--icon-png`/`--icon-ico` still always wins â€” this only fills in when neither was given.

**Why:** many web bundlers (confirmed: `pixi-vn-react-template`'s own `dist/`) already emit
an `icon.png` at the content root for their own PWA manifest. Without this, a game with one
still shipped with Roves' own default branding unless the game dev *also* remembered to pass
`--icon-png` pointing at the exact same file â€” redundant, easy to forget, and silently wrong
by default even though the right image was sitting right there the whole time.

**Same change made identically in `roves-packmaster`** (`src-tauri/src/bundle.rs`'s `apply_icon`) â€”
see that project's own commit/CLAUDE.md; both sides mirror `mach bundle`'s icon behavior on
purpose (see patch 0049's own entry), so this default had to land in both to stay consistent.

**Verification:** syntax-checked (`ast.parse`, via WSL Python) and patch-applies-cleanly
verified against the post-0050 committed tree. **Not run** â€” no real `mach bundle`
invocation attempted, same linker-gap caveat as every other Python-side change this session
(see patch 0045's entry). Whoever builds this next should verify a `--content-dir` containing
an `icon.png` (no explicit `--icon-png`) actually produces a bundle with that icon, on
Windows and Linux at minimum, and that an explicit `--icon-png` still overrides it.

---

## 2026-08-27 â€” Icon auto-detect: fall back to `favicon.ico` when `icon.ico` is absent

**File:** `python/servo/post_build_commands.py`, `bundle()` (extends the previous entry's
auto-detect block).

**Patch:** `patches/servo-v0.4.0/0052-icon-ico-fallback-to-favicon-ico.patch`

**Why:** confirmed via a real game (`pixi-vn-react-template`, through `roves-action`'s "test"
release): its `dist/icon.png` was correctly auto-detected (verified present in the actual
downloaded bundle, right next to `play.exe`), but its `.exe` file's own icon stayed Roves'
default â€” because that template's `public/` has `favicon.ico`, not `icon.ico`, and the
previous entry's auto-detect only ever looked for the latter. `favicon.ico` is what virtually
every bundler actually emits by default (Vite's own starter templates included); `icon.ico`
specifically is comparatively rare. Since `favicon.ico` is already a real, valid (often
multi-size) `.ico` file, there's no format reason not to let `rcedit` patch it in directly.

**Change:** if `icon.ico` isn't found in `--content-dir` (and no explicit `--icon-ico` was
given), also try `favicon.ico` there before falling back to Roves' own branding. Same-name
`icon.png`/`icon.ico` detection (previous entry) still tried first â€” this is one more
fallback step, not a replacement.

**Same change made identically in `roves-packmaster`** (`src-tauri/src/bundle.rs`'s `apply_icon`) â€”
both sides must keep mirroring `mach bundle`'s icon behavior, same as every other entry in
this icon feature's history.

**Verification:** syntax-checked (`ast.parse`, via WSL Python) and patch-applies-cleanly
verified against the post-0051 committed tree. **Not run** â€” no real `mach bundle`
invocation attempted, same linker-gap caveat as every other Python-side change this session.
`roves-packmaster`'s Rust side compiled clean (`cargo check`, only the same pre-existing unrelated
`selected` dead-code warning). Whoever builds this next should verify a `--content-dir`
containing only a `favicon.ico` (no `icon.ico`) actually patches the bundled `play.exe`'s
icon resource.

---

## 2026-08-27 â€” Virtual content root (`game:` protocol)

**Files:**
- `components/url/origin.rs` (new `game://` opaque-origin support, mirroring `file://`'s)
- `components/servo/lib.rs` (`protocol_handler` facade gains a `Destination` re-export)
- `ports/servoshell/desktop/protocols/mod.rs` (new module declarations)
- `ports/servoshell/desktop/protocols/packed_content.rs` (new file â€” see below)
- `ports/servoshell/desktop/protocols/file.rs` (refactored to use the new shared module)
- `ports/servoshell/desktop/protocols/game.rs` (new file â€” the handler itself)
- `ports/servoshell/parser.rs` (`get_default_url` accepts a `game:` URL)
- `ports/servoshell/desktop/bundle_launch.rs` (constructs `game://content/...` URLs)
- `ports/servoshell/desktop/cli.rs`, `ports/servoshell/desktop/app.rs` (wiring)
- `components/script/dom/window/history.rs` (`pushState`/`replaceState` same-origin rule)

**Patch:** `patches/servo-v0.4.0/0053-virtual-content-root-game-protocol.patch`

**Why:** confirmed via a real game (`pixi-vn-react-template`, whose loading-screen hang was
already fixed by patches 0050/0051/0052) that once it actually renders, it immediately shows
its own client-side router's ("TanStack Router", default browser `history`) built-in "Not
Found" page instead of the real app. Root cause: a bundled launch opens a raw
`file:///C:/Users/.../dist/index.html` URL â€” `window.location.pathname` at boot is the real,
absolute OS path, not a root-relative one, so no router's own route table (`/`, `/settings`,
...) can ever match against it. This isn't fixable by rebasing asset *references* the way
patch 0045 already does for `<script src="/...">` â€” a router reads `location.pathname`
directly, there is no request to rebase.

**The fix, in one line:** serve bundled content under a fixed virtual origin instead of a raw
`file:` path â€” `game://content/` (the virtual root, not `game://content/index.html`) instead
of the real disk path â€” the same idea Tauri itself already uses (`tauri://localhost/` /
`https://tauri.localhost/`) to avoid this exact class of problem, not something novel to this
fork. `window.location.pathname` at boot is then simply `/`, matching what every router
already expects, and `pushState("/about")` stays same-origin (same scheme + same fixed host,
"content"), so the History API allows it.

**2026-08-29 correction â€” boot was still opening `/index.html`, not `/`:** the first cut of
this patch built the boot URL as `game://content/<entry_html>` (i.e.
`game://content/index.html`), reasoning that `GameProtocolHandler`'s own SPA fallback would
serve the entry HTML's bytes either way. It does â€” but `window.location.pathname` at boot was
then literally `/index.html`, and a router matches its root route against `/`, never against
a hard-coded `/index.html` (the same reason it would never match a hard-coded `/about.html`).
So the original bug's *symptom* survived the *fix* unchanged: every bundled launch still
opened straight onto the router's own "Not Found" page, just one level of indirection removed
from the original diagnosis. This was missed by the initial verification below because a
*root-level* route loader (data preloading, asset prefetching, `Game.onLoadingLabel`'s
background bundle load, ...) still runs even when no *leaf* route matches â€” so `roves.log`
showed exactly the network activity (main menu image, audio) a working app would produce,
while the actual visible page â€” confirmed by an actual screenshot of a running build, not just
log-reading â€” was nothing but unstyled "Not Found" text top-left of an otherwise empty canvas,
the entire time. Fixed by having `bundle_launch.rs`'s `game_content_url` build `game://content/`
outright instead of joining `entry_html` onto it â€” `GameProtocolHandler::load`'s existing SPA
fallback already serves the entry HTML for that request regardless (`content_root` itself is a
directory, so "no matching file" is just as true for `/` as it was for a made-up sub-route),
so this only changes what `location.pathname` reads as, not what bytes get served.

**What changed, piece by piece:**

- **`ImmutableOrigin::new_opaque_for_game_content`** (`origin.rs`): a `game://` document gets
  a fixed, shared opaque origin (`GAME_ORIGIN_ID`, distinct from `file://`'s own
  `FILE_ORIGIN_ID`) â€” same reasoning as `new_opaque_for_file`, duplicated rather than merged
  into one generalized concept (see that new constructor's own doc comment for why: this
  fork's own stated preference for small, targeted, non-abstracted changes over premature
  refactors). `is_potentially_trustworthy()` and `can_access_storage()` both extended
  identically to `is_file_origin`'s existing carve-outs, so mixed-content checks and
  `localStorage`/`indexedDB` work the same under `game://` as they already do under `file://`.
  A **tuple** origin (`game`, `content`, 0) was considered instead â€” `game://` URLs, unlike
  `file://`, do have a real, meaningful host â€” but would have meant auditing every other
  origin-consuming code path in this engine that currently assumes a tuple origin only ever
  comes from `http(s)`/`ws(s)`; reusing the already-proven opaque-with-a-fixed-id pattern is
  the smaller, lower-risk change for the one thing this fork actually needs (a second
  first-party trusted local content origin, structurally identical to what `file://` already
  is here).
- **`protocols/packed_content.rs`** (new): `file.rs`'s on-demand pack-extraction logic
  (`PackedContent`, `ensure_available`) extracted out verbatim â€” `game.rs` needs the exact
  same behavior (both handlers serve the same on-disk bundled content, just addressed
  differently), and duplicating a mutex-guarded extraction routine across two files is a real
  maintenance hazard (a bug fixed in one copy, forgotten in the other) a real engineer
  wouldn't accept either. `file.rs` itself now just delegates to it â€” no behavior change
  there, confirmed by diffing its own `load()` logic, which is untouched.
- **`protocols/game.rs`** (new): `GameProtocolHandler` â€” resolves `game://content/<path>`
  against a `content_root` given at construction, reusing `PackedContent` for lazy
  extraction. Rejects any host other than the fixed `"content"` outright (nothing but this
  engine's own `bundle_launch.rs` ever constructs a `game:` URL, so anything else reaching
  this handler is a bug elsewhere). **SPA fallback**: a `Destination::Document` request (a
  real navigation â€” a hard reload on `/about`, or `location.href = "/about"`; deliberately
  *not* triggered by a router's own in-app `pushState`, which makes no network request at
  all) whose path doesn't resolve to a real file serves the bundle's entry HTML instead of a
  404 â€” the same rule any static host serving a history-mode SPA needs (nginx's own
  `try_files`, Vite dev server's `historyApiFallback`, ...). Deliberately gated to
  `Destination::Document` only: a genuinely missing *asset* (an image, a script) still
  surfaces as a real error, not silently becomes a page of HTML.
- **`history.rs`'s `can_have_url_rewritten`**: the actual spec algorithm `pushState`/
  `replaceState` call (throws `SecurityError` on failure) â€” Step 3 already special-cases
  `http`/`https` to allow rewriting to *any* same-origin path/query freely; Step 4's `file:`
  carve-out is far narrower (only allows rewrites that leave the *path* completely
  unchanged, since a `file:` URL's "path" is a real OS path, not a route); anything else
  (Step 5) requires path *and* query to stay identical. Without adding `game` to Step 3, a
  router's own `pushState("/about")` call â€” the entire point of this feature â€” would throw
  `SecurityError` on every in-app navigation, even after every other piece above was
  correct. `game://` URLs have real host+path structure exactly like m«ëŒ+Š×®º+º$zzb¥ëZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æ‡GG‡2–FöW2…7FW ¢Ç&VG’&WV—&W2†÷7B÷÷'B÷66†VÖRFòÖF6‚’Â6òG&VF–ær—B–FVçF–6ÆÇ’Fò‡GG‡2–†W&P¢—2F†R6Æ÷6W7B×Fò×7V2Ö–çFVçB6†ö–6RÂæ÷BÖVæ–ævgVÂ7V2f–öÆF–öâà¢Ò¢¦'VæFÆUöÆVæ6‚ç'6¢£¢&W6öÇfUö'VæFÆVEöÆVæ6…ö&w6æ÷r6öç7G'V7G2F†Rf—†V@¢vÖS¢òö6öçFVçBöf—'GVÂ×&ö÷BU$Â‡f–vÖUö6öçFVçE÷W&Æ’–ç7FVBöb&VÂ'6öÇWFP¢f–ÆS¦F‚Âf÷"¦ç’¢'VæFÆVBÆVæ6‚(	B6¶VB†6öçFVçEöF—&–âÆVæ6‚æ§6öæ’÷"Æö÷6P¢†W&ÆöæÇ’ÂÒÖ6öçFVçBÖ6ö×&W73ÖæöæV’âFVÆ–&W&FVÇ’F†R&&R&ö÷BÂæ÷@¢vÖS¢òö6öçFVçBóÆVçG'•ö‡FÖÃæ(	B6VRF†R##bÓ‚Ó#’6÷'&V7F–öâ&÷fRf÷"v‡’¦ö–æ–æp¢VçG'•ö‡FÖÆöçFòF†R&ö÷BU$Â6–ÆVçFÇ’FVfVFVBF†R&÷WFW"ÖÖF6†–ærf—‚F†—2F6‚W†—7G0¢f÷"à¢'VæFÆVDÆVæ6†v–æVBvÖUö6öçFVçC¢÷F–öãÂ…F„'VbÂ7G&–ær“æ‡F†R&VÂöâÖF—6²6öçFVç@¢&ö÷B²VçG'’Ö‡FÖÂ×&VÆF—fR×F‚vÖU&÷Fö6öÄ†æFÆW&æVVG2’F‡&VFVBF‡&÷Vv‚6Æ’ç'6(i ¢£¦æWv(i"ç'6w2æWrvÖS¢òö&Vv—7G&F–öâÂÆöæw6–FRF†RW†—7F–æp¢VæF–æuö&ö÷EöW‡G&7F–öæF‡&VF–ærâF†RÆö÷6RÖ'VæFÆR66Rw26öçFVçB&ö÷B—27F–ÆÂ77VÖV@¢Fò&RF†RVçG'’…DÔÂw2÷vâ&VçBF—&V7F÷'’(	BF†RW†7B6ÖR&RÖW†—7F–ær77V×F–öà¢f–ÆRç'6w2–æ—F–ÅöF—&Ç&VG’ÖFS²æ÷BæWrÆ–Ö—FF–öâ–çG&öGV6VB†W&Rà¢Ò¢¦'6W"ç'6w2vWEöFVfVÇE÷W&Æ¢£¢v–æVB‚&vÖR"Â6öÖR…ò’Âò–ÖF6‚&Ò(	BF†P¢W†—7F–ærÖF6‚öæÇ’WfW"66WFVBf–ÆS¦U$Âv†÷6RF&vWBÇ&VG’W†—7G2öâF—6²Â÷"¢&&RFöÖ–âÖÆ–¶R7G&–ær&Ww&—GFVâFò‡GG¢òö²vÖS¢òö6öçFVçBòââæ7G&–ær‡v†–6‚—6âw@¢&VÂf–ÆW7—7FVÒF‚ÂæBÇv—2–çFW&æÆÇ’6öç7G'V7FVBÂæWfW"W6W"×G—VB’æVVFVB—G2÷và¢&ÒFòæ÷B&R6–ÆVçFÇ’F—66&FVB&6²FòF†R†öÖWvRö&Ææ²×vRfÆÆ&6²à¢Ò¢¥v†B§v6âwB¢6†ævVB¢£¢Æ–âÒ×W&ÆöG&rÖæBÖG&÷FWb×F–ÖRÆVæ6†W2¶VWW6–æp¢f–ÆS¦W†7FÇ’2&Vf÷&R(	BöæÇ’F†RÆVæ6‚æ§6öæÖG&—fVâF‚‡v†BWfW'’&VÀ¢Ö6‚'VæFÆVõ6¶Ö7FW"ö&÷fW2Ö7F–öæ6öç7VÖW"7GVÆÇ’6†—2’Ö÷fVBFòvÖS¦âGvğ¢÷F†W"66†VÖR‚’ÓÒ&f–ÆR&6†V6·2f÷VæBv†–ÆRVF—F–ærf÷"F†—2†Æö6F–öâç'6ğ¢‡FÖÆ‡—W&Æ–æ¶VÆVÖVçGWF–Ç2ç'6w2ç÷'F6WGFW'2Â&÷F‚æ'&÷r&U$ÂF†B6ææ÷B†fR¢÷'B"V—&·2v—F‚æò&V&–æröâ&÷WF–ær’vW&RFVÆ–&W&FVÇ’ÆVgBÆöæR(	Bæ÷BF†—2f÷&²w0¢6öæ6W&âÂæBF÷V6†–ærF†VÒv÷VÆB&R66÷R7&VWv—F‚æò&VÂ&VæVf—Bà ¢¢¥fW&–f–6F–öã¢¢¢6&vò6†V6²×6W'f÷6†VÆÆ†'&öFW7B66÷R6†V6¶VB–WBf÷"6–ævÆRF6€§F†—26W76–öâÂ6–æ6RF†—2F÷V6†W26ö×öæVçG2÷W&ÆÂ6ö×öæVçG2÷6W'föÂ6ö×öæVçG2÷67&—FÀ¦æB÷'G2÷6W'f÷6†VÆÆFövWF†W"’6ö×–ÆVB6ÆVâÂ&÷F‚f÷"F†R÷&–v–æÂ7WBöbF†—2F6‚æ@¦v–âgFW"F†R##bÓ‚Ó#’6÷'&V7F–öâ&÷fRâ¢¥F†R÷&–v–æÂ7WBw2&6öæf—&ÖVBöâ&VÂÀ§'Vææ–ær&–æ'’"6Æ–Ò‡6†—VB2Væv–æRcãBã"’GW&æVB÷WBFò&RfÇ6RæVvF—fR¢£¢—Bv0¦&6VBöâ&VF–ær&÷fW2æÆöv†ÆVæ6‚&w2ÂæWGv÷&²&WVW7G2ÆÂ&W6VçBæB7V66W76gVÂ’'W@¦æWfW"7GVÆÇ’Æöö¶–ærBF†R&VæFW&VBv–æF÷r(	BæBÂW"F†R6÷'&V7F–öâ&÷fRÂF†Rv–æF÷rv0§6†÷v–æræ÷F†–ær'WB$æ÷Bf÷VæB"F†Rv†öÆRF–ÖRFW7—FRF†BÆör7F—f—G’â¢¥&RÖ6öæf—&ÖV@£##bÓ‚Ó#’ÂF†—2F–ÖR'’67&VVç6†÷GF–ærâ7GVÂ'Vææ–ær'V–ÆB¢¢†Æö6ÆÇ’Ö'V–Ç@¦Æ’æW†VÂÆVæ6†VBg&W6‚v–ç7B—†’×fâ×&V7B×FV×ÆFVw2÷vâ6ö×–ÆVB÷WGWB’gFW ¦Ç––ærF†R&ö÷BÕU$Â6÷'&V7F–öã¢F†RvÖR&VæFW&VB7B&ö÷B–çFò&VÂvÖWÆ’–ç7FVBö`§F†R&÷WFW"w2$æ÷Bf÷VæB"fÆÆ&6²âÆW76öâf÷"gWGW&RfW&–f–6F–öâöbç—F†–ærT’×f—6–&ÆR–à§F†—2f÷&³¢&VF–ær&÷fW2æÆöv&÷fW2F†R¦Væv–æR¢F–Bv†Bv26¶VC²—BFöW2æ÷B&÷fRF†P¢§vR¢&VæFW&VBç—F†–ær6Vç6–&ÆR(	B67&VVç6†÷B÷"÷F†W'v—6RF—&V7FÇ’ö'6W'fRF†Rv–æF÷r&Vf÷&P¦6ÆÆ–ær&VæFW&–ærÖffV7F–ærf—‚6öæf—&ÖVBâæ÷B–WB6W&FVÇ’6öæf—&ÖVC¢†&BÖæf–vFRğ§&VÆöBF—&V7FÇ’Fò7V"×&÷WFR†W†W&6—6W2F†R5fÆÆ&6²7V6–f–6ÆÇ’Âæ÷BW†W&6—6VB'’F†—0§'F–7VÆ"vÖRw2÷vâæ÷&ÖÂfÆ÷r’(	BÆ÷vW"×&–÷&—G’vF†âF†R÷&–v–æÂ'VrÂ6–æ6R&÷fW0¦†2æòFG&W72&"f÷"Æ–W"FòG—R7V"×&÷WFRU$Â–çFò–âF†Rf—'7BÆ6Rà ¢ÒÒĞ ¢22##bÓ‚Ó#r(	B”D$–æFW†7W'6÷"7W÷'B†6Æ–VçB×6–FR ¢¢¤f–ÆW3¢¢ ¢Ò6ö×öæVçG2÷67&—Eö&–æF–æw2÷vV&–FÇ2ô”D$–æFW‚çvV&–FÆ‡Væ6öÖÖVçG2÷Vä7W'6÷&ö÷Vä¶W”7W'6÷&¢Ò6ö×öæVçG2÷67&—Eö&–æF–æw2ö6öFVvVâô&–æF–æw2æ6öæf†FG2&÷F‚Fò”D$–æFW†w27†Æ—7B¢Ò6ö×öæVçG2÷67&—BöFöÒö–æFW†VFF"ö–F&–æFW‚ç'6†æWr÷Våö7W'6÷&ö÷Vä7W'6÷&ö÷Vä¶W”7W'6÷&¢Ò6ö×öæVçG2÷67&—BöFöÒö–æFW†VFF"ö–F&7W'6÷"ç'6†æWr&V6÷&G5öf÷%ö–æFW…ö7W'6÷&Â6÷W&6R‚– ¢66W76÷"ÂG&÷2F†Ræ÷rÖ–æ67W&FR5¶W‡V7B‡VçW6VB•Ööâö&¦V7E7F÷&T÷$–æFW†¢Ò6ö×öæVçG2÷67&—BöFöÒö–æFW†VFF"ö–F'&WVW7Bç'6†6ÆÇ2F†R&÷fR&Vf÷&R—FW&FUö7W'6÷&¢Ò6ö×öæVçG2÷67&—BöFöÒö–æFW†VFF"ö–F&ö&¦V7G7F÷&Rç'6‡v–FVç2fW&–g•öæ÷EöFVÆWFVFğ¢6†V6µ÷G&ç67F–öåö7F—fVFòV"†7&FR–6ò”D$–æFW†6â&WW6RF†VÒ ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóSBÖ–F&–æFW‚Ö7W'6÷"×7W÷'BÖ6Æ–VçB×6–FRçF6†  ¢¢¥v‡“¢¢¢F—66÷fW&VBf–F†R6ÖR&VÂvÖRö'V–ÆBF†B7W&f6VBF†RvÖS¦&÷Fö6öÂ'Vr&÷fP®(	Böæ6RF†Bf—‚ÆWB—†’×fâ×&V7B×FV×ÆFV7GVÆÇ’&VæFW"Â—G2æ'&F–öâÖ†—7F÷'’fVGW&P¢†G&–æ72÷—†’×fæ’7&6†VBv—F‚2æ–æFW‚‚âââ’æ÷Vä7W'6÷"—2æ÷BgVæ7F–öæâ”D$–æFW‚çvV&–FÆ ¦†B÷Vä7W'6÷&ö÷Vä¶W”7W'6÷&†æBvWFövWD¶W–övWDÆÆövWDÆÄ¶W—6ö6÷VçFğ¦vWDÆÅ&V6÷&G6’6öÖÖVçFVB÷WB(	BæWfW"W‡÷6VBFò¥2BÆÂ–âF†—26W'fòfW'6–öâÂW7G&VÒÀ¦æ÷B6öÖWF†–ærF†—2f÷&²'&ö¶Râ”D$ö&¦V7E7F÷&VÇ&VG’†2v÷&¶–ær7W'6÷'3²”D$–æFW†F–Fâw@¦W‡÷6Rç’VW'’7W&f6R&W–öæB—G2f÷W"&6–2&÷W'G’vWGFW'2à ¢¢¥v‡’öæÇ’÷Vä7W'6÷&ö÷Vä¶W”7W'6÷&Âæ÷BvWFövWD¶W–övWDÆÆövWDÆÄ¶W—6ö6÷VçFFöó¢¢ §F†÷6RvW&RF†RGvòÖWF†öG27GVÆÇ’7&6†–ærF†R&VÂvÖR(	BF†R÷F†W'2&VÖ–â6öÖÖVçFVB÷W@¦–âF†RçvV&–FÆÂ¶æ÷vâÂæ'&÷vW"föÆÆ÷r×WvÂæ÷B6–ÆVçFÇ’f÷&v÷GFVâà ¢¢¥F†R&VÂ6ö×Æ–6F–öâ(	BF†R7F÷&vR&6¶VæB†2æò6öæ6WBöb–æFW†W2BÆÃ¢¢ ¦”D%&WVW7C£¦W†V7WFUö7–æ6w2ÖW76vRFòF†R7F÷&vRF‡&VB6'&–W2öæÇ’F†R¦ö&¦V7B7F÷&Rw2 ¦æÖRæB¶W•÷&ævV†6öæf—&ÖVB'’&VF–ær6ö×öæVçG2÷7F÷&vRö–æFW†VFF"öVæv–æW2÷7Æ—FRç'6w0¦—FW&FV†æFÆW"F—&V7FÇ’(	B6VÆc£¦vWEöÆÅ÷&V6÷&G2‚f6öææV7F–öâÂö&¦V7E÷7F÷&RÂ¶W•÷&ævR–Âæğ¦–æFW‚&ÖWFW"ç—v†W&R’â–F&7W'6÷"ç'6w2ö&¦V7E7F÷&T÷$–æFWƒ£¤–æFW†f&–çBÇ&VG’W†—7FV@¦æB—FW&FUö7W'6÷&w2÷vâæW‡B÷&WböæW‡GVæ—VR÷&WgVæ—VRÆöv–2v2Ç&VG’gVÆÇ§7V2Ö6ö×Æ–çBf÷"â–æFW‚6÷W&6R‡W"—G2÷vâF÷ÖöbÖgVæ7F–öâæ÷FR’(	B'WBæ÷F†–ærWfW ¢¦6öç7G'V7FVB¢â–æFW‚×6÷W&6VB7W'6÷"Â6òF†BÆöv–2v2FVB6öFR–â&7F–6R††Væ6RF†P¦5¶W‡V7B‡VçW6VB•ÖF†—2F6‚&VÖ÷fW2’(	B6W'fò×6–FRW‡FVç6–öâöbF†R7W'6÷"DôÒÖöFVÂF†@¦æWfW"v÷B6öææV7FVBÆÂF†Rv’F÷vâFò&VÂ&6¶VæBà ¢¢¥Gvòv—2Fò6Æ÷6RF†BvvW&R6öç6–FW&VB¢¢‡6VRF†R6W76–öâw2÷vâFW6–vâF—67W76–öâ“¢ƒ¦W‡FVæBF†R7F÷&vR&÷Fö6öÂö&6¶VæBFòvVçV–æVÇ’VW'’'’–æFW‚Â÷"ƒ"’¶VWF†R&6¶Væ@¦W†7FÇ’2Ö—2æB&RÖFW&—fRV6‚&V6÷&Bw2&VÂ–æFW‚¶W’¢¦6Æ–VçB×6–FR¢¢ÂfVVF–æp¦—FW&FUö7W'6÷&w2Ç&VG’Ö6÷'&V7BÆöv–2â¢¢ƒ"’v26†÷6Vâ¢¢(	BFVÆ–&W&FVÇ’Âæ÷Bf÷"Æ6²ö`§VæFW'7FæF–ærƒ’w2FW6–vã¢W‡FVæF–ær7F÷&vU÷G&—G6÷F†R5Æ—FR&6¶VæB—2&VÀ¦FF&6RÖÆ–W"v÷&²F†—2f÷&²6âwBfW&–g’VæB×FòÖVæBöâF†—2Ö6†–æR†æòv÷&¶–ærÆ–æ²’Âv†–ÆP¢ƒ"’&WW6W2Ö6†–æW'’F†Bw2Ç&VG’&÷fVâ6÷'&V7BæB¶VW2F†R6†ævRVçF—&VÇ’v—F†–à¦6ö×öæVçG2÷67&—Fà ¢¢¤†÷rF†R6Æ–VçB×6–FR&RÖFW&—fF–öâv÷&·2¢¢†&V6÷&G5öf÷%ö–æFW…ö7W'6÷&“¢”D$–æFWƒ£¦÷Våö7W'6÷& §6VæG2F†R¦W†7B6ÖR¢—FW&FV÷W&F–öâ”D$ö&¦V7E7F÷&S£¦÷Våö7W'6÷&FöW2Â'WBv—F‚à¢¢§Væ&÷VæFVB¢¢¶W’&ævR†–æFW†VDD$¶W•&ævS£¦FVfVÇB‚–’&Vv&FÆW72öbv†BF†R6ÆÆW"7GVÆÇ§&WVW7FVB(	BfWF6†–ærWfW'’&V6÷&B–âF†RVæFW&Ç––ærö&¦V7B7F÷&RÂVæf–ÇFW&VBÂ6–æ6RF†R&6¶Væ@¦†2æòv’Fòf–ÇFW"'’â–æFW‚¶W’—BFöW6âwB¶æ÷rW†—7G2âF†R6ÆÆW"w2§&VÂ¢&WVW7FVB&ævP¦Æ—fW2öâF†R”D$7W'6÷&ö&¦V7B—G6VÆb–ç7FVB†7W'6÷"ç&ævV’ÂW6VBÆFW"âöæ6RF†R&r&V6÷&G0¦6öÖR&6²Â–F'&WVW7Bç'66†V6·2v†WF†W"F†R7W'6÷"w26÷W&6R—2–æFW†æBÂ–b6òÂ6ÆÇ0¦&V6÷&G5öf÷%ö–æFW…ö7W'6÷&¢f÷"V6‚&V6÷&BÂFW6W&–Æ—¦W2—G27F÷&VBfÇVRÂ'Vç2F†R¦W†—7F–ær ¦W‡G&7Eö¶W–‡6ÖR¶W’×F‚ÖWfÇVF–öâ6öFR”D$ö&¦V7E7F÷&S£§WFÇ&VG’W6W2v†Vâw&—F–ær¦v–ç7BF†—2–æFW‚w2¶W’F‚ÂæB&W6†W2F†R&V6÷&B6ò¶W–ÒF†RW‡G&7FVB–æFW‚¶W’æ@¦&–Ö'•ö¶W–ÒF†R÷&–v–æÂö&¦V7B7F÷&R¶W’â&V6÷&G2F†BFöâwB†fRfÆ–BfÇVRBF†B¶W§F‚&RG&÷VB‡W"7V2Â7V6‚&V6÷&Bv2æWfW"'BöbF†R–æFW‚’âöæÇ’§F†Vâ¢FöW0¦—FW&FUö7W'6÷&'Vâ(	B6ö×ÆWFVÇ’VæÖöF–f–VBÂ—G2W†—7F–ær–æFW‚Ö'&æ6‚Æöv–2‡÷6—F–öà§G&6¶–ærÂæW‡GVæ—VVö&WgVæ—VVÂ&ævRÖ–âÖ6†V6²v–ç7BF†Ræ÷rÖ6÷'&V7B–æFW‚¶W’’§W7@§v÷&·2à ¢¢¤¶æ÷vâÂFVÆ–&W&FRv¢×VÇF’ÖVçG'’–æFW†W2â¢¢W‡G&7Eö¶W–w2÷vâ×VÇF”VçG'–'&æ6‚—0¦Væ–×ÆVÖVçFVB‚–(	BvVçV–æRW7G&VÒ6W'fòvÂæ÷B6öÖWF†–ær6fRFòW"÷fW"–ç6–FRF†—0§F6‚â×VÇF’ÖVçG'’–æFW‚†â'&’×fÇVVB¶W’F‚ÖVçBFòW‡æB–çFòöæR&V6÷&BW ¦VÆVÖVçB’—2W‡G&7FVBv—F‚×VÇF•öVçG'“¢6öÖR†fÇ6R–&Vv&FÆW72öbF†R–æFW‚w2÷vâfÆr(	@¦6÷'&V7B¶W’§G—R¢†'&—2&RfÆ–B–æFW†VDD"¶W—2öâF†V—"÷vâ’Â'WBæ÷B×VÇF’ÖVçG'’w0§W"ÖVÆVÖVçBW‡ç6–öâ÷Væ—VVæW726VÖçF–72âWfW'’æöâÖ×VÇF’ÖVçG'’–æFW‚‡F†R6öÖÖöâ66RÂæ@§v†B7GVÆÇ’7&6†VBF†R&VÂvÖR’—2VæffV7FVBà ¢¢¤vVçV–æR6öFVvVâ×—7FW'’Â&W6öÇfVB†Fö7VÖVçFVB6ò—B—6âwB&RÖÆ—F–vFVBöâF†RæW‡@§F6‚“¢¢¢vV$”DÂ6öFVvVâFöW6âwB–æfW"W"ÖÖWF†öBv†WF†W"'W7B–×ÆVÖVçFF–öâæVVG27ƒ ¢f×WB¥46öçFW‡Fg&öÒF†RÖWF†öBw2÷vâG—R6–væGW&R(	B—Bw2âW‡Æ–6—BÂÖçVÆÇ’ÖÖ–çF–æV@§W"Ö–çFW&f6RÆÆ÷vÆ—7B–â&–æF–æw2æ6öæf†t”D$–æFW‚s¢²v7‚s¢²ââå×Ö’â”D$ö&¦V7E7F÷&Vw0¦WV—fÆVçBÆ—7BÇ&VG’†B÷Vä7W'6÷&ö÷Vä¶W”7W'6÷&²”D$–æFW†w2æWfW"F–BÂ6–æ6Ræ÷F†–æp¦†BWfW"–×ÆVÖVçFVBF†VÒF†W&R&Vf÷&RâVæ6öÖÖVçF–ærF†RçvV&–FÆÆöæR&öGV6W2v÷&¶–æp¢¦6ö×–ÆR¢v—F‚2×&ÖWFW"G&—BÖWF†öB†æò7†’(	BâV7’G&Â6–æ6RF†R&W7VÇF–ærW'&÷ ¢†SSÂw&öær&ÖWFW"6÷VçB’FöW6âwBö–çBB&–æF–æw2æ6öæfBÆÂà ¢¢¥fW&–f–6F–öã¢¢¢6&vò6†V6²×6W'f÷6†VÆÆ6ö×–ÆVB6ÆVâÂ¦W&òæWrv&æ–æw2â¢¤æ÷B–W@¦W†W&6—6VBöâ&VÂÂÆ–æ¶VBÂ'Vææ–ær&–æ'’¢¢(	BF†—2f—‚ÆæFVBgFW"F†RvÖS¦&÷Fö6öÂf—€§v2Ç&VG’6öæf—&ÖVBv÷&¶–ærf–&VÂ'V–ÆB‡6VRF†BVçG'’&÷fR’Â'WBF†—27V6–f–0¦föÆÆ÷r×W7&6‚†6âwB†B—G2÷vâ&÷VæB×G&—FW7B–WBâv†öWfW"'V–ÆG2F†—2æW‡B6†÷VÆBfW&–g¦—†’×fâ×&V7B×FV×ÆFVw2æ'&F–öâÖ†—7F÷'’fVGW&R†÷"ç’÷F†W"”D$–æFW‚æ÷Vä7W'6÷"‚–ğ¦÷Vä¶W”7W'6÷"‚–6ÆÆW"’æòÆöævW"7&6†W2ÂæBF†B—FW&F–öâ÷&FW"÷&W7VÇG2&R6÷'&V7Bf÷"¦æöâÖ×VÇF’ÖVçG'’–æFW‚‡F†R×VÇF’ÖVçG'’66R—2¶æ÷vâÂ66WFVBv(	B6VR&÷fR’à ¢ÒÒĞ ¢22##bÓ‚Ó#‚(	B–æ¦V7Bv–æF÷råõõ$õdU5õöÖ&¶W"öâWfW'’vRÆö@ ¢¢¤f–ÆS¢¢¢÷'G2÷6W'f÷6†VÆÂöFW6·F÷öç'6Â–âw26W'fòÖ–ç7Fæ6R6WGWÂ&–v‡BgFW ¦W6W$6öçFVçDÖævW#£¦æWvà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãBãóSRÖ–æ¦V7B×v–æF÷r×&÷fW2ÖÖ&¶W"çF6†  ¢¢¤6†ævS¢¢¢FG2öæRW6W%67&—FFòF†RW6W$6öçFVçDÖævW&F†BWfW'’vRÇ&VG’vWG2À§6WGF–ærv–æF÷råõõ$õdU5õòÒG'VV  ¦'W7@§W6W%ö6öçFVçEöÖævW"æFE÷67&—B…&3£¦æWr…W6W%67&—C£¦g&öÒ‚'v–æF÷råõõ$õdU5õòÒG'VS²"’’“°¦  ¥F†—2&WW6W26W'fòw2÷vâW†—7F–ærW6W$6öçFVçDÖævW&öW6W%67&—FÖV6†æ—6Ò†Ç&VG’F†W&P¦f÷"Ò×W6W'67&—G2ÖF—&V7F÷'–Â6VRF†R7W'&÷VæF–ærÆö÷’(	BF†—267&—B§W7B'Vç0§Væ6öæF—F–öæÆÇ’Â&Vf÷&Rç’W6W"×7WÆ–VBöæW2âW"FöÓ£§W6W'67&—G3£¦ÆöE÷67&—FÂW6W §67&—G2'Vâ26ööâ2Æ†VCæW†—7G2öâWfW'’æf–vF–öâÂ&Vf÷&RF†RvRw2÷vâÇ67&—Cæ §Fw2W†V7WFRà ¢¢¥v‡“¢¢¢G&–æ72÷&÷fW2Ö’ö6÷&Vw2—4f–Æ&ÆR‚–‡F†R&Ò’7GVÆÇ’'Vææ–ær–ç6–FR&÷fW2À¦æ÷BÆ–â'&÷w6W"÷"FW&’"6†V6²vV"6öçFVçB—2ÖVçBFòW6R’&Wf–÷W6Ç’†BFòfWF6‚‚– §F†R&÷fW3¦&÷Fö6öÂw2—5öf–Æ&ÆV6öÖÖæBæBv—Bf÷"&W7öç6R(	Bâ7–æ2&÷VæBG&— ¦f÷"v†B6†÷VÆB&R6†VÂ7–æ6‡&öæ÷W26†V6²ÂæBöæRF†B6÷VÆFâwB&W6öÇfRBÆÂVçF–ÂF†P§&÷Fö6öÂ†æFÆW"v2&V6†&ÆRâÖ—'&÷'2†÷rFW&’W‡÷6W2v–æF÷råõõDU$•ô”åDU$äÅ5õöf÷ §F†R6ÖRW'÷6RâG&–æ72÷&÷fW2Ö–cãbãÇ&VG’6†—2F†R¥26–FR&VF–ærF†—2Ö&¶W ¦F—&V7FÇ’†—4f–Æ&ÆR‚“¢&ööÆVææ÷rÂæòÆöævW"&öÖ—6SÆ&ööÆVãæ’(	BF†—2F6‚—2F†P¦ÖF6†–ærVæv–æR×6–FR†Æc²F†RGvò×W7B6†—FövWF†W"f÷"—4f–Æ&ÆR‚–Fò&V†fR6÷'&V7FÇ¢†6†VÆÂv—F†÷WBF†—2Ö&¶W"Ö¶W2—4f–Æ&ÆR‚–Çv—2&WGW&âfÇ6VÂ6–æ6RF†W&Rw2æğ¦&÷fW3¦fÆÆ&6²ç–Ö÷&R’à ¢¢¥fW&–f–6F–öã¢¢¢6&vò6†Ú±î¸Â¸­yêë¢°k¢G§¦*^eck -p servoshell` compiled clean. Confirmed against a real
`@drincs/roves-api@0.6.0`-consuming build (`test-page`) that `window.__ROVES__` is `true`
before the page's own scripts run and `isAvailable()` returns `true` synchronously.

## 2026-08-29 â€” `saves:` protocol: `most_recent` command

**File:** `ports/servoshell/desktop/protocols/saves.rs`.

**Patch:** `patches/servo-v0.4.0/0056-saves-most-recent-command.patch`

**Change:** a new `most_recent` command, returning `{ "key": string, "modifiedMs": number }` or
`null` â€” the most recently modified save key, without reading (or downloading) that save's
own content:

```rust
if best.as_ref().map_or(true, |(_, ms)| modified_ms > *ms) {
    best = Some((key.to_owned(), modified_ms));
}
```

Local candidates come from each `.save` file's own filesystem mtime (`entry.metadata()?.modified()`),
already read as part of the same `fs::read_dir` scan `list`/`clear` already do â€” no extra I/O
per file. When compiled with `--features steam` and a client is running, Steam Cloud files are
also considered via `RemoteStorage::files()` (names only, no content) and each candidate's own
`SteamFile::timestamp()` (a second cheap per-file Steamworks call, still no content read) â€” the
two candidate sets are merged by picking whichever has the higher timestamp.

**Why:** `@drincs/roves-api/saves`'s `getMostRecent()` (the "which save should Continue load"
question) previously had no cheap way to answer this â€” the only option was `list()` (local-only,
see this file's own doc comment) followed by a full `read`/`readJSON` of *every* key just to
compare their own `date` field, downloading and JSON-parsing every save's content (screenshots
included) just to find the winner. Worse, `list()`'s local-only nature meant a save made on
another machine and never yet pulled down locally couldn't be found *at all* â€” an explicit
`read()` of its exact key is the only thing that triggers a Cloud pull, and that key would never
have been known to check in the first place. Steam Cloud's own per-file timestamp is metadata
Steamworks tracks regardless of whether a file has ever been read/pulled locally, so this
command can surface that a newer save exists on another machine â€” the caller can then do a
single, targeted `readJSON` for just the winning key, instead of one per save.

**Caveat:** local mtime and Steam Cloud's own timestamp are two different clocks (the local
filesystem's vs. Steam's own upload bookkeeping) with no guaranteed sub-second alignment â€” for
the "which save is newest" comparison this is a non-issue in practice (real saves are seconds to
hours apart), but don't rely on `modifiedMs` for anything requiring tighter precision than that.

**Verification:** unable to run `cargo check -p servoshell` in this environment (missing
`libclang`, needed by an unrelated `bindgen`-based build dependency â€” a sandbox limitation, not
a code issue). Not yet verified against a real Steam client either â€” same open item as patch
0047's own Steam Cloud sync path (see that entry's own caveat).

## 2026-08-29 â€” Inline SVG `currentColor` stuck on stale restyle

**Files:** `components/shared/layout/lib.rs`, `components/layout/context.rs`,
`components/layout/replaced.rs`, `components/script/dom/window/window.rs`,
`components/script/dom/svg/svgsvgelement.rs`.

**Patch:** `patches/servo-v0.4.0/0057-svg-currentcolor-restyle-invalidation.patch`

**Change:** an inline `<svg>` element's `stroke`/`fill: currentColor` now gets re-baked (and the
cached rasterization re-triggered) when this element's own computed `color` changes, not only
when its attributes or children do.

Root cause (upstream Servo behavior, not introduced by this fork â€” tracked as
[servo/servo#10646](https://github.com/servo/servo/issues/10646)): inline SVG content isn't
laid out/painted in a cascade-aware way at all (`components/layout/stylesheets/servo.css`
disables styling of `<svg>` descendants entirely). Instead, `SVGSVGElement::
serialize_and_cache_subtree` serializes the element's raw XML **once** into a base64
`data:image/svg+xml` url and hands that off to `resvg`/`usvg` â€” a standalone SVG
parser/rasterizer with no knowledge of the host document's CSS. A literal `stroke="currentColor"`
in that markup therefore always resolved `currentColor` to the CSS-initial value (black),
regardless of what this element's real, cascaded `color` actually was â€” and because the only
existing invalidation hooks were `attribute_mutated`/`children_changed`/`unbind_from_tree`, even
a *dynamic* `color` change (e.g. a `.dark` class added to `<html>` in a React `useEffect`,
which runs after first paint) never re-triggered serialization at all: the icon stayed
permanently baked at whatever `color` was in effect on the very first layout, while ordinary
text sitting right next to it correctly repainted with the new color on every subsequent
restyle.

This patch doesn't attempt real cascade-aware SVG layout (the actual fix tracked by #10646 â€”
a much larger undertaking). Instead: `components/layout/replaced.rs`'s `svg_kind_size` now also
computes this element's current resolved `color` (`get_inherited_text().clone_color()`) on every
layout pass and compares it against `SVGElementData::resolved_color` â€” the `color` baked into
the *currently cached* serialization, threaded back from script the same way `source` already
is. A mismatch re-queues the element for serialization via the existing `pending_svg_elements_
for_serialization` mechanism (widened to also carry the resolved `AbsoluteColor`, not just the
node address), exactly like "not serialized yet" already does â€” the stale image keeps being used
for the current layout pass, and the next one picks up the refreshed, correctly-colored image
once script finishes re-serializing it. `SVGSVGElement::serialize_and_cache_subtree` bakes the
color in by inserting a `color="<css color>"` presentation attribute onto the serialized root's
opening tag (`inject_root_color_attribute`, a small quote-aware string splice run on the already-
serialized XML) â€” `usvg` does correctly resolve `currentColor` against a `color` attribute
*within* the SVG document itself, so this doesn't require the document to be cascade-aware, only
this one value to be threaded through at bake time.

**Why:** discovered via `pixi-vn-react-template`'s main menu â€” its `Load`/`Settings` buttons
(shadcn/ui, `variant="outline"`) render `lucide-react` icons (`stroke="currentColor"`) that
render correctly under a real browser but stayed permanently black under Roves, even though the
adjacent button *text* (ordinary CSS `color`, not `currentColor` on an SVG) correctly went white
once the app's theme provider added `.dark` to `<html>` â€” a `useEffect`, so after first paint.
Confirmed via pixel sampling this wasn't an app-code or asset-loading bug: the icon's resolved
paint color never changed from the pre-`.dark`, light-mode `--foreground` (near-black), while
everything else on the page correctly reflected the dark theme.

**Caveat:** this fixes a *dynamic* `color` change after the SVG's first serialization â€” it does
not, and cannot, make inline SVG genuinely cascade-aware. Anything beyond `currentColor` on the
root itself (e.g. `currentColor` used differently per descendant based on the *descendant's own*
computed style, were that ever meaningful for an un-styled SVG subtree) is still out of scope,
as is any other CSS property real cascade-aware SVG support would eventually need (filters,
`mask`, per-element `opacity` transitions, ...) â€” see #10646 for the actual tracked fix.

**Verification:** not yet compiled or run â€” this environment was asked to push straight to CI
(`.github/workflows/test.yml`) rather than run a local `mach build`, given how long a full Servo
rebuild takes. Verify the CI build actually compiles cleanly, then re-test the exact repro above
(`pixi-vn-react-template`'s main menu, `Load`/`Settings` buttons, dark theme) against the
resulting binary before considering this closed.

**Correction (2026-08-30, after a real published release confirmed the fix above did nothing):**
downloaded the actual `v0.4.6` release (built from the commit containing everything above,
CI green on all 6 platform jobs) and re-ran the exact repro â€” the icon was still black. CI
passing only ever proved the engine still *compiles and doesn't crash*, never that this
specific visual bug was actually fixed; nothing in `test.yml` checks rendered pixel colors.

**Root cause of the original fix's own failure:** `components/layout/replaced.rs`'s
`svg_kind_size` (added above) is only ever reached when box-tree construction actually
reconstructs this element â€” which only happens when `components/layout/traversal.rs`'s
`box_damage_action` sees `LayoutDamage::DescendantHasBoxDamage`/`BoxDamage` bits on it
(`BoxDamageAction::TryRebuild`/`RebuildAncestor`). Confirmed directly against the vendored
`stylo` crate: the `color` longhand is declared `servo_restyle_damage = "repaint"`
(`stylo-0.19.0/properties/longhands.toml`), so a `color`-only restyle produces *only*
`RestyleDamage::REPAINT` â€” never any bit `LayoutDamage`'s box-rebuild range overlaps. The
one hook that could escalate a repaint into box-rebuild damage
(`TElement::compute_layout_damage`, `components/script/layout_dom/
servo_dangerous_style_element.rs`) is itself gated on the base damage already containing
`RELAYOUT` (`stylo-0.19.0/servo/restyle_damage.rs`), which a `color` change never has either.
Net effect: `svg_kind_size`'s own color-mismatch check â€” sound on its own â€” simply never got
a chance to run a second time after the SVG's first (pre-theme-toggle, wrong-color)
serialization, for exactly the restyle shape (a `color`-only change from a class toggled onto
a distant ancestor) this whole patch exists to handle. `usvg`/`resvg`'s own `currentColor`
resolution against the `color` presentation attribute (`inject_root_color_attribute`) was
independently confirmed correct and not the problem
(`usvg-0.47.0/src/parser/style.rs`/`svgtree/mod.rs`).

**Files (additional):** `components/layout/traversal.rs`.

**First attempt at a fix (this section originally described it; superseded below, kept for the
record): hooking into `RecalcStyle::process_preorder` â€” didn't compile.** The idea was sound
(escalate damage right after `recalc_style_at` computes fresh style) but reaching
`NodeExt::as_svg()` from there needed widening `RecalcStyle`'s own generic trait bounds to
`E::ConcreteNode: NodeExt<'dom>`. That bound is never satisfiable: `process_preorder` operates
on `E::ConcreteNode`, which resolves to `ServoDangerousStyleNode` (`components/script/
layout_dom/servo_dangerous_style_node.rs`) â€” a distinct type from `ServoLayoutNode`, the only
type `NodeExt` is actually implemented for (`components/layout/dom.rs`). Pushed anyway without
local verification (no working C/C++ toolchain in that environment for even `cargo check` â€”
`glslopt`'s build script needs `clang-cl.exe`/a linker neither this nor the release environment
prior had wired into `PATH`) â€” CI failed `mach build` on all 6 matrix jobs, a genuine compile
error, not flaky infra.

**Actual fix:** `compute_damage_and_rebuild_box_tree_below_dirty_root`
(`components/layout/traversal.rs`) â€” unlike `RecalcStyle::process_preorder`, this function is
**not** generic; it already takes a concrete `node: ServoLayoutNode<'dom>` and already borrows
`element_data` (via `element.element_data_mut()`) to read the freshly-computed style right
where it extracts this element's own `RestyleDamage` into a `LayoutDamage` value â€” no new trait
bounds needed at all. A new `svg_color_is_stale(node, &element_data)` helper does the same
comparison the first attempt did (`NodeExt::as_svg()`'s `resolved_color` vs
`get_inherited_text().clone_color()`), and on a mismatch the computed `LayoutDamage` gets
`DescendantHasBoxDamage` inserted directly â€” the identical bit
`Element::restyle(NodeDamage::ContentOrHeritage)` already uses for "this box's own content
changed, rebuild it and its ancestors, not descendants" (`components/script/dom/element/
element.rs`), which is exactly the right shape here: an SVG replaced element has no real
box-tree descendants of its own to rebuild. This function runs for every element carrying any
restyle damage at all (including plain `REPAINT`, which is why `box_damage_action` has its own
repaint-only fallthrough path) â€” including a `color`-only restyle propagated down from an
inherited-property change on a distant ancestor â€” so it reaches our stale-color SVG on exactly
the restyle this whole patch exists to handle, independent of whatever damage `stylo` itself
classified that restyle as.

**Patch:** regenerated `patches/servo-v0.4.0/0057-svg-currentcolor-restyle-invalidation.patch`
in place â€” one coherent "fix inline SVG `currentColor`" change, not a patch documenting its own
first, non-functional attempt as a separate reviewable step (same reasoning the 2026-08-14 boot
splash icon entry above used for its own multi-round corrections).

**Verification:** not compiled locally â€” this environment does have a Rust toolchain
(`rustc`/`cargo`, and a prior `target/debug` from some earlier build), but no working
C/C++ toolchain reachable from a plain shell (`clang-cl.exe`/`link.exe`/`lld-link.exe` all
missing from `PATH` despite Visual Studio 2022 being installed â€” `mach`'s own environment
setup for it isn't active outside `mach`'s own invocation, and this repo has native
dependencies, like `glslopt`, with their own build scripts that need it even just to
`cargo check`), so this is a real environment gap, not a shortcut taken carelessly the second
time either. Manually re-derived every type in the new call chain against this file's own
existing, already-working code (`ServoLayoutNode`/`LayoutElement`/`ElementData` are all used
identically a few lines away in the same function) rather than guessing blind the way the
superseded generic-bounds attempt above effectively did. This round *did* compile and boot
(CI green, all 6 platforms) â€” but a real screenshot from the resulting `v0.4.7` release still
showed the icon black. Not a regression from this change specifically: re-tested the
*unmodified* `v0.4.6` binary side by side and it now failed identically (previously
confirmed working) â€” a local GPU/driver issue on the testing machine (a full restart fixed
it), unrelated to any of this patch's code. Re-tested `v0.4.7` after the restart: the rest of
the page rendered correctly again, but the icon was still black â€” a real, second miss.

**Second correction, same day: the actual remaining blocker was the baked color's own CSS
syntax, not the escalation logic.** `roves.log` had the answer the whole time, just not read
closely enough until now: `usvg::parser::svgtree] Failed to parse color value:
'oklch(0.985 0 0)'` (and `oklab(...)`), repeated for every resolved color this session baked
via `color="..."`. This app's own CSS defines its theme variables with `oklch(...)`
(`--foreground: oklch(0.985 0 0)`, etc.) â€” `AbsoluteColor` preserves whatever color space a
value was originally specified in rather than normalizing to sRGB, so `clone_color()` here
returns an oklch-space color, and `to_css_string()` faithfully serializes that back out as
`oklch(...)`. `usvg`'s own CSS color parser doesn't understand CSS Color 4 functions
(`oklch()`/`oklab()`/`lab()`/`lch()`) â€” only legacy syntax (`rgb()`/`rgba()`/hex/named). A
color it can't parse is silently treated as unset, so `currentColor` kept falling back to its
own default (black) even though a syntactically-present `color` attribute was right there in
the markup â€” the escalation logic (this entry's main fix) had been re-baking the *correct*
color the whole time, just spelled in a dialect the consumer couldn't read.

**Fix:** `SVGSVGElement::serialize_and_cache_subtree` now calls
`resolved_color.into_srgb_legacy().to_css_string()` instead of a plain `to_css_string()` â€”
`AbsoluteColor::into_srgb_legacy()` (already public stylo API, no crate patch needed) converts
to the sRGB color space and forces the legacy `rgb()`/`rgba()` serialization flag, guaranteeing
a syntax `usvg` has always supported. `cached_resolved_color` still stores the *original*
(oklch-space) value, since `AbsoluteColor` is `Copy` and the comparison in
`svg_color_is_stale`/`svg_kind_size` only needs self-consistency across calls, not any
particular color space.

**Patch:** regenerated `patches/servo-v0.4.0/0057-svg-currentcolor-restyle-invalidation.patch`
in place again â€” same "one coherent fix" reasoning as both prior corrections in this entry.

**Verification:** same local-compile constraint as above, pushed to CI again (green, all 6
platforms) and cut as a real release, `v0.4.8`. **Confirmed fixed this time** â€” downloaded the
actual `v0.4.8` release asset, ran the exact repro (`pixi-vn-react-template`'s main menu,
dark theme) fresh after a full machine restart (an unrelated GPU/driver issue from an earlier
round of testing had been giving false "still broken" readings even on the unmodified
`v0.4.6` binary â€” see the correction above), and both `Load` and `Settings` icons render
white, matching the surrounding text. `roves.log` no longer contains any
`Failed to parse color value` lines. This closes out what turned out to be three separate,
independently-necessary fixes stacked in this one patch: (1) baking the resolved `color` into
the serialized SVG at all, (2) re-triggering that bake when the color changes after a
repaint-only restyle, and (3) serializing that color in a syntax `usvg` can actually parse.

## 2026-08-29 â€” Regenerated icon assets from an updated `icon.svg`

**Files:** `icon.svg`, `resources/servo.svg`, `resources/servo_64.png`,
`resources/servo_1024.png`, `resources/servo.ico`, `resources/servo.icns` â€” binary raster
assets (all but the two `.svg` files) not part of any patch, same reasoning as the 2026-08-13
entry below: a text-based unified diff can't represent new binary content.

**Change:** `icon.svg` was edited directly by the user (the wolf-and-chains mark's own
artwork). Regenerated every derived asset from it through the same one pipeline the
2026-08-13 entry below established (rasterize the vector, then resize/pack every other
format from that one master), so nothing referencing the icon silently keeps showing the
*previous* version of the artwork: `resources/servo.svg` (a byte-identical copy â€” confirmed
via checksum), `resources/servo_1024.png` (boot splash) and `resources/servo_64.png`
(compile-time window/taskbar icon default, `build.rs`'s `window_icon_src`) rasterized from a
2048Ã—2048 master render, `resources/servo.ico` (16/24/32/48/64/128/256, matching the existing
asset's own size set) via `sharp-ico`, and `resources/servo.icns` (`ic07`/`ic08`/`ic09`/`ic10`/
`ic11`/`ic12`/`ic13`/`ic14` â€” 1x and 2x slots from 16pt to 512pt, each a directly-embedded PNG
buffer â€” modern ICNS readers, including macOS itself, accept PNG-encoded entries for these
type codes, so this doesn't need legacy raw-bitmap packing) via a small inline packer, since
no `.icns`-writing package was available in this environment (see below).

Every other `.svg` in this repo was checked and left alone â€” none of them are derived from
`icon.svg`'s own artwork: `resources/roves_wordmark.svg` is a separate text-based lockup (the
"Roves" wordmark shown next to this icon in the boot splash, not the icon itself),
`resources/resource_protocol/servo-color-{positive,negative}-no-container.svg` are upstream
Servo's own wordmark logo (a wide 284Ã—63 text lockup, used for `resource:`-served error/about
pages â€” never Roves-branded), `test-page/public/favicon.svg` is a deliberately unrelated
placeholder for a different feature (see the 2026-08-09 "Game-supplied icon" entry above â€”
literally documented there as unrelated to this mark), and `roves-packmaster`/`pixi-vn-react-template`'s
own `.svg` files belong to Packmaster's and the game template's own, entirely separate branding.

**Tooling note (differs from the 2026-08-13 entry's own pipeline):** that entry used
`cairosvg`/Pillow/`icnsutil` (Python); none of the three were available in this session's
Python environment (only Pillow, used here only to sanity-check the regenerated `.ico`'s
sizes, not to produce anything). Used `sharp`/`sharp-ico` instead (both already present as
transitive `node_modules` of `pixi-vn-react-template`/`roves-wiki` â€” not installed as a new
dependency of this repo) for SVG rasterization and `.ico` packing, and a ~20-line inline
Node script (not committed â€” a one-off, not a maintained tool) for `.icns`, since no
`.icns`-writing npm package was available either. Functionally equivalent output to the
previous pipeline; if a *real* generation script ever gets committed for this (neither
pipeline has one today â€” every regeneration so far, this one included, has been ad hoc), it
should standardize on one toolchain rather than switching per-session.

**Verification:** `resources/servo.svg` confirmed byte-identical to `icon.svg` via `md5sum`.
`resources/servo.ico` confirmed via Pillow to report all 7 expected sizes
`{16,24,32,48,64,128,256}`. `resources/servo.icns` confirmed recognized as a valid "Mac OS X
icon" file. `resources/servo_1024.png` visually confirmed to render the intended
wolf-and-chains artwork correctly (not blank/corrupt). Not done: an actual `mach build`/real
launch showing the *new* artwork in the boot splash/window icon/taskbar â€” no local Servo build
in this environment (same constraint as the SVG `currentColor` entry above); the next real CI
build is what would confirm this end-to-end.

## 2026-08-30 â€” `system_info` diagnostics command on the `roves:` protocol

**Files:** `Cargo.toml` (workspace root), `ports/servoshell/Cargo.toml`,
`ports/servoshell/desktop/protocols/roves.rs`. Plus, outside this `servo/` directory (not
patch-tracked â€” see the README.md/examples/ precedent above): `roves-api/src/core.ts`, adding
a `SystemInfo` interface + `systemInfo()` function.

**Patch:** `patches/servo-v0.4.0/0058-system-info-diagnostics-command.patch`

**Upstream behavior:** no equivalent â€” this is new functionality, not a modification of
existing upstream logic.

**Change:** added a new `system_info` command to `RovesProtocolHandler` (see the 2026-08-06
"Roves' own general-purpose `invoke()` bridge" entry above for how this protocol works),
returning a JSON object with host OS and engine diagnostics: `os_type` and `os_version` (via
the `sysinfo` crate's `System::distribution_id()`/`System::os_version()` associated
functions â€” no `System` instance needed for these two), `bitness` (derived from
`cfg!(target_pointer_width = "64")`, not from `sysinfo`, since that crate doesn't expose
process/OS bitness directly), `architecture` (`std::env::consts::ARCH`), and `engine_version`
(this fork's own `servoshell::VERSION` constant â€” the running Servo build's actual version,
not a generic "webview version" the way Tauri/Electron would report one, since here the
engine itself is the thing worth naming for graphics/compatibility debugging). Added
`sysinfo = { version = "0.38" }` to the workspace's shared dependency table (it was already
present as a transitive dependency in `Cargo.lock` at that version, pulled in indirectly by
something else in the tree, so this adds no new dependency to the build) and wired it into
`ports/servoshell/Cargo.toml`'s existing desktop-only dependency block (alongside
`steamworks`/`surfman`, which live under the same `cfg(not(any(target_os = "android",
target_env = "ohos")))` block since neither Android nor OpenHarmony route through this
protocol handler the same way).

Outside `servo/`: `roves-api/src/core.ts` gained a `SystemInfo` interface and a `systemInfo()`
function (`invoke<SystemInfo>("system_info")`), following the same `core.invoke()` pattern as
`isAvailable()`/`exit()` above it in that file. Field names deliberately mirror
`@tauri-apps/plugin-os`'s (`type()`/`version()`/`arch()`) and the `os_info` crate's
(`os_type`/`version`/`bitness`/`architecture`) own conventions, so code already familiar with
either feels at home; `engine_version` is the one field neither has an equivalent for.

**Why:** requested for bug-report and graphics-compatibility triage â€” knowing the host OS,
its version, process bitness/architecture, and specifically *which build of the Servo fork*
is running (rather than a generic "webview version") narrows down graphics/rendering
discrepancies reported by game developers or players far faster than asking them to describe
their machine by hand.

**Not done (left as a judgment call, not an oversight):** a `graphics_backend` field (which
compositor/GPU backend Servo picked at runtime) was considered but not added â€” there wasn't
an existing, already-computed value to surface cheaply (unlike `os_type`/`architecture`,
which `sysinfo`/`std::env::consts` hand over for free); wiring one up would mean reaching into
`surfman`/`webrender`'s own backend-selection state, a larger change than this pass, deferred
until it's actually needed for a concrete debugging case.

**Verification:** `roves-api`'s own `tsup` build (`npm run build`) confirmed the new
`SystemInfo`/`systemInfo()` TypeScript compiles and emits `.d.ts` output cleanly. The Rust
side was **not** locally compiled â€” this environment has no working C/C++ toolchain reachable
outside `mach`'s own env setup (see the SVG `currentColor` entry above for the same
constraint) â€” so this is pending confirmation from the next CI run/real launch before being
treated as fully verified end-to-end.

**Follow-up (2026-08-31) â€” confirmed working end-to-end:** downloaded the resulting CI test
build and called `roves:system_info` from a real page, confirmed via `roves.log`:
`{"os_type":"windows","os_version":"11 (26200)","bitness":"64-bit","architecture":"x86_64","engine_version":"Servo 0.4.0-<sha>"}`.

## 2026-08-31 â€” Fix `OffscreenCanvas` 2D context silently discarding `font`

**Files:** `components/script/dom/canvas/2d/canvas_state.rs`,
`components/script/dom/canvas/2d/canvasrenderingcontext2d.rs`.

**Patch:** `patches/servo-v0.4.0/0059-offscreencanvas-font-resolution.patch`

**Upstream bug, not a Roves customization:** `CanvasState::set_font` resolves relative/
inherited `font` values (`ctx.font = "..."`) against a live DOM node's computed style via
`Window::resolved_font_style_query`. It required an actual `HTMLCanvasElement` to do this â€”
but an `OffscreenCanvas` constructed directly (`new OffscreenCanvas(w, h)`, not obtained via
`HTMLCanvasElement.transferControlToOffscreen()`) has no such "placeholder canvas" at all, so
`set_font` had a bare `None => return` guard that silently discarded the request entirely,
leaving the context's font permanently stuck on the CSS initial `"10px sans-serif"` â€”
regardless of what was actually requested, for the lifetime of that context.

**Why this matters for games:** this isn't an obscure edge case â€” `OffscreenCanvas` is exactly
what libraries reach for to do font-metrics probing/text rasterization off the main canvas,
specifically because it's cheaper to create than a full `<canvas>` element. PixiJS's own
`CanvasTextMetrics` (used by every `PIXI.Text`) does exactly this. The practical symptom
looked nothing like a font bug at first glance: `PIXI.Text` objects rendered with severely
wrong-looking glyphs (initially misread as a WebGL texture orientation/mirroring bug â€” see
this file's own mirrored-text investigation history) because PixiJS's internal
`ascent`/`descent` measurement (via `TextMetrics.actualBoundingBoxAscent/Descent` on an
`OffscreenCanvas`) came back based on the wrong, never-updated ~10px default font instead of
the real requested size, so PixiJS allocated a drastically undersized text canvas and
mis-positioned the baseline â€” most of each glyph ended up drawn off-canvas/clipped, and the
small remaining visible sliver was what looked like corrupted/mirrored text.

**The fix:** `set_font` now falls back to the owning document's root element
(`window.Document().GetDocumentElement()`) when there's no canvas element to resolve
against, instead of giving up â€” reusing the exact same, already-correct
`resolved_font_style_query` path unchanged, just anchoring it to a different (but always
present, for any `OffscreenCanvas` owned by a `Window`) reference node. A `None` fallback is
kept only for the genuine edge case of an `OffscreenCanvas` owned by a `Worker` global scope
with no `Document`/layout tree to resolve against at all (`global.downcast::<Window>()`
returns `None` there) â€” in that case there's no sensible default to fall back to, so the
existing (already broken, but rarer) no-op behavior is preserved rather than guessed at.

**Verification:** confirmed the root cause directly and in isolation before writing this fix â€”
a minimal test page calling `new OffscreenCanvas(w, h).getContext('2d')`, setting
`ctx.font = '...'`, and calling `measureText()` returned identical, wrong
`actualBoundingBoxAscent`/`actualBoundingBoxDescent`/`width` values **regardless of the
requested font size or the canvas's own dimensions** (tested 0Ã—0 through 2048Ã—64), while the
exact same font/text measured on a regular `<canvas>` element gave correct, expected values
every time.

**Confirmed fixed (2026-08-31):** built via CI, downloaded the resulting test binary, and
verified with a live screenshot â€” the same `PIXI.Text` (`fontFamily: "Arial"`, `fontSize: 60`,
`stroke`, `dropShadow`) that previously rendered as severely corrupted/mirrored-looking text
now renders correctly, right-side up, in both a minimal isolated repro and the real game
(`pixi-vn-react-template`'s own `second_part` narration label, the scene the bug was
originally reported in).

## 2026-08-31 â€” `mach bundle --android`: pack `--content-dir` into the APK, apply `manifest.webmanifest`'s `orientation`

**Files:** `python/servo/post_build_commands.py`,
`support/android/apk/servoapp/src/main/AndroidManifest.xml`,
`support/android/apk/servoapp/build.gradle.kts`,
`support/android/apk/servoapp/src/main/java/org/servo/servoshell/MainActivity.kt`.

**Patch:** `patches/servo-v0.4.0/0060-android-content-bundling-and-orientation.patch`

**Upstream behavior:** upstream Servo already ships a complete, unmodified Android port â€”
JNI bridge (`ports/servoshell/egl/android/`), a real Gradle project
(`support/android/apk/`), and full `--android` target handling in
`python/servo/platform/build_target.py`/`build_commands.py`/`package_commands.py` â€” none of
which this fork had touched before now (see the `.github/workflows/android.yml` entry
below/`.github/workflows/` itself, not patch-tracked, for the CI side of this). But none of it
had any notion of "a game's own web content": `mach bundle` (this fork's own packaging
command, patch 0004) had no `--android` branch at all, `AndroidManifest.xml`'s `MainActivity`
had no `android:screenOrientation` (defaulting to `unspecified` â€” the OS/sensor decides,
rotates freely), and a normal (non-`ACTION_VIEW`) launch never called `loadUri` at all before
native init, so the app had nothing to display beyond Servo's own built-in UI shell.

**Change:** `bundle()` gained an early `is_android(self.target)` branch (before any of the
desktop-only launch.json/icon/per-OS logic) dispatching to a new `_bundle_android` method:
copies `--content-dir` straight into `servoapp/src/main/assets/www/` (an installed APK's
assets are baked in at package time â€” there's no desktop-style "drop loose/packed files next
to an already-built binary" equivalent for Android), then re-invokes Gradle's
`:servoapp:assemble<Arch>Debug` task (mirroring `package_commands.py`'s own arch-string
mapping) with an extra `-PservoScreenOrientation=<value>` project property, and copies the
resulting `.apk` into `--output`. `_resolve_android_orientation` reads
`manifest.webmanifest`'s (or plain `manifest.json`'s) standard `orientation` field and
translates the 6 directional PWA values into their Android `android:screenOrientation`
equivalents (`landscape` â†’ `sensorLandscape`, `landscape-primary` â†’ `landscape`,
`landscape-secondary` â†’ `reverseLandscape`, and the portrait equivalents) â€” `any`/`natural`/
missing/unrecognized all fall back to `unspecified`, so content with no manifest or no
`orientation` field behaves exactly as before. `build.gradle.kts`'s `defaultConfig` sets
`manifestPlaceholders["screenOrientation"]` from a `servoScreenOrientation` Gradle project
property (defaulting to `"unspecified"` when unset, e.g. the plain engine-shell CI build
below), which `AndroidManifest.xml`'s `MainActivity` now references via
`android:screenOrientation="${screenOrientation}"`. `MainActivity.kt`'s `onCreate` now calls
`servoView.loadUri("file:///android_asset/www/index.html")` in the `else` branch of the
existing `Intent.ACTION_VIEW` check (previously that branch did nothing at all) â€” the exact
path the freshly-copied assets land at, mirroring the existing `ACTION_VIEW` call one line
above it rather than introducing a new code pattern.

Getting `bundle()` to actually see `--android` at all needed one more, non-obvious fix: its
`@CommandBase.common_command_arguments(...)` decorator was `binary_selection=True` only, which
neither registers `--android`/`--target` as valid flags nor ever calls
`self.configure_build_target(...)` â€” so `self.target` stayed the host desktop target
regardless of any `--android` passed on the command line, and argparse would have rejected an
unregistered `--android` outright besides. Adding `build_configuration=True` alongside it
fixes both: it registers `--android`/`--target` (plus a handful of build-only flags --
`--features`, `--media-stack`, etc. -- that are meaningless for `bundle` and silently absorbed
by its existing `**kwargs`), and, critically, makes `command_base.py`'s own
`configuration_decorator` call `self.configure_build_target(kwargs)` *before*
`binary_selection`'s `self.get_binary_path(...)` a few lines later in that same wrapper â€” both
inside one closure, in the right order. This is deliberately *not* the
`@CommandBase.allow_target_configuration` decorator `run`/`package` use for the same purpose:
that decorator wraps *around* the `common_command_arguments`-decorated function, so its own
`configure_build_target` call would run *after* `binary_selection` had already resolved
`servo_binary` against the still-unconfigured host target â€” i.e. it would have silently kept
resolving the desktop binary path even with `--android` passed. See `bundle()`'s decorator
comment for the full reasoning.

**Why:** requested to make the just-added Android CI build (see the entry below) actually
capable of shipping a real game rather than only Servo's own bare browser-chrome UI, with
`orientation` singled out as the first, most important manifest field to honor â€” a mobile game
that's designed landscape-only but launches with the OS free to rotate it into portrait (or
vice-versa) is a broken first impression, not a cosmetic gap. App name/icon/`theme_color` from
the same manifest are a deliberate, explicit follow-up â€” not attempted in this pass.

**Not done (left as a judgment call, not an oversight):** no release/signed build path for
Android â€” `mach bundle --android` only ever produces a debug build (the Android Gradle
Plugin's own auto-generated debug key), matching the plain-shell CI build; a signed release
`.apk`/`.aab` would need its own keystore-secret plumbing, deliberately out of scope until an
actual distribution need for one shows up. Also not done: any way to swap a bundled game's
content without a full Gradle rebuild â€” unlike desktop's "one prebuilt shell + loose/packed
files dropped next to it," an installed APK's assets are fixed at package time, so (unlike
every desktop platform) `mach bundle --android` is inherently a per-game rebuild, not a
content-swap onto a prebuilt shell.

**Verification:** `python/servo/post_build_commands.py` parses cleanly (`ast.parse`) and the
new patch applies cleanly with `patch -p1` to a fresh pristine `v0.4.0` extraction (see
`.github/workflows/android.yml`'s own patch-application step for the same mechanism in CI).
The actual Gradle/Kotlin/JNI path (does the APK actually build, does the orientation
placeholder actually resolve, does the bundled `index.html` actually load) is **not** locally
verified â€” Android cross builds only run on Linux/macOS hosts (this environment is Windows),
and there is no local Android SDK/NDK/Gradle toolchain here regardless. Pending a real
`mach bundle --android --content-dir <dist>` run on Linux/macOS before this is treated as
confirmed working end-to-end.

## 2026-08-31 â€” Android: CI build (debug `.apk` on every commit to `main`)

**Files:** none under this directory's patch-tracked Servo source â€” this is a new top-level
CI workflow, `.github/workflows/android.yml`, alongside the existing `test.yml`/`release.yml`
(same reasoning as those two for why workflow files themselves aren't patch-tracked: they're
this fork's own meta files, not modifications to anything that originates in upstream Servo).
`README.md`'s "Supported platforms" table also gained an Android row.

**Patch:** none â€” see above.

**Upstream behavior:** n/a â€” CI infrastructure, not an engine behavior change.

**Change:** `android.yml` builds on every push to `main` (`workflow_dispatch` too), using the
same pristine-tag-plus-patches reconstruction `test.yml` uses (not `release.yml`'s "build the
tracked tree directly" approach â€” this only exists to smoke-test that the patches still apply
and still build, on every commit, the same purpose `test.yml` serves for desktop). Installs a
JDK, the Android SDK (`platforms;android-37`/`build-tools;36.0.0`, matching
`servoapp/build.gradle.kts`'s `compileSdk`/`buildToolsVersion`), and NDK r28 (resolved
dynamically via `sdkmanager --list` rather than a hardcoded exact patch version, since
`python/servo/platform/build_target.py`'s `AndroidTarget` hard-requires major version 28
specifically but doesn't care which r28.x) on `ubuntu-latest` (Android cross builds are
Linux/macOS-only, enforced by that same file). Runs a plain `./mach build --android`, which
upstream's own `build_commands.py` auto-dispatches into `mach package --android` (a Gradle
assemble task) once `libservoshell.so` cross-compiles, since `AndroidTarget.needs_packaging()`
is `True` â€” no `mach bundle` involvement yet at this point (see the entry above, added the
same day, once `--content-dir`/orientation support existed). Uploads the resulting debug
`.apk` as a plain workflow artifact â€” not a GitHub Release (see `test.yml`/`release.yml` for
those), per how this was requested.

**Why:** first step toward Android as a real Roves distribution target, requested explicitly
scoped to "just the shell" for this pass â€” no `roves-action`/`roves-wiki` sync yet (both
normally required in the same turn as a CI/build-surface change â€” see `CLAUDE.md` â€” explicitly
deferred here per that request), and no `mach bundle` content wiring at the time this was
written (see the entry above for that, added later the same day).

**Verification:** not locally testable â€” Android cross builds are Linux/macOS-only host
(this environment is Windows) and there is no local Android SDK/NDK/Gradle toolchain here
regardless. Pending confirmation from the first real run of `android.yml` on GitHub Actions.

## 2026-09-01 â€” `mach bundle --android`: full manifest coverage (app name, icon, theme color), explicit overrides, build from a scratch copy instead of tracked source

**Files:** `python/servo/post_build_commands.py`,
`support/android/apk/servoapp/src/main/AndroidManifest.xml`,
`support/android/apk/servoapp/build.gradle.kts`,
`support/android/apk/servoapp/src/main/java/org/servo/servoshell/MainActivity.kt`.

**Patch:** `patches/servo-v0.4.0/0061-android-manifest-full-coverage-and-scratch-build.patch`

**Correction (of the 2026-08-31 "`mach bundle --android`" entry above):** that entry's
`_bundle_android` copied `--content-dir` straight into the *tracked*
`support/android/apk/servoapp/src/main/assets/www/`, and this entry's icon support would have
done the same to `res/mipmap/servo.webp`. Every other `bundle()` output lands under
`--output`/`target/`, never mutating tracked source â€” this was the one exception, and it had a
real, concrete bug: a specific game's bundled content/icon would silently persist in the
working tree across later, unrelated `mach build --android`/`mach bundle --android` runs on
the same checkout (e.g. the plain engine-shell CI build in `.github/workflows/android.yml`
picking up a leftover game icon from whoever last ran `mach bundle --android` there). Fixed by
copying `support/android/apk/` into `target/<triple>/android-bundle/` (a scratch build
directory, deleted and recreated on every invocation, alongside `_bundle_android`'s
pre-existing `target/<triple>/resources` output) and building from that copy instead â€” the
tracked source tree is never touched by a bundle run at all now.

**Upstream behavior:** n/a for the manifest-field/override additions (continues yesterday's
entry, no further upstream Android functionality touched). The scratch-copy fix has no
upstream equivalent either â€” `mach package --android` (untouched, plain upstream) has always
built in place inside `support/android/apk/`, same as before; only this fork's own
`_bundle_android` (which layers a game's own content on top) needed the copy-first fix.

**Change:**

- `_read_web_manifest(content_dir)` replaces last entry's `_resolve_android_orientation`:
  reads and parses the game's manifest once (trying `manifest.webmanifest`, `manifest.json`,
  then `site.webmanifest` â€” the third is new, added per explicit request to consider
  alternate filenames real tools emit, e.g. realfavicongenerator.net's default output name),
  returning `{}` if none exist/parse as a JSON object. `_bundle_android` now resolves three
  fields off that one dict, each with an explicit-override flag that always wins:
  - `orientation` â†’ `--android-orientation` (same PWA vocabulary as the manifest field itself,
    translated via the pre-existing `_ANDROID_ORIENTATION_MAP`) â†’ Gradle property
    `servoScreenOrientation` â†’ `AndroidManifest.xml`'s `android:screenOrientation` placeholder
    (unchanged from yesterday's entry, just re-plumbed through the shared manifest dict).
  - `short_name` (falling back to `name`) â†’ `--android-app-name` â†’ Gradle property
    `servoAppName` â†’ a new `AndroidManifest.xml` `android:label="${appName}"` placeholder on
    `<application>` (previously the static `android:label="@string/app_name"`).
    `build.gradle.kts` defaults the placeholder to the literal string `"@string/app_name"`
    when neither the manifest nor the flag set one â€” since manifest placeholders are plain
    text substitution into the merged manifest, this reproduces the exact original
    `@string/app_name` resource reference for an unbundled/no-override build, not a
    regression.
  - `theme_color` â†’ `--android-theme-color` â†’ Gradle property `servoThemeColor`. Unlike the
    two above, this one can't go through a manifest placeholder: `android:statusBarColor`
    lives in a *theme* (`res/values/styles.xml`), which manifest placeholders can't reach
    (those only substitute inside `AndroidManifest.xml` itself). Instead, `build.gradle.kts`
    turns it into a new string resource via `resValue("string", "servoThemeColor", ...)`
    (defaulting to `""`), and `MainActivity.kt`'s `onCreate` reads it and calls
    `Window.setStatusBarColor` at runtime when it's non-blank â€” `Color.parseColor` failures
    (e.g. a CSS `rgb(...)` value it doesn't understand) are caught and logged, not fatal.
    `buildFeatures { resValues = true }` was added since AGP 8+ requires this explicit opt-in
    for `resValue` (off by default for build-time-cost reasons).
- Icon: **not** read from the manifest's own `icons` array at all, per explicit instruction â€”
  "one icon-resolution mechanism serves every platform, not one per platform." `bundle()`'s
  existing `--icon-png`/auto-detect-from-`--content-dir` logic (previously computed *after*
  the `is_android` early-return, so never reached it) moved to run *before* that branch, and
  its result is now passed into `_bundle_android`, which replaces
  `res/mipmap/servo.webp` â€” deleting it first, since `AndroidManifest.xml`'s
  `android:icon="@mipmap/servo"` resolves by resource *name*, and having both `servo.webp`
  and a new `servo.png` in the same density bucket is a duplicate-resource-name build error,
  not an override â€” with the resolved PNG (as `servo.png`).

**Why:** direct continuation of the 2026-08-31 entry's own explicitly-deferred follow-up
("App name/icon/theme-color from the same manifest are a deliberate follow-up, not attempted
here") â€” requested the next day, together with corresponding `roves-action`/Roves Packmaster
(`roves-packmaster`) work tracked separately (see `TODO.md` #3, which this entry closes the
engine-side portion of).

**Not done (left as a judgment call, not an oversight):** `display`, `background_color`, and
`lang` from the same manifest still aren't read â€” `display`/`background_color` have weaker,
more involved Android equivalents (a real splash-screen background needs Android 12+'s
SplashScreen API, a bigger change than a manifest placeholder or resValue; a "fullscreen"
`display` value has no single obvious `Activity` flag equivalent worth guessing at), and
`lang` has no clear game-shell-relevant Android equivalent at all (Android's own locale
handling is a system-wide setting, not a per-app manifest attribute an app can just adopt).
`TODO.md` #3 tracks these as still-open if a concrete need for one shows up.

**Verification:** `post_build_commands.py` parses cleanly (`ast.parse`), and the new patch
applies cleanly with `patch -p1` on top of a fresh pristine `v0.4.0` extraction with patches
0001-0060 already applied (confirmed byte-for-byte identical to this working tree afterward
for all 4 touched files â€” same method as the 2026-08-31 entry's own verification). The actual
Gradle/Kotlin path (does `resValue`/the new manifest placeholder actually resolve, does the
status bar color actually apply, does the icon actually get picked up by a real build) is
**not** locally verified, same constraint as every Android entry so far: Android cross builds
are Linux/macOS-only host (this environment is Windows), no local Android SDK/NDK/Gradle
toolchain here regardless. Pending a real `mach bundle --android --content-dir <dist>` run on
Linux/macOS.

---

## 2026-09-10 â€” Fix `ndk-build` invocation for Windows (Gradle needs the `.cmd` extension)

**File:** `support/android/apk/servoview/build.gradle.kts`.

**Patch:** `patches/servo-v0.5.0/0004-android.patch` (regenerated in place â€” same file this
patch already covered for the app-name/orientation/theme-color/icon work above).

**Upstream behavior:** the `ndkbuild<Variant>` Gradle task (used only to copy
`libservoshell.so`/`libc++_shared.so` into the APK's `jniLibs/`, not to compile anything â€” see
this file's own comment above that task) invokes `getNdkDir() + "/ndk-build"`, with no file
extension. On Linux/macOS the NDK ships a real file there named exactly `ndk-build` (a shell
script), so this resolves fine. On Windows the NDK only ships `ndk-build.cmd` â€” no
extension-less `ndk-build` file exists at all â€” and Gradle's `Exec`/`ProcessBuilder`-based
task invocation doesn't do the `PATHEXT`-based extension resolution `cmd.exe` would do for a
bare `ndk-build`µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^m«ëŒ+Š×®º+º$zzb¥âG—VB–çFW&7F—fVÇ’âG&6¶VB2¶æ÷vâv6–æ6RF†RcãRãÖ–w&F–öà§7W&f6VB—B†DôDòæÖF3Bò3b’(	BF†—2—2F†Bf—‚à ¢¢¤6†ævS¢¢¢vWDæF´F—"‚’²†–b„÷W&F–æu7—7FVÒæ7W'&VçB‚’æ—5v–æF÷w2’"öæF²Ö'V–ÆBæ6ÖB"VÇ6P¢"öæF²Ö'V–ÆB"–ÂW6–ærw&FÆRw2÷vâ'VæFÆVB÷&ræw&FÆRæ–çFW&æÂæ÷2ä÷W&F–æu7—7FVÖ†Ç&VG¦öâF†R6Æ77F‚ÂæòæWrFWVæFVæ7’’Fò–6²F†R&–v‡BW‡FVç6–öâW"†÷7Bõ2à ¢¢¥v‡“¢¢¢F†—2v2F†RöæR6öæ7&WFR&Æö6¶W"¶VW–æræG&ö–B÷WBöb&V6‚f÷"ç–öæR'V–ÆF–æp¦g&öÒ6÷W&6Röâv–æF÷w2ÂæB6W&FVÇ’¶WB&÷fW26¶Ö7FW"w2÷vâæG&ö–B&6¶Væ@¢†&÷fW2×6¶Ö7FW"÷7&2×FW&’÷7&2öæG&ö–Bç'6’W‡Æ–6—FÇ’F—6&ÆVBöâv–æF÷w2f–¦6†V6µöæG&ö–Eöf–Æ&–Æ—G’‚–(	BæV—F†W"6ö×–ÆW2ç—F†–ærF†V×6VÇfW2öâv–æF÷w2–à¥6¶Ö7FW"w266RÂ'WB&÷F‚7F–ÆÂ6†VÆÂ÷WBFòF†—2W†7Bw&FÆRF6²Âv†–6‚v2f–Æ–æp§&Vv&FÆW72öbv†ò–çfö¶VB—B÷"v‡’à ¢¢¥fW&–f–6F–öã¢¢¢F†RF6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRãW‡G&7F–öà¢†v—BÇ’ÒÖ6†V6¶Â6öæf—&ÖVB’â¢¤æ÷BfW&–f–VBv–ç7B&VÂv–æF÷w2w&FÆRôäD²'V–ÆB¢¢(	@¦æòæG&ö–B4D²ôäD²FööÆ6†–âöâF†—2v–æF÷w2Ö6†–æRFò7GVÆÇ’–çfö¶RF†—2F6²æB6öæf—&Ğ¦æF²Ö'V–ÆBæ6ÖF'Vç27V66W76gVÆÇ’VæB×FòÖVæC²öæÇ’F†R¶÷FÆ–â7–çF‚æBF†R&V6öæ–ærF†@¦æF²Ö'V–ÆBæ6ÖF—2vVçV–æVÇ’v†BF†Rv–æF÷w2äD²6†—2&R6öæf—&ÖVBâv†öWfW"æW‡BF÷V6†W0¤æG&ö–B4’öâ&VÂv–æF÷w2'VææW"†÷"6¶Ö7FW"w2÷vâv–æF÷w2æG&ö–BF‚Âöæ6RVæ&Æö6¶V@¦'’F†—2’6†÷VÆBG&VBF†—22F†RF†–ærFò&R×fW&–g’f—'7B–b6öÖWF†–ær7F–ÆÂf–Ç2F†W&Rà ¢ÒÒĞ ¢22##bÓ’Ó(	BÖ6‚'VæFÆRÒÖæG&ö–B×&VÆV6V¢&VÂÂ6–væVBæG&ö–B&VÆV6R'V–Æ@ ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†'VæFÆVw2÷vâÒÖæG&ö–B×&VÆV6V ¦6öÖÖæD&wVÖVçFÂæBö'VæFÆUöæG&ö–Fw2æWræG&ö–E÷&VÆV6V&ÖWFW"ôw&FÆR×f&–ç@§6VÆV7F–öâ÷6–væ–ærÖ7&VFVçF–Â6†V6²’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãórÖ'V–ÆB×FööÆ–ærçF6†‡&VvVæW&FVB–âÆ6R(	B6ÖRf–ÆP§F†—2F6‚Ç&VG’6÷fW&VBf÷"Ö6‚'V–ÆFöÖ6‚'VæFÆVv÷&²’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢Ö6‚'VæFÆRÒÖæG&ö–FÇv—2'V–ÇBF†RFV'Vvw&FÆRf&–ç@¢†§6W'fö¦76VÖ&ÆSÄ&6ƒäFV'Vv’ÂWFò×6–væVB'’uw2÷vâF‡&÷vv’FV'Vr¶W’(	Bf–æRf÷"§W"Ö6öÖÖ—B6Öö¶R'V–ÆBÂW6VÆW72f÷"ç—F†–ærÖVçBFò7GVÆÇ’&R–ç7FÆÆVB÷WG6–FP¦F"–ç7FÆÆâW7G&VÒ6W'fòw2÷vâ7W÷'BöæG&ö–Bö²ö'V–ÆE7&2÷7&2öÖ–âö¶÷FÆ–âôæG&ö–Bæ·F ¢†vWE6–væ–æt¶W”–æföÂæ÷B'Böbç’F6‚(	BVçF÷V6†VBÂW7G&VÒÖWF†÷&VB’Ç&VG’†2¦6ö×ÆWFR6–væ–ærÖV6†æ—6Ó¢–bµõ4”tä”äuô´U•õ5Dõ$UõD†‡ÇW2õ5Dõ$Uõ56öôÄ”6ğ¦õ56’&R6WB–âF†RVçf—&öæÖVçBÂ6W'föö'V–ÆBæw&FÆRæ·G66öæf–wW&W2&VÂ&VÆV6V §6–væ–ær6öæf–rg&öÒF†VÓ²÷F†W'v—6RF†R&VÆV6V'V–ÆBG—RV–WFÇ’fÆÇ2&6²Fò6–væ–æp§v—F‚F†R¦FV'Vr¢¶W’âF†—2v2æWfW"v—&VBWFòç—F†–æröâF†RÖ6‚'VæFÆV6–FRÂæ@¦fÆÆ–ær&6²FòFV'Vr×6–væVB'&VÆV6R"'V–ÆB6–ÆVçFÇ’—2W†7FÇ’F†R¶–æBöbf–ÇW&RÖöFP§v÷'F‚&VgW6–ær–ç7FVBöbÆÆ÷v–ærà ¢¢¤6†ævS¢¢¢ÒÖæG&ö–B×&VÆV6V†Æ–â&ööÆVâfÆrÂæòæWr7&VFVçF–ÂfÆw2öb—G2÷vâ(	@§6VR&VÆ÷rf÷"v‡’’7v—F6†W2ö'VæFÆUöæG&ö–Fw2w&FÆRf&–çBg&öÒÄ&6ƒäFV'VvFğ¦Ä&6ƒå&VÆV6Vâ&Vf÷&RFö–ærç—F†–ærVÇ6RÂ6†V6·2µõ4”tä”äuô´U•õ5Dõ$UõD†—27GVÆÇ§6WB–âF†RVçf—&öæÖVçBæB&VgW6W2v—F‚6ÆV"W'&÷"–bæ÷BÂ&F†W"F†â&ö6VVF–ær–çFò¤w&FÆR'V–ÆBF†Bv÷VÆB6–ÆVçFÇ’FV'Vr×6–vâ—Bç—v’à ¢¢¥v‡’æòÒÖæG&ö–BÖ¶W—7F÷&Vö×77v÷&FöÖÆ–6ö×77v÷&FfÆw3¢¢¢ö'VæFÆUöæG&ö–Fw0¦VçbÒ6VÆbæ'V–ÆEöVçb‚–Ç&VG’FöW2÷2æVçf—&öâæ6÷’‚–(	BF†R¦6ÆÆ–ær¢&ö6W72w2gVÆÀ¦Vçf—&öæÖVçBÂ7&VFVçF–Ç2–æ6ÇVFVBÂÇ&VG’fÆ÷w2F‡&÷Vv‚FòF†Râöw&FÆWv7V'&ö6W72F†—0¦gVæ7F–öâ6†VÆÇ2÷WBFòÂ6–æ6RF†Bw2W†7FÇ’F†R6ÖRVçf—&öæÖVçBvWE6–væ–æt¶W”–æfö†§Æ–â7—7FVÒævWFVçb‚âââ–6ÆÂ’&VG2g&öÒöâF†Rw&FÆR6–FRâv†öWfW"–çfö¶W2Ö6‚'VæFÆP¢ÒÖæG&ö–BÒÖæG&ö–B×&VÆV6V†4’v÷&¶fÆ÷rv—F‚F†RBf'26WBg&öÒ6V7&WG2ÂFWfVÆ÷W §v—F‚F†VÒ6WB–âF†V—"6†VÆÂÂ&÷fW26¶Ö7FW"7væ–ærF†R&ö6W72v—F‚F†VÒ6WBf÷"§W7@§F†B6ÆÂ’6WG2F†÷6RBVçbf'2F—&V7FÇ’(	BF†W&Rw2æòÇVÖ&–ærf÷"F†—26öÖÖæBFòFò&W–öæ@¦6†V6¶–ærF†RöæRF†BÖGFW'2Ö÷7B†µõ4”tä”äuô´U•õ5Dõ$UõD†’—27GVÆÇ’F†W&Rà ¢¢¤æ÷BFöæR†W&R†ÆVgBFò&÷fW2Ö7F–öæõ&÷fW26¶Ö7FW"ÂW"DôDòæÖF3R“¢¢¢æV—F†W ¦6öç7VÖW"w2÷vâ7&VFVçF–Â§6÷W&6–ær¢—2v—&VBW–WB(	B&÷fW2Ö7F–öæv÷VÆBæVVBæWp¦æG&ö–BÖ¶W—7F÷&RÒ¦–çWG2†&6ScBÖFV6öF–ær¶W—7F÷&Rg&öÒv—D‡V"6V7&WB–çFòFV×f–ÆRÀ§6WGF–ærF†RBVçbf'2&Vf÷&R–çfö¶–ærF†—27F–öâw2÷vâÖ6‚'VæFÆV7FW’æB&÷fW0¥6¶Ö7FW"v÷VÆBæVVBT’FòvVæW&FR÷"–×÷'B¶W—7F÷&RÆö6ÆÇ’†æòv—D‡V"6V7&WG2FòÆVà¦öâF†W&R’æB6WBF†R6ÖRBVçbf'2&÷VæB—G2÷vâÖ6‚'VæFÆRÒÖæG&ö–F–çfö6F–öââF†—0¦VçG'’öæÇ’6÷fW'2F†RVæv–æRw2÷vâ†Æc¢v—fVâF†RBf'2ÂÒÖæG&ö–B×&VÆV6Væ÷r&öGV6W2§&VÂÂfW&–f–&Ç’×6–væVB&VÆV6Ræ¶–ç7FVBöb6–ÆVçFÇ’ÖFV'Vr×6–væVBöæRà ¢¢¥fW&–f–6F–öã¢¢¢÷7Eö'V–ÆEö6öÖÖæG2ç–'6W26ÆVæÇ’†7Bç'6V’ÂæBF†R&VvVæW&FV@§F6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRãW‡G&7F–öâ†6öæf—&ÖVB’â¢¤æ÷BfW&–f–V@¦v–ç7B&VÂ6–væVB'V–ÆB¢¢(	BæòæG&ö–B4D²ôäD²ôw&FÆRFööÆ6†–âöâF†—2v–æF÷w2Ö6†–æRÀ¦æBæò&VÂ¶W—7F÷&Rö7&VFVçF–Ç2öâ†æBFò7GVÆÇ’W†W&6—6RÒÖæG&ö–B×&VÆV6VVæBFòVæ@¦æB6öæf—&ÒF†R&W7VÇF–æræ¶—2vVçV–æVÇ’&VÆV6R×6–væVB†Rærâf–·6–væW"fW&–g–÷ ¦WV—fÆVçB’âv†öWfW"æW‡B†2&VÂæG&ö–BFööÆ6†–â²F‡&÷vv’¶W—7F÷&Rf–Æ&ÆR6†÷VÆ@¦FòF†BfW&–f–6F–öâ&Vf÷&RF†—2—26öç6–FW&VBFöæRÂæ÷B§W7B'F†R6öFR&VG26÷'&V7FÇ’â  ¢ÒÒĞ ¢22##bÓ’Ó(	Bf—‚Ö6‚'VæFÆRÒÖæG&ö–F7&6†–ærVæ6öæF—F–öæÆÇ’†æWfW"v÷&¶VBBÆÂ ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†'VæFÆVw2÷vâ&–æ'•öF—&6ö×WFF–öâÂæ@¦ö'VæFÆUöæG&ö–Fw24U%dõõD$tUEôD•&FW&—fF–öâ’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãórÖ'V–ÆB×FööÆ–ærçF6†‡&VvVæW&FVB–âÆ6R’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢6öÖÖæEö&6Rç–w26†&VB&–æ'•÷6VÆV7F–öæFV6÷&F÷"Æöv–0¦FVÆ–&W&FVÇ’6WG26W'fõö&–æ'’ÒæöæVf÷"ç’'6¶vVB"F&vWB„æG&ö–BÂ÷Vä†&Ööç’’(	@¦—G2÷vâ6öÖÖVçC¢'vR6âwB'Vâ—BF—&V7FÇ’âââFöW6âwB6VVÒfW'’W6VgVÂâ"'VæFÆR‚–w2÷và¦&öG’F†VâVæ6öæF—F–öæÆÇ’FöW2&–æ'•öF—"ÒF‚æF—&æÖR‡6W'fõö&–æ'’–æV"—G2fW'’F÷À¢¦&Vf÷&R¢WfW"&V6†–ærF†R—5öæG&ö–B‡6VÆbçF&vWB–'&æ6‚F†BF—7F6†W2Fğ¦ö'VæFÆUöæG&ö–F(	BÆ–âG—TW'&÷#¢W‡V7FVB7G"Â'—FW2÷"÷2åF„Æ–¶Rö&¦V7BÂæ÷@¤æöæUG—VÂVæ6öæF—F–öæÆÇ’ÂöâWfW'’6–ævÆRÖ6‚'VæFÆRÒÖæG&ö–F–çfö6F–öâÂv—F‚æğ¦–çWB6öÖ&–æF–öâF†Bfö–G2—Bâö'VæFÆUöæG&ö–F—G6VÆb†2F†RW†7B6ÖR'Vr6V6öæ@§F–ÖRÂöæRÆWfVÂFVWW#¢Vçe²%4U%dõõD$tUEôD•"%ÒÒF‚æF—&æÖR‡6W'fõö&–æ'’–Â6ÖRæöæVÀ§6ÖR7&6‚Â&V6†VBF†RÖöÖVçBF†Rf—'7BöæR—2f—†VBà ¢¢¥v‡’F†—2vVçBVææ÷F–6VBVçF–Âæ÷s¢¢¢F†RöæÇ’GvòF†–æw2F†BWfW"W†W&6—6VBæG&ö–@¦'VæFÆ–ær&Vf÷&RFöF’vW&Ræv—F‡V"÷v÷&¶fÆ÷w2öæG&ö–Bç–ÖÆ†6ÆÇ2Ö6‚'V–ÆBÒÖæG&ö–F ¢¦ÆöæR¢(	B—G2÷vâWFò×6¶vRF—7F6‚ÂÖ6‚6¶vRÒÖæG&ö–FÂW6W26ö×ÆWFVÇ¦F–ffW&VçBÂv÷&¶–ær6öFRF‚–â6¶vUö6öÖÖæG2ç–F†B6ÆÇ26VÆbævWEö&–æ'•÷F‚‚– ¦F—&V7FÇ’–ç7FVBöb&VÇ––æröâF†RFV6÷&F÷"w2&–æ'’×6VÆV7F–öâ÷WGWB’æB&÷fW26¶Ö7FW"w0¦æG&ö–Bç'6†6ÆÇ2Ö6‚'VæFÆRÒÖæG&ö–BÒÖ6öçFVçBÖF—"ÆF—7Cæ(	BF†R7GVÂ7&6†–ærF‚(	@¦'WBWfW'’&–÷"6W76–öâw2÷vâfW&–f–6F–öâæ÷FRf÷"F†B&6¶VæB6–BÂW‡Æ–6—FÇ’Â&æ÷@§fW&–f–VBv—F‚&VÂ'V–ÆBÂæòæG&ö–BFööÆ6†–â–âF†—2Vçf—&öæÖVçBâ"FöF’w0¦&÷fW2Ö7F–öæ4’FF—F–öâ‡6VRF†B&Wòw2÷vâ6öÖÖ—BFF–ær'V–ÆBÖæG&ö–F¦ö"’—2F†P¦f—'7BF†–ærF†BWfW"7GVÆÇ’&âÖ6‚'VæFÆRÒÖæG&ö–FVæBFòVæBöâ&VÂ–æg&7G'V7GW&P®(	B6Vv‡BF†—2öâF†RfW'’f—'7BGFV×Bà ¢¢¤6†ævS¢¢¢'VæFÆR‚–w2&–æ'•öF—&fÆÇ2&6²Fò6VÆbævWE÷F÷öF—"‚–v†Vâ6W'fõö&–æ'– ¦—2æöæV†öæÇ’W6VBF†W&R2¦FVfVÇB¢÷WGWBÆö6F–öâv†VâÒÖ÷WGWF—6âwBv—fVâ(	@¤æG&ö–Bw2÷vâ÷WGWB—2Çv—2G&—fVâ'’ö'VæFÆUöæG&ö–Fw2W‡Æ–6—B÷WGWEöF—&&Ğ§&Vv&FÆW72’âö'VæFÆUöæG&ö–FæòÆöævW"F÷V6†W26W'fõö&–æ'–BÆÃ¢—BvÆö'2f÷ ¦Æ–'6W'f÷6†VÆÂç6öVæFW"F&vWBóÇG&—ÆSâò¢¢öF—&V7FÇ’‡F†R6ÖRF†–æp¦æv—F‡V"÷v÷&¶fÆ÷w2öæG&ö–Bç–ÖÆw2÷vâ6†VÆÂ67&—BÇ&VG’FöW2v—F‚f–æF’Â&ö'W7BFğ§v†–6†WfW"FV'Vr÷&VÆV6R&öf–ÆR7V&F—&V7F÷'’6&vò7GVÆÇ’W6VBÂæBf–Ç2v—F‚6ÆV ¦ÖW76vR–ç7FVBöb&&R7F6²G&6R–bF†R'V–ÆB†6âwB7GVÆÇ’†VæVB–WBà ¢¢¥v‡’æ÷BF‡&VB'V–ÆE÷G—VF‡&÷Vv‚g&öÒF†RFV6÷&F÷"–ç7FVB¢¢†Ö—'&÷&–æp¦6¶vUö6öÖÖæG2ç–w2÷vâ6VÆbævWEö&–æ'•÷F‚†'V–ÆE÷G—RÂ6æ—F—¦W#Òâââ–6ÆÂ“¢'VæFÆVw0¦÷vâ6öÖÖöåö6öÖÖæEö&wVÖVçG2†&–æ'•÷6VÆV7F–öãÕG'VRÂ'V–ÆEö6öæf–wW&F–öãÕG'VR–6ÆÂFöW6âw@§&WVW7B'V–ÆE÷G—SÕG'VVÂ6òF†RFV6÷&F÷"6ö×WFW2æBF†Vâ§÷2¢'V–ÆE÷G—Vö6æ—F—¦W& ¦&6²÷WBöb·v&w6&Vf÷&R'VæFÆR‚–w2&öG’WfW"'Vç2(	B&V6÷fW&–ærF†VÒv÷VÆBÖVâ6†æv–æp§F†B6†&VBFV6÷&F÷"6ÆÂw2fÆw2Âv—F‚Væ6ÆV"VffV7G2öâWfW'’÷F†W"6öç7VÖW"ö`¦&–æ'•÷6VÆV7F–öæ†ÖFW&–ÆÇ’&—6¶–W"6†ævRFò6öÖÖæEö&6Rç–&–Ö—F—fR6WfW&À¦6öÖÖæG2FWVæBöâÂf÷"&VæVf—BvÆö&Ö–ærf÷"F†R7GVÂf–ÆRöâF—6²vWG2§W7B2vVÆÀ§v—F†÷WBF÷V6†–ær6†&VB–æg&7G'V7GW&RBÆÂ’à ¢¢¥fW&–f–6F–öã¢¢¢÷7Eö'V–ÆEö6öÖÖæG2ç–'6W26ÆVæÇ’†7Bç'6V’ÂæBF†R&VvVæW&FV@§F6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRãW‡G&7F–öâ†6öæf—&ÖVB’â¢¤6öæf—&ÖVBFòf—€§F†R7GVÂ7&6‚¢¢f–&÷fW2Ö7F–öæw2÷vâ4’‡&VÂÖ6‚'V–ÆBÒÖæG&ö–F°¦Ö6‚'VæFÆRÒÖæG&ö–BÒÖ6öçFVçBÖF—"Ç&VÂvÖSæVæBFòVæB’(	Bw&FÆRæ÷r'Vç2Fò6ö×ÆWF–öà¢‚$%T”ÄB5T44U54eTÂ"Âc"óc"F6·2’–ç7FVBöb7&6†–ær&Vf÷&RWfW"–çfö¶–ær—BâF†B6ÖR'Và¦–ÖÖVF–FVÇ’7W&f6VB¢§F†—&B¢¢Â6W&FR'Vr–âF†R6ÖRgVæ7F–öâ‡w&öær÷WGWBÖæ¶ ¦Æöö·WF‚’(	B6VRF†RæW‡BVçG'’&VÆ÷r(	B6òF†—27&6‚f—‚öâ—G2÷vâ7F–ÆÂv6âwBVæ÷Vv€¦f÷"vVçV–æVÇ’v÷&¶–ærÖ6‚'VæFÆRÒÖæG&ö–Fâæ÷B–WB6öæf—&ÖVBF†B7V66W76gVÆÇ’ÖÆö6FV@¦æ¶–ç7FÆÇ2÷'Vç26÷'&V7FÇ’öâFWf–6Rà ¢ÒÒĞ ¢22##bÓ’Ó(	Bf—‚ö'VæFÆUöæG&ö–Fw2w&öær÷WGWBÖæ¶Æöö·WF€ ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†ö'VæFÆUöæG&ö–Fw2'V–ÇEö·6vÆö"’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãórÖ'V–ÆB×FööÆ–ærçF6†‡&VvVæW&FVB–âÆ6RÂ6ÖRf–ÆR’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢gFW"âöw&FÆWr§6W'fö¦76VÖ&ÆSÅf&–çCæ6ö×ÆWFW2À¦ö'VæFÆUöæG&ö–FÆöö¶VBf÷"F†R'V–ÇBæ¶B6W'föö'V–ÆBö÷WGWG2ö²óÇf&–çCâò¢æ¶ ¢„uw2&W7VÖVB7FæF&BW"×f&–çB÷WGWBÆö6F–öâ’(	B&VÂ'V–ÆB‡F†R6ÖP¦&÷fW2Ö7F–öæ4’'VâF†B6öæf—&ÖVBF†R&Wf–÷W2VçG'’w27&6‚f—‚’&÷fVBF†—2vÆö"ÖF6†W0¢¦æ÷F†–ær¢ÂFW7—FRw&FÆR—G6VÆb&W÷'F–ær$%T”ÄB5T44U54eTÂ"æBÆÂc"F6·2W†V7WFVC ¢$æòæ²f÷VæBVæFW"6W'föö'V–ÆBö÷WGWG2ö²ô&ÓcDFV'VrògFW"7V66W76gVÂÖÆöö¶–ærw&FÆP¦'V–ÆBâ"6W'föö'V–ÆBæw&FÆRæ·G6w2÷vâ6÷”æE&VæÖSÅf&–çCä¶F6²†f–æÆ—¦VD'–F†P¦76VÖ&ÆRF6²’&VæÖW2F†R&VÂ÷WGWBFò6W'föæ¶æBÖ÷fW2—BFğ¦vWEF&vWDF—"†FV'VrÂ&6‚–(	BF‚6ö×WFVB'’vÆ¶–ær2&VçDf–ÆV2Wg&öÒF†Rw&FÆP¦ÖöGVÆR&ö÷B†'V–ÆE7&2ô–çFW&÷æ·F’Âv†–6‚FöW2¦æ÷B¢ÆæB&6²BF†—2gVæ7F–öâw2÷và§67&F6‚'V–ÆE÷&ö÷FF†Rv’f—'7B&VBöbF†B†VÇW"7VvvW7G2†6öæf—&ÖVBvWGF–ærF†—0¦W†7B&V6ö×WFF–öâw&öæröæ6RÇ&VG’Âv†–6‚—2v†BÖ÷F—fFVBæ÷BG'––ærFò†æB×&V6ö×WFP¦—B6V6öæBF–ÖR†W&R’à ¢¢¤6†ævS¢¢¢6V&6†W2'V–ÆE÷&ö÷F&V7W'6—fVÇ’f÷"6W'föæ¶'’æÖP¢†vÆö"ævÆö"‡F‚æ¦ö–â†'V–ÆE÷&ö÷BÂ"¢¢"Â'6W'föæ²"’Â&V7W'6—fSÕG'VR–’–ç7FVBöbG'––æp§Fò&V6öç7G'V7BV—F†W"uw27FæF&BÆö6F–öâ÷"F†R7W7FöÒF6²w2÷vâÖ÷fVB×FòÆö6F–öâ(	@§&ö'W7BFòW†7FÇ’v†W&Rw&FÆRw2÷vâ'V–ÆB67&—BFV6–FW2FòWB—BÂF†R6ÖR'6V&6‚f÷"F†P¦7GVÂf–ÆR–ç7FVBöb6ö×WF–ærv†W&R—B6†÷VÆBF†V÷&WF–6ÆÇ’&R"f—‚2F†R&Wf–÷W0¦VçG'’w2Æ–'6W'f÷6†VÆÂç6öÆöö·Wà ¢¢¥fW&–f–6F–öã¢¢¢÷7Eö'V–ÆEö6öÖÖæG2ç–'6W26ÆVæÇ’†7Bç'6V’ÂæBF†R&VvVæW&FV@§F6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRãW‡G&7F–öâ†6öæf—&ÖVB’âæ÷B–WB&R×'Và§F‡&÷Vv‚4’Fò6öæf—&ÒF†—27V6–f–2f—‚Æö6FW2F†R²6÷'&V7FÇ’(	BF†RæW‡B&÷fW2Ö7F–öæ §'Vâv–ç7BF†—26öÖÖ—B—2F†BfW&–f–6F–öâÂæ÷B–WBFöæR2öbF†—2VçG'’à ¢¢¤6÷'&V7F–öâ‡6ÖRF’Â6VRF†RæW‡BVçG'’“¢F†—2f—‚v2—G6VÆb7F–ÆÂw&öærâ¢¢F†P¦&÷fW2Ö7F–öæ4’'VâF†BfW&–f–VB—B6öæf—&ÖVBw&FÆR&VÆÇ’F–B&W÷'B$%T”ÄB5T44U54eTÂ ¦VæBFòVæBÂ'WBF†—2gVæ7F–öâ§7F–ÆÂ¢&–çFVB$æò6W'föæ²f÷VæBç—v†W&RVæFW ¦'V–ÆE÷&ö÷F"(	B'V–ÆE÷&ö÷Fv2F†Rw&öær6V&6‚66÷RÂæ÷B§W7BF†Rw&öærW†7BF‚v—F†–à¦—Bâ6VRF†RæW‡BVçG'’f÷"v†W&RF†Rf–ÆR7GVÆÇ’ÆæG2æBv‡’à ¢22##bÓ’Ó(	Bf—‚ö'VæFÆUöæG&ö–Fw2÷WGWBÖæ¶Æöö·W§66÷R¢‡&Wf–÷W2f—‚w2'V–ÆE÷&ö÷Fv2—G6VÆbw&öær ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†ö'VæFÆUöæG&ö–Fw2'V–ÇEö·6vÆö"’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãórÖ'V–ÆB×FööÆ–ærçF6†‡&VvVæW&FVB–âÆ6RÂ6ÖRf–ÆR’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢F†R–ÖÖVF–FVÇ’&V6VF–ærVçG'’w2f—‚(	B6V&6†–ær'V–ÆE÷&ö÷F §&V7W'6—fVÇ’f÷"6W'föæ¶(	Bv2fW&–f–VBv–ç7B&VÂ&÷fW2Ö7F–öæ4’'VâæBf÷Væ@¢¦Ç6ò¢w&öæs¢w&FÆR&W÷'FVB$%T”ÄB5T44U54eTÂ"ÂÆÂc"F6·2W†V7WFVBÂ6÷”æE&VæÖT&ÓcDFV'Vt¶ §&âv—F†÷WBW'&÷"ÂæB–WBæ÷F†–ærVæFW"'V–ÆE÷&ö÷FÖF6†VBâ&ö÷B6W6RÂf÷VæB'’7GVÆÇ§&VF–ær'V–ÆE7&2ô–çFW&÷æ·Fw2vWEF&vWDF—&F‚ÖF‚–ç7FVBöb77VÖ–ær'V–ÆE÷&ö÷Fv0¦6Æ÷6RVæ÷Vvƒ¢vWEF&vWDF—&vÆ·2&ö¦V7Bç&ö÷DF—"ç&VçDf–ÆRç&VçDf–ÆRç&VçDf–ÆV(	B0¦†÷2Wg&öÒF†Rw&FÆR×VÇF’×&ö¦V7B&ö÷BÂv†–6‚¦—2¢'V–ÆE÷&ö÷F—G6VÆ`¢†F&vWBóÇG&—ÆSâöæG&ö–BÖ'VæFÆRö’(	BÆæF–æröâF†—2gVæ7F–öâw2÷vâF÷öF—&†öæRÆWfVÀ¢¦&÷fR¢F&vWBöVçF—&VÇ’’ÂF†Vâ&6²F÷vâ6ö×ÆWFVÇ’F–ffW&VçB'&æ6ƒ ¦F&vWBóÇ'W7B×G&—ÆSâóÅ4U%dõõD$tUEôD•"w2&6VæÖSâ÷6W'föæ¶âF†Bw2F†RW†7B6ÖP¦F—&V7F÷'’Æ–'6W'f÷6†VÆÂç6öv2f÷VæB–âV&Æ–W"–âF†—26ÖRgVæ7F–öâ†6õöÖF6†W5³Öw0¦F—&æÖR’(	B§6–&Æ–ær¢öb'V–ÆE÷&ö÷FÂöæRÆWfVÂWæB&6²F÷vâÂæWfW"FW66VæFçBöb—Bà¤æòvÆö"&ö÷FVBB'V–ÆE÷&ö÷F6÷VÆBWfW"†fRf÷VæB—BÂ&Vv&FÆW72öbF†RvÆö"GFW&âW6VBà ¢¢¤6†ævS¢¢¢v–FVæVBF†R6V&6‚&ö÷Bg&öÒ'V–ÆE÷&ö÷FFòF&vWBóÇF&vWE÷G&—ÆSâö†’æRà¦6VÆbævWE÷F÷öF—"‚’÷F&vWBóÇF&vWE÷G&—ÆSæ’(	B7WW'6WBF†B6÷fW'2&÷F‚'V–ÆE÷&ö÷F ¢†F&vWBóÇG&—ÆSâöæG&ö–BÖ'VæFÆRòââæ’æBF†R&VÂÆæF–ær7÷@¢†F&vWBóÇG&—ÆSâóÆ'V–ÆB×G—Sâ÷6W'föæ¶’v—F†÷WBæVVF–ærF†RW†7B&VçDf–ÆV†÷6÷Vç@§Fò7F’6÷'&V7B–bW7G&VÒWfW"&W7G'V7GW&W2–çFW&÷æ·Fv–âà ¢¢¥fW&–f–6F–öã¢¢¢÷7Eö'V–ÆEö6öÖÖæG2ç–'6W26ÆVæÇ’†7Bç'6V’ÂæBF†R&VvVæW&FV@§F6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRãW‡G&7F–öâ†6öæf—&ÖVB’â4’&R×'Vâv–ç7@§F†—26öÖÖ—B—2F†R&VÂfW&–f–6F–öâÂVæF–ær2öbF†—2VçG'’à ¢22##bÓ’Ó(	B&W7F÷&RBöb‚&FVfVÇBÖöâW‡W&–ÖVçFÂvV"ÆFf÷&ÒfVGW&W2"G&÷VB'’F†RcãRãÖ–w&F–öâ‡&VÂ&Vw&W76–öâÂ6†—VB–âcãBã^(	7cãBã ¢¢¤f–ÆS¢¢¢6ö×öæVçG2ö6öæf–r÷&Vg2ç'6†&VfW&Væ6W3£¦6öç7EöFVfVÇB‚–’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãó×7F÷&vRÖæBÖ÷&–v–âçF6†‡&VvVæW&FVB–âÆ6RÂ6ÖP¦f–ÆR6V7F–öâ2F†RÆ–÷WEò¦&Vg2Ç&VG’F†W&R’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢F†R÷&–v–æÂ##bÓ‚ÓrVçG'’&VÆ÷r‚$FVfVÇBÖöâW‡W&–ÖVçFÂvV §ÆFf÷&ÒfVGW&W2"’fÆ—2‚&Vg2g&öÒW7G&VÒw2÷vâfÇ6VFVfVÇBFòG'VV(	B&VÂvÖP§ÆW6–&Ç’vçG2vV$tÃ"õvV$uRô–æFW†VDD"ö6Æ—&ö&Böæ÷F–f–6F–öç2öWF2âÂv†–6‚W7G&VÒ6†—0¦F—6&ÆVBVæF–ær—G2÷vâ7F&–Æ—¦F–öâ&ö6W72âF†R##bÓ’ÓBcãBã(i'cãRãÖ–w&F–öâVçG'¦&÷fR6Æ–×2F†—2v2&föÆFVB–çFò×7F÷&vRÖæBÖ÷&–v–âçF6†"(	B¢§F†B6Æ–Òv2öæÇ£"ó2G'VRâ¢¢6†V6¶–ærv†B7GVÆÇ’6öçF–æVBFöF’‡&ö×FVB'’&VÂW6W"&W÷'C¢§6¶vVBvÖR†—B–æFW†VDD"—2æ÷BFVf–æVFB'VçF–ÖR’f÷VæBöæÇ’BöbF†R÷&–v–æÂ‚&Vg0§&W6VçB†Æ–÷WEö6öÇVÖç5öVæ&ÆVFÂÆ–÷WEö6öçF–æW%÷VW&–W5öVæ&ÆVFÀ¦Æ–÷WEö775öGG%öVæ&ÆVFÂÆ–÷WEöw&–EöVæ&ÆVF’ÂÇW22æWrÖ–â×cãRã552&Vg2FFV@¦Æöæw6–FRF†VÒâF†R÷F†W"¢£BvW&R6–ÆVçFÇ’G&÷VBGW&–ærF†RÖ–w&F–öâw22×v’ÖW&vR¢¢æ@¦æWfW"&RÖFFVBÂ&WfW'F–ærFòW7G&VÒw2fÇ6V ¦FöÕö7–æ5ö6Æ—&ö&EöVæ&ÆVFÂFöÕöW†V5ö6öÖÖæEöVæ&ÆVFÂFöÕöföçFf6UöVæ&ÆVFÀ¦FöÕö–æFW†VFF%öVæ&ÆVFÂFöÕö–çFW'6V7F–öåöö'6W'fW%öVæ&ÆVFÀ¦FöÕöæf–vF÷%÷&÷Fö6öÅö†æFÆW'5öVæ&ÆVFÂFöÕöæ÷F–f–6F–öåöVæ&ÆVFÀ¦FöÕööfg67&VVåö6çf5öVæ&ÆVFÂFöÕ÷W&Ö—76–öç5öVæ&ÆVFÂFöÕ÷6æ—F—¦W%öVæ&ÆVFÀ¦FöÕ÷7F÷&vUöÖævW%ö•öVæ&ÆVFÂFöÕ÷vV&vÃ%öVæ&ÆVFÂFöÕ÷vV&wUöVæ&ÆVFÀ¦Æ–÷WE÷f&–&ÆUöföçG5öVæ&ÆVF(	Bæ÷F&Ç’¢¥vV$tÃ"æBvV$uR¢¢Âæ÷B§W7BF†Ræ'&÷vW ¤–æFW†VDD"7–×FöÒF†B7W&f6VB—Bà ¢¢¥v‡’F†—2vVçBVææ÷F–6VBf÷"vVV²7&÷72r&VÆV6W2‡cãBãRF‡&÷Vv‚cãBã“¢¢¢F†P¦Ö–w&F–öâw2÷vâ%fW&–f–6F–öâ"æ÷FR‡6VRF†R##bÓ’ÓBVçG'’&÷fR’W‡Æ–6—FÇ’fÆvvVBF†@§F†RF6‚×&V6öç7G'V7F–öâF‚—G6VÆb‡v†BFW7Bç–ÖÆöæG&ö–Bç–ÖÆ7GVÆÇ’W†W&6—6RÂæBv†@¦&VÂ6öç7VÖW"v÷VÆBvWBg&öÒF6†W2ö’v2&æ÷B–WBâââfW&–f–VBVæB×FòÖVæBÂ"öæÇ’F†B¦F—&V7B'&æ6‚'V–ÆB6ö×–ÆVB(	BæB6&vò'V–ÆF7V66W726âwB6F6‚G&÷VB&VbFVfVÇ@¦ç’Ö÷&RF†â—B6Vv‡BF†RVÆVÖVçDFF–×÷'BG&÷F†R6ÖRVçG'’Fö7VÖVçG3²&÷F‚&P§6–ÆVçBÂ6ö×–ÆRÖ6ÆVâ6öçFVçBÆ÷76W22×v’ÖW&vR6â&öGV6Rv—F‚¦W&ò6öæfÆ–7BÖ&¶W'2à¤æ÷F†–ær–âF†—2f÷&²w2÷vâFW7B7V—FRW†W&6—6W2&—2FöÕ÷vV&vÃ%öVæ&ÆVF7GVÆÇ’G'VV@§'VçF–ÖR"(	B—BFöö²&VÂvÖR6†—–ær&VÂ'V–ÆBFò7W&f6R—Bà ¢¢¤6†ævS¢¢¢&W7F÷&VBÆÂB&Vg2FòG'VV–â6öç7EöFVfVÇB‚–ÂV6‚v—F‚F†R6ÖR%&÷fW3 ¦öâ'’FVfVÇB(	BU…U$”ÔTåDÅõ$Te2'VæFÆR"6öÖÖVçB7G–ÆRÇ&VG’W6VBf÷"F†RBF†B7W'f—fVBÀ¦7&÷72×&VfW&Væ6–ærF†Rf—'7Bö67W'&Væ6R†FöÕö7–æ5ö6Æ—&ö&EöVæ&ÆVF’–ç7FVBöb&WVF–ærF†P¦gVÆÂ&F–öæÆRBF–ÖW2à ¢¢¥fW&–f–6F–öã¢¢¢'&6R÷&Vâ&Ææ6R6†V6¶VB&öw&ÖÖF–6ÆÇ’†æòÆö6Â'W7BFööÆ6†–â(	B6VP§F†—2f–ÆRw2÷F†W"VçG&–W2f÷"F†B7FæF–ærv“²F†R&VvVæW&FVBF6‚Æ–W26ÆVæÇ’Fò¦g&W6‚&—7F–æRcãRãW‡G&7F–öâöbÆÂrf–ÆW2F÷V6†W2ÂæBF†RÆ–VB&W7VÇB—0¦'—FRÖ–FVçF–6ÂFòF†Rv÷&¶–ærG&VR†6öæf—&ÖVBf–F–fbÒ×7G&—×G&–Æ–ærÖ7&ÂF†R5$Ä`¦F–ffW&Væ6R&V–ærF†—26æF&÷‚w2÷vâv—BÇ–&RÖæ÷&ÖÆ—¦–ærÆ–æRVæF–æw2Âæ÷B&VÀ¦6öçFVçBF–ffW&Væ6R’â&VÂ4’fW&–f–6F–öâ†FöW26¶vVBvÖRw2–æFW†VDD&övV$tÃ&7GVÆÇ§v÷&²æ÷r’—2VæF–ær2öbF†—2VçG'’(	B6VRDôDòæÖFf÷"F†RföÆÆ÷r×W&VÆV6RF†—2æVVG2à ¢22##bÓ’Ó(	B7GVÆÇ’6WB2·v–æF÷w5÷7V'7—7FVÒÒ'v–æF÷w2%Ö†æWfW"v2ÂFW7—FR&V–ærFW67&–&VB2ÆöBÖ&V&–ær–â2÷F†W"Æ6W2 ¢¢¤f–ÆS¢¢¢÷'G2÷6W'f÷6†VÆÂöÖ–âç'6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóÖFW6·F÷×6†VÆÂÖ6÷&RçF6†‡&VvVæW&FVB–âÆ6RÂ6ÖP¦f–ÆR6V7F–öâ’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢W7G&VÒÖ–âç'66WG2æòv–æF÷w5÷7V'7—7FVÖGG&–'WFRBÆÂ(	B§Æ–â6&vò'V–ÆFö'W7F6&–æ'’öâv–æF÷w2FVfVÇG2FòF†R¦6öç6öÆR¢7V'7—7FVÒÂÖVæ–æp¥v–æF÷w2GF6†W2&VÂ6öç6öÆRFòWfW'’–çfö6F–öâVæÆW726öÖWF†–ær6—2÷F†W'v—6Rà ¢¢¥F†Rv¢¢¢F‡&VR6W&FRÆ6W2–âF†—2f÷&²w2÷vâ6öFRæBFö72FW67&–&P¦÷'G2÷6W'f÷6†VÆÂöÖ–âç'62Ç&VG’6WGF–ær2·v–æF÷w5÷7V'7—7FVÒÒ'v–æF÷w2%Ö(	@¦FW6·F÷ö6Æ’ç'6‚'F†—2—22·v–æF÷w5÷7V'7—7FVÒÒ'v–æF÷w2%ÖÂ6òF†W&Rw2æò6öç6öÆRFğ§6VR7FFW'"–â"’ÂFW6·F÷öÆövv–ærç'6‚$6öÖ&–æVBv—F‚÷'G2÷6W'f÷6†VÆÂöÖ–âç'66WGF–æp¦2·v–æF÷w5÷7V'7—7FVÒÒ'v–æF÷w2%Öâââ"’ÂæBF†—2f–ÆRw2÷vâ%6–ævÆRÖW†V7WF&ÆR'VæFÆR"VçG'¦&÷fR†6Æ–Ö–ærF†RvVæW&FVBÆ’æW†VÆVæ6†W"7GV"W6W2'F†R6ÖRGG&–'WFP¦÷'G2÷6W'f÷6†VÆÂöÖ–âç'6Ç&VG’W6W2öâ6W'f÷6†VÆÂæW†V—G6VÆb"’â¢¤æöæRöbF†Bv2WfW §G'VR¢¢(	BöæÇ’F†R¦ÆVæ6†W"7GV"¢†Æ’æW†VÂ6W&FRÂF–ç’'W7B6÷W&6P¦÷7Eö'V–ÆEö6öÖÖæG2ç–vVæW&FW2æB6ö×–ÆW2öâF†RfÇ’’7GVÆÇ’†BF†RGG&–'WFS²F†P§&VÂVæv–æR&–æ'’—B7vç2†&–â÷6W'f÷6†VÆÂæW†V’æWfW"F–BÂ–âV—F†W"F†RcãBã÷"cãRã §G&VRÂvö–ærÆÂF†Rv’&6²FòF†—2GG&–'WFRw2fW'’f—'7BÖVçF–öâ‡cãBãw2BÖFBÖÖ6‚Ğ¦'VæFÆRÖ6öÖÖæBçF6†Âv†–6‚öæÇ’WfW"FFVB—BFòF†RvVæW&FVBÆVæ6†W"ÂæWfW"Fğ¦Ö–âç'6’à ¢¢¥v‡’F†—2vVçBVææ÷F–6VC¢¢¢Ö–âç'6w2W†—7F–ær5¶6fr‡F&vWEö÷2Ò'v–æF÷w2"•Ö&Æö6²Ç&VG¦6ÆÇ26öç6öÆS£¤g&VT6öç6öÆR‚––ÖÖVF–FVÇ’öâVçG'’‡W7G&VÒ6W'fòw2÷vâ6öFRÂVæÖöF–f–VB'§F†—2f÷&²’(	Bv—F†÷WBF†R7V'7—7FVÒGG&–'WFRÂv–æF÷w2WFòÖÆÆö6FW26öç6öÆRf÷"F†R6öç6öÆRĞ§7V'7—7FVÒ&–æ'’¦&Vf÷&R¢Ö–â‚–'Vç2ÂæBF†—26ÆÂ–ÖÖVF–FVÇ’FWF6†W2—BâF†RæWBVffV7@§&VG22&6öç6öÆRv–æF÷rfÆ6†W2f÷"â–ç7FçBÂF†VâF—6V'2"&F†W"F†â&6öç6öÆP§7F—2÷VâF†Rv†öÆRF–ÖR"(	BV7’FòÖ—72ÂæBV7’FòÖ—6GG&–'WFRFò6öÖWF†–ærVÇ6RÂ6–æ6R—@¦æWfW"&öGV6W2§W'6—7FVçB¢f—6–&ÆR6öç6öÆRFòö–çBBâWfW'’öæRöb6W'fòw2÷vâ×VÇF’Ğ§&ö6W726öçFVçB×&ö6W727vç2†6ö×öæVçG2ö6öç7FVÆÆF–öâ÷6æF&÷†–ærç'6w0¦7våö×VÇF—&ö6W76Âv†–6‚&RÖ–çfö¶W2F†R§6ÖR¢&–æ'’26†–ÆBf÷"V6‚æWp§F"ö–g&ÖR÷v÷&¶W"Âv—F‚æò5$TDUôäõõt”äDõv÷v–æF÷rÖ†–F–ærfÆröb—G2÷vâ’†—G2F†—26ÖP¦fÆ6‚×F†VâÖg&VR6WVVæ6R–æFWVæFVçFÇ’Â6òvRæVVF–ær6WfW&Â6öçFVçB&ö6W76W2&öGV6W0§6WfW&ÂF—7F–æ7BfÆ6†W2–âV–6²7V66W76–öâ(	B&W÷'FVB'’&VÂW6W"2'v†BÆöö²Æ–¶P§6WfW&ÂFW&Ö–æÂv–æF÷w2÷Væ–æræB6Æ÷6–ær"B7F'GWÂ&Vf÷&RF†RöæR&VÂvÖRv–æF÷p¦V'2â6öæf—&ÖVBF†—2—2vVçV–æVÇ’æWrÂæ÷B6öÖWF†–ær–çG&öGV6VB'’F†RcãRãÖ–w&F–öã ¦Ö–âç'6æWfW"†BF†—2GG&–'WFR–âF†RcãBãG&VRV—F†W"à ¢¢¤6†ævS¢¢¢FFVB2·v–æF÷w5÷7V'7—7FVÒÒ'v–æF÷w2%Ö2&VÂ7&FRÖÆWfVÂGG&–'WFR–à¦Ö–âç'6††&ÖÆW72æòÖ÷öâæöâÕv–æF÷w2F&vWG2ÂW"F†RGG&–'WFRw2÷vâFö7VÖVçFV@¦&V†f–÷"’(	BF†Rf—‚F†R7W'&÷VæF–ær6öÖÖVçG2Ç&VG’77VÖVBW†—7FVBà ¢¢¥fW&–f–6F–öã¢¢¢F†R&VvVæW&FVBF6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRã ¦W‡G&7F–öââæòÆö6Â'W7BFööÆ6†–âFò6ö×–ÆRv–ç7B‡7FæF–ærvÂ6VRF†—2f–ÆRw2÷F†W ¦VçG&–W2’(	B&VÂfW&–f–6F–öâ—2v–æF÷w24’÷&VÆV6R'V–ÆB7GVÆÇ’6öæf—&ÖVB6öç6öÆRÖfÆ6‚Ğ¦g&VRÂVæF–ær2öbF†—2VçG'’à ¢22##bÓ’Ó(	BF6†W2÷6W'fò×cãRãóBÖæG&ö–BçF6†v2Ö—76–ær6W'föö'V–ÆBæw&FÆRæ·G6VçF—&VÇ ¢¢¤f–ÆS¢¢¢7W÷'BöæG&ö–Bö²÷6W'föö'V–ÆBæw&FÆRæ·G6à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóBÖæG&ö–BçF6†‡&VvVæW&FVB–âÆ6R(	BæWrF–fb6V7F–öà¦FFVBÂæòW†—7F–ær6V7F–öâ6†ævVB’à ¢¢¥F†Rv¢¢¢F†—2f–ÆRw2÷vâÖ–w&F–öâÖÖ–ærF&ÆR‡6VRF†R##bÓ’ÓBVçG'’w2$æWrF6‚À¤öÆBF6‚72Â6÷fW'2"F&ÆR’6Æ–×2BÖæG&ö–F6÷fW'2ÆÂöb7W÷'BöæG&ö–Bö²ò¢¦À¦ÖVBg&öÒF†RöÆBcãBãF6†W2cöc(	B&÷F‚öbv†–6‚¦F–B¢F÷V6€¦6W'föö'V–ÆBæw&FÆRæ·G6†—G2&W5fÇVVö'V–ÆDfVGW&W2ç&W5fÇVW6&Æö6²Âv†–6‚vVæW&FW0§F†R"ç7G&–ærç6W'fõF†VÖT6öÆ÷&&W6÷W&6RÖ–ä7F—f—G’æ·F&VG2f÷"F†R÷F–öæÂ7FGW2Ö& ¦6öÆ÷"fVGW&R(	B6VRF†R##bÓ‚Ó3VçG'’&÷fR’âF†R&VvVæW&FVBcãRãF6‚æWfW"7GVÆÇ¦6'&–VBF†Bf–ÆRBÆÂ(	B6ö×&–ærWfW'’f–ÆRVæFW"7W÷'BöæG&ö–Bö²ö&WGvVVâg&W6€§&—7F–æRcãRãW‡G&7F–öâæBF†Rv÷&¶–ærG&VRFöF’f÷VæB—B2öæRöböæÇ’Gvò&VÀ¦F–ffW&Væ6W2‡F†R÷F†W"Âw&FÆWræ&FÂ—2W&R5$ÄbôÄbæö—6RÂÇ&VG’6÷'&V7FÇ’W†6ÇVFVB(	B6VP§F†B6ÖR##bÓ’ÓBVçG'’w2÷vâæ÷FRöâF†—2’à ¢¢¥v‡’F†—2vVçBVææ÷F–6VC¢¢¢F†RGvò7W7FöÖ—¦F–öç2F†B§vW&R¢&W6W'fV@¢†6W'f÷f–Wrö'V–ÆBæw&FÆRæ·G6ÂÖ–ä7F—f—G’æ·F’†VâFò&RVæ÷Vv‚f÷"&÷fW2Ö7F–öææ@¦&VÆV6Rç–ÖÆFòv÷&²Â6–æ6R&÷F‚'V–ÆBg&öÒ&VÂv—B6†V6¶÷WBöbF†—2&WòF—&V7FÇ’ÂæWfW §&V6öç7G'V7F–ærg&öÒF6†W2öâöæÇ’F†—2&Wòw2¦÷vâ¢æG&ö–Bç–ÖÆ(	Bv†–6‚F÷væÆöG2§&—7F–æRFræBÆ–W2F6†W2öF†R6ÖRv’FW7Bç–ÖÆFöW2(	B7GVÆÇ’W†W&6—6W2v†WF†W §F†RF6‚6WB—26ö×ÆWFRÂæB—B†FâwB&VVâ&R×'Vâ÷&R×fW&–f–VB6–æ6R&Vf÷&RF†—26W76–öâw0¤æG&ö–Bv÷&²7F'FVBâ—Bf–ÆVBF†RÖöÖVçB—BF–B'Vâv–âFöF“¢§6W'fö ¦6ö×–ÆT&ÓcDFV'Vt¶÷FÆ–âd”ÄTFv—F‚Vç&W6öÇfVB&VfW&Væ6Rw6W'fõF†VÖT6öÆ÷"vÂ6–æ6Rv—F†÷W@¦'V–ÆDfVGW&W2ç&W5fÇVW2ÒG'VVF†W&Rw2æò"ç7G&–ærç6W'fõF†VÖT6öÆ÷&f÷"Ö–ä7F—f—G’æ·Fw0¦W†—7F–ær†6÷'&V7FÇ’×F6†VB’&VfW&Væ6RFò&W6öÇfRv–ç7B(	B6öæf—&ÖVBf–&VÂ4’‡'Và¦3CScƒ“sv’à ¢¢¤6†ævS¢¢¢FFVBF†RÖ—76–ær6W'föö'V–ÆBæw&FÆRæ·G6F–fb‡Væ²‡F†R6ÖR6öçFVçBF†RöÆ@¦cöcF6†W26'&–VBÂ&RÖF–ffVBv–ç7B&—7F–æRcãRã’FòBÖæG&ö–BçF6†à ¢¢¥fW&–f–6F–öã¢¢¢F†R&VvVæW&FVBF6‚Æ–W26ÆVæÇ’Fòg&W6‚&—7F–æRcãRã ¦W‡G&7F–öâÂæBF†RÆ–VB&W7VÇB—2'—FRÖ–FVçF–6ÂFòF†Rv÷&¶–ærG&VR†F–f`¢Ò×7G&—×G&–Æ–ærÖ7&’â&VÂ4’&R×'VâöbæG&ö–Bç–ÖÆv–ç7BF†—26öÖÖ—B‡cãBã"’vVç@¦w&VVâÂ6öæf—&Ö–ærF†Rf—‚f÷"&VÂÂæ÷B§W7B'F†RF–fbÆöö·2&–v‡Bâ  ¢¢¤föÆÆ÷r×Wƒ##bÓ’Ó“¢F†RgVÆÂÇ’ÖWfW'’×F6‚Ö–â×6WVVæ6R×F†VâÖF–fb×F†R×v†öÆR×G&VP§726ö×ÆWFVB6ÆVââ¢¢Æ–VBWfW'’F6‚–âF6†W2÷6W'fò×cãRãö†2öbF†RcãBã ¦6öÖÖ—BÂ–æ6ÇVF–ærF†—2VçG'’w2÷vâf—‚æBF†RGvòVçG&–W2&÷fR—B’Fò6ö×ÆWFVÇ’g&W6€§&—7F–æRcãRãW‡G&7F–öâÂF†VâF–ffVBF†R&W7VÇBv–ç7BF†R7GVÂv÷&¶–ærG&VR7&÷70¦6ö×öæVçG2öÂ÷'G2öÂ—F†öâöÂ7W÷'BöæG&ö–BöÂæB7W÷'Bö6öçFVçB×6¶W"ö(	BWfW'¦f–ÆRç’F6‚F÷V6†W2âcBf–ÆW26†÷vVB&rF–fc²ÆÂcBvW&R6öæf—&ÖVBW&R5$ÄbôÄbæö—6P¢†F–fbÒ×7G&—×G&–Æ–ærÖ7&6†÷vVB¦W&òF–ffW&Væ6Rf÷"WfW'’öæR’ÂF†R6ÖR6æF&÷‚Öv—@¦Ç–Öæ÷&ÖÆ—¦W2ÖÆ–æRÖVæF–æw2'F–f7B2WfW'—v†W&RVÇ6R–âF†—2f–ÆRÂæ÷B&VÂ6öçFVç@¦Æ÷72â¦W&òvVçV–æR6öçFVçBF–ffW&Væ6W2f÷VæBâ6öÖ&–æVBv—F‚F†—26ÖRF’w2F‡&VR&VÂf—†W0¢‡F†—2VçG'’ÂF†R&Vg2ç'2VçG'’ÂæBF†Rv–æF÷w5÷7V'7—7FVÖVçG'’&÷fR’ÂF†—26Æ÷6W2÷WBF†P¢&—2F†RcãRãF6‚6WB7GVÆÇ’6ö×ÆWFR"VW7F–öâF†—2v†öÆR–çfW7F–vF–öâv26†6–ær(	@§F†W&R—2æòf÷W'F‚6–ÆVçFÇ’ÖG&÷VB7W7FöÖ—¦F–öâÇW&¶–ær–âF†R'G2öbF†RG&VRç’F6€§F÷V6†W2â†&W6÷W&6W2ò¦ö7W÷'Bö÷Væ†&Ööç’ò¢öÖVF–ò¦Ç6ò6†÷vVB2F–ffW&–ærÂ0¦W‡V7FVBæBæ÷B'Vs¢F†÷6R&R&–æ'’÷FW‡B×Æ6V†öÆFW"76WG2æWfW"F6‚×G&6¶VB–âF†P¦f—'7BÆ6R(	B6VRF†RÖ–w&F–öâVçG'’w2÷vâæ÷FR(	B6'&–VB÷fW"'’†æBÂæ÷B'’ç’F6‚â ¢22##bÓ’Ó(	B7G&—F†RæG&ö–BF÷vâFòvÖR6†VÆÃ¢æò'&÷w6W"6‡&öÖRÂæòFVfVÇBÖ'&÷w6W"6&–Æ—G’Âæò6WGF–æw2ô†—7F÷'’67&VVç0 ¢¢¤f–ÆW3¢¢¢7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âôæG&ö–DÖæ–fW7Bç†ÖÆÀ¦7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âö¦fö÷&r÷6W'fò÷6W'f÷6†VÆÂôÖ–ä7F—f—G’æ·FÀ¦7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–â÷&W2÷fÇVW2÷7G&–æw2ç†ÖÆ²FVÆWFVBVçF—&VÇ“ ¦6WGF–æw47F—f—G’æ·FÂ†—7F÷'”7F—f—G’æ·FÂ†—7F÷'”ÖævW"æ·FÂ†—7F÷'”VçG'’æ·FÀ¦†—7F÷'”—FVÒæ·FÂæBF†R'&÷uö&6¶ö'&÷uöf÷'v&Fö6æ6VÆö†—7F÷'–ö&Vg&W6†ğ¦6WGF–æw6öFVÆWFVG&v&ÆW2VæFW"&W2öG&v&ÆRöà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóBÖæG&ö–BçF6†‡&VvVæW&FVB(	B&WÆ6W2F†P¦æG&ö–DÖæ–fW7Bç†ÖÆöÖ–ä7F—f—G’æ·F6V7F–öç2ÂFG2æWr6V7F–öç2f÷"7G&–æw2ç†ÖÆæBV6€¦FVÆWFVBf–ÆR’à ¢¢¥W7G&VÒ&V†f–÷#¢¢¢6W'fö—2W7G&VÒ6W'fòw2÷vâ&VfW&Væ6R'6W'f÷6†VÆÂ"æG&ö–@¦'&÷w6W#¢Ö–ä7F—f—G–w266fföÆF†BgVÆÂ'&÷w6W"F÷&&ö&÷GFöÔ&&†âFG&W72Ö& ¦6V&6„&&Â&6²öf÷'v&B÷&Vg&W6‚'WGFöç2Â&öw&W727–ææW"Â6WGF–æw2ô†—7F÷'’–6öç2’Â¦†—7F÷'”ÖævW&W'6—7F–ærWfW'’f—6—FVBU$ÂFò¥4ôâf–ÆRÂ6WGF–æw47F—f—G–ğ¦†—7F÷'”7F—f—G–67&VVç2ÂæBæG&ö–DÖæ–fW7Bç†ÖÆFV6Æ&VBGvò%vV"'&÷w6W"–çFVçG2 ¦Æ–çFVçBÖf–ÇFW#æ2†d”Uvö%$õu4$ÄVf÷"‡GGö‡GG6öf–ÆVöFFö¦f67&—F66†VÖW0¦æBFW‡Bö‡FÖÆWF2âÖ–ÖRG—W2’(	BVæ÷Vv‚f÷"æG&ö–BFòöffW"F†—227—7FVÒ'&÷w6W ¦æB6WBÖ2ÖFVfVÇBÖ'&÷w6W"6æF–FFRâæöæRöbF†—2v2WfW"FG&W76VBv†VâæG&ö–B7W÷'Bv0¦FFVBFòF†—2f÷&²ƒ##bÓ‚Ó3ó’Ó’(	BF†RWV—fÆVçBFW6·F÷7W7FöÖ—¦F–öâ‡&VÖ÷f–ærF†P§FööÆ&"÷F"7G&—6òF†R¦FW6·F÷¢6†VÆÂÆöö·2Æ–¶RæF—fRv–æF÷rÂæ÷B'&÷w6W"’—2öæP¦öbF†—2f÷&²w2öÆFW7B6†ævW2‡6VRF†R##bÓ‚ÓRVçG'’æV"F†RF÷öbF†—2f–ÆR’Â'WBæö&öG§÷'FVBF†B6ÖR–çFVçBFòF†RæG&ö–BF&vWBÂ6–æ6R—Bw2âVçF—&VÇ’6W&FR¶÷FÆ–âôæG&ö–@¥T’Æ–W"Âæ÷B6†&VB6öFRà ¢¢¥&W÷'FVBF—&V7FÇ’'’W6W"¢¢FW7F–ær&VÂFWf–6R'V–ÆBƒ##bÓ’Ó“¢F†Rv0¦–ç7FÆÆ&ÆR2'&÷w6W"6†ö–6R†öffW&VB2'6WB2FVfVÇB'&÷w6W""÷F–öâ’Â6†÷vVBgVÆÀ¦FG&W72&"ö&6²öf÷'v&B÷6WGF–æw2ö†—7F÷'’T’öâF÷öbF†RvÖR6öçFVçBÂæBF†RvÖP¦6öçFVçB—G6VÆbf–ÆVBFòÆöBv—F‚$6÷VÆBæ÷BÆöBF†R&WVW7FVBvS¢÷Væ–ærf–ÆRf–ÆVB"(	@§F†BÆ7B'BGW&æVB÷WBFò&RâVç&VÆFVBÂW‡V7FVB7–×FöÒ‡F†R7V6–f–2²FW7FVB†Bæğ¦'VæFÆVB6öçFVçB(	B6VRF†RÖ6‚'VæFÆRÒÖæG&ö–BÒÖ6öçFVçBÖF—&æ÷FRÇ&VG’–à¦Ö–ä7F—f—G’æ·Fw2÷vâÆöEW&’6öÖÖVçB(	Bæ÷BæWr'Vr’Â'WBF†R'&÷w6W"Ö–FVçF—G’ö6‡&öÖP¦6ö×Æ–çBv2&VÂæB—2v†BF†—2VçG'’f—†W2à ¢¢¤6†ævS¢¢ ¢ÒæG&ö–DÖæ–fW7Bç†ÖÆ¢&VÖ÷fVB&÷F‚%vV"'&÷w6W"–çFVçG2"Æ–çFVçBÖf–ÇFW#æ&Æö6·2æBF†P¢6WGF–æw47F—f—G–ö†—7F÷'”7F—f—G–Æ7F—f—G“æFV6Æ&F–öç2(	BöæÇ’F†RÆ–à¢Ô”æöÄTä4„U&–çFVçBÖf–ÇFW"&VÖ–ç2Â6òF†—26âæòÆöævW"&RöffW&VB2'&÷w6W"÷"¢†æFÆW"f÷"&&—G&'’d”Uv–çFVçG2à¢ÒÖ–ä7F—f—G’æ·F¢66fföÆFw2F÷&&ö&÷GFöÔ&&†FG&W72&"Âæb'WGFöç2Â6WGF–æw2ğ¢†—7F÷'’–6öç2’&VÖ÷fVBVçF—&VÇ’(	BF†R6öçFVçBf–Wr†æG&ö–Ef–Wvw&–ær6W'fõf–Wv’æ÷p¢f–ÆÇ2F†Rv†öÆR67&VVâVFvRFòVFvRÂF†R6ÖR&æò'&÷w6W"6‡&öÖRÂWfW""–çFVçB2F†P¢FW6·F÷6†VÆÂw2÷vâFööÆ&"&VÖ÷fÂâ&6´†æFÆW&w2‡—6–6ÂövW7GW&R&6²Ö'WGFöâ&V†f–÷ ¢†6W'fõf–Wrævô&6²‚–’—2Væ6†ævVB(	BF†Bw2â–çWB†æFÆW"Âæ÷Bf—6–&ÆR6‡&öÖRÂæ@¢v6âwB'Böbv†Bv2&W÷'FVBâF†Ræ÷rÖFVB–b„–çFVçBä5D”ôåõd”UrÓÒ–çFVçBæ7F–öâ– ¢'&æ6‚‡Vç&V6†&ÆRöæ6RF†RÖæ–fW7BæòÆöævW"&÷WFW2d”Uv–çFVçG2†W&R’v2&VÖ÷fVBFöòÀ¢Æöærv—F‚F†R&W‡W&–ÖVçFÂfVGW&W2"6†&VE&VfW&Væ6W6FövvÆR†—G2öæÇ’T’v2F†Ræ÷rĞ¢FVÆWFVB6WGF–æw47F—f—G–(	BF†—2f÷&²Ç&VG’f÷&6W2F†RWV—fÆVçB'W7B×6–FP¢U…U$”ÔTåDÅõ$Te6'VæFÆRöâ'’FVfVÇB&Vv&FÆW72Â6VRF†R##bÓ‚ÓrVçG'’Â6òæ÷F†–æp¢—2Æ÷7B'’&VÖ÷f–ær6V6öæBÂ&VGVæFçBÂæG&ö–BÖöæÇ’FövvÆRf÷"—B’à¢ÒFVÆWFVB6WGF–æw47F—f—G’æ·Fö†—7F÷'”7F—f—G’æ·Fö†—7F÷'”ÖævW"æ·Fö†—7F÷'”VçG'’æ·Fğ¢†—7F÷'”—FVÒæ·F÷WG&–v‡B†6öæf—&ÖVBf–w&W¢&VfW&Væ6VBæ÷v†W&R÷WG6–FRV6‚÷F†W"æ@¢Ö–ä7F—f—G’æ·F’ÂÇW2F†Rræ÷rÖ÷'†æVBFööÆ&"–6öâG&v&ÆW2†6öæf—&ÖVBf–w&W¢¦W&ğ¢&VÖ–æ–ær"æG&v&ÆRâ¦ö"ç7G&–ærâ¦&VfW&Væ6W2’(	B+ZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æÚ±î¸Â¸­yêë¢°k¢G§¦*^µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^eal, if modest, APK size reduction,
  not just dead-code removal for its own sake, per the user's own ask.

**Verification:** re-diffed every one of the 15 touched/deleted files against a fresh pristine
`v0.5.0` download, confirmed the regenerated patch reproduces the exact same result
byte-for-byte (`diff --strip-trailing-cr`) for the 3 modified files and correctly deletes the
other 12; the combined patch applies cleanly to pristine (confirmed twice: once against each
of the 15 files individually, once against a completely fresh full pristine `v0.5.0` download).
`android.yml` (Gradle actually compiling `servoapp` with these Kotlin/manifest/resource
changes) went green. Still not verified: an actual device install confirming no browser
chrome/default-browser prompt shows up in practice, not just "it compiles."

**Correction (2026-09-11, same day, confirmed on a real device):** the browser-chrome removal
above was real and worked as intended. But loading the game itself still failed --
`file:///android_asset/www/index.html` (`MainActivity.kt`'s own `loadUri` call, present since
Android support was first added, completely unrelated to this same-day's chrome removal)
**never actually worked, with or without real bundled content.** See the next entry.

## 2026-09-11 â€” Extract bundled content to a real file instead of loading `file:///android_asset/...` (never worked)

**File:** `support/android/apk/servoapp/src/main/java/org/servo/servoshell/MainActivity.kt`.

**Patch:** `patches/servo-v0.5.0/0004-android.patch` (regenerated in place, same file section).

**Upstream behavior:** N/A â€” `loadUri("file:///android_asset/www/index.html")` is this fork's
own code, added when Android content-bundling was first built (2026-08-31 entry above), not
inherited from upstream Servo.

**The bug:** confirmed on a real device, with a real, verified-present bundled game
(`pixi-vn-react-template`, built via `roves-action`'s own `build-android` CI job â€” `unzip -l`
on the resulting `.apk` showed all 31 expected files including a valid `assets/www/index.html`):
"Could not load the requested page: Opening file failed." Root cause, found by actually
checking what handles a `file://` URL in this engine rather than assuming the Android
convention just works: `ports/servoshell/desktop/protocols/file.rs` opens `file://` URLs with
a plain `std::fs::File::open` â€” grepping the entire engine source confirms `android_asset`,
`AAsset`, and `AssetManager` appear **nowhere** in `components/` or `ports/`. `android_asset`
is a WebView/Chromium-specific virtual-path convention its own internal asset resolver
understands; a generic POSIX file-open has no idea what it means and just fails, exactly as it
would for any other nonexistent path â€” meaning this URI scheme could never have resolved to
real content, on any build, regardless of whether `--content-dir` was ever provided. This
went unnoticed for as long as it did because the *content-less* case (a plain engine-shell
build, no bundled content) was expected to show exactly this same failure â€” see this same
`MainActivity.kt`'s own comment predating this fix â€” so a real bundled build failing
identically read as "business as usual" rather than a distinct, real bug, until someone
actually tested one.

**Change:** new `extractBundledContent`/`copyAssetTree` (in `MainActivity.kt`) copy the
`assets/www/` tree out to `filesDir/www` (always private, always writable, no runtime
permission needed) via Android's own `AssetManager` API the first time a given app build
(tracked by `versionCode` in a marker file, so an update re-extracts) runs, then `loadUri`
loads a real `file://` path from there instead. Mirrors the desktop shell's own "extract
packed content once, then load a real path" shape (see the 2026-08-07/08 entries on that) --
simpler here since Android's own `mach bundle` path has no compression step to reverse, just a
plain asset-to-file copy.

**Verification:** the regenerated patch applies cleanly to a fresh pristine `v0.5.0`
extraction (confirmed against every file `0004-android.patch` now touches, not just this one).
No local Kotlin/Gradle toolchain to compile against (same standing gap as every other Android
change in this file) â€” real verification is a CI Android build compiling this successfully,
and ultimately a real device actually loading the bundled game this time, both pending as of
this entry.

**Correction (2026-09-12, confirmed on a real device):** progress, not a full fix. The
"Opening file failed" error is gone, but the real device instead showed a fully blank white
screen â€” see the next entry. The status/navigation bars staying visible over the game (a
second, unrelated issue reported in the same message) is covered in the entry after that.

## 2026-09-12 â€” Rewrite root-relative asset paths in the extracted HTML (a real `game://`-on-Android port is the complete fix, not attempted here)

**File:** `support/android/apk/servoapp/src/main/java/org/servo/servoshell/MainActivity.kt`
(`copyAssetTree`'s new `.html` branch, and the new `ROOT_RELATIVE_ATTR` regex).

**Patch:** `patches/servo-v0.5.0/0004-android.patch` (regenerated in place, same file section).

**The bug:** confirmed on a real device: after the previous entry's fix, the game's own
`index.html` loads (no more "Opening file failed"), but the screen was fully blank white.
Root cause: `index.html` (this specific template, and any typical Vite-built SPA using the
default `base: '/'`) references its own bundle with **root-relative** paths --
`<script src="/assets/index-....js">`, `<link href="/assets/index-....css">`. Loaded via a
real `file://<extracted-dir>/index.html` URL (the previous entry's own fix), a root-relative
reference resolves against the *entire device filesystem's* root, not the extracted
directory -- so the very first `<script>` tag 404s, no JS ever runs, and the page is
permanently blank. This is the *exact* problem the desktop shell's own `game://content/`
virtual-origin protocol (`ports/servoshell/desktop/protocols/game.rs`) exists to solve --
see that file's own doc comment, "Root-absolute asset references... have the exact same
underlying problem" -- but that protocol is only wired up for the desktop target
(`ports/servoshell/desktop/` is `#[cfg(not(target_os = "android"))]`; the Android/OpenHarmony
EGL app (`ports/servoshell/egl/app.rs`) builds a plain `ServoBuilder::default()` with no
custom protocol registry at all) -- it was never ported to Android when Android support was
added.

**Change (a deliberately narrower stand-in, not the full port):** `.html` files get their
`src="/...")`/`href="/...")` attributes rewritten to `src="./...")`/`href="./...")` at
extraction time (a regex excluding protocol-relative `//host/...` references, which must stay
untouched) -- fixes the specific symptom that produced a fully blank screen (the top-level
script/stylesheet never loading at all).

**What this does *not* fix, on record rather than silently assumed away:** a client-side
router in "history" mode (React Router, TanStack Router, Vue Router, ...) still sees
`location.pathname` as the real device filesystem path at boot, not `/` -- the *other* half of
what `game://content/` solves (see that file's own doc comment on why booting at the root,
not `/index.html`, matters for exactly this). A game using one may load its JS successfully
after this fix and still show its own "not found" fallback instead of real content. The
complete fix is porting `game://` (and its `PackedContent` dependency) out of the
`desktop`-only module tree into something both targets can share, and wiring
`ServoBuilder::protocol_registry(...)` into `egl/app.rs`'s own Android/OpenHarmony
initialization -- a real, higher-risk engine change (touches code shared with OpenHarmony,
unverifiable locally, and previously unexercised on this target at all) deliberately not
attempted in this same sitting as the lower-risk, Kotlin-only fix above. Whoever picks this up
next: `pixi-vn-react-template` (the real-world test case this was diagnosed against) does use
a client-side router, so it's a good, real test case for confirming whether this residual gap
actually manifests in practice, not just a theoretical concern.

**Verification:** the regex's exact behavior was simulated against this template's real
`index.html` content (Python's `re`, equivalent semantics for this pattern) before writing the
Kotlin version -- correctly rewrites `src="/assets/..."`/`href="/favicon.ico"`/
`href="/manifest.webmanifest"` while leaving `href="//external.example.com/..."`/
`href="https://..."` untouched. The regenerated patch applies cleanly to a fresh pristine
`v0.5.0` extraction. No local Kotlin toolchain â€” real verification is a CI Android build
compiling this, and a real device confirming the screen is no longer blank, both pending.

**Correction (2026-09-12, same day): this narrow stand-in wasn't enough, and was superseded
by a real engine-level fix the same day.** Confirmed on the same real device: after this fix,
the page rendered and its own client-side router worked fine (the residual gap flagged above
never actually manifested for this template) â€” but a *runtime-generated* asset reference from
the game's own JS (`[Loader.load] Failed to load file:///assets/images/main-menu-....webp`,
a PixiJS resource loader call, not anything in the static HTML this entry's regex could ever
see) hit the exact same root-relative-path problem one level deeper. A text-rewrite approach
can never fully close this class of bug â€” JS can reference paths from string concatenation,
template literals, or a bundler-embedded manifest, none of which a regex over the *shipped*
files can reliably catch. See the next entry for the real fix this prompted: porting the
already-proven desktop `rebase_to_content_root` mechanism to Android at the protocol level
instead of patching individual files. The `.html`-rewrite code in this entry is **superseded,
not removed** â€” it's still harmless/inert (a plain relative path is still a plain relative
path once the real fix makes `file://` resolution itself correct), so it was left in place
rather than torn out for a one-day-old feature; a future cleanup pass could remove it once the
protocol-level fix has more real-world mileage.

## 2026-09-12 â€” Port `file:`'s content-root rebasing to Android (the real fix, not the HTML-only stand-in two entries up)

**Files:** `ports/servoshell/Cargo.toml` (`headers` moved to an unconditional dependency),
`ports/servoshell/lib.rs` (new `mod protocols;`), `ports/servoshell/protocols/{mod,file,
packed_content}.rs` (**new module** â€” relocated from `desktop/protocols/`, verbatim, see
below), `ports/servoshell/desktop/protocols/{mod,game}.rs` (updated `use` paths only, no
logic change), `ports/servoshell/desktop/app.rs` (one `use` path qualified), `ports/servoshell/
egl/app.rs` (new `file:` protocol registration).

**Patches:** `patches/servo-v0.5.0/0001-desktop-shell-core.patch` (Cargo.toml, desktop/app.rs),
`0002-desktop-protocols.patch` (desktop/protocols/{mod,game}.rs â€” and no longer carries
`file.rs`/`packed_content.rs`, which moved out), and a **new**
`0015-shared-file-protocol.patch` (lib.rs, the new `protocols/` module, egl/app.rs).

**Why the real fix was smaller than expected:** the previous entry considered porting the
full `game://content/` virtual-origin protocol to Android (the complete fix for *both*
root-relative asset paths *and* a client-side router's `location.pathname` mismatch at boot)
and deliberately didn't attempt it â€” bigger, touches code shared with OpenHarmony, higher
risk. But the real device test that day showed the router problem never actually happened for
the test game â€” only asset-path resolution did. Re-reading `ports/servoshell/desktop/
protocols/file.rs` (Servo's own plain `file:` handler, not `game:`) found it already has a
`rebase_to_content_root` fallback solving *exactly* the asset-path half, on its own, already
proven in production for the desktop shell â€” a much smaller, lower-risk change than the full
`game://` port: no virtual origin, no SPA-fallback-to-entry-HTML logic, just "if the literal
path doesn't resolve, retry it relative to the initial launch URL's own directory instead of
the OS filesystem root." `file.rs`/`packed_content.rs` had zero desktop-specific dependencies
(confirmed by reading both files fully) â€” the only real blocker was `headers` (used by
`file.rs` for HTTP Range support) being gated to `not(any(target_os = "android", target_env =
"ohos"))` in `Cargo.toml`; moved to the unconditional dependency block instead.

**Change:** relocated `file.rs`/`packed_content.rs` verbatim from `desktop/protocols/`
(`#[cfg(not(any(target_os = "android", target_env = "ohos")))]`, so never compiled for
Android at all) to a new crate-root `protocols` module compiled for every target â€” `game.rs`
(which stays desktop-only; the *other*, harder half of this fix, still not attempted) updated
to import `PackedContent` from the new location. `egl/app.rs`'s `App::new` â€” which builds a
plain `ServoBuilder::default()` with no protocol registry at all today â€” now computes an
`initial_file_path` from its own parsed `initial_url` (moved earlier in the function for this)
and registers `crate::protocols::file::FileProtocolHandler::new(...)` on the builder before
`.build()`, mirroring `desktop/app.rs`'s own registration of the same handler exactly.
`PackedContent::resolve` already returns `None` harmlessly on Android (no `--content-compress`
support in `mach bundle --android` yet, so no `.roves-content-source` marker it looks for is
ever written there) â€” this port doesn't change that, only makes the *rebasing* fallback (which
doesn't depend on packed content at all) available too.

**Verification:** every touched/moved/added file re-diffed against a fresh pristine `v0.5.0`
download and confirmed byte-identical to the working tree; all three regenerated/new patches
apply cleanly in isolation. No local Rust toolchain to compile against (standing gap, see
this file's other entries) â€” real verification is `android.yml` and `test.yml` (to confirm
the `desktop` target â€” which this change also touches via the `use` path fix and the newly-
unconditional `headers` dependency â€” still builds correctly) both going green, and ultimately
the same real device confirming the PixiJS asset-loading error is gone, both pending as of
this entry.

**Correction (2026-09-12, later the same day):** the "the router problem never actually
happened for the test game" claim above was wrong. Once this fix shipped (v0.4.16) and the
PixiJS asset-loading error it targeted was gone, the real device's *next* screenshot showed a
plain black screen reading "Not Found" â€” the exact router-fallback symptom this entry said
didn't apply. It was never actually absent; it was masked by the more visually prominent
asset-loading error being on top of it in the previous screenshot (a root-level loader/asset
prefetch still runs and produces visible network activity even when no leaf route matches, so
the app *looked* like it was doing something before this fix, for the wrong reason). The full
`game://` port this entry talked itself out of is the real fix â€” see the next-but-one entry
below, done immediately after this was confirmed.

## 2026-09-12 â€” Hide the Android status/navigation bars (immersive mode)

**File:** same `MainActivity.kt` as the two entries above (`hideSystemBars`, called from both
`onCreate` and `onResume`).

**The bug:** reported in the same real-device test as the two entries above: both the status
bar and the navigation bar stayed visible on top of the game, another "looks like a browser/
generic app, not a game" gap never addressed when Android support was added â€” same family of
issue as the browser-chrome removal a day earlier, just a system-level UI layer instead of
this app's own.

**Change:** `WindowCompat.setDecorFitsSystemWindows(window, false)` +
`WindowInsetsControllerCompat.hide(WindowInsetsCompat.Type.systemBars())`, with
`BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE` (a player can still swipe from an edge to reveal them
temporarily â€” standard Android "immersive" behavior, not a fully locked kiosk mode that would
block that entirely). Re-applied in `onResume` too, not just `onCreate` â€” hidden system bars
are well-documented to be able to reappear on their own after the app loses and regains focus.

**Verification:** `androidx.core`'s `WindowCompat`/`WindowInsetsControllerCompat` APIs are
already transitively available (this same file already imports
`androidx.core.content.getSystemService`, from the same artifact) â€” no new Gradle dependency
needed. Initially shipped with a typo (`BEHAVIOR_SHOW_TRANSIENT_BY_SWIPE`, missing "BARS") that
`android.yml` caught as a real compile error â€” fixed and re-verified via CI before release.

## 2026-09-12 â€” Port the full `game://content/` protocol to Android (the router fix the previous entry talked itself out of)

**Files:** `ports/servoshell/protocols/{mod,game}.rs` (`game.rs` relocated here, verbatim,
from `desktop/protocols/`, joining `file.rs`/`packed_content.rs` which already moved in the
entry above), `ports/servoshell/desktop/protocols/mod.rs` (drops `game` too â€” nothing left in
`desktop/protocols/` that's Android-relevant anymore), `ports/servoshell/desktop/app.rs` and
`ports/servoshell/desktop/bundle_launch.rs` (`use` paths re-qualified to `crate::protocols::
game`, no logic change), `ports/servoshell/egl/app.rs` (registers `game:` and boots at
`game://content/` for any bundled launch, not just `file:`).

**Patches:** `0002-desktop-protocols.patch` (drops `game.rs` entirely â€” nothing of it left
under `desktop/protocols/`), and `0015-shared-file-protocol.patch` renamed to
`0015-shared-content-protocols.patch` (now carries `protocols/game.rs` alongside `file.rs`/
`packed_content.rs`, and the fuller `egl/app.rs` boot-URL logic below) â€” `0001` needed no
further change beyond the previous entry's.

**The bug (confirmed on a real device, see the correction above):** `egl/app.rs` was booting
Android at the literal `file://<absolute path>/index.html` URL. Under that URL,
`window.location.pathname` is the real OS path, never `/` â€” so a client-side history router
(this test game uses one) can never match its own root route and falls back to its own "Not
Found" page, exactly as documented in `ports/servoshell/desktop/protocols/game.rs`'s own doc
comment and `desktop/bundle_launch.rs`'s `game_content_url` â€” a bug this project already hit
and fixed *once*, for the desktop shell, in 2026-08-29 (see that entry) â€” Android was simply
never given the same fix when it was added.

**Why not just the `file:` rebase from the previous entry:** `rebase_to_content_root` only
rewrites requests for individual *files* that fail to resolve (assets, sub-resources). It has
no way to change what `location.pathname` the *document itself* was loaded at â€” the router
problem isn't a missing asset, it's the top-level navigation URL having the wrong shape
entirely. Only a real distinct origin (`game://content/`, `ImmutableOrigin::
new_opaque_for_game_content`) makes `location.pathname` come out as `/` at boot.

**Change:** relocated `game.rs` out of `desktop/protocols/` the same way `file.rs`/
`packed_content.rs` already were, into the shared `protocols/` module (compiled for every
target). `egl/app.rs`'s `App::new` now mirrors `desktop/bundle_launch.rs`'s own logic instead
of only registering `file:`: it computes `initial_file_path` from the parsed initial URL (as
before), but now also checks whether that resolves to a real file â€” if so, it registers
`game:` too (`GameProtocolHandler::new(content_root, entry_html)`, content root = the file's
parent directory) and overrides the boot URL to `game://content/` instead of the raw `file:`
path; every other case (no bundled file, e.g. `about:blank`) falls back to the original
`raw_initial_url` unchanged. `ServoBuilder` already had `protocol_registry()` as a builder
method (used by `desktop/app.rs` already) â€” no new engine API needed.

**Verification:** every touched/moved/added file re-diffed against a fresh pristine `v0.5.0`
download (for the two files that exist upstream, `lib.rs`/`egl/app.rs`; `game.rs`/`mod.rs`
are Roves-original, no upstream counterpart) and confirmed byte-identical to the working
tree; `0002-desktop-protocols.patch` and `0015-shared-content-protocols.patch` (installed
under its new name) both apply cleanly against fresh pristine in isolation. No two patches
in `patches/servo-v0.5.0/` touch the same file, so there's no cross-patch ordering risk from
this relocation. No local Rust toolchain â€” real verification is `android.yml` + `test.yml`
both green, and ultimately the real device confirming the "Not Found" page is gone, both
pending as of this entry.


## 2026-09-13 â€” Mobile pivots from Servo to native WebView (Android WebView + iOS WKWebView)

**Files:** too many to list individually â€” see `patches/servo-v0.5.0/0016-mobile-native-webviews.patch`'s
own file list (17 files: `python/servo/post_build_commands.py`, `support/MOBILE.md` (new),
`support/android/apk/servoapp/{build.gradle.kts,src/main/AndroidManifest.xml,src/main/java/
org/servo/servoshell/{MainActivity.kt,MediaSession.kt (deleted),RovesSplashView.kt (new)},
src/main/res/values{,-v31}/styles.xml}`, `support/android/apk/settings.gradle.kts`,
`support/android/apk/servoview/**` (all 4 files deleted), `support/ios/{App.swift,bundle.py}`
(new), `tests/mobile/test_packaging.py` (new)), plus `.github/workflows/android.yml`
(rewritten â€” no patch, this repo's own CI file, not vendored) and `README.md`.

**Why:** Servo's Android JNI bridge (`servoview`, `egl/android/`) works, but every web
platform feature a mobile game gets is whatever this fork's own Servo build supports â€”
narrower than a real WebView, and iOS has no Servo/JNI-equivalent port at all (Servo doesn't
target iOS). Android's `WebView` and iOS's `WKWebView` are both full, independently-updated
browser engines already present on every device â€” swapping to them for mobile trades "one
engine everywhere" for "desktop keeps the custom engine, mobile gets the platform's own,
already-maintained one," and unblocks iOS entirely. Desktop is unaffected: it still uses
Servo, unchanged.

**What changed on Android:** `MainActivity.kt` rewritten from the Compose-based browser-chrome
shell (see this file's earlier 2026-09-11/12 entries) to a plain `WebView` + `WebViewAssetLoader`
(`androidx.webkit`) serving the game at `https://appassets.androidplatform.net/` â€” a real
origin, not `file://`, so `location.pathname` is `/` at boot (client-side routers match) and
root-relative asset references resolve correctly, the same class of fix `ports/servoshell/
protocols/game.rs` gives desktop/embedded via `game://content/` (see this file's 2026-09-12
entries on that). Missing files 404 instead of hitting the real network. `_bundle_android`
(`post_build_commands.py`) no longer compiles anything: no `mach build --android` prerequisite,
no NDK, no `libservoshell.so` â€” it copies `--content-dir` into a scratch Gradle project's own
`assets/www/` and runs `:servoapp:assembleXDebug`/`Release` directly, the exact same `--android-
app-name`/`-orientation`/`-theme-color`/`--android-release` surface as before. `servoview/`
(Servo's own JNI bridge, `JNIServo.kt`/`Servo.kt`/`ServoView.java`) is deleted outright â€”
`settings.gradle.kts` no longer includes it, and nothing else referenced it once `MainActivity`
stopped depending on it. `MediaSession.kt` (a Servo/JNI-specific media-notification integration)
is deleted too; a plain in-page `<video>`/`<audio>` element doesn't need a custom Android-side
media session the way a full custom player did. A native splash (`RovesSplashView.kt`, a plain
`View.onDraw` â€” black background, engine icon, Metal Mania wordmark, animated loading bar) covers
the WebView's own load, removed once `onPageFinished` + `postVisualStateCallback` both fire and
at least 500ms has elapsed (so a near-instant load doesn't just flash); an Android 12+
`windowSplashScreenAnimatedIcon` system splash (`values-v31/styles.xml`) covers the very first
frame before that. Both generated from the same `resources/servo_1024.png` + Metal Mania font
already used for the *desktop* boot splash (`build.gradle.kts`'s new `generateRovesBrandAssets`
Gradle task) â€” one branding asset, reused, not a separate Android-specific one.

**What's new on iOS:** `support/ios/{App.swift,bm«ëŒ+Š×®º+º$zzb¥çVæFÆRç—Ö(	Bâ–æ—F–ÂtµvV%f–Wv6öçF–æW ¢†'VæFÆRç–7FvW2ç7v–gF²ÒÖ6öçFVçBÖF—&w26öçFVçB²'&æF–ær76WG2–çFòà¥†6öFTvVâ&ö¦V7Bæ§6öæ²'V–ÆB÷6–vâ†Vç2–â†6öFRÂÖ4õ2ÖöæÇ’Âæ÷Bv—&VB–çFòÖ6‚'VæFÆV ¦BÆÂ–WB(	B&VÂvÂ6VR$¶æ÷vâv2"&VÆ÷r’âæ÷Bv—&VBF‡&÷Vv‚Ö6†ö÷7Eö'V–ÆEö6öÖÖæG2ç–à ¢¢¤6÷'&V7F–öâÂ6ÖRF“¢¢¢F†Rf—'7BfW'6–öâöbF†—26öçF–æW"W6VBtµvV%f–WræÆöDf–ÆUU$Æ ¢‡Æ–âf–ÆS¢òö’(	BF†R¦W†7B¢'VrF†RæG&ö–B&w&‚&÷fR§W7BFW67&–&VBf—†–ærÂöâF†P¦öæRÆFf÷&Ò–âF†—26ÖR6†ævRF†Bv2w&—F–æræWr6öFRÂæ÷B÷'F–ærâW†—7F–ærf—‚â&VÀ¦FWf–6RFW7F–ærv6âwBf–Æ&ÆRFò6F6‚—B&Vf÷&RÖW&vS²6Vv‡B'’6öFR&Wf–WrÂG&VFVBv—F€§F†R6ÖR6W&–÷W6æW722ç’÷F†W"&VÂÖFWf–6RÖ6öæf—&ÖVB'Vr–âF†—2f–ÆRâf—†VBF†R6ÖRF“ ¦ç7v–gFæ÷r†2—G2÷vâvÖU66†VÖT†æFÆW&†tµU$Å66†VÖT†æFÆW&’Â6W'f–ærF†RvÖR@¦vÖS¢òö6öçFVçBö(	BF†R6ÖRf—'GVÂÖ÷&–v–â–FV2æG&ö–Bw2vV%f–Wt76WDÆöFW&æBFW6·F÷w0¦vÖS¢òö&÷Fö6öÂ†æFÆW"Â&V–×ÆVÖVçFVBæF—fVÇ’–â7v–gB6–æ6RtµU$Å66†VÖT†æFÆW&†2æğ¦'V–ÇBÖ–â76WBÖÆöFW"WV—fÆVçBF†Rv’æG&ö–G‚çvV&¶—FFöW2â–æ6ÇVFW2â5fÆÆ&6°¢‡VæÖF6†VBF‚(i"–æFW‚æ‡FÖÆÂÖ—'&÷&–ærvÖU&÷Fö6öÄ†æFÆW#£¦ÆöFw2÷vâFö26öÖÖVçB’æ@¦&ævV†VFW"7W÷'B‡6òÇf–FVóæöÆVF–óæ6VV¶–ærv÷&·2(	BtµU$Å66†VÖUF6¶æWfW §7–çF†W6—¦W2&ævR†æFÆ–æröâ—G2÷vâ’âFVÆ–&W&FVÇ’Æ–â7W7FöÒ66†VÖRÂæ÷B‡GG6 ¦tµU$Å66†VÖT†æFÆW&&Vv—7FW'2W"Ò§66†VÖR¢Âæ÷BW"Ö†÷7BÂ6ò6Æ–Ö–ær‡GG6—G6VÆbv÷VÆ@¦–çFW&6WBWfW'’&VÂæWGv÷&²&WVW7B†föçG2Â4Dç2ÂæÇ—F–72’FöòâvÖS¢òö—6âwBvV$¶—@§6V7W&R6öçFW‡B†æò6W'f–6Rv÷&¶W'2Â6öÖRæWvW"—2vFVBöâF†B’(	Bâ66WFVBG&FVöfbÂF†P§6ÖRöæRFW6·F÷w2÷vâvÖS¢òöÇ&VG’Ö¶W2Âæ÷Bâ÷fW'6–v‡BâÇ6òFFVB&÷fW57Æ6…f–Wv ¢†T•f–WvÂG&—fVâ'’t´æf–vF–öäFVÆVvFRæF–Df–æ—6†²F†R6ÖRS×2fÆö÷"2æG&ö–B’æ@¦W‡FVæFVB'VæFÆRç–Fò7FvRF†R'&æF–ær76WG2(	B”õ2†BæòWV—fÆVçBFòæG&ö–Bw27F'GW §7Æ6‚BÆÂVçF–ÂF†—2f—‚ÂÆVf–ærÆ–âv†—FR67&VVâGW&–ærÆöBÂW†7FÇ’F†Rv ¦DôDòæÖF÷F†—26W76–öâw2÷vâFW6·F÷v÷&²Ç&VG’fÆvvVB2v÷'F‚f—†–ærWfW'—v†W&Rà ¢¢¥F6‚6öç6öÆ–FF–öã¢¢¢F†R'&æ6‚F†—2ÆæFVBöâ÷&–v–æÆÇ’–çG&öGV6VBRÖÖö&–ÆRÖæF—fRĞ§vV'f–Ww2çF6†(	B6öÆÆ–F–ærv—F‚F†R¦Ç&VG’ÖW†—7F–ær¢R×v–æF÷w2×6¶v–ærçF6††&÷F€¦çVÖ&W&VB#R#²†&ÖÆW722F—7F–æ7Bf–ÆVæÖW2'WB6öægW6–ærÂæBv–ç7BF†—2&ö¦V7Bw2÷và§6WVVçF–ÂÖçVÖ&W&–ær6öçfVçF–öâ’(	BæB—G2F–fg2f÷"÷7Eö'V–ÆEö6öÖÖæG2ç–öæG&ö–DÖæ–fW7Bà§†ÖÆö'V–ÆBæw&FÆRæ·G6öÖ–ä7F—f—G’æ·F÷fW&ÆVBv—F‚BÖæG&ö–BçF6†Âv†–6‚Ç&VG¦÷væVBF†÷6R6ÖRf–ÆW2‡F†—2&ö¦V7Bw2÷vâW7F&Æ—6†VB–çf&–çBÂ6†V6¶VBF‡&÷Vv†÷WBF†—0§6W76–öâÂ—2öæRf–ÆRW"F6‚(	B6VRF†R##bÓ’Ó"vÖS¢òö×÷'BVçG&–W2&÷fRf÷"v‡“¢—Bw0§v†BÆWG2gWGW&R6W'fò×fW'6–öâWw&FR&V6öæ6–ÆRV6‚f–ÆRw27W7FöÖ—¦F–öç2–âW†7FÇ’öæP§Æ6R’â6öæf—&ÖVB2§&VÂ¢&ö&ÆVÒÂæ÷B§W7B7G–ÆRæ—C¢Ç––ærFF†VâF†R÷&–v–æÀ¦V–âF†R&VÂ4’6WVVæ6R7F–ÆÂf–ÆVB÷WG&–v‡Bf÷"÷7Eö'V–ÆEö6öÖÖæG2ç–‡F†Bf–ÆRw0¦F–fbv2vVæW&FVB77VÖ–ærrÖ'V–ÆB×FööÆ–ærçF6†w26†ævW2Ç&VG’Æ–VBÂWfVâF†÷Vv€¦v6÷'G2¦gFW"¢V’(	B&ööbF†—2F6‚†BæWfW"7GVÆÇ’&VVâfW&–f–VBv–ç7B§&VÂg&öÒ×&—7F–æR&V6öç7G'V7F–öââ6W&FVÇ’ÂF‡&VRöb—G2æWrÖf–ÆR6V7F–öç2†Ôô$”ÄRæÖFÀ¦&÷fW57Æ6…f–Wræ·FÂfÇVW2×c3÷7G–ÆW2ç†ÖÆ’vW&RÖ—76–ærF†RæWrf–ÆRÖöFVö–æFW††VFW ¦Æ–æW2v—BÇ–&WV—&W2‡F†÷Vv‚†&ÖÆW72f÷"F†—2&Wòw2÷vâF6‚×Ö&6VB4’(	B6VP¦FW7Bç–ÖÆ’âf—†VB'’&VvVæW&F–ær6–ævÆRÂ6VÆbÖ6öçF–æVBF–fbf÷"WfW'’f–ÆRF—&V7FÇ’g&öĞ¦g&W6‚&—7F–æRcãRã†æ÷Bg&öÒç’–çFW&ÖVF–FRF6†VB7FFR’Â&VÖ÷f–ærF†Rf÷W"÷fW&Æ–æp§6V7F–öç2g&öÒFövÂæB6öç6öÆ–FF–ærWfW'—F†–ærÖö&–ÆR×&VÆFVB–çFòöæR&VçVÖ&W&V@¦bÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†ƒRÓRvW&RÆÂÇ&VG’F¶Vâ’âÇ6ò‡—6–6ÆÇ’FVÆWFV@¦6W'f÷f–Wrö‡6VR&÷fR’&F†W"F†âÆVf–ærFw2æ÷r×ö–çFÆW72F–fbf÷"ÖöGVÆRæ÷F†–æp¦–æ6ÇVFW2ç–Ö÷&RâWfW'’f–ÆR–âF†RæWrF6‚&R×fW&–f–VB'—FRÖ–FVçF–6Âv–ç7BF†R7GVÀ§v÷&¶–ærG&VRg&öÒg&W6‚&—7F–æR&V6öç7G'V7F–öã²w&WÖ‚%æF–fbÒÖv—Bò"F6†W2÷6W'fò×cãRãò¢çF6€§Â6÷'BÂVæ—Ö2Âv²rCãv‡F†—26W76–öâw2÷vâ7FæF&B6†V6²’æ÷r&W÷'G2¦W&òf–ÆW0§F÷V6†VB'’Ö÷&RF†âöæRF6‚Â&Wò×v–FRà ¢¢¤4“¢¢¢æv—F‡V"÷v÷&¶fÆ÷w2öæG&ö–Bç–ÖÆ&Ww&—GFVâFòÖF6‚(	BæòÖ÷&R4D²ôäD²õ'W7BÖ7&÷72Ö6ö×–ÆP¦&ö÷G7G&Â§W7B¦f²æG&ö–B4D²ÆFf÷&Ò×FööÇ2²&VÂâöÖ6‚'VæFÆRÒÖæG&ö–BÒÖ6öçFVçBÖF— ¢âââÒÖ÷WGWBââæ'Vâv–ç7B6Öö¶R×FW7B6öçFVçBÂfW&–g––ærF†R7GVÂ6öFRF‚WfW'’&VÀ¦6öç7VÖW"‡&÷fW2Ö7F–öâÂ6¶Ö7FW"’W6W2–ç7FVBöb†æB×&öÆÆVBâöw&FÆWv–çfö6F–öââGvğ§&VÂ'Vw2f—†VB–âF†R6ÖR73¢ƒ’6F¶ÖævW"wÆFf÷&×3¶æG&ö–BÓ3rv(	BvöövÆRFöW6âw@§V&Æ—6‚F†BW†7B6¶vRæÖRÂöæÇ’fW'6–öæVBæG&ö–BÓ3rãöãöã&(	BF†R¦–FVçF–6Â ¦f–ÇW&RÇ&VG’F–væ÷6VBæBf—†VBf÷"6¶Ö7FW"V&Æ–W"F†—26W76–öâ‡6VR&÷fW2×6¶Ö7FW&w0¦7&2×FW&’÷7&2öæG&ö–Bç'6Â&W6öÇfU÷ÆFf÷&Õ÷6¶vV“²&W6öÇfVBG–æÖ–6ÆÇ’†W&RF†R6ÖRv’à¢ƒ"’F†R÷&–v–æÂ4’7FW6÷–VB6Öö¶R×FW7B6öçFVçBF—&V7FÇ’–çFòF†R§G&6¶VB¢7W÷'BöæG&ö–Bğ¦²÷6W'fö÷7&2öÖ–âö76WG2÷wwrö(	BF†RW†7B'W'6—7G2öæRvÖRw26öçFVçB7&÷72Vç&VÆFV@§'Vç2"çF’×GFW&âö'VæFÆUöæG&ö–Fw2÷vâFö26öÖÖVçBÇ&VG’v&ç2&÷WB‡6VRF†—2f–ÆRw0£##bÓ’ÓæG&ö–BÖ'VæFÆ–ærVçG&–W2’(	B†&ÖÆW72öâg&W6‚4’6†V6¶÷WBÂ'WBÖVçBF†—2v÷&¶fÆ÷p¦æWfW"7GVÆÇ’W†W&6—6VBö'VæFÆUöæG&ö–FBÆÂâf—†VB'’W6–ær&VÂÒÖ6öçFVçBÖF—&öÒÖ÷WGWF ¦–â÷F×–ç7FVBà ¢¢¤¶æ÷vâv2ÂÆVgB÷VâöâW'÷6S¢¢¢”õ27Fv–ær—6âwBv—&VB–çFòÖ6‚'VæFÆV†'VæFÆRç–—0¦7FæFÆöæR67&—C²'V–ÆF–ærF†R†6öFR&ö¦V7B7F–ÆÂæVVG2‡VÖâöâÖ4õ2'Vææ–ær†6öFVvVæ ¦'’†æB’(	BF†RæG&ö–B6–FR—2gVÆÇ’WFöÖFVBÂ”õ2—6âwB–WBâæòFWf–6R'VçF–ÖRfW&–f–6F–öà¦f÷"V—F†W"ÆFf÷&Ò‡7Æ6‚F–Ö–ærÂvV%f–Wr7F÷&vRW'6—7FVæ6RÂf–FVò6VV¶–ærÂ&÷FF–öâÀ¦gVÆÇ67&VVâ’(	BF†—2Ö6†–æR†2æòæG&ö–BV×VÆF÷"öFWf–6RGF6†VBæBæòÖ4õ2õ†6öFRBÆÂÀ§6ò4’Öw&VVâÇW26öFR&Wf–Wr—2F†R6V–Æ–æröbfW&–f–6F–öâ&V6†&ÆRg&öÒ†W&S²&VÂ6÷fW&vP¦æVVG26öÖVöæRv—F‚FWf–6Râ¢¦&÷fW2Ö7F–öææB&÷fW2×6¶Ö7FW&õ6¶Ö7FW"w2÷vâæG&ö–B'VæFÆ–æp§7F–ÆÂ77VÖVBF†RöÆB'W7BôäD²F‚2öbF†—2VçG'’¢¢(	B6VRF†V—"÷vâ5U5DôÔ•¤D”ôå2æÖFğ¦4ÄTDRæÖFVçG&–W2f÷"v†WF†W"F†BÖ–w&F–öâ†2ÆæFVB'’F†RF–ÖR–÷Rw&R&VF–ærF†—2à ¢ÒÒĞ ¢22##bÓ’ÓB(	Bf—'7B&VÂÖFWf–6R72öâF†RvV%f–WrÖö&–ÆR6öçF–æW'3¢–ÖÖW'6—fRÖöFRÂæG&ö–BCB&öG’Â5ÖfÆÆ&6²&—G ¢¢¤f–ÆW3¢¢¢7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âö¦fö÷&r÷6W'fò÷6W'f÷6†VÆÂôÖ–ä7F—f—G’æ·FÀ¦7W÷'Bö–÷2ôç7v–gFÂ7W÷'Bö–÷2ö'VæFÆRç–à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVB(	B6VRF†P£##bÓ’Ó2VçG'’&÷fRf÷"v‡’F†—2—2öæR6öç6öÆ–FFVBF6‚&F†W"F†âæWrçVÖ&W&VBöæR’à ¢¢¥v‡“¢¢¢F†R&Wf–÷W2VçG'’w2$¶æ÷vâv2"fÆvvVBF†BæV—F†W"6öçF–æW"†BWfW"&VVâ'Vâöà¦&VÂFWf–6Râ—B§W7Bv2ÂöâæG&ö–B†f—'7B&VÂ&W÷'B’ÂæBGvò&VÂ'Vw27W&f6VB(	@¦W†7FÇ’F†R6Æ72öbF†–ærF†B6V7F–öâçF–6—FVBà ¢¢¤æG&ö–B&"v2æWfW"7GVÆÇ’†–FFVâ'’FVfVÇBâ¢¢VçFW$–ÖÖW'6—fTÖöFV†5•5DTÕõT•ôdÄuğ¤eTÄÅ45$TTæö„”DUôäd”tD”ôæö”ÔÔU%4•dUõ5D”4µ–ÂÇW2F†RÄ”õUEò¦f&–çG26ò6öçFVç@¦FöW6âwB§V×v†VâF†R&'2G&ç6–VçFÇ’&VV"’&Wf–÷W6Ç’öæÇ’&â–ç6–FRöå6†÷t7W7FöÕf–Wv ®(	B’æRâöæÇ’f÷"…DÔÃRÇf–FVóæôgVÆÇ67&VVâÔ’6öçFVçBÂæWfW"f÷"F†RvÖRw2÷vâæ÷&ÖÂT’âæ÷p¦6ÆÆVBg&öÒöä7&VFV6òF†R7FGW2öæf–vF–öâ&'2&R†–FFVâg&öÒf—'7Bg&ÖRÂæBg&öĞ¦öåv–æF÷tfö7W46†ævVB††4fö7W2ÒG'VR–6–æ6RæG&ö–B6–ÆVçFÇ’6ÆV'2F†W6RfÆw2v†VæWfW"F†P§v–æF÷rÆ÷6W2æB&Vv–ç2fö7W2†æ÷F–f–6F–öâ6†FRÂ7—7FVÒF–ÆörÂ7v—F6†–ær2æB&6²®(	Bv—F†÷WBF†B&VÇ’ÂöæR7v—Rg&öÒF†RW6W"W&ÖæVçFÇ’VâÖ†–FW2F†R&'2f÷"F†R&W7Bö`§F†R6W76–öââÆVfTgVÆÇ67&VVæ‡&Wf–÷W6Ç’&W7F÷&–ær5•5DTÕõT•ôdÄuõd•4”$ÄVv†Vâ…DÔÃP¦gVÆÇ67&VVâVæG2’æ÷r6ÆÇ2VçFW$–ÖÖW'6—fTÖöFV–ç7FVBÂ6òW†—F–ærf–FVògVÆÇ67&VVâ&WGW&ç2Fğ§F†Rw2÷vâÇv—2Ö–ÖÖW'6—fR&6VÆ–æR–ç7FVBöb&WfVÆ–ærF†R&'2à ¢¢¤æG&ö–Bw2CB&W7öç6R†BçVÆÆ&öG’â¢¢6†÷VÆD–çFW&6WE&WVW7Fw2ÖçVÂCBf÷"¦Ö—76–ærvV%f–Wt76WDÆöFW&F‚76VBçVÆÆ2F†RvV%&W6÷W&6U&W7öç6VFF7G&VÒ(	@¦&V6öå‡&6Vv26WBFòF†RÆ—FW&Â7G&–ær$æ÷Bf÷VæB&Âv†–6‚—2F†R…EE7FGW2Æ–æRÂæ÷@§vR6öçFVçC²F†R7GVÂ&W7öç6R&öG’v2V×G’â”õ2w2WV—fÆVçB†vÖU66†VÖT†æFÆW"ç&W7öæF ¦–âç7v–gF’Ç&VG’F–BF†—26÷'&V7FÇ’†FF‚$æ÷Bf÷VæB"çWFc‚–2&VÂ&öG’'—FW2’âf—†V@§FòÖF6ƒ¢$æ÷Bf÷VæB"æ'—FT–çWE7G&VÒ„6†'6WG2åUDeó‚–2F†RFF7G&VÒà ¢¢¤æG&ö–B†Bæò5fÆÆ&6³²”õ2F–Bâ¢¢vÖU66†VÖT†æFÆW&†26W'fVBÖ—76–ærF‚0¦–æFW‚æ‡FÖÆ6–æ6R—Bv2w&—GFVâ‡6VRF†R##bÓ’Ó2VçG'’w2$6÷'&V7F–öâÂ6ÖRF’ §&w&‚’(	B6Æ–VçB×6–FR&÷WFW"æf–vF–ærFòRærâöÆWfVÂó6†2æòf–ÆRBF†BF‚'W@§6†÷VÆB7F–ÆÂvWBF†RVçG'’Fö7VÖVçBæBÆWBF†R&÷WFW"FV6–FRv†BFò&VæFW"âæG&ö–Bw0¦vV%f–Wt76WDÆöFW&F‚†æFÆW"†BæòWV—fÆVçC¢Ö—76–ærF‚§W7BCBvBâFFVBF†R6ÖP¦fÆÆ&6²†76WG2æ†æFÆR‚'wwròGF‚"’ó¢76WG2æ†æFÆR‚'wwrö–æFW‚æ‡FÖÂ"–’f÷"&—G’(	BF†P§6ÖRG&FVöfb&÷F‚ÆFf÷&×2æBFW6·F÷w2÷vâvÖS¢òö†÷'G2÷6W'f÷6†VÆÂ÷&÷Fö6öÇ2övÖRç'6¦Ç&VG’66WC¢vVçV–æVÇ’ÖÖ—76–ær7V"×&W6÷W&6R†â–ÖvRÂ67&—B’æ÷r6W'fW2–æFW‚æ‡FÖÆ §v—F‚#–ç7FVBöb&÷W"CBÂ–âW†6†ævRf÷"6Æ–VçB×6–FR&÷WF–ærv÷&¶–ærBÆÂâF†P¦ÖçVÂCB'&æ6‚—2öæÇ’&V6†&ÆRæ÷rv†Vâ–æFW‚æ‡FÖÆ—G6VÆb—2Ö—76–ærà ¢¢¦”õ2æWfW"†–BF†R7FGW2&"÷"†öÖR–æF–6F÷"â¢¢VæÆ–¶RæG&ö–B‡v†–6‚öæÇ’†BF†P¢¦FVfVÇBÖöâ¢'Vr&÷fR’Â”õ2†Bæò–ÖÖW'6—fRöVFvR×FòÖVFvR†æFÆ–ærBÆÂ(	BvÖUf–Wt6öçG&öÆÆW& ¦æ÷r÷fW'&–FW2&VfW'57FGW4&$†–FFVæö&VfW'4†öÖT–æF–6F÷$WFô†–FFVæ†&÷F‚G'VVÂ&VBöæ6RÀ¦æWfW"FövvÆVBÂ6òæò6WDæVVG57FGW4&$V&æ6UWFFVö6WDæVVG5WFFTöd†öÖT–æF–6F÷$WFô†–FFVæ ¦6ÆÇ2&RæVVFVB’â'VæFÆRç–w2vVæW&FVB–æfòçÆ—7FÇ6ò6WG0¦T•f–Wt6öçG&öÆÆW$&6VE7FGW4&$V&æ6VöT•7FGW4&$†–FFVæW‡Æ–6—FÇ’‡&VGVæFçBv—F‚F†P¦÷fW'&–FRVæFW"FöF’w2FVfVÇG2Â'WBwV&G2v–ç7BgWGW&R6–væ–æröW‡÷'B7FWw2–æfòçÆ—7@¦ÖW&vRWfW"–çG&öGV6–ær6öæfÆ–7F–ærFVfVÇB’à ¢¢¤æ÷B–WBFWf–6R×fW&–f–VC¢¢¢”õ2(	BæòFWf–6R÷6–×VÆF÷"öâF†—2Ö6†–æRFò6öæf—&ÒF†R7FGW0¦&"ö†öÖR–æF–6F÷"f—‚f—7VÆÇ’ÂöæÇ’æG&ö–Bv27GVÆÇ’ö'6W'fVBâ&R×fW&–g’öâ”õ2&Vf÷&P§&VÇ––æröâF†—2VçG'’f÷"F†BÆFf÷&Òà ¢ÒÒĞ ¢22##bÓ’ÓB(	B4’&—G“¢æv—F‡V"÷v÷&¶fÆ÷w2ö–÷2ç–ÖÆÂÖ—'&÷&–æræG&ö–Bç–ÖÆ  ¢¢¤f–ÆW3¢¢¢æv—F‡V"÷v÷&¶fÆ÷w2ö–÷2ç–ÖÆ†æWr’âæòF6‚(	BF†—2&Wòw2÷vâ4’f–ÆRÂæ÷@§fVæF÷&VB‡6ÖR2æG&ö–Bç–ÖÆöFW7Bç–ÖÆö&VÆV6Rç–ÖÆ’à ¢¢¥v‡“¢¢¢F†R##bÓ’Ó2VçG'’w2$¶æ÷vâv2"W‡Æ–6—FÇ’6ÆÆVB÷WBF†B'F†RæG&ö–B6–FR—0¦gVÆÇ’WFöÖFVBÂ”õ2—6âwB–WB"(	B4’öæÇ’WfW"6Öö¶R×FW7FVBÖ6‚'VæFÆRÒÖæG&ö–FÂæWfW ¦7W÷'Bö–÷2ö'VæFÆRç–â&WVW7FVBF—&V7FÇ“¢F†R4’ô4BF†B&öGV6W2âæG&ö–B'V–ÆB6†÷VÆ@¦FòF†R6ÖRf÷"”õ2à ¢¢¥v†B—BFöW3¢¢¢Ö—'&÷'2æG&ö–Bç–ÖÆw2÷vâ7G'V7GW&RæB&V6öæ–ær‡6ÖR6Öö¶R×FW7B6öçFVçBÀ§6ÖRVç7W&R×FW7B×&VÆV6V¦ö"÷&öÆÆ–ær'FW7B"&VÆV6R’'WBG&—fW2F†R&VÂ”õ2F€¦7W÷'BôÔô$”ÄRæÖFFö7VÖVçG2–ç7FVBöbÖ6‚'VæFÆV(	B7W÷'Bö–÷2ö'VæFÆRç–7FvW2F†P¦6öçFVçBÂ†6öFVvVâvVæW&FVGW&ç2F†R&W7VÇF–ær&ö¦V7Bæ§6öæ–çFò&VÂç†6öFW&ö¦ÂF†Và¦†6öFV'V–ÆB×6F²—†öæW6–×VÆF÷"âââ4ôDUõ4”tä”äuôÄÄõtTCÔäö'V–ÆG2—BVç6–væVBf÷"F†P¥6–×VÆF÷"†æòÆRFWfVÆ÷W"öF—7G&–'WF–öâ6–væ–ær–FVçF—G’W†—7G2–â4’(	B6ÖR&V6öà¦&VÆV6Rç–ÖÆw2FW6·F÷'V–ÆG2FöâwBGFV×Bç’Öö&–ÆR7F÷&R6–væ–ærV—F†W"’âF†—2&÷fW2F†P§7Fv–ær²†6öFTvVâ²†6öFR'V–ÆB—VÆ–æR—G6VÆbv÷&·2VæBFòVæC²—B—2¢¦æ÷B¢¢¦FWf–6RÖ–ç7FÆÆ&ÆR÷"F—7G&–'WF&ÆR'V–ÆB(	B‡VÖâöâÖ4õ27F–ÆÂæVVG2Fò÷VâF†RvVæW&FV@§&ö¦V7B–â†6öFRÂ6†ö÷6RFWfVÆ÷ÖVçBFVÒÂæB&6†—fRöW‡÷'Bf÷"&VÂFWf–6R÷"F†R ¥7F÷&RÂW†7FÇ’27W÷'BôÔô$”ÄRæÖFw2÷vâ”õ26V7F–öâÇ&VG’Fö7VÖVçFVB&Vf÷&RF†—26†ævRà¤Ç6òV&Æ—6†W2&÷fW5ö–÷5÷&ö¦V7Bç¦—†6æ6†÷Böb7W÷'Bö–÷2ö’FòF†R&öÆÆ–ær'FW7B §&VÆV6RÂÖ—'&÷&–æræG&ö–Bç–ÖÆw2÷vâ&÷fW5öæG&ö–E÷&ö¦V7Bç¦—(	B6ò6¶Ö7FW"÷&÷fW2Ö7F–öà¦6âfWF6‚F†R”õ27Fv–ærFV×ÆFRF—&V7FÇ’ÂF†R6ÖRv’6¶Ö7FW"w2æG&ö–B&6¶VæBÇ&VG¦FöW2f÷"F†RæG&ö–Bw&FÆR&ö¦V7BÂv—F†÷WBv—B6†V6¶÷WBöbF†—2Væv–æRà ¢¢¥VæÆ–¶RæG&ö–Bç–ÖÆÂæòÖ6†öF6†W2ööFW7G2÷wBö–çföÇfVÖVçBBÆÂ¢£ ¦7W÷'Bö–÷2ö'VæFÆRç–—27FæFÆöæR67&—Bv—F‚æòFWVæFVæ7’öâÖ6†w26öÖÖæBÆöFW ¢‡6VR7W÷'BôÔô$”ÄRæÖFw2÷vâ&”õ2"6V7F–öâ(	B—Bw2–çfö¶VBF—&V7FÇ’v—F‚—F†öã6’Â6òæöæP¦öbæG&ö–Bç–ÖÆw2uB×FööÆ–ær×7'6RÖ6†V6¶÷WBv÷&¶&÷VæBÆ–W2†W&Rà ¢¢¤ÆVgB÷VâöâW'÷6S¢¢¢7F–ÆÂæò&VÂFWf–6Rô7F÷&RF—7G&–'WF–öâF‚–â4’‡v÷VÆBæVV@¦âÆRFWfVÆ÷W"66÷VçBw26–væ–ær6W'F–f–6FR²&÷f—6–öæ–ær&öf–ÆR2&Wò6V7&WG2Â÷W@¦öb66÷Rf÷"v†Bw27F–ÆÂâ–æ—F–Â6öçF–æW"W"F†R##bÓ’Ó2VçG'’’â$TDÔRæÖFw0¢%7W÷'FVBÆFf÷&×2"6V7F–öâæB7W÷'BôÔô$”ÄRæÖFWFFVBFòÖVçF–öâF†R4’6÷fW&vRæ@§F†RÇv—2Ö†–FFVâ7FGW2&"ö†öÖR–æF–6F÷"g&öÒF†RVçG'’&÷fRà ¢¢¤7&÷72×&Wò7–æ26†V6²‡W"F†—2&Wòw2÷vâ4ÄTDRæÖF“¢¢¢&÷fW2Ö7F–öæw27F–öâç–ÖÆ ¦Ç&VG’†2âæG&ö–C¢wG'VRv–çWBÖ—'&÷&–ærÖ6‚'VæFÆRÒÖæG&ö–F†FFVBf÷"F†P£##bÓ’Ó2—f÷B’'WB†2æò–÷6WV—fÆVçB(	BF†—2v÷&¶fÆ÷röæÇ’6Öö¶R×FW7G2F†RVæv–æP§&Wòw2÷vâ”õ27Fv–ærF‚Â—BFöW6âwBFB”õ27W÷'BFò&÷fW2Ö7F–öæ—G6VÆbâfÆvv–æp§F†—2W‡Æ–6—FÇ’&F†W"F†â6–ÆVçFÇ’ÆVf–ær—C¢v†WF†W"&÷fW2Ö7F–öæ6†÷VÆBv–âÖF6†–æp¦–÷3¢wG'VRv–çWB†æBÂ–b6òÂv†WF†W"—B6†÷VÆB6†VÆÂ÷WBFò7W÷'Bö–÷2ö'VæFÆRç–°¥†6öFTvVâF†Rv’F†—2v÷&¶fÆ÷rFöW2Â6–æ6RF†W&Rw27F–ÆÂæòÖ6‚'VæFÆRÒÖ–÷6’—2&VÀ¦föÆÆ÷r×WÂæ÷BFöæR2'BöbF†—26†ævRà ¢ÒÒĞ ¢22##bÓ’ÓB(	BæG&ö–C¢F†R7GVÂ$æ÷Bf÷VæB"&ö÷B6W6R†ÆöF–ærö–æFW‚æ‡FÖÆÂæ÷Bö ¢¢¤f–ÆW3¢¢¢7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âö¦fö÷&r÷6W'fò÷6W'f÷6†VÆÂôÖ–ä7F—f—G’æ·FÀ¦7W÷'BôÔô$”ÄRæÖFà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVBv–â’à ¢¢¥v‡“¢¢¢F†R##bÓ’ÓBVçG'’&÷fR‚$f—'7B&VÂÖFWf–6R72âââ"’f—†VBGvò&VÂ'Vw2'W@¢¦F–FâwB7GVÆÇ’f—‚F†R&W÷'FVB$æ÷Bf÷VæB"67&VVâ¢(	B6öæf—&ÖVB'’FW7F–ærF†R&V'V–ÇB°¦öâF†R6ÖR&VÂFWf–6Rv–ââF–væ÷6VB&÷W&Ç’F†—2F–ÖRf–F†RFWf–6Rw2÷vâ6‡&öÖP¤FWeFööÇ2&VÖ÷FR–ç7V7F–öâ†6‡&öÖS¢òö–ç7V7FÂ&V6†&ÆR6–æ6RFV'Vr·2Ç&VG’Væ&ÆP¦6WEvV$6öçFVçG4FV'Vvv–ætVæ&ÆVF“¢F†R&VÂvÖRw2¥26öç6öÆR6†÷vVBæòæF—fRôæG&ö–B×6–FP¦W'&÷"BÆÂâF†R7GVÂ6W6Rv2VçF—&VÇ’F–ffW&VçBg&öÒç—F†–ær–âF†BV&Æ–W"VçG'’à ¢¢¥F†R&VÂ'Vs¢¢¢öä7&VFV6ÆÆVBvV%f–WræÆöEW&Â‚&‡GG3¢òö76WG2ææG&ö–GÆFf÷&ÒææWBğ¦–æFW‚æ‡FÖÂ"–(	BF†RÆ—FW&Âö–æFW‚æ‡FÖÆF‚Âæ÷BF†R&&R÷&–v–â&ö÷BöâF†—2Ö¶W0¦Æö6F–öâçF†æÖVWVÂö–æFW‚æ‡FÖÆöâ&ö÷BÂv†–6‚6Æ–VçB×6–FR&÷WFW"‡&V7B×&÷WFW"À¦ÖF6†–ærv–ç7Bö2—G2†öÖR&÷WFR(	B6öæf—&ÖVBv–ç7BF†R&VÂ6Öö¶R×FW7B6öçFVçBÀ¦—†’×fâ×&V7B×FV×ÆFV’FöW6âwB&V6övæ—¦RÂ&VæFW&–ær§F†RvÖRw2÷vâ¢$æ÷Bf÷VæB"CB&÷WFRà¥F†—2v2æWfW"æF—fR76WBÖÆöF–ær'VrBÆÂ(	BF÷væÆöF–æræBVç¦—–ærF†R7GVÂ'V–Ç@¤²†&÷fW2Ö7F–öæw2&÷fW5ö7F–öåöæG&ö–EöFV'Vræ¶’6öæf—&ÖVB76WG2÷wwrö–æFW‚æ‡FÖÆ §v26÷'&V7FÇ’&W6VçBv—F‚&VÂ6öçFVçBF†Rv†öÆRF–ÖS²76WG5F„†æFÆW&v2f–æF–ær—@¦f–æRâ”õ2w2ç7v–gFv2æWfW"ffV7FVB'’F†—2(	B—BÇ&VG’ÆöG2F†R&&P¦vÖS¢òö6öçFVçBö†æòö–æFW‚æ‡FÖÆ’Âv†–6‚—2W†7FÇ’v‡’F†—26W76–öâw2V&Æ–W"77V×F–öà¢‚&”õ2&ö&&Ç’†2F†R6ÖR'Vr"’GW&æVB÷WBæ÷BFò†öÆBöæ6R—G26öFRv27GVÆÇ’&RÖ6†V6¶V@¦Æ–æR'’Æ–æRv–ç7BF†—27V6–f–2f–ÇW&RÖöFRâf—†VB'’ÆöF–æp¦‡GG3¢òö76WG2ææG&ö–GÆFf÷&ÒææWBö–ç7FVBÂÖ—'&÷&–ærç7v–gFà ¢¢¤6V6öæBÂ&VÆFVB'Vrf÷VæBv†–ÆRf—†–ærF†Rf—'7C¢¢¢F†R&Wf–÷W2VçG'’w25ÖfÆÆ&6°¦FF—F–öâ†76WG2æ†æFÆR‚âââ’ó¢76WG2æ†æFÆR‚'wwrö–æFW‚æ‡FÖÂ"–’v2FVB6öFRà¦vV%f–Wt76WDÆöFW"ä76WG5F„†æFÆW"æ†æFÆR‚–FöW2¢¦æ÷B¢¢&WGW&âçVÆÆöâÖ—76–ær76W@®(	B6öæf—&ÖVB'’&VF–ærF†R&VÂæG&ö–G‚çvV&¶—F6÷W&6R†vV%f–Wt76WDÆöFW"æ¦fÀ¦æG&ö–G‚ÖÖ–æ'&æ6‚“¢öââ”ôW†6WF–öæ—B&WGW&ç2¦æöâÖçVÆÂ¢vV%&W6÷W&6U&W7öç6V §v—F‚Ö–ÖUG—VöVæ6öF–ævöFFÆÂçVÆÆÂæ÷BçVÆÆ—G6VÆbâó¦VÇf—2fÆÆ&6²¶W–VBöà§F†B6ÆÂF†W&Vf÷&RæWfW"G&–vvW'2(	BF†RÆVgB6–FR—2æWfW"çVÆÂâ6ÖR—77VRffV7FV@¦6†÷VÆD–çFW&6WE&WVW7Fw2÷vâ&W7öç6RÓÒçVÆÆ6†V6²f÷"6öç7G'V7F–ærF†R&VÂCC¢6–æ6P¦vV%f–Wt76WDÆöFW"ç6†÷VÆD–çFW&6WE&WVW7B…W&’–—G6VÆböæÇ’&WGW&ç2çVÆÆv†Vâ¦æò†æFÆW ¦ÖF6†VBBÆÂ¢†–×÷76–&ÆR†W&RÂ÷W"÷vâ†æFÆW"ÖF6†W2WfW'’F‚VæFW"ö’ÂF†B'&æ6€§v2WVÆÇ’Vç&V6†&ÆRâ&÷F‚æ÷r6†V6²&W7öç6SòæFFÓÒçVÆÆ–ç7FVB(	BF†R7GVÂ6–væÀ¦f÷"&WfVâF†RfÆÆ&6²f–ÆVBFò÷Vâ&VÂf–ÆR"(	BfW&–f–VBv–ç7BF†R&VÀ¦F„ÖF6†W&övV%f–Wt76WDÆöFW&6÷W&6RÂæ÷B77VÖVBà ¢¢¤F†—&B'VrÂf÷VæBF†R6ÖRv’v†–ÆR&R×&VF–ærF†—2f–ÆRVæBFòVæC¢¢¢F†RvRw2÷và§6W'f–6Rv÷&¶W"†&Vv—7FW%5ræ§6Âg&öÒf—FR×ÇVv–â×v–âF†R6Öö¶R×FW7B6öçFVçB’f–ÆVBFğ§&Vv—7FW"(	Bf—6–&ÆRF—&V7FÇ’–âF†RFWeFööÇ26öç6öÆR2f–ÆVBFò&Vv—7FW"6W'f–6Uv÷&¶W ¦f÷"66÷R‚âââ“¢âVæ¶æ÷vâW'&÷"ö67W'&VBv†VâfWF6†–ærF†R67&—FâæG&ö–BvV%f–Wr&÷WFW0¦6W'f–6Rv÷&¶W"w2÷vâæWGv÷&²&WVW7G2F‡&÷Vv‚¢§6W&FR¢¢–çFW&6WF–öâ†öö°¢†æG&ö–G‚çvV&¶—Bå6W'f–6Uv÷&¶W$6öçG&öÆÆW$6ö×Fö6W'f–6Uv÷&¶W$6Æ–VçD6ö×F’ÂF—7F–æ7Bg&öĞ¦vV%f–Wt6Æ–VçBç6†÷VÆD–çFW&6WE&WVW7F(	Bv—F†÷WB—BÂ7ræ§6w2÷vâfWF6‚fVÆÂF‡&÷Vv‚FòF†P§&VÂæWGv÷&²Âv†–6‚f–Ç2÷WG&–v‡B6–æ6R76WG2ææG&ö–GÆFf÷&ÒææWF—6âwB&VÂÀ§&W6öÇf&ÆRFöÖ–ââf—†VB'’&Vv—7FW&–ær6W'f–6Uv÷&¶W$6Æ–VçD6ö×F†fVGW&RÖ6†V6¶VBf–¦vV%f–WtfVGW&Ræ—4fVGW&U7W÷'FVFÂ6–æ6Ræ÷BWfW'’vV%f–Wr&÷f–FW"–×ÆVÖVçG2F†—2’F†@§6†&W2F†RW†7B6ÖR–çFW&6WF–öâÆöv–22F†RÖ–âvV%f–Wt6Æ–VçFÂf7F÷&VB–çFòöæRÆö6À¦–çFW&6WB‡W&Â–gVæ7F–öâW6VB'’&÷F‚à ¢¢¤ÆW76öâÂf÷"&VÂF†—2F–ÖS¢¢¢Gvò&÷VæG2öb&f—‚6öæf—&ÖVBf–4’w&VVâ²6öFR&Wf–Wr"&÷F€¦Ö—76VBF†R7GVÂ'VrÂ&V6W6R4’æWfW"'Vç2F†R²ÂæBF†R&VÂW"×7–×FöÒ6W6R†¦6Æ–VçB×6–FR&÷WFW"Ö—6ÖF6‚’FöW6âwBÆöö²Æ–¶RâæG&ö–BõvV%f–Wr&ö&ÆVÒg&öÒ6÷W&6R&VF–æp¦ÆöæRâ&VÂÖFWf–6RFWeFööÇ2–ç7V7F–öâ—2v†B7GVÆÇ’f÷VæB—BÂ–âfWrÖ–çWFW2ÂgFW §7FF–2æÇ—6—2ÆöæR†FâwBâ7W÷'BôÔô$”ÄRæÖFWFFVBFòFW67&–&RF†R6÷'&V7FVB&ö÷BU$Âà ¢ÒÒĞ ¢22##bÓ’ÓB(	Bf—†–ærF†Rf—ƒ¢&VÂ¶÷FÆ–â6ö×–ÆRW'&÷"ÂæBF6‚F†B†B6–ÆVçFÇ’G&–gFVBg&öÒ—G2÷vâ6÷W&6Rf–ÆP ¢¢¤f–ÆW3¢¢¢7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âö¦fö÷&r÷6W'fò÷6W'f÷6†VÆÂôÖ–ä7F—f—G’æ·Fà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVBF†—&BF–ÖR’à ¢¢¥v‡“¢¢¢F†R&Wf–÷W2VçG'’w2f—‚æWfW"7GVÆÇ’&V6†VBFWf–6R(	B&÷fW2Ö7F–öæw0¦'V–ÆBÖæG&ö–F4’¦ö"‡v†–6‚¦FöW2¢'Vâ&VÂw&FÆR76VÖ&ÆRÂVæÆ–¶RF†—2&Wòw2÷và¦æG&ö–Bç–ÖÆ6Öö¶RFW7BâââæòÂ&÷F‚Fò’f–ÆVB÷WG&–v‡Böæ6RF†R–ææVBFrv2'V×VBFğ¦–æ6ÇVFR—BâGvò–æFWVæFVçBÂVç&VÆFVB'Vw2Â&÷F‚6VÆbÖ–æfÆ–7FVB–âF†R6ÖR6öÖÖ—C  ¢¢£â&VÂ¶÷FÆ–â6ö×–ÆRW'&÷"â¢¢F†RF‚†æFÆW"w2fÆÆ&6²v2w&—GFVâ0¦–b‡&–Ö'’æFFÒçVÆÂ’&–Ö'’VÇ6Rââæ(	B'WB76WG5F„†æFÆW"æ†æFÆR‚–w2FV6Æ&V@§&WGW&âG—R—2çVÆÆ&ÆRvV%&W6÷W&6U&W7öç6V†6öæf—&ÖVBv–ç7BF†R&VÂæG&ö–G‚çvV&¶—@§6÷W&6RÂ6ÖR2F†R&Wf–÷W2VçG'’w2÷vâf–æF–ær’Â6ò&–Ö'–†2¶÷FÆ–âG—P¦vV%&W6÷W&6U&W7öç6Söâ66W76–æræFFöâ—Bv—F†÷WB6fR6ÆÂ†òæ’÷"&V6VF–æp¦çVÆÂÖ6†V6²—26ö×–ÆR×F–ÖRW'&÷"Âæ÷B'VçF–ÖRöæR(	BF†—2v26öæfÆF–ær&æWfW"7GVÆÇ¦çVÆÂB'VçF–ÖR"‡G'VR’v—F‚&æ÷BFV6Æ&VBçVÆÆ&ÆR"†fÇ6R’ÂF†R6ÖR6FVv÷'’öbÖ—7F¶P§F†R&Wf–÷W2VçG'’Ç&VG’ÖFRöæ6RæB6÷'&V7FVBVÇ6Wv†W&R–âF†R6ÖRf–ÆR‡F†P¦–çFW&6WFgVæ7F–öâw2&W7öç6SòæFFÓÒçVÆÆ6†V6²¦F–B¢W6R6fR6ÆÂ6÷'&V7FÇ’’(	B§W7@¦Ö—76VB–âF†—2öæR÷F†W"7÷Bâf—†VBFò–b‡&–Ö'’ÒçVÆÂbb&–Ö'’æFFÒçVÆÂ’&–Ö'¦VÇ6RââæÂv†–6‚ÆWG2¶÷FÆ–â6Ö'BÖ67B&–Ö'–FòæöâÖçVÆÂ–ç6–FRF†R'&æ6‚â6Vv‡B'’¦f–ÆVB'V–ÆBÖæG&ö–F4’'Vâ–â&÷fW2Ö7F–öæÂæ÷BÆö6ÆÇ’†æò¶÷FÆ–âôw&FÆRFööÆ6†–à¦f–Æ&ÆR–âF†—2Vçf—&öæÖVçBBÆÂ(	BWfW'’6†V6²öâF†—2f–ÆRF†—26W76–öâ†2&VVâ7FF–0§6÷W&6R&Wf–WrÂæ÷B6ö×–ÆR’(	BF†Rææ÷FF–öç2’öæÇ’7W&f6VBvVæW&–2%&ö6W70¦6ö×ÆWFVBv—F‚W†—B6öFR"f÷"F†Rf–Æ–ær6ö×÷6—FR7FWÂæò6ö×–ÆW"W'&÷"FW‡C²F†Rf—€§v2FW&—fVB'’&RÖFW&—f–ærF†RçVÆÆ&–Æ—G’6†–â'’†æBv–ç7BF†R&VÂæG&ö–G‚çvV&¶—@§6–væGW&W2Ç&VG’fWF6†VBf÷"F†R&Wf–÷W2VçG'’Âæ÷B'’&VF–ærâ7GVÂ6ö×–ÆW"ÖW76vRà ¢¢£"âF6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†w2÷vâ7W÷'BôÔô$”ÄRæÖF&Æö6°¦†B6–ÆVçFÇ’G&–gFVBg&öÒF†R&VÂf–ÆR¢¢(	B6VÆbÖ–æfÆ–7FVB&ö6W72'VrÂæ÷B6öFR'Vrà¥F†R&Wf–÷W2VçG'’w26W76–öâVF—FVB7W÷'BôÔô$”ÄRæÖF–âGvò6W&FR76W2†öæ6Rf÷"F†P¢$f—'7B&VÂÖFWf–6R72"f—†W2Âöæ6RÖ÷&Rf÷"F†—26ÖRVçG'’w2÷vâ&ö÷BÖ6W6Rw&—FWW’'W@¦öæÇ’&VvVæW&FVBF†Bf–ÆRw2§F6‚&Æö6²¢gFW"F†Rf—'7B72Âæ÷BF†R6V6öæB(	B6òF†P¦6öÖÖ—GFVBF6‚V–WFÇ’7F–ÆÂFW67&–&VBF†R¦öÆFW"¢v÷&F–ær†Rærâ7F–ÆÂÆ—7F–ær&Ö—76–æp¦–æFW‚æ‡FÖÂ"2VæF–ærfÆ–FF–öâ—FVÒ’v†–ÆRF†R&VÂ7W÷'BôÔô$”ÄRæÖF†BÇ&VG¦Ö÷fVBöââ6Vv‡BöæÇ’'’F†—26W76–öâw2÷vâ'—FRÖf÷"Ö'—FRfW&–f–6F–öâ7FW‡&V6öç7G'V7@§&—7F–æRÂf÷'v&BÖÇ’ÂF–fbv–ç7BF†R&VÂv÷&¶–ærG&VRf–ÆR'’f–ÆR’6F6†–ærÖ—6ÖF6€¦öâ7W÷'BôÔô$”ÄRæÖF7V6–f–6ÆÇ’(	BW†7FÇ’F†R¶–æBöbG&–gBF†BfW&–f–6F–öâ7FWW†—7G0§Fò6F6‚ÂæBF–Bâ&VvVæW&FVB6÷'&V7FÇ’F†—2F–ÖRg&öÒF†Rf–ÆRw27GVÂ7W'&VçB6öçFVçBà ¢¢¤ÆW76öâöâF÷öbF†R&Wf–÷W2VçG'’w2÷vâÆW76öã¢¢¢fW&–g––ærF6‚&Æ–W26ÆVæÇ’ ¦—6âwBF†R6ÖR2fW&–g––ær—BÖF6†W2F†R¦7W'&VçB¢f–ÆR(	BF6‚6âÇ’W&fV7FÇ’6ÆVà¦æB7F–ÆÂVæ6öFR7FÆR6öçFVçB–bF†RVæFW&Ç––ærf–ÆRv2VF—FVBv–âgFW"F†RF6‚v0¦Æ7B&VvVæW&FVBÂ–âF†R6ÖR6W76–öâÂv—F†÷WBç–öæR&R×'Vææ–ærF†R&VvVæW&F–öâ6V6öæ@§F–ÖRâF†RgVÆÂ'—FRÖf÷"Ö'—FRF–fbÖv–ç7B×v÷&¶–ær×G&VR6†V6²†æ÷B§W7B6ÆVâF6†W†—@¦6öFR’—2v†B6Vv‡BF†—2ÂæB6†÷VÆB&RG&VFVB2F†R7GVÂ&"f÷"&FöæRÂ"æ÷BF†RG'’×'Và¦ÆöæRà ¢ÒÒĞ ¢22##bÓ’ÓB(	B6fR–×÷'BöW‡÷'B†Æ–çWBG—SÒ&f–ÆR#æöÆF÷væÆöCæ’F–FâwBv÷&²–âV—F†W"Öö&–ÆR6öçF–æW  ¢¢¤f–ÆW3¢¢¢7W÷'BöæG&ö–Bö²÷6W'fö÷7&2öÖ–âö¦fö÷&r÷6W'fò÷6W'f÷6†VÆÂôÖ–ä7F—f—G’æ·FÀ¦7W÷'Bö–÷2ôç7v–gFà ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVB’à ¢¢¥v‡“¢¢¢&W÷'FVBF—&V7FÇ’g&öÒ&VÂFWf–6R(	BvÖRw26fRÖÖVçR–×÷'BöW‡÷'B'WGFöç0¢†Æ–çWBG—SÒ&f–ÆR#æFòÆöB6fRÂÆF÷væÆöCæöâ&Æö"öFFU$ÂFòW‡÷'BöæR’F–@¦æ÷F†–ærBÆÂöâæG&ö–BâæV—F†W"—2W&Ö—76–öç2&ö&ÆVÒ†æòÖæ–fW7BW&Ö—76–öâv2WfW ¦Ö—76–ær’(	B&÷F‚vV%f–WvæBtµvV%f–Wv6–×Ç’†fR¢¦æòFVfVÇB†æFÆ–ær¢¢f÷"V—F†W"ö`§F†W6RÂVæÆ–¶R&VÂ'&÷w6W"F#²æF—fR6öçF–æW"†2Fòv—&RWF†RWV—fÆVçB—G6VÆbà ¢¢¤æG&ö–C¢¢ ¢Ò¢¤–×÷'B¢¢æVVFVBvV$6‡&öÖT6Æ–VçBæöå6†÷tf–ÆT6†ö÷6W&(	Bv—F†÷WB—BÂÆ–çWBG—SÒ&f–ÆR#æw0¢æ6Æ–6²‚–6†÷w2æò–6¶W"BÆÂâ–×ÆVÖVçFVBf–F†R6Æ76–27F'D7F—f—G”f÷%&W7VÇFğ¢öä7F—f—G•&W7VÇF—"†Ö–ä7F—f—G–W‡FVæG2Æ–â7F—f—G–Âæ÷BæG&ö–E€¢6ö×öæVçD7F—f—G–Â6òF†RÖöFW&â7F—f—G’&W7VÇB’—6âwBf–Æ&ÆR†W&Rv—F†÷WBÆ&vW ¢&Vf7F÷"’ÇW2vV$6‡&öÖT6Æ–VçBäf–ÆT6†ö÷6W%&×2æ7&VFT–çFVçB‚–öç'6U&W7VÇB‚–à¢Ò¢¤W‡÷'B¢¢æVVFVBvV%f–Wrç6WDF÷væÆöDÆ—7FVæW&(	Bv—F†÷WB—BÂâÆF÷væÆöCæ6Æ–6²öâ¢&Æö#¦öFF¦U$Â—26–ÆVçFÇ’G&÷VBâ&Æö#¦U$Ç2&RöæÇ’fÆ–B–ç6–FRF†RvRw2÷vâ¥0¢6öçFW‡BÂ6òF†RÆ—7FVæW"–æ¦V7G26ÖÆÂWfÇVFT¦f67&—F6æ—WB†fWF6†F†R&Æö"À¢f–ÆU&VFW"ç&VD4FFU$Æ—B’F†B†æG2F†R&W7VÇBFòæWr¦f67&—D–çFW&f6V'&–FvP¢†&÷fW4f–ÆT'&–FvV’&F†W"F†âG'––ærFò&W6öÇfRF†R&Æö#¢U$Â2&VÂæWGv÷&²&WVW7Bà¢FF¦U$Ç2&RFV6öFVBF—&V7FÇ’Âæò¥2&÷VæB×G&—æVVFVBâ&VÂ‡GG‡2’F÷væÆöB†æ÷B¢6fRW‡÷'B’fÆÇ2&6²FòF†R7—7FVÒF÷væÆöDÖævW&à¢Ò¢¥v†W&Rf–ÆW2ÆæC¢¢¢Fö7VÖVçG2óÇF†—2w2÷vâÆVæ6†W"Æ&VÃâóÆf–ÆTæÖSæÂf–F†P¢ÖVF–7F÷&Vf–ÆW6öFö7VÖVçG66öÆÆV7F–öâ†$TÄD•dUõD†’Âæ÷B&rW‡FW&æÂ7F÷&vR(	@¢æVVG2æòu$•DUôU…DU$äÅõ5Dõ$tVW&Ö—76–öâBÆÂöâ’#’²‡F†—2w2÷vâÖ–å6F¶’à¢&VÂÂÆ–W"×f—6–&ÆRÆö6F–öâ„f–ÆW2ÂU4"ôÕE’Âæ÷Bâ×&—fFRF—&V7F÷'’à ¢¢¦”õ3¢¢¢F†RWV—fÆVçBv2ÂFFVBFòtµvV%f–Wrw2÷vâ6öç7G&–çG3 ¢Ò¢¤W‡÷'B¢¢(	BtµvV%f–Wr†2æòF÷væÆöDÆ—7FVæW&WV—fÆVçBF†Bf—&W2f÷"¥2×G&–vvW&V@¢ÆF÷væÆöCæ6Æ–6²öâ&Æö#¢öFF¢U$ÂBÆÂ†t´æf–vF–öäFVÆVvFVw0¢FV6–FUöÆ–7”f÷"æf–vF–öå&W7öç6Vöt´F÷væÆöDFVÆVvFVöæÇ’6÷fW"&VÂF÷ÖÆWfVÀ¢æf–vF–öâFòF÷væÆöF&ÆR&W7öç6RÂæWfW"F†—266R’âf—†VBv—F‚F†R6ÖR6†Rö`¢v÷&¶&÷VæB2æG&ö–Bw2&Æö"†æFÆ–ærÂFFVC¢tµW6W%67&—F–æ¦V7FVBBFö7VÖVçB×7F'@¢–çFW&6WG26Æ–6¶WfVçG2öâ¶F÷væÆöEÖVÆVÖVçG2v†÷6R‡&Vf—2&Æö#¦öFF¦À¢&WfVçG2F†RFVfVÇB7F–öâÂ6öçfW'G2&Æö"FòFFU$ÂF†R6ÖRv’†fWF6†°¢f–ÆU&VFW"ç&VD4FFU$Æ’ÂæB÷7G2—BFòtµ67&—DÖW76vT†æFÆW&†&÷fW56fTf–ÆV¢F†BFV6öFW2æBw&—FW2—B–çFòF†—2w2÷vâFö7VÖVçG2ö(	BÇ&VG’&—fFRW"Ööà¢”õ2Â6ò‡VæÆ–¶RæG&ö–B’F†W&Rw2æò6†&VB6öÆÆV7F–öâFòF—6Ö&–wVFR&WGvVVâ2æ@¢F†W&Vf÷&RæòW"ÖvÖR7V&föÆFW"Fò7&VFRà¢Ò¢¤–×÷'B(	B&VÂvÂöæÇ’'F–ÆÇ’6Æ÷6VC¢¢¢tµT”FVÆVvFVw0¢vV%f–Wr…ó§'Vä÷VåæVÅv—Fƒ¦–æ—F–FVD'”g&ÖS¦6ö×ÆWF–öä†æFÆW#¢–—2F†RöæÇ’†öö²f÷ ¢Æ–çWBG—SÒ&f–ÆR#æ–âtµvV%f–WrÂ&6¶VB'’T”Fö7VÖVçE–6¶W%f–Wt6öçG&öÆÆW&â¢¤6öæf—&ÖV@¢F—&V7FÇ’v–ç7BÆRw2÷vâFö7VÖVçFF–öâ‡ÆFf÷&Òf–Æ&–Æ—G’F&ÆRÂæ÷B77VÖVB“¢F†—0¢ÖWF†öBöæÇ’W†—7G27F'F–ær”õ2ö•Dõ2‚ãB¢¢(	Bf–ÆR–çWB7W÷'B–âtµvV%f–Wr—2¢vVçV–æVÇ’&V6VçBvV$¶—BFF—F–öã²—BW†—7FVBöâÖ4õ26–æ6Rã"'WBæWfW"öâ”õ2VçF–À¢‚ãBâF†—2&ö¦V7Bw2÷vâFWÆ÷–ÖVçBF&vWB—2”õ2Rã†7W÷'Bö–÷2ö'VæFÆRç–’ÂvVÆÀ¢&VÆ÷rF†Bâ–×ÆVÖVçFVBæBvFVB&V†–æBf–Æ&ÆR†”õ2‚ãBÂ¢–&F†W"F†â6¶—V@¢VçF—&VÇ’÷"W6VBFò§W7F–g’&—6–ærF†RFWÆ÷–ÖVçBF&vWC¢öâ”õ2Rã(	3‚ã2f–ÆP¢Æ–çWCæ7F—2W†7FÇ’2–æW'B2—BÇ&VG’v2†æò&Vw&W76–öâ’ÂæB7F'G2v÷&¶–ærF†P¢ÖöÖVçBF†Rõ2—G6VÆbFöW2Âv—F‚¦W&ò6öFR6†ævW2æVVFVBöæ6R&VÂ×v÷&ÆBF÷F–öâö`¢‚ãB²—2†–v‚Væ÷Vv‚æ÷BFòÖGFW"âf—'7BfW'6–öâöbF†—2f—‚FV6Æ&VBF†RÖWF†ö@¢VæwV&FVBÂ77VÖ–ær‡w&öævÇ’Âg&öÒÖVÖ÷'’Âæ÷BfW&–f–VB’F†B”õ2BãRFFVB—B(	B6Vv‡@¢&Vf÷&R6öÖÖ—GF–ær'’7GVÆÇ’6†V6¶–ærÆRw2Fö7VÖVçFF–öâ¥4ôâF—&V7FÇ’Âv—fVâF†P¢&Wf–÷W2GvòVçG&–W2r÷vâÆW76öâ&÷WBæ÷BG'W7F–ærVçfW&–f–VB&V6ÆÂf÷"W†7BÆFf÷&Ğ¢’6öçG&7G2à ¢¢¤æ÷BfW&–f–VBöâ&VÂFWf–6R÷"6–×VÆF÷"ÂV—F†W"ÆFf÷&Ò¢¢(	BæG&ö–Bw27V6–f–2f—‚†W&P¦föÆÆ÷w2F†R6ÖR&VÂÖFWf–6R×&W÷'BGFW&âF†R&Wf–÷W2GvòVçG&–W2F–BÂ'WBF†Rf—‚—G6VÆ`¦†6âwB&VVâ&RÖ6öæf—&ÖVBv÷&¶–ær–WB†æòFWf–6R66W72g&öÒF†—26W76–öâgFW"F†R&W÷'Bv0¦f–ÆVB“²”õ2†2æWfW"&VVâ'VçF–ÖR×FW7FVBBÆÂF†—26W76–öâ†æòÖ4õ2õ†6öFR’â&÷F‚6†÷VÆB&P§&RÖ6†V6¶VB&Vf÷&R&V–ærG&VFVB26WGFÆVBà ¢ÒÒĞ ¢22##bÓ’ÓB(	Bv—&RÒÖ–÷6öÒÖ–÷2×&VÆV6V–çFòÖ6‚'VæFÆV†ö'VæFÆUö–÷6 ¢¢¤f–ÆW3¢¢¢—F†öâ÷6W'fò÷÷7Eö'V–ÆEö6öÖÖæG2ç–†æWrÒÖ–÷6öÒÖ–÷2ÖÖæÖVöÒÖ–÷2Ö'VæFÆRÖ–Fğ¦ÒÖ–÷2×&VÆV6VfÆw2Âö'VæFÆUö–÷6Â÷6–våöæEöW‡÷'Eö–÷5÷&VÆV6V’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVB(	B6ÖRf–ÆRF†—0§F6‚Ç&VG’÷vç2Â6VRF†R##bÓ’Ó2VçG'’&÷fRf÷"v‡’”õ2ôæG&ö–BÖö&–ÆR6†ævW2&P¦6öç6öÆ–FFVB–çFòF†—2öæRF6‚&F†W"F†âæWrçVÖ&W&VBöæR’à ¢¢¥v‡“¢¢¢F†R##bÓ’Ó2VçG'’w2÷vâ$¶æ÷vâv2"æBF†R##bÓ’ÓB4’×&—G’VçG'’w2$ÆVgB÷Và¦öâW'÷6R"&÷F‚fÆvvVBF†R6ÖRF†–æs¢7W÷'Bö–÷2ö'VæFÆRç–v27FæFÆöæR67&—BÂæ÷@§&V6†&ÆRg&öÒÖ6‚'VæFÆVF†Rv’ÒÖæG&ö–F—2(	B‡VÖâöâÖ4õ2†BFò'Và¦'VæFÆRç–ö†6öFVvVæö†6öFV'V–ÆF'’†æBâDôDòæÖBö–çB‚ã"G&6¶VBF†—22F†RæW‡B×&–÷&—G¦”õ2vâ6Æ÷6VBæ÷rÂföÆÆ÷v–ærö'VæFÆUöæG&ö–Fw2W†7B6†R‡F†—2ÖWF†öBF¶W2æğ¦6W'fõö&–æ'–÷F&vWB×G&—ÆR–çföÇfVÖVçBV—F†W"Â6–æ6R†6öFTvVâ÷†6öFV'V–ÆBæVVBæV—F†W"6W'fòæ÷ ¦'W7B7&÷72Ö6ö×–ÆR(	BÖF6†–ærF†BÖWF†öBw2÷vâ&æò'W7B7&÷72Ö6ö×–ÆR"&V6öæ–ær’à ¢¢¥v†Bö'VæFÆUö–÷6FöW3¢¢¢Ö4õ2ÖöæÇ’†6†V6¶VBf–—5öÖ6÷7‚‚–Â&VgW6W2Æ÷VFÇ’öâv–æF÷w2ğ¤Æ–çW‚(	B†6öFV'V–ÆFö†6öFVvVæ&RÆRÖöæÇ’FööÇ2Âæò7&÷72×ÆFf÷&ÒWV—fÆVçB’â&WW6W0¦7W÷'Bö–÷2ö'VæFÆRç–w27FvR‚–2Ö—2†ÆöFVBf––×÷'FÆ–"çWF–Âç7V5ög&öÕöf–ÆUöÆö6F–öæ §&F†W"F†âGWÆ–6FVB(	B'VæFÆRç–7F—2–æFWVæFVçFÇ’–çfö6&ÆRFöòÂ6–æ6R&÷fW2Ö7F–öæw0¦–÷6–çWBæB&&RÖ6†V6¶÷WB6öç7VÖW"&÷F‚7F–ÆÂ6ÆÂ—BF—&V7FÇ’v—F†÷WBgVÆÂVæv–æP¦'V–ÆB’â7FvW2–çFò67&F6‚F&vWBö–÷2Ö'VæFÆV†Ö—'&÷'2ö'VæFÆUöæG&ö–Fw2÷và¦F&vWBóÇG&—ÆSâöæG&ö–BÖ'VæFÆV67&F6‚Ö6÷’&V6öæ–ær(	B&WVBÖ6‚'VæFÆRÒÖ–÷6f÷"¦F–ffW&VçBvÖR7F'G26ÆVâ’â'Vç2†6öFVvVâvVæW&FVF†VâÂv—F†÷WBÒÖ–÷2×&VÆV6VÂF†RW†7@§Vç6–væVB†6öFV'V–ÆB×6F²—†öæW6–×VÆF÷"âââ4ôDUõ4”tä”äuôÄÄõtTCÔäö–çfö6F–öâ–÷2ç–ÖÆ ¦Ç&VG’&â'’†æB(	BF†R&W7VÇF–æræ—26÷–VBFòÒÖ÷WGWFà ¢¢¦ÒÖ–÷2×&VÆV6V(	B&VÂ6–væ–ærÂæ÷Bæ÷F†W"&æò6–væ–ær6öæ6WB"&VgW6Ã¢¢¢VæÆ–¶RFW6·F÷w0¦ÒÖFV&öÒÖ×6–öÒÖFÖv†æò6–væ–ær6öæ6WBBÆÂ’æBÖ—'&÷&–ærÒÖæG&ö–B×&VÆV6Vw2÷và¢'&VgW6RÆ÷VFÇ’&F†W"F†â6–ÆVçFÇ’&öGV6R6öÖWF†–ærvV¶W""7Fæ6RÂÒÖ–÷2×&VÆV6V&WV—&W0¦”õ5õ4”tä”äuô4U%D”d”4DUõ%õD†‚²õ55tõ$F’Â”õ5õ4”tä”äuõ$õd•4”ôä”äuõ$ôd”ÄUõD†æ@¦”õ5õ4”tä”äuõDTÕô”FÇ&VG’6WB–âF†RVçf—&öæÖVçBÂW†7FÇ’F†R6ÖR&Vçbf'2–âÂ&VgW6R–`¦Ö—76–ær"6†R2æG&ö–Bw2µõ4”tä”äuô´U•õ5Dõ$UõD†V'FWB(	BæòæWr6öæf–rf–ÆRÂæòfÆw0¦f÷"6V7&WG2â÷6–våöæEöW‡÷'Eö–÷5÷&VÆV6VF†Vã¢7&VFW2F‡&÷vv’¶W–6†–âVæFW"F†R67&F6€¦'V–ÆB&ö÷B†æWfW"F†R6ÆÆW"w2Æöv–â¶W–6†–â(	B4’†2æöæRÂæBF†—26†÷VÆFâwBF÷V6‚Æö6À¦FWbw2÷vâV—F†W"’Â–×÷'G2F†Rç&–çFò—BÂw&çG26öFW6–væö6V7W&—G–66W70¢†6WBÖ¶W’×'F—F–öâÖÆ—7FÂ÷F†W'v—6RWfW'’W6RöbF†R–×÷'FVB¶W’&ö×G2–çFW&7F—fVÇ’(	BfFÀ¦–â4’’ÂW6†W2—BöçFòF†R¶W–6†–â6V&6‚Æ—7BÂFV6öFW2F†R&÷f—6–öæ–ær&öf–ÆRw2UT”@¢†6V7W&—G’6×2ÔFÂF†RöæÇ’7W÷'FVBv’Fò&VBöæR’æB–ç7FÆÇ2—BBF†RW†7BF‚öæÖP¢†âôÆ–'&'’ôÖö&–ÆTFWf–6Rõ&÷f—6–öæ–ær&öf–ÆW2óÇWV–CâæÖö&–ÆW&÷f—6–öæ’†6öFR—G6VÆbÆöö·2—BW ¦'’ÂF†Vâ'Vç2†6öFV'V–ÆB&6†—fV†ÖçVÂ6–væ–ær7G–ÆRÂW‡Æ–6—BFVÒ”B²&öf–ÆR7V6–f–W"¦föÆÆ÷vVB'’ÖW‡÷'D&6†—fVv—F‚vVæW&FVBW‡÷'D÷F–öç2çÆ—7F†ÖWF†öC¢×7F÷&V’âF†P§FV×÷&'’¶W–6†–â—2Çv—2FVÆWFVB–âf–æÆÇ–Âv†WF†W"6–væ–ær7V66VVFVB÷"æ÷Bà ¢¢¥v‡’æG&ö–BæB”õ26–væ–ærÆöö²ÆÖ÷7BÂ'WBæ÷BV—FRÂF†R6ÖS¢¢¢&÷F‚&R&Vçbf'2–âÀ§&VgW6R–bÖ—76–ær"(	BF†RF–ffW&Væ6R—2§v‡’¢F†R7&VFVçF–Â6âwB&RvVæW&FVB'’v†öWfW"w0§'Vææ–ærF†—2ââæG&ö–B¶W—7F÷&R—26VÆb×6–væVB'’FW6–vâ…Æ’66WG2v†FWfW"¶W’F†P¦FWfVÆ÷W"†öÆG2“²âÆRF—7G&–'WF–öâ6W'F–f–6FR×W7B&R6÷VçFW'6–væVB'’ÆR—G6VÆ`¢‡WÆöF–ær55"FòF†RÆRFWfVÆ÷W"&öw&ÒÂöæÇ’F†R66÷VçB†öÆFW"6âFòF†B’(	B6ğ§VæÆ–¶Rµõ4”tä”äuô´U•õ5Dõ$UõD†Â”õ5õ4”tä”äuô4U%D”d”4DUõ%õD†6âæWfW"&R6öÖWF†–æp§F†—2&WòÂ4’Â÷"ç’FööÆ–ærÖçVf7GW&W2VæB×FòÖVæBöâ—G2÷vã²—BÇv—2G&6W2&6²Fò¦‡VÖâv—F‚ÆRFWfVÆ÷W"&öw&Ò66W72à ¢¢¥fW&–f–6F–öã¢¢¢7–çF‚Ö6†V6¶VB†7Bç'6V’(	BæòÆö6Â'W7Bõ†6öFRFööÆ6†–â–âF†—26W76–öâFğ§'VâÖ6†—G6VÆb‡F†—2&Wòw2÷vâ¶æ÷vâv–æF÷w2FööÆ6†–âvÂ6VRF†—2f–ÆRw2V&Æ–W"VçG&–W3°¥†6öFR—2FF—F–öæÆÇ’Ö4õ2ÖöæÇ’&Vv&FÆW72öbÆFf÷&Ò’â&VÂfW&–f–6F–öâ—24“¢–÷2ç–ÖÆ §WFFVBFò6ÆÂâöÖ6‚'VæFÆRÒÖ–÷6f÷"F†RVç6–væVBF‚‡&WÆ6–ær—G2&Wf–÷W2F—&V7@¦'VæFÆRç–ö†6öFVvVæö†6öFV'V–ÆF7FW2Â6ò4’æ÷rW†W&6—6W2F†R6ÖR6öFRF‚WfW'’&VÀ¦6öç7VÖW"W6W2(	BF†R6ÖR&V6öæ–æræG&ö–Bç–ÖÆw2÷vâ##bÓ’Ó2&Ww&—FRÇ&VG’Æ–VB’ÂÇW0¦æWr6–væ–ær×fW&–f–6F–öâ¦ö"f÷"V6‚ÆFf÷&ÒW6–ærg&W6†Ç’ÖvVæW&FVB¢§FW7B¢¢7&VFVçF–Ç0¢†&VÂÂ6VÆbÖ6öçF–æVBæG&ö–B&VÆV6R¶W—7F÷&R(	B6VÆb×6–væVB¶W—2&R†÷ræG&ö–B6–væ–æp¦æ÷&ÖÆÇ’v÷&·2Âæ÷F†–ærf¶R&÷WB—C²æB6VÆb×6–væVB”õ26W'F–f–6FRF†B6âöæÇ’&÷fRF†P¦¶W–6†–âÖ–×÷'B†Æböb÷6–våöæEöW‡÷'Eö–÷5÷&VÆV6VÖV6†æ–6ÆÇ’v÷&·2Â6–æ6R&VÀ¤ÆR×G'W7FVB&÷f—6–öæ–ær&öf–ÆR6ææ÷B&R7–çF†W6—¦VBv—F†÷WBâ7GVÂÆRFWfVÆ÷W ¥&öw&Ò66÷VçB(	B6VRF†Ræ÷FR&÷fR’â¢¦ÒÖ–÷2×&VÆV6Vw2gVÆÂ&6†—fR¶W‡÷'BF‚—2æ÷B–W@§fW&–f–VBVæB×FòÖVæB¢¢(	BF†BæVVG2&VÂÆRF—7G&–'WF–öâ6W'F–f–6FR²&÷f—6–öæ–ær&öf–ÆP§7WÆ–VB2&W÷6—F÷'’6V7&WG2'’6öÖVöæRv—F‚ÆRFWfVÆ÷W"&öw&Ò66W72FòF†P¢$E&–æ72&öGV7F–öç2"66÷VçC²55"v2vVæW&FVBf÷"W†7FÇ’F†—2W'÷6R††æFVBFòF†RW6W ¦÷WBÖöbÖ&æBÂæ÷B6öÖÖ—GFVBç—v†W&R’'WB7V&Ö—GF–ær—BFòÆRw2÷'FÂ—2ÖçVÂ7FWöæÇ¦â66÷VçB†öÆFW"6âFòà ¢¢¥6ÖR×GW&âföÆÆ÷r×W¢æv—F‡V"÷v÷&¶fÆ÷w2ö–÷2ç–ÖÆw2&÷fW5ö–÷5÷&ö¦V7Bç¦—æ÷rÇ6ò6'&–W0¦&W6÷W&6W2÷6W'fõó#Bçæv²F†RÖWFÂÖæ–föçBf–ÆW2Âæ÷B§W7B7W÷'Bö–÷2öâ¢¢f÷VæBv†–ÆP¦FF–ær”õ27W÷'BFò6¶Ö7FW"†&÷fW2×6¶Ö7FW&Â6–&Æ–ær6†V6¶÷WB“¢7W÷'Bö–÷2ö'VæFÆRç–w0¦7FvR‚–6÷–W2F†÷6R'&æF–ær76WG2g&öÒF†RVæv–æR&Wòw2÷vâF÷ÖÆWfVÂ&W6÷W&6W2ö ¦F—&V7F÷'’B'V–ÆBF–ÖR‡6VRF†R##bÓ’Ó2VçG'’&÷fRÂ%v†Bw2æWröâ”õ2"’(	Bf–æRf÷ ¦ç—F†–ærv—F‚gVÆÂVæv–æR6†V6¶÷WB‡F†—2&Wòw2÷vâ–÷2ç–ÖÆÂ&÷fW2Ö7F–öæ’Â'W@¥6¶Ö7FW"w2÷vâ–÷2ç'6†Ö—'&÷&–ær†÷r—G2W†—7F–æræG&ö–Bç'6F÷væÆöG0¦&÷fW5öæG&ö–E÷&ö¦V7Bç¦—–ç7FVBöbæVVF–ær6†V6¶÷WBBÆÂ’öæÇ’WfW"vWG2F†—2öæR¦—à¤æG&ö–BæWfW"†BF†—2v&V6W6R7W÷'BöæG&ö–Bö²öw2÷vâG&6¶VBG&VRÇ&VG’6'&–W0¦&W2öÖ—Ö÷6W'fòçvV'–ç6–FR—B(	B”õ2w2WV—fÆVçB76WG2Æ—fRöæRÆWfVÂWÂ÷WG6–FP¦7W÷'Bö–÷2öVçF—&VÇ’Â6òF†R¦—7FWæVVFVBFòW‡Æ–6—FÇ’&V6‚f÷"F†VÒFöòâæòF6‚(	@§F†—2—2F†—2&Wòw2÷vâ4’f–ÆRÂæ÷BfVæF÷&VBà ¢22##bÓ’ÓR(	Bf—‚Ö6‚'VæFÆRÒÖ–÷6f–Æ–ær–ÖÖVF–FVÇ’Â&Vf÷&RWfW"&V6†–ærö'VæFÆUö–÷6  ¢¢¤f–ÆS¢¢¢—F†öâ÷6W'fòö6öÖÖæEö&6Rç–†6öÖÖöåö6öÖÖæEö&wVÖVçG6w2&–æ'•÷6VÆV7F–öæ&Æö6²’à ¢¢¥F6ƒ¢¢¢F6†W2÷6W'fò×cãRãóbÖÖö&–ÆRÖæF—fR×vV'f–Ww2çF6†‡&VvVæW&FVB(	BVæFVBæWp¦F–fbf÷"F†—2f–ÆRÂv†–6‚F†RF6‚F–FâwB&Wf–÷W6Ç’F÷V6‚BÆÃ²6VRF†R##bÓ’Ó2VçG'¦&÷fRf÷"v‡’”õ2ôæG&ö–BÖö&–ÆR6†ævW2&R6öç6öÆ–FFVB–çFòF†—2öæRF6‚&F†W"F†âæWp¦çVÖ&W&VBöæR’à ¢¢¥v‡“¢¢¢fÆvvVB2â÷Vâ'Vr–âF†R&Wf–÷W26W76–öâw2„äDôdbæÖF(	B–÷2ç–ÖÆw2–÷6¦ö ¦f–ÆVBÖ6‚'VæFÆRÒÖ–÷6–âãb6V6öæG2v—F‚öæÇ’vVæW&–2%&ö6W726ö×ÆWFVBv—F‚W†—B6öFR ¢†æò7W7FöÒææ÷FF–öâFò&VBæöç–Ö÷W6Ç’ÂæBæòv—D‡V"Bv2f–Æ&ÆR–âF†B6W76–öâFğ§VÆÂF†R&VÂ¦ö"Æör’â6öæf—&ÖVB'’&VF–ærF†R6öFRÂæ÷BF†RÆös¢'VæFÆR‚–—2FV6÷&FVBv—F€¦6öÖÖæD&6Ræ6öÖÖöåö6öÖÖæEö&wVÖVçG2†&–æ'•÷6VÆV7F–öãÕG'VRÂ'V–ÆEö6öæf–wW&F–öãÕG'VR–âF†P¦&–æ'•÷6VÆV7F–öæ&Æö6²&W6öÇfW26W'fõö&–æ'–Væ6öæF—F–öæÆÇ’VæÆW70¦6VÆbçF&vWBææVVG5÷6¶v–ær‚–—2G'VR(	Bv†–6‚—2W†7FÇ’†÷rÒÖæG&ö–FöÒÖö†÷66¶——BÀ§6–æ6[ZŠW«®Šğ®+b-jwZ­Ú.¶›­º$zzb¥æÚ±î¸Â¸­yêë¢°k¢G§¦*^ `configure_build_target` (part of the same `build_configuration=True`) routes those into an
`AndroidTarget`/`OpenHarmonyTarget` (`CrossBuildTarget` subclasses whose `needs_packaging()` is
`True`). **iOS has no `BuildTarget` subclass of its own** â€” `--ios` never sets `--target`, so
`self.target` stays the host default (macOS), whose `needs_packaging()` is `False`. That routed
`--ios` straight into `self.get_binary_path(...)`, which unconditionally requires a prior
`./mach build` (`raise BuildNotFound("No Servo binary found. Perhaps you forgot to run
\`./mach build\`?")`) â€” something `--ios` never needs, since `_bundle_ios` stages/builds a native
Swift/WKWebView app via XcodeGen, never a Servo/Rust binary at all (same reasoning `_bundle_android`
already documents for why it takes no `servo_binary` either). This crashed inside the decorator,
*before* `bundle()`'s own body â€” and therefore its `if ios: return self._bundle_ios(...)` branch
â€” ever ran, matching the observed instant failure exactly (`ios.yml` never runs `mach build`
first, only `mach bundle --ios` directly).

**Fix:** widened the skip condition to `self.target.needs_packaging() or kwargs.get("ios")` â€”
`kwargs.get("ios")` is safe (returns `None`) for every other `binary_selection`-decorated command
(`build`, `run`, `package`) that has no `--ios` flag at all. The `--bin`-conflict error message one
line below was widened the same way (`target_description = "ios" if kwargs.get("ios") else
self.target.triple()` â€” calling `self.target.triple()` unconditionally would itself have thrown
for the iOS case, a real host desktop target has a triple but that's not the relevant one to name
in the error).

**Verification:** syntax-checked (`ast.parse`) and reasoned through by hand against
`_bundle_android`'s already-working equivalent â€” no local Rust/mach toolchain in this session
(this repo's own known Windows toolchain gap, see this file's earlier entries). Patch
re-verified to apply cleanly to a fresh pristine `v0.5.0` extraction.

**Confirmed fixed on real CI** (run for commit `3a202fbbcf1`,
<https://github.com/DRincs-Productions/roves/actions/runs/34939544032>): the `ios` job's
`mach bundle --ios` step went from failing in ~6 seconds to succeeding in ~46 seconds (a real
XcodeGen + `xcodebuild` build this time), and the whole `ios` job is now green end to end â€”
artifact upload, `roves_ios_project.zip` packaging, and the "test" release upload all
succeeded too.

**Left open at the time â€” a separate, unrelated failure in the same CI run, that turned into its
own multi-round saga:** `ios-release-signing-smoke` (the self-signed-test-certificate
keychain-import smoke test, see the 2026-09-14 entry above) also failed in the same run, at the
`security import ... -k "$keychain"` step. Not the same bug â€” that job never calls `mach bundle`
at all â€” but chasing it down took five wrong turns before landing on the real fix, worth reading
in order since each one looked completely plausible until the next run disproved it:

1. **"The two secrets drifted apart."** The real job log (pulled with a user-provided
   read-only PAT â€” anonymous annotations only ever showed the generic "exit code 1") showed
   `SecKeychainItemImport: MAC verification failed during PKCS12 import (wrong password?)`.
   Regenerated and re-uploaded a fresh, verified-matching `IOS_CI_TEST_P12_BASE64`/`_PASSWORD`
   pair. Same error. A byte-for-byte diff proved the re-uploaded value matched exactly, so this
   theory was wrong.
2. **"Whitespace from pasting the secret."** Made the password-consuming step strip
   `[:space:]` before use. Same error again.
3. **Removed the secrets entirely.** At this point the user pointed out â€” correctly â€” that
   both this iOS test cert and Android's analogous release-signing keystore exist purely to
   smoke-test a signing *mechanism*, never reused for a real release, so there was never a
   reason to persist either as a secret at all. Rewrote both `ios-release-signing-smoke`
   (`ios.yml`) and `android-release-signing` (`android.yml`) to generate a throwaway identity
   fresh on the runner instead: iOS via `openssl req -x509`/`openssl pkcs12 -export` (CN
   `Roves CI Test Signing`, `codeSigning` EKU), Android via `keytool -genkeypair` (already on
   `PATH` from `setup-java`) â€” both using a `uuidgen` password threaded via `$GITHUB_ENV`
   (`::add-mask::`-masked). This deleted `IOS_CI_TEST_P12_BASE64`/`_PASSWORD` and
   `ANDROID_KEYSTORE_BASE64`/`_PASSWORD`/`ANDROID_KEY_ALIAS`/`_PASSWORD` as dead secrets
   (removed from GitHub) â€” a real improvement kept regardless of what came next.
4. **The actual bug, finally found:** the very first run after generating everything fresh â€”
   cert, key, and password all created together in the same job, zero secrets or copy-paste
   anywhere â€” failed with the *exact same* "wrong password?" error. That's what finally ruled
   out every secret/whitespace theory at once: OpenSSL 3.0 changed `openssl pkcs12 -export`'s
   default encryption to PBES2/PBKDF2/AES-256-CBC, which macOS's `security import` (the legacy
   `SecKeychainItemImport` API) cannot decode at all â€” and reports as "wrong password?" instead
   of an unsupported-format error, which is exactly what sent every prior round in the wrong
   direction. Fixed with `-certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg SHA1` (verified
   locally that this switches the output to `pbeWithSHA1And3-KeyTripleDES-CBC`). Deliberately
   not the commonly-suggested `-legacy` flag: that needs RC2-40-CBC, missing from this
   session's local OpenSSL's legacy provider entirely â€” the explicit `-certpbe`/`-keypbe`
   combination needs no special provider.
5. **A detour chasing a check the real code never makes.** With the PBE fixed, `security
   import` finally succeeded ("1 identity imported") â€” but the next check,
   `security find-identity -v -p codesigning`, reported "0 valid identities found": a
   self-signed cert isn't trusted for anything by default, and that policy-validated lookup
   requires a trust chain a bare self-signed leaf can't have (a real Apple Distribution cert
   wouldn't hit this, since it chains to Apple's already-trusted root). Tried explicitly
   trusting the cert via `security add-trusted-cert` â€” which then *hung* the job for 12+
   minutes waiting on a `SecurityAgent` GUI authorization dialog that no headless runner can
   ever click (needing a manual cancellation of the stuck run, since a read-only PAT can't
   cancel one), then, after adding the standard `authorizationdb write
   com.apple.trust-settings.admin allow` non-interactive workaround plus `sudo`, still failed
   with `NO (-60005)` (`errSecAuthFailed`) â€” modern macOS apparently blocks system trust-store
   writes outright regardless of that workaround. Stepping back at this point and actually
   reading `_sign_and_export_ios_release` (the real production code this job is meant to
   smoke-test) showed it **never calls `find-identity` or validates trust at all** â€” it hands
   the identity straight to `xcodebuild archive`, which resolves signing via the provisioning
   profile + team ID instead. The whole trust-chasing detour was chasing a self-imposed
   requirement the real code doesn't have.

**Final fix:** dropped `-p codesigning`/`-v` from the verification `find-identity` call entirely
â€” a bare `security find-identity "$keychain"` lists every identity present regardless of trust
validity, which is all this job ever needed to prove (that the cert + matching key made it into
the keychain), matching what `_sign_and_export_ios_release` itself actually relies on.

**Confirmed on real CI**: run
<https://github.com/DRincs-Productions/roves/actions/runs/34967882996> â€” `ios-release-signing-smoke`
green, completing in seconds like every other step in the job, alongside `ios` and
`ensure-test-release` both also green. Six wrong turns (two secret-sync theories, a PBE-format
bug hiding behind a misleading "wrong password" message, a trust-chasing detour that hung the
job then hit a hard macOS security wall, and finally realizing the check itself was testing a
requirement the real code never has) to reach a fix that ended up removing code rather than
adding more of it â€” the lesson being that `security`'s error messages are not reliable guides to
the actual failure, and it's worth checking what the production code an ad-hoc CI check is
supposedly mirroring *actually* does before hardening the check further.

## 2026-09-15 â€” `android-actions/setup-android@v3`'s default `tools` package no longer exists

**File:** `.github/workflows/android.yml` (both `android` and `android-release-signing` jobs'
`android-actions/setup-android@v3` steps).

**Why:** while chasing the iOS signing saga above, both Android CI jobs also failed on the same
run at this same step â€” looked at first like the transient `android-actions/setup-android` flake
a much earlier commit's own message already mentions retrying past once. It wasn't: the real log
showed `Warning: Failed to find package 'tools'` followed by `sdkmanager` exiting 1. Google has
removed the legacy Android SDK `tools` package (the old standalone `android`/`monitor`/etc.
bundle, deprecated for years) from its repository entirely â€” `android-actions/setup-android@v3`
still requests it by default (`packages: tools platform-tools` when no `packages` input is given)
and now hard-fails immediately instead of the license-acceptance step it used to sail through.
This is a real, permanent break caused by an upstream removal, not something that resolves on
retry.

**Fix:** pass `packages: platform-tools` explicitly to both `setup-android` steps, dropping
`tools`. Nothing in this workflow needed it: `platform-tools` (`adb`) plus the specific
`platforms;android-NN`/`build-tools;NN.N.N` the very next step (`Install Android SDK`) installs
by name are everything a Gradle-based `mach bundle --android` actually touches.

**Verification:** confirmed on real CI (run
<https://github.com/DRincs-Productions/roves/actions/runs/34966071804>) â€” `android` and
`android-release-signing` both fully green, `setup-android` included, `mach bundle --android`/
`--android --android-release` and `apksigner verify` all passing (the in-CI-generated release
keystore from the entry above working end to end for the first time).

---

## 2026-09-15 â€” Desktop save export/import: `<a download>` never worked, "open" dialog opened the wrong folder

**Files:** `ports/servoshell/desktop/app.rs`, `ports/servoshell/desktop/protocols/roves.rs`,
`ports/servoshell/desktop/dialog.rs`, `ports/servoshell/desktop/event_loop.rs`,
`ports/servoshell/desktop/headed_window.rs`, `ports/servoshell/desktop/tracing.rs`,
`ports/servoshell/Cargo.toml`.

**Patch:** `patches/servo-v0.5.0/0001-desktop-shell-core.patch` (app.rs, dialog.rs,
event_loop.rs, headed_window.rs, tracing.rs, Cargo.toml),
`patches/servo-v0.5.0/0002-desktop-protocols.patch` (roves.rs).

**Reported by a real user testing `visual-novel-template`'s Save/Load screen on a real Windows
build:** clicking "save to file" (`save.download()`, an `<a download>` click on a `blob:` URL â€”
see that template's `src/lib/utils/save-utility.ts`) navigated to an error page reading exactly
`Could not load the requested page: InvalidOrigin`; clicking "load from file" opened a native
file picker correctly pointed at the game's own `saves/` folder, but the folder looked empty
even though a real save (visible in the game's own save-slot grid) existed.

**Root cause, `<a download>`:** not implemented at all in this Servo tree â€” stock upstream, not
a Roves patch (`components/script/dom/html/htmlanchorelement.rs` has a literal
`// TODO: Download the link is `download` attribute is set.`) â€” so every click just does a real
top-level navigation to the `blob:` URL. That navigation path never attaches the creating
document's origin to the blob (`ensure_blob_referenced_by_url_is_kept_alive` in
`components/script/url.rs` is only wired into `fetch`/XHR/media/worker call sites, not hyperlink
navigation), so the origin gets re-derived from the serialized `blob:` URL text instead
(`components/net/protocols/blob.rs`). That works for an ordinary `https://` document (a tuple
origin round-trips through a `blob:` URL), but never for a Roves game: `game://` documents get
an **opaque** origin by design (`0053-virtual-content-root-game-protocol`,
`components/url/origin.rs`'s `new_opaque_for_game_content`), and an opaque origin can't
round-trip through a `blob:` URL at all â€” guaranteeing a mismatch in
`components/net/filemanager_thread.rs`'s `get_impl`, which is exactly where
`BlobURLStoreError::InvalidOrigin` comes from. Traced the exact string all the way through
`components/net/protocols/blob.rs`'s `format!("{:?}", err)` and
`components/script/dom/servoparser/mod.rs`'s `${reason}` substitution (no prefix added) to
confirm it produces that literal message, not a coincidence.

**Root cause, empty "open" folder:** not a filter bug â€” `accept="application/json"` (the
template's actual value) correctly maps to a `.json` extension filter via `mime_guess` in
`components/script/dom/html/form_controls/input_type/file_input_type.rs`'s `filter_from_accept`
(stock upstream, unmodified). The real problem: `Dialog::new_file_dialog` in `dialog.rs` never
set an initial directory at all, so the dialog opened wherever the OS/`egui_file_dialog`
happened to land â€” which turned out to be this game's own `saves/` folder (Roves' internal
save-slot storage, `.save` files â€” see the "Save-game storage API" entry above), a folder that
structurally never contains a manually exported `.json` file. Compounded by the `<a download>`
bug above: with export never actually succeeding, there was no exported file anywhere to find
regardless of which folder the picker opened to.

**Fix, three parts, all reusing existing infrastructure rather than inventing new plumbing:**

1. **A new injected userscript** (`DOWNLOAD_INTERCEPT_SCRIPT` in `app.rs`, registered alongside
   the existing `window.__ROVES__ = true;` one) intercepts `<a download>` clicks on `blob:`/
   `data:` hrefs with `event.preventDefault()` before Servo ever attempts to navigate â€” the same
   fix already shipped for both mobile WebView containers (see the entry below), applied to
   desktop for the first time. Reads the blob back out via `fetch()`+`FileReader.readAsDataURL`
   (base64), then calls a new `roves:save_file?filename=...&data=...` command.
2. **`roves:save_file`** (`protocols/roves.rs`) is the first command in this handler that
   genuinely has to wait on user interaction rather than answer immediately â€” it base64-decodes
   `data` and sends a new `AppEvent::SaveFileDialog { suggested_name, data, response }`
   (`event_loop.rs`) through the same `EventLoopProxy<AppEvent>` the existing `exit`/
   `close_window` commands already use to reach the main thread from a background protocol-
   handler thread (`RovesProtocolHandler` is `Send + Sync` and runs off-thread; `AppEvent` is
   winit's own cross-thread wakeup queue) â€” the one new piece is `response`, a
   `tokio::sync::oneshot::Sender<Result<(), String>>` the still-pending `fetch()`'s `Future`
   awaits, since (unlike `exit`) this needs a real answer back once the user picks a destination
   or cancels. `ports/servoshell/Cargo.toml`'s `tokio` dependency gained an explicit
   `features = ["sync"]` for this (previously depended on cross-crate feature unification with
   whatever else in the workspace happened to enable it, which happened to work but wasn't
   declared).
3. **A new `Dialog::SaveFile` variant** (`dialog.rs`), driven by `egui-file-dialog`'s own
   `DialogMode::SaveFile` (`FileDialog::save_file()`/`default_file_name()` â€” already a dependency,
   just an unused mode until now) â€” `App::user_event`'s handling for
   `AppEvent::SaveFileDialog` resolves a window/webview itself (this event has no originating
   `WebViewId` the way a DOM `<input type="file">`'s `EmbedderControlRequest` does â€” picks the
   first window with an active webview, which in this fork's usual single-window kiosk setup is
   always the one sensible choice) and calls a new `HeadedWindow::show_save_file_dialog` to add
   it. On `DialogState::Picked`, writes the bytes and answers `response`; on
   `Cancelled`/`Closed`, answers with an error.

**Also fixes the empty-folder bug at the root**, not just as a side effect of export now
working: both `Dialog::new_file_dialog` (open) and the new `Dialog::new_save_file_dialog` (save)
now call a shared `with_default_initial_directory` helper defaulting to `dirs::download_dir()`
(already a workspace dependency) instead of leaving `egui_file_dialog` to land wherever it
otherwise would â€” a real, if imperfect, improvement (a save exported somewhere else entirely
still needs manual navigation), chosen over doing nothing since the previous default landed
specifically in a folder that can *never* be correct for either dialog.

**Verification â€” three pushes, two real bugs, worth reading in order:**

1. **First push failed every `build-and-publish` leg and `steam-emulator-smoke-test`
   identically.** Real cause: `ports/servoshell/desktop/tracing.rs`'s
   `LogTarget for winit::event::Event<AppEvent>` impl matches every `AppEvent` variant
   explicitly with no wildcard arm, and the new `AppEvent::SaveFileDialog` variant wasn't
   covered â€” a plain `error[E0004]: non-exhaustive patterns`. Every leg failing identically
   pointed at code shared by all of them; grepping every `AppEvent` match site in the tree
   found the one uncovered arm. Fixed by adding the missing arm.
2. **Second push (with the `tracing.rs` fix) failed exactly the same way.** This time the
   real logs *were* fetchable (see below) â€” and the actual failure had nothing to do with
   Rust at all: it never got past `download + patch Servo source`.
   `patch` reported `The next patch would create the file ports/servoshell/Cargo.toml, which
   already exists!` for every one of the 6 files this entry's own hunks had regenerated
   (Cargo.toml, app.rs, dialog.rs, event_loop.rs, headed_window.rs, tracing.rs) â€” the
   `patches/`-splicing script used to regenerate them (per-file sections spliced into the
   existing multi-file patch, see the top of this file on why patches are grouped by
   subsystem) reconstructed each "modified file" header as
   `diff --git a/X b/X` / `index 000000000..000000000 100644` / `--- a/X` / `+++ b/X` â€” the
   all-zero `index` hash is git's own convention for "this blob doesn't exist", which made
   `patch`'s git-extended-header parsing treat the file as a *creation* regardless of what
   the `---`/`+++` lines said. The `roves.rs` "new file" section (patch `0002`) had the
   mirror-image bug â€” genuinely missing `--- /dev/null`/`+++ b/...` lines entirely, which
   made `patch` read it as a *deletion* instead. **This is exactly what the earlier
   `patch -p1 --dry-run` verification failed to catch**: it tested the raw, standalone hunk
   files this entry's own splicing script produced, never the *actual spliced patch file* â€”
   proving the ingredients were fine while the assembled dish was broken. Fixed by dropping
   the misleading `index` line from every "modified" section and adding the missing
   `/dev/null` header to the "new file" one, then re-verified for real this time: downloaded
   every pristine file `0001`/`0002` touch (including the two genuinely-new-upstream files,
   `bundle_launch.rs`/`logging.rs`, confirmed absent from pristine and left for `patch` itself
   to create) into one tree each and ran the exact `patch -p1` both patches actually get
   subjected to in CI â€” clean, no prompts, for every file in both patches this time, not just
   the ones this entry touched.
3. Real logs, once fetchable: Windows-side `curl` (this session's usual tool, via its
   git-bash/MSYS build) reliably failed to reach GitHub's log-blob storage with a bare
   connection error, on every retry, across multiple unrelated endpoints â€” but the same
   request through `curl.exe` (Windows' own native curl, invoked from PowerShell instead)
   worked on the first try. Worth remembering for next time this comes up: prefer `curl.exe`
   over git-bash's `curl` for this specific endpoint on this machine.
4. **Third push (with the patch-header fix) got past `download + patch Servo source` for
   the first time and produced a real, single `rustc` error** â€” `error[E0515]: cannot return
   value referencing temporary value` at `app.rs`'s `AppEvent::SaveFileDialog` handler:
   `window.platform_window().as_headed_window()` was being bound to a variable and returned
   out of a `find_map` closure as part of a tuple, but `as_headed_window()` returns
   `Option<&HeadedWindow>` borrowed from the `Rc<dyn PlatformWindow>` `platform_window()`
   returns â€” a temporary that drops at the end of that statement, so the reference couldn't
   outlive it. Every other call site of this exact chain in the codebase (e.g.
   `set_running_control_flow` a few lines below) only ever uses the result immediately in
   the same expression, never stores or returns it â€” this was the first call site that tried
   to carry it further. Fixed by cloning the (cheap) `Rc<ServoShellWindow>` itself out of
   `find` instead, then re-deriving `.platform_window().as_headed_window()` fresh and using
   it immediately, matching the pattern every other call site already follows.

5. **Fourth push (with the sleep bump) came back fully green**: `ensure-test-release`,
   `steam-emulator-smoke-test`, and all 6 `build-and-publish` legs (Windows msi/portable,
   Linux deb/portable, macOS portable/dmg) â€” confirming both real bugs above (the tracing.rs
   exhaustiveness gap and the app.rs temporary-lifetime error) are actually fixed, not just
   locally-plausible. Still not verified against a real device/build by a human, though (this
   session has no working local Windows toolchain â€” see this file's own recurring note on
   that) â€” pending a real re-test of the exact repro steps from the original report.

---

## 2026-09-15 â€” Mobile: save-export silently did nothing on Android; Fullscreen API risked hiding the game

**Files:** `support/android/apk/servoapp/src/main/java/org/servo/servoshell/MainActivity.kt`,
`support/ios/App.swift`.

**Patch:** none â€” neither file has a pristine-upstream counterpart to diff against (same
category as `test-page/`, see that entry's own reasoning: these are Roves-original native
container code, not modifications of any vendored Servo source).

**Reported by the same real-device test as the desktop entry above:** on Android (and,
untested but suspected by the same reporter, iOS), both the save-export and save-import buttons
did nothing at all â€” no dialog, no error, no log output.

**Root cause (Android export only â€” import was already wired correctly):**
`shouldOverrideUrlLoading` (added long before the save feature, `dc0b0925b`, "Use native
Android WebView and add initial iOS WKWebView container") returns `true` â€” "I'm handling this
myself" â€” for **any** non-http(s) scheme, `blob:` included. That cancels the navigation
attempt outright, which starves `setDownloadListener` (added by the same commit that landed
save import/export, `16a461ef6`) of the one signal it needs to ever fire at all: WebView only
invokes a `DownloadListener` when it *attempts* a real navigation and discovers it can't render
the result â€” an attempt `shouldOverrideUrlLoading` had already vetoed before that could happen.
Two features landed in different commits, individually reasonable, silently incompatible with
each other from day one.

**Fix:** adopted `App.swift`'s existing, working pattern instead (it never had this problem â€”
`WKWebView` has no navigation-based download signal to conflict with in the first place, so it
already intercepted the click directly): a new document-start injected script
(`DOWNLOAD_INTERCEPT_SCRIPT`, registered via `WebViewCompat.addDocumentStartJavaScript`, feature-
checked against `WebViewFeature.DOCUMENT_START_SCRIPT` the same way the existing service-worker
interception already feature-checks its own APIs) calls `event.preventDefault()` on any
`a[download]` click before the browser ever attempts to navigate, then hands the decoded bytes
straight to the existing `RovesFileBridge.saveDataUrl` `@JavascriptInterface` â€” which was always
correct, just never reached by the broken navigation-based path. `shouldOverrideUrlLoading`
itself also now excludes `blob:`/`data:` from the blocked-scheme list, so `setDownloadListener`
stays a working fallback for anything the click-interceptor doesn't catch, rather than a
permanently dead path.

**Fullscreen API, both platforms:** this app (and the iOS one) already always run edge-to-edge/
immersive (`enterImmersiveMode`/`prefersStatusBarHidden`) â€” there is no "windowed" mode for the
standard `document.documentElement.requestFullscreen()` to meaningfully toggle into or out of.
On Android specifically this isn't just a redundant no-op: `WebChromeClient.onShowCustomView`
(designed for `<video>` fullscreen) also fires for a whole-document fullscreen request, and it
**hides the entire WebView** (`webView.visibility = View.GONE`) in favor of a custom view never
designed to render a full document â€” a real risk of a game's UI visibly vanishing, not a
theoretical one. The same injected script neutralizes `Element.prototype.requestFullscreen`/
`Document.prototype.exitFullscreen` into a harmless resolved no-op on both platforms â€” on iOS
this is precautionary (`WKPreferences.elementFullscreenEnabled` is never turned on in
`App.swift`, so WebKit's own Fullscreen API support is already off by default there) but kept
for predictability and parity with Android, where it's load-bearing.

**Verification:** not yet verified on a real device (this repo's own CI can build both mobile
targets but has no real device/simulator interaction step for this feature â€” see the "Save
import/export" entry's own note making the same caveat for the original, buggy version of this
code). Pending a real-device re-test of both the save-export and fullscreen-safety fixes.

---

## 2026-09-16 â€” Cargo.lock: regenerate to match actual workspace dependencies

**Files:** `Cargo.lock`.

**Patch:** none needed â€” `Cargo.lock` is not part of the pristine-download-plus-`patches/`
reconstruction `test.yml`/`android.yml` do (no existing patch touches it, and those workflows
never copy this repo's own lockfile in; they resolve fresh against the pristine tag's lock plus
whatever the patched `Cargo.toml` requires). It only matters for tooling that builds directly
from this checkout â€” `release.yml` and any local `cargo`/`mach` invocation.

**Found while starting the dependency-review implementation** (`docs/DEPENDENCY_REVIEW.md`),
before touching anything else: `Cargo.lock` was already stale relative to
`ports/servoshell/Cargo.toml`'s real dependencies â€” missing `roves-content-packer`,
`steamworks`, `steamworks-sys` (all genuinely used, see `support/content-packer` and
`ports/servoshell/src/steam.rs`) and `sysinfo`'s `ntapi` dependency entirely, while still
carrying phantom entries for `malloc_size_of_tests`/`profile_tests`/`script_tests`/
`servo-capi-tests`/`style_tests` â€” workspace members from an earlier layout that no longer
exist. Unrelated to any dependency-review change; fixed first, on its own, so it doesn't get
bundled into or confused with the isolated per-candidate patches that follow.

**Fix:** `cargo metadata` (minimal/conservative resolution â€” adds what's missing and drops what
no longer resolves, without bumping any already-locked compatible version, unlike a bare
`cargo update` which would have moved dozens of unrelated crates to their latest semver-compatible
release). Verified the resulting diff touches only the entries named above, nothing else.

---

## 2026-09-16 â€” mozjs 0.21.0 â†’ 0.21.6 (patch-series bump, not the 0.26 major)

**Files:** `Cargo.toml`, `Cargo.lock`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (regenerated â€” this file already
carried the workspace-members and `sysinfo` hunks; a third hunk now also carries the `js`
pin change).

**Why:** first candidate from `docs/DEPENDENCY_REVIEW.md`'s inventory. Upstream pins `js = {
package = "mozjs", version = "=0.21", ... }` exactly (`=0.21` resolves only `0.21.0`, not the
whole `0.21.x` series â€” a plain `cargo update` without touching this pin can't reach 0.21.6),
which requires `mozjs_sys` `=140.14.0-0` (verified against crates.io's own dependency listing
for `mozjs` `0.21.6`, not assumed from the version number alone). Same mozjs/SpiderMonkey minor
generation as before â€” no rooting/GC/API surface change expected, unlike the separate 0.26
major-jump candidate the review document deliberately keeps out of this same patch.

**Change:** `Cargo.toml`'s `js` pin narrowed from `=0.21` to `=0.21.6`;
`cargo update -p mozjs --precise 0.21.6` then resolved `mozjs_sys` to `140.14.0-0` in
`Cargo.lock` (not the `140.14.0-0-lts` variant cargo also offered â€” the workspace pin doesn't
request an LTS SpiderMonkey build, and nothing else here does either).

**Verification:** `cargo update`'s dependency resolution succeeds and the regenerated patch
applies cleanly to a fresh pristine `v0.5.0` extraction (`patch -p1 --dry-run`). A full
`cargo build`/`mach build` is not possible on this machine (no working `lld-link`/`libclang`
locally, see `CLAUDE.md`) â€” real compile/link verification, and the actual JS/DOM/worker/Wasm
behavior this touches, is pending a `test.yml` CI run on this branch.

---

## 2026-09-16 â€” GStreamer runtime 1.22.x â†’ 1.28.7 â€” Windows install mechanism rebuilt, not just re-pinned

**Files:** `python/servo/platform/macos.py`, `python/servo/platform/windows.py`,
`.github/workflows/test.yml`, `.github/workflows/release.yml`.

**Patch:** `patches/servo-v0.5.0µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^/0006-media-gstreamer.patch` (regenerated â€” now also carries
the `macos.py`/`windows.py` hunks alongside its existing `gstreamer.py` one). The two workflow
files aren't part of the pristine-download-plus-`patches/` reconstruction (see `CLAUDE.md`),
so they're edited directly, no patch involved.

**Why this isn't a version-string bump:** `servo/servo-build-deps` â€” the repo Servo's own
`macos.py`/`windows.py` download prebuilt GStreamer from â€” was never updated past 1.22.3
(macOS)/1.22.8 (Windows); there is no 1.28.7 asset there (verified against its own release/asset
list via the GitHub API before writing any code). Two separate real discoveries followed:

1. **macOS**: GStreamer's own official distribution
   (`gstreamer.freedesktop.org/data/pkg/osx/1.28.7/`) still publishes the same
   `gstreamer-1.0-<version>-universal.pkg` / `-devel-` naming convention `servo-build-deps` used
   to mirror â€” `URL_BASE` now points there directly instead, parameterized by
   `GSTREAMER_PLUGIN_VERSION` so a future bump is a one-line change again. The `.pkg`/
   `sudo installer -target /` install mechanism itself is unchanged.
2. **Windows â€” a real architecture change upstream, not just a new number**: from 1.28.7 on,
   GStreamer no longer ships Windows as two separate MSIs (runtime + devel). The official
   distribution (`gstreamer.freedesktop.org/data/pkg/windows/1.28.7/msvc/`) is now a single
   Inno Setup installer (`gstreamer-1.0-msvc-x86_64-1.28.7.exe`, confirmed via its embedded
   "Inno Setup" signature) bundling both. This breaks the existing install mechanism outright:
   the current pinned version is installed via `msiexec /a ... TARGETDIR=... /qn` â€” MSI's own
   "administrative install" (extract-only, no real install, no elevation) â€” specifically chosen
   to dodge the UAC prompt `mach bootstrap`'s own `Start-Process -verb runAs` install path hangs
   on in non-interactive CI (see `test.yml`'s pre-existing comment on that finding). `msiexec /a`
   doesn't apply to a non-MSI `.exe` at all, so this needed a real replacement, not a tweak.

**Windows fix, found by testing locally (this machine's own Windows 11 session, not CI) before
touching any workflow:** downloaded the real 1.28.7 installer and confirmed
`/CURRENTUSER /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR="<path>"` installs **without any UAC
prompt at all** (no `consent.exe` elevation process ever appeared) and completes in well under a
minute â€” `/CURRENTUSER` is an Inno Setup switch that installs per-user rather than per-machine,
which doesn't require admin rights in the first place, unlike the old MSI path's `-verb runAs`.
The installed layout is flat (`bin/`, `lib/`, `include/`, no per-arch subfolder the old MSI's own
internal package structure produced) â€” pointing `/DIR` straight at
`<DEPENDENCIES_DIR>/gstreamer/1.0/msvc_X86_64` (the exact path `windows.py`'s `gstreamer_root()`
already looks for) reproduces the same effective location with zero changes needed to that
lookup logic. Confirmed both `bin/ffi-7.dll` and `lib/pkgconfig/gobject-2.0.pc` â€” the two files
`is_gstreamer_installed()` checks for â€” land exactly where expected. Test install and its
uninstaller (`unins000.exe`) were both run and cleaned up afterward; nothing was left registered
on this machine (no Start Menu entry, `Test-Path` on the install dir confirmed removed).

**Changed:** `windows.py`'s `_platform_bootstrap_gstreamer` now downloads the single installer
and runs it directly (no `msiexec`, no `-verb runAs`/PowerShell `Start-Process ... -verb runAs`
wrapper) â€” this also means a real end user running `./mach bootstrap` locally on Windows no
longer hits a UAC prompt for this step either, a side benefit of the fix, not just a CI
workaround. `test.yml`/`release.yml`'s own duplicate manual-install steps (there specifically
*because* `mach bootstrap`'s own path used to hang â€” see their own updated comments) were
updated to the same single-installer/`/CURRENTUSER` approach, still run explicitly with
`--skip-platform` rather than switching to trust `mach bootstrap`'s own now-probably-fine path
untested â€” keeping the existing "CI installs it explicitly, visibly, itself" pattern rather than
betting a first real usage on an unattended code path.

**Not changed:** the `gstreamer`/`glib`/etc. Rust binding crate versions (`gstreamer = { version
= "0.25", features = ["v1_18"] }` and siblings). GStreamer maintains runtime API/ABI stability
across the whole 1.x series above whatever minimum a binding's own feature flag (`v1_18` here)
declares â€” matching `docs/DEPENDENCY_REVIEW.md`'s own explicit note not to confuse the Rust
binding version with the native runtime version. No binding bump is needed for this runtime bump.

**Verification:** the `/CURRENTUSER` install behavior above was verified for real, on a real
Windows machine â€” not simulated or assumed. What's still unverified: the macOS `.pkg` install on
an actual macOS runner, and the Windows path end-to-end inside actual GitHub Actions (a real
CI environment differs from an interactive dev session in ways that could still matter â€” a
missing user profile registry hive, a different default session type). Pending a `test.yml` run
on this branch across all three platforms.

**First `test.yml` run (2026-09-16): macOS and Linux green, Windows failed â€” a different, real
bug, not the install mechanism.** Both macOS jobs (portable and dmg) and both Linux jobs
(portable and deb) passed outright, including their launch smoke tests â€” the `.pkg` bump needed
no further changes. Both Windows jobs failed, but not at the "install GStreamer" step (that step, and
`mach bootstrap`, both succeeded) â€” at `mach build`, *after* a clean Rust compile ("Finished
`dev` profile ... in 23m 50s"), during `build_commands.py`'s own post-build DLL-copying:
`ERROR: could not find required GStreamer DLL` for 12 entries (`avcodec-59.dll`,
`avfilter-8.dll`, `avformat-59.dll`, `avutil-57.dll`, `libcrypto-1_1-x64.dll`, `libjpeg-8.dll`,
`libogg-0.dll`, `libpng16-16.dll`, `libssl-1_1-x64.dll`, `libvorbis-0.dll`,
`libvorbisenc-2.dll`, `swresample-4.dll`). Root cause: `python/servo/gstreamer.py`'s
`GSTREAMER_WIN_DEPENDENCY_LIBS` hardcodes the *exact versioned filenames* of GStreamer's bundled
third-party dependencies (ffmpeg, OpenSSL, libjpeg, libogg, libpng, libvorbis) as they existed in
1.22.x â€” 1.28.7 bundles newer versions of every one of them, under different filenames. This is
a real content change orthogonal to the install-mechanism rewrite above, and the dependency
review's own text anticipated exactly this category of gap ("`python/servo/gstreamer.py` e le
liste plugin sono parte della verifica").

**Fix:** re-installed 1.28.7 locally (same `/CURRENTUSER` method, cleaned up again afterward) and
diffed its actual `bin/` contents against the old list to get the real current names, rather than
guessing: `avcodec-61.dll`, `avfilter-10.dll`, `avformat-61.dll`, `avutil-59.dll` (ffmpeg bumped
its own SONAMEs); `libcrypto-3-x64.dll`, `libssl-3-x64.dll` (OpenSSL 1.1 â†’ 3, same convention);
`jpeg8.dll`, `ogg-0.dll`, `png16.dll`, `vorbis-0.dll`, `vorbisenc-2.dll` (all four dropped the
`lib` filename prefix, not just a version bump); `swresample-5.dll`. All 12 renames match exactly
the 12 CI errors â€” no more, no less. The other 17 entries in the list (`bz2.dll`, `ffi-7.dll`,
the `glib`/`gobject`/`gio`/`gmodule` family, `graphene-1.0-0.dll`, `intl-8.dll`,
`libwinpthread-1.dll`, `nice-10.dll`, `opus-0.dll`, `orc-0.4-0.dll`, `pcre2-8-0.dll`, the
`theora`/`theoradec`/`theoraenc` trio, `z-1.dll`) are confirmed unchanged, still present under
the same names. `GSTREAMER_WIN_DEPENDENCY_LIBS_NEEDED_BY_SERVO_DIRECTLY` (this repo's own list,
added by this same patch) only references names from the unchanged set, so it needed no edit.
`GSTREAMER_BASE_LIBS`' own `-1.0-0.dll`-suffixed core libraries (`gstreamer-1.0-0.dll` and
siblings) were also spot-checked against the new `bin/` listing and are unaffected â€” GStreamer
keeps that suffix stable as its own ABI convention across the whole 1.x series, unlike the
bundled third-party libraries above.

## 2026-09-16 â€” mozangle 0.6.0 â†’ 0.7.0

**Files:** `Cargo.toml`, `Cargo.lock`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (regenerated â€” now carries a fourth
hunk for the `mozangle` pin, alongside the workspace-members, `js`, and `sysinfo` ones).

**Why:** next candidate from `docs/DEPENDENCY_REVIEW.md`'s inventory (WebGL/ANGLE, "soprattutto
Windows"). Checked the real upstream diff before touching anything, not just the version number:
`servo/mozangle`'s own `compare/v0.6.0...v0.7.0` is exactly 2 commits â€” both are its own
`update.py`-driven re-vendor of Mozilla's ANGLE fork against a newer pinned Firefox ESR tag
(`FIREFOX_140_12_0esr_RELEASE` â†’ `FIREFOX_153_1_0esr_RELEASE`). Every changed file is under
`gfx/angle/checkout/` (D3D11 renderer internals, GPU info detection, the shader translator) or
`mozangle`'s own build tooling (`update.py`, `generate_build_data.py`) â€” nothing under a
top-level `src/` touches the crate's own Rust binding surface, so this is a pure
implementation/bugfix update, not an API change. Low risk for exactly that reason.

**Change:** `mozangle = "0.6"` â†’ `"0.7"` (Cargo's default caret behavior treats a `0.x` minor
bump as a compatibility boundary, so this needed an explicit `Cargo.toml` edit, not just
`cargo update`). `cargo update -p mozangle --precise 0.7.0` resolved cleanly, touching only this
one crate in `Cargo.lock`.

**Verification:** patch applies cleanly to a fresh pristine extraction. Compile/link and actual
WebGL rendering behavior (the whole point of this candidate) pending a `test.yml` run â€” this is
exactly the kind of change the review document's own caution applies to: "il beneficio dipende
da... verificare il codice effettivamente compilato," not something a source diff alone proves.

---

## 2026-09-16 â€” zstd 0.13.3 â†’ 0.14.0 (content-packer only)

**Files:** `support/content-packer/Cargo.toml`, `Cargo.lock`.

**Patch:** `patches/servo-v0.5.0/0003-content-packer.patch` (regenerated â€” only its
`Cargo.toml` "new file" block changed; the crate's own source files are untouched).

**Why:** next candidate from `docs/DEPENDENCY_REVIEW.md`'s inventory. `zstd` here is only
`support/content-packer`'s own dependency (this repo's tar+zstd game-content packer, not
anything upstream Servo pulls in â€” `components/net`'s own `zstd` reference is just an
`async-compression` feature flag string, not this crate). Checked the real upstream changelog
(`gyscos/zstd-rs` release notes for `v0.14.0`) before touching anything: the one breaking change
is to `with_prepared_dictionary()`'s borrow semantics (content-packer uses plain
`zstd::Encoder::new`/`Decoder::new`, no dictionaries â€” not affected) plus a fixed
`Decoder::finish()` reader-position bug (content-packer's `extract.rs` never calls `.finish()`
on its decoder at all â€” reads it as a plain stream to EOF instead â€” so that fix doesn't change
its behavior either). `pack.rs`'s `encoder.finish()` call is the *encoder* side, a different,
unaffected method. Low risk for the same reason the review document itself gave: this crate's
usage is the minimal streaming read/write path, not the parts of the API that moved.

**Change:** `zstd = "0.13.3"` â†’ `"0.14.0"` in `support/content-packer/Cargo.toml`.
`cargo update -p zstd --precise 0.14.0` pulled `zstd-safe` 7.2.4 â†’ 8.0.0 (zstd 0.14's own
declared requirement, confirmed via crates.io's dependency listing rather than assumed) and
`zstd-sys` 2.0.16 â†’ 2.1.0 â€” the underlying native zstd C library version embedded stayed at
1.5.7 (`zstd-sys`'s own version string keeps that suffix), so this is a pure Rust-wrapper bump,
no change to the actual compression algorithm/format version. `async-compression`'s own
`compression-codecs`/`compression-core` sub-crates also moved to their latest compatible patch
versions as a side effect of the same resolution pass (0.4.38â†’0.4.42, 0.4.32â†’0.4.33) â€” expected
collateral from sharing the `zstd-safe` dependency, not a separate deliberate bump.

**Verification:** patch applies cleanly to a fresh pristine extraction. Pack/extract behavior,
performance, and old-pack/new-pack compatibility (the review document's own required checks for
this candidate â€” "misurare pack/estrazione, CPU, RAM e avvio freddo/caldo") are pending a
`test.yml` run; this machine can't run the crate's own `tests/roundtrip.rs` locally without a
working `cargo test` toolchain (see `CLAUDE.md`'s Windows build gap).

---

## 2026-09-16 â€” tikv-jemallocator/tikv-jemalloc-sys 0.6.1 â†’ 0.7.0/0.7.1

**Files:** `Cargo.toml`, `Cargo.lock`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (regenerated â€” fifth hunk, for the
two `tikv-jemalloc*` pins).

**Why:** next candidate from `docs/DEPENDENCY_REVIEW.md`'s inventory. Checked actual usage
first, since the review flagged this crate needs coordinated `sys`+wrapper checks: this fork
uses `tikv_jemalloc_sys::mallctl` directly (raw FFI, reading `stats.allocated`/`stats.active`/
`stats.mapped` after an explicit `epoch` refresh â€” see `components/allocator/lib.rs`) plus
`tikv_jemallocator::usable_size`, not the separate higher-level `jemalloc-ctl` crate. **jemalloc
is Windows-`cfg`-gated out entirely** (`#[cfg(not(any(windows, feature = "use-system-allocator",
target_env = "ohos")))]` â€” Windows uses `GetProcessHeap`/`HeapSize` instead, a separate code
path this bump doesn't touch at all), so the real risk surface is macOS/Linux only. Checked the
real 0.7.0 changelog: build/cross-compile fixes, a `jemalloc-ctl` update-implementation fix (not
the crate this fork uses), and native jemalloc bumped from 5.3.0(+1 commit) to 5.3.1 â€” a jemalloc
micro release, not a surface this fork's three `mallctl` MIB strings would be sensitive to.

**A real gotcha, not from the changelog:** `cargo update -p tikv-jemalloc-sys --precise 0.7.0`
initially resolved and warned `selected package tikv-jemalloc-sys@0.7.0 was yanked by the
author`. Confirmed via the crate's own version list (`yanked: true` for `0.7.0`, `false` for
`0.7.1`, both built from the identical jemalloc git ref â€” this was a republish, not a version
bump) before re-pinning to `0.7.1` instead. `tikv-jemallocator` itself (the allocator/wrapper
crate, a separate package from `-sys`) has no yanked `0.7.0` and stays at that version.

**Change:** `tikv-jemalloc-sys = "0.6.1"` â†’ `"0.7.1"`, `tikv-jemallocator = "0.6.1"` â†’ `"0.7.0"`
(different patch numbers between the two is correct here, not a typo â€” see the yank above).

**Verification:** patch applies cleanly to a fresh pristine extraction. `test.yml` ran: macOS
(portable + dmg), Linux (portable + deb), and Windows `msi` all green. Windows `portable` failed,
but only at the launch smoke test's save-file-autotest timing check â€” the process itself was
confirmed still running and healthy (`"still running after 10s: True"`), matching a flaky pattern
`test.yml`'s own comments already document on a loaded runner, not a real regression (jemalloc is
`cfg`-gated out of Windows builds entirely â€” see above â€” so it structurally can't be the cause).
Couldn't force an immediate clean re-run: `workflow_dispatch` 403s with this read-only PAT
("Resource not accessible by personal access token", as expected per `CLAUDE.md`), and an empty
commit doesn't retrigger `test.yml` since it only fires on pushes touching `patches/**`,
`test-page/**`, or the workflow file itself â€” none of which an empty commit touches. Will be
reconfirmed as a side effect of the next real `patches/**` push (`test.yml` always rebuilds and
tests the full current patch set, not just that push's own diff).

---

**What's still unverified:** whether 1.28.7 introduced any *new* transitive DLL dependency for
the specific plugin selection this fork copies (as opposed to a rename of an existing one) â€”
the code comment on this list says it's normally curated via `dumpbin` plus "the errors that
appear when starting Servo," neither of which was available here (no local Windows Rust
toolchain, see `CLAUDE.md`; no compiled `play.exe` to run `dumpbin` against). If one exists, the
same "`ERROR: could not find`" mechanism will surface it immediately on the next `test.yml` run,
the same way it caught this. Pending that next run to confirm the Windows jobs are fully green,
not just past this specific error.

**Second `test.yml` run: past the DLL-copy error, one new transitive dependency found.**
`mach build` and `mach bundle` both succeeded this time (the 12 renames above were the whole
copy-step fix) â€” the failure moved downstream, to the actual bundle-launch smoke test:
`GStreamer-WARNING: Failed to load plugin '...\release\lib\gstlibav.dll': The specified module
could not be found`, and `roves.log` confirms it as `ErrorLoadingPlugins(["gstlibav.dll"])`. This
is `_bundle_windows`'s `lib/` split working as designed (see its own docstring in
`post_build_commands.py`) â€” `gstlibav.dll` and every name in `GSTREAMER_WIN_DEPENDENCY_LIBS` both
land in `lib/`, and `main.rs`'s `SetDllDirectoryW` adds `lib/` to the process search path before
any plugin loads â€” so a generic "module not found" here means `gstlibav.dll` needs a DLL that
isn't in that list *at all*, not a renamed one. Compared the full 1.28.7 `bin/` listing (from the
same local reinstall used for the rename fix) against the dependency-review-era assumption that
ffmpeg's own shared libs were fully accounted for: `swscale-8.dll` is present in 1.28.7's `bin/`
and was never in this list, under any name, at any prior version â€” 1.22.x's `gstlibav` build
evidently didn't need libswscale as a separate runtime dependency (statically linked, or simply
unused by the codec paths active then); 1.28.7's does. Added `swscale-8.dll` to
`GSTREAMER_WIN_DEPENDENCY_LIBS`. No other `Failed to load plugin` line appeared in the log, but a
single generic loader error doesn't guarantee there's only one missing dependency â€” the same
detection mechanism will catch a further one immediately if it exists. Pending a third
`test.yml` run to confirm the Windows jobs are fully green end to end, including this launch
smoke test.

---

## 2026-09-16 â€” SDL3 gamepad: GilRs â†’ SDL3, first slice of the SDL3 migration

**Files:** `Cargo.toml`, `ports/servoshell/Cargo.toml`, `ports/servoshell/desktop/gamepad.rs`
(rewritten â€” previously pristine, upstream still uses GilRs itself), `ports/servoshell/desktop/
app.rs`, `ports/servoshell/desktop/event_loop.rs`, `ports/servoshell/running_app_state.rs`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (the `gilrs`â†’`sdl3` workspace pin
swap) and `patches/servo-v0.5.0/0001-desktop-shell-core.patch` (regenerated â€” the four existing
sections plus a brand-new `gamepad.rs` one, `gamepad.rs` having had zero prior customization).

**Why SDL3, and why gamepad first:** `docs/DEPENDENCY_REVIEW.md`'s confirmed decision â€” SDL3
migrates progressively, "iniziando da `gilrs`/gamepad." Of the 13 files using `winit::` directly,
gamepad support was the one piece with no coupling to window creation, IME, or GL surface
sharing, making it the lowest-risk first slice to actually land before tackling the much larger
windowing/event-loop replacement (still open â€” see below).

**A hard architectural constraint found by reading the `sdl3`-rs source before writing any code,
not by a failed CI run:** `sdl3::init()` unconditionally refuses to run on any thread other than
the one that first calls it (checked via a thread-local + a process-wide `AtomicBool`), unless
the crate's own `test-mode` feature is enabled â€” which the crate's error message and feature
documentation both describe as a testing-only escape hatch, not a supported production
configuration. This isn't a Rust-binding-only pedantry check: it reflects a real constraint SDL
itself has on some platforms (Cocoa's own main-thread requirements on macOS). This directly
broke the natural direct port of the old design: GilRs ran on its own dedicated background
thread (`gamepad.rs`'s `ServoshellGamepadDelegate::new` used to `thread::Builder::new().spawn`),
which is exactly the pattern `sdl3::init()` rejects unless running from `main()`'s own thread.

**Decision: integrate into winit's main-thread loop, not `test-mode`.** Using `test-mode` in
shipped code would be exactly the kind of shortcut its own name warns against â€” real risk of
instability on macOS specifically, for a "just make the error go away" reason. Instead:

- `ServoshellGamepadDelegate` no longer spawns a thread. It owns the SDL state directly
  (`Sdl`, `GamepadSubsystem`, `EventPump`, open gamepads, pending haptic effects) behind a
  `RefCell` (needed since the delegate is shared via `Rc`), created by `ServoshellGamepadDelegate::
  new()` â€” called from `App::finish_init`, which only ever runs on the same thread `main()`
  itself runs on, satisfying SDL's real requirement.
- A new `poll(&self, state: &RunningAppState)` method drains SDL's event queue
  (`EventPump::poll_event`, non-blocking) and any due haptic effects/requests, translating and
  dispatching directly â€” inline, synchronously, no more `AppEvent::Gamepad` message hop at all
  (removed that variant from `event_loop.rs`'s `AppEvent` enum, and the now-dead
  `RunningAppState::handle_gamepad_events` indirection it went through).
- `App::new_events`' `Running` arm calls `gamepad_delegate.poll(state)` on every
  `StartCause::ResumeTimeReached`, and `set_running_control_flow` (already shared by `window_event`/
  `user_event`'s tails, and now also called at the end of `new_events`' own `Running` arm) folds a
  `GAMEPAD_POLL_INTERVAL` (100ms â€” the same cadence GilRs' own `next_event_blocking(Some(...))`
  polled at) into its `ControlFlow::WaitUntil` deadline computation, alongside the pre-existing
  boot-splash-animation deadline. This reuses the exact mechanism the splash screen's
  indeterminate animation already relies on to keep winit's event loop ticking instead of fully
  idling (`ControlFlow::Wait`) â€” no new timer/wake mechanism invented, same pattern, folded in.
- The haptic-effect request path (`GamepadDelegate::handle_haptic_effect_request`, called from
  wherever `Gamepad.vibrationActuator.playEffect()` reaches native code â€” not necessarily the
  main thread) still needs a channel, since that call site isn't guaranteed to be on the main
  thread the way event polling now is â€” `sender`/`receiver` (`std::sync::mpsc`) kept for that,
  just drained by `poll()` now instead of a background loop.

**API mapping notes (GilRs â†’ SDL3), each checked against real API signatures, not assumed:**

- GilRs represents the analog triggers as *buttons* with an analog value
  (`ButtonChanged(LeftTrigger2/RightTrigger2, value)`); SDL3 represents them as *axes*
  (`Axis::TriggerLeft`/`TriggerRight` via `GamepadAxisMotion`, range `0..=32767`). Both still map
  to Standard Gamepad button indices 6/7 per the W3C spec â€” `handle_gamepad_events`'s
  `GamepadAxisMotion` arm special-cases these two axes into `GamepadUpdateType::Button`, not
  `Axis`, to preserve that mapping.
- GilRs' Y axes are inverted relative to the Gamepad spec (the old code's own comment says so,
  and negated them for that reason). SDL3's Y axes already match the spec's convention (down is
  positive) directly â€” confirmed against `sdl3-rs`' own axis documentation, not assumed by
  symmetry with X. The negation was *not* carried over; carrying it over unchanged would have
  silently inverted every analog stick's up/down on first real use.
- `Gamepad::set_rumble`/`set_rumble_triggers` are fire-and-forget calls (magnitude + duration),
  unlike GilRs' `EffectBuilder`/`Effect` with an explicit `play()`/`stop()` lifecycle and a
  `ForceFeedbackEffectCompleted` completion event. SDL has no completion event at all â€”
  `request.succeeded()` is now reported once a delayed rumble actually starts, not once it
  finishes; `start_delay` (which SDL's API has no parameter for) is emulated by holding the
  request until `poll()`'s ~100ms cadence notices `fire_at` has passed.
- `supports_trigger_rumble` stays hardcoded `false` on connect, matching the GilRs-era default,
  even though SDL can genuinely drive trigger rumble (`set_rumble_triggers`) â€” real per-device
  capability detection wasn't wired through the `Connected` event payload in this pass; left as a
  known follow-up, not a functional regression (the dual-rumble path GilRs already supported
  works the same as before).

**Native SDL3 build: `build-from-source`, not a system install.** `sdl3-sys` defaults to
`use-pkg-config`/`use-vcpkg` â€” i.e. expecting a pre-installed system SDL3, the same shape of
problem GStreamer's Windows installer saga (above) turned into two extra rounds of CI failures.
Enabled `build-from-source` instead (`sdl3 = { version = "0.20", default-features = false,
features = ["build-from-source"] }`): SDL3 compiles from vendored source (`sdl3-src`, pinned to
`3.4.16`) as part of the normal Rust build, via `cmake` + the C/C++ toolchain this project's other
native dependencies already require â€” no new native-installer/bootstrap surface on any platform,
consistent with the "embedded, versioned by Roves" principle `docs/DEPENDENCY_REVIEW.md`'s own
architecture section states for the whole engine.

**Not done in this pass:** `winit`/`surfman`/window creation, IME, `keyutils.rs`'s key mapping,
`webxr.rs`, and `gui.rs`'s `egui-winit`/`accesskit_winit` accessibility bridge are all still
winit-based â€” see `docs/DEPENDENCY_REVIEW.md`'s own note that dropping `egui-winit` needs a new,
hand-written AccessKit integration, since no maintained `egui`-on-`SDL3` backend exists upstream.
This entry covers gamepad only.

**Verification:** every SDL3 API signature and enum shape cited above (`Sdl::gamepad`/
`event_pump`, `GamepadSubsystem::open`, `Gamepad::set_rumble`/`id`/`name`, the `Event::Gamepad*`
variants and their fields, `Axis`/`Button` variant names, the main-thread check itself) was
checked against the real `sdl3`/`sdl3-sys` crate source (`vhspace/sdl3-rs` on GitHub), not
guessed from memory or by analogy with `sdl2`. The main-thread constraint specifically was caught
this way, *before* it could burn a ~25-35 minute CI round trip discovering it via a runtime panic
instead.

**First `test.yml` run: one real compile miss, found immediately.** `desktop/tracing.rs`'s own
`winit`-event logging `target!` macro had a separate match arm on `AppEvent::Gamepad(..)` that
the `app.rs`/`event_loop.rs` cleanup missed (`error[E0599]: no variant... named Gamepad`) â€”
one-line fix. Encouragingly, everything upstream of that compiled cleanly on the first attempt:
`sdl3-src`/`sdl3-sys`/`sdl3` (`build-from-source`) all built successfully in under 90 seconds on
Linux.

**Second run (after the `tracing.rs` fix): Linux and Windows fully green, macOS failed â€”
not a compile error this time, `mach build`'s own Rust compile finished cleanly in ~15
minutes.** The failure was in `python/servo/gstreamer.py`'s post-build dylib-packaging step
(`package_gstreamer_dylibs` â†’ `make_rpath_path_absolute`): `ERROR: could not package required
dylibs: Unable to satisfy rpath dependency: @rpath/libSDL3.0.dylib`. This function walks every
`@rpath/...` entry `otool -L` reports for the compiled binary and its dependencies and expects
each one to resolve inside GStreamer's own `lib/` directory â€” reasonable when GStreamer's dylibs
were the only `@rpath`-relative dependency in the tree, but `build-from-source`'s default
(dynamic) linking mode gives SDL3 its own `@rpath`-relative `libSDL3.0.dylib` too, which this
GStreamer-specific walker has no way to know isn't one of its own. The existing
`is_separately_packaged_dylib` exclusion (added earlier for `libsteam_api.dylib`, see that entry
above) could have been extended the same way, but the more robust fix is removing the dylib
entirely: switched the `sdl3` feature from `build-from-source` to `build-from-source-static`
(confirmed via `sdl3-sys`'s own `Cargo.toml`: `build-from-source-static = ["build-from-source",
"link-static"]`) â€” SDL3 (zlib-licensed, no static-linking restriction) is now linked directly
into the binary, so there's no separate dylib for *any* platform's packaging step to trip over,
not just macOS's. `cargo metadata` after the switch only dropped `sdl3-net-src`/`sdl3-net-sys`
from the lockfile's reachable-optional-deps set (the `net` feature was never requested either
way) â€” no other change.

**Not yet re-verified:** this exact fix (`build-from-source-static`) hasn't had its own `test.yml`
run yet â€” pending. Linux/Windows were already green under dynamic linking, so the expectation is
they stay green under static linking too, but that's an expectation, not a confirmed result.

**Third run (after the static-link fix): Linux and Windows fully green again, macOS hung â€”
not a build error at all this time, `mach build`/`mach bundle` both succeeded.** The failure
was the launch smoke test itself: the bundled binary never exited, never printed anything (no
`still running`/`stdout`/`roves.log` lines ever appeared, checked by fetching the in-progress
job's own log directly, which the public Checks API allows even before a job finishes), and
`kill "$PID"` didn't unstick it â€” the step sat for 100+ minutes before being abandoned (GitHub's
default job timeout is several hours; this was cut short manually instead of waiting it out, on
a hard time budget for the day). **Root cause not confirmed â€” no interactive Mac was available
to verify directly.** Leading hypothesis: `sdl3::init().gamepad()` triggers `IOHIDManager` device
enumeration, which macOS's Input Monitoring TCC privacy permission can gate; an unattended CI
process has no session to show or answer a permission prompt, which would explain an
unkillable-by-SIGTERM hang (documented elsewhere as a real class of macOS µ¨¥zºè¯
â¶)à²Ö§uªİ¢ëiºĞk¢G§¦*^issue for TCC-gated
processes). This is *not confirmed* to be CI-only â€” a real interactive user might see a
resolvable system prompt on first launch instead of a silent hang, which would still be a real
first-launch UX problem, not just a test artifact. Weakening evidence against the hypothesis:
research indicates SDL3's non-HIDAPI ("classic") macOS joystick backend *also* goes through
`IOHIDManager`, so an `SDL_HINT_JOYSTICK_HIDAPI=0` hint (the first fix considered) likely
wouldn't have helped either way â€” not attempted, to avoid spending the day's last CI round on a
low-confidence guess.

**Decision: disable gamepad on macOS specifically, as an explicit, temporary carve-out â€” not a
permanent product decision.** Extended every `not(any(target_os = "android", target_env =
"ohos"))` gate already used for `feature = "gamepad"` (in `running_app_state.rs`'s
`ServoshellGamepadDelegate` import/field/constructor/accessor, and `window.rs`'s
`gamepad_delegate` call site â€” `window.rs` had zero prior customizations, now patched for the
first time) to also exclude `target_os = "macos"`; `app.rs`'s own (module-tree-already-desktop-
only, so a bare `not(target_os = "macos")` there is equivalent) call sites updated the same way.
`desktop/gamepad.rs` and its `sdl3` dependency are untouched and still compile on macOS â€” they're
simply never invoked there. Windows and Linux gamepad support via SDL3 is unaffected.

**Follow-up needed, tracked in TODO.md:** get real access to a Mac (interactive, not CI) to
observe what actually happens â€” does a permission prompt appear at all, does accepting it
resolve the hang, is this specific to a fresh/first-run TCC state (would a pre-granted
permission avoid it entirely, meaning CI could pre-authorize itself somehow), or is the true
cause something else entirely unrelated to TCC. Until then, macOS ships without gamepad support
compared to upstream Servo's own GilRs-based build, a real (if narrow) functionality gap.

---

## 2026-09-17 â€” SDL3 gamepad on macOS: real fix, not just the carve-out above

**Files:** `ports/servoshell/desktop/gamepad.rs`, `ports/servoshell/desktop/app.rs`,
`ports/servoshell/running_app_state.rs`, `ports/servoshell/window.rs`.

**Patch:** `patches/servo-v0.5.0/0001-desktop-shell-core.patch` (all four files already lived in
this patch from the carve-out above; regenerated).

**Follow-up research (no interactive Mac available, still verified only via CI) turned up a
concrete, narrower fix than the blanket macOS carve-out above.** SDL3 actually ships *two*
independent gamepad backends on macOS, each behind its own hint:
`SDL_HINT_JOYSTICK_IOKIT` (default on) opens devices directly via `IOHIDManager`, while
`SDL_HINT_JOYSTICK_MFI` (also default on, untouched here) goes through Apple's public
GameController framework instead. Only the IOKit path touches `IOHIDManager`, which is gated by
the "Input Monitoring" TCC permission â€” the prime suspect for the indefinite hang on a headless
CI runner with no session for TCC to prompt against (see the previous entry's hypothesis). MFI
never touches that permission at all.

**Fix:** `gamepad.rs`'s `init_sdl()` now calls `sdl3::hint::set("SDL_JOYSTICK_IOKIT", "0")`
before `sdl3::init()`, gated `#[cfg(target_os = "macos")]` (a no-op, harmless everywhere else).
This disables only the TCC-gated backend; MFI alone still covers every modern
Xbox/PlayStation/Switch Pro or other MFi-compliant controller. Reverted every
`not(target_os = "macos")`/`not(any(..., target_os = "macos"))` cfg gate the previous entry
added back to their pre-carve-out form (`app.rs`, `running_app_state.rs`, `window.rs`) â€” macOS
gamepad support is compiled in and enabled again, same as Windows/Linux.

**Trade-off, stated explicitly:** any gamepad that is *not* MFi-compliant (old/exotic USB HID
controllers with no GameController-framework driver) loses macOS support under this fix, where
it would have worked (modulo the hang) under the old IOKit-only GilRs-based upstream build. This
is judged an acceptable, narrow trade-off against "no gamepad support on macOS at all," which is
what the previous entry's carve-out shipped.

**Verification:** pushed and watched via `test.yml`'s macOS job â€” this is the entire reason this
fix could be attempted with no interactive Mac on hand at all: if the IOKit backend really is
what was hanging, disabling it should let `sdl3::init().gamepad()` return promptly instead of
hanging for 100+ minutes, and the rest of the smoke test should proceed normally. If this run
still hangs, the IOKit hypothesis is wrong and this fix doesn't help â€” see the run this entry's
own commit triggered for the actual result before trusting this description.

---

## 2026-09-16 â€” Tracy: new `tracing-tracy` feature (Perfetto already existed upstream)

**Files:** `Cargo.toml`, `ports/servoshell/Cargo.toml`, `ports/servoshell/lib.rs`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (new hunk for the root
`tracing-tracy` pin), `patches/servo-v0.5.0/0001-desktop-shell-core.patch` (`Cargo.toml`
section regenerated again, on top of the SDL3 change above), `patches/servo-v0.5.0/
0015-shared-content-protocols.patch` (`lib.rs` section regenerated).

**Before writing any code: checked what `docs/DEPENDENCY_REVIEW.md`'s "Tracy e Perfetto" plan
item actually still needs, since assuming both are greenfield would have wasted real effort.**
Perfetto turned out to be **pristine upstream Servo functionality that already exists in full**,
not something to build: `tracing`/`tracing-perfetto`/`tracing-hitrace` Cargo features, a
`PerfettoLayer` wired into `init_tracing` (`ports/servoshell/lib.rs`, writing `servo.pftrace`,
openable at ui.perfetto.dev), and â€” found by tracing `Opts::time_profiling`/
`time_profiler_trace_path` from `components/config/opts.rs` forward â€” a **separate**,
also-already-wired Servo time-profiler (`components/profile/time.rs`: CSV output, terminal
output, and an HTML/JS/CSS `TraceDump` viewer under `components/profile/trace-dump*`) already
reachable through this fork's own `prefs.rs` argument parsing (`cmd_args.profile`/
`cmd_args.profiler_trace_path` â†’ `Opts`). None of this was ever a Roves-specific gap; only Tracy
itself was genuinely absent (confirmed: no "tracy"/"Tracy" match anywhere under `ports/` or
`components/`).

**Change:** added `tracing-tracy` (`nagisa/rust_tracy_client`, 0.12.0) as a new optional
feature, `tracing-tracy = ["tracing", "dep:tracing-tracy"]`, mirroring the existing
`tracing-perfetto` feature's exact shape â€” same gating (`tracing` must also be on), same "off
unless explicitly requested" default (not in servoshell's `default = [...]` feature list).
`init_tracing` gains a matching `#[cfg(feature = "tracing-tracy")]` block that adds
`tracing_tracy::TracyLayer::default()` to the subscriber, right alongside the existing
`PerfettoLayer` block â€” both can be enabled together, since they're independent `tracing-
subscriber` layers on the same registry.

**Native build: no new installer risk.** `tracing-tracy` â†’ `tracy-client` â†’ `tracy-client-sys`
only needs the `cc` crate as a build-dependency (verified via that crate's own `Cargo.toml`) â€”
Tracy's C++ client compiles from vendored source using the C/C++ toolchain this project's other
native dependencies already require, no system Tracy install, no `pkg-config`/`vcpkg`, unlike
`sdl3-sys`'s default (see the SDL3 entry above) or GStreamer's own installer story. Left
`tracy-client`'s own default feature set as-is (`system-tracing`, `context-switch-tracing`,
`sampling`, etc.) rather than restricting it pre-emptively â€” this feature is never requested by
any current CI job (`test.yml`/`release.yml` don't pass `--features tracing-tracy`, matching
`tracing-perfetto`'s own current status â€” neither has ever actually been exercised by CI), so
there's no real build-time risk today either way, and no evidence yet to justify overriding
upstream's own considered defaults.

**Deliberately not built in this pass:** the dependency review's own separate, lower-confidence
idea of a lightweight always-shippable Roves diagnostics ring buffer (low-cost, off by default,
explicitly activatable to diagnose a real user's problem in a release build) â€” Servo's existing
CSV/trace-dump profiler and the two `tracing` layers above are session-long, CLI-activated
tools, not a rolling last-N-frames buffer meant to ship quietly in every build. That's a
genuinely new, separate feature (needs its own frame-time hook, buffer design, and export
format/activation mechanism), not something "Tracy e Perfetto" alone implies, and was scoped
out to keep pace with the much larger SDL3 windowing work still ahead. Worth a dedicated pass
later if real user-diagnostic needs come up.

**Verification:** `cargo metadata` resolves cleanly (`tracing-tracy`, `tracy-client`,
`tracy-client-sys`, plus two small transitive deps â€” nothing unrelated moved). Both regenerated
patches apply cleanly to a fresh pristine extraction. Actual compilation is, like the SDL3
change above, pending a `test.yml` run â€” and even a green run only proves this *compiles*
(nothing in CI ever builds with `--features tracing-tracy`/`tracing-perfetto` today, so this
entry doesn't claim the Tracy/Perfetto integration itself was exercised, only that adding the
feature doesn't break the default build).

---

## 2026-09-16 â€” egui/egui-winit/egui_glow 0.34.3 â†’ 0.36.2, egui-file-dialog 0.13.0 â†’ 0.15.0

**Files:** `Cargo.toml`, `ports/servoshell/desktop/gui.rs`.

**Patch:** `patches/servo-v0.5.0/0014-root-workspace.patch` (version bumps), `patches/servo-v0.5.0/
0001-desktop-shell-core.patch` (`gui.rs` section regenerated).

**Why now, not sooner:** originally the plan deferred egui until after the SDL3 windowing/
event-loop replacement, to avoid migrating `egui-winit` integration code twice. SDL3 windowing
wasn't attempted this session (see that entry's own scope write-up above) â€” the window stays on
`winit` for now, so that reason to wait no longer applies for this pass; proceeding with egui now
is a real, independently valuable step rather than one that would need redoing.

**`egui-file-dialog`: version bump only, no code change needed.** Checked its own real
changelog first: 0.14.0's only breaking change was adding a field to `FileDialogConfig` (Roves
never constructs that struct directly, so unaffected); 0.15.0's only breaking change is the
`egui` 0.36 bump itself. `0.15.0` is the first version whose own `egui` dependency requirement
(`^0.36.0`, confirmed via crates.io's dependency listing) actually matches this bump â€” the
review document's own caution ("non presumere che il dialogo accetti 0.36") was correct to flag,
and turned out to require a version jump (`0.13`â†’`0.15`), not a compatibility gap.

**The one real egui-side breaking change, found by reading `egui`'s own source (`containers/
panel.rs`, `egui_glow/src/winit.rs`) before touching any code:** `EguiGlow::run`'s callback
signature changed from `impl FnMut(&egui::Context)` to `impl FnMut(&mut egui::Ui)` â€” egui 0.36
removed the `Context`-taking `Panel::show`/`Panel::show_inside` variants for `CentralPanel`/
`SidePanel`/`TopBottomPanel` entirely (`show_inside` was renamed to `show`; the old top-level
`show(ctx, ...)` is just gone, not merely renamed), in favor of always handing the whole-window
`Ui` to the caller directly. This is exactly what a pre-existing comment in `gui.rs` (next to a
now-removed `#[expect(deprecated)]`) had already predicted: "deprecated in this egui version in
favor of hand-building a full-window `Ui`."

**Floating/positioned containers (`Window`, `Area`, `Tooltip`) did *not* change** â€” verified
each one's real signature individually rather than assuming a blanket rule: `Window::show`,
`Area::show`, and `Tooltip::always_open` all still take `&Context`/`Context` directly, since
(unlike docked panels) they're conceptually independent of any parent `Ui`. This is why
`desktop/dialog.rs`'s own `Dialog::update(&mut self, ctx: &egui::Context)` needed **zero**
internal changes â€” only its call site in `gui.rs` (`dialog.update(ctx)` â†’ `dialog.update(ui.ctx())`).

**Fix, applied to all three of `gui.rs`'s `self.context.run(...)` call sites (`update`,
`update_splash`, `update_content_load_error`):** renamed the closure parameter from `ctx` to
`ui` (it's genuinely a `Ui` now), removed the now-unfulfillable `#[expect(deprecated)]`
attributes (the deprecated method they were suppressing a warning for no longer exists at all â€”
leaving the attribute would itself become a hard "unfulfilled lint expectation" error), and
changed `.show(ctx, ...)` to `.show(ui, ...)`. Every other call on the (renamed) parameter was
checked individually against `Ui`'s and `Context`'s real method lists rather than assumed:
`pixels_per_point`/`available_rect_before_wrap` exist directly on `Ui`, unchanged; `fonts_mut`/
`accesskit_node_builder`/`layer_painter`, and `Tooltip::always_open`'s owned-`Context` argument,
are `Context`-only and now go through `ui.ctx()`.

**Confirmed out of scope:** egui 0.36's other headline breaking change ("Remove `Modifiers` from
`RawInput`, make it an `egui::Event`") doesn't touch this fork's own code â€” `EguiGlow::run`
builds `RawInput` internally via `egui_winit::State::take_egui_input`, and `gui.rs`/
`headed_window.rs` have zero direct `RawInput`/`.modifiers` references (checked). The three other
`#[expect(deprecated)]`/`#[allow(deprecated)]` sites in this fork (`desktop/keyutils.rs` Ã—2,
`desktop/headed_window.rs` Ã—1) are about `winit`'s own `Key`/`create_window` deprecations, not
egui â€” confirmed by reading each one, left untouched.

**Verification:** `cargo metadata` resolves cleanly â€” the larger-than-usual `Cargo.lock` diff
(182 insertions/164 deletions) is `egui`'s own font-shaping dependency stack (`skrifa` 0.40â†’0.44,
`glifo` 0.1.1â†’0.2.0, `harfrust` newly added, `fearless_simd`/`read-fonts` version churn) moving as
a side effect of egui 0.35's own switch to `harfrust` for kerning/ligatures (see its real
changelog) â€” not unrelated collateral. Both regenerated patches apply cleanly to a fresh pristine
extraction. Actual compile/render correctness (the splash screen, the browser-chrome overlay
egui draws over each WebView, the save-file dialog, AccessKit) is pending a `test.yml` run â€” this
machine has no working `cargo build` locally (see `CLAUDE.md`).

---

## 2026-09-17 â€” SDL3 windowing/event-loop: real implementation started (`sdl3-windowing` branch)

**Files:** `Cargo.toml`, `ports/servoshell/desktop/{event_loop,app,gui,headed_window,tracing,
webxr,headless_window}.rs`, `ports/servoshell/window.rs`, `ports/servoshell/desktop/protocols/
roves.rs`.

**Not on `main` â€” lives on a dedicated `sdl3-windowing` branch, deliberately not merged.**
TODO.md's "Finestra + event loop" section had, until today, only scoping (13 files using
`winit::`, no code attempted â€” see the entries above this one). The user explicitly asked for
real implementation to begin, accepting non-green intermediate commits until the whole slice is
ready to verify together â€” this entry documents the first real slice: the entire event-loop and
control-flow architecture, plus window creation and the egui/GL rendering bridge, ported from
winit to SDL3. **Real, known gap: no user input works yet** (keyboard, mouse, touch, IME,
gestures) â€” a window opens, shows the animated boot splash, resizes, and closes cleanly, but
cannot otherwise be interacted with. This is explicitly the next slice, not a hidden regression.

**`desktop/event_loop.rs` â€” full rewrite.** `ServoShellEventLoop`'s `Winit(EventLoop<AppEvent>)`
variant became `Sdl3 { sdl, video, event_subsystem, proxy }`. Introduced this module's own
`ActiveEventLoop` (holds the `VideoSubsystem` plus `Cell<ControlFlow>`/`Cell<bool>` exit state),
`ControlFlow` (`Wait`/`WaitUntil(Instant)`), `WindowId` (a plain `u32` type alias â€” SDL3 windows
carry a raw `u32` id, no opaque newtype needed), and `EventLoopProxy` (wraps `Arc<sdl3::event::
EventSender>` â€” `EventSender` itself has no public constructor and isn't `Send`, only `Sync`, so
one is created once on the main thread and shared via `Arc` rather than re-obtained per clone the
way winit's own, actually-`Clone`, proxy was). `AppEvent` gained a new `RedrawRequested(WindowId)`
variant: SDL3 has no push-based redraw-request event the way winit does, so `HeadedWindow::
request_redraw` sends this instead, routed straight back into the same window-event handling a
real `WindowEvent::RedrawRequested` would have gotten. `run_sdl3_app` (replacing winit's own
`EventLoop::run_app`) is a plain, explicit loop: `wait_event_timeout`/`wait_event` per
`ControlFlow`, `is_user_event()`/`as_user_event_type::<AppEvent>()` for custom events, and a new
`translate_sdl_event` function mapping the handful of `sdl3::event::Event::Window` sub-events
ported so far (`Resized`, `CloseRequested`, `Exposed`â†’`RedrawRequested`, `FocusGained`/`Lost`)
into this module's own, much smaller `WindowEvent` enum â€” everything else returns `None` and is
skipped, not translated yet (see the file's own TODO comment for the full ~16-variant list still
needed, mouse/keyboard/IME/touch/gestures/theme/scale-factor).

**`desktop/app.rs` â€” `ApplicationHandler<AppEvent>` trait impl converted to three plain inherent
methods** (`dispatch_new_events`/`dispatch_window_event`/`dispatch_user_event`, called directly
by `run_sdl3_app` instead of through winit's own trait dispatch) â€” the actual state-machine logic
(`AppState::Booting`/`Running` handling, splash-tick driving, gamepad polling) is otherwise
unchanged, just renamed and re-typed against `event_loop.rs`'s new shim types instead of winit's.

**`desktop/headed_window.rs` â€” window creation ported for real.** `winit_window: winit::window::
Window` â†’ `sdl_window: sdl3::video::Window`, built via `VideoSubsystem::window(title, w,
h).opengl().resizable().high_pixel_density().hidden()` (+ `.fullscreen()` when
`start_fullscreen`). Confirmed via the vendored `sdl3` crate's own source (not assumed) that
`sdl3::video::Window` implements both `raw_window_handle::HasWindowHandle` and `HasDisplayHandle`
with real per-platform implementations (Windows/macOS/iOS/Android, confirmed by reading
`raw_window_handle.rs` directly) â€” this is what let `WindowRenderingContext::new(display_handle,
window_handle, size)` (surfman's own GL-context creation) carry over completely unchanged: it
already only ever needed a `raw-window-handle` pair, never a concrete winit type. Added the
`raw-window-handle` feature to the workspace `sdl3` dependency in `Cargo.toml` for this. All of
`sdl3::video::Window`'s mutating methods (`set_title`, `set_size`, `set_fullscreen`, `maximize`,
`raise`, ...) take `&mut self` even though they just forward to a plain FFI call on the window's
own `Arc`-backed shared native handle â€” `Window` derives `Clone` for exactly this reason (a cheap
`Arc` bump, not a real duplicate window), so every such call in this file goes through
`self.sdl_window.clone().the_call(...)`. `HeadedWindow::handle_winit_window_event` (the ~250-line
dispatch for all ~16 real winit `WindowEvent` variants) is now `handle_window_event`, handling
only `Resized`/`CloseRequested`/`RedrawRequested`/`Focused` â€” the old keyboard/mouse/touch/
gesture/IME/dropped-file handling methods are left in place, unreachable, rather than deleted (a
`#[expect(dead_code)]` is cheaper to undo than re-deriving carefully-written logic from scratch).
Several details simplified or dropped for this pass, each flagged with its own TODO at the call
site: window/taskbar icon loading, Linux taskbar app-id naming, transparent
(`no_native_titlebar`) windows, IME cursor positioning, cursor shape changes, and the inner-vs-
outer-size distinction (SDL3's `Window::size()` doesn't distinguish the client area from OS
decorations the way winit's `inner_size()`/`outer_size()` did â€” treated as equal, zero
`decoration_size`, harmless since `no_native_titlebar` isn't ported either).

**`desktop/gui.rs` â€” `egui_glow::EguiGlow` replaced with a new, local `SdlEguiGlow`.** Confirmed
by reading `egui_glow`'s own source that `EguiGlow` (in its `winit.rs` module) is a convenience
wrapper hard-coded to `&winit::window::Window` at the type level (`run`/`paint`/`new` all take it
directly) â€” but the two things it actually wraps, `egui_glow::Painter` (the real GL renderer,
`painter.rs`) and `egui::Context` itself, are both genuinely toolkit-agnostic. `SdlEguiGlow` holds
those two directly and does the `egui_winit::State`-equivalent input/output bridging itself â€”
except, for this pass, there isn't one: `run` builds an `egui::RawInput` with only a screen rect
(from `sdl_window.size()`/`display_scale()`) and nothing else, so the boot splash renders
correctly (confirmed by this being the very first thing painted, at construction time) but no
SDL3 input (keyboard, mouse, ...) is translated into egui at all yet â€” a real, known gap, not an
oversight. AccessKit integration (`egui_winit::State::init_accesskit`, the `accesskit_winit::
Adapter` reachable via `self.context.egui_winit.accesskit`) is gone for the same reason: it lived
entirely inside the now-removed `egui_winit::State`. `Gui::handle_accesskit_event` and the
tree-update-forwarding tail of `Gui::update` (which reached into that same adapter) are
deleted/stubbed rather than reworked, since nothing produces those events anymore regardless â€”
see TODO.md's own AccessKit de-risking notes for what a real SDL3 bridge needs to replicate.

**`window.rs`, `webxr.rs`, `headless_window.rs`, `protocols/roves.rs` â€” mechanical follow-through.**
`PlatformWindow::new_glwindow`'s `&winit::event_loop::ActiveEventLoop` parameter, and every other
site naming that type, now name `crate::desktop::event_loop::ActiveEventLoop` instead â€” no
behavior change, `webxr.rs`'s own `XRWindow`/`XRWindowPose` secondary-window creation was ported
to `sdl3::video::Window` the same way the primary window was. `protocols/roves.rs`'s
`close_proxy: Option<Arc<Mutex<EventLoopProxy<AppEvent>>>>` dropped the now-meaningless generic
parameter (`EventLoopProxy` isn't generic anymore) â€” the `Arc<Mutex<..>>` wrapping itself is now
redundant (the new `EventLoopProxy` is already cheaply `Clone`) but left as-is to minimize this
pass's blast radius.

**Verification:** both regenerated patches (`0001-desktop-shell-core.patch`,
`0002-desktop-protocols.patch`) apply cleanly to a fresh, independently-extracted pristine v0.5.0
download â€” but this is the *first* time any of this code has been checked by anything resembling
a compiler: this machine has no working local `cargo build`/`check` (see `CLAUDE.md`), so every
type, trait bound, and method signature above was verified by hand against the vendored `sdl3`/
`egui_glow` crates' own source, not by an actual build. A real `test.yml` run against this branch
is the first genuine compiler feedback this change will get â€” expect real errors on the first
attempt; this entry describes intent and design, not a confirmed-working result.

---

## 2026-09-18 â€” SDL3 windowing: first genuinely green compile (Linux + Windows)

**Files:** `patches/servo-v0.5.0/0014-root-workspace.patch`, `ports/servoshell/desktop/
gamepad.rs`, `ports/servoshell/desktop/app.rs`, `ports/servoshell/running_app_state.rs`,
`ports/servoshell/window.rs`.

**Two real CI rounds after the entry above, `desktop/app.rs`/`event_loop.rs`/`headed_window.rs`/
`gui.rs` (as described there) compile cleanly on Linux and Windows** â€” confirmed by a real
`test.yml` run, not by hand-verification. Round 1 (10 compiler errors, all fixed, see git
history) and round 2 (down to just `no method named window_handle/display_handle found for
struct sdl3::video::Window`, persisting despite `raw-window-handle` genuinely being a correctly
resolved feature in this repo's own `Cargo.lock`/`cargo metadata` output) led to the real root
cause: **`patches/servo-v0.5.0/0014-root-workspace.patch` â€” the patch that actually carries the
root `Cargo.toml` into `test.yml`'s pristine-download-plus-patches reconstruction, a *separate*
file from `0001-desktop-shell-core.patch` â€” had never been regenerated after the
`raw-window-handle` feature was added to `Cargo.toml` earlier.** CI's reconstructed manifest kept
requesting `sdl3` with only `build-from-source-static`, so the feature genuinely never activated
in the environment that mattered, no matter how correct the committed `Cargo.lock` was. A
worthwhile lesson for next time: this repo's local `Cargo.toml`/`Cargo.lock` state is not what CI
builds against at all â€” only `patches/servo-v0.5.0/*.patch` is, and every root-level dependency
change needs its *own* patch regenerated (`0014` for `Cargo.toml`, not `0001`, which only covers
`ports/servoshell/*`).

**Also re-disabled gamepad on macOS on this branch** (mirroring `main`'s own 2026-09-17/18 revert
â€” see that entry for the full story: the narrower IOKit-only fix was confirmed via a real 6-hour
CI hang *not* to work, and was reverted on `main`). This branch had inherited the IOKit-only
version from before that revert happened on `main`; left as-is, it would have wasted another full
6-hour CI timeout on macOS for a hang already known and unrelated to the SDL3 windowing work
itself. Re-apply once (if) the gamepad-on-macOS problem gets a real fix.

**Not yet known: whether macOS compiles too** â€” the CI round that found the `0014` fix already
had a macOS job running against the *previous* (IOKit-fix-still-present) commit, which will very
likely also hit the same hang this entry's second fix just avoided for future runs; that
specific run's macOS result is therefore not meaningful and wasn't waited on. The next full run
(with both fixes in place) is the one to check for a real macOS compile signal.
## 2026-09-18 â€” SDL3 input compile blocker: exhaustive event dispatch

**Files:** `ports/servoshell/desktop/headed_window.rs`,
`patches/servo-v0.5.0/0001-desktop-shell-core.patch`.

The first CI run after wiring SDL3 keyboard and mouse events stopped in `servoshell` with one
cross-platform `E0004`: `handle_window_event` still ended in a wildcard arm, but the compiler's
exhaustiveness analysis does not count that arm when the enum is deliberately configured to make
new variants visible. Replaced it with explicit no-op arms for `Resized`, `RedrawRequested`, and
`Focused(false)`; resize/redraw are already handled earlier in the function and focus loss has no
Servo-side action yet. This preserves the intentional guarantee that every future translated SDL3
event must be considered at the dispatch site, while unblocking the same build error observed on
Linux, macOS, and Windows in Actions run 35394065037.
