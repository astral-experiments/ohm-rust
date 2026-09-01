"""Measure elapsed time for complete, independently prepared rustc invocations."""

import subprocess

import pytest
import test_compiler

compiler_fixtures = test_compiler.compiler_fixtures


@pytest.mark.parametrize(
    ("fixture_name", "scenario"),
    [
        pytest.param(name, scenario, id=f"{name}-{scenario}")
        for name in test_compiler.FIXTURE_NAMES
        for scenario in test_compiler.SCENARIOS
    ],
)
def test_compile(benchmark, compiler_fixtures, fixture_name: str, scenario: str):
    fixture = compiler_fixtures[fixture_name]
    benchmark.pedantic(
        subprocess.run,
        setup=lambda: test_compiler.prepare_invocation(fixture, scenario),
        rounds=10,
        iterations=1,
        warmup_rounds=1,
    )
