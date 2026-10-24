"""Verify published bytes and retained joins, without compiling or decoding metadata."""
from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parent
for line in (root / 'SHA256SUMS').read_text().splitlines():
    expected, name = line.split('  ', 1)
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, name
data = json.loads((root / 'findings.json').read_text())
for case in data['cases']:
    raw = (root / case['metadata_path']).read_bytes()
    assert len(raw) == case['rmeta_bytes']
    assert hashlib.sha256(raw).hexdigest() == case['rmeta_sha256']
    for record in case['allocation_records']:
        fragment = raw[record['offset']:record['offset'] + record['size']]
        assert hashlib.sha256(fragment).hexdigest() == record['sha256']
        assert bytes.fromhex(record['decoded']['payload_hex']) in fragment
    text = (root / case['listing_path']).read_text()
    assert re.search(r'^private hash ([0-9a-f]+)', text, re.M)[1] == case['private_hash']
    assert re.search(r'^public_hash ([0-9a-f]+)', text, re.M)[1] == case['public_hash']
    assert int(re.search(r'StableCrateId\((\d+)\)', text)[1]) == case['stable_crate_id']
for arm in ('baseline', 'candidate'):
    cold, warmed = [next(x for x in data['cases'] if x['case'] == arm + '-' + h)
                    for h in ('cold', 'warmed')]
    assert (root / cold['listing_path']).read_bytes() == (root / warmed['listing_path']).read_bytes()
    for key in ('private_hash', 'public_hash', 'public_global_hash', 'stable_crate_id',
                'public_fingerprints', 'consumer_sha256'):
        assert cold[key] == warmed[key]
    assert len(cold['allocation_records']) == 1
    assert len(warmed['allocation_records']) == 2
print('Published byte/hash joins pass; no compiler or metadata decoder was invoked.')
