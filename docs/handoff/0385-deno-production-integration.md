# 0385 - Production deno_core integration: reviewed loading API gap

No production runtime/Cargo/lockfile edits were made. Existing rusty_v8 backend remains
active. Do not claim deno_core adoption, retention resolution or tonight completion.

## Verified progress

Isolated deno_core0.413 class/async probe CI40564575701 passes all four Linux/Windows
normal/JITless jobs. Two Rust tests include twelve logical scenarios: existing module,
bootstrap, native cppgc class and Rust async operation assertions. This is not Servo DOM.

Separate Codex shell changes: local373c4681ccc denies auxiliary WebView allocation via
Servo's existing default-None responder; local84f324d8489 adds dismissible egui development
notice. Bundles identified by existing launch metadata remain log-only; no new dependency.
Replay0389/0390 matches five touched files after LF normalization; syntax and existing
SDL3 windowing contracts pass. Real browser/visual verification remains pending.

## Current blocking decision

Deno's public load_main_es_module/load_side_es_module and _from_code APIs hold
&mut JsRuntime for the loading future (runtime/jsruntime.rs:3235-3395 in published0.413).
Keeping that future while Servo executes JS tasks would violate the exclusive borrow.
Public mod_evaluate returns an independent future, but only AFTER loading/instantiation.
Private JsRealm/ModuleMap are not an acceptable workaround.

Reviewed alternatives, SOURCE ANALYSIS rather than executed browser proofs:

- import() at discovery may execute a head module before the game's #root exists.
- import() at the HTML execution point avoids that error, but its completion waits for
  top-level await. Waiting for that promise to release DOMContentLoaded can deadlock a
  module awaiting DOMContentLoaded. Waiting before the next deferred module changes
  observable order when the first module suspends at await.
- import.defer() is implemented by Deno (modules/map/dynamic.rs:579-666), but eagerly
  evaluates async dependencies. It cannot provide a universal load-without-evaluate
  contract for HTML modules containing TLA. Source-phase import is Wasm-only here.
- Keeping only Servo's root-byte fetch does not discover dependency graphs. Avoid a
  second handwritten graph, unsafe borrow extension or cancelling/restarting load futures.

## Current decision: proceed with a PROVISIONAL static-load pause

The latest user instruction supersedes option2 and authorizes proceeding now with a
ScriptThread pause during STATIC module graph fetch/instantiation. Dynamic imports stay
asynchronous. Preserve the HTML evaluation point and TLA semantics; do not wait for TLA
settlement before unrelated deferred scripts/DOMContentLoaded progression.

This is explicitly an OPEN REVIEW ITEM FOR TESTING, not a proven acceptable pause:

- Record whether it occurs at initial game startup or when adding a new module script to
  a running game; distinguish static imports from dynamic import().
- Measure local packaged assets versus network assets, cold/warm loads and deliberately
  slow responses. Record scene/build/platform/mode and actual pause durations.
- Observe queued input, delayed timers, animation/frame interruptions, network failure,
  timeout and closing behavior; do not claim shell responsiveness without a test.
- Verify dynamic imports still allow input/timers while awaiting data, and TLA evaluation
  does not hold the exclusive static-loading borrow.
- Revisit the design if pauses are problematic. No user-approved latency threshold exists;
  a suitable maintained independently-owned async Deno load API remains the preferred
  future way to remove this limitation. No fork/private API/second graph authorized.

The interrupted seven-file preparation is saved under
 target/task-0385/superseded-option1-wip/incomplete.patch with exact file copies.
Reuse it after review rather than redoing research; it is not a completed cutover.
The earlier choice2 was briefly recorded in local a9d055b06bd and is now superseded.
Claude resumes with reapproved-provisional.txt. Current backend remains until atomic
implementation and review; no partial source/dependency change may be published.
## Post-reset result: new evaluation-error API gap (C2), decision pending

The single28October00:02 retry completed normally and Claude is available, not quota-blocked.
No production source, manifest, lockfile or test was changed. Deliverable:
 target/task-0385/IMPLEMENTATION.md; result reset-result.json; session
ab3e2eb3-6c0f-44ee-9277-de12c954a202. Prefer reusing this context for the next correction
instead of repeating completed research. The one-shot automation was DELETED.

C2 SOURCE REVIEW (not an executed browser proof): public JsRuntime::mod_evaluate returns
Ok on ordinary module evaluation rejection. modules/map/evaluation.rs:153-162 rethrows the
reason in an internal promise reaction, producing an unhandled rejection. The public future
does not expose that reason; module handle/map accessors remain pub(crate). Current Roves
ModuleEvaluate returns the actual evaluation promise and ThrowOnModuleEvaluationFailure
checks its rejected reason (components/roves-js/src/modules_impl.rs:818-849), preserving a
synchronous error path used by Servo. Do not silently replace error/window.onerror with
unhandledrejection or identify internal promises through unreliable origin heuristics.

An asynchronous question asks the user to choose: preserve error/onerror and pause adoption
on C2, or explicitly accept unhandledrejection for static module evaluation errors. This
is SEPARATE from the already-approved provisional static-loading pause; keep that approval
and its performance review requirements intact. No answer means no permission for the new
error-reporting deviation. No fork or upstream communication has been authorized/performed.

C1 has a public candidate solution: external_ops_tracker.ref_op for the document lifetime
prevents Deno from declaring a TLA awaiting Servo timers/fetch/events stalled; unref before
drop. This remains unimplemented/unverified, with wakeups/shutdown behavior to test.
net_traits::fetch_async delivers on the FetchThread and can supply static loading bytes
without reentering ScriptThread JS. Preserve all fetch/security policy in the adapter.
Other risks recorded: inline module import.meta.url identities and extra Deno microtask
checkpoints require explicit review; do not claim full HTML behavior from isolated probes.

The saved seven-file preparation remains OUTSIDE publishable sources. No need to redo B1
research: exact public150.4 headers are available, but compiled shim/actual crate-header
availability is still unverified. No decorative dependency or two V8 static libraries.

## Other integration requirements already authorized

- Unify V8: production152.2 versus Deno150.4 cannot coexist as two static V8 libraries.
- Existing CSP/Trusted Types string-codegen callback must be preserved. Exact public C++
  headers from rusty_v8 v150.4.0 submodule ac1e23989121713ca642f6650b34deff7b686896 confirm
  SetModifyCodeGenerationFromStringsCallback and its result/callback types. Do not use
  mangled names/ABI guessed in the superseded report; use a compiler-checked thin shim.
- Deno main context must become actual Window global; move Roves realm slot2 (Deno uses
  it) and reject subsequent document contexts before creation. Auxiliary-window shell
  denial alone does not block reload, iframe globals or all navigation paths.
- Keep CSP/SRI/credentials/referrer/import-map policy, DOM callbacks, HTML microtask and
  IndexedDB cleanup, rejection events and native wakeups. No privileged Deno game APIs.
- Remove each replaced service when the complete cutover is reviewed; no half backend.

Detailed task-local evidence: target/task-0385/RESULTS.md, CODEX_REVIEW.md, prompt.txt,
followup.txt and v8-public headers. The interrupted assignment is now authorized to resume under the provisional decision.
No quota block, automation or scheduled retry. Chrome MCP new_page was denied to Claude;
not retried. No Chrome measurements or executed B3 counterexamples are claimed.
