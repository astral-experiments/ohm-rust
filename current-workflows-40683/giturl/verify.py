"""Offline verification of one terminal #306 supplemental workflow. Python 3.11+, standard library."""
import collections,hashlib,io,itertools,json,random,re,sys
from pathlib import Path
from statistics import median
from zipfile import ZipFile
D=Path(__file__).resolve().parent
sys.path.insert(0,str(D));import base007
H=lambda b:hashlib.sha256(b).hexdigest()
ARMS=('stock','parent','candidate')
FIX='02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'
DRIVERS=dict(zip(ARMS,['a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de','0d3fb19078786e868260e3b0c437437211aa03e27e7db527da36a29c91198d5f','77946e23b33a3a7a5cadec9737bbed36d09fd2c8fc8ef52d42bc0bc66143bf79']))
LLVM='43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51'
G='giturl';W='workspace';C='alltargets'
A='4954c25a16b246c1b88aef6c3798a76d675c5cd0';B='0bbcf757fdf19d43f52c38a5fc2e32a291c262ee'
SEEDS={G:[22833406831,22833406847,22833406861],C:[20988416101,20988416127,20988416149]}
REG={G:'206a673ee8014ed6a359d274e87965c34e80c9e678dbe2e2cfdfc706263f3c7a',C:'e03c748912234cf92503d49f33f861c692bc5771d2e65a921a15ce9fb349662a',W:'a6c1e21a0e5585b045821abb1a7d097ab240ab52b4beb0d65a7435ca9da0052e'}
PATCH={G:'ce7a9e41be8607950f6a7f8f36f7eec3ff3b150daab75dbbe80597fe3794b6fb',C:'24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb'}
def need(x,msg):
    if not x:raise ValueError(msg)
def parsezip(b):
    with ZipFile(io.BytesIO(b)) as z:
        q=z.infolist();need(len(q)==len(set(z.namelist())) and len(q)<2000 and sum(v.file_size for v in q)<=100<<20 and all(not v.filename.startswith('/') and '..' not in v.filename.split('/') and v.file_size<12<<20 for v in q),'bounded, unique members')
        return {v.filename:z.read(v)for v in q}
def unpack(f):
    m=parsezip(f.read_bytes());idx=json.loads(m.pop('MANIFEST.json'));need({n:H(b)for n,b in m.items()}==idx,'unaltered members');return m
def order(seed):
    a=list(itertools.permutations(sorted(ARMS)));random.Random(seed).shuffle(a);return list(map(list,a))
def transition(a,b,changed):
    need(set(a)==set(b) and {'uv-resolver/lib-uv_resolver','uv-resolver/test-lib-uv_resolver'}<=set(a),'both CI resolver products')
    for k in a:
        need(all(a[k][x]==b[k][x]for x in ('key','relative_rmeta','test','features'))and (b[k]['stamp']>a[k]['stamp']if changed else a[k]==b[k]),'CI product stamp and feature identity')
def cargo(m,ph,cmd):
    q=json.loads(m[ph+'.json']);s=m[ph+'.stderr'];obs=[n.decode().replace('-','_')for n in re.findall(rb'^\s*(?:Checking|Compiling) ([\w-]+) v',s,re.M)]
    need(q['cmd']==cmd and type(q['wall_s'])in (int,float) and q['wall_s']>0 and sorted(q['dirty_units'])==sorted(obs) and b'Finished 'in s,'ordinary Cargo output, product list and clock')
    usage=json.loads(m[ph+'.usage.json']);need(all(type(usage[k])is int and usage[k]>=0 for k in ('sys_us','user_us')),'out-of-clock counters')
    if ph in ('settled','settled-A','noop-D'):need(not obs,'zero dirty units on unedited Cargo repeat')
    return q['wall_s'],sorted(obs)
