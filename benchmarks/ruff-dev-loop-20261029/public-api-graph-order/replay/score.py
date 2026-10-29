"""Recompute each registered endpoint from all counterbalanced adjacent pairs."""

import argparse
import itertools
import json
import math
from pathlib import Path
import random
import statistics


def summarize(rows, phase, count):
    selected = [row for row in rows if row["phase"] == phase]
    expected_order = ["AB", "BA"] * (count // 2)
    random.Random(7222026 + count).shuffle(expected_order)
    assert [row["order"] for row in selected] == expected_order
    assert [row["i"] for row in selected] == list(range(1, count + 1))

    pairs = [(row["values"]["A"], row["values"]["B"]) for row in selected]
    deltas = [math.log(b / a) for a, b in pairs]
    sample = random.Random(7222026)
    means = sorted(
        statistics.mean(sample.choices(deltas, k=count)) for _ in range(10_000)
    )
    n_wins = sum(delta < 0 for delta in deltas)
    n_losses = sum(delta > 0 for delta in deltas)
    non_ties = n_wins + n_losses
    smaller = min(n_wins, n_losses)
    sign_p = min(1, 2 * sum(math.comb(non_ties, i) for i in range(smaller + 1)) / 2**non_ties)

    chronological = [d if order == "AB" else -d for d, order in zip(deltas, expected_order)]
    cutoff = abs(sum(deltas)) - 1e-13
    observed_sum = sum(chronological)
    extreme = sum(
        abs(2 * sum(forward) - observed_sum) >= cutoff
        for forward in itertools.combinations(chronological, count // 2)
    )
    return {
        "pairs": count,
        "parent_median": statistics.median(a for a, _ in pairs),
        "candidate_median": statistics.median(b for _, b in pairs),
        "percent_saved": 100 * (1 - math.exp(statistics.mean(deltas))),
        "paired_95_percent_interval": [100 * (1 - math.exp(means[i])) for i in (9750, 249)],
        "candidate_wins": n_wins,
        "two_sided_sign_p": sign_p,
        "two_sided_balanced_randomization_p": extreme / math.comb(count, count // 2),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("samples", type=Path)
    args = parser.parse_args()
    data = json.loads(args.samples.read_text())
    if isinstance(data, dict):
        assert data["status"] == "complete"
        data = data["rows"]
    print(json.dumps({phase: summarize(data, phase, n) for phase, n in [("wall", 20), ("profile", 12)]}, indent=2))
