"""Verify the two fixed-format PGO contrasts, or the distinct untrained #296 replication."""
import argparse
import csv
import io
import importlib.util
import json
from pathlib import Path
from statistics import median
from zipfile import ZipFile
D=Path(__file__).resolve().parent
def module(p,name):
    spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
u=module(D/'base007.py','paired_cargo_007')
need=u.need
FIX='fixed-format-12'
REP='repaired-untrained'
PLANS={FIX:('c4f39c881fd9fcccaf87412a266ab16c808946787c7cfc6592573b3268d5ef1a',{'discovery':2492922301,'confirm-1':438041005,'confirm-2':1371168561}),REP:('62e1dd1849dc1210a5f74956240da81d23dab84985b8513db97bef1e120b54d9',{'confirm-1':110129611,'confirm-2':110129612})}
T1='97ad3866dfe788d2ed0bd8f945a9972423da2423';H3='26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9';T4='fd96f14140962cc722aa1335c4449c01f3f3a286'
TARGET={FIX:((H3,12),(T4,12)),REP:((T1,11),(H3,12))}
def qualify(q):
    case=q['study'];need(q['registration']==PLANS[case][0] and q['historical_rows']==0 and q['cargo']==u.CARGO and q['stock']['driver_sha256']==u.SDK['stock'][1],'original plan and signed subjects')
    p,c=(q['subjects'][k]for k in ('parent','candidate'))
    need(tuple((x['commit'],x['header_format'])for x in (p,c))==TARGET[case] and len({x['driver_sha256']for x in (p,c,q['stock'])})==3 and p['llvm_sha256']==c['llvm_sha256']==q['shared_llvm'],'all three real native roles')
    if case==FIX:
        need(q['common_companions']>=70 and p['profile_sha256']!=c['profile_sha256'] and p['driver_sha256']=='02dce4f3bff6c0221d3ff88afc6648a2990f21ff4a248327871245277f2e1837' and p['profile_sha256']=='ad75321505ddba72dbfede005c3928184ae5f8aca56630e68723abe16e395ce3' and c['driver_sha256']=='f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642' and c['profile_sha256']=='70892579bb848a9eb268d1d74538e53b849f3de4c0df6a05a504f071bdb869ea','new v12 parent and actual tested finite child')
        for arm,positive in ((p,False),(c,True)):
            need(arm['driver_BOLT'] is False and arm['profile_bytes']>0 and arm['profile_sha256']==arm['use_profile_sha256'] and {n:len(v)for n,v in arm['training'].items()}==u.WORK,'independent training and profile-use')
            need(set(arm['native_phases'])=={'generate','train','use','qualified'} and all(row['exit']==0 and all(u.ishash(row[k]) for k in ('success_sha256','return_sha256','custody_sha256'))for row in arm['native_phases'].values()),'four genuinely closed phases')
            need(len(arm['suites'])==5 and {x['suite']for x in arm['suites']}==set(u.SUITES),'five actual official suites')
            for s in arm['suites']:
                need(s['command_exit']==0 and sorted(x['status']for x in s['mapped'])==u.SUITES[s['suite']] and all(all(x[n]==arm[n]for n in ('driver_sha256','launcher_sha256','llvm_sha256'))for x in s['mapped']),'five mapped compiler/diagnostic assignments')
            f=arm['finite'];role='T4'if positive else'T3'
            need(f['role']==role and f['driver_sha256']==arm['driver_sha256'] and f['profile_sha256']==arm['profile_sha256'] and f['header']==12 and set(f['solvers'])=={role+'-'+role+'/'+s for s in ['default','next']},'separate optimized native finite controls')
            for x in f['solvers'].values():
                need(set(x['provider_counts'])=={'A','unrelated','specialize','removed'} and x['provider_counts']['unrelated']=={'primitive':int(not positive),'pair':int(not positive),'wide':1,'specializing':0},'actual finite provider negative and positive')
            need(arm['solver_modes']==['default','next'] and [a[1]for a in arm['dispatch']]==['3 3','3 3','7 3','3 7','3 3','3 7','3 3'],'seven native canonical dispatches')
    else:
        need(q['pgo'] is False and q['fixed_non_driver_members']==72 and set(q['migration'])=={'forced','unforced'},'untrained native admissions')
        need(q['typed_selector_registration']=='0432719c07f539e3cf4e4bc8b7b15dbe59bee69dcbd622703592b21ebe1f5de7','effective registered repaired entrance')
        for tool,driver,expected in [(p,'d043561dbbec02881096b8f459673f1e509d2fb1fccce733c7c95b13073f1537',['3','3','7','3']),(c,'45e1a53eea14bc393881e3d9a1cc1ce87fc750d6f8cabf20b10c77a14f308824',['3 3','3 3','7 3','3 7','3 3','3 7','3 3'])]:
            n=tool['native_receipt']
            need(tool['driver_sha256']==driver==n['driver_sha256'] and n['executable_runs']==2*len(expected) and n['compiler_executions']==3*len(expected) and n['observed_roster_revisions']==len(expected) and n['stdout_by_solver']=={k:[x+'\n'for x in expected]for k in ('default','next')},'actual untrained tool, suite and both solver programs')
        for m in q['migration'].values():
            need((m['own_cache_loaded'],m['different_format_rejected'],m['genuine_empty_controls'],m['native_performance_rows'])==(5,14,5,0),'independent cache transitions')
            for tag,arm in [('new11',p),('new12',c)]:
                need(all(m['subjects'][tag][k]==arm[k]for k in ('driver_sha256','launcher_sha256','commit','tree','header_format')),'migration used the admitted bytes')
