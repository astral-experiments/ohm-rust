"""Regression tests for the source/cache boundaries of compiler measurements."""

import os
from pathlib import Path
import subprocess
import sys

import pytest

from test_compiler import (
    compiler_environment,
    prepare_invocation,
    test_compile as run_benchmark,
)


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


@pytest.mark.parametrize("scenario", ["Full", "IncrFull"])
def test_cold_setup_discards_previous_measurement(fixture, scenario):
    prepare_invocation(fixture, scenario)
    incremental = Path(fixture["incremental"])
    incremental.mkdir(parents=True, exist_ok=True)
    (incremental / "warmup-result").touch()
    source = Path(fixture["cwd"]) / "src/lib.rs"
    source.write_text("changed by an earlier case\n")

    (command,), kwargs = prepare_invocation(fixture, scenario)

    assert source.read_text() == "fn value() -> u8 { 1 }\n"
    assert not (incremental / "warmup-result").exists()
    assert any(arg.startswith("incremental=") for arg in command) == (
        scenario == "IncrFull"
    )
    assert kwargs["check"] is True


def test_patched_setup_reseeds_before_every_invocation(fixture, monkeypatch):
    run = subprocess.run
    seeded_sources = []
    incremental = Path(fixture["incremental"])

    def record_seed(command, **kwargs):
        if command[0] == sys.executable:
            seeded_sources.append((Path(kwargs["cwd"]) / "src/lib.rs").read_text())
            (incremental / "seed").touch()
            return subprocess.CompletedProcess(command, 0)
        return run(command, **kwargs)

    monkeypatch.setattr(subprocess, "run", record_seed)
    for _ in range(2):
        prepare_invocation(fixture, "IncrPatched")
        assert (
            Path(fixture["cwd"]) / "src/lib.rs"
        ).read_text() == "fn value() -> u8 { 2 }\n"
        assert sorted(path.name for path in incremental.iterdir()) == ["seed"]
        (incremental / "warmup-result").touch()

    assert seeded_sources == ["fn value() -> u8 { 1 }\n"] * 2


def test_replay_keeps_live_instrumentation_without_stale_jobserver(monkeypatch):
    monkeypatch.setenv("LD_PRELOAD", "/live/codspeed.so")
    monkeypatch.setenv("LD_LIBRARY_PATH", "/live/lib")
    monkeypatch.setenv("CODSPEED_TEST_HOOK", "live")
    monkeypatch.setenv("CARGO_MAKEFLAGS", "--jobserver-auth=8,9")
    environment = compiler_environment(
        {
            "LD_PRELOAD": "/old/codspeed.so",
            "LD_LIBRARY_PATH": "/compiler/lib",
            "CODSPEED_TEST_HOOK": "old",
            "CARGO_PKG_NAME": "example",
        }
    )

    assert environment["LD_PRELOAD"] == "/live/codspeed.so"
    assert environment["LD_LIBRARY_PATH"].split(os.pathsep) == [
        "/compiler/lib",
        "/live/lib",
    ]
    assert environment["CODSPEED_TEST_HOOK"] == "live"
    assert environment["CARGO_PKG_NAME"] == "example"
    assert "CARGO_MAKEFLAGS" not in environment


def test_failed_seed_aborts_the_benchmark(fixture):
    fixture["command"] = [sys.executable, "-c", "raise SystemExit(7)"]
    with pytest.raises(subprocess.CalledProcessError) as error:
        prepare_invocation(fixture, "IncrUnchanged")
    assert error.value.returncode == 7


def test_compiler_wait_is_outside_parent_measurement(fixture, monkeypatch):
    measured = False
    processes = []
    popen = subprocess.Popen

    class CompilerProcess(popen):
        def __init__(self, *args, **kwargs):
            assert measured
            super().__init__(*args, **kwargs)
            processes.append(self)

        def wait(self, *args, **kwargs):
            assert not measured
            return super().wait(*args, **kwargs)

    class Benchmark:
        def pedantic(self, target, *, setup, teardown, **_options):
            nonlocal measured
            # Exercise the same lifecycle for warmup and measurement.
            for _ in range(2):
                args, kwargs = setup()
                measured = True
                try:
                    target(*args, **kwargs)
                finally:
                    measured = False
                teardown(*args, **kwargs)
                assert processes[-1].returncode == 0

    monkeypatch.setattr(subprocess, "Popen", CompilerProcess)
    run_benchmark(Benchmark(), {"example": fixture}, "example", "Full")
    assert len(processes) == 2


@pytest.mark.parametrize("interrupt", [False, True])
def test_compiler_failure_or_interruption_reaps_the_child(
    fixture, monkeypatch, interrupt
):
    fixture["command"] = [
        sys.executable,
        "-c",
        "import time; time.sleep(60)" if interrupt else "raise SystemExit(7)",
    ]
    processes = []
    popen = subprocess.Popen

    def record_process(*args, **kwargs):
        process = popen(*args, **kwargs)
        processes.append(process)
        return process

    class Benchmark:
        def pedantic(self, target, *, setup, teardown, **_options):
            args, kwargs = setup()
            target(*args, **kwargs)
            if interrupt:
                raise RuntimeError("Measurement hook failed")
            teardown(*args, **kwargs)

    monkeypatch.setattr(subprocess, "Popen", record_process)
    error_type = RuntimeError if interrupt else subprocess.CalledProcessError
    with pytest.raises(error_type) as error:
        run_benchmark(Benchmark(), {"example": fixture}, "example", "Full")
    assert len(processes) == 1
    assert processes[0].returncode is not None
    if not interrupt:
        assert error.value.returncode == 7
