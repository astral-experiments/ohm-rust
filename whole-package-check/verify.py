"""Read the six original whole-uv first-edit trios. Python 3.11 or later."""
import hashlib
import io
import itertools
import json
from pathlib import Path
import re
import statistics
import sys
from zipfile import ZipFile

roles = ("stock", "parent", "candidate")
phases = ("settled-A", "edit-D", "noop-D")
packages = {"uv", "uv_build_frontend", "uv_cli", "uv_dispatch", "uv_distribution", "uv_installer",
            "uv_requirements", "uv_resolver", "uv_scripts", "uv_settings", "uv_tool", "uv_types", "uv_workspace"}
products = {"lib-" + name for name in packages} | {"bin-uv", "bin-uvx"}
command = ["/tmp/uv-workflow-search/sdk/bin/cargo", "check", "-p", "uv", "--locked", "--offline", "-j8"]
median = statistics.median


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_cohort(path):
    with ZipFile(path) as data:
        assert data.testzip() is None and len(set(data.namelist())) == len(data.namelist())
        assert digest(data.read("closed.json")) == "40a0fe6078d4a5af596536a3f6e3e8b7b1f1aae79c58874e6c59e6245eb921ed"
        closure = json.loads(data.read("closed.json"))
        assert closure["all_workers_returned_zero"] is True and len(closure["files"]) == 60
        assert all(digest(data.read(n)) == h for n, h in closure["files"].items())
        assert digest(data.read("registration.json")) == closure["registration"]
        assert digest(data.read("admission.json")) == closure["admitted"]
        assert json.loads(data.read("subject-intake.json")) == json.loads(data.read("admission.json"))
        reg = json.loads(data.read("registration.json"))
        admitted = json.loads(data.read("subject-intake.json"))
        orders = json.loads(data.read("plan.json"))["sequence"]
        assert orders == reg["orders"]["discovery"] and len(set(map(tuple, orders))) == 6
        assert set(map(tuple, orders)) == set(itertools.permutations(roles))
        assert admitted["cargo"] == "02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573"
        pairs = [json.loads(line) for line in data.read("pairs.jsonl").splitlines()]
        assert len(pairs) == 6
        for arm in roles:
            assert admitted["arms"][arm]["driver_sha256"] == reg["role_pins"][arm]["driver"]
            assert digest(data.read(arm + "/edit.patch")) == "24852ea0d29fe8eb355f2a0d4eb682c54e53fa56af3d048635f4de9049925ecb"
            meta = json.loads(data.read(arm + "/metadata.json"))
            for state, lib in itertools.product(("A", "B"), ("uv", "uv_workspace")):
                product = meta[state]["lib-" + lib]
                listing = data.read(f"{arm}/{state}-{lib}.metadata-list").decode()
                assert re.fullmatch(r"[a-f0-9]{64}", product["metadata_sha256"])
                assert re.search(r"^name " + lib + r"-[a-f0-9]+$", listing, re.M)
                assert re.search("^public_hash " + product["public_hash"] + "$", listing, re.M)
                assert "triple x86_64-unknown-linux-gnu" in listing
        for index, pair in enumerate(pairs, start=1):
            for arm in roles:
                with ZipFile(io.BytesIO(data.read(f"{arm}/pair-{index}.zip"))) as records:
                    snapshots = []
                    for phase, field in zip(phases, ("parent_noop", "edit", "noop")):
                        record = json.loads(records.read(phase + ".json"))
                        assert record["cmd"] == command and record["wall_s"] == pair[arm][field] > 0
                        stderr = records.read(phase + ".stderr")
                        dirty = [p.decode().replace("-", "_") for p in re.findall(rb"^\s*(?:Checking|Compiling) ([\w-]+) v", stderr, re.M)]
                        expected = packages if phase == "edit-D" else set()
                        assert set(dirty) == expected and len(dirty) == len(expected)
                        assert dirty == record["dirty_units"] and b"Finished" in stderr
                        snap = json.loads(records.read(phase + ".all-products.json"))
                        assert set(snap) == products
                        snapshots.append(snap)
                    before, after, settled = snapshots
                    assert after == settled and all(after[n]["mtime"] > before[n]["mtime"] for n in products)
        seconds = [{a: p[a]["edit"] for a in roles} for p in pairs]
        ratio = [r["stock"] / r["candidate"] for r in seconds]
        mech = [r["parent"] / r["candidate"] for r in seconds]
        out = dict(first_edit_seconds=seconds, median_seconds={a: median(r[a] for r in seconds) for a in roles},
                   stock_over_candidate=median(ratio), individual_range=[min(ratio), max(ratio)],
                   parent_over_candidate=median(mech), faster_than_parent=sum(q > 1 for q in mech),
                   delta_to_2x_seconds=median(r["candidate"] - r["stock"] / 2 for r in seconds),
                   opens_confirmation=median(ratio) >= 1.8 and median(mech) > 1,
                   discovery_trios=6, heldout_trios=0, first_edits=18, unchanged_commands=36,
                   dirty_packages_per_edit=13, target_products_per_edit=15, native_metadata_listings=12)
        assert not out["opens_confirmation"]
        return out


if __name__ == "__main__":
    assert __debug__
    print(json.dumps(read_cohort(Path(sys.argv[1])), sort_keys=True, indent=2))