def cohort(ph,d,q):
    reg,seeds=PLANS[q['study']];s=seeds[ph]
    need(d['closed']['registration']==reg,'right physical cohort')
    if q['study']==FIX:
        for a in d['arms'].values():
            loader=a['native_loader'];l=a['loaded'];need(loader['affinity']==list(range(16)) and loader['cargo']==u.CARGO==l['cargo'] and loader['launcher']==l['launcher'] and loader['mapped']['libLLVM'][1]==l['llvm'] and loader['mapped']['librustc_driver'][1]==l['driver'],'actual mapped process')
    else:
        for a in d['arms'].values():
            for version,end in [('A',a['setup']['settled']),('B',a['trials'][-1]['noop-D'])]:
                snapshots=a['settled_products'][version]
                need(set(snapshots)==u.DIRTY and all(u.ishash(x['rmeta'])and u.ishash(x['depinfo']) for x in snapshots.values()) and {k:{f:w for f,w in x.items()if f not in ('rmeta','depinfo')}for k,x in snapshots.items()}==end['products'],'actual settled rmeta/depinfo content')
            for clocks in [a['setup'],*a['trials']]:
                labels=list(clocks)
                for left,right in zip(labels,labels[1:]):
                    before,after=clocks[left]['products'],clocks[right]['products']
                    need(set(before)==set(after)==u.DIRTY,'four substantiated check products')
                    for unit,l in before.items():
                        r=after[unit];need(l['key']==r['key'] and l['features']==r['features'] and r['rmeta_bytes']>16 and (r['stamp']>l['stamp']if right=='edit-D'else l==r),'real Cargo product stamp')
    compat={'stock':q['stock'],'optimized':q['subjects'],'timed':{x:x for x in u.ARMS},'admission_sha256':q['admission_sha256'],'shared_llvm':q['shared_llvm']}
    return u.cohort(ph,s,d,compat)
def csv_bytes(doc):
    out=io.StringIO();wr=csv.writer(out,lineterminator='\n');wr.writerow(['cohort','seed','round','execution_order',*[r+'_'+k+'_s'for r in u.ARMS for k in ('parent_noop','edit','noop')]])
    for ph in PLANS[doc['qualification']['study']][1]:
        if ph not in doc:continue
        for idx,l in enumerate(doc[ph]['pairs_jsonl'].splitlines(),1):
            r=json.loads(l);wr.writerow([ph,doc[ph]['seed'],idx,' '.join(r),*[r[a][k]for a in u.ARMS for k in ('parent_noop','edit','noop')]])
    return out.getvalue().encode()
