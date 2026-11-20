//! Regression for a blitz-paint defect, fixed in the vendored copy for
//! `opacity: 0` only (`vendor/blitz-paint/ROVES_PATCHES.md`, change 0002):
//! `opacity` on a non-atomic inline element (`<span style="opacity: 0">text</span>`
//! in a block) did not reach its glyphs. `render.rs` applied opacity only as the
//! layer of the box being rendered, while the inline root's `draw_inline_layout`
//! -> `text::stroke_text` drew every glyph run with a fixed alpha of 1.0. Negative
//! observed on Roves release 0cdb0da9300: opacity 0 inline 1786 glyph pixels
//! (= opacity 1), opacity 0.5 inline darkest 0, computed values correct.
//!
//! Gate (CSS Color 4 `opacity`, which applies to all elements, and Chrome): an
//! inline under an `opacity: 0` non-atomic inline paints nothing, whatever the
//! nesting and however many layers the frame already used; the opacity of the
//! inline root or of an atomic inline (box opacity path) is applied once.
//!
//! Diagnostic only (not fixed, reported explicitly, never asserted as correct):
//! a non-atomic inline with `0 < opacity < 1`, nested partial opacities and a
//! decoration inside a partial group still paint at alpha 1 in Blitz; the
//! diagnostics record the measured darkness next to the spec value so the gap
//! stays visible, and only assert that the text is painted (not culled).
//!
//! The test separates computed style from painted pixels: Stylo's computed
//! `opacity` (the value `getComputedStyle` returns) must be right in every
//! case. The right part of every cell is a background-only control. The fixture
//! is static and is resolved and rendered twice at the same timeline time: both
//! buffers must be identical, so a failure is not frame or timing dependent.
//!
//! Shared with the published executable through `fixtures/inline-opacity`
//! (`check_png.py` applies the same oracle and diagnostics).

use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use style::properties::{PropertyDeclarationId, PropertyId};

use crate::{VIEWPORT, count_differing, find_element_by_id, pixel, render_rgba};

const FIXTURE: &str = include_str!("../fixtures/inline-opacity/index.html");
const PAGE: [u8; 3] = [255, 255, 255];
/// Text region (x +0..+100) and background-only control region (x +110..+140)
/// of a 140px wide cell (CSS px, DPR 1).
const TEXT_WIDTH: f32 = 100.0;
const CONTROL_OFFSET: f32 = 110.0;
const CONTROL_WIDTH: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Expect {
    /// Opaque black glyph cores.
    Opaque,
    /// Black at alpha 0.5 over white: darkest about 128 (box opacity path).
    Half,
    /// Nothing painted.
    Nothing,
}

impl Expect {
    /// Darkest channel range; antialiasing only lightens.
    fn darkest(self) -> std::ops::RangeInclusive<u8> {
        match self {
            Expect::Opaque => 0..=40,
            Expect::Half => 100..=170,
            Expect::Nothing => 255..=255,
        }
    }
}

/// Gate (case, left, top, height, expectation): same table as `check_png.py`.
const CASES: [(&str, f32, f32, f32, Expect); 7] = [
    ("visible", 20.0, 20.0, 60.0, Expect::Opaque),
    ("hidden", 175.0, 20.0, 60.0, Expect::Nothing),
    ("atomic", 485.0, 20.0, 60.0, Expect::Nothing),
    ("atomic-half", 20.0, 100.0, 60.0, Expect::Half),
    ("ancestor", 330.0, 100.0, 60.0, Expect::Half),
    ("stagger-1", 175.0, 180.0, 50.0, Expect::Opaque),
    ("stagger-3", 175.0, 280.0, 50.0, Expect::Nothing),
];

/// Diagnostics (case, left, top, height, spec alpha): partial opacity on
/// non-atomic inlines, not implemented. Same table as `check_png.py`.
const PARTIAL: [(&str, f32, f32, f32, f32); 5] = [
    ("half", 330.0, 20.0, 60.0, 0.5),
    ("nested", 175.0, 100.0, 60.0, 0.25),
    ("ancestor-nested", 485.0, 100.0, 60.0, 0.25),
    ("decor", 20.0, 180.0, 60.0, 0.5),
    ("stagger-2", 175.0, 230.0, 50.0, 0.5),
];

/// (cell id, left, top, height): fixed geometry the regions above rely on.
const CELLS: [(&str, f32, f32, f32); 10] = [
    ("visible-cell", 20.0, 20.0, 60.0),
    ("hidden-cell", 175.0, 20.0, 60.0),
    ("half-cell", 330.0, 20.0, 60.0),
    ("atomic-cell", 485.0, 20.0, 60.0),
    ("atomic-half-cell", 20.0, 100.0, 60.0),
    ("nested-cell", 175.0, 100.0, 60.0),
    ("ancestor-cell", 330.0, 100.0, 60.0),
    ("ancestor-nested-cell", 485.0, 100.0, 60.0),
    ("decor-cell", 20.0, 180.0, 60.0),
    ("stagger-cell", 175.0, 180.0, 150.0),
];

