# Focused browser benchmarks

Serve this directory locally, for example with `python -m http.server 48762 --bind 127.0.0.1 --directory support/perf`.

- `audio-analysis-bench.html`: click Start; a sine oscillator feeds an AnalyserNode and a muted
  destination. For fftSize 2048 and 8192, take 12 frequency reads on separate animation frames
  after warm-up. Report median/max (the p95 of 12 samples is the maximum), peak bin and dB.
  `?auto=1` starts without input and closes the test window on completion for Roves headless runs.
  A browser autoplay policy can require the button; do not treat a suspended context as a result.
- `webgl-getters-bench.html`: five timed batches of 2000 width/height pairs for WebGL 1 and 2,
  after one warm-up batch. Check the accumulated dimensions and report median/max.
- `analyser-transport-bench.html?nodes=8`: 0–32 analysers with silent output, a 2 s warm-up
  and 12 s sampling window. Default: no analysis reads; `&read=1` reads time-domain data at
  approximately 60 Hz. A final 1 s busy loop exercises backlog recovery; exclude it from CPU
  samples. Checks finite, non-silent 32768-sample history. `&auto=1` starts and closes Roves.
  Record the actual sample rate; Chrome and Roves can choose different device rates.
- `worker-lifecycle-bench.html?auto=1`: retain four idle workers, each holding 8 MiB, then
  terminate them while keeping their main-thread objects alive. Log baseline/ready/terminated/
  after phases; sample OS thread IDs and private memory at each phase. In Roves' current
  one-thread-per-worker implementation, added worker threads should disappear after termination.
  Browsers with reusable thread pools may retain threads. Memory can remain cached by allocators.
  `workers=1..8` and `mib=1..16` control the load. The page remains open for external sampling;
  its completion marker alone is not evidence that worker runtimes were released.
  Automatic mode hides the page and logs only to the console, avoiding glyph/render-worker
  activity from updating the diagnostic output while OS thread IDs are sampled.
- `worker-messaging-bench.html?auto=1`: worker startup/constructor time and sequential
  round-trips of 1 KiB, 128 KiB and 4 MiB ArrayBuffers, cloned or transferred both ways.
  Checks detachment and payload endpoints. Four warm-up iterations precede 16 timed samples;
  their p95 is the maximum, not a robust tail estimate. Report enqueue time separately from
  round-trip latency. Repeat in new Roves processes and record Chrome MCP browser/context state.
- `worker-canvas-probe.html`: report worker OffscreenCanvas/requestAnimationFrame exposure
  and attempt WebGL 1/2 contexts with a red-pixel readback. API existence alone does not mean
  worker rendering works. This probe does not validate presentation into a transferred DOM canvas.
- `worker-queue-probe.html`: check 128 FIFO replies, a later message after the burst, canceled
  timers and a final ping. Empty wake-ups must not terminate a still-live worker. This is a
  functional regression probe, not a throughput benchmark.
- `worker-compute-bench.html?workers=1`: fixed 64 jobs of 1048576 integer iterations across
  1/2/4/8 workers, with a warm-up and deterministic result checks. `&cached=1` uses a captured
  Math.imul function as a control for global-lookup overhead. Startup is reported separately.
  More workers can improve completion time while saturating more cores; this is not a whole-game
  CPU saving. Short fast runs do not provide enough rAF samples for frame-percentile claims.
- `main-intrinsics-bench.html`: warm-up and three alternating direct/captured Math.imul
  runs on the main thread, 8388608 integer iterations each, checking identical results.
  Logs WORKERBENCH with scene=main-intrinsics for the shared terminal runner. This isolates
  global-lookup overhead, not whole-game performance; the page remains open after completion.

- `native-globals-probe.html`: nine functional checks for standard intrinsics, DOM constructors,
  Image/Audio/Option aliases, an initialized property descriptor and iframe constructor realms.
  Logs WORKERBENCH with scene=native-globals. This is not a performance benchmark.

Use the same page, browser build, hardware and rendering mode for before/after comparisons.
Timing these individual APIs does not establish whole-game CPU, GPU or RAM parity with Chrome.
Keep foreground and headless samples separate. Results are in [`ROVES_PERFORMANCE.md`](../../docs/ROVES_PERFORMANCE.md).

Roves can run the audio test from a terminal with `play.exe -z http://127.0.0.1:48762/audio-analysis-bench.html?auto=1`.
On PowerShell, pipe stdout to `Out-File` to wait for the GUI-subsystem executable and retain the
`ANALYSERSTATS` / `ANALYSERBENCH PASS` results. Use a new output file for each run.
Quote URLs containing `&` in PowerShell. Do not add `-x` to the audio benchmarks: it exits
when the image becomes stable, before the timed audio checks finish. `-o` requires an image
filename and is not needed for these tests; the benchmark's `auto=1` closes the window.
