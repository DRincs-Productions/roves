globalThis.referrerImporters.push(() => import("./owner.mjs"), () => eval('import("./owner.mjs")'), new Function('return import("./owner.mjs")'));
