# Roves vs Chrome template typewriter probe

Reproducible observation of the real visual-novel-template typewriter (MarkdownTypewriter
plus Motion, exactly as the template builds them) in the Roves Blitz probe and in Chrome.
Companion to `../visual-matrix/` (deterministic pixels and computed state of synthetic
boxes) and `../fixture-matrix/` (rAF cadence of synthetic scenes): this one uses the
template itself, so its timings are observations, not a deterministic fixture. No build,
no engine change, no new dependency, no template change.

## Files

- `observer.js`: one arrow function, the same code in both engines. Chrome evaluates the
  file itself (`sourcePath`); `probe.mjs roves` sends it to the dev-mcp `evaluate_script`
  (16 KiB limit) without full-line comments, blank lines and indentation, newlines kept,
  and asserts the size and that no trailing/block comment or multi-line template literal
  remains before launching Roves. Keep comments on their own lines. Every evaluation is
  idempotent: the first installs it, each one returns `status()`. It reads the DOM and
  computed style from a single rAF loop and exposes `globalThis.__rovesTw` with `status()`,
  `start()`, `next(minVisible)` and `report()` (a `ROVES_TYPEWRITER_RESULT {json}` string,
  schema 2; `report()` also stops the rAF sampling). `status().ready` is the readiness of
  the dev-mcp playthrough (`../../blitz-v8-probe/dev-mcp/playthrough.mjs`): `pixiVN.navigate`,
  `props.navigate`/`t`/`toast` functions, label `start`, plus `typeof Deno` and boot errors.
  Keep the two conditions in sync.
- `probe.mjs` (Node >= 20, built-ins only):
  - `roves`: drives the existing dev-mcp server (`../../blitz-v8-probe/dev-mcp/server.mjs`,
    `evaluate_script` only) through the steps below and writes the result line.
  - `serve`: serves the same `dist` read-only on 127.0.0.1 for the Chrome leg (History API
    routes fall back to `index.html`, `no-store`). Needed because the template uses
    root-absolute module URLs that `file://` cannot load.
  - `compare`: prints both records side by side and checks the shared contract.

## Template input