/// Element id -> computed opacity expected from Stylo (same as `check_png.py`).
const COMPUTED: [(&str, &str); 15] = [
    ("visible", "1"),
    ("hidden", "0"),
    ("half", "0.5"),
    ("atomic", "0"),
    ("atomic-half", "0.5"),
    ("nested-outer", "0.5"),
    ("nested", "0.5"),
    ("ancestor-cell", "0.5"),
    ("ancestor", "1"),
    ("ancestor-nested-cell", "0.5"),
    ("ancestor-nested", "0.5"),
    ("decor", "0.5"),
    ("stagger-1", "1"),
    ("stagger-2", "0.5"),
    ("stagger-3", "0"),
];

fn document_at(html: &str, time: f64) -> Rc<RefCell<HtmlDocument>> {
    let mut document = HtmlDocument::from_html(html, DocumentConfig::default());
    document.set_viewport(Viewport::new(
        VIEWPORT.0,
        VIEWPORT.1,
        1.0,
        ColorScheme::Light,
    ));
    document.resolve(time);
    Rc::new(RefCell::new(document))
}

/// Stylo's computed `opacity` serialized as `getComputedStyle` does
/// (`op_blitz_computed_style`).
fn computed_opacity(document: &Rc<RefCell<HtmlDocument>>, id: &str) -> String {
    let document = document.borrow();
    let node_id = find_element_by_id(&document, id).expect("fixture element");
    let styles = document
        .get_node(node_id)
        .unwrap()
        .primary_styles()
        .expect("fixture element is styled");
    let property = PropertyId::parse_enabled_for_all_content("opacity").unwrap();
    let longhand = property.longhand_id().expect("opacity is a longhand");
    styles.computed_value_to_string(PropertyDeclarationId::Longhand(longhand))
}

/// Darkest channel value in `rect`: 0 for opaque black glyph cores, about 128
/// for black at opacity 0.5 over white, 255 when nothing is painted.
fn darkest(pixels: &[u8], rect: (f32, f32, f32, f32)) -> u8 {
    let (x0, y0, width, height) = rect;
    let mut darkest = u8::MAX;
    for y in y0 as usize..(y0 + height) as usize {
        for x in x0 as usize..(x0 + width) as usize {
            let [r, g, b, _] = pixel(pixels, x as f32, y as f32);
            darkest = darkest.min(r).min(g).min(b);
        }
    }
    darkest
}

#[derive(Debug)]
struct Measured {
    id: &'static str,
    glyph_pixels: usize,
    darkest: u8,
    control_pixels: usize,
}

impl Measured {
    fn take(pixels: &[u8], id: &'static str, rect: (f32, f32, f32)) -> Self {
        let (left, top, height) = rect;
        let text = (left, top, TEXT_WIDTH, height);
        let control = (left + CONTROL_OFFSET, top, CONTROL_WIDTH, height);
        Measured {
            id,
            glyph_pixels: count_differing(pixels, text, PAGE),
            darkest: darkest(pixels, text),
            control_pixels: count_differing(pixels, control, PAGE),
        }
    }

    /// Gate check: `None` when the measurement matches `expect`.
    fn failure(&self, expect: Expect) -> Option<String> {
        if self.control_pixels != 0 {
            return Some(format!("#{}: background control region painted", self.id));
        }
        let ok = match expect {
            Expect::Nothing => self.glyph_pixels == 0,
            expect => self.glyph_pixels > 100 && expect.darkest().contains(&self.darkest),
        };
        (!ok).then(|| {
            format!(
                "#{}: expected {:?}, {} glyph pixels, darkest {}",
                self.id, expect, self.glyph_pixels, self.darkest
            )
        })
    }

    /// Diagnostic check for a partial opacity that Blitz does not implement:
    /// only that the text is painted (not culled) and the control is clean.
    fn culled_or_dirty(&self) -> Option<String> {
        if self.control_pixels != 0 {
            return Some(format!("#{}: background control region painted", self.id));
        }
        (self.glyph_pixels <= 100).then(|| {
            format!(
                "#{}: partial opacity culled ({} glyph pixels)",
                self.id, self.glyph_pixels
            )
        })
    }
}

