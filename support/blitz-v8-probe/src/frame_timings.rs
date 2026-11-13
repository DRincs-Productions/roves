//! Opt-in frame timing diagnostics for the probe window
//! (`ROVES_PROBE_FRAME_TIMINGS=1`).
//!
//! Without the variable nothing is allocated or measured: the window and the
//! GPU renderer only check an `Option`. With it, three series are sampled into
//! bounded ring buffers (the last `MAX_SAMPLES` values, about 60 seconds at
//! 60 Hz) and one `ROVES_PROBE_FRAME_TIMINGS` JSON record is printed when the
//! window closes:
//!
//! - `frame_step_interval`: wall-clock time between the starts of consecutive
//!   JS/deno_core frame steps (`RovesDocument::poll`).
//! - `cpu_frame_step`: wall-clock duration of one frame step on the shell
//!   thread (tokio yield + `frame_step`: timers, rAF callbacks, microtasks).
//! - `cpu_render_present`: wall-clock duration of `GpuWindowRenderer::render`
//!   on the shell thread (Blitz scene paint, split, Vello CPU raster, uploads,
//!   compose submit and the `surface_present` call). It is NOT GPU execution
//!   time and does not prove that a frame became visible on screen: the queue
//!   runs asynchronously and the compositor/display timing is not observed.
//!
//! `std::time::Instant` and `Vec` are enough; no tracing/profiling dependency.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Samples kept per series: about 60 seconds at 60 Hz.
pub const MAX_SAMPLES: usize = 3600;

/// The 60 Hz frame budget, in milliseconds.
const BUDGET_MS: f64 = 16.67;

/// A bounded ring buffer of durations in milliseconds.
pub struct Samples {
    values: Vec<f64>,
    capacity: usize,
    next: usize,
    /// All samples seen, including those overwritten.
    total: u64,
}

impl Samples {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            values: Vec::with_capacity(capacity),
            capacity,
            next: 0,
            total: 0,
        }
    }

    pub fn push(&mut self, duration: Duration) {
        self.push_ms(duration.as_secs_f64() * 1000.0);
    }

    fn push_ms(&mut self, value: f64) {
        if self.values.len() < self.capacity {
            self.values.push(value);
        } else {
            self.values[self.next] = value;
        }
        self.next = (self.next + 1) % self.capacity;
        self.total += 1;
    }

    pub fn summary(&self) -> Summary {
        let mut sorted = self.values.clone();
        sorted.sort_by(f64::total_cmp);
        Summary {
            count: sorted.len(),
            total: self.total,
            p50: percentile(&sorted, 50.0),
            p95: percentile(&sorted, 95.0),
            p99: percentile(&sorted, 99.0),
            max: sorted.last().copied(),
            over_budget: sorted.iter().filter(|value| **value > BUDGET_MS).count(),
        }
    }
}

/// Nearest-rank percentile of an ascending slice (`None` when empty).
fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

#[derive(Debug, PartialEq)]
pub struct Summary {
    /// Samples in the window (at most `MAX_SAMPLES`).
    pub count: usize,
    /// Samples seen since the start.
    pub total: u64,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub max: Option<f64>,
    /// Samples in the window above 16.67 ms.
    pub over_budget: usize,
}

impl Summary {
    fn json(&self) -> String {
        let number = |value: Option<f64>| match value {
            Some(value) => format!("{value:.3}"),
            None => "null".to_owned(),
        };
        format!(
            "{{\"count\":{},\"total\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"max\":{},\"over_16_67ms\":{}}}",
            self.count,
            self.total,
            number(self.p50),
            number(self.p95),
            number(self.p99),
            number(self.max),
            self.over_budget,
        )
    }
}

pub struct FrameTimings {
    pub frame_step_interval: Samples,
    pub cpu_frame_step: Samples,
    pub cpu_render_present: Samples,
    /// `"gpu"` (`GpuWindowRenderer`, render timings sampled) or `"cpu"`.
    pub renderer: &'static str,
    /// The GPU compositor's raster cache was disabled
    /// (`ROVES_PROBE_DISABLE_RASTER_CACHE=1`); `None` for the CPU renderer.
    pub raster_cache_disabled: Option<bool>,
    printed: bool,
}

