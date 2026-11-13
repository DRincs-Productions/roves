//! Regression for Blitz #1493 (patched in `vendor/blitz-dom` and
//! `vendor/blitz-paint`): a positioned box earlier in tree order with
//! `z-index: 0` or `auto` must paint and hit-test below a later positioned box
//! of the same stacking context (CSS 2.1 Appendix E, step 8), also when the
//! later box is nested in another positioned or static box. Painting
//! (`blitz_paint::paint_scene`) and hit testing (`element_from_point`, the
//! `EventDriver` target) must agree.

use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::{VIEWPORT, assert_color, find_element_by_id, hit_target, pixel, render_rgba};

const OVERLAY: [u8; 3] = [200, 30, 30];
const TARGET: [u8; 3] = [30, 160, 60];
const PAGE: [u8; 3] = [255, 255, 255];

fn document(body: &str) -> Rc<RefCell<HtmlDocument>> {
    let html = format!(
        r#"<!doctype html><html><head><style>
        html, body {{ margin: 0; background: rgb(255, 255, 255); }}
        #overlay {{ background: rgb(200, 30, 30); }}
        #target {{ width: 100px; height: 50px; background: rgb(30, 160, 60); }}
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
    Rc::new(RefCell::new(document))
}

/// The target's centre (it is laid out at (100, 100)) hits and shows the
/// target; a point away from it hits and shows the full-viewport overlay.
fn assert_target_above_overlay(case: &str, body: &str) {
    let document = document(body);
    let (target, overlay) = {
        let document = document.borrow();
        (
            find_element_by_id(&document, "target").expect("#target"),
            find_element_by_id(&document, "overlay").expect("#overlay"),
        )
    };
    assert_eq!(
        hit_target(&document, 150.0, 125.0),
        Some(target),
        "{case}: the later positioned target must be hit above the earlier overlay"
    );
    assert_eq!(
        hit_target(&document, 400.0, 300.0),
        Some(overlay),
        "{case}: the overlay must still be hit outside the target"
    );
    let pixels = render_rgba(&document);
    assert_color(
        pixel(&pixels, 150.0, 125.0),
        TARGET,
        &format!("{case}: target pixel"),
    );
    assert_color(
        pixel(&pixels, 400.0, 300.0),
        OVERLAY,
        &format!("{case}: overlay pixel"),
    );
}

/// A (control, passed before the patch): the later box is an absolutely
/// positioned sibling of the overlay.
#[test]
fn a_later_absolute_sibling_is_above_a_fixed_overlay() {
    assert_target_above_overlay(
        "A",
        r#"<div id="overlay" style="position: fixed; inset: 0; z-index: 0"></div>
        <div id="target" style="position: absolute; left: 100px; top: 100px"></div>"#,
    );
}

/// B (#1493, the template's shape): an absolute layer holds a `z-index: 0`
/// fixed overlay and a later absolute screen whose in-flow content is the
/// target. Blitz painted the screen inside the layer, below the overlay that
/// was hoisted to the root after it. As in the template (Pixi'VN HTML layer),
/// the layer has `pointer-events: none`, the overlay and the target `auto`.
#[test]
fn b_z_index_0_overlay_stays_below_a_later_nested_positioned_box() {
    assert_target_above_overlay(
        "B",
        r#"<div style="position: absolute; left: 0; top: 0; width: 640px; height: 360px;
                      pointer-events: none">
          <div id="overlay" style="position: fixed; inset: 0; z-index: 0; pointer-events: auto"></div>
          <div style="position: absolute; width: 100%; height: 100%">
            <div style="height: 100px"></div>
            <div id="target" style="margin-left: 100px; pointer-events: auto"></div>
          </div>
        </div>"#,
    );
}

/// C (#1493, `z-index: auto`): the overlay sits in a static wrapper and the
/// target is a relatively positioned box inside a later static sibling.
#[test]
fn c_z_index_auto_overlay_stays_below_a_later_relative_box() {
    assert_target_above_overlay(
        "C",
        r#"<div><div id="overlay" style="position: fixed; inset: 0"></div></div>
        <div style="padding-top: 100px">
          <div id="target" style="position: relative; left: 100px"></div>
        </div>"#,
    );
}

/// Positioned boxes are not lifted past a box that clips them: the clip of an
/// `overflow: hidden` ancestor still applies to its relative descendant.
#[test]
fn positioned_boxes_stay_clipped_by_an_overflow_ancestor() {
    let document = document(
        r#"<div style="overflow: hidden; width: 50px; height: 50px">
          <div id="target" style="position: relative; width: 200px; height: 200px"></div>
        </div>"#,
    );
    let target = find_element_by_id(&document.borrow(), "target").expect("#target");
    let pixels = render_rgba(&document);
    assert_color(
        pixel(&pixels, 25.0, 25.0),
        TARGET,
        "clipped box inside its clip",
    );
    assert_color(
        pixel(&pixels, 150.0, 150.0),
        PAGE,
        "clipped box outside its clip",
    );
    assert_eq!(hit_target(&document, 25.0, 25.0), Some(target));
}
