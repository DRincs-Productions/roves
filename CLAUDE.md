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
(`tests/`, `docs/`, `.devcontainer/`, project governance files) — see that file for the full
list. Everything else (`ports/`, `components/`, `python/`, `resources/`, `support/`, build
files, and our own `.github/` and `patches/` — see below) stays trackable.

## This directory is meant to be self-contained and separable

Everything Servo-specific — patches, and the CI that tests them — lives inside `servo/`
itself (`patches/`, `.github/workflows/`), not in the parent project's own `.github/` or
repo root. The intent is that `servo/` will eventually have nothing to do with the parent
pixi-vn-react-template project and could be lifted out wholesale (e.g. pushed as its own
`BlackRam-oss/servo` repo) without leaving anything behind. Keep that in mind before adding
anything Servo-related outside this directory.

One consequence: **`servo/.github/workflows/servo-test-build.yml` does not currently run
anywhere.** GitHub Actions only auto-discovers workflows under `.github/workflows/` at an
actual repository root. Today `servo/` is just a gitignored subfolder of the parent repo —
not its own pushed repository — so this workflow is dormant until `servo/` is pushed as a
real top-level GitHub repo. See the note at the top of that file.

## CRITICAL: keep CUSTOMIZATIONS.md *and* patches/ up to date

**Every time you change a file under this `servo/` directory, in the same turn:**

1. Add or update an entry in [`CUSTOMIZATIONS.md`](./CUSTOMIZATIONS.md) — file touched, what
   changed, why (prose, for humans and for the "reapply on upgrade" workflow below).
2. Regenerate the matching patch file under `patches/servo-v<TAG>/` (one file per logical
   change, numbered `0001-`, `0002-`, ...) — a real, machine-applicable unified diff against
   the pristine upstream file for the current tag. Verify it applies cleanly to a fresh
   pristine copy before moving on (`patch -p1 --dry-run < the.patch` from a clean extraction).

Do this unprompted; don't wait to be asked. **The patch files are not optional documentation
— `.github/workflows/servo-test-build.yml` (once running — see that file for why it's
currently dormant) downloads a pristine copy of this same tag on every run and applies every
`.patch` file under `patches/servo-v<TAG>/` to it.** If a change here isn't reflected as an
up-to-date patch, that CI silently tests something other than what's actually in this
directory.

This two-file setup (prose changelog + real patches) is the entire point of vendoring
instead of forking: when the Servo version gets bumped later, the workflow is "download the
new tag's zip, extract it, read `CUSTOMIZATIONS.md` for context on intent, then try applying
each patch from `patches/` — for any that fail to apply cleanly (upstream code moved or
changed shape), manually re-derive that one change against the new source." Without an
accurate, current changelog and patch set, every future upgrade means re-deriving each
customization from scratch by re-diffing behavior against vanilla Servo — exactly the
tedious work this setup exists to avoid. A stale or incomplete `CUSTOMIZATIONS.md`/patch is
worse than an honest gap: if a change here isn't reflected there, fix it as soon as you
notice, even for changes you didn't make yourself.

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
