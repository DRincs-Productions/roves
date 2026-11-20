"""Roves vs Chrome GPU canvas compositing checker (Python standard library only).

Checks one engine's capture of support/perf/gpu-composite/index.html against the
#gc-cases table embedded in that same file: element rects reported by the page
(ROVES_GPU_COMPOSITE_RESULT) and pixels in fixed regions of a 640x360 PNG. Helpers
(PNG reader, region counting, rings, report text decoding) are imported from
../visual-matrix/check.py, which itself reuses the inline-opacity check_png.py.

    python -I check.py --engine roves|chrome --png <shot.png> --report <file>
                       [--fixture <index.html>] [--out <result.json>]
    python -I check.py --compare <roves-result.json> <chrome-result.json>

Per case classification:
  match                    every `paint` and `expect` region right
  known_no_blur /          `paint` right, `expect` wrong and the case's declared
  known_no_canvas_clip     `alternative` right: the documented Roves divergence
  unsupported_paint        anything else
  unsupported_api          the page reported errors or no rects
Region kinds: `color` (+ `tolerance`) or `range` (per channel [lo, hi]); `inset`
(default tolerance.inset) shrinks the rect first; an optional `ring` checks the
band between ring_inner and ring_outer px outside `bounds` against a colour.

Exit 0: engine chrome, every case `match`; engine roves, every gated case `match`
and every other case `match` or its declared known divergence (listed in the
output, never folded into `match`). Exit 1: otherwise. Exit 2: unusable input.
"""

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "index.html"
VISUAL_CHECK = HERE.parent / "visual-matrix" / "check.py"
PREFIX = "ROVES_GPU_COMPOSITE_RESULT "


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


vm = load_module(VISUAL_CHECK, "visual_matrix_check")


def load_cases(path):
    text = Path(path).read_text(encoding="utf-8")
    match = re.search(r'<script type="application/json" id="gc-cases">(.*?)</script>', text, re.S)
    if not match:
        raise ValueError(f"no #gc-cases block in {path}")
    return json.loads(match.group(1))


def load_report(path):
    text = vm.read_text(path)
    try:
        whole = json.loads(text)
    except ValueError:
        whole = None
    if isinstance(whole, dict) and whole.get("schema") == "roves-gpu-composite":
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
        report, _ = json.JSONDecoder().raw_decode(rest.replace('\\\\', '\\').replace('\\"', '"'))
    return report


