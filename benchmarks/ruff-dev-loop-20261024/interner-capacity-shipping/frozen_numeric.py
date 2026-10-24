# Exact numeric functions from frozen analyze.py c96d0a57e493bc4fd97eed751ee34d4d0fb7c23aa6bc18128a9a52888395ef1f.
import collections
import hashlib
import itertools
import math
import numpy as np


def seed_for(seed, label):
    digest = hashlib.sha256(f"{seed}:{label}".encode()).digest()
    return int.from_bytes(digest[:8], "little")


def randomization_tests(pairs, log_ratios, rng, samples):
    """Permute AB/BA assignment within each session, preserving its balance.

    Under the sharp null, each pair's chronological second-minus-first log-time
    contrast is fixed. AB contributes that contrast; BA contributes its negative.
    Filtering conditions on the observed assignments of the omitted pairs.
    """
    values = np.asarray(log_ratios, dtype=float)
    observed = float(values.sum())
    observed_favorable = int(np.count_nonzero(values < 0))
    tolerance = max(1e-15, float(np.abs(values).sum()) * np.finfo(float).eps * 32)
    sessions = collections.defaultdict(list)
    for pair, value in zip(pairs, values):
        if pair["order"] is None:
            unavailable = {"method": "unavailable_missing_order_plan", "p_one_sided": None}
            return unavailable.copy(), unavailable.copy()
        ab = pair["order"][0] == "base"
        sessions[pair["session"]].append((float(value) if ab else -float(value), ab))
    blocks = [(name, np.array([x[0] for x in items]), sum(x[1] for x in items))
              for name, items in sessions.items()]
    assignment_count = math.prod(math.comb(len(z), k) for _, z, k in blocks)
    design = [{"session": name, "pairs": len(z), "base_first": k,
               "candidate_first": len(z) - k, "possible_assignments": math.comb(len(z), k)}
              for name, z, k in blocks]
    if assignment_count <= 2 ** 20:
        sums, signs = np.array([0.0]), np.array([0])
        for _, chronological, k in blocks:
            contrasts, favorable = [], []
            for selected in itertools.combinations(range(len(chronological)), k):
                assignment = -chronological.copy()
                assignment[list(selected)] *= -1
                contrasts.append(float(assignment.sum()))
                favorable.append(int(np.count_nonzero(assignment < 0)))
            sums = (sums[:, None] + np.array(contrasts)[None, :]).ravel()
            signs = (signs[:, None] + np.array(favorable)[None, :]).ravel()
        extremes = [int(np.count_nonzero(sums <= observed + tolerance)),
                    int(np.count_nonzero(signs >= observed_favorable))]
        counts, correction, method = assignment_count, 0, "exact_balanced_order_randomization"
    else:
        extremes = [0, 0]
        for start in range(0, samples, 4096):
            count = min(4096, samples - start)
            sums, signs = np.zeros(count), np.zeros(count, dtype=int)
            for _, chronological, k in blocks:
                # The k smallest independent continuous ranks form a uniform
                # k-subset, exactly preserving each session's AB count.
                assignment = np.broadcast_to(-chronological, (count, len(chronological))).copy()
                if k:
                    indexes = np.argsort(rng.random(assignment.shape), axis=1)[:, :k]
                    rows = np.arange(count)[:, None]
                    assignment[rows, indexes] *= -1
                sums += assignment.sum(axis=1)
                signs += np.count_nonzero(assignment < 0, axis=1)
            extremes[0] += int(np.count_nonzero(sums <= observed + tolerance))
            extremes[1] += int(np.count_nonzero(signs >= observed_favorable))
        counts, correction, method = samples, 1, "monte_carlo_balanced_order_randomization"
    return tuple({"method": method, "statistic": statistic,
                  "p_one_sided": (extreme + correction) / (counts + correction),
                  "assignments": counts, "possible_assignments": assignment_count,
                  "extreme_assignments": extreme, "session_constraints": design,
                  "correction": "plus-one including observed assignment" if correction else "none (exact enumeration)"}
                 for statistic, extreme in zip(("sum_log_candidate_over_base", "favorable_pair_count"), extremes))


