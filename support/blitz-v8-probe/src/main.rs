use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blitz_dom::{BaseDocument, DocumentConfig, LocalName, Node, NodeData, QualName, ns};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use deno_core::{JsRuntime, OpState, RuntimeOptions, op2, v8};
use deno_error::JsErrorBox;

const FIXTURE: &str = include_str!("../fixtures/layout.html");
const DOM_BOOTSTRAP: &str = include_str!("dom.js");
const GUEST_TEST: &str = include_str!("guest_test.js");
const FRAME_TEST: &str = include_str!("frame_test.js");
const FRAME_CHECK: &str = include_str!("frame_check.js");
/// Built by `npm run build` in `js/` (Motion 12.38.0 bundled with esbuild).
const MOTION_SCENARIO: &str = include_str!("../js/dist/motion-scenario.js");

const ELEMENT_NODE: i32 = 1;
const TEXT_NODE: i32 = 3;
const COMMENT_NODE: i32 = 8;
const DOCUMENT_NODE: i32 = 9;

/// Status codes returned by tree-mutating ops; `dom.js` turns them into the
/// matching `DOMException` (from deno_web) so no custom error classes are needed.
const OK: i32 = 0;
const HIERARCHY_REQUEST_ERROR: i32 = 1;
const NOT_FOUND_ERROR: i32 = 2;

struct ProbeDocument(Rc<RefCell<HtmlDocument>>);

/// Clones the shared document handle out of `OpState`; the `RefCell` borrow is
/// taken by each op only for the duration of that synchronous call, never across
/// guest execution.
fn probe_document(state: &Rc<RefCell<OpState>>) -> Rc<RefCell<HtmlDocument>> {
    state.borrow().borrow::<ProbeDocument>().0.clone()
}

fn attached_node(document: &BaseDocument, node_id: u32) -> Result<&Node, JsErrorBox> {
    document
        .get_node(node_id as usize)
        .ok_or_else(|| JsErrorBox::type_error("unknown Blitz node handle"))
}

/// DOM `getElementById`: first element in tree order whose `id` equals `id`.
/// Walks the public node tree instead of Blitz's crate-private `nodes_to_id`
/// map, so no selector parsing or escaping is involved.
fn find_element_by_id(document: &BaseDocument, id: &str) -> Option<usize> {
    let mut stack = vec![document.root_node().id];
    while let Some(node_id) = stack.pop() {
        let node = document.get_node(node_id)?;
        if node.attrs().is_some_and(|attrs| {
            attrs
                .iter()
                .any(|attr| &*attr.name.local == "id" && attr.value == id)
        }) {
            return Some(node_id);
        }
        stack.extend(node.children.iter().rev().copied());
    }
    None
}

fn html_name(name: &str) -> QualName {
    QualName::new(None, ns!(html), LocalName::from(name))
}

#[op2(fast)]
fn op_blitz_get_element_by_id(state: Rc<RefCell<OpState>>, #[string] id: &str) -> i32 {
    if id.is_empty() {
        return -1;
    }
    let document = probe_document(&state);
    let document = document.borrow();
    find_element_by_id(&document, id).map_or(-1, |node_id| node_id as i32)
}

#[op2(fast)]
fn op_blitz_node_type(state: Rc<RefCell<OpState>>, #[smi] node_id: u32) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(match attached_node(&document, node_id)?.data {
        NodeData::Document => DOCUMENT_NODE,
        NodeData::Element(_) | NodeData::AnonymousBlock(_) => ELEMENT_NODE,
        NodeData::Text(_) => TEXT_NODE,
        NodeData::Comment => COMMENT_NODE,
    })
}

#[op2(fast)]
fn op_blitz_parent(state: Rc<RefCell<OpState>>, #[smi] node_id: u32) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, node_id)?
        .parent
        .map_or(-1, |parent| parent as i32))
}

#[op2]
#[string]
fn op_blitz_local_name(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, node_id)?;
    Ok(node
        .element_data()
        .map(|element| element.name.local.to_string())
        .unwrap_or_default())
}

