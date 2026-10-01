use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=PYTHON");
    if env::var_os("CARGO_FEATURE_WEBIDL_PILOT").is_none() {
        return;
    }
    let bindings = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../script_bindings");
    for path in ["codegen", "webidls/ValidityState.webidl", "third_party/WebIDL/parser", "third_party/ply"] {
        println!("cargo:rerun-if-changed={}", bindings.join(path).display());
    }
    let python = env::var("PYTHON").unwrap_or_else(|_| {
        ["python3", "python"].into_iter().find(|name| {
            Command::new(name).arg("--version").output().is_ok_and(|output| output.status.success())
        }).expect("Python 3 is required for the webidl-pilot feature").to_owned()
    });
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("ValidityStateV8Binding.rs");
    let status = Command::new(python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(bindings.join("webidls/ValidityState.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status().expect("run V8 WebIDL generator");
    assert!(status.success(), "V8 WebIDL generation failed");
}
