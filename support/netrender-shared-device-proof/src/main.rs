//! Opt-in proof that Netrender can composite a WebGPU canvas texture on the
//! device the game created through deno_webgpu: one wgpu-core `Global`, one
//! device, one queue, no copy or readback of the canvas.
//!
//! 1. "Deno side": the exact wgpu-core calls deno_webgpu 0.227.0 makes for
//!    `navigator.gpu.requestAdapter()` / `adapter.requestDevice()` (same
//!    `Global::new` descriptor, WebGPU default limits, no features), then a
//!    canvas-like texture (`RENDER_ATTACHMENT | TEXTURE_BINDING`, no
//!    `COPY_SRC`) drawn by a render pass, as Pixi does, and submitted.
//!    In the Blitz probe these ids are `GPUDevice.instance/id/queue/adapter`
//!    and `GPUTexture.id`, which `src/presentation.rs` already reads.
//! 2. Bridge: `Global::*_clone_id` gives the embedder its own ids for the same
//!    `Arc`s; `wgpu::Instance::from_core_global` and `*_from_core` adopt them
//!    (upstream/wgpu-29-core-interop.patch). The Deno-side ids are then dropped
//!    (the page's GC) before Netrender renders: lifetimes are independent.
//! 3. Netrender d2f13ca5 renders (shared with `src/bin/deno_js.rs`, src/lib.rs) a scene with a layer before the canvas, the
//!    canvas at a scene-op boundary and a translucent layer after it; the final
//!    master texture (only the fixture's output) is read back for assertions.
//!
//! Exit code 0 means every assertion held; ordering defects are reported as
//! failed assertions with the measured pixels.

use std::borrow::Cow;
use std::sync::Arc;

use roves_netrender_shared_device_proof::{
    CANVAS, CANVAS_FORMAT, CANVAS_SHADER, CanvasDesc, EmbedderIds, hub_ids, render_and_check,
};
use wgpu_core::global::Global;
use wgpu_core::id::{AdapterId, DeviceId, QueueId, TextureId};

/// What the page owns after `requestDevice()` and one canvas frame.
struct DenoSide {
    global: Arc<Global>,
    adapter: AdapterId,
    device: DeviceId,
    queue: QueueId,
    texture: TextureId,
    texture_desc: wgpu_core::resource::TextureDescriptor<'static>,
}

fn check<E: std::fmt::Display>(what: &str, error: Option<E>) {
    if let Some(error) = error {
        panic!("{what}: {error}");
    }
}

/// deno_webgpu 0.227.0 `get_or_init_instance` + `request_adapter` +
/// `GPUAdapter::request_device` with an empty descriptor.
fn deno_request_device() -> (Arc<Global>, AdapterId, DeviceId, QueueId) {
    let backends = std::env::var("DENO_WEBGPU_BACKEND").map_or_else(
        |_| wgpu_types::Backends::all(),
        |s| wgpu_types::Backends::from_comma_list(&s),
    );
    let global = Arc::new(Global::new(
        "webgpu",
        wgpu_types::InstanceDescriptor {
            backends,
            flags: wgpu_types::InstanceFlags::from_build_config(),
            memory_budget_thresholds: wgpu_types::MemoryBudgetThresholds {
                for_resource_creation: Some(97),
                for_device_loss: Some(99),
            },
            backend_options: wgpu_types::BackendOptions {
                dx12: wgpu_types::Dx12BackendOptions {
                    shader_compiler: wgpu_types::Dx12Compiler::Fxc,
                    ..Default::default()
                },
                gl: wgpu_types::GlBackendOptions::default(),
                noop: wgpu_types::NoopBackendOptions::default(),
            },
            display: None,
        },
        None,
    ));
    let adapter = global
        .request_adapter(
            &wgpu_core::instance::RequestAdapterOptions {
                power_preference: wgpu_types::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            },
            backends,
            None,
        )
        .expect("requestAdapter");
    let (device, queue) = global
        .adapter_request_device(
            adapter,
            &wgpu_types::DeviceDescriptor {
                label: Some(Cow::Borrowed("game device")),
                required_features: wgpu_types::Features::empty(),
                required_limits: wgpu_types::Limits::default(),
                experimental_features: wgpu_types::ExperimentalFeatures::disabled(),
                memory_hints: Default::default(),
                trace: Default::default(),
            },
            None,
            None,
        )
        .expect("requestDevice");
    (global, adapter, device, queue)
}

