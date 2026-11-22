//! Browser process of the probe: one wgpu 29 DX12 device booted by Netrender for the window
//! surface, shared with welding's Windows CEF producer. Each frame CEF paints the fixture
//! through accelerated OSR, welding copies the callback-scoped shared texture on the GPU and
//! grafting imports it on the same device; Netrender renders its own scene (the "wgpu
//! background") and composites the imported texture over it as a premultiplied external
//! texture; the master is drawn onto the surface by a GPU blit. No CPU copy of the page or of
//! the frame is made to present it: the only CPU readbacks are the verification samples taken
//! at the two check points, after the frame was presented.

use std::{
    fs::File,
    io::Write as _,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use crate::game::{Console, GameOptions, Input, Server};
use crate::game_rules::{load_status_ok, on_origin};
use crate::stall::{Marks, Poll, Ring, Step};
use crate::timings::{Stage, Timings};
use base64::Engine as _;
use netrender::{
    Compositor, ExternalTextureComposite, ExternalTexturePlacement, NetrenderOptions,
    PresentedFrame, Renderer, Scene, SourceAlpha, TenantNeeds,
};
use welding::{
    CefRuntime, CefRuntimeConfig, CefSandboxMode, CefSurfaceConfig, CefSurfaceEvent,
    CefSurfaceProducer as _, CefWindowsSandboxContext, HostWgpuContext, ImportedTexture,
    InteropBackend, NavigationEvent,
    windows_cef::{WindowsCefConfig, WindowsCefProducer},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

const FIXTURE_HTML: &str = include_str!("fixture.html");
const BLIT_WGSL: &str = include_str!("blit.wgsl");
const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize::new(1280, 720);
const RESIZED_SIZE: PhysicalSize<u32> = PhysicalSize::new(960, 540);
/// Composed (Netrender master / plain-wgpu target) format: Netrender's master is a storage
/// texture, which rules out BGRA without an extra feature.
const COMPOSED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Imported frames of the expected size required before a check (the page must have painted
/// its content, not only the initial transparent frame).
const FRAMES_BEFORE_CHECK: u32 = 20;
/// Per-channel tolerance for 8-bit rounding of the 50% layer.
const TOLERANCE: u8 = 3;
const BACKGROUND: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const NETRENDER_PANEL: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
/// `--roves-sustained`: longest gap between two imported producer frames during warmup or
/// sample before the run is declared stalled. The fixture repaints every 16 ms.
const SUSTAINED_STALL: Duration = Duration::from_millis(500);
/// `--roves-sustained` only: welding/grafting `log` records at warn or error level forwarded
/// to the evidence log, at most this many (the rest are only counted).
const UPSTREAM_LOG_CAP: u32 = 64;
/// Warmup start, shared with the `log` forwarder (CEF's UI thread logs too).
static WARMUP_ORIGIN: OnceLock<Instant> = OnceLock::new();
static UPSTREAM_LOG_RECORDS: AtomicU32 = AtomicU32::new(0);

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
        eprintln!("roves-cef-nr cannot locate the bundled CEF distribution");
        return 1;
    };
    // A null sandbox_info (bootstrap.exe without a broker) is refused by welding: the probe
    // has no unsandboxed path.
    // SAFETY: forwarded from bootstrap.exe, which keeps both values valid for this call.
    let context = match unsafe { CefWindowsSandboxContext::from_raw(instance, sandbox_info) } {
        Ok(context) => context,
        Err(error) => {
            eprintln!("roves-cef-nr sandbox-info present=false error={error}");
            return 3;
        },
    };
    match context.execute_process(&cef_path) {
        Ok(Some(code)) => return code,
        Ok(None) => {},
        Err(error) => {
            eprintln!("roves-cef-nr execute-process failed: {error}");
            return 3;
        },
    }

    let options = match Options::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("roves-cef-nr usage error: {error}");
            return 1;
        },
    };
    let log = match Log::open(options.log.as_deref()) {
        Ok(log) => log,
        Err(error) => {
            eprintln!("roves-cef-nr cannot open the log: {error}");
            return 1;
        },
    };
    log.line(format_args!("sandbox-info present=true mode=sandboxed"));
    log.line(format_args!(
        "versions welding=0.15.0@e991684 cef={} netrender=d2f13ca5 wgpu=29.0.4 compose={}",
        cef_version(),
        options.compose.name()
    ));
    if options.sustained {
        UpstreamLog::install(log.clone());
    }

    // Game mode: bind the loopback origin before CEF starts (the window probe's order).
    let server = match options.game.as_ref() {
        Some(game) => match crate::game::server::start(&game.dir, game.port) {
            Ok(server) => {
                log.line(format_args!(
                    "game loopback origin={} game-dir={} profile-dir={} browser_pid={}",
                    server.origin,
                    game.dir.display(),
                    crate::game::profile_dir(&options.cache_dir).display(),
                    std::process::id()
                ));
                Some(server)
            },
            Err(error) => {
                log.line(format_args!("FAIL game loopback server: {error}"));
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
            stop_server(&log, server);
            return 4;
        },
    };
    log.line(format_args!(
        "cef initialized cache={}",
        options.cache_dir.display()
    ));

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log.line(format_args!("event loop failed: {error}"));
            drop(runtime);
            stop_server(&log, server);
            return 1;
        },
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let phase = if options.game.is_some() {
        Phase::Game
    } else {
        Phase::Initial
    };
    let mut probe = Probe {
        timings: (options.timings || options.sustained).then(Timings::default),
        phase_since: None,
        last_import: None,
        last_generation: None,
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
        failures: 0,
        checks: 0,
        exit_code: None,
        started: Instant::now(),
        closing_since: None,
    };
    if let Err(error) = event_loop.run_app(&mut probe) {
        log.line(format_args!("event loop error: {error}"));
        probe.finish();
        return 1;
    }
    let code = probe.finish();
    log.line(format_args!("exit code={code}"));
    code
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Compose {
    /// Netrender d2f13ca5 renders the background scene and composites the CEF texture.
    Netrender,
    /// Same import, composed by one premultiplied blit over a cleared target. Exists because
    /// Netrender/Vello 0.9 crashes on WARP (support/netrender-shared-device-proof/README.md),
    /// the only adapter of GPU-less CI runners.
    Wgpu,
}

