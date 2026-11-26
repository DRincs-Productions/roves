//! Linux only: give the gate an `$ORIGIN` run path so `libcef.so` (and, in the `steam`
//! variant, `libsteam_api.so`) next to it is found without `LD_LIBRARY_PATH`.
//!
//! This is required, not a convenience: with the setuid sandbox, Chromium starts the
//! zygote through the root-owned `chrome-sandbox`, and the dynamic loader removes
//! `LD_LIBRARY_PATH` for set-user-ID programs (secure execution), so a zygote that
//! relied on it could not load libcef. Chrome's own Linux binary uses `$ORIGIN` too.
//!
//! Also rasterizes the startup-screen wordmark with the Windows player's shared build module
//! (README "Startup screen").

#[path = "../cef-windows-player/build_wordmark.rs"]
mod build_wordmark;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=../cef-windows-player/build_wordmark.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bin=roves-cef-linux-gate=-Wl,-rpath,$ORIGIN");
    }
    build_wordmark::rasterize();
}
