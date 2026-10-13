import gsap from "gsap";
import { motion } from "motion/react";
import { useEffect, useRef } from "react";
import PixiPanel from "./PixiPanel.tsx";
import { useFramePacingProbe } from "./framePacing.ts";

const COUNT = 48;
const boxStyle = (i: number) => ({
  position: "absolute" as const,
  left: `${(i % 12) * 8 + 2}%`,
  top: `${Math.floor(i / 12) * 22 + 6}%`,
  width: 40,
  height: 40,
  borderRadius: 8,
  background: `hsl(${(i * 37) % 360} 80% 60%)`,
});

/** GSAP: tweens `transform` and `opacity` through inline styles from its own ticker. */
function GsapBoxes() {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const context = gsap.context(() => {
      gsap.to(".gsap-box", {
        x: 120,
        rotation: 360,
        opacity: 0.3,
        duration: 1.5,
        ease: "sine.inOut",
        repeat: -1,
        yoyo: true,
        stagger: 0.03,
      });
    }, root);
    return () => context.revert();
  }, []);
  return (
    <div ref={root}>
      {Array.from({ length: COUNT }, (_, i) => (
        <div key={i} className="gsap-box" style={boxStyle(i)} />
      ))}
    </div>
  );
}

/** motion: the same animation declared on `motion.div`s. */
function MotionBoxes() {
  return (
    <div>
      {Array.from({ length: COUNT }, (_, i) => (
        <motion.div
          key={i}
          style={boxStyle(i)}
          animate={{ x: [0, 120], rotate: [0, 360], opacity: [1, 0.3] }}
          transition={{
            duration: 1.5,
            ease: "easeInOut",
            repeat: Infinity,
            repeatType: "reverse",
            delay: i * 0.03,
          }}
        />
      ))}
    </div>
  );
}

export type DomAnimationLibrary = "gsap" | "motion";

/** 48 DOM boxes animated by GSAP or motion over an animated PixiJS scene. */
export default function DomAnimationFixture({ library }: { library: DomAnimationLibrary }) {
  useFramePacingProbe(`${library}-pixi`);
  return (
    <main style={{ position: "relative", minHeight: "100vh", background: "#000", overflow: "hidden" }}>
      <div style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center" }}>
        <PixiPanel animated />
      </div>
      {library === "gsap" ? <GsapBoxes /> : <MotionBoxes />}
    </main>
  );
}
