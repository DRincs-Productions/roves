//! Packaged-game mode (README "Packaged game"): an unmodified static game build in `game/`
//! next to the executable is served by the shared loopback server
//! (`support/cef-window-probe/src/server.rs`) and opened in the same sandboxed accelerated
//! OSR -> welding -> wgpu Vulkan -> SDL3 X11 path as the fixture gate. The Linux counterpart
//! of the macOS gate's `player.rs`; checked here:
//!
//! - the main frame's load-end on the loopback origin with an accepted status (Chromium's
//!   `WebContentsObserver::DidFinishLoad`, which CEF reports as `OnLoadEnd`), and no main
//!   frame committed off that origin;
//! - with a DMA-BUF import device: the first accelerated frame imported after that load-end
//!   (the OSR counterpart of `DidFirstVisuallyNonEmptyPaint`), presented in the SDL3 window,
//!   and a last frame with more than one colour;
//! - without one (CPU Vulkan adapter, e.g. a hosted runner): only that CEF delivered (or did
//!   not deliver) accelerated frames after the load-end. Nothing CEF painted is imported or
//!   shown; the window stays black and the run ends with exit 3, never a graphics PASS;
//! - the close path: an SDL quit or window close request ends the loop, then the caller
//!   closes the browser and waits for `OnBeforeClose` before `CefShutdown`.
//!
//! The browser uses welding's on-disk context `<root cache>/game-profile`: persistent in the
//! ordinary launch, per run with `--roves-isolated-profile` (`main.rs`). welding's Linux
//! producer accepts `user_data_dir` in `new` (it pumps the context initialisation itself),
//! as on Windows; the macOS two-step preparation is not needed.
//!
//! Nothing is injected into the page and no DevTools endpoint or remote-debugging port is
//! opened. The host API routes (`host_api.rs`, `core` + `saves`) are served on the loopback
//! origin; their `exit` ends this loop like a window close, and no input is forwarded yet (later slice).
//!
//! Test-only environment (CI): `ROVES_GATE_AUTO_CLOSE_MS` pushes one SDL quit event that long
//! after the first game frame (DMA-BUF device) or after the accepted load-end (no DMA-BUF
//! device, where no frame can be shown), so the ordinary close path runs unattended;
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
    NativeFrame, NavigationEvent,
    linux_cef::{LinuxCefConfig, LinuxCefProducer},
};

