# 0382 — public-API bootstrap namespace removal before game code

The authorized27October19:02 retry succeeded; the one-shot
`ripresa-unica-claude-0382-alle-19-02` was deleted. No new quota/schedule or periodic polling.
The earlier quota/draft checkpoint remains historical in0382-bootstrap-resume.md.

## Implementation

In the isolated support/deno-core-probe, new_runtime creates the Deno runtime and then
removes Deno/__bootstrap/__infra using maintained V8 own-descriptor/delete APIs before any
game/module code. A non-configurable namespace is an explicit blocker, not hidden behind
a facade. The raw constructor is private and used only for the before/after observation.
No new dependency, context, private slot, FFI or production runtime change.

All existing module/static/dynamic/JSON/cycle/TLA/microtask/context-flag/mode scenarios run
after removal. The final scenario observes initial descriptors and removal results, then
asserts namespace absence from the first game statements, standard globals/FIFO jobs,
direct and aliased internal ext: imports rejected (including static import and
import.meta.resolve), no matching privileged extension ops and an empty resource table.
These are authored assertions: expected descriptor/message/count details await CI.
The same two-test/ten-logical-scenario suite runs normal/JITless on a single V8 test thread.

Local metadata/workflow/lock contract checks and Rust syntax parsing pass. Patch replay
must match changed paths after LF normalization. No heavy build or new runtime test was
executed locally. Baseline0381 CI40539608247 was4/4green; it does NOT validate0382.

## Limits and next proof

Absence of named globals is not a general privilege-isolation guarantee. Deno console/
queueMicrotask closures retain core references; internal resource/print/panic ops remain
registered, and import.meta.main is a Deno addition. No privileged fs/network/process
extensions are installed, but the heuristic op-name filter is only a bounded diagnostic,
not an exhaustive capability audit. Recheck this boundary after real DOM/console/task
integration. No Servo DOM/History/router or CSP implementation is simulated here.

Next separate assignment: prove a real native class/owned metadata and cppgc wrapper
identity/tracing through maintained APIs on the cleaned single-document runtime. Keep
that proof distinct from full Servo bindings integration. The module/async owner, HTML
checkpoint timing, per-source CSP/Trusted Types callback gap in V8150, compatible V8 line
and final reclamation remain adoption checks. Product navigation/warnings follow the
runtime choice; no bootstrap success alone authorizes exposing Deno capabilities.

Preceding19103 batch is now terminal and required green: deno probe4/4, general10/10
(Windows/Linux packaging+Steam), Androidsuccess. Apple excluded from the gate; general
Apple jobs also succeeded, but no iOS success is claimed. Publish only complete reviewed
work; collect new runs once, continue next assignment while CI runs, no further push
until that batch permits it. Local logs/prompt/results and old saved draft: target/task-0382/.
