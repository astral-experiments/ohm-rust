"""Qualify declared native inputs and the uv derive edit on the exact release compiler."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time

suite = Path(__file__).resolve().parent
workspace = Path(os.environ['GITHUB_WORKSPACE'])
source = workspace/'source'
project = workspace/'uv'
root = Path(os.environ['RUNNER_TEMP'])/'ohm21-qualification'
evidence = root/'evidence'
control = 'd6b5f336df2255a39f52a7893d52b53c5182a49b'
candidate = 'c79998a5cd143c445a4b2528f20953b176a65c5d'
signed_rust = '7e71d3150938ff8ed82aca03416439d7e5b87ef3'
signed_cargo = '2d44bd4aa68b9ba32cacb5a384ecc18fa5e7553c'
host = 'aarch64-apple-darwin' if platform.system()=='Darwin' else 'x86_64-unknown-linux-gnu'
archive_hashes = {'aarch64-apple-darwin':'7cd292583785c25a315f67bf85c8f028a15ccb0be5461ac39ce6e46720807bb8','x86_64-unknown-linux-gnu':'edb64a2ee39b2f2951614f98d53ec4e4197f55149ff794c911bd81bc7f870b15'}

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for part in iter(lambda:f.read(4*1024*1024),b''):h.update(part)
    return h.hexdigest()

def saved(name, data):
    (evidence/name).write_text(json.dumps(data,indent=2)+'\n')

def output(*args,cwd=source,env=None):
    return subprocess.check_output(args,cwd=cwd,env=env,text=True).strip()

def logged(label,args,cwd,env=None):
    print(label,flush=True)
    log=evidence/(label+'.log')
    with log.open('xb') as f:
        start=time.monotonic()
        run=subprocess.run(args,cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT)
        elapsed=time.monotonic()-start
    if run.returncode:
        with log.open('rb') as f:
            f.seek(max(0,log.stat().st_size-3500))
            print(f.read().decode(errors='replace'),flush=True)
    run.check_returncode()
    return elapsed

def clear_fixture(path):
    assert path.parent==root and path.name in ('live','work-candidate','work-parent','work-signed')
    if path.exists():shutil.rmtree(path)

def install_signed():
    version='ohm-1.99.0-20'
    name=f'{version}-{host}.tar.gz'
    archive=root/name
    url=f'https://labs.astral.sh/ohm/releases/{version}/{name}'
    logged('download-signed',['curl','-fLsS','--retry','4','-o',archive,url],root)
    assert digest(archive)==archive_hashes[host]
    extract=root/'release-files';extract.mkdir()
    logged('extract-signed',['tar','-xzf',archive,'-C',extract],root)
    installer,=extract.glob('*/install.sh')
    sdk=root/'signed'
    logged('install-signed',['sh',installer,'--prefix='+str(sdk),'--disable-ldconfig'],root)
    rv=output(sdk/'bin/rustc','-vV');cv=output(sdk/'bin/cargo','-vV')
    assert 'commit-hash: '+signed_rust in rv and 'commit-hash: '+signed_cargo in cv
    saved('signed.json',{'archive':url,'sha256':digest(archive),'rustc':rv,'cargo':cv})
    return sdk

def source_sdk(lane,sha,signed):
    logged('checkout-'+lane,['git','checkout','--detach',sha],source)
    assert output('git','rev-parse','HEAD')==sha
    assert output('git','rev-parse','HEAD:src/tools/cargo')==signed_cargo
    assert not output('git','status','--porcelain','--untracked-files=no','--ignore-submodules=all')
    build=root/'bootstrap'
    build_args=[sys.executable,'x.py','--ci','false','--set','build.submodules=false','--set','rust.download-rustc=false','--set','rust.omit-git-hash=false','--set','rust.debug-assertions=false','--set','rust.lto=off','build','--build-dir',build,'--stage','1','compiler/rustc','library']
    logged('bootstrap-'+lane,build_args,source)
    stage=build/host/'stage1'
    sdk=root/('sdk-'+lane)
    shutil.copytree(stage,sdk,symlinks=False)
    toolbin=Path('lib/rustlib')/host/'bin'
    (sdk/toolbin).mkdir(parents=True,exist_ok=True)
    for tool in ('rust-lld','gcc-ld'):
        origin=signed/toolbin/tool; target=sdk/toolbin/tool
        if origin.exists() and not target.exists():
            if origin.is_dir():shutil.copytree(origin,target)
            else:shutil.copy2(origin,target)
    if host.startswith('x86_64'):
        rel=Path('lib/rustlib')/host/'bin/wild-ld'
        if not (sdk/rel).exists():shutil.copytree(signed/rel,sdk/rel)
        assert digest(sdk/rel/'ld.bfd')==digest(signed/rel/'ld.bfd')
    rustc=sdk/'bin/rustc'
    version=output(rustc,'-vV')
    assert 'commit-hash: '+sha in version,(sha,version)
    assert Path(output(rustc,'--print','sysroot')).resolve()==sdk.resolve()
    saved(lane+'-compiler.json',{'commit':sha,'version':version,'rustc_sha256':digest(rustc),'source_tree':output('git','rev-parse','HEAD^{tree}'),'build_argv':list(map(str,build_args))})
    return sdk

def run_native(signed,sdk):
    cargo_home=root/'native-cargo';cargo_home.mkdir()
    logged('native-relinks',[sys.executable,suite/'declared-native-probe.py','--cargo',signed/'bin/cargo','--rustc',sdk/'bin/rustc','--work',root/'native-probe','--target',root/'native-target','--cargo-home',cargo_home],root)
    shutil.copy2(root/'native-probe/results.json',evidence/'native-relinks.json')

def paired(signed,sdks):
    fixture=json.loads((suite/'uv-input.json').read_bytes())
    patch=root/'uv-21076.patch'
    patch.write_text(fixture.pop('patch'))
    assert digest(patch)==fixture['patch_sha256'] and output('git','rev-parse','HEAD',cwd=project)==fixture['revision']
    files=list(fixture['files'])
    assert all(digest(project/n)==fixture['files'][n]['before'] for n in files)
    saved('uv-source.json',fixture)
    home=root/'cargo-home';home.mkdir()
    cargo=signed/'bin/cargo'
    base={k:os.environ[k] for k in ('PATH','HOME','USER','LANG','LC_ALL','SSL_CERT_FILE','XDG_RUNTIME_DIR') if k in os.environ}
    base.update(CARGO_HOME=str(home),CARGO_INCREMENTAL='1',CARGO_TERM_COLOR='never',CARGO_CACHE_AUTO_CLEAN_FREQUENCY='never')
    logged('uv-fetch',[cargo,'fetch','--locked','--target',host],project,base|{'RUSTC':str(signed/'bin/rustc')})
    lanes=list(sdks); live=root/'live'; budgets={'cli':['-p','uv-cli'],'binary':['-p','uv','--bin','uv']}
    def restoresource():
        subprocess.run(['git','restore','--source=HEAD','--worktree','--',*files],cwd=project,check=True)
        assert all(digest(project/n)==fixture['files'][n]['before'] for n in files)
    def env(lane):
        return base|{'RUSTC':str(sdks[lane]/'bin/rustc'),'RUSTDOC':str(signed/'bin/rustdoc'),'CARGO_TARGET_DIR':str(live/'target'),'CARGO_BUILD_BUILD_DIR':str(live/'build'),'CARGO_NET_OFFLINE':'true'}
    def invocation(scope):return [cargo,'check','--locked','--offline','-j8',*budgets[scope]]
    for lane in lanes:
        restoresource()
        live.mkdir()
        logged('warm-'+lane,invocation('binary')+['-vv'],project,env(lane))
        logged('warm-cli-'+lane,invocation('cli')+['-vv'],project,env(lane))
        incremental=[]
        for label in ('warm-','warm-cli-'):
            with (evidence/(label+lane+'.log')).open(errors='replace') as f:
                incremental.extend(l.strip() for l in f if '--crate-name uv_cli ' in l and 'incremental=' in l)
        assert incremental,lane
        saved('incremental-'+lane+'.json',{'lane':lane,'CI':env(lane).get('CI'),'uv_cli_rustc_was_incremental':True,'command_sha256':hashlib.sha256(incremental[0].encode()).hexdigest()})
        logged('warm-repeat-'+lane,invocation('binary'),project,env(lane))
        live.rename(root/('work-'+lane))
    observations=[]
    orders=list(itertools.permutations(lanes))
    for i,order in enumerate(orders*2):
        for scope in (('cli','binary') if i%2==0 else ('binary','cli')):
            for lane in order:
                restoresource()
                clear_fixture(live)
                subprocess.run(['cp','-a','--reflink=auto',root/('work-'+lane),live],check=True)
                subprocess.run(['git','apply',patch],cwd=project,check=True)
                assert all(digest(project/n)==fixture['files'][n]['after'] for n in files)
                elapsed=logged(f'block-{i+1:02}-{scope}-{lane}',invocation(scope),project,env(lane))
                observations.append({'block':i+1,'scope':scope,'lane':lane,'order':list(order),'seconds':elapsed})
                saved('samples.json',observations)
    summary={}
    for scope in budgets:
        results={lane: [v['seconds'] for v in observations if v['scope']==scope and v['lane']==lane] for lane in lanes}
        assert all(len(times)==12 for times in results.values())
        summary[scope]={'medians':{l:statistics.median(v) for l,v in results.items()},'paired_parent_over_candidate':statistics.median(a/b for a,b in zip(results['parent'],results['candidate'])),'wins_vs_parent':sum(b<a for a,b in zip(results['parent'],results['candidate']))}
    saved('uv-summary.json',summary)
    print(json.dumps(summary),flush=True)

root.mkdir();evidence.mkdir()
assert host in archive_hashes and platform.machine() in ('x86_64','arm64','aarch64')
signed=install_signed()
subprocess.run(['git','submodule','update','--init','--depth','1','library/backtrace'],cwd=source,check=True)
sdks={'signed':signed}
if host.startswith('x86_64'):sdks['parent']=source_sdk('parent',control,signed)
sdks['candidate']=source_sdk('candidate',candidate,signed)
run_native(signed,sdks['candidate'])
if host.startswith('x86_64'):paired(signed,sdks)
saved('complete.json',{'host':host,'rust':candidate,'cargo':signed_cargo,'declared_native_links':True,'uv_paired':host.startswith('x86_64')})
