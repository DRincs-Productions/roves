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
/// directory. Missing files are reported and left unanswered.
struct GameNetProvider(Arc<GameContent>);

impl NetProvider for GameNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url;
        let file = self
            .0
            .path_for(url.scheme(), url.host_str(), url.path())
            .map(std::fs::read);
        match file {
            Some(Ok(bytes)) => handler.bytes(url.to_string(), Bytes::from(bytes)),
            _ => eprintln!("ROVES_PROBE_GAME_MISSING_RESOURCE {url}"),
        }
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

    if std::env::args().any(|arg| arg == "--story") {
        story(
            runtime,
            document,
            run_animation_frame,
            dispatcher,
            gpu_pacing_stats,
        )
        .await;
    }
}

/// Upper bound, in frames, for each story state to appear and settle.
const STORY_STATE_FRAMES: u32 = 1800;
/// A state is settled when the page text is unchanged for this many checks
/// (one check every 10 frames), so a typewriter effect finishes first.
const STORY_STABLE_CHECKS: u32 = 3;

/// Page-side helpers of the `--story` harness, evaluated in the page's global.
/// They only read the DOM (text, button labels, rects); input goes through the
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
  return {
    snapshot,
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
async fn story(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    dispatcher: &v8::Global<v8::Function>,
    gpu_pacing_stats: &v8::Global<v8::Function>,
) {
    use deno_core::serde_json::{Value, json};
    crate::stage("game_story");
    runtime
        .execute_script("roves-blitz-story-helpers.js", STORY_HELPERS)
        .expect("story helpers should install");
    let errors_before: u64 = crate::evaluate_string(runtime, "String(__rovesGameErrors.length)")
        .parse()
        .unwrap_or(0);
    let menu = crate::evaluate_string(runtime, "__rovesStory.snapshot()");
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
        for method in ["next_control", "dialogue_click", "enter_key", "space_key"] {
            crate::resolve_layout(document);
            let dispatched = match method {
                "next_control" | "dialogue_click" => {
                    let helper = if method == "next_control" {
                        "__rovesStory.next()".to_string()
                    } else {
                        format!("__rovesStory.dialogue({})", Value::String(menu.clone()))
                    };
                    let found = evaluate_owned(runtime, helper);
                    match point(&found) {
                        Some((x, y, element)) => {
                            crate::click_at(runtime, dispatcher, document, x, y)
                                .map(|hit| json!({ "element": element, "hit_node": hit.as_u64() }))
                        },
                        None => None,
                    }
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
                    };
                    target.map(|target| {
                        crate::dispatch_trusted(runtime, dispatcher, target, "keydown", key);
                        crate::dispatch_trusted(runtime, dispatcher, target, "keyup", key);
                        json!({ "target_node": target.as_u64() })
                    })
                },
            };
            let Some(dispatched) = dispatched else {
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

    let errors_after: u64 = crate::evaluate_string(runtime, "String(__rovesGameErrors.length)")
        .parse()
        .unwrap_or(0);
    let errors = crate::evaluate_string(
        runtime,
        "JSON.stringify(__rovesGameErrors.slice(-10).map((e) => e.slice(0, 300)))",
    );
    // Any script error or unhandled rejection captured after the story started
    // fails the run, even with two dialogue states and no toast: a broken
    // listener (e.g. a keyboard guard throwing) can leave the text advancing
    // while game behavior is already wrong (CI 42917654548 passed that way).
    let new_errors = errors_after.saturating_sub(errors_before);
    let new_error_messages = evaluate_owned(
        runtime,
        format!(
            "JSON.stringify(__rovesGameErrors.slice({errors_before}).slice(0, 5).map((e) => e.slice(0, 300)))"
        ),
    );
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
    if let Ok(path) = std::env::var("ROVES_PROBE_STORY_SCREENSHOT") {
        let pixels = crate::render_rgba(document);
        crate::write_png(&path, &pixels);
    }
}
