//! Opt-in Netrender window renderer (feature `netrender`, built only by the
//! isolated manifest `netrender-window/Cargo.toml`; the probe's default graph
//! and renderer are unchanged). First bounded integration of
//! `docs/handoff/2026-11-15-netrender-blitz.md`.
//!
//! - The page's adapter, device and queue are adopted in wgpu through the
//!   proof's bridge (`roves_netrender_shared_device_proof::adopt_device`, on the
//!   wgpu 29 interop patch), so Netrender renders on deno_webgpu's own wgpu-core
//!   device and queue: no second device, no copy of any canvas texture.
//! - The recorded Blitz scene is translated into a Netrender `Scene`
//!   (`translate`): solid axis-aligned rectangles, rectangular clips, group
//!   opacity over a single draw, and each WebGPU canvas placeholder as an
//!   external texture at its scene-op boundary, so content painted after the
//!   canvas stays above it. Anything else (text, paths, images, gradients,
//!   strokes, shadows, filters, blend modes, transformed canvases) is not
//!   translated yet: the window switches once to the existing
//!   `GpuWindowRenderer` (`src/presentation.rs`), which stays the fallback.
//! - The final output (Netrender's master) is drawn onto the window surface
//!   with wgpu's maintained `util::TextureBlitter` (format conversion to the
//!   surface format), submitted on the same queue after Netrender's work.
//!
//! Chromium comparison (reference f2700724,
//! `components/viz/service/display/display_resource_provider.h`): Viz reads a
//! child's resource only inside a `ScopedReadLockSharedImage`, which keeps the
//! shared image alive for the draw and carries the `gpu::SyncToken`s that order
//! the display compositor's GPU work after the producer's. Here the scoped read
//! is the per-frame adoption (`OwnedTexture::adopt`): an embedder-owned id of
//! the canvas texture is cloned and adopted for one frame and dropped after
//! submission (wgpu-core keeps in-flight resources alive). No interprocess or
//! cross-context token is needed: producer (the page's deno_webgpu calls) and
//! consumer (Netrender, blit, present) are one process, one wgpu-core device
//! and one queue, and the page's submissions for a frame precede the
//! compositor's on that queue, so queue order is the synchronization and
//! wgpu-core's trackers insert the usage barriers. Device ownership stays with
//! the page: the embedder only holds extra ids of the same objects.
//!
//! Device loss and errors: the adopted device has its own error sink (interop
//! patch), so `on_uncaptured_error` here never replaces the page's
//! `onuncapturederror`; no device-lost callback is installed (wgpu-core has a
//! single slot, and deno_webgpu resolves the page's `GPUDevice.lost` itself).
//! Any wgpu error reported to this sink, a lost/invalid surface or a panic
//! inside Netrender switches the window to the fallback renderer once.
//!
//! Page device lost (e.g. the page's own `GPUDevice.destroy()`): read from
//! deno_webgpu's public `DeviceErrorHandler::is_lost` of the page's device
//! before each frame (set synchronously by `destroy()`, on this thread), never
//! inferred from errors (wgpu drops `DeviceLost` errors by design). Both
//! renderers present on that same device, so the fallback cannot help: the
//! window stops presenting once (`netrender_window_device_lost`), releases the
//! Netrender objects and surface (or the fallback's surface), reports itself
//! inactive so the shell stops requesting redraws, and leaves the page, its
//! `lost` handlers and the JS loop running; close and teardown stay the
//! normal ones. Chromium (f2700724) keeps compositing instead: Viz's GPU
//! context is separate from the page's WebGPU device, while Roves shares it.
//!
//! Resize: `set_size` reconfigures the surface at once and frames are skipped
//! until the configured size matches the shell's; each Netrender frame is a
//! full redraw at the new size. Chromium's `DirectRenderer::DrawFrame`
//! reshapes lazily, only when it is about to draw, then forces a full redraw,
//! and `Display::DrawAndSwap` skips frames whose root pass size does not match
//! `current_surface_size_`. Same outcome: no stretched or partially sized
//! frame is presented.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyrender::recording::{FillCommand, RenderCommand};
use anyrender::{Paint, RenderContext, ResourceId, Scene, WindowHandle, WindowRenderer};
use deno_error::JsErrorBox;
use deno_webgpu::wgpu_core::id::{AdapterId, DeviceId, QueueId, TextureId};
use deno_webgpu::wgpu_types;
use kurbo::{Affine, PathEl, Point, Rect, Shape};
use netrender::{
    Compositor, ExternalTextureComposite, ExternalTexturePlacement, NetrenderOptions,
    PresentedFrame, SourceAlpha, WgpuHandles, create_netrender_instance,
};
use peniko::{Compose, Mix};
use roves_netrender_shared_device_proof::{CanvasDesc, adopt_device, adopt_texture};

use crate::presentation::{GpuWindowRenderer, PresentStats, SharedPresentation};

// ---------------------------------------------------------------------------
// Embedder-owned ids (scoped reads of the page's objects)

/// Embedder-owned ids of the page's adapter, device and queue, cloned in
/// `op_blitz_present_device` while the page's objects were borrowed. Dropped
/// (ids removed from the registry) unless adopted.
pub struct OwnedDeviceIds {
    global: deno_webgpu::Instance,
    adapter: AdapterId,
    device: DeviceId,
    queue: QueueId,
    /// The page `GPUDevice`'s deno_webgpu error handler (shared `Rc`): its
    /// public `is_lost` is set once the device is lost or destroyed.
    page_errors: deno_webgpu::error::ErrorHandler,
    adopted: bool,
}

impl OwnedDeviceIds {
    pub fn clone_from_page(
        global: &deno_webgpu::Instance,
        adapter: AdapterId,
        device: DeviceId,
        queue: QueueId,
        page_errors: deno_webgpu::error::ErrorHandler,
    ) -> Self {
        Self {
            global: global.clone(),
            adapter: global.adapter_clone_id(adapter),
            device: global.device_clone_id(device),
            queue: global.queue_clone_id(queue),
            page_errors,
            adopted: false,
        }
    }

    fn adopt(mut self) -> (WgpuHandles, deno_webgpu::Instance) {
        self.adopted = true;
        // SAFETY: same wgpu-core build (deno_webgpu's, patched by the manifest);
        // the ids are live clones owned only by `self`, the queue is the
        // device's, and ownership moves to the wgpu objects (`adopted`).
        let handles = unsafe { adopt_device(&self.global, self.adapter, self.device, self.queue) };
        (handles, self.global.clone())
    }
}

impl Drop for OwnedDeviceIds {
    fn drop(&mut self) {
        if !self.adopted {
            self.global.queue_drop(self.queue);
            self.global.device_drop(self.device);
            self.global.adapter_drop(self.adapter);
        }
    }
}

