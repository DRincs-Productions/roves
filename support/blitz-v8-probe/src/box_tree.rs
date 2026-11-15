//! Regression for Blitz 0.3.0-beta.4's box-tree lifetime (patched in
//! `vendor/blitz-dom`, `src/resolve.rs`): a container that leaves the box tree
//! (a `display: none` ancestor, or detached from the DOM) kept its
//! `layout_children`, which Blitz never revisits there. Freeing one of its
//! children then left a slab key that no longer exists in that list, and the
//! next `resolve` aborted in debug builds with "layout child ... is not in the
//! slab" (the visual-novel-template Skip crash, 14 November 2026). Each test
//! frees the child like the probe's DOM ops do for a node without a JS wrapper
//! (`remove_node`, then `remove_and_drop_node` of the detached node), then
//! brings the container back and checks that its box is rebuilt from the live
//! children only.

use blitz_dom::{BaseDocument, DocumentConfig, NodeId};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::{VIEWPORT, attribute_name, find_element_by_id, html_name};

const BODY: &str = r#"<div id="outer"><div id="container"><p id="kept">kept</p><div id="victim">victim</div></div></div>"#;

fn document() -> HtmlDocument {
    let html = format!("<!doctype html><html><body>{BODY}</body></html>");
    let mut document = HtmlDocument::from_html(&html, DocumentConfig::default());
    document.set_viewport(Viewport::new(
        VIEWPORT.0,
        VIEWPORT.1,
        1.0,
        ColorScheme::Light,
    ));
    document.resolve(0.0);
    document
}

fn id(document: &BaseDocument, id: &str) -> NodeId {
    find_element_by_id(document, id).unwrap_or_else(|| panic!("#{id}"))
}

/// `removeChild` of a node no JS wrapper references: detached, then freed.
fn free_child(document: &mut HtmlDocument, child: NodeId) {
    document.mutate().remove_node(child);
    document.mutate().remove_and_drop_node(child);
    assert!(document.get_node(child).is_none(), "the child is freed");
}

fn set_hidden(document: &mut HtmlDocument, node: NodeId, hidden: bool) {
    let mut mutator = document.mutate();
    if hidden {
        mutator.set_attribute(node, attribute_name("style"), "display: none");
    } else {
        mutator.clear_attribute(node, attribute_name("style"));
    }
}

/// Appends a new `<div>` with text to `parent`.
fn append_div(document: &mut HtmlDocument, parent: NodeId) -> NodeId {
    let mut mutator = document.mutate();
    let div = mutator.create_element(html_name("div"), Vec::new());
    let text = mutator.create_text_node("added");
    mutator.append_children(div, &[text]);
    mutator.append_children(parent, &[div]);
    div
}

/// The container is laid out again with exactly `expected` as its boxes, each
/// pointing back at it, and the appended box has a height.
fn assert_rebuilt(document: &HtmlDocument, container: NodeId, expected: &[NodeId]) {
    let node = document.get_node(container).expect("container");
    let children = node.layout_children.borrow().clone();
    assert_eq!(
        children.as_deref(),
        Some(expected),
        "the container's box lists only its live children"
    );
    for &child in expected {
        let child = document.get_node(child).expect("live layout child");
        assert_eq!(child.layout_parent.get(), Some(container));
        assert!(
            child.final_layout().size.height > 0.0,
            "a rebuilt child is laid out"
        );
    }
}

#[test]
fn a_child_freed_under_a_hidden_ancestor_does_not_outlive_the_box() {
    let mut document = document();
    let (outer, container, kept, victim) = (
        id(&document, "outer"),
        id(&document, "container"),
        id(&document, "kept"),
        id(&document, "victim"),
    );
    set_hidden(&mut document, outer, true);
    document.resolve(0.0);
    free_child(&mut document, victim);
    // Aborted here before the patch: the hidden container still listed `victim`.
    document.resolve(0.0);
    let added = append_div(&mut document, container);
    set_hidden(&mut document, outer, false);
    document.resolve(0.0);
    assert_rebuilt(&document, container, &[kept, added]);
}

#[test]
fn hiding_and_freeing_in_the_same_frame() {
    let mut document = document();
    let (outer, container, kept, victim) = (
        id(&document, "outer"),
        id(&document, "container"),
        id(&document, "kept"),
        id(&document, "victim"),
    );
    set_hidden(&mut document, outer, true);
    free_child(&mut document, victim);
    document.resolve(0.0);
    document.resolve(0.0);
    set_hidden(&mut document, outer, false);
    document.resolve(0.0);
    assert_rebuilt(&document, container, &[kept]);
}

#[test]
fn a_child_freed_while_its_container_is_detached_then_reinserted() {
    let mut document = document();
    let (outer, container, kept, victim) = (
        id(&document, "outer"),
        id(&document, "container"),
        id(&document, "kept"),
        id(&document, "victim"),
    );
    // Detached but kept alive (a JS wrapper still references it).
    document.mutate().remove_node(container);
    document.resolve(0.0);
    free_child(&mut document, victim);
    document.resolve(0.0);
    document.mutate().append_children(outer, &[container]);
    document.resolve(0.0);
    assert_rebuilt(&document, container, &[kept]);
}
