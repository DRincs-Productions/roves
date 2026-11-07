# Roves Blitz + Deno/V8 probe

Isolated Windows feasibility probe; it is not production code and is not connected to the main Cargo workspace.

## Runtime ownership

Deno `deno_core` and its V8 isolate are the only JavaScript runtime. Guest JavaScript runs through that runtime. No privileged `Deno.*` APIs are exposed. Blitz supplies HTML parsing, DOM, CSS/style resolution, layout, painting and the candidate native shell. The high-level `blitz` launcher is intentionally avoided because Roves needs to own its event loop and embedder policies.

The selected crates are the official DioxusLabs line, moved on 2026-11-07 from the stable 0.2 releases to `0.3.0-beta.4` (`blitz-dom`, `blitz-html`, `blitz-shell`, `blitz-paint`, `blitz-traits`) with `stylo 0.22.0`, `anyrender 0.14.0`, `anyrender_vello_cpu 0.18.0` and `deno_core 0.413.0`, all pinned exactly. The 0.2 line had three defects that 0.3.0-beta.4 already fixes (below). The port was checked by CI only (no local build, by request). `cargo tree -i v8` reports one `v8 150.4.0` through `deno_v8 0.5.0`. The PathScale 0.6.5 graph was rejected because its transitive `nagoya 0.1.19` fails compilation on Windows with POSIX-only APIs.

## What the current executable verifies

