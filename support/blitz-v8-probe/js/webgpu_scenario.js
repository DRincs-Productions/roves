// PixiJS 8.19.0 (the version Pixi'VN 1.9.8 resolves to in the template) on the
// probe's WebGPU (deno_webgpu). The probe exposes no WebGL context, so Pixi's
// autoDetectRenderer falls back to WebGPU exactly as it would for a game.
// Pixels are read back here, through standard WebGPU calls, for the check only.
import { Application, Assets, Graphics, Sprite } from "pixi.js";

(() => {
  "use strict";
  const result = { status: "running", detail: null, renderer: null, step: "start", adapter: null };
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
    result.step = "adapter";
    if (!navigator.gpu) return finish("no_webgpu");
    const adapter = await navigator.gpu.requestAdapter();
    if (adapter === null) return finish("no_adapter");
    // Standard adapter info only (diagnostic): lets CI tell a software/fallback
    // adapter (e.g. WARP on the runner) apart from a hardware GPU even when the
    // scenario fails before teardown prints the wgpu-core adapter line. A
    // software adapter is still run, never treated as unavailable.
    try {
      const info = adapter.info;
      result.adapter = {
        vendor: info?.vendor ?? null,
        architecture: info?.architecture ?? null,
        device: info?.device ?? null,
        description: info?.description ?? null,
        fallback: info?.isFallbackAdapter ?? adapter.isFallbackAdapter ?? null,
      };
    } catch (error) {
      result.adapter = { error: String(error) };
    }

    const canvas = document.createElement("canvas");
    canvas.width = 256;
    canvas.height = 256;
    result.step = "init";
    const app = new Application();
    await app.init({ canvas, width: 256, height: 256, resolution: 1, background: "#204060", autoStart: false });
    result.renderer = app.renderer.name ?? String(app.renderer.type);

    result.step = "render";
    app.stage.addChild(new Graphics().rect(0, 0, 128, 128).fill(0xff0000).circle(192, 192, 40).fill(0x00ff00));
    // An image texture: fetch(data:) -> createImageBitmap (deno_image) ->
    // copyExternalImageToTexture (probe polyfill) -> sprite.
    result.step = "image";
    const imageTexture = await Assets.load("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=");
    const sprite = new Sprite(imageTexture);
    sprite.position.set(200, 20);
    sprite.width = 32;
    sprite.height = 32;
    app.stage.addChild(sprite);
    result.step = "render";
    app.render();

    result.step = "readback";
    const context = canvas.getContext("webgpu");
    const texture = context.getCurrentTexture();
    const { device, format } = context.getConfiguration();
    const red = await readPixel(device, texture, format, 64, 64);
    const green = await readPixel(device, texture, format, 192, 192);
    const background = await readPixel(device, texture, format, 10, 240);
    const image = await readPixel(device, texture, format, 216, 36);
    const close = (actual, expected) => expected.every((value, index) => Math.abs(actual[index] - value) <= 3);
    const pixels = { red, green, background, image };

    // Save-thumbnail path: extract.base64 -> a WebGPU canvas -> toBlob
    // (probe readback + PNG) -> FileReader.readAsDataURL.
    // The PNG is decoded with createImageBitmap, uploaded to a texture and read.
    result.step = "extract";
    const dataUrl = await app.renderer.extract.base64(app.stage);
    const extracted = { prefix: dataUrl.slice(0, 22), length: dataUrl.length };
    pixels.extract = extracted;
    if (!dataUrl.startsWith("data:image/png;base64,")) return finish("failed", pixels);
    const bitmap = await createImageBitmap(await (await fetch(dataUrl)).blob());
    extracted.size = [bitmap.width, bitmap.height];
    const decoded = device.createTexture({
      size: [bitmap.width, bitmap.height],
      format: "rgba8unorm",
      usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.COPY_SRC | GPUTextureUsage.TEXTURE_BINDING,
    });
    device.queue.copyExternalImageToTexture({ source: bitmap }, { texture: decoded }, [bitmap.width, bitmap.height]);
    extracted.red = await readPixel(device, decoded, "rgba8unorm", 64, 64);
    extracted.green = await readPixel(device, decoded, "rgba8unorm", 192, 192);
    decoded.destroy();
    const extractOk = bitmap.width >= 200 && bitmap.height >= 200
      && close(extracted.red, [255, 0, 0, 255]) && close(extracted.green, [0, 255, 0, 255]);

    if (extractOk && close(red, [255, 0, 0]) && close(green, [0, 255, 0]) && close(background, [0x20, 0x40, 0x60]) && close(image, [255, 0, 0])) {
      finish("passed", pixels);
    } else {
      finish("failed", pixels);
    }
  })().catch((error) => finish("failed", `${result.step}: ${error?.stack ?? error?.message ?? String(error)} (${Object.prototype.toString.call(error)})`));
})();
