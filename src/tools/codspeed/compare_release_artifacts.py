#!/usr/bin/env python3
"""One-off #107/#108 UV Release comparison; no CodSpeed upload or acceptance gate."""

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import urllib.request

REPO = Path(__file__).resolve().parents[3]
OUT = REPO / "build/uv-release-comparison"
EVIDENCE = OUT / "evidence"
UV = "3392e29bdc4f09bb4f6d92f7aca8d450b835ce4f"
RUFF = "efcffd2178ce62e9951a53d4c50cadc225a0cfec"
SUITE = "c0301bc44d175b9b2c5442b25049475c39d7700c"
EXPECTED = {
    "A": {
        "revision": "4627d2156ce9cb9889f48c34f2ecf018150c6bb0",
        "run_id": 33842773781,
        "artifact_id": 9926674271,
        "bytes": 260457377,
        "digest": "sha256:aef36a546f0a4e5c8845f8a4bd825427c8f194ea96a9c3b985c517ed764fffdb",
    },
    "B": {
        "revision": "66c1849a85ca82fc00552f5b46f27eeab858d557",
        "run_id": 33897058066,
        "artifact_id": 9946685343,
        "bytes": 260342093,
        "digest": "sha256:ee806476abb3ea731081c2124f01f6d843199c29437abbd54e66b521c4c8e818",
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


def verify_artifact(arm, artifact):
    expected = EXPECTED[arm]
    run = artifact["workflow_run"]
    if not (
        artifact["id"] == expected["artifact_id"]
        and artifact["name"] == "codspeed-toolchain"
        and artifact["digest"] == expected["digest"]
        and artifact["size_in_bytes"] == expected["bytes"]
        and artifact["expired"] is False
        and run["id"] == expected["run_id"]
        and run["head_sha"] == expected["revision"]
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
        artifacts[arm] = artifact
        save("artifacts.json", artifacts)


def source_records():
    records = {}
    for name, path, revision in (
        ("uv", REPO / "build/codspeed-sources/uv", UV),
        ("ruff", REPO / "build/codspeed-sources/ruff", RUFF),
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


def main():
    from capture import COMPILE_ENV
    from test_compiler_walltime import prepare_invocation

    if platform.system() != "Linux" or platform.machine() != "aarch64":
        raise ValueError("These artifacts require the Linux ARM64 Macro runner")
    artifacts = json.loads((EVIDENCE / "artifacts.json").read_text())
    for arm in EXPECTED:
        verify_artifact(arm, artifacts[arm])

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
    save("runtimes.json", runtimes)

    for arm in EXPECTED:
        command = [
            sys.executable, str(Path(__file__).with_name("prepare.py")),
            "--rustc", runtimes[arm]["rustc"]["path"],
            "--cargo", runtimes[arm]["cargo"]["path"],
            "--shard", "uv-release-resolver", "--output", str(PREPARED[arm]),
        ]
        save(f"prepare-{arm}-command.json", {"argv": command, "env": environment})
        with (EVIDENCE / f"prepare-{arm}.stdout.txt").open("wb") as stdout, (EVIDENCE / f"prepare-{arm}.stderr.txt").open("wb") as stderr:
            subprocess.run(command, check=True, stdout=stdout, stderr=stderr)
        manifest_path = PREPARED[arm] / "manifest.json"
        manifest = json.loads(manifest_path.read_text())
        if len(manifest["fixtures"]) != 1:
            raise ValueError("Expected exactly one prepared Release fixture")
        fixture = manifest["fixtures"][0]
        if not (fixture["name"] == "uv-release-resolver" and fixture["profile"] == "release" and fixture["source_revision"] == UV):
            raise ValueError("Wrong Release fixture")
        if any(key not in COMPILE_ENV and not key.startswith(("CARGO_PKG_", "CARGO_FEATURE_")) for key in fixture["env"]):
            raise ValueError("Unreviewed environment key in captured manifest")
        if fixture["command"][0] != runtimes[arm]["rustc"]["path"]:
            raise ValueError("Captured command uses another compiler")
        if "opt-level=3" not in fixture["command"] or "--emit=dep-info,metadata,link" not in fixture["command"]:
            raise ValueError("Captured command is not the expected optimized library build")
        shutil.copy2(manifest_path, EVIDENCE / f"manifest-{arm}.json")
        shutil.copy2(PREPARED[arm] / "uv-release-resolver.json", EVIDENCE / f"capture-{arm}.json")
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

    plan = [("warmup", arm) for arm in "AB"] + [("measured", arm) for arm in "ABBA" * 3]
    result = {
        "status": "running", "role": "one-off descriptive walltime comparison",
        "baseline": EXPECTED["A"], "candidate": EXPECTED["B"],
        "case": "uv-release-resolver-Full", "scenario": "Full",
        "planned_warmups": 2, "planned_measured": 12,
        "preparation_leaf_compilations": 2,
        "order": "warm AB; measured ABBA ABBA ABBA",
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
        with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
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
        print(f"{index + 1}/14 {phase} {arm}: {row['elapsed_ns']} ns", flush=True)

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
    if sys.argv[1:] == ["--check-artifacts"]:
        check_artifacts()
    elif not sys.argv[1:]:
        main()
    else:
        raise SystemExit("Only --check-artifacts or the default comparison is supported")
