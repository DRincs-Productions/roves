#!/usr/bin/env python3
"""Loopback smoke fixtures using the standard-library HTTP server."""
import functools
import http.server
import json
import sys
import time
import re
import threading
from urllib.parse import parse_qs, urlsplit

# Server-side record of the slow static module fetch, keyed by a per-run query token.
# Each entry holds monotonic times of slow-module request starts/releases and of probe
# requests sent by a document timer, so the smoke test can prove timer progress while
# the response is still being withheld (not merely before the module evaluated).
SLOW_LOCK = threading.Lock()
SLOW_FETCHES = {}
TOKEN = re.compile(r"[A-Za-z0-9-]{1,64}")


def new_slow_state():
    return {"starts": [], "ends": [], "probes": []}


def slow_token(path):
    values = parse_qs(urlsplit(path).query).get("token", [])
    if len(values) != 1 or not TOKEN.fullmatch(values[0]):
        return None
    return values[0]


def slow_report(state):
    if state is None:
        return {"error": "unknown token"}
    if len(state["starts"]) != 1 or len(state["ends"]) != 1:
        return {"error": "expected exactly one completed slow-module fetch",
                "starts": len(state["starts"]), "ends": len(state["ends"])}
    start, end = state["starts"][0], state["ends"][0]
    during = sum(1 for probe in state["probes"] if start < probe < end)
    return {"fetch_hold_ms": (end - start) * 1000, "probes_during_fetch": during,
            "probes_total": len(state["probes"])}

class Handler(http.server.SimpleHTTPRequestHandler):
    # Explicit fixture types precede mimetypes' platform/Windows registry database.
    # Keep the wrong-MIME route below deliberate; do not relax engine MIME validation.
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".json": "application/json",
    }

    def do_GET(self):
        route = urlsplit(self.path).path
        if route == "/redirect-module.mjs":
            self.send_response(302)
            self.send_header("Location", "/single-document-fixtures/redirect/entry.mjs")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if route in ("/wrong-mime-module.mjs", "/sri-module.mjs", "/no-referrer-parent.mjs"):
            source = b"export const ready = true;\n"
            if route == "/sri-module.mjs":
                source = b"globalThis.rovesSriExecuted = (globalThis.rovesSriExecuted || 0) + 1; export const ready = true;\n"
            elif route == "/no-referrer-parent.mjs":
                source = b"import referrer from './module-referrer-probe.mjs?policy-none'; export default referrer;\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain" if route == "/wrong-mime-module.mjs" else "text/javascript")
            self.send_header("Content-Length", str(len(source)))
            if route == "/no-referrer-parent.mjs":
                self.send_header("Referrer-Policy", "no-referrer")
            self.end_headers()
            self.wfile.write(source)
            return
        if route in ("/cors-module.mjs", "/no-cors-module.mjs"):
            source = b"export const ready = true;\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript")
            self.send_header("Content-Length", str(len(source)))
            if route == "/cors-module.mjs":
                self.send_header("Access-Control-Allow-Origin", "http://127.0.0.1:18765")
            self.end_headers()
            self.wfile.write(source)
            return
        if route in ("/slow-fetch-probe", "/slow-fetch-report"):
            token = slow_token(self.path)
            if token is None:
                self.send_error(400, "missing or invalid token")
                return
            with SLOW_LOCK:
                if route == "/slow-fetch-probe":
                    # Arrival time at the server: a probe recorded inside the hold window
                    # was sent by the document before the module response was released.
                    SLOW_FETCHES.setdefault(token, new_slow_state())["probes"].append(time.monotonic())
                    body = None
                else:
                    state = SLOW_FETCHES.pop(token, None)
                    body = slow_report(state)
            if body is None:
                self.send_response(204)
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                return
            payload = json.dumps(body).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        if route == "/slow-module.mjs":
            # Only token-tagged requests (the runtime pause scene) are tracked; the
            # untagged startup fixture keeps the plain delay.
            token = slow_token(self.path)
            if token is not None:
                with SLOW_LOCK:
                    SLOW_FETCHES.setdefault(token, new_slow_state())["starts"].append(time.monotonic())
            # A fixed test-server delay, not a product latency threshold.
            time.sleep(0.25)
            if token is not None:
                # Closed before any response byte is written: later probes do not count.
                with SLOW_LOCK:
                    SLOW_FETCHES.setdefault(token, new_slow_state())["ends"].append(time.monotonic())
            source = b"export const ready = true;\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript")
            self.send_header("Content-Length", str(len(source)))
            self.end_headers()
            self.wfile.write(source)
            return
        if urlsplit(self.path).path == "/module-referrer-probe.mjs":
            source = ("export default " + json.dumps(self.headers.get("Referer")) + ";\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript")
            self.send_header("Content-Length", str(len(source)))
            self.end_headers()
            self.wfile.write(source)
            return
        super().do_GET()

if __name__ == "__main__":
    handler = functools.partial(Handler, directory=sys.argv[2])
    with http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), handler) as server:
        server.serve_forever()