Template commit `e88e9ac`. The template checkout and its committed source are never
edited: like the CI playthrough (`../../blitz-v8-probe/dev-mcp/README.md`, "Template
playthrough"), build a disposable copy where `template-testing-optin.mjs` enables
Pixi'VN's `Game.testing` bridge (`window.pixiVN`), which is how both engines start and
advance the story. Do not reuse `D:/roves-ci/typewriter-e88-diagnostic/dist`: its
`index.html` carries an injected rAF reader from the earlier diagnosis. Both engines must
load the same `dist`; the record's `env.bundle` (entry module URL) proves it.

## What is recorded (per dialogue line)

Characters (glyphs) are the leaf `<span>`s of the animated MarkdownTypewriter (`p.prose`
second child) with non-whitespace text that are not Motion's `aria-hidden` phantoms: the
`" "` spacer and any space-only leaf left after completion are not characters (Chrome e88
keeps one in the animated child after line 0 completes, which made the schema-1 observer
wait forever). Line texts are compared whitespace-free (`text_norm`). Leaf test uses
`children.length` (`childElementCount` is missing in the probe DOM). A character is
visible when its effective computed opacity (product of `getComputedStyle().opacity` from
the span up to the animated container) is >= 0.99, partial above 0.01. Once visible a
character is not re-read. Game.testing state is read only at events (line mount and
finalization, cancel, start/continue settlement), the DOM evidence object is rebuilt only
when the `p.prose` shape changes, and sampling stops at `report()`; the whole loop cost
is reported as `observer` (frames, ms max/sum, DOM shape changes).

- Times in ms relative to the line's first rAF sample with characters (`t_mount`):
  `first_partial_ms`, `first_visible_ms`, `quartiles_ms` (25/50/75/100 % visible),
  `char_visible_ms` (per character), `finalized_ms`.
- Gaps: `char_gap_ms` (p50/p95/max between consecutive characters becoming visible),
  `max_progress_gap_ms` (longest time without new visible characters), `frames` and
  `max_frame_gap_ms` (rAF cadence while typing), `in_order`.
- Completion: the line is finalized when the animated child holds no real text (its
  non-whitespace text minus `aria-hidden` phantoms is empty) and the static
  MarkdownTypewriter's whitespace-free text equals the line's (the template's
  `finalizeDialogue`/`forceComplete` path). `outcome`: `completed` (all characters were
  visible first), `cancelled` (finalized before all were visible after the armed cancel),
  `completed_before_cancel_effect`, `finalized_early`, `advanced` (story step changed:
  the action advanced instead of completing) or `pending`. A pending line whose static
  text differs from it records the first such DOM shape in `static_mismatch`.
- Missed lines: when no line is pending and the static child holds a new whole text for
  two consecutive frames with no animated characters (including a `p.prose` with a single
  child), the line is recorded with `path: "static_on_first_observation"` and outcome
  `missed_no_stagger`, finalized at once so the run continues instead of timing out. It
  has no timings; its DOM evidence (`dom_mount`/`dom_final`: child counts of `p.prose`,
  static and animated child, their whitespace-free texts, animated real-text length,
  character count) says what was seen. Typed lines have `path: "stagger"`. `compare`
  reports a missed line as a problem and never as parity.
- Flow, aligned with the playthrough: `ready` (snapshot when readiness first holds),
  `flow` (`ready`, `start_called/_fulfilled/_rejected`, `continue_called/...`, each with
  label, step, path, dialogue text and error count), `game_mount`/`game_final` per line,
  `game_end`, and the last DOM shape `dom_last`. `start()` clears boot errors first, as
  the playthrough does. `cancel_arm.not_exercised` says why the cancel did not fire (line
  not observed, no stagger, finalized below `min_visible`); the cancel targets only the
  line right after `next()`.
- `inline_opacity_max`: characters with an inline `style.opacity` in one frame (Chrome:
  Motion animates with WAAPI, `env.waapi` true; the probe has no `Element.animate`, so
  Motion writes inline styles from its own rAF loop). This explains why the earlier
  attribute-only reader saw nothing: finished characters are re-rendered without any
  `style`, so only computed opacity works in both engines.
- `env`: WAAPI presence, viewport, DPR, the template's `typewriter_delay_millisecond` and
  font-size settings (must be `null` = defaults in both), visibility at start/end and
  `hidden_seen`.

## Steps (identical in both engines)

1. Evaluate `observer.js` (installs it once) until `status().ready.ready` is `true`; stop
   with a failure if `ready.deno` is not `"undefined"`.
2. `__rovesTw.start()` (clear errors, Game.testing `start("start")`); wait until line 0 is
   finalized (natural completion, or `missed_no_stagger`).
3. `__rovesTw.next(8)`: arms one cancel for the next line, then Game.testing `continue()`.
   When 8 characters of line 1 are visible, the rAF loop dispatches a synthetic `Enter`
   `keydown` + `keyup` on the active element. This is the template's own hotkey path
   (`useNarrationHotkeys`: `TextDisplaySettings.complete()` while typing) reached through
   an untrusted test event, the only action available identically in both engines (the
   dev-mcp server has no native input tool). Firing from the rAF loop, not from the MCP
   client, removes round-trip latency from the cancel point. Wait until line 1 is
   finalized. If line 1 is missed the cancel is not exercised and the report says so.
4. Read `__rovesTw.report()`.

Contract checked by `compare` (schema 2 only): playthrough readiness reached, no `Deno`,
no boot or game errors, `start` settled on label `start` at `/game/narration`; same
bundle, delay setting, whitespace-free line texts, line paths and game label/step/path at
each line mount and at `ready`/`start_fulfilled`/`continue_fulfilled`; line 0
`completed`, line 1 `cancelled` with the cancel exercised, characters visible in order,
opacity readable, page never hidden, no observer notes. Every difference is listed even
when the cancel could not be exercised. Timings are printed side by side only for lines
observed typing (`-` otherwise), not gated: no threshold has been accepted for them.

## Invocation

Prerequisites: a published dev-mcp probe (`roves-dev-mcp-windows` artifact, or a probe
built with `--features dev-mcp`) and `npm ci` in `support/blitz-v8-probe/dev-mcp` (or pass
`--server <bundle>/mcp/server.mjs`). PowerShell, from the repository root:

```powershell
$tpl = 'D:/roves-ci/template-e88e9ac'; $out = 'D:/roves-ci/typewriter-probe-<date>'
git clone https://github.com/DRincs-Productions/visual-novel-template $tpl
git -C $tpl checkout --detach e88e9ac
node support/blitz-v8-probe/dev-mcp/template-testing-optin.mjs $tpl
Push-Location $tpl; npm install; npm run build; Pop-Location   # as blitz-probe.yml
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem Env: | Where-Object Name -match '^(ROVES_PROBE_|ROVES_EXTRA_ARGS|DENO_WEBGPU_|WGPU_)' |
  ForEach-Object { Remove-Item "Env:$($_.Name)" }
node support/perf/typewriter-probe/probe.mjs roves '<probe.exe>' "$tpl/dist" --out "$out/roves.txt"
# JIT-less variant: $env:ROVES_PROBE_JITLESS = '1' before the line above, --out "$out/roves-jitless.txt"
node support/perf/typewriter-probe/probe.mjs serve "$tpl/dist" --port 8765   # separate terminal, keep running
```

Chrome DevTools (MCP), visible foreground page, nothing covering it:

1. `new_page` `url: "about:blank"`, `isolatedContext: "typewriter-<date>"` (fresh storage,
   no service worker or settings from earlier runs).
2. `emulate` `viewport: "640x360x1"`.
3. `navigate_page` to `http://127.0.0.1:8765/`.
4. Repeat `evaluate_script` with `sourcePath`
   `<repo>/support/perf/typewriter-probe/observer.js` until `ready.ready` is `true` (the
   first call installs the observer; the rest only return `status()`).
5. `evaluate_script` `() => globalThis.__rovesTw.start()`; repeat
   `() => globalThis.__rovesTw.status()` until `finalized[0]` is `true`.
6. `evaluate_script` `() => globalThis.__rovesTw.next(8)`; repeat `status()` until
   `finalized[1]` is `true`.
7. `evaluate_script` `() => globalThis.__rovesTw.report()` with `filePath` `<out>/chrome.txt`.
   Optionally `performance_start_trace`/`performance_stop_trace` around steps 5-6 for a
   Chrome trace of the same scene (recorded separately, not compared by `compare`).

```powershell
node support/perf/typewriter-probe/probe.mjs compare "$out/roves.txt" "$out/chrome.txt"
```

Record the Chrome version, probe artifact/commit, GPU, display scale and whether the
Chrome page was visible. A Chrome page that is hidden or occluded throttles rAF and WAAPI
timing (`hidden_seen`; see `../fixture-matrix/README.md`): record that run as not run.

## Pixels

Actual pixels are covered by the existing static gate, not by this probe: run the
inline-opacity part of `../visual-matrix/README.md` (Roves headless
`ROVES_PROBE_SCREENSHOT` plus Chrome `take_screenshot`, both checked by
`support/blitz-v8-probe/fixtures/inline-opacity/check_png.py`). It proves that inline
spans at opacity 0, intermediate and 1 paint the same in both engines, which is what the
computed-opacity milestones above rely on.

Limits, stated rather than worked around:

- No mid-typing screenshots. The template is not deterministic in time (Motion uses
  wall-clock, WAAPI in Chrome vs a JS rAF loop in the probe), the probe screenshot exists
  only in headless `--game` runs captured after the last frame, the dev-mcp window has no
  capture tool, and Chrome `take_screenshot` is not synchronised with a rAF index. A
  timeline of screenshots would compare different instants, not rendering.
- Computed opacity is the engine's style result, not the presented frame: an engine that
  computes the right value but paints it wrong is caught only by the static gate, and a
  presented-frame delay (compositor, present queue) is not seen here. Displayed frames
  need PresentMon / a Chrome trace (`docs/ROVES_PERFORMANCE.md`).
- Occlusion: Chrome stops rAF for a hidden or covered page; the probe never throttles. A
  covered Chrome window makes timings meaningless even when the run completes.
- Sampling granularity is one rAF frame; the observer runs before Motion's callbacks in
  the same frame, so a value written in frame N is seen in frame N+1 in both engines.
- The observer adds work to every frame (`observer`) in both engines, from installation
  (before the game boots) until `report()`.
- A line is `missed_no_stagger` when its whole text is static at the first frame the
  observer sees it: either the engine never ran the per-character stagger or it finished
  within one rAF frame. The probe cannot tell these apart; the DOM evidence is recorded.
- The cancel is an untrusted synthetic event; native keyboard input in the Roves window
  is covered by the `--story` harness, not here.