def timed(m,case,meta,doc):
    phases=['discovery','hold-1','hold-2'];raw_score=doc('score.json');qa=doc('qa/closed.json')
    need(qa['registration']==REG[case]and qa['admission']==meta['admission'] and qa['previous_scored_rows']==0 and H(m['qa/closed.json'])==meta['qa'],'complete previously admitted native QA')
    patch=PATCH[case];need(H(m['edit.patch'])==patch and meta['source']['patch']==patch,'complete uncommitted patch')
    want_source=[A,B]if case==G else['b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7','1979349b68dbaa936b98c0f409c1df0598fa72f1']
    need(meta['source']['pair']==want_source and meta['registry_packages']==(654 if case==G else 658),'literal revisions and frozen registry')
    need(meta['heads']==dict(parent='b3d745e44826c2d1e4b5ca89222ddbab7af5cc35',candidate='9a5c767f83959802f6d6634deec4aba99b820bc1') and meta['headers']==[40683,40683]and meta['profiles']==dict(parent='be54a6441b917696343c0c6afc5cb4ed1dbac7ffcfaa1268d9511c6c20af9679',candidate='f9880853960d9d258d92d5467ab322d9359c976e34aba9aa12382dbff2b07a58'),'two separately trained sources')
    src=doc('source.json')
    if case==G:
        need(src['parent_first_parent'] and src['lock_blob']=='4947b7ec7062c31273db734c991c74e776c2cc9f'and src['git_patch_sha256']==patch and [src['A'],src['B']]==want_source and src['trees']==['29ef8fd72047d97f9b944f7cded8dcb4880a3188','550999cbe246a0e6340c44a26ff53afc52195b80'] and len(src['changed_paths'])==3 and all((' b/'+p+'\n').encode()in m['edit.patch']for p in src['changed_paths']),'literal source and changed paths')
    else:need(src['source'][:2]==want_source and src['patch']==patch and src['approved_packages']==658,'literal checked all-target source')
    views=meta['cargo_views'];native=views['verified_native'];need(views['fixed_argv']==meta['fixed_cargo']==FIX and native['stock'][0]==FIX and native['parent']==native['candidate'] and native['parent'][0]=='9fb29f6abecad05acb6015990615c5fbb435a5907543f0bd33cf88cada4f5826' and native['parent'][2]==77,'issued SDK vs benchmark Cargo')
    need(m['supervisor.log'].rstrip().endswith(m['score.json'].rstrip()),'actual completed discovery and native replay agree')
    cmd=['/tmp/uv-workflow-search/sdk/bin/cargo','check','-p','uv-resolver']+(['--all-targets','--all-features']if case==C else[])+['--locked','--offline','-j8']
    for k in ARMS:
        p='qa/'+k+'/';a=doc(p+'accepted.json');ret=doc(p+'return.json');loader=doc(p+'loader.json')
        need(ret['exit']==0 and ret['accepted']==H(m[p+'accepted.json'])and ret['log']==H(m[p+'worker.log']) and a['loader']==H(m[p+'loader.json']) and loader['librustc_driver']['sha256']==DRIVERS[k] and loader['libLLVM']['sha256']==LLVM,'native QA reached correct actual loader')
        need(a['scored_rows']==0 and a['role']==k and a['registration']==REG[case] and a['admission']==meta['admission'] and set(a['states'])=={'A','B','fresh-B'},'three authentic product states per role')
        if case==G:
            for phase,v in a['states'].items():
                is_b=phase!='A';sm=v['smoke'];obs=v['meta']['observed'];prefix=p+phase+'/'
                for n in ['client.stdout','snapshot-parse.stdout','snapshot-line_endings.stdout','test-list.stdout']:
                    need(H(m[prefix+n])==a['raw_files'][phase+'/'+n],'bound raw runtime/fixture witness')
                need(obs=={name:{'accepted':flag,'wrong_layout_e0308':0 if flag else 2}for name,flag in [('pointer_layout',is_b),('inline_layout',not is_b)]} and sm['wrong_layout_client_exit']==101,'positive/negative real layout and panic checks')
                behavior=m[prefix+'client.stdout'].decode();need(behavior==sm['behavior'] and 'detached=true' in behavior and ('shared='+str(is_b).lower())in behavior and behavior.startswith('git=8 wrapper=24 pointer=8 'if is_b else'git=280 wrapper=296 pointer=8 '),'native value layout, clone and detach')
                for name,fam in zip(sm['two_original_snapshot_tests'],('parse','line_endings'),strict=True):
                    need(name.startswith('test::'+fam+'::') and 'whitespace'in name and (name+': test') in m[prefix+'test-list.stdout'].decode() and b'1 passed; 0 failed'in m[prefix+'snapshot-'+fam+'.stdout'],'actual named historical smoke')
            need(a['states']['B']['dirty']==qa['rosters'][k] and len(set(qa['rosters'][k]))==19,'nineteen historical dirty packages')
        else:
            roster=doc(p+'all-B-owners.json');need(a['entire_graph']==H(m[p+'all-B-owners.json']) and roster['progress']==qa['rosters'][k],'full native CI owner graph')
            for phase,s in a['states'].items():
                need(s['run']['focused']==(phase!='A') and len(set(s['run']['tests']))==99 and s['run']['tests']==a['states']['A']['run']['tests']and s['client']['check_only_public_bounds']==['clap::ValueEnum','schemars::JsonSchema'] and len(a['mode_specific_error'])==1,'actual A/B focused test and listing')
    blocks=[];timings={};summary={};cold={};eligible=None
    for j,ph in enumerate(phases):
        prefix=ph+'/';present=prefix+'pairs.jsonl' in m
        if j and not eligible:
            need(not present and not any(n.startswith(prefix)for n in m),'ineligible held-out rows');continue
        need(present,'complete enrolled phase')
        plan=doc(prefix+'plan.json');closed=doc(prefix+'closed.json');seed=SEEDS[case][j];orders=order(seed);rows=[json.loads(x)for x in m[prefix+'pairs.jsonl'].splitlines()]
        need(plan['seed']==seed and plan['sequence']==orders and plan['source']==want_source and plan['plain'] and not plan['diagnostics'] and [list(row)for row in rows]==orders and closed==dict(registration=REG[case],admission=meta['admission'],qa=meta['qa'],exit=0),'genuine six complete counterbalanced triples')
        cold[ph]={}
        for k in ARMS:
            pre=prefix+k+'/';ident=doc(pre+'identity.json');prod=doc(pre+'actual-products.json')
            need(ident['cargo']==FIX and [v['sha256']for n,v in ident['linked'].items()if n.startswith('librustc_driver')]==[DRIVERS[k]] and [v['sha256']for n,v in ident['linked'].items()if n.startswith('libLLVM')]==[LLVM],'ordinary compiler and fixed Cargo')
            need(prod['qa']==meta['qa']and (prod.get('admission')or prod.get('admitted'))==meta['admission'],'actual timed-target QA')
            if case==G:
                for state in ('A','B'):need(prod['semantic'][state]['observed']==doc('qa/'+k+'/accepted.json')['states'][state]['meta']['observed'],'timed-target metadata semantic witness')
            else:need(set(prod['features'])=={'A','B'} and prod['previous_scored_rows']==0,'both timed CI feature clients')
            setup=parsezip(m[pre+'setup.zip']);cold[ph][k]=cargo(setup,'warm',cmd)[0];cargo(setup,'settled',cmd)
            for i,row in enumerate(rows,1):
                bags=parsezip(m[pre+'pair-'+str(i)+'.zip']);new,dirty=cargo(bags,'edit-D',cmd);na,_=cargo(bags,'settled-A',cmd);nb,_=cargo(bags,'noop-D',cmd)
                rr=row[k];need(rr['state']=='measured'and rr['trial']==str(i)and (rr['edit'],rr['parent_noop'],rr['noop'])==(new,na,nb) and sorted(rr['dirty'])==dirty==sorted(n.replace('-','_')for n in qa['rosters'][k]),'first edit and both unedited command clocks')
                if case==C:
                    a,b,no=[json.loads(bags[ph+'.ci-units.json'])for ph in ('settled-A','edit-D','noop-D')];transition(a,b,True);transition(b,no,False)
                    for state in ('A','B'):
                        f=prod['features'][state];need(set(f['members'])==set(a)and f['check_only_public_bounds']==['clap::ValueEnum','schemars::JsonSchema'],'timed lib+harness feature consumers')
        blocks.append(rows);timings[ph]={k:median(z[k]['edit']for z in rows)for k in ARMS}
        if j==0:
            direct=median(row['stock']['edit']/row['candidate']['edit']for row in rows);matched=median(row['parent']['edit']/row['candidate']['edit']for row in rows)
            eligible=direct>=1.8 and matched>1
            g=raw_score['discovery'];need(g['rows']==6 and g['eligible']==eligible and (g.get('direct_median',g.get('direct')),g.get('matched_median',g.get('matched')))==(direct,matched),'excluded discovery decision')
    need(raw_score['registration']==REG[case]and raw_score['admission']==meta['admission'] and raw_score['discovery_rows_scored']==raw_score['previous_scored_rows']==0 and raw_score['heldout_triples']==(12 if eligible else 0),'unpooled enrollment')
    if eligible:
        for name,k,cut in [('direct_signed18_T4','stock',2),('matched_H3_T4','parent',1)]:
            z=base007.contrast(blocks[1:],(k,'candidate'),.05/(14 if case==G else 16),cut);need(z==raw_score[name],'heldout score');summary[k]=z
        need(raw_score['joint']==all(x['passed']for x in summary.values()),'joint decision')
    else:need(not raw_score['joint'],'no eligible performance claim')
    return dict(case=case,command=cmd[1:],source=want_source,excluded_triples=6,direct_discovery=direct,matched_discovery=matched,heldout_triples=raw_score['heldout_triples'],joint=raw_score['joint'],cold_seconds=cold,first_edit_median_seconds=timings,dirty_roster=qa['rosters'],heldout=summary)
