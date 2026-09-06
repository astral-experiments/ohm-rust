"""Check state restoration and timing boundaries for Cargo workflows."""

import json
import subprocess
import sys
from pathlib import Path

import pytest
from benchmarks import WORKSPACES
from pytest_codspeed.config import (
    BenchmarkMarkerOptions,
    CodSpeedConfig,
    PedanticOptions,
)
from pytest_codspeed.instruments import MeasurementMode, walltime
from test_workflow_walltime import test_workflow as run_benchmark
from verify_walltime import expected_cases, verify_walltime
from workflows import (
    SCENARIOS,
    copy_build,
    prepare_sample,
    run_commands,
    verify_sample,
    workflow_environment,
)


@pytest.fixture
def fixture(tmp_path):
    source, cwd, target = (
        tmp_path / name for name in ("source", "workspace", "target")
    )
    source.mkdir()
    cwd.mkdir()
    target.mkdir()
    edits = {
        "dependency": ("dependency.rs", "value = 1", "value = 2", "dependency"),
        "direct": ("direct.rs", "value = 3", "value = 4", "direct"),
    }
    for relative, before, _, _ in edits.values():
        (source / relative).write_text(before)
        (cwd / relative).write_text("leftover edit")
    snapshots = {}
    for seed in ("check", "test"):
        snapshot = tmp_path / f"{seed}-snapshot"
        snapshot.mkdir()
        (snapshot / "state").write_text(seed)
        snapshots[seed] = str(snapshot)
    return {
        "cwd": str(cwd),
        "source": str(source),
        "target": str(target),
        "edits": edits,
        "snapshots": snapshots,
        "env": {},
        "check": ["cargo", "check"],
        "test": ["cargo", "test", "--no-run"],
        "log": str(tmp_path / "sample.log"),
    }


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_repeated_samples_restore_source_and_mutable_build_state(fixture, scenario):
    target, cwd = Path(fixture["target"]), Path(fixture["cwd"])
    seed = "test" if scenario in ("DirectEditTest", "CheckTestCheck") else "check"
    snapshot = Path(fixture["snapshots"][seed])
    previous_log = None
    for _ in range(2):
        (target / "stale-output").touch()
        args, kwargs = prepare_sample(fixture, scenario)
        assert (target / "state").read_text() == seed
        assert not (target / "stale-output").exists()
        assert (target / "state").stat().st_ino != (snapshot / "state").stat().st_ino
        assert (target / "state").stat().st_mtime_ns == (
            snapshot / "state"
        ).stat().st_mtime_ns
        for kind, (relative, before, after, _) in fixture["edits"].items():
            edited = kind == (
                "dependency" if scenario == "DependencyEditCheck" else "direct"
            )
            assert (cwd / relative).read_text() == (after if edited else before)
            assert (Path(fixture["source"]) / relative).read_text() == before
            (cwd / relative).write_text("leftover edit")
        assert kwargs["log"] != previous_log
        previous_log = kwargs["log"]
        if scenario == "CheckTestCheck":
            assert args == ([fixture["check"], fixture["test"], fixture["check"]],)
        else:
            assert args == (
                [fixture["test"] if scenario == "DirectEditTest" else fixture["check"]],
            )
        (target / "state").write_text("mutated by the measured compiler")
        assert (snapshot / "state").read_text() == seed


def test_environment_ignores_local_compiler_flags_and_credentials(
    monkeypatch, tmp_path
):
    for key in (
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_BUILD_DIR",
        "CARGO_MAKEFLAGS",
        "CARGO_UNSTABLE_OHM_PROC_MACRO_TRUST",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "GITHUB_TOKEN",
    ):
        monkeypatch.setenv(key, "do-not-inherit")
    monkeypatch.setenv("CARGO_HOME", str(tmp_path / "cargo-home"))
    env = workflow_environment(tmp_path / "rustc", tmp_path / "target", 8)
    assert "do-not-inherit" not in env.values()
    assert env["CARGO_HOME"] == str(tmp_path / "cargo-home")
    assert env["CARGO_BUILD_JOBS"] == "8"
    assert env["CARGO_INCREMENTAL"] == "1"


