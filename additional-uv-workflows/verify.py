"""Replay the original Cargo and client records using only the Python standard library."""
import hashlib
import io
import json
from pathlib import Path
import statistics
from zipfile import ZipFile


HERE = Path(__file__).resolve().parent
CARGO = "02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573"
STOCK = "a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de"
T4 = "f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642"
CLOSED = {
    "cli": "57cb4aa12680c00d3ff240af9e59fa3e7ebe3a68cf7ba92ab1c0307bd5395c22",
    "tests": "43d671850c44640ffd821e4fe4cf3fe100dbc8492b94357beb244ecba3f9ee9d",
}
REGISTRATIONS = {
    "cli": "691512c87b2af759272f16231135bfdc1ef04e25d901db68b876ace9882e0441",
    "tests": "69a8f9f44e608cff4aa2be7d13d525ec772f64faf3029bd36e6feb505a661ace",
}
QA_BOOK = {
    "stock": "4e3d9ac5a674c9eb3ba229c0f38cca33f020de616a37b7f8f1da337679d700e3",
    "t4": "76d62d4a2194dddae59fa4d818c8c182e5feadb0e019b354ac1277ca6192098d",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(archive, path):
    return json.loads(archive.read(path))


def cli_clients(archive):
    outcome = read(archive, "cli/terminal.json")
    assert outcome["registration"] == REGISTRATIONS["cli"] and outcome["heldout_pairs"] == 0
    assert not outcome["opens_confirmation"] and outcome["discovery_excluded"] == 6
    for role, checksum in QA_BOOK.items():
        root = f"cli/qa/{role}/"
        book_bytes = archive.read(root + "closed.json")
        assert sha(book_bytes) == checksum
        book = json.loads(book_bytes)
        for name in archive.namelist():
            if name.startswith(root) and name[len(root):] in book:
                assert sha(archive.read(name)) == book[name[len(root):]]
        receipt = read(archive, root + "success.json")
        assert receipt["registration"] == REGISTRATIONS["cli"] and receipt["performance_rows"] == 0
        assert read(archive, root + "return.json")["exit"] == 0
        for state in ("A", "B", "fresh-B"):
            base = root + state + "/"
            linked = read(archive, base + "client-link.json")["selected"]
            for name in ("serde", "uv_configuration"):
                pair = [Path(item["path"]) for item in linked[name]]
                assert [p.suffix for p in pair] == [".rlib", ".rmeta"]
                assert pair[0].with_suffix(".rmeta") == pair[1]
            assert archive.read(base + "old-trait.stdout") == b"Columns\n"
            assert read(archive, base + "introduced-traits-build.return.json")["exit"] == int(state == "A")
            if state == "A":
                messages = [json.loads(line) for line in archive.read(base + "introduced-traits-build.stderr").splitlines()]
                errors = [m for m in messages if m["level"] == "error" and m["message"] != "aborting due to 1 previous error"]
                assert len(errors) == 1 and errors[0]["code"]["code"] == "E0432"
            else:
                assert read(archive, base + "introduced-traits.return.json")["exit"] == 0
                assert archive.read(base + "introduced-traits.stdout") == b"Columns\nnew enum: preview and unknown select Preview\n"


def replay(archive, label):
    receipt = archive.read(label + "/closed.json")
    assert sha(receipt) == CLOSED[label]
    assert sha(archive.read(label + "/registration.json")) == REGISTRATIONS[label]
    prefix = label + "/trial/"
    for name, checksum in json.loads(receipt)["files"].items():
        assert sha(archive.read(prefix + name)) == checksum
    if label == "cli":
        other = "t4"
        args = ["check", "-p", "uv-cli"]
        gate = 2.0
        dirty = {"uv_audit", "uv_cli", "uv_client", "uv_configuration", "uv_settings", "uv_workspace"}
    else:
        other = "candidate"
        case = read(archive, "tests/case.json")
        args, gate = case["args"], 1.8
        dirty = set(case["required_dirty"])
    sequence = read(archive, prefix + "plan.json")["sequence"]
    assert len(sequence) == 6 and sum(order[0] == "stock" for order in sequence) == 3
    paired = [json.loads(line) for line in archive.read(prefix + "pairs.jsonl").splitlines()]
    assert len(paired) == 6
    edits = {}
    for role in ("stock", other):
        identity = read(archive, prefix + role + "/identity.json")
        assert identity["cargo"] == CARGO
        driver, = [entry["sha256"] for name, entry in identity["linked"].items() if "rustc_driver" in name]
        assert driver == (STOCK if role == "stock" else T4)
        values = []
        for i, pair in enumerate(paired, 1):
            with ZipFile(io.BytesIO(archive.read(prefix + f"{role}/pair-{i}.zip"))) as raw:
                before, edit, after = [read(raw, k + ".json") for k in ("settled-A", "edit-D", "noop-D")]
                assert edit["cmd"][1:] == [*args, "--locked", "--offline", "-j8"]
                assert not before["dirty_units"] and not after["dirty_units"]
                assert set(edit["dirty_units"]) == dirty
                assert edit["wall_s"] == pair[role]["edit"]
                values.append(edit["wall_s"])
        edits[role] = values
        if label == "tests":
            with ZipFile(io.BytesIO(archive.read(prefix + role + "/functional.zip"))) as native:
                for name in case["tests"]:
                    passed = read(native, name + ".verified.json")
                    assert passed["fresh_artifacts"] == 604 and passed["recompiled"] == 0
                    assert "1 passed; 0 failed" in passed["status"] and "79 filtered out" in passed["status"]
    ratios = [s / c for s, c in zip(edits["stock"], edits[other])]
    middle = statistics.median(ratios)
    assert middle < gate
    return {"stock_s": statistics.median(edits["stock"]), "T4_s": statistics.median(edits[other]),
            "paired_speedup": middle, "range": [min(ratios), max(ratios)],
            "faster": sum(r > 1 for r in ratios), "discovery_pairs": 6, "held_out_pairs": 0}


if __name__ == "__main__":
    if not __debug__:
        raise RuntimeError("Run without Python optimization, so all assertions execute.")
    with ZipFile(HERE / "records.zip") as archive:
        assert len(archive.namelist()) == len(set(archive.namelist()))
        cli_clients(archive)
        print(json.dumps({name: replay(archive, name) for name in CLOSED}, indent=2))
