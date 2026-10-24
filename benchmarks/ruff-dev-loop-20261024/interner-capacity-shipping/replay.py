"""Replay frozen statistical endpoints from all sanitized paired observations."""
from pathlib import Path
import csv, hashlib, importlib.util, json, math
from collections import Counter
HERE = Path(__file__).resolve().parent
ALPHA = 1.220703125e-6
NUMERIC_SHA = 'cfb87993aab7d8b08b7059696db9f9859791c596734f657c64e3c624d007d2bc'
def require(value,message):
    if not value:raise RuntimeError(message)

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def read(path):return json.loads(Path(path).read_text())

def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

def numeric_module():
    require(sha(HERE/'frozen_numeric.py')==NUMERIC_SHA,'Frozen numeric helper changed')
    return load(HERE/'frozen_numeric.py','fixed_frozen_numeric')

def reductions(pairs,metric):
    require(pairs and all(type(row[arm][metric]) in (int,float) and math.isfinite(row[arm][metric])
                         and row[arm][metric]>0 for row in pairs for arm in ['base','candidate']),
            'Every fixed observation must have a positive finite endpoint')
    np=numeric_module().np
    ratios=np.log([row['candidate'][metric]/row['base'][metric] for row in pairs])
    return 100*(1-float(np.exp(ratios.mean())))

def numeric_report(ir_pairs,guard_pairs,plan):
    load(HERE/'schedule.py','fixed_alpha_floor').validate_attainable_alpha(plan['alpha'])
    require(len(ir_pairs)==24 and len(guard_pairs)==36,'All primary and guard pairs are mandatory')
    require(plan['gain_hypotheses']==1 and plan['alpha']==plan['planned_alpha'] and plan['separate_point_floor_applied'] is False
            and plan['lower_99_percent_strictly_above']==0.2,'Frozen sole-primary policy differs')
    require(not any(row['flags'] for row in ir_pairs),'Primary rows cannot be filtered')
    reductions(ir_pairs,'Ir');frozen=numeric_module()
    primary=frozen.summarize(ir_pairs,seed=plan['analysis_seed'],label='Ir:all',bootstrap_samples=50000,
                             permutation_samples=2000000,metric='Ir',sensitivity=False)
    require(primary['pairs']==primary['numeric_pairs']==24 and primary['bootstrap_stratum_sizes']==[6]*4,
            'Primary rows were omitted or restratified')
    randomization=primary['randomization_test']
    require(randomization['method']=='exact_balanced_order_randomization'
            and randomization['assignments']==randomization['possible_assignments']==853776,
            'Frozen exact randomization branch differs')
    primary['holm_adjusted_p']=frozen.holm({'Ir':randomization['p_one_sided']})['Ir']
    primary['blocks']={}
    for block in [1,2]:
        rows=[row for row in ir_pairs if row['session']==block];require(len(rows)==12,'Wrong primary block size')
        primary['blocks'][str(block)]=dict(pairs=12,geometric_reduction_percent=reductions(rows,'Ir'))
    guards={}
    for mode in ['check','build','test']:
        rows=[row for row in guard_pairs if row['mode']==mode];require(len(rows)==12,'Missing native guard mode')
        blocks={}
        for block in ['a','b']:
            selected=[row for row in rows if row['session']==block];require(len(selected)==6,'Missing native guard block')
            blocks[block]=dict(pairs=6,geometric_reduction_percent=reductions(selected,'command_wall_seconds'))
        guards[mode]=dict(pairs=12,geometric_reduction_percent=reductions(rows,'command_wall_seconds'),blocks=blocks,
                          endpoint='command_wall_seconds',descriptive_only=True,positive_native_gain_claim=False,
                          noninferiority_claim=False)
    return dict(primary=primary,native_guards=guards)

