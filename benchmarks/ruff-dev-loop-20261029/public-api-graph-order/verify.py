"""Recompute the paired statistics and verify the ordinary Cargo, profile and output receipts."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import tarfile


HERE = Path(__file__).resolve().parent
summarize = runpy.run_path(str(HERE / "replay/score.py"))["summarize"]


def logical(files):
    return {re.sub(r"/v2-[0-9a-f]{16}/", "/<compiler>/", key): value for key, value in files.items()}


def verify(case, raw):
    rows, native, profiles = case["rows"], case["native"], case["profiles"]
    scores = {name: summarize(rows, name, n) for name, n in [("wall", 20), ("profile", 12)]}
    assert scores == case["statistics"] and len(profiles) == 24
    a, b = native["A"], native["B"]
    for key in ("edit", "ruff", "cargo", "wild", "rustc", "sources"):
        assert a["subject"][key] == b["subject"][key]
    assert a["subject"]["driver"] != b["subject"]["driver"]
    assert a["environment_hash"] == b["environment_hash"]
    assert a["cargo"] == b["cargo"] and a["cargo_artifacts"] == b["cargo_artifacts"]
    assert a["test_verdicts"] == b["test_verdicts"]
    assert a["test_verdicts"]["previous"]["exit"] == 101 and a["test_verdicts"]["current"]["exit"] == 0
    assert {k: v["identity"] for k, v in a["externs"].items()} == {k: v["identity"] for k, v in b["externs"].items()}
    assert a["direct_output_identities"] == b["direct_output_identities"]
    assert logical(a["dirty_output_identities"]) == logical(b["dirty_output_identities"])
    for role in (a, b):
        assert role["dirty_rustc"] == [case["crate"]] and role["dirty_links"] == int(case["mode"] == "test")
        assert role["unchanged"] == {"rustc": [], "wild": 0}
        assert role["dirty_output_identities"] == role["clean_output_identities"]
        assert role["extern_operands"] == len(role["externs"])
        graph = role["genuine_hardlinks"]
        assert graph["regular_paths"] - graph["physical_files"] == sum(len(group)-1 for group in graph["groups"])
        assert len(graph["groups"]) > 0
    for row in rows:
        if row["phase"] == "profile":
            for arm in "AB":
                label = f"profile-{row['i']:02}-{arm}"
                measurement = profiles[label]
                assert measurement["ir"] == row["values"][arm]
                assert measurement["outputs"] == a["direct_output_identities"]
                assert 0 <= measurement["graph"]["inclusive"] <= measurement["ir"]
    if raw is not None:
        for name, digest in case["raw_archives"].items():
            with (raw / name).open("rb") as stream:
                assert hashlib.file_digest(stream, "sha256").hexdigest() == digest
        with tarfile.open(raw / "receipts.tgz") as receipts:
            for label, measurement in profiles.items():
                original = json.load(receipts.extractfile(f"{label}/measurement.json"))
                assert all(original[key] == value for key, value in measurement.items() if key != "graph")
        with tarfile.open(raw / "profiles.tar.xz", "r|xz") as archives:
            seen = set()
            for member in archives:
                label = member.name.removesuffix(".callgrind")
                assert label not in seen
                data = archives.extractfile(member).read()
                record = profiles[label]
                assert len(data) == record["bytes"] and hashlib.sha256(data).hexdigest() == record["raw_sha256"]
                assert b"\nevents: Ir\n" in data and re.findall(rb"^totals: (\d+)$", data, re.M) == [str(record["ir"]).encode()]
                seen.add(label)
            assert seen == set(profiles)
    return scores


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", action="append", default=[], metavar="CASE=DIR", help="also reopen the original recordings for this case")
    options = parser.parse_args()
    if not __debug__:
        parser.error("run without Python optimization so validation assertions execute")
    sources = {case: Path(path) for case, path in (spec.split("=", 1) for spec in options.raw)}
    cases = sorted((HERE / "cases").glob("*.json"))
    assert cases and sources.keys() <= {path.stem for path in cases}
    for path in cases:
        data = json.loads(path.read_bytes())
        stats = verify(data, sources.get(path.stem))
        print(path.stem, json.dumps(stats, sort_keys=True))
