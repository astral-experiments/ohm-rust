"""Time complete Cargo workflows, including rebuild decisions and linking."""

import json
import os
from pathlib import Path

import pytest
from benchmarks import WORKSPACES
from workflows import SCENARIOS, prepare_sample, run_commands, verify_sample


@pytest.fixture(scope="session")
def workflow_fixtures():
    path = Path(
        os.environ.get(
            "RUSTC_CODSPEED_WORKFLOWS", "build/codspeed-workflows/manifest.json"
        )
    )
    manifest = json.loads(path.read_text())
    return {fixture["name"]: fixture for fixture in manifest["fixtures"]}


@pytest.mark.parametrize(
    ("workspace", "scenario"),
    [
        pytest.param(name, scenario, id=f"{name}-{scenario}")
        for name in WORKSPACES
        for scenario in SCENARIOS
    ],
)
def test_workflow(benchmark, workflow_fixtures, workspace, scenario):
    fixture = workflow_fixtures[workspace]
    benchmark.pedantic(
        run_commands,
        setup=lambda: prepare_sample(fixture, scenario),
        teardown=lambda *_args, **_kwargs: verify_sample(fixture, scenario),
        rounds=10,
        iterations=1,
        warmup_rounds=1,
    )
