#!/usr/bin/env python3
"""Windows smoke: close the real ServoShell window while a static module fetch is held.

    python support/v8-smoke/window_close_pending_fetch.py path/to/servoshell.exe

A loopback server (stdlib, reusing http_server.Handler's fixture MIME map) serves
window-close-pending-fetch.html, whose static module imports /window-close-held.mjs. The server
holds that response. Once the request has arrived and the headed servoshell process owns a
visible native window with the fixed --window-title, the harness posts WM_CLOSE to that window
(no mouse/keyboard input, so no unlocked desktop is needed), then releases the held response so
shutdown races with the pending fetch. The process must exit with code 0 within a bound, without
a panic or the former spurious wake-up failure in its output or native log. The child process and
the server are always cleaned up. Bounds below are harness limits, not product latency targets.
"""
import argparse
import functools
import http.server
import os
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from urllib.parse import urlsplit

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))
import http_server  # noqa: E402  (fixture MIME map shared with the other smoke pages)

FIXTURE = "window-close-pending-fetch.html"
HELD_ROUTE = "/window-close-held.mjs"
EVALUATED_ROUTE = "/window-close-evaluated"
WINDOW_TITLE = "Roves window-close pending-fetch smoke"
HELD_SOURCE = b"export const held = true;\n"
# Panic hook output (panic_hook.rs), Rust's default panic text, and the task-queue expect()
# that patch 0437 removed for wakes outliving their task while a static fetch is pending.
FAILURE_MARKERS = ("Panic hook called", "panicked at", "Spurious wake-up")
WM_CLOSE = 0x0010

REQUEST_TIMEOUT = 180.0  # debug build cold start on a CI runner, through to the held request
WINDOW_TIMEOUT = 60.0
EXIT_TIMEOUT = 60.0
HOLD_LIMIT = 300.0  # the server never holds forever, even if the harness itself dies


class HoldState:
    """What the loopback server observed; read by the harness and the unit tests."""

    def __init__(self):
        self.lock = threading.Lock()
        self.requested = threading.Event()
        self.release = threading.Event()
        self.evaluated = threading.Event()
        self.held_requests = 0
        self.held_outcome = None  # set after the release: delivered, or the socket error
        self.log = []

    def note(self, line):
        with self.lock:
            self.log.append(f"{time.monotonic():.3f} {line}")


class HoldHandler(http_server.Handler):
    state = None  # bound per server by start_server

    def do_GET(self):
        route = urlsplit(self.path).path
        if route == HELD_ROUTE:
            state = self.state
            with state.lock:
                state.held_requests += 1
            state.note(f"held request {self.path}")
            state.requested.set()
            released = state.release.wait(HOLD_LIMIT)
            try:
                self.send_response(200)
                self.send_header("Content-Type", "text/javascript")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(HELD_SOURCE)))
                self.end_headers()
                self.wfile.write(HELD_SOURCE)
                self.wfile.flush()
                outcome = "delivered" if released else "delivered after hold limit"
            except OSError as error:
                outcome = f"client gone: {type(error).__name__}: {error}"
            state.held_outcome = outcome
            state.note(f"held response {outcome}")
            return
        if route == EVALUATED_ROUTE:
            self.state.note(f"evaluated report {self.path}")
            self.state.evaluated.set()
            self.send_response(204)
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            return
        super().do_GET()

    def log_message(self, format, *args):
        self.state.note("http " + (format % args))


def start_server(state, directory=DIR):
    handler = type("BoundHoldHandler", (HoldHandler,), {"state": state})
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(handler, directory=str(directory)))
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, name="hold-server", daemon=True)
    thread.start()
    return server


def stop_server(server, state):
    state.release.set()
    server.shutdown()
    server.server_close()


def wait_event(event, proc, timeout, step=0.25):
    """Waits for `event`, returning early (False) when `proc` exits or the bound expires."""
    deadline = time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return event.is_set()
        if event.wait(min(step, remaining)):
            return True
        if proc.poll() is not None:
            return event.is_set()


