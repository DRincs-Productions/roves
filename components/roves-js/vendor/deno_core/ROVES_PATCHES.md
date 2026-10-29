# Temporary deno_core0.413.0 snapshot

Authorized by the user on2026-10-29 for a game-blocking cached-TLA import bug.

- Published crate SHA256: `7ad7d5294f640fd99db08c455e2453c9860b1fa728b76fbdacf0eed3e308d48a`.
- Published source revision: `fe7ed012d92c642fd8e5607ac5a45d517538b5c5`, `libs/core` in denoland/deno.
- Fork: https://github.com/BlackRam-oss/deno
- Fix branch commit: `49a03aaf1ae59c864539c88f510ef0582073826e`.
- Issue: https://github.com/denoland/deno/issues/37097
- Draft PR: https://github.com/denoland/deno/pull/37098

The sole semantic change is `&& !module.is_graph_async()` in modules/map/dynamic.rs.
The general Deno evaluation-promise path handles async graphs; synchronous graphs retain
the shortcut. No Roves module graph or privileged game API is added.

Packaging-only differences: registry cache markers are omitted; examples/benchmarks and
their Cargo target declarations are omitted (including the unused wasm binary example).
The library sources, features and dependency declarations are otherwise the published
package. Cargo.toml.orig and upstream source metadata remain for auditing. LICENSE.md is
the upstream MIT license; upstream copyright headers are preserved.

Root workspace and standalone roves-js manifests apply this same path snapshot through
Cargo patch.crates-io; default crate features and a single V8 version stay unchanged.
Do not edit the Cargo registry/cache. New semantic changes need review, tests and an
upstream issue/PR. This is temporary, not a permanent substitute for maintained releases.

Removal checklist is in the root TODO.md: inspect the upstream fix and released crate,
remove both overrides and this snapshot, regenerate locks, and verify the root/descendant
success/rejection/cache matrix plus normal/JITless browser/security checks before removal.
