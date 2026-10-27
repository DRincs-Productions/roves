//! Assertions for the single-document deno_core probe. The same suite runs once per V8 mode,
//! in separate processes: `V8_TEST_MODE=normal` and `V8_TEST_MODE=jitless`, each with
//! `--test-threads=1`. Expected orders follow ECMA-262 module evaluation and HTML's
//! module-type rules; each scenario names what it does *not* prove.
//!
//! libtest may run each `#[test]` on its own OS thread even with `--test-threads=1`, so every
//! V8-bearing scenario runs from the single `single_document_scenarios` entry point: V8 is
//! initialized (`v8_mode`) and every `JsRuntime` is created and dropped on that one thread.
//! Only the pure `parse_mode` checks are a separate test.

use roves_deno_core_probe as probe;
use roves_deno_core_probe::Mode;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn mode_values_are_validated() {
    assert_eq!(probe::parse_mode(Some("normal")), Ok(Mode::Normal));
    assert_eq!(probe::parse_mode(Some("jitless")), Ok(Mode::Jitless));
    assert!(probe::parse_mode(Some("JITLESS")).is_err());
    assert!(probe::parse_mode(Some("")).is_err());
    assert!(probe::parse_mode(None).is_err());
}

/// The eleven V8-bearing scenarios, in a fixed order, on one thread. The first one initializes
/// V8 in the configured mode; a failure stops the remaining scenarios. Every scenario except
/// the "before" half of the bootstrap one runs after deno_core's bootstrap globals were
/// removed (`new_runtime`, or the same path plus a fixture extension).
#[test]
fn single_document_scenarios() {
    configured_mode_controls_webassembly();
    static_imports_json_module_and_cycle_evaluate_in_spec_order();
    top_level_await_suspends_only_its_dependents();
    classic_scripts_leave_the_microtask_checkpoint_to_the_embedder();
    dynamic_import_of_javascript_and_json();
    import_attributes_and_module_types_follow_html_rules();
    only_allowlisted_urls_reach_the_loader();
    context_flag_switches_string_compilation_off_and_on();
    bootstrap_namespace_is_removed_before_game_code();
    native_class_instances_own_their_values_and_share_owned_metadata();
    native_async_op_completes_or_is_dropped_with_the_runtime();
}

fn configured_mode_controls_webassembly() {
    // Fails here, with the reason, when V8_TEST_MODE is unset or invalid.
    let mode = probe::v8_mode().unwrap();
    let wasm = probe::webassembly_availability().unwrap();
    match mode {
        Mode::Normal => assert_eq!(wasm, "compiled"),
        // V8 has no WebAssembly without its JIT: either absent or refusing to compile.
        Mode::Jitless => assert_ne!(wasm, "compiled"),
    }
}

fn static_imports_json_module_and_cycle_evaluate_in_spec_order() {
    // Cycle a -> b -> a: b evaluates first, then a, then main.
    let log = probe::static_json_and_cycle().unwrap();
    assert_eq!(log, strings(&["b", "a:function:A", "main:A:42"]));
}

fn top_level_await_suspends_only_its_dependents() {
    let log = probe::top_level_await_and_microtasks().unwrap();
    assert_eq!(
        log,
        strings(&["slow:start", "fast", "slow:end", "main", "main:micro", "main:queueMicrotask"])
    );
}

fn classic_scripts_leave_the_microtask_checkpoint_to_the_embedder() {
    // Not proven: that deno_core's *module* paths respect HTML's "only when the JS execution
    // context stack is empty" rule, or the order against Servo tasks.
    let (before, after) = probe::classic_script_checkpoint().unwrap();
    assert_eq!(before, strings(&["sync"]));
    assert_eq!(after, strings(&["sync", "micro"]));
}

fn dynamic_import_of_javascript_and_json() {
    let log = probe::dynamic_import().unwrap();
    assert_eq!(log.first().map(String::as_str), Some("after-import-calls"));
    let position = |entry: &str| log.iter().position(|item| item == entry).unwrap_or_else(|| panic!("{entry} in {log:?}"));
    assert!(position("dyn:evaluated") < position("dyn:7"));
    position("json:42");
    assert_eq!(log.len(), 4, "{log:?}");
}

fn import_attributes_and_module_types_follow_html_rules() {
    let log = probe::import_attribute_policy().unwrap();
    assert_eq!(log.len(), 5, "{log:?}");
    for label in ["bytes-type", "js-as-json", "json-without-type", "text-type", "unknown-key"] {
        assert!(log.iter().any(|entry| entry.starts_with(&format!("{label}:rejected"))), "{label} in {log:?}");
    }
}

