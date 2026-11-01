"""Independent standard-library replay of four cold parents; Python 3.11+."""
import argparse,gzip,hashlib,json,math,re,sys,zipfile
from collections import Counter
from pathlib import Path,PurePosixPath
REG='90db359b0b4b9f820de6db1ec5ccaf3babfe83ce37d7cc492b90e6b8126281fd'
BARE='d8c9b773f401ec3f39fa687556590fe2f9467a014ffbf7a66616abb774073064'
POINT='5c5f7bdd669fdef1b8f1ae51789b88c29f6d292a257dc24d8c9748307f3c5f5e'
CARGO='02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'
ORDER=[['bare','point'],['point','bare']]
VISITS=['cold-A','settled-A','first-B','settled-B']
QA=['client','client-opposite','list','form_metadata_import_names','source_dist_pkg_info_rejects_duplicates']
def digest(b):return hashlib.sha256(b).hexdigest()
def require(b,msg):
    if not b:raise ValueError(msg)
def items(rows):
    return sorted([v['package_id'],v['target']['name'],v['target']['kind'],v['profile']['test'],sorted(v['features'])] for v in rows)
def gates(a,b):return [min(a,b)>1.10 and math.sqrt(a*b)>=1.20,min(a,b)>1.65 and math.sqrt(a*b)>=1.80]
def names():
    top={'index.json','registration.json','subjects.json','source.json','edit.patch','reference.json','plan.json','decision.json','completion.json'}
    for i,order in enumerate(ORDER,1):
        for r in order:
            pref=f'{i:02}-{r}/'
            top.update(pref+f for f in ('terminal.json','accepted.json','loader.json','A-products.json.gz','B-products.json.gz'))
            for v in VISITS:top.update(pref+v+'/'+k for k in ('command.json','stdout.gz','stderr.gz','workspace-rustc.json.gz'))
            for state in 'AB':
                top.add(pref+'semantics-'+state+'/accepted.json')
                for t in QA:
                    top.update(pref+'semantics-'+state+'/'+t+'.'+ext for ext in ('json','stdout','stderr')if (t,ext)!=('client-opposite','stderr'))
                top.add(pref+'semantics-'+state+'/opposite-location.txt')
    return top
def products(prior,post):
    results={}
    for scope in sorted(prior.keys()|post.keys()):
        x=prior.get(scope,{}).get('products',{});y=post.get(scope,{}).get('products',{});count=Counter()
        for name in sorted(x.keys()|y.keys()):
            if name not in x:state='added'
            elif name not in y:state='removed'
            else:
                require(x[name]['key']==y[name]['key'],'named CGU changed identity')
                a,b=x[name]['sig'],y[name]['sig']
                state='different_bytes'if a[-1]!=b[-1]else 'equal_same_inode'if a[:2]==b[:2]else'equal_new_inode'
            count[state]+=1
        results[scope]=dict(count)
    return results
def paircheck(row):
    ratios=[d['bare']['first-B']/d['point']['first-B']for d in row];return ratios,math.sqrt(math.prod(ratios)),gates(*ratios)
def hashed(data):
    return digest(json.dumps(data,sort_keys=True,separators=(',',':')).encode())
