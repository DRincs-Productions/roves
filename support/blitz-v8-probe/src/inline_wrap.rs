//! Regression for the typewriter line jump (vendor/blitz-dom 0008, port of
//! upstream DioxusLabs/blitz#1697). markdown-typewriter 2.2.0, while typing a
//! line with HTML children, inserts `display:none` pause spans after each `,`
//! and a trailing `display:block;height:0;width:0` span. The block makes the
//! inline ancestors take Blitz's anonymous-block path, where every hidden child
//! closed the open anonymous block, so the text broke onto a new line after
//! each comma (the visual-novel-template line grew from 2 to 4 lines while
//! typing). The markup mirrors `D:/roves-ci/typewriter-phantom-ab` (2026-11-22
//! diagnosis), where Chrome 157 keeps every case at the finalized height.
//!
//! Not covered by 0008 and left open: a block inside an inline still turns the
//! whole inline into a block (upstream DioxusLabs/blitz#1434), so the typed
//! "Hey" starts a new line after the static text. Chromium keeps the block in
//! the inline formatting context instead (`AppendBlockInInline`,
//! `HandleBlockInInline`); see the ignored test at the end.

use blitz_dom::{BaseDocument, DocumentConfig, NodeData, NodeId};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::{VIEWPORT, attribute_name, find_element_by_id};

const PREV: &str = "Earlier dialogue line that is already typed.";
const HEY: &str = "Hey";
const REST: &str = ", you, Peter- be nice to our little brother,";

