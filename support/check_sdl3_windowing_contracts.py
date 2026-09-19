#!/usr/bin/env python3
"""Fast structural checks for the SDL3 desktop event pipeline."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVENT_LOOP = ROOT / "ports/servoshell/desktop/event_loop.rs"
HEADED_WINDOW = ROOT / "ports/servoshell/desktop/headed_window.rs"
TRACING = ROOT / "ports/servoshell/desktop/tracing.rs"
KEYUTILS = ROOT / "ports/servoshell/desktop/keyutils.rs"
PATCH = ROOT / "patches/servo-v0.5.0/0001-desktop-shell-core.patch"


def braced_body(source: str, marker: str) -> str:
    start = source.index(marker)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : index]
    raise AssertionError(f"unclosed block after {marker!r}")


def enum_variants(source: str, name: str) -> set[str]:
    body = braced_body(source, f"enum {name}")
    return set(re.findall(r"(?m)^    ([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)", body))


def main() -> None:
    event_source = EVENT_LOOP.read_text(encoding="utf-8")
    headed_source = HEADED_WINDOW.read_text(encoding="utf-8")
    tracing_source = TRACING.read_text(encoding="utf-8")
    keyutils_source = KEYUTILS.read_text(encoding="utf-8")
    variants = enum_variants(event_source, "WindowEvent")
    assert variants, "WindowEvent has no detected variants"

    trace_body = braced_body(tracing_source, "impl LogTarget for WindowEvent")
    traced = set(re.findall(r"Self::([A-Z][A-Za-z0-9_]*)", trace_body))
    assert variants == traced, (
        f"WindowEvent tracing mismatch: missing={sorted(variants - traced)}, "
        f"unknown={sorted(traced - variants)}"
    )
    assert "_ =>" not in trace_body, "WindowEvent tracing must remain exhaustive"

    handler = braced_body(headed_source, "pub(crate) fn handle_window_event")
    handled = set(re.findall(r"WindowEvent::([A-Z][A-Za-z0-9_]*)", handler))
    assert variants <= handled, f"unhandled WindowEvent variants: {sorted(variants - handled)}"

    translated = braced_body(event_source, "fn translate_sdl_event")
    gameplay_events = {
        "KeyDown", "KeyUp", "MouseMotion", "MouseButtonDown",
        "MouseButtonUp", "MouseWheel", "CursorLeft",
    }
    translated_variants = set(re.findall(r"WindowEvent::([A-Z][A-Za-z0-9_]*)", translated))
    assert gameplay_events <= translated_variants, "SDL3 gameplay input translation is incomplete"
    assert "DroppedFile" in translated_variants, "SDL3 dropped-file translation is missing"
    assert "Touch" in translated_variants, "SDL3 touch translation is missing"
    for event_name in ("FingerDown", "FingerMotion", "FingerUp", "FingerCanceled"):
        assert event_name in translated, f"SDL3 {event_name} translation is missing"

    keyboard_factory = braced_body(keyutils_source, "pub fn keyboard_event_from_sdl")
    assert re.search(
        r"keyboard_modifiers_from_sdl_mod\(keymod\),\s*repeat,\s*false,",
        keyboard_factory,
    ), "KeyboardEvent repeat/is_composing arguments are swapped"

    subprocess.run(
        ["git", "apply", "--numstat", str(PATCH)],
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    print(f"SDL3 windowing contracts OK ({len(variants)} WindowEvent variants)")


if __name__ == "__main__":
    main()
