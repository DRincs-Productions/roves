//! Shared part of the shared-device proof: adopting the game's wgpu-core ids
//! in wgpu, the Netrender scene and the pixel assertions. Two producers feed it:
//!
//! - `src/main.rs`: repeats deno_webgpu 0.227.0's wgpu-core calls in Rust;
//! - `src/bin/deno_js.rs` (feature `deno`): a real deno_core 0.413 `JsRuntime`
//!   with deno_webgpu 0.227.0, where unprivileged JS draws the canvas texture.

use std::sync::Arc;

use netrender::{
    Compositor, ExternalTextureComposite, ExternalTexturePlacement, NetrenderOptions,
    PresentedFrame, Scene, SourceAlpha, WgpuHandles, create_netrender_instance,
};
use wgpu_core::global::Global;
use wgpu_core::id::{AdapterId, DeviceId, QueueId, TextureId};

pub const DIM: u32 = 64;
pub const CANVAS: u32 = 16;
pub const CANVAS_FORMAT: wgpu_types::TextureFormat = wgpu_types::TextureFormat::Rgba8Unorm;
pub const CANVAS_SHADER: &str = include_str!("canvas.wgsl");

/// How the canvas texture was created (what wgpu's `Texture` accessors report).
#[derive(Clone, Debug)]
pub struct CanvasDesc {
    pub size: wgpu_types::Extent3d,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub dimension: wgpu_types::TextureDimension,
    pub format: wgpu_types::TextureFormat,
    pub usage: wgpu_types::TextureUsages,
}

/// The embedder's own ids (from `Global::*_clone_id`) for the game's adapter,
/// device, queue and canvas texture, plus what the game device was created with.
pub struct EmbedderIds {
    pub global: Arc<Global>,
    pub adapter: AdapterId,
    pub device: DeviceId,
    pub queue: QueueId,
    pub texture: TextureId,
    pub desc: CanvasDesc,
    pub game_features: wgpu_types::Features,
    pub game_limits: wgpu_types::Limits,
}

impl EmbedderIds {
    /// Registers the embedder's ids for objects the game (Deno) owns. The game
    /// keeps its own ids and may drop them at any time afterwards.
    pub fn clone_from_game(
        global: &Arc<Global>,
        adapter: AdapterId,
        device: DeviceId,
        queue: QueueId,
        texture: TextureId,
        desc: CanvasDesc,
    ) -> Self {
        assert!(
            !desc.usage.contains(wgpu_types::TextureUsages::COPY_SRC),
            "the canvas must not be copyable: any copy would then be a validation error"
        );
        Self {
            global: global.clone(),
            adapter: global.adapter_clone_id(adapter),
            device: global.device_clone_id(device),
            queue: global.queue_clone_id(queue),
            texture: global.texture_clone_id(texture),
            desc,
            game_features: global.device_features(device),
            game_limits: global.device_limits(device),
        }
    }

    /// Adopts the ids in wgpu (upstream/wgpu-29-core-interop.patch). Netrender
    /// gets the game device as-is: features and limits are asserted equal.
    pub fn adopt(self) -> (WgpuHandles, wgpu::Texture) {
        // SAFETY: same wgpu-core build and backends; every id is live, belongs
        // to `global` and is owned exclusively by the wgpu object adopting it
        // (`clone_from_game`); the texture was created on `device` with `desc`.
        let (handles, canvas) = unsafe {
            let handles = adopt_device(&self.global, self.adapter, self.device, self.queue);
            let canvas = adopt_texture(&handles, self.texture, &self.desc);
            (handles, canvas)
        };
        assert_eq!(
            handles.device.features(),
            self.game_features,
            "same feature set as the game device"
        );
        assert_eq!(
            handles.device.limits(),
            self.game_limits,
            "same limits as the game device"
        );
        (handles, canvas)
    }
}

/// Wraps the game's `Global` and adopts embedder-owned adapter, device and
/// queue ids (from `Global::*_clone_id`) as wgpu objects. Shared by both
/// producers of this proof and by the Blitz probe's opt-in Netrender window
/// renderer (`support/blitz-v8-probe/netrender-window`). The adopted device
/// gets its own error sink: handlers installed on it never replace the game's.
///
/// # Safety
///
/// Same wgpu-core build and backends as `global`; every id is live, belongs to
/// `global`, the queue belongs to the device and each id is owned exclusively
/// by the caller (the returned objects drop them).
pub unsafe fn adopt_device(
    global: &Arc<Global>,
    adapter: AdapterId,
    device: DeviceId,
    queue: QueueId,
) -> WgpuHandles {
    unsafe {
        let instance = wgpu::Instance::from_core_global(global.clone());
        let adapter = instance.adapter_from_core(adapter);
        let (device, queue) = instance.device_from_core(device, queue);
        WgpuHandles {
            instance,
            adapter,
            device,
            queue,
        }
    }
}

