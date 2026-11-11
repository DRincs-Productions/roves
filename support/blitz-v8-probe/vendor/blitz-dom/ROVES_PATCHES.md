# Roves patches to blitz-dom 0.3.0-beta.4

This directory is the crates.io package `blitz-dom` 0.3.0-beta.4, unpacked unchanged
(`.cargo_vcs_info.json`: DioxusLabs/blitz commit
`92b11f1bd8ab7975411328b0406a95a09445b4e7`, `packages/blitz-dom`), with its original
`LICENSE-MIT` and `LICENSE-APACHE`. Only the Blitz/V8 probe uses it, through
`[patch.crates-io]` in `support/blitz-v8-probe/Cargo.toml`; the production engine does
not depend on it.

## Why

`Element.getAnimations` must report Stylo's real CSS animations and transitions. The
visual-novel-template (Base UI 1.9.0 `ScrollAreaViewport`) calls it unguarded and threw
`TypeError: e.getAnimations is not a function` (CI 43027572412, story). blitz-dom keeps
the `style::animation::DocumentAnimationSet` in `BaseDocument.animations`, which is
`pub(crate)` both in 0.3.0-beta.4 and on upstream `main` (as checked when the patch was
made, 11 November 2026), and exposes no getter or snapshot.

## Patch

`patches/blitz-dom-0.3.0-beta.4/0001-read-only-animations-accessor.patch` (relative to
this directory) adds one method to `src/document.rs`:

```rust
pub fn animations(&self) -> &DocumentAnimationSet { &self.animations }
```

Read-only: the probe takes Stylo's read lock and never mutates the set. Blitz itself
already advances the state machine (`Pending -> Running -> Finished`, canceled entries
dropped) inside `resolve_stylist` (`src/stylo.rs`), so no embedder state update is
needed.

`0002-record-finished-animations.patch` (`src/stylo.rs`, `src/document.rs`,
`src/lib.rs`): Stylo's restyle, which runs in `resolve_stylist` right after Blitz's
`Running -> Finished` step, removes finished transitions from the set (only animations
are kept, for fill modes). Read through `animations()` afterwards, a transition that
completed is indistinguishable from a canceled one, so `getAnimations()` rejected its
`finished` with `AbortError` (CI 43047120469). The patch records each entry the state
machine moves to `Finished` (node, pseudo-element, kind, name, Stylo times) before the
traversal, in `BaseDocument::finished_animations` (capped at 1024, oldest dropped), and
adds `pub fn take_finished_animations(&mut self) -> Vec<FinishedAnimation>`. Blitz's own
behavior is unchanged; this is the point where Servo emits `transitionend`
(`components/script/animations.rs`, `finish_running_animations`). It covers headless
flushes and blitz-shell's own `resolve` in `--window` alike.

Nothing else differs from the published crate (`ROVES_PATCHES.md` is the only added
file).

## Removal

Drop this directory and the `[patch.crates-io]` entry as soon as a maintained blitz-dom
release exposes the animation set (or an equivalent snapshot) and the entries it
finishes (or animation events), bumping every blitz crate
and Stylo together. Tracked in `TODO.md` (Blitz probe, `Element.getAnimations`); an
upstream issue/PR proposing the accessor is still to be opened.
