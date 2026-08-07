import { useState } from "react";

/**
 * localStorage round-trip check. A game needs to persist save data, and
 * this fork's `file://` origin stability fix (see ../../CUSTOMIZATIONS.md's
 * "Stable file:// origin" entry) is exactly what storage partitioning keys
 * off of — this is the simplest possible check that writes/reads land
 * instead of silently no-op'ing under a `file://` document.
 */
export default function StorageButton() {
  const [status, setStatus] = useState("Not tested.");

  const test = () => {
    const key = "roves-test-page-storage-check";
    const value = String(Date.now());
    try {
      localStorage.setItem(key, value);
      const readBack = localStorage.getItem(key);
      localStorage.removeItem(key);
      setStatus(
        readBack === value
          ? "ok — write/read/remove round-tripped correctly"
          : `FAILED — wrote "${value}", read back "${readBack}"`,
      );
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={test}>
        Test localStorage
      </button>
      <span>{status}</span>
    </div>
  );
}
