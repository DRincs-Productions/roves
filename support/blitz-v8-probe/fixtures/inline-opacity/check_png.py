"""Inline opacity oracle for a published probe executable (no engine rebuild).

Applies to the ROVES_PROBE_SCREENSHOT capture of `--game fixtures/inline-opacity`
the same regions and thresholds as src/inline_opacity.rs, and reads the computed
opacities that the page wrote into the game_page report (optional log argument).

    python -I check_png.py <screenshot.png> [<probe.log>]

Exit 0: every gate expectation holds (opacity 0 culled, box opacity applied
once). Exit 1: at least one gate case failed (the negative); the per-line
evidence is printed either way. Partial opacity on non-atomic inlines is not
implemented in Blitz: those cases are reported under "partial_diagnostic"
(measured vs spec darkness) and only fail when the text is culled or a control
is painted. Exit 2: unusable input. Python standard library only.
"""

import json
import re
import struct
import sys
import zlib

WIDTH, HEIGHT = 640, 360
# Text region x +0..+100 and background-only control x +110..+140 of a cell.
TEXT_WIDTH, CONTROL_OFFSET, CONTROL_WIDTH = 100, 110, 30
# Expected darkest channel ranges (black over white at the given alpha).
OPAQUE, HALF, NOTHING = "opaque", "half", "nothing"
DARKEST = {OPAQUE: (0, 40), HALF: (100, 170)}
# Gate (case, left, top, height, expectation): same table as src/inline_opacity.rs
CASES = [
    ("visible", 20, 20, 60, OPAQUE),
    ("hidden", 175, 20, 60, NOTHING),
    ("atomic", 485, 20, 60, NOTHING),
    ("atomic-half", 20, 100, 60, HALF),
    ("ancestor", 330, 100, 60, HALF),
    ("stagger-1", 175, 180, 50, OPAQUE),
    ("stagger-3", 175, 280, 50, NOTHING),
]
# Diagnostics (case, left, top, height, spec alpha): partial opacity on
# non-atomic inlines, not implemented; same table as src/inline_opacity.rs
PARTIAL = [
    ("half", 330, 20, 60, 0.5),
    ("nested", 175, 100, 60, 0.25),
    ("ancestor-nested", 485, 100, 60, 0.25),
    ("decor", 20, 180, 60, 0.5),
    ("stagger-2", 175, 230, 50, 0.5),
]
# Element id -> computed opacity expected (getComputedStyle).
COMPUTED = {
    "visible": "1", "hidden": "0", "half": "0.5", "atomic": "0", "atomic-half": "0.5",
    "nested-outer": "0.5", "nested": "0.5", "ancestor-cell": "0.5", "ancestor": "1",
    "ancestor-nested-cell": "0.5", "ancestor-nested": "0.5", "decor": "0.5",
    "stagger-1": "1", "stagger-2": "0.5", "stagger-3": "0",
}


def read_rgba(path):
    with open(path, "rb") as handle:
        data = handle.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    offset, header, idat = 8, None, bytearray()
    while offset < len(data):
        (length,) = struct.unpack(">I", data[offset : offset + 4])
        kind = data[offset + 4 : offset + 8]
        body = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    width, height, depth, color, _, _, interlace = header
    if (width, height, depth, color, interlace) != (WIDTH, HEIGHT, 8, 6, 0):
        raise ValueError(f"expected {WIDTH}x{HEIGHT} RGBA8 non-interlaced, got {header}")
    raw = zlib.decompress(bytes(idat))
    stride, bpp = width * 4, 4
    rows, previous, position = [], bytearray(stride), 0
    for _ in range(height):
        kind, line = raw[position], bytearray(raw[position + 1 : position + 1 + stride])
        position += 1 + stride
        for i in range(stride):
            left = line[i - bpp] if i >= bpp else 0
            up = previous[i]
            corner = previous[i - bpp] if i >= bpp else 0
            if kind == 1:
                line[i] = (line[i] + left) & 0xFF
            elif kind == 2:
                line[i] = (line[i] + up) & 0xFF
            elif kind == 3:
                line[i] = (line[i] + ((left + up) >> 1)) & 0xFF
            elif kind == 4:
                p = left + up - corner
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                predictor = left if pa <= pb and pa <= pc else (up if pb <= pc else corner)
                line[i] = (line[i] + predictor) & 0xFF
            elif kind != 0:
                raise ValueError(f"bad PNG filter {kind}")
        rows.append(bytes(line))
        previous = line
    return rows


def region(rows, x0, y0, width, height):
    """(pixels differing from white beyond 2 or not opaque, darkest channel)"""
    differing, darkest = 0, 255
    for y in range(y0, y0 + height):
        row = rows[y]
        for x in range(x0, x0 + width):
            r, g, b, a = row[x * 4 : x * 4 + 4]
            if a != 255 or min(r, g, b) < 253:
                differing += 1
            darkest = min(darkest, r, g, b)
    return differing, darkest


def computed_from_log(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        match = re.search(r"INLINE_OPACITY_COMPUTED (\{[^}]*\})", handle.read().replace('\\"', '"'))
    return json.loads(match.group(1)) if match else None


def main():
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2
    try:
        rows = read_rgba(sys.argv[1])
    except (OSError, ValueError, zlib.error, TypeError) as error:
        print(f"INLINE_OPACITY unusable screenshot: {error}")
        return 2
    computed = computed_from_log(sys.argv[2]) if len(sys.argv) == 3 else None
    failures, cases = [], {}
    if computed is not None:
        for element, expected in COMPUTED.items():
            if computed.get(element) != expected:
                failures.append(f"{element}: computed opacity {computed.get(element)!r}, expected {expected!r}")
    for name, left, top, height, expected in CASES:
        glyphs, darkest = region(rows, left, top, TEXT_WIDTH, height)
        control, _ = region(rows, left + CONTROL_OFFSET, top, CONTROL_WIDTH, height)
        cases[name] = {"expected": expected, "glyph_pixels": glyphs, "darkest": darkest, "control_pixels": control}
        if control:
            failures.append(f"{name}: background control painted")
        if expected == NOTHING:
            if glyphs:
                failures.append(f"{name}: painted glyphs at opacity 0")
        else:
            low, high = DARKEST[expected]
            if not (glyphs > 100 and low <= darkest <= high):
                failures.append(f"{name}: expected {expected} glyphs, darkest {darkest}, {glyphs} pixels")
    partial = {}
    for name, left, top, height, alpha in PARTIAL:
        glyphs, darkest = region(rows, left, top, TEXT_WIDTH, height)
        control, _ = region(rows, left + CONTROL_OFFSET, top, CONTROL_WIDTH, height)
        partial[name] = {
            "spec_alpha": alpha, "spec_darkest": round(255 * (1 - alpha)),
            "glyph_pixels": glyphs, "darkest": darkest, "control_pixels": control,
            "painted_at_alpha_1": darkest <= 40,
        }
        if control:
            failures.append(f"{name}: background control painted")
        if glyphs <= 100:
            failures.append(f"{name}: partial opacity culled ({glyphs} pixels)")
    status = "passed" if not failures else "failed"
    print("INLINE_OPACITY " + json.dumps(
        {"status": status, "cases": cases, "partial_diagnostic": partial, "failures": failures}))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
