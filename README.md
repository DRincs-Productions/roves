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
submodule — see [AGENTS.md](./AGENTS.md) for why, and [CUSTOMIZATIONS.md] for exactly what
changed on top of pristine upstream Servo and how those changes are carried forward when
upstream is upgraded.

**JavaScript engine: V8.** Since v0.7.0 Roves runs JavaScript on
[V8](https://v8.dev/), the engine used by Chrome, instead of Servo's SpiderMonkey. Game code
needs no changes: libraries such as PixiJS, Three.js and Phaser depend on standard JavaScript
and Web APIs, not on the engine. Roves's engine layer is `components/roves-js`, which
implements the JavaScript API Servo's DOM code expects on top of V8.

V8 also has a **JIT-less** mode, the configuration for platforms that forbid generating
executable code at runtime, such as consoles. The Servo `js_disable_jit` preference selects
this mode at engine startup (for example `--pref js_disable_jit=true`); V8 flags cannot change
after initialization. The V8 smoke checks this preference in a separate fresh process; Linux
normal/JIT-less browser CI confirms it (the browser smoke runs on Linux only). In that mode there is no WebAssembly, so
WebAssembly games (Unity, Godot or Emscripten exports) need the regular build. Developer-tools
JavaScript debugging is not available on V8 yet. SpiderMonkey-only Baseline/Ion, asm.js and
per-tier Wasm tuning preferences were removed because V8 ignored them; use `js_disable_jit`
for the supported startup JIT switch. The migration plan and its status are in
`docs/V8_MIGRATION.md`; the [Roves wiki](https://github.com/DRincs-Productions/roves-wiki)
explains what the engine means for your game. How Roves performs against Chrome with PixiJS,
three.js, motion, GSAP, Tone.js and Web Workers is on the wiki's
[Performance page](https://github.com/DRincs-Productions/roves-wiki/blob/main/content/docs/performance.mdx).
The V8 smoke CI treats failed assertions, incomplete reports, unexpected skips and missing
Window/CSP/navigation results as failures. JIT-less Wasm skips and explicitly configured
missing audio/GPU devices are the only accepted exceptions. Completed suites stop their browser
without waiting for the full deadline, then undergo the same strict result validation. When WebGPU has no usable adapter,
`requestAdapter()` resolves with `null`, including for local `file://` pages; a request that
hangs still fails the smoke check.
Desktop bundle smoke checks wait for their required page results (including synthetic input and
save-file checks) with a bounded timeout, so slow cold starts do not truncate later assertions.
On Linux the Xvfb probe waits up to 60 seconds for the shell's own visible window, never picks
another client's window on a shared display, and must report its window/input/resize cycle
before the smoke stops the shell.
The browser smoke checks ordinary JavaScript prototype mutation and the actual Window separately.
Window now reuses V8's template-created canonical prototype during Servo binding initialization,
so V8 can reject prototype replacement while leaving ordinary properties writable. Linux normal/JIT-less CI now confirms
that immutable Window path. Remote WindowProxy representatives now use V8 Proxy
so Servo's cross-origin traps actually run; their prototype getter returns null and replacement
is rejected. Foreign native Window values now get cached wrappers at realm conversion boundaries so
their prototype handlers run. Browser confirmation of that wrapping path remains pending;
other origin-aware WindowProxy traps and navigation identities remain under audit. The smoke reports trusted
eval and Function results, checks the separate Wasm CSP directive and isolates Window diagnostics.
A separate opt-in GC smoke process starts with `--pref dom_testutils_enabled=true` (default
false) to expose Servo's existing [`TestUtils.gc()`](https://testutils.spec.whatwg.org/); it checks
that retained DOM nodes, expandos and closures survive repeated collections, that unreachable
DOM nodes, cycles and a dropped script-compiled importer are actually collected, and that
`import()` still works after collection. Ordinary pages must see `TestUtils` undefined. It does
not measure native finalizers, realm/module/navigation retention or performance; browser
results are pending CI. The same page also loads one module URL in two same-origin frames and
checks that each copy's dynamic `import()` stays owned by its own frame: every module source now
keeps its own engine identity, even with identical code and URL, and natives called from module
code see that module's private. This can reduce V8's compilation-cache reuse for identical
module sources; it has not been measured. `import()` inside direct `eval` code is also checked:
it resolves against the module or script that contains the `eval`, as in browsers. Code created
with `new Function` (and its generator/async variants) resolves against its calling module or
script too; like V8's own compilation cache, an identical body built again reuses the first
caller's origin.
A module's owner page is now a reference the collector traces, not a permanent root; realm and
module tables still keep pages and modules alive, so this saves no memory yet (not measured).
Its main Window fixture and cross-origin iframe use
temporary loopback HTTP servers on different ports, so Servo exercises an HTTP origin and the
iframe has a distinct origin. Failed smoke runs retain bounded browser logs and report iframe
append, load and contentWindow access separately.
Trusted Types enforcement for `eval(TrustedScript)` is connected to Servo's CSP checks, including
the source-body value used by the check; the normal-string and ordinary-object paths retain their
separate JavaScript behavior. The smoke also checks the WPT-required case where every dynamic Function argument is a
`TrustedScript`. Per-realm V8 Proxy adapters now preserve those arguments for Servo's CSP
checker, extracting trusted data without invoking altered `toString` methods and retaining
V8's native constructor/parser. Mixed strings and counterfeit objects remain blocked. This
path passed Linux normal/JIT-less browser CI; migration status and remaining limitations are tracked in
[`docs/V8_MIGRATION.md`](./docs/V8_MIGRATION.md). Chrome 155 still rejects the all-trusted case.
GeneratorFunction, AsyncFunction and AsyncGeneratorFunction inherit from the exposed `Function`,
as in the language specification, so the unwrapped constructor is not reachable through them.
The experimental WebIDL V8 pilot is retained as a generator-only evaluation tool and is not
compiled into the production bindings crate.
WindowProxy prototype reads and writes now use Servo’s same-origin checks; a strict browser smoke
checks same-origin and cross-origin access, confirmed in Linux normal/JIT-less browser CI.
Property traps now reuse Servo's origin checks and WebIDL cross-origin annotations; a V8
WeakMap caches accessors without rooting dead receivers. Linux normal/JIT-less browser CI confirms these property checks.
The CSP smoke also covers all four dynamic constructors with trusted/plain argument combinations.
Runtime regressions check that constructor authorization cannot escape into a newTarget getter.
The patch set includes its cross-origin HTTP and Wasm CSP fixture pages for pristine-source reconstruction.
The V8 caller adapter now handles an empty script stack safely and allows Servo to use its
callback settings stack when no JavaScript caller is active. Callback activation overrides
remain under migration audit. Object realm lookup uses V8 creation contexts; unused optional
context storage was removed from per-object bookkeeping. During proxy construction, GC
traces owned values before invoking native hooks only once the object cell is initialized.
WindowProxy GC hooks also tolerate an uninitialized native-owner slot during construction.
Script/module private references now share the existing ownership hooks and are released
before runtime teardown; collection of unused metadata during navigation remains under audit.
The native reference callbacks use Rust's standard Rc count APIs directly.
Classic-script metadata now follows V8 source/function reachability; pending imports retain
it until resolution. Linux normal/JIT-less browser smoke passes on commit 8442f4998b9; realm/module retention
and browser GC stress remain open.
Shared agent instructions live in [AGENTS.md](./AGENTS.md). The
[agent workflow audit](./docs/AGENT_WORKFLOW.md) records the development process.
Escaped `import.meta.resolve` retains its module metadata across garbage collections.
Inside the V8 backend, unreachable source modules, evaluated or unlinked JSON modules,
native functions and immutable prototypes can be collected with their owners. Realm contexts
now use a weak registry and traced ownership too; current realms, saved nested entries,
rooted objects/functions and queued jobs keep their contexts alive. Realm principals are
released once at collection or shutdown. Promise-job drain scheduling is isolated per runtime,
so restarting a runtime on the same thread does not inherit an abandoned pending drain. Delayed V8 tasks also keep their original
registration, so an isolate address reused by a later runtime does not retarget old work. Tasks handed to the
embedder also carry a weak runtime owner and cannot execute in a replacement runtime. Accepted
dispatched work is reclaimed before isolate disposal, leaving late event-loop payloads empty. The initial context stays alive, and JSON modules
linked but never evaluated remain retained. Local lifetime tests pass; full browser validation
runs in CI. Servo still roots page globals and loaded module maps: page collection and memory
or performance improvements have not been demonstrated.

For maintainers, [`docs/INVENTARIO_CODICE_ROVES.md`](./docs/INVENTARIO_CODICE_ROVES.md)
maps the Roves-specific code areas, their approximate size, existing library dependencies and
possible opportunities to delegate more work to external libraries.

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
  (`<script src>`, `<link href>`, `modulepreload` hints, etc.), every `.js`/`.mjs`/`.css` file
  (so lazily imported chunks never wait on an asset pack), plus anything matched by
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
| Android | 🚧 Native WebView APK, `mach bundle --android`, real release signing | Android WebView |
| iOS/iPadOS | 🚧 Native WKWebView, `mach bundle --ios`, real release signing | WKWebView |
| Nintendo Switch | 🚧 In development | `nx` |
| Nintendo 3DS | 🚧 In development | `ctru-rs` |
| PlayStation Portable (PSP) | 🚧 In development | `rust-psp` |
| PlayStation Vita | 🚧 In development | `vitasdk-rs` |
| PlayStation 4 | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| PlayStation 5 | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| Xbox One | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |
| Xbox Series X\|S | 🚧 In development | — (official SDK, NDA-gated; no public Rust crate) |

Desktop targets (Windows, macOS, Linux) use Servo. Mobile games use the system
WebView and do not ship or compile Servo. See [mobile packaging](support/MOBILE.md)
for Android APK and iOS app instructions — both platforms run edge-to-edge
(status bar and system navigation/home indicator hidden) and are built the
same way as desktop, via `mach bundle --android`/`--ios`, no separate Xcode-
or Gradle-only workflow needed. `--android-release`/`--ios-release` produce a
real, signed release build instead of the default debug/simulator one — see
`support/MOBILE.md` for the required signing credentials (an Android keystore
you can generate yourself, or an Apple Distribution certificate + provisioning
profile from your own Apple Developer Program account for iOS). CI builds a
native WebView smoke-test APK on Android and an unsigned iOS Simulator build
on macOS, plus a real signed-release verification for Android. Console
targets remain on the roadmap.

## Embedding

Your web content has no *build-time* way to know it's running inside Roves rather than a
regular browser (or, for that matter, Tauri). A build-time signal is still an option if you
want one (the parent pixi-vn-react-template project this fork ships with exposes a
`process.env.ROVES_BUILD` value, set by whoever builds specifically for Roves — see its
`vite.config.ts` — a convention of that project, not something Roves itself provides). For a
genuine *runtime* check instead, Roves
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
  `isAvailable()` — a genuine, synchronous runtime "is this page running inside Roves" check —
  and `systemInfo()`, host OS/engine diagnostics (OS type/version, bitness, architecture, the
  running Servo build's own version) for bug reports and graphics-compatibility triage.
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

Separately, letting a player export/import a save as a plain file (`<a download>` on a Blob,
`<input type="file">`) works out of the box with no `roves-api` call needed — Roves intercepts
both transparently on every platform (a native "Save As"/"Open" dialog on desktop, a
player-visible folder and native document picker on Android/iOS). See the wiki's
[Save-game storage](https://github.com/DRincs-Productions/roves-wiki/blob/main/content/docs/integrations/saves.mdx)
page for the pattern. Desktop also intercepts programmatic `anchor.click()` downloads and
retains the original Blob before Servo can try to navigate to its opaque-origin `blob:` URL.

## Getting started

Prebuilt, versioned engine shell builds (no game content bundled — see below) are published
on the [Releases page](https://github.com/DRincs-Productions/roves/releases) as a portable
zip for Windows, macOS, and Linux, starting with `v0.1.0`. Each platform is published twice
— the plain default build (`roves_shell_<platform>.zip`) and a Steam-enabled one
(`roves_shell_<platform>_steam.zip`, see the "Steam" section above) — pick whichever your
game needs. Grab one of those if you just want to try the shell; the rest of this section is
for building from source instead (e.g. to bundle your own game's content, or to work on
Roves itself).

If you're iterating on your own game's content, though, you likely don't need any of this at
all: the [Roves VS Code extension](https://marketplace.visualstudio.com/items?itemName=DRincs-Productions.roves-run)
([source](https://github.com/DRincs-Productions/roves-vscode)) downloads a prebuilt shell for
you and runs your already-built `dist/` folder directly from your editor, with no toolchain
install and nothing added to your project — see the
[wiki](https://github.com/DRincs-Productions/roves-wiki) for details.

An AI coding agent working in this repo (or any project that uses Roves) can also load
[`.claude/skills/roves`](./.claude/skills/roves/SKILL.md) — a Claude Code skill covering what
Roves is and how to test a game with it, wire up the VS Code extension, or add a
`roves-action` GitHub Actions workflow, without re-deriving any of that from scratch.

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
The Steam emulator job also waits up to 60 seconds for the page's end-to-end result marker,
so Xvfb startup time does not consume the page-load window.

### Diagnosing a launch that appears to do nothing

The development panic handler writes the message, thread and source location to `roves.log`
before hard-fail termination, so that exit path preserves its cause.

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

### Repeatable performance fixtures

Roves [v0.7.13](https://github.com/DRincs-Productions/roves/releases/tag/v0.7.13) delegates
global intrinsic lookup to V8 and handles exceptions without source locations safely.
The verified release passes DOM/iframe, worker FIFO/timer and visible typewriter/Pixi probes.

The v0.7.13 consumer CI passes on all platforms. Updated visible Chrome/Roves reference
measurements and their limits are recorded in [the performance notes](docs/ROVES_PERFORMANCE.md).

The bundled diagnostic page also exposes deliberately minimal comparison workloads through
`?perf=blank`, `?perf=pixi-static`, and `?perf=pixi-animated`. Run the same built page, window
size, warm-up and sampling interval in Roves and Chrome. The static PixiJS fixture renders one
frame and stops its ticker; unlike the normal diagnostic page, these fixtures omit gamepad
polling, FPS probes, Steam checks and other page-level timers that would contaminate an idle
measurement. The entry point loads the normal diagnostic panels lazily, so performance
fixtures also avoid their module side effects, including Tone.js initialization. Rebaseline both
browsers after this fixture correction; it is not an engine speedup. The measurement plan and current findings are tracked in
[`ROVES_PERFORMANCE.md`](./docs/ROVES_PERFORMANCE.md).

Roves [v0.7.12](https://github.com/DRincs-Productions/roves/releases/tag/v0.7.12) caches actual WebGL drawing-buffer dimensions between resizes
to avoid repeated synchronous thread queries. Isolated getter benchmarks show lower latency;
the effect on whole-game CPU still needs measurement.
Pixi comparison fixtures explicitly select WebGL and disable their FPS polling timer.
It also delegates Web Audio frequency analysis to RealFFT/RustFFT,
replacing the quadratic direct transform while preserving Web Audio scaling and smoothing.
The analyser transport uses Crossbeam's bounded queue to retain the latest audio
window without per-block DOM tasks; whole-game CPU savings remain to be measured.
An isolated debug/headless test with eight unread analyzers measured about 43% lower process
CPU, with about 1.2 MiB more private memory; conditions and variability are in the performance notes.
Worker lifecycle diagnostics also check that terminating idle workers releases their threads;
the release fix wakes the existing Crossbeam event loop. Three repeated CI-artifact
runtime cycles release all four worker threads, with no progressive private-memory growth
observed in those cycles. The FIFO/timer regression probe also passes. No scheduler is added.
The release structured-clone bridge also moves V8's serialized byte vector through Rust
ownership instead of copying it twice; full ArrayBuffer transfer still has separate payload
copies. In an alternating optimized-build benchmark, median 4 MiB round-trip latency falls
from 5.72 to 3.88 ms for cloning and 6.37 to 4.15 ms for transfer. These are API timings,
not whole-game CPU savings; the Chrome transfer reference remains about 0.10 ms.
Worker OffscreenCanvas WebGL rendering is not yet supported; the canvas capability probe
distinguishes this from working OffscreenCanvas contexts on the DOM thread.
The v0.7.13 release removes redundant standard-class hooks from worker globals,
delegating intrinsic lookups to V8. All desktop/V8 CI checks and the downloaded runtime pass;
the fixed-work direct-lookup probe takes 93 ms in the new debug build, versus 8551 ms in the
previous optimized build. This diagnostic comparison is not a matched-profile CPU percentage.
It uses V8 native data properties for Window's lazy interfaces, with 39 engine-layer tests
passing, including cross-global ownership, exception recovery and missing source locations.
The large worker probe scales from about 8.4 s with one worker to 1.1 s with eight on this machine;
total process CPU remains about 11 core-seconds and observed private memory rises from 174 to 250 MiB.
These headless arithmetic probes do not establish game frame pacing or CPU parity.
Animation comparison fixtures omit FPS polling; the Three.js panel releases its scene
resources and canvas when unmounted.
Downloaded CI builds pass the numerical and WebGL resize/readback checks; reproducible
API benchmarks live in [`support/perf`](./support/perf/README.md), with results and scope in
[`ROVES_PERFORMANCE.md`](./docs/ROVES_PERFORMANCE.md). These measurements do not establish whole-game parity with Chrome.

Windows uses mimalloc by default; `--features system-allocator` selects the system allocator.
`./mach build --features mimalloc` forces mimalloc on other targets (normally jemalloc there).
The `perf-ab.yml`
workflow publishes Windows release builds of both to the `perf-ab` pre-release.

For shell-side counts, launch a desktop build with `ROVES_PERF_LOG_INTERVAL_MS=10000` (or another
positive interval). The normal `roves.log` then receives aggregate `[roves-perf]` lines for SDL
events/timeouts, redraw requests and coalescing, WebView paints, and final window presents, plus
`[roves-perf-time]` lines with total/max microseconds spent in WebView paint, egui update, shell
paint and window present. The diagnostic is disabled by default and does not create a sampling
timer or background thread.

Desktop builds present a full-window game directly (fast path A): with no dialog, status
overlay, egui focus, active AccessKit or pending egui textures, the shell skips egui tessellation
and paint and only blits Servo's off-screen frame; any other frame uses the composed path. Watch
`direct_presents` with `ROVES_PERF_LOG_INTERVAL_MS=1000`; see
[the rendering plan](./docs/FAST_PATH_RENDERING.md).

Compiled graphics shaders are cached per game on disk, so later launches skip shader-compile stalls.

When the window is minimized or fully covered, the page becomes `document.hidden` (a
`visibilitychange` event fires) and is throttled, as in Chrome, so games can pause their music.

Images, audio and other files a game downloads over http(s) are kept in a per-game disk cache
and reused on later launches while the server marks them fresh.

On Windows laptops with two GPUs, the shell asks NVIDIA Optimus and AMD switchable graphics for
the discrete GPU. WebXR is not built into the shell.

[Tauri]: https://v2.tauri.app/distribute/

## Relationship to upstream Servo

Roves tracks a pinned upstream Servo release plus a small set of targeted patches — see
[CUSTOMIZATIONS.md] for the itemized list, and [AGENTS.md](./AGENTS.md) for how upgrades to
a newer Servo release are meant to happen. All credit for the underlying engine goes to the
[Servo Project](https://github.com/servo/servo) and its contributors; issues specific to
this fork's own customizations should stay local to this repository rather than upstream.
The SDL3 virtual gamepad CI probe and remaining hardware checks are documented in
[`SDL3_WINDOWING_TESTING.md`](./SDL3_WINDOWING_TESTING.md). On macOS, gamepad support is off by
default and can be tried experimentally by launching with `ROVES_MACOS_GAMEPAD=1`; the test
page's Input section shows every key, mouse button and gamepad event the page receives.

Retired V8 foreground handlers release captured embedder data outside the registry lock,
so a destructor can reenter registration safely (covered by normal/JITless backend tests).

Unregistering a V8 runtime also destroys its delayed engine tasks still waiting in the timer
(such as `Atomics.waitAsync` timeouts) before the isolate goes away, instead of leaking them.

Code that calls into another same-origin window (for example a parent page calling a function
defined in an iframe) now reports the window whose code is running as its caller, as browsers
do, and callbacks run by the engine no longer see the script that registered them as the caller.
`FinalizationRegistry` cleanup callbacks can call page APIs such as `drawImage` on V8 without
crashing the page's script thread.
`postMessage`/`structuredClone` of a `SharedArrayBuffer` or `WebAssembly.Module` fails with a
`DataCloneError` on V8, as it did on SpiderMonkey, instead of failing silently or with a generic error.

A reference to a window (`iframe.contentWindow`, `window.opener`, a popup from `window.open`)
stays the same object when that window navigates to another page served by the same script
thread, as in browsers. Going back or forward in history therefore reloads the page instead of
resuming it from memory (`session_history_max_length` defaults to 0).

V8 migration status and agent handoff are maintained in [docs/V8_MIGRATION.md](docs/V8_MIGRATION.md).
The current Linux GC smoke has an entry-point regression under investigation; removed-frame
collection remains diagnostic, not a completed page-retention guarantee.
