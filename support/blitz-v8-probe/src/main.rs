use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::{BaseDocument, DocumentConfig, Node};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use deno_core::{JsRuntime, OpState, RuntimeOptions, op2};
use deno_error::JsErrorBox;

const FIXTURE: &str = include_str!("../fixtures/layout.html");
const DOM_BOOTSTRAP: &str = include_str!("dom.js");
const GUEST_TEST: &str = include_str!("guest_test.js");

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

#[op2(fast)]
fn op_blitz_get_element_by_id(state: Rc<RefCell<OpState>>, #[string] id: &str) -> i32 {
    if id.is_empty() {
        return -1;
    }
    let document = probe_document(&state);
    let document = document.borrow();
    find_element_by_id(&document, id).map_or(-1, |node_id| node_id as i32)
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

deno_core::extension!(
    roves_blitz_v8_probe,
    ops = [
        op_blitz_get_element_by_id,
        op_blitz_text_content,
        op_blitz_set_text_content
    ]
);

fn main() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime should initialize");
    tokio.block_on(async {
        let document = Rc::new(RefCell::new(HtmlDocument::from_html(FIXTURE, DocumentConfig::default())));
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![roves_blitz_v8_probe::init()],
            ..Default::default()
        });
        runtime.op_state().borrow_mut().put(ProbeDocument(document.clone()));
        runtime
            .execute_script("roves-blitz-dom.js", DOM_BOOTSTRAP)
            .expect("DOM wrapper bootstrap should install");
        runtime
            .execute_script("roves-blitz-guest-test.js", GUEST_TEST)
            .expect("guest DOM checks should pass");

        {
            let document = document.borrow();
            let app = find_element_by_id(&document, "app").expect("fixture #app element");
            assert_eq!(document.get_node(app).unwrap().text_content(), "mutated from V8");
        }

        let mut document = document.borrow_mut();
        document.set_viewport(Viewport::new(640, 360, 1.0, ColorScheme::Light));
        document.resolve(0.0);
        let root_layout = document.root_element().final_layout;
        assert!(root_layout.size.width > 0.0 && root_layout.size.height > 0.0);

        println!("ROVES_PROBE_RESULT {{\"check\":\"js_get_element_by_id_stable_wrapper_identity\",\"status\":\"passed\"}}");
        println!("ROVES_PROBE_RESULT {{\"check\":\"js_text_content_reads_writes_blitz_document\",\"status\":\"passed\"}}");
        println!("ROVES_PROBE_RESULT {{\"check\":\"blitz_stylesheet_layout_pass\",\"status\":\"passed\",\"width\":{},\"height\":{}}}", root_layout.size.width, root_layout.size.height);
        println!("ROVES_PROBE_RESULT {{\"check\":\"js_events_frames_rendering_wrapper_gc\",\"status\":\"not_implemented\"}}");
    });
}
