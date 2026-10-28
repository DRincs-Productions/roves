"""Regression tests for the smoke protocol and the complete shell runner."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

from check_heap_snapshot import problems as snapshot_problems
from check_results import REQUIRED_CHECKS, validate

HERE = Path(__file__).resolve().parent


class ResultsTests(unittest.TestCase):
    def test_complete(self):
        self.assertEqual([], validate('suite.html', 'noise\nSUITE PASS example\nSUITE TOTAL 1/1\n'))

    def test_rejects_false_green(self):
        for output in (
            '',
            'SUITE PASS example\n',
            'SUITE FAIL example :: false\nSUITE TOTAL 0/1\n',
            'SUITE FAIL example :: false\nSUITE TOTAL 1/1\n',
            'SUITE TOTAL 0/0\n',
            'SUITE PASS a\nSUITE TOTAL 2/2\n',
            'SUITE PASS a\nSUITE TOTAL 1/1\nSUITE TOTAL 1/1\n',
            'SUITE PASS a\nSUITE TOTAL 1/1\nSUITE PASS b\n',
            'SUITE PASS a\nSUITE PASS a\nSUITE TOTAL 2/2\n',
            'SUITE PASS a\nSUITE TOTAL broken\n',
            'SUITE BOGUS a\nSUITE TOTAL 0/0\n',
        ):
            with self.subTest(output=output):
                self.assertTrue(validate('suite.html', output))

    def test_skip_policy(self):
        output = 'SUITE PASS base\nSUITE SKIP WebAssembly.instantiate (async compile)\nSUITE TOTAL 1/1 (1 skipped)\n'
        self.assertTrue(validate('suite2.html', output))
        self.assertEqual([], validate('suite2.html', output, 'jitless'))
        self.assertTrue(validate('other.html', output, 'jitless'))
        output = 'SUITE PASS base\nSUITE SKIP WebGPU adapter request (when available)\nSUITE TOTAL 1/1 (1 skipped)\n'
        self.assertTrue(validate('graphics.html', output))
        self.assertEqual([], validate('graphics.html', output, allow_missing_devices=True))
        self.assertTrue(validate('saves.html', 'SUITE SKIP saves: protocol is available\nSUITE TOTAL 0/0 (1 skipped)'))

    def test_jitless_cannot_silently_enable_wasm(self):
        output = 'SUITE PASS WebAssembly.instantiate (async compile)\nSUITE TOTAL 1/1'
        self.assertTrue(validate('suite2.html', output, 'jitless'))
        self.assertEqual([], validate('suite2.html', output))

    def test_required_checks(self):
        # A page that drops a required check still reports a consistent total: reject it.
        names = REQUIRED_CHECKS['events.html']
        name = names[0]
        self.assertEqual([f'missing required check: {name}' for name in names],
                         validate('events.html', 'SUITE PASS example\nSUITE TOTAL 1/1\n'))
        passes = ''.join(f'SUITE PASS {name}\n' for name in names)
        self.assertEqual([], validate('events.html', f'SUITE PASS example\n{passes}SUITE TOTAL {len(names) + 1}/{len(names) + 1}\n'))
        for dropped in names:
            kept = ''.join(f'SUITE PASS {name}\n' for name in names if name != dropped)
            self.assertEqual([f'missing required check: {dropped}'],
                             validate('events.html', f'SUITE PASS example\n{kept}SUITE TOTAL {len(names)}/{len(names)}\n'))
        self.assertTrue(validate('events.html', f'SUITE FAIL {name} :: final: contentWindow changed\nSUITE TOTAL 0/1\n'))
        self.assertTrue(validate('events.html', f'SUITE SKIP {name}\nSUITE TOTAL 0/0 (1 skipped)\n'))
        self.assertEqual([], validate('suite.html', 'SUITE PASS example\nSUITE TOTAL 1/1\n'))
        # stress.html: dropping the worker FIFO check leaves a consistent total that must fail.
        worker = REQUIRED_CHECKS['stress.html'][0]
        self.assertEqual([f'missing required check: {worker}'],
                         validate('stress.html', 'SUITE PASS example\nSUITE TOTAL 1/1\n'))
        self.assertTrue(validate('stress.html', f'SUITE SKIP {worker}\nSUITE TOTAL 0/0 (1 skipped)\n'))


def page_scripts(page):
    # Use the standard HTML parser so nonce/type attributes and head/body placement
    # do not silently hide an inline check from the required-check validator.
    from html.parser import HTMLParser

    class Scripts(HTMLParser):
        def __init__(self):
            super().__init__()
            self.active = False
            self.parts = []
            self.current = []

        def handle_starttag(self, tag, attrs):
            if tag == "script":
                self.active = "src" not in dict(attrs)
                self.current = []

        def handle_data(self, data):
            if self.active:
                self.current.append(data)

        def handle_endtag(self, tag):
            if tag == "script" and self.active:
                self.parts.append("".join(self.current))
                self.active = False

    parser = Scripts()
    parser.feed((HERE / page).read_text(encoding="utf-8"))
    return parser.parts


class EventsPageTests(unittest.TestCase):
    """Static and syntax checks of events.html; the origin-change check needs a real browser."""

    def test_required_checks_are_defined(self):
        for page, names in REQUIRED_CHECKS.items():
            scripts = ''.join(page_scripts(page))
            for name in names:
                with self.subTest(page=page, name=name):
                    self.assertEqual(1, scripts.count(f'check("{name}"'))

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_scripts_compile(self):
        scripts = page_scripts('events.html') + page_scripts('stress.html')
        self.assertEqual(2, len(scripts))
        for script in scripts:
            result = subprocess.run(['node', '-e', 'new (require("vm").Script)(require("fs").readFileSync(0, "utf8"))'],
                                    input=script, capture_output=True, text=True, encoding='utf-8', timeout=30)
            self.assertEqual(0, result.returncode, result.stderr)


SCHEDULER_CHECK = 'microtasks run before timer and frame callbacks, in either order'
FIFO_CHECK = REQUIRED_CHECKS['events.html'][1]
# The check events.html used before 0319: it resolved at the frame callback and required the
# 0 ms timer to have run first, rejecting the valid frame-first order Chrome shows.
OLD_SCHEDULER_CHECK = r'''check("microtasks run before timers, timers before the next frame callback", () => new Promise(resolve => {
  const order = [];
  setTimeout(() => order.push("timeout"), 0);
  requestAnimationFrame(() => { order.push("frame"); resolve(order.join() === "sync,microtask,queueMicrotask,timeout,frame" || order.join()); });
  Promise.resolve().then(() => order.push("microtask"));
  queueMicrotask(() => order.push("queueMicrotask"));
  order.push("sync");
}));'''


def registered_check(page, name):
    """The source of the `check("name", ...)` call registered by the page's script."""
    script = ''.join(page_scripts(page))
    found = re.findall(r'^check\("' + re.escape(name) + r'".*?^\}\)\);$', script, re.S | re.M)
    assert len(found) == 1, f'{name} registered {len(found)} times'
    return found[0]


def run_scheduler_check(check, steps, late_microtasks=False):
    """Runs harness.js and one check in Node with mock timer and frame queues.

    `steps` deliver callbacks in order, one per event loop turn: "timer" runs the 0 ms timer,
    "frame" the animation frame callback, "deadline" the harness timeout. `late_microtasks`
    makes queueMicrotask callbacks run as later tasks, like an engine violating the order.
    No real timer or animation timing is involved."""
    driver = r'''
const vm = require("vm");
const [harness, check, steps, late] = JSON.parse(require("fs").readFileSync(0, "utf8"));
const lines = [], timers = [], frames = [];
let nextId = 1;
const context = vm.createContext({
  console: { log: (...parts) => lines.push(parts.join(" ")) },
  setTimeout: (fn, ms = 0) => { timers.push({ id: nextId, fn, ms }); return nextId++; },
  clearTimeout: id => { const index = timers.findIndex(timer => timer.id === id); if (index >= 0) timers.splice(index, 1); },
  requestAnimationFrame: fn => { frames.push(fn); return frames.length; },
  queueMicrotask: late ? fn => setImmediate(fn) : queueMicrotask,
});
vm.runInContext(harness + "\n" + check + "\nfinish();", context);
const turn = () => new Promise(resolve => setImmediate(resolve));
const take = ms => {
  const index = timers.findIndex(timer => timer.ms === ms);
  if (index < 0) throw new Error("no timer of " + ms + " ms");
  return timers.splice(index, 1)[0].fn;
};
(async () => {
  await turn();
  for (const step of steps) {
    if (step === "frame") { if (!frames.length) throw new Error("no frame callback"); frames.shift()(); }
    else take(step === "timer" ? 0 : 10000)();
    await turn();
  }
  await turn();
  lines.push("pending frames " + frames.length);
  process.stdout.write(lines.join("\n"));
})().catch(error => { process.stdout.write("driver " + error.message); process.exitCode = 1; });
'''
    payload = json.dumps([(HERE / 'harness.js').read_text(encoding='utf-8'), check, list(steps), late_microtasks])
    return subprocess.run(['node', '-e', driver], input=payload, capture_output=True,
                          text=True, encoding='utf-8', timeout=30)


@unittest.skipUnless(shutil.which('node'), 'needs Node')
class SchedulerCheckTests(unittest.TestCase):
    """Runs the scheduler check events.html registers, with deterministic timer and frame queues."""

    def run_check(self, steps, check=None, late_microtasks=False):
        result = run_scheduler_check(check or registered_check('events.html', SCHEDULER_CHECK), steps, late_microtasks)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        return result.stdout.split('\n')

    def test_either_callback_order_passes(self):
        for steps in (('timer', 'frame'), ('frame', 'timer')):
            with self.subTest(steps=steps):
                lines = self.run_check(steps)
                self.assertEqual([f'SUITE PASS {SCHEDULER_CHECK}', 'SUITE TOTAL 1/1', 'pending frames 0'], lines)
                self.assertEqual([], validate('scheduler.html', '\n'.join(lines)))

    def test_callback_before_microtasks_fails(self):
        for steps, seen in ((('timer', 'frame'), 'sync,microtask,timeout,queueMicrotask,frame'),
                            (('frame', 'timer'), 'sync,microtask,frame,queueMicrotask,timeout')):
            with self.subTest(steps=steps):
                lines = self.run_check(steps, late_microtasks=True)
                self.assertEqual([f'SUITE FAIL {SCHEDULER_CHECK} :: {seen}', 'SUITE TOTAL 0/1', 'pending frames 0'], lines)
                self.assertTrue(validate('scheduler.html', '\n'.join(lines)))

    def test_missing_callback_times_out(self):
        # The check never resolves early: without the second callback only the harness timeout
        # reports, as a failure.
        for steps, pending in ((('timer', 'deadline'), 1), (('frame', 'deadline'), 0)):
            with self.subTest(steps=steps):
                lines = self.run_check(steps)
                self.assertEqual([f'SUITE FAIL {SCHEDULER_CHECK} :: timeout', 'SUITE TOTAL 0/1', f'pending frames {pending}'], lines)
                self.assertTrue(validate('scheduler.html', '\n'.join(lines)))

    def test_microtask_fifo_check(self):
        # Two 0 ms timers: the second case runs one task after the first.
        check = registered_check('events.html', FIFO_CHECK)
        self.assertEqual([f'SUITE PASS {FIFO_CHECK}', 'SUITE TOTAL 1/1', 'pending frames 0'],
                         self.run_check(('timer', 'timer'), check))
        # queueMicrotask callbacks after the promise jobs, like the batched drain before 0329.
        lines = self.run_check(('timer', 'timer'), check, late_microtasks=True)
        self.assertEqual([f'SUITE FAIL {FIFO_CHECK} :: p1,p2,q p1,q1,p2', 'SUITE TOTAL 0/1', 'pending frames 0'], lines)
        self.assertTrue(validate('events.html', '\n'.join(lines)))

    def test_old_check_rejected_frame_first(self):
        old = 'microtasks run before timers, timers before the next frame callback'
        self.assertEqual([f'SUITE PASS {old}', 'SUITE TOTAL 1/1', 'pending frames 0'],
                         self.run_check(('timer', 'frame'), OLD_SCHEDULER_CHECK))
        self.assertEqual([f'SUITE FAIL {old} :: sync,microtask,queueMicrotask,frame', 'SUITE TOTAL 0/1', 'pending frames 0'],
                         self.run_check(('frame', 'timer'), OLD_SCHEDULER_CHECK))


