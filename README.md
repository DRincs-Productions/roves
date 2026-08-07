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

## Goal

Ship web-based games as real, native, double-click-to-run desktop (and, eventually,
console) binaries — without bundling a full general-purpose browser engine like Electron
or CEF, and without paying Chromium's footprint and overhead for a game that only ever
needs a canvas and a window. Servo is already lighter and written in Rust; this fork trims
it down further to exactly what a game needs and nothing else.

`./mach bundle` (see its own `--help`) turns a build into that double-click-ready package
per target platform — see [CUSTOMIZATIONS.md] for what it produces on each.

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

Your web content has no way to know on its own that it's running inside Roves rather than
a regular browser (or, for that matter, Tauri) — Roves doesn't inject any global marker
into the page. Whoever builds your frontend needs to bake that signal in itself, at build
time, however it prefers to (an env var read by the bundler, a build flag, etc.). The
parent pixi-vn-react-template project this fork ships with does exactly that: it sets an
`EMBEDDED_TARGET=roves` environment variable when building the frontend for Roves (see its
`.github/workflows/embedded.yml`), and Vite bakes that into a `__EMBEDDED_TARGET__` constant
the frontend code checks against (`__EMBEDDED_TARGET__ === "roves"`, see e.g. its
`src/lib/hooks/quit-hooks.ts`) — this is a convention of that project, not something Roves
itself provides or requires.

### Talking to native APIs

Web content can't call into Rust directly — there's no Tauri-style `invoke()` runtime built
in. Instead, Roves lets native code register custom URL schemes (`ProtocolHandler`s — see
`ports/servoshell/desktop/protocols/`) that respond to ordinary `fetch()` calls from page
JS. One is shipped today:

- **`roves:`** (`protocols/roves.rs`) — a small, generic "control this app" surface
  (window/process lifecycle; currently just `exit`/`close_window`).

[**`@drincs/roves-api`**](../roves-api) is the JS package wrapping it, deliberately shaped
to feel familiar if you already know `@tauri-apps/api` (though it's a real, independent
implementation, not a shim over Tauri's runtime):

- `@drincs/roves-api/core` — the generic `invoke(cmd, args)`, talking to `roves:`.
- `@drincs/roves-api/process` — `exit()`, built on `core`.

See the "Roves' own general-purpose `invoke()` bridge" entry in [CUSTOMIZATIONS.md] for how
the Rust side is wired up.

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

## Getting started

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
- Package a runnable bundle: `./mach bundle`

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
- Package a runnable bundle: `./mach bundle` (add `--deb` for a Debian/Ubuntu package instead
  of the default self-contained `play.sh`)

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
- Package a runnable bundle: `.\mach bundle`

## Relationship to upstream Servo

Roves tracks a pinned upstream Servo release plus a small set of targeted patches — see
[CUSTOMIZATIONS.md] for the itemized list, and [CLAUDE.md](./CLAUDE.md) for how upgrades to
a newer Servo release are meant to happen. All credit for the underlying engine goes to the
[Servo Project](https://github.com/servo/servo) and its contributors; issues specific to
this fork's own customizations should stay local to this repository rather than upstream.
