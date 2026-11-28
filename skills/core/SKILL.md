---
name: roves
description: Use when packaging a built web game (Vite/webpack dist folder) as a native Roves app with the roves CLI or the @drincs/roves-cli Node API - desktop zips for Windows, macOS and Linux (standard, Steam or MCP development player), project icons, Android APKs and iOS builds, downloading and caching the player, pinning the shell version, and diagnosing roves bundle errors.
---

# Roves CLI (`@drincs/roves-cli`)

## What Roves is

Roves turns a web game (HTML/JS/CSS with an `index.html`) into a native app without the
developer compiling anything:

- **Desktop (Windows x64, macOS arm64, Linux x64)**: the Roves **CEF player** (Chromium Embedded
  Framework for the web content, SDL3 for the window and input, wgpu for presentation). Each
  engine release publishes a prebuilt player per platform; the CLI downloads it, puts the game in
  its `game/` folder, applies the game's icon and zips the result. Three variants exist per
  platform:
    - **standard**: what you ship to players;
    - **Steam**: the same player plus the Steamworks redistributable (`steam_api64.dll`,
      `libsteam_api.dylib`, `libsteam_api.so`) next to the executable, so the game can reach
      Steam through the Roves host API (Steam missing or not running is never fatal);
    - **MCP** (development only): a local MCP (Model Context Protocol) bridge that lets an AI
      agent or IDE read the running game's state and console, take screenshots and frame
      sequences, read the page and the PixiJS scene, inspect HTML/CSS, run JavaScript in the
      page, click, press keys and type (skills `roves-playtest` and `roves-diagnostics`).
      Never ship it to players.
- **Android and iOS (experimental)**: the native WebView tooling of the older Servo-era engine
  (`v0.7.15`, the "mobile pin"), through its `mach bundle --android` / `support/ios/bundle.py` /
  `mach bundle --ios --ios-release`. These need the platform toolchains installed.

The CLI does exactly what the `DRincs-Productions/roves-action` GitHub Action and Roves
Packmaster do, but from any machine and any CI.

## Install

```bash
npm install -g @drincs/roves-cli   # Node.js >= 20.19
roves --help
roves --version
```

Or without a global install: `npx @drincs/roves-cli bundle ...`.

Building the web game is your own step (e.g. `npm run build`); the CLI packages its output.

## `roves bundle`: desktop packages

```bash
roves bundle --content-dir dist [options]
```

The game folder (`--content-dir`, required) must contain `index.html` at its root. Desktop
packaging is pure JavaScript, so **any host can build any desktop platform** (e.g. a macOS
`.app` and a Linux package from Windows): the player's zip entries are copied as they are, so
Unix modes, the setuid `chrome-sandbox` and macOS framework symlinks survive.

