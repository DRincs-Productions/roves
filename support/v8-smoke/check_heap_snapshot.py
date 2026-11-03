#!/usr/bin/env python3
"""Checks a V8 heap snapshot written by a debug servoshell (see frame-retention.html).

    check_heap_snapshot.py SNAPSHOT [REQUIRED-STRING...]

The file must be V8's complete `.heapsnapshot` JSON. Besides checking its structure and scene
strings, an optional marker check counts owners of one tagged retention probe: judged against
--expected-tagged-probes, or only reported with --report-tagged-probes. --count-node-name NAME
reports how many nodes carry exactly that name (diagnostic only, never judged). It does not trace
retainer paths or establish that unrelated objects are free of retention. Prints one annotation.
"""

import argparse
import json
import sys


def tagged_probe_count(data, property_name, marker):
    """Count owners whose named property points at the retention-scene marker string."""
    meta = data['snapshot']['meta']
    node_fields, edge_fields = meta['node_fields'], meta['edge_fields']
    node_types, edge_types = meta['node_types'][0], meta['edge_types'][0]
    nodes, edges, strings = data['nodes'], data['edges'], data['strings']
    node_width, edge_width = len(node_fields), len(edge_fields)
    if not node_width or not edge_width:
        raise ValueError('node and edge metadata must declare fields')
    if not all(name in node_fields for name in ('type', 'name', 'edge_count')):
        raise ValueError('node metadata is missing type, name, or edge_count')
    if not all(name in edge_fields for name in ('type', 'name_or_index', 'to_node')):
        raise ValueError('edge metadata is missing type, name_or_index, or to_node')
    if not node_types or not edge_types:
        raise ValueError('node and edge metadata must declare type names')
    node_index = {name: node_fields.index(name) for name in ('type', 'name', 'edge_count')}
    edge_index = {name: edge_fields.index(name) for name in ('type', 'name_or_index', 'to_node')}
    node_count = data['snapshot']['node_count']
    if len(nodes) % node_width:
        raise ValueError(f'{len(nodes)} node fields are not divisible by width {node_width}')
    if len(edges) % edge_width:
        raise ValueError(f'{len(edges)} edge fields are not divisible by width {edge_width}')

    def node_at(offset):
        if offset < 0 or offset % node_width or offset + node_width > len(nodes):
            raise ValueError(f'edge target node offset {offset} is invalid')
        row = nodes[offset:offset + node_width]
        type_index, name_index = row[node_index['type']], row[node_index['name']]
        if type_index < 0 or type_index >= len(node_types) or name_index < 0 or name_index >= len(strings):
            raise ValueError(f'node at offset {offset} has an invalid type or string index')
        return node_types[type_index], strings[name_index]

    edge_offset = 0
    matches = 0
    for node_offset in range(0, len(nodes), node_width):
        row = nodes[node_offset:node_offset + node_width]
        edge_count = row[node_index['edge_count']]
        if edge_count < 0:
            raise ValueError(f'node at offset {node_offset} has a negative edge count')
        for _ in range(edge_count):
            edge = edges[edge_offset:edge_offset + edge_width]
            if len(edge) != edge_width:
                raise ValueError('node edge counts exceed the edge table')
            edge_offset += edge_width
            edge_type, name_or_index = edge[edge_index['type']], edge[edge_index['name_or_index']]
            if edge_type < 0 or edge_type >= len(edge_types):
                raise ValueError('edge has an invalid type index')
            if edge_types[edge_type] != 'property':
                continue
            if name_or_index < 0 or name_or_index >= len(strings) or strings[name_or_index] != property_name:
                continue
            target_type, target_name = node_at(edge[edge_index['to_node']])
            if target_type == 'string' and target_name == marker:
                matches += 1
    if edge_offset != len(edges):
        raise ValueError(f'node edge counts cover {edge_offset // edge_width} of {len(edges) // edge_width} edges')
    if len(nodes) != node_count * node_width:
        raise ValueError('node table changed while checking tagged probes')
    return matches


