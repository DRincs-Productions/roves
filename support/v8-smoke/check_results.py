#!/usr/bin/env python3
"""Validate the smoke harness protocol; no browser or third-party packages required."""

import argparse
import json
import math
from collections import Counter
from pathlib import Path
import re

# Explicit exceptions for runners without physical output devices. A hung GPU request
# is not an unavailable device: the page must report that as a failure.
HARDWARE_SKIPS = {
    ("games.html", "AudioContext decodes and plays a buffer"),
    ("graphics.html", "WebGPU adapter request (when available)"),
}
WASM_SKIPS = {
    ("suite2.html", "WebAssembly.instantiate (async compile)"),
    ("suite2.html", "WebAssembly.compileStreaming (fetched)"),
    ("games.html", "module Worker instantiates WebAssembly"),
}
# Checks a page must report: a page that drops one still reports a consistent total.
REQUIRED_CHECKS = {
    "module-csp.html": ("classic script nonce authorizes its dynamic module fetch", "an unnonced classic script remains blocked"),
    "document-interface-alias.html": ("fresh interface resolution preserves the Document alias",),
    "module-order.html": ('initial static module loading reports parser and timer observations', 'deferred modules see the parsed body and TLA does not block DOMContentLoaded', 'static evaluation errors are rejection events and the game continues', 'inline module URLs are distinct and preserve the relative asset base', 'classic errors still use error events', 'static parse failure preserves its syntax error contract', 'static link failure preserves its syntax error contract', 'static fetch failure remains a script loading failure', 'dynamic fetch failure rejects the importing promise', 'classic eval and Function imports preserve each source directory', 'descendant module fetch uses its importing module referrer', 'runtime static module loading resumes timers and frames', 'cross-origin modules require a matching CORS response', 'JSON preload reports unavailable while typed imports still work', 'module redirects preserve final URL and relative imports', 'module fetch rejects a non-JavaScript MIME type', 'module response referrer policy controls descendant requests', 'static module integrity permits matching bytes and denies mismatches', 'static import attribute failures retain TypeError', 'static parse reports identify the external module source', 'cached dynamic imports settle across turns and preserve namespace identity', 'inline module diagnostics retain document rows and source identity'),
    "single-document.html": (
        "reload preserves the live game document",
        "same-origin assignment preserves the live game document",
        "cross-origin assignment preserves the live game document",
        "location replacement preserves the live game document",
        "iframes cannot create a document or execute child script",
        "synthetic external link activation preserves the game",
        "auxiliary windows are refused without replacing the game",
        "History routes replace push back and forward without a new document",
        "hash routing preserves document and state",
    ),
    "events.html": (
        "postMessage preserves the single document source and origin",
        "promise jobs and queueMicrotask callbacks share one FIFO microtask queue",
        "an event listener preserves the document callback across route changes",
        "event listener list keeps its identity, once and mutation semantics",
        "inline event handlers keep identity, position and return semantics",
        "Document [LegacyOverrideBuiltIns] named properties win over the prototype on reads",
        "Window.HTMLDocument is the Document interface object",
        "HTMLFormElement named properties stay hidden by the prototype",
        "DOMStringMap [LegacyOverrideBuiltIns] data attributes win over the prototype on reads",
        "Document ownKeys keeps supported names with NUL and Unicode exact",
        "Document ownKeys lists only visible named properties, once",
        "HTMLFormElement ownKeys omits names hidden by the prototype",
        "HTMLFormElement ownKeys, reads and hasOwn work with a null prototype",
        "HTMLFormElement ownKeys lists a name once when a prototype trap shadows it",
        "HTMLFormElement ownKeys does not query the prototype for a name hidden by an own property",
        "HTMLFormElement ownKeys omits array-index names and keeps other numeric names",
        "Document ownKeys keeps numeric named properties",
        "Document visible named properties cannot be deleted without a named deleter",
        "HTMLFormElement deletes only hidden named properties and records no read",
        "HTMLFormElement named visibility queries prototypes through getOwnPropertyDescriptor",
        "DOMStringMap named deleter removes the data attribute",
    ),
    "stress.html": (
        "dedicated worker: promise jobs and queueMicrotask callbacks share one FIFO queue",
    ),
}
TOTAL = re.compile(r"SUITE TOTAL (\d+)/(\d+)(?: \((\d+) skipped\))?")


