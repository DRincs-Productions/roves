"""Gates for the Netrender window fixture log (CI and local runs).

    python -I check_log.py negative|positive|resize|device-destroy|text|path LOG STATUS

negative: Netrender d2f13ca5 as published. The renderer must be ready, never
fall back, and the fixture must fail exactly the three pixels covered by the
translucent #after layer (Netrender blends the ops after an external texture
twice; support/netrender-shared-device-proof README, blocker 2).
positive: the same revision with the order fix: all six pixels pass, the whole
window ran on Netrender, the presentation and the WebGPU teardown passed and
the process exited 0.

Lifecycle scenarios (positive build, ROVES_PROBE_NETRENDER_SCENARIO, separate
runs). Both first pass every positive gate above, unchanged (the six
FIXTURE_CHECKS pixels are read back before the scenario starts), then:
resize: one native resize was requested after that read back, the page saw
the new viewport, and netrender_window_resize passed at the requested size
(latest canvas color, area added by the resize, no stretched frame), still
on Netrender with no fallback.
device-destroy: the page's own GPUDevice.destroy() ran once after that read
back, the renderer stopped exactly once (netrender_window_device_lost, no
fallback renderer on the lost device), the page's own device.lost handler ran
with reason "destroyed", nothing panicked, and the normal close and WebGPU
teardown passed.
text: the page showed #text (two opaque yellow U+2588 glyphs) over the canvas
once after that read back, Netrender drew the glyph runs itself (no fallback),
and netrender_window_text passed: the two glyph cells are text-colored, the
canvas left of and below them is unchanged (no text box), every pixel derived
from the translated run lies over the canvas, and no glyph is .notdef.
path: the page showed #circle (a curved path over the canvas) and #diamond (a
rotated square over #before) once after that read back, Netrender drew the
filled paths itself (no fallback), and netrender_window_path passed all five
PATH_CHECKS pixels: inside each shape, the circle's box corner outside its
curve, the diamond's tip beyond the unrotated square and the unrotated
square's corner outside the diamond.
"""

import json
import sys

SCENARIO_CHECKS = (
    "window_scenario_resize",
    "window_scenario_page_call",
    "netrender_window_resize",
    "netrender_window_device_lost",
    "netrender_window_text",
    "netrender_window_path",
)

# Scenario records each kind may contain (all others must be absent).
ALLOWED_SCENARIO_CHECKS = {
    "resize": {"window_scenario_resize", "netrender_window_resize"},
    "device-destroy": {"window_scenario_page_call", "netrender_window_device_lost"},
    "text": {"window_scenario_page_call", "netrender_window_text"},
    "path": {"window_scenario_page_call", "netrender_window_path"},
}

PATH_PIXELS = 5

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


def page_events(log):
    """NETRENDER_FIXTURE_PAGE lines printed by the fixture page itself."""
    events = []
    with open(log, encoding="utf-8", errors="replace") as f:
        for line in f:
            if line.startswith("NETRENDER_FIXTURE_PAGE "):
                try:
                    events.append(json.loads(line[len("NETRENDER_FIXTURE_PAGE "):]))
                except json.JSONDecodeError:
                    continue
    return events


def panics(log):
    with open(log, encoding="utf-8", errors="replace") as f:
        return [line.strip() for line in f if "panicked at" in line]


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
        expected_window = "device_lost" if kind == "device-destroy" else "netrender"
        if one("netrender_window").get("status") != expected_window:
            errors.append(f"netrender_window: {found.get('netrender_window')}")
        if one("gpu_window_presentation").get("status") != "passed":
            errors.append(f"gpu_window_presentation: {found.get('gpu_window_presentation')}")
        if one("gpu_teardown").get("status") != "passed":
            errors.append(f"gpu_teardown: {found.get('gpu_teardown')}")
        if status != "0":
            errors.append(f"exit status {status}")
    if kind == "resize":
        errors += resize_errors(found, page_events(log))
    elif kind == "device-destroy":
        errors += device_destroy_errors(found, page_events(log), panics(log))
    elif kind == "text":
        errors += text_errors(found, page_events(log), panics(log))
    elif kind == "path":
        errors += path_errors(found, page_events(log), panics(log))
    unexpected = sorted(
        check for check in SCENARIO_CHECKS
        if check in found and check not in ALLOWED_SCENARIO_CHECKS.get(kind, set())
    )
    if unexpected:
        errors.append(f"unexpected scenario records for {kind}: {unexpected}")
    for pixel in fixture.get("pixels", []):
        print(f"::notice::{kind} {pixel['name']}: {pixel['actual']} expected {pixel['expected']}")
    for pixel in (found.get("netrender_window_resize") or [{}])[-1].get("pixels", []):
        print(f"::notice::{kind} after resize {pixel['name']}: {pixel['actual']} expected {pixel['expected']}")
    for pixel in (found.get("netrender_window_text") or [{}])[-1].get("pixels", []):
        print(f"::notice::{kind} text {pixel['name']} at ({pixel['x']}, {pixel['y']}): {pixel['actual']} expected {pixel['expected']}")
    for pixel in (found.get("netrender_window_path") or [{}])[-1].get("pixels", []):
        print(f"::notice::{kind} path {pixel['name']} at ({pixel['x']}, {pixel['y']}): {pixel['actual']} expected {pixel['expected']}")
    if errors:
        for error in errors:
            print(f"::error::{kind}: {error}"[:900])
        sys.exit(1)
    print(f"::notice::{kind}: Netrender window fixture as expected")


