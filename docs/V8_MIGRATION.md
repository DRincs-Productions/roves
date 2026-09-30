# Roves V8 migration plan

## Goal

Migrate Roves from SpiderMonkey/`mozjs` to V8 as the **only** JavaScript engine. This is a replacement, not a dual-engine architecture. Once the migration is complete, SpiderMonkey-specific dependencies, features, tracing infrastructure, build configuration, and code paths must be removed.

Roves remains a fork of Servo focused on games. The shell must remain modular. V8 is an implementation detail of the scripting/web-runtime layer and must not leak into shell, platform, renderer, networking, media, or game-service APIs.

A second goal is to keep the architecture suitable for future console ports. For that reason, a V8 JIT-less configuration is a first-class requirement from the beginning, even though desktop builds normally use V8 with JIT enabled.

## Decisions already made

- V8 completely replaces SpiderMonkey. Do not maintain two JavaScript backends.
- Add an internal workspace crate at `components/roves-v8`, package name `roves-v8`.
- `roves-v8` is internal to this repository and must not be published to crates.io.
- Use the Rust `v8`/rusty_v8 bindings as the low-level V8 integration.
- `serde_v8` may be used for suitable Roves native API value conversion, but not as a replacement for DOM/WebIDL bindings.
- Do not introduce `deno_core` initially. Roves already has Servo's web runtime, networking, storage, event loop integration, DOM, rendering and other browser infrastructure; adding another runtime layer would create overlapping responsibilities. Re-evaluate only if implementation evidence later shows that a substantial amount of useful infrastructure is being reimplemented.
- Adapt Servo's existing WebIDL/binding generation architecture to generate/use V8 bindings. Do not replace Servo's WebIDL model with hand-written bindings for a small Roves-only web subset.
- Replace SpiderMonkey tracing/rooting rather than emulating `JSTraceable` on top of V8.
- V8 JIT-less must be continuously buildable/testable as a console-oriented constraint.
- Do not introduce custom V8 startup snapshots during the initial migration. Revisit snapshots only after correctness and compatibility are established.
- Preserve the modular Roves shell.

## Hard architectural rule

V8 types must not escape the scripting boundary.

Types such as `v8::Isolate`, `v8::Context`, `v8::Local`, `v8::Global`, `v8::Value`, `v8::Object`, `v8::Function`, `v8::HandleScope`, etc. must not become part of shell/platform/public Roves APIs.

Direct V8 dependencies should be restricted to the smallest practical set of scripting crates, centered on:

```text
components/roves-v8
components/script_bindings
components/script        (only where migration temporarily requires it)
```

The target architecture is approximately:

```text
Roves shell / platform backends
            |
            v
       Roves / Servo core
            |
            +---------------- Renderer / layout / media / network / etc.
            |
            v
       Script + WebIDL
            |
            v
       components/roves-v8
            |
            v
             V8
```

The shell must not create isolates, contexts, V8 functions, or V8 objects. Platform integrations such as Steam, Google Play, Game Center, Epic Online Services and future console APIs should expose engine-neutral Rust/native contracts to the scripting binding layer.

## `components/roves-v8`

This crate should own the V8-specific runtime integration, including as appropriate:

- V8 platform initialization and shutdown policy;
- isolate creation/configuration and lifecycle;
- context/realm creation and lifecycle;
- persistent and weak handles;
- scopes and safe wrapper patterns;
- JS value/object/function/string primitives required by Servo bindings;
- exception creation, propagation and reporting;
- Promise/microtask integration;
- ES module compilation/instantiation/evaluation and module resolution hooks;
- ArrayBuffer/TypedArray integration;
- callbacks between Rust and JavaScript;
- GC/finalization hooks required for Rust DOM ownership;
- V8 flags/configuration needed by Roves;
- JIT and JIT-less configuration;
- WebAssembly integration/configuration where required;
- low-level helpers needed by generated WebIDL bindings.

Keep the crate focused. It is not a second browser runtime and should not duplicate Servo subsystems.

Mark it non-publishable (`publish = false`).

## Dependencies

### Add

