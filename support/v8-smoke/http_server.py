#!/usr/bin/env python3
"""Loopback smoke fixtures using the standard-library HTTP server."""
import functools
import http.server
import json
import sys
import time
from urllib.parse import urlsplit

class Handler(http.server.SimpleHTTPRequestHandler):
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
        if urlsplit(self.path).path == "/slow-module.mjs":
            # A fixed test-server delay, not a product latency threshold.
            time.sleep(0.25)
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
