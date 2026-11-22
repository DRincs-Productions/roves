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

## Changes

The fixes are edited directly in this directory; Roves keeps no separate diff files. Get
the exact differences by comparing it with the published crate (commit above) or through
git history. The numbered `patches/blitz-dom-0.3.0-beta.4/` files that used to describe
them were removed on 2026-11-16 (last present at commit `d7905291f71`); the numbers below
are kept as names for the individual changes.

0001 (`read-only-animations-accessor`) adds one method to `src/document.rs`:

```rust
pub fn animations(&self) -> &DocumentAnimationSet { &self.animations }
```

Read-only: the probe takes Stylo's read lock and never mutates the set. Blitz itself
already advances the state machine (`Pending -> Running -> Finished`, canceled entries
dropped) inside `resolve_stylist` (`src/stylo.rs`), so no embedder state update is
needed.

0002 (`record-finished-animations`) (`src/stylo.rs`, `src/document.rs`,
`src/lib.rs`): Stylo's restyle, which runs in `resolve_stylist` right after Blitz's
`Running -> Finished` step, removes finished transitions AND finished animations from
the set (Stylo 0.22 `matching.rs` retains only entries whose state is not `Finished`;
the computed style keeps a `forwards` fill without a set entry). Read through
`animations()` afterwards, a transition that completed is indistinguishable from a
canceled one, so `getAnimations()` rejected its `finished` with `AbortError` (CI
43047120469), and a forwards-filled animation vanished (CI 43058658768). The patch
records each entry the state machine moves to `Finished` (node, pseudo-element, kind,
name, Stylo times and, for keyframes animations, an `Arc` of the target's computed
style before the traversal, i.e. the `animation-*` declarations that produced it) in
`BaseDocument::finished_animations` (capped at 1024, oldest dropped), and adds
`pub fn take_finished_animations(&mut self) -> Vec<FinishedAnimation>` plus
`pub fn animation_target_style(node, pseudo)` (primary or eager `::before`/`::after`
style, read the same way as `flush_pseudo_elements`). The probe compares the recorded
and current computed `animation-*` longhands to keep a completed `forwards`/`both`
fill enumerable while its declaration still applies. Blitz's own
behavior is unchanged; this is the point where Servo emits `transitionend`
(`components/script/animations.rs`, `finish_running_animations`). It covers headless
flushes and blitz-shell's own `resolve` in `--window` alike.

0003 (`input-type-state`) (`src/layout/construct.rs`): Blitz created the
`TextInputData` editor only when an `input`'s raw `type` attribute was missing or one
of the literal keywords `text`, `password`, `email`, `number`, `search`, `tel`, `url`
(and the checkbox state only for literal `checkbox`/`radio`). HTML's `type` is ASCII
case-insensitive and its invalid value default is the Text state, so the
visual-novel-template's name prompt (`<input type="string">`, from Pixi'VN
`requestInput({ type: "string" })`) and `type="TEXT"` got no editor and ignored typing.
The patch maps the attribute to its type state (lowercased keyword, otherwise `text`)
before the same checks. A fix of a game-blocking library defect, not a behavior change
for valid lowercase values.

