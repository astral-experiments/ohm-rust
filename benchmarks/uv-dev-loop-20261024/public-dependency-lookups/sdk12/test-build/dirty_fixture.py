#!/usr/bin/env python3
"""Prepare equivalent dirty-source Cargo samples and restore the original file."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import time


def digest(data):
    return hashlib.sha256(data).hexdigest()


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def journal(state, entry):
    with (state / "journal.jsonl").open("a") as stream:
        stream.write(json.dumps({"at": now(), **entry}) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


@contextmanager
def locked(state):
    with (state / "lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def validate_paths(targets, shared):
    baseline, candidate = targets["baseline"], targets["candidate"]
    if baseline == candidate or baseline in candidate.parents or candidate in baseline.parents:
        raise ValueError("baseline and candidate target directories must be distinct and disjoint")
    if len(os.fsencode(baseline)) != len(os.fsencode(candidate)):
        raise ValueError("use target directory paths of equal byte length (for example base/cand)")
    for target in targets.values():
        if target == shared or target in shared.parents or shared in target.parents:
            raise ValueError("shared build directory must be separate from both target directories")


def initialize(state, source, workspace, targets, shared):
    source = Path(source).absolute()
    if source.is_symlink() or not source.is_file():
        raise ValueError("source must be a regular file, not a symlink")
    source = source.resolve()
    workspace = Path(workspace).resolve()
    if workspace not in source.parents:
        raise ValueError("source must be inside the configured workspace")
    targets = {name: Path(path).resolve() for name, path in targets.items()}
    shared = Path(shared).resolve()
    validate_paths(targets, shared)
    original = source.read_bytes()
    if b"// ohm-dev-loop-fixture " in original:
        raise ValueError("source already contains a fixture marker; restore its owner first")
    state = Path(state).resolve()
    state.mkdir(parents=True, exist_ok=False)
    (state / "original.bin").write_bytes(original)
    manifest = {
        "schema_version": 1,
        "created_at": now(),
        "source": str(source),
        "workspace": str(workspace),
        "source_sha256": digest(original),
        "source_mode": stat.S_IMODE(source.stat().st_mode),
        "targets": {name: str(path) for name, path in targets.items()},
        "shared_build_dir": str(shared),
        "edit": "Append a deterministic non-doc comment; BEFORE and AFTER differ in content with equal byte length.",
    }
    write_json(state / "manifest.json", manifest)
    write_json(state / "state.json", {"current_sha256": digest(original), "restored": True, "last_tokens": {}})
    journal(state, {"operation": "initialize", "source_sha256": digest(original)})
    return manifest


def load(state):
    manifest = json.loads((state / "manifest.json").read_text())
    status = json.loads((state / "state.json").read_text())
    original = (state / "original.bin").read_bytes()
    if digest(original) != manifest["source_sha256"]:
        raise ValueError("saved original content was changed")
    source = Path(manifest["source"])
    if source.is_symlink() or not source.is_file():
        raise ValueError("source stopped being a regular file")
    current = digest(source.read_bytes())
    if current not in {manifest["source_sha256"], status["current_sha256"], status.get("pending_sha256")}:
        raise ValueError("source contains an unrelated edit; refusing to overwrite it")
    return manifest, status, original


def content(original, batch, stage, pair, position):
    if stage not in {"warmup", "measurement"} or pair < 0 or pair > 99999999:
        raise ValueError("invalid fixture stage or pair")
    if position not in {"before", "after_"}:
        raise ValueError("invalid fixture content position")
    batch_id = digest(str(Path(batch).resolve()).encode())[:16]
    stage_id = "warm" if stage == "warmup" else "meas"
    separator = b"\n" if original.endswith(b"\n") else b"\n\n"
    marker = f"// ohm-dev-loop-fixture batch={batch_id} stage={stage_id} pair={pair:08d} state={position}\n".encode()
    return original + separator + marker


def replace_source(state, manifest, status, data):
    source = Path(manifest["source"])
    previous_mtime = source.stat().st_mtime_ns
    # Record the intended content before the atomic replacement, so an interrupted
    # process can safely restore either side of the replacement.
    status["pending_sha256"] = digest(data)
    write_json(state / "state.json", status)
    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".ohm-fixture-", dir=source.parent, delete=False) as temporary:
            temporary_path = Path(temporary.name)
            temporary.write(data)
            temporary.flush()
            os.fsync(temporary.fileno())
        temporary_path.chmod(manifest["source_mode"])
        temporary_path.replace(source)
        timestamp = max(time.time_ns(), previous_mtime + 1)
        os.utime(source, ns=(timestamp, timestamp))
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)
    status["current_sha256"] = digest(data)
    status["restored"] = digest(data) == manifest["source_sha256"]
    status.pop("pending_sha256", None)
    write_json(state / "state.json", status)


def validate_build_command(command):
    if "test" in command:
        if "--no-run" not in command:
            raise ValueError("test workload must include --no-run")
    elif "check" not in command:
        raise ValueError("fixture warm command must be cargo check or cargo test --no-run")


def prepare(state, batch, stage, pair, variant, command, environment=None, warm_runner=None):
    environment = os.environ.copy() if environment is None else environment
    validate_build_command(command)
    state = Path(state).resolve()
    with locked(state):
        manifest, status, original = load(state)
        if variant not in manifest["targets"]:
            raise ValueError("variant must be baseline or candidate")
        target = environment.get("CARGO_TARGET_DIR")
        shared = environment.get("CARGO_BUILD_BUILD_DIR")
        if not target or str(Path(target).resolve()) != manifest["targets"][variant]:
            raise ValueError("CARGO_TARGET_DIR does not match this variant's isolated target")
        if not shared or str(Path(shared).resolve()) != manifest["shared_build_dir"]:
            raise ValueError("CARGO_BUILD_BUILD_DIR does not match the fixture's shared registry cache")
        before = content(original, batch, stage, pair, "before")
        after = content(original, batch, stage, pair, "after_")
        token = digest(after)
        if status["last_tokens"].get(variant) == token:
            raise ValueError("this variant's fixture step was already prepared; use a new batch output directory")
        replace_source(state, manifest, status, before)
        started = time.perf_counter_ns()
        process = (warm_runner or subprocess.run)(command, cwd=manifest["workspace"], env=environment)
        warm_ns = time.perf_counter_ns() - started
        if process.returncode:
            journal(state, {"operation": "warm-failed", "batch": str(Path(batch).resolve()), "stage": stage, "pair": pair, "variant": variant, "returncode": process.returncode})
            raise RuntimeError("untimed warm command failed; source restoration remains available")
        # The warm command must not itself alter the edited source.
        load(state)
        if digest(Path(manifest["source"]).read_bytes()) != digest(before):
            raise ValueError("warm command changed the fixture source")
        replace_source(state, manifest, status, after)
        status["last_tokens"][variant] = token
        write_json(state / "state.json", status)
        record = {
            "operation": "prepare", "batch": str(Path(batch).resolve()), "stage": stage,
            "pair": pair, "variant": variant, "target": target, "shared_build_dir": shared,
            "before_sha256": digest(before), "after_sha256": digest(after),
            "before_bytes": len(before), "after_bytes": len(after),
            "warm_command": command, "warm_wall_ns": warm_ns,
            "source_mtime_ns": Path(manifest["source"]).stat().st_mtime_ns,
        }
        if hasattr(process, "preparation_proof"):
            record["preparation_proof"] = process.preparation_proof
        journal(state, record)
        return record


def restore(state):
    state = Path(state).resolve()
    with locked(state):
        manifest, status, original = load(state)
        if digest(Path(manifest["source"]).read_bytes()) != manifest["source_sha256"]:
            replace_source(state, manifest, status, original)
        status["restored"] = True
        write_json(state / "state.json", status)
        record = {"operation": "restore", "source": manifest["source"], "sha256": digest(original)}
        journal(state, record)
        return record


def compile_packages(stderr):
    return sorted(set(re.findall(r"^\s*(?:Checking|Compiling)\s+([^\s]+)\s+v[^\s]+", stderr, re.MULTILINE)))


def verify(state, results, expected_packages, require_check_convergence=False):
    state, results = Path(state).resolve(), Path(results).resolve()
    manifest = json.loads((state / "manifest.json").read_text())
    measurements = json.loads((results / "samples.json").read_text())
    run_manifest = json.loads((results / "manifest.json").read_text())
    entries = [json.loads(line) for line in (state / "journal.jsonl").read_text().splitlines()]
    preparations = {}
    errors = []
    if run_manifest.get("state") != "complete":
        errors.append("benchmark run did not complete successfully")
    if not measurements:
        errors.append("benchmark has no samples")
    for entry in entries:
        if entry["operation"] != "prepare" or entry["batch"] != str(results):
            continue
        key = (entry["stage"], entry["pair"], entry["variant"])
        if key in preparations:
            errors.append(f"duplicate fixture preparation: {key}")
        preparations[key] = entry
    records = []
    paired = {}
    for sample in measurements:
        key = (sample["stage"], sample["pair"], sample["variant"])
        entry = preparations.get(key)
        if entry is None:
            errors.append(f"missing fixture preparation: {key}")
            continue
        try:
            validate_build_command(sample["command"])
        except ValueError as error:
            errors.append(f"{key}: {error}")
        if require_check_convergence:
            import check_convergence
            try:
                check_convergence.verify_record(entry)
            except (ValueError, OSError, KeyError) as error:
                errors.append(f"{key}: invalid check preparation convergence: {error}")
        packages = compile_packages(Path(sample["stderr"]).read_text())
        if set(packages) != set(expected_packages):
            errors.append(f"{key}: compiled packages {packages}, expected {sorted(expected_packages)}")
        if sample["returncode"] != 0:
            errors.append(f"{key}: failed timed command")
        records.append({"stage": sample["stage"], "pair": sample["pair"], "variant": sample["variant"], "compiled_packages": packages, "before_sha256": entry["before_sha256"], "after_sha256": entry["after_sha256"], "target": entry["target"]})
        paired.setdefault((sample["stage"], sample["pair"]), {})[sample["variant"]] = entry
    sample_keys = {(sample["stage"], sample["pair"], sample["variant"]) for sample in measurements}
    if len(sample_keys) != len(measurements):
        errors.append("benchmark contains duplicate sample identifiers")
    if set(preparations) != sample_keys:
        errors.append("fixture preparations and benchmark sample identifiers differ")
    for key, entries in paired.items():
        if set(entries) != {"baseline", "candidate"}:
            errors.append(f"{key}: missing variant")
        elif any(entries["baseline"][field] != entries["candidate"][field] for field in ("before_sha256", "after_sha256")):
            errors.append(f"{key}: paired input content differs")
    restored = digest(Path(manifest["source"]).read_bytes()) == manifest["source_sha256"]
    if not restored:
        errors.append("original source has not been restored")
    report = {"valid": not errors, "expected_compiled_packages": sorted(expected_packages), "source_restored": restored, "samples": records, "errors": errors}
    write_json(results / "fixture-validation.json", report)
    if errors:
        raise ValueError("dirty fixture validation failed: " + "; ".join(errors))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    init = commands.add_parser("init")
    init.add_argument("--state", required=True)
    init.add_argument("--source", required=True)
    init.add_argument("--workspace", required=True)
    init.add_argument("--baseline-target", required=True)
    init.add_argument("--candidate-target", required=True)
    init.add_argument("--shared-build-dir", required=True)
    prep = commands.add_parser("prepare")
    prep.add_argument("--state", required=True)
    prep.add_argument("--batch", required=True)
    prep.add_argument("--stage", required=True, choices=("warmup", "measurement"))
    prep.add_argument("--pair", required=True, type=int)
    prep.add_argument("--variant", required=True, choices=("baseline", "candidate"))
    prep.add_argument("command", nargs=argparse.REMAINDER)
    cleanup = commands.add_parser("restore")
    cleanup.add_argument("--state", required=True)
    check = commands.add_parser("verify")
    check.add_argument("--state", required=True)
    check.add_argument("--results", required=True)
    check.add_argument("--expected-package", action="append", default=None)
    arguments = parser.parse_args()
    if arguments.operation == "init":
        result = initialize(arguments.state, arguments.source, arguments.workspace, {"baseline": arguments.baseline_target, "candidate": arguments.candidate_target}, arguments.shared_build_dir)
    elif arguments.operation == "prepare":
        command = arguments.command[1:] if arguments.command[:1] == ["--"] else arguments.command
        result = prepare(arguments.state, arguments.batch, arguments.stage, arguments.pair, arguments.variant, command)
    elif arguments.operation == "restore":
        result = restore(arguments.state)
    else:
        result = verify(arguments.state, arguments.results, arguments.expected_package or ["uv"])
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
