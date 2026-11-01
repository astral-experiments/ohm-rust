"""Offline verification of the two high-format #20988 experiments. Python 3.11+."""
import argparse
import csv
import io
import re
from copy import deepcopy
import hashlib
import importlib.util
import itertools
import json
from pathlib import Path
from statistics import median
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
sha = lambda data: hashlib.sha256(data).hexdigest()
policy_bytes = (HERE / 'policy.json').read_bytes()
if sha(policy_bytes) != '4244d647bbea20a2769a108e18dd9faa68399766736d8c01bf882dae602127e8':
    raise ValueError('source and study policy changed')
POLICY = json.loads(policy_bytes)
def module(name):
    p = HERE / (name + '.py')
    if sha(p.read_bytes()) != POLICY['core_sources'][p.name]:
        raise ValueError('published reader changed')
    spec = importlib.util.spec_from_file_location(name, p)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m
u = module('base007')
d942 = module('d942')
need = u.need
PHASES = list(u.PH)
RESTORED_SEEDS = dict(zip(PHASES, [3040683011, 3040683012, 3040683013]))
FRESH_SEEDS = [406833041, 406833067]
FIELDS = {'user_us','system_us','minor_faults','major_faults','voluntary_context_switches','involuntary_context_switches'}
USAGE = {f'{i:02}-{arm}-{ph}' for i,arm,ph in itertools.product(range(1,13),u.ARMS,d942.PHASES)}
IDS = ['driver_sha256','launcher_sha256','llvm_sha256']
def qualify(q):
    need(q['registration']==POLICY['restored_registration'] and q['decision']==POLICY['decision'] and q['migration']==POLICY['migration'] and q['matrix_registration']==POLICY['matrix_dispatch'], 'genuine high-format scientific and migration gates')
    need(q['scored_prior_rows']==0 and q['common_companions']>=70 and u.ishash(q['admission_sha256']) and u.ishash(q['common_companions_sha256']), 'complete positive subject enrollment')
    need(q['stock']==POLICY['signed'] and q['cargo']==u.CARGO and q['llvm']==POLICY['llvm'] and set(q['subjects'])=={'parent','candidate'}, 'fixed shipped baseline and both optimized roles')
    for role, arm in q['subjects'].items():
        src=POLICY['heads'][role]
        for a,b in [('commit','new_commit'),('tree','tree'),('compiler_tree','compiler_tree'),('header_format','new_version')]:
            need(arm[a]==src[b], 'wrong source revision or incremental header: '+role)
        need(arm['producer']==POLICY['producers'][role] and arm['pre_BOLT'] and arm['header_format']==40683, 'role-specific separately trained source')
        need(q['admission_sha256']==POLICY['admission'] and arm['cargo_sha256']==POLICY['bundled_cargo']!=q['cargo'] and arm['driver_sha256']==POLICY['optimized_drivers'][role] and arm['profile_sha256']==POLICY['optimized_profiles'][role] and arm['llvm_sha256']==q['llvm'] and all(u.ishash(arm[k])for k in IDS), 'exact timed SDK and loader')
        need(arm['driver_sha256'] not in POLICY['blocked_drivers'] and arm['profile_sha256'] not in POLICY['blocked_profiles'] and arm['profile_sha256']==arm['use_profile_sha256'] and arm['profile_bytes']>0 and u.ishash(arm['profile_sha256']), 'fresh generated profile and driver')
        need({k:len(v)for k,v in arm['training'].items()}==u.WORK and u.ishash(arm['training_receipt']), 'actual nine training workloads')
        need(set(arm['phases'])=={'generate','train','use','qualified'} and all(v['exit']==0 and all(u.ishash(v[k])for k in ('issuer','return','durable'))for v in arm['phases'].values()), 'positive custody for all four phases')
        need(len(arm['suites'])==5 and {s['name']for s in arm['suites']}==set(u.SUITES),'five native suites')
        for s in arm['suites']:
            need(s['exit']==0 and sorted(z['status']for z in s['mapped'])==u.SUITES[s['name']] and all(all(z[k]==arm[k]for k in IDS)for z in s['mapped']), 'actual tested compiler, not just mapped filenames')
        finite=arm['finite'];tag='T3' if role=='parent' else 'T4'
        need(finite['role']==tag and finite['header']==40683 and finite['compiler']==arm['compiler_tree'] and finite['tree']==arm['tree'] and finite['profile_sha256']==arm['profile_sha256'] and finite['driver_sha256']==arm['driver_sha256'] and finite['launcher_sha256']==arm['launcher_sha256'] and finite['raw_zip_sha256']==arm['finite_archive'] and set(finite['solvers'])=={f'{tag}-{tag}/{solver}' for solver in ('default','next')}, 'two real optimized finite-receiver controls')
        for solver in finite['solvers'].values():
            need(solver['provider_counts']['unrelated']=={'primitive':int(role=='parent'),'pair':int(role=='parent'),'wide':1,'specializing':0}, 'actual reuse and fallback under both solvers')
        need(arm['solver_modes']==['default','next'] and [d[1]for d in arm['dispatch']]==['3 3','3 3','7 3','3 7','3 3','3 7','3 3'],'canonical runtime dispatch')
    need(len({row['durable'] for x in q['subjects'].values()for row in x['phases'].values()})==8,'eight independently issued phase books')
    need(len({q['stock']['driver_sha256'],*(s['driver_sha256']for s in q['subjects'].values())})==3 and len({s['profile_sha256']for s in q['subjects'].values()})==2, 'three genuine compilers, two independent profiles')