/// Per-character spans as markdown-typewriter emits them, with `phantoms`
/// hidden pause spans after every comma.
fn chars(text: &str, phantoms: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        out.push_str(&format!(r#"<span style="opacity:0">{c}</span>"#));
        if c == ',' {
            for _ in 0..phantoms {
                out.push_str(r#"<span style="display:none" aria-hidden="true"></span>"#);
            }
        }
    }
    out
}

/// One `.box` of the fixture: typed static text, then the line being typed.
fn typing_case(id: &str, phantoms: usize, block: bool) -> String {
    let block = if block {
        r#"<span style="display:block;height:0;width:0"></span>"#
    } else {
        ""
    };
    format!(
        r#"<div class="box" id="{id}"><p><span>{PREV}</span><span><span> </span><span style="opacity:1"><span class="ib">{}</span>{}{block}</span></span></p></div>"#,
        chars(HEY, 0),
        chars(REST, phantoms),
    )
}

fn document(body: &str) -> HtmlDocument {
    let html = format!(
        r#"<!doctype html><html><head><style>
        body {{ margin: 0; font: 16px/24px Arial, sans-serif; }}
        .box {{ width: 420px; margin: 0 0 8px 0; }}
        p {{ margin: 0; }}
        .ib {{ display: inline-block; }}
        </style></head><body>{body}</body></html>"#
    );
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

/// `[box height, top of "Hey" relative to the box]`, as the fixture reports.
fn measure(document: &BaseDocument, box_id: &str) -> [f32; 2] {
    let box_node = document.get_node(id(document, box_id)).unwrap();
    let ib = box_node
        .children
        .first()
        .copied()
        .and_then(|p| find_class(document, p, "ib"))
        .expect(".ib");
    let box_top = box_node.absolute_position(0.0, 0.0).y;
    let ib_top = document.get_node(ib).unwrap().absolute_position(0.0, 0.0).y;
    [
        box_node.final_layout().size.height,
        (ib_top - box_top).round(),
    ]
}

fn find_class(document: &BaseDocument, node: NodeId, class: &str) -> Option<NodeId> {
    let n = document.get_node(node)?;
    if n.element_data()
        .and_then(|el| el.attr(attribute_name("class").local))
        == Some(class)
    {
        return Some(node);
    }
    n.children
        .iter()
        .find_map(|&child| find_class(document, child, class))
}

fn anonymous_layout_children(document: &BaseDocument, container: NodeId) -> usize {
    let node = document.get_node(container).unwrap();
    let children = node.layout_children.borrow();
    children
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|&&child| {
            matches!(
                document.get_node(child).unwrap().data,
                NodeData::AnonymousBlock(_)
            )
        })
        .count()
}

/// Combined case (the template): hidden pause spans next to a block in an
/// inline. Unpatched: `phantoms_block` measured 96px (3 typed lines) against
/// 48px for `block_end` (probe run of 2026-11-22).
#[test]
fn hidden_pause_spans_do_not_split_text_beside_a_block_in_inline() {
    let body = [
        typing_case("block_end", 0, true),
        typing_case("phantoms_block", 2, true),
        typing_case("one_phantom_block", 1, true),
    ]
    .concat();
    let document = document(&body);
    let reference = measure(&document, "block_end");
    assert_eq!(
        measure(&document, "phantoms_block"),
        reference,
        "two pause spans per comma change the layout"
    );
    assert_eq!(
        measure(&document, "one_phantom_block"),
        reference,
        "one pause span per comma changes the layout"
    );
}

/// Controls without the block: the line is a single inline formatting context
/// and hidden spans never split it (these already passed unpatched).
#[test]
fn typing_without_a_block_matches_the_finalized_text() {
    let finalized = format!(
        r#"<div class="box" id="final_text"><p><span>{PREV} <span class="ib">{HEY}</span>{REST}</span><span></span></p></div>"#
    );
    let body = [
        finalized,
        typing_case("chars_only", 0, false),
        typing_case("phantoms", 2, false),
    ]
    .concat();
    let document = document(&body);
    let reference = measure(&document, "final_text");
    assert_eq!(reference[0], 48.0, "finalized text takes two lines");
    assert_eq!(measure(&document, "chars_only"), reference);
    assert_eq!(measure(&document, "phantoms"), reference);
}

/// Independent block-flow case of upstream #1697: hidden children between text
/// runs of a block container keep one anonymous block (unpatched: three, one
/// line each, 96px).
#[test]
fn hidden_children_keep_one_anonymous_block_in_block_flow() {
    let document = document(
        r#"<div id="flow" style="width:420px">one, <span style="display:none"></span>two, <span hidden></span>three<div>block</div></div>"#,
    );
    let flow = id(&document, "flow");
    assert_eq!(anonymous_layout_children(&document, flow), 1);
    assert_eq!(
        document.get_node(flow).unwrap().final_layout().size.height,
        48.0,
        "one text line and one block line"
    );
}

/// Independent flex case of upstream #1697 (its own test subject): text around a
/// hidden child stays one anonymous flex item.
#[test]
fn hidden_children_keep_one_anonymous_flex_item() {
    let document = document(
        r#"<div id="flex" style="display:flex">one <span style="display:none"></span>two</div>"#,
    );
    let flex = id(&document, "flex");
    assert_eq!(anonymous_layout_children(&document, flex), 1);
}

/// Hidden children reached through `display: contents` (`push_hoisted_child`)
/// do not split the anonymous block either.
#[test]
fn hidden_children_hoisted_through_contents_keep_one_anonymous_block() {
    let document = document(
        r#"<div id="flow" style="width:420px">one <span style="display:contents">two<span style="display:none"></span> three</span><div>block</div></div>"#,
    );
    let flow = id(&document, "flow");
    assert_eq!(anonymous_layout_children(&document, flow), 1);
    assert_eq!(
        document.get_node(flow).unwrap().final_layout().size.height,
        48.0
    );
}

/// Remaining Chromium difference (upstream DioxusLabs/blitz#1434): the typed
/// "Hey" must continue the static text line, not start a new one. Chrome 157:
/// `[48, 0]` for every case; Blitz with 0008: `[48, 24]`.
#[test]
#[ignore = "block-in-inline (DioxusLabs/blitz#1434) is not implemented"]
fn block_in_inline_keeps_the_typed_text_on_the_static_line() {
    let document = document(&typing_case("block_end", 0, true));
    assert_eq!(measure(&document, "block_end"), [48.0, 0.0]);
}
