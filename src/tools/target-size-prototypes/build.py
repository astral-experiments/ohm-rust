"""Build one isolated, pinned compiler variant and preserve its build receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path('/tmp/ohm-target-size-prototypes')
parser = argparse.ArgumentParser()
parser.add_argument('name')
parser.add_argument('revision')
args = parser.parse_args()
source = ROOT / 'sources' / args.name
source.mkdir(parents=True, exist_ok=False)

def run(command, **kwargs):
    print('RUN', command, flush=True)
    return subprocess.run(command, check=True, **kwargs)

run(['tar', '-xzf', str(ROOT / 'inputs/rust-base.tar.gz'), '-C', str(source)])
run(['tar', '-xzf', str(ROOT / 'inputs/backtrace-base.tar.gz'),
     '-C', str(source / 'library/backtrace')])
patch = ROOT / 'inputs' / (args.name + '.patch')
if patch.exists():
    run(['git', 'apply', '--check', str(patch)], cwd=source)
    run(['git', 'apply', str(patch)], cwd=source)
(source / 'git-commit-info').write_text(
    args.revision + '\n' + args.revision[:11] + '\n2026-09-05\n')
build = Path('/tmp/codex-cargo-targets/ohm-rust/isolated/target-size-prototypes') / args.name
config = f'''change-id = "ignore"
[build]
build-dir = "{build}"
rustc = "{ROOT}/toolchain/bin/rustc"
cargo = "{ROOT}/toolchain/bin/cargo"
jobs = 24
docs = false
locked-deps = true
submodules = false
[llvm]
download-ci-llvm = false
link-shared = true
[rust]
llvm-tools = false
channel = "dev"
download-rustc = false
optimize = 3
debug-assertions = false
debug-logging = false
incremental = false
debuginfo-level = 1
codegen-units = 16
codegen-units-std = 1
lto = "thin-local"
jemalloc = false
mimalloc = true
omit-git-hash = false
[target.x86_64-unknown-linux-gnu]
llvm-config = "{ROOT}/llvm/bin/llvm-config"
'''
(source / 'bootstrap.toml').write_text(config)
env = dict(os.environ, CARGO_HOME='/tmp/codex-cargo-home', __CARGO_OHM_DEFAULTS='no')
env['LD_LIBRARY_PATH'] = str(ROOT / 'llvm/lib')
for key in ['CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'RUSTFLAGS', 'RUSTC_WRAPPER',
            'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN', 'RUSTC', 'RUSTDOC']:
    env.pop(key, None)
logs = ROOT / 'logs'
logs.mkdir(exist_ok=True)
receipt = {'name': args.name, 'revision': args.revision, 'source': str(source),
           'build': str(build), 'config': config, 'started': time.time(),
           'patch_sha256': (hashlib.sha256(patch.read_bytes()).hexdigest()
                            if patch.exists() else None)}
(logs / (args.name + '-status.json')).write_text(json.dumps(receipt, indent=2))
with (logs / (args.name + '-build.log')).open('w') as log:
    result = subprocess.run(['python3', 'x.py', 'build', '--stage', '1', '-j', '24',
        'compiler/rustc', 'library'],
        cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT)
receipt.update(returncode=result.returncode, finished=time.time())
(logs / (args.name + '-status.json')).write_text(json.dumps(receipt, indent=2))
print((logs / (args.name + '-build.log')).read_text()[-10000:], flush=True)
result.check_returncode()
