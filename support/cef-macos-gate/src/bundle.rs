//! Assembles `roves-cef-macos-gate.app` from the release binaries already built with
//! `cargo build --release --locked --bins [--features steam]` (cef's `build_bundle` would
//! rebuild in debug).
//!
//! Usage: `target/release/bundle <output-dir> [--resources <dir>]`. cef's bundler writes the
//! Info.plists, copies `Chromium Embedded Framework.framework` (including
//! `Libraries/libcef_sandbox.dylib`) from the CEF distribution this build downloaded, and
//! stamps the helper into the five `Helper*.app` bundles. `--resources`: cef's bundler copies
//! `<dir>/mac` into `Contents/Resources` and, when `<dir>/mac/roves-cef-macos-gate.icns`
//! exists, sets `CFBundleIconFile` (README "Application icon", `packaging/icns.sh`).
//!
//! Built with the `steam` feature (README "Steam"), it also copies steamworks-sys's
//! `libsteam_api.dylib` into `Contents/MacOS`, where its install name
//! `@loader_path/libsteam_api.dylib` resolves from the main executable.

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cef::build_util::mac::{BundleInfo, bundle};

    let mut output = None;
    let mut resources = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--resources" => {
                resources = Some(std::path::PathBuf::from(
                    args.next().ok_or("--resources needs a directory")?,
                ))
            },
            _ if output.is_none() => output = Some(std::path::PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg:?}").into()),
        }
    }
    let output = output.ok_or("usage: bundle <output-dir> [--resources <dir>]")?;
    std::fs::create_dir_all(&output)?;
    // This executable sits next to the binaries it bundles (target/release).
    let exe = std::env::current_exe()?;
    let target = exe.parent().ok_or("bundle: executable has no directory")?;
    let app = bundle(
        &output,
        target,
        "roves-cef-macos-gate",
        "roves-cef-macos-gate-helper",
        resources,
        BundleInfo::new(
            "roves-cef-macos-gate",
            "com.drincs.roves.cef-macos-gate",
            "Roves CEF macOS gate",
            "English",
            semver::Version::new(0, 0, 0),
        ),
    )?;
    #[cfg(feature = "steam")]
    {
        let lib = steam_lib::find(target, "libsteam_api.dylib")?;
        std::fs::copy(&lib, app.join("Contents/MacOS/libsteam_api.dylib"))?;
        println!("copied {}", lib.display());
    }
    println!("bundled: {}", app.display());
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "steam"))]
mod steam_lib;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bundle is macOS-only");
    std::process::exit(1);
}
