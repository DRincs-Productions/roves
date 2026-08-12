use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// On-disk `manifest.json` sitting next to a set of `.pack` files, produced by
/// `pack` and consumed by `extract`. Bumped whenever the pack/extract contract
/// changes shape, so an old extractor refuses to (mis)handle a newer manifest.
///
/// v2 added `PackEntry::boot` and `Manifest::files` (the boot/lazy split —
/// see CUSTOMIZATIONS.md). v3 added `Manifest::entry_html`.
pub const FORMAT_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    /// sha256 over every packed/excluded file's relative path + contents, in a
    /// fixed sorted order — changes iff the extracted output would change.
    /// Used by `extract` to tell whether a destination's cached content is
    /// stale (and must be wiped) or still matches this exact input.
    pub content_hash: String,
    pub compression_level: i32,
    pub packs: Vec<PackEntry>,
    /// Relative paths (forward-slash, relative to the content root) that were
    /// left out of every pack by `--exclude` and instead sit next to the packs
    /// as plain files, to be copied verbatim by `extract`.
    pub excluded: Vec<String>,
    /// Maps every packed (non-excluded) file's relative path to the filename
    /// of the pack that contains it, so a lookup for one specific path (e.g.
    /// a `file:` load for a path not yet extracted) can find — and lazily
    /// decompress — the one archive that holds it, without touching any
    /// other. A `BTreeMap` (not a `HashMap`) purely so `manifest.json`
    /// serializes in a fixed, sorted key order — see `pack`'s deterministic
    /// output guarantee.
    pub files: BTreeMap<String, String>,
    /// Relative path (forward-slash, relative to the content root) of the
    /// entry html file — the same string passed as `--html-file` at pack
    /// time. Read back by the engine's own in-process boot extraction (see
    /// CUSTOMIZATIONS.md's single-executable-bundle entry) to build the
    /// final `file:` URL to open, without needing that filename threaded
    /// through as a separate CLI argument.
    pub entry_html: String,
    /// The game's own display name, as passed via `--name` at pack time
    /// (`mach bundle` resolves this from `manifest.json`'s/`package.json`'s
    /// `name` — the same source used for the window title, see
    /// `python/servo/post_build_commands.py`'s `_resolve_window_title`).
    /// `None` for a dev/uncompressed build, or an older manifest predating
    /// this field (`#[serde(default)]` so those still deserialize). Used by
    /// `extract::default_dest` to name the on-disk extraction cache
    /// directory after the game instead of a bare content hash — see
    /// CUSTOMIZATIONS.md.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackEntry {
    /// Filename of the archive, relative to the directory the manifest lives in.
    pub file: String,
    /// `true` = `file` is a zstd-compressed tar (`tar.zst`). `false` = a plain,
    /// uncompressed tar — used for buckets made entirely of already-compressed
    /// extensions (images/audio/video/fonts/archives), where re-compressing
    /// would spend CPU for essentially no size win.
    pub compressed: bool,
    /// Whether this archive is part of the small "boot set": the html file
    /// itself plus whatever it directly references (script/link/img `src`/
    /// `href`), or anything matched by `--boot-include`. Boot archives are
    /// extracted eagerly, in full, before the engine opens anything — every
    /// other archive stays compressed until something actually requests a
    /// file inside it (see `ports/servoshell/desktop/protocols/file.rs`).
    pub boot: bool,
}
