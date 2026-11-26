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
  files `welding/src/cpu_paint.rs` and `welding/src/navigation.rs` carry the MPL-2.0 notice,
  so the changes are available under the same license.

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

### 2. Main-frame navigation veto (`OnBeforeBrowse`)

Problem: welding implements no `CefRequestHandler::OnBeforeBrowse`, so a host cannot stop a
page from navigating its main frame away. Roves runs one game document on its loopback origin;
a plain `<a href="https://...">` click or a `location.href` assignment committed off the
origin, the player could only notice it afterwards (`AddressChanged`) and close the game, and
the off-origin request had already been sent. External links therefore could not be handed to
the system browser either.

Change:

- `welding/src/navigation.rs` (new, MPL-2.0 notice): `MainFrameNavigationPolicy`
  (`Arc<dyn Fn(&str) -> bool + Send + Sync>`, re-exported from `lib.rs`) and
  `NavigationGuard`. `before_browse` is the shared body of the three backends'
  `on_before_browse`: with a policy, a main-frame navigation whose URL the policy refuses is
  cancelled (return 1) and reported; sub-frame navigations and hosts without a policy are
  untouched. The guard remembers the last vetoed URL so the load handler drops the
  `ERR_ABORTED` (-3) `OnLoadError` that Chromium may raise for the cancelled navigation (only
  that code and URL, once); any other load error is still reported.
- `surface.rs`: `CefSurfaceConfig::main_frame_navigation_policy` (default `None`, i.e.
  upstream behaviour) and `NavigationEvent::NavigationBlocked { url, user_gesture,
  is_redirect }` (CEF's flags), pushed through the same event queue as `NewWindowRequested`.
- `{windows,macos,linux}_cef/{mod.rs,cef_backed.rs}`: the guard is built from the config,
  shared by the request handler (`on_before_browse`) and the load handler (`on_load_error`).

The policy runs synchronously on CEF's UI thread inside the callback (documented: cheap,
non-blocking, no calls back into the producer). It is asked for every main-frame navigation
CEF reports to `OnBeforeBrowse`, including the initial URL, host `navigate_to_url` calls,
reloads and server redirects, so a host policy must allow its own bootstrap and start URLs.
Roves' policy and what it does with the events live in the players
(`support/cef-netrender-probe/src/external_link.rs`).

References: CEF 154 `cef_request_handler_t::on_before_browse` ("Called on the UI thread
before browser navigation. Return true (1) to cancel the navigation or false (0) to allow the
navigation to proceed"; `user_gesture`, `is_redirect`), as generated in `cef` 154.5.0
(`ImplRequestHandler::on_before_browse`).

Regression tests: `welding/src/navigation.rs` unit tests (no policy never vetoes; the policy
decides; the abort echo is consumed once and only for `ERR_ABORTED` on the vetoed URL), run
with `cargo test --no-default-features --features wgpu-29,cpu-paint-fallback --lib` (62
pass). End to end: the macOS and Linux player-fixture CI steps (a click-initiated off-origin
`location.href` must be vetoed and reported, the document must stay, a single load-end).

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

**Issue — "No way to veto main-frame navigations (OnBeforeBrowse)"**

> `welding` implements no `CefRequestHandler::OnBeforeBrowse`. A host that must keep its
> surface on one origin (an embedded app or game) can only observe an off-origin commit
> after the fact (`AddressChanged`), when the request has already been sent and the document
> replaced, and cannot route the link to the system browser instead. Expected: an optional
> host policy consulted for main-frame navigations, with refused ones cancelled and
> reported like `NewWindowRequested`.

**Pull request — "Optional main-frame navigation policy via OnBeforeBrowse"**

> Adds `CefSurfaceConfig::main_frame_navigation_policy: Option<Arc<dyn Fn(&str) -> bool +
> Send + Sync>>` (default `None`, no behaviour change) and
> `NavigationEvent::NavigationBlocked { url, user_gesture, is_redirect }`. All three
> backends implement `on_before_browse`: for a main-frame navigation the policy refuses, the
> navigation is cancelled and the event pushed. The `ERR_ABORTED` load error CEF may report
> for the cancelled navigation is dropped (only for the vetoed URL, once). Unit tests cover
> the guard; verified downstream on macOS and Linux CI. Fixes #<issue>.

## Removal criteria

Drop this directory and return the three players to a git/crates.io `welding` dependency
when an upstream release implements both the `OnPaint` fallback (equivalent behaviour:
`prefer_accelerated = false` gives software frames through `acquire_frame`), with the WARP and
lavapipe CI gates passing against it, and a main-frame navigation veto equivalent to change 2
(the players' `main_frame_policy` adapted to its API), with the macOS/Linux player-fixture
external-link checks passing. Until then re-check upstream at each welding bump; any
other welding update must be merged into this copy by hand and recorded here.
