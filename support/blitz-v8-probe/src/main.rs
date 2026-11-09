use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blitz_dom::{
    BaseDocument, DocumentConfig, LocalName, Namespace, Node, NodeData, NodeId, Prefix, QualName,
    ns,
};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use deno_core::{JsRuntime, OpState, RuntimeOptions, op2, v8};
use deno_error::JsErrorBox;
use style::properties::{PropertyDeclarationId, PropertyId};

mod audio;
mod game;
mod net_policy;
mod window;

use net_policy::op_blitz_fetch_check;
mod worker;

const FIXTURE: &str = include_str!("../fixtures/layout.html");
const HISTORY: &str = include_str!("history.js");
const FETCH: &str = include_str!("fetch.js");
const WEB_AUDIO: &str = include_str!("web_audio.js");
const INDEXED_DB: &str = include_str!("indexeddb.js");
const INDEXED_DB_TEST: &str = include_str!("indexeddb_test.js");
const WORKER: &str = include_str!("worker.js");
const WORKER_TEST: &str = include_str!("worker_test.js");
const DOM_BOOTSTRAP: &str = include_str!("dom.js");
const FETCH_TEST: &str = include_str!("fetch_test.js");
const FETCH_CHECK: &str = include_str!("fetch_check.js");
const STORAGE_TEST: &str = include_str!("storage_test.js");
const HISTORY_TEST: &str = include_str!("history_test.js");
const HISTORY_CHECK: &str = include_str!("history_check.js");
const GUEST_TEST: &str = include_str!("guest_test.js");
const FRAME_TEST: &str = include_str!("frame_test.js");
const FRAME_CHECK: &str = include_str!("frame_check.js");
const RESIZE_OBSERVER_TEST: &str = include_str!("resize_observer_test.js");
const MOTION_CHECK: &str = include_str!("motion_check.js");
const GC_TEST: &str = include_str!("gc_test.js");
const DOM_API_TEST: &str = include_str!("dom_api_test.js");
const EVENTS_TEST: &str = include_str!("events_test.js");
const EVENTS_CHECK: &str = include_str!("events_check.js");
const WINDOW_TEST: &str = include_str!("window_test.js");
const SCROLL_TEST: &str = include_str!("scroll_test.js");
const GC_CHECK: &str = include_str!("gc_check.js");
/// Built by `npm run build` in `js/` (Motion 12.38.0 bundled with esbuild).
const MOTION_SCENARIO: &str = include_str!("../js/dist/motion-scenario.js");
/// PixiJS 8.19.0 on WebGPU (bundled with esbuild in `js/`).
const WEBGPU_SCENARIO: &str = include_str!("../js/dist/webgpu-scenario.js");
// Tone.js 15.1.22 + standardized-audio-context bundled by js/package.json.
const TONE_SCENARIO: &str = include_str!("../js/dist/tone-scenario.js");

const ELEMENT_NODE: i32 = 1;
const TEXT_NODE: i32 = 3;
const COMMENT_NODE: i32 = 8;
const DOCUMENT_NODE: i32 = 9;

/// Status codes returned by tree-mutating ops; `dom.js` turns them into the
/// matching `DOMException` (from deno_web) so no custom error classes are needed.
const OK: i32 = 0;
const HIERARCHY_REQUEST_ERROR: i32 = 1;
const NOT_FOUND_ERROR: i32 = 2;

/// "No node" handle returned to JS.
const NO_NODE: f64 = -1.0;

struct ProbeDocument(Rc<RefCell<HtmlDocument>>);

/// Number of live JS wrappers per Blitz node (a node can briefly have two: a
/// collected wrapper whose finalizer has not run yet, and its replacement).
#[derive(Default)]
struct WrappedNodes(HashMap<NodeId, u32>);

fn is_connected(document: &BaseDocument, node_id: NodeId) -> bool {
    let root = document.root_node().id;
    let mut current = Some(node_id);
    while let Some(id) = current {
        if id == root {
            return true;
        }
        current = document.get_node(id).and_then(|node| node.parent);
    }
    false
}

/// Frees the detached subtree containing `node_id` when no node in it has a live
/// JS wrapper. Connected nodes are never freed here: they live with the document.
/// Returns whether a subtree was dropped.
fn drop_if_unreferenced(
    document: &mut HtmlDocument,
    wrapped: &WrappedNodes,
    node_id: NodeId,
) -> bool {
    let root = document.root_node().id;
    let mut top = node_id;
    while let Some(parent) = document.get_node(top).and_then(|node| node.parent) {
        top = parent;
    }
    if top == root || document.get_node(top).is_none() {
        return false;
    }
    let mut stack = vec![top];
    while let Some(id) = stack.pop() {
        if wrapped.0.get(&id).is_some_and(|count| *count > 0) {
            return false;
        }
        if let Some(node) = document.get_node(id) {
            stack.extend(node.children.iter().copied());
        }
    }
    document.mutate().remove_and_drop_node(top);
    true
}

/// Clones the shared document handle out of `OpState`; the `RefCell` borrow is
/// taken by each op only for the duration of that synchronous call, never across
/// guest execution.
fn probe_document(state: &Rc<RefCell<OpState>>) -> Rc<RefCell<HtmlDocument>> {
    state.borrow().borrow::<ProbeDocument>().0.clone()
}

/// Blitz 0.3 node ids are versioned (slot index + generation), so a handle kept
/// by a JS wrapper never resolves to a different node after its slot is reused:
/// `get_node` returns `None` for a stale handle. Handles cross into JS as numbers
/// (exact while the generation stays below 2^21).
fn to_handle(id: NodeId) -> f64 {
    id.as_u64() as f64
}

fn from_handle(handle: f64) -> NodeId {
    NodeId::from_u64(handle as u64)
}

fn attached_node(document: &BaseDocument, handle: f64) -> Result<&Node, JsErrorBox> {
    if !(handle >= 0.0) {
        return Err(JsErrorBox::type_error("invalid Blitz node handle"));
    }
    document
        .get_node(from_handle(handle))
        .ok_or_else(|| JsErrorBox::type_error("unknown Blitz node handle"))
}

/// DOM `getElementById`: first element in tree order whose `id` equals `id`.
/// Walks the public node tree, so no selector parsing or escaping is involved.
fn find_element_by_id(document: &BaseDocument, id: &str) -> Option<NodeId> {
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

/// Attribute names have no namespace in the DOM (only element names are in the
/// HTML namespace); using `ns!(html)` here would create a second attribute next
/// to the parser's one instead of replacing it.
fn attribute_name(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

#[op2(fast)]
fn op_blitz_document_root(state: Rc<RefCell<OpState>>) -> f64 {
    let document = probe_document(&state);
    let document = document.borrow();
    to_handle(document.root_node().id)
}

#[op2(fast)]
fn op_blitz_get_element_by_id(state: Rc<RefCell<OpState>>, #[string] id: &str) -> f64 {
    if id.is_empty() {
        return NO_NODE;
    }
    let document = probe_document(&state);
    let document = document.borrow();
    find_element_by_id(&document, id).map_or(NO_NODE, to_handle)
}

#[op2(fast)]
fn op_blitz_node_type(state: Rc<RefCell<OpState>>, handle: f64) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(match attached_node(&document, handle)?.data {
        NodeData::Document(_) => DOCUMENT_NODE,
        NodeData::Element(_) | NodeData::AnonymousBlock(_) => ELEMENT_NODE,
        NodeData::Text(_) => TEXT_NODE,
        NodeData::Comment { .. } => COMMENT_NODE,
    })
}

#[op2(fast)]
fn op_blitz_parent(state: Rc<RefCell<OpState>>, handle: f64) -> Result<f64, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, handle)?
        .parent
        .map_or(NO_NODE, to_handle))
}

#[op2]
#[string]
fn op_blitz_local_name(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    Ok(node
        .element_data()
        .map(|element| element.name.local.to_string())
        .unwrap_or_default())
}

#[op2]
#[string]
fn op_blitz_text_content(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, handle)?.text_content())
}

/// DOM `textContent` setter. Text nodes are updated in place. Element children are
/// detached with `remove_node`, not dropped: a detached node stays reachable from
/// its JS wrapper (like a removed DOM node). Freeing nodes needs a GC-aware
/// wrapper lifecycle; versioned ids make a stale handle fail instead of aliasing.
#[op2(fast)]
fn op_blitz_set_text_content(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
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
    let children: Vec<NodeId> = node.children.iter().copied().collect();
    {
        let mut mutator = document.mutate();
        for child in children.iter().copied() {
            mutator.remove_node(child);
        }
        if !value.is_empty() {
            let text = mutator.create_text_node(value);
            mutator.append_children(node_id, &[text]);
        }
    }
    // Replaced children nobody references from JS are freed right away.
    let state = state.borrow();
    let wrapped = state.borrow::<WrappedNodes>();
    for child in children {
        drop_if_unreferenced(&mut document, wrapped, child);
    }
    Ok(())
}

