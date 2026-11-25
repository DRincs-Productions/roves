//! `bundle` with the `steam` feature (shared with the Linux gate's `bundle` through
//! `#[path]`): locates the Steamworks redistributable that steamworks-sys's build script
//! copied into its `OUT_DIR` (`<target>/build/steamworks-sys-<hash>/out/`). The crate has no
//! `links` key, so the directory is found by name; every copy comes from the same pinned
//! crate (steamworks-sys 0.13.0), which is checked byte for byte when several exist.

use std::path::{Path, PathBuf};

pub(crate) fn find(target: &Path, name: &str) -> Result<PathBuf, String> {
    let build = target.join("build");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&build)
        .map_err(|e| format!("{}: {e}", build.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("steamworks-sys-")
        })
        .map(|entry| entry.path().join("out").join(name))
        .filter(|path| path.is_file())
        .collect();
    found.sort();
    let first = found.first().ok_or_else(|| {
        format!(
            "{name} not found under {}/steamworks-sys-*/out",
            build.display()
        )
    })?;
    let bytes = std::fs::read(first).map_err(|e| format!("{}: {e}", first.display()))?;
    for other in &found[1..] {
        if std::fs::read(other).map_err(|e| format!("{}: {e}", other.display()))? != bytes {
            return Err(format!(
                "{} and {} differ: clean the target directory",
                first.display(),
                other.display()
            ));
        }
    }
    Ok(first.clone())
}
