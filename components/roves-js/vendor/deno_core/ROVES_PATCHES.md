# Temporary deno_core0.413.0 snapshot

Authorized by the user on2026-10-29 for a game-blocking cached-TLA import bug.

- Published crate SHA256: `7ad7d5294f640fd99db08c455e2453c9860b1fa728b76fbdacf0eed3e308d48a`.
- Published source revision: `fe7ed012d92c642fd8e5607ac5a45d517538b5c5`, `libs/core` in denoland/deno.
- Fork: https://github.com/BlackRam-oss/deno
- Fix branch commit: `49a03aaf1ae59c864539c88f510ef0582073826e`.
- Issue: https://github.com/denoland/deno/issues/37097
- Draft PR: https://github.com/denoland/deno/pull/37098

## Semantic change 1: cached TLA dynamic import

`&& !module.is_graph_async()` in modules/map/dynamic.rs.
The general Deno evaluation-promise path handles async graphs; synchronous graphs retain
the shortcut. No Roves module graph or privileged game API is added.

## Semantic change 2: stepwise side module load (`SideModuleLoad`)

Added for the non-blocking static module graphs of task 0457 (the 0456 measurement showed
input handled ~21 s late during a delayed static fetch while `load_side_es_module` borrowed
the runtime). Not yet proposed upstream: an issue/PR is still to be opened, and this
change also needs its own removal path (an equivalent maintained public API, or Roves
returning to `load_side_es_module`).

- runtime/jsruntime.rs: public `SideModuleLoad` handle, `JsRuntime::start_side_es_module_load`
  and `JsRuntime::poll_side_es_module_load`. The handle owns the `RecursiveModuleLoad` and an
  `Rc` of the realm's existing module map, never a borrow of the runtime, so between polls
  the runtime runs its event loop, scripts and other loads on the same single module map.
  A poll compiles/registers at most one source, then polls once more to start its child fetches.
  If another step is already ready, it stores that step and schedules one cooperative wake so
  Servo returns to the networking task queue between cached/ready sources; if a fetch is truly
  pending, only that fetch wakes the task (no busy loop). The API does not pump the event loop
  itself and returns the same errors as `load_side_es_module[_from_code]`. Dropping the handle cancels its fetches
  (calling `finish_load` if started); it may outlive the runtime.
  `drive_es_module_load` now shares the extracted helpers `register_es_module_from_code`,
  `register_loaded_module` and `instantiate_loaded_root` with it; its behaviour is unchanged.
- runtime/mod.rs and lib.rs: re-export `SideModuleLoad`.
- API behavior is exercised by `components/roves-js/src/tests.rs` (borrow release, wake-ups,
  concurrency on the single map, inline roots, errors and handle lifetime); those tests are
  part of the root V8 normal/JITless CI. The vendor crate is excluded from that workspace.
- Contract not covered: loaders whose `pump_event_loop_during_load()` is true need their
  caller to run the event loop between polls; Roves' loader keeps the default (false).

The bounded cooperative-yield follow-up is kept separately as
`patches/deno_core-0.413.0/0458-cooperative-module-poll.patch`. It is a zero-context
diff; replay it after 0457 with `git apply --unidiff-zero`.

Roves callers: components/roves-js/src/deno_impl.rs (`start_module_graph`,
`poll_module_graph`, and the blocking `load_module_graph` driving the same handle).

Packaging-only differences: registry cache markers are omitted; examples/benchmarks and
their Cargo target declarations are omitted (including the unused wasm binary example).
The library sources, features and dependency declarations are otherwise the published
package. Cargo.toml.orig and upstream source metadata remain for auditing. LICENSE.md is
the upstream MIT license; upstream copyright headers are preserved.

Root workspace and standalone roves-js manifests apply this same path snapshot through
Cargo patch.crates-io; default crate features and a single V8 version stay unchanged.
Do not edit the Cargo registry/cache. New semantic changes need review, tests and an
upstream issue/PR (still missing for semantic change 2). This is temporary, not a permanent
substitute for maintained releases.

Removal checklist is in the root TODO.md: inspect the upstream fix and released crate,
remove both overrides and this snapshot, regenerate locks, and verify the root/descendant
success/rejection/cache matrix plus normal/JITless browser/security checks before removal.
