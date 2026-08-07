use std::fs;
use std::path::PathBuf;

use crate::manifest::Manifest;

pub struct ExtractOptions {
    pub content_dir: PathBuf,
    pub dest: PathBuf,
    pub force: bool,
}

/// Marker file dropped in `dest` recording the `content_hash` of the manifest
/// it was extracted from, so a later run with unchanged content can skip
/// re-extracting entirely instead of paying the decompression cost on every
/// launch.
const HASH_MARKER: &str = ".content-hash";

pub fn extract(opts: &ExtractOptions) -> Result<(), String> {
    let manifest_path = opts.content_dir.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|e| format!("reading {manifest_path:?}: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("parsing {manifest_path:?}: {e}"))?;

    let hash_marker = opts.dest.join(HASH_MARKER);
    if !opts.force {
        if let Ok(existing) = fs::read_to_string(&hash_marker) {
            if existing.trim() == manifest.content_hash {
                return Ok(());
            }
        }
    }

    if opts.dest.exists() {
        fs::remove_dir_all(&opts.dest).map_err(|e| format!("clearing {:?}: {e}", opts.dest))?;
    }
    fs::create_dir_all(&opts.dest).map_err(|e| format!("creating {:?}: {e}", opts.dest))?;

    for pack in &manifest.packs {
        let pack_path = opts.content_dir.join(&pack.file);
        let file = fs::File::open(&pack_path).map_err(|e| format!("opening {pack_path:?}: {e}"))?;
        if pack.compressed {
            let decoder =
                zstd::Decoder::new(file).map_err(|e| format!("decoding {pack_path:?}: {e}"))?;
            tar::Archive::new(decoder)
                .unpack(&opts.dest)
                .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
        } else {
            tar::Archive::new(file)
                .unpack(&opts.dest)
                .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
        }
    }

    for rel in &manifest.excluded {
        let src = opts.content_dir.join(rel);
        let dst = opts.dest.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
        }
        fs::copy(&src, &dst).map_err(|e| format!("copying {src:?}: {e}"))?;
    }

    fs::write(&hash_marker, &manifest.content_hash)
        .map_err(|e| format!("writing {hash_marker:?}: {e}"))?;

    Ok(())
}
