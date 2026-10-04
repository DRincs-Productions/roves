# Roves V8 migration plan

## Status note — 2026-10-04 (CP37 in progress; Phase 3 production ownership remains open)

The opt-in WebIDL pilot now generates enum operation arguments, applies WebIDL string
conversion and rejects values outside the declared enum set before invoking native callbacks.
Optional enum arguments also use their declared defaults. Local generator, runtime (normal and
JIT-less), default-runtime, and integrated `servo-script` checks pass. Patch 0095 applies across
the complete pristine patch series; the wiki production build passes all 82 pages. CI is green:
V8 37201595906, Servo patch validation/SDL3/Steam/all six bundles 37202006567, Android
37202073963 and iOS 37202073968. This is pilot coverage only: production still uses SpiderMonkey.

CP32 adds an integration test against the actual Servo `ValidityState.webidl` and its generated
V8 binding. It checks all 11 attributes, descriptor shape, `instanceof`, and that the `valid`
getter reflects each of the ten native validation flags. Full local generator/runtime/default
tests and the integrated `servo-script` check pass; CI is green: V8 37205017801, Servo 37205017877
(patch validation, SDL3, Steam, all six bundles), Android 37205017867, and iOS 37205017982. This
remains pilot coverage; production still uses SpiderMonkey. A trial test inside
`servo-script-bindings` compiled but could not link while both engines are present: `mozjs` and V8
export duplicate C++ symbols (`v8::internal::PrintF`, plus `diplomat_alloc/free`). Therefore the
pilot binding is compile-checked through this crate, while executable WebIDL tests stay in the
isolated `roves-v8` test binary. The production cutover must remove SpiderMonkey from the linked
binary before a runtime test can exercise both through Servo's binding crate.

CP33 adds a weak wrapper-identity cache to the isolated V8 runtime. A stable native identity now
reuses its existing live JavaScript wrapper without rerunning the native factory; the cache entry
is removed by the guaranteed finalizer, and a new wrapper can be created after collection. Tests
cover `===`, exactly-once native destruction, recreation after GC, and an old finalizer racing a
replacement entry. Local verification passes: 26 generator tests; normal/JIT-less each with 66
unit + 4 WebIDL integration + 2 doctests; default with 54 unit + 2 doctests; and the integrated
`servo-script-bindings` cargo check. Patch 0097 applies in the pristine overlay validator. CI is
green: V8 37213254328, Android 37213254318, iOS 37213254280, and Servo 37213254289 (patch
validation, SDL3 probes, Steam, six bundles, Linux layout/paint-api tests and bundle smoke tests).
This remains an ownership primitive prototype, not production DOM integration.

CP34 assigns every Servo `Reflector` a monotonic, process-unique `NativeObjectId`, independent of
allocation addresses and JS engine handles. This is the native-side key needed to connect Servo
DOM objects to the weak V8 wrapper identity cache. The focused local test, opt-in bindings check,
rustfmt, and diff checks pass. Patch 0098 passes pristine-series validation. CI is green: V8
37217091986, Android 37217091942, iOS 37217092008, and Servo 37217091945 (all 13 jobs, including
Steam, SDL3, all six bundles, Linux layout/paint-api, and platform smoke tests). Wiki production
build passes all 82 pages at commit `69d7578`. Production rooting/tracing and wrapper creation are
still SpiderMonkey-backed.

CP35 connects the opt-in WebIDL generator to V8's weak native-identity cache. Generated bindings
now expose `create_with_identity`, accepting the engine-neutral ID from a DOM `Reflector` and a
lazy native wrapper factory. A generated `ValidityState` test confirms the same ID reuses the JS
wrapper and skips the factory. Generator (26 tests), normal/JIT-less/default runtime suites,
integrated binding checks, and patch 0099 pristine validation pass. CI is green: V8 37220584940,
Servo 37220584970 (13/13 jobs: patch validation, SDL3, Steam, all six bundles, Linux layout/
paint-api, and smoke tests), Android 37220610791, and iOS 37220610787. This remains pilot-only.

CP36 changes `Reflector::PartialEq` to compare `NativeObjectId`, making DOM identity engine-neutral
and available before JS wrapper creation. Both focused reflector tests pass locally with
`cargo test -p servo-script-bindings --features js/jit reflector::tests:: --offline --locked`;
rustfmt and diff checks pass, and patch 0100 reverse-checks. CI and patch-series validation are
pending. CI is green: V8 37224061095, Servo 37224061184 (13/13 jobs: pristine patch validation,
SDL3, Steam, all six bundles, Linux layout/paint-api and smoke tests), Android 37224070360, and
iOS 37224070357. Production still uses SpiderMonkey.