WORKER_FIFO_CHECK = REQUIRED_CHECKS['stress.html'][0]
# Mock dedicated worker for the worker FIFO check of stress.html: the Blob source (the page's real
# worker program) runs in its own Node vm context; each worker task (message, 0 ms timer) is one
# setImmediate turn, and Node's queueMicrotask shares V8's queue with promise jobs. `fault`:
# "batched" runs queueMicrotask callbacks as later tasks (the drain before 0329), "duplicate"
# posts every reply twice, "foreign" replaces the nonce, "silent" never answers the ping,
# "construct" makes the constructor throw, "error" fires an error event instead of answering b,
# "send" makes postMessage of b throw. Page timers run 50 times faster than requested. Not a
# browser: a real worker is checked by CI.
WORKER_DRIVER = r'''
const vm = require("vm");
const [harness, check, fault] = JSON.parse(require("fs").readFileSync(0, "utf8"));
const lines = [], sources = new Map(), revoked = [], workers = [];
let terminated = 0;
const clone = data => JSON.parse(JSON.stringify(data));
const domError = (name, message) => Object.assign(new Error(message), { name });
class Worker {
  constructor(url) {
    if (fault === "construct") throw domError("SecurityError", "blocked by CSP");
    workers.push(this);
    this.alive = true;
    const task = fn => setImmediate(() => { if (this.alive) fn(); });
    const reply = data => setImmediate(() => { if (this.onmessage) this.onmessage({ data: clone(data) }); });
    const scope = vm.createContext({
      setTimeout: fn => task(fn),
      queueMicrotask: fault === "batched" ? fn => task(fn) : queueMicrotask,
      postMessage: data => {
        if (fault === "silent" && data.phase === "ping") return;
        if (fault === "foreign") data = { ...data, nonce: "other" };
        reply(data);
        if (fault === "duplicate") reply(data);
      },
    });
    vm.runInContext(sources.get(url), scope);
    this.postMessage = data => {
      if (fault === "send" && data.phase === "b") throw domError("DataCloneError", "could not clone");
      if (fault === "error" && data.phase === "b") {
        return setImmediate(() => { if (this.onerror) this.onerror({ message: "boom", preventDefault() {} }); });
      }
      task(() => scope.onmessage({ data: clone(data) }));
    };
  }
  terminate() { this.alive = false; terminated++; }
}
const context = vm.createContext({
  console: { log: (...parts) => lines.push(parts.join(" ")) },
  setTimeout: (fn, ms = 0) => setTimeout(fn, ms / 50), clearTimeout, Worker,
  Blob: class { constructor(parts) { this.text = parts.join(""); } },
  URL: { createObjectURL: blob => { const url = "blob:" + sources.size; sources.set(url, blob.text); return url; },
         revokeObjectURL: url => revoked.push(url) },
});
vm.runInContext(harness + "\n" + check + "\nfinish();", context);
setTimeout(() => {
  const live = workers.filter(worker => worker.alive || worker.onmessage || worker.onerror).length;
  lines.push(`workers ${workers.length} live ${live} terminated ${terminated} revoked ${revoked.length}`);
  process.stdout.write(lines.join("\n"));
}, 600);
'''


@unittest.skipUnless(shutil.which('node'), 'needs Node')
class WorkerFifoCheckTests(unittest.TestCase):
    """Runs the worker FIFO check of stress.html against a mock worker, not a browser."""

    CLEANUP = 'workers 1 live 0 terminated 1 revoked 1'

    def run_check(self, fault=''):
        payload = json.dumps([(HERE / 'harness.js').read_text(encoding='utf-8'),
                              registered_check('stress.html', WORKER_FIFO_CHECK), fault])
        result = subprocess.run(['node', '-e', WORKER_DRIVER], input=payload, capture_output=True,
                                text=True, encoding='utf-8', timeout=30)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        return result.stdout.split('\n')

    def test_fifo_worker_passes(self):
        lines = self.run_check()
        self.assertEqual([f'SUITE PASS {WORKER_FIFO_CHECK}', 'SUITE TOTAL 1/1', self.CLEANUP], lines)
        self.assertEqual([], validate('stress.html', '\n'.join(lines)))

    def test_faults_fail_and_clean_up(self):
        for fault, detail in (
            ('batched', 'a: p1,p2,q'),
            ('duplicate', 'stale reply a while waiting for b'),
            ('foreign', 'reply from another run while waiting for a'),
            ('silent', 'timeout waiting for ping'),
            ('construct', 'new Worker threw: SecurityError: blocked by CSP'),
            ('error', 'worker error: boom'),
            ('send', 'postMessage b threw: DataCloneError: could not clone'),
        ):
            with self.subTest(fault=fault):
                lines = self.run_check(fault)
                cleanup = 'workers 0 live 0 terminated 0 revoked 1' if fault == 'construct' else self.CLEANUP
                self.assertEqual([f'SUITE FAIL {WORKER_FIFO_CHECK} :: {detail}', 'SUITE TOTAL 0/1', cleanup], lines)
                self.assertEqual([f'failed check: {WORKER_FIFO_CHECK} :: {detail}'],
                                 validate('stress.html', '\n'.join(lines)))


ORIGIN_CHECK = "an iframe keeps its WindowProxy across origin changes and back"
# Mock iframe for the origin-change check: each srcdoc navigation runs the child script in a new
# Node vm context one task later; a sandbox without allow-same-origin gives the document an
# opaque origin, so the held proxy then exposes only the cross-origin allowlist and throws a
# SecurityError DOMException for anything else. `strategy` picks how postMessage is exposed:
# "shared" returns one function that routes to the current document (as Chrome 155 does),
# "fresh" a new function per document. `fault` makes the mock misbehave like a broken engine
# from the second opaque document on. Timers run 20 times faster than requested.
ORIGIN_DRIVER = r'''
const vm = require("vm");
const [harness, check, fault, strategy] = JSON.parse(require("fs").readFileSync(0, "utf8"));
const lines = [], navigations = [];
const later = fn => setImmediate(fn);
const clone = data => JSON.parse(JSON.stringify(data));
class DOMException extends Error { constructor(message, name) { super(message); this.name = name; } }
const parentOrigin = "http://roves.test";
const parentListeners = new Set();
const parent = vm.createContext({
  console: { log: (...parts) => lines.push(parts.join(" ")) },
  setTimeout: (fn, ms = 0) => setTimeout(fn, ms / 20), clearTimeout, DOMException,
  addEventListener: (type, fn) => { if (type === "message") parentListeners.add(fn); },
  removeEventListener: (type, fn) => { if (type === "message") parentListeners.delete(fn); },
});
const parentGlobal = vm.runInContext("globalThis.window = globalThis.self = globalThis;", parent);
parent.origin = parentOrigin;
const dispatch = (listeners, event) => later(() => { for (const fn of [...listeners]) fn(event); });
// The parent as a sandboxed child sees it: document access throws (postMessage is mocked below).
const parentSeenFromOpaque = new Proxy({}, { get() { throw new DOMException("cross-origin", "SecurityError"); } });
let current = null, previousOpaque = null, lastOpaque = null, opaqueCount = 0, connected = false, removed = 0, staleProxy = null;
const allowlist = { closed: () => false, window: () => proxy, self: () => proxy, frames: () => proxy, length: () => 0, parent: () => parentGlobal };
const handler = {
  get(target, key) {
    if (!current) return undefined;
    if (!current.opaque) return key === "document" ? current.document : current.global[key];
    if (key === "postMessage") return fault === "unstable-postmessage" && opaqueCount > 1 ? current.makePostMessage() : current.postMessage;
    if (key in allowlist) return allowlist[key]();
    if (fault === "document-readable" && opaqueCount > 1 && key === "document") return current.document;
    throw new DOMException("cross-origin", "SecurityError");
  },
  getOwnPropertyDescriptor(target, key) {
    if (current && current.opaque && key === "postMessage") {
      const value = fault === "descriptor-postmessage" && opaqueCount > 1 ? current.makePostMessage() : current.postMessage;
      return { value, writable: false, enumerable: false, configurable: true };
    }
    throw new DOMException("cross-origin", "SecurityError");
  },
};
const proxy = new Proxy({}, handler);
// `owner` is the document a fresh function belongs to; "stale-routing" delivers to the previous
// opaque document instead of the current one.
const postMessageFor = owner => function postMessage(data) {
  if (this !== proxy) throw new TypeError("illegal invocation");
  const target = fault === "stale-routing" && opaqueCount > 1 ? previousOpaque : strategy === "shared" ? current : owner;
  dispatch(target.childListeners, { data: clone(data), source: target.global.parent, origin: parentOrigin });
};
const sharedPostMessage = postMessageFor(null);
function load(srcdoc, opaque) {
  const match = /^<p id="marker">(.*?)<\/p><script>([\s\S]*)<\/script>$/.exec(srcdoc);
  if (!match) throw new Error("unexpected srcdoc " + srcdoc);
  const childListeners = new Set();
  const marker = { textContent: match[1] };
  const document = { getElementById: id => (id === "marker" ? marker : null) };
  const origin = opaque ? "null" : parentOrigin;
  const source = fault === "new-proxy" && opaque && opaqueCount > 1 ? (staleProxy = staleProxy || new Proxy({}, handler)) : proxy;
  const parentView = opaque ? parentSeenFromOpaque : parent;
  const toParent = data => dispatch(parentListeners, { data: clone(data), source, origin });
  const global = vm.createContext({ document, addEventListener: (type, fn) => { if (type === "message") childListeners.add(fn); } });
  vm.runInContext("globalThis.window = globalThis.self = globalThis;", global);
  global.origin = origin;
  if (fault === "stale-content" && opaque && opaqueCount > 1) {
    // The new script runs, but the document and global still show the previous opaque one.
    marker.textContent = current.document.getElementById("marker").textContent;
    Object.defineProperty(global, "rovesOriginPhase", { get: () => marker.textContent, set() {} });
  }
  global.parent = new Proxy(parentView, { get: (target, key) => (key === "postMessage" ? toParent : Reflect.get(target, key)) });
  const doc = { opaque, document, global, childListeners, script: match[2], makePostMessage: () => postMessageFor(doc) };
  doc.postMessage = strategy === "shared" ? sharedPostMessage : doc.makePostMessage();
  if (opaque) [previousOpaque, lastOpaque] = [lastOpaque, doc];
  current = doc;
  vm.runInContext(match[2], global);
}
const frame = {
  sandbox: null, value: "",
  setAttribute(name, value) { if (name === "sandbox") this.sandbox = String(value); },
  removeAttribute(name) { if (name === "sandbox") this.sandbox = null; },
  get isConnected() { return connected; },
  get contentWindow() { return connected ? (fault === "new-proxy" && opaqueCount > 1 && current.opaque ? staleProxy || proxy : proxy) : null; },
  get contentDocument() { return connected && current && !current.opaque ? current.document : null; },
  set srcdoc(value) { this.value = value; if (connected) this.navigate(); },
  get srcdoc() { return this.value; },
  remove() { connected = false; removed++; },
  navigate() {
    const opaque = this.sandbox !== null && !this.sandbox.split(/\s+/).includes("allow-same-origin");
    navigations.push(opaque ? "opaque" : "same");
    if (opaque) opaqueCount++;
    const previous = current, srcdoc = this.value;
    if (opaque && opaqueCount === 2 && fault === "no-load") return;
    if (opaque && opaqueCount === 2 && fault === "keep-document") {
      // The old opaque document stays and posts its load message again.
      return later(() => { current = previous; vm.runInContext(previous.script, previous.global); });
    }
    later(() => load(srcdoc, opaque));
  },
};
parent.document = { createElement: () => frame, body: { append: () => { connected = true; frame.navigate(); } } };
vm.runInContext(harness + "\n" + check + "\nfinish();", parent);
setTimeout(() => {
  lines.push("navigations " + navigations.join(","), "listeners " + parentListeners.size, "connected " + connected + " removed " + removed);
  process.stdout.write(lines.join("\n"));
}, 1500);
'''


