//! Browser process: welding imports the accelerated CEF frame on the host DX12 device.
//! The existing wgpu blits compose and present it. CPU readback is verification-only.

use std::{
    fs::File,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use crate::external_link::{ExternalLinks, main_frame_policy};
use crate::game::host_api::SteamHandle;
use crate::game::{Console, GameOptions, Input, Server};
use crate::game_rules::{
    ClickAction, ClickInput, ClickTest, CycleAction, FORCE_CPU_PAINT_ENV, KeyAction, KeyInput,
    KeyTest, Milestone, MinimizeCycle, PLAYER_CEF_DIR, PLAYER_GAME_DIR, PLAYER_LOCK_FILE,
    PLAYER_ORIGIN_PORT_FILE, PLAYER_SAVES_DIR, PaintPath, Startup, StartupFailure, StartupGate,
    Suspension, TEST_CLICK_MOUSE_ID, TEST_CLICK_POINT, TEST_KEY_KEYBOARD_ID, TEST_MINIMIZE_HOLD,
    TEST_WHEEL_LINES, TEST_WHEEL_MOUSE_ID, TestKey, WheelAction, WheelInput, WheelMarker,
    WheelTest, is_click_marker, key_marker, load_status_ok, on_origin, origin_port, paint_path,
    parse_origin_port, parse_test_key_delay, parse_test_wheel_delay, player_profile_name,
    splash_indicator, splash_layout, suspension_change, wheel_marker,
};
use crate::mcp::Call;
use crate::splash::{
    BAR_INDICATOR, BAR_SIZE, BAR_TRACK, Branding, LOGO_PX, SPLASH_BACKGROUND, SPLASH_GAP,
    WORDMARK_SIZE, Wordmark,
};
use crate::stall::{Marks, Poll, Ring, Step};
use crate::timings::{Stage, Timings};
use base64::Engine as _;
use dpi::PhysicalSize;
use sdl3::{
    EventSubsystem, VideoSubsystem,
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod, Scancode},
    mouse::{MouseButton as SdlButton, MouseWheelDirection},
    video::{Window, WindowFlags},
};
use serde_json::{Value, json};
use welding::{
    CefRuntime, CefRuntimeConfig, CefSandboxMode, CefSurfaceConfig, CefSurfaceEvent,
    CefSurfaceProducer as _, CefWindowsSandboxContext, HostWgpuContext, ImportedTexture,
    InteropBackend, MainFrameNavigationPolicy, NavigationEvent,
    windows_cef::{WindowsCefConfig, WindowsCefProducer},
};

const FIXTURE_HTML: &str = include_str!("fixture.html");
const BLIT_WGSL: &str = include_str!("blit.wgsl");
const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize::new(1280, 720);
const RESIZED_SIZE: PhysicalSize<u32> = PhysicalSize::new(960, 540);
/// A static first screen paints before its load-end and never again, so no frame can be
/// newer than the load-end. Chromium's paint holding also ends on a timeout
/// (`RenderWidgetHostImpl` new-content rendering timeout, 4 s); after this delay the startup
/// screen gives way to a frame the game document already painted.
const STARTUP_SETTLE_AFTER_LOAD_END: Duration = Duration::from_secs(1);
/// Composed target format (`Rgba8Unorm`: a choice of the removed Netrender comparison, kept
/// unchanged; a BGRA target or no intermediate pass is a performance question to measure).
const COMPOSED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Imported frames of the expected size required before a check (the page must have painted
/// its content, not only the initial transparent frame).
const FRAMES_BEFORE_CHECK: u32 = 20;
/// Per-channel tolerance for 8-bit rounding of the 50% layer.
const TOLERANCE: u8 = 3;
const BACKGROUND: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
/// `--roves-sustained`: longest gap between two imported producer frames during warmup or
/// sample before the run is declared stalled. The fixture repaints every 16 ms.
const SUSTAINED_STALL: Duration = Duration::from_millis(500);
/// `--roves-sustained` only: welding/grafting `log` records at warn or error level forwarded
/// to the evidence log, at most this many (the rest are only counted).
const UPSTREAM_LOG_CAP: u32 = 64;
/// Warmup start, shared with the `log` forwarder (CEF's UI thread logs too).
static WARMUP_ORIGIN: OnceLock<Instant> = OnceLock::new();
static UPSTREAM_LOG_RECORDS: AtomicU32 = AtomicU32::new(0);
/// Longest SDL wait per loop turn while frame work is suspended (window minimized/hidden):
/// bounds how late CEF navigation events, the load timeout and `--roves-auto-close-ms` are
/// handled. OS events (restore, show, close) end the wait immediately.
const SUSPENDED_TICK: Duration = Duration::from_millis(16);

/// Entry of the browser process and of every CEF subprocess (via bootstrap.exe).
///
/// # Safety
///
/// `instance` and `sandbox_info` come unchanged from CEF's bootstrap.exe.
pub(crate) unsafe fn run(instance: *mut std::ffi::c_void, sandbox_info: *mut u8) -> i32 {
    let Some(cef_path) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    else {
        eprintln!("roves-cef-win cannot locate the bundled CEF distribution");
        return 1;
    };
    // A null sandbox_info (bootstrap.exe without a broker) is refused by welding: the probe
    // has no unsandboxed path.
    // SAFETY: forwarded from bootstrap.exe, which keeps both values valid for this call.
    let context = match unsafe { CefWindowsSandboxContext::from_raw(instance, sandbox_info) } {
        Ok(context) => context,
        Err(error) => {
            eprintln!("roves-cef-win sandbox-info present=false error={error}");
            return 3;
        },
    };
    match context.execute_process(&cef_path) {
        Ok(Some(code)) => return code,
        Ok(None) => {},
        Err(error) => {
            eprintln!("roves-cef-win execute-process failed: {error}");
            return 3;
        },
    }
    // Browser process only (CEF subprocesses returned above): origin of the `startup`
    // timeline. Excludes OS process creation and CEF child-process startup.
    let mut startup = Startup::new(Instant::now());

    let mut options = match Options::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("roves-cef-win usage error: {error}");
            return 1;
        },
    };
    let log = match Log::open(options.log.as_deref()) {
        Ok(log) => log,
        Err(error) => {
            eprintln!("roves-cef-win cannot open the log: {error}");
            return 1;
        },
    };
    log.line(format_args!("sandbox-info present=true mode=sandboxed"));
    log.line(format_args!(
        "versions welding=0.15.0@e991684 cef={} wgpu=29.0.4",
        cef_version()
    ));
    if options.sustained {
        UpstreamLog::install(log.clone());
    }
    if options.player {
        log.line(format_args!(
            "player launch game-dir={} profile={} cache={} clean-cache={} error-dialog={}",
            options
                .game
                .as_ref()
                .map_or_else(String::new, |g| g.dir.display().to_string()),
            if options.player_profile.is_some() {
                "persistent"
            } else {
                "isolated"
            },
            options.cache_dir.display(),
            options.clean_cache,
            options.error_dialog
        ));
    }
    // Player, persistent profile: one browser process per profile (the lock lives until this
    // function returns, after CEF shut down), then the stored origin port. A busy profile or
    // a damaged port file ends the run before the loopback server and CEF start.
    let profile = match options.player_profile.as_deref() {
        Some(root) => match PlayerProfile::open(root) {
            Ok(profile) => {
                log.line(format_args!(
                    "player profile root={} lock=acquired origin-port={}",
                    root.display(),
                    profile
                        .stored_port
                        .map_or_else(|| "new".to_owned(), |port| port.to_string())
                ));
                if let Some(game) = options.game.as_mut() {
                    game.port = profile.stored_port.unwrap_or(0);
                }
                Some(profile)
            },
            Err(ProfileError::Busy) => {
                log.line(format_args!(
                    "FAIL player profile busy root={} (another instance of this game is running)",
                    root.display()
                ));
                player_error_dialog(
                    &options,
                    &log,
                    "The game is already running. Close the other game window first: two \
                     copies cannot share the same saves.",
                );
                log.line(format_args!("exit code={EXIT_CONTENTION}"));
                return EXIT_CONTENTION;
            },
            Err(ProfileError::Failed(error)) => {
                log.line(format_args!(
                    "FAIL player profile root={}: {error}",
                    root.display()
                ));
                player_error_dialog(
                    &options,
                    &log,
                    &format!(
                        "The game could not open its save folder.\n\nDetails: {error}\n\
                         Folder: {}",
                        root.display()
                    ),
                );
                log.line(format_args!("exit code=1"));
                return 1;
            },
        },
        None => None,
    };
    // Player, isolated profile only: the run's own cache, removed on every exit path after
    // CEF shut down.
    let player_cache = options.clean_cache.then(|| options.cache_dir.clone());
    let cleanup = |log: &Log| {
        if let Some(dir) = player_cache.as_deref() {
            remove_player_cache(log, dir);
        }
    };
    startup_line(&log, &mut startup, Milestone::OptionsLogReady);

    // Game mode: bind the loopback origin before CEF starts (the window probe's order). The
    // host API (README "Host API") is served on the same origin; its `exit` reaches the main
    // loop through `game_exit`.
    let game_exit = Arc::new(OnceLock::<&'static str>::new());
    // `steam` feature, game mode: the run's one Steamworks client (README "Steam"), before
    // CEF and the wgpu device exist; dropped before CEF shuts down (`shutdown_steam`).
    let steam = start_steam(&log, &options, &cef_path);
    let server = match options.game.as_ref() {
        Some(game) => match start_game_server(&log, &options, game, &game_exit, steam.clone()) {
            Ok(server) => {
                log.line(format_args!(
                    "game loopback origin={} game-dir={} profile-dir={} browser_pid={}",
                    server.origin,
                    game.dir.display(),
                    crate::game::profile_dir(&options.cache_dir).display(),
                    std::process::id()
                ));
                // Persistent profile, first launch: keep this origin for every later launch.
                if let Some(profile) = profile.as_ref().filter(|p| p.stored_port.is_none()) {
                    let stored = origin_port(&server.origin)
                        .ok_or_else(|| format!("unexpected origin {}", server.origin))
                        .and_then(|port| profile.store_port(port).map(|()| port));
                    match stored {
                        Ok(port) => log.line(format_args!("player origin-port stored port={port}")),
                        Err(error) => {
                            log.line(format_args!("FAIL player origin-port not stored: {error}"));
                            stop_server(&log, Some(server));
                            player_error_dialog(
                                &options,
                                &log,
                                &format!(
                                    "The game could not write to its save folder.\n\n\
                                     Details: {error}\nFolder: {}",
                                    profile.root.display()
                                ),
                            );
                            log.line(format_args!("exit code=1"));
                            return 1;
                        },
                    }
                }
                startup_line(&log, &mut startup, Milestone::LoopbackReady);
                Some(server)
            },
            Err(error) => {
                // Persistent profile: the stored port is the saves' origin; another port
                // would silently show the player an empty game, so a taken port ends the run.
                let taken = matches!(
                    error.kind(),
                    std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
                );
                if profile.is_some() && taken {
                    log.line(format_args!(
                        "FAIL game loopback port busy port={} error={error}",
                        game.port
                    ));
                    player_error_dialog(
                        &options,
                        &log,
                        &format!(
                            "The game could not start: another program is using its local \
                             port {}. Close that program and start the game again.",
                            game.port
                        ),
                    );
                    log.line(format_args!("exit code={EXIT_CONTENTION}"));
                    return EXIT_CONTENTION;
                }
                log.line(format_args!("FAIL game loopback server: {error}"));
                player_error_dialog(
                    &options,
                    &log,
                    &format!("The game could not start its local file server.\n\nDetails: {error}"),
                );
                cleanup(&log);
                return 1;
            },
        },
        None => None,
    };

    let mut config = CefRuntimeConfig::new(cef_path, CefSandboxMode::Sandboxed);
    config.cache_path = Some(options.cache_dir.clone());
    let runtime = match context.initialize(config) {
        Ok(runtime) => runtime,
        Err(error) => {
            log.line(format_args!("cef initialize failed: {error}"));
            shutdown_steam(&log, steam.as_deref());
            stop_server(&log, server);
            cleanup(&log);
            return 4;
        },
    };
    log.line(format_args!(
        "cef initialized cache={}",
        options.cache_dir.display()
    ));
    startup_line(&log, &mut startup, Milestone::CefInitialized);

    let pushes_events =
        options.test_click.is_some() || options.test_key.is_some() || options.test_wheel.is_some();
    let sdl = match sdl3::init().and_then(|sdl| {
        let video = sdl.video()?;
        let events = sdl.event_pump()?;
        // `--roves-test-click-ms`/`--roves-test-key-ms`/`--roves-test-wheel-ms` only: pushes
        // the synthetic events into the SDL queue.
        let sender = pushes_events.then(|| sdl.event()).transpose()?;
        Ok((video, events, sdl.mouse(), sender))
    }) {
        Ok(parts) => parts,
        Err(error) => {
            log.line(format_args!("SDL3 startup failed: {error}"));
            shutdown_steam(&log, steam.as_deref());
            drop(runtime);
            stop_server(&log, server);
            cleanup(&log);
            return 1;
        },
    };
    let (video, mut events, mouse, event_sender) = sdl;
    startup_line(&log, &mut startup, Milestone::SdlReady);
    let game_mode = options.game.is_some();
    let phase = if game_mode {
        Phase::Game
    } else {
        Phase::Initial
    };
    let minimize_cycle = options.test_minimize.map(|_| MinimizeCycle::Waiting);
    let click_test = options.test_click.map(|_| ClickTest::Waiting);
    let key_test = options.test_key.map(|_| KeyTest::Waiting);
    let wheel_test = options.test_wheel.map(|_| WheelTest::Waiting);
    let mut probe = Probe {
        timings: (options.timings || options.sustained).then(Timings::default),
        phase_since: None,
        last_import: None,
        last_generation: None,
        game_start_generation: None,
        stall: options.sustained.then(Box::<Ring>::default),
        sustained_done: false,
        options,
        log: log.clone(),
        runtime: Some(runtime),
        state: None,
        phase,
        server,
        game_loaded_at: None,
        console: Console::default(),
        minimize_cycle,
        click_test,
        key_test,
        wheel_test,
        external_links: ExternalLinks::from_env(),
        event_sender,
        gate: game_mode.then_some(StartupGate::Loading),
        failure_reported: false,
        failure_presented: false,
        released_at: None,
        failures: 0,
        checks: 0,
        exit_code: None,
        started: Instant::now(),
        closing_since: None,
        startup,
        mcp: None,
        bootstrap: None,
        page_init: None,
        game_exit,
        steam,
    };
    probe.start_mcp();
    match probe.start(&video) {
        Ok(state) => {
            probe.state = Some(state);
            startup_line(
                &probe.log,
                &mut probe.startup,
                Milestone::WindowSurfaceProducerReady,
            );
            while !probe.is_closed() {
                // Minimized/hidden: no wgpu frame work, so nothing blocks on the swap chain.
                // Block on SDL instead; any OS event (restore, close) wakes the loop at once.
                if probe.phase != Phase::Closing
                    && probe.suspended()
                    && let Some(event) = events.wait_event_timeout(SUSPENDED_TICK)
                {
                    probe.window_event(event);
                }
                for event in events.poll_iter() {
                    probe.window_event(event);
                }
                if probe.phase == Phase::Closing {
                    std::thread::sleep(Duration::from_millis(2));
                } else if probe.suspended() {
                    probe.idle(&mouse);
                } else {
                    probe.redraw(&mouse);
                }
                probe.devtools_step();
                probe.mcp_step();
                probe.game_exit_step();
                probe.steam_step();
                probe.report_startup_failure();
            }
        },
        Err(error) => {
            probe.fail(format_args!("startup: {error}"));
            probe.exit_code = Some(5);
            // Player: no window to show a failure screen in; the reason still reaches the
            // player instead of a silent exit.
            if probe.options.player && probe.options.error_dialog {
                let shown = sdl3::messagebox::show_simple_message_box(
                    sdl3::messagebox::MessageBoxFlag::ERROR,
                    "Roves - the game could not start",
                    &format!(
                        "The game window could not be created ({error}). Update the graphics \
                         driver; a DirectX 12 graphics adapter is required."
                    ),
                    None,
                );
                probe.log.line(format_args!(
                    "startup failure dialog closed ok={}",
                    shown.is_ok()
                ));
            }
        },
    }
    let code = probe.finish();
    cleanup(&log);
    log.line(format_args!("exit code={code}"));
    code
}

