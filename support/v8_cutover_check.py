#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Measures how far the workspace is from compiling against `roves-js` (mozjs on V8).

The script temporarily points the workspace `js` dependency at components/roves-js, runs
`cargo check` on the requested crate, and reports the number of errors and the most frequent
error kinds. It restores Cargo.toml and Cargo.lock afterwards. It uses a separate target
directory, so the normal SpiderMonkey build cache is untouched.

    python support/v8_cutover_check.py [crate] [--top N] [--build [--release]]

With `--build` it runs `cargo build` instead, producing a V8 build of the crate in
target/v8-cutover (servoshell: a runnable browser on roves-js).
"""

import argparse
import collections
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MOZJS_LINE = re.compile(r'^js = \{ package = "mozjs".*$', re.M)
ROVES_JS_LINE = 'js = { package = "roves-js", path = "components/roves-js" }'


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("crate", nargs="?", default="servo-script-bindings")
    parser.add_argument("--top", type=int, default=25)
    parser.add_argument("--build", action="store_true", help="cargo build instead of cargo check")
    parser.add_argument("--release", action="store_true", help="with --build: a release build")
    parser.add_argument("--online", action="store_true", help="let cargo fetch dependencies (CI)")
    args = parser.parse_args()

    manifest = ROOT / "Cargo.toml"
    lock = ROOT / "Cargo.lock"
    original_manifest = manifest.read_bytes()
    original_lock = lock.read_bytes()
    text = original_manifest.decode("utf-8")
    if not MOZJS_LINE.search(text):
        print("Cargo.toml no longer depends on mozjs: nothing to swap", file=sys.stderr)
        return 1
    try:
        manifest.write_text(MOZJS_LINE.sub(ROVES_JS_LINE, text), encoding="utf-8", newline="")
        env = dict(os.environ, AWS_LC_SYS_NO_ASM="1")
        command = ["cargo", "build" if args.build else "check", "-p", args.crate,
                   "--message-format=short", "--target-dir", str(ROOT / "target" / "v8-cutover")]
        if not args.online:
            command.append("--offline")
        if args.build and args.release:
            command.append("--release")
        result = subprocess.run(
            command,
            cwd=ROOT, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace",
        )
    finally:
        manifest.write_bytes(original_manifest)
        lock.write_bytes(original_lock)

    log = ROOT / "target" / "v8-cutover" / f"{args.crate}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text(result.stderr, encoding="utf-8")
    print(f"Full compiler output: {log}")
    errors = [line for line in result.stderr.splitlines() if ": error" in line]
    kinds = collections.Counter()
    for line in errors:
        message = line.split(": error", 1)[1]
        message = re.sub(r"`[^`]*`", "`_`", message)
        kinds[message.strip()] += 1
    missing = collections.Counter()
    for line in errors:
        found = re.search(r"cannot find (?:macro|type|value|trait|function[^`]*) `([^`]+)`", line)
        if not found:
            found = re.search(r"(?:unresolved import|no) `([^`]+)`", line)
        if found:
            missing[found.group(1)] += 1
    failing = sorted({line.split(":", 1)[0] for line in errors})
    print(f"V8 cutover check ({args.crate}): {len(errors)} errors in {len(failing)} files")
    for message, count in kinds.most_common(args.top):
        print(f"{count:6}  {message}")
    if missing:
        print("Most missing names:")
        for name, count in missing.most_common(args.top):
            print(f"{count:6}  {name}")
    if not errors and result.returncode != 0:
        print(result.stderr[-4000:])
    return 0


if __name__ == "__main__":
    sys.exit(main())
