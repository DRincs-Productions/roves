//! Linux feasibility gate: CEF 154 accelerated OSR (`OnAcceleratedPaint`, native pixmap
//! DMA-BUF planes) -> welding/grafting Vulkan external-memory import -> wgpu 29 Vulkan device
//! that also presents to an SDL3 X11 window, with Chromium's sandbox on. No CEF windowed
//! mode and no software (`OnPaint`) path exist here.
//!
//! Two modes, chosen by the gate directory (README "Packaged game"):
//! - fixture gate (no `game/index.html` next to the executable): a `data:` page whose centre
//!   must match a known colour;
//! - packaged game (`src/player.rs`): an unmodified static game build in `game/` next to the
//!   executable, served from the shared loopback origin; load-end, first frame after it and
//!   the close path are checked.
//!
//! Exit codes (README "Verdicts"):
//! - 0: the mode's checks passed on an imported frame and every renderer was sandboxed. The
//!   PASS line carries `evidence=gpu` or `evidence=software-adapter`.
//! - 2: no accelerated frame arrived before the timeout (expected without a GPU/DRI3, e.g.
//!   on a hosted CI runner), but nothing failed and every renderer was sandboxed.
//! - 3: the Vulkan adapter is a CPU implementation (lavapipe) without the DMA-BUF import
//!   extensions, so no import device exists and nothing CEF painted can be shown; CEF, the
//!   sandbox, SDL3 presentation (black), the game load (packaged mode) and shutdown were
//!   still checked and nothing failed. Never accepted on a real GPU; never graphics evidence.
//! - 6: packaged game only, before CEF starts: the persistent profile is held by another
//!   instance, or its stored origin port is taken by another program.
//! - 1: any failure (CEF/SDL/wgpu setup, DMA-BUF device on a real GPU, import error incl.
//!   `IMPLICIT_MODIFIER`, wrong pixels, wgpu error, content process death, renderer not
//!   sandboxed or not found, game load refused, close not completed).
//!
//! Every verdict line starts with `gate:` on stdout; CI checks those lines.

/// The Windows player's pure rules (origin check, load statuses, profile name and files,
/// origin-port parsing), compiled unchanged; std only. Most of it (Windows keyboard
/// translation, test sequences) is unused here.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/game_rules.rs"]
mod game_rules;
/// The players' shared host API (`/__roves/<module>/<cmd>` on the loopback origin; Windows
/// player README "Host API"): `core`, `saves` and `steam` (always "unavailable" here). The
/// `window.__ROVES__` page marker is registered before the game document (`session.rs`), so
/// `isAvailable()` of roves-api is `true` in the packaged game.
#[cfg(target_os = "linux")]
#[path = "../../cef-window-probe/src/host_api.rs"]
mod host_api;
/// The Windows player's project-icon header rules (std only).
#[cfg(target_os = "linux")]
#[path = "../../cef-netrender-probe/src/icon_rules.rs"]
mod icon_rules;
/// The players' shared SDL3 -> welding page input (key codes: `keys.rs`).
#[cfg(target_os = "linux")]
#[path = "../../cef-netrender-probe/src/input.rs"]
mod input;
/// The macOS gate's pure key-code rules (Chromium's table via `keycode`).
#[cfg(target_os = "linux")]
#[path = "../../cef-macos-gate/src/key_rules.rs"]
mod key_rules;
/// The macOS gate's key translation (XKB native codes on Linux).
#[cfg(target_os = "linux")]
#[path = "../../cef-macos-gate/src/keys.rs"]
mod keys;
/// The Windows MCP bridge's pure helpers: only the page marker script and its bootstrap URL
/// are used here (no MCP bridge on Linux).
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../../cef-netrender-probe/src/mcp_rules.rs"]
mod mcp_rules;
/// The Windows player's page-marker registration over welding's DevTools channel.
#[cfg(target_os = "linux")]
#[path = "../../cef-netrender-probe/src/page_init.rs"]
mod page_init;
#[cfg(target_os = "linux")]
mod player;
/// The macOS gate's profile glue, compiled unchanged: `PlayerProfile` (`flock` lock and
/// stored origin port), `cef_dir` and the shared constants. Its macOS root helpers
/// (`default_root`, `translocated`) are unused; the Linux root is `player::default_root`.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../../cef-macos-gate/src/profile.rs"]
mod profile;
/// The stage-1 window probe's loopback server (axum + tower-http `ServeDir` on 127.0.0.1),
/// compiled unchanged as in the Windows player and the macOS gate.
#[cfg(target_os = "linux")]
#[path = "../../cef-window-probe/src/server.rs"]
mod server;
/// The macOS gate's packaged-game session glue (page markers, input, input test hook).
#[cfg(target_os = "linux")]
#[path = "../../cef-macos-gate/src/session.rs"]
mod session;
/// The players' shared window icon (Roves logo, project `icon.png` next to the executable).
#[cfg(target_os = "linux")]
#[path = "../../cef-netrender-probe/src/window_icon.rs"]
mod window_icon;

#[cfg(target_os = "linux")]
fn main() {
    std::process::exit(gate::run());
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("roves-cef-linux-gate is Linux-only");
    std::process::exit(1);
}

