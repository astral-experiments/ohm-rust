"""Replay the separately observed pre-fix behavior and four repaired cache matrices, with stock Python."""
import argparse
import base64
import hashlib
import itertools
import json
from pathlib import Path
import re
import sys
import zipfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
import cache_read as old
need=old.check
FORMATS={'base8':8,'old9':9,'old10':10,'retired11':11,'retired12':12,'CTFE':11,'T1':35256,'T3':40683,'T4':40683}
ROLES={'legacy':('base8','old9','old10','retired11','retired12','T1','T3','T4'),
       'ctfe':('CTFE','T1','T3','T4'),'previous':('retired11','CTFE')}
HIGHS={'T1','T3','T4'}
CASES={'no_edit':('A','ordinary'),'positive':('B','ordinary'),'negative':('E','ordinary'),'assert_loaded':('B','loaded'),'assert_not_loaded':('B','not-loaded')}
REG='e0b31bee28c8a3be1e31f51cc22193bbbe18108c7d30348e56ce1734aca4f965'
DISPATCH='3a1bf211812111332500c6d3b163fe678271472172f72c20f1140924125bf8bd'
MREG='5e5d355a407dde3f97b9247344f1c7f9c1a5950d324f3a7049f7b577f4f6df68'
def encode(x):return (json.dumps(x,sort_keys=True,indent=2)+'\n').encode()
def hashes(src):
    need(old.sha(src)==old.FIXTURE,'previous actual native fixture')
    return {'A':old.sha(src),'B':old.sha(src.replace(b'        11\n',b'        17\n')),'E':old.sha(src.replace(b'impl Score for Left {',b'impl Score for Right {'))}

def valid(row,want):
    return row.get('worker_exit')==0 and row.get('compile',{}).get('exit')==0 and row.get('run',{}).get('exit')==0 and row['run']['stdout']==want+'\n'
def raw_bytes(obs):
    if 'stdout_raw' not in obs:return
    for n in ('stdout','stderr'):
        raw=base64.b64decode(obs[n+'_retained_base64'],validate=True) if n+'_retained_base64'in obs else obs[n].encode()
        p=obs[n+'_raw'];need(len(raw)<=p['bytes'] and len(raw)<=524288 and p['all_retained']==(len(raw)==p['bytes']),'native stream retention')
        if p['all_retained']:need(old.sha(raw)==p['sha256'],'native stream SHA-256')
        need(raw.decode(errors='replace')==obs[n],'decoded native stream')
def classify(r):
    if r.get('worker_exit')!=0:return 'worker error'
    c=r['compile']
    if c['terminated_by_guard'] or any(not c[n+'_raw']['all_retained']for n in ('stdout','stderr')):return 'guarded/partial'
    if c['exit']<0:return 'compiler signal '+str(-c['exit'])
    if any(s in c['stderr']for s in ('internal compiler error','panicked at','Invalid DepKind')):return 'compiler error with ICE/panic marker'
    if c['exit']!=0:return 'compiler rejected input'
    if r.get('run',{}).get('exit')!=0:return 'compiled, native execution failed'
    return 'compiled and ran'
def state(rows):
    def diagnostic(k,message):
        c=rows[k]
        if c.get('worker_exit')!=0 or c['compile']['exit']!=1:return False
        for line in c['compile']['stderr'].splitlines():
            try:d=json.loads(line)
            except ValueError:continue
            if isinstance(d,dict) and d.get('$message_type')=='diagnostic' and d.get('level')=='error' and d.get('message')==message:return True
        return False
    a=diagnostic('assert_not_loaded','we asserted that the incremental cache should not be loaded, but it was loaded')
    b=diagnostic('assert_loaded','we asserted that an existing incremental cache directory should be successfully loaded, but it was not')
    if a and b:return 'contradictory independent assertions'
    if a:return 'rustc explicitly reported loaded'
    if b:return 'rustc explicitly reported not-loaded'
    return 'no decisive cache-state report'
def matches(r,ref):
    if r.get('worker_exit')!=0 or classify(r)=='guarded/partial':return False
    return (r['compile']['exit'],r.get('run',{}).get('exit'),r.get('run',{}).get('stdout'),r['diagnostics'])==(ref['compile']['exit'],ref.get('run',{}).get('exit'),ref.get('run',{}).get('stdout'),ref['diagnostics'])
