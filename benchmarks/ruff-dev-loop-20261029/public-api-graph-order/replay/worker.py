"""Run one isolated whole-compiler screening phase on a pair of real Ruff sources."""
import ast
import fcntl
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess as sp
import sys
import time

W = Path(__file__).resolve().parent
M = Path(os.environ['OHM_RERUN_WORK'])
DONOR = Path(os.environ['OHM_RERUN_RUFF'])
SOURCE = M / 'source'
ROOT, OUT, PHASE, LABEL, LANE = sys.argv[1:]
OUT = Path(OUT)
O = OUT / LABEL
O.mkdir()
SDK = Path(os.environ['OHM_STUDY_SDK'])
product_path = Path(os.environ['OHM_STUDY_PRODUCT']) / 'product.json'
ARM = {'current-722':'A','child-public-graph':'B'}[LANE]
MODE=(M/'workload').read_text().strip()
WORKLOAD={'check':['check'],'build':['build'],'test':['test','--lib','--no-run']}[MODE]
snapshot=runpy.run_path(str(W/'snapshot_links.py'))
inspect_archive=runpy.run_path(str(W/'member_archive_base.py'))['inspect']
RUFF = '93526150df078af7dd8fc4226cdcc469539d2001'
GIB = 1 << 30

def resource():
    memo = int(next(line for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')).split()[1]) * 1024
    values = [shutil.disk_usage(M).free, memo]
    assert all(a > int(os.environ[b]) for a,b in zip(values,('OHM_RERUN_MIN_DISK','OHM_RERUN_MIN_RAM'))), values
    return values

def shas(data):
    return hashlib.sha256(data).hexdigest()

def sha(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

def save(name, val):
    (O/name).write_text(json.dumps(val, sort_keys=True, indent=2)+'\n')

def zipped(path, data):
    with gzip.open(path, 'wb', compresslevel=4) as f:
        f.write(data)

def git(*args):
    return sp.check_output(['git','--no-optional-locks','-C',DONOR,*map(str,args)])

assert resource() and os.getuid()!=0 and M.stat().st_uid==os.getuid()
assert (M/'.rerun-owner').read_text()=='private rustc graph replication\n'
assert SOURCE.is_dir() and not SOURCE.is_symlink() and (M/'cargo-home').is_dir()
assert git('rev-parse','HEAD').decode().strip()==RUFF
assert ROOT in ('ruff_python_trivia','ruff_source_file')
if ROOT=='ruff_python_trivia':
    rel='crates/ruff_python_trivia/src/textwrap.rs'
    edit='83a3afc3156d73ae4296f3fa74ede28e4c72bfcd'
    current=git('show',f'{RUFF}:{rel}')
    patch=git('diff','-U0',edit+'^',edit,'--',rel).splitlines(keepends=True)
    hunks=[i for i,row in enumerate(patch) if row.startswith(b'@@')]
    assert len(hunks)==3
    hunk=patch[hunks[0]+1:hunks[1]]
    before=b''.join(row[1:] for row in hunk if row.startswith(b'-'))
    after=b''.join(row[1:] for row in hunk if row.startswith(b'+'))
    assert current.count(after)==1 and before!=after
    old=current.replace(after,before)
    expected=('28924b3e970299c42f667703bf8fc803f5ed17f28363d0821f7a571275dbd273', '5ac70d40126b21302e0f5ff21e9ef60c65f1bbcada53e2f0d92dd3d42b5ba0f8')
    tests=['textwrap::tests::adjust_indent']
else:
    rel='crates/ruff_source_file/src/line_index.rs'
    edit='88d851449d2345e11da991794f3e9bfbbd666719'
    current = git('show',f'{RUFF}:{rel}')
    patch=git('diff','-U0',edit+'^',edit,'--',rel).splitlines(keepends=True)
    hunks=[i for i,row in enumerate(patch) if row.startswith(b'@@')]; assert len(hunks)==2
    hunk=patch[hunks[0]+1:hunks[1]]
    before=b''.join(r[1:] for r in hunk if r.startswith(b'-'))
    after=b''.join(r[1:] for r in hunk if r.startswith(b'+'))
    assert current.count(after)==1 and before!=after
    old=current.replace(after,before)
    expected=('0ebaad70053510b8fcddb8bb26d05517d997b5622fc0933750fe1fd08b176168','9e67f88ad0255bff8b4dfeca2cc536aa13c9052b0b531f6bdd7a31d0b5fb6d64')
    tests=['line_end_exclusive_handles_different_line_endings']
assert (shas(current),shas(old))==expected
assert (DONOR/rel).read_bytes()==git('show',f'{RUFF}:{rel}')==current

env={k:os.environ[k] for k in ('HOME','USER','LOGNAME','RUSTUP_HOME')}
env.update(PATH=os.environ['OHM_RERUN_PROXY_DIR']+':/usr/bin:/bin',CARGO_HOME=str(M/'cargo-home'),CARGO_TARGET_DIR=str(M/'products'),CARGO_BUILD_BUILD_DIR=str(M/'shared'),TMPDIR=str(M/'tmp'),CARGO_NET_OFFLINE='true',CARGO_CACHE_AUTO_CLEAN_FREQUENCY='never',CARGO_TERM_COLOR='never',RUSTUP_AUTO_INSTALL='0',TZ='UTC',LC_ALL='C.UTF-8')
assert Path(sp.check_output(['rustc','+ohm','--print','sysroot'],env=env).decode().strip()).samefile(SDK)
version=sp.check_output(['rustc','+ohm','-vV'],env=env).decode()
driver=next((SDK/'lib').glob('librustc_driver-*.so'))
wild=SDK/'lib/rustlib/x86_64-unknown-linux-gnu/bin/wild-ld/ld.bfd'
fixed=[sha(SDK/'bin/cargo'),sha(wild)]
assert fixed==['789c2f116732ed642565d0f92c500838f8cd53664fbe60f205a5621c43cbecf1','0d6335808b647d499f3d74993b602e64ff0625206c809bf4913c487eea380cd5']
product=json.loads(product_path.read_bytes())
stamps = {'original': ('722b9412',), 'fixed-stack': ('9f8db07b','569aefbe')}
assert any(stamp in version for stamp in stamps[product['source_pair']]), version
bind=dict(ruff=RUFF,edit=edit,sources=expected,cargo=fixed[0],wild=fixed[1],rustc=sha(SDK/'bin/rustc'),driver=sha(driver),version=version,product=sha(product_path),resources=resource(),root=ROOT,phase=PHASE,lane=LANE)
assert bind['rustc']==product['rustc']
assert bind['driver']==product['driver']
bind.update(commit=product['commit'],mode=MODE)
save('binding.json',bind)

SEED_ARM = 'A' if PHASE in ('profile','direct') else ARM
SEED = M/('seed-'+SEED_ARM)
STATE = M/('state-'+SEED_ARM+'.json')

def setstate(value, *, fixed=False):
    (SOURCE/rel).write_bytes(value)
    if fixed:
        stamp=json.loads(STATE.read_text())['edit_mtime_ns']
        os.utime(SOURCE/rel,ns=(stamp,stamp))
    assert (DONOR/rel).read_bytes()==current

def inventory(path):
    digest=hashlib.sha256(); n=0
    for p in sorted(path.rglob('*')):
        s=p.lstat()
        row=[p.relative_to(path).as_posix(),s.st_mode,s.st_mtime_ns]
        if p.is_symlink(): row.append(os.readlink(p))
        elif p.is_file(): row.append(sha(p))
        digest.update(json.dumps(row).encode()+b'\n'); n+=1
    return [n,digest.hexdigest()]

def reset(empty=False):
    for kind in ('products','shared'):
        directory=M/kind
        assert directory.parent==M and (M/'.rerun-owner').is_file() and not directory.is_symlink()
        if directory.exists(): shutil.rmtree(directory)
    if empty:
        for kind in ('products','shared'):
            (M/kind).mkdir()
    else:
        state=json.loads(STATE.read_text())
        assert snapshot['copy'](SEED,M)==state['links']
        for kind in ('products','shared'):
            assert inventory(M/kind)==state['seed'][kind]

def identity(path):
    path=Path(path)
    if path.suffix!='.rlib':
        return sha(path)
    archive=inspect_archive(path)
    objects=archive['objects']
    names=[re.fullmatch(r'(.+)\.([0-9a-z]{7})\.rcgu\.o',name) for name,_,_ in objects]
    assert all(names) and len({match[2] for match in names})==1
    token=b'.'+names[0][2].encode()+b'.rcgu.o'
    raw=path.read_bytes()
    # The parser proves these are the ordered member names and their exact rmeta-link
    # references. Reject incidental occurrences in object code or elsewhere.
    assert raw.count(token)==2*len(names) and archive['rmeta_link']['rust_object_order_matches_archive']
    return shas(raw.replace(token,b'.0000000.rcgu.o'))

def cargo(label, command, trace=False):
    resource()
    td=M/'traces'/LABEL/label
    prefix=[]
    if trace:
        td.mkdir(parents=True)
        prefix=['/usr/bin/strace','-ff','-qq','-v','-s','131072','-e','trace=execve,chdir','-o',td/'task']
    args=['cargo','+ohm',*command,'--offline','--locked','-p',ROOT,'-j','4','--message-format=json']
    start=time.monotonic_ns()
    p=sp.run([*prefix,*args],cwd=SOURCE,env=env,capture_output=True,timeout=480)
    elapsed=time.monotonic_ns()-start
    zipped(O/(label+'.jsonl.gz'),p.stdout); zipped(O/(label+'.stderr.gz'),p.stderr)
    if p.returncode: raise RuntimeError((args,p.returncode,p.stderr[-3000:]))
    rows=[json.loads(line) for line in p.stdout.splitlines()]
    assert rows[-1]=={'reason':'build-finished','success':True}
    arts=[a for a in rows if a['reason']=='compiler-artifact']
    roots=[a for a in arts if a['target']['name']==ROOT]; assert len(roots)==1
    info=dict(args=args,rc=p.returncode,typed=sorted([[a['target']['name'],a['target']['kind'],a['fresh'],a['features']] for a in arts]),outputs={str(Path(n).relative_to(M)):sha(n) for n in roots[0]['filenames']},executable=roots[0]['executable'])
    info['elapsed_ns']=elapsed
    info['identities']={str(Path(n).relative_to(M)):identity(n) for n in roots[0]['filenames']}
    save(label+'.json',info)
    if trace:
        actual=[]; stock=0; root_commands=[]; raw=[]
        for path in sorted(td.glob('task.*')):
            for line in path.read_text().splitlines():
                if not(line.startswith('execve(') and line.endswith(') = 0')): continue
                binary,argv,pairs=ast.literal_eval('('+line[7:-len(') = 0')]+')')
                raw.append((path.name,line))
                try: is_rustc=Path(binary).samefile(SDK/'bin/rustc'); is_wild=Path(binary).samefile(wild)
                except FileNotFoundError: is_rustc=is_wild=False
                stock+=is_wild
                if is_rustc and '--crate-name' in argv and '--out-dir' in argv:
                    name=argv[argv.index('--crate-name')+1]; actual.append([name,argv]);
                    if name==ROOT: root_commands.append({'args':argv,'env':dict(s.split('=',1) for s in pairs)})
        zipped(O/(label+'.exec.gz'),json.dumps(raw).encode())
        save(label+'.roster.json',{'rustc':actual,'wild':stock})
        if label=='native':
            assert len(actual)==1 and len(root_commands)==1 and actual[0][0]==ROOT and stock==int(MODE=='test'),(len(actual),stock,MODE)
            assert [r[0] for r in info['typed'] if not r[2]]==[ROOT],info['typed']
            save('rustc-command.json',root_commands[0])
            argv=root_commands[0]['args']
            inputs={}
            for i,arg in enumerate(argv):
                if arg!='--extern' or '=' not in argv[i+1]: continue
                name,path=argv[i+1].split('=',1)
                raw=sha(path)
                try: canonical=identity(path)
                except (AssertionError,ValueError): canonical=raw
                # Cargo test can pass an rlib and rmeta with the same extern name.
                inputs[f'{i}:{name}']={'raw':raw,'identity':canonical}
            save('externs.json',inputs)
        if label=='repeat': assert not actual and not stock
    return info

def make_pipe(flags):
    matched=re.search(r'--jobserver-(?:fds|auth)=(\d+),(\d+)',flags)
    if not matched: return
    a,b=map(int,matched.groups()); r,w=os.pipe()
    x=fcntl.fcntl(r,fcntl.F_DUPFD_CLOEXEC,max(a,b)+4); y=fcntl.fcntl(w,fcntl.F_DUPFD_CLOEXEC,max(a,b)+4)
    os.close(r); os.close(w); os.dup2(x,a,inheritable=True); os.dup2(y,b,inheritable=True); os.close(x); os.close(y); os.write(b,b'+++')

if PHASE=='train':
    reset(empty=True); setstate(old); train=cargo('previous',WORKLOAD)
    SEED.mkdir()
    state={'seed':{},'links':snapshot['copy'](M,SEED)}
    for k in ('products','shared'):
        state['seed'][k]=inventory(M/k); assert inventory(SEED/k)==state['seed'][k]
    state['edit_mtime_ns']=time.time_ns(); STATE.write_text(json.dumps(state)); save('state.json',state)
elif PHASE=='native':
    # Capture ordinary rustc, link and product receipts on the actual still-live previous output.
    # The only operation since training was to copy an independent immutable seed.
    state=json.loads(STATE.read_text())
    physical,live=snapshot['describe'](M)
    seed_inodes,stored=snapshot['describe'](SEED)
    assert physical.isdisjoint(seed_inodes) and live==stored==state['links']
    for kind in ('products','shared'):
        assert inventory(M/kind)==state['seed'][kind]
    save('authentic-warm.json',live)
    setstate(current,fixed=True)
    normal=cargo('native',WORKLOAD,True); repeat=cargo('repeat',WORKLOAD,True)
    assert all(r[2] for r in repeat['typed']) and normal['outputs']==repeat['outputs']
    if ARM=='B':
        prior=json.loads((OUT/'native-A/native.json').read_text())
        assert prior['typed']==normal['typed'] and prior['args']==normal['args']
elif PHASE=='wall':
    reset(); setstate(current,fixed=True)
    normal=cargo('wall',WORKLOAD)
    native=json.loads((OUT/('native-'+ARM)/'native.json').read_bytes())
    assert native['typed']==normal['typed'] and native['identities']==normal['identities']
elif PHASE in ('profile','direct'):
    reset(); setstate(current,fixed=True); native=OUT/'native-A'
    command=json.loads((native/'rustc-command.json').read_text()); args,baseenv=command['args'],command['env']
    prof=M/'profiles'/LABEL; prof.mkdir(parents=True)
    prefix=['/usr/bin/valgrind','--quiet','--tool=callgrind','--trace-children=no','--log-file='+str(prof/'vg.%p'),'--callgrind-out-file='+str(prof/'cg.%p')]
    if PHASE=='direct': prefix=[]
    p=sp.run([*prefix,*args],cwd=SOURCE,env=baseenv,capture_output=True,timeout=420,preexec_fn=lambda:make_pipe(baseenv.get('CARGO_MAKEFLAGS','')),close_fds=False)
    zipped(O/'rustc.stderr.gz',p.stderr); zipped(O/'rustc.stdout.gz',p.stdout)
    for file in prof.glob('vg.*'): zipped(O/(file.name+'.gz'),file.read_bytes())
    assert p.returncode==0,(p.returncode,p.stderr[-500:])
    gold=json.loads((native/'native.json').read_text())['identities']; actual={};raw_outputs={}
    out_dir=Path(args[args.index('--out-dir')+1]); extra=next(a.split('=',1)[1] for a in args if a.startswith('extra-filename='))
    for f in gold:
        name=Path(f).name; physical=out_dir/name
        if not physical.is_file():
            ext=Path(name).suffix
            assert ext in ('.rmeta','.rlib') and name=='lib'+ROOT+ext,name
            physical=out_dir/('lib'+ROOT+extra+ext)
        actual[f]=identity(physical)
        raw_outputs[f]=sha(physical)
    assert actual==gold,(actual,gold)
    hits=[]
    for file in prof.glob('cg.*'):
        data=file.read_bytes()
        if ('--crate-name '+ROOT+' ').encode() not in data[:18000]: continue
        assert len(data)<45<<20 and b'\nevents: Ir\n' in data
        totals=re.findall(rb'^totals: (\d+)\s*$',data,re.M); assert len(totals)==1 and int(totals[0])>10_000_000
        hits.append({'raw_sha256':shas(data),'ir':int(totals[0]),'bytes':len(data),'outputs':actual,'raw_outputs':raw_outputs})
        zipped(O/'rustc.callgrind.gz',data)
    if PHASE=='profile':
        assert len(hits)==1,hits
        save('measurement.json',hits[0])
    else: save('native-replay.json',{'outputs':actual,'exit':p.returncode})
elif PHASE=='clean':
    reset(empty=True); setstate(current,fixed=True); clean=cargo('clean',WORKLOAD)
    golden=json.loads((OUT/('native-'+ARM)/'native.json').read_text())['identities']; assert clean['identities']==golden,(clean['identities'],golden)
    for state,data in (('current',current),('previous',old)):
        setstate(data); result=cargo('test-'+state,['test','--lib','--no-run'],True); exe=result['executable']; assert exe
        inv=sp.check_output([exe,'--list'],cwd=SOURCE,env=env); zipped(O/('inventory-'+state+'.gz'),inv)
        for suffix in tests:
            found=[line[:-6] for line in inv.decode().splitlines() if line.endswith(suffix+': test')]; assert len(found)==1,found
            check=sp.run([exe,'--exact',found[0]],cwd=SOURCE,env=env,capture_output=True)
            expect=101 if state=='previous' else 0
            save(state+'-'+suffix+'.json',{'exit':check.returncode,'name':found[0],'stdout':check.stdout.decode(),'stderr':check.stderr.decode(),'binary':sha(exe),'inventory':shas(inv)})
            assert check.returncode==expect and b'running 1 test' in check.stdout,(state,check.returncode,expect)
else: raise ValueError(PHASE)
save('terminal.json',{'exit':0,'resource':resource(),'source_lower':sha(DONOR/rel)})
print(ROOT,LABEL,'complete',flush=True)
