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

The user explicitly selected OPTION1 after the review: a temporary ScriptThread pause during
static graph loading/instantiation is accepted; dynamic imports remain asynchronous. This
unblocks B3. Preserve HTML evaluation timing and TLA semantics; the authorization does not
permit waiting for TLA settlement before progressing deferred scripts/DOMContentLoaded.
Network completion must not depend on reentering JS tasks while JsRuntime is exclusively
borrowed. Reuse maintained Deno/Tokio plus Servo's existing network security policy.

Claude resumed actual implementation using target/task-0385/approved-static-load.txt.
The earlier diagnosis below remains evidence of why the tradeoff was presented, not a
current request for approval. No upstream issue/message was sent; no fork or second graph
authorized. Production integration is still pending code/review/necessary checks.

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
followup.txt and v8-public headers. The initial diagnosis was delivered; a new implementation assignment is active.
No quota block, automation or scheduled retry. Chrome MCP new_page was denied to Claude;
not retried. No Chrome measurements or executed B3 counterexamples are claimed.
