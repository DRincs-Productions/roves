//! Assembles the flat CEF directory of the Linux gate from the release binary already built
//! with `cargo build --release --locked --bins`.
//!
//! Usage: `target/release/bundle <output-dir>`. cef's Linux bundler copies every file of the
//! CEF distribution this build downloaded (`libcef.so`, `chrome-sandbox`, `icudtl.dat`,
//! `*.pak`, `v8_context_snapshot.bin`, ...) and `locales/` next to the executable, which is the layout CEF expects on Linux
//! (resources and `chrome-sandbox` are looked up in the executable's directory).
//!
//! `chrome-sandbox` is copied as an ordinary file. Making it the setuid sandbox helper
//! (owner root, mode 4755) needs root and is done by the caller (CI) on purpose.
//!
//! Built with the `steam` feature (README "Steam"), it also copies steamworks-sys's
//! `libsteam_api.so` next to the executable, where the gate's `$ORIGIN` run path finds it.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: bundle <output-dir>")?,
    );
    std::fs::create_dir_all(&output)?;
    // This executable sits next to the binary it bundles (target/release).
    let exe = std::env::current_exe()?;
    let target = exe.parent().ok_or("bundle: executable has no directory")?;
    let gate = cef::build_util::linux::bundle(&output, target, "roves-cef-linux-gate")?;
    #[cfg(feature = "steam")]
    {
        let lib = steam_lib::find(target, "libsteam_api.so")?;
        std::fs::copy(&lib, output.join("libsteam_api.so"))?;
        println!("copied {}", lib.display());
    }
    println!("bundled: {}", gate.display());
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "steam"))]
#[path = "../../cef-macos-gate/src/steam_lib.rs"]
mod steam_lib;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bundle is Linux-only");
    std::process::exit(1);
}
