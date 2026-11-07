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

/// The document URL games see (matches `src/history.js`).
pub const GAME_URL: &str = "game://content/index.html";

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
        r#"JSON.stringify({
  errors: __rovesGameErrors.slice(0, 30),
  error_count: __rovesGameErrors.length,
  root_children: document.getElementById("root")?.childNodes.length ?? -1,
  body_elements: document.body?.querySelectorAll("*").length ?? -1,
  text: (document.body?.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 300),
})"#,
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
}