/// Explicit diagnostic line for a partial opacity case: measured darkness next
/// to the spec value (black at `alpha` over white).
fn partial_diagnostic(measured: &Measured, alpha: f32) -> String {
    let spec = (255.0 * (1.0 - alpha)).round() as u8;
    format!(
        "#{}: spec alpha {alpha} (darkest ~{spec}), measured darkest {}, {}",
        measured.id,
        measured.darkest,
        if measured.darkest <= 40 {
            "painted at alpha 1 (known gap)"
        } else {
            "partially transparent"
        }
    )
}

#[test]
fn inline_zero_opacity_is_culled_and_box_opacity_applied_once() {
    // Fixed timeline time: the page has no animation, so this is its only state.
    let document = document_at(FIXTURE, 0.0);
    let first = render_rgba(&document);
    // Same state, second frame: re-resolve at the same time and repaint.
    document.borrow_mut().resolve(0.0);
    let second = render_rgba(&document);
    assert!(
        first == second,
        "the static fixture must paint identical frames at the same time"
    );
    if let Ok(path) = std::env::var("ROVES_PROBE_INLINE_OPACITY_PNG") {
        crate::write_png(&path, &first);
    }

    // The fixture's absolute geometry must hold, or the regions would not
    // cover the text.
    for (id, left, top, height) in CELLS {
        let cell = crate::element_rect(&document, id);
        assert_eq!(
            (cell.0, cell.1, cell.2, cell.3),
            (left, top, 140.0, height),
            "#{id} geometry"
        );
    }
    // Computed style is right in every case (it was before the paint fix too).
    for (id, expected) in COMPUTED {
        assert_eq!(
            computed_opacity(&document, id),
            expected,
            "computed opacity of #{id}"
        );
    }

    let measured: Vec<(Measured, Expect)> = CASES
        .iter()
        .map(|&(id, left, top, height, expect)| {
            (Measured::take(&first, id, (left, top, height)), expect)
        })
        .collect();
    let evidence = format!("{measured:#?}");
    eprintln!("INLINE_OPACITY_EVIDENCE {evidence}");
    let failures: Vec<String> = measured
        .iter()
        .filter_map(|(m, expect)| m.failure(*expect))
        .collect();
    assert!(
        failures.is_empty(),
        "inline opacity paint mismatches: {failures:#?}\n{evidence}"
    );
}

/// Explicit diagnostic for the unimplemented partial cases of the same fixture.
/// Prints `INLINE_OPACITY_PARTIAL_DIAGNOSTIC` with measured vs spec darkness;
/// fails only if such text is culled or a control region is painted, never on
/// the alpha itself (see `ROVES_PATCHES.md`, 0002, known gaps).
#[test]
fn inline_partial_opacity_diagnostic() {
    let document = document_at(FIXTURE, 0.0);
    let pixels = render_rgba(&document);
    let measured: Vec<(Measured, f32)> = PARTIAL
        .iter()
        .map(|&(id, left, top, height, alpha)| {
            (Measured::take(&pixels, id, (left, top, height)), alpha)
        })
        .collect();
    let lines: Vec<String> = measured
        .iter()
        .map(|(m, alpha)| partial_diagnostic(m, *alpha))
        .collect();
    eprintln!("INLINE_OPACITY_PARTIAL_DIAGNOSTIC {lines:#?}");
    let failures: Vec<String> = measured
        .iter()
        .filter_map(|(m, _)| m.culled_or_dirty())
        .collect();
    assert!(
        failures.is_empty(),
        "partial inline opacity diagnostic: {failures:#?}\n{lines:#?}"
    );
}

