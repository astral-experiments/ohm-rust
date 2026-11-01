"""External Linux x86-64 rerun using the evidence branch's original Cargo runner. Python 3.11+."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics

D=Path(__file__).resolve().parent
BASE_SHA='3f2b95afb7a8039bd89864ab93b24f2b5b826769dad5cb6d6d38b15d40d6c7a1'
SEEDS=(415670321,367210859)
DRIVERS={'stock':'a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de','parent':'62e6dcf4febdcf50d4a214ab140a339a82be47461a7c7d56c734510b371bf30c','candidate':'f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642'}
LAUNCHERS={'stock':'c6ab483d6698f68eebb49d74b27361f3e02ede55452dc0416c3fdec9a45b7f60','parent':'594a7f3e784ffb15d62899adb3c2db2b33e176ee55406003fe1b0481734f473d','candidate':'594a7f3e784ffb15d62899adb3c2db2b33e176ee55406003fe1b0481734f473d'}
def load(path,name):
    s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def classify(ident,ack,base):
    obs={k:(v['librustc_driver']['sha256'],v['launcher'])for k,v in ident.items()}
    expected={k:(DRIVERS[k],LAUNCHERS[k])for k in DRIVERS}
    base.require(set(obs)==set(DRIVERS) and obs['stock']==expected['stock'] and len({x[0]for x in obs.values()})==3 and all(x['libLLVM']['sha256']==base.LLVM for x in ident.values()),'Supply signed −18, two distinct compilers and the original LLVM.')
    base.require(obs['parent'][0]!=DRIVERS['candidate'] and obs['candidate'][0]!=DRIVERS['parent'],'Optimized T3/T4 roles are reversed.')
    changed=[k for k in DRIVERS if obs[k]!=expected[k]]
    base.require(not changed or ack,'New compiler bytes require --acknowledge-rebuilt and separately verified source/profile provenance.')
    return {'changed_compilers':changed,'observed_driver_launcher':obs,'scope':'external newly built compilers'if changed else'external original compiler identities'}
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--base-runner',type=Path,default=D.parent/'ohm-uv-resolver-repro.py')
    inputs=('uv','stock','parent','candidate','cargo','cargo-home','scratch')
    for name in (*inputs,'summarize'):p.add_argument('--'+name,type=Path)
    p.add_argument('--describe',action='store_true');p.add_argument('--acknowledge-rebuilt',action='store_true');a=p.parse_args()
    if hashlib.sha256(a.base_runner.read_bytes()).hexdigest()!=BASE_SHA:p.error('The shared published runner SHA differs.')
    base=load(a.base_runner,'shared_cargo_runner');u=load(D/'base007.py','finite_statistics');base.SEEDS=SEEDS
    if a.describe:
        print(json.dumps(dict(A=base.A,B=base.B,patch=base.PATCH,flags=base.ARGS,drivers=DRIVERS,seeds=SEEDS,orders=base.orders(),alpha=.01),indent=2));return
    old=base.summarize
    def summary(root):
        original=old(root)  # Check every Cargo invocation, elapsed integer, stderr roster and order.
        plan=json.loads((root/'plan.json').read_bytes());base.require(plan['cargo']==base.CARGO,'Fixed signed −18 Cargo required.')
        origin=classify(plan['sdks'],a.acknowledge_rebuilt,base)
        rows=[{arm:{'edit':json.loads((root/f'{i:02}-{arm}/edit.json').read_bytes())['elapsed_ns']}for arm in order}for i,order in enumerate(base.orders(),1)]
        blocks=[rows[:6],rows[6:]]
        return {'scope':'independent external rerun with 36 cold parents, separate from the original two held-out histories per SDK','compiler_identity':origin,'arm_median_ns':original['arm_median_ns'],'signed18_over_T4':u.contrast(blocks,('stock','candidate'),.01,2),'optimized_T3_over_T4':u.contrast(blocks,('parent','candidate'),.01,1)}
    base.summarize=summary
    if a.summarize:
        if any(getattr(a,k.replace('-','_'))for k in inputs):p.error('Use --summarize alone with the same identity acknowledgement if needed.')
        print(json.dumps(summary(a.summarize),indent=2));return
    if not all(getattr(a,k.replace('-','_'))for k in inputs):p.error('Supply all seven run inputs, --describe or --summarize.')
    base.require(base.sha(a.cargo)==base.CARGO,'Fixed signed −18 Cargo required.')
    ids={k:base.identity(getattr(a,k).resolve(strict=True),{'PATH':'/usr/bin:/bin','LC_ALL':'C'}) for k in DRIVERS}
    classify(ids,a.acknowledge_rebuilt,base);base.run(a)
if __name__=='__main__':main()
