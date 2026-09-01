"""Check walltime result coverage and the native repetition/timing contract."""

import copy
import json
from pathlib import Path
import subprocess
import sys

import pytest

from verify_walltime import verify_walltime


@pytest.fixture
def results(tmp_path):
    folder = tmp_path / "profiles"
    (folder / "results").mkdir(parents=True)
    benchmarks = []
    for fixture in ("bitmaps-3.2.1", "syn-2.0.101"):
        for scenario in ("Full", "IncrFull", "IncrUnchanged", "IncrPatched"):
            name = f"test_compile[{fixture}-{scenario}]"
            benchmarks.append(
                {
                    "name": name,
                    "uri": f"src/tools/codspeed/test_compiler_walltime.py::{name}",
                    "config": {"max_time_ns": 3000000000, "max_rounds": 100},
                    "stats": {
                        "min_ns": 1000000,
                        "max_ns": 3000000,
                        "mean_ns": 2000000,
                        "stdev_ns": 500000,
                        "q1_ns": 1500000,
                        "median_ns": 2000000,
                        "q3_ns": 2500000,
                        "rounds": 10,
                        "total_time": 12345.0,
                        "iqr_outlier_rounds": 0,
                        "stdev_outlier_rounds": 0,
                        "iter_per_round": 1,
                        "warmup_iters": 1,
                    },
                }
            )
    document = {
        "creator": {"name": "pytest-codspeed", "version": "5.0.3", "pid": 123},
        "instrument": {"type": "walltime", "clock_info": {}},
        "benchmarks": benchmarks,
    }
    path = folder / "results/123.json"
    path.write_text(json.dumps(document))
    return folder, path, document


def test_keeps_native_stats_and_ignores_profiler_metadata(results):
    folder, _, document = results
    (folder / "walltime_metadata.json").write_text('{"profiler": "samply"}')
    evidence = verify_walltime(folder)
    assert evidence["instrument"] == "walltime"
    assert len(evidence["benchmarks"]) == 8
    assert evidence["benchmarks"][0]["stats"] == document["benchmarks"][0]["stats"]
    assert evidence["benchmarks"][0]["pytest_pid"] == 123
    assert evidence["stats_units"]["mean_ns"] == "nanoseconds"
    assert (
        evidence["stats_units"]["total_time"] == "seconds, including setup and teardown"
    )


@pytest.mark.parametrize("fault", ["missing", "duplicate", "unexpected"])
def test_rejects_incomplete_or_reused_cases(results, fault):
    folder, path, document = results
    if fault == "missing":
        document["benchmarks"].pop()
    elif fault == "unexpected":
        document["benchmarks"][0]["uri"] = "unrelated.py::test_compile"
    else:
        duplicate = copy.deepcopy(document)
        duplicate["creator"]["pid"] = 124
        duplicate["benchmarks"] = duplicate["benchmarks"][:1]
        (folder / "results/124.json").write_text(json.dumps(duplicate))
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="[Mm]issing|duplicate|unexpected"):
        verify_walltime(folder)


@pytest.mark.parametrize("fault", ["instrument", "creator"])
def test_rejects_wrong_result_owner_or_instrument(results, fault):
    folder, path, document = results
    if fault == "instrument":
        document["instrument"]["type"] = "simulation"
    else:
        document["creator"]["pid"] = 124
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="instrument|creator or PID"):
        verify_walltime(folder)


@pytest.mark.parametrize(
    ("field", "value"),
    [("rounds", 1), ("iter_per_round", 2), ("warmup_iters", 0)],
)
def test_rejects_wrong_repetition_protocol(results, field, value):
    folder, path, document = results
    document["benchmarks"][0]["stats"][field] = value
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match=f"expected {field}"):
        verify_walltime(folder)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("mean_ns", 0),
        ("median_ns", float("nan")),
        ("max_ns", float("inf")),
        ("stdev_ns", -1),
    ],
)
def test_rejects_invalid_measured_times(results, field, value):
    folder, path, document = results
    document["benchmarks"][0]["stats"][field] = value
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match=f"invalid timing statistic {field}"):
        verify_walltime(folder)


def test_rejects_absent_results(tmp_path):
    with pytest.raises(ValueError, match="Missing walltime benchmarks"):
        verify_walltime(tmp_path)


def test_cli_reports_sample_times_without_setup_time(results, monkeypatch):
    folder, _, _ = results
    output = folder / "audit.json"
    summary = folder / "summary.md"
    summary.write_text("Existing summary\n")
    monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(summary))
    result = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).with_name("verify_walltime.py")),
            str(folder),
            "--output",
            str(output),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    assert json.loads(output.read_text()) == verify_walltime(folder)
    assert (
        "| bitmaps-3.2.1-Full | 2.000 | 2.000 | 0.500 | 1.000 | 3.000 |"
        in result.stdout
    )
    assert "12345" not in result.stdout
    assert "not a compiler duration" in result.stdout
    assert result.stdout in summary.read_text()
    assert summary.read_text().startswith("Existing summary\n")
