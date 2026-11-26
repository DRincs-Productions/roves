// Same-origin child frame of the fixture: the isMcpEnabled() marker must stay absent here.
globalThis.rovesMcpFrame = {
  enabled: window.__ROVES_MCP__ === true,
  own: Object.prototype.hasOwnProperty.call(window, "__ROVES_MCP__"),
};
console.log(`roves-mcp-frame enabled=${globalThis.rovesMcpFrame.enabled}`);
