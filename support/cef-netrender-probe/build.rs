//! Rasterizes the startup-screen "Roves" wordmark (`build_wordmark.rs`, shared with the macOS
//! and Linux players' build scripts) and, with the `steam` feature, delay-loads the
//! Steamworks redistributable.

mod build_wordmark;

use std::env;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build_wordmark.rs");
    // `steam` feature (README "Steam"): delay-load the Steamworks redistributable, so CEF's
    // sandboxed child processes (which run the same play.dll) never load it and the player
    // checks for it before the first Steamworks call.
    if env::var_os("CARGO_FEATURE_STEAM").is_some()
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo::rustc-link-arg-cdylib=/DELAYLOAD:steam_api64.dll");
        println!("cargo::rustc-link-lib=delayimp");
    }
    build_wordmark::rasterize();
}
