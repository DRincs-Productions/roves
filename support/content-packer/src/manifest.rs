use serde::{Deserialize, Serialize};

/// On-disk `manifest.json` sitting next to a set of `.pack` files, produced by
/// `pack` and consumed by `extract`. Bumped whenever the pack/extract contract
/// changes shape, so an old extractor refuses to (mis)handle a newer manifest.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    /// sha256 over every packed/excluded file's relative path + contents, in a
    /// fixed sorted order — changes iff the extracted output would change.
    /// Used by `extract` to skip re-extracting unchanged content.
    pub content_hash: String,
    pub compression_level: i32,
    pub packs: Vec<PackEntry>,
    /// Relative paths (forward-slash, relative to the content root) that were
    /// left out of every pack by `--exclude` and instead sit next to the packs
    /// as plain files, to be copied verbatim by `extract`.
    pub excluded: Vec<String>,
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
}
