//! `--game <dist dir>`: load a real game build (e.g. visual-novel-template's Vite
//! `dist/`) instead of the fixture. Blitz parses `index.html` and loads its
//! stylesheets through a `NetProvider` serving `game://content/...` from the
//! directory; deno_core loads the page's module scripts through a matching
//! `ModuleLoader`. The run reports script errors, unhandled rejections and how
//! much of the page rendered, so missing Web APIs show up as a list.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use deno_core::{
    JsRuntime, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, RequestedModuleType,
    ResolutionKind, resolve_import, v8,
};
use deno_error::JsErrorBox;

use crate::window;

const MEDIA_MATRIX: &str = include_str!("media_matrix.js");

/// The document URL games see (matches `src/history.js`): the content root, as
/// the Servo shell boots (`ports/servoshell/protocols/game.rs`), so a client-side
/// router matches its `/` route. The document itself is still the directory's
/// `index.html` (`GameContent::document`); module scripts, stylesheets and
/// relative fetches resolve against the same directory as before.
pub const GAME_URL: &str = "game://content/";

pub struct GameContent {
    root: PathBuf,
}

impl GameContent {
    /// `--game <dir>` from the command line.
    pub fn from_args() -> Option<Arc<Self>> {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--game" {
                let root = PathBuf::from(args.next().expect("--game needs a directory"));
                return Some(Arc::new(Self { root }));
            }
        }
        None
    }

    /// Maps `game://content/<path>` to a file under the content root. Other
    /// schemes/hosts and any `..` segment are refused.
    fn path_for(&self, scheme: &str, host: Option<&str>, path: &str) -> Option<PathBuf> {
        if scheme != "game" || host != Some("content") {
            return None;
        }
        let mut full = self.root.clone();
        for part in path.split('/') {
            match part {
                "" | "." => {},
                ".." => return None,
                part => full.push(part),
            }
        }
        Some(full)
    }

    /// Bytes of a `game://content/...` file (the probe's `fetch`), if it exists.
    pub fn read(&self, url: &str) -> Option<Vec<u8>> {
        let url = deno_core::url::Url::parse(url).ok()?;
        let path = self.path_for(url.scheme(), url.host_str(), url.path())?;
        std::fs::read(path).ok()
    }

    pub fn document(self: &Arc<Self>) -> HtmlDocument {
        let html = std::fs::read_to_string(self.root.join("index.html"))
            .expect("the game directory must contain index.html");
        HtmlDocument::from_html(
            &html,
            DocumentConfig {
                base_url: Some(GAME_URL.to_string()),
                net_provider: Some(Arc::new(GameNetProvider(self.clone()))),
                html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                ..Default::default()
            },
        )
    }

    pub fn module_loader(self: &Arc<Self>) -> Rc<dyn ModuleLoader> {
        Rc::new(GameModuleLoader(self.clone()))
    }
}

/// Serves Blitz's resource requests (stylesheets, fonts, images) from the game
/// directory, plus inline `data:` URLs (e.g. a save thumbnail's
/// `data:image/png;base64,...`) decoded with the `data-url` crate. Nothing else
/// is fetched: other schemes, hosts, missing files and malformed `data:` URLs
/// are reported and left unanswered.
struct GameNetProvider(Arc<GameContent>);

impl GameNetProvider {
    fn resource(&self, url: &blitz_traits::net::Url) -> Option<Vec<u8>> {
        if url.scheme() == "data" {
            let data = data_url::DataUrl::process(url.as_str()).ok()?;
            return data.decode_to_vec().ok().map(|(bytes, _fragment)| bytes);
        }
        let path = self.0.path_for(url.scheme(), url.host_str(), url.path())?;
        std::fs::read(path).ok()
    }
}

impl NetProvider for GameNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url;
        match self.resource(&url) {
            Some(bytes) => handler.bytes(url.to_string(), Bytes::from(bytes)),
            None => {
                // A data: URL can be megabytes long; keep the log line short.
                let shown: String = url.as_str().chars().take(120).collect();
                eprintln!("ROVES_PROBE_GAME_MISSING_RESOURCE {shown}");
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records what the provider hands to Blitz's resource handler.
    struct Capture(Arc<Mutex<Option<(String, Vec<u8>)>>>);

    impl NetHandler for Capture {
        fn bytes(self: Box<Self>, resolved_url: String, bytes: Bytes) {
            *self.0.lock().unwrap() = Some((resolved_url, bytes.to_vec()));
        }
    }

    fn fetch(provider: &GameNetProvider, url: &str) -> Option<(String, Vec<u8>)> {
        let seen = Arc::new(Mutex::new(None));
        let url = blitz_traits::net::Url::parse(url).unwrap();
        provider.fetch(0, Request::get(url), Box::new(Capture(seen.clone())));
        seen.lock().unwrap().take()
    }

    fn provider(root: &std::path::Path) -> GameNetProvider {
        GameNetProvider(Arc::new(GameContent {
            root: root.to_path_buf(),
        }))
    }

    // Regression (artifact 43288469871, template bb9ea779): a save thumbnail is
    // an inline PNG and was reported as ROVES_PROBE_GAME_MISSING_RESOURCE.
    #[test]
    fn inline_png_reaches_the_resource_handler() {
        // 8-byte PNG signature, base64.
        let url = "data:image/png;base64,iVBORw0KGgo=";
        let (resolved, bytes) = fetch(&provider(&std::env::temp_dir()), url).expect("served");
        assert_eq!(resolved, url);
        assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn game_files_are_served_and_other_origins_are_not() {
        let root = std::env::temp_dir().join(format!("roves-net-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("style.css"), "a{}").unwrap();
        let provider = provider(&root);
        let served = fetch(&provider, "game://content/style.css");
        let other_host = fetch(&provider, "game://other/style.css");
        let remote = fetch(&provider, "https://example.com/style.css");
        let bad_data = fetch(&provider, "data:image/png;base64,!!!");
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(served.map(|(_, bytes)| bytes), Some(b"a{}".to_vec()));
        assert!(other_host.is_none() && remote.is_none() && bad_data.is_none());
    }
}

/// Loads `game://content/...` ES modules (JavaScript, or JSON with
/// `with { type: "json" }`) from the game directory.
struct GameModuleLoader(Arc<GameContent>);

impl ModuleLoader for GameModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> deno_core::ModuleResolveResponse {
        resolve_import(specifier, referrer).map_err(JsErrorBox::from_err)
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        _referrer: Option<&ModuleLoadReferrer>,
        options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        let load = || {
            let path = self
                .0
                .path_for(specifier.scheme(), specifier.host_str(), specifier.path())
                .ok_or_else(|| {
                    JsErrorBox::type_error(format!("not a game:// module: {specifier}"))
                })?;
            let code = std::fs::read(&path)
                .map_err(|error| JsErrorBox::generic(format!("{specifier}: {error}")))?;
            let json = path
                .extension()
                .is_some_and(|extension| extension == "json");
            let json_requested = options.requested_module_type == RequestedModuleType::Json;
            if json != json_requested {
                return Err(JsErrorBox::type_error(format!(
                    "{specifier}: JSON modules need `with {{ type: \"json\" }}` (and only they may use it)"
                )));
            }
            let module_type = if json {
                ModuleType::Json
            } else {
                ModuleType::JavaScript
            };
            Ok(ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(code.into_boxed_slice().into()),
                specifier,
                None,
            ))
        };
        ModuleLoadResponse::Sync(load())
    }
}

/// Script elements of the parsed page: (`type`, `src`, inline text).
fn page_scripts(document: &Rc<RefCell<HtmlDocument>>) -> Vec<(String, Option<String>, String)> {
    let document = document.borrow();
    let root = document.root_node().id;
    let scripts = document
        .query_selector_all_in(root, "script")
        .unwrap_or_default();
    scripts
        .into_iter()
        .filter_map(|id| document.get_node(id))
        .map(|node| {
            let attr = |name: &str| {
                node.attrs().and_then(|attrs| {
                    attrs
                        .iter()
                        .find(|attr| &*attr.name.local == name)
                        .map(|attr| attr.value.clone())
                })
            };
            (
                attr("type").unwrap_or_default(),
                attr("src"),
                node.text_content(),
            )
        })
        .collect()
}

/// Runs the game page: installs error collection, evaluates its scripts in
/// document order, drives `frames` frames and prints a report.
pub async fn run(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    document_loaded: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    gpu_pacing_stats: &v8::Global<v8::Function>,
    web_audio_trace: &v8::Global<v8::Function>,
    frames: u32,
) {
    runtime
        .execute_script(
            "roves-blitz-game-errors.js",
            r#"globalThis.__rovesGameErrors = [];
addEventListener("error", (event) => {
  __rovesGameErrors.push("error: " + String(event.error?.stack ?? event.message));
  event.preventDefault();
});
addEventListener("unhandledrejection", (event) => {
  __rovesGameErrors.push("unhandledrejection: " + String(event.reason?.stack ?? event.reason));
  event.preventDefault();
});"#,
        )
        .expect("game error collection should install");

    let mut load_errors = Vec::new();
    let mut loaded_main = false;
    for (kind, src, text) in page_scripts(document) {
        let result: Result<(), String> = async {
            if kind == "module" {
                let id = match &src {
                    Some(src) => {
                        let url = resolve_import(src, GAME_URL).map_err(|e| e.to_string())?;
                        if loaded_main {
                            runtime.load_side_es_module(&url).await
                        } else {
                            loaded_main = true;
                            runtime.load_main_es_module(&url).await
                        }
                    },
                    None => {
                        let url = resolve_import("./roves-inline-module.js", GAME_URL)
                            .map_err(|e| e.to_string())?;
                        runtime.load_side_es_module_from_code(&url, text).await
                    },
                }
                .map_err(|e| e.to_string())?;
                let evaluation = runtime.mod_evaluate(id);
                tokio::time::timeout(
                    Duration::from_secs(20),
                    runtime.with_event_loop_future(Box::pin(evaluation), Default::default()),
                )
                .await
                .map_err(|_| "module evaluation timed out".to_string())?
                .map_err(|e| e.to_string())
            } else if src.is_none() && (kind.is_empty() || kind == "text/javascript") {
                runtime
                    .execute_script("roves-blitz-inline-script.js", text)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        .await;
        if let Err(error) = result {
            load_errors.push(format!("{kind} {}: {error}", src.unwrap_or_default()));
        }
    }

    // Scripts ran: readyState interactive/complete, DOMContentLoaded and load.
    {
        deno_core::scope!(scope, runtime);
        let function = v8::Local::new(scope, document_loaded);
        let receiver = v8::undefined(scope).into();
        function.call(scope, receiver, &[]);
    }

    crate::drive_frames(runtime, document, run_animation_frame, frames).await;

    let report = crate::evaluate_string(
        runtime,
        // Route/connectivity diagnostics (also extracted into a separate CI
        // annotation). `start_menu_like` is a text heuristic over the rendered
        // buttons, not a guarantee the menu works.
        r#"(() => {
  const text = (document.body?.textContent ?? "").replace(/\s+/g, " ").trim();
  let buttons = [];
  try {
    buttons = Array.from(document.body?.querySelectorAll("button, [role=button]") ?? [])
      .map((button) => (button.textContent ?? "").replace(/\s+/g, " ").trim())
      .filter((label) => label !== "");
  } catch (error) {
    buttons = ["<query failed: " + error + ">"];
  }
  return JSON.stringify({
    location: location.href,
    pathname: location.pathname,
    navigator_online: navigator.onLine,
    not_found_text: /not found/i.test(text),
    offline_text: /no internet connection/i.test(text),
    start_menu_like: buttons.some((label) => /\b(start|new game|continue|load)\b/i.test(label)),
    buttons: buttons.slice(0, 8).map((label) => label.slice(0, 40)),
    errors: __rovesGameErrors.slice(0, 30),
    error_count: __rovesGameErrors.length,
    root_children: document.getElementById("root")?.childNodes.length ?? -1,
    body_elements: document.body?.querySelectorAll("*").length ?? -1,
    text: text.slice(0, 300),
  });
})()"#,
    );
    let load_errors = format!(
        "[{}]",
        load_errors
            .iter()
            .map(|error| format!("{:?}", error))
            .collect::<Vec<_>>()
            .join(",")
    );
    if let Ok(path) = std::env::var("ROVES_PROBE_SCREENSHOT") {
        let pixels = crate::render_rgba(document);
        crate::write_png(&path, &pixels);
    }
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_page\",\"status\":\"ran\",\"frames\":{frames},\"load_errors\":{load_errors},\"page\":{report}}}"
    );
    // Frame pacing over the page run (ticks, frames, deferred rAF ticks, max
    // tick gap, saturation, poll errors); all zero-deferral on hardware.
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_frame_pacing\",\"status\":\"ran\",\"pacing\":{}}}",
        crate::call_hook(runtime, gpu_pacing_stats, &[])
    );

    if std::env::args().any(|arg| arg == "--media-matrix") {
        media_matrix(runtime, document, run_animation_frame).await;
    }

    if std::env::args().any(|arg| arg == "--story") {
        story(
            runtime,
            document,
            run_animation_frame,
            dispatcher,
            gpu_pacing_stats,
        )
        .await;
    } else if std::env::args().any(|arg| arg == "--story-restore") {
        restore_story(runtime, document, run_animation_frame, dispatcher).await;
    }
    // Sound files over the whole run (page and story): decode requests, buffers
    // ready or decode errors, and starts of sources playing a decoded buffer, in
    // order. Informational, printed once. `output` is the configured sink path
    // (device/none), never a device name.
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_audio\",\"status\":\"ran\",\"output\":\"{}\",\"audio\":{}}}",
        crate::audio::AudioOutput::configured().label(),
        crate::call_hook(runtime, web_audio_trace, &[])
    );
}