#[op2]
#[string]
fn op_blitz_text_content(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, node_id)?.text_content())
}

/// DOM `textContent` setter. Text nodes are updated in place. Element children are
/// detached with `remove_node`, not dropped: Blitz stores nodes in a slab that
/// reuses ids of dropped nodes, which would let a cached JS wrapper alias an
/// unrelated new node. Dropping needs a GC-aware wrapper lifecycle first.
#[op2(fast)]
fn op_blitz_set_text_content(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, node_id)?;
    let node_id = node.id;
    if node.is_text_node() {
        document.mutate().set_node_text(node_id, value);
        return Ok(());
    }
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error(
            "textContent is only supported on elements and text nodes",
        ));
    }
    let children = node.children.clone();
    let mut mutator = document.mutate();
    for child in children {
        mutator.remove_node(child);
    }
    if !value.is_empty() {
        let text = mutator.create_text_node(value);
        mutator.append_children(node_id, &[text]);
    }
    Ok(())
}

/// `document.createElement` for the HTML namespace; `dom.js` lowercases and
/// validates the name before calling.
#[op2(fast)]
fn op_blitz_create_element(state: Rc<RefCell<OpState>>, #[string] local_name: &str) -> i32 {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    document
        .mutate()
        .create_element(html_name(local_name), Vec::new()) as i32
}

#[op2(fast)]
fn op_blitz_create_text_node(state: Rc<RefCell<OpState>>, #[string] data: &str) -> i32 {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    document.mutate().create_text_node(data) as i32
}

/// `appendChild`: rejects inserting a node into itself or its descendants and
/// a document node anywhere, then detaches the child from any previous parent
/// (Blitz's `append_children` does not) and appends it.
#[op2(fast)]
fn op_blitz_append_child(
    state: Rc<RefCell<OpState>>,
    #[smi] parent_id: u32,
    #[smi] child_id: u32,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let child = attached_node(&document, child_id)?;
    if matches!(child.data, NodeData::Document) {
        return Ok(HIERARCHY_REQUEST_ERROR);
    }
    let parent = attached_node(&document, parent_id)?;
    if matches!(parent.data, NodeData::Text(_) | NodeData::Comment) {
        return Ok(HIERARCHY_REQUEST_ERROR);
    }
    let mut ancestor = Some(parent.id);
    while let Some(id) = ancestor {
        if id == child_id as usize {
            return Ok(HIERARCHY_REQUEST_ERROR);
        }
        ancestor = document.get_node(id).and_then(|node| node.parent);
    }
    let had_parent = child.parent.is_some();
    let mut mutator = document.mutate();
    if had_parent {
        mutator.remove_node(child_id as usize);
    }
    mutator.append_children(parent_id as usize, &[child_id as usize]);
    Ok(OK)
}

/// `removeChild`: detaches without dropping (see `op_blitz_set_text_content`).
#[op2(fast)]
fn op_blitz_remove_child(
    state: Rc<RefCell<OpState>>,
    #[smi] parent_id: u32,
    #[smi] child_id: u32,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    attached_node(&document, parent_id)?;
    if attached_node(&document, child_id)?.parent != Some(parent_id as usize) {
        return Ok(NOT_FOUND_ERROR);
    }
    document.mutate().remove_node(child_id as usize);
    Ok(OK)
}

#[op2]
#[string]
fn op_blitz_get_attribute(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
    #[string] name: &str,
) -> Result<Option<String>, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, node_id)?;
    Ok(node.attrs().and_then(|attrs| {
        attrs
            .iter()
            .find(|attr| &*attr.name.local == name)
            .map(|attr| attr.value.clone())
    }))
}

#[op2(fast)]
fn op_blitz_set_attribute(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
    #[string] name: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    if attached_node(&document, node_id)?.element_data().is_none() {
        return Err(JsErrorBox::type_error("setAttribute on a non-element"));
    }
    document
        .mutate()
        .set_attribute(node_id as usize, html_name(name), value);
    Ok(())
}

