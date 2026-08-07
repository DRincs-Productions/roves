use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::Manifest;

pub struct ExtractOptions {
    pub content_dir: PathBuf,
    /// Where to extract to. `None` picks a location under the OS temp
    /// directory instead — see [`default_dest`] — which is how every
    /// generated launcher calls this in practice: nothing is ever extracted
    /// next to the bundle itself.
    pub dest: Option<PathBuf>,
    pub force: bool,
}

/// Marker file dropped in the destination recording the `content_hash` of
/// the manifest it was extracted from, so a later run with unchanged content
/// can skip re-extracting entirely instead of paying the decompression cost
/// on every launch.
const HASH_MARKER: &str = ".content-hash";

/// Picks a stable, per-install destination under the OS temp directory —
/// used whenever the caller doesn't pass an explicit `--dest` (i.e. every
/// generated launcher). Keyed by a hash of the resolved `content_dir` path so
/// repeat launches of the *same* install reuse the same destination (letting
/// the content-hash skip-cache below actually help), while different
/// installs/games on the same machine don't collide with each other. Nothing
/// is ever written next to the bundle: the OS temp directory is periodically
/// cleaned by the OS itself (tmpfs on Linux is RAM-backed and cleared on
/// reboot; Windows/macOS both prune old temp entries on their own schedule),
/// which is the whole point — see CUSTOMIZATIONS.md.
fn default_dest(content_dir: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(content_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    std::env::temp_dir().join(format!("roves-content-{short}"))
}

/// Extracts the packed content at `opts.content_dir` back to plain files,
/// returning the directory it extracted (or already-cached-and-skipped) into.
pub fn extract(opts: &ExtractOptions) -> Result<PathBuf, String> {
    let content_dir = opts
        .content_dir
        .canonicalize()
        .map_err(|e| format!("resolving {:?}: {e}", opts.content_dir))?;
    let dest = match &opts.dest {
        Some(dest) => dest.clone(),
        None => default_dest(&content_dir),
    };

    let manifest_path = content_dir.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|e| format!("reading {manifest_path:?}: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("parsing {manifest_path:?}: {e}"))?;

    let hash_marker = dest.join(HASH_MARKER);
    if !opts.force {
        if let Ok(existing) = fs::read_to_string(&hash_marker) {
            if existing.trim() == manifest.content_hash {
                return Ok(dest);
            }
        }
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("clearing {dest:?}: {e}"))?;
    }
    fs::create_dir_all(&dest).map_err(|e| format!("creating {dest:?}: {e}"))?;

    for pack in &manifest.packs {
        let pack_path = content_dir.join(&pack.file);
        let file = fs::File::open(&pack_path).map_err(|e| format!("opening {pack_path:?}: {e}"))?;
        if pack.compressed {
            let decoder =
                zstd::Decoder::new(file).map_err(|e| format!("decoding {pack_path:?}: {e}"))?;
            tar::Archive::new(decoder)
                .unpack(&dest)
                .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
        } else {
            tar::Archive::new(file)
                .unpack(&dest)
                .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
        }
    }

    for rel in &manifest.excluded {
        let src = content_dir.join(rel);
        let dst = dest.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
        }
        fs::copy(&src, &dst).map_err(|e| format!("copying {src:?}: {e}"))?;
    }

    fs::write(&hash_marker, &manifest.content_hash)
        .map_err(|e| format!("writing {hash_marker:?}: {e}"))?;

    Ok(dest)
}
