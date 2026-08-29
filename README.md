# Roves

**Roves** (an anagram of *Servo*) is a customized fork of the [Servo](https://servo.org/)
web engine, repurposed as a runtime for shipping **web-based games** as real native
applications — instead of as a browser.

## What this is

[Servo](https://servo.org/) is a general-purpose web browser engine written in
[Rust](https://github.com/rust-lang/rust). Roves takes that engine and strips out
everything that exists to make it *behave like a browser* — the toolbar, the tab strip,
the address bar, back/forward history, favicon handling, and the state that only existed
to feed all of that — none of which a game needs (see [CUSTOMIZATIONS.md] for the full,
itemized list of what was removed and why).

What's left is a lean embeddable window that loads a local web bundle (HTML/CSS/JS —
Phaser, PixiJS, Three.js, or your own engine, anything that renders to a page) full-window,
looking and behaving like a native application, not a browser tab.

Roves is a **vendored, patched copy** of a pinned Servo release, not a live Git fork or
submodule — see [CLAUDE.md](./CLAUDE.md) for why, and [CUSTOMIZATIONS.md] for exactly what
changed on top of pristine upstream Servo and how those changes are carried forward when
upstream is upgraded.

[CUSTOMIZATIONS.md]: ./CUSTOMIZATIONS.md

## Naming: "Roves" vs. "Servo" vs. `servoshell`

Every label a player or the OS actually sees — the window title, the taskbar/dock app
identity, the Linux `.desktop` menu entry — says **Roves**, not Servo (see
[CUSTOMIZATIONS.md] for the exact rename). The one exception: the portable bundle's own
binary/bundle name (`play.exe`/`play.app`/`play`) is a neutral placeholder, deliberately the
same generic name on every platform rather than "Roves"-branded — see "Portable vs.
installable packages" below. What's still named after the upstream project:

- The underlying engine itself is genuinely still Servo — Roves doesn't fork Servo's
  rendering/DOM/script internals, just the shell around it (see "What this is" above) —
  so places that credit *the engine* (not the product) intentionally still say "Servo".
- The binary and Cargo package are, and will keep being, named `servoshell` (e.g.
  `target/release/servoshell`, `ports/servoshell/`). Renaming it to something like
  `rovesshell` was considered and deliberately decided against: it would touch the upstream
  Python build tooling under `python/servo/` too (not just branding, and not verifiable
  without a real build), for no functional benefit. This is a settled decision, not a
  pending one — don't expect or propose a `rovesshell` rename later.

## Goal

Ship web-based games as real, native, double-click-to-run desktop (and, eventually,
console) binaries — without bundling a full general-purpose browser engine like Electron
or CEF, and without paying Chromium's footprint and overhead for a game that only ever
needs a canvas and a window. Servo is already lighter and written in Rust; this fork trims
it down further to exactly what a game needs and nothing else.

`./mach bundle` (see its own `--help`) turns a build into that double-click-ready package
per target platform — see [CUSTOMIZATIONS.md] for what it produces on each.

## Content packing & compression

By default, `./mach bundle --content-dir dist/` does **not** copy your game's built web
content into the release as plain, individually browsable files — the loose `.html`/`.js`/
image/audio files a bundler like Vite produces are exactly what someone poking around inside
an unzipped release would otherwise find and lift straight out. Instead, `--content-dir` is
packed into a handful of `tar`+`zstd` archives, split into two tiers:

- A small **boot set** — the html file itself, plus whatever it directly references
  (`<script src>`, `<link href>`, `modulepreload` hints, etc.), plus anything matched by
  `--content-boot-include`. This gets extracted eagerly, in full, before the engine even
  starts — into your OS's own cache directory (`~/.cache`, `Library/Caches`, `%LOCALAPPDATA%`
  — real disk, not anywhere inside the shipped game folder, and not a RAM-backed location
  either, which matters once a project's assets reach the multi-GB range).
- **Everything else** stays compressed and is decompressed on demand, per archive, the first
  time the running page actually requests a file from it — so a large game's first launch
  only ever pays for what that session actually touches, not the whole install. Once
  decompressed, a pack stays that way (including across relaunches) until the packed content
  actually changes.

This is about not handing out your source assets for free by default, **not** DRM or
anti-tampering — the archives aren't encrypted, and anyone willing to run the extractor
themselves (it ships right there in the bundle) gets the original files back byte-for-byte.

How the split works, so a large game doesn't turn into either one giant archive or hundreds
of tiny ones: past the boot set, `dist/`'s own root files become one archive; each direct
subfolder's own files become another; everything nested deeper than that (however many
levels) is flattened into a third archive per top-level subfolder. Every archive is capped at
500 MB by default, splitting into further parts past that. Files with an already-compressed
extension (images, audio, video, fonts, existing archives) skip zstd entirely instead of
spending CPU for no size win.

How small the boot set stays depends on your own bundler's code-splitting: a bundler that
statically imports (or `modulepreload`-hints) most of the app from the entry HTML ends up with
most of the app in the boot set too — same as a plain browser would eagerly fetch it. Structure
anything not needed immediately (a later level, optional content) behind a dynamic `import()`
to keep it lazy.

Tune or disable all of this with flags on `./mach bundle`:

| Flag | Description | Default |
| --- | --- | --- |
| `--content-compress <auto\|none>` | Pack `--content-dir` into tar+zstd archives (`auto`), or copy it in as loose, uncompressed files with none of the above (`none`). | `auto` |
| `--content-compression-level <N>` | zstd compression level used by `--content-compress=auto`. Low favors speed. | `1` |
| `--content-max-pack-size <SIZE>` | Max size per content archive (e.g. `500M`, `1G`) before splitting into further parts. | `500M` |
| `--content-exclude <GLOB>` | Leave files matching this glob (relative to `--content-dir`) loose/uncompressed instead of packing them — e.g. a save-data or user-config subfolder that shouldn't sit inside a read-only archive. Repeatable. | unset |
| `--content-boot-include <GLOB>` | Force files matching this glob (relative to `--content-dir`) into the eager boot set, beyond the html file and whatever it directly references — e.g. a splash image. Repeatable. | unset |

See the "Pack game content into compressed archives" and "Split packed content into an eager
boot set + lazy, on-demand extraction" entries in [CUSTOMIZATIONS.md] for the full design
(archive naming, the manifest format, the extraction cache, the `file:` handler, and why
`tar`+`zstd`) and for what's deliberately out of scope today (per-file integrity hashes, real
encryption). A native, Roves-branded boot splash *is* shown throughout both extraction and
your page's own initial load — see the "Native boot splash" and later boot-splash entries in
[CUSTOMIZATIONS.md] for how it works.

