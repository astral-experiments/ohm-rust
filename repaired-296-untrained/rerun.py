"""Run an independent Linux x86-64, Python 3.11+ comparison with the identified SDKs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

D=Path(__file__).resolve().parent
def load(path,name):
    s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def ids(actual,wants,allow,base):
    got={k:[r['librustc_driver']['sha256'],r['launcher']]for k,r in actual.items()};want={k:[r['driver_sha256'],r['launcher_sha256']]for k,r in wants.items()}
    base.require(got['stock']==want['stock'] and all(r['libLLVM']['sha256']==base.LLVM for r in actual.values()) and len({x[0]for x in got.values()})==3,'Expected signed −18, original LLVM and three distinct drivers.')
    base.require(all(got[x][0]!=want[y][0]for x,y in [('candidate','parent'),('parent','candidate')]),'Matched compiler roles reversed.')
    new=[k for k in got if got[k]!=want[k]];base.require(not new or allow,'New SDKs require explicit --acknowledge-rebuilt, after checking their build provenance.')
    return dict(rebuilt_arms=new,observed_driver_launcher=got)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--base-runner',type=Path,default=D.parent/'ohm-uv-resolver-repro.py');p.add_argument('--describe',action='store_true')
    p.add_argument('--packet',type=Path,default=D/'records.zip');p.add_argument('--acknowledge-rebuilt',action='store_true')
    inputs=('uv','stock','parent','candidate','cargo','cargo-home','scratch')
    for k in (*inputs,'summarize'):p.add_argument('--'+k,type=Path)
    a=p.parse_args()
    if hashlib.sha256(a.base_runner.read_bytes()).hexdigest()!='3f2b95afb7a8039bd89864ab93b24f2b5b826769dad5cb6d6d38b15d40d6c7a1':p.error('Unexpected shared Cargo runner.')
    b=load(a.base_runner,'verified_shared_cargo');v=load(D/'verify.py','evidence_scores');doc=v.load(a.packet);v.compute(doc);q=doc['qualification'];seeds=v.PLANS[q['study']][1]
    b.SEEDS=tuple(seeds[k]for k in ('confirm-1','confirm-2'));wants={'stock':q['stock'],**q['subjects']};fixed=q['study']==v.FIX;alpha=.05/7 if fixed else .05
    if a.describe:
        print(json.dumps(dict(parent=b.A,child=b.B,command=b.ARGS,drivers={k:x['driver_sha256']for k,x in wants.items()},seeds=b.SEEDS,orders=b.orders(),alpha=alpha),indent=2));return
    old=b.summarize
    def summary(path):
        previous=old(path);plan=json.loads((path/'plan.json').read_bytes());b.require(plan['cargo']==b.CARGO,'Need the fixed signed Cargo.')
        obs=ids(plan['sdks'],wants,a.acknowledge_rebuilt,b)
        rows=[{arm:{'edit':json.loads((path/f'{i:02}-{arm}/edit.json').read_bytes())['elapsed_ns']}for arm in order}for i,order in enumerate(b.orders(),1)]
        return {'scope':'independent external rerun with 36 cold parents and no imported trial times','identity':obs,'arm_median_ns':previous['arm_median_ns'],'signed_over_candidate':v.u.contrast([rows[:6],rows[6:]],('stock','candidate'),alpha,2),'parent_over_candidate':v.u.contrast([rows[:6],rows[6:]],('parent','candidate'),alpha,1)}
    b.summarize=summary
    if a.summarize:
        if any(getattr(a,n.replace('-','_'))for n in inputs):p.error('Use --summarize on its own.')
        print(json.dumps(summary(a.summarize),indent=2));return
    if not all(getattr(a,n.replace('-','_'))for n in inputs):p.error('Supply all seven native run paths or --describe.')
    b.require(b.sha(a.cargo)==b.CARGO,'Supply signed −18 Cargo.')
    observed={k:b.identity(getattr(a,k).resolve(strict=True),{'PATH':'/usr/bin:/bin','LC_ALL':'C'})for k in wants}
    ids(observed,wants,a.acknowledge_rebuilt,b);b.run(a)
if __name__=='__main__':main()
