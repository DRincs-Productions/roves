// Minimal IndexedDB for the visual-novel template's save store (loaded by dom.js),
// persisted through deno_webstorage 0.255's localStorage (SQLite in the probe's
// origin storage directory). deno_core 0.413, deno_web 0.291 and deno_webstorage
// provide no IndexedDB, and maintained polyfills either keep data only in memory
// (fake-indexeddb) or need a WebSQL backend (indexeddbshim); see the README.
//
// Only the calls made by the template's narration-hooks bundle are implemented:
//   indexedDB.open(name[, version]) with onupgradeneeded/onsuccess/onerror;
//   db.objectStoreNames.contains, db.createObjectStore(name, { keyPath, autoIncrement }),
//   db.transaction(stores, mode), db.close();
//   store.createIndex(name, keyPath, { unique }), store.index(name), store.indexNames,
//   store.put(value), get(key), delete(key), openCursor([null[, direction]]);
//   index.openCursor([null[, direction]]); cursor.key/primaryKey/value/continue().
// Everything else throws NotSupportedError (or is absent). Keys are numbers,
// strings and Dates; key paths are single property names (in-line keys only).
//
// Storage layout (keys start with "__roves_idb:1:"):
//   "__roves_idb:1:<db>"                  JSON schema: version, stores, key generators
//   "__roves_idb:1:<db>/<store>/<key>"    one record, V8 structured-serialized + base64
// Each put/delete is written to localStorage when its request runs, so a
// transaction is not atomic and is never rolled back (see the README).
// Requests of all transactions run in creation order, one timer task each, on
// the V8 thread; their events fire on the request only (no bubbling).
globalThis.__rovesCreateIndexedDB = (dependencies) => {
  "use strict";
  const { storage, serialize, deserialize, atob, btoa, Event, EventTarget, DOMException, setIsTrusted, setTimeout } =
    dependencies;

  const INTERNAL = Symbol("internal");
  const PREFIX = "__roves_idb:1:";
  const slots = new WeakMap();
  const illegalConstructor = () => {
    throw new TypeError("Illegal constructor");
  };
  const domError = (name, message) => new DOMException(message, name);
  const unsupported = (what) => domError("NotSupportedError", `${what} is not supported by this IndexedDB subset`);
  function slot(object) {
    const record = slots.get(object);
    if (record === undefined) throw new TypeError("Illegal invocation");
    return record;
  }

  // ---- keys: numbers, strings and Dates ---------------------------------------

  function toKey(input) {
    if (typeof input === "number") return Number.isNaN(input) ? undefined : input;
    if (typeof input === "string") return input;
    if (input instanceof Date) {
      const time = input.getTime();
      return Number.isNaN(time) ? undefined : new Date(time);
    }
    return undefined;
  }
  function requireKey(input) {
    const key = toKey(input);
    if (key === undefined) throw domError("DataError", "The parameter is not a valid key");
    return key;
  }
  // IndexedDB order: numbers < Dates < strings.
  const rank = (key) => (typeof key === "number" ? 0 : typeof key === "string" ? 2 : 1);
  function compareKeys(a, b) {
    const order = rank(a) - rank(b);
    if (order !== 0) return Math.sign(order);
    const valueA = a instanceof Date ? a.getTime() : a;
    const valueB = b instanceof Date ? b.getTime() : b;
    return valueA === valueB ? 0 : valueA > valueB ? 1 : -1;
  }
  // The encoded key is the last segment of a "<database>/<store>/<key>" storage
  // name, so string keys escape "%" and "/" (and nothing else: lone surrogates,
  // valid in keys, would make encodeURIComponent throw). Decoding is one pass.
  const escapeKey = (text) => text.replace(/[%/]/g, (char) => (char === "%" ? "%25" : "%2F"));
  const unescapeKey = (text) => text.replace(/%(25|2F)/g, (match) => (match === "%25" ? "%" : "/"));
  const encodeKey = (key) =>
    typeof key === "number" ? `n${key}` : typeof key === "string" ? `s${escapeKey(key)}` : `d${key.getTime()}`;
  function decodeKey(text) {
    const rest = text.slice(1);
    return text[0] === "n" ? Number(rest) : text[0] === "s" ? unescapeKey(rest) : new Date(Number(rest));
  }
  const copyKey = (key) => (key instanceof Date ? new Date(key.getTime()) : key);

  function checkKeyPath(keyPath) {
    if (typeof keyPath !== "string" || !/^[$A-Z_a-z][$\w]*$/u.test(keyPath)) {
      throw unsupported("A key path other than a single property name");
    }
    return keyPath;
  }
  // Key at `keyPath` in `value`, or undefined when absent or not a valid key.
  function keyAt(value, keyPath) {
    if (value === null || typeof value !== "object" || !Object.prototype.hasOwnProperty.call(value, keyPath)) {
      return undefined;
    }
    return toKey(value[keyPath]);
  }

  // ---- values (V8 structured serialization) -----------------------------------

  function serializeValue(value) {
    try {
      return serialize(value, { forStorage: true }, (message) => {
        throw domError("DataCloneError", message);
      });
    } catch (error) {
      if (error instanceof DOMException) throw error;
      throw domError("DataCloneError", String(error?.message ?? error));
    }
  }
  function toBase64(bytes) {
    let binary = "";
    for (let index = 0; index < bytes.length; index += 0x8000) {
      binary += String.fromCharCode.apply(null, bytes.subarray(index, index + 0x8000));
    }
    return btoa(binary);
  }
  function decodeValue(data) {
    const binary = atob(data);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
    return deserialize(bytes, { forStorage: true });
  }

  // ---- events -------------------------------------------------------------------

  class IDBVersionChangeEvent extends Event {
    #oldVersion;
    #newVersion;
    constructor(type, init = {}) {
      super(type, init);
      this.#oldVersion = Number(init.oldVersion ?? 0);
      this.#newVersion = init.newVersion === undefined || init.newVersion === null ? null : Number(init.newVersion);
    }
    get oldVersion() {
      return this.#oldVersion;
    }
    get newVersion() {
      return this.#newVersion;
    }
  }
  // `on<type>` event handler attributes (same approach as src/web_audio.js).
  const handlers = new WeakMap();
  function defineEventHandlers(prototype, types) {
    for (const type of types) {
      Object.defineProperty(prototype, `on${type}`, {
        configurable: true,
        enumerable: true,
        get() {
          return handlers.get(this)?.[type] ?? null;
        },
        set(value) {
          let handlerSlots = handlers.get(this);
          if (handlerSlots === undefined) {
            handlerSlots = {};
            handlers.set(this, handlerSlots);
          }
          if (!(type in handlerSlots)) {
            this.addEventListener(type, (event) => {
              const handler = handlerSlots[type];
              if (typeof handler === "function") return handler.call(this, event);
            });
          }
          handlerSlots[type] = typeof value === "function" ? value : null;
        },
      });
    }
  }
  function fire(target, event) {
    setIsTrusted(event, true);
    target.dispatchEvent(event);
  }

  // ---- task queue: one timer task per request, in creation order ---------------

  const jobs = [];
  let scheduled = false;
  function schedule(job) {
    jobs.push(job);
    if (!scheduled) {
      scheduled = true;
      setTimeout(runJob, 0);
    }
  }
  function runJob() {
    scheduled = false;
    try {
      jobs.shift()();
    } catch (error) {
      globalThis.console?.error("IndexedDB internal error", error);
    } finally {
      if (jobs.length > 0 && !scheduled) {
        scheduled = true;
        setTimeout(runJob, 0);
      }
    }
  }

  // ---- persistent database state -----------------------------------------------

  const metaKey = (name) => PREFIX + encodeURIComponent(name);
  const recordKey = (database, store, encoded) =>
    `${metaKey(database.name)}/${encodeURIComponent(store.name)}/${encoded}`;
  const databases = new Map(); // name -> { name, version, stores: Map, upgrade }

  function indexKeys(store, value) {
    const keys = new Map();
    for (const [name, index] of store.indexes) keys.set(name, keyAt(value, index.keyPath));
    return keys;
  }
  function loadDatabase(name) {
    let database = databases.get(name);
    if (database !== undefined) return database;
    database = { name, version: 0, stores: new Map(), upgrade: null };
    const local = storage();
    const raw = local.getItem(metaKey(name));
    if (raw === null) {
      databases.set(name, database);
      return database;
    }
    const schema = JSON.parse(raw);
    database.version = schema.version;
    for (const [storeName, info] of Object.entries(schema.stores)) {
      database.stores.set(storeName, {
        name: storeName,
        keyPath: info.keyPath,
        autoIncrement: info.autoIncrement,
        current: info.current,
        indexes: new Map(Object.entries(info.indexes)),
        records: new Map(), // encoded key -> { key, data, indexKeys }
      });
    }
    const prefix = `${metaKey(name)}/`;
    for (let position = 0; position < local.length; position++) {
      const storageKey = local.key(position);
      if (!storageKey?.startsWith(prefix)) continue;
      const rest = storageKey.slice(prefix.length);
      const slash = rest.indexOf("/");
      const store = database.stores.get(decodeURIComponent(rest.slice(0, slash)));
      if (store === undefined) continue;
      const encoded = rest.slice(slash + 1);
      const data = local.getItem(storageKey);
      const keys = store.indexes.size > 0 ? indexKeys(store, decodeValue(data)) : new Map();
      store.records.set(encoded, { key: decodeKey(encoded), data, indexKeys: keys });
    }
    // Cached only once fully read, so a failed load is retried by the next open().
    databases.set(name, database);
    return database;
  }
  function saveMeta(database) {
    const stores = {};
    for (const [name, store] of database.stores) {
      stores[name] = {
        keyPath: store.keyPath,
        autoIncrement: store.autoIncrement,
        current: store.current,
        indexes: Object.fromEntries(store.indexes),
      };
    }
    storage().setItem(metaKey(database.name), JSON.stringify({ version: database.version, stores }));
  }
  const nameList = (names) => {
    const list = [...names].sort();
    return Object.freeze(Object.assign(list, { contains: (name) => list.includes(String(name)), item: (i) => list[i] ?? null }));
  };

  // ---- requests -----------------------------------------------------------------

  class IDBRequest extends EventTarget {
    constructor(token, source, transaction) {
      if (token !== INTERNAL) illegalConstructor();
      super();
      slots.set(this, { source, transaction, readyState: "pending", result: undefined, error: null });
    }
    get result() {
      const request = slot(this);
      if (request.readyState !== "done") throw domError("InvalidStateError", "The request has not finished");
      return request.result;
    }
    get error() {
      const request = slot(this);
      if (request.readyState !== "done") throw domError("InvalidStateError", "The request has not finished");
      return request.error;
    }
    get source() {
      return slot(this).source;
    }
    get transaction() {
      return slot(this).transaction;
    }
    get readyState() {
      return slot(this).readyState;
    }
  }
  defineEventHandlers(IDBRequest.prototype, ["success", "error"]);
  class IDBOpenDBRequest extends IDBRequest {}
  defineEventHandlers(IDBOpenDBRequest.prototype, ["upgradeneeded"]);

  function settle(request, result, error) {
    const state = slot(request);
    state.readyState = "done";
    state.result = error === null ? result : undefined;
    state.error = error;
    if (error === null) fire(request, new Event("success"));
    else fire(request, new Event("error", { bubbles: true, cancelable: true }));
  }
  // Queues `operation` as a request of `transaction`: its result or DOMException
  // settles the request in its own task.
  function addRequest(transaction, request, operation) {
    const state = slot(transaction);
    if (state.finished) throw domError("TransactionInactiveError", "The transaction has finished");
    state.pending++;
    slot(request).readyState = "pending";
    schedule(() => {
      let result;
      let error = null;
      try {
        result = operation();
      } catch (thrown) {
        error = thrown instanceof DOMException ? thrown : domError("UnknownError", String(thrown?.message ?? thrown));
      }
      settle(request, result, error);
      if (--state.pending === 0) schedule(() => finishTransaction(transaction));
    });
    return request;
  }

  // ---- transactions -------------------------------------------------------------

  class IDBTransaction extends EventTarget {
    constructor(token, connection, scope, mode, onFinish) {
      if (token !== INTERNAL) illegalConstructor();
      super();
      slots.set(this, { connection, scope, mode, onFinish, pending: 0, finished: false, stores: new Map() });
      schedule(() => finishTransaction(this));
    }
    get db() {
      return slot(this).connection;
    }
    get mode() {
      return slot(this).mode;
    }
    objectStore(name) {
      const state = slot(this);
      name = String(name);
      if (state.finished) throw domError("InvalidStateError", "The transaction has finished");
      const store = slot(state.connection).database.stores.get(name);
      if (store === undefined || (state.scope !== null && !state.scope.includes(name))) {
        throw domError("NotFoundError", `No object store named "${name}" in this transaction`);
      }
      let object = state.stores.get(name);
      if (object === undefined) {
        object = new IDBObjectStore(INTERNAL, store, this);
        state.stores.set(name, object);
      }
      return object;
    }
  }
  defineEventHandlers(IDBTransaction.prototype, ["complete"]);
  // A transaction completes once no request is pending after its last task.
  function finishTransaction(transaction) {
    const state = slot(transaction);
    if (state.finished || state.pending > 0) return;
    state.finished = true;
    fire(transaction, new Event("complete"));
    state.onFinish?.();
  }
  function requireWritable(transaction) {
    const state = slot(transaction);
    if (state.mode === "readonly") throw domError("ReadOnlyError", "The transaction is read-only");
    if (state.finished) throw domError("TransactionInactiveError", "The transaction has finished");
  }
  function requireUpgrade(transaction) {
    const state = slot(transaction);
    if (state.mode !== "versionchange" || state.finished) {
      throw domError("InvalidStateError", "Schema changes need a running upgrade transaction");
    }
    return state;
  }

  // ---- cursors ------------------------------------------------------------------

  class IDBCursorWithValue {
    constructor(token, request, source, direction) {
      if (token !== INTERNAL) illegalConstructor();
      slots.set(this, { request, source, direction, key: undefined, primaryKey: undefined, value: undefined, gotValue: false });
    }
    get source() {
      return slot(this).source;
    }
    get direction() {
      return slot(this).direction;
    }
    get key() {
      return copyKey(slot(this).key);
    }
    get primaryKey() {
      return copyKey(slot(this).primaryKey);
    }
    get value() {
      return slot(this).value;
    }
    get request() {
      return slot(this).request;
    }
    continue(key) {
      if (key !== undefined) throw unsupported("continue(key)");
      const cursor = slot(this);
      if (!cursor.gotValue) throw domError("InvalidStateError", "The cursor is advancing or finished");
      cursor.gotValue = false;
      addRequest(slot(cursor.request).transaction, cursor.request, () => stepCursor(this));
    }
  }
  // Sorted [key, primaryKey, record] entries of the cursor's store or index.
  function cursorEntries(cursor) {
    const source = slot(cursor.source);
    const entries = [];
    for (const record of source.store.records.values()) {
      const key = source.index === undefined ? record.key : record.indexKeys.get(source.index);
      if (key !== undefined) entries.push([key, record.key, record]);
    }
    return entries.sort((a, b) => compareKeys(a[0], b[0]) || compareKeys(a[1], b[1]));
  }
  // Moves to the entry after (next) or before (prev) the current position; the
  // records are read at each step, so changes made meanwhile are seen.
  function stepCursor(cursorObject) {
    const cursor = slot(cursorObject);
    const entries = cursorEntries(cursor);
    const position = (entry) =>
      cursor.key === undefined ? 1 : compareKeys(entry[0], cursor.key) || compareKeys(entry[1], cursor.primaryKey);
    const found =
      cursor.direction === "next"
        ? entries.find((entry) => position(entry) > 0)
        : entries.findLast((entry) => cursor.key === undefined || position(entry) < 0);
    if (found === undefined) {
      cursor.key = cursor.primaryKey = cursor.value = undefined;
      return null;
    }
    [cursor.key, cursor.primaryKey] = found;
    cursor.value = decodeValue(found[2].data);
    cursor.gotValue = true;
    return cursorObject;
  }
  function openCursor(source, transaction, query, direction) {
    if (query !== undefined && query !== null) throw unsupported("A cursor key or key range");
    direction = String(direction);
    if (direction === "nextunique" || direction === "prevunique") throw unsupported(`Cursor direction "${direction}"`);
    if (direction !== "next" && direction !== "prev") throw new TypeError(`Invalid cursor direction "${direction}"`);
    const request = new IDBRequest(INTERNAL, source, transaction);
    const cursor = new IDBCursorWithValue(INTERNAL, request, source, direction);
    return addRequest(transaction, request, () => stepCursor(cursor));
  }

  // ---- object stores and indexes --------------------------------------------------

  class IDBIndex {
    constructor(token, store, name, objectStore) {
      if (token !== INTERNAL) illegalConstructor();
      slots.set(this, { store, index: name, objectStore });
    }
    get name() {
      return slot(this).index;
    }
    get objectStore() {
      return slot(this).objectStore;
    }
    get keyPath() {
      const index = slot(this);
      return index.store.indexes.get(index.index).keyPath;
    }
    get unique() {
      const index = slot(this);
      return index.store.indexes.get(index.index).unique;
    }
    openCursor(query, direction = "next") {
      const index = slot(this);
      return openCursor(this, slot(index.objectStore).transaction, query, direction);
    }
  }

  class IDBObjectStore {
    constructor(token, store, transaction) {
      if (token !== INTERNAL) illegalConstructor();
      slots.set(this, { store, transaction, indexes: new Map() });
    }
    get name() {
      return slot(this).store.name;
    }
    get keyPath() {
      return slot(this).store.keyPath;
    }
    get autoIncrement() {
      return slot(this).store.autoIncrement;
    }
    get indexNames() {
      return nameList(slot(this).store.indexes.keys());
    }
    get transaction() {
      return slot(this).transaction;
    }
    createIndex(name, keyPath, options = {}) {
      const { store, transaction } = slot(this);
      requireUpgrade(transaction);
      name = String(name);
      if (store.indexes.has(name)) throw domError("ConstraintError", `Index "${name}" already exists`);
      checkKeyPath(keyPath);
      if (options?.multiEntry) throw unsupported("multiEntry");
      const unique = Boolean(options?.unique);
      const keys = new Map();
      for (const [encoded, record] of store.records) {
        const key = keyAt(decodeValue(record.data), keyPath);
        if (unique && key !== undefined && [...keys.values()].some((other) => compareKeys(other, key) === 0)) {
          throw domError("ConstraintError", `Existing records violate the unique index "${name}"`);
        }
        keys.set(encoded, key);
      }
      store.indexes.set(name, { keyPath, unique });
      for (const [encoded, record] of store.records) record.indexKeys.set(name, keys.get(encoded));
      return this.index(name);
    }
    index(name) {
      const objectStore = slot(this);
      name = String(name);
      if (slot(objectStore.transaction).finished) throw domError("InvalidStateError", "The transaction has finished");
      if (!objectStore.store.indexes.has(name)) throw domError("NotFoundError", `No index named "${name}"`);
      let index = objectStore.indexes.get(name);
      if (index === undefined) {
        index = new IDBIndex(INTERNAL, objectStore.store, name, this);
        objectStore.indexes.set(name, index);
      }
      return index;
    }
    put(value, key) {
      const { store, transaction } = slot(this);
      requireWritable(transaction);
      if (key !== undefined) throw unsupported("An out-of-line key");
      // The value is cloned now, so later changes by the page are not stored.
      const bytes = serializeValue(value);
      const clone = deserialize(bytes, { forStorage: true });
      // A missing or undefined property means "generate a key" (key generator only).
      const isObject = clone !== null && typeof clone === "object";
      const raw = isObject && Object.prototype.hasOwnProperty.call(clone, store.keyPath) ? clone[store.keyPath] : undefined;
      let primaryKey;
      if (raw !== undefined || !store.autoIncrement) {
        primaryKey = toKey(raw);
        if (primaryKey === undefined) throw domError("DataError", `The value has no valid key at "${store.keyPath}"`);
      } else if (!isObject) {
        throw domError("DataError", "A generated key needs an object value");
      }
      const database = slot(slot(transaction).connection).database;
      const request = new IDBRequest(INTERNAL, this, transaction);
      return addRequest(transaction, request, () => {
        let data;
        let keyNow = primaryKey;
        if (keyNow === undefined) {
          if (store.current > 2 ** 53) throw domError("ConstraintError", "The key generator is exhausted");
          keyNow = store.current;
          clone[store.keyPath] = keyNow;
          data = toBase64(serializeValue(clone));
        } else {
          data = toBase64(bytes);
        }
        const encoded = encodeKey(keyNow);
        const keys = indexKeys(store, clone);
        for (const [name, index] of store.indexes) {
          const indexKey = keys.get(name);
          if (!index.unique || indexKey === undefined) continue;
          for (const [otherEncoded, other] of store.records) {
            const otherKey = other.indexKeys.get(name);
            if (otherEncoded !== encoded && otherKey !== undefined && compareKeys(otherKey, indexKey) === 0) {
              throw domError("ConstraintError", `The value violates the unique index "${name}"`);
            }
          }
        }
        // The key generator is written first: if the record write then fails
        // (e.g. QuotaExceededError) the only effect is a skipped generated key.
        if (store.autoIncrement && typeof keyNow === "number" && keyNow >= store.current) {
          store.current = Math.floor(keyNow) + 1;
          saveMeta(database);
        }
        storage().setItem(recordKey(database, store, encoded), data);
        store.records.set(encoded, { key: keyNow, data, indexKeys: keys });
        return copyKey(keyNow);
      });
    }
    get(key) {
      const { store, transaction } = slot(this);
      const primaryKey = requireKey(key);
      const request = new IDBRequest(INTERNAL, this, transaction);
      return addRequest(transaction, request, () => {
        const record = store.records.get(encodeKey(primaryKey));
        return record === undefined ? undefined : decodeValue(record.data);
      });
    }
    delete(key) {
      const { store, transaction } = slot(this);
      requireWritable(transaction);
      const primaryKey = requireKey(key);
      const database = slot(slot(transaction).connection).database;
      const request = new IDBRequest(INTERNAL, this, transaction);
      return addRequest(transaction, request, () => {
        const encoded = encodeKey(primaryKey);
        storage().removeItem(recordKey(database, store, encoded));
        store.records.delete(encoded);
        return undefined;
      });
    }
    openCursor(query, direction = "next") {
      return openCursor(this, slot(this).transaction, query, direction);
    }
  }

  // ---- connections and the factory ------------------------------------------------

  class IDBDatabase extends EventTarget {
    constructor(token, database) {
      if (token !== INTERNAL) illegalConstructor();
      super();
      slots.set(this, { database, version: database.version, closed: false, upgrade: null });
    }
    get name() {
      return slot(this).database.name;
    }
    get version() {
      return slot(this).version;
    }
    get objectStoreNames() {
      return nameList(slot(this).database.stores.keys());
    }
    createObjectStore(name, options = {}) {
      const connection = slot(this);
      if (connection.upgrade === null) {
        throw domError("InvalidStateError", "Schema changes need a running upgrade transaction");
      }
      requireUpgrade(connection.upgrade);
      name = String(name);
      const { database } = connection;
      if (database.stores.has(name)) throw domError("ConstraintError", `Object store "${name}" already exists`);
      const keyPath = checkKeyPath(options?.keyPath);
      const store = {
        name,
        keyPath,
        autoIncrement: Boolean(options?.autoIncrement),
        current: 1,
        indexes: new Map(),
        records: new Map(),
      };
      database.stores.set(name, store);
      return connection.upgrade.objectStore(name);
    }
    transaction(storeNames, mode = "readonly") {
      const connection = slot(this);
      if (connection.closed) throw domError("InvalidStateError", "The connection is closed");
      if (connection.database.upgrade !== null) throw domError("InvalidStateError", "An upgrade is running");
      if (mode !== "readonly" && mode !== "readwrite") throw new TypeError(`Invalid transaction mode "${mode}"`);
      const scope = [...new Set(typeof storeNames === "string" ? [storeNames] : Array.from(storeNames, String))];
      if (scope.length === 0) throw domError("InvalidAccessError", "A transaction needs at least one object store");
      for (const name of scope) {
        if (!connection.database.stores.has(name)) throw domError("NotFoundError", `No object store named "${name}"`);
      }
      return new IDBTransaction(INTERNAL, this, scope, mode, null);
    }
    close() {
      slot(this).closed = true;
    }
  }
  class IDBFactory {
    constructor(token) {
      if (token !== INTERNAL) illegalConstructor();
    }
    open(name, version) {
      name = String(name);
      if (version !== undefined) {
        version = Math.trunc(Number(version));
        if (!(version >= 1 && version <= Number.MAX_SAFE_INTEGER)) throw new TypeError("Invalid database version");
      }
      const request = new IDBOpenDBRequest(INTERNAL, null, null);
      const run = () => {
        let database;
        try {
          database = loadDatabase(name);
        } catch (error) {
          return settle(request, undefined, domError("UnknownError", `Unreadable database: ${error?.message ?? error}`));
        }
        // Another connection's upgrade must finish first.
        if (database.upgrade !== null) return schedule(run);
        const oldVersion = database.version;
        const newVersion = version ?? Math.max(oldVersion, 1);
        if (newVersion < oldVersion) {
          return settle(request, undefined, domError("VersionError", `Database "${name}" is at version ${oldVersion}`));
        }
        if (newVersion === oldVersion) return settle(request, new IDBDatabase(INTERNAL, database), null);
        database.version = newVersion;
        const connectionObject = new IDBDatabase(INTERNAL, database);
        const connection = slot(connectionObject);
        const state = slot(request);
        const transaction = new IDBTransaction(INTERNAL, connectionObject, null, "versionchange", () => {
          database.upgrade = connection.upgrade = state.transaction = null;
          try {
            saveMeta(database);
          } catch (error) {
            return settle(request, undefined, error instanceof DOMException ? error : domError("UnknownError", String(error)));
          }
          settle(request, connectionObject, null);
        });
        database.upgrade = connection.upgrade = state.transaction = transaction;
        state.readyState = "done";
        state.result = connectionObject;
        fire(request, new IDBVersionChangeEvent("upgradeneeded", { oldVersion, newVersion }));
      };
      schedule(run);
      return request;
    }
  }

  const indexedDB = new IDBFactory(INTERNAL);
  return {
    indexedDB,
    globals: {
      IDBFactory,
      IDBDatabase,
      IDBTransaction,
      IDBObjectStore,
      IDBIndex,
      IDBCursorWithValue,
      IDBRequest,
      IDBOpenDBRequest,
      IDBVersionChangeEvent,
    },
  };
};
