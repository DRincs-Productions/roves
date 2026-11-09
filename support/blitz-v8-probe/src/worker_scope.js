// Global scope of a probe dedicated worker (src/worker.rs), run in the worker's
// own isolate before its script. Built only from deno_web/deno_webidl: events,
// timers, console, structuredClone, URL, Blob, text encoding, base64, performance.
// No DOM, fetch, createImageBitmap, WebGPU, storage, Web Audio or `Deno.*`, so a
// feature probe run in the worker reports what the worker really has.
//
// Errors: deno_web's reportException dispatches `error` on the worker global
// (`self.onerror(message, filename, lineno, colno, error)` returning true, or
// preventDefault, cancels it); an uncancelled error ends the worker, as in Deno,
// and reaches the owner's Worker as an `error` event.
(() => {
  "use strict";
  const core = globalThis.Deno.core;
  const {
    op_worker_scope_source,
    op_worker_scope_url,
    op_worker_scope_name,
    op_worker_scope_post,
    op_worker_scope_next,
    op_worker_scope_close,
  } = core.ops;
  const {
    Event,
    EventTarget,
    CustomEvent,
    ErrorEvent,
    MessageEvent,
    PromiseRejectionEvent,
    setIsTrusted,
    setEventTargetData,
    saveGlobalThisReference,
    reportException,
    defineEventHandler,
  } = core.loadExtScript("ext:deno_web/02_event.js");
  const { Console } = core.loadExtScript("ext:deno_web/01_console.js");
  const timers = core.loadExtScript("ext:deno_web/02_timers.js");
  const { DOMException } = core.loadExtScript("ext:deno_web/01_dom_exception.js");
  const { structuredClone } = core.loadExtScript("ext:deno_web/02_structured_clone.js");
  const { URL, URLSearchParams } = core.loadExtScript("ext:deno_web/00_url.js");
  const { Blob, File } = core.loadExtScript("ext:deno_web/09_file.js");
  const { TextDecoder, TextEncoder } = core.loadExtScript("ext:deno_web/08_text_encoding.js");
  const { atob, btoa } = core.loadExtScript("ext:deno_web/05_base64.js");
  const { performance, setTimeOrigin } = core.loadExtScript("ext:deno_web/15_performance.js");
  const webidl = core.loadExtScript("ext:deno_webidl/00_webidl.js");
  setTimeOrigin();
  const { evalContext, serialize, deserialize } = core;
  const name = op_worker_scope_name();

  const cloneError = (message) => {
    throw new DOMException(message, "DataCloneError");
  };
  // postMessage(message, transfer) / postMessage(message, { transfer }).
  // Transferring is not supported: a non-empty transfer list is a DataCloneError.
  function serializeMessage(message, transferOrOptions) {
    const transfer = transferOrOptions == null
      ? []
      : typeof transferOrOptions[Symbol.iterator] === "function"
      ? [...transferOrOptions]
      : [...(transferOrOptions.transfer ?? [])];
    if (transfer.length > 0) cloneError("transferring objects to or from this worker is not supported");
    return serialize(message, undefined, cloneError);
  }

  class WorkerGlobalScope extends EventTarget {
    constructor() {
      throw new TypeError("Illegal constructor");
    }
    get self() {
      return globalThis;
    }
  }
  class DedicatedWorkerGlobalScope extends WorkerGlobalScope {
    get name() {
      return name;
    }
    postMessage(message, transfer = undefined) {
      webidl.requiredArguments(arguments.length, 1, "Failed to execute 'postMessage' on 'DedicatedWorkerGlobalScope'");
      op_worker_scope_post(serializeMessage(message, transfer));
    }
    close() {
      closing = true;
      op_worker_scope_close();
    }
  }
  let closing = false;

  setEventTargetData(globalThis);
  globalThis[webidl.brand] = webidl.brand;
  Object.setPrototypeOf(globalThis, DedicatedWorkerGlobalScope.prototype);
  saveGlobalThisReference(globalThis);
  core.setReportExceptionCallback(reportException);
  core.setUnhandledPromiseRejectionHandler((promise, reason) => {
    const event = new PromiseRejectionEvent("unhandledrejection", { cancelable: true, promise, reason });
    if (globalThis.dispatchEvent(event)) {
      core.print(`Uncaught (in worker promise) ${reason?.stack ?? reason}\n`, true);
    }
    return true;
  });
  defineEventHandler(globalThis, "error", undefined, true);
  defineEventHandler(globalThis, "message");
  defineEventHandler(globalThis, "messageerror");
  defineEventHandler(globalThis, "unhandledrejection");

  const globals = {
    WorkerGlobalScope,
    DedicatedWorkerGlobalScope,
    Event,
    EventTarget,
    CustomEvent,
    ErrorEvent,
    MessageEvent,
    PromiseRejectionEvent,
    DOMException,
    structuredClone,
    URL,
    URLSearchParams,
    Blob,
    File,
    TextDecoder,
    TextEncoder,
    atob,
    btoa,
    performance,
    setTimeout: timers.setTimeout,
    clearTimeout: timers.clearTimeout,
    setInterval: timers.setInterval,
    clearInterval: timers.clearInterval,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, writable: true, configurable: true });
  }
  Object.defineProperty(globalThis, "console", {
    value: new Console((message, level) => core.print(message, level > 1)),
    writable: true,
    configurable: true,
  });
  delete globalThis.Deno;
  delete globalThis.__bootstrap;

  // Run the classic script at global scope; then deliver the owner's messages
  // (queued meanwhile) in order, one task each, until close/terminate.
  const [, error] = evalContext(op_worker_scope_source(), op_worker_scope_url());
  if (error !== null) reportException(error.thrown);

  (async () => {
    for (;;) {
      const bytes = await op_worker_scope_next();
      if (bytes.length === 0 || closing) return;
      let data;
      try {
        data = deserialize(bytes);
      } catch {
        const event = new MessageEvent("messageerror");
        setIsTrusted(event, true);
        globalThis.dispatchEvent(event);
        continue;
      }
      const event = new MessageEvent("message", { data });
      setIsTrusted(event, true);
      globalThis.dispatchEvent(event);
    }
  })();
})();
