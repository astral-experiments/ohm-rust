"""Reproduce a Ruff public API graph experiment with two same-stamp LLVM rustc drivers, as a regular user."""
import argparse
import hashlib
import json
import random
import runpy
import statistics
import gzip
import os
from pathlib import Path
import shutil
import subprocess as sp
import sys
import time

HERE=Path(__file__).resolve().parent
TARGET='x86_64-unknown-linux-gnu'
PREVIOUS='93526150df078af7dd8fc4226cdcc469539d2001'
p=argparse.ArgumentParser(description=__doc__)
for n in ['ruff','parent-sdk','candidate-driver','stock-sdk','cargo-home','work']:
    p.add_argument('--'+n,required=True,type=Path)
p.add_argument('--rustup-bin-dir',default=Path.home()/'.cargo/bin',type=Path)
p.add_argument('--min-free-gib',type=float,default=1)
p.add_argument('--min-memory-gib',type=float,default=2)
p.add_argument('--native-only',action='store_true')
p.add_argument('--source-pair', choices=['original','fixed-stack'], default='original', help='source pair used to build the two SDK drivers')
p.add_argument('--crate', choices=['ruff_source_file','ruff_python_trivia'], required=True)
p.add_argument('--mode', choices=['check','build','test'], required=True)
a=p.parse_args()
if os.getuid()==0:p.error('Run Cargo and rustc as a regular user.')
for f in [a.ruff,a.parent_sdk,a.stock_sdk,a.cargo_home,a.rustup_bin_dir]:
    if not f.is_dir():p.error(f'missing directory {f}')
if a.work.exists() or a.work.is_symlink():p.error('--work must be an absent path under an existing owned directory')
W=a.work.resolve(); source=a.ruff.resolve();parent=a.parent_sdk.resolve();stock=a.stock_sdk.resolve();candidate=a.candidate_driver.resolve()
assert W.parent.is_dir() and source.is_dir() and candidate.is_file() and os.uname().machine=='x86_64'
assert sp.check_output(['git','--no-optional-locks','-C',str(source),'rev-parse','HEAD']).decode().strip()==PREVIOUS
def reserve():
    mem=int(next(x for x in Path('/proc/meminfo').read_text().splitlines() if x.startswith('MemAvailable:')).split()[1])*1024
    avail=shutil.disk_usage(W.parent).free
    if min(avail-a.min_free_gib*2**30,mem-a.min_memory_gib*2**30)<=0:raise RuntimeError(f'local capacity guard: disk {avail}, memory {mem}')
def digest(f):
    with Path(f).open('rb') as s:return hashlib.file_digest(s,'sha256').hexdigest()
def put(f,body):f.write_text(json.dumps(body,sort_keys=True,indent=2)+'\n')
reserve();W.mkdir(mode=0o700);(W/'.rerun-owner').write_text('private rustc graph replication\n')
probe=W/'check-exec';shutil.copy2('/bin/true',probe)
try:sp.run([probe],check=True)
except PermissionError:raise SystemExit('The private work filesystem is mounted noexec; use an executable filesystem for the supplied SDK and test programs.')
probe.unlink()
(W/'workload').write_text(a.mode+'\n');out=W/'records';out.mkdir();(W/'tmp').mkdir();(W/'cargo-home').symlink_to(a.cargo_home.resolve(),target_is_directory=True)
sp.run(['git','clone','--quiet','--shared','--no-checkout','--',str(source),str(W/'source')],check=True)
sp.run(['git','-C',W/'source','checkout','--quiet','--detach',PREVIOUS],check=True)
sdk=W/'sdk';(sdk/'bin').mkdir(parents=True);(sdk/'lib/rustlib'/TARGET/'bin').mkdir(parents=True)
shutil.copy2(parent/'bin/rustc',sdk/'bin/rustc');(sdk/'bin/cargo').symlink_to(stock/'bin/cargo')
pd,=list((parent/'lib').glob('librustc_driver-*.so'))
for f in (parent/'lib').iterdir():
    if f.name!=pd.name and f.name!='rustlib':(sdk/'lib'/f.name).symlink_to(f,target_is_directory=f.is_dir())
for f in (parent/'lib/rustlib').iterdir():
    if f.name!=TARGET:(sdk/'lib/rustlib'/f.name).symlink_to(f,target_is_directory=f.is_dir())
for f in (parent/'lib/rustlib'/TARGET).iterdir():
    if f.name!='bin':(sdk/'lib/rustlib'/TARGET/f.name).symlink_to(f,target_is_directory=f.is_dir())
for f in (stock/'lib/rustlib'/TARGET/'bin').iterdir():
    (sdk/'lib/rustlib'/TARGET/'bin'/f.name).symlink_to(f,target_is_directory=f.is_dir())
rustup=W/'rustup';(rustup/'toolchains').mkdir(parents=True);(rustup/'toolchains/ohm').symlink_to(sdk,target_is_directory=True)
(rustup/'settings.toml').write_text('version = "12"\nprofile = "minimal"\n\n[overrides]\n')
common=[digest(sdk/'bin/cargo'),digest(sdk/'lib/rustlib'/TARGET/'bin/wild-ld/ld.bfd')]
revisions = {
    'original': ('722b9412d2386e70d6cffba132544c0e3ae09cd7', '181a2fd00972e05e75b47ec4c3cf1941f0f0d8ec'),
    'fixed-stack': ('569aefbe9016e23bbfdf7f5615990102f2cb5e44', '6ab241bbf3ee174260bb553ed29b1e29e64e885d'),
}
selected = dict(zip('AB', revisions[a.source_pair]))
sources={'A':pd,'B':candidate};lanes={'A':'current-722','B':'child-public-graph'};states={}
for arm,f in sources.items():
    root=W/('subject-'+arm);root.mkdir()
    print('authenticating',arm,str(f),flush=True)
    states[arm]={'commit':selected[arm], 'source_pair':a.source_pair,'driver':digest(f),'common':common,'rustc':digest(sdk/'bin/rustc'),'source':str(f)}
    put(root/'product.json',states[arm])
