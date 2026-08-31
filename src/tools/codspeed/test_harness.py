"""Regression tests for the source/cache boundaries of compiler measurements."""

import os
from pathlib import Path
import subprocess
import sys

import pytest

from test_compiler import compiler_environment, prepare_invocation


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
