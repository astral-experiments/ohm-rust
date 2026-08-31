"""Reject profiles that pass pytest without measuring the candidate compiler."""

import json
from pathlib import Path
import subprocess
import sys

import pytest

from verify_profiles import EXPECTED_CASES, read_profile, verify_profiles


def part(number, *, uri=None, children=(), ir=0, events="Ir Dr", counts=None):
    trigger = f"Client Request: {uri}" if uri else "Program termination"
    return (
        f"\npart: {number}\nthread: {number + 100}\n"
        + "".join(f"desc: Spawned pid: {pid}\n" for pid in children)
        + f"desc: Trigger: {trigger}\nevents: {events}\n"
        + f"summary: {ir if counts is None else counts}\n"
    )


def write_profile(folder, pid, command, parts, symbol="rustc_interface::run_compiler"):
    path = folder / f"{pid}.out"
    path.write_text(
        f"# callgrind format\nversion: 1\npid: {pid}\ncmd: {command}\n"
        + "".join(parts)
        + (f"fn={symbol}\n1 10\n" if symbol else "")
    )
    return path


@pytest.fixture
def profiles(tmp_path):
    compiler = tmp_path / "stage1/bin/rustc"
    compiler.parent.mkdir(parents=True)
    compiler.touch()
    folder = tmp_path / "profiles"
    folder.mkdir()
    parent_parts = []
    for pid, uri in enumerate(EXPECTED_CASES.values(), start=10):
        parent_parts.append(part(pid, uri=uri, children=[pid], ir=30))
        write_profile(
            folder, pid, str(compiler) + " --crate-type lib", [part(1, ir=100)]
        )
    write_profile(folder, 1, "/usr/bin/python -m pytest", parent_parts, symbol=None)
    return folder, compiler


def test_counts_threads_once_and_uses_event_names(profiles):
    folder, compiler = profiles
    uri = next(iter(EXPECTED_CASES.values()))
    parent = folder / "1.out"
    # Per-thread dumps repeat the process's spawn edges. They must not double
    # count the child's profile, which itself contains several thread parts.
    with parent.open("a") as stream:
        stream.write(part(50, uri=uri, children=[10], ir=40))
    child = write_profile(
        folder,
        10,
        str(compiler),
        [
            part(1, events="Dr Ir Dw", counts="23 100"),
            part(1, events="Ir Dr", counts="20 300"),
            part(2, events="Dr Ir", counts="12"),
        ],
    )
    with child.open("a") as stream:
        stream.write("cfn=rustc_middle::query\n")
    alias = compiler.parent / "rustc-alias"
    alias.symlink_to(compiler)

    evidence = verify_profiles(folder, alias)

    assert len(evidence) == 8
    assert evidence[0]["child_pids"] == [10]
    assert evidence[0]["instructions"] == 120
    assert evidence[0]["uri"] == uri
    assert evidence[0]["compiler_symbols"] == [
        "rustc_interface::run_compiler",
        "rustc_middle::query",
    ]


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("zero_cost", "positive Ir"),
        ("wrong_compiler", "positive Ir"),
        ("missing_child", "positive Ir"),
        ("missing_case", "positive Ir"),
        ("missing_symbols", "no rustc_ symbols"),
        ("unattributed_warmup", "positive Ir"),
    ],
)
def test_rejects_missing_compiler_measurements(profiles, fault, message):
    folder, compiler = profiles
    uri = next(iter(EXPECTED_CASES.values()))
    child = folder / "10.out"
    if fault == "zero_cost":
        child.write_text(child.read_text().replace("summary: 100", "summary: 0"))
    elif fault == "wrong_compiler":
        child.write_text(child.read_text().replace(str(compiler), "/stage0/bin/rustc"))
    elif fault == "missing_child":
        child.unlink()
    elif fault == "missing_case":
        parent = folder / "1.out"
        parent.write_text(parent.read_text().replace(uri, "unrelated::test_compile"))
    elif fault == "missing_symbols":
        child.write_text(child.read_text().replace("rustc_interface", "unknown"))
    elif fault == "unattributed_warmup":
        parent = folder / "1.out"
        parent.write_text(
            parent.read_text().replace("Spawned pid: 10\n", "Spawned pid: 99\n")
        )
        write_profile(folder, 99, str(compiler), [part(1, ir=0)])
        # Positive compiler work outside the named benchmark must not satisfy it.
        with parent.open("a") as stream:
            stream.write(part(60, children=[10], ir=10))
    with pytest.raises(ValueError, match=message):
        verify_profiles(folder, compiler)


def test_ignores_zero_cost_setup_children(profiles):
    folder, compiler = profiles
    uri = next(iter(EXPECTED_CASES.values()))
    write_profile(folder, 99, str(compiler), [part(1, ir=0)], symbol=None)
    with (folder / "1.out").open("a") as stream:
        stream.write(part(60, uri=uri, children=[99], ir=0))
    assert verify_profiles(folder, compiler)[0]["child_pids"] == [10]


def test_bounds_symbol_examples(profiles):
    folder, _ = profiles
    with (folder / "10.out").open("a") as stream:
        for number in range(100):
            stream.write(f"fn=rustc_middle::query{number}\n")
    assert len(read_profile(folder / "10.out").symbols) == 3


def test_cli_writes_evidence_and_step_summary(profiles, monkeypatch):
    folder, compiler = profiles
    output = folder / "coverage.json"
    summary = folder / "summary.md"
    summary.write_text("Existing summary\n")
    monkeypatch.setenv("GITHUB_STEP_SUMMARY", str(summary))
    result = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).with_name("verify_profiles.py")),
            str(folder),
            "--rustc",
            str(compiler),
            "--output",
            str(output),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    assert json.loads(output.read_text()) == verify_profiles(folder, compiler)
    assert result.stdout in summary.read_text()
    assert summary.read_text().startswith("Existing summary\n")
