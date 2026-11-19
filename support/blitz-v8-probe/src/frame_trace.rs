//! Opt-in per-frame trace of the GPU probe window, for correlation with an
//! external PresentMon capture (`ROVES_PROBE_FRAME_TRACE=<path>`, only
//! together with `ROVES_PROBE_FRAME_TIMINGS=1`).
//!
//! Without both variables nothing is allocated and no timestamp is taken.
//! With them, one record per `GpuWindowRenderer::render` attempt and one per
//! discrete input event (pointer/key down/up, wheel) is kept in a single
//! bounded ring of `MAX_RECORDS` entries, allocated once; nothing is written
//! during the run. When the window closes the ring is written once as NDJSON
//! (`write_to`): a header line, then the kept records oldest first.
//!
//! Timestamps are raw Windows `QueryPerformanceCounter` ticks (the timebase
//! PresentMon's `CPU_START_QPC` uses), with `qpc_frequency` ticks per second
//! in the header. The shared timebase allows per-scene windows, not per-frame
//! pairing: on the 15 November Vulkan captures at most 3 of 3,590 present
//! QPCs per run fell within 100 us of a PresentMon `CPU_START_QPC` (see
//! `docs/ROVES_PERFORMANCE.md`). Durations are integer nanoseconds from `std::time::Instant`
//! (the same measurements as the `ROVES_PROBE_FRAME_TIMINGS` summary). On
//! other platforms, or if `QueryPerformanceFrequency` fails, the clock is
//! `unavailable` and every QPC field is `null`; durations are still recorded.
//!
//! A record is a render ATTEMPT, not a displayed frame: `outcome` says whether
//! `surface_present` returned success without errors, which still does not
//! prove that the frame reached the screen (only PresentMon's
//! `DISPLAYED_TIME` does). Missing values (phase not reached, present not
//! called, QPC read failure) are `null`, never zero. Records are numeric
//! only: no URL, text, node id, asset, key code or pointer position.
//!
//! Slow frame steps (19 November): a frame record only carries the LATEST
//! step before its attempt, so with several steps between two attempts a long
//! step could vanish from the trace. Every step whose `cpu_frame_step` exceeds
//! `SLOW_STEP_THRESHOLD` (the summary's `over_16_67ms` rule) is therefore also
//! pushed as its own `slow_step` record when it ends, independently of render
//! attempts, into the same ring (no extra storage, clock read or allocation).
//! Join on `step_seq`: a `slow_step` exists at most once per step, and a later
//! frame record may point at the same `step_seq` (same QPC, duration and
//! phases); count slow steps from `slow_step` records only. `renders_before` is
//! the `render_seq` of the last attempt before the step ended, so the first
//! attempt that can show the step's effects is `renders_before + 1`. Mirrors
//! Chromium's per-task main-thread slices and Long Tasks (pinned `f2700724`,
//! `PerformanceMonitor::DidProcessTask` in
//! `third_party/blink/renderer/core/frame/performance_monitor.cc`): a long
//! task is reported when it ends, from its own start/end, not via a frame.
//!
//! `std::time::Instant` cannot expose its QPC value, so the only addition is
//! `windows-sys` (already locked through winit/wgpu, `Win32_System_Performance`
//! feature) for `QueryPerformanceCounter`/`QueryPerformanceFrequency`.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use crate::frame_timings::{FramePhases, MissCounts, MissReason, Phase, StepPhases};

/// Records kept (frames and input markers together): about 60 seconds of
/// render attempts at 60 Hz.
pub const MAX_RECORDS: usize = 3600;

pub const SCHEMA: &str = "roves-frame-trace";
pub const SCHEMA_VERSION: u32 = 1;

/// Frame steps strictly longer than this get a `slow_step` record (the same
/// 16.67 ms budget as the summary's `over_16_67ms`).
pub const SLOW_STEP_THRESHOLD: Duration = Duration::from_micros(16_670);

/// Reads the QPC timebase; `None` when unavailable.
pub type Clock = fn() -> Option<i64>;

/// `QueryPerformanceCounter`; `None` on failure.
#[cfg(windows)]
pub fn qpc_now() -> Option<i64> {
    let mut value = 0i64;
    // SAFETY: a valid pointer to an i64 the call writes.
    let ok =
        unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut value) };
    (ok != 0).then_some(value)
}

#[cfg(not(windows))]
pub fn qpc_now() -> Option<i64> {
    None
}

