"""Check exactly the ordinary Cargo visits retained from the interrupted cold-parent experiment."""
from hashlib import sha256
import itertools
import json
from pathlib import Path
from random import Random
import re
from zipfile import ZipFile


def digest(data):
    return sha256(data).hexdigest()


def verify(path):
    with ZipFile(path) as z:
        manifest = json.loads(z.read("MANIFEST.json"))
        assert len(z.namelist()) == len(set(z.namelist())) == len(manifest) + 1
        raw = {name: z.read(name) for name in manifest}
        assert {name: digest(data) for name, data in raw.items()} == manifest
    plan = json.loads(raw["records/plan.json"])
    inputs = json.loads(raw["inputs.json"])
    policy = "613dff69e9382a658d1de956b1bcc3134ea3d1fd6b79e019450e19dbbd1d7452"
    receipt = json.loads(raw["return.json"])
    assert inputs["registration"] == receipt["registration"] == policy
    assert receipt["exit"] == 1 and receipt["verified_sha256"] is None
    assert digest(raw["records/edit.patch"]) == plan["patch_sha256"] == inputs["source"]["patch"]
    assert plan["cargo"] == inputs["cargo_custody"]["explicit_cargo"] == "02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573"
    assert plan["command"][1:] == ["check", "-p", "uv-resolver", "--locked", "--offline", "-j8"]
    assert plan["cpus"] == list(range(16)) and plan["seeds"] == [406831121, 406831127]
    schedule = []
    for seed in plan["seeds"]:
        block = list(itertools.permutations(("candidate", "parent", "stock")))
        Random(seed).shuffle(block)
        schedule.extend(map(list, block))
    assert schedule == plan["order"]
    for role, sdk in plan["sdks"].items():
        identity = inputs["roles"][role]
        assert (sdk["librustc_driver"]["sha256"], sdk["libLLVM"]["sha256"], sdk["launcher"]) == (identity["driver"], identity["llvm"], identity["launcher"])

    expected = {"records/plan.json", "records/edit.patch", "inputs.json", "enrolled.json", "return.json"}
    finished = completed = last_end = 0
    for number, order in enumerate(schedule[:7], 1):
        for role in order:
            slot = f"{number:02}-{role}"
            phases = ("cold-A",) if slot == "07-parent" else ("cold-A", "noop-A", "edit", "noop-B")
            for phase in phases:
                key = "records/" + slot + "/" + phase
                obs, out, err = (raw[key + ext] for ext in (".json", ".stdout", ".stderr"))
                usage = "usage/" + slot + "-" + phase + ".json"
                expected.update([key + ".json", key + ".stdout", key + ".stderr", usage])
                c = json.loads(obs)
                assert c["cpus"] == plan["cpus"] and c["argv"] == plan["command"]
                assert digest(out) == c["stdout_sha256"] and digest(err) == c["stderr_sha256"]
                assert c["end_ns"] - c["start_ns"] == c["elapsed_ns"] > 0 and c["start_ns"] >= last_end
                last_end = c["end_ns"]
                dirty = sorted(n.decode() for n in re.findall(rb"^\s*(?:Checking|Compiling) ([\w-]+) v", err, re.M))
                assert c["dirty"] == dirty
                stats = json.loads(raw[usage])
                assert set(stats) == {"user_us", "system_us", "minor_faults", "major_faults", "voluntary_context_switches", "involuntary_context_switches"}
                assert all(type(x) is int and x >= 0 for x in stats.values())
                if slot == "07-parent":
                    assert c["exit"] == 101 and not dirty and b"(never executed)" in err and b"No such file or directory (os error 2)" in err
                    assert digest(err) == "78651906303e421a36fecee10f723a8146eae165e7398aef5556d15b3e65ccdd"
                else:
                    assert c["exit"] == 0 and b"Finished " in err
                    if phase == "cold-A":
                        assert "uv-resolver" in dirty
                    else:
                        assert dirty == (["uv-distribution", "uv-resolver", "uv-types", "uv-workspace"] if phase == "edit" else [])
                    finished += 1
            completed += len(phases) == 4
    assert set(raw) == expected and (completed, finished) == (20, 80)
    return {"complete_trios": 6, "complete_visits": completed, "successful_Cargo_calls": finished,
            "failed_cold_calls": 1, "failed_role": "07-parent", "signed_Cargo_calls": finished + 1,
            "scored_trios": 0, "original_worker_exit": receipt["exit"]}


if __name__ == "__main__":
    assert __debug__
    print(json.dumps(verify(Path(__file__).with_name("records.zip")), indent=2, sort_keys=True))
