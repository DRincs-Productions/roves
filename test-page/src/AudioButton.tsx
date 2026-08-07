import { useState } from "react";

/**
 * WebAudio smoke test: a short synthesized beep, no asset file needed. Games
 * live or die on audio actually reaching the speakers — this is the
 * cheapest possible check that the audio pipeline exists in this build.
 */
export default function AudioButton() {
  const [status, setStatus] = useState("Not tested.");

  const beep = () => {
    try {
      const ctx = new AudioContext();
      const oscillator = ctx.createOscillator();
      const gain = ctx.createGain();
      oscillator.frequency.value = 440;
      gain.gain.value = 0.1;
      oscillator.connect(gain).connect(ctx.destination);
      oscillator.start();
      oscillator.stop(ctx.currentTime + 0.2);
      oscillator.onended = () => void ctx.close();
      setStatus("ok — played a 440Hz beep for 200ms");
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={beep}>
        Play test beep (WebAudio)
      </button>
      <span>{status}</span>
    </div>
  );
}
