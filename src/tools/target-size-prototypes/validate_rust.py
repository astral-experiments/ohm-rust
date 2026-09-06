"""Validate and compare independently built target-size compiler prototypes.

All compiler preparation is outside measured calls. Run measurements serially on
the dedicated host only after the compiler build queue is idle.
"""
import argparse
from collections import defaultdict
import fcntl
from functools import cache
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tarfile
import time


ROOT = Path('/tmp/ohm-target-size-prototypes')
BUILDS = Path('/tmp/codex-cargo-targets/ohm-rust/isolated/target-size-prototypes')
VARIANTS = {'baseline': '00', 'dep-fingerprints': '01', 'allocation-aliases': '02',
            'sparse-metadata': '03', 'cloned-prefixes': '04'}
FIXTURES = {'syn-2.0.101': 'syn', 'regex-automata-0.4.8': 'regex_automata'}
REVISIONS = [False, False, True, True, False, False, True]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def clean_env():
    env = os.environ.copy()
    for key in ['CARGO_MAKEFLAGS', 'MAKEFLAGS', 'MFLAGS', 'CARGO_BUILD_BUILD_DIR',
                'CARGO_TARGET_DIR', 'RUSTUP_TOOLCHAIN', 'RUSTC', 'RUSTDOC',
                'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS',
                'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_FORCE_RUSTC_VERSION',
                '__CARGO_OHM_DEFAULTS']:
        env.pop(key, None)
    env.update(CARGO_HOME='/tmp/codex-cargo-home', RUSTC_BOOTSTRAP='1',
               LD_LIBRARY_PATH=str(ROOT / 'llvm/lib'))
    return env


def sysroot(variant):
    return BUILDS / variant / 'x86_64-unknown-linux-gnu/stage1'


def compiler(variant):
    return sysroot(variant) / 'bin/rustc'


@cache
def version(variant):
    rustc = compiler(variant)
    value = subprocess.check_output([str(rustc), '-vV'], text=True, env=clean_env())
    status = json.loads((ROOT / 'logs' / (variant + '-status.json')).read_text())
    assert status['returncode'] == 0 and status['revision'] in value, value
    libraries = list((sysroot(variant) / 'lib').glob('librustc_driver-*.so'))
    libraries += list((sysroot(variant) / 'lib/rustlib/x86_64-unknown-linux-gnu/lib')
                      .glob('libstd-*.rlib'))
    assert libraries, sysroot(variant)
    return {'version': value, 'sha256': sha(rustc), 'revision': status['revision'],
            'patch_sha256': status['patch_sha256'],
            'library_sha256': {str(path.relative_to(sysroot(variant))): sha(path)
                               for path in libraries}}


def storage(path):
    """Sum apparent file lengths; count physical blocks once per inode."""
    totals = defaultdict(lambda: {'logical': 0, 'physical': 0, 'files': 0})
    seen = set()
    for file in path.rglob('*'):
        if not file.is_file() or file.is_symlink():
            continue
        stat = file.stat()
        category = file.name if file.name in ('query-cache.bin', 'dep-graph.bin') else file.suffix
        value = totals[category or 'other']
        value['logical'] += stat.st_size
        value['files'] += 1
        inode = (stat.st_dev, stat.st_ino)
        if inode not in seen:
            seen.add(inode)
            value['physical'] += stat.st_blocks * 512
    return dict(totals)



def dep_graph_counts(path, variant):
    """Read the fixed-size graph footer outside the timed compiler call."""
    compact = variant == 'dep-fingerprints'
    fields = ['node_max', 'node_count', 'edge_count'] + (['zero_value_count'] if compact else [])
    total = dict.fromkeys(fields, 0)
    total['files'] = 0
    for file in path.rglob('dep-graph.bin'):
        with file.open('rb') as stream:
            stream.seek(-(len(fields) * 8 + len(b'rust-end-file')), os.SEEK_END)
            footer = stream.read()
        assert footer.endswith(b'rust-end-file'), file
        for index, field in enumerate(fields):
            total[field] += int.from_bytes(footer[index * 8:(index + 1) * 8], 'little')
        total['files'] += 1
    if compact:
        total['net_omitted_header_bytes'] = 16 * total['zero_value_count'] - 8 * total['files']
    return total


