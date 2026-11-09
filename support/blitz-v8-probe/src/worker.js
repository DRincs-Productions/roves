// Dedicated Worker for games (loaded by dom.js; threads and isolates in
// src/worker.rs, the worker's global scope in src/worker_scope.js).
//
// - Classic scripts only; `type: "module"` throws TypeError (module workers are
//   not implemented). The script comes from a blob: URL of this page (deno_web's
//   blob store), a data: URL or a game://content/ file (the probe's fetch); a
//   failed fetch or a non-JavaScript MIME type fires `error` (Event) at the Worker.
// - Messages: V8 structured serialization; transfer lists are unsupported
//   (DataCloneError); undeserializable data fires `messageerror`.
// - Worker events are delivered by the frame loop's event loop poll; the receive
//   promise is unref'd, so a live worker never keeps the probe alive by itself.
// - An uncaught worker error fires a cancelable ErrorEvent at the Worker; if not
//   cancelled it is reported on the page's global as well.
globalThis.__rovesCreateWorker = (dependencies) => {
  "use strict";
  const {
    core,
    webidl,
    EventTarget,
    ErrorEvent,
    MessageEvent,
    Event,
    DOMException,
    setIsTrusted,
    defineEventHandler,
    URL,
    blobFromObjectUrl,
    fetch,
    baseUrl,
  } = dependencies;
  const {
    op_worker_start,
    op_worker_post,
    op_worker_next_event,
    op_worker_take_message,
    op_worker_take_error,
    op_worker_terminate,
  } = core.ops;
  const EVENT_MESSAGE = 1;
  const EVENT_ERROR = 2;
  const JAVASCRIPT_MIME_TYPES = new Set([
    "text/javascript", "application/javascript", "application/ecmascript", "application/x-ecmascript",
    "application/x-javascript", "text/ecmascript", "text/javascript1.0", "text/javascript1.1",
    "text/javascript1.2", "text/javascript1.3", "text/javascript1.4", "text/javascript1.5",
    "text/jscript", "text/livescript", "text/x-ecmascript", "text/x-javascript",
  ]);
  const isJavaScriptMime = (type) => JAVASCRIPT_MIME_TYPES.has(String(type).split(";")[0].trim().toLowerCase());

  const cloneError = (message) => {
    throw new DOMException(message, "DataCloneError");
  };
  function serializeMessage(message, transferOrOptions) {
    const transfer = transferOrOptions == null
      ? []
      : typeof transferOrOptions[Symbol.iterator] === "function"
      ? [...transferOrOptions]
      : [...(transferOrOptions.transfer ?? [])];
    if (transfer.length > 0) cloneError("transferring objects to or from this worker is not supported");
    return core.serialize(message, undefined, cloneError);
  }

  // Classic worker script: source text, or null for a network error.
  async function fetchScript(url) {
    if (url.protocol === "blob:") {
      const blob = blobFromObjectUrl(url.href);
      if (blob === null || !isJavaScriptMime(blob.type)) return null;
      return blob.text();
    }
    // Worker scripts must be same-origin: never from a remote (CORS) origin.
    if (url.protocol !== "data:" && url.protocol !== "game:") return null;
    try {
      const response = await fetch(url.href);
      if (!response.ok || !isJavaScriptMime(response.headers.get("content-type") ?? "")) return null;
      return response.text();
    } catch {
      return null;
    }
  }

  function fire(target, event) {
    setIsTrusted(event, true);
    return target.dispatchEvent(event);
  }

  const states = new WeakMap();
  class Worker extends EventTarget {
    constructor(scriptURL, options = undefined) {
      const prefix = "Failed to construct 'Worker'";
      webidl.requiredArguments(arguments.length, 1, prefix);
      const href = webidl.converters.USVString(scriptURL, prefix, "Argument 1");
      const type = options?.type === undefined ? "classic" : String(options.type);
      if (type !== "classic" && type !== "module") {
        throw new TypeError(`${prefix}: '${type}' is not a valid WorkerType`);
      }
      if (type === "module") throw new TypeError(`${prefix}: module workers are not supported`);
      const name = options?.name === undefined ? "" : String(options.name);
      let url;
      try {
        url = new URL(href, baseUrl());
      } catch {
        throw new DOMException(`${prefix}: The URL '${href}' is invalid.`, "SyntaxError");
      }
      super();
      const state = { id: 0, terminated: false, queue: [] };
      states.set(this, state);
      start(this, state, url, name);
    }
    postMessage(message, transfer = undefined) {
      webidl.requiredArguments(arguments.length, 1, "Failed to execute 'postMessage' on 'Worker'");
      const state = states.get(this);
      if (state === undefined) throw new TypeError("Illegal invocation");
      const bytes = serializeMessage(message, transfer);
      if (state.terminated) return;
      if (state.id === 0) state.queue.push(bytes);
      else op_worker_post(state.id, bytes);
    }
    terminate() {
      const state = states.get(this);
      if (state === undefined) throw new TypeError("Illegal invocation");
      if (state.terminated) return;
      state.terminated = true;
      state.queue = [];
      if (state.id !== 0) op_worker_terminate(state.id);
    }
  }
  defineEventHandler(Worker.prototype, "message");
  defineEventHandler(Worker.prototype, "messageerror");
  defineEventHandler(Worker.prototype, "error");

  async function start(worker, state, url, name) {
    const source = await fetchScript(url);
    if (state.terminated) return;
    if (source === null) {
      state.terminated = true;
      fire(worker, new Event("error"));
      return;
    }
    try {
      state.id = op_worker_start(source, url.href, name);
    } catch (error) {
      state.terminated = true;
      core.print(`Worker could not start: ${error}\n`, true);
      fire(worker, new Event("error"));
      return;
    }
    for (const bytes of state.queue) op_worker_post(state.id, bytes);
    state.queue = [];
    for (;;) {
      const promise = op_worker_next_event(state.id);
      core.unrefOpPromise(promise);
      const kind = await promise;
      if (state.terminated) return;
      if (kind === EVENT_MESSAGE) {
        const bytes = op_worker_take_message(state.id);
        let data;
        try {
          data = core.deserialize(bytes);
        } catch {
          fire(worker, new MessageEvent("messageerror"));
          continue;
        }
        fire(worker, new MessageEvent("message", { data }));
      } else if (kind === EVENT_ERROR) {
        const { message = "", filename = "", lineno = 0, colno = 0 } = JSON.parse(op_worker_take_error(state.id));
        const init = { message, filename, lineno, colno, cancelable: true };
        if (fire(worker, new ErrorEvent("error", init)) && fire(globalThis, new ErrorEvent("error", init))) {
          core.print(`Uncaught (in worker) ${message}\n`, true);
        }
      } else {
        // The worker ended (close() or an uncaught error): free its owner state.
        op_worker_terminate(state.id);
        return;
      }
    }
  }

  return { globals: { Worker } };
};
