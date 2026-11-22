# CEF accelerated OSR -> wgpu-weld -> Netrender probe (Windows feasibility)

Isolated experiment, separate Cargo workspace. Nothing in the engine, `roves-action`,
Packmaster, any release or the stage-1 window probe (`support/cef-window-probe`, CEF 156,
`.github/workflows/cef-window-probe.yml`, both unchanged) depends on it.

Question: can Chromium (through CEF, its own V8 running the page) paint into a GPU texture
that a Roves-owned wgpu device composites with Netrender, without a CEF fork, without a CPU
copy on the presentation path and without giving up the Windows sandbox? Deno host services,
IPC, input, audio, `game://`, CSP and the single-document policy are out of scope here.

**Status (2026-11-22): built in CI (run 45038791259) and run. On a real GPU (GTX 1050,
DX12) both compositions pass all 16 pixel samples, before and after the resize, with a clean
close and exit 0. On WARP CEF delivers no accelerated frame (exit 2, as the workflow
expects).** Evidence below. Frame cost and pacing are not measured yet: the optional
`--roves-timings` summaries (below) exist for that comparison and have not been run on
hardware yet.

## Evidence (2026-11-22, CI run 45038791259)

Same bundle (`cef-netrender-probe-windows-x64`), CEF 154.0.34 / Chromium 154.0.8037.98,
welding 0.15.0@e991684, Netrender d2f13ca5, wgpu 29.0.4.

| Run | Adapter | Result |
| --- | --- | --- |
| local, `--roves-compose=netrender` | NVIDIA GeForce GTX 1050, Dx12, DiscreteGpu, driver 32.0.15.8163 | exit 0; 16/16 `PASS pixel`; sandboxed; surface `Bgra8Unorm`; import `Bgra8Unorm`, usage `COPY_SRC \| TEXTURE_BINDING`, `path=dx12-shared-handle`; first frame at 1280x720 = import 1 (generation 4), at 960x540 = import 23 (generation 37); `closing imported=42 presented=54 elapsed_ms=3188` |
| local, `--roves-compose=wgpu` | same | exit 0; 16/16 `PASS pixel`; first frames import 1 (generation 2) and import 24 (generation 27); `closing imported=43 presented=81 elapsed_ms=1784` |
| CI, `--roves-compose=wgpu` | Microsoft Basic Render Driver (WARP), Dx12, Cpu, 10.0.26100.33648 | exit 2: `FAIL timeout in phase Initial: load_ended=true imported=0 at_size=0`, `presented=708` (background only) over `elapsed_ms=90056`; close and `cef shutdown complete` clean |

What the real-GPU runs show: Chromium paints into a GPU texture that a Roves-owned wgpu 29
device imports and composites, with Netrender's scene visible through transparent page
pixels, premultiplied alpha in the imported texel (`[0,0,128,128]` for 50% blue) and the
blend correct within tolerance (`[127,0,128,255]` vs `[128,0,128,255]`), at both sizes, under
the sandbox, without a CEF fork and without a CPU copy on the presentation path. Generation
gaps between imports (paint generations advance faster than imports) show that welding's
single-slot mailbox replaced some paints before the host took them; how many per second is
not measured.

What they do not show: per-frame cost, pacing or GPU load. `elapsed_ms` and `presented`
span startup, page load, both waits for 20 frames, the resize and the verification
readbacks, so they are **not** a frame-rate comparison between the two compositions.

WARP: the gpu-process starts, but both renderer processes are launched with
`--disable-gpu-compositing` (`process-command-lines.txt`), so Chromium falls back to software
compositing and `OnAcceleratedPaint` never fires; `cef-debug.log` only reports
`direct_composition_support.cc: GetGpuDriverOverlayInfo: Failed to retrieve video device`.
The accelerated path is therefore verified only on real hardware; WARP keeps proving build,
sandbox, process topology and clean shutdown. No renderer command line contained
`--no-sandbox`. The first winit size on the runner was 752x480 before the requested
1280x720 applied.

## Pipeline

1. CEF's `bootstrap.exe` (shipped as `roves_cef_netrender_probe.exe`) initializes the
   Windows sandbox broker and calls `RunWinMain` in `roves_cef_netrender_probe.dll`.
   welding's `CefWindowsSandboxContext::from_raw` borrows the instance and sandbox info,
   `execute_process` returns for subprocesses and `initialize` starts CEF with
   `CefSandboxMode::Sandboxed`. Without sandbox information the browser process exits 3;
   there is no unsandboxed path.