/// `document.createElement` for the HTML namespace; `dom.js` lowercases and
/// validates the name before calling.
#[op2(fast)]
fn op_blitz_create_element(state: Rc<RefCell<OpState>>, #[string] local_name: &str) -> f64 {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let id = document
        .mutate()
        .create_element(html_name(local_name), Vec::new());
    to_handle(id)
}

/// `document.createElementNS`: `dom.js` validates the qualified name and the
/// namespace/prefix constraints, then passes the parts verbatim (no case
/// folding); "" means a null namespace or prefix. Blitz's own namespace-qualified
/// `create_element` builds the node, so SVG and foreign elements share the tree.
#[op2(fast)]
fn op_blitz_create_element_ns(
    state: Rc<RefCell<OpState>>,
    #[string] namespace: &str,
    #[string] prefix: &str,
    #[string] local_name: &str,
) -> f64 {
    let prefix = (!prefix.is_empty()).then(|| Prefix::from(prefix));
    let name = QualName::new(
        prefix,
        Namespace::from(namespace),
        LocalName::from(local_name),
    );
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let id = document.mutate().create_element(name, Vec::new());
    to_handle(id)
}

/// Element prefix ("" when none), for `prefix` and the qualified `tagName`.
#[op2]
#[string]
fn op_blitz_prefix(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    Ok(node
        .element_data()
        .and_then(|element| element.name.prefix.as_ref())
        .map(|prefix| prefix.to_string())
        .unwrap_or_default())
}

#[op2(fast)]
fn op_blitz_create_text_node(state: Rc<RefCell<OpState>>, #[string] data: &str) -> f64 {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let id = document.mutate().create_text_node(data);
    to_handle(id)
}

/// `appendChild`; see `insert_child`.
#[op2(fast)]
fn op_blitz_append_child(
    state: Rc<RefCell<OpState>>,
    parent_handle: f64,
    child_handle: f64,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    insert_child(&mut document, parent_handle, child_handle, None)
}

/// `removeChild`: detaches without dropping (see `op_blitz_set_text_content`).
#[op2(fast)]
fn op_blitz_remove_child(
    state: Rc<RefCell<OpState>>,
    parent_handle: f64,
    child_handle: f64,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let parent_id = attached_node(&document, parent_handle)?.id;
    let child = attached_node(&document, child_handle)?;
    let child_id = child.id;
    if child.parent != Some(parent_id) {
        return Ok(NOT_FOUND_ERROR);
    }
    document.mutate().remove_node(child_id);
    let state = state.borrow();
    drop_if_unreferenced(&mut document, state.borrow::<WrappedNodes>(), child_id);
    Ok(OK)
}

#[op2]
#[string]
fn op_blitz_get_attribute(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] name: &str,
) -> Result<Option<String>, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
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
    handle: f64,
    #[string] name: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error("setAttribute on a non-element"));
    }
    let node_id = node.id;
    document
        .mutate()
        .set_attribute(node_id, attribute_name(name), value);
    Ok(())
}

#[op2]
#[string]
fn op_blitz_namespace(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    Ok(node
        .element_data()
        .map(|element| element.name.ns.to_string())
        .unwrap_or_default())
}

/// `getComputedStyle(element).getPropertyValue(name)`: flushes styles only
/// (`resolve_stylist`, no layout), then serializes Stylo's computed value.
/// Longhands and custom properties are supported; shorthands, unknown names and
/// elements without styles (detached, or inside `display: none`) return "".
/// Values are computed, not CSSOM "resolved" ones (e.g. `width: auto` stays
/// `auto` instead of the used pixel width).
#[op2]
#[string]
fn op_blitz_computed_style(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] name: &str,
) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error("getComputedStyle on a non-element"));
    }
    let node_id = node.id;
    document.resolve_stylist(0.0);
    let node = document.get_node(node_id).unwrap();
    let Some(styles) = node.primary_styles() else {
        return Ok(String::new());
    };
    let Ok(property) = PropertyId::parse_enabled_for_all_content(name) else {
        return Ok(String::new());
    };
    let declaration = match &property {
        PropertyId::Custom(custom) => PropertyDeclarationId::Custom(custom),
        PropertyId::NonCustom(_) => match property.longhand_id() {
            Some(longhand) => PropertyDeclarationId::Longhand(longhand),
            None => return Ok(String::new()),
        },
    };
    Ok(styles.computed_value_to_string(declaration))
}

/// Inline style write (`element.style.x = v`); an empty value removes the
/// declaration, like `style.removeProperty`. Blitz 0.3 marks the element for
/// restyle itself (0.2.1 did not).
#[op2(fast)]
fn op_blitz_set_style_property(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] name: &str,
    #[string] value: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error("style on a non-element"));
    }
    let node_id = node.id;
    let mut mutator = document.mutate();
    if value.is_empty() {
        mutator.remove_style_property(node_id, name);
    } else {
        mutator.set_style_property(node_id, name, value);
    }
    Ok(())
}

/// A JS wrapper was created for this node.
#[op2(fast)]
fn op_blitz_retain(state: Rc<RefCell<OpState>>, handle: f64) {
    let id = from_handle(handle);
    *state
        .borrow_mut()
        .borrow_mut::<WrappedNodes>()
        .0
        .entry(id)
        .or_default() += 1;
}

/// A JS wrapper of this node was garbage-collected (FinalizationRegistry).
#[op2(fast)]
fn op_blitz_release(state: Rc<RefCell<OpState>>, handle: f64) {
    let id = from_handle(handle);
    let document = probe_document(&state);
    let mut state = state.borrow_mut();
    let wrapped = state.borrow_mut::<WrappedNodes>();
    if let Some(count) = wrapped.0.get_mut(&id) {
        *count -= 1;
        if *count == 0 {
            wrapped.0.remove(&id);
        }
    }
    let mut document = document.borrow_mut();
    if document.get_node(id).is_some() {
        drop_if_unreferenced(&mut document, wrapped, id);
    }
}

#[op2(fast)]
fn op_blitz_is_connected(state: Rc<RefCell<OpState>>, handle: f64) -> bool {
    let document = probe_document(&state);
    let document = document.borrow();
    handle >= 0.0 && is_connected(&document, from_handle(handle))
}

/// Result list of the last `op_blitz_query`; read back with `op_blitz_query_result`
/// so no array has to cross the op boundary.
#[derive(Default)]
struct QueryResults(Vec<NodeId>);

/// Shared by `appendChild` (`reference == None`) and `insertBefore`. Rejects a
/// document child, a text/comment parent and inserting a node into itself or its
/// descendants; a reference that is not a child of `parent` is NotFoundError.
/// The child is detached from its old parent first (Blitz's insert does not).
fn insert_child(
    document: &mut HtmlDocument,
    parent_handle: f64,
    child_handle: f64,
    reference_handle: Option<f64>,
) -> Result<i32, JsErrorBox> {
    let child = attached_node(document, child_handle)?;
    let child_id = child.id;
    if matches!(child.data, NodeData::Document(_)) {
        return Ok(HIERARCHY_REQUEST_ERROR);
    }
    let had_parent = child.parent.is_some();
    let parent = attached_node(document, parent_handle)?;
    let parent_id = parent.id;
    if matches!(parent.data, NodeData::Text(_) | NodeData::Comment { .. }) {
        return Ok(HIERARCHY_REQUEST_ERROR);
    }
    let mut reference = match reference_handle {
        None => None,
        Some(handle) => {
            let reference = attached_node(document, handle)?;
            if reference.parent != Some(parent_id) {
                return Ok(NOT_FOUND_ERROR);
            }
            Some(reference.id)
        },
    };
    let mut ancestor = Some(parent_id);
    while let Some(id) = ancestor {
        if id == child_id {
            return Ok(HIERARCHY_REQUEST_ERROR);
        }
        ancestor = document.get_node(id).and_then(|node| node.parent);
    }
    // DOM: inserting a node before itself means before its next sibling.
    if reference == Some(child_id) {
        let siblings = &document.get_node(parent_id).unwrap().children;
        let index = siblings.iter().position(|id| *id == child_id).unwrap();
        reference = siblings.get(index + 1).copied();
    }
    let mut mutator = document.mutate();
    if had_parent {
        mutator.remove_node(child_id);
    }
    match reference {
        Some(reference) => mutator.insert_nodes_before(reference, &[child_id]),
        None => mutator.append_children(parent_id, &[child_id]),
    }
    Ok(OK)
}

#[op2(fast)]
fn op_blitz_insert_before(
    state: Rc<RefCell<OpState>>,
    parent_handle: f64,
    child_handle: f64,
    reference_handle: f64,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let reference = (reference_handle >= 0.0).then_some(reference_handle);
    insert_child(&mut document, parent_handle, child_handle, reference)
}

#[op2(fast)]
fn op_blitz_child_count(state: Rc<RefCell<OpState>>, handle: f64) -> Result<u32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, handle)?.children.len() as u32)
}

#[op2(fast)]
fn op_blitz_child_at(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    index: u32,
) -> Result<f64, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    Ok(node
        .children
        .get(index as usize)
        .copied()
        .map_or(NO_NODE, to_handle))
}

