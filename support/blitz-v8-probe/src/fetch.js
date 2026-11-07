// Minimal fetch for games (loaded by dom.js). deno_fetch was evaluated and set
// aside (no game:// scheme without a fork; it brings the network stack, TLS,
// deno_fs and permissions, i.e. network access a game must not have).
// Supported: game://content/... (files of the game, via op_blitz_game_read) and
// data: URLs. Every other scheme rejects with TypeError: no network access.
// Blob and TextDecoder/TextEncoder come from deno_web.
globalThis.__rovesCreateFetch = (dependencies) => {
  "use strict";
  const { op_blitz_game_read, Blob, TextDecoder, TextEncoder, URL, location } = dependencies;

  const CONTENT_TYPES = {
    html: "text/html", htm: "text/html", js: "text/javascript", mjs: "text/javascript",
    css: "text/css", json: "application/json", txt: "text/plain", xml: "application/xml",
    png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp",
    gif: "image/gif", avif: "image/avif", svg: "image/svg+xml", ico: "image/x-icon",
    mp3: "audio/mpeg", ogg: "audio/ogg", wav: "audio/wav", m4a: "audio/mp4", flac: "audio/flac",
    mp4: "video/mp4", webm: "video/webm", woff: "font/woff", woff2: "font/woff2",
    ttf: "font/ttf", otf: "font/otf", wasm: "application/wasm", atlas: "text/plain", fnt: "text/plain",
  };
  const contentTypeFor = (pathname) => {
    const extension = pathname.split(".").pop()?.toLowerCase() ?? "";
    return CONTENT_TYPES[extension] ?? "application/octet-stream";
  };

  class Headers {
    #map = new Map();
    constructor(init) {
      if (init === undefined || init === null) return;
      const pairs = init instanceof Headers ? init.entries() : Array.isArray(init) ? init : Object.entries(init);
      for (const [name, value] of pairs) this.append(name, value);
    }
    append(name, value) {
      const key = String(name).toLowerCase();
      const previous = this.#map.get(key);
      this.#map.set(key, previous === undefined ? String(value) : `${previous}, ${value}`);
    }
    set(name, value) {
      this.#map.set(String(name).toLowerCase(), String(value));
    }
    get(name) {
      return this.#map.get(String(name).toLowerCase()) ?? null;
    }
    has(name) {
      return this.#map.has(String(name).toLowerCase());
    }
    delete(name) {
      this.#map.delete(String(name).toLowerCase());
    }
    *entries() {
      yield* [...this.#map.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    }
    *keys() {
      for (const [name] of this.entries()) yield name;
    }
    *values() {
      for (const [, value] of this.entries()) yield value;
    }
    forEach(callback, thisArg) {
      for (const [name, value] of this.entries()) callback.call(thisArg, value, name, this);
    }
    [Symbol.iterator]() {
      return this.entries();
    }
  }

  const toBytes = (body) => {
    if (body === null || body === undefined) return null;
    if (typeof body === "string") return new TextEncoder().encode(body);
    if (body instanceof ArrayBuffer) return new Uint8Array(body.slice(0));
    if (ArrayBuffer.isView(body)) return new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength));
    throw new TypeError("Response bodies of this type are not supported by the probe yet");
  };

  class Response {
    #bytes;
    #used = false;
    #status;
    #statusText;
    #headers;
    #url = "";
    constructor(body = null, init = {}) {
      this.#bytes = toBytes(body);
      this.#status = init.status ?? 200;
      this.#statusText = init.statusText ?? "";
      this.#headers = new Headers(init.headers);
      if (typeof body === "string" && !this.#headers.has("content-type")) {
        this.#headers.set("content-type", "text/plain;charset=UTF-8");
      }
    }
    static #withUrl(response, url) {
      response.#url = url;
      return response;
    }
    static fromProbe(body, init, url) {
      return Response.#withUrl(new Response(body, init), url);
    }
    static error() {
      return new Response(null, { status: 0 });
    }
    static json(data, init = {}) {
      const headers = new Headers(init.headers);
      if (!headers.has("content-type")) headers.set("content-type", "application/json");
      return new Response(JSON.stringify(data), { ...init, headers });
    }
    get ok() {
      return this.#status >= 200 && this.#status < 300;
    }
    get status() {
      return this.#status;
    }
    get statusText() {
      return this.#statusText;
    }
    get headers() {
      return this.#headers;
    }
    get url() {
      return this.#url;
    }
    get type() {
      return this.#status === 0 ? "error" : "basic";
    }
    get redirected() {
      return false;
    }
    get bodyUsed() {
      return this.#used;
    }
    #consume() {
      if (this.#used) throw new TypeError("Body has already been consumed.");
      this.#used = true;
      return this.#bytes ?? new Uint8Array(0);
    }
    async arrayBuffer() {
      const bytes = this.#consume();
      return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    }
    async bytes() {
      return new Uint8Array(await this.arrayBuffer());
    }
    async text() {
      return new TextDecoder().decode(this.#consume());
    }
    async json() {
      return JSON.parse(await this.text());
    }
    async blob() {
      return new Blob([this.#consume()], { type: this.#headers.get("content-type") ?? "" });
    }
    clone() {
      if (this.#used) throw new TypeError("Body has already been consumed.");
      const copy = new Response(this.#bytes, { status: this.#status, statusText: this.#statusText, headers: this.#headers });
      return Response.#withUrl(copy, this.#url);
    }
  }

  class Request {
    #url;
    #method;
    #headers;
    constructor(input, init = {}) {
      this.#url = input instanceof Request ? input.url : new URL(String(input), location.href).href;
      this.#method = String(init.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
      this.#headers = new Headers(init.headers ?? (input instanceof Request ? input.headers : undefined));
    }
    get url() {
      return this.#url;
    }
    get method() {
      return this.#method;
    }
    get headers() {
      return this.#headers;
    }
  }

  function dataResponse(url) {
    const text = url.href.slice("data:".length);
    const comma = text.indexOf(",");
    if (comma < 0) throw new TypeError("Invalid data: URL");
    const meta = text.slice(0, comma);
    const payload = text.slice(comma + 1);
    const base64 = /;base64$/i.test(meta);
    const type = (base64 ? meta.slice(0, -";base64".length) : meta) || "text/plain;charset=US-ASCII";
    let bytes;
    if (base64) {
      const binary = atob(decodeURIComponent(payload));
      bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    } else {
      bytes = new TextEncoder().encode(decodeURIComponent(payload));
    }
    return Response.fromProbe(bytes, { status: 200, statusText: "OK", headers: { "content-type": type } }, url.href);
  }

  async function fetch(input, init = {}) {
    const request = new Request(input, init);
    const url = new URL(request.url);
    if (url.protocol === "data:") return dataResponse(url);
    if (url.protocol === "game:") {
      if (request.method !== "GET" && request.method !== "HEAD") {
        throw new TypeError(`fetch: ${request.method} is not supported for game:// resources`);
      }
      let bytes;
      try {
        bytes = op_blitz_game_read(url.href);
      } catch {
        return Response.fromProbe(null, { status: 404, statusText: "Not Found" }, url.href);
      }
      const headers = { "content-type": contentTypeFor(url.pathname), "content-length": String(bytes.byteLength) };
      return Response.fromProbe(request.method === "HEAD" ? null : bytes, { status: 200, statusText: "OK", headers }, url.href);
    }
    throw new TypeError(`fetch: ${url.protocol} URLs are not available to games (no network access)`);
  }

  return { fetch, Headers, Request, Response };
};
