//! Mods and DLC of the CEF players (README "Mods and DLC"), shared by the three players
//! through `server.rs` (a sibling of `content.rs`; `super::super::host_api` is the players'
//! host API module in every crate that compiles these files).
//!
//! - The game: `game/roves-game.json`, written by roves-cli next to the build (or next to the
//!   content archives), with the `name` and `version` of the game's `package.json`, whether
//!   mods are allowed, the DLC public key and the libraries the game shares. A game without
//!   it has no mod support (older packages).
//! - Add-ons: a folder with a `package.json` or a `.rovesmod` file (a ZIP of that folder,
//!   read through [`Content::open_archive`]), found in `dlc/` and `mods/` next to `game/`,
//!   in `<profile root>/mods`, and in every `--roves-dev-mod=<folder>` (`roves mod dev`).
//! - `package.json`: npm fields (`name` is the id; `displayName`, `description`, `author`,
//!   `version`, `main`, `peerDependencies` = libraries reused from the game,
//!   `dependencies` = libraries bundled by the add-on) and a `roves` section (`kind`,
//!   `game: {name, version}`, `mods`, `steamAppId`). Ranges follow npm (`nodejs-semver`).
//! - The cover is the file named `cover.png`, `cover.jpg` or `cover.webp` at the root.
//! - DLC can only come from the game's developer: a DLC must carry `roves-signature.json`,
//!   an Ed25519 signature (made by `roves mod pack` with the key kept with the game's
//!   source) over the SHA-256 of every other file, verified with the game's public key.
//!   An optional `steamAppId` also requires Steam to report the DLC as installed.
//! - Serving (`server.rs`): an active add-on's `overrides/<path>` replaces or adds the game
//!   file `<path>` (a later add-on in the load order wins), and its own files are served at
//!   `/__mods/<id>/<path>`. Only active add-ons are served, except the cover, as Chromium
//!   serves only enabled extensions' resources but still a disabled extension's icon
//!   (f2700724f59f `extensions/browser/extension_protocols.cc`, `GetDirectoryForExtensionURL`).
//! - State: the disabled ids and the load order in `<profile root>/mods.json` (in memory
//!   without a persistent profile). Chromium keeps the same per-extension state in the
//!   profile prefs (`extension_prefs.cc`, `disable_reasons`) but applies it live; the player
//!   keeps one document per window and blocks reloads, so the active set is fixed at launch
//!   and a change applies at the next launch (`core/relaunch`).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use axum::http::StatusCode;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use nodejs_semver::{Range, Version};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::super::host_api::{Call, Module, Reply, SteamHandle, write_atomic};
use super::content::Content;

/// The game's identity file, next to the build (`game/`).
pub const GAME_FILE: &str = "roves-game.json";
/// The only `roves-game.json` format this player reads.
const GAME_FORMAT: u64 = 1;
/// An add-on's manifest.
pub const PACKAGE: &str = "package.json";
/// A DLC's signature file.
pub const SIGNATURE: &str = "roves-signature.json";
/// Extension of a packed add-on.
pub const ARCHIVE_EXT: &str = "rovesmod";
/// Folder of the replaced and added game files inside an add-on.
pub const OVERRIDES: &str = "overrides/";
/// URL prefix of add-on files.
pub const URL_PREFIX: &str = "/__mods/";
/// `roves mod dev`: an unpacked add-on folder, loaded first and never signature-checked.
#[allow(dead_code)] // the macOS/Linux switch loops; the Windows parser matches `dev-mod`
pub const DEV_SWITCH: &str = "--roves-dev-mod=";
/// Saved state in the profile root.
const STATE_FILE: &str = "mods.json";
/// First line of the signed message (domain separation).
const SIGNATURE_DOMAIN: &str = "roves-dlc-signature-v1\n";
const COVERS: [&str; 3] = ["cover.png", "cover.jpg", "cover.webp"];

/// What a player hands to [`Mods::discover`].
pub struct ModsConfig {
    /// The game build (`game/`); `dlc/` and `mods/` are its siblings.
    pub game_dir: PathBuf,
    /// The persistent profile root (`None` when isolated): `mods/` and `mods.json`.
    pub profile_root: Option<PathBuf>,
    /// `--roves-dev-mod=` folders, in command-line order.
    pub dev: Vec<PathBuf>,
    /// The player's Steamworks client, for `steamAppId` DLC.
    pub steam: Option<Arc<SteamHandle>>,
}

/// The `--roves-dev-mod=` folders of this process's command line.
pub fn dev_mod_dirs() -> Vec<PathBuf> {
    std::env::args()
        .filter_map(|arg| arg.strip_prefix(DEV_SWITCH).map(PathBuf::from))
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect()
}

/// The files of one add-on.
#[derive(Clone)]
pub enum Files {
    Dir(PathBuf),
    Packed(Content),
}