def restored_check(phased,score,q,*,registration,seeds,alpha,header,matched,configs=None):
    compat=dict(stock=q['stock'],optimized=q['subjects'],timed={n:n for n in u.ARMS},admission_sha256=q['admission_sha256'],shared_llvm=q['llvm'])
    need(score['registration']==registration and score['admitted']==q['admission_sha256'] and score['historical_timing_rows']==0 and score['excluded_discovery_triples']==6,'registered separated three-way study')
    checked={};cold={};dirty={}
    for ph,d in phased.items():
        need(ph in seeds and d['closed']['registration']==registration and d['closed']['files']['pairs.jsonl']==sha(d['pairs_jsonl'].encode()),'cohort and original digest')
        if configs is not None:need(d['closed']['configuration']==configs[ph],'recorded configuration for this seed')
        for role,arm in d['arms'].items():
            ids=arm['loaded'];loader=arm['native_loader']
            if registration==POLICY['restored_registration']:
                need(ids['sdk_cargo_before_overlay']==(q['cargo'] if role=='stock' else POLICY['bundled_cargo']), 'restored original Cargo before fixed overlay')
            need(loader['cargo']==ids['cargo'] and loader['launcher']==ids['launcher'] and loader['mapped']['libLLVM'][1]==ids['llvm'] and loader['mapped']['librustc_driver'][1]==ids['driver'] and loader['affinity']==list(range(16)),'actual loaded compiler at scheduled CPU set')
        checked[ph],cold[ph]=u.cohort(ph,seeds[ph],d,compat)
        dirty[ph]={k:len(v['setup']['warm']['cargo']['dirty_units'])for k,v in d['arms'].items()}
    discovery={arm:median(t[arm]['edit']/t['candidate']['edit'] for t in checked['discovery'])for arm in ('stock','parent')}
    go=discovery['stock']>=1.8 and discovery['parent']>1
    need(go==score['opens_confirmation'] and set(phased)==(set(PHASES)if go else {'discovery'}) and score['heldout_triples']==(12 if go else 0),'only fully closed, prospectively eligible cohorts')
    contrasts={};direct=incremental=None
    if go:
        holds=[checked['confirm-1'],checked['confirm-2']]
        for name,pair,a,threshold in [('signed18_over_T4',('stock','candidate'),alpha,2),(matched,('parent','candidate'),alpha,1),('signed18_over_T3_context',('stock','parent'),.05,1)]:
            calculated=u.contrast(holds,pair,a,threshold)
            need(calculated==score[name],'independent score mismatch: '+name);contrasts[name]=calculated
        direct=contrasts['signed18_over_T4']['passed'] and min(contrasts[matched]['seed_medians'])>1
        incremental=contrasts[matched]['passed']
        need((score['qualified_direct_twofold'],score['positive_matched_effect'],score['joint'],score['headers'])==(direct,incremental,direct and incremental,[header,header]),'registered direct and incremental decisions')
    samples=sum([checked[x]for x in PHASES[1:]],[]) if go else checked['discovery']
    return dict(excluded_discovery=6,discovery_medians=discovery,heldouts=12 if go else 0,direct=direct,incremental=incremental,
                contrasts=contrasts,cold_seconds=cold,cold_dirty_counts=dirty,median_seconds={n:{k:median(r[n][k]for r in samples)for k in ('parent_noop','edit','noop')}for n in u.ARMS},
                dirty_counts_per_observation={'settled-A':0,'edit-D':4,'noop-D':0},headers=[header,header])
