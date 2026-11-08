// ResizeObserver over Blitz layout. Rust runs this setup, then alternates a few
// driven frames with `resizeObserverPhase(n)` (n = 1..4), which checks what the
// frames delivered and prepares the next change. Motion 12 observes elements
// with it during init (`motion-C7hs86Fb.js`, "ResizeObserver is not defined").
(() => {
  "use strict";
  const LOOP_MESSAGE = "ResizeObserver loop completed with undelivered notifications.";
  const log = [];
  const fail = (message) => {
    throw new Error(`ResizeObserver: ${message}: ${JSON.stringify(log)}`);
  };
  const check = (condition, message) => {
    if (!condition) fail(message);
  };
  const throwsTypeError = (action) => {
    try {
      action();
    } catch (error) {
      return error instanceof TypeError;
    }
    return false;
  };
  const size = (sizes) => `${sizes[0].inlineSize}x${sizes[0].blockSize}`;
  const describe = (entry) => ({
    id: entry.target.id,
    content: [entry.contentRect.x, entry.contentRect.y, entry.contentRect.width, entry.contentRect.height].join(","),
    contentBox: size(entry.contentBoxSize),
    borderBox: size(entry.borderBoxSize),
    devicePixel: size(entry.devicePixelContentBoxSize),
  });

  // Absolutely positioned right of #app so the later pixel checks are unaffected;
  // removed in the last phase.
  const container = document.createElement("div");
  container.setAttribute("id", "ro-container");
  container.setAttribute("style", "position: absolute; left: 400px; top: 0px; width: 100px;");
  const target = document.createElement("div");
  target.setAttribute("id", "ro-target");
  // Width is auto: it follows the container, so a container change is a
  // layout-driven resize of the target.
  target.setAttribute("style", "height: 20px; padding: 5px; border: 2px solid black;");
  const hidden = document.createElement("div");
  hidden.setAttribute("id", "ro-hidden");
  hidden.setAttribute("style", "display: none;");
  container.appendChild(target);
  container.appendChild(hidden);
  document.body.appendChild(container);
  const detached = document.createElement("div");
  detached.setAttribute("id", "ro-detached");

  check(typeof ResizeObserver === "function" && typeof ResizeObserverEntry === "function", "globals");
  check(throwsTypeError(() => new ResizeObserver()), "constructor requires a callback");
  check(throwsTypeError(() => new ResizeObserverEntry()), "ResizeObserverEntry is not constructible");
  check(throwsTypeError(() => new ResizeObserverSize()), "ResizeObserverSize is not constructible");

  const calls = { content: [], border: [], detached: [], grow: [] };
  // A throw inside a callback would only become an error event; record instead.
  const callbackErrors = [];
  const expect = (condition, message) => {
    if (!condition) callbackErrors.push(message);
  };
  const contentObserver = new ResizeObserver(function (entries, observer) {
    expect(this === contentObserver && observer === contentObserver, "callback this/observer");
    calls.content.push(entries.map(describe));
    for (const entry of entries) {
      expect(entry instanceof ResizeObserverEntry, "entry class");
      expect(entry.contentRect instanceof DOMRectReadOnly && !(entry.contentRect instanceof DOMRect), "contentRect is a DOMRectReadOnly");
      expect(Object.isFrozen(entry.contentBoxSize) && entry.contentBoxSize[0] instanceof ResizeObserverSize, "frozen size sequences");
    }
  });
  check(throwsTypeError(() => contentObserver.observe({})), "observe rejects non-elements");
  check(throwsTypeError(() => contentObserver.observe(target, { box: "padding-box" })), "observe rejects unknown boxes");
  contentObserver.observe(target);
  contentObserver.observe(hidden);
  const borderObserver = new ResizeObserver((entries) => calls.border.push(entries.map(describe)));
  borderObserver.observe(target, { box: "border-box" });
  const detachedObserver = new ResizeObserver((entries) => calls.detached.push(entries.map(describe)));
  detachedObserver.observe(detached);
  check(calls.content.length === 0 && calls.border.length === 0, "observe() delivers asynchronously, at the next frame");

  let loopErrors = 0;
  addEventListener("error", (event) => {
    if (event.message === LOOP_MESSAGE) {
      loopErrors++;
      event.preventDefault();
    }
  });

  let growObserver;
  let growing;
  let deep;
  let removable;
  const phases = {
    1() {
      // Initial reports: every observed target once, in observation order, with
      // sizes from Blitz layout (content box = auto width 100 - 2*2 border - 2*5 padding).
      log.push({ phase: 1, calls });
      check(calls.content.length === 1, `one content-box callback, got ${calls.content.length}`);
      const [targetEntry, hiddenEntry] = calls.content[0];
      check(calls.content[0].length === 2 && targetEntry.id === "ro-target" && hiddenEntry.id === "ro-hidden", "entries in observation order");
      check(targetEntry.content === "5,5,86,20", `contentRect from padding and content box: ${targetEntry.content}`);
      check(targetEntry.contentBox === "86x20" && targetEntry.devicePixel === "86x20", "content-box sizes");
      check(targetEntry.borderBox === "100x34", `border-box size: ${targetEntry.borderBox}`);
      check(hiddenEntry.content === "0,0,0,0" && hiddenEntry.borderBox === "0x0", "display:none reports 0x0 once");
      check(calls.border.length === 1 && calls.border[0][0].borderBox === "100x34", "border-box observer");
      // Spec depth counts the node itself, so a never-inserted target (the root
      // of its own tree, depth 1) is deeper than the initial gather depth 0.
      check(
        calls.detached.length === 1 && calls.detached[0].length === 1 && calls.detached[0][0].id === "ro-detached" && calls.detached[0][0].contentBox === "0x0",
        "a disconnected target reports 0x0 once",
      );

      // A connected target that is later removed from the tree: it reports its
      // laid-out size first, then 0x0 once removed (no longer rendered).
      removable = document.createElement("div");
      removable.setAttribute("id", "ro-removable");
      removable.setAttribute("style", "width: 30px; height: 7px;");
      container.appendChild(removable);
      detachedObserver.observe(removable);

      // Next frames: the container resizes the target through layout; the
      // border-box observer is disconnected; hidden is unobserved, then shown.
      container.style.width = "150px";
      borderObserver.disconnect();
      contentObserver.unobserve(hidden);
      hidden.style.display = "block";
      hidden.style.height = "10px";

      // Depth rule: a callback that resizes the shallower target leaves it
      // undelivered (loop error, reported the next frame), while a deeper target
      // resized in the same callback is delivered in the same frame.
      growing = document.createElement("div");
      growing.setAttribute("id", "ro-growing");
      growing.setAttribute("style", "width: 40px; height: 10px;");
      deep = document.createElement("div");
      deep.setAttribute("id", "ro-deep");
      deep.setAttribute("style", "width: 10px; height: 5px;");
      growing.appendChild(deep);
      container.appendChild(growing);
      growObserver = new ResizeObserver((entries) => {
        calls.grow.push(entries.map(describe));
        if (calls.grow.length === 1) {
          growing.style.height = "11px";
          deep.style.width = "20px";
        }
      });
      growObserver.observe(growing);
      growObserver.observe(deep);
    },
    2() {
      log.push({ phase: 2, calls, loopErrors });
      check(calls.content.length === 2 && calls.content[1].length === 1, "one callback for the layout-driven resize, hidden unobserved");
      check(calls.content[1][0].content === "5,5,136,20" && calls.content[1][0].borderBox === "150x34", "resized sizes");
      check(calls.border.length === 1, "a disconnected observer gets no callbacks");
      check(
        calls.detached.length === 2 && calls.detached[1].length === 1 && calls.detached[1][0].id === "ro-removable" && calls.detached[1][0].contentBox === "30x7",
        "only the newly observed target reports; the disconnected one has no size change",
      );
      removable.remove();
      const grow = calls.grow.map((entries) => entries.map((entry) => `${entry.id}:${entry.contentBox}`).join(" "));
      check(grow.join("|") === "ro-growing:40x10 ro-deep:10x5|ro-deep:20x5|ro-growing:40x11", `depth-limited delivery: ${grow.join("|")}`);
      check(loopErrors === 1, `one loop error for the skipped shallower target, got ${loopErrors}`);

      // Re-observing replaces the observation and reports again, even unchanged.
      contentObserver.observe(target);
    },
    3() {
      log.push({ phase: 3, calls });
      check(calls.content.length === 3 && calls.content[2][0].contentBox === "136x20", "re-observe reports again");
      check(
        calls.detached.length === 3 && calls.detached[2].length === 1 && calls.detached[2][0].id === "ro-removable" && calls.detached[2][0].content === "0,0,0,0" && calls.detached[2][0].borderBox === "0x0",
        "a target removed from the tree reports 0x0 once",
      );
      contentObserver.disconnect();
      growObserver.disconnect();
      detachedObserver.disconnect();
      target.style.height = "40px";
    },
    4() {
      log.push({ phase: 4, calls });
      check(calls.content.length === 3 && calls.grow.length === 3, "disconnect stops every callback");
      container.remove();
    },
  };
  globalThis.resizeObserverPhase = (phase) => {
    check(callbackErrors.length === 0, `callback checks: ${callbackErrors.join("; ")}`);
    phases[phase]();
  };
})();