## Supported platforms

| Device | Status | Compatibility library |
| --- | --- | --- |
| **Windows** (x64) | ✅ Implemented | native (Rust std) |
| **macOS** (Apple Silicon and Intel) | ✅ Implemented | native (Rust std) |
| **Linux** (x64) | ✅ Implemented | native (Rust std) |
| Nintendo Switch | 🚧 In development | `nx` |
| Nintendo 3DS | 🚧 In development | `ctru-rs` |
| PlayStation Portable (PSP) | 🚧 In development | `rust-psp` |
| PlayStation Vita | 🚧 In development | `vitasdk-rs` |
| PlayStation 4 | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| PlayStation 5 | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| Xbox One | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| Xbox Series X\|S | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |

Desktop targets (Windows, macOS, Linux) are supported today. All console targets are on the
roadmap but not yet functional.

## Embedding

Your web content has no *build-time* way to know it's running inside Roves rather than a
regular browser (or, for that matter, Tauri). A build-time signal is still an option if you
want one (the parent pixi-vn-react-template project this fork ships with sets an
`EMBEDDED_TARGET=roves` environment variable when building for Roves — see its
`.github/workflows/embedded.yml` and `src/lib/hooks/quit-hooks.ts` — a convention of that
project, not something Roves itself provides). For a genuine *runtime* check instead, Roves
injects `window.__ROVES__ = true` into every page as soon as `<head>` exists, before the
page's own scripts run (see the "Inject a `window.__ROVES__` marker" entry in
[CUSTOMIZATIONS.md]) — use `@drincs/roves-api/core`'s `isAvailable()` (see below) to read it
rather than checking `window.__ROVES__` directly: `false` in a plain browser, `true` only
when actually running under Roves, no build step required, no async wait needed.

