# Vendored wgpu-weld (`welding`): Roves changes

Temporary fork under the AGENTS.md exception "fix di librerie che bloccano i giochi": edited
in place, no generated patch files. Exact diffs come from comparing this tree with upstream at
the revision below (or from git history).

## Provenance

- Upstream: <https://github.com/merely-made/wgpu-weld>, commit
  `e991684bab23d0eaea42d418e216317173ace2a9` (main, 2026-11-20), crate `welding` 0.15.0 — the
  exact revision the three CEF players pinned as a git dependency before this fork.
- Copied from that checkout: `welding/` (`Cargo.toml`, `build.rs`, `README.md`, `src/`), the
  repository `LICENSE` (MPL-2.0), `LICENSES.md`, `NOTICE` and `rustfmt.toml`, unchanged. The
  `demo-weld-*` members, design docs, scripts and CI are not vendored.
- `Cargo.toml` here is upstream's workspace root reduced to `members = ["welding"]`; the
  `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]` values are verbatim,
  so `welding/Cargo.toml` keeps its `workspace = true` inheritance unchanged.
- Consumers: `support/cef-netrender-probe` (Windows), `support/cef-macos-gate`,
  `support/cef-linux-gate`, each with
  `welding = { path = "../vendor/wgpu-weld/welding", default-features = false, features =
  ["wgpu-29", "cef-runtime", "cpu-paint-fallback"] }`. Their `Cargo.lock` files changed only
  by dropping welding's `source = "git+…"` line; every dependency, including `grafting` (git,
  locked at `d903cfa`), resolves exactly as before. This directory has no lockfile of its own.
- License: MPL-2.0 (file-level copyleft). Modified upstream files keep their headers; the new
  file `welding/src/cpu_paint.rs` carries the MPL-2.0 notice, so the changes are available
  under the same license.

## Changes

### 1. CPU paint fallback through CEF's software OSR path (release blocker)

Problem: welding only implements `OnAcceleratedPaint`. Upstream already declares the
`cpu-paint-fallback` feature, `NativeFrame::CpuBitmap`, `CefSurfaceMode::CpuPaint`,
`CefSurfaceConfig::prefer_accelerated` and an importer arm that uploads a `CpuBitmap`, but no
producer creates a software browser or implements `OnPaint`, and `prefer_accelerated` is
ignored. On a Windows WARP adapter (CI, and machines without a GPU) and on Linux without a
DMA-BUF import device (lavapipe, NVIDIA proprietary, VMs) accelerated OSR yields no
importable frame, so a game window stays black and the Roves startup screen never releases.

