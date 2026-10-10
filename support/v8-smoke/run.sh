#!/usr/bin/env bash
# Runs the V8 smoke pages headlessly with a servoshell built on roves-js
# (`python support/v8_cutover_check.py servoshell --build`) and reports each result as a
# GitHub annotation (readable without a token). Fails when a page crashes the browser, when
# the smoke suite does not report its total, or when the basic page checks fail.
#
#     support/v8-smoke/run.sh path/to/servoshell [runner args...]
set -u
BIN="$1"
shift
RUNNER=("$@")
DIR="$(cd "$(dirname "$0")" && pwd)"
STATUS=0

run_page() {
  local page="$1" seconds="$2" log="$3"
  timeout "$seconds" "${RUNNER[@]}" "$BIN" -z "file://$DIR/$page" > "$log" 2>&1
  local code=$?
  # 124: the timeout ended the run (pages keep the browser open); anything else but 0 crashed.
  if [ "$code" -ne 0 ] && [ "$code" -ne 124 ]; then
    echo "::error::$page: servoshell exited with $code"
    grep -a -v '^\s*$' "$log" | tail -20 | sed 's/^/::error::  /'
    STATUS=1
  fi
}

run_suite() {
  local page="$1" log="$2"
  run_page "$page" 60 "$log"
  # GitHub shows at most 10 annotations of a kind per step: one summary carries every failure.
  local total failures
  total=$(grep -a "^SUITE TOTAL " "$log" | head -1)
  failures=$(grep -a "^SUITE FAIL " "$log" | sed 's/^SUITE FAIL //' | tr '
' '|' | sed 's/|/%0A/g')
  if [ -n "$failures" ]; then
    echo "::warning title=$page failures (${total#SUITE TOTAL })::$failures"
  fi
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    { echo "## $page: ${total#SUITE TOTAL }"; echo; grep -a "^SUITE " "$log" | sed 's/^/- /'; } >> "$GITHUB_STEP_SUMMARY"
  fi
  if [ -n "$total" ]; then
    echo "::notice title=$page::$total"
  else
    echo "::error::$page did not report its total"
    grep -a -v '^\s*$' "$log" | tail -20 | sed 's/^/::error::  /'
    STATUS=1
  fi
}

run_suite suite.html suite.log
run_suite suite2.html suite2.log

run_page test.html 30 test.log
for expected in "ROVES-V8 dom: changed by js" "ROVES-V8 click event click true" "ROVES-V8 promise 5" "ROVES-V8 timeout fired"; do
  if grep -aq "$expected" test.log; then echo "::notice::test.html: $expected"; else echo "::error::test.html: missing '$expected'"; STATUS=1; fi
done
if grep -aq "ROVES-V8 class 2 {\"a\":\[1,2\]} true true" test.log; then echo "::notice::test.html: window === globalThis"; else echo "::warning::test.html: window === globalThis not confirmed"; fi

run_page csp.html 30 csp.log
for expected in "ROVES-V8 csp eval EvalError" "ROVES-V8 csp function EvalError"; do
  if grep -aq "$expected" csp.log; then echo "::notice::csp.html: $expected"; else echo "::warning::csp.html: missing '$expected' ($(grep -a 'ROVES-V8 csp' csp.log | tr '
' ' '))"; fi
done

run_page nav1.html 30 nav.log
for expected in "ROVES-V8 nav2 loaded nav2.html true second page" "ROVES-V8 nav1 pageshow persisted=true"; do
  if grep -aq "$expected" nav.log; then echo "::notice::navigation: $expected"; else echo "::error::navigation: missing '$expected'"; STATUS=1; fi
done

exit $STATUS
