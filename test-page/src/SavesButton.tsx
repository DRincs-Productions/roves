import { saves } from "@drincs/roves-api/saves";
import { useState } from "react";

/**
 * `saves:` protocol round-trip check, via the real `@drincs/roves-api/saves`
 * wrapper a game actually imports (same "test the real wrapper, not just the
 * raw protocol" reasoning as App.tsx's steam checks) — see
 * ../../CUSTOMIZATIONS.md's "Save-game storage API" entry for the full
 * design (where saves land, Steam Cloud mirroring, etc.).
 *
 * Exercises write → read → list → delete → read-after-delete, so a broken
 * step (e.g. delete not actually removing the file, or list not reflecting
 * it) shows up as a clear failure instead of a false "ok" from an
 * incomplete check.
 */
export default function SavesButton() {
  const [status, setStatus] = useState("Not tested.");

  const test = async () => {
    const key = "roves-test-page-saves-check";
    const value = `roves-saves-probe-${Date.now()}`;

    try {
      const available = await saves.isAvailable();
      if (!available) {
        setStatus("FAILED — saves.isAvailable() returned false");
        return;
      }

      const wrote = await saves.writeText(key, value);
      if (!wrote) {
        setStatus("FAILED — writeText() returned false");
        return;
      }

      const readBack = await saves.readText(key);
      if (readBack !== value) {
        setStatus(`FAILED — wrote "${value}", read back ${JSON.stringify(readBack)}`);
        return;
      }

      const keys = await saves.list();
      if (!keys.includes(key)) {
        setStatus(`FAILED — list() didn't include "${key}": ${JSON.stringify(keys)}`);
        return;
      }

      const deleted = await saves.delete(key);
      if (!deleted) {
        setStatus("FAILED — delete() returned false");
        return;
      }

      const afterDelete = await saves.readText(key);
      if (afterDelete !== null) {
        setStatus(`FAILED — read() after delete() should be null, got ${JSON.stringify(afterDelete)}`);
        return;
      }

      setStatus("ok — write/read/list/delete round-tripped correctly");
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={test}>
        Test saves: protocol (@drincs/roves-api)
      </button>
      <span>{status}</span>
    </div>
  );
}
