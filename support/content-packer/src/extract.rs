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
/// Linux, which matters once a project's assets reach the multi-GB range):
/// `<cache_dir>/<game_name>/cache/<hash8>/`. `game_name` (from
/// `Manifest::name`, plumbed in by every caller of this function — see
/// `resolve_dest`) makes the top-level folder recognizable as "this game's
/// stuff" instead of an opaque `roves-content-<hash>`; it's sanitized via
/// [`sanitize_path_segment`] and falls back to `"roves"` if absent (a dev/
/// uncompressed build, or a manifest predating this field) or if nothing
/// filesystem-safe survives sanitizing. The trailing hash — of the resolved
/// `content_dir` path, *not* game name — is what actually keeps repeat
/// launches of the *same* install pointed at the same destination (letting
/// the marker-based skip-cache below help) while different installs/games
/// that happen to share a display name still don't collide with each other.
/// Nothing is ever written next to the bundle itself — see CUSTOMIZATIONS.md.
fn default_dest(content_dir: &Path, game_name: Option<&str>) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(content_dir.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let game_dir = game_name.and_then(sanitize_path_segment).unwrap_or_else(|| "roves".to_string());
    cache_dir().join(game_dir).join("cache").join(short)
}

/// Turns arbitrary manifest-supplied text (a game's display name) into a
/// single path segment safe to use as a directory name on Linux, macOS, and
/// Windows alike: strips path separators, the `:` a Windows drive letter
/// uses (and that a `NAME:AlternateStream` could otherwise smuggle in),
/// Windows' other reserved characters (`*?"<>|`), and control characters;
/// trims leading/trailing whitespace and `.` (Windows also disallows a
/// trailing dot or space, and a lone leading dot reads as a hidden file on
/// Unix); and caps length well under every platform's per-segment limit.
/// Returns `None` if nothing usable survives, so the caller can fall back to
/// a sane default instead of creating an empty (or `.`/`..`-named) directory.
fn sanitize_path_segment(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c.is_whitespace() || c == '.');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(64).collect())
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

/// Resolves `content_dir` to its canonical form and picks the destination
/// (`dest`, or [`default_dest`] if `None`, using `game_name` — see that
/// function) — the cheap, non-extracting half of [`prepare_dest`], split out
/// so a caller can learn *where* boot content will end up (e.g. to build the
/// `file:` URL it'll load once extraction finishes) without paying for the
/// actual decompression yet. Does no I/O beyond `canonicalize`.
pub fn resolve_dest(
    content_dir: &Path,
    dest: Option<PathBuf>,
    game_name: Option<&str>,
) -> Result<(PathBuf, PathBuf), String> {
    let content_dir = content_dir
        .canonicalize()
        .map_err(|e| format!("resolving {content_dir:?}: {e}"))?;
    let dest = dest.unwrap_or_else(|| default_dest(&content_dir, game_name));
    Ok((content_dir, dest))
}

