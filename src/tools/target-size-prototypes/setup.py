"""Prepare pinned compiler inputs on the dedicated target-size devbox."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

ROOT = Path('/tmp/ohm-target-size-prototypes')

def run(args, **kwargs):
    print('RUN', args, flush=True)
    return subprocess.run(args, check=True, **kwargs)

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

manifest = json.loads((ROOT / 'inputs.json').read_text())
for name, expected in manifest.items():
    path = ROOT / 'inputs' / name
    assert sha(path) == expected['sha256'], name
    assert path.stat().st_size == expected['bytes'], name

run(['sudo', '-n', 'apt-get', 'update', '-qq'])
run(['sudo', '-n', 'apt-get', 'install', '-y', 'build-essential', 'cmake',
     'ninja-build', 'pkg-config', 'libssl-dev', 'valgrind', 'clang', 'lld',
     'btrfs-progs'])

release = ROOT / 'release'
release.mkdir(exist_ok=True)
run(['tar', '-xzf', str(ROOT / 'inputs/ohm-1.98.0-9-x86_64-unknown-linux-gnu.tar.gz'),
     '-C', str(release)])
distribution = release / 'rust-1.98.0-dev-x86_64-unknown-linux-gnu'
prefix = ROOT / 'toolchain'
run(['sh', str(distribution / 'install.sh'), '--prefix=' + str(prefix), '--disable-ldconfig'])
llvm = ROOT / 'llvm'
llvm.mkdir(exist_ok=True)
run(['tar', '-xJf', str(ROOT / 'inputs/rust-dev-beta-x86_64-unknown-linux-gnu.tar.xz'),
     '-C', str(llvm), '--strip-components=2'])
run([str(llvm / 'bin/llvm-config'), '--version'])
runtime = {}
for tool in ['rustc', 'cargo']:
    executable = prefix / 'bin' / tool
    runtime[tool] = {'sha256': sha(executable), 'version': subprocess.check_output(
        [str(executable), '-Vv'], text=True)}
assert manifest['rust-base.tar.gz']['revision'] in runtime['rustc']['version']
(ROOT / 'setup-result.json').write_text(json.dumps({'runtime': runtime,
    'inputs': manifest, 'free_bytes': shutil.disk_usage('/').free}, indent=2) + '\n')
print(json.dumps(runtime), flush=True)
