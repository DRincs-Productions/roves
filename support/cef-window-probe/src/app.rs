//! Browser-process glue, reduced from cef-rs' `cefsimple` example (Views window, Chrome
//! runtime style, close sequence `can_close` -> `try_close_browser` -> `do_close` ->
//! `on_before_close` -> `quit_message_loop`). Roves additions: the loopback origin, the
//! per-run profile directory, popup refusal (one game document per window), the
//! sandbox/close evidence log and an optional automatic window close for CI.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use cef::*;

use crate::server;

// Exit codes of the browser process (subprocesses return CEF's own code).
const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 1;
const EXIT_NOT_LOADED: i32 = 2;
const EXIT_NO_SANDBOX: i32 = 3;
const EXIT_CEF_INIT: i32 = 4;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
/// Set when the main frame finished loading the game's index from the loopback origin.
static LOADED: AtomicBool = AtomicBool::new(false);
/// Set when any load error, refused popup or crash makes the run unacceptable.
static FAILED: AtomicBool = AtomicBool::new(false);

/// One line per event, `roves-cef <event> key=value...`, to the `--roves-log` file and stderr.
fn log(line: impl AsRef<str>) {
    let line = format!("roves-cef {}", line.as_ref());
    eprintln!("{line}");
    if let Some(file) = LOG.get()
        && let Ok(mut file) = file.lock()
    {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

fn switch(cmd: &CommandLine, name: &str) -> String {
    CefString::from(&cmd.switch_value(Some(&CefString::from(name)))).to_string()
}

fn cef_path(path: &Path) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

pub fn run(main_args: MainArgs, sandbox_info: *mut u8) -> i32 {
    // Initialize the CEF API version (as cefsimple does) before any other CEF call.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::from(main_args);
    let Some(cmd) = args.as_cmd_line() else {
        return EXIT_USAGE;
    };
    let code = execute_process(Some(args.as_main_args()), None::<&mut App>, sandbox_info);
    if code >= 0 {
        // Renderer, GPU, network, utility... subprocess: it has already run to completion.
        return code;
    }
    browser_main(args.as_main_args(), &cmd, sandbox_info)
}

struct Options {
    game_dir: PathBuf,
    profile_dir: PathBuf,
    port: u16,
    auto_close_ms: Option<i64>,
}

fn options(cmd: &CommandLine) -> Result<Options, String> {
    let game_dir = switch(cmd, "roves-game-dir");
    if game_dir.is_empty() {
        return Err("missing --roves-game-dir=<game build directory>".into());
    }
    // std::path::absolute, not canonicalize: no `\\?\` verbatim prefix in paths given to CEF.
    let game_dir = std::path::absolute(&game_dir).map_err(|e| format!("{game_dir}: {e}"))?;
    if !game_dir.join("index.html").is_file() {
        return Err(format!("{} has no index.html", game_dir.display()));
    }
    let profile_dir = match switch(cmd, "roves-profile-dir") {
        dir if dir.is_empty() => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            std::env::temp_dir()
                .join("roves-cef-probe")
                .join(format!("run-{stamp}-{}", std::process::id()))
        },
        dir => PathBuf::from(dir),
    };
    fs::create_dir_all(&profile_dir).map_err(|e| format!("{}: {e}", profile_dir.display()))?;
    // CEF requires an absolute root_cache_path.
    let profile_dir = std::path::absolute(&profile_dir).map_err(|e| e.to_string())?;
    let port = match switch(cmd, "roves-port") {
        port if port.is_empty() => 0,
        port => port
            .parse()
            .map_err(|_| format!("invalid --roves-port={port}"))?,
    };
    let auto_close_ms = match switch(cmd, "roves-auto-close-ms") {
        ms if ms.is_empty() => None,
        ms => Some(
            ms.parse::<i64>()
                .ok()
                .filter(|ms| *ms >= 0)
                .ok_or_else(|| format!("invalid --roves-auto-close-ms={ms}"))?,
        ),
    };
    Ok(Options {
        game_dir,
        profile_dir,
        port,
        auto_close_ms,
    })
}

fn browser_main(main_args: &MainArgs, cmd: &CommandLine, sandbox_info: *mut u8) -> i32 {
    let log_path = switch(cmd, "roves-log");
    if !log_path.is_empty()
        && let Ok(file) = File::create(&log_path)
    {
        let _ = LOG.set(Mutex::new(file));
    }
    let cef_version = String::from_utf8_lossy(sys::CEF_VERSION);
    log(format!(
        "start cef={} pid={}",
        cef_version.trim_end_matches('\0'),
        std::process::id()
    ));

    // bootstrap.exe passes the sandbox information; without it CEF would run unsandboxed.
    let sandboxed = !sandbox_info.is_null() && cfg!(feature = "sandbox");
    log(format!("sandbox-info present={sandboxed}"));
    if !sandboxed {
        log("error sandbox unavailable: start the bundled bootstrap executable");
        return EXIT_NO_SANDBOX;
    }

    let opts = match options(cmd) {
        Ok(opts) => opts,
        Err(error) => {
            log(format!("error usage: {error}"));
            return EXIT_USAGE;
        },
    };
    let server = match server::start(&opts.game_dir, opts.port) {
        Ok(server) => server,
        Err(error) => {
            log(format!("error loopback-server: {error}"));
            return EXIT_USAGE;
        },
    };
    log(format!(
        "loopback origin={} game-dir={} profile-dir={}",
        server.origin,
        opts.game_dir.display(),
        opts.profile_dir.display()
    ));

    // One private on-disk profile per run: root_cache_path == cache_path, so CEF persists the
    // "Default" profile (localStorage, IndexedDB, cookies) there and takes the process
    // singleton lock on this directory instead of the shared per-user CEF directory.
    let settings = Settings {
        no_sandbox: 0,
        root_cache_path: cef_path(&opts.profile_dir),
        cache_path: cef_path(&opts.profile_dir),
        persist_session_cookies: 1,
        ..Default::default()
    };
    let state = Arc::new(Mutex::new(State {
        origin: server.origin.clone(),
        auto_close_ms: opts.auto_close_ms,
    }));
    let mut app = ProbeApp::new(state);
    if initialize(
        Some(main_args),
        Some(&settings),
        Some(&mut app),
        sandbox_info,
    ) != 1
    {
        log("error cef-initialize failed");
        let _ = server.stop();
        return EXIT_CEF_INIT;
    }
    run_message_loop();
    log("message-loop exited");
    shutdown();
    log("cef shutdown complete");
    match server.stop() {
        Ok(()) => log("loopback stopped"),
        Err(error) => log(format!("error loopback-stop: {error}")),
    }

    let code = if FAILED.load(Ordering::SeqCst) || !LOADED.load(Ordering::SeqCst) {
        EXIT_NOT_LOADED
    } else {
        EXIT_OK
    };
    log(format!("exit code={code}"));
    code
}

struct State {
    origin: String,
    auto_close_ms: Option<i64>,
}

type Shared = Arc<Mutex<State>>;

wrap_app! {
    struct ProbeApp {
        state: Shared,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ProbeBrowserProcessHandler::new(self.state.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct ProbeBrowserProcessHandler {
        state: Shared,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let url = format!("{}/", self.state.lock().unwrap().origin);
            log(format!("create-browser url={url}"));
            let mut client = ProbeClient::new(self.state.clone());
            let mut view_delegate = ProbeBrowserViewDelegate::new();
            let browser_view = browser_view_create(
                Some(&mut client),
                Some(&CefString::from(url.as_str())),
                Some(&BrowserSettings::default()),
                None,
                None,
                Some(&mut view_delegate),
            );
            let mut window_delegate = ProbeWindowDelegate::new(Arc::new(Mutex::new(browser_view)));
            window_create_top_level(Some(&mut window_delegate));
        }

        // Evidence that no child is launched with the sandbox disabled.
        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            let Some(cmd) = command_line else {
                return;
            };
            let no_sandbox = cmd.has_switch(Some(&CefString::from("no-sandbox"))) != 0;
            log(format!("child-launch type={} no-sandbox={no_sandbox}", switch(cmd, "type")));
            if no_sandbox {
                FAILED.store(true, Ordering::SeqCst);
            }
        }
    }
}

wrap_browser_view_delegate! {
    struct ProbeBrowserViewDelegate {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {}
}

wrap_window_delegate! {
    struct ProbeWindowDelegate {
        browser_view: Arc<Mutex<Option<BrowserView>>>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 720 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let browser_view = self.browser_view.lock().unwrap();
            if let (Some(window), Some(browser_view)) = (window, browser_view.as_ref()) {
                window.add_child_view(Some(&mut View::from(browser_view)));
                window.show();
                log("window shown");
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            *self.browser_view.lock().unwrap() = None;
            log("window destroyed");
        }

        // Same path for the OS close button and the CI auto-close: let the page run its
        // unload handlers and close the browser before the window goes away.
        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            let browser_view = self.browser_view.lock().unwrap();
            match browser_view.as_ref().and_then(|view| view.browser()) {
                Some(browser) => browser.host().map_or(1, |host| host.try_close_browser()),
                None => 1,
            }
        }
    }
}

wrap_client! {
    struct ProbeClient {
        state: Shared,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(ProbeDisplayHandler::new(self.state.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ProbeLifeSpanHandler::new(self.state.clone()))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(ProbeLoadHandler::new(self.state.clone()))
        }
    }
}

wrap_display_handler! {
    struct ProbeDisplayHandler {
        state: Shared,
    }

    impl DisplayHandler {
        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            log(format!("title {title:?}"));
            let mut browser = browser.cloned();
            if let Some(window) = browser_view_get_for_browser(browser.as_mut()).and_then(|v| v.window()) {
                window.set_title(Some(&CefString::from(title.as_str())));
            }
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            log(format!(
                "console level={} source={}:{line} message={:?}",
                sys::cef_log_severity_t::from(level) as i32,
                source.map(CefString::to_string).unwrap_or_default(),
                message.map(CefString::to_string).unwrap_or_default(),
            ));
            // Keep CEF's own console logging as well.
            0
        }
    }
}

