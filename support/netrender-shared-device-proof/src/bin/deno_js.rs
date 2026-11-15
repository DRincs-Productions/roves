//! V8 path of the shared-device proof (feature `deno`): the canvas texture is
//! created and drawn by unprivileged JavaScript through deno_webgpu 0.227.0 in a
//! real deno_core 0.413 `JsRuntime`, then composited by Netrender on the same
//! wgpu-core device (src/lib.rs, same scene and assertions as `src/main.rs`).
//!
//! 1. `src/deno_bootstrap.js` (privileged) loads deno_webgpu as Deno does,
//!    exposes only `navigator.gpu` and the WebGPU flag namespaces, captures
//!    `op_netrender_proof_adopt` and deletes `Deno`/`__bootstrap`. Its
//!    completion value is the embedder hook, held only by Rust.
//! 2. `src/guest.js` checks that no privileged global is visible, requests the
//!    adapter/device, creates a `RENDER_ATTACHMENT | TEXTURE_BINDING` texture
//!    (no `COPY_SRC`), draws it, submits and resolves to
//!    `{ adapter, device, texture }`.
//! 3. The embedder calls the hook with that frame; the op validates the cppgc
//!    objects (same `Global`, device of that adapter, texture of that device
//!    and queue) and registers the embedder's own ids (`Global::*_clone_id`).
//! 4. Every JS handle is released and V8 is collected until deno_webgpu's
//!    `Drop` impls have dropped the guest's ids: the hub must go from 2 ids per
//!    object (guest + embedder) to 1. Only then is the canvas adopted in wgpu
//!    and rendered by Netrender, while the runtime is still alive.
//! 5. Teardown: the queue is drained, every wgpu object is dropped (hub empty),
//!    then the runtime; the test's own `Arc<Global>` must be the last one.

use std::sync::Arc;

use deno_core::{JsRuntime, OpState, PollEventLoopOptions, RuntimeOptions, op2, v8};
use deno_error::JsErrorBox;
use deno_webgpu::adapter::GPUAdapter;
use deno_webgpu::device::GPUDevice;
use deno_webgpu::texture::GPUTexture;
use roves_netrender_shared_device_proof::{
    CANVAS_SHADER, CanvasDesc, EmbedderIds, HubIds, hub_ids, render_and_check,
};

/// Set by the op; taken by the embedder once the guest's handles are gone.
struct Adopted {
    ids: EmbedderIds,
    /// Hub ids when the op ran, before the embedder's clones.
    guest_only: HubIds,
}

/// Registers the embedder's ids for the guest's adapter, device and canvas
/// texture. Captured by the bootstrap, never reachable from the guest.
///
/// The adapter is an explicit argument: `GPUDevice` keeps only the adapter
/// *id*, not the `GPUAdapter` object, whose finalizer drops that id. Once the
/// guest's adapter is unreachable a GC can vacate `device.adapter` and
/// `adapter_clone_id` would panic (`Adapter[..] does not exist`). Borrowing the
/// cppgc `GPUAdapter` here keeps the id live for the clone.
#[op2(fast)]
fn op_netrender_proof_adopt(
    state: &mut OpState,
    #[cppgc] adapter: &GPUAdapter,
    #[cppgc] device: &GPUDevice,
    #[cppgc] texture: &GPUTexture,
) -> Result<(), JsErrorBox> {
    if state.has::<Adopted>() {
        return Err(JsErrorBox::type_error("canvas already adopted"));
    }
    if !Arc::ptr_eq(&adapter.instance, &device.instance) || adapter.id != device.adapter {
        return Err(JsErrorBox::type_error(
            "the device was not requested from this adapter",
        ));
    }
    if !Arc::ptr_eq(&device.instance, &texture.instance)
        || texture.device_id != device.id
        || texture.queue_id != device.queue
    {
        return Err(JsErrorBox::type_error(
            "the texture does not belong to this device",
        ));
    }
    let guest_only = hub_ids(&device.instance);
    let desc = CanvasDesc {
        size: texture.size,
        mip_level_count: texture.mip_level_count,
        sample_count: texture.sample_count,
        dimension: texture.dimension.clone().into(),
        format: texture.format.clone().into(),
        usage: texture.usage.0,
    };
    if desc.usage.contains(wgpu_types::TextureUsages::COPY_SRC) {
        return Err(JsErrorBox::type_error("the canvas must not be copyable"));
    }
    let ids = EmbedderIds::clone_from_game(
        &device.instance,
        adapter.id,
        device.id,
        device.queue,
        texture.id,
        desc,
    );
    state.put(Adopted { ids, guest_only });
    Ok(())
}

deno_core::extension!(
    netrender_proof,
    deps = [deno_webgpu],
    ops = [op_netrender_proof_adopt],
);