2. A winit window and a wgpu 29 DX12 instance/surface; `netrender::boot_on` (Netrender's
   tenancy contract) requests the single adapter/device/queue. The same `wgpu::Device`
   and `Queue` go to welding's `HostWgpuContext` (backend must be `Dx12`). One device: no
   second device, no wgpu-core id adoption, no wgpu patch (unlike the Deno shared-device
   proof, nothing here owns wgpu-core ids outside wgpu).
3. `WindowsCefProducer` (welding) creates a windowless browser with
   `shared_texture_enabled` (accelerated OSR), `windowless_frame_rate = 60`, transparent
   background, on the fixture `src/fixture.html` loaded as a `data:` URL (no server).
4. Per paint, CEF calls `OnAcceleratedPaint` with a callback-scoped D3D11 shared NT handle.
   welding duplicates it, takes the keyed mutex and `CopyResource`s it on the GPU into a
   welding-owned shared texture, waits for that copy with a D3D11 event query, creates an
   NT handle, and grafting (`import_dx12_shared_texture`, d903cfa) opens it on the host's
   D3D12 device and wraps it as a `wgpu::Texture` (`TEXTURE_BINDING | COPY_SRC`). welding
   then submits a 1-texel copy into an unmapped buffer and waits for it (its documented
   cache-visibility step for the D3D11 -> D3D12 handoff).
5. `--roves-compose=netrender` (default): Netrender d2f13ca5 renders its own scene (opaque
   red background, a yellow panel in the bottom-right quarter) with Vello into its master
   (`Rgba8Unorm`), then composites the imported CEF texture over it as an external texture
   with `SourceAlpha::Premultiplied`, placed 1:1 at the frame's own size and after every
   scene op (no tail fragment, so the d2f13ca5 ordering issue of the shared-device proof is
   never involved). `--roves-compose=wgpu`: same import, composed by one
   premultiplied-alpha blit over a red clear (exists because Vello crashes on WARP, see
   CI).
6. A nearest-sampling blit draws the composed texture onto a non-sRGB surface
   (`Bgra8Unorm`/`Rgba8Unorm`, `Opaque`), viewport = source size. Present.

No step maps a buffer or copies pixels to the CPU to present. The only CPU readbacks are
the verification samples of the two checks below, issued after the frame was presented and
logged as `verification-readback ... (not on the presentation path)`.

## Checks (automatic mode)

After `load-end` and 20 imported frames whose size equals the surface:

| Point (fraction of the viewport) | Page content | Composed RGBA (netrender / wgpu) | CEF texel, premultiplied RGBA |
| --- | --- | --- | --- |
| (0.25, 0.25) | opaque `rgb(0,255,0)` | `[0,255,0,255]` | `[0,255,0,255]` |
| (0.75, 0.25) | `rgba(0,0,255,0.5)` | `[128,0,128,255]` (50% blue over red) | `[0,0,128,128]` |
| (0.25, 0.75) | nothing (transparent) | `[255,0,0,255]` (background) | `[0,0,0,0]` |
| (0.75, 0.75) | nothing (transparent) | `[255,255,0,255]` Netrender panel / `[255,0,0,255]` | `[0,0,0,0]` |

Tolerance 3/255 per channel. The CEF texel row proves premultiplied alpha and real
transparency in the imported texture itself; the composed row proves the blend and that
the Netrender scene shows through. Then the window is resized from 1280x720 to 960x540
(`request_inner_size`, surface reconfigured, `producer.resize`), the probe waits for 20
frames at the new size (the imported frame must match the surface; it is never stretched)
and repeats the 4x2 samples. Then `producer.close()`, wait for CEF's close, drain the
queue, drop producer and renderer, then `CefRuntime` (`cef::shutdown`).

Exit codes: 0 both checks passed; 1 a pixel/resize/close failure or usage error; 2 timeout
(no load, no frames at the expected size, or close not completed) or fewer checks than
expected; 3 no sandbox; 4 `cef initialize` failed; 5 window/wgpu/Netrender/producer startup
failed. `FAIL ...` lines name each failure.

