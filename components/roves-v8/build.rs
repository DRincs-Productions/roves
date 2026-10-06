use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=PYTHON");
    if env::var_os("CARGO_FEATURE_WEBIDL_PILOT").is_none() {
        return;
    }
    let bindings = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../script_bindings");
    for path in [
        "codegen",
        "webidls/ValidityState.webidl",
        "webidls/Screen.webidl",
        "third_party/WebIDL/parser",
        "third_party/ply",
    ] {
        println!("cargo:rerun-if-changed={}", bindings.join(path).display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/Utf16StringState.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/MutablePrimitives.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/NullableDomString.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/UsvStrings.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/StringOperations.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/NullableOperations.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalOperations.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/ByteStringOperations.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalDefaults.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalStringDefaults.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalNullableStringDefaults.webidl").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/EnumOperations.webidl").display()
    );
    let python = env::var("PYTHON").unwrap_or_else(|_| {
        ["python3", "python"].into_iter().find(|name| {
            Command::new(name).arg("--version").output().is_ok_and(|output| output.status.success())
        }).expect("Python 3 is required for the webidl-pilot feature").to_owned()
    });
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (interface, webidl) in [
        ("ValidityState", "ValidityState.webidl"),
        ("Screen", "Screen.webidl"),
    ] {
        let output = out_dir.join(format!("{interface}V8Binding.rs"));
        let status = Command::new(&python)
            .arg(bindings.join("codegen/run_v8.py"))
            .arg(bindings.join("webidls").join(webidl))
            .arg(output)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .status()
            .expect("run V8 WebIDL generator");
        assert!(status.success(), "V8 WebIDL generation failed for {interface}");
    }
    let output = out_dir.join("Utf16StringStateV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/Utf16StringState.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run UTF-16 WebIDL fixture generator");
    assert!(status.success(), "UTF-16 WebIDL fixture generation failed");
    let output = out_dir.join("MutablePrimitivesV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/MutablePrimitives.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run mutable primitive WebIDL fixture generator");
    assert!(status.success(), "mutable primitive WebIDL fixture generation failed");
    let output = out_dir.join("NullableDomStringV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/NullableDomString.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run nullable DOMString WebIDL fixture generator");
    assert!(status.success(), "nullable DOMString WebIDL fixture generation failed");
    let output = out_dir.join("UsvStringsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/UsvStrings.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run USVString WebIDL fixture generator");
    assert!(status.success(), "USVString WebIDL fixture generation failed");
    let output = out_dir.join("StringOperationsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/StringOperations.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run string operation WebIDL fixture generator");
    assert!(status.success(), "string operation WebIDL fixture generation failed");
    let output = out_dir.join("NullableOperationsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/NullableOperations.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run nullable operation WebIDL fixture generator");
    assert!(status.success(), "nullable operation WebIDL fixture generation failed");
    let output = out_dir.join("OptionalOperationsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalOperations.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run optional operation WebIDL fixture generator");
    assert!(status.success(), "optional operation WebIDL fixture generation failed");
    let output = out_dir.join("ByteStringOperationsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/ByteStringOperations.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run ByteString operation WebIDL fixture generator");
    assert!(status.success(), "ByteString operation WebIDL fixture generation failed");
    let output = out_dir.join("OptionalDefaultsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalDefaults.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run optional default WebIDL fixture generator");
    assert!(status.success(), "optional default WebIDL fixture generation failed");
    let output = out_dir.join("OptionalStringDefaultsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalStringDefaults.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run optional string default WebIDL fixture generator");
    assert!(status.success(), "optional string default WebIDL fixture generation failed");
    let output = out_dir.join("OptionalNullableStringDefaultsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/OptionalNullableStringDefaults.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run optional nullable string default WebIDL fixture generator");
    assert!(status.success(), "optional nullable string default WebIDL fixture generation failed");
    let output = out_dir.join("EnumOperationsV8Binding.rs");
    let status = Command::new(&python)
        .arg(bindings.join("codegen/run_v8.py"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl/EnumOperations.webidl"))
        .arg(output)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run WebIDL enum operation fixture generator");
    assert!(status.success(), "WebIDL enum operation fixture generation failed");
    // Inheritance: the derived binding is generated with its ancestor's file as parse
    // context and refers to the base binding's sibling module.
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webidl");
    for (interface, context) in [
        ("InheritanceBase", &[][..]),
        ("InheritanceDerived", &["InheritanceBase"][..]),
        ("ConstructibleCounter", &[][..]),
        ("ConstructibleChild", &["ConstructibleCounter"][..]),
        ("ThrowingOperations", &[][..]),
        ("UnforgeableBase", &[][..]),
        ("UnforgeableChild", &["UnforgeableBase"][..]),
        ("WebIdlConstants", &[][..]),
        ("LinkedNode", &[][..]),
        ("LinkedLeaf", &["LinkedNode"][..]),
        ("ExposureGated", &[][..]),
        ("HiddenInterface", &[][..]),
        ("OverloadedOperations", &[][..]),
        ("CeReactive", &[][..]),
        ("CallbackOperations", &[][..]),
        ("HandlerHost", &[][..]),
        ("SequenceOperations", &[][..]),
        ("DictionaryProbe", &[][..]),
        ("ListenerTarget", &[][..]),
        ("TokenProbe", &[][..]),
        ("AttributeProbe", &["TokenProbe"][..]),
        ("VariadicOperations", &[][..]),
        ("JsonBase", &[][..]),
        ("JsonChild", &["JsonBase"][..]),
        ("PromiseOperations", &[][..]),
        ("DefaultsProbe", &[][..]),
        ("BufferOperations", &[][..]),
        ("PathProbe", &[][..]),
        ("DrawProbe", &["PathProbe"][..]),
        ("GradientProbe", &[][..]),
        ("StyleProbe", &["GradientProbe"][..]),
        ("ItemList", &[][..]),
        ("ItemListChild", &["ItemList"][..]),
        ("NamedCollection", &[][..]),
        ("LenientProbe", &[][..]),
        ("ShadowBase", &[][..]),
        ("ShadowChild", &["ShadowBase"][..]),
        ("TypeGapsProbe", &[][..]),
    ] {
        println!("cargo:rerun-if-changed={}", fixtures.join(format!("{interface}.webidl")).display());
        println!("cargo:rerun-if-changed={}", fixtures.join("Bindings.conf").display());
        let status = Command::new(&python)
            .arg(bindings.join("codegen/run_v8.py"))
            .arg(fixtures.join(format!("{interface}.webidl")))
            .arg(out_dir.join(format!("{interface}V8Binding.rs")))
            .args(context.iter().map(|ancestor| fixtures.join(format!("{ancestor}.webidl"))))
            .arg("--bindings-conf")
            .arg(fixtures.join("Bindings.conf"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .status()
            .expect("run WebIDL inheritance fixture generator");
        assert!(status.success(), "WebIDL inheritance fixture generation failed for {interface}");
    }
}