/// Logs `startup milestone=<name> elapsed_ms=<n>` the first time `milestone` is reached
/// (README "Startup timeline"). Wall-clock since the main probe run's entry, not CPU time.
fn startup_line(log: &Log, startup: &mut Startup, milestone: Milestone) {
    if let Some(elapsed_ms) = startup.reach(milestone, Instant::now()) {
        log.line(format_args!(
            "startup milestone={} elapsed_ms={elapsed_ms}",
            milestone.name()
        ));
    }
}

/// Starts the game's loopback server with the host API routes (README "Host API"): saves in
/// `<profile root>\saves` with a persistent profile, unavailable otherwise; `exit` and
/// `clear_content_cache` set `game_exit`.
fn start_game_server(
    log: &Log,
    options: &Options,
    game: &GameOptions,
    game_exit: &Arc<OnceLock<&'static str>>,
    steam: Option<Arc<SteamHandle>>,
) -> std::io::Result<Server> {
    use crate::game::host_api::{HostApi, HostConfig};
    let exit = game_exit.clone();
    let (api, saves) = HostApi::new(HostConfig {
        saves_dir: options
            .player_profile
            .as_ref()
            .map(|root| root.join(PLAYER_SAVES_DIR)),
        engine_version: format!("roves-cef-player cef-{}", cef_version()),
        request_exit: Arc::new(move |cmd| {
            let _ = exit.set(cmd);
        }),
        steam,
    });
    match saves {
        Ok(dir) => log.line(format_args!("host api saves dir={}", dir.display())),
        Err(reason) => log.line(format_args!("host api saves unavailable reason={reason}")),
    }
    crate::game::server::start_with(&game.dir, game.port, |origin| api.router(origin))
}

/// `steam` feature, game mode only: Steamworks for the host API's `steam` module. Steam
/// missing or not running is never fatal: one log line (no persona name or Steam ID), and the
/// module answers "unavailable".
#[cfg(feature = "steam")]
fn start_steam(log: &Log, options: &Options, exe_dir: &Path) -> Option<Arc<SteamHandle>> {
    options.game.as_ref()?;
    match crate::steam::init(exe_dir) {
        Ok((handle, app_id)) => {
            log.line(format_args!("steam initialized app_id={app_id}"));
            Some(handle)
        },
        Err(reason) => {
            log.line(format_args!("steam unavailable reason={reason}"));
            None
        },
    }
}

/// Without the `steam` feature the host API's `steam` module answers "unavailable".
#[cfg(not(feature = "steam"))]
fn start_steam(_: &Log, _: &Options, _: &Path) -> Option<Arc<SteamHandle>> {
    None
}

/// Drops the Steamworks client (`SteamAPI_Shutdown`); called before CEF shuts down.
fn shutdown_steam(log: &Log, steam: Option<&SteamHandle>) {
    if steam.is_some_and(SteamHandle::shutdown) {
        log.line(format_args!("steam shutdown complete"));
    }
}

/// The page marker script for the game origin (`None` in fixture mode).
fn game_origin_script(server: Option<&Server>, mcp: bool) -> Option<String> {
    crate::mcp_rules::page_marker_script(&server?.origin, mcp)
}

/// Stops the game's loopback server, after CEF shut down (in-flight responses finish first).
fn stop_server(log: &Log, server: Option<Server>) {
    if let Some(server) = server {
        match server.stop() {
            Ok(()) => log.line(format_args!("game loopback stopped")),
            Err(error) => log.line(format_args!("FAIL game loopback stop: {error}")),
        }
    }
}

/// The CEF build the bindings were generated for (`154.0.34+g14c5a08+chromium-154.0.8037.98`).
fn cef_version() -> String {
    std::ffi::CStr::from_bytes_with_nul(cef::sys::CEF_VERSION)
        .map_or_else(|_| "unknown".into(), |v| v.to_string_lossy().into_owned())
}

struct Options {
    log: Option<PathBuf>,
    cache_dir: PathBuf,
    timeout: Duration,
    keep_open: bool,
    /// `--roves-timings`: aggregated host-thread stage timings per phase (off by default).
    timings: bool,
    /// `--roves-sustained`: after the initial check, warmup then one timed sample at the
    /// fixed initial size, no resize phase (off by default).
    sustained: bool,
    /// `--roves-warmup-ms` (default 10000).
    warmup: Duration,
    /// `--roves-sample-ms` (default 30000).
    sample: Duration,
    /// `--roves-game-dir`: opt-in real-game mode (README "Real-game mode"); `None` keeps the
    /// deterministic fixture smoke.
    game: Option<GameOptions>,
    /// `--roves-auto-close-ms` (game mode only): close this long after the game's load-end.
    auto_close: Option<Duration>,
    /// `--roves-test-minimize-ms` (game mode, tests only): this long after load-end, minimize
    /// the window through SDL, hold it suspended, restore it (README "Real-game mode").
    test_minimize: Option<Duration>,
    /// `--roves-test-click-ms` (game mode, tests only): this long after load-end, push one
    /// synthetic left click into the SDL queue and require it to be forwarded through welding
    /// and the page's click marker (README "Real-game mode").
    test_click: Option<Duration>,
    /// `--roves-test-key-ms` (game mode, tests only, at most 600000): this long after
    /// load-end, push one synthetic key down + up (`test_key_name`) into the SDL queue and
    /// require it to be forwarded through welding and the page's keydown then keyup markers
    /// (README "Real-game mode").
    test_key: Option<Duration>,
    /// `--roves-test-key=ArrowRight|CapsLock` (only with `--roves-test-key-ms`; default
    /// ArrowRight): the key of the synthetic press.
    test_key_name: TestKey,
    /// `--roves-test-wheel-ms` (game mode, tests only, at most 600000): this long after
    /// load-end, push one synthetic wheel notch into the SDL queue and require it to be
    /// forwarded through welding with the expected CEF delta and exactly one expected page
    /// wheel marker (README "Real-game mode").
    test_wheel: Option<Duration>,
    /// Packaged launch (README "Packaged player"): no `--roves-game-dir`, and the game build
    /// is the `game` directory next to the executable. A failed startup ends the run.
    player: bool,
    /// Player, ordinary launch (README "Saves and the player profile"): the persistent
    /// profile root holding the lock, the stored origin port and CEF's root cache
    /// (`cache_dir` = `<root>\cef`). Default `%LOCALAPPDATA%\Roves\Player\<name>`,
    /// `--roves-cache-dir` replaces the root. `None` outside the player and with
    /// `--roves-isolated-profile`.
    player_profile: Option<PathBuf>,
    /// Player with `--roves-isolated-profile` only (CI and tests): the run's own cache
    /// directory, removed after CEF shut down; nothing persists.
    clean_cache: bool,
    /// Player only: show the failure in a native dialog (`--roves-no-error-dialog` turns it
    /// off for unattended runs; the failure screen and log stay).
    error_dialog: bool,
    /// Game mode only: the optional local MCP bridge (README "Local MCP bridge"). `None`
    /// (default) opens no pipe, publishes no session and leaves the DevTools channel off.
    mcp: Option<crate::mcp::Source>,
}

/// A fresh per-run directory `%TEMP%\<root>\run-<unix ms>-<pid>` (absolute).
fn run_cache_dir(root: &str) -> PathBuf {
    std::env::temp_dir().join(root).join(format!(
        "run-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis()),
        std::process::id()
    ))
}

/// Packaged launch with `--roves-isolated-profile` (CI and tests): CEF's root cache (and the
/// game profile inside it) for this run only, removed after shutdown (`remove_player_cache`).
/// Nothing persists between launches.
fn player_cache_dir() -> PathBuf {
    run_cache_dir("roves-cef-player")
}

/// Exit code of a packaged launch that found its persistent profile in use by another
/// instance, or the profile's stored origin port taken by another program.
const EXIT_CONTENTION: i32 = 6;

/// Default persistent profile root of the packaged launch:
/// `%LOCALAPPDATA%\Roves\Player\<exe stem>-<hash of the install directory>`.
fn default_player_profile_root(exe: &Path) -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .ok_or("LOCALAPPDATA is not set: pass --roves-cache-dir=<profile root>")?;
    let install = exe
        .parent()
        .ok_or("the executable has no parent directory")?;
    let install = std::fs::canonicalize(install)
        .map_err(|e| format!("install directory {}: {e}", install.display()))?;
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    Ok(PathBuf::from(local)
        .join("Roves")
        .join("Player")
        .join(player_profile_name(&stem, &install.to_string_lossy())))
}

/// Packaged launch, persistent profile: the held profile lock and the stored origin port.
/// The lock (`LockFileEx` through `File::try_lock`) is released when the file is dropped or
/// the process ends, including a crash, so a stale lock never blocks the next launch.
struct PlayerProfile {
    root: PathBuf,
    _lock: File,
    /// `None` on the first launch: the port the OS picks is stored after the bind.
    stored_port: Option<u16>,
}

enum ProfileError {
    /// Another process holds the profile lock.
    Busy,
    Failed(String),
}

impl PlayerProfile {
    fn open(root: &Path) -> Result<Self, ProfileError> {
        let failed = |what: &str, e: std::io::Error| ProfileError::Failed(format!("{what}: {e}"));
        std::fs::create_dir_all(root).map_err(|e| failed("create the profile directory", e))?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(PLAYER_LOCK_FILE))
            .map_err(|e| failed("open the profile lock", e))?;
        match lock.try_lock() {
            Ok(()) => {},
            Err(std::fs::TryLockError::WouldBlock) => return Err(ProfileError::Busy),
            Err(std::fs::TryLockError::Error(e)) => return Err(failed("lock the profile", e)),
        }
        let port_file = root.join(PLAYER_ORIGIN_PORT_FILE);
        let stored_port = match std::fs::read_to_string(&port_file) {
            Ok(text) => Some(parse_origin_port(&text).ok_or_else(|| {
                ProfileError::Failed(format!(
                    "{} is damaged (expected a port number); the saves belong to that port",
                    port_file.display()
                ))
            })?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(failed("read the origin port", e)),
        };
        Ok(PlayerProfile {
            root: root.to_path_buf(),
            _lock: lock,
            stored_port,
        })
    }

    /// Writes the origin port through a temporary file and a rename (replaces atomically),
    /// so an interrupted first launch never leaves a partial port file.
    fn store_port(&self, port: u16) -> Result<(), String> {
        let target = self.root.join(PLAYER_ORIGIN_PORT_FILE);
        let temporary = self.root.join(format!("{PLAYER_ORIGIN_PORT_FILE}.tmp"));
        std::fs::write(&temporary, format!("{port}\n"))
            .and_then(|()| std::fs::rename(&temporary, &target))
            .map_err(|e| format!("{}: {e}", target.display()))
    }
}

/// Player only, before the window exists: the reason in a native SDL message box without a
/// parent window (`--roves-no-error-dialog` skips it; the log line stays).
fn player_error_dialog(options: &Options, log: &Log, message: &str) {
    if !options.player {
        return;
    }
    if !options.error_dialog {
        log.line(format_args!(
            "player error dialog skipped (--roves-no-error-dialog)"
        ));
        return;
    }
    let shown = sdl3::messagebox::show_simple_message_box(
        sdl3::messagebox::MessageBoxFlag::ERROR,
        "Roves - the game could not start",
        message,
        None,
    );
    log.line(format_args!(
        "player error dialog closed ok={}",
        shown.is_ok()
    ));
}

/// Packaged launch with `--roves-isolated-profile` only: removes the run's own cache after
/// CEF shut down and the loopback server stopped. Retried briefly: CEF child processes may still hold files while they exit.
fn remove_player_cache(log: &Log, dir: &std::path::Path) {
    let mut last = None;
    for attempt in 0..20 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {
                log.line(format_args!(
                    "player cache removed dir={} attempts={}",
                    dir.display(),
                    attempt + 1
                ));
                return;
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                log.line(format_args!(
                    "player cache removed dir={} attempts={} (absent)",
                    dir.display(),
                    attempt + 1
                ));
                return;
            },
            Err(error) => last = Some(error),
        }
    }
    if let Some(error) = last {
        log.line(format_args!(
            "player cache not removed dir={} error={error}",
            dir.display()
        ));
    }
}

fn parse_ms(name: &str, value: &str) -> Result<Duration, String> {
    value
        .parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|e| format!("--roves-{name}: {e}"))
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Options {
            log: None,
            cache_dir: run_cache_dir("roves-cef-windows-player"),
            timeout: Duration::from_secs(120),
            keep_open: false,
            timings: false,
            sustained: false,
            warmup: Duration::from_secs(10),
            sample: Duration::from_secs(30),
            game: None,
            auto_close: None,
            test_minimize: None,
            test_click: None,
            test_key: None,
            test_key_name: TestKey::default(),
            test_wheel: None,
            player: false,
            player_profile: None,
            clean_cache: false,
            error_dialog: true,
            mcp: None,
        };
        let mut mcp_switch = false;
        let mut game_dir = None;
        let mut test_key_name = None;
        let mut port = None;
        let mut cache_dir_given = false;
        let mut no_error_dialog = false;
        let mut isolated_profile = false;
        for arg in std::env::args().skip(1) {
            let Some(rest) = arg.strip_prefix("--roves-") else {
                continue; // CEF/Chromium switches are CEF's business.
            };
            let (name, value) = rest.split_once('=').unwrap_or((rest, ""));
            match name {
                "log" => options.log = Some(value.into()),
                "cache-dir" => {
                    options.cache_dir = value.into();
                    cache_dir_given = true;
                },
                "no-error-dialog" => no_error_dialog = true,
                "isolated-profile" => isolated_profile = true,
                "mcp" if value.is_empty() => mcp_switch = true,
                "timeout-ms" => options.timeout = parse_ms(name, value)?,
                "keep-open" => options.keep_open = true,
                "timings" => options.timings = true,
                "sustained" => options.sustained = true,
                "warmup-ms" => options.warmup = parse_ms(name, value)?,
                "sample-ms" => options.sample = parse_ms(name, value)?,
                "game-dir" => game_dir = Some(value.to_owned()),
                "port" => {
                    port = Some(
                        value
                            .parse::<u16>()
                            .map_err(|e| format!("--roves-port={value}: {e}"))?,
                    )
                },
                "auto-close-ms" => options.auto_close = Some(parse_ms(name, value)?),
                "test-minimize-ms" => options.test_minimize = Some(parse_ms(name, value)?),
                "test-click-ms" => options.test_click = Some(parse_ms(name, value)?),
                "test-key-ms" => options.test_key = Some(parse_test_key_delay(value)?),
                "test-key" => test_key_name = Some(TestKey::parse(value)?),
                "test-wheel-ms" => options.test_wheel = Some(parse_test_wheel_delay(value)?),
                _ => return Err(format!("unknown option --roves-{name}")),
            }
        }
        if let Some(key) = test_key_name {
            if options.test_key.is_none() {
                return Err("--roves-test-key needs --roves-test-key-ms".into());
            }
            options.test_key_name = key;
        }
        if options.sustained && options.keep_open {
            return Err("--roves-sustained closes after its sample: drop --roves-keep-open".into());
        }
        let hooks = [
            options.test_minimize.is_some(),
            options.test_click.is_some(),
            options.test_key.is_some(),
            options.test_wheel.is_some(),
        ];
        if hooks.into_iter().filter(|on| *on).count() > 1 {
            return Err(
                "one test hook per run: keep only one of --roves-test-minimize-ms, \
                        --roves-test-click-ms, --roves-test-key-ms and --roves-test-wheel-ms"
                    .into(),
            );
        }
        if options.sample.is_zero() {
            return Err("--roves-sample-ms must be > 0".into());
        }
        // Packaged launch: resolved from the executable, never from the working directory,
        // so a double click, a shortcut or another current directory open the same game.
        let exe = std::env::current_exe().ok();
        if game_dir.is_none()
            && !options.sustained
            && !options.keep_open
            && let Some(dir) = exe
                .as_deref()
                .and_then(|exe| exe.parent().map(|p| p.join(PLAYER_GAME_DIR)))
                .filter(|dir| dir.join("index.html").is_file())
        {
            game_dir = Some(dir.to_string_lossy().into_owned());
            options.player = true;
        }
        if options.player
            && (options.test_minimize.is_some()
                || options.test_click.is_some()
                || options.test_key.is_some()
                || options.test_wheel.is_some())
        {
            return Err(
                "the --roves-test-*-ms hooks need an explicit --roves-game-dir \
                        (not the packaged launch)"
                    .into(),
            );
        }
        if isolated_profile {
            if !options.player {
                return Err(
                    "--roves-isolated-profile needs the packaged game directory \
                            (game\\index.html next to the executable)"
                        .into(),
                );
            }
            if cache_dir_given {
                return Err("--roves-isolated-profile uses its own per-run cache: \
                            drop --roves-cache-dir"
                    .into());
            }
            options.cache_dir = player_cache_dir();
            options.clean_cache = true;
        } else if options.player {
            // Ordinary launch: the persistent profile and its stored origin keep the saves.
            if port.is_some() {
                return Err("the packaged launch keeps its origin port in the profile: \
                            drop --roves-port (or add --roves-isolated-profile)"
                    .into());
            }
            let root = if cache_dir_given {
                std::path::absolute(&options.cache_dir)
                    .map_err(|e| format!("--roves-cache-dir: {e}"))?
            } else {
                default_player_profile_root(exe.as_deref().ok_or("executable path unknown")?)?
            };
            options.cache_dir = root.join(PLAYER_CEF_DIR);
            options.player_profile = Some(root);
        }
        if no_error_dialog {
            if !options.player {
                return Err("--roves-no-error-dialog needs the packaged game directory \
                            (game\\index.html next to the executable)"
                    .into());
            }
            options.error_dialog = false;
        }
        match game_dir {
            Some(dir) => {
                if options.sustained || options.keep_open {
                    return Err("--roves-game-dir has no fixture checks: \
                                drop --roves-sustained/--roves-keep-open"
                        .into());
                }
                options.game = Some(GameOptions::new(&dir, port.unwrap_or(0))?);
                // welding requires an absolute root cache for the per-producer profile.
                options.cache_dir = std::path::absolute(&options.cache_dir)
                    .map_err(|e| format!("--roves-cache-dir: {e}"))?;
            },
            None if port.is_some()
                || options.auto_close.is_some()
                || options.test_minimize.is_some()
                || options.test_click.is_some()
                || options.test_key.is_some()
                || options.test_wheel.is_some() =>
            {
                return Err(
                    "--roves-port, --roves-auto-close-ms, --roves-test-minimize-ms, \
                            --roves-test-click-ms, --roves-test-key-ms and \
                            --roves-test-wheel-ms need --roves-game-dir"
                        .into(),
                );
            },
            None => {},
        }
        // Local MCP bridge: game mode only, off unless asked for (package marker
        // `mcp\enabled` next to the executable).
        if options.game.is_some() {
            options.mcp = crate::mcp::source(mcp_switch, exe.as_deref().and_then(Path::parent));
        } else if mcp_switch {
            return Err("--roves-mcp needs a game (--roves-game-dir or the packaged game)".into());
        }
        Ok(options)
    }
}

