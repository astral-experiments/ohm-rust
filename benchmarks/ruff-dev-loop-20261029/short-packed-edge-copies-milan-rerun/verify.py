"""Independently recalculate the frozen paired decision and, if supplied, reopen every raw Callgrind profile."""
import argparse
import gzip
import hashlib
import itertools
import json
import math
from pathlib import Path
import random
import re
import statistics

SCHEDULE=['AB','BA','BA','AB','AB','AB','BA','BA','AB','BA','AB','BA']
PUB='1a0048705c5d712ef683400597c98d98b623e974'
MAN='ec98e063dbd31580966530439da3ffc271054e6ac5ae6b3407d7e756a35a74c2'
def recalc(slots):
    changes=[math.log(slots[f'{i}-B']['ir']/slots[f'{i}-A']['ir']) for i in range(1,13)]
    diffs=[slots[f'{i}-B']['ir']-slots[f'{i}-A']['ir'] for i in range(1,13)]
    obs=statistics.median(changes)
    chrono=[v if order=='AB' else -v for v,order in zip(changes,SCHEDULE)]
    perms=[statistics.median(v if i in fwd else -v for i,v in enumerate(chrono)) for fwd in itertools.combinations(range(12),6)]
    tails=[sum(p<=-abs(obs)+1e-15 for p in perms),sum(p>=abs(obs)-1e-15 for p in perms)]
    prob=sum(tails)/len(perms)
    rng=random.Random('ruff_text_size-decoder-holdout-20261029')
    bootstrap=sorted(statistics.median(rng.choices(changes,k=12)) for _ in range(30000))
    return dict(pairs=12,lower_pairs=sum(i<0 for i in diffs),median_ir=statistics.median(diffs),median_percent=100*math.expm1(obs),order_medians={o:100*math.expm1(statistics.median(v for i,v in enumerate(changes) if SCHEDULE[i]==o)) for o in ['AB','BA']},permutations=len(perms),tail_counts=tails,two_sided_p=prob,paired_ci95=[100*math.expm1(bootstrap[i]) for i in (749,29249)],decoder_inclusive_percent=statistics.median((slots[f'{i}-B']['decoder_inclusive']/slots[f'{i}-A']['decoder_inclusive']-1)*100 for i in range(1,13)),per_arm_total_range={a:[min(slots[f'{i}-{a}']['ir'] for i in range(1,13)),max(slots[f'{i}-{a}']['ir'] for i in range(1,13))] for a in 'AB'},passes=obs<0 and prob<.05 and sum(i<0 for i in diffs)>=10)
def sha(b):return hashlib.sha256(b).hexdigest()
def verify(report,raw):
    assert report['launcher_publication']==PUB and report['launcher_manifest']==MAN and report['schedule']==SCHEDULE
    assert report['private_clone'] and report['first_attempt_completed'] and report['independent'] and len(report['slots'])==24 and report['launch_exit']==0
    expected=report['native']['whole_rmeta']
    for arm in 'AB':
        for name,checks in report['native']['arms'][arm]['commands'].items():
            assert len(checks['typed'])==1 and checks['typed'][0][0]=='ruff_text_size' and checks['cargo_exit']==0
            assert checks['typed'][0][2]==(name=='repeat')
            if name in ('native','repeat','clean'):assert checks['whole_rmeta']==expected
        suite=report['native']['arms'][arm]['external']
        assert suite['previous-external']['codes']==['E0599'] and suite['previous-external']['compiler_exit']==1
        for n in ['previous-legacy','current-legacy','current-external']:
            assert suite[n]['compiler_exit']==suite[n]['exit']==0 and suite[n]['stdout']=='1..3:é\n'
            assert suite[n]['binary']==report['native']['arms']['A']['external'][n]['binary']
        assert report['native']['arms'][arm]['direct']['sha256']==expected['sha256']
    assert sorted(report['native']['arms']['A']['commands'])==sorted(report['native']['arms']['B']['commands'])==['build-current','build-previous','clean','native','previous','repeat']
    assert report['native']['arms']['A']['driver']!=report['native']['arms']['B']['driver']
    for label,row in report['slots'].items():
        assert row['whole_metadata_sha256']==expected['sha256'] and row['ir']>10_000_000 and row['decoder_inclusive']>row['decoder_self']
        if raw:
            f=raw/f'slot-{label}'/'rustc.callgrind.gz'; b=gzip.decompress(f.read_bytes()); m=json.loads((f.parent/'measurement.json').read_bytes())
            assert len(b)==row['bytes']==m['bytes'] and sha(b)==row['raw_sha256']==m['raw_sha256']
            assert row['ir']==m['ir']==int(re.search(rb'^totals: (\d+)\s*$',b,re.M).group(1))
            assert b'\nevents: Ir\n' in b and set(m['outputs'].values())=={expected['sha256']}
    assert recalc(report['slots'])==report['statistics']
    return report['statistics']
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--report',type=Path,default=Path(__file__).with_name('result.json'));p.add_argument('--raw',type=Path);a=p.parse_args();print(json.dumps(verify(json.loads(a.report.read_bytes()),a.raw),sort_keys=True))
