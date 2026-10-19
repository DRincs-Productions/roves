# Roves V8 migration plan

**Surface.** An inventory of `script`, `script_bindings` and `script_webgpu` found about 590
distinct `js::` items:
- `jsapi`: 212;
- `rust::wrappers2`: 190 (thin `&mut JSContext` wrappers over the same operations);
- `rust`: 51;
- `glue`: 45;
- `typedarray`: 26;
- `gc`: 16;
- `jsval`: 12;
- the rest are small.

There are two layers:
- **The DOM code (`script`).** It uses a narrow vocabulary: `JSContext`, `Handle*`/`Rooted`/
  `rooted!`, `Heap<T>`, `JSVal`, conversions, typed arrays, realms and a set of
  object/property/exception/promise operations. `roves-js` emulates this vocabulary.
- **The SpiderMonkey binding machinery (`script_bindings` internals and the generated
  bindings).** This is `JSClass`, proxy handler families, JIT info and DOM class hooks.
  - *Revised 2026-10-08 (CP95), after reading the generated code.* It is **emulated** too,
    not replaced. The generated bindings only touch the JSAPI through `script_bindings`'
    helpers (`init`, `wrap`, `interface`, `utils`, `proxyhandler`) and through literal
    bindgen structs (`JSJitInfo`, `JSPropertySpec`, `JSFunctionSpec`, `JSClass`,
    `JSClassOps`), which `roves-js` can define with the same names and fields. Emulating them
    keeps Servo's mature WebIDL generator and its semantics unchanged, and avoids rewriting
    about 520 bindings.
  - How the emulation works on V8:
    - A `JSClass` becomes an object template with internal fields for its reserved slots.
    - A DOM reflector is wrapped (`Object::wrap`) with a cppgc box. The box's trace calls the
      class trace hook with a `JSTracer` that forwards to the cppgc visitor, and its
      destruction calls the finalize hook. This closes the GC graph: `Dom<T>` traces its
      target's reflector, whose class hook traces the native's fields.
    - Native callbacks (`JSNative`, JIT getters/setters/methods) are bridged through a
      SpiderMonkey-style `vp` array (callee, this, arguments) of `JSVal`s.
    - Proxy handler families map onto V8 named/indexed interceptors that call the proxy traps.
  - The `CGV8BindingRoot` backend (486/486 shapes) stays as a reference and for tests.

**Value model.**
- `JSVal` keeps immediates (undefined, null, booleans, int32, doubles) inline.
- A GC thing (`*mut JSObject`, `*mut JSString`, symbols, BigInts) is a pointer to a
  heap-allocated **cell** that holds a V8 handle:
  - An object's cell is interned through a private symbol on the object, so one object always
    has one pointer and pointer equality keeps working.
  - A cell is strong while it is rooted (`Rooted`/`rooted!`/`Handle` keep a root count) and
    weak otherwise. A weak cell is released when V8 collects its object.
- An unrooted pointer has the same hazard it has under SpiderMonkey: it is only valid until the
  next GC.
- `Heap<T>` holds the cell plus a traced reference that the owning DOM object reports to cppgc
  (`#[derive(Trace)]`/`JSTraceable` → `roves_v8::Tracer`).

`roves-js` is the engine layer, like `mozjs`, so it may use `v8` directly. The hard rule still
applies one level up: no `v8` type may appear in DOM or shell APIs.

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

The historical migration target included removing these SpiderMonkey-specific dependencies
and infrastructure where no longer needed:

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

Servo's DOM code still uses SpiderMonkey-shaped rooting/tracing APIs, implemented through the
`roves-js` compatibility layer on V8. Do **not** treat API-name compatibility as proof that
those ownership abstractions are ideal or fully audited for V8.

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

Documentation describes the shipped architecture: Roves is a Servo fork oriented toward games
that uses **V8 for JavaScript and SDL3 for the shell/platform direction**. Keep claims about API
coverage distinct from claims about the engine architecture.

## Console-oriented requirement: JIT-less

Desktop Roves should normally use V8 with JIT enabled for performance. However, future consoles or other restricted platforms may prohibit dynamically generated executable memory.

Therefore JIT-less is a first-class supported configuration.

