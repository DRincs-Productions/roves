import { saves } from "@drincs/roves-api/saves";
import { steam } from "@drincs/roves-api/steam";
import { VERSION as PIXI_VERSION } from "pixi.js";
import { version as REACT_VERSION } from "react";
import { useState } from "react";
import * as THREE from "three";

// Nonstandard, Chromium-only (performance.memory) — not in lib.dom.d.ts.
interface PerformanceMemory {
  usedJSHeapSize: number;
  totalJSHeapSize: number;
  jsHeapSizeLimit: number;
}

const KEY_EXTENSIONS = [
  "WEBGL_debug_renderer_info",
  "WEBGL_lose_context",
  "WEBGL_compressed_texture_s3tc",
  "WEBGL_compressed_texture_astc",
  "WEBGL_compressed_texture_etc",
  "OES_texture_float",
  "OES_texture_float_linear",
  "OES_texture_half_float",
  "OES_element_index_uint",
  "OES_standard_derivatives",
  "ANGLE_instanced_arrays",
  "EXT_texture_filter_anisotropic",
  "EXT_color_buffer_float",
];

function probeWebgl() {
  // Two separate canvases: a canvas can only ever be bound to one context
  // type — asking the same canvas for both "webgl2" and "webgl" would make
  // the second call return null regardless of what's actually supported.
  const gl2 = document.createElement("canvas").getContext("webgl2");
  const gl1 = document.createElement("canvas").getContext("webgl");
  const gl = gl2 ?? gl1;

  if (!gl) {
    return {
      webgl1Supported: false,
      webgl2Supported: false,
      vendor: "",
      renderer: "",
      unmaskedVendor: "",
      unmaskedRenderer: "",
      hardwareAccelerated: false,
      maxTextureSize: 0,
      floatTexturesSupported: false,
      msaaSupported: false,
      shadersCompile: false,
      keyExtensions: KEY_EXTENSIONS.map((name) => ({ name, supported: false })),
      allExtensions: [] as string[],
    };
  }

  const vendor = String(gl.getParameter(gl.VENDOR));
  const renderer = String(gl.getParameter(gl.RENDERER));
  const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
  const unmaskedVendor = debugInfo ? String(gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL)) : vendor;
  const unmaskedRenderer = debugInfo
    ? String(gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL))
    : renderer;

  // Trivial shader pair, GLSL ES 1.00 (the default for both WebGL1 and
  // WebGL2 contexts unless a "#version 300 es" pragma is added) — just
  // checking the compiler pipeline itself runs, not rendering anything.
  let shadersCompile = false;
  const vertexShader = gl.createShader(gl.VERTEX_SHADER);
  const fragmentShader = gl.createShader(gl.FRAGMENT_SHADER);
  if (vertexShader && fragmentShader) {
    gl.shaderSource(vertexShader, "void main() { gl_Position = vec4(0.0, 0.0, 0.0, 1.0); }");
    gl.compileShader(vertexShader);
    gl.shaderSource(fragmentShader, "void main() { gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0); }");
    gl.compileShader(fragmentShader);
    shadersCompile =
      gl.getShaderParameter(vertexShader, gl.COMPILE_STATUS) === true &&
      gl.getShaderParameter(fragmentShader, gl.COMPILE_STATUS) === true;
  }

  return {
    webgl1Supported: gl1 !== null,
    webgl2Supported: gl2 !== null,
    vendor,
    renderer,
    unmaskedVendor,
    unmaskedRenderer,
    // `null` (not a guess) when `WEBGL_debug_renderer_info` isn't available:
    // without the unmasked string, this heuristic has nothing real to check
    // — `renderer` alone is just the generic browser-controlled placeholder
    // every WebGL implementation returns for the masked RENDERER parameter,
    // so it will never contain "swiftshader"/"llvmpipe" regardless of what's
    // actually rendering, and reporting `true` there would be a false signal.
    hardwareAccelerated: debugInfo
      ? !/swiftshader|llvmpipe|software|warp/i.test(unmaskedRenderer)
      : null,
    maxTextureSize: Number(gl.getParameter(gl.MAX_TEXTURE_SIZE)),
    floatTexturesSupported: gl2 !== null || gl.getExtension("OES_texture_float") !== null,
    msaaSupported: gl.getContextAttributes()?.antialias ?? false,
    shadersCompile,
    keyExtensions: KEY_EXTENSIONS.map((name) => ({ name, supported: gl.getExtension(name) !== null })),
    allExtensions: gl.getSupportedExtensions() ?? [],
  };
}

