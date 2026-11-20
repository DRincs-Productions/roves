// Controlled WARP cost experiment (diagnostic fixture, no PixiJS, no template).
//
// Question (CI 44134799443, JITless, Windows runner adapter "Microsoft Basic
// Render Driver"): the template's slowest completed batch took 17369.9 ms for
// 8 submits / 4 rAF with ZERO texture bytes, while the same process's first
// batches on the menu took <= 57.8 ms. Is the per-frame GPU cost on WARP driven
// by the template's fixed 1920x1080 canvas backing, by MSAA, by full-screen
// texture sampling, by Pixi's two submits per rAF (GpuUniformBatchPipe copy +
// GpuEncoderSystem.postrender), or does a constant workload get slower over
// time? Each configuration below renders the same Pixi-shaped frame through the
// probe's own canvas context (getCurrentTexture of a <canvas> element, exactly
// the path Pixi uses) and waits for that frame alone with
// queue.onSubmittedWorkDone(), so a sample is one frame's GPU completion
// latency plus a constant bias (deno_webgpu's 10 ms poll sleep and the tick):
// the `empty` configuration measures that bias.
//
// Loaded with ROVES_PROBE_WEBGPU=1 ROVES_PROBE_WEBGPU_SCENARIO=<this file> into
// an already built probe (no rebuild); it reports through the same
// `webgpuResult` slot as the embedded Pixi scenario (check webgpu_pixi_8_19_0).
//
// Bounds (CI c36c0c0fc7a: the driver's fixed frame batches ended while
// tex8_1080_s1 was still waiting, so the probe asserted on "running" and the
// last cases and teardown never reported). The scenario now declares its total
// wall-clock budget in `budget_ms`; the probe driver honours it (plus a grace)
// for overrides only. Every case has its own slot, and each frame's
// onSubmittedWorkDone() is raced against what is left of that slot, so one
// stalled WARP frame cannot consume the others. After a frame timeout the
// queue gets one bounded drain; if it drains, the next case starts on an idle
// queue, otherwise the remaining cases are skipped as "queue_not_drained"
// (measuring behind pending work would be wrong). Status: "passed" = every case
// ran its fixed frames; "partial" = some case stopped on its own bound (see
// `stopped`/`skipped` per case); "failed" = exception, device loss or an
// uncaptured WebGPU error, never downgraded to partial.
(() => {
  "use strict";
  // Upload slot + 8 cases x 8 s + growth 24 s + one drain, with margin for
  // pipeline creation. Driver deadline = BUDGET_MS + 15 s, then teardown <= 120 s,
  // inside the workflow's `timeout 330`.
  const BUDGET_MS = 125_000;
  const CASE_MS = 8_000;
  const GROWTH_CASE_MS = 24_000;
  const UPLOAD_MS = 15_000;
  const DRAIN_MS = 15_000;
  const MIN_SLOT_MS = 1_000;
  const result = { status: "running", step: "start", detail: null, adapter: null, budget_ms: BUDGET_MS };
  globalThis.webgpuResult = result;
  const finish = (status, detail = null) => {
    result.status = status;
    result.detail = detail;
  };
  const startedAt = performance.now();
  const elapsed = () => performance.now() - startedAt;
  const TIMED_OUT = Symbol("timed_out");
  // Resolves with the promise's value, or TIMED_OUT after `ms`. The GPU work is
  // not cancelled: the caller must treat the queue as still busy.
  const within = (promise, ms) => {
    let timer;
    const timeout = new Promise((resolve) => {
      timer = setTimeout(() => resolve(TIMED_OUT), Math.max(0, ms));
    });
    return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
  };
  const round = (value) => Math.round(value * 10) / 10;
  const median = (values) => {
    if (values.length === 0) return null;
    const sorted = [...values].sort((a, b) => a - b);
    return round(sorted[Math.floor(sorted.length / 2)]);
  };

  const SHADER = `
struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) index: u32) -> Out {
  let xy = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
  var out: Out;
  out.position = vec4f(xy * 2.0 - 1.0, 0.0, 1.0);
  out.uv = vec2f(xy.x, 1.0 - xy.y);
  return out;
}
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var imageSampler: sampler;
@fragment fn fs(input: Out) -> @location(0) vec4f {
  return textureSample(image, imageSampler, input.uv);
}`;

  (async () => {
    result.step = "adapter";
    if (!navigator.gpu) return finish("no_webgpu");
    const adapter = await navigator.gpu.requestAdapter();
    if (adapter === null) return finish("no_adapter");
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
    const device = await adapter.requestDevice();
    // Real device errors end the run as "failed", never as a partial result.
    let lostInfo = null;
    device.lost?.then((info) => {
      lostInfo = { reason: info?.reason ?? null, message: String(info?.message ?? "") };
    });
    const uncapturedErrors = [];
    device.addEventListener?.("uncapturederror", (event) => {
      if (uncapturedErrors.length < 5) uncapturedErrors.push(String(event?.error?.message ?? event?.error ?? event));
    });
    const assertDevice = () => {
      if (lostInfo !== null) throw new Error(`device lost: ${JSON.stringify(lostInfo)}`);
      if (uncapturedErrors.length > 0) throw new Error(`uncaptured WebGPU error: ${uncapturedErrors.join(" | ")}`);
    };
    const format = navigator.gpu.getPreferredCanvasFormat();
    const canvas = document.createElement("canvas");
    document.body.appendChild(canvas);
    const context = canvas.getContext("webgpu");
    context.configure({ device, format, alphaMode: "premultiplied" });

    // One 1920x1080 rgba8unorm image, uploaded once like the template's
    // background (8294400 bytes, the size CI reported).
    result.step = "upload";
    const IMAGE_W = 1920;
    const IMAGE_H = 1080;
    const image = device.createTexture({
      size: [IMAGE_W, IMAGE_H],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT,
    });
    const row = new Uint8Array(IMAGE_W * 4);
    for (let x = 0; x < IMAGE_W; x++) {
      row[x * 4] = x & 255;
      row[x * 4 + 1] = (x >> 3) & 255;
      row[x * 4 + 2] = 128;
      row[x * 4 + 3] = 255;
    }
    const pixels = new Uint8Array(IMAGE_W * IMAGE_H * 4);
    for (let y = 0; y < IMAGE_H; y++) pixels.set(row, y * row.length);
    let started = performance.now();
    device.queue.writeTexture({ texture: image }, pixels, { bytesPerRow: IMAGE_W * 4, rowsPerImage: IMAGE_H }, [IMAGE_W, IMAGE_H]);
    device.queue.submit([device.createCommandEncoder().finish()]);
    const uploadCpuMs = performance.now() - started;
    // Pending GPU work after a timeout (frame or upload); later cases never
    // measure behind it.
    let pending = null;
    const uploadDone = device.queue.onSubmittedWorkDone();
    const upload = { bytes: pixels.byteLength, cpu_ms: round(uploadCpuMs), done_ms: null };
    if (await within(uploadDone, UPLOAD_MS) === TIMED_OUT) {
      upload.timed_out_ms = UPLOAD_MS;
      pending = uploadDone;
    } else {
      upload.done_ms = round(performance.now() - started);
    }
    assertDevice();
    // One bounded wait for pending work; records the outcome on `report`.
    const drain = async (report) => {
      const waitMs = Math.min(DRAIN_MS, BUDGET_MS - elapsed());
      const drainStart = performance.now();
      if (waitMs > 0 && await within(pending, waitMs) !== TIMED_OUT) {
        report.drain_ms = round(performance.now() - drainStart);
        pending = null;
      } else {
        report.drain = "not_done";
        report.drain_wait_ms = round(Math.max(0, waitMs));
      }
      assertDevice();
    };
    if (pending) await drain(upload);

    result.step = "pipelines";
    const module = device.createShaderModule({ code: SHADER });
    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
    const pipelines = new Map();
    const pipelineFor = (samples) => {
      if (!pipelines.has(samples)) {
        pipelines.set(samples, device.createRenderPipeline({
          layout: "auto",
          vertex: { module, entryPoint: "vs" },
          fragment: {
            module,
            entryPoint: "fs",
            // Premultiplied "normal" blending, as Pixi's batcher.
            targets: [{
              format,
              blend: {
                color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
                alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
              },
            }],
          },
          primitive: { topology: "triangle-list" },
          multisample: { count: samples },
        }));
      }
      return pipelines.get(samples);
    };
    const bindGroups = new Map();
    const bindGroupFor = (samples) => {
      if (!bindGroups.has(samples)) {
        bindGroups.set(samples, device.createBindGroup({
          layout: pipelineFor(samples).getBindGroupLayout(0),
          entries: [{ binding: 0, resource: image.createView() }, { binding: 1, resource: sampler }],
        }));
      }
      return bindGroups.get(samples);
    };
    // Pixi's per-frame uniform-batch copy (GpuUniformBatchPipe): a separate
    // submit with a small copyBufferToBuffer.
    const uniformSource = device.createBuffer({ size: 512, usage: GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
    const uniformTarget = device.createBuffer({ size: 512, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.UNIFORM });

    // name, backing size, MSAA samples, full-screen draws, extra uniform submit.
    const CONFIGS = [
      { name: "empty", width: 1920, height: 1080, samples: 1, draws: -1, uniformSubmit: false, frames: 8 },
      { name: "clear_1080_s1", width: 1920, height: 1080, samples: 1, draws: 0, uniformSubmit: false, frames: 8 },
      { name: "tex2_1080_s1", width: 1920, height: 1080, samples: 1, draws: 2, uniformSubmit: false, frames: 8 },
      { name: "tex2_540_s1", width: 960, height: 540, samples: 1, draws: 2, uniformSubmit: false, frames: 8 },
      { name: "tex2_1080_s4", width: 1920, height: 1080, samples: 4, draws: 2, uniformSubmit: false, frames: 8 },
      { name: "tex2_540_s4", width: 960, height: 540, samples: 4, draws: 2, uniformSubmit: false, frames: 8 },
      { name: "tex2_1080_s1_pixi2submits", width: 1920, height: 1080, samples: 1, draws: 2, uniformSubmit: true, frames: 8 },
      { name: "tex8_1080_s1", width: 1920, height: 1080, samples: 1, draws: 8, uniformSubmit: false, frames: 8 },
      // Constant workload over time: does the per-frame latency grow?
      { name: "growth_tex2_1080_s1", width: 1920, height: 1080, samples: 1, draws: 2, uniformSubmit: true, frames: 40, slotMs: GROWTH_CASE_MS },
    ];
    const WARMUP = 2;
    const reports = [];
    // Published live: if the embedder's deadline ends first, the result line
    // (status "running") still carries the configurations measured.
    result.configs = reports;
    result.upload = upload;
    let budgetExhausted = false;
    for (const config of CONFIGS) {
      if (pending) {
        reports.push({ name: config.name, skipped: "queue_not_drained" });
        continue;
      }
      const slotMs = Math.min(config.slotMs ?? CASE_MS, BUDGET_MS - elapsed());
      if (slotMs < MIN_SLOT_MS) {
        budgetExhausted = true;
        reports.push({ name: config.name, skipped: "budget" });
        continue;
      }
      result.step = config.name;
      if (canvas.width !== config.width) canvas.width = config.width;
      if (canvas.height !== config.height) canvas.height = config.height;
      const msaa = config.samples > 1 && config.draws >= 0
        ? device.createTexture({ size: [config.width, config.height], format, sampleCount: config.samples, usage: GPUTextureUsage.RENDER_ATTACHMENT })
        : null;
      const latencies = [];
      const warmupLatencies = [];
      const cpu = [];
      const configStart = performance.now();
      let stopped = "complete";
      let timedOutFrame = null;
      for (let frame = 0; frame < WARMUP + config.frames; frame++) {
        assertDevice();
        if (performance.now() - configStart >= slotMs) {
          stopped = "slot";
          break;
        }
        const frameStart = performance.now();
        if (config.uniformSubmit) {
          device.queue.writeBuffer(uniformSource, 0, new Float32Array([frame, 1, 2, 3]));
          const encoder = device.createCommandEncoder();
          encoder.copyBufferToBuffer(uniformSource, 0, uniformTarget, 0, 256);
          device.queue.submit([encoder.finish()]);
        }
        const encoder = device.createCommandEncoder();
        if (config.draws >= 0) {
          const target = context.getCurrentTexture().createView();
          const pass = encoder.beginRenderPass({
            colorAttachments: [{
              view: msaa ? msaa.createView() : target,
              resolveTarget: msaa ? target : undefined,
              clearValue: { r: 0, g: 0, b: 0, a: 1 },
              loadOp: "clear",
              storeOp: msaa ? "discard" : "store",
            }],
          });
          if (config.draws > 0) {
            pass.setPipeline(pipelineFor(config.samples));
            pass.setBindGroup(0, bindGroupFor(config.samples));
            for (let draw = 0; draw < config.draws; draw++) pass.draw(3);
          }
          pass.end();
        }
        device.queue.submit([encoder.finish()]);
        const submitted = performance.now();
        const done = device.queue.onSubmittedWorkDone();
        if (await within(done, slotMs - (submitted - configStart)) === TIMED_OUT) {
          stopped = "frame_timeout";
          timedOutFrame = frame;
          pending = done;
          break;
        }
        (frame >= WARMUP ? latencies : warmupLatencies).push(performance.now() - frameStart);
        if (frame >= WARMUP) cpu.push(submitted - frameStart);
      }
      const report = {
        name: config.name,
        width: config.width,
        height: config.height,
        samples: config.samples,
        draws: config.draws,
        submits_per_frame: config.uniformSubmit ? 2 : 1,
        frames: latencies.length,
        frames_planned: config.frames,
        stopped,
        slot_ms: round(slotMs),
        warmup_ms: warmupLatencies.map(round),
        median_ms: median(latencies),
        max_ms: latencies.length ? round(Math.max(...latencies)) : null,
        cpu_median_ms: median(cpu),
        config_ms: round(performance.now() - configStart),
      };
      if (timedOutFrame !== null) report.timed_out_frame = timedOutFrame;
      if (latencies.length >= 20) {
        report.first10_median_ms = median(latencies.slice(0, 10));
        report.last10_median_ms = median(latencies.slice(-10));
      }
      reports.push(report);
      result.step = `${config.name}:drain`;
      if (pending) await drain(report);
      // Destroyed only once its last frame completed (or is known still busy).
      msaa?.destroy();
      assertDevice();
    }
    const incomplete = reports
      .filter((report) => report.skipped || report.stopped !== "complete")
      .map((report) => report.name);
    if (upload.timed_out_ms) incomplete.unshift("upload");
    finish(incomplete.length === 0 ? "passed" : "partial", {
      format,
      upload,
      warmup_frames: WARMUP,
      budget_ms: BUDGET_MS,
      budget_exhausted: budgetExhausted,
      incomplete,
      queue_pending_at_end: pending !== null,
      total_ms: round(elapsed()),
      configs: reports,
    });
  })().catch((error) => finish("failed", { step: result.step, error: String(error?.stack ?? error) }));
})();