/// The process's original argv, in the kernel-provided argument area.
///
/// Chromium names zygote-forked children by rewriting that area: `ContentMain` calls
/// `base::SetProcessTitleFromCommandLine(argv)` (content/app/content_main.cc), whose
/// `setproctitle_init` only accepts an argv whose strings are contiguous and followed by
/// `environ` (base/process/set_process_title_linux.cc); after the fork the child calls it
/// again with the renderer/utility command line received over the zygote socket
/// (content/zygote/zygote_linux.cc, `Zygote::ReadArgsAndFork`). `cef::args::Args::new()`
/// builds a fresh `CString` array instead, so the check fails silently and every
/// zygote-forked process keeps showing `--type=zygote` in `/proc/<pid>/cmdline`.
/// glibc passes `(argc, argv, envp)` to `.init_array` functions (Rust's std relies on the
/// same convention), which is how the original pointers are captured here.
#[cfg(target_os = "linux")]
mod kernel_argv {
    use std::{
        ffi::{CStr, c_char, c_int},
        sync::atomic::{AtomicI32, AtomicPtr, Ordering},
    };

    static ARGC: AtomicI32 = AtomicI32::new(0);
    static ARGV: AtomicPtr<*mut c_char> = AtomicPtr::new(std::ptr::null_mut());

    extern "C" fn capture(argc: c_int, argv: *mut *mut c_char, _envp: *mut *mut c_char) {
        ARGC.store(argc, Ordering::Relaxed);
        ARGV.store(argv, Ordering::Relaxed);
    }

    #[used]
    #[unsafe(link_section = ".init_array")]
    static CAPTURE: extern "C" fn(c_int, *mut *mut c_char, *mut *mut c_char) = capture;

    /// `Some` only for a CEF subprocess (a `--type=` switch is present) whose original argv
    /// was captured. The browser process keeps welding's own `Args`: Chromium may rewrite
    /// this area, and Rust's `std::env::args` reads it lazily.
    pub fn subprocess_main_args() -> Option<cef::MainArgs> {
        let argc = ARGC.load(Ordering::Relaxed);
        let argv = ARGV.load(Ordering::Relaxed);
        if argv.is_null() || argc <= 0 {
            return None;
        }
        // SAFETY: glibc's argv has `argc` valid NUL-terminated strings and stays mapped for
        // the life of the process; nothing has rewritten it before the first CEF call.
        let is_subprocess = (0..argc as usize).any(|i| unsafe {
            let arg = *argv.add(i);
            !arg.is_null() && CStr::from_ptr(arg).to_bytes().starts_with(b"--type=")
        });
        is_subprocess.then_some(cef::MainArgs { argc, argv })
    }
}

#[cfg(target_os = "linux")]
mod gate {
    use std::{
        collections::HashSet,
        os::unix::fs::{MetadataExt, PermissionsExt},
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
        linux_cef::{LinuxCefConfig, LinuxCefProducer},
    };

    use crate::game_rules::PLAYER_GAME_DIR;

    /// Deterministic page: dodger blue everywhere, plus a 1 px corner element animated with
    /// requestAnimationFrame so CEF keeps painting (accelerated OSR only paints on change).
    /// Same fixture as the macOS gate and wgpu-weld's `WELD_PIXEL_FIXTURE`.
    const FIXTURE_URL: &str = "data:text/html,<!doctype html><html><body style=\"margin:0;\
        background:%231e90ff;width:100vw;height:100vh\"><div id=t style=\"position:fixed;\
        left:0;top:0;width:1px;height:1px\"></div><script>let n=0;(function f(){t.style.\
        background=(n++&1)?'%23000':'%23fff';requestAnimationFrame(f)})()</script></body>\
        </html>";
    /// Dodger blue (30, 144, 255). welding maps CEF's colour type to `Bgra8UnormSrgb` or
    /// `Rgba8UnormSrgb` (`linux_cef/cef_backed.rs`); a texture-to-buffer copy returns the
    /// stored sRGB bytes unchanged, so only the channel order depends on the format.
    const EXPECTED_RGB: [u8; 3] = [30, 144, 255];
    const TOLERANCE: u8 = 8;
    /// Centre sample: 64 x 4 bytes is exactly one `COPY_BYTES_PER_ROW_ALIGNMENT` row.
    const SAMPLE: u32 = 64;
    const FIXTURE_WINDOW: (u32, u32) = (640, 400);
    const GAME_WINDOW: (u32, u32) = (1280, 720);
    /// Upper bound for CEF's `OnBeforeClose` after `close()`; a run that exceeds it fails.
    const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
    /// Exact message of welding e991684's `LifeSpanHandler::on_before_close`
    /// (`linux_cef/cef_backed.rs`), the only close-completion signal its Linux producer
    /// exposes (Windows has `is_closed`); the macOS gate relies on the same record.
    const BEFORE_CLOSE_MESSAGE: &str = "CEF on_before_close";
    const WELDING_LINUX_TARGET: &str = "welding::linux_cef";
    /// Upper bound on the drive loop rate (~60 Hz), independent of vsync.
    const FRAME_INTERVAL: Duration = Duration::from_millis(16);
    pub(crate) const EXE_NAME: &str = "roves-cef-linux-gate";

