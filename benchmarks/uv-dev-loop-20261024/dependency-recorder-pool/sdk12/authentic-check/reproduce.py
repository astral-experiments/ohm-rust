"""Portable numerical replay; does not recreate private collection validation."""
from pathlib import Path
import argparse
import gzip
import hashlib
import json
import math
import random
import re
import statistics

def quantile(values, probability):
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    fraction = position - low
    return ordered[low] * (1 - fraction) + ordered[high] * fraction


def exact_sign_test(wins, losses):
    non_ties = wins + losses
    if not non_ties:
        return 1.0
    tail = sum(math.comb(non_ties, count) for count in range(min(wins, losses) + 1))
    return min(1.0, 2 * tail / (2 ** non_ties))


def paired_statistics(pairs, confidence=0.99, resamples=20000, seed=0):
    if not pairs:
        raise ValueError("no measurement pairs")
    logs = [math.log(pair["baseline"] / pair["candidate"]) for pair in pairs]
    generator = random.Random(seed)
    bootstraps = [statistics.fmean(generator.choices(logs, k=len(logs))) for _ in range(resamples)]
    alpha = (1 - confidence) / 2
    interval = [math.exp(quantile(bootstraps, alpha)), math.exp(quantile(bootstraps, 1 - alpha))]
    ratio = math.exp(statistics.fmean(logs))
    wins = sum(pair["baseline"] > pair["candidate"] for pair in pairs)
    losses = sum(pair["baseline"] < pair["candidate"] for pair in pairs)
    return {
        "pairs": len(pairs),
        "baseline_median_seconds": statistics.median(pair["baseline"] for pair in pairs),
        "candidate_median_seconds": statistics.median(pair["candidate"] for pair in pairs),
        "baseline_mean_seconds": statistics.fmean(pair["baseline"] for pair in pairs),
        "candidate_mean_seconds": statistics.fmean(pair["candidate"] for pair in pairs),
        "paired_geometric_mean_speedup": ratio,
        "paired_median_speedup": statistics.median(math.exp(value) for value in logs),
        "paired_geometric_mean_time_reduction": 1 - 1 / ratio,
        "confidence": confidence,
        "bootstrap_speedup_interval": interval,
        "bootstrap_time_reduction_interval": [1 - 1 / interval[0], 1 - 1 / interval[1]],
        "bootstrap_resamples": resamples,
        "bootstrap_seed": seed,
        "wins": wins,
        "losses": losses,
        "ties": len(pairs) - wins - losses,
        "exact_two_sided_sign_p": exact_sign_test(wins, losses),
        "log_ratios": logs,
        "order": {
            order: {
                "pairs": len(selected),
                "geometric_mean_speedup": math.exp(statistics.fmean(math.log(pair["baseline"] / pair["candidate"]) for pair in selected)),
            }
            for order in ("AB", "BA")
            if (selected := [pair for pair in pairs if pair.get("order") == order])
        },
    }


def paired_count_statistics(pairs, confidence=0.99, resamples=20000, seed=0):
    if any(pair[variant] <= 0 for pair in pairs for variant in ("baseline", "candidate")):
        return {"pairs": len(pairs), "note": "At least one count is zero; paired ratio statistics omitted."}
    statistics_result = paired_statistics(pairs, confidence, resamples, seed)
    replacements = {"seconds": "count", "speedup": "ratio", "time_reduction": "count_reduction"}
    result = {}
    for name, value in statistics_result.items():
        for old, new in replacements.items():
            name = name.replace(old, new)
        result[name] = value
    result["order"] = {
        order: {"pairs": values["pairs"], "geometric_mean_ratio": values["geometric_mean_speedup"]}
        for order, values in statistics_result["order"].items()
    }
    return result