/// An embedder-owned id of a canvas texture registered by the page
/// (`op_blitz_present_canvas`), with its creation descriptor. Dropping it
/// removes only this id; the texture lives while any id or in-flight
/// submission holds it.
pub struct OwnedTexture {
    global: deno_webgpu::Instance,
    id: TextureId,
    desc: CanvasDesc,
}

impl OwnedTexture {
    pub fn clone_from_page(texture: &deno_webgpu::texture::GPUTexture) -> Result<Self, JsErrorBox> {
        let desc = CanvasDesc {
            size: texture.size,
            mip_level_count: texture.mip_level_count,
            sample_count: texture.sample_count,
            dimension: texture.dimension.clone().into(),
            format: texture.format.clone().into(),
            usage: texture.usage.0,
        };
        Ok(Self {
            global: texture.instance.clone(),
            id: texture.instance.texture_clone_id(texture.id),
            desc,
        })
    }

    /// Whether Netrender can sample it as premultiplied 8-bit linear values
    /// (the same formats the existing compositor accepts).
    fn composable(&self) -> bool {
        self.desc
            .usage
            .contains(wgpu_types::TextureUsages::TEXTURE_BINDING)
            && self.desc.sample_count == 1
            && matches!(
                self.desc.format,
                wgpu_types::TextureFormat::Rgba8Unorm | wgpu_types::TextureFormat::Bgra8Unorm
            )
    }

    /// The scoped read: a fresh id adopted by a wgpu `Texture` that lives for
    /// one frame.
    fn adopt(&self, handles: &WgpuHandles) -> wgpu::Texture {
        let id = self.global.texture_clone_id(self.id);
        // SAFETY: `id` is a live clone owned only by the returned texture,
        // created on the adopted device (checked in op_blitz_present_canvas)
        // with `desc`.
        unsafe { adopt_texture(handles, id, &self.desc) }
    }
}

impl Drop for OwnedTexture {
    fn drop(&mut self) {
        self.global.texture_drop(self.id);
    }
}

// ---------------------------------------------------------------------------
// Scene translation (no GPU)

#[derive(Clone, Debug, PartialEq)]
pub enum PlanOp {
    /// Device-space `[x0, y0, x1, y1]` and straight-alpha sRGB-encoded color.
    Rect { rect: [f32; 4], color: [f32; 4] },
    Canvas {
        resource: ResourceId,
        rect: [f32; 4],
        opacity: f32,
    },
}

#[derive(Clone, Copy, Debug)]
struct OpenLayer {
    clip: Rect,
    alpha: f32,
    draws: usize,
}

/// Device-space rectangle of `shape` under `transform`, if both are
/// axis-aligned (scale/translate only, a single straight-line subpath).
///
/// Accepted encodings (all from blitz-paint/kurbo, closed implicitly by the
/// fill or explicitly by `ClosePath`): kurbo's `Rect` path
/// (`MoveTo` + 3 `LineTo` + `ClosePath`), blitz-paint's sharp CSS box path
/// (`MoveTo` + 3 `LineTo`, no `ClosePath`) and an explicit return to the start.
/// Consecutive repeated points are dropped first: they add no area.
///
/// - Non-degenerate bounds: exactly four distinct corners of the bounds joined
///   by axis-aligned edges, i.e. the rectangle itself.
/// - Zero-width or zero-height bounds (e.g. the zero-height `html` box of a
///   page whose children are all absolutely positioned): every point lies on
///   one line segment, so the polygon has no area. It paints nothing as a fill
///   and clips everything as a clip, exactly as the empty rectangle returned.
///
/// Curves, several subpaths and any other polygon (rotated, L-shaped,
/// self-overlapping) are rejected.
fn axis_aligned_rect(transform: Affine, shape: &kurbo::BezPath) -> Option<Rect> {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    if b != 0.0 || c != 0.0 || a <= 0.0 || d <= 0.0 {
        return None;
    }
    let mut points: Vec<Point> = Vec::with_capacity(5);
    let elements = shape.elements();
    for (index, element) in elements.iter().enumerate() {
        let point = match *element {
            PathEl::MoveTo(point) if index == 0 => point,
            PathEl::LineTo(point) if index > 0 => point,
            // Only as the last element: a second subpath may follow otherwise.
            PathEl::ClosePath if index > 0 && index + 1 == elements.len() => continue,
            _ => return None,
        };
        if points.last() != Some(&point) {
            points.push(point);
        }
    }
    // The implicit or explicit closing edge back to the start.
    while points.len() > 1 && points.last() == points.first() {
        points.pop();
    }
    if points.is_empty() || points.iter().any(|point| !point.is_finite()) {
        return None;
    }
    let bounds = shape.bounding_box();
    let degenerate = bounds.width() == 0.0 || bounds.height() == 0.0;
    if !degenerate {
        if points.len() != 4 {
            return None;
        }
        for index in 0..4 {
            let (p, q) = (points[index], points[(index + 1) % 4]);
            // Exactly one coordinate changes along each edge.
            if (p.x != q.x) == (p.y != q.y) {
                return None;
            }
        }
        let corners = [
            Point::new(bounds.x0, bounds.y0),
            Point::new(bounds.x1, bounds.y0),
            Point::new(bounds.x0, bounds.y1),
            Point::new(bounds.x1, bounds.y1),
        ];
        // Four distinct corners: a repeated one (e.g. TL, TR, BR, TR) would
        // pass the edge test with no area.
        if !corners.iter().all(|corner| points.contains(corner)) {
            return None;
        }
    }
    let p0 = transform * Point::new(bounds.x0, bounds.y0);
    let p1 = transform * Point::new(bounds.x1, bounds.y1);
    Some(Rect::new(p0.x, p0.y, p1.x, p1.y))
}

fn rect_array(rect: Rect) -> [f32; 4] {
    [
        rect.x0 as f32,
        rect.y0 as f32,
        rect.x1 as f32,
        rect.y1 as f32,
    ]
}

fn clip_of(layers: &[OpenLayer], viewport: Rect) -> Rect {
    layers
        .iter()
        .fold(viewport, |clip, layer| clip.intersect(layer.clip))
}

fn canvas_fill(fill: &FillCommand) -> Option<ResourceId> {
    match &fill.brush {
        Paint::Resource(brush) => Some(brush.image),
        _ => None,
    }
}