- Creates one Deno `JsRuntime` (current-thread Tokio) with `deno_webidl 0.260.0`, `deno_web 0.291.0` (the release paired with `deno_core 0.413`) and the probe extension.
- `src/dom.js` installs a minimal DOM surface over the Blitz document and evaluates to a trusted-event dispatcher that Rust keeps. Afterwards `Deno` and `__bootstrap` are deleted from the global; `src/guest_test.js` checks that the guest cannot see them.
- DOM surface: `document.getElementById/createElement/createTextNode`, `Node.nodeType/parentNode/getRootNode/textContent/appendChild/removeChild`, `Element.localName/tagName/id/getAttribute/setAttribute`. Ops (`op_blitz_*`) borrow the shared Blitz document only for one synchronous call.
- Blitz owns the only tree. Wrapper identity: a JS-side `Map` from Blitz node id to wrapper (`===`, expandos survive mutation); handles live in a `WeakMap`, wrappers are not guest-constructible.
- DOM errors use deno_web's `DOMException`: `HierarchyRequestError` (self/ancestor/document insertion), `NotFoundError` (`removeChild` of a non-child), `InvalidCharacterError` (names). `appendChild` moves an attached node (Blitz's `append_children` alone does not detach it).
- Events: `Node` extends deno_web's `EventTarget`. Its spec dispatch builds the path from `parentNode`/`getRootNode` of objects with a `nodeType`, so capture/target/bubble follow the Blitz tree with no event code of ours. Rust resolves layout, hit-tests the centre of the JS-created `#button` with `BaseDocument::hit` (text hits retarget to the parent element) and calls the dispatcher; the guest verifies `isTrusted`, `target` and the order capture → target → bubble. A guest-dispatched event is untrusted.
- Parses `fixtures/layout.html`, sets a 640×360 viewport and runs Blitz's style/layout resolve.
- Offscreen paint: `blitz_paint::paint_scene` into `anyrender_vello_cpu::VelloCpuImageRenderer` (Vello CPU raster, `anyrender_vello_cpu 0.8.1` for anyrender 0.6). The wgpu renderer (`anyrender_vello 0.6.0`) passed locally but crashed with `STATUS_ACCESS_VIOLATION` on the GPU-less Windows CI runners (stage markers showed every earlier phase passing); the CPU raster is deterministic and removes the wgpu stack (~50 crates) from the probe. Pixel checks: `#app` and the JS-created button backgrounds have their CSS colours, and both the `#app` text line and the button's inner area (inset past its UA border) contain painted text. `ROVES_PROBE_SCREENSHOT=<path>` also writes a PNG (the CI artifact). Text needs `blitz-dom`'s `system_fonts` feature: without it no font is loaded and nothing is drawn.
- Headless frame loop (`--exit-after-frames N`, default 8, paced at 60 Hz in real time): each frame runs `requestAnimationFrame` callbacks, performs a microtask checkpoint, polls the deno_core event loop once (deno_web timers) and resolves Blitz layout. `src/frame_test.js`/`src/frame_check.js` verify: one rAF chain step per frame with increasing timestamps; microtasks drained before the next frame; callbacks of one frame share a timestamp; a callback requested during a frame runs in a later one; a handle cancelled by an earlier callback is skipped; a throwing callback reaches a global `error` listener (deno_web `reportException`, global object made an `EventTarget`) without stopping the others; `setTimeout(…, 0)` fires. `element.style.x = v` / `setProperty` / `removeProperty` write Blitz inline declarations (reads are not implemented); the rAF-animated box reaches width 60 in layout and paint.
- Motion 12.38.0 (the visual-novel-template version), bundled from `js/` with esbuild: `animate(element, { x: [0, 100], opacity: [0.2, 1] }, { duration: 0.1 })` runs on its JS frameloop (no `element.animate` here, so no WAAPI path) over rAF and `performance.now`, finishes within the driven frames, and the paint shows the box translated by 100 px (Blitz layout ignores transforms; paint applies them) at full opacity. APIs it needed beyond the earlier surface: `window`, `performance` (deno_web, `setTimeOrigin` at bootstrap), `HTMLElement`/`SVGElement` classes (elements are now wrapped by namespace) and a `NodeList` interface for an `instanceof` test. A second animation (`opacity: 0.5`, no start value) reads its start through `getComputedStyle`; `src/motion_check.js` verifies the computed end value. `matchMedia` is still missing (Motion guards it). rAF timestamps come from `performance.now()`.
- `getComputedStyle(element)`: a live object; each read flushes styles only (`resolve_stylist`, no layout) and serializes Stylo's computed value (`stylo 0.8.0`, the version blitz-dom 0.2.1 uses, now a direct dependency for `PropertyId`). Longhands and custom properties work; shorthands, pseudo-elements and unstyled elements return `""`. Values are computed, not CSSOM resolved (`width: auto` stays `auto`).
- Wrapper/node lifecycle: wrappers of connected nodes are held strongly (expandos survive), wrappers of detached nodes only through a `WeakRef` (re-evaluated after each animation frame and on `appendChild`). A `FinalizationRegistry` callback tells Rust (`op_blitz_release`) when V8 collects a wrapper; Rust keeps a per-node wrapper count and frees a detached subtree with `remove_and_drop_node` once no node in it has a live wrapper. `textContent` and `removeChild` free replaced/removed subtrees immediately when nothing references them. Collections are forced from Rust with `Isolate::low_memory_notification` (no `--expose-gc`: that flag installs a non-deletable `gc` global the guest would see). `src/gc_test.js` creates 1000 unreferenced detached nodes plus a referenced detached node and a connected node with expandos; Rust forces collections, asserts the live Blitz node count returns to the starting level (±10) and `src/gc_check.js` checks the retained wrappers and expandos.
- Tree API: `childNodes`/`children` (static `NodeList` snapshots; live in browsers), first/last child, siblings, `insertBefore`, `contains`, `isConnected`, `append`/`remove`, `querySelector(All)` scoped to any node (Blitz/Stylo matching), `classList` (`DOMTokenList` over `class`), `className`, `has/removeAttribute`, `innerHTML` (html5ever provider; referenced replaced nodes survive detached) and `outerHTML`. Test: `src/dom_api_test.js`.
- Geometry and input: `getBoundingClientRect`/`offset*` force style + layout (Blitz `get_client_bounding_rect`/`offset_rect`); `innerWidth`/`innerHeight`; `matchMedia` with a small evaluator (media types, `not`, `and`, comma lists, width/height, orientation, prefers-reduced-motion/color-scheme, hover, pointer; not live); `focus()`/`blur()`/`activeElement` on Blitz focus with trusted focus/blur/focusin/focusout; `UIEvent`, `MouseEvent`, `PointerEvent`, `WheelEvent`, `KeyboardEvent`, `FocusEvent` over deno_web's `Event`. Rust sends a trusted pointerdown/mousedown/pointerup/mouseup/click sequence at the hit-tested button and keydown/keyup to the focused element. Tests: `src/events_test.js`, `src/events_check.js`.
- `ROVES_PROBE_JITLESS=1` sets V8 `--jitless` before the runtime starts, like production's `roves-v8` `jitless` feature; the CI matrix runs both modes and checks the reported mode.

Negative checks performed on 2026-11-07: disabling the wrapper cache fails with `same node must return the same wrapper`; dispatching the click with `bubbles: false` fails with `click listeners did not run in DOM order`; iterating the live rAF map instead of a snapshot fails with `rAF timestamps must increase`; skipping the microtask checkpoint and event-loop poll fails with `microtasks must drain before the next frame`; driving no frames after starting Motion fails with `Motion animation did not finish`; building without `system_fonts` fails with `the button text must be painted over its background`. The offscreen screenshot was inspected visually on the local Windows machine (text and boxes as expected); that is an offscreen render, not a window presented on screen.