| Option                      | Meaning                                                                                                                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-c, --content-dir <dir>`   | Built web game, `index.html` at its root. Required. Packed into the player's `game/` folder (see `--content-layout`).                                                                       |
| `--content-layout <layout>` | `archive` (default with player v0.9.1+): `game/` holds `roves-content.json`, `code.pack` (all JS/CSS/HTML/JSON/WASM) and `data-<folder>[-N].pack` (assets by top-level folder). `files`: loose copy. With an older `--shell-version` the default falls back to `files` with a warning; an explicit `archive` fails. |
| `--compression-level <0-9>` | Archive compression: `0` (default, stored: fastest in game), `1`-`9` smaller download, slower first access. Media (images, audio, video, fonts) are always stored.                          |
| `--max-archive-size <size>` | Largest asset archive, e.g. `300MB` (default), `1GiB`, `500000000`; bigger folders are split (`data-<folder>-2.pack`, ...). At least `1MB`.                                                |
| `-p, --platform <platform>` | `windows`, `macos`, `linux`; repeat it or use a comma list (`-p windows,linux`). Default: the host platform.                                                                                |
| `--icon-png <file>`         | Square PNG project icon, 16-2048 px, at most 8 MiB: window icon, macOS app icon, source of `play.exe`'s icon. Default: `<content-dir>/icon.png`.                                            |
| `--icon-ico <file>`         | Project `.ico` for `play.exe` (Windows). Default: generated from the PNG, else `<content-dir>/icon.ico` or `favicon.ico`. On macOS/Linux only used as the window icon when there is no PNG. |
| `--steam`                   | Package the Steam variant (`roves_shell_<platform>_steam.zip`).                                                                                                                             |
| `--steam-app-id <id>`       | Write `steam_appid.txt` with this App ID next to the executable (for local testing outside Steam; Steam itself does not need it). Requires `--steam`.                                       |
| `--enable-mcp`              | Package the MCP development player (`roves_shell_<platform>_mcp.zip`). Cannot be combined with `--steam`.                                                                                   |
| `--no-mods`                 | Do not load third-party mods (default: `package.json` `roves.mods`, else allowed). DLC still load.                                                                                         |
| `--dlc <path>` | Ship an official DLC in `dlc/` next to `game/`: a DLC folder (packed and registered in the project's `roves-dlc.json`) or a registered `.rovesmod`. Repeatable. |
| `--project-dir <dir>`       | The game's project folder with `package.json` (name and version that add-ons are checked against). Default: the nearest one above the content dir.                                         |
| `-n, --name <name>`         | Base name of the zip and of the folder inside it. Default `roves-game`.                                                                                                                     |
| `-o, --out-dir <dir>`       | Where outputs are written. Default: current directory.                                                                                                                                      |
| `--no-extract`              | Write only the zip, not the unzipped folder.                                                                                                                                                |
| `--shell-version <tag>`     | Engine release of the player (default: the pinned version, see below).                                                                                                                      |
| `--shell-zip <file>`        | Use a local player zip instead of downloading (exactly one `--platform`); for offline work and CI tests.                                                                                    |
| `--no-cache`                | Download into a temporary directory instead of the cache.                                                                                                                                   |
| `--refresh`                 | Download again even if cached.                                                                                                                                                              |
| `--json`                    | Print the results as JSON on stdout.                                                                                                                                                        |
| `--progress <format>` | `plain` (default) or `json`: one `{"type":"progress","platform","stage","fraction"}` line per event on stderr (stages `downloading`, `assembling`, `extracting`, `done`), for tools that show a progress bar. |

### Outputs

For one platform: `<out-dir>/<name>.zip` containing one folder `<name>/` (Windows/Linux) or the
app bundle `<name>.app/` at the zip root (macOS), plus the unzipped `<out-dir>/<name>/` (or
`<name>.app`). For several platforms the zip and folder names get a platform suffix:
`<name>_windows.zip`, `<name>_macos.zip`, `<name>_linux.zip` (the folder inside stays `<name>`).
Existing outputs with the same name are replaced.

| Platform | Package root  | Executable                            | `game/` and window `icon.png` |
| -------- | ------------- | ------------------------------------- | ----------------------------- |
| Windows  | `<name>/`     | `play.exe`                            | next to `play.exe`            |
| macOS    | `<name>.app/` | `Contents/MacOS/<CFBundleExecutable>` | `Contents/Resources/`         |
| Linux    | `<name>/`     | the player executable                 | next to the executable        |

stdout reports, per platform, `archive-path`, `bundle-dir`, `mcp-enabled` and `icon-source`
(`--json` also gives `contentLayout`, `archive` or `files`)
(e.g. `window project (game icon.png 512x512); executable project (ICO generated from game icon.png)`);
progress and warnings go to stderr. Ship the **zip**: the unzipped folder is for a quick look,
and on Windows symlinks of macOS/Linux packages may not be creatable there (a warning says so).

### What happens

1. Downloads `https://github.com/DRincs-Productions/roves/releases/download/<version>/roves_shell_<platform>[_steam|_mcp].zip` (cached).
2. Verifies it: Windows against its own `SHA256SUMS.txt` (nothing missing, changed or
   unlisted); macOS the `Info.plist` executable/icon, the CEF framework; Linux `libcef.so`,
   `chrome-sandbox` with its execute bit, `icudtl.dat`, `resources.pak`. The Steam library
   must be present exactly for `--steam`; `--enable-mcp` needs a complete `mcp/` folder.
