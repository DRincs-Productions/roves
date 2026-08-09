/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

// Copies the Steamworks redistributable library from steamworks-sys's OUT_DIR
// into target/<profile>/, right next to the built `servoshell` binary — see
// the parent project's `src-tauri/build.rs` for the equivalent Tauri-side
// copy (which lands the same file in src-tauri/ for Tauri's own bundler to
// pick up instead). Landing it in target/<profile>/ means `./mach bundle`
// (see ../../CUSTOMIZATIONS.md) picks it up for free via the same
// *.dll/*.dylib/*.so glob it already uses for ANGLE/GStreamer — no extra
// plumbing needed there.
fn copy_steam_lib(target_profile_dir: &Path) {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_dir = Path::new(&out_dir);
    // OUT_DIR layout: .../target/<profile>/build/<pkg-hash>/out — walk up to
    // the shared `build/` directory to find steamworks-sys's own OUT_DIR,
    // which is a sibling of ours, not a descendant.
    let build_dir = out_dir
        .ancestors()
        .nth(2)
        .expect("unexpected OUT_DIR layout");

    let lib_name = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => "steam_api64.dll",
        Ok("macos") => "libsteam_api.dylib",
        _ => "libsteam_api.so",
    };

    let Ok(entries) = std::fs::read_dir(build_dir) else {
        panic!("Steam: could not read {}", build_dir.display());
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join("out").join(lib_name);
        if candidate.exists() {
            let dest = target_profile_dir.join(lib_name);
            std::fs::copy(&candidate, &dest)
                .unwrap_or_else(|e| panic!("failed to copy {lib_name}: {e}"));
            println!("cargo:warning=Steam: copied {lib_name} -> {}", dest.display());
            return;
        }
    }
    panic!(
        "Steam: could not find `{lib_name}` in build artifacts. \
         Ensure `--features steam` is active and steamworks-sys compiled successfully."
    );
}

