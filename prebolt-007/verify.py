"""Independently check the 007 resolver records using Python 3.10+ with no installed packages."""
import argparse
import csv
import io
from collections import Counter
from hashlib import sha256
import itertools
import json
import math
from pathlib import Path
import random
import re
from statistics import median
from zipfile import ZipFile

PH=('discovery','confirm-1','confirm-2');ARMS=('stock','parent','candidate')
ARGV=['cargo','check','-p','uv-resolver','--locked','--offline','-j8']
DIRTY={'uv_workspace','uv_types','uv_distribution','uv_resolver'}
PATCH='24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb'
CARGO='02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'
SDK={
 'stock':('a4fa11bcd38280639cb94daacc125472657db6c3','a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de'),
 'base':('556a8364f1485a03be31b1a50c250d63198a943d','c502c3d3454f20b07806de770170d27b6d2043b6d8d8abbab829d5c99cc88ad0'),
 'T1':('8833a44ae11c06f328141f5b4254729c2d7b27cc','cdb9a3ad169b7af296562f96f9cfa4f10fb8d40d8830d2511bfced8b8f87f560'),
 'T3':('a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5','62e6dcf4febdcf50d4a214ab140a339a82be47461a7c7d56c734510b371bf30c')}
WORK={'externs':9,'ctfe-stress-5':9,'cargo-0.87.1':12,'token-stream-stress':9,'match-stress':9,'tuple-stress':12,'diesel-2.2.10':12,'bitmaps-3.2.1':12,'serde-1.0.219-new-solver':3}
SUITES={'tests/incremental/overlapping-impls-in-new-solver-issue-135514.rs':[0,1],
        'tests/incremental/ich_method_call_trait_scope.rs':[0,0], 'tests/incremental/change_implementation_cross_crate':[0,0],
        'tests/ui/traits/ambiguous-trait-matching-distinct-lifetimes.rs':[1],'tests/run-make/trait-impl-bucket-projection':[0]*14}
def need(condition,msg):
    if not condition:raise ValueError(msg)
def digest(b):return sha256(b).hexdigest()
def ishash(s):return bool(re.fullmatch('[0-9a-f]{64}',s))
PROFILE={
 'base':('f2f1991cef35ff8706a8f4e619082b36dc5867f00d08da7f619050ee49055e43',43479456),
 'T1':('bad0f4c8d291ddd5f27d3ecdc022837523de4ff7513a7dc4df4f4f2d25f03614',43534456),
 'T3':('2fc5ae8ff82814328637a40961bfbba33d9f69664e15a380a977f7a449e59c37',43542120)}
def qualify(q):
    need(q['admission_sha256']=='035b7f81b07a6019afa975a03836c5f47e99f4613618cc81f3bab11e1e8e1f93' and q['registration']=='b5345ed4baccaf610c4d3bc2e69d422bcbecd360d04c44926c99dfcaf08b1fec','admitted 007 subjects')
    need(q['stock']['release']['sources']['rust']['revision']==SDK['stock'][0] and q['stock']['archive_sha256']=='629abab6a87c84bb4d910549ca96a31e437e3dc9de3039fedcbb7b50d198a5e8','signed release archive and revision')
    need(q['timed']==dict(stock='stock',parent='base',candidate='T3') and q['cargo']==CARGO and q['stock']['driver_sha256']==SDK['stock'][1],'fixed subjects')
    need(set(q['optimized'])=={'base','T1','T3'} and q['older_independent_performance_rows']==0,'three build origins')
    for role,form in zip(('base','T1','T3'),(8,9,10)):
        v=q['optimized'][role]
        need(v['commit']==SDK[role][0] and (SDK[role][1] is None or v['driver_sha256']==SDK[role][1]) and v['header_format']==form,'named source/driver/format')
        need((v['profile_sha256'],v['profile_bytes'])==PROFILE[role] and set(v['compiler_flag_events'])=={'generate','use'} and all(x['units']>=100 and ishash(x['events'])for x in v['compiler_flag_events'].values()),'captured profile and compiler flag events')
        need(v['driver_BOLT'] is False and v['profile_bytes']>0 and v['profile_sha256']==v['use_profile_sha256'] and v['llvm_sha256']==q['shared_llvm'],'pre-BOLT optimized compiler')
        need({k:len(rows)for k,rows in v['training'].items()}==WORK and v['training_exit']==0,'nine actual non-uv collector targets')
        for k,rows in v['training'].items():
            need(len(rows)==len(set(rows)) and all(x.startswith('eprintln-Test-'+k+'-') and x.endswith('-Threads1')for x in rows),'typed training markers')
        need(v['solver_modes']==['default','next'] and [r[1] for r in v['dispatch']]==['3 3','3 3','7 3','3 7','3 3','3 7','3 3'],'actual seven-revision native dispatch')
        need(set(v['native_phases'])=={'generate','train','use','qualified'} and all(x['exit']==0 for x in v['native_phases'].values()),'native phase exits')
        need(len(v['suites'])==5 and {s['suite']for s in v['suites']}==set(SUITES),'five actual suites')
        for s in v['suites']:
            need(s['command_exit']==0 and sorted(x['status']for x in s['mapped'])==SUITES[s['suite']],'expected actual compiler child results')
            for x in s['mapped']:
                need(all(x[k]==v[k]for k in ('driver_sha256','llvm_sha256','launcher_sha256')) and ishash(x['source_sha256']),'mapped child compiler')
    need(len({v['profile_sha256'] for v in q['optimized'].values()})==3,'each compiler trained separately')

