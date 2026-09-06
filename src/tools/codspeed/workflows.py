"""Prepare reproducible Cargo edit/build workflows on pinned real workspaces."""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from benchmarks import WORKSPACES

# Keep Cargo fixed while comparing compiler revisions. This is Ohm 1.98.0-9's Cargo.
CARGO_REVISION = "49a7295fc6b46da600c6dca91d188a972d6abcc7"
SCENARIOS = (
    "DependencyEditCheck",
    "DirectEditCheck",
    "DirectEditTest",
    "CheckTestCheck",
)
EDITS = {
    "uv-resolver": {
        "dependency": (
            "crates/uv-normalize/src/lib.rs",
            "b'a'..=b'z' | b'0'..=b'9' => {\n                normalized.push",
            "b'0'..=b'9' | b'a'..=b'z' => {\n                normalized.push",
            "uv_normalize",
        ),
        "direct": (
            "crates/uv-resolver/src/dependency_mode.rs",
            "matches!(self, Self::Transitive)",
            "!matches!(self, Self::Direct)",
            "uv_resolver",
        ),
    },
    "ruff-ty_python_semantic": {
        "dependency": (
            "crates/ruff_python_trivia/src/whitespace.rs",
            "'\\r' | '\\n' => {",
            "'\\n' | '\\r' => {",
            "ruff_python_trivia",
        ),
        "direct": (
            "crates/ty_python_semantic/src/dunder_all.rs",
            "if arguments.len() != 1 {",
            "if 1 != arguments.len() {",
            "ty_python_semantic",
        ),
    },
}


def copy_build(source: Path, destination: Path) -> None:
    """Copy mutable build state without hard-linking it to the baseline."""
    if source.is_relative_to(destination) or destination.is_relative_to(source):
        raise ValueError(
            "Build snapshot and working target must be separate directories"
        )
    if destination.exists():
        shutil.rmtree(destination)
    if sys.platform == "linux":
        subprocess.run(
            ["cp", "-a", "--reflink=auto", str(source), str(destination)], check=True
        )
    elif sys.platform == "darwin":
        subprocess.run(["cp", "-cR", str(source), str(destination)], check=True)
    else:
        shutil.copytree(source, destination, symlinks=True)


def workflow_environment(rustc: Path, target: Path, jobs: int) -> dict[str, str]:
    environment = {
        key: value
        for key, value in os.environ.items()
        if key
        in {"PATH", "HOME", "USER", "LOGNAME", "TMPDIR", "TMP", "TEMP", "SYSTEMROOT"}
    }
    if "CARGO_HOME" in os.environ:
        environment["CARGO_HOME"] = os.environ["CARGO_HOME"]
    environment.update(
        RUSTC=str(rustc),
        RUSTC_BOOTSTRAP="1",
        CARGO_INCREMENTAL="1",
        CARGO_BUILD_JOBS=str(jobs),
        CARGO_TARGET_DIR=str(target),
        CARGO_TERM_COLOR="never",
        CARGO_TERM_PROGRESS_WHEN="never",
    )
    return environment


def run_commands(
    commands: list[list[str]], cwd: str, env: dict[str, str], log: str
) -> None:
    with Path(log).open("w") as stream:
        for command in commands:
            subprocess.run(
                command, cwd=cwd, env=env, stdout=stream, stderr=stream, check=True
            )


def prepare_sample(fixture: dict, scenario: str) -> tuple[tuple, dict]:
    if scenario not in SCENARIOS:
        raise ValueError(f"Unknown workflow scenario: {scenario}")
    cwd = Path(fixture["cwd"])
    for relative, _, _, _ in fixture["edits"].values():
        shutil.copy2(Path(fixture["source"]) / relative, cwd / relative)
    seed = "test" if scenario in ("DirectEditTest", "CheckTestCheck") else "check"
    copy_build(Path(fixture["snapshots"][seed]), Path(fixture["target"]))
    edit = fixture["edits"][
        "dependency" if scenario == "DependencyEditCheck" else "direct"
    ]
    relative, before, after, _ = edit
    path = cwd / relative
    contents = path.read_bytes()
    if not before or before == after or contents.count(before.encode()) != 1:
        raise ValueError(f"Expected one nontrivial edit in {path}")
    path.write_bytes(contents.replace(before.encode(), after.encode(), 1))
    fixture["samples"] = fixture.get("samples", 0) + 1
    fixture["log"] = str(cwd.parent / f"{scenario}-{fixture['samples']}.log")
    check, test = fixture["check"], fixture["test"]
    commands = (
        [check, test, check]
        if scenario == "CheckTestCheck"
        else [test if scenario == "DirectEditTest" else check]
    )
    return (commands,), {"cwd": str(cwd), "env": fixture["env"], "log": fixture["log"]}


