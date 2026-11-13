//! `--window` GPU presentation: the page and its WebGPU canvases composed on
//! deno_webgpu's own wgpu-core device and presented in the native window,
//! without reading any GPU texture back to the CPU.
//!
//! Pieces, all reused except the glue below:
//! - Blitz's `custom-widget` feature paints a `<canvas>` with a WebGPU context
//!   in paint order: `CanvasWidget::paint` emits one fill of its content box
//!   with `Paint::Resource(id)`, a placeholder that anyrender's recording
//!   `Scene` keeps verbatim (a `Paint::Custom` would be dropped there).
//! - `split_scene` cuts the recorded Blitz scene at those placeholders into
//!   raster segments and canvas quads, closing open layers at each cut and
//!   reopening them in the next segment, so DOM content painted after a canvas
//!   (a React UI over the Pixi canvas) stays above it.
//! - Each raster segment is rasterized by Vello CPU (the renderer the probe
//!   already uses) into premultiplied RGBA8 and uploaded with
//!   `queue_write_texture` (CPU to GPU only). Canvas quads sample the page's
//!   current canvas texture directly.
//! - `GpuCompositor` draws the layers in order with one textured-quad pipeline
//!   (premultiplied source-over) on the same wgpu-core `Global`, device and
//!   queue as deno_webgpu (`GPUDevice.instance/id/queue`), so the canvas
//!   texture needs no copy and the compositor's submission is ordered after
//!   the page's own submissions on that queue.
//! - `GpuWindowRenderer` implements anyrender's `WindowRenderer` for
//!   `blitz-shell`: a wgpu-core surface on the window from deno_webgpu's
//!   Instance, configured for the shared device.
//!
//! Limits (documented in the README): the canvas quad ignores clip layers
//! (overflow, clip-path, border radius) but honours the 2D transform and the
//! product of open group opacities; a group with opacity or a blend mode that
//! spans a canvas is applied separately to each side of the cut; a raster
//! segment is either reused unchanged (`RasterCache`) or re-rasterized and
//! uploaded as a full viewport (no damage tracking inside a segment); the
//! canvas must use a non-sRGB 8-bit format (`bgra8unorm`/`rgba8unorm`).

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use anyrender::filters::FilterEffect;
use anyrender::recording::{FillCommand, RenderCommand};
use anyrender::{
    ImageRenderer, Paint, PaintScene, RenderContext, ResourceId, Scene, WindowHandle,
    WindowRenderer,
};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::node::ComputedStyles;
use blitz_dom::{NodeId, Widget};
use deno_core::{OpState, op2};
use deno_error::JsErrorBox;
use deno_webgpu::wgpu_core::id::{
    BindGroupLayoutId, BufferId, DeviceId, QueueId, RenderPipelineId, SamplerId, SurfaceId,
    TextureId, TextureViewId,
};
use deno_webgpu::{wgpu_core, wgpu_types};

use crate::frame_timings::Phase;
use kurbo::{Affine, Point, Rect, Shape};
use peniko::{ImageBrush, ImageSampler};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// Window surface format: 8-bit non-sRGB, so Vello CPU's sRGB-encoded values
/// pass through unchanged, as with the softbuffer window.
const SURFACE_FORMAT: wgpu_types::TextureFormat = wgpu_types::TextureFormat::Bgra8Unorm;
/// Format of the uploaded raster segments (Vello CPU's premultiplied RGBA8).
const RASTER_FORMAT: wgpu_types::TextureFormat = wgpu_types::TextureFormat::Rgba8Unorm;
/// Size of one layer's uniform block (`Layer` in `SHADER`).
const LAYER_UNIFORM_SIZE: u64 = 48;

/// The device deno_webgpu created for the page (the probe's single shared
/// device), registered by dom.js through `op_blitz_present_device`.
#[derive(Clone)]
pub struct PresentDevice {
    pub instance: deno_webgpu::Instance,
    pub device: DeviceId,
    pub queue: QueueId,
}

/// A texture view owned by the embedder. wgpu-core views hold their parent
/// texture (`Arc`), so the canvas texture stays allocated while it is
/// registered here even if the page's `GPUTexture` object (and with it the
/// wgpu-core texture id) has already been collected: the compositor never
/// touches the page's `TextureId`.
pub struct OwnedView {
    instance: deno_webgpu::Instance,
    pub id: TextureViewId,
}

impl Drop for OwnedView {
    fn drop(&mut self) {
        // In-flight submissions keep the view alive inside wgpu-core.
        self.instance.texture_view_drop(self.id);
    }
}

/// The texture a WebGPU canvas renders into this frame.
pub struct CanvasSource {
    pub view: OwnedView,
    pub width: u32,
    pub height: u32,
    pub opaque: bool,
}

/// Shared between the presentation ops (JS side) and the window renderer.
#[derive(Default)]
pub struct Presentation {
    pub device: Option<PresentDevice>,
    /// Placeholder resource id (`ResourceId::into_ffi`) to the canvas texture.
    pub canvases: HashMap<u64, Option<CanvasSource>>,
    /// Canvas node to the placeholder resource of its widget.
    widgets: HashMap<NodeId, ResourceId>,
    /// Resources of widgets Blitz dropped (node removed, widget replaced);
    /// a separate cell because widgets drop inside document mutations that
    /// may run while `Presentation` is borrowed.
    retired: Rc<RefCell<Vec<ResourceId>>>,
}

pub type SharedPresentation = Rc<RefCell<Presentation>>;

impl Presentation {
    fn is_canvas(&self, resource: ResourceId) -> bool {
        self.canvases.contains_key(&resource.into_ffi())
    }

    /// Drops the entries (and owned views) of widgets Blitz has dropped, so a
    /// reused `NodeId` never maps to a stale resource.
    fn prune(&mut self) {
        let retired = std::mem::take(&mut *self.retired.borrow_mut());
        for resource in retired {
            self.unregister(resource);
        }
    }

    /// Removes `resource` from both maps; returns its canvas node, if any.
    fn unregister(&mut self, resource: ResourceId) -> Option<NodeId> {
        self.canvases.remove(&resource.into_ffi());
        let node = self
            .widgets
            .iter()
            .find_map(|(node, id)| (*id == resource).then_some(*node));
        if let Some(node) = node {
            self.widgets.remove(&node);
        }
        node
    }

    /// The canvas context owning `resource` was collected (dom.js
    /// FinalizationRegistry): drop the owned texture view and both map
    /// entries, and detach the placeholder widget if its node still exists.
    /// Unknown or already-forgotten resources are a no-op.
    pub fn forget(&mut self, document: &mut blitz_dom::BaseDocument, resource: ResourceId) {
        self.prune();
        let Some(node) = self.unregister(resource) else {
            return;
        };
        let has_widget = document
            .get_node(node)
            .and_then(|node| node.element_data())
            .is_some_and(|element| element.custom_widget_data().is_some());
        if has_widget {
            // Drops the widget, which retires `resource` again (idempotent).
            document.remove_custom_widget(node);
        }
        self.prune();
    }
}

/// Paints a WebGPU canvas as one placeholder fill of its content box (physical
/// pixels: blitz-paint passes scaled sizes and appends the scene at the content
/// box origin with the element's transform).
struct CanvasWidget {
    resource: ResourceId,
    retired: Rc<RefCell<Vec<ResourceId>>>,
}

impl Drop for CanvasWidget {
    fn drop(&mut self) {
        if let Ok(mut retired) = self.retired.try_borrow_mut() {
            retired.push(self.resource);
        }
    }
}

impl Widget for CanvasWidget {
    fn requires_redraw(&self) -> bool {
        // The canvas changes whenever the page renders a frame.
        true
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Paint::Resource(ImageBrush {
                image: self.resource,
                sampler: ImageSampler::default(),
            })
            .as_ref(),
            None,
            &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        );
        scene
    }
}

/// Attaches the placeholder widget to a canvas node once and returns its
/// resource id.
pub fn attach_canvas_widget(
    presentation: &SharedPresentation,
    document: &mut blitz_dom::BaseDocument,
    node: NodeId,
) -> ResourceId {
    let mut presentation = presentation.borrow_mut();
    presentation.prune();
    if let Some(resource) = presentation.widgets.get(&node) {
        return *resource;
    }
    let resource = ResourceId::new();
    let retired = presentation.retired.clone();
    document.set_custom_widget(node, Box::new(CanvasWidget { resource, retired }));
    // Replacing a widget Blitz still had on the node retires its resource.
    presentation.prune();
    presentation.widgets.insert(node, resource);
    presentation.canvases.insert(resource.into_ffi(), None);
    resource
}

fn shared(state: &Rc<RefCell<OpState>>) -> SharedPresentation {
    state.borrow().borrow::<SharedPresentation>().clone()
}

/// Registers the page's shared WebGPU device for window presentation
/// (captured by dom.js, never exposed to the guest).
#[op2(fast)]
pub fn op_blitz_present_device(
    state: Rc<RefCell<OpState>>,
    #[cppgc] device: &deno_webgpu::device::GPUDevice,
) {
    shared(&state).borrow_mut().device = Some(PresentDevice {
        instance: device.instance.clone(),
        device: device.id,
        queue: device.queue,
    });
}

/// Presents `texture` as the content of the canvas node `handle` (attaching
/// the placeholder widget on first use) and returns the placeholder resource
/// id, which dom.js keeps for `op_blitz_present_canvas_clear/forget`. The
/// registered texture is held through an embedder-owned view (`OwnedView`).
/// Captured by dom.js.
#[op2(fast)]
pub fn op_blitz_present_canvas(
    state: Rc<RefCell<OpState>>,
    handle: f64,
    #[cppgc] texture: &deno_webgpu::texture::GPUTexture,
    opaque: bool,
) -> Result<f64, JsErrorBox> {
    let presentation = shared(&state);
    if let Some(device) = &presentation.borrow().device {
        if texture.device_id != device.device {
            return Err(JsErrorBox::type_error(
                "the canvas is configured with a device other than the presented one",
            ));
        }
    }
    let document = crate::probe_document(&state);
    let node = crate::canvas_node(&document, handle)?;
    let (view, error) = texture.instance.texture_create_view(
        texture.id,
        &wgpu_core::resource::TextureViewDescriptor::default(),
        None,
    );
    let view = OwnedView {
        instance: texture.instance.clone(),
        id: view,
    };
    if let Some(error) = error {
        return Err(JsErrorBox::type_error(format!(
            "canvas texture view for presentation failed: {error}"
        )));
    }
    let resource = attach_canvas_widget(&presentation, &mut document.borrow_mut(), node);
    // Replaces (and drops the view of) the previous texture.
    presentation.borrow_mut().canvases.insert(
        resource.into_ffi(),
        Some(CanvasSource {
            view,
            width: texture.size.width,
            height: texture.size.height,
            opaque,
        }),
    );
    Ok(resource.into_ffi() as f64)
}

/// The canvas has no current texture (unconfigured): its box shows nothing.
/// Stale resources (widget already dropped) are ignored.
#[op2(fast)]
pub fn op_blitz_present_canvas_clear(state: Rc<RefCell<OpState>>, resource: f64) {
    let presentation = shared(&state);
    let mut presentation = presentation.borrow_mut();
    presentation.prune();
    if let Some(source) = presentation.canvases.get_mut(&(resource as u64)) {
        *source = None;
    }
}

/// The canvas context owning `resource` was garbage collected (dom.js
/// FinalizationRegistry): see `Presentation::forget`.
#[op2(fast)]
pub fn op_blitz_present_canvas_forget(state: Rc<RefCell<OpState>>, resource: f64) {
    let presentation = shared(&state);
    let document = crate::probe_document(&state);
    let mut document = document.borrow_mut();
    presentation
        .borrow_mut()
        .forget(&mut document, ResourceId::from_ffi(resource as u64));
}

deno_core::extension!(
    roves_blitz_presentation,
    ops = [
        op_blitz_present_device,
        op_blitz_present_canvas,
        op_blitz_present_canvas_clear,
        op_blitz_present_canvas_forget,
    ],
);