def fixture_inputs():
    manifest = json.loads((ROOT / 'rustc-perf-fixtures.json').read_text())
    archive = ROOT / 'inputs/rustc-perf-fixtures.tar.gz'
    assert sha(archive) == manifest['archive_sha256']
    source = ROOT / 'benchmark-sources'
    if not source.exists():
        source.mkdir()
        with tarfile.open(archive) as stream:
            stream.extractall(source, filter='data')
    return source, manifest


def paths(variant, fixture, profile):
    data = ROOT / 'measurements' / VARIANTS[variant] / (fixture + '-' + profile)
    target = BUILDS / 'measurements' / VARIANTS[variant] / (fixture + '-' + profile)
    return data, target


def restore_source(fixture, patched):
    source, _ = fixture_inputs()
    cwd = ROOT / 'benchmark-work' / fixture
    if cwd.exists():
        shutil.rmtree(cwd)
    shutil.copytree(source / fixture, cwd)
    if patched:
        env = clean_env()
        env['GIT_CEILING_DIRECTORIES'] = str(cwd.parent)
        env.pop('GIT_DIR', None)
        env.pop('GIT_WORK_TREE', None)
        subprocess.run(['git', 'apply', str(cwd / '0-println.patch')], cwd=cwd,
                       env=env, check=True, capture_output=True)
    return cwd


def prepare(variant, fixture, profile):
    compiler_info = version(variant)
    _, inputs = fixture_inputs()
    data, target = paths(variant, fixture, profile)
    data.mkdir(parents=True, exist_ok=False)
    cwd = restore_source(fixture, False)
    capture = data / 'capture.json'
    env = clean_env()
    env.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               RUSTC=str(compiler(variant)), RUSTC_FORCE_RUSTC_VERSION='rustc-perf',
               RUSTC_WRAPPER=str(ROOT / 'sources/baseline/src/tools/codspeed/capture.py'),
               OHM_CAPTURE_CRATE=FIXTURES[fixture], OHM_CAPTURE_FILE=str(capture),
               CARGO_TERM_COLOR='never', CARGO_ENCODED_RUSTFLAGS='')
    with (data / 'prepare.log').open('w') as log:
        subprocess.run([str(ROOT / 'toolchain/bin/cargo'), 'rustc', '--locked',
                        '--lib', '--profile', profile, '--', '-Zthreads=1'], cwd=cwd, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    invocation = json.loads(capture.read_text())
    assert invocation['command'][0] == str(compiler(variant))
    emitted = next(arg.removeprefix('--emit=') for arg in invocation['command']
                   if arg.startswith('--emit='))
    assert 'metadata' in emitted and ('link' in emitted) == (profile == 'dev'), emitted
    cargo = ROOT / 'toolchain/bin/cargo'
    write_json(data / 'manifest.json', {**invocation, 'compiler': compiler_info,
               'inputs': inputs, 'variant': variant, 'fixture': fixture,
               'harness_sha256': sha(Path(__file__)),
               'cargo': {'sha256': sha(cargo),
                         'version': subprocess.check_output(
                             [str(cargo), '-Vv'], env=env, text=True),
                         'ohm_defaults_override': env.get('__CARGO_OHM_DEFAULTS')},
               'profile': profile, 'incremental': str(target / 'measured-incremental'),
               'target': str(target)})


def invocation(variant, fixture, profile):
    data, _ = paths(variant, fixture, profile)
    manifest = json.loads((data / 'manifest.json').read_text())
    current = version(variant)
    assert all(current[key] == value for key, value in manifest['compiler'].items())
    manifest['compiler'] = current
    env = clean_env() | manifest['env']
    for key in ['CARGO_MAKEFLAGS', 'MAKEFLAGS', 'MFLAGS', '__CARGO_OHM_DEFAULTS']:
        env.pop(key, None)
    env['LD_LIBRARY_PATH'] = str(ROOT / 'llvm/lib')
    # Earlier captures used verification during preparation. Never measure it.
    command = [arg for arg in manifest['command'] if arg != '-Zincremental-verify-ich']
    command += ['-C', 'incremental=' + manifest['incremental']]
    manifest['effective_command'] = command
    manifest['effective_harness_sha256'] = sha(Path(__file__))
    manifest['effective_llvm_library_path'] = env['LD_LIBRARY_PATH']
    manifest['effective_compile_env'] = (manifest['env']
                                         | {'LD_LIBRARY_PATH': env['LD_LIBRARY_PATH']})
    manifest['effective_ohm_defaults_override'] = env.get('__CARGO_OHM_DEFAULTS')
    return manifest, command, env


def reset_incremental(manifest):
    path = Path(manifest['incremental'])
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)


