#!/usr/bin/env python3
"""Validate the smoke harness protocol; no browser or third-party packages required."""

import argparse
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
    "events.html": (
        "an iframe keeps its WindowProxy across origin changes and back",
        "promise jobs and queueMicrotask callbacks share one FIFO microtask queue",
        "an event listener keeps the callback context of its registration",
        "event listener list keeps its identity, once and mutation semantics",
    ),
    "stress.html": (
        "dedicated worker: promise jobs and queueMicrotask callbacks share one FIFO queue",
    ),
}
TOTAL = re.compile(r"SUITE TOTAL (\d+)/(\d+)(?: \((\d+) skipped\))?")


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