impl Files {
    pub fn has(&self, path: &str) -> bool {
        match self {
            Files::Dir(root) => root.join(path).is_file(),
            Files::Packed(content) => content.contains(path),
        }
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        match self {
            Files::Dir(root) => fs::read(root.join(path)),
            Files::Packed(content) => content
                .get(path)
                .map(|hit| hit.bytes.to_vec())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned())),
        }
    }

    /// Lowercase hex SHA-256 of one file (folders are streamed, archives hashed in place).
    fn sha256(&self, path: &str) -> io::Result<String> {
        let mut hasher = Sha256::new();
        match self {
            Files::Dir(root) => {
                let mut file = File::open(root.join(path))?;
                let mut buffer = vec![0u8; 1 << 20];
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
            },
            Files::Packed(content) => {
                let hit = content
                    .get(path)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))?;
                hasher.update(&hit.bytes);
            },
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }

    /// Every file path, `/`-separated, sorted.
    fn list(&self) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        match self {
            Files::Dir(root) => walk(root, "", &mut out)?,
            Files::Packed(content) => out.extend(content.paths().map(str::to_owned)),
        }
        out.sort();
        Ok(out)
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        let path = format!("{prefix}{name}");
        if kind.is_dir() {
            walk(&entry.path(), &format!("{path}/"), out)?;
        } else if kind.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// The game's `roves-game.json`.
pub struct Game {
    pub name: String,
    pub version: Version,
    /// Third-party mods allowed (`roves bundle --mods=false` turns them off; DLC still load).
    pub mods: bool,
    pub key: Option<VerifyingKey>,
    /// Libraries the game shares with add-ons, with their exact versions.
    pub shared: BTreeMap<String, Version>,
}

