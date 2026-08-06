/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Exposes a small subset of the Steamworks SDK to web content through a
//! `steam:` custom protocol, fetchable from ordinary page JS
//! (`fetch('steam:unlock_achievement?id=ACH_X')`). This mirrors the Tauri
//! build's `steam` feature (see the parent project's `src-tauri/src/steam.rs`)
//! command-for-command, so the JS side (`src/lib/steam.ts`) exposes the exact
//! same API regardless of which native shell is running the game.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use headers::{ContentType, HeaderMapExt};
use servo::protocol_handler::{
    DoneChannel, FetchContext, NetworkError, ProtocolHandler, Request, ResourceFetchTiming,
    Response, ResponseBody,
};
use steamworks::{AppId, Client, OverlayToStoreFlag};

pub struct SteamProtocolHandler {
    client: Option<Client>,
}

impl SteamProtocolHandler {
    /// Try to initialise Steam once, at registration time. `client` stays
    /// `None` (never a hard error) when Steam isn't running or the app was
    /// launched outside Steam — e.g. during CI builds, or in local dev.
    /// Every command below degrades to a harmless default in that case,
    /// exactly like the Tauri-side `steam::try_init()` it mirrors.
    pub fn new() -> Self {
        let client = match Client::init() {
            Ok(client) => {
                // `Client` is Clone + Send + Sync: safe to move into a thread.
                let ticker = client.clone();
                std::thread::spawn(move || loop {
                    ticker.run_callbacks();
                    std::thread::sleep(Duration::from_millis(100));
                });
                eprintln!("[Steam] Initialised — App ID {}", client.utils().app_id().0);
                Some(client)
            },
            Err(error) => {
                eprintln!("[Steam] Not available: {error}");
                None
            },
        };
        Self { client }
    }
}

impl ProtocolHandler for SteamProtocolHandler {
    fn is_fetchable(&self) -> bool {
        true
    }

    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let query: HashMap<String, String> = url.as_url().query_pairs().into_owned().collect();
        let command = url.path();

        let body = match &self.client {
            Some(client) => handle_command(client, command, &query),
            // No Steam client: every read answers with its "unavailable"
            // default, every write is a silent no-op — same contract the
            // Tauri commands provide via their own `Option<Client>` guard.
            None => handle_unavailable(command),
        };

        Box::pin(std::future::ready(match body {
            Some(body) => json_response(request, body),
            None => Response::network_error(NetworkError::ResourceLoadError(
                format!("Unknown steam: command '{command}'"),
            )),
        }))
    }
}

fn handle_unavailable(command: &str) -> Option<String> {
    use serde_json::Value;
    let value = match command {
        "is_available" | "is_achievement_unlocked" | "is_dlc_installed" | "open_overlay"
        | "open_store" => Value::Bool(false),
        "get_stat_int" => Value::from(0i32),
        "get_stat_float" => Value::from(0f32),
        "get_player_name" | "get_app_id" => Value::Null,
        "unlock_achievement" | "clear_achievement" | "set_stat_int" | "set_stat_float"
        | "store_stats" => Value::Null,
        _ => return None,
    };
    Some(value.to_string())
}

fn handle_command(client: &Client, command: &str, query: &HashMap<String, String>) -> Option<String> {
    use serde_json::Value;

    let stats = client.user_stats();
    let string_arg = |key: &str| query.get(key).cloned();
    let u32_arg = |key: &str| query.get(key).and_then(|v| v.parse::<u32>().ok());
    let i32_arg = |key: &str| query.get(key).and_then(|v| v.parse::<i32>().ok());
    let f32_arg = |key: &str| query.get(key).and_then(|v| v.parse::<f32>().ok());

    let value = match command {
        "is_available" => Value::Bool(true),
        "get_player_name" => Value::String(client.friends().name()),
        "get_app_id" => Value::from(client.utils().app_id().0),

        // Param is `achievement_id`, not `id`: matches the snake_case'd
        // query param names `callRoves()` derives from the Tauri commands'
        // own camelCase arg names (`achievementId`) in src/lib/steam.ts.
        "unlock_achievement" => {
            let id = string_arg("achievement_id")?;
            let ok = stats.achievement(&id).set().is_ok() && stats.store_stats().is_ok();
            Value::Bool(ok)
        },
        "is_achievement_unlocked" => {
            let id = string_arg("achievement_id")?;
            Value::Bool(stats.achievement(&id).get().unwrap_or(false))
        },
        "clear_achievement" => {
            let id = string_arg("achievement_id")?;
            let ok = stats.achievement(&id).clear().is_ok() && stats.store_stats().is_ok();
            Value::Bool(ok)
        },

        "set_stat_int" => {
            let (name, val) = (string_arg("name")?, i32_arg("value")?);
            Value::Bool(stats.set_stat_i32(&name, val).is_ok())
        },
        "get_stat_int" => Value::from(stats.get_stat_i32(&string_arg("name")?).unwrap_or(0)),
        "set_stat_float" => {
            let (name, val) = (string_arg("name")?, f32_arg("value")?);
            Value::Bool(stats.set_stat_f32(&name, val).is_ok())
        },
        "get_stat_float" => Value::from(stats.get_stat_f32(&string_arg("name")?).unwrap_or(0.0)),
        "store_stats" => Value::Bool(stats.store_stats().is_ok()),

        "is_dlc_installed" => {
            Value::Bool(client.apps().is_dlc_installed(AppId(u32_arg("app_id")?)))
        },

        "open_overlay" => {
            client.friends().activate_game_overlay(&string_arg("dialog")?);
            Value::Bool(true)
        },
        "open_store" => {
            let target = u32_arg("app_id").map(AppId).unwrap_or_else(|| client.utils().app_id());
            client
                .friends()
                .activate_game_overlay_to_store(target, OverlayToStoreFlag::None);
            Value::Bool(true)
        },

        _ => return None,
    };
    Some(value.to_string())
}

fn json_response(request: &Request, body: String) -> Response {
    let mut response = Response::new(
        request.current_url(),
        ResourceFetchTiming::new(request.timing_type()),
    );
    response.headers.typed_insert(ContentType::json());
    *response.body.lock() = ResponseBody::Done(body.into_bytes());
    response
}
