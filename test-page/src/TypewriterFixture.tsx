import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { MarkdownTypewriter } from "react-markdown-typewriter";
import PixiPanel from "./PixiPanel.tsx";
import ThreePanel from "./ThreePanel.tsx";

const DIALOGUE = `**Alice:** Welcome back! This line is written by *react-markdown-typewriter*, one
character at a time with **motion**, over a moving scene.

- Each letter fades in on its own animation.
- The background keeps rendering every frame.

> If this text stutters while the scene stays smooth, the problem is in the DOM animations.`;

/**
 * Logs `RAFSTATS ...` once, after `seconds` of `requestAnimationFrame` callbacks, so the same
 * frame-pacing numbers can be read from `roves.log` in Roves and from the console in Chrome.
 */
function useFramePacingProbe(label: string, seconds = 20) {
  useEffect(() => {
    const intervals: number[] = [];
    let last = 0;
    let start = 0;
    let handle = requestAnimationFrame(function frame(now) {
      if (!start) start = now;
      if (last) intervals.push(now - last);
      last = now;
      if (now - start < seconds * 1000) {
        handle = requestAnimationFrame(frame);
        return;
      }
      intervals.sort((a, b) => a - b);
      const mean = intervals.reduce((a, b) => a + b, 0) / intervals.length;
      const at = (q: number) => intervals[Math.floor(intervals.length * q)].toFixed(1);
      const over = (ms: number) => intervals.filter((v) => v > ms).length;
      console.log(
        `RAFSTATS ${label} frames=${intervals.length} mean=${mean.toFixed(2)} p50=${at(0.5)} ` +
          `p95=${at(0.95)} p99=${at(0.99)} max=${intervals[intervals.length - 1].toFixed(1)} ` +
          `over20ms=${over(20)} over33ms=${over(33)}`,
      );
    });
    return () => cancelAnimationFrame(handle);
  }, [label, seconds]);
}

export type TypewriterScene = "pixi" | "three";

/**
 * motion + a WebGL scene, the combination every pixi-vn game uses: a typewriter dialogue
 * (react-markdown-typewriter, built on motion) and a looping motion box over PixiJS or three.js.
 */
export default function TypewriterFixture({ scene }: { scene: TypewriterScene }) {
  const [run, setRun] = useState(0);
  useFramePacingProbe(`typewriter-${scene}`);

  return (
    <main style={{ position: "relative", minHeight: "100vh", background: "#000", color: "#eee" }}>
      <div style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center" }}>
        {scene === "pixi" ? <PixiPanel animated /> : <ThreePanel />}
      </div>
      <motion.div
        animate={{ x: [0, 300, 0], rotate: [0, 180, 360] }}
        transition={{ duration: 3, repeat: Infinity, ease: "easeInOut" }}
        style={{ position: "absolute", top: 24, left: 24, width: 48, height: 48, background: "#8ab4f8" }}
      />
      <div
        style={{
          position: "absolute",
          left: "10%",
          right: "10%",
          bottom: 24,
          padding: "1rem 1.5rem",
          background: "rgba(0,0,0,0.7)",
          borderRadius: 12,
          fontFamily: "sans-serif",
          fontSize: 18,
          minHeight: 160,
        }}
      >
        <MarkdownTypewriter
          key={run}
          delay={20}
          motionProps={{ onAnimationComplete: () => setTimeout(() => setRun((r) => r + 1), 800) }}
        >
          {DIALOGUE}
        </MarkdownTypewriter>
      </div>
    </main>
  );
}