fn deno_draw_canvas() -> DenoSide {
    let (global, adapter, device, queue) = deno_request_device();
    let texture_desc = wgpu_core::resource::TextureDescriptor {
        label: Some(Cow::Borrowed("canvas")),
        size: wgpu_types::Extent3d {
            width: CANVAS,
            height: CANVAS,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu_types::TextureDimension::D2,
        format: CANVAS_FORMAT,
        // No COPY_SRC: any copy of the canvas would be a validation error.
        usage: wgpu_types::TextureUsages::RENDER_ATTACHMENT
            | wgpu_types::TextureUsages::TEXTURE_BINDING,
        view_formats: Vec::new(),
    };
    let (texture, error) = global.device_create_texture(device, &texture_desc, None);
    check("canvas texture", error);
    let (view, error) = global.texture_create_view(
        texture,
        &wgpu_core::resource::TextureViewDescriptor::default(),
        None,
    );
    check("canvas view", error);
    let (module, error) = global.device_create_shader_module(
        device,
        &wgpu_core::pipeline::ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgpu_types::ShaderRuntimeChecks::default(),
        },
        wgpu_core::pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(CANVAS_SHADER)),
        None,
    );
    check("canvas shader", error);
    let stage = |entry: &'static str| wgpu_core::pipeline::ProgrammableStageDescriptor {
        module,
        entry_point: Some(Cow::Borrowed(entry)),
        constants: Default::default(),
        zero_initialize_workgroup_memory: true,
    };
    let (pipeline, error) = global.device_create_render_pipeline(
        device,
        &wgpu_core::pipeline::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu_core::pipeline::VertexState {
                stage: stage("vs_main"),
                buffers: Cow::Owned(Vec::new()),
            },
            primitive: wgpu_types::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu_types::MultisampleState::default(),
            fragment: Some(wgpu_core::pipeline::FragmentState {
                stage: stage("fs_main"),
                targets: Cow::Owned(vec![Some(wgpu_types::ColorTargetState {
                    format: CANVAS_FORMAT,
                    blend: None,
                    write_mask: wgpu_types::ColorWrites::ALL,
                })]),
            }),
            cache: None,
            multiview_mask: None,
        },
        None,
    );
    check("canvas pipeline", error);
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgpu_types::CommandEncoderDescriptor { label: None },
        None,
    );
    check("canvas encoder", error);
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        encoder,
        &wgpu_core::command::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Owned(vec![Some(
                wgpu_core::command::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    load_op: wgpu_types::LoadOp::Clear(wgpu_types::Color::TRANSPARENT),
                    store_op: wgpu_types::StoreOp::Store,
                },
            )]),
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        },
    );
    check("canvas pass", error);
    check(
        "set pipeline",
        global.render_pass_set_pipeline(&mut pass, pipeline).err(),
    );
    check("draw", global.render_pass_draw(&mut pass, 3, 1, 0, 0).err());
    check("end pass", global.render_pass_end(&mut pass).err());
    let (commands, error) = global.command_encoder_finish(
        encoder,
        &wgpu_types::CommandBufferDescriptor { label: None },
        None,
    );
    check("finish", error.map(|(_, error)| error));
    // Not awaited: Netrender's later submission on the same queue is ordered
    // after this one without any CPU wait.
    if let Err((_, error)) = global.queue_submit(queue, &[commands]) {
        panic!("canvas submit: {error}");
    }
    global.texture_view_drop(view);
    global.render_pipeline_drop(pipeline);
    global.shader_module_drop(module);
    DenoSide {
        global,
        adapter,
        device,
        queue,
        texture,
        texture_desc,
    }
}

fn main() {
    let deno = deno_draw_canvas();
    let global = deno.global.clone();
    let desc = CanvasDesc {
        size: deno.texture_desc.size,
        mip_level_count: deno.texture_desc.mip_level_count,
        sample_count: deno.texture_desc.sample_count,
        dimension: deno.texture_desc.dimension,
        format: deno.texture_desc.format,
        usage: deno.texture_desc.usage,
    };
    let embedder = EmbedderIds::clone_from_game(
        &global,
        deno.adapter,
        deno.device,
        deno.queue,
        deno.texture,
        desc,
    );
    let devices_before = hub_ids(&global).devices;
    let (handles, canvas) = embedder.adopt();

    // The page's GC: Deno drops its ids; the embedder's keep the objects alive.
    global.texture_drop(deno.texture);
    global.queue_drop(deno.queue);
    global.device_drop(deno.device);
    global.adapter_drop(deno.adapter);
    let devices_after = hub_ids(&global).devices;
    println!(
        "device ids: {devices_before} with the clones, {devices_after} after Deno dropped its own (no extra device)"
    );
    assert_eq!(devices_before, 2, "Deno's device id and the embedder clone");
    assert_eq!(
        devices_after, 1,
        "only the embedder id remains, no second device"
    );

    let failed = render_and_check(&global, handles, canvas);
    if failed > 0 {
        eprintln!("{failed} pixel assertion(s) failed");
        std::process::exit(1);
    }
    println!("netrender shared-device proof: all assertions passed");
}
