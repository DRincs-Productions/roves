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