- `v8` (rusty_v8) as the primary V8 Rust binding.
- `serde_v8`, where it provides a clear benefit for Roves-specific native API data exchange.

`serde_v8` should be used selectively. Prefer typed Rust structs/tuples/primitives over round-tripping through `serde_json::Value`. Do not use Serde serialization for high-frequency DOM operations or as the object identity/lifetime mechanism for DOM objects. Performance-sensitive bridges should be measured and may require direct V8 conversions.

### Do not add initially

- `deno_core`;
- `deno_runtime`;
- Node compatibility/N-API layers;
- another JavaScript runtime abstraction framework unless a concrete need is demonstrated.

### Remove by the end of the migration

At minimum, remove the SpiderMonkey-specific dependency and infrastructure represented by:

- `mozjs` / workspace `js` dependency;
- `jstraceable_derive` where it exists only for SpiderMonkey GC tracing;
- `debugmozjs`;
- `profilemozjs`;
- `jitspew`;
- SpiderMonkey-specific `js_jit` plumbing (replace with engine-neutral/V8 configuration where needed);
- direct SpiderMonkey JSAPI usage;
- SpiderMonkey rooting/tracing helpers that are no longer meaningful under V8;
- SpiderMonkey-specific build and CI configuration.

Do not mechanically delete a dependency until its responsibility has been replaced and the relevant step is buildable.

## GC and DOM ownership

This is one of the highest-risk parts of the migration.

Servo currently integrates Rust DOM objects with SpiderMonkey's GC through SpiderMonkey-specific tracing/rooting machinery. Do **not** reproduce `JSTraceable` as a cosmetic V8 wrapper.

Design ownership around V8's actual model: local handles, persistent/global handles, weak handles, finalization callbacks and V8 GC lifecycle facilities. Preserve DOM object identity and prevent both premature collection and Rust-side leaks/cycles.

Before implementing the final ownership model, inventory current uses of:

- `JSTraceable` / `jstraceable_derive`;
- `Heap`;
- rooted/handle types;
- raw `JSObject`/`JSContext`/`JS::Value` references;
- tracing hooks and DOM reflector ownership;
- weak references/finalizers;
- SpiderMonkey GC callbacks and barriers.

The migration must include stress tests for object creation/destruction, wrapper identity, cycles, weak references and repeated GC.

Do not add a third-party GC abstraction merely because one exists. A library is acceptable only if it maps cleanly onto current V8 APIs, is actively maintained, does not impose a second runtime/GC model, and measurably reduces unsafe glue. Prefer the facilities exposed by `v8` itself unless an audited library provides a concrete advantage.

## WebIDL and generated bindings

Preserve Servo's WebIDL-driven DOM architecture.

The binding generator should be adapted so generated bindings target the V8 integration instead of SpiderMonkey JSAPI. Avoid manually implementing DOM APIs one by one except as temporary experiments used to validate the runtime.

Important areas include:

- constructors and prototypes;
- interfaces and inheritance;
- methods/properties;
- overload resolution;
- dictionaries/enums/unions;
- callbacks;
- exceptions;
- Promise-returning APIs;
- iterable/maplike/setlike behavior;
- typed arrays/ArrayBuffers;
- object wrapping/unwrapping and identity;
- cross-context/realm behavior;
- generated conversions between WebIDL and Rust/V8 values.

The goal is to retain Servo's web-platform surface rather than creating a Roves-specific incompatible DOM.

## Shell modularity

The current modular shell design is a requirement and must remain intact.

V8 must sit below the Servo/Roves scripting layer. The shell should continue to deal with concerns such as windowing, SDL3, input, lifecycle, packaging and platform services through engine-neutral interfaces.

Desired direction:

```text
                  Roves core
                     |
       +-------------+-------------+
       |                           |
  Web runtime                  Platform API
       |                           |
      V8                  +--------+--------+
                          |        |        |
                       Desktop   Mobile   Console
```

A future platform backend must not need to understand V8 in order to provide controller input, filesystem paths, achievements, cloud saves, store services or lifecycle events.

## SDL3

SDL3 remains part of the Roves direction and is independent from the V8 migration. Do not undo the SDL3 migration or couple SDL3 directly to V8.