CUSTODY_MEMBERS={'custody/archive-manifest.json','custody/restored-sdks.json','custody/archived-issuance.json','physical/mounted-inputs.json','physical/enrolled.json'}
def archive_custody(m,q):
    names={'custody/archive-manifest.json':'archive_manifest','custody/restored-sdks.json':'restore','custody/archived-issuance.json':'deep_issuance','physical/mounted-inputs.json':'native_inputs','physical/enrolled.json':'enrolled'}
    need(all(sha(m[n])==POLICY[pk]for n,pk in names.items()),'actual immutable archive and physical qualification receipts')
    manifest,restore,deep,inputs,enrolled=[json.loads(m[n]) for n in names]
    fixed=POLICY['archive']
    need(manifest['sha256']==deep['archive']==restore['archive']==inputs['archive_sha256']==fixed and manifest['bytes']==restore['bytes']==POLICY['archive_bytes'],'actual tested SDK archive')
    need(restore['compared'] and restore['source_manifest']==deep['manifest']==POLICY['archive_manifest'] and deep['restore']==POLICY['restore'] and deep['archive_and_profiles_rehashed'],'native byte comparison, full archive and both PGO profiles')
    need(restore['admission']==manifest['admission_sha256']==deep['issuance_sha256']==inputs['issued']==enrolled['issuance_sha256']==q['admission_sha256'],'same original trained cohorts')
    need(q['registration']==POLICY['restored_registration'] and enrolled['native_gate']==POLICY['matrix_dispatch'] and inputs['registration']==enrolled['registration']==POLICY['fresh_registration'],'correct physical attempt')
    need(deep['sdk_files']==inputs['complete_mounted_sdk_files']=={'stock':71,'parent':77,'candidate':77} and inputs['cargo_custody']=={'sdk_bin_cargo':{'stock':q['cargo'],'parent':POLICY['bundled_cargo'],'candidate':POLICY['bundled_cargo']},'explicit_cargo':q['cargo']},'all mounted archive SDKs and fixed executable')
    need(set(manifest['roles'])==set(restore['roots'])==set(deep['subjects'])==set(q['subjects'])=={'parent','candidate'},'both optimizer roles')
    for role,tag in [('parent','H3'),('candidate','T4')]:
        a=q['subjects'][role];arm=manifest['roles'][role];rest=restore['roots'][role];fs=arm['files']
        prefix='trait-rosters-high-40683-'+tag.lower()+'-prebolt-002/'
        need(arm['sdk']==rest['sdk']==prefix+'use-'+tag+'/sdk' and arm['profile']==prefix+'train-'+tag+'/rustc.profdata' and arm['sdk'] in manifest['members'] and arm['profile'] in manifest['members'],'original relative archive members')
        need(rest==dict(driver=a['driver_sha256'],files=77,profile=a['profile_sha256'],sdk=arm['sdk']) and arm['commit']==a['commit'] and arm['driver_sha256']==a['driver_sha256'] and arm['profile_sha256']==a['profile_sha256'] and arm['qualification']=={k:i['durable']for k,i in a['phases'].items()},'matching actual PGO profiles, protected phases and compilers')
        need(len(fs)==77 and all(not k.startswith('/') and '..' not in k.split('/') and u.ishash(row[0]) and type(row[1])is int and row[1]>0 and row[2]in [0o600,0o644,0o700,0o755]for k,row in fs.items()),'protected entries, sizes and modes')
        need(arm['driver']==a['driver_relative'] and fs[arm['driver']][0]==a['driver_sha256'] and fs['bin/rustc'][0]==a['launcher_sha256'] and fs['bin/cargo'][0]==POLICY['bundled_cargo'],'archived compiler, launcher and SDK Cargo')
        need(deep['subjects'][role]==dict(commit=a['commit'],driver=a['driver_sha256'],profile=a['profile_sha256'],phases=4,suites=5),'genuine native qualification intake')
    return inputs,enrolled
