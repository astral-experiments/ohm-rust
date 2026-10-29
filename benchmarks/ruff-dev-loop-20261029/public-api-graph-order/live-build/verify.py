"""Verify the four-installation native Ruff timing summary from each individual elapsed time."""
from itertools import permutations
import hashlib
import json
import math
from pathlib import Path
import random
import statistics

base=Path(__file__).resolve().parent
if not __debug__: raise SystemExit("Run without Python optimization.")
case=json.loads((base/"case.json").read_bytes())
order=json.loads((base/"schedule.json").read_bytes())
roles=("A1","B1","B2","A2")
assert hashlib.sha256((base/"REGISTRATION.sha256").read_bytes()).hexdigest()==case["protocol_manifest"]
assert all(set(map(tuple,sequence))==set(permutations(roles)) and len(sequence)==24 for sequence in order.values())
subjects=case["subjects"]
assert len({tuple(s["driver_inode"]) for s in subjects.values()})==4
for k in ("rustc","wild","cargo"):
    assert len({s["artifacts"][k] for s in subjects.values()})==1
assert {s:subjects[s+"1"]["artifacts"]["driver"]==subjects[s+"2"]["artifacts"]["driver"] for s in "AB"}=={"A":True,"B":True}
assert subjects["A1"]["artifacts"]["driver"]!=subjects["B1"]["artifacts"]["driver"]
rows=case["rows"]
assert [[r["block"],r["direction"],r["lane"]] for r in rows]==[[i+1,s,n] for i in range(24) for s in ("new","old") for n in order[s][i]]
for r in rows:
    assert r["dirty"]["raw"]==r["fresh"]["raw"]
    assert all(r[c]["wall_ns"]>0 and r[c]["usage"]["ru_utime"]>0 for c in ("dirty","fresh"))
ref=case["reference"]
assert [ref[s]["test"]["rc"] for s in ("old","new")]==[101,0]
assert ref["old"]["outputs"]!=ref["new"]["outputs"] and ref["old"]["test"]["inventory"]==ref["new"]["test"]["inventory"]
for s in ("old","new"):
    assert [n[0] for n in ref[s]["typed"] if not n[3]]==["ruff_source_file"]
    for k in ("dirty","fresh"):
        d=[]
        for i in range(24):
            x={r["lane"]:math.log(r[k]["wall_ns"]/1e6) for r in rows if r["direction"]==s and r["block"]==i+1}
            d.append((x["B1"]+x["B2"]-x["A1"]-x["A2"])/2)
        saving=lambda v:100*(1-math.exp(v))
        rng=random.Random(26120261029)
        boot=sorted(sum(rng.choices(d,k=24))/24 for _ in range(50000))
        ci=[saving(boot[48750]),saving(boot[1249])]
        actual=case["statistics"][s+"-"+k]["wall"]
        assert actual["percent_saved"]==saving(statistics.mean(d)) and actual["ci95"]==ci
        print(s,k,f'{actual["percent_saved"]:.3f}%', f'[{ci[0]:.3f}%, {ci[1]:.3f}%]')