def run_origin_check(check, fault='', strategy='fresh'):
    payload = json.dumps([(HERE / 'harness.js').read_text(encoding='utf-8'), check, fault, strategy])
    return subprocess.run(['node', '-e', ORIGIN_DRIVER], input=payload, capture_output=True,
                          text=True, encoding='utf-8', timeout=30)


@unittest.skipUnless(shutil.which('node'), 'needs Node')
class OriginCheckTests(unittest.TestCase):
    """Runs the origin-change check of events.html against a mock iframe, not a browser."""

    NAVIGATIONS = 'navigations same,opaque,opaque,same,opaque,opaque,same'
    CLEANUP = ['listeners 0', 'connected false removed 1']

    STRATEGIES = ('shared', 'fresh')

    def run_check(self, fault='', strategy='fresh'):
        result = run_origin_check(registered_check('legacy-multidocument/events.html', ORIGIN_CHECK), fault, strategy)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        return result.stdout.split('\n')

    def test_two_opaque_documents_per_round_pass(self):
        # Reusing one postMessage across documents and minting a new one are both valid.
        for strategy in self.STRATEGIES:
            with self.subTest(strategy=strategy):
                lines = self.run_check(strategy=strategy)
                self.assertEqual([f'SUITE PASS {ORIGIN_CHECK}', 'SUITE TOTAL 1/1', self.NAVIGATIONS] + self.CLEANUP, lines)
                # This check alone (events.html also requires the microtask FIFO check).
                self.assertEqual([], validate('origin-check.html', '\n'.join(lines)))

    def test_engine_faults_fail_and_clean_up(self):
        # Every fault starts at the second opaque document (opaque-b-0) and is reported there.
        for fault, detail in (
            ('stale-routing', 'timeout waiting for opaque-b-0 echo'),
            ('unstable-postmessage', 'opaque-b-0: postMessage reads differ, postMessage descriptor differs from reads'),
            ('descriptor-postmessage', 'opaque-b-0: postMessage descriptor differs from reads'),
            ('new-proxy', 'opaque-b-0: contentWindow changed, source is not the held window'),
            ('document-readable', 'opaque-b-0: document did not throw'),
            ('stale-content', 'opaque-b-0: child shows opaque-b-0/opaque-a-0/opaque-a-0'),
            ('keep-document', 'stale message from opaque-a-0 while waiting for opaque-b-0'),
            ('no-load', 'timeout waiting for opaque-b-0'),
        ):
            for strategy in self.STRATEGIES:
                with self.subTest(fault=fault, strategy=strategy):
                    lines = self.run_check(fault, strategy)
                    self.assertEqual(f'SUITE FAIL {ORIGIN_CHECK} :: {detail}', lines[0])
                    self.assertEqual(['SUITE TOTAL 0/1'], lines[1:2])
                    self.assertEqual(self.CLEANUP, lines[-2:])
                    self.assertTrue(validate('events.html', '\n'.join(lines[:2])))


def gc_page_script(name="gc-lifecycle.html"):
    page = (HERE / name).read_text(encoding="utf-8")
    return page.split('<script>', 1)[1].split('</script>', 1)[0]


def run_page_in_node(script, setup='', after='process.stdout.write(lines.join("\\n"));'):
    """Runs the page script in a Node vm context without TestUtils or a DOM, then `after`."""
    harness = r'''
const vm = require("vm");
const lines = [];
const log = (...parts) => lines.push(parts.join(" "));
const context = vm.createContext({ console: { log, error: log }, setTimeout, clearTimeout, addEventListener() {} });
''' + setup + r'''
try { vm.runInContext(require("fs").readFileSync(0, "utf8"), context); }
catch (error) { lines.push("thrown " + error.name + " " + error.message); }
setTimeout(() => { ''' + after + ''' }, 200);
'''
    result = subprocess.run(['node', '-e', harness], input=script, capture_output=True,
                            text=True, encoding='utf-8', timeout=30)
    return result


# Mock same-origin frames: each loads one task after insertion, and its classic script token is
# copied to its window. No module ever loads, so the module group fails its import check but is
# still recorded. The mock never releases anything: only reporting is checked, not collection.
FRAME_MOCK = r'''
const listenerOptions = [];
const inserted = [];
context.location = { href: "http://127.0.0.1:18765/gc-lifecycle.html" };
context.URL = URL;
context.document = {
  createElement() {
    const listeners = {};
    return {
      srcdoc: "", contentWindow: null, contentDocument: null, listeners,
      addEventListener(type, listener, options) { listenerOptions.push(type + " " + JSON.stringify(options)); listeners[type] = listener; },
      remove() { this.contentWindow = null; this.contentDocument = null; },
    };
  },
  body: {
    appendChild(frame) {
      inserted.push(frame);
      setTimeout(() => {
        const token = /rovesPlainToken = "([^"]*)"/.exec(frame.srcdoc);
        frame.contentWindow = token ? { Object, rovesPlainToken: token[1] } : { Object };
        frame.contentDocument = {};
        frame.listeners.load();
      }, 0);
    },
  },
};
'''

FRAME_GROUPS_RUN = r'''
context.TestUtils = { gc: async () => {} };
vm.runInContext(`checkModuleOwners().then(checkPlainFrames).then(reportRemovedFrames).then(() =>
  console.log("weak only " + Object.values(removedFrames).every(group =>
    [group.elements, group.documents, group.globals].every(refs => refs.every(ref => ref instanceof WeakRef)))))`, context)
  .catch(error => lines.push("thrown " + error.name + " " + error.message))
  .finally(() => {
    lines.push("listeners " + listenerOptions.join(","));
    process.stdout.write(lines.join("\n"));
  });
'''

FRAME_GROUP_MARKERS = (
    'ROVES-V8 gc removed frame group module elements 2 documents 2 globals 2',
    'ROVES-V8 gc no-module frames loaded true',
    'ROVES-V8 gc removed frame group no-module elements 2 documents 2 globals 2',
)


def frame_group_problems(lines):
    """What run.sh's frame group checks would miss in this output."""
    problems = [marker for marker in FRAME_GROUP_MARKERS if marker not in lines]
    for group in ('module', 'no-module'):
        pattern = (f'ROVES-V8 gc diagnostic removed frames released {group} '
                   r'elements \d+/2 documents \d+/2 globals \d+/2')
        if not any(re.fullmatch(pattern, line) for line in lines):
            problems.append('diagnostic ' + group)
    return problems


class GcPageTests(unittest.TestCase):
    """Static and Node checks of gc-lifecycle.html; they do not run a browser or a collection."""

    def test_required_markers_have_a_producer(self):
        # Each marker run.sh requires must be reported by the page, under a label made of its
        # leading words, e.g. "unreachable dom collected true" by expectCollected("unreachable dom").
        script = gc_page_script()
        runner = (HERE / 'run.sh').read_text(encoding='utf-8')
        loop = next(line for line in runner.splitlines() if line.startswith('for expected in "ROVES-V8 gc '))
        markers = [part[len('ROVES-V8 gc '):] for part in loop.split('"')[1::2]]
        self.assertIn('result pass', markers)
        for marker in markers:
            words = marker.split()
            labels = [' '.join(words[:n]) for n in range(1, len(words) + 1)]
            with self.subTest(marker=marker):
                self.assertTrue(any(f'"{label}"' in script for label in labels))

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_entry_sequence_runs(self):
        # Without TestUtils or a DOM the page must still define its entry functions, reach main(),
        # report the missing API, move on to the cleanup check (which fails on the missing Image)
        # and report one result. A missing entry function throws before any marker.
        after = r'''
const entries = ["main", "checkCleanupEntrySettings", "reportSingleDocumentLifetime"];
lines.push("entries " + entries.map(name => typeof context[name]).join(" "));
process.stdout.write(lines.join("\n"));
'''
        result = run_page_in_node(gc_page_script(), after=after)
        self.assertEqual('', result.stderr)
        lines = result.stdout.splitlines()
        self.assertNotIn('thrown', result.stdout)
        self.assertIn('entries function function function', lines)
        self.assertIn('ROVES-V8 gc api false', lines)
        results = [line for line in lines if line.startswith('ROVES-V8 gc result ')]
        self.assertEqual(['ROVES-V8 gc result fail TestUtils.gc unavailable; '
                          'exception ReferenceError Image is not defined'], results)

    def test_main_runs_both_frame_groups(self):
        main = gc_page_script("legacy-multidocument/gc-lifecycle.html").split('async function main() {', 1)[1].split('\n}\n', 1)[0]
        self.assertLess(main.index('await checkModuleOwners();'), main.index('await checkPlainFrames();'))
        self.assertLess(main.index('await checkPlainFrames();'), main.index('checkKept("retained nodes final");'))

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_frame_groups_report(self):
        # Both groups load two frames through one listener pair each (once) and a bounded
        # timeout, keep only WeakRefs and report per-group counts. Collection is not observed.
        result = run_page_in_node(gc_page_script("legacy-multidocument/gc-lifecycle.html"), FRAME_MOCK, FRAME_GROUPS_RUN)
        self.assertEqual('', result.stderr)
        lines = result.stdout.splitlines()
        self.assertNotIn('thrown', result.stdout)
        self.assertEqual([], frame_group_problems(lines), result.stdout)
        self.assertIn('weak only true', lines)
        self.assertIn('listeners ' + ','.join(['load {"once":true}', 'error {"once":true}'] * 4), lines)

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_frame_group_checks_catch_omissions(self):
        script = gc_page_script("legacy-multidocument/gc-lifecycle.html")
        mutations = (
            # The no-module group no longer records or removes its frames.
            ('removeFrames(removedFrames.plain, frames);', '',
             [FRAME_GROUP_MARKERS[2], 'diagnostic no-module']),
            # The diagnostic reports only the first group.
            ('for (const group of Object.values(removedFrames)) {',
             'for (const group of [removedFrames.module]) {', ['diagnostic no-module']),
            # The module group skips its records.
            ('removeFrames(removedFrames.module, frames);', '',
             [FRAME_GROUP_MARKERS[0], 'diagnostic module']),
        )
        for original, replacement, expected in mutations:
            with self.subTest(original=original):
                self.assertEqual(1, script.count(original))
                result = run_page_in_node(script.replace(original, replacement), FRAME_MOCK, FRAME_GROUPS_RUN)
                self.assertEqual(expected, frame_group_problems(result.stdout.splitlines()), result.stdout)


