"""Summarize paired first-edit clocks and keep each immediate repeat separate."""
import argparse
import json
import math
from pathlib import Path
from statistics import median


def median_interval(values):
    n = len(values)
    cuts = [i for i in range(n // 2) if 1 - 2 * sum(math.comb(n,j) for j in range(i + 1)) / 2**n >= 0.95]
    if not cuts:
        return None
    ix = max(cuts)
    vals = sorted(values)
    return [vals[ix], vals[-1-ix],
            1 - 2 * sum(math.comb(n,j) for j in range(ix + 1)) / 2**n]


def sign_p(values, threshold):
    vals = [v for v in values if v != threshold]
    high = sum(v > threshold for v in vals)
    return sum(math.comb(len(vals),i) for i in range(high,len(vals)+1)) / 2**len(vals)


def score(paths):
    samples = []
    plans = []
    for path in paths:
        plan = json.loads((path / "plan.json").read_bytes())
        pts = [json.loads(line) for line in (path / "pairs.jsonl").read_text().splitlines()]
        assert len(pts) == len(plan["sequence"])
        for row, seq in zip(pts,plan["sequence"],strict=True):
            assert list(row) == seq
        if plans:
            assert all(plan[k] == plans[0][k] for k in ("source","inputs","runner","shared_runner"))
        plans.append(plan)
        samples.extend(pts)
    roles = list(plans[0]["inputs"])
    summary = {"pairs":len(samples),"cohorts":[p.name for p in paths],"roles":{},"comparisons":{}}
    for role in roles:
        summary["roles"][role] = {k:median(row[role][k] for row in samples)
                                  for k in ("edit","noop","parent_noop")}
    for role in roles:
        if role == "candidate":
            continue
        ratios = [row[role]["edit"] / row["candidate"]["edit"] for row in samples]
        summary["comparisons"][role] = {"pair_ratios":ratios,"median":median(ratios),
            "median_interval":median_interval(ratios),"above_one":sum(r>1 for r in ratios),
            "above_two":sum(r>2 for r in ratios),"sign_p_above_one":sign_p(ratios,1),
            "sign_p_above_two":sign_p(ratios,2)}
    return summary


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("cohorts", nargs="+", type=Path)
    opts = p.parse_args()
    print(json.dumps(score(opts.cohorts),indent=2))
