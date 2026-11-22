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
expects).** Evidence below. A first sustained ABBA series (8 trials, bundle of CI run
45042438963) is recorded in "Sustained trials" below: 6 healthy, 2 Netrender trials
rejected for a 630/650 ms import stall during warmup. The stalls are not attributed yet
(a diagnostic is proposed in "Attributing the warmup stalls"). Two healthy Netrender
trials are not enough to compare the two compositions.

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

## Sustained trials (2026-11-22, bundle of CI run 45042438963)

Bundle: artifact `cef-netrender-probe-windows-x64` of
[run 45042438963](https://github.com/DRincs-Productions/roves/actions/runs/45042438963)
(workflow "CEF -> wgpu-weld -> Netrender probe", branch `codex/cef-netrender-probe`, head
`a487b763205`, conclusion success). Same versions as above (CEF 154.0.34 / Chromium
154.0.8037.98, welding 0.15.0@e991684, Netrender d2f13ca5, wgpu 29.0.4). Run locally on
NVIDIA GeForce GTX 1050, Dx12, DiscreteGpu, driver 32.0.15.8163, surface `Bgra8Unorm`
1280x720, scale 1, sandboxed. Order `N W W N N W W N` (ABBA), default warmup 10 s,
sample 30 s, stall limit 500 ms. Each trial had a fresh CEF cache directory
(`D:\roves-ci\45042438963-cef-nr\cache-<i>`, logged on the `cef initialized` line), so
Chromium's on-disk caches start cold every time. The logs carry no smoke-phase
`timings phase=initial` lines, so `--roves-timings` was not passed (sustained mode records
the sample anyway). **Not recorded:** the exact command lines and the monitor refresh rate,
both required by the procedure above. Logs (local, not committed):
`D:\roves-ci\45042438963-cef-nr\trial-<i>-<mode>.log`.

Every trial passed the initial check (8/8 `PASS pixel`, no `FAIL pixel`), closed cleanly
(`browser closed`, `cef shutdown complete`).

| # | Mode | Exit | Result | `presented` / `imported` / superseded / empty polls over `span_ms` |
| --- | --- | --- | --- | --- |
| 1 | netrender | 2 | **rejected**: `imports stalled: no producer frame for 630 ms during Warmup`, 7863 ms after warmup start | no sample (whole run: `closing imported=376 presented=438 elapsed_ms=10871`) |
| 2 | wgpu | 0 | healthy | 1724 / 1489 / 237 / 235 over 30005 |
| 3 | wgpu | 0 | healthy | 1724 / 1494 / 232 / 230 over 30005 |
| 4 | netrender | 2 | **rejected**: `imports stalled: no producer frame for 650 ms during Warmup`, 7418 ms after warmup start | no sample (whole run: `closing imported=312 presented=377 elapsed_ms=8802`) |
| 5 | netrender | 0 | healthy | 1724 / 1480 / 246 / 244 over 30005 |
| 6 | wgpu | 0 | healthy | 1724 / 1487 / 239 / 237 over 30004 |
| 7 | wgpu | 0 | healthy | 1724 / 1482 / 242 / 242 over 30005 |
| 8 | netrender | 0 | healthy | 1719 / 1435 / 281 / 284 over 30005 |

Rejects: 2 of 8, both Netrender (2 of 4 Netrender trials, 0 of 4 wgpu). In both, the
`sustained result=unhealthy` line reports `presented=0 imported=0` because those counters
are the sample's and sampling never started; warmup redraws are not recorded, so the
rejected trials have **no stage data at all**. The reported 630/650 ms is the gap since the
last import *when the check ran* (after a redraw returned), not the measured length of a
stall that started at a known time.

Healthy trials, wall-clock µs on the winit thread (nearest-rank):

| # | Mode | `frame` p50/p95/p99/max | `present-interval` p50/p95/p99/max | `import-interval` p95/p99/max | `import` p50/p95/p99/max | `compose` p50/p95/p99/max | `acquire` p50/p95/max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | wgpu | 17287/17401/17532/18106 | 17404/17520/17664/18211 | 34944/37585/53821 | 1445/4025/4706/5346 | 93/173/244/416 (+submit 56/93/122/265) | 14993/16747/16869 |
| 3 | wgpu | 17291/17394/17520/33807 | 17405/17510/17650/34010 | 34893/37746/53787 | 1452/4040/4635/5623 | 90/175/312/726 (+submit 55/94/140/277) | 14999/16761/16862 |
| 6 | wgpu | 17286/17405/17560/33195 | 17403/17522/17674/33300 | 34931/38199/54963 | 1425/4016/4863/5347 | 91/162/241/433 (+submit 54/90/135/271) | 15032/16750/31660 |
| 7 | wgpu | 17285/17418/17581/22640 | 17403/17535/17728/22893 | 34975/37576/56180 | 1447/4033/4973/8176 | 93/216/417/1009 (+submit 56/111/224/800) | 14978/16741/16847 |
| 5 | netrender | 17285/17418/17697/19331 | 17404/17544/17822/19582 | 34894/35960/52647 | 1341/2354/2773/4187 | 1006/1788/2696/10082 | 14183/15843/15959 |
| 8 | netrender | 17285/17525/20784/129297 | 17402/17670/21302/132737 | 34982/37723/367779 | 1339/2723/11815/125501 | 1023/2594/7088/18163 | 14160/15841/30476 |

What this shows, and only this:

- **Presents are not displayed frames.** `present-interval` is the time between successful
  `present()` calls on the host thread; there is no DXGI frame statistics or presentation
  feedback, so whether and when each present reached the screen is not measured. The p50 of
  about 17.40 ms (about 57.5 presents/s) is the same in every trial; without the recorded
  refresh rate it cannot be read as "every vblank" or as a missed-frame rate.
- **Presents are not new page frames either.** 83–87% of presents carried a newly imported
  CEF frame (1489/1724 in trial 2, 1435/1719 in trial 8); the rest re-presented the previous
  one. `import-interval` p50 (about 17.43 ms) is quantised by the host's own redraw cadence
  (one mailbox poll per redraw), and its p95 of about 35 ms is two redraws: the
  superseded-paint count closely tracks the empty-poll count (e.g. 237/235), i.e. two paints
  landing in one redraw interval are followed by an interval with none. imported + superseded
  (paints that reached welding's callback) is 1716–1726 per 30 s in every trial, close to
  the present count; why the paint rate sits there rather than at `windowless_frame_rate =
  60` is not explained by these logs.
- Both modes are swap-chain bound at the median (`acquire` p50 14–15 ms of a 17.3 ms
  `frame`). Netrender's `compose` (which includes its own submits) is about 1.0 ms p50 against
  about 0.15 ms for wgpu's blit plus its submit: host CPU wall time, not GPU time.
- Tails: trial 8 has a 125.5 ms `import` call, a 129 ms `frame`, a 132.7 ms present interval
  and a 368 ms import interval (under the 500 ms limit, so the sample is healthy). Trial 5
  has none of that. The `import` stage cannot be read as producer cost: welding's
  `import_owned_dx12_callback_frame` submits a 1-texel copy and waits for it with
  `device.poll(Wait)`, and on the single shared queue that waits for every earlier
  submission too, including the previous Netrender composition and present blit.
- Same-mode spread versus mode gap: wgpu `frame` p99 spans 17520–17581 µs over four
  trials; Netrender has two healthy trials (17697 and 20784). With two trials the
  procedure's rule (a gap smaller than the same-mode spread is not a difference) cannot be
  applied, so **no composition comparison is claimed**.

## Attributing the warmup stalls (proposed diagnostic, not implemented)

Goal: tell, for a rejected Netrender trial, whether the 630/650 ms gap happened because
**the producer did not deliver** (no frame reached welding's mailbox) or because **the host
thread was blocked** (frames were waiting, nobody polled). Not an optimization; nothing on
the presentation path changes.

Where the two sides meet today:

- Producer: welding e991684 `welding/src/windows_cef/cef_backed.rs:60-135`
  (`on_accelerated_paint`, on CEF's UI thread): `DuplicateHandle`, generation assigned at
  line 110 *before* the copy, `copy_dx12_callback_frame_to_owned` (D3D11 `CopyResource` +
  event-query wait), then the single slot `frame_slot` (line 131) is overwritten. A failed
  copy only calls `log::error!`, and the probe installs no `log` logger, so it is invisible
  and would look exactly like "no paint".
- Hand-off: `welding/src/windows_cef/mod.rs:468-478` (`acquire_frame`: take the slot, then
  import); `welding/src/native_frame/dx12.rs:241-290` (grafting open, 1-texel copy,
  `device.poll(Wait)` on the shared queue).
- Host: `support/cef-netrender-probe/src/app.rs` `redraw()` (lines 488-649: events, one
  `acquire_frame` per redraw, compose, surface acquire, submit, present),
  `sustained_step()` (718-745: the stall check, run once after each redraw returns),
  `start_warmup()` (689-715), and the `record` match at 494-498, which records nothing
  during warmup. That is why both rejected logs hold no stage data.
- Chromium (pinned `f2700724f59f`,
  `components/viz/service/frame_sinks/video_capture/frame_sink_video_capturer_impl.cc`):
  capture is damage-driven (`OnFrameDamaged`, ~721). A frame is not captured when the
  oracle rate-limits (`FpsRateLimited`, ~842-853, then `MaybeScheduleRefreshFrame`, refresh
  timer ~630) or when the 11-frame pool is exhausted (`PipelineLimited`, ~943-958; pool
  slots come back only when the consumer releases the frame, i.e. after welding's callback
  returns). Each capture is a `Capture`/`FrameInUse` trace span (~1003/~1031, ended in
  `MaybeDeliverFrame` ~1548-1595), all in trace category `gpu.capture`. CEF's own OSR glue
  (`CefVideoConsumerOSR`) is not in the checkout. CEF 154 also passes `timestamp` and
  `capture_counter` in `cef_accelerated_paint_info_common_t` (cef-dll-sys 154.5.0
  bindings), but welding drops both, so the host cannot see Viz-level frame numbers without
  a welding change.

Smallest diagnostic (host-only, no welding fork, sustained mode only):

1. **Redraw ring during warmup and sample.** A fixed array of the last 128 redraws
   (allocated once, no allocation per frame), each with: start offset from warmup start,
   total redraw duration, the durations of `events`, the `acquire_frame` call, `compose`,
   surface acquire, submit and present, and the poll outcome (`empty` or the imported
   generation). That is the existing `Instant` laps, kept even when `record` is false. It is
   printed only when a sample is rejected for a stall, as `stall-trace` lines, so healthy
   output stays unchanged.
2. **One mailbox re-poll at the stall.** In `sustained_step()`, before rejecting, call
   `acquire_frame` once more (not recorded) and log
   `stall-mailbox frame generation=<g> delta=<g - last imported generation>` or
   `stall-mailbox empty`.
3. **Forward `log` warnings/errors to the probe log** (a minimal `log::Log` writing to the
   existing log; the `log` crate is already in the graph through welding), so a welding
   copy failure shows up as a line instead of as a missing frame.

Reading it:

| Observation in the rejected trial | Attribution |
| --- | --- |
| One redraw (or a few) covers most of the gap, with the blocking stage named; `stall-mailbox` has a frame with `delta > 1` | host blocking (paints arrived and were superseded while the thread was busy) |
| Redraws keep their normal ~17 ms cadence through the gap, about gap/17 consecutive `poll=empty`, `stall-mailbox empty`, no welding error | producer delivery (nothing was published by Chromium/welding's callback) |
| A welding `failed to copy CEF accelerated frame` line inside the gap | producer side, welding copy failure (not Chromium) |

Limit: this attributes *which side did not move*, not *which GPU workload caused it*.
welding's D3D11 copy and its event-query wait run on the same GPU as Netrender's D3D12
work, so a producer-side gap can still be caused by host GPU load. If the producer side is
indicated, the next discriminator is Chromium's `gpu.capture` trace (`FpsRateLimited`,
`PipelineLimited`, `Capture` spans) via CEF startup-tracing switches (not yet verified to
work in this CEF build), or welding exposing callback timing and `capture_counter` (an
upstream request, not filed).

Reproducible next test (after the diagnostic is built by `cef-netrender-probe.yml`): same
GTX 1050, monitor and refresh rate (record it), the same ABBA order with a fresh cache
directory per trial, and the exact command lines saved next to the logs:

```pwsh
$dir = 'D:\roves-ci\<new run id>-cef-nr'
$exe = "$dir\bundle\roves_cef_netrender_probe.exe"
$modes = 'netrender','wgpu','wgpu','netrender','netrender','wgpu','wgpu','netrender'
$i = 0
foreach ($m in $modes) {
  $i++
  $cmd = "--roves-sustained --roves-compose=$m --roves-cache-dir=$dir\cache-$i --roves-log=$dir\trial-$i-$m.log"
  Add-Content "$dir\commands.txt" "$i $exe $cmd"
  & $exe --roves-sustained --roves-compose=$m "--roves-cache-dir=$dir\cache-$i" "--roves-log=$dir\trial-$i-$m.log"
  Add-Content "$dir\commands.txt" "$i $m exit=$LASTEXITCODE"
}
```

Pass criterion for the diagnostic (not for performance): every rejected trial has
`stall-trace` and `stall-mailbox` lines and falls into one row of the table above. If
no trial is rejected, report that (2/4 Netrender rejects were seen once) and do not infer
anything about the cause.

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