/// `nextSibling` (`next == true`) or `previousSibling`.
#[op2(fast)]
fn op_blitz_sibling(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    next: bool,
) -> Result<f64, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    let Some(parent) = node.parent else {
        return Ok(NO_NODE);
    };
    let siblings = &document.get_node(parent).unwrap().children;
    let index = siblings.iter().position(|id| *id == node.id).unwrap();
    let sibling = if next {
        siblings.get(index + 1)
    } else {
        index.checked_sub(1).and_then(|index| siblings.get(index))
    };
    Ok(sibling.copied().map_or(NO_NODE, to_handle))
}

/// `querySelector` (`all == false`) / `querySelectorAll` on the descendants of
/// `scope` with Blitz's Stylo-based matching. Returns the number of matches, or
/// -1 for an invalid selector (SyntaxError in JS).
#[op2(fast)]
fn op_blitz_query(
    state: Rc<RefCell<OpState>>,
    scope_handle: f64,
    #[string] selector: &str,
    all: bool,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let scope = attached_node(&document, scope_handle)?.id;
    let found: Vec<NodeId> = if all {
        match document.query_selector_all_in(scope, selector) {
            Ok(found) => found.into_iter().collect(),
            Err(_) => return Ok(-1),
        }
    } else {
        match document.query_selector_in(scope, selector) {
            Ok(found) => found.into_iter().collect(),
            Err(_) => return Ok(-1),
        }
    };
    let count = found.len() as i32;
    state.borrow_mut().borrow_mut::<QueryResults>().0 = found;
    Ok(count)
}

#[op2(fast)]
fn op_blitz_query_result(state: Rc<RefCell<OpState>>, index: u32) -> f64 {
    state
        .borrow()
        .borrow::<QueryResults>()
        .0
        .get(index as usize)
        .copied()
        .map_or(NO_NODE, to_handle)
}

#[op2]
#[string]
fn op_blitz_outer_html(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    Ok(attached_node(&document, handle)?.outer_html())
}

#[op2]
#[string]
fn op_blitz_inner_html(state: Rc<RefCell<OpState>>, handle: f64) -> Result<String, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node = attached_node(&document, handle)?;
    let mut html = String::new();
    for child in node.children.iter() {
        if let Some(child) = document.get_node(*child) {
            child.write_outer_html(&mut html);
        }
    }
    Ok(html)
}

/// `innerHTML` setter: detaches the old children (wrapped ones survive detached,
/// unreferenced ones are freed) and parses the fragment with Blitz's html5ever
/// provider.
#[op2(fast)]
fn op_blitz_set_inner_html(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] html: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error("innerHTML on a non-element"));
    }
    let node_id = node.id;
    let children: Vec<NodeId> = node.children.iter().copied().collect();
    {
        let mut mutator = document.mutate();
        for child in children.iter().copied() {
            mutator.remove_node(child);
        }
        mutator.set_inner_html(node_id, html);
    }
    let state = state.borrow();
    let wrapped = state.borrow::<WrappedNodes>();
    for child in children {
        drop_if_unreferenced(&mut document, wrapped, child);
    }
    Ok(())
}

#[op2(fast)]
fn op_blitz_remove_attribute(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] name: &str,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node = attached_node(&document, handle)?;
    if node.element_data().is_none() {
        return Err(JsErrorBox::type_error("removeAttribute on a non-element"));
    }
    let node_id = node.id;
    document
        .mutate()
        .clear_attribute(node_id, attribute_name(name));
    Ok(())
}

/// Last values measured by `op_blitz_measure` (x, y, width, height) or
/// `op_blitz_box_sizes` (content x, y, width, height, border-box width, height).
#[derive(Default)]
struct LastRect([f64; 6]);

/// Forces style + layout (as browsers do for geometry reads), then stores the
/// client bounding rect (`offset == false`) or the offset rect of the node.
/// Returns false when the node has no box.
#[op2(fast)]
fn op_blitz_measure(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    offset: bool,
) -> Result<bool, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    document.resolve(0.0);
    let rect = if offset {
        document
            .offset_rect(node_id)
            .map(|rect| [rect.x, rect.y, rect.width, rect.height])
    } else {
        client_rect(&mut document, node_id)
    };
    let Some([x, y, width, height]) = rect else {
        state.borrow_mut().borrow_mut::<LastRect>().0 = [0.0; 6];
        return Ok(false);
    };
    state.borrow_mut().borrow_mut::<LastRect>().0 = [x, y, width, height, 0.0, 0.0];
    Ok(true)
}

/// `getBoundingClientRect` in viewport coordinates. Blitz's rect is read with the
/// viewport scroll and the node's own scroll offset temporarily set to zero (an
/// element's border box does not move when its own content scrolls; ancestors'
/// offsets, which do move it, stay applied), then the viewport scroll is
/// subtracted. This makes the result independent of whether Blitz's rect already
/// accounts for either offset.
fn client_rect(document: &mut BaseDocument, node_id: NodeId) -> Option<[f64; 4]> {
    let viewport = document.viewport_scroll();
    let own = document.get_node(node_id).map(|node| *node.scroll_offset());
    let mut unscrolled = viewport;
    unscrolled.x = Default::default();
    unscrolled.y = Default::default();
    document.set_viewport_scroll(unscrolled);
    if let Some(node) = document.get_node_mut(node_id) {
        let offset = node.scroll_offset_mut();
        offset.x = Default::default();
        offset.y = Default::default();
    }
    let rect = document
        .get_client_bounding_rect(node_id)
        .map(|rect| [rect.x, rect.y, rect.width, rect.height]);
    document.set_viewport_scroll(viewport);
    if let (Some(own), Some(node)) = (own, document.get_node_mut(node_id)) {
        *node.scroll_offset_mut() = own;
    }
    let (scroll_x, scroll_y) = (viewport.x as f64, viewport.y as f64);
    rect.map(|[x, y, width, height]| [x - scroll_x, y - scroll_y, width, height])
}

/// Viewport size in CSS pixels.
fn viewport_css_size(document: &BaseDocument) -> (f64, f64) {
    let viewport = document.viewport();
    let scale = f64::from(viewport.hidpi_scale.max(f32::MIN_POSITIVE));
    (
        f64::from(viewport.window_size.0) / scale,
        f64::from(viewport.window_size.1) / scale,
    )
}

/// Whether computed `overflow-x`/`overflow-y` (`longhand`) makes the element a
/// scroll container on that axis (anything but `visible` and `clip`; `hidden`
/// still scrolls programmatically, as in browsers).
fn overflow_scrolls(node: &Node, longhand: &str) -> bool {
    let Some(styles) = node.primary_styles() else {
        return false;
    };
    let property = PropertyId::parse_enabled_for_all_content(longhand);
    let Some(longhand) = property.ok().and_then(|property| property.longhand_id()) else {
        return false;
    };
    let value = styles.computed_value_to_string(PropertyDeclarationId::Longhand(longhand));
    !matches!(value.as_str(), "visible" | "clip")
}

/// Scroll state of one element (the root element stands for the viewport).
struct ScrollBox {
    offset: (f64, f64),
    /// Largest scroll offset per axis: Taffy's scroll extent (the bound Blitz uses
    /// for wheel scrolling), or document extent minus viewport for the root.
    max: (f64, f64),
    client: (f64, f64),
    border: (f64, f64),
    scrolls: (bool, bool),
}

fn scroll_box(document: &BaseDocument, node_id: NodeId) -> Option<ScrollBox> {
    let node = document.get_node(node_id)?;
    let layout = node.final_layout();
    if node_id == document.root_element().id {
        let (width, height) = viewport_css_size(document);
        let extent_x = layout.location.x + layout.size.width + layout.scroll_width();
        let extent_y = layout.location.y + layout.size.height + layout.scroll_height();
        let scroll = document.viewport_scroll();
        return Some(ScrollBox {
            offset: (scroll.x as f64, scroll.y as f64),
            max: (
                (f64::from(extent_x) - width).max(0.0),
                (f64::from(extent_y) - height).max(0.0),
            ),
            client: (width, height),
            border: (0.0, 0.0),
            scrolls: (true, true),
        });
    }
    let (size, border) = (layout.size, layout.border);
    Some(ScrollBox {
        offset: (node.scroll_offset().x, node.scroll_offset().y),
        max: (
            f64::from(layout.scroll_width().max(0.0)),
            f64::from(layout.scroll_height().max(0.0)),
        ),
        client: (
            f64::from(
                (size.width - border.left - border.right - layout.scrollbar_size.width).max(0.0),
            ),
            f64::from(
                (size.height - border.top - border.bottom - layout.scrollbar_size.height).max(0.0),
            ),
        ),
        border: (f64::from(border.left), f64::from(border.top)),
        scrolls: (
            overflow_scrolls(node, "overflow-x"),
            overflow_scrolls(node, "overflow-y"),
        ),
    })
}

/// Last values read by `op_blitz_scroll_metrics`: scrollLeft, scrollTop,
/// scrollWidth, scrollHeight, clientWidth, clientHeight, clientLeft, clientTop.
#[derive(Default)]
struct ScrollMetrics([f64; 8]);