Options: `--roves-log=<file>`, `--roves-compose=netrender|wgpu`,
`--roves-cache-dir=<dir>` (default a per-run directory under `%TEMP%`),
`--roves-timeout-ms=<ms>` (default 120000), `--roves-keep-open` (one check, then the
window stays until the OS closes it), `--roves-timings` (below; off by default, CI does not
pass it), `--roves-sustained` with `--roves-warmup-ms=<ms>` (default 10000) and
`--roves-sample-ms=<ms>` (default 30000) (below; off by default, CI does not pass it,
exclusive with `--roves-keep-open`). CEF switches such as `--log-file` pass through.

## Optional timings (`--roves-timings`)

**Smoke only.** Each smoke phase ends after about 20 imported frames at its size (well under
a second on a real GPU), so these summaries hold a few dozen samples that include the
first frames after load or resize. They show where the time goes, not steady-state pacing:
use `--roves-sustained` (next section) for that. With the option, each phase (`initial`, `resized`; `*-timeout` if it timed out) ends with a
header line and one line per stage, emitted **before** that phase's verification readback:

```text
roves-cef-nr timings phase=initial compose=wgpu clock=wall-monotonic unit=us window_ms=... presented=... imported=... superseded_producer_frames=... empty_acquire_polls=... dropped_samples=... excluded=loading,verification-readback,closing percentile=nearest-rank
roves-cef-nr timings phase=initial stage=import count=... min=... p50=... p95=... p99=... max=... mean=...
```

Clock and meaning: every sample is **wall-clock time on the winit thread** around one host
call, read with `std::time::Instant` (monotonic), in microseconds. It includes blocking
waits inside the call; it is not process CPU time and not GPU execution time (no GPU
timestamp queries: that would need `TIMESTAMP_QUERY` on the device Netrender boots).

| Stage | Covers |
| --- | --- |
| `events` | draining CEF navigation/title events (`poll_web_event` until empty) |
| `import` | `acquire_frame` calls that returned a frame: mailbox take, grafting's D3D12 open/wrap, welding's 1-texel copy and CPU wait. Calls that found nothing are only counted (`empty_acquire_polls`) |
| `compose` | Netrender: `render_with_compositor_and_external_textures`, including Netrender's own submits (not separable without changing Netrender). wgpu: encoding the composition blit only |
| `compose-submit` | wgpu mode only: `queue.submit` of the composition blit |
| `acquire` | `surface.get_current_texture` (blocks on the 2-frame latency limit / vsync with `AutoVsync`); failed acquisitions are sampled too |
| `present-encode` | encoding the composed -> surface blit |
| `submit` | `queue.submit` of the surface blit |
| `present` | `pre_present_notify` + `present` |
| `frame` | whole redraw from its start to just after `present` |
| `present-interval` | between consecutive successful presents |
| `import-interval` | between consecutive imported producer frames |

`superseded_producer_frames` sums the gaps in welding's paint generation counter between
imports: paints replaced in its single-slot mailbox before the host took them (a failed
welding copy also leaves a gap, and welding logs it).

Exclusions: no sample before `load-end` (the frame that receives it included), after the
check of the last phase, or while closing. The summary is emitted and cleared before the
verification readback, and the next interval starts fresh, so readback time never enters a
sample. Surface reconfiguration (`Resized`) also breaks the intervals. The `resized` phase
deliberately includes frames that arrive at the old size until CEF catches up, since that is
what a resize costs. Samples are kept exactly, up to 65536 per stage and phase (more are
counted in `dropped_samples`), and summarised with nearest-rank percentiles; std only, no
new dependency. The aggregation is unit-tested stand-alone:
`rustc --edition 2024 --test src/timings.rs`.

Not covered: welding's D3D11 copy and event-query wait run inside CEF's `OnAcceleratedPaint`
on CEF's UI thread (welding sets `multi_threaded_message_loop` on Windows), not on the winit
thread, so they are not in `import`; Chromium's own raster/capture cost is not visible.
Numbers are to be recorded only from real runs; none are claimed here.

## Optional sustained sample (`--roves-sustained`)

Same stages, clock and summary format as above, over a steady-state window instead of the
smoke phases. The correctness smoke and its defaults are unchanged without the option.

1. The `initial` phase runs as usual (load, 20 imported frames at 1280x720, pixel check;
   timed only if `--roves-timings` is also given). The sustained part starts **only if that
   check passed** and the surface is exactly 1280x720; otherwise `FAIL sustained not
   started: ...`.
2. Warmup (`--roves-warmup-ms`, default 10 s): frames are presented but not recorded. The
   window is created non-resizable and there is no resize phase.