// Both storage checks are real round-trips, not just "does the API exist" —
// they're keyed off the document's *origin*, and a `file://` document in
// this fork gets an opaque origin (see ../../components/url/origin.rs's
// `new_opaque_for_file`), for which the Storage Standard mandates a
// `SecurityError`/rejected promise for every storage shelf, localStorage
// and indexedDB alike. That's spec-mandated, not a bug this page could
// paper over — see IndexedDbButton.tsx/StorageButton.tsx for the standalone
// versions of these same two checks.
function testIndexedDb(): Promise<boolean> {
  return new Promise((resolve) => {
    if (!("indexedDB" in window)) {
      resolve(false);
      return;
    }

    const dbName = "roves-diagnostics-probe";
    const value = Date.now();

    try {
      const openRequest = indexedDB.open(dbName, 1);
      openRequest.onupgradeneeded = () => {
        openRequest.result.createObjectStore("kv");
      };
      openRequest.onerror = () => resolve(false);
      openRequest.onblocked = () => resolve(false);
      openRequest.onsuccess = () => {
        const db = openRequest.result;
        const writeTx = db.transaction("kv", "readwrite");
        writeTx.objectStore("kv").put(value, "probe");
        writeTx.onerror = () => {
          db.close();
          resolve(false);
        };
        writeTx.oncomplete = () => {
          const readRequest = db.transaction("kv", "readonly").objectStore("kv").get("probe");
          readRequest.onerror = () => {
            db.close();
            resolve(false);
          };
          readRequest.onsuccess = () => {
            const ok = readRequest.result === value;
            db.close();
            indexedDB.deleteDatabase(dbName);
            resolve(ok);
          };
        };
      };
    } catch {
      resolve(false);
    }
  });
}

async function probeCapabilities() {
  const localStorageOk = (() => {
    const key = "roves-diagnostics-probe";
    try {
      localStorage.setItem(key, "1");
      const ok = localStorage.getItem(key) === "1";
      localStorage.removeItem(key);
      return ok;
    } catch {
      return false;
    }
  })();

  // The real `@drincs/roves-api/steam`/`@drincs/roves-api/saves` wrappers —
  // see CUSTOMIZATIONS.md's "steam: protocol bridge"/"Save-game storage API"
  // entries for why both degrade to `false` instead of throwing when
  // unavailable (`isAvailable()` already swallows that itself on each). Only
  // `isAvailable()` here, not a full round-trip — see SavesButton.tsx for the
  // real write/read/list/delete check.
  const [indexedDbOk, steamAvailable, savesAvailable] = await Promise.all([
    testIndexedDb(),
    steam.isAvailable(),
    saves.isAvailable(),
  ]);

  return {
    audio: typeof AudioContext !== "undefined",
    gamepad: "getGamepads" in navigator,
    clipboard: typeof navigator.clipboard?.writeText === "function",
    fullscreen: document.fullscreenEnabled,
    localStorage: localStorageOk,
    indexedDb: indexedDbOk,
    steamAvailable,
    savesAvailable,
  };
}

function probeMemory() {
  const memory = (performance as Performance & { memory?: PerformanceMemory }).memory;
  return memory
    ? {
        usedJSHeapSize: memory.usedJSHeapSize,
        totalJSHeapSize: memory.totalJSHeapSize,
        jsHeapSizeLimit: memory.jsHeapSizeLimit,
      }
    : null;
}

// Real-frame-rate sample, decoupled from the PixiJS/Three.js panels above —
// this measures the page's own rAF/paint rate, so it still means something
// even if neither render test is currently toggled on.
function sampleFps(sampleMs = 500): Promise<number> {
  return new Promise((resolve) => {
    let frames = 0;
    const start = performance.now();
    const tick = () => {
      frames += 1;
      if (performance.now() - start >= sampleMs) {
        resolve(Math.round((frames * 1000) / sampleMs));
      } else {
        requestAnimationFrame(tick);
      }
    };
    requestAnimationFrame(tick);
  });
}

