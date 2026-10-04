# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Explicit V8 pilot driver using Servo's existing WebIDL parser and generator.

This is an opt-in migration backend, not a replacement for run.py yet. The tiny
Window exposure declaration describes the isolated realm, not an implementation
of Window. Actual selected interface definitions come unmodified from webidls/.
"""
import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(ROOT / "third_party/WebIDL/parser"), str(ROOT / "third_party/ply")]

import WebIDL
from codegen import CGV8BindingRoot


def generate(webidl: Path, out_dir: Path) -> str:
    out_dir.mkdir(parents=True, exist_ok=True)
    parser = WebIDL.Parser(str(out_dir / "cache"))
    parser.parse("[Global=Window, Exposed=Window] interface Window {};", "V8RealmExposure.webidl")
    parser.parse("[Global=Worker, Exposed=Worker] interface Worker {};", "V8RealmExposure.webidl")
    parser.parse(webidl.read_text(encoding="utf-8"), str(webidl))
    interfaces = [
        item for item in parser.finish()
        if isinstance(item, WebIDL.IDLInterface) and item.identifier.name not in {"Window", "Worker"}
    ]
    if len(interfaces) != 1 or not isinstance(interfaces[0], WebIDL.IDLInterface):
        raise TypeError("V8 pilot requires exactly one interface")
    return CGV8BindingRoot(interfaces[0]).define()


def main() -> None:
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("webidl", type=Path)
    cli.add_argument("output", type=Path)
    args = cli.parse_args()
    source = generate(args.webidl, args.output.parent)
    args.output.write_text(source, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
