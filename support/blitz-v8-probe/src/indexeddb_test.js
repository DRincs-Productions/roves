// IndexedDB subset (src/indexeddb.js), exercised with the calls of the
// visual-novel-template's narration-hooks bundle: game_db-shaped schema upgrade
// with indexes, auto-increment put, get/delete, ordered cursors over the store
// and an index with consumer-side pagination, reopen without a version, and
// persistence across processes. Requests complete from timer tasks, so main.rs
// drives frames until `indexedDbResult.done`. `indexedDbResult.runs` counts the
// processes that used the same storage directory; CI checks it reaches 2.
(() => {
  "use strict";
  const result = { done: false, step: "start", failures: [], runs: 0 };
  globalThis.indexedDbResult = result;
  const expect = (condition, message) => {
    if (!condition) result.failures.push(`${result.step}: ${message}`);
  };
  const request = (idbRequest) =>
    new Promise((resolve, reject) => {
      idbRequest.onsuccess = () => resolve(idbRequest.result);
      idbRequest.onerror = () => reject(idbRequest.error);
    });
  const complete = (transaction) => new Promise((resolve) => (transaction.oncomplete = resolve));
  // The consumer's cursor loop: iterate until null, paginating by itself.
  const page = (cursorRequest, offset, limit) =>
    new Promise((resolve, reject) => {
      const items = [];
      let index = 0;
      cursorRequest.onsuccess = () => {
        const cursor = cursorRequest.result;
        if (cursor === null) return resolve(items);
        if (index >= offset && items.length < limit) items.push(cursor.value.id);
        index++;
        cursor.continue();
      };
      cursorRequest.onerror = () => reject(cursorRequest.error);
    });
  const SCRATCH = "roves_probe_scratch";

  // The template's initialization, same shape.
  function openTemplateDatabase(version) {
    return new Promise((resolve, reject) => {
      const open = version === undefined ? indexedDB.open(SCRATCH) : indexedDB.open(SCRATCH, version);
      let upgrade = null;
      open.onupgradeneeded = (event) => {
        upgrade = { oldVersion: event.oldVersion, newVersion: event.newVersion };
        const db = open.result;
        if (!db.objectStoreNames.contains("saves")) {
          const store = db.createObjectStore("saves", { keyPath: "id", autoIncrement: true });
          store.createIndex("id", "id", { unique: true });
          store.createIndex("date", "date", { unique: false });
          store.createIndex("name", "name", { unique: false });
          store.createIndex("gameVersion", "gameVersion", { unique: false });
        }
      };
      open.onsuccess = () => resolve({ db: open.result, upgrade });
      open.onerror = () => reject(open.error);
    });
  }

  (async () => {
    result.step = "reset_scratch";
    // A previous process on the same storage directory left the scratch database.
    const stale = [];
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key.startsWith(`__roves_idb:1:${SCRATCH}`)) stale.push(key);
    }
    stale.forEach((key) => localStorage.removeItem(key));

    result.step = "upgrade";
    let synchronous = true;
    const pending = openTemplateDatabase(2);
    synchronous = false;
    const { db, upgrade } = await pending;
    expect(!synchronous, "open completes asynchronously");
    expect(upgrade?.oldVersion === 0 && upgrade.newVersion === 2, `upgrade versions ${JSON.stringify(upgrade)}`);
    expect(db.name === SCRATCH && db.version === 2 && db.objectStoreNames.contains("saves"), "database after upgrade");
    const schema = JSON.parse(localStorage.getItem(`__roves_idb:1:${SCRATCH}`) ?? "null");
    expect(schema?.version === 2 && schema.stores.saves.indexes.id.unique === true, "schema persisted");
    {
      const store = db.transaction(["saves"], "readonly").objectStore("saves");
      expect(store.keyPath === "id" && store.autoIncrement === true, "store keyPath/autoIncrement");
      expect([...store.indexNames].join() === "date,gameVersion,id,name", `indexNames ${[...store.indexNames]}`);
      expect(store.index("id").unique === true && store.index("date").unique === false, "index unique flags");
      const dateIndex = store.index("date");
      expect(dateIndex instanceof IDBIndex && !(dateIndex instanceof IDBObjectStore), "index() returns an IDBIndex");
      expect(dateIndex.objectStore === store && dateIndex.name === "date" && dateIndex.keyPath === "date", "IDBIndex links its store");
      expect(store.index("date") === dateIndex, "index() wrapper is cached per object store");
      expect(typeof dateIndex.openCursor === "function", "IDBIndex has openCursor");
      let threw = null;
      try {
        store.put({ name: "x" });
      } catch (error) {
        threw = error.name;
      }
      expect(threw === "ReadOnlyError", `readonly put: ${threw}`);
    }

    result.step = "put_get_delete";
    const day = (n) => new Date(Date.UTC(2026, 10, n));
    {
      // The template's write path: transaction([store], "readwrite").objectStore(store).put(record).
      const transaction = db.transaction(["saves"], "readwrite");
      const store = transaction.objectStore("saves");
      const record = { name: "b", date: day(3), gameVersion: "1.0.0", data: { flags: new Map([["met", true]]) } };
      const put = store.put(record);
      expect(put.readyState === "pending", "request pending before its task");
      record.name = "changed after put";
      const keys = [await request(put)];
      // Chained from promise continuations, as promise wrappers do.
      keys.push(await request(store.put({ name: "a", date: day(1), gameVersion: "1.0.0", id: undefined })));
      keys.push(await request(store.put({ name: "c", date: day(2), gameVersion: "1.0.1" })));
      keys.push(await request(store.put({ name: "d", date: day(4), gameVersion: "1.0.1" })));
      await complete(transaction);
      expect(keys.join() === "1,2,3,4", `generated keys ${keys}`);
      expect(record.id === undefined, "the page's object is not modified (the stored clone is)");
    }
    {
      const transaction = db.transaction(["saves"], "readwrite");
      const store = transaction.objectStore("saves");
      const first = await request(store.get(1));
      expect(first?.id === 1 && first.name === "b", `get(1) ${JSON.stringify(first)}`);
      expect(first?.date instanceof Date && first.date.getTime() === day(3).getTime(), "Date survives storage");
      expect(first?.data.flags instanceof Map && first.data.flags.get("met") === true, "Map survives storage");
      expect((await request(store.get(99))) === undefined, "missing key is undefined");
      // Overwrite an existing slot by id, then delete another.
      expect((await request(store.put({ ...first, name: "b2" }))) === 1, "put with existing id");
      await request(store.delete(4));
      expect((await request(store.get(4))) === undefined, "deleted record");
      const unique = store.put({ id: 9, name: "x" });
      expect(unique.readyState === "pending", "explicit key put");
      await request(unique);
      await request(store.delete(9));
      await complete(transaction);
    }

    result.step = "cursors";
    {
      // Closed and reopened without a version, as the template does.
      db.close();
      const { db: reopened, upgrade: again } = await openTemplateDatabase();
      expect(again === null && reopened.version === 2, `reopen version ${reopened.version}`);
      const store = reopened.transaction(["saves"], "readonly").objectStore("saves");
      expect((await page(store.openCursor(), 0, 10)).join() === "1,2,3", "store cursor next");
      const dateIndex = store.index("date");
      expect(dateIndex instanceof IDBIndex, "index() after reopen returns an IDBIndex");
      expect((await page(dateIndex.openCursor(null, "prev"), 0, 10)).join() === "1,3,2", "date index prev");
      expect((await page(store.index("date").openCursor(null, "next"), 1, 1)).join() === "3", "date index page 2");
      expect((await page(store.index("name").openCursor(null, "prev"), 0, 2)).join() === "3,1", "name index prev page");
      let threw = null;
      try {
        store.openCursor(1);
      } catch (error) {
        threw = error.name;
      }
      expect(threw === "NotSupportedError", `key range cursor: ${threw}`);
      // The new id after a delete keeps growing (key generator is persisted).
      const write = reopened.transaction(["saves"], "readwrite").objectStore("saves");
      expect((await request(write.put({ name: "e", date: day(5) }))) === 10, "generator after explicit id 9");
      reopened.close();
    }

    result.step = "process_persistence";
    {
      // Never deleted: each process adds one record to the same storage directory.
      const open = indexedDB.open("roves_probe_persist", 2);
      open.onupgradeneeded = () => {
        const names = open.result.objectStoreNames;
        if (!names.contains("runs")) open.result.createObjectStore("runs", { keyPath: "id", autoIncrement: true });
        if (!names.contains("slots")) open.result.createObjectStore("slots", { keyPath: "id" });
      };
      const db = await request(open);
      const transaction = db.transaction(["runs", "slots"], "readwrite");
      const store = transaction.objectStore("runs");
      const before = (await page(store.openCursor(), 0, Infinity)).length;
      const key = await request(store.put({ at: Date.now() }));
      // String keys containing the storage path separator (and "%") must
      // round-trip through a reload: from the second process on, this record
      // comes from storage rather than from the in-memory database.
      const slots = transaction.objectStore("slots");
      const SLOT = "slot/one%2F";
      if (before > 0) {
        const loaded = await request(slots.get(SLOT));
        expect(loaded?.id === SLOT && loaded.run === before, `slash key after reload ${JSON.stringify(loaded)}`);
        const cursorKeys = [];
        await new Promise((resolve, reject) => {
          const cursorRequest = slots.openCursor();
          cursorRequest.onsuccess = () => {
            const cursor = cursorRequest.result;
            if (cursor === null) return resolve();
            cursorKeys.push(cursor.key);
            cursor.continue();
          };
          cursorRequest.onerror = () => reject(cursorRequest.error);
        });
        expect(cursorKeys.length === 1 && cursorKeys[0] === SLOT, `slash key cursor ${JSON.stringify(cursorKeys)}`);
      }
      await request(slots.put({ id: SLOT, run: before + 1 }));
      await complete(transaction);
      expect(key === before + 1, `persisted key generator ${key} after ${before} records`);
      result.runs = before + 1;
      db.close();
    }
    result.step = "finished";
  })()
    .catch((error) => result.failures.push(`${result.step}: ${error?.name ?? ""} ${error?.message ?? error}`))
    .finally(() => (result.done = true));
})();