function detectPlatform(userAgent: string): string {
  if (/Windows/i.test(userAgent)) return "Windows";
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "macOS";
  if (/Android/i.test(userAgent)) return "Android";
  if (/iPhone|iPad|iOS/i.test(userAgent)) return "iOS";
  if (/Linux/i.test(userAgent)) return "Linux";
  return "Unknown";
}

function detectBrowserEngine(userAgent: string): string {
  // Servo's own UA string (components/config/prefs.rs's
  // `UserAgentPlatform::to_user_agent_string`) is Firefox-shaped — e.g.
  // "... rv:140.0) Servo/<version> Firefox/140.0" — so the "Servo/" token
  // has to be checked before the Gecko/Firefox fallback below, or this
  // build would misreport itself as plain Gecko.
  if (/Servo\//.test(userAgent)) return "Servo";
  if (/Gecko\/\d/.test(userAgent) || /Firefox\//.test(userAgent)) return "Gecko";
  if (/AppleWebKit/.test(userAgent) && !/Chrome|Chromium|Edg\//.test(userAgent)) return "WebKit";
  if (/Chrome|Chromium|Edg\//.test(userAgent)) return "Blink";
  return "Unknown";
}

async function buildDiagnosticsReport() {
  const [capabilities, fps] = await Promise.all([probeCapabilities(), sampleFps()]);
  const userAgent = navigator.userAgent;

  return {
    generatedAt: new Date().toISOString(),
    webgl: probeWebgl(),
    capabilities,
    resolution: {
      innerWidth: window.innerWidth,
      innerHeight: window.innerHeight,
      screenWidth: window.screen.width,
      screenHeight: window.screen.height,
      devicePixelRatio: window.devicePixelRatio,
    },
    memory: probeMemory(),
    fps,
    userAgent,
    platform: detectPlatform(userAgent),
    browserEngine: detectBrowserEngine(userAgent),
    reactVersion: REACT_VERSION,
    pixiJsVersion: PIXI_VERSION,
    threeVersion: THREE.REVISION,
  };
}

/**
 * Full diagnostics report as a single copy-pasteable JSON blob — same idea
 * as the parent project's own in-game diagnostics report, adapted to what
 * this standalone page actually has available (no `@drincs/pixi-vn`/
 * `tone`/`motion` here, since this isn't the real game). Kept as a plain
 * toggled view rather than a native `<dialog>`: `HTMLDialogElement`/
 * `showModal` support in this Servo fork isn't verified yet, and the report
 * is more useful reliably visible than gated behind a feature this page is
 * also implicitly testing.
 */
export default function DiagnosticsPanel() {
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<Awaited<ReturnType<typeof buildDiagnosticsReport>> | null>(null);
  const [copyStatus, setCopyStatus] = useState<string | null>(null);

  const refresh = async () => {
    setReport(null);
    setCopyStatus(null);
    setReport(await buildDiagnosticsReport());
  };

  const toggle = async () => {
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    await refresh();
  };

  const copyToClipboard = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
      setCopyStatus("Copied.");
    } catch (error) {
      setCopyStatus(`Copy failed — ${String(error)}`);
    }
  };

  return (
    <div>
      <button type="button" onClick={toggle}>
        {open ? "Hide" : "Show"} diagnostics report
      </button>

      {open && (
        <div
          style={{
            marginTop: "0.75rem",
            display: "flex",
            flexDirection: "column",
            gap: "0.5rem",
            alignItems: "center",
          }}
        >
          <div style={{ display: "flex", gap: "0.75rem" }}>
            <button type="button" onClick={refresh}>
              Refresh
            </button>
            <button type="button" onClick={copyToClipboard} disabled={!report}>
              Copy JSON
            </button>
            {copyStatus && <span>{copyStatus}</span>}
          </div>
          <pre
            style={{
              background: "#111",
              padding: "1rem",
              borderRadius: "6px",
              maxWidth: "90vw",
              maxHeight: "50vh",
              overflow: "auto",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              fontSize: "0.8rem",
              textAlign: "left",
            }}
          >
            {report ? JSON.stringify(report, null, 2) : "Generating..."}
          </pre>
        </div>
      )}
    </div>
  );
}
