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

## CRITICAL: keep `roves-action` in sync with CI/CD changes here

Alongside this repo, on the same machine, there are sibling checkouts of other
`DRincs-Productions` repos that depend on this engine's build/bundle surface — most
importantly `../roves-action` (`DRincs-Productions/roves-action`, a separate git repo, not a
submodule of this one). `roves-action` is a GitHub Action that runs `mach build` + `mach
bundle` against a pinned checkout of *this* repo on behalf of any third-party game's CI —
its own README says outright that it mirrors this repo's own
`.github/workflows/test.yml`. Its `action.yml` inputs are a near 1:1 forwarding layer over
`mach build`/`mach bundle`'s CLI flags (each tagged `[servo]` or `[roves]` in that file to
say which side owns it).

That mirroring is not automatic — it's a manual sync someone (you) has to maintain. **Any
change here that touches this repo's CI/CD or build/bundle surface must be reflected in
`roves-action` in the same turn**, not deferred. Concretely, this includes:

- A new, removed, renamed, or redefaulted `mach build` or `mach bundle` flag (the command
  lives in `python/servo/post_build_commands.py`; see the `## CRITICAL: keep
  CUSTOMIZATIONS.md` section above — those same changes already need a `CUSTOMIZATIONS.md`
  entry and patch).
- A new `mach bundle` output format/target (e.g. the existing `--deb`, or anything added
  after it) — `roves-action`'s `action.yml` needs a matching input, and its `README.md`
  needs a matching example/row in its input tables.
- Changes to what `.github/workflows/test.yml` actually *does* with those flags (build
  matrix entries, packaging/zipping conventions, defaults) — if `test.yml` is the reference
  implementation `roves-action` mirrors, drift here silently makes that mirror stale.

If `../roves-action` isn't present as a sibling checkout when you're making a change like
this, say so explicitly rather than silently skipping the sync — don't assume the update is
someone else's problem. `roves-api` and `roves-wiki` (also sibling repos) are not part of
this mirroring relationship and don't need this same treatment unless a change here directly
affects what they document or consume.

## Ask whether a new `mach bundle` setting belongs in `roves-ui` too

`../roves-ui` (a sibling checkout, "Roves Packmaster") is a GUI that wraps `mach bundle`'s
own options as a settings screen a game developer clicks through instead of typing flags —
see that project's own `CLAUDE.md` and `src/lib/settings.ts` for its current settings shape
(portable platforms, installer formats, Steam, content compression).

**Any time you add, remove, or change the meaning of a `mach bundle` flag or default (see
the `CUSTOMIZATIONS.md`/patches section above — those same changes already need an entry
there), ask explicitly whether it should also be reflected in `roves-ui`'s settings** —
don't assume it does or doesn't; the answer depends on whether the new option is something
a game developer using the GUI would plausibly want to control, which isn't always obvious
from this side. If `../roves-ui` isn't present as a sibling checkout, say so explicitly
rather than silently skipping the question.

## CRITICAL: every user-facing feature needs a README mention *and* a wiki page