Change (CEF's own mechanism, no rendering code of ours):

- `welding/src/cpu_paint.rs` (new): `SoftwarePaint` (view + popup) and `CpuPaintSurface`.
  `OnPaint(type, dirtyRects, buffer, width, height)` copies only the dirty rows of CEF's
  callback-scoped BGRA buffer into a shadow buffer (whole buffer on the first paint or a size
  change) and accumulates the union of the dirty rectangles. `acquire` uploads that union with
  `Queue::write_texture` into one persistent texture (recreated only on resize; usage
  `TEXTURE_BINDING | COPY_DST | COPY_SRC`) and returns it as an ordinary `ImportedTexture`, so
  hosts present it exactly like an imported frame. Texture format: the one the platform's
  accelerated frames use for the same bytes (`Bgra8Unorm` on Windows/macOS,
  `Bgra8UnormSrgb` on Linux).
- `windows_cef`, `linux_cef`, `macos_cef` (`mod.rs` + `cef_backed.rs`): when
  `prefer_accelerated == false` and the feature is on, the browser is created with
  `shared_texture_enabled = 0` (windowless software output); `on_paint` routes to
  `SoftwarePaint`; `surface_mode` reports `CpuPaint`; `acquire_frame` / `acquire_popup` /
  `acquire_native_frame` (a `CpuBitmap` copy) serve the software state; a hidden popup clears
  it. `windowless_frame_rate` (60) and `external_begin_frame_enabled = 0` are unchanged, so
  CEF paints at the same rate in both modes. Without the feature, or with
  `prefer_accelerated == true` (default), behaviour is byte-for-byte upstream's.
- `surface.rs`: `prefer_accelerated` documentation describes the real behaviour (the host
  decides; CEF cannot report up front whether accelerated frames will arrive).
- `native_frame/mod.rs`: the existing `upload_cpu` importer adds `COPY_SRC`, like the
  imported paths, so hosts can read frames back.
- `lib.rs`: declares the module.

Selection policy lives in the hosts (shared `game_rules::paint_path`): software when the
adapter is `DeviceType::Cpu` (WARP, lavapipe/llvmpipe), when Linux
`build_dmabuf_capable_device` fails, or when `ROVES_FORCE_CPU_PAINT=1`.

References: CEF 154 `cef_render_handler_t::on_paint` ("|buffer| will be |width|*|height|*4
bytes in size and represents a BGRA image with an upper-left origin. This function is only
called when cef_window_tInfo::shared_texture_enabled is set to false"; dirty rects in pixel
coordinates), `cef_browser_settings_t::windowless_frame_rate` (maximum `OnPaint` rate), as
generated in `cef-dll-sys` 154.5.0. Chromium (`support/chromium-reference`, `f2700724f59f`):
`components/viz/service/display_embedder/software_output_device_win.cc`
(`SoftwareOutputDeviceWinProxy`: the software compositor draws into a viewport-sized shared
bitmap and hands each frame's damage rect to the browser through
`components/viz/host/layered_window_updater_impl.cc`), the model CEF's OSR software output
follows; uploading only the damaged region mirrors that.

Regression tests: `welding/src/cpu_paint.rs` unit tests (dirty-rect clamping, row-only copies,
accumulation until taken, resize resets, null buffer ignored), run with
`cargo test --no-default-features --features wgpu-29,cpu-paint-fallback --lib` (60 pass) and
without the feature (59 pass). End to end: `cef-netrender-probe.yml` (WARP fixture with 16
pixel checks, template package startup release, host API launches) and `cef-linux-gate.yml`
(lavapipe fixture colour readback and packaged game, `evidence=software`).

## Upstream

Not opened yet (no credentials in this session); see `TODO.md`. Prepared texts:

**Issue — "CPU paint fallback is declared but no producer implements OnPaint"**

> `welding` 0.15.0 (e991684) declares `cpu-paint-fallback`, `NativeFrame::CpuBitmap`,
> `CefSurfaceMode::CpuPaint` and `CefSurfaceConfig::prefer_accelerated`, but every producer
> hard-codes `shared_texture_enabled: 1`, no render handler implements `on_paint`, and
> `prefer_accelerated` is never read. Where accelerated OSR delivers nothing importable —
> Windows WARP (`Microsoft Basic Render Driver`, also used on machines without a GPU),
> Linux without a DMA-BUF import device (lavapipe, NVIDIA proprietary, many VMs) — the host
> gets no frame at all and the surface stays black. Expected: with the feature enabled and
> `prefer_accelerated = false`, the producer creates the browser with
> `shared_texture_enabled = 0` and serves `OnPaint` BGRA frames through `acquire_frame`.
> Reproduction: run any demo on a WARP adapter (`DeviceType::Cpu`) or on lavapipe; no
> `OnAcceleratedPaint` arrives.

**Pull request — "Implement the OnPaint CPU fallback in all producers"**

> Implements the declared `cpu-paint-fallback` path. `prefer_accelerated = false` (with the
> feature) creates the browser with `shared_texture_enabled = 0`; `OnPaint` copies only the
> dirty rows of CEF's BGRA buffer into a shadow buffer, and `acquire_frame` uploads the union
> of the dirty rectangles with `Queue::write_texture` into one persistent texture (recreated
> on resize) returned as a normal `ImportedTexture`, so hosts need no new code path. Popups
> use a second surface; `surface_mode()` reports `CpuPaint`; `acquire_native_frame()` returns
> a `CpuBitmap`. The texture format matches each platform's accelerated frames. The default
> (`prefer_accelerated = true`) is unchanged. The host chooses the mode (CEF cannot report in
> advance whether accelerated frames will arrive), e.g. on `DeviceType::Cpu` or when
> `build_dmabuf_capable_device` fails. Unit tests cover dirty-rect clamping and accumulation;
> verified downstream on WARP and lavapipe CI. Fixes #<issue>.

## Removal criteria

Drop this directory and return the three players to a git/crates.io `welding` dependency
when an upstream release implements the `OnPaint` fallback (equivalent behaviour:
`prefer_accelerated = false` gives software frames through `acquire_frame`) and the WARP and
lavapipe CI gates pass against it. Until then re-check upstream at each welding bump; any
other welding update must be merged into this copy by hand and recorded here.