Documentation should describe Roves as a Servo fork oriented toward games that uses **V8 for JavaScript and SDL3 for the shell/platform direction** once those changes are actually present in the build. During the migration, documentation must clearly distinguish current implementation from planned architecture so it does not claim completed work prematurely.

## Console-oriented requirement: JIT-less

Desktop Roves should normally use V8 with JIT enabled for performance. However, future consoles or other restricted platforms may prohibit dynamically generated executable memory.

Therefore JIT-less is a first-class supported configuration.

Add CI coverage for a JIT-less configuration as early as practical. A Linux JIT-less CI build/test is not a console certification test; it is a guardrail that prevents Roves from accidentally assuming that runtime code generation is always available.

Do not allow Roves APIs outside `roves-v8` to depend on JIT availability.

Explicitly test JavaScript-heavy games under JIT-less because performance characteristics can differ substantially from normal desktop V8.

WebAssembly must also be tested separately under the selected JIT-less V8 configuration; do not assume that desktop JIT behavior, available Wasm features, signal/trap handling or performance automatically carry over to restricted platforms.

## Build strategy

No local build is assumed as part of this migration workflow. GitHub CI is the build/test authority.

Work incrementally. Once a migration step reaches a coherent **buildable** state:

1. commit it;
2. push it;
3. observe all relevant CI jobs;
4. fix failures before expanding the migration further;
5. add or strengthen tests when a CI failure exposes an untested contract or when a newly migrated subsystem needs regression coverage.

Do not accumulate a huge unvalidated rewrite if a smaller buildable checkpoint is possible.

Conversely, do not keep SpiderMonkey as a permanent fallback merely to make intermediate architecture easier. Temporary migration code is acceptable only while actively replacing it; the final state is V8-only.

## Suggested migration sequence

### Phase 0 — inventory and baseline

- Inventory every direct and indirect SpiderMonkey-specific usage in `script`, `script_bindings`, generated bindings, DOM support crates and build scripts.
- Classify each use as runtime lifecycle, value conversion, rooting/GC, DOM reflector, callback, Promise, module, ArrayBuffer/TypedArray, exception, WebIDL generation, testing/debugging or build configuration.
- Record existing relevant CI/test coverage before changing semantics.
- Identify all SpiderMonkey-specific Cargo features and cfgs.

### Phase 1 — introduce `components/roves-v8`

- Add the non-published workspace crate.
- Integrate `v8`.
- Establish V8 initialization and isolate/context lifecycle.
- Add minimal smoke tests for executing ECMAScript and reporting exceptions.
- Establish normal and JIT-less build configurations.
- Keep V8 implementation types contained inside the scripting boundary.

Checkpoint: CI builds the new crate and its smoke tests without changing the production script engine yet.

**Status (2026-09-29): done, verified locally.** `components/roves-v8` exists with a minimal
`Runtime` (isolate + one-shot context per `eval` call), 5 passing unit tests (basic expression,
string concatenation, thrown exception message, syntax error, and a second `Runtime` after the
first is dropped). `v8` 152.2.0 pinned as the dependency; `jitless` is a Cargo feature that
applies `--jitless` before platform init (not yet exercised by a dedicated test/CI leg — that's
follow-up work, not done here). No `v8::*` type appears in the crate's public API (`Runtime`,
`ScriptResult = Result<String, String>`).

Two real findings from writing this, worth knowing before touching this crate again:

- **This `v8` crate version's scopes are pinned, not plain references.** `HandleScope::new`/
  `TryCatch::new` produce a `!Unpin` value that must be pinned in place before use — calling
  them directly and taking `&mut` of the result (the classic older rusty_v8 pattern) does not
  compile. Use the `v8::scope!`/`v8::tc_scope!` macros (see this crate's own `src/scope.rs`);
  they handle the `Pin`/`.init()` dance and bind a `&mut PinnedRef<...>` you can pass around
  normally. `ContextScope::new` does not need this treatment itself, but its first argument must
  already be one of these pinned handles.
- **Only one V8 isolate may be "entered" per OS thread at a time** (see the `v8` crate's own
  `isolate.rs` doc comment on `OwnedIsolate`) — sharing one across threads, or holding more than
  one alive on a single thread, needs the `Locker`/`Unlocker` API, which this phase does not use.
  This was found by a test that created two `Runtime`s (two isolates) alive simultaneously on
  one thread and crashed the whole test process with a fatal, unrecoverable V8 error (not a
  catchable panic) — not a flake, and not specific to parallel `cargo test`: it reproduced
  identically under `--test-threads=1`. Fixed by making that test sequential (create, use, drop,
  create the next). This is not a Phase 1 limitation to route around later — it matches Roves's
  actual production model (one game, one isolate, one process) — but any future API on
  `Runtime`/a successor type must not assume isolates are freely shareable across threads
  without explicit locking.

### Phase 2 — fundamental value/conversion layer

Implement the primitives needed by generated bindings:

- strings;
- numbers/booleans/null/undefined;
- objects/functions;
- persistent/weak references;
- exceptions;
- callbacks;
- ArrayBuffer/TypedArray;
- Promise/microtasks;
- module primitives.

Add `serde_v8` for appropriate Roves-native data paths, not as the DOM binding implementation.

**Status (2026-09-30): partially done, verified locally — first checkpoint.** Done so far, all
in `components/roves-v8/src/lib.rs`, all covered by unit tests (10/10 passing via
`cargo test -p roves-v8`):

- **Primitive value conversion.** A public `Value` enum (`Undefined`/`Null`/`Bool`/`Number`/
  `String`/`Bytes`/`Object`) plus `Runtime::eval_value` (like `eval`, but returns `Value` instead
  of a debug string). `Object` is deliberately a one-way read-only marker in this phase — no
  structural property access yet, that's Phase 4's WebIDL-bindings job.
- **ArrayBuffer/TypedArray**, scoped to the one case Phase 2 needs: `Value::Bytes(Vec<u8>)`
  round-trips through a JS `Uint8Array`.
- **Persistent references.** A `Handle` type backed by `v8::Global<v8::Value>` (isolate-scoped,
  not context-scoped), with `Runtime::store`/`Runtime::load`.
- **Callbacks**, restricted to non-capturing function pointers: `Runtime::define_native_function`
  registers a `fn(&[Value]) -> Value` as a JS-callable global function, using V8's `External`-data
  mechanism to smuggle the pointer through `Function::builder(...).data(...)` (this `v8` crate
  version's `Function::new`/`builder` require the callback closure itself to carry no captured
  state — see `NativeFunction`'s own doc comment in the crate).
- **Exceptions**: already covered by Phase 1's `eval`/`tc_scope!` pattern; `eval_value` reuses it.
- **Promise/microtasks** (second checkpoint, 2026-09-30): `Runtime::eval_resolved` — like
  `eval_value`, but if the result is a `Promise`, drives `Isolate::perform_microtask_checkpoint`
  in a bounded loop (10,000 checkpoints) until it settles, returning the resolved value or the
  rejection's message. Pumping happens outside any live handle/context scope, since that call
  needs `&mut Isolate` directly and a scope already borrows it — each loop iteration opens and
  closes its own short-lived scope just to read the promise's current state. 4 tests cover
  pass-through, an already-settled promise, a chained `.then` that needs an actual pump (not
  just a state read), and a rejection.
- **Module primitives** (third checkpoint, 2026-09-30): `Runtime::eval_module` compiles,
  instantiates and evaluates a self-contained ES module (no import support — resolution is a
  never-called Rust fn matching the plain 4-arg logical signature the `v8` crate's own doc
  comment describes, not a raw platform-ABI `extern "C" fn`, which `MapFnTo` rejects outright).
  A module's `ScriptOrigin` needs `is_module: true` or V8 fatally aborts the process — not a
  catchable error like everything else in this crate. Module evaluation always produces a
  `Promise` (spec top-level-await semantics), so this reuses `eval_resolved`'s pump loop. 3
  tests: a module whose top-level assigns a global (checked via a later `eval_value`), a thrown
  exception, a syntax error.

**Phase 2 is complete** as of the third checkpoint above — every item in this section's own
checklist (strings; numbers/booleans/null/undefined; objects/functions; persistent/weak
references; exceptions; callbacks; ArrayBuffer/TypedArray; Promise/microtasks; module
primitives) has at least the coverage this phase needs. One caveat worth remembering before
Phase 3: "objects/functions" and "persistent/weak references" are covered only at the level
`Value::Object` (a one-way read-only marker) and `Handle`/`v8::Global` (strong references) need —
full structural property access and true weak-handle semantics are Phase 3's GC/DOM ownership
job, not something this phase left half-done.

Two real findings from this checkpoint, worth knowing before extending this crate further:

- **`Runtime` needs exactly one persistent context for its whole lifetime, not a throwaway one
  per method call.** The first version of `eval_value`/`store`/`load`/`define_native_function`
  each created its own brand-new `v8::Context` internally (copying Phase 1's `eval`, which does
  the same thing safely because it never needs to share state with another call). That's wrong
  once one method's job is to make something visible to a *later, separate* call:
  `define_native_function("double", ...)` installed the function onto one throwaway context's
  global object, which was already gone by the time a later `eval("double(21)")` created its own
  new, unrelated context — caught by a real test failure (`ReferenceError: double is not
  defined`), not a hunch. Fixed by giving `Runtime` a `context: v8::Global<v8::Context>` field,
  created once in `Runtime::new()`, entered via `v8::Local::new(scope, &self.context)` by every
  method. This matches how a real JS realm actually works (one global object for its whole
  lifetime) and is what Roves's production usage needs anyway — not a Phase-2-only workaround.
- **`FunctionCallbackArguments::data()` returns a plain `Local<'_, Value>` directly, not
  `Option<Local<'_, Value>>`** — easy to guess wrong (and did, on the first pass) since "was any
  data provided" reads like an `Option` question; it isn't one in this crate version.

### Phase 3 — GC/DOM ownership

- Replace SpiderMonkey rooting/tracing assumptions with V8 ownership.
- Replace/remove `JSTraceable` infrastructure as responsibilities move.
- Implement wrapper identity and finalization.
- Add GC stress/regression tests.

Do not proceed on the assumption that a successful simple script proves DOM lifetime correctness.

**Status (2026-09-30): first checkpoint done, verified locally and in CI.**
`components/roves-v8/src/lib.rs` gained `Runtime::create_wrapped<T: 'static>(value: T) -> Handle`:
reflects a Rust value into a fresh JS object via an internal field pointer, with a `v8::Weak` +
*guaranteed* finalizer that drops it exactly once, when V8 collects the wrapper — this phase's
actual ownership primitive, addressing the warning right above ("do not proceed on the
assumption that a successful simple script proves DOM lifetime correctness") with real GC stress
tests rather than script-execution tests: a value staying alive while its `Handle` exists, a
value dropping exactly once after its `Handle` is dropped and GC is forced, a 200-object stress
run verifying every one is collected exactly once, and two differently-typed wrapped objects
coexisting without corrupting each other's finalizer. `Runtime::force_full_gc_for_testing`
(test-only, gated behind `--expose-gc`, itself `#[cfg(test)]`-only for its documented perf cost)
makes this deterministic enough to test instead of hoping GC happens to run within a test's short
lifetime. 21/21 tests pass locally (`cargo test -p roves-v8`) and in CI.

Two real findings, worth knowing before extending this further: an ambiguous `.into()` call
needs an explicit `Local<Value>` type annotation when multiple `Handle`-implementing target types
are possible; and V8's sandboxed external-pointer table only accepts a narrow range of
internal-field tag values — an arbitrary one (`0xC0DE`) fatally aborts the whole process
(`ToExternalPointerTag: the provided tag is outside the allowed range`), not a catchable error —
`0` (matching the `v8` crate's own test suite) works.

**Second checkpoint (2026-09-30): typed JS-to-Rust read-back.** `Runtime::get_wrapped<T>(&Handle)
-> Option<&T>` reads back a `create_wrapped`-created value, now stored as a `Box<dyn
std::any::Any>` (double-boxed so an internal field, which can only hold a thin pointer, can hold
it) instead of a raw `Box<T>` — `get_wrapped` checks the requested type against the actual one
via `Any::downcast_ref`, returning `None` on a mismatch instead of a memory-unsafe blind cast.
The returned reference's lifetime is tied to the `&Handle` argument via an explicit unsafe
re-borrow, sound because a live `Handle` is exactly what guarantees the guaranteed finalizer
hasn't freed the data yet. 4 tests: correct read-back, a rejected type mismatch, `None` for a
non-wrapped handle, two distinct wrapped objects staying distinct. 25/25 tests pass.

**Not yet done — this phase's checklist has more left:**

- **SpiderMonkey rooting/tracing assumptions in `components/script`/`components/script_bindings`
  themselves haven't been touched at all.** Both checkpoints so far are purely additive within
  `roves-v8` — the real, much larger task this phase is ultimately about (replacing `JSTraceable`
  and friends across the ~133+6+2 files `docs/V8_MIGRATION_PHASE0_INVENTORY.md` counted) hasn't
  started. These checkpoints validate the *ownership primitives* those files would eventually be
  rewritten to use, not the rewrite itself.
- **Wrapper identity** ("the same Rust object always yields the same JS wrapper") now has its
  JS-to-Rust read-back half (the second checkpoint above), but nothing yet ensures wrapping the
  same conceptual object twice reuses the first wrapper instead of creating a second, independent
  one — that half is still open.
- **Cycles between wrapped objects: now tested (third checkpoint, 2026-09-30).**
  `Runtime::link(on, property, other)` sets a plain JS property linking two `Handle`s —
  deliberately not an extra Rust-side `v8::Global` cross-reference, since V8's own tracing GC
  already collects cycles among ordinary JS object graphs correctly (the entire point of tracing
  over reference counting); an extra `Global` on the Rust side is the only way this model could
  still leak a cycle. A stress test links two `create_wrapped` objects to each other in both
  directions, drops both external `Handle`s, and confirms both sides get collected once GC runs
  — not leaked, not double-collected. 26/26 tests pass. This was a real risk worth actually
  testing, not assuming away: a naive design keeping cross-references as extra strong `Global`s
  would have leaked this exact shape forever.
- **`JSTraceable`/`jstraceable_derive` removal itself** — not attempted; still present and doing
  its job for the current SpiderMonkey-based production path, unaffected by any of this checkpoint.

### Phase 4 — WebIDL generator and DOM bindings

- Adapt generated bindings to V8.
- Migrate interfaces incrementally by dependency groups.
- Preserve Servo WebIDL semantics.
- Remove direct JSAPI use as each area moves.

Suggested validation progression:

1. basic ECMAScript execution;
2. global/window exposure;
3. `document` and basic DOM objects;
4. element creation/properties/events;
5. callbacks and exceptions;
6. Promise APIs;
7. modules/dynamic import;
8. fetch/storage;
9. Canvas;
10. WebGL;
11. WebGPU;
12. audio/media;
13. Workers;
14. WebAssembly.

### Phase 5 — real game validation

Run representative Roves games/workloads, including at minimum:

- a PixiJS game/test;
- a Three.js game/test;
- ES modules and dynamic imports;
- gamepad/input;
- audio;
- save/storage paths;
- networking used by games;
- WebGL and WebGPU where supported;
- a Wasm workload;
- Roves native/platform API bridges.

Run meaningful JavaScript-heavy coverage both with normal V8 and JIT-less V8.

### Phase 6 — delete SpiderMonkey

Only after V8 provides the required production path:

- remove `mozjs`;
- remove SpiderMonkey-specific generated binding paths;
- remove `jstraceable_derive` where no longer needed;
- remove SpiderMonkey feature flags and build scripts;
- remove dead compatibility code;
- remove SpiderMonkey CI/tooling;
- update lockfiles and dependency review documentation;
- search the repository for residual `mozjs`, `SpiderMonkey`, `JSContext`, `JSObject`, `JS::`, old rooting/tracing types and obsolete feature names.

Any remaining reference should either be intentionally historical/documentary or treated as unfinished migration work.

### Phase 7 — documentation and ecosystem

Once V8 is the actual production engine:

- update the root `README.md` to clearly state that Roves is a Servo fork focused on games and uses V8 as its JavaScript engine and SDL3 in its shell/platform architecture;
- update the Roves wiki with the same architecture and migration outcome;
- document V8 build requirements and CI behavior;
- document JIT vs JIT-less support and why JIT-less exists;
- document the `components/roves-v8` boundary;
- update architecture/dependency documentation;
- update `CUSTOMIZATIONS.md` according to this fork's existing change-record protocol;
- remove documentation that incorrectly describes SpiderMonkey as the current engine.

Do not update documentation early in a way that presents planned V8 behavior as already shipped. During implementation, wording may say that migration is in progress.

## Compatibility expectations

Normal JavaScript game libraries such as PixiJS, Three.js, Phaser and similar libraries should depend on ECMAScript and Web APIs rather than on the identity of the JavaScript engine. Their compatibility therefore depends primarily on preserving correct ECMAScript/WebIDL/DOM/Canvas/WebGL/WebGPU/etc. behavior.

Changing the JavaScript engine does not turn Servo into Chromium: DOM, layout, CSS, rendering, media, networking and other web-platform implementations remain Roves/Servo components unless separately changed.

Code that relies on SpiderMonkey-specific/non-standard behavior must be identified and migrated.

## Performance

Do not treat the V8 migration itself as proof of a performance improvement.

Benchmark before/after using representative Roves workloads. Separate at least:

- JavaScript execution;
- JS/Rust binding overhead;
- GC behavior and pauses;
- DOM-heavy workloads;
- rendering/frame pacing;
- WebGL/WebGPU;
- memory usage;
- startup time;
- JIT versus JIT-less.

`serde_v8` is a convenience tool, not automatically the fastest bridge for every API. Avoid high-frequency allocation-heavy serialization paths when direct V8 representations are more appropriate.

## V8 build and CI considerations

V8 is a large C++ dependency with its own GN/Ninja-based build configuration. Treat its build as a first-class part of Roves infrastructure.

For ordinary supported development/CI targets, rusty_v8 prebuilt archives may be useful for iteration speed. For unusual targets, custom configurations, JIT-less validation and especially future console work, expect to need controlled source builds/custom V8 archives and target-specific GN configuration.

Pin compatible `v8` and `serde_v8` versions deliberately. Do not independently upgrade one without verifying compatibility with the other and running the full scripting/binding test set.

Cache V8 artifacts in CI where appropriate; otherwise build time can dominate Roves CI.

## Completion criteria

The migration is complete only when all of the following are true:

- production Roves uses V8 and does not link SpiderMonkey;
- `mozjs` is gone from the production dependency graph;
- SpiderMonkey-specific tracing/rooting has been replaced;
- WebIDL-generated DOM bindings target V8;
- shell modularity is preserved and shell/platform APIs expose no V8 types;
- representative DOM, Promise, module, typed-array, Worker and Wasm behavior passes;
- Canvas/WebGL/WebGPU/media paths required by Roves games are validated;
- representative PixiJS and Three.js workloads run correctly;
- Roves native/platform APIs work through engine-neutral boundaries;
- normal desktop V8 CI passes;
- JIT-less CI passes;
- GC/lifetime stress tests pass;
- obsolete SpiderMonkey features/build code are removed;
- README, wiki and architecture/dependency documentation accurately describe V8 + SDL3 Roves;
- `CUSTOMIZATIONS.md` records the completed fork changes;
- CI is green at the final migration commit.

## Guidance for Codex/implementers

Treat this document as architectural intent, not permission for a single giant rewrite.

Before each phase, inspect the current code because Servo/Roves may have changed since this plan was written. Prefer the smallest coherent buildable change. Commit buildable milestones and use CI feedback before continuing.

Do not silently change unrelated Servo behavior to make V8 integration easier. If an existing web-platform semantic must change, document why and add a regression test.

Do not expose V8 implementation details merely to bypass a difficult binding problem. Difficult ownership/binding issues belong in the scripting layer.

When uncertain about V8 APIs, use current V8/rusty_v8 behavior rather than assuming SpiderMonkey concepts map one-to-one.