/// Evidence log: one `roves-cef-win <event>` line per event, to the file and to stderr.
#[derive(Clone)]
struct Log(Option<Arc<Mutex<File>>>);

impl Log {
    fn open(path: Option<&std::path::Path>) -> std::io::Result<Self> {
        Ok(Log(path
            .map(File::create)
            .transpose()?
            .map(|f| Arc::new(Mutex::new(f)))))
    }

    fn line(&self, args: std::fmt::Arguments<'_>) {
        let line = format!("roves-cef-win {args}\n");
        eprint!("{line}");
        if let Some(file) = &self.0
            && let Ok(mut file) = file.lock()
        {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// `--roves-sustained` only: forwards welding/grafting `log` records at warn or error level
/// (for example welding's `failed to copy CEF accelerated frame`, which otherwise goes
/// nowhere) as `upstream-log` lines. Other targets and lower levels are dropped.
struct UpstreamLog(Log);

impl UpstreamLog {
    fn install(log: Log) {
        let logger: &'static UpstreamLog = Box::leak(Box::new(UpstreamLog(log.clone())));
        match log::set_logger(logger) {
            Ok(()) => {
                log::set_max_level(log::LevelFilter::Warn);
                log.line(format_args!(
                    "upstream-log forwarding targets=welding,grafting level=warn cap={UPSTREAM_LOG_CAP}"
                ));
            },
            Err(error) => log.line(format_args!("upstream-log not installed: {error}")),
        }
    }
}

impl log::Log for UpstreamLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        let target = metadata.target();
        metadata.level() <= log::Level::Warn
            && (target.starts_with("welding") || target.starts_with("grafting"))
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let n = UPSTREAM_LOG_RECORDS.fetch_add(1, Ordering::Relaxed);
        if n >= UPSTREAM_LOG_CAP {
            return;
        }
        // Same origin as the ring's `start_us`; `-` before warmup started.
        let since = WARMUP_ORIGIN.get().map_or_else(
            || "-".to_owned(),
            |origin| origin.elapsed().as_micros().to_string(),
        );
        let thread = std::thread::current();
        self.0.line(format_args!(
            "upstream-log since_warmup_us={since} thread={:?} level={} target={} {}",
            thread.name().unwrap_or("?"),
            record.level(),
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Initial,
    Resizing,
    /// `--roves-sustained` only: unrecorded frames after the passed initial check.
    Warmup,
    /// `--roves-sustained` only: the recorded sample.
    Sampling,
    /// `--roves-game-dir` only: the game runs until the OS (or `--roves-auto-close-ms`)
    /// closes the window. Recorded with `--roves-timings` after load-end.
    Game,
    Closing,
    Done,
}

struct Probe {
    options: Options,
    log: Log,
    runtime: Option<CefRuntime>,
    state: Option<State>,
    phase: Phase,
    failures: u32,
    /// Completed pixel checks (2 in automatic mode, 1 with `--roves-keep-open`).
    checks: u32,
    exit_code: Option<i32>,
    /// Load timeout origin (`--roves-timeout-ms`, smoke phases and game load), set after CEF
    /// and SDL initialization. Not the `startup` timeline origin.
    started: Instant,
    closing_since: Option<Instant>,
    /// `startup milestone=...` timeline since the main probe run's entry.
    startup: Startup,
    /// Present only with `--roves-timings` or `--roves-sustained`.
    timings: Option<Timings>,
    /// Start of the current warmup or sample.
    phase_since: Option<Instant>,
    /// Last imported producer frame during warmup/sample (stall detection).
    last_import: Option<Instant>,
    /// welding paint generation of the last imported frame (any phase).
    last_generation: Option<u64>,
    /// Newest paint generation when the game's own navigation started (`Some(None)`: no frame
    /// yet then; `None`: not started). Frames newer than it belong to the game document.
    game_start_generation: Option<Option<u64>>,
    /// `--roves-sustained` only: redraw ring of warmup and sample, printed on rejection.
    stall: Option<Box<Ring>>,
    /// The sustained sample completed and was healthy.
    sustained_done: bool,
    /// Game mode: the loopback origin, stopped after CEF shutdown.
    server: Option<Server>,
    /// Game mode: first accepted main-frame load-end on the loopback origin.
    game_loaded_at: Option<Instant>,
    console: Console,
    /// `--roves-test-minimize-ms` only: the scripted minimize/restore cycle.
    minimize_cycle: Option<MinimizeCycle>,
    /// `--roves-test-click-ms` only: the synthetic click sequence and the SDL event subsystem
    /// it is pushed through.
    click_test: Option<ClickTest>,
    /// `--roves-test-key-ms` only: the synthetic key sequence.
    key_test: Option<KeyTest>,
    /// `--roves-test-wheel-ms` only: the synthetic wheel sequence.
    wheel_test: Option<WheelTest>,
    /// Game mode: refused popups and vetoed off-origin navigations (`external_link.rs`).
    external_links: ExternalLinks,
    /// `--roves-test-click-ms`/`--roves-test-key-ms`/`--roves-test-wheel-ms` only: the SDL
    /// event subsystem the synthetic events are pushed through.
    event_sender: Option<EventSubsystem>,
    /// Game mode: startup screen until the game's first fresh frame (README "Packaged
    /// player"). `None` keeps the fixture path unchanged.
    gate: Option<StartupGate>,
    /// Player only: the failure screen was presented (or the window is suspended) and the
    /// failure was reported; the run is closing.
    failure_reported: bool,
    /// Game mode: the failure screen has been presented at least once.
    failure_presented: bool,
    /// Game mode: when the startup screen gave way to the game.
    released_at: Option<Instant>,
    /// Game mode with `Options::mcp` only: the local MCP bridge, dropped (session record
    /// removed) when the run starts closing.
    mcp: Option<crate::mcp::Bridge>,
    /// Game mode: the browser starts on `about:blank` until the page marker registration
    /// settled (`crate::page_init` module docs).
    bootstrap: Option<Bootstrap>,
    /// Game mode: registration of the `__ROVES__` (and, with the bridge, `__ROVES_MCP__`)
    /// page markers through welding's DevTools channel.
    page_init: Option<crate::page_init::PageInit>,
    /// Set by the host API's `exit`/`clear_content_cache` (server thread); the main loop then
    /// closes through the same path as an OS close request.
    game_exit: Arc<OnceLock<&'static str>>,
    /// `steam` feature, game mode, Steam running: the Steamworks client shared with the host
    /// API; callbacks run once per loop turn, shut down first in `finish`.
    steam: Option<Arc<SteamHandle>>,
}

/// The `about:blank` document that precedes the game while the page marker script is
/// registered. Its navigation events are not the game's and are skipped; once the game
/// committed on its origin, `about:blank` is an off-origin navigation again.
struct Bootstrap {
    /// The game URL, taken when the registration settled.
    game_url: Option<String>,
    /// An on-origin load-start or address change was seen.
    game_committed: bool,
}

impl Bootstrap {
    /// Whether `event` belongs to the bootstrap document. Load-end is skipped until the game's
    /// own accepted load-end (`game_loaded`): CEF may report the bootstrap load-end late.
    fn skips(&mut self, event: &NavigationEvent, origin: &str, game_loaded: bool) -> bool {
        let blank = |url: &str| url == crate::mcp_rules::BOOTSTRAP_URL;
        match event {
            NavigationEvent::LoadStart { url } | NavigationEvent::AddressChanged { url } => {
                if on_origin(origin, url) {
                    self.game_committed = true;
                }
                !self.game_committed && blank(url)
            },
            NavigationEvent::LoadEnd { url, .. } => !game_loaded && blank(url),
            _ => false,
        }
    }
}

/// How long the browser may take to close after `close()` before the probe gives up.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Field order is drop order: the producer goes before the device it imports into and before
/// the CEF runtime (`cef::shutdown` on drop).
struct State {
    producer: WindowsCefProducer,
    frame: Option<ImportedTexture>,
    composed_target: Option<wgpu::Texture>,
    blit: Blit,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    host: HostWgpuContext,
    window: Window,
    runtime: CefRuntime,
    imported: u32,
    imported_at_size: u32,
    presented: u32,
    load_ended: bool,
    /// Accelerated (shared texture import) or CEF software output (README "Software paint").
    paint: PaintPath,
    /// Game mode only: SDL3 -> welding input forwarding.
    input: Option<Input>,
    /// Set while the window is minimized or hidden: no frame import, compose or present.
    suspended_since: Option<Instant>,
    /// Loop turns spent suspended (event handling and deadlines only).
    suspended_ticks: u64,
    /// Resume instant until the first frame is presented after it.
    resumed_at: Option<Instant>,
    /// Resume instant until the first producer frame is imported after it.
    resumed_import_at: Option<Instant>,
    /// Clear colour of the composed target: red in the fixture smoke (its pixel checks rely
    /// on it), the dark startup-screen colour in game mode.
    background: wgpu::Color,
    /// Game mode only: the startup/failure screen logo textures.
    splash: Option<Splash>,
}

/// Game mode: the startup/failure screen textures, uploaded once (premultiplied
/// `Rgba8Unorm`): the `LOGO_PX` logo, the `WORDMARK_SIZE` wordmark and two 1x1 colours for
/// the loading bar. `None` logo/wordmark: its decoding failed and the screens omit it (the
/// bar needs no decoding).
struct Splash {
    logo: Option<wgpu::TextureView>,
    failed: Option<wgpu::TextureView>,
    wordmark: Option<wgpu::TextureView>,
    wordmark_failed: Option<wgpu::TextureView>,
    track: wgpu::TextureView,
    indicator: wgpu::TextureView,
    /// Origin of the loading-bar animation: when the startup screen was first composed.
    started: Instant,
}

impl Splash {
    fn new(
        host: &HostWgpuContext,
        branding: Option<&Branding>,
        wordmark: Option<&Wordmark>,
    ) -> Self {
        let upload = |label, (width, height): (u32, u32), rgba: &[u8]| {
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = host.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COMPOSED_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            host.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                size,
            );
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        };
        let logo = (LOGO_PX, LOGO_PX);
        Splash {
            logo: branding.map(|b| upload("roves startup logo", logo, &b.logo)),
            failed: branding.map(|b| upload("roves failure logo", logo, &b.failed)),
            wordmark: wordmark.map(|w| upload("roves startup wordmark", WORDMARK_SIZE, &w.rgba)),
            wordmark_failed: wordmark
                .map(|w| upload("roves failure wordmark", WORDMARK_SIZE, &w.failed)),
            track: upload("roves loading-bar track", (1, 1), &BAR_TRACK),
            indicator: upload("roves loading-bar indicator", (1, 1), &BAR_INDICATOR),
            started: Instant::now(),
        }
    }
}

impl Probe {
    fn fail(&mut self, what: std::fmt::Arguments<'_>) {
        self.failures += 1;
        self.log.line(format_args!("FAIL {what}"));
    }

    fn start(&mut self, video: &VideoSubsystem) -> Result<State, String> {
        let game = self.options.game.is_some();
        let mut builder = video.window(
            if game {
                "Roves"
            } else {
                "Roves CEF + SDL3 probe"
            },
            INITIAL_SIZE.width,
            INITIAL_SIZE.height,
        );
        builder.high_pixel_density();
        if !self.options.sustained {
            builder.resizable();
        }
        // Game mode: hidden until the startup screen can be presented, so the OS never shows
        // an empty window first. The fixture keeps its visible window unchanged.
        if game {
            builder.hidden();
        }
        let mut window = builder.build().map_err(|e| format!("window: {e}"))?;
        let branding = if game {
            let started = Instant::now();
            // Window icon: the project's `icon.png` next to the executable when valid, the
            // Roves logo otherwise (README "Project icon"). The screens keep the Roves logo.
            let exe = std::env::current_exe().ok();
            let project = match crate::window_icon::project_icon(
                exe.as_deref().and_then(std::path::Path::parent),
            ) {
                crate::window_icon::ProjectIcon::Found { icon, source_px } => {
                    let set = window.set_icon(&icon);
                    self.log.line(format_args!(
                        "window icon source=project size={source_px}x{source_px} set={set}"
                    ));
                    set
                },
                crate::window_icon::ProjectIcon::Absent => {
                    self.log
                        .line(format_args!("window icon source=roves reason=absent"));
                    false
                },
                crate::window_icon::ProjectIcon::Invalid(error) => {
                    self.log.line(format_args!(
                        "window icon source=roves reason=invalid error={error}"
                    ));
                    false
                },
            };
            match crate::splash::decode() {
                Ok(branding) => {
                    let icon = project || window.set_icon(&branding.icon);
                    self.log.line(format_args!(
                        "branding decoded logo={LOGO_PX}x{LOGO_PX} window-icon={icon} elapsed_ms={}",
                        started.elapsed().as_millis()
                    ));
                    Some(branding)
                },
                Err(error) => {
                    // Not fatal: the screens fall back to the plain dark background.
                    self.log
                        .line(format_args!("branding unavailable error={error}"));
                    None
                },
            }
        } else {
            None
        };
        let wordmark = if game {
            match crate::splash::wordmark() {
                Ok(wordmark) => {
                    let (w, h) = WORDMARK_SIZE;
                    self.log.line(format_args!(
                        "wordmark rendered size={w}x{h} font=MetalMania"
                    ));
                    Some(wordmark)
                },
                Err(error) => {
                    // Not fatal: the screens omit the wordmark.
                    self.log
                        .line(format_args!("wordmark unavailable error={error}"));
                    None
                },
            }
        } else {
            None
        };
        if game {
            video.text_input().start(&window);
        }
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // Welding's Windows import is DX12 only (D3D12 OpenSharedHandle through grafting).
        instance_desc.backends = wgpu::Backends::DX12;
        let instance = wgpu::Instance::new(instance_desc);
        // SAFETY: SDL owns the HWND. State drops the surface before the window,
        // and both stay on this thread. Locals also drop in reverse creation order.
        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_display_and_window(&window, &window)
                .map_err(|e| format!("SDL3 window handle: {e}"))?;
            instance.create_surface_unsafe(target)
        }
        .map_err(|e| format!("surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("DX12 adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Roves CEF host device"),
            ..Default::default()
        }))
        .map_err(|e| format!("DX12 device: {e}"))?;
        let info = adapter.get_info();
        self.log.line(format_args!(
            "adapter name={:?} backend={:?} type={:?} driver={:?} {:?}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        ));
        let host = HostWgpuContext::new(device, queue);
        if host.backend != InteropBackend::Dx12 {
            return Err(format!("interop backend {:?}, expected Dx12", host.backend));
        }
        // WARP (`DeviceType::Cpu`, also what Windows uses without a GPU) delivers no
        // accelerated OSR frame: CEF's software output is the only path that shows the game.
        let force = std::env::var(FORCE_CPU_PAINT_ENV).ok();
        let paint = paint_path(
            info.device_type == wgpu::DeviceType::Cpu,
            true,
            force.as_deref(),
        );
        self.log.line(format_args!(
            "paint path={paint:?} evidence={}",
            paint.evidence()
        ));
        host.device.on_uncaptured_error(Arc::new({
            let log = self.log.clone();
            move |error| log.line(format_args!("FAIL wgpu uncaptured error: {error}"))
        }));

        let caps = surface.get_capabilities(&adapter);
        // A non-sRGB surface keeps the composed bytes unchanged (no second encode).
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or_else(|| format!("no 8-bit unorm surface format in {:?}", caps.formats))?;
        let size = pixel_size(&window);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&host.device, &surface_config);
        self.log.line(format_args!(
            "surface format={format:?} size={}x{} scale={}",
            surface_config.width,
            surface_config.height,
            window.display_scale()
        ));

        let blit = Blit::new(&host.device, format);
        let background = if game {
            let [r, g, b, a] = SPLASH_BACKGROUND.map(f64::from);
            wgpu::Color { r, g, b, a }
        } else {
            let [r, g, b, a] = BACKGROUND.map(f64::from);
            wgpu::Color { r, g, b, a }
        };
        let splash = game.then(|| Splash::new(&host, branding.as_ref(), wordmark.as_ref()));
        drop((branding, wordmark));
        // Game mode: the first startup-screen frame, then the window appears, before the CEF
        // browser is even created.
        let mut composed_target = None;
        if let Some(splash) = splash.as_ref() {
            let visible = window.show();
            let shown = composed_target_of(
                &host,
                &mut composed_target,
                (surface_config.width, surface_config.height),
            );
            let commands = splash_commands(&host, &blit, &shown, background, splash, false);
            host.queue.submit([commands]);
            let presented = present_to(&surface, &surface_config, &host, &blit, &shown, None, None);
            self.log.line(format_args!(
                "startup screen shown size={}x{} window-shown={visible} presented={presented}",
                surface_config.width, surface_config.height
            ));
        }
        let runtime = self.runtime.take().ok_or("CEF runtime already consumed")?;
        let game_origin = self.server.as_ref().map(|server| server.origin.clone());
        let url = match game_origin {
            Some(origin) => format!("{origin}/"),
            None => format!(
                "data:text/html;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(FIXTURE_HTML)
            ),
        };
        // Game mode: the game URL waits for the page marker registration (`__ROVES__`, plus
        // `__ROVES_MCP__` while the bridge runs).
        let url = match game_origin_script(self.server.as_ref(), self.mcp.is_some()) {
            Some(script) if game => {
                self.page_init = Some(crate::page_init::PageInit::new(script));
                self.bootstrap = Some(Bootstrap {
                    game_url: Some(url),
                    game_committed: false,
                });
                crate::mcp_rules::BOOTSTRAP_URL.to_owned()
            },
            _ => url,
        };
        let producer = WindowsCefProducer::new(
            &runtime,
            WindowsCefConfig {
                surface: CefSurfaceConfig {
                    initial_url: url,
                    initial_size: size,
                    scale_factor: window.display_scale(),
                    // Fixture: transparent, uncovered page pixels arrive with premultiplied
                    // alpha 0. Game: opaque white where the page paints nothing, as in Chrome.
                    background_color: game.then_some([255, 255, 255]),
                    prefer_accelerated: paint == PaintPath::Accelerated,
                    // Game: a per-producer on-disk profile inside the run's root cache. The
                    // fixture keeps welding's isolated in-memory profile, unchanged.
                    user_data_dir: game.then(|| crate::game::profile_dir(&self.options.cache_dir)),
                    // Game mode: welding's in-process DevTools channel (`SendDevToolsMessage` +
                    // a message observer; no port), for the page markers and the MCP bridge.
                    devtools_protocol: self.page_init.is_some(),
                    // Game mode: one document on the loopback origin (`external_link.rs`);
                    // off-origin main-frame navigations are cancelled before they start. The
                    // fixture keeps every navigation, unchanged.
                    main_frame_navigation_policy: self.server.as_ref().map(|server| {
                        Arc::new(main_frame_policy(&server.origin)) as MainFrameNavigationPolicy
                    }),
                    ..Default::default()
                },
            },
            &host,
        )
        .map_err(|e| format!("WindowsCefProducer: {e}"))?;
        self.log.line(format_args!(
            "producer created accelerated={} background={} evidence={}",
            paint == PaintPath::Accelerated,
            if game { "white" } else { "transparent" },
            paint.evidence()
        ));
        Ok(State {
            producer,
            frame: None,
            composed_target,
            blit,
            surface,
            surface_config,
            host,
            window,
            runtime,
            imported: 0,
            imported_at_size: 0,
            presented: 0,
            load_ended: false,
            paint,
            input: game.then(Input::new),
            suspended_since: None,
            suspended_ticks: 0,
            resumed_at: None,
            resumed_import_at: None,
            background,
            splash,
        })
    }

    fn redraw(&mut self, mouse: &sdl3::mouse::MouseUtil) {
        let Some(s) = self.state.as_mut() else { return };
        // Steady-state samples only: smoke phases after load-end (the frame that receives
        // load-end is not sampled) with `--roves-timings`, and the sustained sample; never
        // warmup or closing. Each check readback runs after `present` and after its phase's
        // report, so it never lands in a sample.
        let record = match self.phase {
            Phase::Initial | Phase::Resizing | Phase::Game => self.options.timings && s.load_ended,
            Phase::Sampling => true,
            Phase::Warmup | Phase::Closing | Phase::Done => false,
        };
        // Stall ring: warmup and sample redraws, independent of `record`, own clock reads.
        let sustained_phase = matches!(self.phase, Phase::Warmup | Phase::Sampling);
        let mut marks =
            (self.stall.is_some() && sustained_phase).then(|| Marks::new(Instant::now()));
        let frame_start = (record && self.timings.is_some()).then(Instant::now);
        let events = drain_web_events(s);
        if let (Some(t), Some(start)) = (self.timings.as_mut(), frame_start) {
            t.lap(Stage::Events, start);
        }
        if !self.handle_web_events(events, mouse) {
            return;
        }
        let Some(s) = self.state.as_mut() else { return };
        let mut timings = self.timings.as_mut().filter(|_| record);

        // Ring only: `events` also covers handling and logging the drained events.
        mark(&mut marks, Step::Events);
        let acquire_start = now_if(&timings);
        let acquired = s.producer.acquire_frame(&s.host);
        mark(&mut marks, Step::AcquireFrame);
        let poll = match &acquired {
            Ok(Some(frame)) => Poll::Imported(frame.generation),
            Ok(None) => Poll::Empty,
            Err(_) => Poll::Error,
        };
        if let (Some(t), Some(acquire_start)) = (timings.as_deref_mut(), acquire_start) {
            match &acquired {
                Ok(Some(frame)) => {
                    let now = t.lap(Stage::Import, acquire_start);
                    t.imported(now, frame.generation);
                },
                Ok(None) => t.empty_poll(),
                Err(_) => {},
            }
        }
        let fresh = matches!(acquired, Ok(Some(_)));
        match acquired {
            Ok(Some(frame)) => {
                s.imported += 1;
                if s.imported == 1 {
                    startup_line(&self.log, &mut self.startup, Milestone::FirstImport);
                }
                self.last_generation = Some(frame.generation);
                if matches!(self.phase, Phase::Warmup | Phase::Sampling) {
                    self.last_import = Some(Instant::now());
                }
                if let Some(at) = s.resumed_import_at.take() {
                    self.log.line(format_args!(
                        "resume first-import after_ms={} generation={}",
                        at.elapsed().as_millis(),
                        frame.generation
                    ));
                }
                let expected = (s.surface_config.width, s.surface_config.height);
                let size = (frame.size.width, frame.size.height);
                if size == expected {
                    s.imported_at_size += 1;
                }
                if s.imported == 1 || s.imported_at_size == 1 {
                    self.log.line(format_args!(
                        "import frame={} size={}x{} format={:?} generation={} usage={:?} path={} evidence={}",
                        s.imported,
                        size.0,
                        size.1,
                        frame.format,
                        frame.generation,
                        frame.texture.usage(),
                        match s.paint {
                            PaintPath::Accelerated => "dx12-shared-handle",
                            PaintPath::Software(_) => "cpu-paint-upload",
                        },
                        s.paint.evidence()
                    ));
                }
                s.frame = Some(frame);
            },
            Ok(None) => {},
            Err(error) => self.log.line(format_args!("acquire_frame error: {error}")),
        }
        // Game mode: the startup screen gives way on the first frame painted after load-end.
        if let Some(gate) = self.gate.as_mut() {
            let settle_due = self
                .game_loaded_at
                .is_some_and(|at| at.elapsed() >= STARTUP_SETTLE_AFTER_LOAD_END);
            let game_painted = match (self.last_generation, self.game_start_generation) {
                (Some(newest), Some(start)) => start.is_none_or(|b| newest > b),
                _ => false,
            };
            let released = match self.last_generation {
                Some(generation) if fresh && gate.imported(generation) => Some((generation, false)),
                Some(generation) if settle_due && gate.settle(game_painted) => {
                    Some((generation, true))
                },
                _ => None,
            };
            if let Some((generation, settled)) = released {
                self.log.line(format_args!(
                    "startup released generation={generation} imported={} after_load_end_ms={}                      settled={settled}",
                    s.imported,
                    self.game_loaded_at.map_or(0, |at| at.elapsed().as_millis())
                ));
                startup_line(&self.log, &mut self.startup, Milestone::StartupReleased);
                self.released_at = Some(Instant::now());
                if let Some(delay) = self.options.auto_close
                    && self.options.player
                {
                    self.log.line(format_args!(
                        "auto-close scheduled delay_ms={} from=startup-release",
                        delay.as_millis()
                    ));
                }
            }
            gate.end_turn(self.last_generation);
        }

        let composed = match (self.gate, s.splash.as_ref()) {
            (Some(gate), Some(splash)) if !gate.released() => {
                let size = (s.surface_config.width, s.surface_config.height);
                let target = composed_target_of(&s.host, &mut s.composed_target, size);
                let commands = splash_commands(
                    &s.host,
                    &s.blit,
                    &target,
                    s.background,
                    splash,
                    gate.failure().is_some(),
                );
                s.host.queue.submit([commands]);
                Some(target)
            },
            _ => compose(s, timings.as_deref_mut()),
        };
        mark(&mut marks, Step::Compose);
        let sampling = self.phase == Phase::Sampling;
        let Some(composed) = composed else {
            if let (Some(ring), Some(marks)) = (self.stall.as_mut(), marks.as_ref()) {
                ring.push(marks, poll, false, sampling);
            }
            return;
        };
        let presented = present(s, &composed, timings.as_deref_mut(), marks.as_mut());
        if presented {
            s.presented += 1;
            if s.presented == 1 {
                startup_line(&self.log, &mut self.startup, Milestone::FirstPresent);
            }
            if let Some(failure) = self.gate.and_then(StartupGate::failure)
                && !self.failure_presented
            {
                self.failure_presented = true;
                self.log.line(format_args!(
                    "startup failure screen presented reason={}",
                    failure.token()
                ));
            }
            if let (Some(t), Some(frame_start)) = (timings, frame_start) {
                let now = t.lap(Stage::Frame, frame_start);
                t.presented(now);
            }
            // Before any new producer frame (`fresh=false`) this is the last frame imported
            // before the suspension, re-presented on the reconfigured surface.
            if let Some(at) = s.resumed_at.take() {
                self.log.line(format_args!(
                    "resume first-present after_ms={} fresh_import={fresh} generation={:?}",
                    at.elapsed().as_millis(),
                    self.last_generation
                ));
            }
        }
        // Before `sustained_step`, so the redraw that trips the stall check is in the ring.
        if let (Some(ring), Some(marks)) = (self.stall.as_mut(), marks.as_ref()) {
            ring.push(marks, poll, presented, sampling);
        }

        let ready = s.load_ended && s.imported_at_size >= FRAMES_BEFORE_CHECK;
        match self.phase {
            Phase::Initial if ready => {
                self.report_smoke_timings("initial");
                let (checks, failures) = (self.checks, self.failures);
                self.check("initial", &composed);
                let passed = self.checks > checks && self.failures == failures;
                if self.options.sustained {
                    if passed {
                        self.start_warmup();
                    } else {
                        self.fail(format_args!(
                            "sustained not started: the initial check did not pass"
                        ));
                        self.close();
                    }
                } else if self.options.keep_open {
                    self.phase = Phase::Done;
                } else {
                    let s = self.state.as_mut().expect("state");
                    s.imported_at_size = 0;
                    let scale = s.window.display_scale().max(1.0);
                    let width = (RESIZED_SIZE.width as f32 / scale).round() as u32;
                    let height = (RESIZED_SIZE.height as f32 / scale).round() as u32;
                    let result = s.window.set_size(width, height);
                    let applied = pixel_size(&s.window);
                    self.log.line(format_args!(
                        "resize requested {}x{} applied={applied:?} result={result:?}",
                        RESIZED_SIZE.width, RESIZED_SIZE.height
                    ));
                    if let Err(error) = result {
                        self.fail(format_args!("resize request: {error}"));
                        self.close();
                        return;
                    }
                    self.resized(applied);
                    self.phase = Phase::Resizing;
                }
            },
            Phase::Resizing if ready => {
                self.report_smoke_timings("resized");
                self.check("resized", &composed);
                self.close();
            },
            // Not bounded by --roves-timeout-ms: bounded by warmup + sample, and stall-checked.
            Phase::Warmup | Phase::Sampling => self.sustained_step(),
            _ => self.deadlines(),
        }
    }

    /// One loop turn while frame work is suspended: CEF events and deadlines, no wgpu work.
    /// The producer's latest-frame slot keeps only the newest paint for the resume.
    fn idle(&mut self, mouse: &sdl3::mouse::MouseUtil) {
        let Some(s) = self.state.as_mut() else { return };
        s.suspended_ticks += 1;
        let events = drain_web_events(s);
        if self.handle_web_events(events, mouse) {
            self.deadlines();
        }
    }

    /// Timeouts and auto-close that do not depend on a presented frame.
    fn deadlines(&mut self) {
        let timed_out = self.started.elapsed() > self.options.timeout;
        match self.phase {
            Phase::Initial | Phase::Resizing if timed_out => {
                let s = self.state.as_ref().expect("state");
                let (load_ended, imported, at_size) =
                    (s.load_ended, s.imported, s.imported_at_size);
                let phase = self.phase;
                self.report_smoke_timings(if phase == Phase::Initial {
                    "initial-timeout"
                } else {
                    "resized-timeout"
                });
                self.fail(format_args!(
                    "timeout in phase {phase:?}: load_ended={load_ended} imported={imported} at_size={at_size}"
                ));
                self.exit_code.get_or_insert(2);
                self.close();
            },
            // Packaged player: --roves-timeout-ms bounds the whole startup screen (load-end and
            // the first fresh frame); the failure screen and dialog then end the run.
            Phase::Game
                if self.options.player
                    && timed_out
                    && self.gate.and_then(StartupGate::timed_out).is_some() =>
            {
                let failure = self
                    .gate
                    .and_then(StartupGate::timed_out)
                    .expect("checked in the guard");
                self.fail(format_args!(
                    "startup not released within {} ms ({})",
                    self.options.timeout.as_millis(),
                    failure.token()
                ));
                self.fail_startup(failure);
            },
            // Game: --roves-timeout-ms bounds only the load; afterwards the player (or
            // --roves-auto-close-ms) decides when to close.
            Phase::Game if !self.options.player && self.game_loaded_at.is_none() && timed_out => {
                self.fail(format_args!(
                    "timeout: the game did not finish loading in {} ms",
                    self.options.timeout.as_millis()
                ));
                self.exit_code.get_or_insert(2);
                self.close();
            },
            Phase::Game if self.minimize_cycle.is_some_and(|c| !c.finished()) => {
                self.step_minimize_cycle();
            },
            Phase::Game if self.click_test.is_some_and(|c| !c.finished()) => {
                self.step_click_test();
            },
            Phase::Game if self.key_test.is_some_and(|k| !k.finished()) => {
                self.step_key_test();
            },
            Phase::Game if self.wheel_test.is_some_and(|w| !w.finished()) => {
                self.step_wheel_test();
            },
            // With a --roves-test-*-ms hook, only once the test sequence has finished. Counted
            // from load-end, or in the packaged player from the startup release (the game on
            // screen), so an unreleased startup still ends in its failure path.
            Phase::Game
                if self
                    .options
                    .auto_close
                    .zip(if self.options.player {
                        self.released_at
                    } else {
                        self.game_loaded_at
                    })
                    .is_some_and(|(delay, at)| at.elapsed() >= delay) =>
            {
                self.log.line(format_args!("auto-close window"));
                self.close();
            },
            _ => {},
        }
    }

    /// `--roves-test-minimize-ms` only: advances the scripted minimize -> hold -> restore
    /// cycle. Runs from `deadlines`, so also on suspended turns: the restore request is
    /// itself evidence that deadlines kept running while frame work was suspended.
    fn step_minimize_cycle(&mut self) {
        let (Some(mut cycle), Some(delay), Some(loaded)) = (
            self.minimize_cycle,
            self.options.test_minimize,
            self.game_loaded_at,
        ) else {
            return;
        };
        let suspended = self.suspended();
        let action = cycle.step(Instant::now(), loaded.elapsed() >= delay, suspended);
        self.minimize_cycle = Some(cycle);
        let s = self.state.as_mut().expect("state");
        match action {
            CycleAction::None => {},
            CycleAction::Minimize => {
                let ok = s.window.minimize();
                self.log.line(format_args!(
                    "test minimize requested ok={ok} hold_ms={}",
                    TEST_MINIMIZE_HOLD.as_millis()
                ));
                if !ok {
                    self.minimize_cycle = Some(MinimizeCycle::Failed);
                    self.fail(format_args!("test minimize: SDL_MinimizeWindow failed"));
                    self.close();
                }
            },
            CycleAction::Restore => {
                let ok = s.window.restore();
                self.log.line(format_args!(
                    "test restore requested ok={ok} idle_ticks={}",
                    s.suspended_ticks
                ));
                if !ok {
                    self.minimize_cycle = Some(MinimizeCycle::Failed);
                    self.fail(format_args!("test restore: SDL_RestoreWindow failed"));
                    self.close();
                }
            },
            CycleAction::Complete => {
                self.log.line(format_args!("test minimize cycle complete"));
            },
            CycleAction::Fail(what) => {
                self.fail(format_args!("test minimize cycle: {what}"));
                self.close();
            },
        }
    }

    /// `--roves-test-click-ms` only: requests the synthetic click once due and times out the
    /// pending steps. The button events are pushed into the SDL queue (stamped with
    /// `TEST_CLICK_MOUSE_ID`) and come back through the loop's ordinary `poll_iter` ->
    /// `window_event` -> `Input` -> welding path on a later turn.
    fn step_click_test(&mut self) {
        let (Some(mut click), Some(delay), Some(loaded)) = (
            self.click_test,
            self.options.test_click,
            self.game_loaded_at,
        ) else {
            return;
        };
        let action = click.tick(Instant::now(), loaded.elapsed() >= delay);
        self.click_test = Some(click);
        match action {
            ClickAction::Push => {
                let s = self.state.as_ref().expect("state");
                let window_id = s.window.id();
                let (x, y) = TEST_CLICK_POINT;
                let button = |down: bool| {
                    let (timestamp, which, mouse_btn, clicks) =
                        (0, TEST_CLICK_MOUSE_ID, SdlButton::Left, 1);
                    if down {
                        Event::MouseButtonDown {
                            timestamp,
                            window_id,
                            which,
                            mouse_btn,
                            clicks,
                            x,
                            y,
                        }
                    } else {
                        Event::MouseButtonUp {
                            timestamp,
                            window_id,
                            which,
                            mouse_btn,
                            clicks,
                            x,
                            y,
                        }
                    }
                };
                let pushed = match self.event_sender.as_ref() {
                    Some(events) => events
                        .push_event(button(true))
                        .and_then(|()| events.push_event(button(false)))
                        .map_err(|e| e.to_string()),
                    None => Err("no SDL event subsystem".into()),
                };
                self.log.line(format_args!(
                    "test click requested x={x} y={y} window_id={window_id} pushed={}",
                    pushed.is_ok()
                ));
                if let Err(error) = pushed {
                    self.click_test = Some(ClickTest::Failed);
                    self.fail(format_args!("test click: SDL_PushEvent failed: {error}"));
                    self.close();
                }
            },
            ClickAction::Fail(what) => {
                self.fail(format_args!("test click: {what}"));
                self.close();
            },
            ClickAction::None | ClickAction::Complete => {},
        }
    }

    /// `--roves-test-click-ms` only: applies a test click action that came from an input or
    /// console event.
    fn click_test_action(&mut self, action: ClickAction) {
        match action {
            ClickAction::Complete => self.log.line(format_args!("test click complete")),
            ClickAction::Fail(what) => {
                self.fail(format_args!("test click: {what}"));
                self.close();
            },
            ClickAction::None | ClickAction::Push => {},
        }
    }

    /// `--roves-test-key-ms` only: requests the synthetic press of `--roves-test-key`
    /// (ArrowRight by default, or CapsLock) once due and times out the pending steps. The key
    /// events carry the matching SDL keycode and scancode, are pushed into the SDL queue
    /// (stamped with `TEST_KEY_KEYBOARD_ID`, no modifiers, not a repeat) and come back through
    /// the loop's ordinary `poll_iter` -> `window_event` -> `Input` -> welding
    /// `send_keyboard_input` path on a later turn. SDL produces no `TextInput` for a pushed key
    /// event; neither key has a character anyway. `raw` stays 0 (SDL's own value for an event
    /// without a scan code), so the run exercises the `MapVirtualKeyW` fallback of
    /// `native_key_code` (for ArrowRight the path where Windows drops the `0xE0`) and, for
    /// CapsLock, the `non_character_virtual_key` VK_CAPITAL entry; real keys carry SDL's scan
    /// code in `raw`.
    fn step_key_test(&mut self) {
        let (Some(mut key), Some(delay), Some(loaded)) =
            (self.key_test, self.options.test_key, self.game_loaded_at)
        else {
            return;
        };
        let action = key.tick(Instant::now(), loaded.elapsed() >= delay);
        self.key_test = Some(key);
        match action {
            KeyAction::Push => {
                let s = self.state.as_ref().expect("state");
                let window_id = s.window.id();
                let test_key = self.options.test_key_name;
                let (sdl_keycode, sdl_scancode) = match test_key {
                    TestKey::ArrowRight => (Keycode::Right, Scancode::Right),
                    TestKey::CapsLock => (Keycode::CapsLock, Scancode::CapsLock),
                };
                let event = |down: bool| {
                    let (timestamp, keycode, scancode, keymod, repeat, which, raw) = (
                        0,
                        Some(sdl_keycode),
                        Some(sdl_scancode),
                        Mod::empty(),
                        false,
                        TEST_KEY_KEYBOARD_ID,
                        0,
                    );
                    if down {
                        Event::KeyDown {
                            timestamp,
                            window_id,
                            keycode,
                            scancode,
                            keymod,
                            repeat,
                            which,
                            raw,
                        }
                    } else {
                        Event::KeyUp {
                            timestamp,
                            window_id,
                            keycode,
                            scancode,
                            keymod,
                            repeat,
                            which,
                            raw,
                        }
                    }
                };
                let pushed = match self.event_sender.as_ref() {
                    Some(events) => events
                        .push_event(event(true))
                        .and_then(|()| events.push_event(event(false)))
                        .map_err(|e| e.to_string()),
                    None => Err("no SDL event subsystem".into()),
                };
                self.log.line(format_args!(
                    "test key requested key={} window_id={window_id} pushed={}",
                    test_key.sdl_name(),
                    pushed.is_ok()
                ));
                if let Err(error) = pushed {
                    self.key_test = Some(KeyTest::Failed);
                    self.fail(format_args!("test key: SDL_PushEvent failed: {error}"));
                    self.close();
                }
            },
            KeyAction::Fail(what) => {
                self.fail(format_args!("test key: {what}"));
                self.close();
            },
            KeyAction::None | KeyAction::Complete => {},
        }
    }

    /// `--roves-test-key-ms` only: applies a test key action that came from an input or
    /// console event.
    fn key_test_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Complete => self.log.line(format_args!("test key complete")),
            KeyAction::Fail(what) => {
                self.fail(format_args!("test key: {what}"));
                self.close();
            },
            KeyAction::None | KeyAction::Push => {},
        }
    }

