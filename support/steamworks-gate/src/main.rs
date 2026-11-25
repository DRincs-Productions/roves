/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Standalone Steamworks feasibility gate (see README.md).
//!
//! Exercises the three steps the CEF player would need from `steamworks` 0.13.1 —
//! initialization, callback dispatch, shutdown — and nothing else: no store upload, no
//! account credentials, no persona name or Steam ID in the output, no stat changes.
//!
//! The binary cannot tell a real Steam client from an emulator by itself; the caller states
//! what the environment is with `--expect`, and the gate fails when the observed outcome
//! does not match it:
//!
//! * `unavailable` — no Steam client: `SteamAPI_InitFlat` must fail gracefully (the
//!   `libsteam_api` redistributable loads, init returns an error, the process exits cleanly).
//! * `emulation`   — a local Steamworks emulator replaced `libsteam_api` (CI, Linux only).
//! * `real`        — a running, logged-in Steam client (manual opt-in smoke only).
//! * `any`         — report whatever happens; fails only on a broken initialized round trip.
//!
//! Output lines start with `gate:` so CI can grep them.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use steamworks::{AppId, Client, SteamAPIInitError, UserStatsReceived, UserStatsStored};

/// Valve's public "Spacewar" test application: usable by any Steam account and by the
/// emulator, never a DRincs-Productions title.
const DEFAULT_APP_ID: u32 = 480;
const DEFAULT_TIMEOUT_SECS: u64 = 20;
/// Extra time given to the optional `UserStatsReceived` callback after the gated one.
const RECEIVED_GRACE: Duration = Duration::from_secs(2);
const STORE_RETRY: Duration = Duration::from_millis(250);
const FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Any,
    Unavailable,
    Emulation,
    Real,
}

impl Expect {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "any" => Self::Any,
            "unavailable" => Self::Unavailable,
            "emulation" => Self::Emulation,
            "real" => Self::Real,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Unavailable => "unavailable",
            Self::Emulation => "emulation",
            Self::Real => "real",
        }
    }
}

struct Options {
    expect: Expect,
    app_id: u32,
    timeout: Duration,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        expect: Expect::Any,
        app_id: DEFAULT_APP_ID,
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--expect" => {
                let value = value()?;
                options.expect =
                    Expect::parse(&value).ok_or_else(|| format!("unknown --expect {value}"))?;
            },
            "--app-id" => {
                options.app_id = value()?.parse().map_err(|e| format!("--app-id: {e}"))?;
            },
            "--timeout-secs" => {
                let secs: u64 = value()?
                    .parse()
                    .map_err(|e| format!("--timeout-secs: {e}"))?;
                options.timeout = Duration::from_secs(secs);
            },
            _ => {
                return Err(format!(
                    "unknown argument {arg}; usage: roves-steamworks-gate \
                     [--expect any|unavailable|emulation|real] [--app-id N] [--timeout-secs N]"
                ));
            },
        }
    }
    Ok(options)
}

fn fail(message: &str) -> ExitCode {
    println!("gate: FAIL {message}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => return fail(&message),
    };
    println!(
        "gate: start steamworks=0.13.1 steamworks-sys=0.13.0 os={} arch={} expect={} app_id={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        options.expect.name(),
        options.app_id,
    );

    // `init_app` only sets SteamAppId/SteamGameId for this process before SteamAPI_InitFlat;
    // no steam_appid.txt is written and Steam is never asked to relaunch the app.
    let client = match Client::init_app(AppId(options.app_id)) {
        Ok(client) => client,
        Err(error) => {
            let kind = match &error {
                SteamAPIInitError::NoSteamClient(_) => "NoSteamClient",
                SteamAPIInitError::VersionMismatch(_) => "VersionMismatch",
                SteamAPIInitError::FailedGeneric(_) => "FailedGeneric",
            };
            println!("gate: init outcome=unavailable kind={kind} detail={error:?}");
            return match options.expect {
                Expect::Unavailable | Expect::Any => {
                    // Nothing was initialized, so there is nothing to shut down.
                    println!("gate: PASS outcome=unavailable (graceful init failure)");
                    ExitCode::SUCCESS
                },
                _ => fail(&format!(
                    "expected {} but Steamworks init failed ({kind})",
                    options.expect.name()
                )),
            };
        },
    };

    println!("gate: init outcome=initialized");
    if options.expect == Expect::Unavailable {
        // Still shut down cleanly before reporting the mismatch.
        drop(client);
        println!("gate: shutdown complete");
        return fail("expected no Steam client but Steamworks initialized");
    }

    let result = round_trip(&client, &options);
    // Client is the last owner of the Steamworks context: dropping it calls SteamAPI_Shutdown
    // (steamworks 0.13.1 `impl Drop for Manager`). Callback handles were dropped in round_trip.
    println!("gate: shutdown begin");
    drop(client);
    println!("gate: shutdown complete");

    match result {
        Ok(()) => {
            println!(
                "gate: PASS outcome=initialized expect={}",
                options.expect.name()
            );
            ExitCode::SUCCESS
        },
        Err(message) => fail(&message),
    }
}

