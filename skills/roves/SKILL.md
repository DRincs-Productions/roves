---
name: roves
description: Inspect and control a local Roves game through its diagnostic MCP server, including DOM checks, PixiJS scene inspection, and live frame timing analysis.
---

# Diagnose a Roves game

Use a diagnostic session to reproduce a specific game action, inspect its
state, and collect frame timings. Start with the
[connection guide](references/mcp.md), then discover the available tools
with the MCP client's tool list.

Read page text and visible controls before interacting. Use
`evaluate_script` to inspect the DOM or invoke a specific control with
`click()`. A click from JavaScript is synthetic. If a flow requires trusted
input, reproduce it using the native window and record that distinction.

For stutter, compare menu idle, the same dialogue, and Skip using the
[performance guide](references/performance.md). Close the MCP session when
finished so the diagnostic game and its temporary saves can be cleaned up.