def behavior(row,want):
    need(valid(row,want),'actual native '+want)
    need(row['run']['argv']==['/tmp/w/probe'] and row['run']['stderr']=='','native binary invocation')
def wire(row,edit,flag,r,s,fresh,force,source,info=False):
    if row.get('worker_exit')!=0:return
    for n in ('compile','run','rustc'):
        if n in row:raw_bytes(row[n])
    parsed=[]
    for line in row['compile']['stderr'].splitlines():
        try:item=json.loads(line)
        except ValueError:continue
        if isinstance(item,dict) and item.get('level')=='error' and (item.get('spans')or item.get('code')):
            parsed.append({k:item.get(k)for k in ('code','message','level','spans','children')})
    need(row['diagnostics']==parsed,'raw compiler diagnostic messages, spans and children')
    if info:
        need(row['compile']['argv'][-1]=='-Zincremental-info','separate incremental-info attempt')
        row=dict(row,compile=dict(row['compile'],argv=row['compile']['argv'][:-1]))
    old.validate_row(row,edit,flag,r,s,fresh,force,source)
def source(data,get):
    hist=get('source/history.json');nums=sorted(set(hist['blob_to_version'].values()))
    need(len(hist['tips'])==len(set(hist['tips']))==hist['distinct_tips']==550 and len(hist['commit_to_blob'])==54 and len(hist['blob_to_version'])==42,'bounded timestamped source census')
    for oid,v in hist['blob_to_version'].items():
        raw=data['headers/'+oid+'.rs'];need(old.gitblob(raw)==oid,'actual source header Git identity')
        number=re.findall(rb'(?m)^\s*const\s+HEADER_FORMAT_VERSION\s*:\s*u(?:8|16|32)\s*=\s*([0-9a-fA-Fx_]+)\s*;',raw)
        need(len(number)==1 and int(number[0].replace(b'_',b''),0)==v,'actual historical header literal')
    need(nums==hist['versions'] and set(nums).isdisjoint((35256,40683)),'numeric IDs absent in observed reachable header sources')
    meta=get('source/source.json');q=get('source/query-ids.json');rr=get('source/rosters.json')['private-20261101']
    for k,oid in q.items():
        raw=data['queries/'+oid+'.rs'];need(old.gitblob(raw)==oid,'query blob '+k)
        names=[v.decode() for pat in (rb'^    non_query (\w+)',rb'^    query (\w+)\(') for v in re.findall(pat,raw,re.M)]
        need(names==rr[k],'native numeric query kinds '+k)
    need(set(rr)==set(q)==set(FORMATS),'all participating rosters')
    for k in HIGHS:
        row=meta['rows'][k];raw=data['headers/'+row['new_header_blob']+'.rs']
        need(old.gitblob(raw)==row['new_header_blob'] and row['new_version']==FORMATS[k] and row['query_blob']==q[k],'actual new query and header blobs')
        num=re.search(rb'const HEADER_FORMAT_VERSION: u16 = ([0-9a-fxA-F_]+);',raw)
        need(num and int(num[1].replace(b'_',b''),0)==FORMATS[k],'real new header')
        need(b'if header_format_version != HEADER_FORMAT_VERSION' in raw and b'if buffer != rustc_version(sess).as_bytes()' in raw,'real reader rejects version or format mismatch')
    need(rr['T3']==rr['T4'] and len(rr['T1'])==363 and len(rr['T4'])==364 and len(rr['CTFE'])==360,'actual roster lengths')
    for k in HIGHS:
        r=meta['rows'][k];digest=old.sha(('\n'.join(rr[k])+'\n').encode())
        need(digest==r['roster_sha256'] and (0x8000|(int(digest[:4],16)&0x7fff))==r['new_version'],'numeric format from checked query roster')
        files=sorted(re.findall(rb'^diff --git a/([^ ]+) b/',data['source/T4-parent-to-final.patch']if k=='T4'else data['source/'+k+'-restack.patch'],re.M))
        need((bool(files) and meta['header'].encode()in files) if k in ('T1','T3') else len(files)==6,'full stacked source delta')
    live=get('source/live-header-census.json')
    need((live['live_named_branches'],live['live_open_prs'],live['distinct_active_head_commits'])==(373,182,372) and set(live['active_header_objects'].values()).isdisjoint({35256,40683}),'timestamped living remote head census')
    latest=get('source/20261101T070151Z-lldb.json');oid='9a0ba14e7641b0402d6eb79c26510016b42de241'
    need({a['head']for a in latest}=={'f808a60d5f9c58846d15c3126a19b731b2f7dde4','4f605903c34df9e218e0f88e83da7906f31a35a7'} and all(a['header_blob']==oid and a['format']==hist['blob_to_version'][oid]==10 and set(a['files'])=={'src/etc/rust-lldb','tests/run-make/rust-lldb-toolchain/rmake.rs'} for a in latest),'two separate later debugger sources')
    left,right=rr['retired11'],rr['CTFE']
    shifted=sum(left.index(n)!=right.index(n) for n in set(left)&set(right))
    need(shifted==172 and left[188]=='blanket_trait_impls' and right[188]=='trait_impls_by_const_arg','concrete collision at old format 11')
    return rr,{'historical_header_literals':nums,'resolvable_tips_with_history':hist['reachable_tips_with_history'],'distinct_tips':hist['distinct_tips'],'missing_tips':len(hist['unresolvable_tips']),'same_named_queries_reindexed':shifted}
