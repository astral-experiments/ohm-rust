"""Check walltime case coverage and the sampling contract."""

import json
from pathlib import Path
import subprocess
import sys

import pytest

from verify_walltime import verify_walltime


@pytest.fixture
def results(tmp_path):
    (tmp_path / "results").mkdir()
    document = {
        "benchmarks": [
            {
                "uri": (
                    "src/tools/codspeed/test_compiler_walltime.py"
                    f"::test_compile[{name}-{scenario}]"
                ),
                "stats": {
                    "rounds": 10,
                    "iter_per_round": 1,
                    "warmup_iters": 1,
                    "min_ns": 1000,
                    "mean_ns": 2000,
                },
            }
            for name in ("bitmaps-3.2.1", "syn-2.0.101")
            for scenario in ("Full", "IncrFull", "IncrUnchanged", "IncrPatched")
        ]
    }
    path = tmp_path / "results/run.json"
    path.write_text(json.dumps(document))
    return tmp_path, path, document


def test_complete_results_cli(results):
    folder, path, _ = results
    original = path.read_bytes()
    (folder / "profiling.json").write_text("{}")
    result = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).with_name("verify_walltime.py")),
            str(folder),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    assert result.stdout == "Verified 8 walltime benchmarks.\n"
    assert path.read_bytes() == original


@pytest.mark.parametrize("fault", ["missing", "duplicate", "unexpected"])
def test_rejects_incomplete_or_reused_cases(results, fault):
    folder, path, document = results
    if fault == "missing":
        document["benchmarks"].pop()
    elif fault == "unexpected":
        document["benchmarks"][0]["uri"] = "unrelated.py::test_compile"
    else:
        (folder / "results/duplicate.json").write_text(json.dumps(document))
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="Missing|Unexpected or duplicate"):
        verify_walltime(folder)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("rounds", 1),
        ("iter_per_round", 2),
        ("warmup_iters", 0),
        ("min_ns", 0),
        ("mean_ns", -1),
        ("min_ns", float("inf")),
        ("mean_ns", float("nan")),
    ],
)
def test_rejects_invalid_samples(results, field, value):
    folder, path, document = results
    document["benchmarks"][0]["stats"][field] = value
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="expected 10 rounds|invalid"):
        verify_walltime(folder)
