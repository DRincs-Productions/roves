//! Monotonic CSS animation timeline of the page document.
//!
//! Blitz takes the animation time as an argument of `BaseDocument::resolve` /
//! `resolve_stylist` (seconds, forwarded to Stylo's
//! `SharedStyleContext::current_time_for_animations`); `blitz-shell` passes the
//! seconds elapsed (`Instant`) since its first redraw. Every flush of one document
//! must use the same timeline, otherwise animation start times recorded at one
//! flush are compared with times from another origin. The probe therefore keeps
//! one clock per page document and every flush path (headless frames, layout and
//! style reads from JS ops) reads it.
//!
//! The probe (like the product's single-document policy) runs exactly one page
//! document, on the thread that owns its `JsRuntime`, so the clock lives in a
//! thread-local instead of being threaded through every frame helper. Other
//! documents (the animation regression below) own a separate `AnimationClock`.
//!
//! Origins:
//! - headless: aligned once, before the first flush, with the guest's
//!   `performance.now()` origin (`align_to_js`), so CSS animation times and rAF
//!   timestamps share one origin (within the precision of `performance.now()`);
//! - `--window`: blitz-shell's private timer is authoritative for its own
//!   resolves and cannot be read, so the clock restarts at zero on the shell's
//!   first document access (`restart`, within one event-loop turn of the shell's
//!   first redraw). Animations started headless before the window opens are not
//!   carried over (the fixture has none on the page document).

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
}

impl AnimationClock {
    pub fn new() -> Self {
        Self(Rc::new(State {
            origin: Cell::new(Instant::now()),
            last: Cell::new(0.0),
        }))
    }

    /// Seconds since the origin, never smaller than a previous result.
    pub fn now(&self) -> f64 {
        let elapsed = self.0.origin.get().elapsed().as_secs_f64();
        let now = elapsed.max(self.0.last.get());
        self.0.last.set(now);
        now
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

    /// Starts a new timeline at zero (a new authoritative origin, see the module
    /// docs). The only operation that resets monotonicity.
    pub fn restart(&self) {
        self.0.origin.set(Instant::now());
        self.0.last.set(0.0);
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

/// Current time of the page document's timeline, for `resolve`/`resolve_stylist`.
pub fn page_time() -> f64 {
    PAGE_CLOCK.with(AnimationClock::now)
}