def workspace(m,meta,doc):
    term=doc('terminal.json');need(term['registration']==REG[W]and term['scored_pairs']==0 and term['status']in ('failed','complete'),'actual structural terminal')
    for k in ['cold-A','settled-A','first-B','settled-B']:
        tag=k+'/';f=tag+'cargo-return.json'
        if f in m:need(term['files'][f]==H(m[f]),'structural original Cargo return')
    observations=doc('observations.json')
    import gzip
    for stage in ('cold-A','settled-A','first-B','settled-B'):
        tag=stage+'/';local=observations.get(stage,{})
        for name in ('cargo-return.json','raw-trace-size.json','products.json.gz'):
            path=tag+name
            if path not in term['files']:continue
            need(path in m and H(m[path])==term['files'][path],'every retained physical stage receipt')
            if name=='products.json.gz':
                p=json.loads(gzip.decompress(m[path]));need(local['products']==dict(actual_roots=len(p['roots']),cargo_dirty=p['cargo_dirty'],workspace_rustc=len(p['workspace_rustc']),link_owners=len(p['link_execs']),roster_difference=p['roster_difference']),'actual structural counts')
            else:need(local[name]==doc(path),'unaltered structural stream exit and storage budget')
    if term['exit']!=0:
        need(term['status']=='failed' and not term['accepted'],'failure cannot qualify as a full structural result')
        need(observations['failure']['worker_sha256']==term['files']['worker.log'],'native failed worker custody')
        return dict(case=W,status='failed',scored_pairs=0,completed_returns={p.split('/')[0]:json.loads(b) for p,b in m.items()if p.endswith('/cargo-return.json')},available_evidence=doc('observations.json'))
    success=doc('worker-accepted.json');need(term['status']=='complete' and H(m['worker-accepted.json'])==term['accepted'] and success['status']=='complete'and success['root_count']==92 and success['scored_pairs']==0,'four completed full-workspace visits')
    claims={};before=None
    for name in ('cold-A','settled-A','first-B','settled-B'):
        path=name+'/products.json.gz';import gzip
        need(H(m[path])==term['files'][path] and doc(name+'/cargo-return.json')['exit']==0,'retained Cargo source records')
        p=json.loads(gzip.decompress(m[path]));need(len(p['roots'])==92 and not any(p['roster_difference'].values()),'all actual planned products')
        if name.startswith('settled'):need(p['cargo_dirty']==0 and not p['workspace_rustc'] and not p['outside_workspace_nonprobe'] and p['files']==before['files'],'genuine settled products')
        claims[name]=dict(roots=len(p['roots']),cargo_dirty=p['cargo_dirty'],workspace_rustc=len(p['workspace_rustc']),link_owners=len(p['link_execs']),target_allocated=doc(name+'/cargo-return.json')['target_allocated'])
        if name=='cold-A':initial=p
        if name=='first-B':edited=p
        before=p
    from collections import Counter
    states=Counter('new'if n not in initial['files']else'removed'if n not in edited['files']else'different-bytes'if initial['files'][n][-1]!=edited['files'][n][-1]else'same-bytes'for n in initial['files'].keys()|edited['files'].keys())
    need(states==success['product_states'] and sum(states.values())==success['product_count'],'structural artifact comparison')
    for stage in ['A','B']:
        f='semantics-'+stage+'/accepted.json';s=doc(f);need(H(m[f])==term['files'][f] and s['state']==stage and s['client_opposite']==101 and len(set(s['tests']))>=5,'actual workspace semantics after each visit')
    return dict(case=W,status='complete',visits=claims,product_states=states,scored_pairs=0)
