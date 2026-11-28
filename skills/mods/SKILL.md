---
name: roves-mods
description: Use when making, testing, packing or shipping mods and DLC for a Roves game, or when a game should support them - the add-on package.json and roves section, overrides/ and main modules with defineMod and mods.loadAll, roves mod init/dev/check/pack, official DLC recorded in roves-dlc.json and shipped with roves bundle --dlc, --no-mods, and sharing the game's libraries (pixi.js, @drincs/pixi-vn, tone...) with add-ons as single instances through named entry chunks. Also for diagnosing why an add-on is incompatible or not listed.
---

# Mods and DLC for Roves games

A Roves game can load **DLC** (made by the game's developer, shipped with the game or installed
next to it) and **mods** (made by anyone). The player lists them, the game lets the player turn
them on and off, and their code runs when the game calls `mods.loadAll(api)`. Changes apply at
the next launch. Needs the Roves desktop player v0.9.2 or later and `@drincs/roves-cli` 0.6 or
later; mobile builds do not load add-ons.

User documentation: the wiki page "Mods and DLC" (https://github.com/DRincs-Productions/roves-wiki,
`content/docs/integrations/mods-dlc.mdx`).

## Where add-ons live

| Place | What |
| ----- | ---- |
| `dlc/` next to `game/` (macOS: `Contents/Resources/dlc`) | official DLC, `<id>.rovesmod` |
| `mods/` next to `game/` | mods, `<id>-<version>.rovesmod` or folders |
| the player's profile `mods/` (Windows `%LOCALAPPDATA%\Roves\Player\<game>\mods`, macOS `~/Library/Application Support/Roves/Player/<game>/mods`, Linux `~/.config/Roves/Player/<game>/mods`) | mods; the recommended place, and the only one on macOS |
| `roves mod dev` | one unpacked folder, for development |

The profile also holds `mods.json` (the player's disabled add-ons and load order).

## The add-on: package.json

An add-on is a folder (or its `.rovesmod` ZIP) with an ordinary `package.json`:

```json
{
  "name": "@author/new-ending",
  "displayName": "New ending",
  "description": "An extra ending after chapter 5.",
  "version": "1.2.0",
  "author": "Author",
  "type": "module",
  "main": "dist/index.js",
  "files": ["dist", "overrides", "cover.png"],
  "peerDependencies": { "@drincs/pixi-vn": "^1.3.0", "pixi.js": "^8.0.0" },
  "dependencies": { "lodash-es": "^4.17.21" },
  "roves": {
    "kind": "mod",
    "game": { "name": "mygame", "version": "^1.4.0" },
    "mods": { "@other/characters": "^2.0.0" }
  }
}
```

- `name` is the id (npm rules); `version` is semver; ranges follow npm (`^`, `~`, `>=`, `||`).
- `roves.kind`: `mod` (default) or `dlc`. `roves.game.name` is the `name` of the game's own
  `package.json`. `roves.mods`: add-ons this one needs. `roves.steamAppId` (DLC): shown only when
  Steam reports the DLC installed.
- `peerDependencies`: libraries reused from the game (see "Shared libraries"). `dependencies`:
  bundled into the add-on.
- Cover: `cover.png`, `cover.jpg` or `cover.webp` at the root. No field needed.
- `overrides/<path>` replaces or adds the game file `<path>`. The last add-on in load order wins.
- `main`: an ES module whose default export has `activate(api, info)`.

## The game side

```ts
import { mods } from "@drincs/roves-api/mods";
import { relaunch } from "@drincs/roves-api/process";

await mods.loadAll({ addLabel, addCharacter }); // expose only what modders may use
const list = await mods.list(); // id, name, description, cover_url, version, enabled, active, compatible, problems...
await mods.setEnabled(id, false);
if (await mods.restartRequired()) await relaunch();
```

The add-on's main module:

```ts
import { defineMod } from "@drincs/roves-api/mods";
export default defineMod({ activate(api, info) { api.addLabel(myNewEnding); } });
```

Mods are allowed by default. `roves bundle --no-mods`, or `"roves": { "mods": false }` in the
game's `package.json`, turns them off. DLC still load.

## Commands

| Command | Use |
| ------- | --- |
| `roves mod init [dir] --game <unzipped package> \| --game-name <name> [--kind dlc]` | New `package.json`, `index.js`, `overrides/`; in an existing project only adds `roves`. Prints the libraries the game shares. |
| `roves mod dev [dir] --game <unzipped package>` | Runs the game with the folder loaded unpacked (`--roves-dev-mod`). `overrides/` is read live; other changes restart the game (`--no-watch` to turn it off). |
| `roves mod check [mod] --game <unzipped package> [--json]` | The player's checks: game name and range, shared libraries, `main`, and for a DLC whether that `.rovesmod` is the recorded build. Exit code 1 when incompatible. |
| `roves mod pack [dir] [-o <dir>] [--project-dir <game>] [--json]` | Writes the files `npm pack` would take (never `node_modules`/`.git`), exact `dependencies` versions, deterministic. Mod: `<id>-<version>.rovesmod`. DLC: `<id>.rovesmod`, recorded in the game project's `roves-dlc.json`. |
| `roves bundle -c dist --dlc <folder or .rovesmod>` | Ships an official DLC in `dlc/`. A folder is packed and recorded first; a file must already be recorded. |

## Official DLC

Only the game's own DLC load. `roves mod pack` (run with the DLC inside the game's project, or
with `--project-dir`) records id, version and SHA-256 of the `.rovesmod` in `roves-dlc.json`
next to the game's `package.json`. Keep that file in the repository: old versions stay recorded.
`roves bundle` copies the list into `game/roves-game.json`, and the player lists a DLC only when
its file matches. Recorded DLC are enabled by default. A new DLC, or a new DLC version, is
recognized only by game builds made after it was recorded. Someone who edits an installed game
can change the list, and that is accepted.

