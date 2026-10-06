# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Measure how much of Servo's real WebIDL the V8 backend can generate.

Parses every file under webidls/ together (as run.py does, so inheritance and
cross-file types resolve) and runs CGV8BindingRoot on each interface, namespace
and callback interface. The backend fails closed, so each rejection carries the
first unsupported shape it hit; reasons are bucketed with the interface and
member names removed so the most common blockers can be ranked. This is a
migration progress metric only: it does not build or run anything.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(ROOT / "third_party/WebIDL/parser"), str(ROOT / "third_party/ply")]

import WebIDL
from codegen import CGV8BindingRoot

FILTER_PATTERN = re.compile("// skip-unless ([A-Z_]+)\n")


def parse_all(cache_dir: Path) -> list:
    parser = WebIDL.Parser(str(cache_dir))
    for webidl in sorted((ROOT / "webidls").glob("*.webidl")):
        contents = webidl.read_text(encoding="utf-8")
        match = FILTER_PATTERN.search(contents)
        if match and not os.environ.get(match.group(1)):
            continue
        parser.parse(contents, str(webidl))
    return parser.finish()


def bucket(message: str) -> str:
    """Strip the per-interface/member suffix so equal blockers group together."""
    reason = message.split(": ", 1)[0]
    # Attribute-list rejections name the attributes after the interface name;
    # keep the attribute list, drop the interface.
    attributes = re.match(r"(V8 backend unsupported attributes on) \w+: (.*)", message)
    if attributes:
        return f"{attributes.group(1)} interface: {attributes.group(2)}"
    # Type rejections keep the offending type, which is what needs implementing.
    typed = re.match(r"(V8 backend unsupported [\w ]+type): [\w.]+: (.*)", message)
    if typed:
        return f"{typed.group(1)}: {typed.group(2)}"
    return reason


def measure() -> dict:
    with tempfile.TemporaryDirectory() as directory:
        results = parse_all(Path(directory))
    definitions = sorted(
        (item for item in results if isinstance(item, WebIDL.IDLInterfaceOrNamespace)),
        key=lambda item: item.identifier.name,
    )
    own_errors = {}
    for definition in definitions:
        if getattr(definition, "iterableInterface", None) is not None:
            continue
        try:
            CGV8BindingRoot(definition).define()
        except TypeError as error:
            own_errors[definition.identifier.name] = str(error)
    supported = []
    reasons = Counter()
    rejected = {}
    for definition in definitions:
        name = definition.identifier.name
        error = own_errors.get(name)
        owner = getattr(definition, "iterableInterface", None)
        if owner is not None:
            # The runtime implements the iterator objects of its owner's pair iterable.
            owner_error = own_errors.get(owner.identifier.name)
            error = owner_error and f"V8 backend requires the iterable's interface to generate: {name}: {owner.identifier.name}"
        if error is None:
            # A child binding installs on its parent's binding, so it is only usable once
            # every ancestor generates too.
            ancestor = getattr(definition, "parent", None)
            while ancestor is not None and ancestor.identifier.name not in own_errors:
                ancestor = ancestor.parent
            if ancestor is not None:
                error = f"V8 backend requires every ancestor to generate: {name}: {ancestor.identifier.name}"
        if error is None:
            supported.append(name)
        else:
            reasons[bucket(error)] += 1
            rejected[name] = error
    return {
        "total": len(definitions),
        "supported": supported,
        "rejected": rejected,
        "reasons": reasons.most_common(),
    }


def main() -> None:
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--json", action="store_true", help="print the full report as JSON")
    cli.add_argument("--top", type=int, default=20, help="number of blocker buckets to print")
    args = cli.parse_args()
    report = measure()
    if args.json:
        print(json.dumps(report, indent=2))
        return
    total, supported = report["total"], report["supported"]
    print(f"V8 backend coverage: {len(supported)}/{total} WebIDL definitions")
    print("Supported: " + (", ".join(supported) or "(none)"))
    print(f"Top {args.top} blockers (first unsupported shape per definition):")
    for reason, count in report["reasons"][: args.top]:
        print(f"{count:5}  {reason}")


if __name__ == "__main__":
    main()