`CUSTOMIZATIONS.md`/`patches/` (above) capture *what changed relative to upstream Servo*, for
the next version upgrade. That's a different audience from *someone using Roves right now* —
a game developer bundling their game, or a player hitting a problem — who reads
[`README.md`](./README.md) and the [`roves-wiki`](https://github.com/DRincs-Productions/roves-wiki)
docs site instead, and neither of those updates automatically just because
`CUSTOMIZATIONS.md` did.

**Any time you add, change, or remove a user-facing feature — a new `mach bundle`/`mach
build` flag, a new bundle output, a new CLI behavior, anything a game developer or player
would need to know about — in the same turn:**

1. Add at least a brief mention to `README.md` — even one sentence and a link out is enough;
   it doesn't need to duplicate the wiki's full explanation, just enough that someone skimming
   the README doesn't miss that the feature exists.
2. Add or update the matching page (or section of an existing page) under `roves-wiki`'s
   `content/docs/` — this is where the real explanation, rationale, and worked examples live.
   `roves-wiki` is a sibling checkout (see the `roves-action` section above for what "sibling
   checkout present" means in practice) — same rule applies: if it isn't present, say so
   explicitly rather than silently skipping this.

Do this unprompted, the same way `CUSTOMIZATIONS.md` gets updated unprompted — don't wait to
be asked, and don't treat "I already wrote the `CUSTOMIZATIONS.md` entry" as covering this
too. A change that's only documented in `CUSTOMIZATIONS.md` is invisible to everyone except
the next person upgrading the Servo version; a real user has no reason to ever open that file.

## Cutting a versioned release (`v<major>.<minor>.<patch>` tags)

`.github/workflows/release.yml` builds and publishes a real, versioned GitHub Release —
distinct from `.github/workflows/test.yml`'s rolling "test" release (see that file's own
comment for why it exists separately). It triggers on pushing a tag matching `v*.*.*` (e.g.
`v0.1.0`), builds directly from that tagged checkout of this repo — not a pristine-download-
plus-`patches/` reconstruction the way `test.yml` does, since this repo already tracks the
full patched source (see the top of this file) — and publishes portable + installable
bundles for Windows/macOS/Linux to that Release. The first cut, `v0.1.0`, is the engine shell
only: no bundled UI/game content (`roves-ui` content lands in a future release).

### To cut a release

1. Decide the version (semver), e.g. `0.1.0`.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0` — this alone triggers
   `release.yml`; nothing else needs to happen by hand.
3. Watch the run: `https://github.com/DRincs-Productions/roves/actions/workflows/release.yml`.
4. Once green, verify what actually got published at
   `https://github.com/DRincs-Productions/roves/releases/tag/v0.1.0` — check all 6
   platform/mode zips are attached (`windows-portable`, `windows-msi`, `macos-portable`,
   `macos-dmg`, `linux-portable`, `linux-deb`) and the notes rendered as expected. Don't
   consider the release done on "the workflow went green" alone — confirm the artifacts are
   actually there.

### If a run fails: delete-and-republish loop

GitHub won't let a tag be re-pushed over itself, and `gh release create` (used by the
workflow's own `create-release` job) errors on an existing release of that name — retrying
means tearing both down and starting clean, not just re-running the failed job:

1. Delete the GitHub Release for that tag, if one was already created — via the web UI
   ("Delete this release") or `gh release delete vX.Y.Z --yes`. (`create-release` also
   self-heals this automatically on the *next* push of the same tag — see its own `gh
   release delete ... || true` step — so this is belt-and-braces, not strictly required
   before re-pushing.)
2. Delete the tag both locally and on the remote: `git tag -d vX.Y.Z && git push origin
   --delete vX.Y.Z`.
3. Fix whatever caused the failure.
4. Re-tag and re-push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. Repeat until every one of the 6 matrix jobs is green and the release page shows all 6
   zips.

### Diagnosing a failure without `gh`/a token on hand

Raw step logs need repo-admin auth to download (the Actions API's job-logs endpoint 403s
"Must have admin rights to Repository" for an anonymous/public request) — but `::notice::`/
`::error::` **annotations** are readable anonymously via `GET /repos/DRincs-Productions/
roves/check-runs/{job_id}/annotations`. That's why the smoke-test step re-emits stdout/
stderr/`roves.log` as `::notice::` lines on a launch failure (mirroring `test.yml`'s own
reasoning, see its comments). A failure inside `mach build`/`mach bootstrap` itself (a
compile error, a GStreamer install failure) has no custom annotation, only the generic
"Process completed with exit code N" one — diagnosing those needs either `gh run view --log`
(if `gh`/a token is available) or someone with repo access checking the run's logs directly
at its `html_url`.

### Design notes worth knowing before touching this workflow

- **No `--content-dir`**: this first release is the engine shell only — see README.md's
  "Portable vs. installable packages" section for what `--package-name`/`--package-version`
  do, and revisit this once a future release adds real `roves-ui` content.
- **Real GStreamer, not `--media-stack dummy`**: unlike `test.yml`, this release ships
  working audio/video. Linux and macOS get it for free (`mach bootstrap` already installs
  GStreamer non-interactively there — apt packages on Linux, `sudo installer -pkg` on macOS
  with `--yes`). **Windows needs a workaround**: `mach bootstrap`'s own GStreamer installer
  wraps `msiexec` in `Start-Process -verb runAs` (a UAC elevation prompt) that hangs forever
  on a non-interactive GH runner — see `test.yml`'s own comment on this exact, already-
  documented finding (that workflow sidesteps it by using `--media-stack dummy` instead of
  solving it). `release.yml` installs the same two MSIs itself, directly via `msiexec /a ...
  TARGETDIR=... /qn` (no elevation prompt), to the exact path `python/servo/platform/
  windows.py`'s `DEPENDENCIES_DIR` would use, then passes `--skip-platform` to `mach
  bootstrap` so it doesn't redundantly (and hang-prone-ly) try to install it again.
- **No `--features steam`**: this is the plain default build, matching README.md's own
  documented build instructions verbatim — Steam integration stays opt-in for whoever builds
  their own game on top of this engine.

## Upgrading to a newer Servo version (rough steps)

1. Download the new tag's source zip: `https://github.com/servo/servo/archive/refs/tags/v<NEW_VERSION>.zip`.
2. Extract it to a scratch directory (do **not** overwrite this `servo/` directory directly).
3. Read `CUSTOMIZATIONS.md` top to bottom; reapply each entry's change to the freshly
   extracted tree (the upstream file/line in question may have moved or changed shape since
   `v0.4.0` — re-verify the intent still applies, don't blindly copy-paste diffs).
4. Re-reason about the default-on experimental prefs (`CUSTOMIZATIONS.md`'s "Default-on
   experimental web platform features" entry, patch `0009-default-on-experimental-web-
   platform-prefs`) — **don't just mechanically reapply that patch's literal 18 field
   names.** That list was a snapshot of one Servo version's `EXPERIMENTAL_PREFS`
   (`ports/servoshell/prefs.rs`) and `Preferences` struct (`components/config/prefs.rs`), not
   a fixed policy. The new tag's versions of both will likely differ — new prefs added,
   some removed, some graduated from experimental to stable (already `true` upstream, so
   nothing to do). For every pref that's new in either place compared to the old tag, reason
   about it the same way that `CUSTOMIZATIONS.md` entry did: would a real video game
   plausibly want this (graphics/audio/input/storage/UI capability), or is it dev-tooling,
   testing-only, or unrelated to running a game (the entry lists what was deliberately left
   off and why: WebRTC, Web Animations, Screen Wake Lock, Bluetooth, Geolocation, Credential
   Management — plausibly game-relevant but not upstream's own vetted bundle, so left as a
   judgment call rather than defaulted on)? Default the former to `true` here, leave the
   latter alone, and update that `CUSTOMIZATIONS.md` entry's field list and reasoning to
   match — don't leave it describing the previous version's set.
5. Swap the new, patched tree in for this `servo/` directory.
6. Update `SERVO_TAG` in `../.github/workflows/embedded.yml`.
7. Build (`./mach build --release`) and manually verify each customization still behaves as
   intended (no toolbar/tabs, etc.) before considering the upgrade done.
8. Update `CUSTOMIZATIONS.md` with the new baseline version at the top.