3. Sample (`--roves-sample-ms`, default 30 s): every redraw is recorded; no readback runs.
   At the end the log has `timings phase=sustained ...` (header with `window_ms`, presented,
   imported, superseded and dropped counts, then one line per stage with its `count`) and
   `sustained result=healthy phase=Sampling span_ms=<actual sample span> target_sample_ms=...
   warmup_ms=... presented=... imported=... superseded_producer_frames=... dropped_samples=...`.
4. The probe then closes the browser and shuts CEF down as in the smoke run; exit 0 only
   with one passed check and a healthy sample.

A sample is **rejected** (`sustained result=unhealthy`, `FAIL sustained sample rejected:
<reason>`, summary labelled `phase=sustained-unhealthy` when sampling had started, exit 2) if, during warmup or sample, no producer frame is imported for more than 500 ms (the
fixture repaints every 16 ms; checked after every redraw), the window size changes (the
resize is not sampled: recording stops before it is reported), the sample has no imported
or presented frame, any stage overflowed the 65536-sample cap, or the OS closes the window
first (`FAIL no healthy sustained sample completed`). `--roves-timeout-ms` bounds only the
load and initial phase; warmup and sample are bounded by their own durations and the stall
check. The summary header still lists `excluded=loading,verification-readback,closing`;
in sustained mode warmup and resize are excluded too, as described here.

### Comparing the compositions with alternating trials

Run on the same machine, same monitor and refresh rate, plugged in, nothing else running,
the window visible and unobstructed (an occluded window changes swap-chain throttling).
Alternate the modes so that drift (thermals, background activity, driver clocks) affects
both equally, e.g. six runs `N W N W N W` or the ABBA order `N W W N N W W N`:

```pwsh
$modes = 'netrender','wgpu','wgpu','netrender','netrender','wgpu','wgpu','netrender'
$i = 0
foreach ($m in $modes) {
  $i++
  .\roves_cef_netrender_probe.exe --roves-sustained --roves-compose=$m --roves-log="sustained-$i-$m.log"
  "$i $m exit=$LASTEXITCODE"
}
```

Use only trials with exit 0 and `sustained result=healthy`; report how many were rejected
and why. Per mode, list each trial's `frame` and `present-interval` p50/p95/p99/max and
`import-interval` p95/p99, plus `superseded_producer_frames` and `presented` over
`span_ms`; compare the spread across trials of the same mode with the gap between modes
before calling a difference real (with four trials per mode, a difference smaller than the
same-mode spread is not one). Then see which stage (`compose`, `acquire`, `import`) moves.
Record adapter, driver, refresh rate and the exact command lines next to the numbers.

## Build and run

Heavy builds run in CI (`.github/workflows/cef-netrender-probe.yml`). Locally only:

```pwsh
cd support/cef-netrender-probe
cargo +1.99.1 check --locked --features dox --all-targets   # no CEF download
```

A real GPU run uses the `cef-netrender-probe-windows-x64` artifact:
`roves_cef_netrender_probe.exe --roves-log=probe.log` (Netrender composition) and
`--roves-compose=wgpu`; `--roves-timings` adds smoke-phase stage summaries and
`--roves-sustained` the steady-state comparison (alternating trials, above). Building it
locally needs MSVC, CMake and
`cargo run --locked --release --bin bundle -- <dir> --release` (downloads CEF 154).

## CI (`cef-netrender-probe.yml`, push on this directory or manual)

Rust 1.99.1, separate cache key and `CEF_PATH` (CEF 154 never mixes with the stage-1 CEF
156 download). Steps: dependency-graph gate (`cargo tree`: welding, grafting, Netrender,
Vello and the probe on wgpu 29.0.4; wgpu 30.0.2 only under `cef`), release bundle, run with
`--roves-compose=wgpu` on WARP. Gated: sandbox information, DX12 adapter, surface and
producer lines, close/shutdown/exit lines, at least one `--type=renderer` child, no child
command line with `--no-sandbox` (sampled with `Win32_Process` while running), no process
left. If frames were imported, exit 0 and 16 `PASS pixel` lines are required; if WARP
delivers no accelerated frame, a notice says so and exit 2 is required (the GPU path is
then verified only on real hardware). Netrender composition does not run in CI.

## Versions evaluated (2026-11-22)

