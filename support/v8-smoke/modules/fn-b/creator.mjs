// Each body names its directory: V8's eval cache would reuse the first origin for an
// identical body, as with the builtin constructor.
export function load() { return Function("return import('./leaf.mjs') /* b */")(); }