    /// Set by the logger when welding reports `OnBeforeClose`.
    static BROWSER_CLOSED: AtomicBool = AtomicBool::new(false);

    struct Logger;

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
                || metadata.target().starts_with(WELDING_LINUX_TARGET)
        }
        fn log(&self, record: &log::Record) {
            if record.level() <= log::Level::Info {
                println!(
                    "gate: log {} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            } else if record.target().starts_with(WELDING_LINUX_TARGET)
                // `as_str` is `Some` only for a literal message: no formatting cost.
                && record.args().as_str() == Some(BEFORE_CLOSE_MESSAGE)
            {
                BROWSER_CLOSED.store(true, Ordering::SeqCst);
            }
        }
        fn flush(&self) {}
    }

    pub fn run() -> i32 {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(error) => {
                println!("gate: FAIL current_exe: {error}");
                return 1;
            },
        };
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        // MUST be the first CEF call: Linux CEF re-executes this binary for the zygote,
        // renderer, GPU and utility processes. Those return here and never reach the gate.
        // Subprocesses get the kernel's own argv (see `kernel_argv`) so Chromium can rewrite
        // the process title of zygote-forked children; the browser keeps welding's copy.
        if let Some(main_args) = super::kernel_argv::subprocess_main_args() {
            let args = cef::args::Args::from(main_args);
            return match CefRuntime::try_run_subprocess(&args, CefSandboxMode::Sandboxed) {
                Ok(code) if code >= 0 => code,
                Ok(code) => {
                    eprintln!("roves-cef-linux-gate: --type= process returned {code}");
                    1
                },
                Err(error) => {
                    eprintln!("roves-cef-linux-gate: CEF subprocess entry: {error}");
                    1
                },
            };
        }
        match CefRuntime::execute_process_from(&dir, CefSandboxMode::Sandboxed) {
            Ok(Some(code)) => return code,
            Ok(None) => {},
            Err(error) => {
                println!("gate: FAIL CEF subprocess entry: {error}");
                return 1;
            },
        }
        // Debug only so welding's `on_before_close` record reaches the logger; other debug
        // records are dropped in `log` without being formatted.
        let _ = log::set_logger(&Logger).map(|()| log::set_max_level(log::LevelFilter::Debug));
        let code = match run_inner(&exe, &dir) {
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

    fn run_inner(exe: &Path, dir: &Path) -> Result<i32, String> {
        let timeout = Duration::from_secs(env_u64("ROVES_GATE_TIMEOUT_SECS").unwrap_or(60));
        if !dir.join("libcef.so").exists() || !dir.join("icudtl.dat").exists() {
            return Err(format!(
                "no CEF distribution next to the executable in {}: run from the directory \
                 built by `bundle`",
                dir.display()
            ));
        }
        // The packaged game: an unmodified static build copied next to the executable, as
        // the Windows player's `<exe dir>\game`.
        let game_dir = dir.join(PLAYER_GAME_DIR);
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
                "{} needs the packaged game ({PLAYER_GAME_DIR}/index.html next to the \
                 executable)",
                crate::profile::ISOLATED_PROFILE_SWITCH
            ));
        }
        report_host(dir)?;

        // Packaged game, ordinary launch: the persistent profile is opened (lock, stored
        // origin port) before the loopback server and CEF start, as on Windows and macOS.
        // The lock lives until this function returns, after CEF shut down.
        let profile = match game.as_ref().filter(|_| !isolated) {
            Some(_) => {
                let root = crate::player::default_root(exe, dir)?;
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
                // Steam is a Windows player feature only (README "Steam").
                steam: None,
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
            Some(game_dir) => {
                let port = profile.as_ref().and_then(|p| p.stored_port).unwrap_or(0);
                let server = match start_game_server(game_dir, port) {
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
                    game_dir.display(),
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
            None => std::env::temp_dir().join(format!("{EXE_NAME}-{}", std::process::id())),
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
        let mut config = CefRuntimeConfig::new(dir, CefSandboxMode::Sandboxed);
        config.cache_path = Some(cache.clone());
        config.command_line_switches = vec![
            // A fresh profile would otherwise stop on first-run UI inside CefInitialize
            // (same switches as wgpu-weld's Linux demo).
            ("no-first-run".to_owned(), None),
            ("no-default-browser-check".to_owned(), None),
            // Unattended: no Secret Service/KWallet lookup for the cookie key. A gate choice;
            // the player needs a deliberate one before distribution (README "Open risks").
            ("password-store".to_owned(), Some("basic".to_owned())),
            // Keep Chromium on Ozone/X11 whatever the session is: its GPU process gets the
            // GBM device for native pixmaps from the X server through DRI3
            // (ui/gfx/linux/gbm_support_x11.cc), the path welding's Linux producer is
            // validated on. Wayland is a documented gap, not a silent fallback.
            ("ozone-platform".to_owned(), Some("x11".to_owned())),
        ];
        // CEF first, SDL second (same order as the macOS gate).
        let runtime = match CefRuntime::initialize(config) {
            Ok(runtime) => runtime,
            Err(error) => {
                stop_server(server);
                return Err(format!("CEF initialize: {error}"));
            },
        };
        println!(
            "gate: cef initialized sandbox=Sandboxed ozone=x11 cache={}",
            cache.display()
        );

        let code = {
            let result = (|| {
                // X11 is the gate path. ROVES_GATE_SDL_VIDEO_DRIVER only exists to observe the
                // Wayland gap on purpose; the run then says so and is not the X11 gate.
                let driver = std::env::var("ROVES_GATE_SDL_VIDEO_DRIVER")
                    .unwrap_or_else(|_| "x11".to_owned());
                let _ = sdl3::hint::set("SDL_VIDEO_DRIVER", &driver);
                let sdl = sdl3::init().map_err(|e| format!("SDL init: {e}"))?;
                let video = sdl
                    .video()
                    .map_err(|e| format!("SDL video ({driver}): {e}"))?;
                println!("gate: sdl video driver={}", video.current_video_driver());
                if video.current_video_driver() != "x11" {
                    println!(
                        "gate: notice SDL driver is not x11: this run is outside the X11 gate \
                         (README \"Wayland\")"
                    );
                }
                let mut events = sdl.event_pump().map_err(|e| format!("SDL events: {e}"))?;
                let (title, (w, h)) = if server.is_some() {
                    ("Roves", GAME_WINDOW)
                } else {
                    ("Roves CEF Linux gate", FIXTURE_WINDOW)
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
                    let line = crate::window_icon::apply(&mut window, exe.parent());
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
                            )
                        },
                        None => drive_fixture(&runtime, &window, &mut events, &gpu, timeout),
                    };
                    // The surface goes before the browser closes, the window after it.
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

    /// Closes the browser and pumps CEF until `OnBeforeClose` (CEF requires it before
    /// `CefShutdown`; Chromium closes every browser before quitting). A close that does not
    /// complete within `CLOSE_TIMEOUT` fails the run. Same procedure as the macOS gate.
    fn close_browser(
        runtime: &CefRuntime,
        events: &mut sdl3::EventPump,
        mut producer: LinuxCefProducer,
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

    /// Facts that decide which Chromium sandbox layer 1 can apply and whether a GPU path can
    /// exist at all. Reported, not judged, except running as root, which Chromium refuses
    /// without `--no-sandbox` (content/browser/zygote_host/zygote_host_impl_linux.cc).
    fn report_host(dir: &Path) -> Result<(), String> {
        let uid = std::fs::metadata("/proc/self")
            .map(|m| m.uid())
            .map_err(|e| format!("/proc/self: {e}"))?;
        if uid == 0 {
            return Err("running as root: Chromium refuses its sandbox as root".into());
        }
        let read = |path: &str| {
            std::fs::read_to_string(path)
                .map(|s| s.trim().to_owned())
                .unwrap_or_else(|_| "absent".to_owned())
        };
        println!(
            "gate: host userns max_user_namespaces={} unprivileged_userns_clone={} \
             apparmor_restrict_unprivileged_userns={}",
            read("/proc/sys/user/max_user_namespaces"),
            read("/proc/sys/kernel/unprivileged_userns_clone"),
            read("/proc/sys/kernel/apparmor_restrict_unprivileged_userns"),
        );
        // Chromium's own test (sandbox::Credentials::CanCreateProcessInNewUserNS) runs in
        // the browser; this independent probe mirrors it with util-linux to explain the
        // expected layer 1: a new user namespace mapping the current ids, then a nested
        // unprivileged unshare(CLONE_NEWUSER) inside it.
        let userns = std::process::Command::new("unshare")
            .args(["--user", "--map-current-user", "unshare", "--user", "true"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| if s.success() { "yes" } else { "no" })
            .unwrap_or("unknown");
        let sandbox = dir.join("chrome-sandbox");
        let suid = match std::fs::metadata(&sandbox) {
            Ok(m) => {
                let mode = m.permissions().mode();
                let ok = m.uid() == 0 && mode & 0o4000 != 0 && mode & 0o001 != 0;
                println!(
                    "gate: host chrome-sandbox uid={} mode={:o} setuid-ready={ok}",
                    m.uid(),
                    mode & 0o7777
                );
                ok
            },
            Err(_) => {
                println!("gate: host chrome-sandbox absent");
                false
            },
        };
        // Same order as ZygoteHostImpl::Init: namespace sandbox first, setuid second,
        // otherwise Chromium stops with "No usable sandbox!".
        let expected = match (userns, suid) {
            ("yes", _) => "namespace",
            (_, true) => "setuid",
            ("unknown", false) => "namespace-or-none",
            _ => "none",
        };
        println!("gate: host userns-probe={userns} sandbox-layer1-expected={expected}");
        let dri: Vec<String> = std::fs::read_dir("/dev/dri")
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        println!(
            "gate: host dri={:?} DISPLAY={:?} WAYLAND_DISPLAY={:?} XDG_SESSION_TYPE={:?}",
            dri,
            std::env::var("DISPLAY").ok(),
            std::env::var("WAYLAND_DISPLAY").ok(),
            std::env::var("XDG_SESSION_TYPE").ok(),
        );
        // Steam Linux Runtime (pressure-vessel) and Flatpak are not supported by this gate;
        // report them so a run inside one is not mistaken for the plain-host result.
        let container = [
            "PRESSURE_VESSEL_RUNTIME",
            "STEAM_RUNTIME",
            "SteamAppId",
            "FLATPAK_ID",
            "container",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| format!("{k}={v}")))
        .collect::<Vec<_>>();
        println!("gate: host container-env={container:?}");
        Ok(())
    }

    /// Whether the host device can import CEF's DMA-BUF frames.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Import {
        /// `welding::build_dmabuf_capable_device` succeeded.
        DmaBuf,
        /// CPU adapter without the extension set: frames are counted, never imported, and
        /// the window only ever shows black. Never graphics evidence.
        Unavailable,
    }

    /// wgpu Vulkan device presenting to the SDL3 X11 window; welding imports into the same
    /// device when it was created with the DMA-BUF extensions.
    pub(crate) struct Gpu {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        blitter: wgpu::util::TextureBlitter,
        pub(crate) host: HostWgpuContext,
        pub(crate) failed: Arc<AtomicBool>,
        pub(crate) import: Import,
        /// `gpu` or `software-adapter`, carried by every PASS line.
        pub(crate) evidence: &'static str,
    }

    impl Gpu {
        fn new(window: &sdl3::video::Window) -> Result<Gpu, String> {
            let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
            // welding's Linux import is Vulkan only (DMA-BUF -> VkImage -> grafting).
            instance_desc.backends = wgpu::Backends::VULKAN;
            let instance = wgpu::Instance::new(instance_desc);
            // SAFETY: SDL owns the X11 window; `Gpu` (and so the surface) is dropped before
            // the caller drops the window, all on the main thread.
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
                .map_err(|e| format!("Vulkan adapter: {e}"))?;
            let info = adapter.get_info();
            let software = info.device_type == wgpu::DeviceType::Cpu;
            let evidence = if software { "software-adapter" } else { "gpu" };
            println!(
                "gate: adapter name={:?} backend={:?} type={:?} driver={:?} driver_info={:?} \
                 evidence={evidence}",
                info.name, info.backend, info.device_type, info.driver, info.driver_info
            );
            let desc = wgpu::DeviceDescriptor {
                label: Some("Roves CEF Linux gate"),
                ..Default::default()
            };
            // wgpu 29 has no VULKAN_EXTERNAL_MEMORY_DMA_BUF feature; grafting adds the
            // external-memory, DRM-modifier and foreign-queue extensions itself.
            let (device, queue, import) =
                match welding::build_dmabuf_capable_device(&adapter, &desc) {
                    Ok((device, queue)) => {
                        println!("gate: dmabuf device created");
                        (device, queue, Import::DmaBuf)
                    },
                    Err(error) if software => {
                        println!("gate: dmabuf device unavailable on the CPU adapter: {error}");
                        let (device, queue) = pollster::block_on(adapter.request_device(&desc))
                            .map_err(|e| format!("Vulkan device: {e}"))?;
                        (device, queue, Import::Unavailable)
                    },
                    Err(error) => return Err(format!("dmabuf device on a GPU adapter: {error}")),
                };
            let host = HostWgpuContext::new(device, queue);
            if host.backend != InteropBackend::Vulkan {
                return Err(format!(
                    "interop backend {:?}, expected Vulkan",
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
            // CEF hands over sRGB-encoded bytes in an *Srgb texture; an sRGB target keeps the
            // blit colour-correct (as wgpu-weld's Linux demo does).
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let mut config = surface
                .get_default_config(&adapter, width.max(1), height.max(1))
                .ok_or("surface not supported by the adapter")?;
            config.format = format;
            config.present_mode = wgpu::PresentMode::AutoVsync;
            surface.configure(&host.device, &config);
            // FIFO is always offered, but under Xvfb + lavapipe it does not block (CI
            // 45612105775 presented ~2450 times per second); the drive loops are therefore
            // also paced by FRAME_INTERVAL (`Pacer`).
            println!(
                "gate: surface format={format:?} present_modes={:?} pacing_ms={}",
                caps.present_modes,
                FRAME_INTERVAL.as_millis()
            );
            let blitter = wgpu::util::TextureBlitter::new(&host.device, format);
            Ok(Gpu {
                surface,
                config,
                blitter,
                host,
                failed,
                import,
                evidence,
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
        }

        /// Shows the latest imported frame in the SDL3 window (stretched) or black before
        /// the first frame. Returns whether a texture was presented.
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
            match frame {
                Some(frame) => {
                    self.blitter
                        .copy(&self.host.device, &mut encoder, &frame.view, &target)
                },
                None => {
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

    /// Bounded pacing whatever the present mode does: at most one loop iteration (CEF pump,
    /// frame poll, present) per FRAME_INTERVAL; a late iteration resets the schedule instead
    /// of bursting to catch up.
    pub(crate) struct Pacer(Instant);

    impl Pacer {
        pub(crate) fn new() -> Pacer {
            Pacer(Instant::now())
        }

        pub(crate) fn wait(&mut self) {
            self.0 += FRAME_INTERVAL;
            let now = Instant::now();
            if self.0 > now {
                std::thread::sleep(self.0 - now);
            } else {
                self.0 = now;
            }
        }
    }

    fn drive_fixture(
        runtime: &CefRuntime,
        window: &sdl3::video::Window,
        events: &mut sdl3::EventPump,
        gpu: &Gpu,
        timeout: Duration,
    ) -> Result<(i32, LinuxCefProducer), String> {
        let size = gpu.size();
        let evidence = gpu.evidence;
        let mut producer = LinuxCefProducer::new(
            runtime,
            LinuxCefConfig {
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
        let mut delivered = 0u32;
        let mut matched = false;
        let mut failed = false;
        let mut presented = 0u32;
        let mut pacer = Pacer::new();
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
            match gpu.import {
                Import::DmaBuf => match producer.acquire_frame(&gpu.host) {
                    Ok(Some(new)) => {
                        delivered += 1;
                        if delivered == 1 {
                            println!(
                                "gate: first accelerated frame imported size={}x{} format={:?} \
                                 elapsed_ms={}",
                                new.size.width,
                                new.size.height,
                                new.format,
                                started.elapsed().as_millis()
                            );
                        }
                        match centre_matches(&gpu.host, &new) {
                            Ok(true) => {
                                println!(
                                    "gate: PASS pixel centre {SAMPLE}x{SAMPLE} == rgb \
                                     {EXPECTED_RGB:?} ±{TOLERANCE} format={:?} frame={delivered} \
                                     evidence={evidence}",
                                    new.format
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
                        report_acquire_error(&error.to_string());
                        failed = true;
                    },
                },
                Import::Unavailable => {
                    // Take (and drop, closing the duplicated fds) what CEF delivered so
                    // the count is honest even though nothing can be imported.
                    if producer.acquire_native_frame().is_some() {
                        delivered += 1;
                        if delivered == 1 {
                            println!(
                                "gate: first accelerated frame delivered (not imported: no \
                                 dmabuf device) elapsed_ms={}",
                                started.elapsed().as_millis()
                            );
                        }
                    }
                },
            }
            if gpu.present(frame.as_ref()) {
                presented += 1;
            }
            failed |= gpu.failed.load(Ordering::Relaxed);
            pacer.wait();
        }
        let elapsed = started.elapsed();
        println!(
            "gate: loop end delivered={delivered} matched={matched} presented={presented} \
             elapsed_ms={} present_rate_hz={:.1}",
            elapsed.as_millis(),
            f64::from(presented) / elapsed.as_secs_f64().max(0.001)
        );
        if gpu.import == Import::DmaBuf && delivered > 0 && !matched && !failed {
            println!("gate: FAIL {delivered} frame(s) imported, none matched the fixture colour");
            failed = true;
        }
        if presented == 0 && !failed {
            println!("gate: FAIL nothing was presented to the SDL3 window");
            failed = true;
        }
        // Taken while the browser is alive, so the subprocesses are still running.
        let sandboxed = renderer_sandboxed();
        let code = if failed || !sandboxed {
            1
        } else if matched {
            0
        } else if gpu.import == Import::Unavailable {
            println!(
                "gate: NO_DMABUF_DEVICE on a CPU adapter (frames delivered={delivered}, none \
                 importable): CEF, sandbox, SDL3/wgpu presentation verified; import needs a GPU"
            );
            3
        } else {
            println!(
                "gate: NO_ACCELERATED_FRAME within {}s (no OnAcceleratedPaint; e.g. no GPU or \
                 no DRI3/GBM device for Chromium)",
                timeout.as_secs()
            );
            2
        };
        drop(frame);
        Ok((code, producer))
    }

    /// One `gate: FAIL` line for an `acquire_frame` error. welding refuses CEF's implicit
    /// modifier on the wgpu-29 row (native_frame/vulkan_dmabuf.rs); AMD/radeonsi buffers hit
    /// it, so it gets its own marker.
    pub(crate) fn report_acquire_error(text: &str) {
        if text.contains("DRM_FORMAT_MOD_INVALID") {
            println!("gate: FAIL IMPLICIT_MODIFIER acquire_frame: {text}");
        } else {
            println!("gate: FAIL acquire_frame: {text}");
        }
    }

    /// Copies a `w` x `h` region at (`x`, `y`) of the imported texture (grafting creates it
    /// with `COPY_SRC`) and returns it tightly packed, 4 bytes per texel, in the frame's own
    /// channel order. Same procedure as the macOS gate's `read_region`.
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

    /// Whether the frame's 8-bit format stores red first (`Some(true)`), blue first
    /// (`Some(false)`) or is something else (`None`).
    fn red_first(format: wgpu::TextureFormat) -> Option<bool> {
        match format {
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba8Unorm => Some(true),
            wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm => Some(false),
            _ => None,
        }
    }

    /// The whole frame read back, tightly packed, converted to B G R A byte order (for the
    /// BMP dump; welding may hand out either order on Linux).
    pub(crate) fn frame_pixels_bgra(
        host: &HostWgpuContext,
        frame: &ImportedTexture,
    ) -> Result<Vec<u8>, String> {
        let rgba = red_first(frame.format)
            .ok_or_else(|| format!("frame format {:?}, expected 8-bit RGBA/BGRA", frame.format))?;
        let mut pixels = read_region(host, frame, (0, 0, frame.size.width, frame.size.height))?;
        if rgba {
            for px in pixels.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        Ok(pixels)
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

    /// Writes a top-down 32-bit BMP (`BI_RGB`, byte order B G R X), the simplest lossless
    /// format every image viewer opens; evidence artifact only (same writer as the macOS
    /// gate).
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

    /// Compares every texel of the frame's centre `SAMPLE` x `SAMPLE` block with the fixture
    /// colour, in the frame's channel order.
    fn centre_matches(host: &HostWgpuContext, frame: &ImportedTexture) -> Result<bool, String> {
        let (w, h) = (frame.size.width, frame.size.height);
        if w < SAMPLE || h < SAMPLE {
            return Err(format!("frame {w}x{h} smaller than the sample"));
        }
        let expected: [u8; 4] = match red_first(frame.format) {
            Some(false) => [EXPECTED_RGB[2], EXPECTED_RGB[1], EXPECTED_RGB[0], 255],
            Some(true) => [EXPECTED_RGB[0], EXPECTED_RGB[1], EXPECTED_RGB[2], 255],
            None => {
                return Err(format!(
                    "frame format {:?}, expected 8-bit RGBA/BGRA",
                    frame.format
                ));
            },
        };
        let pixels = read_region(
            host,
            frame,
            ((w - SAMPLE) / 2, (h - SAMPLE) / 2, SAMPLE, SAMPLE),
        )?;
        Ok(pixels.as_chunks::<4>().0.iter().all(|px| {
            px.iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= TOLERANCE)
        }))
    }

    /// One CEF subprocess of this gate, read from world-readable `/proc` files (the
    /// sandboxed processes are non-dumpable, so `/proc/<pid>/ns/*` and `environ` are not).
    struct Child {
        pid: u32,
        ppid: u32,
        /// `--type=` value, `?` if absent.
        kind: String,
        /// Whether `cmdline` is Chromium's rewritten process title (one string) rather than
        /// the exec-time argv.
        titled: bool,
        no_sandbox: bool,
        /// `--no-zygote-sandbox`: marks the unsandboxed zygote (zygote_host_impl_linux.cc).
        no_zygote_sandbox: bool,
        /// `Seccomp:` 2 = seccomp-BPF filter mode (Chromium's layer 2).
        seccomp: String,
        no_new_privs: String,
        /// Number of `NSpid:` entries: 2+ means the process lives in a nested PID
        /// namespace, which both layer-1 sandboxes (user-namespace and setuid) create.
        pid_ns_depth: usize,
    }

    /// What a gate subprocess is, decided from Chromium's own naming plus the process tree.
    enum Role {
        /// `--type=zygote` without seccomp. `generic` = the pre-sandboxed zygote that forks
        /// renderers (no `--no-zygote-sandbox`).
        Zygote { generic: bool },
        /// `--type=renderer` whose parent is the generic zygote.
        Renderer,
        /// GPU, utility or anything else: reported only.
        Other,
        /// Identity cannot be established; never counted as a renderer nor ignored.
        Unidentified(&'static str),
    }

    /// Identity comes from the `--type=` that Chromium writes into the child's process
    /// title from the command line the browser sent in the zygote fork request
    /// (`Zygote::ReadArgsAndFork` -> `SetProcessTitleFromCommandLine`), checked against the
    /// launch structure of the pinned Chromium: renderers fork from the generic zygote
    /// (`RendererSandboxedProcessLauncherDelegate::GetZygote`), the GPU process is
    /// launched directly, utilities from the generic zygote, the unsandboxed zygote or
    /// directly (`UtilitySandboxedProcessLauncherDelegate::GetZygote`). A zygote never
    /// installs seccomp-BPF itself (its children do, after the fork), so a seccomp process
    /// still titled `--type=zygote` is a forked child whose title was not applied: it could
    /// be a renderer or a utility, and is reported as unidentified.
    fn role(child: &Child, parent: Option<&Child>) -> Role {
        let is_generic_zygote =
            |c: &Child| c.kind == "zygote" && !c.no_zygote_sandbox && c.seccomp != "2";
        match child.kind.as_str() {
            "zygote" if child.seccomp == "2" => Role::Unidentified(
                "zygote-forked child still shows the zygote command line (Chromium's process \
                 title was not applied)",
            ),
            "zygote" => Role::Zygote {
                generic: !child.no_zygote_sandbox,
            },
            "renderer" if parent.is_some_and(is_generic_zygote) => Role::Renderer,
            "renderer" => {
                Role::Unidentified("titled renderer whose parent is not the sandboxed zygote")
            },
            _ => Role::Other,
        }
    }

    /// Requires at least one renderer descendant of this process, every renderer with
    /// seccomp-BPF (`Seccomp: 2`), `NoNewPrivs: 1`, a nested PID namespace and no
    /// `--no-sandbox`, and no unidentified subprocess. Zygote, GPU and utility processes
    /// are reported only.
    pub(crate) fn renderer_sandboxed() -> bool {
        let me = std::process::id();
        let children = match gate_children(me) {
            Ok(children) => children,
            Err(error) => {
                println!("gate: FAIL /proc scan: {error}");
                return false;
            },
        };
        let mut renderers = 0;
        let mut sandboxed = 0;
        let mut unidentified = 0;
        for child in &children {
            let parent = children.iter().find(|c| c.pid == child.ppid);
            let role = role(child, parent);
            let label = match role {
                Role::Zygote { generic: true } => "zygote",
                Role::Zygote { generic: false } => "unsandboxed-zygote",
                Role::Renderer => "renderer",
                Role::Other => "other",
                Role::Unidentified(_) => "unidentified",
            };
            println!(
                "gate: child pid={} ppid={} type={} role={label} title={} seccomp={} \
                 no_new_privs={} pid_ns_depth={} no-sandbox={}",
                child.pid,
                child.ppid,
                child.kind,
                if child.titled { "chromium" } else { "exec" },
                child.seccomp,
                child.no_new_privs,
                child.pid_ns_depth,
                child.no_sandbox
            );
            match role {
                Role::Renderer => {
                    renderers += 1;
                    if child.seccomp == "2"
                        && child.no_new_privs == "1"
                        && child.pid_ns_depth >= 2
                        && !child.no_sandbox
                    {
                        sandboxed += 1;
                    }
                },
                Role::Unidentified(reason) => {
                    unidentified += 1;
                    println!(
                        "gate: FAIL sandbox child pid={} unidentified: {reason}",
                        child.pid
                    );
                },
                Role::Zygote { .. } | Role::Other => {},
            }
        }
        let ok = renderers > 0 && sandboxed == renderers && unidentified == 0;
        if ok {
            println!(
                "gate: PASS sandbox renderers={renderers} all with seccomp-bpf and a PID \
                 namespace"
            );
        } else {
            println!(
                "gate: FAIL sandbox renderers={renderers} sandboxed={sandboxed} \
                 unidentified={unidentified}"
            );
        }
        ok
    }

    /// Splits `/proc/<pid>/cmdline` of a gate process into its switches, or `None` if the
    /// process is not this gate. Two shapes exist: the exec-time argv (NUL-separated,
    /// argv[0] = the gate) and Chromium's process title, one string
    /// `"<exe or argv0> <arg> <arg> ..."` (base/process/set_process_title.cc). Returns the
    /// switches and whether the title shape was seen. The title shape assumes no space in
    /// the gate's path.
    fn parse_cmdline(raw: &[u8]) -> Option<(Vec<String>, bool)> {
        let elements: Vec<String> = raw
            .split(|&b| b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        let is_gate = |s: &str| Path::new(s).file_name().is_some_and(|n| n == EXE_NAME);
        let first = elements.first()?;
        if is_gate(first) {
            return Some((elements[1..].to_vec(), false));
        }
        let (head, rest) = first.split_once(' ')?;
        if elements.len() != 1 || !is_gate(head) {
            return None;
        }
        Some((
            rest.split(' ')
                .filter(|a| !a.is_empty())
                .map(str::to_owned)
                .collect(),
            true,
        ))
    }

    fn status_field(status: &str, key: &str) -> Option<String> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|v| v.trim().to_owned())
    }

    /// Every process whose `PPid` chain reaches `root` within a few hops and whose argv[0]
    /// is this gate (Linux CEF re-executes the gate binary for its subprocesses).
    fn gate_children(root: u32) -> Result<Vec<Child>, String> {
        let mut children = Vec::new();
        for entry in std::fs::read_dir("/proc").map_err(|e| e.to_string())? {
            let Ok(entry) = entry else { continue };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == root || !descends_from(pid, root) {
                continue;
            }
            let base = PathBuf::from("/proc").join(pid.to_string());
            let Ok(cmdline) = std::fs::read(base.join("cmdline")) else {
                continue;
            };
            let Some((args, titled)) = parse_cmdline(&cmdline) else {
                continue;
            };
            let Ok(status) = std::fs::read_to_string(base.join("status")) else {
                continue;
            };
            children.push(Child {
                pid,
                ppid: status_field(&status, "PPid:")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                kind: args
                    .iter()
                    .find_map(|a| a.strip_prefix("--type="))
                    .unwrap_or("?")
                    .to_owned(),
                titled,
                no_sandbox: args.iter().any(|a| a == "--no-sandbox"),
                no_zygote_sandbox: args.iter().any(|a| a == "--no-zygote-sandbox"),
                seccomp: status_field(&status, "Seccomp:").unwrap_or_else(|| "?".into()),
                no_new_privs: status_field(&status, "NoNewPrivs:").unwrap_or_else(|| "?".into()),
                pid_ns_depth: status_field(&status, "NSpid:")
                    .map(|v| v.split_whitespace().count())
                    .unwrap_or(0),
            });
        }
        children.sort_by_key(|c| c.pid);
        Ok(children)
    }

    fn descends_from(mut pid: u32, root: u32) -> bool {
        for _ in 0..8 {
            let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
                return false;
            };
            let Some(parent) = status_field(&status, "PPid:").and_then(|v| v.parse().ok()) else {
                return false;
            };
            if parent == root {
                return true;
            }
            if parent <= 1 {
                return false;
            }
            pid = parent;
        }
        false
    }
}