/// Forces style + layout, stores the element's scroll metrics (read back with
/// `op_blitz_scroll_component`) and returns which axes scroll (bit 0 = x,
/// bit 1 = y). The root element reports the viewport (`scrollingElement`).
#[op2(fast)]
fn op_blitz_scroll_metrics(state: Rc<RefCell<OpState>>, handle: f64) -> Result<u32, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    document.resolve(0.0);
    let Some(scroll) = scroll_box(&document, node_id) else {
        state.borrow_mut().borrow_mut::<ScrollMetrics>().0 = [0.0; 8];
        return Ok(0);
    };
    state.borrow_mut().borrow_mut::<ScrollMetrics>().0 = [
        scroll.offset.0,
        scroll.offset.1,
        scroll.client.0 + scroll.max.0,
        scroll.client.1 + scroll.max.1,
        scroll.client.0,
        scroll.client.1,
        scroll.border.0,
        scroll.border.1,
    ];
    Ok(u32::from(scroll.scrolls.0) | (u32::from(scroll.scrolls.1) << 1))
}

#[op2(fast)]
fn op_blitz_scroll_component(state: Rc<RefCell<OpState>>, index: u32) -> f64 {
    state.borrow().borrow::<ScrollMetrics>().0[(index as usize).min(7)]
}

/// Sets the scroll offset of an element (the viewport for the root element),
/// clamped to `0..=max` per axis; NaN keeps an axis, and axes that do not scroll
/// are left unchanged. Applies immediately (no smooth animation, no `scroll`
/// event). Only Blitz state is written: no JS runs while the document is borrowed.
#[op2(fast)]
fn op_blitz_scroll_to(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    x: f64,
    y: f64,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    document.resolve(0.0);
    let Some(scroll) = scroll_box(&document, node_id) else {
        return Ok(());
    };
    let axis = |value: f64, current: f64, max: f64, scrolls: bool| {
        if scrolls && !value.is_nan() {
            value.clamp(0.0, max)
        } else {
            current
        }
    };
    let x = axis(x, scroll.offset.0, scroll.max.0, scroll.scrolls.0);
    let y = axis(y, scroll.offset.1, scroll.max.1, scroll.scrolls.1);
    if node_id == document.root_element().id {
        let mut viewport = document.viewport_scroll();
        viewport.x = x as _;
        viewport.y = y as _;
        document.set_viewport_scroll(viewport);
    } else if let Some(node) = document.get_node_mut(node_id) {
        let offset = node.scroll_offset_mut();
        offset.x = x as _;
        offset.y = y as _;
    }
    Ok(())
}

/// Box sizes for ResizeObserver, from the Taffy layout Blitz already computed:
/// stores the content rect relative to the border box (padding-left,
/// padding-top, content width, content height) and the border-box size.
/// `flush` forces style + layout first (once per observation pass in `dom.js`).
/// Elements without a box (e.g. `display: none`) have a zero layout.
#[op2(fast)]
fn op_blitz_box_sizes(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    flush: bool,
) -> Result<(), JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    if flush {
        document.resolve(0.0);
    }
    let layout = *document
        .get_node(node_id)
        .ok_or_else(|| JsErrorBox::type_error("node is gone"))?
        .final_layout();
    let (size, border, padding) = (layout.size, layout.border, layout.padding);
    let content_width = size.width
        - border.left
        - border.right
        - padding.left
        - padding.right
        - layout.scrollbar_size.width;
    let content_height = size.height
        - border.top
        - border.bottom
        - padding.top
        - padding.bottom
        - layout.scrollbar_size.height;
    state.borrow_mut().borrow_mut::<LastRect>().0 = [
        f64::from(padding.left),
        f64::from(padding.top),
        f64::from(content_width.max(0.0)),
        f64::from(content_height.max(0.0)),
        f64::from(size.width),
        f64::from(size.height),
    ];
    Ok(())
}

#[op2(fast)]
fn op_blitz_rect_component(state: Rc<RefCell<OpState>>, index: u32) -> f64 {
    state.borrow().borrow::<LastRect>().0[(index as usize).min(5)]
}

/// Viewport size in CSS pixels: 0 = width, 1 = height.
#[op2(fast)]
fn op_blitz_viewport_size(state: Rc<RefCell<OpState>>, component: u32) -> f64 {
    let document = probe_document(&state);
    let document = document.borrow();
    let (width, height) = viewport_css_size(&document);
    if component == 0 { width } else { height }
}

/// Moves Blitz focus (`:focus` styling, later keyboard routing). `dom.js` checks
/// focusability and fires the focus events.
#[op2(fast)]
fn op_blitz_focus(state: Rc<RefCell<OpState>>, handle: f64) -> Result<bool, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    Ok(document.set_focus_to(node_id))
}

#[op2(fast)]
fn op_blitz_blur(state: Rc<RefCell<OpState>>) {
    let document = probe_document(&state);
    document.borrow_mut().clear_focus();
}

#[op2(fast)]
fn op_blitz_focused(state: Rc<RefCell<OpState>>) -> f64 {
    let document = probe_document(&state);
    let document = document.borrow();
    document.get_focussed_node_id().map_or(NO_NODE, to_handle)
}

/// RGBA8 pixels of a deno_image `ImageBitmap`, for the probe's
/// `GPUQueue.copyExternalImageToTexture` polyfill (deno_webgpu 0.227 lacks it).
#[op2]
#[buffer]
fn op_blitz_image_bitmap_rgba(#[cppgc] bitmap: &deno_image::bitmap::ImageBitmap) -> Vec<u8> {
    bitmap.data.borrow().to_rgba8().into_raw()
}

/// Teardown only (captured by the bootstrap's `shutdownGpu`, never exposed to
/// the guest): blocks until the device's queue has no pending submission,
/// within `GPU_SHUTDOWN_TIMEOUT`. The pinned deno_webgpu 0.227 has no
/// `GPUQueue.onSubmittedWorkDone`, so this polls wgpu-core directly; the
/// caller destroys the device only after it succeeds. Returns the poll status
/// and wait time as JSON; a wgpu error or timeout throws.
#[op2]
#[string]
fn op_blitz_gpu_wait_idle(
    #[cppgc] device: &deno_webgpu::device::GPUDevice,
) -> Result<String, JsErrorBox> {
    let start = std::time::Instant::now();
    let status = device
        .instance
        .device_poll(
            device.id,
            deno_webgpu::wgpu_types::PollType::Wait {
                submission_index: None,
                timeout: Some(GPU_SHUTDOWN_TIMEOUT),
            },
        )
        .map_err(|error| {
            JsErrorBox::generic(format!(
                "WebGPU queue did not become idle at teardown after {} ms: {error}",
                start.elapsed().as_millis()
            ))
        })?;
    Ok(format!(
        "{{\"poll_status\":\"{status:?}\",\"queue_idle_wait_ms\":{}}}",
        start.elapsed().as_millis()
    ))
}

/// Teardown diagnostics only (captured by the bootstrap's `shutdownGpu`, never
/// exposed to the guest): the device's adapter as wgpu-core reports it
/// (backend, device type, name, driver) plus one non-blocking queue poll, taken
/// before `op_blitz_gpu_wait_idle` blocks so a timeout still leaves evidence of
/// which adapter and backend hung. A wgpu error in the poll is reported, not thrown.
#[op2]
#[string]
fn op_blitz_gpu_adapter_info(#[cppgc] device: &deno_webgpu::device::GPUDevice) -> String {
    let info = device.instance.adapter_get_info(device.adapter);
    let poll = device
        .instance
        .device_poll(device.id, deno_webgpu::wgpu_types::PollType::Poll)
        .map_or_else(
            |error| format!("error: {error}"),
            |status| format!("{status:?}"),
        );
    deno_core::serde_json::json!({
        "backend": format!("{:?}", info.backend),
        "device_type": format!("{:?}", info.device_type),
        "name": info.name,
        "vendor": info.vendor,
        "device": info.device,
        "driver": info.driver,
        "driver_info": info.driver_info,
        "pre_wait_poll": poll,
    })
    .to_string()
}

/// `element.matches(selector)` with Blitz's Stylo matching: 1 / 0, or -1 for an
/// invalid selector (SyntaxError in JS).
#[op2(fast)]
fn op_blitz_matches(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[string] selector: &str,
) -> Result<i32, JsErrorBox> {
    let document = probe_document(&state);
    let document = document.borrow();
    let node_id = attached_node(&document, handle)?.id;
    Ok(match document.matches_selector(node_id, selector) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => -1,
    })
}

#[op2(fast)]
fn op_blitz_create_comment(state: Rc<RefCell<OpState>>, #[string] data: &str) -> f64 {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let id = document.mutate().create_comment_node(data);
    to_handle(id)
}

