---
name: roves-github-action
description: Use when creating or changing a GitHub Actions workflow that packages a web game with Roves through DRincs-Productions/roves-action - release workflows on version tags, a matrix over Windows, macOS and Linux runners with standard and Steam packages, uploading the zips to a GitHub Release, MCP playtest builds, Android APK and iOS jobs with their signing secrets, the action's inputs and outputs, and diagnosing a failing roves-action step.
---

# GitHub Actions workflows with roves-action

[`DRincs-Productions/roves-action`](https://github.com/DRincs-Productions/roves-action) is a
composite action that packages an **already built** web game (a `dist/` folder with
`index.html`) with the Roves player, on GitHub's runners. It runs `roves bundle` from
`@drincs/roves-cli` (see the **roves** skill for what the package contains), and for Android/iOS
it installs the toolchains first. User documentation: the wiki page "GitHub Action"
(`content/docs/action.mdx` in https://github.com/DRincs-Productions/roves-wiki).

Use `@v1` (the CEF desktop player). `@v0` is the old Servo-based shell: do not use it for new
workflows.

## Rules

- Build the game before the action (`npm ci && npm run build`); `content-dir` points at the
  build output.
- One action run makes one package for the **runner's** platform. Use a matrix over
  `windows-latest`, `macos-latest`, `ubuntu-latest` for the three desktop packages.
- Ship the `archive-path` zip. `bundle-dir` is the unzipped folder, for tests.
- Give each job a distinct `artifact-name` (it names the zip and the folder inside it).
- `enable-mcp: 'true'` makes a development package an AI agent can drive: never upload it to a
  public release. It cannot be combined with `steam`.
- Uploading to a Release needs `permissions: contents: write`.

## Release on a version tag (the wiki example)

```yml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  create-release:
    runs-on: ubuntu-latest
    steps:
      - env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: gh release create "${{ github.ref_name }}" --repo "${{ github.repository }}" --title "My Game ${{ github.ref_name }}"

  package:
    needs: create-release
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
        steam: ['false', 'true']
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
      - run: npm ci && npm run build

      - id: roves
        uses: DRincs-Productions/roves-action@v1
        with:
          content-dir: dist
          steam: ${{ matrix.steam }}
          artifact-name: my-game_${{ runner.os }}${{ matrix.steam == 'true' && '_steam' || '' }}

      - env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: gh release upload "${{ github.ref_name }}" "${{ steps.roves.outputs.archive-path }}"
```

Drop the `steam` axis if the game is not on Steam.

## Playtest build for AI agents

```yml
- uses: DRincs-Productions/roves-action@v1
  with:
    content-dir: dist
    enable-mcp: 'true'
    artifact-name: my-game_${{ runner.os }}_mcp
```

Upload it with `actions/upload-artifact`, not to the public release.

## Inputs

| Input | Meaning |
| ----- | ------- |
| `content-dir` | Built game folder with `index.html` (required). |
| `icon-png` / `icon-ico` | Project icon (PNG 16-2048 px; `.ico` for `play.exe`). Default: `icon.png` in the build. |
| `steam` | `'true'`: Steam package (Steamworks library included). |
| `enable-mcp` | `'true'`: MCP development package (never ship). |
| `content-layout` | `archive` (default) or `files`. |
| `compression-level` | `0` (default, fastest in game) to `9` (smallest download). |
| `max-archive-size` | Largest asset archive, e.g. `300MB`. |
| `artifact-name` | Name of the zip and of the folder inside it (default `roves-game`). |
| `output` | Output folder (the package goes in its parent, named `artifact-name`). |
| `android` | `'true'`: Android WebView APK instead of a desktop package (experimental). With `android-release: 'true'` it needs `android-keystore-base64`, `android-keystore-password`, `android-key-alias`, `android-key-password` (from secrets). `android-app-name`, `android-orientation`, `android-theme-color`, `bootstrap`. |
| `ios` | `'true'` on a macOS runner: `.xcodeproj` + Simulator build, or with `ios-release: 'true'` a signed `.ipa` (`ios-certificate-p12-base64`, `ios-certificate-p12-password`, `ios-provisioning-profile-base64`, `ios-team-id`, `ios-bundle-id`, `ios-app-name`). Experimental. |

Outputs: `archive-path`, `bundle-dir`, `mcp-enabled`, `content-layout`, `icon-source`.

Keep signing material in repository secrets, never in the workflow file.

## When a step fails

- `content dir ... has no index.html`: the build did not run, or `content-dir` points at the
  wrong folder.
- The download of the player fails: transient network error or GitHub rate limit; re-run the job.
- macOS jobs cancelled or queued for long: runner capacity; a cancelled job did not produce a
  package, so re-run it.
- Android/iOS signing errors: a missing or wrong secret; the release inputs must all be set.
- For any `roves bundle` error message, the **roves** skill lists the common errors.