def load(p):
    with ZipFile(p)as z:
        need(len(set(z.namelist()))==len(z.namelist()) and all(x.file_size<2_000_000 for x in z.infolist()),'bounded ordinary files');raw={n:z.read(n) for n in z.namelist()}
    m=json.loads(raw.pop('MANIFEST.json'));need({k:u.digest(v)for k,v in raw.items()}==m and u.digest(raw.pop('uv-20988.patch'))==u.PATCH,'actual zip/patch identities')
    csv_data=raw.pop('times.csv');doc={k[:-5]:json.loads(v)for k,v in raw.items()};q=doc['qualification'];ph=PLANS[q['study']][1]
    need(set(doc)=={'source','qualification','score',*ph} or (q['study']==FIX and set(doc)=={'source','qualification','score','discovery'}),'complete eligible or excluded phases')
    s=doc['source'];need(s['A']==s['parent_of_B']=='b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7' and s['B']=='1979349b68dbaa936b98c0f409c1df0598fa72f1' and s['lock_A']==s['lock_B']=='74588b85635b44a3c37821d423f179c7a24fdcb3' and s['tree_A']=='475eccff53635001b11fa5e2adebc9b27f5152e4' and s['tree_B']=='ec9a3f83e89381b5c509ca0c45fb4a87ec71289f' and csv_bytes(doc)==csv_data,'all source trees and raw clocks')
    return doc
def compute(doc):
    q=doc['qualification'];qualify(q);case=q['study'];score=doc['score'];ph=[k for k in PLANS[case][1] if k in doc];rows={};cold={}
    for k in ph:rows[k],cold[k]=cohort(k,doc[k],q)
    eligible=None;ds={}
    if case==FIX:
        need('discovery'in rows,'separate discovery')
        ds={r:median(x[r]['edit']/x['candidate']['edit']for x in rows['discovery'])for r in ('stock','parent')};eligible=ds['stock']>=1.8 and ds['parent']>1
        need(eligible==score['opens_confirmation'] and score['excluded_discovery_triples']==6 and score['registration']==q['registration'] and score['admitted']==q['admission_sha256'] and score['historical_timing_rows']==0,'original nomination and context')
    scoring=case==REP or eligible
    if scoring:need(set(rows)>={'confirm-1','confirm-2'},'both heldout blocks required')
    else:need(set(rows)=={'discovery'} and score['heldout_triples']==0,'no holds after discovery miss')
    blocks=[rows[n]for n in ('confirm-1','confirm-2')]if scoring else[]
    contrasts={};direct=inc=material=None
    if case==FIX and blocks:
        alpha=.05/7
        for k,pair,a,t in [('signed18_over_T4',('stock','candidate'),alpha,2),('matched_v12_T3_over_T4',('parent','candidate'),alpha,1),('signed18_over_T3_context',('stock','parent'),.05,1)]:
            contrasts[k]=u.contrast(blocks,pair,a,t);need(contrasts[k]==score[k],'five earlier plus two matched endpoint tests')
        direct=contrasts['signed18_over_T4']['passed'] and min(contrasts['matched_v12_T3_over_T4']['seed_medians'])>1
        inc=contrasts['matched_v12_T3_over_T4']['passed'];need((score['heldout_triples'],score['qualified_direct_twofold'],score['positive_matched_effect'],score['joint'],score['headers'])==(12,direct,inc,direct and inc,[12,12]),'independent direct/incremental/joint gates')
    if case==REP:
        recovery=q['post_clock_recovery']
        need(recovery['recovery']=='0f7076c4f7d5dfa697047d4b306a8e579879e094acd5a72ace1178b280c2523d' and recovery['original_failed_validator_return']==1 and recovery['original_physical_files']==48 and recovery['six_triples']==6 and recovery['actual_dispatches']==18,'original failed postprocessing retained and recovered separately')
        need(doc['confirm-1']['original_closure_sha256']=='f6c1be134e5af6f2546ae2fd05669926b6f5892d15d4489009fb5a06430c9931' and recovery['original_compatible_closure']==doc['confirm-1']['closed']['original_completed_receipt'],'actual independent first closure')
        for ph in ('confirm-1','confirm-2'):
            need(doc[ph]['closed']['original_completed_receipt']==dict(phase=ph,registration=q['registration'],subjects=q['admission_sha256'],raw=u.digest(doc[ph]['pairs_jsonl'].encode())),'exact original source rows and actual admission')
        registered=score['repaired_result'];need(registered['heldout_triples']==12,'twelve unscreened holds')
        for role in ('stock','parent'):
            v=u.contrast(blocks,(role,'candidate'),.05,1);contrasts[role]=v
            for name,computed in [('median','median'),('interval','interval'),('sign_p_above_one','one_sided_sign_p'),('order_medians','order_medians'),('seed_medians','seed_medians'),('paired','paired')]:
                want=v['interval']+[v['coverage']] if name=='interval' else v[computed]
                need(registered['contrasts'][role][name]==want,'original untrained ratio/interval and coverage')
        pos=all(v['interval'][0]>1 and v['one_sided_sign_p']<.05 for v in contrasts.values());inc=pos
        material=pos and min(contrasts['stock']['seed_medians'])>=1.2 and min(contrasts['parent']['seed_medians'])>=1.1 and min(contrasts['parent']['order_medians'])>1
        direct=u.contrast(blocks,('stock','candidate'),.05,2)['passed'] and min(contrasts['parent']['seed_medians'])>1
        cpu=[]
        for ph in ('confirm-1','confirm-2'):
            h=doc[ph]['arms']
            for i in range(6):
                nums=[sum(h[r]['trials'][i]['edit-D']['usage'][j]for j in ('user_us','sys_us'))for r in ('parent','candidate')]
                cpu.append(nums[0]/nums[1])
        need(median(cpu)==registered['matched_child_cpu_median'],'actual paired child CPU')
        need((registered['material'],registered['twofold']['qualified'],score['above_one_on_both'],score['replicates_material_result'],score['twofold'])==(material,direct,pos,material,direct),'untrained material and separate twofold gates')
    flat=sum(blocks,[])if blocks else rows['discovery']
    return dict(study=case,nominated=eligible,discovery_ratios=ds,heldout_triples=12 if blocks else 0,excluded_discovery_triples=6 if case==FIX else 0,comparison=contrasts,
                direct_twofold=direct,incremental_positive=inc,material=material,joint=(direct and inc)if blocks and case==FIX else None,
                absolute_seconds={k:{f:median(r[k][f]for r in flat)for f in ('edit','parent_noop','noop')}for k in u.ARMS},cold_parent_seconds=cold)
