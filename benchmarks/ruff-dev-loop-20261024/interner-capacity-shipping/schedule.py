"""Fixed two-block design; consumes root-bound orders and never draws a seed."""
import math
import random


def require(value, message):
    if not value:
        raise RuntimeError(message)


def orders_from_seed(seed):
    require(type(seed) is int and seed >= 0, 'Root schedule seed missing')
    orders = ['AB'] * 6 + ['BA'] * 6
    random.Random(seed).shuffle(orders)
    return orders


def calls_for_block(block, orders):
    require(block in (1, 2), 'Unknown block')
    require(isinstance(orders, list) and len(orders) == 12
            and orders.count('AB') == orders.count('BA') == 6, 'HELD or unbalanced pair orders')
    calls = []
    for arm in ('AB' if block == 1 else 'BA'):
        calls.append(dict(id='warm-' + arm, arm=arm, phase='warm', ordinal=0,
                          pair=None, order=None, source_variant='before', cycle=None, cycle_position=None))
    for ordinal, order in enumerate(orders, 1):
        for arm in order:
            calls.append(dict(id=f'pair-{ordinal:02d}-{arm}', arm=arm, phase='edited', ordinal=ordinal,
                              pair=ordinal, order=order, source_variant='after' if ordinal % 2 else 'before',
                              cycle=(ordinal - 1) // 3 + 1, cycle_position=(ordinal - 1) % 3 + 1))
    require(len(calls) == 26 and len({row['id'] for row in calls}) == 26, 'Wrong schedule cardinality')
    return calls


def validate_plan(plan, activated=False):
    require(plan['schema'] == 'parser-compiler-instruction-confirmation-proposal-v1', 'Wrong design')
    require(plan['blocks'] == 2 and plan['pairs_per_block'] == 12 and plan['pairs_total'] == 24
            and plan['warm_calls_per_block'] == 2 and plan['profiled_calls_per_block'] == 24
            and plan['cargo_calls_total'] == 52 and plan['profiled_calls_total'] == 48, 'Changed sample counts')
    require(isinstance(plan['comparison']['A'], str) and isinstance(plan['comparison']['B'], str)
            and plan['comparison']['A'] != plan['comparison']['B']
            and plan['comparison']['fixture'] == 'parser', 'Changed runtime/edit selection')
    require(plan['primary_endpoint'] == 'selected original compiler PID Ir'
            and plan['separate_point_floor_applied'] is False
            and 0 < plan['planned_alpha'] <= 0.01
            and plan['lower_99_percent_strictly_above'] == 0.2
            and plan['both_block_geometric_reductions_positive'] is True, 'Changed endpoint or effect gates')
    require(plan['exact_randomization_assignments'] == math.comb(12, 6) ** 2 == 853776
            and plan['gain_hypotheses'] == 1 and plan['bootstrap_samples'] == 50000
            and plan['permutation_samples'] == 2000000, 'Changed inference design')
    require(plan['adaptive_extension'] is False and plan['automatic_retry'] is False
            and plan['prior_exploratory_observations_in_inference'] == 0 and plan['exclusions'] == []
            and plan['alpha_ledger_actions'] == 0, 'Adaptive or reused observations')
    require(plan['collection_limits_per_block'] == dict(files=2048, per_file_bytes=134217728,
                                                      decoded_bytes=536870912, response_bytes=805306368),
            'Changed collection bounds')
    require(plan['warm_orders']=={'1':'AB','2':'BA'} and plan['no_interim_effects'] is True,
            'Changed warm order or interim policy')
    guard=plan['native_regression_guard']
    require(guard['fixture']=='linter' and guard['selected_crate']=='ruff_linter'
            and guard['needle']=='suppression' and guard['replacement']=='suppr3ssion'
            and guard['pairs_per_mode']==12 and guard['pairs_per_block_mode']==6
            and guard['warm_calls']==12 and guard['timed_calls']==72
            and guard['endpoint']=='command_wall_seconds'
            and guard['overall_reduction_percent_lte_veto']==-2.0
            and guard['either_block_reduction_percent_lte_veto']==-5.0
            and guard['all_36_pairs_required_before_final_decision'] is True
            and guard['positive_native_gain_claim'] is False and guard['noninferiority_claim'] is False,
            'Changed mandatory native guard policy')
    expected=[('a-check','a','check'),('a-build','a','build'),('a-test','a','test'),
              ('b-test','b','test'),('b-build','b','build'),('b-check','b','check')]
    require([(x['id'],x['block'],x['mode']) for x in plan['native_guard_components']]==expected
            and all(x['pairs']==6 for x in plan['native_guard_components']), 'Changed native component schedule')
    validate_attainable_alpha(plan['planned_alpha'])
    gate=plan.get('screen_gate')
    if gate is not None:
        require(gate['must_pass_before_confirmation_authorization'] is True
                and gate['screening_observations_in_confirmation_statistics']==0, 'Changed screen selection scope')
    if activated:
        require(gate is not None and gate['screen_passed'] is True and all(isinstance(gate[key],dict)
                and isinstance(gate[key].get('sha256'),str) and len(gate[key]['sha256'])==64
                for key in ['screen_result','independent_screen_result','independent_audit']),'HELD: actual passed screen required')
        require(plan['alpha_allocation_required_before_first_call'], 'Changed allocation boundary')
        require(type(plan['analysis_seed']) is int and plan['analysis_seed'] >= 0
                and type(plan['alpha']) is float and plan['alpha'] == plan['planned_alpha']
                and isinstance(plan['alpha_ledger_predecessor'], dict), 'Root analysis/alpha binding missing')
        require(all(type(x['schedule_seed']) is int and x['schedule_seed']>=0 for x in plan['native_guard_components'])
                and len({x['schedule_seed'] for x in plan['native_guard_components']})==6, 'Native guard seeds remain held')
        validate_attainable_alpha(plan['alpha'])
        seeds = list(plan['schedule_seeds'].values())
        require(len(seeds) == 2 and all(type(seed) is int and seed >= 0 for seed in seeds)
                and len(set(seeds)) == 2, 'Root schedule seeds missing or repeated')
        for block in (1, 2):
            require(plan['pair_orders'][str(block)] == orders_from_seed(plan['schedule_seeds'][str(block)]),
                    'Orders do not replay the root-provided uniform shuffle')
            calls_for_block(block, plan['pair_orders'][str(block)])
    return plan


def validate_attainable_alpha(alpha):
    """Reject an impossible fixed-design allocation without draws or observations."""
    require(type(alpha) in (int, float) and math.isfinite(alpha) and 0 < alpha <= 0.01,
            'Alpha must be a finite positive allocation within the fixed family budget')
    assignments = math.comb(12, 6) ** 2
    require(assignments == 853776 and assignments <= 2 ** 20, 'Exact design changed')
    # This is the same representable float used by the unchanged exact-test helper.
    require(alpha >= 1 / assignments,
            'Unsupported alpha: below attainable minimum 1/853776 for the frozen exact test')
    return dict(assignments=assignments, minimum_numerator=1, minimum_denominator=assignments,
                minimum_p=1 / assignments, alpha=alpha)
