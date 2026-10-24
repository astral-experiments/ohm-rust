"""Replay the frozen statistical endpoints from sanitized paired observations only."""
from pathlib import Path
import csv, hashlib, importlib.util, json, math
from collections import Counter
HERE = Path(__file__).resolve().parent
MODES = ('check', 'build', 'test')
COUNTS = {'check': 64, 'build': 24, 'test': 24}
ALPHA = 0.000009765625
PRIMARY_MODE = 'check'
NUMERIC_SHA = 'cfb87993aab7d8b08b7059696db9f9859791c596734f657c64e3c624d007d2bc'
def require(value, message):
    if not value:
        raise RuntimeError(message)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def read(name):
    return json.loads((HERE / name).read_bytes())
def numeric_module():
    path = HERE / 'frozen_numeric.py'
    require(sha(path) == NUMERIC_SHA, 'Frozen numeric helper changed')
    spec = importlib.util.spec_from_file_location('frozen_numeric_replay', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def numeric_report(pairs,plan):
 frozen=numeric_module();groups={}
 for mode in MODES:
  selected=[p for p in pairs if p['mode']==mode]
  require(len(selected)==COUNTS[mode] and all(not any(f.endswith('invalid_selected_metric') for f in p['flags']) for p in selected),'No pair may be omitted')
  pooled=frozen.summarize(selected,seed=plan['analysis_seed'],label=mode+':all',bootstrap_samples=50000,permutation_samples=2000000,metric='wall_seconds',sensitivity=False)
  require(pooled['pairs']==pooled['numeric_pairs']==COUNTS[mode] and pooled['bootstrap_stratum_sizes']==[COUNTS[mode]//4]*4,'Numeric helper omitted or restratified rows')
  possibilities=math.comb(COUNTS[mode]//2,COUNTS[mode]//4)**2;exact=possibilities<=2**20
  require(pooled['randomization_test']['method']==('exact_balanced_order_randomization' if exact else 'monte_carlo_balanced_order_randomization') and pooled['randomization_test']['assignments']==(possibilities if exact else 2000000),'Existing randomization branch differs')
  blocks={}
  for block in ['a','b']:
   items=[p for p in selected if p['session']==block];ratios=frozen.np.log([p['candidate']['wall_seconds']/p['base']['wall_seconds'] for p in items])
   blocks[block]={'pairs':len(items),'geometric_reduction_percent':100*(1-float(frozen.np.exp(ratios.mean()))),'paired_log_ratio_sample_sd':float(ratios.std(ddof=1))}
  cpu_ratios=frozen.np.log([p['candidate']['family_cpu_seconds']/p['base']['family_cpu_seconds'] for p in selected])
  groups[mode]={'all':pooled,'blocks':blocks,'family_cpu_diagnostic':{'geometric_reduction_percent':100*(1-float(frozen.np.exp(cpu_ratios.mean()))),'inferential_gate':False}}
 adjusted=frozen.holm({mode:groups[mode]['all']['randomization_test']['p_one_sided'] for mode in MODES})
 for mode in MODES:groups[mode]['all']['holm_adjusted_p']=adjusted[mode]
 return groups

def decision(groups, plan):
    require(set(groups) == set(MODES), 'All three family members are required')
    require(plan['minimum_point_percent'] == 1.0 and plan['minimum_lower99_percent'] == 1.0 and plan['lower99_strict'] is True, 'Prospective decision floors differ')
    gains, vetoes = {}, []
    for mode in MODES:
        pooled = groups[mode]['all']; point = pooled['geometric_reduction_percent']
        lower, upper = pooled['geometric_reduction_percent_ci99']; adjusted = pooled['holm_adjusted_p']
        blocks = {name: groups[mode]['blocks'][name]['geometric_reduction_percent'] for name in ('a', 'b')}
        require(all(math.isfinite(value) for value in [point, lower, upper, adjusted, *blocks.values()])
                and lower <= upper and 0 <= adjusted <= 1, 'Invalid summary')
        gates = {'point_at_least_1_percent': point >= plan['minimum_point_percent'], 'lower99_strictly_above_1_percent': lower > plan['minimum_lower99_percent'],
                 'both_blocks_positive': all(value > 0 for value in blocks.values()), 'holm_within_allocation': adjusted <= ALPHA}
        gains[mode] = {'gates': gates, 'passed': all(gates.values())}
        if point <= -2: vetoes.append({'mode': mode, 'reason': 'overall_point_reduction_at_most_minus_2_percent', 'value': point})
        for block, value in blocks.items():
            if value <= -5: vetoes.append({'mode': mode, 'block': block, 'reason': 'block_point_reduction_at_most_minus_5_percent', 'value': value})
        if upper < 0: vetoes.append({'mode': mode, 'reason': '99_percent_interval_establishes_regression', 'upper99': upper})
    publish = gains[PRIMARY_MODE]['passed'] and not vetoes
    conclusions = {}
    for mode in MODES:
        if groups[mode]['all']['geometric_reduction_percent_ci99'][1] < 0:
            conclusions[mode] = 'evidence_of_regression; publication_veto'
        elif gains[mode]['passed']:
            conclusions[mode] = 'gain_meets_frozen_criteria' if publish else 'gain_gate_met_but_publication_withheld'
        else:
            conclusions[mode] = 'inconclusive; no_gain_or_non_inferiority_claim'
    return {'publication_qualified': publish, 'primary_mode': PRIMARY_MODE, 'primary_passed': gains[PRIMARY_MODE]['passed'],
            'gain_gates': gains, 'regression_vetoes': vetoes, 'mode_conclusions': conclusions,
            'permitted_gain_claims': [mode for mode in MODES if publish and gains[mode]['passed']],
            'no_adaptive_extension': True, 'alpha_ledger_actions': 0}

def main():
    manifest = read('manifest.json')
    require(manifest['schema'] == 'lint-tool-prefix-public-evidence-v1', 'Unknown manifest')
    for name, item in manifest['files'].items():
        path = HERE / name
        require(Path(name).name == name and not path.is_symlink(), 'Unsafe member')
        require(path.stat().st_size == item['bytes'] and sha(path) == item['sha256'], 'File digest differs: ' + name)
    import numpy as np
    require(np.__version__ == '2.3.5', 'NumPy 2.3.5 is required')
    plan, pairs, expected = read('plan.json'), read('pairs.json'), read('expected.json')
    require(plan['pairs_per_mode'] == COUNTS and plan['primary_mode'] == PRIMARY_MODE and plan['alpha'] == ALPHA, 'Frozen allocation changed')
    require(plan['analysis_seed'] == 874677819 and plan['bootstrap_samples'] == 50000 and plan['permutation_samples'] == 2000000 and plan['numeric_sha256'] == NUMERIC_SHA, 'Frozen numeric inputs changed')
    require(plan['original_pilot_screen_passed'] is False and plan['pilot_data_pooled'] is False and plan['extension_allowed'] is False and plan['exclusions'] == [], 'Selection or retention changed')
    require(len(pairs) == 112 and Counter(p['mode'] for p in pairs) == COUNTS, 'Pair membership changed')
    offset = 0
    for component in plan['components']:
        n = component['pairs']; selected = pairs[offset:offset+n]; offset += n
        require(len(selected) == n and Counter(tuple(p['order']) for p in selected) == {('base','candidate'): n//2, ('candidate','base'): n//2}, 'Unbalanced component')
        for number, pair in enumerate(selected):
            require(pair['component'] == component['id'] and pair['block'] == pair['session'] == component['block'] and pair['mode'] == component['mode'] and pair['pair'] == number, 'Pair identity differs')
            require(pair['all_rows_retained'] is True and pair['flags'] == [], 'Retained flags differ')
            variant = 'edited' if number % 2 == 0 else 'original'
            require(pair['source_variant'] == variant and pair['source_sha256'] == plan['source_image_hashes'][variant], 'Source image differs')
            for arm in ('base','candidate'):
                value = pair[arm]
                require(all(math.isfinite(value[k]) and value[k] > 0 for k in ('wall_seconds','family_cpu_seconds','family_wall_seconds')), 'Invalid measurement')
                require(value['family_wall_seconds'] >= value['wall_seconds'] and value['drain_seconds'] >= 0, 'Invalid drainage')
                require(math.isclose(value['family_cpu_seconds'], value['usage']['ru_utime'] + value['usage']['ru_stime'], rel_tol=1e-14, abs_tol=1e-14), 'Family CPU differs')
    with (HERE / 'pairs.csv').open(newline='') as stream:
        csv_rows = list(csv.DictReader(stream))
    require(len(csv_rows) == len(pairs), 'CSV cardinality differs')
    for row, pair in zip(csv_rows, pairs):
        require(all(row[key] == str(pair[key]) for key in ('mode','block','component','pair','source_variant','source_sha256')), 'CSV identity differs')
        require(row['order'] == ''.join('A' if arm == 'base' else 'B' for arm in pair['order']) and json.loads(row['flags']) == pair['flags'], 'CSV order/flags differ')
        for arm in ('base','candidate'):
            for key in ('wall_seconds','family_cpu_seconds','family_wall_seconds','drain_seconds'):
                require(float(row[arm+'_'+key]) == pair[arm][key], 'CSV measurement differs')
            for key in ('ru_utime','ru_stime'):
                require(float(row[arm+'_'+key]) == pair[arm]['usage'][key], 'CSV usage differs')
    groups = numeric_report(pairs, plan)
    actual = {'groups': groups, 'decision': decision(groups, plan)}
    require(actual == expected, 'Saved statistical endpoints or decision did not reproduce exactly')
    canonical = json.dumps(actual, sort_keys=True, separators=(',', ':')).encode()
    print(json.dumps({'passed': True, 'numpy_version': np.__version__, 'manifest_sha256': sha(HERE/'manifest.json'), 'files_verified': len(manifest['files']), 'pairs': len(pairs), 'csv_rows': len(csv_rows), 'all_endpoint_values_exactly_match': True, 'canonical_endpoints_sha256': hashlib.sha256(canonical).hexdigest(), 'expected_file_sha256': sha(HERE/'expected.json'), 'permitted_gain_claims': actual['decision']['permitted_gain_claims'], 'native_commands': 0, 'new_measurements': 0, 'alpha_ledger_actions': 0}, indent=2, sort_keys=True))

if __name__ == '__main__':
    main()
