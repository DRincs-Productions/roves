"""Helper and harness-flow tests for window_close_pending_fetch.py (no servoshell needed).

    python -m unittest discover -s support/v8-smoke -p test_window_close_pending_fetch.py

A fake child process stands in for servoshell and a fake Win32 for the native window, so the
ordering contract (request held -> WM_CLOSE posted -> exit observed with the response still
unreleased and the dependent module not evaluated -> release) and the failure paths are checked
on every OS, Windows included."""
import subprocess
import sys
import tempfile
import threading
import unittest
import urllib.request
from pathlib import Path
from unittest import mock

import window_close_pending_fetch as harness

# Fake servoshell: requests the held dependency like the fixture's static import (in a daemon
# thread, never reading a reply), waits for the fake WM_CLOSE (a file created by FakeWin32), then
# exits 0. Optional flags: MARK:<text> printed at exit, EVALUATE reports evaluation before exit.
FAKE_CHILD = """
import os, sys, threading, time, urllib.request
from pathlib import Path
args = sys.argv[1:]
url, close_file = args[-1], Path(args[-2])
base = url.rsplit('/', 1)[0]
html = urllib.request.urlopen(url, timeout=30).read()
assert b'window-close-pending-fetch.mjs' in html
threading.Thread(daemon=True, target=lambda: urllib.request.urlopen(
    base + '/window-close-held.mjs', timeout=120).read()).start()
deadline = time.monotonic() + 60
while not close_file.exists():
    if time.monotonic() > deadline:
        sys.exit(3)
    time.sleep(0.05)
if 'EVALUATE' in args:
    urllib.request.urlopen(base + '/window-close-evaluated', timeout=10).read()
print(next((a for a in args if a.startswith('MARK:')), 'fake exit'), flush=True)
os._exit(0)
"""


class FakeWin32:
    def __init__(self, state_holder, close_file, title=harness.WINDOW_TITLE, visible=True,
                 honor_close=True):
        self.state_holder = state_holder
        self.close_file = close_file
        self.title = title
        self.visible = visible
        self.honor_close = honor_close
        self.pid = None
        self.posted = []

    def windows(self):
        return [(0x20, self.pid + 1, harness.WINDOW_TITLE, True),
                (0x10, self.pid, self.title, self.visible)]

    def post_close(self, hwnd):
        # Records whether the response was still held when WM_CLOSE was posted.
        self.posted.append((hwnd, self.state_holder["release_set"]()))
        if self.honor_close:
            self.close_file.touch()


def fake_launch(win32, holder, extra=()):
    def launch(argv, **kwargs):
        assert "-z" not in argv and "--window-title" in argv
        proc = subprocess.Popen(
            [sys.executable, "-c", FAKE_CHILD, *extra, str(win32.close_file), argv[-1]], **kwargs)
        win32.pid = proc.pid
        holder["procs"].append(proc)
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


class HoldStateTests(unittest.TestCase):
    def test_record_exit_snapshots_release_and_evaluation(self):
        state = harness.HoldState()
        state.record_exit()
        self.assertEqual((True, False), (state.unreleased_at_exit, state.evaluated_at_exit))
        state.evaluated.set()
        state.release.set()
        state.record_exit()
        self.assertEqual((False, True), (state.unreleased_at_exit, state.evaluated_at_exit))
        state = harness.HoldState()
        state.held_outcome = "delivered"
        state.record_exit()
        self.assertFalse(state.unreleased_at_exit)


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


class RecordingEvent(threading.Event):
    """Release event that records, at each set(), whether every fake child had already exited."""

    def __init__(self, holder):
        super().__init__()
        self.holder = holder

    def set(self):
        procs = self.holder["procs"]
        self.holder["release_after_exit"].append(
            bool(procs) and all(proc.poll() is not None for proc in procs))
        super().set()


class RunFlowTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.close_file = Path(self.tmp.name) / "fake-wm-close"
        self.holder = {"release_set": lambda: None, "procs": [], "release_after_exit": []}
        real_state = harness.HoldState

        def tracked_state():
            state = real_state()
            state.release = RecordingEvent(self.holder)
            self.holder["release_set"] = state.release.is_set
            self.holder["state"] = state
            return state
        patcher = mock.patch.object(harness, "HoldState", tracked_state)
        patcher.start()
        self.addCleanup(patcher.stop)

    def win32(self, **kwargs):
        return FakeWin32(self.holder, self.close_file, **kwargs)

    def run_harness(self, win32, extra=()):
        return harness.run(Path("servoshell.exe"), Path(self.tmp.name), win32,
                           fake_launch(win32, self.holder, extra))

    def test_exit_is_observed_while_held_then_released(self):
        win32 = self.win32()
        errors, notices, state, _, _ = self.run_harness(win32)
        self.assertEqual([], errors)
        self.assertEqual([(0x10, False)], win32.posted)
        self.assertIs(True, state.unreleased_at_exit)
        self.assertIs(False, state.evaluated_at_exit)
        # Every release (post-exit and the final cleanup one) happened after the child exited.
        self.assertTrue(self.holder["release_after_exit"])
        self.assertTrue(all(self.holder["release_after_exit"]), self.holder["release_after_exit"])
        self.assertTrue(any("exit code 0" in notice for notice in notices))
        self.assertTrue(any("unreleased=True" in notice for notice in notices), notices)

    def test_evaluation_before_exit_fails(self):
        errors, _, state, _, _ = self.run_harness(self.win32(), ["EVALUATE"])
        self.assertIs(True, state.evaluated_at_exit)
        self.assertIs(True, state.unreleased_at_exit)
        self.assertTrue(any("dependent module evaluated" in error for error in errors), errors)

    def test_release_before_exit_fails(self):
        # A regression that lets the response go before the exit must not pass.
        win32 = self.win32()
        post_close = win32.post_close

        def releasing_close(hwnd):
            # Make the simulated regression deterministic: release before signaling the fake
            # process to exit in response to WM_CLOSE.
            self.holder["state"].release.set()
            post_close(hwnd)
        win32.post_close = releasing_close
        errors, _, state, _, _ = self.run_harness(win32)
        self.assertIs(False, state.unreleased_at_exit)
        self.assertFalse(self.holder["release_after_exit"][0])
        self.assertTrue(any("no longer held" in error for error in errors), errors)

    def test_panic_marker_fails(self):
        errors, *_ = self.run_harness(self.win32(), ["MARK:x panicked at y.rs:1"])
        self.assertTrue(any("failure markers" in error for error in errors), errors)

    def test_exit_timeout_fails_then_releases_and_reaps(self):
        win32 = self.win32(honor_close=False)
        with mock.patch.object(harness, "EXIT_TIMEOUT", 0.5):
            errors, _, state, _, _ = self.run_harness(win32)
        self.assertTrue(any("did not exit within" in error for error in errors), errors)
        self.assertEqual([(0x10, False)], win32.posted)
        self.assertIsNone(state.unreleased_at_exit)  # no exit was observed
        self.assertTrue(state.release.is_set())
        self.assertIsNotNone(self.holder["procs"][0].poll())

    def test_missing_window_fails_without_posting_and_kills_child(self):
        win32 = self.win32(title="Roves")
        with mock.patch.object(harness, "WINDOW_TIMEOUT", 0.5):
            errors, *_ = self.run_harness(win32)
        self.assertTrue(any("no single visible window" in error for error in errors), errors)
        self.assertEqual([], win32.posted)
        self.assertIsNotNone(self.holder["procs"][0].poll())

    def test_child_exit_before_request_fails(self):
        win32 = self.win32()

        def exiting_launch(argv, **kwargs):
            proc = subprocess.Popen([sys.executable, "-c", "pass"], **kwargs)
            win32.pid = proc.pid
            return proc
        errors, *_ = harness.run(Path("servoshell.exe"), Path(self.tmp.name), win32, exiting_launch)
        self.assertTrue(any("never reached the test server" in error for error in errors), errors)
        self.assertEqual([], win32.posted)


if __name__ == "__main__":
    unittest.main()
