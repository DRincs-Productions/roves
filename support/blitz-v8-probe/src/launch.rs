//! The probe's effective command line, including the packaged (double-click)
//! launch of a bundled game.
//!
//! A bundle made by `mach bundle` (or Roves Packmaster) ships one executable
//! with a `launch.json` next to it; the Servo shell reads it in
//! `ports/servoshell/desktop/bundle_launch.rs`. This module accepts the same
//! file, with the same rule for when it applies: only when the process got no
//! arguments at all and the file exists next to the executable. Any explicit
//! argument (`--game <dir>`, the fixture run's flags, a developer invocation)
//! leaves the real `argv` untouched, so the default fixture run and `--game`
//! behave exactly as before.
//!
//! Supported `launch.json` (paths relative to the file's own directory), as
//! written by `_write_launch_config` in `python/servo/post_build_commands.py`
//! and `write_launch_json` in Packmaster. Loose files
//! (`--content-compress none`):
//!
//! ```json
//! { "content_dir": null, "url": "dist/index.html",
//!   "args": ["--window-size", "1280x720", "--window-title", "My game"] }
//! ```
//!
//! Packed content (`--content-compress auto`, the default): `content_dir`
//! names the directory holding `manifest.json` and the `.pack` archives
//! written by `support/content-packer` (zstd-compressed tar, or plain tar for
//! already-compressed media), and the entry document comes from the
//! manifest's `entry_html`:
//!
//! ```json
//! { "content_dir": "dist", "url": null, "args": [...] }
//! ```
//!
//! On macOS, as in the Servo shell, packed content lives in the bundle's
//! `Contents/Resources/` rather than next to the executable.
//!
//! A packaged launch becomes `--game <content root> --window`, reusing the
//! whole `--game` path (`game://content/` origin, its URL checks, CSP and fetch
//! policy, the window and its close path). For loose files the content root
//! is the directory of `url`. For packed content it is the shared extractor's
//! per-install cache (`extract::resolve_dest`, the same directory the Servo
//! shell uses): the boot archives are extracted there before the page is
//! parsed, synchronously (the Servo shell shows a splash and extracts on a
//! background thread; this entry point has no splash), with the extractor's
//! own content-hash invalidation; every other archive is extracted the first
//! time a `game://content/` request touches one of its files
//! ([`PackedContent::ensure_available`], `src/game.rs`). A manifest of another
//! format version, a missing archive or a failed extraction is an error, never
//! a fallback to other content. Of the Servo shell options in `args`, only
//! `--window-title` is used; the others are reported and ignored. As in the
//! Servo shell, `ROVES_EXTRA_ARGS` (whitespace separated) appends probe
//! options to a packaged launch, the explicit way to enable diagnostics such
//! as `--mcp-discover` in a `dev-mcp` build; nothing is enabled by default.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use deno_core::serde_json::Value;
use roves_content_packer::extract::{self, ExtractOptions};
use roves_content_packer::manifest::{FORMAT_VERSION, Manifest};

const LAUNCH_CONFIG_FILE: &str = "launch.json";
/// Same switch as the Servo shell's bundled launch (`bundle_launch.rs`).
const EXTRA_ARGS_ENV: &str = "ROVES_EXTRA_ARGS";

/// A packaged launch resolved from `launch.json`.
#[derive(Debug)]
pub struct Bundled {
    /// Directory `game://content/` maps to (the entry document's directory).
    pub content_root: PathBuf,
    /// File name of the entry document inside `content_root`.
    pub entry_html: String,
    /// `--window-title` from the `args` of `launch.json`.
    pub window_title: Option<String>,
    /// Servo shell options of `launch.json` this entry point does not apply.
    pub ignored_args: Vec<String>,
    /// Stable id of this installed bundle (see [`bundle_id`]): keeps the
    /// persistent storage of two packaged games apart.
    pub storage_id: String,
    /// Set for packed content: `content_root` is then its extraction cache.
    pub packed: Option<Arc<PackedContent>>,
}

/// Packed content of a packaged launch, after its boot archives were
/// extracted into the cache directory (the launch's `content_root`).
#[derive(Debug)]
pub struct PackedContent {
    /// Canonical directory of `manifest.json` and the `.pack` archives.
    content_dir: PathBuf,
    /// Extraction cache, `game://content/`'s root.
    cache_dir: PathBuf,
    manifest: Manifest,
    /// One extraction at a time: two requests for files of the same
    /// not-yet-extracted archive must not unpack it concurrently.
    extracting: Mutex<()>,
}

