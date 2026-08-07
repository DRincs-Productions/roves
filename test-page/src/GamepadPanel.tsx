import { useEffect, useState } from "react";

/**
 * Live Gamepad API readout. `gamepad` is one of servoshell's default Cargo
 * features (see ports/servoshell/Cargo.toml) — this is the one common game
 * input device that isn't already covered by keyboard/mouse just working,
 * and "nothing connected" vs. "the API doesn't exist here" are otherwise
 * indistinguishable from a game's own code.
 */
export default function GamepadPanel() {
  const [status, setStatus] = useState(
    "No gamepad detected yet — press a button on a connected pad.",
  );

  useEffect(() => {
    if (!("getGamepads" in navigator)) {
      setStatus("navigator.getGamepads is not available in this build.");
      return;
    }

    let frameId: number | undefined;

    const tick = () => {
      const pads = navigator.getGamepads().filter((pad): pad is Gamepad => pad !== null);

      setStatus(
        pads.length === 0
          ? "No gamepad detected yet — press a button on a connected pad."
          : pads
              .map((pad) => {
                const pressed = pad.buttons
                  .map((button, index) => (button.pressed ? index : null))
                  .filter((index): index is number => index !== null);
                const axes = pad.axes.map((axis) => axis.toFixed(2)).join(", ");
                return `${pad.id}\n  buttons pressed: [${pressed.join(", ")}]\n  axes: [${axes}]`;
              })
              .join("\n\n"),
      );
      frameId = requestAnimationFrame(tick);
    };
    frameId = requestAnimationFrame(tick);

    return () => {
      if (frameId !== undefined) cancelAnimationFrame(frameId);
    };
  }, []);

  return (
    <div>
      <p>Gamepad:</p>
      <pre
        style={{
          background: "#111",
          padding: "0.75rem",
          borderRadius: "6px",
          maxWidth: "90vw",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontSize: "0.85rem",
        }}
      >
        {status}
      </pre>
    </div>
  );
}