/// Adopts an embedder-owned texture id created on `handles.device` with `desc`.
///
/// # Safety
///
/// `texture` is a live id of the adopted device's `Global`, created on that
/// device as `desc` describes and owned exclusively by the caller.
pub unsafe fn adopt_texture(
    handles: &WgpuHandles,
    texture: TextureId,
    desc: &CanvasDesc,
) -> wgpu::Texture {
    let desc = wgpu::TextureDescriptor {
        label: None,
        size: desc.size,
        mip_level_count: desc.mip_level_count,
        sample_count: desc.sample_count,
        dimension: desc.dimension,
        format: desc.format,
        usage: desc.usage,
        view_formats: &[],
    };
    unsafe {
        handles
            .instance
            .texture_from_core(&handles.device, texture, &desc)
    }
}

/// Registry entries of the shared `Global` (one per id, not per object).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubIds {
    pub adapters: usize,
    pub devices: usize,
    pub queues: usize,
    pub textures: usize,
}

pub fn hub_ids(global: &Global) -> HubIds {
    let hub = global.generate_report().hub;
    HubIds {
        adapters: hub.adapters.num_allocated,
        devices: hub.devices.num_allocated,
        queues: hub.queues.num_allocated,
        textures: hub.textures.num_allocated,
    }
}

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

fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * DIM + x) * 4) as usize;
    [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
}

/// Renders the fixture scene with Netrender on the adopted device, waits for
/// the queue to drain, drops every wgpu object (the embedder's ids) and returns
/// the number of failed pixel assertions.
///
/// Scene: op 0 (before the canvas) opaque red background; the canvas at
/// `[16, 16, 48, 48]`, scene-op boundary 1; op 1 (after the canvas) 50% white
/// over the right part of the canvas. Only the final master (the fixture's
/// output, never the canvas) is read back. Asserts that creating and running
/// Netrender registers no further device or queue in the shared `Global`.
pub fn render_and_check(global: &Global, handles: WgpuHandles, canvas: wgpu::Texture) -> usize {
    let device = handles.device.clone();
    let ids_before = hub_ids(global);
    println!("adapter: {:?}", device.adapter_info());
    let canvas_view = canvas.create_view(&wgpu::TextureViewDescriptor::default());
    let renderer = create_netrender_instance(
        handles,
        NetrenderOptions {
            tile_cache_size: Some(32),
            enable_vello: true,
            ..Default::default()
        },
    )
    .expect("create_netrender_instance on the game device");
    // Phase marker for CI logs: Vello's compute pipelines exist; the next
    // phase submits Vello's compute frame and composites the canvas.
    println!("netrender: renderer created on the game device");

    let mut scene = Scene::new(DIM, DIM);
    scene.push_rect(0.0, 0.0, DIM as f32, DIM as f32, [1.0, 0.0, 0.0, 1.0]);
    scene.push_rect(32.0, 8.0, 56.0, 56.0, [1.0, 1.0, 1.0, 0.5]);
    let composites = [ExternalTextureComposite::new(
        &canvas_view,
        ExternalTexturePlacement::new([16.0, 16.0, 48.0, 48.0])
            .with_alpha(SourceAlpha::Premultiplied),
    )
    .with_scene_op_boundary(1)];
    let mut compositor = CaptureMaster::default();
    renderer.render_with_compositor_and_external_textures(
        &scene,
        wgpu::TextureFormat::Rgba8Unorm,
        &mut compositor,
        netrender::peniko::Color::new([0.0, 0.0, 0.0, 0.0]),
        &composites,
    );
    let ids_during = hub_ids(global);
    println!("hub ids before Netrender {ids_before:?}, after its frame {ids_during:?}");
    assert_eq!(
        (ids_during.devices, ids_during.queues),
        (ids_before.devices, ids_before.queues),
        "Netrender must not create a second device or queue"
    );
    drop(composites);
    drop(canvas_view);
    drop(canvas);

    let master = compositor.master.expect("captured master");
    // Fixture-only readback of the final output (the canvas is never read).
    let bytes = renderer.wgpu_device.read_rgba8_texture(&master, DIM, DIM);
    drop(master);
    // wgpu-core 29's Queue Drop waits only a few seconds for the last
    // submission: drain it before the last owner of the queue goes away.
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("drain the shared queue");
    drop(renderer);
    drop(device);

    let checks: [(&str, u32, u32, [u8; 4]); 6] = [
        ("before layer only", 4, 4, [255, 0, 0, 255]),
        (
            "after layer over before layer",
            50,
            10,
            [255, 128, 128, 255],
        ),
        ("opaque canvas top half", 20, 20, [0, 255, 0, 255]),
        ("50% canvas over before layer", 20, 40, [128, 0, 128, 255]),
        (
            "after layer over opaque canvas",
            40,
            20,
            [128, 255, 128, 255],
        ),
        (
            "after layer over 50% canvas over before layer",
            40,
            40,
            [192, 128, 192, 255],
        ),
    ];
    let mut failed = 0;
    for (name, x, y, expected) in checks {
        let actual = pixel(&bytes, x, y);
        let diff = (0..4)
            .map(|i| (actual[i] as i16 - expected[i] as i16).unsigned_abs())
            .max()
            .unwrap();
        let ok = diff <= 2;
        failed += usize::from(!ok);
        println!(
            "{} {name} ({x},{y}): actual {actual:?} expected {expected:?}",
            if ok { "PASS" } else { "FAIL" }
        );
    }
    failed
}
