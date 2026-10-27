# 0381 — isolated deno_core single-document probe

Implements the first experiment under0380's revised product scope: one game document,
internal History/TanStack-style routing, no required frames or document replacements.
This is not a production runtime or navigation-policy implementation.

## Delivered

support/deno-core-probe is a standalone Cargo workspace: deno_core0.413.0, copied locked
dependency graph (194 packages, rusty_v8 150.4.0 behind deno_v8 0.5.0), no root manifest or
lockfile changes. Public maintained APIs own module graphs and async driving. No DOM,
History API or router is mocked; URL allowlisting is not proof of HTML CSP/origin policy.

Ten logical scenarios in two Rust tests: pure mode validation and one entry point running
all nine V8-bearing scenarios on the same OS thread. This avoids treating libtest's
--test-threads=1 as a same-thread guarantee. Set V8_TEST_MODE=normal/jitless before the
first runtime, initialize once, create runtimes inside Tokio, fail on invalid/unset mode.
The first failed scenario stops later ones; no successful result is claimed for them.

Assertions cover mode/Wasm behavior, static/dynamic/JSON imports, cycles, TLA/microtask
ordering, classic-script checkpoints, import attributes/types, URL refusals, context-level
string-compilation switching and observed bootstrap globals. The old print-only main
program and second-context scenario were removed, not adopted into production.

New deno-core-probe.yml compiles/runs the same suite on Linux/Windows normal/JITless in
separate processes with an isolated target/cache and failure logs. No Apple workflow is
disabled; concurrency queues rather than cancelling. Heavy compilation runs online only.
roves-action README documents that this experiment changes no input, pin or bundle behavior.

## Review and evidence

Cargo metadata --no-deps --locked, Rust syntax parsing and maintained PyYAML workflow/
manifest/lock checks pass locally. Cargo.lock matches the previously resolved bytes.
Windows/Linux patch replay reproduced8/8paths after LF normalization. No heavy cargo
compilation or runtime scenarios ran locally. CI40539608247 now passes4/4 Linux/Windows
normal/JITless; Linux logs explicitly show both Rust tests pass with zero ignored/filtered.
No startup/RAM/CPU/GPU comparison or actual Servo DOM integration is claimed.
Local prompts, public source API review and logs: target/task-0381/.

Important correction: published rusty_v8 150.4.0 lacks Roves152's
ModifyCodeGenerationFromStringsResult/set_modify_code_generation_from_strings_callback.
The probe now uses the available Context::set_allow_generation_from_strings and reads
its state, asserting denial and subsequent allowance of eval/Function. This is a whole-
context switch, not a per-source CSP/Trusted Types callback or full security integration.
deno_v8's V8 backend reexports rusty_v8; no private API, new FFI or fork was added.

## Next bounded work

While the experiment CI runs, study/test whether public APIs can hide Deno/bootstrap/op
globals before any game code without breaking the same module/async scenarios. Do not
claim this solves CSP or native DOM ownership. Keep a single active Claude assignment;
keep complete subsequent commits local while required runs are active. Analyse all
non-Apple failures before the next push.

Remaining adoption checks: one compatible V8 line; real Servo classes/globals/cppgc and
DOM/events; one module/async owner with deletion of the replaced service; HTML checkpoint
timing; CSP/Trusted Types and privileged capability isolation; shutdown/reclamation. Policy
enforcement/warnings/external-browser handling follows the runtime decision, not this probe.
SourceText0361 and remaining legacy DOM/retention questions are still open.

Preceding0eb batch is terminal green for required checks: V8405281191797/7,
general4052811924810/10 including packaging and Steam, Android40528119174success.
Apple checks are excluded, not asserted green. It is safe to publish complete reviewed
work from this checkpoint, then identify the new runs and continue without polling.
