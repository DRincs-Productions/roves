use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::{Manifest, PackEntry};

pub struct ExtractOptions {
    pub content_dir: PathBuf,
    /// Where to extract to. `None` picks a location under the OS's real
    /// (disk-backed) cache directory instead — see [`default_dest`] — which
    /// is how every generated launcher calls this in practice: nothing is
    /// ever extracted next to the bundle itself.
    pub dest: Option<PathBuf>,
    pub force: bool,
}

/// Records the manifest `content_hash` a destination was last (re)built for.
/// A mismatch means `content_dir`'s packed content changed since — the
/// destination (including any lazily-extracted packs from a previous run,
/// now potentially stale) is wiped and rebuilt from scratch.
const CONTENT_HASH_MARKER: &str = ".roves-content-hash";
/// Directory of empty per-pack marker files (named after the pack's own
/// filename) recording which packs have already been extracted into this
/// destination — lets both `extract_boot` and the in-process on-demand path
/// skip re-extracting a pack that's already there, across launches too.
const EXTRACTED_MARKER_DIR: &str = ".roves-extracted";
/// Plain text file recording the resolved, canonical `content_dir` path this
/// destination was extracted from — read back by servoshell's own `file:`
/// handler at startup so it knows where to find `manifest.json` and the
/// `.pack` files for on-demand extraction, without needing a dedicated CLI
/// argument threaded all the way through `mach bundle`/the launcher/the
/// engine. Absence of this file (e.g. a destination that isn't packed
/// content at all) means "don't do anything special here".
pub const CONTENT_SOURCE_MARKER: &str = ".roves-content-source";

/// Picks a stable, per-install destination under the OS's cache directory
/// (`~/.cache` on Linux, `Library/Caches` on macOS, `%LOCALAPPDATA%` on
/// Windows — real disk, *not* a RAM-backed `tmpfs` like `/tmp` often is on
/// Linux, which matters once a project's assets reach the multi-GB range).
/// Keyed by a hash of the resolved `content_dir` path so repeat launches of
/// the *same* install reuse the same destination (letting the marker-based
/// skip-cache below actually help), while different installs/games on the
/// same machine don't collide with each other. Nothing is ever written next
/// to the bundle itself — see CUSTOMIZATIONS.md.
fn default_dest(content_dir: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(content_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    cache_dir().join(format!("roves-content-{short}"))
}

/// Resolves the OS's disk-backed user cache directory, matching each
/// platform's own convention (not `std::env::temp_dir()`, which on Linux is
/// commonly `tmpfs` — RAM, not disk).
fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return dir;
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return PathBuf::from(home).join("Library").join("Caches");
        }
    } else if cfg!(target_os = "windows") {
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(dir);
        }
    } else if let Some(home) = home {
        return PathBuf::from(home).join(".cache");
    }
    // Last-resort fallback for the rare case none of the above resolved
    // (e.g. no HOME/USERPROFILE set at all) — still better than failing.
    std::env::temp_dir()
}

pub fn load_manifest(content_dir: &Path) -> Result<Manifest, String> {
    let manifest_path = content_dir.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|e| format!("reading {manifest_path:?}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parsing {manifest_path:?}: {e}"))
}