wrap_life_span_handler! {
    struct ProbeLifeSpanHandler {
        state: Shared,
    }

    impl LifeSpanHandler {
        // One game document in one window: window.open and target=_blank are refused.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            log(format!("popup refused url={url}"));
            1
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            log("browser created");
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            log("browser do-close");
            // Allow the close; for a Views browser CEF then closes the hosting window.
            0
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            log("browser before-close");
            // The only browser is gone: leave run_message_loop so shutdown() can run.
            quit_message_loop();
        }
    }
}

wrap_load_handler! {
    struct ProbeLoadHandler {
        state: Shared,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let url = CefString::from(&frame.url()).to_string();
            let state = self.state.lock().unwrap();
            let expected = format!("{}/", state.origin);
            let ok = http_status_code == 200 && url.starts_with(&expected);
            log(format!("load-end main-frame status={http_status_code} url={url} ok={ok}"));
            if !ok {
                FAILED.store(true, Ordering::SeqCst);
                return;
            }
            // Only the first successful load schedules the automatic close.
            if LOADED.swap(true, Ordering::SeqCst) {
                return;
            }
            if let (Some(delay), Some(browser)) = (state.auto_close_ms, browser) {
                log(format!("auto-close scheduled delay-ms={delay}"));
                let mut task = CloseWindow::new(browser.clone());
                post_delayed_task(ThreadId::UI, Some(&mut task), delay);
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let main = frame.is_some_and(|frame| frame.is_main() != 0);
            let code = sys::cef_errorcode_t::from(error_code) as i32;
            log(format!(
                "load-error main-frame={main} code={code} text={} url={}",
                error_text.map(CefString::to_string).unwrap_or_default(),
                failed_url.map(CefString::to_string).unwrap_or_default(),
            ));
            if main {
                FAILED.store(true, Ordering::SeqCst);
            }
        }
    }
}

wrap_task! {
    struct CloseWindow {
        browser: Browser,
    }

    impl Task {
        // Closes the top-level window exactly like the OS close button would.
        fn execute(&self) {
            log("auto-close window");
            let mut browser = self.browser.clone();
            match browser_view_get_for_browser(Some(&mut browser)).and_then(|view| view.window()) {
                Some(window) => window.close(),
                None => {
                    if let Some(host) = browser.host() {
                        host.close_browser(0);
                    }
                },
            }
        }
    }
}
