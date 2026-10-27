# 0378 — deno_core production adoption: NO-GO for version 0.413.0

## Decision and scope

Keep the Servo shell and rusty_v8. The user authorizes replacing and removing Servo
services, including module/async services; their current architecture is not a constraint.
DOM/layout/rendering remain Servo initially. deno_core is not adopted in production: the
published public API cannot replace the required multi-context service without keeping
Roves' implementation alongside it or maintaining a private fork. This is a decision for
the evaluated release, not a claim that all future versions or other embeddings are unsuitable.

## Reproducible evidence

Inspected crates.io deno_core0.413.0 source (MIT), not a speculative dependency name.
Cargo generate-lockfile for the isolated [workspace] succeeds and resolves194 packages:
deno_core0.413.0, deno_v8 0.5.0, v8 150.4.0 and v8x149.4.0-rc.4 in the lockfile. A lock
entry is not proof that every optional backend is built. Production Roves pins v8 152.2.0.
Cargo metadata --no-deps --locked and Rust syntax parsing of the probe pass. No production
manifest or lockfile was changed. Public sources:
https://crates.io/crates/deno_core/0.413.0 and
https://docs.rs/deno_core/0.413.0/deno_core/struct.JsRuntime.html.

The published source exposes CreateRealmOptions but no public create_realm consumer;
runtime/jsrealm.rs declares JsRealm pub(crate), and main_realm is crate-private/test-only.
The runtime's context constructor passes a global template, not a caller-provided existing
global proxy. Roves requires same-isolate frame interaction and WindowProxy identity across
navigation. Raw V8 context creation does not initialize Deno's private realm slots: its
isolate-wide callbacks read those slots and assume valid internal state. The unsafe path
is identified by source inspection, not an observed runtime crash; do not execute it just
to force a failure. Own module-evaluation checkpoints also need integration with HTML
checkpoint order; loader fetch/security metadata still requires an adapter.

## Experiment and limitations

Claude prepared an isolated in-memory-loader probe in target/task-0378/probe with nine
test cases for static/dynamic/JSON imports, cycles, top-level await/microtasks, classic
checkpoints, URL refusal, string-codegen hook and limited raw-context observations. Its
uncompiled source and detailed report remain local evaluation artifacts, not shipped code
or a claimed working feature. Main --jitless only requests the flag and prints observations;
it is not evidence that all assertions pass in JITless. Multi-frame modules, WindowProxy
reuse, HTML CSP/origin security and lifetime reclamation are NOT proven by these scenarios.

No runtime scenarios were executed and no startup, CPU, RAM, GPU or binary-size comparison
was measured. Avoid a heavy second-V8 build now: the public-API adoption gate already fails,
so successful single-context tests would not change the production decision. If re-evaluated,
compile/run the probe online with assertion-based normal/JITless tests before any adoption;
first review the callback signatures against the resolved V8 version. Do not add an unused
production dependency or publish an unverified probe as a supported component.

## Replacement inventory and future gate

deno_core could own module records/maps, dynamic-import completion and async module driving.
Servo security/fetch/CSP/import-map policy, DOM bindings and HTML task coordination would
still need adapters. With this release non-main contexts would retain the old service,
which defeats removal of duplicate implementation. No source lines were removed through
deno_core adoption, and no savings are claimed.

Revisit only with a concrete maintained public-API route supporting realm lifecycle and
existing global proxies, compatible callback ownership, HTML checkpoint control and one
V8 version line. Then verify normal/JITless, all import cases, origin/CSP policy, frame
isolation/navigation and reclamation, inventory exact deletions/adapters and measure like
for like. Until then use existing maintained rusty_v8 APIs directly; do not reimplement
services that a usable maintained API already covers. Legacy DOM gaps, sourceText0361 and
overall retention remain open.
