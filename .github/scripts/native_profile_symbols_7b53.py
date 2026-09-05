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
    resolved = {}

    def finish_before(address):
        nonlocal position, unknown
        while position < len(pending) and pending[position] < address:
            offset = pending[position]
            if previous is not None and previous[0] <= offset < previous[0] + previous[1]:
                counts[previous[2]] += offsets[offset]
                resolved[offset] = previous[2]
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
    return counts, unknown, resolved


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


FOLD_FAMILIES = {
    'arg_folder': '::binder::ArgFolder',
    'opportunistic_var_resolver': '::resolve::OpportunisticVarResolver',
    'canonicalizer': '::canonicalizer::Canonicalizer',
    'region_folder': '::fold::RegionFolder',
    'type_freshener': '::freshen::TypeFreshener',
}


def fold_family_counts(samples, leaf_names, caller_names, decoded):
    """Count each family once per sample, even through recursive/nested helpers."""
    masks = {
        name: sum(1 << index for index, needle in enumerate(FOLD_FAMILIES.values())
                  if needle in display)
        for name, display in decoded.items()
    }
    leaf_counts = [0] * len(FOLD_FAMILIES)
    transitive_counts = [0] * len(FOLD_FAMILIES)
    any_transitive = 0
    for sample in samples:
        leaf_mask = masks.get(leaf_names.get(sample[3]), 0)
        transitive_mask = leaf_mask
        if len(sample) > 7:
            # OR is the per-sample union, including repeated frames of one function
            # and different generic helpers from the same family.
            for offset in sample[7]:
                transitive_mask |= masks.get(caller_names.get(offset), 0)
        any_transitive += bool(transitive_mask)
        for index in range(len(FOLD_FAMILIES)):
            leaf_counts[index] += bool(leaf_mask & (1 << index))
            transitive_counts[index] += bool(transitive_mask & (1 << index))
    total = len(samples)
    return {
        'note': 'Each family counts at most once per sample across the leaf and captured '
                'stack. Families can overlap; missing frames can undercount. Shares use '
                'all samples, including samples outside the driver.',
        'families': [
            {'family': family, 'leaf_samples': leaf_counts[index],
             'transitive_samples': transitive_counts[index],
             'leaf_share_of_all_samples': leaf_counts[index] / total if total else 0,
             'transitive_share_of_all_samples': transitive_counts[index] / total if total else 0}
            for index, family in enumerate(FOLD_FAMILIES)
        ],
        'any_family_transitive_samples': any_transitive,
        'any_family_transitive_share_of_all_samples': any_transitive / total if total else 0,
    }