#[op2]
#[string]
fn op_blitz_namespace(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, node_id)?;
    Ok(node
        .element_data()
        .map(|element| element.name.ns.to_string())
        .unwrap_or_default())
}

/// Inline style write (`element.style.x = v`); an empty value removes the
/// declaration, like `style.removeProperty`.
#[op2(fast)]
fn op_blitz_set_style_property(
    state: Rc<RefCell<OpState>>,
    #[smi] node_id: u32,
    #[string] name: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    if attached_node(&document, node_id)?.element_data().is_none() {
        return Err(JsErrorBox::type_error("style on a non-element"));
    }
    let mut mutator = document.mutate();
    if value.is_empty() {
        mutator.remove_style_property(node_id as usize, name);
    } else {
        mutator.set_style_property(node_id as usize, name, value);
    }
    Ok(())
}

deno_core::extension!(
    roves_blitz_v8_probe,
    ops = [
        op_blitz_get_element_by_id,
        op_blitz_node_type,
        op_blitz_parent,
        op_blitz_local_name,
        op_blitz_text_content,
        op_blitz_set_text_content,
        op_blitz_create_element,
        op_blitz_create_text_node,
        op_blitz_append_child,
        op_blitz_remove_child,
        op_blitz_get_attribute,
        op_blitz_set_attribute,
        op_blitz_set_style_property,
        op_blitz_namespace,
    ]
);

fn resolve_layout(document: &Rc<RefCell<HtmlDocument>>) {
    let mut document = document.borrow_mut();
    document.set_viewport(Viewport::new(
        VIEWPORT.0,
        VIEWPORT.1,
        1.0,
        ColorScheme::Light,
    ));
    document.resolve(0.0);
}

/// Hit-tests a point with Blitz and returns the event target: the hit node, or
/// its parent element when the hit lands on a text node (text is never a
/// mouse event target).
fn hit_target(document: &Rc<RefCell<HtmlDocument>>, x: f32, y: f32) -> Option<usize> {
    let document = document.borrow();
    let hit = document.hit(x, y)?;
    let node = document.get_node(hit.node_id)?;
    if node.is_text_node() {
        node.parent
    } else {
        Some(hit.node_id)
    }
}

const VIEWPORT: (u32, u32) = (640, 360);

/// Border-box rectangle `(x, y, width, height)` of the element with `id`, in
/// viewport pixels. Blitz `final_layout` locations are relative to the parent.
fn element_rect(document: &Rc<RefCell<HtmlDocument>>, id: &str) -> (f32, f32, f32, f32) {
    let document = document.borrow();
    let node_id = find_element_by_id(&document, id).expect("element present");
    let size = document.get_node(node_id).unwrap().final_layout.size;
    let (mut x, mut y) = (0.0, 0.0);
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = document.get_node(id).unwrap();
        x += node.final_layout.location.x;
        y += node.final_layout.location.y;
        current = node.parent;
    }
    (x, y, size.width, size.height)
}

/// Paints the resolved document into an RGBA8 buffer. Uses `ImageRenderer::render`
/// on a pre-sized buffer: `render_to_vec`/`render_to_buffer` in anyrender_vello
/// 0.6.0 pass a zero-length vector to the copy-out step.
fn render_rgba(document: &Rc<RefCell<HtmlDocument>>) -> Vec<u8> {
    use anyrender::ImageRenderer;
    let (width, height) = VIEWPORT;
    let document = document.borrow();
    let mut renderer = anyrender_vello::VelloImageRenderer::new(width, height);
    let mut pixels = vec![0; (width * height * 4) as usize];
    renderer.render(
        |scene| blitz_paint::paint_scene(scene, &document, 1.0, width, height),
        &mut pixels,
    );
    pixels
}

fn pixel(pixels: &[u8], x: f32, y: f32) -> [u8; 4] {
    let index = ((y as usize) * VIEWPORT.0 as usize + x as usize) * 4;
    pixels[index..index + 4].try_into().unwrap()
}

