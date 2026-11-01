"""Replay the two native Cargo endpoints from their admitted raw rows. Python 3.11+, stdlib."""
import argparse, csv, hashlib, io, itertools, json, math, random, re, statistics, zipfile
from pathlib import Path

R=('stock','candidate')
A='b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7'; B='1979349b68dbaa936b98c0f409c1df0598fa72f1'
SEAL='e44b58dc6d00f7b837d137174a84d34515979471aefee0959cfc8a9265a052e6'
EDIT='24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb'
I='e417531add1277ac1b77005668c4b75d9387827325f7938830b125d807074d87'
CARGO='02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'
LLVM='43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51'
DR=dict(zip(R,['a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de','f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642']))
LA=dict(zip(R,['c6ab483d6698f68eebb49d74b27361f3e02ede55452dc0416c3fdec9a45b7f60','594a7f3e784ffb15d62899adb3c2db2b33e176ee55406003fe1b0481734f473d']))
T4='fd96f14140962cc722aa1335c4449c01f3f3a286';T4_TREE='7fd44bf57a11609d65d7feeea1db6b6279a77f04'
PROF='70892579bb848a9eb268d1d74538e53b849f3de4c0df6a05a504f071bdb869ea'
LIST='ec73b0be264a4f677a47495384b47f0de80d9a28308e474424db446a4554920f'
NAMES={'uv_workspace','uv_types','uv_distribution','uv_resolver'}
SDK='/tmp/uv-workflow-search/sdk/bin/'
FOCUS='workspace::tests::exclude_package_with_normalized_glob_and_escaped_root'
PH=('discovery','confirm-1','confirm-2')
median=statistics.median
def sha(b):return hashlib.sha256(b).hexdigest()
def need(ok,what):
    if not ok:raise ValueError(what)
def checkzip(z,small):
    names=z.namelist();need(len(names)==len(set(names)) and all(not Path(n).is_absolute() and '..' not in Path(n).parts for n in names),'ZIP names')
    need(sum(n.file_size for n in z.infolist())<small,'bounded ZIP')
def plain(b):return json.loads(b)
def order(seed):
    opts=list(itertools.permutations(sorted(R)));rng=random.Random(seed);out=[]
    for _ in range(3):rng.shuffle(opts);out.extend(map(list,opts))
    return out
def signed_identity(k,x):
    need(x['cargo']==CARGO and x['launcher']==LA[k] and x['sysroot']==SDK.removesuffix('/bin/'),'actual Cargo/launcher/sysroot')
    ds=x['linked'];need(len(ds)==2 and {v['sha256'] for v in ds.values()}=={LLVM,DR[k]} and all(v['path'].startswith('/tmp/uv-workflow-search/sdk/')for v in ds.values()),'loader-selected driver and LLVM')
def listing(b):
    n=sorted(x[:-6] for x in b.decode().splitlines() if x.endswith(': test'))
    need(sha(b)==LIST and len(n)==len(set(n))==99,'99 registered tests');return n
def passed(b,expected):
    names=sorted(re.findall(rb'^test ([\w:]+) \.\.\. ok$',b,re.M))
    need(names==[n.encode() for n in expected] and f'test result: ok. {len(expected)} passed; 0 failed; 0 ignored; 0 measured;'.encode() in b,'test execution')
def command(form):return [SDK+'cargo',form,'-p','uv-resolver','--lib',*(['--no-run']if form=='test' else []),'--locked','--offline','-j8']
def prog(row,raw,form,label):
    actual=[n.decode().replace('-','_') for n in re.findall(rb'^\s*(?:Checking|Compiling) ([\w-]+) v',raw,re.M)]
    need(row['cmd']==command(form) and row['dirty_units']==actual and math.isfinite(row['wall_s'])and row['wall_s']>0,'Cargo argv/progress/clock')
    if label=='edit-D':need(len(actual)==4 and set(actual)==NAMES,'four edited units')
    elif label=='warm':need(NAMES<=set(actual),'cold native build')
    else:need(not actual,'zero-dirty repeat')
def paired(row,seq,k,i,items):
    need(list(row)==seq and set(seq)==set(R) and row[k]['state']=='measured' and int(row[k]['trial'])==i,'registered pairing/order')
    for label,field in [('settled-A','parent_noop'),('edit-D','edit'),('noop-D','noop')]:need(row[k][field]==items[label][0]['wall_s'],'modified or unmatched row')
    need(row[k]['dirty']==items['edit-D'][0]['dirty_units'],'pair dirty rows')
