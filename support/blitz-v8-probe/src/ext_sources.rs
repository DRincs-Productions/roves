//! Bundle-relative Deno extension sources for the downloadable probe.
//!
//! deno_core 0.413's `extension!`/`include_js_files!` always records extension
//! JS as `LoadedFromFsDuringSnapshot(concat!(env!("CARGO_MANIFEST_DIR"), "/", file))`
//! and, without a startup snapshot, reads it at `JsRuntime` creation. Those are
//! absolute paths of the build machine's Cargo registry
//! (`<CARGO_HOME>/registry/src/<index>/<crate>-<version>/<file>`);
//! `--remap-path-prefix` does not change `env!` values.
//!
//! When a `deno-extension-sources` directory sits next to the executable (the
//! downloadable bundle staged by `dev-mcp/package-windows.ps1`, same
//! `<index>/<crate>-<version>/<file>` layout), every such path is redirected to
//! that directory before the extensions reach deno_core. deno_core still reads
//! the file itself, at the same moment and with the same errors. In that mode
//! the build path is never used: a missing bundled file fails runtime creation
//! instead of falling back to a registry that may hold other content. Without
//! the directory (`cargo run`, CI targets) extensions are returned unchanged.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use deno_core::{Extension, ExtensionFileSource, ExtensionFileSourceCode};

/// Directory name next to the executable, as staged by `package-windows.ps1`.
pub(crate) const BUNDLE_DIR: &str = "deno-extension-sources";

/// The bundle's extension source root, if the executable ships one.
fn bundle_root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::current_exe().ok()?.parent()?.join(BUNDLE_DIR);
        root.is_dir().then_some(root)
    })
    .as_deref()
}

/// Redirects registry extension sources to the bundle, when there is one.
pub(crate) fn portable(extensions: Vec<Extension>) -> Vec<Extension> {
    match bundle_root() {
        Some(root) => redirect(root, extensions),
        None => extensions,
    }
}

/// `<index>/<crate>-<version>/<file>` of a path below `.../registry/src/`.
fn registry_relative(path: &str) -> Option<&str> {
    path.rmatch_indices("registry").find_map(|(at, word)| {
        let rest = path[at + word.len()..].strip_prefix(['/', '\\'])?;
        let rest = rest.strip_prefix("src")?.strip_prefix(['/', '\\'])?;
        (!rest.is_empty()).then_some(rest)
    })
}

