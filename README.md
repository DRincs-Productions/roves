# Roves

**Roves** turns a built web game (HTML/CSS/JS, e.g. a Vite or webpack `dist/` folder with an
`index.html`) into a native application that players download and double-click, not a
browser tab. Game developers never compile anything: they package their build with the
[`roves` CLI](https://github.com/DRincs-Productions/roves-cli), the
[GitHub Action](https://github.com/DRincs-Productions/roves-action) or
[Roves Packmaster](https://github.com/DRincs-Productions/roves-packmaster), and the
[Roves wiki](https://github.com/DRincs-Productions/roves-wiki) explains every step.

This repository holds the desktop **player** that those tools download, its CI and release
pipeline, and the agent skills.

## The desktop player

| Platform | Package | Graphics |
| --- | --- | --- |
| Windows x64 | `play.exe` + `game\` | DX12 through wgpu |
| macOS arm64 | `<name>.app` (game in `Contents/Resources/game`) | Metal through wgpu |
| Linux x64 | executable + `game/` | Vulkan through wgpu |

The game runs in Chromium through [CEF](https://bitbucket.org/chromiumembedded/cef) 154
(the [`cef`](https://crates.io/crates/cef) crate), off screen: Chromium's own V8 runs the
game's JavaScript and paints into a GPU texture that
[wgpu-weld](https://github.com/merely-made/wgpu-weld) hands to a wgpu device owned by the
player, with no CPU copy on the presentation path and the Chromium sandbox on. SDL3 owns the
window, the event loop and input. Without an importable GPU (software rendering, some VMs)
the player falls back to CEF's software paint.

What the player adds around the page:

- one game document per window: reloads and full navigations are refused; external links
  open in the system browser only after a real click;
- the game is served from a private loopback origin, so saves (`localStorage`, IndexedDB)
  persist between launches;
- a native startup screen until the game has painted its first frame;
- the project icon on the window, the executable (`play.exe`) and the macOS app;
- a host API for the page (`@drincs/roves-api`: `core`, `saves`, `steam`).

Each release ships three variants per platform: **standard**; **Steam**, with the Steamworks
redistributable next to the executable (Steam missing is never fatal); and **MCP**
(development only), a local bridge that lets an AI agent look at the running game
(screenshots, frame sequences, page text, HTML/CSS, the PixiJS scene), play it and run
JavaScript in it. Never ship the MCP variant to players.

Sources: [`support/cef-windows-player`](support/cef-windows-player/README.md) (Windows, and the
modules the other players share through `#[path]`),
[`support/cef-macos-gate`](support/cef-macos-gate/README.md),
[`support/cef-linux-gate`](support/cef-linux-gate/README.md); the MCP server is in
[`support/cef-windows-player/mcp`](support/cef-windows-player/mcp/README.md).

### Packed game content

By default `roves bundle` (roves-cli, with player v0.9.1 or later) writes the game folder as a
few archives instead of loose files: one archive with all the code (JS chunks, CSS, HTML),
asset archives grouped by top-level folder (at most 300 MB each), uncompressed by default.
The player memory-maps them and serves files straight from memory, with read-ahead of the next
files of the same folder; an optional compression level (1-9) makes the download smaller at
some in-game cost. Options: `--content-layout archive|files`, `--compression-level 0-9`,
`--max-archive-size` (also in roves-action and Roves Packmaster). Details in the
[wiki](https://github.com/DRincs-Productions/roves-wiki) and
[support/cef-windows-player/README.md](support/cef-windows-player/README.md) ("Packed game
content").

### Mods and DLC (in progress)

The player loads DLC and mods placed next to the game or in the player's profile: each one is
a folder or a `.rovesmod` file with a `package.json` (id, name, description, version, the game
versions it supports as an npm range, libraries) and an optional `cover` image. Files in its
`overrides/` folder replace the game's, and its `main` module runs when the game calls
`mods.loadAll(api)` from `@drincs/roves-api/mods`, which also lists add-ons and turns them on
and off. Libraries that must run once (such as `pixi.js`) are shared with mods when the game
builds them as entry chunks named after the package. Only the game's official DLC load: roves-cli records each packed DLC in the game
project and the package carries that list. roves-cli packs and checks add-ons
(`roves mod init/dev/check/pack`, `roves bundle --dlc`/`--no-mods`); the player loads them from
v0.9.2. Details in
the [wiki](https://github.com/DRincs-Productions/roves-wiki) and
[support/cef-windows-player/README.md](support/cef-windows-player/README.md) ("Mods and DLC").

## Mobile (experimental)

Android APKs and iOS builds still come from the earlier Servo-based engine at the tag
[`v0.7.15`](https://github.com/DRincs-Productions/roves/tree/v0.7.15), which the CLI and the
Action check out on demand (`roves android`, `roves ios`). See the wiki's mobile page.

## Releases

Pushing a tag `v*.*.*` runs [`release.yml`](.github/workflows/release.yml): it builds and
verifies the three players (with the
[visual-novel-template](https://github.com/DRincs-Productions/visual-novel-template) sample
game) and publishes `roves_shell_{windows,macos,linux}.zip`, their `_steam` and `_mcp`
variants and `SHA256SUMS.txt`. The published zips are the shell only: the CLI, the Action and
Packmaster add the developer's game as `game/`. A push to a `release-check/<name>` branch runs
the same pipeline as a dry run without publishing. Ordinary commits run the Windows player
workflow only; macOS and Linux run at release time or by hand.

## Agent skills

This repository ships three agent skills, installable with `npx skills add DRincs-Productions/roves`:
`roves` ([`skills/core`](skills/core/SKILL.md)) explains Roves and every command of the
[`roves` CLI](https://github.com/DRincs-Productions/roves-cli) (`npm i -g @drincs/roves-cli`),
`roves-diagnostics` ([`skills/diagnostics`](skills/diagnostics/SKILL.md)) diagnoses a running
game through its development MCP bridge, and `roves-playtest`
([`skills/playtest`](skills/playtest/SKILL.md)) lets an agent play and test it: screenshots and
frame sequences, page text and controls, HTML/CSS inspection and the PixiJS scene
([PixiJS devtools reference](skills/playtest/references/pixijs-devtools.md)).
`roves-mods` ([`skills/mods`](skills/mods/SKILL.md)) covers mods and DLC, including how a game
shares single-instance libraries such as `pixi.js` with them, and `roves-github-action`
([`skills/github-action`](skills/github-action/SKILL.md)) writes GitHub Actions workflows with
roves-action. [`skills/categories.json`](skills/categories.json) maps each skill to its folder.

## Repository layout

| Path | What it is |
| --- | --- |
| `support/cef-windows-player`, `support/cef-macos-gate`, `support/cef-linux-gate` | The three players (separate Cargo workspaces) |
| `support/cef-host-api-tests` | CEF-free unit tests of the loopback server, host API and shared rules |
| `support/vendor/wgpu-weld` | Temporary fork of wgpu-weld (software-paint fallback, navigation veto); see its `ROVES_PATCHES.md` |
| `support/sdl3-gamepad-probe` | SDL3 virtual-gamepad probe (manual workflow) |
| `skills/` | Agent skills |
| `test-page/` | A web page exercising Web APIs inside the player |
| `roves-action`, `roves-cli`, `roves-packmaster`, `roves-api`, `roves-vscode`, `roves-wiki` | The ecosystem's other repositories, as submodules |

The git history records every significant change and its
reason; [`TODO.md`](TODO.md) is the open backlog; [`AGENTS.md`](AGENTS.md) holds the rules for
coding agents. Heavy builds run in CI; [`docs/ROVES_PERFORMANCE.md`](docs/ROVES_PERFORMANCE.md)
describes how performance is measured against Chrome.

## History

Roves (an anagram of *Servo*) began as a customized copy of the [Servo](https://servo.org/)
web engine, later running its JavaScript on V8 instead of SpiderMonkey. On 2026-11-23 the
desktop product moved to the CEF player and the Servo and V8 sources were removed from `main`;
`v0.7.15` is the last Servo-based release and still serves the mobile builds. A Blitz/Deno
engine and a Netrender compositor were also evaluated and dropped. The git history keeps the
details (the changelog kept until 2026-11-28 is at [roves 791eff000a6](https://github.com/DRincs-Productions/roves/blob/791eff000a6/CUSTOMIZATIONS.md)).

## License

[Mozilla Public License 2.0](LICENSE). Bundled third-party components (CEF/Chromium, SDL3,
wgpu, wgpu-weld, the Steamworks redistributable) keep their own licenses. Every package ships
the license texts and Chromium's third-party notices (`provenance\licenses\` on Windows,
`licenses/` on Linux, `Contents/Resources/licenses/` on macOS).
