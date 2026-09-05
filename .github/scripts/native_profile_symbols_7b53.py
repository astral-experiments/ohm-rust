#!/usr/bin/env python3
"""Summarize interrupted-worker PC samples without making timing comparisons."""

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import subprocess


def symbolize(offsets, driver, diagnostics):
    pending = sorted(offsets)
    position = 0
    previous = None
    counts = Counter()
    unknown = 0

    def finish_before(address):
        nonlocal position, unknown
        while position < len(pending) and pending[position] < address:
            offset = pending[position]
            if previous is not None and previous[0] <= offset < previous[0] + previous[1]:
                counts[previous[2]] += offsets[offset]
            else:
                unknown += offsets[offset]
            position += 1

    with diagnostics.open('w') as stderr:
        process = subprocess.Popen(
            ['nm', '--numeric-sort', '--print-size', '--defined-only', str(driver)],
            stdout=subprocess.PIPE, stderr=stderr, text=True, errors='replace',
        )
        try:
            for line in process.stdout:
                fields = line.split(maxsplit=3)
                if len(fields) != 4 or fields[2] not in ('t', 'T', 'w', 'W'):
                    continue
                address, size = int(fields[0], 16), int(fields[1], 16)
                if not size:
                    continue
                finish_before(address)
                previous = (address, size, fields[3].strip())
            finish_before(1 << 64)
            if process.wait(timeout=30):
                raise RuntimeError('nm failed; see nm-status.txt')
        finally:
            process.stdout.close()
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)
    return counts, unknown


def symbol_family(name):
    plain = re.sub(r'\[[0-9a-f]+\]', '', name)
    if '::query::caches::' in plain or 'DefIdCache' in plain:
        return 'query_cache'
    if any(part in plain for part in ('CtxtInterners', 'InternedInSet', '::intern_with',
                                      'rustc_middle::ty::context::TyCtxt>::mk_')):
        return 'interner'
    if (plain.startswith(('mi_', '_mi_', '__rust_alloc', '__rust_dealloc', '__rust_realloc'))
            or any(part in plain for part in ('mimalloc', 'alloc::alloc::', 'rustc_arena::'))
            or plain in ('malloc', 'free', 'calloc', 'realloc')):
        return 'allocator'
    if any(part in plain for part in ('hashbrown::', 'rustc_hash::', '::hashers::',
                                      'rustc_data_structures::sharded::')):
        return 'hash_table'
    if any(part in plain for part in ('rustc_middle::query::', 'rustc_query_impl::',
                                      'rustc_query_system::query::')):
        return 'query_dispatch'
    return 'other'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('profile', type=Path)
    args = parser.parse_args()
    profile = args.profile.resolve()
    samples = json.loads((profile / 'samples.json').read_text())
    offsets = Counter(sample[3] for sample in samples['samples'] if sample[3] is not None)
    counts, unknown = symbolize(offsets, Path(samples['driver']['path']), profile / 'nm-status.txt')
    names = list(counts)
    if names:
        process = subprocess.run(
            ['c++filt', '--format=rust'], input='\n'.join(names) + '\n',
            text=True, capture_output=True, check=True, timeout=30,
        )
        demangled = process.stdout.splitlines()
        if len(demangled) != len(names):
            raise RuntimeError('Demangler returned an unexpected number of symbols')
        decoded = dict(zip(names, demangled))
    else:
        decoded = {}
    driver_count = sum(offsets.values())
    symbols = [
        {'samples': count, 'share_of_driver_samples': count / driver_count,
         'symbol': decoded[name], 'mangled': name, 'family': symbol_family(decoded[name])}
        for name, count in counts.most_common()
    ]
    crates = Counter()
    families = Counter()
    for symbol in symbols:
        matched = re.search(r'(rustc_[A-Za-z0-9_]+)(?:\[[0-9a-f]+\])?::', symbol['symbol'])
        crates[matched.group(1) if matched else '<other>'] += symbol['samples']
        families[symbol['family']] += symbol['samples']
    result = {
        'description': 'Random runnable worker instruction pointers; no timing comparison',
        'fixture': samples['fixture'], 'source_revision': samples['source_revision'],
        'architecture': samples['architecture'], 'rustc_version': samples['rustc_version'],
        'driver': samples['driver'], 'error': samples['error'],
        'cleanup_complete': samples.get('cleanup_complete'),
        'cleanup_error': samples.get('cleanup_error'),
        'samples': len(samples['samples']), 'driver_samples': driver_count,
        'outside_driver_samples': len(samples['samples']) - driver_count,
        'unsymbolized_driver_samples': unknown, 'symbols': symbols,
        'compiler_crates': crates.most_common(), 'symbol_families': families.most_common(),
        'family_grouping_note': 'Disjoint leaf-symbol name groups; generic hashing and allocation cannot be assigned to callers without stacks',
    }
    (profile / 'symbol-summary.json').write_text(json.dumps(result, indent=2) + '\n')
    lines = [result['description'],
             f"Fixture: {result['fixture']}; architecture: {result['architecture']}",
             f"Samples: {result['samples']}; driver: {driver_count}; unsymbolized driver: {unknown}",
             f"Sampler error: {result['error']}; cleanup complete: {result['cleanup_complete']}",
             '', 'Count   Driver sample share   Symbol']
    lines.extend(f"{entry['samples']:6d}  {entry['share_of_driver_samples']:8.2%}  {entry['symbol']}"
                 for entry in symbols[:100])
    lines.extend(['', 'Leaf symbol family sample counts (no caller attribution):'])
    lines.extend(f'{count:6d}  {name}' for name, count in families.most_common())
    lines.extend(['', 'Compiler crate sample counts:'])
    lines.extend(f'{count:6d}  {name}' for name, count in crates.most_common())
    report = '\n'.join(lines) + '\n'
    (profile / 'symbol-summary.txt').write_text(report)
    print(report)


if __name__ == '__main__':
    main()
