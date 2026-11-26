// The fixture's entry script (index.html loads it first, classic and parser-blocking): what
// `isMcpEnabled()` of @drincs/roves-api/core would read here, before any other page code.
const descriptor = Object.getOwnPropertyDescriptor(window, "__ROVES_MCP__");
globalThis.rovesMcpEntry = {
  enabled: window.__ROVES_MCP__ === true,
  descriptor: descriptor
    ? {
        value: descriptor.value,
        writable: descriptor.writable,
        enumerable: descriptor.enumerable,
        configurable: descriptor.configurable,
      }
    : null,
};
console.log(`roves-mcp-entry enabled=${globalThis.rovesMcpEntry.enabled}`);
// What `isAvailable()` reads: the host API marker, defined in every game-mode launch.
const host = window.__ROVES__;
globalThis.rovesHostEntry = host ?? null;
console.log(
  `roves-host-entry transport=${host?.transport} api=${host?.api} frozen=${Object.isFrozen(host ?? {}) && host !== undefined}`,
);