/// Translates a recorded Blitz scene, or names the first construct this
/// renderer does not translate yet (the caller then uses the fallback).
pub fn translate(
    scene: &Scene,
    viewport: (u32, u32),
    is_canvas: impl Fn(ResourceId) -> bool,
) -> Result<Vec<PlanOp>, String> {
    let viewport = Rect::new(0.0, 0.0, f64::from(viewport.0), f64::from(viewport.1));
    let mut layers: Vec<OpenLayer> = Vec::new();
    let mut ops = Vec::new();
    for command in &scene.commands {
        match command {
            RenderCommand::PushClipLayer(layer) => {
                let clip = axis_aligned_rect(layer.transform, &layer.clip)
                    .ok_or("non-rectangular or transformed clip")?;
                layers.push(OpenLayer {
                    clip,
                    alpha: 1.0,
                    draws: 0,
                });
            },
            RenderCommand::PushLayer(layer) => {
                if layer.filter.is_some() || layer.backdrop_filter.is_some() {
                    return Err("filter or backdrop-filter layer".into());
                }
                if layer.blend.mix != Mix::Normal || layer.blend.compose != Compose::SrcOver {
                    return Err("blend mode layer".into());
                }
                let clip = axis_aligned_rect(layer.transform, &layer.clip)
                    .ok_or("non-rectangular or transformed layer clip")?;
                layers.push(OpenLayer {
                    clip,
                    alpha: layer.alpha,
                    draws: 0,
                });
            },
            RenderCommand::PopLayer => {
                let layer = layers.pop().ok_or("unbalanced PopLayer")?;
                // Folding group opacity into each draw is exact only for one draw.
                if layer.alpha < 1.0 && layer.draws > 1 {
                    return Err("group opacity over several draws".into());
                }
            },
            RenderCommand::Fill(fill) => {
                let clip = clip_of(&layers, viewport);
                let alpha: f32 = layers.iter().map(|layer| layer.alpha).product();
                for layer in &mut layers {
                    layer.draws += 1;
                }
                let rect = axis_aligned_rect(fill.transform, &fill.shape)
                    .ok_or("non-rectangular or transformed fill")?;
                if let Some(resource) = canvas_fill(fill) {
                    if !is_canvas(resource) {
                        return Err("image resource that is not a WebGPU canvas".into());
                    }
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        // A zero-area canvas box shows nothing.
                        continue;
                    }
                    if clip.intersect(rect) != rect {
                        return Err("clipped canvas".into());
                    }
                    ops.push(PlanOp::Canvas {
                        resource,
                        rect: rect_array(rect),
                        opacity: alpha,
                    });
                    continue;
                }
                let Paint::Solid(color) = &fill.brush else {
                    return Err("gradient, image or custom fill".into());
                };
                let clipped = clip.intersect(rect);
                if clipped.width() <= 0.0 || clipped.height() <= 0.0 {
                    continue;
                }
                let [r, g, b, a] = color.components;
                ops.push(PlanOp::Rect {
                    rect: rect_array(clipped),
                    color: [r, g, b, a * alpha],
                });
            },
            RenderCommand::Stroke(_) => return Err("stroke".into()),
            RenderCommand::GlyphRun(_) => return Err("text".into()),
            RenderCommand::BoxShadow(_) => return Err("box shadow".into()),
        }
    }
    if !layers.is_empty() {
        return Err("unbalanced PushLayer".into());
    }
    Ok(ops)
}

// ---------------------------------------------------------------------------
// GPU state

#[derive(Default)]
struct CaptureMaster {
    master: Option<wgpu::Texture>,
}

impl Compositor for CaptureMaster {
    fn declare_surface(&mut self, _key: netrender::SurfaceKey, _world_bounds: [f32; 4]) {}
    fn destroy_surface(&mut self, _key: netrender::SurfaceKey) {}
    fn present_frame(&mut self, frame: PresentedFrame<'_>) {
        self.master = Some(frame.master.clone());
    }
}

/// Netrender's master is `Rgba8Unorm`: a storage texture, and the game device
/// has no `BGRA8UNORM_STORAGE` feature.
const MASTER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Fields drop in declaration order: the surface (configured on the device)
/// first, then Netrender and the blitter, then the adopted wgpu objects.
struct Gpu {
    surface: Option<(wgpu::Surface<'static>, Arc<dyn WindowHandle>)>,
    surface_format: Option<wgpu::TextureFormat>,
    /// Size the current surface is configured for (`None`: not configured).
    configured: Option<(u32, u32)>,
    renderer: netrender::Renderer,
    blitter: Option<wgpu::util::TextureBlitter>,
    handles: WgpuHandles,
}

/// `ROVES_PROBE_NETRENDER_FIXTURE=1`: after this many presented frames the
/// final output (never a canvas) is read back once and the fixture pixels of
/// `fixtures/netrender-window/index.html` are checked.
const FIXTURE_FRAME: u64 = 30;

/// Fixture pixels in CSS px (viewport 640x360) and the expected sRGB-encoded
/// RGBA: opaque red `#before`, a 50% premultiplied blue canvas, a 50% white
/// `#after` over both, and the black page background.
const FIXTURE_CHECKS: [(&str, f64, f64, [u8; 4]); 6] = [
    ("page background only", 600.0, 320.0, [0, 0, 0, 255]),
    ("before layer only", 60.0, 60.0, [255, 0, 0, 255]),
    (
        "after layer over before layer",
        350.0,
        100.0,
        [255, 128, 128, 255],
    ),
    (
        "50% canvas over before layer",
        150.0,
        180.0,
        [128, 0, 128, 255],
    ),
    (
        "after layer over 50% canvas over before layer",
        240.0,
        120.0,
        [192, 128, 192, 255],
    ),
    (
        "after layer over page background",
        380.0,
        300.0,
        [128, 128, 128, 255],
    ),
];

/// `ROVES_PROBE_NETRENDER_SCENARIO` (with `ROVES_PROBE_NETRENDER_FIXTURE=1`):
/// a lifecycle step started once, right after the fixture's first verified
/// frame (`FIXTURE_CHECKS` read back), on a separate run of the fixture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scenario {
    /// A native window resize to `RESIZE_LOGICAL` (winit, see
    /// `window::WindowScenario`), then `RESIZE_CHECKS` at the new size.
    Resize,
    /// The page's own `GPUDevice.destroy()` (its `netrenderFixtureDestroyDevice`).
    DeviceDestroy,
}

impl Scenario {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "resize" => Some(Self::Resize),
            "device-destroy" => Some(Self::DeviceDestroy),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Resize => "resize",
            Self::DeviceDestroy => "device-destroy",
        }
    }
}

/// Logical size requested by the resize scenario: larger than the 640x360
/// viewport in both directions, so the added area is checked too.
const RESIZE_LOGICAL: (f64, f64) = (800.0, 450.0);
/// Frames presented at the new size before the read back: the page has seen
/// the new `innerWidth` and drawn its canvas in the new color by then.
const RESIZE_SETTLE_FRAMES: u64 = 10;