/// Run the deterministic media fixture set in the same game document/runtime
/// used by Chrome's DevTools comparison. Non-baseline format gaps are reported
/// but do not fail this harness; image decoding/error behaviour, SVG image
/// loading, and decoded WAV sample equality are the deterministic baseline.
async fn media_matrix(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
) {
    use deno_core::serde_json::{Value, json};

    crate::stage("media_matrix");
    runtime
        .execute_script("roves-blitz-media-matrix.js", MEDIA_MATRIX)
        .expect("media matrix should start");

    let mut completed = false;
    for _ in 0..3600 {
        crate::drive_frames(runtime, document, run_animation_frame, 1).await;
        if crate::evaluate_string(runtime, "String(__rovesMediaMatrix?.done === true)") == "true" {
            completed = true;
            break;
        }
    }
    let report_text = crate::evaluate_string(
        runtime,
        "JSON.stringify(__rovesMediaMatrix?.report ?? null)",
    );
    let report: Value = deno_core::serde_json::from_str(&report_text).unwrap_or(Value::Null);
    let error = crate::evaluate_string(runtime, "String(__rovesMediaMatrix?.error ?? '')");
    let baseline_failures = report
        .get("baselineFailures")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let status = if completed && error.is_empty() && baseline_failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"media_matrix_baseline\",\"status\":\"{status}\",\"completed\":{completed},\"error\":{},\"baseline_failures\":{},\"report\":{report}}}",
        json!(error),
        json!(baseline_failures),
    );
}

/// Upper bound, in frames, for each story state to appear and settle.
const STORY_STATE_FRAMES: u32 = 1800;
/// A state is settled when the page text is unchanged for this many checks
/// (one check every 10 frames), so a typewriter effect finishes first.
const STORY_STABLE_CHECKS: u32 = 3;

