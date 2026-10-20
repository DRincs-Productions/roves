#!/usr/bin/env bash
# Runs the V8 smoke pages headlessly with a servoshell built on roves-js
# (`python support/v8_cutover_check.py servoshell --build`) and reports each result as a
# GitHub annotation (readable without a token). Fails when a page crashes the browser, when
# the smoke suite does not report its total, or when the basic page checks fail.
#
#     support/v8-smoke/run.sh path/to/servoshell [runner args...]
set -u
MODE="${V8_SMOKE_MODE:-normal}"
case "$MODE" in normal|jitless) ;; *) echo "::error::Invalid V8_SMOKE_MODE: $MODE"; exit 1 ;; esac
PYTHON="${PYTHON3:-python3}"
BIN="$1"
shift
RUNNER_COUNT=$#
RUNNER=("$@")
DIR="$(cd "$(dirname "$0")" && pwd)"
STATUS=0
# Every suite's total, reported as one annotation at the end (at most 10 notices show per step).
TOTALS=""
HTTP_PORT=18765
HTTP_CROSS_ORIGIN_PORT=18766
HTTP_LOG="${TMPDIR:-/tmp}/roves-v8-smoke-http-$$.log"
"$PYTHON" -m http.server "$HTTP_PORT" --bind 127.0.0.1 --directory "$DIR" >"$HTTP_LOG" 2>&1 &
HTTP_PID=$!
HTTP_CROSS_ORIGIN_LOG="${TMPDIR:-/tmp}/roves-v8-smoke-http-cross-origin-$$.log"
"$PYTHON" -m http.server "$HTTP_CROSS_ORIGIN_PORT" --bind 127.0.0.1 --directory "$DIR" >"$HTTP_CROSS_ORIGIN_LOG" 2>&1 &
HTTP_CROSS_ORIGIN_PID=$!
cleanup_http_server() {
  kill "$HTTP_PID" 2>/dev/null || true
  kill "$HTTP_CROSS_ORIGIN_PID" 2>/dev/null || true
  wait "$HTTP_PID" 2>/dev/null || true
  wait "$HTTP_CROSS_ORIGIN_PID" 2>/dev/null || true
  rm -f "$HTTP_LOG"
  rm -f "$HTTP_CROSS_ORIGIN_LOG"
}
trap cleanup_http_server EXIT
http_ready=0
for _ in $(seq 1 50); do
  if "$PYTHON" -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:$HTTP_PORT/cross-origin.html', timeout=0.2).close(); urllib.request.urlopen('http://127.0.0.1:$HTTP_CROSS_ORIGIN_PORT/cross-origin.html', timeout=0.2).close()" 2>/dev/null; then
    http_ready=1
    break
  fi
  sleep 0.1
done
if [ "$http_ready" -ne 1 ]; then
  echo "::error::V8 smoke local HTTP fixture servers did not start on 127.0.0.1:$HTTP_PORT and 127.0.0.1:$HTTP_CROSS_ORIGIN_PORT"
  exit 1
fi

run_page() {
  local page="$1" seconds="$2" log="$3" url="${4:-file://$DIR/$1}"
  # Bash 3.2 treats an empty array as unset under nounset. Do not expand it.
  if [ "$RUNNER_COUNT" -gt 0 ]; then
    timeout "$seconds" "${RUNNER[@]}" "$BIN" -z "$url" > "$log" 2>&1
  else
    timeout "$seconds" "$BIN" -z "$url" > "$log" 2>&1
  fi
  local code=$?
  # 124: the timeout ended the run (pages keep the browser open); anything else but 0 crashed.
  if [ "$code" -ne 0 ] && [ "$code" -ne 124 ]; then
    echo "::error::$page: servoshell exited with $code"
    grep -a -v '^\s*$' "$log" | tail -20 | sed 's/^/::error::  /'
    STATUS=1
  fi
}

run_suite() {
  local page="$1" log="$2" seconds="${3:-60}"
  run_page "$page" "$seconds" "$log"
  # GitHub shows at most 10 annotations of a kind per step: one summary carries every failure.
  local total
  total=$(grep -a "^SUITE TOTAL " "$log" | head -1)
  local validator_args=(--mode "$MODE")
  if [ "${V8_SMOKE_ALLOW_MISSING_DEVICES:-0}" = 1 ]; then validator_args+=(--allow-missing-devices); fi
  if ! "$PYTHON" "$DIR/check_results.py" "$page" "$log" "${validator_args[@]}"; then STATUS=1; fi
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    { echo "## $page: ${total#SUITE TOTAL }"; echo; grep -a "^SUITE " "$log" | sed 's/^/- /'; } >> "$GITHUB_STEP_SUMMARY"
  fi
  if [ -n "$total" ]; then
    TOTALS="$TOTALS%0A$page ${total#SUITE TOTAL }"
  else
    echo "::error::$page did not report its total"
    grep -a -v '^\s*$' "$log" | tail -20 | sed 's/^/::error::  /'
    STATUS=1
  fi
}

