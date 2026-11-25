//! `steam` feature only (README "Steam"): the player's one `steamworks::Client` (steamworks-rs
//! 0.13.1, the crate the legacy Servo `steam` feature shipped), behind the host API's
//! [`SteamApi`]. Glue only: every method is one steamworks-rs call, as in the legacy
//! `ports/servoshell/desktop/protocols/steam.rs` (git 27ca0811d6d).
//!
//! `Client` is `Send + Sync`; calls from the server's blocking threads are serialized by
//! [`SteamHandle`], callbacks are dispatched from the SDL main loop and the client is dropped
//! (`SteamAPI_Shutdown`) before CEF shuts down. `steam_api64.dll` is delay-loaded (build.rs),
//! so CEF's sandboxed child processes never load it and a package without it still starts.

use std::{path::Path, sync::Arc};

use steamworks::{AppId, Client, OverlayToStoreFlag, SteamAPIInitError};

use crate::game::host_api::{SteamApi, SteamHandle};

/// The Steamworks redistributable, next to `play.exe` in a Steam package.
const STEAM_API_DLL: &str = "steam_api64.dll";
/// Development App ID file next to the executable (Steam's own convention).
const APP_ID_FILE: &str = "steam_appid.txt";

/// Initializes Steamworks once. `Err` is the reason Steam is unavailable (never fatal; the
/// log line carries no persona name or Steam ID). App ID: the Steam launch environment
/// (`SteamAppId`) when present, otherwise a `steam_appid.txt` next to the executable, otherwise
/// whatever `SteamAPI_Init` finds itself.
pub(crate) fn init(exe_dir: &Path) -> Result<(Arc<SteamHandle>, u32), String> {
    // The delay-load stub would fault on the first call without the DLL.
    if !exe_dir.join(STEAM_API_DLL).is_file() {
        return Err(format!("{STEAM_API_DLL} missing next to the executable"));
    }
    let from_file = std::env::var_os("SteamAppId")
        .is_none()
        .then(|| std::fs::read_to_string(exe_dir.join(APP_ID_FILE)).ok())
        .flatten()
        .and_then(|text| text.trim().parse::<u32>().ok());
    let client = match from_file {
        Some(id) => Client::init_app(AppId(id)),
        None => Client::init(),
    }
    .map_err(|error| {
        let kind = match &error {
            SteamAPIInitError::NoSteamClient(_) => "NoSteamClient",
            SteamAPIInitError::VersionMismatch(_) => "VersionMismatch",
            SteamAPIInitError::FailedGeneric(_) => "FailedGeneric",
        };
        format!("{kind} {error}")
    })?;
    let app_id = client.utils().app_id().0;
    Ok((SteamHandle::new(Box::new(Steam(client))), app_id))
}

struct Steam(Client);

impl SteamApi for Steam {
    fn player_name(&self) -> String {
        self.0.friends().name()
    }

    fn app_id(&self) -> u32 {
        self.0.utils().app_id().0
    }

    fn unlock_achievement(&self, id: &str) -> bool {
        let stats = self.0.user_stats();
        stats.achievement(id).set().is_ok() && stats.store_stats().is_ok()
    }

    fn is_achievement_unlocked(&self, id: &str) -> bool {
        self.0.user_stats().achievement(id).get().unwrap_or(false)
    }

    fn clear_achievement(&self, id: &str) -> bool {
        let stats = self.0.user_stats();
        stats.achievement(id).clear().is_ok() && stats.store_stats().is_ok()
    }

    fn set_stat_int(&self, name: &str, value: i32) -> bool {
        self.0.user_stats().set_stat_i32(name, value).is_ok()
    }

    fn get_stat_int(&self, name: &str) -> i32 {
        self.0.user_stats().get_stat_i32(name).unwrap_or(0)
    }

    fn set_stat_float(&self, name: &str, value: f32) -> bool {
        self.0.user_stats().set_stat_f32(name, value).is_ok()
    }

    fn get_stat_float(&self, name: &str) -> f32 {
        self.0.user_stats().get_stat_f32(name).unwrap_or(0.0)
    }

    fn store_stats(&self) -> bool {
        self.0.user_stats().store_stats().is_ok()
    }

    fn is_dlc_installed(&self, app_id: u32) -> bool {
        self.0.apps().is_dlc_installed(AppId(app_id))
    }

    fn open_overlay(&self, dialog: &str) {
        self.0.friends().activate_game_overlay(dialog);
    }

    fn open_store(&self, app_id: Option<u32>) {
        let target = app_id.map_or_else(|| self.0.utils().app_id(), AppId);
        self.0
            .friends()
            .activate_game_overlay_to_store(target, OverlayToStoreFlag::None);
    }

    fn run_callbacks(&self) {
        self.0.run_callbacks();
    }
}