/// Page-side helpers of the `--story` harness, evaluated in the page's global.
/// They inspect the DOM, events and save records. Input goes through the
/// embedder's hit-test and trusted-event bridge. Toasts/alerts (role alert,
/// aria-live assertive, sonner/notistack/MUI/"toast"/"snackbar" classes) are
/// kept out of the page text: accumulating notifications must never count as
/// story progress (CI 42898669401 passed on two identical error toasts).
const STORY_HELPERS: &str = r#"globalThis.__rovesStory = (() => {
  const norm = (text) => (text ?? "").replace(/\s+/g, " ").trim();
  const attr = (element, name) => element.getAttribute(name) ?? "";
  const TOAST_CLASS = /toast|snackbar|notistack|MuiAlert/i;
  const isToast = (element) =>
    ["alert", "alertdialog"].includes(attr(element, "role")) ||
    attr(element, "aria-live") === "assertive" ||
    element.getAttribute("data-sonner-toast") !== null ||
    TOAST_CLASS.test(attr(element, "class"));
  const ERROR_MARK = /error|danger|fail/i;
  const ERROR_TEXT = /\b(error|errore|errors|failed|failure|exception|went wrong|TypeError|ReferenceError|RangeError)\b/i;
  const isErrorToast = (element, text) =>
    ERROR_MARK.test(`${attr(element, "class")} ${attr(element, "data-type")} ${attr(element, "data-variant")}`) || ERROR_TEXT.test(text);
  const isButton = (element) => element.localName === "button" || attr(element, "role") === "button";
  // Text nodes outside toasts, scripts and styles; `inButton` marks control labels.
  const walk = (node, onText, onToast, inButton = false) => {
    for (const child of Array.from(node.childNodes)) {
      if (child.nodeType === 3) {
        const text = norm(child.textContent);
        if (text) onText(text, child.parentElement, inButton);
      } else if (child.nodeType === 1) {
        if (isToast(child)) {
          onToast(child);
        } else if (child.localName !== "script" && child.localName !== "style") {
          walk(child, onText, onToast, inButton || isButton(child));
        }
      }
    }
  };
  const visible = (element) => {
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  };
  const snapshot = () => {
    const parts = [];
    if (document.body) walk(document.body, (text) => parts.push(text), () => {});
    return parts.join(" ");
  };
  // Dialogue-like text not on the menu: not a control label, at least two
  // words and eight characters.
  const dialogueLines = (menu) => {
    const lines = [];
    if (document.body) {
      walk(document.body, (text, _, inButton) => {
        if (!inButton && text.length >= 8 && text.split(" ").length >= 2 && !menu.includes(text) && !lines.includes(text)) lines.push(text);
      }, () => {});
    }
    return lines;
  };
  const center = (element, extra) => {
    const rect = element.getBoundingClientRect();
    if (!(rect.width > 0 && rect.height > 0)) return null;
    return JSON.stringify({ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2, w: rect.width, h: rect.height, ...extra });
  };
  const buttons = () => Array.from(document.body?.querySelectorAll("button, [role=button]") ?? []);
  const label = (element) => norm(element.textContent) || norm(element.getAttribute("aria-label")) || norm(element.getAttribute("title"));
  // Mounted dialogs (Base UI Dialog/AlertDialog popups carry the role).
  const dialogElements = () => Array.from(document.body?.querySelectorAll("[role=dialog], [role=alertdialog]") ?? []);
  const dialogTitle = (element) => {
    const id = element.getAttribute("aria-labelledby");
    const title = (id && document.getElementById(id)) || element.querySelector("[data-slot=dialog-title]");
    return title ? norm(title.textContent) : "";
  };
  // Running finite animations in the subtree (an infinite spinner never ends).
  const running = (element) => {
    if (typeof element.getAnimations !== "function") return -1;
    return element.getAnimations({ subtree: true })
      .filter((animation) => animation.playState === "running" && animation.effect?.getTiming?.().iterations !== Infinity)
      .length;
  };
  const describe = (element) => ({
    role: attr(element, "role"),
    title: dialogTitle(element).slice(0, 60),
    open: element.hasAttribute("data-open"),
    closed: element.hasAttribute("data-closed"),
    visible: visible(element),
    running: running(element),
    text: norm(element.textContent).slice(0, 120),
  });
  const describeFocus = () => {
    const element = document.activeElement;
    if (!element) return null;
    return {
      tag: element.localName ?? null,
      role: element.getAttribute?.("role") ?? null,
      text: norm(element.textContent).slice(0, 40),
      in_dialog: dialogElements().some((dialog) => dialog === element || dialog.contains(element)),
    };
  };
  return {
    snapshot,
    // The menu's Settings button (exact label).
    settings() {
      const button = buttons().find((element) => /^settings$/i.test(label(element)));
      return button ? (center(button, { label: label(button) }) ?? "null") : "null";
    },
    continueButton() {
      const button = buttons().find((element) => /^continue$/i.test(label(element)) &&
        !element.hasAttribute("disabled") && element.getAttribute("aria-disabled") !== "true");
      return button ? center(button, {}) ?? "" : "";
    },
    restoredLine(expected) {
      return dialogueLines([]).find((line) => line.includes(expected)) ?? "";
    },
    dialogs() {
      return JSON.stringify(dialogElements().map(describe));
    },
    // A mounted dialog (title matching `title`, any when null) that is visible,
    // marked open and has finished its open animation; "" until there is one.
    dialogOpen(title) {
      const dialog = dialogElements().find((element) =>
        (title === null || title.test(dialogTitle(element))) && visible(element) &&
        element.hasAttribute("data-open") && !element.hasAttribute("data-closed") && running(element) <= 0);
      return dialog ? JSON.stringify(describe(dialog)) : "";
    },
    focused() {
      return JSON.stringify(describeFocus());
    },
    watchNameInput() {
      const input = dialogElements().map((dialog) => dialog.querySelector("input")).find(Boolean);
      const events = [];
      if (!input) return;
      for (const type of ["keydown", "input"]) {
        input.addEventListener(type, (event) => {
          if (events.length < 48) events.push([type, event.key ?? null, input.value]);
        }, { capture: true });
      }
      this.nameInputEvents = events;
    },
    nameConfirm() {
      const button = dialogElements()
        .flatMap((dialog) => Array.from(dialog.querySelectorAll("button")))
        .find((button) => /^confirm$/i.test(label(button)) && !button.hasAttribute("disabled"));
      return button ? center(button, {}) ?? "" : "";
    },
    quickSaveEvidence(since) {
      if (this.quickSaveResult) return this.quickSaveResult;
      if (this.quickSaveReading) return "";
      this.quickSaveReading = true;
      const open = indexedDB.open("game_db");
      open.onerror = () => {
        this.quickSaveReading = false;
        this.quickSaveResult = JSON.stringify({ error: open.error?.name ?? "database open failed" });
      };
      open.onsuccess = () => {
        const db = open.result;
        if (!db.objectStoreNames.contains("saves")) {
          db.close();
          this.quickSaveReading = false;
          this.quickSaveResult = JSON.stringify({ error: "save store missing" });
          return;
        }
        const read = db.transaction("saves", "readonly").objectStore("saves").openCursor();
        read.onerror = () => {
          db.close();
          this.quickSaveReading = false;
          this.quickSaveResult = JSON.stringify({ error: read.error?.name ?? "save read failed" });
        };
        read.onsuccess = () => {
          const cursor = read.result;
          const save = cursor?.value;
          if (cursor && !(save.id >= -7 && save.id <= -2 && Number(new Date(save.date)) >= since)) {
            cursor.continue();
            return;
          }
          db.close();
          this.quickSaveReading = false;
          if (save) this.quickSaveResult = JSON.stringify({
            id: save.id,
            has_state: !!save.saveData && typeof save.saveData === "object",
            thumbnail_png: typeof save.image === "string" && save.image.startsWith("data:image/png;base64,"),
          });
        };
      };
      return "";
    },
    // The first visible text input inside a mounted dialog: its center, value,
    // type attribute, focus, and whether the dialog's Confirm button is disabled.
    dialogInput() {
      for (const dialog of dialogElements()) {
        const input = dialog.querySelector("input");
        if (!input) continue;
        const confirm = Array.from(dialog.querySelectorAll("button")).find((button) => /^confirm$/i.test(label(button)));
        return center(input, {
          value: input.value,
          type: input.getAttribute("type"),
          focused: document.activeElement === input,
          confirm_disabled: confirm ? confirm.hasAttribute("disabled") : null,
        }) ?? "null";
      }
      return "null";
    },
    // Non-control text containing `name`: speaker labels and dialogue blocks.
    // Markdown/typewriter renderers may split one visible line into multiple
    // text nodes, so inspect each text node's ancestors and use their combined
    // textContent instead of requiring the name to occur in one node.
    nameLines(name) {
      const speaker = [];
      const dialogue = [];
      const seenSpeaker = new Set();
      const seenDialogue = new Set();
      if (document.body) {
        walk(document.body, (text, element, inButton) => {
          if (inButton || !element) return;
          if (text === name) seenSpeaker.add(text);
          for (let block = element; block && block !== document.body; block = block.parentElement) {
            const combined = norm(block.textContent);
            if (!combined.includes(name)) continue;
            if (combined === name) {
              seenSpeaker.add(combined);
            } else {
              seenDialogue.add(combined.slice(0, 120));
            }
            break;
          }
        }, () => {});
      }
      speaker.push(...seenSpeaker);
      dialogue.push(...seenDialogue);
      return JSON.stringify({ speaker, dialogue });
    },
    // The HUD's "Save/Load" button (outside dialogs), marked with
    // data-roves-probe-target so the embedder can tell whether its hit test
    // at the button's center resolves inside it.
    hudSaveLoad() {
      const dialogs = dialogElements();
      const button = buttons().find((element) => /^save\s*\/\s*load$/i.test(label(element)) &&
        !dialogs.some((dialog) => dialog.contains(element)));
      if (!button) return "null";
      const rect = button.getBoundingClientRect();
      const found = center(button, {
        label: label(button),
        rect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
        disabled: button.hasAttribute("disabled"),
      });
      if (found) button.setAttribute("data-roves-probe-target", "");
      return found ?? "null";
    },
    clearProbeTarget() {
      for (const element of Array.from(document.querySelectorAll("[data-roves-probe-target]"))) {
        element.removeAttribute("data-roves-probe-target");
      }
      return "";
    },
    // Non-control text outside mounted dialogs and tooltips. Unlike
    // dialogueLines it keeps short single-word lines (the PixiVN template's
    // "...Zorvaq." after the name input, plus its speaker label), requiring
    // only two letters so one-letter typewriter fragments are skipped; text
    // equal to a button label (a hover tooltip) is ignored too.
    narration(menu) {
      const dialogs = dialogElements();
      const labels = buttons().map(label);
      const lines = [];
      if (document.body) {
        walk(document.body, (text, element, inButton) => {
          if (inButton || (text.match(/\p{L}/gu) ?? []).length < 2 || menu.includes(text) || lines.includes(text) ||
            labels.includes(text)) return;
          if (element && (element.closest("[role=tooltip]") || dialogs.some((dialog) => dialog.contains(element)))) return;
          lines.push(text);
        }, () => {});
      }
      return JSON.stringify(lines);
    },
    // An open, settled dialog showing the save/load page (its Quick Saves /
    // Save to File / Load from File controls); "" until there is one.
    saveLoadPanel() {
      const dialog = dialogElements().find((element) => visible(element) &&
        element.hasAttribute("data-open") && !element.hasAttribute("data-closed") && running(element) <= 0 &&
        Array.from(element.querySelectorAll("[aria-label]")).some((control) =>
          /^(quick saves|save to file|load from file)$/i.test(norm(control.getAttribute("aria-label")))));
      if (!dialog) return "";
      return JSON.stringify({ ...describe(dialog), settings_tab_save_load: /save-load/.test(decodeURIComponent(location.search)) });
    },
    // The menu's Start button: exact "Start"/"New game" first, then a label containing it.
    start() {
      const all = buttons();
      const exact = all.find((button) => /^(start|new game)$/i.test(label(button)));
      const loose = all.find((button) => /\b(start|new game)\b/i.test(label(button)));
      const button = exact ?? loose;
      return button ? (center(button, { label: label(button) }) ?? "null") : "null";
    },
    // A "next" control of the dialogue UI, by label/aria-label/title.
    next() {
      const button = buttons().find((element) => /^(next|›|»|>|→)$/i.test(label(element)) || /\bnext\b/i.test(norm(element.getAttribute("aria-label"))));
      return button ? (center(button, { label: label(button) }) ?? "null") : "null";
    },
    // The element holding the longest non-control text that was not on the menu.
    dialogue(menu) {
      let best = null;
      if (document.body) {
        walk(document.body, (text, element, inButton) => {
          if (!inButton && text.length >= 3 && !menu.includes(text) && (best === null || text.length > best.text.length)) {
            best = { text, element };
          }
        }, () => {});
      }
      if (best === null || best.element === null) return "null";
      return center(best.element, { label: best.text.slice(0, 60) }) ?? "null";
    },
    lines(menu) {
      return JSON.stringify(dialogueLines(menu));
    },
    // A React error-boundary fallback ("Something went wrong" text or a "Show
    // Error" control, as in the PixiVN template) replacing the game UI: page
    // text including alerts, or "" when absent. It is never a story state.
    errorBoundary() {
      const parts = [];
      if (document.body) walk(document.body, (text) => parts.push(text), (element) => parts.push(norm(element.textContent)));
      const text = parts.join(" ");
      const control = buttons().some((element) => /^show (error|details)$/i.test(label(element)));
      return control || /something went wrong/i.test(text) ? (text.slice(0, 300) || "(error boundary)") : "";
    },
    // Visible toasts/alerts, each flagged when it reports an error.
    toasts() {
      const found = [];
      if (document.body) {
        walk(document.body, () => {}, (element) => {
          const text = norm(element.textContent);
          if (text && visible(element)) found.push({ text: text.slice(0, 200), error: isErrorToast(element, text) });
        });
      }
      return JSON.stringify(found);
    },
  };
})();"#;