def summarize(pairs, *, seed, label, bootstrap_samples, permutation_samples, metric="wall_seconds", sensitivity=False):
    numeric = [p for p in pairs if not any(f.endswith("invalid_selected_metric") for f in p["flags"])]
    if not numeric:
        return {"pairs": len(pairs), "numeric_pairs": 0}
    baseline = np.array([p["base"][metric] for p in numeric])
    candidate = np.array([p["candidate"][metric] for p in numeric])
    ratios = np.log(candidate / baseline)
    rng = np.random.default_rng(seed_for(seed, label + ":bootstrap"))
    strata = collections.defaultdict(list)
    for index, pair in enumerate(numeric):
        strata[(pair["session"], tuple(pair["order"] or []))].append(index)
    bootstraps = np.empty(bootstrap_samples)
    for start in range(0, bootstrap_samples, 4096):
        count = min(4096, bootstrap_samples - start)
        totals = np.zeros(count)
        for indices in strata.values():
            stratum = ratios[indices]
            draws = rng.integers(0, len(stratum), size=(count, len(stratum)))
            totals += stratum[draws].sum(axis=1)
        bootstraps[start : start + count] = totals / len(ratios)
    low, high = np.quantile(bootstraps, [0.005, 0.995])
    favorable = int(np.count_nonzero(candidate < baseline))
    unfavorable = int(np.count_nonzero(candidate > baseline))
    non_ties = favorable + unfavorable
    sign_p = (sum(math.comb(non_ties, k) for k in range(favorable, non_ties + 1)) /
              2 ** non_ties) if non_ties else 1.0
    mean_ratio = float(np.exp(ratios.mean()))
    permutation, sign_permutation = randomization_tests(
        numeric, ratios, np.random.default_rng(seed_for(seed, label + ":randomization")), permutation_samples)
    result = {
        "pairs": len(pairs), "numeric_pairs": len(numeric),
        "metric": metric,
        "flagged_pairs": sum(bool(p["flags"]) for p in pairs),
        "baseline_median": float(np.median(baseline)),
        "candidate_median": float(np.median(candidate)),
        "median_paired_saving": float(np.median(baseline - candidate)),
        "mean_paired_saving": float(np.mean(baseline - candidate)),
        "mean_log_candidate_over_base": float(ratios.mean()),
        "geometric_candidate_over_base": mean_ratio,
        "geometric_reduction_percent": 100 * (1 - mean_ratio),
        "geometric_reduction_percent_ci99": [100 * (1 - math.exp(high)), 100 * (1 - math.exp(low))],
        "favorable_pairs": favorable, "unfavorable_pairs": unfavorable,
        "tied_pairs": len(numeric) - non_ties,
        "bootstrap_stratum_sizes": [len(indices) for indices in strata.values()],
        "bootstrap_caveat": "Approximate interval assuming independent observations within session/order strata; temporal correlation or singleton strata can understate uncertainty.",
        "selection_caveat": ("Filtered-pair inference conditions on omitted assignments and is exact only if selection is assignment-independent; use as sensitivity, not primary confirmation."
                             if sensitivity or len(numeric) != len(pairs) else None),
        "sign_test": sign_permutation,
        "binomial_sign_diagnostic": {"method": "binomial_ties_excluded", "p_one_sided": sign_p,
                                     "non_tied_pairs": non_ties,
                                     "caveat": "Not a design-exact p-value: session-balanced assignments make pair signs dependent."},
        "randomization_test": permutation,
    }
    if metric == "wall_seconds":
        result.update(baseline_median_seconds=result["baseline_median"],
                      candidate_median_seconds=result["candidate_median"],
                      median_paired_saving_seconds=result["median_paired_saving"],
                      mean_paired_saving_seconds=result["mean_paired_saving"])
    for arm in ("base", "candidate"):
        cpu = [p[arm].get("usage", {}) for p in numeric]
        if all("ru_utime" in usage and "ru_stime" in usage for usage in cpu):
            result[f"{arm}_median_cpu_seconds"] = float(np.median([
                usage["ru_utime"] + usage["ru_stime"] for usage in cpu]))
    return result


def holm(pvalues):
    adjusted = {}
    running = 0.0
    for rank, (name, value) in enumerate(sorted(pvalues.items(), key=lambda item: item[1])):
        running = max(running, min(1.0, value * (len(pvalues) - rank)))
        adjusted[name] = running
    return adjusted
