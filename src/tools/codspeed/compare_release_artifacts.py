#!/usr/bin/env python3
"""Release artifact comparison against #107; no CodSpeed upload or acceptance gate."""

import argparse
import base64
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time
import urllib.request

REPO = Path(__file__).resolve().parents[3]
OUT = REPO / "build/release-comparison"
EVIDENCE = OUT / "evidence"
UV = "3392e29bdc4f09bb4f6d92f7aca8d450b835ce4f"
RUFF = "efcffd2178ce62e9951a53d4c50cadc225a0cfec"
CODEX = "74004b5397b24662a87a5264a6ae80664168c7f3"
SUITE = "c0301bc44d175b9b2c5442b25049475c39d7700c"
CANDIDATE_TREE = "ff43ac080b0e0d21fbcb7491ba5851224d78370c"
CANDIDATE_PATCH_SHA = "7a46d4bd984ac8134592bebb900cd6d44659115c2d3afd2ba6ca53210571084a"
BOOTSTRAP_SHA = "9aa59b04767b1d89a3bc57c52895020423a3a189130820e1e33fb733b47db169"
EXPECTED = {
    "A": {
        "revision": "4627d2156ce9cb9889f48c34f2ecf018150c6bb0",
        "run_id": 33842773781,
        "artifact_id": 9926674271,
        "bytes": 260457377,
        "digest": "sha256:aef36a546f0a4e5c8845f8a4bd825427c8f194ea96a9c3b985c517ed764fffdb",
    },
    "B": {
        "revision": "4627d2156ce9cb9889f48c34f2ecf018150c6bb0",
        "artifact_name": "codspeed-toolchain-candidate",
    },
}
TOOLCHAINS = {arm: OUT / "toolchains" / arm for arm in EXPECTED}
PREPARED = {arm: OUT / "prepared" / arm for arm in EXPECTED}


def save(name, value):
    (EVIDENCE / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def record(path):
    path = Path(path)
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest}


@contextmanager
def leaf_affinity(cpus):
    previous = os.sched_getaffinity(0)
    os.sched_setaffinity(0, cpus)
    try:
        actual = sorted(os.sched_getaffinity(0))
        if actual != cpus:
            raise ValueError("Leaf CPU affinity differs")
        yield actual
    finally:
        os.sched_setaffinity(0, previous)


def configure_candidate(raw):
    candidate = json.loads(raw)
    required = {"run_id", "artifact_id", "workflow_revision", "digest"}
    if (not isinstance(candidate, dict) or not required <= candidate.keys()
            or candidate.keys() - required - {"bytes"}
            or any(type(candidate[key]) is not int or candidate[key] <= 0
                   for key in ("run_id", "artifact_id"))
            or not re.fullmatch(r"[0-9a-f]{40}", candidate["workflow_revision"])
            or not re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", candidate["digest"])
            or ("bytes" in candidate and
                (type(candidate["bytes"]) is not int or candidate["bytes"] <= 0))):
        raise ValueError("Supply pinned candidate artifact ID, run, workflow revision and digest")
    candidate["digest"] = "sha256:" + candidate["digest"].removeprefix("sha256:")
    EXPECTED["B"].update(candidate)


def candidate_source(source, *, prepare):
    source = Path(source).resolve()
    git = lambda *args: subprocess.check_output(["git", "-C", str(source), *args], text=True).strip()
    patch = base64.b64decode(Path(__file__).with_name("mir-block-count.patch.b64").read_bytes().strip(), validate=True)
    if git("rev-parse", "HEAD") != EXPECTED["A"]["revision"] or hashlib.sha256(patch).hexdigest() != CANDIDATE_PATCH_SHA:
        raise ValueError("Candidate must start at exact #107 with the reviewed patch")
    if prepare:
        if git("status", "--porcelain", "--untracked-files=all"):
            raise ValueError("Candidate source checkout is not clean")
        subprocess.run(["git", "-C", str(source), "apply", "--index", "-"], input=patch, check=True)
    if (git("write-tree") != CANDIDATE_TREE or git("diff", "--name-only")
            or git("ls-files", "--others", "--exclude-standard")):
        raise ValueError("Candidate source differs from #107 plus the scheduling patch")
    if record(source / "src/tools/codspeed/bootstrap.toml")["sha256"] != BOOTSTRAP_SHA:
        raise ValueError("Candidate compiler recipe changed")
    return source