def archive(b,form,labels):
    data={}
    with zipfile.ZipFile(io.BytesIO(b))as z:
        checkzip(z,8<<20)
        for label in labels:
            rd=lambda ext:z.read(label+'.'+ext)
            v=plain(rd('verified.json'));artifact=plain(rd('artifact.json'));obs=plain(rd('json'));raw=rd('stderr')
            prog(obs,raw,form,label);need(v['artifact']==artifact and artifact['size']>0 and re.fullmatch('[0-9a-f]{64}',v['sha256']) and set(v['units'])==NAMES,'product identity')
            need(rd('stdout')==b'' and set(plain(rd('usage.json')))>={'user_us','sys_us'},'ordinary stdout/resource evidence')
            if form=='test':
                exes=re.findall(rb'^\s*Executable [^\n]*\(([^()\n]*uv_resolver-[0-9a-f]+)\)\s*$',raw,re.M)
                need(len(exes)==1 and exes[0].decode().endswith(artifact['path']) and v['listing']==LIST,'Cargo-selected native executable')
                listing(rd('listing'))
            else:
                members=rd('members');need(artifact['path']=='debug/libuv_resolver.rlib' and sha(members)==v['archive_members'] and b'lib.rmeta' in members.splitlines() and any(m.endswith(b'.o') for m in members.splitlines()),'measured resolver archive and members')
            data[label]=(obs,v)
    return data
def change(a,b):
    need(a['artifact']['path']==b['artifact']['path'] and all(a['units'][n][0]==b['units'][n][0] and a['units'][n][1]<b['units'][n][1]for n in NAMES),'four newer same-unit stamps')
def qualify(q):
    t=q['T4'];stock=q['signed18']
    need(t['commit']==T4 and t['tree']==T4_TREE and t['header_format']==12 and t['profile_sha256']==t['use_profile_sha256']==PROF and len(t['training'])==9 and t['driver_BOLT'] is False,'T4 release flags and own 9-workload profile')
    need(t['driver_sha256']==DR['candidate'] and stock['driver_sha256']==DR['stock'] and t['llvm_sha256']==q['llvm']==LLVM and q['fixed_cargo']==CARGO,'qualified SDKs')
    need(stock['release']['toolchain']=='ohm-1.99.0-18' and stock['release']['sources']['cargo']['revision']=='7caf39914da1e28a67744ecbc963a5f26361474d','signed release')
    s=t['suites'];need(len(s)==len({x['suite']for x in s})==5 and all(x['command_exit']==0 and x['mapped'] and all(y['driver_sha256']==DR['candidate']and y['launcher_sha256']==LA['candidate']for y in x['mapped'])for x in s),'five mapped native suites')
    f=t['finite'];need(f['driver_sha256']==DR['candidate'] and f['header']==12 and f['profile_sha256']==PROF and set(f['solvers'])=={'T4-T4/default','T4-T4/next'},'both finite solver qualifications')
    need(all(y['llvm_sha256']==LLVM for x in s for y in x['mapped']) and set(t['native_phases'])=={'generate','train','use','qualified'} and all(t['native_phases'][phase]['exit']==0 for phase in t['native_phases']),'compiled, trained and qualified native role')
    for x in f['solvers'].values():
        forms=x['fingerprints'];counts=x['provider_counts']
        need(counts['unrelated']['primitive']==counts['unrelated']['pair']==0 and counts['unrelated']['wide']==1,'reuse and unsupported-shape control')
        need(all(forms['A'][k]==forms['unrelated'][k]==forms['removed'][k]!=forms['specialize'][k]for k in ('primitive','pair')),'genuinely changed and removed matching impls')
