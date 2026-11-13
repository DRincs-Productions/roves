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
//! With the GPU window renderer two more blocks are reported:
//!
//! - `render_phases`: per-frame CPU wall-clock sums of the phases inside
//!   `cpu_render_present` (`Phase`), each in its own bounded series. A phase
//!   is sampled only on frames that reached it (no zero is invented: a frame
//!   whose segments were all cache hits has no `cpu_raster` sample), and every
//!   `render` call counts, including frames later skipped.
//!   `cpu_compose_segments` is the segment-to-layer loop of `compose` minus
//!   the nested `cpu_cache_check`/`cpu_cache_diff`/`cpu_raster`/
//!   `cpu_texture_upload` time, so
//!   no interval is counted twice. The remainder of
//!   `cpu_render_present` (work outside the phases, bookkeeping) is not
//!   attributed. `cpu_surface_acquire` and `cpu_present` may include
//!   blocking waits of the swapchain (FIFO/VSync, frame latency): they are not
//!   GPU work. `cpu_texture_upload` is the CPU side of `queue_write_texture`
//!   (staging copy), not the GPU transfer.
//! - `raster_cache_misses`: why each re-rasterized segment of a presented
//!   frame missed the raster cache (`MissReason`); the counts add up to
//!   `raster_uploads`.
//! - `raster_cache_content_diff`: for the `content_changed` misses of
//!   presented frames only, where the cached and the current segment first
//!   differ (`DiffCounts`). Aggregate counters only: no text, font, asset,
//!   scene content, command index or node id is recorded. `first_command`
//!   counts the kind of the current segment's first command that differs from
//!   the cached one at the same position; `length_only` counts segments whose
//!   common prefix is equal but whose length changed; `no_command_diff` counts
//!   segments with equal commands (a different scene tolerance). These three
//!   add up to `content_changed`. `length_changed` and `kind_changed` are
//!   overlapping flags (command count differs; the first differing commands
//!   are of different kinds). A command containing NaN never compares equal
//!   and is counted as a difference of its kind. When the first differing
//!   command is a `Fill`, `fill` buckets the area of its transformed bounding
//!   box (`AreaBucket`, squared scene units) and, only if the cached command
//!   at that position is a fill too (`compared`), counts which parts of the
//!   two `FillCommand`s differ (`FillChanges`: transform, brush, path, any
//!   other field such as the fill rule or brush transform when those three
//!   are equal, transformed bounds) as overlapping flags, plus `brush_only`.
//!   No geometry, color or paint value is kept. The comparison itself is
//!   timed as the `cpu_cache_diff` phase, nested in the segment loop like
//!   `cpu_cache_check`, so it is not counted in `cpu_raster`.
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

/// A measured phase of `GpuWindowRenderer::render` (CPU wall-clock only).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Blitz paint into the recording `Scene` (the `draw_fn` callback).
    ScenePaint,
    /// Widget pruning and `split_scene` (iteration over the recorded scene).
    Split,
    /// `surface_get_current_texture` (may wait for a free swapchain image).
    SurfaceAcquire,
    /// `RasterCache::is_current` for every raster segment.
    CacheCheck,
    /// Diagnostics only: the cached/current comparison of `content_changed`
    /// misses (`raster_cache_content_diff`), not part of the cache itself.
    CacheDiff,
    /// Vello CPU rasterization of the segments that missed the cache.
    Raster,
    /// `queue_write_texture` of those segments (CPU side only).
    TextureUpload,
    /// The `compose` loop turning segments into layers (raster texture
    /// lookup/creation, canvas texture lookup), excluding the nested
    /// `SEGMENT_NESTED` phases, which keep their own series.
    ComposeSegments,
    /// Layer uniforms, bind groups, command encoding and `queue_submit`.
    EncodeSubmit,
    /// `surface_present` (may block on VSync/frame latency).
    Present,
}

impl Phase {
    pub const ALL: [Phase; 10] = [
        Phase::ScenePaint,
        Phase::Split,
        Phase::SurfaceAcquire,
        Phase::CacheCheck,
        Phase::CacheDiff,
        Phase::Raster,
        Phase::TextureUpload,
        Phase::ComposeSegments,
        Phase::EncodeSubmit,
        Phase::Present,
    ];

    /// Phases measured inside the `ComposeSegments` loop.
    pub const SEGMENT_NESTED: [Phase; 4] = [
        Phase::CacheCheck,
        Phase::CacheDiff,
        Phase::Raster,
        Phase::TextureUpload,
    ];

    fn key(self) -> &'static str {
        match self {
            Phase::ScenePaint => "cpu_scene_paint",
            Phase::Split => "cpu_split",
            Phase::SurfaceAcquire => "cpu_surface_acquire",
            Phase::CacheCheck => "cpu_cache_check",
            Phase::CacheDiff => "cpu_cache_diff",
            Phase::Raster => "cpu_raster",
            Phase::TextureUpload => "cpu_texture_upload",
            Phase::ComposeSegments => "cpu_compose_segments",
            Phase::EncodeSubmit => "cpu_encode_submit",
            Phase::Present => "cpu_present",
        }
    }
}

/// Phase durations of one `render` call; `None` for phases not reached.
#[derive(Default, Debug, PartialEq)]
pub struct FramePhases([Option<Duration>; Phase::ALL.len()]);

impl FramePhases {
    /// Adds `duration` to `phase` (a phase may run once per segment).
    pub fn add(&mut self, phase: Phase, duration: Duration) {
        let slot = &mut self.0[phase as usize];
        *slot = Some(slot.unwrap_or_default() + duration);
    }

    pub fn get(&self, phase: Phase) -> Option<Duration> {
        self.0[phase as usize]
    }

    /// The summed durations of `phases` so far (unreached ones count zero).
    pub fn sum(&self, phases: &[Phase]) -> Duration {
        phases.iter().filter_map(|phase| self.get(*phase)).sum()
    }
}

/// Why a raster segment missed the cache and was rasterized and uploaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MissReason {
    /// `ROVES_PROBE_DISABLE_RASTER_CACHE=1`.
    CacheDisabled,
    /// The segment references external/mutable paint (`reusable` is false).
    NotReusable,
    /// The texture never held a segment while diagnostics were on.
    FirstUse,
    /// The texture holds a different (or NaN-containing) segment.
    ContentChanged,
    /// The previous upload into this texture reported an error.
    AfterUploadError,
    /// The previous segment in this texture was not reusable, so not cached.
    PreviousNotCached,
    /// The textures were recreated for a new viewport size.
    AfterResize,
    /// The cache was cleared because a frame was not presented.
    AfterDiscard,
}

