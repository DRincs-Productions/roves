import { useState } from "react";

/**
 * WebAudio smoke test: a short synthesized beep, no asset file needed. Games
 * live or die on audio actually reaching the speakers — this is the
 * cheapest possible check that the audio pipeline exists in this build.
 */
export default function AudioButton() {
  const [status, setStatus] = useState("Not tested.");

  const beep = async () => {
    try {
      const ctx = new AudioContext();
      // A fresh AudioContext can start "suspended" even from a click handler —
      // this is a real click (a user gesture), but nothing above actually
      // resumes it, so oscillator.start() below would otherwise report success
      // while producing no audible sound at all.
      if (ctx.state === "suspended") {
        await ctx.resume();
      }
      const oscillator = ctx.createOscillator();
      const gain = ctx.createGain();
      oscillator.frequency.value = 440;
      gain.gain.value = 0.1;
      oscillator.connect(gain).connect(ctx.destination);
      oscillator.start();
      oscillator.stop(ctx.currentTime + 0.2);
      oscillator.onended = () => void ctx.close();
      setStatus(`ok — played a 440Hz beep for 200ms (context state: ${ctx.state})`);
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
