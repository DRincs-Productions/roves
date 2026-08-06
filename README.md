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

### Desktop — supported today

- **Windows** (x64)
- **macOS** (Apple Silicon and Intel)
- **Linux** (x64)

### Consoles — in development

The following are on the roadmap but not yet functional:

- PlayStation 5
- Xbox Series X|S
- Nintendo Switch

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