def negatives(doc):
    from copy import deepcopy
    ph='confirm-2'if 'confirm-2'in doc else 'discovery'
    tests={'earlier driver substituted':lambda x:x['qualification']['subjects']['parent'].update(driver_sha256=u.SDK['T3'][1]),'wrong seed':lambda x:x[ph].update(seed=0),'lost dirty unit':lambda x:x[ph]['arms']['candidate']['trials'][0]['edit-D']['cargo']['dirty_units'].pop(),'invalid CPU':lambda x:x[ph]['arms']['parent']['trials'][0]['edit-D']['usage'].update(user_us=-1),'incorrect header':lambda x:x['qualification']['subjects']['candidate'].update(header_format=10)}
    if 'confirm-1'in doc:tests['incomplete heldouts']=lambda x:x.pop('confirm-1')
    if doc['qualification']['study']==FIX:tests['fallback counter']=lambda x:x['qualification']['subjects']['parent']['finite']['solvers']['T3-T3/next']['provider_counts']['unrelated'].update(primitive=0)
    else:tests['changed no-op stamp']=lambda x:x[ph]['arms']['candidate']['trials'][0]['noop-D']['products']['uv_resolver'].update(stamp=0)
    for n,replace in tests.items():
        copy=deepcopy(doc);replace(copy)
        try:compute(copy)
        except ValueError:continue
        raise ValueError('negative control accepted: '+n)
    return list(tests)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('packet',nargs='?',type=Path,default=D/'records.zip');p.add_argument('--negative',action='store_true');a=p.parse_args()
    doc=load(a.packet);result=compute(doc)
    if a.negative:result['rejected_controls']=negatives(doc)
    print(json.dumps(result,indent=2,sort_keys=True))