def resize_errors(found, events):
    errors = []
    requests = found.get("window_scenario_resize", [])
    if len(requests) != 1:
        errors.append(f"resize: expected one native resize request, got {requests}")
    if "window_scenario_page_call" in found or "netrender_window_device_lost" in found:
        errors.append("resize: unexpected page call or device loss")
    resized = [e for e in events if e.get("event") == "resized"]
    if len(resized) != 1:
        errors.append(f"resize: the page must see the new viewport once, got {resized}")
    records = found.get("netrender_window_resize", [])
    if len(records) != 1:
        errors.append(f"resize: expected one netrender_window_resize record, got {len(records)}")
        return errors
    record = records[0]
    failing = sorted(p["name"] for p in record.get("pixels", []) if not p.get("pass"))
    if record.get("status") != "passed" or failing or not record.get("pixels"):
        errors.append(f"resize: failing pixels {failing}")
    if record.get("readback") != "final_output_only":
        errors.append("resize: the read back must be of the final output only")
    fixture = (found.get("netrender_window_fixture") or [{}])[-1]
    before = (fixture.get("width"), fixture.get("height"))
    after = (record.get("width"), record.get("height"))
    if None in before or None in after or not (after[0] > before[0] and after[1] > before[1]):
        errors.append(f"resize: size {before} -> {after}, expected larger in both directions")
    if requests and record.get("requested_logical") != requests[0].get("logical"):
        errors.append(f"resize: read back for {record.get('requested_logical')}, requested {requests[0].get('logical')}")
    return errors


def device_destroy_errors(found, events, panic_lines):
    errors = []
    calls = found.get("window_scenario_page_call", [])
    if len(calls) != 1 or calls[0].get("status") != "ran":
        errors.append(f"device-destroy: expected one page call that ran, got {calls}")
    if "window_scenario_resize" in found or "netrender_window_resize" in found:
        errors.append("device-destroy: unexpected resize")
    lost = found.get("netrender_window_device_lost", [])
    if len(lost) != 1:
        errors.append(f"device-destroy: expected exactly one device-lost stop, got {lost}")
    elif lost[0].get("renderer") != "netrender" or lost[0].get("fixture_verified") is not True:
        errors.append(f"device-destroy: stop {lost[0]}, expected on Netrender after the verified frame")
    destroys = [e for e in events if e.get("event") == "destroy"]
    handled = [e for e in events if e.get("event") == "lost"]
    if len(destroys) != 1:
        errors.append(f"device-destroy: the page must destroy its device once, got {destroys}")
    if len(handled) != 1 or handled[0].get("reason") != "destroyed" or handled[0].get("destroyed_by_page") is not True:
        errors.append(f"device-destroy: the page's own lost handler must run once with reason destroyed, got {handled}")
    if panic_lines:
        errors.append(f"device-destroy: panic in the log: {panic_lines[:3]}")
    closed = (found.get("game_window_closed") or [{}])[-1]
    if closed.get("reason") != "frame_limit":
        errors.append(f"device-destroy: window close {closed}, expected the normal frame-limit close")
    return errors


def text_errors(found, events, panic_lines):
    errors = []
    calls = found.get("window_scenario_page_call", [])
    if len(calls) != 1 or calls[0].get("status") != "ran":
        errors.append(f"text: expected one page call that ran, got {calls}")
    shown = [e for e in events if e.get("event") == "text"]
    if len(shown) != 1:
        errors.append(f"text: the page must show #text once, got {shown}")
    records = found.get("netrender_window_text", [])
    if len(records) != 1:
        errors.append(f"text: expected one netrender_window_text record, got {records}")
        return errors
    record = records[0]
    failing = sorted(p["name"] for p in record.get("pixels", []) if not p.get("pass"))
    if record.get("status") != "passed" or failing or len(record.get("pixels", [])) != 4:
        errors.append(f"text: {record.get('reason') or 'failing pixels'} {failing}")
    if record.get("readback") != "final_output_only":
        errors.append("text: the read back must be of the final output only")
    if record.get("notdef_glyphs") != 0 or len(record.get("glyph_ids", [])) < 2:
        errors.append(f"text: glyphs {record.get('glyph_ids')} (no .notdef, at least two expected)")
    window = (found.get("netrender_window") or [{}])[-1]
    if not window.get("glyph_runs"):
        errors.append(f"text: Netrender drew no glyph run: {window}")
    if panic_lines:
        errors.append(f"text: panic in the log: {panic_lines[:3]}")
    return errors


def path_errors(found, events, panic_lines):
    errors = []
    calls = found.get("window_scenario_page_call", [])
    if len(calls) != 1 or calls[0].get("status") != "ran":
        errors.append(f"path: expected one page call that ran, got {calls}")
    shown = [e for e in events if e.get("event") == "paths"]
    if len(shown) != 1:
        errors.append(f"path: the page must show the shapes once, got {shown}")
    records = found.get("netrender_window_path", [])
    if len(records) != 1:
        errors.append(f"path: expected one netrender_window_path record, got {records}")
        return errors
    record = records[0]
    failing = sorted(p["name"] for p in record.get("pixels", []) if not p.get("pass"))
    if record.get("status") != "passed" or failing or len(record.get("pixels", [])) != PATH_PIXELS:
        errors.append(f"path: failing pixels {failing} of {len(record.get('pixels', []))}")
    if record.get("readback") != "final_output_only":
        errors.append("path: the read back must be of the final output only")
    if record.get("paths_in_frame") != 2:
        errors.append(f"path: {record.get('paths_in_frame')} paths in the read-back frame, expected 2")
    window = (found.get("netrender_window") or [{}])[-1]
    if not window.get("paths"):
        errors.append(f"path: Netrender drew no path: {window}")
    if panic_lines:
        errors.append(f"path: panic in the log: {panic_lines[:3]}")
    return errors


if __name__ == "__main__":
    main(*sys.argv[1:4])
