"""Retain the counter rows, byte counts, timings, and provenance needed to review a prototype."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path('/tmp/ohm-target-size-prototypes')
VARIANTS = {'baseline': '00', 'dep-fingerprints': '01', 'allocation-aliases': '02',
            'sparse-metadata': '03', 'cloned-prefixes': '04'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('variant', choices=VARIANTS.keys() - {'baseline'})
    parser.add_argument('--fixture', default='syn-2.0.101')
    parser.add_argument('--profile', default='check', choices=['check', 'dev'])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    leaf = args.fixture + '-' + args.profile
    prefix = args.variant + '-' + leaf
    cycle_path = ROOT / 'measurements' / (prefix + '-cycles.json')
    counts_path = ROOT / 'measurements' / (prefix + '-cachegrind.json')
    cycles = json.loads(cycle_path.read_text())
    counts = json.loads(counts_path.read_text()) if counts_path.exists() else []
    manifests = {}
    for variant in ['baseline', args.variant]:
        path = ROOT / 'measurements' / VARIANTS[variant] / leaf / 'manifest.json'
        manifests[variant] = cycles.get('effective_manifests', {}).get(variant) or json.loads(path.read_text())
    samples = []
    for cycle in cycles['results']:
        for result in cycle['result']:
            storage = result['storage']
            sample = {key: cycle[key] for key in ['variant', 'round', 'slot']}
            sample.update({key: result[key] for key in ['revision', 'patched', 'elapsed_ns', 'returncode']})
            sample['dep_graph_counts'] = result.get('dep_graph_counts')
            sample['storage'] = {
                'total': {kind: sum(group[kind] for group in storage.values())
                          for kind in ['logical', 'physical', 'files']},
                **{name: storage[name] for name in ['dep-graph.bin', 'query-cache.bin', '.rmeta', '.rlib', '.o']
                   if name in storage},
            }
            samples.append(sample)
    result = {
        'schema': 1,
        'variant': args.variant,
        'fixture': args.fixture,
        'profile': args.profile,
        'verdict': 'PENDING REVIEW',
        'cachegrind_measured': bool(counts),
        'host_receipt_sha256': hashlib.sha256((ROOT / 'measurements/host.json').read_bytes()).hexdigest(),
        'measurement_notes': [
            'Dependencies prepared outside measured direct compiler invocations, separately per compiler.',
            'Cycles start empty; revisions are original, original, body patch, body patch, original, original, body patch.',
            'Walltime uses warmup and ABBA/BAAB cycles; Cachegrind uses recreated prior state and AB/BA order.',
            'Cachegrind instruction counts include the complete compiler process; no cache or branch simulation.',
            'Physical bytes count st_blocks once per inode within each private target; reflink sharing is not inferred.',
            'Compiler binaries and matching standard libraries built without PGO/BOLT; see build configuration.',
            'Native and Cachegrind commands exclude incremental hash verification; separate correctness cycles include it.',
        ],
        'manifests': manifests,
        'timing_summary': cycles['summary'],
        'samples': samples,
        'counter_rows': counts,
        'raw_receipt_hashes': {
            cycle_path.name: hashlib.sha256(cycle_path.read_bytes()).hexdigest(),
            **({counts_path.name: hashlib.sha256(counts_path.read_bytes()).hexdigest()} if counts_path.exists() else {}),
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
