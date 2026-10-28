## PUNTO CORRENTE — 28 ottobre 2026, handoff autonomo

Il documento autorevole completo è [V8_MIGRATION.md](../V8_MIGRATION.md), sezioni1–10 prima
di FINE DEL PUNTO OPERATIVO. Contiene decisioni approvate, Git/CI, mappa file, prove reali,
comandi, problemi e incarichi in ordine. Leggere quello prima di questo storico.

- Deno produttivo è nel WORKTREE e incompleto; non committato/pubblicato. TODO.md è utente.
- Codex implementa direttamente; nessun Claude attivo. Non usare Claude prima del30ottobre
  alle17:30 Europe/Rome. Promemoria unico ripresa-claude-il-30-ottobre-alle-17-30 creato.
- C3 inline fragment accettato, oltre a pausa statica provvisoria e C2rejection.
- Test reali aggiornati:108normal/107featureJITless, rispettivamente2/3ignore;9Deno specifici.
  Log JITless valido: direct-cache-guard-feature-jitless.log. V8_TEST_MODE da solo NON basta.
- Riprodotta regressione Function/import relativo; guard pubblico --no-compilation-cache
  risolve. Costo avvio/compilazione ancora da misurare. Cache moduli Deno resta attiva.
- Helper smoke56pass/8skip; nuove prove documento unico collegate; nuova scena snapshot e
  link esterni tipizzati ancora da consolidare/verificare. Nessuna prova browser del port.
- Batch pubblicato54ae generale13/13 e Androidverdi; non verificano il worktree.
- Backup sorgenti più recente target/task-0385/handoff-2026-10-28/manifest.json + diff +
  new-files. NON sovrascrivere il worktree con incomplete-cutover-2026-10-28/incomplete.patch.

### Storico0385 sotto: alcuni stati, backup e orari sono superati

## Decisione C3 confermata — 28 ottobre 2026

Accettato provvisoriamente il frammento `#roves-inline-module-N` SOLO per `import.meta.url`
dei moduli inline. Verificare due inline distinti, import relativi corretti e URL esterni
invariati. La risposta dell'utente chiude la richiesta di scelta, non prova l'integrazione.
Il worktree attuale include correzioni Codex successive al backup: non sovrascriverlo.

# Current direct Codex continuation - 28 October 2026

User explicitly authorized direct Codex implementation after Claude quota. Retry13:21 deleted;
no Claude active. Restored cutover draft is NOW in the worktree with further Codex corrections.
Do NOT overwrite it with the older saved patch. No partial source committed or published.

Actual Windows isolated results: cargo check --tests passes; full roves-js suite107passed,
2benchmarks ignored normal;106passed,3ignored JITless (2benchmarks+Wasm). Deno modules8/8 both
modes, document-adoption3/3 normal. Public C++ bindings added for UnboundScript::GetId,
Object::IsCallable/IsConstructor and ObjectTemplate::SetCallAsFunctionHandler; exact150
headers, no mangled-symbol guesses/private object layout. Actual linking/runtime now tested
by these native tests, not full Servo/browser or Linux.

Engine policy replaced the per-thread late guard with script-side preflight and constellation
defenses before navigation/iframe pipeline mutation; history range checked before drain.
Same-document fragment path preserved. Shared bounded category logging added. Syntax parses;
actual Servo browser execution remains CI. Development notice for new categories still to
wire. Classic-script entry now installs the Deno loader, including classic workers.

Next work/constraints are at the top of docs/V8_MIGRATION.md. In particular convert old
frame-dependent browser coverage without losing named-property/GC/referrer/error checks;
add actual single-document/HTML scheduling regression fixtures, update standalone locks,
complete native warning bridge and review source/compiler/module semantics. Heavy browser
builds online. No WIP publication, dependency/API benefits or overall retention claim.

---

Historical checkpoint below, superseded where it describes a parked draft or pending retry.

# 0385 - Current resume: unverified Deno cutover, native and policy corrections required

Latest28October2026 state: the runtime/module/async cutover was written but remains WIP.
Static-load pause and static evaluation unhandledrejection are BOTH ACCEPTED, with the
pause explicitly provisional and requiring the test review detailed below. No new fork,
public-layout shortcut, duplicate graph or general error-contract deviation was authorized.

## Saved implementation and exact scope

Production/CI sources restored to the verified backend after saving30 task-owned files
(26tracked+4new) to target/task-0385/incomplete-cutover-2026-10-28. Base b8fb2eb4904;
manifest.json records every file. incomplete.patch SHA256:
 b81d31d495be7492d596aba6ce05e02f57f052f0c660d738b30ec9df9039bdbc.
