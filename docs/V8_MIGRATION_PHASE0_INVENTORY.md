# V8 migration — Phase 0 inventory

Produced 2026-09-29, read-only research per `docs/V8_MIGRATION.md`'s Phase 0. No code was
changed to produce this. Grep-based counts are approximate (file-level, not line-level dedup)
and meant as a starting map for implementers, not a final audit — re-verify before relying on a
specific number in a later phase, since the codebase will keep moving.

## 1. Build configuration (Cargo dependencies/features)

Direct SpiderMonkey dependency chain, root to leaf:

| Crate | What it pulls in |
| --- | --- |
| `Cargo.toml` (workspace root) | `js` = `mozjs 0.21.6`; `jstraceable_derive` (path dep, `components/jstraceable_derive`) |
| `components/script/Cargo.toml` | `js`, `jstraceable_derive` directly; defines features `debugmozjs`, `jitspew`, `js_jit`, `profilemozjs`, `crown` (all forwarding to `js/...`), plus standalone `js_backtrace` |
| `components/script_bindings/Cargo.toml` | `js`, `jstraceable_derive` directly |
| `components/script_webgpu/Cargo.toml` | `js`, `jstraceable_derive` directly |
| `components/servo/Cargo.toml` | forwards the same four flags down the chain |
| `ports/servoshell/Cargo.toml` | forwards the same four flags; **`js_jit` is in `default` features** — i.e. JIT SpiderMonkey ships by default today |
| `ffi/capi/Cargo.toml` | also enables `servo/js_jit` (not part of the original 6-file list from the plan doc — found during this inventory, worth tracking separately) |

