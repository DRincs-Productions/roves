//! Optional host-thread stage timings (`--roves-timings`), std only.
//!
//! Every sample is a wall-clock duration read from the monotonic `Instant` clock on the winit
//! thread around one host call: it includes any blocking wait inside that call (welding's
//! 1-texel D3D12 copy wait in `acquire_frame`, swap-chain throttling in
//! `get_current_texture`), it is not CPU time of the process and it is not GPU execution time.
//! Samples are kept exactly (bounded per stage) and summarised once per phase with
//! nearest-rank percentiles, then cleared. Pure std so the module also builds stand-alone:
//! `rustc --edition 2024 --test src/timings.rs`.

use std::time::{Duration, Instant};

/// Samples kept per stage and phase; further samples are counted as dropped.
const MAX_SAMPLES: usize = 1 << 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stage {
    /// Draining CEF navigation/title events (`poll_web_event` until empty).
    Events,
    /// `acquire_frame` that returned a frame: mailbox take, grafting's D3D12 open/wrap and
    /// welding's 1-texel copy + wait.
    Import,
    /// Encoding the composition blit.
    Compose,
    /// `queue.submit` of the composition blit.
    ComposeSubmit,
    /// `surface.get_current_texture` (may block on the frame-latency limit / vsync).
    Acquire,
    /// Encoding the composed -> surface blit.
    PresentEncode,
    /// `queue.submit` of the surface blit.
    Submit,
    /// `pre_present_notify` + `SurfaceTexture::present`.
    Present,
    /// Whole sampled redraw, from its start to just after `present` (verification excluded).
    Frame,
    /// Between two consecutive successful presents.
    PresentInterval,
    /// Between two consecutive imported producer frames.
    ImportInterval,
}

impl Stage {
    pub(crate) const ALL: [Stage; 11] = [
        Stage::Events,
        Stage::Import,
        Stage::Compose,
        Stage::ComposeSubmit,
        Stage::Acquire,
        Stage::PresentEncode,
        Stage::Submit,
        Stage::Present,
        Stage::Frame,
        Stage::PresentInterval,
        Stage::ImportInterval,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Stage::Events => "events",
            Stage::Import => "import",
            Stage::Compose => "compose",
            Stage::ComposeSubmit => "compose-submit",
            Stage::Acquire => "acquire",
            Stage::PresentEncode => "present-encode",
            Stage::Submit => "submit",
            Stage::Present => "present",
            Stage::Frame => "frame",
            Stage::PresentInterval => "present-interval",
            Stage::ImportInterval => "import-interval",
        }
    }
}

/// Nearest-rank summary in microseconds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Summary {
    pub count: usize,
    pub min: u32,
    pub p50: u32,
    pub p95: u32,
    pub p99: u32,
    pub max: u32,
    pub mean: u64,
}

/// Sorts `samples` in place; `None` when empty.
pub(crate) fn summarize(samples: &mut [u32]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let n = samples.len();
    // Nearest rank: the smallest sample with at least p% of the samples at or below it.
    let rank = |p: usize| samples[(n * p).div_ceil(100).clamp(1, n) - 1];
    let sum: u64 = samples.iter().map(|&v| u64::from(v)).sum();
    Some(Summary {
        count: n,
        min: samples[0],
        p50: rank(50),
        p95: rank(95),
        p99: rank(99),
        max: samples[n - 1],
        mean: sum / n as u64,
    })
}

fn micros(d: Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
}

/// Counters of the current (not yet reported) window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Totals {
    pub presented: u64,
    pub imported: u64,
    pub superseded: u64,
    pub dropped: u64,
}

#[derive(Default)]
pub(crate) struct Timings {
    samples: [Vec<u32>; Stage::ALL.len()],
    dropped: u64,
    /// Producer paints replaced in welding's single-slot mailbox before the host took them
    /// (gaps in the paint generation counter; a failed welding copy also leaves a gap and is
    /// logged by welding).
    superseded: u64,
    empty_polls: u64,
    presented: u64,
    imported: u64,
    first_sample: Option<Instant>,
    last_present: Option<Instant>,
    last_import: Option<Instant>,
    last_generation: Option<u64>,
}