    /// `--roves-test-wheel-ms` only: requests the synthetic wheel notch once due and times out
    /// the pending steps. One `MouseWheel` (`TEST_WHEEL_LINES`, normal direction, stamped with
    /// `TEST_WHEEL_MOUSE_ID`) is pushed into the SDL queue and comes back through the loop's
    /// ordinary `poll_iter` -> `window_event` -> `Input` -> welding `send_mouse_input`
    /// (`WheelScrolled`) path on a later turn. `Input` places the wheel at its tracked cursor
    /// (the last forwarded motion, initially 0,0), not at the event's `mouse_x`/`mouse_y`.
    fn step_wheel_test(&mut self) {
        let (Some(mut wheel), Some(delay), Some(loaded)) = (
            self.wheel_test,
            self.options.test_wheel,
            self.game_loaded_at,
        ) else {
            return;
        };
        let action = wheel.tick(Instant::now(), loaded.elapsed() >= delay);
        self.wheel_test = Some(wheel);
        match action {
            WheelAction::Push => {
                let s = self.state.as_ref().expect("state");
                let window_id = s.window.id();
                let (x, y) = TEST_WHEEL_LINES;
                let event = Event::MouseWheel {
                    timestamp: 0,
                    window_id,
                    which: TEST_WHEEL_MOUSE_ID,
                    x,
                    y,
                    direction: MouseWheelDirection::Normal,
                    mouse_x: 0.0,
                    mouse_y: 0.0,
                    integer_x: x as i32,
                    integer_y: y as i32,
                };
                let pushed = match self.event_sender.as_ref() {
                    Some(events) => events.push_event(event).map_err(|e| e.to_string()),
                    None => Err("no SDL event subsystem".into()),
                };
                self.log.line(format_args!(
                    "test wheel requested x={x} y={y} direction=normal window_id={window_id} pushed={}",
                    pushed.is_ok()
                ));
                if let Err(error) = pushed {
                    self.wheel_test = Some(WheelTest::Failed);
                    self.fail(format_args!("test wheel: SDL_PushEvent failed: {error}"));
                    self.close();
                }
            },
            WheelAction::Fail(what) => {
                self.fail(format_args!("test wheel: {what}"));
                self.close();
            },
            WheelAction::None | WheelAction::Complete => {},
        }
    }