CP37 adds the `DomObject::native_object_id()` engine-neutral API and updates generated DOM
`PartialEq` implementations to use it. The two focused reflector tests and all three
`servo-dom-struct` macro tests pass locally; patch 0101 reverse-checks. Cross-platform CI is
pending.

The next ownership work is not a mechanical replacement of `Reflector::get_jsobject`: an audit
shows that reflector callers mix native identity/prototype queries, JS rooting, and operations
that explicitly call SpiderMonkey APIs. `Reflector` currently stores `Heap<*mut JSObject>`,
`root.rs` derives stack roots from SpiderMonkey's `Traceable`, and `trace.rs` calls
`CallObjectTracer`. The migration should first define a scripting-boundary native identity and
engine-owned wrapper/root contract, then migrate one caller category at a time. Do not add a
V8-specific handle to public DOM/shell APIs or claim production integration while `script` still
links SpiderMonkey.

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

A Windows local toolchain is available: use local builds and tests for fast iteration, then GitHub CI for cross-platform and final verification. CI does not replace local work that can be done.

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

**Ownership hardening (2026-10-01):** `get_wrapped` now borrows the runtime as well as
its handle, preventing overlapping JS mutation or isolate disposal while native data is read
(two compile-fail doctests). Completed finalizer records are reclaimed on allocation only after
callback completion; setter callback allocations are runtime-owned rather than leaked. Shared
materialization flags reject late member registration on ancestors after child instantiation.
49 unit tests plus 2 compile-fail doctests; isolated normal/JIT-less CI covers all desktop OSes.
Production tracing replacement and wrapper reuse remain open.

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

**Status (2026-09-30): first checkpoint done, verified locally and in CI.**
`Runtime::set_global_property(name, &Handle)` prototypes validation-progression milestone #2
("global/window exposure") entirely within `components/roves-v8` — a wrapped Rust struct exposed
as `globalThis.window`, with a property on it readable from real JS (`window.name`), built
entirely from Phase 2/3's existing primitives (`create_wrapped`, `link`, `eval_value`). 1 new
test, 27/27 total passing.

