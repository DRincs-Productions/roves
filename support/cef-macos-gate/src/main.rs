//! macOS feasibility gate: CEF 154 accelerated OSR (`OnAcceleratedPaint`, IOSurface) ->
//! welding Metal import -> wgpu 29 Metal device that also presents to an SDL3 window, with
//! Chromium's sandbox on. No CEF windowed mode and no software (`OnPaint`) path exist here.
//!
//! Two modes, chosen by the bundle (README "Packaged game"):
//! - fixture gate (no `Contents/Resources/game/index.html`): a `data:` page whose centre must
//!   match a known colour;
//! - packaged game (`src/player.rs`): an unmodified static game build packaged inside the
//!   `.app`, served from the shared loopback origin; load-end, first frame after it and the
//!   close path are checked.
//!
//! Exit codes (README "Verdicts"):
//! - 0: the mode's checks passed and the renderer was sandboxed.
//! - 2: no accelerated frame arrived before the timeout (e.g. a VM where Chromium disables
//!   GPU compositing), but nothing failed and the renderer was sandboxed.
//! - 6: packaged game only, before CEF starts: the persistent profile is held by another
//!   instance, or its stored origin port is taken by another program.
//! - 1: any failure.
//!
//! Every verdict line starts with `gate:` on stdout; CI checks those lines.

/// The Windows player's pure rules (origin check, load statuses, profile name and files,
/// origin-port parsing), compiled unchanged; std only. Most of it (Windows keyboard
/// translation, test sequences) is unused here.
#[cfg(target_os = "macos")]
#[path = "../../cef-netrender-probe/src/external_link.rs"]
mod external_link;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/game_rules.rs"]
mod game_rules;
/// The players' shared host API (`/__roves/<module>/<cmd>` on the loopback origin; Windows
/// player README "Host API"): `core`, `saves` and `steam` ("unavailable" without the `steam`
/// feature or without a running Steam client). The
/// `window.__ROVES__` page marker is registered before the game document (`session.rs`), so
/// `isAvailable()` of roves-api is `true` in the packaged game.
#[cfg(target_os = "macos")]
#[path = "../../cef-window-probe/src/host_api.rs"]
mod host_api;
/// The Windows player's project-icon header rules (std only).
#[cfg(target_os = "macos")]
#[path = "../../cef-netrender-probe/src/icon_rules.rs"]
mod icon_rules;
/// The players' shared SDL3 -> welding page input (key codes: `keys.rs`).
#[cfg(target_os = "macos")]
#[path = "../../cef-netrender-probe/src/input.rs"]
mod input;
/// Pure key-code rules of the macOS and Linux players (Chromium's table via `keycode`).
#[cfg(target_os = "macos")]
mod key_rules;
#[cfg(target_os = "macos")]
mod keys;
/// The Windows MCP bridge's pure helpers: only the page marker script and its bootstrap URL
/// are used here (no MCP bridge on macOS).
#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/mcp_rules.rs"]
mod mcp_rules;
/// The Windows player's page-marker registration over welding's DevTools channel.
#[cfg(target_os = "macos")]
#[path = "../../cef-netrender-probe/src/page_init.rs"]
mod page_init;
#[cfg(target_os = "macos")]
mod player;
/// Steamworks lifetime (start before CEF, callbacks per turn, shutdown before CEF), shared
/// with the Linux gate.
#[cfg(target_os = "macos")]
mod player_steam;
#[cfg(target_os = "macos")]
mod profile;
/// The stage-1 window probe's loopback server (axum + tower-http `ServeDir` on 127.0.0.1),
/// compiled unchanged as in the Windows player (`support/cef-netrender-probe/src/game.rs`).
#[cfg(target_os = "macos")]
#[path = "../../cef-window-probe/src/server.rs"]
mod server;
#[cfg(target_os = "macos")]
mod session;
/// The Windows player's startup-screen branding (Roves logo, MetalMania wordmark from
/// build.rs); its window-icon surface and failure-screen variants are unused here.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/splash.rs"]
mod splash;
/// Startup screen until the first game frame (README "Startup screen").
#[cfg(target_os = "macos")]
mod startup_screen;
/// `steam` feature: the Windows player's Steamworks client (README "Steam").
#[cfg(all(target_os = "macos", feature = "steam"))]
#[path = "../../cef-netrender-probe/src/steam.rs"]
mod steam;
/// The players' shared window icon (Roves logo, project `icon.png`).
#[cfg(target_os = "macos")]
#[path = "../../cef-netrender-probe/src/window_icon.rs"]
mod window_icon;

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(gate::run());
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("roves-cef-macos-gate is macOS-only");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
mod gate {
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use dpi::PhysicalSize;
    use welding::{
        CefRuntime, CefRuntimeConfig, CefSandboxMode, CefSurfaceConfig, CefSurfaceEvent,
        CefSurfaceProducer, HostWgpuContext, ImportedTexture, InteropBackend, NavigationEvent,
        macos_cef::{MacosCefConfig, MacosCefProducer},
    };

