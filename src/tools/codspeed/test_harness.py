"""Regression tests for the source/cache boundaries of compiler measurements."""

import subprocess
import sys
from pathlib import Path

import pytest
from pytest_codspeed.config import (
    BenchmarkMarkerOptions,
    CodSpeedConfig,
    PedanticOptions,
)
from pytest_codspeed.instruments import MeasurementMode, walltime

from benchmarks import SCENARIOS
from test_compiler_walltime import prepare_invocation
from test_compiler_walltime import test_compile as run_benchmark


@pytest.fixture
def fixture(tmp_path):
    source = tmp_path / "source"
    (source / "src").mkdir(parents=True)
    (source / "src/lib.rs").write_text("fn value() -> u8 { 1 }\n")
    (source / "0-println.patch").write_text(
        "diff --git a/src/lib.rs b/src/lib.rs\n"
        "--- a/src/lib.rs\n"
        "+++ b/src/lib.rs\n"
        "@@ -1 +1 @@\n"
        "-fn value() -> u8 { 1 }\n"
        "+fn value() -> u8 { 2 }\n"
    )
    # The real benchmark work directory is inside the compiler checkout. Git
    # must apply the fixture patch there instead of silently skipping its paths.
    repository = tmp_path / "repository"
    subprocess.run(["git", "init", "--quiet", str(repository)], check=True)
    cwd = repository / "build/fixtures/example"
    return {
        "source": str(source),
        "cwd": str(cwd),
        "command": [sys.executable, "-c", "pass"],
        "env": {},
        "incremental": str(repository / "build/incremental/example"),
        "patch": str(cwd / "0-println.patch"),
    }


def test_failed_seed_aborts_the_benchmark(fixture):
    fixture["command"] = [sys.executable, "-c", "raise SystemExit(7)"]
    with pytest.raises(subprocess.CalledProcessError) as error:
        prepare_invocation(fixture, "IncrUnchanged")
    assert error.value.returncode == 7


def test_full_workspace_invocation_needs_no_patch(fixture):
    source = Path(fixture["source"])
    (source / "0-println.patch").unlink()
    del fixture["patch"]
    member = source / "crates/member/src"
    member.mkdir(parents=True)
    (member / "lib.rs").write_text("pub fn member() {}\n")
    fixture["command"] = [
        sys.executable,
        "-c",
        "from pathlib import Path; "
        "assert Path('crates/member/src/lib.rs').read_text() == 'pub fn member() {}\\n'",
    ]
    incremental = Path(fixture["incremental"])
    incremental.mkdir(parents=True)
    (incremental / "previous-output").touch()

    args, kwargs = prepare_invocation(fixture, "Full")

    assert args == (fixture["command"],)
    assert not incremental.exists()
    assert not (Path(fixture["cwd"]) / "0-println.patch").exists()
    subprocess.run(*args, **kwargs)


@pytest.mark.parametrize("exit_code", [0, 7])
def test_compiler_completion_and_failure_are_measured(fixture, monkeypatch, exit_code):
    fixture["command"] = [sys.executable, "-c", f"raise SystemExit({exit_code})"]
    measured = False
    processes = []
    popen = subprocess.Popen

    class CompilerProcess(popen):
        def __init__(self, *args, **kwargs):
            assert measured
            super().__init__(*args, **kwargs)
            processes.append(self)

        def wait(self, *args, **kwargs):
            assert measured
            return super().wait(*args, **kwargs)

    class Benchmark:
        def pedantic(self, target, *, setup, **_options):
            nonlocal measured
            args, kwargs = setup()
            measured = True
            try:
                assert target(*args, **kwargs).returncode == 0
            finally:
                measured = False

    monkeypatch.setattr(subprocess, "Popen", CompilerProcess)
    if exit_code:
        with pytest.raises(subprocess.CalledProcessError) as error:
            run_benchmark(Benchmark(), {"example": fixture}, "example", "Full")
        assert error.value.returncode == exit_code
    else:
        run_benchmark(Benchmark(), {"example": fixture}, "example", "Full")
    assert len(processes) == 1
    assert processes[0].returncode == exit_code


