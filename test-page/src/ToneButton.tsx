import { useState } from "react";
import * as Tone from "tone";

/**
 * Tone.js smoke test, alongside AudioButton.tsx's raw WebAudio one -- Tone.js
 * wraps the same underlying AudioContext/OscillatorNode this fork's own
 * GStreamer backend implements (see CUSTOMIZATIONS.md's "GStreamer audio
 * sink" diagnostic entries), but it's what a real game is actually likely to
 * use (synths, players, effects chains), so a report against raw WebAudio
 * alone doesn't necessarily generalize to it. Same short/long split as
 * AudioButton.tsx, for the same reason: isolate a device-open/timing race
 * from a fundamentally broken pipeline.
 */
export default function ToneButton() {
  const [shortStatus, setShortStatus] = useState("Not tested.");
  const [longStatus, setLongStatus] = useState("Not tested.");

  const playNote = async (durationSeconds: number, report: (status: string) => void) => {
    try {
      // Tone.start() resumes the underlying AudioContext -- same "must be a
      // real user gesture, but doesn't resume itself" caveat as
      // AudioButton.tsx's ctx.resume() call.
      await Tone.start();
      const synth = new Tone.Synth().toDestination();
      const start = Tone.now();
      synth.triggerAttackRelease("A4", durationSeconds, start);
      // Tone.js has no onended-equivalent event, so poll Tone.now() until the
      // scheduled release has actually elapsed on Tone's own transport clock
      // (same clock triggerAttackRelease scheduled against) before disposing
      // -- disposing early would tear down the voice mid-playback.
      const poll = () => {
        if (Tone.now() >= start + durationSeconds) {
          synth.dispose();
          report(`ok — played an A4 note for ${Math.round(durationSeconds * 1000)}ms (Tone.js)`);
        } else {
          setTimeout(poll, 20);
        }
      };
      poll();
    } catch (error) {
      report(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <button type="button" onClick={() => void playNote(0.2, setShortStatus)}>
          Play test note (Tone.js, 200ms)
        </button>
        <span>{shortStatus}</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <button type="button" onClick={() => void playNote(2, setLongStatus)}>
          Play long test note (Tone.js, 2s)
        </button>
        <span>{longStatus}</span>
      </div>
    </div>
  );
}
