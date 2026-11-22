//! Monotonic CSS animation timeline of the page document.
//!
//! Blitz takes the animation time as an argument of `BaseDocument::resolve` /
//! `resolve_stylist` (seconds, forwarded to Stylo's
//! `SharedStyleContext::current_time_for_animations`); `blitz-shell` passes the
//! seconds of a private `Instant` read inside `View::redraw`. Every flush of one
//! document must use the same timeline, otherwise animation start times recorded
//! at one flush are compared with times from another origin. The probe therefore
//! keeps one clock per page document: every flush path (headless frames, layout
//! and style reads from JS ops) reads it, and the page document's
//! `set_animation_time_source` (vendor/blitz-dom 0007) makes blitz-shell's own
//! `resolve` read it too, ignoring the shell's time.
//!
//! The probe (like the product's single-document policy) runs exactly one page
//! document, on the thread that owns its `JsRuntime`, so the clock lives in a
//! thread-local instead of being threaded through every frame helper. Other
//! documents (the animation regression in main.rs) own a separate
//! `AnimationClock`.
//!
//! Origin: aligned once, before the first flush, with the guest's
//! `performance.now()` origin (`align_to_js`), so CSS animation times and rAF
//! timestamps share one origin (within the precision of `performance.now()`),
//! headless and in `--window` alike.
//!
//! Frame time: Chromium (f2700724, `PageAnimator::ServiceScriptedAnimations`
//! and `AnimationClock`, third_party/blink/renderer/core/{page,animation}) sets
//! the document's animation clock once at the start of a rendering update and
//! holds it until the update ends, so the `finished` promises settled while
//! updating animations and the style/paint of that frame see one time. The
//! probe's tick ends with the before-paint settlement (dom.js
//! `settleCssAnimationsBeforePaint`), so it `hold`s the clock right before it
//! and `release`s it after the paint flush (`resolve_layout` headless, the
//! shell's `RedrawRequested` in the window) or at the next tick: a paint can
//! never finish an animation whose `finished` the settlement did not see.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct AnimationClock(Rc<State>);

impl Default for AnimationClock {
    fn default() -> Self {
        Self::new()
    }
}

struct State {
    origin: Cell<Instant>,
    /// Last time handed out: `now` never goes backwards, even if the origin is
    /// re-aligned after a read.
    last: Cell<f64>,
    /// The frame time while a rendering update holds the clock (see `hold`).
    held: Cell<Option<f64>>,
}

impl AnimationClock {
    pub fn new() -> Self {
        Self(Rc::new(State {
            origin: Cell::new(Instant::now()),
            last: Cell::new(0.0),
            held: Cell::new(None),
        }))
    }

    /// Seconds since the origin, never smaller than a previous result; the
    /// held frame time while the clock is held.
    pub fn now(&self) -> f64 {
        if let Some(held) = self.0.held.get() {
            return held;
        }
        let elapsed = self.0.origin.get().elapsed().as_secs_f64();
        let now = elapsed.max(self.0.last.get());
        self.0.last.set(now);
        now
    }

    /// Freezes the timeline at its current time until `release` (the frame
    /// time of a rendering update, see the module docs) and returns it. A
    /// second `hold` keeps the first time.
    pub fn hold(&self) -> f64 {
        let now = self.now();
        self.0.held.set(Some(now));
        now
    }

    /// Ends a `hold`; the next `now` reads the real time again (never less than
    /// the held time). No-op when not held.
    pub fn release(&self) {
        self.0.held.set(None);
    }

    /// Moves the origin to the guest's `performance.now()` origin, given a
    /// `performance.now()` reading (milliseconds) taken just before the call.
    pub fn align_to_js(&self, performance_now_ms: f64) {
        if !performance_now_ms.is_finite() || performance_now_ms < 0.0 {
            return;
        }
        if let Some(origin) =
            Instant::now().checked_sub(Duration::from_secs_f64(performance_now_ms / 1000.0))
        {
            self.0.origin.set(origin);
        }
    }

    /// Moves the timeline forward by `by` without waiting (deterministic tests).
    pub fn advance(&self, by: Duration) {
        if let Some(origin) = self.0.origin.get().checked_sub(by) {
            self.0.origin.set(origin);
        }
    }
}

thread_local! {
    static PAGE_CLOCK: AnimationClock = AnimationClock::new();
}

/// The page document's clock (see the module docs).
pub fn page_clock() -> AnimationClock {
    PAGE_CLOCK.with(AnimationClock::clone)
}

/// Current time of the page document's timeline, for `resolve`/`resolve_stylist`
/// and as the page document's `set_animation_time_source`.
pub fn page_time() -> f64 {
    PAGE_CLOCK.with(AnimationClock::now)
}

#[cfg(test)]
mod tests {
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use super::*;

    #[test]
    fn hold_freezes_the_timeline_and_release_resumes_monotonically() {
        let clock = AnimationClock::new();
        let held = clock.hold();
        clock.advance(Duration::from_millis(50));
        assert_eq!(clock.now(), held, "a held clock keeps the frame time");
        assert_eq!(clock.hold(), held, "a second hold keeps the first time");
        clock.release();
        let after = clock.now();
        assert!(after >= held + 0.05, "released: {after} after {held}");
        clock.release();
        assert!(clock.now() >= after, "never backwards");
    }

    /// `opacity` of `#exit`: a 100 ms `fill-mode: none` exit animation (as
    /// tw-animate-css `animate-out`) from 0.5 to 0 over an unanimated 1.
    fn exit_opacity(document: &HtmlDocument) -> f32 {
        let node_id = crate::find_element_by_id(document, "exit").expect("#exit");
        document
            .get_node(node_id)
            .and_then(|node| node.primary_styles())
            .map(|styles| styles.get_effects().opacity)
            .expect("#exit styles")
    }

    /// vendor/blitz-dom 0007: with the page timeline installed, the paint
    /// flush (`resolve` with blitz-shell's private time, here deliberately far
    /// past the end) uses the held frame time, so an exit animation that the
    /// settlement saw running is still painted animated; after `release` the
    /// next flush finishes it.
    #[test]
    fn page_document_resolve_uses_the_held_page_time() {
        const HTML: &str = r#"<!doctype html><html><head><style>
            @keyframes exit { from { opacity: 0.5 } to { opacity: 0 } }
            #exit { height: 5px; opacity: 1; animation: exit 100ms linear; }
            </style></head><body><div id="exit"></div></body></html>"#;
        let clock = page_clock();
        let mut document = HtmlDocument::from_html(HTML, DocumentConfig::default());
        document.set_viewport(Viewport::new(320, 240, 1.0, ColorScheme::Light));
        document.set_animation_time_source(Some(page_time));
        document.resolve(1.0e6);
        assert!(
            exit_opacity(&document) <= 0.5,
            "started on the page timeline"
        );

        clock.advance(Duration::from_millis(40));
        clock.hold();
        // Wall time past the end while held, and a caller time that would end
        // the animation (it started at 1e6 if the argument were used): the
        // frame keeps its time.
        clock.advance(Duration::from_millis(200));
        document.resolve(2.0e6);
        assert!(
            exit_opacity(&document) <= 0.5,
            "the held frame paints the animation it settled as running"
        );

        clock.release();
        document.resolve(0.0);
        assert_eq!(exit_opacity(&document), 1.0, "finished after release");
    }
}
