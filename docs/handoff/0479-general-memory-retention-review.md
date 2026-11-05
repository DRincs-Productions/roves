# V8 migration handoff: general memory retention

Updated: 2026-11-05

This handoff is focused on the remaining general-memory investigation. The authoritative
migration status remains at the top of `docs/V8_MIGRATION.md`.

## Repository and CI checkpoint

- The evidence and CI summarized here are based on code commit `863cebaa82a` (`docs: record post-settled retention analysis`).
- V8 fix commit `f5222db619f` passed run `42025623358` (8/8 jobs).
- Android `42025623379` passed (3/3); iOS `42025623456` passed signing smoke, with the simulator skipped by policy.
- Documentation checkpoint `863cebaa82a`: Android `42038252711` and iOS `42038252707` both passed; iOS simulator skipped by policy.
- Nested `roves-action` and `roves-wiki` worktrees have separate local modifications; preserve them and do not reset or include them without inspecting ownership.

## What the latest memory evidence establishes

Task 0478 added an explicit post-settlement collection and snapshot 34 to the existing
`document-retention.html` series (snapshots 2/16/32 remain during the workload). MemLab 2.0.6
was used offline; no dependency was added.

The first post-settlement capture exposed a test-fixture root: its global `main` function
retained its own closure through `Window.main`. Commit `f5222db619f` changed the fixture to an
unbound async function expression and added a runner assertion that `typeof main` is undefined.
The corrected CI snapshot no longer reports the `main` closure among growing objects.

For the corrected artifact, MemLab reports no increasing collections. The `RovesJsCell` counts
are 128/129/129/126 at collections 2/16/32/34. The fixture's 128 temporary element and probe
WeakRefs pass their release checks while the deliberate control probe remains. The Window
retained size changes by about 2.1 KiB between collections 32 and 34; this single bounded scene
does not identify a persistent owner or prove a leak. Snapshot hashes and exact byte counts are
in `docs/measurements/0478-document-retention-post-settled-ci.json`.

## What remains open

This proves only the controlled DOM create/remove/listener fixture. It does **not** establish
absence of general JavaScript retention or cover cppgc, native allocations, repeated real route
transitions, or long-running game sessions. Do not mark overall retention resolved or claim no
leaks based on this result.

The other two previously tracked areas are not open migration gates: window policy and close
during a pending static fetch passed their required checks; real-scene pause/fluidity review
belongs to the separate performance flow and is excluded from V8 completion. SourceText and
JSON modulepreload remain deferred TODO items.

## Next step

Choose one concrete, reproducible memory-retention scenario outside the current bounded DOM
fixture. Before implementation, check existing tests and maintained analysis libraries so work
does not duplicate `gc-lifecycle.html` or the 0478 series. Prefer V8/MemLab public maintained
APIs over a custom heap parser or graph traversal. Keep artifacts and logs under ignored
`target/`, add only assertions supported by repeatable evidence, and update the migration
handoff and measurement record. Run lightweight checks locally; keep heavy builds in CI. No
user interaction or performance measurement is needed for this handoff.
