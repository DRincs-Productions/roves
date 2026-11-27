//! Packed game content: the game folder as a few ZIP archives instead of loose files
//! (README "Packed game content"; written by roves-cli). Shared by the three players through
//! `server.rs`.
//!
//! Layout: `game/roves-content.json` (`{"format": 1, "code": "code.pack", "assets":
//! ["data-root.pack", ...]}`) next to the archives it names. `code.pack` holds the entry
//! document `index.html` and the other code files (JS chunks, CSS, HTML, JSON, WASM); every
//! `data-*.pack` holds the assets of one top-level folder, split at the packer's size limit.
//! Entries are stored (the default) or raw deflate (compression level 1-9).
//!
//! Reading follows Chromium's resource packs (f2700724f59f `ui/base/resource/data_pack.cc`:
//! `DataPack::LoadFromPathInternal` maps the whole `.pak` with `base::MemoryMappedFile` and
//! `GetStringView` returns views into the mapping; `resource_bundle.cc` `DecompressIfNeeded`
//! inflates compressed resources only when they are requested):
//! - every archive is memory-mapped once; only the ZIP central directory is parsed (the `zip`
//!   crate) into one path index;
//! - a stored entry is served as a zero-copy [`Bytes`] view into the mapping: no read call,
//!   no copy, the OS page cache is the cache and can reclaim it under memory pressure;
//! - compressed code is inflated at boot (behind the startup screen) and kept in RAM, as the
//!   page needs all of it; compressed assets are inflated on first request into a bounded
//!   in-memory cache ([`quick_cache`], weighted by bytes), never written back to disk.
//!
//! Prediction (Roves addition; Chromium relies on its own caches): a request queues the
//! entries that follow it in the same archive (the packer writes them in path order, so these
//! are the same folder's next files) on a background thread, which touches their pages
//! (stored) or inflates them into the cache (compressed). After boot the same thread warms the
//! archives' pages sequentially, chunk by chunk, always serving queued requests first.

use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{self, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};

use bytes::Bytes;
use memmap2::Mmap;
use zip::CompressionMethod;

/// Name of the manifest that turns a game folder into packed content.
pub const MANIFEST: &str = "roves-content.json";
/// The only manifest format this player reads.
pub const FORMAT: u64 = 1;
/// The document the player opens; must be in the code archive.
const ENTRY: &str = "index.html";

/// Budget of inflated assets kept in RAM (compressed archives only).
const ASSET_CACHE_BYTES: u64 = 256 << 20;
/// How far a request predicts in its archive: entries and bytes after the requested one.
const PREDICT_ENTRIES: u32 = 64;
const PREDICT_BYTES: u64 = 32 << 20;
/// Background warm-up after boot: at most this much of the archives, in chunks of this size
/// (a queued prediction waits at most one chunk).
const WARM_LIMIT: u64 = 1 << 30;
const WARM_CHUNK: u64 = 4 << 20;
const PAGE: usize = 4096;

