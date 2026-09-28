#!/usr/bin/env bash
set -eu

# Re-enter under a private display once. Keeping the probe and the application in the same
# xvfb-run process makes DISPLAY available to xdotool as well as SDL3.
if [ -z "${DISPLAY:-}" ]; then
  exec xvfb-run -a --server-args="-screen 0 1280x720x24" "$0" "$@"
fi

"$@" &
app_pid=$!
cleanup() {
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

window_id=""
for _ in $(seq 1 50); do
  kill -0 "$app_pid" 2>/dev/null || break
  window_id=$(xdotool search --onlyvisible --pid "$app_pid" 2>/dev/null | head -1 || true)
  [ -n "$window_id" ] && break
  sleep 0.2
done

if [ -z "$window_id" ]; then
  # Some X11 backends omit _NET_WM_PID. The private Xvfb has no unrelated application windows,
  # so falling back to its only visible titled window remains deterministic.
  window_id=$(xdotool search --onlyvisible --name . 2>/dev/null | head -1 || true)
  if [ -z "$window_id" ]; then
    echo "[roves-xvfb-window-cycle] no visible SDL3 window for pid $app_pid" >&2
    exit 1
  fi
fi

# Wait for the page's input listeners (test-page InputPanel logs "[roves-input] ready" to stdout)
# when the caller says where stdout goes, so the synthetic input below reaches the page instead of
# racing its load. Without ROVES_INPUT_READY_LOG this keeps the original window-only behavior.
if [ -n "${ROVES_INPUT_READY_LOG:-}" ]; then
  for _ in $(seq 1 100); do
    grep -q '\[roves-input\] ready' "$ROVES_INPUT_READY_LOG" 2>/dev/null && break
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.2
  done
fi

# Exercise real X11/SDL3 delivery without depending on pixels. These operations cover pointer
# movement/click, wheel, keyboard focus/input and a resize/redraw cycle; with the test page loaded,
# each one also produces a "[roves-input]" line the workflow asserts on.
# Clicks go through XTEST (no --window): with --window xdotool uses XSendEvent, whose synthetic
# button events SDL3 ignores (keys still arrive that way).
xdotool mousemove --window "$window_id" 40 40
xdotool click 1
xdotool click 5
xdotool key --window "$window_id" Tab a Escape
xdotool windowsize "$window_id" 960 640
sleep 1
kill -0 "$app_pid"
echo "[roves-xvfb-window-cycle] ok window=$window_id"

wait "$app_pid"
