//! `--roves-sustained` only: bounded per-redraw trace kept during warmup and sample and
//! printed only when the sustained run is rejected (`stall-trace` lines), std only.
//!
//! Every duration is wall-clock time read from the monotonic `Instant` clock on the winit
//! thread around one host call (same clock and caveats as `timings.rs`): it includes any
//! blocking wait inside the call and is neither CPU time nor GPU execution time. The ring is
//! a fixed array allocated once; recording a redraw writes one slot and allocates nothing.
//! It never feeds the sample statistics. Unit-tested stand-alone:
//! `rustc --edition 2024 --test src/stall.rs`.

use std::time::{Duration, Instant};

/// Redraws kept (the most recent ones; older ones are overwritten and counted).
pub(crate) const CAPACITY: usize = 128;

/// Host stages of one redraw, in call order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    /// `poll_web_event` until empty, plus handling (and logging) the drained events.
    Events,
    /// The single `acquire_frame` call (mailbox take, plus import when a frame was there).
    AcquireFrame,
    /// Netrender render with its internal submits, or the wgpu blit encode plus its submit.
    Compose,
    /// `surface.get_current_texture`.
    SurfaceAcquire,
    /// Encoding and submitting the composed -> surface blit.
    Submit,
    /// `pre_present_notify` + `present`.
    Present,
}

impl Step {
    pub(crate) const ALL: [Step; 6] = [
        Step::Events,
        Step::AcquireFrame,
        Step::Compose,
        Step::SurfaceAcquire,
        Step::Submit,
        Step::Present,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Step::Events => "events",
            Step::AcquireFrame => "acquire_frame",
            Step::Compose => "compose",
            Step::SurfaceAcquire => "surface_acquire",
            Step::Submit => "submit",
            Step::Present => "present",
        }
    }
}

/// Outcome of the redraw's mailbox poll.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Poll {
    /// `acquire_frame` returned `Ok(None)`: the mailbox was empty.
    #[default]
    Empty,
    /// A frame of this welding paint generation was imported.
    Imported(u64),
    /// `acquire_frame` returned an error (also logged as `acquire_frame error`).
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Redraw {
    /// Start of the redraw since the ring origin (warmup start), µs.
    pub start_us: u64,
    /// From the redraw start to the last mark, µs.
    pub total_us: u32,
    /// Per-step durations in `Step::ALL` order, µs; 0 for steps not reached.
    pub steps_us: [u32; Step::ALL.len()],
    pub poll: Poll,
    /// `present()` returned true.
    pub presented: bool,
    /// Recorded during the sample (false: warmup).
    pub sampling: bool,
}

fn micros(d: Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
}

/// Marks taken during one redraw.
pub(crate) struct Marks {
    start: Instant,
    last: Instant,
    steps_us: [u32; Step::ALL.len()],
}

impl Marks {
    pub(crate) fn new(start: Instant) -> Self {
        Marks {
            start,
            last: start,
            steps_us: [0; Step::ALL.len()],
        }
    }

    /// Ends `step` at `now` (it started at the previous mark).
    pub(crate) fn mark(&mut self, step: Step, now: Instant) {
        let i = Step::ALL.iter().position(|s| *s == step).unwrap_or(0);
        self.steps_us[i] = micros(now.saturating_duration_since(self.last));
        self.last = now;
    }
}

/// Fixed ring of the last `CAPACITY` redraws.
pub(crate) struct Ring {
    origin: Option<Instant>,
    entries: [Redraw; CAPACITY],
    next: usize,
    len: usize,
    overwritten: u64,
}

impl Default for Ring {
    fn default() -> Self {
        Ring {
            origin: None,
            entries: [Redraw::default(); CAPACITY],
            next: 0,
            len: 0,
            overwritten: 0,
        }
    }
}

/// Derived reading of the redraws after the last import in the ring.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct GapSummary {
    /// Whether a redraw that imported a frame is still in the ring. When false, every
    /// counter below covers the whole ring, not the whole gap.
    pub import_in_ring: bool,
    /// Generation imported by that redraw.
    pub last_generation: Option<u64>,
    /// Redraws after it.
    pub redraws: u32,
    pub empty_polls: u32,
    pub poll_errors: u32,
    pub presented: u32,
    /// Longest of those redraws, its offset and its longest step.
    pub max_redraw_us: u32,
    pub max_redraw_start_us: u64,
    pub max_redraw_step: Option<Step>,
    pub max_redraw_step_us: u32,
    /// Sum of their `total_us` (time spent inside redraws).
    pub busy_us: u64,
    /// Time from the end of the importing redraw (or the first redraw in the ring) to the end
    /// of the last redraw minus `busy_us`: time on the winit thread outside these redraws
    /// (event loop, other events, OS scheduling), not idle CPU.
    pub outside_us: u64,
}

