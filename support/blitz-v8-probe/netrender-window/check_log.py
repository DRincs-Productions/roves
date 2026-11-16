"""Gates for the Netrender window fixture log (CI and local runs).

    python -I check_log.py negative|positive LOG STATUS

negative: Netrender d2f13ca5 as published. The renderer must be ready, never
fall back, and the fixture must fail exactly the three pixels covered by the
translucent #after layer (Netrender blends the ops after an external texture
twice; support/netrender-shared-device-proof README, blocker 2).
positive: the same revision with the order fix: all six pixels pass, the whole
window ran on Netrender, the presentation and the WebGPU teardown passed and
the process exited 0.
"""

import json
import sys

AFTER_LAYER_PIXELS = {
    "after layer over before layer",
    "after layer over 50% canvas over before layer",
    "after layer over page background",
}


def results(log):
    found = {}
    with open(log, encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("ROVES_PROBE_RESULT "):
                try:
                    record = json.loads(line[len("ROVES_PROBE_RESULT "):])
                except json.JSONDecodeError:
                    continue
                found.setdefault(record.get("check"), []).append(record)
    return found


def main(kind, log, status):
    found = results(log)
    errors = []

    def one(check):
        records = found.get(check, [])
        if not records:
            errors.append(f"missing {check}")
            return {}
        return records[-1]

    if one("netrender_window_prepare").get("status") != "ready":
        errors.append(f"netrender_window_prepare: {found.get('netrender_window_prepare')}")
    if "netrender_window_fallback" in found:
        errors.append(f"fell back: {found['netrender_window_fallback']}")
    fixture = one("netrender_window_fixture")
    failing = {p["name"] for p in fixture.get("pixels", []) if not p.get("pass")}
    if fixture and fixture.get("readback") != "final_output_only":
        errors.append("the fixture must read back only the final output")
    if kind == "negative":
        if failing != AFTER_LAYER_PIXELS:
            errors.append(f"negative: failing pixels {sorted(failing)}, expected {sorted(AFTER_LAYER_PIXELS)}")
    else:
        if fixture.get("status") != "passed" or failing:
            errors.append(f"positive: failing pixels {sorted(failing)}")
        if one("netrender_window").get("status") != "netrender":
            errors.append(f"netrender_window: {found.get('netrender_window')}")
        if one("gpu_window_presentation").get("status") != "passed":
            errors.append(f"gpu_window_presentation: {found.get('gpu_window_presentation')}")
        if one("gpu_teardown").get("status") != "passed":
            errors.append(f"gpu_teardown: {found.get('gpu_teardown')}")
        if status != "0":
            errors.append(f"exit status {status}")
    for pixel in fixture.get("pixels", []):
        print(f"::notice::{kind} {pixel['name']}: {pixel['actual']} expected {pixel['expected']}")
    if errors:
        for error in errors:
            print(f"::error::{kind}: {error}"[:900])
        sys.exit(1)
    print(f"::notice::{kind}: Netrender window fixture as expected")


if __name__ == "__main__":
    main(*sys.argv[1:4])
