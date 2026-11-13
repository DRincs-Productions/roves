# Roves patches to blitz-paint 0.3.0-beta.4

This directory is the crates.io package `blitz-paint` 0.3.0-beta.4, unpacked unchanged
(`.cargo_vcs_info.json`: DioxusLabs/blitz commit
`92b11f1bd8ab7975411328b0406a95a09445b4e7`, `packages/blitz-paint`), with its original
`LICENSE-MIT` and `LICENSE-APACHE`. Only the Blitz/V8 probe uses it, through
`[patch.crates-io]` in `support/blitz-v8-probe/Cargo.toml`; the production engine does
not depend on it.

## Why

It pairs with blitz-dom patch 0004 for
[Blitz #1493](https://github.com/DioxusLabs/blitz/issues/1493) (see
`../blitz-dom/ROVES_PATCHES.md`): positioned `z-index: auto`/`0` boxes are now painted by
their stacking-context root from its hoisted list, like z-indexed boxes already were.
`render_element` culled a node when its own scrollable overflow and border box missed
the clip rectangle, while `Node::hit_inner` also descends into the hoisted boxes inside
`StackingContext::hoisted_content_bbox`. A hoisted box outside its root's overflow
could therefore be hit but never drawn.

## Patch

`patches/blitz-paint-0.3.0-beta.4/0001-cull-with-hoisted-content.patch` (relative to
this directory) changes `src/render.rs`: for a node with a non-empty stacking context,
the cull extent is the union of its overflow, its border box and
`hoisted_content_bbox` (CSS px relative to the border box, scaled to device px like
`overflow`). Drawing order and offsets are unchanged; nothing is drawn that was not in
the paint tree.

Nothing else differs from the published crate (`ROVES_PATCHES.md` is the only added
file).

## Removal

Drop this directory and its `[patch.crates-io]` entry together with blitz-dom patch
0004, as soon as a maintained Blitz release passes `support/blitz-v8-probe/src/paint_order.rs`
without them, bumping every blitz crate and Stylo together. Tracked in `TODO.md`
(Blitz #1493).
