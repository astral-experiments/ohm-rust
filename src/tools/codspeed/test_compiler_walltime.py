"""Measure elapsed time for complete, independently prepared rustc invocations."""

import json
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

from benchmarks import FIXTURE_SCENARIOS, SCENARIOS

DEFAULT_MANIFEST = Path(__file__).resolve().parents[3] / "build/codspeed/manifest.json"


def prepare_invocation(
    fixture: dict[str, Any], scenario: str
) -> tuple[tuple[list[str]], dict[str, Any]]:
    """Restore source and incremental state before each compiler invocation."""
    if scenario not in SCENARIOS:
        raise ValueError(f"Unknown compiler benchmark scenario: {scenario}")

    source = Path(fixture["source"])
    cwd = Path(fixture["cwd"])
    incremental = Path(fixture["incremental"])
    command = list(fixture["command"])
    environment = os.environ | fixture["env"]
    # The Cargo process that owned these jobserver descriptors has already exited.
    for name in ("CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        environment.pop(name, None)

    # Cargo dependencies and outputs are outside this source-only directory.
    shutil.copytree(
        source, cwd, dirs_exist_ok=True, ignore=shutil.ignore_patterns(".git")
    )
    if incremental.exists():
        shutil.rmtree(incremental)
    if scenario != "Full":
        incremental.mkdir(parents=True)
        command.extend(["-C", f"incremental={incremental}"])

    kwargs = {"cwd": str(cwd), "env": environment}
    if scenario in ("IncrUnchanged", "IncrPatched"):
        subprocess.run(command, check=True, **kwargs)

    if scenario == "IncrPatched":
        patch_environment = environment.copy()
        # Otherwise git finds the enclosing rustc checkout and can silently skip
        # the patch because the fixture is beneath an ignored build directory.
        patch_environment["GIT_CEILING_DIRECTORIES"] = str(cwd.parent)
        patch_environment.pop("GIT_DIR", None)
        patch_environment.pop("GIT_WORK_TREE", None)
        subprocess.run(
            ["git", "apply", str(fixture["patch"])],
            cwd=cwd,
            env=patch_environment,
            check=True,
        )

    return (command,), {**kwargs, "check": True}


@pytest.fixture(scope="session")
def compiler_fixtures() -> dict[str, dict[str, Any]]:
    manifest_path = Path(os.environ.get("RUSTC_CODSPEED_MANIFEST", DEFAULT_MANIFEST))
    manifest = json.loads(manifest_path.read_text())
    return {fixture["name"]: fixture for fixture in manifest["fixtures"]}


@pytest.mark.parametrize(
    ("fixture_name", "scenario"),
    [
        pytest.param(name, scenario, id=f"{name}-{scenario}")
        for name, scenarios in FIXTURE_SCENARIOS.items()
        for scenario in scenarios
    ],
)
def test_compile(benchmark, compiler_fixtures, fixture_name: str, scenario: str):
    fixture = compiler_fixtures[fixture_name]
    benchmark.pedantic(
        subprocess.run,
        setup=lambda: prepare_invocation(fixture, scenario),
        rounds=10,
        iterations=1,
        warmup_rounds=1,
    )
