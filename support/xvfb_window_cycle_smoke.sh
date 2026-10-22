#!/usr/bin/env bash
set -eu

# Re-enter under a private display once. Keeping the probe and the application in the same
# xvfb-run process makes DISPLAY available to xdotool as well as SDL3. The sentinel argument
# records that this script owns the display, which is the only case where a visible window
# without _NET_WM_PID can be attributed to the application.
private_display=0
if [ "${1:-}" = "--roves-private-display" ]; then
  private_display=1
  shift
elif [ -z "${DISPLAY:-}" ]; then
  exec xvfb-run -a --server-args="-screen 0 1280x720x24" "$0" --roves-private-display "$@"
fi

# One deadline for the window and the page's input listeners, kept below the 75 s the workflow
# gives the whole smoke step: a debug build on a loaded runner can take well over 10 s to map
# its first window.
deadline=$((SECONDS + ${ROVES_XVFB_READY_TIMEOUT:-60}))

"$@" &
app_pid=$!
cleanup() {
  kill "$app_pid" 2>/dev/null || true
  # SDL3 turns SIGTERM into a quit event that a busy main thread may never process: do not let
  # the wrapper (and with it the workflow step) wait on it indefinitely.
  for _ in 1 2 3 4 5; do
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 1
  done
  kill -9 "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 1' INT TERM

window_id=""
while [ "$SECONDS" -lt "$deadline" ]; do
  kill -0 "$app_pid" 2>/dev/null || break
  window_id=$(xdotool search --onlyvisible --pid "$app_pid" 2>/dev/null | head -1 || true)
  if [ -z "$window_id" ] && [ "$private_display" = 1 ]; then
    # Some X11 backends omit _NET_WM_PID. The private Xvfb has no unrelated application
    # windows, so its only visible titled window is the application's.
    window_id=$(xdotool search --onlyvisible --name . 2>/dev/null | head -1 || true)
  fi
  [ -n "$window_id" ] && break
  sleep 0.2
done

if [ -z "$window_id" ]; then
  if kill -0 "$app_pid" 2>/dev/null; then
    echo "[roves-xvfb-window-cycle] no visible SDL3 window for pid $app_pid after ${SECONDS}s" >&2
  else
    echo "[roves-xvfb-window-cycle] pid $app_pid exited before a visible SDL3 window appeared" >&2
  fi
  exit 1
fi

# Wait for the page's input listeners (test-page InputPanel logs "[roves-input] ready" to stdout)
# when the caller says where stdout goes, so the synthetic input below reaches the page instead of
# racing its load. Without ROVES_INPUT_READY_LOG this keeps the original window-only behavior.
if [ -n "${ROVES_INPUT_READY_LOG:-}" ]; then
  while [ "$SECONDS" -lt "$deadline" ]; do
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
xdotool key --window "$window_id" ctrl+c ctrl+x ctrl+v
xdotool windowsize "$window_id" 960 640
sleep 1
kill -0 "$app_pid"
echo "[roves-xvfb-window-cycle] ok window=$window_id"

wait "$app_pid"