def enroll(get,scope,mode,arms,rr,fhash,complete):
    pre=scope+'/'+mode+'/';doc=get(pre+'receipt.json');seeds={};clean={};results={};clashes={};cache_dir=set();force='rustc-perf' if mode=='forced' else None
    previous=scope=='previous';roles=ROLES[scope]
    for tag in roles:
        s=seeds[tag]=get(pre+tag+'-seed.json');em=clean[tag]=get(pre+tag+'-empty.json');arm=arms[tag]
        wire(s,'A','ordinary',s,{},True,force,fhash);behavior(s,'11')
        need((s['driver_sha256'],s['launcher_sha256'],s['llvm_sha256'])==tuple(arm[n]for n in ('driver_sha256','launcher_sha256','llvm_sha256')),'actual seed SDK bytes')
        need(s['driver_path']=='/tmp/sdk/'+arm['driver_relative'] and s['llvm_path']=='/tmp/sdk/'+old.LLVM,'loaded SDK paths')
        if tag in HIGHS:size=get('sdk/'+tag+'.json')[arm['driver_relative']][1]
        elif tag=='CTFE':size=get('sdk/CTFE.json')['files'][arm['driver_relative']][1]
        else:size=get('source/earlier-subjects.json')['subjects'][tag]['driver_bytes']
        need(s['selected_driver_identity'][2]==size and s['selected_llvm_identity'][2]==208144480,'physical DSO sizes')
        hdr=s['seed_headers'];cache_dir.update(Path(n).parts[0]for n in hdr)
        need(set(Path(n).name for n in hdr)=={'query-cache.bin','dep-graph.bin','work-products.bin'} and all((x['version'],x['format'])==(old.VERSIONS[mode],FORMATS[tag])and old.D64.fullmatch(x['sha256'])for x in hdr.values()),'full actual emitted file headers')
        need(s['rustc']['exit']==0 and 'host: '+old.TARGET in s['rustc']['stdout'].splitlines(),'native compiler platform')
        cnt=s['graph_census'];h=cnt['by_number'];need(len(h)==len(rr[tag]) and sum(h)==cnt['nodes']>0,'observed node histogram')
        prefix=''if previous else'empty_'
        a=('assert_'if previous else prefix)
        for n,e,g in [('B','B','ordinary'),('E','E','ordinary'),(a+'loaded','B','loaded'),(a+'not_loaded','B','not-loaded')]:
            need(em[n]['worker_exit']==0,'clean worker');wire(em[n],e,g,s,{},True,force,fhash)
        behavior(em['B'],'17');old.diagnostics(em['E']);behavior(em[a+'not_loaded'],'17')
        need(state({'assert_loaded':em[a+'loaded'],'assert_not_loaded':em[a+'not_loaded']})=='rustc explicitly reported not-loaded','fresh cache-state negative and positive')
        if previous:
            peer=next(k for k in roles if k!=tag)
            clashes[tag]={str(i):{'count':v,'writer_kind':rr[tag][i],'other_reader_kind':rr[peer][i]if i<len(rr[peer])else None,**cnt['first'][str(i)]}for i,v in enumerate(h)if v and (i>=len(rr[peer])or rr[tag][i]!=rr[peer][i])}
    need(len(cache_dir)==1,'same real incremental crate subdirectory per scope')
    todo=((a,b)for a,b in itertools.product(roles,repeat=2)if previous or a==b or a in HIGHS or b in HIGHS)
    for a,b in todo:
        k=a+'-to-'+b;rows=get(pre+k+'.json');expected=CASES.copy()
        if previous:expected['info_B']=('B','ordinary')
        need(set(rows)==set(expected),'complete first compile, edited, invalid, complementary cases')
        for n,(e,g)in expected.items():
            if not previous:need(rows[n]['worker_exit']==0,'clean repaired work')
            wire(rows[n],e,g,seeds[b],seeds[a],False,force,fhash,n=='info_B')
        observed=state(rows)
        if previous:
            result={'assertion':observed,'outcomes':{n:{'classification':classify(v),'compiler_exit':v.get('compile',{}).get('exit'),'native_stdout':v.get('run',{}).get('stdout')}for n,v in rows.items()},
                    'unedited_matches_expected':valid(rows['no_edit'],'11'),'edited_matches_fresh':matches(rows['positive'],clean[b]['B']),'invalid_matches_fresh_E0277':matches(rows['negative'],clean[b]['E'])}
            if a==b:need(observed=='rustc explicitly reported loaded' and all(result[n]for n in ('unedited_matches_expected','edited_matches_fresh','invalid_matches_fresh_E0277')),'pre-fix own-history control')
            results[k]=result
        else:
            compatible=a==b or {a,b}=={'T3','T4'}
            behavior(rows['no_edit'],'11');behavior(rows['positive'],'17');need(old.diagnostics(rows['negative'])==old.diagnostics(clean[b]['E']),'fresh E0277 including spans/children')
            want='loaded'if compatible else'not-loaded'
            need(observed=='rustc explicitly reported '+want,'direct complementary cache-state diagnostic '+k);old.assertion(rows,compatible);results[k]=want
    if previous:
        need(doc['observations']==results and doc['actual_emitted_conflicting_kinds']==clashes,'actual pre-fix outcomes and histogram indices')
        official=complete['previous'][mode]
        need(official['directions']=={n:v for n,v in results.items()if n in ('CTFE-to-retired11','retired11-to-CTFE')} and doc['same_cache_directory'] and doc['same_native_header_hex']==old.header(11,old.VERSIONS[mode]).hex() and official['sha256']==doc['original_sha256'],'closed old behavior/hashes')
        observed={}
        for direction in official['directions']:
            raw=get(pre+direction+'.json')['positive']
            stderr=raw.get('compile',{}).get('stderr','')
            observed[direction]=[s for s in ('Invalid DepKind 360','dep_node.rs:70:13','uninterned StableCrateId: StableCrateId(0)','cstore_impl.rs:911:32') if s in stderr]
        return {'directions':official['directions'],'edited_diagnostic_markers':observed,'same_header':doc['same_native_header_hex'],'genuine_misinterpretable_nodes':{k:sum(o['count']for o in vals.values())for k,vals in clashes.items()}}
    loads=sum(v=='loaded'for v in results.values());rejected=len(results)-loads
    o=complete['matrices'][scope][mode]
    need(doc['states']==results and doc['compatible_loads']==o['compatible']==loads and doc['different_format_rejected']==o['rejected']==rejected and doc['registration']==MREG and doc['scope']==scope and doc['mode']==mode,'original states and counts')
    need((o['own'],o['cross_T3_T4'],loads,rejected)==((8,2,10,34)if scope=='legacy'else(4,2,6,10)) and o['sha256']==doc['original_sha256'] and o['same_cache_subdirectory']in cache_dir and doc['native_performance_rows']==0,'actual typed scopes')
    return {'loaded':loads,'rejected':rejected,'reciprocal_T3_T4':2,'actual_transitions':len(results),'own':len(roles)}