impl Timings {
    pub(crate) fn record(&mut self, stage: Stage, duration: Duration) {
        self.first_sample.get_or_insert_with(Instant::now);
        let samples = &mut self.samples[stage as usize];
        if samples.len() < MAX_SAMPLES {
            samples.push(micros(duration));
        } else {
            self.dropped += 1;
        }
    }

    /// Records `stage` as the time elapsed since `since` and returns now.
    pub(crate) fn lap(&mut self, stage: Stage, since: Instant) -> Instant {
        let now = Instant::now();
        self.record(stage, now - since);
        now
    }

    pub(crate) fn presented(&mut self, at: Instant) {
        self.presented += 1;
        if let Some(last) = self.last_present.replace(at) {
            self.record(Stage::PresentInterval, at - last);
        }
    }

    pub(crate) fn imported(&mut self, at: Instant, generation: u64) {
        self.imported += 1;
        if let Some(last) = self.last_import.replace(at) {
            self.record(Stage::ImportInterval, at - last);
        }
        if let Some(last) = self.last_generation.replace(generation) {
            self.superseded += generation.saturating_sub(last).saturating_sub(1);
        }
    }

    pub(crate) fn totals(&self) -> Totals {
        Totals {
            presented: self.presented,
            imported: self.imported,
            superseded: self.superseded,
            dropped: self.dropped,
        }
    }

    pub(crate) fn empty_poll(&mut self) {
        self.empty_polls += 1;
    }

    /// Forgets the previous present/import instants so that the next interval does not span
    /// excluded work (verification readback, surface reconfiguration, loading).
    pub(crate) fn break_intervals(&mut self) {
        self.last_present = None;
        self.last_import = None;
    }

    /// Emits one header line and one line per non-empty stage, then clears everything.
    pub(crate) fn report(&mut self, phase: &str, mut emit: impl FnMut(String)) {
        let window_ms = self.first_sample.map_or(0, |t| t.elapsed().as_millis());
        emit(format!(
            "timings phase={phase} clock=wall-monotonic unit=us window_ms={window_ms} \
             presented={} imported={} superseded_producer_frames={} empty_acquire_polls={} \
             dropped_samples={} excluded=loading,verification-readback,closing percentile=nearest-rank",
            self.presented, self.imported, self.superseded, self.empty_polls, self.dropped
        ));
        for stage in Stage::ALL {
            if let Some(s) = summarize(&mut self.samples[stage as usize]) {
                emit(format!(
                    "timings phase={phase} stage={} count={} min={} p50={} p95={} p99={} max={} mean={}",
                    stage.name(),
                    s.count,
                    s.min,
                    s.p50,
                    s.p95,
                    s.p99,
                    s.max,
                    s.mean
                ));
            }
        }
        *self = Timings::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let mut v: Vec<u32> = (1..=100).rev().collect();
        let s = summarize(&mut v).unwrap();
        assert_eq!((s.min, s.p50, s.p95, s.p99, s.max), (1, 50, 95, 99, 100));
        assert_eq!(s.mean, 50);
        assert_eq!(summarize(&mut []), None);
        let s = summarize(&mut [7]).unwrap();
        assert_eq!((s.p50, s.p99, s.count), (7, 7, 1));
        let s = summarize(&mut [10, 20, 30]).unwrap();
        assert_eq!((s.p50, s.p95), (20, 30));
    }

    #[test]
    fn generation_gaps_and_interval_breaks() {
        let mut t = Timings::default();
        let t0 = Instant::now();
        t.imported(t0, 4);
        t.imported(t0 + Duration::from_millis(16), 7);
        assert_eq!(t.superseded, 2);
        t.presented(t0);
        let totals = t.totals();
        assert_eq!(
            (totals.imported, totals.presented, totals.dropped),
            (2, 1, 0)
        );
        t.break_intervals();
        t.presented(t0 + Duration::from_secs(1));
        assert!(t.samples[Stage::PresentInterval as usize].is_empty());
        assert_eq!(t.samples[Stage::ImportInterval as usize], vec![16_000]);
        let mut lines = Vec::new();
        t.report("initial", |l| lines.push(l));
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("superseded_producer_frames=2"));
        assert!(lines[1].contains("stage=import-interval count=1"));
        assert_eq!(t.imported, 0);
    }
}
