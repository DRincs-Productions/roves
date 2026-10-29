# Copyright 2026 The Servo Project Developers. See the COPYRIGHT file.
# Licensed under the Apache License, Version 2.0 or the MIT license, at your option.
"""Stage Cargo-built ANGLE DLLs for Windows browser execution (no build or download)."""
import argparse
from pathlib import Path
import shutil


def copy_built_angle_dlls(binary_directory: str | Path) -> bool:
    destination = Path(binary_directory)
    build = destination / "build"
    complete = True
    print(" • Copying ANGLE DLLs to binary directory...")
    for name in ("libEGL.dll", "libGLESv2.dll"):
        source = next(build.rglob(name), None)
        if source is None:
            print(f"WARNING: could not find {name}")
            complete = False
        else:
            shutil.copy(source, destination)
    return complete


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary_directory", type=Path)
    return 0 if copy_built_angle_dlls(parser.parse_args().binary_directory) else 1


if __name__ == "__main__":
    raise SystemExit(main())
