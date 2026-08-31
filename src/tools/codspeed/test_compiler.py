"""Measure leaf rustc invocations with explicitly prepared incremental state."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest


FIXTURE_NAMES = ("bitmaps-3.2.1", "syn-2.0.101")
SCENARIOS = ("Full", "IncrFull", "IncrUnchanged", "IncrPatched")
LOADER_PATHS = ("LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH")
DEFAULT_MANIFEST = Path(__file__).resolve().parents[3] / "build/codspeed/manifest.json"


def compiler_environment(saved: dict[str, str]) -> dict[str, str]:
    """Restore Cargo's compile environment while retaining live CodSpeed hooks."""
    environment = os.environ.copy()
    for name, value in saved.items():
        if name == "LD_PRELOAD" or name.startswith(("CODSPEED_", "PYTHON")):
            continue
        if name in LOADER_PATHS:
            paths = [
                *value.split(os.pathsep),
                *environment.get(name, "").split(os.pathsep),
            ]
            value = os.pathsep.join(dict.fromkeys(path for path in paths if path))
        environment[name] = value

    # The Cargo process that owned these jobserver descriptors has already exited.
    for name in ("CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        environment.pop(name, None)
    return environment


def prepare_invocation(
    fixture: dict[str, Any], scenario: str
) -> tuple[tuple[list[str]], dict[str, Any]]:
    """Restore a scenario and return the arguments for its measured compiler call.

    CodSpeed can invoke an unmeasured warmup even in simulation mode. This function
    is the pedantic setup hook so both warmup and measurement start from the same
    source and incremental state.
    """
    if scenario not in SCENARIOS:
        raise ValueError(f"Unknown compiler benchmark scenario: {scenario}")

    source = Path(fixture["source"])
    cwd = Path(fixture["cwd"])
    incremental = Path(fixture["incremental"])
    command = list(fixture["command"])
    environment = compiler_environment(fixture["env"])

    # Cargo dependencies and outputs are outside this source-only directory.
    shutil.copytree(source, cwd, dirs_exist_ok=True)
    if incremental.exists():
        shutil.rmtree(incremental)
    if scenario != "Full":
        incremental.mkdir(parents=True)
        command.extend(["-C", f"incremental={incremental}"])

    kwargs = {"cwd": str(cwd), "env": environment, "check": True}
    if scenario in ("IncrUnchanged", "IncrPatched"):
        subprocess.run(command, **kwargs)

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

    return (command,), kwargs


@pytest.fixture(scope="session")
def compiler_fixtures() -> dict[str, dict[str, Any]]:
    manifest_path = Path(os.environ.get("RUSTC_CODSPEED_MANIFEST", DEFAULT_MANIFEST))
    manifest = json.loads(manifest_path.read_text())
    return {fixture["name"]: fixture for fixture in manifest["fixtures"]}


@pytest.mark.parametrize(
    ("fixture_name", "scenario"),
    [
        pytest.param(name, scenario, id=f"{name}-{scenario}")
        for name in FIXTURE_NAMES
        for scenario in SCENARIOS
    ],
)
def test_compile(benchmark, compiler_fixtures, fixture_name: str, scenario: str):
    fixture = compiler_fixtures[fixture_name]
    benchmark.pedantic(
        subprocess.run,
        setup=lambda: prepare_invocation(fixture, scenario),
        rounds=1,
        iterations=1,
        warmup_rounds=0,
    )
