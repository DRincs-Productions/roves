use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::manifest::{FORMAT_VERSION, Manifest, PackEntry};

/// Extensions that are already internally compressed (lossy/lossless image,
/// audio, video, font and archive formats) — re-running zstd over them buys
/// essentially nothing and just spends CPU, so files with these extensions go
/// into a plain, uncompressed tar ("stored") instead of a `tar.zst`.
const STORED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "avif", "gif", "ico", "mp3", "ogg", "oga", "opus", "m4a", "aac",
    "flac", "mp4", "webm", "mov", "mkv", "avi", "woff", "woff2", "zip", "gz", "bz2", "xz", "zst",
    "br", "7z", "rar", "ktx2", "basis", "astc", "dds",
];

pub struct PackOptions {
    pub input: PathBuf,
    pub output: PathBuf,
    pub level: i32,
    pub max_pack_size: u64,
    pub exclude: Vec<glob::Pattern>,
}

struct FileEntry {
    /// Forward-slash path relative to `input`, e.g. `assets/icons/logo.png`.
    rel_path: String,
    abs_path: PathBuf,
    size: u64,
}

pub fn pack(opts: &PackOptions) -> Result<(), String> {
    fs::create_dir_all(&opts.output).map_err(|e| format!("creating {:?}: {e}", opts.output))?;

    let mut all_files = collect_files(&opts.input)?;
    all_files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let mut excluded = Vec::new();
    let mut packable = Vec::new();
    for f in all_files {
        if opts.exclude.iter().any(|p| p.matches(&f.rel_path)) {
            excluded.push(f);
        } else {
            packable.push(f);
        }
    }

    // Bucket key: `None` = dist's own root files. `Some((folder, nested))` =
    // files directly inside a direct subfolder of dist (`nested = false`), or
    // files anywhere deeper than that, flattened into one bucket per
    // top-level subfolder (`nested = true`) — see CUSTOMIZATIONS.md.
    let mut buckets: BTreeMap<(Option<String>, bool), Vec<FileEntry>> = BTreeMap::new();
    for f in packable {
        let mut components = f.rel_path.split('/');
        let first = components.next().unwrap_or("");
        let depth_after_first = components.count();
        let key = if first == f.rel_path {
            (None, false)
        } else if depth_after_first == 1 {
            (Some(first.to_string()), false)
        } else {
            (Some(first.to_string()), true)
        };
        buckets.entry(key).or_default().push(f);
    }

    let mut hasher = Sha256::new();
    let mut pack_entries = Vec::new();
    let mut used_names = HashSet::new();

    for ((folder, nested), files) in buckets {
        let base_name = bucket_base_name(folder.as_deref(), nested, &mut used_names);
        let (stored, compressible): (Vec<_>, Vec<_>) =
            files.into_iter().partition(|f| is_stored_extension(&f.rel_path));

        write_group(opts, &base_name, false, compressible, &mut pack_entries, &mut hasher)?;
        write_group(opts, &base_name, true, stored, &mut pack_entries, &mut hasher)?;
    }

    let mut excluded_rel = Vec::new();
    for f in &excluded {
        let bytes = fs::read(&f.abs_path).map_err(|e| format!("reading {:?}: {e}", f.abs_path))?;
        hasher.update(f.rel_path.as_bytes());
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(&bytes);

        let dest = opts.output.join(&f.rel_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
        }
        fs::write(&dest, &bytes).map_err(|e| format!("writing {dest:?}: {e}"))?;
        excluded_rel.push(f.rel_path.clone());
    }

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        content_hash: format!("sha256:{}", hex_encode(&hasher.finalize())),
        compression_level: opts.level,
        packs: pack_entries,
        excluded: excluded_rel,
    };
    let manifest_path = opts.output.join("manifest.json");
    let manifest_file =
        File::create(&manifest_path).map_err(|e| format!("creating {manifest_path:?}: {e}"))?;
    serde_json::to_writer_pretty(manifest_file, &manifest)
        .map_err(|e| format!("writing {manifest_path:?}: {e}"))?;
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<FileEntry>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|e| format!("walking {root:?}: {e}"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs_path = entry.path().to_path_buf();
        let rel_path = abs_path
            .strip_prefix(root)
            .map_err(|e| format!("{abs_path:?} is not under {root:?}: {e}"))?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let size = entry.metadata().map_err(|e| e.to_string())?.len();
        files.push(FileEntry { rel_path, abs_path, size });
    }
    Ok(files)
}

