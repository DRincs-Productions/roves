//! Thin wrapper over cef-rs' own Windows bundler (`cef::build_util::win`, the code behind the
//! upstream `bundle-cef-app` tool): builds the `roves_cef_probe` cdylib and lays out
//! `roves_cef_probe.exe` (CEF's bootstrap.exe), `roves_cef_probe.dll`, the manifest and the
//! CEF runtime files in the output directory.
//!
//! Usage: `cargo run --bin bundle -- <output-dir> [--release]`

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = None;
    let mut release = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--release" => release = true,
            _ if output.is_none() => output = Some(std::path::PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg:?}").into()),
        }
    }
    let output = output.ok_or("usage: bundle <output-dir> [--release]")?;
    let exe = cef::build_util::win::build_bundle(&output, env!("CARGO_PKG_NAME"), release)?;
    println!("Run the probe from {}", exe.display());
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the CEF window probe bundle is Windows-only");
    std::process::exit(1);
}