| Component | Pin | Notes |
| --- | --- | --- |
| wgpu-weld / `welding` | git `e991684` (main, 2026-11-20, crate 0.15.0) | `default-features = false`, `wgpu-29` + `cef-runtime`. `welding/src` is unchanged since the 0.15.0 release-notes commit `5e7ce9f`; later commits change only the manifest (grafting from git main, wgpu 30.0.2) and repository tooling. Not compared byte-for-byte with the crates.io 0.15.0 package. MSRV 1.97.1; its CI toolchain 1.99.1. MPL-2.0 |
| `grafting` / `grafting-frame` | git d903cfa (0.6.1 / 0.1.1) | Pinned with `cargo update --precise` to the revision wgpu-weld's own `Cargo.lock` reviewed at e991684 (grafting main is at df803e0, 0.6.3). MPL-2.0 |
| `cef` / `cef-dll-sys` | `=154.5.0+154.0.34` (Chromium 154.0.8037.98) | sha256 `495579dc...930b` / `a52b7101...5f69`, equal to wgpu-weld's lock |
| `wgpu` | `=29.0.4` (wgpu-core/hal/types 29.0.4, naga 29.0.4) | welding's `wgpu-29` row; Netrender d2f13ca5 |
| Netrender | git `d2f13ca5` (Vello 0.9.0) | the revision Roves already pins |
| winit | `=0.30.13` | as wgpu-weld's demos |

Compatibility findings:

- **CEF 154 vs the baseline's CEF 156: not unifiable today.** welding e991684 requires
  `cef ^154.5.0`, and every `cef-dll-sys` declares `links = "cef_dll_wrapper"`, so 154 and
  156 cannot coexist in one Cargo graph; moving welding to 156 would need welding's CEF
  handler code updated (its 151 -> 154 migration recorded a handler-signature diff). Hence
  a separate workspace, a separate CEF download and the baseline untouched. Unification
  trigger: a welding release on the CEF the baseline uses (or the baseline moving to
  welding's).
- **wgpu-29 row vs Netrender d2f13ca5 (29.0.4): compatible.** welding/grafting declare
  `wgpu ^29.0.1`, Netrender `29`; one wgpu 29.0.4 serves all of them (`cargo tree -d`).
  The `wgpu-29` row has DX12 and Metal receipts from earlier releases; wgpu-weld's CEF 154
  Windows qualification (`docs/receipts/cef154_windows_20261007`, 4096 pixel samples,
  sandboxed DX12) was taken on the `wgpu-30` row, so the 29 row on CEF 154 is unverified
  until this probe runs.
- **Graph cost, not a soundness issue:** welding enables `cef`'s `accelerated_osr`
  feature, which pulls wgpu 30.0.2 (+ core/hal/naga 30) for `cef::osr_texture_import`;
  welding does not call that module (it only needs `AcceleratedPaintInfo`, which is not
  feature-gated). The wgpu 30 copy is compiled, never instantiated. Downstream cannot turn
  off a dependency's feature; the fix is upstream in welding (drop `accelerated_osr` or
  gate it per row). Not filed yet.
- **Netrender HEAD (`90ccc1f`, 2026-11-21) is not a drop-in alternative:** it is on wgpu
  30.0.2 and replaces Vello with renamed forks (`netrender-vello` >= 0.11.1,
  `netrender-cpu`, `netrender-gpu`, `netrender-sparse`). It would unify with welding's
  default row and with cef's wgpu 30, but it means re-validating the Netrender pin Roves
  uses elsewhere; not done here.

## Chromium reference (pinned `f2700724f59f`, Chromium 160.0.8162.0)

Consulted before writing code (main checkout's sparse `support/chromium-reference`,
`components/viz` and `cc`):

- `components/viz/service/frame_sinks/video_capture/frame_sink_video_capturer_impl.cc`
  (pixel-format note at line ~86, `GetVideoFramePoolForFormat`): with
  `kPreferSharedImageWithNativeHandle` + ARGB the capturer requests **BGRA** shared images;
  `kFramePoolCapacity = kDesignLimitMaxFrames + 1 = 11` frames in flight
  (`frame_sink_video_capturer_impl.h:185`), and the minimum capture period is clamped to
  `media::limits::kMaxFramesPerSecond`. Hence: expect `Bgra8Unorm`, and copy/release each
  CEF frame promptly (welding copies inside the callback) so the producer's pool is never
  starved.
