"""Measure retained donor plus child storage on a dedicated Btrfs filesystem.

Uses the same candidate compiler with either opt-in flag disabled or enabled.
The donor is a reflink snapshot of the complete target after revision one. It
remains pinned while the child runs six subsequent builds. This models retained
build state; it does not measure Cargo's worktree admission or seeding policy.
"""

import argparse
import fcntl
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path[:0] = [
    str(Path(__file__).resolve().parent),
    str(Path(__file__).resolve().parent.parent),
]
import validate_rust as common
from btrfs_extent_inventory import inventory

ROOT = common.ROOT
DATA = ROOT / "prefix-measurements"
COW = common.BUILDS / "cloned-prefixes-cow"
WORK = COW / "work"
MODES = {"off": (0, 0), "query": (1, 0), "metadata": (0, 1), "both": (1, 1)}
REVISIONS = [False, False, True, True, False, False, True]


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def provision():
    image = ROOT / "cloned-prefixes-32.btrfs"
    COW.mkdir(parents=True, exist_ok=True)
    assert not image.exists() and not any(COW.iterdir())
    assert (
        subprocess.run(
            ["findmnt", "-M", str(COW)], capture_output=True, check=False
        ).returncode
        != 0
    )
    assert shutil.disk_usage(ROOT).free > 40 * 1024**3
    with image.open("xb") as stream:
        stream.truncate(32 * 1024**3)
    subprocess.run(
        ["mkfs.btrfs", "--label", "ohm-prefix-screen", str(image)], check=True
    )
    subprocess.run(
        ["sudo", "-n", "mount", "-o", "loop,compress=no", str(image), str(COW)],
        check=True,
    )
    subprocess.run(
        ["sudo", "-n", "chown", f"{os.getuid()}:{os.getgid()}", str(COW)], check=True
    )
    write(
        DATA / "filesystem.json",
        json.loads(subprocess.check_output(["findmnt", "-T", str(COW), "-J"])),
    )


def hashes(path):
    return {
        str(p.relative_to(path)): common.sha(p)
        for p in path.rglob("*")
        if p.is_file() and not p.is_symlink()
    }


def restore(fixture, patched):
    cwd = common.restore_source(fixture, False)
    if patched:
        if fixture == "syn-2.0.101":
            source = cwd / "src/ident.rs"
            old, new = '"while" | "yield" => false,', '"while" | "yield" => true ,'
        else:
            source = cwd / "src/util/alphabet.rs"
            old, new = "self.bits.0 == [0, 0]", "self.bits.0 == [0, 1]"
        text = source.read_text()
        assert len(old) == len(new) and text.count(old) == 1
        source.write_text(text.replace(old, new))
    return cwd


def setup(fixture, profile, mode):
    manifest, command, env = common.invocation("cloned-prefixes", fixture, profile)
    if WORK.exists():
        # This directory is owned exclusively by this driver. Evidence is on
        # the host filesystem, outside it, and survives every workload reset.
        shutil.rmtree(WORK)
    WORK.mkdir()
    original = Path(manifest["target"])
    target = WORK / "target"
    shutil.copytree(original, target)
    cache = target / Path(manifest["incremental"]).relative_to(original)
    if cache.exists():
        shutil.rmtree(cache)
    cache.mkdir()
    command = [value.replace(str(original), str(target)) for value in command]
    env = {key: value.replace(str(original), str(target)) for key, value in env.items()}
    env.update(
        RUSTC_EXPERIMENTAL_CLONED_QUERY_PREFIX=str(MODES[mode][0]),
        RUSTC_EXPERIMENTAL_CLONED_METADATA_PREFIX=str(MODES[mode][1]),
    )
    return manifest, command, env, target


