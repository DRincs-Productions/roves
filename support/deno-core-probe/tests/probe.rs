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

/// The nine V8-bearing scenarios, in a fixed order, on one thread. The first one initializes
/// V8 in the configured mode; a failure stops the remaining scenarios.
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
    main_realm_exposes_deno_globals();
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

fn main_realm_exposes_deno_globals() {
    // Observation pinned so a deno_core upgrade that changes it is noticed. Games must not
    // see `Deno`: hiding or removing it is an open bootstrap item (README.md).
    let log = probe::main_realm_globals().unwrap();
    assert!(log.contains(&"Deno:object".to_string()), "{log:?}");
}
