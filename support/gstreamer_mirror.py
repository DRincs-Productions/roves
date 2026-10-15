#!/usr/bin/env python3
"""Copy the GStreamer installers Roves builds with into this repo's own GitHub release.

gstreamer.freedesktop.org ends responses after about 1 MiB and is often unavailable (503), which
failed release builds. This script fetches each file in 1 MiB ranges, retrying every range until
it arrives whole, checks the published SHA-256, and leaves the files in the output directory for
the workflow to upload. Patient by design: a CI job may run it for hours.
"""

import hashlib
import sys
import time
import urllib.request
from pathlib import Path

VERSION = "1.28.7"
BASE = "https://gstreamer.freedesktop.org/data/pkg"
FILES = [
    f"windows/{VERSION}/msvc/gstreamer-1.0-msvc-x86_64-{VERSION}.exe",
    f"osx/{VERSION}/gstreamer-1.0-{VERSION}-universal.pkg",
    f"osx/{VERSION}/gstreamer-1.0-devel-{VERSION}-universal.pkg",
]
CHUNK = 1024 * 1024


def fetch(url: str, headers: dict[str, str] | None = None) -> bytes:
    """GET `url`, retrying (with a capped growing pause) until the server answers."""
    for attempt in range(1, 10000):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "roves-mirror", **(headers or {})})
            with urllib.request.urlopen(request, timeout=60) as response:
                return response.read()
        except Exception as error:  # noqa: BLE001 - every network error means "try again"
            print(f"  retry {attempt} for {url} ({error})", flush=True)
            time.sleep(min(5 * attempt, 60))
    raise RuntimeError(f"giving up on {url}")


def expected_sha256(path: str) -> str:
    while True:
        text = fetch(f"{BASE}/{path}.sha256sum").decode(errors="replace").split()
        if text and len(text[0]) == 64:
            return text[0].lower()
        print("  checksum file not available yet, waiting", flush=True)
        time.sleep(30)


def total_length(url: str) -> int:
    for attempt in range(1, 10000):
        try:
            request = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "roves-mirror"})
            with urllib.request.urlopen(request, timeout=60) as response:
                return int(response.headers["Content-Length"])
        except Exception as error:  # noqa: BLE001
            print(f"  retry {attempt} for HEAD {url} ({error})", flush=True)
            time.sleep(min(5 * attempt, 60))
    raise RuntimeError(f"no length for {url}")


def download(path: str, output: Path) -> None:
    url = f"{BASE}/{path}"
    target = output / Path(path).name
    sha = expected_sha256(path)
    length = total_length(url)
    with open(target, "wb") as file:
        for start in range(0, length, CHUNK):
            end = min(start + CHUNK, length) - 1
            while True:
                data = fetch(url, {"Range": f"bytes={start}-{end}"})
                if len(data) == end - start + 1:
                    break
                print(f"  short range {start}-{end}: got {len(data)} bytes, retrying", flush=True)
                time.sleep(5)
            file.write(data)
            if (start // CHUNK) % 50 == 0:
                print(f"{target.name}: {end + 1}/{length}", flush=True)
    actual = hashlib.sha256(target.read_bytes()).hexdigest()
    if actual != sha:
        raise RuntimeError(f"{target.name}: sha256 {actual} != published {sha}")
    print(f"{target.name}: ok ({length} bytes, sha256 {sha})", flush=True)


def main() -> None:
    output = Path(sys.argv[1] if len(sys.argv) > 1 else "gstreamer-mirror")
    output.mkdir(parents=True, exist_ok=True)
    for path in FILES:
        download(path, output)


if __name__ == "__main__":
    main()