# A mock blank same-origin frame that loads one task after insertion; counts listeners and the
# explicit collections. It never releases anything: collection is not observed.
RETENTION_MOCK = r'''
const listeners = { added: 0, removed: 0 };
let collections = 0;
context.WeakRef = WeakRef;
context.TestUtils = { gc: async () => { collections++; } };
context.document = {
  createElement() {
    return {
      id: "", srcdoc: "", isConnected: false, contentWindow: null, contentDocument: null, handlers: {},
      addEventListener(type, listener) { listeners.added++; this.handlers[type] = listener; },
      removeEventListener(type, listener) { if (this.handlers[type] === listener) { listeners.removed++; delete this.handlers[type]; } },
      remove() { this.isConnected = false; },
    };
  },
  body: {
    appendChild(frame) {
      frame.isConnected = true;
      setTimeout(() => { frame.contentWindow = { Object }; frame.contentDocument = {}; frame.handlers.load(); }, 0);
    },
  },
};
'''

RETENTION_AFTER = r'''
lines.push("collections " + collections);
lines.push("listeners " + listeners.added + " added " + listeners.removed + " removed");
process.stdout.write(lines.join("\n"));
'''


def retention_page_script():
    scripts = page_scripts('frame-retention.html')
    assert len(scripts) == 1
    return scripts[0]


def run_retention_page(script, mock=RETENTION_MOCK):
    return run_page_in_node(script, mock, RETENTION_AFTER)


class FrameRetentionPageTests(unittest.TestCase):
    """Static and Node checks of frame-retention.html; no browser, collection or snapshot."""

    def test_collection_count_matches_the_runner(self):
        runner = (HERE / 'run.sh').read_text(encoding='utf-8')
        self.assertIn('HEAP_SNAPSHOT_COLLECTION=8\n', runner)
        self.assertEqual(1, retention_page_script().count('const COLLECTIONS = 8;'))

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_scene_reports_after_exactly_eight_collections(self):
        result = run_retention_page(retention_page_script())
        self.assertEqual('', result.stderr)
        lines = result.stdout.splitlines()
        for marker in ('ROVES-V8 frame-retention api true', 'ROVES-V8 frame-retention frame created true removed true',
                       'ROVES-V8 frame-retention collections complete 8',
                       # The mock keeps everything: reported, not a failure.
                       'ROVES-V8 frame-retention diagnostic released element false document false probe false',
                       'ROVES-V8 frame-retention result pass', 'collections 8', 'listeners 2 added 2 removed'):
            self.assertIn(marker, lines, result.stdout)
        self.assertEqual(1, sum(line.startswith('ROVES-V8 frame-retention result ') for line in lines))

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_missing_api_and_frame_failures_report_one_result(self):
        script = retention_page_script()
        lines = run_page_in_node(script).stdout.splitlines()
        self.assertIn('ROVES-V8 frame-retention api false', lines)
        self.assertEqual(['ROVES-V8 frame-retention result fail exception Error TestUtils.gc unavailable'],
                         [line for line in lines if line.startswith('ROVES-V8 frame-retention result ')])
        # A frame that never loads times out (bounded) and still removes its listeners.
        stalled = RETENTION_MOCK.replace(' frame.handlers.load(); }, 0);', ' }, 0);')
        self.assertNotEqual(stalled, RETENTION_MOCK)
        result = run_retention_page(script.replace('FRAME_TIMEOUT_MS = 5000', 'FRAME_TIMEOUT_MS = 20'), stalled)
        lines = result.stdout.splitlines()
        self.assertIn('ROVES-V8 frame-retention result fail exception Error frame load timed out', lines, result.stdout)
        self.assertIn('collections 0', lines)
        self.assertIn('listeners 2 added 2 removed', lines)

    @unittest.skipUnless(shutil.which('node'), 'needs Node')
    def test_mutations_are_caught(self):
        script = retention_page_script()
        for original, replacement, missing in (
            ('const COLLECTIONS = 8;', 'const COLLECTIONS = 7;', 'collections 8'),
            ('    frame.removeEventListener("load", onLoad);\n', '', 'listeners 2 added 2 removed'),
            ('  frame.remove();\n', '', 'ROVES-V8 frame-retention frame created true removed true'),
        ):
            with self.subTest(original=original):
                self.assertEqual(1, script.count(original))
                result = run_retention_page(script.replace(original, replacement))
                self.assertNotIn(missing, result.stdout.splitlines(), result.stdout)


@unittest.skipUnless(shutil.which("node"), "needs Node")
class DocumentRetentionPageTests(unittest.TestCase):
    """Exercise the scene protocol with a mock; this does not simulate garbage collection."""

    def test_eight_collections_and_retained_control(self):
        script = page_scripts("document-retention.html")[0]
        mock = r'''
let collections = 0;
context.WeakRef = WeakRef;
context.TestUtils = { gc: async () => { collections++; } };
const control = {};
context.document = {
  getElementById: () => control,
  createElement: () => ({ isConnected: false, addEventListener() {}, remove() { this.isConnected = false; } }),
  body: { append(node) { node.isConnected = true; } },
};
'''
        after = 'lines.push("collections " + collections); process.stdout.write(lines.join("\\n"));'
        result = run_page_in_node(script, mock, after)
        self.assertEqual('', result.stderr)
        lines = result.stdout.splitlines()
        for marker in ('collections 8', 'ROVES-V8 document-retention node created true removed true',
                       'ROVES-V8 document-retention retained control true',
                       'ROVES-V8 document-retention result pass'):
            self.assertIn(marker, lines)
        self.assertTrue(any(line.startswith('ROVES-V8 document-retention diagnostic released element ') for line in lines))

    def test_runner_requires_retained_control(self):
        runner = (HERE / "run.sh").read_text(encoding="utf-8")
        line = next(line for line in runner.splitlines() if line.strip().startswith('for expected in "ROVES-V8 document-retention'))
        self.assertIn('"ROVES-V8 document-retention retained control true"', line)

    def test_missing_gc_reports_failure(self):
        script = page_scripts("document-retention.html")[0]
        result = run_page_in_node(script)
        self.assertIn('ROVES-V8 document-retention result fail exception Error TestUtils.gc unavailable', result.stdout)


class HeapSnapshotCheckTests(unittest.TestCase):
    """check_heap_snapshot.py on synthetic files (the real format comes from V8 in CI)."""

    def check(self, content, *required):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'scene.heapsnapshot'
            if content is not None:
                path.write_text(content, encoding='utf-8')
            return snapshot_problems(str(path), required)

    def test_valid_and_invalid_snapshots(self):
        valid = json.dumps({'snapshot': {'meta': {'node_fields': ['type', 'name']}, 'node_count': 2},
                            'nodes': [0, 0, 0, 1], 'strings': ['a', 'probe']})
        self.assertEqual([], self.check(valid, 'probe'))
        self.assertEqual(["missing string 'other'"], self.check(valid, 'probe', 'other'))
        self.assertEqual(['3 node fields for 2 nodes of 2 fields'], self.check(valid.replace('[0, 0, 0, 1]', '[0, 0, 0]')))
        for content in (None, '', valid[:-5], '{"snapshot": {}}', '[]'):
            with self.subTest(content=content):
                found = self.check(content)
                self.assertEqual(1, len(found))
                self.assertTrue(found[0].startswith('not a V8 heap snapshot: '), found)


ROOT = HERE.parents[1]


def braced_body(source, marker):
    opening = source.index('{', source.index(marker))
    depth = 0
    for index in range(opening, len(source)):
        depth += {'{': 1, '}': -1}.get(source[index], 0)
        if depth == 0:
            return source[opening + 1:index]
    raise AssertionError(f'unclosed block after {marker!r}')


def checkpoint_problems(microtask, runtime, wrappers):
    """Whether the HTML microtask checkpoint performs step 6 (ClearKeptObjects) through the
    roves-js bridge: once per outermost checkpoint, unconditionally (an empty microtask queue
    included), after the rejected-promise and IndexedDB steps and before the flag is reset."""
    problems = []
    code = re.sub(r'//[^\n]*', '', microtask)
    if not re.search(r'use js::rust::wrappers2::\{[^}]*\bClearKeptObjects\b', code):
        problems.append('microtask.rs does not import the ClearKeptObjects wrapper')
    body = braced_body(code, 'fn checkpoint(')
    calls = [match.start() for match in re.finditer(r'\bClearKeptObjects\(cx\)', body)]
    if len(calls) != 1:
        return problems + [f'checkpoint calls ClearKeptObjects {len(calls)} times']
    statement = re.search(r'unsafe \{ ClearKeptObjects\(cx\) \};', body)
    if statement is None:
        return problems + ['step 6 is not a statement of its own']
    calls = [statement.start()]
    call = calls[0]
    reentry = body.find('if self.performing_a_microtask_checkpoint.get()')
    steps = [body.find('notify_about_rejected_promises('), body.find('cleanup_indexeddb_transactions(')]
    reset = body.find('self.performing_a_microtask_checkpoint.set(false)')
    if reentry < 0 or reentry > call:
        problems.append('step 6 runs before the re-entrancy guard')
    if min(steps) < 0 or max(steps) > call:
        problems.append('step 6 runs before the rejected-promise or IndexedDB steps')
    if reset < 0 or reset < call:
        problems.append('step 6 runs after performing a microtask checkpoint is reset')
    if body[:call].count('{') != body[:call].count('}'):
        problems.append('step 6 is conditional or inside a loop')
    bridge = braced_body(runtime, 'pub unsafe fn ClearKeptObjects(') if 'pub unsafe fn ClearKeptObjects(' in runtime else ''
    if 'clear_kept_objects()' not in bridge or 'perform_microtask_checkpoint' in bridge:
        problems.append("the bridge must call V8's clear_kept_objects, not run microtasks")
    if 'wrap!(jsapi: pub fn ClearKeptObjects(cx: &JSContext));' not in wrappers:
        problems.append('wrappers2 does not export ClearKeptObjects')
    return problems + fifo_problems(code, body, runtime, wrappers)


# Step 3 (0329): V8's queue (promise jobs and a turn per HTML microtask), then one HTML
# microtask whose turn a termination dropped, until both queues are empty.
STEP3_LOOP = re.compile(r'loop \{\s*unsafe \{ PerformMicrotaskCheckpoint\(cx\) \};\s*'
                        r'if self\.microtask_queue\.borrow\(\)\.is_empty\(\) \{\s*break;\s*\}\s*'
                        r'self\.run_next\(cx\);\s*\}')


