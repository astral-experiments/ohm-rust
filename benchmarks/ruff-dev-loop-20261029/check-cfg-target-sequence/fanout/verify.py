"""Read the public paired observations and verify the registered five-compiler result."""

import hashlib
import importlib.util
import json
from pathlib import Path
import random


def read(path):
    return json.loads(path.read_bytes())


here = Path(__file__).resolve().parent
plan_path = here / "registration.json"
assert hashlib.sha256(plan_path.read_bytes()).hexdigest() == "6d6582494b6b18c5ef1ac9288fd84b6fc4a0c5efa5775eb60dbb2b29952477aa"
plan = read(plan_path)
native = read(here / "native.json")
rows = read(here / "observations.json")
proof = read(here / "products.json")
expected = read(here / "outcomes.json")

# Use the same analysis that read the private original per-PID Callgrind files.
spec = importlib.util.spec_from_file_location("raw_observations", here / "raw-reader.py")
raw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(raw)

rng = random.Random(plan["schedule_seed"])
for direction in "PH":
    order = ["AD"] * 6 + ["DA"] * 6
    rng.shuffle(order)
    assert plan["schedule"][direction] == order

assert native["compiler"] == plan["input"]
assert native["direct_opens_of_six_registry_outputs"] == 0
assert sum(sum(arm.values()) for arm in native["all_six_per_process_open_census"].values()) == 6719
roster = raw.ROLE_NAMES
for arm in "AD":
    data = native["natural"][arm]
    for name in ("warm/addition", "warm/removal", "warm/reapply"):
        assert set(data[name]["full_compilers"]) == roster
        assert data[name]["stock_wild_jobs"] == 4
        assert data[name]["fresh_compilers"] == data[name]["fresh_linkers"] == 0

slots = {f"{arm}-pair-{i:02d}-{state}" for arm in "AD" for i in range(1, 13) for state in "PH"}
assert set(rows) == set(proof) == slots
assert len({v["raw_slot"] for v in rows.values()}) == 48
full_rustcs = links = 0
for name in sorted(slots):
    arm, _, _, state = name.split("-")
    row, observed = rows[name], proof[name]
    assert (row["arm"], row["direction"]) == (arm, state)
    current = native["natural"][arm][f"cold-{state}/empty"]
    assert row["reader"] == current["verdict"] and row["reader_elf"] == current["binary"]
    assert set(row["compiled"]) == set(observed["rustc_argv_sha256"]) == set(observed["rustc_profile_sha256"]) == roster
    for metric in ("total", "fill"):
        assert row[metric] == sum(c[metric] for c in row["compiled"].values())
    assert all(v["total"] > v["fill"] > 0 and v["fill_calls"] > 0 for v in row["compiled"].values())
    assert observed["raw_archive"] == row["raw_slot"]
    assert observed["fresh_units"] == 11 and observed["fresh_jobs"] == 0
    assert len(observed["wild_argv_sha256"]) == 4
    assert observed["harness"] == current["test_harness"]
    assert sum(int(data["cases"]) for data in observed["harness"].values()) == 10
    files = observed["files"]
    assert len(files) == 16
    differences = [f for f in files if f["sha256"] != f["clean_sha256"]]
    assert all(f["target"] == "ruff_text_size" and f["filename"].endswith(".rlib") for f in differences)
    assert len(differences) <= 1
    full_rustcs += len(row["compiled"])
    links += len(observed["wild_argv_sha256"])

replay = read(here / "replay-sanity.json")
assert replay["script_sha256"] == hashlib.sha256((here / "replay.py").read_bytes()).hexdigest()
assert replay["additional_guest_profiles"]["copies"] == 10 and replay["additional_guest_profiles"]["stock_links"] == 8
for arm in "AD":
    sample = replay["states"][arm]["all_five_native_phases"]
    assert len(sample) == 5
    for warm, clean in (("add", "empty-added"), ("remove", "empty-removed")):
        assert sample[warm]["tests"] == sample[clean]["tests"]
        assert sample[warm]["external"] == sample[clean]["external"]
        assert sum(sample[warm]["counts"].values()) == 10
    assert sample["remove"]["external"] == "E0599" and len(sample["add"]["external"]) == 64
    final = read(here / "final-archives.json")[arm]
    candidates = [f for f in proof[final["slot"]]["files"] if f["target"] == "ruff_text_size" and f["filename"].endswith(".rlib")]
    item, = candidates
    assert [item["sha256"], item["clean_sha256"], item["bytes"]] == [final["recorded_sha256"], final["independent_empty_sha256"], final["bytes_each"]]
    assert final["reconstructed_entire_file"] and len(set(final["actual_names"])) == 2

outcomes = raw.analysis(rows, plan)
assert outcomes == expected["effects"]
assert expected["passes"] == all(value["passes"] for value in outcomes.values())
print(json.dumps({"full_rustc": full_rustcs, "physical_links": links, "passes": expected["passes"],
                  "paired_reductions": {d: outcomes[d]["median_paired"] for d in "PH"}}, sort_keys=True))
