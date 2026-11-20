"""Roves vs Chrome deterministic visual matrix checker (Python standard library only).

Checks one engine's capture of support/perf/visual-matrix/index.html against the
#vm-cases table embedded in that same file: computed state reported by the page
(ROVES_VISUAL_RESULT) and actual pixels in fixed regions of a 640x360 PNG. No
full-screenshot equality: only solid-colour interiors (inset from every edge),
vacated/blank regions and a background ring around each case's bounds.

    python -I check.py --engine roves|chrome --png <shot.png> --report <file>
                       [--fixture <index.html>] [--out <result.json>]
    python -I check.py --compare <roves-result.json> <chrome-result.json>

--report: the probe stdout log (line `ROVES_VISUAL_RESULT {...}`), or a file holding
that line, the report object, or a JSON string of the line (Chrome DevTools
evaluate_script output). PNG decoding is shared with
support/blitz-v8-probe/fixtures/inline-opacity/check_png.py (640x360 RGB/RGBA).

Per case classification: match, unsupported_paint (state right, pixels wrong),
unsupported_state (pixels right, state wrong), unsupported_state_and_paint,
unsupported_api (an API threw or a value was missing), not_reported.
Exit 0: every gated case matches (engine chrome: every case, since Chrome
validates the fixture; engine roves: cases with "gate": true, the others are
diagnostic). Exit 1: a gated case does not match. Exit 2: unusable input.
"""

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "index.html"
CHECK_PNG = HERE.parents[1] / "blitz-v8-probe" / "fixtures" / "inline-opacity" / "check_png.py"
PREFIX = "ROVES_VISUAL_RESULT "


def load_check_png():
    spec = importlib.util.spec_from_file_location("inline_opacity_check_png", CHECK_PNG)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_cases(path):
    text = Path(path).read_text(encoding="utf-8")
    match = re.search(r'<script type="application/json" id="vm-cases">(.*?)</script>', text, re.S)
    if not match:
        raise ValueError(f"no #vm-cases block in {path}")
    return json.loads(match.group(1))


def read_text(path):
    """UTF-8 (with or without BOM) or UTF-16 with BOM (Windows PowerShell redirection)."""
    data = Path(path).read_bytes()
    if data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        return data.decode("utf-16", errors="replace")
    return data.decode("utf-8-sig", errors="replace")


def load_report(path):
    text = read_text(path)
    try:
        whole = json.loads(text)
    except ValueError:
        whole = None
    if isinstance(whole, dict) and whole.get("schema") == "roves-visual-matrix":
        return whole
    if isinstance(whole, str):
        text = whole
    index = text.rfind(PREFIX)
    if index < 0:
        return None
    rest = text[index + len(PREFIX):]
    try:
        report, _ = json.JSONDecoder().raw_decode(rest)
    except ValueError:
        # The line quoted inside a wrapper (e.g. a tool's JSON string output).
        report, _ = json.JSONDecoder().raw_decode(rest.replace('\\\\', '\\').replace('\\"', '"'))
    return report


