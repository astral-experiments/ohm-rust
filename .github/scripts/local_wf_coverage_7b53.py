#!/usr/bin/env python3
"""Count actual acceptance and first-rejection reasons at C31's local TypeOp hook.

The denominator excludes preexisting trivial-WF/Sized fast paths. Counts identify
coverage on pinned workloads, not benchmark timings or speedup measurements.
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

CANDIDATE = "5bcf3b8256639ecfb531de1dc3873721e197a7eb"
FIELDS = ("root_kind", "root_identity", "root_shape", "root_flags", "root_binders", "caller_bounds",
          "outcome", "reason", "failure_kind", "failure_identity", "failure_shape", "failure_flags", "failure_binders", "detail")
LIMITS = (65536, 2, 4)
ENVIRONMENT = "OHM_LOCAL_WF_COVERAGE_DIR"
REASONS = {
    "accepted": {"local-wf", "direct-type-outlives", "direct-region-outlives"},
    "rejected": {"debug-assertions", "regions-disabled", "assumptions-on-binders", "root-binder",
        "param-env-error", "unsupported-predicate-kind", "unsupported-clause-kind", "root-flags",
        "non-type-wf-term", "wf-generation-none", "generated-obligation-budget", "generated-binder",
        "generated-flags", "unsupported-generated-predicate", "dyn-incompatible", "input-node-budget",
        "input-dyn-predicate-budget", "input-associated-const", "unsupported-input-type", "unsupported-input-const"},
}

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


def parse_coverage(directory, pid, stderr):
    begun = set()
    for line in stderr.splitlines():
        if line.startswith("OHM_LOCAL_WF_COVERAGE_DUMP_ERROR\t"):
            raise ValueError("compiler reported a worker coverage dump error")
        if line.startswith("OHM_LOCAL_WF_COVERAGE_BEGIN\t"):
            fields = line.split("\t")
            if len(fields) != 3 or int(fields[1]) != pid or int(fields[2]) in begun:
                raise ValueError("invalid or duplicate worker BEGIN marker")
            begun.add(int(fields[2]))
    paths = sorted(directory.glob("local-wf-coverage-*.tsv"))
    expected = {f"local-wf-coverage-{pid}-{index}.tsv" for index in begun}
    if not begun or {path.name for path in paths} != expected or list(directory.glob("*.partial")):
        raise ValueError("worker coverage dumps do not match BEGIN markers")
    combined = {}
    total = overflow = limited_shapes = 0
    for path in paths:
        lines = path.read_text().splitlines()
        if len(lines) < 2 or lines[0].split("\t") != ["ohm-local-wf-coverage-v1", *map(str, LIMITS)]:
            raise ValueError(f"invalid header: {path}")
        footer = lines[-1].split("\t")
        if len(footer) != 9 or footer[0] != "complete":
            raise ValueError(f"missing complete footer: {path}")
        started, finished, key_count, lost, limited, unfinished, active, counted = map(int, footer[1:])
        records = {}
        for line in lines[1:-1]:
            fields = line.split("\t")
            if len(fields) != len(FIELDS) + 2 or fields[0] != "row":
                raise ValueError(f"invalid coverage row: {path}")
            count = int(fields[1])
            key = tuple(map(unescape, fields[2:]))
            if count <= 0 or key in records or key[7] not in REASONS.get(key[6], set()):
                raise ValueError(f"invalid or duplicate coverage row or unfinished outcome: {path}")
            if int(key[4]) < 0 or int(key[5]) < 0 or (key[12] != "-" and int(key[12]) < 0):
                raise ValueError(f"invalid binder or caller-bound count: {path}")
            records[key] = count
        if (started <= 0 or started != finished or key_count != len(records)
                or counted != sum(records.values()) or counted + lost != finished):
            raise ValueError(f"coverage totals do not reconcile: {path}")
        if not (0 <= key_count <= LIMITS[0] and 0 <= limited <= finished and lost >= 0
                and unfinished == active == 0):
            raise ValueError(f"invalid coverage bounds or unfinished recorder: {path}")
        for key, count in records.items():
            combined[key] = combined.get(key, 0) + count
        total += started
        overflow += lost
        limited_shapes += limited
    rows = []
    kinds = {}
    decisions = {}
    wf_shapes = {}
    for key, count in combined.items():
        row = {**dict(zip(FIELDS, key)), "calls": count}
        rows.append(row)
        groups = [kinds.setdefault(row["root_kind"], {"accepted": 0, "rejected": 0})]
        if row["root_kind"] == "WellFormedType":
            shape = row["root_shape"].split("(", 1)[0]
            groups.append(wf_shapes.setdefault(shape, {"accepted": 0, "rejected": 0}))
        for group in groups:
            group[row["outcome"]] += count
        decision = (row["root_kind"], row["outcome"], row["reason"])
        decisions[decision] = decisions.get(decision, 0) + count
    def grouped(groups, field):
        return [{field: name, "calls": sum(values.values()), **values,
                 "accepted_fraction": values["accepted"] / sum(values.values())}
                for name, values in sorted(groups.items(), key=lambda item: sum(item[1].values()), reverse=True)]
    rows.sort(key=lambda row: row["calls"], reverse=True)
    return {
        "status": "partial" if overflow else "complete",
        "total_calls": total,
        "counted_calls": total - overflow,
        "overflow_calls": overflow,
        "shape_detail_limited_calls": limited_shapes,
        "denominator": "Calls that actually enter C31 ProvePredicate::try_perform_locally; preexisting trivial-WF/Sized fast-path successes are excluded.",
        "limits": dict(zip(("distinct_keys_per_worker", "shape_depth", "shape_width"), LIMITS)),
        "worker_files": [path.name for path in paths],
        "by_predicate_kind": grouped(kinds, "root_kind"),
        "by_wf_root_shape": grouped(wf_shapes, "root_shape"),
        "by_decision": [{"root_kind": key[0], "outcome": key[1], "reason": key[2], "calls": count}
                        for key, count in sorted(decisions.items(), key=lambda item: item[1], reverse=True)],
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
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--fixture", choices=("ruff-ty_python_semantic", "uv-resolver"))
    mode.add_argument("--calibrate", action="store_true")
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--rustc", type=Path, default=Path("build/codspeed-toolchain/stage1/bin/rustc"))
    parser.add_argument("--manifest", type=Path, default=Path("build/codspeed/manifest.json"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("coverage output must be a fresh directory")
    if ENVIRONMENT in os.environ:
        parser.error("coverage must be unset outside the single compiler child")
    output.mkdir(parents=True)
    report = {"status": "failed", "candidate_sha": CANDIDATE, "instrumented_sha": args.expected_commit}
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
            "platform": platform.platform(), "release": platform.release(), "machine": machine,
            "cpu_affinity": sorted(os.sched_getaffinity(0)),
            "cpuinfo": {key: sorted(values) for key, values in sorted(cpu_fields.items())},
        }
        (output / "host-metadata.json").write_text(json.dumps(report["host_metadata"], indent=2) + "\n")
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        assert head == args.expected_commit, head
        ancestor = subprocess.check_output(["git", "merge-base", CANDIDATE, head], text=True).strip()
        assert ancestor == CANDIDATE, ancestor
        allowed = {
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate.rs",
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate/local_wf_coverage.rs",
            ".github/scripts/local_wf_coverage_7b53.py",
            ".github/workflows/codspeed-local-wf-coverage-7b53.yml",
        }
        changed = set(subprocess.check_output(["git", "diff", "--name-only", CANDIDATE, head], text=True).splitlines())
        assert changed <= allowed, changed - allowed
        config = Path("src/tools/codspeed/bootstrap.toml").read_bytes()
        assert config == subprocess.check_output(["git", "show", CANDIDATE + ":src/tools/codspeed/bootstrap.toml"])
        report["candidate_config_sha256"] = hashlib.sha256(config).hexdigest()
        rustc = args.rustc.resolve(strict=True)
        version = subprocess.check_output([str(rustc), "-vV"], text=True, timeout=30)
        for line in (f"commit-hash: {head}", "host: aarch64-unknown-linux-gnu"):
            assert line in version.splitlines(), line
        (output / "out").mkdir()
        counters = output / "counters"
        counters.mkdir()
        if args.calibrate:
            source = Path("tests/ui/ohm/local-wf.rs")
            data = source.read_bytes()
            assert data == subprocess.check_output(["git", "show", CANDIDATE + ":" + str(source)])
            report.update(fixture="C31-local-wf-calibration", fixture_sha256=hashlib.sha256(data).hexdigest())
            command = [str(rustc), "--edition=2021", "--cfg=current", "--emit=metadata", "-Zthreads=1",
                       str(source.resolve()), "-o", str(output / "out/calibration.rmeta")]
            environment = dict(os.environ)
            cwd = str(Path.cwd())
        else:
            manifest = json.loads(args.manifest.read_text())
            assert manifest["rustc_version"] == version.strip() or manifest["rustc_version"] == version
            sys.path.insert(0, str(Path("src/tools/codspeed").resolve()))
            from benchmarks import WORKSPACES
            from test_compiler_walltime import prepare_invocation
            fixture, = [fixture for fixture in manifest["fixtures"] if fixture["name"] == args.fixture]
            assert fixture["source_revision"] == WORKSPACES[args.fixture]["revision"]
            assert fixture["profile"] == "check"
            assert ENVIRONMENT not in fixture["env"]
            (command,), kwargs = prepare_invocation(fixture, "Full")
            assert Path(command[0]).resolve() == rustc
            assert "-Zthreads=1" in command and not any("incremental=" in argument for argument in command)
            assert "--emit=dep-info,metadata" in command
            command[command.index("--out-dir") + 1] = str(output / "out")
            environment = kwargs["env"]
            cwd = kwargs["cwd"]
            report.update(fixture=args.fixture, source_revision=fixture["source_revision"], manifest=str(args.manifest.resolve()))
        environment[ENVIRONMENT] = str(counters)
        library_path = str(rustc.parent.parent / "lib")
        environment["LD_LIBRARY_PATH"] = library_path + os.pathsep + environment.get("LD_LIBRARY_PATH", "")
        report.update(compiler_version=version, command=command, cwd=cwd,
            environment_delta={key: environment[key] for key in (ENVIRONMENT, "LD_LIBRARY_PATH")}, timeout_seconds=180)
        (output / "plan.json").write_text(json.dumps({**report, "status": "planned"}, indent=2) + "\n")
        previous = Path.cwd()
        try:
            os.chdir(cwd)
            result = run_owned_compiler(command, environment, output / "compiler.stdout.txt", output / "compiler.stderr.txt")
        finally:
            os.chdir(previous)
        report.update(result)
        if result["compiler_exit"] != 0 or result["process_errors"]:
            raise RuntimeError("instrumented compiler did not exit normally")
        report["coverage"] = parse_coverage(counters, result["compiler_pid"], (output / "compiler.stderr.txt").read_text())
        wf, = [row for row in report["coverage"]["by_predicate_kind"] if row["root_kind"] == "WellFormedType"]
        assert wf["accepted"] and wf["rejected"], "coverage must exercise actual WF acceptance and rejection"
        report["status"] = report["coverage"]["status"]
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({key: value for key, value in report.items() if key != "coverage"}, indent=2))
        if "coverage" in report:
            print(json.dumps({key: value for key, value in report["coverage"].items() if key != "rows"}, indent=2))
    return 0 if report["status"] == "complete" else 1


if __name__ == "__main__":
    sys.exit(main())
