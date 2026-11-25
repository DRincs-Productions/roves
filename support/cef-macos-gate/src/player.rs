//! Packaged-game mode (README "Packaged game"): an unmodified static game build inside
//! `Contents/Resources/game` is served by the shared loopback server
//! (`support/cef-window-probe/src/server.rs`) and opened in the same sandboxed accelerated
//! OSR -> welding -> wgpu Metal -> SDL3 path as the fixture gate. Checked here:
//!
//! - the main frame's load-end on the loopback origin with an accepted status (Chromium's
//!   `WebContentsObserver::DidFinishLoad`, which CEF reports as `OnLoadEnd`);
//! - the first accelerated frame imported after that load-end (the OSR counterpart of
//!   `DidFirstVisuallyNonEmptyPaint`), presented in the SDL3 window;
//! - the close path: an SDL quit or window close request ends the loop, then the caller
//!   closes the browser and waits for `OnBeforeClose` before `CefShutdown`.
//!
//! The browser uses welding's on-disk context `<root cache>/game-profile`: persistent in the
//! ordinary launch, per run with `--roves-isolated-profile` (`src/profile.rs`, `main.rs`).
//!
//! Nothing is injected into the page and no DevTools endpoint or remote-debugging port is
//! opened. The host API routes (`host_api.rs`, `core` + `saves`) are served on the loopback
//! origin; their `exit` ends this loop like a window close, and no input is forwarded yet (later slice).
//!
//! Test-only environment (CI): `ROVES_GATE_AUTO_CLOSE_MS` pushes one SDL quit event that long
//! after the first game frame, so the ordinary close path runs unattended;
//! `ROVES_GATE_FRAME_DUMP=<file.bmp>` writes the last frame before close.

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
    macos_cef::{MacosCefConfig, MacosCefProducer},
};

use crate::{
    game_rules::{load_status_ok, on_origin},
    gate::{Gpu, distinct_colours, env_u64, frame_pixels, renderer_sandboxed, write_bmp},
};

/// Console lines printed before only counting them.
const CONSOLE_LOG_CAP: u32 = 200;
/// Longest console message kept per line (characters).
const CONSOLE_MESSAGE_MAX: usize = 300;