def clock(c,expected):
    d=c['cargo'];names=[n.replace('-','_') for n in re.findall(r'^\s*(?:Checking|Compiling) ([\w-]+) v',c['stderr'],re.M)]
    need(d['cmd']==ARGV and d['dirty_units']==names and c['stdout']=='' and 'Finished 'in c['stderr'],'ordinary Cargo log and command')
    need(math.isfinite(d['wall_s']) and d['wall_s']>0 and c['usage']['sys_us']>=0 and c['usage']['user_us']>0,'elapsed and CPU')
    need((names and DIRTY<=set(names)) if expected=='cold' else (len(names)==4 and set(names)==DIRTY) if expected=='edit' else names==[],'observed dirty crates/no-op')
    return d['wall_s']

def cohort(phase,number,v,q):
    seed=6302098800+number
    roles=list(itertools.permutations(sorted(ARMS)));random.Random(seed).shuffle(roles)
    need(v['phase']==phase and v['seed']==seed and v['closed']['all_workers_returned_zero'] and v['closed']['admitted']==q['admission_sha256'],'actual closed cohort')
    rows=[json.loads(s) for s in v['pairs_jsonl'].splitlines()];orders=[list(x)for x in roles]
    need(v['order']==orders==[list(x)for x in rows] and set(v['arms'])==set(ARMS),'six counterbalanced triples')
    cold={}
    for role in ARMS:
        a=v['arms'][role];identity=a['loaded'];tag=q['timed'][role];sdk=q['stock'] if tag=='stock' else q['optimized'][tag]
        need(identity['cargo']==CARGO and identity['driver']==sdk['driver_sha256'] and identity['llvm']==q['shared_llvm'] and identity['launcher']==sdk['launcher_sha256'],'timed process identity')
        need(a['protocol'][0]=={'patch':PATCH,'state':'ready'} and a['protocol'][1:]==[r[role]for r in rows],'worker dispatch')
        cold[role]=clock(a['setup']['warm'],'cold');clock(a['setup']['settled'],'noop')
        need(len(a['trials'])==6,'six real restores')
        for i,(x,row) in enumerate(zip(a['trials'],rows,strict=True),1):
            raw=row[role];need(raw['state']=='measured' and raw['trial']==str(i) and raw['dirty']==x['edit-D']['cargo']['dirty_units'],'row identity')
            for label,dirt,field in [('settled-A','noop','parent_noop'),('edit-D','edit','edit'),('noop-D','noop','noop')]:
                need(clock(x[label],dirt)==raw[field],'raw Cargo time differs')
    return rows,cold

