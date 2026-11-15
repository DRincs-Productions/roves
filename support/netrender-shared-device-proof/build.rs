// Windows/MSVC link fix for `deno_js` only (same target gating as
// ports/servoshell/build.rs; cfg!(windows) would test the host, not the target).
//
// rusty_v8's prebuilt `rusty_v8.lib` is compiled with the static CRT: its objects
// carry `/DEFAULTLIB:libcmt.lib`. lcms2-sys (deno_webgpu -> deno_image -> lcms2) is
// compiled by `cc` with `/MD`, so it imports the CRT (`__imp_*`) from `ucrt.lib`.
// Both `msvcrt.lib` and `libcmt.lib` name their UCRT only from `initializers.obj`;
// when lld-link takes the CRT startup from libcmt, only the static `libucrt.lib`
// is added, and the imports it does not happen to define (`remove`, `_wassert`,
// `_time64`) stay undefined. Naming `ucrt.lib` restores the import library that
// the `/MD` objects were compiled for. Not needed with `+crt-static`, where cc
// builds lcms2 with `/MT` and v8 links libcpmt.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let crt_static = std::env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|feature| feature == "crt-static");
    // `deno_js` exists only with the `deno` feature (required-features).
    if std::env::var_os("CARGO_FEATURE_DENO").is_some()
        && target_os == "windows"
        && target_env == "msvc"
        && !crt_static
    {
        println!("cargo:rustc-link-arg-bin=deno_js=/DEFAULTLIB:ucrt.lib");
    }
}