def pixels_off(rows, rect, color, tolerance):
    """Pixels in rect (x, y, w, h) whose RGB differs from color by more than tolerance."""
    x0, y0, width, height = (int(v) for v in rect)
    off = 0
    for y in range(max(y0, 0), min(y0 + height, len(rows))):
        row = rows[y]
        for x in range(max(x0, 0), min(x0 + width, len(row) // 4)):
            r, g, b = row[x * 4 : x * 4 + 3]
            if max(abs(r - color[0]), abs(g - color[1]), abs(b - color[2])) > tolerance:
                off += 1
    return off


def inset(rect, amount):
    x, y, w, h = rect
    return [x + amount, y + amount, w - 2 * amount, h - 2 * amount]


def ring(rect, inner, outer):
    """Four strips between rect grown by `inner` and rect grown by `outer`."""
    x, y, w, h = rect
    band = outer - inner
    return [
        [x - outer, y - outer, w + 2 * outer, band],
        [x - outer, y + h + inner, w + 2 * outer, band],
        [x - outer, y - inner, band, h + 2 * inner],
        [x + w + inner, y - inner, band, h + 2 * inner],
    ]


def check_state(expected, actual, tol):
    result, ok, missing = {}, True, False
    for key, want in expected.items():
        got = actual.get(key)
        if key == "rect":
            good = isinstance(got, list) and len(got) == 4 and all(
                isinstance(v, (int, float)) and abs(v - w) <= tol["px"] for v, w in zip(got, want))
        elif key == "opacity":
            try:
                good = abs(float(got) - want) <= tol["opacity"]
            except (TypeError, ValueError):
                good = False
        else:
            good = isinstance(got, (int, float)) and abs(got - want) <= tol["px"]
        if got is None or got == "":
            missing = True
        ok = ok and good
        result[key] = {"expected": want, "actual": got, "ok": good}
    return ok, missing, result


def check_paint(case, rows, spec):
    tol = spec["tolerance"]
    background = spec["background"]
    paint, ok = {"regions": []}, True
    for item in case["paint"]:
        region = inset(item["rect"], tol["inset_paint"])
        off = pixels_off(rows, region, item["color"], item.get("tolerance", tol["color"]))
        paint["regions"].append({"kind": "paint", "rect": region, "color": item["color"], "off": off})
        ok = ok and off == 0
    for rect in case["blank"]:
        region = inset(rect, tol["inset_blank"])
        off = pixels_off(rows, region, background, tol["background"])
        paint["regions"].append({"kind": "blank", "rect": region, "off": off})
        ok = ok and off == 0
    ring_off = sum(pixels_off(rows, strip, background, tol["background"])
                   for strip in ring(case["bounds"], tol["ring_inner"], tol["ring_outer"]))
    paint["regions"].append({"kind": "ring", "bounds": case["bounds"], "off": ring_off})
    return ok and ring_off == 0, paint


def classify(errors, state_ok, missing, paint_ok):
    if errors or missing:
        return "unsupported_api"
    if state_ok and paint_ok:
        return "match"
    if state_ok:
        return "unsupported_paint"
    if paint_ok:
        return "unsupported_state"
    return "unsupported_state_and_paint"


def check(args):
    check_png = load_check_png()
    try:
        spec = load_cases(args.fixture)
        rows = check_png.read_rgba(args.png)
        report = load_report(args.report)
    except (OSError, ValueError, TypeError) as error:
        print(f"VISUAL_MATRIX unusable input: {error}")
        return 2
    problems = []
    if report is None:
        problems.append("no ROVES_VISUAL_RESULT report")
        report = {"cases": {}, "status": "missing"}
    elif report.get("status") != "ok":
        problems.append(f"report status {report.get('status')}: {report.get('errors')}")
    if report.get("fixture_version") not in (None, spec["fixture_version"]):
        problems.append(f"report fixture_version {report.get('fixture_version')} != {spec['fixture_version']}")
    viewport = report.get("viewport") or {}
    if viewport.get("device_pixel_ratio") not in (None, 1):
        print(f"VISUAL_MATRIX unusable input: devicePixelRatio {viewport.get('device_pixel_ratio')}, need 1")
        return 2
    warnings = []
    if viewport and [viewport.get("inner_width"), viewport.get("inner_height")] != spec["viewport"]:
        # Geometry is absolute from the top-left corner, so this does not move any region.
        warnings.append(f"viewport {viewport.get('inner_width')}x{viewport.get('inner_height')} != 640x360")
    cases, gate_failures = {}, []
    for case in spec["cases"]:
        reported = report.get("cases", {}).get(case["id"])
        gated = args.engine == "chrome" or case["gate"]
        # Pixels are checked even without a report, as evidence.
        paint_ok, paint = check_paint(case, rows, spec)
        paint["ok"] = paint_ok
        if reported is None:
            classification, state, errors = "not_reported", {}, []
        else:
            errors = reported.get("errors") or []
            state_ok, missing, state = check_state(case["state"], reported.get("state") or {}, spec["tolerance"])
            classification = classify(errors, state_ok, missing, paint_ok)
            state["transform_info"] = (reported.get("state") or {}).get("transform")
        cases[case["id"]] = {"group": case["group"], "gated": gated, "classification": classification,
                             "errors": errors, "state": state, "paint": paint}
        if gated and classification != "match":
            gate_failures.append(f"{case['id']}: {classification}")
    status = "passed" if not gate_failures and not problems else "failed"
    result = {
        "schema": "roves-visual-matrix-check", "version": 1, "engine": args.engine, "status": status,
        "fixture": str(Path(args.fixture).resolve()), "fixture_sha256": sha256(args.fixture),
        "fixture_version": spec["fixture_version"], "png_sha256": sha256(args.png),
        "report_status": report.get("status"), "report_frames": report.get("frames"),
        "report_errors": report.get("errors"), "viewport": viewport, "user_agent": report.get("user_agent"),
        "problems": problems, "warnings": warnings, "gate_failures": gate_failures, "cases": cases,
    }
    if args.out:
        Path(args.out).write_text(json.dumps(result, indent=2), encoding="utf-8")
    summary = {name: c["classification"] for name, c in cases.items()}
    print("VISUAL_MATRIX " + json.dumps({"engine": args.engine, "status": status, "classification": summary,
                                         "gate_failures": gate_failures, "problems": problems,
                                         "warnings": warnings}))
    return 0 if status == "passed" else 1


def compare(paths):
    results = [json.loads(Path(p).read_text(encoding="utf-8")) for p in paths]
    if len({r["fixture_sha256"] for r in results}) != 1:
        print("VISUAL_MATRIX_COMPARE different fixture bytes: " + ", ".join(r["fixture_sha256"] for r in results))
        return 2
    rows = []
    for name in results[0]["cases"]:
        row = {"case": name}
        for r in results:
            c = r["cases"].get(name, {})
            row[r["engine"]] = c.get("classification")
            row[r["engine"] + "_state"] = {k: v.get("actual") for k, v in (c.get("state") or {}).items()
                                           if isinstance(v, dict)}
        rows.append(row)
        print(f"{name:24} " + "  ".join(f"{r['engine']}={row[r['engine']]}" for r in results))
    print("VISUAL_MATRIX_COMPARE " + json.dumps({"fixture_sha256": results[0]["fixture_sha256"], "cases": rows}))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--engine", choices=["roves", "chrome"])
    parser.add_argument("--png")
    parser.add_argument("--report")
    parser.add_argument("--fixture", default=str(FIXTURE))
    parser.add_argument("--out")
    parser.add_argument("--compare", nargs=2, metavar=("A", "B"))
    args = parser.parse_args()
    if args.compare:
        return compare(args.compare)
    if not (args.engine and args.png and args.report):
        parser.error("--engine, --png and --report are required (or --compare A B)")
    return check(args)


if __name__ == "__main__":
    sys.exit(main())
