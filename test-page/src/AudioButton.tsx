import { useState } from "react";

/**
 * WebAudio smoke test: a short synthesized beep, no asset file needed. Games
 * live or die on audio actually reaching the speakers — this is the
 * cheapest possible check that the audio pipeline exists in this build.
 *
 * Two durations on purpose: on Windows, GStreamer's `autoaudiosink` (see
 * components/media/backends/gstreamer/audio_sink.rs) can take a few hundred ms
 * to actually open the real output device on a cold start. `pipeline.set_state
 * (Playing)` returns success as soon as the state change is *requested*, not
 * once the device is actually open — so a very short tone can finish and
 * close its context before any sample physically reaches the speakers, while
 * still reporting "ok" (no exception was ever thrown). The long tone exists to
 * tell apart "audio pipeline is fundamentally broken" from "it works, the
 * short tone just loses a race against device open latency".
 */
export default function AudioButton() {
  const [shortStatus, setShortStatus] = useState("Not tested.");
  const [longStatus, setLongStatus] = useState("Not tested.");

  const playTone = async (durationSeconds: number, report: (status: string) => void) => {
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
      oscillator.stop(ctx.currentTime + durationSeconds);
      oscillator.onended = () => void ctx.close();
      report(
        `ok — played a 440Hz tone for ${Math.round(durationSeconds * 1000)}ms (context state: ${ctx.state})`,
      );
    } catch (error) {
      report(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <button type="button" onClick={() => void playTone(0.2, setShortStatus)}>
          Play test beep (WebAudio)
        </button>
        <span>{shortStatus}</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
        <button type="button" onClick={() => void playTone(2, setLongStatus)}>
          Play long test tone (2s, WebAudio)
        </button>
        <span>{longStatus}</span>
      </div>
    </div>
  );
}