def fresh_check(records,usage,closure,q):
    need(set(records)==d942.ALL_RECORDS and set(usage)==USAGE,'all 36 physical parents, four observations each and out-of-clock counters')
    previously=d942.SEEDS;d942.SEEDS=FRESH_SEEDS
    try:result,table=d942.records(records)
    finally:d942.SEEDS=previously
    need(all(set(v)==FIELDS and all(type(x)is int and x>=0 for x in v.values())for v in usage.values()),'144 actual observational resource rows')
    inp=closure['inputs'];verified=closure['verified'];ret=closure['return'];plan=json.loads(records['plan.json'])
    pro=closure['process'];argv=pro['argv']
    need(pro['exit']==0 and pro['complete'] and argv==['taskset','-c','0-15','/usr/bin/python3','-ISB','entry.py','--uv','/tmp/finite-rerun/inputs/bank/uv.git','--stock','/tmp/finite-rerun/inputs/stock','--parent','/tmp/finite-rerun/inputs/parent','--candidate','/tmp/finite-rerun/inputs/candidate','--cargo','/tmp/finite-rerun/inputs/stock/bin/cargo','--cargo-home','/tmp/finite-rerun/inputs/bank','--scratch','/tmp/finite-rerun/run'],'actual cold-parent dispatch and normal exit')
    need(closure['enrolled']['issuance_sha256']==q['admission_sha256'] and closure['enrolled']['native_gate']==POLICY['matrix_dispatch'] and closure['enrolled']['registration']==POLICY['fresh_registration'],'physical worker enrollment')
    need(inp['cargo_custody']=={'sdk_bin_cargo':{'stock':q['cargo'],'parent':POLICY['bundled_cargo'],'candidate':POLICY['bundled_cargo']},'explicit_cargo':q['cargo']},'external installed SDKs and explicit published Cargo')
    desc=inp['describe']
    need(desc['A']==d942.A and desc['B']==d942.B and desc['seeds']==FRESH_SEEDS and desc['orders']==plan['order'] and desc['drivers']=={k:v['driver']for k,v in inp['roles'].items()} and desc['patch']==u.PATCH and desc['flags']==d942.FLAGS and desc['alpha']==.01,'actual high strict program config')
    need(ret['exit']==0 and ret['registration']==POLICY['fresh_registration']==inp['registration']==verified['registration'] and ret['verified_sha256']==closure['verified_sha256'] and ret['log_sha256']==closure['log_sha256'],'external native process returned normally')
    need(inp['issued']==verified['high_admission']==closure['issuance_sha256']==q['admission_sha256'] and verified['headers']==[40683,40683] and verified['prior_timed_rows']==0,'independent high-format study and separately qualified SDKs')
    need(inp['source']['patch']==u.PATCH and inp['source']['crates']==inp['source']['sparse_checksum_rows']==658 and inp['source']['archives_bytes']==79475307 and inp['source']['tag']=='46ead609859e3c8f8572ca44e766fa003fbec43a' and inp['source']['describe']==['0.12.2-14-gb60da7a','0.12.2-15-g1979349'] and plan['registry_configs']=={'/.cargo/config.toml':inp['source']['config']},'actual locked source and registry')
    for name in u.ARMS:
        a=q['stock'] if name=='stock'else q['subjects'][name]
        info=inp['roles'][name];p=plan['sdks'][name]
        need(info=={'launcher':a['launcher_sha256'],'driver':a['driver_sha256'],'llvm':q['llvm']} and p['launcher']==info['launcher'] and p['librustc_driver']['sha256']==info['driver'] and p['libLLVM']['sha256']==info['llvm'],'physical-parent launch and actual loaded bytes')
    summary=json.loads(records['summary.json']);cmp=result['comparisons'];ds={k:v['passed']for k,v in cmp.items()}
    need(not summary['compiler_identity']['changed_compilers'] and verified['comparisons']==summary and verified['fresh_parent_decisions']==ds and verified['fresh_parent_joint']==all(ds.values()),'actual external decision and unchanged compiler IDs')
    need(verified['ordinary_results']=={k:sha(v)for k,v in records.items()} and verified['usage_sidecars']==closure['usage_sha256'],'physical 435 original hashes and 144 genuine sidecars')
    return result
