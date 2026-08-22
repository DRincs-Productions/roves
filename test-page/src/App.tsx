import { exit } from "@drincs/roves-api/process";
import { steam } from "@drincs/roves-api/steam";
import { useState } from "react";
import ClearCacheButton from "./ClearCacheButton.tsx";
import DiagnosticsPanel from "./DiagnosticsPanel.tsx";
import FullscreenButton from "./FullscreenButton.tsx";
import GamepadPanel from "./GamepadPanel.tsx";
import GpuInfoPanel from "./GpuInfoPanel.tsx";
import IndexedDbButton from "./IndexedDbButton.tsx";
import PixiPanel from "./PixiPanel.tsx";
import StorageButton from "./StorageButton.tsx";
import ThreePanel from "./ThreePanel.tsx";
import ToneButton from "./ToneButton.tsx";

type RenderTest = "none" | "pixi" | "three";

/**
 * Manual diagnostic page for ../../.github/workflows/test.yml's build-from-source
 * smoke test — a human clicks through this after downloading a build from the
 * "test" GitHub release, it's not a CI assertion.
 *
 * Steam checks: two deliberately separate ones, at two different layers —
 * "raw fetch" hand-rolls `fetch("steam:is_available")` directly against the
 * `steam:` protocol handler (see ../../CUSTOMIZATIONS.md's "steam: protocol
 * bridge" entry), while "roves-api" instead calls the real
 * `@drincs/roves-api/steam` wrapper the actual game imports — now a normal
 * published npm dependency (see package.json), not resolved through the
 * parent monorepo's own workspace. Keeping both checks means a failure here
 * can tell apart "the protocol itself is broken" from "the JS wrapper has a
 * bug the protocol doesn't" — the raw-fetch one isn't just legacy left in
 * place.
 *
 * The "quit" button below exercises `@drincs/roves-api/process`'s `exit()`
 * the same way — the real, destructive `roves:exit` command, guarded behind
 * a confirm() since it actually closes the window. ClearCacheButton is the
 * same shape for `@drincs/roves-api/cache`'s `clearContentCache()`, which
 * also closes the window (see that module's own doc comment for why).
 *
 * PixiJS / Three.js checks: the real game renders through PixiJS
 * (`@drincs/pixi-vn`); Three.js is a second, unrelated WebGL consumer included
 * purely to tell apart "WebGL itself is broken in this Servo build" from
 * "something specific to PixiJS is broken" — see PixiPanel.tsx/ThreePanel.tsx,
 * both now also reporting fps alongside the render check.
 *
 * The rest (GpuInfoPanel, GamepadPanel, FullscreenButton, AudioButton,
 * StorageButton) round out the page into game-platform diagnostics rather
 * than just "does WebGL work": which GPU/renderer string is actually behind
 * WebGL, gamepad input, fullscreen, audio, and save-data persistence.
 *
 * DiagnosticsPanel bundles all of the above (plus resolution/memory/fps/UA)
 * into one copy-pasteable JSON report, mirroring the parent project's own
 * in-game diagnostics report shape — see that file for why it's a plain
 * toggled view rather than a native `<dialog>`.
 */
export default function App() {
  const [steamResult, setSteamResult] = useState("Click a button above.");
  const [exitStatus, setExitStatus] = useState<string | null>(null);
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

  const checkSteamApi = async () => {
    try {
      const available = await steam.isAvailable();
      setSteamResult(`@drincs/roves-api/steam — steam.isAvailable(): ${available}`);
    } catch (error) {
      setSteamResult(`@drincs/roves-api/steam — FAILED: ${String(error)}`);
    }
  };

  const quitApp = async () => {
    if (!window.confirm("This calls @drincs/roves-api/process's exit() — it will close this window. Continue?")) {
      return;
    }
    try {
      await exit();
    } catch (error) {
      setExitStatus(`exit() FAILED: ${String(error)}`);
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
        <button type="button" onClick={checkSteamApi}>
          Test steam: protocol (@drincs/roves-api)
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

      <div style={{ display: "flex", gap: "1.5rem", flexWrap: "wrap", justifyContent: "center" }}>
        <FullscreenButton />
        <ToneButton />
        <StorageButton />
        <IndexedDbButton />
        <ClearCacheButton />
        <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
          <button type="button" onClick={quitApp}>
            Quit (@drincs/roves-api/process exit())
          </button>
          {exitStatus && <span>{exitStatus}</span>}
        </div>
      </div>

      <GpuInfoPanel />
      <GamepadPanel />

      <DiagnosticsPanel />
    </div>
  );
}
