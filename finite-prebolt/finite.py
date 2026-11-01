"""Offline verifier for the independent finite-receiver first-edit study, Python 3.10+."""
import argparse
import csv
import importlib.util
import io
import itertools
import json
from pathlib import Path
import random
import statistics
from zipfile import ZipFile
spec=importlib.util.spec_from_file_location('retained_007_stats',Path(__file__).with_name('base007.py'))
u=importlib.util.module_from_spec(spec);spec.loader.exec_module(u)

need=u.need
SEEDS=dict(zip(u.PH,(822049809,415670321,367210859)))
REG='158ca5f2df2d6559e1a7484cf8f19214dbc80267516922497366db6075909891'
STAGE='f3248b04128d45079b7ec04843e4ece71788979a3008d657da6fccebb84cd128'
T4='fd96f14140962cc722aa1335c4449c01f3f3a286'
TREE='7fd44bf57a11609d65d7feeea1db6b6279a77f04'
COMPILER='82b38d6000dfe18d60ec7c8d4ff5e68f702a5346'
PRIOR='e0146d0e13d2d37c6326995082a5081761decf8f115ae70e1c800c07213dd65f'
DEPS={'primitive':0,'pair':0,'wide':1,'specializing':0}
def qualify(q):
    t3,t4=q['optimized']['T3'],q['optimized']['T4']
    need(q['registration']==REG and q['issuer_registration']==STAGE and q['original_007_packet']==PRIOR and q['timed']=={'stock':'stock','parent':'T3','candidate':'T4'} and q['older_independent_performance_rows']==0,'prospective sources and roles')
    need(q['cargo']==u.CARGO and q['stock']['driver_sha256']==u.SDK['stock'][1] and q['stock']['release']['sources']['rust']['revision']==u.SDK['stock'][0],'signed stock and fixed Cargo')
    need((t3['commit'],t3['driver_sha256'])==u.SDK['T3'] and (t3['profile_sha256'],t3['profile_bytes'])==u.PROFILE['T3'] and t3['header_format']==10,'original pre-BOLT 007 T3')
    need(t4['commit']==T4 and t4['tree']==TREE and t4['compiler_tree']==COMPILER and t4['header_format']==12 and t4['source_parent']=='26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9','repaired v12 head and compiler')
    need(q['common_companions']>=70 and u.ishash(q['common_companions_sha256']) and t3['driver_sha256']!=t4['driver_sha256'] and t4['profile_sha256']!=t3['profile_sha256'],'different real subjects and common SDK')
    for role,a in [('T3',t3),('T4',t4)]:
        need(a['driver_BOLT'] is False and a['profile_bytes']>0 and a['profile_sha256']==a['use_profile_sha256'] and a['llvm_sha256']==q['shared_llvm'],'individual pre-BOLT source and profile')
        need(set(a['native_phases'])=={'generate','train','use','qualified'} and all(x['exit']==0 and all(u.ishash(x[k])for k in ('success_sha256','return_sha256','custody_sha256'))for x in a['native_phases'].values()),'closed build/training/use/native qualification')
        need({k:len(rows) for k,rows in a['training'].items()}==u.WORK and all(len(rows)==len(set(rows)) and all(x.startswith('eprintln-Test-'+k+'-')and x.endswith('-Threads1')for x in rows) for k,rows in a['training'].items()),'actual nine training workloads')
        need(len(a['suites'])==5 and {s['suite']for s in a['suites']}==set(u.SUITES),'actual five compiler suites')
        for s in a['suites']:
            need(s['command_exit']==0 and sorted(x['status']for x in s['mapped'])==u.SUITES[s['suite']] and all(all(x[k]==a[k]for k in ('driver_sha256','llvm_sha256','launcher_sha256'))for x in s['mapped']),'mapped compilers and expected child outcomes')
        expected=['3 3','3 3','7 3','3 7','3 3','3 7','3 3']
        need(a['solver_modes']==['default','next'] and [x[1]for x in a['dispatch']]==expected,'actual compiler dispatch oracle')
    f=t4['finite'];need((f['driver_sha256'],f['launcher_sha256'],f['profile_sha256'],f['header'],f['tree'],f['compiler'])==(t4['driver_sha256'],t4['launcher_sha256'],t4['profile_sha256'],12,TREE,COMPILER),'separate finite witness')
    need(f['raw_zip_sha256']==t4['finite_archive'] and set(f['solvers'])=={'T4-T4/default','T4-T4/next'} and all(set(x['provider_counts'])=={'A','unrelated','specialize','removed'} and x['provider_counts']['unrelated']==DEPS for x in f['solvers'].values()),'two solvers, finite cuts and oversized fallback')

