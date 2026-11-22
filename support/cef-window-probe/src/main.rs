//! The probe always runs sandboxed: on Windows CEF's bootstrap.exe (renamed
//! roves_cef_probe.exe by the bundler) loads roves_cef_probe.dll and calls `RunWinMain` with
//! the sandbox information. This executable target exists only because the bundler expects
//! the package to have one; launching it directly is refused instead of running unsandboxed.

fn main() {
    eprintln!(
        "roves_cef_probe must be started through the bundled bootstrap executable \
         (cargo run --bin bundle -- <output-dir>), which keeps the CEF sandbox enabled"
    );
    std::process::exit(3);
}
