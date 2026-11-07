// PixiJS 8.19.0 (the version Pixi'VN 1.9.8 resolves to in the template) on the
// probe's WebGPU (deno_webgpu). The probe exposes no WebGL context, so Pixi's
// autoDetectRenderer falls back to WebGPU exactly as it would for a game.
// Pixels are read back here, through standard WebGPU calls, for the check only.
import { Application, Graphics } from "pixi.js";

(() => {
  "use strict";
  const result = { status: "running", detail: null, renderer: null };
  globalThis.webgpuResult = result;

  const finish = (status, detail = null) => {
    result.status = status;
    result.detail = detail;
  };

  async function readPixel(device, texture, format, x, y) {
    const bytesPerRow = Math.ceil((texture.width * 4) / 256) * 256;
    const buffer = device.createBuffer({
      size: bytesPerRow * texture.height,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = device.createCommandEncoder();
    encoder.copyTextureToBuffer({ texture }, { buffer, bytesPerRow }, [texture.width, texture.height]);
    device.queue.submit([encoder.finish()]);
    await buffer.mapAsync(GPUMapMode.READ);
    const bytes = new Uint8Array(buffer.getMappedRange());
    const offset = y * bytesPerRow + x * 4;
    let [r, g, b, a] = bytes.slice(offset, offset + 4);
    if (format.startsWith("bgra")) [r, b] = [b, r];
    buffer.unmap();
    return [r, g, b, a];
  }

  (async () => {
    if (!navigator.gpu) return finish("no_webgpu");
    const adapter = await navigator.gpu.requestAdapter();
    if (adapter === null) return finish("no_adapter");

    const canvas = document.createElement("canvas");
    canvas.width = 256;
    canvas.height = 256;
    const app = new Application();
    await app.init({ canvas, width: 256, height: 256, resolution: 1, background: "#204060", autoStart: false });
    result.renderer = app.renderer.name ?? String(app.renderer.type);

    app.stage.addChild(new Graphics().rect(0, 0, 128, 128).fill(0xff0000).circle(192, 192, 40).fill(0x00ff00));
    app.render();

    const context = canvas.getContext("webgpu");
    const texture = context.getCurrentTexture();
    const { device, format } = context.getConfiguration();
    const red = await readPixel(device, texture, format, 64, 64);
    const green = await readPixel(device, texture, format, 192, 192);
    const background = await readPixel(device, texture, format, 10, 240);
    const close = (actual, expected) => expected.every((value, index) => Math.abs(actual[index] - value) <= 3);
    const pixels = { red, green, background };
    if (close(red, [255, 0, 0]) && close(green, [0, 255, 0]) && close(background, [0x20, 0x40, 0x60])) {
      finish("passed", pixels);
    } else {
      finish("failed", pixels);
    }
  })().catch((error) => finish("failed", String(error?.stack ?? error)));
})();