0004 (`positioned-paint-layer-tree-order`) (`src/layout/paint_tree.rs`,
`src/node/node.rs`), for [Blitz #1493](https://github.com/DioxusLabs/blitz/issues/1493):
the paint tree kept a positioned `z-index: auto` box together with its whole subtree in
its parent's list, so a later positioned box nested in it (or in a static box) painted
and hit-tested below an earlier `position: fixed; z-index: 0|auto` sibling hoisted to
the root. In the visual-novel-template the Pixi'VN HTML layer (`absolute`) holds
`NarrationClickOverlay` (`fixed inset-0 z-0`) and then `NarrationScreen` (`absolute`,
with the Save/Load button); clicks on the button advanced the narration instead. CSS 2.1
Appendix E paints all positioned descendants with z-index auto/0 at the stacking
context's step 8, in tree order. The patch lifts such boxes (absolute/fixed, and
relative outside flex/grid containers) into the enclosing stacking context's hoisted
list with z-index 0, and sorts that list by (z-index, tree order) through DOM parents,
because entries are collected per subtree and per containing block. Paint
(`blitz-paint` `draw_children`) and hit testing (`Node::hit_inner`) already traverse
the list in matching opposite orders with the same `HoistedPaintChild::position`.
Lifting stops where it would drop a clip or change offsets: at boxes that clip overflow
(as blitz-paint decides, the root element excepted), `contain: paint`, a CSS `clip`,
images/sub-documents/text inputs, inline roots, sticky boxes and relative flex/grid
items (whose subtrees keep the previous placement). `Node::paint_lifts_positioned`
records the decision so a clean subtree is only replayed when it is unchanged. The
related upstream PR [#1421](https://github.com/DioxusLabs/blitz/pull/1421) covers
explicit z-index stacking only. Regression: `support/blitz-v8-probe/src/paint_order.rs`
(A control passes before and after; B `z-index: 0` and C `z-index: auto` fail with the
unpatched crates and pass with this patch plus blitz-paint 0001; an overflow clip
check passes in both).

0005 (`detach-boxes-leaving-the-box-tree`) (`src/resolve.rs`): a game-blocking
defect of the published crate. `resolve_layout_children` only visits boxes reachable
from the root's `layout_children`. When a container's reconstruction dropped a child
box that still existed (a `display: none` ancestor, a node detached from the DOM, a box
that became inline content), that child kept its own `layout_children` and nobody
visited it again. Freeing one of its children (the probe frees unreferenced detached
nodes at once, `drop_if_unreferenced`) left a freed slab key in that list, so the
crate's own debug assertion `assert_layout_parents_consistent` (unchanged) aborted the
next `resolve` with "layout child NodeId(..) of NodeId(..) is not in the slab". In the
visual-novel-template this happened after Skip and the accepted name prompt
(`0xc0000409`, 14 November 2026). Before rebuilding a container, the patch records the
boxes it laid out, directly or through the anonymous blocks it frees, with their
`layout_parent` cleared; after the new children are collected and visited, a previous
box that no container adopted (or that became the inline content of an inline root)
has left the box tree and its box subtree is detached: child lists taken, links cleared
and its anonymous blocks freed. A box without `layout_children` (never built or
detached) is rebuilt in full when a container collects it again: `ALL_DAMAGE`, layout
cache invalidated and the damaged-descendant flags marked, because its own damage may
have been cleared while it was outside the box tree. No list is filtered: the
assertion and the existing resolver paths are unchanged, and only entries of a list
being discarded are allowed to be gone already. Regressions:
`support/blitz-v8-probe/src/box_tree.rs` (hide/free/show and same-frame variants abort
with the unpatched crate; the detached/reinsert control passes in both) and the
`--story-skip` template gate.

0006 (`svg-current-color-legacy-srgb`) (`src/node/serialize.rs`): inline `<svg>` is
serialized for usvg with `currentColor` replaced by the element's computed `color`,
written with Stylo's `to_css_string()`. A computed `oklch()` (or `lab()`, `color()`,
...) is serialized in that syntax, which svgtypes 0.16.2 (usvg 0.48.1) does not parse,
so usvg dropped the paint. The visual-novel-template (Tailwind v4/shadcn, every theme
color in `oklch`) showed no Lucide icon in the main menu and Settings, while fixtures
with `rgb`/hex colors painted them. The patch converts the color with Stylo's
`AbsoluteColor::into_srgb_legacy()` (gamut-clamped `rgb()`/`rgba()`, alpha kept) before
serializing. Regression: `support/blitz-v8-probe/src/svg_current_color.rs` (`rgb`
control; the same Lucide icon under `oklch(14.5% 0 0)` painted nothing with the
unpatched CI release build of `359d0cc7bec`; a translucent `oklch` case checks that
`rgba()` keeps the stroke). Upstream issue/PR still to be opened.

Nothing else differs from the published crate (`ROVES_PATCHES.md` is the only added
file).

## Removal

Drop this directory and the `[patch.crates-io]` entry as soon as a maintained blitz-dom
release exposes the animation set (or an equivalent snapshot) and the entries it
finishes (or animation events), creates the text editor for every input in the
Text state, passes `src/paint_order.rs` without patch 0004 (drop
`vendor/blitz-paint` at the same time), `src/box_tree.rs` without patch 0005 and
`src/svg_current_color.rs` without patch 0006,
bumping every blitz crate and Stylo together.
Tracked in `TODO.md` (Blitz probe, `Element.getAnimations`, Blitz #1493); an
upstream issue/PR proposing the accessor is still to be opened.
