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

run_page suite.html 60 suite.log
while IFS= read -r line; do
  case "$line" in
    "SUITE PASS "*) echo "::notice::$line" ;;
    "SUITE FAIL "*) echo "::warning::$line" ;;
    "SUITE TOTAL "*) echo "::notice::$line" ;;
  esac
done < <(grep -a "^SUITE " suite.log)
if ! grep -aq "^SUITE TOTAL " suite.log; then
  echo "::error::suite.html did not report its total"
  grep -a -v '^\s*$' suite.log | tail -20 | sed 's/^/::error::  /'
  STATUS=1
fi

run_page test.html 30 test.log
for expected in "ROVES-V8 dom: changed by js" "ROVES-V8 click event click true" "ROVES-V8 promise 5" "ROVES-V8 timeout fired"; do
  if grep -aq "$expected" test.log; then echo "::notice::test.html: $expected"; else echo "::error::test.html: missing '$expected'"; STATUS=1; fi
done
if grep -aq "ROVES-V8 class 2 {\"a\":\[1,2\]} true true" test.log; then echo "::notice::test.html: window === globalThis"; else echo "::warning::test.html: window === globalThis not confirmed"; fi

run_page nav1.html 30 nav.log
for expected in "ROVES-V8 nav2 loaded nav2.html true second page" "ROVES-V8 nav1 pageshow persisted=true"; do
  if grep -aq "$expected" nav.log; then echo "::notice::navigation: $expected"; else echo "::error::navigation: missing '$expected'"; STATUS=1; fi
done

exit $STATUS