/// `QueryPerformanceFrequency` (ticks per second); `None` on failure or 0.
#[cfg(windows)]
pub fn qpc_frequency() -> Option<i64> {
    let mut value = 0i64;
    // SAFETY: a valid pointer to an i64 the call writes.
    let ok =
        unsafe { windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut value) };
    (ok != 0 && value > 0).then_some(value)
}

#[cfg(not(windows))]
pub fn qpc_frequency() -> Option<i64> {
    None
}

fn unavailable() -> Option<i64> {
    None
}

/// Reads `clock` only when tracing is enabled (`Some`).
pub fn stamp(clock: Option<Clock>) -> Option<i64> {
    clock.and_then(|clock| clock())
}

/// The return value of `surface_present`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentResult {
    Good,
    Suboptimal,
    Timeout,
    Occluded,
    Outdated,
    Lost,
    OtherStatus,
    Error,
}

impl PresentResult {
    fn key(self) -> &'static str {
        match self {
            PresentResult::Good => "good",
            PresentResult::Suboptimal => "suboptimal",
            PresentResult::Timeout => "timeout",
            PresentResult::Occluded => "occluded",
            PresentResult::Outdated => "outdated",
            PresentResult::Lost => "lost",
            PresentResult::OtherStatus => "other_status",
            PresentResult::Error => "error",
        }
    }
}

/// How a render attempt ended.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Outcome {
    /// `surface_present` succeeded and no wgpu error was reported for the
    /// frame (committed like `PresentStats::frames`); not proof of display.
    Presented,
    /// `surface_get_current_texture` returned an error.
    AcquireError,
    /// The acquired surface status was not good/suboptimal.
    AcquireStatus,
    /// `surface_present` returned an error or a non-good status.
    PresentFailed,
    /// Presented, but a wgpu error was reported during the frame (skipped).
    FrameError,
    /// Not presented on purpose: the window would show exactly the last
    /// presented frame (`PresentStats::unchanged_frames`); no acquire,
    /// compose or present call.
    Unchanged,
    /// `render` returned before any of the above (should not happen).
    #[default]
    NotReached,
}

impl Outcome {
    fn key(self) -> &'static str {
        match self {
            Outcome::Presented => "presented",
            Outcome::AcquireError => "acquire_error",
            Outcome::AcquireStatus => "acquire_status",
            Outcome::PresentFailed => "present_failed",
            Outcome::FrameError => "frame_error",
            Outcome::Unchanged => "unchanged",
            Outcome::NotReached => "not_reached",
        }
    }
}

/// Bounded counters of one render attempt (presented or discarded).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttemptCounters {
    pub canvas_layers: u64,
    pub raster_segments: u64,
    pub raster_uploads: u64,
    pub raster_cache_hits: u64,
    pub upload_bytes: u64,
    pub misses: MissCounts,
}

/// What the renderer measured during one attempt (filled by the compositor).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttemptTrace {
    pub render_start_qpc: Option<i64>,
    pub present_start_qpc: Option<i64>,
    pub present_end_qpc: Option<i64>,
    pub render_end_qpc: Option<i64>,
    /// `None` when `surface_present` was not called.
    pub present_result: Option<PresentResult>,
    pub outcome: Outcome,
    /// `None` when the compositor did not reach commit/discard.
    pub counters: Option<AttemptCounters>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StepInfo {
    seq: u64,
    start_qpc: Option<i64>,
    cpu: Option<Duration>,
    phases: StepPhases,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameRecord {
    /// 1-based, every render attempt.
    render_seq: u64,
    /// 1-based among attempts that called `surface_present`.
    present_seq: Option<u64>,
    /// The latest frame step started before this attempt.
    step: Option<StepInfo>,
    /// Frame steps since the previous render attempt (0: no new step).
    steps_since_previous_render: u32,
    attempt: AttemptTrace,
    cpu_render_present: Duration,
    phases: FramePhases,
}

/// A discrete input event dispatched to the page (no key, button or position).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkerKind {
    PointerDown,
    PointerUp,
    KeyDown,
    KeyUp,
    Wheel,
}

