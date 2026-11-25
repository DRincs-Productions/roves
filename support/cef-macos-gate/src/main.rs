//! macOS feasibility gate: CEF 154 accelerated OSR (`OnAcceleratedPaint`, IOSurface) ->
//! welding Metal import -> wgpu 29 Metal device that also presents to an SDL3 window, with
//! Chromium's sandbox on. No CEF windowed mode and no software (`OnPaint`) path exist here.
//!
//! Exit codes (README "Verdicts"):
//! - 0: an imported CEF frame matched the fixture colour and the renderer was sandboxed.
//! - 2: no accelerated frame arrived before the timeout (e.g. a VM where Chromium disables
//!   GPU compositing), but nothing failed and the renderer was sandboxed.
//! - 1: any failure (CEF/SDL/wgpu setup, wrong pixels, wgpu error, content process death,
//!   renderer not sandboxed or not found).
//!
//! Every verdict line starts with `gate:` on stdout; CI checks those lines.

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
    const WINDOW: (u32, u32) = (640, 400);
    const CLOSE_PUMP: Duration = Duration::from_millis(1500);

    struct Logger;

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                println!(
                    "gate: log {} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            }
        }
        fn flush(&self) {}
    }

    pub fn run() -> i32 {
        let _ = log::set_logger(&Logger).map(|()| log::set_max_level(log::LevelFilter::Info));
        match run_inner() {
            Ok(code) => code,
            Err(error) => {
                println!("gate: FAIL {error}");
                1
            },
        }
    }

    fn run_inner() -> Result<i32, String> {
        let timeout = Duration::from_secs(
            std::env::var("ROVES_GATE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        );
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        // <app>.app/Contents/MacOS/<exe> -> <app>.app/Contents/Frameworks
        let frameworks = exe
            .parent()
            .and_then(|macos| macos.parent())
            .map(|contents| contents.join("Frameworks"))
            .ok_or("cannot resolve Contents/Frameworks")?;
        if !frameworks
            .join("Chromium Embedded Framework.framework")
            .exists()
        {
            return Err(format!(
                "no CEF framework under {}: run from the .app built by `bundle`",
                frameworks.display()
            ));
        }

        let cache =
            std::env::temp_dir().join(format!("roves-cef-macos-gate-{}", std::process::id()));
        let mut config = CefRuntimeConfig::new(&frameworks, CefSandboxMode::Sandboxed);
        config.cache_path = Some(cache.clone());
        // Unattended: Chromium's Safe Storage would otherwise ask for the login keychain
        // password and block CefInitialize (same switch as wgpu-weld's macOS demo).
        config
            .command_line_switches
            .push(("use-mock-keychain".to_owned(), None));
        // CEF first, SDL second: the order of wgpu-weld's macOS demo (CEF, then the windowing
        // library that creates NSApp).
        let runtime = CefRuntime::initialize(config).map_err(|e| format!("CEF initialize: {e}"))?;
        println!(
            "gate: cef initialized sandbox=Sandboxed cache={}",
            cache.display()
        );

        let code = {
            let sdl = sdl3::init().map_err(|e| format!("SDL init: {e}"))?;
            let video = sdl.video().map_err(|e| format!("SDL video: {e}"))?;
            let mut events = sdl.event_pump().map_err(|e| format!("SDL events: {e}"))?;
            let window = video
                .window("Roves CEF macOS gate", WINDOW.0, WINDOW.1)
                .high_pixel_density()
                .build()
                .map_err(|e| format!("SDL window: {e}"))?;
            println!(
                "gate: sdl window pixels={:?} scale={}",
                window.size_in_pixels(),
                window.display_scale()
            );
            let result = drive(&runtime, &window, &mut events, timeout);
            // Close the browser and let CEF finish it before cef::shutdown (runtime drop).
            // welding's macOS producer exposes no close-completion signal (Windows has
            // `is_closed`), so pump for a bounded time.
            let code = match result {
                Ok((code, mut producer)) => {
                    let _ = producer.close();
                    let until = Instant::now() + CLOSE_PUMP;
                    while Instant::now() < until {
                        for _ in events.poll_iter() {}
                        runtime.do_message_loop_work();
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    drop(producer);
                    code
                },
                Err(error) => {
                    println!("gate: FAIL {error}");
                    1
                },
            };
            drop(window);
            code
        };
        drop(runtime);
        let _ = std::fs::remove_dir_all(&cache);
        println!("gate: shutdown complete exit={code}");
        Ok(code)
    }

    fn drive(
        runtime: &CefRuntime,
        window: &sdl3::video::Window,
        events: &mut sdl3::EventPump,
        timeout: Duration,
    ) -> Result<(i32, MacosCefProducer), String> {
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // welding's macOS import is Metal only (IOSurface -> MTLTexture -> grafting).
        instance_desc.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(instance_desc);
        // SAFETY: SDL owns the NSWindow/NSView; the surface is dropped at the end of this
        // function, before the caller drops the window, all on the main thread.
        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window)
                .map_err(|e| format!("SDL3 window handle: {e}"))?;
            instance.create_surface_unsafe(target)
        }
        .map_err(|e| format!("surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
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
        let wgpu_failed = Arc::new(AtomicBool::new(false));
        host.device.on_uncaptured_error(Arc::new({
            let failed = wgpu_failed.clone();
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
        let mut surface_config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface not supported by the adapter")?;
        surface_config.format = format;
        surface_config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&host.device, &surface_config);
        let blitter = wgpu::util::TextureBlitter::new(&host.device, format);

        let mut producer = MacosCefProducer::new(
            runtime,
            MacosCefConfig {
                surface: CefSurfaceConfig {
                    initial_url: FIXTURE_URL.to_owned(),
                    initial_size: PhysicalSize::new(width, height),
                    scale_factor: window.display_scale(),
                    ..Default::default()
                },
            },
        )
        .map_err(|e| format!("CEF producer: {e}"))?;
        println!(
            "gate: producer created mode={:?} size={width}x{height}",
            producer.surface_mode()
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
            match producer.acquire_frame(&host) {
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
                    match centre_matches(&host, &new) {
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
            if !present(&surface, &surface_config, &host, &blitter, frame.as_ref()) {
                std::thread::sleep(Duration::from_millis(4));
            }
            failed |= wgpu_failed.load(Ordering::Relaxed);
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
        drop(surface);
        Ok((code, producer))
    }

    /// Copies the centre `SAMPLE` x `SAMPLE` block of the imported texture (grafting creates
    /// it with `COPY_SRC`) and compares every texel with the fixture colour.
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
        let row = SAMPLE * 4;
        let buffer = host.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gate readback"),
            size: u64::from(row * SAMPLE),
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
                origin: wgpu::Origin3d {
                    x: (w - SAMPLE) / 2,
                    y: (h - SAMPLE) / 2,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(SAMPLE),
                },
            },
            wgpu::Extent3d {
                width: SAMPLE,
                height: SAMPLE,
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
        let ok = slice
            .get_mapped_range()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| {
                px.iter()
                    .zip(EXPECTED_BGRA)
                    .all(|(&a, b)| a.abs_diff(b) <= TOLERANCE)
            });
        buffer.unmap();
        Ok(ok)
    }

    /// Shows the latest imported frame in the SDL3 window (stretched) or black before the
    /// first frame. Returns whether a texture was presented (vsync then paces the loop).
    fn present(
        surface: &wgpu::Surface<'_>,
        config: &wgpu::SurfaceConfiguration,
        host: &HostWgpuContext,
        blitter: &wgpu::util::TextureBlitter,
        frame: Option<&ImportedTexture>,
    ) -> bool {
        let output = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                surface.configure(&host.device, config);
                return false;
            },
            _ => return false,
        };
        let target = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = host
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gate present"),
            });
        match frame {
            Some(frame) => blitter.copy(&host.device, &mut encoder, &frame.view, &target),
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
        host.queue.submit([encoder.finish()]);
        output.present();
        true
    }

    /// Chromium's macOS launcher appends `--seatbelt-client=<fd>` to every child it starts
    /// sandboxed and nothing when `--no-sandbox` or an unsandboxed type applies
    /// (`content/browser/child_process_launcher_helper_mac.cc`, README "Chromium reference").
    /// Requires at least one renderer helper of this bundle, all of them with the switch and
    /// none with `--no-sandbox`; GPU and utility helpers are reported only.
    fn renderer_sandboxed() -> bool {
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