// ---------------------------------------------------------------------------
// Scene splitting (no GPU)

/// A canvas placeholder found in the scene: its resource and the device-space
/// quad of its content box (top-left, top-right, bottom-left, bottom-right).
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasDraw {
    pub resource: ResourceId,
    pub quad: [Point; 4],
    /// Product of the opacities of the layers open around the placeholder.
    pub alpha: f32,
    /// A clip or layer was open around it (ignored by the compositor).
    pub clipped: bool,
}

#[derive(Debug)]
pub enum Segment {
    Raster(Scene),
    Canvas(CanvasDraw),
}

fn draws(command: &RenderCommand) -> bool {
    !matches!(
        command,
        RenderCommand::PushLayer(_) | RenderCommand::PushClipLayer(_) | RenderCommand::PopLayer
    )
}

fn canvas_quad(fill: &FillCommand) -> [Point; 4] {
    let bounds = fill.shape.bounding_box();
    [
        fill.transform * Point::new(bounds.x0, bounds.y0),
        fill.transform * Point::new(bounds.x1, bounds.y0),
        fill.transform * Point::new(bounds.x0, bounds.y1),
        fill.transform * Point::new(bounds.x1, bounds.y1),
    ]
}

/// Splits a recorded scene at the canvas placeholders (see the module docs).
/// Raster segments without any drawing command are omitted.
pub fn split_scene(scene: Scene, is_canvas: impl Fn(ResourceId) -> bool) -> Vec<Segment> {
    let tolerance = scene.tolerance;
    let mut segments = Vec::new();
    let mut open: Vec<RenderCommand> = Vec::new();
    let mut current: Vec<RenderCommand> = Vec::new();
    let flush = |current: Vec<RenderCommand>, open_count: usize, segments: &mut Vec<Segment>| {
        if current.iter().any(draws) {
            let mut commands = current;
            commands.extend(std::iter::repeat_n(RenderCommand::PopLayer, open_count));
            segments.push(Segment::Raster(Scene {
                tolerance,
                commands,
            }));
        }
    };
    for command in scene.commands {
        match &command {
            RenderCommand::PushLayer(_) | RenderCommand::PushClipLayer(_) => {
                open.push(command.clone());
                current.push(command);
            },
            RenderCommand::PopLayer => {
                open.pop();
                current.push(command);
            },
            RenderCommand::Fill(fill) if matches!(&fill.brush, Paint::Resource(brush) if is_canvas(brush.image)) =>
            {
                let Paint::Resource(brush) = &fill.brush else {
                    unreachable!()
                };
                let draw = CanvasDraw {
                    resource: brush.image,
                    quad: canvas_quad(fill),
                    alpha: open
                        .iter()
                        .map(|layer| match layer {
                            RenderCommand::PushLayer(layer) => layer.alpha,
                            _ => 1.0,
                        })
                        .product(),
                    clipped: !open.is_empty(),
                };
                flush(
                    std::mem::replace(&mut current, open.clone()),
                    open.len(),
                    &mut segments,
                );
                segments.push(Segment::Canvas(draw));
            },
            _ => current.push(command),
        }
    }
    flush(current, open.len(), &mut segments);
    segments
}

/// Fixture self-check without a GPU: a real Blitz document with a canvas
/// widget under an absolutely positioned overlay is painted by blitz-paint into
/// a recording scene and split. Prints `presentation_scene_split`.
pub fn self_check() {
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::{ColorScheme, Viewport};

    const HTML: &str = r#"<!doctype html><html><head><style>
        body { margin: 0; background: rgb(255, 255, 255) }
        #stage { position: relative; width: 200px; height: 100px; overflow: hidden }
        canvas { position: absolute; left: 10px; top: 20px; width: 120px; height: 60px }
        #over { position: absolute; left: 30px; top: 30px; width: 20px; height: 20px;
                background: rgb(255, 0, 0) }
        </style></head><body><div id="stage"><canvas id="c" width="120" height="60"></canvas>
        <div id="over"></div></div></body></html>"#;
    let mut document = HtmlDocument::from_html(HTML, DocumentConfig::default());
    document.set_viewport(Viewport::new(240, 120, 1.0, ColorScheme::Light));
    let canvas = crate::find_element_by_id(&document, "c").expect("self-check canvas");
    let presentation: SharedPresentation = Rc::default();
    let resource = attach_canvas_widget(&presentation, &mut document, canvas);
    document.resolve(0.0);
    let mut scene = Scene::new();
    blitz_paint::paint_scene(&mut scene, &mut document, 1.0, 240, 120, 0, 0);
    let presentation = presentation.borrow();
    let segments = split_scene(scene, |id| presentation.is_canvas(id));
    let shape: Vec<&str> = segments
        .iter()
        .map(|segment| match segment {
            Segment::Raster(_) => "raster",
            Segment::Canvas(_) => "canvas",
        })
        .collect();
    let canvas_draw = segments.iter().find_map(|segment| match segment {
        Segment::Canvas(draw) => Some(draw.clone()),
        Segment::Raster(_) => None,
    });
    let overlay_after_canvas = segments
        .iter()
        .skip_while(|segment| matches!(segment, Segment::Raster(_)))
        .any(|segment| match segment {
            Segment::Raster(scene) => scene.commands.iter().any(|command| {
                matches!(command, RenderCommand::Fill(fill)
                    if matches!(fill.brush, Paint::Solid(color) if color.to_rgba8().to_u8_array() == [255, 0, 0, 255]))
            }),
            Segment::Canvas(_) => false,
        });
    // Every raster segment must be balanced (layers reopened and closed).
    let balanced = segments.iter().all(|segment| match segment {
        Segment::Raster(scene) => {
            let mut depth: i32 = 0;
            for command in &scene.commands {
                match command {
                    RenderCommand::PushLayer(_) | RenderCommand::PushClipLayer(_) => depth += 1,
                    RenderCommand::PopLayer => depth -= 1,
                    _ => {},
                }
                if depth < 0 {
                    return false;
                }
            }
            depth == 0
        },
        Segment::Canvas(_) => true,
    });
    let quad = canvas_draw.as_ref().map(|draw| {
        [
            draw.quad[0].x,
            draw.quad[0].y,
            draw.quad[3].x,
            draw.quad[3].y,
        ]
    });
    let passed = shape == ["raster", "canvas", "raster"]
        && canvas_draw
            .as_ref()
            .is_some_and(|draw| draw.resource == resource)
        && quad == Some([10.0, 20.0, 130.0, 80.0])
        && overlay_after_canvas
        && balanced;
    let quad_json = quad.map_or_else(|| "null".to_string(), |quad| format!("{quad:?}"));
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"presentation_scene_split\",\"status\":\"{}\",\"segments\":{shape:?},\"canvas_quad\":{quad_json},\"overlay_after_canvas\":{overlay_after_canvas},\"balanced\":{balanced},\"clipped\":{}}}",
        if passed { "passed" } else { "failed" },
        canvas_draw.as_ref().is_some_and(|draw| draw.clipped),
    );
    assert!(passed, "presentation scene split self-check failed");
}

// ---------------------------------------------------------------------------
// GPU composition (wgpu-core, deno_webgpu's device)

const SHADER: &str = r#"
struct Layer {
    corners01: vec4<f32>, // top-left.xy, top-right.xy (clip space)
    corners23: vec4<f32>, // bottom-left.xy, bottom-right.xy
    params: vec4<f32>,    // alpha, opaque (1 = ignore source alpha)
};
@group(0) @binding(0) var<uniform> layer: Layer;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var corners = array<vec2<f32>, 4>(layer.corners01.xy, layer.corners01.zw,
                                      layer.corners23.xy, layer.corners23.zw);
    var uvs = array<vec2<f32>, 4>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0),
                                  vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0));
    var out: VertexOut;
    out.position = vec4<f32>(corners[index], 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var color = textureSample(source, source_sampler, in.uv);
    if (layer.params.y > 0.5) {
        color.a = 1.0;
    }
    // Premultiplied source, scaled by the group opacity.
    return color * layer.params.x;
}
"#;

/// One textured quad to draw, in order.
struct GpuLayer {
    view: TextureViewId,
    quad: [Point; 4],
    alpha: f32,
    opaque: bool,
}

/// Counters for the `gpu_window_presentation` result line.
#[derive(Default, Clone, Copy, Debug)]
pub struct PresentStats {
    pub frames: u64,
    pub canvas_layers: u64,
    /// Raster segments composed (uploaded or reused).
    pub raster_segments: u64,
    /// Raster segments rasterized by Vello CPU and uploaded.
    pub raster_uploads: u64,
    /// Raster segments reused from the previous upload (`RasterCache`).
    pub raster_cache_hits: u64,
    pub upload_bytes: u64,
    pub max_layers: usize,
    pub skipped_frames: u64,
    pub errors: u64,
}

struct RasterTexture {
    texture: TextureId,
    view: TextureViewId,
}

/// Whether two equal (`PartialEq`) segments are guaranteed to rasterize to the
/// same pixels, so a texture uploaded for one can be reused for the other.
///
/// Audit of the pinned anyrender 0.14 / peniko 0.6 / Vello CPU 0.18 stack:
/// each segment is rasterized alone into a cleared buffer of the viewport
/// size, so its pixels depend only on its commands, the scene tolerance and
/// that size (the Vello CPU image cache only memoizes conversions). Shapes,
/// transforms, strokes, colors and gradients are plain values. Images and
/// fonts compare by `Blob` id, which comes from a global monotonic counter;
/// the bytes behind a `Blob` are shared read-only (`Arc<dyn AsRef<[u8]>>`,
/// Blitz builds them from `Arc<Vec<u8>>` or static data), so an equal id means
/// equal bytes, and width/height/format/alpha type are compared too. A NaN
/// value makes `PartialEq` fail, which only costs an upload; the cache uses
/// `raster_cache_equal`, which accepts only bit-identical NaN fill paths
/// (equal input bits rasterize identically). Excluded, because
/// they reference mutable or external state that equality does not capture:
/// `Paint::Resource` (an embedder resource id; Vello CPU paints it transparent
/// today, but its content is not part of the command), `Paint::Custom`
/// (compared by pointer) and filters with an external image
/// (`FilterEffect::Image`, an id into embedder state).
fn reusable(scene: &Scene) -> bool {
    let stable_paint = |paint: &Paint| !matches!(paint, Paint::Resource(_) | Paint::Custom(_));
    scene.commands.iter().all(|command| match command {
        RenderCommand::PushLayer(layer) => [&layer.filter, &layer.backdrop_filter]
            .into_iter()
            .flatten()
            .flat_map(|filter| filter.nodes())
            .all(|node| !matches!(node.effect, FilterEffect::Image(_))),
        RenderCommand::Fill(fill) => stable_paint(&fill.brush),
        RenderCommand::Stroke(stroke) => stable_paint(&stroke.brush),
        RenderCommand::GlyphRun(run) => stable_paint(&run.brush),
        RenderCommand::PushClipLayer(_) | RenderCommand::PopLayer | RenderCommand::BoxShadow(_) => {
            true
        },
    })
}

/// Raster cache equality: `PartialEq`, except that two `Fill` commands whose
/// paths are bit-for-bit identical (same element count, verbs and every
/// coordinate's `f64::to_bits`) but `PartialEq`-unequal, which happens only
/// when the path holds NaN (`NaN != NaN`), still compare equal provided every
/// other field of the fill (fill rule, transform, brush, brush transform)
/// is `PartialEq`-equal. Scene tolerance and command count must be equal
/// too. Nothing else is relaxed: a NaN anywhere else (transform, brush,
/// other commands, tolerance) or a different NaN payload is still a miss.
/// Sound because the rasterizer is deterministic on its input bits, so
/// bit-identical commands rasterize to the same pixels (see `reusable`).
/// Confirmed cause on the original template (GTX 1050/Vulkan, JITless,
/// 300 frames): 299 `content_changed` misses were fills with NaN `MoveTo` and
/// `LineTo` coordinates and bit-identical paths.
fn raster_cache_equal(cached: &Scene, current: &Scene) -> bool {
    if cached == current {
        return true;
    }
    cached.tolerance == current.tolerance
        && cached.commands.len() == current.commands.len()
        && cached
            .commands
            .iter()
            .zip(&current.commands)
            .all(|(cached, current)| cached == current || nan_identical_fills(cached, current))
}