const GC_PASSES: usize = 8;

fn main() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime");
    let failed = tokio.block_on(run());
    if failed > 0 {
        eprintln!("{failed} pixel assertion(s) failed");
        std::process::exit(1);
    }
    println!("netrender shared-device proof (V8 + deno_webgpu): all assertions passed");
}

fn function(runtime: &mut JsRuntime, value: v8::Global<v8::Value>) -> v8::Global<v8::Function> {
    deno_core::scope!(scope, runtime);
    let value = v8::Local::new(scope, value);
    let function =
        v8::Local::<v8::Function>::try_from(value).expect("bootstrap returns a function");
    v8::Global::new(scope, function)
}

async fn run() -> usize {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![
            deno_webidl::deno_webidl::init(),
            deno_web::deno_web::init(
                Arc::new(deno_web::BlobStore::default()),
                None,
                false,
                deno_web::InMemoryBroadcastChannel::default(),
            ),
            deno_webgpu::deno_webgpu::init(),
            netrender_proof::init(),
        ],
        ..Default::default()
    });

    let hook = runtime
        .execute_script(
            "netrender-proof-bootstrap.js",
            include_str!("../deno_bootstrap.js"),
        )
        .expect("bootstrap");
    let hook = function(&mut runtime, hook);

    // canvas.wgsl's comments contain backticks: embed it as a JSON string
    // literal (valid JavaScript), never by splicing it into a template literal.
    let wgsl = deno_core::serde_json::to_string(CANVAS_SHADER).expect("WGSL as a JSON string");
    let guest = format!(
        "const CANVAS_WGSL = {wgsl};\n{}",
        include_str!("../guest.js")
    );
    let promise = runtime
        .execute_script("guest.js", guest)
        .expect("guest script");
    let resolve = runtime.resolve(promise);
    let frame = runtime
        .with_event_loop_promise(Box::pin(resolve), PollEventLoopOptions::default())
        .await
        .expect("guest frame");
    // guest.js rejects (and `expect` panics above) if any Deno/__bootstrap/op_*
    // global is visible to it.
    println!("guest: no privileged global visible, frame resolved");
    let adopt = runtime.call_with_args(&hook, &[frame]);
    runtime
        .with_event_loop_promise(Box::pin(adopt), PollEventLoopOptions::default())
        .await
        .expect("embedder adopts the guest canvas");
    drop(hook);

    let Adopted { ids, guest_only } = runtime
        .op_state()
        .borrow_mut()
        .try_take::<Adopted>()
        .expect("the hook registered the canvas");
    let global = ids.global.clone();
    println!(
        "hub ids: guest only {guest_only:?}, with the embedder clones {:?}",
        hub_ids(&global)
    );
    assert_eq!(
        guest_only.devices, 1,
        "the guest created exactly one device"
    );
    assert_eq!(guest_only.queues, 1, "and one queue");
    assert_eq!(
        hub_ids(&global),
        HubIds {
            adapters: guest_only.adapters + 1,
            devices: 2,
            queues: 2,
            textures: guest_only.textures + 1,
        },
        "one embedder id per adopted object, no new object"
    );

    // The page's GC: no JS handle to the frame remains (the guest kept none,
    // the embedder dropped its handles), so deno_webgpu's GPUTexture,
    // GPUDevice, GPUQueue (freed after the device that holds it) and
    // GPUAdapter drop the guest's ids. The embedder's ids keep the objects.
    let embedder_only = HubIds {
        adapters: 1,
        devices: 1,
        queues: 1,
        textures: 1,
    };
    let mut passes = 0;
    while hub_ids(&global) != embedder_only && passes < GC_PASSES {
        runtime.v8_isolate().low_memory_notification();
        runtime
            .run_event_loop(PollEventLoopOptions::default())
            .await
            .expect("event loop after GC");
        passes += 1;
    }
    println!("hub ids after {passes} GC pass(es): {:?}", hub_ids(&global));
    assert_eq!(
        hub_ids(&global),
        embedder_only,
        "V8 GC must release every guest id while the embedder's stay"
    );

    // Rendering with the runtime alive, as the probe would every frame.
    let (handles, canvas) = ids.adopt();
    let failed = render_and_check(&global, handles, canvas);
    assert_eq!(
        hub_ids(&global),
        HubIds {
            adapters: 0,
            devices: 0,
            queues: 0,
            textures: 0,
        },
        "the embedder dropped its ids after the drained frame"
    );

    drop(runtime);
    assert_eq!(
        Arc::strong_count(&global),
        1,
        "deno_webgpu (OpState) and wgpu released the shared Global"
    );
    drop(global);
    failed
}