def compact_symbol_map(leaf_names, caller_names, link_names, decoded):
    """Retain only observed offsets, sharing names across all three sample roles."""
    resolved = {}
    for role in (leaf_names, caller_names, link_names):
        for offset, name in role.items():
            previous = resolved.setdefault(offset, name)
            if previous != name:
                raise RuntimeError(f'Inconsistent symbols for driver offset {offset:#x}')
    names = sorted(set(resolved.values()))
    indices = {name: index for index, name in enumerate(names)}
    return {
        'symbol_columns': ['mangled', 'demangled'],
        'symbols': [[name, decoded[name]] for name in names],
        'offset_columns': ['driver_offset', 'symbol_index'],
        'offsets': [[offset, indices[name]] for offset, name in sorted(resolved.items())],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('profile', type=Path)
    args = parser.parse_args()
    profile = args.profile.resolve()
    samples = json.loads((profile / 'samples.json').read_text())
    offsets = Counter(sample[3] for sample in samples['samples'] if sample[3] is not None)
    counts, unknown, leaf_names = symbolize(offsets, Path(samples['driver']['path']), profile / 'nm-status.txt')
    caller_offsets = Counter(offset for sample in samples['samples'] if len(sample) > 7
                             for offset in set(sample[7]))
    caller_counts, caller_unknown, caller_names = symbolize(
        caller_offsets, Path(samples['driver']['path']), profile / 'nm-callers-status.txt')
    link_offsets = Counter(sample[8] for sample in samples['samples']
                           if len(sample) > 8 and sample[8] is not None)
    link_counts, link_unknown, link_names = symbolize(
        link_offsets, Path(samples['driver']['path']), profile / 'nm-links-status.txt')
    names = list(counts.keys() | caller_counts.keys() | link_counts.keys())
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
    # Count each caller function only once per sample, including recursive stacks.
    caller_counts = Counter(name for sample in samples['samples'] if len(sample) > 7
                            for name in {caller_names[offset] for offset in sample[7]
                                         if offset in caller_names})
    atomic_callers = Counter()
    outside_callers = Counter()
    for sample in samples['samples']:
        if len(sample) <= 8:
            continue
        leaf = decoded.get(leaf_names.get(sample[3]), '')
        if leaf.startswith('__aarch64_') and sample[8] in link_names:
            atomic_callers[link_names[sample[8]]] += 1
        if sample[3] is None:
            caller = next((caller_names[offset] for offset in sample[7]
                           if offset in caller_names), None)
            if caller is not None:
                outside_callers[caller] += 1
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
        'caller_samples': sum(caller_offsets.values()),
        'fold_family_counts': fold_family_counts(
            samples['samples'], leaf_names, caller_names, decoded),
        'atomic_callers': [{'samples': count, 'symbol': decoded[name]}
                           for name, count in atomic_callers.most_common()],
        'outside_driver_callers': [{'samples': count, 'symbol': decoded[name]}
                                   for name, count in outside_callers.most_common()],
        'unsymbolized_caller_samples': caller_unknown,
        'unsymbolized_link_register_samples': link_unknown,
        'callers': [{'samples': count, 'symbol': decoded[name]}
                    for name, count in caller_counts.most_common()],
        'link_registers': [{'samples': count, 'symbol': decoded[name]}
                           for name, count in link_counts.most_common()],
        'stack_note': 'Frame-pointer chains only; may be incomplete. Each caller function counts once per sample. ARM link register may point within the current function, except when sampled in a leaf atomic helper.',
        'compiler_crates': crates.most_common(), 'symbol_families': families.most_common(),
        'family_grouping_note': 'Disjoint leaf-symbol name groups; generic hashing and allocation cannot be assigned to callers without stacks',
    }
    (profile / 'symbol-summary.json').write_text(json.dumps(result, indent=2) + '\n')
    offset_map = {
        'schema_version': 1,
        'fixture': samples['fixture'],
        'source_revision': samples['source_revision'],
        'driver': samples['driver'],
        **compact_symbol_map(leaf_names, caller_names, link_names, decoded),
    }
    (profile / 'offset-symbols.json').write_text(
        json.dumps(offset_map, separators=(',', ':')) + '\n')
    lines = [result['description'],
             f"Fixture: {result['fixture']}; architecture: {result['architecture']}",
             f"Samples: {result['samples']}; driver: {driver_count}; unsymbolized driver: {unknown}",
             f"Sampler error: {result['error']}; cleanup complete: {result['cleanup_complete']}",
             '', 'Count   Driver sample share   Symbol']
    lines.extend(f"{entry['samples']:6d}  {entry['share_of_driver_samples']:8.2%}  {entry['symbol']}"
                 for entry in symbols[:100])
    lines.extend(['', 'Leaf symbol family sample counts (no caller attribution):'])
    lines.extend(f'{count:6d}  {name}' for name, count in families.most_common())
    lines.extend(['', 'Fold family per-sample unions (leaf + captured stack; all-sample shares):'])
    lines.extend(
        f"{entry['transitive_samples']:6d}  {entry['transitive_share_of_all_samples']:8.2%}  "
        f"{entry['family']} (leaf: {entry['leaf_samples']})"
        for entry in result['fold_family_counts']['families'])
    lines.append(
        f"{result['fold_family_counts']['any_family_transitive_samples']:6d}  "
        f"{result['fold_family_counts']['any_family_transitive_share_of_all_samples']:8.2%}  "
        'any listed family (union)')
    lines.extend(['', 'Frame-pointer caller records (incomplete chains; counts overlap):'])
    lines.extend(f'{count:6d}  {decoded[name]}' for name, count in caller_counts.most_common(50))
    lines.extend(['', 'Callers of leaf atomic helpers:'])
    lines.extend(f'{count:6d}  {decoded[name]}' for name, count in atomic_callers.most_common(20))
    lines.extend(['', 'Nearest driver caller when sampled outside the driver:'])
    lines.extend(f'{count:6d}  {decoded[name]}' for name, count in outside_callers.most_common(20))
    lines.extend(['', 'ARM link register targets (may be within the current function):'])
    lines.extend(f'{count:6d}  {decoded[name]}' for name, count in link_counts.most_common(30))
    lines.extend(['', 'Compiler crate sample counts:'])
    lines.extend(f'{count:6d}  {name}' for name, count in crates.most_common())
    report = '\n'.join(lines) + '\n'
    (profile / 'symbol-summary.txt').write_text(report)
    print(report)


if __name__ == '__main__':
    main()