/// Picks the on-disk basename for a bucket's archive(s), sanitized for safe
/// use as a filename and disambiguated against any other bucket that would
/// otherwise produce the same name (e.g. a real subfolder literally named
/// `root` colliding with the fixed root-bucket name).
fn bucket_base_name(folder: Option<&str>, nested: bool, used: &mut HashSet<String>) -> String {
    let sanitized = match folder {
        None => "__root__".to_string(),
        Some(f) => f
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>(),
    };
    let mut name = if nested { format!("{sanitized}.nested") } else { sanitized.clone() };
    let mut suffix = 2;
    while !used.insert(name.clone()) {
        name = format!("{sanitized}_{suffix}");
        if nested {
            name.push_str(".nested");
        }
        suffix += 1;
    }
    name
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_stored_extension(rel_path: &str) -> bool {
    match rel_path.rsplit_once('.') {
        Some((_, ext)) => STORED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// Splits `files` into `--max-pack-size`-bounded parts and writes each as its
/// own `.pack` archive (`<base_name>[.stored][.N].pack`), recording a
/// [`PackEntry`] per part. No-op if `files` is empty (a bucket with only
/// stored, or only compressible, files skips the other archive entirely).
fn write_group(
    opts: &PackOptions,
    base_name: &str,
    stored: bool,
    files: Vec<FileEntry>,
    pack_entries: &mut Vec<PackEntry>,
    hasher: &mut Sha256,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    let mut parts: Vec<Vec<FileEntry>> = Vec::new();
    let mut current = Vec::new();
    let mut current_size = 0u64;
    for f in files {
        if !current.is_empty() && current_size + f.size > opts.max_pack_size {
            parts.push(std::mem::take(&mut current));
            current_size = 0;
        }
        current_size += f.size;
        current.push(f);
    }
    if !current.is_empty() {
        parts.push(current);
    }

    let multi_part = parts.len() > 1;
    for (i, part_files) in parts.into_iter().enumerate() {
        let mut name = base_name.to_string();
        if stored {
            name.push_str(".stored");
        }
        if multi_part {
            name.push('.');
            name.push_str(&(i + 1).to_string());
        }
        name.push_str(".pack");

        let out_path = opts.output.join(&name);
        write_pack_file(&out_path, &part_files, !stored, opts.level, hasher)?;
        pack_entries.push(PackEntry { file: name, compressed: !stored });
    }
    Ok(())
}

fn write_pack_file(
    out_path: &Path,
    files: &[FileEntry],
    compress: bool,
    level: i32,
    hasher: &mut Sha256,
) -> Result<(), String> {
    let out_file = File::create(out_path).map_err(|e| format!("creating {out_path:?}: {e}"))?;
    if compress {
        let encoder =
            zstd::Encoder::new(out_file, level).map_err(|e| format!("{out_path:?}: {e}"))?;
        let mut builder = tar::Builder::new(encoder);
        append_entries(&mut builder, files, hasher)?;
        let encoder = builder.into_inner().map_err(|e| format!("{out_path:?}: {e}"))?;
        let mut inner = encoder.finish().map_err(|e| format!("{out_path:?}: {e}"))?;
        inner.flush().map_err(|e| format!("{out_path:?}: {e}"))?;
    } else {
        let mut builder = tar::Builder::new(out_file);
        append_entries(&mut builder, files, hasher)?;
        let mut inner = builder.into_inner().map_err(|e| format!("{out_path:?}: {e}"))?;
        inner.flush().map_err(|e| format!("{out_path:?}: {e}"))?;
    }
    Ok(())
}

fn append_entries<W: Write>(
    builder: &mut tar::Builder<W>,
    files: &[FileEntry],
    hasher: &mut Sha256,
) -> Result<(), String> {
    for f in files {
        let bytes = fs::read(&f.abs_path).map_err(|e| format!("reading {:?}: {e}", f.abs_path))?;
        hasher.update(f.rel_path.as_bytes());
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(&bytes);

        let mut header = tar::Header::new_gnu();
        header
            .set_path(&f.rel_path)
            .map_err(|e| format!("tar path {}: {e}", f.rel_path))?;
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append(&header, bytes.as_slice())
            .map_err(|e| format!("appending {}: {e}", f.rel_path))?;
    }
    Ok(())
}
