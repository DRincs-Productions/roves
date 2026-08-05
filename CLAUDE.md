# Servo (vendored, customized)

This directory is a **vendored copy** of [Servo](https://github.com/servo/servo), extracted
from the official source zip for tag `v0.4.0` — not a git clone, not a GitHub fork, not a
submodule. It is the rendering engine used by the parent project's "embedded" distribution
target (see `../.github/workflows/embedded.yml`, `SERVO_TAG`), as an alternative to the
Tauri build.

## Why vendored instead of forked/submoduled

Servo's full repository is enormous (~1.3GB unpacked), almost entirely `tests/` (the Web
Platform Tests conformance mirror + perf benchmarks — irrelevant to this project). A GitHub
fork was tried first and abandoned: cloning/maintaining a fork of a repo this size for what
is meant to be a handful of small, surgical UI/behavior tweaks was overkill. Downloading the
tagged source zip and applying targeted patches locally is far cheaper to maintain.

## Local git repo

`servo/.git` is a **standalone, local-only** git repository — no remote, not connected to
`servo/servo` or any fork. It was `git init`-ed here purely so the customizations layered on
top of pristine upstream Servo can be diffed/tracked locally. `servo/.gitignore` extends
upstream's own `.gitignore` to additionally exclude upstream bulk/noise we'll never touch
(`tests/`, `docs/`, `.github/`, `.devcontainer/`, project governance files) — see that file
for the full list. Everything else (`ports/`, `components/`, `python/`, `resources/`,
`support/`, build files) stays trackable.

## CRITICAL: keep CUSTOMIZATIONS.md up to date

**Every time you change a file under this `servo/` directory, add or update an entry in
[`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) in the same turn** — file touched, what changed,
why. Do this unprompted; don't wait to be asked.

This is the entire point of the setup: when the Servo version gets bumped later, the
workflow is "download the new tag's zip, extract it, open `CUSTOMIZATIONS.md`, reapply each
listed change to the new tree." Without an accurate, current changelog, every future upgrade
means re-deriving each customization from scratch by re-diffing behavior against vanilla
Servo — exactly the tedious work this file exists to avoid. A stale or incomplete
`CUSTOMIZATIONS.md` is worse than an honest gap: if a change here isn't reflected there, note
it in `CUSTOMIZATIONS.md` as soon as you notice, even for changes you didn't make yourself.

## Upgrading to a newer Servo version (rough steps)

1. Download the new tag's source zip: `https://github.com/servo/servo/archive/refs/tags/v<NEW_VERSION>.zip`.
2. Extract it to a scratch directory (do **not** overwrite this `servo/` directory directly).
3. Read `CUSTOMIZATIONS.md` top to bottom; reapply each entry's change to the freshly
   extracted tree (the upstream file/line in question may have moved or changed shape since
   `v0.4.0` — re-verify the intent still applies, don't blindly copy-paste diffs).
4. Swap the new, patched tree in for this `servo/` directory.
5. Update `SERVO_TAG` in `../.github/workflows/embedded.yml`.
6. Build (`./mach build --release`) and manually verify each customization still behaves as
   intended (no toolbar/tabs, etc.) before considering the upgrade done.
7. Update `CUSTOMIZATIONS.md` with the new baseline version at the top.
