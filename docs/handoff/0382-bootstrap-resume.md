> Status update: the authorized27October19:02 retry succeeded. Claude completed the
> draft and Codex reviewed it; the one-shot automation was deleted. The quota below is
> historical, not a current stop/retry instruction. Current implementation/evidence:
> [bootstrap isolation](0382-deno-bootstrap-isolation.md). New CI still must validate it.

# 0382 — bootstrap isolation draft suspended at verified Claude session quota

## Current checkpoint

Published root19103d9ceffb493d53e348d9015bfdddac05fa66, wiki1d69714, roves-action4b2a67f.
The single-document deno_core probe0381 is compiled and green in CI40539608247: four jobs
(Linux/Windows normal/JITless), two Rust tests per job containing ten logical scenarios.
Linux normal/JITless logs explicitly show both tests pass with no ignored/filtered tests.
This proves the isolated module/async/context-flag scenarios, not Servo DOM, CSP, routing,
bootstrap privilege isolation or production adoption. Android40539608239 is green.
General40539608252 last aggregate: shared/probe/patch checks green, Windows/Linux packaging
and Steam still active, no non-Apple failure. Apple40539608230/macOS excluded, not green.

## Saved incomplete work

Claude began removing Deno/bootstrap/internal globals before game code and updating the
same module/async scenarios. It exhausted the session before a final report or review.
Do not call this complete or publish it. Saved:

- target/task-0382/incomplete-bootstrap.patch
- target/task-0382/wip/support/deno-core-probe/{src/lib.rs,tests/probe.rs,README.md}
- target/task-0382/prompt.txt, result.json (session_id for an authorized CLI resume), stderr.log
- target/task-0382/quota.txt

The three source files were restored byte-for-byte from published HEAD; only complete
0381 remains in publishable sources. The probe's previously authored /target/ .gitignore
is housekeeping, independent of this incomplete runtime experiment. TODO.md is a separate
user change and is not included in migration commits.

## Quota and single retry

Interactive Claude /usage at27October17:26 Europe/Rome confirms session100%, reset19:00.
No model request was sent through that command; the interactive CLI was exited.
One retry is scheduled27October19:02 Europe/Rome (reset plus2min), automation
`ripresa-unica-claude-0382-alle-19-02`, COUNT1, current thread. No intermediate polling.
Delete the retry after that attempt succeeds or is blocked. If the same limit persists,
record it and stop without model switching, chained retries or another schedule for this
block. No periodic automation and no CLAUDE_RESUME.md.

## Resume assignment

Inspect Git and the saved draft first; resume only this bootstrap task with Claude via
Read/Edit/Write/Grep/Glob, no denied shell retries, commits/push or subagents. If applying
the saved diff, review conflicts and preserve newer source changes rather than force it.
Public APIs only; no facade/private slots/fork. Verify actual configurable descriptors,
namespace absence before the game, standard globals and module/async functionality after
removal, and direct/aliased ext:core/mod.js/ext:core/ops import refusal. Distinguish lack of
named globals from universal privilege isolation and full CSP. Heavy builds remain online.

Finish/review the implementation and tests, update patch/CUSTOMIZATIONS/README/wiki/handoff
once complete, commit locally. Check all current required CI jobs at that point. No push
while any required non-Apple job is active or has unresolved failures. Continue the loop
if quota is available; next separate proof concerns real class/cppgc ownership. Navigation
policy and notices remain after the runtime decision: single document with internal routes,
development-only non-modal notices/released logs, external browser only after real click.
