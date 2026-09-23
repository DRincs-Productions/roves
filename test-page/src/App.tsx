import { exit } from "@drincs/roves-api/process";
import { steam } from "@drincs/roves-api/steam";
import { useEffect, useState } from "react";
import ClearCacheButton from "./ClearCacheButton.tsx";
import DiagnosticsPanel from "./DiagnosticsPanel.tsx";
import FullscreenButton from "./FullscreenButton.tsx";
import GamepadPanel from "./GamepadPanel.tsx";
import GpuInfoPanel from "./GpuInfoPanel.tsx";
import IndexedDbButton from "./IndexedDbButton.tsx";
import PixiPanel from "./PixiPanel.tsx";
import PerformanceFixture, { performanceFixtureMode } from "./PerformanceFixture.tsx";
import SavesButton from "./SavesButton.tsx";
import StorageButton from "./StorageButton.tsx";
import ThreePanel from "./ThreePanel.tsx";
import ToneButton from "./ToneButton.tsx";

type RenderTest = "none" | "pixi" | "three";

// Matches the fixture `steam_settings/achievements.json` / `stats.json` the
// steam-emulator-smoke-test CI job (../../.github/workflows/test.yml) generates
// for the Goldberg/GSE emulator it swaps in for `libsteam_api.so` before
// launching this build — see that job for the emulator setup, and
// STEAM_AUTOTEST_MARKER below for how the two sides meet.
const STEAM_AUTOTEST_ACHIEVEMENT_ID = "ACH_TEST_1";
const STEAM_AUTOTEST_STAT_NAME = "test_stat";
const STEAM_AUTOTEST_STAT_VALUE = 42;
const STEAM_AUTOTEST_MARKER = "[roves-steam-autotest]";

// Regression coverage for the `roves:save_file` command (../../CUSTOMIZATIONS.md's 2026-09-15
// desktop save export/import entry) that doesn't require a human to click through a native
// "Save As" dialog -- CI has no way to automate that part, so this only exercises the
// parameter-validation path, which returns before `protocols/roves.rs` ever reaches
// `AppEvent::SaveFileDialog`/shows any UI at all. The real end-to-end path (a save actually
// landing on disk) still needs the manual "Test save export" button below, clicked by a human
// after downloading a build from the "test" release.
const SAVE_FILE_AUTOTEST_MARKER = "[roves-save-file-autotest]";

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
 * StorageButton, SavesButton) round out the page into game-platform
 * diagnostics rather than just "does WebGL work": which GPU/renderer string
 * is actually behind WebGL, gamepad input, fullscreen, audio, and save-data
 * persistence — both the browser-native kind (StorageButton/IndexedDbButton)
 * and Roves' own `saves:` API (SavesButton).
 *
 * DiagnosticsPanel bundles all of the above (plus resolution/memory/fps/UA)
 * into one copy-pasteable JSON report, mirroring the parent project's own
 * in-game diagnostics report shape — see that file for why it's a plain
 * toggled view rather than a native `<dialog>`.
 */
export default function App() {
  const performanceMode = performanceFixtureMode(window.location.search);
  if (performanceMode) {
    return <PerformanceFixture mode={performanceMode} />;
  }

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

  // Runs unconditionally on mount — this page is never shipped to a real
  // player (see the file doc comment above), so there's no harm in always
  // exercising the full round trip in addition to the manual buttons.
  // console.log() here reaches the CI-visible logs for free: servoshell's
  // WebViewDelegate::show_console_message already forwards page console
  // output to stdout/roves.log (see ../../ports/servoshell/desktop/headed_window.rs
  // and headless_window.rs), which the smoke-test steps in test.yml already
  // capture — no new plumbing needed on the Rust side to get this result out
  // of a headless CI run. A short delay first gives the background
  // `client.run_callbacks()` thread in steam.rs a chance to receive the
  // initial user-stats callback the Steamworks SDK needs before achievement/
  // stat reads are reliable (see @drincs/roves-api/steam's isAchievementUnlocked
  // doc comment, which already calls this out for real Steam too).
  useEffect(() => {
    const timer = setTimeout(async () => {
      const result: Record<string, unknown> = {};
      try {
        result.isAvailable = await steam.isAvailable();
        result.appId = await steam.getAppId();
        result.playerName = await steam.getPlayerName();
        result.unlockAchievement = await steam.unlockAchievement(STEAM_AUTOTEST_ACHIEVEMENT_ID);
        result.isAchievementUnlocked = await steam.isAchievementUnlocked(STEAM_AUTOTEST_ACHIEVEMENT_ID);
        result.setStatInt = await steam.setStatInt(STEAM_AUTOTEST_STAT_NAME, STEAM_AUTOTEST_STAT_VALUE);
        result.storeStats = await steam.storeStats();
        result.getStatInt = await steam.getStatInt(STEAM_AUTOTEST_STAT_NAME);
      } catch (error) {
        result.error = String(error);
      }
      console.log(`${STEAM_AUTOTEST_MARKER} ${JSON.stringify(result)}`);
    }, 2000);
    return () => clearTimeout(timer);
  }, []);

  // See SAVE_FILE_AUTOTEST_MARKER's own doc comment: only the parameter-validation path,
  // which answers before any native dialog would ever show. `roves:save_file` (no params at
  // all) should fail fast with a 4xx-shaped NetworkError, not hang or crash the page.
  useEffect(() => {
    const timer = setTimeout(async () => {
      const result: Record<string, unknown> = {};
      try {
        const response = await fetch("roves:save_file");
        result.ok = response.ok;
        result.status = response.status;
      } catch (error) {
        result.error = String(error);
      }
      console.log(`${SAVE_FILE_AUTOTEST_MARKER} ${JSON.stringify(result)}`);
    }, 2000);
    return () => clearTimeout(timer);
  }, []);

  // Manual only (see the file doc comment) -- actually exercises the full round trip a human
  // tester needs to click through: the download-intercept userscript (app.rs) catches this
  // `<a download>` click, reads the Blob back out, and calls `roves:save_file`, which pops a
  // real native "Save As" dialog (`Dialog::SaveFile` in dialog.rs). CI can't automate picking
  // a destination in that dialog, so this can only ever be a manual check.
  const testSaveExport = () => {
    const blob = new Blob([JSON.stringify({ hello: "from the test page", at: new Date().toISOString() })], {
      type: "application/json",
    });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "roves-test-export.json";
    // Must be attached to the document before click(): a detached element's synthetic click
    // event has no ancestor chain to bubble/capture through, so app.rs's download-intercept
    // userscript (a document-level, capture-phase click listener) never sees it -- the click
    // falls through to a real top-level navigation to the blob: URL instead, which fails with
    // "Could not load the requested page: InvalidOrigin" (see that script's own doc comment for
    // why). Confirmed for real: this button reproduced exactly that error before this fix.
    document.body.appendChild(a);
    a.click();
    a.remove();
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
        <button type="button" onClick={testSaveExport}>
          Test save export (real file, needs manual click-through)
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

      {renderTest === "pixi" && <PixiPanel animated />}
      {renderTest === "three" && <ThreePanel />}

      <div style={{ display: "flex", gap: "1.5rem", flexWrap: "wrap", justifyContent: "center" }}>
        <FullscreenButton />
        <ToneButton />
        <StorageButton />
        <IndexedDbButton />
        <SavesButton />
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