fn close(actual: [u8; 4], expected: [u8; 3]) -> bool {
    actual[..3]
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 2)
        && actual[3] == 255
}

fn assert_color(actual: [u8; 4], expected: [u8; 3], what: &str) {
    assert!(
        close(actual, expected),
        "{what}: expected {expected:?}, painted {actual:?}"
    );
}

fn count_differing(pixels: &[u8], rect: (f32, f32, f32, f32), background: [u8; 3]) -> usize {
    let (x0, y0, width, height) = rect;
    let mut count = 0;
    for y in y0 as usize..(y0 + height) as usize {
        for x in x0 as usize..(x0 + width) as usize {
            if !close(pixel(pixels, x as f32, y as f32), background) {
                count += 1;
            }
        }
    }
    count
}

fn write_png(path: &str, pixels: &[u8]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("screenshot path"));
    let mut encoder = png::Encoder::new(file, VIEWPORT.0, VIEWPORT.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(pixels).expect("png data");
}

/// Drives `frames` frames without a window, paced at 60 Hz in real time so timers
/// become due: rAF callbacks, a microtask checkpoint, one non-blocking poll of the
/// deno_core event loop (timers, op completions), then Blitz layout.
fn drive_frames(
    runtime: &mut JsRuntime,
    document: &Rc<RefCell<HtmlDocument>>,
    run_animation_frame: &v8::Global<v8::Function>,
    frames: u32,
) {
    let frame_interval = std::time::Duration::from_micros(16_667);
    let start = std::time::Instant::now();
    for frame in 0..frames {
        let deadline = start + frame_interval * frame;
        if let Some(wait) = deadline.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(wait);
        }
        {
            deno_core::scope!(scope, runtime);
            let function = v8::Local::new(scope, run_animation_frame);
            let receiver = v8::undefined(scope).into();
            function
                .call(scope, receiver, &[])
                .expect("animation frame should not throw past reportException");
            scope.perform_microtask_checkpoint();
        }
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        if let std::task::Poll::Ready(Err(error)) =
            runtime.poll_event_loop(&mut context, Default::default())
        {
            panic!("event loop error in frame {frame}: {error}");
        }
        resolve_layout(document);
    }
}

/// `--exit-after-frames N` (default 8): frames driven by the headless loop.
fn frame_count() -> u32 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--exit-after-frames" {
            let value = args.next().expect("--exit-after-frames needs a value");
            return value
                .parse()
                .expect("--exit-after-frames must be a positive integer");
        }
    }
    8
}