impl PackedContent {
    /// Extracts the archive holding `path` (a file under the cache directory)
    /// unless the extractor's per-archive marker says it already was. Paths
    /// outside the manifest (excluded files, copied at boot, or missing ones)
    /// are left to the caller's plain file read.
    pub fn ensure_available(&self, path: &Path) -> Result<(), String> {
        let Ok(relative) = path.strip_prefix(&self.cache_dir) else {
            return Ok(());
        };
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let _guard = self
            .extracting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        extract::ensure_file_available(
            &self.content_dir,
            &self.cache_dir,
            &self.manifest,
            &relative,
        )
        .map(|_| ())
        .map_err(|error| format!("extracting game://content/{relative}: {error}"))
    }
}

#[derive(Debug)]
pub struct Launch {
    /// Effective `argv`, including the program name, like `std::env::args()`.
    pub(crate) args: Vec<String>,
    pub(crate) bundled: Option<Bundled>,
}

static LAUNCH: OnceLock<Launch> = OnceLock::new();

fn passthrough() -> Launch {
    Launch {
        args: std::env::args().collect(),
        bundled: None,
    }
}

fn launch() -> &'static Launch {
    LAUNCH.get_or_init(passthrough)
}

/// Resolves the launch once, before anything reads the command line. A
/// `launch.json` that is present but unusable is an error rather than a
/// fallback to the fixture run, which means nothing to a player.
pub fn init() -> Result<(), String> {
    let launch = resolve(
        std::env::args().collect(),
        std::env::current_exe().ok().as_deref(),
        std::env::var(EXTRA_ARGS_ENV).ok().as_deref(),
    )?;
    if let Some(bundled) = &launch.bundled {
        println!(
            "ROVES_PROBE_STAGE bundled_launch {} {}",
            bundled.content_root.display(),
            bundled.entry_html
        );
        if let Some(packed) = &bundled.packed {
            let packs = &packed.manifest.packs;
            println!(
                "ROVES_PROBE_LAUNCH_PACKED {} archives ({} compressed, {} boot) from {}",
                packs.len(),
                packs.iter().filter(|pack| pack.compressed).count(),
                packs.iter().filter(|pack| pack.boot).count(),
                packed.content_dir.display()
            );
        }
        if !bundled.ignored_args.is_empty() {
            eprintln!(
                "ROVES_PROBE_LAUNCH ignoring launch.json options: {:?}",
                bundled.ignored_args
            );
        }
    }
    // A second init (not done today) keeps the first resolution.
    let _ = LAUNCH.set(launch);
    Ok(())
}

/// The effective arguments, program name first (drop-in for `std::env::args()`).
pub fn args() -> impl ExactSizeIterator<Item = String> {
    launch().args.clone().into_iter()
}

pub fn has_flag(flag: &str) -> bool {
    launch().args.iter().skip(1).any(|arg| arg == flag)
}

pub fn bundled() -> Option<&'static Bundled> {
    launch().bundled.as_ref()
}

/// Resolution separated from the process environment for tests. For packed
/// content it extracts the boot archives into the shared per-install cache.
pub(crate) fn resolve(
    argv: Vec<String>,
    exe: Option<&Path>,
    extra_args: Option<&str>,
) -> Result<Launch, String> {
    resolve_into(argv, exe, extra_args, None)
}