impl MissReason {
    pub const ALL: [MissReason; 8] = [
        MissReason::CacheDisabled,
        MissReason::NotReusable,
        MissReason::FirstUse,
        MissReason::ContentChanged,
        MissReason::AfterUploadError,
        MissReason::PreviousNotCached,
        MissReason::AfterResize,
        MissReason::AfterDiscard,
    ];

    fn key(self) -> &'static str {
        match self {
            MissReason::CacheDisabled => "cache_disabled",
            MissReason::NotReusable => "not_reusable",
            MissReason::FirstUse => "first_use",
            MissReason::ContentChanged => "content_changed",
            MissReason::AfterUploadError => "after_upload_error",
            MissReason::PreviousNotCached => "previous_not_cached",
            MissReason::AfterResize => "after_resize",
            MissReason::AfterDiscard => "after_discard",
        }
    }
}

/// Bounded cache-miss counters, one per `MissReason`.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct MissCounts([u64; MissReason::ALL.len()]);

impl MissCounts {
    pub fn add(&mut self, reason: MissReason) {
        self.0[reason as usize] += 1;
    }

    pub fn merge(&mut self, other: &MissCounts) {
        for (total, count) in self.0.iter_mut().zip(other.0) {
            *total += count;
        }
    }

    pub fn get(&self, reason: MissReason) -> u64 {
        self.0[reason as usize]
    }

    #[cfg(test)]
    pub fn total(&self) -> u64 {
        self.0.iter().sum()
    }

    fn json(&self) -> String {
        let fields: Vec<String> = MissReason::ALL
            .iter()
            .map(|reason| format!("\"{}\":{}", reason.key(), self.get(*reason)))
            .collect();
        format!("{{{}}}", fields.join(","))
    }
}

/// The kind of a recorded `anyrender::recording::RenderCommand`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CommandKind {
    Fill,
    Stroke,
    GlyphRun,
    PushLayer,
    PushClipLayer,
    PopLayer,
    BoxShadow,
}

impl CommandKind {
    pub const ALL: [CommandKind; 7] = [
        CommandKind::Fill,
        CommandKind::Stroke,
        CommandKind::GlyphRun,
        CommandKind::PushLayer,
        CommandKind::PushClipLayer,
        CommandKind::PopLayer,
        CommandKind::BoxShadow,
    ];

    fn key(self) -> &'static str {
        match self {
            CommandKind::Fill => "fill",
            CommandKind::Stroke => "stroke",
            CommandKind::GlyphRun => "glyph_run",
            CommandKind::PushLayer => "push_layer",
            CommandKind::PushClipLayer => "push_clip_layer",
            CommandKind::PopLayer => "pop_layer",
            CommandKind::BoxShadow => "box_shadow",
        }
    }
}

/// Where a `content_changed` segment first differs from the cached one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FirstDiff {
    /// The current segment's first command unequal to the cached command at
    /// the same position.
    Command(CommandKind),
    /// The common prefix is equal; only the command count changed.
    LengthOnly,
    /// Equal commands (the scenes differ elsewhere, e.g. tolerance).
    NoCommandDiff,
}

/// Bucket of the area of a fill's transformed bounding box, in squared
/// scene units (the space the segment is rasterized in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AreaBucket {
    /// Below 32x32.
    Under1K,
    /// Below 128x128.
    Under16K,
    /// Below 512x512.
    Under256K,
    /// Below 1024x1024.
    Under1M,
    AtLeast1M,
    /// NaN or infinite bounds.
    NonFinite,
}

impl AreaBucket {
    pub const ALL: [AreaBucket; 6] = [
        AreaBucket::Under1K,
        AreaBucket::Under16K,
        AreaBucket::Under256K,
        AreaBucket::Under1M,
        AreaBucket::AtLeast1M,
        AreaBucket::NonFinite,
    ];

    /// `area` must be non-negative unless non-finite (`Rect::area` of an
    /// ordered rectangle); a negative value is treated by magnitude.
    pub fn of(area: f64) -> Self {
        let area = area.abs();
        if !area.is_finite() {
            AreaBucket::NonFinite
        } else if area < 1024.0 {
            AreaBucket::Under1K
        } else if area < 16_384.0 {
            AreaBucket::Under16K
        } else if area < 262_144.0 {
            AreaBucket::Under256K
        } else if area < 1_048_576.0 {
            AreaBucket::Under1M
        } else {
            AreaBucket::AtLeast1M
        }
    }

    fn key(self) -> &'static str {
        match self {
            AreaBucket::Under1K => "lt_1k",
            AreaBucket::Under16K => "lt_16k",
            AreaBucket::Under256K => "lt_256k",
            AreaBucket::Under1M => "lt_1m",
            AreaBucket::AtLeast1M => "ge_1m",
            AreaBucket::NonFinite => "non_finite",
        }
    }
}

/// Bucket of the number of path elements (`BezPath::elements().len()`) of a
/// fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ElementBucket {
    Empty,
    /// 1 to 4.
    UpTo4,
    /// 5 to 16.
    UpTo16,
    /// 17 to 64.
    UpTo64,
    /// 65 to 256.
    UpTo256,
    Over256,
}

impl ElementBucket {
    pub const ALL: [ElementBucket; 6] = [
        ElementBucket::Empty,
        ElementBucket::UpTo4,
        ElementBucket::UpTo16,
        ElementBucket::UpTo64,
        ElementBucket::UpTo256,
        ElementBucket::Over256,
    ];

    pub fn of(elements: usize) -> Self {
        match elements {
            0 => ElementBucket::Empty,
            1..=4 => ElementBucket::UpTo4,
            5..=16 => ElementBucket::UpTo16,
            17..=64 => ElementBucket::UpTo64,
            65..=256 => ElementBucket::UpTo256,
            _ => ElementBucket::Over256,
        }
    }

    fn key(self) -> &'static str {
        match self {
            ElementBucket::Empty => "empty",
            ElementBucket::UpTo4 => "le_4",
            ElementBucket::UpTo16 => "le_16",
            ElementBucket::UpTo64 => "le_64",
            ElementBucket::UpTo256 => "le_256",
            ElementBucket::Over256 => "gt_256",
        }
    }
}

/// The first stage of a fill's bounds computation that is not finite, in
/// pipeline order: path element points, fill transform, untransformed
/// bounding box (`Shape::bounding_box`), transformed bounding box
/// (`Affine::transform_rect_bbox`), then its area (`Rect::area`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoundsOrigin {
    /// Every stage, area included, is finite.
    Finite,
    /// At least one path element point is NaN or infinite.
    PathNonFinite,
    /// Finite path, non-finite transform coefficient.
    TransformNonFinite,
    /// Finite path and transform, non-finite untransformed bounding box.
    LocalBoxNonFinite,
    /// Finite untransformed box, non-finite transformed box.
    TransformedBoxNonFinite,
    /// Finite transformed box whose area overflows.
    AreaNonFinite,
}

