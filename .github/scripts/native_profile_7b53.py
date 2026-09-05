#!/usr/bin/env python3
"""Sample only workers of an owned rustc child; without --run, print the plan.

Adapted from the existing diagnostic sampler. These interrupted-worker samples
identify native hotspots; elapsed time here is not a benchmark measurement.
"""

import argparse
import ctypes
import errno
import json
import os
from pathlib import Path
import platform
import random
import resource
import signal
import subprocess
import sys
import time


PTRACE_PEEKDATA = 2
PTRACE_CONT = 7
PTRACE_GETREGS = 12
PTRACE_GETREGSET = 0x4204
NT_PRSTATUS = 1
PTRACE_SEIZE = 0x4206
PTRACE_INTERRUPT = 0x4207
PTRACE_O_EXITKILL = 1 << 20
PTRACE_EVENT_STOP = 128
WALL = 0x40000000
DEFAULT_RUSTC = Path('build/codspeed-toolchain/stage1/bin/rustc')
DEFAULT_MANIFEST = Path('build/codspeed/manifest.json')


class X86Regs(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in (
        'r15', 'r14', 'r13', 'r12', 'rbp', 'rbx', 'r11', 'r10', 'r9', 'r8', 'rax',
        'rcx', 'rdx', 'rsi', 'rdi', 'orig_rax', 'rip', 'cs', 'eflags', 'rsp',
        'ss', 'fs_base', 'gs_base', 'ds', 'es', 'fs', 'gs',
    )]


class Arm64Regs(ctypes.Structure):
    # Linux UAPI user_pt_regs: arch/arm64/include/uapi/asm/ptrace.h.
    # https://github.com/torvalds/linux/blob/v6.12/arch/arm64/include/uapi/asm/ptrace.h
    _fields_ = [('regs', ctypes.c_uint64 * 31), ('sp', ctypes.c_uint64),
                ('pc', ctypes.c_uint64), ('pstate', ctypes.c_uint64)]


class IOVec(ctypes.Structure):
    _fields_ = [('base', ctypes.c_void_p), ('length', ctypes.c_size_t)]


assert ctypes.sizeof(Arm64Regs) == 272
assert Arm64Regs.pc.offset == 256


LIBC = ctypes.CDLL(None, use_errno=True)
LIBC.ptrace.argtypes = [ctypes.c_uint, ctypes.c_uint, ctypes.c_void_p, ctypes.c_void_p]
LIBC.ptrace.restype = ctypes.c_long


def ptrace(request, tid, address=0, data=0):
    ctypes.set_errno(0)
    result = LIBC.ptrace(request, tid, ctypes.c_void_p(address), ctypes.c_void_p(data))
    error = ctypes.get_errno()
    if result == -1 and error:
        raise OSError(error, os.strerror(error), (request, tid))
    return result


def read_program_counter(tid, regs):
    if isinstance(regs, Arm64Regs):
        vector = IOVec(ctypes.addressof(regs), ctypes.sizeof(regs))
        ptrace(PTRACE_GETREGSET, tid, address=NT_PRSTATUS, data=ctypes.addressof(vector))
        if vector.length != ctypes.sizeof(regs):
            raise RuntimeError(f'Unexpected ARM64 register size: {vector.length}')
        return regs.pc
    ptrace(PTRACE_GETREGS, tid, data=ctypes.addressof(regs))
    return regs.rip


