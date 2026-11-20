() => {
  // Cross-engine typewriter observer for the unmodified visual-novel-template
  // (MarkdownTypewriter + Motion as built). Same code in both engines: Roves
  // runs `(<this file>)()` through the dev-mcp evaluate_script tool, with
  // comments, blank lines and indentation stripped by probe.mjs to fit its
  // 16 KiB limit; Chrome DevTools (MCP) runs this file with `sourcePath`.
  // Keep comments on their own lines and strings on one line.
  // Every evaluation is idempotent and returns status(), whose `ready` field
  // is the same Game.testing readiness the dev-mcp playthrough waits for. It
  // only reads the DOM, computed style and Game.testing state, except for the
  // scripted actions below (start, next with an armed cancel). See README.
  if (globalThis.__rovesTw) return globalThis.__rovesTw.status();
  const SCHEMA = 2;
  const now = () => performance.now();
  const origin = now();
  const rel = (t) => (t == null ? null : Math.round(t - origin));
  const norm = (s) => String(s ?? "").replace(/\s+/g, "");
  const QUARTILES = [25, 50, 75, 100];
  const MAX_NOTES = 8;
  const MAX_FLOW = 24;
  const lines = [];
  const notes = [];
  const flow = [];
  const note = (what, error) => {
    if (notes.length < MAX_NOTES) notes.push(what + ": " + String(error?.message ?? error).slice(0, 200));
  };
  let current = null;
  let lastT = null;
  let hiddenSeen = false;
  let cancelArm = null;
  let env = null;
  let readySnapshot = null;
  let candidate = null;
  let lastDom = null;
  let lastSig = null;
  let stopped = false;
  const cost = { frames: 0, ms_max: 0, ms_sum: 0, dom_changes: 0 };
  const visibility = () => (typeof document.visibilityState === "string" ? document.visibilityState : null);
  const path = () => location.pathname + location.hash;

  // Same condition as dev-mcp/playthrough.mjs ("Game.testing bridge with
  // live props and actions"); keep the two in sync.
  function readiness() {
    const p = window.pixiVN;
    if (!p) return { ready: false, deno: typeof globalThis.Deno };
    const props = p.props ?? {};
    const labels = p.getLabelIds();
    return {
      ready: typeof p.navigate === "function" && typeof props.navigate === "function" &&
        typeof props.t === "function" && typeof props.toast === "function" && labels.includes("start"),
      deno: typeof globalThis.Deno,
      props: Object.keys(props).sort(),
      labels: labels.length,
      path: path(),
      boot_errors: (p.errors ?? []).map((entry) => String(entry.error?.stack ?? entry.error).slice(0, 300)),
    };
  }
  // Same fields as the playthrough's STATE, read only at events (never per frame).
  function game() {
    const p = window.pixiVN;
    if (typeof p?.getState !== "function") return { bridge: false, path: path() };
    try {
      const s = p.getState();
      const d = s.dialogue;
      const c = d?.character;
      const errors = Array.isArray(p.errors) ? p.errors : [];
      return {
        text: d == null ? null : (Array.isArray(d.text) ? d.text.join(" ") : String(d.text ?? "")).slice(0, 300),
        character: typeof c === "string" ? c : c?.id ?? null,
        label: s.currentLabelId ?? null,
        step: s.stepCounter ?? null,
        canContinue: s.canContinue ?? null,
        path: path(),
        error_count: errors.length,
        errors: errors.slice(0, 2).map((entry) => String(entry.error?.stack ?? entry.error).slice(0, 300)),
      };
    } catch (error) {
      return { path: path(), error: String(error?.message ?? error).slice(0, 200) };
    }
  }
  function mark(what, extra = {}) {
    if (flow.length < MAX_FLOW) flow.push({ what, t_ms: rel(now()), ...extra, game: game() });
  }

  // Character spans of the animated MarkdownTypewriter: leaf <span>s with
  // non-whitespace text that are not Motion's aria-hidden phantoms (the
  // leading " " spacer and any trailing space-only span are excluded by
  // their text). `realLen` is the animated child's non-whitespace text minus
  // the phantoms: 0 means the animated child holds no real text.
  // children.length, not childElementCount (missing in the Roves probe DOM).
  function inspect(animated) {
    const chars = [];
    let hidden = 0;
    for (const n of Array.from(animated.querySelectorAll("span"))) {
      if (n.getAttribute("aria-hidden") === "true") {
        if (n.parentElement?.getAttribute("aria-hidden") !== "true") hidden += norm(n.textContent).length;
        continue;
      }
      if (n.children.length === 0 && norm(n.textContent) !== "") chars.push(n);
    }
    const text = norm(animated.textContent);
    return { chars, text, realLen: Math.max(0, text.length - hidden) };
  }
  // Effective opacity: product of computed opacity from the span up to the
  // animated container (group opacity of Motion wrappers included).
  function effective(n, stop, cache) {
    let value = 1;
    for (let e = n; e; e = e.parentElement) {
      let o = cache.get(e);
      if (o === undefined) {
        o = parseFloat(getComputedStyle(e).opacity);
        if (!Number.isFinite(o)) o = null;
        cache.set(e, o);
      }
      if (o === null) return null;
      value *= o;
      if (e === stop) break;
    }
    return value;
  }
  function newLine(kind, text, textNorm, n, t, dom) {
    current = {
      index: lines.length, path: kind, text: text.slice(0, 60), text_norm: textNorm, n, t_mount: t,
      t_first_partial: null, t_first_visible: null, q: {}, char_visible: [], visible: 0, partial: 0,
      inline_opacity_max: 0, unreadable_opacity: 0, frames: 0, max_frame_gap_ms: 0,
      last_progress: t, max_progress_gap_ms: 0, t_finalized: null, visible_before_final: null,
      game_mount: game(), game_final: null, dom_mount: dom, dom_final: null, static_mismatch: null,
      cancel: null,
    };
    lines.push(current);
    return current;
  }
  function finalize(line, t) {
    line.t_finalized = t;
    line.visible_before_final = line.path === "stagger" ? line.visible : null;
    line.game_final = game();
    line.dom_final = lastDom;
  }
  function maybeCancel(line, t) {
    if (!cancelArm || cancelArm.fired || line.index !== cancelArm.after) return;
    if (line.visible < cancelArm.min_visible) return;
    cancelArm.fired = true;
    const target = document.activeElement ?? document.body;
    const init = { key: "Enter", code: "Enter", bubbles: true, cancelable: true };
    const record = { t_ms: Math.round(t - line.t_mount), visible: line.visible, step: game().step ?? null,
      target: target?.tagName ?? null, keydown_not_cancelled: null, keyup_not_cancelled: null };
    try {
      record.keydown_not_cancelled = target.dispatchEvent(new KeyboardEvent("keydown", init));
      record.keyup_not_cancelled = target.dispatchEvent(new KeyboardEvent("keyup", init));
    } catch (error) {
      note("cancel", error);
    }
    line.cancel = record;
  }
  function typing(t, animated, chars) {
    const raw = chars.map((n) => n.textContent).join("");
    const textNorm = norm(raw);
    // Chunks of the same line may mount later: a longer text with the same
    // prefix is the same line.
    if (!current || current.t_finalized !== null || current.path !== "stagger" ||
        !(textNorm.startsWith(current.text_norm) || current.text_norm.startsWith(textNorm))) {
      newLine("stagger", raw, textNorm, chars.length, t, lastDom);
    } else if (textNorm.length > current.text_norm.length) {
      current.text = raw.slice(0, 60);
      current.text_norm = textNorm;
      current.n = chars.length;
    }
    const line = current;
    const cache = new Map();
    let visible = 0;
    let partial = 0;
    let inline = 0;
    chars.forEach((n, i) => {
      if (line.char_visible[i] != null) { visible++; return; }
      if (n.style.opacity !== "") inline++;
      const o = effective(n, animated, cache);
      if (o === null) { line.unreadable_opacity++; return; }
      if (o >= 0.99) {
        line.char_visible[i] = t - line.t_mount;
        visible++;
      } else if (o > 0.01) partial++;
    });
    line.inline_opacity_max = Math.max(line.inline_opacity_max, inline);
    if (partial > 0 && line.t_first_partial === null) line.t_first_partial = t;
    if (visible > 0 && line.t_first_visible === null) line.t_first_visible = t;
    if (visible > line.visible) {
      line.max_progress_gap_ms = Math.max(line.max_progress_gap_ms, t - line.last_progress);
      line.last_progress = t;
    }
    line.visible = visible;
    line.partial = partial;
    for (const q of QUARTILES) {
      if (line.q[q] == null && visible >= Math.ceil((q / 100) * line.n)) line.q[q] = t;
    }
    maybeCancel(line, t);
  }
  function sample(t) {
    if (visibility() !== null && visibility() !== "visible") hiddenSeen = true;
    if (current && current.t_finalized === null && lastT !== null) {
      current.frames++;
      current.max_frame_gap_ms = Math.max(current.max_frame_gap_ms, t - lastT);
    }
    lastT = t;
    const p = document.querySelector("p.prose");
    if (!p) {
      if (lastSig !== "none") { lastSig = "none"; lastDom = { prose: false, t_ms: rel(t) }; cost.dom_changes++; }
      candidate = null;
      return;
    }
    const kids = p.children;
    const staticEl = kids.length > 0 ? kids[0] : null;
    const animated = kids.length > 1 ? kids[1] : null;
    const staticNorm = staticEl ? norm(staticEl.textContent) : "";
    const a = animated ? inspect(animated) : { chars: [], text: "", realLen: 0 };
    // DOM evidence is rebuilt only when its shape changes (bounded cost).
    const sig = `${kids.length}|${staticEl?.children.length}|${staticNorm.length}|` +
      `${animated?.children.length}|${a.text.length}|${a.realLen}|${a.chars.length}`;
    if (sig !== lastSig) {
      lastSig = sig;
      cost.dom_changes++;
      lastDom = {
        prose: true, t_ms: rel(t), p_children: kids.length,
        static_children: staticEl ? staticEl.children.length : null,
        animated_children: animated ? animated.children.length : null,
        static_text: staticNorm.slice(0, 80), static_len: staticNorm.length,
        animated_text: a.text.slice(0, 80), animated_real_len: a.realLen, chars: a.chars.length,
      };
    }
    if (a.chars.length > 0) { candidate = null; typing(t, animated, a.chars); return; }
    // Real animated text without character spans: no line decision yet; the
    // shape stays in `dom_last` and in the pending line's evidence.
    if (a.realLen > 0) { candidate = null; return; }
    if (current && current.t_finalized === null) {
      // The template's finalizeDialogue/forceComplete result: the animated
      // child holds no real text and the static child holds exactly the line.
      if (staticNorm === current.text_norm) finalize(current, t);
      else if (staticNorm && !current.static_mismatch) {
        current.static_mismatch = { t_ms: Math.round(t - current.t_mount), dom: lastDom };
      }
      return;
    }
    // No pending line: a whole static text not recorded yet is a line that
    // was never observed typing. Confirmed on a second consecutive frame.
    const previous = lines.at(-1);
    if (!staticNorm || (previous && previous.text_norm === staticNorm)) { candidate = null; return; }
    if (candidate?.text_norm !== staticNorm) { candidate = { text_norm: staticNorm, t, dom: lastDom }; return; }
    const line = newLine("static_on_first_observation", staticNorm, staticNorm, null, candidate.t, candidate.dom);
    finalize(line, t);
    candidate = null;
  }
  function frame() {
    if (stopped) return;
    requestAnimationFrame(frame);
    const t = now();
    try { sample(t); } catch (error) { note("frame", error); }
    const spent = now() - t;
    cost.frames++;
    cost.ms_max = Math.max(cost.ms_max, spent);
    cost.ms_sum += spent;
  }
  requestAnimationFrame(frame);

  const pct = (sorted, p) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)] : null);
  function outcome(line) {
    if (line.path === "static_on_first_observation") return "missed_no_stagger";
    if (line.t_finalized === null) return "pending";
    const [a, b] = [line.game_mount?.step, line.game_final?.step];
    if (a != null && b != null && a !== b) return "advanced";
    if (line.visible_before_final === line.n) return line.cancel ? "completed_before_cancel_effect" : "completed";
    return line.cancel ? "cancelled" : "finalized_early";
  }
  const brief = (g) => (g ? { label: g.label ?? null, step: g.step ?? null, path: g.path ?? null,
    text: g.text == null ? null : g.text.slice(0, 80), error_count: g.error_count ?? null, error: g.error } : null);
  function publicLine(line) {
    const sinceMount = (t) => (t == null ? null : Math.round(t - line.t_mount));
    const times = line.char_visible.map((v) => (v == null ? null : Math.round(v)));
    const seen = times.filter((v) => v != null);
    const gaps = [];
    for (let i = 1; i < seen.length; i++) gaps.push(seen[i] - seen[i - 1]);
    const sorted = gaps.filter((g) => g >= 0).sort((a, b) => a - b);
    return {
      index: line.index, path: line.path, text: line.text, text_norm: line.text_norm.slice(0, 300),
      text_norm_len: line.text_norm.length, n: line.n, outcome: outcome(line),
      t_mount_ms: rel(line.t_mount), first_partial_ms: sinceMount(line.t_first_partial),
      first_visible_ms: sinceMount(line.t_first_visible),
      quartiles_ms: Object.fromEntries(QUARTILES.map((q) => [q, sinceMount(line.q[q])])),
      finalized_ms: sinceMount(line.t_finalized), visible_before_final: line.visible_before_final,
      in_order: gaps.every((g) => g >= 0), char_visible_ms: times,
      char_gap_ms: { p50: pct(sorted, 50), p95: pct(sorted, 95), max: sorted.at(-1) ?? null },
      max_progress_gap_ms: Math.round(line.max_progress_gap_ms), frames: line.frames,
      max_frame_gap_ms: Math.round(line.max_frame_gap_ms), inline_opacity_max: line.inline_opacity_max,
      unreadable_opacity: line.unreadable_opacity, cancel: line.cancel,
      game_mount: brief(line.game_mount), game_final: brief(line.game_final),
      dom_mount: line.dom_mount, dom_final: line.dom_final, static_mismatch: line.static_mismatch,
    };
  }
  function publicArm() {
    if (!cancelArm) return null;
    const line = lines[cancelArm.after];
    let reason = null;
    if (!cancelArm.fired) {
      reason = !line ? "line not observed" :
        line.path !== "stagger" ? "line never observed typing (no stagger): cancel could not be exercised" :
        line.t_finalized !== null ? `line finalized with ${line.visible_before_final} of ${line.n} visible, below min_visible` :
        "line still typing below min_visible";
    }
    return { ...cancelArm, not_exercised: reason };
  }
  const api = {
    status: () => {
      let ready;
      try {
        ready = readiness();
      } catch (error) {
        ready = { ready: false, error: String(error?.message ?? error).slice(0, 200) };
      }
      if (ready.ready && !readySnapshot) {
        readySnapshot = { ...ready, t_ms: rel(now()) };
        mark("ready");
      }
      return { installed: true, schema: SCHEMA, ready, lines: lines.length,
        finalized: lines.map((l) => l.t_finalized !== null), outcomes: lines.map(outcome),
        flow: flow.map((f) => f.what), dom: lastDom, notes: notes.length, stopped };
    },
    // Same order as the playthrough: clear errors captured during boot, then
    // Game.testing start("start") (the template's start navigates to
    // /game/narration). Settlement is recorded in `flow` with the game state.
    start: () => {
      env = {
        waapi: typeof Element.prototype.animate === "function",
        viewport: [globalThis.innerWidth ?? null, globalThis.innerHeight ?? null],
        dpr: globalThis.devicePixelRatio ?? null,
        delay_setting: localStorage.getItem("typewriter_delay_millisecond"),
        font_size_setting: localStorage.getItem("text_display_font_size"),
        bundle: document.querySelector('script[type="module"]')?.getAttribute("src") ?? null,
        visibility_start: visibility(),
      };
      try { window.pixiVN.clearErrors(); } catch (error) { note("clearErrors", error); }
      mark("start_called");
      Promise.resolve().then(() => window.pixiVN.start("start")).then(
        () => mark("start_fulfilled"),
        (error) => { note("start", error); mark("start_rejected", { error: String(error?.message ?? error).slice(0, 200) }); });
      return true;
    },
    // Arms one synthetic Enter keydown+keyup for the next line only, fired
    // from the rAF loop once `minVisible` characters are visible, then continues.
    next: (minVisible = 8) => {
      cancelArm = { after: lines.length, min_visible: minVisible, fired: false };
      mark("continue_called");
      Promise.resolve().then(() => window.pixiVN.continue()).then(
        () => mark("continue_fulfilled"),
        (error) => { note("continue", error); mark("continue_rejected", { error: String(error?.message ?? error).slice(0, 200) }); });
      return true;
    },
    // Final read: stops the rAF sampling so the observer costs nothing afterwards.
    report: () => {
      stopped = true;
      return "ROVES_TYPEWRITER_RESULT " + JSON.stringify({
        schema: SCHEMA,
        env: env && { ...env, visibility_end: visibility(), hidden_seen: hiddenSeen },
        notes, ready: readySnapshot, flow, dom_last: lastDom, game_end: game(),
        observer: { frames: cost.frames, ms_max: +cost.ms_max.toFixed(2), ms_sum: +cost.ms_sum.toFixed(1),
          dom_changes: cost.dom_changes },
        cancel_arm: publicArm(), lines: lines.map(publicLine),
      });
    },
  };
  Object.defineProperty(globalThis, "__rovesTw", { value: api });
  return api.status();
}