impl Game {
    /// `Ok(None)` without the file (no mod support); errors name what is wrong.
    pub fn read(game_dir: &Path) -> Result<Option<Game>, String> {
        let path = game_dir.join(GAME_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Game::parse(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Game, String> {
        let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if json["format"].as_u64() != Some(GAME_FORMAT) {
            return Err(format!("this player reads format {GAME_FORMAT}"));
        }
        let name = json["name"].as_str().filter(|n| !n.is_empty());
        let name = name.ok_or("no \"name\"")?.to_owned();
        let version = json["version"].as_str().ok_or("no \"version\"")?;
        let version = Version::parse(version).map_err(|e| format!("\"version\": {e}"))?;
        let key = match json["dlcPublicKey"].as_str() {
            None => None,
            Some(text) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(text)
                    .ok()
                    .and_then(|b| <[u8; 32]>::try_from(b).ok())
                    .ok_or("\"dlcPublicKey\" is not a base64 Ed25519 public key")?;
                Some(
                    VerifyingKey::from_bytes(&bytes)
                        .map_err(|_| "\"dlcPublicKey\" is not a valid Ed25519 key")?,
                )
            },
        };
        let mut shared = BTreeMap::new();
        if let Some(map) = json["shared"].as_object() {
            for (lib, version) in map {
                let version = version
                    .as_str()
                    .and_then(|v| Version::parse(v).ok())
                    .ok_or_else(|| format!("\"shared\".{lib} is not a version"))?;
                shared.insert(lib.clone(), version);
            }
        }
        Ok(Game {
            name,
            version,
            mods: json["mods"].as_bool().unwrap_or(true),
            key,
            shared,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Mod,
    Dlc,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Location {
    Dev,
    Dlc,
    Game,
    Profile,
}

impl Location {
    fn name(self) -> &'static str {
        match self {
            Location::Dev => "dev",
            Location::Dlc => "dlc",
            Location::Game => "game",
            Location::Profile => "profile",
        }
    }
}

/// One add-on as read from its `package.json`.
pub struct Addon {
    pub id: String,
    pub kind: Kind,
    pub display_name: String,
    pub description: String,
    pub author: Option<String>,
    pub version: String,
    pub main: Option<String>,
    pub peer: BTreeMap<String, String>,
    pub bundled: BTreeMap<String, String>,
    pub game: Option<(String, String)>,
    pub requires: BTreeMap<String, String>,
    pub steam_app_id: Option<u32>,
    pub cover: Option<String>,
    pub location: Location,
    pub files: Files,
    /// Why it cannot load (empty: compatible).
    pub problems: Vec<String>,
}

fn string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// A path inside an add-on that the server may serve (no `..`, no absolute paths).
fn valid_inner_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl Addon {
    /// Parses `package.json`; a broken manifest is an error (the add-on is skipped and logged).
    pub fn parse(text: &str, files: Files, location: Location) -> Result<Addon, String> {
        let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let id = json["name"].as_str().unwrap_or_default().to_owned();
        if !valid_id(&id) {
            return Err(format!("\"name\" {id:?} is not a valid package name"));
        }
        let version = json["version"].as_str().unwrap_or_default().to_owned();
        let roves = &json["roves"];
        let kind = match roves["kind"].as_str() {
            None | Some("mod") => Kind::Mod,
            Some("dlc") => Kind::Dlc,
            Some(other) => return Err(format!("\"roves.kind\" {other:?} is not mod or dlc")),
        };
        let author = match &json["author"] {
            Value::String(name) => Some(name.clone()),
            Value::Object(person) => person
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        };
        let main = json["main"]
            .as_str()
            .map(|m| m.trim_start_matches("./").to_owned());
        let game = roves["game"].as_object().map(|g| {
            (
                g.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                g.get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("*")
                    .to_owned(),
            )
        });
        let mut problems = Vec::new();
        if Version::parse(&version).is_err() {
            problems.push(format!("\"version\" {version:?} is not a semver version"));
        }
        if let Some(main) = &main
            && (!valid_inner_path(main) || !files.has(main))
        {
            problems.push(format!("\"main\" {main} is missing"));
        }
        let cover = COVERS
            .iter()
            .find(|c| files.has(c))
            .map(|c| (*c).to_owned());
        Ok(Addon {
            display_name: json["displayName"].as_str().unwrap_or(&id).to_owned(),
            description: json["description"].as_str().unwrap_or_default().to_owned(),
            id,
            kind,
            author,
            version,
            main,
            peer: string_map(&json["peerDependencies"]),
            bundled: string_map(&json["dependencies"]),
            game,
            requires: string_map(&roves["mods"]),
            steam_app_id: roves["steamAppId"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok()),
            cover,
            location,
            files,
            problems,
        })
    }

    fn info(&self, enabled: bool, active: bool) -> Value {
        let url = |path: &str| format!("{URL_PREFIX}{}/{}", self.id, encode_path(path));
        json!({
            "id": self.id,
            "kind": match self.kind { Kind::Mod => "mod", Kind::Dlc => "dlc" },
            "name": self.display_name,
            "description": self.description,
            "author": self.author,
            "version": self.version,
            "cover_url": self.cover.as_deref().map(url),
            "main_url": self.main.as_deref().map(url),
            "location": self.location.name(),
            "enabled": enabled,
            "active": active,
            "compatible": self.problems.is_empty(),
            "problems": self.problems,
            "game": self.game.as_ref().map(|(name, version)| json!({ "name": name, "version": version })),
            "requires": self.requires,
            "libraries": { "peer": self.peer, "bundled": self.bundled },
            "steam_app_id": self.steam_app_id,
        })
    }
}

/// An npm package name: optional `@scope/`, then lowercase URL-safe characters.
fn valid_id(id: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && p.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'-' | b'_' | b'.' | b'~')
            })
    };
    if id.len() > 214 {
        return false;
    }
    match id.strip_prefix('@') {
        Some(scoped) => matches!(scoped.split_once('/'), Some((s, n)) if part(s) && part(n)),
        None => part(id),
    }
}

/// Percent-encodes a `/`-separated path for a URL (unreserved characters and `/` kept).
pub fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Checks a DLC's `roves-signature.json` against the game's key and every file's hash.
pub fn verify_signature(files: &Files, key: &VerifyingKey) -> Result<(), String> {
    let text = files
        .read(SIGNATURE)
        .map_err(|_| format!("not signed ({SIGNATURE} missing)"))?;
    let json: Value = serde_json::from_slice(&text).map_err(|e| format!("{SIGNATURE}: {e}"))?;
    let listed: BTreeMap<String, String> = string_map(&json["files"]);
    let signature = json["signature"]
        .as_str()
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        .and_then(|b| Signature::from_slice(&b).ok())
        .ok_or_else(|| format!("{SIGNATURE}: bad \"signature\""))?;
    key.verify_strict(signed_message(&listed).as_bytes(), &signature)
        .map_err(|_| "signature does not match this game's key".to_owned())?;
    let actual: Vec<String> = files
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|p| p != SIGNATURE)
        .collect();
    if let Some(extra) = actual.iter().find(|p| !listed.contains_key(*p)) {
        return Err(format!("{extra} is not signed"));
    }
    for (path, hash) in &listed {
        let found = files
            .sha256(path)
            .map_err(|_| format!("{path} is missing"))?;
        if &found != hash {
            return Err(format!("{path} was modified after signing"));
        }
    }
    Ok(())
}

/// The signed text: the domain line, then `sha256sum`-style lines in path order.
pub fn signed_message(files: &BTreeMap<String, String>) -> String {
    let mut message = String::from(SIGNATURE_DOMAIN);
    for (path, hash) in files {
        message.push_str(&format!("{hash}  {path}\n"));
    }
    message
}

#[derive(Clone, Default, PartialEq, Debug)]
struct State {
    disabled: BTreeSet<String>,
    order: Vec<String>,
}

impl State {
    fn read(path: &Path) -> State {
        let Ok(text) = fs::read_to_string(path) else {
            return State::default();
        };
        let json: Value = serde_json::from_str(&text).unwrap_or_default();
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        State {
            disabled: strings(&json["disabled"]).into_iter().collect(),
            order: strings(&json["order"]),
        }
    }

    fn to_json(&self) -> Value {
        json!({ "format": 1, "disabled": self.disabled, "order": self.order })
    }
}

struct Inner {
    supported: bool,
    /// In load order (the saved order, then the rest: dev first, DLC before mods, by id).
    addons: Vec<Addon>,
    /// Indexes of `addons` that load in this run, in load order.
    active: Vec<usize>,
    boot: State,
    state: Mutex<State>,
    state_dir: Option<PathBuf>,
    summary: String,
}

