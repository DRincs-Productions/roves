//! CEF helper for every `Helper*.app` of the gate bundle (renderer, GPU, utility, ...).
//!
//! macOS CEF launches these helpers instead of re-executing the main binary. The helper
//! loads the framework relative to its own bundle and enters CEF through welding, which
//! first initializes Chromium's seatbelt sandbox from the framework's
//! `libcef_sandbox.dylib` (`CefSandboxMode::Sandboxed`). There is no unsandboxed mode: the
//! gate exists to show the sandboxed path works.

#[cfg(target_os = "macos")]
fn main() {
    let exe = std::env::current_exe().expect("helper: current_exe failed");
    // `helper: true` resolves the framework from
    // `<app>.app/Contents/Frameworks/<app> Helper*.app/Contents/MacOS/`.
    let loader = cef::library_loader::LibraryLoader::new(&exe, true);
    if !loader.load() {
        eprintln!("gate helper: failed to load the Chromium Embedded Framework");
        std::process::exit(1);
    }
    // Through welding rather than cef::execute_process, so the helper runs the same CEF app
    // as the browser process and the sandbox is entered before CEF.
    let args = cef::args::Args::new();
    let code = welding::CefRuntime::try_run_subprocess(&args, welding::CefSandboxMode::Sandboxed)
        .unwrap_or_else(|error| {
            eprintln!("gate helper: {error}");
            1
        });
    std::process::exit(code);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("roves-cef-macos-gate-helper is macOS-only");
    std::process::exit(1);
}