def contrast(blocks,role,alpha,threshold):
    a,b=role;raw=blocks[0]+blocks[1]
    def value(row):
        # Reproduce the registered secondary stock/base arithmetic, including binary64 rounding.
        return (row['stock']['edit']/row['candidate']['edit'])/(row['parent']['edit']/row['candidate']['edit']) if role==('stock','parent') else row[a]['edit']/row[b]['edit']
    vals=list(map(value,raw));n=len(vals);need(n==12,'exactly twelve held-out pairs')
    k=max(k for k in range(n//2)if 2*sum(math.comb(n,i)for i in range(k+1))/2**n<=alpha)
    interval=[sorted(vals)[k],sorted(vals)[-k-1]]
    strata=[[value(r)for r in raw if (list(r).index(a)<list(r).index(b))==side]for side in (False,True)]
    need(list(map(len,strata))==[6,6],'relative order strata')
    seeds=[median(map(value,c))for c in blocks];orders=list(map(median,strata))
    untied=[v for v in vals if v!=threshold];m=len(untied);p=sum(math.comb(m,j)for j in range(sum(v>threshold for v in untied),m+1))/2**m
    return dict(median=median(vals),paired=vals,interval=interval,coverage=1-2*sum(math.comb(n,i)for i in range(k+1))/2**n,one_sided_sign_p=p,threshold=threshold,alpha=alpha,order_medians=orders,seed_medians=seeds,passed=interval[0]>threshold and p<alpha and min(seeds+orders)>threshold)

def analyze(doc):
    q=doc['qualification'];qualify(q)
    cohorts=[];cold=[]
    for i,ph in enumerate(PH,1):
        block,warm=cohort(ph,i,doc[ph],q);cohorts.append(block);cold.append(warm)
    x=cohorts[0];disc={role:median(r[role]['edit']/r['candidate']['edit']for r in x)for role in ARMS[:2]}
    need((disc['stock']>=1.8 and disc['parent']>1) or (disc['stock']>=1.2 and disc['parent']>=1.10),'original nomination predicate')
    cs=cohorts[1:];official=doc['score']
    pairs=[('direct_signed18_over_T3',('stock','candidate'),.05/3,2),('secondary_base_over_T3',('parent','candidate'),.05,1),('secondary_signed18_over_base',('stock','parent'),.05,1)]
    out={k:contrast(cs,pair,alpha,threshold)for k,pair,alpha,threshold in pairs}
    need(official['heldout_triples']==12 and all(official[k]==v for k,v in out.items()),'registered statistic differs')
    direct=out['direct_signed18_over_T3']['passed'] and min(out['secondary_base_over_T3']['seed_medians'])>1
    need(official['qualified_direct_twofold']==direct and official['positive_matched_effect']==out['secondary_base_over_T3']['passed'],'registered gate differs')
    warm={r:[block[r] for block in cold[1:]]for r in ARMS}
    return dict(heldout_triples=12,excluded_discovery_triples=6,discovery_ratios=disc,discovery_cold_s=cold[0],heldout_cold_s=warm,
                wall_seconds={r:{k:median(x[r][k]for block in cs for x in block)for k in ['edit','parent_noop','noop']}for r in ARMS},
                comparisons=out,qualified_direct_twofold=direct,positive_matched_effect=official['positive_matched_effect'])

def table(docs):
    out=io.StringIO();rows=csv.writer(out,lineterminator='\n')
    rows.writerow(['cohort','seed','round','actual_order',*[r+'_'+m+'_s' for r in ARMS for m in ('parent_noop','edit','noop')]])
    for ph in PH:
        d=docs[ph]
        for num,l in enumerate(d['pairs_jsonl'].splitlines(),1):
            row=json.loads(l);rows.writerow([ph,d['seed'],num,' '.join(row),*[row[r][m]for r in ARMS for m in ('parent_noop','edit','noop')]])
    return out.getvalue().encode()
def read_packet(p):
    with ZipFile(p)as z:
        names=set(z.namelist());allowed={'MANIFEST.json','times.csv','uv-20988.patch','source.json','qualification.json','score.json',*[ph+'.json'for ph in PH]}
        need(names==allowed and len(names)==len(z.namelist()),'unexpected/missing archive member')
        need(all(i.file_size<3<<20 for i in z.infolist()),'oversize member')
        files={n:z.read(n) for n in names};manifest=json.loads(files.pop('MANIFEST.json'))
        need({n:digest(b)for n,b in files.items()}==manifest and digest(files.pop('uv-20988.patch'))==PATCH,'member digests')
        csv_bytes=files.pop('times.csv');docs={n[:-5]:json.loads(b)for n,b in files.items()}
        need(table(docs)==csv_bytes,'per-trial table and returned rows')
    s=docs['source'];need(s['A']=='b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7' and s['B']=='1979349b68dbaa936b98c0f409c1df0598fa72f1' and s['tree_A']=='475eccff53635001b11fa5e2adebc9b27f5152e4' and s['tree_B']=='ec9a3f83e89381b5c509ca0c45fb4a87ec71289f' and s['parent_of_B']==s['A'] and s['patch_sha256']==PATCH and s['lock_A']==s['lock_B']=='74588b85635b44a3c37821d423f179c7a24fdcb3','historical source')
    return docs

def negatives(doc):
    from copy import deepcopy
    cases=[('discovery clocks substituted for holdout',lambda x:x.update({'confirm-1':x['discovery']})),('lost dirty crate',lambda x:x['confirm-2']['arms']['parent']['trials'][0]['edit-D']['cargo']['dirty_units'].pop()),
           ('reversed measured arms',lambda x:x['confirm-1']['order'][0].reverse()),('invented CPU clock',lambda x:x['confirm-2']['arms']['stock']['trials'][1]['edit-D']['usage'].update(user_us=-1)),
           ('invented gate',lambda x:x['score'].update(qualified_direct_twofold=not x['score']['qualified_direct_twofold'])),('wrong source',lambda x:x['qualification']['optimized']['T3'].update(header_format=12)),
           ('failed native suite',lambda x:x['qualification']['optimized']['base']['suites'][4].update(command_exit=1)),('missing workload',lambda x:x['qualification']['optimized']['T3']['training'].pop('externs'))]
    for name,change in cases:
        x=deepcopy(doc);change(x)
        try:analyze(x)
        except ValueError:continue
        raise ValueError('negative control accepted: '+name)
    return [n for n,_ in cases]
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('packet',nargs='?',type=Path,default=Path(__file__).with_name('records-007.zip'));p.add_argument('--negative',action='store_true');args=p.parse_args()
    doc=read_packet(args.packet);res=analyze(doc)
    if args.negative:res['rejected_controls']=negatives(doc)
    print(json.dumps(res,indent=2,sort_keys=True))