/// Mods and DLC of one run; cheap to clone.
#[derive(Clone)]
pub struct Mods(Arc<Inner>);

impl Mods {
    /// Finds and checks every add-on; never fails (problems are reported per add-on and in
    /// [`Mods::summary`]).
    pub fn discover(config: ModsConfig) -> Mods {
        let mut notes = Vec::new();
        let game = match Game::read(&config.game_dir) {
            Ok(game) => game,
            Err(e) => {
                notes.push(e);
                None
            },
        };
        let state_dir = config.profile_root.clone();
        let boot = state_dir
            .as_deref()
            .map(|dir| State::read(&dir.join(STATE_FILE)))
            .unwrap_or_default();
        let Some(game) = game else {
            return Mods::empty(
                false,
                boot,
                state_dir,
                format!("mods off (no {GAME_FILE}) {}", notes.join("; ")),
            );
        };

        let mut places: Vec<(PathBuf, Location)> = config
            .dev
            .iter()
            .map(|dir| (dir.clone(), Location::Dev))
            .collect();
        let mut roots = Vec::new();
        if let Some(parent) = config.game_dir.parent() {
            roots.push((parent.join("dlc"), Location::Dlc));
            if game.mods {
                roots.push((parent.join("mods"), Location::Game));
            }
        }
        if let Some(profile) = &config.profile_root
            && game.mods
        {
            roots.push((profile.join("mods"), Location::Profile));
        }
        for (root, location) in roots {
            let Ok(entries) = fs::read_dir(&root) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            found.sort();
            places.extend(found.into_iter().map(|p| (p, location)));
        }

        let mut addons: Vec<Addon> = Vec::new();
        for (path, location) in places {
            let files = if path.is_dir() {
                if !path.join(PACKAGE).is_file() {
                    continue;
                }
                Files::Dir(path.clone())
            } else if path.extension().is_some_and(|e| e == ARCHIVE_EXT) {
                match Content::open_archive(&path) {
                    Ok(content) => Files::Packed(content),
                    Err(e) => {
                        notes.push(format!("skipped {}: {e}", path.display()));
                        continue;
                    },
                }
            } else {
                continue;
            };
            let text = match files.read(PACKAGE) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    notes.push(format!("skipped {}: {PACKAGE}: {e}", path.display()));
                    continue;
                },
            };
            let mut addon = match Addon::parse(&text, files, location) {
                Ok(addon) => addon,
                Err(e) => {
                    notes.push(format!("skipped {}: {PACKAGE}: {e}", path.display()));
                    continue;
                },
            };
            if addons.iter().any(|a| a.id == addon.id) {
                notes.push(format!(
                    "skipped {}: duplicate id {}",
                    path.display(),
                    addon.id
                ));
                continue;
            }
            check_static(&mut addon, &game, config.steam.as_deref());
            addons.push(addon);
        }

        // Load order: the saved order first, then dev add-ons, DLC, mods, each by id.
        let rank = |a: &Addon| {
            let saved = boot.order.iter().position(|id| *id == a.id);
            let group = match (a.location, a.kind) {
                (Location::Dev, _) => 0,
                (_, Kind::Dlc) => 1,
                _ => 2,
            };
            (saved.is_none(), saved.unwrap_or(0), group, a.id.clone())
        };
        addons.sort_by_key(rank);
        check_requirements(&mut addons, &boot.disabled);
        let active: Vec<usize> = (0..addons.len())
            .filter(|&i| addons[i].problems.is_empty() && !boot.disabled.contains(&addons[i].id))
            .collect();
        let summary = format!(
            "mods game={}@{} mods_allowed={} found={} active={} [{}]{}",
            game.name,
            game.version,
            game.mods,
            addons.len(),
            active.len(),
            active
                .iter()
                .map(|&i| format!("{}@{}", addons[i].id, addons[i].version))
                .collect::<Vec<_>>()
                .join(" "),
            if notes.is_empty() {
                String::new()
            } else {
                format!(" notes: {}", notes.join("; "))
            }
        );
        Mods(Arc::new(Inner {
            supported: game.mods,
            addons,
            active,
            state: Mutex::new(boot.clone()),
            boot,
            state_dir,
            summary,
        }))
    }

    fn empty(supported: bool, boot: State, state_dir: Option<PathBuf>, summary: String) -> Mods {
        Mods(Arc::new(Inner {
            supported,
            addons: Vec::new(),
            active: Vec::new(),
            state: Mutex::new(boot.clone()),
            boot,
            state_dir,
            summary,
        }))
    }

    /// One line for the player's log.
    pub fn summary(&self) -> &str {
        &self.0.summary
    }

    /// The files of every add-on, indexed as [`Mods::overlay`] and [`Mods::addon_file`] answer.
    pub fn sources(&self) -> Vec<Files> {
        self.0.addons.iter().map(|a| a.files.clone()).collect()
    }

    /// Game path -> (add-on index, path inside it) when an active add-on replaces it.
    pub fn overlay(&self, path: &str) -> Option<(usize, String)> {
        let inner_path = format!("{OVERRIDES}{path}");
        self.0
            .active
            .iter()
            .rev()
            .find(|&&i| self.0.addons[i].files.has(&inner_path))
            .map(|&i| (i, inner_path))
    }

    /// `/__mods/<id>/<path>` (decoded, after the prefix) -> (add-on index, path inside it).
    pub fn addon_file(&self, rest: &str) -> Option<(usize, String)> {
        let split = if rest.starts_with('@') {
            rest.match_indices('/').nth(1).map(|(i, _)| i)
        } else {
            rest.find('/')
        }?;
        let (id, path) = (&rest[..split], &rest[split + 1..]);
        if !valid_inner_path(path) {
            return None;
        }
        let index = self.0.addons.iter().position(|a| a.id == id)?;
        let addon = &self.0.addons[index];
        let allowed = self.0.active.contains(&index) || addon.cover.as_deref() == Some(path);
        (allowed && addon.files.has(path)).then(|| (index, path.to_owned()))
    }

    fn list(&self) -> Value {
        let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        Value::Array(
            self.0
                .addons
                .iter()
                .enumerate()
                .map(|(i, a)| a.info(!state.disabled.contains(&a.id), self.0.active.contains(&i)))
                .collect(),
        )
    }

    fn update(&self, change: impl FnOnce(&mut State) -> Result<(), &'static str>) -> Reply {
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut next = state.clone();
        if let Err(reason) = change(&mut next) {
            return Reply::Error(StatusCode::BAD_REQUEST, reason);
        }
        if let Some(dir) = &self.0.state_dir {
            let data = next.to_json().to_string();
            if fs::create_dir_all(dir)
                .and_then(|()| write_atomic(dir, STATE_FILE, data.as_bytes()))
                .is_err()
            {
                return Reply::Error(StatusCode::INTERNAL_SERVER_ERROR, "state not saved");
            }
        }
        *state = next;
        Reply::Json(Value::Bool(true))
    }

    fn known(&self, id: &str) -> bool {
        self.0.addons.iter().any(|a| a.id == id)
    }
}

