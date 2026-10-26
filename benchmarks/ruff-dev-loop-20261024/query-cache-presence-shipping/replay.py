"""Reproduce the predeclared decision from the paired measurements in data.json."""
import hashlib
import json
import math
from collections import Counter
from pathlib import Path

import frozen_numeric as numeric

HERE=Path(__file__).resolve().parent

def reduction(rows, index):
    return 100*(1-math.exp(sum(math.log(pair[index+1]/pair[index]) for pair in rows)/len(rows)))

def replay(data):
    assert data['design']['alpha']==6.103515625e-7 and data['design']['exact_assignments']==11778624
    assert hashlib.sha256((HERE/'frozen_numeric.py').read_bytes()).hexdigest()=='80a7bfdc74a94f91753f74f9b7e61f92357f8cd9bc5fab709f24b905d5fa07c0'
    primary=[]
    for block in (1,2):
        rows=[r for r in data['primary'] if r['block']==block]
        assert [r['pair'] for r in rows]==list(range(1,15)) and Counter(r['order'] for r in rows)=={'AB':7,'BA':7}
        for r in rows:
            assert all(type(r[k]) is int and r[k]>0 for k in ('A_Ir','B_Ir'))
            primary.append({'session':block,'order':['base' if a=='A' else 'candidate' for a in r['order']],
                            'base':{'Ir':r['A_Ir']},'candidate':{'Ir':r['B_Ir']},'flags':[]})
    assert len(primary)==len(data['primary'])==28 and len(data['guards'])==36
    stats=numeric.summarize(primary,seed=data['design']['analysis_seed'],label='Ir:all',
                            bootstrap_samples=50000,permutation_samples=2000000,metric='Ir')
    p=stats['randomization_test']
    assert p['assignments']==11778624 and p['method']=='exact_balanced_order_randomization'
    blocks={str(b):reduction([(r['A_Ir'],r['B_Ir']) for r in data['primary'] if r['block']==b],0) for b in (1,2)}
    guards={}
    veto=False
    for mode in ('check','build','test'):
        rows=[r for r in data['guards'] if r['mode']==mode]
        assert len(rows)==12
        def wall(items): return reduction([(r['A_wall_seconds'],r['B_wall_seconds']) for r in items],0)
        each={}
        for block in ('a','b'):
            group=[r for r in rows if r['block']==block]
            assert [r['pair'] for r in group]==list(range(1,7)) and Counter(r['order'] for r in group)=={'AB':3,'BA':3}
            each[block]=wall(group)
        all_pairs=wall(rows)
        veto=veto or all_pairs<=-2 or min(each.values())<=-5
        guards[mode]={'overall':all_pairs,'blocks':each}
    passed=p['p_one_sided']<=data['design']['alpha'] and stats['geometric_reduction_percent_ci99'][0]>0.2 and min(blocks.values())>0 and not veto
    return {'primary_reduction_percent':stats['geometric_reduction_percent'],'primary_ci99':stats['geometric_reduction_percent_ci99'],
            'favorable_pairs':sum(r['B_Ir']<r['A_Ir'] for r in data['primary']),'primary_blocks':blocks,
            'exact_p':p['p_one_sided'],'guard_reductions_percent':guards,'qualified':passed}

if __name__=='__main__':
    data=json.loads((HERE/'data.json').read_bytes())
    expected=json.loads((HERE/'expected.json').read_bytes())
    actual=replay(data)
    def match(a,b):
        if isinstance(a,dict): return isinstance(b,dict) and set(a)==set(b) and all(match(a[k],b[k]) for k in a)
        if isinstance(a,list): return isinstance(b,list) and len(a)==len(b) and all(match(x,y) for x,y in zip(a,b))
        if type(a) is float: return math.isclose(a,b,rel_tol=1e-12,abs_tol=1e-12)
        return a==b
    assert match(actual,expected)
    print(json.dumps(actual,indent=2))