/// Zero-opacity culling must not depend on `LayerManager`'s per-frame layer
/// limit (1024): 1100 `inline-block`s at opacity 0.5 (each a box opacity layer)
/// are painted first, then 1200 per-character `opacity: 0` spans, an
/// `opacity: 0` span inside an `opacity: 0.5` one, and an `opacity: 1` span
/// inside an `opacity: 0` one: none of them may paint. A visible control painted
/// after them proves that text is still drawn.
#[test]
fn inline_zero_opacity_is_culled_past_the_layer_limit() {
    let mut html = String::from(
        r#"<!doctype html><html><head><style>
        html, body { margin: 0; background: rgb(255, 255, 255); overflow: hidden; }
        #exhaust { position: absolute; left: 0; top: 330px; width: 640px; height: 30px;
                   font-size: 0; line-height: 0; }
        #exhaust i { display: inline-block; width: 1px; height: 1px; opacity: 0.5;
                     background: rgb(0, 0, 255); }
        #many { position: absolute; left: 20px; top: 20px; width: 600px; height: 260px;
                font-family: monospace; font-size: 10px; line-height: 10px;
                color: rgb(0, 0, 0); }
        .cell { position: absolute; top: 285px; width: 140px; height: 40px;
                font-family: monospace; font-size: 30px; line-height: 40px;
                color: rgb(0, 0, 0); white-space: nowrap; }
        #nested-zero-cell { left: 20px; }
        #control-cell { left: 300px; }
        </style></head><body><div id="exhaust">"#,
    );
    for _ in 0..1100 {
        html.push_str("<i></i>");
    }
    html.push_str(r#"</div><div id="many">"#);
    for _ in 0..1200 {
        html.push_str(r#"<span style="opacity: 0">M</span> "#);
    }
    html.push_str(
        r#"</div><div id="nested-zero-cell" class="cell"><span style="opacity: 0.5"><span
        style="opacity: 0">MM</span></span><span style="opacity: 0"><span
        style="opacity: 1">MM</span></span></div><div id="control-cell"
        class="cell"><span>MM</span></div></body></html>"#,
    );

    let document = document_at(&html, 0.0);
    let pixels = render_rgba(&document);
    let many = count_differing(&pixels, (20.0, 20.0, 600.0, 260.0), PAGE);
    let nested = count_differing(&pixels, (20.0, 285.0, 140.0, 40.0), PAGE);
    let control = Measured::take(&pixels, "control", (300.0, 285.0, 40.0));
    eprintln!(
        "INLINE_OPACITY_LAYER_LIMIT_EVIDENCE many {many}, nested {nested}, control {control:?}"
    );
    assert_eq!(
        many, 0,
        "opacity 0 per-character spans painted past the layer limit"
    );
    assert_eq!(
        nested, 0,
        "nested opacity 0 inline painted past the layer limit"
    );
    assert_eq!(
        control.failure(Expect::Opaque),
        None,
        "visible control after the layer limit"
    );
}

/// A staggered reveal as games write it: sibling inline spans in one block,
/// each fading in with a CSS animation and an increasing `animation-delay`.
/// Sampled at a fixed timeline time 1.5 s after the start, the first span has
/// finished (1), the second is halfway (0.5) and the third has not started
/// (0, `both` fill). Gate: the first is opaque and the third paints nothing.
/// The second is an explicit diagnostic (partial opacity not implemented).
const STAGGER: &str = r#"<!doctype html><html><head><style>
    @keyframes roves-reveal { from { opacity: 0 } to { opacity: 1 } }
    html, body { margin: 0; background: rgb(255, 255, 255); overflow: hidden; }
    #cell { position: absolute; left: 20px; top: 20px; width: 140px; height: 150px;
            font-family: monospace; font-size: 40px; line-height: 50px;
            color: rgb(0, 0, 0); white-space: nowrap; }
    #cell span { animation: roves-reveal 1s linear both; }
    </style></head><body><div id="cell"><span id="reveal-1">MM</span><br><span
    id="reveal-2" style="animation-delay: 1s">MM</span><br><span id="reveal-3"
    style="animation-delay: 2s">MM</span></div></body></html>"#;

#[test]
fn staggered_css_reveal_culls_spans_not_yet_started() {
    const START: f64 = 1.0;
    let document = document_at(STAGGER, START);
    document.borrow_mut().resolve(START + 1.5);
    let pixels = render_rgba(&document);

    let computed: Vec<(&str, f32)> = ["reveal-1", "reveal-2", "reveal-3"]
        .into_iter()
        .map(|id| {
            let value = computed_opacity(&document, id);
            (id, value.parse().unwrap_or(f32::NAN))
        })
        .collect();
    // Animation sampling first, so a failure there is not reported as paint.
    for ((id, value), expected) in computed.iter().zip([1.0, 0.5, 0.0]) {
        assert!(
            (value - expected).abs() <= 0.02,
            "animated computed opacity of #{id}: {computed:?}"
        );
    }

    let gate = [
        (
            Measured::take(&pixels, "reveal-1", (20.0, 20.0, 50.0)),
            Expect::Opaque,
        ),
        (
            Measured::take(&pixels, "reveal-3", (20.0, 120.0, 50.0)),
            Expect::Nothing,
        ),
    ];
    let partial = Measured::take(&pixels, "reveal-2", (20.0, 70.0, 50.0));
    let diagnostic = partial_diagnostic(&partial, 0.5);
    let evidence = format!("{gate:#?}");
    eprintln!("INLINE_OPACITY_STAGGER_EVIDENCE {computed:?} {evidence}");
    eprintln!("INLINE_OPACITY_PARTIAL_DIAGNOSTIC {diagnostic}");
    let mut failures: Vec<String> = gate
        .iter()
        .filter_map(|(m, expect)| m.failure(*expect))
        .collect();
    failures.extend(partial.culled_or_dirty());
    assert!(
        failures.is_empty(),
        "staggered reveal paint mismatches: {failures:#?}\n{evidence}\n{diagnostic}"
    );
}
