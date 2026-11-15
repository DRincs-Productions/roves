// Embedder bootstrap (privileged, runs before any guest code). Mirrors the
// Blitz probe's dom.js: deno_webgpu is loaded lazily as Deno does, only
// `navigator.gpu` and the WebGPU flag namespaces reach the guest, the private
// op is captured here and `Deno`/`__bootstrap` are deleted from the global.
// The script's completion value is the embedder hook; Rust keeps it as a
// handle, so no bridge entry point is reachable from the guest global.
(() => {
  const core = globalThis.Deno.core;
  const { op_netrender_proof_adopt } = core.ops;
  const webgpu = core.createLazyLoader("ext:deno_webgpu/01_webgpu.js")();
  webgpu.initGPU();
  const navigator = Object.create(null);
  Object.defineProperty(navigator, "gpu", { value: webgpu.gpu, enumerable: true });
  Object.defineProperty(globalThis, "navigator", {
    value: Object.freeze(navigator),
    enumerable: true,
  });
  for (const name of ["GPUBufferUsage", "GPUColorWrite", "GPUMapMode", "GPUShaderStage", "GPUTextureUsage"]) {
    Object.defineProperty(globalThis, name, { value: webgpu[name], enumerable: false, configurable: true, writable: true });
  }
  delete globalThis.Deno;
  delete globalThis.__bootstrap;
  // Called by the embedder with the guest's frame. The cppgc op arguments
  // reject anything that is not a real GPUAdapter/GPUDevice/GPUTexture.
  return function adoptCanvas(frame) {
    op_netrender_proof_adopt(frame.adapter, frame.device, frame.texture);
  };
})();