def decision(report,plan):
    load(HERE/'schedule.py','fixed_alpha_floor').validate_attainable_alpha(plan['alpha'])
    primary=report['primary'];guards=report['native_guards'];require(set(guards)=={'check','build','test'},'All guards required')
    lower,upper=primary['geometric_reduction_percent_ci99'];p=primary['holm_adjusted_p']
    values=[lower,upper,p,primary['geometric_reduction_percent'],*[row['geometric_reduction_percent'] for row in primary['blocks'].values()]]
    require(all(math.isfinite(value) for value in values) and lower<=upper and 0<=p<=1,'Invalid primary summary')
    require(set(primary['blocks'])=={'1','2'} and plan['alpha']==plan['planned_alpha']
            and plan['lower_99_percent_strictly_above']==0.2 and plan['separate_point_floor_applied'] is False,'Wrong primary gates')
    gates=dict(exact_sole_primary_p_within_allocation=p<=plan['alpha'],lower99_strictly_above_0_2_percent=lower>0.2,
               both_blocks_positive=all(row['geometric_reduction_percent']>0 for row in primary['blocks'].values()))
    policy=plan['native_regression_guard'];require(policy['all_36_pairs_required_before_final_decision'] is True
        and policy['overall_reduction_percent_lte_veto']==-2.0 and policy['either_block_reduction_percent_lte_veto']==-5.0,
        'Frozen native veto policy differs')
    vetoes=[]
    for mode,row in guards.items():
        require(row['pairs']==12 and set(row['blocks'])=={'a','b'} and all(x['pairs']==6 for x in row['blocks'].values()),'Incomplete guard')
        value=row['geometric_reduction_percent'];require(math.isfinite(value),'Invalid guard summary')
        if value<=-2.0:vetoes.append(dict(mode=mode,scope='overall',reduction_percent=value))
        for block,item in row['blocks'].items():
            value=item['geometric_reduction_percent'];require(math.isfinite(value),'Invalid guard block')
            if value<=-5.0:vetoes.append(dict(mode=mode,scope=block,reduction_percent=value))
    return dict(primary_gates=gates,primary_passed=all(gates.values()),native_regression_vetoes=vetoes,
                qualification_passed=all(gates.values()) and not vetoes,separate_point_floor_applied=False,
                permitted_gain_endpoint='selected original compiler PID Ir' if all(gates.values()) and not vetoes else None,
                native_wall_gain_claim=False,noninferiority_claim=False,alpha_ledger_actions=0,
                adaptive_extension=False,publication_action_taken=False)

def csv_rows(ir_pairs, guard_pairs):
    rows = []
    diagnostics = ('command_wall_seconds', 'family_wall_seconds', 'family_cpu_seconds',
                   'user_cpu_seconds', 'system_cpu_seconds', 'drain_seconds')
    for kind, pairs in [('Ir', ir_pairs), ('native_guard', guard_pairs)]:
        for pair in pairs:
            row = dict(kind=kind, mode=pair['mode'], block=pair['session'], component=pair['component'],
                       pair=pair['pair'], order=''.join('A' if arm == 'base' else 'B' for arm in pair['order']),
                       source_variant=pair['source_variant'], source_sha256=pair['source_sha256'],
                       flags=json.dumps(pair['flags'], separators=(',', ':')))
            for arm in ('base', 'candidate'):
                row[arm+'_Ir'] = pair[arm].get('Ir', '')
                for name in diagnostics:
                    row[arm+'_'+name] = pair[arm].get(name, '')
            rows.append(row)
    return rows


def validate_pairs(plan, ir_pairs, guard_pairs):
    require(plan['source_plan_sha256'] == 'ef874412df58c3d9a1e06e6c50d2b97d942614eb2972688bc373db287df24a6b'
            and plan['numeric_sha256'] == NUMERIC_SHA and plan['analysis_seed'] == 282139565327582089072857975991756123694
            and plan['alpha'] == ALPHA and plan['alpha_ordinal'] == 13, 'Frozen numeric/alpha inputs differ')
    require(plan['bootstrap_samples'] == 50000 and plan['exact_randomization_assignments'] == 853776
            and plan['permutation_samples'] == 2000000 and plan['exclusions'] == []
            and not plan['adaptive_extension'] and plan['prior_exploratory_observations_in_inference'] == 0,
            'Frozen method or retention differs')
    require(len(ir_pairs) == 24 and len(guard_pairs) == 36, 'All 60 pairs are required')
    require([(p['session'], p['pair']) for p in ir_pairs] == [(b, n) for b in (1, 2) for n in range(1, 13)],
            'Primary pair membership/order differs')
    for pair in ir_pairs:
        ordinal = pair['pair']; block = pair['session']
        require(pair['component'] == 'ir-block-'+str(block) and pair['mode'] == 'check'
                and pair['order'] == ['base' if x == 'A' else 'candidate' for x in plan['pair_orders'][str(block)][ordinal-1]]
                and pair['cycle'] == (ordinal-1)//3+1 and pair['cycle_position'] == (ordinal-1)%3+1
                and pair['observed_compaction_claim'] is False, 'Primary schedule/interpretation differs')
        require(pair['source_variant'] == ('after' if ordinal % 2 else 'before')
                and pair['source_sha256'] == plan['source_image_hashes']['parser'][pair['source_variant']], 'Parser source differs')
        for arm in ('base', 'candidate'):
            require(type(pair[arm]['Ir']) is int and pair[arm]['Ir'] > 0, 'Invalid instruction count')
    expected = [(c['id'], n) for c in plan['native_guard_components'] for n in range(6)]
    require([(p['component'], p['pair']) for p in guard_pairs] == expected, 'Guard pair membership/order differs')
    for component in plan['native_guard_components']:
        selected = [p for p in guard_pairs if p['component'] == component['id']]
        require(Counter(tuple(p['order']) for p in selected) == {('base', 'candidate'): 3, ('candidate', 'base'): 3},
                'Guard block order balance differs')
        for pair in selected:
            require(pair['mode'] == component['mode'] and pair['session'] == component['block']
                    and pair['order'] == ['base' if x == 'A' else 'candidate' for x in plan['native_pair_orders'][component['id']][pair['pair']]],
                    'Guard identity/order differs')
            require(pair['source_variant'] == ('after' if pair['pair'] % 2 == 0 else 'before')
                    and pair['source_sha256'] == plan['source_image_hashes']['linter'][pair['source_variant']], 'Linter source differs')
            for arm in ('base', 'candidate'):
                value = pair[arm]
                require(all(type(value[k]) in (int, float) and math.isfinite(value[k]) and value[k] > 0
                            for k in ('command_wall_seconds', 'family_wall_seconds', 'family_cpu_seconds')),
                        'Invalid native endpoint')
                require(value['family_wall_seconds'] >= value['command_wall_seconds'] and value['drain_seconds'] >= 0
                        and math.isclose(value['family_cpu_seconds'], value['user_cpu_seconds']+value['system_cpu_seconds'],
                                         rel_tol=1e-14, abs_tol=1e-14), 'Native accounting differs')
    require(all(p['flags'] == [] and p['all_rows_retained'] is True for p in ir_pairs+guard_pairs), 'Rows/flags were omitted')
    return True


