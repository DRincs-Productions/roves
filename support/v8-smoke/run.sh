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
# Native heap snapshot settings reach only the dedicated document-retention process (PAGE_ENV).
unset ROVES_V8_HEAP_SNAPSHOT_PATH ROVES_V8_HEAP_SNAPSHOT_GC
PAGE_ENV=()
PAGE_ENV_COUNT=0
HTTP_CROSS_ORIGIN_PORT=18766
HTTP_LOG="${TMPDIR:-/tmp}/roves-v8-smoke-http-$$.log"
"$PYTHON" "$DIR/http_server.py" "$HTTP_PORT" "$DIR" >"$HTTP_LOG" 2>&1 &
HTTP_PID=$!
HTTP_CROSS_ORIGIN_LOG="${TMPDIR:-/tmp}/roves-v8-smoke-http-cross-origin-$$.log"
"$PYTHON" "$DIR/http_server.py" "$HTTP_CROSS_ORIGIN_PORT" "$DIR" >"$HTTP_CROSS_ORIGIN_LOG" 2>&1 &
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

# run_page PAGE SECONDS LOG [URL] [COMPLETION-REGEX] [BROWSER ARGS...]
run_page() {
  local page="$1" seconds="$2" log="$3" url="${4:-file://$DIR/$1}" done_pattern="${5:-}"
  local completed=0 pid code runtime_cache
  # Keep each process's existing file logger, which otherwise truncates roves.log at restart.
  # An isolated cache makes these CI scenes cold-cache observations, not warm-load benchmarks.
  mkdir -p "$PWD/.v8-smoke-runtime" || { STATUS=1; return 1; }
  runtime_cache=$(mktemp -d "$PWD/.v8-smoke-runtime/${log%.log}.XXXXXX") || { STATUS=1; return 1; }
  # Bash 3.2 treats an empty array as unset under nounset. Only expand non-empty arrays.
  local argv=()
  if [ "$RUNNER_COUNT" -gt 0 ]; then argv=("${RUNNER[@]}"); fi
  # Reuse Servo's crash-test mode: a failed script thread must terminate this process,
  # rather than leaving a blank window alive until every page timeout expires.
  argv+=("$BIN" --hard-fail)
  if [ $# -gt 5 ]; then shift 5; argv+=("$@"); fi
  argv+=(-z "$url")
  if [ "$PAGE_ENV_COUNT" -gt 0 ]; then
    env XDG_CACHE_HOME="$runtime_cache" "${PAGE_ENV[@]}" timeout "$seconds" "${argv[@]}" > "$log" 2>&1 &
  else
    env XDG_CACHE_HOME="$runtime_cache" timeout "$seconds" "${argv[@]}" > "$log" 2>&1 &
  fi
  pid=$!
  if [ -n "$done_pattern" ]; then
    # The page reports completion before the deadline. GNU timeout forwards TERM to the
    # browser; callers still validate the reported results.
    while kill -0 "$pid" 2>/dev/null; do
      if grep -aq "$done_pattern" "$log"; then
        completed=1
        kill -TERM "$pid" 2>/dev/null || true
        break
      fi
      sleep 0.1
    done
  fi
  wait "$pid"
  code=$?
  if [ "$completed" -eq 1 ] && [ "$code" -eq 143 ]; then code=0; fi
  # 124: the timeout ended the run (pages keep the browser open); anything else but 0 crashed.
  if [ "$code" -ne 0 ] && [ "$code" -ne 124 ]; then
    echo "::error::$page: servoshell exited with $code"
    grep -a -v '^\s*$' "$log" | tail -20 | sed 's/^/::error::  /'
    STATUS=1
  fi
}

run_suite() {
  local page="$1" log="$2" seconds="${3:-60}" url="${4:-}"
  run_page "$page" "$seconds" "$log" "$url" '^SUITE TOTAL '
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

# Windows CI reuses the same real-browser module checks without Unix graphics tooling.
if [ "${V8_SMOKE_MODULE_ORDER_ONLY:-0}" = 1 ]; then
  run_suite module-order.html module-order.log 60 "http://127.0.0.1:$HTTP_PORT/module-order.html"
  echo "::notice::V8 module order smoke:$TOTALS"
  exit "$STATUS"
fi

run_suite suite.html suite.log
run_suite suite2.html suite2.log
run_suite games.html games.log
run_suite modules.html modules.log
run_suite errors.html errors.log
run_suite events.html events.log
run_suite single-document.html single-document.log 60 "http://127.0.0.1:$HTTP_PORT/single-document.html"
run_suite module-order.html module-order.log 60 "http://127.0.0.1:$HTTP_PORT/module-order.html"
run_suite module-csp.html module-csp.log 60 "http://127.0.0.1:$HTTP_PORT/module-csp.html"
# Each alias resolution order requires a fresh process; the active document is never reloaded.
for first in Document HTMLDocument descriptor; do
  run_suite document-interface-alias.html "document-interface-alias-$first.log" 30 "http://127.0.0.1:$HTTP_PORT/document-interface-alias.html?first=$first"
done
# Refused navigation/frame targets must not reach either HTTP fixture server.
if grep -aFq 'blocked-single-document-target.html' "$HTTP_LOG" "$HTTP_CROSS_ORIGIN_LOG"; then
  echo "::error::a blocked document operation fetched its target"
  STATUS=1
fi
run_suite graphics.html graphics.log
run_suite stress.html stress.log 240
run_suite saves.html saves.log 120
echo "::notice title=V8 smoke suites::${TOTALS#%0A}"

run_page test.html 30 test.log "http://127.0.0.1:$HTTP_PORT/test.html"
for expected in "ROVES-V8 dom: changed by js" "ROVES-V8 click event click true" "ROVES-V8 promise 5" "ROVES-V8 timeout fired"; do
  if grep -aFq "$expected" test.log; then echo "::notice::test.html: $expected"; else echo "::error::test.html: missing '$expected'"; STATUS=1; fi
done
if grep -aFxq 'ROVES-V8 class 2 {"a":[1,2]} true true' test.log; then echo "::notice::test.html: window === globalThis"; else echo "::error::test.html: window === globalThis not confirmed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 testutils default undefined' test.log; then echo "::notice::test.html: TestUtils hidden by default"; else echo "::error::test.html: TestUtils exposed without dom_testutils_enabled ($(grep -aF 'ROVES-V8 testutils ' test.log | tr '\n' ' '))"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 ordinary prototype mutation true' test.log; then echo "::notice::test.html: ordinary prototype mutation"; else echo "::error::test.html: ordinary prototype mutation failed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 immutable Window prototype true true true true' test.log; then echo "::notice::test.html: immutable Window prototype"; else echo "::error::test.html: immutable Window prototype not confirmed"; STATUS=1; fi
if grep -aFxq 'ROVES-V8 cross-origin iframe refused true true' test.log; then echo "::notice::test.html: cross-origin iframe refused"; else echo "::error::test.html: cross-origin iframe created a child document"; STATUS=1; fi

run_page csp.html 30 csp.log
for expected in "ROVES-V8 csp eval EvalError" "ROVES-V8 csp function EvalError" "ROVES-V8 csp trusted eval 42" "ROVES-V8 csp trusted function 42" "ROVES-V8 csp trusted function guards true" "ROVES-V8 csp trusted function matrix true true true" "ROVES-V8 csp constructor graph true true true" "ROVES-V8 csp ordinary eval unchanged true" "ROVES-V8 csp attribute allowed true true"; do
  if grep -aFxq "$expected" csp.log; then echo "::notice::csp.html: $expected"; else STATUS=1; echo "::error::csp.html: missing '$expected' ($(grep -a 'ROVES-V8 csp' csp.log | tr '
' ' '))"; fi
done

run_page csp-attr.html 30 csp-attr.log
if grep -aFxq 'ROVES-V8 csp attribute blocked true true true' csp-attr.log; then
  echo "::notice::csp-attr.html: script-src-attr 'none' blocks parsed, scripted and forwarded handlers"
else
  STATUS=1; echo "::error::csp-attr.html: missing 'ROVES-V8 csp attribute blocked true true true' ($(grep -a 'ROVES-V8 csp' csp-attr.log | tr '
' ' '))"
fi

# script-src-attr 'unsafe-hashes' + exact sha256, then the same rules through the script-src
# fallback: the matching body runs on an element and on a <body> handler forwarded to Window;
# a different body or a nonce never authorizes a handler attribute.
for kind in hash fallback; do
  run_page "csp-attr-$kind.html" 30 "csp-attr-$kind.log"
  for expected in "ROVES-V8 csp attribute $kind allowed true true" "ROVES-V8 csp attribute $kind blocked true true true"; do
    if grep -aFxq "$expected" "csp-attr-$kind.log"; then echo "::notice::csp-attr-$kind.html: $expected"; else STATUS=1; echo "::error::csp-attr-$kind.html: missing '$expected' ($(grep -a 'ROVES-V8 csp' "csp-attr-$kind.log" | tr '
' ' '))"; fi
  done
done

# Same exact hash and nonce without 'unsafe-hashes': attribute handlers stay blocked.
run_page csp-attr-no-unsafe-hashes.html 30 csp-attr-no-unsafe-hashes.log
if grep -aFxq 'ROVES-V8 csp attribute no-unsafe-hashes blocked true true' csp-attr-no-unsafe-hashes.log; then
  echo "::notice::csp-attr-no-unsafe-hashes.html: a matching hash without 'unsafe-hashes' blocks element and forwarded handlers"
else
  STATUS=1; echo "::error::csp-attr-no-unsafe-hashes.html: missing 'ROVES-V8 csp attribute no-unsafe-hashes blocked true true' ($(grep -a 'ROVES-V8 csp' csp-attr-no-unsafe-hashes.log | tr '
' ' '))"
fi

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

# The former full-document navigation positives remain in legacy-multidocument/.
# single-document.html now requires refusal plus functioning History/hash routes.

# V8 flags are process-wide and fixed before the first isolate: the preference needs its own
# fresh process. Wasm stays required in the normal pages above, so a build that is jitless
# globally cannot pass. In JIT-less mode this cannot tell the preference from the feature.
run_page pref-jitless.html 15 pref-jitless.log "" '^ROVES-V8 pref js_disable_jit ' --pref js_disable_jit=true
if grep -aFxq 'ROVES-V8 pref js_disable_jit js true wasm unavailable' pref-jitless.log; then
  echo "::notice::pref-jitless.html: --pref js_disable_jit=true started V8 without WebAssembly"
else
  echo "::error::pref-jitless.html: --pref js_disable_jit=true not confirmed ($(grep -aF 'ROVES-V8 pref ' pref-jitless.log | tr '\n' ' '))"
  STATUS=1
fi

# Servo's TestUtils.gc() is exposed only with dom_testutils_enabled (default false, checked
# hidden in test.html), so it gets its own process. The single-document scene preserves
# DOM/closure/handler/importer collection controls and module/eval/resolve behavior across GC.
# The old removed-frame groups are archived, not claimed as passing under the new policy.
run_page gc-lifecycle.html 40 gc-lifecycle.log "http://127.0.0.1:$HTTP_PORT/gc-lifecycle.html" '^ROVES-V8 gc result ' --pref dom_testutils_enabled=true
for expected in "ROVES-V8 gc api true" "ROVES-V8 gc unreachable dom collected true" "ROVES-V8 gc unreachable cycles collected true" "ROVES-V8 gc retained nodes true true true" "ROVES-V8 gc importer compiled true" "ROVES-V8 gc dynamic import after collection 42" "ROVES-V8 gc dropped importer collected true" "ROVES-V8 gc self-capturing listener nodes collected true" "ROVES-V8 gc signal listener nodes collected true" "ROVES-V8 gc aborted signal listener nodes collected true" "ROVES-V8 gc listener control after collection true" "ROVES-V8 gc self-capturing inline handler nodes collected true" "ROVES-V8 gc inline handler control after collection true" "ROVES-V8 gc window inline handlers after collection true" "ROVES-V8 gc retained nodes final true true true" "ROVES-V8 gc cleanup callback entry settings true" "ROVES-V8 gc single document module owner before true" "ROVES-V8 gc single document module owner after true" "ROVES-V8 gc single document module resolve before true" "ROVES-V8 gc single document module resolve after true" "ROVES-V8 gc single document remains live true" "ROVES-V8 gc result pass"; do
  if grep -aFxq "$expected" gc-lifecycle.log; then echo "::notice::gc-lifecycle.html: $expected"; else STATUS=1; echo "::error::gc-lifecycle.html: missing '$expected' ($(grep -aF 'ROVES-V8 gc ' gc-lifecycle.log | tr '\n' ' '))"; fi
done
# Opt-in (V8_SMOKE_HEAP_SNAPSHOT, an absolute path to a new file): a debug servoshell dumps one
# native V8 heap snapshot at the last of document-retention.html's 8 explicit collections, for
# reading the retainers of detached DOM wrappers with a live control. A requested snapshot must exist and be valid.
HEAP_SNAPSHOT="${V8_SMOKE_HEAP_SNAPSHOT:-}"
HEAP_SNAPSHOT_COLLECTION=8
SNAPSHOT_LOGS=""
if [ -n "$HEAP_SNAPSHOT" ]; then
  case "$HEAP_SNAPSHOT" in
    /*) snapshot_ok=1 ;;
    *) snapshot_ok=0; echo "::error::V8_SMOKE_HEAP_SNAPSHOT must be an absolute path: $HEAP_SNAPSHOT" ;;
  esac
  if [ "$snapshot_ok" -eq 1 ] && [ -e "$HEAP_SNAPSHOT" ]; then
    snapshot_ok=0; echo "::error::V8_SMOKE_HEAP_SNAPSHOT already exists and is never overwritten: $HEAP_SNAPSHOT"
  fi
  if [ "$snapshot_ok" -eq 1 ] && ! mkdir -p "$(dirname "$HEAP_SNAPSHOT")"; then
    snapshot_ok=0; echo "::error::cannot create the directory of V8_SMOKE_HEAP_SNAPSHOT: $HEAP_SNAPSHOT"
  fi
  if [ "$snapshot_ok" -eq 1 ]; then
    PAGE_ENV=("ROVES_V8_HEAP_SNAPSHOT_PATH=$HEAP_SNAPSHOT" "ROVES_V8_HEAP_SNAPSHOT_GC=$HEAP_SNAPSHOT_COLLECTION")
    PAGE_ENV_COUNT=2
    SNAPSHOT_LOGS="document-retention.log"
    run_page document-retention.html 60 document-retention.log "http://127.0.0.1:$HTTP_PORT/document-retention.html" '^ROVES-V8 document-retention result ' --pref dom_testutils_enabled=true
    PAGE_ENV=()
    PAGE_ENV_COUNT=0
    for expected in "ROVES-V8 document-retention api true" "ROVES-V8 document-retention node created true removed true" "ROVES-V8 document-retention collections complete $HEAP_SNAPSHOT_COLLECTION" "ROVES-V8 document-retention retained control true" "ROVES-V8 document-retention result pass" "[roves-heap-snapshot] wrote " ; do
      if grep -aFq "$expected" document-retention.log; then echo "::notice::document-retention.html: $(grep -aF "$expected" document-retention.log | head -1)"; else snapshot_ok=0; echo "::error::document-retention.html: missing '$expected' ($(grep -aF -e 'ROVES-V8 document-retention ' -e '[roves-heap-snapshot]' document-retention.log | tr '\n' ' '))"; fi
    done
    if ! grep -aFq " to $HEAP_SNAPSHOT at explicit collection $HEAP_SNAPSHOT_COLLECTION" document-retention.log; then
      snapshot_ok=0; echo "::error::document-retention.html: no snapshot written to $HEAP_SNAPSHOT at explicit collection $HEAP_SNAPSHOT_COLLECTION"
    fi
    # Released counts are diagnostic only, but must be reported.
    diagnostic=$(grep -aE '^ROVES-V8 document-retention diagnostic released element (true|false) probe (true|false)$' document-retention.log | head -1)
    if [ -n "$diagnostic" ]; then echo "::notice::document-retention.html: $diagnostic"; else snapshot_ok=0; echo "::error::document-retention.html: missing released diagnostic"; fi
    if [ ! -s "$HEAP_SNAPSHOT" ]; then
      snapshot_ok=0; echo "::error::document-retention.html: requested heap snapshot missing or empty: $HEAP_SNAPSHOT"
    elif ! "$PYTHON" "$DIR/check_heap_snapshot.py" "$HEAP_SNAPSHOT" rovesDocumentRetentionProbe roves-document-retention-probe; then
      snapshot_ok=0
    fi
  fi
  if [ "$snapshot_ok" -ne 1 ]; then STATUS=1; fi
fi

if [ "$STATUS" -ne 0 ]; then
  # Marker summaries alone hide exceptions thrown between setup and a callback's result.
  # Keep bounded raw diagnostics as ordinary log lines, outside GitHub's annotation quota.
  for page_log in test.log csp.log csp-attr.log csp-attr-hash.log csp-attr-fallback.log csp-attr-no-unsafe-hashes.log wasm-csp.log pref-jitless.log gc-lifecycle.log $SNAPSHOT_LOGS; do
    echo "V8 smoke diagnostics: $page_log"
    tail -80 "$page_log"
  done
  for server_log in "$HTTP_LOG" "$HTTP_CROSS_ORIGIN_LOG"; do
    if [ -s "$server_log" ]; then
      sed 's/^/::notice::V8 smoke HTTP: /' "$server_log"
    fi
  done
fi

exit $STATUS
