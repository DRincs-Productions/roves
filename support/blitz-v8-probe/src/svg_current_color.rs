//! Regression for inline `<svg>` painted with `currentColor` (patched in
//! `vendor/blitz-dom`, 0006): Blitz serializes the subtree for usvg and replaces
//! `currentColor` with the element's computed `color`. Stylo serializes a
//! computed `oklch()` as `oklch(...)`, which svgtypes 0.16 cannot parse, so the
//! stroke was dropped and Lucide icons in Tailwind v4 / shadcn themes (all
//! `oklch` colors) painted nothing. A `rgb()` color is the control.

use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::{VIEWPORT, count_differing, element_rect, render_rgba};

/// Lucide `save` (lucide-react 0.5x markup), stroked with `currentColor`.
const LUCIDE_SAVE: &str = r#"<svg id="icon" xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-save" aria-hidden="true"><path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/></svg>"#;

/// Pixels of the 64x64 icon box that differ from the white page, with the
/// page `color` set to `color` (the template's preflight `svg{display:block}`
/// and a `calc(var(--spacing) * N)` size as in shadcn's button rule).
fn painted_icon_pixels(color: &str) -> usize {
    let html = format!(
        r#"<!doctype html><html><head><style>
        :root {{ --spacing: 4px; --foreground: {color}; }}
        html, body {{ margin: 0; background: rgb(255, 255, 255); color: var(--foreground); }}
        svg {{ display: block; }}
        #icon {{ width: calc(var(--spacing) * 16); height: calc(var(--spacing) * 16); }}
        </style></head><body>{LUCIDE_SAVE}</body></html>"#
    );
    let mut document = HtmlDocument::from_html(&html, DocumentConfig::default());
    document.set_viewport(Viewport::new(
        VIEWPORT.0,
        VIEWPORT.1,
        1.0,
        ColorScheme::Light,
    ));
    document.resolve(0.0);
    let document = Rc::new(RefCell::new(document));
    let rect = element_rect(&document, "icon");
    assert_eq!((rect.2, rect.3), (64.0, 64.0), "{color}: icon box size");
    let pixels = render_rgba(&document);
    count_differing(&pixels, rect, [255, 255, 255])
}

/// Control: a legacy sRGB color already painted before the patch.
#[test]
fn rgb_current_color_strokes_the_icon() {
    let painted = painted_icon_pixels("rgb(37, 37, 37)");
    assert!(painted > 200, "rgb icon painted {painted} pixels");
}

/// The template's `--foreground: oklch(14.5% 0 0)`: painted nothing unpatched
/// (CI release build of `359d0cc7bec`, offscreen screenshot).
#[test]
fn oklch_current_color_strokes_the_icon() {
    let control = painted_icon_pixels("rgb(37, 37, 37)");
    let painted = painted_icon_pixels("oklch(14.5% 0 0)");
    assert!(painted > 200, "oklch icon painted {painted} pixels");
    assert!(
        painted.abs_diff(control) * 10 <= control,
        "oklch icon painted {painted} pixels, rgb control {control}"
    );
}

/// A translucent modern color keeps its alpha through `rgba()`.
#[test]
fn translucent_oklch_current_color_strokes_the_icon() {
    let painted = painted_icon_pixels("oklch(14.5% 0 0 / 0.5)");
    assert!(
        painted > 200,
        "translucent oklch icon painted {painted} pixels"
    );
}