/// The bundled path for a recorded build path. Leaked once per distinct path
/// (deno_core takes `&'static str`): bounded by the number of extension files.
fn bundled_path(root: &Path, path: &'static str) -> &'static str {
    static PATHS: OnceLock<Mutex<HashMap<(PathBuf, &'static str), &'static str>>> = OnceLock::new();
    let mut paths = PATHS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    paths.entry((root.to_path_buf(), path)).or_insert_with(|| {
        let Some(relative) = registry_relative(path) else {
            panic!(
                "extension source {path} is not below a Cargo registry/src directory, \
                 so {BUNDLE_DIR} cannot provide it"
            );
        };
        let bundled = root.join(relative);
        if !bundled.is_file() {
            eprintln!(
                "roves-blitz-v8-probe: bundled extension source missing: {}",
                bundled.display()
            );
        }
        Box::leak(bundled.to_string_lossy().into_owned().into_boxed_str())
    })
}

fn redirect(root: &Path, mut extensions: Vec<Extension>) -> Vec<Extension> {
    for extension in &mut extensions {
        for files in [
            &mut extension.js_files,
            &mut extension.esm_files,
            &mut extension.lazy_loaded_esm_files,
            &mut extension.lazy_loaded_js_files,
        ] {
            if files.iter().all(ExtensionFileSource::is_runtime_loadable) {
                continue;
            }
            let redirected = files
                .iter()
                .map(|file| match &file.code {
                    ExtensionFileSourceCode::LoadedFromFsDuringSnapshot(path) => {
                        ExtensionFileSource::loaded_during_snapshot(
                            file.specifier,
                            bundled_path(root, *path),
                        )
                    },
                    _ => file.clone(),
                })
                .collect::<Vec<_>>();
            *files = Cow::Owned(redirected);
        }
    }
    extensions
}

#[cfg(test)]
mod tests {
    use super::*;
    use deno_core::{JsRuntime, RuntimeOptions, ascii_str};

    #[test]
    fn registry_relative_handles_both_separators() {
        assert_eq!(
            registry_relative(
                r"C:\roves-cargo\registry\src\index.crates.io-1949cf8c6b5b557f\deno_web-0.291.0/02_event.js"
            ),
            Some(r"index.crates.io-1949cf8c6b5b557f\deno_web-0.291.0/02_event.js")
        );
        assert_eq!(
            registry_relative(
                "/home/runner/.cargo/registry/src/index.crates.io-x/deno_fetch-0.284.0/26_fetch.js"
            ),
            Some("index.crates.io-x/deno_fetch-0.284.0/26_fetch.js")
        );
        assert_eq!(registry_relative("C:/work/roves/support/x/a.js"), None);
        assert_eq!(registry_relative("C:/cargo/registry/src/"), None);
        assert_eq!(registry_relative("C:/cargo/registry/index/a.js"), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("roves-ext-sources-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn extension(build_path: &'static str) -> Extension {
        Extension {
            name: "roves_ext_sources_test",
            js_files: Cow::Owned(vec![
                ExtensionFileSource::loaded_during_snapshot(
                    "ext:roves_ext_sources_test/a.js",
                    build_path,
                ),
                ExtensionFileSource::new(
                    "ext:roves_ext_sources_test/inline.js",
                    ascii_str!("globalThis.inline = 1;"),
                ),
            ]),
            ..Default::default()
        }
    }

    /// Positive: the bundled copy is evaluated although the build path does
    /// not exist. Inline sources are untouched.
    #[test]
    fn runtime_reads_the_bundled_copy() {
        let root = scratch("positive");
        let crate_dir = root.join("index.test").join("deno_fake-1.0.0");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(crate_dir.join("a.js"), "globalThis.fromBundle = 42;").unwrap();
        let build = "Z:/absent-build-machine/registry/src/index.test/deno_fake-1.0.0/a.js";
        let extensions = redirect(&root, vec![extension(build)]);
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions,
            ..Default::default()
        });
        let value = runtime
            .execute_script("check.js", "globalThis.fromBundle + globalThis.inline")
            .unwrap();
        deno_core::scope!(scope, runtime);
        let value = deno_core::v8::Local::new(scope, value);
        assert_eq!(value.integer_value(scope), Some(43));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Negative: with a bundle, a missing bundled file fails runtime creation
    /// even when the build path exists (no fallback to the build registry).
    #[test]
    fn missing_bundled_file_does_not_fall_back() {
        let root = scratch("negative-bundle");
        let build_home = scratch("negative-build");
        let crate_dir = build_home.join("registry/src/index.test/deno_fake-1.0.0");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(crate_dir.join("a.js"), "globalThis.fromBuild = 1;").unwrap();
        let build: &'static str = Box::leak(
            crate_dir
                .join("a.js")
                .to_string_lossy()
                .into_owned()
                .into_boxed_str(),
        );
        let extensions = redirect(&root, vec![extension(build)]);
        let ExtensionFileSourceCode::LoadedFromFsDuringSnapshot(path) =
            &extensions[0].js_files[0].code
        else {
            panic!("redirected source should still be read by deno_core");
        };
        assert!(Path::new(path).starts_with(&root), "{path}");
        let created = JsRuntime::try_new(RuntimeOptions {
            extensions,
            ..Default::default()
        });
        assert!(created.is_err());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(build_home);
    }
}
