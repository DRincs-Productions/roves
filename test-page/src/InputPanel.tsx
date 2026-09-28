import { useEffect, useRef, useState } from "react";

/**
 * Event history for every input path a game relies on: keyboard, mouse buttons and wheel, and
 * Gamepad API button/axis changes (edge-detected from polling, since the Gamepad API has no
 * per-button events). Complements GamepadPanel's live state readout.
 *
 * Every entry is also written to the console with INPUT_MARKER, which reaches stdout/roves.log:
 * CI launches with synthetic input (xdotool on Linux, CGEvent on macOS, a virtual SDL gamepad)
 * and greps for these lines to prove the event reached the page, not just the shell.
 * Mouse moves are deliberately not logged — too noisy to be useful.
 */
export const INPUT_MARKER = "[roves-input]";

const MAX_ENTRIES = 12;
const AXIS_THRESHOLD = 0.5;

function modifiers(event: KeyboardEvent | MouseEvent | WheelEvent) {
  return [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Meta"]
    .filter(Boolean)
    .join("+");
}

function mouseButtonName(button: number) {
  return ["left", "middle", "right", "back", "forward"][button] ?? `button${button}`;
}

export default function InputPanel() {
  const [entries, setEntries] = useState<string[]>([]);
  const [heldKeys, setHeldKeys] = useState<string[]>([]);
  const [heldMouse, setHeldMouse] = useState<string[]>([]);
  const counter = useRef(0);

  useEffect(() => {
    const record = (kind: string, detail: string) => {
      counter.current += 1;
      console.log(`${INPUT_MARKER} kind=${kind} ${detail}`);
      const line = `#${counter.current} ${kind}: ${detail}`;
      setEntries((current) => [line, ...current].slice(0, MAX_ENTRIES));
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return;
      const mods = modifiers(event);
      record("keydown", `key=${JSON.stringify(event.key)} code=${event.code}${mods ? ` mods=${mods}` : ""}`);
      setHeldKeys((current) => (current.includes(event.code) ? current : [...current, event.code]));
    };
    const onKeyUp = (event: KeyboardEvent) => {
      record("keyup", `key=${JSON.stringify(event.key)} code=${event.code}`);
      setHeldKeys((current) => current.filter((code) => code !== event.code));
    };
    const onMouseDown = (event: MouseEvent) => {
      const name = mouseButtonName(event.button);
      const mods = modifiers(event);
      record("mousedown", `button=${name} x=${event.clientX} y=${event.clientY}${mods ? ` mods=${mods}` : ""}`);
      setHeldMouse((current) => (current.includes(name) ? current : [...current, name]));
    };
    const onMouseUp = (event: MouseEvent) => {
      const name = mouseButtonName(event.button);
      record("mouseup", `button=${name} x=${event.clientX} y=${event.clientY}`);
      setHeldMouse((current) => current.filter((button) => button !== name));
    };
    const onWheel = (event: WheelEvent) => {
      record("wheel", `dx=${Math.round(event.deltaX)} dy=${Math.round(event.deltaY)} mode=${event.deltaMode}`);
    };
    const onContextMenu = (event: MouseEvent) => event.preventDefault();
    const onBlur = () => {
      setHeldKeys([]);
      setHeldMouse([]);
    };
    const onGamepadConnected = (event: GamepadEvent) => {
      record("gamepadconnected", `index=${event.gamepad.index} id=${JSON.stringify(event.gamepad.id)}`);
    };
    const onGamepadDisconnected = (event: GamepadEvent) => {
      record("gamepaddisconnected", `index=${event.gamepad.index} id=${JSON.stringify(event.gamepad.id)}`);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("mousedown", onMouseDown);
    window.addEventListener("mouseup", onMouseUp);
    window.addEventListener("wheel", onWheel, { passive: true });
    window.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("blur", onBlur);
    window.addEventListener("gamepadconnected", onGamepadConnected);
    window.addEventListener("gamepaddisconnected", onGamepadDisconnected);

    // Gamepad buttons/axes: edge-detect against the previous poll.
    const previousButtons = new Map<number, boolean[]>();
    const previousAxes = new Map<number, number[]>();
    let frameId: number | undefined;
    const poll = () => {
      if ("getGamepads" in navigator) {
        for (const pad of navigator.getGamepads()) {
          if (!pad) continue;
          const buttons = pad.buttons.map((button) => button.pressed);
          const lastButtons = previousButtons.get(pad.index) ?? [];
          buttons.forEach((pressed, index) => {
            if (pressed !== (lastButtons[index] ?? false)) {
              record(pressed ? "gamepadbuttondown" : "gamepadbuttonup", `index=${pad.index} button=${index}`);
            }
          });
          previousButtons.set(pad.index, buttons);

          const axes = pad.axes.map((axis) => (Math.abs(axis) >= AXIS_THRESHOLD ? Math.sign(axis) : 0));
          const lastAxes = previousAxes.get(pad.index) ?? [];
          axes.forEach((direction, index) => {
            if (direction !== (lastAxes[index] ?? 0)) {
              record("gamepadaxis", `index=${pad.index} axis=${index} direction=${direction}`);
            }
          });
          previousAxes.set(pad.index, axes);
        }
      }
      frameId = requestAnimationFrame(poll);
    };
    frameId = requestAnimationFrame(poll);

    console.log(`${INPUT_MARKER} ready`);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("mouseup", onMouseUp);
      window.removeEventListener("wheel", onWheel);
      window.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("gamepadconnected", onGamepadConnected);
      window.removeEventListener("gamepaddisconnected", onGamepadDisconnected);
      if (frameId !== undefined) cancelAnimationFrame(frameId);
    };
  }, []);

  const boxStyle = {
    background: "#111",
    padding: "0.75rem",
    borderRadius: "6px",
    maxWidth: "90vw",
    minWidth: "min(36rem, 90vw)",
    whiteSpace: "pre-wrap" as const,
    wordBreak: "break-word" as const,
    fontSize: "0.85rem",
    margin: 0,
  };

  return (
    <div>
      <p>Input (keyboard, mouse, gamepad) — press keys, click, scroll or use a controller:</p>
      <pre style={boxStyle}>
        {`Keys held:  ${heldKeys.length ? heldKeys.join(" ") : "—"}\n`}
        {`Mouse held: ${heldMouse.length ? heldMouse.join(" ") : "—"}\n\n`}
        {entries.length ? entries.join("\n") : "No input yet."}
      </pre>
    </div>
  );
}
