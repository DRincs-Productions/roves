# CEF accelerated OSR -> wgpu-weld -> Netrender probe (Windows feasibility)

Isolated experiment, separate Cargo workspace. Nothing in the engine, `roves-action`,
Packmaster, any release or the stage-1 window probe (`support/cef-window-probe`, CEF 156,
`.github/workflows/cef-window-probe.yml`, both unchanged) depends on it.

Question: can Chromium (through CEF, its own V8 running the page) paint into a GPU texture
that a Roves-owned wgpu device composites with Netrender, without a CEF fork, without a CPU
copy on the presentation path and without giving up the Windows sandbox? Deno host services,
IPC, input, audio, `game://`, CSP and the single-document policy are out of scope here.

**Status (2026-11-22): implemented and type-checked locally (`dox`), clippy and rustfmt
clean; never built with CEF, never run.** The first CI run and a run of its bundle on a real
GPU decide the open rows below. No result is claimed before then.

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
window stays until the OS closes it). CEF switches such as `--log-file` pass through.

## Build and run

Heavy builds run in CI (`.github/workflows/cef-netrender-probe.yml`). Locally only:

```pwsh
cd support/cef-netrender-probe
cargo +1.99.1 check --locked --features dox --all-targets   # no CEF download
```

A real GPU run uses the `cef-netrender-probe-windows-x64` artifact:
`roves_cef_netrender_probe.exe --roves-log=probe.log` (Netrender composition) and
`--roves-compose=wgpu`. Building it locally needs MSVC, CMake and
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

- Never built or run (see Status). WARP may not deliver accelerated OSR frames at all.
- Per frame welding creates a D3D11 texture and an NT handle, the host opens it on D3D12,
  and the CPU waits twice (D3D11 event query in CEF's paint callback; the 1-texel D3D12
  submission in `acquire_frame`). GPU-only data movement, but synchronous; frame pacing
  and cost are unmeasured and belong to the performance flow, not to this probe.
- Input, IME, focus, cursor, audio, popups (`<select>`), HiDPI beyond logging the page's
  `devicePixelRatio`, DevTools and navigation policy are not exercised.
- A `GPUDevice`/context loss inside CEF, or `device.destroy()` on the host, is not handled
  beyond logging uncaptured wgpu errors as `FAIL`.
- Netrender composition only runs on real GPUs (Vello 0.9 crashes on WARP, see the
  shared-device proof). Netrender's tail re-render ordering issue is avoided by placing
  the external texture last, not fixed.
- Windows x64 only; no code signing; the bundle DLL and bootstrap.exe must stay unsigned
  or be signed together (CEF bootstrap rule).