def fifo_problems(code, body, runtime, wrappers):
    """Whether step 3 runs V8's microtask queue, which holds the promise jobs and a turn for each
    queued HTML microtask (0329): unconditionally, before the rejected-promise step, running an
    HTML microtask itself only once V8's queue is empty, until both are."""
    problems = []
    if not re.search(r'use js::rust::wrappers2::\{[^}]*\bPerformMicrotaskCheckpoint\b', code):
        problems.append('microtask.rs does not import the PerformMicrotaskCheckpoint wrapper')
    statement = STEP3_LOOP.search(body)
    single = len(re.findall(r'\bPerformMicrotaskCheckpoint\(', body)) == 1 and len(re.findall(r'\brun_next\(', body)) == 1
    if not single or statement is None:
        return problems + ['step 3 is not the loop of V8\'s checkpoint and one dropped microtask']
    call = statement.start()
    if body.find('if self.performing_a_microtask_checkpoint.get()') > call or call > body.find('notify_about_rejected_promises('):
        problems.append('step 3 runs outside the guard or after the rejected-promise step')
    if body[:call].count('{') != body[:call].count('}'):
        problems.append('step 3 is conditional or nested in another block')
    bridges = [('PerformMicrotaskCheckpoint', 'perform_microtask_checkpoint()'), ('JobQueueMayNotBeEmpty', 'enqueue_microtask(')]
    for name, call in bridges:
        marker = f'pub unsafe fn {name}('
        if marker not in runtime or call not in braced_body(runtime, marker):
            problems.append(f'the {name} bridge does not call V8\'s {call}')
    if 'wrap!(jsapi: pub fn PerformMicrotaskCheckpoint(cx: &mut JSContext));' not in wrappers:
        problems.append('wrappers2 does not export PerformMicrotaskCheckpoint')
    return problems


class MicrotaskCheckpointContractTests(unittest.TestCase):
    """The browser's checkpoint is not built here; its source is checked against the contract
    (the bridge itself is covered by roves-js's native WeakRef test)."""

    def sources(self):
        return [(ROOT / path).read_text(encoding='utf-8') for path in (
            'components/script/microtask.rs', 'components/roves-js/src/runtime_impl.rs',
            'components/roves-js/src/wrappers2.in.rs')]

    def test_step_six_clears_kept_objects(self):
        self.assertEqual([], checkpoint_problems(*self.sources()))

    def test_mutations_are_caught(self):
        microtask, runtime, wrappers = self.sources()
        call = '        unsafe { ClearKeptObjects(cx) };\n'
        self.assertEqual(1, microtask.count(call))
        without = microtask.replace(call, '')
        cases = {
            'missing': without,
            'commented': microtask.replace(call, '        // unsafe { ClearKeptObjects(cx) };\n'),
            'before IndexedDB': without.replace('        for global in globalscopes.iter() {', call + '        for global in globalscopes.iter() {'),
            'after reset': without.replace('        // TODO: Step 8.', call + '        // TODO: Step 8.'),
            'only when jobs ran': without.replace('        // Step 7.', '        if !globalscopes.is_empty() {\n    ' + call + '        }\n        // Step 7.'),
            'inside the loop': without.replace('            self.run_next(cx);\n', '            self.run_next(cx);\n    ' + call),
        }
        step3 = '            unsafe { PerformMicrotaskCheckpoint(cx) };\n'
        loop = '        loop {\n'
        fallback = '            self.run_next(cx);\n'
        for part in (step3, loop, fallback):
            self.assertEqual(1, microtask.count(part))
        cases.update({
            'step 3 missing': microtask.replace(step3, ''),
            'step 3 after the rejected-promise step': microtask.replace(step3, '').replace(
                '        // Step 5.', step3 + '        // Step 5.'),
            'step 3 conditional': microtask.replace(loop, '        if !globalscopes.is_empty() {\n' + loop).replace(
                '        // Step 4.', '        }\n        // Step 4.'),
            # The first 0329 draft: dropped microtasks ran without returning to V8's queue.
            'fallback after the loop': microtask.replace(fallback, '').replace(
                '        // Step 4.', '        while !self.microtask_queue.borrow().is_empty() {\n    ' + fallback + '        }\n        // Step 4.'),
            'fallback before V8': microtask.replace(fallback, '').replace(loop, loop + fallback),
            'fallback drains the queue': microtask.replace(
                fallback, '            while !self.microtask_queue.borrow().is_empty() {\n    ' + fallback + '            }\n'),
        })
        for name, mutated in cases.items():
            with self.subTest(name):
                self.assertNotEqual(microtask, mutated)
                self.assertNotEqual([], checkpoint_problems(mutated, runtime, wrappers))
        run_jobs = runtime.replace('isolate(cx).clear_kept_objects();', 'isolate(cx).perform_microtask_checkpoint();')
        self.assertNotEqual(runtime, run_jobs)
        self.assertNotEqual([], checkpoint_problems(microtask, run_jobs, wrappers))
        unexported = wrappers.replace('wrap!(jsapi: pub fn ClearKeptObjects(cx: &JSContext));\n', '')
        self.assertNotEqual([], checkpoint_problems(microtask, runtime, unexported))
        # 0329: HTML microtasks get no turn in V8's queue (the batched order: p1,p2,q).
        no_turn = runtime.replace('scope.enqueue_microtask(function);', '')
        self.assertNotEqual(runtime, no_turn)
        self.assertNotEqual([], checkpoint_problems(microtask, no_turn, wrappers))
        unexported = wrappers.replace('wrap!(jsapi: pub fn PerformMicrotaskCheckpoint(cx: &mut JSContext));\n', '')
        self.assertNotEqual(wrappers, unexported)
        self.assertNotEqual([], checkpoint_problems(microtask, runtime, unexported))


def job_queue_problems(script_runtime, microtask, webidl, runtime):
    """Whether the script's job queue leaves the promise jobs to V8 (0330): roves-js only calls
    the `runJobs` trap, so every other trap is `None`, Servo's old promise and debugger interrupt
    paths are gone and no interrupt queues are allocated (0331)."""
    problems = []
    code = re.sub(r'//[^\n]*', '', script_runtime)
    traps = braced_body(code, 'static JOB_QUEUE_TRAPS: JobQueueTraps')
    if not re.search(r'\benqueuePromiseJob: None,', traps):
        problems.append('JOB_QUEUE_TRAPS installs an enqueuePromiseJob trap')
    if not re.search(r'\brunJobs: Some\(run_jobs\),', traps):
        problems.append('JOB_QUEUE_TRAPS does not install run_jobs')
    # 0331: roves-js reads no other trap and never frees the interrupt queues pointer.
    for trap in ('getHostDefinedData', 'empty', 'pushNewInterruptQueue', 'popInterruptQueue', 'dropInterruptQueues'):
        if not re.search(rf'\b{trap}: None,', traps):
            problems.append(f'JOB_QUEUE_TRAPS installs a {trap} trap')
    create = re.search(r'CreateJobQueue\((.*?)\);', code, re.S)
    if not create or not re.search(r',\s*ptr::null_mut\(\),?\s*$', create.group(1)):
        problems.append('CreateJobQueue gets interrupt queues roves-js never frees')
    old = ('enqueue_promise_job', 'EnqueuedPromiseCallback', 'PromiseJobCallback', 'get_host_defined_data',
           'HOST_DEFINED_DATA_CLASS', 'INCUMBENT_SETTING_SLOT', 'push_new_interrupt_queue', 'pop_interrupt_queue',
           'drop_interrupt_queues', 'interrupt_queues')
    if re.search(r'\bfn empty\b', re.sub(r'//[^\n]*', '', microtask)):
        problems.append('microtask.rs keeps the empty trap helper')
    for name, source in (('script_runtime.rs', code), ('microtask.rs', re.sub(r'//[^\n]*', '', microtask)),
                         ('Promise.webidl', re.sub(r'//[^\n]*', '', webidl))):
        problems += [f'{name} keeps {symbol}' for symbol in old if re.search(rf'\b{symbol}\b', source)]
    native = re.sub(r'//[^\n]*', '', runtime)
    if re.search(r'\.enqueuePromiseJob\b', native):
        problems.append('roves-js calls the enqueuePromiseJob trap')
    if '_traps.runJobs' not in native:
        problems.append('roves-js does not call the runJobs trap')
    return problems


class JobQueueTrapContractTests(unittest.TestCase):
    """The script crate is not built here: its job queue traps are checked against what
    roves-js calls (the native job queue tests cover roves-js itself)."""

    def sources(self):
        return [(ROOT / path).read_text(encoding='utf-8') for path in (
            'components/script/script_runtime.rs', 'components/script/microtask.rs',
            'components/script_bindings/webidls/Promise.webidl', 'components/roves-js/src/runtime_impl.rs')]

    def test_promise_jobs_stay_in_v8(self):
        self.assertEqual([], job_queue_problems(*self.sources()))

    def test_old_promise_path_is_caught(self):
        script_runtime, microtask, webidl, runtime = self.sources()
        trap = '    enqueuePromiseJob: None,\n'
        self.assertEqual(1, script_runtime.count(trap))
        mutations = {
            'trap installed': (script_runtime.replace(trap, '    enqueuePromiseJob: Some(enqueue_promise_job),\n'), microtask, webidl, runtime),
            'no run_jobs': (script_runtime.replace('runJobs: Some(run_jobs),', 'runJobs: None,'), microtask, webidl, runtime),
            'old runnable': (script_runtime, microtask + '\npub(crate) struct EnqueuedPromiseCallback {}\n', webidl, runtime),
            'old callback type': (script_runtime, microtask, webidl + '\ncallback PromiseJobCallback = undefined();\n', runtime),
            'roves-js enqueues': (script_runtime, microtask, webidl, runtime.replace(
                'if let Some(run) = queue._traps.runJobs {', 'if let Some(run) = queue._traps.enqueuePromiseJob {')),
            'host data trap': (script_runtime.replace('getHostDefinedData: None,', 'getHostDefinedData: Some(get_host_defined_data),'),
                               microtask, webidl, runtime),
            'interrupt trap': (script_runtime.replace('dropInterruptQueues: None,', 'dropInterruptQueues: Some(drop_interrupt_queues),'),
                               microtask, webidl, runtime),
            'interrupt allocation': (script_runtime.replace(
                '                ptr::null_mut(),\n            );\n            SetJobQueue',
                '                Box::into_raw(interrupt_queues) as *mut c_void,\n            );\n            SetJobQueue'),
                microtask, webidl, runtime),
            'empty helper': (script_runtime, microtask + '\nimpl MicrotaskQueue { pub(crate) fn empty(&self) -> bool { true } }\n',
                             webidl, runtime),
        }
        for name, mutated in mutations.items():
            with self.subTest(name):
                self.assertNotEqual((script_runtime, microtask, webidl, runtime), mutated)
                self.assertNotEqual([], job_queue_problems(*mutated))


