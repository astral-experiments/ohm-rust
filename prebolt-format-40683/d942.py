"""Replay the d9422a4c external rerun using Python 3.11+ and the standard library."""
import argparse
import csv
import hashlib
import io
import itertools
import json
import math
from pathlib import Path
import random
import re
from statistics import median
import subprocess
import sys
import tempfile
from zipfile import ZipFile

D = Path(__file__).resolve().parent
ARMS = ('stock','parent','candidate')
PHASES = ('cold-A','noop-A','edit','noop-B')
FLAGS = ['check','-p','uv-resolver','--locked','--offline','-j8']
DIRTY = ['uv-distribution','uv-resolver','uv-types','uv-workspace']
SEEDS = [415670321,367210859]
A = 'b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7'
B = '1979349b68dbaa936b98c0f409c1df0598fa72f1'
PATCH = '24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb'
CARGO = '02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573'
LLVM = '43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51'
ARCHIVE = '9d4f45f15ff69095b78d6806fa8a449d3e8ac5da36a4275092f3516006c26d28'
REG = 'b75769b86be8953858b57058557ee0cea29f053bda50deb17d346b41a453366c'
SMOKE = 'c252d17bac8461fc75531b953372fe4642c38828fde0a50c803c352e00803e5a'
CID = 'd9422a4c5a8c7bc4269fd821a6aa00bce90b61c9'
SOURCES = {
 'finite-prebolt/rerun.py':'44bf0ba7699645406d1e6ce3b072667ce3ff4c4626e084a9966888f43c6bbb8b',
 'finite-prebolt/base007.py':'e04f7342044b6732cde2c673d0dac841d477a8f8f71b00a0f3387ef65b1869cb',
 'ohm-uv-resolver-repro.py':'3f2b95afb7a8039bd89864ab93b24f2b5b826769dad5cb6d6d38b15d40d6c7a1',
 'finite-prebolt/verify.py':'08f159ac3464d2cac5eb40780a6a54e5f82e7340536ff18a6b708cf995933dfe',
 'finite-prebolt/finite.py':'d6ac7f9e4b250a4baeee494a5ad5e4fdba6e80df2607e04a4f94264d4aa6821c',
 'finite-prebolt/README.md':'c84481f236d738c1fde7d1959383a820d8c8949d89ffa28be95c87503e4b3487',
}
GENERATIONS = {'stock':'a4fa11bcd38280639cb94daacc125472657db6c3','parent':'a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5','candidate':'fd96f14140962cc722aa1335c4449c01f3f3a286'}
ORIGIN = {'host-shape.json','source.json','qualification.json','restore.json','inputs.json','process.json','return.json','verified.json','program.log','smoke.json','registration.json','supervision.json'}
ALL_RECORDS = {f'{i:02}-{arm}/{phase}.{ext}' for i,arm,phase,ext in itertools.product(range(1,13),ARMS,PHASES,('json','stdout','stderr'))} | {'plan.json','edit.patch','summary.json'}
def need(ok, why):
    if not ok:
        raise ValueError(why)
def sha(b):return hashlib.sha256(b).hexdigest()
def load_members(path, names):
    with ZipFile(path) as z:
        entries=z.infolist()
        need(len(entries)==len(names) and set(z.namelist())==names and sum(i.file_size for i in entries)<10<<20 and all(i.file_size<=2<<20 for i in entries),'exact bounded member names and sizes')
        return {i.filename:z.read(i) for i in entries}
def schedule():
    out=[]
    for seed in SEEDS:
        arr=list(itertools.permutations(sorted(ARMS)));random.Random(seed).shuffle(arr)
        out.extend(map(list,arr))
    return out
def contrast(values,orders,ref,threshold):
    sorted_ratios=sorted(values)
    # For 12 paired observations, the second and eleventh order statistics give 99.3652% coverage.
    coverage=1-2*(1+12)/4096
    untied=[v for v in values if v!=threshold]
    wins=sum(v>threshold for v in untied);n=len(untied)
    p=sum(math.comb(n,j)for j in range(wins,n+1))/2**n
    interval=[sorted_ratios[1],sorted_ratios[-2]]
    seed_medians=[median(values[:6]),median(values[6:])]
    order_medians=[median(v for order,v in zip(orders,values,strict=True)if (order.index(ref)<order.index('candidate'))==side)for side in (False,True)]
    return dict(median=median(values),paired=values,interval=interval,coverage=coverage,one_sided_sign_p=p,threshold=threshold,alpha=.01,seed_medians=seed_medians,order_medians=order_medians,
                passed=interval[0]>threshold and p<.01 and min(seed_medians+order_medians)>threshold)
