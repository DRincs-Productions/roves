// Tone.js 15.1.22 (through standardized-audio-context 25.3.77, as in
// visual-novel-template) on the probe's Web Audio: src/web_audio.js over
// web-audio-api 1.7.0. The frame loop drives it; src/main.rs reads the result.
import * as Tone from "tone";

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

  result.status = "passed";
})().catch((error) => {
  result.status = "failed";
  result.error = String(error?.stack ?? error);
});
