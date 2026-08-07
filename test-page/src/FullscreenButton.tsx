import { useState } from "react";

/**
 * Fullscreen API round-trip check. Most games want to run fullscreen rather
 * than embedded at the window's default size; this checks request/exit
 * actually work rather than silently doing nothing.
 */
export default function FullscreenButton() {
  const [status, setStatus] = useState("Not tested.");

  const toggle = async () => {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
        setStatus("Exited fullscreen.");
      } else {
        await document.documentElement.requestFullscreen();
        setStatus("Entered fullscreen.");
      }
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={toggle}>
        Toggle fullscreen
      </button>
      <span>{status}</span>
    </div>
  );
}
