"""Retain the counter rows, byte counts, timings, and provenance needed to review a prototype."""
import argparse
import hashlib
import json
import statistics
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
    parser.add_argument('--verdict', default='PENDING REVIEW')
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
        manifests[variant] = (cycles.get('effective_manifests', {}).get(variant)
                              or json.loads(path.read_text()))
    samples = []
    for cycle in cycles['results']:
        for result in cycle['result']:
            storage = result['storage']
            sample = {key: cycle[key] for key in ['variant', 'round', 'slot']}
            sample.update({key: result[key]
                           for key in ['revision', 'patched', 'elapsed_ns', 'returncode']})
            sample['dep_graph_counts'] = result.get('dep_graph_counts')
            sample['storage'] = {
                'total': {kind: sum(group[kind] for group in storage.values())
                          for kind in ['logical', 'physical', 'files']},
                **{name: storage[name]
                   for name in ['dep-graph.bin', 'query-cache.bin', '.rmeta', '.rlib', '.o']
                   if name in storage},
            }
            samples.append(sample)
    workflow = {}
    for variant in ['baseline', args.variant]:
        totals = [sum(row['elapsed_ns'] for row in cycle['result'])
                  for cycle in cycles['results'] if cycle['variant'] == variant]
        workflow[variant] = {'samples_ns': totals, 'median_ns': statistics.median(totals),
                             'min_ns': min(totals), 'max_ns': max(totals)}
    workflow['median_ratio'] = (workflow[args.variant]['median_ns']
                                / workflow['baseline']['median_ns'])
    instruction_summary = {}
    for revision in sorted({row['revision'] for row in counts}):
        arms = {}
        for variant in ['baseline', args.variant]:
            values = [row['events']['Ir'] for row in counts
                      if row['revision'] == revision and row['variant'] == variant]
            arms[variant] = {'samples': values, 'median': statistics.median(values),
                             'min': min(values), 'max': max(values)}
        arms['median_ratio'] = arms[args.variant]['median'] / arms['baseline']['median']
        instruction_summary[str(revision)] = arms
    result = {
        'schema': 1,
        'variant': args.variant,
        'fixture': args.fixture,
        'profile': args.profile,
        'verdict': args.verdict,
        'cachegrind_measured': bool(counts),
        'host_receipt_sha256': hashlib.sha256(
            (ROOT / 'measurements/host.json').read_bytes()).hexdigest(),
        'measurement_notes': [
            'Dependencies prepared outside measured direct compiler invocations, '
            'separately per compiler.',
            'Cycles start empty; revisions are original, original, body patch, '
            'body patch, original, original, body patch.',
            'Walltime uses warmup and ABBA/BAAB cycles; Cachegrind uses recreated '
            'prior state and AB/BA order.',
            'Cachegrind instruction counts include the complete compiler process; '
            'no cache or branch simulation.',
            'Native timing runs were isolated from other benchmark and compiler '
            'work on the dedicated VM.',
            'Cachegrind may overlap independent fixture jobs after all native '
            'timings completed; its elapsed times are diagnostic only.',
            'Physical bytes count st_blocks once per inode within each private '
            'target; reflink sharing is not inferred.',
            'Compiler binaries and matching standard libraries built without '
            'PGO/BOLT; see build configuration.',
            'Native and Cachegrind commands exclude incremental hash verification; '
            'separate correctness cycles include it.',
        ],
        'manifests': manifests,
        'timing_summary': cycles['summary'],
        'seven_revision_compiler_time': workflow,
        'samples': samples,
        'instruction_summary': instruction_summary,
        'counter_rows': counts,
        'raw_receipt_hashes': {
            cycle_path.name: hashlib.sha256(cycle_path.read_bytes()).hexdigest(),
            **({counts_path.name: hashlib.sha256(counts_path.read_bytes()).hexdigest()}
               if counts_path.exists() else {}),
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # Keep each sample on one line while leaving provenance and summaries expanded.
    fields = []
    for key, value in result.items():
        if key in ['samples', 'counter_rows'] and value:
            rendered = '[\n' + ',\n'.join('    ' + json.dumps(row, separators=(',', ':'))
                                             for row in value) + '\n  ]'
        else:
            rendered = json.dumps(value, indent=2).replace('\n', '\n  ')
        fields.append('  ' + json.dumps(key) + ': ' + rendered)
    text = '{\n' + ',\n'.join(fields) + '\n}\n'
    assert json.loads(text) == result
    args.output.write_text(text)


if __name__ == '__main__':
    main()