def records(data):
    need(set(data)==ALL_RECORDS,'exact 435 result members')
    plan=json.loads(data['plan.json']);orders=schedule();times={};table=[]
    need(plan['order']==orders and plan['seeds']==SEEDS and plan['parent']==A and plan['child']==B and sha(data['edit.patch'])==plan['patch_sha256']==PATCH,'two original seeds, complete literal edit and source')
    need(plan['cpus']==list(range(16)) and plan['command']==['/tmp/finite-rerun/run/bin/cargo',*FLAGS] and plan['cargo']==CARGO and plan['runner_sha256']==SOURCES['ohm-uv-resolver-repro.py'],'fixed ordinary command and CPU mask')
    need(plan['host']['node']=='finite-repro' and plan['host']['machine']=='x86_64' and plan['host']['system']=='Linux','actual isolated Linux worker')
    end=0;cold={};nonempty_stdout=0
    for i,order in enumerate(orders,1):
        for slot,arm in enumerate(order,1):
            row={};prefix=f'{i:02}-{arm}/'
            for phase in PHASES:
                path=prefix+phase+'.';c=json.loads(data[path+'json']);err=data[path+'stderr'];out=data[path+'stdout']
                need(c['stdout_sha256']==sha(out) and c['stderr_sha256']==sha(err) and c['exit']==0 and c['cpus']==plan['cpus'] and c['argv']==plan['command'],'real logs, exit and exact argv '+path)
                need(type(c['elapsed_ns']) is int and c['end_ns']-c['start_ns']==c['elapsed_ns']>0 and c['start_ns']>=end,'monotone unmodified enclosing Cargo clock '+path)
                end=c['end_ns'];row[phase]=c['elapsed_ns']
                found=sorted(s.decode()for s in re.findall(rb'^\s*(?:Checking|Compiling) ([\w-]+) v',err,re.M))
                need(c['dirty']==found and b'Finished ' in err,'ordinary successful Cargo output '+path)
                nonempty_stdout+=bool(out)
                if phase=='cold-A':
                    need(set(DIRTY)<=set(found) and len(found)>len(DIRTY),'genuine cold build with downstream and dependencies '+path)
                    cold[i,arm]=len(found)
                else:need(found==(DIRTY if phase=='edit' else []),'actual first-edit and no-op units '+path)
            times[i,arm]=row
            table.append([1+(i>6),SEEDS[int(i>6)],i,' '.join(order),slot,arm,*[row[phase]for phase in PHASES],cold[i,arm]])
    measures={}
    for label,arm,threshold in [('signed18_over_T4','stock',2),('optimized_T3_over_T4','parent',1)]:
        vals=[times[i,arm]['edit']/times[i,'candidate']['edit']for i in range(1,13)]
        measures[label]=contrast(vals,orders,arm,threshold)
    byarm={arm:{phase:median(times[i,arm][phase]for i in range(1,13))for phase in PHASES}for arm in ARMS}
    summary=json.loads(data['summary.json'])
    need(all(measures[k]==summary[k] for k in measures) and summary['arm_median_ns']=={k:byarm[k]['edit']for k in ARMS},'independent paired statistics agree with published summary')
    return {'ordinary_Cargo_invocations':144,'nonempty_stdout_observations':nonempty_stdout,'separate_cold_parents':36,'fresh_triples':12,'cold_dirty_counts':{arm:sorted({cold[i,arm]for i in range(1,13)})for arm in ARMS},'medians_ns':byarm,'comparisons':measures},table
def validate_origin(origin):
    need(all(sha(origin['published/'+k])==v for k,v in SOURCES.items()),'unchanged d942 published source')
    need(sha(origin['source.json'])=='f859d87abb1354b130684402e468226d0505f43db953e9c3da3be5534176ea00' and sha(origin['qualification.json'])=='d93589f4b51070af9e57a920db50bb385c516a89d4cde4e446685af6ebb603af','original independently published source and native QA')
    need(sha(origin['smoke.json'])==SMOKE and sha(origin['registration.json'])==REG,'original native dispatch')
