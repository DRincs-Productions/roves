// fetch on deno_fetch: data:/blob: through Deno, game:// 404 outside --game,
// CORS and revalidated redirects against the loopback server main.rs starts in
// fixture mode (`fetchTestServer`), the safe-destination policy (https, public
// addresses, CSP connect-src from fixtures/layout.html), Response API.
// No test needs the network: refused requests never leave the process, and
// https://localhost:1 is refused by the resolver before connecting.
// Async: main.rs drives frames until done, then fetch_check.js verifies.
(() => {
  "use strict";
  const result = { done: false, failures: [] };
  globalThis.fetchResult = result;
  const expect = (condition, message) => {
    if (!condition) result.failures.push(message);
  };
  const rejectsWith = async (name, label, run) => {
    try {
      await run();
      result.failures.push(`${label}: resolved, expected ${name}`);
    } catch (error) {
      expect(error?.name === name, `${label}: ${error?.name} ${error?.message}`);
    }
  };
  (async () => {
    const server = globalThis.fetchTestServer;
    const hello = await fetch("data:text/plain;base64,SGVsbG8=");
    expect(hello instanceof Response && hello.url.startsWith("data:"), "data: goes through deno_fetch");
    expect(hello.ok && hello.status === 200, "data: URL status");
    expect(hello.headers.get("Content-Type") === "text/plain", `data: content-type ${hello.headers.get("content-type")}`);
    expect((await hello.text()) === "Hello", "data: base64 text");
    expect(hello.bodyUsed, "bodyUsed after text()");
    let consumedTwice = false;
    try {
      await hello.text();
    } catch (error) {
      consumedTwice = error instanceof TypeError;
    }
    expect(consumedTwice, "a body can be consumed once");

    const json = await fetch("data:application/json,%7B%22a%22%3A1%7D");
    expect((await json.json()).a === 1, "data: JSON");

    const blobUrl = URL.createObjectURL(new Blob(["from a blob"], { type: "text/plain" }));
    const fromBlob = await fetch(blobUrl);
    expect((await fromBlob.text()) === "from a blob", "blob: URL through deno_fetch");
    URL.revokeObjectURL(blobUrl);

    const missing = await fetch("game://content/does-not-exist.png");
    expect(missing instanceof Response && missing.status === 404 && !missing.ok, `game:// missing file status ${missing.status}`);
    expect(missing.url === "game://content/does-not-exist.png" && missing.type === "basic", `game:// url/type ${missing.url} ${missing.type}`);
    const relative = await fetch("missing-relative.png#fragment");
    const expected = new URL("missing-relative.png", location.href).href;
    expect(relative.status === 404 && relative.url === expected, `relative URL ${relative.url} (expected ${expected})`);
    await rejectsWith("TypeError", "game:// POST", () => fetch("game://content/x.json", { method: "POST", body: "x" }));

    // Remote: any safe origin, no allow-list. The fixture's loopback server is the
    // only non-https/non-public destination accepted, and only in fixture mode.
    const cors = await fetch(`${server}/cors`, { headers: { Accept: "text/plain" } });
    expect(cors.status === 200 && cors.type === "cors" && cors.url === `${server}/cors` && !cors.redirected, `CORS response ${cors.status} ${cors.type} ${cors.url}`);
    expect(cors.headers.get("x-exposed") === "yes" && cors.headers.get("content-type") === "text/plain", "safelisted and exposed headers kept");
    expect(cors.headers.get("x-hidden") === null && cors.headers.get("set-cookie") === null, "unexposed headers and Set-Cookie filtered");
    const echoed = await cors.text();
    expect(echoed === `origin=${new URL(location.href).origin};credentials=false`, `Origin sent, no credentials: ${echoed}`);
    const head = await fetch(`${server}/cors`, { method: "HEAD" });
    expect(head.ok && (await head.text()) === "", "HEAD has no body");
    await rejectsWith("TypeError", "missing Access-Control-Allow-Origin", () => fetch(`${server}/no-cors`));
    // Redirects are followed by hand, revalidating every hop.
    const redirected = await fetch(`${server}/redirect`);
    expect(redirected.status === 200 && redirected.redirected && redirected.url === `${server}/cors`, `same-origin redirect ${redirected.status} ${redirected.redirected} ${redirected.url}`);
    expect((await redirected.text()).startsWith("origin="), "redirected body");
    await rejectsWith("TypeError", "redirect mode error", () => fetch(`${server}/redirect`, { redirect: "error" }));
    await rejectsWith("TypeError", "redirect mode manual", () => fetch(`${server}/redirect`, { redirect: "manual" }));
    await rejectsWith("TypeError", "redirect without Access-Control-Allow-Origin", () => fetch(`${server}/redirect-no-cors`));
    await rejectsWith("TypeError", "redirect loop", () => fetch(`${server}/redirect-loop`));
    // Unsafe hops are refused by the Rust policy before they are contacted.
    const refusedWith = async (label, pattern, run) => {
      try {
        await run();
        result.failures.push(`${label}: resolved, expected a TypeError`);
      } catch (error) {
        expect(error?.name === "TypeError" && pattern.test(error?.message ?? ""), `${label}: ${error?.name} ${error?.message}`);
      }
    };
    await refusedWith("redirect to http:", /https: origins/, () => fetch(`${server}/redirect-http`));
    await refusedWith("redirect to a link-local IP", /non-public address/, () => fetch(`${server}/redirect-private`));
    await refusedWith("redirect with credentials", /embedded credentials/, () => fetch(`${server}/redirect-credentials`));
    await rejectsWith("TypeError", "redirect to localhost (refused by the resolver)", () => fetch(`${server}/redirect-localhost`));
    await rejectsWith("TypeError", "POST", () => fetch(`${server}/cors`, { method: "POST", body: "x" }));
    await rejectsWith("TypeError", "non-safelisted header", () => fetch(`${server}/cors`, { headers: { "X-Custom": "1" } }));
    await rejectsWith("TypeError", "credentials include", () => fetch(`${server}/cors`, { credentials: "include" }));
    await rejectsWith("TypeError", "no-cors mode", () => fetch(`${server}/cors`, { mode: "no-cors" }));
    // Refused by net_policy or already by Deno's Request constructor (spec).
    await refusedWith("URL credentials", /credentials/i, () => fetch(server.replace("://", "://user:pw@") + "/cors"));
    await rejectsWith("AbortError", "aborted signal", () => fetch(`${server}/cors`, { signal: AbortSignal.abort() }));
    // Other loopback ports are not the fixture server: plain http is refused.
    await refusedWith("other loopback port over http", /https: origins/, () => fetch("http://127.0.0.1:1/"));
    await refusedWith("plain http origin", /https: origins/, () => fetch("http://example.com/"));
    await refusedWith("loopback IP literal", /non-public address/, () => fetch("https://127.0.0.1/"));
    await refusedWith("obfuscated loopback IP", /non-public address/, () => fetch("https://0x7f.1/"));
    await refusedWith("IPv6 loopback", /non-public address/, () => fetch("https://[::1]/"));
    await refusedWith("IPv4-mapped private", /non-public address/, () => fetch("https://[::ffff:192.168.0.1]/"));
    await refusedWith("private network", /non-public address/, () => fetch("https://10.0.0.1/"));
    await refusedWith("cloud metadata", /non-public address/, () => fetch("https://169.254.169.254/latest/meta-data/"));
    // A name the static check accepts and CSP allows (https://localhost:*): the
    // resolver refuses it on the resolved loopback address (main.rs asserts that
    // the refusal came from the resolver, whatever message deno_fetch reports).
    await rejectsWith("TypeError", "localhost name refused at resolution", () => fetch("https://localhost:1/"));
    // The fixture's <meta http-equiv="Content-Security-Policy"> does not list it.
    await refusedWith("document CSP connect-src", /Content-Security-Policy/, () => fetch("https://blocked.invalid/"));
    await rejectsWith("TypeError", "file: URLs", () => fetch("file:///etc/hosts"));

    const form = new FormData();
    form.append("field", "value");
    expect((await new Response(form).text()).includes("value"), "FormData body from deno_fetch");

    const response = new Response("abc", { status: 201, headers: { "X-Test": "1" } });
    expect(response.status === 201 && response.headers.get("x-test") === "1", "Response init");
    const copy = response.clone();
    const blob = await response.blob();
    expect(blob instanceof Blob && blob.size === 3, "Response.blob()");
    expect(new Uint8Array(await copy.arrayBuffer())[0] === 97, "Response.clone() keeps the body");
    expect((await Response.json({ b: 2 }).json()).b === 2, "Response.json()");

    // Image decoding through deno_image: a 2x2 red PNG from a data: URL.
    const png = await fetch("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=");
    const bitmap = await createImageBitmap(await png.blob());
    expect(bitmap instanceof ImageBitmap && bitmap.width === 2 && bitmap.height === 2, `createImageBitmap ${bitmap?.width}x${bitmap?.height}`);

    // MutationObserver: records delivered in a microtask.
    const host = document.createElement("div");
    document.body.appendChild(host);
    const records = [];
    const observer = new MutationObserver((list) => records.push(...list));
    observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeOldValue: true, characterData: true, characterDataOldValue: true });
    const child = document.createElement("span");
    host.appendChild(child);
    child.setAttribute("data-a", "1");
    child.setAttribute("data-a", "2");
    const text = document.createTextNode("x");
    child.appendChild(text);
    text.textContent = "y";
    host.removeChild(child);
    expect(records.length === 0, "MutationObserver delivers asynchronously");
    await Promise.resolve();
    const summary = records.map((record) => `${record.type}:${record.attributeName ?? ""}:${record.oldValue ?? ""}:${record.addedNodes.length}/${record.removedNodes.length}`).join(" ");
    expect(summary === "childList:::1/0 attributes:data-a::0/0 attributes:data-a:1:0/0 childList:::1/0 characterData::x:0/0 childList:::0/1", `MutationObserver records: ${summary}`);
    const filtered = [];
    const filteredObserver = new MutationObserver((list) => filtered.push(...list));
    filteredObserver.observe(host, { attributes: true, attributeFilter: ["data-keep"] });
    host.setAttribute("data-skip", "1");
    host.setAttribute("data-keep", "1");
    expect(filteredObserver.takeRecords().map((record) => record.attributeName).join() === "data-keep", "attributeFilter and takeRecords");
    observer.disconnect();
    filteredObserver.disconnect();
    host.setAttribute("data-after", "1");
    await Promise.resolve();
    expect(filtered.length === 0, "takeRecords empties the queue");
    host.remove();
    result.done = true;
  })().catch((error) => {
    result.failures.push(`threw: ${error?.stack ?? error}`);
    result.done = true;
  });
})();
