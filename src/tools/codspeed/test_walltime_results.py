"""Check walltime case coverage and the sampling contract."""

import json
from pathlib import Path
import subprocess
import sys

import pytest

from benchmarks import SHARDS, selected_fixtures
from verify_walltime import EXPECTED_CASES, expected_cases, verify_walltime


@pytest.fixture
def results(tmp_path):
    (tmp_path / "results").mkdir()
    document = {
        "benchmarks": [
            {
                "uri": uri,
                "stats": {
                    "rounds": 10,
                    "iter_per_round": 1,
                    "warmup_iters": 1,
                    "min_ns": 1000,
                    "mean_ns": 2000,
                },
            }
            for uri in sorted(EXPECTED_CASES)
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
    assert result.stdout == "Verified 23 walltime benchmarks.\n"
    assert path.read_bytes() == original


def test_expected_cases_preserve_original_ids_and_full_only_fixtures():
    cases = {
        f"{name}-{scenario}"
        for name in (
            "bitmaps-3.2.1",
            "syn-2.0.101",
            "serde-1.0.219",
            "regex-automata-0.4.8",
        )
        for scenario in ("Full", "IncrFull", "IncrUnchanged", "IncrPatched")
    }
    cases.update(
        f"{name}-Debug-Full"
        for name in (
            "bitmaps-3.2.1",
            "syn-2.0.101",
            "serde-1.0.219",
            "regex-automata-0.4.8",
        )
    )
    cases.update(
        (
            "ruff-ty_python_semantic-Full",
            "uv-resolver-Full",
            "uv-release-resolver-Full",
        )
    )
    assert EXPECTED_CASES == {
        f"src/tools/codspeed/test_compiler_walltime.py::test_compile[{case}]"
        for case in cases
    }


def test_shards_cover_every_case_once():
    cases = [case for shard in SHARDS for case in expected_cases(shard)]
    assert len(SHARDS) == 7
    assert len(cases) == len(set(cases)) == 23
    assert set(cases) == EXPECTED_CASES
    assert [len(expected_cases(shard)) for shard in SHARDS] == [5, 5, 5, 5, 1, 1, 1]
    # The workflow selects shards with pytest -k, which matches substrings.
    for shard in SHARDS:
        assert {case for case in EXPECTED_CASES if shard in case} == expected_cases(shard)
    with pytest.raises(ValueError, match="Unknown compiler benchmark shard"):
        selected_fixtures("missing")


@pytest.mark.parametrize("shard", SHARDS)
def test_shard_results_reject_missing_and_unassigned_cases(results, shard):
    folder, path, document = results
    with pytest.raises(ValueError, match="Unexpected or duplicate"):
        verify_walltime(folder, shard=shard)
    document["benchmarks"] = [
        case for case in document["benchmarks"] if case["uri"] in expected_cases(shard)
    ]
    path.write_text(json.dumps(document))
    assert verify_walltime(folder, shard=shard) == len(expected_cases(shard))
    document["benchmarks"].pop()
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="Missing walltime benchmarks"):
        verify_walltime(folder, shard=shard)


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