fn round_trip(client: &Client, options: &Options) -> Result<(), String> {
    let reported_app = client.utils().app_id().0;
    let user = client.user();
    let steam_id = user.steam_id();
    let logged_on = user.logged_on();
    // The persona name and Steam ID are personal data: report only whether they exist.
    println!(
        "gate: identity app_id={reported_app} steam_id_valid={} logged_on={logged_on} \
         language={}",
        !steam_id.is_invalid(),
        client.apps().current_game_language(),
    );
    if reported_app != options.app_id {
        return Err(format!(
            "app id {reported_app} != requested {}",
            options.app_id
        ));
    }
    if options.expect == Expect::Real && !logged_on {
        return Err("expected a logged-on real Steam client".into());
    }

    let stored = Arc::new(AtomicBool::new(false));
    let stored_ok = Arc::new(AtomicBool::new(false));
    let received = Arc::new(AtomicBool::new(false));
    let stored_handle = {
        let (stored, stored_ok) = (stored.clone(), stored_ok.clone());
        client.register_callback(move |event: UserStatsStored| {
            println!("gate: callback UserStatsStored result={:?}", event.result);
            stored_ok.store(event.result.is_ok(), Ordering::SeqCst);
            stored.store(true, Ordering::SeqCst);
        })
    };
    let received_handle = {
        let received = received.clone();
        client.register_callback(move |event: UserStatsReceived| {
            println!("gate: callback UserStatsReceived result={:?}", event.result);
            received.store(true, Ordering::SeqCst);
        })
    };

    // Informative only: whether this asynchronous request arrives as a broadcast callback
    // differs between Steam and emulators.
    client.user_stats().request_user_stats(steam_id.raw());

    // Gated callback: StoreStats with no pending stat changes writes nothing but posts
    // UserStatsStored_t. It can return false until the current user's stats are loaded, so
    // retry while dispatching callbacks.
    let dispatched = AtomicU32::new(0);
    let started = Instant::now();
    let deadline = started + options.timeout;
    let mut store_accepted_at: Option<Instant> = None;
    let mut next_store = started;
    let mut stored_at: Option<Instant> = None;
    loop {
        let now = Instant::now();
        if store_accepted_at.is_none() && now >= next_store {
            if client.user_stats().store_stats().is_ok() {
                store_accepted_at = Some(now);
                println!(
                    "gate: StoreStats accepted after {} ms",
                    now.duration_since(started).as_millis()
                );
            } else {
                next_store = now + STORE_RETRY;
            }
        }
        client.process_callbacks(|_| {
            dispatched.fetch_add(1, Ordering::Relaxed);
        });
        if stored_at.is_none() && stored.load(Ordering::SeqCst) {
            stored_at = Some(Instant::now());
        }
        let done = match stored_at {
            Some(at) => received.load(Ordering::SeqCst) || at.elapsed() >= RECEIVED_GRACE,
            None => false,
        };
        if done || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(FRAME);
    }

    drop(stored_handle);
    drop(received_handle);
    println!(
        "gate: callbacks dispatched={} stored={} stored_ok={} received={} elapsed_ms={}",
        dispatched.load(Ordering::Relaxed),
        stored.load(Ordering::SeqCst),
        stored_ok.load(Ordering::SeqCst),
        received.load(Ordering::SeqCst),
        started.elapsed().as_millis(),
    );

    if store_accepted_at.is_none() {
        return Err("StoreStats never accepted before the timeout".into());
    }
    if !stored.load(Ordering::SeqCst) {
        return Err("UserStatsStored callback not dispatched before the timeout".into());
    }
    if !stored_ok.load(Ordering::SeqCst) {
        return Err("UserStatsStored reported an error".into());
    }
    Ok(())
}