def identities(origin,data,digest):
    read=lambda n:json.loads(origin[n]);q=read('qualification.json');src=read('source.json');backup=read('restore.json');inp=read('inputs.json');s=read('smoke.json');reg=read('registration.json')
    validate_origin(origin)
    host=read('host-shape.json');need(sha(origin['host-shape.json'])=='7eac0338d02abcb5a28a215deee154043780c56ae255036df8b5fb05c43dcec9' and host['control_cpu']==29 and host['benchmark_cpus']==list(range(16)) and host['hardware']['Thread(s) per core']=='1' and host['hardware']['CPU(s)']=='32' and host['cpu_max'] is None,'contemporaneous guest topology')
    need(inp['registration']==REG and reg['publication']==CID and reg['actual_readonly_dispatch']==SMOKE and s['source']==inp['source'] and inp['source']['patch']==PATCH and inp['source']['crates']==inp['source']['sparse_checksum_rows']==658 and inp['source']['archives_bytes']==79475307,'actual guarded entry, immutable source and registry')
    plan=json.loads(data['plan.json']);need(plan['registry_configs']=={'/.cargo/config.toml':inp['source']['config']} and inp['source']['describe']==['0.12.2-14-gb60da7a','0.12.2-15-g1979349'] and src['A']==src['parent_of_B']==A and src['B']==B,'authenticated lockfiles, tag and ordinary registry configuration')
    need(backup['sha256']==inp['archive']==ARCHIVE and backup['bytes']==505864346 and backup['tar_compared'] and backup['fixed_cargo']==CARGO and backup['admission_sha256']==q['admission_sha256'],'original optimized archive and prior native admission')
    need(inp['subjects']==s['loader_sha256'] and s['compiler_runs']==s['cargo_runs']==0 and all(inp['dispatcher'][n]==h for n,h in SOURCES.items()if n in inp['dispatcher']) and len(inp['dispatcher'])==3 and s['published_program']==inp['dispatcher'],'physical loader and original source smoke')
    for arm,sw in zip(ARMS,[q['stock'],q['optimized']['T3'],q['optimized']['T4']],strict=True):
        ident=plan['sdks'][arm]
        need(inp['subjects'][arm]=={'driver':sw['driver_sha256'],'launcher':sw['launcher_sha256'],'llvm':LLVM} and ident['launcher']==sw['launcher_sha256'] and ident['librustc_driver']['sha256']==sw['driver_sha256'] and ident['libLLVM']['sha256']==LLVM,'actual SDK under strict original identities')
        need(ident['librustc_driver']['path']==ident['sdk']+'/'+sw['driver_relative'] and ident['libLLVM']['path'].startswith(ident['sdk']+'/lib/') and 'host: x86_64-unknown-linux-gnu' in ident['version'],'actual sysroot and loader')
        if arm!='stock':
            saved=backup['roles'][arm]
            need(len(saved['files'])==77 and saved['files']['bin/rustc'][0]==sw['launcher_sha256'] and saved['files'][sw['driver_relative']][0]==sw['driver_sha256'] and saved['profile_sha256']==sw['profile_sha256']==sw['use_profile_sha256'] and sw['commit']==GENERATIONS[arm] and sw['header_format']==(10 if arm=='parent' else 12) and sw['driver_BOLT'] is False,'whole measured SDK, profile and source')
    op=read('process.json');term=read('return.json');vr=read('verified.json');sup=read('supervision.json');program=origin['program.log']
    argv=op['original_cargo_argv'];need(argv[:5]==['taskset','-c','0-15','/usr/bin/python3','-ISB'] and argv[5]=='/tmp/finite-rerun/inputs/published/finite-prebolt/rerun.py' and argv[6::2]==['--uv','--stock','--parent','--candidate','--cargo','--cargo-home','--scratch'] and argv[9]==plan['sdks']['stock']['sdk'] and argv[11]==plan['sdks']['parent']['sdk'] and argv[13]==plan['sdks']['candidate']['sdk'] and argv[15]==argv[9]+'/bin/cargo' and argv[7]=='/tmp/finite-rerun/inputs/bank/uv.git' and argv[17]=='/tmp/finite-rerun/inputs/bank' and argv[19]=='/tmp/finite-rerun/run','published strict run argv')
    need(sup['status']=='accepted' and op['exit']==term['exit']==0 and op['complete'] and term['complete'] and term['log_sha256']==sha(program) and term['registration']==REG,'worker and supervisor returned zero')
    need(re.findall(rb'^Completed triple (\d+)/12$',program,re.M)==[str(i).encode()for i in range(1,13)] and vr['raw_zip']==digest and vr['members']==435 and vr['summary_sha256']==sha(data['summary.json']) and vr['replay']==json.loads(data['summary.json']) and not vr['replay']['compiler_identity']['changed_compilers'],'genuine durable checkpoint stream and literal replay')
    return inp