def pause_diagnostic_problems(output):
    """Require observations, not a performance threshold or a claimed suite pass."""
    prefix = "ROVES-V8 module-pause diagnostic "
    scenes = set()
    errors = []
    finished = False
    for line in output.splitlines():
        if line.startswith("SUITE TOTAL "):
            finished = True
        if not line.startswith(prefix):
            continue
        try:
            if finished:
                raise ValueError("diagnostic after suite completion")
            row = json.loads(line[len(prefix):])
            if not isinstance(row, dict):
                raise ValueError("expected object")
            scene = row.get("scene")
            if scene not in ("cold", "warm", "slow") or scene in scenes:
                raise ValueError("unknown or duplicate scene")
            for name in ("elapsed_ms", "max_timer_gap_ms", "max_frame_gap_ms"):
                value = row.get(name)
                if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                    raise ValueError("invalid duration")
            for name in ("timer_ticks", "frame_samples"):
                value = row.get(name)
                if type(value) is not int or value < 1:
                    raise ValueError("missing resumed timer/frame")
            if row.get("visibility") not in ("visible", "hidden"):
                raise ValueError("invalid visibility")
            scenes.add(scene)
        except (ValueError, TypeError, OverflowError):
            errors.append("invalid module-pause diagnostic")
    for scene in ("cold", "warm", "slow"):
        if scene not in scenes:
            errors.append(f"missing module-pause diagnostic: {scene}")
    return errors


def startup_diagnostic_problems(output):
    """Validate one startup observation without inventing a latency requirement."""
    prefix = "ROVES-V8 module-startup diagnostic "
    rows, finished = 0, False
    for line in output.splitlines():
        if line.startswith("SUITE TOTAL "):
            finished = True
        if not line.startswith(prefix):
            continue
        rows += 1
        try:
            row = json.loads(line[len(prefix):])
            if finished or rows != 1 or not isinstance(row, dict):
                raise ValueError("late, duplicate or non-object observation")
            for key in ("parser_span_ms", "dom_ready_ms", "first_frame_ms", "max_timer_gap_ms"):
                value = row.get(key)
                if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                    raise ValueError("invalid duration")
            ticks, before = row.get("timer_ticks"), row.get("ticks_before_dom_ready")
            if type(ticks) is not int or ticks < 1 or type(before) is not int or not 0 <= before <= ticks:
                raise ValueError("invalid timer observations")
            if row["dom_ready_ms"] < row["parser_span_ms"] or row.get("visibility") not in ("visible", "hidden"):
                raise ValueError("inconsistent startup observation")
        except (ValueError, TypeError, OverflowError):
            return ["invalid module-startup diagnostic"]
    return [] if rows == 1 else ["missing module-startup diagnostic"]


def validate(page, output, mode="normal", allow_missing_devices=False):
    errors = []
    counts = Counter()
    names = set()
    summary = None
    finished = False
    for line in output.splitlines():
        if not line.startswith("SUITE "):
            continue
        if finished:
            errors.append("harness output after its final total")
        if line.startswith("SUITE TOTAL "):
            match = TOTAL.fullmatch(line)
            if match is None:
                errors.append("malformed total")
            elif summary is not None:
                errors.append("duplicate total")
            else:
                summary = tuple(int(n or 0) for n in match.groups())
            finished = True
            continue
        match = re.fullmatch(r"SUITE (PASS|FAIL|SKIP) (.+)", line)
        if match is None:
            errors.append("malformed harness result")
            continue
        status, detail = match.groups()
        name = detail.split(" :: ", 1)[0]
        if name in names:
            errors.append(f"duplicate check: {name}")
        names.add(name)
        counts[status] += 1
        if status == "FAIL":
            errors.append(f"failed check: {detail}")
        elif status == "SKIP":
            key = (page, name)
            allowed = (mode == "jitless" and key in WASM_SKIPS) or (
                allow_missing_devices and key in HARDWARE_SKIPS
            )
            if not allowed:
                errors.append(f"unexpected skip: {name}")
        elif mode == "jitless" and (page, name) in WASM_SKIPS:
            errors.append(f"WebAssembly unexpectedly available in JIT-less: {name}")
    if summary is None:
        errors.append("missing final total (suite incomplete or timed out)")
    else:
        passed, total, skipped = summary
        if (passed, total, skipped) != (
            counts["PASS"], counts["PASS"] + counts["FAIL"], counts["SKIP"]
        ):
            errors.append("total does not match individual results")
        if not total:
            errors.append("suite executed no checks")
    for name in REQUIRED_CHECKS.get(page, ()):
        if name not in names:
            errors.append(f"missing required check: {name}")
    if page == "module-order.html":
        errors.extend(pause_diagnostic_problems(output))
        errors.extend(startup_diagnostic_problems(output))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("page")
    parser.add_argument("log", type=Path)
    parser.add_argument("--mode", choices=("normal", "jitless"), default="normal")
    parser.add_argument("--allow-missing-devices", action="store_true")
    args = parser.parse_args()
    errors = validate(args.page, args.log.read_text(errors="replace"), args.mode, args.allow_missing_devices)
    for error in errors:
        # Escape untrusted log text before emitting a GitHub annotation.
        message = error.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error::{args.page}: {message}")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