Keep CI coverage for a JIT-less configuration. A Linux JIT-less CI build/test is not a console certification test; it is a guardrail that prevents Roves from accidentally assuming that runtime code generation is always available.

Do not allow Roves APIs outside `roves-v8` to depend on JIT availability.

Explicitly test JavaScript-heavy games under JIT-less because performance characteristics can differ substantially from normal desktop V8.

WebAssembly must also be tested separately under the selected JIT-less V8 configuration; do not assume that desktop JIT behavior, available Wasm features, signal/trap handling or performance automatically carry over to restricted platforms.

## Build strategy

A Windows local toolchain is available: use local builds and tests for fast iteration, then GitHub CI for cross-platform and final verification. CI does not replace local work that can be done.

For compatibility fixes, proceed incrementally. Once a change reaches a coherent
**buildable** state:

1. commit it;
2. push it;
3. observe all relevant CI jobs;
4. fix failures before expanding the migration further;
5. add or strengthen tests when a CI failure exposes an untested contract or when a newly migrated subsystem needs regression coverage.

Do not accumulate a huge unvalidated rewrite if a smaller buildable checkpoint is possible.

The production engine is V8-only. Keep compatibility code only where it is still needed by
Servo call sites; remove it incrementally when its behavior has been replaced and verified.

## Stato attuale e limiti aperti

Il cutover di produzione, i build desktop normale/JIT-less e le smoke suite end-to-end sono
completati. `support/v8-smoke/` copre DOM, moduli, Promise, typed arrays, Worker, WebAssembly,
Canvas/WebGL, storage e workload PixiJS/Three.js; questo non equivale a copertura WPT completa
né certifica ogni API o gioco.

Restano aperti i seguenti limiti, verificati nel codice e nell'handoff del 18 ottobre 2026:

- Compatibilità dei proxy `Window` cross-origin, prototipi immutabili, valutazione di
  `TrustedScript`, e durata dei realm/script/module privates e delle callable class boxes.
- I realm delle pagine navigate restano vivi fino alla distruzione del runtime; manca copertura
  Web Platform Tests su scala ampia.
- Web Audio non offre ancora `PeriodicWave`, `ScriptProcessor` e `AudioWorklet`. I nodi
  `WaveShaper`, `Delay`, `Convolver` e `DynamicsCompressor` sono stati implementati dopo le
  note iniziali dell'handoff; validarli nel workload Tone resta necessario.
- Worker WebGL 1/2 restituisce `null` e il worker non ha `requestAnimationFrame`; l'esposizione
  di `OffscreenCanvas` non dimostra che PixiJS/Three.js funzionino nei worker.
- Prestazioni e memoria rispetto a Chrome restano inferiori su alcuni carichi; risultati e
  limiti sono raccolti in `docs/ROVES_PERFORMANCE.md`.


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

## Lavoro residuo

- Verify cross-origin window behavior, immutable prototypes and `TrustedScript` CSP handling.
- Ensure realms and native script/module data can be reclaimed after navigation.
- Expand WPT and representative game coverage, including validation of the recently added Web
  Audio nodes in Tone.js.
- Implement or explicitly defer `PeriodicWave`, `ScriptProcessor` and `AudioWorklet`; validate
  worker WebGL and worker `requestAnimationFrame` before claiming worker rendering support.
- Keep normal and JIT-less desktop CI green and record performance/memory comparisons without
  treating engine identity as a performance claim.
- Audit remaining SpiderMonkey-shaped compatibility APIs and `jstraceable_derive`; remove them
  only where their responsibilities have been replaced and tests cover the transition.

## Guidance for Codex/implementers

Treat this document as architectural reference and a concise backlog, not as permission for a
single giant rewrite.

Before each change, inspect the current code. Prefer the smallest coherent buildable change,
then use local checks and CI feedback before continuing.

Do not silently change unrelated Servo behavior to make V8 integration easier. If an existing web-platform semantic must change, document why and add a regression test.

Do not expose V8 implementation details merely to bypass a difficult binding problem. Difficult ownership/binding issues belong in the scripting layer.

When uncertain about V8 APIs, use current V8/rusty_v8 behavior rather than assuming SpiderMonkey concepts map one-to-one.