assert states['A']['driver']!=states['B']['driver'],'requires two different actual drivers'
put(out/'tool-inputs.json',{'subject':states,'affinity':sorted(os.sched_getaffinity(0)),'ruff':PREVIOUS,'native_only':a.native_only,'shared_standard_libraries':{f.name:digest(f) for f in sorted((sdk/'lib/rustlib'/TARGET/'lib').iterdir()) if f.is_file()},'parent_compiler_root':str(parent),'stock':str(stock)})
def phase(arm,name,label):
    reserve()
    driver=sdk/'lib'/pd.name
    if not driver.exists() or digest(driver)!=states[arm]['driver']:
        temporary=driver.with_suffix('.new');shutil.copy2(sources[arm],temporary);os.replace(temporary,driver)
    assert digest(driver)==states[arm]['driver']
    env=dict(os.environ,OHM_RERUN_RUFF=str(source),OHM_RERUN_WORK=str(W),OHM_RERUN_PROXY_DIR=str(a.rustup_bin_dir.resolve()),OHM_RERUN_MIN_DISK=str(int(a.min_free_gib*(1<<30))),OHM_RERUN_MIN_RAM=str(int(a.min_memory_gib*(1<<30))),OHM_STUDY_SDK=str(sdk),OHM_STUDY_PRODUCT=str(W/('subject-'+arm)),RUSTUP_HOME=str(rustup))
    sp.run([sys.executable,'-IB',HERE/'worker.py',a.crate,out,name,label,lanes[arm]],check=True,env=env)
def get(name):
    return json.loads((out/name).read_bytes())

def admit():
    import re
    link_temp=runpy.run_path(str(HERE/'link_temp.py'))['is_link_temp_argument']
    commands=[get(f'native-{arm}/rustc-command.json') for arm in 'AB']
    namespaces=[next(v for v in Path(c['args'][c['args'].index('--out-dir')+1]).parts if re.fullmatch(r'v2-[0-9a-f]{16}',v)) for c in commands]
    def replace(value,ns):return value.replace('/'+ns+'/', '/<cargo-compiler-namespace>/')
    x,y=commands
    assert x['env']==y['env'] and len(x['args'])==len(y['args'])
    for i,(s,t) in enumerate(zip(x['args'],y['args'])):
        if s!=t:
            normal=i and x['args'][i-1] in ('--out-dir','--extern','-L')
            typed_temp=a.mode=='test' and all(link_temp(k['args'],i,a.crate) for k in (x,y))
            assert normal or typed_temp
            assert replace(s,namespaces[0])==replace(t,namespaces[1])
    for stage,step in [('native','native'),('clean','clean'),('clean','test-current'),('clean','test-previous')]:
        maps=[{replace(k,ns):v for k,v in get(f'{stage}-{arm}/{step}.json')['identities'].items()} for arm,ns in zip('AB',namespaces)]
        assert maps[0]==maps[1]
    external=[{k:v['identity'] for k,v in get(f'native-{arm}/externs.json').items()} for arm in 'AB']
    assert external[0]==external[1]
    for state in ['current','previous']:
        assert gzip.decompress((out/f'clean-A/inventory-{state}.gz').read_bytes())==gzip.decompress((out/f'clean-B/inventory-{state}.gz').read_bytes())
    topologies={arm:get('native-'+arm+'/authentic-warm.json') for arm in 'AB'}
    for arm in 'AB':
        assert topologies[arm]==get('train-'+arm+'/state.json')['links']
    return {'namespaces':namespaces,'external':external[0],'genuine_warm':topologies}

try:
    for arm in 'AB':
        for stage in ['train','native','clean']:phase(arm,stage,stage+'-'+arm)
    put(out/'native-admission.json',admit())
    for arm in 'AB':phase(arm,'direct','direct-'+arm)
    rows=[]
    if not a.native_only:
        read=runpy.run_path(str(HERE/'cg_edges.py'))['read']
        for stage,n in [('wall',20),('profile',12)]:
            schedule=['AB','BA']*(n//2)
            random.Random(7222026+n).shuffle(schedule)
            for i,order in enumerate(schedule,1):
                values={}
                for arm in order:
                    label=f'{stage}-{i:02}-{arm}'; phase(arm,stage,label)
                    if stage=='wall':
                        values[arm]=get(label+'/wall.json')['elapsed_ns']/1e9
                    else:
                        m=get(label+'/measurement.json')
                        cg=out/label/'rustc.callgrind.gz'
                        raw=gzip.decompress(cg.read_bytes())
                        assert hashlib.sha256(raw).hexdigest()==m['raw_sha256']
                        names,edges,own=read(cg)
                        assert sum(own.values())==m['ir']
                        focus={ix for ix,s in names.items() if 'ReachabilityGraphBuilder>::build_graph' in s}
                        scoped=sum(cost for (caller,callee),(_,cost) in edges.items() if callee in focus and caller not in focus)
                        assert scoped>10_000 or a.mode=='test'
                        put(out/label/'graph.json',{'inclusive':scoped})
                        values[arm]=m['ir']
                rows.append({'phase':stage,'i':i,'order':order,'values':values})
                put(out/'samples.json',rows)
        sp.run([sys.executable,'-IB',HERE/'score.py',out/'samples.json'],check=True)
    (out/'launcher-exit').write_text('0'+chr(10))
    print('verified',out,flush=True)
except BaseException:
    (out/'launcher-exit').write_text('1'+chr(10))
    raise