def main():
    manifest = read(HERE/'manifest.json')
    require(manifest['schema'] == 'interner-capacity-public-evidence-v1', 'Unknown manifest')
    for name, row in manifest['files'].items():
        path = HERE/name
        require(Path(name).name == name and not path.is_symlink()
                and path.stat().st_size == row['bytes'] and sha(path) == row['sha256'], 'Bundle digest differs: '+name)
    import numpy as np
    require(np.__version__ == '2.3.5', 'NumPy 2.3.5 is required')
    plan = read(HERE/'plan.json'); ir_pairs = read(HERE/'primary-pairs.json'); guards = read(HERE/'native-guard-pairs.json')
    validate_pairs(plan, ir_pairs, guards)
    with (HERE/'pairs.csv').open(newline='') as stream:
        actual_csv = list(csv.DictReader(stream))
    require(actual_csv == [{k: str(v) for k, v in row.items()} for row in csv_rows(ir_pairs, guards)], 'CSV and JSON pairs differ')
    profiles = read(HERE/'profile-references.json')
    require(len(profiles) == 48 and {(p['block'], p['pair'], p['arm']) for p in profiles} ==
            {(b, n, a) for b in (1, 2) for n in range(1, 13) for a in ('A', 'B')}, 'Profile reference membership differs')
    for row in profiles:
        pair = ir_pairs[(row['block']-1)*12+row['pair']-1]
        require(row['Ir'] == pair['base' if row['arm'] == 'A' else 'candidate']['Ir']
                and row['other_pids_summed'] is False and row['profiles'], 'Selected compiler profile join differs')
        for profile in row['profiles']:
            require(set(profile) == {'sha256', 'bytes'} and len(profile['sha256']) == 64
                    and type(profile['bytes']) is int and profile['bytes'] > 0, 'Invalid raw profile reference')
    report = numeric_report(ir_pairs, guards, plan)
    actual = dict(report=report, decision=decision(report, plan))
    expected = read(HERE/'expected.json')
    require(actual == expected, 'Frozen statistical endpoints or decision did not reproduce exactly')
    canonical = json.dumps(actual, sort_keys=True, separators=(',', ':')).encode()
    print(json.dumps(dict(passed=True, numpy_version=np.__version__, manifest_sha256=sha(HERE/'manifest.json'),
        files_verified=len(manifest['files']), primary_pairs=24, native_guard_pairs=36, csv_rows=60,
        selected_compiler_observations=48, all_endpoint_values_exactly_match=True,
        canonical_endpoints_sha256=hashlib.sha256(canonical).hexdigest(), expected_file_sha256=sha(HERE/'expected.json'),
        qualification_passed=actual['decision']['qualification_passed'], native_wall_gain_claim=False,
        native_commands=0, new_measurements=0, alpha_ledger_actions=0), indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