@pytest.mark.parametrize("scenario", SCENARIOS)
def test_sampling_restores_state_outside_measurement(fixture, monkeypatch, scenario):
    instrument = walltime.WallTimeInstrument(CodSpeedConfig(), MeasurementMode.WallTime)
    run = subprocess.run
    incremental = Path(fixture["incremental"])
    in_target = False
    seeds = []
    invocations = []
    now_ns = 0
    seeded = scenario in ("IncrUnchanged", "IncrPatched")
    fixture["env"] = {"CARGO_PKG_NAME": "example", "LD_LIBRARY_PATH": "/compiler/lib"}
    monkeypatch.setenv("LD_LIBRARY_PATH", "/old/lib")
    for name in ("CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"):
        monkeypatch.setenv(name, "--jobserver-auth=8,9")
    monkeypatch.setattr(walltime, "perf_counter_ns", lambda: now_ns)

    def record_compile(command, **kwargs):
        nonlocal now_ns
        if command[0] != sys.executable:
            return run(command, **kwargs)
        assert kwargs["check"] is True
        assert kwargs["env"]["CARGO_PKG_NAME"] == "example"
        assert kwargs["env"]["LD_LIBRARY_PATH"] == "/compiler/lib"
        assert not {"CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS"} & kwargs["env"].keys()
        assert any(arg.startswith("incremental=") for arg in command) == (
            scenario != "Full"
        )
        source = Path(kwargs["cwd"]) / "src/lib.rs"
        if in_target:
            value = 2 if scenario == "IncrPatched" else 1
            assert source.read_text() == f"fn value() -> u8 {{ {value} }}\n"
            if scenario == "Full":
                assert not incremental.exists()
            else:
                assert sorted(path.name for path in incremental.iterdir()) == (
                    ["seed"] if seeded else []
                )
            invocations.append(source.read_text())
            incremental.mkdir(parents=True, exist_ok=True)
            (incremental / "previous-output").touch()
            source.write_text("changed by a previous invocation\n")
            now_ns += 10
        else:
            assert source.read_text() == "fn value() -> u8 { 1 }\n"
            assert not list(incremental.iterdir())
            seeds.append(source.read_text())
            (incremental / "seed").touch()
        return subprocess.CompletedProcess(command, 0)

    class Benchmark:
        def pedantic(self, target, *, setup, **options):
            def prepare():
                nonlocal in_target, now_ns
                in_target = False
                args = setup()
                now_ns += 100
                in_target = True
                return args

            return instrument.measure_pedantic(
                BenchmarkMarkerOptions(),
                PedanticOptions(target=target, setup=prepare, teardown=None, **options),
                "example",
                "example",
            )

    monkeypatch.setattr(subprocess, "run", record_compile)
    run_benchmark(Benchmark(), {"example": fixture}, "example", scenario)

    # The pinned plugin also makes one untimed call to obtain the result.
    assert len(invocations) == 12
    assert len(seeds) == (12 if seeded else 0)
    stats = instrument.benchmarks[0].stats
    assert (stats.warmup_iters, stats.rounds, stats.iter_per_round) == (1, 10, 1)
    # Only the target's ten clock ticks count, not setup's hundred ticks.
    assert stats.min_ns == stats.max_ns == 10