/// `profile_dir` is welding's on-disk context for the game (`<root cache>/game-profile`), in
/// the persistent profile and in the isolated per-run cache alike, as on Windows. welding's
/// macOS producer refuses `user_data_dir` in `new`: CEF initializes such a context
/// asynchronously, so it is prepared first and the browser is created once CEF reports it
/// ready (`prepare_profile` / `try_new_with_prepared_profile`, pumped from this loop).
#[allow(clippy::too_many_arguments)]
pub(crate) fn drive(
    runtime: &CefRuntime,
    window: &sdl3::video::Window,
    events: &mut sdl3::EventPump,
    sdl_events: &sdl3::EventSubsystem,
    gpu: &Gpu,
    origin: &str,
    profile_dir: &Path,
    timeout: Duration,
    game_exit: &std::sync::OnceLock<&'static str>,
) -> Result<(i32, MacosCefProducer), String> {
    let auto_close = env_u64("ROVES_GATE_AUTO_CLOSE_MS").map(Duration::from_millis);
    let dump = std::env::var_os("ROVES_GATE_FRAME_DUMP").map(PathBuf::from);
    let size = gpu.size();
    let url = format!("{origin}/");
    let config = MacosCefConfig {
        surface: CefSurfaceConfig {
            initial_url: url.clone(),
            initial_size: size,
            scale_factor: window.display_scale(),
            user_data_dir: Some(profile_dir.to_path_buf()),
            ..Default::default()
        },
    };
    let preparing = Instant::now();
    let mut prepared = MacosCefProducer::prepare_profile(runtime, &config)
        .map_err(|e| format!("CEF game profile: {e}"))?;
    let mut producer = loop {
        for event in events.poll_iter() {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::Window {
                        win_event: WindowEvent::CloseRequested,
                        ..
                    }
            ) {
                return Err("closed before the game profile was ready".into());
            }
        }
        runtime.do_message_loop_work();
        if let Some(producer) =
            MacosCefProducer::try_new_with_prepared_profile(&config, &mut prepared)
                .map_err(|e| format!("CEF producer: {e}"))?
        {
            break producer;
        }
        if preparing.elapsed() >= timeout {
            return Err(format!(
                "game profile not ready within {}s",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(4));
    };
    drop(prepared);
    println!(
        "gate: game profile ready dir={} elapsed_ms={}",
        profile_dir.display(),
        preparing.elapsed().as_millis()
    );
    println!(
        "gate: producer created mode={:?} size={}x{} url={url}",
        producer.surface_mode(),
        size.width,
        size.height
    );

    let started = Instant::now();
    let mut frame: Option<ImportedTexture> = None;
    let mut imported = 0u32;
    let mut loaded_at: Option<Instant> = None;
    let mut first_game_frame: Option<Instant> = None;
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
                _ => {},
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
        while let Some(event) = producer.poll_web_event() {
            let CefSurfaceEvent::Navigation(event) = event else {
                continue;
            };
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
                        println!(
                            "gate: game loaded elapsed_ms={}",
                            started.elapsed().as_millis()
                        );
                        // Re-send the size so an already static page paints once more
                        // (welding's macOS `resize` is `WasResized`).
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
                // the loopback origin fails the run (as on Windows). History API and
                // fragment routes stay on the origin.
                NavigationEvent::AddressChanged { url } => {
                    if on_origin(origin, &url) {
                        println!("gate: address {url}");
                    } else {
                        println!("gate: FAIL main frame left the game origin url={url}");
                        failed = true;
                    }
                },
                NavigationEvent::TitleChanged { title } => println!("gate: title {title:?}"),
                NavigationEvent::ConsoleMessage {
                    level,
                    message,
                    source,
                    line,
                } => {
                    console_lines += 1;
                    *console_levels.entry(level).or_default() += 1;
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
                        "gate: first accelerated frame size={}x{} format={:?} elapsed_ms={}",
                        new.size.width,
                        new.size.height,
                        new.format,
                        started.elapsed().as_millis()
                    );
                }
                if let Some(loaded) = loaded_at
                    && first_game_frame.is_none()
                {
                    first_game_frame = Some(Instant::now());
                    let colours = frame_pixels(&gpu.host, &new).map(|px| distinct_colours(&px));
                    println!(
                        "gate: first game frame after load-end ms={} size={}x{} format={:?} \
                         generation={} distinct_colours={}",
                        loaded.elapsed().as_millis(),
                        new.size.width,
                        new.size.height,
                        new.format,
                        new.generation,
                        colours
                            .as_ref()
                            .map_or_else(|e| format!("error({e})"), usize::to_string)
                    );
                    if let Err(error) = colours {
                        println!("gate: FAIL readback: {error}");
                        failed = true;
                    }
                    if let Some(delay) = auto_close {
                        println!(
                            "gate: test auto-close scheduled delay_ms={}",
                            delay.as_millis()
                        );
                    }
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

        if let (Some(at), Some(delay)) = (first_game_frame, auto_close)
            && !close_pushed
            && at.elapsed() >= delay
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
        if first_game_frame.is_none() && started.elapsed() >= timeout {
            timed_out = true;
            break;
        }
    }

    println!(
        "gate: loop end imported={imported} loaded={} game_frame={} close={} elapsed_ms={}",
        loaded_at.is_some(),
        first_game_frame.is_some(),
        close_source.unwrap_or("none"),
        started.elapsed().as_millis()
    );
    println!("gate: console lines={console_lines} by_level={console_levels:?}");
    if let Some(source) = close_source {
        println!("gate: close requested source={source}");
    }

    let mut no_frame = false;
    if close_source.is_some() && first_game_frame.is_none() && !failed {
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
    // The last frame shown before close: evidence of what the game displayed.
    if !failed && first_game_frame.is_some() {
        match frame.as_ref().map(|f| (f, frame_pixels(&gpu.host, f))) {
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
    // Taken while the browser is alive, so the helper processes are still running.
    let sandboxed = renderer_sandboxed();
    let code = if failed || !sandboxed {
        1
    } else if no_frame {
        println!(
            "gate: NO_ACCELERATED_FRAME within {}s (game loaded, no OnAcceleratedPaint)",
            timeout.as_secs()
        );
        2
    } else {
        println!("gate: PASS packaged game loaded, first frame shown, close requested");
        0
    };
    drop(frame);
    Ok((code, producer))
}