impl MarkerKind {
    fn key(self) -> &'static str {
        match self {
            MarkerKind::PointerDown => "pointer_down",
            MarkerKind::PointerUp => "pointer_up",
            MarkerKind::KeyDown => "key_down",
            MarkerKind::KeyUp => "key_up",
            MarkerKind::Wheel => "wheel",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerRecord {
    marker_seq: u64,
    kind: MarkerKind,
    /// QPC at the start of the dispatch to the page.
    qpc: Option<i64>,
    /// CPU wall-clock time of the dispatch (event handlers, microtasks).
    cpu_dispatch: Duration,
}

/// A frame step longer than `SLOW_STEP_THRESHOLD`, recorded when it ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlowStepRecord {
    step: StepInfo,
    /// `render_seq` of the last render attempt before the step ended (0: none).
    renders_before: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Record {
    Frame(FrameRecord),
    Marker(MarkerRecord),
    SlowStep(SlowStepRecord),
}

pub struct FrameTrace {
    path: PathBuf,
    clock: Clock,
    frequency: Option<i64>,
    records: Vec<Record>,
    capacity: usize,
    next: usize,
    total: u64,
    render_seq: u64,
    present_seq: u64,
    presented: u64,
    step_seq: u64,
    marker_seq: u64,
    slow_steps: u64,
    last_step: Option<StepInfo>,
    steps_since_render: u32,
    /// QPC reads that failed while the timebase was available.
    qpc_errors: u64,
    written: bool,
}

impl FrameTrace {
    /// `Some` only when frame timings are enabled and `trace` (the value of
    /// `ROVES_PROBE_FRAME_TRACE`) is a non-empty path.
    pub fn from_values(timings_enabled: bool, trace: Option<&str>) -> Option<Box<Self>> {
        let path = trace.filter(|path| !path.is_empty())?;
        if !timings_enabled {
            eprintln!("ROVES_PROBE_FRAME_TRACE ignored: it requires ROVES_PROBE_FRAME_TIMINGS=1");
            return None;
        }
        let frequency = qpc_frequency();
        let clock: Clock = if frequency.is_some() {
            qpc_now
        } else {
            unavailable
        };
        Some(Box::new(Self::new(
            path.into(),
            clock,
            frequency,
            MAX_RECORDS,
        )))
    }

    fn new(path: PathBuf, clock: Clock, frequency: Option<i64>, capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            path,
            clock,
            frequency,
            records: Vec::with_capacity(capacity),
            capacity,
            next: 0,
            total: 0,
            render_seq: 0,
            present_seq: 0,
            presented: 0,
            step_seq: 0,
            marker_seq: 0,
            slow_steps: 0,
            last_step: None,
            steps_since_render: 0,
            qpc_errors: 0,
            written: false,
        }
    }

    /// The clock the renderer stamps with (QPC, or always `None`).
    pub fn clock(&self) -> Clock {
        self.clock
    }

    /// Reads the clock, counting failures of an available timebase.
    pub fn now(&mut self) -> Option<i64> {
        let value = (self.clock)();
        self.count_missing(value);
        value
    }

    fn count_missing(&mut self, value: Option<i64>) {
        if value.is_none() && self.frequency.is_some() {
            self.qpc_errors += 1;
        }
    }

    fn push(&mut self, record: Record) {
        if self.records.len() < self.capacity {
            self.records.push(record);
        } else {
            self.records[self.next] = record;
        }
        self.next = (self.next + 1) % self.capacity;
        self.total += 1;
    }

    /// A frame step started at `start_qpc` and took `cpu`, split into
    /// `phases` (see `frame_timings::StepPhase`).
    pub fn record_step(&mut self, start_qpc: Option<i64>, cpu: Duration, phases: &StepPhases) {
        self.step_seq += 1;
        self.steps_since_render += 1;
        let step = StepInfo {
            seq: self.step_seq,
            start_qpc,
            cpu: Some(cpu),
            phases: *phases,
        };
        self.last_step = Some(step);
        if cpu > SLOW_STEP_THRESHOLD {
            self.slow_steps += 1;
            self.push(Record::SlowStep(SlowStepRecord {
                step,
                renders_before: self.render_seq,
            }));
        }
    }

    /// One render attempt. The present timestamps come from the renderer's
    /// `clock`, so their read failures are counted here.
    pub fn record_frame(
        &mut self,
        attempt: AttemptTrace,
        cpu_render_present: Duration,
        phases: &FramePhases,
    ) {
        self.count_missing(attempt.render_start_qpc);
        self.count_missing(attempt.render_end_qpc);
        if attempt.present_result.is_some() {
            self.count_missing(attempt.present_start_qpc);
            self.count_missing(attempt.present_end_qpc);
        }
        self.render_seq += 1;
        let present_seq = attempt.present_result.map(|_| {
            self.present_seq += 1;
            self.present_seq
        });
        self.presented += u64::from(attempt.outcome == Outcome::Presented);
        let record = FrameRecord {
            render_seq: self.render_seq,
            present_seq,
            step: self.last_step,
            steps_since_previous_render: std::mem::take(&mut self.steps_since_render),
            attempt,
            cpu_render_present,
            phases: *phases,
        };
        self.push(Record::Frame(record));
    }

    pub fn record_marker(&mut self, kind: MarkerKind, qpc: Option<i64>, cpu_dispatch: Duration) {
        self.marker_seq += 1;
        self.push(Record::Marker(MarkerRecord {
            marker_seq: self.marker_seq,
            kind,
            qpc,
            cpu_dispatch,
        }));
    }

    /// Kept records, oldest first.
    fn ordered(&self) -> impl Iterator<Item = &Record> {
        let (newer, older) = if self.records.len() < self.capacity {
            (&self.records[..], &self.records[..0])
        } else {
            self.records.split_at(self.next)
        };
        older.iter().chain(newer)
    }

    /// The NDJSON trace: a header line, then one line per kept record.
    pub fn write_to(
        &self,
        out: &mut impl Write,
        renderer: &str,
        raster_cache_disabled: Option<bool>,
    ) -> std::io::Result<()> {
        let raster_cache = match raster_cache_disabled {
            Some(true) => "\"disabled\"",
            Some(false) => "\"enabled\"",
            None => "null",
        };
        writeln!(
            out,
            "{{\"schema\":\"{SCHEMA}\",\"version\":{SCHEMA_VERSION},\"clock\":\"{}\",\"qpc_frequency\":{},\"qpc_unit\":\"ticks\",\"duration_unit\":\"ns\",\"max_records\":{},\"records_total\":{},\"records_kept\":{},\"records_overwritten\":{},\"render_attempts\":{},\"present_calls\":{},\"presented\":{},\"frame_steps\":{},\"input_markers\":{},\"qpc_errors\":{},\"slow_steps\":{},\"slow_step_threshold_ns\":{},\"renderer\":\"{renderer}\",\"raster_threading\":\"{}\",\"raster_cache\":{raster_cache},\"note\":\"one frame record per GpuWindowRenderer::render attempt; outcome presented means surface_present returned good/suboptimal with no wgpu error for the frame, not that the frame was displayed (use PresentMon DISPLAYED_TIME); outcome unchanged means the attempt skipped acquire, compose and present because the window already showed an identical frame; present QPCs are NOT tick-matched to PresentMon CPU_START_QPC (Vulkan captures of 15 November: at most 3 of 3,590 records per run within 100 us, nearest offsets of a few ms), so compare PresentMon per scene over shared QPC windows, not frame by frame; null means not reached or not measured, never zero; phases_ns are the same CPU wall-clock phase sums as ROVES_PROBE_FRAME_TIMINGS; counters cover this attempt only, discarded attempts included; step_phases_ns (additive in version 1, last field) splits cpu_frame_step_ns of the same step into consecutive non-overlapping cpu_step_* phases that add up to it, null when no step or phase not reached; slow_step records (additive in version 1, ignore unknown type values) are pushed when a frame step longer than slow_step_threshold_ns ends, independently of render attempts, at most one per step_seq: a frame record may reference the same step_seq with identical step fields, so count slow steps from slow_step records only and join frames on step_seq; renders_before is the render_seq of the last attempt before the step ended\"}}",
            if self.frequency.is_some() {
                "qpc"
            } else {
                "unavailable"
            },
            opt(self.frequency),
            self.capacity,
            self.total,
            self.records.len(),
            self.total - self.records.len() as u64,
            self.render_seq,
            self.present_seq,
            self.presented,
            self.step_seq,
            self.marker_seq,
            self.qpc_errors,
            self.slow_steps,
            SLOW_STEP_THRESHOLD.as_nanos(),
            crate::presentation::RASTER_THREADING,
        )?;
        for record in self.ordered() {
            match record {
                Record::Frame(frame) => writeln!(out, "{}", frame_json(frame))?,
                Record::Marker(marker) => writeln!(out, "{}", marker_json(marker))?,
                Record::SlowStep(slow) => writeln!(out, "{}", slow_step_json(slow))?,
            }
        }
        out.flush()
    }

    /// Writes the trace file once (later calls are no-ops) and prints a
    /// `ROVES_PROBE_FRAME_TRACE` line with the outcome (no path).
    pub fn write_once(&mut self, renderer: &str, raster_cache_disabled: Option<bool>) {
        if self.written {
            return;
        }
        self.written = true;
        let result = std::fs::File::create(&self.path).and_then(|file| {
            let mut out = std::io::BufWriter::new(file);
            self.write_to(&mut out, renderer, raster_cache_disabled)
        });
        match result {
            Ok(()) => println!(
                "ROVES_PROBE_FRAME_TRACE {{\"check\":\"frame_trace\",\"status\":\"written\",\"schema\":\"{SCHEMA}\",\"version\":{SCHEMA_VERSION},\"records\":{},\"render_attempts\":{}}}",
                self.records.len(),
                self.render_seq,
            ),
            Err(error) => {
                eprintln!(
                    "ROVES_PROBE_FRAME_TRACE_ERROR {}: {error}",
                    self.path.display()
                );
                println!(
                    "ROVES_PROBE_FRAME_TRACE {{\"check\":\"frame_trace\",\"status\":\"failed\",\"records\":{}}}",
                    self.records.len()
                );
            },
        }
        let _ = std::io::stdout().flush();
    }
}

fn opt<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn nanos(duration: Duration) -> u128 {
    duration.as_nanos()
}

fn frame_json(frame: &FrameRecord) -> String {
    let attempt = &frame.attempt;
    let phases: Vec<String> = Phase::ALL
        .iter()
        .map(|phase| {
            format!(
                "\"{}\":{}",
                phase.key(),
                opt(frame.phases.get(*phase).map(nanos))
            )
        })
        .collect();
    let counters = attempt.counters.map_or_else(
        || "null".to_owned(),
        |counters| {
            let misses: Vec<String> = MissReason::ALL
                .iter()
                .map(|reason| format!("\"{}\":{}", reason.key(), counters.misses.get(*reason)))
                .collect();
            format!(
                "{{\"canvas_layers\":{},\"raster_segments\":{},\"raster_uploads\":{},\"raster_cache_hits\":{},\"upload_bytes\":{},\"misses\":{{{}}}}}",
                counters.canvas_layers,
                counters.raster_segments,
                counters.raster_uploads,
                counters.raster_cache_hits,
                counters.upload_bytes,
                misses.join(",")
            )
        },
    );
    format!(
        "{{\"type\":\"frame\",\"render_seq\":{},\"present_seq\":{},\"step_seq\":{},\"step_start_qpc\":{},\"cpu_frame_step_ns\":{},\"steps_since_previous_render\":{},\"render_start_qpc\":{},\"present_start_qpc\":{},\"present_end_qpc\":{},\"render_end_qpc\":{},\"present_result\":{},\"outcome\":\"{}\",\"cpu_render_present_ns\":{},\"phases_ns\":{{{}}},\"counters\":{counters},\"step_phases_ns\":{}}}",
        frame.render_seq,
        opt(frame.present_seq),
        opt(frame.step.map(|step| step.seq)),
        opt(frame.step.and_then(|step| step.start_qpc)),
        opt(frame.step.and_then(|step| step.cpu).map(nanos)),
        frame.steps_since_previous_render,
        opt(attempt.render_start_qpc),
        opt(attempt.present_start_qpc),
        opt(attempt.present_end_qpc),
        opt(attempt.render_end_qpc),
        attempt.present_result.map_or_else(
            || "null".to_owned(),
            |result| format!("\"{}\"", result.key())
        ),
        attempt.outcome.key(),
        nanos(frame.cpu_render_present),
        phases.join(","),
        frame.step.map_or_else(
            || "null".to_owned(),
            |step| step.phases.json(|duration| nanos(duration).to_string())
        ),
    )
}

fn slow_step_json(slow: &SlowStepRecord) -> String {
    let step = &slow.step;
    format!(
        "{{\"type\":\"slow_step\",\"step_seq\":{},\"step_start_qpc\":{},\"cpu_frame_step_ns\":{},\"renders_before\":{},\"step_phases_ns\":{}}}",
        step.seq,
        opt(step.start_qpc),
        opt(step.cpu.map(nanos)),
        slow.renders_before,
        step.phases.json(|duration| nanos(duration).to_string()),
    )
}

fn marker_json(marker: &MarkerRecord) -> String {
    format!(
        "{{\"type\":\"marker\",\"marker_seq\":{},\"kind\":\"{}\",\"qpc\":{},\"cpu_dispatch_ns\":{}}}",
        marker.marker_seq,
        marker.kind.key(),
        opt(marker.qpc),
        nanos(marker.cpu_dispatch),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_clock() -> Option<i64> {
        Some(42)
    }

    fn trace(capacity: usize, clock: Clock, frequency: Option<i64>) -> FrameTrace {
        FrameTrace::new("unused.ndjson".into(), clock, frequency, capacity)
    }

    fn output(trace: &FrameTrace) -> Vec<String> {
        let mut bytes = Vec::new();
        trace
            .write_to(&mut bytes, "gpu", Some(false))
            .expect("in-memory write");
        String::from_utf8(bytes)
            .expect("utf-8")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn presented(start: i64) -> AttemptTrace {
        AttemptTrace {
            render_start_qpc: Some(start),
            present_start_qpc: Some(start + 10),
            present_end_qpc: Some(start + 20),
            render_end_qpc: Some(start + 25),
            present_result: Some(PresentResult::Good),
            outcome: Outcome::Presented,
            counters: Some(AttemptCounters::default()),
        }
    }

    #[test]
    fn disabled_without_both_variables_and_never_reads_the_clock() {
        assert!(FrameTrace::from_values(false, None).is_none());
        assert!(FrameTrace::from_values(true, None).is_none());
        assert!(FrameTrace::from_values(true, Some("")).is_none());
        // The trace needs ROVES_PROBE_FRAME_TIMINGS=1 as well.
        assert!(FrameTrace::from_values(false, Some("trace.ndjson")).is_none());
        let enabled = FrameTrace::from_values(true, Some("trace.ndjson")).expect("enabled");
        assert_eq!(enabled.records.capacity(), MAX_RECORDS);
        assert_eq!(MAX_RECORDS, 3600);
        // Disabled tracing passes no clock to the renderer: nothing is read.
        assert_eq!(stamp(None), None);
        assert_eq!(stamp(Some(fixed_clock)), Some(42));
    }

    #[test]
    fn the_ring_is_bounded_and_written_oldest_first() {
        let mut trace = trace(3, fixed_clock, Some(10_000_000));
        trace.record_frame(
            presented(100),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        trace.record_marker(MarkerKind::PointerDown, Some(150), Duration::from_micros(5));
        trace.record_frame(
            presented(200),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        trace.record_frame(
            presented(300),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        trace.record_frame(
            presented(400),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        assert_eq!(trace.records.len(), 3);
        let lines = output(&trace);
        assert_eq!(lines.len(), 4);
        assert!(
            lines[0].contains("\"records_total\":5,\"records_kept\":3,\"records_overwritten\":2")
        );
        assert!(lines[0].contains("\"render_attempts\":4,\"present_calls\":4,\"presented\":4"));
        assert!(lines[0].contains("\"input_markers\":1"));
        // The first frame and the marker were overwritten; order is kept.
        assert!(lines[1].contains("\"render_seq\":2,\"present_seq\":2"));
        assert!(lines[2].contains("\"render_seq\":3,"));
        assert!(lines[3].contains("\"render_seq\":4,"));
        assert!(lines[3].contains("\"render_start_qpc\":400,\"present_start_qpc\":410,\"present_end_qpc\":420,\"render_end_qpc\":425"));
    }

    #[test]
    fn markers_and_steps_share_the_qpc_timeline_without_content() {
        let mut trace = trace(8, fixed_clock, Some(10_000_000));
        let mut phases = StepPhases::default();
        phases.finish(Duration::from_micros(700));
        trace.record_step(Some(90), Duration::from_micros(700), &phases);
        phases.finish(Duration::from_micros(800));
        trace.record_step(Some(95), Duration::from_micros(800), &phases);
        trace.record_marker(MarkerKind::KeyDown, Some(96), Duration::from_micros(30));
        trace.record_frame(
            presented(100),
            Duration::from_millis(2),
            &FramePhases::default(),
        );
        trace.record_frame(
            presented(200),
            Duration::from_millis(2),
            &FramePhases::default(),
        );
        let lines = output(&trace);
        assert_eq!(
            lines[1],
            "{\"type\":\"marker\",\"marker_seq\":1,\"kind\":\"key_down\",\"qpc\":96,\"cpu_dispatch_ns\":30000}"
        );
        assert!(lines[2].contains("\"step_seq\":2,\"step_start_qpc\":95,\"cpu_frame_step_ns\":800000,\"steps_since_previous_render\":2"));
        // No new step before the second attempt: it points at the same step.
        assert!(lines[3].contains("\"step_seq\":2,"));
        assert!(lines[3].contains("\"steps_since_previous_render\":0"));
        assert!(lines[0].contains("\"frame_steps\":2"));
        // The step's phases are the last field and add up to its duration.
        assert!(lines[2].ends_with(",\"step_phases_ns\":{\"cpu_step_tokio_yield\":null,\"cpu_step_mcp_poll\":null,\"cpu_step_animation_frame\":null,\"cpu_step_microtasks\":null,\"cpu_step_event_loop\":null,\"cpu_step_other\":800000}}"));
    }

    #[test]
    fn a_slow_step_survives_later_steps_before_one_render() {
        let mut trace = trace(8, fixed_clock, Some(10_000_000));
        trace.record_frame(
            presented(10),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        // At the threshold: not slow (strictly above, like over_16_67ms).
        let mut phases = StepPhases::default();
        phases.finish(SLOW_STEP_THRESHOLD);
        trace.record_step(Some(50), SLOW_STEP_THRESHOLD, &phases);
        let slow = Duration::from_millis(300);
        phases.finish(slow);
        trace.record_step(Some(60), slow, &phases);
        phases.finish(Duration::from_millis(2));
        trace.record_step(Some(70), Duration::from_millis(2), &phases);
        trace.record_frame(
            presented(100),
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        let lines = output(&trace);
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("\"records_total\":3,"));
        assert!(lines[0].contains("\"frame_steps\":3,"));
        assert!(lines[0].contains("\"slow_steps\":1,\"slow_step_threshold_ns\":16670000,"));
        // The slow step keeps its own id, QPC, duration and phases.
        assert_eq!(
            lines[2],
            "{\"type\":\"slow_step\",\"step_seq\":2,\"step_start_qpc\":60,\"cpu_frame_step_ns\":300000000,\"renders_before\":1,\"step_phases_ns\":{\"cpu_step_tokio_yield\":null,\"cpu_step_mcp_poll\":null,\"cpu_step_animation_frame\":null,\"cpu_step_microtasks\":null,\"cpu_step_event_loop\":null,\"cpu_step_other\":300000000}}"
        );
        // The following attempt still links only the latest (fast) step.
        assert!(lines[3].contains("\"render_seq\":2,"));
        assert!(lines[3].contains("\"step_seq\":3,\"step_start_qpc\":70,\"cpu_frame_step_ns\":2000000,\"steps_since_previous_render\":3"));
    }

    #[test]
    fn missing_phases_counters_and_present_are_null_not_zero() {
        let mut trace = trace(4, fixed_clock, Some(10_000_000));
        let mut phases = FramePhases::default();
        phases.add(Phase::Raster, Duration::from_micros(1500));
        phases.add(Phase::SurfaceAcquire, Duration::ZERO);
        let attempt = AttemptTrace {
            render_start_qpc: Some(5),
            outcome: Outcome::AcquireError,
            ..AttemptTrace::default()
        };
        trace.record_frame(attempt, Duration::from_micros(2500), &phases);
        let mut counters = AttemptCounters {
            raster_segments: 3,
            raster_uploads: 2,
            raster_cache_hits: 1,
            upload_bytes: 64,
            ..AttemptCounters::default()
        };
        counters.misses.add(MissReason::FirstUse);
        counters.misses.add(MissReason::ContentChanged);
        trace.record_frame(
            AttemptTrace {
                present_result: Some(PresentResult::Outdated),
                outcome: Outcome::PresentFailed,
                counters: Some(counters),
                ..presented(10)
            },
            Duration::from_millis(3),
            &FramePhases::default(),
        );
        let lines = output(&trace);
        let first = &lines[1];
        assert!(first.contains("\"present_seq\":null,\"step_seq\":null,\"step_start_qpc\":null,\"cpu_frame_step_ns\":null"));
        assert!(first.contains("\"present_start_qpc\":null,\"present_end_qpc\":null,\"render_end_qpc\":null,\"present_result\":null,\"outcome\":\"acquire_error\""));
        // A measured zero stays zero; an unreached phase is null.
        assert!(first.contains("\"cpu_surface_acquire\":0,"));
        assert!(first.contains("\"cpu_raster\":1500000,"));
        assert!(first.contains("\"cpu_present\":null}"));
        // Every phase plus cpu_frame_step_ns and cpu_render_present_ns.
        assert_eq!(first.matches("\"cpu_").count(), Phase::ALL.len() + 2);
        assert!(first.ends_with("\"counters\":null,\"step_phases_ns\":null}"));
        let second = &lines[2];
        assert!(second.contains("\"present_seq\":1,"));
        assert!(second.contains("\"present_result\":\"outdated\",\"outcome\":\"present_failed\""));
        assert!(second.contains(
            "\"raster_segments\":3,\"raster_uploads\":2,\"raster_cache_hits\":1,\"upload_bytes\":64"
        ));
        assert!(second.contains("\"first_use\":1,"));
        assert!(second.contains("\"content_changed\":1,"));
        assert!(lines[0].contains("\"render_attempts\":2,\"present_calls\":1,\"presented\":0"));
        assert!(lines[0].starts_with("{\"schema\":\"roves-frame-trace\",\"version\":1,"));
        // The A/B variant is declared by the build (opt-in `vello-mt` feature).
        let threading = if cfg!(feature = "vello-mt") {
            "multi"
        } else {
            "single"
        };
        assert!(lines[0].contains(&format!(
            "\"renderer\":\"gpu\",\"raster_threading\":\"{threading}\","
        )));
    }

    #[test]
    fn units_are_raw_qpc_ticks_and_integer_nanoseconds() {
        let mut trace = trace(2, fixed_clock, Some(10_000_000));
        assert_eq!(trace.now(), Some(42));
        trace.record_frame(
            presented(1_234_567_890_123),
            Duration::new(1, 5),
            &FramePhases::default(),
        );
        let lines = output(&trace);
        assert!(lines[0].contains("\"clock\":\"qpc\",\"qpc_frequency\":10000000,\"qpc_unit\":\"ticks\",\"duration_unit\":\"ns\""));
        // QPC values pass through unscaled; durations are whole nanoseconds.
        assert!(lines[1].contains("\"render_start_qpc\":1234567890123,"));
        assert!(lines[1].contains("\"cpu_render_present_ns\":1000000005,"));
        assert!(lines[0].contains("\"qpc_errors\":0"));
    }

    #[test]
    fn qpc_failures_are_null_and_counted_and_an_unavailable_clock_is_declared() {
        fn failing() -> Option<i64> {
            None
        }
        let mut trace = trace(4, failing, Some(10_000_000));
        assert_eq!(trace.now(), None);
        trace.record_frame(
            AttemptTrace {
                present_result: Some(PresentResult::Good),
                outcome: Outcome::Presented,
                ..AttemptTrace::default()
            },
            Duration::from_millis(1),
            &FramePhases::default(),
        );
        let lines = output(&trace);
        // One failed read in `now`, then render start/end and present
        // start/end missing in the attempt.
        assert!(lines[0].contains("\"qpc_errors\":5"));
        assert!(lines[1].contains(
            "\"render_start_qpc\":null,\"present_start_qpc\":null,\"present_end_qpc\":null"
        ));

        // No timebase (non-Windows or QueryPerformanceFrequency failure): the
        // clock is declared unavailable and missing reads are not errors.
        let mut unavailable = trace_with_unavailable_clock();
        assert_eq!(unavailable.now(), None);
        let lines = output(&unavailable);
        assert!(lines[0].contains("\"clock\":\"unavailable\",\"qpc_frequency\":null"));
        assert!(lines[0].contains("\"qpc_errors\":0"));
    }

    fn trace_with_unavailable_clock() -> FrameTrace {
        trace(2, unavailable, None)
    }

    #[test]
    fn an_unwritable_path_fails_without_panicking_and_writes_once() {
        let dir = std::env::temp_dir().join(format!("roves-frame-trace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Parent directory missing: creation fails, reported, no panic.
        let mut missing =
            FrameTrace::new(dir.join("missing").join("t.ndjson"), fixed_clock, None, 2);
        missing.write_once("gpu", None);
        assert!(missing.written);

        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("trace.ndjson");
        let mut trace = FrameTrace::new(path.clone(), fixed_clock, Some(10_000_000), 2);
        trace.record_marker(MarkerKind::Wheel, Some(1), Duration::ZERO);
        trace.write_once("gpu", Some(true));
        let written = std::fs::read_to_string(&path).expect("trace readable");
        assert_eq!(written.lines().count(), 2);
        assert!(written.contains("\"raster_cache\":\"disabled\""));
        // A second call (window drop after an explicit exit path) is a no-op.
        std::fs::remove_file(&path).expect("remove");
        trace.write_once("gpu", Some(true));
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_platform_timebase_is_consistent() {
        // Windows: positive frequency and a monotonic counter; elsewhere
        // both unavailable. No PresentMon service is involved.
        match qpc_frequency() {
            Some(frequency) => {
                assert!(frequency > 0);
                let first = qpc_now().expect("QPC readable");
                let second = qpc_now().expect("QPC readable");
                assert!(second >= first);
            },
            None => {
                assert!(
                    !cfg!(windows),
                    "QueryPerformanceFrequency failed on Windows"
                );
                assert_eq!(qpc_now(), None);
            },
        }
    }
}