def tables(members):
    get=lambda n:json.loads(members[n])
    out=io.StringIO();w=csv.writer(out,lineterminator='\n')
    w.writerow(['phase','seed','round','order','role','cold_parent_s','settled_parent_s','first_edit_s','settled_edit_s','cold_dirty','parent_noop_dirty','edit_dirty','edit_noop_dirty'])
    for ph in PHASES:
        key='restored/'+ph+'.json'
        if key not in members:continue
        d=get(key)
        for arm in u.ARMS:
            s=d['arms'][arm]['setup']
            w.writerow([ph,d['seed'],0,'setup',arm,s['warm']['cargo']['wall_s'],s['settled']['cargo']['wall_s'],'','',len(s['warm']['cargo']['dirty_units']),0,'',''])
        for i,order in enumerate(d['order']):
            for arm in order:
                t=d['arms'][arm]['trials'][i]
                w.writerow([ph,d['seed'],i+1,' '.join(order),arm,'',t['settled-A']['cargo']['wall_s'],t['edit-D']['cargo']['wall_s'],t['noop-D']['cargo']['wall_s'],'',0,len(t['edit-D']['cargo']['dirty_units']),0])
    old=d942.SEEDS;d942.SEEDS=FRESH_SEEDS
    try:_,rows=d942.records({n:members['physical/records/'+n]for n in d942.ALL_RECORDS})
    finally:d942.SEEDS=old
    return {'restored-times.csv':out.getvalue().encode(),'physical-times.csv':d942.csv_bytes(rows)}
def archive(path):
    with ZipFile(path)as z:
        infos=z.infolist();need(len(infos)==len(set(z.namelist()))<=620 and sum(f.file_size for f in infos)<30<<20 and all(f.file_size<2<<20 and '..' not in f.filename.split('/') and not f.filename.startswith('/')for f in infos),'bounded evidence members')
        members={n:z.read(n)for n in z.namelist()}
    manifest=json.loads(members.pop('MANIFEST.json'))
    need({k:sha(x)for k,x in members.items()}==manifest,'all archive member digests')
    return members
def compute(members):
    doc=lambda n:json.loads(members[n])
    q=doc('qualification.json');qualify(q)
    need(sha(members['qualification.json'])==POLICY['qualified_subjects'],'eight protected stage books and actual enrolled high SDKs')
    native_inputs,native_enrolled=archive_custody(members,q)
    src=doc('source.json')
    need(src['A']==src['parent_of_B']==d942.A and src['B']==d942.B and src['patch_sha256']==sha(members['uv-20988.patch'])==u.PATCH and src['tree_A']=='475eccff53635001b11fa5e2adebc9b27f5152e4' and src['tree_B']=='ec9a3f83e89381b5c509ca0c45fb4a87ec71289f' and src['lock_A']==src['lock_B']=='74588b85635b44a3c37821d423f179c7a24fdcb3','authentic complete uncommitted #20988 edit')
    score=doc('restored/score.json');phases=PHASES if score['opens_confirmation']else PHASES[:1]
    restored=restored_check({p:doc('restored/'+p+'.json')for p in phases},score,q,registration=POLICY['restored_registration'],seeds=RESTORED_SEEDS,alpha=.05/9,header=40683,matched='matched_high_T3_over_T4',configs=POLICY['configs'])
    rec={n:members['physical/records/'+n] for n in d942.ALL_RECORDS}
    usage={k:doc('physical/usage/'+k+'.json')for k in USAGE};cl=doc('physical/closure.json');proof=members['physical/verified.json'];passed=members['physical/program.log']
    need(sha(proof)==cl['verified_sha256'] and sha(passed)==cl['log_sha256'] and re.findall(rb'^Completed triple (\d+)/12$',passed,re.M)==[str(i).encode()for i in range(1,13)],'raw durable terminal and all twelve distinct progress events')
    log=members['physical/supervisor-events.log'].decode().splitlines()
    actions=['begin run','run returned 0','begin replay','replay returned 0','begin repeatability','repeatability returned 0']
    need(len(log)==6 and [s.partition(' ')[2]for s in log]==actions and all(log[i].split()[0]<=log[i+1].split()[0]for i in range(5)), 'archive wrapper and both registered physical replays closed')
    report=doc('physical/independent-replay.json')
    need(report['archive']==POLICY['archive'] and report['interrupted_002_accepted']==report['earlier_measured_triples_used']==0 and report['usage_records']==144 and report['seeds']==FRESH_SEEDS,'genuine 003 replay on protected records')
    need(cl['supervisor']==POLICY['supervisor'] and cl['host_dispatch']==POLICY['host_dispatch'] and cl['original_host_stop']==POLICY['prior_host_log'],'operational recovery bound to the same scientific attempt')
    need(cl['inputs']==native_inputs and cl['enrolled']==native_enrolled,'portable physical closure matches original mounted view and enrollment')
    cl['verified']=json.loads(proof)
    need(cl['usage_sha256']=={k:sha(members['physical/usage/'+k+'.json'])for k in USAGE},'unaltered counters')
    need(set(members)==CUSTODY_MEMBERS|{'physical/supervisor-events.log','physical/independent-replay.json','qualification.json','source.json','uv-20988.patch','restored/score.json','physical/closure.json','physical/verified.json','physical/program.log'}|{'restored/'+p+'.json' for p in phases}|{'physical/records/'+n for n in rec}|{'physical/usage/'+k+'.json'for k in usage},'exact distinct studies and raw members')
    fresh=fresh_check(rec,usage,cl,q);f=fresh['comparisons']
    need(report['independent']==fresh, 'separate native-side and portable clock/score reconstructions agree')
    return dict(restored=restored,physical=fresh,repeatability={'both_direct':bool(restored['direct']) and f['signed18_over_T4']['passed'],'both_incremental':bool(restored['incremental']) and f['optimized_T3_over_T4']['passed'],'pooled_rows':0})
