import PixiPanel from "./PixiPanel.tsx";
import DomAnimationFixture from "./DomAnimationFixture.tsx";
import TypewriterFixture from "./TypewriterFixture.tsx";
import FullStackFixture, { type FixturePart } from "./FullStackFixture.tsx";

export type PerformanceFixtureMode =
  | "blank"
  | "pixi-static"
  | "pixi-animated"
  | "typewriter-pixi"
  | "typewriter-three"
  | "gsap-pixi"
  | "motion-pixi"
  | "full-stack"
  | "only-three"
  | "only-tone"
  | "only-motion"
  | "only-gsap"
  | "only-worker";

export function performanceFixtureMode(search: string): PerformanceFixtureMode | null {
  const value = new URLSearchParams(search).get("perf");
  if (
    value === "blank" ||
    value === "pixi-static" ||
    value === "pixi-animated" ||
    value === "typewriter-pixi" ||
    value === "typewriter-three" ||
    value === "gsap-pixi" ||
    value === "motion-pixi" ||
    value === "full-stack" ||
    value === "only-three" ||
    value === "only-tone" ||
    value === "only-motion" ||
    value === "only-gsap" ||
    value === "only-worker"
  ) {
    return value;
  }
  return null;
}

/**
 * Minimal, deterministic pages for comparing the same workload in Roves and Chrome.
 * Keep this component free of timers, diagnostics probes and unrelated panels: those would
 * contaminate CPU/wake-up measurements, especially the static and blank baselines.
 */
export default function PerformanceFixture({ mode }: { mode: PerformanceFixtureMode }) {
  if (mode === "full-stack") return <FullStackFixture />;
  if (mode.startsWith("only-")) return <FullStackFixture name={mode} parts={[mode.slice(5) as FixturePart]} />;
  if (mode === "gsap-pixi" || mode === "motion-pixi") {
    return <DomAnimationFixture library={mode === "gsap-pixi" ? "gsap" : "motion"} />;
  }
  if (mode === "typewriter-pixi" || mode === "typewriter-three") {
    return <TypewriterFixture scene={mode === "typewriter-pixi" ? "pixi" : "three"} />;
  }
  return (
    <main
      data-performance-fixture={mode}
      style={{
        margin: 0,
        minHeight: "100vh",
        boxSizing: "border-box",
        display: "grid",
        placeItems: "center",
        background: "#000",
        color: "#eee",
        fontFamily: "sans-serif",
      }}
    >
      {mode === "blank" ? null : <PixiPanel animated={mode === "pixi-animated"} diagnostics={false} />}
    </main>
  );
}
