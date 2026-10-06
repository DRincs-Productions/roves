# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Explicit V8 pilot driver using Servo's existing WebIDL parser and generator.

This is an opt-in migration backend, not a replacement for run.py yet. The tiny
Window exposure declaration describes the isolated realm, not an implementation
of Window. Actual selected interface definitions come unmodified from webidls/.

The generated binding is for the single interface defined in `webidl`. Files
passed as `context` are parsed only so that interface's ancestors resolve; they
are generated separately, into sibling modules (see codegen.v8_module_name).
"""
import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(ROOT / "third_party/WebIDL/parser"), str(ROOT / "third_party/ply")]

import WebIDL
import codegen
from codegen import CGV8BindingRoot


def generate(webidl: Path, out_dir: Path, context: tuple[Path, ...] = ()) -> str:
    out_dir.mkdir(parents=True, exist_ok=True)
    parser = WebIDL.Parser(str(out_dir / "cache"))
    parser.parse("[Global=Window, Exposed=Window] interface Window {};", "V8RealmExposure.webidl")
    parser.parse("[Global=Worker, Exposed=Worker] interface Worker {};", "V8RealmExposure.webidl")
    for dependency in context:
        parser.parse(dependency.read_text(encoding="utf-8"), str(dependency))
    parser.parse(webidl.read_text(encoding="utf-8"), str(webidl))
    interfaces = [
        item for item in parser.finish()
        # An iterator interface (`PairsIterator`) is implemented by the runtime with its owner.
        if isinstance(item, (WebIDL.IDLInterface, WebIDL.IDLNamespace)) and not item.isCallback()
        and getattr(item, "iterableInterface", None) is None
        and item.location.filename == str(webidl)
    ]
    if len(interfaces) != 1:
        raise TypeError("V8 pilot requires exactly one interface")
    return CGV8BindingRoot(interfaces[0]).define()


def main() -> None:
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("webidl", type=Path)
    cli.add_argument("output", type=Path)
    cli.add_argument("context", type=Path, nargs="*", help="WebIDL files defining ancestors")
    cli.add_argument("--bindings-conf", type=Path, action="append", default=[],
                     help="extra Bindings.conf-style file (e.g. 'cx' lists for test fixtures)")
    args = cli.parse_args()
    codegen.V8_EXTRA_BINDINGS_CONFS.extend(str(path) for path in args.bindings_conf)
    source = generate(args.webidl, args.output.parent, tuple(args.context))
    args.output.write_text(source, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
