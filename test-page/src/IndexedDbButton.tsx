import { useState } from "react";

/**
 * IndexedDB round-trip check, same idea as StorageButton's localStorage
 * check. Both are keyed off the document's *origin*, not the API's mere
 * existence — worth testing separately from `"indexedDB" in window`,
 * because a `file://` document in this fork gets an opaque origin (see
 * ../../components/url/origin.rs's `new_opaque_for_file`), and the Storage
 * Standard explicitly disallows any storage shelf — localStorage,
 * indexedDB, the lot — for opaque origins. That's a spec-mandated
 * `SecurityError`, not a bug this page's own code could work around.
 */
export default function IndexedDbButton() {
  const [status, setStatus] = useState("Not tested.");

  const test = () => {
    if (!("indexedDB" in window)) {
      setStatus("FAILED — indexedDB is not available in this build.");
      return;
    }

    const dbName = "roves-diagnostics-probe";
    const value = Date.now();

    try {
      const openRequest = indexedDB.open(dbName, 1);

      openRequest.onupgradeneeded = () => {
        openRequest.result.createObjectStore("kv");
      };

      openRequest.onerror = () => {
        setStatus(`FAILED — ${String(openRequest.error)}`);
      };

      openRequest.onblocked = () => {
        setStatus("FAILED — blocked (another connection to the probe database is open)");
      };

      openRequest.onsuccess = () => {
        const db = openRequest.result;
        const writeTx = db.transaction("kv", "readwrite");
        writeTx.objectStore("kv").put(value, "probe");

        writeTx.onerror = () => {
          db.close();
          setStatus(`FAILED — ${String(writeTx.error)}`);
        };

        writeTx.oncomplete = () => {
          const readTx = db.transaction("kv", "readonly");
          const readRequest = readTx.objectStore("kv").get("probe");

          readRequest.onerror = () => {
            db.close();
            setStatus(`FAILED — ${String(readRequest.error)}`);
          };

          readRequest.onsuccess = () => {
            const readBack = readRequest.result;
            db.close();
            indexedDB.deleteDatabase(dbName);
            setStatus(
              readBack === value
                ? "ok — write/read round-tripped correctly"
                : `FAILED — wrote ${value}, read back ${String(readBack)}`,
            );
          };
        };
      };
    } catch (error) {
      setStatus(`FAILED — ${String(error)}`);
    }
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <button type="button" onClick={test}>
        Test indexedDB
      </button>
      <span>{status}</span>
    </div>
  );
}