impl Compose {
    fn name(self) -> &'static str {
        match self {
            Compose::Netrender => "netrender",
            Compose::Wgpu => "wgpu",
        }
    }
}

struct Options {
    log: Option<PathBuf>,
    cache_dir: PathBuf,
    compose: Compose,
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
            cache_dir: std::env::temp_dir()
                .join("roves-cef-netrender-probe")
                .join(format!(
                    "run-{}-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_millis()),
                    std::process::id()
                )),
            compose: Compose::Netrender,
            timeout: Duration::from_secs(120),
            keep_open: false,
            timings: false,
            sustained: false,
            warmup: Duration::from_secs(10),
            sample: Duration::from_secs(30),
            game: None,
            auto_close: None,
        };
        let mut game_dir = None;
        let mut port = None;
        for arg in std::env::args().skip(1) {
            let Some(rest) = arg.strip_prefix("--roves-") else {
                continue; // CEF/Chromium switches are CEF's business.
            };
            let (name, value) = rest.split_once('=').unwrap_or((rest, ""));
            match name {
                "log" => options.log = Some(value.into()),
                "cache-dir" => options.cache_dir = value.into(),
                "compose" => {
                    options.compose = match value {
                        "netrender" => Compose::Netrender,
                        "wgpu" => Compose::Wgpu,
                        _ => return Err(format!("--roves-compose={value}: netrender|wgpu")),
                    }
                },
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
                _ => return Err(format!("unknown option --roves-{name}")),
            }
        }
        if options.sustained && options.keep_open {
            return Err("--roves-sustained closes after its sample: drop --roves-keep-open".into());
        }
        if options.sample.is_zero() {
            return Err("--roves-sample-ms must be > 0".into());
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
            None if port.is_some() || options.auto_close.is_some() => {
                return Err("--roves-port/--roves-auto-close-ms need --roves-game-dir".into());
            },
            None => {},
        }
        Ok(options)
    }
}

