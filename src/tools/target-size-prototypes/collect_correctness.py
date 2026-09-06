"""Summarize successful compiler checks without treating overlap timings as benchmarks."""
import hashlib
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_rust as harness

ROOT = harness.ROOT


def collect(variant):
    data = ROOT / 'measurements' / harness.VARIANTS[variant]
    build_path = ROOT / 'logs' / (variant + '-status.json')
    build = json.loads(build_path.read_text())
    assert build['returncode'] == 0
    result = {'compiler': harness.version(variant), 'build': build,
              'bootstrap_environment': {'__CARGO_OHM_DEFAULTS': 'no',
                                        'LD_LIBRARY_PATH': str(ROOT / 'llvm/lib')},
              'notes': ['Correctness checks ran while independent compiler variants built; their wall times are excluded from performance claims.'],
              'real_workloads': []}
    for fixture in harness.FIXTURES:
        for profile in ['check', 'dev']:
            path = data / (fixture + '-' + profile) / 'correctness'
            invocation = json.loads((path / 'invocation.json').read_text())
            rows = json.loads((path / 'results.json').read_text())
            assert len(rows) == 7 and all(row['returncode'] == 0 for row in rows)
            result['real_workloads'].append({'fixture': fixture, 'profile': profile,
                'revisions_passed': 7, 'verification': True, 'command': invocation['command'],
                'receipt_sha256': harness.sha(path / 'results.json'),
                'harness_sha256': invocation['manifest'].get('effective_harness_sha256')})
    regression_path = data / 'regressions/results.json'
    regression = json.loads(regression_path.read_text())
    assert len(regression['results']) == 14
    result['regressions'] = []
    for row in regression['results']:
        assert row['returncode'] == 0
        log = data / 'regressions' / (row['name'] + '-r' + str(row['revision']) + '.stderr')
        assert harness.sha(log) == row['stderr_sha256']
        aliases = re.findall(r'Retained allocation payload aliases: (\d+)', log.read_text())
        result['regressions'].append({'name': row['name'], 'revision': row['revision'],
            'compiler_returncode': 0, 'executable_returncode': 0,
            'forwarded_allocation_count': int(aliases[-1]) if aliases else None,
            'storage': row['storage'], 'stderr_sha256': row['stderr_sha256']})
    result['regression_receipt_sha256'] = harness.sha(regression_path)
    if variant != 'baseline':
        log = ROOT / 'logs/correctness' / (variant + '-unit.log')
        lines = log.read_text().splitlines()
        summary = [line for line in lines if 'test result:' in line]
        assert any('1 passed; 0 failed' in line for line in summary), summary
        result['unit_test'] = {'summary': summary, 'log_sha256': harness.sha(log)}
    return result


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('variant', choices=['dep-fingerprints', 'allocation-aliases'])
    args = parser.parse_args()
    result = {'candidate': collect(args.variant), 'baseline': collect('baseline')}
    harness.write_json(ROOT / 'measurements' / (args.variant + '-correctness.json'), result)