/// After the resize (CSS px, expected sRGB-encoded RGBA). The page clears its
/// canvas to 50% premultiplied green once `innerWidth` changed, so a stale
/// canvas texture (blue) fails both canvas pixels; the added area must show
/// the page background (Netrender clears to white), and a pixel near the
/// right edge of `#before` fails if the old frame were stretched.
const RESIZE_CHECKS: [(&str, f64, f64, [u8; 4]); 8] = [
    ("page background only", 600.0, 320.0, [0, 0, 0, 255]),
    ("before layer only", 60.0, 60.0, [255, 0, 0, 255]),
    (
        "after layer over before layer",
        350.0,
        100.0,
        [255, 128, 128, 255],
    ),
    (
        "latest 50% green canvas over before layer",
        150.0,
        180.0,
        [128, 128, 0, 255],
    ),
    (
        "after layer over latest 50% green canvas over before layer",
        240.0,
        120.0,
        [192, 192, 128, 255],
    ),
    (
        "after layer over page background",
        380.0,
        300.0,
        [128, 128, 128, 255],
    ),
    (
        "page background in the area added by the resize",
        760.0,
        420.0,
        [0, 0, 0, 255],
    ),
    (
        "before layer near its right edge (frame not stretched)",
        430.0,
        230.0,
        [255, 0, 0, 255],
    ),
];

pub struct NetrenderWindowRenderer {
    gpu: Option<Gpu>,
    presentation: SharedPresentation,
    fallback: GpuWindowRenderer,
    /// Why the window uses the fallback renderer (set once).
    fallback_reason: Option<String>,
    window: Option<Arc<dyn WindowHandle>>,
    width: u32,
    height: u32,
    errors: Arc<Mutex<Vec<String>>>,
    own: PresentStats,
    /// Shared with the document (`gpu_window_presentation` line).
    pub stats: Rc<RefCell<PresentStats>>,
    fixture: bool,
    fixture_reported: bool,
    /// Device pixels per CSS px, from the first `resume` (the window opens at
    /// `crate::VIEWPORT` CSS px); fixed for the run, also after a resize.
    scale: Option<f64>,
    /// Lost state of the page's device (see the module docs).
    page_errors: deno_webgpu::error::ErrorHandler,
    /// The page's device was lost: nothing is presented any more.
    device_lost: bool,
    /// `render` calls after `device_lost` (no GPU work, no retry).
    calls_after_device_lost: u64,
    scenario: Option<(Scenario, crate::window::SharedWindowScenario)>,
    resize_requested: bool,
    frames_at_resize_target: u64,
    resize_reported: bool,
}

