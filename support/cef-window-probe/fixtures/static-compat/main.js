// Static compatibility checks, run as a module (a JavaScript MIME type is required for it to
// execute at all). Every check logs one line; the summary line is what CI and manual runs read.
import { preloadedValue } from './modules/preloaded.js';

const results = [];
const check = (name, ok, detail = '') => {
  results.push(ok);
  console.log(`roves-static-fixture check=${name} ok=${ok}${detail ? ` ${detail}` : ''}`);
};

const preloadEvent = (name) => new Promise((resolve) => {
  const deadline = performance.now() + 5000;
  const poll = () => {
    const seen = window.rovesPreload[name];
    if (seen || performance.now() > deadline) resolve(seen ?? 'none');
    else setTimeout(poll, 20);
  };
  poll();
});

// Resource Timing entries per URL: a preload that was consumed is fetched only once.
const fetchCount = (path) => performance.getEntriesByName(new URL(path, location.href).href).length;

try {
  const moduleEvent = await preloadEvent('module');
  check('modulepreload', moduleEvent === 'load' && preloadedValue === 'preloaded',
    `event=${moduleEvent}`);

  const styleEvent = await preloadEvent('style');
  const styleValue = getComputedStyle(document.documentElement)
    .getPropertyValue('--roves-fixture').trim();
  check('preload-style', styleEvent === 'load' && styleValue === 'ok',
    `event=${styleEvent} applied=${styleValue === 'ok'}`);

  const wasmEvent = await preloadEvent('wasm');
  const wasmResponse = await fetch('./tiny.wasm');
  const wasmHeaders = wasmResponse.headers;
  const cacheControl = wasmHeaders.get('cache-control');
  const etag = wasmHeaders.get('etag');
  const lastModified = wasmHeaders.get('last-modified');
  // instantiateStreaming rejects anything not served as application/wasm.
  const { instance } = await WebAssembly.instantiateStreaming(wasmResponse);
  check('preload-fetch-wasm', wasmEvent === 'load' && instance.exports.answer() === 42,
    `event=${wasmEvent}`);
  check('cache-headers', cacheControl === 'no-cache' && !!etag && lastModified === null,
    `cache-control=${cacheControl} etag=${etag !== null} last-modified=${lastModified !== null}`);

  // Counted before the MIME fetches below add their own entries.
  const counts = ['./modules/preloaded.js', './style.css', './tiny.wasm'].map(fetchCount);
  check('preloads-consumed-once', counts.every((n) => n === 1), `counts=${counts.join(',')}`);

  for (const [path, type] of [
    ['./main.js', 'text/javascript'],
    ['./style.css', 'text/css'],
    ['./tiny.wasm', 'application/wasm'],
  ]) {
    const response = await fetch(path, { cache: 'no-store' });
    const got = response.headers.get('content-type') ?? '';
    check(`mime ${path.slice(2)}`, response.ok && got.startsWith(type), `content-type=${got}`);
  }

  const range = await fetch('./range.txt', { headers: { Range: 'bytes=4-9' }, cache: 'no-store' });
  const rangeText = await range.text();
  const contentRange = range.headers.get('content-range') ?? '';
  check('range-206', range.status === 206 && rangeText === '456789' &&
    contentRange.startsWith('bytes 4-9/'), `status=${range.status} content-range=${contentRange}`);
} catch (error) {
  check('exception', false, `error=${error}`);
}

const ok = results.length > 0 && results.every(Boolean);
console.log(`roves-static-fixture done ok=${ok} checks=${results.length}`);
document.getElementById('status').textContent = ok ? 'ok' : 'FAIL';
document.title = ok ? 'roves static fixture ok' : 'roves static fixture FAIL';