/// Outcome of waiting for a new settled page state.
enum StoryState {
    Settled(String, u32),
    Timeout(String),
    /// The page shows an error-boundary fallback (text, frames): fails at once.
    ErrorBoundary(String, u32),
}

/// Error-boundary fallback text currently on the page, if any.
fn error_boundary(runtime: &mut JsRuntime) -> Option<String> {
    let text = crate::evaluate_string(runtime, "__rovesStory.errorBoundary()");
    (!text.is_empty()).then_some(text)
}

/// Records every distinct visible toast (`{text, error}`) the page shows; toasts
/// can auto-dismiss, so this runs at every check, not only on settled states.
fn collect_toasts(runtime: &mut JsRuntime, seen: &mut Vec<deno_core::serde_json::Value>) {
    let found = crate::evaluate_string(runtime, "__rovesStory.toasts()");
    let found: Vec<deno_core::serde_json::Value> =
        deno_core::serde_json::from_str(&found).unwrap_or_default();
    for toast in found {
        // Bounded: a changing toast text (e.g. a countdown) adds one per check.
        if seen.len() < 20 && !seen.contains(&toast) {
            seen.push(toast);
        }
    }
}

/// Drives frames until the toast-free page text differs from every `previous`
/// state and stays unchanged for `STORY_STABLE_CHECKS` checks, within
/// `STORY_STATE_FRAMES`.
async fn wait_for_state(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    previous: &[String],
    toasts: &mut Vec<deno_core::serde_json::Value>,
) -> StoryState {
    let mut candidate = String::new();
    let mut stable = 0;
    let mut frames = 0;
    while frames < STORY_STATE_FRAMES {
        crate::drive_frames(runtime, document, run_animation_frame, 10).await;
        frames += 10;
        collect_toasts(runtime, toasts);
        // Checked before settling: a crash fallback is reported at once, never
        // waited on or mistaken for dialogue text.
        if let Some(text) = error_boundary(runtime) {
            return StoryState::ErrorBoundary(text, frames);
        }
        let text = crate::evaluate_string(runtime, "__rovesStory.snapshot()");
        if text.is_empty() || previous.contains(&text) {
            candidate.clear();
            stable = 0;
            continue;
        }
        if text == candidate {
            stable += 1;
            if stable >= STORY_STABLE_CHECKS {
                return StoryState::Settled(text, frames);
            }
        } else {
            candidate = text;
            stable = 0;
        }
    }
    StoryState::Timeout(candidate)
}

/// Center point of a helper result, if the helper found an element.
fn point(json: &str) -> Option<(f32, f32, deno_core::serde_json::Value)> {
    let value: deno_core::serde_json::Value = deno_core::serde_json::from_str(json).ok()?;
    let x = value.get("x")?.as_f64()? as f32;
    let y = value.get("y")?.as_f64()? as f32;
    Some((x, y, value))
}

/// Evaluates a runtime-built script that returns a string.
fn evaluate_owned(runtime: &mut JsRuntime, source: String) -> String {
    let value = runtime
        .execute_script("roves-blitz-story-eval.js", source)
        .expect("story helper should not throw");
    deno_core::scope!(scope, runtime);
    let value = v8::Local::new(scope, value);
    value.to_rust_string_lossy(scope)
}

fn excerpt(text: &str) -> String {
    text.chars().take(240).collect()
}

/// Dialogue-like lines currently on the page that were not on the menu.
fn dialogue_lines(runtime: &mut JsRuntime, menu: &str) -> Vec<String> {
    let lines = evaluate_owned(
        runtime,
        format!(
            "__rovesStory.lines({})",
            deno_core::serde_json::Value::String(menu.to_owned())
        ),
    );
    deno_core::serde_json::from_str(&lines).unwrap_or_default()
}

/// Optional `ROVES_PROBE_STORY_EXPECT`: a JSON array of two strings that the
/// first and the second story state's new dialogue must contain.
fn expected_lines() -> Result<Option<[String; 2]>, String> {
    let Ok(raw) = std::env::var("ROVES_PROBE_STORY_EXPECT") else {
        return Ok(None);
    };
    deno_core::serde_json::from_str::<[String; 2]>(&raw)
        .map(Some)
        .map_err(|error| format!("invalid ROVES_PROBE_STORY_EXPECT {raw:?}: {error}"))
}

/// `--story` (opt-in, separate from the smoke gate): from the start menu, a
/// real Start click (Blitz hit-test + trusted-event bridge, as a window's input
/// layer sends it), then the first two settled story states. State 1 must show
/// dialogue text that was not on the menu; state 2 must add a dialogue line
/// absent from state 1 and is reached by the first advance method that works: a
/// "next" control, a click on the dialogue text, Enter, Space. Toast text never
/// counts as a state; any visible error toast, and any script error or
/// unhandled rejection captured after the story starts, fails the run. An
/// error-boundary fallback ("Something went wrong" / "Show Error") fails the
/// run at the first check that sees it, before Start or while waiting. With
/// `ROVES_PROBE_STORY_EXPECT='["line 1","line 2"]'` the two states must also
/// contain those known lines. Prints one `game_story` result; any failure is
/// reported with its reason, never as a pass.
///
/// Before Start, `settings_popup` opens and closes the Settings dialog
/// (`game_settings_popup`); after the two states, `name_input` types a name into
/// the story's input dialog (`game_name_input`); after an accepted name,
/// `hud_save_load` clicks the HUD's Save/Load button (`game_hud_save_load`).
/// Each prints its own result and counts only the script errors captured during
/// its own phase.
async fn story(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    gpu_pacing_stats: &v8::Global<v8::Function>,
) {
    crate::stage("game_story");
    runtime
        .execute_script("roves-blitz-story-helpers.js", STORY_HELPERS)
        .expect("story helpers should install");
    crate::stage("game_settings_popup");
    settings_popup(runtime, document, run_animation_frame, dispatcher).await;
    crate::stage("game_story_dialogues");
    let progress = two_dialogues(
        runtime,
        document,
        run_animation_frame,
        dispatcher,
        gpu_pacing_stats,
    )
    .await;
    if progress.is_some() {
        crate::stage("game_quick_save");
        quick_save(runtime, document, run_animation_frame, dispatcher).await;
    } else {
        println!(
            r#"ROVES_PROBE_RESULT {{"check":"game_quick_save","status":"not_run","detail":{{"reason":"story setup failed"}}}}"#
        );
    }
    let menu = progress.as_ref().map(|progress| progress.menu.clone());
    crate::stage("game_name_input");
    let named = name_input(runtime, document, run_animation_frame, dispatcher, progress).await;
    crate::stage("game_hud_save_load");
    hud_save_load(
        runtime,
        document,
        run_animation_frame,
        dispatcher,
        menu.filter(|_| named),
    )
    .await;
    if let Ok(path) = std::env::var("ROVES_PROBE_STORY_SCREENSHOT") {
        let pixels = crate::render_rgba(document);
        crate::write_png(&path, &pixels);
    }
}

/// What `name_input` needs from the two-dialogue run: the menu text (dialogue
/// lines are text not on the menu), the advance method that produced state 2,
/// and the last settled page text.
struct StoryProgress {
    menu: String,
    method: &'static str,
    last_state: String,
}

