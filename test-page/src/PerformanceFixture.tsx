import PixiPanel from "./PixiPanel.tsx";

export type PerformanceFixtureMode = "blank" | "pixi-static" | "pixi-animated";

export function performanceFixtureMode(search: string): PerformanceFixtureMode | null {
  const value = new URLSearchParams(search).get("perf");
  if (value === "blank" || value === "pixi-static" || value === "pixi-animated") {
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
      {mode === "blank" ? null : <PixiPanel animated={mode === "pixi-animated"} />}
    </main>
  );
}
