import { useEffect, useState } from "react";

/**
 * Reads back which GPU/renderer WebGL is actually backed by — the check
 * TODO.md's "Verificare che la GPU venga usata correttamente" item asks for.
 * A canvas that renders without throwing can still be silently running on a
 * software rasterizer (llvmpipe/SwiftShader/WARP), which would tank a real
 * game's frame rate; this surfaces the actual renderer string instead of
 * just "it worked".
 */
export default function GpuInfoPanel() {
  const [info, setInfo] = useState("Probing...");

  useEffect(() => {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
    if (!gl) {
      setInfo("No WebGL context available at all.");
      return;
    }

    const maskedVendor = String(gl.getParameter(gl.VENDOR));
    const maskedRenderer = String(gl.getParameter(gl.RENDERER));
    const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
    const unmaskedVendor = debugInfo
      ? String(gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL))
      : "(WEBGL_debug_renderer_info unavailable)";
    const unmaskedRenderer = debugInfo
      ? String(gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL))
      : "(WEBGL_debug_renderer_info unavailable)";

    const looksLikeSoftware = /swiftshader|llvmpipe|software|warp/i.test(
      `${maskedRenderer} ${unmaskedRenderer}`,
    );

    setInfo(
      [
        `WebGL version: ${String(gl.getParameter(gl.VERSION))}`,
        `Vendor (masked): ${maskedVendor}`,
        `Renderer (masked): ${maskedRenderer}`,
        `Vendor (unmasked): ${unmaskedVendor}`,
        `Renderer (unmasked): ${unmaskedRenderer}`,
        looksLikeSoftware
          ? "⚠ looks like a SOFTWARE renderer, not real GPU acceleration"
          : "looks like real GPU acceleration",
      ].join("\n"),
    );
  }, []);

  return (
    <div>
      <p>GPU / WebGL info:</p>
      <pre
        style={{
          background: "#111",
          padding: "0.75rem",
          borderRadius: "6px",
          maxWidth: "90vw",
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
          fontSize: "0.85rem",
        }}
      >
        {info}
      </pre>
    </div>
  );
}
