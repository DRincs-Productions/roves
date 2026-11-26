//! Thin wrapper over cef-rs' own Windows bundler (`cef::build_util::win`, the code behind the
//! upstream `bundle-cef-app` tool): builds the cdylib and lays
//! out `roves_cef_windows_player.exe` (CEF 154's bootstrap.exe), the DLL, the manifest and
//! the CEF runtime files in the output directory.
//!
//! Usage: `cargo run --bin bundle -- <output-dir> [--release] [--features <list>]`
//!
//! `--features`: cef-rs' `build_bundle` builds the cdylib without features, so the cdylib is
//! built here with them and laid out by cef-rs' `bundle`. With `steam` (README "Steam"),
//! steamworks-sys's `steam_api64.dll` (located through cargo's own build-script messages) is
//! copied next to the executable.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = None;
    let mut release = false;
    let mut features = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => release = true,
            "--features" => features = Some(args.next().ok_or("--features needs a value")?),
            _ if output.is_none() => output = Some(std::path::PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg:?}").into()),
        }
    }
    let output = output.ok_or("usage: bundle <output-dir> [--release] [--features <list>]")?;
    let name = env!("CARGO_PKG_NAME");
    let exe = match features {
        None => cef::build_util::win::build_bundle(&output, name, release)?,
        Some(features) => {
            let steam_dll = build_cdylib(name, release, &features)?;
            let target = cef::build_util::metadata::get_cargo_metadata()?
                .target_directory()
                .join(if release { "release" } else { "debug" });
            let exe = cef::build_util::win::bundle(&output, &target, name)?;
            if let Some(dll) = steam_dll {
                std::fs::copy(&dll, output.join("steam_api64.dll"))?;
                println!("Copied {}", dll.display());
            } else if features.split([',', ' ']).any(|f| f == "steam") {
                return Err("steam_api64.dll not found in steamworks-sys's OUT_DIR".into());
            }
            exe
        },
    };
    println!("Run the probe from {}", exe.display());
    Ok(())
}

/// `cargo build --locked -p <name> --lib --features <features>`; returns steamworks-sys's
/// `steam_api64.dll` when its build script ran or was reused in this build.
#[cfg(windows)]
fn build_cdylib(
    name: &str,
    release: bool,
    features: &str,
) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
    use std::io::BufRead as _;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut command = std::process::Command::new(cargo);
    command.args([
        "build",
        "--locked",
        "-p",
        name,
        "--lib",
        "--features",
        features,
        "--message-format=json-render-diagnostics",
    ]);
    if release {
        command.arg("--release");
    }
    let mut child = command.stdout(std::process::Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().ok_or("no cargo stdout")?;
    let mut steam_dll = None;
    for line in std::io::BufReader::new(stdout).lines() {
        let message: serde_json::Value = serde_json::from_str(&line?).unwrap_or_default();
        if message["reason"] == "build-script-executed"
            && message["package_id"]
                .as_str()
                .is_some_and(|id| id.contains("steamworks-sys"))
            && let Some(out) = message["out_dir"].as_str()
        {
            let dll = std::path::Path::new(out).join("steam_api64.dll");
            if dll.is_file() {
                steam_dll = Some(dll);
            }
        }
    }
    if !child.wait()?.success() {
        return Err("cargo build of the cdylib failed".into());
    }
    Ok(steam_dll)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Windows CEF player bundle is Windows-only");
    std::process::exit(1);
}
