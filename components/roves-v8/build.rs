/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Compiles `src/shim.cc`, the thin binding of public V8 APIs rusty_v8 150 does not bind,
//! against the public headers of the exact `v8` crate this package pins (Cargo.toml).

use std::path::{Path, PathBuf};

/// Must match the `v8` pin in Cargo.toml: the headers are those of the linked library.
const V8_CRATE: &str = "v8-150.4.0";

fn main() {
    println!("cargo:rerun-if-changed=src/shim.cc");
    println!("cargo:rerun-if-env-changed=ROVES_V8_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    let include = v8_include_dir();
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .include(&include)
        .file("src/shim.cc")
        // Only V8's exported functions and plain values cross the boundary: no C++ runtime
        // (exceptions, RTTI, the standard library) is needed beside the prebuilt V8.
        .cpp_link_stdlib(None)
        .flag_if_supported("-fno-exceptions")
        .flag_if_supported("-fno-rtti")
        .flag_if_supported("/Zc:__cplusplus")
        .flag_if_supported("/GR-")
        .warnings(false);
    build.compile("roves_v8_shim");
}

/// The `include` directory of the pinned `v8` crate's V8 sources, as Cargo unpacked them
/// (`ROVES_V8_INCLUDE_DIR` overrides it, e.g. for vendored sources).
fn v8_include_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ROVES_V8_INCLUDE_DIR") {
        let dir = PathBuf::from(dir);
        assert!(dir.join("v8-isolate.h").is_file(), "ROVES_V8_INCLUDE_DIR={} has no v8-isolate.h", dir.display());
        return dir;
    }
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|home| Path::new(&home).join(".cargo"))
    });
    let registry = cargo_home.map(|home| home.join("registry").join("src"));
    if let Some(found) = registry.as_deref().and_then(find_in_registry) {
        return found;
    }
    panic!(
        "roves-v8: cannot find the public V8 headers of {V8_CRATE} under {:?}; set ROVES_V8_INCLUDE_DIR to its v8/include directory",
        registry
    );
}

fn find_in_registry(registry: &Path) -> Option<PathBuf> {
    std::fs::read_dir(registry).ok()?.flatten().find_map(|index| {
        let include = index.path().join(V8_CRATE).join("v8").join("include");
        include.join("v8-isolate.h").is_file().then_some(include)
    })
}