def named_node_count(data, name):
    """Count nodes whose name is exactly `name` (any node type); no retention judgement."""
    node_fields = data['snapshot']['meta']['node_fields']
    if 'name' not in node_fields:
        raise ValueError('node metadata is missing name')
    width, offset = len(node_fields), node_fields.index('name')
    indices = {index for index, string in enumerate(data['strings']) if string == name}
    nodes = data['nodes']
    return sum(1 for start in range(offset, len(nodes), width) if nodes[start] in indices)


def analyze(path, required, expected_tagged_probes=None,
            tag_property='rovesProbeTag', tag_marker='roves-document-retention-probe',
            report_tagged_probes=False, node_names=()):
    """Problems and reported metrics: {'tagged': count (when requested), 'named': {name: count}}."""
    metrics = {}
    try:
        with open(path, encoding='utf-8') as file:
            data = json.load(file)
        meta = data['snapshot']['meta']
        nodes, strings = data['nodes'], data['strings']
        node_count = data['snapshot']['node_count']
        width = len(meta['node_fields'])
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f'not a V8 heap snapshot: {type(error).__name__}: {error}'], metrics
    found = []
    if node_count <= 0 or len(nodes) != node_count * width:
        found.append(f'{len(nodes)} node fields for {node_count} nodes of {width} fields')
    present = set(strings)
    found += [f'missing string {name!r}' for name in required if name not in present]
    if (expected_tagged_probes is not None or report_tagged_probes) and not found:
        try:
            metrics['tagged'] = tagged_probe_count(data, tag_property, tag_marker)
        except (IndexError, KeyError, TypeError, ValueError) as error:
            found.append(f'invalid tagged-probe graph: {type(error).__name__}: {error}')
        else:
            if expected_tagged_probes is not None and metrics['tagged'] != expected_tagged_probes:
                found.append(
                    f'expected {expected_tagged_probes} tagged probe object(s) with '
                    f'{tag_property!r} -> {tag_marker!r}, found {metrics["tagged"]}')
    if node_names and not found:
        try:
            metrics['named'] = {name: named_node_count(data, name) for name in node_names}
        except (KeyError, TypeError, ValueError) as error:
            found.append(f'invalid node table: {type(error).__name__}: {error}')
    return found, metrics


def problems(path, required, expected_tagged_probes=None,
             tag_property='rovesProbeTag', tag_marker='roves-document-retention-probe'):
    return analyze(path, required, expected_tagged_probes, tag_property, tag_marker)[0]


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--expected-tagged-probes', type=int)
    parser.add_argument('--report-tagged-probes', action='store_true')
    parser.add_argument('--tag-property', default='rovesProbeTag')
    parser.add_argument('--tag-marker', default='roves-document-retention-probe')
    parser.add_argument('--count-node-name', action='append', default=[])
    parser.add_argument('snapshot')
    parser.add_argument('required', nargs='*')
    args = parser.parse_args(argv[1:])
    found, metrics = analyze(args.snapshot, args.required, args.expected_tagged_probes,
                             args.tag_property, args.tag_marker, args.report_tagged_probes,
                             args.count_node_name)
    if found:
        print(f'::error::heap snapshot {args.snapshot}: ' + '; '.join(found))
        return 1
    detail = ', '.join(map(repr, args.required))
    if args.expected_tagged_probes is not None:
        detail += f'; {args.expected_tagged_probes} tagged retention probe(s)'
    elif 'tagged' in metrics:
        detail += f'; {metrics["tagged"]} tagged retention probe(s) (reported, not judged)'
    for name, count in metrics.get('named', {}).items():
        detail += f'; {count} node(s) named {name!r} (diagnostic, not judged)'
    print(f'::notice::heap snapshot {args.snapshot}: valid V8 snapshot with ' + detail)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