Git apply --check succeeds against restored source. Action README mirror saved separately
as action-readme.patch; its live checkout was restored. TODO.md is external and untouched.
Do not publish the saved patch as a completed integration or silently lose its work.

Code includes Deno owner/main-context adoption, public C++ API shim, module/async adapter,
static-load duration logging and provisional per-thread document refusal. The old module
source files are emptied, not yet removed; remove them only when actual replacement works.
Existing legacy tests were archived under cfg(any()); review replacement coverage rather
than accepting disabled tests as evidence of compatibility.

## Evidence already obtained - do not duplicate it

- Mechanical root lock resolved exactly one v8 package150.4.0, saved with the draft.
- Published v8 crate contains all four C++ headers; V8_PACKAGE_HEADERS.md records this.
- Actual isolated Windows cargo check --tests -j1 compiled the C++ shim, then failed34errors.
- A diagnostic COPY of tests.rs added explicit std::string::String/std::result::Result to
  deno_modules (jsapi glob imports shadowed them). This reduces errors to6: four public
  methods unbound by Rusty150, script_id used three times, is_callable, is_constructor and
  set_call_as_function_handler. native-check-types-fixed.log and NATIVE_REVIEW.md are exact
  evidence. These diagnostic test imports are NOT in the saved production source yet.
- Bind missing maintained PUBLIC V8 APIs through the shim or use truly equivalent public
  Rusty150 APIs; do not approximate callable objects as functions or guess mangled symbols.
- Native check is not a linked test, normal/JITless run, browser proof or performance result.

## Next assignments, one at a time

1. Restore the draft after checking base/manifest/hash. Resume same Claude context
   ab3e2eb3-6c0f-44ee-9277-de12c954a202 for the narrow native compatibility/test-name fixes.
   Codex verifies native check/tests normal/JITless in the existing isolated workspace;
   update copied source deliberately. Regenerate standalone crate locks (not only root).
2. Correct engine policy from target/task-0385/review-policy.txt. Current thread-local
   pre_page_load gate can be bypassed by newScriptThread/cross-origin navigation and can
   return after pipeline/history changes, leaving pending work. Enforce denial before
   fetch/pipeline/global creation/state mutation, with proper completion and bounded logs.
   Preserve initial load/necessary redirects/close, same-document History/hash/back-forward;
   no panic via a late null global. Add actual browser module/order/error/policy fixtures.
3. Review inline import.meta.url, classic-worker dynamic imports, source/referrer coverage,
   rejection callback integration, wake/spin and shutdown. Unapproved behavior regressions
   remain corrections to do; C2 does not authorize losing other game APIs.
4. Only after complete review/tests: remove dead services/files, generate real source patch,
   replay, update shared docs/README/wiki/action mirror and commit/push under required CI gate.
   Heavy browser builds online. No WIP publication.

## Actual quota and one-shot restart

New five-hour-session limit hit during the subsequent policy-correction request after the
successful00:02 restart and real implementation. /usage28October09:08 Europe/Rome:
session100%, LIVE reset13:19 (cached13:20 refreshed), week95% used. One retry28October13:21,
automation ripresa-unica-claude-0385-alle-13-21. No active Claude process. Delete that retry
after its attempt; if the same limit persists, save/stop without chained retries, model
change or another schedule for this block. No periodic polling. Old00:02 retry was deleted.

---

The design evidence below is retained for context; current execution status is ABOVE.

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
## Post-reset result: evaluation-error deviation (C2) explicitly ACCEPTED

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

The user explicitly ACCEPTED unhandledrejection instead of error/window.onerror for STATIC
MODULE EVALUATION errors. This unblocks C2; do not ask again or invent heuristic promise
classification to preserve the old error event. Overlays listening only to window.onerror
may miss these errors. Do not extend acceptance to classic-script errors, fetch/parse/link
failures or dynamic imports. Keep their distinct existing contracts and test them.

Preserve normal unhandledrejection/rejectionhandled dispatch and cancellation behavior;
Deno's internal rejection must not terminate the runtime. The promise may be internal to
Deno, not one the game created; do not claim otherwise. This is separate from the accepted
PROVISIONAL static-loading pause and its performance review. No fork was authorized.
Resume the completed Claude session with the accepted direction, reuse existing findings
and the saved seven-file preparation instead of repeating research. Deliver actual code.

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
