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

  // `ended` travels from the audio event thread to a listener on the V8 thread.
  const raw = context.rawContext;
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