def verify(store):
    require(set(store)==names(),'exact complete four-role packet')
    def j(k):return json.loads(store[k])
    def jz(k):return json.loads(gzip.decompress(store[k]))
    def hash_is(k,h):require(digest(store[k])==h,'raw member hash '+k)
    index=j('index.json');require(set(index)==set(store)-{'index.json'},'complete member index')
    for k,h in index.items():hash_is(k,h)
    hash_is('registration.json',REG);reg=j('registration.json');hash_is('subjects.json',reg['subjects']);src=j('source.json');subjects=j('subjects.json')
    require(src['A']=='4954c25a16b246c1b88aef6c3798a76d675c5cd0' and src['B']=='0bbcf757fdf19d43f52c38a5fc2e32a291c262ee' and src['git_patch_sha256']=='ce7a9e41be8607950f6a7f8f36f7eec3ff3b150daab75dbbe80597fe3794b6fb'and src['parent_first_parent'],'true uv parent and child');hash_is('edit.patch',src['git_patch_sha256'])
    require((subjects['roles']['bare']['driver'],subjects['roles']['point']['driver'],subjects['signed_cargo'][0])==(BARE,POINT,CARGO),'true stage2 drivers and fixed Cargo')
    plan=j('plan.json');require(plan==dict(registration=REG,subjects=reg['subjects'],orders=ORDER,scored_rows=0),'original native plan')
    archives={};ref=j('reference.json');require(hashed(ref)=='e3d55743a11f94ea87d742a52392b6b96a794426221168b9ac3fc21dd697613d','independent signed native workload and source-state controls');block=[];stat={};tail=0
    for i,order in enumerate(ORDER,1):
        pair={}
        for role in order:
            d=f'{i:02}-{role}/';t=j(d+'terminal.json');cert=j(d+'accepted.json');loader=j(d+'loader.json');require(t['exit']==0 and t['role']==role and t['registration']==REG==cert['registration'],'completed correct worker')
            for k in store:
                if k.startswith(d) and k!=d+'terminal.json'and not k.endswith('/opposite-location.txt'):hash_is(k,t['files'][k[len(d):]])
            require(cert['order_label']==d[:-1]and cert['role']==role and cert['scored_rows']==0 and cert['source']==[src['A'],src['B'],*src['trees'],src['git_patch_sha256']],'actual edit and order')
            h=subjects['roles'][role];require(loader['librustc_driver']['sha256']==h['driver'] and loader['launcher']==h['rustc'] and loader['libLLVM']['sha256']==h['llvm'],'genuine observed ELF mapping')
            times={}
            for v in VISITS:
                a=j(d+v+'/command.json');lo,hi=a['mono_ns'];require(a['cmd']==['/tmp/uv-workflow-search/sdk/bin/cargo',*reg['command']] and a['exit']==a['scored_rows']==0 and a['rustc_wrapper']is None and hi>lo>=tail and hi-lo==a['wall_ns']==cert['clocks_ns'][v],'ordinary command, chronological wall boundaries and status');times[v]=hi-lo;tail=hi
                raw=[json.loads(l)for l in gzip.decompress(store[d+v+'/stdout.gz']).splitlines()];require(raw[-1]=={'reason':'build-finished','success':True},'Cargo completion');rows=[k for k in raw if k.get('reason')=='compiler-artifact']
                require(len(rows)==501 and hashed(items(rows))==ref['workload_sha256'],'all 501 real package/feature/target records')
                dirty=sorted([k['target']['name'].replace('-','_'),k['profile']['test']]for k in rows if not k['fresh'])
                if v=='first-B':
                    require(dirty==ref['edit_dirty']and len(dirty)==12,'actual first edit dirty closure')
                    processes=jz(d+v+'/workspace-rustc.json.gz');observed={(p['crate'],p['test'])for p in processes};require({('uv_publish',True),('uv_git_types',False),('uv_pypi_types',False)}<=observed and all(p['argv'][0]=='/tmp/uv-workflow-search/sdk/bin/rustc' and not any('self-profile'in a for a in p['argv'])for p in processes),'native Cargo compiler owners and unprofiled argv')
                elif v=='cold-A':require(len(dirty)==501,'empty target cold A')
                else:require(not dirty and not jz(d+v+'/workspace-rustc.json.gz'),'zero-dirty no-op')
            p=[]
            for state in 'AB':
                sem=d+'semantics-'+state+'/';s=j(sem+'accepted.json');hash_is(sem+'accepted.json',cert['semantics'][state]);require(s['state']==state and set(s['owner_argv'])=={'uv_git_types','uv_pypi_types','uv_publish_test'},'matched Cargo owners')
                for name in QA:
                    rr=j(sem+name+'.json');hash_is(sem+name+'.stdout',rr['stdout']);require(rr['exit']==(101 if name=='client-opposite'else 0),'native harness, witnesses and negative state')
                    if name=='client-opposite':require(b'client.rs:30:5'in store[sem+'opposite-location.txt']and b'assertion'in store[sem+'opposite-location.txt']and rr['stderr']==t['files'][sem[len(d):]+name+'.stderr'],'opposite-state trimmed location')
                    else:hash_is(sem+name+'.stderr',rr['stderr'])
                require(s['list_sha256']==ref['semantic'][state]['list_sha256']==digest(store[sem+'list.stdout'])and s['correct_client_sha256']==ref['semantic'][state]['correct_client_sha256']==digest(store[sem+'client.stdout']),'expected native outputs including actual GitUrl state')
                for name in QA[-2:]:require(b'1 passed; 0 failed'in store[sem+name+'.stdout']and (f'tests::{name}: test'.encode())in store[sem+'list.stdout'],'two real named tests')
                inv=jz(d+state+'-products.json.gz');p.append(inv['scopes'])
                require(set(inv['rlibs'])==set(s['externs'])=={'uv_git_types','uv_pypi_types'},'two directly exercised rlibs')
                for name,lib in inv['rlibs'].items():
                    require([lib['rlib'],lib['rmeta']]==s['externs'][name],'real separate archive and rmeta');m=lib['ordered_members']
                    require(len(m)>=2 and any(x[0]in ('/','/SYM64/','__.SYMDEF','__.SYMDEF SORTED')for x in m)and any(x[0].endswith('.o')for x in m)and all(x[1]>=0 and re.fullmatch('[0-9a-f]{64}',x[2])and re.fullmatch('[0-9a-f]{64}',x[3])for x in m),'index and complete member-order fingerprints')
                archives[(i,role,state)]=inv['rlibs'];stat[d+state]={n:{'rlib':v['rlib'][1],'rmeta':v['rmeta'][1],'ordered_content':hashed([m[:3]for m in v['ordered_members']]),'members':len(v['ordered_members'])}for n,v in inv['rlibs'].items()}
            pair[role]=times|{'observed_cgu_objects':products(*p)}
        block.append(pair)
    ratios,geom,enroll=paircheck(block);decision=j('decision.json')
    require(decision==dict(registration=REG,orders=ORDER,cargo=CARGO,visits=block,bare_over_point=ratios,exploratory_geomean=geom,useful_change_screen=enroll[0],twofold_pursuit_gate=enroll[1],automatic_release_training=False,statistical_claim=False,scored_rows=0),'independent complete original result')
    comp=j('completion.json');require(comp['native']==comp['replay']==0 and comp['decision']==digest(store['decision.json'])and comp['registration']==REG,'physical supervisor completed')
    across={}
    for role in ('bare','point'):
        for state in 'AB':
            for lib in sorted(archives[1,role,state]):
                a,b=(archives[i,role,state][lib]['ordered_members']for i in (1,2));x,y=([m for m in l if m[0].endswith('.o')]for l in (a,b))
                left,right=(Counter(m[2]for m in l)for l in (x,y));across['/'.join((role,state,lib))]={'objects':[len(x),len(y)],'identical_object_payloads':sum((left&right).values()),'matching_object_names':len({m[0]for m in x}&{m[0]for m in y}),'equal_GNU_symbol_index_payload':a[0][2]==b[0][2]}
    return {'archive_comparison_independent_histories':across,'orders':ORDER,'wall_seconds':[{r:{v:d[r][v]/1e9 for v in VISITS}for r in order}for order,d in zip(ORDER,block)],'bare_over_point':ratios,'geomean':geom,'useful_change_screen':enroll[0],'twofold_pursuit_gate':enroll[1],'archive_members':stat,'cgu_states':[{r:dict(sum((Counter(c)for c in d[r]['observed_cgu_objects'].values()),Counter()))for r in o}for d,o in zip(block,ORDER)],'scored_rows':0}