3. Adds the game as `game/`: packed into archives once for every platform (default) or as loose
   files. Release assets from v0.9.0 are the player shell only; an older base that still has a
   `game/` (and sample icon/provenance) has them dropped.
4. Applies the project icon; on Windows patches `play.exe`'s icon resource (pure JS).
5. Drops `mcp/` unless `--enable-mcp`; writes `steam_appid.txt` with `--steam-app-id`.
6. Windows: rewrites `PROVENANCE.txt` (a "Game replaced" note) and regenerates `SHA256SUMS.txt`.

### Project icon precedence

- **Window icon**: `--icon-png`, else `<content-dir>/icon.png`, else the largest frame of the
  project ICO (`--icon-ico`, else `<content-dir>/icon.ico` / `favicon.ico`). None: the player
  shows the Roves logo.
- **Windows `play.exe`**: `--icon-ico`, else an ICO generated from the PNG (16-256 px frames),
  else `icon.ico`/`favicon.ico`, else the Roves icon shipped in the player (`roves.ico`).
- **macOS app icon**: an `.icns` generated from the window PNG replaces the Roves one.
- **Linux**: window icon only (no `.desktop` entry).

An explicit `--icon-png`/`--icon-ico` that breaks the rules (square, 16-2048 px, at most 8 MiB,
decodable) fails the run; an invalid file found in the content folder is a warning and the next
source is used.

### Examples

```bash
# Windows package of dist/ on a Windows machine -> ./roves-game.zip and ./roves-game/
roves bundle -c dist

# All three desktop platforms from one machine, named my-game_<platform>.zip, into release/
roves bundle -c dist -p windows,macos,linux -n my-game -o release

# Steam builds with a development App ID (480 = Spacewar test app)
roves bundle -c dist -p windows,linux --steam --steam-app-id 480 -n my-game_steam

# A playtest build an AI agent can drive (never ship it)
roves bundle -c dist --enable-mcp -n my-game_mcp

# Explicit icons
roves bundle -c dist --icon-png assets/icon.png --icon-ico assets/icon.ico

# Machine-readable output for scripts
roves bundle -c dist -p linux --json
```

**Linux sandbox**: unzipping keeps execute bits but not root ownership or the setuid bit of
`chrome-sandbox`. Where unprivileged user namespaces are restricted (Ubuntu 23.10+), players or
the installer must run `sudo chown root:root chrome-sandbox && sudo chmod 4755 chrome-sandbox`,
otherwise Chromium stops with "No usable sandbox!".

**macOS**: the `.app` is not signed or notarized; signing and notarizing is your release step.
**Windows**: `play.exe` is not code-signed.

## Mods and DLC (`roves mod`)

`roves bundle` writes `game/roves-game.json` from the game's `package.json` (found above the
content dir, or `--project-dir`): its `name` and `version` (what add-ons are checked against),
whether mods are allowed (`roves.mods`, or `--no-mods`), the libraries the game shares
(`roves.shared`) and the official DLC from the project's `roves-dlc.json`. Without a
`package.json` the package has no mod or DLC support (a warning). The player loads add-ons from
player v0.9.2: it lists a DLC only when its `.rovesmod` file is the registered build (same
SHA-256), and enables it by default; with the same id twice, the highest version wins.