/// Checks that need only the add-on and the game: kind, signature, Steam, game, libraries.
fn check_static(addon: &mut Addon, game: &Game, steam: Option<&SteamHandle>) {
    let dev = addon.location == Location::Dev;
    match addon.kind {
        Kind::Mod if !game.mods => addon
            .problems
            .push("this game does not allow mods".to_owned()),
        Kind::Mod if addon.location == Location::Dlc => addon
            .problems
            .push("a mod in the dlc folder (set \"roves.kind\": \"dlc\")".to_owned()),
        Kind::Dlc if !dev => match &game.key {
            None => addon
                .problems
                .push("this game accepts no DLC (no DLC key)".to_owned()),
            Some(key) => {
                if let Err(e) = verify_signature(&addon.files, key) {
                    addon.problems.push(format!("DLC {e}"));
                }
            },
        },
        _ => {},
    }
    if let Some(app) = addon
        .steam_app_id
        .filter(|_| addon.kind == Kind::Dlc && !dev)
    {
        let owned = steam.and_then(|s| s.with(|api| api.is_dlc_installed(app)));
        if owned != Some(true) {
            addon
                .problems
                .push(format!("Steam DLC {app} is not installed"));
        }
    }
    match &addon.game {
        None => addon
            .problems
            .push("no \"roves.game\" in package.json".to_owned()),
        Some((name, _)) if *name != game.name => addon
            .problems
            .push(format!("made for the game {name}, not {}", game.name)),
        Some((_, range)) => match Range::parse(range) {
            Ok(range) if game.version.satisfies(&range) => {},
            Ok(_) => addon.problems.push(format!(
                "needs game version {range}, this is {}",
                game.version
            )),
            Err(_) => addon
                .problems
                .push(format!("bad game version range {range:?}")),
        },
    }
    for (lib, range) in &addon.peer {
        let ok = match (game.shared.get(lib), Range::parse(range)) {
            (Some(version), Ok(range)) => version.satisfies(&range),
            _ => false,
        };
        if !ok {
            let has = game
                .shared
                .get(lib)
                .map_or("not shared by the game".to_owned(), |v| {
                    format!("the game shares {v}")
                });
            addon
                .problems
                .push(format!("needs library {lib} {range}, {has}"));
        }
    }
}

