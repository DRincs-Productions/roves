//! Steamworks lifetime of the macOS and Linux players (the Linux gate through `#[path]`),
//! the Windows player's order: initialized once in packaged-game mode before CEF starts
//! (never in a CEF subprocess), callbacks dispatched once per drive-loop turn, and the client
//! dropped (`SteamAPI_Shutdown`) after the browser closed and before CEF shuts down. Without
//! the `steam` feature the host API's `steam` module answers "unavailable".

use std::{path::Path, sync::Arc};

use crate::host_api::SteamHandle;

/// `steam` feature: Steamworks for the host API (`crate::steam`, the Windows player's
/// `steam.rs`). Steam missing or not running is never fatal: one log line without persona
/// name or Steam ID, and the host API answers "unavailable". `lib_dir` holds the
/// redistributable and the optional `steam_appid.txt` (the executable's directory).
#[cfg(feature = "steam")]
pub(crate) fn start(lib_dir: &Path) -> Option<Arc<SteamHandle>> {
    match crate::steam::init(lib_dir) {
        Ok((handle, app_id)) => {
            println!("gate: steam initialized app_id={app_id}");
            Some(handle)
        },
        Err(reason) => {
            println!("gate: steam unavailable reason={reason}");
            None
        },
    }
}

#[cfg(not(feature = "steam"))]
pub(crate) fn start(_lib_dir: &Path) -> Option<Arc<SteamHandle>> {
    None
}

/// One turn of Steamworks callback dispatch (`SteamAPI_RunCallbacks`).
pub(crate) fn run_callbacks(steam: Option<&SteamHandle>) {
    if let Some(steam) = steam {
        steam.with(|api| api.run_callbacks());
    }
}

/// Drops the Steamworks client; called after the browser closed, before CEF shuts down.
pub(crate) fn shutdown(steam: Option<&SteamHandle>) {
    if steam.is_some_and(SteamHandle::shutdown) {
        println!("gate: steam shutdown complete");
    }
}