fn only_allowlisted_urls_reach_the_loader() {
    // Not proven: Servo's fetch, CORS, CSP `script-src` or SRI. The allowlist stands in for
    // the place where those checks would be called.
    let report = probe::resolution_allowlist().unwrap();
    assert!(report.log.contains(&"same-origin:fulfilled".to_string()), "{:?}", report.log);
    assert_eq!(report.log.iter().filter(|entry| entry.contains(":rejected")).count(), 7, "{:?}", report.log);
    assert_eq!(report.loads, strings(&["https://game.invalid/ok.js"]));
    let refused: std::collections::BTreeSet<_> = report.refused.iter().collect();
    assert_eq!(refused.len(), 7, "{:?}", report.refused);
    assert!(!report.pwned);
}

fn context_flag_switches_string_compilation_off_and_on() {
    // Context-wide switch only. Not proven: per-string decisions (no code-generation
    // callback in v8 150.4.0), CSP `unsafe-eval` reporting or Trusted Types.
    let (denied_flag, denied, allowed_flag, allowed) = probe::string_compilation_context_flag().unwrap();
    assert!(!denied_flag);
    assert_eq!(denied, strings(&["eval:EvalError", "Function:EvalError", "script:ran"]));
    assert!(allowed_flag);
    assert_eq!(allowed, strings(&["eval:allowed", "Function:allowed", "script:ran"]));
}

fn bootstrap_namespace_is_removed_before_game_code() {
    // Not proven: general privilege isolation. Closures inside deno_core's builtins (console,
    // queueMicrotask) still hold the core internally, `import.meta.main` is a Deno extra, and
    // there is no Servo DOM, HTML origin or CSP here. This pins the absence of the namespace.
    let report = probe::bootstrap_isolation().unwrap();

    // Raw deno_core: plain configurable data properties (a non-configurable one would have
    // failed `bootstrap_isolation` with a "blocker:" error instead).
    assert_eq!(
        report.before,
        strings(&["Deno:data:configurable", "__bootstrap:data:configurable", "__infra:absent"])
    );
    assert_eq!(report.removal, strings(&["Deno:deleted", "__bootstrap:deleted", "__infra:absent"]));

    let log = &report.log;
    let position = |entry: &str| log.iter().position(|item| item == entry).unwrap_or_else(|| panic!("{entry} in {log:?}"));
    // The game's first statements already see a global without the namespace.
    assert_eq!(
        log[..3],
        strings(&["Deno:undefined:false", "__bootstrap:undefined:false", "__infra:undefined:false"]),
        "{log:?}"
    );
    position("Object:a");
    position("namespace-leaks:");
    position("privileged-globals:");
    position("import.meta:main,resolve,url");
    position("meta-resolve:TypeError");
    // Standard Promise jobs and queueMicrotask still run, in FIFO order, right after the module.
    let sync_done = position("main:sync-done");
    assert_eq!(position("promise:p"), sync_done + 1, "{log:?}");
    assert_eq!(position("queueMicrotask:ran"), sync_done + 2, "{log:?}");

    // Direct internal specifiers stop at the loader's allowlist; aliases that a loader maps
    // onto them are refused by deno_core's own ext: guard, dynamic and static alike.
    position("ext-mod:rejected:other");
    position("ext-ops:rejected:other");
    position("alias-ops:rejected:core-guard");
    position("alias-mod:rejected:core-guard");
    assert!(log.iter().any(|entry| entry.starts_with("static-alias:rejected:")), "{log:?}");
    assert!(!log.iter().any(|entry| entry.contains(":fulfilled") || entry.starts_with("static-alias:evaluated")), "{log:?}");
    assert_eq!(log.len(), 16, "{log:?}");

    assert_eq!(report.loads, strings(&["https://game.invalid/main.js", "https://game.invalid/static-alias.js"]));
    let refused: std::collections::BTreeSet<_> = report.refused.iter().map(String::as_str).collect();
    assert_eq!(refused, ["ext:core/mod.js", "ext:core/ops"].into_iter().collect(), "{:?}", report.refused);
    assert!(report.privileged_ops.is_empty(), "{:?}", report.privileged_ops);
    assert_eq!(report.resources, 0);
}

