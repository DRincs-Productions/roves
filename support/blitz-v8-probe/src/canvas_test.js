// Canvas 2D pixel subset and PNG export (dom.js), with the calls of Pixi's
// extract paths: getContext("2d") -> createImageData -> data.set ->
// putImageData -> toDataURL, and toBlob -> FileReader.readAsDataURL (what
// Pixi's extract.base64 prefers). Also context mode, resize, clipping, dirty
// rectangle, bounds and the toBlob snapshot. Encoding completes from tasks, so
// main.rs drives frames until `canvasResult.done`, then decodes
// `canvasResult.pngHex` with the png crate and compares it with PIXELS.
(() => {
  "use strict";
  const result = { done: false, step: "start", failures: [], checks: 0, pngHex: null };
  globalThis.canvasResult = result;
  const expect = (condition, message) => {
    result.checks++;
    if (!condition) result.failures.push(`${result.step}: ${message}`);
  };
  const errorName = (callback) => {
    try {
      callback();
      return "none";
    } catch (error) {
      return error?.name ?? String(error);
    }
  };
  const same = (actual, expected) => actual.length === expected.length && expected.every((value, index) => actual[index] === value);
  const PNG_PREFIX = "data:image/png;base64,";
  // 3x2 straight-alpha RGBA: opaque red/green/blue, then white, a translucent
  // pixel and transparent black. main.rs holds the same bytes.
  const PIXELS = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
    255, 255, 255, 255, 10, 20, 30, 128, 0, 0, 0, 0,
  ];
  const pixel = (x, y) => PIXELS.slice((y * 3 + x) * 4, (y * 3 + x) * 4 + 4);
  const toBlob = (canvas, ...args) => new Promise((resolve) => canvas.toBlob(resolve, ...args));
  const readDataUrl = (blob) =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });

  result.step = "context_mode";
  const canvas = document.createElement("canvas");
  expect(canvas.width === 300 && canvas.height === 150, `default size ${canvas.width}x${canvas.height}`);
  canvas.setAttribute("width", " +7px");
  expect(canvas.width === 7, `width attribute parsing gave ${canvas.width}`);
  canvas.width = 3;
  canvas.height = 2;
  const context = canvas.getContext("2d");
  expect(context instanceof CanvasRenderingContext2D, "getContext('2d') returns a CanvasRenderingContext2D");
  expect(context.canvas === canvas, "context.canvas");
  expect(canvas.getContext("2d", { willReadFrequently: true }) === context, "same 2d context on the second call");
  expect(canvas.getContext("webgpu") === null && canvas.getContext("webgl") === null, "other context types return null once 2d is set");
  expect(errorName(() => new CanvasRenderingContext2D()) === "TypeError", "CanvasRenderingContext2D is not constructible");
  expect(errorName(() => canvas.getContext()) === "TypeError", "getContext requires an argument");

  // Pixi GlTextureSystem.generateCanvas: createImageData + data.set + putImageData.
  result.step = "pixel_upload";
  const image = context.createImageData(3, 2);
  expect(image instanceof ImageData && image.width === 3 && image.height === 2, "createImageData(3, 2) is a 3x2 ImageData");
  expect(image.data instanceof Uint8ClampedArray && image.data.length === 24 && image.data.every((value) => value === 0), "new ImageData is transparent black");
  image.data.set(PIXELS);
  context.putImageData(image, 0, 0);
  expect(same(context.getImageData(0, 0, 3, 2).data, PIXELS), "getImageData returns the uploaded pixels exactly");
  const copy = context.createImageData(image);
  expect(copy.width === 3 && copy.height === 2 && copy.data.every((value) => value === 0), "createImageData(imagedata) copies the size only");
  // Pixels outside the bitmap read as transparent black; negative sizes select
  // the rectangle before the origin.
  expect(same(context.getImageData(-1, -1, 2, 2).data, [...new Array(12).fill(0), ...pixel(0, 0)]), "clipped read");
  expect(same(context.getImageData(1, 1, -1, -1).data, pixel(0, 0)), "negative width/height read the rectangle before the origin");

  result.step = "dirty_rect";
  const small = document.createElement("canvas");
  small.width = 2;
  small.height = 2;
  const smallContext = small.getContext("2d");
  // Clipped put at (-1, -1): (0,0) <- source (1,1), (1,0) <- source (2,1).
  smallContext.putImageData(image, -1, -1);
  smallContext.putImageData(image, 5, 5);
  // Dirty rectangles (source (0,0) only), the second normalized from -1 sizes.
  smallContext.putImageData(image, 1, 0, 0, 0, 1, 1);
  smallContext.putImageData(image, 0, 1, 1, 1, -1, -1);
  expect(
    same(smallContext.getImageData(0, 0, 2, 2).data, [...pixel(1, 1), ...pixel(0, 0), ...pixel(0, 0), 0, 0, 0, 0]),
    `dirty rectangle and clipped puts gave ${Array.from(smallContext.getImageData(0, 0, 2, 2).data)}`,
  );

  result.step = "errors_and_bounds";
  expect(errorName(() => context.createImageData(0, 1)) === "IndexSizeError", "createImageData with zero width");
  expect(errorName(() => context.getImageData(0, 0, 1, 0)) === "IndexSizeError", "getImageData with zero height");
  expect(errorName(() => context.createImageData(20000, 20000)) === "RangeError", "createImageData over the probe bound");
  expect(errorName(() => context.getImageData(0, 0, 16385, 1)) === "RangeError", "getImageData over the probe bound");
  expect(errorName(() => context.createImageData(Infinity, 1)) === "TypeError", "createImageData enforces the long range");
  expect(errorName(() => context.putImageData({ width: 1, height: 1, data: new Uint8ClampedArray(4) }, 0, 0)) === "TypeError", "putImageData requires an ImageData");
  expect(errorName(() => context.putImageData(image, 0, 0, 0)) === "TypeError", "putImageData takes 3 or 7 arguments");
  const huge = document.createElement("canvas");
  huge.width = 16385;
  huge.height = 1;
  expect(errorName(() => huge.toDataURL()) === "RangeError", "toDataURL over the probe bound");
  expect(errorName(() => huge.getContext("2d").putImageData(image, 0, 0)) === "RangeError", "2d bitmap over the probe bound is not allocated");

  result.step = "resize";
  const resized = document.createElement("canvas");
  resized.width = 2;
  resized.height = 2;
  const resizedContext = resized.getContext("2d");
  resizedContext.putImageData(image, 0, 0);
  resized.width = 2;
  expect(same(resizedContext.getImageData(0, 0, 2, 2).data, new Array(16).fill(0)), "setting width (same value) clears the bitmap");
  resized.height = 1;
  expect(same(resizedContext.getImageData(0, 0, 2, 2).data, new Array(16).fill(0)) && resized.height === 1, "resized bitmap is transparent");
  resizedContext.putImageData(image, 0, 0);
  expect(same(resizedContext.getImageData(0, 0, 2, 2).data, [...pixel(0, 0), ...pixel(1, 0), 0, 0, 0, 0, 0, 0, 0, 0]), "resized bitmap clips at the new height");

  result.step = "to_data_url";
  const url = canvas.toDataURL("image/png");
  expect(url.startsWith(PNG_PREFIX), `toDataURL is a PNG data URL (${url.slice(0, 32)})`);
  expect(canvas.toDataURL("image/webp", 0.5) === url && canvas.toDataURL() === url, "unsupported types and quality fall back to the same PNG");
  const empty = document.createElement("canvas");
  empty.width = 0;
  expect(empty.toDataURL() === "data:,", "zero-size canvas gives data:,");
  const blank = document.createElement("canvas");
  blank.width = 1;
  blank.height = 1;
  expect(blank.toDataURL().startsWith(PNG_PREFIX), "canvas without a context encodes transparent black");
  const binary = atob(url.slice(PNG_PREFIX.length));
  expect(binary.startsWith("\x89PNG\r\n\x1a\n"), "PNG signature");
  result.pngHex = Array.from(binary, (char) => char.charCodeAt(0).toString(16).padStart(2, "0")).join("");

  (async () => {
    result.step = "to_blob";
    // The bitmap is captured when toBlob is called: a later put must not change it.
    const pending = toBlob(canvas, "image/png", 1);
    const scratch = context.createImageData(3, 2);
    context.putImageData(scratch, 0, 0);
    const blob = await pending;
    expect(blob instanceof Blob && blob.type === "image/png" && blob.size > 8, `toBlob gives a PNG Blob (${blob?.type}, ${blob?.size})`);
    const blobUrl = await readDataUrl(blob);
    expect(blobUrl === url, "FileReader.readAsDataURL(toBlob) equals the toDataURL taken before the later put");
    const bitmap = await createImageBitmap(blob);
    expect(bitmap.width === 3 && bitmap.height === 2, `deno_image decodes the toBlob PNG as ${bitmap.width}x${bitmap.height}`);
    expect((await toBlob(empty)) === null, "zero-size toBlob calls back with null");
    expect(errorName(() => canvas.toBlob()) === "TypeError" && errorName(() => canvas.toBlob(1)) === "TypeError", "toBlob requires a callback");
    result.step = "finished";
  })()
    .catch((error) => result.failures.push(`${result.step}: ${error?.stack ?? error}`))
    .finally(() => {
      result.done = true;
    });
})();
