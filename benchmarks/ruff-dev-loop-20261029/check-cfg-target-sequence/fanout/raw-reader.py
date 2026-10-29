"""Verify all registered natural compiler counts from their physical per-command archives."""
from __future__ import annotations
import argparse
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import random
import re
from statistics import median
import tarfile

ROLE_NAMES = {"ruff_text_size", "auto_traits", "constructors", "indexing", "main"}
FILL = b"CheckCfg>::fill_well_known"
SYMBOL = re.compile(rb"\((\d+)\)(?: (.*))?")

def digest(b):
    return hashlib.sha256(b).hexdigest()

def raw_file(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

def profile(data, pid, crate):
    self_total = 0
    counted = set()
    calls = defaultdict(lambda: [0, 0])
    current = callee = pending = reported = header_pid = None
    saw_command = saw_events = False
    for line in data.splitlines():
        if line.startswith((b"fn=", b"cfn=")):
            label, definition = line.split(b"=", 1)
            ident = SYMBOL.fullmatch(definition)
            assert ident, line
            key = int(ident[1])
            if ident[2] is not None and FILL in ident[2]:
                counted.add(key)
            if label == b"fn":
                current = key
            else:
                callee = key
        elif line.startswith(b"calls="):
            assert pending is None and callee is not None
            pending = int(line.split(b"=", 1)[1].split()[0])
        elif line[:1] in (b"+", b"-", b"*") or line[:1].isdigit():
            fields = line.split()
            assert len(fields) == 2, line
            cost = int(fields[1])
            if pending is None:
                self_total += cost
            else:
                edge = calls[current, callee]
                edge[0] += pending
                edge[1] += cost
                pending = None
        elif line.startswith(b"totals: "):
            assert reported is None
            reported = int(line.split()[1])
        elif line.startswith(b"pid: "):
            header_pid = int(line.split()[1])
        elif line.startswith(b"cmd: "):
            saw_command = ("--crate-name " + crate + " ").encode() in line
        elif line.startswith(b"events: "):
            saw_events = line == b"events: Ir"
    assert reported == self_total and self_total > 25_000_000 and header_pid == pid and saw_command and saw_events
    edges = [(n, cost) for (a, b), (n, cost) in calls.items() if b in counted and a not in counted]
    assert edges and sum(cost for _, cost in edges) > 0
    return {"total": reported, "fill": sum(cost for _, cost in edges), "fill_calls": sum(x for x, _ in edges)}

def slot(task):
    base, name, q = task
    receipt = json.loads((base / "slots" / (name + ".json")).read_bytes())
    raw = base / "raw" / (name + ".tar.gz")
    assert receipt["native"] == "complete" and raw.stat().st_size == receipt["bytes"] and raw_file(raw) == receipt["archive"]
    files = receipt["member"]; seen = set(); roles = {}; profiles = {}; meta = {}; products = {}; maps = {}
    with tarfile.open(raw, "r|gz") as ar:
        for m in ar:
            if m.isdir():
                continue
            key = m.name.removeprefix("./")
            assert key in files and key not in seen and m.isfile()
            seen.add(key)
            body = ar.extractfile(m).read()
            assert files[key] == [m.size, m.mode, digest(body)]
            if key == "native.json":
                meta = json.loads(body)
            elif key == "maps.json":
                maps = json.loads(body)
            elif key == "after/products.json":
                products = json.loads(body)
            elif match := re.fullmatch(r'guest/rustc\.(\d+)\.argv', key):
                argv = body.rstrip(b'\0').decode().split('\0')
                crate = argv[argv.index("--crate-name") + 1]
                assert crate in ROLE_NAMES and "--check-cfg" in argv and "-Zlink-only" not in argv and "link-only" not in argv
                roles[int(match[1])] = [crate, digest(body)]
            elif match := re.fullmatch(r"guest/output\.(\d+)\.(\d+)", key):
                pid1, pid2 = map(int, match.groups())
                assert pid1 == pid2 and pid1 not in profiles
                cmd = next(x for x in body.split(b'\n', 30)[:30] if x.startswith(b"cmd: "))
                crate = cmd.split(b" --crate-name ", 1)[1].split(b" ", 1)[0].decode()
                assert crate in ROLE_NAMES
                profiles[pid1] = [crate, digest(body), profile(body, pid1, crate)]
    assert set(files) == seen and set(roles) == set(profiles) == set(map(int, maps)) and len(profiles) == 5
    arm = receipt["role"]; direction = receipt["slot"][-1]
    assert meta["role"] == arm and meta["source"] == direction and meta["label"] == receipt["slot"] and meta["wilds"] == 4 and meta["measurement_eligible"]
    assert len([f for f in files if re.fullmatch(r"guest/wild\.\d+\.argv", f)]) == 4
    assert products["fresh_jobs"] == 0 and products["fresh_units"] == 11 and len(products["selected_files"]) == 16
    assert {name for name, value in products["harness"].items()} == ROLE_NAMES - {"ruff_text_size"}
    assert sum(int(x["cases"]) for x in products["harness"].values()) == 10
    assert products["reader"] == ("1..4\n" if direction == "H" else "E0599")
    result = {}
    for pid in sorted(roles):
        crate, argv_sha = roles[pid]; row = meta["individual_full_rustc"][crate]
        assert row["pid"] == pid and row["argv_sha256"] == argv_sha and row["driver"] == q["input"]["driver"][arm]
        assert len(row["original_jobserver_fds"]) == 2 and len(set(row["original_jobserver_fds"])) == 2
        assert profiles[pid][:2] == [crate, row["profile_sha"]]
        assert Path(maps[str(pid)][0]).name.startswith("callgrind-") and maps[str(pid)][1]
        result[crate] = profiles[pid][2]
    assert set(result) == ROLE_NAMES
    return name, {"arm": arm, "direction": direction, "raw_slot": receipt["archive"], "compiled": result,
                  "total": sum(x["total"] for x in result.values()), "fill": sum(x["fill"] for x in result.values()),
                  "reader": products["reader"], "reader_elf": products["reader_elf"]}

def quantile(values, p):
    xs = sorted(values); at = (len(xs) - 1) * p; i = int(at)
    return xs[i] + (xs[min(len(xs) - 1, i + 1)] - xs[i]) * (at - i)

def signs(values):
    nonzero = [x for x in values if x != 0]
    count = sum(x > 0 for x in nonzero); last = min(count, len(nonzero) - count)
    return min(1, 2 * sum(math.comb(len(nonzero), k) for k in range(last+1)) / 2**len(nonzero))

def analysis(rows, plan):
    rng = random.Random(plan["bootstrap_seed"]); effects = dict.fromkeys('PH')
    for source in 'PH':
        values = []; grouped = {"AD": [], "DA": []}; own = {"A": [], "D": []}; fills = {"A": [], "D": []}
        per = {n: {"A": [], "D": []} for n in ROLE_NAMES}
        for i, order in enumerate(plan["schedule"][source]):
            case = {a: rows[f"{a}-pair-{i + 1:02d}-{source}"] for a in "AD"}
            for a, v in case.items():
                own[a].append(v["total"]); fills[a].append(v["fill"])
                for name, metric in v["compiled"].items():
                    per[name][a].append(metric["total"])
            ratio = 1 - case["D"]["total"] / case["A"]["total"]
            values.append(ratio); grouped[order].append(ratio)
        assert len(values) == 12 and len(grouped['AD']) == len(grouped['DA']) == 6
        ci = [median(rng.choices(grouped['AD'], k=6) + rng.choices(grouped['DA'], k=6)) for _ in range(50_000)]
        effects[source] = {"pairs": 12, "median_paired": median(values), "ci95": [quantile(ci,.025),quantile(ci,.975)],
                           "exact_sign_p": signs(values), "favorable": sum(x > 0 for x in values),
                           "median_total": {a: median(v) for a,v in own.items()}, "median_fill": {a: median(v) for a,v in fills.items()},
                           "individual_medians": {k: {a: median(v) for a,v in w.items()} for k,w in sorted(per.items())},
                           "order_medians": {k: median(v) for k,v in grouped.items()}}
    ceiling = 0
    for j,s in enumerate(sorted('PH', key=lambda k: effects[k]["exact_sign_p"])):
        e = effects[s]; ceiling = max(ceiling, min(1, (2-j)*e["exact_sign_p"])); e["holm_p"] = ceiling
        e["passes"] = e["median_paired"] > 0 and e["ci95"][0] > .001 and e["favorable"] >= 10 and ceiling < .05
    return effects

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("result", type=Path); parser.add_argument("registration", type=Path); parser.add_argument("output", type=Path)
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    assert args.jobs in (1,2,3,4) and (args.result/"exit").read_text().strip() == "0"
    plan = json.loads(args.registration.read_bytes())
    assert raw_file(args.registration) == "6d6582494b6b18c5ef1ac9288fd84b6fc4a0c5efa5775eb60dbb2b29952477aa"
    ref = random.Random(plan["schedule_seed"])
    for d in 'PH':
        expected = ['AD']*6 + ['DA']*6;ref.shuffle(expected);assert plan['schedule'][d] == expected
    paths = sorted(p.stem for p in (args.result/'slots').glob('*.json'))
    assert set(paths) == {f"{a}-pair-{i:02d}-{s}" for a in 'AD' for i in range(1,13) for s in 'PH'}
    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        rows = dict(pool.map(slot, [(args.result, name, plan) for name in paths]))
    effects = analysis(rows,plan)
    args.output.mkdir()
    (args.output/'observations.json').write_text(json.dumps(rows, sort_keys=True, indent=2)+'\n')
    (args.output/'outcomes.json').write_text(json.dumps({'effects':effects,'passes':all(s['passes'] for s in effects.values())}, sort_keys=True, indent=2)+'\n')
    print(json.dumps(effects,sort_keys=True))
if __name__ == "__main__":
    main()