def cohort(ph,d,q):
    seed=SEEDS[ph];choices=list(itertools.permutations(sorted(u.ARMS)));random.Random(seed).shuffle(choices)
    need(d['seed']==seed and d['order']==[list(x)for x in choices] and d['closed']['registration']==REG,'new phase and balanced seed')
    for role,a in d['arms'].items():
        m=a['native_loader'];l=a['loaded']
        need(m['affinity']==list(range(16)) and m['cargo']==l['cargo']==u.CARGO and m['launcher']==l['launcher'] and m['mapped']['librustc_driver'][1]==l['driver'] and m['mapped']['libLLVM'][1]==l['llvm'],'native loader, actual Cargo and CPU')
    return u.cohort(ph,seed,d,q)

def compute(doc):
    q=doc['qualification'];qualify(q)
    d=doc['discovery'];dis,cold=cohort('discovery',d,q)
    med=statistics.median;disc={r:med(x[r]['edit']/x['candidate']['edit']for x in dis)for r in ('stock','parent')}
    nominated=disc['stock']>=1.8 and disc['parent']>1
    score=doc['score'];need(score['opens_confirmation']==nominated and score['excluded_discovery_triples']==6 and score['historical_timing_rows']==0 and score['registration']==REG and score['admitted']==q['admission_sha256'],'actual discovery nomination')
    blocks=[dis];cost={'discovery':cold};comps={}
    if nominated:
        for ph in u.PH[1:]:
            need(ph in doc,'nominated discovery requires both independent complete heldouts')
            rows,cold=cohort(ph,doc[ph],q);blocks.append(rows);cost[ph]=cold
        for k,pair,alpha,target in [('signed18_over_T4',('stock','candidate'),.01,2),('optimized_T3_over_T4',('parent','candidate'),.01,1),('signed18_over_T3_context',('stock','parent'),.05,1)]:
            comps[k]=u.contrast(blocks[1:],pair,alpha,target);need(comps[k]==score[k],'registered interval/sign/seed/order')
        direct=comps['signed18_over_T4']['passed'] and min(comps['optimized_T3_over_T4']['seed_medians'])>1
        inc=comps['optimized_T3_over_T4']['passed'];joint=direct and inc
        need((score['heldout_triples'],score['qualified_direct_twofold'],score['positive_matched_effect'],score['joint'])==(12,direct,inc,joint) and score['independent_profiles'] is True and score['different_headers']==[10,12],'original direct, incremental and joint gates')
    else:
        need(not any(ph in doc for ph in u.PH[1:]) and score['heldout_triples']==0 and not any(k in score for k in ('joint','qualified_direct_twofold','positive_matched_effect')),'terminal discovery only')
        direct=inc=joint=None
    analyzed=blocks[1:]if nominated else blocks
    return dict(discovery_ratios=disc,nominated=nominated,excluded_discovery_triples=6,heldout_triples=score['heldout_triples'],cold_a_s=cost,
                comparison=comps,qualified_direct_twofold=direct,incremental_positive=inc,joint=joint,
                medians_s={r:{k:med(row[r][k]for bl in analyzed for row in bl)for k in ('edit','parent_noop','noop')}for r in u.ARMS})