def cycle(fixture, profile, mode, label, collect_storage=False, instrument=None):
    evidence = DATA / (fixture + "-" + profile) / label
    evidence.mkdir(parents=True, exist_ok=False)
    manifest, command, env, target = setup(fixture, profile, mode)
    donor = WORK / "donor"
    result = {
        "compiler": manifest["compiler"],
        "inputs": manifest["inputs"],
        "capture_manifest": manifest,
        "harness_sha256": common.sha(Path(__file__)),
        "inventory_sha256": common.sha(
            Path(__file__).with_name("btrfs_extent_inventory.py")
        ),
        "mode": mode,
        "fixture": fixture,
        "profile": profile,
        "command": command,
        "environment": {
            key: value
            for key, value in env.items()
            if key in manifest["env"]
            or key.startswith("RUSTC_EXPERIMENTAL_")
            or key in {"LD_LIBRARY_PATH", "RUSTC_BOOTSTRAP", "CARGO_HOME"}
        },
        "revisions": [],
        "scope": "candidate off/on; full target plus retained donor; same-length private body edits",
    }
    donor_hashes = None
    for revision, patched in enumerate(REVISIONS, 1):
        cwd = restore(fixture, patched)
        actual_command = command.copy()
        if collect_storage:
            actual_command += ["-Zincremental-info"]
        measured = instrument == revision
        if measured:
            actual_command = [
                "valgrind",
                "--tool=cachegrind",
                "--cache-sim=no",
                "--branch-sim=no",
                "--cachegrind-out-file=" + str(evidence / f"r{revision}.cachegrind"),
                *actual_command,
            ]
        started = time.perf_counter_ns()
        process = subprocess.run(
            actual_command, cwd=cwd, env=env, capture_output=True, check=False
        )
        elapsed = time.perf_counter_ns() - started
        (evidence / f"r{revision}.stdout").write_bytes(process.stdout)
        (evidence / f"r{revision}.stderr").write_bytes(process.stderr)
        assert process.returncode == 0, process.stderr.decode(errors="replace")[-8000:]
        if revision == 1:
            subprocess.run(
                ["cp", "-a", "--reflink=always", str(target), str(donor)], check=True
            )
            donor_hashes = hashes(donor)
        assert hashes(donor) == donor_hashes, "retained donor was modified"
        receipt = {
            "revision": revision,
            "patched": patched,
            "elapsed_ns": elapsed,
            "source_digest": hashes(cwd),
            "returncode": process.returncode,
            "stdout_sha256": common.sha(evidence / f"r{revision}.stdout"),
            "stderr_sha256": common.sha(evidence / f"r{revision}.stderr"),
        }
        if measured:
            raw = (evidence / f"r{revision}.cachegrind").read_text()
            events = next(
                line.split()[1:]
                for line in raw.splitlines()
                if line.startswith("events:")
            )
            counters = next(
                list(map(int, line.split()[1:]))
                for line in raw.splitlines()
                if line.startswith("summary:")
            )
            receipt["instructions"] = counters[events.index("Ir")]
            receipt["cachegrind_sha256"] = common.sha(
                evidence / f"r{revision}.cachegrind"
            )
        if collect_storage:
            subprocess.run(["sync", "-f", str(WORK)], check=True)
            receipt["storage"] = inventory(WORK, evidence / f"r{revision}-extents.json")
            receipt["messages"] = [
                line
                for line in process.stderr.decode(errors="replace").splitlines()
                if "query cache" in line
                or "metadata preserved" in line
                or "metadata prefix" in line
            ]
        result["revisions"].append(receipt)
        write(evidence / "result.json", result)
        if measured:
            break
    result["donor_unchanged"] = True
    write(evidence / "result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "action", choices=["provision", "storage", "native", "instructions"]
    )
    parser.add_argument(
        "--fixture", choices=list(common.FIXTURES), default="syn-2.0.101"
    )
    parser.add_argument("--profile", choices=["check", "dev"], default="dev")
    args = parser.parse_args()
    if args.action == "provision":
        return provision()
    mount = json.loads(subprocess.check_output(["findmnt", "-T", str(COW), "-J"]))
    assert mount["filesystems"][0]["fstype"] == "btrfs", mount
    if args.action == "storage":
        for mode in MODES:
            cycle(
                args.fixture,
                args.profile,
                mode,
                "storage-" + mode,
                collect_storage=True,
            )
    elif args.action == "native":
        for mode in ["off", "both"]:
            cycle(args.fixture, args.profile, mode, "warmup-" + mode)
        rows = []
        for round, order in enumerate(
            [["off", "both", "both", "off"], ["both", "off", "off", "both"]]
        ):
            for slot, mode in enumerate(order):
                rows.append(
                    cycle(
                        args.fixture,
                        args.profile,
                        mode,
                        f"native-{round}-{slot}-{mode}",
                    )
                )
        summary = {}
        for revision in range(7):
            values = {
                mode: [
                    r["revisions"][revision]["elapsed_ns"]
                    for r in rows
                    if r["mode"] == mode
                ]
                for mode in ["off", "both"]
            }
            summary[str(revision + 1)] = {
                mode: statistics.median(v) for mode, v in values.items()
            }
            summary[str(revision + 1)]["ratio"] = statistics.median(
                values["both"]
            ) / statistics.median(values["off"])
        write(
            DATA / f"{args.fixture}-{args.profile}-native.json",
            {"raw": rows, "summary": summary},
        )
    else:
        rows = []
        for revision in [2, 3, 4]:
            for round, order in enumerate([["off", "both"], ["both", "off"]]):
                for mode in order:
                    rows.append(
                        cycle(
                            args.fixture,
                            args.profile,
                            mode,
                            f"instructions-r{revision}-{round}-{mode}",
                            instrument=revision,
                        )
                    )
        write(DATA / f"{args.fixture}-{args.profile}-instructions.json", rows)


if __name__ == "__main__":
    with (ROOT / "common-validation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        main()
