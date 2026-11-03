"""Helper and harness-flow tests for window_close_pending_fetch.py (no servoshell needed).

    python -m unittest discover -s support/v8-smoke -p test_window_close_pending_fetch.py

A fake child process stands in for servoshell and a fake Win32 for the native window, so the
ordering contract (request held -> WM_CLOSE posted -> response released -> clean exit) and the
failure paths are checked on every OS."""
import subprocess
import sys
import tempfile
import threading
import unittest
import urllib.request
from pathlib import Path
from unittest import mock

import window_close_pending_fetch as harness

# Fake servoshell: requests the held dependency like the fixture's static import, then exits.
FAKE_CHILD = """
import sys, urllib.request
base = sys.argv[-1].rsplit('/', 1)[0]
html = urllib.request.urlopen(sys.argv[-1], timeout=30).read()
assert b'window-close-pending-fetch.mjs' in html
urllib.request.urlopen(base + '/window-close-held.mjs', timeout=60).read()
print(sys.argv[1] if len(sys.argv) > 2 and sys.argv[1].startswith('MARK:') else 'fake exit')
"""


class FakeWin32:
    def __init__(self, state_holder, title=harness.WINDOW_TITLE, visible=True):
        self.state_holder = state_holder
        self.title = title
        self.visible = visible
        self.pid = None
        self.posted = []

    def windows(self):
        return [(0x20, self.pid + 1, harness.WINDOW_TITLE, True),
                (0x10, self.pid, self.title, self.visible)]

    def post_close(self, hwnd):
        # Records whether the response was still held when WM_CLOSE was posted.
        self.posted.append((hwnd, self.state_holder["release_set"]()))


def fake_launch(win32, extra=()):
    def launch(argv, **kwargs):
        assert "-z" not in argv and "--window-title" in argv
        proc = subprocess.Popen([sys.executable, "-c", FAKE_CHILD, *extra, argv[-1]], **kwargs)
        win32.pid = proc.pid
        return proc
    return launch


class SelectWindowTests(unittest.TestCase):
    def test_picks_only_visible_exact_title_of_the_process(self):
        windows = [(1, 7, harness.WINDOW_TITLE, True), (2, 8, harness.WINDOW_TITLE, False),
                   (3, 8, "Roves", True), (4, 8, harness.WINDOW_TITLE, True)]
        hwnd, seen = harness.select_window(windows, 8, harness.WINDOW_TITLE)
        self.assertEqual(4, hwnd)
        self.assertIn("'Roves'", seen)

    def test_miss_and_ambiguity_return_none_with_diagnostics(self):
        self.assertEqual((None, "none"), harness.select_window([(1, 7, "x", True)], 8, "x"))
        hwnd, _ = harness.select_window([(1, 8, "x", True), (2, 8, "x", True)], 8, "x")
        self.assertIsNone(hwnd)


class MarkerTests(unittest.TestCase):
    def test_markers(self):
        text = ("ok\nWARN Panic hook called.\nthread 'Script' panicked at a.rs:1\n"
                "Script#1: spurious wake-up in messaging.rs:447\nfine\n")
        self.assertEqual(3, len(harness.scan_failure_markers(text)))
        self.assertEqual([], harness.scan_failure_markers("clean shutdown\n"))


class ServerTests(unittest.TestCase):
    def test_holds_until_release_and_serves_fixture_types(self):
        state = harness.HoldState()
        server = harness.start_server(state)
        base = f"http://127.0.0.1:{server.server_address[1]}"
        try:
            with urllib.request.urlopen(base + "/" + harness.FIXTURE, timeout=10) as response:
                self.assertIn("text/html", response.headers["Content-Type"])
            with urllib.request.urlopen(base + "/window-close-pending-fetch.mjs", timeout=10) as response:
                self.assertEqual("text/javascript", response.headers["Content-Type"])
                self.assertIn(b"/window-close-held.mjs", response.read())
            result = {}
            fetcher = threading.Thread(target=lambda: result.setdefault(
                "body", urllib.request.urlopen(base + harness.HELD_ROUTE, timeout=30).read()))
            fetcher.start()
            self.assertTrue(state.requested.wait(10))
            fetcher.join(0.3)
            self.assertTrue(fetcher.is_alive(), "held response was sent before the release")
            state.release.set()
            fetcher.join(10)
            self.assertEqual(harness.HELD_SOURCE, result["body"])
            self.assertEqual(1, state.held_requests)
        finally:
            harness.stop_server(server, state)


class RunFlowTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.holder = {"release_set": lambda: None}
        real_state = harness.HoldState

        def tracked_state():
            state = real_state()
            self.holder["release_set"] = state.release.is_set
            return state
        patcher = mock.patch.object(harness, "HoldState", tracked_state)
        patcher.start()
        self.addCleanup(patcher.stop)

    def run_harness(self, win32, extra=()):
        return harness.run(Path("servoshell.exe"), Path(self.tmp.name), win32, fake_launch(win32, extra))

    def test_close_is_posted_while_held_then_clean_exit(self):
        win32 = FakeWin32(self.holder)
        errors, notices, state, _, _ = self.run_harness(win32)
        self.assertEqual([], errors)
        self.assertEqual([(0x10, False)], win32.posted)
        self.assertEqual("delivered", state.held_outcome)
        self.assertTrue(any("exit code 0" in notice for notice in notices))

    def test_panic_marker_fails(self):
        errors, *_ = self.run_harness(FakeWin32(self.holder), ["MARK:x panicked at y.rs:1"])
        self.assertTrue(any("failure markers" in error for error in errors), errors)

    def test_missing_window_fails_without_posting_and_kills_child(self):
        win32 = FakeWin32(self.holder, title="Roves")
        launched = []
        launch = fake_launch(win32)

        def tracking_launch(argv, **kwargs):
            launched.append(launch(argv, **kwargs))
            return launched[-1]
        with mock.patch.object(harness, "WINDOW_TIMEOUT", 0.5):
            errors, *_ = harness.run(Path("servoshell.exe"), Path(self.tmp.name), win32, tracking_launch)
        self.assertTrue(any("no single visible window" in error for error in errors), errors)
        self.assertEqual([], win32.posted)
        self.assertIsNotNone(launched[0].poll())

    def test_child_exit_before_request_fails(self):
        win32 = FakeWin32(self.holder)

        def exiting_launch(argv, **kwargs):
            proc = subprocess.Popen([sys.executable, "-c", "pass"], **kwargs)
            win32.pid = proc.pid
            return proc
        errors, *_ = harness.run(Path("servoshell.exe"), Path(self.tmp.name), win32, exiting_launch)
        self.assertTrue(any("never reached the test server" in error for error in errors), errors)
        self.assertEqual([], win32.posted)


if __name__ == "__main__":
    unittest.main()
