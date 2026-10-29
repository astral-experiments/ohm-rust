"""Recompute the two separately registered studies from the exported paired observations."""
from __future__ import annotations
import hashlib
import json
import math
import random
import statistics
from pathlib import Path

here = Path(__file__).resolve().parent
read = lambda name: json.loads((here / name).read_bytes())

def quantile(samples, p):
    xs = sorted(samples)
    index = (len(xs) - 1) * p
    k = int(index)
    return xs[k] + (xs[min(k + 1, len(xs) - 1)] - xs[k]) * (index - k)

def sign_p(values):
    nonzero = [v for v in values if v != 0]
    count = sum(v > 0 for v in nonzero)
    tail = min(count, len(nonzero) - count)
    return min(1, 2 * sum(math.comb(len(nonzero), j) for j in range(tail + 1)) / (2 ** len(nonzero)))

def span(values):
    return [quantile(values, .025), quantile(values, .975)]

data = read("observations.json")
outcomes = read("outcomes.json")
checks = read("controls.json")
assert checks["rmeta"]["H"] != checks["rmeta"]["P"]
assert checks["caller_statuses"] == [0, 101, 101, 0]
assert checks["caller_failure_codes"] == ["E0432", "E0407"]
assert checks["target_values"] == 151 and checks["buckets"] == 9
assert checks["same_119_compiler_argv"] and checks["same_53_std"]
assert checks["interner_and_order_cases"] == checks["interner_and_order_matched"] == 170
assert checks["external_callback_exit"] == 0

rng = random.Random(2026102915382)
med = statistics.median
for direction in "HP":
    records = data["instructions"][direction]
    assert [r["n"] for r in records] == list(range(1, 13))
    assert sorted(r["order"] for r in records) == ["AD"] * 6 + ["DA"] * 6
    values = [1 - r["D"][0] / r["A"][0] for r in records]
    ci = span([med(rng.choices(values, k=12)) for _ in range(50_000)])
    got = outcomes["instructions"][direction]
    assert ci == got["ci95"] and med(values) == got["paired_median"] and sign_p(values) == got["sign_p"]
    assert sum(v > 0 for v in values) == got["favorable"]
    a, d = [med(r[arm][1] for r in records) for arm in "AD"]
    assert (a, d) == (got["fill_A"], got["fill_D"])
    assert got["passes"] == (ci[0] > 0 and got["favorable"] >= 9 and d < a)
if "wall" in data:
    pvalues = {}
    for counter, direction in enumerate("HP"):
        records = data["wall"][direction]
        assert [r["n"] for r in records] == list(range(1, 33))
        assert sorted(r["order"] for r in records) == ["AD"] * 16 + ["DA"] * 16
        for endpoint, phase in enumerate(["edit", "repeat"]):
            rng = random.Random(2026102915572 + counter * 2 + endpoint)
            grouped = [[1 - r["D"][phase] / r["A"][phase] for r in records if r["order"] == o] for o in ["AD", "DA"]]
            values = grouped[0] + grouped[1]
            ci = span([med(rng.choices(grouped[0], k=16) + rng.choices(grouped[1], k=16)) for _ in range(50_000)])
            got = outcomes["wall"][direction][phase]
            assert got["ci95"] == ci and got["paired_median"] == med(values)
            assert sum(v > 0 for v in values) == got["favorable"]
            if phase == "edit":
                pvalues[direction] = sign_p(values)
                assert pvalues[direction] == got["exact_p"]
    largest = 0
    for rank, d in enumerate(sorted(pvalues, key=pvalues.get)):
        largest = max(largest, min(1, (2-rank) * pvalues[d]))
        assert largest == outcomes["wall"][d]["edit"]["adjusted_p"]
    positives = []
    practical = []
    for d in "HP":
        e = outcomes["wall"][d]["edit"]
        positives.append(e["paired_median"] > 0 and e["ci95"][0] > 0 and e["adjusted_p"] < .05 and e["favorable"] >= 20)
        practical.append(e["paired_median"] >= .01 and outcomes["wall"][d]["repeat"]["ci95"][0] > -.02)
    assert outcomes["wall_statistical"] == all(positives)
    assert outcomes["wall_practical"] == (all(positives) and all(practical))
print(json.dumps({"instructions": {d: outcomes["instructions"][d]["passes"] for d in "HP"}, "whole_cargo": outcomes.get("wall_statistical"), "practical": outcomes.get("wall_practical")}, sort_keys=True))