/// Whether two `Fill` commands differ by `PartialEq` only because their
/// bit-identical paths contain NaN (see `raster_cache_equal`).
fn nan_identical_fills(cached: &RenderCommand, current: &RenderCommand) -> bool {
    use crate::frame_timings::PathBits;
    let (RenderCommand::Fill(cached), RenderCommand::Fill(current)) = (cached, current) else {
        return false;
    };
    if cached.shape == current.shape
        || path_bits(&cached.shape, &current.shape) != PathBits::SameBitwise
    {
        return false;
    }
    // Every other field compares by `PartialEq`. The paths, already proven
    // bit-identical above, are replaced by the same empty path on both sides:
    // comparing them again by `PartialEq` would fail on their NaN.
    let without_shape = |fill: &FillCommand| FillCommand {
        shape: kurbo::BezPath::new(),
        ..fill.clone()
    };
    without_shape(cached) == without_shape(current)
}

/// The scene last uploaded, without error, into each raster texture (by
/// segment index), so an equal segment in the same position reuses it.
/// Cleared on resize (textures recreated) and whenever a frame is discarded;
/// an entry is dropped before each upload and stored again only if the
/// upload reported no error.
/// With `disabled` (`ROVES_PROBE_DISABLE_RASTER_CACHE=1`, an A/B diagnostic)
/// nothing is stored and every segment is rasterized and uploaded again.
#[derive(Default)]
struct RasterCache {
    contents: Vec<Option<Scene>>,
    disabled: bool,
}

/// `ROVES_PROBE_DISABLE_RASTER_CACHE=1`: the GPU compositor re-rasterizes and
/// uploads every segment each frame (cache A/B comparison; default enabled).
pub fn raster_cache_disabled_by_env() -> bool {
    std::env::var("ROVES_PROBE_DISABLE_RASTER_CACHE").is_ok_and(|value| value == "1")
}

impl RasterCache {
    fn new(disabled: bool) -> Self {
        Self {
            contents: Vec::new(),
            disabled,
        }
    }

    fn is_current(&self, index: usize, scene: &Scene) -> bool {
        !self.disabled
            && reusable(scene)
            && self
                .contents
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|cached| raster_cache_equal(cached, scene))
    }

    /// Forgets the content of texture `index` (an upload into it begins).
    fn invalidate(&mut self, index: usize) {
        if let Some(content) = self.contents.get_mut(index) {
            *content = None;
        }
    }

    /// Records the scene just uploaded into texture `index` without error
    /// (kept only if reusable).
    fn store(&mut self, index: usize, scene: Scene) {
        if self.disabled {
            return;
        }
        if self.contents.len() <= index {
            self.contents.resize_with(index + 1, || None);
        }
        self.contents[index] = reusable(&scene).then_some(scene);
    }

    fn clear(&mut self) {
        self.contents.clear();
    }
}

/// Why a raster cache entry is empty, as far as the diagnostics saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EmptyReason {
    Resize,
    Discard,
    UploadError,
    NotCached,
}

fn command_kind(command: &RenderCommand) -> crate::frame_timings::CommandKind {
    use crate::frame_timings::CommandKind;
    match command {
        RenderCommand::Fill(_) => CommandKind::Fill,
        RenderCommand::Stroke(_) => CommandKind::Stroke,
        RenderCommand::GlyphRun(_) => CommandKind::GlyphRun,
        RenderCommand::PushLayer(_) => CommandKind::PushLayer,
        RenderCommand::PushClipLayer(_) => CommandKind::PushClipLayer,
        RenderCommand::PopLayer => CommandKind::PopLayer,
        RenderCommand::BoxShadow(_) => CommandKind::BoxShadow,
    }
}

/// The transformed bounding box of a fill, in scene coordinates.
fn fill_bounds(fill: &FillCommand) -> Rect {
    fill.transform
        .transform_rect_bbox(fill.shape.bounding_box())
}

/// Diagnostics only: buckets a fill's path element count, records which
/// `PathEl` point slots, value classes and axes are non-finite (presence
/// flags) and finds the first non-finite stage of `fill_bounds` +
/// `Rect::area`, using public Kurbo APIs (no geometry is kept).
fn path_class(fill: &FillCommand) -> crate::frame_timings::PathClass {
    use crate::frame_timings::NonFiniteClass::{
        CurveCtrl1, CurveCtrl2, CurveEnd, LineTo, MoveTo, QuadCtrl, QuadEnd,
    };
    use crate::frame_timings::{BoundsOrigin, ElementBucket, NonFiniteSlots, PathClass};
    let elements = fill.shape.elements();
    let mut non_finite = NonFiniteSlots::NONE;
    let mut point = |slot, p: kurbo::Point| non_finite.point(slot, p.x, p.y);
    for element in elements {
        match *element {
            kurbo::PathEl::MoveTo(p) => point(MoveTo, p),
            kurbo::PathEl::LineTo(p) => point(LineTo, p),
            kurbo::PathEl::QuadTo(p1, p2) => {
                point(QuadCtrl, p1);
                point(QuadEnd, p2);
            },
            kurbo::PathEl::CurveTo(p1, p2, p3) => {
                point(CurveCtrl1, p1);
                point(CurveCtrl2, p2);
                point(CurveEnd, p3);
            },
            kurbo::PathEl::ClosePath => {},
        }
    }
    let local_box = fill.shape.bounding_box();
    let transformed_box = fill.transform.transform_rect_bbox(local_box);
    PathClass {
        elements: ElementBucket::of(elements.len()),
        non_finite,
        origin: BoundsOrigin::of(
            non_finite == NonFiniteSlots::NONE,
            fill.transform.is_finite(),
            local_box.is_finite(),
            transformed_box.is_finite(),
            transformed_box.area().is_finite(),
        ),
    }
}

/// Compares two paths by element count, `PathEl` kinds and coordinate
/// `f64::to_bits`, in memory (only the class is kept). Used by the
/// diagnostics and by `nan_identical_fills` (raster cache equality).
fn path_bits(cached: &kurbo::BezPath, current: &kurbo::BezPath) -> crate::frame_timings::PathBits {
    use crate::frame_timings::PathBits;
    use kurbo::PathEl::*;
    let same = |a: kurbo::Point, b: kurbo::Point| {
        a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits()
    };
    let (cached, current) = (cached.elements(), current.elements());
    if cached.len() != current.len() {
        return PathBits::CountMismatch;
    }
    let mut points = true;
    for (cached_element, current_element) in cached.iter().zip(current) {
        points &= match (*cached_element, *current_element) {
            (MoveTo(a), MoveTo(b)) | (LineTo(a), LineTo(b)) => same(a, b),
            (QuadTo(a1, a2), QuadTo(b1, b2)) => same(a1, b1) && same(a2, b2),
            (CurveTo(a1, a2, a3), CurveTo(b1, b2, b3)) => {
                same(a1, b1) && same(a2, b2) && same(a3, b3)
            },
            (ClosePath, ClosePath) => true,
            _ => return PathBits::VerbMismatch,
        };
    }
    if points {
        PathBits::SameBitwise
    } else {
        PathBits::PointBitsDiffer
    }
}

/// Diagnostics only: classifies a first differing `current` fill by its
/// bounds area and path class and, if `cached` is a fill too, by which
/// public fields differ and the cached path class (flags and buckets only;
/// no geometry, color or index is kept).
fn fill_diff(cached: &RenderCommand, current: &FillCommand) -> crate::frame_timings::FillDiff {
    use crate::frame_timings::{AreaBucket, FillChanges, FillDiff};
    let bounds = fill_bounds(current);
    FillDiff {
        area: AreaBucket::of(bounds.area()),
        current: path_class(current),
        cached: match cached {
            RenderCommand::Fill(cached) => Some(path_class(cached)),
            _ => None,
        },
        changes: match cached {
            RenderCommand::Fill(cached) => {
                let transform = cached.transform != current.transform;
                let brush = cached.brush != current.brush;
                let shape = cached.shape != current.shape;
                Some(FillChanges {
                    transform,
                    brush,
                    shape,
                    // The commands differ, so another field (fill rule or
                    // brush transform) does when these three are equal.
                    other: !(transform || brush || shape),
                    bounds: fill_bounds(cached) != bounds,
                    path_bits: path_bits(&cached.shape, &current.shape),
                })
            },
            _ => None,
        },
    }
}

/// Diagnostics only: where `current` first differs from `cached` (kinds and
/// flags, no content, index or allocation). Called for `content_changed`
/// misses with `ROVES_PROBE_FRAME_TIMINGS=1`.
fn content_diff(cached: &Scene, current: &Scene) -> crate::frame_timings::ContentDiff {
    use crate::frame_timings::{ContentDiff, FirstDiff};
    let length_changed = cached.commands.len() != current.commands.len();
    match cached
        .commands
        .iter()
        .zip(&current.commands)
        .find(|(cached, current)| cached != current)
    {
        Some((cached, current)) => {
            let (before, after) = (command_kind(cached), command_kind(current));
            ContentDiff {
                first: FirstDiff::Command(after),
                length_changed,
                kind_changed: before != after,
                fill: match current {
                    RenderCommand::Fill(fill) => Some(fill_diff(cached, fill)),
                    _ => None,
                },
            }
        },
        None => ContentDiff {
            first: if length_changed {
                FirstDiff::LengthOnly
            } else {
                FirstDiff::NoCommandDiff
            },
            length_changed,
            kind_changed: false,
            fill: None,
        },
    }
}

/// `ROVES_PROBE_FRAME_TIMINGS=1` only: classifies raster cache misses by
/// mirroring why each entry of `RasterCache` is empty. Read-only with respect
/// to the cache: it never changes what is reused or uploaded.
#[derive(Default)]
struct MissTracker {
    /// By texture index; `None` while the entry is (or may be) populated.
    empty: Vec<Option<EmptyReason>>,
    /// Misses of the frame being composed.
    frame: crate::frame_timings::MissCounts,
    /// Misses of presented frames (committed like `PresentStats`).
    committed: crate::frame_timings::MissCounts,
    /// `content_changed` classifications of the frame being composed and of
    /// presented frames, committed/discarded together with the misses.
    frame_diff: crate::frame_timings::DiffCounts,
    committed_diff: crate::frame_timings::DiffCounts,
}

impl MissTracker {
    /// Called on a miss, before the entry is invalidated.
    fn classify(
        &self,
        cache: &RasterCache,
        index: usize,
        scene: &Scene,
    ) -> crate::frame_timings::MissReason {
        use crate::frame_timings::MissReason;
        if cache.disabled {
            MissReason::CacheDisabled
        } else if !reusable(scene) {
            MissReason::NotReusable
        } else if cache.contents.get(index).is_some_and(Option::is_some) {
            MissReason::ContentChanged
        } else {
            match self.empty.get(index).copied().flatten() {
                None => MissReason::FirstUse,
                Some(EmptyReason::Resize) => MissReason::AfterResize,
                Some(EmptyReason::Discard) => MissReason::AfterDiscard,
                Some(EmptyReason::UploadError) => MissReason::AfterUploadError,
                Some(EmptyReason::NotCached) => MissReason::PreviousNotCached,
            }
        }
    }

    /// Called on a miss, before the entry is invalidated. A `content_changed`
    /// miss is also compared with the cached segment, timed as `CacheDiff`.
    fn miss(
        &mut self,
        cache: &RasterCache,
        index: usize,
        scene: &Scene,
        phases: &mut crate::frame_timings::FramePhases,
    ) {
        use crate::frame_timings::MissReason;
        let reason = self.classify(cache, index, scene);
        self.frame.add(reason);
        if reason == MissReason::ContentChanged
            && let Some(Some(cached)) = cache.contents.get(index)
        {
            let start = std::time::Instant::now();
            self.frame_diff.add(content_diff(cached, scene));
            phases.add(Phase::CacheDiff, start.elapsed());
        }
    }