/// [`resolve`] with an explicit extraction cache (`extract::ExtractOptions::dest`)
/// instead of the per-user default, so tests do not write to the user's cache.
pub(crate) fn resolve_into(
    argv: Vec<String>,
    exe: Option<&Path>,
    extra_args: Option<&str>,
    cache_dir: Option<PathBuf>,
) -> Result<Launch, String> {
    let unchanged = |argv| {
        Ok(Launch {
            args: argv,
            bundled: None,
        })
    };
    if argv.len() != 1 {
        return unchanged(argv);
    }
    let Some(exe_dir) = exe.and_then(Path::parent) else {
        return unchanged(argv);
    };
    let config_path = exe_dir.join(LAUNCH_CONFIG_FILE);
    let text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        // The common case: a development binary has no launch.json.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return unchanged(argv),
        Err(error) => return Err(format!("reading {}: {error}", config_path.display())),
    };
    let config: Value = deno_core::serde_json::from_str(&text)
        .map_err(|error| format!("parsing {}: {error}", config_path.display()))?;
    let mut bundled = bundled_from_config(exe_dir, &config, cache_dir)
        .map_err(|error| format!("{}: {error}", config_path.display()))?;
    bundled.storage_id = bundle_id(exe_dir)?;

    let mut args = argv;
    args.extend([
        "--game".to_owned(),
        bundled.content_root.to_string_lossy().into_owned(),
        "--window".to_owned(),
    ]);
    if let Some(extra) = extra_args {
        eprintln!("ROVES_PROBE_LAUNCH {EXTRA_ARGS_ENV}: appending {extra:?}");
        args.extend(extra.split_whitespace().map(str::to_owned));
    }
    Ok(Launch {
        args,
        bundled: Some(bundled),
    })
}

/// `value` as a relative path of plain names: the bundlers' layout. An
/// absolute or `..` path would point the game origin outside the bundle.
fn inside_bundle<'a>(field: &str, value: &'a str) -> Result<&'a Path, String> {
    let relative = Path::new(value);
    if value.is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "\"{field}\" must be a path inside the bundle: {value:?}"
        ));
    }
    Ok(relative)
}

/// Where packed content sits relative to the executable: macOS app bundles
/// keep it in `Contents/Resources/` (`_bundle_macos`), as the Servo shell's
/// `bundle_launch.rs` reads it; elsewhere it is next to the executable.
fn packed_content_base(exe_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        exe_dir.join("..").join("Resources")
    } else {
        exe_dir.to_path_buf()
    }
}

/// Packed content: the shared extractor's manifest, cache and boot
/// extraction. Returns the content root (the cache), the entry document and
/// the state for on-demand extraction.
fn packed_from_config(
    content_dir: &Path,
    cache_dir: Option<PathBuf>,
) -> Result<(PathBuf, String, Arc<PackedContent>), String> {
    let manifest = extract::load_manifest(content_dir)?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "{}: packed-content format {} (this build reads format {FORMAT_VERSION})",
            content_dir.join("manifest.json").display(),
            manifest.format_version
        ));
    }
    let entry_html = inside_bundle("entry_html", &manifest.entry_html)?
        .to_string_lossy()
        .into_owned();
    let (content_dir, cache_dir) =
        extract::resolve_dest(content_dir, cache_dir, manifest.name.as_deref())?;
    let cache_dir = extract::extract_boot(&ExtractOptions {
        content_dir: content_dir.clone(),
        dest: Some(cache_dir),
        force: false,
    })
    .map_err(|error| format!("extracting the boot archives: {error}"))?;
    if !cache_dir.join(&entry_html).is_file() {
        return Err(format!(
            "entry document {entry_html:?} is not in the extracted boot set at {}",
            cache_dir.display()
        ));
    }
    let packed = PackedContent {
        content_dir,
        cache_dir: cache_dir.clone(),
        manifest,
        extracting: Mutex::new(()),
    };
    Ok((cache_dir, entry_html, Arc::new(packed)))
}

fn bundled_from_config(
    base: &Path,
    config: &Value,
    cache_dir: Option<PathBuf>,
) -> Result<Bundled, String> {
    let (content_root, entry_html, packed) =
        if let Some(packed) = config.get("content_dir").filter(|value| !value.is_null()) {
            let packed = packed
                .as_str()
                .ok_or("\"content_dir\" must be a string or null")?;
            let content_dir = packed_content_base(base).join(inside_bundle("content_dir", packed)?);
            let (root, entry, packed) = packed_from_config(&content_dir, cache_dir)?;
            (root, entry, Some(packed))
        } else {
            let url = config
                .get("url")
                .and_then(Value::as_str)
                .ok_or("neither \"content_dir\" nor \"url\" is set")?;
            let html_path = base.join(inside_bundle("url", url)?);
            if !html_path.is_file() {
                return Err(format!("entry document {} not found", html_path.display()));
            }
            // As in the Servo shell: the content root is the entry document's directory.
            let content_root = html_path
                .parent()
                .ok_or("entry document has no directory")?
                .to_path_buf();
            let entry_html = html_path
                .file_name()
                .ok_or("entry document has no file name")?
                .to_string_lossy()
                .into_owned();
            (content_root, entry_html, None)
        };

    let mut window_title = None;
    let mut ignored_args = Vec::new();
    let mut options = config
        .get("args")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned);
    while let Some(option) = options.next() {
        if option == "--window-title" {
            window_title = options.next();
        } else {
            ignored_args.push(option);
        }
    }
    Ok(Bundled {
        content_root,
        entry_html,
        window_title,
        ignored_args,
        storage_id: String::new(),
        packed,
    })
}