impl NetrenderWindowRenderer {
    /// Adopts the page's device registered by `op_blitz_present_device`;
    /// returns the fallback with the reason when that is not possible.
    pub fn new(
        presentation: SharedPresentation,
        fallback: GpuWindowRenderer,
    ) -> Result<Self, (GpuWindowRenderer, String)> {
        let Some(ids) = presentation.borrow_mut().core_ids.take() else {
            return Err((fallback, "no page device registered".into()));
        };
        let page_errors = ids.page_errors.clone();
        let (handles, _global) = ids.adopt();
        let errors: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = errors.clone();
        // The adopted device's own error sink (the page's is untouched).
        handles
            .device
            .on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
                if let Ok(mut errors) = sink.lock() {
                    errors.push(error.to_string());
                }
            }));
        let renderer_handles = WgpuHandles {
            instance: handles.instance.clone(),
            adapter: handles.adapter.clone(),
            device: handles.device.clone(),
            queue: handles.queue.clone(),
        };
        let renderer = match create_netrender_instance(
            renderer_handles,
            NetrenderOptions {
                tile_cache_size: Some(64),
                enable_vello: true,
                ..Default::default()
            },
        ) {
            Ok(renderer) => renderer,
            Err(error) => {
                return Err((fallback, format!("create_netrender_instance: {error:?}")));
            },
        };
        let fixture =
            std::env::var("ROVES_PROBE_NETRENDER_FIXTURE").is_ok_and(|value| value == "1");
        let scenario = std::env::var("ROVES_PROBE_NETRENDER_SCENARIO")
            .ok()
            .filter(|_| fixture)
            .and_then(|value| Scenario::parse(&value))
            .map(|scenario| (scenario, crate::window::SharedWindowScenario::default()));
        Ok(Self {
            gpu: Some(Gpu {
                surface: None,
                surface_format: None,
                configured: None,
                renderer,
                blitter: None,
                handles,
            }),
            presentation,
            fallback,
            fallback_reason: None,
            window: None,
            width: 0,
            height: 0,
            errors,
            own: PresentStats::default(),
            stats: Rc::default(),
            fixture,
            fixture_reported: false,
            scale: None,
            page_errors,
            device_lost: false,
            calls_after_device_lost: 0,
            scenario,
            resize_requested: false,
            frames_at_resize_target: 0,
            resize_reported: false,
        })
    }

    /// The scenario requests the window run carries out (fixture only).
    pub fn scenario(&self) -> Option<crate::window::SharedWindowScenario> {
        self.scenario.as_ref().map(|(_, shared)| shared.clone())
    }

    fn page_device_lost(&self) -> bool {
        self.page_errors.is_lost.get().is_some()
    }

    /// One controlled stop when the page's device is lost (module docs).
    fn stop_on_device_lost(&mut self) {
        if self.device_lost {
            return;
        }
        self.device_lost = true;
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"netrender_window_device_lost\",\"status\":\"stopped\",\"renderer\":\"{}\",\"netrender_frames\":{},\"fixture_verified\":{},\"scenario\":{}}}",
            if self.fallback_reason.is_some() {
                "fallback"
            } else {
                "netrender"
            },
            self.own.frames,
            self.fixture_reported,
            self.scenario.as_ref().map_or_else(
                || "null".to_string(),
                |(scenario, _)| crate::window::json_string(scenario.name())
            )
        );
        // Netrender's objects, its surface and the adopted ids (field order),
        // or the fallback's surface: nothing presents on the lost device.
        self.gpu = None;
        if self.fallback_reason.is_some() {
            self.fallback.suspend();
        }
    }

    /// Starts the scenario right after the first verified fixture frame.
    fn start_scenario(&mut self) {
        let Some((scenario, shared)) = &self.scenario else {
            return;
        };
        match scenario {
            Scenario::Resize => {
                self.resize_requested = true;
                shared.resize_to.set(Some(RESIZE_LOGICAL));
            },
            Scenario::DeviceDestroy => {
                shared
                    .page_call
                    .set(Some("globalThis.netrenderFixtureDestroyDevice()"));
            },
        }
    }

    /// Device size of the resize target at the run's scale.
    fn resize_target(&self) -> (u32, u32) {
        let scale = self.scale.unwrap_or(1.0);
        (
            (RESIZE_LOGICAL.0 * scale).round() as u32,
            (RESIZE_LOGICAL.1 * scale).round() as u32,
        )
    }

    pub fn set_timings(&mut self, timings: Option<crate::frame_timings::SharedFrameTimings>) {
        // Only the fallback samples frame timings; the Netrender path does not.
        self.fallback.timings = timings;
    }

    fn take_errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .map(|mut errors| std::mem::take(&mut *errors))
            .unwrap_or_default()
    }

    fn publish_stats(&self) {
        let fallback = *self.fallback.stats.borrow();
        let own = self.own;
        *self.stats.borrow_mut() = PresentStats {
            frames: own.frames + fallback.frames,
            canvas_layers: own.canvas_layers + fallback.canvas_layers,
            raster_segments: own.raster_segments + fallback.raster_segments,
            raster_uploads: own.raster_uploads + fallback.raster_uploads,
            raster_cache_hits: own.raster_cache_hits + fallback.raster_cache_hits,
            upload_bytes: own.upload_bytes + fallback.upload_bytes,
            max_layers: own.max_layers.max(fallback.max_layers),
            skipped_frames: own.skipped_frames + fallback.skipped_frames,
            errors: own.errors + fallback.errors,
        };
    }

    /// Switches the window to the existing renderer once: drops the Netrender
    /// surface and objects (the page's device is untouched), then resumes the
    /// fallback on the same window.
    fn use_fallback(&mut self, reason: String) {
        if self.fallback_reason.is_some() {
            return;
        }
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"netrender_window_fallback\",\"status\":\"fallback\",\"frames\":{},\"reason\":{}}}",
            self.own.frames,
            crate::window::json_string(&reason)
        );
        self.fallback_reason = Some(reason);
        if let Some(gpu) = self.gpu.take() {
            // Drain is not needed for correctness (wgpu-core keeps in-flight
            // resources alive); the surface goes first (field order).
            drop(gpu);
        }
        if let Some(window) = self.window.clone() {
            self.fallback.resume(window, self.width, self.height, || {});
        }
    }

    fn configure(&mut self) {
        let (width, height) = (self.width, self.height);
        if width == 0 || height == 0 {
            return;
        }
        let Some(gpu) = &mut self.gpu else { return };
        let Some((surface, _)) = &gpu.surface else {
            return;
        };
        if gpu.surface_format.is_none() {
            let caps = surface.get_capabilities(&gpu.handles.adapter);
            // 8-bit non-sRGB, as the existing compositor: Blitz's colors are
            // sRGB-encoded values blended as stored.
            let found = [
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureFormat::Rgba8Unorm,
            ]
            .into_iter()
            .find(|format| caps.formats.contains(format));
            let Some(format) = found else {
                self.use_fallback("no 8-bit non-sRGB surface format".into());
                return;
            };
            gpu.surface_format = Some(format);
            gpu.blitter = Some(wgpu::util::TextureBlitter::new(&gpu.handles.device, format));
        }
        let Some(format) = gpu.surface_format else {
            return;
        };
        surface.configure(
            &gpu.handles.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: Vec::new(),
            },
        );
        gpu.configured = Some((width, height));
    }

    /// Builds the Netrender scene and the external textures for one frame.
    /// Returns `Err` for anything the fallback must render instead.
    fn render_netrender(&mut self, scene: &Scene) -> Result<(), String> {
        let (width, height) = (self.width, self.height);
        self.presentation.borrow_mut().prune();
        let plan = {
            let presentation = self.presentation.borrow();
            translate(scene, (width, height), |id| presentation.is_canvas(id))?
        };
        let Some(gpu) = &self.gpu else {
            return Err("renderer dropped".into());
        };
        let Some((surface, _)) = &gpu.surface else {
            return Ok(());
        };
        if gpu.configured != Some((width, height)) {
            // Zero-sized or not yet configured: nothing to present.
            return Ok(());
        }

        // Scoped reads of the canvases drawn this frame.
        let mut nr_scene = netrender::Scene::new(width, height);
        let mut canvases: Vec<(wgpu::TextureView, ExternalTexturePlacement, usize)> = Vec::new();
        {
            let presentation = self.presentation.borrow();
            let mut textures: HashMap<u64, wgpu::Texture> = HashMap::new();
            for op in &plan {
                match op {
                    PlanOp::Rect { rect, color } => {
                        nr_scene.push_rect(rect[0], rect[1], rect[2], rect[3], *color);
                    },
                    PlanOp::Canvas {
                        resource,
                        rect,
                        opacity,
                    } => {
                        // Unconfigured canvas: its box shows nothing.
                        let Some(Some(source)) = presentation.canvases.get(&resource.into_ffi())
                        else {
                            continue;
                        };
                        if !source.texture.composable() {
                            return Err("canvas format/usage not composable (sRGB, MSAA or no TEXTURE_BINDING)".into());
                        }
                        if source.opaque {
                            if *opacity < 1.0 {
                                return Err("opaque canvas under group opacity".into());
                            }
                            // alphaMode "opaque": premultiplied content over black
                            // shows the stored RGB, as alpha forced to 1 does.
                            nr_scene.push_rect(
                                rect[0],
                                rect[1],
                                rect[2],
                                rect[3],
                                [0.0, 0.0, 0.0, 1.0],
                            );
                        }
                        let texture = textures
                            .entry(resource.into_ffi())
                            .or_insert_with(|| source.texture.adopt(&gpu.handles));
                        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                        let placement = ExternalTexturePlacement::new(*rect)
                            .with_opacity(*opacity)
                            .with_alpha(SourceAlpha::Premultiplied);
                        canvases.push((view, placement, nr_scene.ops.len()));
                    },
                }
            }
        }

        let output = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.own.skipped_frames += 1;
                return Ok(());
            },
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.own.skipped_frames += 1;
                self.configure();
                return Ok(());
            },
            wgpu::CurrentSurfaceTexture::Lost => return Err("surface lost".into()),
            _ => return Err("surface texture validation error".into()),
        };

        let composites: Vec<ExternalTextureComposite<'_>> = canvases
            .iter()
            .map(|(view, placement, boundary)| {
                ExternalTextureComposite::new(view, *placement).with_scene_op_boundary(*boundary)
            })
            .collect();
        let mut compositor = CaptureMaster::default();
        let renderer = &gpu.renderer;
        // Netrender panics on Vello errors (e.g. after a device loss): treat
        // it like any other failure of this path.
        let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            renderer.render_with_compositor_and_external_textures(
                &nr_scene,
                MASTER_FORMAT,
                &mut compositor,
                // The existing compositor composes over white.
                netrender::peniko::Color::new([1.0, 1.0, 1.0, 1.0]),
                &composites,
            );
        }));
        drop(composites);
        if rendered.is_err() {
            return Err("Netrender panicked while rendering".into());
        }
        let master = compositor.master.ok_or("Netrender presented no master")?;
        let master_view = master.create_view(&wgpu::TextureViewDescriptor::default());
        let target = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            gpu.handles
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("roves netrender window blit"),
                });
        gpu.blitter.as_ref().ok_or("surface not configured")?.copy(
            &gpu.handles.device,
            &mut encoder,
            &master_view,
            &target,
        );
        // Same queue as the page's and Netrender's submissions: queue order.
        gpu.handles.queue.submit([encoder.finish()]);
        output.present();
        let errors = self.take_errors();
        if !errors.is_empty() {
            self.own.errors += errors.len() as u64;
            return Err(format!("wgpu error: {}", errors.join("; ")));
        }
        self.own.frames += 1;
        self.own.canvas_layers += canvases.len() as u64;
        self.own.max_layers = self.own.max_layers.max(canvases.len() + 1);
        if !self.fixture {
            return Ok(());
        }
        let scale = self.scale.unwrap_or(1.0);
        if !self.fixture_reported && self.own.frames >= FIXTURE_FRAME {
            self.fixture_reported = true;
            let Some(gpu) = &self.gpu else { return Ok(()) };
            // Fixture-only readback of the final output; never of a canvas.
            let bytes = gpu
                .renderer
                .wgpu_device
                .read_rgba8_texture(&master, width, height);
            report_pixels(
                "netrender_window_fixture",
                &FIXTURE_CHECKS,
                &bytes,
                (width, height),
                scale,
                "",
            );
            self.start_scenario();
        } else if self.resize_requested && !self.resize_reported {
            if (width, height) == self.resize_target() {
                self.frames_at_resize_target += 1;
            }
            if self.frames_at_resize_target >= RESIZE_SETTLE_FRAMES {
                self.resize_reported = true;
                let Some(gpu) = &self.gpu else { return Ok(()) };
                let bytes = gpu
                    .renderer
                    .wgpu_device
                    .read_rgba8_texture(&master, width, height);
                report_pixels(
                    "netrender_window_resize",
                    &RESIZE_CHECKS,
                    &bytes,
                    (width, height),
                    scale,
                    &format!(
                        ",\"requested_logical\":[{},{}],\"frames_at_size\":{}",
                        RESIZE_LOGICAL.0, RESIZE_LOGICAL.1, self.frames_at_resize_target
                    ),
                );
            }
        }
        Ok(())
    }
}