    /// `--roves-test-wheel-ms` only: applies a test wheel action that came from an input or
    /// console event.
    fn wheel_test_action(&mut self, action: WheelAction) {
        match action {
            WheelAction::Complete => self.log.line(format_args!("test wheel complete")),
            WheelAction::Fail(what) => {
                self.fail(format_args!("test wheel: {what}"));
                self.close();
            },
            WheelAction::None | WheelAction::Push => {},
        }
    }

    /// Game mode: the startup screen turns into the failure screen (dimmed logo, title). The
    /// player run then reports it once presented (`report_startup_failure`).
    fn fail_startup(&mut self, failure: StartupFailure) {
        if !self.gate.as_mut().is_some_and(|gate| gate.fail(failure)) {
            return;
        }
        self.log
            .line(format_args!("startup failed reason={}", failure.token()));
        if let Some(s) = self.state.as_mut() {
            let _ = s.window.set_title("Roves - the game could not start");
        }
    }

    /// Player only, once per run after each loop turn: when startup failed and the failure
    /// screen was presented (or the window is minimized/hidden, so nothing can be presented),
    /// shows the reason in a native SDL message box over the window unless
    /// `--roves-no-error-dialog`, then closes with exit code 2. A failure while the run is
    /// already closing only sets the exit code.
    fn report_startup_failure(&mut self) {
        if self.failure_reported || !self.options.player {
            return;
        }
        let Some(failure) = self.gate.and_then(StartupGate::failure) else {
            return;
        };
        let closing = self.phase == Phase::Closing;
        if !(closing || self.failure_presented || self.suspended()) {
            return;
        }
        self.failure_reported = true;
        self.exit_code.get_or_insert(2);
        if closing {
            return;
        }
        if self.options.error_dialog {
            let message = format!(
                "{}\n\nDetails: {} (log: {})",
                failure.message(),
                failure.token(),
                self.options
                    .log
                    .as_ref()
                    .map_or_else(|| "not written".to_owned(), |p| p.display().to_string())
            );
            let window = self.state.as_ref().map(|s| &s.window);
            let shown = sdl3::messagebox::show_simple_message_box(
                sdl3::messagebox::MessageBoxFlag::ERROR,
                "Roves - the game could not start",
                &message,
                window,
            );
            self.log.line(format_args!(
                "startup failure dialog closed ok={}",
                shown.is_ok()
            ));
        } else {
            self.log.line(format_args!(
                "startup failure dialog skipped (--roves-no-error-dialog)"
            ));
        }
        self.close();
    }

    /// Suspends frame work while the window is minimized or hidden and resumes it once it is
    /// neither. Decided from the window flags, so a `Shown` that arrives while the window is
    /// still minimized does not resume. Occlusion alone does not suspend.
    fn update_suspension(&mut self, reason: &str) {
        let Some(s) = self.state.as_mut() else { return };
        let flags = WindowFlags::from(s.window.window_flags());
        let change = suspension_change(
            s.suspended_since.is_some(),
            flags.contains(WindowFlags::MINIMIZED),
            flags.contains(WindowFlags::HIDDEN),
        );
        match (change, s.suspended_since) {
            (Some(Suspension::Suspend), _) => {
                s.suspended_since = Some(Instant::now());
                s.suspended_ticks = 0;
                s.resumed_at = None;
                s.resumed_import_at = None;
                self.log.line(format_args!(
                    "frame work suspended reason={reason} phase={:?} imported={} presented={}",
                    self.phase, s.imported, s.presented
                ));
                if matches!(self.phase, Phase::Warmup | Phase::Sampling) {
                    self.end_sustained(Some(format!("window {reason} during {:?}", self.phase)));
                }
            },
            (Some(Suspension::Resume), Some(since)) => {
                let now = Instant::now();
                s.suspended_since = None;
                s.resumed_at = Some(now);
                s.resumed_import_at = Some(now);
                self.log.line(format_args!(
                    "frame work resumed reason={reason} suspended_ms={} idle_ticks={} imported={} presented={}",
                    (now - since).as_millis(),
                    s.suspended_ticks,
                    s.imported,
                    s.presented
                ));
                // The suspended span is not a steady-state frame interval.
                if let Some(t) = self.timings.as_mut() {
                    t.break_intervals();
                }
                // Size changes while suspended were deferred.
                if self.phase != Phase::Closing {
                    let size = pixel_size(&s.window);
                    self.resized(size);
                }
            },
            _ => {},
        }
    }

