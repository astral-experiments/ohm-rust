"""Exercise real compiler prefix clones, fallback, mode changes and failed builds.

Run outside timed workloads. Config supplies an exact compiler and a fresh task
root on Btrfs or ext4. Query snapshots use hard links to finalized files so a
write through an old inode is detected by both hash and inode checks.
"""

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from btrfs_extent_inventory import ranges, union_size
from query_generations import generations

config = json.loads(Path(sys.argv[1]).read_text())
root = Path(config["root"])
root.mkdir(parents=True, exist_ok=False)
rustc = Path(config["rustc"])
source = root / "fixture.rs"
binary = root / "fixture"
original = Path(config["fixture_source"]).read_bytes()
source.write_bytes(original)
assert hashlib.sha256(rustc.read_bytes()).hexdigest() == config["rustc_sha256"]
cache = root / "incremental"
saved = root / "snapshots"
saved.mkdir()
receipt = {"configuration": config, "commands": [], "snapshots": []}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def cache_hashes():
    return {
        str(p.relative_to(cache)): digest(p) for p in cache.rglob("query-cache.bin")
    }


def verify_snapshots():
    for row in receipt["snapshots"]:
        assert digest(Path(row["path"])) == row["sha256"], row


def compile_revision(revision, enabled, expect=0):
    command = [
        str(rustc),
        str(source),
        "--crate-name",
        "retained_prefix_fixture",
        "--edition=2024",
        "--cfg",
        f"rpass{revision}",
        "-Ccodegen-units=1",
        "-Zthreads=4",
        "-Zquery-dep-graph",
        "-Zincremental-verify-ich",
        "-Zincremental-info",
        "-Cincremental=" + str(cache),
        "-o",
        str(binary),
        *config.get("link_flags", []),
    ]
    env = dict(
        os.environ,
        OHM_CLIPPY_FUNCTION_LINTS="0",
        RUSTC_EXPERIMENTAL_CLONED_QUERY_PREFIX=str(enabled),
    )
    env.pop("RUSTFLAGS", None)
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    result = subprocess.run(
        command, env=env, capture_output=True, text=True, check=False
    )
    name = f"rpass{revision}-mode{enabled}" + ("-failure" if expect else "")
    (root / (name + ".log")).write_text(result.stdout + result.stderr)
    receipt["commands"].append(
        {
            "name": name,
            "command": command,
            "mode": enabled,
            "returncode": result.returncode,
            "prefix_messages": [
                line for line in result.stderr.splitlines() if "query cache" in line
            ],
        }
    )
    assert (result.returncode == 0) if not expect else (result.returncode != 0), (
        result.stderr
    )
    verify_snapshots()
    if not expect:
        subprocess.run([binary], check=True)


for revision, enabled in enumerate([0, 1, 0, 1, 1, 1, 1], start=1):
    compile_revision(revision, enabled)
    current = max(cache.rglob("query-cache.bin"), key=lambda p: p.stat().st_mtime_ns)
    target = saved / f"rpass{revision}.bin"
    os.link(current, target)
    extents, flags = ranges(target)
    receipt["snapshots"].append(
        {
            "revision": revision,
            "mode": enabled,
            "path": str(target),
            "size": target.stat().st_size,
            "device": target.stat().st_dev,
            "inode": target.stat().st_ino,
            "sha256": digest(target),
            "generations": generations(target),
            "extents": extents,
            "flags": flags,
        }
    )
    if revision == 3:
        before = cache_hashes()
        source.write_bytes(original + b"\nthis is not valid Rust!\n")
        compile_revision(revision, 1, expect=1)
        assert cache_hashes() == before
        source.write_bytes(original)
    (root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
verify_snapshots()
assert len({row["inode"] for row in receipt["snapshots"]}) == 7
modes = {row["name"]: row for row in receipt["commands"]}
if config["expect_cloning"]:
    for revision in [2, 5, 6]:
        assert any(
            "preserved" in message
            for message in modes[f"rpass{revision}-mode1"]["prefix_messages"]
        )
else:
    assert any(
        "clone failure" in message
        for row in receipt["commands"]
        for message in row["prefix_messages"]
    )
# Extent sharing is measured separately from byte identity.
for previous, current in zip(receipt["snapshots"], receipt["snapshots"][1:]):
    if previous["flags"] & (2 | 4 | 8 | 0x200) or current["flags"] & (
        2 | 4 | 8 | 0x200
    ):
        continue
    left = [(e["physical"], e["physical"] + e["length"]) for e in previous["extents"]]
    right = [(e["physical"], e["physical"] + e["length"]) for e in current["extents"]]
    current["shared_data_bytes_with_previous"] = (
        union_size(left) + union_size(right) - union_size(left + right)
    )
receipt["completed"] = True
(root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps({k: v for k, v in receipt.items() if k != "snapshots"}, indent=2))