@unittest.skipUnless(os.name == 'posix' and shutil.which('bash'), 'shell integration needs POSIX bash')
class RunnerTests(unittest.TestCase):
    def run_fixture(self, scenario, wrapped=False, pref_flag=None, gc_flag=None, snapshot=None, extra_env=None):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            runner = HERE / 'run.sh'
            if pref_flag is not None or gc_flag is not None:
                # Simulate a runner regression that drops or changes a preference flag.
                copy = directory / 'smoke'
                copy.mkdir()
                shutil.copy(HERE / 'check_results.py', copy)
                shutil.copy(HERE / 'check_heap_snapshot.py', copy)
                source = runner.read_text()
                gc_original = "'^ROVES-V8 gc result ' --pref dom_testutils_enabled=true\n"
                for original, replacement in ((" --pref js_disable_jit=true\n", pref_flag),
                                              (gc_original, None if gc_flag is None else "'^ROVES-V8 gc result '" + gc_flag)):
                    self.assertEqual(1, source.count(original))
                    if replacement is not None:
                        source = source.replace(original, replacement + "\n")
                (copy / 'run.sh').write_text(source)
                runner = copy / 'run.sh'
            python = shutil.which('python3') or sys.executable
            python_stub = directory / 'python3'
            python_stub.write_text(f'''#!{sys.executable}
import os, sys, time
args = sys.argv[1:]
if args[:2] == ['-m', 'http.server'] or (args and args[0].endswith('http_server.py')):
    time.sleep(120)
elif args[:1] == ['-c']:
    raise SystemExit(0)
else:
    os.execv(os.environ['SMOKE_TEST_REAL_PYTHON'], [os.environ['SMOKE_TEST_REAL_PYTHON'], *args])
''')
            python_stub.chmod(0o755)
            # Stub browser launches and HTTP setup; execute the real runner and validator.
            launcher = directory / 'timeout'
            launcher.write_text('''#!/usr/bin/env python3
import json, os, sys
if os.environ['SMOKE_TEST_WRAPPED'] == '1':
    assert sys.argv[2:4] == ['wrapper with spaces', '--option'], sys.argv
    browser = 4
else:
    browser = 2
assert sys.argv[browser] == 'unused-browser', sys.argv
assert sys.argv[-2] == '-z', sys.argv
options = sys.argv[browser + 1:-2]
page = sys.argv[-1].rsplit('/', 1)[-1].split('?', 1)[0]
scenario = os.environ['SMOKE_TEST_SCENARIO']
if scenario == 'runtime-logs':
    from pathlib import Path
    cache = Path(os.environ['XDG_CACHE_HOME'])
    assert cache.is_absolute() and cache.parent == Path.cwd() / '.v8-smoke-runtime', cache
    log = cache / 'roves' / 'roves.log'
    log.parent.mkdir(parents=True)
    log.write_text(sys.argv[-1])
if scenario == 'crash' and page == 'suite.html':
    raise SystemExit(139)
snapshot_env = {name: os.environ[name] for name in ('ROVES_V8_HEAP_SNAPSHOT_PATH', 'ROVES_V8_HEAP_SNAPSHOT_GC') if name in os.environ}
if page == 'document-retention.html':
    with open('snapshot-env.txt', 'w') as received:
        received.write(json.dumps([snapshot_env, options]))
else:
    # Only the dedicated snapshot process gets the native settings.
    assert snapshot_env == {}, (page, snapshot_env)
if page not in ('pref-jitless.html', 'gc-lifecycle.html', 'document-retention.html'):
    # Only the dedicated fresh processes may change a preference.
    assert options == [], sys.argv
prefs = {}
if page in ('pref-jitless.html', 'gc-lifecycle.html', 'document-retention.html'):
    if page != 'document-retention.html':
        with open(('pref' if page == 'pref-jitless.html' else 'gc') + '-args.txt', 'w') as received:
            received.write('\\n'.join(sys.argv[2:browser + 1] + options))
    # Like servoshell: each `--pref name[=value]` must name a real preference.
    for flag, value in zip(options[::2], options[1::2]):
        assert flag == '--pref', sys.argv
        name, _, setting = value.partition('=')
        if name not in ('js_disable_jit', 'dom_testutils_enabled'):
            print(f'panicked: Unknown preference: "{name}"', file=sys.stderr)
            raise SystemExit(101)
        prefs[name] = setting in ('', 'true')
if page == 'gc-lifecycle.html':
    if scenario == 'gc-crash':
        raise SystemExit(139)
    # Without the preference the TestUtils namespace is not exposed.
    api = prefs.get('dom_testutils_enabled', False) and scenario != 'gc-api'
    print('ROVES-V8 gc api ' + str(api).lower())
    if not api:
        print('ROVES-V8 gc failure TestUtils.gc unavailable')
        print('ROVES-V8 gc result fail TestUtils.gc unavailable')
    elif scenario == 'gc-retention':
        print('ROVES-V8 gc unreachable dom collected false 12/240 alive after 8 collections')
        print('ROVES-V8 gc result fail unreachable dom collected')
    elif scenario != 'gc-result':
        for line in ('unreachable dom collected true', 'unreachable cycles collected true',
                     'retained nodes true true true', 'importer compiled true',
                     'dynamic import after collection 42', 'dropped importer collected true',
                     'single document module owner before true', 'single document module owner after true',
                     'single document module resolve before true', 'single document module resolve after true',
                     'single document remains live true',
                     'self-capturing listener nodes collected true',
                     'signal listener nodes collected true',
                     'aborted signal listener nodes collected true',
                     'listener control after collection true',
                     'self-capturing inline handler nodes collected true',
                     'diagnostic inline handler nodes alive closure 0 compiled 0 uncompiled 0',
                     'inline handler control after collection true',
                     'window inline handlers after collection true',
                     'retained nodes final true true true',
                     'cleanup callback entry settings true',
                     # Retained frames, as in CI so far: diagnostic, not a failure.
                     'diagnostic removed frames released module elements 0/2 documents 0/2 globals 0/2',
                     'diagnostic removed frames released no-module elements 0/2 documents 0/2 globals 0/2',
                     'result pass'):
            # A page that skips the no-module group or its diagnostic and still claims success.
            if scenario == 'gc-document-missing' and line == 'single document remains live true':
                continue
            if scenario == 'gc-resolve-before-missing' and line == 'single document module resolve before true':
                continue
            if scenario == 'gc-document-replaced' and line == 'single document remains live true':
                line = line.replace('true', 'false')
            if scenario == 'gc-import' and line.startswith('dynamic import'):
                line = 'dynamic import after collection undefined'
            if scenario == 'gc-import' and line == 'result pass':
                line = 'result fail dynamic import'
            # A module import resolved through the other frame's module owner.
            if scenario == 'gc-owner' and line.startswith('single document module owner'):
                line = line.replace('true', 'false')
            if scenario == 'gc-owner' and line == 'result pass':
                line = 'result fail module owners; module owners after collection'
            # A page that claims success without reporting the owner check.
            if scenario == 'gc-owner-missing' and line.startswith('single document module owner'):
                continue
            # Eval code whose import() took the other frame's module owner; the page claims success.
            if scenario == 'gc-eval-owner' and line.startswith('single document module owner'):
                line = line.replace('true', 'false')
            if scenario == 'gc-eval-owner-missing' and line.startswith('single document module owner'):
                continue
            # import.meta.resolve against the frame's base URL; the page claims success.
            if scenario == 'gc-resolve' and line.startswith('single document module resolve'):
                line = line.replace('true', 'false realms true http://127.0.0.1/missing-base/gc-owner.js')
            if scenario == 'gc-resolve-missing' and line == 'single document module resolve after true':
                continue
            # Listener callbacks permanently rooted: nodes captured by their listeners survive.
            if scenario == 'gc-listener' and line == 'self-capturing listener nodes collected true':
                line = 'self-capturing listener nodes collected false 20/20 alive after 8 collections'
            if scenario == 'gc-listener' and line == 'result pass':
                line = 'result fail self-capturing listener nodes collected'
            # Inline handlers permanently rooted (expected before task 0355): closures and
            # compiled attributes keep their nodes, never-compiled attributes do not.
            if scenario == 'gc-inline' and line == 'self-capturing inline handler nodes collected true':
                line = 'self-capturing inline handler nodes collected false 20/30 alive after 8 collections'
            if scenario == 'gc-inline' and line.startswith('diagnostic inline handler'):
                line = 'diagnostic inline handler nodes alive closure 10 compiled 10 uncompiled 0'
            if scenario == 'gc-inline' and line == 'result pass':
                line = 'result fail self-capturing inline handler nodes collected'
            print('ROVES-V8 gc ' + line)
elif page == 'document-retention.html':
    # Like a debug servoshell: the snapshot is written at the requested explicit collection.
    path = snapshot_env.get('ROVES_V8_HEAP_SNAPSHOT_PATH')
    api = prefs.get('dom_testutils_enabled', False)
    print('ROVES-V8 document-retention api ' + str(api).lower())
    if api and scenario != 'snapshot-scene':
        print('ROVES-V8 document-retention node created true removed true')
    if api and path and snapshot_env.get('ROVES_V8_HEAP_SNAPSHOT_GC') == '8' and scenario != 'snapshot-dump-failed':
        if scenario != 'snapshot-file-missing':
            with open(path, 'x') as written:
                written.write('{"broken"' if scenario == 'snapshot-invalid' else json.dumps(
                    {'snapshot': {'meta': {'node_fields': ['type', 'name']}, 'node_count': 1}, 'nodes': [0, 0],
                     'strings': ['rovesDocumentRetentionProbe', 'roves-document-retention-probe']}))
        print(f'[roves-heap-snapshot] wrote 31 bytes to {path} at explicit collection 8', file=sys.stderr)
    elif scenario == 'snapshot-dump-failed':
        print(f'[roves-heap-snapshot] failed at explicit collection 8: {path}: No space left on device', file=sys.stderr)
    if api:
        if scenario != 'snapshot-control':
            print('ROVES-V8 document-retention retained control true')
        print('ROVES-V8 document-retention collections complete 8')
        if scenario != 'snapshot-diagnostic':
            print('ROVES-V8 document-retention diagnostic released element false probe false')
    print('ROVES-V8 document-retention result ' + ('pass' if api else 'fail exception Error TestUtils.gc unavailable'))
elif page == 'pref-jitless.html':
    if scenario == 'pref-cli' and '--pref' in options:
        print('error: --pref is not expected in this context', file=sys.stderr)
        raise SystemExit(1)
    jitless = prefs.get('js_disable_jit', False)
    if scenario == 'pref-crash':
        raise SystemExit(139)
    if scenario == 'pref-wasm':
        jitless = False
    if scenario != 'pref-result':
        print('ROVES-V8 pref js_disable_jit js true wasm ' + ('unavailable' if jitless else 'available'))
elif page == 'test.html':
    print('ROVES-V8 dom: changed by js v8 test 1')
    print('ROVES-V8 click event click true')
    print('ROVES-V8 promise 5')
    print('ROVES-V8 timeout fired')
    if scenario != 'window': print('ROVES-V8 class 2 {"a":[1,2]} true true')
    print('ROVES-V8 testutils default ' + ('object' if scenario == 'testutils-default' else 'undefined'))
    if scenario != 'immutable-prototype': print('ROVES-V8 ordinary prototype mutation true')
    if scenario != 'window-prototype': print('ROVES-V8 immutable Window prototype true true true true')
    if scenario not in ('cross-origin-prototype', 'cross-origin-properties', 'cross-origin-timeout'): print('ROVES-V8 cross-origin iframe refused true true')
elif page == 'csp.html':
    if scenario != 'csp': print('ROVES-V8 csp eval EvalError')
    print('ROVES-V8 csp function EvalError')
    if scenario != 'trusted-script': print('ROVES-V8 csp trusted eval 42')
    if scenario != 'trusted-function': print('ROVES-V8 csp trusted function 42')
    if scenario != 'trusted-function-guards': print('ROVES-V8 csp trusted function guards true')
    if scenario != 'trusted-function-matrix': print('ROVES-V8 csp trusted function matrix true true true')
    if scenario != 'constructor-graph': print('ROVES-V8 csp constructor graph true true true')
    print('ROVES-V8 csp ordinary eval unchanged true')
    if scenario != 'csp-attribute-allowed': print('ROVES-V8 csp attribute allowed true true')
elif page == 'csp-attr.html':
    # A forwarded <body> handler that skips the CSP check reports its third value as false.
    print('ROVES-V8 csp attribute blocked true true ' + ('false' if scenario == 'csp-attribute' else 'true'))
elif page in ('csp-attr-hash.html', 'csp-attr-fallback.html'):
    # Each scenario breaks one value: a matching hash rejected on the forwarded <body> handler or
    # through the script-src fallback, a nonce accepted for a handler, a mismatched body that ran.
    kind = page[len('csp-attr-'):-len('.html')]
    allowed = {'csp-hash-allowed': 'true false', 'csp-fallback-allowed': 'false true'}
    blocked = {'csp-hash-nonce': 'true true false', 'csp-fallback-body': 'true false true'}
    print(f'ROVES-V8 csp attribute {kind} allowed ' + (allowed[scenario] if scenario in allowed and scenario.startswith('csp-' + kind) else 'true true'))
    print(f'ROVES-V8 csp attribute {kind} blocked ' + (blocked[scenario] if scenario in blocked and scenario.startswith('csp-' + kind) else 'true true true'))
elif page == 'csp-attr-no-unsafe-hashes.html':
    # A matching hash wrongly accepted without 'unsafe-hashes' on the forwarded <body> handler.
    print('ROVES-V8 csp attribute no-unsafe-hashes blocked true ' + ('false' if scenario == 'csp-no-unsafe-hashes-body' else 'true'))
elif page == 'wasm-csp.html':
    if scenario != 'csp-wasm': print('ROVES-V8 csp wasm blocked CompileError')
elif page == 'nav1.html':
    print('ROVES-V8 nav2 loaded nav2.html true second page')
    if scenario != 'navigation': print('ROVES-V8 nav1 pageshow persisted=false')
else:
    required = json.loads(os.environ['SMOKE_TEST_REQUIRED_CHECKS'])
    if page in required:
        # Every required check of the page; the scenario drops the first one with a consistent total.
        names = required[page]
        if scenario == 'missing:' + page or (scenario == 'navigation' and page == 'single-document.html'):
            names = names[1:]
        print('SUITE PASS example')
        for name in names:
            print('SUITE PASS ' + name)
        print(f'SUITE TOTAL {len(names) + 1}/{len(names) + 1}')
    elif page == 'suite.html' and scenario == 'assertion':
        print('SUITE FAIL example :: intentionally false')
        print('SUITE TOTAL 0/1')
    elif page == 'suite.html' and scenario == 'incomplete':
        print('SUITE PASS example')
    elif page == 'suite.html' and scenario == 'skip':
        print('SUITE SKIP unexpected capability')
        print('SUITE TOTAL 0/0 (1 skipped)')
    else:
        print('SUITE PASS example')
        print('SUITE TOTAL 1/1')
# Browsers may stay open after reporting all results; that timeout is acceptable.
if scenario == 'linger' and page.endswith('.html') and page not in ('test.html', 'csp.html', 'csp-attr.html', 'csp-attr-hash.html', 'csp-attr-fallback.html', 'csp-attr-no-unsafe-hashes.html', 'wasm-csp.html', 'nav1.html'):
    sys.stdout.flush()
    import time
    time.sleep(120)
raise SystemExit(124 if scenario == 'timeout' or scenario == 'incomplete' else 0)
''')
            launcher.chmod(0o755)
            env = dict(os.environ, PATH=str(directory) + os.pathsep + os.environ['PATH'],
                       SMOKE_TEST_SCENARIO=scenario, SMOKE_TEST_WRAPPED=str(int(wrapped)), V8_SMOKE_MODE='normal',
                       V8_SMOKE_ALLOW_MISSING_DEVICES='0', PYTHON3=str(python_stub),
                       SMOKE_TEST_REAL_PYTHON=python,
                       SMOKE_TEST_REQUIRED_CHECKS=json.dumps(REQUIRED_CHECKS))
            env.pop('GITHUB_STEP_SUMMARY', None)
            env.pop('V8_SMOKE_HEAP_SNAPSHOT', None)
            if snapshot == 'relative':
                env['V8_SMOKE_HEAP_SNAPSHOT'] = 'relative.heapsnapshot'
            elif snapshot is not None:
                env['V8_SMOKE_HEAP_SNAPSHOT'] = str(directory / snapshot)
                if snapshot.startswith('existing/'):
                    (directory / 'existing').mkdir()
                    (directory / snapshot).write_text('kept')
            env.update(extra_env or {})
            result = subprocess.run(['bash', str(runner), 'unused-browser',
                                     *(['wrapper with spaces', '--option'] if wrapped else [])],
                                    cwd=directory, env=env, capture_output=True, text=True, timeout=20)
            received = directory / 'pref-args.txt'
            result.pref_args = received.read_text().split('\n') if received.exists() else None
            received = directory / 'gc-args.txt'
            result.gc_args = received.read_text().split('\n') if received.exists() else None
            received = directory / 'snapshot-env.txt'
            result.snapshot_env = json.loads(received.read_text()) if received.exists() else None
            written = directory / 'out' / 'scene.heapsnapshot'
            result.snapshot = written.read_text() if written.exists() else None
            existing = directory / 'existing' / 'scene.heapsnapshot'
            result.existing = existing.read_text() if existing.exists() else None
            result.runtime_logs = [path.read_text() for path in
                                   (directory / '.v8-smoke-runtime').glob('*/roves/roves.log')]
            return result

    def test_runtime_logs_survive_fresh_process_restarts(self):
        result = self.run_fixture('runtime-logs', wrapped=True,
                                  extra_env={'XDG_CACHE_HOME': '/unused-parent-cache'})
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        alias_logs = [url for url in result.runtime_logs if 'document-interface-alias.html' in url]
        self.assertEqual(3, len(alias_logs))
        self.assertEqual(3, len(set(alias_logs)))
        self.assertTrue(any('module-order.html' in url for url in result.runtime_logs))
        self.assertTrue(any('gc-lifecycle.html' in url for url in result.runtime_logs))

    def test_heap_snapshot_is_opt_in(self):
        # Unset: the snapshot page does not run, even when the job's environment carries the
        # native settings (the stub fails any other page that receives them).
        result = self.run_fixture('pass', extra_env={'ROVES_V8_HEAP_SNAPSHOT_PATH': '/tmp/leak.heapsnapshot',
                                                     'ROVES_V8_HEAP_SNAPSHOT_GC': '1'})
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertIsNone(result.snapshot_env)
        self.assertNotIn('document-retention', result.stdout)

    def test_requested_heap_snapshot(self):
        for wrapped in (False, True):
            with self.subTest(wrapped=wrapped):
                result = self.run_fixture('pass', wrapped=wrapped, snapshot='out/scene.heapsnapshot')
                self.assertEqual(0, result.returncode, result.stdout + result.stderr)
                settings, options = result.snapshot_env
                self.assertEqual(['ROVES_V8_HEAP_SNAPSHOT_GC', 'ROVES_V8_HEAP_SNAPSHOT_PATH'], sorted(settings))
                self.assertEqual('8', settings['ROVES_V8_HEAP_SNAPSHOT_GC'])
                self.assertTrue(settings['ROVES_V8_HEAP_SNAPSHOT_PATH'].endswith('/out/scene.heapsnapshot'))
                self.assertEqual(['--pref', 'dom_testutils_enabled=true'], options)
                self.assertIsNotNone(result.snapshot)
                self.assertIn('::notice::document-retention.html: ROVES-V8 document-retention diagnostic released element false', result.stdout)
                self.assertIn("valid V8 snapshot with 'rovesDocumentRetentionProbe', 'roves-document-retention-probe'", result.stdout)
                wrapper = ['wrapper with spaces', '--option'] if wrapped else []
                self.assertEqual(wrapper + ['unused-browser', '--pref', 'dom_testutils_enabled=true'], result.gc_args)

    def test_requested_heap_snapshot_failures(self):
        cases = (
            ('snapshot-dump-failed', 'out/scene.heapsnapshot', 'requested heap snapshot missing or empty'),
            ('snapshot-file-missing', 'out/scene.heapsnapshot', 'requested heap snapshot missing or empty'),
            ('snapshot-invalid', 'out/scene.heapsnapshot', '::error::heap snapshot '),
            ('snapshot-scene', 'out/scene.heapsnapshot', "missing 'ROVES-V8 document-retention node created true removed true'"),
            ('snapshot-diagnostic', 'out/scene.heapsnapshot', 'missing released diagnostic'),
            ('snapshot-control', 'out/scene.heapsnapshot', "missing 'ROVES-V8 document-retention retained control true'"),
            ('pass', 'existing/scene.heapsnapshot', 'already exists and is never overwritten'),
            ('pass', 'relative', 'must be an absolute path'),
        )
        for scenario, snapshot, message in cases:
            with self.subTest(scenario=scenario, snapshot=snapshot):
                result = self.run_fixture(scenario, snapshot=snapshot)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn(message, result.stdout)
                if snapshot != 'out/scene.heapsnapshot':
                    # Refused before launching: nothing runs, nothing is overwritten.
                    self.assertIsNone(result.snapshot_env)
                    if snapshot.startswith('existing/'):
                        self.assertEqual('kept', result.existing)
                else:
                    self.assertIn('V8 smoke diagnostics: document-retention.log', result.stdout)

    def test_complete_and_post_completion_timeout(self):
        for wrapped in (False, True):
            for scenario in ('pass', 'timeout'):
                with self.subTest(scenario=scenario, wrapped=wrapped):
                    result = self.run_fixture(scenario, wrapped=wrapped)
                    self.assertEqual(0, result.returncode, result.stdout + result.stderr)
                    wrapper = ['wrapper with spaces', '--option'] if wrapped else []
                    self.assertEqual(wrapper + ['unused-browser', '--pref', 'js_disable_jit=true'], result.pref_args)
                    self.assertIn('::notice::pref-jitless.html: --pref js_disable_jit=true started V8 without WebAssembly', result.stdout)
                    self.assertEqual(wrapper + ['unused-browser', '--pref', 'dom_testutils_enabled=true'], result.gc_args)
                    self.assertIn('::notice::gc-lifecycle.html: ROVES-V8 gc result pass', result.stdout)
                    self.assertIn('::notice::gc-lifecycle.html: ROVES-V8 gc single document remains live true', result.stdout)

    def test_gc_lifecycle_failures(self):
        cases = (
            ('pass', '', "missing 'ROVES-V8 gc api true' (ROVES-V8 gc api false"),
            ('pass', ' --pref dom_testutils_enabled=false', "missing 'ROVES-V8 gc api true' (ROVES-V8 gc api false"),
            ('pass', ' --pref dom_testutils=true', 'gc-lifecycle.html: servoshell exited with 101'),
            ('gc-api', None, "missing 'ROVES-V8 gc api true'"),
            ('gc-retention', None, "missing 'ROVES-V8 gc unreachable dom collected true' (ROVES-V8 gc api true ROVES-V8 gc unreachable dom collected false 12/240"),
            ('gc-import', None, "missing 'ROVES-V8 gc dynamic import after collection 42'"),
            ('gc-owner', None, "missing 'ROVES-V8 gc single document module owner before true' (ROVES-V8 gc api true"),
            ('gc-owner-missing', None, "missing 'ROVES-V8 gc single document module owner after true'"),
            ('gc-eval-owner', None, "missing 'ROVES-V8 gc single document module owner before true' (ROVES-V8 gc api true"),
            ('gc-eval-owner-missing', None, "missing 'ROVES-V8 gc single document module owner after true'"),
            ('gc-resolve', None, "missing 'ROVES-V8 gc single document module resolve before true' (ROVES-V8 gc api true"),
            ('gc-resolve-missing', None, "missing 'ROVES-V8 gc single document module resolve after true'"),
            ('gc-document-missing', None, "missing 'ROVES-V8 gc single document remains live true'"),
            ('gc-listener', None, "missing 'ROVES-V8 gc self-capturing listener nodes collected true' (ROVES-V8 gc api true"),
            ('gc-inline', None, "missing 'ROVES-V8 gc self-capturing inline handler nodes collected true' (ROVES-V8 gc api true"),
            ('gc-resolve-before-missing', None, "missing 'ROVES-V8 gc single document module resolve before true'"),
            ('gc-document-replaced', None, "missing 'ROVES-V8 gc single document remains live true'"),
            ('gc-result', None, "missing 'ROVES-V8 gc result pass'"),
            ('gc-crash', None, 'gc-lifecycle.html: servoshell exited with 139'),
        )
        for scenario, gc_flag, message in cases:
            with self.subTest(scenario=scenario, gc_flag=gc_flag):
                result = self.run_fixture(scenario, gc_flag=gc_flag)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                if scenario in ('gc-owner-missing', 'gc-eval-owner', 'gc-eval-owner-missing', 'gc-resolve',
                                'gc-resolve-missing', 'gc-document-missing', 'gc-resolve-before-missing',
                                'gc-document-replaced'):
                    # A reported pass cannot replace a missing or wrong required marker.
                    self.assertIn('::notice::gc-lifecycle.html: ROVES-V8 gc result pass', result.stdout)
                else:
                    self.assertIn("::error::gc-lifecycle.html: missing 'ROVES-V8 gc result pass'", result.stdout)
                self.assertIn(message, result.stdout)
                self.assertIn('V8 smoke diagnostics: gc-lifecycle.log', result.stdout)
                # The other preference process keeps its exact flag.
                self.assertEqual(['unused-browser', '--pref', 'js_disable_jit=true'], result.pref_args)

    def test_preference_probe_failures(self):
        cases = (
            ('pass', '', 'wasm available'),
            ('pass', ' --pref js_disable_jit=false', 'wasm available'),
            ('pref-wasm', None, 'wasm available'),
            ('pref-result', None, 'not confirmed ()'),
            ('pref-crash', None, 'pref-jitless.html: servoshell exited with 139'),
            ('pref-cli', None, 'pref-jitless.html: servoshell exited with 1'),
        )
        for scenario, pref_flag, message in cases:
            with self.subTest(scenario=scenario, pref_flag=pref_flag):
                result = self.run_fixture(scenario, pref_flag=pref_flag)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn('::error::pref-jitless.html: --pref js_disable_jit=true not confirmed', result.stdout)
                self.assertIn(message, result.stdout)
                self.assertIn('V8 smoke diagnostics: pref-jitless.log', result.stdout)

    def test_failure_paths(self):
        for scenario in ('assertion', 'incomplete', 'crash', 'window', 'immutable-prototype', 'window-prototype', 'cross-origin-prototype', 'cross-origin-properties', 'cross-origin-timeout', 'csp', 'trusted-script', 'trusted-function', 'trusted-function-guards', 'trusted-function-matrix', 'constructor-graph', 'csp-attribute', 'csp-attribute-allowed', 'csp-hash-allowed', 'csp-hash-nonce', 'csp-fallback-allowed', 'csp-fallback-body', 'csp-no-unsafe-hashes-body', 'csp-wasm', 'navigation', 'skip', 'testutils-default', *('missing:' + page for page in REQUIRED_CHECKS)):
            with self.subTest(scenario=scenario):
                result = self.run_fixture(scenario)
                self.assertNotEqual(0, result.returncode, result.stdout + result.stderr)
                self.assertIn('::error', result.stdout)
                if scenario == 'immutable-prototype':
                    self.assertIn('ordinary prototype mutation failed', result.stdout)
                if scenario == 'window-prototype':
                    self.assertIn('immutable Window prototype not confirmed', result.stdout)
                if scenario == 'cross-origin-prototype':
                    self.assertIn('cross-origin iframe created a child document', result.stdout)
                if scenario == 'cross-origin-properties':
                    self.assertIn('cross-origin iframe created a child document', result.stdout)
                if scenario == 'constructor-graph':
                    self.assertIn("missing 'ROVES-V8 csp constructor graph true true true'", result.stdout)
                if scenario == 'csp-attribute':
                    self.assertIn("csp-attr.html: missing 'ROVES-V8 csp attribute blocked true true true' (ROVES-V8 csp attribute blocked true true false", result.stdout)
                if scenario == 'csp-attribute-allowed':
                    self.assertIn("missing 'ROVES-V8 csp attribute allowed true true'", result.stdout)
                csp_hash_errors = {
                    'csp-hash-allowed': "csp-attr-hash.html: missing 'ROVES-V8 csp attribute hash allowed true true' (ROVES-V8 csp attribute hash allowed true false",
                    'csp-hash-nonce': "csp-attr-hash.html: missing 'ROVES-V8 csp attribute hash blocked true true true' (ROVES-V8 csp attribute hash allowed true true ROVES-V8 csp attribute hash blocked true true false",
                    'csp-fallback-allowed': "csp-attr-fallback.html: missing 'ROVES-V8 csp attribute fallback allowed true true' (ROVES-V8 csp attribute fallback allowed false true",
                    'csp-fallback-body': "csp-attr-fallback.html: missing 'ROVES-V8 csp attribute fallback blocked true true true' (ROVES-V8 csp attribute fallback allowed true true ROVES-V8 csp attribute fallback blocked true false true",
                    'csp-no-unsafe-hashes-body': "csp-attr-no-unsafe-hashes.html: missing 'ROVES-V8 csp attribute no-unsafe-hashes blocked true true' (ROVES-V8 csp attribute no-unsafe-hashes blocked true false",
                }
                if scenario in csp_hash_errors:
                    self.assertIn(csp_hash_errors[scenario], result.stdout)
                    # Only the broken marker fails; the other page keeps passing.
                    self.assertEqual(1, sum('::error::csp-attr-' in line for line in result.stdout.splitlines()))
                if scenario.startswith('missing:'):
                    # Only the dropped check fails: the page's other required checks are reported.
                    page = scenario.partition(':')[2]
                    errors = [line for line in result.stdout.splitlines() if 'missing required check' in line]
                    processes = 3 if page == 'document-interface-alias.html' else 1
                    self.assertEqual([f'::error::{page}: missing required check: {REQUIRED_CHECKS[page][0]}'] * processes, errors)
                if scenario == 'testutils-default':
                    self.assertIn('TestUtils exposed without dom_testutils_enabled (ROVES-V8 testutils default object', result.stdout)

    def test_completed_suites_do_not_wait_for_the_deadline(self):
        started = time.monotonic()
        result = self.run_fixture('linger')
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertLess(time.monotonic() - started, 15)