fn native_class_instances_own_their_values_and_share_owned_metadata() {
    // Not proven: Servo DOM bindings, Web IDL conversions, cross-realm wrappers, inheritance,
    // or collection *before* teardown (V8's public GC entry points do not promise it; see
    // README.md). Identity checks alone would be a JS tautology: the Rust-side `Rc`/`Weak`
    // counts and the cppgc finalizer counters are what tie the wrappers to native ownership.
    let report = probe::native_class_ownership().unwrap();

    assert_eq!(
        report.log,
        strings(&[
            "same-proto:true",
            "constructor:NativeClass",
            "instanceof:true",
            "own-props:0",
            "accessors:function,function,function,function",
            "kind:NativeClass:NativeClass",
            "value:alpha:beta",
            "child:alpha-child:beta-child",
            "info-stable:true",
            "info-distinct:true",
            "distinct:true",
            // Native brand check: a wrapper-less receiver cannot reach a native value, and the
            // exposed constructor does not mint native objects.
            "foreign-receiver:threw",
            "constructed-receiver:threw",
        ])
    );

    // The creator's own Rc is gone; exactly the two native instances keep the metadata alive.
    assert_eq!(report.meta_strong_after_creator_drop, 2);
    // A full GC with both wrappers reachable finalizes nothing: wrappers keep their native
    // objects, and each parent's traced Member keeps its wrapper-less child.
    assert_eq!(report.drops_while_reachable, [0, 0, 0]);
    assert_eq!(report.after_gc, "NativeClass/alpha/alpha-child,NativeClass/beta/beta-child");
    // After the roots are deleted, collection before teardown is allowed but not required.
    for (count, total) in report.drops_after_unroot.iter().zip([2, 2, 2]) {
        assert!(*count <= total, "{:?}", report.drops_after_unroot);
    }
    // Dropping the runtime finalizes every native object once, and with them the metadata.
    assert_eq!(report.drops_after_teardown, [2, 2, 2]);
    assert!(!report.meta_alive_after_teardown);
}

fn native_async_op_completes_or_is_dropped_with_the_runtime() {
    // Not proven: cancellation of real resources (I/O, timers, DOM work), Servo task ordering,
    // abort signals, or ordering against unrelated futures. One fixture op on deno_core's
    // driver; the Rust-side counters, not the JS log alone, show that a native future ran.
    let report = probe::native_async_op_lifecycle().unwrap();
    let counts = |started, resumed_after_gate, completed, cancelled, payloads_dropped| probe::OpCounts {
        started,
        resumed_after_gate,
        completed,
        cancelled,
        payloads_dropped,
    };

    // Exactly one game-visible entry point; no bootstrap namespace, no op by name.
    let head = strings(&[
        "bootstrap-globals:",
        "fixture:function:nativeFixture",
        "returns-promise:true",
        // deno_core's stub tags pending op promises with a Symbol.for key (internal detail).
        "internal-id-symbol:true",
        "script:returned",
    ]);
    // The classic script returned with both futures polled once and suspended in Rust.
    assert_eq!(report.completion_log_after_script, head);
    assert_eq!(report.completion_counts_after_script, counts(2, 0, 0, 0, 0));
    assert!(report.fixture_op_registered);
    assert!(report.privileged_ops.is_empty(), "{:?}", report.privileged_ops);

    // The script's own Promise job runs first; the op callbacks only after their futures
    // completed on a later event loop turn. The two ops' relative order is not asserted.
    let log = &report.completion_log;
    assert_eq!(log[..head.len()], head[..], "{log:?}");
    assert_eq!(log[head.len()], "promise-job", "{log:?}");
    let position = |entry: &str| log.iter().position(|item| item == entry).unwrap_or_else(|| panic!("{entry} in {log:?}"));
    assert!(position("ok:fulfilled:native:alpha") < position("ok:chained"), "{log:?}");
    position("fail:rejected:BadResource:fixture failed: beta");
    assert_eq!(log.len(), head.len() + 4, "{log:?}");
    // Both guards saw a result (Ok and Err alike), both owned payloads were freed.
    assert_eq!(report.completion_counts, counts(2, 0, 2, 0, 2));

    // A rejection with no handler surfaces as the event loop's error.
    let error = report.unhandled_error.as_deref().unwrap_or_else(|| panic!("unhandled rejection was swallowed"));
    assert!(error.contains("fixture failed: unhandled"), "{error}");
    assert_eq!(report.unhandled_counts, counts(3, 0, 3, 0, 3));
    assert!(!report.completion_payload_alive_after_teardown);

    // Pending: first poll happened (started), one public event loop tick ran the Promise job
    // and stayed pending; no game callback, the payload is alive.
    assert_eq!(report.pending_counts_before_poll, counts(1, 0, 0, 0, 0));
    assert!(report.pending_poll_was_pending);
    assert_eq!(report.pending_log, strings(&["script:returned", "promise-job"]));
    assert_eq!(report.pending_counts_after_poll, counts(1, 0, 0, 0, 0));
    assert!(report.pending_payload_alive_before_drop);

    // Dropping the runtime dropped the suspended future: guard cancelled, payload freed, and
    // the runtime kept no clone of the gate or the counters.
    assert_eq!(report.cancel_counts_after_drop, counts(1, 0, 0, 1, 1));
    assert!(!report.cancel_payload_alive_after_drop);
    assert_eq!(report.gate_strong_after_drop, 1);
    assert_eq!(report.counters_strong_after_drop, 1);
    // Notifying the gate afterwards resumes nothing.
    assert_eq!(report.counts_after_gate_release, counts(1, 0, 0, 1, 1));
}