def record_candidate_build(source):
    source = candidate_source(source, prepare=False)
    stage = source / "build/host"
    version = subprocess.check_output([str(stage / "stage1/bin/rustc"), "-vV"], text=True).strip()
    fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
    if fields.get("commit-hash") != EXPECTED["A"]["revision"] or fields.get("host") != "aarch64-unknown-linux-gnu":
        raise ValueError("Candidate compiler source revision or host differs")
    files = [stage / "stage1/bin/rustc", stage / "stage0/bin/cargo"]
    files += sorted((stage / "stage1/lib").glob("librustc_driver*"))
    if len(files) < 3:
        raise ValueError("Candidate driver is missing")
    receipt = {
        "source_revision": EXPECTED["A"]["revision"], "source_tree": CANDIDATE_TREE,
        "patch_sha256": CANDIDATE_PATCH_SHA, "bootstrap_sha256": BOOTSTRAP_SHA,
        "workflow_revision": os.environ["GITHUB_SHA"], "run_id": int(os.environ["GITHUB_RUN_ID"]),
        "rustc_version": version,
        "files": {str(path.relative_to(stage)): {key: value for key, value in record(path).items()
                                                 if key != "path"} for path in files},
    }
    (source / "build/candidate-build.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


def verify_artifact(arm, artifact):
    expected = EXPECTED[arm]
    run = artifact["workflow_run"]
    if not (
        artifact["id"] == expected["artifact_id"]
        and artifact["name"] == expected.get("artifact_name", "codspeed-toolchain")
        and artifact["digest"] == expected["digest"]
        and artifact["size_in_bytes"] > 0
        and ("bytes" not in expected or artifact["size_in_bytes"] == expected["bytes"])
        and artifact["expired"] is False
        and run["id"] == expected["run_id"]
        and run["head_sha"] == expected.get("workflow_revision", expected["revision"])
        and run["repository_id"] == run["head_repository_id"] == 1319437392
    ):
        raise ValueError(f"Unexpected artifact provenance for {arm}")


def check_artifacts():
    # This mode alone receives the same-repository, read-only GitHub token.
    EVIDENCE.mkdir(parents=True, exist_ok=False)
    artifacts = {}
    for arm, expected in EXPECTED.items():
        request = urllib.request.Request(
            "https://api.github.com/repos/astral-sh/ohm-rust/actions/artifacts/"
            + str(expected["artifact_id"]),
            headers={
                "Authorization": "Bearer " + os.environ["GH_TOKEN"],
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            artifact = json.load(response)
        verify_artifact(arm, artifact)
        expected["bytes"] = artifact["size_in_bytes"]
        artifacts[arm] = artifact
        save("artifacts.json", artifacts)
    if "GITHUB_OUTPUT" in os.environ:
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write(f"candidate-run-id={EXPECTED['B']['run_id']}\n")
            output.write(f"candidate-artifact-id={EXPECTED['B']['artifact_id']}\n")


def source_records():
    records = {}
    for name, path, revision in (
        ("uv", REPO / "build/codspeed-sources/uv", UV),
        ("ruff", REPO / "build/codspeed-sources/ruff", RUFF),
        ("codex", REPO / "build/codspeed-sources/codex/codex-rs", CODEX),
        ("rustc-perf", REPO / "src/tools/rustc-perf", SUITE),
    ):
        actual = subprocess.check_output(
            ["git", "-C", str(path), "rev-parse", "HEAD"], text=True
        ).strip()
        changes = subprocess.check_output(
            ["git", "-C", str(path), "status", "--porcelain", "--untracked-files=all"],
            text=True,
        )
        if actual != revision or changes:
            raise ValueError(f"Unexpected source revision or changes: {name}")
        records[name] = {"revision": actual, "status": changes}
        if name != "rustc-perf":
            records[name]["lockfile"] = record(path / "Cargo.lock")
    return records


def normalized(arm, value):
    # Only the two separately prepared roots are interchangeable. Crate hashes,
    # flags, relative paths, and every captured environment value must match.
    if isinstance(value, str):
        return value.replace(str(TOOLCHAINS[arm]), "$TOOLCHAIN").replace(
            str(PREPARED[arm]), "$PREPARED"
        )
    if isinstance(value, list):
        return [normalized(arm, item) for item in value]
    if isinstance(value, dict):
        return {key: normalized(arm, item) for key, item in value.items()}
    return value


def main(shard, cycles):
    from benchmarks import WORKSPACES
    from capture import COMPILE_ENV
    from test_compiler_walltime import prepare_invocation

    if platform.system() != "Linux" or platform.machine() != "aarch64":
        raise ValueError("These artifacts require the Linux ARM64 Macro runner")
    leaf_cpus = sorted(os.sched_getaffinity(0))[:4]
    if len(leaf_cpus) != 4:
        raise ValueError("Four available CPUs are required")
    artifacts = json.loads((EVIDENCE / "artifacts.json").read_text())
    for arm in EXPECTED:
        verify_artifact(arm, artifacts[arm])
        EXPECTED[arm]["bytes"] = artifacts[arm]["size_in_bytes"]

    # prepare.py and prepare_invocation inherit os.environ. Replace it before
    # either runs: no Actions tokens, private registry configuration, loader
    # overrides, user Cargo credentials, jobserver handles, or compiler flags.
    cargo_home = OUT / "public-cargo"
    cargo_home.mkdir(exist_ok=False)
    temporary = OUT / "tmp"
    temporary.mkdir(exist_ok=False)
    environment = {
        "PATH": os.environ["PATH"],
        "HOME": os.environ["HOME"],
        "CARGO_HOME": str(cargo_home),
        "TMPDIR": str(temporary),
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TZ": "UTC",
        "CARGO_TERM_COLOR": "never",
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    os.environ.clear()
    os.environ.update(environment)
    before = source_records()
    save("sources-before.json", before)
    save("environment.json", environment)
    save("harness.json", {
        "checkout": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "files": [record(path) for path in sorted(Path(__file__).parent.glob("*.py"))]
        + [record(Path(__file__).with_name("requirements.txt"))],
        "machine": platform.uname()._asdict(),
        "cpu_affinity": sorted(os.sched_getaffinity(0)),
        "leaf_cpu_affinity": leaf_cpus,
    })

    runtimes, fixtures, invocations, dependencies = {}, {}, {}, {}
    for arm, expected in EXPECTED.items():
        toolchain = TOOLCHAINS[arm]
        rustc = toolchain / "stage1/bin/rustc"
        cargo = toolchain / "stage0/bin/cargo"
        version = subprocess.check_output([str(rustc), "-vV"], text=True).strip()
        fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
        if fields.get("commit-hash") != expected["revision"] or fields.get("host") != "aarch64-unknown-linux-gnu":
            raise ValueError(f"Wrong compiler revision/host for {arm}: {version}")
        runtimes[arm] = {
            "rustc_version": version,
            "version_fields": fields,
            "rustc": record(rustc),
            "cargo": record(cargo),
            "cargo_version": subprocess.check_output([str(cargo), "--version", "--verbose"], text=True).strip(),
            "archive": record(OUT / "downloads" / arm / "codspeed-toolchain.tar.gz"),
            "driver": [record(path) for path in sorted((toolchain / "stage1/lib").glob("librustc_driver*"))],
        }
    if runtimes["A"]["cargo"]["sha256"] != runtimes["B"]["cargo"]["sha256"]:
        raise ValueError("The artifacts contain different Cargo binaries")
    comparable = lambda arm: {key: value for key, value in runtimes[arm]["version_fields"].items() if key not in {"commit-hash", "commit-date"}}
    if comparable("A") != comparable("B"):
        raise ValueError("Compiler release/host/LLVM versions differ")
    build_path = OUT / "downloads/B/candidate-build.json"
    build = json.loads(build_path.read_text())
    expected_build = {
        "source_revision": EXPECTED["A"]["revision"], "source_tree": CANDIDATE_TREE,
        "patch_sha256": CANDIDATE_PATCH_SHA, "bootstrap_sha256": BOOTSTRAP_SHA,
        "workflow_revision": EXPECTED["B"]["workflow_revision"], "run_id": EXPECTED["B"]["run_id"],
        "rustc_version": runtimes["B"]["rustc_version"],
        "files": {str(Path(item["path"]).relative_to(TOOLCHAINS["B"])):
                  {key: value for key, value in item.items() if key != "path"}
                  for item in [runtimes["B"]["rustc"], runtimes["B"]["cargo"], *runtimes["B"]["driver"]]},
    }
    if build != expected_build:
        raise ValueError("Candidate artifact does not match the reviewed compiler build")
    shutil.copy2(build_path, EVIDENCE / "candidate-build.json")
    save("runtimes.json", runtimes)

    for arm in EXPECTED:
        command = [
            sys.executable, str(Path(__file__).with_name("prepare.py")),
            "--rustc", runtimes[arm]["rustc"]["path"],
            "--cargo", runtimes[arm]["cargo"]["path"],
            "--shard", shard, "--output", str(PREPARED[arm]),
        ]
        save(f"prepare-{arm}-command.json", {"argv": command, "env": environment})
        print(f"Preparing {shard} for arm {arm}", flush=True)
        with (EVIDENCE / f"prepare-{arm}.stdout.txt").open("wb") as stdout, (EVIDENCE / f"prepare-{arm}.stderr.txt").open("wb") as stderr:
            subprocess.run(command, check=True, stdout=stdout, stderr=stderr)
        print(f"Prepared {shard} for arm {arm}", flush=True)
        manifest_path = PREPARED[arm] / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        if len(manifest["fixtures"]) != 1:
            raise ValueError("Expected exactly one prepared Release fixture")
        fixture = manifest["fixtures"][0]
        if not (fixture["name"] == shard and fixture["profile"] == "release" and fixture["source_revision"] == WORKSPACES[shard]["revision"]):
            raise ValueError("Wrong Release fixture")
        if any(key not in COMPILE_ENV and not key.startswith(("CARGO_PKG_", "CARGO_FEATURE_")) for key in fixture["env"]):
            raise ValueError("Unreviewed environment key in captured manifest")
        if fixture["command"][0] != runtimes[arm]["rustc"]["path"]:
            raise ValueError("Captured command uses another compiler")
        if "opt-level=3" not in fixture["command"] or "--emit=dep-info,metadata,link" not in fixture["command"]:
            raise ValueError("Captured command is not the expected optimized library build")
        shutil.copy2(manifest_path, EVIDENCE / f"manifest-{arm}.json")
        shutil.copy2(PREPARED[arm] / f"{shard}.json", EVIDENCE / f"capture-{arm}.json")
        fixtures[arm] = fixture
        args, kwargs = prepare_invocation(fixture, "Full")
        invocations[arm] = {"argv": args[0], "cwd": kwargs["cwd"], "env": kwargs["env"]}
        save(f"invocation-{arm}.json", invocations[arm])
        command = fixture["command"]
        dependencies[arm] = [record(command[index + 1].split("=", 1)[1]) for index, arg in enumerate(command) if arg == "--extern"]
    save("dependencies-before.json", dependencies)
    comparison = {arm: normalized(arm, value) for arm, value in invocations.items()}
    save("normalized-invocations.json", comparison)
    if comparison["A"] != comparison["B"]:
        raise ValueError("A/B normalized compiler arguments or environments differ")

    plan = [("warmup", arm) for arm in "AB"] + [("measured", arm) for arm in "ABBA" * cycles]
    result = {
        "status": "running", "role": "descriptive walltime comparison",
        "baseline": EXPECTED["A"], "candidate": EXPECTED["B"],
        "case": f"{shard}-Full", "scenario": "Full",
        "planned_warmups": 2, "planned_measured": 4 * cycles,
        "preparation_leaf_compilations": 2,
        "leaf_cpu_affinity": leaf_cpus,
        "order": "warm AB; measured " + " ".join(["ABBA"] * cycles),
        "acceptance_evaluated": False, "codspeed_upload": False, "calls": [],
    }
    save("results.json", result)
    for index, (phase, arm) in enumerate(plan):
        args, kwargs = prepare_invocation(fixtures[arm], "Full")
        actual = {"argv": args[0], "cwd": kwargs["cwd"], "env": kwargs["env"]}
        if actual != invocations[arm]:
            raise ValueError("Compiler invocation changed after admission")
        stdout_path = EVIDENCE / f"call-{index:02d}-{phase}-{arm}.stdout.txt"
        stderr_path = EVIDENCE / f"call-{index:02d}-{phase}-{arm}.stderr.txt"
        row = {"index": index, "phase": phase, "arm": arm}
        with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr, leaf_affinity(leaf_cpus) as affinity:
            row["cpu_affinity"] = affinity
            start = time.perf_counter_ns()
            try:
                child = subprocess.run(*args, **kwargs, stdout=stdout, stderr=stderr)
                stop = time.perf_counter_ns()
                row["returncode"] = child.returncode
            except BaseException as error:
                stop = time.perf_counter_ns()
                row["error"] = type(error).__name__
                row["returncode"] = getattr(error, "returncode", None)
                result["status"] = "failed"
                raise
            finally:
                row["elapsed_ns"] = stop - start
                row["stdout"] = str(stdout_path)
                row["stderr"] = str(stderr_path)
                result["calls"].append(row)
                save("results.json", result)
        print(f"{index + 1}/{len(plan)} {phase} {arm}: {row['elapsed_ns']} ns", flush=True)

    after = source_records()
    save("sources-after.json", after)
    if before != after:
        raise ValueError("Pinned source changed during comparison")
    for arm, records in dependencies.items():
        if [record(item["path"]) for item in records] != records:
            raise ValueError(f"Prepared dependencies changed: {arm}")
        for key in ("rustc", "cargo"):
            if record(runtimes[arm][key]["path"]) != runtimes[arm][key]:
                raise ValueError(f"Runtime changed: {arm} {key}")
        if [record(item["path"]) for item in runtimes[arm]["driver"]] != runtimes[arm]["driver"]:
            raise ValueError(f"Compiler driver changed: {arm}")
    result["status"] = "complete"
    result["source_runtime_dependencies_preserved"] = True
    save("results.json", result)
    save("evidence-files.json", [record(path) for path in sorted(EVIDENCE.iterdir()) if path.is_file()])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--prepare-candidate-source", type=Path)
    mode.add_argument("--record-candidate-build", type=Path)
    parser.add_argument("--check-artifacts", action="store_true")
    parser.add_argument("--candidate-artifact", default="")
    parser.add_argument("--cycles", type=int, choices=(1, 3), default=3)
    parser.add_argument("--shard", choices=("uv-release-resolver", "codex-release-protocol"), default="uv-release-resolver")
    args = parser.parse_args()
    if args.prepare_candidate_source:
        candidate_source(args.prepare_candidate_source, prepare=True)
    elif args.record_candidate_build:
        record_candidate_build(args.record_candidate_build)
    else:
        configure_candidate(args.candidate_artifact)
        if args.check_artifacts:
            check_artifacts()
        else:
            main(args.shard, args.cycles)