    /// The upload into texture `index` finished: `stored` if the cache kept
    /// it, `failed` if it reported an error.
    fn uploaded(&mut self, index: usize, stored: bool, failed: bool) {
        if self.empty.len() <= index {
            self.empty.resize(index + 1, None);
        }
        self.empty[index] = if failed {
            Some(EmptyReason::UploadError)
        } else if stored {
            None
        } else {
            Some(EmptyReason::NotCached)
        };
    }

    /// The whole cache was cleared (`RasterCache::clear`).
    fn cleared(&mut self, reason: EmptyReason) {
        self.empty.fill(Some(reason));
    }

    fn commit_frame(&mut self) {
        let frame = std::mem::take(&mut self.frame);
        self.committed.merge(&frame);
        let diff = std::mem::take(&mut self.frame_diff);
        self.committed_diff.merge(&diff);
    }

    fn discard_frame(&mut self) {
        self.frame = Default::default();
        self.frame_diff = Default::default();
        self.cleared(EmptyReason::Discard);
    }
}

/// `ROVES_PROBE_FRAME_TIMINGS=1` only: per-phase times of the current frame
/// and the cache-miss classification. `None` otherwise: no timestamp is taken.
#[derive(Default)]
struct Diagnostics {
    phases: crate::frame_timings::FramePhases,
    misses: MissTracker,
}

pub struct GpuCompositor {
    device: PresentDevice,
    pipeline: RenderPipelineId,
    layout: BindGroupLayoutId,
    sampler: SamplerId,
    uniforms: Vec<BufferId>,
    rasters: Vec<RasterTexture>,
    raster_cache: RasterCache,
    raster_size: (u32, u32),
    vello: VelloCpuImageRenderer,
    pixels: Vec<u8>,
    /// Committed counters: only frames presented without error.
    pub stats: PresentStats,
    /// Counters of the frame being composed (see `commit_frame`).
    frame: PresentStats,
    /// `Some` only with `ROVES_PROBE_FRAME_TIMINGS=1` (set by the renderer).
    diagnostics: Option<Diagnostics>,
}

fn report<E: std::fmt::Display>(stats: &mut PresentStats, what: &str, error: Option<E>) {
    if let Some(error) = error {
        stats.errors += 1;
        eprintln!("ROVES_PROBE_PRESENT_ERROR {what}: {error}");
    }
}

impl GpuCompositor {
    pub fn new(device: PresentDevice) -> Self {
        let global = &device.instance;
        let (module, error) = global.device_create_shader_module(
            device.device,
            &wgpu_core::pipeline::ShaderModuleDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
                runtime_checks: wgpu_types::ShaderRuntimeChecks::default(),
            },
            wgpu_core::pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(SHADER)),
            None,
        );
        if let Some(error) = error {
            panic!("compositor shader failed: {error}");
        }
        let entries = vec![
            wgpu_types::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu_types::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu_types::BindingType::Buffer {
                    ty: wgpu_types::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu_types::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu_types::ShaderStages::FRAGMENT,
                ty: wgpu_types::BindingType::Texture {
                    sample_type: wgpu_types::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu_types::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu_types::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu_types::ShaderStages::FRAGMENT,
                ty: wgpu_types::BindingType::Sampler(wgpu_types::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let (layout, error) = global.device_create_bind_group_layout(
            device.device,
            &wgpu_core::binding_model::BindGroupLayoutDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
                entries: Cow::Owned(entries),
            },
            None,
        );
        if let Some(error) = error {
            panic!("compositor bind group layout failed: {error}");
        }
        let (pipeline_layout, error) = global.device_create_pipeline_layout(
            device.device,
            &wgpu_core::binding_model::PipelineLayoutDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
                bind_group_layouts: Cow::Owned(vec![Some(layout)]),
                immediate_size: 0,
            },
            None,
        );
        if let Some(error) = error {
            panic!("compositor pipeline layout failed: {error}");
        }
        let blend = wgpu_types::BlendComponent {
            src_factor: wgpu_types::BlendFactor::One,
            dst_factor: wgpu_types::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu_types::BlendOperation::Add,
        };
        let stage = |entry: &'static str| wgpu_core::pipeline::ProgrammableStageDescriptor {
            module,
            entry_point: Some(Cow::Borrowed(entry)),
            constants: Default::default(),
            zero_initialize_workgroup_memory: true,
        };
        let (pipeline, error) = global.device_create_render_pipeline(
            device.device,
            &wgpu_core::pipeline::RenderPipelineDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
                layout: Some(pipeline_layout),
                vertex: wgpu_core::pipeline::VertexState {
                    stage: stage("vs_main"),
                    buffers: Cow::Owned(Vec::new()),
                },
                primitive: wgpu_types::PrimitiveState {
                    topology: wgpu_types::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu_types::MultisampleState::default(),
                fragment: Some(wgpu_core::pipeline::FragmentState {
                    stage: stage("fs_main"),
                    targets: Cow::Owned(vec![Some(wgpu_types::ColorTargetState {
                        format: SURFACE_FORMAT,
                        blend: Some(wgpu_types::BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                        write_mask: wgpu_types::ColorWrites::ALL,
                    })]),
                }),
                cache: None,
                multiview_mask: None,
            },
            None,
        );
        if let Some(error) = error {
            panic!("compositor pipeline failed: {error}");
        }
        let (sampler, error) = global.device_create_sampler(
            device.device,
            &wgpu_core::resource::SamplerDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
                address_modes: [wgpu_types::AddressMode::ClampToEdge; 3],
                mag_filter: wgpu_types::FilterMode::Linear,
                min_filter: wgpu_types::FilterMode::Linear,
                mipmap_filter: wgpu_types::MipmapFilterMode::Nearest,
                lod_min_clamp: 0.0,
                lod_max_clamp: 32.0,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            },
            None,
        );
        if let Some(error) = error {
            panic!("compositor sampler failed: {error}");
        }
        Self {
            device,
            pipeline,
            layout,
            sampler,
            uniforms: Vec::new(),
            rasters: Vec::new(),
            raster_cache: RasterCache::new(raster_cache_disabled_by_env()),
            raster_size: (0, 0),
            vello: VelloCpuImageRenderer::new(1, 1),
            pixels: Vec::new(),
            stats: PresentStats::default(),
            frame: PresentStats::default(),
            diagnostics: None,
        }
    }

    pub fn device(&self) -> &PresentDevice {
        &self.device
    }

    /// A timestamp only when diagnostics are on.
    fn phase_start(&self) -> Option<std::time::Instant> {
        self.diagnostics.as_ref().map(|_| std::time::Instant::now())
    }

    fn phase_end(&mut self, start: Option<std::time::Instant>, phase: Phase) {
        if let (Some(start), Some(diagnostics)) = (start, &mut self.diagnostics) {
            diagnostics.phases.add(phase, start.elapsed());
        }
    }

    /// Current sum of the phases nested in the segment loop of `compose`.
    fn nested_segment_phases(&self) -> std::time::Duration {
        self.diagnostics
            .as_ref()
            .map_or_else(Default::default, |diagnostics| {
                diagnostics.phases.sum(&Phase::SEGMENT_NESTED)
            })
    }

    fn release_rasters(&mut self) {
        self.raster_cache.clear();
        let global = &self.device.instance;
        for raster in self.rasters.drain(..) {
            global.texture_view_drop(raster.view);
            global.texture_drop(raster.texture);
        }
    }

    /// A full-viewport texture for raster segment `index`.
    fn raster(&mut self, index: usize, width: u32, height: u32) -> TextureViewId {
        if self.raster_size != (width, height) {
            if let Some(diagnostics) = &mut self.diagnostics {
                diagnostics.misses.cleared(EmptyReason::Resize);
            }
            self.release_rasters();
            self.raster_size = (width, height);
            self.vello.resize(width, height);
        }
        while self.rasters.len() <= index {
            let global = &self.device.instance;
            let (texture, error) = global.device_create_texture(
                self.device.device,
                &wgpu_core::resource::TextureDescriptor {
                    label: Some(Cow::Borrowed("roves-page-segment")),
                    size: wgpu_types::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu_types::TextureDimension::D2,
                    format: RASTER_FORMAT,
                    usage: wgpu_types::TextureUsages::TEXTURE_BINDING
                        | wgpu_types::TextureUsages::COPY_DST,
                    view_formats: Vec::new(),
                },
                None,
            );
            report(&mut self.stats, "raster texture", error);
            let view = self.view(texture);
            self.rasters.push(RasterTexture { texture, view });
        }
        self.rasters[index].view
    }

    fn view(&mut self, texture: TextureId) -> TextureViewId {
        let (view, error) = self.device.instance.texture_create_view(
            texture,
            &wgpu_core::resource::TextureViewDescriptor::default(),
            None,
        );
        report(&mut self.stats, "texture view", error);
        view
    }

    fn uniform(&mut self, index: usize) -> BufferId {
        while self.uniforms.len() <= index {
            let (buffer, error) = self.device.instance.device_create_buffer(
                self.device.device,
                &wgpu_core::resource::BufferDescriptor {
                    label: Some(Cow::Borrowed("roves-compositor-layer")),
                    size: LAYER_UNIFORM_SIZE,
                    usage: wgpu_types::BufferUsages::UNIFORM | wgpu_types::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
                None,
            );
            report(&mut self.stats, "layer uniform", error);
            self.uniforms.push(buffer);
        }
        self.uniforms[index]
    }

    /// Rasterizes one segment with Vello CPU and uploads it (CPU to GPU only),
    /// unless texture `index` already holds an equal reusable segment.
    fn upload_segment(
        &mut self,
        index: usize,
        scene: Scene,
        width: u32,
        height: u32,
    ) -> TextureViewId {
        let errors_before = self.stats.errors;
        // Recreates the textures (and clears the cache) on resize.
        let view = self.raster(index, width, height);
        self.frame.raster_segments += 1;
        let check = self.phase_start();
        let current = self.raster_cache.is_current(index, &scene);
        self.phase_end(check, Phase::CacheCheck);
        if current {
            self.frame.raster_cache_hits += 1;
            return view;
        }
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics
                .misses
                .miss(&self.raster_cache, index, &scene, &mut diagnostics.phases);
        }
        self.raster_cache.invalidate(index);
        // Kept only for comparison with the next frame's segment.
        let cached = (!self.raster_cache.disabled && reusable(&scene)).then(|| scene.clone());
        let raster = self.phase_start();
        self.pixels.clear();
        self.pixels.resize(width as usize * height as usize * 4, 0);
        let pixels = &mut self.pixels;
        self.vello.render(
            |painter| painter.append_scene(scene, Affine::IDENTITY),
            pixels,
        );
        self.vello.reset();
        self.phase_end(raster, Phase::Raster);
        let upload = self.phase_start();
        let error = self.device.instance.queue_write_texture(
            self.device.queue,
            &wgpu_types::TexelCopyTextureInfo {
                texture: self.rasters[index].texture,
                mip_level: 0,
                origin: wgpu_types::Origin3d::ZERO,
                aspect: wgpu_types::TextureAspect::All,
            },
            &self.pixels,
            &wgpu_types::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            &wgpu_types::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.phase_end(upload, Phase::TextureUpload);
        report(&mut self.stats, "segment upload", error.err());
        let failed = self.stats.errors != errors_before;
        let stored = cached.is_some() && !failed;
        // A failed texture creation or write leaves unknown contents: the
        // entry stays invalid and the next frame uploads again.
        if let Some(cached) = cached
            && self.stats.errors == errors_before
        {
            self.raster_cache.store(index, cached);
        }
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics.misses.uploaded(index, stored, failed);
        }
        self.frame.upload_bytes += self.pixels.len() as u64;
        self.frame.raster_uploads += 1;
        view
    }

    /// Composes `segments` into `target` (a `width`x`height` view of
    /// `SURFACE_FORMAT`) over an opaque white background and submits.
    pub fn compose(
        &mut self,
        segments: Vec<Segment>,
        canvases: &HashMap<u64, Option<CanvasSource>>,
        target: TextureViewId,
        width: u32,
        height: u32,
    ) {
        self.frame = PresentStats::default();
        let mut layers = Vec::new();
        let full = [
            Point::new(0.0, 0.0),
            Point::new(f64::from(width), 0.0),
            Point::new(0.0, f64::from(height)),
            Point::new(f64::from(width), f64::from(height)),
        ];
        let mut raster_index = 0;
        // The nested cache check/raster/upload phases keep their own series
        // and are subtracted from this one.
        let compose = self.phase_start();
        let nested_before = self.nested_segment_phases();
        for segment in segments {
            match segment {
                Segment::Raster(scene) => {
                    let view = self.upload_segment(raster_index, scene, width, height);
                    raster_index += 1;
                    layers.push(GpuLayer {
                        view,
                        quad: full,
                        alpha: 1.0,
                        opaque: false,
                    });
                },
                Segment::Canvas(draw) => {
                    let Some(Some(source)) = canvases.get(&draw.resource.into_ffi()) else {
                        continue;
                    };
                    if source.width == 0 || source.height == 0 {
                        continue;
                    }
                    // The embedder-owned view keeps the canvas texture alive
                    // for this draw and its submission.
                    self.frame.canvas_layers += 1;
                    layers.push(GpuLayer {
                        view: source.view.id,
                        quad: draw.quad,
                        alpha: draw.alpha,
                        opaque: source.opaque,
                    });
                },
            }
        }
        if let (Some(start), Some(diagnostics)) = (compose, &mut self.diagnostics) {
            let elapsed = start.elapsed();
            let nested = diagnostics
                .phases
                .sum(&Phase::SEGMENT_NESTED)
                .saturating_sub(nested_before);
            diagnostics
                .phases
                .add(Phase::ComposeSegments, elapsed.saturating_sub(nested));
        }
        self.frame.max_layers = layers.len();
        let encode = self.phase_start();
        let global = self.device.instance.clone();
        let mut bind_groups = Vec::new();
        for (index, layer) in layers.iter().enumerate() {
            let uniform = self.uniform(index);
            let clip = |point: Point| {
                [
                    (point.x / f64::from(width) * 2.0 - 1.0) as f32,
                    (1.0 - point.y / f64::from(height) * 2.0) as f32,
                ]
            };
            let [a, b, c, d] = layer.quad.map(clip);
            let values: [f32; 12] = [
                a[0],
                a[1],
                b[0],
                b[1],
                c[0],
                c[1],
                d[0],
                d[1],
                layer.alpha,
                if layer.opaque { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ];
            let bytes: Vec<u8> = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let error = global.queue_write_buffer(self.device.queue, uniform, 0, &bytes);
            report(&mut self.stats, "layer uniform write", error.err());
            let (bind_group, error) = global.device_create_bind_group(
                self.device.device,
                &wgpu_core::binding_model::BindGroupDescriptor {
                    label: None,
                    layout: self.layout,
                    entries: Cow::Owned(vec![
                        wgpu_core::binding_model::BindGroupEntry {
                            binding: 0,
                            resource: wgpu_core::binding_model::BindingResource::Buffer(
                                wgpu_core::binding_model::BufferBinding {
                                    buffer: uniform,
                                    offset: 0,
                                    size: None,
                                },
                            ),
                        },
                        wgpu_core::binding_model::BindGroupEntry {
                            binding: 1,
                            resource: wgpu_core::binding_model::BindingResource::TextureView(
                                layer.view,
                            ),
                        },
                        wgpu_core::binding_model::BindGroupEntry {
                            binding: 2,
                            resource: wgpu_core::binding_model::BindingResource::Sampler(
                                self.sampler,
                            ),
                        },
                    ]),
                },
                None,
            );
            report(&mut self.stats, "layer bind group", error);
            bind_groups.push(bind_group);
        }

        let (encoder, error) = global.device_create_command_encoder(
            self.device.device,
            &wgpu_types::CommandEncoderDescriptor {
                label: Some(Cow::Borrowed("roves-compositor")),
            },
            None,
        );
        report(&mut self.stats, "command encoder", error);
        let (mut pass, error) = global.command_encoder_begin_render_pass(
            encoder,
            &wgpu_core::command::RenderPassDescriptor {
                label: None,
                color_attachments: Cow::Owned(vec![Some(
                    wgpu_core::command::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        load_op: wgpu_types::LoadOp::Clear(wgpu_types::Color::WHITE),
                        store_op: wgpu_types::StoreOp::Store,
                    },
                )]),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            },
        );
        report(&mut self.stats, "render pass", error);
        report(
            &mut self.stats,
            "set pipeline",
            global
                .render_pass_set_pipeline(&mut pass, self.pipeline)
                .err(),
        );
        for bind_group in &bind_groups {
            report(
                &mut self.stats,
                "set bind group",
                global
                    .render_pass_set_bind_group(&mut pass, 0, Some(*bind_group), &[])
                    .err(),
            );
            report(
                &mut self.stats,
                "draw",
                global.render_pass_draw(&mut pass, 4, 1, 0, 0).err(),
            );
        }
        report(
            &mut self.stats,
            "end pass",
            global.render_pass_end(&mut pass).err(),
        );
        let (commands, error) = global.command_encoder_finish(
            encoder,
            &wgpu_types::CommandBufferDescriptor { label: None },
            None,
        );
        report(&mut self.stats, "finish", error.map(|(_, error)| error));
        if let Err((_, error)) = global.queue_submit(self.device.queue, &[commands]) {
            report(&mut self.stats, "submit", Some(error));
        }
        // wgpu-core keeps the resources alive until the submission completes.
        for bind_group in bind_groups {
            global.bind_group_drop(bind_group);
        }
        self.phase_end(encode, Phase::EncodeSubmit);
    }

    /// Adds the counters of the frame just composed to `stats`: called only
    /// once the frame was presented without any error.
    fn commit_frame(&mut self) {
        let frame = std::mem::take(&mut self.frame);
        self.stats.frames += 1;
        self.stats.canvas_layers += frame.canvas_layers;
        self.stats.raster_segments += frame.raster_segments;
        self.stats.raster_uploads += frame.raster_uploads;
        self.stats.raster_cache_hits += frame.raster_cache_hits;
        self.stats.upload_bytes += frame.upload_bytes;
        self.stats.max_layers = self.stats.max_layers.max(frame.max_layers);
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics.misses.commit_frame();
        }
    }

    /// The frame was not presented (or had an error): count it as skipped.
    /// Its uploads may not have reached the textures (e.g. a failed submit),
    /// so no segment is reused from it.
    fn discard_frame(&mut self) {
        self.raster_cache.clear();
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics.misses.discard_frame();
        }
        self.frame = PresentStats::default();
        self.stats.skipped_frames += 1;
    }
}