With the same id installed more than once, the player keeps the highest version.

## Shared libraries (one instance)

Two copies of libraries like `pixi.js` break the game (two renderers, two registries,
`instanceof` failing across copies). An add-on must use the game's own instance.

**Game developer:** the developer controls the build. Roves does not change it. Give each
library to share its own **entry chunk named after the package without its scope**, with the
public API kept:

```js
// vite.config.js of the game
export default {
    build: {
        rollupOptions: {
            input: { index: "index.html", "pixi.js": "pixi.js", "pixi-vn": "@drincs/pixi-vn", tone: "tone" },
            preserveEntrySignatures: "strict",
        },
    },
};
```

- `output.manualChunks` is **not** enough: it keeps one copy inside the game, but the chunk exports
  only a few minified internal names (`export { a as n }`), not `Sprite` or `Assets`.
- `roves bundle` looks, for every `dependencies` entry, for `<name>-<hash>.js` (or `<name>.js`).
  It records the found chunks in `game/roves-game.json` (`shared`: version, file, default
  export).
- `roves bundle` warns when a single-instance library has no chunk, or only a `manualChunks` one.
  Two chunks with the same name are an error.
- Libraries to always share this way: `pixi.js` and `@pixi/*`, `@drincs/pixi-vn`, `phaser`,
  `three`, `@babylonjs/core`, `excalibur`, `kaplay`/`kaboom`, `matter-js`, `planck`, `tone`,
  `howler`, `@pixi/sound`, `motion`/`framer-motion`, `gsap`, `react`/`react-dom`, `vue`,
  `solid-js`.
- Small stateless helpers (e.g. `lodash`) can stay inside the game; mods bundle their own.

**Add-on developer:** declare the library in `peerDependencies` with the range you need, keep it
out of the bundle and point it at the player's re-export module
`/__mods/_shared/<package>.js`:

```js
// vite.config.js of the mod
export default {
    build: {
        lib: { entry: "src/index.js", formats: ["es"], fileName: () => "index.js" },
        rollupOptions: {
            external: ["pixi.js", "@drincs/pixi-vn"],
            output: {
                paths: {
                    "pixi.js": "/__mods/_shared/pixi.js.js",
                    "@drincs/pixi-vn": "/__mods/_shared/@drincs/pixi-vn.js",
                },
            },
        },
    },
};
```

The player serves that module as `export * from "/<the game's chunk>"`, so the mod gets the
game's very module. A redirect would create a second instance. `roves mod init --game` prints
what the game shares; `roves mod check` fails when a peer range does not match.

## Common problems

| Symptom | Cause / fix |
| ------- | ----------- |
| DLC missing from `mods.list()` | Not recorded for this build: pack it with `roves mod pack` in the game project, then build the game again. A folder in `dlc/` is never an official DLC. The player log line `mods ... notes:` says why. |
| `needs library X ^a, not shared by the game` | The game has no entry chunk named after X: add it to the game's `input` (see above). |
| `needs game version ...` | `roves.game.version` does not include this game version. |
| Mod runs but the game breaks / objects not recognized | The mod bundled its own copy of a single-instance library: make it `external` with `output.paths`. |
| `X only exports internal names` (bundle warning) | X was split with `manualChunks`: use an entry with `preserveEntrySignatures: "strict"`. |
| Changes not visible | Enabling, disabling and order apply at the next launch (`relaunch()`). |
