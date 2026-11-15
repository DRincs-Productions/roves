// Guest (game) code, unprivileged: plain WebGPU, as PixiJS would use it.
// `CANVAS_WGSL` is prepended by the embedder (src/canvas.wgsl, the same shader
// the raw-core producer uses). Evaluates to a promise of the frame the
// embedder presents; the guest keeps no other reference to it.
(async () => {
  const privileged = Object.getOwnPropertyNames(globalThis).filter((name) =>
    /^(Deno|__bootstrap|op_)/.test(name)
  );
  if (privileged.length > 0) throw new Error(`privileged globals visible: ${privileged}`);
  if (typeof Deno !== "undefined") throw new Error("Deno namespace visible to the guest");

  const adapter = await navigator.gpu.requestAdapter();
  if (adapter === null) throw new Error("no WebGPU adapter");
  const device = await adapter.requestDevice();
  device.pushErrorScope("validation");
  device.pushErrorScope("out-of-memory");
  // A canvas-like texture without COPY_SRC: nobody can copy or read it back.
  const texture = device.createTexture({
    label: "canvas",
    size: [16, 16],
    format: "rgba8unorm",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
  });
  const module = device.createShaderModule({ code: CANVAS_WGSL });
  const pipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs_main" },
    fragment: { module, entryPoint: "fs_main", targets: [{ format: "rgba8unorm" }] },
  });
  const encoder = device.createCommandEncoder();
  const pass = encoder.beginRenderPass({
    colorAttachments: [{
      view: texture.createView(),
      loadOp: "clear",
      clearValue: { r: 0, g: 0, b: 0, a: 0 },
      storeOp: "store",
    }],
  });
  pass.setPipeline(pipeline);
  pass.draw(3);
  pass.end();
  // Not awaited: Netrender's later submission on the same queue is ordered after it.
  device.queue.submit([encoder.finish()]);
  for (const scope of ["out-of-memory", "validation"]) {
    const error = await device.popErrorScope();
    if (error !== null) throw new Error(`${scope} error: ${error.message}`);
  }
  // The adapter is part of the frame: GPUDevice keeps only its id, and the
  // embedder must clone that id while the GPUAdapter is still alive.
  return { adapter, device, texture };
})();
