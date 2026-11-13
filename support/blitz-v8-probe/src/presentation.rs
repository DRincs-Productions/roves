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
/// value makes equality fail, which only costs an upload. Excluded, because
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

/// The scene last uploaded, without error, into each raster texture (by
/// segment index), so an equal segment in the same position reuses it.
/// Cleared on resize (textures recreated) and whenever a frame is discarded;
/// an entry is dropped before each upload and stored again only if the
/// upload reported no error.
#[derive(Default)]
struct RasterCache {
    contents: Vec<Option<Scene>>,
}

impl RasterCache {
    fn is_current(&self, index: usize, scene: &Scene) -> bool {
        reusable(scene) && self.contents.get(index).and_then(Option::as_ref) == Some(scene)
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
        if self.contents.len() <= index {
            self.contents.resize_with(index + 1, || None);
        }
        self.contents[index] = reusable(&scene).then_some(scene);
    }

    fn clear(&mut self) {
        self.contents.clear();
    }
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
            raster_cache: RasterCache::default(),
            raster_size: (0, 0),
            vello: VelloCpuImageRenderer::new(1, 1),
            pixels: Vec::new(),
            stats: PresentStats::default(),
            frame: PresentStats::default(),
        }
    }

    pub fn device(&self) -> &PresentDevice {
        &self.device
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
        if self.raster_cache.is_current(index, &scene) {
            self.frame.raster_cache_hits += 1;
            return view;
        }
        self.raster_cache.invalidate(index);
        // Kept only for comparison with the next frame's segment.
        let cached = reusable(&scene).then(|| scene.clone());
        self.pixels.clear();
        self.pixels.resize(width as usize * height as usize * 4, 0);
        let pixels = &mut self.pixels;
        self.vello.render(
            |painter| painter.append_scene(scene, Affine::IDENTITY),
            pixels,
        );
        self.vello.reset();
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
        report(&mut self.stats, "segment upload", error.err());
        // A failed texture creation or write leaves unknown contents: the
        // entry stays invalid and the next frame uploads again.
        if let Some(cached) = cached
            && self.stats.errors == errors_before
        {
            self.raster_cache.store(index, cached);
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
        self.frame.max_layers = layers.len();
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
    }

    /// The frame was not presented (or had an error): count it as skipped.
    /// Its uploads may not have reached the textures (e.g. a failed submit),
    /// so no segment is reused from it.
    fn discard_frame(&mut self) {
        self.raster_cache.clear();
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
        })
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
        let Some(active) = &self.surface else { return };
        if !self.configured {
            return;
        }
        let surface = active.surface;
        let mut scene = Scene::new();
        draw_fn(&mut scene);
        self.presentation.borrow_mut().prune();
        let presentation = self.presentation.borrow();
        let segments = split_scene(scene, |id| presentation.is_canvas(id));
        let global = self.compositor.device().instance.clone();
        let output = match global.surface_get_current_texture(surface, None) {
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
        let presented = match global.surface_present(surface) {
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