def unpack(path):
    with zipfile.ZipFile(path)as z:
        members=z.infolist();need(path.stat().st_size<=6<<20 and len(members)<=550 and sum(m.file_size for m in members)<=64<<20,'bounded original packet')
        need(len(members)==len(set(m.filename for m in members)) and all(re.fullmatch(r'[A-Za-z0-9._/-]+',m.filename) and not m.filename.startswith('/') and '..'not in Path(m.filename).parts for m in members),'safe unique members')
        return {m.filename:z.read(m)for m in members}
def assessment(data):
    get=lambda n:json.loads(data[n]);manifest=get('manifest.json')
    need(set(manifest['members'])==set(data)-{'manifest.json'} and all(old.sha(data[n])==h for n,h in manifest['members'].items()),'all raw member digests')
    for n,digest in manifest['stored_raw'].items():need(old.sha(data[n])==digest,'original raw worker bytes '+n)
    src,coverage=source(data,get);complete=get('closed.json');arms=get('subjects.json');versions=old.VERSIONS
    need(set(arms)==set(FORMATS) and complete['registration_sha256']==DISPATCH and complete['original_closed_registration_sha256']==REG and complete['performance_rows']==0,'full source-issued matrix')
    need(old.sha(data['policy/closed.py'])=='1639ee9334caaaf11489ee1bc09333fab8aa7845bbb2ade6c068d994b2b446f3' and old.sha(data['policy/closed-registration.json'])==REG and old.sha(data['policy/matrix-registration.json'])==MREG,'native accession code')
    dispatched=get('dispatch/registration.json');need(old.sha(data['dispatch/registration.json'])==DISPATCH and dispatched['old_reader']==REG and dispatched['science']==MREG and dispatched['invalid_row_count']==0,'actual corrected barrier')
    for n in ('run.py','previous-worker.py','matrix-worker.py'):
        need(old.sha(data['dispatch/'+n])==dispatched['files'][n],'source for each actual worker')
    smoke=get('dispatch/smoke.json');need(smoke['original_sha256']==complete['dispatch_smoke_sha256'] and smoke['locks']==4 and smoke['performance_rows']==0 and smoke['prior_failed_inputs']==complete['prior_failed_inputs']=='ebe4d5e863c57fb856554508ea4538e29a1eea6b1c92692ed52532139585a438' and smoke['prior_failure']==complete['prior_failure']=='e3952b35216ae59db0a26e29534f06b4e7d2c83b50ecc0303ee6a1240a99c330' and smoke['worker_exit']!=0 and smoke['actual_final_diagnostic']=='ValueError: exact graph decoder','original refused input and real reproducer')
    for k,mode in [('base8','unforced'),('CTFE','forced')]:
        case=smoke['source_cases'][k];wire(case['seed'],'A','ordinary',case['seed'],{},True,'rustc-perf' if mode=='forced' else None,hashes(data['probe.rs']))
        wire(case['loaded'],'B','loaded',case['seed'],case['seed'],False,'rustc-perf' if mode=='forced' else None,hashes(data['probe.rs']))
        behavior(case['seed'],'11');behavior(case['loaded'],'17');need(case['mode']==mode and case['seed_sha256']==case['seed']['seed_gz_sha256'],'fixed dispatch and real reconsumption')
    for scope,mode in itertools.product(ROLES,old.VERSIONS):
        issue=get('dispatch/'+scope+'-'+mode+'.json');rec=get(scope+'/'+mode+'/receipt.json')
        worker='previous-worker.py' if scope=='previous' else 'matrix-worker.py'
        need(issue==dict(registration=DISPATCH,science=rec['registration'],smoke=smoke['original_sha256'],prior_failed_inputs=smoke['prior_failed_inputs'],prior_failure=smoke['prior_failure'],scope=scope,mode=mode,signed_result=rec['original_sha256'],worker_source=old.sha(data['dispatch/'+worker]),performance_rows=0),'genuine fresh dispatch receipts')
    prior=get('source/earlier-subjects.json')
    need(prior['published_mandatory_zip']=='ed26eb4ff05e70577bd6b5516a6a14a94e8b4b69a3fcfaae8991ab238bd55255' and {k:{n:v for n,v in row.items()if n!='driver_bytes'}for k,row in prior['subjects'].items()}=={k:arms[k]for k in ROLES['legacy']if k not in HIGHS},'five previously qualified original SDKs')
    for k in ('T1','T3','T4'):
        curr=arms[k];issued=complete['subjects'][k]
        need(all(issued[n]==v for n,v in curr.items()),'complete source and driver origin')
        q=get('source/source.json')['rows'][k];need((curr['commit'],curr['tree'],curr['header_format'])==(q['new_commit'],q['tree'],q['new_version']),'actual high source')
        inv=get('sdk/'+k+'.json');ev=get('sdk/'+k+'-native.json');need(len(inv)==73 and old.sha(data['sdk/'+k+'.json'])==ev['manifest'] and inv[curr['driver_relative']][0]==curr['driver_sha256'] and inv['bin/rustc'][0]==curr['launcher_sha256'] and inv[old.LLVM][0]==curr['llvm_sha256']==old.LLHASH,'original native images')
        need(ev['original_zip']==curr['raw_packet_sha256'] and ev['native_return']==ev['suite_exit']==0 and ev['registration']==get('policy/matrix-registration.json')['native_registration'] and ev['compiler_source']==q['compiler_tree'] and set(ev['solver_stdout'])=={'default','next'} and 'trait-impl-bucket-projection ... ok'in ev['suite_line'],'actual parent and child suite/solver qualification')
    fm=get('sdk/T4-finite-matrix.json');need(old.sha(data['sdk/T4-finite-matrix.json'])==get('sdk/T4-native.json')['finite_matrix'],'native finite query proof')
    for writer,reader,solver in itertools.product(('T3','T4'),('T3','T4'),('default','next')):
        obs=fm[writer+'-'+reader+'/'+solver];value=int(reader=='T3')
        need(obs['provider_counts']['unrelated']=={'primitive':value,'pair':value,'wide':1,'specializing':0},'actual finite and unsupported controls')
    cf=get('sdk/CTFE.json');need(len(cf['files'])==81 and cf['git']==complete['ctfe']['source']==arms['CTFE']['commit']=='df2c077a113a709ab379c8ec34e62c94c7513325' and arms['CTFE']['driver_sha256']==complete['ctfe']['driver_sha256']=='481e8cf9c46278874b40a129f44a3825d64bc050de5f0b3ff34c0986cb4d177c' and cf['tree']==arms['CTFE']['tree'] and cf['original_receipt']==complete['ctfe']['positive_81']=='5ac1a5f16aed95f762c8546620c2bf61e66bf1dbc34ce1df92a1779e6d5bd754','actual original CTFE compiler')
    need(cf['files'][arms['CTFE']['driver_relative']][:2]==[arms['CTFE']['driver_sha256'],142851744] and cf['files']['bin/rustc'][0]==arms['CTFE']['launcher_sha256'] and len(cf['overlay_members'])==2 and cf['untouched_F_members']==79 and cf['native_job']==124771325405,'81 true CTFE image members')
    need(len({a['driver_sha256']for a in arms.values()})==len(arms) and all(v['header_format']==FORMATS[k]for k,v in arms.items()),'distinct SDKs')
    fixed={};previous={};fhash=hashes(data['probe.rs'])
    source_members={'source/'+n for n in ('history.json','live-header-census.json','20261101T070151Z-lldb.json','query-ids.json','header-ids.json','source.json','serialized-owner.json','rosters.json','T1-restack.patch','T3-restack.patch','T4-restack.patch','T4-parent-to-final.patch','earlier-subjects.json')}
    policy={'policy/'+n for n in ('closed.py','closed-registration.json','matrix-registration.json','previous-registration.json','migration.py','prefix.py')}
    src_objects={'headers/'+k+'.rs' for k in (set(get('source/history.json')['blob_to_version'])|set(get('source/header-ids.json').values()))}|{'queries/'+k+'.rs'for k in get('source/query-ids.json').values()}
    sdk={'sdk/'+n for n in ('T1.json','T3.json','T4.json','CTFE.json','T4-finite-matrix.json','T1-native.json','T3-native.json','T4-native.json')}
    samples=set();scope_markers=set()
    for sc,ro in ROLES.items():
        names={tag+end for tag in ro for end in ('-seed.json','-empty.json')}
        names|={a+'-to-'+b+'.json'for a,b in itertools.product(ro,repeat=2)if sc=='previous' or a==b or a in HIGHS or b in HIGHS}
        for mode in versions:
            prefix=sc+'/'+mode+'/';scope_markers.add(prefix+'receipt.json');samples.update(prefix+n for n in names)
    dispatch={'dispatch/'+n for n in ('smoke.json','registration.json','run.py','matrix-worker.py','previous-worker.py')}|{'dispatch/'+sc+'-'+md+'.json' for sc,md in itertools.product(ROLES,old.VERSIONS)}
    need(set(manifest['stored_raw'])==samples and set(data)==samples|scope_markers|dispatch|source_members|policy|src_objects|sdk|{'probe.rs','manifest.json','closed.json','subjects.json'},'only registered sources and complete raw cases')
    need(set(manifest['intake'])=={sc+'/'+md for sc,md in itertools.product(ROLES,versions)},'six distinct original intakes')
    for key,intake in manifest['intake'].items():
        rec=get(key+'/receipt.json');intake_file='intake.json' if key.startswith('previous/') else 'inputs.json'
        actual={n.split('/')[-1]:h for n,h in manifest['stored_raw'].items() if n.startswith(key+'/')}
        need(intake['real_leases']==4 and intake['registered']==rec['registration'] and intake['raw_issuer']==rec['original_sha256']
             and old.D64.fullmatch(intake['original_intake']) and rec['original_member_digests']=={**actual,intake_file:intake['original_intake']},'all native per-scope hash records and private four-lease intake')
    for sc in ROLES:
        for mode in versions:
            rr=enroll(get,sc,mode,arms,src,fhash,complete)
            if sc=='previous':previous[mode]=rr
            else:fixed[sc+'/'+mode]=rr
    return {'repaired':fixed,'actual_old_v11':previous,'source_census':coverage,'native_timing_rows':0}