**Real scale finding, worth internalizing before going further:** inspecting a real local
build's generated output (`target/debug/build/servo-script-bindings-*/out/Bindings/` after a
plain `./mach build`) found **522 generated binding files** — one per WebIDL interface — and
**~700 lines even for the simplest one checked** (`ConsoleBinding.rs`). This is the concrete
number behind the plan's own warning against "a single giant rewrite": adapting `codegen.py`
wholesale in one sitting is not a realistic scope for a single change, or likely even a single
session. The approach going forward is deliberate: prototype each remaining
validation-progression milestone (`document`/basic DOM objects; element creation/properties/
events; DOM-level callbacks/exceptions — distinct from Phase 2's own script-level callbacks/
exceptions, already done; Promise APIs at the DOM level; modules/dynamic import; fetch/storage;
Canvas; WebGL; WebGPU; audio/media; Workers; WebAssembly) as a temporary, `roves-v8`-only
experiment first (explicitly sanctioned by this document: "manually implementing DOM APIs one by
one... as temporary experiments used to validate the runtime"), validating the underlying
primitive each milestone actually needs, before any of it touches
`components/script`/`components/script_bindings`/`codegen.py` — the real, separate,
materially-higher-risk undertaking (production code the current SpiderMonkey build also
depends on) this groundwork is meant to de-risk, not replace.

**Second checkpoint (2026-09-30): interface/prototype-chain foundation.** Prompted by a real
finding while scoping the actual `codegen.py` work: production `components/script_bindings`'s
support modules (`interface.rs`, `proxyhandler.rs`, `finalize.rs`) are mutually interdependent
around SpiderMonkey's `JSClass`-based object model — none can be swapped for a V8 equivalent in
isolation (a `finalize.rs` callback is invoked *by* the same `JSClass` machinery `interface.rs`
uses to create the object in the first place). The maintainer's call: keep building the
foundation inside `roves-v8` at increasing fidelity before touching production code. `Interface` +
`Runtime::define_interface(name, parent)` creates a `v8::FunctionTemplate` constructor,
optionally inheriting via `FunctionTemplate::inherit` — WebIDL's own interface-inheritance shape
(`Element` inheriting from `Node`) — exposed as a named global constructor (`window.Node`).
`Runtime::create_instance<T>(&Interface, value)` creates an instance whose `[[Prototype]]` comes
from the interface, so `instanceof` and the prototype chain work from JS, with the same
ownership/finalization/typed-read-back guarantees `create_wrapped` already had. 5 tests, 32/32
total passing, including `instanceof` walking a real inheritance chain.

**Third checkpoint (2026-09-30): property accessors.** `PropertyGetter` +
`Runtime::define_property` adds a read-only accessor computed live from an instance's wrapped
Rust data. Two real findings, both caught by testing actual values rather than assuming
behavior: this `v8` crate version's `PropertyCallbackArguments` has no way to recover the actual
receiver in an accessor callback (only `.holder()`, which returns the *prototype* object for an
accessor installed on a shared prototype template — every read silently returned `Undefined`,
fixed by installing on the interface's instance template instead); and, a pleasant surprise, V8's
`FunctionTemplate::inherit` still propagates instance-template accessors down the interface
hierarchy despite that per-instance installation — a property defined on a parent interface
correctly inherits to child interfaces' instances, matching `instanceof`, confirmed by an initial
test that assumed the opposite and failed. 5 new tests, 36/36 total passing.

**Fourth checkpoint (2026-09-30): property setters.** `PropertySetter` +
`Runtime::define_settable_property` adds settable attributes on the same instance-template
foundation. Both callbacks' fn pointers share one deliberately-leaked
`Box<(PropertyGetter, PropertySetter)>` behind a single `External` (`AccessorConfiguration` has
only one shared `data` slot) — a small, bounded, per-property-definition cost, not per-instance.
2 new tests, 38/38 total passing, including a check that a JS-side assignment is reflected back
on the Rust side via `get_wrapped`, not just readable again from JS.

**Fifth checkpoint (2026-09-30): callable methods.** `NativeMethod` + `Runtime::define_method`
adds callable methods on an interface's **prototype** template — cleanly, on the shared
prototype, because `FunctionCallbackArguments::this()` gives the real receiver directly
(`PropertyCallbackArguments` only has `.holder()`, the source of the earlier accessor bug).
Real bug found and fixed: `define_interface` was eagerly calling `get_function()` to expose the
constructor globally, which materializes the actual prototype object from the template's
contents *at that moment* — later `define_method` calls had no effect on that already-frozen
object, every method test failing with `"X is not a function"`. V8 templates aren't fully
"late-bound" the way one might assume. Fixed by deferring constructor exposure to the first
`create_instance` call per interface, which also required fixing 4 existing tests that checked
`instanceof SomeInterface` without ever instantiating that interface. 3 new tests plus 5
adjusted, 41/41 total passing. With properties (readable and settable) and methods now both
covered, Phase 4's core "interface member" primitives are complete at the `roves-v8` prototype
level.

**Sixth checkpoint (2026-10-01): indexed read interception.**
`IndexedPropertyGetter` + `Runtime::define_indexed_property_getter` validates indexed read
interception on wrapped instances. `Some(Value::Undefined)` handles a supported index;
`None` permits ordinary own/prototype fallback. Canonical array indices are distinct from named
keys. Unlike accessors, indexed handlers do not propagate through `FunctionTemplate::inherit`:
install them explicitly on derived interfaces. Six new tests, 47/47 passing locally in normal
and JIT-less modes, include GC finalization and prototype-chain holder lookup. This remains a
read-only interception primitive, not full WebIDL collection semantics; register handlers before
any instance or descendant materializes templates. Production codegen remains untouched.

**Seventh checkpoint (2026-10-01): first generated WebIDL interface.** An opt-in V8 backend
now consumes Servo's real WebIDL parser AST and generates a typed `ValidityState` binding for
the actual `components/script_bindings/webidls/ValidityState.webidl` (11 readonly boolean
attributes). It installs accessors on the interface prototype with receiver signatures, exposes
the prototype/tag and a non-enumerable global interface property, and rejects unsupported
interface/member shapes instead of silently omitting them. The opt-in
`servo-script-bindings/v8-bindings-pilot` feature compiles the generated binding while the
default SpiderMonkey generator remains unchanged. Local generator tests and a full Windows
`mach build -p servo-script-bindings --features v8-bindings-pilot,js/jit --locked` passed;
runtime integration tests cover all 11 getters and descriptors in normal and JIT-less modes.
This is generator/backend groundwork, not a production DOM binding: its native trait is not yet
implemented by Servo's DOM, and no production runtime selects V8. Only Window-exposed interfaces
are accepted by this pilot. CI runs generator and runtime tests across Windows, Linux and macOS.

**Eighth checkpoint (2026-10-01): numeric WebIDL attributes.** The same backend now generates
both the real `ValidityState.webidl` and `Screen.webidl`. Alongside booleans, it maps WebIDL
`double` to Rust `f64` and `unsigned long` to `u32`/JavaScript Number. The Screen integration
test checks all six live values and `instanceof`; unsupported numeric types continue to fail
closed. Five generator tests and 51 unit + 3 WebIDL integration + 2 doctests pass locally in
normal and JIT-less modes, and `mach build` compiles `servo-script-bindings` with both generated
interfaces. This expands generated primitive coverage only; it does not yet implement Servo's
`Screen` DOM object or wire V8 into production. CI for this checkpoint is pending.

**Ninth checkpoint (2026-10-01): native Screen adapter.** Servo's existing `Screen` DOM type
now implements the generated `ScreenNative` contract under the opt-in `servo-script`
`v8-bindings-pilot` feature, forwarded to the binding crate. It delegates the four dimensions to
the existing embedder-backed DOM methods and preserves Servo's current 24-bit depth values. The
feature-gated adapter compiles together with `servo-script` on Windows using
`mach build --no-package -- -p servo-script --features v8-bindings-pilot,js_jit --locked`
(`AWS_LC_SYS_NO_ASM=1` was needed because NASM is absent locally; this is supported for debug
builds). The adapter validates native type compatibility but does not install a V8 object into a
realm or replace SpiderMonkey's reflector. Cross-platform CI for checkpoints 8 and 9 is pending.

**Tenth checkpoint (2026-10-01): lossless DOMString transport.** The engine-neutral V8 `Value`
now has a UTF-16 variant for strings with unpaired surrogates; scalar-valid strings continue to
use Rust `String`. The opt-in generator maps readonly WebIDL `DOMString` attributes to
`Vec<u16>`/`Value::Utf16String`. A generated fixture and runtime tests check code-unit identity
through a native getter, `store`/`load`, and JavaScript `charCodeAt`. This preserves the string
boundary without borrowing Servo's SpiderMonkey-rooted `DOMString` implementation. Local runtime
tests pass in default, normal V8 pilot and JIT-less V8 pilot modes; six Python generator tests pass.
Cross-platform CI is pending.

**Twenty-third checkpoint (2026-10-03): integer and float WebIDL operation arguments.**
The pilot generator supports all required WebIDL integer argument widths, restricted `float`, and
`unrestricted float`, alongside checkpoint 22's `double` forms and `unsigned long`. Runtime
conversion applies ToNumber and the WebIDL integer modulo/sign rules; restricted f32 arguments
reject non-finite results (including finite f64 values that overflow f32), while unrestricted
float preserves them. Fixture tests cover 8/16/32/64-bit boundaries, float conversion and errors.
Local verification passes 18 generator tests, 56 unit + 3 WebIDL integration + 2 doctests in normal
and JIT-less pilot modes, 52 unit + 2 doctests default, and the integrated `servo-script` check.
Patch 0087 applies cleanly. CI is green: V8 6/6 (37132532003), all six Servo bundles plus Steam and patch validation (37132531959), Android (37132531996), and iOS (37132532177). Production remains SpiderMonkey.

**Twenty-fourth checkpoint (2026-10-03): string WebIDL operation arguments.** The opt-in
generator accepts required `DOMString` and `USVString` operation arguments. DOMString is passed to
native Rust as lossless UTF-16 code units (`Vec<u16>`); USVString becomes a scalar-valid `String`,
replacing unpaired surrogates with U+FFFD. JavaScript ToString conversion runs before native
dispatch, Symbol throws TypeError, and exceptions raised by user `toString` hooks are rethrown
unchanged. Omitted required strings convert `undefined` to the string `"undefined"`. At this
checkpoint nullable, optional, and ByteString arguments remain fail-closed. Local verification
passes 19 generator
tests, 57 unit + 3 integration + 2 doctests in normal and JIT-less pilot modes, 52 default unit +
2 doctests, and the integrated `servo-script` check. Patch 0088 applies cleanly. CI is green: V8
6/6 (37137152487), all six Servo bundles plus Steam, SDL3 checks and patch validation (37137152516),
Android (37137152568), and iOS (37137152538). Production still uses SpiderMonkey.

**Twenty-sixth checkpoint (2026-10-03): optional WebIDL operation arguments without defaults.**
The pilot represents omitted and explicit-`undefined` optional arguments as the engine-neutral
`WebIdlOptionalArgument::Missing` state. Optional nullable parameters preserve the distinct
`Present(None)` state for `null`, and ordinary present values use the established conversion.
Default-valued optional parameters and variadics remain fail-closed. Runtime fixture coverage
checks omitted, `undefined`, `null`, coercion and Symbol errors. Local verification passes 21
generator tests, 59 unit + 3 WebIDL integration + 2 doctests in normal and JIT-less pilot modes,
52 default unit + 2 doctests, and the integrated `servo-script` check. Patch 0090 applies cleanly.
CI is green: V8 matrix ([37144196093](https://github.com/DRincs-Productions/roves/actions/runs/37144196093)),
all six Servo bundles plus Steam, SDL3 and patch validation ([37144196126](https://github.com/DRincs-Productions/roves/actions/runs/37144196126)),
Android ([37144196111](https://github.com/DRincs-Productions/roves/actions/runs/37144196111)), and iOS
([37144196107](https://github.com/DRincs-Productions/roves/actions/runs/37144196107)). This remains
pilot-only; production still uses SpiderMonkey.

**Twenty-seventh checkpoint (2026-10-03): WebIDL ByteString operation arguments.** The pilot
converts JavaScript ToString output to engine-neutral `Vec<u8>` only when every UTF-16 code unit is
at most 255, otherwise it throws TypeError. Symbol and user `toString` exceptions preserve their
specified behavior. Required, nullable and optional nullable forms are covered; defaults and
variadics remain fail-closed. Local verification passes 22 generator tests, 60 unit + 3 WebIDL
integration + 2 doctests in normal and JIT-less pilot modes, 52 default unit + 2 doctests, and the
integrated `servo-script` check. Patch 0091 applies cleanly. CI is green: V8 ([37147151024](https://github.com/DRincs-Productions/roves/actions/runs/37147151024)),
all six Servo bundles plus Steam, SDL3 and patch validation ([37147151005](https://github.com/DRincs-Productions/roves/actions/runs/37147151005)),
Android ([37147150994](https://github.com/DRincs-Productions/roves/actions/runs/37147150994)), and iOS
([37147151023](https://github.com/DRincs-Productions/roves/actions/runs/37147151023)). This remains
pilot-only; production still uses SpiderMonkey.


**Thirtieth checkpoint (2026-10-04): nullable string defaults for optional WebIDL arguments.** Added explicit fixtures for optional nullable DOMString, USVString and ByteString arguments defaulting to null. Omission and undefined select the default null; explicit null stays null, and concrete values retain their existing conversions. Local verification passes 25 generator tests; 63 unit + 3 WebIDL integration + 2 doctests in both normal and JIT-less pilot modes; 52 default unit + 2 doctests; and integrated servo-script check. Patch 0094 passes pristine-series validation; wiki build passes all 82 pages (commit 1fcf64f). CI is green: V8 37192646577, Servo patch validation/SDL3/Steam/six bundles 37192646570, Android 37192646472 and iOS 37192646529. This remains pilot-only; production still uses SpiderMonkey.
**Twenty-ninth checkpoint (2026-10-03): optional WebIDL string defaults.**
The opt-in generator emits optional defaults for DOMString, USVString and ByteString. DOMString
defaults become exact UTF-16 code units, USVString defaults become scalar-valid Rust strings, and
ASCII ByteString defaults become bytes; nullable string defaults preserve omission versus null.
Runtime fixtures cover a literal backslash, a supplementary Unicode character, null and explicit
argument values. Local verification passes 24 generator tests, 62 unit + 3 integration + 2 doctests
in both normal and JIT-less pilot modes, 52 default unit + 2 doctests, and the integrated
servo-script check. Patch 0093 applies to the CP28 source snapshot; wiki build passes all 82 pages
and commit 462ed36 is published. CI is green: V8 37189196623, Servo patch validation/SDL3/Steam/all
six bundles 37189196591, Android 37189196574 and iOS 37189196578. This remains pilot-only and
production still uses SpiderMonkey.

**Twenty-eighth checkpoint (2026-10-03): optional WebIDL arguments with explicit primitive defaults.**
The pilot applies explicit boolean, integer and floating-point defaults when an optional argument
is omitted or explicitly undefined; supplied values retain ordinary WebIDL conversion, including
null conversions, while nullable arguments preserve explicit null. Numeric defaults retain the
IDL type and handle unrestricted infinities with Rust constants. Dictionary/string/other defaults
remain fail-closed. Generator and runtime tests cover false, zero, signed/unsigned numbers,
finite and unrestricted floats, nullable null and nullable non-null defaults. Local verification
passes 23 generator tests, 61 unit + 3 integration + 2 doctests in normal and JIT-less pilot modes,
52 default unit + 2 doctests, and the integrated servo-script check. Patch 0092 applies to the CP27
source snapshot; wiki build passes all 82 pages. CI is green: V8 matrix
(37151335289), Servo bundles/Steam/SDL3/pristine patch validation (37151335249), Android
(37151335284), and iOS (37151335248). This remains pilot-only; production still uses SpiderMonkey.

**Twenty-fifth checkpoint (2026-10-03): nullable WebIDL operation arguments.** Required nullable
boolean, numeric, `DOMString`, and `USVString` arguments now generate `Option<T>` native contracts.
For nullable types WebIDL maps both JavaScript `null` and `undefined` to IDL null; other values use
the inner type's existing Boolean, numeric, ToString, UTF-16, or scalar-value conversion. A mixed
nullable/non-nullable signature verifies the per-argument conversion flags. Optional and variadic
arguments and other unsupported types still fail closed. Local verification passes 20
generator tests, 58 unit + 3 integration + 2 doctests in normal and JIT-less pilot modes, 52 default
unit + 2 doctests, and the integrated `servo-script` check. Patch 0089 applies cleanly. CI is
green: V8 (37141033528), all six Servo bundles plus Steam and patch validation (37141033571),
Android (37141033507), and iOS (37141033514). Production remains SpiderMonkey.

**Twenty-second checkpoint (2026-10-03): required numeric WebIDL operation arguments.**
The opt-in generator now accepts required `double`, `unrestricted double`, and `unsigned long`
arguments in addition to booleans. Runtime coercion uses V8 ToNumber before native dispatch;
restricted doubles reject NaN/infinities, unrestricted doubles preserve them, and unsigned long
uses modulo 2^32. Omitted required double arguments and Symbol conversions throw before native
dispatch. Runtime fixtures exercise strings, Infinity, modulo wrapping, omitted values, and
conversion errors; unsupported optional, nullable, and string arguments remain fail-closed.
Local generator and normal/JIT-less/default runtime tests pass, as does the integrated
`servo-script` check. Patch 0086 applies cleanly. CI verde: V8 6/6 (37117775121), sei bundle Servo con Steam e patch validation (37117775170), Android (37117775250) e iOS (37117775238). Production remains SpiderMonkey.

**Twenty-first checkpoint (2026-10-03): finite and unrestricted WebIDL floating-point semantics.**
The generated contract distinguishes restricted `float`/`double` from `unrestricted float`/
`unrestricted double`. Engine-neutral `FiniteF32`/`FiniteF64` wrappers reject NaN and infinities at
construction, so restricted operation returns and nullable values cannot violate the WebIDL
invariant; unrestricted types preserve them as JavaScript Numbers. Mutable `double` attribute
conversion now throws `TypeError` for non-finite results before native mutation, while nullable
`null` remains IDL null. Servo's `Screen` adapter uses the finite contract, activated only under
the pilot feature. Local validation: 16 generator tests; 56 unit + 3 WebIDL integration + 2
doctests in normal and JIT-less pilot modes; 52 unit + 2 doctests default; `cargo check` of
`servo-script` with V8 pilot and JIT features. Patch 0085 applies cleanly. CI is green: V8 matrix 6/6 (37108919478), all six Servo bundles plus Steam and patch validation (37108919488), Android (37108919483), and iOS (37108919477). Production remains
SpiderMonkey.
**Twentieth checkpoint (2026-10-02): numeric WebIDL operation returns.** Supported operations
now return `byte`, `octet`, `short`, `unsigned short`, `long`, `long long`, `unsigned long long`,
and `float`, in both nullable and non-nullable form. The generated native contract uses the
matching Rust integer width or `f32`, and all results become JavaScript Numbers; nullable `None`
becomes `null`. Generator and runtime fixtures cover signedness, width, float and nullable values.
The local normal/JIT-less pilot and default suites pass (56 unit + 3 integration + 2 doctests per pilot mode; 52 unit + 2 doctests default), as do 16 generator tests and the integrated binding-crate check. Patch 0084 applies cleanly. CI is green: V8 6/6 (37061193384), all six Servo bundles plus Steam and patch validation (37061193356), Android (37061193392), and iOS (37061193362). This remains pilot coverage; production
still uses SpiderMonkey.

**Nineteenth checkpoint (2026-10-02): nullable primitive WebIDL operation returns.** Supported
operations can now return `boolean?`, `double?`, and `unsigned long?`, with native `Option<T>`
contracts. `Some` maps to the JS primitive and `None` maps to `null`. Generated runtime fixtures
cover all three types. Local verification passes 15 generator tests, 56 unit + 3 integration + 2
doctests in pilot normal and JIT-less configurations, 52 unit + 2 doctests without the pilot,
and the integrated `servo-script-bindings` check. Patch 0083 applies cleanly and cross-platform CI is green: V8 matrix 6/6 (37048609736), all six Servo bundles plus Steam and patch validation (37048609764), Android (37048609658), and iOS (37048609889).
This remains pilot coverage; production still uses SpiderMonkey.

**Eighteenth checkpoint (2026-10-02): string-returning WebIDL operations.** Supported
zero-argument operations can now return `DOMString`, `DOMString?`, `USVString`, and
`USVString?`. DOMString uses UTF-16 code units, preserving lone surrogates through the V8 value
boundary; USVString uses scalar-valid Rust strings; nullable values preserve IDL null. Generated
runtime fixtures cover each return shape. Local verification passes 14 generator tests, 56 unit +
3 integration + 2 doctests in pilot normal and JIT-less configurations, 52 unit + 2 doctests
without the pilot, and the integrated `servo-script-bindings` check. Patch 0082 and cross-platform
CI are pending. This remains pilot coverage; production still uses SpiderMonkey.

**Seventeenth checkpoint (2026-10-02): boolean WebIDL operation arguments.** The pilot generator
now supports required boolean parameters on otherwise supported single-signature instance
operations. JavaScript truthiness maps to IDL boolean; omitted arguments become `undefined` and
convert to false, while extra JS arguments are ignored. Optional, nullable, variadic, and other
parameter types remain rejected. Generated fixture/runtime tests cover truthiness and arity.
Local verification passes 13 generator tests, 56 unit + 3 integration + 2 doctests in pilot normal
and JIT-less configurations, 52 unit + 2 doctests without the pilot, and the integrated
`servo-script-bindings` check. Patch 0081 and cross-platform CI are green: V8 matrix 6/6 (37036084532), all six Servo bundles plus Steam and patch validation (37036084693), Android (37036084595), and iOS (37036084598). This remains pilot
coverage; production still uses SpiderMonkey.

**Sixteenth checkpoint (2026-10-02): zero-argument WebIDL operations.** The opt-in generator now emits interface methods with no arguments and exactly one signature, returning `undefined`, `boolean`, `double`, or `unsigned long`. Generated methods downcast through the existing engine-neutral native method callback and preserve interface receiver checks. Overloads, arguments, static operations, extended attributes, and other result types fail closed. Runtime tests call generated void and primitive-returning methods from JavaScript. Local verification passes 11 generator tests, 56 unit + 3 integration + 2 doctests in normal and JIT-less pilot configurations, 52 unit + 2 doctests without the pilot, and the integrated `servo-script-bindings` check. Patch 0080 and cross-platform CI are green: V8 matrix 6/6 (37029579196), all six Servo bundles plus Steam and patch validation (37029580090), Android (37029579200), and iOS (37029579293). This remains pilot coverage; production still uses SpiderMonkey.

**Fifteenth checkpoint (2026-10-02): nullable primitive WebIDL attributes.** The opt-in generator and runtime now support `boolean?`, `double?`, and `unsigned long?` with native `Option<T>` values. JavaScript `null` maps directly to IDL null; other values use Boolean or numeric WebIDL conversion, including modulo 2^32 for unsigned long. Conversion runs before native mutation, and Symbol conversion failures preserve the previous value. The generated fixture tests null, string/undefined coercions, modulo conversion, and conversion exceptions. Local verification: 10 generator tests; 56 unit + 3 integration + 2 doctests in normal and JIT-less pilot modes; 52 unit + 2 doctests without the pilot; `cargo check -p servo-script-bindings --features v8-bindings-pilot,js/jit --locked` passes. Patch 0079 is green in the ordered overlay validator. CI passed: V8 matrix 6/6 (37024610041), all six Servo bundles plus Steam and patch validation (37024609945), Android (37024609737), and iOS (37024609529). This remains opt-in pilot coverage; production still uses SpiderMonkey.

**Fourteenth checkpoint (2026-10-02): `USVString` and nullable `USVString?` attributes.** The opt-in generator emits Rust `String` / `Option<String>` contracts and uses JavaScript string coercion followed by scalar-value conversion, replacing unpaired UTF-16 surrogates with U+FFFD as required for USVString. Nullable `null` remains IDL null; `undefined` and other values use JavaScript `ToString`, and conversion exceptions occur before native mutation. Generated and runtime fixtures cover these cases, including Symbol errors. Local verification passes: 9 Python generator tests; 56 unit + 3 integration + 2 doctests in normal and JIT-less pilot configurations; 52 unit + 2 doctests without the pilot. Patch 0078 is applied by the Linux overlay validator. CI is green: V8 matrix 6/6 (run 37016952782), all six Servo bundle jobs plus Steam and patch validation (37016951496), Android (37016951470), and iOS (37016951542). This remains an opt-in pilot; production bindings still use SpiderMonkey.

**Thirteenth checkpoint (2026-10-02): nullable DOMString attributes.** The opt-in V8 backend
now accepts nullable `DOMString?` attributes, with native `Option<Vec<u16>>` values and `null`
represented by `Value::Null`. Mutable setters map JavaScript `null` directly to IDL null and apply
`ToString` to other values, preserving UTF-16 code units; failed conversions such as `Symbol`
leave native state unchanged. Local generator tests pass 8/8, and runtime tests pass in normal,
JIT-less and non-pilot configurations. This remains an opt-in pilot; no production Servo reflector
or SpiderMonkey path has been replaced. Patch 0077 is being prepared and cross-platform CI is in
progress.

**Twelfth checkpoint (2026-10-01): mutable primitive attributes.** The opt-in WebIDL backend
now generates setters for `boolean`, `double`, and `unsigned long` in addition to mutable
`DOMString`. Boolean uses JavaScript truthiness; numeric inputs use JavaScript `ToNumber`, and
`unsigned long` applies WebIDL's modulo-2³² conversion (including negative and out-of-range
values). Conversion completes before borrowing native state, so user-defined coercion hooks may
run safely; conversion failures such as Symbol-to-number leave the native value untouched.
Generated fixture and runtime tests cover these semantics; local normal and JIT-less V8 tests and
seven Python generator tests pass. This remains an isolated pilot, not production DOM binding.

**Eleventh checkpoint (2026-10-01): mutable DOMString attributes.** The opt-in WebIDL backend
also generates a setter for mutable `DOMString` attributes. `roves-v8` applies JavaScript
`ToString` in the V8 callback, then passes UTF-16 code units to the generated native trait, so
ordinary non-string values convert according to JavaScript and lone surrogates survive. Converting
a Symbol raises the expected JavaScript `TypeError`. Other mutable WebIDL types continue to fail
closed pending their precise WebIDL conversion semantics. The fixture exercises native read/write
round trips; local normal and JIT-less tests pass. This remains an isolated pilot path and does not
attach SpiderMonkey-rooted DOMString values to V8 or activate V8 in production.

**Not yet done:** production use of V8; binding existing Servo DOM implementations to generated
wrappers; remaining WebIDL types, members, inheritance and interfaces; complete
`proxyhandler.rs`-equivalent indexed/named property semantics (only indexed reads prototyped);
replacement of SpiderMonkey rooting/tracing in production; `document`/DOM events and callbacks;
Promise APIs at the DOM level; modules/dynamic import; fetch/storage; Canvas; WebGL; WebGPU;
audio/media; Workers; WebAssembly; and the Phase 5 real game validation matrix.

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