def select_window(windows, pid, title):
    """Picks the visible top-level window of `pid` titled exactly `title`.

    `windows` holds (hwnd, pid, title, visible) tuples. Returns (hwnd or None, description of
    the process's windows) so a miss is diagnosable."""
    own = [w for w in windows if w[1] == pid]
    matches = [w for w in own if w[3] and w[2] == title]
    seen = "; ".join(f"{w[0]:#x} visible={bool(w[3])} title={w[2]!r}" for w in own) or "none"
    return (matches[0][0] if len(matches) == 1 else None), seen


def scan_failure_markers(text):
    markers = tuple(marker.casefold() for marker in FAILURE_MARKERS)
    return [line.strip() for line in text.splitlines()
            if any(marker in line.casefold() for marker in markers)]


class Win32:
    """The user32 calls the harness needs, through ctypes (no extra dependency)."""

    def __init__(self):
        import ctypes
        from ctypes import wintypes
        self.ctypes = ctypes
        self.wintypes = wintypes
        self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        self.enum_proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        self.user32.EnumWindows.argtypes = [self.enum_proc, wintypes.LPARAM]
        self.user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        self.user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
        self.user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
        self.user32.IsWindowVisible.argtypes = [wintypes.HWND]
        self.user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        self.user32.PostMessageW.restype = wintypes.BOOL

    def windows(self):
        found = []

        def visit(hwnd, _):
            pid = self.wintypes.DWORD()
            self.user32.GetWindowThreadProcessId(hwnd, self.ctypes.byref(pid))
            length = self.user32.GetWindowTextLengthW(hwnd)
            buffer = self.ctypes.create_unicode_buffer(length + 1)
            self.user32.GetWindowTextW(hwnd, buffer, length + 1)
            found.append((hwnd or 0, pid.value, buffer.value, bool(self.user32.IsWindowVisible(hwnd))))
            return True

        self.user32.EnumWindows(self.enum_proc(visit), 0)
        return found

    def post_close(self, hwnd):
        if not self.user32.PostMessageW(hwnd, WM_CLOSE, 0, 0):
            raise OSError(self.ctypes.get_last_error(), "PostMessageW(WM_CLOSE) failed")


def find_window(win32, proc, timeout, step=0.2):
    """Bounded wait for the fixed-title window; a top-level window has no creation event to wait on."""
    deadline = time.monotonic() + timeout
    seen = "none"
    while time.monotonic() < deadline and proc.poll() is None:
        hwnd, seen = select_window(win32.windows(), proc.pid, WINDOW_TITLE)
        if hwnd:
            return hwnd, seen
        time.sleep(step)
    return None, seen


def tail(path, lines=40):
    try:
        return path.read_text(encoding="utf-8", errors="replace").splitlines()[-lines:]
    except OSError as error:
        return [f"<unreadable {path}: {error}>"]


