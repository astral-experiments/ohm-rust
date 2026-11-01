"""Read the #22833 native workspace census or the independently scored #20988 all-targets successor."""
import collections,gzip,hashlib,json,sys
from pathlib import Path
D=Path(__file__).resolve().parent;sys.path.insert(0,str(D));import common as c
REG={'workspace':'7e6fe0abec955f171af023b6feeb5175d1a8880aa7490d1d0e6cfee838f47d68','alltargets':'ef40536e3489af2f80904b13f43df0322377c1c855262e62c37e0a8821d736b1'}
ORIG={role:c.REG[role]for role in REG};c.REG.update(REG)
VISITS=('cold-A','settled-A','first-B','settled-B');CMD=['/tmp/uv-workflow-search/sdk/bin/cargo','test','--workspace','--no-run','--locked','--offline','-j8','--message-format=json-render-diagnostics']
need=c.need
def structural(m):
    j=lambda n:json.loads(m[n]);z=lambda n:json.loads(gzip.decompress(m[n]))
    t=j('terminal.json');f=t['files'];need(t['registration']==REG['workspace'] and t['scored_pairs']==0,'original zero-score terminal')
    reg=j('registration.json');need(reg['predecessor_registration']==ORIG['workspace'] and reg['predecessor_terminal']==c.H(m['previous-terminal.json']) and reg['command']==CMD[1:]and reg['target_cap']==20<<30,'original failed study, full fixed command and capacity')
    prev=j('previous-terminal.json');need(prev['exit']!=0 and prev['status']=='failed'and prev['scored_pairs']==0,'previous scope separately unscored')
    need(c.H(m['source.json'])==j('inputs.json')['source']and j('source.json')['parent_first_parent']and j('source.json')['git_patch_sha256']==c.PATCH[c.G],'source custody')
    for n,b in m.items():
        if n in f:need(c.H(b)==f[n],'original record '+n)
    need(j('loader.json')['librustc_driver']['sha256']==c.DRIVERS['candidate']and j('loader.json')['libLLVM']['sha256']==c.LLVM,'real loaded compiler')
    obs={};vis={};ops={};same=[];counters={};role='workspace'
    expected=j('roots.json');need(len(expected)==len(set(tuple(row)for row in expected))==92,'complete manifest census')
    for stage in VISITS:
        key=stage+'/';name=key+'cargo-return.json'
        if name not in f:continue
        q=j(name);v={'exit':q['exit'],'target_allocated':q['target_allocated'],'sampled_peak':q['sampled_peak']}
        need(j(key+'command.json')==CMD and 0<=v['target_allocated']<=v['sampled_peak']<=20<<30,'ordinary command and bounded target')
        obs[stage]=v
        if key+'products.json.gz'not in f:continue
        p=z(key+'products.json.gz');acts=z(key+'workspace-rustc.json.gz');o=j(key+'ownership-public.json');cargo=j(key+'cargo-products.json')
        need(o['source_sha256']==f[key+'cargo-ownership.json'] and o['root_argv']==CMD and o['root_exit']==q['exit']==0 and o['parent_edges']+1==o['traced_tasks'],'genuine supervised Cargo root')
        need(cargo['stdout_sha256']==q['streams']['stdout']['sha256'] and cargo['build_finished'] and sum(x[1]for x in cargo['workspace_and_external_counts'])==p['cargo_fresh'] and sum(x[2]for x in cargo['workspace_and_external_counts'])==p['cargo_dirty'],'Cargo artifact frequency and completion')
        count=collections.defaultdict(lambda:[0,0])
        for package,target,kind,test,fresh in cargo['units']:count[package][int(not fresh)]+=1
        need(sorted([pkg,*ns]for pkg,ns in count.items())==cargo['workspace_and_external_counts'],'all per-package Cargo frequencies from actual unit records')
        need(p['roots']==expected and not any(p['roster_difference'].values()),'92 distinct manifest harnesses')
        real={a['pid']:a for a in acts if not a['probe']};need(sorted(real)==sorted(p['workspace_rustc']),'genuine workspace rustc processes')
        v['nonzero_workspace_rustc']=[dict(pid=pid,package=a['package'],crate=a['crate'],exit=a['exit'])for pid,a in real.items()if a['exit']!=0]
        for pid,a in real.items():need(type(a['exit'])is int and a['stage']==stage and a['package'] and a['argv'][0]=='/tmp/uv-workflow-search/sdk/bin/rustc','actual compiler and argv')
        link={pid for pid,a in real.items()if any(Path(x).name in ('ld','ld.bfd','ld.lld','wild','collect2','lld','rust-lld','mold','ld.gold','ld.wild')for _,x in a['successful_execs'])}
        need(link==set(map(int,p['link_execs'])),'actual linked processes')
        v.update(harnesses=len(p['roots']),workspace_paths=len(p['files']),cargo_fresh=p['cargo_fresh'],cargo_dirty=p['cargo_dirty'],cargo_unit_frequency=cargo['workspace_and_external_counts'],workspace_rustc=len(real),rustc_with_linker=len(link),outside_workspace_nonprobe=p['outside_workspace_nonprobe'],nested_cargo_commands=o['nested_commands'])
        if stage.startswith('settled'):
            pre=vis['cold-A'if stage.endswith('A')else'first-B'];v['identical_files_and_stamps']=(pre['files']==p['files'])
            if t['exit']==0:need(not real and not p['cargo_dirty'] and not p['outside_workspace_nonprobe']and v['identical_files_and_stamps'],'clean, settled repeat')
        ops[stage]=real;vis[stage]=p
    need(set(obs)<=set(VISITS),'valid visits')
    result=dict(case=role,status=t['status'],exit=t['exit'],scored_pairs=0,visits=obs)
    if 'first-B'in vis:
        a,b=vis['cold-A'],vis['first-B'];cnt=collections.Counter();kinds=collections.defaultdict(collections.Counter);roster={};allstates=collections.Counter()
        for n in a['files'].keys()|b['files'].keys():
            x,y=a['files'].get(n),b['files'].get(n);state='new'if x is None else'removed'if y is None else'same-bytes'if x[-1]==y[-1]else'different-bytes';allstates[state]+=1
            if y is None:continue
            u=b['units'][n];is_harness=u['test']and u['executable'];owned={o[1] for o in u['owner']};linked=any(o[-1]for o in u['owner']);need(owned<=ops['first-B'].keys(),'PID stamped same output')
            kind='harness'if is_harness else Path(n).suffix or 'other';kinds[kind][':'.join([state,'fresh'if u['fresh']else'dirty','linked'if linked else'compiled'if owned else'no-matched-rustc'])]+=1
            if not is_harness:continue
            roster[n]=dict(package=u['package'],source=u['source'],state=state,fresh=u['fresh'],linked=linked)
            if linked and state=='same-bytes':
                argv=ops['first-B'][next(iter(owned))]['argv'];deps={z.split('=',1)[1].removeprefix('/tmp/uv-workflow-search/target/')for fl,z in zip(argv,argv[1:])if fl=='--extern'and '='in z}
                seen=sorted(deps&a['files'].keys()&b['files'].keys());changed=[z for z in seen if a['files'][z][-1]!=b['files'][z][-1]]
                same.append(dict(package=u['package'],source=u['source'],output=n,matched_workspace_externs=len(seen),changed_direct_workspace_externs=changed,output_sha256=y[-1]))
        need(len(roster)==len(b['roots']),'all executable harness paths')
        result.update(workspace_products=allstates,product_kinds=dict(kinds),harness_outcomes=sorted(roster.items()),relinked_equal=sorted(same,key=lambda v:v['source']))
    if t['exit']==0:
        accepted=j('worker-accepted.json');need(t['status']=='complete' and t['accepted']==c.H(m['worker-accepted.json'])and accepted['scored_pairs']==0 and accepted['input_admission']==j('inputs.json')['admission'] and accepted['input_qa']==j('inputs.json')['qa']and len(obs)==4 and accepted['root_count']==92 and accepted['product_states']==result['workspace_products'],'complete structural result')
        for stage in ('A','B'):
            pre='semantics-'+stage+'/';s=j(pre+'accepted.json');meta=j(pre+'metadata/accepted.json');text=m[pre+'client.stdout'].decode()
            need(s['layout']==c.H(m[pre+'metadata/accepted.json'])and s['state']==stage and s['client_opposite']==101 and len(set(s['tests']))==5 and 'detached=true'in text and ('shared='+str(stage=='B').lower())in text,'actual client and linked metadata')
            for nm in s['tests']:need(b'1 passed; 0 failed'in m[pre+nm.replace('::','_')+'.stdout'],'linked workspace semantic tests')
        result['target_final_bytes']=accepted['final_target_allocated']
        result['object_overlap']=j('object-overlap.json')
    else:need(t['status']=='failed'and t['accepted']is None,'failed structural scope stays unscored')
    return result
