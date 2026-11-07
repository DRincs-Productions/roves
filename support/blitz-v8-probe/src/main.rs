use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blitz_dom::{BaseDocument, DocumentConfig, LocalName, Node, NodeData, NodeId, QualName, ns};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use deno_core::{JsRuntime, OpState, RuntimeOptions, op2, v8};
use deno_error::JsErrorBox;
use style::properties::{PropertyDeclarationId, PropertyId};

mod window;

const FIXTURE: &str = include_str!("../fixtures/layout.html");
const DOM_BOOTSTRAP: &str = include_str!("dom.js");
const GUEST_TEST: &str = include_str!("guest_test.js");
const FRAME_TEST: &str = include_str!("frame_test.js");
const FRAME_CHECK: &str = include_str!("frame_check.js");
const MOTION_CHECK: &str = include_str!("motion_check.js");
const GC_TEST: &str = include_str!("gc_test.js");
const DOM_API_TEST: &str = include_str!("dom_api_test.js");
const EVENTS_TEST: &str = include_str!("events_test.js");
const EVENTS_CHECK: &str = include_str!("events_check.js");
const WINDOW_TEST: &str = include_str!("window_test.js");
const GC_CHECK: &str = include_str!("gc_check.js");
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

/// Last rectangle measured by `op_blitz_measure` (x, y, width, height).
#[derive(Default)]
struct LastRect([f64; 4]);

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
        document.offset_rect(node_id)
    } else {
        document.get_client_bounding_rect(node_id)
    };
    let Some(rect) = rect else {
        state.borrow_mut().borrow_mut::<LastRect>().0 = [0.0; 4];
        return Ok(false);
    };
    state.borrow_mut().borrow_mut::<LastRect>().0 = [rect.x, rect.y, rect.width, rect.height];
    Ok(true)
}

#[op2(fast)]
fn op_blitz_rect_component(state: Rc<RefCell<OpState>>, index: u32) -> f64 {
    state.borrow().borrow::<LastRect>().0[(index as usize).min(3)]
}

/// Viewport size in CSS pixels: 0 = width, 1 = height.
#[op2(fast)]
fn op_blitz_viewport_size(state: Rc<RefCell<OpState>>, component: u32) -> f64 {
    let document = probe_document(&state);
    let document = document.borrow();
    let viewport = document.viewport();
    let scale = f64::from(viewport.hidpi_scale.max(f32::MIN_POSITIVE));
    let size = if component == 0 {
        viewport.window_size.0
    } else {
        viewport.window_size.1
    };
    f64::from(size) / scale
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

deno_core::extension!(
    roves_blitz_v8_probe,
    ops = [
        op_blitz_document_root,
        op_blitz_measure,
        op_blitz_rect_component,
        op_blitz_viewport_size,
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

/// Drives `frames` frames without a window, paced at 60 Hz in real time so timers
/// become due: `frame_step`, then Blitz layout.
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
    tokio.block_on(async {
        stage("parse_html");
        let document = Rc::new(RefCell::new(HtmlDocument::from_html(
            FIXTURE,
            // The html5ever provider enables `innerHTML` fragment parsing.
            DocumentConfig {
                html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                ..Default::default()
            },
        )));
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
                roves_blitz_v8_probe::init(),
            ],
            ..Default::default()
        });
        runtime
            .op_state()
            .borrow_mut()
            .put(ProbeDocument(document.clone()));
        runtime.op_state().borrow_mut().put(WrappedNodes::default());
        runtime.op_state().borrow_mut().put(QueryResults::default());
        runtime.op_state().borrow_mut().put(LastRect::default());

        // The bootstrap evaluates to the embedder hooks. Keeping them as Rust-held
        // handles means no dispatch or frame entry point is left on the guest global.
        stage("dom_bootstrap");
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
            (
                function("dispatchTrustedEvent"),
                function("runAnimationFrame"),
            )
        };

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

        stage("frame_loop");
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
        stage("motion");
        runtime
            .execute_script("roves-blitz-motion-scenario.js", MOTION_SCENARIO)
            .expect("Motion scenario should load");
        drive_frames(&mut runtime, &document, &run_animation_frame, 20);
        runtime
            .execute_script(
                "roves-blitz-motion-check.js",
                MOTION_CHECK,
            )
            .expect("Motion animation should finish");
        let motion_rect = element_rect(&document, "motion");
        let fading_rect = element_rect(&document, "fading");

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
            drive_frames(&mut runtime, &document, &run_animation_frame, 2);
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
        if !std::env::args().any(|arg| arg == "--window") {
            println!(
                "ROVES_PROBE_RESULT {{\"check\":\"window_presentation_real_input\",\"status\":\"not_run\",\"reason\":\"pass --window\"}}"
            );
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