impl Drop for GpuCompositor {
    fn drop(&mut self) {
        self.release_rasters();
        let global = &self.device.instance;
        for buffer in self.uniforms.drain(..) {
            global.buffer_drop(buffer);
        }
        global.sampler_drop(self.sampler);
        global.render_pipeline_drop(self.pipeline);
        global.bind_group_layout_drop(self.layout);
    }
}

// ---------------------------------------------------------------------------
// Window renderer

/// The window surface (RAII): dropping it (suspend, or the renderer dropped on
/// a normal window close) destroys the wgpu-core surface first and only then
/// releases the window handle (`Drop::drop` runs before the fields drop).
struct ActiveSurface {
    instance: deno_webgpu::Instance,
    surface: SurfaceId,
    _window: Arc<dyn WindowHandle>,
}

impl Drop for ActiveSurface {
    fn drop(&mut self) {
        self.instance.surface_drop(self.surface);
    }
}

/// anyrender `WindowRenderer` for blitz-shell: records the Blitz scene, splits
/// it at canvas placeholders and composes it on deno_webgpu's device.
pub struct GpuWindowRenderer {
    // Declared first: the surface (configured on the compositor's device) is
    // dropped before the compositor and before the window it references.
    surface: Option<ActiveSurface>,
    presentation: SharedPresentation,
    compositor: GpuCompositor,
    width: u32,
    height: u32,
    configured: bool,
    /// Shared with the document so it can print the result line.
    pub stats: Rc<RefCell<PresentStats>>,
    /// `ROVES_PROBE_FRAME_TIMINGS=1` only: samples `cpu_render_present`, its
    /// phases and the raster cache-miss reasons.
    pub timings: Option<crate::frame_timings::SharedFrameTimings>,
}

impl GpuWindowRenderer {
    /// `None` until dom.js registered the page's device (op_blitz_present_device).
    pub fn new(presentation: SharedPresentation) -> Option<Self> {
        let device = presentation.borrow().device.clone()?;
        Some(Self {
            surface: None,
            presentation,
            compositor: GpuCompositor::new(device),
            width: 0,
            height: 0,
            configured: false,
            stats: Rc::default(),
            timings: None,
        })
    }

    pub fn raster_cache_disabled(&self) -> bool {
        self.compositor.raster_cache.disabled
    }

    fn configure(&mut self) {
        let Some(active) = &self.surface else { return };
        if self.width == 0 || self.height == 0 {
            return;
        }
        let device = self.compositor.device();
        let error = device.instance.surface_configure(
            active.surface,
            device.device,
            &wgpu_types::SurfaceConfiguration {
                usage: wgpu_types::TextureUsages::RENDER_ATTACHMENT,
                format: SURFACE_FORMAT,
                width: self.width,
                height: self.height,
                present_mode: wgpu_types::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu_types::CompositeAlphaMode::Auto,
                view_formats: Vec::new(),
            },
        );
        if let Some(error) = error {
            panic!("window surface configuration failed: {error}");
        }
        self.configured = true;
    }
}

impl GpuWindowRenderer {
    /// The body of `render` for an active, configured surface.
    fn render_frame<F: FnOnce(&mut Scene)>(&mut self, draw_fn: F) {
        let Some(active) = &self.surface else { return };
        let surface = active.surface;
        let mut scene = Scene::new();
        let paint = self.compositor.phase_start();
        draw_fn(&mut scene);
        self.compositor.phase_end(paint, Phase::ScenePaint);
        let split = self.compositor.phase_start();
        self.presentation.borrow_mut().prune();
        let presentation = self.presentation.borrow();
        let segments = split_scene(scene, |id| presentation.is_canvas(id));
        self.compositor.phase_end(split, Phase::Split);
        let global = self.compositor.device().instance.clone();
        let acquire = self.compositor.phase_start();
        let output = global.surface_get_current_texture(surface, None);
        self.compositor.phase_end(acquire, Phase::SurfaceAcquire);
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                eprintln!("ROVES_PROBE_PRESENT_ERROR surface texture: {error}");
                self.compositor.discard_frame();
                *self.stats.borrow_mut() = self.compositor.stats;
                return;
            },
        };
        let texture = match (output.status, output.texture) {
            (
                wgpu_types::SurfaceStatus::Good | wgpu_types::SurfaceStatus::Suboptimal,
                Some(texture),
            ) => texture,
            (status, texture) => {
                if let Some(texture) = texture {
                    global.texture_drop(texture);
                }
                self.compositor.discard_frame();
                *self.stats.borrow_mut() = self.compositor.stats;
                if matches!(
                    status,
                    wgpu_types::SurfaceStatus::Outdated | wgpu_types::SurfaceStatus::Lost
                ) {
                    drop(presentation);
                    self.configure();
                }
                return;
            },
        };
        let (target, error) = global.texture_create_view(
            texture,
            &wgpu_core::resource::TextureViewDescriptor::default(),
            None,
        );
        let errors_before = self.compositor.stats.errors;
        report(&mut self.compositor.stats, "surface view", error);
        self.compositor.compose(
            segments,
            &presentation.canvases,
            target,
            self.width,
            self.height,
        );
        global.texture_view_drop(target);
        let present = self.compositor.phase_start();
        let result = global.surface_present(surface);
        self.compositor.phase_end(present, Phase::Present);
        let presented = match result {
            Ok(wgpu_types::SurfaceStatus::Good | wgpu_types::SurfaceStatus::Suboptimal) => true,
            Ok(status) => {
                eprintln!("ROVES_PROBE_PRESENT_ERROR present status: {status:?}");
                false
            },
            Err(error) => {
                report(&mut self.compositor.stats, "present", Some(error));
                false
            },
        };
        global.texture_drop(texture);
        // Only frames presented without any error count (frames, layers,
        // segments, uploads); the others are skipped.
        if presented && self.compositor.stats.errors == errors_before {
            self.compositor.commit_frame();
        } else {
            self.compositor.discard_frame();
        }
        *self.stats.borrow_mut() = self.compositor.stats;
    }
}