/// One fixture record: `checks` (CSS px at `scale`) against the final output
/// read back at `size`; `extra` is appended to the record (JSON members).
fn report_pixels(
    check: &str,
    checks: &[(&str, f64, f64, [u8; 4])],
    bytes: &[u8],
    (width, height): (u32, u32),
    scale: f64,
    extra: &str,
) {
    let mut failed = 0;
    let mut pixels = Vec::new();
    for &(name, x, y, expected) in checks {
        let (px, py) = ((x * scale) as u32, (y * scale) as u32);
        let index = ((py.min(height - 1) * width + px.min(width - 1)) * 4) as usize;
        let actual = [
            bytes[index],
            bytes[index + 1],
            bytes[index + 2],
            bytes[index + 3],
        ];
        let diff = (0..4)
            .map(|i| (i16::from(actual[i]) - i16::from(expected[i])).unsigned_abs())
            .max()
            .unwrap_or(0);
        let ok = diff <= 2;
        failed += usize::from(!ok);
        pixels.push(format!(
            "{{\"name\":{},\"x\":{px},\"y\":{py},\"actual\":{actual:?},\"expected\":{expected:?},\"pass\":{ok}}}",
            crate::window::json_string(name)
        ));
    }
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"{check}\",\"status\":\"{}\",\"failed\":{failed},\"readback\":\"final_output_only\",\"width\":{width},\"height\":{height}{extra},\"pixels\":[{}]}}",
        if failed == 0 { "passed" } else { "failed" },
        pixels.join(",")
    );
}

impl Drop for NetrenderWindowRenderer {
    fn drop(&mut self) {
        println!(
            "ROVES_PROBE_RESULT {{\"check\":\"netrender_window\",\"status\":\"{}\",\"netrender_frames\":{},\"canvas_layers\":{},\"skipped_frames\":{},\"errors\":{},\"size\":[{},{}],\"calls_after_device_lost\":{},\"fallback_reason\":{}}}",
            if self.device_lost {
                "device_lost"
            } else if self.fallback_reason.is_none() && self.own.frames > 0 {
                "netrender"
            } else {
                "fallback"
            },
            self.own.frames,
            self.own.canvas_layers,
            self.own.skipped_frames,
            self.own.errors,
            self.width,
            self.height,
            self.calls_after_device_lost,
            self.fallback_reason
                .as_deref()
                .map_or_else(|| "null".to_string(), crate::window::json_string)
        );
    }
}

impl RenderContext for NetrenderWindowRenderer {}

