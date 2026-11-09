// fetch for games on deno_fetch 0.284.0 (loaded by dom.js). Headers, Request,
// Response, FormData and the data:/blob: fetches are Deno's; this file only adds
// the policy games need on top of them:
// - game://content/... reads the game's files through op_blitz_game_read and
//   answers with a Deno Response (404 when missing or outside --game);
// - remote requests to any *safe* origin, no allow-list: every hop is checked by
//   op_blitz_fetch_check (src/net_policy.rs: https only, no URL credentials, no
//   non-public IP literal, the document's CSP connect-src) and, at connection
//   time, by deno_fetch's resolver/request hook (public addresses only). They are
//   CORS requests without preflight: GET/HEAD, CORS-safelisted request headers,
//   no credentials, `Origin` set to the document's origin (`null` after a
//   cross-origin redirect), Access-Control-Allow-Origin checked on every response
//   including redirects, redirects followed by hand (at most 20) so each hop is
//   revalidated, response headers filtered as for a "cors" response;
// - every other scheme (http: outside the fixture, file:, ...) rejects with TypeError.
// Deno's own `fetch`, `Deno.*` and HttpClient stay out of the guest's reach.
globalThis.__rovesCreateFetch = (dependencies) => {
  "use strict";
  const { core, internals, primordials, op_blitz_game_read, op_blitz_fetch_check, URL, location } = dependencies;
  // Policy lookups use primordials, so a game patching Map/Set prototypes cannot
  // widen them. The destination policy itself is enforced in Rust.
  const { SafeMap, SafeSet, MapPrototypeGet, SetPrototypeHas } = primordials;

  // 26_fetch.js takes its OpenTelemetry hooks from `internals` and otherwise loads
  // ext:deno_telemetry, which this runtime does not include. Games get no tracing:
  // every flag reads false and every hook is a no-op.
  const inert = () => undefined;
  const disabledTelemetry = new Proxy({}, {
    get: (_target, name) => (typeof name === "string" && name.includes("ENABLED") ? false : inert),
  });
  internals.__telemetry ??= disabledTelemetry;
  internals.__telemetryUtil ??= disabledTelemetry;

  const { fetch: denoFetch } = core.loadExtScript("ext:deno_fetch/26_fetch.js");
  const { Headers } = core.loadExtScript("ext:deno_fetch/20_headers.js");
  const { FormData } = core.loadExtScript("ext:deno_fetch/21_formdata.js");
  const { Request } = core.loadExtScript("ext:deno_fetch/23_request.js");
  const { Response, toInnerResponse } = core.loadExtScript("ext:deno_fetch/23_response.js");
  // Base URL of `new Request("relative")` (Deno reads it from deno_web's location
  // state). fetch() itself resolves against the current, pushState-aware URL.
  core.loadExtScript("ext:deno_web/12_location.js").setLocationHref?.(location.href);

  const MAX_REDIRECTS = 20;
  const REDIRECT_STATUSES = new SafeSet([301, 302, 303, 307, 308]);
  const NULL_BODY_STATUSES = new SafeSet([204, 205, 304]);

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

  // Fetch spec: CORS-safelisted request headers (no preflight needed). Content-Type
  // is left out because GET/HEAD carry no body; Range is left out too.
  const CORS_UNSAFE_BYTE = /[\x00-\x08\x0a-\x1f"():<>?@[\\\]{}\x7f]/;
  const LANGUAGE_VALUE = /^[0-9A-Za-z *,\-.;=]*$/;
  // A Map, so header names such as "constructor" never hit Object.prototype.
  const SAFELISTED_REQUEST_HEADERS = new SafeMap([
    ["accept", (value) => !CORS_UNSAFE_BYTE.test(value)],
    ["accept-language", (value) => LANGUAGE_VALUE.test(value)],
    ["content-language", (value) => LANGUAGE_VALUE.test(value)],
  ]);
  const SAFELISTED_RESPONSE_HEADERS = new SafeSet([
    "cache-control", "content-language", "content-length", "content-type", "expires", "last-modified", "pragma",
  ]);

  // The URL list (so `url` and `redirected`) and type a browser reports;
  // deno_fetch's constructor leaves both unset.
  const finish = (response, urls, type) => {
    const inner = toInnerResponse(response);
    for (const url of urls) {
      const withoutFragment = new URL(url);
      withoutFragment.hash = "";
      inner.urlList.push(withoutFragment.href);
    }
    inner.type = type;
    return response;
  };

  function gameResponse(request) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      throw new TypeError(`fetch: ${request.method} is not supported for game:// resources`);
    }
    let bytes;
    try {
      bytes = op_blitz_game_read(request.url);
    } catch {
      return finish(new Response(null, { status: 404, statusText: "Not Found" }), [request.url], "basic");
    }
    const headers = {
      "content-type": contentTypeFor(new URL(request.url).pathname),
      "content-length": String(bytes.byteLength),
    };
    const body = request.method === "HEAD" ? null : bytes;
    return finish(new Response(body, { status: 200, statusText: "OK", headers }), [request.url], "basic");
  }

  async function corsFetch(request, init) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      throw new TypeError(`fetch: only GET and HEAD are allowed for remote requests, not ${request.method}`);
    }
    if ((init?.credentials ?? "same-origin") === "include") {
      throw new TypeError('fetch: credentials: "include" is not supported for remote requests');
    }
    const mode = init?.mode ?? "cors";
    if (mode !== "cors") {
      throw new TypeError(`fetch: request mode "${mode}" is not supported for remote requests`);
    }
    const redirectMode = request.redirect;
    if (redirectMode !== "follow" && redirectMode !== "error") {
      throw new TypeError(`fetch: redirect: "${redirectMode}" is not supported for remote requests`);
    }
    const safelisted = [];
    for (const [name, value] of request.headers) {
      if (!MapPrototypeGet(SAFELISTED_REQUEST_HEADERS, name)?.(value) || value.length > 128) {
        throw new TypeError(`fetch: header "${name}" would need a CORS preflight, which is not supported`);
      }
      safelisted.push([name, value]);
    }
    const documentUrl = location.href;
    let origin = new URL(documentUrl).origin;
    let current = new URL(request.url);
    const urls = [];
    for (let redirects = 0; ; redirects++) {
      // Rust policy for this hop (throws TypeError): https, no URL credentials,
      // no non-public IP literal, document CSP. Host names are checked again on
      // the resolved addresses when deno_fetch connects.
      op_blitz_fetch_check(current.href, documentUrl, redirects);
      const headers = new Headers(safelisted);
      headers.set("origin", origin);
      // "manual": deno_fetch returns the 3xx itself, so the hop is never
      // followed without passing through this loop.
      const response = await denoFetch(current.href, {
        method: request.method,
        headers,
        redirect: "manual",
        signal: request.signal,
      });
      urls.push(current.href);
      const allowOrigin = response.headers.get("access-control-allow-origin");
      if (allowOrigin !== "*" && allowOrigin !== origin) {
        await response.body?.cancel();
        throw new TypeError(`fetch: CORS check failed for ${current.href} (Access-Control-Allow-Origin: ${allowOrigin})`);
      }
      const target = SetPrototypeHas(REDIRECT_STATUSES, response.status) ? response.headers.get("location") : null;
      if (target === null) return finish(filterResponse(response), urls, "cors");
      await response.body?.cancel();
      if (redirectMode === "error") {
        throw new TypeError(`fetch: ${current.href} redirected and the request's redirect mode is "error"`);
      }
      if (redirects >= MAX_REDIRECTS) {
        throw new TypeError(`fetch: too many redirects from ${request.url}`);
      }
      const next = new URL(target, current);
      // Fetch spec: a cross-origin redirect of a CORS request continues with an
      // opaque (null) origin. GET/HEAD keep their method on every redirect status.
      if (next.origin !== current.origin) origin = "null";
      current = next;
    }
  }

  // A "cors" filtered response: safelisted headers plus Access-Control-Expose-Headers,
  // never Set-Cookie.
  function filterResponse(response) {
    const exposed = new SafeSet(
      (response.headers.get("access-control-expose-headers") ?? "")
        .split(",")
        .map((name) => name.trim().toLowerCase())
        .filter(Boolean),
    );
    const filtered = new Headers();
    for (const [name, value] of response.headers) {
      if (name === "set-cookie" || name === "set-cookie2") continue;
      if (
        SetPrototypeHas(SAFELISTED_RESPONSE_HEADERS, name) ||
        SetPrototypeHas(exposed, "*") ||
        SetPrototypeHas(exposed, name)
      ) {
        filtered.append(name, value);
      }
    }
    const responseInit = { status: response.status, statusText: response.statusText, headers: filtered };
    // Null body statuses (204, 205, 304) may not be given a body, even an empty one.
    const body = SetPrototypeHas(NULL_BODY_STATUSES, response.status) ? null : response.body;
    return new Response(body, responseInit);
  }

  async function fetch(input, init = undefined) {
    // Relative URLs resolve against the current document URL (it follows pushState).
    const target = input instanceof Request ? input : new URL(String(input), location.href).href;
    const request = new Request(target, init);
    if (request.signal.aborted) throw request.signal.reason;
    const protocol = new URL(request.url).protocol;
    switch (protocol) {
      case "data:":
      case "blob:":
        return denoFetch(request);
      case "game:":
        return gameResponse(request);
      case "http:":
      case "https:":
        return corsFetch(request, init);
      default:
        throw new TypeError(`fetch: ${protocol} URLs are not available to games`);
    }
  }

  return { fetch, FormData, Headers, Request, Response };
};