def read_caller_offsets(tid, regs, mappings, driver_base, driver_segments):
    """Read at most 16 frame records from the stopped owned worker's stack."""
    if isinstance(regs, Arm64Regs):
        frame, stack = regs.regs[29], regs.sp
        # A leaf assembly helper may omit its frame record. Retain LR separately.
        link = regs.regs[30]
    else:
        frame, stack, link = regs.rbp, regs.rsp, None
    callers = []
    for _ in range(16):
        if not (stack <= frame < stack + 64 * 1024 * 1024) or frame % 8:
            break
        if not any(m['start'] <= frame and frame + 16 <= m['end']
                   and 'r' in m['permissions'] and 'w' in m['permissions']
                   for m in mappings):
            break
        try:
            previous = ptrace(PTRACE_PEEKDATA, tid, address=frame) & ((1 << 64) - 1)
            address = ptrace(PTRACE_PEEKDATA, tid, address=frame + 8) & ((1 << 64) - 1)
        except OSError as exc:
            if exc.errno in (errno.EFAULT, errno.EIO, errno.ESRCH):
                break
            raise
        callsite = address - 1
        if any(start <= callsite < end for start, end in driver_segments):
            callers.append(callsite - driver_base)
        if previous <= frame:
            break
        frame = previous
    link_offset = (link - 1 - driver_base if link is not None
                   and any(start <= link - 1 < end for start, end in driver_segments) else None)
    return callers, link_offset


def owned_worker(pid, tid):
    # Recheck membership immediately before seizing a sampled thread. The only
    # candidate IDs come from the child process's own task directory.
    try:
        status = Path(f'/proc/{pid}/task/{tid}/status').read_text()
    except FileNotFoundError:
        return False
    group = next(line.split()[1] for line in status.splitlines() if line.startswith('Tgid:'))
    return int(group) == pid and tid != pid


def cleanup_child(child, traced, result):
    if child is None:
        return
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 3
    pending = set(traced)
    while pending and time.monotonic() < deadline:
        for tid in list(pending):
            try:
                observed, status = os.waitpid(tid, WALL | os.WNOHANG)
            except ChildProcessError:
                pending.discard(tid)
                continue
            if observed == 0:
                continue
            if os.WIFSTOPPED(status):
                try:
                    ptrace(PTRACE_CONT, tid, data=signal.SIGKILL)
                except OSError as exc:
                    if exc.errno != errno.ESRCH:
                        result['cleanup_error'] = str(exc)
            elif os.WIFEXITED(status) or os.WIFSIGNALED(status):
                pending.discard(tid)
        if pending:
            time.sleep(0.005)
    if pending:
        result['cleanup_error'] = f'Traced workers not reaped within cleanup limit: {sorted(pending)}'
    try:
        result['returncode'] = child.wait(timeout=max(0.01, deadline - time.monotonic()))
    except subprocess.TimeoutExpired:
        result['cleanup_error'] = 'Child did not exit after SIGKILL within cleanup limit'
    result['cleanup_complete'] = child.returncode is not None and not pending


def read_maps(pid):
    result = []
    for line in Path(f'/proc/{pid}/maps').read_text().splitlines():
        fields = line.split(maxsplit=5)
        start, end = (int(x, 16) for x in fields[0].split('-'))
        result.append({
            'start': start, 'end': end, 'permissions': fields[1],
            'file_offset': int(fields[2], 16),
            'path': fields[5] if len(fields) == 6 else '',
        })
    return result


def worker_tasks(pid):
    tasks = []
    for path in Path(f'/proc/{pid}/task').iterdir():
        try:
            stat = (path / 'stat').read_text()
        except FileNotFoundError:
            continue
        name = stat[stat.index('(') + 1:stat.rindex(')')]
        rest = stat[stat.rindex(')') + 2:].split()
        tasks.append({'tid': int(path.name), 'name': name, 'state': rest[0],
                      'cpu_ticks': int(rest[11]) + int(rest[12]),
                      'user_ticks': int(rest[11]), 'system_ticks': int(rest[12])})
    return tasks


