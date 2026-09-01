"""Check completeness of pytest-codspeed 5.0.3 compiler walltime results."""

from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
from runpy import run_path


BENCHMARKS = run_path(str(Path(__file__).with_name("test_compiler.py")))
EXPECTED_CASES = {
    f"src/tools/codspeed/test_compiler_walltime.py::test_compile[{name}-{scenario}]": (
        f"{name}-{scenario}"
    )
    for name in BENCHMARKS["FIXTURE_NAMES"]
    for scenario in BENCHMARKS["SCENARIOS"]
}
PROTOCOL = {"rounds": 10, "iter_per_round": 1, "warmup_iters": 1}
TIMING_FIELDS = (
    "min_ns",
    "max_ns",
    "mean_ns",
    "q1_ns",
    "median_ns",
    "q3_ns",
    "stdev_ns",
)


def verify_walltime(folder: Path) -> dict:
    """Require every native benchmark once, with the intended sampling protocol."""
    benchmarks = {}
    # The plugin writes results/<pytest-pid>.json. The runner's profiling
    # metadata is elsewhere in the profile root and is not a benchmark result.
    for path in sorted((folder / "results").glob("*.json")):
        data = json.loads(path.read_text())
        if not isinstance(data, dict):
            raise ValueError(f"{path}: expected a pytest result object")
        instrument = data.get("instrument")
        if not isinstance(instrument, dict) or instrument.get("type") != "walltime":
            raise ValueError(f"{path}: expected the walltime instrument")
        creator = data.get("creator")
        if (
            not isinstance(creator, dict)
            or creator.get("name") != "pytest-codspeed"
            or creator.get("version") != "5.0.3"
            or type(creator.get("pid")) is not int
            or creator["pid"] <= 0
            or path.name != f"{creator['pid']}.json"
        ):
            raise ValueError(f"{path}: unexpected pytest creator or PID")
        records = data.get("benchmarks")
        if not isinstance(records, list):
            raise ValueError(f"{path}: missing benchmark records")
        for benchmark in records:
            if not isinstance(benchmark, dict):
                raise ValueError(f"{path}: malformed benchmark record")
            uri = benchmark.get("uri")
            if not isinstance(uri, str) or uri not in EXPECTED_CASES:
                raise ValueError(f"{path}: unexpected benchmark URI {uri!r}")
            if uri in benchmarks:
                raise ValueError(f"{path}: duplicate benchmark {uri}")
            case = EXPECTED_CASES[uri]
            if benchmark.get("name") != f"test_compile[{case}]":
                raise ValueError(f"{path}: benchmark name disagrees with {uri}")
            stats = benchmark.get("stats")
            if not isinstance(stats, dict) or not isinstance(
                benchmark.get("config"), dict
            ):
                raise ValueError(f"{case}: missing statistics or configuration")
            for field, expected in PROTOCOL.items():
                value = stats.get(field)
                if type(value) is not int or value != expected:
                    raise ValueError(
                        f"{case}: expected {field}={expected}, got {value!r}"
                    )
            for field in (*TIMING_FIELDS, "total_time"):
                value = stats.get(field)
                if (
                    type(value) not in (int, float)
                    or not math.isfinite(value)
                    or value < 0
                    or (value == 0 and field != "stdev_ns")
                ):
                    raise ValueError(
                        f"{case}: invalid timing statistic {field}={value!r}"
                    )
            benchmarks[uri] = {
                "case": case,
                "uri": uri,
                "result_file": str(path),
                "pytest_pid": creator["pid"],
                "config": benchmark["config"],
                "stats": stats,
            }
    missing = EXPECTED_CASES.keys() - benchmarks.keys()
    if missing:
        raise ValueError(f"Missing walltime benchmarks: {', '.join(sorted(missing))}")
    return {
        "instrument": "walltime",
        "stats_units": {
            **dict.fromkeys(TIMING_FIELDS, "nanoseconds"),
            "total_time": "seconds, including setup and teardown",
        },
        "benchmarks": [benchmarks[uri] for uri in EXPECTED_CASES],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile_dir", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        evidence = verify_walltime(args.profile_dir)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Walltime result verification failed: {error}\n")
    args.output.write_text(json.dumps(evidence, indent=2, allow_nan=False) + "\n")
    report = (
        "Compiler walltime: 10 measured rounds, one invocation per round, one warmup. "
        "Times below exclude setup and teardown. The retained total_time statistic "
        "includes that work and is not a compiler duration.\n\n"
        "| Compiler benchmark | Mean (ms) | Median (ms) | Stddev (ms) | Min (ms) | Max (ms) |\n"
        "| --- | ---: | ---: | ---: | ---: | ---: |\n"
    )
    for row in evidence["benchmarks"]:
        times = " | ".join(
            f"{row['stats'][field] / 1_000_000:.3f}"
            for field in ("mean_ns", "median_ns", "stdev_ns", "min_ns", "max_ns")
        )
        report += f"| {row['case']} | {times} |\n"
    print(report, end="")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as stream:
            stream.write("\n" + report)


if __name__ == "__main__":
    main()