/// Resolves `content_dir`/`dest`, loads the manifest, and ensures `dest`
/// isn't stale — wiping and recreating it (fresh marker directory, fresh
/// `.roves-content-hash`/`.roves-content-source`) if its recorded content
/// hash doesn't match, or if `force` was passed. Safe to call repeatedly:
/// a matching, non-forced call is a no-op past the manifest read.
fn prepare_dest(
    content_dir: &Path,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<(PathBuf, PathBuf, Manifest), String> {
    let content_dir = content_dir
        .canonicalize()
        .map_err(|e| format!("resolving {content_dir:?}: {e}"))?;
    let dest = dest.unwrap_or_else(|| default_dest(&content_dir));
    let manifest = load_manifest(&content_dir)?;

    let hash_marker = dest.join(CONTENT_HASH_MARKER);
    let up_to_date = !force &&
        fs::read_to_string(&hash_marker)
            .map(|existing| existing.trim() == manifest.content_hash)
            .unwrap_or(false);

    if !up_to_date {
        if dest.exists() {
            fs::remove_dir_all(&dest).map_err(|e| format!("clearing {dest:?}: {e}"))?;
        }
        fs::create_dir_all(dest.join(EXTRACTED_MARKER_DIR))
            .map_err(|e| format!("creating {dest:?}: {e}"))?;
        fs::write(&hash_marker, &manifest.content_hash)
            .map_err(|e| format!("writing {hash_marker:?}: {e}"))?;
        fs::write(dest.join(CONTENT_SOURCE_MARKER), content_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("writing content-source marker in {dest:?}: {e}"))?;
    }

    Ok((content_dir, dest, manifest))
}

fn extracted_marker_path(dest: &Path, pack_file: &str) -> PathBuf {
    dest.join(EXTRACTED_MARKER_DIR).join(pack_file)
}

/// Decompresses `pack` into `dest` if (and only if) it hasn't been already —
/// checked via a per-pack marker file, so this is cheap to call repeatedly,
/// including across separate launches of the same, unchanged install.
pub fn ensure_pack_extracted(content_dir: &Path, dest: &Path, pack: &PackEntry) -> Result<(), String> {
    let marker = extracted_marker_path(dest, &pack.file);
    if marker.exists() {
        return Ok(());
    }

    let pack_path = content_dir.join(&pack.file);
    let file = fs::File::open(&pack_path).map_err(|e| format!("opening {pack_path:?}: {e}"))?;
    if pack.compressed {
        let decoder =
            zstd::Decoder::new(file).map_err(|e| format!("decoding {pack_path:?}: {e}"))?;
        tar::Archive::new(decoder)
            .unpack(dest)
            .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
    } else {
        tar::Archive::new(file)
            .unpack(dest)
            .map_err(|e| format!("extracting {pack_path:?}: {e}"))?;
    }

    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
    }
    fs::write(&marker, b"").map_err(|e| format!("writing {marker:?}: {e}"))?;
    Ok(())
}

fn copy_excluded(content_dir: &Path, dest: &Path, manifest: &Manifest) -> Result<(), String> {
    for rel in &manifest.excluded {
        let src = content_dir.join(rel);
        let dst = dest.join(rel);
        if dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
        }
        fs::copy(&src, &dst).map_err(|e| format!("copying {src:?}: {e}"))?;
    }
    Ok(())
}

/// Extracts just the boot set (every [`PackEntry`] with `boot: true`) plus
/// every excluded/loose file, and returns the destination directory. This is
/// what every generated launcher calls before starting the engine — small
/// and fast by construction, since the boot set is deliberately just the
/// html file and whatever it directly references. Everything else stays
/// compressed until [`ensure_file_available`] is asked for it, in-process,
/// by the engine's own `file:` handler.
pub fn extract_boot(opts: &ExtractOptions) -> Result<PathBuf, String> {
    let (content_dir, dest, manifest) = prepare_dest(&opts.content_dir, opts.dest.clone(), opts.force)?;
    for pack in manifest.packs.iter().filter(|p| p.boot) {
        ensure_pack_extracted(&content_dir, &dest, pack)?;
    }
    copy_excluded(&content_dir, &dest, &manifest)?;
    Ok(dest)
}

/// Looks up `rel_path` (forward-slash, relative to `dest`) in `manifest` and
/// ensures whichever pack contains it has been extracted into `dest`.
/// Returns `Ok(false)` if `rel_path` isn't a packed file at all (the caller
/// should fall back to treating the path as an ordinary, possibly-missing
/// file — e.g. it's one of the already-loose `excluded` files, or simply
/// doesn't exist).
pub fn ensure_file_available(
    content_dir: &Path,
    dest: &Path,
    manifest: &Manifest,
    rel_path: &str,
) -> Result<bool, String> {
    let Some(pack_file) = manifest.files.get(rel_path) else {
        return Ok(false);
    };
    let pack = manifest
        .packs
        .iter()
        .find(|p| &p.file == pack_file)
        .ok_or_else(|| format!("manifest inconsistency: {rel_path} points at unknown pack {pack_file}"))?;
    ensure_pack_extracted(content_dir, dest, pack)?;
    Ok(true)
}
