"""Compile and run metadata clients while retaining all prior output inodes."""

import hashlib
import json
import os
import stat
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from btrfs_extent_inventory import ranges, union_size

config = json.loads(Path(sys.argv[1]).read_text())
root = Path(config["root"])
root.mkdir(parents=True, exist_ok=False)
rustc = Path(config["rustc"])
assert hashlib.sha256(rustc.read_bytes()).hexdigest() == config["rustc_sha256"]
fixture = Path(config["fixture_source"])
source = (fixture / "library.rs").read_text()
(root / "main.rs").write_bytes((fixture / "main.rs").read_bytes())
snapshots = []
receipt = {"configuration": config, "revisions": []}
readonly_sources = []
for revision, mode in enumerate([0, 1, 1, 0, 1, 1]):
    if revision == 5:
        # Linux FICLONE creates a writable destination even from a read-only
        # source. This checks source immutability and successful reuse/fallback;
        # macOS clonefile permission-preservation is tested separately.
        retained = root / "readonly-inputs"
        retained.mkdir()
        seen = set()
        for original in sorted((root / "incremental").rglob("*.rmeta")):
            info = original.stat()
            if info.st_ino in seen:
                continue
            seen.add(info.st_ino)
            saved = retained / f"{len(readonly_sources)}.rmeta"
            os.link(original, saved)
            original.chmod(stat.S_IMODE(info.st_mode) & ~0o222)
            readonly_sources.append(
                {
                    "original": str(original),
                    "retained": str(saved),
                    "inode": info.st_ino,
                    "mode": stat.S_IMODE(saved.stat().st_mode),
                    "sha256": hashlib.sha256(saved.read_bytes()).hexdigest(),
                }
            )
        assert readonly_sources, "Expected previous metadata work products"
        receipt["readonly_sources"] = readonly_sources
        # The published output can hard-link the same inode as a work product.
        # Remove only that name so rustc's output-permission preflight does not
        # reject it before reaching the read-only input reuse being tested.
        published = root / "liblibrary.rmeta"
        receipt["readonly_source_setup"] = {
            "removed_published_output": str(published),
            "published_inode": published.stat().st_ino,
            "aliased_readonly_input": published.stat().st_ino in seen,
        }
        published.unlink()
        (root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    value = 10 + revision
    (root / "library.rs").write_text(source.replace("    10", f"    {value}"))
    command = [
        str(rustc),
        "library.rs",
        "-Cincremental=incremental",
        "--emit=link,metadata",
        "-Zpublic-api-hash",
        "-Zincremental-verify-ich",
        "-Zincremental-info",
    ]
    env = dict(os.environ, RUSTC_EXPERIMENTAL_CLONED_METADATA_PREFIX=str(mode))
    process = subprocess.run(
        command, cwd=root, env=env, capture_output=True, text=True, check=False
    )
    (root / f"r{revision}.stderr").write_text(process.stderr)
    assert process.returncode == 0, process.stderr
    subprocess.run(
        [str(rustc), "main.rs", "--extern", "library=liblibrary.rlib"],
        cwd=root,
        env=env,
        check=True,
        capture_output=True,
    )
    output = subprocess.check_output([str(root / "main")], text=True)
    assert output == f"{value}\n", output
    for saved in readonly_sources:
        path = Path(saved["retained"])
        assert path.stat().st_ino == saved["inode"]
        assert stat.S_IMODE(path.stat().st_mode) == saved["mode"]
        assert hashlib.sha256(path.read_bytes()).hexdigest() == saved["sha256"]
    for saved, expected in snapshots:
        assert saved.read_bytes() == expected, saved
    snapshot = root / f"retained-{revision}.rmeta"
    os.link(root / "liblibrary.rmeta", snapshot)
    snapshots.append((snapshot, snapshot.read_bytes()))
    extents, flags = ranges(snapshot)
    messages = [
        line for line in process.stderr.splitlines() if "metadata preserved" in line
    ]
    if mode and config["expect_cloning"]:
        assert messages, process.stderr
    if not config["expect_cloning"]:
        assert not messages, process.stderr
    receipt["revisions"].append(
        {
            "revision": revision,
            "mode": mode,
            "readonly_source_case": revision == 5,
            "command": command,
            "output": output,
            "size": snapshot.stat().st_size,
            "inode": snapshot.stat().st_ino,
            "sha256": hashlib.sha256(snapshot.read_bytes()).hexdigest(),
            "extents": extents,
            "flags": flags,
            "messages": messages,
        }
    )
    (root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
assert len({item["inode"] for item in receipt["revisions"]}) == 6
for previous, current in zip(receipt["revisions"], receipt["revisions"][1:]):
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
print(json.dumps(receipt, indent=2))