def verify(m):
    k=json.loads(m['case.json'])['case'];need(k in REG and c.H(m['registration.json'])==REG[k],'specific successor source')
    if k=='workspace':return structural(m)
    note=json.loads(m['prior-attempts.json']);r=json.loads(m['registration.json'])
    need(r['source_predecessor']==ORIG[k]and r['source_failed_qa']==c.H(m['previous-qa-return.json'])and json.loads(m['previous-qa-return.json'])['exit']!=0 and note['old_import_last_exception']=="AttributeError: module 'native' has no attribute 'foreign'"and note['previous_rows_used']==0,'predecessor failures')
    observed=json.loads(m['qa/accepted-files.json']);qa=json.loads(m['qa/closed.json']);need(observed['closure']==c.H(m['qa/closed.json'])and observed['admitted']==qa['admission'],'real native issuer')
    import re
    for role in c.ARMS:
        pre='qa/'+role+'/';a=json.loads(m[pre+'accepted.json']);need(all(observed['issuers'][role][n]==c.H(m[pre+n])for n in ('accepted.json','return.json','loader.json','all-B-owners.json')),'mode-specific native issuer')
        for stage in ('A','B','fresh-B'):
            for op in ('native-list','native-tests','changed-workspace-test'):
                n=stage+'/'+op;raw=m[pre+n+'.stdout'];desc=json.loads(m[pre+n+'.json'])
                need(desc['exit']==0 and desc['stdout']==c.H(raw)==a['raw_files'][n+'.stdout'] and c.H(m[pre+n+'.json'])==a['raw_files'][n+'.json'],'per-command native output/exit')
                if op=='native-list':need(desc['args'][1:]==['--list']and c.H(raw)=='ec73b0be264a4f677a47495384b47f0de80d9a28308e474424db446a4554920f','actual 99-test original bytes')
                else:
                    expected=a['states'][stage]['run']['tests']if op=='native-tests'else ([]if stage=='A'else['workspace::tests::exclude_package_with_normalized_glob_and_escaped_root'])
                    actual=sorted(re.findall(r'^test (.+) \.\.\. ok$',raw.decode(),re.M));need(actual==expected and f'{len(expected)} passed; 0 failed'.encode()in raw,'unaltered actual passing native test identities')
    v=c.verify(m)
    if v['heldout_triples']:
        from statistics import median
        rows=[json.loads(l) for blk in ('hold-1','hold-2')for l in m[blk+'/pairs.jsonl'].splitlines()]
        values=[{k:u['edit']for k,u in row.items()}for row in rows]
        need(values==json.loads(m['score.json'])['seconds'],'all raw held-out seconds')
        v['pooled_within_heldout_only_median_seconds']={k:median(row[k]for row in values)for k in c.ARMS}
    owners=[[i['package'],i['target'],i['test'],i['features']]for i in json.loads(m['qa/stock/all-B-owners.json'])['units']]
    for k in c.ARMS:need(sorted(owners)==sorted([i['package'],i['target'],i['test'],i['features']]for i in json.loads(m['qa/'+k+'/all-B-owners.json'])['units']),'identical underlying first-B compiler roles')
    v['dirty_compiler_roles']=owners
    return v
def main():
    data=c.unpack(D/'records.zip');result=verify(data)
    if '--negative'in sys.argv:
        keys=(['first-B/products.json.gz','settled-A/cargo-return.json','worker-accepted.json','loader.json','cold-A/ownership-public.json']if result['case']=='workspace'and result['status']=='complete'else ['terminal.json']if result['case']=='workspace'else['previous-qa-return.json','discovery/pairs.jsonl','discovery/parent/pair-6.zip','score.json'])+['registration.json']
        for name in keys:
            bad=data.copy();del bad[name]
            try:verify(bad)
            except (KeyError,ValueError,AssertionError):continue
            raise ValueError('accepted missing '+name)
    print(json.dumps(result,sort_keys=True,indent=2))
if __name__=='__main__':main()
