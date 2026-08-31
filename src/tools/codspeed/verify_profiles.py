"""Check compiler coverage in CodSpeed 5.2.1's retained Callgrind profiles."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
import os
from pathlib import Path
from runpy import run_path
import shlex


# pytest-codspeed 5.0.3 makes URIs relative to the Git root, independently of
# pytest's rootdir. These IDs match test_compile's explicit parameter IDs.
BENCHMARKS = run_path(str(Path(__file__).with_name("test_compiler.py")))
EXPECTED_CASES = {
    f"{name}-{scenario}": (
        f"src/tools/codspeed/test_compiler.py::test_compile[{name}-{scenario}]"
    )
    for name in BENCHMARKS["FIXTURE_NAMES"]
    for scenario in BENCHMARKS["SCENARIOS"]
}


@dataclass
class Profile:
    pid: int = 0
    command: str = ""
    instructions: int = 0
    symbols: set[str] = field(default_factory=set)
    children: dict[str, set[int]] = field(default_factory=dict)


def read_profile(path: Path) -> Profile:
    """Stream combined dumps, retaining only counters, spawn edges, and examples."""
    profile = Profile()
    uri = None
    children: set[int] = set()
    events: list[str] = []

    def finish_part():
        if uri is not None:
            profile.children.setdefault(uri, set()).update(children)

    with path.open(errors="replace") as stream:
        for line in stream:
            key, _, value = line.strip().partition(": ")
            if key == "part":
                finish_part()
                uri, children, events = None, set(), []
            elif key == "pid":
                profile.pid = int(value)
            elif key == "cmd":
                profile.command = value
            elif key == "desc":
                if value.startswith("Trigger: Client Request: "):
                    uri = value.removeprefix("Trigger: Client Request: ")
                elif value.startswith("Spawned pid: "):
                    children.add(int(value.removeprefix("Spawned pid: ")))
            elif key == "events":
                events = value.split()
            elif key == "summary":
                if "Ir" not in events:
                    raise ValueError(f"{path}: summary without an Ir event")
                counts = [int(count) for count in value.split()]
                # Callgrind can omit trailing zero counters.
                index = events.index("Ir")
                profile.instructions += counts[index] if index < len(counts) else 0
            elif line.startswith(("fn=", "cfn=")) and len(profile.symbols) < 3:
                symbol = line.partition("=")[2].strip()
                if "rustc_" in symbol:
                    profile.symbols.add(symbol)
    finish_part()
    if not profile.pid or not profile.command:
        raise ValueError(f"{path}: missing process identity")
    return profile


def verify_profiles(folder: Path, rustc: Path) -> list[dict]:
    """Require an attributed, counted compiler process for every expected case."""
    profiles = {}
    children: dict[str, set[int]] = {}
    for path in sorted(folder.glob("*.out")):
        profile = read_profile(path)
        if profile.pid in profiles:
            raise ValueError(f"Duplicate profile for PID {profile.pid}")
        profiles[profile.pid] = profile
        for uri, pids in profile.children.items():
            children.setdefault(uri, set()).update(pids)

    compiler = rustc.resolve(strict=True)
    compiler_profiles = {
        pid: profile
        for pid, profile in profiles.items()
        if Path(shlex.split(profile.command)[0]).resolve() == compiler
        and profile.instructions > 0
    }
    evidence = []
    for case, uri in EXPECTED_CASES.items():
        pids = sorted(children.get(uri, set()) & compiler_profiles.keys())
        if not pids:
            raise ValueError(f"{case}: no attributed compiler child with positive Ir")
        symbols = set()
        for pid in pids:
            if not compiler_profiles[pid].symbols:
                raise ValueError(f"{case}: compiler PID {pid} has no rustc_ symbols")
            for symbol in sorted(compiler_profiles[pid].symbols):
                if len(symbols) < 3:
                    symbols.add(symbol)
        evidence.append(
            {
                "case": case,
                "uri": uri,
                "child_pids": pids,
                "instructions": sum(
                    compiler_profiles[pid].instructions for pid in pids
                ),
                "compiler_symbols": sorted(symbols)[:3],
            }
        )
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile_dir", type=Path)
    parser.add_argument("--rustc", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        evidence = verify_profiles(args.profile_dir, args.rustc)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Compiler profile verification failed: {error}\n")
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    table = "| Compiler benchmark | Instructions |\n| --- | ---: |\n" + "".join(
        f"| {row['case']} | {row['instructions']:,} |\n" for row in evidence
    )
    print(table, end="")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as stream:
            stream.write("\nCompiler child-process coverage\n\n" + table)


if __name__ == "__main__":
    main()
