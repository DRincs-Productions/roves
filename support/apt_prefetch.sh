#!/usr/bin/env bash
# Downloads, in parallel, the .deb files apt would fetch to install the given packages, into
# apt's archive cache (Dir::Cache::Archives, set by the CI workflows), so a later
# `apt-get install` (or mach bootstrap) only verifies and installs them. The GitHub runner's
# apt mirror sometimes serves a single connection at a few KB/s, and apt downloads one file at
# a time from it. Best effort: apt checks every file's hash and fetches again what is missing.
#
# Usage: support/apt_prefetch.sh [--jobs N] PACKAGE...
set -uo pipefail

jobs=16
if [ "${1:-}" = "--jobs" ]; then jobs=$2; shift 2; fi
[ "$#" -gt 0 ] || exit 0

eval "$(apt-config shell APT_ARCHIVES Dir::Cache::Archives/d)"
[ -n "${APT_ARCHIVES:-}" ] && [ -w "$APT_ARCHIVES" ] || { echo "apt_prefetch: no writable archive directory" >&2; exit 0; }
export APT_ARCHIVES

# `--print-uris` lists 'URL' FILE SIZE HASH for what is not already in the cache.
list=$(apt-get install --print-uris -qq -y -m --no-install-recommends "$@" 2>/dev/null \
  | sed -nE "s/^'([^']+)' ([^ ]+) .*/\1 \2/p")
[ -n "$list" ] || { echo "apt_prefetch: nothing to download"; exit 0; }

# GitHub's runners use apt's mirror method (`mirror+file:/etc/apt/apt-mirrors.txt/pool/...`),
# which curl cannot fetch: resolve it with the listed mirrors, spreading files across them.
list=$(echo "$list" | awk '
  /^mirror\+file:/ {
    url = $1; sub(/^mirror\+file:/, "", url)
    split(url, parts, "/pool/"); file = parts[1]; path = substr(url, length(file) + 1)
    if (!(file in count)) {
      count[file] = 0
      while ((getline line < file) > 0) {
        if (line ~ /^[[:space:]]*(#|$)/) continue
        split(line, fields, /[ \t]/); sub(/\/$/, "", fields[1]); mirrors[file, count[file]++] = fields[1]
      }
      close(file)
    }
    if (count[file] == 0) next
    print mirrors[file, seen[file]++ % count[file]] path, $2
    next
  }
  { print }')
echo "apt_prefetch: $(echo "$list" | wc -l) files with $jobs connections into $APT_ARCHIVES"

start=$(date +%s)
# shellcheck disable=SC2016  # expanded by the inner shell
echo "$list" | xargs -P "$jobs" -L 1 sh -c '
  part="$APT_ARCHIVES/$1.prefetch"
  if curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 20 -o "$part" "$0"; then
    mv "$part" "$APT_ARCHIVES/$1"
  else
    rm -f "$part"
  fi
'
echo "apt_prefetch: done in $(( $(date +%s) - start ))s"
exit 0
