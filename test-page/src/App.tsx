import { useState } from "react";
import PixiPanel from "./PixiPanel.tsx";
import ThreePanel from "./ThreePanel.tsx";

type RenderTest = "none" | "pixi" | "three";

/**
 * Manual diagnostic page for ../../.github/workflows/test.yml's build-from-source
 * smoke test — a human clicks through this after downloading a build from the
 * "test" GitHub release, it's not a CI assertion.
 *
 * Steam check: the `steam:` protocol handler the real game's `src/lib/steam.ts`
 * (Tauri) / `@drincs/roves-api/steam` (Roves) both ultimately call via plain
 * `fetch()` — see ../../CUSTOMIZATIONS.md's "steam: protocol bridge" entry. This
 * deliberately calls `fetch()` directly rather than importing
 * `@drincs/roves-api/steam` itself: that package isn't published to npm yet (it's
 * only resolved today via the parent monorepo's npm workspace — see its own
 * package.json), and this directory is meant to stay buildable as a standalone
 * repo (see ../CLAUDE.md), so it can't take a dependency that only resolves
 * inside that other workspace. Revisit once `@drincs/roves-api` is actually
 * published — at that point this can import and call the real `steam.isAvailable()`
 * instead of hand-rolling the same request shape.
 *
 * PixiJS / Three.js checks: the real game renders through PixiJS
 * (`@drincs/pixi-vn`); Three.js is a second, unrelated WebGL consumer included
 * purely to tell apart "WebGL itself is broken in this Servo build" from
 * "something specific to PixiJS is broken" — see PixiPanel.tsx/ThreePanel.tsx.
 */
export default function App() {
  const [steamResult, setSteamResult] = useState("Click the button above.");
  const [renderTest, setRenderTest] = useState<RenderTest>("none");

  const checkSteamFetch = async () => {
    try {
      const response = await fetch("steam:is_available");
      const body = await response.json();
      setSteamResult(
        `fetch("steam:is_available") — reachable\n${JSON.stringify({ status: response.status, body }, null, 2)}`,
      );
    } catch (error) {
      setSteamResult(`fetch("steam:is_available") — NOT reachable\n${String(error)}`);
    }
  };

  const toggleRenderTest = (test: RenderTest) => {
    setRenderTest((current) => (current === test ? "none" : test));
  };

  return (
    <div
      style={{
        margin: 0,
        minHeight: "100vh",
        boxSizing: "border-box",
        padding: "2rem",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: "1rem",
        background: "#1b1b1f",
        color: "#eee",
        fontFamily: "sans-serif",
      }}
    >
      <h1 style={{ fontSize: "1.4rem", margin: 0, textAlign: "center" }}>
        Servo customization test build — no toolbar, no tabs.
      </h1>

      <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap", justifyContent: "center" }}>
        <button type="button" onClick={checkSteamFetch}>
          Test steam: protocol (raw fetch)
        </button>
        <button type="button" onClick={() => toggleRenderTest("pixi")}>
          {renderTest === "pixi" ? "Stop" : "Test"} PixiJS render
        </button>
        <button type="button" onClick={() => toggleRenderTest("three")}>
          {renderTest === "three" ? "Stop" : "Test"} Three.js render
        </button>
      </div>

      <pre
        style={{
          background: "#111",
          padding: "1rem",
          borderRadius: "6px",
          maxWidth: "90vw",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
        }}
      >
        {steamResult}
      </pre>

      {renderTest === "pixi" && <PixiPanel />}
      {renderTest === "three" && <ThreePanel />}
    </div>
  );
}
