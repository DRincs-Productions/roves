---
name: roves
description: What Roves is, and how to test a game with it, wire up the VS Code extension, or add a Roves GitHub Actions workflow. Use whenever a task mentions Roves, "servoshell", running/bundling a game with Roves, or the Roves VS Code extension/action.
---

# Roves

**Roves** is a customized fork of the [Servo](https://servo.org/) web engine, repurposed as a
runtime for shipping web-based games (PixiJS, Phaser, Three.js, or any HTML/CSS/JS bundle) as
real native desktop apps instead of running them in a browser tab. Full docs, including
every wiki page referenced below, live at **https://roves.pixi-vn.com/** — read there for
anything this skill doesn't cover in enough depth, and prefer it over guessing when a detail
here seems stale (this skill is a map to the docs, not a replacement for them).

The ecosystem has several separate repos — know which one a task actually concerns before
picking an approach:

- **`roves`** — the engine itself (this fork of Servo). Source builds happen here.
- **`roves-action`** (`DRincs-Productions/roves-action`) — a GitHub Action that bundles a
  game's already-built web content into a Roves shell, for CI.
- **`roves-vscode`** (`DRincs-Productions/roves-vscode`, marketplace id
  `DRincs-Productions.roves-run`) — a VS Code extension ("Roves") that downloads a prebuilt
  shell and runs a project's built `dist/` folder locally, no toolchain needed.
- **`roves-api`** (`@drincs/roves-api` on npm) — the JS/TS package a game's own code imports
  to talk to Roves (saves, Steam, process control, diagnostics, etc.).
- **`roves-packmaster`** — a GUI wrapping `mach bundle` for non-technical game devs.

## Testing/launching a game with Roves

When asked to "test this with Roves" or "run this in Roves", pick the lightest option that
fits the situation — don't reach for a full source build unless the task actually needs one
(editing the engine itself, or a flag/feature not yet in a published release):

1. **Fastest — VS Code extension** (best default when working inside VS Code / this harness
   and the project already has a built `dist/` folder, or can be built with `npm run build`):
   - Ensure the project has an `index.html` inside its output folder (default `dist`).
   - Install the extension if not already present: marketplace id
     `DRincs-Productions.roves-run`.
   - Run the **Roves: Run** command (Command Palette, or the status bar item it adds). First
     run downloads the right prebuilt shell for the OS (cached outside the workspace,
     persists across runs) and shows progress in the status bar; then it launches the shell
     pointed at the project's built output.
   - Relevant settings (`.vscode/settings.json` or user settings), all optional:
     `roves-run.distFolder` (default `"dist"`), `roves-run.version` (default `"latest"`),
     `roves-run.steam`, `roves-run.steamAppId`, `roves-run.windowSize`,
     `roves-run.extraArgs`. See the extension's own README/wiki page
     (`https://roves.pixi-vn.com/docs/vscode`) for the full list.
   - Output from the shell process streams to the **Roves** output channel; use **Roves:
     Stop** to kill it.
   - This is also how *this skill's own agent* should self-test a change to a game's web
     content without touching the engine at all.

2. **No editor / scripted environment — download a prebuilt shell directly:**
   - Grab a portable zip from `https://github.com/DRincs-Productions/roves/releases`
     (`roves_shell_<platform>.zip`, or `_steam` variant), or use the rolling `test` release
     tag's assets for the latest engine-side patches not yet in a tagged version.
   - Extract it, then either:
     - Point it at a real bundle: run `mach bundle` (see below) with `--content-dir <dist>`,
       which produces a `launch.json` + packed content next to the shell binary; or
     - For a quick one-off, write/edit the shell's own `launch.json` next to its binary:
       `{"url": "<absolute path to index.html>", "args": [...]}` — an **absolute** `url`
       ignores the extraction dir entirely (no `content_dir` key), which is exactly how the
       VS Code extension itself points a cached shell at an arbitrary project without ever
       running `mach bundle`. Don't set `content_dir` and `url` together — `content_dir`
       wins and `url` is ignored if both are present.
   - Launch the binary (`play`/`play.exe`/`play.app`) directly, or pass a URL as its first
     CLI arg (`./play https://example.com`) for a non-bundled, ad hoc load.
   - Logs land in a `roves.log` next to a per-game data dir (OS-appropriate: e.g.
     `%LOCALAPPDATA%\<window-title-or-game-name>\roves.log` on Windows) — check it first when
     something doesn't render as expected; a green build/launch does **not** by itself prove
     a UI change actually rendered correctly — screenshot the real window when verifying a
     visual fix, don't trust logs alone.

3. **Building the engine from source** (only when the task needs an engine change, or a flag
   not yet released): follow this repo's own README.md "Getting started" section
   (`./mach bootstrap`, `./mach build`, `./mach bundle`) — platform-specific prerequisites are
   listed there, don't restate them from memory. `mach build`/`mach bundle` fail immediately
   on a fresh checkout unless `tests/wpt/tests/tools/` exists (this repo deliberately excludes
   the multi-GB WPT test content from git, but `mach`'s command loader still imports test
   tooling from that path on every invocation) — sparse-checkout just that `tools/`
   subdirectory from the pinned upstream Servo tag before building; see this repo's own
   `.github/workflows/release.yml` for exactly how CI does it if a concrete command is needed.

## Setting up the VS Code extension in a project

To wire up a game project so a human (or another agent session) can use **Roves: Run**
without extra setup:

1. Confirm the project has a working build producing a loose (uncompressed) `index.html` +
   assets folder — `roves-run.distFolder` must point at real, unpacked output, not something
   `mach bundle --content-compress=auto` already packed.
2. If the default output folder isn't `dist`, add to `.vscode/settings.json`:
   ```json
   { "roves-run.distFolder": "build" }
   ```
3. Recommend the extension to contributors via `.vscode/extensions.json`:
   ```json
   { "recommendations": ["DRincs-Productions.roves-run"] }
   ```
4. Nothing else is required — the extension needs no Rust/Python toolchain and adds nothing
   to the project's own dependencies or build output.

## Creating a GitHub Actions workflow for Roves

Use **`DRincs-Productions/roves-action`** — don't hand-roll `mach build`/`mach bundle` calls
in a workflow; the action wraps that (and, by default, skips compiling anything at all by
downloading the same prebuilt shell Packmaster uses). Basic shape, one job per desktop
platform:

```yml
name: bundle
on:
  push:
    branches: [main]
jobs:
  bundle:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
            name: my-game_windows
          - os: macos-latest
            name: my-game_macos
          - os: ubuntu-latest
            name: my-game_linux
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22' }
      - run: npm install && npm run build
      - id: roves
        uses: DRincs-Productions/roves-action@v0
        with:
          content-dir: dist
          artifact-name: ${{ matrix.name }}
      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.name }}
          path: ${{ steps.roves.outputs.archive-path }}
```

Key inputs to reach for based on what's actually asked (don't add ones that aren't needed):

- `content-dir` (required) — the game's own built output, from a prior build step. This
  action never builds the game's web content itself.
- `deb` / `msi` / `dmg` (`'true'`) — an installable package instead of the default portable
  binary, one flag per OS (Linux/Windows/macOS respectively) — pair with `package-name` and
  `package-version`.
- `content-exclude` (newline list of globs) — leave matching files loose/uncompressed instead
  of packed (e.g. a save-data folder).
- `icon` — a custom game icon, works without `advanced-mode`.
- `advanced-mode: 'true'` — compiles the engine from source with `mach build`/`mach bundle`
  instead of downloading a prebuilt shell; slower, unlocks every `mach build` flag, at the
  cost of a much longer CI run. Only reach for this when a needed flag genuinely isn't
  exposed by the action's own inputs.
- To publish to a GitHub Release instead of a workflow artifact, trigger on `push: tags:
  ['v*']`, add `permissions: contents: write`, and replace the `upload-artifact` step with
  `gh release upload "${{ github.ref_name }}" "${{ steps.roves.outputs.archive-path }}"`.

Full input reference and more examples: `roves-action`'s own README, or
`https://roves.pixi-vn.com/docs/action` if that repo isn't checked out locally.

## Talking to Roves from game code

A game's own web content reaches Roves-specific features (saves, Steam, process control,
diagnostics) through **`@drincs/roves-api`**, not by hand-rolling `fetch('roves:...')` calls —
that package is the maintained, typed wrapper. Check `isAvailable()` before calling anything
Roves-only if the same code might also run in a plain browser during development. See
`https://roves.pixi-vn.com/docs/roves-api` for the full module list (`core`, `saves`,
`steam`, `process`, `cache`).