    /// Deterministic page: dodger blue everywhere, plus a 1 px corner element animated with
    /// requestAnimationFrame so CEF keeps painting (accelerated OSR only paints on change).
    const FIXTURE_URL: &str = "data:text/html,<!doctype html><html><body style=\"margin:0;\
        background:%231e90ff;width:100vw;height:100vh\"><div id=t style=\"position:fixed;\
        left:0;top:0;width:1px;height:1px\"></div><script>let n=0;(function f(){t.style.\
        background=(n++&1)?'%23000':'%23fff';requestAnimationFrame(f)})()</script></body>\
        </html>";
    /// Dodger blue (30, 144, 255) in the `Bgra8Unorm` byte order CEF delivers on macOS
    /// (Chromium's capturer requests BGRA shared images; README "Chromium reference").
    const EXPECTED_BGRA: [u8; 4] = [255, 144, 30, 255];
    const TOLERANCE: u8 = 8;
    /// Centre sample: 64 x 4 bytes is exactly one `COPY_BYTES_PER_ROW_ALIGNMENT` row.
    const SAMPLE: u32 = 64;
    const FIXTURE_WINDOW: (u32, u32) = (640, 400);
    const GAME_WINDOW: (u32, u32) = (1280, 720);
    /// Upper bound for CEF's `OnBeforeClose` after `close()`; a run that exceeds it fails.
    const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
    /// Exact message of welding e991684's `LifeSpanHandler::on_before_close`
    /// (`macos_cef/cef_backed.rs`), the only close-completion signal its macOS producer
    /// exposes (Windows has `is_closed`). Pinned revision; README "Close".
    const BEFORE_CLOSE_MESSAGE: &str = "CEF on_before_close";
    const WELDING_MACOS_TARGET: &str = "welding::macos_cef";

    /// Set by the logger when welding reports `OnBeforeClose`.
    static BROWSER_CLOSED: AtomicBool = AtomicBool::new(false);

