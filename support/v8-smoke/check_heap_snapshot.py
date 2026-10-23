#!/usr/bin/env python3
"""Checks a V8 heap snapshot written by a debug servoshell (see frame-retention.html).

    check_heap_snapshot.py SNAPSHOT [REQUIRED-STRING...]

The file must be V8's complete `.heapsnapshot` JSON: a node table matching the node count its
metadata declares, and a string table containing every required string. That identifies the
scene; it says nothing about which objects are retained. Prints one GitHub annotation.
"""

import json
import sys


def problems(path, required):
    try:
        with open(path, encoding='utf-8') as file:
            data = json.load(file)
        meta = data['snapshot']['meta']
        nodes, strings = data['nodes'], data['strings']
        node_count = data['snapshot']['node_count']
        width = len(meta['node_fields'])
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f'not a V8 heap snapshot: {type(error).__name__}: {error}']
    found = []
    if node_count <= 0 or len(nodes) != node_count * width:
        found.append(f'{len(nodes)} node fields for {node_count} nodes of {width} fields')
    present = set(strings)
    found += [f'missing string {name!r}' for name in required if name not in present]
    return found


def main(argv):
    path, required = argv[1], argv[2:]
    found = problems(path, required)
    if found:
        print(f'::error::heap snapshot {path}: ' + '; '.join(found))
        return 1
    print(f'::notice::heap snapshot {path}: valid V8 snapshot with ' + ', '.join(map(repr, required)))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