def run_call(command, cwd, env, output):
    started = time.perf_counter_ns()
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True)
    elapsed = time.perf_counter_ns() - started
    Path(str(output) + '.stdout').write_bytes(result.stdout)
    Path(str(output) + '.stderr').write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace')[-8000:])
    return {'elapsed_ns': elapsed, 'returncode': result.returncode,
            'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(),
            'stderr_sha256': hashlib.sha256(result.stderr).hexdigest()}


def cycle(variant, fixture, profile, label, verify=False):
    manifest, command, env = invocation(variant, fixture, profile)
    data, _ = paths(variant, fixture, profile)
    output = data / label
    output.mkdir(exist_ok=False)
    if verify:
        command += ['-Zincremental-verify-ich']
    write_json(output / 'invocation.json', {'manifest': manifest, 'command': command,
               'verification': verify})
    reset_incremental(manifest)
    result = []
    for index, patched in enumerate(REVISIONS, 1):
        cwd = restore_source(fixture, patched)
        receipt = run_call(command, cwd, env, output / f'r{index}')
        receipt.update(revision=index, patched=patched,
                       storage=storage(Path(manifest['target'])),
                       dep_graph_counts=dep_graph_counts(Path(manifest['target']), variant))
        result.append(receipt)
        write_json(output / 'results.json', result)
    return result


def cycles(candidate, fixture, profile, rounds):
    # One warm-up cycle per compiler; ABBA/BAAB order balances later drift.
    for variant in ['baseline', candidate]:
        cycle(variant, fixture, profile, 'warmup-' + candidate)
    results = []
    for round in range(rounds):
        order = ['baseline', candidate, candidate, 'baseline']
        if round % 2:
            order = [candidate, 'baseline', 'baseline', candidate]
        for slot, variant in enumerate(order):
            result = cycle(variant, fixture, profile, f'cycle-{candidate}-{round}-{slot}')
            results.append({'variant': variant, 'round': round, 'slot': slot, 'result': result})
    summary = {}
    for index in range(7):
        base = [r['result'][index]['elapsed_ns'] for r in results if r['variant'] == 'baseline']
        head = [r['result'][index]['elapsed_ns'] for r in results if r['variant'] == candidate]
        summary[str(index + 1)] = {'baseline_median_ns': statistics.median(base),
            'candidate_median_ns': statistics.median(head),
            'ratio': statistics.median(head) / statistics.median(base)}
    write_json(ROOT / 'measurements' / f'{candidate}-{fixture}-{profile}-cycles.json',
               {'results': results, 'summary': summary,
                'effective_manifests': {variant: invocation(variant, fixture, profile)[0]
                                        for variant in ['baseline', candidate]}})


