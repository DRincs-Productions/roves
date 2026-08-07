import { Application, Graphics, RendererType } from "pixi.js";
import { useEffect, useRef, useState } from "react";

/**
 * Minimal PixiJS smoke test: spins up a WebGL/WebGPU-backed `Application` on a
 * canvas and animates a shape. Exercises the same rendering path the real game
 * (`@drincs/pixi-vn`, built on PixiJS) depends on — if this doesn't render or
 * throws, Servo's canvas/WebGL support is the first thing to suspect.
 */
export default function PixiPanel() {
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState("Not started.");
  const [fps, setFps] = useState<number | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let app: Application | undefined;
    let cancelled = false;
    let fpsInterval: number | undefined;

    (async () => {
      try {
        app = new Application();
        await app.init({ width: 300, height: 200, background: "#111", antialias: true });
        if (cancelled) {
          app.destroy(true);
          return;
        }
        container.appendChild(app.canvas);

        const box = new Graphics().rect(-40, -40, 80, 80).fill(0x66ccff);
        box.position.set(150, 100);
        app.stage.addChild(box);
        app.ticker.add((ticker) => {
          box.rotation += 0.05 * ticker.deltaTime;
        });

        setStatus(`ok — renderer: ${app.renderer.type === RendererType.WEBGL ? "webgl" : "webgpu"}`);
        // Rendering without throwing doesn't rule out a crawling software
        // fallback (see TODO.md's GPU-verification item) — surface the
        // actual frame rate too, not just "it worked".
        fpsInterval = window.setInterval(() => setFps(app?.ticker.FPS ?? null), 500);
      } catch (error) {
        setStatus(`FAILED — ${String(error)}`);
      }
    })();

    return () => {
      cancelled = true;
      if (fpsInterval !== undefined) window.clearInterval(fpsInterval);
      app?.destroy(true);
    };
  }, []);

  return (
    <div>
      <p>
        PixiJS: {status}
        {fps !== null && ` (${fps.toFixed(0)} fps)`}
      </p>
      <div ref={containerRef} style={{ width: 300, height: 200 }} />
    </div>
  );
}