def qa(get,form,rev,observed):
    j=lambda n:plain(get(n));success=j('success.json');need(success=={'measured_sha256':observed['sha256'],'form':form,'revision':rev},'QA against measured product')
    def transcript(n):
        c=j(n+'.json');out=get(n+'.out');err=get(n+'.err');need(c['exit']==0 and c['stdout']==sha(out)and c['stderr']==sha(err),'command return/transcript');return c['argv'],out,err
    file='/tmp/uv-workflow-search/target/'+observed['artifact']['path']
    if form=='build':
        args,_,_=transcript('link-measured-rlib');client='/tmp/uv-workflow-search/resolver-rlib-client-'+rev
        need(args[:7]==[SDK+'rustc','--crate-name','resolver_product_check','--edition=2024',client+'/client.rs','--extern','uv_resolver='+file]and args[-2:]==['-o',client+'/client'],'consumer linked exact rlib')
        args,out,_=transcript('run-rlib-client');need(args==[client+'/client'] and out==b'highest|lowest|lowest-direct\n','actual consumer')
    else:
        args,ls,_=transcript('exact-list');need(args==[file,'--list'],'listed actual measured product')
        args,out,err=transcript('all-resolver-tests');need(args==[SDK+'cargo','test','-p','uv-resolver','--lib','--locked','--offline','-j8','--','--test-threads=1'] and Path(file).name.encode() in err and not re.search(rb'^\s*(?:Compiling|Checking) ',err,re.M),'zero-compile execution of measured harness');passed(out,listing(ls))
    if rev=='B':
        args,out,_=transcript('changed-workspace-test');need(args==[SDK+'cargo','test','-p','uv-workspace','--lib','--locked','--offline','-j8',FOCUS,'--','--exact','--test-threads=1'],'actual workspace regression');passed(out,[FOCUS])
def cohort(z,reg,form,phase,clock_rows):
    pre=f'{form}/{phase}/';rd=lambda n:z.read(pre+n);j=lambda n:plain(rd(n))
    closure=j('closed.json');need(closure['registration']==SEAL,'original positive closure')
    plan=j('plan.json');issuer=j('subject-intake.json');cfg=plan['config'];seq=order(cfg['seed'])
    for name,digest in closure['raw'].items():
        if name in ('plan.json','subject-intake.json'):need(j('source-digests.json')[name]==digest,'hash of original machine locator document')
        else:need(sha(rd(name))==digest,'original closed file '+name)
    need(plan['source']==[A,B]and plan['runner']==reg['files']['run.py'] and cfg['case']=='resolver-'+form and cfg['plain'] and not cfg['diagnostics'] and cfg['pairs']==6 and seq==plan['sequence']==reg['schedules'][f'{form}-{phase}-306-001'],'source/registration/schedule')
    fi=['build','test'].index(form);pi=PH.index(phase)
    need(cfg['seed']==2026110105110271+401*pi+937*fi and list(cfg['sdks'])==(list(R)if (fi+pi)%2==0 else list(reversed(R))),'frozen seed and cold order')
    need(issuer['registration']==SEAL and issuer['issuer']==I and issuer['arms']['candidate']['commit']==T4 and issuer['arms']['candidate']['header_format']==12 and issuer['arms']['candidate']['profile_sha256']==PROF,'admitted actual T4')
    rows=[plain(l)for l in rd('pairs.jsonl').splitlines()];need(len(rows)==6,'complete phase');seconds={}
    for k in R:
        stem=k+'/';arm=issuer['arms'][k];need(arm['driver_sha256']==DR[k]and arm['launcher_sha256']==LA[k]and plan['inputs'][k]['cargo']==CARGO and plan['inputs'][k]['launcher']==LA[k],'selected arm and overlay')
        signed_identity(k,j(stem+'identity.json'));need(sha(rd(stem+'edit.patch'))==EDIT and rd(stem+'edit.patch')==z.read('uv-20988.patch'),'actual unstaged edit')
        need(b'ohm-1.99.0-18' in rd(stem+'cargo.txt') and b'LLVM version: 23.1.1' in rd(stem+'rustc.txt'),'observed executables')
        cold=archive(rd(stem+'setup.zip'),form,('warm','settled'));a=cold['settled'][1];need(a==cold['warm'][1],'original warm A and identical settled A')
        for l,(obj,v) in cold.items():clock_rows.append([form,phase,0,k,l,obj['wall_s'],v['sha256'],list(cfg['sdks']).index(k)+1])
        for i,(row,actual_order) in enumerate(zip(rows,seq,strict=True),1):
            three=archive(rd(stem+f'pair-{i}.zip'),form,('settled-A','edit-D','noop-D'));first=three['settled-A'][1];b=three['edit-D'][1]
            paired(row,actual_order,k,i,three);need(first['sha256']==a['sha256']and first['units']==a['units']and b==three['noop-D'][1],'own A restore and equal own B repeat');change(first,b)
            for l,(obj,v) in three.items():clock_rows.append([form,phase,i,k,l,obj['wall_s'],v['sha256'],actual_order.index(k)+1])
        for rev,v,d in [('A',a,'offclock-A/'),('B',b,'offclock/')]:qa(lambda n:rd(stem+d+n),form,rev,v)
        seconds[k]={'warm':cold['warm'][0]['wall_s'],'settled':cold['settled'][0]['wall_s'],**{n:median(r[k][n]for r in rows) for n in ('edit','noop','parent_noop')}}
    ratios=[[row['stock']['edit']/row['candidate']['edit'],list(row)[0]]for row in rows];need(ratios==closure['ratios'],'raw paired ratio')
    return ratios,seconds