Cargo features/cfgs that are SpiderMonkey-specific: `js/debugmozjs`, `js/jitspew`, `js/jit`,
`js/profilemozjs`, `js/crown` (all `mozjs` crate features forwarded through
`script` → `servo` → `servoshell`/`ffi/capi`), plus the standalone `js_backtrace` feature and
`cfg(crown)` (Servo's own custom DOM-safety lint tool, reads SpiderMonkey-era invariants).
Direct `#[cfg(feature = "...")]` usage in Rust source itself was minimal in this pass (1 grep
hit) — most gating happens via Cargo dependency/module inclusion rather than inline `cfg`, so a
`cargo metadata`-based pass (not just grep) would be more accurate before Phase 6's removal step.

## 2. Rooting/GC (highest-risk area per the plan)

Approximate counts, file-level:

- `#[derive(JSTraceable...)]`: 133 files in `components/script`, 6 in `components/script_bindings`,
  2 in `components/script_webgpu`.
- `Rooted<`/`HandleValue`/`HandleObject`/`MutableHandle`: ~1848 matching lines total (342 files
  in script, 19 in script_bindings, 2 in script_webgpu).
- `JSContext`: 739 files with at least one match (workspace-wide).
- `Heap<`: ~125 matches (61 files in script, 8 in script_bindings, 0 in script_webgpu).
- `JSObject`: 82 files.
- `ArrayBuffer`/`TypedArray`: 29 files.
- Manual tracing escape hatches (`trace_in_no_trace_scope`, `unsafe_no_jsmanaged_fields`): 9
  files — these are exactly the places where someone already had to reason manually about
  SpiderMonkey GC safety; read these first before designing V8's ownership model (Phase 3).

Key files to read before designing V8 ownership (Phase 3), in rough reading order:
`components/script/dom/bindings/trace.rs`, `.../dom/bindings/root.rs`,
`.../dom/bindings/reflector.rs`, `.../dom/bindings/conversions.rs`,
`components/script_bindings/callback.rs`, `.../dom/bindings/buffer_source.rs`,
`.../dom/promise.rs`, `.../dom/bindings/error.rs`, and
`components/script_bindings/{script_runtime,realms,init}.rs`.

No existing GC stress-test harness was found anywhere in the repo. Phase 3's own requirement
("stress tests for object creation/destruction, wrapper identity, cycles, weak references and
repeated GC") starts from zero — there is nothing to adapt, it has to be written new.

## 3. WebIDL generation (Phase 4 target)

- `components/script_bindings/codegen/codegen.py` (~9,549 lines) is the primary codegen driver
  and the main Phase 4 target. ~125 lines hard-code JSAPI type strings directly (e.g.
  `HandleValue::from_raw`, `*mut JSObject`, `&mut JSContext`,
  `js::jsapi::JS_GlobalObjectTraceHook`) — these are the parts that need a V8-targeting
  rewrite, not the whole file.
- Supporting files: `configuration.py`, `run.py`, `Bindings.conf`, and HTML doc templates in the
  same codegen directory.
- Driven at build time by `components/script_bindings/build.rs`, which discovers a Python
  interpreter (uv/python3/python) and invokes the codegen script.
- 562 `.webidl` files under `webidls/` are engine-neutral (pure interface descriptions) — these
  do not need to change for a V8 migration, only what consumes them does.
- **Not inspected:** generated `Bindings/*.rs`/`ConcreteBindings/*.rs` output is not checked into
  the repo (produced fresh into `OUT_DIR` by `build.rs` on every build) — this inventory could
  not read actual generated code without running a build, which was out of scope for a read-only
  Phase 0 pass. A future implementer doing Phase 4 will need to actually build once and inspect
  `OUT_DIR` to see current generated output before designing the V8-targeting replacement.

## 4. Testing/debugging coverage that already exists

- Only `.github/workflows/test.yml` builds and tests engine-bearing crates, and per its own
  comments it's a build-smoke-test for this fork's Servo patches, not a dedicated
  SpiderMonkey/DOM regression suite.
- It runs `mach test-unit` for `servoshell`, `servo-layout`, `servo-paint-api` only — **no
  `-p script` / `-p script_bindings` unit test leg was found** in current CI.
- No WPT (Web Platform Tests) execution job exists in CI, despite the vendored
  `tests/wpt/tests/` suite being present locally (gitignored bulk content, per this repo's own
  vendoring model — see `CLAUDE.md`).
- In-repo test fixtures worth reusing for future DOM/binding validation:
  `components/script/dom/testing/*` plus the matching `TestBinding*.webidl` files, and
  `components/script_bindings/third_party/WebIDL/parser/tests/` (~80 Python parser tests,
  engine-neutral, don't depend on SpiderMonkey vs V8).

## What this means for the next phase

Phase 1 (introduce `components/roves-v8`) does not depend on resolving any of the above — it's
additive and isolated per the plan's own design. The rooting/GC inventory (section 2) is what
Phase 3 will actually need, and the codegen inventory (section 3) is what Phase 4 will need.
Both are large enough that neither should be attempted as a single change; the plan's own phase
boundaries already reflect that.

The biggest testing gap found here — no `-p script`/`script_bindings` CI leg, no WPT execution,
no GC stress harness — is worth closing *incrementally* as migration work touches each area
(per `docs/V8_MIGRATION.md`'s own Phase 5 guidance: "add or strengthen tests when a CI failure
exposes an untested contract"), not as a prerequisite blocking Phase 1 from starting.

## 2026-10-04 update - resolved Cargo graph and executable link boundary

A workspace-resolved Cargo tree (`cargo tree -i mozjs --workspace --locked` and
`cargo tree -i servo-jstraceable-derive --workspace --locked`) confirms the production
fan-out: `mozjs` is a direct dependency of `servo-script`, `servo-script-bindings`, and
`servo-script-webgpu`; these feed `servo`, which is consumed by `servoshell`, `servo-capi`,
`servo-layout`, and `servo-media-examples`. The `servo-jstraceable-derive` proc macro is directly
used by the same three script crates. The opt-in `roves-v8` crate is the only workspace consumer
of `v8`.

CP32's attempted executable test with the pilot feature proved that enabling both engines in one
binary is not a viable migration bridge on Windows: linking `servo-script-bindings` with
`v8-bindings-pilot,js/jit` failed on duplicate `v8::internal::PrintF` symbols from the V8 archive
and SpiderMonkey's `mozjs_sys` archive, and duplicate `diplomat_alloc`/`diplomat_free` symbols.
`cargo check` still passes because it does not link a binary. Continue the replacement as a
single-engine cutover: migrate/remove the SpiderMonkey-dependent binding/runtime layers before
attempting an executable Servo V8 integration test. Do not use the dual-engine pilot feature as a
production configuration.

The dependency tree also shows ICU C API `diplomat-runtime` 0.8.3 under mozjs, while V8's
`temporal_capi` uses 0.16.0; the reported duplicate symbols were between two distinct 0.16.0
artifacts at link time, so version unification alone is not established as a fix. Keep the direct
linker collision as a cutover constraint and re-evaluate it only after SpiderMonkey is removed.