def pixels_out_of_range(rows, rect, ranges):
    x0, y0, width, height = (int(v) for v in rect)
    off = 0
    for y in range(max(y0, 0), min(y0 + height, len(rows))):
        row = rows[y]
        for x in range(max(x0, 0), min(x0 + width, len(row) // 4)):
            pixel = row[x * 4 : x * 4 + 3]
            if any(not lo <= value <= hi for value, (lo, hi) in zip(pixel, ranges)):
                off += 1
    return off


def mean_rgb(rows, rect):
    x0, y0, width, height = (int(v) for v in rect)
    total, count = [0, 0, 0], 0
    for y in range(max(y0, 0), min(y0 + height, len(rows))):
        row = rows[y]
        for x in range(max(x0, 0), min(x0 + width, len(row) // 4)):
            for i in range(3):
                total[i] += row[x * 4 + i]
            count += 1
    return [round(v / count, 1) for v in total] if count else None


def check_regions(items, rows, tol, kind):
    results, ok = [], True
    for item in items:
        rect = vm.inset(item["rect"], item.get("inset", tol["inset"]))
        if "range" in item:
            off = pixels_out_of_range(rows, rect, item["range"])
            want = {"range": item["range"]}
        else:
            off = vm.pixels_off(rows, rect, item["color"], item.get("tolerance", tol["color"]))
            want = {"color": item["color"]}
        results.append({"kind": kind, "rect": rect, **want, "off": off, "mean": mean_rgb(rows, rect)})
        ok = ok and off == 0
    return ok, results


def check_case(case, rows, tol):
    paint_ok, regions = check_regions(case.get("paint", []), rows, tol, "paint")
    if "ring" in case:
        ring = case["ring"]
        off = sum(vm.pixels_off(rows, strip, ring["color"], tol["color"])
                  for strip in vm.ring(ring["bounds"], tol["ring_inner"], tol["ring_outer"]))
        regions.append({"kind": "ring", "bounds": ring["bounds"], "color": ring["color"], "off": off})
        paint_ok = paint_ok and off == 0
    expect_ok, expect = check_regions(case.get("expect", []), rows, tol, "expect")
    alt_ok, alternative = check_regions(case.get("alternative", []), rows, tol, "alternative")
    regions += expect + alternative
    if not paint_ok:
        return "unsupported_paint", regions
    if expect_ok:
        return "match", regions
    if case.get("known") and case.get("alternative") and alt_ok:
        return case["known"], regions
    return "unsupported_paint", regions


def check_state(spec, report):
    tol = spec["tolerance"]["px"]
    state, ok = {}, True
    for name, want in spec["state"].items():
        got = (report.get("state") or {}).get(name)
        good = isinstance(got, list) and len(got) == 4 and all(
            isinstance(v, (int, float)) and abs(v - w) <= tol for v, w in zip(got, want))
        state[name] = {"expected": want, "actual": got, "ok": good}
        ok = ok and good
    return ok, state


def check(args):
    check_png = vm.load_check_png()
    try:
        spec = load_cases(args.fixture)
        rows = check_png.read_rgba(args.png)
        report = load_report(args.report)
    except (OSError, ValueError, TypeError) as error:
        print(f"GPU_COMPOSITE unusable input: {error}")
        return 2
    problems, warnings = [], []
    if report is None:
        problems.append("no ROVES_GPU_COMPOSITE_RESULT report")
        report = {"status": "missing"}
    elif report.get("status") != "ok":
        problems.append(f"report status {report.get('status')}: errors {report.get('errors')}, "
                        f"gpu_errors {(report.get('webgpu') or {}).get('gpu_errors')}")
    if report.get("fixture_version") not in (None, spec["fixture_version"]):
        problems.append(f"report fixture_version {report.get('fixture_version')} != {spec['fixture_version']}")
    viewport = report.get("viewport") or {}
    if viewport.get("device_pixel_ratio") not in (None, 1):
        print(f"GPU_COMPOSITE unusable input: devicePixelRatio {viewport.get('device_pixel_ratio')}, need 1")
        return 2
    if viewport and [viewport.get("inner_width"), viewport.get("inner_height")] != spec["viewport"]:
        warnings.append(f"viewport {viewport.get('inner_width')}x{viewport.get('inner_height')} != 640x360")
    state_ok, state = check_state(spec, report)
    api_failed = report.get("status") in ("missing", "failed", "no_webgpu", "no_adapter") or bool(report.get("errors"))
    cases, gate_failures, known = {}, [], []
    for case in spec["cases"]:
        classification, regions = check_case(case, rows, spec["tolerance"])
        if api_failed:
            classification = "unsupported_api"
        gated = args.engine == "chrome" or case["gate"]
        accepted = {"match"} if gated else {"match", case.get("known")}
        if classification not in accepted:
            gate_failures.append(f"{case['id']}: {classification}")
        elif classification != "match":
            known.append(f"{case['id']}: {classification}")
        cases[case["id"]] = {"gated": gated, "known": case.get("known"),
                             "classification": classification, "regions": regions}
    if not state_ok:
        problems.append("element rects differ: " + ", ".join(k for k, v in state.items() if not v["ok"]))
    status = "passed" if not gate_failures and not problems else "failed"
    if status == "passed" and known:
        status = "passed_with_known_divergences"
    resolved = [c["id"] for c in spec["cases"]
                if args.engine == "roves" and c.get("known") and cases[c["id"]]["classification"] == "match"]
    if resolved:
        warnings.append("known divergence no longer observed (update the fixture/docs): " + ", ".join(resolved))
    result = {
        "schema": "roves-gpu-composite-check", "version": 1, "engine": args.engine, "status": status,
        "fixture": str(Path(args.fixture).resolve()), "fixture_sha256": vm.sha256(args.fixture),
        "fixture_version": spec["fixture_version"], "png_sha256": vm.sha256(args.png),
        "report_status": report.get("status"), "report_frames": report.get("frames"),
        "report_errors": report.get("errors"), "webgpu": report.get("webgpu"), "info": report.get("info"),
        "viewport": viewport, "user_agent": report.get("user_agent"), "state": state,
        "problems": problems, "warnings": warnings, "gate_failures": gate_failures,
        "known_divergences": known, "cases": cases,
    }
    if args.out:
        Path(args.out).write_text(json.dumps(result, indent=2), encoding="utf-8")
    summary = {name: c["classification"] for name, c in cases.items()}
    print("GPU_COMPOSITE " + json.dumps({"engine": args.engine, "status": status, "classification": summary,
                                         "known_divergences": known, "gate_failures": gate_failures,
                                         "problems": problems, "warnings": warnings}))
    return 0 if status.startswith("passed") else 1


def compare(paths):
    results = [json.loads(Path(p).read_text(encoding="utf-8")) for p in paths]
    if len({r["fixture_sha256"] for r in results}) != 1:
        print("GPU_COMPOSITE_COMPARE different fixture bytes: " + ", ".join(r["fixture_sha256"] for r in results))
        return 2
    rows = []
    for name in results[0]["cases"]:
        row = {"case": name}
        for r in results:
            c = r["cases"].get(name, {})
            row[r["engine"]] = c.get("classification")
            row[r["engine"] + "_means"] = [region.get("mean") for region in c.get("regions", [])]
        rows.append(row)
        print(f"{name:22} " + "  ".join(f"{r['engine']}={row[r['engine']]}" for r in results))
    print("GPU_COMPOSITE_COMPARE " + json.dumps({"fixture_sha256": results[0]["fixture_sha256"], "cases": rows}))
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
