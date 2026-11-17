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
//! and `write_launch_json` in Packmaster for `--content-compress none`:
//!
//! ```json
//! { "content_dir": null, "url": "dist/index.html",
//!   "args": ["--window-size", "1280x720", "--window-title", "My game"] }
//! ```
//!
//! A packaged launch becomes `--game <dir of url> --window`, reusing the whole
//! `--game` path (`game://content/` origin, its URL checks, CSP and fetch
//! policy, the window and its close path), with the entry document named by
//! `url`. Packed content (`content_dir` set, `.pack` archives) is refused with
//! an explicit error instead of silently running the probe fixtures: the
//! shared extractor (`support/content-packer`) is not in this crate's graph
//! yet. Of the Servo shell options in `args`, only `--window-title` is used;
//! the others are reported and ignored. As in the Servo shell,
//! `ROVES_EXTRA_ARGS` (whitespace separated) appends probe options to a
//! packaged launch, the explicit way to enable diagnostics such as
//! `--mcp-discover` in a `dev-mcp` build; nothing is enabled by default.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use deno_core::serde_json::Value;

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

/// Pure resolution, separated from the process environment for tests.
pub(crate) fn resolve(
    argv: Vec<String>,
    exe: Option<&Path>,
    extra_args: Option<&str>,
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
    let mut bundled = bundled_from_config(exe_dir, &config)
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

fn bundled_from_config(base: &Path, config: &Value) -> Result<Bundled, String> {
    if let Some(packed) = config.get("content_dir").and_then(Value::as_str) {
        return Err(format!(
            "packed content ({packed:?}, .pack archives) is not supported by the Blitz entry \
             point yet; bundle the game with --content-compress none"
        ));
    }
    let url = config
        .get("url")
        .and_then(Value::as_str)
        .ok_or("neither \"content_dir\" nor \"url\" is set")?;
    // Same relative layout the bundlers write; an absolute or `..` path would
    // point the game origin outside the bundle.
    let relative = Path::new(url);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!("\"url\" must be a path inside the bundle: {url:?}"));
    }
    let html_path = base.join(relative);
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

/// Parent of the per-bundle storage directories: persistent, under the same
/// per-user cache root and `roves` folder the Servo shell uses for a
/// loose-file bundle (`extract::game_data_dir(None)` in
/// `support/content-packer`), in its own subdirectory because
/// deno_webstorage's SQLite files are not Servo's format.
fn bundled_storage_root() -> PathBuf {
    let cache = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    };
    cache
        .unwrap_or_else(std::env::temp_dir)
        .join("roves")
        .join("blitz-webstorage")
}

#[cfg(test)]
mod tests {
    use super::*;

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
