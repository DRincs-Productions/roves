import { motion } from "motion/react";
import { useState } from "react";
import { MarkdownTypewriter } from "markdown-typewriter/react";
import PixiPanel from "./PixiPanel.tsx";
import ThreePanel from "./ThreePanel.tsx";
import { useFramePacingProbe } from "./framePacing.ts";

const DIALOGUE = `**Alice:** Welcome back! This line is written by *markdown-typewriter*, one
character at a time with **motion**, over a moving scene.

- Each letter fades in on its own animation.
- The background keeps rendering every frame.

> If this text stutters while the scene stays smooth, the problem is in the DOM animations.`;

export type TypewriterScene = "pixi" | "three";

/**
 * motion + a WebGL scene, the combination every pixi-vn game uses: a typewriter dialogue
 * (markdown-typewriter, built on motion) and a looping motion box over PixiJS or three.js.
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