def run(binary, runtime_root, win32=None, launch=subprocess.Popen):
    """`win32` and `launch` are replaceable only for the unit tests' fake process/window."""
    runtime_root.mkdir(parents=True, exist_ok=True)
    runtime = Path(tempfile.mkdtemp(prefix="window-close-pending-fetch.", dir=runtime_root))
    output_path = runtime / "servoshell-output.log"
    native_log = runtime / "roves" / "roves.log"
    errors, notices = [], []
    state = HoldState()
    win32 = win32 or Win32()
    server = start_server(state)
    url = f"http://127.0.0.1:{server.server_address[1]}/{FIXTURE}"
    proc = None
    try:
        env = dict(os.environ, XDG_CACHE_HOME=str(runtime))
        # Headed on purpose (no -z): the native window is what gets closed.
        argv = [str(binary), "--hard-fail", "--window-title", WINDOW_TITLE, url]
        with open(output_path, "wb") as output:
            proc = launch(argv, stdout=output, stderr=subprocess.STDOUT, env=env)
        started = time.monotonic()
        if not wait_event(state.requested, proc, REQUEST_TIMEOUT):
            errors.append(f"the static module request {HELD_ROUTE} never reached the test server "
                          f"(exit code {proc.poll()}, {REQUEST_TIMEOUT:.0f} s bound)")
            return errors, notices, state, output_path, native_log
        notices.append(f"held static module request after {time.monotonic() - started:.1f} s")
        hwnd, seen = find_window(win32, proc, WINDOW_TIMEOUT)
        if not hwnd:
            errors.append(f"no single visible window titled {WINDOW_TITLE!r} for pid {proc.pid} "
                          f"(exit code {proc.poll()}; process windows: {seen})")
            return errors, notices, state, output_path, native_log
        if state.release.is_set() or state.held_outcome is not None or state.evaluated.is_set():
            errors.append("the static module response was no longer pending before WM_CLOSE")
            return errors, notices, state, output_path, native_log
        try:
            win32.post_close(hwnd)
        except OSError as error:
            errors.append(f"could not post WM_CLOSE to {hwnd:#x}: {error}")
            return errors, notices, state, output_path, native_log
        closed = time.monotonic()
        state.note(f"posted WM_CLOSE to {hwnd:#x}")
        # Release only now: the response and the shutdown race on purpose.
        state.release.set()
        try:
            code = proc.wait(EXIT_TIMEOUT)
        except subprocess.TimeoutExpired:
            errors.append(f"servoshell did not exit within {EXIT_TIMEOUT:.0f} s of WM_CLOSE")
            return errors, notices, state, output_path, native_log
        notices.append(f"exit code {code} {time.monotonic() - closed:.1f} s after WM_CLOSE "
                       f"(diagnostic, not a threshold)")
        if code != 0:
            errors.append(f"servoshell exited with {code} after WM_CLOSE")
        if state.held_requests != 1:
            errors.append(f"expected exactly 1 held module request, saw {state.held_requests}")
        # Server-side completion of the released response, bounded by the server's own write.
        deadline = time.monotonic() + 10
        while state.held_outcome is None and time.monotonic() < deadline:
            time.sleep(0.05)
        notices.append(f"held response after release: {state.held_outcome or 'still writing'}; "
                       f"dependent module evaluated before exit: {state.evaluated.is_set()} "
                       f"(race diagnostics)")
        texts = [output_path.read_text(encoding="utf-8", errors="replace")]
        if native_log.exists():
            texts.append(native_log.read_text(encoding="utf-8", errors="replace"))
        else:
            notices.append(f"native log not found at {native_log}; scanned process output only")
        markers = scan_failure_markers("\n".join(texts))
        if markers:
            errors.append("failure markers in servoshell output/log: " + " | ".join(markers[:10]))
        return errors, notices, state, output_path, native_log
    finally:
        if proc is not None and proc.poll() is None:
            proc.kill()
            try:
                proc.wait(15)
            except subprocess.TimeoutExpired:
                errors.append(f"could not reap servoshell pid {proc.pid} after kill")
        stop_server(server, state)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("servoshell")
    parser.add_argument("--runtime-root", default=str(Path.cwd() / ".v8-smoke-runtime"))
    args = parser.parse_args(argv)
    if sys.platform != "win32":
        print("::error::window_close_pending_fetch.py posts WM_CLOSE and runs only on Windows")
        return 1
    binary = Path(args.servoshell).resolve()
    if not binary.is_file():
        print(f"::error::servoshell binary not found: {binary}")
        return 1
    errors, notices, state, output_path, native_log = run(binary, Path(args.runtime_root))
    for notice in notices:
        print(f"::notice title=Window close with pending fetch::{notice}")
    if errors:
        for error in errors:
            print(f"::error title=Window close with pending fetch::{error}")
        print("Test server events:")
        for line in state.log[-40:]:
            print(f"  {line}")
        for path in (output_path, native_log):
            print(f"Last lines of {path}:")
            for line in tail(path):
                print(f"  {line}")
        return 1
    print("window close with a pending static module fetch: clean exit")
    return 0


if __name__ == "__main__":
    sys.exit(main())
