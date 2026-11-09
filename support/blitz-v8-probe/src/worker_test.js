// Dedicated Worker (src/worker.js, src/worker_scope.js, src/worker.rs).
// Asynchronous: Rust drives frames until workerResult.done, then checks failures.
// Includes PixiJS 8.19.0's checkImageBitmap worker verbatim: the worker scope has
// no createImageBitmap, so it must answer false (Pixi then decodes on the main
// thread with deno_image), never a faked true.
(() => {
  "use strict";
  const result = { done: false, step: "start", failures: [], checks: 0 };
  globalThis.workerResult = result;
  const check = (condition, message) => {
    result.checks++;
    if (!condition) result.failures.push(message);
  };
  const throws = (fn, test, message) => {
    try {
      fn();
    } catch (error) {
      check(test(error), `${message}: got ${error}`);
      return;
    }
    result.failures.push(`${message}: no exception`);
  };
  const isDom = (name) => (error) => error instanceof DOMException && error.name === name;
  const blobUrl = (source, type = "application/javascript") => URL.createObjectURL(new Blob([source], { type }));
  const nextEvent = (worker, type, ms = 4000) =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`no ${type} event within ${ms} ms`)), ms);
      worker.addEventListener(type, (event) => {
        clearTimeout(timer);
        resolve(event);
      }, { once: true });
    });
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  // Surface and constructor errors (synchronous).
  check(typeof Worker === "function" && Worker.prototype instanceof EventTarget, "Worker is an EventTarget interface");
  check(typeof MessageEvent === "function", "MessageEvent global");
  check(typeof URL.createObjectURL === "function" && typeof URL.revokeObjectURL === "function", "blob URLs from deno_web");
  check(typeof createImageBitmap === "function", "the page keeps deno_image createImageBitmap");
  throws(() => new Worker(), (error) => error instanceof TypeError, "new Worker() needs an argument");
  throws(() => new Worker("a.js", { type: "module" }), (error) => error instanceof TypeError, "module workers are refused");
  throws(() => new Worker("a.js", { type: "bogus" }), (error) => error instanceof TypeError, "invalid WorkerType");
  throws(() => new Worker("http://["), isDom("SyntaxError"), "unparsable URL is a SyntaxError");
  throws(() => Worker.prototype.terminate.call({}), (error) => error instanceof TypeError, "terminate brand check");

  async function pixiCheckImageBitmap() {
    // packages/pixi.js 8.19.0 lib/_virtual/checkImageBitmap.worker.mjs (WORKER_CODE).
    const WORKER_CODE = "(function () {\n    'use strict';\n\n    const WHITE_PNG = \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=\";\n    async function checkImageBitmap() {\n      try {\n        if (typeof createImageBitmap !== \"function\") return false;\n        const response = await fetch(WHITE_PNG);\n        const imageBlob = await response.blob();\n        const imageBitmap = await createImageBitmap(imageBlob);\n        return imageBitmap.width === 1 && imageBitmap.height === 1;\n      } catch (_e) {\n        return false;\n      }\n    }\n    void checkImageBitmap().then((result) => {\n      self.postMessage(result);\n    });\n\n})();\n";
    const url = URL.createObjectURL(new Blob([WORKER_CODE], { type: "application/javascript" }));
    const worker = new Worker(url);
    // WorkerManager.isImageBitmapSupported: first message, then terminate + revoke.
    const event = await nextEvent(worker, "message");
    worker.terminate();
    URL.revokeObjectURL(url);
    check(event instanceof MessageEvent && event.isTrusted && event.target === worker, "Pixi check: trusted MessageEvent at the Worker");
    check(event.data === false, `Pixi check: worker reports no createImageBitmap (got ${event.data})`);
  }

  async function scopeAndMessages() {
    const worker = new Worker(blobUrl(`
      postMessage({
        self: self === globalThis,
        scope: self instanceof DedicatedWorkerGlobalScope && self instanceof WorkerGlobalScope && self instanceof EventTarget,
        name: self.name,
        types: [typeof Deno, typeof document, typeof window, typeof fetch, typeof createImageBitmap,
          typeof importScripts, typeof localStorage, typeof navigator],
        timers: typeof setTimeout, clone: typeof structuredClone, console: typeof console.log,
      });
      onmessage = (event) => postMessage({ echo: event.data, trusted: event.isTrusted, type: event.constructor.name });
    `), { name: "probe-worker" });
    // Posted before the script runs: queued and delivered in order afterwards.
    worker.postMessage(1);
    const value = { map: new Map([["a", 1]]), date: new Date(5), bytes: new Uint8Array([1, 2, 3]), nested: { list: [1, "x", null] } };
    worker.postMessage(value);
    const replies = [];
    worker.onmessage = (event) => replies.push(event.data);
    for (let i = 0; i < 200 && replies.length < 3; i++) await sleep(10);
    worker.terminate();
    const [scope, first, second] = replies;
    check(scope?.self === true && scope?.scope === true, `worker global is a DedicatedWorkerGlobalScope: ${JSON.stringify(scope)}`);
    check(scope?.name === "probe-worker", "worker name option");
    check(JSON.stringify(scope?.types) === JSON.stringify(Array(8).fill("undefined")),
      `worker has no Deno/DOM/fetch/createImageBitmap/importScripts/storage/navigator: ${JSON.stringify(scope?.types)}`);
    check(scope?.timers === "function" && scope?.clone === "function" && scope?.console === "function", "worker timers, structuredClone, console");
    check(first?.echo === 1 && first?.trusted === true && first?.type === "MessageEvent", `queued message first: ${JSON.stringify(first)}`);
    const echo = second?.echo;
    check(echo?.map instanceof Map && echo.map.get("a") === 1 && echo.date instanceof Date && echo.date.getTime() === 5
      && echo.bytes instanceof Uint8Array && echo.bytes.join() === "1,2,3" && echo.nested.list[1] === "x",
      "structured clone round trip through the worker");
    throws(() => worker.postMessage(() => {}), isDom("DataCloneError"), "functions are not cloneable");
    const fresh = new Worker(blobUrl(""));
    throws(() => fresh.postMessage(new ArrayBuffer(4), [new ArrayBuffer(4)]), isDom("DataCloneError"), "transfer lists are unsupported");
    fresh.terminate();
  }

  async function errors() {
    // Uncaught error: cancelable ErrorEvent at the Worker with the worker's location.
    const thrower = new Worker(blobUrl("throw new Error('boom in worker');"));
    const event = await nextEvent(thrower, "error");
    event.preventDefault();
    check(event instanceof ErrorEvent && event.isTrusted && /boom in worker/.test(event.message), `worker error event: ${event.message}`);
    check(event.cancelable, "worker error event is cancelable");
    thrower.terminate();
    // Cancelled inside the worker (onerror returning true): the worker keeps running.
    const survivor = new Worker(blobUrl(`
      self.onerror = (message) => /handled/.test(message);
      setTimeout(() => { throw new Error("handled"); }, 0);
      setTimeout(() => postMessage("alive"), 20);
    `));
    let survivorErrors = 0;
    survivor.onerror = () => survivorErrors++;
    const alive = await nextEvent(survivor, "message");
    check(alive.data === "alive" && survivorErrors === 0, "an error cancelled in the worker neither reaches the owner nor ends the worker");
    survivor.terminate();
    // Script fetch failures: plain `error` Event, no worker runs.
    const wrongType = new Worker(blobUrl("postMessage('ran')", "text/plain"));
    const failed = await nextEvent(wrongType, "error");
    check(failed.constructor === Event && !(failed instanceof ErrorEvent), "non-JavaScript MIME type fires a plain error Event");
    const revokedUrl = blobUrl("postMessage('ran')");
    URL.revokeObjectURL(revokedUrl);
    const revoked = new Worker(revokedUrl);
    await nextEvent(revoked, "error");
    check(true, "revoked blob URL fires error");
  }

  async function lifecycle() {
    // terminate() stops a busy isolate and drops later messages.
    const busy = new Worker(blobUrl("postMessage('started'); for (;;) {}"));
    await nextEvent(busy, "message");
    busy.terminate();
    const ticker = new Worker(blobUrl("setInterval(() => postMessage('tick'), 1);"));
    await nextEvent(ticker, "message");
    ticker.terminate();
    let ticksAfterTerminate = 0;
    ticker.onmessage = () => ticksAfterTerminate++;
    await sleep(100);
    check(ticksAfterTerminate === 0, `no messages after terminate(): ${ticksAfterTerminate}`);
    // close(): the worker ends after the current task; queued timers never run.
    const closer = new Worker(blobUrl("postMessage('before'); setTimeout(() => postMessage('never'), 0); close();"));
    const received = [];
    closer.onmessage = (event) => received.push(event.data);
    await sleep(300);
    check(received[0] === "before" && !received.includes("never"), `close() ends the worker: ${JSON.stringify(received)}`);
    closer.terminate();
    // A new worker still starts after the terminated ones.
    const after = new Worker(blobUrl("postMessage(40 + 2);"));
    const answer = await nextEvent(after, "message");
    check(answer.data === 42, "workers start after earlier terminations");
    after.terminate();
  }

  (async () => {
    for (const [step, run] of [
      ["pixi_check_image_bitmap", pixiCheckImageBitmap],
      ["scope_and_messages", scopeAndMessages],
      ["errors", errors],
      ["lifecycle", lifecycle],
    ]) {
      result.step = step;
      try {
        await run();
      } catch (error) {
        result.failures.push(`${step}: ${error?.stack ?? error}`);
      }
    }
    result.done = true;
  })();
})();