    fn suspended(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.suspended_since.is_some())
    }

    /// Handles drained CEF events. Returns false when the run was closed because the game's
    /// main frame left the loopback origin.
    fn handle_web_events(
        &mut self,
        events: Vec<CefSurfaceEvent>,
        mouse: &sdl3::mouse::MouseUtil,
    ) -> bool {
        let Some(s) = self.state.as_mut() else {
            return false;
        };
        // Game mode only (`None` keeps the fixture's event handling unchanged).
        let origin = self.server.as_ref().map(|server| server.origin.clone());
        let mut left_origin = None;
        let mut click_actions = Vec::new();
        let mut key_actions = Vec::new();
        let mut wheel_actions = Vec::new();
        let mut startup_failures = Vec::new();
        for event in events {
            if let CefSurfaceEvent::Navigation(event) = event {
                if let (Some(boot), Some(origin)) = (self.bootstrap.as_mut(), origin.as_deref())
                    && boot.skips(&event, origin, self.game_loaded_at.is_some())
                {
                    self.log
                        .line(format_args!("page bootstrap event skipped {event:?}"));
                    continue;
                }
                match &event {
                    NavigationEvent::LoadEnd { http_status, url } if origin.is_some() => {
                        let origin = origin.as_deref().unwrap_or_default();
                        let ok = load_status_ok(*http_status) && on_origin(origin, url);
                        self.log.line(format_args!(
                            "load-end status={http_status} url={url} ok={ok}"
                        ));
                        if !ok {
                            self.log
                                .line(format_args!("FAIL game load-end not accepted"));
                            self.failures += 1;
                            startup_failures.push(StartupFailure::LoadRejected);
                        } else if self.game_loaded_at.is_none() {
                            if let Some(mcp) = self.mcp.as_mut() {
                                mcp.url.get_or_insert_with(|| url.clone());
                            }
                            s.load_ended = true;
                            self.game_loaded_at = Some(Instant::now());
                            startup_line(&self.log, &mut self.startup, Milestone::LoadEnd);
                            // A frame painted after load-end releases the startup screen.
                            // welding's resize re-sends the current size with an invalidate
                            // (`was_resized` + `Invalidate(PET_VIEW)`), so a page that is
                            // already static still paints once.
                            if self.gate.as_mut().is_some_and(StartupGate::load_end) {
                                let size = PhysicalSize::new(
                                    s.surface_config.width,
                                    s.surface_config.height,
                                );
                                let requested = s.producer.resize(size);
                                self.log.line(format_args!(
                                    "startup fresh-frame requested ok={}",
                                    requested.is_ok()
                                ));
                            }
                            if let Some(delay) = self.options.auto_close
                                && !self.options.player
                            {
                                self.log.line(format_args!(
                                    "auto-close scheduled delay_ms={}",
                                    delay.as_millis()
                                ));
                            }
                        }
                    },
                    NavigationEvent::LoadEnd { http_status, .. } => {
                        s.load_ended = true;
                        self.log.line(format_args!("load-end status={http_status}"));
                        startup_line(&self.log, &mut self.startup, Milestone::LoadEnd);
                    },
                    // welding's main-frame veto (`main_frame_policy`) cancels off-origin
                    // navigations before they start, so this is a backstop: a main frame that
                    // still commits off the loopback origin fails the run and closes the
                    // browser. History API and fragment routes stay on origin.
                    NavigationEvent::AddressChanged { url } if origin.is_some() => {
                        if on_origin(origin.as_deref().unwrap_or_default(), url) {
                            self.log.line(format_args!("address {url}"));
                            if let Some(mcp) = self.mcp.as_mut() {
                                mcp.url = Some(url.clone());
                            }
                        } else {
                            left_origin = Some(url.clone());
                        }
                    },
                    NavigationEvent::LoadStart { url } if origin.is_some() => {
                        self.log.line(format_args!("load-start url={url}"));
                        self.game_start_generation
                            .get_or_insert(self.last_generation);
                        startup_line(&self.log, &mut self.startup, Milestone::LoadStart);
                    },
                    // Fixture: no per-event line, only the first startup milestone.
                    NavigationEvent::LoadStart { .. } => {
                        startup_line(&self.log, &mut self.startup, Milestone::LoadStart);
                    },
                    // welding already cancelled both (one surface per producer, no popups; the
                    // main-frame veto). Game mode: an external link may go to the system
                    // browser (`external_link.rs`); the URL is logged as scheme + host only.
                    NavigationEvent::NewWindowRequested { url, user_gesture }
                    | NavigationEvent::NavigationBlocked {
                        url, user_gesture, ..
                    } if origin.is_some() => {
                        let via = if matches!(event, NavigationEvent::NewWindowRequested { .. }) {
                            "popup"
                        } else {
                            "navigation"
                        };
                        let line = self.external_links.handle(
                            origin.as_deref().unwrap_or_default(),
                            via,
                            url,
                            *user_gesture,
                            open::that_detached,
                        );
                        self.log.line(format_args!("{line}"));
                    },
                    NavigationEvent::NewWindowRequested { url, user_gesture } => {
                        self.log.line(format_args!(
                            "popup refused url={url} user_gesture={user_gesture}"
                        ));
                    },
                    NavigationEvent::ConsoleMessage {
                        level,
                        message,
                        source,
                        line,
                    } if origin.is_some() => {
                        if let Some(text) = self.console.line(*level, message, source, *line) {
                            self.log.line(format_args!("{text}"));
                        }
                        if let Some(mcp) = self.mcp.as_mut() {
                            mcp.console(level, message, source, *line);
                        }
                        // `--roves-test-click-ms` only: the fixture's click marker.
                        if let Some(click) = self.click_test.as_mut()
                            && is_click_marker(message)
                            && on_origin(origin.as_deref().unwrap_or_default(), source)
                        {
                            self.log.line(format_args!("test click marker received"));
                            click_actions.push(click.marker());
                        }
                        // `--roves-test-key-ms` only: the fixture's keydown/keyup markers.
                        if let Some(key) = self.key_test.as_mut()
                            && let Some(marker) = key_marker(message)
                            && on_origin(origin.as_deref().unwrap_or_default(), source)
                        {
                            self.log
                                .line(format_args!("test key marker received kind={marker:?}"));
                            key_actions.push(key.marker(marker));
                        }
                        // `--roves-test-wheel-ms` only: the fixture's wheel marker.
                        if let Some(wheel) = self.wheel_test.as_mut()
                            && let Some(marker) = wheel_marker(message)
                            && on_origin(origin.as_deref().unwrap_or_default(), source)
                        {
                            match marker {
                                WheelMarker::Expected {
                                    delta_y,
                                    delta_mode,
                                } => self.log.line(format_args!(
                                    "test wheel marker received delta_y={delta_y} delta_mode={delta_mode}"
                                )),
                                WheelMarker::Unexpected(why) => self
                                    .log
                                    .line(format_args!("test wheel marker received unexpected={why:?}")),
                            }
                            wheel_actions.push(wheel.marker(marker));
                        }
                    },
                    NavigationEvent::TitleChanged { title } if origin.is_some() => {
                        self.log.line(format_args!("title {title}"));
                        if let Some(mcp) = self.mcp.as_mut() {
                            mcp.title = Some(title.clone());
                        }
                        let _ = s.window.set_title(title);
                    },
                    NavigationEvent::LoadError {
                        error_code,
                        error_text,
                        ..
                    } => {
                        self.log
                            .line(format_args!("FAIL load-error {error_code} {error_text}"));
                        self.failures += 1;
                        startup_failures.push(StartupFailure::LoadError);
                    },
                    NavigationEvent::TitleChanged { title } => {
                        self.log.line(format_args!("title {title}"));
                    },
                    NavigationEvent::ContentProcessTerminated { status, .. } => {
                        self.log
                            .line(format_args!("FAIL renderer terminated {status:?}"));
                        self.failures += 1;
                        startup_failures.push(StartupFailure::RendererTerminated);
                    },
                    _ => {},
                }
            }
        }
        for failure in startup_failures {
            self.fail_startup(failure);
        }
        for action in click_actions {
            self.click_test_action(action);
        }
        for action in key_actions {
            self.key_test_action(action);
        }
        for action in wheel_actions {
            self.wheel_test_action(action);
        }
        if let Some(url) = left_origin {
            self.fail(format_args!(
                "game main frame left the loopback origin: {url}"
            ));
            self.close();
            return false;
        }
        let Some(s) = self.state.as_mut() else {
            return false;
        };
        if let Some(input) = s.input.as_mut()
            && let Some(line) = input.per_frame(&mut s.producer, mouse, s.imported > 0)
        {
            self.log.line(format_args!("{line}"));
        }
        true
    }

    fn resized(&mut self, size: PhysicalSize<u32>) {
        let Some(s) = self.state.as_mut() else { return };
        let (width, height) = (size.width.max(1), size.height.max(1));
        if (width, height) == (s.surface_config.width, s.surface_config.height) {
            return;
        }
        s.surface_config.width = width;
        s.surface_config.height = height;
        s.surface.configure(&s.host.device, &s.surface_config);
        s.imported_at_size = 0;
        // The reconfiguration is not steady-state presentation work.
        if let Some(t) = self.timings.as_mut() {
            t.break_intervals();
        }
        if let Err(error) = s.producer.resize(size) {
            self.log.line(format_args!("FAIL producer resize: {error}"));
            self.failures += 1;
        }
        self.log
            .line(format_args!("resized surface={width}x{height}"));
        if matches!(self.phase, Phase::Warmup | Phase::Sampling) {
            self.end_sustained(Some(format!(
                "window resized to {width}x{height} during {:?}",
                self.phase
            )));
        }
    }

    /// Emits and clears the smoke-phase summary when `--roves-timings` is set. Called before
    /// the verification readback, so readback time is never part of a sample.
    fn report_smoke_timings(&mut self, phase: &str) {
        if self.options.timings {
            self.report_timings(phase);
        }
    }

    /// `--roves-sustained`: entered after the initial check passed (its readback is over and
    /// its frames were reported or not recorded). Warmup frames are not recorded.
    fn start_warmup(&mut self) {
        let s = self.state.as_ref().expect("state");
        let size = (s.surface_config.width, s.surface_config.height);
        if size != (INITIAL_SIZE.width, INITIAL_SIZE.height) {
            self.fail(format_args!(
                "sustained not started: surface {}x{} != fixed {}x{}",
                size.0, size.1, INITIAL_SIZE.width, INITIAL_SIZE.height
            ));
            self.close();
            return;
        }
        let now = Instant::now();
        self.phase = Phase::Warmup;
        self.phase_since = Some(now);
        self.last_import = Some(now);
        if let Some(t) = self.timings.as_mut() {
            t.break_intervals();
        }
        if let Some(ring) = self.stall.as_mut() {
            ring.start(now);
        }
        let _ = WARMUP_ORIGIN.set(now);
        self.log.line(format_args!(
            "sustained warmup start size={}x{} warmup_ms={} sample_ms={} stall_ms={}",
            size.0,
            size.1,
            self.options.warmup.as_millis(),
            self.options.sample.as_millis(),
            SUSTAINED_STALL.as_millis()
        ));
    }

    /// End of each warmup/sample redraw: stall check, warmup -> sample, sample end.
    fn sustained_step(&mut self) {
        let now = Instant::now();
        let since = self.phase_since.unwrap_or(now);
        let gap = self.last_import.map_or(Duration::ZERO, |t| now - t);
        if gap > SUSTAINED_STALL {
            self.repoll_mailbox_at_stall(now);
            self.end_sustained(Some(format!(
                "imports stalled: no producer frame for {} ms during {:?}",
                gap.as_millis(),
                self.phase
            )));
            return;
        }
        match self.phase {
            Phase::Warmup if now - since >= self.options.warmup => {
                self.phase = Phase::Sampling;
                self.phase_since = Some(now);
                if let Some(t) = self.timings.as_mut() {
                    t.break_intervals();
                }
                self.log.line(format_args!(
                    "sustained sample start warmup_span_ms={}",
                    (now - since).as_millis()
                ));
            },
            Phase::Sampling if now - since >= self.options.sample => self.end_sustained(None),
            _ => {},
        }
    }

    /// Observational only, after the stall check tripped and before the run is rejected:
    /// takes welding's single-slot mailbox once more with `acquire_native_frame`, which does
    /// not import (no grafting open, no 1-texel copy, no `device.poll(Wait)`), logs the
    /// waiting frame's generation and drops it, closing its owned shared handle. The run is
    /// closing anyway, so consuming the slot changes nothing that is measured.
    fn repoll_mailbox_at_stall(&mut self, stall_checked: Instant) {
        let Some(s) = self.state.as_mut() else { return };
        let start = Instant::now();
        let frame = s.producer.acquire_native_frame();
        let took_us = start.elapsed().as_micros();
        let since_last_poll_us = start.saturating_duration_since(stall_checked).as_micros();
        match frame {
            Some(frame) => {
                let generation = frame.generation();
                let delta = self
                    .last_generation
                    .map_or_else(|| "-".to_owned(), |last| generation.wrapping_sub(last).to_string());
                self.log.line(format_args!(
                    "stall-mailbox frame generation={generation} last_imported_generation={} delta={delta} size={}x{} took_us={took_us} after_stall_check_us={since_last_poll_us} (observational, not imported)",
                    self.last_generation.map_or_else(|| "-".to_owned(), |g| g.to_string()),
                    frame.size().width,
                    frame.size().height,
                ));
                drop(frame);
            },
            None => self.log.line(format_args!(
                "stall-mailbox empty last_imported_generation={} took_us={took_us} after_stall_check_us={since_last_poll_us} (observational)",
                self.last_generation.map_or_else(|| "-".to_owned(), |g| g.to_string()),
            )),
        }
    }

    /// Prints the stall ring (rejected sustained runs only): a header, a derived summary of
    /// the redraws after the last import, then one line per redraw, oldest first.
    fn dump_stall_trace(&self, problem: &str) {
        let Some(ring) = self.stall.as_ref() else {
            return;
        };
        let log = &self.log;
        log.line(format_args!(
            "stall-trace begin problem={problem:?} redraws={} capacity={} overwritten={} stall_ms={} upstream_log_records={} clock=monotonic-wall-us-sdl-thread",
            ring.len(),
            crate::stall::CAPACITY,
            ring.overwritten(),
            SUSTAINED_STALL.as_millis(),
            UPSTREAM_LOG_RECORDS.load(Ordering::Relaxed),
        ));
        if let Some(g) = ring.gap_summary() {
            log.line(format_args!(
                "stall-trace after-last-import import_in_ring={} last_generation={} redraws={} empty_polls={} poll_errors={} presented={} busy_us={} outside_us={} max_redraw_us={} max_redraw_start_us={} max_redraw_step={} max_redraw_step_us={}",
                g.import_in_ring,
                g.last_generation.map_or_else(|| "-".to_owned(), |v| v.to_string()),
                g.redraws,
                g.empty_polls,
                g.poll_errors,
                g.presented,
                g.busy_us,
                g.outside_us,
                g.max_redraw_us,
                g.max_redraw_start_us,
                g.max_redraw_step.map_or("-", Step::name),
                g.max_redraw_step_us,
            ));
        }
        let mut previous_end = None;
        for (i, r) in ring.iter().enumerate() {
            log.line(format_args!(
                "stall-trace {}",
                crate::stall::format_redraw(i, previous_end, r)
            ));
            previous_end = Some(r.start_us + u64::from(r.total_us));
        }
        log.line(format_args!("stall-trace end"));
    }

    /// Ends warmup/sample, reports the sample (labelled unhealthy on any problem) and closes.
    fn end_sustained(&mut self, mut problem: Option<String>) {
        let phase = self.phase;
        let span_ms = self.phase_since.map_or(0, |t| t.elapsed().as_millis());
        let totals = self
            .timings
            .as_ref()
            .map(Timings::totals)
            .unwrap_or_default();
        if problem.is_none() {
            problem = if phase != Phase::Sampling {
                Some(format!("ended in {phase:?}"))
            } else if totals.imported == 0 || totals.presented == 0 {
                Some("no imported or presented frame in the sample".into())
            } else if totals.dropped > 0 {
                Some(format!("{} samples dropped over the cap", totals.dropped))
            } else {
                None
            };
        }
        let healthy = problem.is_none();
        if phase == Phase::Sampling {
            self.report_timings(if healthy {
                "sustained"
            } else {
                "sustained-unhealthy"
            });
        }
        self.log.line(format_args!(
            "sustained result={} phase={phase:?} span_ms={span_ms} target_sample_ms={} warmup_ms={} presented={} imported={} superseded_producer_frames={} dropped_samples={}",
            if healthy { "healthy" } else { "unhealthy" },
            self.options.sample.as_millis(),
            self.options.warmup.as_millis(),
            totals.presented,
            totals.imported,
            totals.superseded,
            totals.dropped
        ));
        if let Some(problem) = problem.as_deref() {
            self.dump_stall_trace(problem);
        }
        match problem {
            Some(problem) => self.fail(format_args!("sustained sample rejected: {problem}")),
            None => self.sustained_done = true,
        }
        self.close();
    }

    /// Emits and clears the timings summary of the phase that just ended.
    fn report_timings(&mut self, phase: &str) {
        if let Some(t) = self.timings.as_mut() {
            let log = &self.log;
            t.report(phase, |line| {
                log.line(format_args!("{line}"));
            });
        }
    }

    /// Verification only, after the frame was presented: samples the composed frame and the
    /// imported CEF texture at four fractions of the viewport.
    fn check(&mut self, label: &str, composed: &wgpu::Texture) {
        let s = self.state.as_ref().expect("state");
        let Some(frame) = s.frame.as_ref() else {
            self.fail(format_args!("check {label}: no imported frame"));
            return;
        };
        let (w, h) = (s.surface_config.width, s.surface_config.height);
        if (frame.size.width, frame.size.height) != (w, h) {
            let (fw, fh) = (frame.size.width, frame.size.height);
            self.fail(format_args!(
                "check {label}: frame {fw}x{fh} != surface {w}x{h}"
            ));
            return;
        }
        let panel = BACKGROUND.map(|channel| (channel * 255.0) as u8);
        // (name, fx, fy, expected composed RGBA, expected CEF texel RGBA premultiplied)
        type Sample = (&'static str, f32, f32, [u8; 4], [u8; 4]);
        let samples: [Sample; 4] = [
            (
                "opaque-green",
                0.25,
                0.25,
                [0, 255, 0, 255],
                [0, 255, 0, 255],
            ),
            (
                "half-blue-over-background",
                0.75,
                0.25,
                [128, 0, 128, 255],
                [0, 0, 128, 128],
            ),
            (
                "transparent-over-background",
                0.25,
                0.75,
                [255, 0, 0, 255],
                [0, 0, 0, 0],
            ),
            (
                "transparent-over-scene-panel",
                0.75,
                0.75,
                panel,
                [0, 0, 0, 0],
            ),
        ];
        let points: Vec<(u32, u32)> = samples
            .iter()
            .map(|&(_, fx, fy, _, _)| ((w as f32 * fx) as u32, (h as f32 * fy) as u32))
            .collect();
        let composed_px = read_texels(&s.host, composed, &points);
        let frame_px = read_texels(&s.host, &frame.texture, &points);
        let (composed_px, frame_px) = match (composed_px, frame_px) {
            (Ok(c), Ok(f)) => (c, f),
            (c, f) => {
                let error = c.err().or(f.err()).unwrap_or_default();
                self.fail(format_args!(
                    "check {label}: verification readback failed: {error}"
                ));
                return;
            },
        };
        let frame_format = frame.format;
        self.checks += 1;
        self.log.line(format_args!(
            "verification-readback {label} size={w}x{h} points={} (not on the presentation path)",
            points.len()
        ));
        for (i, &(name, _, _, want_composed, want_frame)) in samples.iter().enumerate() {
            let got_composed = composed_px[i];
            let got_frame = match frame_format {
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                    let [b, g, r, a] = frame_px[i];
                    [r, g, b, a]
                },
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                    frame_px[i]
                },
                other => {
                    self.fail(format_args!(
                        "check {label}: unsupported CEF format {other:?}"
                    ));
                    return;
                },
            };
            let (x, y) = points[i];
            for (what, got, want) in [
                ("composed", got_composed, want_composed),
                ("cef-texture", got_frame, want_frame),
            ] {
                let ok = got
                    .iter()
                    .zip(want)
                    .all(|(g, w)| g.abs_diff(w) <= TOLERANCE);
                let verdict = if ok { "PASS" } else { "FAIL" };
                self.log.line(format_args!(
                    "{verdict} pixel {label} {what} {name} ({x},{y}) actual={got:?} expected={want:?}"
                ));
                if !ok {
                    self.failures += 1;
                }
            }
        }
    }

    fn close(&mut self) {
        if self.phase == Phase::Closing {
            return;
        }
        if self.phase == Phase::Game {
            // Emitted before closing starts, so the close sequence is never sampled.
            self.report_smoke_timings("game");
        }
        self.phase = Phase::Closing;
        self.closing_since = Some(Instant::now());
        self.stop_mcp();
        if let Some(s) = self.state.as_mut() {
            self.log.line(format_args!(
                "closing imported={} presented={} elapsed_ms={}",
                s.imported,
                s.presented,
                self.started.elapsed().as_millis()
            ));
            if let Some(input) = s.input.as_ref() {
                self.log.line(format_args!(
                    "game input forwarded={} rejected={} discarded_before_load_end={} console_messages={}",
                    input.forwarded, input.rejected, input.discarded, self.console.seen
                ));
            }
            if let Err(error) = s.producer.close() {
                self.log.line(format_args!("FAIL producer close: {error}"));
                self.failures += 1;
            }
        }
    }

    fn finish(&mut self) -> i32 {
        self.stop_mcp();
        // Steamworks first: no Steam call outlives CEF (README "Steam").
        shutdown_steam(&self.log, self.steam.take().as_deref());
        if let Some(s) = self.state.take() {
            // Drain the shared queue before dropping the device's last owners.
            let _ = s.host.device.poll(wgpu::PollType::wait_indefinitely());
            let State {
                producer,
                frame,
                composed_target,
                runtime,
                ..
            } = s;
            drop(frame);
            drop(producer);
            drop(composed_target);
            self.log.line(format_args!("producer and renderer dropped"));
            drop(runtime);
            self.log.line(format_args!("cef shutdown complete"));
        }
        // A startup failure leaves the runtime here: shut CEF down before the server.
        drop(self.runtime.take());
        stop_server(&self.log, self.server.take());
        if let Some(code) = self.exit_code {
            return code;
        }
        if self.options.game.is_some() {
            if let Some(cycle) = self.minimize_cycle
                && !cycle.finished()
            {
                self.log.line(format_args!(
                    "FAIL test minimize cycle incomplete state={cycle:?}"
                ));
                self.failures += 1;
            }
            if let Some(click) = self.click_test
                && !click.finished()
            {
                self.log
                    .line(format_args!("FAIL test click incomplete state={click:?}"));
                self.failures += 1;
            }
            if let Some(key) = self.key_test
                && !key.finished()
            {
                self.log
                    .line(format_args!("FAIL test key incomplete state={key:?}"));
                self.failures += 1;
            }
            if let Some(wheel) = self.wheel_test
                && !wheel.finished()
            {
                self.log
                    .line(format_args!("FAIL test wheel incomplete state={wheel:?}"));
                self.failures += 1;
            }
            if self.game_loaded_at.is_none() {
                self.log
                    .line(format_args!("FAIL the game never finished loading"));
                return 2;
            }
            return i32::from(self.failures > 0);
        }
        let expected = if self.options.keep_open || self.options.sustained {
            1
        } else {
            2
        };
        if self.checks < expected {
            self.log.line(format_args!(
                "FAIL only {} of {expected} checks ran",
                self.checks
            ));
            return 2;
        }
        if self.options.sustained && !self.sustained_done {
            self.log
                .line(format_args!("FAIL no healthy sustained sample completed"));
            return 2;
        }
        i32::from(self.failures > 0)
    }

    fn is_closed(&mut self) -> bool {
        if self.phase != Phase::Closing {
            return false;
        }
        if self.state.as_ref().is_none_or(|s| s.producer.is_closed()) {
            self.log.line(format_args!("browser closed"));
            return true;
        }
        if self
            .closing_since
            .is_some_and(|t| t.elapsed() > CLOSE_TIMEOUT)
        {
            self.fail(format_args!(
                "browser did not close within {CLOSE_TIMEOUT:?}"
            ));
            self.exit_code.get_or_insert(2);
            return true;
        }
        false
    }

    /// Whether page input is forwarded: after the accepted load-end, and in the packaged
    /// launch only once the startup screen gave way to the game.
    fn input_ready(&self) -> bool {
        if self.options.player {
            self.gate.is_some_and(StartupGate::released)
        } else {
            self.game_loaded_at.is_some()
        }
    }

    /// Game mode with `--roves-mcp` or the package marker: opens the local pipe and publishes
    /// the session record at launch, before the window and the browser exist (early requests
    /// wait in the queue or get "browser not ready").
    fn start_mcp(&mut self) {
        let Some(source) = self.options.mcp else {
            return;
        };
        let Some(origin) = self.server.as_ref().map(|server| server.origin.clone()) else {
            return;
        };
        match crate::mcp::start(&origin) {
            Ok(bridge) => {
                // The token stays in the session record only; it is never logged.
                self.log.line(format_args!(
                    "mcp enabled source={source:?} pipe={} record={}",
                    bridge.pipe,
                    bridge.record.display()
                ));
                self.mcp = Some(bridge);
            },
            Err(error) => self.log.line(format_args!(
                "mcp unavailable source={source:?} error={error}"
            )),
        }
    }

    /// Removes the session record (discovery stops finding this game); pending requests are
    /// answered with an error by the pipe thread.
    fn stop_mcp(&mut self) {
        if let Some(bridge) = self.mcp.take() {
            let calls = bridge.calls;
            drop(bridge);
            self.log
                .line(format_args!("mcp session removed calls={calls}"));
        }
    }

    /// One loop turn of welding's DevTools queue (bounded, so it is drained every turn while
    /// the channel is on): page marker replies, then the bridge's; once the marker
    /// registration settled (registered or not), the game URL is loaded.
    fn devtools_step(&mut self) {
        let (Some(s), Some(init)) = (self.state.as_mut(), self.page_init.as_mut()) else {
            return;
        };
        while let Some(message) = s.producer.poll_devtools_message() {
            let Ok(message) = serde_json::from_str::<Value>(&message) else {
                continue;
            };
            if init.handle(&message) {
                continue;
            }
            if let Some(bridge) = self.mcp.as_mut() {
                bridge.devtools_message(&mut s.producer, &message);
            }
        }
        if let Some(bridge) = self.mcp.as_mut() {
            bridge.expire(&mut s.producer);
        }
        if let Some(boot) = self.bootstrap.as_mut()
            && boot.game_url.is_some()
            && let Some(outcome) = init.step(&mut s.producer)
        {
            let url = boot.game_url.take().unwrap_or_default();
            let mcp = self.mcp.is_some();
            match outcome {
                Ok(identifier) => self.log.line(format_args!(
                    "page markers registered identifier={identifier} mcp={mcp}"
                )),
                Err(error) => self.log.line(format_args!(
                    "page markers unavailable mcp={mcp} error={error}"
                )),
            }
            match s.producer.navigate_to_url(&url) {
                Ok(()) => self.log.line(format_args!("game navigation url={url}")),
                Err(error) => {
                    self.log
                        .line(format_args!("FAIL game navigation url={url} error={error}"));
                    self.failures += 1;
                    self.fail_startup(StartupFailure::LoadError);
                },
            }
        }
    }

    /// One loop turn of Steamworks callback dispatch (`SteamAPI_RunCallbacks`).
    fn steam_step(&self) {
        if let Some(steam) = self.steam.as_deref() {
            steam.with(|api| api.run_callbacks());
        }
    }

    /// The host API asked to close (`exit`, `clear_content_cache`): the OS close path.
    fn game_exit_step(&mut self) {
        let Some(cmd) = self.game_exit.get() else {
            return;
        };
        if self.phase == Phase::Closing || self.state.is_none() {
            return;
        }
        self.log
            .line(format_args!("close requested by the game command={cmd}"));
        self.close();
    }

    /// One loop turn of the MCP bridge: a few queued requests.
    fn mcp_step(&mut self) {
        let Some(bridge) = self.mcp.as_mut() else {
            return;
        };
        for (call, reply) in bridge.take(crate::mcp::CALLS_PER_TURN) {
            let method = call.method();
            let result = match call {
                Call::State => Ok(self.mcp_state()),
                Call::Evaluate(_) | Call::Screenshot(_)
                    if self
                        .bootstrap
                        .as_ref()
                        .is_some_and(|b| b.game_url.is_some()) =>
                {
                    Err("the game page is not loaded yet".to_owned())
                },
                Call::Evaluate(expression) => {
                    if let (Some(bridge), Some(s)) = (self.mcp.as_mut(), self.state.as_mut()) {
                        bridge.evaluate(&mut s.producer, &expression, reply);
                        self.log
                            .line(format_args!("mcp call method={method} queued=true"));
                        continue;
                    }
                    Err("the browser is not ready yet".to_owned())
                },
                Call::Screenshot(shot) => {
                    if let (Some(bridge), Some(s)) = (self.mcp.as_mut(), self.state.as_mut()) {
                        let (width, height) = s.window.size();
                        let view = (f64::from(width), f64::from(height));
                        let dpr = f64::from(s.window.display_scale());
                        bridge.screenshot(&mut s.producer, &shot, view, dpr, reply);
                        self.log
                            .line(format_args!("mcp call method={method} queued=true"));
                        continue;
                    }
                    Err("the browser is not ready yet".to_owned())
                },
                Call::Click { x, y } => match self.state.as_ref() {
                    Some(s) => crate::mcp::click_events(&s.window, x, y)
                        .and_then(|events| self.mcp_input(&events)),
                    None => Err("the window is not ready yet".into()),
                },
                Call::PressKey(key) => {
                    let window_id = self.state.as_ref().map_or(0, |s| s.window.id());
                    self.mcp_input(&crate::mcp::key_events(window_id, key))
                },
                Call::TypeText(text) => {
                    let window_id = self.state.as_ref().map_or(0, |s| s.window.id());
                    self.mcp_input(&[crate::mcp::text_event(window_id, text)])
                },
            };
            self.log.line(format_args!(
                "mcp call method={method} ok={}",
                result.is_ok()
            ));
            reply.send(result);
        }
    }

    /// Host-side state for `state`: no page script runs for it.
    fn mcp_state(&self) -> Value {
        let s = self.state.as_ref();
        let mcp = self.mcp.as_ref();
        let startup = match self.gate {
            None => "none",
            Some(gate) if gate.failure().is_some() => "failed",
            Some(gate) if gate.released() => "released",
            Some(_) => "loading",
        };
        json!({
            "pid": std::process::id(),
            "launch": if self.options.player { "package" } else { "game-dir" },
            "origin": self.server.as_ref().map(|server| server.origin.clone()),
            "url": mcp.and_then(|m| m.url.clone()),
            "title": mcp.and_then(|m| m.title.clone()),
            "loaded": self.game_loaded_at.is_some(),
            // `isMcpEnabled()` marker: "pending" until registered, then whether the game
            // document received it.
            "page_capability": match self.page_init.as_ref().and_then(|p| p.registered) {
                None => "pending",
                Some(true) => "registered",
                Some(false) => "unavailable",
            },
            "startup": startup,
            "input_ready": self.input_ready(),
            "window": s.map(|s| {
                let (width, height) = s.window.size();
                let (pixel_width, pixel_height) = s.window.size_in_pixels();
                json!({
                    "width": width,
                    "height": height,
                    "pixel_width": pixel_width,
                    "pixel_height": pixel_height,
                    "scale": s.window.display_scale(),
                    "suspended": s.suspended_since.is_some(),
                })
            }),
            "frames": s.map(|s| json!({ "imported": s.imported, "presented": s.presented })),
            "input": s.and_then(|s| s.input.as_ref()).map(|input| json!({
                "forwarded": input.forwarded,
                "rejected": input.rejected,
                "discarded": input.discarded,
            })),
            "console": {
                "seen": self.console.seen,
                "recent": mcp.map(crate::mcp::Bridge::recent_console),
            },
        })
    }

    /// Feeds bridge-generated SDL events to the game's `Input` (`mcp::forward`), exactly as
    /// the loop feeds real ones; refused until page input is forwarded for real input too
    /// (`input_ready`).
    fn mcp_input(&mut self, events: &[Event]) -> Result<Value, String> {
        if !self.input_ready() {
            return Err(if self.options.player {
                "input is not ready: the game is not on screen yet (startup screen not released)"
            } else {
                "input is not ready: the game has not finished loading"
            }
            .into());
        }
        let Some(s) = self.state.as_mut() else {
            return Err("the window is not ready yet".into());
        };
        let Some(input) = s.input.as_mut() else {
            return Err("no game input".into());
        };
        let log = &mut self.log;
        crate::mcp::forward(input, &mut s.producer, &s.window, events, |line| {
            log.line(format_args!("{line}"))
        })
    }

    fn window_event(&mut self, event: Event) {
        // Set when the accepted game load-end was handled; the test hooks only push after it.
        // Packaged player: page input starts with the game on screen (startup released), as
        // Chromium holds input until the first frame of new content.
        let game_loaded = self.input_ready();
        let Some(s) = self.state.as_mut() else { return };
        // `--roves-test-click-ms` only: the pushed button events, recognized by their mouse id.
        let synthetic = match &event {
            Event::MouseButtonDown { which, .. } if self.click_test.is_some() => {
                (*which == TEST_CLICK_MOUSE_ID).then_some(true)
            },
            Event::MouseButtonUp { which, .. } if self.click_test.is_some() => {
                (*which == TEST_CLICK_MOUSE_ID).then_some(false)
            },
            _ => None,
        };
        // `--roves-test-key-ms` only: the pushed key events, recognized by their keyboard id.
        let synthetic_key = match &event {
            Event::KeyDown { which, .. } if self.key_test.is_some() => {
                (*which == TEST_KEY_KEYBOARD_ID).then_some(true)
            },
            Event::KeyUp { which, .. } if self.key_test.is_some() => {
                (*which == TEST_KEY_KEYBOARD_ID).then_some(false)
            },
            _ => None,
        };
        // `--roves-test-wheel-ms` only: the pushed wheel event, recognized by its mouse id.
        let synthetic_wheel = self.wheel_test.is_some()
            && matches!(&event, Event::MouseWheel { which, .. } if *which == TEST_WHEEL_MOUSE_ID);
        let mut click_forwarded = None;
        let mut key_forwarded = None;
        let mut key_codes = None;
        let mut wheel_forwarded = None;
        if self.phase == Phase::Game
            && let Some(input) = s.input.as_mut()
        {
            let forwarded = input.forwarded;
            if synthetic_key.is_some() {
                input.last_key = None;
            }
            if synthetic_wheel {
                input.last_wheel = None;
            }
            if let Some(line) = input.window_event(&mut s.producer, &s.window, &event, game_loaded)
            {
                self.log.line(format_args!("{line}"));
            }
            click_forwarded = synthetic.map(|down| (down, input.forwarded > forwarded));
            key_forwarded = synthetic_key.map(|down| (down, input.forwarded > forwarded));
            if key_forwarded.is_some() {
                key_codes = input.last_key.take();
            }
            if synthetic_wheel {
                wheel_forwarded = Some(WheelInput {
                    ok: input.forwarded > forwarded,
                    delta: input.last_wheel.take(),
                });
            }
        }
        if let Some(input) = wheel_forwarded {
            let delta = input.delta.map_or_else(
                || "none".to_owned(),
                |(x, y)| format!("delta_x={x} delta_y={y}"),
            );
            self.log
                .line(format_args!("test wheel forwarded {delta} ok={}", input.ok));
            if let Some(wheel) = self.wheel_test.as_mut() {
                let action = wheel.input(input);
                self.wheel_test_action(action);
            }
        }
        if let Some((down, ok)) = key_forwarded {
            let event = if down { "down" } else { "up" };
            // Separate line: the CI gate anchors `test key forwarded ... ok=true$`.
            let codes = key_codes.map_or_else(|| "none".to_owned(), |c| c.to_string());
            self.log
                .line(format_args!("test key codes event={event} {codes}"));
            self.log
                .line(format_args!("test key forwarded event={event} ok={ok}"));
            let input = if down {
                KeyInput::Down { ok }
            } else {
                KeyInput::Up { ok }
            };
            if let Some(key) = self.key_test.as_mut() {
                let action = key.input(input);
                self.key_test_action(action);
            }
        }
        if let Some((down, ok)) = click_forwarded {
            self.log.line(format_args!(
                "test click forwarded event={} ok={ok}",
                if down { "down" } else { "up" }
            ));
            let input = if down {
                ClickInput::Down { ok }
            } else {
                ClickInput::Up { ok }
            };
            if let Some(click) = self.click_test.as_mut() {
                let action = click.input(input);
                self.click_test_action(action);
            }
        }
        let Some(s) = self.state.as_mut() else { return };
        match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: WindowEvent::CloseRequested,
                ..
            } => {
                self.log.line(format_args!("close requested by the OS"));
                self.close();
            },
            // Deferred while suspended: the resume re-reads the size.
            Event::Window {
                win_event: WindowEvent::PixelSizeChanged(_, _) | WindowEvent::DisplayChanged(_),
                ..
            } if s.suspended_since.is_none() => {
                let size = pixel_size(&s.window);
                self.resized(size);
            },
            Event::Window {
                win_event: WindowEvent::Minimized,
                ..
            } => self.update_suspension("minimized"),
            Event::Window {
                win_event: WindowEvent::Hidden,
                ..
            } => self.update_suspension("hidden"),
            Event::Window {
                win_event: WindowEvent::Restored,
                ..
            } => self.update_suspension("restored"),
            Event::Window {
                win_event: WindowEvent::Maximized,
                ..
            } => self.update_suspension("maximized"),
            Event::Window {
                win_event: WindowEvent::Shown,
                ..
            } => self.update_suspension("shown"),
            _ => {},
        }
    }
}

