//! Assembles `roves-cef-macos-gate.app` from the release binaries already built with
//! `cargo build --release --locked --bins` (cef's `build_bundle` would rebuild in debug).
//!
//! Usage: `target/release/bundle <output-dir>`. cef's bundler writes the Info.plists, copies
//! `Chromium Embedded Framework.framework` (including `Libraries/libcef_sandbox.dylib`) from
//! the CEF distribution this build downloaded, and stamps the helper into the five
//! `Helper*.app` bundles.

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cef::build_util::mac::{BundleInfo, bundle};

    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: bundle <output-dir>")?,
    );
    std::fs::create_dir_all(&output)?;
    // This executable sits next to the binaries it bundles (target/release).
    let exe = std::env::current_exe()?;
    let target = exe.parent().ok_or("bundle: executable has no directory")?;
    let app = bundle(
        &output,
        target,
        "roves-cef-macos-gate",
        "roves-cef-macos-gate-helper",
        None,
        BundleInfo::new(
            "roves-cef-macos-gate",
            "com.drincs.roves.cef-macos-gate",
            "Roves CEF macOS gate",
            "English",
            semver::Version::new(0, 0, 0),
        ),
    )?;
    println!("bundled: {}", app.display());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bundle is macOS-only");
    std::process::exit(1);
}