### Talking to native APIs

Web content can't call into Rust directly — there's no Tauri-style `invoke()` runtime built
in. Instead, Roves lets native code register custom URL schemes (`ProtocolHandler`s — see
`ports/servoshell/desktop/protocols/`) that respond to ordinary `fetch()` calls from page
JS. One is shipped today:

- **`roves:`** (`protocols/roves.rs`) — a small, generic "control this app" surface: window/
  process lifecycle (`exit`/`close_window`) and `is_available` (see below).

[**`@drincs/roves-api`**](../roves-api) is the JS package wrapping it, deliberately shaped
to feel familiar if you already know `@tauri-apps/api` (though it's a real, independent
implementation, not a shim over Tauri's runtime):

- `@drincs/roves-api/core` — the generic `invoke(cmd, args)`, talking to `roves:`, plus
  `isAvailable()` — a genuine, synchronous runtime "is this page running inside Roves" check.
- `@drincs/roves-api/process` — `exit()`, built on `core`.
- `@drincs/roves-api/saves` — save-game storage; see "Save data" below.

See the "Roves' own general-purpose `invoke()` bridge" entry in [CUSTOMIZATIONS.md] for how
the Rust side is wired up.

### Content root & client-side routing

Your bundled content is served from a fixed virtual origin, `game://content/`, not the raw
`file://<absolute path>/index.html` you might expect — and the app boots by requesting the
*root* (`game://content/`), not `game://content/index.html` directly. Both exist for the same
reason: under a real `file://` path, `location.pathname` is the actual OS path and never
matches `/`, so any client-side history router (React Router, TanStack Router, Vue Router in
"history" mode, ...) falls back to its own "not found" page immediately, at boot and on every
`pushState` navigation. `game://content/` sidesteps this the same way Tauri's own
`tauri://localhost/` does — a real, root-relative origin your router's own routes match
against, so History API navigation "just works". A direct navigation/reload on a sub-route
(no matching file on disk) falls back to serving your bundle's own entry HTML, the same way a
static host's SPA fallback (nginx's `try_files`, Vite's `historyApiFallback`) does. See the
"Virtual content root (`game:` protocol)" entry in [CUSTOMIZATIONS.md] for the full design.

A custom, non-`http(s)` scheme like `game:` isn't always transparent to every JS library —
some hardcode `http`/`https` in their own URL handling. PixiJS is one: it needs both
`resolver.rootPath` (set *before* `Assets.init()`) and `basePath` set to
`` `${location.protocol}//${location.host}/` ``, or a root-absolute asset reference
(`/foo.png`) fails to load. See the
[wiki](https://github.com/DRincs-Productions/roves-wiki) for the exact snippet, why `basePath`
alone isn't enough, and other framework-specific gotchas.

### Steam

Steam support is opt-in at build time: passing `--features steam` (e.g.
`./mach build --features steam`) compiles in a second, dedicated custom protocol,
`steam:` (`protocols/steam.rs`), rather than routing it through `roves:` — Steamworks is a
large, self-contained SDK surface, not a generic app command. When the feature isn't
enabled — the default — the `steam:` scheme doesn't exist in the binary at all.

It wraps the [`steamworks`](https://crates.io/crates/steamworks) crate, which binds the
official Steamworks SDK (achievements, stats, DLC checks, the overlay, the store page). If
the feature is enabled but the app wasn't actually launched through Steam, every command
degrades to a harmless default instead of failing (`is_available` reports `false`, writes
are silent no-ops).

From JS, use [**`@drincs/roves-api/steam`**](../roves-api) — a full Steamworks wrapper
(achievements, stats, DLC, overlay, store) talking to `steam:` directly. See the "`steam:`
protocol bridge" entry in [CUSTOMIZATIONS.md] for how the Rust side is wired up.

You don't need to build this yourself just to get Steam support: every platform's published
release (see "Getting started" below) also ships a prebuilt `_steam`-suffixed shell variant,
which [Roves Packmaster](https://github.com/DRincs-Productions/roves-packmaster) can download and
bundle your game into directly, App ID and all — no Rust/Python toolchain needed.

### Save data

[**`@drincs/roves-api/saves`**](../roves-api) is an async, origin-scoped key/value store for
player save data (shaped like IndexedDB, backed by real files), talking to its own dedicated
`saves:` protocol (`protocols/saves.rs`). Roves picks the on-disk location for you — a
`saves/` folder next to the game when it's running portably, the OS cache directory when
installed via `--msi`/`--dmg`/`--deb` (distinguished at runtime by a marker file the installer
build writes, since nothing else tells those two cases apart) — and, when built with
`--features steam` and a Steam client is running, transparently mirrors every write/delete to
Steam Cloud — and `getMostRecent()` can tell which save is newest, Cloud-only saves included,
without downloading anything. See the "Save-game storage API" entry in [CUSTOMIZATIONS.md] for
the full design, and the [wiki](https://github.com/DRincs-Productions/roves-wiki) for player/game-dev-facing
docs.

## Getting started

Prebuilt, versioned engine shell builds (no game content bundled — see below) are published
on the [Releases page](https://github.com/DRincs-Productions/roves/releases) as a portable
zip for Windows, macOS, and Linux, starting with `v0.1.0`. Each platform is published twice
— the plain default build (`roves_shell_<platform>.zip`) and a Steam-enabled one
(`roves_shell_<platform>_steam.zip`, see the "Steam" section above) — pick whichever your
game needs. Grab one of those if you just want to try the shell; the rest of this section is
for building from source instead (e.g. to bundle your own game's content, or to work on
Roves itself).

These are the same build steps as upstream Servo — Roves is a source-level fork, not a
different build system. For deeper background, see the Servo Book's own [Getting the Code]
and [Building Servo] pages; where this fork's behavior actually differs from what's
described there, that's covered in [CUSTOMIZATIONS.md] instead.

[Getting the Code]: https://book.servo.org/building/getting-the-code.html
[Building Servo]: https://book.servo.org/building/building.html

### macOS

- Download and install [Xcode](https://developer.apple.com/xcode/) and [`brew`](https://brew.sh/).
- Install `uv`: `curl -LsSf https://astral.sh/uv/install.sh | sh`
- Install `rustup`: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Restart your shell to make sure `cargo` is available
- Install the other dependencies: `./mach bootstrap`
- Build: `./mach build`
- Package a runnable bundle: `./mach bundle` (add `--dmg` to wrap it in an installable disk
  image instead of the default self-contained `play.app` — see "Portable vs. installable
  packages" below)

### Linux

- Install `curl`:
  - Arch: `sudo pacman -S --needed curl`
  - Debian, Ubuntu: `sudo apt install curl`
  - Fedora: `sudo dnf install curl`
  - Gentoo: `sudo emerge net-misc/curl`
- Install `uv`: `curl -LsSf https://astral.sh/uv/install.sh | sh`
- Install `rustup`: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Restart your shell to make sure `cargo` is available
- Install the other dependencies: `./mach bootstrap`
- Build: `./mach build`
- Package a runnable bundle: `./mach bundle` (add `--deb` for an installable Debian/Ubuntu
  package instead of the default self-contained `play` binary — see "Portable vs. installable
  packages" below)

### Windows

- Download [`uv`](https://docs.astral.sh/uv/getting-started/installation/#standalone-installer), [`choco`](https://chocolatey.org/install#individual), and [`rustup`](https://win.rustup.rs/)
  - Be sure to select *Quick install via the Visual Studio Community installer*
- In the Visual Studio Installer, ensure the following components are installed:
  - **Windows 10/11 SDK (anything >= 10.0.19041.0)** (`Microsoft.VisualStudio.Component.Windows{10, 11}SDK.{>=19041}`)
  - **MSVC v143 - VS 2022 C++ x64/x86 build tools (Latest)** (`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`)
  - **C++ ATL for latest v143 build tools (x86 & x64)** (`Microsoft.VisualStudio.Component.VC.ATL`)
- Restart your shell to make sure `cargo` is available
- Install the other dependencies: `.\mach bootstrap`
- Build: `.\mach build`
- Package a runnable bundle: `.\mach bundle` (add `--msi` for an installable Windows package
  instead of the default self-contained `play.exe` — see "Portable vs. installable packages"
  below; requires the WiX Toolset's `candle`/`light` on `PATH`)

## Portable vs. installable packages

By default, on every platform, `./mach bundle` produces a **portable** bundle — something a
player downloads, unzips, and runs directly, no install step, no admin/root privileges. Each
platform also has an **installable** package alternative, matching the shape of what a
bundler like [Tauri]'s own bundler offers (`msi`/`nsis` on Windows, `dmg`/`app` on macOS,
`deb`/`rpm`/`appimage` on Linux) — Roves supports one format per platform today, the ones
with existing, reusable packaging logic in this fork's Servo lineage (`./mach package`'s own
WiX/`hdiutil` code, see [CUSTOMIZATIONS.md]):

| Platform | Portable (default) | Installable |
| --- | --- | --- |
| Windows | `play.exe` + a few DLLs, GStreamer plugins in `lib/` | `--msi`: a real `.msi`, via WiX |
| macOS | `play.app` | `--dmg`: that same `.app`, wrapped in a `.dmg` disk image |
| Linux | `play` + `.so` deps, flat | `--deb`: a real, installable `.deb` |

An installable package always *wraps* the exact same content the portable bundle would have
had (same `--content-dir`, same packing settings) — it isn't a second, different build.
`--package-name`/`--package-version` (defaults `roves`/`0.0.0`) name and version whichever
one you asked for; nothing else about `./mach bundle`'s other flags changes based on
portable-vs-installable. `nsis`/`rpm`/`appimage` aren't implemented yet — see
[CUSTOMIZATIONS.md] if you're adding one.

This repo's own smoke-test CI, [`.github/workflows/test.yml`], exercises both variants on
every platform on every push (a `package_mode` matrix axis of `portable`/`msi`/`dmg`/`deb`,
one job per platform per mode) — not a manual toggle, since its whole job is proving `mach
bundle` still works in every mode it supports, not just the default. It doesn't stop at a
successful build either: after assembling each bundle, it actually launches the resulting
binary for a few seconds and fails the job if the process doesn't stay up — a plain build
success was once mistaken for "the bundle works," which cost real debugging time (see
[CUSTOMIZATIONS.md]'s launch.json entries) before this check existed. If you're building your
own release pipeline around `mach bundle` (or `roves-action`, see below), pick whichever
mode(s) you actually want to ship, the same way that workflow's per-mode `BUNDLE_ARGS` do.

### Diagnosing a launch that appears to do nothing

`mach bundle --diagnostic-script` ships a `diagnose.bat` (Windows) / `diagnose.sh`
(macOS/Linux) next to the game binary — in the installable packages too, not just the
portable one (not `--deb`, where a terminal already shows the same output directly). Running
it instead of the game launches the same binary from a console that stays open afterward,
printing the exit code and the newest `roves.log`'s contents inline, so a tester who hits "I
double-clicked it and nothing happened" can just run this and paste the result into a bug
report. Off by default — a real release has no reason to carry debug tooling players never
asked for. See the wiki's [Diagnosing a silent launch] page for the full story (`roves.log`'s
location per platform, and a real incident this was built to catch).

[Diagnosing a silent launch]: https://github.com/DRincs-Productions/roves-wiki/blob/main/content/docs/distribution/diagnosing-a-silent-launch.mdx

[`.github/workflows/test.yml`]: .github/workflows/test.yml

[Tauri]: https://v2.tauri.app/distribute/

## Relationship to upstream Servo

Roves tracks a pinned upstream Servo release plus a small set of targeted patches — see
[CUSTOMIZATIONS.md] for the itemized list, and [CLAUDE.md](./CLAUDE.md) for how upgrades to
a newer Servo release are meant to happen. All credit for the underlying engine goes to the
[Servo Project](https://github.com/servo/servo) and its contributors; issues specific to
this fork's own customizations should stay local to this repository rather than upstream.
