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

run_page() {
  local page="$1" seconds="$2" log="$3"
  # Bash 3.2 treats an empty array as unset under nounset. Do not expand it.
  if [ "$RUNNER_COUNT" -gt 0 ]; then
    timeout "$seconds" "${RUNNER[@]}" "$BIN" -z "file://$DIR/$page" > "$log" 2>&1
  else
    timeout "$seconds" "$BIN" -z "file://$DIR/$page" > "$log" 2>&1
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

run_page test.html 30 test.log
for expected in "ROVES-V8 dom: changed by js" "ROVES-V8 click event click true" "ROVES-V8 promise 5" "ROVES-V8 timeout fired"; do
  if grep -aFq "$expected" test.log; then echo "::notice::test.html: $expected"; else echo "::error::test.html: missing '$expected'"; STATUS=1; fi
done
if grep -aFxq 'ROVES-V8 class 2 {"a":[1,2]} true true' test.log; then echo "::notice::test.html: window === globalThis"; else echo "::error::test.html: window === globalThis not confirmed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 immutable window prototype true' test.log; then echo "::notice::test.html: immutable Window prototype"; else echo "::error::test.html: immutable Window prototype not confirmed"; STATUS=1; fi
if grep -aFq 'ROVES-V8 window prototype diagnostics ' test.log; then echo "::notice::test.html: Window prototype diagnostics available"; else echo "::error::test.html: Window prototype diagnostics missing"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 cross-origin WindowProxy prototype true true' test.log; then echo "::notice::test.html: cross-origin WindowProxy prototype"; else echo "::error::test.html: cross-origin WindowProxy prototype not confirmed"; STATUS=1; fi
if grep -aFq 'ROVES-V8 cross-origin iframe timeout ' test.log; then echo "::error::test.html: cross-origin iframe did not load"; STATUS=1; fi

run_page csp.html 30 csp.log
for expected in "ROVES-V8 csp eval EvalError" "ROVES-V8 csp function EvalError" "ROVES-V8 csp trusted eval 42" "ROVES-V8 csp ordinary eval unchanged true"; do
  if grep -aFxq "$expected" csp.log; then echo "::notice::csp.html: $expected"; else STATUS=1; echo "::error::csp.html: missing '$expected' ($(grep -a 'ROVES-V8 csp' csp.log | tr '
' ' '))"; fi
done

run_page nav1.html 30 nav.log
for expected in "ROVES-V8 nav2 loaded nav2.html true second page" "ROVES-V8 nav1 pageshow persisted=true"; do
  if grep -aFxq "$expected" nav.log; then echo "::notice::navigation: $expected"; else echo "::error::navigation: missing '$expected'"; STATUS=1; fi
done

exit $STATUS