/// `node.cloneNode(deep)`: Blitz deep-clones; a shallow clone detaches the
/// cloned children again (they stay unreferenced and are freed on GC paths).
#[op2(fast)]
fn op_blitz_clone_node(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    deep: bool,
) -> Result<f64, JsErrorBox> {
    let document = probe_document(&state);
    let mut document = document.borrow_mut();
    let node_id = attached_node(&document, handle)?.id;
    let clone = document.mutate().deep_clone_node(node_id);
    if !deep {
        let children: Vec<NodeId> = document
            .get_node(clone)
            .map(|node| node.children.iter().copied().collect())
            .unwrap_or_default();
        let mut mutator = document.mutate();
        for child in children {
            mutator.remove_and_drop_node(child);
        }
    }
    Ok(to_handle(clone))
}

/// Game files reachable through `fetch` (only in `--game` mode).
struct GameFiles(Option<Arc<game::GameContent>>);

/// `fetch("game://content/...")`: the file's bytes, or an error (404 in JS).
#[op2]
#[buffer]
fn op_blitz_game_read(state: &mut OpState, #[string] url: &str) -> Result<Vec<u8>, JsErrorBox> {
    state
        .borrow::<GameFiles>()
        .0
        .as_ref()
        .and_then(|game| game.read(url))
        .ok_or_else(|| JsErrorBox::type_error(format!("not found: {url}")))
}

/// The PermissionsContainer op_fetch requires. Net is granted to every host
/// (`Some(vec![])`): which destinations are safe is decided by src/net_policy.rs
/// (https, public addresses at resolution time, CSP) and src/fetch.js (CORS),
/// not by an allow-list. Every other permission stays denied without prompting.
fn fetch_permissions() -> deno_permissions::PermissionsContainer {
    use deno_permissions::{
        Permissions, PermissionsContainer, PermissionsOptions, RuntimePermissionDescriptorParser,
    };
    let parser = Arc::new(RuntimePermissionDescriptorParser::new(
        sys_traits::impls::RealSys,
    ));
    let options = PermissionsOptions {
        allow_net: Some(vec![]),
        prompt: false,
        ..Default::default()
    };
    let permissions = Permissions::from_options(parser.as_ref(), &options)
        .expect("net permission options must be valid");
    PermissionsContainer::new(parser, permissions)
}

/// Mozilla's root store (webpki-roots via deno_tls) for https, built on first use.
#[derive(Default)]
struct WebPkiRoots(std::sync::OnceLock<deno_tls::rustls::RootCertStore>);

impl deno_tls::RootCertStoreProvider for WebPkiRoots {
    fn get_or_try_init(&self) -> Result<&deno_tls::rustls::RootCertStore, JsErrorBox> {
        Ok(self.0.get_or_init(deno_tls::create_default_root_cert_store))
    }
}

/// Fixture mode only: a loopback HTTP/1.1 server for the remote `fetch` tests, so
/// they need no network. It is the only non-https, non-public destination the
/// policy accepts, registered with `net_policy::allow_fixture_loopback` (never in
/// a `--game` run). Each response closes its connection. Paths: `/cors` (allowed,
/// echoes the received Origin and whether credentials were sent), `/no-cors` (no
/// Access-Control-Allow-Origin), `/redirect` (302 to `/cors`), `/redirect-loop`,
/// `/redirect-no-cors`, and redirects to unsafe destinations (`/redirect-http`,
/// `/redirect-private`, `/redirect-localhost`, `/redirect-credentials`).
fn spawn_fetch_test_server() -> deno_core::url::Url {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    net_policy::allow_fixture_loopback(address);
    let port = address.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut origin = String::from("(none)");
            let mut credentials = false;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    match name.trim().to_ascii_lowercase().as_str() {
                        "origin" => origin = value.trim().to_string(),
                        "authorization" | "cookie" => credentials = true,
                        _ => {},
                    }
                }
            }
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            let (status, headers, body) = match path {
                "/cors" => (
                    "200 OK",
                    "Access-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: X-Exposed\r\n\
                     X-Exposed: yes\r\nX-Hidden: no\r\nSet-Cookie: probe=1\r\n",
                    format!("origin={origin};credentials={credentials}"),
                ),
                "/no-cors" => ("200 OK", "", "secret".to_string()),
                "/redirect" => (
                    "302 Found",
                    "Access-Control-Allow-Origin: *\r\nLocation: /cors\r\n",
                    String::new(),
                ),
                "/redirect-loop" => (
                    "307 Temporary Redirect",
                    "Access-Control-Allow-Origin: *\r\nLocation: /redirect-loop\r\n",
                    String::new(),
                ),
                "/redirect-no-cors" => ("302 Found", "Location: /cors\r\n", String::new()),
                // Unsafe hops: every one must be refused before it is contacted.
                "/redirect-http" => (
                    "302 Found",
                    "Access-Control-Allow-Origin: *\r\nLocation: http://example.com/\r\n",
                    String::new(),
                ),
                "/redirect-private" => (
                    "301 Moved Permanently",
                    "Access-Control-Allow-Origin: *\r\nLocation: https://169.254.169.254/latest/meta-data/\r\n",
                    String::new(),
                ),
                "/redirect-localhost" => (
                    "308 Permanent Redirect",
                    "Access-Control-Allow-Origin: *\r\nLocation: https://localhost:8443/\r\n",
                    String::new(),
                ),
                "/redirect-credentials" => (
                    "302 Found",
                    "Access-Control-Allow-Origin: *\r\nLocation: https://user:pw@example.com/\r\n",
                    String::new(),
                ),
                _ => (
                    "404 Not Found",
                    "Access-Control-Allow-Origin: *\r\n",
                    String::new(),
                ),
            };
            // HEAD reports the GET length without sending the body.
            let length = body.len();
            let sent = if request_line.starts_with("HEAD ") {
                ""
            } else {
                body.as_str()
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {length}\r\n\
                 Connection: close\r\n{headers}\r\n{sent}"
            );
            let _ = (&stream).write_all(response.as_bytes());
        }
    });
    deno_core::url::Url::parse(&format!("http://127.0.0.1:{port}")).expect("loopback origin")
}

deno_core::extension!(
    roves_blitz_v8_probe,
    ops = [
        op_blitz_document_root,
        op_blitz_game_read,
        op_blitz_fetch_check,
        op_blitz_matches,
        op_blitz_create_comment,
        op_blitz_clone_node,
        op_blitz_image_bitmap_rgba,
        op_blitz_gpu_wait_idle,
        op_blitz_gpu_adapter_info,
        op_blitz_measure,
        op_blitz_box_sizes,
        op_blitz_rect_component,
        op_blitz_viewport_size,
        op_blitz_scroll_metrics,
        op_blitz_scroll_component,
        op_blitz_scroll_to,
        op_blitz_focus,
        op_blitz_blur,
        op_blitz_focused,
        op_blitz_insert_before,
        op_blitz_child_count,
        op_blitz_child_at,
        op_blitz_sibling,
        op_blitz_query,
        op_blitz_query_result,
        op_blitz_outer_html,
        op_blitz_inner_html,
        op_blitz_set_inner_html,
        op_blitz_remove_attribute,
        op_blitz_retain,
        op_blitz_release,
        op_blitz_is_connected,
        op_blitz_get_element_by_id,
        op_blitz_node_type,
        op_blitz_parent,
        op_blitz_local_name,
        op_blitz_text_content,
        op_blitz_set_text_content,
        op_blitz_create_element,
        op_blitz_create_element_ns,
        op_blitz_prefix,
        op_blitz_create_text_node,
        op_blitz_append_child,
        op_blitz_remove_child,
        op_blitz_get_attribute,
        op_blitz_set_attribute,
        op_blitz_set_style_property,
        op_blitz_namespace,
        op_blitz_computed_style,
    ]
);

fn resolve_layout(document: &Rc<RefCell<HtmlDocument>>) {
    let mut document = document.borrow_mut();
    // Apply resources (stylesheets, fonts) delivered by the NetProvider.
    document.handle_messages();
    document.set_viewport(Viewport::new(
        VIEWPORT.0,
        VIEWPORT.1,
        1.0,
        ColorScheme::Light,
    ));
    document.resolve(0.0);
}

/// Hit-tests a point with Blitz's `elementFromPoint` semantics (text and
/// anonymous boxes resolve to their element), which is the mouse event target.
fn hit_target(document: &Rc<RefCell<HtmlDocument>>, x: f32, y: f32) -> Option<NodeId> {
    document.borrow().element_from_point(x, y)
}

const VIEWPORT: (u32, u32) = (640, 360);

/// Border-box rectangle `(x, y, width, height)` of the element with `id`, in
/// document pixels (`Node::absolute_position` walks the containing blocks).
fn element_rect(document: &Rc<RefCell<HtmlDocument>>, id: &str) -> (f32, f32, f32, f32) {
    let document = document.borrow();
    let node_id = find_element_by_id(&document, id).expect("element present");
    let node = document.get_node(node_id).unwrap();
    let size = node.final_layout().size;
    let origin = node.absolute_position(0.0, 0.0);
    (origin.x, origin.y, size.width, size.height)
}