impl WindowRenderer for NetrenderWindowRenderer {
    type ScenePainter<'a>
        = Scene
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(
        &mut self,
        window: Arc<dyn WindowHandle>,
        width: u32,
        height: u32,
        on_ready: F,
    ) {
        self.window = Some(window.clone());
        if self.scale.is_none() && width > 0 {
            self.scale = Some(f64::from(width) / f64::from(crate::VIEWPORT.0));
        }
        if self.device_lost {
            self.width = width;
            self.height = height;
            on_ready();
            return;
        }
        if self.fallback_reason.is_some() {
            self.fallback.resume(window, width, height, on_ready);
            return;
        }
        self.width = width;
        self.height = height;
        let created = self.gpu.as_mut().map(|gpu| {
            gpu.surface = None;
            gpu.configured = None;
            gpu.handles.instance.create_surface(window.clone())
        });
        match created {
            Some(Ok(surface)) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.surface = Some((surface, window));
                }
                self.configure();
            },
            Some(Err(error)) => self.use_fallback(format!("create_surface: {error}")),
            None => {},
        }
        on_ready();
    }

    fn complete_resume(&mut self) -> bool {
        // blitz-shell asserts that a resume it started completes; after a
        // device loss `render` does nothing.
        if self.device_lost {
            return true;
        }
        if self.fallback_reason.is_some() {
            return self.fallback.complete_resume();
        }
        true
    }

    fn suspend(&mut self) {
        self.window = None;
        if self.fallback_reason.is_some() {
            self.fallback.suspend();
        } else if let Some(gpu) = &mut self.gpu {
            gpu.surface = None;
            gpu.configured = None;
        }
    }

    fn is_active(&self) -> bool {
        // Inactive after a device loss: the shell stops requesting redraws.
        if self.device_lost {
            return false;
        }
        if self.fallback_reason.is_some() {
            return self.fallback.is_active();
        }
        self.gpu.as_ref().is_some_and(|gpu| gpu.surface.is_some())
    }

    fn set_size(&mut self, width: u32, height: u32) {
        let changed = (width, height) != (self.width, self.height);
        if self.scale.is_none() && width > 0 {
            // First non-zero size (normally already set by `resume`).
            self.scale = Some(f64::from(width) / f64::from(crate::VIEWPORT.0));
        }
        self.width = width;
        self.height = height;
        if self.device_lost {
            return;
        }
        if self.fallback_reason.is_some() {
            self.fallback.set_size(width, height);
        } else if changed {
            self.configure();
        }
    }

    fn render<F: FnOnce(&mut Scene)>(&mut self, draw_fn: F) {
        if self.device_lost {
            self.calls_after_device_lost += 1;
            return;
        }
        // Checked before any GPU work, by either renderer (module docs).
        if self.page_device_lost() {
            self.stop_on_device_lost();
            self.publish_stats();
            return;
        }
        if self.fallback_reason.is_some() {
            self.fallback.render(draw_fn);
            self.publish_stats();
            return;
        }
        let mut scene = Scene::new();
        draw_fn(&mut scene);
        if let Err(reason) = self.render_netrender(&scene) {
            if self.page_device_lost() {
                // Not a Netrender failure: the fallback shares the device.
                self.stop_on_device_lost();
            } else {
                self.use_fallback(reason);
                // This frame is drawn by the fallback from the same recording.
                self.fallback.render(move |target| *target = scene);
            }
        }
        self.publish_stats();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/netrender-window/index.html");

    /// The fixture page, painted by blitz-paint, translates into exactly the
    /// layers the window fixture checks: page background, `#before`, the
    /// canvas at boundary 2, then `#after` above it.
    #[test]
    fn fixture_scene_translates_in_paint_order() {
        use blitz_dom::DocumentConfig;
        use blitz_html::HtmlDocument;
        use blitz_traits::shell::{ColorScheme, Viewport};

        let mut document = HtmlDocument::from_html(FIXTURE, DocumentConfig::default());
        document.set_viewport(Viewport::new(640, 360, 1.0, ColorScheme::Light));
        let canvas = crate::find_element_by_id(&document, "canvas").expect("fixture canvas");
        let presentation: SharedPresentation = Rc::default();
        let resource =
            crate::presentation::attach_canvas_widget(&presentation, &mut document, canvas);
        document.resolve(0.0);
        let mut scene = Scene::new();
        blitz_paint::paint_scene(&mut scene, &mut document, 1.0, 640, 360, 0, 0);
        let presentation = presentation.borrow();
        let plan =
            translate(&scene, (640, 360), |id| presentation.is_canvas(id)).expect("translated");
        let canvas_index = plan
            .iter()
            .position(|op| matches!(op, PlanOp::Canvas { .. }))
            .expect("canvas op");
        assert_eq!(
            plan[canvas_index],
            PlanOp::Canvas {
                resource,
                rect: [120.0, 80.0, 280.0, 200.0],
                opacity: 1.0
            }
        );
        let rects: Vec<_> = plan
            .iter()
            .filter_map(|op| match op {
                PlanOp::Rect { rect, color } => Some((*rect, *color)),
                PlanOp::Canvas { .. } => None,
            })
            .collect();
        assert!(rects.contains(&([40.0, 40.0, 440.0, 240.0], [1.0, 0.0, 0.0, 1.0])));
        let after = plan
            .iter()
            .position(
                |op| matches!(op, PlanOp::Rect { color, .. } if (color[3] - 0.5).abs() < 0.01),
            )
            .expect("translucent #after");
        assert!(after > canvas_index, "#after paints above the canvas");
        let before = plan
            .iter()
            .position(
                |op| matches!(op, PlanOp::Rect { color, .. } if *color == [1.0, 0.0, 0.0, 1.0]),
            )
            .expect("#before");
        assert!(before < canvas_index, "#before paints below the canvas");
    }

    fn fill(shape: kurbo::BezPath, brush: Paint) -> RenderCommand {
        RenderCommand::Fill(FillCommand {
            fill: peniko::Fill::NonZero,
            transform: Affine::IDENTITY,
            brush,
            brush_transform: None,
            shape,
        })
    }

    fn polygon(points: &[(f64, f64)], close: bool) -> kurbo::BezPath {
        let mut path = kurbo::BezPath::new();
        for (index, point) in points.iter().enumerate() {
            if index == 0 {
                path.move_to(*point);
            } else {
                path.line_to(*point);
            }
        }
        if close {
            path.close_path();
        }
        path
    }

    /// The encodings recorded for the fixture (CI 43850232485 fell back at
    /// frame 0 on the second): blitz-paint's CSS box path has no `ClosePath`,
    /// and the zero-height `html` box repeats a corner.
    #[test]
    fn blitz_rect_encodings_are_accepted() {
        let translate_at = |x: f64, y: f64| Affine::translate((x, y));
        // Sharp CSS box (`#before`): MoveTo + 3 LineTo, implicit close.
        let open_box = polygon(
            &[(0.0, 0.0), (400.0, 0.0), (400.0, 200.0), (0.0, 200.0)],
            false,
        );
        assert_eq!(
            axis_aligned_rect(translate_at(40.0, 40.0), &open_box),
            Some(Rect::new(40.0, 40.0, 440.0, 240.0))
        );
        // Explicit return to the start, with and without ClosePath.
        for close in [false, true] {
            let returned = polygon(
                &[(0.0, 0.0), (8.0, 0.0), (8.0, 4.0), (0.0, 4.0), (0.0, 0.0)],
                close,
            );
            assert_eq!(
                axis_aligned_rect(Affine::IDENTITY, &returned),
                Some(Rect::new(0.0, 0.0, 8.0, 4.0))
            );
        }
        // Zero-height `html` box: no area, nothing painted.
        let flat = polygon(&[(0.0, 0.0), (640.0, 0.0), (640.0, 0.0), (0.0, 0.0)], false);
        assert_eq!(
            axis_aligned_rect(Affine::IDENTITY, &flat),
            Some(Rect::new(0.0, 0.0, 640.0, 0.0))
        );
        let scene = Scene {
            tolerance: 0.1,
            commands: vec![
                fill(flat, Paint::Solid(peniko::Color::BLACK)),
                fill(open_box, Paint::Solid(peniko::Color::WHITE)),
            ],
        };
        assert_eq!(
            translate(&scene, (640, 360), |_| false),
            Ok(vec![PlanOp::Rect {
                rect: [0.0, 0.0, 400.0, 200.0],
                color: [1.0, 1.0, 1.0, 1.0]
            }])
        );
    }

    #[test]
    fn non_rectangular_polygons_are_rejected() {
        let rejected = [
            // Repeated corner: axis-aligned edges, no area.
            polygon(&[(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (8.0, 0.0)], false),
            // Bow tie over the four corners.
            polygon(&[(0.0, 0.0), (8.0, 8.0), (8.0, 0.0), (0.0, 8.0)], true),
            // L shape.
            polygon(
                &[
                    (0.0, 0.0),
                    (8.0, 0.0),
                    (8.0, 4.0),
                    (4.0, 4.0),
                    (4.0, 8.0),
                    (0.0, 8.0),
                ],
                true,
            ),
            // Triangle.
            polygon(&[(0.0, 0.0), (8.0, 0.0), (0.0, 8.0)], true),
            // Two subpaths (ClosePath before more elements).
            {
                let mut path = polygon(&[(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)], true);
                path.line_to((4.0, 4.0));
                path
            },
            {
                let mut path = polygon(&[(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)], true);
                path.move_to((20.0, 20.0));
                path.line_to((30.0, 20.0));
                path
            },
            // Curved corner.
            Rect::new(0.0, 0.0, 8.0, 8.0)
                .to_rounded_rect(2.0)
                .to_path(0.1),
        ];
        for shape in &rejected {
            assert_eq!(
                axis_aligned_rect(Affine::IDENTITY, shape),
                None,
                "{shape:?}"
            );
        }
        // A zero-area polygon is still rejected when transformed.
        let flat = polygon(&[(0.0, 0.0), (8.0, 0.0)], false);
        assert_eq!(axis_aligned_rect(Affine::rotate(0.5), &flat), None);
        // A zero-area fill is skipped, but a zero-area clip hides what it clips.
        let scene = Scene {
            tolerance: 0.1,
            commands: vec![
                RenderCommand::PushClipLayer(anyrender::recording::ClipCommand {
                    fill: peniko::Fill::NonZero,
                    transform: Affine::IDENTITY,
                    clip: flat,
                }),
                fill(
                    Rect::new(0.0, 0.0, 8.0, 8.0).to_path(0.1),
                    Paint::Solid(peniko::Color::WHITE),
                ),
                RenderCommand::PopLayer,
            ],
        };
        assert_eq!(translate(&scene, (64, 64), |_| false), Ok(vec![]));
    }

    #[test]
    fn scenarios_parse_only_known_names() {
        assert_eq!(Scenario::parse("resize"), Some(Scenario::Resize));
        assert_eq!(
            Scenario::parse("device-destroy"),
            Some(Scenario::DeviceDestroy)
        );
        for other in ["", "Resize", "destroy", "device_destroy"] {
            assert_eq!(Scenario::parse(other), None, "{other}");
        }
    }

    /// The resize checks discriminate what they claim: every pixel lies in the
    /// resized viewport, the "added area" pixel lies outside the original one,
    /// and the "not stretched" pixel would sample `#after` (not `#before`
    /// alone) if the 640x360 frame were stretched to the new size.
    #[test]
    fn resize_checks_are_discriminating() {
        let (old_w, old_h) = (f64::from(crate::VIEWPORT.0), f64::from(crate::VIEWPORT.1));
        let (new_w, new_h) = RESIZE_LOGICAL;
        assert!(new_w > old_w && new_h > old_h);
        for (name, x, y, _) in RESIZE_CHECKS {
            assert!(x < new_w && y < new_h, "{name}");
        }
        let added = RESIZE_CHECKS
            .iter()
            .find(|check| check.0.contains("area added"))
            .expect("added-area check");
        assert!(added.1 >= old_w || added.2 >= old_h);
        let edge = RESIZE_CHECKS
            .iter()
            .find(|check| check.0.contains("not stretched"))
            .expect("stretch check");
        // `#after` covers x 200..400, y 60..320; `#before` x 40..440, y 40..240.
        let inside_after =
            |x: f64, y: f64| (200.0..400.0).contains(&x) && (60.0..320.0).contains(&y);
        let inside_before =
            |x: f64, y: f64| (40.0..440.0).contains(&x) && (40.0..240.0).contains(&y);
        assert!(inside_before(edge.1, edge.2) && !inside_after(edge.1, edge.2));
        let (sx, sy) = (edge.1 * old_w / new_w, edge.2 * old_h / new_h);
        assert!(
            inside_after(sx, sy),
            "a stretched frame samples #after there"
        );
        // The canvas pixels differ from the pre-resize (blue) expectation.
        for (name, x, y, expected) in RESIZE_CHECKS {
            if let Some(original) = FIXTURE_CHECKS
                .iter()
                .find(|check| check.1 == x && check.2 == y)
            {
                if name.contains("canvas") {
                    assert_ne!(original.3, expected, "{name}");
                } else {
                    assert_eq!(original.3, expected, "{name}");
                }
            }
        }
    }

    #[test]
    fn untranslated_constructs_are_named() {
        let circle = kurbo::Circle::new((10.0, 10.0), 5.0).to_path(0.1);
        let scene = Scene {
            tolerance: 0.1,
            commands: vec![fill(circle, Paint::Solid(peniko::Color::WHITE))],
        };
        assert_eq!(
            translate(&scene, (64, 64), |_| false),
            Err("non-rectangular or transformed fill".into())
        );
        let rect = Rect::new(0.0, 0.0, 8.0, 8.0).to_path(0.1);
        let rotated = Scene {
            tolerance: 0.1,
            commands: vec![RenderCommand::Fill(FillCommand {
                fill: peniko::Fill::NonZero,
                transform: Affine::rotate(0.5),
                brush: Paint::Solid(peniko::Color::WHITE),
                brush_transform: None,
                shape: rect.clone(),
            })],
        };
        assert!(translate(&rotated, (64, 64), |_| false).is_err());
        let solid = Scene {
            tolerance: 0.1,
            commands: vec![fill(rect, Paint::Solid(peniko::Color::WHITE))],
        };
        assert_eq!(
            translate(&solid, (64, 64), |_| false),
            Ok(vec![PlanOp::Rect {
                rect: [0.0, 0.0, 8.0, 8.0],
                color: [1.0, 1.0, 1.0, 1.0]
            }])
        );
    }
}