/// The two-dialogue story run (see `story`); prints `game_story` and returns the
/// progress only when it passed.
async fn two_dialogues(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    gpu_pacing_stats: &v8::Global<v8::Function>,
) -> Option<StoryProgress> {
    use deno_core::serde_json::{Value, json};
    let errors_before = error_count(runtime);
    let menu = crate::evaluate_string(runtime, "__rovesStory.snapshot()");
    let mut advance_method: Option<&'static str> = None;
    let mut report = json!({ "menu": excerpt(&menu) });
    let mut steps: Vec<Value> = Vec::new();
    let mut states: Vec<String> = Vec::new();
    // New dialogue lines of state 1 and state 2 (state 2: lines not in state 1).
    let mut dialogue: Vec<Vec<String>> = Vec::new();
    let mut toasts: Vec<Value> = Vec::new();
    collect_toasts(runtime, &mut toasts);

    let failure: Option<String> = 'story: {
        let expected = match expected_lines() {
            Ok(expected) => expected,
            Err(error) => break 'story Some(error),
        };
        if let Some(text) = error_boundary(runtime) {
            break 'story Some(format!(
                "error boundary shown before Start: {:?}",
                excerpt(&text)
            ));
        }
        crate::resolve_layout(document);
        let start = crate::evaluate_string(runtime, "__rovesStory.start()");
        let Some((x, y, start)) = point(&start) else {
            break 'story Some("no visible Start/New game button on the page".into());
        };
        let Some(hit) = crate::click_at(runtime, dispatcher, document, x, y) else {
            break 'story Some(format!(
                "hit-test found no node at the Start button ({x}, {y})"
            ));
        };
        steps.push(json!({ "action": "click_start", "button": start, "hit_node": hit.as_u64() }));
        // State 1 must show dialogue text; up to two settled states without it
        // (e.g. a loading screen) are skipped first.
        let mut previous = vec![menu.clone()];
        loop {
            match wait_for_state(
                runtime,
                document,
                run_animation_frame,
                &previous,
                &mut toasts,
            )
            .await
            {
                StoryState::Settled(text, frames) => {
                    let lines = dialogue_lines(runtime, &menu);
                    if lines.is_empty() {
                        steps.push(json!({ "after": "click_start", "result": "settled without dialogue text", "frames": frames, "last_text": excerpt(&text) }));
                        previous.push(text);
                        if previous.len() > 3 {
                            break 'story Some(
                                "no state after Start showed dialogue text, only menu/control text"
                                    .into(),
                            );
                        }
                        continue;
                    }
                    steps.push(
                        json!({ "state": 1, "frames": frames, "dialogue_lines": lines.len() }),
                    );
                    previous.push(text.clone());
                    states.push(text);
                    dialogue.push(lines);
                    break;
                },
                StoryState::Timeout(last) => {
                    break 'story Some(format!(
                        "no settled state after Start within {STORY_STATE_FRAMES} frames (last text: {:?})",
                        excerpt(&last)
                    ));
                },
                StoryState::ErrorBoundary(text, frames) => {
                    steps.push(json!({ "after": "click_start", "result": "error boundary", "frames": frames }));
                    break 'story Some(format!(
                        "error boundary shown after Start ({frames} frames): {:?}",
                        excerpt(&text)
                    ));
                },
            }
        }
        for method in ADVANCE_METHODS {
            let Some(dispatched) = advance(runtime, document, dispatcher, method, &menu) else {
                steps.push(json!({ "action": method, "result": "no target" }));
                continue;
            };
            match wait_for_state(
                runtime,
                document,
                run_animation_frame,
                &previous,
                &mut toasts,
            )
            .await
            {
                StoryState::Settled(text, frames) => {
                    // Only a dialogue line absent from state 1 is story progress;
                    // other page changes (UI counters, overlays) are not.
                    let new_lines: Vec<String> = dialogue_lines(runtime, &menu)
                        .into_iter()
                        .filter(|line| !dialogue[0].contains(line))
                        .collect();
                    if new_lines.is_empty() {
                        steps.push(json!({ "action": method, "input": dispatched, "result": "settled without a new dialogue line", "frames": frames, "last_text": excerpt(&text) }));
                        previous.push(text);
                        continue;
                    }
                    steps.push(json!({ "action": method, "input": dispatched, "state": 2, "frames": frames, "dialogue_lines": new_lines.len() }));
                    advance_method = Some(method);
                    states.push(text);
                    dialogue.push(new_lines);
                    break;
                },
                StoryState::Timeout(last) => {
                    steps.push(json!({ "action": method, "input": dispatched, "result": "no new settled state", "last_text": excerpt(&last) }));
                    // A text that appeared without settling (e.g. an animation)
                    // is not accepted as the second state by the next method.
                    if !last.is_empty() {
                        previous.push(last);
                    }
                },
                StoryState::ErrorBoundary(text, frames) => {
                    steps.push(json!({ "action": method, "input": dispatched, "result": "error boundary", "frames": frames }));
                    break 'story Some(format!(
                        "error boundary shown after {method} ({frames} frames): {:?}",
                        excerpt(&text)
                    ));
                },
            }
        }
        if states.len() < 2 {
            break 'story Some(
                "no advance method produced a second settled state with a new dialogue line".into(),
            );
        }
        if let Some(expected) = expected {
            for (index, needle) in expected.iter().enumerate() {
                if !dialogue[index]
                    .iter()
                    .any(|line| line.contains(needle.as_str()))
                {
                    break 'story Some(format!(
                        "state {} dialogue does not contain the expected line {needle:?}",
                        index + 1
                    ));
                }
            }
        }
        None
    };
    collect_toasts(runtime, &mut toasts);
    // A visible error toast fails the run even when two states were reached.
    let error_toasts: Vec<String> = toasts
        .iter()
        .filter(|toast| toast["error"] == Value::Bool(true))
        .filter_map(|toast| toast["text"].as_str().map(str::to_owned))
        .collect();
    let failure = failure.or_else(|| {
        (!error_toasts.is_empty())
            .then(|| format!("visible error toast(s) during the story: {error_toasts:?}"))
    });

    let errors = crate::evaluate_string(
        runtime,
        "JSON.stringify(__rovesGameErrors.slice(-10).map((e) => e.slice(0, 300)))",
    );
    // Any script error or unhandled rejection captured after the story started
    // fails the run, even with two dialogue states and no toast: a broken
    // listener (e.g. a keyboard guard throwing) can leave the text advancing
    // while game behavior is already wrong (CI 42917654548 passed that way).
    let (new_errors, new_error_messages) = errors_since(runtime, errors_before);
    let failure = failure.or_else(|| {
        (new_errors > 0).then(|| {
            format!("{new_errors} script error(s) captured during the story: {new_error_messages}")
        })
    });
    let status = if failure.is_none() {
        "passed"
    } else {
        "failed"
    };
    report["reason"] = json!(failure);
    report["states"] = json!(
        states
            .iter()
            .map(|state| excerpt(state))
            .collect::<Vec<_>>()
    );
    report["dialogue"] = json!(
        dialogue
            .iter()
            .map(|lines| lines.iter().map(|line| excerpt(line)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    report["toasts"] = json!(toasts);
    report["steps"] = json!(steps);
    report["new_errors"] = json!(new_errors);
    report["new_error_messages"] =
        deno_core::serde_json::from_str(&new_error_messages).unwrap_or(Value::Null);
    report["recent_errors"] = deno_core::serde_json::from_str(&errors).unwrap_or(Value::Null);
    report["pacing"] =
        deno_core::serde_json::from_str(&crate::call_hook(runtime, gpu_pacing_stats, &[]))
            .unwrap_or(Value::Null);
    // Fixed key order for CI greps; serde_json sorts the detail's keys.
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_story\",\"status\":\"{status}\",\"detail\":{report}}}"
    );
    if failure.is_some() {
        return None;
    }
    Some(StoryProgress {
        menu,
        method: advance_method?,
        last_state: states.pop()?,
    })
}

/// Advance methods tried, in order, to reach story state 2.
const ADVANCE_METHODS: [&str; 4] = ["next_control", "dialogue_click", "enter_key", "space_key"];

/// Sends one advance input of `method` (see `ADVANCE_METHODS`); returns what was
/// dispatched, or `None` when the page has no target for it.
fn advance(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    dispatcher: &v8::Global<v8::Function>,
    method: &str,
    menu: &str,
) -> Option<deno_core::serde_json::Value> {
    use deno_core::serde_json::{Value, json};
    crate::resolve_layout(document);
    match method {
        "next_control" | "dialogue_click" => {
            let helper = if method == "next_control" {
                "__rovesStory.next()".to_string()
            } else {
                format!("__rovesStory.dialogue({})", Value::String(menu.to_owned()))
            };
            let found = evaluate_owned(runtime, helper);
            let (x, y, element) = point(&found)?;
            crate::click_at(runtime, dispatcher, document, x, y)
                .map(|hit| json!({ "element": element, "hit_node": hit.as_u64() }))
        },
        _ => {
            let key = if method == "enter_key" {
                r#"{"key":"Enter","code":"Enter"}"#
            } else {
                r#"{"key":" ","code":"Space"}"#
            };
            let target = {
                let document = document.borrow();
                document.get_focussed_node_id().or_else(|| {
                    let root = document.root_node().id;
                    document
                        .query_selector_all_in(root, "body")
                        .ok()?
                        .into_iter()
                        .next()
                })
            }?;
            crate::dispatch_trusted(runtime, dispatcher, target, "keydown", key);
            crate::dispatch_trusted(runtime, dispatcher, target, "keyup", key);
            Some(json!({ "target_node": target.as_u64() }))
        },
    }
}

/// Number of script errors and unhandled rejections captured so far.
fn error_count(runtime: &mut JsRuntime) -> u64 {
    crate::evaluate_string(runtime, "String(__rovesGameErrors.length)")
        .parse()
        .unwrap_or(0)
}

/// Errors captured since `before` (see `error_count`): count and a JSON array
/// of the first five messages.
fn errors_since(runtime: &mut JsRuntime, before: u64) -> (u64, String) {
    let count = error_count(runtime).saturating_sub(before);
    let messages = evaluate_owned(
        runtime,
        format!(
            "JSON.stringify(__rovesGameErrors.slice({before}).slice(0, 5).map((e) => e.slice(0, 300)))"
        ),
    );
    (count, messages)
}

/// Drives frames (10 per check, at most `max_frames`) until `probe` (a script
/// returning a string) returns a non-empty value; returns it with the frames
/// driven, or `None` with the last value on timeout. Toasts are collected at
/// every check.
async fn wait_until(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    probe: &str,
    max_frames: u32,
    toasts: &mut Vec<deno_core::serde_json::Value>,
) -> Result<(String, u32), String> {
    let mut frames = 0;
    let mut last = String::new();
    while frames < max_frames {
        crate::drive_frames(runtime, document, run_animation_frame, 10).await;
        frames += 10;
        collect_toasts(runtime, toasts);
        crate::resolve_layout(document);
        last = evaluate_owned(runtime, probe.to_owned());
        if !last.is_empty() {
            return Ok((last, frames));
        }
    }
    Err(last)
}

/// Upper bound, in frames, for a dialog to mount, finish opening or unmount.
const DIALOG_FRAMES: u32 = 600;

/// Continue after launching a fresh process with the story run's storage.
async fn restore_story(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
) {
    use deno_core::serde_json::{Value, json};
    crate::stage("game_restore_save");
    runtime
        .execute_script("roves-blitz-story-helpers.js", STORY_HELPERS)
        .expect("story helpers should install");
    let errors_before = error_count(runtime);
    let mut toasts = Vec::new();
    let mut report = json!({});
    let failure = 'restore: {
        let Ok(expected) = std::env::var("ROVES_PROBE_RESTORE_LINE") else {
            break 'restore Some("missing ROVES_PROBE_RESTORE_LINE".into());
        };
        if expected.trim().is_empty() {
            break 'restore Some("the expected restored dialogue line is empty".into());
        }
        let button = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.continueButton()",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        let Some((x, y, _)) = button.ok().and_then(|(button, _)| point(&button)) else {
            break 'restore Some("the main menu did not enable Continue".into());
        };
        if crate::click_at(runtime, dispatcher, document, x, y).is_none() {
            break 'restore Some("the Continue button had no hit target".into());
        }
        let probe = format!("__rovesStory.restoredLine({})", Value::String(expected));
        match wait_until(
            runtime,
            document,
            run_animation_frame,
            &probe,
            STORY_STATE_FRAMES,
            &mut toasts,
        )
        .await
        {
            Ok((line, frames)) => {
                report = json!({ "dialogue": excerpt(&line), "frames": frames });
                None
            },
            Err(_) => Some("Continue did not restore the saved dialogue".into()),
        }
    };
    let (status, report) = phase_result(
        runtime,
        "game_restore_save",
        errors_before,
        failure,
        report,
        toasts,
        Vec::new(),
    );
    println!(
        r#"ROVES_PROBE_RESULT {{"check":"game_restore_save","status":"{status}","detail":{report}}}"#
    );
}

