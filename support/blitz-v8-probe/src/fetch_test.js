// fetch: data: URLs, game:// 404 outside --game, no network, Response API.
// Async: fetch_check.js verifies after the frame loop ran the promises.
(() => {
  "use strict";
  const result = { done: false, failures: [] };
  globalThis.fetchResult = result;
  const expect = (condition, message) => {
    if (!condition) result.failures.push(message);
  };
  (async () => {
    const hello = await fetch("data:text/plain;base64,SGVsbG8=");
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

    const missing = await fetch("game://content/does-not-exist.png");
    expect(missing.status === 404 && !missing.ok, `game:// missing file status ${missing.status}`);

    let blocked = false;
    try {
      await fetch("https://example.com/");
    } catch (error) {
      blocked = error instanceof TypeError;
    }
    expect(blocked, "network URLs are rejected");

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
    result.done = true;
  })().catch((error) => {
    result.failures.push(`threw: ${error?.stack ?? error}`);
    result.done = true;
  });
})();