def result(rows,form,separate):
    ratios=rows['discovery'];opens=median(x for x,_ in ratios)>=1.8
    r={'form':form,'discovery':ratios,'discovery_median':median(x for x,_ in ratios),'seconds':separate,'opens':opens,'heldout_pairs':0,'status':'screen below 1.8'}
    if opens:
        need(set(rows)==set(PH),'earned separate held-outs');cases=[rows[k]for k in PH[1:]];v=[x for b in cases for x,_ in b];vsort=sorted(v); orders=[median(x for b in cases for x,k in b if k==r)for r in R];seeds=[median(x for x,_ in t)for t in cases];signs=[x for x in v if x!=2];p=sum(math.comb(len(signs),a)for a in range(sum(x>2 for x in signs),len(signs)+1))/2**len(signs)
        r.update(heldout_pairs=12,status='two independently seeded cohorts complete',ratios=v,median=median(v),order_stat_interval=[vsort[1],vsort[-2]],coverage=1-26/4096,orders=orders,seeds=seeds,p_above_two=p,qualifies=vsort[1]>2 and p<.025 and min(orders+seeds)>2)
    else:need(set(rows)=={'discovery'},'below-gate phase is discovery only')
    return r
def verify(path):
    clocks=[];scores={}
    with zipfile.ZipFile(path)as z:
        checkzip(z,50<<20);manifest=plain(z.read('MANIFEST.json'));need(set(manifest)==set(z.namelist())-{'MANIFEST.json'} and all(sha(z.read(n))==h for n,h in manifest.items()),'whole packet content')
        reg=plain(z.read('measurement/registration.json'));need(sha(z.read('measurement/registration.json'))==SEAL and sha(z.read('uv-20988.patch'))==EDIT,'original preregistration and source')
        for name in ('run.py','products.py','controls.py','client.rs','README.md'):need(sha(z.read('measurement/'+name))==reg['files'][name],'registered implementation selector '+name)
        src=plain(z.read('source.json'));need(src['A']==src['parent_of_B']==A and src['B']==B and src['lock_A']==src['lock_B']and src['patch_sha256']==EDIT,'original source facts')
        qualify(plain(z.read('qualification.json')))
        for form in ('build','test'):
            phases=[k for k in PH if f'{form}/{k}/closed.json' in manifest];need(phases and phases[0]==PH[0],'completed discovery')
            rows={};seconds={}
            for ph in phases:rows[ph],seconds[ph]=cohort(z,reg,form,ph,clocks)
            scores[form]=result(rows,form,seconds);need(scores[form]==plain(z.read(form+'/score.stdout.json')),'original final scorer')
    hold={k:i for i,k in enumerate(PH)};labels={k:i for i,k in enumerate(('warm','settled','settled-A','edit-D','noop-D'))}
    clocks.sort(key=lambda x:(x[0],hold[x[1]],x[2],x[-1],labels[x[4]]))
    return scores,clocks
def write_csv(stream,rows):
    w=csv.writer(stream);w.writerow(['command','cohort','pair','arm','observation','seconds','product_sha256','order_within_setup_or_pair']);w.writerows(rows)
if __name__=='__main__':
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('packet',nargs='?',type=Path,default=Path(__file__).with_name('records.zip'));ap.add_argument('--csv',type=Path,help='write every raw clock and product hash');a=ap.parse_args();s,cl=verify(a.packet)
    if a.csv:
        with a.csv.open('x',newline='')as stream:
            write_csv(stream,cl)
    for form,r in s.items():
        sec=r['seconds']['discovery'];v=[x for x,_ in r['discovery']]
        print(f"{form}: six discovery pairs, signed {sec['stock']['edit']:.6f}s, T4 {sec['candidate']['edit']:.6f}s, median {r['discovery_median']:.6f}x, range {min(v):.6f}–{max(v):.6f}x, {sum(x>1 for x in v)}/6 faster, {sum(x>=2 for x in v)}/6 at 2x; {r['heldout_pairs']} held-out pairs.")
        if r['heldout_pairs']:print('  Held-out:',r['median'],r['order_stat_interval'],'qualified:',r['qualifies'])
    print(f"{len(cl)} raw ordinary Cargo calls verified. Both zero-dirty repeats and off-clock A/B product checks passed.")