impl BoundsOrigin {
    pub const ALL: [BoundsOrigin; 6] = [
        BoundsOrigin::Finite,
        BoundsOrigin::PathNonFinite,
        BoundsOrigin::TransformNonFinite,
        BoundsOrigin::LocalBoxNonFinite,
        BoundsOrigin::TransformedBoxNonFinite,
        BoundsOrigin::AreaNonFinite,
    ];

    /// Classifies by stage finiteness flags, earliest non-finite first.
    pub fn of(
        path: bool,
        transform: bool,
        local_box: bool,
        transformed_box: bool,
        area: bool,
    ) -> Self {
        if !path {
            BoundsOrigin::PathNonFinite
        } else if !transform {
            BoundsOrigin::TransformNonFinite
        } else if !local_box {
            BoundsOrigin::LocalBoxNonFinite
        } else if !transformed_box {
            BoundsOrigin::TransformedBoxNonFinite
        } else if !area {
            BoundsOrigin::AreaNonFinite
        } else {
            BoundsOrigin::Finite
        }
    }

    fn key(self) -> &'static str {
        match self {
            BoundsOrigin::Finite => "finite",
            BoundsOrigin::PathNonFinite => "path_non_finite",
            BoundsOrigin::TransformNonFinite => "transform_non_finite",
            BoundsOrigin::LocalBoxNonFinite => "local_box_non_finite",
            BoundsOrigin::TransformedBoxNonFinite => "transformed_box_non_finite",
            BoundsOrigin::AreaNonFinite => "area_non_finite",
        }
    }
}

/// Classes of non-finite path point coordinates: the `PathEl` point slot
/// (kind and position within the element), the value class and the axis.
/// Overlapping: one non-finite coordinate sets one slot, one value class and
/// one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NonFiniteClass {
    /// `MoveTo(p)`.
    MoveTo,
    /// `LineTo(p)`.
    LineTo,
    /// `QuadTo(p1, _)`.
    QuadCtrl,
    /// `QuadTo(_, p2)`.
    QuadEnd,
    /// `CurveTo(p1, _, _)`.
    CurveCtrl1,
    /// `CurveTo(_, p2, _)`.
    CurveCtrl2,
    /// `CurveTo(_, _, p3)`.
    CurveEnd,
    Nan,
    PosInf,
    NegInf,
    X,
    Y,
}

impl NonFiniteClass {
    pub const ALL: [NonFiniteClass; 12] = [
        NonFiniteClass::MoveTo,
        NonFiniteClass::LineTo,
        NonFiniteClass::QuadCtrl,
        NonFiniteClass::QuadEnd,
        NonFiniteClass::CurveCtrl1,
        NonFiniteClass::CurveCtrl2,
        NonFiniteClass::CurveEnd,
        NonFiniteClass::Nan,
        NonFiniteClass::PosInf,
        NonFiniteClass::NegInf,
        NonFiniteClass::X,
        NonFiniteClass::Y,
    ];

    fn key(self) -> &'static str {
        match self {
            NonFiniteClass::MoveTo => "move_to",
            NonFiniteClass::LineTo => "line_to",
            NonFiniteClass::QuadCtrl => "quad_to_ctrl",
            NonFiniteClass::QuadEnd => "quad_to_end",
            NonFiniteClass::CurveCtrl1 => "curve_to_ctrl1",
            NonFiniteClass::CurveCtrl2 => "curve_to_ctrl2",
            NonFiniteClass::CurveEnd => "curve_to_end",
            NonFiniteClass::Nan => "nan",
            NonFiniteClass::PosInf => "pos_inf",
            NonFiniteClass::NegInf => "neg_inf",
            NonFiniteClass::X => "x",
            NonFiniteClass::Y => "y",
        }
    }

    /// The value class of a non-finite coordinate; `None` if finite.
    fn of_value(value: f64) -> Option<Self> {
        if value.is_nan() {
            Some(NonFiniteClass::Nan)
        } else if value == f64::INFINITY {
            Some(NonFiniteClass::PosInf)
        } else if value == f64::NEG_INFINITY {
            Some(NonFiniteClass::NegInf)
        } else {
            None
        }
    }
}

/// Which `NonFiniteClass`es occur at least once in one path (presence
/// flags only: no count, position or value is kept).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NonFiniteSlots(u16);

impl NonFiniteSlots {
    pub const NONE: Self = Self(0);

    pub fn contains(self, class: NonFiniteClass) -> bool {
        self.0 & (1 << class as u16) != 0
    }

    fn insert(&mut self, class: NonFiniteClass) {
        self.0 |= 1 << class as u16;
    }

    /// Records the point `(x, y)` found at `slot` (a point-slot class).
    pub fn point(&mut self, slot: NonFiniteClass, x: f64, y: f64) {
        for (value, axis) in [(x, NonFiniteClass::X), (y, NonFiniteClass::Y)] {
            if let Some(class) = NonFiniteClass::of_value(value) {
                self.insert(slot);
                self.insert(class);
                self.insert(axis);
            }
        }
    }
}

/// Aggregate shape class of one fill path; no geometry is kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathClass {
    pub elements: ElementBucket,
    pub origin: BoundsOrigin,
    pub non_finite: NonFiniteSlots,
}

/// Bounded counters of `PathClass`es (fixed size, no content).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct PathCounts {
    elements: [u64; ElementBucket::ALL.len()],
    origin: [u64; BoundsOrigin::ALL.len()],
    /// Paths with at least one coordinate of each class.
    non_finite: [u64; NonFiniteClass::ALL.len()],
}

impl PathCounts {
    pub fn add(&mut self, class: PathClass) {
        self.elements[class.elements as usize] += 1;
        self.origin[class.origin as usize] += 1;
        for slot in NonFiniteClass::ALL {
            self.non_finite[slot as usize] += u64::from(class.non_finite.contains(slot));
        }
    }

    pub fn merge(&mut self, other: &PathCounts) {
        for (total, count) in self.elements.iter_mut().zip(other.elements) {
            *total += count;
        }
        for (total, count) in self.origin.iter_mut().zip(other.origin) {
            *total += count;
        }
        for (total, count) in self.non_finite.iter_mut().zip(other.non_finite) {
            *total += count;
        }
    }

    #[cfg(test)]
    pub fn non_finite(&self, class: NonFiniteClass) -> u64 {
        self.non_finite[class as usize]
    }

    #[cfg(test)]
    pub fn elements(&self, bucket: ElementBucket) -> u64 {
        self.elements[bucket as usize]
    }

    #[cfg(test)]
    pub fn origin(&self, origin: BoundsOrigin) -> u64 {
        self.origin[origin as usize]
    }