impl RenderContext for GpuWindowRenderer {}

impl WindowRenderer for GpuWindowRenderer {
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
        let display = window
            .display_handle()
            .expect("window display handle")
            .as_raw();
        let handle = window.window_handle().expect("window handle").as_raw();
        // Drop the previous surface (if any) before creating the new one.
        self.surface = None;
        let instance = self.compositor.device().instance.clone();
        // SAFETY: `ActiveSurface` keeps the window and its `Drop` destroys the
        // surface before releasing it (suspend, resume or renderer drop).
        let surface = unsafe { instance.instance_create_surface(Some(display), handle, None) }
            .unwrap_or_else(|error| panic!("window surface creation failed: {error}"));
        self.surface = Some(ActiveSurface {
            instance,
            surface,
            _window: window,
        });
        self.configured = false;
        self.set_size(width, height);
        on_ready();
    }

    fn complete_resume(&mut self) -> bool {
        true
    }

    fn suspend(&mut self) {
        // `ActiveSurface::drop`: surface first, then the window handle.
        self.surface = None;
        self.configured = false;
    }

    fn is_active(&self) -> bool {
        self.surface.is_some()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if (width, height) != (self.width, self.height) || !self.configured {
            self.width = width;
            self.height = height;
            self.configure();
        }
    }

    fn render<F: FnOnce(&mut Scene)>(&mut self, draw_fn: F) {
        if self.surface.is_none() || !self.configured {
            return;
        }
        // Opt-in (ROVES_PROBE_FRAME_TIMINGS=1): CPU wall-clock time of this
        // call until it returns, plus its phases; not GPU time, not proof of
        // a visible frame. Without the variable no timestamp is taken.
        let start = self.timings.as_ref().map(|_| std::time::Instant::now());
        if start.is_some() {
            self.compositor
                .diagnostics
                .get_or_insert_with(Diagnostics::default)
                .phases = Default::default();
        }
        self.render_frame(draw_fn);
        if let (Some(timings), Some(start)) = (&self.timings, start) {
            let mut timings = timings.borrow_mut();
            timings.cpu_render_present.push(start.elapsed());
            if let Some(diagnostics) = &self.compositor.diagnostics {
                timings.record_render_phases(&diagnostics.phases);
                timings.raster_cache_misses = Some(diagnostics.misses.committed);
                timings.raster_cache_content_diff = Some(diagnostics.misses.committed_diff);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blitz_canvas_widget_splits_the_scene_in_paint_order() {
        self_check();
    }

    fn canvas_document() -> (blitz_html::HtmlDocument, NodeId) {
        let document = blitz_html::HtmlDocument::from_html(
            r#"<!doctype html><html><body><canvas id="c"></canvas></body></html>"#,
            blitz_dom::DocumentConfig::default(),
        );
        let canvas = crate::find_element_by_id(&document, "c").expect("canvas");
        (document, canvas)
    }

    fn has_widget(document: &blitz_dom::BaseDocument, node: NodeId) -> bool {
        document
            .get_node(node)
            .and_then(|node| node.element_data())
            .is_some_and(|element| element.custom_widget_data().is_some())
    }

    #[test]
    fn forgetting_a_collected_context_unregisters_its_entries() {
        let (mut document, canvas) = canvas_document();
        let presentation: SharedPresentation = Rc::default();
        let resource = attach_canvas_widget(&presentation, &mut document, canvas);
        assert_eq!(
            attach_canvas_widget(&presentation, &mut document, canvas),
            resource
        );
        assert!(presentation.borrow().is_canvas(resource));
        assert!(has_widget(&document, canvas));

        presentation.borrow_mut().forget(&mut document, resource);
        {
            let presentation = presentation.borrow();
            assert!(presentation.canvases.is_empty());
            assert!(presentation.widgets.is_empty());
            assert!(presentation.retired.borrow().is_empty());
        }
        assert!(!has_widget(&document, canvas));
        // A stale (already forgotten) or unknown resource is a no-op.
        presentation.borrow_mut().forget(&mut document, resource);
        presentation
            .borrow_mut()
            .forget(&mut document, ResourceId::new());
        // A later context on the same canvas gets a fresh placeholder.
        let again = attach_canvas_widget(&presentation, &mut document, canvas);
        assert_ne!(again, resource);
        assert!(has_widget(&document, canvas));
    }

    #[test]
    fn widgets_dropped_by_blitz_are_pruned() {
        let (mut document, canvas) = canvas_document();
        let presentation: SharedPresentation = Rc::default();
        let resource = attach_canvas_widget(&presentation, &mut document, canvas);
        // What Blitz does when the node is dropped (UnloadCustomWidget).
        document.remove_custom_widget(canvas);
        presentation.borrow_mut().prune();
        let presentation = presentation.borrow();
        assert!(!presentation.is_canvas(resource));
        assert!(presentation.widgets.is_empty());
    }

    fn raster_scenes(segments: Vec<Segment>) -> Vec<Scene> {
        segments
            .into_iter()
            .filter_map(|segment| match segment {
                Segment::Raster(scene) => Some(scene),
                Segment::Canvas(_) => None,
            })
            .collect()
    }

    /// Paints a Blitz document with text, an image-free overlay and a canvas
    /// placeholder, split like the window renderer does.
    fn painted_rasters(document: &mut blitz_html::HtmlDocument, canvas: ResourceId) -> Vec<Scene> {
        let mut scene = Scene::new();
        blitz_paint::paint_scene(&mut scene, document, 1.0, 240, 120, 0, 0);
        raster_scenes(split_scene(scene, |id| id == canvas))
    }

    #[test]
    fn unchanged_blitz_segments_are_reused_and_changes_upload_again() {
        use blitz_traits::shell::{ColorScheme, Viewport};
        let mut document = blitz_html::HtmlDocument::from_html(
            r#"<!doctype html><html><head><style>
            body { margin: 0 } canvas { width: 120px; height: 60px }
            #over { position: absolute; left: 0; top: 0; color: rgb(255, 0, 0) }
            </style></head><body><p>Before</p><canvas id="c"></canvas>
            <div id="over">Over the canvas</div></body></html>"#,
            blitz_dom::DocumentConfig::default(),
        );
        document.set_viewport(Viewport::new(240, 120, 1.0, ColorScheme::Light));
        let node = crate::find_element_by_id(&document, "c").expect("canvas");
        let presentation: SharedPresentation = Rc::default();
        let canvas = attach_canvas_widget(&presentation, &mut document, node);
        document.resolve(0.0);

        let first = painted_rasters(&mut document, canvas);
        assert_eq!(first.len(), 2, "raster/canvas/raster expected");
        assert!(
            first.iter().all(|scene| scene
                .commands
                .iter()
                .any(|command| matches!(command, RenderCommand::GlyphRun(_)))),
            "both segments should contain text (font Blob identity)"
        );
        let mut cache = RasterCache::default();
        for (index, scene) in first.iter().enumerate() {
            assert!(!cache.is_current(index, scene));
            cache.store(index, scene.clone());
        }
        // A second paint of the unchanged document reuses both segments.
        let second = painted_rasters(&mut document, canvas);
        assert!(
            second
                .iter()
                .enumerate()
                .all(|(index, scene)| cache.is_current(index, scene))
        );
        // Segment order matters: the first segment is not the second texture.
        assert!(!cache.is_current(1, &second[0]));
        // A changed command uploads only that segment.
        let mut changed = second[1].clone();
        let RenderCommand::GlyphRun(run) = changed
            .commands
            .iter_mut()
            .find(|command| matches!(command, RenderCommand::GlyphRun(_)))
            .expect("overlay text")
        else {
            unreachable!()
        };
        run.brush = Paint::Solid(peniko::Color::BLACK);
        assert!(cache.is_current(0, &second[0]));
        assert!(!cache.is_current(1, &changed));
        // A failed upload invalidates its entry; resize clears everything.
        cache.invalidate(0);
        assert!(!cache.is_current(0, &second[0]));
        cache.clear();
        assert!(!cache.is_current(1, &second[1]));
    }

    #[test]
    fn segments_with_external_or_mutable_paint_are_never_reused() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut resource = Scene::new();
        resource.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Paint::Resource(ImageBrush {
                image: ResourceId::new(),
                sampler: ImageSampler::default(),
            })
            .as_ref(),
            None,
            &rect,
        );
        let mut external = Scene::new();
        external.push_layer(
            peniko::Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &rect,
            Some(Arc::new(anyrender::Filter::single(FilterEffect::Image(
                anyrender::filters::ExternalImageSource {
                    image_id: 1,
                    transform: None,
                },
            )))),
            None,
        );
        external.pop_layer();
        let mut nan = Scene::new();
        nan.fill(
            peniko::Fill::NonZero,
            Affine::translate((f64::NAN, 0.0)),
            peniko::Color::WHITE,
            None,
            &rect,
        );
        let mut cache = RasterCache::default();
        for scene in [resource, external] {
            assert!(!reusable(&scene));
            cache.store(0, scene.clone());
            assert!(!cache.is_current(0, &scene));
        }
        // NaN never compares equal: always uploaded, never a false hit.
        cache.store(0, nan.clone());
        assert!(!cache.is_current(0, &nan));
    }

    #[test]
    fn a_disabled_raster_cache_never_reports_a_hit() {
        let mut solid = Scene::new();
        solid.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        assert!(reusable(&solid));
        // Control: the default cache reuses the same reusable segment.
        let mut enabled = RasterCache::new(false);
        enabled.store(0, solid.clone());
        assert!(enabled.is_current(0, &solid));
        // Disabled (ROVES_PROBE_DISABLE_RASTER_CACHE=1): nothing is stored and
        // `upload_segment` always takes the upload path.
        let mut disabled = RasterCache::new(true);
        for _ in 0..3 {
            assert!(!disabled.is_current(0, &solid));
            disabled.store(0, solid.clone());
            assert!(disabled.contents.is_empty());
        }
        // The default (and `Default`) stays enabled.
        assert!(!RasterCache::default().disabled);
    }

    #[test]
    fn raster_cache_reuses_only_bit_identical_nan_fill_paths() {
        /// A finite fill (every command is compared) followed by the fill
        /// under test.
        fn fill(
            rule: peniko::Fill,
            transform: Affine,
            color: peniko::Color,
            brush_transform: Option<Affine>,
            shape: &kurbo::BezPath,
        ) -> Scene {
            let mut scene = Scene::new();
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                peniko::Color::BLACK,
                None,
                &Rect::new(0.0, 0.0, 4.0, 4.0),
            );
            scene.fill(rule, transform, color, brush_transform, shape);
            scene
        }
        // MoveTo/LineTo with `value` in x and y (NaN like the measured
        // template fills), then a finite LineTo at `x`.
        let path = |value: f64, x: f64| {
            let mut path = kurbo::BezPath::new();
            path.move_to((value, 0.0));
            path.line_to((value, value));
            path.line_to((x, 1.0));
            path.close_path();
            path
        };
        let (nz, id, white) = (
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::WHITE,
        );
        let nan_path = path(f64::NAN, 2.0);
        let base = fill(nz, id, white, None, &nan_path);
        // Derived PartialEq misses (NaN != NaN); the cache still reuses it.
        assert_ne!(base, base.clone());
        assert!(raster_cache_equal(&base, &base.clone()));
        let mut cache = RasterCache::default();
        cache.store(0, base.clone());
        assert!(cache.is_current(0, &base.clone()));
        // A freshly recorded, bit-identical scene is reused too.
        let recorded_again = fill(nz, id, white, None, &path(f64::NAN, 2.0));
        assert!(cache.is_current(0, &recorded_again));

        // Every other change is still a miss.
        let other_nan = f64::from_bits(f64::NAN.to_bits() ^ 1);
        assert!(other_nan.is_nan());
        let black = peniko::Color::BLACK;
        let moved = Affine::translate((1.0, 0.0));
        let scaled = Some(Affine::scale(2.0));
        let changed = [
            // Different NaN payload.
            fill(nz, id, white, None, &path(other_nan, 2.0)),
            // Different finite point.
            fill(nz, id, white, None, &path(f64::NAN, 3.0)),
            // Brush.
            fill(nz, id, black, None, &nan_path),
            // Transform.
            fill(nz, moved, white, None, &nan_path),
            // Style (fill rule).
            fill(peniko::Fill::EvenOdd, id, white, None, &nan_path),
            // Brush transform.
            fill(nz, id, white, scaled, &nan_path),
        ];
        for scene in &changed {
            assert!(!raster_cache_equal(&base, scene));
            assert!(!cache.is_current(0, scene));
        }
        // A NaN transform is not relaxed, even with an identical NaN path.
        let nan_moved = Affine::translate((f64::NAN, 0.0));
        let nan_transform = fill(nz, nan_moved, white, None, &nan_path);
        assert!(!raster_cache_equal(&nan_transform, &nan_transform.clone()));
        // Tolerance and command count are compared too.
        let mut tolerant = base.clone();
        tolerant.tolerance = base.tolerance * 2.0;
        assert!(!raster_cache_equal(&base, &tolerant));
        let mut longer = base.clone();
        longer.fill(nz, id, white, None, &nan_path);
        assert!(!raster_cache_equal(&base, &longer));
        assert!(!raster_cache_equal(&longer, &base));
        // Only fills are relaxed: a bit-identical NaN stroke is still a miss.
        let mut stroke = Scene::new();
        stroke.stroke(&kurbo::Stroke::new(1.0), id, white, None, &nan_path);
        assert!(!raster_cache_equal(&stroke, &stroke.clone()));

        // Finite scenes keep their PartialEq behavior (including -0.0 == 0.0).
        let finite = fill(nz, id, white, None, &path(0.0, 2.0));
        assert!(raster_cache_equal(&finite, &finite.clone()));
        let negative_zero = fill(nz, id, white, None, &path(-0.0, 2.0));
        assert_eq!(finite, negative_zero);
        assert!(raster_cache_equal(&finite, &negative_zero));
        let finite_moved = fill(nz, id, white, None, &path(0.0, 3.0));
        assert!(!raster_cache_equal(&finite, &finite_moved));
        // Rule unchanged: a non-reusable segment is never a hit.
        let mut resource = Scene::new();
        resource.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Paint::Resource(ImageBrush {
                image: ResourceId::new(),
                sampler: ImageSampler::default(),
            })
            .as_ref(),
            None,
            &nan_path,
        );
        cache.store(0, resource.clone());
        assert!(!cache.is_current(0, &resource));
    }

    /// Mirrors the cache/tracker calls of `upload_segment` for one segment;
    /// returns the miss reason (`None` on a hit).
    fn upload(
        cache: &mut RasterCache,
        tracker: &mut MissTracker,
        index: usize,
        scene: &Scene,
        failed: bool,
    ) -> Option<crate::frame_timings::MissReason> {
        if cache.is_current(index, scene) {
            return None;
        }
        let reason = tracker.classify(cache, index, scene);
        tracker.miss(
            cache,
            index,
            scene,
            &mut crate::frame_timings::FramePhases::default(),
        );
        cache.invalidate(index);
        let cached = (!cache.disabled && reusable(scene)).then(|| scene.clone());
        let stored = cached.is_some() && !failed;
        if let Some(cached) = cached
            && !failed
        {
            cache.store(index, cached);
        }
        tracker.uploaded(index, stored, failed);
        Some(reason)
    }

    #[test]
    fn raster_cache_misses_are_classified_without_changing_the_cache() {
        use crate::frame_timings::MissReason::*;
        let solid = |color: peniko::Color| {
            let mut scene = Scene::new();
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &Rect::new(0.0, 0.0, 10.0, 10.0),
            );
            scene
        };
        let white = solid(peniko::Color::WHITE);
        let black = solid(peniko::Color::BLACK);
        let mut resource = Scene::new();
        resource.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Paint::Resource(ImageBrush {
                image: ResourceId::new(),
                sampler: ImageSampler::default(),
            })
            .as_ref(),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mut cache = RasterCache::default();
        let mut tracker = MissTracker::default();
        let step =
            |cache: &mut RasterCache, tracker: &mut MissTracker, scene: &Scene, failed: bool| {
                let reason = upload(cache, tracker, 0, scene, failed);
                tracker.commit_frame();
                reason
            };
        assert_eq!(
            step(&mut cache, &mut tracker, &white, false),
            Some(FirstUse)
        );
        assert_eq!(step(&mut cache, &mut tracker, &white, false), None);
        assert_eq!(
            step(&mut cache, &mut tracker, &black, false),
            Some(ContentChanged)
        );
        assert_eq!(
            step(&mut cache, &mut tracker, &resource, false),
            Some(NotReusable)
        );
        assert_eq!(
            step(&mut cache, &mut tracker, &white, false),
            Some(PreviousNotCached)
        );
        assert_eq!(
            step(&mut cache, &mut tracker, &black, true),
            Some(ContentChanged)
        );
        assert_eq!(
            step(&mut cache, &mut tracker, &black, false),
            Some(AfterUploadError)
        );
        // Resize (`GpuCompositor::raster`) and discard clear the cache.
        tracker.cleared(EmptyReason::Resize);
        cache.clear();
        assert_eq!(
            step(&mut cache, &mut tracker, &black, false),
            Some(AfterResize)
        );
        tracker.discard_frame();
        cache.clear();
        assert_eq!(
            step(&mut cache, &mut tracker, &black, false),
            Some(AfterDiscard)
        );
        // A texture index never used after a resize is a first use.
        assert_eq!(
            upload(&mut cache, &mut tracker, 3, &black, false),
            Some(FirstUse)
        );
        assert_eq!(tracker.frame.total(), 1);

        // A discarded frame's misses are not committed.
        tracker.discard_frame();
        assert_eq!(tracker.frame.total(), 0);
        let committed = tracker.committed;
        assert_eq!(committed.total(), 8);
        assert_eq!(committed.get(ContentChanged), 2);
        // Both content_changed misses (white -> black) differ in a Fill.
        assert_eq!(
            tracker.committed_diff.total(),
            committed.get(ContentChanged)
        );
        assert_eq!(
            tracker
                .committed_diff
                .first_command(crate::frame_timings::CommandKind::Fill),
            2
        );

        // A disabled cache is reported as such and still stores nothing.
        let mut disabled = RasterCache::new(true);
        let mut tracker = MissTracker::default();
        for _ in 0..2 {
            assert_eq!(
                upload(&mut disabled, &mut tracker, 0, &white, false),
                Some(CacheDisabled)
            );
        }
        assert!(disabled.contents.is_empty());
    }

    #[test]
    fn content_diff_reports_the_first_differing_command_kind() {
        use crate::frame_timings::{
            AreaBucket, BoundsOrigin, CommandKind, ContentDiff, ElementBucket, FillChanges,
            FillDiff, FirstDiff, NonFiniteSlots, PathBits, PathClass,
        };
        // MoveTo, three LineTo and ClosePath, all finite.
        const RECT: PathClass = PathClass {
            elements: ElementBucket::UpTo16,
            origin: BoundsOrigin::Finite,
            non_finite: NonFiniteSlots::NONE,
        };
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let solid = |color: peniko::Color| {
            let mut scene = Scene::new();
            scene.fill(peniko::Fill::NonZero, Affine::IDENTITY, color, None, &rect);
            scene
        };
        let white = solid(peniko::Color::WHITE);
        let black = solid(peniko::Color::BLACK);
        let diff = |first, length_changed, kind_changed| ContentDiff {
            first,
            length_changed,
            kind_changed,
            fill: None,
        };
        let changes = |changes| {
            Some(FillDiff {
                area: AreaBucket::Under1K,
                current: RECT,
                changes: Some(changes),
                cached: Some(RECT),
            })
        };

        // Fill: same length, same kind, only the brush differs (10x10 area).
        assert_eq!(
            content_diff(&white, &black),
            ContentDiff {
                fill: changes(FillChanges {
                    brush: true,
                    ..FillChanges::default()
                }),
                ..diff(FirstDiff::Command(CommandKind::Fill), false, false)
            }
        );
        // A fill moved by its transform: transform and bounds, not the path.
        let mut moved = Scene::new();
        moved.fill(
            peniko::Fill::NonZero,
            Affine::translate((5.0, 0.0)),
            peniko::Color::WHITE,
            None,
            &rect,
        );
        assert_eq!(
            content_diff(&white, &moved).fill,
            changes(FillChanges {
                transform: true,
                bounds: true,
                ..FillChanges::default()
            })
        );
        // A resized full-viewport fill: path and bounds, area bucket of the
        // current (large) fill.
        let mut large = Scene::new();
        large.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &Rect::new(0.0, 0.0, 1920.0, 1080.0),
        );
        assert_eq!(
            content_diff(&white, &large).fill,
            Some(FillDiff {
                area: AreaBucket::AtLeast1M,
                current: RECT,
                changes: Some(FillChanges {
                    shape: true,
                    bounds: true,
                    path_bits: PathBits::PointBitsDiffer,
                    ..FillChanges::default()
                }),
                cached: Some(RECT),
            })
        );
        // Only the fill rule or only the brush transform: `other`.
        for (rule, brush_transform) in [
            (peniko::Fill::EvenOdd, None),
            (peniko::Fill::NonZero, Some(Affine::scale(2.0))),
        ] {
            let mut rule_changed = Scene::new();
            rule_changed.fill(
                rule,
                Affine::IDENTITY,
                peniko::Color::WHITE,
                brush_transform,
                &rect,
            );
            assert_eq!(
                content_diff(&white, &rule_changed).fill,
                changes(FillChanges {
                    other: true,
                    ..FillChanges::default()
                })
            );
        }
        // A fill where a layer was: area only, no field comparison.
        let mut layer_first = Scene::new();
        layer_first.push_layer(
            peniko::Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &rect,
            None,
            None,
        );
        assert_eq!(
            content_diff(&layer_first, &white),
            ContentDiff {
                fill: Some(FillDiff {
                    area: AreaBucket::Under1K,
                    current: RECT,
                    changes: None,
                    cached: None,
                }),
                ..diff(FirstDiff::Command(CommandKind::Fill), false, true)
            }
        );
        // Equal prefix, one command appended.
        let mut longer = white.clone();
        longer.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &rect,
        );
        assert_eq!(
            content_diff(&white, &longer),
            diff(FirstDiff::LengthOnly, true, false)
        );
        // A layer where the fill was: the current kind is counted.
        let mut layered = Scene::new();
        layered.push_layer(
            peniko::Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &rect,
            None,
            None,
        );
        layered.pop_layer();
        assert_eq!(
            content_diff(&white, &layered),
            diff(FirstDiff::Command(CommandKind::PushLayer), true, true)
        );
        // Equal commands, different tolerance.
        let mut tolerant = white.clone();
        tolerant.tolerance = white.tolerance * 2.0;
        assert_ne!(tolerant, white);
        assert_eq!(
            content_diff(&white, &tolerant),
            diff(FirstDiff::NoCommandDiff, false, false)
        );

        // GlyphRun: a Blitz text segment whose glyph brush changes.
        let mut document = blitz_html::HtmlDocument::from_html(
            r#"<!doctype html><html><body><p>Text</p></body></html>"#,
            blitz_dom::DocumentConfig::default(),
        );
        document.set_viewport(blitz_traits::shell::Viewport::new(
            120,
            60,
            1.0,
            blitz_traits::shell::ColorScheme::Light,
        ));
        document.resolve(0.0);
        let mut text = Scene::new();
        blitz_paint::paint_scene(&mut text, &mut document, 1.0, 120, 60, 0, 0);
        let mut changed = text.clone();
        let RenderCommand::GlyphRun(run) = changed
            .commands
            .iter_mut()
            .find(|command| matches!(command, RenderCommand::GlyphRun(_)))
            .expect("text glyph run")
        else {
            unreachable!()
        };
        run.brush = Paint::Solid(peniko::Color::from_rgb8(1, 2, 3));
        assert_eq!(
            content_diff(&text, &changed),
            diff(FirstDiff::Command(CommandKind::GlyphRun), false, false)
        );
    }

    /// A single recorded fill of `shape` under `transform`.
    fn recorded_fill(transform: Affine, shape: &kurbo::BezPath) -> FillCommand {
        let mut scene = Scene::new();
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            peniko::Color::WHITE,
            None,
            shape,
        );
        match scene.commands.pop() {
            Some(RenderCommand::Fill(fill)) => fill,
            _ => unreachable!("one fill was recorded"),
        }
    }

    #[test]
    fn path_class_finds_the_first_non_finite_stage() {
        use crate::frame_timings::{BoundsOrigin, ElementBucket};
        let class = |transform, path: &kurbo::BezPath| {
            let class = path_class(&recorded_fill(transform, path));
            (class.elements, class.origin)
        };
        let rect = |size: f64| Rect::new(0.0, 0.0, size, size).to_path(0.1);

        assert_eq!(
            class(Affine::IDENTITY, &rect(10.0)),
            (ElementBucket::UpTo16, BoundsOrigin::Finite)
        );
        // An empty path has a finite (default) bounding box.
        assert_eq!(
            class(Affine::IDENTITY, &kurbo::BezPath::new()),
            (ElementBucket::Empty, BoundsOrigin::Finite)
        );
        // A NaN point in the path itself.
        let mut nan = kurbo::BezPath::new();
        nan.move_to((0.0, 0.0));
        nan.line_to((f64::NAN, 1.0));
        nan.line_to((1.0, 1.0));
        assert_eq!(
            class(Affine::IDENTITY, &nan),
            (ElementBucket::UpTo4, BoundsOrigin::PathNonFinite)
        );
        // An infinite control point of a cubic is also a path point.
        let mut curve = kurbo::BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.curve_to((1.0, 1.0), (f64::INFINITY, 0.0), (2.0, 2.0));
        assert_eq!(
            class(Affine::IDENTITY, &curve).1,
            BoundsOrigin::PathNonFinite
        );
        // A finite path under a non-finite transform.
        assert_eq!(
            class(
                Affine::new([f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0]),
                &rect(10.0)
            )
            .1,
            BoundsOrigin::TransformNonFinite
        );
        // Finite path and transform whose transformed box overflows.
        assert_eq!(
            class(Affine::scale(10.0), &rect(1e308)).1,
            BoundsOrigin::TransformedBoxNonFinite
        );
        // Finite transformed box whose area overflows.
        assert_eq!(
            class(Affine::IDENTITY, &rect(1e200)).1,
            BoundsOrigin::AreaNonFinite
        );
        // Element count buckets.
        let mut long = kurbo::BezPath::new();
        long.move_to((0.0, 0.0));
        for x in 0..300 {
            long.line_to((f64::from(x), 1.0));
        }
        assert_eq!(class(Affine::IDENTITY, &long).0, ElementBucket::Over256);
    }

    #[test]
    fn path_class_records_non_finite_slots() {
        use crate::frame_timings::NonFiniteClass::{self, *};
        let slots = |path: &kurbo::BezPath| {
            let slots = path_class(&recorded_fill(Affine::IDENTITY, path)).non_finite;
            NonFiniteClass::ALL
                .into_iter()
                .filter(|class| slots.contains(*class))
                .collect::<Vec<_>>()
        };
        assert!(slots(&Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1)).is_empty());
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, f64::NAN));
        path.quad_to((f64::INFINITY, 0.0), (1.0, 1.0));
        path.curve_to((0.0, 0.0), (0.0, 0.0), (f64::NEG_INFINITY, 1.0));
        path.close_path();
        assert_eq!(
            slots(&path),
            [MoveTo, QuadCtrl, CurveEnd, Nan, PosInf, NegInf, X, Y]
        );
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.curve_to((0.0, 0.0), (f64::NAN, f64::NAN), (1.0, 1.0));
        path.quad_to((0.0, 0.0), (0.0, f64::INFINITY));
        assert_eq!(slots(&path), [QuadEnd, CurveCtrl2, Nan, PosInf, X, Y]);
    }

    #[test]
    fn path_bits_compares_coordinates_by_bits() {
        use crate::frame_timings::PathBits;
        let path = |x: f64, close: bool| {
            let mut path = kurbo::BezPath::new();
            path.move_to((0.0, 0.0));
            path.line_to((x, 1.0));
            path.quad_to((1.0, 1.0), (2.0, 2.0));
            if close {
                path.close_path();
            } else {
                path.line_to((0.0, 0.0));
            }
            path
        };
        let nan = path(f64::NAN, true);
        // PartialEq treats NaN != NaN; the bits are equal.
        assert_ne!(nan, nan.clone());
        assert_eq!(path_bits(&nan, &nan.clone()), PathBits::SameBitwise);
        assert_eq!(
            path_bits(&path(1.0, true), &path(1.0, true)),
            PathBits::SameBitwise
        );
        // A different NaN payload and -0.0 vs 0.0 differ by bits.
        let other_nan = f64::from_bits(f64::NAN.to_bits() ^ 1);
        assert!(other_nan.is_nan());
        assert_eq!(
            path_bits(&nan, &path(other_nan, true)),
            PathBits::PointBitsDiffer
        );
        assert_eq!(path(0.0, true), path(-0.0, true));
        assert_eq!(
            path_bits(&path(0.0, true), &path(-0.0, true)),
            PathBits::PointBitsDiffer
        );
        // Same count, ClosePath vs LineTo; verbs win over points.
        assert_eq!(
            path_bits(&path(1.0, true), &path(2.0, false)),
            PathBits::VerbMismatch
        );
        let mut longer = path(1.0, true);
        longer.line_to((3.0, 3.0));
        assert_eq!(
            path_bits(&path(1.0, true), &longer),
            PathBits::CountMismatch
        );

        // Through content_diff: a bit-identical NaN fill is a shape change
        // by PartialEq only.
        let scene = |shape: &kurbo::BezPath| {
            let mut scene = Scene::new();
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                peniko::Color::WHITE,
                None,
                shape,
            );
            scene
        };
        let changes = content_diff(&scene(&nan), &scene(&nan))
            .fill
            .and_then(|fill| fill.changes)
            .expect("fill vs fill");
        assert!(changes.shape && !changes.transform && !changes.brush);
        assert_eq!(changes.path_bits, PathBits::SameBitwise);
    }

    #[test]
    fn content_diff_counts_follow_commit_and_discard() {
        use crate::frame_timings::MissReason::ContentChanged;
        let solid = |color: peniko::Color| {
            let mut scene = Scene::new();
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &Rect::new(0.0, 0.0, 10.0, 10.0),
            );
            scene
        };
        let (white, black) = (solid(peniko::Color::WHITE), solid(peniko::Color::BLACK));
        let mut cache = RasterCache::default();
        let mut tracker = MissTracker::default();
        let mut phases = crate::frame_timings::FramePhases::default();
        upload(&mut cache, &mut tracker, 0, &white, false);
        tracker.commit_frame();
        // Diffing is timed only on a content_changed miss.
        tracker.miss(&cache, 0, &black, &mut phases);
        assert!(phases.get(Phase::CacheDiff).is_some());
        assert!(
            crate::frame_timings::FramePhases::default()
                .get(Phase::CacheDiff)
                .is_none()
        );
        // A discarded frame keeps neither the miss nor its diff.
        tracker.discard_frame();
        assert_eq!(tracker.committed_diff.total(), 0);
        assert_eq!(tracker.committed.get(ContentChanged), 0);
        assert_eq!(
            *tracker.committed_diff.fill(),
            crate::frame_timings::FillCounts::default()
        );
        // A presented one commits both together.
        cache.clear();
        upload(&mut cache, &mut tracker, 0, &white, false);
        upload(&mut cache, &mut tracker, 0, &black, false);
        tracker.commit_frame();
        assert_eq!(tracker.committed.get(ContentChanged), 1);
        assert_eq!(tracker.committed_diff.total(), 1);
        assert_eq!(tracker.frame_diff.total(), 0);
        // The fill classification is committed with it (white -> black).
        let fill = tracker.committed_diff.fill();
        assert_eq!(fill.field("compared"), 1);
        assert_eq!(fill.field("brush_only"), 1);
        assert_eq!(fill.area(crate::frame_timings::AreaBucket::Under1K), 1);
        // Both path classes are committed with it.
        use crate::frame_timings::BoundsOrigin::Finite;
        assert_eq!(fill.current_path().origin(Finite), 1);
        assert_eq!(fill.cached_path().origin(Finite), 1);
        assert_eq!(*tracker.frame_diff.fill(), Default::default());

        // A discarded non-finite path is not committed.
        let mut nan = kurbo::BezPath::new();
        nan.move_to((0.0, 0.0));
        nan.line_to((f64::NAN, 1.0));
        let mut broken = Scene::new();
        broken.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &nan,
        );
        let before = *tracker.committed_diff.fill();
        upload(&mut cache, &mut tracker, 0, &broken, false);
        assert_eq!(
            tracker
                .frame_diff
                .fill()
                .current_path()
                .origin(crate::frame_timings::BoundsOrigin::PathNonFinite),
            1
        );
        tracker.discard_frame();
        assert_eq!(*tracker.committed_diff.fill(), before);
    }

    #[test]
    fn split_reopens_layers_and_keeps_other_resources_in_the_raster() {
        let canvas = ResourceId::new();
        let other = ResourceId::new();
        let mut scene = Scene::new();
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let resource = |image| {
            Paint::Resource(ImageBrush {
                image,
                sampler: ImageSampler::default(),
            })
        };
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &rect,
        );
        scene.push_layer(
            peniko::Fill::NonZero,
            peniko::Mix::Normal,
            0.5,
            Affine::IDENTITY,
            &rect,
            None,
            None,
        );
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            resource(other).as_ref(),
            None,
            &rect,
        );
        scene.fill(
            peniko::Fill::NonZero,
            Affine::translate((5.0, 7.0)),
            resource(canvas).as_ref(),
            None,
            &rect,
        );
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &rect,
        );
        scene.pop_layer();
        let segments = split_scene(scene, |id| id == canvas);
        assert_eq!(segments.len(), 3);
        let Segment::Raster(first) = &segments[0] else {
            panic!("first segment should be raster")
        };
        // White fill, open layer, the non-canvas resource, then the cut's pop.
        assert_eq!(first.commands.len(), 4);
        assert!(matches!(first.commands[3], RenderCommand::PopLayer));
        let Segment::Canvas(draw) = &segments[1] else {
            panic!("second segment should be the canvas")
        };
        assert_eq!(draw.resource, canvas);
        assert_eq!(draw.alpha, 0.5);
        assert!(draw.clipped);
        assert_eq!(draw.quad[0], Point::new(5.0, 7.0));
        assert_eq!(draw.quad[3], Point::new(15.0, 17.0));
        let Segment::Raster(last) = &segments[2] else {
            panic!("last segment should be raster")
        };
        // The layer is reopened, the black fill drawn, then the original pop.
        assert!(matches!(last.commands[0], RenderCommand::PushLayer(_)));
        assert_eq!(last.commands.len(), 3);
    }
}
