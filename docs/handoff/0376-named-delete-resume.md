# 0376 — named deletion assignment saved at the session limit

No implementation started: Claude's first request returned the session quota limit;
working sources remained clean. Interactive /usage on27October11:46 Europe/Rome confirms
session100% used and reset14:00. One retry14:02 is scheduled as
ripresa-unica-claude-quota-0376; delete it after the attempt. Do not chain retries if the
same limit persists. The earlier0347 retry was completed and deleted.

## Next bounded assignment

Fix only deletion of visible non-indexed named properties without a named deleter:
CGDOMJSProxyHandler_delete currently delegates directly to expando-only deletion, returning
true while a synthetic Document/Form property remains. Reuse maintained WebIDL/codegen,
existing own/prototype helpers and ObjectOpResult::fail_cant_delete (already implemented).
Own/unforgeable properties must short-circuit to ordinary deletion; supported visible names
without a deleter must return false (sloppy/Reflect) or TypeError in strict JS. Hidden names,
missing names and symbols retain ordinary behavior. Existing named deleters such as dataset
and indexed deletion are separate, unchanged branches for this assignment.

Do not invoke NamedGetter merely to query support: Form's getter can modify its past-names
map. Inspect the maintained DOM implementation and reuse SupportedPropertyNames metadata
with DOMString/id conversion, preserving support/own checks before prototype callbacks.
A fresh-form rename-after-delete regression should detect accidental past-name registration;
a real property read must retain its legitimate existing behavior. Explain any bounded list
allocation/search cost rather than claim a performance saving. No new cache/dependency.

Extend real-parser/generator tests and required normal/JITless browser smoke; no heavy local
build. Full local prompt and usage evidence: target/task-0376/. No source WIP to restore.
Publisheddac65a4dbc8/wiki bc80b3c; completed local0375 commit68d687ce5ae/wiki86cbaf5 must
wait for the preceding non-Apple CI batch. Current batch IDs/status are in V8_MIGRATION.md.
SourceText0361 and overall retention remain open. Patch0376 is this document only.