/// Paints the resolved document into an RGBA8 buffer with Vello's CPU renderer.
/// CPU on purpose: the wgpu path (anyrender_vello 0.6) crashed with
/// STATUS_ACCESS_VIOLATION on GPU-less Windows CI runners, and a CPU raster is
/// deterministic everywhere. Presenting to a real window is a separate step.
fn render_rgba(document: &Rc<RefCell<HtmlDocument>>) -> Vec<u8> {
    let (width, height) = VIEWPORT;
    let mut document = document.borrow_mut();
    let document: &mut BaseDocument = &mut document;
    anyrender::render_to_buffer::<anyrender_vello_cpu::VelloCpuImageRenderer, _>(
        |scene| blitz_paint::paint_scene(scene, document, 1.0, width, height, 0, 0),
        width,
        height,
    )
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

/// One frame of the JS side: rAF callbacks, a microtask checkpoint and one
/// non-blocking poll of the deno_core event loop (timers, op completions). Style
/// and layout are resolved by the caller (headless loop) or the window shell.
fn frame_step(runtime: &mut JsRuntime, run_animation_frame: &v8::Global<v8::Function>) {
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
        panic!("event loop error: {error}");
    }
}

/// Drives `frames` frames without a window, paced at 60 Hz in real time (awaiting
/// tokio, so its timers advance too) so timers become due: `frame_step`, then Blitz layout.
async fn drive_frames(
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
            // Await (not thread::sleep) so tokio's timer driver runs: deno_webgpu's
            // mapAsync and other tokio-based ops only progress while it does.
            tokio::time::sleep(wait).await;
        }
        frame_step(runtime, run_animation_frame);
        resolve_layout(document);
    }
}

/// Dispatches a trusted UI event of `kind` with a JSON init dictionary to `target`
/// through the bootstrap's dispatcher (which picks MouseEvent, PointerEvent,
/// KeyboardEvent or FocusEvent by type).
fn dispatch_trusted(
    runtime: &mut JsRuntime,
    dispatcher: &v8::Global<v8::Function>,
    target: NodeId,
    kind: &str,
    init: &str,
) {
    deno_core::scope!(scope, runtime);
    let function = v8::Local::new(scope, dispatcher);
    let receiver = v8::undefined(scope).into();
    let node = v8::Number::new(scope, to_handle(target)).into();
    let kind = v8::String::new(scope, kind).unwrap().into();
    let init = v8::String::new(scope, init).unwrap().into();
    function
        .call(scope, receiver, &[node, kind, init])
        .expect("trusted event dispatch should not throw");
}

/// Evaluates a script that returns a string.
fn evaluate_string(runtime: &mut JsRuntime, source: &'static str) -> String {
    let value = runtime
        .execute_script("roves-blitz-evaluate.js", source)
        .expect("evaluation should succeed");
    deno_core::scope!(scope, runtime);
    let value = v8::Local::new(scope, value);
    value.to_rust_string_lossy(scope)
}

/// Calls a bootstrap hook that returns a string.
fn evaluate_hook(runtime: &mut JsRuntime, hook: &v8::Global<v8::Function>) -> String {
    deno_core::scope!(scope, runtime);
    let function = v8::Local::new(scope, hook);
    let receiver = v8::undefined(scope).into();
    let value = function
        .call(scope, receiver, &[])
        .expect("hook should not throw");
    value.to_rust_string_lossy(scope)
}

