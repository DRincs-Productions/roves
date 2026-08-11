import { clearContentCache } from "@drincs/roves-api/cache";
import { useState } from "react";

/**
 * Exercises `@drincs/roves-api/cache`'s `clearContentCache()` — the real,
 * destructive `roves:clear_content_cache` command. Guarded behind a
 * confirm() like the quit button in App.tsx, for the same reason: this
 * cache directory is the live document root while the game runs, so
 * clearing it also closes the window (see that module's own doc comment).
 */
export default function ClearCacheButton() {
  const [status, setStatus] = useState<string | null>(null);

  const clearCache = async () => {
    if (
      !window.confirm(
        "This calls @drincs/roves-api/cache's clearContentCache() — it deletes the startup " +
          "extraction cache (not save data) and closes this window. Continue?",
      )
    ) {
      return;
    }
    try {
      await clearContentCache();
    } catch (error) {
      setStatus(`clearContentCache() FAILED: ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={clearCache}>
        Clear extraction cache (@drincs/roves-api/cache)
      </button>
      {status && <span>{status}</span>}
    </div>
  );
}