    fn json(&self) -> String {
        let elements: Vec<String> = ElementBucket::ALL
            .iter()
            .map(|bucket| format!("\"{}\":{}", bucket.key(), self.elements[*bucket as usize]))
            .collect();
        let origin: Vec<String> = BoundsOrigin::ALL
            .iter()
            .map(|origin| format!("\"{}\":{}", origin.key(), self.origin[*origin as usize]))
            .collect();
        let non_finite: Vec<String> = NonFiniteClass::ALL
            .iter()
            .map(|class| format!("\"{}\":{}", class.key(), self.non_finite[*class as usize]))
            .collect();
        format!(
            "{{\"elements\":{{{}}},\"bounds\":{{{}}},\"non_finite\":{{{}}}}}",
            elements.join(","),
            origin.join(","),
            non_finite.join(",")
        )
    }
}

/// How a cached and a current fill path compare when every point
/// coordinate is compared by `f64::to_bits` (so a NaN equals itself bit for
/// bit, unlike `PartialEq`, and `-0.0` differs from `0.0`). The first
/// matching case wins in this order: count, verbs, points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PathBits {
    /// Same element count, kinds and coordinate bits.
    #[default]
    SameBitwise,
    /// Different `BezPath::elements().len()`.
    CountMismatch,
    /// Same count, at least one `PathEl` kind differs.
    VerbMismatch,
    /// Same count and kinds, at least one coordinate's bits differ.
    PointBitsDiffer,
}

impl PathBits {
    pub const ALL: [PathBits; 4] = [
        PathBits::SameBitwise,
        PathBits::CountMismatch,
        PathBits::VerbMismatch,
        PathBits::PointBitsDiffer,
    ];

    fn key(self) -> &'static str {
        match self {
            PathBits::SameBitwise => "same_bitwise_path",
            PathBits::CountMismatch => "count_mismatch",
            PathBits::VerbMismatch => "verb_mismatch",
            PathBits::PointBitsDiffer => "point_bits_differ",
        }
    }
}

/// Which parts of the cached and the current first differing command
/// differ, when both are fills (overlapping flags).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FillChanges {
    pub transform: bool,
    /// The paint (color, gradient, image or resource).
    pub brush: bool,
    /// The path itself.
    pub shape: bool,
    /// Transform, brush and path are equal, so another field differs (the
    /// fill rule or the brush transform).
    pub other: bool,
    /// The transformed bounding box (position or size on the raster).
    pub bounds: bool,
    /// The paths compared by coordinate bits (not part of `brush_only`).
    pub path_bits: PathBits,
}

/// Classification of a first differing command that is a `Fill`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillDiff {
    /// Bucket of the current fill's transformed bounding-box area.
    pub area: AreaBucket,
    /// Path class of the current fill.
    pub current: PathClass,
    /// `None` when the cached command at that position is not a fill.
    pub changes: Option<FillChanges>,
    /// Path class of the cached fill; set together with `changes`.
    pub cached: Option<PathClass>,
}

/// Classification of one `content_changed` miss; no content is kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentDiff {
    pub first: FirstDiff,
    /// The command counts differ.
    pub length_changed: bool,
    /// The first differing commands are of different kinds.
    pub kind_changed: bool,
    /// Set only when `first` is `Command(CommandKind::Fill)`.
    pub fill: Option<FillDiff>,
}

/// Bounded counters of `FillDiff`s (fixed size, no content).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct FillCounts {
    area: [u64; AreaBucket::ALL.len()],
    /// Indexed like `FILL_COUNTERS`.
    counters: [u64; FILL_COUNTERS.len()],
    /// Only fill-vs-fill comparisons (sums to `compared`).
    path_bits: [u64; PathBits::ALL.len()],
    /// `shape_changed` (`PartialEq`) although the paths are bitwise equal:
    /// a subset of `same_bitwise_path`, only possible with NaN coordinates.
    shape_changed_same_bits: u64,
    /// Every first-differing fill.
    current_path: PathCounts,
    /// Only fill-vs-fill comparisons (sums to `compared`).
    cached_path: PathCounts,
}

/// `compared` counts fill-vs-fill comparisons; the `*_changed` flags count
/// only those. `brush_only`: only the brush differs (same transform, path,
/// other fields and bounds).
const FILL_COUNTERS: [&str; 7] = [
    "compared",
    "transform_changed",
    "brush_changed",
    "shape_changed",
    "other_changed",
    "bounds_changed",
    "brush_only",
];

impl FillCounts {
    pub fn add(&mut self, diff: FillDiff) {
        self.area[diff.area as usize] += 1;
        self.current_path.add(diff.current);
        if let (Some(_), Some(cached)) = (diff.changes, diff.cached) {
            self.cached_path.add(cached);
        }
        if let Some(changes) = diff.changes {
            let brush_only = FillChanges {
                path_bits: PathBits::default(),
                ..changes
            } == FillChanges {
                brush: true,
                ..FillChanges::default()
            };
            self.path_bits[changes.path_bits as usize] += 1;
            self.shape_changed_same_bits +=
                u64::from(changes.shape && changes.path_bits == PathBits::SameBitwise);
            let flags = [
                true,
                changes.transform,
                changes.brush,
                changes.shape,
                changes.other,
                changes.bounds,
                brush_only,
            ];
            for (counter, flag) in self.counters.iter_mut().zip(flags) {
                *counter += u64::from(flag);
            }
        }
    }

    fn fields(&self) -> impl Iterator<Item = (&'static str, u64)> {
        FILL_COUNTERS.into_iter().zip(self.counters)
    }

    pub fn merge(&mut self, other: &FillCounts) {
        for (total, count) in self.area.iter_mut().zip(other.area) {
            *total += count;
        }
        for (total, count) in self.counters.iter_mut().zip(other.counters) {
            *total += count;
        }
        for (total, count) in self.path_bits.iter_mut().zip(other.path_bits) {
            *total += count;
        }
        self.shape_changed_same_bits += other.shape_changed_same_bits;
        self.current_path.merge(&other.current_path);
        self.cached_path.merge(&other.cached_path);
    }

    #[cfg(test)]
    pub fn area(&self, bucket: AreaBucket) -> u64 {
        self.area[bucket as usize]
    }

    #[cfg(test)]
    pub fn current_path(&self) -> &PathCounts {
        &self.current_path
    }

    #[cfg(test)]
    pub fn cached_path(&self) -> &PathCounts {
        &self.cached_path
    }

    #[cfg(test)]
    pub fn path_bits(&self, bits: PathBits) -> u64 {
        self.path_bits[bits as usize]
    }

    #[cfg(test)]
    pub fn shape_changed_same_bits(&self) -> u64 {
        self.shape_changed_same_bits
    }

