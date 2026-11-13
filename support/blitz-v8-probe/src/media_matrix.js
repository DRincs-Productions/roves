// Shared media-compatibility fixture matrix (test-only harness, no media
// implementation). The SAME file runs in Chrome (DevTools `evaluate_script`) and
// in the Blitz probe fixture stage (`media_matrix`, normal and JITless), so both
// engines see byte-identical fixtures and report the same JSON shape.
//
// Every fixture is generated here, deterministically, from authored bytes: no
// external media file and no licensing question. Each fixture's byte length and
// FNV-1a hash are in the report, so two reports can be checked to have tested
// the same bytes.
//
// What is measured (real outcomes, next to capability answers):
//   - <img>: load/error/timeout, naturalWidth/naturalHeight, currentSrc;
//   - createImageBitmap(Blob): decoded/rejected and dimensions;
//   - animated GIF: pixel change of the <img> over time, only where a 2D canvas
//     can draw the element (otherwise reported "untested", never "static");
//   - Web Audio: decodeAudioData of a PCM WAV, decoded samples compared with
//     the source; an invalid buffer must be rejected;
//   - <audio>/<video>: element brand, play/pause/load presence, `src` support,
//     canPlayType answers and, only where `src` exists, the WAV's
//     loadedmetadata/error outcome. Nothing is ever played (play() is not
//     called: autoplay policy makes it non-deterministic), and Web Audio decode
//     support is never reported as HTML media playback.
//
// Output: `globalThis.__rovesMediaMatrix = { done, report, error }` and the
// promise `globalThis.__rovesMediaMatrixRun` resolving to the report object.
// `report.baselineFailures` lists violations of the shared Chrome contract:
// raster fixtures through <img>/createImageBitmap, SVG through <img> (Chrome
// rejects SVG Blob in createImageBitmap), invalid image rejection and WAV
// decoded to the source samples. Other gaps are reported, not asserted.
(() => {
  "use strict";
  const state = { done: false, report: null, error: null };
  globalThis.__rovesMediaMatrix = state;

  const TIMEOUT_MS = 5000;

  // ---- byte helpers ---------------------------------------------------------
  const concat = (parts) => {
    const length = parts.reduce((sum, part) => sum + part.length, 0);
    const out = new Uint8Array(length);
    let offset = 0;
    for (const part of parts) {
      out.set(part, offset);
      offset += part.length;
    }
    return out;
  };
  const ascii = (text) => Uint8Array.from(text, (c) => c.charCodeAt(0));
  const be32 = (value) => Uint8Array.of(value >>> 24, (value >>> 16) & 255, (value >>> 8) & 255, value & 255);
  const le16 = (value) => Uint8Array.of(value & 255, (value >>> 8) & 255);
  const le32 = (value) => Uint8Array.of(value & 255, (value >>> 8) & 255, (value >>> 16) & 255, value >>> 24);
  const base64 = (bytes) => {
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary);
  };
  const fromBase64 = (text) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
  const fnv1a = (bytes) => {
    let hash = 0x811c9dc5;
    for (const byte of bytes) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
    return hash.toString(16).padStart(8, "0");
  };

  // ---- PNG: RGBA8, zlib with one stored (uncompressed) deflate block ---------
  const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc32 = (bytes) => {
    let c = 0xffffffff;
    for (const byte of bytes) c = CRC_TABLE[(c ^ byte) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const adler32 = (bytes) => {
    let a = 1;
    let b = 0;
    for (const byte of bytes) {
      a = (a + byte) % 65521;
      b = (b + a) % 65521;
    }
    return ((b << 16) | a) >>> 0;
  };
  const pngChunk = (type, data) => {
    const body = concat([ascii(type), data]);
    return concat([be32(data.length), body, be32(crc32(body))]);
  };
  function png(width, height, rgba) {
    const rows = [];
    for (let y = 0; y < height; y++) {
      rows.push(Uint8Array.of(0), rgba.subarray(y * width * 4, (y + 1) * width * 4));
    }
    const raw = concat(rows); // < 65536 bytes: one stored block
    const zlib = concat([
      Uint8Array.of(0x78, 0x01, 0x01),
      le16(raw.length),
      le16(~raw.length & 0xffff),
      raw,
      be32(adler32(raw)),
    ]);
    const ihdr = concat([be32(width), be32(height), Uint8Array.of(8, 6, 0, 0, 0)]);
    return concat([
      Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a),
      pngChunk("IHDR", ihdr),
      pngChunk("IDAT", zlib),
      pngChunk("IEND", new Uint8Array(0)),
    ]);
  }

  // ---- GIF89a: 4-colour palette; LZW emits a clear code before every pixel so
  // the code size stays 3 bits and no dictionary entry is ever used ------------
  function gifLzw(indices) {
    const codes = [];
    for (const index of indices) codes.push(4, index);
    codes.push(5);
    const out = [];
    let bits = 0;
    let count = 0;
    for (const code of codes) {
      bits |= code << count;
      count += 3;
      while (count >= 8) {
        out.push(bits & 255);
        bits >>>= 8;
        count -= 8;
      }
    }
    if (count > 0) out.push(bits & 255);
    const blocks = [Uint8Array.of(2)]; // LZW minimum code size
    for (let i = 0; i < out.length; i += 255) {
      const block = out.slice(i, i + 255);
      blocks.push(Uint8Array.of(block.length), Uint8Array.from(block));
    }
    blocks.push(Uint8Array.of(0));
    return concat(blocks);
  }
  function gif(width, height, palette, frames, { loop = false } = {}) {
    const parts = [
      ascii("GIF89a"),
      le16(width),
      le16(height),
      Uint8Array.of(0xf1, 0, 0), // global colour table, 4 entries
      Uint8Array.from(palette.flat()),
    ];
    if (loop) {
      parts.push(Uint8Array.of(0x21, 0xff, 0x0b), ascii("NETSCAPE2.0"), Uint8Array.of(3, 1, 0, 0, 0));
    }
    for (const frame of frames) {
      if (frame.delay !== undefined) {
        // Graphic control: disposal 1 (do not dispose), delay in 1/100 s.
        parts.push(Uint8Array.of(0x21, 0xf9, 4, 0x04), le16(frame.delay), Uint8Array.of(0, 0));
      }
      parts.push(Uint8Array.of(0x2c), le16(0), le16(0), le16(width), le16(height), Uint8Array.of(0));
      parts.push(gifLzw(frame.indices));
    }
    parts.push(Uint8Array.of(0x3b));
    return concat(parts);
  }

  // ---- JPEG: baseline, 16x8 grayscale, uniform value 144 ----------------------
  // Quantization all 1; DC Huffman {0:"00", 8:"01"}, AC {EOB:"0"}. Block 1 has
  // DC 128 (category 8), block 2 a zero difference; IDCT gives 128 + 128/8 = 144.
  function jpeg() {
    const segment = (marker, body) => concat([Uint8Array.of(0xff, marker), be16(body.length + 2), body]);
    const be16 = (value) => Uint8Array.of(value >>> 8, value & 255);
    const counts = (lengths) => Uint8Array.from({ length: 16 }, (_, i) => lengths[i + 1] ?? 0);
    return concat([
      Uint8Array.of(0xff, 0xd8),
      segment(0xdb, concat([Uint8Array.of(0), new Uint8Array(64).fill(1)])),
      segment(0xc0, Uint8Array.of(8, 0, 8, 0, 16, 1, 1, 0x11, 0)),
      segment(0xc4, concat([Uint8Array.of(0x00), counts({ 2: 2 }), Uint8Array.of(0, 8)])),
      segment(0xc4, concat([Uint8Array.of(0x10), counts({ 1: 1 }), Uint8Array.of(0)])),
      segment(0xda, Uint8Array.of(1, 1, 0x00, 0, 63, 0)),
      // 01 10000000 0 | 00 0 | padding 11
      Uint8Array.of(0x60, 0x03),
      Uint8Array.of(0xff, 0xd9),
    ]);
  }

  // ---- WAV: 16-bit PCM mono at the decoding context's rate (no resampling) ----
  const wavSamples = () => Int16Array.from({ length: 256 }, (_, i) => ((i % 16) < 8 ? 1 : -1) * (8192 + 512 * (i % 8)));
  function wav(samples, rate) {
    const data = new Uint8Array(samples.buffer.slice(0));
    return concat([
      ascii("RIFF"), le32(36 + data.length), ascii("WAVEfmt "),
      le32(16), le16(1), le16(1), le32(rate), le32(rate * 2), le16(2), le16(16),
      ascii("data"), le32(data.length), data,
    ]);
  }

  // ---- image fixtures ---------------------------------------------------------
  const PNG_PIXELS = Uint8Array.of(
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
    255, 255, 255, 255, 10, 20, 30, 128, 0, 0, 0, 0,
  );
  const PALETTE = [[255, 0, 0], [0, 0, 255], [0, 255, 0], [255, 255, 255]];
  const SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><rect width="5" height="3" fill="#00aa00"/></svg>';
  // 1x1 WebP detection vectors from Google's "WebP FAQ" (lossy VP8, lossless
  // VP8L, and a one-frame animated container); not media content.
  const IMAGE_FIXTURES = [
    { id: "png_rgba_3x2", mime: "image/png", bytes: png(3, 2, PNG_PIXELS), width: 3, height: 2, baseline: true },
    { id: "jpeg_gray_16x8", mime: "image/jpeg", bytes: jpeg(), width: 16, height: 8, baseline: true },
    { id: "gif_static_3x2", mime: "image/gif", bytes: gif(3, 2, PALETTE, [{ indices: [0, 1, 2, 3, 2, 1] }]), width: 3, height: 2, baseline: true },
    {
      id: "gif_animated_2x2", mime: "image/gif", width: 2, height: 2, animated: true, baseline: true,
      bytes: gif(2, 2, PALETTE, [{ indices: [0, 0, 0, 0], delay: 10 }, { indices: [1, 1, 1, 1], delay: 10 }], { loop: true }),
    },
    { id: "webp_lossy_1x1", mime: "image/webp", bytes: fromBase64("UklGRiIAAABXRUJQVlA4IBYAAAAwAQCdASoBAAEADsD+JaQAA3AAAAAA"), width: 1, height: 1, baseline: true },
    { id: "webp_lossless_1x1", mime: "image/webp", bytes: fromBase64("UklGRhoAAABXRUJQVlA4TA0AAAAvAAAAEAcQERGIiP4HAA=="), width: 1, height: 1, baseline: true },
    {
      id: "webp_animated_container_1x1", mime: "image/webp", width: 1, height: 1,
      bytes: fromBase64("UklGRlIAAABXRUJQVlA4WAoAAAASAAAAAAAAAAAAQU5JTQYAAAD/////AABBTk1GJgAAAAAAAAAAAAAAAAAAAGQAAABWUDhMDQAAAC8AAAAQBxAREYiI/gcA"), baseline: true,
      note: "single frame: decode only, no animation check",
    },
    { id: "svg_5x3", mime: "image/svg+xml", bytes: ascii(SVG), width: 5, height: 3, baseline: true, bitmapExpected: "rejected" },
    { id: "invalid_png_bytes", mime: "image/png", bytes: ascii("not an image"), width: 0, height: 0, invalid: true, baseline: true, bitmapExpected: "rejected" },
  ];
  const OMITTED = [
    { format: "image/avif", reason: "no small AVIF fixture could be authored and verified without an external encoder; not tested" },
    { format: "video/*", reason: "no small authored video fixture; <video> reports capabilities only" },
  ];
  const CAN_PLAY_TYPES = {
    audio: [
      "audio/wav", 'audio/wav; codecs="1"', "audio/mpeg", 'audio/ogg; codecs="vorbis"', 'audio/ogg; codecs="opus"',
      'audio/webm; codecs="opus"', 'audio/mp4; codecs="mp4a.40.2"', "audio/aac", "audio/flac",
    ],
    video: [
      'video/mp4; codecs="avc1.42E01E"', 'video/mp4; codecs="av01.0.05M.08"', 'video/webm; codecs="vp8"',
      'video/webm; codecs="vp9"', 'video/ogg; codecs="theora"',
    ],
  };

  // ---- runners ----------------------------------------------------------------
  const errorText = (error) => (error && typeof error === "object" ? `${error.name}: ${error.message}` : String(error));
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const timeout = (promise, ms, value) => Promise.race([promise, sleep(ms).then(() => value)]);
  const dataUrl = (fixture) => `data:${fixture.mime};base64,${base64(fixture.bytes)}`;

  async function imageElement(fixture) {
    if (typeof Image !== "function") return { outcome: "unavailable" };
    const url = dataUrl(fixture);
    const img = new Image();
    const settled = new Promise((resolve) => {
      img.onload = () => resolve("load");
      img.onerror = () => resolve("error");
    });
    img.src = url;
    const outcome = await timeout(settled, TIMEOUT_MS, "timeout");
    return {
      outcome,
      naturalWidth: img.naturalWidth,
      naturalHeight: img.naturalHeight,
      complete: img.complete,
      currentSrcIsFixture: img.currentSrc === url,
      currentSrcLength: img.currentSrc.length,
      dimensionsMatch: img.naturalWidth === fixture.width && img.naturalHeight === fixture.height,
    };
  }

  async function imageBitmap(fixture) {
    if (typeof createImageBitmap !== "function") return { outcome: "unavailable" };
    try {
      const bitmap = await timeout(createImageBitmap(new Blob([fixture.bytes], { type: fixture.mime })), TIMEOUT_MS, null);
      if (!bitmap) return { outcome: "timeout" };
      const result = {
        outcome: "decoded",
        width: bitmap.width,
        height: bitmap.height,
        dimensionsMatch: bitmap.width === fixture.width && bitmap.height === fixture.height,
      };
      bitmap.close?.();
      return result;
    } catch (error) {
      return { outcome: "rejected", error: errorText(error) };
    }
  }

  // Draws the <img> into a 2D canvas over ~1 s and records the distinct colours
  // of pixel (0,0). Needs drawImage(HTMLImageElement); otherwise "untested".
  async function animation(fixture) {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext?.("2d");
    if (!context || typeof context.drawImage !== "function") {
      return { outcome: "untested", reason: "no CanvasRenderingContext2D.drawImage to sample <img> frames" };
    }
    if (!document.body) return { outcome: "untested", reason: "no document.body to render the <img>" };
    canvas.width = fixture.width;
    canvas.height = fixture.height;
    const img = new Image();
    img.style.cssText = "position:fixed;left:0;top:0;width:8px;height:8px";
    const loaded = new Promise((resolve) => {
      img.onload = () => resolve(true);
      img.onerror = () => resolve(false);
    });
    img.src = dataUrl(fixture);
    document.body.appendChild(img);
    try {
      if (!(await timeout(loaded, TIMEOUT_MS, false))) return { outcome: "untested", reason: "the <img> did not load" };
      const colors = [];
      for (let i = 0; i < 25; i++) {
        context.clearRect(0, 0, canvas.width, canvas.height);
        context.drawImage(img, 0, 0);
        const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
        const color = `${r},${g},${b},${a}`;
        if (colors.at(-1) !== color) colors.push(color);
        await sleep(40);
      }
      const distinct = new Set(colors).size;
      return distinct > 1
        ? { outcome: "pixels_changed", colorSequence: colors.slice(0, 8) }
        : {
          outcome: "no_change_observed",
          colorSequence: colors,
          visibilityState: document.visibilityState,
          reason: "inconclusive: a hidden or throttled page may not advance image animation",
        };
    } finally {
      img.remove();
    }
  }

  async function webAudio() {
    const Context = globalThis.AudioContext ?? globalThis.webkitAudioContext;
    const result = {
      AudioContext: typeof Context === "function",
      OfflineAudioContext: typeof globalThis.OfflineAudioContext === "function",
    };
    if (!result.AudioContext) return { ...result, wav: { outcome: "unavailable" } };
    // A context is created but never started by a gesture: decoding does not
    // depend on its state, and nothing is played.
    const context = new Context();
    try {
      result.contextSampleRate = context.sampleRate;
      const samples = wavSamples();
      const bytes = wav(samples, context.sampleRate);
      result.wavFixture = { bytes: bytes.length, fnv1a: fnv1a(bytes), sampleRate: context.sampleRate, frames: samples.length };
      try {
        const buffer = await timeout(context.decodeAudioData(bytes.slice().buffer), TIMEOUT_MS, null);
        if (!buffer) {
          result.wav = { outcome: "timeout" };
        } else {
          const channel = buffer.getChannelData(0);
          let nonzero = 0;
          let peak = 0;
          let maxError = 0;
          for (let i = 0; i < channel.length; i++) {
            if (channel[i] !== 0) nonzero++;
            peak = Math.max(peak, Math.abs(channel[i]));
            if (i < samples.length) maxError = Math.max(maxError, Math.abs(channel[i] - samples[i] / 32768));
          }
          result.wav = {
            outcome: "decoded",
            length: buffer.length,
            sampleRate: buffer.sampleRate,
            numberOfChannels: buffer.numberOfChannels,
            nonzeroSamples: nonzero,
            peak: Number(peak.toFixed(6)),
            matchesSource: buffer.length === samples.length && maxError < 1e-4,
          };
        }
      } catch (error) {
        result.wav = { outcome: "rejected", error: errorText(error) };
      }
      try {
        await timeout(context.decodeAudioData(ascii("not audio").buffer), TIMEOUT_MS, null);
        result.invalid = { outcome: "decoded_unexpectedly" };
      } catch (error) {
        result.invalid = { outcome: "rejected", errorName: error?.name ?? String(error) };
      }
    } finally {
      await context.close?.().catch(() => {});
    }
    return result;
  }

  async function mediaElement(kind) {
    const element = document.createElement(kind);
    const prototype = Object.getPrototypeOf(element);
    const result = {
      element: prototype?.constructor?.name ?? null,
      isHTMLMediaElement: typeof HTMLMediaElement === "function" && element instanceof HTMLMediaElement,
      methods: Object.fromEntries(["play", "pause", "load", "canPlayType"].map((name) => [name, typeof element[name] === "function"])),
      srcProperty: "src" in element,
      canPlayType: {},
      playback: "not_attempted",
    };
    for (const type of CAN_PLAY_TYPES[kind]) {
      try {
        result.canPlayType[type] = typeof element.canPlayType === "function" ? element.canPlayType(type) : null;
      } catch (error) {
        result.canPlayType[type] = `threw ${errorText(error)}`;
      }
    }
    if (kind === "audio") {
      // Metadata load of the same WAV fixture: an outcome, not playback.
      if (!result.srcProperty || !result.methods.load) {
        result.wavMetadata = { outcome: "untested", reason: "no HTMLMediaElement src/load API" };
      } else {
        const bytes = wav(wavSamples(), 44100);
        const settled = new Promise((resolve) => {
          element.addEventListener("loadedmetadata", () => resolve("loadedmetadata"), { once: true });
          element.addEventListener("error", () => resolve("error"), { once: true });
        });
        element.preload = "metadata";
        element.src = `data:audio/wav;base64,${base64(bytes)}`;
        const outcome = await timeout(settled, TIMEOUT_MS, "timeout");
        result.wavMetadata = {
          outcome,
          duration: Number.isFinite(element.duration) ? Number(element.duration.toFixed(4)) : String(element.duration),
          expectedDuration: Number((256 / 44100).toFixed(4)),
          readyState: element.readyState,
          errorCode: element.error?.code ?? null,
        };
        element.removeAttribute("src");
        element.load();
      }
    } else {
      result.fixture = "none";
    }
    return result;
  }

  async function run() {
    const started = performance.now();
    const images = [];
    for (const fixture of IMAGE_FIXTURES) {
      images.push({
        id: fixture.id,
        mime: fixture.mime,
        bytes: fixture.bytes.length,
        fnv1a: fnv1a(fixture.bytes),
        expected: fixture.invalid ? "error" : `${fixture.width}x${fixture.height}`,
        ...(fixture.note ? { note: fixture.note } : {}),
        img: await imageElement(fixture),
        createImageBitmap: await imageBitmap(fixture),
      });
    }
    const animations = [];
    for (const fixture of IMAGE_FIXTURES.filter((f) => f.animated)) {
      animations.push({ id: fixture.id, ...(await animation(fixture)) });
    }
    const audio = await webAudio();
    const htmlMedia = { audio: await mediaElement("audio"), video: await mediaElement("video") };

    const gaps = [];
    for (const [kind, media] of Object.entries(htmlMedia)) {
      if (!media.srcProperty || !media.methods.play || !media.methods.pause || !media.methods.load) {
        gaps.push(`<${kind}>: no src/play/pause/load playback API (capability query only)`);
      }
      if (Object.values(media.canPlayType).every((answer) => answer === "")) {
        gaps.push(`<${kind}>: canPlayType answers "" for every tested type`);
      }
    }
    if (audio.wav?.outcome === "decoded" && htmlMedia.audio.wavMetadata?.outcome !== "loadedmetadata") {
      gaps.push("Web Audio decodes the WAV fixture but <audio> does not load it: Web Audio support is not HTML media playback");
    }
    for (const image of images) {
      if (image.expected !== "error" && image.img.outcome !== "load") gaps.push(`<img> ${image.id}: ${image.img.outcome}`);
      const bitmapExpected = IMAGE_FIXTURES.find((fixture) => fixture.id === image.id)?.bitmapExpected ?? "decoded";
      if (image.createImageBitmap.outcome !== bitmapExpected) {
        gaps.push(`createImageBitmap ${image.id}: ${image.createImageBitmap.outcome} (expected ${bitmapExpected})`);
      }
    }
    for (const entry of animations) {
      if (entry.outcome !== "pixels_changed") gaps.push(`animation ${entry.id}: ${entry.outcome}`);
    }

    const baselineFailures = [];
    for (const fixture of IMAGE_FIXTURES.filter((f) => f.baseline)) {
      const image = images.find((entry) => entry.id === fixture.id);
      if (fixture.invalid) {
        if (image.img.outcome !== "error") baselineFailures.push(`${fixture.id}: <img> ${image.img.outcome}, expected error`);
        if (image.createImageBitmap.outcome !== "rejected") {
          baselineFailures.push(`${fixture.id}: createImageBitmap ${image.createImageBitmap.outcome}, expected rejected`);
        }
      } else {
        if (image.img.outcome !== "load" || !image.img.dimensionsMatch || !image.img.currentSrcIsFixture) {
          baselineFailures.push(`${fixture.id}: <img> ${JSON.stringify(image.img)}`);
        }
        const bitmapExpected = fixture.bitmapExpected ?? "decoded";
        if (image.createImageBitmap.outcome !== bitmapExpected ||
          (bitmapExpected === "decoded" && !image.createImageBitmap.dimensionsMatch)) {
          baselineFailures.push(`${fixture.id}: createImageBitmap ${JSON.stringify(image.createImageBitmap)}`);
        }
      }
    }
    if (audio.wav?.outcome !== "decoded" || !audio.wav.matchesSource || !(audio.wav.nonzeroSamples > 0)) {
      baselineFailures.push(`wav decode: ${JSON.stringify(audio.wav)}`);
    }
    if (audio.invalid?.outcome !== "rejected") baselineFailures.push(`invalid audio: ${JSON.stringify(audio.invalid)}`);

    return {
      schema: 1,
      harness: "roves-media-matrix",
      userAgent: globalThis.navigator?.userAgent ?? null,
      durationMs: Math.round(performance.now() - started),
      images,
      animations,
      webAudio: audio,
      htmlMedia,
      omitted: OMITTED,
      gaps,
      baselineFailures,
    };
  }

  globalThis.__rovesMediaMatrixRun = run().then(
    (report) => {
      state.report = report;
      state.done = true;
      return report;
    },
    (error) => {
      state.error = errorText(error) + (error?.stack ? `\n${error.stack}` : "");
      state.done = true;
      throw error;
    },
  );
})();