/// Exercise the template's F5 quick save and inspect only its committed row.
/// A fresh timestamp prevents stale slots from satisfying a local replay.
async fn quick_save(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
) {
    use deno_core::serde_json::{Value, json};
    use keyboard_types::{Key, NamedKey};

    let errors_before = error_count(runtime);
    let since = crate::evaluate_string(runtime, "String(Date.now())");
    window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::F5));
    let mut toasts = Vec::new();
    let mut report = json!({});
    let probe = format!("__rovesStory.quickSaveEvidence({since})");
    let failure = match wait_until(
        runtime,
        document,
        run_animation_frame,
        &probe,
        DIALOG_FRAMES,
        &mut toasts,
    )
    .await
    {
        Ok((evidence, frames)) => {
            let evidence: Value = deno_core::serde_json::from_str(&evidence).unwrap_or(Value::Null);
            let valid = evidence["has_state"] == Value::Bool(true)
                && evidence["thumbnail_png"] == Value::Bool(true);
            report = json!({ "evidence": evidence, "frames": frames });
            (!valid).then(|| {
                "the quick-save row did not contain a game state and a PNG thumbnail".into()
            })
        },
        Err(_) => Some("F5 did not produce a fresh IndexedDB quick-save slot".into()),
    };
    let (status, report) = phase_result(
        runtime,
        "game_quick_save",
        errors_before,
        failure,
        report,
        toasts,
        Vec::new(),
    );
    println!(
        r#"ROVES_PROBE_RESULT {{"check":"game_quick_save","status":"{status}","detail":{report}}}"#
    );
}

/// Common tail of the popup/name-input results: toasts (an error toast fails),
/// new script errors since `errors_before` (any fails); returns the status and
/// the detail for the caller's result line.
fn phase_result(
    runtime: &mut JsRuntime,
    check: &str,
    errors_before: u64,
    failure: Option<String>,
    mut report: deno_core::serde_json::Value,
    mut toasts: Vec<deno_core::serde_json::Value>,
    steps: Vec<deno_core::serde_json::Value>,
) -> (&'static str, deno_core::serde_json::Value) {
    use deno_core::serde_json::{Value, json};
    collect_toasts(runtime, &mut toasts);
    let error_toasts: Vec<String> = toasts
        .iter()
        .filter(|toast| toast["error"] == Value::Bool(true))
        .filter_map(|toast| toast["text"].as_str().map(str::to_owned))
        .collect();
    let (new_errors, new_error_messages) = errors_since(runtime, errors_before);
    let failure = failure
        .or_else(|| {
            (!error_toasts.is_empty())
                .then(|| format!("visible error toast(s) during {check}: {error_toasts:?}"))
        })
        .or_else(|| {
            (new_errors > 0).then(|| {
                format!(
                    "{new_errors} script error(s) captured during {check}: {new_error_messages}"
                )
            })
        });
    let status = if failure.is_none() {
        "passed"
    } else {
        "failed"
    };
    report["reason"] = json!(failure);
    report["toasts"] = json!(toasts);
    report["steps"] = json!(steps);
    report["new_errors"] = json!(new_errors);
    report["new_error_messages"] =
        deno_core::serde_json::from_str(&new_error_messages).unwrap_or(Value::Null);
    (status, report)
}

/// Settings popup on the start menu: a trusted click on the menu's "Settings"
/// button must mount the template's Base UI dialog (`role="dialog"`, title
/// "Settings", `data-open`), and Escape, pressed as a keyboard sends it through
/// Blitz's `EventDriver` (`window::press_key`) to the focused element, must
/// close it: the dialog is unmounted (after its exit animation) and the
/// `settings` search parameter is gone. Prints `game_settings_popup`.
async fn settings_popup(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
) {
    use deno_core::serde_json::{Value, json};
    use keyboard_types::{Key, NamedKey};
    let errors_before = error_count(runtime);
    let mut toasts: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    let mut report = json!({});
    let failure: Option<String> = 'popup: {
        if let Some(text) = error_boundary(runtime) {
            break 'popup Some(format!(
                "error boundary shown before Settings: {:?}",
                excerpt(&text)
            ));
        }
        crate::resolve_layout(document);
        let before = crate::evaluate_string(runtime, "__rovesStory.dialogs()");
        if before != "[]" {
            break 'popup Some(format!("a dialog is already mounted on the menu: {before}"));
        }
        let button = crate::evaluate_string(runtime, "__rovesStory.settings()");
        let Some((x, y, button)) = point(&button) else {
            break 'popup Some("no visible Settings button on the menu".into());
        };
        let Some(hit) = crate::click_at(runtime, dispatcher, document, x, y) else {
            break 'popup Some(format!(
                "hit-test found no node at the Settings button ({x}, {y})"
            ));
        };
        steps.push(
            json!({ "action": "click_settings", "button": button, "hit_node": hit.as_u64() }),
        );
        // Open: the Settings dialog is mounted, visible and marked open, and
        // its open animation has settled (no running animation in the dialog).
        let opened = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.dialogOpen(/^settings$/i)",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        let (dialog, frames) = match opened {
            Ok(found) => found,
            Err(last) => {
                let dialogs = crate::evaluate_string(runtime, "__rovesStory.dialogs()");
                break 'popup Some(format!(
                    "the Settings dialog did not open within {DIALOG_FRAMES} frames (last: {last:?}, dialogs: {dialogs}, location: {})",
                    crate::evaluate_string(runtime, "location.href")
                ));
            },
        };
        let dialog: Value = deno_core::serde_json::from_str(&dialog).unwrap_or(Value::Null);
        steps.push(json!({ "result": "dialog open", "frames": frames, "dialog": dialog }));
        report["opened"] = dialog;
        report["location_open"] = json!(crate::evaluate_string(runtime, "location.href"));
        if let Some(text) = error_boundary(runtime) {
            break 'popup Some(format!(
                "error boundary shown with Settings open: {:?}",
                excerpt(&text)
            ));
        }
        let focused = crate::evaluate_string(runtime, "__rovesStory.focused()");
        window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::Escape));
        steps.push(json!({ "action": "escape_key", "focused": deno_core::serde_json::from_str::<Value>(&focused).unwrap_or(Value::Null) }));
        let closed = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.dialogs() === '[]' && !/[?&]settings=/.test(location.search) ? 'closed' : ''",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        match closed {
            Ok((_, frames)) => {
                steps.push(json!({ "result": "dialog unmounted", "frames": frames }));
            },
            Err(_) => {
                let dialogs = crate::evaluate_string(runtime, "__rovesStory.dialogs()");
                break 'popup Some(format!(
                    "the Settings dialog was not closed and unmounted within {DIALOG_FRAMES} frames after Escape (dialogs: {dialogs}, location: {})",
                    crate::evaluate_string(runtime, "location.href")
                ));
            },
        }
        report["location_closed"] = json!(crate::evaluate_string(runtime, "location.href"));
        // Let the menu settle again before the story run takes its snapshot.
        crate::drive_frames(runtime, document, run_animation_frame, 30).await;
        None
    };
    let (status, report) = phase_result(
        runtime,
        "game_settings_popup",
        errors_before,
        failure,
        report,
        toasts,
        steps,
    );
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_settings_popup\",\"status\":\"{status}\",\"detail\":{report}}}"
    );
}

/// Unique name typed into the story's input dialog (no letters bound to the
/// template's single-key hotkeys).
const TYPED_NAME: &str = "Zorvaq";

