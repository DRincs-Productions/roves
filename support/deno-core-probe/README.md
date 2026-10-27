# deno_core single-document probe

Isolated feasibility experiment for running the one Roves game document on
[deno_core](https://crates.io/crates/deno_core) (MIT, maintained by the Deno authors),
following [docs/handoff/0380-single-document-runtime.md](../../docs/handoff/0380-single-document-runtime.md).
It is **not** an adoption, not a production dependency and not wired into the engine: the
crate is its own Cargo workspace, nothing in the Roves workspace depends on it, and no game
API, Deno extension or capability (file system, network, process) is exposed.

## Scope

One document = one `JsRuntime` and its main realm. Additional realms, frames, tabs and
WindowProxy identity across replaced documents are out of scope for the product, so the
earlier secondary-context scenario (task 0378) was removed: it needed realm hooks deno_core
does not make public, and the product no longer requires them.

Scenarios in `tests/probe.rs`. The task 0381 baseline (without namespace removal) passed in
CI run 40539608247 (4/4 jobs: Ubuntu and Windows, normal and JITless). The task 0382 changes
below (namespace removal, `bootstrap_namespace_is_removed_before_game_code`) have not been
compiled or run yet. All go
through public deno_core APIs and its `v8` re-export, with an in-memory `ModuleLoader`
serving `https://game.invalid/`. Those URLs are a mock: the scenarios run modules and
scripts directly, with no HTML document, no HTML origin and no CSP header behind them.

Game code only ever runs on `new_runtime`, which removes deno_core's bootstrap namespace
globals (`Deno`, `__bootstrap`, `__infra`) with the native `v8::Object` API right after
`JsRuntime::try_new` and before any script or module; a non-configurable global is reported
as a blocker instead of being hidden. So every scenario below runs on the stripped global.
The raw constructor is private and used only for the before/after observation.

The 10 logical scenarios are grouped in **2 Rust tests**:
`mode_values_are_validated` (pure, no V8) and `single_document_scenarios`, which runs the
other nine in a fixed order on one thread, so V8 initialization and every `JsRuntime` stay
on that thread (`--test-threads=1` alone serializes tests but may still run each on its own
thread). The first failing scenario stops the rest; they are not 10 independent tests.

| Scenario | Checks | Does not prove |
|---|---|---|
| `mode_values_are_validated` | `V8_TEST_MODE` accepts only `normal`/`jitless`; unset or other values are errors | — |
| `configured_mode_controls_webassembly` | V8 was initialized in the requested mode: WebAssembly compiles in normal mode and does not in JITless mode | JITless performance or every JIT-dependent feature |
| `static_imports_json_module_and_cycle_evaluate_in_spec_order` | Static imports, a JSON module, an a↔b cycle | Fetch, CORS, MIME sniffing |
| `top_level_await_suspends_only_its_dependents` | TLA ordering and FIFO microtasks after module evaluation | HTML checkpoint placement in nested cases |
| `classic_scripts_leave_the_microtask_checkpoint_to_the_embedder` | `execute_script` does not drain microtasks; the embedder checkpoints | Order against Servo tasks; module-path checkpoints |
| `dynamic_import_of_javascript_and_json` | `import()` of JS and JSON from a classic script | Promise-job order against Servo tasks |
| `import_attributes_and_module_types_follow_html_rules` | Unsupported attributes/types and type mismatches reject (validator callback + loader check) | CSS module scripts |
| `only_allowlisted_urls_reach_the_loader` | Other origin/port, `data:`, `file:`, `ext:`, `node:`, bare specifiers never reach `load` | Servo fetch, CORS, CSP `script-src`, SRI, import maps |
| `context_flag_switches_string_compilation_off_and_on` | With the context flag off, `eval`/`new Function` throw `EvalError` while embedder-compiled scripts still run; with it on, both work | Per-string decisions, CSP `unsafe-eval` reporting, Trusted Types: the flag is context-wide, not a CSP implementation |
| `bootstrap_namespace_is_removed_before_game_code` | Raw deno_core has `Deno`/`__bootstrap` as configurable data properties (`__infra` already gone); after removal the game's first statements see none of them, no global value re-exposes `core`/`ops`/`primordials`/`internals`, `Object`/`Promise`/`queueMicrotask` work; `ext:core/mod.js` and `ext:core/ops` are refused by direct `import()`, by `import.meta.resolve`, and — when a loader alias maps a game specifier onto them — by deno_core's own `ext:` guard for dynamic and static imports; no fs/net/fetch/process-style op is registered and the resource table is empty | General privilege isolation: deno_core's builtin closures (`console`, `queueMicrotask`) still hold the core internally, builtin resource ops (`op_read`, `op_print`, ...) stay registered though unreachable from game code, `import.meta.main` is a Deno extra; no Servo DOM, origin or CSP |

There is **no Servo DOM, no History API and no TanStack-style router** here, and none is
simulated: nothing in this crate proves that Servo's DOM, events or same-document routing
work on deno_core. The navigation/external-link policy of handoff 0380 is not implemented
either; it follows the runtime choice.

## Running

V8 is initialized exactly once per process, before any `JsRuntime`, in the mode named by
`V8_TEST_MODE` (required). Run each mode in its own process, single-threaded:

```sh
V8_TEST_MODE=normal  cargo test --locked --manifest-path support/deno-core-probe/Cargo.toml -- --test-threads=1
V8_TEST_MODE=jitless cargo test --locked --manifest-path support/deno-core-probe/Cargo.toml -- --test-threads=1
```

The build downloads the prebuilt rusty_v8 static library; it is heavy, so it runs online in
`.github/workflows/deno-core-probe.yml` (Ubuntu 24.04 and Windows, normal and JITless, no
browser executable needed). No result is claimed until that workflow has run; see
`target/task-0381/RESULTS.md` and `target/task-0382/RESULTS.md` for what was and was not
executed.

## Engine version gap

`deno_core 0.413.0` depends on `deno_v8 0.5.0` (aliased as `v8` inside deno_core), which
resolved to `v8 150.4.0` in `Cargo.lock`. Roves pins `v8 = "=152.2.0"`. This probe never
imports the engine's `v8` crate; it uses `deno_core::v8` only. Adoption would mean one V8
line for the whole engine (deno_core's), not two copies side by side.

## Remaining public-API checks before any adoption

Each needs a bounded proof using existing maintained APIs, not a new framework:

1. **DOM classes, globals and cppgc ownership.** Servo's DOM objects on deno_core's cppgc
   (`deno_core::cppgc`) with Roves-owned class metadata: the existing Roves class fixtures
   need owned metadata, prototype chains and wrapper identity, traced by V8's GC.
2. **Module and async single owner.** deno_core must replace Servo's module map and
   dynamic-import service and the Promise job queue for the document, not run beside them;
   the old path is removed in the same change.
3. **Bootstrap globals and capabilities.** Namespace removal is authored
   (`bootstrap_namespace_is_removed_before_game_code`, awaiting CI). The absence of these
   globals is not a universal capability proof: privilege isolation still has to be
   re-checked once Servo's DOM globals, console and event loop replace deno_core's
   builtins, and production integration is pending.
4. **HTML checkpoint timing, security and teardown.** Microtask checkpoints interleaved with
   Servo tasks per HTML, CSP/codegen decided by Servo's policy, clean isolate teardown at
   shutdown, all in normal and JITless mode.
5. **Per-string code-generation callback.** Roves today decides `eval`/`Function` per
   source string through V8's modify-code-generation callback (CSP, Trusted Types). The v8
   150.4.0 crate deno_core resolves to exposes no such callback (only the context flag
   above and the Wasm code-generation callback), so this is a remaining integration
   difference: a context-wide switch cannot express "allowed for this TrustedScript, denied
   for that string".

No CPU, RAM, startup or binary-size figure has been measured.
