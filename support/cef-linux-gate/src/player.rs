//! Packaged-game mode (README "Packaged game"): an unmodified static game build in `game/`
//! next to the executable is served by the shared loopback server
//! (`support/cef-window-probe/src/server.rs`) and opened in the same sandboxed accelerated
//! OSR -> welding -> wgpu Vulkan -> SDL3 X11 path as the fixture gate. The Linux counterpart
//! of the macOS gate's `player.rs`; checked here:
//!
//! - the main frame's load-end on the loopback origin with an accepted status (Chromium's
//!   `WebContentsObserver::DidFinishLoad`, which CEF reports as `OnLoadEnd`), and no main
//!   frame committed off that origin;
//! - the first frame imported after that load-end (the OSR counterpart of
//!   `DidFirstVisuallyNonEmptyPaint`), presented in the SDL3 window, and a last frame with
//!   more than one colour. With a DMA-BUF import device the frame is CEF's accelerated
//!   texture (`evidence=gpu`); without one (CPU Vulkan adapter such as lavapipe on a hosted
//!   runner, no import device, or `ROVES_FORCE_CPU_PAINT=1`) it is CEF's software output
//!   uploaded by welding (`evidence=software`, README "Software paint");
//! - the close path: an SDL quit or window close request ends the loop, then the caller
//!   closes the browser and waits for `OnBeforeClose` before `CefShutdown`.
//!
//! The browser uses welding's on-disk context `<root cache>/game-profile`: persistent in the
//! ordinary launch, per run with `--roves-isolated-profile` (`main.rs`). welding's Linux
//! producer accepts `user_data_dir` in `new` (it pumps the context initialisation itself),
//! as on Windows; the macOS two-step preparation is not needed.
//!
//! The host API routes (`host_api.rs`, `core` + `saves`) are served on the loopback origin;
//! their `exit` ends this loop like a window close. The macOS gate's `session.rs` registers
//! the `window.__ROVES__` marker through welding's in-process DevTools channel before the game
//! document (no remote-debugging port is opened) and forwards SDL3 page input; the window is
//! resizable (surface reconfigured, CEF `WasResized`) and takes the game document's title.
//! `ROVES_GATE_TEST_INPUT_MS` runs the synthetic input test (`session.rs`); the test
//! auto-close waits for it.
//!
//! Test-only environment (CI): `ROVES_GATE_AUTO_CLOSE_MS` pushes one SDL quit event that long
//! after the first game frame, so the ordinary close path runs unattended;
//! `ROVES_GATE_FRAME_DUMP=<file.bmp>` writes the last imported frame before close.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use sdl3::event::{Event, WindowEvent};
use welding::{
    CefRuntime, CefSurfaceConfig, CefSurfaceEvent, CefSurfaceProducer, ImportedTexture,
    NavigationEvent,
    linux_cef::{LinuxCefConfig, LinuxCefProducer},
};

use crate::{
    game_rules::{PaintPath, load_status_ok, on_origin, player_profile_name},
    gate::{
        Gpu, Pacer, distinct_colours, env_u64, frame_pixels_bgra, renderer_sandboxed,
        report_acquire_error, write_bmp,
    },
    session::Session,
};

/// Console lines printed before only counting them.
const CONSOLE_LOG_CAP: u32 = 200;
/// Longest console message kept per line (characters).
const CONSOLE_MESSAGE_MAX: usize = 300;

/// Default persistent profile root: `<config home>/Roves/Player/<name>`, where `<config
/// home>` is `$XDG_CONFIG_HOME` when it is an absolute path, otherwise `$HOME/.config`
/// (XDG Base Directory: relative values are ignored), and `<name>` is the Windows rule
/// (`player_profile_name`) applied to the executable stem and the canonical directory of the
/// executable (the flat CEF directory that contains `game/`). Chromium's Linux user-data
/// directory has the same parent (`chrome/common/chrome_paths_linux.cc`
/// `GetDefaultUserDataDirectory`: `$CHROME_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else
/// `~/.config`); unlike Chromium the name is per install, so two packaged games never share
/// storage, and `CHROME_CONFIG_HOME` is not read.
pub(crate) fn default_root(exe: &Path, dir: &Path) -> Result<PathBuf, String> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let config = match absolute("XDG_CONFIG_HOME") {
        Some(config) => config,
        None => absolute("HOME")
            .ok_or("neither XDG_CONFIG_HOME nor HOME is an absolute path")?
            .join(".config"),
    };
    let dir = std::fs::canonicalize(dir)
        .map_err(|e| format!("install directory {}: {e}", dir.display()))?;
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    Ok(config
        .join("Roves")
        .join("Player")
        .join(player_profile_name(&stem, &dir.to_string_lossy())))
}