| Command | What it does |
| ------- | ------------ |
| `roves mod init [dir] --game <package> \| --game-name <name> [--kind dlc]` | Creates `package.json` (`name`, `version`, `main`, `files`, `roves.game` with `^<game version>`), `index.js` (a `defineMod` starter) and `overrides/`; in an existing project only adds the `roves` section. |
| `roves mod dev [dir] --game <package>` | Runs an unzipped package of the game with the folder loaded unpacked (`--roves-dev-mod`; a DLC there needs no registration). `overrides/` is read live; other changes restart the game (`--no-watch` to turn it off). |
| `roves mod check [mod] --game <package> [--json]` | The player's checks against that package: game name and version range, shared libraries, `main`, and for a DLC whether that `.rovesmod` is registered. Exit code 1 when incompatible. |
| `roves mod pack [dir] [-o <dir>] [--project-dir <game>] [--json]` | Writes the files `npm pack` would take (`files`, plus `package.json` and `cover.*`; never `node_modules` or `.git`), `dependencies` rewritten to the installed versions, deterministic. A mod becomes `<id>-<version>.rovesmod`; a DLC becomes `<id>.rovesmod` and is registered (id, version, SHA-256) in the game project's `roves-dlc.json` (found above the DLC folder, or `--project-dir`). Keep `roves-dlc.json` in the repository. |
| `roves bundle --dlc <path>` | Ships an official DLC in `dlc/` next to `game/`: a DLC folder (packed and registered first) or a `.rovesmod` already registered. Repeatable. A new DLC, or a new version, is recognized only by game builds made after it was registered. |

An add-on's `package.json`: `name` (id), `displayName`, `description`, `author`, `version`,
`main`, `files`, `peerDependencies` (libraries reused from the game), `dependencies` (bundled),
and `roves`: `kind` (`mod`/`dlc`), `game: {name, version}` (npm range), `mods` (required
add-ons), `steamAppId` (DLC). The cover is `cover.png`/`cover.jpg`/`cover.webp`; files in
`overrides/` replace the game's.

## `roves android`: Android APK

```bash
roves android -c dist [options]
```