pub type SharedFrameTimings = Rc<RefCell<FrameTimings>>;

impl FrameTimings {
    fn new() -> Self {
        Self {
            frame_step_interval: Samples::with_capacity(MAX_SAMPLES),
            cpu_frame_step: Samples::with_capacity(MAX_SAMPLES),
            cpu_render_present: Samples::with_capacity(MAX_SAMPLES),
            renderer: "cpu",
            raster_cache_disabled: None,
            printed: false,
        }
    }

    /// `Some` only with `ROVES_PROBE_FRAME_TIMINGS=1`.
    pub fn from_env() -> Option<SharedFrameTimings> {
        std::env::var("ROVES_PROBE_FRAME_TIMINGS")
            .is_ok_and(|value| value == "1")
            .then(|| Rc::new(RefCell::new(Self::new())))
    }

    /// Prints the single `ROVES_PROBE_FRAME_TIMINGS` record (later calls are
    /// no-ops). `stats` are the GPU presentation counters, if any.
    pub fn print_once(&mut self, stats: Option<&crate::presentation::PresentStats>) {
        if self.printed {
            return;
        }
        self.printed = true;
        let raster_cache = match self.raster_cache_disabled {
            Some(true) => "\"disabled\"",
            Some(false) => "\"enabled\"",
            None => "null",
        };
        let counters = match stats {
            Some(stats) => format!(
                "\"presented_frames\":{},\"raster_segments\":{},\"raster_uploads\":{},\"raster_cache_hits\":{},\"upload_bytes\":{},\"skipped_frames\":{}",
                stats.frames,
                stats.raster_segments,
                stats.raster_uploads,
                stats.raster_cache_hits,
                stats.upload_bytes,
                stats.skipped_frames,
            ),
            None => "\"presented_frames\":null".to_owned(),
        };
        println!(
            "ROVES_PROBE_FRAME_TIMINGS {{\"check\":\"frame_timings\",\"unit\":\"ms\",\"window_max_samples\":{MAX_SAMPLES},\"renderer\":\"{}\",\"raster_cache\":{raster_cache},{counters},\"frame_step_interval\":{},\"cpu_frame_step\":{},\"cpu_render_present\":{},\"note\":\"cpu_* are shell-thread wall-clock durations; cpu_render_present is not GPU time and does not prove a visible frame\"}}",
            self.renderer,
            self.frame_step_interval.summary().json(),
            self.cpu_frame_step.summary().json(),
            self.cpu_render_present.summary().json(),
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let mut samples = Samples::with_capacity(200);
        // 1..=100 ms, pushed out of order.
        for value in (1..=100).rev() {
            samples.push_ms(f64::from(value));
        }
        let summary = samples.summary();
        assert_eq!(summary.count, 100);
        assert_eq!(summary.total, 100);
        assert_eq!(summary.p50, Some(50.0));
        assert_eq!(summary.p95, Some(95.0));
        assert_eq!(summary.p99, Some(99.0));
        assert_eq!(summary.max, Some(100.0));
        // 17..=100 are above 16.67 ms.
        assert_eq!(summary.over_budget, 84);

        let mut single = Samples::with_capacity(4);
        single.push(Duration::from_millis(20));
        let summary = single.summary();
        assert_eq!(
            (summary.p50, summary.p99, summary.over_budget),
            (Some(20.0), Some(20.0), 1)
        );
    }

    #[test]
    fn empty_series_report_null() {
        let summary = Samples::with_capacity(4).summary();
        assert_eq!(summary.count, 0);
        assert_eq!((summary.p50, summary.max), (None, None));
        assert!(summary.json().contains("\"p99\":null"));
    }

    #[test]
    fn the_window_keeps_only_the_latest_samples() {
        let mut samples = Samples::with_capacity(3);
        for value in [100.0, 1.0, 2.0, 3.0, 4.0] {
            samples.push_ms(value);
        }
        let summary = samples.summary();
        // 100 and 1 were overwritten; total still counts them.
        assert_eq!(summary.count, 3);
        assert_eq!(summary.total, 5);
        assert_eq!(summary.max, Some(4.0));
        assert_eq!(summary.p50, Some(3.0));
        assert_eq!(summary.over_budget, 0);
        assert_eq!(MAX_SAMPLES, 3600);
    }
}
