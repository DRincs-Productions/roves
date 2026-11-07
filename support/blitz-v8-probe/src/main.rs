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
    ]
);

fn resolve_layout(document: &Rc<RefCell<HtmlDocument>>) {
    let mut document = document.borrow_mut();
    document.set_viewport(Viewport::new(640, 360, 1.0, ColorScheme::Light));
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

fn main() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime should initialize");
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

        // The bootstrap evaluates to the trusted-event dispatcher. Keeping it as a
        // Rust-held handle means no dispatch entry point is left on the guest global.
        let dispatcher = runtime
            .execute_script("roves-blitz-dom.js", DOM_BOOTSTRAP)
            .expect("DOM wrapper bootstrap should install");
        let dispatcher: v8::Global<v8::Function> = {
            deno_core::scope!(scope, runtime);
            let local = v8::Local::new(scope, dispatcher);
            let function = v8::Local::<v8::Function>::try_from(local)
                .expect("bootstrap must evaluate to the dispatcher function");
            v8::Global::new(scope, function)
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
        let (x, y) = {
            let document = document.borrow();
            let button = find_element_by_id(&document, "button").unwrap();
            let layout = document.get_node(button).unwrap().final_layout;
            let app = document.get_node(find_element_by_id(&document, "app").unwrap()).unwrap();
            // final_layout locations are relative to the parent box.
            let mut origin = (layout.location.x, layout.location.y);
            let mut ancestor = Some(app.id);
            while let Some(id) = ancestor {
                let node = document.get_node(id).unwrap();
                origin.0 += node.final_layout.location.x;
                origin.1 += node.final_layout.location.y;
                ancestor = node.parent;
            }
            (
                origin.0 + layout.size.width / 2.0,
                origin.1 + layout.size.height / 2.0,
            )
        };
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

        resolve_layout(&document);
        let root_layout = document.borrow().root_element().final_layout;
        assert!(root_layout.size.width > 0.0 && root_layout.size.height > 0.0);

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
            "ROVES_PROBE_RESULT {{\"check\":\"window_paint_frames_raf_wrapper_gc\",\"status\":\"not_implemented\"}}"
        );
    });
}
