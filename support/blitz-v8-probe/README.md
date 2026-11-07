# Roves Blitz + Deno/V8 probe

Isolated Windows feasibility probe; it is not production code and is not connected to the main Cargo workspace.

## Runtime ownership

Deno `deno_core` and its V8 isolate are the only JavaScript runtime. Guest JavaScript runs through that runtime. No privileged `Deno.*` APIs are exposed. Blitz supplies HTML parsing, DOM, CSS/style resolution, layout, painting and the candidate native shell. The high-level `blitz` launcher is intentionally avoided because Roves needs to own its event loop and embedder policies.

The selected crates are the official DioxusLabs line: `blitz-dom 0.2.1`, `blitz-html 0.2.0`, `blitz-shell 0.2.2`, `blitz-paint 0.2.1`, `anyrender_vello 0.6.0`, `blitz-traits 0.2.0`, and `deno_core 0.413.0`. These versions follow the official `blitz 0.2.1` lockfile where available and are pinned in this probe's Cargo.lock. `cargo tree -i v8` reports one `v8 150.4.0` through `deno_v8 0.5.0`. The PathScale 0.6.5 graph was rejected because its transitive `nagoya 0.1.19` fails compilation on Windows with POSIX-only APIs.

## What the current executable verifies

- Creates one Deno `JsRuntime` inside a current-thread Tokio runtime.
- `src/dom.js` installs a minimal DOM surface over the Blitz document: `document.getElementById(id)` and `Node.prototype.textContent` (getter/setter). Guest code (`src/guest_test.js`) uses only that surface, not `Deno.core`.
- Three ops back it, each borrowing the shared Blitz document only for the duration of one synchronous call: `op_blitz_get_element_by_id` (fast), `op_blitz_text_content`, `op_blitz_set_text_content` (fast).
- Wrapper identity: a JS-side `Map` from Blitz node id to wrapper guarantees `document.getElementById("app") === document.getElementById("app")`, and expandos survive mutation. The node handle is a class-private field; guest code cannot construct wrappers (`Illegal constructor`).
- `getElementById` walks the public Blitz node tree in tree order comparing `id` attributes, instead of Blitz's crate-private `nodes_to_id` map or a `#id` selector (which would need CSS escaping). Missing/empty ids return `null`.
- `textContent` read uses Blitz `Node::text_content`. The setter coerces like the DOM (`null` → empty, others → `String`), updates text nodes in place, and for elements detaches all children and appends one text node when non-empty. Rust then verifies the new text in the same document.
- Parses `fixtures/layout.html`, sets a 640×360 viewport and runs Blitz's complete style/layout resolve; the root box is asserted non-empty.
- Prints one `ROVES_PROBE_RESULT` per verified capability. Event listeners, a visible window, painting, input dispatch, frame scheduling, microtasks-per-frame, Motion and wrapper GC remain unimplemented.

Negative check performed on 2026-11-07: disabling the wrapper cache makes the guest test fail with `same node must return the same wrapper`.

### Identity constraint found

Blitz stores nodes in a slab and **reuses the ids of dropped nodes**. A wrapper cache keyed by node id is therefore only sound while no referenced node is dropped. The probe's setter uses `DocumentMutator::remove_node` (detach) instead of `remove_and_drop_*`, so detached nodes stay allocated, and the cache is strong. That leaks detached nodes; production needs a GC-aware lifecycle (e.g. dropping a Blitz node only after its wrapper is collected, possibly via deno_core cppgc objects) before nodes can be freed.

The probe does not open a browser window, inspect pixels or measure frame pacing/CPU/GPU. It proves runtime-to-document reads/writes and wrapper identity only.

## Run

```powershell
cargo check --manifest-path support/blitz-v8-probe/Cargo.toml --locked
cargo run --manifest-path support/blitz-v8-probe/Cargo.toml --locked
cargo tree --manifest-path support/blitz-v8-probe/Cargo.toml -i v8
```

CI entry: `.github/workflows/blitz-probe.yml` (Windows, `cargo check --locked` and run; manually dispatchable).

## Next steps

1. Design the wrapper/node lifecycle (GC-aware freeing of Blitz nodes, see above) before adding more DOM methods; extend the surface only as concrete games need it.
2. Bind event listeners and Blitz hit-test/event dispatch, then integrate shell frames, RAF and microtasks without re-entering a borrowed `JsRuntime`.
3. Exercise a real window and paint path; capture a screenshot on a Windows CI runner.
4. Test Motion after DOM methods and frame scheduling work. Test Pixi/WebGL separately because GPU surface sharing is not established.

## Removal

Delete `support/blitz-v8-probe/` and `.github/workflows/blitz-probe.yml`; no production crate imports the probe.