def csv_bytes(table):
    out=io.StringIO();w=csv.writer(out,lineterminator='\n');w.writerow(['block','seed','triple','order','position','role',*[n+'_ns'for n in PHASES],'cold_dirty']);w.writerows(table);return out.getvalue().encode()
def prior_qa(p,q):
    script="""import importlib.util,json,runpy,sys
sp=importlib.util.spec_from_file_location('prior',sys.argv[1]);m=importlib.util.module_from_spec(sp);sp.loader.exec_module(m)
issuer=runpy.run_path(sys.argv[2]);doc=json.load(open(sys.argv[3]))
issuer['check_fixture'](doc);issuer['check_failure'](doc['displaced_generation']);issuer['configure'](m).qualify(doc)
"""
    subprocess.run([sys.executable,'-ISB','-c',script,p.with_name('finite.py'),p.with_name('verify.py'),q],check=True)
def stock_replay(origin,data):
    with tempfile.TemporaryDirectory()as tmp:
        d=Path(tmp)
        for group,base in [(data,d/'rows'),({k:v for k,v in origin.items() if k.startswith('published/')},d)]:
            for name,b in group.items():
                p=base/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b)
        p=d/'published/finite-prebolt/rerun.py'
        result=json.loads(subprocess.check_output([sys.executable,'-ISB',p,'--summarize',d/'rows']))
        need(result==json.loads(data['summary.json']),'unmodified published --summarize byte-value equality')
        # Reuse only the genuine preexisting native correctness issuer. No prior performance input.
        q=d/'prior-qualification.json';q.write_bytes(origin['qualification.json'])
        prior_qa(p,q)
def reject(data,origin,digest):
    def altered(name,key,transform):
        d=data.copy();v=json.loads(d[key]);transform(v);d[key]=json.dumps(v).encode()
        try:records(d)
        except (ValueError,KeyError,TypeError):return name
        raise AssertionError('unrejected: '+name)
    checks=[altered('first edit replaced by zero-dirty check','01-stock/edit.json',lambda v:v.update(dirty=[])),
            altered('monotonic elapsed altered','02-parent/edit.json',lambda v:v.update(elapsed_ns=v['elapsed_ns']+1)),
            altered('confirmation order swapped','plan.json',lambda v:v['order'][0].reverse()),
            altered('published ratio altered','summary.json',lambda v:v['signed18_over_T4'].update(median=0))]
    d=data.copy();key='01-stock/cold-A';r=json.loads(d[key+'.json']);d[key+'.stderr']=d['01-stock/noop-A.stderr'];r.update(stderr_sha256=sha(d[key+'.stderr']),dirty=[]);d[key+'.json']=json.dumps(r).encode()
    try:records(d)
    except ValueError:checks.append('rehashed zero-dirty parent misrepresented as a fresh cold start')
    else:raise AssertionError('false cold parent accepted')
    d=data.copy();del d['12-candidate/noop-B.stdout']
    try:records(d)
    except ValueError:checks.append('incomplete last triple')
    else:raise AssertionError('partial accepted')
    o=origin.copy();v=json.loads(o['inputs.json']);v['subjects']['candidate']=v['subjects']['parent'];o['inputs.json']=json.dumps(v).encode()
    try:identities(o,data,digest)
    except ValueError:checks.append('swapped candidate driver')
    else:raise AssertionError('wrong compiler accepted')
    return checks
def main(root,neg):
    path=root/'records.zip';digest=sha(path.read_bytes());data=load_members(path,ALL_RECORDS)
    origin=load_members(root/'recipe.zip',{'published/'+n for n in SOURCES}|ORIGIN)
    identities(origin,data,digest);result,table=records(data);stock_replay(origin,data)
    need(csv_bytes(table)==(root/'times.csv').read_bytes(),'every raw integer time and order in CSV')
    if neg:result['rejected_controls']=reject(data,origin,digest)
    return result
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--negative',action='store_true');a=p.parse_args()
    print(json.dumps(main(D,a.negative),indent=2,sort_keys=True))
