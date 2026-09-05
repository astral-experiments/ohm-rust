#!/usr/bin/env python3
"""Time baseline canonical TypeOp calls in one prepared leaf compilation.

These diagnostic intervals include canonical query cache hits and clock overhead.
They attribute work and are not benchmark results or speedup measurements.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import select
import signal
import subprocess
import sys
import time

BASELINE = "7d2c9ee7651076a9930b109f3738a1c6cb4dbb29"
FIELDS = ("operation", "predicate_kind", "trait", "subject_shape", "flags", "caller_bounds")
LIMITS = (65536, 2, 4)


def unescape(value):
    result = []
    cursor = 0
    replacements = {"\\": "\\", "t": "\t", "r": "\r", "n": "\n"}
    while cursor < len(value):
        character = value[cursor]
        if character == "\\":
            cursor += 1
            if cursor == len(value) or value[cursor] not in replacements:
                raise ValueError("invalid counter-field escape")
            character = replacements[value[cursor]]
        result.append(character)
        cursor += 1
    return "".join(result)


def metric_summary(values):
    calls, inclusive_ns, outer_calls, outer_ns = values
    return {
        "calls": calls,
        "inclusive_ns": inclusive_ns,
        "outer_calls": outer_calls,
        "outer_ns": outer_ns,
        "nested_calls": calls - outer_calls,
        "mean_inclusive_ns": inclusive_ns / calls if calls else None,
        "mean_outer_ns": outer_ns / outer_calls if outer_calls else None,
    }


def valid_metrics(values):
    calls, inclusive_ns, outer_calls, outer_ns = values
    return (
        all(value >= 0 for value in values)
        and outer_calls <= calls
        and outer_ns <= inclusive_ns
        and (calls != 0 or inclusive_ns == 0)
        and (outer_calls != 0 or outer_ns == 0)
        and (outer_calls != calls or outer_ns == inclusive_ns)
    )


def parse_timings(directory, pid, stderr):
    begun = set()
    for line in stderr.splitlines():
        if line.startswith("OHM_PROOF_TIMING_DUMP_ERROR\t"):
            raise ValueError("compiler reported a worker timing dump error")
        if line.startswith("OHM_PROOF_TIMING_BEGIN\t"):
            fields = line.split("\t")
            if len(fields) != 3 or int(fields[1]) != pid or int(fields[2]) in begun:
                raise ValueError("invalid or duplicate worker BEGIN marker")
            begun.add(int(fields[2]))
    paths = sorted(directory.glob("proof-timing-*.tsv"))
    expected = {f"proof-timing-{pid}-{index}.tsv" for index in begun}
    if not begun or {path.name for path in paths} != expected or list(directory.glob("*.partial")):
        raise ValueError("worker timing dumps do not match BEGIN markers")
    combined = {}
    totals = [0] * 4
    overflow = [0] * 4
    limited_shapes = max_depth = 0
    for path in paths:
        lines = path.read_text().splitlines()
        if len(lines) < 2 or lines[0].split("\t") != ["ohm-proof-timing-v1", *map(str, LIMITS)]:
            raise ValueError(f"invalid header: {path}")
        footer = lines[-1].split("\t")
        if len(footer) != 14 or footer[0] != "complete":
            raise ValueError(f"missing complete footer: {path}")
        (calls, key_count, lost_calls, limited, inclusive_ns, outer_calls, outer_ns,
         lost_inclusive_ns, lost_outer_calls, lost_outer_ns, depth, final_depth,
         nesting_errors) = map(int, footer[1:])
        total = [calls, inclusive_ns, outer_calls, outer_ns]
        lost = [lost_calls, lost_inclusive_ns, lost_outer_calls, lost_outer_ns]
        if not valid_metrics(total) or not valid_metrics(lost):
            raise ValueError(f"invalid timing totals: {path}")
        records = {}
        for line in lines[1:-1]:
            fields = line.split("\t")
            if len(fields) != 11 or fields[0] != "row":
                raise ValueError(f"invalid timing row: {path}")
            values = list(map(int, fields[1:5]))
            key = tuple(map(unescape, fields[5:]))
            if values[0] <= 0 or not valid_metrics(values) or key in records:
                raise ValueError(f"invalid or duplicate timing row: {path}")
            records[key] = values
        if (calls <= 0 or key_count != len(records)
                or any(sum(values[index] for values in records.values()) + lost[index] != total[index]
                       for index in range(4))):
            raise ValueError(f"timing totals do not reconcile: {path}")
        if not (0 <= key_count <= LIMITS[0] and 0 <= limited <= calls
                and 1 <= depth <= calls and final_depth == nesting_errors == 0):
            raise ValueError(f"invalid timing bounds or nesting: {path}")
        for key, values in records.items():
            previous = combined.setdefault(key, [0] * 4)
            for index, value in enumerate(values):
                previous[index] += value
        for index in range(4):
            totals[index] += total[index]
            overflow[index] += lost[index]
        limited_shapes += limited
        max_depth = max(max_depth, depth)
    rows = []
    operations = {}
    prove_kinds = {}
    for key, values in combined.items():
        row = {**dict(zip(FIELDS, key)), **metric_summary(values)}
        rows.append(row)
        groups = [operations.setdefault(row["operation"], [0] * 4)]
        if row["operation"] == "ProvePredicate":
            groups.append(prove_kinds.setdefault(row["predicate_kind"], [0] * 4))
        for group in groups:
            for index, value in enumerate(values):
                group[index] += value
    def grouped(groups, field):
        return [{field: name, **metric_summary(values)} for name, values in sorted(
            groups.items(), key=lambda item: (item[1][3], item[1][1]), reverse=True
        )]
    rows.sort(key=lambda row: (row["outer_ns"], row["inclusive_ns"]), reverse=True)
    return {
        "status": "partial" if overflow[0] else "complete",
        "totals": metric_summary(totals),
        "counted_calls": totals[0] - overflow[0],
        "overflow": metric_summary(overflow),
        "shape_detail_limited_calls": limited_shapes,
        "max_depth": max_depth,
        "interval_note": (
            "Inclusive times overlap. Depth-zero outer times are additive within each worker; "
            "sums across workers are not wall-clock runtime. Nested key preparation and recording "
            "are included in parent intervals when max_depth exceeds one. Diagnostic times include "
            "clock overhead and canonical cache hits; they are not benchmark results or speedups."
        ),
        "limits": dict(zip(("distinct_keys_per_worker", "shape_depth", "shape_width"), LIMITS)),
        "worker_files": [path.name for path in paths],
        "by_operation": grouped(operations, "operation"),
        "by_prove_kind": grouped(prove_kinds, "predicate_kind"),
        "rows": rows,
    }


def kill_owned_session(session_id, errors, timeout=5):
    """Bound forced cleanup to live processes in our dedicated compiler session."""
    handles = {}
    try:
        for entry in Path("/proc").iterdir():
            if not entry.name.isdecimal():
                continue
            try:
                fields = (entry / "stat").read_text().rpartition(")")[2].split()
                if int(fields[3]) != session_id or fields[0] in ("Z", "X"):
                    continue
                pid = int(entry.name)
                handle = os.pidfd_open(pid)
                # Verify membership again after opening the stable process handle.
                try:
                    fields = (entry / "stat").read_text().rpartition(")")[2].split()
                    if int(fields[3]) != session_id or fields[0] in ("Z", "X"):
                        os.close(handle)
                        continue
                except BaseException:
                    os.close(handle)
                    raise
                handles[handle] = pid
            except (FileNotFoundError, ProcessLookupError):
                continue
        if not handles:
            return []
        errors.append(f"forced cleanup of owned session processes: {sorted(handles.values())}")
        poller = select.poll()
        for handle in handles:
            poller.register(handle, select.POLLIN)
            try:
                signal.pidfd_send_signal(handle, signal.SIGKILL)
            except ProcessLookupError:
                pass
        pending = set(handles)
        deadline = time.monotonic() + timeout
        while pending and time.monotonic() < deadline:
            for handle, _ in poller.poll(max(1, int((deadline - time.monotonic()) * 1000))):
                pending.discard(handle)
                poller.unregister(handle)
        if pending:
            errors.append(f"owned processes still live after forced cleanup: {[handles[h] for h in pending]}")
        return sorted(handles.values())
    finally:
        for handle in handles:
            os.close(handle)

def run_owned_compiler(command, env, stdout_path, stderr_path, timeout=180, grace=15):
    errors = []
    timed_out = False
    forced_pids = []
    # File-backed output survives timeout and avoids pipes held open by an inferior.
    with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
        process = subprocess.Popen(
            command, env=env, text=True, stdout=stdout, stderr=stderr,
            start_new_session=True,
        )
        try:
            process.wait(timeout=timeout)
        except BaseException as error:
            timed_out = isinstance(error, subprocess.TimeoutExpired)
            errors.append(f"{type(error).__name__}: {error}")
            try:
                process.send_signal(signal.SIGINT)
                process.wait(timeout=grace)
            except BaseException as cleanup_error:
                errors.append(f"graceful interrupt: {type(cleanup_error).__name__}: {cleanup_error}")
        finally:
            try:
                forced_pids = kill_owned_session(process.pid, errors)
            except BaseException as cleanup_error:
                errors.append(f"session cleanup: {type(cleanup_error).__name__}: {cleanup_error}")
            if process.poll() is None:
                process.kill()
                try:
                    process.wait(timeout=5)
                except BaseException as cleanup_error:
                    errors.append(f"compiler cleanup: {type(cleanup_error).__name__}: {cleanup_error}")
    return {
        "compiler_exit": process.returncode,
        "compiler_pid": process.pid,
        "timed_out": timed_out,
        "forced_pids": forced_pids,
        "process_errors": errors,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", choices=("ruff-ty_python_semantic", "uv-resolver"), required=True)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--rustc", type=Path, default=Path("build/codspeed-toolchain/stage1/bin/rustc"))
    parser.add_argument("--manifest", type=Path, default=Path("build/codspeed/manifest.json"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("timing output must be a fresh directory")
    if "OHM_PROOF_TIMING_DIR" in os.environ:
        parser.error("timing must be unset outside the single leaf compiler child")
    output.mkdir(parents=True)
    report = {"status": "failed", "baseline_sha": BASELINE, "instrumented_sha": args.expected_commit}
    try:
        machine = platform.machine()
        assert machine == "aarch64", machine
        cpu_fields = {}
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            key, separator, value = line.partition(":")
            key = key.strip()
            if separator and key in {"model name", "CPU implementer", "CPU part", "Hardware"}:
                cpu_fields.setdefault(key, set()).add(value.strip())
        report["host_metadata"] = {
            "platform": platform.platform(),
            "release": platform.release(),
            "machine": machine,
            "cpu_affinity": sorted(os.sched_getaffinity(0)),
            "cpuinfo": {key: sorted(values) for key, values in sorted(cpu_fields.items())},
        }
        (output / "host-metadata.json").write_text(json.dumps(report["host_metadata"], indent=2) + "\n")
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        assert head == args.expected_commit, head
        ancestor = subprocess.check_output(["git", "merge-base", BASELINE, head], text=True).strip()
        assert ancestor == BASELINE, ancestor
        allowed = {
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate.rs",
            "compiler/rustc_trait_selection/src/traits/query/type_op/mod.rs",
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate/proof_timing.rs",
            ".github/scripts/proof_timing_7b53.py",
            ".github/workflows/codspeed-proof-timing-7b53.yml",
        }
        changed = set(subprocess.check_output(["git", "diff", "--name-only", BASELINE, head], text=True).splitlines())
        assert changed <= allowed, changed - allowed
        config = Path("src/tools/codspeed/bootstrap.toml").read_bytes()
        assert config == subprocess.check_output(["git", "show", BASELINE + ":src/tools/codspeed/bootstrap.toml"])
        report["baseline_config_sha256"] = hashlib.sha256(config).hexdigest()
        rustc = args.rustc.resolve(strict=True)
        version = subprocess.check_output([str(rustc), "-vV"], text=True, timeout=30)
        manifest = json.loads(args.manifest.read_text())
        for line in (f"commit-hash: {head}", "host: aarch64-unknown-linux-gnu"):
            assert line in version.splitlines() and line in manifest["rustc_version"].splitlines(), line
        sys.path.insert(0, str(Path("src/tools/codspeed").resolve()))
        from benchmarks import WORKSPACES
        from test_compiler_walltime import prepare_invocation
        fixture, = [fixture for fixture in manifest["fixtures"] if fixture["name"] == args.fixture]
        assert fixture["source_revision"] == WORKSPACES[args.fixture]["revision"]
        assert fixture["profile"] == "check"
        assert "OHM_PROOF_TIMING_DIR" not in fixture["env"]
        (command,), kwargs = prepare_invocation(fixture, "Full")
        assert Path(command[0]).resolve() == rustc
        assert "-Zthreads=1" in command and not any("incremental=" in argument for argument in command)
        assert "--emit=dep-info,metadata" in command
        (output / "out").mkdir()
        counters = output / "counters"
        counters.mkdir()
        command[command.index("--out-dir") + 1] = str(output / "out")
        environment = kwargs["env"]
        environment["OHM_PROOF_TIMING_DIR"] = str(counters)
        library_path = str(rustc.parent.parent / "lib")
        environment["LD_LIBRARY_PATH"] = library_path + os.pathsep + environment.get("LD_LIBRARY_PATH", "")
        report.update(
            compiler_version=version,
            fixture=args.fixture,
            source_revision=fixture["source_revision"],
            manifest=str(args.manifest.resolve()),
            command=command,
            cwd=kwargs["cwd"],
            environment_delta={key: environment[key] for key in ("OHM_PROOF_TIMING_DIR", "LD_LIBRARY_PATH")},
            timeout_seconds=180,
        )
        (output / "plan.json").write_text(json.dumps({**report, "status": "planned"}, indent=2) + "\n")
        previous = Path.cwd()
        try:
            os.chdir(kwargs["cwd"])
            result = run_owned_compiler(command, environment, output / "compiler.stdout.txt", output / "compiler.stderr.txt")
        finally:
            os.chdir(previous)
        report.update(result)
        if result["compiler_exit"] != 0 or result["process_errors"]:
            raise RuntimeError("timed compiler did not exit normally")
        report["timings"] = parse_timings(
            counters, result["compiler_pid"], (output / "compiler.stderr.txt").read_text(),
        )
        report["status"] = report["timings"]["status"]
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({key: value for key, value in report.items() if key != "timings"}, indent=2))
        if "timings" in report:
            print(json.dumps({key: value for key, value in report["timings"].items() if key != "rows"}, indent=2))
    return 0 if report["status"] == "complete" else 1


if __name__ == "__main__":
    sys.exit(main())