/// Name prompt with real typing, after the two-dialogue run: advances (with the
/// method that produced state 2) until the template's InputRequestDialog shows
/// its text input (start.label step 5, `requestInput({type:"string"}, "Peter")`),
/// focuses it with a pointer press through Blitz's `EventDriver` if the dialog
/// did not focus it, clears the default with End and Backspace, types
/// `TYPED_NAME` key by key through `window::press_key` (Blitz edits its own
/// editor; React's onChange sees the `input` events), checks the value and the
/// enabled Confirm button, tries Enter and the pointer confirmation, then
/// advances until a dialogue line contains the typed name (step 7,
/// `...${mc.name}.`). Prints `game_name_input` (`not_run` when the two-dialogue
/// run did not pass) and returns whether it passed.
async fn name_input(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    progress: Option<StoryProgress>,
) -> bool {
    use deno_core::serde_json::{Value, json};
    use keyboard_types::{Key, NamedKey};
    let Some(progress) = progress else {
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"game_name_input\",\"status\":\"not_run\",\"reason\":\"the two-dialogue story run did not pass\"}}"
        );
        return false;
    };
    let StoryProgress {
        menu,
        method,
        last_state,
    } = progress;
    let errors_before = error_count(runtime);
    let mut toasts: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    let mut report = json!({ "name": TYPED_NAME, "advance_method": method });
    let failure: Option<String> = 'name: {
        // Reach the prompt: at most six advances (state 2 is step 2 of 5).
        let mut previous = vec![last_state];
        let mut prompt = None;
        for _ in 0..6 {
            crate::resolve_layout(document);
            let input = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
            if input != "null" {
                prompt = Some(input);
                break;
            }
            let Some(dispatched) = advance(runtime, document, dispatcher, method, &menu) else {
                break 'name Some(format!("no target for the advance method {method}"));
            };
            match wait_for_state(
                runtime,
                document,
                run_animation_frame,
                &previous,
                &mut toasts,
            )
            .await
            {
                StoryState::Settled(text, frames) => {
                    steps.push(json!({ "action": method, "input": dispatched, "frames": frames, "text": excerpt(&text) }));
                    previous.push(text);
                },
                StoryState::Timeout(last) => {
                    break 'name Some(format!(
                        "no settled state after {method} within {STORY_STATE_FRAMES} frames (last text: {:?})",
                        excerpt(&last)
                    ));
                },
                StoryState::ErrorBoundary(text, frames) => {
                    break 'name Some(format!(
                        "error boundary shown after {method} ({frames} frames): {:?}",
                        excerpt(&text)
                    ));
                },
            }
            // The dialog opens 200 ms after the typewriter ends (debounced).
            if let Ok((input, frames)) = wait_until(
                runtime,
                document,
                run_animation_frame,
                "(() => { const input = __rovesStory.dialogInput(); return input === 'null' ? '' : input; })()",
                60,
                &mut toasts,
            )
            .await
            {
                steps.push(json!({ "result": "input dialog shown", "frames": frames }));
                prompt = Some(input);
                break;
            }
        }
        if prompt.is_none() {
            break 'name Some("the name input dialog did not appear within six advances".into());
        }
        // Let the open animation settle (same condition as the Settings dialog).
        let opened = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.dialogOpen(null)",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        let dialog = match opened {
            Ok((dialog, _)) => dialog,
            Err(last) => {
                break 'name Some(format!(
                    "the input dialog did not finish opening within {DIALOG_FRAMES} frames (last: {last:?})"
                ));
            },
        };
        report["dialog"] = deno_core::serde_json::from_str(&dialog).unwrap_or(Value::Null);
        crate::resolve_layout(document);
        let input = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
        let Some((x, y, input)) = point(&input) else {
            break 'name Some("the input dialog has no visible text input".into());
        };
        report["input_initial"] = input.clone();
        let mut focus_method = "dialog_initial_focus";
        if input["focused"] != Value::Bool(true) {
            focus_method = "pointer_press";
            if !window::pointer_click(runtime, dispatcher, document, x, y) {
                break 'name Some(format!("hit-test found no node at the input ({x}, {y})"));
            }
            let focused = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
            let focused: Value = deno_core::serde_json::from_str(&focused).unwrap_or(Value::Null);
            if focused["focused"] != Value::Bool(true) {
                steps.push(json!({
                    "action": "pointer_press",
                    "result": "input not focused",
                    "focused": crate::evaluate_string(runtime, "__rovesStory.focused()"),
                }));
                focus_method = "tab_key";
                let mut reached_input = false;
                for _ in 0..16 {
                    window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::Tab));
                    let focused = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
                    if deno_core::serde_json::from_str::<Value>(&focused)
                        .is_ok_and(|input| input["focused"] == Value::Bool(true))
                    {
                        reached_input = true;
                        break;
                    }
                }
                if !reached_input {
                    break 'name Some(format!(
                        "pointer and Tab did not focus the input (focused: {})",
                        crate::evaluate_string(runtime, "__rovesStory.focused()")
                    ));
                }
            }
        }
        report["focus_method"] = json!(focus_method);
        crate::evaluate_string(runtime, "__rovesStory.watchNameInput()");
        // Clear the default value as a user would: End, then Backspace.
        let initial_length = input["value"]
            .as_str()
            .map_or(0, |value| value.chars().count());
        window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::End));
        for _ in 0..initial_length + 2 {
            window::press_key(
                runtime,
                dispatcher,
                document,
                Key::Named(NamedKey::Backspace),
            );
        }
        crate::drive_frames(runtime, document, run_animation_frame, 10).await;
        let cleared = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
        let cleared: Value = deno_core::serde_json::from_str(&cleared).unwrap_or(Value::Null);
        steps.push(json!({ "action": "clear", "keys": format!("End + {} Backspace", initial_length + 2), "input": cleared }));
        report["input_cleared"] = cleared.clone();
        if cleared["value"] != json!("") {
            break 'name Some(format!(
                "End/Backspace did not clear the input (value {})",
                cleared["value"]
            ));
        }
        for character in TYPED_NAME.chars() {
            window::press_key(
                runtime,
                dispatcher,
                document,
                Key::Character(character.to_string().into()),
            );
        }
        // React re-renders the controlled input and the Confirm button.
        crate::drive_frames(runtime, document, run_animation_frame, 10).await;
        let typed = crate::evaluate_string(runtime, "__rovesStory.dialogInput()");
        let typed: Value = deno_core::serde_json::from_str(&typed).unwrap_or(Value::Null);
        steps.push(json!({ "action": "type", "keys": TYPED_NAME, "input": typed }));
        report["input_typed"] = typed.clone();
        if typed["value"] != json!(TYPED_NAME) {
            break 'name Some(format!(
                "typing {TYPED_NAME:?} left the input value {}",
                typed["value"]
            ));
        }
        if typed["confirm_disabled"] != Value::Bool(false) {
            break 'name Some(format!(
                "the Confirm button is not enabled after typing (confirm_disabled {})",
                typed["confirm_disabled"]
            ));
        }
        window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::Enter));
        steps.push(json!({ "action": "enter_key" }));
        let input_events = crate::evaluate_string(
            runtime,
            "JSON.stringify(__rovesStory.nameInputEvents ?? [])",
        );
        println!("ROVES_PROBE_NAME_INPUT_EVENTS {input_events}");
        // Accepted once no dialog is still marked open: the template may keep
        // the closed dialog mounted (`data-closed`), so unmounting is not required.
        const NAME_DIALOG_CLOSED: &str = "JSON.parse(__rovesStory.dialogs()).every((dialog) => dialog.closed || !dialog.open) ? 'closed' : ''";
        let mut closed = wait_until(
            runtime,
            document,
            run_animation_frame,
            NAME_DIALOG_CLOSED,
            90,
            &mut toasts,
        )
        .await;
        report["enter_accepted"] = json!(closed.is_ok());
        if closed.is_err() {
            let confirm = crate::evaluate_string(runtime, "__rovesStory.nameConfirm()");
            let Some((x, y, _)) = point(&confirm) else {
                break 'name Some(
                    "Enter left the dialog open with no enabled Confirm button".into(),
                );
            };
            let hit = crate::hit_target(document, x, y).map(|node| node.as_u64());
            let sent = window::pointer_click(runtime, dispatcher, document, x, y);
            report["confirm_pointer"] = json!({ "sent": sent, "hit": hit });
            if !sent {
                break 'name Some("the Confirm button point did not hit any element".into());
            }
            closed = wait_until(
                runtime,
                document,
                run_animation_frame,
                NAME_DIALOG_CLOSED,
                DIALOG_FRAMES,
                &mut toasts,
            )
            .await;
        }
        match closed {
            Ok((_, frames)) => {
                steps.push(json!({ "result": "input dialog closed", "frames": frames }))
            },
            Err(_) => {
                break 'name Some(format!(
                    "Confirm left the input dialog open after {DIALOG_FRAMES} frames (dialogs: {})",
                    crate::evaluate_string(runtime, "__rovesStory.dialogs()")
                ));
            },
        }
        // The accepted value is used by the following steps: advance until a
        // dialogue line (not only the speaker label) contains the name.
        let needle = Value::String(TYPED_NAME.into());
        let mut previous = vec![crate::evaluate_string(runtime, "__rovesStory.snapshot()")];
        for _ in 0..4 {
            let Some(dispatched) = advance(runtime, document, dispatcher, method, &menu) else {
                break 'name Some(format!(
                    "no target for the advance method {method} after the prompt"
                ));
            };
            match wait_for_state(
                runtime,
                document,
                run_animation_frame,
                &previous,
                &mut toasts,
            )
            .await
            {
                StoryState::Settled(text, frames) => {
                    let found =
                        evaluate_owned(runtime, format!("__rovesStory.nameLines({needle})"));
                    let found: Value =
                        deno_core::serde_json::from_str(&found).unwrap_or(Value::Null);
                    steps.push(json!({ "action": method, "input": dispatched, "frames": frames, "text": excerpt(&text), "name_lines": found }));
                    if found["dialogue"]
                        .as_array()
                        .is_some_and(|lines| !lines.is_empty())
                    {
                        // The name can appear in a live typewriter text node
                        // before Pixi'VN commits that line to narration. Do not
                        // hand off to the HUD click test while only the speaker
                        // label is in the settled narration: that click would
                        // legitimately advance the still-running dialogue.
                        let name = Value::String(TYPED_NAME.into());
                        // `wait_until` treats any non-empty evaluation string as ready;
                        // stringify the predicate as an empty/non-empty sentinel because
                        // the literal result "false" would also satisfy that contract.
                        let settled_named_line = format!(
                            "JSON.parse(__rovesStory.narration({})).some((line) => line !== {} && line.includes({})) ? 'committed' : ''",
                            Value::String(menu.clone()),
                            name,
                            name,
                        );
                        if wait_until(
                            runtime,
                            document,
                            run_animation_frame,
                            &settled_named_line,
                            STORY_STATE_FRAMES,
                            &mut toasts,
                        )
                        .await
                        .is_err()
                        {
                            break 'name Some(format!(
                                "the typewriter exposed {TYPED_NAME:?} in the DOM but never committed a named dialogue line to narration (current: {})",
                                evaluate_owned(
                                    runtime,
                                    format!(
                                        "__rovesStory.narration({})",
                                        Value::String(menu.clone())
                                    )
                                )
                            ));
                        }
                        report["name_in_dialogue"] = found;
                        break 'name None;
                    }
                    previous.push(text);
                },
                StoryState::Timeout(last) => {
                    break 'name Some(format!(
                        "no settled state after the prompt within {STORY_STATE_FRAMES} frames (last text: {:?})",
                        excerpt(&last)
                    ));
                },
                StoryState::ErrorBoundary(text, frames) => {
                    break 'name Some(format!(
                        "error boundary shown after the prompt ({frames} frames): {:?}",
                        excerpt(&text)
                    ));
                },
            }
        }
        Some(format!(
            "no dialogue line contained the typed name {TYPED_NAME:?} within four advances after the prompt"
        ))
    };
    let (status, report) = phase_result(
        runtime,
        "game_name_input",
        errors_before,
        failure,
        report,
        toasts,
        steps,
    );
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_name_input\",\"status\":\"{status}\",\"detail\":{report}}}"
    );
    status == "passed"
}

