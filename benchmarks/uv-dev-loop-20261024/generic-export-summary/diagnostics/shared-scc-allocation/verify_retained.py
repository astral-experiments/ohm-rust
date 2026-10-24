"""Verify the four retained files and their bound byte ranges; no compiler runs."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent
MASK = (1 << 128) - 1


def usize(raw, pos):
    value = shift = 0
    while True:
        byte = raw[pos]
        pos += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
        shift += 7
        assert shift < 64


def string(raw, pos):
    size, pos = usize(raw, pos)
    value = raw[pos:pos + size].decode()
    assert raw[pos + size] == 0xc1
    return value, pos + size + 1


def fingerprints(raw, table):
    """Pinned Option<Fingerprint> table encoding, including sparse indices."""
    width, length, pos = table['byte_width'], table['length'], table['position']
    sparse = bool(table['width'] & 128)
    indices = list(range(length))
    if sparse:
        indices = []
        for bucket in range((length + 63) // 64):
            offset = pos + bucket * 12
            bitmap = int.from_bytes(raw[offset:offset + 8], 'little')
            preceding = int.from_bytes(raw[offset + 8:offset + 12], 'little')
            assert preceding == len(indices)
            assert bitmap >> min(64, length - bucket * 64) == 0
            indices.extend(bucket * 64 + bit for bit in range(64) if bitmap >> bit & 1)
        pos += ((length + 63) // 64) * 12
    assert len(indices) == table['stored_entries'] and width <= 17
    result = [None] * length
    for entry, index in enumerate(indices):
        fragment = raw[pos + entry * width:pos + (entry + 1) * width].ljust(17, b'\0')
        assert fragment[0] in (0, 1)
        if fragment[0]:
            result[index] = int.from_bytes(fragment[1:], 'little')
        else:
            assert not any(fragment)
    return result


for line in (ROOT / 'SHA256SUMS').read_text().splitlines():
    expected, name = line.split('  ', 1)
    assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == expected, name
data = json.loads((ROOT / 'findings.json').read_text())
decoded = {}
for case in data['cases']:
    raw = (ROOT / case['metadata_path']).read_bytes()
    assert len(raw) == case['rmeta_bytes']
    assert hashlib.sha256(raw).hexdigest() == case['rmeta_sha256'] == case['confirmation_rmeta_sha256']
    assert raw[:8] == b'rust\0\0\0\x0a' and raw.endswith(b'rust-end-file')
    assert string(raw, 16)[0] == case['compiler_version_string']
    pos = int.from_bytes(raw[8:16], 'little')
    assert pos == case['root_position'] and raw[pos] == 0
    triple, pos = string(raw, pos + 1)
    assert triple == 'x86_64-unknown-linux-gnu'
    assert raw[pos:pos + 16][::-1].hex() == case['private_hash']
    assert raw[pos + 16:pos + 32][::-1].hex() == case['public_hash']
    offset = case['public_global_hash_offset']
    assert raw[offset:offset + 16][::-1].hex() == case['public_global_hash']
    if case['summary_bit'] is not None:
        assert case['summary_bit'] is False and raw[case['summary_bit_offset']] == 0
    positions, pos = [], case['allocation_index']['position']
    for _ in range(case['allocation_index']['length']):
        value, pos = usize(raw, pos)
        positions.append(value)
    assert positions == [record['offset'] for record in case['allocation_records']]
    for record in case['allocation_records']:
        fragment = raw[record['offset']:record['offset'] + record['size']]
        # Memory, immutable/alignment-one, 19 bytes, no provenance, initialized lazy mask.
        assert fragment == b'\x00\x00\x13gzip, deflate, zstd\x00\x00\x01\x13'
        assert hashlib.sha256(fragment).hexdigest() == record['sha256']
    values = {}
    for kind in ('local', 'expn'):
        values[kind] = fingerprints(raw, case['public_tables'][kind])
        assert values[kind] == [int(x, 16) if x is not None else None
                                for x in case['public_fingerprints'][kind]]
    decoded[case['case']] = values
    table = case['optimized_mir_table']
    assert not table['index_bytes']
    pos, width = table['position'], table['byte_width']
    mir_indices = [i for i in range(table['length'])
                   if any(raw[pos + i * width:pos + (i + 1) * width])]
    assert mir_indices == case['encoded_optimized_mir_def_indices'] == [3, 4]
    text = (ROOT / case['listing_path']).read_text()
    assert re.search(r'^private hash ([0-9a-f]+)', text, re.M)[1] == case['private_hash']
    assert re.search(r'^public_hash ([0-9a-f]+)', text, re.M)[1] == case['public_hash']
    assert int(re.search(r'StableCrateId\((\d+)\)', text)[1]) == case['stable_crate_id']

for arm in ('baseline', 'candidate'):
    cold, warm = [next(x for x in data['cases'] if x['case'] == arm + '-' + history)
                  for history in ('cold', 'warmed')]
    for key in ('private_hash', 'public_global_hash', 'stable_crate_id', 'summary_bit', 'consumer_sha256'):
        assert cold[key] == warm[key]
    assert cold['public_hash'] != warm['public_hash']
    a, b = [(ROOT / row['listing_path']).read_text() for row in (cold, warm)]
    for label in ('public_hash ', 'Public hash: '):
        old, new = label + cold['public_hash'], label + warm['public_hash']
        assert a.count(old) == 1
        a = a.replace(old, new)
    assert a == b
    assert len(cold['allocation_records']) == 1 and len(warm['allocation_records']) == 2
    before, after = decoded[cold['case']], decoded[warm['case']]
    assert before['expn'] == after['expn']
    deltas = {i: (y - x) & MASK for i, (x, y) in enumerate(zip(before['local'], after['local']))
              if x is not None and y is not None and x != y}
    expected = int(data['warmed_minus_cold_delta_modulo_2_128'], 16)
    assert deltas == {0: expected, 3: expected, 4: expected}
    assert before['local'][3] == before['local'][4] and after['local'][3] == after['local'][4]
    print(f'{arm}: 1 -> 2 allocations; local indices 0,3,4 delta {expected:032x}')
print('All published byte/hash joins pass. No compiler or full MIR-edge decoder was invoked.')