fn drain_web_events(s: &mut State) -> Vec<CefSurfaceEvent> {
    let mut events = Vec::new();
    while let Some(event) = s.producer.poll_web_event() {
        events.push(event);
    }
    events
}

fn pixel_size(window: &Window) -> PhysicalSize<u32> {
    let (width, height) = window.size_in_pixels();
    PhysicalSize::new(width, height)
}

/// Composes the current frame into a texture of the surface size. Returns `None` while there
/// is nothing to present yet.
fn compose(s: &mut State, mut timings: Option<&mut Timings>) -> Option<wgpu::Texture> {
    let start = now_if(&timings);
    let size = (s.surface_config.width, s.surface_config.height);
    let target = composed_target_of(&s.host, &mut s.composed_target, size);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let source = s.frame.as_ref().map(|f| BlitSource {
        view: &f.view,
        size: (f.size.width, f.size.height),
        at: (0, 0),
    });
    let commands = s.blit.encode(
        &s.host,
        &view,
        wgpu::LoadOp::Clear(s.background),
        source.as_slice(),
        true,
    );
    let submit = lap(&mut timings, Stage::Compose, start);
    s.host.queue.submit([commands]);
    lap(&mut timings, Stage::ComposeSubmit, submit);
    Some(target)
}

/// The composed target of `(width, height)`, recreated when the size changed.
fn composed_target_of(
    host: &HostWgpuContext,
    slot: &mut Option<wgpu::Texture>,
    (w, h): (u32, u32),
) -> wgpu::Texture {
    match slot {
        Some(t) if t.width() == w && t.height() == h => t.clone(),
        _ => {
            let texture = host.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("roves composed frame (wgpu mode)"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COMPOSED_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            *slot = Some(texture.clone());
            texture
        },
    }
}

/// Game mode: the startup (or, with `failed`, failure) screen into `target`: the dark
/// background with a centred column of the logo, the "Roves" wordmark (both 1:1) and, on
/// the startup screen only, the indeterminate loading bar animated from `splash.started`,
/// through the same premultiplied blit as the CEF frame (`splash_layout` drops what does not
/// fit). The failure screen shows the dimmed logo and wordmark and no bar. No CEF frame is
/// drawn.
fn splash_commands(
    host: &HostWgpuContext,
    blit: &Blit,
    target: &wgpu::Texture,
    background: wgpu::Color,
    splash: &Splash,
    failed: bool,
) -> wgpu::CommandBuffer {
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let (logo, wordmark) = if failed {
        (splash.failed.as_ref(), splash.wordmark_failed.as_ref())
    } else {
        (splash.logo.as_ref(), splash.wordmark.as_ref())
    };
    let [logo_at, wordmark_at, bar_at] = splash_layout(
        target.width(),
        target.height(),
        [
            logo.map(|_| (LOGO_PX, LOGO_PX)),
            wordmark.map(|_| WORDMARK_SIZE),
            (!failed).then_some(BAR_SIZE),
        ],
        SPLASH_GAP,
    );
    let mut sources = Vec::with_capacity(4);
    if let (Some(view), Some(at)) = (logo, logo_at) {
        sources.push(BlitSource {
            view,
            size: (LOGO_PX, LOGO_PX),
            at,
        });
    }
    if let (Some(view), Some(at)) = (wordmark, wordmark_at) {
        sources.push(BlitSource {
            view,
            size: WORDMARK_SIZE,
            at,
        });
    }
    if let Some((x, y)) = bar_at {
        sources.push(BlitSource {
            view: &splash.track,
            size: BAR_SIZE,
            at: (x, y),
        });
        let elapsed = splash.started.elapsed().as_millis();
        if let Some((offset, width)) = splash_indicator(BAR_SIZE.0, elapsed) {
            sources.push(BlitSource {
                view: &splash.indicator,
                size: (width, BAR_SIZE.1),
                at: (x + offset, y),
            });
        }
    }
    blit.encode(host, &view, wgpu::LoadOp::Clear(background), &sources, true)
}

/// GPU blit of the composed frame onto the window surface.
fn present(
    s: &mut State,
    composed: &wgpu::Texture,
    timings: Option<&mut Timings>,
    marks: Option<&mut Marks>,
) -> bool {
    present_to(
        &s.surface,
        &s.surface_config,
        &s.host,
        &s.blit,
        composed,
        timings,
        marks,
    )
}

fn present_to(
    surface: &wgpu::Surface<'static>,
    surface_config: &wgpu::SurfaceConfiguration,
    host: &HostWgpuContext,
    blit: &Blit,
    composed: &wgpu::Texture,
    mut timings: Option<&mut Timings>,
    mut marks: Option<&mut Marks>,
) -> bool {
    let start = now_if(&timings);
    let current = surface.get_current_texture();
    // Failed or outdated acquisitions are sampled too: they cost the frame.
    let since = lap(&mut timings, Stage::Acquire, start);
    mark_ref(&mut marks, Step::SurfaceAcquire);
    let output = match current {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            surface.configure(&host.device, surface_config);
            return false;
        },
        _ => return false,
    };
    let target = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let source = composed.create_view(&wgpu::TextureViewDescriptor::default());
    let commands = blit.encode(
        host,
        &target,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        &[BlitSource {
            view: &source,
            size: (composed.width(), composed.height()),
            at: (0, 0),
        }],
        false,
    );
    let since = lap(&mut timings, Stage::PresentEncode, since);
    host.queue.submit([commands]);
    let since = lap(&mut timings, Stage::Submit, since);
    mark_ref(&mut marks, Step::Submit);
    output.present();
    lap(&mut timings, Stage::Present, since);
    mark_ref(&mut marks, Step::Present);
    true
}

