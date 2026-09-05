#!/usr/bin/env python3
"""Count baseline ProvePredicate fallthroughs in one prepared leaf compilation.

These counts include requests satisfied by canonical query caches. They are
coverage data, not timings or a promise that a candidate can prove every hit.
"""

import argparse
from collections import Counter
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
FIELDS = ("predicate_kind", "trait", "subject_shape", "flags", "caller_bounds")
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


def parse_counts(directory, pid, stderr):
    begun = set()
    for line in stderr.splitlines():
        if line.startswith("OHM_PROOF_STATS_DUMP_ERROR\t"):
            raise ValueError("compiler reported a worker counter dump error")
        if line.startswith("OHM_PROOF_STATS_BEGIN\t"):
            fields = line.split("\t")
            if len(fields) != 3 or int(fields[1]) != pid or int(fields[2]) in begun:
                raise ValueError("invalid or duplicate worker BEGIN marker")
            begun.add(int(fields[2]))
    paths = sorted(directory.glob("proof-stats-*.tsv"))
    expected = {f"proof-stats-{pid}-{index}.tsv" for index in begun}
    if not begun or {path.name for path in paths} != expected or list(directory.glob("*.partial")):
        raise ValueError("worker counter dumps do not match BEGIN markers")
    combined = Counter()
    total = overflow = limited_shapes = 0
    for path in paths:
        lines = path.read_text().splitlines()
        if len(lines) < 2 or lines[0].split("\t") != ["ohm-proof-stats-v1", *map(str, LIMITS)]:
            raise ValueError(f"invalid header: {path}")
        footer = lines[-1].split("\t")
        if len(footer) != 5 or footer[0] != "complete":
            raise ValueError(f"missing complete footer: {path}")
        count, key_count, lost, limited = map(int, footer[1:])
        records = {}
        for line in lines[1:-1]:
            values = line.split("\t")
            if len(values) != 7 or values[0] != "row":
                raise ValueError(f"invalid counter row: {path}")
            value = int(values[1])
            key = tuple(map(unescape, values[2:]))
            if value <= 0 or key in records:
                raise ValueError(f"invalid or duplicate counter: {path}")
            records[key] = value
        if count <= 0 or key_count != len(records) or sum(records.values()) + lost != count:
            raise ValueError(f"counter totals do not reconcile: {path}")
        if not (0 <= key_count <= LIMITS[0] and 0 <= lost <= count and 0 <= limited <= count):
            raise ValueError(f"invalid counter bounds: {path}")
        combined.update(records)
        total += count
        overflow += lost
        limited_shapes += limited
    rows = [{"count": count, **dict(zip(FIELDS, key))} for key, count in combined.most_common()]
    kinds = Counter()
    traits = Counter()
    for row in rows:
        kinds[row["predicate_kind"]] += row["count"]
        if row["trait"] != "-":
            traits[(row["predicate_kind"], row["trait"])] += row["count"]
    return {
        "status": "partial" if overflow else "complete",
        "total_fallthroughs": total,
        "counted_fallthroughs": total - overflow,
        "key_overflow_hits": overflow,
        "shape_detail_limited_hits": limited_shapes,
        "limits": dict(zip(("distinct_keys_per_worker", "shape_depth", "shape_width"), LIMITS)),
        "worker_files": [path.name for path in paths],
        "by_kind": dict(kinds.most_common()),
        "by_trait": [{"predicate_kind": kind, "trait": name, "count": count}
                     for (kind, name), count in traits.most_common()],
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
        parser.error("counter output must be a fresh directory")
    if "OHM_PROOF_STATS_DIR" in os.environ:
        parser.error("statistics must be unset outside the single leaf compiler child")
    output.mkdir(parents=True)
    report = {"status": "failed", "baseline_sha": BASELINE, "instrumented_sha": args.expected_commit}
    try:
        machine = platform.machine()
        assert machine == "aarch64", machine
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        assert head == args.expected_commit, head
        ancestor = subprocess.check_output(["git", "merge-base", BASELINE, head], text=True).strip()
        assert ancestor == BASELINE, ancestor
        allowed = {
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate.rs",
            "compiler/rustc_trait_selection/src/traits/query/type_op/prove_predicate/proof_stats.rs",
            ".github/scripts/proof_stats_7b53.py",
            ".github/workflows/codspeed-proof-stats-7b53.yml",
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
        assert "OHM_PROOF_STATS_DIR" not in fixture["env"]
        (command,), kwargs = prepare_invocation(fixture, "Full")
        assert Path(command[0]).resolve() == rustc
        assert "-Zthreads=1" in command and not any("incremental=" in argument for argument in command)
        assert "--emit=dep-info,metadata" in command
        (output / "out").mkdir()
        counters = output / "counters"
        counters.mkdir()
        command[command.index("--out-dir") + 1] = str(output / "out")
        environment = kwargs["env"]
        environment["OHM_PROOF_STATS_DIR"] = str(counters)
        library_path = str(rustc.parent.parent / "lib")
        environment["LD_LIBRARY_PATH"] = library_path + os.pathsep + environment.get("LD_LIBRARY_PATH", "")
        report.update(
            compiler_version=version,
            fixture=args.fixture,
            source_revision=fixture["source_revision"],
            manifest=str(args.manifest.resolve()),
            command=command,
            cwd=kwargs["cwd"],
            environment_delta={key: environment[key] for key in ("OHM_PROOF_STATS_DIR", "LD_LIBRARY_PATH")},
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
            raise RuntimeError("counted compiler did not exit normally")
        report["counts"] = parse_counts(
            counters, result["compiler_pid"], (output / "compiler.stderr.txt").read_text(),
        )
        report["status"] = report["counts"]["status"]
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
    finally:
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({key: value for key, value in report.items() if key != "counts"}, indent=2))
        if "counts" in report:
            print(json.dumps({key: value for key, value in report["counts"].items() if key not in ("rows", "by_trait")}, indent=2))
    return 0 if report["status"] == "complete" else 1


if __name__ == "__main__":
    sys.exit(main())