run_suite suite.html suite.log
run_suite suite2.html suite2.log
run_suite games.html games.log
run_suite modules.html modules.log
run_suite errors.html errors.log
run_suite events.html events.log
run_suite graphics.html graphics.log
run_suite stress.html stress.log 240
run_suite saves.html saves.log 120
echo "::notice title=V8 smoke suites::${TOTALS#%0A}"

run_page test.html 30 test.log "http://127.0.0.1:$HTTP_PORT/test.html"
for expected in "ROVES-V8 dom: changed by js" "ROVES-V8 click event click true" "ROVES-V8 promise 5" "ROVES-V8 timeout fired"; do
  if grep -aFq "$expected" test.log; then echo "::notice::test.html: $expected"; else echo "::error::test.html: missing '$expected'"; STATUS=1; fi
done
if grep -aFxq 'ROVES-V8 class 2 {"a":[1,2]} true true' test.log; then echo "::notice::test.html: window === globalThis"; else echo "::error::test.html: window === globalThis not confirmed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 immutable prototype JSAPI target true' test.log; then echo "::notice::test.html: ordinary JSAPI prototype mutation"; else echo "::error::test.html: ordinary JSAPI prototype mutation failed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 cross-origin WindowProxy prototype true true' test.log; then echo "::notice::test.html: cross-origin WindowProxy prototype"; else echo "::error::test.html: cross-origin WindowProxy prototype not confirmed ($(grep -aF 'ROVES-V8 cross-origin iframe ' test.log | tr '\n' ' '))"; STATUS=1; fi
if grep -aFq 'ROVES-V8 cross-origin iframe timeout ' test.log; then echo "::error::test.html: cross-origin iframe did not load"; STATUS=1; fi
if grep -aFq 'ROVES-V8 cross-origin iframe error' test.log; then echo "::error::test.html: cross-origin iframe load failed"; STATUS=1; fi

run_page csp.html 30 csp.log
for expected in "ROVES-V8 csp eval EvalError" "ROVES-V8 csp function EvalError" "ROVES-V8 csp trusted eval 42" "ROVES-V8 csp trusted function 42" "ROVES-V8 csp ordinary eval unchanged true"; do
  if grep -aFxq "$expected" csp.log; then echo "::notice::csp.html: $expected"; else STATUS=1; echo "::error::csp.html: missing '$expected' ($(grep -a 'ROVES-V8 csp' csp.log | tr '
' ' '))"; fi
done

run_page wasm-csp.html 30 wasm-csp.log
if [ "$MODE" = normal ]; then
  if grep -aFxq 'ROVES-V8 csp wasm blocked CompileError' wasm-csp.log; then
    echo "::notice::wasm-csp.html: blocked without wasm-unsafe-eval"
  else
    echo "::error::wasm-csp.html: WebAssembly compilation was not blocked by CSP"
    STATUS=1
  fi
else
  if grep -aFxq 'ROVES-V8 csp wasm unavailable' wasm-csp.log || grep -aFxq 'ROVES-V8 csp wasm blocked CompileError' wasm-csp.log; then
    echo "::notice::wasm-csp.html: Wasm unavailable or rejected in JIT-less mode"
  else
    echo "::error::wasm-csp.html: unexpected JIT-less WebAssembly CSP result"
    STATUS=1
  fi
fi

run_page nav1.html 30 nav.log
for expected in "ROVES-V8 nav2 loaded nav2.html true second page" "ROVES-V8 nav1 pageshow persisted=true"; do
  if grep -aFxq "$expected" nav.log; then echo "::notice::navigation: $expected"; else echo "::error::navigation: missing '$expected'"; STATUS=1; fi
done

if [ "$STATUS" -ne 0 ]; then
  for server_log in "$HTTP_LOG" "$HTTP_CROSS_ORIGIN_LOG"; do
    if [ -s "$server_log" ]; then
      sed 's/^/::notice::V8 smoke HTTP: /' "$server_log"
    fi
  done
fi

exit $STATUS