/// `profile_dir` is welding's on-disk context for the game (`<root cache>/game-profile`), in
/// the persistent profile and in the isolated per-run cache alike, as on Windows and macOS.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drive(
    runtime: &CefRuntime,
    window: &mut sdl3::video::Window,
    events: &mut sdl3::EventPump,
    sdl_events: &sdl3::EventSubsystem,
    mouse: &sdl3::mouse::MouseUtil,
    gpu: &mut Gpu,
    origin: &str,
    profile_dir: &Path,
    timeout: Duration,
    game_exit: &std::sync::OnceLock<&'static str>,
) -> Result<(i32, LinuxCefProducer), String> {
    let auto_close = env_u64("ROVES_GATE_AUTO_CLOSE_MS").map(Duration::from_millis);
    let dump = std::env::var_os("ROVES_GATE_FRAME_DUMP").map(PathBuf::from);
    let size = gpu.size();
    let mut session = Session::new(origin)?;
    let url = session.initial_url();
    let mut scale = window.display_scale();
    let creating = Instant::now();
    let mut producer = LinuxCefProducer::new(
        runtime,
        LinuxCefConfig {
            surface: CefSurfaceConfig {
                initial_url: url.clone(),
                initial_size: size,
                scale_factor: scale,
                user_data_dir: Some(profile_dir.to_path_buf()),
                prefer_accelerated: gpu.paint == PaintPath::Accelerated,
                // The page markers' in-process DevTools channel (no port).
                devtools_protocol: session.devtools(),
                ..Default::default()
            },
        },
    )
    .map_err(|e| format!("CEF producer: {e}"))?;
    println!(
        "gate: producer created mode={:?} size={}x{} url={url} profile={} elapsed_ms={}",
        producer.surface_mode(),
        size.width,
        size.height,
        profile_dir.display(),
        creating.elapsed().as_millis()
    );
    let anchor = "first-frame";

    let started = Instant::now();
    let mut pacer = Pacer::new();
    let mut frame: Option<ImportedTexture> = None;
    let mut imported = 0u32;
    let mut presented = 0u32;
    let mut loaded_at: Option<Instant> = None;
    let mut first_game_frame: Option<Instant> = None;
    let mut close_at: Option<Instant> = None;
    let mut close_pushed = false;
    let mut close_source: Option<&'static str> = None;
    let mut failed = false;
    let mut console_lines = 0u32;
    let mut console_levels: BTreeMap<i32, u32> = BTreeMap::new();
    let mut timed_out = false;

    while close_source.is_none() && !failed {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => close_source = close_source.or(Some("quit")),
                Event::Window {
                    win_event: WindowEvent::CloseRequested,
                    ..
                } => close_source = close_source.or(Some("window-close")),
                Event::Window {
                    win_event: WindowEvent::PixelSizeChanged(..),
                    ..
                } => {
                    let (width, height) = window.size_in_pixels();
                    gpu.resize(width, height);
                    let resized = producer.resize(gpu.size());
                    let now = window.display_scale();
                    let rescaled = if now == scale {
                        Ok(())
                    } else {
                        scale = now;
                        producer.set_scale_factor(now)
                    };
                    println!(
                        "gate: resized surface={}x{} scale={now} ok={}",
                        gpu.size().width,
                        gpu.size().height,
                        resized.is_ok() && rescaled.is_ok()
                    );
                },
                event => session.sdl_event(&mut producer, window, &event),
            }
        }
        // Host API `exit`/`clear_content_cache` (server thread): the same close path.
        if close_source.is_none()
            && let Some(cmd) = game_exit.get()
        {
            println!("gate: close requested by the game command={cmd}");
            close_source = Some("game-exit");
        }
        runtime.do_message_loop_work();
        session.devtools_step(&mut producer);
        while let Some(event) = producer.poll_web_event() {
            let CefSurfaceEvent::Navigation(event) = event else {
                continue;
            };
            // The about:blank document of the page-marker registration (`session.rs`).
            let bootstrap = match &event {
                NavigationEvent::LoadStart { url }
                | NavigationEvent::LoadEnd { url, .. }
                | NavigationEvent::LoadError { url, .. }
                | NavigationEvent::AddressChanged { url } => session.bootstrap_event(url),
                _ => false,
            };
            if bootstrap {
                println!("gate: bootstrap document event={event:?}");
                continue;
            }
            match event {
                NavigationEvent::LoadStart { url } => println!("gate: load-start url={url}"),
                NavigationEvent::LoadEnd { url, http_status } => {
                    let ok = load_status_ok(http_status) && on_origin(origin, &url);
                    println!("gate: load-end status={http_status} url={url} ok={ok}");
                    if !ok {
                        println!("gate: FAIL game load-end not accepted");
                        failed = true;
                    } else if loaded_at.is_none() {
                        loaded_at = Some(Instant::now());
                        session.loaded();
                        println!(
                            "gate: game loaded elapsed_ms={}",
                            started.elapsed().as_millis()
                        );
                        // Re-send the size so an already static page paints once more
                        // (welding's Linux `resize` is `WasResized`).
                        let requested = producer.resize(gpu.size());
                        println!("gate: fresh frame requested ok={}", requested.is_ok());
                    }
                },
                NavigationEvent::LoadError {
                    url,
                    error_code,
                    error_text,
                } => {
                    println!(
                        "gate: FAIL main-frame load error code={error_code} url={url} {error_text}"
                    );
                    failed = true;
                },
                // welding exposes no pre-navigation veto, so a main frame that commits off
                // the loopback origin fails the run (as on Windows and macOS). History API
                // and fragment routes stay on the origin.
                NavigationEvent::AddressChanged { url } => {
                    if on_origin(origin, &url) {
                        println!("gate: address {url}");
                    } else {
                        println!("gate: FAIL main frame left the game origin url={url}");
                        failed = true;
                    }
                },
                NavigationEvent::TitleChanged { title } => {
                    println!("gate: title {title:?}");
                    if session.game_document() {
                        let _ = window.set_title(&title);
                    }
                },
                NavigationEvent::ConsoleMessage {
                    level,
                    message,
                    source,
                    line,
                } => {
                    console_lines += 1;
                    *console_levels.entry(level).or_default() += 1;
                    session.console(&source, &message);
                    if console_lines <= CONSOLE_LOG_CAP {
                        let message: String = message.chars().take(CONSOLE_MESSAGE_MAX).collect();
                        println!(
                            "gate: console level={level} source={source}:{line} message={message:?}"
                        );
                    }
                },
                // welding cancels popup creation itself; reported only.
                NavigationEvent::NewWindowRequested { url, user_gesture } => {
                    println!("gate: popup refused url={url} user_gesture={user_gesture}")
                },
                NavigationEvent::ContentProcessTerminated {
                    status,
                    error_code,
                    error_string,
                } => {
                    println!(
                        "gate: FAIL content process terminated status={status:?} \
                         code={error_code} {error_string}"
                    );
                    failed = true;
                },
                _ => {},
            }
        }
        match producer.acquire_frame(&gpu.host) {
            Ok(Some(new)) => {
                imported += 1;
                if imported == 1 {
                    println!(
                        "gate: first frame imported size={}x{} format={:?} evidence={} \
                         elapsed_ms={}",
                        new.size.width,
                        new.size.height,
                        new.format,
                        gpu.evidence,
                        started.elapsed().as_millis()
                    );
                }
                if let Some(loaded) = loaded_at
                    && first_game_frame.is_none()
                {
                    first_game_frame = Some(Instant::now());
                    let colours =
                        frame_pixels_bgra(&gpu.host, &new).map(|px| distinct_colours(&px));
                    println!(
                        "gate: first game frame after load-end ms={} size={}x{} format={:?} \
                         generation={} distinct_colours={} evidence={}",
                        loaded.elapsed().as_millis(),
                        new.size.width,
                        new.size.height,
                        new.format,
                        new.generation,
                        colours
                            .as_ref()
                            .map_or_else(|e| format!("error({e})"), usize::to_string),
                        gpu.evidence
                    );
                    if let Err(error) = colours {
                        println!("gate: FAIL readback: {error}");
                        failed = true;
                    }
                    schedule_close(auto_close, anchor, &mut close_at);
                }
                frame = Some(new);
            },
            Ok(None) => {},
            Err(error) => {
                report_acquire_error(&error.to_string());
                failed = true;
            },
        }
        session.per_turn(&mut producer, mouse, sdl_events, window);
        if gpu.present(frame.as_ref()) {
            presented += 1;
        }
        failed |= gpu.failed.load(Ordering::Relaxed) || session.failed;

        if let Some(at) = close_at
            && !close_pushed
            && Instant::now() >= at
            && session.tests_finished()
        {
            close_pushed = true;
            // Through SDL's own queue, so the ordinary close handling above runs.
            match sdl_events.push_event(Event::Quit { timestamp: 0 }) {
                Ok(()) => println!("gate: test auto-close pushed SDL quit"),
                Err(error) => {
                    println!("gate: FAIL test auto-close push: {error}");
                    failed = true;
                },
            }
        }
        // The first game frame ends the wait (as on macOS).
        if first_game_frame.is_none() && started.elapsed() >= timeout {
            timed_out = true;
            break;
        }
        pacer.wait();
    }

    let elapsed = started.elapsed();
    println!(
        "gate: loop end imported={imported} presented={presented} loaded={} game_frame={} \
         close={} elapsed_ms={}",
        loaded_at.is_some(),
        first_game_frame.is_some(),
        close_source.unwrap_or("none"),
        elapsed.as_millis()
    );
    println!("gate: console lines={console_lines} by_level={console_levels:?}");
    session.finish();
    failed |= session.failed;
    if let Some(source) = close_source {
        println!("gate: close requested source={source}");
    }

    let mut no_frame = false;
    if close_source.is_some() && !failed && first_game_frame.is_none() {
        println!("gate: FAIL closed before the first game frame");
        failed = true;
    }
    if timed_out && !failed {
        if loaded_at.is_none() {
            println!(
                "gate: FAIL game load-end not received within {}s",
                timeout.as_secs()
            );
            failed = true;
        } else if imported == 0 {
            no_frame = true;
        } else {
            println!(
                "gate: FAIL no frame after the game load-end within {}s",
                timeout.as_secs()
            );
            failed = true;
        }
    }
    if presented == 0 && !failed {
        println!("gate: FAIL nothing was presented to the SDL3 window");
        failed = true;
    }
    // The last frame shown before close: evidence of what the game displayed.
    if !failed && first_game_frame.is_some() {
        match frame.as_ref().map(|f| (f, frame_pixels_bgra(&gpu.host, f))) {
            Some((f, Ok(pixels))) => {
                let colours = distinct_colours(&pixels);
                println!(
                    "gate: last game frame size={}x{} generation={} distinct_colours={colours}",
                    f.size.width, f.size.height, f.generation
                );
                if colours < 2 {
                    println!("gate: FAIL last game frame is a single colour");
                    failed = true;
                }
                if let Some(path) = dump.as_ref() {
                    match write_bmp(path, f.size.width, f.size.height, &pixels) {
                        Ok(()) => println!("gate: frame dump written {}", path.display()),
                        Err(error) => println!("gate: frame dump not written: {error}"),
                    }
                }
            },
            Some((_, Err(error))) => {
                println!("gate: FAIL readback: {error}");
                failed = true;
            },
            None => {},
        }
    }
    // Taken while the browser is alive, so the subprocesses are still running.
    let sandboxed = renderer_sandboxed();
    let code = if failed || !sandboxed {
        1
    } else if no_frame {
        println!(
            "gate: NO_FRAME within {}s (game loaded, no paint) evidence={}",
            timeout.as_secs(),
            gpu.evidence
        );
        2
    } else {
        println!(
            "gate: PASS packaged game loaded, first frame shown, close requested evidence={}",
            gpu.evidence
        );
        0
    };
    drop(frame);
    Ok((code, producer))
}

/// Arms the test auto-close (if configured) once, from now.
fn schedule_close(auto_close: Option<Duration>, anchor: &str, close_at: &mut Option<Instant>) {
    if let Some(delay) = auto_close
        && close_at.is_none()
    {
        *close_at = Some(Instant::now() + delay);
        println!(
            "gate: test auto-close scheduled delay_ms={} anchor={anchor}",
            delay.as_millis()
        );
    }
}
