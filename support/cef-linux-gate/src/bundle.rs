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
//!
//! After cef's bundler: the build-only files of the CEF distribution (`BUILD_ONLY`) are
//! removed; `CREDITS.html` (Chromium's third-party notices) moves into `licenses/` with CEF's,
//! Roves' and Metal Mania's licenses; the CEF shared libraries are stripped with
//! `strip --strip-unneeded` (CEF's Linux `libcef.so` ships ~1.2 GB of symbols and debug
//! sections that are never loaded at run time; the dynamic symbol table is kept).

/// Files of the CEF distribution needed only to build against CEF, never at run time.
#[cfg(target_os = "linux")]
const BUILD_ONLY: [&str; 2] = ["CMakeLists.txt", "archive.json"];

/// CEF's shared libraries, stripped in place (Valve's `libsteam_api.so` is copied untouched).
#[cfg(target_os = "linux")]
const CEF_LIBRARIES: [&str; 3] = ["libcef.so", "libvk_swiftshader.so", "libvulkan.so.1"];

/// License files written into `licenses/` (`CEF-CREDITS.html` is moved there from the bundle).
#[cfg(target_os = "linux")]
const LICENSES: [(&str, &[u8]); 3] = [
    ("Roves-LICENSE.txt", include_bytes!("../../../LICENSE")),
    (
        "MetalMania-OFL.txt",
        include_bytes!("../../../resources/fonts/MetalMania-OFL.txt"),
    ),
    (
        "CEF-LICENSE.txt",
        include_bytes!("../../../resources/licenses/CEF-LICENSE.txt"),
    ),
];

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
    for file in BUILD_ONLY {
        match std::fs::remove_file(output.join(file)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {},
        }
    }
    let licenses = output.join("licenses");
    std::fs::create_dir_all(&licenses)?;
    std::fs::rename(
        output.join("CREDITS.html"),
        licenses.join("CEF-CREDITS.html"),
    )
    .map_err(|e| format!("moving CEF CREDITS.html into licenses/: {e}"))?;
    for (name, bytes) in LICENSES {
        std::fs::write(licenses.join(name), bytes)?;
    }
    for lib in CEF_LIBRARIES.map(|name| output.join(name)) {
        if !lib.is_file() {
            continue;
        }
        let before = std::fs::metadata(&lib)?.len();
        let status = std::process::Command::new("strip")
            .arg("--strip-unneeded")
            .arg(&lib)
            .status()
            .map_err(|e| format!("running strip: {e}"))?;
        if !status.success() {
            return Err(format!("strip {} failed: {status}", lib.display()).into());
        }
        let after = std::fs::metadata(&lib)?.len();
        println!("stripped {}: {before} -> {after} bytes", lib.display());
    }
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