ROVES_JS = HERE.parents[1] / 'components' / 'roves-js' / 'src'
GEN_WRAPPERS2 = HERE.parent / 'roves_js' / 'gen_wrappers2.py'
sys.path.insert(0, str(GEN_WRAPPERS2.parent))
import gen_wrappers2  # noqa: E402


def mozjs_lists():
    """mozjs 0.21.6's wrapper lists in the local cargo registry, if fetched."""
    registry = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'registry' / 'src'
    return next((path / 'src' for path in registry.glob('*/mozjs-0.21.6')), None)


def raw_signature(name):
    """Parameter types (last path segment) and return type of roves-js's raw `name`."""
    for path in ROVES_JS.glob('*.rs'):
        match = re.search(r'pub unsafe fn ' + name + r'\(([^)]*)\)\s*(?:->\s*([\w:]+))?\s*\{', path.read_text(encoding='utf-8'))
        if match:
            params = [p.split(':', 1)[1].strip() for p in match.group(1).split(',') if p.strip()]
            return [p.replace('*mut ', '&').split('::')[-1] for p in params], match.group(2)
    raise AssertionError(name + ' is not defined in roves-js')


class Wrappers2GeneratorTests(unittest.TestCase):
    # mozjs entries plus roves-js's own; the 0327 entry keeps mozjs's shared context.
    REQUIRED = [
        'wrap!(jsapi: pub fn ClearKeptObjects(cx: &JSContext));',
        *[line for lines in gen_wrappers2.ROVES_ENTRIES.values() for line in lines],
    ]

    def setUp(self):
        self.checked_in = (ROVES_JS / 'wrappers2.in.rs').read_bytes().decode('utf-8')

    def mozjs_from_checked_in(self, directory):
        """Rebuilds mozjs-shaped lists from the checked-in file, without roves-js's entries."""
        own = {line for lines in gen_wrappers2.ROVES_ENTRIES.values() for line in lines}
        entries = [line for line in self.checked_in.splitlines() if line.startswith('wrap!(') and line not in own]
        for module in ('jsapi', 'glue'):
            Path(directory, module + '2_wrappers.in.rs').write_text(
                ''.join(line + '\n' for line in entries if line.startswith(f'wrap!({module}:')), encoding='utf-8')

    def test_generation_keeps_roves_entries_in_place(self):
        with tempfile.TemporaryDirectory() as mozjs:
            self.mozjs_from_checked_in(mozjs)
            self.assertEqual(self.checked_in, gen_wrappers2.generate(mozjs, str(ROVES_JS))[0])

    def test_required_entries_and_context_mutability(self):
        lines = self.checked_in.splitlines()
        for line in self.REQUIRED:
            self.assertEqual(1, lines.count(line), line)
            module, name = gen_wrappers2.ENTRY.match(line).groups()
            self.assertIn(name, gen_wrappers2.visible(str(ROVES_JS), module + '.rs'))
            wrapped = re.search(r'fn \w+\((.*)\)(?: -> (\w+))?\);$', line)
            types = [p.split(':', 1)[1].strip().replace('&mut ', '&') for p in wrapped.group(1).split(', ')]
            self.assertEqual(raw_signature(name), (types, wrapped.group(2)), line)
        self.assertNotIn('ClearKeptObjects(cx: &mut', self.checked_in)

    def test_generator_without_roves_entries_drops_them(self):
        with tempfile.TemporaryDirectory() as mozjs:
            self.mozjs_from_checked_in(mozjs)
            original = gen_wrappers2.ROVES_ENTRIES
            gen_wrappers2.ROVES_ENTRIES = {}
            try:
                text = gen_wrappers2.generate(mozjs, str(ROVES_JS))[0]
            finally:
                gen_wrappers2.ROVES_ENTRIES = original
        self.assertNotEqual(self.checked_in, text)
        self.assertEqual(self.REQUIRED[:1], [line for line in self.REQUIRED if line in text])

    @unittest.skipUnless(mozjs_lists(), "needs mozjs 0.21.6's source in the cargo registry")
    def test_real_mozjs_lists_regenerate_the_checked_in_file(self):
        target = ROVES_JS / 'wrappers2.in.rs'
        before = target.stat().st_mtime_ns
        result = subprocess.run([sys.executable, '-I', str(GEN_WRAPPERS2), '--check', str(mozjs_lists()), str(ROVES_JS), str(target)],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertEqual(before, target.stat().st_mtime_ns)
        self.assertEqual(self.checked_in.encode('utf-8'), target.read_bytes())


if __name__ == '__main__':
    unittest.main()