def negative(data):
    import copy
    def must(name,out):
        try:assessment(out)
        except (AssertionError,ValueError,KeyError,TypeError):return name
        raise ValueError('bad input accepted '+name)
    def alter(member,edit,tag):
        v=json.loads(data[member]);edit(v);d=data.copy();d[member]=encode(v)
        m=json.loads(data['manifest.json']);m['members'][member]=old.sha(d[member])
        if member in m['stored_raw']:
            digest=old.sha(d[member]);m['stored_raw'][member]=digest
            rec='/'.join(member.split('/')[:-1])+'/receipt.json'
            source=json.loads(d[rec]);source['original_member_digests'][Path(member).name]=digest;d[rec]=encode(source);m['members'][rec]=old.sha(d[rec])
        d['manifest.json']=encode(m)
        return must(tag,d)
    checks=[
      alter('dispatch/legacy-forced.json',lambda r:r.update(smoke='0'*64),'wrong physical repaired dispatcher'),
      alter('legacy/forced/T3-to-T4.json',lambda r:r['assert_not_loaded']['compile'].update(exit=0),'wrong complementary diagnostic'),
      alter('ctfe/unforced/T4-to-CTFE.json',lambda r:r['positive']['run'].update(stdout='11\n'),'stale repaired native answer'),
      alter('previous/forced/receipt.json',lambda r:r['observations']['retired11-to-CTFE'].update(assertion='invented old result'),'invented old observation'),
      alter('previous/unforced/retired11-seed.json',lambda r:r['graph_census']['by_number'].__setitem__(188,r['graph_census']['by_number'][188]+1),'changed genuine conflicting-kind count'),
      alter('ctfe/forced/T3-to-T4.json',lambda r:r['positive'].update(seed_sha256='0'*64),'wrong restored writer'),
      alter('sdk/T4.json',lambda r:r['bin/rustc'].__setitem__(0,'0'*64),'different launcher'),
      alter('source/history.json',lambda r:r['versions'].append(40683),'invented historical header')]
    return checks
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('packet',type=Path,nargs='?',default=Path(__file__).with_name('raw.zip'));parser.add_argument('--negative',action='store_true');args=parser.parse_args()
    d=unpack(args.packet);out=assessment(d)
    if args.negative:out['rejected_controls']=negative(d)
    print(json.dumps(out,sort_keys=True,indent=2))
