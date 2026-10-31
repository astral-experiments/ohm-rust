"""Verify native uv #20988 clocks and recompute the two independent held-out seed results."""
from pathlib import Path
from statistics import median
from zipfile import ZipFile
import csv
import hashlib
import io
import itertools
import json
import math
import random
import re
import runpy

ROOT=Path(__file__).resolve().parent
recipe=runpy.run_path(str(ROOT/"statistics_recipe.py"))
median_interval,sign_p=recipe["median_interval"],recipe["sign_p"]
ARMS=("stock","parent","candidate")
SEEDS={"T1":[209880004,209880015,209880023],"T3":[209884229,209884243,209884267]}
CMD=["/tmp/uv-workflow-search/sdk/bin/cargo","check","-p","uv-resolver","--locked","--offline","-j8"]
CARGO="02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573"
PATCH="24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb"
ROSTER={"uv_workspace","uv_types","uv_distribution","uv_resolver"}
digest=lambda b:hashlib.sha256(b).hexdigest()


def clock(z,label):
    q=json.loads(z.read(label+".json")); log=z.read(label+".stderr")
    units=[n.decode().replace("-","_") for n in re.findall(rb"^\s*(?:Checking|Compiling) ([\w-]+) v",log,re.M)]
    assert q["cmd"]==CMD and q["dirty_units"]==units and math.isfinite(q["wall_s"]) and q["wall_s"]>0
    assert z.read(label+".stdout")==b"" and b"Finished " in log
    u=json.loads(z.read(label+".usage.json"))
    assert min(u["sys_us"],u["user_us"])>=0 and u["sys_us"]+u["user_us"]>0
    return q["wall_s"],units,u["user_us"]+u["sys_us"]


def analyze(packet,stage):
    with ZipFile(packet) as z:
        raw=z.read;load=lambda n:json.loads(raw(n))
        members=load("SHA256.json")
        assert set(members)|{"SHA256.json"}==set(z.namelist()) and len(set(z.namelist()))==len(z.namelist())
        assert all(digest(raw(n))==v for n,v in members.items()) and digest(raw("uv-20988.patch"))==PATCH
        subjects=load("compiler-receipts.json");official=load("registered-result.json")
        assert subjects["stage"]==stage and subjects["roles"]["stock"]["driver_sha256"]=="a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de"
        cohorts=[];flat=[]
        for cohort,seed in zip(("discovery","holdout-1","holdout-2"),SEEDS[stage],strict=True):
            plan=load(cohort+"/plan.json");seq=list(itertools.permutations(sorted(ARMS)));random.Random(seed).shuffle(seq)
            seq=list(map(list,seq));assert plan["seed"]==seed and plan["orders"]==seq
            rows=[json.loads(l) for l in raw(cohort+"/pairs.jsonl").splitlines()]
            assert len(rows)==6 and [list(r) for r in rows]==seq
            for arm in ARMS:
                prefix=f"{cohort}/{arm}/";identity=load(prefix+"identity.json")
                assert identity["cargo"]==plan["inputs"][arm]["cargo"]==CARGO and identity["launcher"]==plan["inputs"][arm]["launcher"]
                libraries=identity["linked"];[driver]=[v["sha256"] for k,v in libraries.items() if k.startswith("librustc_driver")]
                assert driver==subjects["roles"][arm]["driver_sha256"] and driver in plan["inputs"][arm]["drivers"].values()
                assert [v["sha256"] for k,v in libraries.items() if k.startswith("libLLVM")]==[subjects["external_llvm_sha256"]]
                protocol=[json.loads(i[7:]) for i in raw(prefix+"protocol.log").splitlines() if i.startswith(b"UVPAIR ")]
                assert protocol[0]=={"state":"ready","patch":PATCH} and protocol[1:]==[row[arm] for row in rows]
                with ZipFile(io.BytesIO(raw(prefix+"setup.zip"))) as setup:
                    assert clock(setup,"warm")[1] and not clock(setup,"settled")[1]
                for i,row in enumerate(rows,1):
                    v=row[arm];assert v["trial"]==str(i) and v["state"]=="measured"
                    with ZipFile(io.BytesIO(raw(prefix+f"pair-{i}.zip")))as pair:
                        a,au,_=clock(pair,"settled-A");b,bu,cpu=clock(pair,"edit-D");n,nu,_=clock(pair,"noop-D")
                    assert [a,b,n]==[v["parent_noop"],v["edit"],v["noop"]] and not au and not nu
                    assert bu==v["dirty"] and {"uv_workspace","uv_resolver"}<=set(bu)<=ROSTER
                    if stage=="T3":assert len(bu)==len(ROSTER) and set(bu)==ROSTER
                    flat.append([cohort,i,",".join(seq[i-1]),arm,a,b,n,cpu])
            cohorts.append(rows)
        times=list(csv.reader(io.StringIO(raw("times.csv").decode())))
        assert times[1:]==[[str(v) for v in row] for row in flat]
    held=cohorts[1]+cohorts[2]
    summary={"held_out_triples":12,"discovery_excluded":6,"cold_parent_setups_in_holdouts":6,
             "wall_s":{a:{k:median(r[a][k] for r in held) for k in ("edit","parent_noop","noop")} for a in ARMS},"comparisons":{}}
    for arm in ARMS[:2]:
        values=[r[arm]["edit"]/r["candidate"]["edit"] for r in held]
        order=[[v for v,r in zip(values,held) if (list(r).index(arm)<list(r).index("candidate"))==b] for b in (False,True)]
        assert list(map(len,order))==[6,6]
        stats=dict(median=median(values),interval=median_interval(values),p_above_one=sign_p(values,1),p_above_two=sign_p(values,2),
                   seed_medians=[median(values[:6]),median(values[6:])],order_medians=list(map(median,order)),paired=values)
        baseline=official["contrasts"][arm] if stage=="T3" else official["statistics"]["comparisons"][arm]
        assert stats["median"]==baseline["median"] and stats["interval"]==baseline["interval" if stage=="T3" else "median_interval"]
        assert stats["p_above_one"]==baseline["sign_p_above_one"]
        summary["comparisons"][arm]=stats
    s=summary["comparisons"]["stock"];m=summary["comparisons"]["parent"]
    summary["twofold"]=s["interval"][0]>2 and s["p_above_two"]<.05 and min(s["order_medians"]+s["seed_medians"])>2 and min(m["seed_medians"])>1
    summary["practical"]=s["interval"][0]>1 and m["interval"][0]>1 and min(s["seed_medians"])>=1.2 and min(m["seed_medians"])>=(1.10 if stage=="T3" else 1.30)
    if stage=="T3":summary["practical"] &= s["p_above_one"]<.05 and m["p_above_one"]<.05 and min(m["order_medians"])>1
    two=official["twofold"] if stage=="T3" else official["held_out"]
    assert summary["twofold"]==two["qualified"] and s["p_above_two"]==two["sign_p_above_two"]
    assert summary["practical"]==(official["material"] if stage=="T3" else two["practical"])
    assert summary["wall_s"]==(official["wall_seconds"] if stage=="T3" else official["statistics"]["roles"])
    return summary


if __name__=="__main__":
    assert __debug__
    results={}
    for stage in ("T3","T1"):
        packet=ROOT/f"records-{stage}.zip"
        if stage=="T1" and not packet.exists():continue
        assert digest(packet.read_bytes())==(ROOT/f"records-{stage}.sha256").read_text().split()[0]
        results[stage]=analyze(packet,stage)
    print(json.dumps(results,indent=2))