`anyrender::render_to_buffer` with the GPU `VelloImageRenderer` panics in `anyrender_vello 0.6.0` (zero-length buffer); fixed in 0.15.0. The CPU renderer used now resizes correctly.

Cost of deno_web here: 15 more crates in the probe lockfile, including a second `cssparser` (0.36.0, Stylo uses 0.35.0) and `brotli`. Not measured: binary size, startup, memory.

### Option A chosen over a JS DOM library

The technical handoff left open a JS DOM library mirrored into Blitz (option B). Checked on 2026-11-07: `happy-dom 21.0.4` imports Node built-ins (`vm`, `fs`, `buffer`, `stream`, `url`), unusable in this runtime without exposing a Node layer. `linkedom 0.18.15` is pure JS, but its `dispatchEvent` is "intentionally simplified" (no real capture phase, which React's root listeners rely on) and its `MutationObserver` uses non-standard record matching, so mirroring into Blitz would need hooks into its internals, plus a second tree in memory. The probe therefore keeps Blitz as the single tree and reuses deno_web for Event/EventTarget/DOMException.

### Blitz 0.2.1 gaps found (fixed in 0.3.0-beta.4, which the probe now uses)

- `DocumentMutator::set_style_property` sets no restyle hint (`set_attribute` does), so inline style writes may never be restyled. On 0.2.1 the probe set `RestyleHint::RESTYLE_STYLE_ATTRIBUTE` itself; 0.3.0-beta.4 does it inside Blitz, so the workaround is gone. Negative check: without the hint, `computed style must be live and flush pending writes` fails.
- `blitz-paint` draws an element's own background and border before its opacity layer, so `opacity` only fades children and text. On 0.2.1 this was reported as a known gap; on 0.3 the probe asserts the faded pixel. 0.3.0-beta.4 draws the background inside the layer.
- `anyrender_vello 0.6.0` `render_to_vec` (see above; the probe no longer uses the GPU renderer).

These argue for moving the probe to the Blitz 0.3 line once its betas are usable here; that changes Stylo/anyrender versions and needs its own check.

### Identity constraint found

Update for Blitz 0.3: `NodeId` is versioned (slot index + generation), so a handle held by a wrapper resolves to `None` once its node is freed instead of aliasing a new node. The probe passes handles to JS as numbers and still detaches instead of freeing; the remaining work is to free nodes when wrappers are collected.

Original 0.2 finding:

Blitz stores nodes in a slab and **reuses the ids of dropped nodes**. A wrapper cache keyed by node id is therefore only sound while no referenced node is dropped. The probe's setter uses `DocumentMutator::remove_node` (detach) instead of `remove_and_drop_*`, so detached nodes stay allocated, and the cache is strong. That leaks detached nodes; production needs a GC-aware lifecycle (e.g. dropping a Blitz node only after its wrapper is collected, possibly via deno_core cppgc objects) before nodes can be freed.

The probe does not open a browser window, inspect pixels or measure frame pacing/CPU/GPU. It proves runtime-to-document reads/writes and wrapper identity only.

## Run

```powershell
npm ci --prefix support/blitz-v8-probe/js
npm run build --prefix support/blitz-v8-probe/js   # js/dist/motion-scenario.js, read by include_str!
cargo check --manifest-path support/blitz-v8-probe/Cargo.toml --locked
cargo run --manifest-path support/blitz-v8-probe/Cargo.toml --locked
cargo tree --manifest-path support/blitz-v8-probe/Cargo.toml -i v8
```

CI entry: `.github/workflows/blitz-probe.yml` (Windows, `cargo check --locked` and run; manually dispatchable).

## Next steps

1. Design the wrapper/node lifecycle (GC-aware freeing of Blitz nodes, see above) before adding many more DOM methods; extend the surface only as concrete games need it.
2. Open a real `blitz-shell` window and drive the existing frame step (rAF, microtasks, timers, layout, paint) from its redraw/vsync instead of the headless timer, and route real pointer/keyboard input through the trusted-event dispatcher.
3. Exercise a real window and paint path; capture a screenshot on a Windows CI runner.
4. Motion with values read from the page (`getComputedStyle`), springs/gestures, and React-driven Motion from the template. Test Pixi/WebGL separately because GPU surface sharing is not established.

## Removal

Delete `support/blitz-v8-probe/` and `.github/workflows/blitz-probe.yml`; no production crate imports the probe.
