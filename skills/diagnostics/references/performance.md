# Measure CEF frame timing

Use the Windows Release player and Chrome with the same web build, viewport,
DPR, refresh rate and canvas backing size. Separate cold startup, first opening
of a screen, repeat opening and idle. Record P95/P99 of displayed frame
intervals with PresentMon and attribute CPU to the host and Chromium processes.

Use the player's timing log to correlate host work with the display trace.
If the game is Pixi based, record the backing size and renderer resolution.
Compare one change at a time and report the scene and sample size.
See the [performance backlog](../../../docs/ROVES_PERFORMANCE.md).
