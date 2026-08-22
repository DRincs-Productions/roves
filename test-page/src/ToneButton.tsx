import { useState } from "react";
import * as Tone from "tone";

/**
 * Audio smoke test via Tone.js -- what a real game is actually likely to use
 * (synths, players, effects chains) rather than raw
 * AudioContext/OscillatorNode calls. See CUSTOMIZATIONS.md's "GStreamer
 * audio sink" diagnostic entries: an earlier raw-WebAudio version of this
 * test (creating and closing a fresh AudioContext per click) produced no/
 * erratic audio on at least one real machine, while Tone.js's single reused
 * context (Tone.getContext()) played correctly -- so this is the version
 * kept. Short/long split lets a future report tell a duration-specific issue
 * apart from a fundamentally broken pipeline.
 */
export default function ToneButton() {
  const [shortStatus, setShortStatus] = useState("Not tested.");
  const [longStatus, setLongStatus] = useState("Not tested.");

  const playNote = async (durationSeconds: number, report: (status: string) => void) => {
    try {
      // Tone.start() resumes the underlying AudioContext -- still needs a
      // real user gesture to actually take effect, same as the WebAudio
      // autoplay policy in general.
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