- `components/viz/common/frame_sinks/copy_output_result.h:39`: Viz copy results are
  "N32Premul" (premultiplied); RGBA texture results are `kRGBA_8888` shared images.
  Hence `SourceAlpha::Premultiplied` and the premultiplied CEF texel expectations.
- `components/viz/service/display/direct_renderer.cc:250-270`: transparency of the root
  render pass is an explicit, costlier mode (`has_transparent_background`); a transparent
  page background is a deliberate request (CEF `background_color = 0`), not the default.

Scheduling and timing reference, consulted for `--roves-timings` (same revision):

- `components/viz/service/display/display_scheduler.cc` (`DisplayScheduler::DrawAndSwap`
  ~322, `AttemptDrawAndSwap` ~801, "Swap throttled" instant ~717): Viz draws on a
  BeginFrame deadline and throttles on pending swaps. The probe has no BeginFrame source: it
  redraws continuously (`ControlFlow::Poll`) and is throttled only by the swap chain
  (`desired_maximum_frame_latency = 2`, `AutoVsync`), which surfaces as time in `acquire`.
  That is why `acquire` is its own stage and `frame` is reported next to `present-interval`.
- `components/viz/service/display/display.cc` (`Display::DrawAndSwap` ~751,
  `base::ElapsedTimer aggregate_timer` ~829, `draw_timer` ~935, `DidReceiveSwapBuffersAck`
  ~1162 with `Swap`/`WaitForPresentation` instants from `gfx::SwapTimings`): Chromium times
  the CPU-side stages (aggregate, draw) with monotonic `base::TimeTicks` in microseconds and
  takes swap start/end from the GPU side separately. The probe mirrors the CPU-side split
  (`import` ~ surface aggregation of the producer frame, `compose` ~ draw, `submit` +
  `present` ~ swap issue) with `std::time::Instant` in microseconds, and does not claim the
  GPU side: there is no equivalent of `SwapTimings` or presentation feedback here.
- `cc/metrics/compositor_frame_reporter.h:127` (`VizBreakdown`: submit-to-receive,
  received-to-start-draw, start-draw-to-swap-start, swap-start-to-swap-end,
  swap-end-to-presentation): stage names follow that order. Chromium aggregates in bucketed
  UMA histograms (e.g. `Compositing.SurfaceAggregator.AggregateUs`) and per-frame Perfetto
  trace events; the probe keeps exact bounded samples and prints percentiles in its log
  instead (no trace or histogram dependency for a one-shot comparison).

Not verified here: CEF's own OSR glue (`CefVideoConsumerOSR`, which drives Viz's
`FrameSinkVideoCapturer` for `OnAcceleratedPaint`) is not in the Chromium checkout; the
claim that OSR frames come from that capturer is CEF's documented design, not something
this probe read. CEF 154 embeds Chromium 154.0.8037.98, older than the reference; the
behaviours above are long-standing.

Differences from Chromium's own model: Chromium presents through its own compositor and
DirectComposition; here the frame leaves Chromium as a captured texture and is composed by
an external renderer, so every frame pays the capture copy inside Chromium, welding's GPU
copy and two CPU waits (below). That is the cost of keeping Chromium unmodified.

## Limitations and open questions

- Verified on one real GPU (GTX 1050) only; WARP delivers no accelerated OSR frame
  (renderers get `--disable-gpu-compositing`), so CI cannot exercise the import path.
- Per frame welding creates a D3D11 texture and an NT handle, the host opens it on D3D12,
  and the CPU waits twice (D3D11 event query in CEF's paint callback on CEF's UI thread; the
  1-texel D3D12 submission in `acquire_frame` on the host thread). GPU-only data movement,
  but synchronous. `--roves-timings` measures the host-thread part; real-scene frame pacing
  remains with the performance flow.
- Input, IME, focus, cursor, audio, popups (`<select>`), HiDPI beyond logging the page's
  `devicePixelRatio`, DevTools and navigation policy are not exercised.
- A `GPUDevice`/context loss inside CEF, or `device.destroy()` on the host, is not handled
  beyond logging uncaptured wgpu errors as `FAIL`.
- Netrender composition only runs on real GPUs (Vello 0.9 crashes on WARP, see the
  shared-device proof). Netrender's tail re-render ordering issue is avoided by placing
  the external texture last, not fixed.
- Windows x64 only; no code signing; the bundle DLL and bootstrap.exe must stay unsigned
  or be signed together (CEF bootstrap rule).