/// A loose game (`index.html`) or packed content (the manifest).
pub fn is_game_dir(dir: &Path) -> bool {
    dir.join(ENTRY).is_file() || dir.join(MANIFEST).is_file()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Method {
    Stored,
    Deflated,
}

struct Entry {
    archive: u32,
    /// Position in the archive's `order` (data offset order).
    order: u32,
    start: u64,
    csize: u64,
    size: u64,
    crc: u32,
    method: Method,
}

struct Archive {
    name: String,
    map: Arc<Mmap>,
    /// Entry paths sorted by data offset (the packer's path order).
    order: Vec<String>,
    /// Highest `order` index already predicted (+1), so overlapping requests do not repeat work.
    predicted: AtomicU32,
}

/// One served file: the bytes and a strong validator.
pub struct Hit {
    pub bytes: Bytes,
    pub crc: u32,
}

enum Job {
    Predict { archive: u32, after: u32 },
}

struct Inner {
    archives: Vec<Archive>,
    entries: HashMap<String, Entry>,
    /// Code inflated at boot (compressed code archive only).
    code: HashMap<String, Bytes>,
    cache: quick_cache::sync::Cache<String, Bytes, Weight>,
    stats: Stats,
}

#[derive(Default)]
pub struct Stats {
    pub requests: AtomicU64,
    pub misses: AtomicU64,
    pub inflated: AtomicU64,
    pub inflated_bytes: AtomicU64,
    pub predicted_bytes: AtomicU64,
    pub warmed_bytes: AtomicU64,
}

impl Stats {
    pub fn report(&self) -> String {
        let n = |v: &AtomicU64| v.load(Ordering::Relaxed);
        format!(
            "content requests={} misses={} inflated={} inflated_bytes={} predicted_bytes={} \
             warmed_bytes={}",
            n(&self.requests),
            n(&self.misses),
            n(&self.inflated),
            n(&self.inflated_bytes),
            n(&self.predicted_bytes),
            n(&self.warmed_bytes)
        )
    }
}

#[derive(Clone)]
struct Weight;

impl quick_cache::Weighter<String, Bytes> for Weight {
    fn weight(&self, _key: &String, value: &Bytes) -> u64 {
        value.len().max(1) as u64
    }
}

/// The inflated-asset cache. Few shards: quick_cache splits the byte budget per shard and never
/// keeps an item larger than its shard, so 4 shards keep assets up to 64 MiB.
fn new_cache() -> quick_cache::sync::Cache<String, Bytes, Weight> {
    let options = quick_cache::OptionsBuilder::new()
        .estimated_items_capacity(4096)
        .weight_capacity(ASSET_CACHE_BYTES)
        .shards(4)
        .build()
        .expect("static cache options");
    quick_cache::sync::Cache::with_options(options, Weight, Default::default(), Default::default())
}

/// Packed game content, cheap to clone (shared with the prediction thread).
#[derive(Clone)]
pub struct Content {
    inner: Arc<Inner>,
    jobs: mpsc::Sender<Job>,
    /// Boot report for the player's log.
    pub summary: String,
}

/// Zero-copy owner of a stored entry's bytes inside the mapping.
struct MapSlice {
    map: Arc<Mmap>,
    start: usize,
    end: usize,
}

impl AsRef<[u8]> for MapSlice {
    fn as_ref(&self) -> &[u8] {
        &self.map[self.start..self.end]
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Game-relative path of an archive entry, or `None` for names the player never serves.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl Content {
    /// `Ok(None)` for a loose game folder; the packed content when `dir` has the manifest.
    /// Errors name the broken manifest or archive (never a silent fallback to loose files).
    pub fn open(dir: &Path) -> io::Result<Option<Content>> {
        let manifest = dir.join(MANIFEST);
        if !manifest.is_file() {
            return Ok(None);
        }
        let started = Instant::now();
        let text = std::fs::read_to_string(&manifest)?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| invalid(format!("{}: {e}", manifest.display())))?;
        let format = json["format"].as_u64();
        if format != Some(FORMAT) {
            return Err(invalid(format!(
                "{}: format {format:?}, this player reads format {FORMAT}",
                manifest.display()
            )));
        }
        let code_name = json["code"]
            .as_str()
            .ok_or_else(|| invalid(format!("{}: no \"code\" archive", manifest.display())))?;
        let mut names = vec![code_name.to_owned()];
        for asset in json["assets"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let name = asset.as_str().ok_or_else(|| {
                invalid(format!("{}: \"assets\" must be names", manifest.display()))
            })?;
            names.push(name.to_owned());
        }

        let mut archives = Vec::with_capacity(names.len());
        let mut entries = HashMap::new();
        let mut total = 0u64;
        for (index, name) in names.iter().enumerate() {
            if !valid_name(name) || name.contains('/') {
                return Err(invalid(format!(
                    "{}: bad archive name {name:?}",
                    manifest.display()
                )));
            }
            let path = dir.join(name);
            let file =
                File::open(&path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
            // SAFETY: the archive is a read-only file of the installed package; like Chromium's
            // `.pak` mappings, it must not be truncated while the player runs.
            let map = Arc::new(unsafe { Mmap::map(&file) }?);
            total += map.len() as u64;
            let order = index_archive(&map, index as u32, &mut entries)
                .map_err(|e| invalid(format!("{}: {e}", path.display())))?;
            archives.push(Archive {
                name: name.clone(),
                map,
                order,
                predicted: AtomicU32::new(0),
            });
        }
        match entries.get(ENTRY) {
            Some(entry) if entry.archive == 0 => {},
            _ => {
                return Err(invalid(format!(
                    "{}: {ENTRY} is not in the code archive {code_name}",
                    manifest.display()
                )));
            },
        }

        let mut inner = Inner {
            archives,
            entries,
            code: HashMap::new(),
            cache: new_cache(),
            stats: Stats::default(),
        };
        // Code: inflated now (compressed) or paged in now (stored); the page asks for it first.
        let code = &inner.archives[0];
        let mut code_bytes = 0u64;
        let mut inflated = HashMap::new();
        for path in &code.order {
            let entry = &inner.entries[path];
            code_bytes += entry.size;
            match entry.method {
                Method::Deflated => {
                    inflated.insert(path.clone(), inflate(code, entry, path)?);
                },
                Method::Stored => {
                    touch(&code.map, entry.start, entry.csize);
                },
            }
        }
        inner.code = inflated;
        let compressed = inner.entries.values().any(|e| e.method == Method::Deflated);
        let summary = format!(
            "content archives={} entries={} code_files={} code_bytes={code_bytes} \
             archive_bytes={total} compressed={compressed} open_ms={}",
            inner.archives.len(),
            inner.entries.len(),
            code.order.len(),
            started.elapsed().as_millis()
        );

        let inner = Arc::new(inner);
        let (jobs, queue) = mpsc::channel();
        let worker = Arc::clone(&inner);
        std::thread::Builder::new()
            .name("roves-content-prefetch".into())
            .spawn(move || worker.run(queue))?;
        Ok(Some(Content {
            inner,
            jobs,
            summary,
        }))
    }

    /// The file at a game-relative path (`assets/a.png`), predicting the next ones.
    pub fn get(&self, path: &str) -> Option<Hit> {
        let inner = &self.inner;
        inner.stats.requests.fetch_add(1, Ordering::Relaxed);
        let Some(entry) = inner.entries.get(path) else {
            inner.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };
        let archive = &inner.archives[entry.archive as usize];
        let bytes = if let Some(code) = inner.code.get(path) {
            code.clone()
        } else {
            match entry.method {
                Method::Stored => Bytes::from_owner(MapSlice {
                    map: Arc::clone(&archive.map),
                    start: entry.start as usize,
                    end: (entry.start + entry.csize) as usize,
                }),
                Method::Deflated => inner.cached_inflate(archive, entry, path).ok()?,
            }
        };
        if entry.archive != 0 {
            let _ = self.jobs.send(Job::Predict {
                archive: entry.archive,
                after: entry.order,
            });
        }
        Some(Hit {
            bytes,
            crc: entry.crc,
        })
    }

    /// One standalone archive (a packed mod or DLC, `mods.rs`): every entry is served like an
    /// asset (stored zero-copy, compressed inflated on request into the cache), with no entry
    /// document, no boot inflation, no prediction and no warm-up thread.
    pub fn open_archive(path: &Path) -> io::Result<Content> {
        let file = File::open(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        // SAFETY: as for the game archives, a read-only file of the installation or profile
        // that must not be truncated while the player runs.
        let map = Arc::new(unsafe { Mmap::map(&file) }?);
        let mut entries = HashMap::new();
        let order = index_archive(&map, 0, &mut entries)
            .map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let summary = format!("archive entries={} bytes={}", order.len(), map.len());
        let inner = Inner {
            archives: vec![Archive {
                name: path.display().to_string(),
                map,
                order,
                predicted: AtomicU32::new(0),
            }],
            entries,
            code: HashMap::new(),
            cache: new_cache(),
            stats: Stats::default(),
        };
        // No worker: predictions are never sent for archive 0, and a closed queue is ignored.
        let (jobs, _) = mpsc::channel();
        Ok(Content {
            inner: Arc::new(inner),
            jobs,
            summary,
        })
    }

    /// Every entry path, in no particular order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.inner.entries.keys().map(String::as_str)
    }

    /// Whether the path is an entry (directory redirects: `dir` -> `dir/`).
    pub fn contains(&self, path: &str) -> bool {
        self.inner.entries.contains_key(path)
    }

    pub fn stats(&self) -> &Stats {
        &self.inner.stats
    }
}

/// Indexes one archive's central directory into `entries`; returns the paths in data order.
fn index_archive(
    map: &Mmap,
    archive: u32,
    entries: &mut HashMap<String, Entry>,
) -> Result<Vec<String>, String> {
    let mut zip = zip::ZipArchive::new(io::Cursor::new(&map[..])).map_err(|e| e.to_string())?;
    let mut found = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let file = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().map_err(|e| e.to_string())?.into_owned();
        if !valid_name(&name) {
            return Err(format!("bad entry name {name:?}"));
        }
        let method = match file.compression() {
            CompressionMethod::Stored => Method::Stored,
            m if m == CompressionMethod::DEFLATE => Method::Deflated,
            other => return Err(format!("{name}: unsupported compression {other:?}")),
        };
        let start = file
            .data_start()
            .ok_or_else(|| format!("{name}: no data offset"))?;
        let (csize, size) = (file.compressed_size(), file.size());
        if start
            .checked_add(csize)
            .is_none_or(|end| end > map.len() as u64)
            || (method == Method::Stored && csize != size)
        {
            return Err(format!(
                "{name}: entry outside the archive or size mismatch"
            ));
        }
        found.push((start, name, csize, size, file.crc32(), method));
    }
    found.sort_by_key(|f| f.0);
    let mut order = Vec::with_capacity(found.len());
    for (position, (start, name, csize, size, crc, method)) in found.into_iter().enumerate() {
        let entry = Entry {
            archive,
            order: position as u32,
            start,
            csize,
            size,
            crc,
            method,
        };
        if entries.insert(name.clone(), entry).is_some() {
            return Err(format!("{name} is in more than one archive"));
        }
        order.push(name);
    }
    Ok(order)
}

/// Raw-deflate entry -> bytes, checked against its size and CRC-32.
fn inflate(archive: &Archive, entry: &Entry, path: &str) -> io::Result<Bytes> {
    let data = &archive.map[entry.start as usize..(entry.start + entry.csize) as usize];
    let mut out = Vec::with_capacity(entry.size as usize);
    flate2::read::DeflateDecoder::new(data)
        .take(entry.size + 1)
        .read_to_end(&mut out)?;
    let crc = {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&out);
        hasher.finalize()
    };
    if out.len() as u64 != entry.size || crc != entry.crc {
        return Err(invalid(format!(
            "{}: {path} is corrupt (size or CRC-32 mismatch)",
            archive.name
        )));
    }
    Ok(Bytes::from(out))
}

/// Reads one byte per page so the OS pages the range in (portable read-ahead).
fn touch(map: &Mmap, start: u64, len: u64) -> u64 {
    let (start, end) = (start as usize, (start + len) as usize);
    let mut acc = 0u8;
    let mut at = start;
    while at < end {
        acc ^= map[at];
        at += PAGE;
    }
    if end > start {
        acc ^= map[end - 1];
    }
    std::hint::black_box(acc);
    (end - start) as u64
}

impl Inner {
    /// From the cache, or inflated once: a request and a prediction of the same entry wait
    /// for one another instead of inflating it twice (quick_cache placeholder guard).
    fn cached_inflate(&self, archive: &Archive, entry: &Entry, path: &str) -> io::Result<Bytes> {
        self.cache.get_or_insert_with(path, || {
            let bytes = inflate(archive, entry, path)?;
            self.stats.inflated.fetch_add(1, Ordering::Relaxed);
            self.stats
                .inflated_bytes
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            Ok(bytes)
        })
    }

    /// Prediction jobs first; between them, the warm-up advances one chunk at a time.
    fn run(&self, queue: mpsc::Receiver<Job>) {
        let mut warm: VecDeque<(u32, u64)> = (0..self.archives.len() as u32)
            .skip(1)
            .map(|a| (a, 0))
            .collect();
        let mut warmed = 0u64;
        loop {
            let job = if warm.is_empty() || warmed >= WARM_LIMIT {
                match queue.recv() {
                    Ok(job) => Some(job),
                    Err(_) => return,
                }
            } else {
                match queue.try_recv() {
                    Ok(job) => Some(job),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            };
            match job {
                Some(Job::Predict { archive, after }) => self.predict(archive, after),
                None => {
                    let Some((archive, offset)) = warm.pop_front() else {
                        continue;
                    };
                    let map = &self.archives[archive as usize].map;
                    let len = WARM_CHUNK.min(map.len() as u64 - offset);
                    let done = touch(map, offset, len);
                    warmed += done;
                    self.stats.warmed_bytes.fetch_add(done, Ordering::Relaxed);
                    if offset + len < map.len() as u64 {
                        warm.push_front((archive, offset + len));
                    }
                },
            }
        }
    }

    fn predict(&self, archive: u32, after: u32) {
        let a = &self.archives[archive as usize];
        let from = (after + 1).max(a.predicted.load(Ordering::Relaxed));
        let end = (after + 1 + PREDICT_ENTRIES).min(a.order.len() as u32);
        let mut budget = PREDICT_BYTES;
        let mut next = from;
        while next < end && budget > 0 {
            let path = &a.order[next as usize];
            let entry = &self.entries[path];
            match entry.method {
                Method::Stored => {
                    touch(&a.map, entry.start, entry.csize);
                },
                Method::Deflated => {
                    let _ = self.cached_inflate(a, entry, path);
                },
            }
            self.stats
                .predicted_bytes
                .fetch_add(entry.size, Ordering::Relaxed);
            budget = budget.saturating_sub(entry.size);
            next += 1;
        }
        a.predicted.fetch_max(next, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// Writes `files` as packed content into `dir`: `code` paths into code.pack, the rest into
    /// data-<top folder>.pack, at `level` (0 = stored).
    pub(crate) fn pack(dir: &Path, files: &[(String, Vec<u8>)], code: &[&str], level: i64) {
        let options = if level == 0 {
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
        } else {
            SimpleFileOptions::default()
                .compression_method(CompressionMethod::DEFLATE)
                .compression_level(Some(level))
        };
        let mut groups: Vec<(String, Vec<&(String, Vec<u8>)>)> = Vec::new();
        for file in files {
            let group = if code.contains(&file.0.as_str()) {
                "code.pack".to_owned()
            } else {
                match file.0.split_once('/') {
                    Some((top, _)) => format!("data-{top}.pack"),
                    None => "data-root.pack".to_owned(),
                }
            };
            match groups.iter_mut().find(|(g, _)| *g == group) {
                Some((_, list)) => list.push(file),
                None => groups.push((group, vec![file])),
            }
        }
        for (group, list) in &mut groups {
            list.sort_by(|a, b| a.0.cmp(&b.0));
            let mut zip = zip::ZipWriter::new(File::create(dir.join(&*group)).unwrap());
            for (path, data) in list.iter() {
                zip.start_file(path.as_str(), options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        let assets: Vec<&str> = groups
            .iter()
            .map(|(g, _)| g.as_str())
            .filter(|g| *g != "code.pack")
            .collect();
        let manifest = serde_json::json!({ "format": 1, "code": "code.pack", "assets": assets });
        std::fs::write(dir.join(MANIFEST), manifest.to_string()).unwrap();
    }

    fn sample() -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "index.html".into(),
                b"<!doctype html><script src=main.js></script>".to_vec(),
            ),
            ("main.js".into(), b"console.log('hi')".repeat(50)),
            ("assets/a.png".into(), vec![7u8; 10_000]),
            ("assets/b/c.txt".into(), b"hello ".repeat(2_000)),
            ("readme.txt".into(), b"root asset".to_vec()),
        ]
    }

    #[test]
    fn loose_folder_is_not_packed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "x").unwrap();
        assert!(is_game_dir(dir.path()));
        assert!(Content::open(dir.path()).unwrap().is_none());
    }

    #[test]
    fn stored_and_deflated_round_trip() {
        for level in [0, 1, 9] {
            let dir = tempfile::tempdir().unwrap();
            let files = sample();
            pack(dir.path(), &files, &["index.html", "main.js"], level);
            assert!(is_game_dir(dir.path()));
            let content = Content::open(dir.path()).unwrap().unwrap();
            // code.pack, data-assets.pack, data-root.pack
            assert!(
                content.summary.contains("archives=3"),
                "{}",
                content.summary
            );
            for (path, data) in &files {
                let hit = content.get(path).unwrap();
                assert_eq!(&hit.bytes[..], &data[..], "level {level} {path}");
                assert_eq!(hit.crc, crc32fast::hash(data));
                // A second read hits the mapping or the cache.
                assert_eq!(&content.get(path).unwrap().bytes[..], &data[..]);
            }
            assert!(content.get("missing.js").is_none());
            assert!(content.contains("assets/b/c.txt") && !content.contains("assets/b"));
        }
    }

    #[test]
    fn broken_content_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        pack(dir.path(), &sample(), &["main.js"], 0);
        let err = Content::open(dir.path()).err().unwrap().to_string();
        assert!(
            err.contains("index.html is not in the code archive"),
            "{err}"
        );

        std::fs::write(
            dir.path().join(MANIFEST),
            r#"{"format":2,"code":"code.pack"}"#,
        )
        .unwrap();
        let err = Content::open(dir.path()).err().unwrap().to_string();
        assert!(err.contains("format Some(2)"), "{err}");

        std::fs::write(
            dir.path().join(MANIFEST),
            r#"{"format":1,"code":"../x.pack"}"#,
        )
        .unwrap();
        assert!(Content::open(dir.path()).is_err());

        std::fs::write(
            dir.path().join(MANIFEST),
            r#"{"format":1,"code":"none.pack"}"#,
        )
        .unwrap();
        assert!(Content::open(dir.path()).is_err());
    }

    #[test]
    fn corrupt_deflate_entry_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            ("index.html".to_owned(), b"<p>x</p>".to_vec()),
            (
                "data/big.bin".to_owned(),
                (0..200_000u32).map(|i| (i % 251) as u8).collect(),
            ),
        ];
        pack(dir.path(), &files, &["index.html"], 6);
        let path = dir.path().join("data-data.pack");
        let mut bytes = std::fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();
        let content = Content::open(dir.path()).unwrap().unwrap();
        assert!(content.get("data/big.bin").is_none());
    }

    /// roves-cli's archives (yazl) read back byte for byte against the loose build:
    /// `ROVES_LOOSE_DIR=<dist> ROVES_PACKED_DIR=<roves-cli output> cargo test -- --ignored
    /// reads_roves_cli_archives` (CI runs it on the sample game at every level it packs).
    #[test]
    #[ignore = "needs ROVES_LOOSE_DIR and ROVES_PACKED_DIR"]
    fn reads_roves_cli_archives() {
        let loose = std::path::PathBuf::from(std::env::var("ROVES_LOOSE_DIR").unwrap());
        let packed = std::path::PathBuf::from(std::env::var("ROVES_PACKED_DIR").unwrap());
        let content = Content::open(&packed)
            .unwrap()
            .expect("no roves-content.json");
        println!("{}", content.summary);
        let mut stack = vec![loose.clone()];
        let mut count = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(&loose)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let hit = content.get(&rel).unwrap_or_else(|| panic!("{rel} missing"));
                assert!(
                    hit.bytes[..] == std::fs::read(&path).unwrap()[..],
                    "{rel} differs"
                );
                count += 1;
            }
        }
        assert_eq!(
            count,
            content.inner.entries.len(),
            "extra entries in the archives"
        );
        println!("{count} files identical");
    }

    /// Archive access speed (README "Packed game content"): `cargo test --release -- --ignored
    /// --nocapture archive_access_speed`. Prints, never asserts timings: loose files with
    /// `std::fs::read` against packed content stored and compressed, cold (first access after
    /// opening) and warm (second pass), as throughput and per-file latency.
    #[test]
    #[ignore = "benchmark; run explicitly"]
    fn archive_access_speed() {
        let mut files = vec![
            ("index.html".to_owned(), b"<!doctype html>".to_vec()),
            (
                "main.js".to_owned(),
                b"export const x = 1;\n".repeat(20_000),
            ),
        ];
        // ~120 MB of assets: many small (UI, sprites), some medium (backgrounds), a few large
        // (audio/video), pseudo-random so deflate has real work to do.
        let mut seed = 0x9e37_79b9u32;
        let mut noise = |len: usize| -> Vec<u8> {
            (0..len)
                .map(|i| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    if i % 4 == 0 {
                        (seed & 0xff) as u8
                    } else {
                        (i % 64) as u8
                    }
                })
                .collect()
        };
        for i in 0..600 {
            files.push((format!("images/ui/{i:04}.bin"), noise(16 << 10)));
        }
        for i in 0..120 {
            files.push((format!("images/bg/{i:04}.bin"), noise(600 << 10)));
        }
        for i in 0..6 {
            files.push((format!("audio/{i:02}.bin"), noise(6 << 20)));
        }
        let assets: Vec<&(String, Vec<u8>)> = files.iter().skip(2).collect();
        let total: usize = assets.iter().map(|(_, d)| d.len()).sum();
        println!(
            "archive_access_speed: {} asset files, {:.1} MB",
            assets.len(),
            total as f64 / 1e6
        );

        let report = |label: &str, durations: &mut Vec<f64>, elapsed: f64| {
            durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let pct = |p: f64| durations[((durations.len() - 1) as f64 * p) as usize] * 1e6;
            println!(
                "  {label:<26} {:>8.0} MB/s  p50 {:>7.1} us  p99 {:>8.1} us  total {:>7.1} ms",
                total as f64 / 1e6 / elapsed,
                pct(0.5),
                pct(0.99),
                elapsed * 1e3
            );
        };

        let loose = tempfile::tempdir().unwrap();
        for (path, data) in &files {
            let full = loose.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, data).unwrap();
        }
        for pass in ["loose files (fs::read)", "loose files, 2nd pass"] {
            let mut durations = Vec::new();
            let all = Instant::now();
            for (path, _) in &assets {
                let t = Instant::now();
                let data = std::fs::read(loose.path().join(path)).unwrap();
                std::hint::black_box(data.len());
                durations.push(t.elapsed().as_secs_f64());
            }
            report(pass, &mut durations, all.elapsed().as_secs_f64());
        }

        for level in [0i64, 1, 6] {
            let dir = tempfile::tempdir().unwrap();
            pack(dir.path(), &files, &["index.html", "main.js"], level);
            let open = Instant::now();
            let content = Content::open(dir.path()).unwrap().unwrap();
            println!(
                "  level {level}: open {:.1} ms ({})",
                open.elapsed().as_secs_f64() * 1e3,
                content.summary
            );
            for pass in ["cold", "warm"] {
                let mut durations = Vec::new();
                let all = Instant::now();
                for (path, data) in &assets {
                    let t = Instant::now();
                    let hit = content.get(path).unwrap();
                    // Touch every byte, as the server's socket write would.
                    let sum = hit.bytes.iter().step_by(PAGE).fold(0u8, |a, b| a ^ b);
                    std::hint::black_box(sum);
                    durations.push(t.elapsed().as_secs_f64());
                    assert_eq!(hit.bytes.len(), data.len());
                }
                report(
                    &format!("level {level} {pass}"),
                    &mut durations,
                    all.elapsed().as_secs_f64(),
                );
            }
            let s = content.stats();
            println!(
                "  level {level}: inflated {} files / {:.1} MB, predicted {:.1} MB, warmed {:.1} MB",
                s.inflated.load(Ordering::Relaxed),
                s.inflated_bytes.load(Ordering::Relaxed) as f64 / 1e6,
                s.predicted_bytes.load(Ordering::Relaxed) as f64 / 1e6,
                s.warmed_bytes.load(Ordering::Relaxed) as f64 / 1e6
            );
        }
    }
}