/// Resolves `content_dir`/`dest`, loads the manifest, and ensures `dest`
/// isn't stale — wiping and recreating it (fresh marker directory, fresh
/// `.roves-content-hash`/`.roves-content-source`) if its recorded content
/// hash doesn't match, or if `force` was passed. Safe to call repeatedly:
/// a matching, non-forced call is a no-op past the manifest read.
///
/// Loads the manifest *before* resolving `dest` (rather than after, as it
/// used to) so `Manifest::name` is available for [`default_dest`] to name
/// the destination after — a `content_dir.join("manifest.json")` read
/// either way, just reordered; nothing here depends on `content_dir` being
/// canonicalized first.
fn prepare_dest(
    content_dir: &Path,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<(PathBuf, PathBuf, Manifest), String> {
    let manifest = load_manifest(content_dir)?;
    let (content_dir, dest) = resolve_dest(content_dir, dest, manifest.name.as_deref())?;

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

/// Shared body of [`extract_boot`]/[`extract_boot_with_progress`]. `on_progress`,
/// when given, is called after each boot pack finishes (as `done / total`,
/// where `total` is the boot pack count) and once more with `1.0` after the
/// final `copy_excluded` — coarse (per-pack, not per-byte) but the boot set
/// is deliberately just the html file and whatever it directly references,
/// so it's usually only one or two packs anyway.
fn extract_boot_impl(
    opts: &ExtractOptions,
    mut on_progress: Option<&mut dyn FnMut(f32)>,
) -> Result<PathBuf, String> {
    let (content_dir, dest, manifest) = prepare_dest(&opts.content_dir, opts.dest.clone(), opts.force)?;
    let boot_packs: Vec<_> = manifest.packs.iter().filter(|p| p.boot).collect();
    let total = boot_packs.len().max(1);
    for (i, pack) in boot_packs.into_iter().enumerate() {
        ensure_pack_extracted(&content_dir, &dest, pack)?;
        if let Some(cb) = on_progress.as_deref_mut() {
            cb((i + 1) as f32 / total as f32);
        }
    }
    copy_excluded(&content_dir, &dest, &manifest)?;
    if let Some(cb) = on_progress.as_deref_mut() {
        cb(1.0);
    }
    Ok(dest)
}

/// Extracts just the boot set (every [`PackEntry`] with `boot: true`) plus
/// every excluded/loose file, and returns the destination directory. This is
/// what every generated launcher calls before starting the engine — small
/// and fast by construction, since the boot set is deliberately just the
/// html file and whatever it directly references. Everything else stays
/// compressed until [`ensure_file_available`] is asked for it, in-process,
/// by the engine's own `file:` handler.
pub fn extract_boot(opts: &ExtractOptions) -> Result<PathBuf, String> {
    extract_boot_impl(opts, None)
}

/// Same as [`extract_boot`], but calls `on_progress` as extraction proceeds
/// — see [`extract_boot_impl`]. Used by the engine to drive a boot splash's
/// progress bar while extraction runs on a background thread; not exposed
/// through the CLI, which has no equivalent UI to feed.
pub fn extract_boot_with_progress(
    opts: &ExtractOptions,
    mut on_progress: impl FnMut(f32),
) -> Result<PathBuf, String> {
    extract_boot_impl(opts, Some(&mut on_progress))
}

/// Returns whether `dir` is a destination directory previously created by
/// [`prepare_dest`] (i.e. actually managed packed-content cache) — checked
/// via [`CONTENT_SOURCE_MARKER`]'s presence, the same signal `FileProtocolHandler`
/// uses to recognize one. Guards [`clear_cache`] against wiping an unrelated
/// directory, e.g. a plain dev `--url` launch's own folder, which this crate
/// never wrote to in the first place.
pub fn is_managed_cache_dir(dir: &Path) -> bool {
    dir.join(CONTENT_SOURCE_MARKER).is_file()
}

/// Deletes a previously-extracted destination directory in its entirety —
/// the whole point being that the next launch's `prepare_dest` sees no
/// `CONTENT_HASH_MARKER` and rebuilds it from scratch. Refuses (rather than
/// deleting) a `dir` that isn't a recognized managed cache — see
/// [`is_managed_cache_dir`].
pub fn clear_cache(dir: &Path) -> Result<(), String> {
    if !is_managed_cache_dir(dir) {
        return Err(format!("{dir:?} is not a managed content cache directory"));
    }
    fs::remove_dir_all(dir).map_err(|e| format!("clearing {dir:?}: {e}"))
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

#[cfg(test)]
mod tests {
    use super::{default_dest, sanitize_path_segment};

    #[test]
    fn sanitize_path_segment_strips_unsafe_characters_and_trims() {
        assert_eq!(sanitize_path_segment("Roves test-page").unwrap(), "Roves test-page");
        assert_eq!(sanitize_path_segment("My:Game/Title*?\"<>|").unwrap(), "My-Game-Title------");
        assert_eq!(sanitize_path_segment("  ..leading and trailing.. ").unwrap(), "leading and trailing");
        assert_eq!(sanitize_path_segment(&"x".repeat(200)).unwrap().len(), 64);
    }

    #[test]
    fn sanitize_path_segment_rejects_nothing_usable() {
        assert!(sanitize_path_segment("").is_none());
        assert!(sanitize_path_segment("   ").is_none());
        assert!(sanitize_path_segment("...").is_none());
        // Path separators/reserved characters become `-`, which (unlike
        // whitespace/`.`) isn't trimmed — a filesystem-safe, if unhelpful,
        // name, not "nothing usable".
        assert_eq!(sanitize_path_segment("/\\:").unwrap(), "---");
    }

    #[test]
    fn default_dest_nests_cache_and_hash_under_the_game_name() {
        let content_dir = std::path::Path::new("/some/build/dist");
        let named = default_dest(content_dir, Some("Roves test-page"));
        let mut components = named.components().rev();
        let hash = components.next().unwrap().as_os_str().to_str().unwrap().to_string();
        assert_eq!(components.next().unwrap().as_os_str(), "cache");
        assert_eq!(components.next().unwrap().as_os_str(), "Roves test-page");
        assert_eq!(hash.len(), 16, "8 bytes, hex-encoded");

        // Same content_dir, no name at all -> same hash, generic top-level folder.
        let unnamed = default_dest(content_dir, None);
        assert_eq!(unnamed.file_name().unwrap(), named.file_name().unwrap());
        assert_eq!(unnamed.parent().unwrap().file_name().unwrap(), "cache");
        assert_eq!(unnamed.parent().unwrap().parent().unwrap().file_name().unwrap(), "roves");
    }
}
