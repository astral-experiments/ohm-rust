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
SYSCALL_EVENTS = ("SysCount", "SysTime", "SysCpuTime")


@dataclass
class Profile:
    pid: int = 0
    command: str = ""
    instructions: int = 0
    symbols: set[str] = field(default_factory=set)
    children: dict[str, set[int]] = field(default_factory=dict)
    syscall_parts: list[dict[str, int]] = field(default_factory=list)
    benchmark_syscalls: dict[str, list[dict[str, int]]] = field(default_factory=dict)


def syscall_totals(parts: list[dict[str, int]]) -> dict[str, int | None]:
    """Keep an absent event distinct from a recorded zero across all parts."""
    return {
        event: sum(part[event] for part in parts)
        if parts and all(event in part for part in parts)
        else None
        for event in SYSCALL_EVENTS
    }


def read_profile(path: Path) -> Profile:
    """Stream combined dumps, retaining only counters, spawn edges, and examples."""
    profile = Profile()
    uri = None
    children: set[int] = set()
    events: list[str] = []
    syscalls: dict[str, int] = {}

    def finish_part():
        if uri is not None:
            profile.children.setdefault(uri, set()).update(children)
            profile.benchmark_syscalls.setdefault(uri, []).append(syscalls)

    with path.open(errors="replace") as stream:
        for line in stream:
            key, _, value = line.strip().partition(": ")
            if key == "part":
                finish_part()
                uri, children, events, syscalls = None, set(), [], {}
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
                syscalls = {
                    event: counts[index] if index < len(counts) else 0
                    for index, event in enumerate(events)
                    if event in SYSCALL_EVENTS
                }
                profile.syscall_parts.append(syscalls)
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
    parents: dict[str, set[int]] = {}
    parent_syscalls: dict[str, list[dict[str, int]]] = {}
    for path in sorted(folder.glob("*.out")):
        profile = read_profile(path)
        if profile.pid in profiles:
            raise ValueError(f"Duplicate profile for PID {profile.pid}")
        profiles[profile.pid] = profile
        for uri, pids in profile.children.items():
            children.setdefault(uri, set()).update(pids)
            parents.setdefault(uri, set()).add(profile.pid)
            parent_syscalls.setdefault(uri, []).extend(profile.benchmark_syscalls[uri])

    compiler = rustc.resolve(strict=True)
    compiler_profiles = {
        pid: profile
        for pid, profile in profiles.items()
        if Path(shlex.split(profile.command)[0]).resolve() == compiler
        and profile.instructions > 0
    }
    evidence = []
    claimed_pids: dict[int, str] = {}
    for case, uri in EXPECTED_CASES.items():
        pids = sorted(children.get(uri, set()) & compiler_profiles.keys())
        if not pids:
            raise ValueError(f"{case}: no attributed compiler child with positive Ir")
        symbols = set()
        for pid in pids:
            if pid in claimed_pids:
                raise ValueError(
                    f"Compiler PID {pid} reused across cases: {claimed_pids[pid]}, {case}"
                )
            claimed_pids[pid] = case
            if not compiler_profiles[pid].symbols:
                raise ValueError(f"{case}: compiler PID {pid} has no rustc_ symbols")
            for symbol in sorted(compiler_profiles[pid].symbols):
                if len(symbols) < 3:
                    symbols.add(symbol)
        evidence.append(
            {
                "case": case,
                "uri": uri,
                "parent_pids": sorted(parents[uri]),
                "child_pids": pids,
                "instructions": sum(
                    compiler_profiles[pid].instructions for pid in pids
                ),
                "parent_syscalls": syscall_totals(parent_syscalls[uri]),
                "compiler_syscalls": syscall_totals(
                    [
                        part
                        for pid in pids
                        for part in compiler_profiles[pid].syscall_parts
                    ]
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
    table = (
        "| Compiler benchmark | Instructions | Parent SysCount | Parent SysTime (ns) "
        "| Parent SysCpuTime (ns) | Compiler SysCount | Compiler SysTime (ns) "
        "| Compiler SysCpuTime (ns) |\n"
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n"
    )
    for row in evidence:
        counters = [
            "not recorded" if row[owner][event] is None else f"{row[owner][event]:,}"
            for owner in ("parent_syscalls", "compiler_syscalls")
            for event in SYSCALL_EVENTS
        ]
        table += (
            f"| {row['case']} | {row['instructions']:,} | "
            + " | ".join(counters)
            + " |\n"
        )
    report = (
        "Parent counters cover only the named benchmark's dump parts; compiler counters "
        "cover its attributed compiler processes. SysCount is the syscall count. "
        "SysTime (elapsed) and SysCpuTime (thread CPU) are simulator-observed durations, "
        "not native kernel timings. An event absent from any contributing part is "
        "shown as not recorded.\n\n" + table
    )
    print(report, end="")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as stream:
            stream.write("\nCompiler child-process coverage\n\n" + report)


if __name__ == "__main__":
    main()