impl Ring {
    /// Sets the time origin of `start_us` (the warmup start). Clears earlier entries.
    pub(crate) fn start(&mut self, origin: Instant) {
        *self = Ring {
            origin: Some(origin),
            ..Ring::default()
        };
    }

    pub(crate) fn push(&mut self, marks: &Marks, poll: Poll, presented: bool, sampling: bool) {
        let Some(origin) = self.origin else { return };
        self.entries[self.next] = Redraw {
            start_us: u64::try_from(marks.start.saturating_duration_since(origin).as_micros())
                .unwrap_or(u64::MAX),
            total_us: micros(marks.last.saturating_duration_since(marks.start)),
            steps_us: marks.steps_us,
            poll,
            presented,
            sampling,
        };
        self.next = (self.next + 1) % CAPACITY;
        if self.len == CAPACITY {
            self.overwritten += 1;
        } else {
            self.len += 1;
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Redraws recorded since `start` that are no longer in the ring.
    pub(crate) fn overwritten(&self) -> u64 {
        self.overwritten
    }

    /// Oldest first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Redraw> {
        let first = (self.next + CAPACITY - self.len) % CAPACITY;
        (0..self.len).map(move |i| &self.entries[(first + i) % CAPACITY])
    }

    pub(crate) fn gap_summary(&self) -> Option<GapSummary> {
        let entries: Vec<&Redraw> = self.iter().collect();
        let last = *entries.last()?;
        let import = entries
            .iter()
            .rposition(|r| matches!(r.poll, Poll::Imported(_)));
        let (after, from_us, last_generation) = match import {
            Some(i) => {
                let r = entries[i];
                let generation = match r.poll {
                    Poll::Imported(g) => Some(g),
                    _ => None,
                };
                (
                    &entries[i + 1..],
                    r.start_us + u64::from(r.total_us),
                    generation,
                )
            },
            None => (&entries[..], entries[0].start_us, None),
        };
        let mut s = GapSummary {
            import_in_ring: import.is_some(),
            last_generation,
            redraws: 0,
            empty_polls: 0,
            poll_errors: 0,
            presented: 0,
            max_redraw_us: 0,
            max_redraw_start_us: 0,
            max_redraw_step: None,
            max_redraw_step_us: 0,
            busy_us: 0,
            outside_us: 0,
        };
        for r in after {
            s.redraws += 1;
            s.busy_us += u64::from(r.total_us);
            match r.poll {
                Poll::Empty => s.empty_polls += 1,
                Poll::Error => s.poll_errors += 1,
                Poll::Imported(_) => {},
            }
            if r.presented {
                s.presented += 1;
            }
            if r.total_us >= s.max_redraw_us {
                s.max_redraw_us = r.total_us;
                s.max_redraw_start_us = r.start_us;
                let (step, us) = Step::ALL
                    .iter()
                    .zip(r.steps_us)
                    .max_by_key(|(_, us)| *us)
                    .map(|(s, us)| (*s, us))
                    .unwrap_or((Step::Events, 0));
                s.max_redraw_step = Some(step);
                s.max_redraw_step_us = us;
            }
        }
        if !after.is_empty() {
            let end = last.start_us + u64::from(last.total_us);
            s.outside_us = end.saturating_sub(from_us).saturating_sub(s.busy_us);
        }
        Some(s)
    }
}

/// One `stall-trace` line body for `r` (the caller adds the prefix).
pub(crate) fn format_redraw(index: usize, previous_end_us: Option<u64>, r: &Redraw) -> String {
    use std::fmt::Write as _;
    let mut line = format!(
        "i={index} phase={} start_us={}",
        if r.sampling { "sample" } else { "warmup" },
        r.start_us
    );
    match previous_end_us {
        Some(end) => {
            let _ = write!(
                line,
                " outside_before_us={}",
                r.start_us.saturating_sub(end)
            );
        },
        None => line.push_str(" outside_before_us=-"),
    }
    let _ = write!(line, " total_us={}", r.total_us);
    for (step, us) in Step::ALL.iter().zip(r.steps_us) {
        let _ = write!(line, " {}_us={us}", step.name());
    }
    match r.poll {
        Poll::Empty => line.push_str(" poll=empty"),
        Poll::Imported(g) => {
            let _ = write!(line, " poll=generation:{g}");
        },
        Poll::Error => line.push_str(" poll=error"),
    }
    let _ = write!(line, " presented={}", r.presented);
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(origin: Instant, start_ms: u64, steps_ms: [u64; 6]) -> Marks {
        let start = origin + Duration::from_millis(start_ms);
        let mut m = Marks::new(start);
        let mut t = start;
        for (step, ms) in Step::ALL.iter().zip(steps_ms) {
            t += Duration::from_millis(ms);
            m.mark(*step, t);
        }
        m
    }

    #[test]
    fn marks_and_push_record_offsets_and_steps() {
        let origin = Instant::now();
        let mut ring = Ring::default();
        // Not started: ignored.
        ring.push(&marks(origin, 0, [1; 6]), Poll::Empty, true, false);
        assert_eq!(ring.len(), 0);
        ring.start(origin);
        ring.push(
            &marks(origin, 5, [1, 2, 3, 4, 5, 6]),
            Poll::Imported(7),
            true,
            false,
        );
        let r = ring.iter().next().copied().unwrap();
        assert_eq!(r.start_us, 5_000);
        assert_eq!(r.total_us, 21_000);
        assert_eq!(r.steps_us, [1_000, 2_000, 3_000, 4_000, 5_000, 6_000]);
        assert_eq!(r.poll, Poll::Imported(7));
    }

    #[test]
    fn ring_is_bounded_and_ordered() {
        let origin = Instant::now();
        let mut ring = Ring::default();
        ring.start(origin);
        for i in 0..(CAPACITY as u64 + 10) {
            ring.push(
                &marks(origin, i * 17, [0; 6]),
                Poll::Imported(i),
                true,
                false,
            );
        }
        assert_eq!(ring.len(), CAPACITY);
        assert_eq!(ring.overwritten(), 10);
        let gens: Vec<u64> = ring
            .iter()
            .map(|r| match r.poll {
                Poll::Imported(g) => g,
                _ => u64::MAX,
            })
            .collect();
        assert_eq!(gens.first(), Some(&10));
        assert_eq!(gens.last(), Some(&(CAPACITY as u64 + 9)));
        assert!(gens.windows(2).all(|w| w[0] + 1 == w[1]));
    }

    #[test]
    fn gap_summary_names_the_blocking_redraw() {
        let origin = Instant::now();
        let mut ring = Ring::default();
        ring.start(origin);
        // Import at 0 (total 17 ms), one 600 ms redraw blocked in acquire_frame, one empty.
        ring.push(
            &marks(origin, 0, [1, 2, 2, 10, 1, 1]),
            Poll::Imported(40),
            true,
            false,
        );
        ring.push(
            &marks(origin, 18, [1, 600, 2, 1, 1, 1]),
            Poll::Empty,
            true,
            false,
        );
        ring.push(
            &marks(origin, 624, [1, 1, 1, 10, 1, 1]),
            Poll::Empty,
            true,
            false,
        );
        let s = ring.gap_summary().unwrap();
        assert!(s.import_in_ring);
        assert_eq!(s.last_generation, Some(40));
        assert_eq!(s.redraws, 2);
        assert_eq!(s.empty_polls, 2);
        assert_eq!(s.presented, 2);
        assert_eq!(s.max_redraw_us, 606_000);
        assert_eq!(s.max_redraw_start_us, 18_000);
        assert_eq!(s.max_redraw_step, Some(Step::AcquireFrame));
        assert_eq!(s.max_redraw_step_us, 600_000);
        assert_eq!(s.busy_us, 606_000 + 15_000);
        // From 17 ms to 639 ms = 622 ms, minus 621 ms inside redraws.
        assert_eq!(s.outside_us, 1_000);
    }

    #[test]
    fn gap_summary_without_import_covers_the_ring() {
        let origin = Instant::now();
        let mut ring = Ring::default();
        ring.start(origin);
        assert!(ring.gap_summary().is_none());
        ring.push(
            &marks(origin, 0, [0, 0, 1, 15, 0, 1]),
            Poll::Empty,
            true,
            false,
        );
        ring.push(
            &marks(origin, 17, [0, 0, 1, 15, 0, 1]),
            Poll::Error,
            false,
            true,
        );
        let s = ring.gap_summary().unwrap();
        assert!(!s.import_in_ring);
        assert_eq!(s.last_generation, None);
        assert_eq!(
            (s.redraws, s.empty_polls, s.poll_errors, s.presented),
            (2, 1, 1, 1)
        );
        assert_eq!(s.outside_us, 0);
    }

    #[test]
    fn format_lists_every_step() {
        let origin = Instant::now();
        let mut ring = Ring::default();
        ring.start(origin);
        ring.push(
            &marks(origin, 2, [1, 2, 3, 4, 5, 6]),
            Poll::Imported(9),
            true,
            true,
        );
        let r = ring.iter().next().unwrap();
        assert_eq!(
            format_redraw(0, Some(1_000), r),
            "i=0 phase=sample start_us=2000 outside_before_us=1000 total_us=21000 \
             events_us=1000 acquire_frame_us=2000 compose_us=3000 surface_acquire_us=4000 \
             submit_us=5000 present_us=6000 poll=generation:9 presented=true"
        );
        assert!(format_redraw(0, None, r).contains("outside_before_us=- "));
    }
}
