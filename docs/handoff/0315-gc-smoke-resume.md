# Archived draft — superseded by completed0315

The authorized18:45 retry succeeded. Production fix is
patches/servo-v0.5.0/0315-gc-smoke-entry-point.patch; verification/current status is in
../V8_MIGRATION.md. Do NOT apply the incomplete archive over current sources. The old
instructions below are historical evidence of the quota interruption, not current work.

# Task0315 — incomplete Claude draft, 23 October 2026

Current published source8e4fb2e6cf4; V8 run39973655294 fails both Linux smoke jobs because
main() is called but never defined in gc-lifecycle.html after0314. General/Android/iOS green.
Codex delegated the complete fix. Claude session limit ended the invocation before tests,
negative verification, patch replay or documentation completion. CLI reports reset18:40
Europe/Rome23October. Do not retry automatically or switch model to bypass quota.

Draft0315-incomplete-gc-smoke-main.patch is an archive, NOT a production patch. Sources were
restored to published state; no incomplete source is staged. On explicit user resumption:
review/apply this draft, restore full previous GC sequence while preserving0314 diagnostics,
mandatory run.sh markers and cleanup entry settings. Evaluate the Node entry regression:
it exercises the missing-TestUtils failure path, not real browser collection; add any
necessary checks without weakening smoke requirements. Run existing Python runner tests,
prove missing-main negative fails, restore byte-identical production source. Generate actual
next numbered production patch only once complete, verify pristine+prior replay, update
CUSTOMIZATIONS/README/wiki/handoff and deliver diff/checks/logs. Heavy builds online.
No commit/push by Claude. Codex reviews then publishes the complete fix, since prior batch
is terminal but red and next push must contain the correction. Do not recreate CLAUDE_RESUME.

Ignored logs/prompt (not required to understand this archive): target/task-0315/prompt.txt,
claude-result.json, claude-stderr.log. No tests or replay completed for this draft.