fn main() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime should initialize");
    // Same switch production uses (roves-v8's `jitless` feature): it must be set
    // before the first JsRuntime initializes V8.
    let jitless = std::env::var("ROVES_PROBE_JITLESS").is_ok_and(|value| value == "1");
    if jitless {
        let rejected = deno_core::v8_set_flags(vec![String::new(), "--jitless".into()]);
        assert!(rejected.len() == 1, "V8 rejected flags: {rejected:?}");
    }
    tokio.block_on(async {
        let document = Rc::new(RefCell::new(HtmlDocument::from_html(
            FIXTURE,
            DocumentConfig::default(),
        )));
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![
                deno_webidl::deno_webidl::init(),
                deno_web::deno_web::init(
                    Arc::new(deno_web::BlobStore::default()),
                    None,
                    false,
                    deno_web::InMemoryBroadcastChannel::default(),
                ),
                roves_blitz_v8_probe::init(),
            ],
            ..Default::default()
        });
        runtime
            .op_state()
            .borrow_mut()
            .put(ProbeDocument(document.clone()));

        // The bootstrap evaluates to the embedder hooks. Keeping them as Rust-held
        // handles means no dispatch or frame entry point is left on the guest global.
        let hooks = runtime
            .execute_script("roves-blitz-dom.js", DOM_BOOTSTRAP)
            .expect("DOM wrapper bootstrap should install");
        let (dispatcher, run_animation_frame) = {
            deno_core::scope!(scope, runtime);
            let hooks = v8::Local::new(scope, hooks);
            let hooks = v8::Local::<v8::Object>::try_from(hooks).expect("bootstrap hooks object");
            let function = |name: &str| {
                let key = v8::String::new(scope, name).unwrap();
                let value = hooks.get(scope, key.into()).expect(name);
                let function = v8::Local::<v8::Function>::try_from(value).expect(name);
                v8::Global::new(scope, function)
            };
            (function("dispatchTrustedEvent"), function("runAnimationFrame"))
        };

        runtime
            .execute_script("roves-blitz-guest-test.js", GUEST_TEST)
            .expect("guest DOM checks should pass");

        {
            let document = document.borrow();
            let app = find_element_by_id(&document, "app").expect("fixture #app element");
            assert_eq!(
                document.get_node(app).unwrap().text_content(),
                "mutated from V8press"
            );
            let button = find_element_by_id(&document, "button").expect("JS-created #button");
            assert_eq!(document.get_node(button).unwrap().parent, Some(app));
        }

        // Click: Blitz layout + hit-test pick the target, the trusted event is
        // dispatched through deno_web's EventTarget along the Blitz parent chain.
        resolve_layout(&document);
        let button_rect = element_rect(&document, "button");
        let (x, y) = (
            button_rect.0 + button_rect.2 / 2.0,
            button_rect.1 + button_rect.3 / 2.0,
        );
        let target = hit_target(&document, x, y).expect("hit-test should find a node");
        {
            deno_core::scope!(scope, runtime);
            let function = v8::Local::new(scope, &dispatcher);
            let receiver = v8::undefined(scope).into();
            let node = v8::Integer::new(scope, target as i32).into();
            let kind = v8::String::new(scope, "click").unwrap().into();
            function
                .call(scope, receiver, &[node, kind])
                .expect("click dispatch should not throw");
        }
        runtime
            .execute_script(
                "roves-blitz-click-check.js",
                "if (document.getElementById('button').textContent !== 'clicked: capture,target,bubble') \
                 throw new Error('click listeners did not run in DOM order: ' + document.getElementById('button').textContent);",
            )
            .expect("click must run capture, target and bubble listeners");

        // Frame loop without a window: each frame runs the rAF callbacks, drains
        // microtasks, polls the deno_core event loop once (timers) and resolves
        // layout. Frames are paced in real time so timers can become due.
        runtime
            .execute_script("roves-blitz-frame-test.js", FRAME_TEST)
            .expect("frame test setup should run");
        let frames = frame_count();
        drive_frames(&mut runtime, &document, &run_animation_frame, frames);
        runtime
            .execute_script("roves-blitz-frame-check.js", FRAME_CHECK)
            .expect("frame loop checks should pass");
        let box_rect = element_rect(&document, "box");
        assert_eq!(box_rect.2, 60.0, "rAF style writes must reach Blitz layout");

        // Motion: its JS frameloop (no WAAPI here) runs on rAF + performance.now and
        // writes inline transform/opacity. 0.1 s needs ~6 frames; 20 leave margin.
        runtime
            .execute_script("roves-blitz-motion-scenario.js", MOTION_SCENARIO)
            .expect("Motion scenario should load");
        drive_frames(&mut runtime, &document, &run_animation_frame, 20);
        runtime
            .execute_script(
                "roves-blitz-motion-check.js",
                "if (motionResult.error !== null || !motionResult.started || !motionResult.finished) \
                 throw new Error('Motion animation did not finish: ' + JSON.stringify(motionResult));",
            )
            .expect("Motion animation should finish");
        let motion_rect = element_rect(&document, "motion");

        resolve_layout(&document);
        let root_layout = document.borrow().root_element().final_layout;
        assert!(root_layout.size.width > 0.0 && root_layout.size.height > 0.0);

        // Offscreen paint of the mutated document (Blitz painter + Vello on wgpu).
        let pixels = render_rgba(&document);
        if let Ok(path) = std::env::var("ROVES_PROBE_SCREENSHOT") {
            write_png(&path, &pixels);
        }
        let app_rect = element_rect(&document, "app");
        let button_rect = element_rect(&document, "button");
        // Bottom-right corners stay clear of the text, which starts top-left.
        let app_pixel = pixel(&pixels, app_rect.0 + app_rect.2 - 4.0, app_rect.1 + app_rect.3 - 4.0);
        let button_pixel = pixel(
            &pixels,
            button_rect.0 + button_rect.2 - 4.0,
            button_rect.1 + button_rect.3 - 4.0,
        );
        assert_color(app_pixel, [24, 32, 48], "#app background");
        assert_color(button_pixel, [200, 80, 40], "JS-created #button background");
        let box_pixel = pixel(
            &pixels,
            box_rect.0 + box_rect.2 - 2.0,
            box_rect.1 + box_rect.3 / 2.0,
        );
        assert_color(box_pixel, [40, 160, 90], "rAF-animated #box background");
        // Layout ignores transforms; paint must show the box translated by 100px
        // at full opacity, with nothing left at its untransformed position.
        let motion_centre = (
            motion_rect.0 + motion_rect.2 / 2.0,
            motion_rect.1 + motion_rect.3 / 2.0,
        );
        let moved_pixel = pixel(&pixels, motion_centre.0 + 100.0, motion_centre.1);
        let origin_pixel = pixel(&pixels, motion_centre.0, motion_centre.1);
        assert_color(moved_pixel, [90, 120, 220], "Motion-translated #motion background");
        assert_color(origin_pixel, [24, 32, 48], "#motion must have left its layout position");
        // Text must be painted inside the boxes; insets skip the UA button border.
        let inset = |rect: (f32, f32, f32, f32), by: f32| {
            (rect.0 + by, rect.1 + by, rect.2 - 2.0 * by, rect.3 - 2.0 * by)
        };
        let text_pixels = count_differing(&pixels, inset(button_rect, 4.0), [200, 80, 40]);
        assert!(text_pixels > 0, "the button text must be painted over its background");
        let app_text_band = (
            app_rect.0 + 12.0,
            app_rect.1 + 12.0,
            app_rect.2 - 24.0,
            button_rect.1 - app_rect.1 - 12.0,
        );
        assert!(app_text_band.3 > 0.0, "#app text must occupy a line above the button");
        let app_text_pixels = count_differing(&pixels, app_text_band, [24, 32, 48]);
        assert!(app_text_pixels > 0, "the #app text must be painted");

        println!("ROVES_PROBE_RESULT {{\"check\":\"v8_mode\",\"jitless\":{jitless}}}");
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"js_get_element_by_id_stable_wrapper_identity\",\"status\":\"passed\"}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"js_tree_mutation_text_attributes_blitz_document\",\"status\":\"passed\"}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"blitz_hit_test_dispatches_deno_web_event_capture_target_bubble\",\"status\":\"passed\",\"target\":{target},\"x\":{x},\"y\":{y}}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"blitz_stylesheet_layout_pass\",\"status\":\"passed\",\"width\":{},\"height\":{}}}",
            root_layout.size.width, root_layout.size.height
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"offscreen_paint_reflects_js_mutations\",\"status\":\"passed\",\"app\":{app_pixel:?},\"button\":{button_pixel:?},\"button_text_pixels\":{text_pixels},\"app_text_pixels\":{app_text_pixels}}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"headless_frames_raf_microtasks_timers_style\",\"status\":\"passed\",\"frames\":{frames},\"box_width\":{}}}",
            box_rect.2
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"motion_12_38_0_animate_transform_opacity\",\"status\":\"passed\",\"moved_pixel\":{moved_pixel:?}}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"window_presentation_real_input_wrapper_gc\",\"status\":\"not_implemented\"}}"
        );
    });
}