    struct Logger;

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
                || metadata.target().starts_with(WELDING_MACOS_TARGET)
        }
        fn log(&self, record: &log::Record) {
            if record.level() <= log::Level::Info {
                println!(
                    "gate: log {} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            } else if record.target().starts_with(WELDING_MACOS_TARGET)
                // `as_str` is `Some` only for a literal message: no formatting cost.
                && record.args().as_str() == Some(BEFORE_CLOSE_MESSAGE)
            {
                BROWSER_CLOSED.store(true, Ordering::SeqCst);
            }
        }
        fn flush(&self) {}
    }

    pub fn run() -> i32 {
        // Debug only so welding's `on_before_close` record reaches the logger; other debug
        // records are dropped in `log` without being formatted.
        let _ = log::set_logger(&Logger).map(|()| log::set_max_level(log::LevelFilter::Debug));
        let code = match run_inner() {
            Ok(code) => code,
            Err(error) => {
                println!("gate: FAIL {error}");
                1
            },
        };
        println!("gate: exit code={code}");
        code
    }

    pub(crate) fn env_u64(name: &str) -> Option<u64> {
        std::env::var(name).ok().and_then(|v| v.parse().ok())
    }

    fn run_inner() -> Result<i32, String> {
        let timeout = Duration::from_secs(env_u64("ROVES_GATE_TIMEOUT_SECS").unwrap_or(60));
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        // <app>.app/Contents/MacOS/<exe> -> <app>.app/Contents
        let contents = exe
            .parent()
            .and_then(Path::parent)
            .ok_or("cannot resolve Contents")?
            .to_path_buf();
        let frameworks = contents.join("Frameworks");
        if !frameworks
            .join("Chromium Embedded Framework.framework")
            .exists()
        {
            return Err(format!(
                "no CEF framework under {}: run from the .app built by `bundle`",
                frameworks.display()
            ));
        }
        // The packaged game: an unmodified static build copied into the bundle's resources.
        let game_dir = contents.join("Resources").join("game");
        let game = game_dir.join("index.html").is_file().then_some(game_dir);

        // Our only switch (README "Saves and the player profile"); Chromium ignores it.
        let mut isolated = false;
        for arg in std::env::args().skip(1) {
            if arg == crate::profile::ISOLATED_PROFILE_SWITCH {
                isolated = true;
            } else if arg.starts_with("--roves-") {
                return Err(format!("unknown switch {arg}"));
            }
        }
        if isolated && game.is_none() {
            return Err(format!(
                "{} needs the packaged game (Contents/Resources/game/index.html)",
                crate::profile::ISOLATED_PROFILE_SWITCH
            ));
        }

        // Packaged game, ordinary launch: the persistent profile is opened (lock, stored
        // origin port) before the loopback server and CEF start, as on Windows. The lock
        // lives until this function returns, after CEF shut down.
        let profile = match game.as_ref().filter(|_| !isolated) {
            Some(_) => {
                let app = contents.parent().ok_or("cannot resolve the .app")?;
                let root = crate::profile::default_root(&exe, app)?;
                if crate::profile::translocated(app) {
                    println!(
                        "gate: WARN app is translocated by Gatekeeper; its profile follows \
                         this launch's path (move the app to keep saves)"
                    );
                }
                match crate::profile::PlayerProfile::open(&root) {
                    Ok(profile) => {
                        println!(
                            "gate: player profile root={} lock=acquired origin-port={}",
                            root.display(),
                            profile
                                .stored_port
                                .map_or_else(|| "new".to_owned(), |port| port.to_string())
                        );
                        Some(profile)
                    },
                    Err(crate::profile::ProfileError::Busy) => {
                        println!(
                            "gate: FAIL player profile busy root={} (another instance of this \
                             game is running)",
                            root.display()
                        );
                        return Ok(crate::profile::EXIT_CONTENTION);
                    },
                    Err(crate::profile::ProfileError::Failed(error)) => {
                        return Err(format!("player profile root={}: {error}", root.display()));
                    },
                }
            },
            None => None,
        };

        // Host API on the same origin (Windows player README "Host API"): saves in
        // `<profile root>/saves` with the persistent profile, unavailable when isolated;
        // `exit`/`clear_content_cache` end the drive loop like a window close.
        let game_exit = std::sync::Arc::new(std::sync::OnceLock::<&'static str>::new());
        // `steam` feature, packaged game: the run's one Steamworks client, before CEF starts
        // (README "Steam"), with libsteam_api.dylib and steam_appid.txt in Contents/MacOS;
        // shut down after the browser closed, before CEF shuts down.
        let steam = game
            .as_ref()
            .and_then(|_| exe.parent().and_then(crate::player_steam::start));
        let saves_dir = profile
            .as_ref()
            .map(|p| p.root.join(crate::game_rules::PLAYER_SAVES_DIR));
        let start_game_server = |dir: &std::path::Path, port: u16| {
            let exit = game_exit.clone();
            let (api, saves) = crate::host_api::HostApi::new(crate::host_api::HostConfig {
                saves_dir: saves_dir.clone(),
                engine_version: format!("roves-cef-player cef-{}", cef_version()),
                request_exit: std::sync::Arc::new(move |cmd| {
                    let _ = exit.set(cmd);
                }),
                steam: steam.clone(),
            });
            match saves {
                Ok(dir) => println!("gate: host api saves dir={}", dir.display()),
                Err(reason) => println!("gate: host api saves unavailable reason={reason}"),
            }
            crate::server::start_with(dir, port, |origin| api.router(origin))
        };

        // Packaged game: the loopback origin must be known before the browser is created,
        // and it outlives CEF so in-flight responses finish (Windows player order).
        let server = match game.as_deref() {
            Some(dir) => {
                let port = profile.as_ref().and_then(|p| p.stored_port).unwrap_or(0);
                let server = match start_game_server(dir, port) {
                    Ok(server) => server,
                    // The stored port is the saves' origin: another port would silently show
                    // an empty game, so a taken port ends the run.
                    Err(error)
                        if profile.is_some()
                            && matches!(
                                error.kind(),
                                std::io::ErrorKind::AddrInUse
                                    | std::io::ErrorKind::PermissionDenied
                            ) =>
                    {
                        println!("gate: FAIL game loopback port busy port={port} error={error}");
                        return Ok(crate::profile::EXIT_CONTENTION);
                    },
                    Err(error) => return Err(format!("game loopback server: {error}")),
                };
                println!(
                    "gate: mode=packaged-game dir={} origin={}",
                    dir.display(),
                    server.origin
                );
                // First launch of a persistent profile: keep this origin for every later one.
                if let Some(profile) = profile.as_ref().filter(|p| p.stored_port.is_none()) {
                    let stored = crate::game_rules::origin_port(&server.origin)
                        .ok_or_else(|| format!("unexpected origin {}", server.origin))
                        .and_then(|port| profile.store_port(port).map(|()| port));
                    match stored {
                        Ok(port) => println!("gate: player origin-port stored port={port}"),
                        Err(error) => {
                            stop_server(Some(server));
                            return Err(format!("player origin-port not stored: {error}"));
                        },
                    }
                }
                Some(server)
            },
            None => {
                println!("gate: mode=fixture");
                None
            },
        };

        // Persistent profile: `<root>/cef`, kept. Fixture gate and isolated packaged game: a
        // per-run directory under the temporary directory, removed at exit.
        let cache = match profile.as_ref() {
            Some(profile) => crate::profile::cef_dir(&profile.root),
            None => {
                std::env::temp_dir().join(format!("roves-cef-macos-gate-{}", std::process::id()))
            },
        };
        if game.is_some() {
            println!(
                "gate: player profile={} cache={}",
                if profile.is_some() {
                    "persistent"
                } else {
                    "isolated"
                },
                cache.display()
            );
        }
        let mut config = CefRuntimeConfig::new(&frameworks, CefSandboxMode::Sandboxed);
        config.cache_path = Some(cache.clone());
        // Unattended: Chromium's Safe Storage would otherwise ask for the login keychain
        // password and block CefInitialize (same switch as wgpu-weld's macOS demo).
        config
            .command_line_switches
            .push(("use-mock-keychain".to_owned(), None));
        // CEF first, SDL second: the order of wgpu-weld's macOS demo (CEF, then the windowing
        // library that creates NSApp).
        let runtime = match CefRuntime::initialize(config) {
            Ok(runtime) => runtime,
            Err(error) => {
                stop_server(server);
                return Err(format!("CEF initialize: {error}"));
            },
        };
        println!(
            "gate: cef initialized sandbox=Sandboxed cache={}",
            cache.display()
        );

        let code = {
            let result = (|| {
                let sdl = sdl3::init().map_err(|e| format!("SDL init: {e}"))?;
                let video = sdl.video().map_err(|e| format!("SDL video: {e}"))?;
                let mut events = sdl.event_pump().map_err(|e| format!("SDL events: {e}"))?;
                let (title, (w, h)) = if server.is_some() {
                    ("Roves", GAME_WINDOW)
                } else {
                    ("Roves CEF macOS gate", FIXTURE_WINDOW)
                };
                let mut builder = video.window(title, w, h);
                builder.high_pixel_density();
                if server.is_some() {
                    // Packaged game: resizable (surface and CEF view follow the pixel size).
                    builder.resizable();
                }
                let mut window = builder.build().map_err(|e| format!("SDL window: {e}"))?;
                if server.is_some() {
                    // README "Window": project icon.png, else the Roves logo.
                    let line = crate::window_icon::apply(
                        &mut window,
                        Some(contents.join("Resources").as_path()),
                    );
                    println!("gate: {line}");
                    // Committed text (SDL_EVENT_TEXT_INPUT) for the page, as on Windows.
                    video.text_input().start(&window);
                }
                println!(
                    "gate: sdl window pixels={:?} scale={}",
                    window.size_in_pixels(),
                    window.display_scale()
                );
                let code = {
                    let mut gpu = Gpu::new(&window)?;
                    if server.is_some() {
                        // README "Startup screen": until the first game frame.
                        gpu.show_startup_screen();
                    }
                    let result = match server.as_ref() {
                        Some(server) => {
                            let sdl_events = sdl.event().map_err(|e| format!("SDL event: {e}"))?;
                            crate::player::drive(
                                &runtime,
                                &mut window,
                                &mut events,
                                &sdl_events,
                                &sdl.mouse(),
                                &mut gpu,
                                &server.origin,
                                &cache.join(crate::profile::GAME_PROFILE_DIR),
                                timeout,
                                &game_exit,
                                steam.as_deref(),
                            )
                        },
                        None => drive_fixture(&runtime, &window, &mut events, &gpu, timeout),
                    };
                    drop(gpu);
                    match result {
                        Ok((code, producer)) => {
                            close_browser(&runtime, &mut events, producer, code)
                        },
                        Err(error) => {
                            println!("gate: FAIL {error}");
                            1
                        },
                    }
                };
                drop(window);
                Ok::<i32, String>(code)
            })();
            result.unwrap_or_else(|error| {
                println!("gate: FAIL {error}");
                1
            })
        };
        // Steamworks first: no Steam call outlives CEF (README "Steam").
        crate::player_steam::shutdown(steam.as_deref());
        drop(runtime);
        println!("gate: cef shutdown complete");
        let code = if stop_server(server) { code } else { 1 };
        match profile.as_ref() {
            // The saves: kept; the lock is released when `profile` drops on return.
            Some(profile) => println!("gate: player profile kept root={}", profile.root.display()),
            None => match std::fs::remove_dir_all(&cache) {
                Ok(()) => println!("gate: cache removed"),
                Err(error) => println!("gate: cache not removed: {error}"),
            },
        }
        println!("gate: shutdown complete exit={code}");
        Ok(code)
    }

    /// The CEF build the bindings were generated for (as in the Windows player).
    fn cef_version() -> String {
        std::ffi::CStr::from_bytes_with_nul(cef::sys::CEF_VERSION)
            .map_or_else(|_| "unknown".into(), |v| v.to_string_lossy().into_owned())
    }

    /// Stops the loopback server after CEF shut down. `false` when stopping failed.
    fn stop_server(server: Option<crate::server::Server>) -> bool {
        match server.map(crate::server::Server::stop) {
            None => true,
            Some(Ok(())) => {
                println!("gate: loopback stopped");
                true
            },
            Some(Err(error)) => {
                println!("gate: FAIL loopback stop: {error}");
                false
            },
        }
    }

    /// Closes the browser and pumps CEF until `OnBeforeClose` (Chromium closes every browser
    /// before quitting, `chrome/browser/app_controller_mac.mm` `tryToTerminateApplication`;
    /// CEF requires `OnBeforeClose` before `CefShutdown`). A close that does not complete
    /// within `CLOSE_TIMEOUT` fails the run.
    fn close_browser(
        runtime: &CefRuntime,
        events: &mut sdl3::EventPump,
        mut producer: MacosCefProducer,
        code: i32,
    ) -> i32 {
        let started = Instant::now();
        if let Err(error) = producer.close() {
            println!("gate: FAIL close: {error}");
            return 1;
        }
        while !BROWSER_CLOSED.load(Ordering::SeqCst) && started.elapsed() < CLOSE_TIMEOUT {
            for _ in events.poll_iter() {}
            runtime.do_message_loop_work();
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(producer);
        if BROWSER_CLOSED.load(Ordering::SeqCst) {
            println!(
                "gate: browser closed elapsed_ms={}",
                started.elapsed().as_millis()
            );
            code
        } else {
            println!(
                "gate: FAIL browser not closed within {} ms (no OnBeforeClose)",
                CLOSE_TIMEOUT.as_millis()
            );
            1
        }
    }

    /// wgpu Metal device presenting to the SDL3 window; welding imports into the same device.
    pub(crate) struct Gpu {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        blitter: wgpu::util::TextureBlitter,
        pub(crate) host: HostWgpuContext,
        pub(crate) failed: Arc<AtomicBool>,
        /// Packaged game: shown instead of black until the first game frame.
        startup: Option<crate::startup_screen::StartupScreen>,
    }

    impl Gpu {
        fn new(window: &sdl3::video::Window) -> Result<Gpu, String> {
            let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
            // welding's macOS import is Metal only (IOSurface -> MTLTexture -> grafting).
            instance_desc.backends = wgpu::Backends::METAL;
            let instance = wgpu::Instance::new(instance_desc);
            // SAFETY: SDL owns the NSWindow/NSView; `Gpu` (and so the surface) is dropped
            // before the caller drops the window, all on the main thread.
            let surface = unsafe {
                let target = wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window)
                    .map_err(|e| format!("SDL3 window handle: {e}"))?;
                instance.create_surface_unsafe(target)
            }
            .map_err(|e| format!("surface: {e}"))?;
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .map_err(|e| format!("Metal adapter: {e}"))?;
            let info = adapter.get_info();
            println!(
                "gate: adapter name={:?} backend={:?} type={:?} driver={:?}",
                info.name, info.backend, info.device_type, info.driver
            );
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("Roves CEF macOS gate"),
                    ..Default::default()
                }))
                .map_err(|e| format!("Metal device: {e}"))?;
            let host = HostWgpuContext::new(device, queue);
            if host.backend != InteropBackend::Metal {
                return Err(format!(
                    "interop backend {:?}, expected Metal",
                    host.backend
                ));
            }
            let failed = Arc::new(AtomicBool::new(false));
            host.device.on_uncaptured_error(Arc::new({
                let failed = failed.clone();
                move |error| {
                    failed.store(true, Ordering::Relaxed);
                    println!("gate: FAIL wgpu uncaptured error: {error}");
                }
            }));

            let (width, height) = window.size_in_pixels();
            let caps = surface.get_capabilities(&adapter);
            let format = [
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureFormat::Rgba8Unorm,
            ]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .unwrap_or(caps.formats[0]);
            let mut config = surface
                .get_default_config(&adapter, width.max(1), height.max(1))
                .ok_or("surface not supported by the adapter")?;
            config.format = format;
            config.present_mode = wgpu::PresentMode::AutoVsync;
            surface.configure(&host.device, &config);
            let blitter = wgpu::util::TextureBlitter::new(&host.device, format);
            Ok(Gpu {
                surface,
                config,
                blitter,
                host,
                failed,
                startup: None,
            })
        }

        pub(crate) fn size(&self) -> PhysicalSize<u32> {
            PhysicalSize::new(self.config.width, self.config.height)
        }

        /// Reconfigures the surface for a new window pixel size (packaged game, resizable).
        pub(crate) fn resize(&mut self, width: u32, height: u32) {
            let (width, height) = (width.max(1), height.max(1));
            if (width, height) == (self.config.width, self.config.height) {
                return;
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.host.device, &self.config);
            if let Some(startup) = self.startup.as_mut() {
                startup.resize(&self.host.device, &self.host.queue, width, height);
            }
        }

        /// Packaged game: the startup screen replaces black before the first game frame.
        pub(crate) fn show_startup_screen(&mut self) {
            let mut startup = crate::startup_screen::StartupScreen::new(self.config.format);
            startup.resize(
                &self.host.device,
                &self.host.queue,
                self.config.width,
                self.config.height,
            );
            self.startup = Some(startup);
        }

        /// The first game frame is shown: the startup screen's texture is released.
        pub(crate) fn hide_startup_screen(&mut self) {
            if self.startup.take().is_some() {
                println!("gate: startup screen released");
            }
        }

        /// Shows the latest imported frame in the SDL3 window (stretched) or, before the
        /// first frame, the startup screen (packaged game) or black (fixture). Returns whether a texture was presented (vsync then paces the
        /// loop).
        pub(crate) fn present(&self, frame: Option<&ImportedTexture>) -> bool {
            let output = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    self.surface.configure(&self.host.device, &self.config);
                    return false;
                },
                _ => return false,
            };
            let target = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder =
                self.host
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("gate present"),
                    });
            match (frame, self.startup.as_ref()) {
                (Some(frame), _) => {
                    self.blitter
                        .copy(&self.host.device, &mut encoder, &frame.view, &target)
                },
                (None, Some(startup)) => startup.draw(
                    &self.host.device,
                    &self.host.queue,
                    &mut encoder,
                    &self.blitter,
                    &target,
                ),
                (None, None) => {
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("gate clear"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                },
            }
            self.host.queue.submit([encoder.finish()]);
            output.present();
            true
        }
    }

    /// Copies a `w` x `h` region at (`x`, `y`) of the imported texture (grafting creates it
    /// with `COPY_SRC`) and returns it tightly packed, 4 bytes per texel.
    pub(crate) fn read_region(
        host: &HostWgpuContext,
        frame: &ImportedTexture,
        (x, y, w, h): (u32, u32, u32, u32),
    ) -> Result<Vec<u8>, String> {
        let row = w * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = host.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gate readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = host
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gate readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
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
        let pixels = slice
            .get_mapped_range()
            .chunks(padded as usize)
            .flat_map(|line| &line[..row as usize])
            .copied()
            .collect();
        buffer.unmap();
        Ok(pixels)
    }

    /// The whole frame read back, tightly packed `Bgra8Unorm`.
    pub(crate) fn frame_pixels(
        host: &HostWgpuContext,
        frame: &ImportedTexture,
    ) -> Result<Vec<u8>, String> {
        if frame.format != wgpu::TextureFormat::Bgra8Unorm {
            return Err(format!(
                "frame format {:?}, expected Bgra8Unorm",
                frame.format
            ));
        }
        read_region(host, frame, (0, 0, frame.size.width, frame.size.height))
    }

    pub(crate) fn distinct_colours(pixels: &[u8]) -> usize {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| u32::from_le_bytes(*px))
            .collect::<HashSet<_>>()
            .len()
    }

    /// Writes a top-down 32-bit BMP (`BI_RGB`, byte order B G R X, i.e. `Bgra8Unorm`), the
    /// simplest lossless format every image viewer opens; evidence artifact only.
    pub(crate) fn write_bmp(
        path: &PathBuf,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> std::io::Result<()> {
        let image = u32::try_from(bgra.len()).map_err(std::io::Error::other)?;
        let mut out = Vec::with_capacity(54 + bgra.len());
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(54 + image).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(width as i32).to_le_bytes());
        // Negative height: rows top to bottom, as read back.
        out.extend_from_slice(&(-(height as i32)).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        out.extend_from_slice(&image.to_le_bytes());
        out.extend_from_slice(&2835i32.to_le_bytes());
        out.extend_from_slice(&2835i32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(bgra);
        std::fs::write(path, out)
    }

    fn drive_fixture(
        runtime: &CefRuntime,
        window: &sdl3::video::Window,
        events: &mut sdl3::EventPump,
        gpu: &Gpu,
        timeout: Duration,
    ) -> Result<(i32, MacosCefProducer), String> {
        let size = gpu.size();
        let mut producer = MacosCefProducer::new(
            runtime,
            MacosCefConfig {
                surface: CefSurfaceConfig {
                    initial_url: FIXTURE_URL.to_owned(),
                    initial_size: size,
                    scale_factor: window.display_scale(),
                    ..Default::default()
                },
            },
        )
        .map_err(|e| format!("CEF producer: {e}"))?;
        println!(
            "gate: producer created mode={:?} size={}x{}",
            producer.surface_mode(),
            size.width,
            size.height
        );

        let started = Instant::now();
        let mut frame: Option<ImportedTexture> = None;
        let mut imported = 0u32;
        let mut matched = false;
        let mut failed = false;
        while started.elapsed() < timeout && !matched && !failed {
            for event in events.poll_iter() {
                if let sdl3::event::Event::Quit { .. } = event {
                    println!("gate: FAIL window closed before a verdict");
                    failed = true;
                }
            }
            runtime.do_message_loop_work();
            while let Some(event) = producer.poll_web_event() {
                match event {
                    CefSurfaceEvent::Navigation(NavigationEvent::ContentProcessTerminated {
                        status,
                        error_code,
                        error_string,
                    }) => {
                        println!(
                            "gate: FAIL content process terminated status={status:?} \
                             code={error_code} {error_string}"
                        );
                        failed = true;
                    },
                    CefSurfaceEvent::Navigation(event) => println!("gate: navigation {event:?}"),
                    _ => {},
                }
            }
            match producer.acquire_frame(&gpu.host) {
                Ok(Some(new)) => {
                    imported += 1;
                    if imported == 1 {
                        println!(
                            "gate: first accelerated frame size={}x{} format={:?} elapsed_ms={}",
                            new.size.width,
                            new.size.height,
                            new.format,
                            started.elapsed().as_millis()
                        );
                    }
                    match centre_matches(&gpu.host, &new) {
                        Ok(true) => {
                            println!(
                                "gate: PASS pixel centre {SAMPLE}x{SAMPLE} == {EXPECTED_BGRA:?} \
                                 ±{TOLERANCE} frame={imported}"
                            );
                            matched = true;
                        },
                        Ok(false) => {},
                        Err(error) => {
                            println!("gate: FAIL readback: {error}");
                            failed = true;
                        },
                    }
                    frame = Some(new);
                },
                Ok(None) => {},
                Err(error) => {
                    println!("gate: FAIL acquire_frame: {error}");
                    failed = true;
                },
            }
            if !gpu.present(frame.as_ref()) {
                std::thread::sleep(Duration::from_millis(4));
            }
            failed |= gpu.failed.load(Ordering::Relaxed);
        }
        println!(
            "gate: loop end imported={imported} matched={matched} elapsed_ms={}",
            started.elapsed().as_millis()
        );
        if imported > 0 && !matched && !failed {
            println!("gate: FAIL {imported} frame(s) imported, none matched the fixture colour");
            failed = true;
        }
        // Taken while the browser is alive, so the helper processes are still running.
        let sandboxed = renderer_sandboxed();
        let code = if failed || !sandboxed {
            1
        } else if matched {
            0
        } else {
            println!(
                "gate: NO_ACCELERATED_FRAME within {}s (no OnAcceleratedPaint; e.g. GPU \
                 compositing disabled on this machine)",
                timeout.as_secs()
            );
            2
        };
        drop(frame);
        Ok((code, producer))
    }

    /// Compares every texel of the frame's centre `SAMPLE` x `SAMPLE` block with the fixture
    /// colour.
    fn centre_matches(host: &HostWgpuContext, frame: &ImportedTexture) -> Result<bool, String> {
        let (w, h) = (frame.size.width, frame.size.height);
        if w < SAMPLE || h < SAMPLE {
            return Err(format!("frame {w}x{h} smaller than the sample"));
        }
        if frame.format != wgpu::TextureFormat::Bgra8Unorm {
            return Err(format!(
                "frame format {:?}, expected Bgra8Unorm",
                frame.format
            ));
        }
        let pixels = read_region(
            host,
            frame,
            ((w - SAMPLE) / 2, (h - SAMPLE) / 2, SAMPLE, SAMPLE),
        )?;
        Ok(pixels.as_chunks::<4>().0.iter().all(|px| {
            px.iter()
                .zip(EXPECTED_BGRA)
                .all(|(&a, b)| a.abs_diff(b) <= TOLERANCE)
        }))
    }

    /// Chromium's macOS launcher appends `--seatbelt-client=<fd>` to every child it starts
    /// sandboxed and nothing when `--no-sandbox` or an unsandboxed type applies
    /// (`content/browser/child_process_launcher_helper_mac.cc`, README "Chromium reference").
    /// Requires at least one renderer helper of this bundle, all of them with the switch and
    /// none with `--no-sandbox`; GPU and utility helpers are reported only.
    pub(crate) fn renderer_sandboxed() -> bool {
        let output = match std::process::Command::new("/bin/ps")
            .args(["-axww", "-o", "pid=,command="])
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                println!("gate: FAIL ps: {error}");
                return false;
            },
        };
        let mut renderers = 0;
        let mut sandboxed = 0;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if !line.contains("roves-cef-macos-gate Helper") {
                continue;
            }
            let kind = line
                .split_whitespace()
                .find_map(|arg| arg.strip_prefix("--type="))
                .unwrap_or("?");
            let seatbelt = line.contains("--seatbelt-client=");
            let no_sandbox = line.contains("--no-sandbox");
            println!("gate: helper type={kind} seatbelt={seatbelt} no-sandbox={no_sandbox}");
            if kind == "renderer" {
                renderers += 1;
                if seatbelt && !no_sandbox {
                    sandboxed += 1;
                }
            }
        }
        let ok = renderers > 0 && sandboxed == renderers;
        if ok {
            println!("gate: PASS sandbox renderers={renderers} all with --seatbelt-client");
        } else {
            println!("gate: FAIL sandbox renderers={renderers} sandboxed={sandboxed}");
        }
        ok
    }
}
