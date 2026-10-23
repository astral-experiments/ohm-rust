from pathlib import Path
import hashlib
import importlib.util
import json
import math

root = Path(__file__).resolve().parent
manifest = json.loads((root / 'CONTEXT-MANIFEST.json').read_text())
for name, row in manifest['files'].items():
    data = (root / name).read_bytes()
    assert len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256']
core = root.parent
assert hashlib.sha256((core / 'CORE-MANIFEST.json').read_bytes()).hexdigest() == manifest['core_manifest_sha256']
spec = importlib.util.spec_from_file_location('core_reader', core / 'reproduce.py')
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)
data = json.loads((root / 'native-check-times.json').read_text())
summaries = []
for block in data['blocks']:
    index = block['block']
    expected = json.loads((root / f'native-check-block-{index}.summary.json').read_text())
    assert len(block['pairs']) == 8 and len(block['calls']) == 16
    for call in block['calls']:
        assert call['elapsed_ns'] == block['pairs'][call['pair']][call['variant']]
        assert call['returncode'] == 0
    stats = reader.paired_count_statistics(block['pairs'], confidence=0.99, resamples=200000, seed=2026102302 + index)
    assert stats == expected['statistics']
    assert expected['block_gate_passed'] == (stats['wins'] == 8 and stats['bootstrap_count_reduction_interval'][0] > 0)
    summaries.append(expected)
pairs = summaries[0]['pairs'] + summaries[1]['pairs']
pooled = reader.paired_count_statistics(pairs, confidence=0.99, resamples=200000, seed=2026102307)
combined = json.loads((root / 'native-check-combined.json').read_text())
assert pooled == combined['statistics']
losses = sum(row['candidate'] > row['baseline'] for row in pairs)
blocked = losses >= 12 and pooled['bootstrap_count_reduction_interval'][1] < -0.01
assert blocked == combined['regression_blocker']
assert combined['wall_time_claim_gate'] == all(row['block_gate_passed'] for row in summaries)
assert combined['inconclusive'] and not blocked and not combined['wall_time_claim_gate']
discovery = json.loads((root / 'discovery-summary.json').read_text())
metrics = json.loads((root / 'discovery-metric-integrity.json').read_text())['profiles']
assert [x['order'] for x in discovery['pairs']] == ['AB','BA','BA','AB'] and len(metrics) == 8
for pair_index, pair in enumerate(discovery['pairs']):
    for position, letter in enumerate(pair['order']):
        arm = 'baseline' if letter == 'A' else 'candidate'
        row, = [r for r in metrics if r['label'] == f'pair-{pair_index}-{position}-{arm}']
        assert pair[arm] == row['final_ir'] == row['self_cost_sum'] == row['stderr_collected']
reduction = 1 - math.exp(sum(math.log(p['candidate'] / p['baseline']) for p in discovery['pairs']) / 4)
assert reduction == discovery['geometric_reduction']
assert discovery['block_gate_passed'] == (all(p['candidate'] < p['baseline'] for p in discovery['pairs']) and reduction >= .05)
print(json.dumps({'native_check_geometric_reduction': pooled['paired_geometric_mean_count_reduction'],
                  'native_check_ci99': pooled['bootstrap_count_reduction_interval'],
                  'native_check_wins': pooled['wins'], 'native_check_losses': losses,
                  'native_check_inconclusive': True, 'discovery_geometric_reduction': reduction,
                  'discovery_kept_separate': True, 'private_collection_validation_replayed': False}, indent=2))