def final_ir(raw,stderr,expected_argv):
    text=raw.decode();assert text.endswith('\n')and 'positions: line\n'in text and 'events: Ir\n'in text and 'desc: Trigger: Program termination\n'in text
    assert re.findall(r'^part: (\d+)$',text,re.M)==['1']
    cmd=re.findall(r'^cmd:\s*(.*)$',text,re.M);assert cmd==[' '.join(expected_argv)]
    totals=re.findall(r'^totals: (\d+)$',text,re.M);headers=re.findall(r'^summary: (\d+)$',text,re.M);assert len(totals)==len(headers)==1
    final,header=int(totals[0]),int(headers[0]);self_total=0;pending=False
    for line in text.splitlines():
        if line.startswith('calls='):assert not pending;pending=True
        elif line and line[0]in'0123456789+-*':
            match=re.fullmatch(r'(?:\d+|[+-]\d+|\*)\s+(\d+)\s*',line);assert match
            if pending:pending=False
            else:self_total+=int(match[1])
    assert not pending and self_total==final
    collected=re.findall(r'^==\d+== Collected\s*:\s*(\d+)\s*$',stderr,re.M);assert len(collected)==1 and int(collected[0])==final
    return {'final_ir':final,'header_summary_ir':header,'final_minus_header':final-header,'self_cost_sum':self_total,'stderr_collected':int(collected[0]),'brk_overflow_warning':'brk segment overflow'in stderr}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profiles', type=Path, help='Optional sanitized profile directory with PROFILE-MANIFEST.json')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    data = json.loads((root / 'counts.json').read_text())
    manifest = json.loads((root / 'CORE-MANIFEST.json').read_text())
    for name, row in manifest['files'].items():
        raw = (root / name).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], name
    if args.profiles:
        pm = json.loads((args.profiles / 'PROFILE-MANIFEST.json').read_text())
        assert len(pm['profiles']) == 32
        for block in data['blocks']:
            for metric in block['profiles']:
                key = block['name'] + '/' + metric['label']
                row = pm['profiles'][key]
                compressed = (args.profiles / row['file']).read_bytes()
                assert hashlib.sha256(compressed).hexdigest() == row['sha256']
                raw = gzip.decompress(compressed)
                assert hashlib.sha256(raw).hexdigest() == row['raw_sha256']
                observed = final_ir(raw, row['counter_line'] + '\n', ['rustc', '<accepted-private-invocation>'])
                assert observed == {k: v for k, v in metric.items() if k != 'label'}
    assert [b['seed'] for b in data['blocks']] == [20261024101, 20261024102]
    reports = []
    schedules = [['AB','BA','BA','AB','BA','AB','AB','BA'], ['BA','AB','AB','BA','AB','BA','BA','AB']]
    for index, block in enumerate(data['blocks']):
        assert len(block['pairs']) == 8 and len(block['profiles']) == 16
        assert [p['order'] for p in block['pairs']] == schedules[index]
        for pair_index, pair in enumerate(block['pairs']):
            for position, letter in enumerate(pair['order']):
                arm = 'baseline' if letter == 'A' else 'candidate'
                label = f'pair-{pair_index}-{position}-{arm}'
                metric, = [p for p in block['profiles'] if p['label'] == label]
                assert pair[arm] == metric['final_ir'] == metric['self_cost_sum'] == metric['stderr_collected']
                assert metric['final_minus_header'] == metric['final_ir'] - metric['header_summary_ir']
                assert not metric['brk_overflow_warning']
        stats = paired_count_statistics(block['pairs'], confidence=0.99, resamples=200000, seed=block['seed'])
        expected = json.loads((root / 'results' / (block['name'] + '.json')).read_text())['statistics']
        assert stats == expected
        passed = stats['wins'] == 8 and stats['bootstrap_count_reduction_interval'][0] > 0.001 and stats['exact_two_sided_sign_p'] < 0.01
        assert passed
        reports.append({'block': block['name'], 'geometric_reduction': stats['paired_geometric_mean_count_reduction'],
                        'ci99': stats['bootstrap_count_reduction_interval'], 'wins': stats['wins'],
                        'sign_p': stats['exact_two_sided_sign_p'], 'instruction_gate_passed': passed})
    print(json.dumps({'confirmations': reports, 'prospective_concurrent_two_host_protocol_complete': True, 'native_or_test_build_claim': False,
                      'private_collection_validation_replayed': False}, indent=2))


if __name__ == '__main__':
    main()