/// Stall ring: ends `step` now while the redraw is traced (warmup/sample of
/// `--roves-sustained`); otherwise does not read the clock.
fn mark(marks: &mut Option<Marks>, step: Step) {
    if let Some(m) = marks.as_mut() {
        m.mark(step, Instant::now());
    }
}

fn mark_ref(marks: &mut Option<&mut Marks>, step: Step) {
    if let Some(m) = marks.as_deref_mut() {
        m.mark(step, Instant::now());
    }
}

/// Reads the clock only while samples are recorded.
fn now_if(timings: &Option<&mut Timings>) -> Option<Instant> {
    timings.is_some().then(Instant::now)
}

/// Records `stage` since `since` while recording and returns the next lap start.
fn lap(
    timings: &mut Option<&mut Timings>,
    stage: Stage,
    since: Option<Instant>,
) -> Option<Instant> {
    match (timings.as_deref_mut(), since) {
        (Some(t), Some(since)) => Some(t.lap(stage, since)),
        _ => None,
    }
}

struct Blit {
    present: wgpu::RenderPipeline,
    over: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl Blit {
    fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("roves blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let pipeline = |format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("roves blit"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Blit {
            present: pipeline(surface_format, None),
            over: pipeline(
                COMPOSED_FORMAT,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            ),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("roves blit nearest"),
                ..Default::default()
            }),
        }
    }

    /// Encodes one render pass on `target`: `load`, then each source drawn in order at its
    /// `(width, height)` with its top-left corner at `at` (with premultiplied blending when
    /// `over`). A source whose size equals its texture is copied 1:1; the startup screen's
    /// 1x1 loading-bar textures are stretched to a flat rectangle. The caller submits it.
    fn encode(
        &self,
        host: &HostWgpuContext,
        target: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
        sources: &[BlitSource<'_>],
        over: bool,
    ) -> wgpu::CommandBuffer {
        let pipeline = if over { &self.over } else { &self.present };
        let bind_groups: Vec<_> = sources
            .iter()
            .map(|source| {
                host.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("roves blit"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();
        let mut encoder = host
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("roves blit"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("roves blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let tw = target.texture().width();
            let th = target.texture().height();
            pass.set_pipeline(pipeline);
            for (source, bind_group) in sources.iter().zip(&bind_groups) {
                let ((w, h), (x, y)) = (source.size, source.at);
                if x >= tw || y >= th || w == 0 || h == 0 {
                    continue;
                }
                // Viewport = source size, clipped to the target: texel-exact (never scaled)
                // whenever the size is the texture's own.
                pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
                pass.set_scissor_rect(x, y, w.min(tw - x), h.min(th - y));
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        encoder.finish()
    }
}

/// One `Blit::encode` draw: `view` sampled over a `size` rectangle whose top-left corner
/// is `at` in the target.
struct BlitSource<'a> {
    view: &'a wgpu::TextureView,
    size: (u32, u32),
    at: (u32, u32),
}

/// Verification-only readback of single texels (never on the presentation path).
fn read_texels(
    host: &HostWgpuContext,
    texture: &wgpu::Texture,
    points: &[(u32, u32)],
) -> Result<Vec<[u8; 4]>, String> {
    let row = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = host.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("roves verification readback"),
        size: row * points.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = host
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("roves verification readback"),
        });
    for (i, &(x, y)) in points.iter().enumerate() {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: row * i as u64,
                    bytes_per_row: Some(row as u32),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    host.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    host.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("poll: {e}"))?;
    rx.recv()
        .map_err(|e| format!("map callback: {e}"))?
        .map_err(|e| format!("map: {e}"))?;
    let texels = {
        let data = slice.get_mapped_range();
        (0..points.len())
            .map(|i| {
                let o = row as usize * i;
                [data[o], data[o + 1], data[o + 2], data[o + 3]]
            })
            .collect()
    };
    buffer.unmap();
    Ok(texels)
}