def identity(path):
    stat = path.stat()
    info = {'path': str(path), 'size': stat.st_size, 'inode': stat.st_ino,
            'mtime_ns': stat.st_mtime_ns}
    notes = subprocess.run(['readelf', '-n', str(path)], check=True, text=True,
                           capture_output=True).stdout
    info['build_id'] = next((line.split('Build ID:', 1)[1].strip()
                             for line in notes.splitlines() if 'Build ID:' in line), None)
    return info


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true', help='Launch and sample rustc')
    parser.add_argument('--rustc', type=Path, default=DEFAULT_RUSTC)
    parser.add_argument('--manifest', type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument('--fixture', choices=('ruff-ty_python_semantic', 'uv-resolver'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--period-us', type=int, default=500)
    parser.add_argument('--timeout', type=float, default=120.0)
    parser.add_argument('--expected-commit', default='7d2c9ee7651076a9930b109f3738a1c6cb4dbb29')
    args = parser.parse_args()
    if not (250 <= args.period_us <= 1000 and 1 <= args.timeout <= 180):
        parser.error('Require 250 <= period-us <= 1000 and 1 <= timeout <= 180')
    args.output = args.output.resolve()
    if args.output.parent != Path('/tmp') or not args.output.name.startswith('ohm-native-7b53-'):
        parser.error('Output must be an immediate /tmp/ohm-native-7b53-* directory')
    machine = platform.machine()
    if machine not in ('x86_64', 'aarch64'):
        parser.error('Only native x86_64 and AArch64 register layouts are supported')

    rustc = args.rustc.resolve(strict=True)
    drivers = list((rustc.parent.parent / 'lib').glob('librustc_driver-*.so'))
    if len(drivers) != 1:
        raise RuntimeError(f'Expected one frozen driver, got {drivers}')
    driver = drivers[0].resolve(strict=True)
    manifest = json.loads(args.manifest.read_text())
    version = subprocess.check_output([str(rustc), '-vV'], text=True, timeout=10)
    required = (f'commit-hash: {args.expected_commit}', f'host: {machine}-unknown-linux-gnu')
    for line in required:
        if line not in version.splitlines() or line not in manifest['rustc_version'].splitlines():
            raise RuntimeError(f'Compiler/manifest identity mismatch: {line}')
    fixture, = [f for f in manifest['fixtures'] if f['name'] == args.fixture]
    if Path(fixture['command'][0]).resolve() != rustc:
        raise RuntimeError('Captured invocation uses a different compiler')
    command = list(fixture['command'])
    if '-Zthreads=1' not in command:
        raise RuntimeError('Expected the captured single-compiler-worker benchmark command')
    if any('incremental=' in arg for arg in command):
        raise RuntimeError('Full replay must not enable incremental compilation')
    command[command.index('--out-dir') + 1] = str(args.output / 'out')
    environment = os.environ | fixture['env']
    for name in ('CARGO_MAKEFLAGS', 'MAKEFLAGS', 'MFLAGS'):
        environment.pop(name, None)
    environment['LD_LIBRARY_PATH'] = str(driver.parent) + os.pathsep + environment.get('LD_LIBRARY_PATH', '')
    plan = {'rustc': identity(rustc), 'driver': identity(driver),
            'manifest': str(args.manifest.resolve()), 'fixture': args.fixture, 'profile': fixture['profile'],
            'architecture': machine, 'rustc_version': version,
            'source_revision': fixture.get('source_revision'),
            'register_api': 'PTRACE_GETREGSET/NT_PRSTATUS' if machine == 'aarch64' else 'PTRACE_GETREGS',
            'cwd': fixture['cwd'], 'command': command,
            'fixture_environment': fixture['env'],
            'effective_library_path': environment['LD_LIBRARY_PATH'],
            'period_us': args.period_us, 'period_jitter': [0.8, 1.2], 'timeout_s': args.timeout,
            'output': str(args.output), 'sampling_mode': 'random_runnable_worker',
            'clock_ticks_per_second': os.sysconf('SC_CLK_TCK')}
    if not args.run:
        print(json.dumps(plan, indent=2))
        return 0

    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / 'out').mkdir()
    (args.output / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    result = dict(plan, samples=[], map_snapshots=[], attached_threads=[],
                  signal_stops=[], returncode=None, error=None, threads={}, idle_polls=0)
    result['sample_columns'] = ['elapsed_ns', 'tid', 'pc', 'driver_offset',
                                'interrupt_latency_ns', 'observed_stopped_ns', 'runnable_workers',
                                'caller_driver_offsets', 'link_register_driver_offset']
    child = None
    launch_ns = time.perf_counter_ns()
    active_tid = None
    traced = set()
    driver_base = None
    driver_segments = []
    process_mappings = []
    sample_regs = Arm64Regs() if machine == 'aarch64' else X86Regs()
    random_period = random.Random(94601)
    previous_usage = resource.getrusage(resource.RUSAGE_CHILDREN)

    def alarm(_signum, _frame):
        raise TimeoutError(f'Profile exceeded {args.timeout:g} seconds')

    def terminate(signum, _frame):
        raise InterruptedError(f'Profile interrupted by signal {signum}')

    def snapshot_maps():
        nonlocal driver_base, driver_segments, process_mappings
        try:
            mappings = read_maps(child.pid)
        except FileNotFoundError:
            return False
        segments = [m for m in mappings if m['path'] == str(driver)]
        if not segments:
            return False
        base = min(m['start'] - m['file_offset'] for m in segments)
        if driver_base is not None and driver_base != base:
            raise RuntimeError('Frozen driver load base changed during replay')
        driver_base = base
        driver_segments = [(m['start'], m['end']) for m in segments if 'x' in m['permissions']]
        process_mappings = mappings
        result['map_snapshots'].append({'elapsed_ns': time.perf_counter_ns() - launch_ns,
                                         'mappings': mappings})
        return bool(driver_segments)

    signal.signal(signal.SIGALRM, alarm)
    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGINT, terminate)
    signal.setitimer(signal.ITIMER_REAL, args.timeout)
    try:
        with (args.output / 'stdout.log').open('wb') as out, (args.output / 'stderr.log').open('wb') as err:
            launch_ns = time.perf_counter_ns()
            child = subprocess.Popen(command, cwd=fixture['cwd'], env=environment,
                                     stdout=out, stderr=err, start_new_session=True)
            result['pid'] = child.pid
            # Attach lazily to selected workers. Exited or signal-stopped traced
            # threads are reaped/resumed every iteration so backend shutdown cannot
            # wait on an undelivered ptrace event.
            next_map_ns = launch_ns
            while True:
                for tid in list(traced):
                    try:
                        observed, status = os.waitpid(tid, WALL | os.WNOHANG)
                    except ChildProcessError:
                        traced.discard(tid)
                        continue
                    if observed == 0:
                        continue
                    if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                        result['threads'][str(tid)]['exit_status'] = status
                        traced.discard(tid)
                    elif os.WIFSTOPPED(status):
                        stop_signal = os.WSTOPSIG(status)
                        event = status >> 16
                        result['signal_stops'].append({'elapsed_ns': time.perf_counter_ns() - launch_ns,
                                                       'tid': tid, 'signal': stop_signal, 'event': event})
                        ptrace(PTRACE_CONT, tid, data=stop_signal if event == 0 else 0)

                result['returncode'] = child.poll()
                if result['returncode'] is not None:
                    break
                now_ns = time.perf_counter_ns()
                if now_ns >= next_map_ns:
                    snapshot_maps()
                    next_map_ns = now_ns + (250_000_000 if driver_base is not None else 1_000_000)
                try:
                    tasks = worker_tasks(child.pid)
                except FileNotFoundError:
                    continue
                for task in tasks:
                    key = str(task['tid'])
                    info = result['threads'].setdefault(key, {'first_seen_ns': now_ns - launch_ns, 'samples': 0})
                    info.update(task, last_seen_ns=now_ns - launch_ns)
                active = [task for task in tasks if task['tid'] != child.pid and task['state'] == 'R']
                if driver_base is None or not active:
                    result['idle_polls'] += 1
                    time.sleep(0.0005)
                    continue
                selected = random_period.choice(active)
                active_tid = selected['tid']
                if active_tid not in traced:
                    if not owned_worker(child.pid, active_tid):
                        continue
                    try:
                        ptrace(PTRACE_SEIZE, active_tid, data=PTRACE_O_EXITKILL)
                    except OSError as exc:
                        if exc.errno == errno.ESRCH:
                            continue
                        raise
                    traced.add(active_tid)
                    result['attached_threads'].append(dict(selected, elapsed_ns=time.perf_counter_ns() - launch_ns))
                request_ns = time.perf_counter_ns()
                try:
                    ptrace(PTRACE_INTERRUPT, active_tid)
                except OSError as exc:
                    if exc.errno == errno.ESRCH:
                        continue
                    raise
                observed_tid, status = os.waitpid(active_tid, WALL)
                stopped_ns = time.perf_counter_ns()
                if observed_tid != active_tid:
                    raise RuntimeError(f'Unexpected waitpid result {observed_tid}')
                if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                    result['threads'][str(active_tid)]['exit_status'] = status
                    traced.discard(active_tid)
                    continue
                if not os.WIFSTOPPED(status):
                    raise RuntimeError(f'Unexpected ptrace status {status}')
                stop_signal = os.WSTOPSIG(status)
                event = status >> 16
                if event != PTRACE_EVENT_STOP or stop_signal != signal.SIGTRAP:
                    result['signal_stops'].append({'elapsed_ns': stopped_ns - launch_ns,
                                                   'tid': active_tid, 'signal': stop_signal, 'event': event})
                    ptrace(PTRACE_CONT, active_tid, data=stop_signal if event == 0 else 0)
                    continue

                pc = read_program_counter(active_tid, sample_regs)
                offset = pc - driver_base if any(start <= pc < end for start, end in driver_segments) else None
                callers, link = read_caller_offsets(
                    active_tid, sample_regs, process_mappings, driver_base, driver_segments)
                resume_ns = time.perf_counter_ns()
                ptrace(PTRACE_CONT, active_tid)
                result['samples'].append([stopped_ns - launch_ns, active_tid, pc, offset,
                                           stopped_ns - request_ns, resume_ns - stopped_ns, len(active), callers, link])
                result['threads'][str(active_tid)]['samples'] += 1
                time.sleep(args.period_us * 1e-6 * random_period.uniform(0.8, 1.2))

            if result['returncode'] != 0:
                raise RuntimeError(f'Compiler returned {result["returncode"]}')
            if len(result['samples']) < 100:
                raise RuntimeError('Too few samples')
    except BaseException as exc:
        result['error'] = f'{type(exc).__name__}: {exc}'
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        cleanup_child(child, traced, result)
        result['wall_time_s'] = (time.perf_counter_ns() - launch_ns) / 1e9
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        result['child_user_s'] = usage.ru_utime - previous_usage.ru_utime
        result['child_system_s'] = usage.ru_stime - previous_usage.ru_stime
        result['driver_base'] = driver_base
        result['sample_count'] = len(result['samples'])
        result['driver_sample_count'] = sum(sample[3] is not None for sample in result['samples'])
        if result['samples']:
            result['first_sample_ms'] = result['samples'][0][0] / 1e6
            result['last_sample_ms'] = result['samples'][-1][0] / 1e6
            for field, column in [('interrupt_latency', 4), ('observed_stopped', 5)]:
                values = sorted(sample[column] for sample in result['samples'])
                result[field + '_total_ms'] = sum(values) / 1e6
                result[field + '_median_us'] = values[len(values) // 2] / 1e3
                result[field + '_p99_us'] = values[min(len(values) - 1, int(len(values) * 0.99))] / 1e3
        (args.output / 'samples.json').write_text(json.dumps(result) + '\n')
        print(json.dumps({key: value for key, value in result.items()
                          if key not in ('samples', 'map_snapshots', 'command', 'fixture_environment', 'threads', 'attached_threads')}, indent=2))
    return 0 if result['error'] is None and result.get('cleanup_complete') else 1


if __name__ == '__main__':
    sys.exit(main())