/// Elements from `hit` up to the root (at most `HIT_PATH_DEPTH` described):
/// tag, class excerpt and the `data-roves-probe-target` marker; returns whether
/// any of them carries the marker, and the description.
fn hit_path(
    document: &Rc<RefCell<HtmlDocument>>,
    hit: blitz_dom::NodeId,
) -> (bool, Vec<deno_core::serde_json::Value>) {
    use deno_core::serde_json::json;
    const HIT_PATH_DEPTH: usize = 6;
    let document = document.borrow();
    let mut on_target = false;
    let mut path = Vec::new();
    let mut current = Some(hit);
    while let Some(id) = current {
        let Some(node) = document.get_node(id) else {
            break;
        };
        if let Some(element) = node.element_data() {
            let attr = |name: &str| {
                node.attrs().and_then(|attrs| {
                    attrs
                        .iter()
                        .find(|attr| &*attr.name.local == name)
                        .map(|attr| attr.value.clone())
                })
            };
            let marked = attr("data-roves-probe-target").is_some();
            on_target |= marked;
            if path.len() < HIT_PATH_DEPTH {
                path.push(json!({
                    "node": id.as_u64(),
                    "tag": &*element.name.local,
                    "class": attr("class").map(|class| class.chars().take(80).collect::<String>()),
                    "marked": marked,
                }));
            }
        }
        current = node.parent;
    }
    (on_target, path)
}

/// Frames driven after the Save/Load panel opens before the narration is
/// compared again, so a click-through advance has time to render.
const HUD_ADVANCE_FRAMES: u32 = 120;

/// HUD Save/Load (Windows GTX 1050 window test, 2026-11-13: after the name and
/// Confirm, an OS click on Save/Load advanced the dialogue twice instead of
/// opening the panel). After an accepted name, waits until the narration
/// outside dialogs is settled, finds the HUD's "Save/Load" button by label,
/// reads its `getBoundingClientRect` and sends a pointer press and release at
/// its center through Blitz's `EventDriver` (`window::pointer_click`, as the
/// window's input layer does). Records the hit-test path and whether it lies in
/// the button, then requires the save/load page of the Settings dialog to open
/// and no new narration line to appear, both while the panel is open and after
/// Escape closes it. Prints `game_hud_save_load` (`not_run` without an accepted
/// name).
async fn hud_save_load(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    menu: Option<String>,
) {
    use deno_core::serde_json::{Value, json};
    use keyboard_types::{Key, NamedKey};
    let Some(menu) = menu else {
        println!(
            r#"ROVES_PROBE_RESULT {{"check":"game_hud_save_load","status":"not_run","detail":{{"reason":"the name input run did not pass"}}}}"#
        );
        return;
    };
    let errors_before = error_count(runtime);
    let mut toasts: Vec<Value> = Vec::new();
    let mut steps: Vec<Value> = Vec::new();
    let mut report = json!({});
    let narration_probe = format!("__rovesStory.narration({})", Value::String(menu));
    let new_lines = |runtime: &mut JsRuntime, before: &[String]| -> Vec<String> {
        let now = evaluate_owned(runtime, narration_probe.clone());
        let mut now: Vec<String> = deno_core::serde_json::from_str(&now).unwrap_or_default();
        now.retain(|line| !before.contains(line));
        now.iter().map(|line| excerpt(line)).collect()
    };
    let failure: Option<String> = 'hud: {
        if wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.dialogs() === '[]' ? 'none' : ''",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await
        .is_err()
        {
            break 'hud Some(format!(
                "a dialog is still mounted before the HUD click: {}",
                crate::evaluate_string(runtime, "__rovesStory.dialogs()")
            ));
        }
        // Settled narration: the same non-empty lines for STORY_STABLE_CHECKS checks.
        let mut before = String::new();
        let mut stable = 0;
        let mut frames = 0;
        while stable < STORY_STABLE_CHECKS {
            if frames >= STORY_STATE_FRAMES {
                break 'hud Some(format!(
                    "the narration did not settle within {STORY_STATE_FRAMES} frames (last: {:?})",
                    excerpt(&before)
                ));
            }
            crate::drive_frames(runtime, document, run_animation_frame, 10).await;
            frames += 10;
            collect_toasts(runtime, &mut toasts);
            let lines = evaluate_owned(runtime, narration_probe.clone());
            if lines != "[]" && lines == before {
                stable += 1;
            } else {
                before = lines;
                stable = 0;
            }
        }
        let before: Vec<String> = deno_core::serde_json::from_str(&before).unwrap_or_default();
        let excerpts: Vec<String> = before.iter().map(|line| excerpt(line)).collect();
        report["narration_before"] = json!(excerpts);
        steps.push(json!({ "result": "narration settled", "frames": frames }));
        crate::resolve_layout(document);
        let button = crate::evaluate_string(runtime, "__rovesStory.hudSaveLoad()");
        let Some((x, y, button)) = point(&button) else {
            break 'hud Some("no visible HUD Save/Load button outside dialogs".into());
        };
        report["button"] = button;
        let hit = crate::hit_target(document, x, y);
        let (on_target, path) = hit.map(|hit| hit_path(document, hit)).unwrap_or_default();
        report["hit"] = json!({ "x": x, "y": y, "node": hit.map(|hit| hit.as_u64()), "on_target": on_target, "path": path });
        let sent = window::pointer_click(runtime, dispatcher, document, x, y);
        crate::evaluate_string(runtime, "__rovesStory.clearProbeTarget()");
        steps.push(json!({ "action": "pointer_click", "sent": sent }));
        if !sent {
            break 'hud Some(format!(
                "hit-test found no node at the Save/Load button center ({x}, {y})"
            ));
        }
        let opened = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.saveLoadPanel()",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        crate::drive_frames(runtime, document, run_animation_frame, HUD_ADVANCE_FRAMES).await;
        let advanced_open = new_lines(runtime, &before);
        report["new_lines_open"] = json!(advanced_open);
        report["location_open"] = json!(crate::evaluate_string(runtime, "location.href"));
        let panel = match opened {
            Ok((panel, frames)) => {
                steps.push(json!({ "result": "save/load panel open", "frames": frames }));
                panel
            },
            Err(_) => {
                break 'hud Some(format!(
                    "the Save/Load click did not open the save/load panel within {DIALOG_FRAMES} frames (hit on button: {on_target}, new narration lines: {advanced_open:?}, dialogs: {}, location: {})",
                    crate::evaluate_string(runtime, "__rovesStory.dialogs()"),
                    crate::evaluate_string(runtime, "location.href")
                ));
            },
        };
        let panel: Value = deno_core::serde_json::from_str(&panel).unwrap_or(Value::Null);
        report["panel"] = panel;
        if !advanced_open.is_empty() {
            break 'hud Some(format!(
                "the Save/Load click advanced the dialogue (hit on button: {on_target}, new lines: {advanced_open:?})"
            ));
        }
        if !on_target {
            break 'hud Some(format!(
                "the panel opened, but the hit test at the button center ({x}, {y}) resolved outside the Save/Load button"
            ));
        }
        window::press_key(runtime, dispatcher, document, Key::Named(NamedKey::Escape));
        let closed = wait_until(
            runtime,
            document,
            run_animation_frame,
            "__rovesStory.dialogs() === '[]' ? 'closed' : ''",
            DIALOG_FRAMES,
            &mut toasts,
        )
        .await;
        report["closed"] = json!(closed.is_ok());
        crate::drive_frames(runtime, document, run_animation_frame, HUD_ADVANCE_FRAMES).await;
        let advanced_closed = new_lines(runtime, &before);
        report["new_lines_closed"] = json!(advanced_closed);
        if closed.is_err() {
            break 'hud Some(format!(
                "Escape left the save/load panel mounted after {DIALOG_FRAMES} frames (dialogs: {})",
                crate::evaluate_string(runtime, "__rovesStory.dialogs()")
            ));
        }
        steps.push(json!({ "action": "escape_key", "result": "dialog unmounted" }));
        if advanced_closed.is_empty() {
            None
        } else {
            Some(format!(
                "the dialogue advanced after closing the save/load panel: {advanced_closed:?}"
            ))
        }
    };
    let (status, report) = phase_result(
        runtime,
        "game_hud_save_load",
        errors_before,
        failure,
        report,
        toasts,
        steps,
    );
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"game_hud_save_load\",\"status\":\"{status}\",\"detail\":{report}}}"
    );
}