def csv_bytes(doc):
    s=io.StringIO();w=csv.writer(s,lineterminator='\n');w.writerow(['cohort','seed','round','actual_order',*[r+'_'+k+'_s'for r in u.ARMS for k in ('parent_noop','edit','noop')]])
    for ph in u.PH:
        if ph not in doc:continue
        d=doc[ph]
        for i,l in enumerate(d['pairs_jsonl'].splitlines(),1):
            row=json.loads(l);w.writerow([ph,d['seed'],i,' '.join(row),*[row[r][k]for r in u.ARMS for k in ('parent_noop','edit','noop')]])
    return s.getvalue().encode()
def load(path):
    with ZipFile(path)as z:
        files={'MANIFEST.json','times.csv','uv-20988.patch','source.json','qualification.json','score.json','discovery.json'}
        full=files|{'confirm-1.json','confirm-2.json'}
        need(set(z.namelist())in (files,full) and len(set(z.namelist()))==len(z.namelist()) and all(i.file_size<2_000_000 for i in z.infolist()),'bounded exact members')
        raw={n:z.read(n)for n in z.namelist()}
    m=json.loads(raw.pop('MANIFEST.json'));need({n:u.digest(b)for n,b in raw.items()}==m and u.digest(raw.pop('uv-20988.patch'))==u.PATCH,'raw archive identities')
    csvdata=raw.pop('times.csv');doc={n[:-5]:json.loads(v)for n,v in raw.items()};s=doc['source']
    need(s['A']==s['parent_of_B']=='b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7' and s['B']=='1979349b68dbaa936b98c0f409c1df0598fa72f1' and s['lock_A']==s['lock_B']=='74588b85635b44a3c37821d423f179c7a24fdcb3' and s['tree_A']=='475eccff53635001b11fa5e2adebc9b27f5152e4' and s['tree_B']=='ec9a3f83e89381b5c509ca0c45fb4a87ec71289f' and s['patch_sha256']==u.PATCH and csv_bytes(doc)==csvdata,'literal source and per-trial table')
    return doc
def negatives(doc):
    from copy import deepcopy
    changes={'wrong driver':lambda d:d['discovery']['arms']['candidate']['native_loader']['mapped']['librustc_driver'].__setitem__(1,'0'*64),
             'wrong header':lambda d:d['qualification']['optimized']['T4'].update(header_format=10),
             'old seed':lambda d:d['discovery'].update(seed=6302098801),
             'lost dirty crate':lambda d:d['discovery']['arms']['stock']['trials'][0]['edit-D']['cargo']['dirty_units'].pop(),
             'failed suite':lambda d:d['qualification']['optimized']['T4']['suites'][0].update(command_exit=1),
             'missing workload':lambda d:d['qualification']['optimized']['T4']['training'].pop('externs'),
             'typed fallback changed':lambda d:d['qualification']['optimized']['T4']['finite']['solvers']['T4-T4/next']['provider_counts']['unrelated'].update(wide=0),
             'discovery gate inverted':lambda d:d['score'].update(opens_confirmation=not d['score']['opens_confirmation'])}
    if 'confirm-1'in doc:changes.update({'partial confirmation':lambda d:d.pop('confirm-1'),'invented direct win':lambda d:d['score'].update(qualified_direct_twofold=not d['score']['qualified_direct_twofold'])})
    for name,change in changes.items():
        d=deepcopy(doc);change(d)
        try:compute(d)
        except ValueError:continue
        raise ValueError('corruption was accepted: '+name)
    return list(changes)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('packet',nargs='?',type=Path,default=Path(__file__).with_name('records.zip'));p.add_argument('--negative',action='store_true');a=p.parse_args()
    d=load(a.packet);r=compute(d)
    if a.negative:r['rejected_controls']=negatives(d)
    print(json.dumps(r,sort_keys=True,indent=2))
