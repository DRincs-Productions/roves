//! Rasterizes the startup-screen wordmark with the Windows player's shared build module
//! (README "Startup screen").

#[path = "../cef-netrender-probe/build_wordmark.rs"]
mod build_wordmark;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=../cef-netrender-probe/build_wordmark.rs");
    build_wordmark::rasterize();
}
