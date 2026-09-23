/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Opt-in counters for diagnosing shell overhead on real game builds.
//!
//! Set `ROVES_PERF_LOG_INTERVAL_MS` to a positive interval (for example `10000`) to emit one
//! aggregate `[roves-perf]` line per interval. When the variable is absent or invalid, recording
//! is disabled after a single environment lookup; no timer or background thread is created.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const PERF_LOG_INTERVAL_ENV: &str = "ROVES_PERF_LOG_INTERVAL_MS";

#[derive(Default)]
struct Counts {
    real_events: AtomicU64,
    wait_timeouts: AtomicU64,
    redraw_queued: AtomicU64,
    redraw_coalesced: AtomicU64,
    redraw_dispatched: AtomicU64,
    webview_paints: AtomicU64,
    window_presents: AtomicU64,
}

impl Counts {
    fn take(&self) -> Snapshot {
        Snapshot {
            real_events: self.real_events.swap(0, Ordering::Relaxed),
            wait_timeouts: self.wait_timeouts.swap(0, Ordering::Relaxed),
            redraw_queued: self.redraw_queued.swap(0, Ordering::Relaxed),
            redraw_coalesced: self.redraw_coalesced.swap(0, Ordering::Relaxed),
            redraw_dispatched: self.redraw_dispatched.swap(0, Ordering::Relaxed),
            webview_paints: self.webview_paints.swap(0, Ordering::Relaxed),
            window_presents: self.window_presents.swap(0, Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Snapshot {
    real_events: u64,
    wait_timeouts: u64,
    redraw_queued: u64,
    redraw_coalesced: u64,
    redraw_dispatched: u64,
    webview_paints: u64,
    window_presents: u64,
}

struct PerformanceCounters {
    interval: Duration,
    last_report: Mutex<Instant>,
    counts: Counts,
}

static COUNTERS: OnceLock<Option<PerformanceCounters>> = OnceLock::new();

fn parse_interval(value: Option<&str>) -> Option<Duration> {
    let milliseconds = value?.parse::<u64>().ok()?;
    (milliseconds > 0).then(|| Duration::from_millis(milliseconds))
}

fn counters() -> Option<&'static PerformanceCounters> {
    COUNTERS
        .get_or_init(|| {
            parse_interval(std::env::var(PERF_LOG_INTERVAL_ENV).ok().as_deref()).map(|interval| {
                log::info!(
                    "[roves-perf] enabled interval_ms={}",
                    interval.as_millis()
                );
                PerformanceCounters {
                    interval,
                    last_report: Mutex::new(Instant::now()),
                    counts: Counts::default(),
                }
            })
        })
        .as_ref()
}

fn report_if_due(counters: &PerformanceCounters) {
    let now = Instant::now();
    let Ok(mut last_report) = counters.last_report.try_lock() else {
        return;
    };
    let elapsed = now.saturating_duration_since(*last_report);
    if elapsed < counters.interval {
        return;
    }
    *last_report = now;
    let counts = counters.counts.take();
    log::info!(
        "[roves-perf] interval_ms={} real_events={} wait_timeouts={} redraw_queued={} redraw_coalesced={} redraw_dispatched={} webview_paints={} window_presents={}",
        elapsed.as_millis(),
        counts.real_events,
        counts.wait_timeouts,
        counts.redraw_queued,
        counts.redraw_coalesced,
        counts.redraw_dispatched,
        counts.webview_paints,
        counts.window_presents,
    );
}

pub(crate) fn record_event_loop_wake(timed_out: bool) {
    let Some(counters) = counters() else {
        return;
    };
    let counter = if timed_out {
        &counters.counts.wait_timeouts
    } else {
        &counters.counts.real_events
    };
    counter.fetch_add(1, Ordering::Relaxed);
    report_if_due(counters);
}

pub(crate) fn record_redraw_request(queued: bool) {
    let Some(counters) = counters() else {
        return;
    };
    let counter = if queued {
        &counters.counts.redraw_queued
    } else {
        &counters.counts.redraw_coalesced
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_redraw_dispatched() {
    if let Some(counters) = counters() {
        counters
            .counts
            .redraw_dispatched
            .fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn record_webview_paint() {
    if let Some(counters) = counters() {
        counters
            .counts
            .webview_paints
            .fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn record_window_present() {
    if let Some(counters) = counters() {
        counters
            .counts
            .window_presents
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{Counts, Snapshot, parse_interval};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn missing_invalid_and_zero_intervals_disable_diagnostics() {
        assert_eq!(parse_interval(None), None);
        assert_eq!(parse_interval(Some("invalid")), None);
        assert_eq!(parse_interval(Some("0")), None);
    }

    #[test]
    fn positive_interval_is_accepted_in_milliseconds() {
        assert_eq!(parse_interval(Some("10000")), Some(Duration::from_secs(10)));
    }

    #[test]
    fn taking_a_snapshot_resets_every_counter() {
        let counts = Counts::default();
        counts.real_events.store(2, Ordering::Relaxed);
        counts.window_presents.store(3, Ordering::Relaxed);
        assert_eq!(
            counts.take(),
            Snapshot {
                real_events: 2,
                window_presents: 3,
                ..Snapshot::default()
            }
        );
        assert_eq!(counts.take(), Snapshot::default());
    }
}
