# Roves patches to blitz-paint 0.3.0-beta.4

This directory is the crates.io package `blitz-paint` 0.3.0-beta.4, unpacked unchanged
(`.cargo_vcs_info.json`: DioxusLabs/blitz commit
`92b11f1bd8ab7975411328b0406a95a09445b4e7`, `packages/blitz-paint`), with its original
`LICENSE-MIT` and `LICENSE-APACHE`. Only the Blitz/V8 probe uses it, through
`[patch.crates-io]` in `support/blitz-v8-probe/Cargo.toml`; the production engine does
not depend on it.

## Why

Change 0001 pairs with blitz-dom patch 0004 for
[Blitz #1493](https://github.com/DioxusLabs/blitz/issues/1493) (see
`../blitz-dom/ROVES_PATCHES.md`): positioned `z-index: auto`/`0` boxes are now painted by
their stacking-context root from its hoisted list, like z-indexed boxes already were.
`render_element` culled a node when its own scrollable overflow and border box missed
the clip rectangle, while `Node::hit_inner` also descends into the hoisted boxes inside
`StackingContext::hoisted_content_bbox`. A hoisted box outside its root's overflow
could therefore be hit but never drawn.

## Change

Change 0001 (`cull-with-hoisted-content`) is edited directly in this directory; Roves keeps
no separate diff file (the former
`patches/blitz-paint-0.3.0-beta.4/0001-cull-with-hoisted-content.patch` was removed on
2026-11-16, last present at commit `d7905291f71`; compare with the published crate or use
git history for the exact diff). It changes `src/render.rs`: for a node with a non-empty stacking context,
the cull extent is the union of its overflow, its border box and
`hoisted_content_bbox` (CSS px relative to the border box, scaled to device px like
`overflow`). Drawing order and offsets are unchanged; nothing is drawn that was not in
the paint tree.

Besides 0001 and 0002 below, nothing differs from the published crate
(`ROVES_PATCHES.md` is the only added file).

### 0002 `inline-opacity` (2026-11-20)

Defect: `opacity` on a non-atomic inline element never reached its glyphs. `render_element`
applies opacity only as the layer of the box it renders; the glyph runs of every inline
element are drawn by the inline root's `draw_inline_layout` -> `text::stroke_text` with a
fixed alpha of 1.0, and inline backgrounds likewise. Negative observed on Roves release
`0cdb0da9300` with `support/blitz-v8-probe/fixtures/inline-opacity`: `opacity: 0` inline 1786
glyph pixels (same as `opacity: 1`), `opacity: 0.5` inline darkest channel 0, `inline-block`
at `opacity: 0` 0 pixels; Stylo's computed values were correct. This breaks the common
per-character/per-word reveal (`<span>`s faded with `opacity`).

Change, scoped to `opacity: 0`, in `src/text.rs` and `src/render.rs`: `run_is_transparent`
walks from a glyph run's node up to (excluding) its inline root and reports whether any
element on the way has computed `opacity` exactly 0 (`display: contents` excluded: it has no
box). `ElementCx::draw_inline_layout` passes `cull_transparent = true` to
`draw_inline_backgrounds` and `stroke_text`, which skip such runs: no background, glyphs or
decoration contribution. It is the same single pass and paint order as upstream, with no
layer, no recursion and no per-group rescan; the cost is one ancestor walk per run per
paint, like the decoration stack's. Culling never depends on `LayerManager`'s per-frame
layer limit (1024), so per-character `opacity: 0` text of any length stays invisible. The
inline root's own opacity and atomic inlines (`inline-block`, their own inline root) keep
the `render_element` box path, which skips a box at opacity 0 before pushing a layer, so no
opacity is applied twice. Text inputs and list markers pass `false` (upstream behaviour).
Layout is untouched.

An earlier draft of this change painted every `opacity < 1` inline as a recursive group
layer. It was withdrawn before commit: over the layer limit a group silently painted at
alpha 1, and each group level re-walked all runs (groups x runs per frame for a
per-character stagger). Partial opacity is therefore not implemented rather than
approximated.

Model compared (Chromium `f2700724`, verified by the reviewer):
`third_party/blink/renderer/core/paint/paint_property_tree_builder.cc` builds the effect node
from `style.Opacity()` for any object that needs one, a non-atomic `LayoutInline` included,
and `DirectlyUpdateOpacity` updates that node in place for opacity-only animation changes.
The inline's content is then composited as a group under that effect. Roves has no effect
tree in blitz-paint; at opacity 0 a group is invisible as a whole, so skipping its runs is
equivalent and needs no layer. A spec-correct partial group needs isolated compositing
that does not depend on the layer budget and an inline paint pass that does not rescan
runs per group; both are out of this change's scope.

Known gaps (explicit, reported by the diagnostics below, not regressions of the published
crate, which ignored inline opacity entirely):
- A non-atomic inline with `0 < opacity < 1` still paints at alpha 1, nested partial
  opacities do not multiply, and a decoration inside such an inline is opaque. The box path
  (inline root, `inline-block`) applies partial opacity correctly. Over `LayerManager`'s
  limit, box opacity layers keep the upstream degradation (painted without opacity); box
  `opacity: 0` is culled before any layer and is unaffected.
- Only `opacity` is considered: `filter`, `mix-blend-mode`, `transform` and `clip-path` on
  non-atomic inlines are still ignored, as before.

Regression tests: `support/blitz-v8-probe/src/inline_opacity.rs` (in the `blitz-probe.yml`
gate). Gate: opacity 1 and 0 spans, `inline-block` at 0 and 0.5, inline root 0.5 with a span
at 1, frozen stagger spans at 1 and 0, against computed style and pixels with background
controls and two identical frames; 1200 per-character `opacity: 0` spans, `0` inside `0.5`
and `1` inside `0` painted after 1100 box opacity layers (past the limit) must paint
nothing, with a visible control after them; a CSS-animated stagger sampled at a fixed
timeline time must cull the span not yet started. Diagnostics
(`INLINE_OPACITY_PARTIAL_DIAGNOSTIC`): the 0.5 span, nested 0.5 x 0.5, root 0.5 x span 0.5,
underlined 0.5 span and the 0.5 stagger steps print measured vs spec darkness and fail only
if that text is culled. `fixtures/inline-opacity/check_png.py` applies the same gate and
diagnostics to a published executable. Upstream issue: [Blitz #1170](https://github.com/DioxusLabs/blitz/issues/1170). Recheck the gate once an upstream release addresses it.

## Removal

Drop this directory and its `[patch.crates-io]` entry together with blitz-dom patch
0004, as soon as a maintained Blitz release passes `support/blitz-v8-probe/src/paint_order.rs`
and `support/blitz-v8-probe/src/inline_opacity.rs` without them, bumping every blitz crate
and Stylo together. Tracked in `TODO.md` (Blitz #1493, inline opacity).