def cachegrind(candidate, fixture, profile, rounds, revisions):
    results = []
    for revision in revisions:
        for round in range(rounds):
            order = ['baseline', candidate] if round % 2 == 0 else [candidate, 'baseline']
            for variant in order:
                manifest, command, env = invocation(variant, fixture, profile)
                data, _ = paths(variant, fixture, profile)
                output = data / f'cachegrind-{candidate}-{revision}-{round}'
                output.mkdir(exist_ok=False)
                reset_incremental(manifest)
                for index in range(revision - 1):
                    cwd = restore_source(fixture, REVISIONS[index])
                    run_call(command, cwd, env, output / f'seed-{index}')
                cwd = restore_source(fixture, REVISIONS[revision - 1])
                counts = output / 'cachegrind.out'
                measured = ['valgrind', '--tool=cachegrind', '--cache-sim=no',
                            '--branch-sim=no', '--cachegrind-out-file=' + str(counts)] + command
                write_json(output / 'invocation.json', {'manifest': manifest,
                           'command': measured, 'verification': False})
                receipt = run_call(measured, cwd, env, output / 'measured')
                lines = counts.read_text().splitlines()
                events = next(x.split()[1:] for x in lines if x.startswith('events:'))
                values = next(x.split()[1:] for x in lines if x.startswith('summary:'))
                receipt.update(variant=variant, revision=revision, round=round,
                               events=dict(zip(events, map(int, values))),
                               counts_sha256=sha(counts),
                               invocation_sha256=sha(output / 'invocation.json'),
                               compiler_command=command,
                               effective_compile_env=manifest['effective_compile_env'],
                               harness_sha256=sha(Path(__file__)))
                results.append(receipt)
                write_json(ROOT / 'measurements'
                           / f'{candidate}-{fixture}-{profile}-cachegrind.json', results)


def regression(variant):
    compiler_info = version(variant)
    out = ROOT / 'measurements' / VARIANTS[variant] / 'regressions'
    out.mkdir(parents=True, exist_ok=False)
    sources = ROOT / 'sources/allocation-aliases/tests/incremental'
    results = []
    for name in ['forward-allocation-payload.rs', 'forward-cached-query-value.rs']:
        incremental = out / (name + '.incr')
        for revision in range(1, 8):
            prefix = out / (name + f'-r{revision}')
            binary = out / (name + '.bin')
            command = [str(compiler(variant)), str(sources / name), '--edition=2024',
                       '--cfg', f'rpass{revision}', '-Ccodegen-units=1', '-Zthreads=4',
                       '-Zquery-dep-graph', '-Zincremental-verify-ich', '-Zincremental-info',
                       '-Cincremental=' + str(incremental), '-o', str(binary)]
            receipt = run_call(command, sources, clean_env(), prefix)
            subprocess.run([str(binary)], check=True, env=clean_env(), capture_output=True)
            receipt.update(name=name, revision=revision, storage=storage(incremental))
            results.append(receipt)
            write_json(out / 'results.json', {'compiler': compiler_info, 'results': results})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',
                        choices=['prepare', 'regression', 'verify', 'cycles', 'cachegrind'])
    parser.add_argument('--variant', required=True, choices=VARIANTS)
    parser.add_argument('--fixture', choices=FIXTURES, default='syn-2.0.101')
    parser.add_argument('--profile', choices=['check', 'dev'], default='check')
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--revisions', type=int, nargs='+', choices=[2, 3, 4], default=[2, 3, 4],
                        help='Cachegrind revisions to measure (default: 2 3 4)')
    args = parser.parse_args()
    # All variants restore the same fixture directory. Serialize preparation and
    # measurement even when different callers submit jobs concurrently.
    with (ROOT / 'common-validation.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if args.action == 'prepare':
            prepare(args.variant, args.fixture, args.profile)
        elif args.action == 'regression':
            regression(args.variant)
        elif args.action == 'verify':
            cycle(args.variant, args.fixture, args.profile, 'correctness', verify=True)
        elif args.action == 'cycles':
            cycles(args.variant, args.fixture, args.profile, args.rounds)
        else:
            cachegrind(args.variant, args.fixture, args.profile, args.rounds, args.revisions)


if __name__ == '__main__':
    main()