/// `roves.mods`: every required add-on present, enabled, loadable and in range; repeated
/// until stable, since a failure can cascade.
fn check_requirements(addons: &mut [Addon], disabled: &BTreeSet<String>) {
    loop {
        let loadable: HashMap<String, Option<Version>> = addons
            .iter()
            .filter(|a| a.problems.is_empty() && !disabled.contains(&a.id))
            .map(|a| (a.id.clone(), Version::parse(&a.version).ok()))
            .collect();
        let mut changed = false;
        for addon in addons.iter_mut().filter(|a| a.problems.is_empty()) {
            for (id, range) in &addon.requires {
                let ok = match (loadable.get(id), Range::parse(range)) {
                    (Some(Some(version)), Ok(range)) => version.satisfies(&range),
                    _ => false,
                };
                if !ok {
                    addon
                        .problems
                        .push(format!("needs {id} {range} enabled and compatible"));
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// The host API's `mods` module (roves-api `mods`).
impl Module for Mods {
    fn call(&self, cmd: &str, call: &Call<'_>) -> Reply {
        let post_only = || Reply::Error(StatusCode::METHOD_NOT_ALLOWED, "POST only");
        match cmd {
            "is_supported" => Reply::Json(Value::Bool(self.0.supported)),
            "list" => Reply::Json(self.list()),
            "restart_required" => {
                let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
                Reply::Json(Value::Bool(*state != self.0.boot))
            },
            "set_enabled" if call.post() => {
                let id = call.arg("id").unwrap_or_default().to_owned();
                let enabled = match call.arg("enabled") {
                    Some("true") => true,
                    Some("false") => false,
                    _ => return Reply::Error(StatusCode::BAD_REQUEST, "enabled=true|false"),
                };
                if !self.known(&id) {
                    return Reply::Error(StatusCode::NOT_FOUND, "unknown id");
                }
                self.update(|state| {
                    if enabled {
                        state.disabled.remove(&id);
                    } else {
                        state.disabled.insert(id);
                    }
                    Ok(())
                })
            },
            "set_order" if call.post() => {
                let Ok(order) = serde_json::from_slice::<Vec<String>>(call.body) else {
                    return Reply::Error(StatusCode::BAD_REQUEST, "body: JSON array of ids");
                };
                self.update(|state| {
                    let unique: BTreeSet<&String> = order.iter().collect();
                    if unique.len() != order.len() {
                        return Err("duplicate id");
                    }
                    state.order = order;
                    Ok(())
                })
            },
            "set_enabled" | "set_order" => post_only(),
            _ => Reply::Error(StatusCode::NOT_FOUND, "unknown command"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::{io::Write, net::TcpStream};

    const KEY_SEED: [u8; 32] = [7; 32];

    fn write(root: &Path, path: &str, data: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, data).unwrap();
    }

    /// A `package.json` for `mygame ^1.4.0`; `extra` is appended inside `roves`, `top` at the
    /// top level.
    fn package(name: &str, top: &str, extra: &str) -> String {
        format!(
            r#"{{"name":"{name}","version":"1.0.0","displayName":"Mod {name}",{top}
               "roves":{{"game":{{"name":"mygame","version":"^1.4.0"}}{extra}}}}}"#
        )
    }

    /// `<tmp>/install/game` with roves-game.json, plus `<tmp>/profile`.
    fn install(tmp: &Path, shared: &str, mods_allowed: bool) -> ModsConfig {
        let key = SigningKey::from_bytes(&KEY_SEED).verifying_key();
        let key = base64::engine::general_purpose::STANDARD.encode(key.as_bytes());
        write(
            tmp,
            "install/game/roves-game.json",
            &format!(
                r#"{{"format":1,"name":"mygame","version":"1.5.2","mods":{mods_allowed},
                   "dlcPublicKey":"{key}","shared":{{{shared}}}}}"#
            ),
        );
        write(tmp, "install/game/index.html", "game");
        write(tmp, "install/game/a.txt", "game a");
        ModsConfig {
            game_dir: tmp.join("install/game"),
            profile_root: Some(tmp.join("profile")),
            dev: Vec::new(),
            steam: None,
        }
    }

    /// What `roves mod pack` does for a DLC: hash every file, sign with the game's key.
    fn sign(root: &Path) {
        let files = Files::Dir(root.to_owned());
        let mut hashes = BTreeMap::new();
        for path in files.list().unwrap() {
            hashes.insert(path.clone(), files.sha256(&path).unwrap());
        }
        let signature = SigningKey::from_bytes(&KEY_SEED).sign(signed_message(&hashes).as_bytes());
        let json = json!({
            "format": 1,
            "files": hashes,
            "signature": base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        });
        fs::write(root.join(SIGNATURE), json.to_string()).unwrap();
    }

    fn info(mods: &Mods, id: &str) -> Value {
        mods.list()
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} not listed: {}", mods.summary()))
    }

    fn post(mods: &Mods, cmd: &str, query: &[(&str, &str)], body: &[u8]) -> Reply {
        let query: Vec<(String, String)> = query
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        mods.call(
            cmd,
            &Call {
                method: &axum::http::Method::POST,
                query: &query,
                body,
            },
        )
    }

    #[test]
    fn game_file_and_ids() {
        assert!(
            Game::parse(r#"{"format":1,"name":"g","version":"1.0.0"}"#)
                .unwrap()
                .mods
        );
        assert!(Game::parse(r#"{"format":2,"name":"g","version":"1.0.0"}"#).is_err());
        assert!(Game::parse(r#"{"format":1,"name":"g","version":"one"}"#).is_err());
        for good in ["mod", "@author/new-ending", "a.b_c~d"] {
            assert!(valid_id(good), "{good}");
        }
        for bad in ["", "Mod", "@author", "@a/b/c", "../x", ".hidden", "a b"] {
            assert!(!valid_id(bad), "{bad}");
        }
        let tmp = tempfile::tempdir().unwrap();
        let mods = Mods::discover(ModsConfig {
            game_dir: tmp.path().to_owned(),
            profile_root: None,
            dev: Vec::new(),
            steam: None,
        });
        assert!(
            !mods.0.supported && mods.list() == json!([]),
            "{}",
            mods.summary()
        );
    }

    #[test]
    fn compatibility_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let config = install(tmp.path(), r#""@drincs/pixi-vn":"1.3.4""#, true);
        let dir = tmp.path().join("install/mods");
        write(&dir, "ok/package.json", &package("ok", "", ""));
        write(&dir, "ok/cover.webp", "img");
        write(
            &dir,
            "old/package.json",
            &package("old", "", "").replace("^1.4.0", "^2.0.0"),
        );
        write(
            &dir,
            "other/package.json",
            &package("other", "", "").replace("mygame", "theirs"),
        );
        write(
            &dir,
            "peer/package.json",
            &package(
                "peer",
                r#""peerDependencies":{"@drincs/pixi-vn":"^1.3.0","pixi.js":"^8"},"#,
                "",
            ),
        );
        write(
            &dir,
            "needs/package.json",
            &package("needs", "", r#","mods":{"old":"*"}"#),
        );
        write(
            &dir,
            "chain/package.json",
            &package("chain", "", r#","mods":{"needs":"*"}"#),
        );
        write(
            &dir,
            "uses-ok/package.json",
            &package("uses-ok", "", r#","mods":{"ok":"~1.0.0"}"#),
        );
        write(&dir, "broken/package.json", "{");
        write(
            &dir,
            "fake-dlc/package.json",
            &package("fake-dlc", "", r#","kind":"dlc""#),
        );
        let mods = Mods::discover(config);
        let summary = mods.summary().to_owned();
        assert!(
            summary.contains("skipped") && summary.contains("broken"),
            "{summary}"
        );
        assert_eq!(info(&mods, "ok")["active"], true);
        assert_eq!(info(&mods, "ok")["cover_url"], "/__mods/ok/cover.webp");
        assert_eq!(info(&mods, "uses-ok")["active"], true);
        for (id, problem) in [
            ("old", "needs game version ^2.0.0"),
            ("other", "made for the game theirs"),
            ("peer", "pixi.js ^8, not shared"),
            ("needs", "needs old"),
            ("chain", "needs needs"),
            ("fake-dlc", "DLC not signed"),
        ] {
            let m = info(&mods, id);
            assert_eq!(m["compatible"], false, "{id}");
            assert!(m["problems"].to_string().contains(problem), "{id}: {m}");
        }
    }

    #[test]
    fn dlc_must_be_signed_by_the_game() {
        let tmp = tempfile::tempdir().unwrap();
        let config = install(tmp.path(), "", false);
        let dlc = tmp.path().join("install/dlc");
        let kind = r#","kind":"dlc""#;
        write(&dlc, "signed/package.json", &package("signed", "", kind));
        write(&dlc, "signed/overrides/a.txt", "dlc a");
        sign(&dlc.join("signed"));
        write(
            &dlc,
            "tampered/package.json",
            &package("tampered", "", kind),
        );
        write(&dlc, "tampered/x.txt", "original");
        sign(&dlc.join("tampered"));
        write(&dlc, "tampered/x.txt", "changed");
        write(&dlc, "added/package.json", &package("added", "", kind));
        sign(&dlc.join("added"));
        write(&dlc, "added/extra.js", "evil");
        write(
            &dlc,
            "steam/package.json",
            &package("steam", "", r#","kind":"dlc","steamAppId":480"#),
        );
        sign(&dlc.join("steam"));
        // Mods are off for this game: a mod is never scanned from mods/ ...
        write(
            tmp.path(),
            "install/mods/m/package.json",
            &package("m", "", ""),
        );
        // ... and a dev mod is listed but refused.
        write(tmp.path(), "dev/package.json", &package("dev-mod", "", ""));
        let mods = Mods::discover(ModsConfig {
            dev: vec![tmp.path().join("dev")],
            ..config
        });
        assert_eq!(info(&mods, "signed")["active"], true, "{}", mods.summary());
        assert_eq!(
            mods.overlay("a.txt").map(|(_, p)| p).as_deref(),
            Some("overrides/a.txt")
        );
        for (id, problem) in [
            ("tampered", "x.txt was modified"),
            ("added", "extra.js is not signed"),
            ("steam", "Steam DLC 480 is not installed"),
            ("dev-mod", "does not allow mods"),
        ] {
            let m = info(&mods, id);
            assert!(m["problems"].to_string().contains(problem), "{id}: {m}");
        }
        assert!(!mods.list().to_string().contains("\"Mod m\""));
        assert_eq!(
            post(&mods, "is_supported", &[], b""),
            Reply::Json(Value::Bool(false))
        );
    }

    #[test]
    fn state_order_and_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let config = || install(tmp.path(), "", true);
        let dir = tmp.path().join("install/mods");
        write(&dir, "first/package.json", &package("first", "", ""));
        write(&dir, "first/overrides/a.txt", "first a");
        // A packed add-on: the same layout in a .rovesmod ZIP.
        let archive = File::create(dir.join("second.rovesmod")).unwrap();
        let mut zip = zip::ZipWriter::new(archive);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (path, data) in [
            (
                "package.json",
                package("@me/second", r#""main":"main.js","#, ""),
            ),
            ("overrides/a.txt", "second a".to_owned()),
            ("main.js", "export default {}".to_owned()),
        ] {
            zip.start_file(path, stored).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        fs::write(dir.join("notes.txt"), "not an add-on").unwrap();
        let mods = Mods::discover(config());
        let second = info(&mods, "@me/second");
        assert_eq!(second["active"], true, "{}", mods.summary());
        assert_eq!(second["main_url"], "/__mods/@me/second/main.js");
        // Later in the load order wins: default order is by id ("@me/second" < "first").
        let (winner, _) = mods.overlay("a.txt").unwrap();
        assert_eq!(mods.0.addons[winner].id, "first");
        assert!(mods.addon_file("@me/second/main.js").is_some());
        assert!(mods.addon_file("@me/second/../x").is_none());
        assert!(mods.overlay("index.html").is_none());

        // Disable `first` and move it first: saved, applied at the next launch.
        assert_eq!(
            post(
                &mods,
                "set_enabled",
                &[("id", "first"), ("enabled", "false")],
                b""
            ),
            Reply::Json(Value::Bool(true))
        );
        assert_eq!(
            post(&mods, "set_order", &[], br#"["first","@me/second"]"#),
            Reply::Json(Value::Bool(true))
        );
        assert!(matches!(
            post(
                &mods,
                "set_enabled",
                &[("id", "nope"), ("enabled", "true")],
                b""
            ),
            Reply::Error(StatusCode::NOT_FOUND, _)
        ));
        assert!(matches!(
            post(&mods, "set_order", &[], br#"["a","a"]"#),
            Reply::Error(StatusCode::BAD_REQUEST, _)
        ));
        assert_eq!(
            post(&mods, "restart_required", &[], b""),
            Reply::Json(Value::Bool(true))
        );
        assert_eq!(info(&mods, "first")["active"], true);
        assert_eq!(info(&mods, "first")["enabled"], false);

        let next = Mods::discover(config());
        assert_eq!(info(&next, "first")["active"], false);
        assert_eq!(next.0.addons[0].id, "first");
        let (winner, _) = next.overlay("a.txt").unwrap();
        assert_eq!(next.0.addons[winner].id, "@me/second");
        assert!(next.addon_file("first/overrides/a.txt").is_none());
        assert_eq!(
            post(&next, "restart_required", &[], b""),
            Reply::Json(Value::Bool(false))
        );
    }

    fn get(origin: &str, path: &str) -> (u16, String) {
        let addr = origin.trim_start_matches("http://");
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let status = raw.split(' ').nth(1).unwrap().parse().unwrap();
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
        (status, body)
    }

    #[test]
    fn served_over_http() {
        let tmp = tempfile::tempdir().unwrap();
        let config = install(tmp.path(), "", true);
        let dir = tmp.path().join("install/mods");
        write(
            &dir,
            "on/package.json",
            &package("on", r#""main":"./dist/index.js","#, ""),
        );
        write(&dir, "on/dist/index.js", "export default 1");
        write(&dir, "on/overrides/a.txt", "mod a");
        write(&dir, "on/overrides/new dir/b.txt", "mod b");
        write(
            &dir,
            "off/package.json",
            &package("off", "", "").replace("^1.4.0", "^9"),
        );
        write(&dir, "off/cover.png", "cover");
        write(&dir, "off/overrides/a.txt", "off a");
        write(&dir, "off/secret.js", "x");
        let game_dir = config.game_dir.clone();
        let mods = Mods::discover(config);
        assert_eq!(info(&mods, "on")["main_url"], "/__mods/on/dist/index.js");
        let server =
            super::super::start_with_mods(&game_dir, 0, Some(mods), |_| axum::Router::new())
                .unwrap();
        let origin = server.origin.clone();
        assert_eq!(get(&origin, "/a.txt"), (200, "mod a".into()));
        assert_eq!(get(&origin, "/new%20dir/b.txt"), (200, "mod b".into()));
        assert_eq!(get(&origin, "/"), (200, "game".into()));
        assert_eq!(
            get(&origin, "/__mods/on/dist/index.js"),
            (200, "export default 1".into())
        );
        assert_eq!(get(&origin, "/__mods/off/cover.png"), (200, "cover".into()));
        assert_eq!(get(&origin, "/__mods/off/secret.js").0, 404);
        assert_eq!(get(&origin, "/__mods/on/../../game/a.txt").0, 404);
        assert_eq!(get(&origin, "/__mods/nope/x").0, 404);
        server.stop().unwrap();
    }
}