/// Evidence log: one `roves-cef-nr <event>` line per event, to the file and to stderr.
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
        let line = format!("roves-cef-nr {args}\n");
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
    started: Instant,
    closing_since: Option<Instant>,
    /// Present only with `--roves-timings` or `--roves-sustained`.
    timings: Option<Timings>,
    /// Start of the current warmup or sample.
    phase_since: Option<Instant>,
    /// Last imported producer frame during warmup/sample (stall detection).
    last_import: Option<Instant>,
    /// welding paint generation of the last imported frame (any phase).
    last_generation: Option<u64>,
    /// `--roves-sustained` only: redraw ring of warmup and sample, printed on rejection.
    stall: Option<Box<Ring>>,
    /// The sustained sample completed and was healthy.
    sustained_done: bool,
    /// Game mode: the loopback origin, stopped after CEF shutdown.
    server: Option<Server>,
    /// Game mode: first accepted main-frame load-end on the loopback origin.
    game_loaded_at: Option<Instant>,
    console: Console,
}

/// How long the browser may take to close after `close()` before the probe gives up.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Field order is drop order: the producer goes before the device it imports into and before
/// the CEF runtime (`cef::shutdown` on drop).
struct State {
    producer: WindowsCefProducer,
    frame: Option<ImportedTexture>,
    composer: Composer,
    blit: Blit,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    host: HostWgpuContext,
    window: Arc<Window>,
    runtime: CefRuntime,
    imported: u32,
    imported_at_size: u32,
    presented: u32,
    load_ended: bool,
    /// Game mode only: winit -> welding input forwarding.
    input: Option<Input>,
}

enum Composer {
    Netrender(Box<Renderer>),
    Wgpu(Option<wgpu::Texture>),
}

