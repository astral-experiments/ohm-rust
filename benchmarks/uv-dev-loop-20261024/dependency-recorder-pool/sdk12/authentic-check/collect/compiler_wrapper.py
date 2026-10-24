"""Stable pass-through wrapper; profile only the first selected child transition."""
from pathlib import Path
import hashlib
import json
import os
import sys
import time

ROOT = Path(__file__).resolve().parent
INPUTS = json.loads((ROOT / 'inputs.json').read_text())
FIXTURE = Path(INPUTS['fixture'])
phase = json.loads((FIXTURE / 'phase.json').read_text())
argv = sys.argv[1:]
assert argv and argv[0] == str(FIXTURE / 'runtime/bin/rustc')
environment_rows = [key + '=' + value for key, value in os.environ.items()]
environment = dict(os.environ)
selected = ('--crate-name' in argv and argv[argv.index('--crate-name') + 1] == 'uv_client'
            and '--crate-type' in argv and argv[argv.index('--crate-type') + 1] == 'lib'
            and '--emit=dep-info,metadata' in argv and '--test' not in argv
            and any(arg == 'crates/uv-client/src/lib.rs' or arg == str(FIXTURE / 'uv/crates/uv-client/src/lib.rs') for arg in argv))
row = {'pid': os.getpid(), 'parent_pid': os.getppid(), 'monotonic_ns': time.monotonic_ns(),
       'argv': argv, 'environment_rows': environment_rows, 'cwd': os.getcwd(),
       'selected_ordinary_check_library': selected, 'profile': phase['profile'] and selected,
       'affinity': sorted(os.sched_getaffinity(0))}
directory = FIXTURE / 'records' / phase['name']
path = directory / 'compiler' / (str(os.getpid()) + '.json')
with path.open('x') as stream:
    json.dump(row, stream, indent=2)
    stream.write('\n')
if row['profile']:
    assert row['affinity'] == list(range(8))
    assert argv.count('-Zthreads=8') == 1
    assert not any(any(flag in arg for flag in ('target-cpu', 'target-feature', 'tune-cpu', 'llvm-args')) for arg in argv)
    assert '--target' not in argv and not any(arg.startswith('--target=') for arg in argv)
    assert 'LD_PRELOAD' not in environment and 'LD_LIBRARY_PATH' not in environment
    assert environment['OHM_BENCH_NORMALIZE_VALGRIND_ENV'] == '1'
    assert environment['RUSTC_WRAPPER'] == str(FIXTURE / 'bin/rustc-wrapper')
    tools = Path(INPUTS['profiling_tools'])
    profile_args = [str(tools / 'valgrind/usr/bin/valgrind.bin'), '--tool=callgrind',
                    '--trace-children=no', '--demangle=no',
                    '--extra-debuginfo-path=' + str(tools / 'valgrind/usr/lib/debug'),
                    '--callgrind-out-file=' + str(directory / 'check-rustc.%p.callgrind'),
                    '--log-file=' + str(directory / 'check-rustc.%p.valgrind.stderr')]
    with (directory / 'selected-once.json').open('x') as stream:
        json.dump({'pid': os.getpid(), 'context_path': str(path),
                   'context_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                   'original_jobserver_preserved': environment.get('CARGO_MAKEFLAGS'),
                   'profile_args': profile_args}, stream, indent=2)
        stream.write('\n')
    environment['LD_PRELOAD'] = str(tools / 'ohm-valgrind-env.so')
    os.execve(profile_args[0], profile_args + argv, environment)
os.execve(argv[0], argv, environment)
