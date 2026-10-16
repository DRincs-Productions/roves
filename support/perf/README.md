# Focused browser benchmarks

Serve this directory locally, for example with `python -m http.server 48762 --bind 127.0.0.1 --directory support/perf`.

- `audio-analysis-bench.html`: click Start; a sine oscillator feeds an AnalyserNode and a muted
  destination. For fftSize 2048 and 8192, take 12 frequency reads on separate animation frames
  after warm-up. Report median/max (the p95 of 12 samples is the maximum), peak bin and dB.
  `?auto=1` starts without input and closes the test window on completion for Roves headless runs.
  A browser autoplay policy can require the button; do not treat a suspended context as a result.
- `webgl-getters-bench.html`: five timed batches of 2000 width/height pairs for WebGL 1 and 2,
  after one warm-up batch. Check the accumulated dimensions and report median/max.

Use the same page, browser build, hardware and rendering mode for before/after comparisons.
Timing these individual APIs does not establish whole-game CPU, GPU or RAM parity with Chrome.
Keep foreground and headless samples separate. Results are in `ROVES_PERFORMANCE.md`.

Roves can run the audio test from a terminal with `play.exe -z http://127.0.0.1:48762/audio-analysis-bench.html?auto=1`.
On PowerShell, pipe stdout to `Out-File` to wait for the GUI-subsystem executable and retain the
`ANALYSERSTATS` / `ANALYSERBENCH PASS` results. Use a new output file for each run.