    #[cfg(test)]
    pub fn field(&self, key: &str) -> u64 {
        self.fields()
            .find(|(name, _)| *name == key)
            .map(|(_, count)| count)
            .expect("known fill counter")
    }

    fn json(&self) -> String {
        let areas: Vec<String> = AreaBucket::ALL
            .iter()
            .map(|bucket| format!("\"{}\":{}", bucket.key(), self.area[*bucket as usize]))
            .collect();
        let fields: Vec<String> = self
            .fields()
            .map(|(key, count)| format!("\"{key}\":{count}"))
            .collect();
        let bits: Vec<String> = PathBits::ALL
            .iter()
            .map(|bits| format!("\"{}\":{}", bits.key(), self.path_bits[*bits as usize]))
            .collect();
        format!(
            "{{\"area\":{{{}}},{},\"path_bits\":{{{},\"shape_changed_same_bits\":{}}},\"current_path\":{},\"cached_path\":{}}}",
            areas.join(","),
            fields.join(","),
            bits.join(","),
            self.shape_changed_same_bits,
            self.current_path.json(),
            self.cached_path.json()
        )
    }
}

/// Bounded counters of `ContentDiff`s (fixed size, no content).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct DiffCounts {
    first_command: [u64; CommandKind::ALL.len()],
    length_only: u64,
    no_command_diff: u64,
    length_changed: u64,
    kind_changed: u64,
    fill: FillCounts,
}

impl DiffCounts {
    pub fn add(&mut self, diff: ContentDiff) {
        match diff.first {
            FirstDiff::Command(kind) => self.first_command[kind as usize] += 1,
            FirstDiff::LengthOnly => self.length_only += 1,
            FirstDiff::NoCommandDiff => self.no_command_diff += 1,
        }
        self.length_changed += u64::from(diff.length_changed);
        self.kind_changed += u64::from(diff.kind_changed);
        if let (FirstDiff::Command(CommandKind::Fill), Some(fill)) = (diff.first, diff.fill) {
            self.fill.add(fill);
        }
    }

    #[cfg(test)]
    pub fn fill(&self) -> &FillCounts {
        &self.fill
    }

    pub fn merge(&mut self, other: &DiffCounts) {
        for (total, count) in self.first_command.iter_mut().zip(other.first_command) {
            *total += count;
        }
        self.length_only += other.length_only;
        self.no_command_diff += other.no_command_diff;
        self.length_changed += other.length_changed;
        self.kind_changed += other.kind_changed;
        self.fill.merge(&other.fill);
    }

    #[cfg(test)]
    pub fn first_command(&self, kind: CommandKind) -> u64 {
        self.first_command[kind as usize]
    }

    /// Classified misses: equals `content_changed` of the same frames.
    #[cfg(test)]
    pub fn total(&self) -> u64 {
        self.first_command.iter().sum::<u64>() + self.length_only + self.no_command_diff
    }

    fn json(&self) -> String {
        let kinds: Vec<String> = CommandKind::ALL
            .iter()
            .map(|kind| format!("\"{}\":{}", kind.key(), self.first_command[*kind as usize]))
            .collect();
        format!(
            "{{\"first_command\":{{{}}},\"length_only\":{},\"no_command_diff\":{},\"length_changed\":{},\"kind_changed\":{},\"fill\":{}}}",
            kinds.join(","),
            self.length_only,
            self.no_command_diff,
            self.length_changed,
            self.kind_changed,
            self.fill.json(),
        )
    }
}

/// One bounded series per `Phase`.
pub struct PhaseSamples([Samples; Phase::ALL.len()]);

impl PhaseSamples {
    fn new() -> Self {
        Self(std::array::from_fn(|_| Samples::with_capacity(MAX_SAMPLES)))
    }

    /// Samples the phases `frame` reached; the others get no sample.
    pub fn record(&mut self, frame: &FramePhases) {
        for phase in Phase::ALL {
            if let Some(duration) = frame.get(phase) {
                self.0[phase as usize].push(duration);
            }
        }
    }

    fn json(&self) -> String {
        let fields: Vec<String> = Phase::ALL
            .iter()
            .map(|phase| {
                let summary = self.0[*phase as usize].summary();
                format!("\"{}\":{}", phase.key(), summary.json())
            })
            .collect();
        format!("{{{}}}", fields.join(","))
    }
}

pub struct FrameTimings {
    pub frame_step_interval: Samples,
    pub cpu_frame_step: Samples,
    pub cpu_render_present: Samples,
    /// GPU renderer only, created on its first frame.
    render_phases: Option<Box<PhaseSamples>>,
    /// Committed cache-miss reasons of the GPU renderer, if any.
    pub raster_cache_misses: Option<MissCounts>,
    /// Committed `content_changed` classifications of the GPU renderer.
    pub raster_cache_content_diff: Option<DiffCounts>,
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
            render_phases: None,
            raster_cache_misses: None,
            raster_cache_content_diff: None,
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