impl Probe {
    fn fail(&mut self, what: std::fmt::Arguments<'_>) {
        self.failures += 1;
        self.log.line(format_args!("FAIL {what}"));
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<State, String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Roves CEF -> wgpu-weld -> Netrender probe")
                        .with_inner_size(INITIAL_SIZE)
                        // Sustained samples are taken at the fixed initial size.
                        .with_resizable(!self.options.sustained),
                )
                .map_err(|e| format!("window: {e}"))?,
        );
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // Welding's Windows import is DX12 only (D3D12 OpenSharedHandle through grafting).
        instance_desc.backends = wgpu::Backends::DX12;
        let instance = wgpu::Instance::new(instance_desc);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("surface: {e}"))?;
        // One device for Netrender and the CEF import, booted through Netrender's tenancy
        // contract (its limits raised over wgpu's defaults, no extra features).
        let handles = netrender::boot_on(
            instance,
            Some(&surface),
            &TenantNeeds {
                label: Some("roves-cef-netrender-probe"),
                ..Default::default()
            },
        )
        .map_err(|e| format!("netrender boot_on: {e:?}"))?;
        let info = handles.adapter.get_info();
        self.log.line(format_args!(
            "adapter name={:?} backend={:?} type={:?} driver={:?} {:?}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        ));
        let host = HostWgpuContext::new(handles.device.clone(), handles.queue.clone());
        if host.backend != InteropBackend::Dx12 {
            return Err(format!("interop backend {:?}, expected Dx12", host.backend));
        }
        host.device.on_uncaptured_error(Arc::new({
            let log = self.log.clone();
            move |error| log.line(format_args!("FAIL wgpu uncaptured error: {error}"))
        }));

        let caps = surface.get_capabilities(&handles.adapter);
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
        let size = window.inner_size();
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
            window.scale_factor()
        ));

        let blit = Blit::new(&host.device, format);
        let composer = match self.options.compose {
            Compose::Netrender => {
                let renderer = netrender::create_netrender_instance(
                    handles,
                    NetrenderOptions {
                        tile_cache_size: Some(64),
                        enable_vello: true,
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("create_netrender_instance: {e:?}"))?;
                self.log.line(format_args!(
                    "netrender renderer created on the shared device"
                ));
                Composer::Netrender(Box::new(renderer))
            },
            Compose::Wgpu => Composer::Wgpu(None),
        };

        let runtime = self.runtime.take().ok_or("CEF runtime already consumed")?;
        let game_origin = self.server.as_ref().map(|server| server.origin.clone());
        let game = game_origin.is_some();
        let url = match game_origin {
            Some(origin) => format!("{origin}/"),
            None => format!(
                "data:text/html;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(FIXTURE_HTML)
            ),
        };
        let producer = WindowsCefProducer::new(
            &runtime,
            WindowsCefConfig {
                surface: CefSurfaceConfig {
                    initial_url: url,
                    initial_size: size,
                    scale_factor: window.scale_factor() as f32,
                    // Fixture: transparent, uncovered page pixels arrive with premultiplied
                    // alpha 0. Game: opaque white where the page paints nothing, as in Chrome.
                    background_color: game.then_some([255, 255, 255]),
                    prefer_accelerated: true,
                    // Game: a per-producer on-disk profile inside the run's root cache. The
                    // fixture keeps welding's isolated in-memory profile, unchanged.
                    user_data_dir: game.then(|| crate::game::profile_dir(&self.options.cache_dir)),
                    ..Default::default()
                },
            },
            &host,
        )
        .map_err(|e| format!("WindowsCefProducer: {e}"))?;
        self.log.line(format_args!(
            "producer created accelerated=true background={}",
            if game { "white" } else { "transparent" }
        ));
        window.request_redraw();
        Ok(State {
            producer,
            frame: None,
            composer,
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
            input: game.then(Input::new),
        })
    }

    fn redraw(&mut self) {
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
        let mut timings = self.timings.as_mut().filter(|_| record);
        // Stall ring: warmup and sample redraws, independent of `record`, own clock reads.
        let sustained_phase = matches!(self.phase, Phase::Warmup | Phase::Sampling);
        let mut marks =
            (self.stall.is_some() && sustained_phase).then(|| Marks::new(Instant::now()));
        let frame_start = now_if(&timings);
        let mut events = Vec::new();
        while let Some(event) = s.producer.poll_web_event() {
            events.push(event);
        }
        lap(&mut timings, Stage::Events, frame_start);
        // Game mode only (`None` keeps the fixture's event handling unchanged).
        let origin = self.server.as_ref().map(|server| server.origin.clone());
        let mut left_origin = None;
        for event in events {
            if let CefSurfaceEvent::Navigation(event) = event {
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
                        } else if self.game_loaded_at.is_none() {
                            s.load_ended = true;
                            self.game_loaded_at = Some(Instant::now());
                            if let Some(delay) = self.options.auto_close {
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
                    },
                    // welding exposes no pre-navigation veto (no OnBeforeBrowse hook), so a
                    // main frame that commits off the loopback origin fails the run and
                    // closes the browser. History API and fragment routes stay on origin.
                    NavigationEvent::AddressChanged { url } if origin.is_some() => {
                        if on_origin(origin.as_deref().unwrap_or_default(), url) {
                            self.log.line(format_args!("address {url}"));
                        } else {
                            left_origin = Some(url.clone());
                        }
                    },
                    NavigationEvent::LoadStart { url } if origin.is_some() => {
                        self.log.line(format_args!("load-start url={url}"));
                    },
                    // welding already cancelled it: one surface per producer, no popups.
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
                    },
                    NavigationEvent::TitleChanged { title } if origin.is_some() => {
                        self.log.line(format_args!("title {title}"));
                        s.window.set_title(title);
                    },
                    NavigationEvent::LoadError {
                        error_code,
                        error_text,
                        ..
                    } => {
                        self.log
                            .line(format_args!("FAIL load-error {error_code} {error_text}"));
                        self.failures += 1;
                    },
                    NavigationEvent::TitleChanged { title } => {
                        self.log.line(format_args!("title {title}"));
                    },
                    NavigationEvent::ContentProcessTerminated { status, .. } => {
                        self.log
                            .line(format_args!("FAIL renderer terminated {status:?}"));
                        self.failures += 1;
                    },
                    _ => {},
                }
            }
        }
        if let Some(url) = left_origin {
            self.fail(format_args!(
                "game main frame left the loopback origin: {url}"
            ));
            self.close();
            return;
        }
        if let Some(input) = s.input.as_mut()
            && let Some(line) = input.per_frame(&mut s.producer, &s.window, s.imported > 0)
        {
            self.log.line(format_args!("{line}"));
        }

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
        match acquired {
            Ok(Some(frame)) => {
                s.imported += 1;
                self.last_generation = Some(frame.generation);
                if matches!(self.phase, Phase::Warmup | Phase::Sampling) {
                    self.last_import = Some(Instant::now());
                }
                let expected = (s.surface_config.width, s.surface_config.height);
                let size = (frame.size.width, frame.size.height);
                if size == expected {
                    s.imported_at_size += 1;
                }
                if s.imported == 1 || s.imported_at_size == 1 {
                    self.log.line(format_args!(
                        "import frame={} size={}x{} format={:?} generation={} usage={:?} path=dx12-shared-handle",
                        s.imported,
                        size.0,
                        size.1,
                        frame.format,
                        frame.generation,
                        frame.texture.usage()
                    ));
                }
                s.frame = Some(frame);
            },
            Ok(None) => {},
            Err(error) => self.log.line(format_args!("acquire_frame error: {error}")),
        }

        let composed = compose(s, timings.as_deref_mut());
        mark(&mut marks, Step::Compose);
        let sampling = self.phase == Phase::Sampling;
        let Some(composed) = composed else {
            if let (Some(ring), Some(marks)) = (self.stall.as_mut(), marks.as_ref()) {
                ring.push(marks, poll, false, sampling);
            }
            s.window.request_redraw();
            return;
        };
        let presented = present(s, &composed, timings.as_deref_mut(), marks.as_mut());
        if presented {
            s.presented += 1;
            if let (Some(t), Some(frame_start)) = (timings, frame_start) {
                let now = t.lap(Stage::Frame, frame_start);
                t.presented(now);
            }
        }
        // Before `sustained_step`, so the redraw that trips the stall check is in the ring.
        if let (Some(ring), Some(marks)) = (self.stall.as_mut(), marks.as_ref()) {
            ring.push(marks, poll, presented, sampling);
        }

        let ready = s.load_ended && s.imported_at_size >= FRAMES_BEFORE_CHECK;
        let timed_out = self.started.elapsed() > self.options.timeout;
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
                    let applied = s.window.request_inner_size(RESIZED_SIZE);
                    self.log.line(format_args!(
                        "resize requested {}x{} applied={applied:?}",
                        RESIZED_SIZE.width, RESIZED_SIZE.height
                    ));
                    if let Some(size) = applied {
                        self.resized(size);
                    }
                    self.phase = Phase::Resizing;
                }
            },
            Phase::Resizing if ready => {
                self.report_smoke_timings("resized");
                self.check("resized", &composed);
                self.close();
            },
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
            // Not bounded by --roves-timeout-ms: bounded by warmup + sample, and stall-checked.
            Phase::Warmup | Phase::Sampling => self.sustained_step(),
            // Game: --roves-timeout-ms bounds only the load; afterwards the player (or
            // --roves-auto-close-ms) decides when to close.
            Phase::Game if self.game_loaded_at.is_none() && timed_out => {
                self.fail(format_args!(
                    "timeout: the game did not finish loading in {} ms",
                    self.options.timeout.as_millis()
                ));
                self.exit_code.get_or_insert(2);
                self.close();
            },
            Phase::Game
                if self
                    .options
                    .auto_close
                    .zip(self.game_loaded_at)
                    .is_some_and(|(delay, at)| at.elapsed() >= delay) =>
            {
                self.log.line(format_args!("auto-close window"));
                self.close();
            },
            _ => {},
        }
        if let Some(s) = self.state.as_ref() {
            s.window.request_redraw();
        }
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
            "stall-trace begin problem={problem:?} redraws={} capacity={} overwritten={} stall_ms={} upstream_log_records={} clock=monotonic-wall-us-winit-thread",
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
            t.report(phase, self.options.compose.name(), |line| {
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
        let netrender = matches!(s.composer, Composer::Netrender(_));
        let panel: [u8; 4] = if netrender {
            [255, 255, 0, 255]
        } else {
            [255, 0, 0, 255]
        };
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
        if let Some(s) = self.state.as_mut() {
            self.log.line(format_args!(
                "closing imported={} presented={} elapsed_ms={}",
                s.imported,
                s.presented,
                self.started.elapsed().as_millis()
            ));
            if let Some(input) = s.input.as_ref() {
                self.log.line(format_args!(
                    "game input forwarded={} rejected={} console_messages={}",
                    input.forwarded, input.rejected, self.console.seen
                ));
            }
            if let Err(error) = s.producer.close() {
                self.log.line(format_args!("FAIL producer close: {error}"));
                self.failures += 1;
            }
        }
    }

    fn finish(&mut self) -> i32 {
        if let Some(s) = self.state.take() {
            // Drain the shared queue before dropping the device's last owners.
            let _ = s.host.device.poll(wgpu::PollType::wait_indefinitely());
            let State {
                producer,
                frame,
                composer,
                runtime,
                ..
            } = s;
            drop(frame);
            drop(producer);
            drop(composer);
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

    /// Exits the event loop once the browser reports closed (or the close timed out).
    fn exit_if_closed(&mut self, event_loop: &ActiveEventLoop) {
        if self.phase != Phase::Closing {
            return;
        }
        if self.state.as_ref().is_none_or(|s| s.producer.is_closed()) {
            self.log.line(format_args!("browser closed"));
            event_loop.exit();
        } else if self
            .closing_since
            .is_some_and(|t| t.elapsed() > CLOSE_TIMEOUT)
        {
            self.fail(format_args!(
                "browser did not close within {CLOSE_TIMEOUT:?}"
            ));
            self.exit_code.get_or_insert(2);
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() || self.exit_code.is_some() {
            return;
        }
        match self.start(event_loop) {
            Ok(state) => self.state = Some(state),
            Err(error) => {
                self.log.line(format_args!("FAIL startup: {error}"));
                self.exit_code = Some(5);
                event_loop.exit();
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.phase == Phase::Game
            && let Some(s) = self.state.as_mut()
            && let Some(input) = s.input.as_mut()
            && let Some(line) = input.window_event(&mut s.producer, &event)
        {
            self.log.line(format_args!("{line}"));
        }
        match event {
            WindowEvent::CloseRequested => {
                self.log.line(format_args!("close requested by the OS"));
                self.close();
            },
            WindowEvent::Resized(size) => self.resized(size),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {},
        }
        self.exit_if_closed(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.exit_if_closed(event_loop);
        if let Some(s) = self.state.as_ref() {
            s.window.request_redraw();
        }
    }
}

/// Composes the current frame into a texture of the surface size. Returns `None` while there
/// is nothing to present yet.
fn compose(s: &mut State, mut timings: Option<&mut Timings>) -> Option<wgpu::Texture> {
    let start = now_if(&timings);
    let (w, h) = (s.surface_config.width, s.surface_config.height);
    match &mut s.composer {
        Composer::Netrender(renderer) => {
            let mut scene = Scene::new(w, h);
            scene.push_rect(0.0, 0.0, w as f32, h as f32, BACKGROUND);
            scene.push_rect(
                w as f32 * 0.5,
                h as f32 * 0.5,
                w as f32,
                h as f32,
                NETRENDER_PANEL,
            );
            let view = s.frame.as_ref().map(|f| (&f.view, f.size));
            // Placed 1:1 at the frame's own size (never stretched while a resize is pending)
            // after every scene op, so Netrender's tail re-render (the d2f13ca5 ordering
            // issue in the shared-device proof) is never involved.
            let composites: Vec<ExternalTextureComposite<'_>> = view
                .map(|(view, size)| {
                    ExternalTextureComposite::new(
                        view,
                        ExternalTexturePlacement::new([
                            0.0,
                            0.0,
                            size.width as f32,
                            size.height as f32,
                        ])
                        .with_alpha(SourceAlpha::Premultiplied),
                    )
                })
                .into_iter()
                .collect();
            let mut capture = CaptureMaster(None);
            renderer.render_with_compositor_and_external_textures(
                &scene,
                COMPOSED_FORMAT,
                &mut capture,
                netrender::peniko::Color::new([0.0, 0.0, 0.0, 0.0]),
                &composites,
            );
            // Netrender submits internally: its submit cannot be timed separately here.
            lap(&mut timings, Stage::Compose, start);
            capture.0
        },
        Composer::Wgpu(target) => {
            let target = match target {
                Some(t) if t.width() == w && t.height() == h => t.clone(),
                _ => {
                    let texture = s.host.device.create_texture(&wgpu::TextureDescriptor {
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
                    *target = Some(texture.clone());
                    texture
                },
            };
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let [r, g, b, a] = BACKGROUND.map(f64::from);
            let source = s.frame.as_ref().map(|f| (&f.view, f.size));
            let commands = s.blit.encode(
                &s.host,
                &view,
                wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                source.map(|(view, size)| (view, size.width, size.height)),
                true,
            );
            let submit = lap(&mut timings, Stage::Compose, start);
            s.host.queue.submit([commands]);
            lap(&mut timings, Stage::ComposeSubmit, submit);
            Some(target)
        },
    }
}

/// GPU blit of the composed frame onto the window surface.
fn present(
    s: &mut State,
    composed: &wgpu::Texture,
    mut timings: Option<&mut Timings>,
    mut marks: Option<&mut Marks>,
) -> bool {
    let start = now_if(&timings);
    let current = s.surface.get_current_texture();
    // Failed or outdated acquisitions are sampled too: they cost the frame.
    let since = lap(&mut timings, Stage::Acquire, start);
    mark_ref(&mut marks, Step::SurfaceAcquire);
    let output = match current {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            s.surface.configure(&s.host.device, &s.surface_config);
            return false;
        },
        _ => return false,
    };
    let target = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let source = composed.create_view(&wgpu::TextureViewDescriptor::default());
    let commands = s.blit.encode(
        &s.host,
        &target,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        Some((&source, composed.width(), composed.height())),
        false,
    );
    let since = lap(&mut timings, Stage::PresentEncode, since);
    s.host.queue.submit([commands]);
    let since = lap(&mut timings, Stage::Submit, since);
    mark_ref(&mut marks, Step::Submit);
    s.window.pre_present_notify();
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

struct CaptureMaster(Option<wgpu::Texture>);

impl Compositor for CaptureMaster {
    fn declare_surface(&mut self, _key: netrender::SurfaceKey, _world_bounds: [f32; 4]) {}
    fn destroy_surface(&mut self, _key: netrender::SurfaceKey) {}
    fn present_frame(&mut self, frame: PresentedFrame<'_>) {
        self.0 = Some(frame.master.clone());
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

    /// Encodes one render pass on `target`: `load`, then `source` drawn 1:1 from the top-left
    /// corner (with premultiplied blending when `over`). The caller submits it.
    fn encode(
        &self,
        host: &HostWgpuContext,
        target: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
        source: Option<(&wgpu::TextureView, u32, u32)>,
        over: bool,
    ) -> wgpu::CommandBuffer {
        let pipeline = if over { &self.over } else { &self.present };
        let bind_group = source.map(|(view, _, _)| {
            host.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("roves blit"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
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
            if let (Some((_, w, h)), Some(bind_group)) = (source, bind_group.as_ref()) {
                let tw = target.texture().width();
                let th = target.texture().height();
                // Viewport = source size, clipped to the target: texel-exact, never scaled.
                pass.set_viewport(0.0, 0.0, w as f32, h as f32, 0.0, 1.0);
                pass.set_scissor_rect(0, 0, w.min(tw), h.min(th));
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        encoder.finish()
    }
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
