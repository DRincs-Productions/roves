// Tone.js 15.1.22 (through standardized-audio-context 25.3.77, as in
// visual-novel-template) on the probe's Web Audio: src/web_audio.js over
// web-audio-api 1.7.0. The frame loop drives it; src/main.rs reads the result.
import * as Tone from "tone";
// PixiJS 8.19.0, as in visual-novel-template: only its Assets loader is used,
// to check the parser it selects for the template's `.wav` assets.
import { Assets } from "pixi.js";

const result = { status: "running" };
globalThis.__rovesToneScenario = result;

const check = (condition, message) => {
  if (!condition) throw new Error(message);
};
async function waitFor(predicate, what, timeoutMs = 3000) {
  const start = performance.now();
  while (!predicate()) {
    if (performance.now() - start > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
async function rejection(promise, name, what) {
  let error = null;
  try {
    await promise;
  } catch (caught) {
    error = caught;
  }
  check(error?.name === name, `${what} should reject with ${name}, got ${error}`);
  return error;
}
function throws(operation, name, what) {
  let error = null;
  try {
    operation();
  } catch (caught) {
    error = caught;
  }
  check(error?.name === name, `${what} should throw ${name}, got ${error}`);
}
// A 16-bit PCM mono WAV file holding `samples`.
function pcm16Wav(samples, sampleRate) {
  const bytes = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(bytes);
  const text = (offset, value) => [...value].forEach((char, index) => view.setUint8(offset + index, char.charCodeAt(0)));
  text(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true); // block align
  view.setUint16(34, 16, true); // bits per sample
  text(36, "data");
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((sample, index) => view.setInt16(44 + index * 2, sample, true));
  return bytes;
}
// A 440 Hz tone at half scale, like the template's 16-bit mono 44.1 kHz files.
function toneWav(frames, sampleRate) {
  const samples = Array.from({ length: frames }, (_, index) => Math.round(16384 * Math.sin((2 * Math.PI * 440 * index) / sampleRate)));
  return pcm16Wav(samples, sampleRate);
}
const blobUrl = (bytes) => URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
function peak(buffer) {
  let max = 0;
  for (let channel = 0; channel < buffer.numberOfChannels; channel++) {
    for (const sample of buffer.getChannelData(channel)) max = Math.max(max, Math.abs(sample));
  }
  return max;
}

// The template's sound files (visual-novel-template on Pixi'VN 1.9.8). Pixi'VN's
// sound.backgroundLoadBundle first hands the bundle to PixiJS Assets, which has
// no audio load parser (PixiJS 8.19 ships none and neither Pixi'VN nor the
// template registers one): the `.wav` entries resolve to null with the "don't
// know how to parse it" warning, in any browser and whatever canPlayType
// answers. Pixi'VN then loads each alias itself with Tone's
// ToneAudioBuffer.load (fetch + decodeAudioData), which never consults
// canPlayType, and sound.play awaits that load before Tone.Player.start.
// `order` records the steps; src/main.rs also checks the bridge's own trace.
async function templateSoundFiles(context, result) {
  const order = [];
  result.audio = { order };

  // 1. PixiJS parser choice, with the probe's canPlayType ("" for every type,
  // since no media element can play) and with one claiming "probably".
  const media = HTMLMediaElement.prototype;
  const canPlayType = media.canPlayType;
  const realWarn = console.warn;
  const warnings = [];
  const parsers = {};
  try {
    console.warn = (...args) => warnings.push(args.join(" "));
    for (const answer of ["", "probably"]) {
      media.canPlayType = function () {
        return answer;
      };
      const url = `https://roves.invalid/audio/canplay-${answer || "empty"}.wav`;
      const parser = Assets.loader.parsers.find((candidate) => candidate.load && candidate.test?.(url, { src: url }, Assets.loader));
      const loaded = await Assets.loader.load(url);
      check(loaded === null, `Pixi loaded ${url} as ${loaded}`);
      check(warnings.some((warning) => warning.includes(url) && warning.includes("don't know how to parse it")), `no Pixi warning for ${url}`);
      parsers[answer || "empty"] = parser?.name ?? null;
      order.push(`pixi_parser:${answer || "empty"}:${parser?.name ?? "none"}`);
    }
  } finally {
    media.canPlayType = canPlayType;
    console.warn = realWarn;
  }
  check(parsers.empty === null && parsers.probably === null, `Pixi chose an audio parser: ${JSON.stringify(parsers)}`);
  // Tone's own capability check reports the truth (no playable media element);
  // the load path below does not depend on it.
  check(Tone.ToneAudioBuffer.supportsType("wav") === false, "ToneAudioBuffer.supportsType must reflect canPlayType");

  // 2. Decoding completes later, from the frame loop, never synchronously or
  // within the same microtask checkpoint.
  const native = new AudioContext();
  let settled = false;
  const delayed = native.decodeAudioData(toneWav(441, 44100)).finally(() => {
    settled = true;
  });
  for (let turn = 0; turn < 8; turn++) await Promise.resolve();
  check(!settled, "decodeAudioData settled before a frame dispatched the decoder result");
  order.push("delayed_decode:pending");
  check(peak(await delayed) > 0.4, "the delayed decode returned silence");
  order.push("delayed_decode:ready");
  await native.close();

  // 3. Pixi'VN's path for the two template files: both loads start (the
  // background bundle load), then sound.play waits for its buffer and starts.
  const sfxUrl = blobUrl(toneWav(4410, 44100));
  const bgmUrl = blobUrl(toneWav(44100, 44100));
  const load = (name, url) => {
    order.push(`request:${name}`);
    return Tone.ToneAudioBuffer.load(url).then((buffer) => {
      order.push(`buffer_ready:${name}`);
      return buffer;
    });
  };
  // Before its buffer is ready a Player cannot start: this is the Tone error
  // Pixi'VN surfaces when a load failed and left an empty buffer.
  const early = new Tone.Player(new Tone.ToneAudioBuffer()).toDestination();
  let earlyError = null;
  try {
    early.start();
  } catch (error) {
    earlyError = error;
  }
  check(String(earlyError?.message).includes("buffer is either not set or not loaded"), `an unloaded Player started: ${earlyError}`);
  // Not disposed: Tone recorded the failed start in its state timeline, and
  // dispose() would stop a source that never existed. It holds no buffer.
  early.disconnect();
  const [sfx, bgm] = await Promise.all([load("sfx", sfxUrl), load("bgm", bgmUrl)]);
  URL.revokeObjectURL(sfxUrl);
  URL.revokeObjectURL(bgmUrl);
  for (const [name, buffer, seconds] of [["sfx", sfx, 0.1], ["bgm", bgm, 1]]) {
    check(buffer instanceof AudioBuffer && buffer.sampleRate === context.sampleRate, `${name} decoded at ${buffer?.sampleRate} Hz`);
    check(Math.abs(buffer.duration - seconds) < 0.002, `${name} duration ${buffer.duration} s, expected ${seconds}`);
    const level = peak(buffer);
    check(level > 0.4 && level < 0.6, `${name} peak ${level}, expected about 0.5`);
    result.audio[name] = { channels: buffer.numberOfChannels, frames: buffer.length, rate: buffer.sampleRate, peak: Math.round(level * 1000) / 1000 };
  }
  const player = new Tone.Player(new Tone.ToneAudioBuffer(sfx)).toDestination();
  check(player.loaded, "a Player on a decoded buffer must be loaded");
  const stopped = new Promise((resolve) => {
    player.onstop = resolve;
  });
  player.start();
  order.push("start:sfx");
  await stopped;
  order.push("stop:sfx");
  player.dispose();

  // 4. Bytes that are not audio: the load rejects (Pixi'VN then warns and keeps
  // an empty buffer); nothing is decoded to silence.
  const badUrl = blobUrl(new Uint8Array(64).fill(7));
  await rejection(Tone.ToneAudioBuffer.load(badUrl), "EncodingError", "loading bytes that are not audio");
  URL.revokeObjectURL(badUrl);
  order.push("invalid:rejected");

  const index = (step) => order.indexOf(step);
  for (const name of ["sfx", "bgm"]) check(index(`request:${name}`) < index(`buffer_ready:${name}`), `${name} ready before its request`);
  check(index("request:bgm") < index("buffer_ready:sfx"), "both loads must be requested before either completes");
  check(index("buffer_ready:sfx") < index("start:sfx") && index("start:sfx") < index("stop:sfx"), `start order ${order}`);
}

(async () => {
  // Without a native AudioContext Tone keeps its DummyContext (sample rate 0 and
  // an empty destination): what broke Pixi'VN's `volumeAll` in the template.
  const context = Tone.getContext();
  check(context.sampleRate > 0, `Tone kept its DummyContext (sampleRate ${context.sampleRate})`);
  const destination = Tone.getDestination();
  check(destination.volume.value === 0, `destination volume ${destination.volume.value} dB, expected 0`);
  const bpm = Tone.getTransport().bpm.value;
  check(Math.abs(bpm - 120) < 1e-6, `transport bpm ${bpm}, expected 120`);

  await Tone.start();
  check(context.state === "running", `context state ${context.state} after Tone.start()`);
  const startedAt = context.currentTime;

  // Pixi'VN's volumeAll setter: dB on Tone's side, gain on the native param once
  // the render thread reaches the scheduled time.
  destination.volume.value = -6;
  check(Math.abs(destination.volume.value + 6) < 1e-6, `volume ${destination.volume.value} dB after setting -6`);
  const nativeGain = destination.volume._param;
  const expected = Tone.dbToGain(-6);
  await waitFor(() => Math.abs(nativeGain.value - expected) < 1e-4, `native gain ${expected} (now ${nativeGain.value})`);
  check(context.currentTime > startedAt, "currentTime must advance while running");

  const raw = context.rawContext;

  // Pixi'VN's sound init creates stereo panners: with a native createStereoPanner
  // standardized-audio-context uses it instead of its merger/wave-shaper faker.
  const panner = new Tone.Panner(0.5).toDestination();
  check(Math.abs(panner.pan.value - 0.5) < 1e-6, `panner pan ${panner.pan.value}, expected 0.5`);
  const nativePan = panner.pan._param;
  check(nativePan.minValue === -1 && nativePan.maxValue === 1, `pan range [${nativePan.minValue}, ${nativePan.maxValue}]`);
  await waitFor(() => Math.abs(nativePan.value - 0.5) < 1e-4, `native pan 0.5 (now ${nativePan.value})`);
  let panError = null;
  try {
    raw.createStereoPanner().channelCountMode = "max";
  } catch (error) {
    panError = error;
  }
  check(panError?.name === "NotSupportedError", `StereoPanner mode 'max' threw ${panError}`);

  // ChannelMergerNode contract (Tone.Merge and the faker path both use it).
  const merge = new Tone.Merge({ channels: 2 }).toDestination();
  const merger = merge.input;
  check(merger.numberOfInputs === 2 && merger.numberOfOutputs === 1, `merger ${merger.numberOfInputs} in, ${merger.numberOfOutputs} out`);
  check(merger.channelCount === 1 && merger.channelCountMode === "explicit", `merger ${merger.channelCount} ch, ${merger.channelCountMode}`);
  let mergerError = null;
  try {
    merger.channelCount = 2;
  } catch (error) {
    mergerError = error;
  }
  check(mergerError?.name === "InvalidStateError", `merger channelCount 2 threw ${mergerError}`);
  const defaultMerger = raw.createChannelMerger();
  check(defaultMerger.numberOfInputs === 6, `default merger has ${defaultMerger.numberOfInputs} inputs`);
  let countError = null;
  try {
    raw.createChannelMerger(33);
  } catch (error) {
    countError = error;
  }
  check(countError?.name === "IndexSizeError", `createChannelMerger(33) threw ${countError}`);
  const right = raw.createConstantSource();
  right.connect(merger, 0, 1);
  right.start();
  right.stop(raw.currentTime + 0.02);
  merge.dispose();
  panner.dispose();

  // `ended` travels from the audio event thread to a listener on the V8 thread.
  const source = raw.createConstantSource();
  let ended = false;
  source.onended = () => {
    ended = true;
  };
  source.connect(raw.destination);
  source.start();
  source.stop(raw.currentTime + 0.05);
  await waitFor(() => ended, "the ended event of a stopped ConstantSourceNode");

  // Sound files: decodeAudioData decodes the WAV natively on the decoder thread
  // (the WAV uses the context rate, so no resampling changes the samples).
  const native = new AudioContext();
  await native.resume();
  const rate = native.sampleRate;
  const FRAMES = 1024;
  const pattern = [0, 16384, -16384, 8192];
  const samples = Array.from({ length: FRAMES }, (_, index) => pattern[index % 4]);
  const wav = pcm16Wav(samples, rate);
  let callbackBuffer = null;
  const decoding = native.decodeAudioData(wav, (buffer) => {
    callbackBuffer = buffer;
  });
  check(wav.detached && wav.byteLength === 0, "decodeAudioData must detach its ArrayBuffer");
  const decoded = await decoding;
  check(decoded instanceof AudioBuffer && callbackBuffer === decoded, "decodeAudioData must resolve and call back with one AudioBuffer");
  check(
    decoded.numberOfChannels === 1 && decoded.length === FRAMES && decoded.sampleRate === rate,
    `decoded ${decoded.numberOfChannels} channel(s) × ${decoded.length} at ${decoded.sampleRate} Hz`,
  );
  check(Math.abs(decoded.duration - FRAMES / rate) < 1e-9, `decoded duration ${decoded.duration}`);
  const head = new Float32Array(4);
  decoded.copyFromChannel(head, 0, 4);
  check([0, 0.5, -0.5, 0.25].every((value, index) => Math.abs(head[index] - value) < 1e-4), `decoded samples ${[...head]}`);
  // standardized-audio-context's out-of-bounds probe: copies nothing, throws nothing.
  decoded.copyToChannel(new Float32Array(1), 0, -1);
  const live = decoded.getChannelData(0);
  check(live === decoded.getChannelData(0) && Math.abs(live[5] - 0.5) < 1e-4, "getChannelData must return the same live samples");
  throws(() => decoded.getChannelData(1), "IndexSizeError", "getChannelData(1) of a mono buffer");

  // Failures are reported, never decoded to silence.
  let callbackError = null;
  const encodingError = await rejection(
    native.decodeAudioData(new Uint8Array(64).buffer, null, (error) => {
      callbackError = error;
    }),
    "EncodingError",
    "decoding bytes that are not audio",
  );
  check(callbackError === encodingError, "the error callback must receive the rejection");
  await rejection(native.decodeAudioData(wav), "DataCloneError", "decoding a detached ArrayBuffer");
  await rejection(native.decodeAudioData(new Uint8Array(4)), "TypeError", "decoding a Uint8Array");

  // A decoded buffer plays to its end and fires `ended` without stop().
  const player = native.createBufferSource();
  check(player.buffer === null && !player.loop && player.playbackRate.value === 1 && player.detune.value === 0, "buffer source defaults");
  player.buffer = decoded;
  throws(() => (player.buffer = decoded), "InvalidStateError", "assigning a second buffer");
  let playerEnded = false;
  player.onended = () => {
    playerEnded = true;
  };
  player.connect(native.destination);
  player.start(0, 0);
  throws(() => player.start(), "InvalidStateError", "starting a buffer source twice");
  throws(() => native.createBufferSource().start(0, -1), "RangeError", "a negative offset");
  await waitFor(() => playerEnded, "the ended event of a buffer source that played to its end");

  // A looping source keeps playing past its buffer until stop().
  // An in-flight render quantum can publish the old native parameter value.
  // Check the constructor while rendering is suspended, then verify playback.
  await native.suspend();
  const looping = new AudioBufferSourceNode(native, { buffer: decoded, loop: true, loopStart: 0.001, loopEnd: 0.01, playbackRate: 2 });
  check(looping.loop && looping.loopStart === 0.001 && looping.loopEnd === 0.01, "loop options");
  check(looping.playbackRate.value === 2, `playbackRate ${looping.playbackRate.value}`);
  let loopEnded = false;
  looping.onended = () => {
    loopEnded = true;
  };
  looping.connect(native.destination);
  looping.start(0, 0.002);
  await native.resume();
  await waitFor(() => looping.playbackRate.value === 2, "the looping source playback rate");
  const stopAt = native.currentTime + 0.1;
  looping.stop(stopAt);
  await waitFor(() => loopEnded, "the ended event of a stopped looping source");
  check(native.currentTime >= stopAt - 0.01, `the looping source ended at ${native.currentTime}, before stop(${stopAt})`);

  // Buffers built in JS: samples written there are what the source acquires.
  const created = native.createBuffer(2, 128, rate);
  check(created.numberOfChannels === 2 && created.length === 128 && created.sampleRate === rate, "createBuffer shape");
  created.copyToChannel(Float32Array.of(0.25, 0.5), 1, 126);
  const tail = new Float32Array(4);
  created.copyFromChannel(tail, 1, 126);
  check(tail[0] === 0.25 && tail[1] === 0.5 && tail[2] === 0, `copied samples ${[...tail]}`);
  throws(() => native.createBuffer(0, 1, rate), "NotSupportedError", "a buffer without channels");
  throws(() => new AudioBuffer({ length: 1, sampleRate: 1000 }), "NotSupportedError", "a 1000 Hz buffer");
  const fromJs = native.createBufferSource();
  let fromJsEnded = false;
  fromJs.onended = () => {
    fromJsEnded = true;
  };
  fromJs.buffer = created;
  fromJs.connect(native.destination);
  fromJs.start();
  await waitFor(() => fromJsEnded, "the ended event of a buffer built with createBuffer");
  await native.close();

  // Tone.Player on a file decoded through Tone and standardized-audio-context,
  // the path Pixi'VN's sound playback takes.
  const toneBuffer = await context.decodeAudioData(pcm16Wav(samples, context.sampleRate));
  check(toneBuffer instanceof AudioBuffer && toneBuffer.length === FRAMES, `Tone decoded ${toneBuffer?.length} frames`);
  const tonePlayer = new Tone.Player(toneBuffer).toDestination();
  check(tonePlayer.loaded && Math.abs(tonePlayer.buffer.duration - toneBuffer.duration) < 1e-9, "Tone.Player must load the decoded buffer");
  tonePlayer.playbackRate = 2;
  let toneStopped = false;
  tonePlayer.onstop = () => {
    toneStopped = true;
  };
  tonePlayer.start();
  await waitFor(() => toneStopped, "Tone.Player to stop at the end of its buffer");
  tonePlayer.dispose();

  await templateSoundFiles(context, result);

  result.status = "passed";
})().catch((error) => {
  result.status = "failed";
  result.error = String(error?.stack ?? error);
});
