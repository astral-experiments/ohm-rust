"""Run one isolated whole-compiler screening phase on a pair of real Ruff sources."""
import ast
import fcntl
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
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
ARM = {'current-722':'A','child-decoder':'B'}[LANE]
RUFF = '93526150df078af7dd8fc4226cdcc469539d2001'
GIB = 1 << 30

def resource():
    memo = int(next(line for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')).split()[1]) * 1024
    values = [shutil.disk_usage(M).free,memo]
    assert values[0]>int(os.environ["OHM_RERUN_MIN_DISK"]) and values[1]>int(os.environ["OHM_RERUN_MIN_RAM"]),values
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

assert resource() and os.getuid()!=0
assert (M/'.rerun-owner').read_text()=='private rustc decoder replication\n' and SOURCE.is_dir() and (M/'cargo-home').is_dir()
assert git('rev-parse','HEAD').decode().strip()==RUFF
assert ROOT=='ruff_text_size'
rel='crates/ruff_text_size/src/range.rs'
edit='61a49c89eb809a238b01616b0a50a0f785702891'
current=git('show',f'{RUFF}:{rel}')
patch=git('diff','-U0',edit+'^',edit,'--',rel).splitlines(keepends=True)
hunks=[i for i,row in enumerate(patch) if row.startswith(b'@@')];assert len(hunks)==1
hunk=patch[hunks[0]+1:];assert not any(row.startswith(b'-') for row in hunk)
added=b''.join(row[1:] for row in hunk if row.startswith(b'+'));assert len(added)==449 and current.count(added)==1
old=current.replace(added,b'')
expected=('f61f9d66ec9dcc29bac8067dd4d5910db6b7e52009ef7cc976ac0478c6800c52','097615fdf261460b1c30900d535bb5bde4ab709a402d4850b49820a6923b0771')
assert (shas(current),shas(old))==expected
assert (DONOR/rel).read_bytes()==git('show',f'{RUFF}:{rel}')==current

env={k:os.environ[k] for k in ('HOME','USER','LOGNAME','RUSTUP_HOME')}
env.update(PATH=os.environ['OHM_RERUN_PROXY_DIR']+':/usr/bin:/bin',CARGO_HOME=str(M/'cargo-home'),CARGO_TARGET_DIR=str(M/'products'),CARGO_BUILD_BUILD_DIR=str(M/'shared'),TMPDIR=str(M/'tmp'),CARGO_NET_OFFLINE='true',CARGO_CACHE_AUTO_CLEAN_FREQUENCY='never',CARGO_TERM_COLOR='never',RUSTUP_AUTO_INSTALL='0',TZ='UTC',LC_ALL='C.UTF-8')
assert Path(sp.check_output(['rustc','+ohm','--print','sysroot'],env=env).decode().strip()).samefile(SDK)
version=sp.check_output(['rustc','+ohm','-vV'],env=env).decode()
driver=next((SDK/'lib').glob('librustc_driver-*.so'))
wild=SDK/'lib/rustlib/x86_64-unknown-linux-gnu/bin/wild-ld/ld.bfd'
fixed=[sha(SDK/'bin/cargo'),sha(wild)]
assert fixed==json.loads(product_path.read_bytes())['common']
assert '722b9412' in version, version
bind=dict(ruff=RUFF,edit=edit,sources=expected,cargo=fixed[0],wild=fixed[1],rustc=sha(SDK/'bin/rustc'),driver=sha(driver),version=version,product=sha(product_path),resources=resource(),root=ROOT,phase=PHASE,lane=LANE)
assert bind['driver']==json.loads(product_path.read_bytes())['driver']
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
        assert directory.parent==M and (M/'.rerun-owner').read_text()=='private rustc decoder replication\n' and not directory.is_symlink()
        if directory.exists(): shutil.rmtree(directory)
        if empty: directory.mkdir()
        else:
            shutil.copytree(SEED/kind,directory,symlinks=True)
            expected_seed=json.loads(STATE.read_text())['seed'][kind]
            assert inventory(directory)==expected_seed

def cargo(label, command, trace=False):
    resource()
    td=M/'traces'/LABEL/label
    prefix=[]
    if trace:
        td.mkdir(parents=True)
        prefix=['/usr/bin/strace','-ff','-qq','-v','-s','131072','-e','trace=execve,chdir','-o',td/'task']
    args=['cargo','+ohm',*command,'--offline','--locked','-p',ROOT,'-j','4','--message-format=json']
    p=sp.run([*prefix,*args],cwd=SOURCE,env=env,capture_output=True,timeout=480)
    zipped(O/(label+'.jsonl.gz'),p.stdout); zipped(O/(label+'.stderr.gz'),p.stderr)
    if p.returncode: raise RuntimeError((args,p.returncode,p.stderr[-3000:]))
    rows=[json.loads(line) for line in p.stdout.splitlines()]
    assert rows[-1]=={'reason':'build-finished','success':True}
    arts=[a for a in rows if a['reason']=='compiler-artifact']
    roots=[a for a in arts if a['target']['name']==ROOT]; assert len(roots)==1
    info=dict(args=args,rc=p.returncode,typed=sorted([[a['target']['name'],a['target']['kind'],a['fresh'],a['features']] for a in arts]),outputs={str(Path(n).relative_to(M)):sha(n) for n in roots[0]['filenames']},executable=roots[0]['executable'])
    save(label+'.json',info)
    if label in ('previous','native','repeat','clean'):
        for item,digest in info['outputs'].items():
            f=M/item
            if f.suffix!='.rmeta':continue
            data=f.read_bytes();assert shas(data)==digest
            zipped(O/(label+'.root.rmeta.gz'),data)

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
            assert len(actual)==1 and len(root_commands)==1 and actual[0][0]==ROOT and stock==0,(len(actual),stock)
            assert [r[0] for r in info['typed'] if not r[2]]==[ROOT],info['typed']
            save('rustc-command.json',root_commands[0])
        if label=='repeat': assert not actual and not stock
    return info

def make_pipe(flags):
    matched=re.search(r'--jobserver-(?:fds|auth)=(\d+),(\d+)',flags)
    if not matched: return
    a,b=map(int,matched.groups()); r,w=os.pipe()
    x=fcntl.fcntl(r,fcntl.F_DUPFD_CLOEXEC,max(a,b)+4); y=fcntl.fcntl(w,fcntl.F_DUPFD_CLOEXEC,max(a,b)+4)
    os.close(r); os.close(w); os.dup2(x,a,inheritable=True); os.dup2(y,b,inheritable=True); os.close(x); os.close(y); os.write(b,b'+++')

if PHASE=='train':
    reset(empty=True); setstate(old); train=cargo('previous',['check','--lib'])
    SEED.mkdir()
    state={'seed':{}}
    for k in ('products','shared'):
        shutil.copytree(M/k,SEED/k,symlinks=True)
        state['seed'][k]=inventory(M/k); assert inventory(SEED/k)==state['seed'][k]
    state['edit_mtime_ns']=time.time_ns(); STATE.write_text(json.dumps(state)); save('state.json',state)
elif PHASE=='native':
    reset(); setstate(current,fixed=True)
    normal=cargo('native',['check','--lib'],True); repeat=cargo('repeat',['check','--lib'],True)
    assert all(r[2] for r in repeat['typed']) and normal['outputs']==repeat['outputs']
    if ARM=='B':
        prior=json.loads((OUT/'native-A/native.json').read_text())
        assert prior['typed']==normal['typed'] and prior['args']==normal['args']
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
    gold=json.loads((native/'native.json').read_text())['outputs']; actual={}
    out_dir=Path(args[args.index('--out-dir')+1]); extra=next(a.split('=',1)[1] for a in args if a.startswith('extra-filename='))
    for f in gold:
        name=Path(f).name; physical=out_dir/name
        if not physical.is_file():
            assert name=='lib'+ROOT+'.rmeta',name
            physical=out_dir/('lib'+ROOT+extra+'.rmeta')
        actual[f]=sha(physical)
    assert actual==gold,(actual,gold)
    hits=[]
    for file in prof.glob('cg.*'):
        data=file.read_bytes()
        if ('--crate-name '+ROOT+' ').encode() not in data[:18000]: continue
        assert len(data)<250<<20 and b'\nevents: Ir\n' in data
        totals=re.findall(rb'^totals: (\d+)\s*$',data,re.M); assert len(totals)==1 and int(totals[0])>10_000_000
        hits.append({'raw_sha256':shas(data),'ir':int(totals[0]),'bytes':len(data),'outputs':actual})
        zipped(O/'rustc.callgrind.gz',data)
    if PHASE=='profile':
        assert len(hits)==1,hits
        save('measurement.json',hits[0])
    else: save('native-replay.json',{'outputs':actual,'exit':p.returncode})
elif PHASE=='clean':
    reset(empty=True); setstate(current,fixed=True); clean=cargo('clean',['check','--lib'])
    golden=json.loads((OUT/('native-'+ARM)/'native.json').read_text())['outputs']; assert clean['outputs']==golden,(clean['outputs'],golden)
    for state,data in (('current',current),('previous',old)):
        setstate(data); result=cargo('build-'+state,['build','--lib'],True)
        rs=[json.loads(t) for t in gzip.decompress((O/('build-'+state+'.jsonl.gz')).read_bytes()).splitlines()]
        arts=[x for x in rs if x['reason']=='compiler-artifact'];scripts=[x for x in rs if x['reason']=='build-script-executed']
        lib,=[Path(y) for x in arts if x['target']['name']==ROOT for y in x['filenames'] if y.endswith('.rlib')]
        dirs=sorted({str(Path(z).parent) for x in arts for z in x['filenames'] if Path(z).suffix in ('.rlib','.rmeta','.so')})
        native=sorted({z for x in scripts for z in x['linked_paths']})
        for kind in ('external','legacy'):
            exe=M/('actual-textsize-consumer-'+state+'-'+kind)
            path=W/(kind+'.rs')
            args=['rustc','+ohm','--edition=2024',path,'--error-format=json','--extern',ROOT+'='+str(lib)]
            for directory in dirs:args += ['-L','dependency='+directory]
            for directory in native:args += ['-L',directory]
            args+=['-o',exe]
            built=sp.run(args,cwd=SOURCE,env=env,capture_output=True,timeout=120)
            zipped(O/('consumer-'+state+'-'+kind+'.compile-stderr.gz'),built.stderr)
            diagnostics=[json.loads(line) for line in built.stderr.splitlines()]
            named=[diag for diag in diagnostics if diag.get('code') is not None]
            record={'compiler_exit':built.returncode,'codes':[x['code']['code'] for x in named],'diagnostics':[x['message'] for x in named],'source':sha(SOURCE/rel),'rlib':sha(lib),'caller':sha(path),'argv':list(map(str,args))}
            if state=='previous' and kind=='external':
                assert built.returncode==1 and not built.stdout and record['codes']==['E0599'] and 'to_std_range' in record['diagnostics'][0] and any(s['line_start']==5 and s['is_primary'] for s in named[0]['spans']),record
                assert not exe.exists()
            else:
                assert built.returncode==0 and not built.stderr and not built.stdout,record
                ran=sp.run([exe],cwd=SOURCE,env=env,capture_output=True,timeout=8)
                record.update({'exit':ran.returncode,'stdout':ran.stdout.decode(),'stderr':ran.stderr.decode(),'binary':sha(exe)})
                assert ran.returncode==0 and ran.stdout=='1..3:é\n'.encode() and not ran.stderr,record
            save('consumer-'+state+'-'+kind+'.json',record)
else: raise ValueError(PHASE)
save('terminal.json',{'exit':0,'resource':resource(),'source_lower':sha(DONOR/rel)})
print(ROOT,LABEL,'complete',flush=True)