def negatives(m):
    def changed(name,transform):
        b=m.copy();v=json.loads(b[name]);transform(v);b[name]=json.dumps(v).encode();return b
    bad={'old format 12 source':changed('qualification.json',lambda q:q['subjects']['parent'].update(header_format=12)),
         'prior driver':changed('qualification.json',lambda q:q['subjects']['parent'].update(driver_sha256=POLICY['blocked_drivers'][0])),
         'copied training profile':changed('qualification.json',lambda q:q['subjects']['parent'].update(profile_sha256=q['subjects']['candidate']['profile_sha256'])),
         'older restored registration':changed('restored/score.json',lambda q:q.update(registration='0'*64)),
         'discovery clock substituted for independent physical edit':changed('physical/records/01-stock/edit.json',lambda q:q.update(elapsed_ns=1)),
         'missing last physical arm':{k:v for k,v in m.items() if k!='physical/records/12-parent/edit.json'},
         'counter overwritten':changed('physical/usage/02-candidate-edit.json',lambda q:q.update(user_us=q['user_us']+1))}
    bad['bundled Cargo marked executed']=changed('qualification.json',lambda q:q.update(cargo=POLICY['bundled_cargo']))
    bad['fixed Cargo marked SDK-bundled']=changed('qualification.json',lambda q:q['subjects']['parent'].update(cargo_sha256=u.CARGO))
    bad['physical SDK Cargo overwritten']=changed('physical/closure.json',lambda q:q['inputs']['cargo_custody']['sdk_bin_cargo'].update(candidate=u.CARGO))
    bad['wrong durable profile receipt']=changed('custody/archived-issuance.json',lambda q:q.update(archive_and_profiles_rehashed=False))
    bad['wrong tested archive']=changed('custody/archive-manifest.json',lambda q:q.update(sha256='0'*64))
    bad['partial predecessor seeds']=changed('physical/records/plan.json',lambda q:q.update(seeds=[406831121,406831127]))
    if 'restored/confirm-2.json' in m:
        bad['missing second restoration']={k:v for k,v in m.items()if k!='restored/confirm-2.json'}
    for name,altered in bad.items():
        try:compute(altered)
        except (ValueError,AssertionError,KeyError):continue
        raise ValueError('negative control accepted '+name)
    return list(bad)
if __name__=='__main__':
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--negative',action='store_true');a=ap.parse_args();m=archive(HERE/'records.zip');result=compute(m)
    need(all((HERE/n).read_bytes()==b for n,b in tables(m).items()),'portable clock tables agree with raw logs')
    if a.negative:result['rejected_controls']=negatives(m)
    print(json.dumps(result,indent=2,sort_keys=True))
