# Measure frame timing

Use a Release player on Windows and Chrome with the same web build, viewport, device pixel
ratio, refresh rate and canvas backing size. Measure cold startup, the first opening of a
screen, a repeated opening and idle separately. Record the P95 and P99 of displayed frame
intervals with PresentMon, and attribute CPU to the player and Chromium processes.

Correlate host work with the display trace through the player's timing log. For a PixiJS
game, record the canvas backing size and the renderer resolution (`pixi_scene` reports both
and the ticker FPS). Change one thing at a time and report the scene and the sample size.

A `watch` sequence shows what changed on screen, not how fast: its timestamps include the
capture round trip, so never use it as a frame-timing measurement.
