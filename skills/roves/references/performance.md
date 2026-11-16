# Measure frame timing

Call `performance_sample` with `{}` while the game runs. Sample once
after at least five seconds of idle, then during the action that stutters.
Compare matching scenes and record the sample count and P95/P99. CPU
durations are in milliseconds. The phase counts cover frames that reached
each phase. A phase with no samples has no measured duration.

The timing window covers the last five seconds. Presentation and skip
counters cover the session when the renderer supplies them. Compare their
deltas across calls. Present calls describe work handed to the surface.
Use PresentMon on Windows to measure actual display timing. GPU execution
time and displayed-frame counts are unavailable from the probe MCP.

Check `pixi_status` when a Pixi game stutters. Record the canvas backing
size and renderer resolution alongside the timing sample. Verify any
suspected cause by repeating the same scene and changing one setting.
