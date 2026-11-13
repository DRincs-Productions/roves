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
//   - Web Audio: decodeAudioData of PCM WAV and compressed MP3, Vorbis, Opus
//     and AAC fixtures; decoded samples are checked for non-silence;
//   - <audio>/<video>: element brand, play/pause/load presence, `src` support,
//     canPlayType answers and, only where `src` exists, the WAV's
//     metadata outcome plus muted compressed-audio play/pause/resume/end events;
//     video playback remains untested without an authored video fixture. Web
//     Audio decode support is never reported as HTML media playback.
//
// Output: `globalThis.__rovesMediaMatrix = { done, report, error }` and the
// promise `globalThis.__rovesMediaMatrixRun` resolving to the report object.
// `report.baselineFailures` lists violations of the shared Chrome contract:
// raster fixtures through <img>/createImageBitmap, SVG through <img> (Chrome
// rejects SVG Blob in createImageBitmap), invalid image rejection and WAV
// decoded to the source samples. MP3, Vorbis and AAC decode are also required;
// codec gaps such as Symphonia's current missing Opus decoder are reported.
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

  // Short authored audio vectors generated from a 440 Hz sine wave with ffmpeg
  // 8.0.1. They exercise compressed containers with the existing Symphonia decoder.
  const COMPRESSED_AUDIO_FIXTURES = [
    { id: "mp3_mpeg_1", mime: "audio/mpeg", bytes: fromBase64("//sUxAAABAwVTTTAgDiVhmjDNiAAAABZsAABuI5/YkAgAIBwmPh4eHngAAAIHZh7/+AZ/+HiEIMreEqTK4dRcyKXAUy6Qku+Alf2vP9z+eFAfAQa4VO5URHvrVVjAUEg//sUxAKDxEwdJh3QACCXA6LBv+RNECZhAD5kJaZ+rgBpmSxjKIAQDaE8wKAcwAAFEmQmMCZlYsacWnVvRhEAa2aQ8ImGEsAO5gVYAQfQgIYEDYJYMoB+SqpkC6wbW0AA//sUxAOABQwfVbWBgDCRiKhDNnAAAO/D8cZQIFAibT4uwZYM93NI1EZ/MH3xOD4PghD8HDnDAscG2xT7T/+RUaB0tiP+txp97P/Dwwa2JZMgq+TBzh8E/lD4DkxBTUUz//sUxAKDwAABpBwAACAAADSAAAAELjEwMKqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"), baseline: true },
    { id: "ogg_vorbis", mime: "audio/ogg", bytes: fromBase64("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Y5l93VlmX3eO4Rg6v/Djqaqvm64rDKcsC7/t68az+76yjK7r+6osC78q28Kx677z/L6wLKPs+sJqy8Kw2rYx3L5uLL9wHMtr68ox675RtnV8X3gKw/N0dV15Zl3H9nV040c4fsoAAIABBwCAABPKQKEhKwKAOAEAjySJomRZoihZliiKpui6omi6rqRppqlpnmlammeapmmqsimarixpmmlanmaamqeZpmiarmuapqyKpinLpmrKsmmasuy6sm27rmzbomnKsmmasmyapiy7sqvbruzquqRZpql5nmlqnmeapmrKsmmarqt5nmp6nmiqniiqqmqqqq2qqixbnmeamuippieKqmqqpq2aqirLpqrasmmqtmyqqm27quz6sm3rummqsm2qpi2bqmrbruzqsizbui9pmmlqnmeamueZpmmasmyaqitbnqeaniiqquaJpmqqqiybpqrKlueZqieKquqJnmuaqirLpmraqmmatmyqqi2bpirLrm37vuvKsm6qqmybqmrrpmrKsmzLvu/Kqu6KpinLpqrasmmqsi3bsu/Lsqz7omnKsmmqsm2qqi7Lsm0bs2z7umiasm2qpi2bqirbsi37uizbuu/Krm+rqqzrsi37uu76rnDrujC8smz7qqz6uivbum/rMtv2fUTTlGVTNW3bVFVZdmXZ9mXb9n3RNG1bVVVbNk3VtmVZ9n1Ztm1hNE3ZNlVV1k3VtG1Zlm1htmXhdmXZt2Vb9nXXlXVf133j12Xd5rqy7cuyrfuqq/q27vvCcOuu8AoAABhwAAAIMKEMFBqyEgCIAgAAjGGMMQiNUs45B6FRyjnnIGTOQQghlcw5CCGUkjkHoZSUMucglJJSCKGUlFoLIZSUUmsFAAAUOAAABNigKbE4QKEhKwGAVAAAg+NYlueZomrasmNJnieKqqmqtu1IlueJommqqm1bnieKpqmqruvrmueJommqquvqumiapqmqruu6ui6aoqmqquu6sq6bpqqqriu7suzrpqqqquvKriz7wqq6rivLsm3rwrCqruvKsmzbtm/cuq7rvu/7wpGt67ou/MIxDEcBAOAJDgBABTasjnBSNBZYaMhKACADAIAwBiGDEEIGIYSQUkohpZQSAAAw4AAAEGBCGSg0ZCUAEAMAABABIYMQQgghhBBCCCGEEEIIIYQQQuecc84555xzzgkA2I9wAJB6MDExhYWGrAQAUgEAAGOUUoox5yBEjDnGGHQSSooYc44xB6WkVDkHIYRUWsutcg5CCCm1VFvmnJTWYow5xsw5KSnFVnPOoZTUYqy55po7Ka3VmmvNuZbWas0151xzLq3FmmvONefccsw155xzzjnGnHPOOeeccwEAOA0OAKAHNqyOcFI0FlhoyEoAIBUAgEBGKcaccw46hBRjzjkHIYRIIcaccw5CCBVjzjkHHYQQKsYccw5CCCFkzjkHIYQQQsicgw46CCGE0EEHIYQQQiilcxBCCCGEEkoIIYQQQgghhA5CCCGEEEIIIYQQQgihlBJCCCGEUEIoJRQAAFjgAAAQYMPqCCdFY4GFhqwEAIAAACCHJaiUM2GQY9BjQ5By1EyDEFNOdKaYk9pMxRRkDkQnnUSGWlC2l8wCAAAgCAAIMAEEBggKvhACYgwAQBAiM0RCYRUsMCiDBod5APAAESERACQmKNIuLqDLABd0cdeBEIIQhCAWB1BAAg5OuOGJNzzhBifoFJU6CAAAAAAAAwB4AAA4KICIiOYqLC4wMjQ2ODo8AgAAAABABQA+AACODyAiorkKiwuMDI0Njg6PAAAAAAAAAAAAICAgAAAAAAAQAAAAICBPZ2dTAARgCQAAAAAAAGgde/ECAAAAksb8vQQdNyqFdOf7b8/6rxKY5JAWW2wvvhgfjY9Go9FoNBqNBgAaOd4OXgXw59ldb/imipriGQBgjAEAAAAAAAAAgBXcFwF5vvOMEQAAiMc/+bPm+JaLEwG7vn463jg+MlcBqN97wIF3QqgpnhQAAFAAAAAAAJgZwIlBW8B+XhIAD6AAQCwAfsi9DNzp79z9/oVnWNQUr9yAhykbrCRhFQr2w/RTW9iPfe72CL99e/dUdVM4y5uu2qkz5WLsbuVuLSkeu6jdrbyWKcVjFyt3a6/FKRdjdyt3a0nx2MXK3cprmXIxtlt51pLSu6jdLa9lYo/tas+SGGfXdgtTqdkHexWm0gA4gw9MAEg0AA=="), baseline: true },
    { id: "webm_opus", mime: "audio/webm", bytes: fromBase64("GkXfo59ChoEBQveBAULygQRC84EIQoKEd2VibUKHgQRChYECGFOAZwEAAAAAAALmEU2bdLpNu4tTq4QVSalmU6yBoU27i1OrhBZUrmtTrIHWTbuMU6uEElTDZ1OsggFATbuMU6uEHFO7a1OsggLQ7AEAAAAAAABZAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAVSalmsCrXsYMPQkBNgIxMYXZmNjIuMy4xMDBXQYxMYXZmNjIuMy4xMDBEiYhATQAAAAAAABZUrmvlrgEAAAAAAABc14EBc8WImgNZmqNdNlKcgQAitZyDdW5kiIEAhoZBX09QVVNWqoNjLqBWu4QExLQAg4EC4ZGfgQG1iEDncAAAAAAAYmSBEGOik09wdXNIZWFkAQE4AYC7AAAAAAASVMNn/HNzn2PAgGfImUWjh0VOQ09ERVJEh4xMYXZmNjIuMy4xMDBzc9djwItjxYiaA1mao102UmfIokWjh0VOQ09ERVJEh5VMYXZjNjIuMTEuMTAwIGxpYm9wdXNnyKFFo4hEVVJBVElPTkSHkzAwOjAwOjAwLjA1ODAwMDAwMAAfQ7Z1QQnngQCj44EAAIB4gaddbJ6ZrAAACArgWtURnEQxFQV7PeZ+HLj5DVlZXyHQujqljnU4PBoYH7n68WrZ9dVwkU8LOBr+mEOwtUOq8Flr4vQ1Dg0AZcQNEuHhcmxBjTKROLLISJSA5aXqBKPGgQAVgHifZwHn/JVNTqoYpxnf5VxZ3/tuoTNl9ailSQ3kV3eHjwpC0JJGkf0f1DtwP/fVl6g4VoJSJXALupY8HA9PQfeJvaDXoc+BACkAeJtDUAd1Cx/1noG4UIEU5WOLiDsaovcyttZUtcepd2uvD7oi3IWtFtHWFWNbtaERdOY4nPZKMCe8gqVPx5StZDL8w4f8AplFlC4cdaKDNWfgHFO7a5G7j7OBALeK94EB8YIBwfCBAw==") },
    { id: "m4a_aac", mime: "audio/mp4", bytes: fromBase64("AAAAHGZ0eXBNNEEgAAACAE00QSBpc29taXNvMgAAAwptb292AAAAbG12aGQAAAAAAAAAAAAAAAAAAAPoAAAAMgABAAABAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAAACNXRyYWsAAABcdGtoZAAAAAMAAAAAAAAAAAAAAAEAAAAAAAAAMgAAAAAAAAAAAAAAAQEAAAAAAQAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAACRlZHRzAAAAHGVsc3QAAAAAAAAAAQAAADIAAAQAAAEAAAAAAa1tZGlhAAAAIG1kaGQAAAAAAAAAAAAAAAAAALuAAAANYFXEAAAAAAAtaGRscgAAAAAAAAAAc291bgAAAAAAAAAAAAAAAFNvdW5kSGFuZGxlcgAAAAFYbWluZgAAABBzbWhkAAAAAAAAAAAAAAAkZGluZgAAABxkcmVmAAAAAAAAAAEAAAAMdXJsIAAAAAEAAAEcc3RibAAAAGpzdHNkAAAAAAAAAAEAAABabXA0YQAAAAAAAAABAAAAAAAAAAAAAQAQAAAAALuAAAAAAAA2ZXNkcwAAAAADgICAJQABAASAgIAXQBUAAAAAAL1AAAC9QAWAgIAFEYhW5QAGgICAAQIAAAAgc3R0cwAAAAAAAAACAAAAAwAABAAAAAABAAABYAAAABxzdHNjAAAAAAAAAAEAAAABAAAABAAAAAEAAAAkc3RzegAAAAAAAAAAAAAABAAAAI4AAACfAAAAfgAAAAUAAAAUc3RjbwAAAAAAAAABAAADNgAAABpzZ3BkAQAAAHJvbGwAAAACAAAAAf//AAAAHHNiZ3AAAAAAcm9sbAAAAAEAAAAEAAAAAQAAAGF1ZHRhAAAAWW1ldGEAAAAAAAAAIWhkbHIAAAAAAAAAAG1kaXJhcHBsAAAAAAAAAAAAAAAALGlsc3QAAAAkqXRvbwAAABxkYXRhAAAAAQAAAABMYXZmNjIuMy4xMDAAAAAIZnJlZQAAAbhtZGF03gIATGF2YzYyLjExLjEwMAACaKNZoSiNNYdKmV9vinEeaXuSJDwiSf/+rkgTdMNzYrT1s210i6u4O2aO/De4sFbKmtMlIlXNtRnMtEpEpEiZKRKBgYGRAwMbNmwYGRIkRs2bNokSKWWWzcASouI3BEqpyNZNjXCM9tFXSM5dBWyM05KqHAaZKcmia5KWrgE0lNrJXZZLqyXUFOm/386ttauf9vH7+da41ev/4vX7+da41ev/7F/jzrWtasbU7NuueeYNZbLPcKL3+ygYN0D6UK+lA+n01mA+n0+lAH0+n01gfT6fTXQYAfT6fTWZsAa560WLVJ/Nt0iEMTPlEz5RM+XyiJg+Xy4xMHy+XEmD5fL5RREAPl8vlxiiAFlz1nAkJCQkJCQkJCQkJCQk5wE4lbLJXZRLnTZLqyXOlz+vXGnDq9f+k/Xri+Nar/6evbrV8avXj/X461etXsMfuXv6ao3lAwIyuJlfKhXyoE4+XyE1HTGU2XkDEzWTNZPfQgwfRsBg+n0AeZ9PoAOdg+n0+kgfHEYuojaY9Md0d0d0f6rh8Sn2n1PqfU+p/gEYgbRw"), baseline: true },
  ];

  // Authored 32x24, five-frame VP8/WebM clip from ffmpeg 8.0.1's testsrc2.
  const VIDEO_FIXTURES = [
    {
      id: "webm_vp8_32x24", mime: "video/webm", width: 32, height: 24,
      bytes: fromBase64("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"),
    },
  ];

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

  async function exerciseAudioPlayback(element) {
    if (typeof element.play !== "function" || typeof element.pause !== "function") {
      return { outcome: "untested", reason: "no HTMLMediaElement play/pause API" };
    }
    element.muted = true;
    const events = [];
    const loggers = [];
    for (const type of ["play", "playing", "pause", "seeked", "ended", "error"]) {
      const logger = () => events.push(type);
      loggers.push([type, logger]);
      element.addEventListener(type, logger);
    }
    const event = (type) => new Promise((resolve) => {
      element.addEventListener(type, () => resolve(type), { once: true });
      element.addEventListener("error", () => resolve("error"), { once: true });
    });
    try {
      const started = event("playing");
      await element.play();
      const firstPlaying = await timeout(started, TIMEOUT_MS, "timeout");
      if (firstPlaying !== "playing") return { outcome: firstPlaying, events };

      element.pause();
      const pausedAfterPause = element.paused;
      let seekOutcome = "unsupported";
      if (Number.isFinite(element.duration) && element.duration > 0) {
        const seeked = event("seeked");
        try {
          element.currentTime = element.duration / 2;
          seekOutcome = await timeout(seeked, TIMEOUT_MS, "timeout");
        } catch (error) {
          seekOutcome = `rejected: ${errorText(error)}`;
        }
      }
      const ended = event("ended");
      const resumed = event("playing");
      await element.play();
      const secondPlaying = await timeout(resumed, TIMEOUT_MS, "timeout");
      if (secondPlaying !== "playing") return { outcome: secondPlaying, pausedAfterPause, seekOutcome, events };
      const endOutcome = await timeout(ended, TIMEOUT_MS, "timeout");
      return { outcome: endOutcome === "ended" ? "played_to_end" : endOutcome, pausedAfterPause, seekOutcome, events };
    } catch (error) {
      return { outcome: "rejected", error: errorText(error), pausedAfterPause: element.paused, events };
    } finally {
      for (const [type, logger] of loggers) element.removeEventListener(type, logger);
      element.pause();
      element.muted = false;
    }
  }

  async function exerciseVideoPlayback(element, fixture) {
    const available = ["src", "load", "play", "pause"].every((name) =>
      name === "src" ? "src" in element : typeof element[name] === "function"
    );
    const report = {
      id: fixture.id,
      mime: fixture.mime,
      bytes: fixture.bytes.length,
      fnv1a: fnv1a(fixture.bytes),
    };
    if (!available) return { ...report, outcome: "untested", reason: "no HTMLMediaElement src/load/play/pause API" };

    element.muted = true;
    element.preload = "auto";
    const events = [];
    const loggers = [];
    for (const type of ["loadedmetadata", "loadeddata", "canplay", "play", "playing", "pause", "seeked", "ended", "error"]) {
      const logger = () => events.push(type);
      loggers.push([type, logger]);
      element.addEventListener(type, logger);
    }
    const event = (type) => new Promise((resolve) => {
      element.addEventListener(type, () => resolve(type), { once: true });
      element.addEventListener("error", () => resolve("error"), { once: true });
    });
    try {
      element.src = dataUrl(fixture);
      const metadata = await timeout(event("loadedmetadata"), TIMEOUT_MS, "timeout");
      if (metadata !== "loadedmetadata") return { ...report, outcome: metadata, events };

      const frame = typeof element.requestVideoFrameCallback === "function"
        ? new Promise((resolve) => element.requestVideoFrameCallback((_now, metadata) => resolve({
          presentedFrames: metadata.presentedFrames,
          mediaTime: Number(metadata.mediaTime.toFixed(4)),
        })))
        : Promise.resolve("unavailable");
      const firstPlaying = event("playing");
      await element.play();
      const firstPlayingOutcome = await timeout(firstPlaying, TIMEOUT_MS, "timeout");
      if (firstPlayingOutcome !== "playing") return { ...report, outcome: firstPlayingOutcome, events };
      const firstFrame = await timeout(frame, TIMEOUT_MS, "timeout");

      element.pause();
      const pausedAfterPause = element.paused;
      const seeked = event("seeked");
      element.currentTime = Math.min(element.duration / 2, 0.2);
      const seekOutcome = await timeout(seeked, TIMEOUT_MS, "timeout");
      const ended = event("ended");
      const resumed = event("playing");
      await element.play();
      const resumeOutcome = await timeout(resumed, TIMEOUT_MS, "timeout");
      const endOutcome = resumeOutcome === "playing" ? await timeout(ended, TIMEOUT_MS, "timeout") : resumeOutcome;
      return {
        ...report,
        outcome: endOutcome === "ended" ? "played_to_end" : endOutcome,
        metadata,
        dimensions: [element.videoWidth, element.videoHeight],
        duration: Number.isFinite(element.duration) ? Number(element.duration.toFixed(4)) : String(element.duration),
        firstFrame,
        pausedAfterPause,
        seekOutcome,
        events,
      };
    } catch (error) {
      return { ...report, outcome: "rejected", error: errorText(error), events };
    } finally {
      for (const [type, logger] of loggers) element.removeEventListener(type, logger);
      element.pause();
      element.removeAttribute("src");
      element.load();
      element.muted = false;
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
      result.compressed = [];
      for (const fixture of COMPRESSED_AUDIO_FIXTURES) {
        try {
          const buffer = await timeout(context.decodeAudioData(fixture.bytes.slice().buffer), TIMEOUT_MS, null);
          if (!buffer) {
            result.compressed.push({ id: fixture.id, mime: fixture.mime, bytes: fixture.bytes.length, fnv1a: fnv1a(fixture.bytes), outcome: "timeout" });
            continue;
          }
          const channel = buffer.getChannelData(0);
          let nonzero = 0;
          let peak = 0;
          for (const sample of channel) {
            if (sample !== 0) nonzero++;
            peak = Math.max(peak, Math.abs(sample));
          }
          result.compressed.push({
            id: fixture.id,
            mime: fixture.mime,
            bytes: fixture.bytes.length,
            fnv1a: fnv1a(fixture.bytes),
            outcome: "decoded",
            frames: buffer.length,
            sampleRate: buffer.sampleRate,
            channels: buffer.numberOfChannels,
            nonzeroSamples: nonzero,
            peak: Number(peak.toFixed(6)),
          });
        } catch (error) {
          result.compressed.push({ id: fixture.id, mime: fixture.mime, bytes: fixture.bytes.length, fnv1a: fnv1a(fixture.bytes), outcome: "rejected", error: errorText(error) });
        }
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
        result.compressedMetadata = [];
        for (const fixture of COMPRESSED_AUDIO_FIXTURES) {
          const metadata = new Promise((resolve) => {
            element.addEventListener("loadedmetadata", () => resolve("loadedmetadata"), { once: true });
            element.addEventListener("error", () => resolve("error"), { once: true });
          });
          element.preload = "metadata";
          element.src = dataUrl(fixture);
          const codecOutcome = await timeout(metadata, TIMEOUT_MS, "timeout");
          result.compressedMetadata.push({
            id: fixture.id,
            mime: fixture.mime,
            bytes: fixture.bytes.length,
            fnv1a: fnv1a(fixture.bytes),
            outcome: codecOutcome,
            duration: Number.isFinite(element.duration) ? Number(element.duration.toFixed(4)) : String(element.duration),
            readyState: element.readyState,
            errorCode: element.error?.code ?? null,
          });
          result.compressedMetadata.at(-1).playback = codecOutcome === "loadedmetadata"
            ? await exerciseAudioPlayback(element)
            : { outcome: "skipped", reason: `metadata ${codecOutcome}` };
          element.removeAttribute("src");
          element.load();
        }
      }
    } else {
      result.videoFixture = await exerciseVideoPlayback(element, VIDEO_FIXTURES[0]);
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
    for (const fixture of COMPRESSED_AUDIO_FIXTURES.filter((entry) => entry.baseline)) {
      const decoded = audio.compressed?.find((entry) => entry.id === fixture.id);
      if (decoded?.outcome !== "decoded" || !(decoded.nonzeroSamples > 0)) {
        baselineFailures.push(`${fixture.id} decode: ${JSON.stringify(decoded)}`);
      }
    }
    if (audio.invalid?.outcome !== "rejected") baselineFailures.push(`invalid audio: ${JSON.stringify(audio.invalid)}`);
    for (const decoded of audio.compressed ?? []) {
      if (decoded.outcome !== "decoded" || !(decoded.nonzeroSamples > 0)) {
        gaps.push(`Web Audio ${decoded.id}: ${decoded.outcome}${decoded.error ? ` (${decoded.error})` : ""}`);
      }
    }

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