def verify(m):
    doc=lambda n:json.loads(m[n]);meta=doc('case.json');k=meta['case'];need(meta['registration']==REG[k],'case registration')
    if k==W:return workspace(m,meta,doc)
    return timed(m,k,meta,doc)
def negative(m):
    names=['case.json']+(['terminal.json']if json.loads(m['case.json'])['case']==W else ['edit.patch','qa/closed.json','discovery/pairs.jsonl','discovery/candidate/pair-6.zip','score.json'])
    for name in names:
        bad=m.copy();del bad[name]
        try:verify(bad)
        except (KeyError,ValueError,AssertionError):continue
        raise ValueError('missing receipt admitted: '+name)
    if json.loads(m['case.json'])['case']!=W:
        for name,field,val in [('score.json','heldout_triples',99),('case.json','fixed_cargo','0'*64),('case.json','headers',[10,12]),('discovery/plan.json','seed',1),('qa/candidate/return.json','exit',1)]:
            bad=m.copy();q=json.loads(bad[name]);q[field]=val;bad[name]=json.dumps(q).encode()
            try:verify(bad)
            except (KeyError,ValueError,AssertionError):continue
            raise ValueError('tampered evidence admitted: '+name+' '+field)
    return names
if __name__=='__main__':
    m=unpack(D/'records.zip');result=verify(m)
    if '--negative' in sys.argv:negative(m)
    print(json.dumps(result,sort_keys=True,indent=2))
