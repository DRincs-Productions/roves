#!/usr/bin/env bash
# cargo-retry.sh LOG cargo-args...
# Runs `cargo <args>` teeing to LOG. rusty_v8's build script downloads the
# prebuilt V8 from GitHub releases (no official mirror): retry only a transient
# HTTP 502/503/504 of that download, as .github/workflows/blitz-probe.yml does.
# Any other failure (Rust errors included) exits immediately.
set -u
log=$1
shift
for attempt in 1 2 3; do
  cargo "$@" 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  [ "$status" -eq 0 ] && exit 0
  if [ "$attempt" -lt 3 ] \
    && grep -q 'Failed to download V8 prebuilt archive' "$log" \
    && grep -qE 'HTTP Error 50[234]' "$log"; then
    echo "::warning::V8 prebuilt download failed with HTTP 502/503/504 (attempt $attempt); retrying"
    sleep $((attempt * 60))
    continue
  fi
  exit "$status"
done