/// Upper bound for the GPU queue to drain at teardown (native `device_poll` in
/// `op_blitz_gpu_wait_idle`) and for a still-pending `requestDevice` to settle.
/// Far above wgpu-core 29's own ~6 s Queue Drop budget so a slow (software)
/// adapter can finish; exceeding it fails the run loudly.
const GPU_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Orderly WebGPU teardown before the runtime (and with it deno_webgpu's wgpu
/// Instance, Device and Queue) is dropped: awaits the bootstrap's `shutdownGpu`
/// (settles a pending `requestDevice`, waits for the queue to be idle through
/// `op_blitz_gpu_wait_idle`, then `GPUDevice.destroy`) while the event loop
/// runs, so wgpu-core's Queue Drop finds no pending submission instead of
/// panicking after its fixed wait. Prints a `gpu_teardown` result line.
async fn shutdown_gpu_before_drop(
    runtime: &mut JsRuntime,
    shutdown_gpu: &v8::Global<v8::Function>,
) {
    stage("gpu_teardown");
    let start = std::time::Instant::now();
    let value = tokio::time::timeout(GPU_SHUTDOWN_TIMEOUT, runtime.call_and_await(shutdown_gpu))
        .await
        .unwrap_or_else(|_| {
            panic!("WebGPU queue did not become idle within {GPU_SHUTDOWN_TIMEOUT:?} at teardown")
        })
        .unwrap_or_else(|error| panic!("WebGPU teardown failed: {error}"));
    let detail = {
        deno_core::scope!(scope, runtime);
        let value = v8::Local::new(scope, value);
        value.to_rust_string_lossy(scope)
    };
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"gpu_teardown\",\"status\":\"passed\",\"elapsed_ms\":{},\"detail\":{detail}}}",
        start.elapsed().as_millis()
    );
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

/// Progress marker printed and flushed immediately, so a hard crash (no panic
/// message) still shows the last phase reached in the CI log.
fn stage(name: &str) {
    use std::io::Write;
    println!("ROVES_PROBE_STAGE {name}");
    let _ = std::io::stdout().flush();
}

fn main() {
    if std::env::args().len() == 1 {
        audio::check_offline_render();
    }
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime should initialize");
    // Same switch production uses (roves-v8's `jitless` feature): it must be set
    // before the first JsRuntime initializes V8.
    let jitless = std::env::var("ROVES_PROBE_JITLESS").is_ok_and(|value| value == "1");
    let mut flags = vec![String::new()];
    if jitless {
        flags.push("--jitless".into());
    }
    let rejected = deno_core::v8_set_flags(flags);
    assert!(rejected.len() == 1, "V8 rejected flags: {rejected:?}");
    // Origin storage for deno_webstorage; CI points two runs at the same directory
    // to check that localStorage persists across processes.
    let storage_dir = std::env::var_os("ROVES_PROBE_STORAGE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("roves-blitz-probe-storage"));
    std::fs::create_dir_all(&storage_dir).expect("storage directory");
    tokio.block_on(async {
        stage("parse_html");
        let game = game::GameContent::from_args();
        // Fixture mode only: the loopback server for the remote fetch tests, the one
        // non-public destination the network policy accepts (never with --game).
        let fetch_test_server = game.is_none().then(spawn_fetch_test_server);
        if game.is_none() {
            net_policy::self_check();
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"fetch_public_address_policy\",\"status\":\"passed\"}}"
            );
        }
        let document = Rc::new(RefCell::new(match &game {
            Some(game) => game.document(),
            None => HtmlDocument::from_html(
                FIXTURE,
                // The html5ever provider enables `innerHTML` fragment parsing.
                DocumentConfig {
                    html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                    ..Default::default()
                },
            ),
        }));
        stage("create_js_runtime");
        let mut runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![
                deno_webidl::deno_webidl::init(),
                deno_web::deno_web::init(
                    Arc::new(deno_web::BlobStore::default()),
                    None,
                    false,
                    deno_web::InMemoryBroadcastChannel::default(),
                ),
                // localStorage in SQLite under the origin directory; sessionStorage in memory.
                deno_webstorage::deno_webstorage::init(Some(storage_dir.clone())),
                // WebGPU (navigator.gpu) from Deno's maintained extension.
                deno_webgpu::deno_webgpu::init(),
                // createImageBitmap (image decoding) from Deno's maintained extension.
                deno_image::deno_image::init(),
                // fetch/Headers/Request/Response/FormData; src/fetch.js wraps fetch
                // with the game://, CORS and redirect policy. Every connection goes
                // through net_policy's public-address resolver and request hook. No
                // client certificate or Deno.createHttpClient; file: keeps
                // deno_fetch's refusing handler. `proxy: None` does not disable the
                // environment proxies deno_fetch's create_http_client always reads,
                // so net_policy refuses every request while one is configured.
                deno_fetch::deno_fetch::init(deno_fetch::Options {
                    user_agent: "Roves".to_string(),
                    root_cert_store_provider: Some(Arc::new(WebPkiRoots::default())),
                    request_builder_hook: Some(net_policy::request_hook),
                    resolver: deno_fetch::dns::Resolver::custom(Arc::new(
                        net_policy::PublicResolver,
                    )),
                    ..Default::default()
                }),
                // Only because deno_fetch's 22_http_client.js loads
                // ext:deno_net/02_tls.js at bootstrap (same position as in Deno's
                // runtime, after deno_fetch). Its socket/TLS ops are reachable only
                // through `Deno.core`, which dom.js deletes from the guest global; no
                // root store or certificate-error override is given to them.
                deno_net::deno_net::init(None, None),
                audio::roves_blitz_audio::init(),
                // Dedicated workers: one isolate and thread each (src/worker.rs).
                worker::roves_blitz_worker::init(),
                roves_blitz_v8_probe::init(),
            ],
            module_loader: game.as_ref().map(|game| game.module_loader()),
            ..Default::default()
        });
        runtime
            .op_state()
            .borrow_mut()
            .put(ProbeDocument(document.clone()));
        runtime.op_state().borrow_mut().put(WrappedNodes::default());
        runtime.op_state().borrow_mut().put(QueryResults::default());
        runtime.op_state().borrow_mut().put(LastRect::default());
        runtime.op_state().borrow_mut().put(ScrollMetrics::default());
        runtime
            .op_state()
            .borrow_mut()
            .put(GameFiles(game.clone()));
        // op_fetch requires a PermissionsContainer (net only, see fetch_permissions).
        runtime.op_state().borrow_mut().put(fetch_permissions());
        // Remote fetch honours the CSP the page declared in <head> when parsed.
        let document_csp = net_policy::DocumentCsp::from_document(&document.borrow());
        runtime.op_state().borrow_mut().put(document_csp);

        // The bootstrap evaluates to the embedder hooks. Keeping them as Rust-held
        // handles means no dispatch or frame entry point is left on the guest global.
        if std::env::args().len() == 1 {
            runtime
                .execute_script("roves-blitz-audio-test.js", include_str!("audio_test.js"))
                .expect("V8 should create and close a headless audio context");
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"web_audio_v8_context\",\"status\":\"passed\"}}"
            );
        }
        runtime
            .execute_script("roves-blitz-history.js", HISTORY)
            .expect("history factory should load");
        runtime
            .execute_script("roves-blitz-fetch.js", FETCH)
            .expect("fetch factory should load");
        runtime
            .execute_script("roves-blitz-web-audio.js", WEB_AUDIO)
            .expect("Web Audio factory should load");
        runtime
            .execute_script("roves-blitz-indexeddb.js", INDEXED_DB)
            .expect("IndexedDB factory should load");
        runtime
            .execute_script("roves-blitz-worker.js", WORKER)
            .expect("Worker factory should load");
        stage("dom_bootstrap");
        let hooks = runtime
            .execute_script("roves-blitz-dom.js", DOM_BOOTSTRAP)
            .expect("DOM wrapper bootstrap should install");
        let (
            dispatcher,
            run_animation_frame,
            blocked_navigations,
            document_loaded,
            shutdown_gpu,
        ) = {
            deno_core::scope!(scope, runtime);
            let hooks = v8::Local::new(scope, hooks);
            let hooks = v8::Local::<v8::Object>::try_from(hooks).expect("bootstrap hooks object");
            let function = |name: &str| {
                let key = v8::String::new(scope, name).unwrap();
                let value = hooks.get(scope, key.into()).expect(name);
                let function = v8::Local::<v8::Function>::try_from(value).expect(name);
                v8::Global::new(scope, function)
            };
            (
                function("dispatchTrustedEvent"),
                function("runAnimationFrame"),
                function("blockedNavigations"),
                function("documentLoaded"),
                function("shutdownGpu"),
            )
        };

        if game.is_some() {
            // A real game page instead of the fixture tests.
            stage("game_page");
            resolve_layout(&document);
            game::run(
                &mut runtime,
                &document,
                &run_animation_frame,
                &document_loaded,
                frame_count().max(120),
            )
            .await;
            shutdown_gpu_before_drop(&mut runtime, &shutdown_gpu).await;
            return;
        }

        stage("guest_dom_test");
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
        stage("storage_and_history");
        runtime
            .execute_script("roves-blitz-storage-test.js", STORAGE_TEST)
            .expect("localStorage/sessionStorage checks should pass");
        runtime
            .execute_script("roves-blitz-history-test.js", HISTORY_TEST)
            .expect("History/Location checks should pass");
        let storage_runs = evaluate_string(&mut runtime, "String(storageRuns)");

        stage("dom_api_test");
        runtime
            .execute_script("roves-blitz-dom-api-test.js", DOM_API_TEST)
            .expect("tree navigation, selectors, classList and innerHTML checks should pass");

        // Geometry, matchMedia, focus and event classes; registers listeners for
        // the trusted pointer/mouse/keyboard events Rust sends below.
        resolve_layout(&document);
        stage("events_test");
        runtime
            .execute_script("roves-blitz-events-test.js", EVENTS_TEST)
            .expect("geometry, matchMedia and focus checks should pass");

        stage("hit_test_and_click");
        let target = hit_target(&document, x, y).expect("hit-test should find a node");
        // What a window's input layer would send for a primary-button click.
        let mouse = format!(
            r#"{{"clientX":{x},"clientY":{y},"screenX":{x},"screenY":{y},"button":0,"buttons":1,"pointerId":1,"pointerType":"mouse","isPrimary":true}}"#
        );
        let released = mouse.replace(r#""buttons":1"#, r#""buttons":0"#);
        for (kind, init) in [
            ("pointerdown", &mouse),
            ("mousedown", &mouse),
            ("pointerup", &released),
            ("mouseup", &released),
            ("click", &released),
        ] {
            dispatch_trusted(&mut runtime, &dispatcher, target, kind, init);
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
        // Keyboard input goes to the focused element (focused by events_test.js).
        stage("keyboard");
        let focused = document
            .borrow()
            .get_focussed_node_id()
            .expect("events_test.js must leave a focused element");
        let key = r#"{"key":"Enter","code":"Enter"}"#;
        dispatch_trusted(&mut runtime, &dispatcher, focused, "keydown", key);
        dispatch_trusted(&mut runtime, &dispatcher, focused, "keyup", key);
        runtime
            .execute_script("roves-blitz-events-check.js", EVENTS_CHECK)
            .expect("trusted pointer, mouse and keyboard events should reach listeners");

        // Viewport and overflow-element scrolling on Blitz's scroll state; the
        // test restores scroll 0,0 and removes its elements before the pixel checks.
        stage("scroll");
        runtime
            .execute_script("roves-blitz-scroll-test.js", SCROLL_TEST)
            .expect("scroll offsets, dimensions, scrollTo and scrollIntoView checks should pass");
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"scroll_viewport_and_elements\",\"status\":\"passed\"}}"
        );

        stage("frame_loop");
        let server = fetch_test_server.expect("fixture mode starts the fetch test server");
        runtime
            .execute_script(
                "roves-blitz-fetch-server.js",
                format!(
                    "globalThis.fetchTestServer = {:?};",
                    server.origin().ascii_serialization()
                ),
            )
            .expect("fetch test server origin should be set");
        runtime
            .execute_script("roves-blitz-fetch-test.js", FETCH_TEST)
            .expect("fetch test setup should run");
        runtime
            .execute_script("roves-blitz-frame-test.js", FRAME_TEST)
            .expect("frame test setup should run");
        let frames = frame_count();
        drive_frames(&mut runtime, &document, &run_animation_frame, frames).await;
        runtime
            .execute_script("roves-blitz-frame-check.js", FRAME_CHECK)
            .expect("frame loop checks should pass");
        // Loopback fetches complete through deno_fetch's tokio ops: drive frames
        // until the test chain reports done (bounded), then check its result.
        for _ in 0..150 {
            if evaluate_string(&mut runtime, "String(fetchResult.done)") == "true" {
                break;
            }
            drive_frames(&mut runtime, &document, &run_animation_frame, 2).await;
        }
        runtime
            .execute_script("roves-blitz-fetch-check.js", FETCH_CHECK)
            .expect("fetch checks should pass");
        // The https://localhost:8443/ fetches passed the static checks and CSP; they
        // must have been refused by the resolver on the resolved loopback address.
        // The port must not be on the Fetch "bad port" list (e.g. 1): deno_fetch
        // rejects those before DNS, so the resolver would never run (CI 42668768386).
        let refused = net_policy::refused_hosts();
        assert!(
            refused.iter().any(|host| host == "localhost"),
            "the public-address resolver should have refused localhost, refused: {refused:?}; \
             if empty, the localhost fetches were rejected before DNS (Fetch bad port, \
             proxy or CSP) instead of by net_policy::PublicResolver"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"fetch_resolver_refuses_non_public\",\"status\":\"passed\"}}"
        );
        runtime
            .execute_script("roves-blitz-history-check.js", HISTORY_CHECK)
            .expect("asynchronous popstate/hashchange must have fired");
        let box_rect = element_rect(&document, "box");
        assert_eq!(box_rect.2, 60.0, "rAF style writes must reach Blitz layout");

        // IndexedDB requests complete from timer tasks: drive frames until the
        // asynchronous test chain reports done (bounded), then check its result.
        stage("indexed_db");
        runtime
            .execute_script("roves-blitz-indexeddb-test.js", INDEXED_DB_TEST)
            .expect("IndexedDB test setup should run");
        for _ in 0..150 {
            if evaluate_string(&mut runtime, "String(indexedDbResult.done)") == "true" {
                break;
            }
            drive_frames(&mut runtime, &document, &run_animation_frame, 2).await;
        }
        let indexed_db_runs = evaluate_string(
            &mut runtime,
            "if (!indexedDbResult.done) throw new Error('IndexedDB tests did not finish at ' + indexedDbResult.step); \
             if (indexedDbResult.failures.length > 0) throw new Error('IndexedDB: ' + indexedDbResult.failures.join(' | ')); \
             String(indexedDbResult.runs)",
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"indexed_db\",\"status\":\"passed\",\"indexed_db_runs\":{indexed_db_runs}}}"
        );

        // Dedicated workers run on their own threads; their messages and errors
        // arrive through the event loop polled by each frame (bounded wait).
        stage("worker");
        runtime
            .execute_script("roves-blitz-worker-test.js", WORKER_TEST)
            .expect("Worker test setup should run");
        for _ in 0..300 {
            if evaluate_string(&mut runtime, "String(workerResult.done)") == "true" {
                break;
            }
            drive_frames(&mut runtime, &document, &run_animation_frame, 2).await;
        }
        let worker_checks = evaluate_string(
            &mut runtime,
            "if (!workerResult.done) throw new Error('Worker tests did not finish at ' + workerResult.step); \
             if (workerResult.failures.length > 0) throw new Error('Worker: ' + workerResult.failures.join(' | ')); \
             String(workerResult.checks)",
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"dedicated_worker_pixi_image_bitmap_probe\",\"status\":\"passed\",\"checks\":{worker_checks}}}"
        );

        // ResizeObserver: observations are processed by runAnimationFrame after
        // the rAF callbacks; each phase checks the frames driven before it.
        stage("resize_observer");
        runtime
            .execute_script("roves-blitz-resize-observer-test.js", RESIZE_OBSERVER_TEST)
            .expect("ResizeObserver setup should run");
        for phase in [
            "resizeObserverPhase(1)",
            "resizeObserverPhase(2)",
            "resizeObserverPhase(3)",
            "resizeObserverPhase(4)",
        ] {
            drive_frames(&mut runtime, &document, &run_animation_frame, 3).await;
            runtime
                .execute_script("roves-blitz-resize-observer-check.js", phase)
                .expect("ResizeObserver observe/resize/disconnect checks should pass");
        }

        // Motion: its JS frameloop (no WAAPI here) runs on rAF + performance.now and
        // writes inline transform/opacity. 0.1 s needs ~6 frames; 20 leave margin.
        stage("motion");
        runtime
            .execute_script("roves-blitz-motion-scenario.js", MOTION_SCENARIO)
            .expect("Motion scenario should load");
        drive_frames(&mut runtime, &document, &run_animation_frame, 20).await;
        runtime
            .execute_script(
                "roves-blitz-motion-check.js",
                MOTION_CHECK,
            )
            .expect("Motion animation should finish");
        let motion_rect = element_rect(&document, "motion");
        let fading_rect = element_rect(&document, "fading");

        // Tone.js bootstrap on the Web Audio bridge: context start, destination
        // volume through native automation, `ended` delivered on the V8 thread.
        stage("web_audio_tone");
        runtime
            .execute_script("roves-blitz-tone-scenario.js", TONE_SCENARIO)
            .expect("Tone.js scenario should load");
        let tone_status = "globalThis.__rovesToneScenario.status";
        let mut tone_frames = 0;
        while tone_frames < 360 && evaluate_string(&mut runtime, tone_status) == "running" {
            drive_frames(&mut runtime, &document, &run_animation_frame, 10).await;
            tone_frames += 10;
        }
        assert_eq!(
            evaluate_string(&mut runtime, tone_status),
            "passed",
            "Tone.js scenario: {}",
            evaluate_string(&mut runtime, "JSON.stringify(globalThis.__rovesToneScenario)")
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"web_audio_tone_15_1_22\",\"status\":\"passed\",\"frames\":{tone_frames}}}"
        );

        resolve_layout(&document);
        let root_layout = *document.borrow().root_element().final_layout();
        assert!(root_layout.size.width > 0.0 && root_layout.size.height > 0.0);

        // Offscreen paint of the mutated document (Blitz painter + Vello on wgpu).
        // GC lifecycle: detached nodes whose wrappers V8 collects are freed in
        // Blitz; connected nodes and still-referenced wrappers survive.
        stage("gc_lifecycle");
        let live_nodes = |document: &Rc<RefCell<HtmlDocument>>| document.borrow().tree().len();
        let nodes_before = live_nodes(&document);
        runtime
            .execute_script("roves-blitz-gc-test.js", GC_TEST)
            .expect("GC test setup should run");
        let nodes_created = live_nodes(&document);
        assert!(
            nodes_created >= nodes_before + 1000,
            "GC test must create its detached nodes: {nodes_before} -> {nodes_created}"
        );
        for _ in 0..3 {
            // A full collection through the V8 API: no `gc` global is ever
            // exposed to guest code (an --expose-gc global is not deletable).
            runtime.v8_isolate().low_memory_notification();
            drive_frames(&mut runtime, &document, &run_animation_frame, 2).await;
        }
        let nodes_after = live_nodes(&document);
        assert!(
            nodes_after <= nodes_before + 10,
            "unreferenced detached nodes must be freed: before {nodes_before}, created {nodes_created}, after {nodes_after}"
        );
        runtime
            .execute_script("roves-blitz-gc-check.js", GC_CHECK)
            .expect("retained wrappers must survive GC");

        stage("offscreen_paint");
        let pixels = render_rgba(&document);
        stage("pixel_checks");
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
        // White at opacity 0.5 over rgb(24, 32, 48) is about rgb(140, 144, 152).
        // blitz-paint 0.2.1 left the element's own background opaque; 0.3 draws it
        // inside the opacity layer.
        let fading_pixel = pixel(
            &pixels,
            fading_rect.0 + fading_rect.2 / 2.0,
            fading_rect.1 + fading_rect.3 / 2.0,
        );
        assert!(
            fading_pixel[..3]
                .iter()
                .zip([140u8, 144, 152])
                .all(|(actual, expected)| actual.abs_diff(expected) <= 4),
            "#fading must be painted at opacity 0.5: {fading_pixel:?}"
        );
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
            "ROVES_PROBE_RESULT {{\"check\":\"blitz_hit_test_dispatches_deno_web_event_capture_target_bubble\",\"status\":\"passed\",\"target\":{},\"x\":{x},\"y\":{y}}}",
            target.as_u64()
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
            "ROVES_PROBE_RESULT {{\"check\":\"opacity_on_element_background\",\"status\":\"passed\",\"computed_opacity\":\"0.5\",\"painted\":{fading_pixel:?}}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"dom_tree_navigation_selectors_classlist_innerhtml\",\"status\":\"passed\"}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"geometry_matchmedia_focus_pointer_keyboard_events\",\"status\":\"passed\"}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"gc_frees_unreferenced_detached_nodes\",\"status\":\"passed\",\"nodes_before\":{nodes_before},\"nodes_created\":{nodes_created},\"nodes_after\":{nodes_after}}}"
        );
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"storage_history_single_document\",\"status\":\"passed\",\"local_storage_runs\":{storage_runs},\"blocked_navigations\":{}}}",
            evaluate_hook(&mut runtime, &blocked_navigations)
        );
        // WebGPU + PixiJS: opt-in (ROVES_PROBE_WEBGPU=1) because it needs a GPU;
        // the CI runners have none, so this runs from the CI-built artifact on a
        // machine with a GPU.
        if std::env::var("ROVES_PROBE_WEBGPU").is_ok_and(|value| value == "1") {
            stage("webgpu_pixi");
            // ROVES_PROBE_WEBGPU_SCENARIO=<bundle.js> overrides the embedded bundle,
            // so the scenario can be iterated on with a downloaded executable.
            let scenario = match std::env::var("ROVES_PROBE_WEBGPU_SCENARIO") {
                Ok(path) => std::fs::read_to_string(&path).expect("WebGPU scenario override"),
                Err(_) => WEBGPU_SCENARIO.to_string(),
            };
            runtime
                .execute_script("roves-blitz-webgpu-scenario.js", scenario)
                .expect("WebGPU scenario should load");
            let mut status = String::from("running");
            for _ in 0..600 {
                drive_frames(&mut runtime, &document, &run_animation_frame, 1).await;
                status = evaluate_string(&mut runtime, "webgpuResult.status");
                if status != "running" {
                    break;
                }
            }
            let detail = evaluate_string(&mut runtime, "JSON.stringify(webgpuResult)");
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"webgpu_pixi_8_19_0\",\"status\":\"{status}\",\"detail\":{detail}}}"
            );
            assert!(status == "passed", "WebGPU/Pixi scenario did not pass: {detail}");
        } else {
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"webgpu_pixi_8_19_0\",\"status\":\"not_run\",\"reason\":\"set ROVES_PROBE_WEBGPU=1 on a machine with a GPU\"}}"
            );
        }
        if !std::env::args().any(|arg| arg == "--window") {
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"window_presentation_real_input\",\"status\":\"not_run\",\"reason\":\"pass --window\"}}"
            );
            shutdown_gpu_before_drop(&mut runtime, &shutdown_gpu).await;
            return;
        }
        // Real window: blitz-shell presents the same document and runtime and
        // routes OS input; the process exits once the window test settles.
        stage("window");
        runtime
            .execute_script("roves-blitz-window-test.js", WINDOW_TEST)
            .expect("window test setup should run");
        window::run(window::RovesDocument::new(
            document,
            runtime,
            dispatcher,
            run_animation_frame,
        ));
    });
}
