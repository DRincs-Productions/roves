#!/usr/bin/env bash
# Deterministic checks for xvfb_window_cycle_smoke.sh with fake xvfb-run/xdotool and a fake
# application process: no X server, browser build or display is needed.
set -u

here=$(cd "$(dirname "$0")" && pwd)
script=${XVFB_WINDOW_CYCLE_SCRIPT:-"$here/xvfb_window_cycle_smoke.sh"}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"

# xvfb-run: drop its options and run the command on a fake private display.
cat > "$work/bin/xvfb-run" <<'EOF'
#!/usr/bin/env bash
while [ "${1#-}" != "$1" ]; do shift; done
DISPLAY=:99 exec "$@"
EOF

# xdotool: the fake application publishes its window by writing its pid to $FAKE/window.pid.
# FAKE_NO_NET_WM_PID hides it from --pid searches; FAKE_UNRELATED is another client's window.
cat > "$work/bin/xdotool" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$FAKE/xdotool.log"
[ "$1" = search ] || exit 0
owner=$(cat "$FAKE/window.pid" 2>/dev/null || true)
case "$*" in
  *--pid*)
    [ -z "${FAKE_NO_NET_WM_PID:-}" ] && [ -n "$owner" ] && [ "$owner" = "${*: -1}" ] && echo 4242
    ;;
  *--name*)
    [ -n "${FAKE_UNRELATED:-}" ] && echo "$FAKE_UNRELATED"
    [ -n "$owner" ] && echo 4242
    ;;
esac
exit 0
EOF

# Fake application: maps its window after APP_WINDOW_DELAY seconds (never when empty), then
# stays alive APP_LIFETIME seconds. APP_IGNORE_TERM mimics an event loop that ignores SIGTERM.
cat > "$work/app" <<'EOF'
#!/usr/bin/env bash
[ -n "${APP_IGNORE_TERM:-}" ] && trap '' TERM
echo $$ > "$FAKE/app.pid"
[ -n "${APP_EXIT:-}" ] && exit "$APP_EXIT"
if [ -n "${APP_WINDOW_DELAY:-}" ]; then
  sleep "$APP_WINDOW_DELAY"
  echo $$ > "$FAKE/window.pid"
fi
sleep "${APP_LIFETIME:-30}"
EOF
chmod +x "$work/bin/xvfb-run" "$work/bin/xdotool" "$work/app"

failures=0
run_case() {
  name=$1 expect=$2 pattern=$3 max_seconds=$4
  shift 4
  export FAKE="$work/$name"
  mkdir -p "$FAKE"
  start=$SECONDS
  env PATH="$work/bin:$PATH" "$@" "$script" "$work/app" > "$FAKE/out.log" 2>&1
  status=$?
  elapsed=$((SECONDS - start))
  ok=true
  if [ "$expect" = pass ]; then
    [ "$status" -eq 0 ] || ok=false
    grep -q '\[roves-xvfb-window-cycle\] ok window=4242' "$FAKE/out.log" || ok=false
  else
    [ "$status" -ne 0 ] || ok=false
    grep -q '\[roves-xvfb-window-cycle\] ok' "$FAKE/out.log" && ok=false
  fi
  grep -q -- "$pattern" "$FAKE/out.log" || ok=false
  [ "$elapsed" -le "$max_seconds" ] || ok=false
  app_pid=$(cat "$FAKE/app.pid" 2>/dev/null || true)
  if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
    ok=false
    kill -9 "$app_pid" 2>/dev/null
  fi
  if [ "$ok" = true ]; then
    echo "PASS $name (status=$status, ${elapsed}s)"
  else
    echo "FAIL $name (status=$status, ${elapsed}s)"
    sed 's/^/    /' "$FAKE/out.log"
    failures=$((failures + 1))
  fi
}

# The CI failure: the window maps after the old fixed 10 s probe, within the shared deadline.
run_case slow-window pass 'ok window=4242' 35 \
  DISPLAY= ROVES_XVFB_READY_TIMEOUT=30 APP_WINDOW_DELAY=20 APP_LIFETIME=3
run_case app-exited fail 'exited before a visible SDL3 window' 5 \
  DISPLAY= APP_EXIT=3
run_case no-window fail 'no visible SDL3 window for pid' 6 \
  DISPLAY= ROVES_XVFB_READY_TIMEOUT=2
# A pre-existing display may hold other clients' windows: never fall back to a name search there.
run_case shared-display-unrelated-window fail 'no visible SDL3 window for pid' 6 \
  DISPLAY=:5 ROVES_XVFB_READY_TIMEOUT=2 FAKE_UNRELATED=777 FAKE_NO_NET_WM_PID=1 \
  APP_WINDOW_DELAY=0 APP_LIFETIME=30
run_case shared-display-own-window pass 'ok window=4242' 10 \
  DISPLAY=:5 ROVES_XVFB_READY_TIMEOUT=5 FAKE_UNRELATED=777 APP_WINDOW_DELAY=0 APP_LIFETIME=3
run_case private-display-without-net-wm-pid pass 'ok window=4242' 10 \
  DISPLAY= ROVES_XVFB_READY_TIMEOUT=5 FAKE_NO_NET_WM_PID=1 APP_WINDOW_DELAY=1 APP_LIFETIME=3
# Cleanup must not wait forever on a process that ignores SIGTERM.
run_case term-ignored fail 'no visible SDL3 window for pid' 12 \
  DISPLAY= ROVES_XVFB_READY_TIMEOUT=2 APP_IGNORE_TERM=1

[ "$failures" -eq 0 ] || { echo "$failures case(s) failed"; exit 1; }
echo "all cases passed"