@pytest.mark.parametrize("outcome,count", [("pass", 40), ("early", 20), ("late", 40), ("compiler", 7), ("timeout", 7), ("diagnostics", 11), ("source_mutation", 7), ("hash_failure", 7), ("short", 6)])
def test_release_confirmation_cadence_and_cleanup(fixture, tmp_path, monkeypatch, outcome, count):
    from contextlib import nullcontext
    import hashlib
    import json
    import compare_release_artifacts as comparison
    import test_compiler_walltime as walltime_helper

    original, edited = b"pub fn value() -> u8 { 1 }\n", b"pub fn value() -> u8 { let n = 1; n }\n"
    source = Path(fixture["source"]) / "protocol/src/tool_name.rs"
    source.parent.mkdir(parents=True)
    source.write_bytes(original)
    edit = {"before": original.decode(), "after": edited.decode(), "occurrences": 1,
            "original_sha256": hashlib.sha256(original).hexdigest(),
            "edited_sha256": hashlib.sha256(edited).hexdigest()}
    fixtures = {arm: fixture | {"cwd": str(tmp_path / arm)} for arm in "AB"}
    invocations = {}
    for arm in "AB":
        args, kwargs = prepare_invocation(fixtures[arm], "Full")
        invocations[arm] = {"argv": args[0], "cwd": kwargs["cwd"], "env": kwargs["env"]}
    evidence = tmp_path / "evidence"
    evidence.mkdir()
    monkeypatch.setattr(comparison, "EVIDENCE", evidence)
    monkeypatch.setattr(comparison, "PREPARED", {arm: tmp_path / arm for arm in "AB"})
    monkeypatch.setattr(comparison, "leaf_affinity", nullcontext)
    confirm = outcome != "short"
    plan = comparison.comparison_plan(1, confirm)
    if confirm:
        assert [(arm, phase) for _, arm, _, phase in plan[::5]] == [(arm, phase) for arm in "ABBA" for phase in ("original", "edited")]
    else:
        assert plan == [("warmup", "A"), ("warmup", "B")] + [("measured", arm) for arm in "ABBA"]
    result = {"status": "running", "calls": [], "blocks": []}
    now, observed, fail_next_record = 0, [], False
    record = comparison.record

    def record_source(path):
        nonlocal fail_next_record
        if fail_next_record:
            fail_next_record = False
            raise OSError("mock post-call source read failure")
        return record(path)

    def setup(fixture, scenario):
        nonlocal now
        now += 10000  # Setup must not enter the measured interval.
        args, kwargs = prepare_invocation(fixture, scenario)
        assert (Path(kwargs["cwd"]) / "protocol/src/tool_name.rs").read_bytes() == original
        return args, kwargs

    def compiler(command, **kwargs):
        nonlocal now, fail_next_record
        index = len(observed)
        phase, arm, *details = plan[index]
        source_phase = details[-1] if confirm else "original"
        path = Path(kwargs["cwd"]) / "protocol/src/tool_name.rs"
        assert path.read_bytes() == (edited if source_phase == "edited" else original)
        assert kwargs["check"] is True
        observed.append((phase, arm, source_phase))
        duration = 100 if arm == "A" else 97
        if ((outcome == "early" and arm == "B" and source_phase == "edited")
                or (outcome == "late" and details[0] == 2 and source_phase == "original")) and index % 5 == 3:
            duration = 102  # Mean fails while the block minimum still passes.
        now += duration if phase == "measured" else 5000
        message = {"artifact": str(Path(kwargs["cwd"]) / "out.rmeta")}
        if outcome == "diagnostics" and index == 10:
            message["unexpected"] = True
        kwargs["stderr"].write(json.dumps(message).encode())
        if index == 6 and outcome == "source_mutation":
            path.write_text("compiler-side mutation must reject and restore\n")
        if index == 6 and outcome == "hash_failure":
            fail_next_record = True
        if index == 6 and outcome in ("compiler", "timeout"):
            if outcome == "timeout":
                raise TimeoutError("mock process timeout")
            raise subprocess.CalledProcessError(7, command)
        return subprocess.CompletedProcess(command, 0)

    monkeypatch.setattr(walltime_helper, "prepare_invocation", setup)
    monkeypatch.setattr(comparison.subprocess, "run", compiler)
    monkeypatch.setattr(comparison.time, "perf_counter_ns", lambda: now)
    monkeypatch.setattr(comparison, "record", record_source)
    if outcome in ("compiler", "timeout", "diagnostics", "source_mutation", "hash_failure"):
        with pytest.raises((subprocess.CalledProcessError, TimeoutError, ValueError, OSError)):
            comparison.run_calls(fixtures, invocations, [0, 1, 2, 3], result, plan, edit)
        assert result["status"] == "failed"
    else:
        comparison.run_calls(fixtures, invocations, [0, 1, 2, 3], result, plan, edit if confirm else None)
    assert len(observed) == len(result["calls"]) == count
    assert all(row["cpu_affinity"] == [0, 1, 2, 3] for row in result["calls"])
    assert all(row["elapsed_ns"] in (97, 100, 102) for row in result["calls"] if row["phase"] == "measured")
    assert all((tmp_path / arm / "protocol/src/tool_name.rs").read_bytes() == original for arm in "AB")
    if confirm:
        assert result["actual_calls"] == count and not result["confirmed"]
        if outcome in ("pass", "early", "late"):
            assert result["actual_measured"] == (12 if outcome == "early" else 24)
            assert result["strict_gate_passed"] == (outcome == "pass")
            assert all(gate["passed"] for gate in result["gates"] if gate["statistic"] == "min")
            assert result["status"] == ("early-rejected" if outcome == "early" else "running")


def test_release_confirmation_is_strict_and_codex_only():
    import compare_release_artifacts as comparison

    blocks = [{"arm": arm, "phases": {phase: {"min_ns": value, "sum_ns": 3 * value}
                                       for phase in ("original", "edited")}}
              for arm, value in (("A", 100), ("B", 98))]
    assert not any(gate["passed"] for gate in comparison.confirmation_gates(blocks))
    with pytest.raises(ValueError, match="only the frozen Codex"):
        comparison.main("uv-release-resolver", 3, confirm=True)
