"""Replay all eight exploratory profiles without pooling them into confirmation."""
from pathlib import Path
import gzip
import hashlib
import importlib.util
import json

root = Path(__file__).resolve().parent
core = root.parent
manifest = json.loads((root / 'PROFILE-MANIFEST.json').read_text())
assert hashlib.sha256((core / 'CORE-MANIFEST.json').read_bytes()).hexdigest() == manifest['core_manifest_sha256']
assert len(manifest['profiles']) == 8
spec = importlib.util.spec_from_file_location('core_reader', core / 'reproduce.py')
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)
metrics = json.loads((core / 'context/discovery-metric-integrity.json').read_text())['profiles']
for metric in metrics:
    row = manifest['profiles']['discovery/' + metric['label']]
    compressed = (root / row['file']).read_bytes()
    assert hashlib.sha256(compressed).hexdigest() == row['sha256']
    raw = gzip.decompress(compressed)
    assert hashlib.sha256(raw).hexdigest() == row['raw_sha256']
    result = reader.final_ir(raw, row['counter_line'] + '\n', ['rustc', '<accepted-private-invocation>'])
    assert result == {key: value for key, value in metric.items() if key != 'label'}
print(json.dumps({'accepted_discovery_profiles': len(metrics), 'all_final_self_stderr_counts_exact': True,
                  'pooled_into_confirmation': False, 'private_collection_validation_replayed': False}))
