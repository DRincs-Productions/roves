# 0377 — named visibility queries own descriptors on prototypes

Use the maintained JS_GetPropertyDescriptorById operation, starting at the object's
prototype, instead of JS_HasPropertyById. Existing V8-backed traversal asks own descriptors
and follows prototypes without invoking accessors. No new traversal, cache, dependency or
public API. Null prototypes and pending exceptions retain their existing handling.

Affected visibility callers are generated getOwnPropertyDescriptor/hasOwn, ownKeys,
named deletion and WindowProperties. Actual property-value reads are unchanged. Adapted
browser fixtures retain reentrant expando, duplicate-key, own-property short-circuit and
numeric-index gating coverage, now through descriptor traps; has traps must remain unused.
The new required fixture covers undefined/accessor descriptors, inherited properties,
opposite has/descriptor answers, null chains and exceptions from both descriptor/prototype
traps. Query order for all ownKeys names is not asserted; query multiplicity is.

Local validation: one new native contract passes normal and JITless; runner42 passes
(8 POSIX skips on Windows), generator14 passes, utils Rust syntax parses. An isolated
negative replacing the descriptor query with the former HasProperty operation fails on
the actual `has:undefinedKey` versus `descriptor:undefinedKey` trap difference. The mutated
isolated source was restored; tracked sources were never mutated for that negative.
Browser compilation/execution remains CI-only. Source/logs in target/task-0377/.

Tradeoff: descriptor objects/conversion and two additional stack roots replace a boolean
presence query; no measured CPU/RAM improvement is claimed. The generic traversal still
does not skip named-properties objects inserted in custom chains, and WindowProperties'
starting point is unchanged. These remain explicit gaps. SourceText0361, indexed deletion,
property definition and overall retention are not resolved by this task.

Preceding735a batch: V840520566885 required7/7 green; Android40520566920 green.
General40520566879 probes/shared checks pass, but macOS probe cancellation caused all
build-and-publish and Steam jobs to skip through the aggregate dependency. Required
Windows/Linux packaging/Steam coverage is therefore missing, not green. Apple checks
remain excluded; repair the dependency coupling without disabling Apple workflows.