def controls(s):
    out=[]
    def reject(name,m):
        try:verify(m)
        except (KeyError,ValueError,TypeError,AssertionError):out.append(name);return
        raise ValueError('false acceptance '+name)
    def recode(x):return(json.dumps(x,sort_keys=True)+'\n').encode()
    def alter(k,data):
        copy=dict(s);copy[k]=data;idx=json.loads(s['index.json']);idx[k]=digest(data);copy['index.json']=recode(idx);return copy
    def rewrite(k,sub):
        obj=json.loads(s[k]);sub(obj);return alter(k,recode(obj))
    reject('missing last cold parent',{k:v for k,v in s.items()if k!='02-bare/terminal.json'})
    reject('swapped plan',rewrite('plan.json',lambda x:x['orders'].reverse()))
    reject('claim despite samples',rewrite('decision.json',lambda x:x.update(twofold_pursuit_gate=not x['twofold_pursuit_gate'])))
    reject('changed first B clock',rewrite('01-bare/first-B/command.json',lambda x:x.update(wall_ns=1)))
    reject('foreign driver',rewrite('01-point/loader.json',lambda x:x['librustc_driver'].update(sha256=BARE)))
    require(gates(1.10,2)==[False,False]and gates(1.3,1.3)==[True,False]and gates(1.65,2.1)==[True,False]and gates(1.8,1.81)==[True,True],'fixed gate boundaries')
    return out
def readzip(path):
    with zipfile.ZipFile(path)as z:
        require(len(z.infolist())==len(names()) and set(z.namelist())==names() and sum(k.file_size for k in z.infolist())<48<<20,'safe bounded member roster')
        return {k:z.read(k)for k in sorted(names())}
if __name__=='__main__':
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('zip',type=Path);a.add_argument('--controls',action='store_true');args=a.parse_args();s=readzip(args.zip);res=verify(s)
    if args.controls:res['rejected_controls']=controls(s)
    print(json.dumps(res,sort_keys=True,indent=2))
