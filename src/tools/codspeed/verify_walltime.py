"""Check that every compiler walltime benchmark completed its measured rounds."""

import argparse
import json
import math
from pathlib import Path
from benchmarks import SHARDS, selected_fixtures


def expected_cases(shard: str | None = None) -> set[str]:
    return {
        f"src/tools/codspeed/test_compiler_walltime.py::test_compile[{name}-{scenario}]"
        for name, scenarios in selected_fixtures(shard).items()
        for scenario in scenarios
    }


EXPECTED_CASES = expected_cases()


def verify_walltime(folder: Path, *, shard: str | None = None) -> int:
    expected = expected_cases(shard)
    seen = set()
    # Profiling metadata outside results/ is not a benchmark result.
    for path in sorted((folder / "results").glob("*.json")):
        for benchmark in json.loads(path.read_text())["benchmarks"]:
            uri = benchmark["uri"]
            if uri not in expected or uri in seen:
                raise ValueError(f"Unexpected or duplicate benchmark: {uri}")
            stats = benchmark["stats"]
            if (stats["rounds"], stats["iter_per_round"], stats["warmup_iters"]) != (
                10,
                1,
                1,
            ):
                raise ValueError(
                    f"{uri}: expected 10 rounds, one iteration and one warmup"
                )
            for field in ("min_ns", "mean_ns"):
                value = stats[field]
                if (
                    type(value) not in (int, float)
                    or not math.isfinite(value)
                    or value <= 0
                ):
                    raise ValueError(f"{uri}: invalid {field}: {value!r}")
            seen.add(uri)
    if missing := expected - seen:
        raise ValueError(f"Missing walltime benchmarks: {', '.join(sorted(missing))}")
    return len(seen)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile_dir", type=Path)
    parser.add_argument("--shard", choices=SHARDS)
    args = parser.parse_args()
    try:
        count = verify_walltime(args.profile_dir, shard=args.shard)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Walltime result verification failed: {error}\n")
    print(f"Verified {count} walltime benchmarks.")


if __name__ == "__main__":
    main()