Checks the prerequisites (see [Mobile prerequisites](#mobile-prerequisites)), clones the
engine at the mobile pin (`v0.7.15`, shallow, cached) unless `--engine-dir` points at an
existing checkout, optionally runs `mach bootstrap`, then `mach bundle --android`. Nothing is compiled. `rustup show` runs first when rustup is
installed (`mach bundle` itself needs no Rust).

| Option                                           | Meaning                                                                                                                                                   |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-c, --content-dir <dir>`                        | Built web game. Required.                                                                                                                                 |
| `--icon-png <file>`                              | Launcher icon (square PNG).                                                                                                                               |
| `--app-name <name>`                              | Launcher label. Default: the web manifest's `short_name`/`name`.                                                                                          |
| `--orientation <value>`                          | `any`, `natural`, `landscape`, `landscape-primary`, `landscape-secondary`, `portrait`, `portrait-primary`, `portrait-secondary`. Default: the manifest's. |
| `--theme-color <color>`                          | Status bar color, e.g. `#ffffff`. Default: the manifest's `theme_color`.                                                                                  |
| `--release`                                      | Signed release `.apk` instead of the debug one; needs all four signing values, never falls back to debug signing.                                         |
| `--keystore <file>` / `--keystore-base64 <data>` | Release keystore (file or base64, e.g. a CI secret). Env: `ROVES_ANDROID_KEYSTORE`, `ROVES_ANDROID_KEYSTORE_BASE64`.                                      |
| `--keystore-password <p>`                        | Store password. Env: `ROVES_ANDROID_KEYSTORE_PASSWORD`.                                                                                                   |
| `--key-alias <alias>`                            | Key alias. Env: `ROVES_ANDROID_KEY_ALIAS`.                                                                                                                |
| `--key-password <p>`                             | Key password. Env: `ROVES_ANDROID_KEY_PASSWORD`.                                                                                                          |
| `--no-bootstrap`                                 | Skip `mach bootstrap` (you provisioned its dependencies). On Windows bootstrap runs with `--skip-platform`.                                               |
| `-n, --name`, `-o, --out-dir`                    | Output folder `<out-dir>/<name>/` and `<out-dir>/<name>.zip`.                                                                                             |
| `--engine-dir <dir>`                             | Existing engine checkout at `v0.7.15`.                                                                                                                    |
| `--json`                                         | JSON result.                                                                                                                                              |

Signing passes the four variables the Gradle project reads: `APK_SIGNING_KEY_STORE_PATH`,
`APK_SIGNING_KEY_STORE_PASS`, `APK_SIGNING_KEY_ALIAS`, `APK_SIGNING_KEY_PASS`; a base64 keystore
is decoded to a private temporary file that is deleted afterwards. Prefer the environment
variables for secrets (command-line arguments are visible to other local processes).

```bash
roves android -c dist --app-name "My Game" --orientation landscape
ROVES_ANDROID_KEYSTORE_PASSWORD=... ROVES_ANDROID_KEY_PASSWORD=... \
  roves android -c dist --release --keystore release.jks --key-alias upload -n my-game_android
```

## `roves ios`: iOS (macOS hosts only)

```bash
roves ios -c dist [options]
```

Default: the engine's `support/ios/bundle.py` writes a ready-to-open project, XcodeGen
generates `RovesGame.xcodeproj`, and an **unsigned iOS Simulator** `.app` is built and copied
next to it. With `--release`: `mach bundle --ios --ios-release` archives and exports a signed
`.ipa` (no raw `.xcodeproj`). Requires macOS, Xcode, XcodeGen and Python (default mode) or uv
(`--release`), see [Mobile prerequisites](#mobile-prerequisites). Icons are not supported yet
(add them in Xcode).

| Option                                                                   | Meaning                                                                                                      |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| `--app-name <name>`                                                      | Display name (default "Roves Game").                                                                         |
| `--bundle-id <id>`                                                       | Bundle identifier (default `org.roves.game`).                                                                |
| `--release`                                                              | Signed `.ipa`; needs certificate, profile and team ID.                                                       |
| `--certificate-p12 <file>` / `--certificate-p12-base64 <data>`           | Apple Distribution certificate + key. Env: `ROVES_IOS_CERTIFICATE_P12`, `ROVES_IOS_CERTIFICATE_P12_BASE64`.  |
| `--certificate-p12-password <p>`                                         | `.p12` password. Env: `ROVES_IOS_CERTIFICATE_P12_PASSWORD`.                                                  |
| `--provisioning-profile <file>` / `--provisioning-profile-base64 <data>` | Matching `.mobileprovision`. Env: `ROVES_IOS_PROVISIONING_PROFILE`, `ROVES_IOS_PROVISIONING_PROFILE_BASE64`. |
| `--team-id <id>`                                                         | Apple Developer Team ID. Env: `ROVES_IOS_TEAM_ID`.                                                           |
| `-n`, `-o`, `--engine-dir`, `--json`                                     | As for `android`.                                                                                            |

The Apple certificate must come from a real Apple Developer Program account (Apple countersigns
it); unlike an Android keystore it cannot be self-generated.

## Mobile prerequisites

The CLI does not install toolchains. `roves android`/`roves ios` detect them first and stop
with every missing item, why it was rejected and how to install it; the detected `JAVA_HOME` and
`ANDROID_SDK_ROOT`/`ANDROID_HOME` are passed only to the processes `roves` starts (the shell
environment is never changed). In GitHub Actions, the
[roves-action](https://github.com/DRincs-Productions/roves-action) installs all of them on its
runners before it calls the CLI.

| Tool | Detected from | Install hint |
| --- | --- | --- |
| JDK 17+ (Android) | `JAVA_HOME`, else `java` on PATH (its `java.home` becomes `JAVA_HOME` for mach, so an older `JAVA_HOME` is not used) | Temurin 17 from https://adoptium.net, set `JAVA_HOME` |
| Android SDK | `ANDROID_SDK_ROOT`, `ANDROID_HOME`, then Android Studio's default folder; must contain `platform-tools`, `platforms/android-37*`, `build-tools/36.0.0` | command-line tools or Android Studio, then `sdkmanager "platform-tools" "platforms;android-37" "build-tools;36.0.0"` |
| uv (Android, iOS `--release`) | `uv` on PATH | https://docs.astral.sh/uv/ (`pipx install uv`, `brew install uv`, standalone installer) |
| git | `git` on PATH (not needed with `--engine-dir`) | https://git-scm.com/downloads |
| Xcode (iOS) | `xcodebuild -version` | App Store, open once, `sudo xcode-select -s /Applications/Xcode.app` |
| XcodeGen (iOS default) | `xcodegen` on PATH | `brew install xcodegen` |
| Python (iOS default) | `python3`, else uv's managed Python | python.org or uv |

## `roves download`: fetch and cache a player

```bash
roves download [-p windows|macos|linux] [--steam | --enable-mcp] [--shell-version vX.Y.Z] [--output file.zip] [--refresh] [--no-cache] [--json]
```

Prints the path of the cached zip (or of `--output`). `--no-cache` needs `--output`. The cache
lives in the user cache directory (`%LOCALAPPDATA%\roves\Cache`, `~/Library/Caches/roves`,
`~/.cache/roves`), keyed by version and asset; override it with `ROVES_CACHE_DIR` (useful to
cache it in CI). A download is retried on network errors, fails immediately on HTTP 404, and is
cached only after its zip directory reads back.

## Shell version pinning

Each CLI release pins one engine release for the desktop player (`src/shell-version.txt`,
shown in `roves --help`), so the same CLI version always produces the same package.
`--shell-version vX.Y.Z` overrides it per run. Android/iOS always use the mobile pin `v0.7.15`.

## Standalone executable

Each roves-cli GitHub Release attaches `roves-<target-triple>[.exe]` (Windows x64, macOS arm64,
Linux x64): the same CLI with Node.js built in (Node.js Single Executable Application, about
85 MB), for machines without Node.js. Roves Packmaster ships it as its Tauri sidecar. Use it
exactly like `roves`.

## Programmatic API

```ts
import { bundleDesktop, buildAndroid, ensureShellAsset, assetName } from "@drincs/roves-cli";

const results = await bundleDesktop({
    contentDir: "dist",
    platforms: ["windows", "linux"],
    name: "my-game",
    outDir: "release",
    log: console.error,
});
console.log(results.map((r) => r.archivePath));
```

Lower-level pieces are exported too: `PlayerBase` (open and verify a player zip), `assemble`,
`resolveProjectIcon`, `setExeIcon`, `generateIco`, `generateIcns`, `ensureShellAsset`,
`androidBundleArgs`/`machCommand` and friends. All functions throw `RovesError` for expected
failures.

## Common errors

| Message                                                                       | Cause / fix                                                                                            |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `content dir ... has no index.html at its root`                               | Point `-c` at the build output folder, not the project.                                                |
| `cannot download ...: HTTP 404 (is the Roves release vX published with ...?)` | The pinned or requested release has no such asset yet; use another `--shell-version` or `--shell-zip`. |
| `--enable-mcp and --steam can't be combined`                                  | No Steam + MCP player is published.                                                                    |
| `--steam-app-id requires --steam`                                             | Add `--steam`.                                                                                         |
| `--shell-zip needs exactly one --platform`                                    | Run once per platform with its own zip.                                                                |
| `the ... player package lacks X` / `does not match ... SHA256SUMS.txt`        | The zip is not an intact Roves player of that platform/variant (wrong asset, re-zipped, modified).     |
| `the standard ... player package unexpectedly contains libsteam_api...`       | A Steam asset used without `--steam`.                                                                  |
| `chrome-sandbox lost its execute bit` / `Unix modes`                          | The Linux zip was re-created on Windows; use the original release asset.                               |
| `icon-png ...: 300x200 is not square`                                         | Explicit icons must be valid; fix the file or drop the option.                                         |
| `Android prerequisites missing: ...` / `iOS prerequisites missing: ...`       | Install each listed tool with its hint (roves-action does it on its runners).                          |
| `iOS packaging requires macOS`                                                | iOS only runs on macOS.                                                                                |
| `--release requires ...`                                                      | Provide every signing value (there is no debug-signed fallback).                                       |