use crate::{
    game_rules::{load_status_ok, on_origin, player_profile_name},
    gate::{
        Gpu, Import, Pacer, distinct_colours, env_u64, frame_pixels_bgra, renderer_sandboxed,
        report_acquire_error, write_bmp,
    },
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
    window: &sdl3::video::Window,
    events: &mut sdl3::EventPump,
    sdl_events: &sdl3::EventSubsystem,
    gpu: &Gpu,
    origin: &str,
    profile_dir: &Path,
    timeout: Duration,
    game_exit: &std::sync::OnceLock<&'static str>,
) -> Result<(i32, LinuxCefProducer), String> {
    let auto_close = env_u64("ROVES_GATE_AUTO_CLOSE_MS").map(Duration::from_millis);
    let dump = std::env::var_os("ROVES_GATE_FRAME_DUMP").map(PathBuf::from);
    let size = gpu.size();
    let url = format!("{origin}/");
    let creating = Instant::now();
    let mut producer = LinuxCefProducer::new(
        runtime,
        LinuxCefConfig {
            surface: CefSurfaceConfig {
                initial_url: url.clone(),
                initial_size: size,
                scale_factor: window.display_scale(),
                user_data_dir: Some(profile_dir.to_path_buf()),
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
    let anchor = match gpu.import {
        Import::DmaBuf => "first-frame",
        Import::Unavailable => "load-end",
    };

    let started = Instant::now();
    let mut pacer = Pacer::new();
    let mut frame: Option<ImportedTexture> = None;
    let mut imported = 0u32;
    let mut delivered = 0u32;
    let mut delivered_after_load = 0u32;
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
                        // (welding's Linux `resize` is `WasResized`).
                        let requested = producer.resize(gpu.size());
                        println!("gate: fresh frame requested ok={}", requested.is_ok());
                        if gpu.import == Import::Unavailable {
                            schedule_close(auto_close, anchor, &mut close_at);
                        }
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
        match gpu.import {
            Import::DmaBuf => match producer.acquire_frame(&gpu.host) {
                Ok(Some(new)) => {
                    imported += 1;
                    if imported == 1 {
                        println!(
                            "gate: first accelerated frame imported size={}x{} format={:?} \
                             elapsed_ms={}",
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
                        let colours =
                            frame_pixels_bgra(&gpu.host, &new).map(|px| distinct_colours(&px));
                        println!(
                            "gate: first game frame after load-end ms={} size={}x{} \
                             format={:?} generation={} distinct_colours={}",
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
                        schedule_close(auto_close, anchor, &mut close_at);
                    }
                    frame = Some(new);
                },
                Ok(None) => {},
                Err(error) => {
                    report_acquire_error(&error.to_string());
                    failed = true;
                },
            },
            // Take (and drop, closing the duplicated fds) what CEF delivered so the count is
            // honest even though nothing can be imported or shown.
            Import::Unavailable => {
                if let Some(native) = producer.acquire_native_frame() {
                    delivered += 1;
                    if delivered == 1 {
                        println!(
                            "gate: first accelerated frame delivered (not imported: no dmabuf \
                             device) {} elapsed_ms={}",
                            describe(&native),
                            started.elapsed().as_millis()
                        );
                    }
                    if let Some(loaded) = loaded_at {
                        delivered_after_load += 1;
                        if delivered_after_load == 1 {
                            println!(
                                "gate: first game frame delivered after load-end ms={} (not \
                                 imported, not shown: no dmabuf device)",
                                loaded.elapsed().as_millis()
                            );
                        }
                    }
                }
            },
        }
        if gpu.present(frame.as_ref()) {
            presented += 1;
        }
        failed |= gpu.failed.load(Ordering::Relaxed);

        if let Some(at) = close_at
            && !close_pushed
            && Instant::now() >= at
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
        // Without an import device only the load-end can end the wait; with one, the first
        // game frame (as on macOS).
        let waiting = match gpu.import {
            Import::DmaBuf => first_game_frame.is_none(),
            Import::Unavailable => loaded_at.is_none(),
        };
        if waiting && started.elapsed() >= timeout {
            timed_out = true;
            break;
        }
        pacer.wait();
    }

    let elapsed = started.elapsed();
    println!(
        "gate: loop end imported={imported} delivered_unimported={delivered} \
         delivered_after_load={delivered_after_load} presented={presented} loaded={} \
         game_frame={} close={} elapsed_ms={}",
        loaded_at.is_some(),
        first_game_frame.is_some(),
        close_source.unwrap_or("none"),
        elapsed.as_millis()
    );
    println!("gate: console lines={console_lines} by_level={console_levels:?}");
    if let Some(source) = close_source {
        println!("gate: close requested source={source}");
    }

    let mut no_frame = false;
    if close_source.is_some() && !failed {
        match gpu.import {
            Import::DmaBuf if first_game_frame.is_none() => {
                println!("gate: FAIL closed before the first game frame");
                failed = true;
            },
            Import::Unavailable if loaded_at.is_none() => {
                println!("gate: FAIL closed before the game load-end");
                failed = true;
            },
            _ => {},
        }
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
    } else if gpu.import == Import::Unavailable {
        println!(
            "gate: NO_DMABUF_DEVICE packaged game loaded on a CPU adapter (frames delivered \
             after load-end={delivered_after_load}, none importable, window black): load, \
             sandbox and close verified; game graphics NOT verified"
        );
        3
    } else if no_frame {
        println!(
            "gate: NO_ACCELERATED_FRAME within {}s (game loaded, no OnAcceleratedPaint)",
            timeout.as_secs()
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

/// Size, format, plane count and DRM modifier of a delivered frame that is not imported.
fn describe(frame: &NativeFrame) -> String {
    let size = frame.size();
    let planes = match frame {
        NativeFrame::DmaBufImage(image) => {
            format!(
                "planes={} modifier={:#x}",
                image.planes().len(),
                image.modifier()
            )
        },
        _ => "planes=? modifier=?".to_owned(),
    };
    format!(
        "size={}x{} format={:?} {planes}",
        size.width,
        size.height,
        frame.format()
    )
}