def test_sequence_waits_and_stops_on_command_failure(tmp_path):
    marker = tmp_path / "must-not-run"
    commands = [
        [
            sys.executable,
            "-c",
            "print('first command', flush=True); raise SystemExit(7)",
        ],
        [sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"],
    ]
    log = tmp_path / "commands.log"
    with pytest.raises(subprocess.CalledProcessError) as error:
        run_commands(commands, str(tmp_path), {}, str(log))
    assert error.value.returncode == 7
    assert "first command" in log.read_text()
    assert not marker.exists()


def test_cargo_status_lines_do_not_prove_the_edited_crate_ran(fixture):
    log = Path(fixture["log"])
    log.write_text("Checking dependency v0.0.0\nFresh direct v0.0.0\n")
    with pytest.raises(ValueError, match="was not compiled"):
        verify_sample(fixture, "DependencyEditCheck")
    log.write_text("Running `/tools/rustc --crate-name dependency --emit=metadata`\n")
    verify_sample(fixture, "DependencyEditCheck")
    with pytest.raises(ValueError, match="was not compiled"):
        verify_sample(fixture, "DirectEditCheck")
    log.write_text("Running `/tools/rustc --crate-name direct --emit=metadata`\n")
    with pytest.raises(ValueError, match="No test harness"):
        verify_sample(fixture, "DirectEditTest")
    with log.open("a") as stream:
        stream.write("Running `/tools/rustc --crate-name dependency --test`\n")
    with pytest.raises(ValueError, match="No test harness"):
        verify_sample(fixture, "DirectEditTest")
    log.write_text("Running `/tools/rustc --crate-name direct --test`\n")
    verify_sample(fixture, "DirectEditTest")


def test_copy_rejects_overlapping_snapshot_and_target(tmp_path):
    for source, target in (
        (tmp_path, tmp_path),
        (tmp_path, tmp_path / "child"),
        (tmp_path / "child", tmp_path),
    ):
        with pytest.raises(ValueError, match="separate directories"):
            copy_build(source, target)
    assert tmp_path.exists()


def test_restore_and_validation_are_outside_measured_work(fixture, monkeypatch):
    instrument = walltime.WallTimeInstrument(CodSpeedConfig(), MeasurementMode.WallTime)
    now_ns = 0
    measured = False
    calls = []
    monkeypatch.setattr(walltime, "perf_counter_ns", lambda: now_ns)

    def run(commands, cwd, env, log):
        nonlocal now_ns
        assert measured
        calls.append(commands)
        Path(log).write_text("Running `/tools/rustc --crate-name direct --test`\n")
        now_ns += 10

    class Benchmark:
        def pedantic(self, target, *, setup, teardown, **options):
            def prepare():
                nonlocal now_ns, measured
                assert not measured
                result = setup()
                now_ns += 100
                measured = True
                return result

            def finish(*args, **kwargs):
                nonlocal now_ns, measured
                measured = False
                teardown(*args, **kwargs)
                now_ns += 1000

            return instrument.measure_pedantic(
                BenchmarkMarkerOptions(),
                PedanticOptions(
                    target=target, setup=prepare, teardown=finish, **options
                ),
                "workflow",
                "workflow",
            )

    monkeypatch.setattr("test_workflow_walltime.run_commands", run)
    run_benchmark(Benchmark(), {"example": fixture}, "example", "CheckTestCheck")
    assert len(calls) == 12
    assert all(
        commands == [fixture["check"], fixture["test"], fixture["check"]]
        for commands in calls
    )
    stats = instrument.benchmarks[0].stats
    assert (stats.warmup_iters, stats.rounds, stats.iter_per_round) == (1, 10, 1)
    assert stats.min_ns == stats.max_ns == 10


@pytest.mark.parametrize("shard", WORKSPACES)
def test_workflow_result_coverage_is_separate_from_leaf_compilation(tmp_path, shard):
    expected = expected_cases(shard, workflows=True)
    assert len(expected) == 4
    assert len(expected_cases(workflows=True)) == 8
    assert not expected & expected_cases()
    (tmp_path / "results").mkdir()
    path = tmp_path / "results/run.json"
    document = {
        "benchmarks": [
            {
                "uri": uri,
                "stats": {
                    "rounds": 10,
                    "iter_per_round": 1,
                    "warmup_iters": 1,
                    "min_ns": 1,
                    "mean_ns": 2,
                },
            }
            for uri in expected
        ]
    }
    path.write_text(json.dumps(document))
    assert verify_walltime(tmp_path, shard=shard, workflows=True) == 4
    with pytest.raises(ValueError, match="Unexpected"):
        verify_walltime(tmp_path, shard=shard)
    document["benchmarks"].pop()
    path.write_text(json.dumps(document))
    with pytest.raises(ValueError, match="Missing"):
        verify_walltime(tmp_path, shard=shard, workflows=True)