fn git_sha() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        let hash = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        Ok(hash.trim().to_owned())
    } else {
        let stderr = String::from_utf8(output.stderr).map_err(|e| e.to_string())?;
        Err(stderr)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rustc-check-cfg=cfg(servo_production)");
    println!("cargo::rustc-check-cfg=cfg(servo_do_not_use_in_production)");
    // Cargo does not expose the profile name to crates or their build scripts,
    // but we can extract it from OUT_DIR and set a custom cfg() ourselves.
    let out = std::env::var("OUT_DIR")?;
    let out = Path::new(&out);
    let krate = out.parent().unwrap();
    let build = krate.parent().unwrap();
    let profile = build
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    if profile == "production" || profile.starts_with("production-") {
        println!("cargo:rustc-cfg=servo_production");
    } else {
        println!("cargo:rustc-cfg=servo_do_not_use_in_production");
    }

    // The window/taskbar icon (`headed_window.rs`'s `include_bytes!`) and — on
    // Windows, below — the compiled `.exe`'s own icon resource both prefer a
    // game-supplied icon over Roves' own branding, so a shipped game looks
    // like itself rather than the shell it happens to be built on. Falls back
    // to the Roves-branded resource if the game hasn't supplied one — see
    // CUSTOMIZATIONS.md. The boot splash's icon is deliberately exempt from
    // this fallback (always Roves-branded, see `gui.rs`'s `update_splash`).
    let game_window_icon = Path::new("../../test-page/public/icon.png");
    let fallback_window_icon = Path::new("../../resources/servo_64.png");
    println!("cargo:rerun-if-changed={}", game_window_icon.display());
    println!("cargo:rerun-if-changed={}", fallback_window_icon.display());
    let window_icon_src = if game_window_icon.exists() { game_window_icon } else { fallback_window_icon };
    std::fs::copy(window_icon_src, out.join("window_icon.png"))
        .expect("failed to copy window icon into OUT_DIR");

    // Note: We can't use `#[cfg(windows)]`, since that would check the host platform
    // and not the target platform
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();

    // `CARGO_FEATURE_STEAM` (not `#[cfg(feature = "steam")]`, which build
    // scripts don't get applied to their own compilation) is how Cargo tells
    // a build script that the crate's `steam` feature is enabled.
    if std::env::var("CARGO_FEATURE_STEAM").is_ok() {
        copy_steam_lib(build.parent().unwrap());
    }

    if target_os == "windows" {
        #[cfg(windows)]
        {
            let game_exe_icon = Path::new("../../test-page/public/icon.ico");
            println!("cargo:rerun-if-changed={}", game_exe_icon.display());
            let exe_icon = if game_exe_icon.exists() {
                game_exe_icon
            } else {
                Path::new("../../resources/servo.ico")
            };

            let mut res = winresource::WindowsResource::new();
            res.set_icon(exe_icon.to_str().expect("icon path is not valid UTF-8"));
            res.set_manifest_file("platform/windows/servoshell.exe.manifest");
            res.compile().unwrap();
        }
        #[cfg(not(windows))]
        panic!("Cross-compiling to windows is currently not supported");
    } else if target_os == "macos" {
        println!("cargo:rerun-if-changed=platform/macos/count_threads.c");
        cc::Build::new()
            .file("platform/macos/count_threads.c")
            .compile("count_threads");
    } else if target_os == "android" {
        // FIXME: We need this workaround since jemalloc-sys still links
        // to libgcc instead of libunwind, but Android NDK 23c and above
        // don't have libgcc. We can't disable jemalloc for Android as
        // in 64-bit aarch builds, the system allocator uses tagged
        // pointers by default which causes the assertions in SM & mozjs
        // to fail. See https://github.com/servo/servo/issues/32175.
        let mut libgcc = File::create(out.join("libgcc.a")).unwrap();
        libgcc.write_all(b"INPUT(-lunwind)").unwrap();
        println!("cargo:rustc-link-search=native={}", out.display());
    }

    match git_sha() {
        Ok(hash) => println!("cargo:rustc-env=GIT_SHA={}", hash),
        Err(error) => {
            println!(
                "cargo:warning=Could not generate git version information: {:?}",
                error
            );
            println!("cargo:rustc-env=GIT_SHA=nogit");
        },
    }

    // On MacOS, all dylib dependencies are shipped along with the binary
    // in the "/lib" directory. Setting the rpath here, allows the dynamic
    // linker to locate them. See `man dyld` for more info.
    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/lib/");
    }

    // Kiosk/embedded fork: on Linux, `mach bundle` ships `.so` dependencies
    // flat next to the binary (see `python/servo/post_build_commands.py`),
    // and used to rely on a wrapper launcher script (`play.sh`) setting
    // `LD_LIBRARY_PATH` before exec'ing the real engine binary. That
    // wrapper is gone (see CUSTOMIZATIONS.md's single-executable-bundle
    // entry) — the shipped binary now needs to find its own siblings, same
    // as the macOS case above.
    if target_os == "linux" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }

    // On OpenHarmony, libservoshell.so is loaded by ArkTS as a NAPI module.
    // Passing a version script allows us to inform the linker about required
    // symbol visibility (only one), which improves stripping of unused sections.
    if target_env == "ohos" {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        let version_script = Path::new(&manifest_dir)
            .join("platform")
            .join("openharmony")
            .join("libservoshell.ver");
        let version_script_str = version_script.to_str().expect("Expected UTF-8 text");
        assert!(
            version_script.exists(),
            "Expected version script to exist at path `{version_script_str}`"
        );
        println!("cargo:rerun-if-changed={version_script_str}");
        // Using `rustc-link-arg-cdylib` causes a false-positive warning:
        // https://github.com/rust-lang/cargo/issues/16487
        // We work around this by just using the unconditional link-arg, which
        // should be fine, since we always build servo as a cdylib on OpenHarmony.
        println!("cargo:rustc-link-arg=-Wl,--version-script={version_script_str}");
    }
    Ok(())
}