def verify_sample(fixture: dict, scenario: str) -> None:
    log = Path(fixture["log"]).read_text()
    crate = fixture["edits"][
        "dependency" if scenario == "DependencyEditCheck" else "direct"
    ][3]
    # Cargo status lines may describe early cutoff. Require a compiler command.
    invocations = re.findall(
        r"Running `([^\n]*--crate-name " + re.escape(crate) + r"(?: |`)[^\n]*)", log
    )
    if not invocations:
        raise ValueError(
            f"The edited crate {crate} was not compiled; see {fixture['log']}"
        )
    if scenario in ("DirectEditTest", "CheckTestCheck") and not any(
        re.search(r"(?:^| )--test(?: |`|$)", command) for command in invocations
    ):
        raise ValueError(f"No test harness was compiled; see {fixture['log']}")


def prepare_fixture(
    name: str,
    source: Path,
    output: Path,
    target: Path,
    cargo: Path,
    rustc: Path,
    jobs: int,
) -> dict:
    if output.is_relative_to(source) or target.is_relative_to(source):
        raise ValueError("Workflow outputs must be outside the source checkout")
    spec = WORKSPACES[name]
    revision = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != spec["revision"]:
        raise ValueError(f"Unexpected source revision for {name}: {revision}")
    if subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain"], text=True
    ):
        raise ValueError(f"Workspace must have no local changes: {source}")
    cwd = output / "workspace"
    shutil.copytree(
        source, cwd, ignore=shutil.ignore_patterns(".git", "target", "__pycache__")
    )
    for relative, before, after, _ in EDITS[name].values():
        if (cwd / relative).read_bytes().count(before.encode()) != 1 or before == after:
            raise ValueError(f"Expected one nontrivial edit in {relative}")
        original = output / "originals" / relative
        original.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(cwd / relative, original)
    env = workflow_environment(rustc, target, jobs)
    common = ["--locked", "--offline", "--verbose", "-p", spec["package"]]
    if spec["features"]:
        common += ["--features", ",".join(spec["features"])]
    check = [str(cargo), "check", *common]
    test = [str(cargo), "test", *common, "--lib", "--no-run"]
    fixture = {
        "name": name,
        "source": str(output / "originals"),
        "source_revision": revision,
        "cwd": str(cwd),
        "target": str(target),
        "env": env,
        "edits": EDITS[name],
        "check": check,
        "test": test,
        "log": str(output / "sample.log"),
        "snapshots": {},
    }
    if target.exists():
        raise ValueError(f"Workflow target must be fresh: {target}")
    # Prefetch outside measurement; all warmups and samples are locked and offline.
    fetch_env = env | {
        key: value
        for key, value in os.environ.items()
        if key.lower() in {"http_proxy", "https_proxy", "all_proxy", "no_proxy"}
    }
    run_commands(
        [[str(cargo), "fetch", "--locked"]],
        str(cwd),
        fetch_env,
        str(output / "fetch.log"),
    )
    for seed, commands in (("check", [check]), ("test", [test, check])):
        run_commands(commands, str(cwd), env, str(output / f"warmup-{seed}.log"))
        snapshot = target.with_name(f"{target.name}-{seed}-snapshot")
        if snapshot.exists():
            raise ValueError(f"Workflow snapshot must be fresh: {snapshot}")
        copy_build(target, snapshot)
        fixture["snapshots"][seed] = str(snapshot)
    return fixture


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", type=Path, required=True)
    parser.add_argument("--cargo", type=Path, required=True)
    parser.add_argument(
        "--workspaces", type=Path, default=Path("build/codspeed-sources")
    )
    parser.add_argument("--output", type=Path, default=Path("build/codspeed-workflows"))
    parser.add_argument("--shard", choices=WORKSPACES)
    parser.add_argument("--jobs", type=int, default=8)
    args = parser.parse_args()
    rustc, cargo = args.rustc.resolve(strict=True), args.cargo.resolve(strict=True)
    output = args.output.resolve()
    if output.exists() or args.jobs <= 0:
        parser.error(
            "preparation requires a fresh output directory and positive job count"
        )
    cargo_version = subprocess.check_output(
        [str(cargo), "--version", "--verbose"], text=True
    )
    if f"commit-hash: {CARGO_REVISION}" not in cargo_version:
        parser.error(f"workflow Cargo must be built from {CARGO_REVISION}")
    output.mkdir(parents=True)
    target_root = (
        Path(os.environ.get("CARGO_TARGET_DIR", output / "targets")).resolve()
        / "workflows"
    )
    target_root.mkdir(parents=True, exist_ok=True)
    manifest = {
        "rustc_version": subprocess.check_output([str(rustc), "-vV"], text=True),
        "cargo_version": cargo_version,
        "cargo_revision": CARGO_REVISION,
        "jobs": args.jobs,
        "fixtures": [],
    }
    for name, spec in WORKSPACES.items():
        if args.shard and args.shard != name:
            continue
        fixture = prepare_fixture(
            name,
            (args.workspaces / spec["directory"]).resolve(),
            output / name,
            target_root / name,
            cargo,
            rustc,
            args.jobs,
        )
        manifest["fixtures"].append(fixture)
        (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"Prepared workflows for {name}", flush=True)


if __name__ == "__main__":
    main()