/// Stable id of the bundle in `bundle_dir` (the directory holding the
/// executable's `launch.json`): the first 8 bytes, in hex, of the SHA-256 of
/// its canonical path, the digest and length `support/content-packer`'s
/// `default_dest` uses for the Servo shell's per-install cache. Canonicalizing
/// first gives one id per real directory however it was reached (`.`/`..`,
/// symlinks or junctions, Windows letter case), and a different id for each
/// copy of a game, so a moved bundle starts a new storage, as the Servo
/// shell's extracted content does.
pub(crate) fn bundle_id(bundle_dir: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let canonical = std::fs::canonicalize(bundle_dir).map_err(|error| {
        format!(
            "resolving bundle directory {}: {error}",
            bundle_dir.display()
        )
    })?;
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    Ok(digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Origin storage directory: deno_webstorage's SQLite localStorage, through
/// which the probe's IndexedDB subset (`src/indexeddb.js`) also persists, so
/// both always share this root. An explicit `ROVES_PROBE_STORAGE_DIR` wins (CI
/// points two runs at one directory to check persistence); a packaged launch
/// persists per bundle; any other run (fixtures, `--game`) keeps the shared
/// temporary directory it always used.
pub fn storage_dir(explicit: Option<std::ffi::OsString>, bundled: Option<&Bundled>) -> PathBuf {
    if let Some(dir) = explicit {
        return PathBuf::from(dir);
    }
    match bundled {
        Some(bundled) => bundled_storage_root().join(&bundled.storage_id),
        None => std::env::temp_dir().join("roves-blitz-probe-storage"),
    }
}

/// Parent of the per-bundle storage directories: persistent, under the
/// per-user cache root and `roves` folder the Servo shell uses for a
/// loose-file bundle (the shared `extract::game_data_dir(None)`), for packed
/// content too, in its own subdirectory because deno_webstorage's SQLite
/// files are not Servo's format.
fn bundled_storage_root() -> PathBuf {
    extract::game_data_dir(None).join("blitz-webstorage")
}

/// A copy of `fixtures/bundle-packed` in a fresh directory, laid out for this
/// platform (packed content under `Resources/` on macOS). Returns the bundle
/// directory to remove afterwards, the executable path and an extraction
/// cache path inside it.
#[cfg(test)]
pub(crate) fn packed_test_bundle(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/bundle-packed");
    let root =
        std::env::temp_dir().join(format!("roves-launch-packed-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let exe_dir = if cfg!(target_os = "macos") {
        root.join("MacOS")
    } else {
        root.clone()
    };
    let dist = packed_content_base(&exe_dir).join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::copy(
        fixture.join(LAUNCH_CONFIG_FILE),
        exe_dir.join(LAUNCH_CONFIG_FILE),
    )
    .unwrap();
    for entry in std::fs::read_dir(fixture.join("dist")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dist.join(entry.file_name())).unwrap();
    }
    let cache = root.join("cache");
    (root, exe_dir.join("play.exe"), cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed_launch(exe: &Path, cache: &Path) -> Result<Launch, String> {
        resolve_into(
            vec!["play".into()],
            Some(exe),
            None,
            Some(cache.to_path_buf()),
        )
    }

    fn edit_manifest(exe: &Path, edit: impl FnOnce(&mut Value)) {
        let path = packed_content_base(exe.parent().unwrap()).join("dist/manifest.json");
        let mut manifest: Value =
            deno_core::serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        edit(&mut manifest);
        std::fs::write(&path, deno_core::serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    // The committed archives are what the shared packer makes of the loose
    // fixture (same files, archive split and content hash), as `mach bundle`
    // runs it. Compressed bytes are not compared: they depend on the zstd build.
    #[test]
    fn packed_fixture_matches_the_loose_sources() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let out = std::env::temp_dir().join(format!("roves-launch-repack-{}", std::process::id()));
        std::fs::remove_dir_all(&out).ok();
        roves_content_packer::pack::pack(&roves_content_packer::pack::PackOptions {
            input: fixtures.join("bundle-loose/dist"),
            output: out.clone(),
            level: 1,
            max_pack_size: 500 * 1024 * 1024,
            exclude: Vec::new(),
            html_file: "game.html".to_owned(),
            boot_include: Vec::new(),
            name: Some("Roves bundle fixture".to_owned()),
        })
        .unwrap();
        let fresh = extract::load_manifest(&out).unwrap();
        let committed = extract::load_manifest(&fixtures.join("bundle-packed/dist")).unwrap();
        std::fs::remove_dir_all(&out).ok();
        assert_eq!(committed.content_hash, fresh.content_hash);
        assert_eq!(committed.files, fresh.files);
        assert_eq!(committed.entry_html, fresh.entry_html);
        assert_eq!(committed.name, fresh.name);
        let packs = |manifest: &Manifest| {
            manifest
                .packs
                .iter()
                .map(|pack| (pack.file.clone(), pack.compressed, pack.boot))
                .collect::<Vec<_>>()
        };
        assert_eq!(packs(&committed), packs(&fresh));
        // A genuine compressed fixture: zstd archives, one of them past the boot set.
        assert!(committed.packs.iter().all(|pack| pack.compressed));
        assert!(committed.packs.iter().any(|pack| !pack.boot));
        assert_eq!(committed.files["data/lazy.json"], "data.pack");
        for pack in &committed.packs {
            let bytes =
                std::fs::read(fixtures.join("bundle-packed/dist").join(&pack.file)).unwrap();
            assert_eq!(
                bytes[..4],
                [0x28, 0xb5, 0x2f, 0xfd],
                "{} is zstd",
                pack.file
            );
        }
    }

    // A relaunch reuses the cache (lazily extracted files stay); a changed
    // content hash rebuilds it, the shared extractor's invalidation.
    #[test]
    fn packed_cache_is_reused_until_the_content_changes() {
        let (root, exe, cache) = packed_test_bundle("relaunch");
        let first = packed_launch(&exe, &cache).unwrap();
        let packed = first.bundled.as_ref().unwrap().packed.clone().unwrap();
        packed
            .ensure_available(&cache.join("data/lazy.json"))
            .unwrap();
        let again = packed_launch(&exe, &cache).map(|_| ());
        let kept = cache.join("data/lazy.json").is_file();
        edit_manifest(&exe, |manifest| {
            manifest["content_hash"] = "sha256:changed".into()
        });
        let changed = packed_launch(&exe, &cache).map(|_| ());
        let rebuilt = !cache.join("data/lazy.json").exists() && cache.join("game.html").is_file();
        std::fs::remove_dir_all(&root).ok();
        again.unwrap();
        changed.unwrap();
        assert!(kept, "a relaunch keeps lazily extracted files");
        assert!(
            rebuilt,
            "a new content hash rebuilds the cache from the boot set"
        );
    }

    #[test]
    fn broken_packed_content_is_an_error() {
        let failure = |name: &str, break_it: &dyn Fn(&Path)| {
            let (root, exe, cache) = packed_test_bundle(name);
            break_it(&exe);
            let result = packed_launch(&exe, &cache).map(|_| ());
            std::fs::remove_dir_all(&root).ok();
            result.expect_err(name)
        };
        let dist = |exe: &Path| packed_content_base(exe.parent().unwrap()).join("dist");
        let config = |exe: &Path, text: &str| {
            std::fs::write(exe.parent().unwrap().join(LAUNCH_CONFIG_FILE), text).unwrap()
        };
        let error = failure("format", &|exe| {
            edit_manifest(exe, |manifest| manifest["format_version"] = 99.into())
        });
        assert!(error.contains("format 99"), "{error}");
        let error = failure("corrupt", &|exe| {
            std::fs::write(dist(exe).join("__boot__.pack"), b"not zstd").unwrap()
        });
        assert!(error.contains("boot archives"), "{error}");
        failure("no-manifest", &|exe| {
            std::fs::remove_file(dist(exe).join("manifest.json")).unwrap()
        });
        failure("entry-escape", &|exe| {
            edit_manifest(exe, |manifest| {
                manifest["entry_html"] = "../game.html".into()
            })
        });
        failure("dir-escape", &|exe| {
            config(exe, r#"{"content_dir":"../dist","url":null,"args":[]}"#)
        });
        failure("dir-type", &|exe| {
            config(exe, r#"{"content_dir":1,"url":null,"args":[]}"#)
        });

        // A missing lazy archive is reported on access, not replaced.
        let (root, exe, cache) = packed_test_bundle("lazy-missing");
        std::fs::remove_file(dist(&exe).join("data.pack")).unwrap();
        let launch = packed_launch(&exe, &cache);
        let lazy = launch.as_ref().map(|launch| {
            let packed = launch.bundled.as_ref().unwrap().packed.as_ref().unwrap();
            packed.ensure_available(&cache.join("data/lazy.json"))
        });
        std::fs::remove_dir_all(&root).ok();
        let error = lazy.unwrap().unwrap_err();
        assert!(error.contains("data/lazy.json"), "{error}");
    }

    /// A loose bundle (launch.json + dist/index.html) in a fresh directory.
    fn bundle(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("roves-launch-id-{}-{name}", std::process::id()))
            .join("game");
        std::fs::create_dir_all(dir.join("dist")).unwrap();
        std::fs::write(dir.join("dist/index.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join(LAUNCH_CONFIG_FILE),
            r#"{"content_dir":null,"url":"dist/index.html","args":[]}"#,
        )
        .unwrap();
        dir
    }

    fn storage_of(exe: &Path) -> PathBuf {
        let launch = resolve(vec!["play".into()], Some(exe), None).unwrap();
        storage_dir(None, launch.bundled.as_ref())
    }

    // Two copies of a game (same layout, same file names, different folders)
    // must not share localStorage/IndexedDB.
    #[test]
    fn distinct_bundles_get_distinct_storage() {
        let first = bundle("first");
        let second = bundle("second");
        let a = storage_of(&first.join("play.exe"));
        let b = storage_of(&second.join("play.exe"));
        let again = storage_of(&first.join("play.exe"));
        std::fs::remove_dir_all(first.parent().unwrap()).ok();
        std::fs::remove_dir_all(second.parent().unwrap()).ok();
        assert_ne!(a, b);
        assert_eq!(a, again, "the id is stable across launches");
        assert_eq!(a.parent(), Some(bundled_storage_root().as_path()));
        let id = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            id.len() == 16 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{id}"
        );
        // Neither is the shared directory of fixture and --game runs.
        assert_ne!(a, storage_dir(None, None));
    }

    #[test]
    fn equivalent_bundle_paths_share_storage() {
        let dir = bundle("equivalent");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let plain = storage_of(&dir.join("play.exe"));
        let mut equivalent = vec![
            dir.join(".").join("play.exe"),
            dir.join("sub").join("..").join("play.exe"),
            std::fs::canonicalize(&dir).unwrap().join("play.exe"),
        ];
        if cfg!(windows) {
            // NTFS names are case-insensitive: the same directory in other case.
            let upper = PathBuf::from(dir.to_string_lossy().to_uppercase());
            equivalent.push(upper.join("play.exe"));
        }
        let ids: Vec<PathBuf> = equivalent.iter().map(|exe| storage_of(exe)).collect();
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
        for (exe, id) in equivalent.iter().zip(ids) {
            assert_eq!(id, plain, "{}", exe.display());
        }
    }

    #[test]
    fn explicit_storage_dir_and_non_bundled_runs_are_unchanged() {
        let dir = bundle("explicit");
        let launch = resolve(vec!["play".into()], Some(&dir.join("play.exe")), None).unwrap();
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
        let explicit = PathBuf::from("explicit-storage");
        assert_eq!(
            storage_dir(Some(explicit.clone().into()), launch.bundled.as_ref()),
            explicit
        );
        assert_eq!(storage_dir(Some(explicit.clone().into()), None), explicit);
        assert_eq!(
            storage_dir(None, None),
            std::env::temp_dir().join("roves-blitz-probe-storage")
        );
    }
}