    /// Samples the phases of one GPU `render` call.
    pub fn record_render_phases(&mut self, frame: &FramePhases) {
        self.render_phases
            .get_or_insert_with(|| Box::new(PhaseSamples::new()))
            .record(frame);
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
        let render_phases = self
            .render_phases
            .as_ref()
            .map_or_else(|| "null".to_owned(), |phases| phases.json());
        let misses = self
            .raster_cache_misses
            .as_ref()
            .map_or_else(|| "null".to_owned(), MissCounts::json);
        let content_diff = self
            .raster_cache_content_diff
            .as_ref()
            .map_or_else(|| "null".to_owned(), DiffCounts::json);
        println!(
            "ROVES_PROBE_FRAME_TIMINGS {{\"check\":\"frame_timings\",\"unit\":\"ms\",\"window_max_samples\":{MAX_SAMPLES},\"renderer\":\"{}\",\"raster_cache\":{raster_cache},{counters},\"frame_step_interval\":{},\"cpu_frame_step\":{},\"cpu_render_present\":{},\"render_phases\":{render_phases},\"raster_cache_misses\":{misses},\"raster_cache_content_diff\":{content_diff},\"note\":\"cpu_* are shell-thread wall-clock durations; cpu_render_present is not GPU time and does not prove a visible frame\",\"phase_note\":\"render_phases are per-frame CPU sums sampled only on frames that reached the phase (all render calls, skipped ones included); cpu_compose_segments is the compose segment loop minus its nested cpu_cache_check/cpu_cache_diff/cpu_raster/cpu_texture_upload time; cpu_cache_diff is diagnostics-only comparison work; cpu_surface_acquire and cpu_present may include swapchain/VSync waits; the unattributed remainder of cpu_render_present is not reported; raster_cache_misses cover presented frames and add up to raster_uploads; raster_cache_content_diff covers the same presented content_changed misses (first_command + length_only + no_command_diff = content_changed; length_changed and kind_changed are overlapping flags; fill.area buckets the transformed bounding-box area, in squared scene units, of first-differing fills, and fill.compared counts those whose cached command is also a fill, the only ones carrying the overlapping *_changed flags (other_changed: transform, brush and path are equal); brush_only is the subset where only the brush differs; fill.current_path covers every first-differing fill and fill.cached_path only the compared ones: elements buckets the path element count, bounds names the first non-finite stage in the order path points, transform, untransformed bounding box, transformed bounding box, area, or finite)\"}}",
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

    #[test]
    fn phases_accumulate_and_unreached_phases_get_no_sample() {
        let mut frame = FramePhases::default();
        assert_eq!(frame.get(Phase::Raster), None);
        // Two segments rasterized in one frame are summed.
        frame.add(Phase::Raster, Duration::from_millis(3));
        frame.add(Phase::Raster, Duration::from_millis(4));
        frame.add(Phase::Present, Duration::from_millis(1));
        assert_eq!(frame.get(Phase::Raster), Some(Duration::from_millis(7)));

        let mut samples = PhaseSamples::new();
        samples.record(&frame);
        samples.record(&FramePhases::default());
        let raster = samples.0[Phase::Raster as usize].summary();
        assert_eq!((raster.count, raster.p50), (1, Some(7.0)));
        // Never reached: an empty series, reported as null.
        assert_eq!(samples.0[Phase::TextureUpload as usize].summary().count, 0);
        let json = samples.json();
        assert!(json.contains("\"cpu_texture_upload\":{\"count\":0,\"total\":0,\"p50\":null"));
        assert!(json.contains("\"cpu_present\":{\"count\":1"));
        assert_eq!(json.matches("\"cpu_").count(), Phase::ALL.len());
    }

    #[test]
    fn compose_segments_is_a_separate_series_and_nested_phases_sum() {
        let mut frame = FramePhases::default();
        assert_eq!(frame.sum(&Phase::SEGMENT_NESTED), Duration::ZERO);
        frame.add(Phase::CacheCheck, Duration::from_millis(1));
        frame.add(Phase::CacheCheck, Duration::from_millis(1));
        frame.add(Phase::Raster, Duration::from_millis(5));
        frame.add(Phase::EncodeSubmit, Duration::from_millis(9));
        frame.add(Phase::ComposeSegments, Duration::from_millis(2));
        // Encode/submit and the loop itself are not nested segment phases.
        assert_eq!(frame.sum(&Phase::SEGMENT_NESTED), Duration::from_millis(7));
        assert_eq!(frame.get(Phase::CacheCheck), Some(Duration::from_millis(2)));

        let mut samples = PhaseSamples::new();
        samples.record(&frame);
        let json = samples.json();
        assert!(json.contains("\"cpu_compose_segments\":{\"count\":1,\"total\":1,\"p50\":2.000"));
        assert!(json.contains("\"cpu_cache_check\":{\"count\":1,\"total\":1,\"p50\":2.000"));
        assert!(json.contains("\"cpu_encode_submit\":{\"count\":1,\"total\":1,\"p50\":9.000"));
    }

    #[test]
    fn miss_counts_merge_and_serialize_every_reason() {
        let mut frame = MissCounts::default();
        frame.add(MissReason::FirstUse);
        frame.add(MissReason::ContentChanged);
        frame.add(MissReason::ContentChanged);
        let mut total = MissCounts::default();
        total.merge(&frame);
        total.merge(&frame);
        assert_eq!(total.get(MissReason::ContentChanged), 4);
        assert_eq!(total.total(), 6);
        let json = total.json();
        assert!(json.contains("\"first_use\":2"));
        assert!(json.contains("\"after_discard\":0"));
        assert_eq!(json.matches(':').count(), MissReason::ALL.len());
    }

    #[test]
    fn diff_counts_merge_and_serialize_only_aggregates() {
        let mut frame = DiffCounts::default();
        frame.add(ContentDiff {
            first: FirstDiff::Command(CommandKind::GlyphRun),
            length_changed: false,
            kind_changed: false,
            fill: None,
        });
        frame.add(ContentDiff {
            first: FirstDiff::Command(CommandKind::Fill),
            length_changed: true,
            kind_changed: true,
            fill: Some(FillDiff {
                area: AreaBucket::AtLeast1M,
                current: FINITE_RECT,
                changes: None,
                cached: None,
            }),
        });
        frame.add(ContentDiff {
            first: FirstDiff::LengthOnly,
            length_changed: true,
            kind_changed: false,
            fill: None,
        });
        frame.add(ContentDiff {
            first: FirstDiff::NoCommandDiff,
            length_changed: false,
            kind_changed: false,
            fill: None,
        });
        let mut total = DiffCounts::default();
        total.merge(&frame);
        total.merge(&frame);
        assert_eq!(total.first_command(CommandKind::GlyphRun), 2);
        assert_eq!(total.first_command(CommandKind::Fill), 2);
        // Flags overlap; the classification sums to the classified misses.
        assert_eq!(total.total(), 8);
        let json = total.json();
        assert!(json.contains("\"glyph_run\":2"));
        assert!(json.contains("\"box_shadow\":0"));
        assert!(json.contains("\"length_only\":2,\"no_command_diff\":2"));
        assert!(json.contains("\"length_changed\":4,\"kind_changed\":2"));
        // The fill vs non-fill difference has an area but no field flags.
        assert_eq!(total.fill().area(AreaBucket::AtLeast1M), 2);
        assert_eq!(total.fill().field("compared"), 0);
        assert!(json.contains("\"fill\":{\"area\":{\"lt_1k\":0,"));
        assert!(json.contains("\"ge_1m\":2,\"non_finite\":0},\"compared\":0,"));
        // Not compared: only the current path is classified.
        assert_eq!(total.fill().current_path().origin(BoundsOrigin::Finite), 2);
        assert_eq!(*total.fill().cached_path(), PathCounts::default());
        // first_command + 7 kinds + 4 scalars + fill + area + 6 buckets +
        // 7 fill counters + path_bits (4 + 1) + 2 path blocks of (elements +
        // bounds + non_finite + 6 + 6 + 12): fixed size.
        assert_eq!(
            json.matches(':').count(),
            1 + CommandKind::ALL.len()
                + 4
                + 2
                + AreaBucket::ALL.len()
                + FILL_COUNTERS.len()
                + 1
                + PathBits::ALL.len()
                + 1
                + 2 * (4
                    + ElementBucket::ALL.len()
                    + BoundsOrigin::ALL.len()
                    + NonFiniteClass::ALL.len())
        );
    }

    /// A finite 4-segment rectangle path.
    const FINITE_RECT: PathClass = PathClass {
        elements: ElementBucket::UpTo16,
        origin: BoundsOrigin::Finite,
        non_finite: NonFiniteSlots::NONE,
    };

    #[test]
    fn non_finite_slots_record_slot_value_and_axis_presence() {
        let mut slots = NonFiniteSlots::default();
        slots.point(NonFiniteClass::LineTo, 1.0, 2.0);
        assert_eq!(slots, NonFiniteSlots::NONE);
        slots.point(NonFiniteClass::LineTo, f64::NAN, 2.0);
        slots.point(NonFiniteClass::CurveCtrl2, 0.0, f64::NEG_INFINITY);
        slots.point(NonFiniteClass::LineTo, f64::NAN, f64::NAN);
        use NonFiniteClass::*;
        let present = [LineTo, CurveCtrl2, Nan, NegInf, X, Y];
        for class in NonFiniteClass::ALL {
            assert_eq!(slots.contains(class), present.contains(&class), "{class:?}");
        }
        let mut counts = PathCounts::default();
        counts.add(PathClass {
            elements: ElementBucket::UpTo16,
            origin: BoundsOrigin::PathNonFinite,
            non_finite: slots,
        });
        counts.add(PathClass {
            elements: ElementBucket::UpTo16,
            origin: BoundsOrigin::PathNonFinite,
            non_finite: slots,
        });
        // Presence per path, not per coordinate.
        assert_eq!(counts.non_finite(LineTo), 2);
        assert_eq!(counts.non_finite(Nan), 2);
        assert_eq!(counts.non_finite(PosInf), 0);
        assert!(counts.json().ends_with(
            "\"non_finite\":{\"move_to\":0,\"line_to\":2,\"quad_to_ctrl\":0,\"quad_to_end\":0,\"curve_to_ctrl1\":0,\"curve_to_ctrl2\":2,\"curve_to_end\":0,\"nan\":2,\"pos_inf\":0,\"neg_inf\":2,\"x\":2,\"y\":2}}"
        ));
    }

    #[test]
    fn fill_counts_classify_path_bits() {
        let compare = |shape, path_bits| FillDiff {
            area: AreaBucket::Under1K,
            current: FINITE_RECT,
            changes: Some(FillChanges {
                shape,
                path_bits,
                ..FillChanges::default()
            }),
            cached: Some(FINITE_RECT),
        };
        let mut frame = FillCounts::default();
        // NaN paths: PartialEq differs, bits are equal.
        frame.add(compare(true, PathBits::SameBitwise));
        frame.add(compare(true, PathBits::CountMismatch));
        frame.add(compare(true, PathBits::VerbMismatch));
        frame.add(compare(true, PathBits::PointBitsDiffer));
        // -0.0 vs 0.0: PartialEq equal, bits differ.
        frame.add(compare(false, PathBits::PointBitsDiffer));
        // Not compared: no path_bits.
        frame.add(FillDiff {
            changes: None,
            cached: None,
            ..compare(true, PathBits::SameBitwise)
        });
        let mut total = FillCounts::default();
        total.merge(&frame);
        assert_eq!(total.field("compared"), 5);
        assert_eq!(total.path_bits(PathBits::SameBitwise), 1);
        assert_eq!(total.path_bits(PathBits::CountMismatch), 1);
        assert_eq!(total.path_bits(PathBits::VerbMismatch), 1);
        assert_eq!(total.path_bits(PathBits::PointBitsDiffer), 2);
        assert_eq!(total.shape_changed_same_bits(), 1);
        assert!(total.json().contains(
            "\"path_bits\":{\"same_bitwise_path\":1,\"count_mismatch\":1,\"verb_mismatch\":1,\"point_bits_differ\":2,\"shape_changed_same_bits\":1}"
        ));
        // path_bits does not affect brush_only.
        let mut brush = FillCounts::default();
        brush.add(FillDiff {
            changes: Some(FillChanges {
                brush: true,
                path_bits: PathBits::PointBitsDiffer,
                ..FillChanges::default()
            }),
            ..compare(false, PathBits::SameBitwise)
        });
        assert_eq!(brush.field("brush_only"), 1);
    }

    #[test]
    fn element_buckets_have_fixed_bounds() {
        let cases = [
            (0, ElementBucket::Empty),
            (1, ElementBucket::UpTo4),
            (4, ElementBucket::UpTo4),
            (5, ElementBucket::UpTo16),
            (16, ElementBucket::UpTo16),
            (17, ElementBucket::UpTo64),
            (64, ElementBucket::UpTo64),
            (65, ElementBucket::UpTo256),
            (256, ElementBucket::UpTo256),
            (257, ElementBucket::Over256),
            (usize::MAX, ElementBucket::Over256),
        ];
        for (elements, bucket) in cases {
            assert_eq!(ElementBucket::of(elements), bucket, "{elements}");
        }
    }

    #[test]
    fn bounds_origin_reports_the_earliest_non_finite_stage() {
        use BoundsOrigin::*;
        assert_eq!(BoundsOrigin::of(true, true, true, true, true), Finite);
        // Every later stage is non-finite too: the earliest one wins.
        assert_eq!(
            BoundsOrigin::of(false, false, false, false, false),
            PathNonFinite
        );
        assert_eq!(
            BoundsOrigin::of(true, false, false, false, false),
            TransformNonFinite
        );
        assert_eq!(
            BoundsOrigin::of(true, true, false, false, false),
            LocalBoxNonFinite
        );
        assert_eq!(
            BoundsOrigin::of(true, true, true, false, false),
            TransformedBoxNonFinite
        );
        assert_eq!(
            BoundsOrigin::of(true, true, true, true, false),
            AreaNonFinite
        );
        // A non-finite path is reported even if its box were finite.
        assert_eq!(
            BoundsOrigin::of(false, true, true, true, true),
            PathNonFinite
        );
    }

    #[test]
    fn path_counts_merge_and_serialize_fixed_buckets() {
        let mut frame = PathCounts::default();
        frame.add(FINITE_RECT);
        frame.add(PathClass {
            elements: ElementBucket::Empty,
            origin: BoundsOrigin::LocalBoxNonFinite,
            non_finite: NonFiniteSlots::NONE,
        });
        let mut total = PathCounts::default();
        total.merge(&frame);
        total.merge(&frame);
        assert_eq!(total.elements(ElementBucket::UpTo16), 2);
        assert_eq!(total.elements(ElementBucket::Empty), 2);
        assert_eq!(total.origin(BoundsOrigin::LocalBoxNonFinite), 2);
        assert_eq!(
            total.json(),
            "{\"elements\":{\"empty\":2,\"le_4\":0,\"le_16\":2,\"le_64\":0,\"le_256\":0,\"gt_256\":0},\"bounds\":{\"finite\":2,\"path_non_finite\":0,\"transform_non_finite\":0,\"local_box_non_finite\":2,\"transformed_box_non_finite\":0,\"area_non_finite\":0},\"non_finite\":{\"move_to\":0,\"line_to\":0,\"quad_to_ctrl\":0,\"quad_to_end\":0,\"curve_to_ctrl1\":0,\"curve_to_ctrl2\":0,\"curve_to_end\":0,\"nan\":0,\"pos_inf\":0,\"neg_inf\":0,\"x\":0,\"y\":0}}"
        );
    }

    #[test]
    fn area_buckets_have_fixed_bounds() {
        assert_eq!(AreaBucket::of(0.0), AreaBucket::Under1K);
        assert_eq!(AreaBucket::of(1023.9), AreaBucket::Under1K);
        assert_eq!(AreaBucket::of(1024.0), AreaBucket::Under16K);
        assert_eq!(AreaBucket::of(128.0 * 128.0), AreaBucket::Under256K);
        assert_eq!(AreaBucket::of(512.0 * 512.0), AreaBucket::Under1M);
        assert_eq!(AreaBucket::of(1280.0 * 720.0), AreaBucket::Under1M);
        assert_eq!(AreaBucket::of(1920.0 * 1080.0), AreaBucket::AtLeast1M);
        assert_eq!(AreaBucket::of(f64::NAN), AreaBucket::NonFinite);
        assert_eq!(AreaBucket::of(f64::INFINITY), AreaBucket::NonFinite);
    }

    #[test]
    fn fill_counts_flag_fields_and_brush_only_changes() {
        let brush = FillChanges {
            brush: true,
            ..FillChanges::default()
        };
        let moved = FillChanges {
            transform: true,
            bounds: true,
            ..FillChanges::default()
        };
        let path_nan = PathClass {
            elements: ElementBucket::Over256,
            origin: BoundsOrigin::PathNonFinite,
            non_finite: NonFiniteSlots::NONE,
        };
        let mut frame = FillCounts::default();
        frame.add(FillDiff {
            area: AreaBucket::AtLeast1M,
            current: FINITE_RECT,
            changes: Some(brush),
            cached: Some(FINITE_RECT),
        });
        frame.add(FillDiff {
            area: AreaBucket::NonFinite,
            current: path_nan,
            changes: Some(moved),
            cached: Some(FINITE_RECT),
        });
        let mut total = FillCounts::default();
        total.merge(&frame);
        total.merge(&frame);
        assert_eq!(total.area(AreaBucket::AtLeast1M), 2);
        assert_eq!(total.area(AreaBucket::NonFinite), 2);
        assert_eq!(total.field("compared"), 4);
        // Current and cached path classes are counted separately.
        assert_eq!(total.current_path().origin(BoundsOrigin::PathNonFinite), 2);
        assert_eq!(total.current_path().elements(ElementBucket::Over256), 2);
        assert_eq!(total.cached_path().origin(BoundsOrigin::Finite), 4);
        assert_eq!(total.cached_path().elements(ElementBucket::UpTo16), 4);
        assert_eq!(total.field("brush_changed"), 2);
        assert_eq!(total.field("brush_only"), 2);
        assert_eq!(total.field("transform_changed"), 2);
        assert_eq!(total.field("bounds_changed"), 2);
        assert_eq!(total.field("shape_changed"), 0);
        assert_eq!(total.field("other_changed"), 0);
        let json = total.json();
        let no_non_finite = "\"non_finite\":{\"move_to\":0,\"line_to\":0,\"quad_to_ctrl\":0,\"quad_to_end\":0,\"curve_to_ctrl1\":0,\"curve_to_ctrl2\":0,\"curve_to_end\":0,\"nan\":0,\"pos_inf\":0,\"neg_inf\":0,\"x\":0,\"y\":0}";
        assert_eq!(
            json,
            format!(
                "{{\"area\":{{\"lt_1k\":0,\"lt_16k\":0,\"lt_256k\":0,\"lt_1m\":0,\"ge_1m\":2,\"non_finite\":2}},\"compared\":4,\"transform_changed\":2,\"brush_changed\":2,\"shape_changed\":0,\"other_changed\":0,\"bounds_changed\":2,\"brush_only\":2,\"path_bits\":{{\"same_bitwise_path\":4,\"count_mismatch\":0,\"verb_mismatch\":0,\"point_bits_differ\":0,\"shape_changed_same_bits\":0}},\"current_path\":{{\"elements\":{{\"empty\":0,\"le_4\":0,\"le_16\":2,\"le_64\":0,\"le_256\":0,\"gt_256\":2}},\"bounds\":{{\"finite\":2,\"path_non_finite\":2,\"transform_non_finite\":0,\"local_box_non_finite\":0,\"transformed_box_non_finite\":0,\"area_non_finite\":0}},{no_non_finite}}},\"cached_path\":{{\"elements\":{{\"empty\":0,\"le_4\":0,\"le_16\":4,\"le_64\":0,\"le_256\":0,\"gt_256\":0}},\"bounds\":{{\"finite\":4,\"path_non_finite\":0,\"transform_non_finite\":0,\"local_box_non_finite\":0,\"transformed_box_non_finite\":0,\"area_non_finite\":0}},{no_non_finite}}}}}"
            )
        );
        // A brush change together with any other change is not brush_only.
        let mut mixed = FillCounts::default();
        mixed.add(FillDiff {
            area: AreaBucket::Under16K,
            current: FINITE_RECT,
            changes: Some(FillChanges {
                brush: true,
                other: true,
                ..FillChanges::default()
            }),
            cached: Some(FINITE_RECT),
        });
        assert_eq!(mixed.field("brush_only"), 0);
        assert_eq!(mixed.field("other_changed"), 1);

        // Only first-differing fills reach the fill counters.
        let mut counts = DiffCounts::default();
        counts.add(ContentDiff {
            first: FirstDiff::Command(CommandKind::Stroke),
            length_changed: false,
            kind_changed: false,
            fill: Some(FillDiff {
                area: AreaBucket::Under1K,
                current: FINITE_RECT,
                changes: Some(brush),
                cached: Some(FINITE_RECT),
            }),
        });
        assert_eq!(*counts.fill(), FillCounts::default());
    }

    #[test]
    fn the_cpu_renderer_reports_null_phase_blocks() {
        let timings = FrameTimings::new();
        assert!(timings.render_phases.is_none());
        assert!(timings.raster_cache_misses.is_none());
        assert!(timings.raster_cache_content_diff.is_none());
    }
}
