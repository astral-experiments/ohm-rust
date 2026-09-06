"""Conservative per-tree physical data accounting on one uncompressed Btrfs FS.

Data extent unions exclude filesystem metadata. Inline or uninterpretable extents
are reported separately and never treated as shared physical address zero.
"""

import fcntl
import json
import os
import struct
from collections import defaultdict
from pathlib import Path

FS_IOC_FIEMAP = 0xC020660B
UNINTERPRETABLE = 2 | 4 | 8 | 0x200


def ranges(path):
    result = []
    start = 0
    flags_seen = 0
    with path.open("rb") as stream:
        while True:
            capacity = 256
            buf = bytearray(32 + 56 * capacity)
            struct.pack_into(
                "=QQIIII", buf, 0, start, (1 << 64) - 1 - start, 1, 0, capacity, 0
            )
            fcntl.ioctl(stream, FS_IOC_FIEMAP, buf, True)
            count = struct.unpack_from("=QQIIII", buf)[3]
            if not count:
                break
            for i in range(count):
                logical, physical, length, _, _, flags, *_ = struct.unpack_from(
                    "=QQQQQIIII", buf, 32 + 56 * i
                )
                flags_seen |= flags
                result.append(
                    {
                        "logical": logical,
                        "physical": physical,
                        "length": length,
                        "flags": flags,
                    }
                )
            last = result[-1]
            if last["flags"] & 1:
                break
            following = last["logical"] + last["length"]
            if following <= start:
                raise RuntimeError(f"FIEMAP made no progress: {path}")
            start = following
    return result, flags_seen


def union_size(values):
    result = 0
    end = 0
    for lo, hi in sorted(values):
        if hi > end:
            result += hi - max(lo, end)
            end = hi
    return result


def inventory(root, evidence):
    root = Path(root)
    evidence = Path(evidence)
    seen = set()
    devices = set()
    rows = []
    known = []
    grouped = defaultdict(list)
    totals = defaultdict(lambda: defaultdict(int))
    for directory, subdirs, filenames in os.walk(root):
        for name in filenames:
            path = Path(directory) / name
            if path.is_symlink() or not path.is_file():
                continue
            info = path.stat()
            identity = (info.st_dev, info.st_ino)
            if identity in seen:
                continue
            seen.add(identity)
            devices.add(info.st_dev)
            kind = (
                "query-cache"
                if name == "query-cache.bin"
                else "metadata"
                if name.endswith(".rmeta")
                else "incremental-object"
                if name.endswith(".o")
                and any(
                    part in {"incremental", "measured-incremental"}
                    for part in path.parts
                )
                else "other-incremental"
                if any(
                    part in {"incremental", "measured-incremental"}
                    for part in path.parts
                )
                else "other"
            )
            summary = totals[kind]
            summary["files"] += 1
            summary["logical_bytes"] += info.st_size
            summary["inode_allocated_bytes"] += info.st_blocks * 512
            extents, flags = ranges(path)
            row = {
                "path": str(path.relative_to(root)),
                "device": info.st_dev,
                "inode": info.st_ino,
                "logical": info.st_size,
                "allocated": info.st_blocks * 512,
                "kind": kind,
                "extents": extents,
            }
            if flags & UNINTERPRETABLE:
                summary["uninterpretable_files"] += 1
                summary["uninterpretable_logical_bytes"] += info.st_size
                summary["uninterpretable_inode_allocated_bytes"] += info.st_blocks * 512
                row["physical_union_excluded"] = True
            else:
                physical = [
                    (e["physical"], e["physical"] + e["length"]) for e in extents
                ]
                if any(lo == 0 for lo, _ in physical):
                    raise RuntimeError(f"Unknown physical zero address: {path}")
                known.extend(physical)
                grouped[kind].extend(physical)
            rows.append(row)
    if len(devices) != 1:
        raise RuntimeError(f"Expected one filesystem: {devices}")
    for kind, values in grouped.items():
        totals[kind]["known_physical_data_union_bytes"] = union_size(values)
    result = {
        "scope": str(root),
        "filesystem": "task-owned Btrfs loop image, compression disabled",
        "method": (
            "FIEMAP with SYNC, unique-inode scan and union of physical ranges; "
            "filesystem metadata excluded"
        ),
        "known_physical_data_union_bytes": union_size(known),
        "categories": {k: dict(v) for k, v in totals.items()},
        "files": rows,
    }
    evidence.write_text(json.dumps(result, indent=2) + "\n")
    return {k: v for k, v in result.items() if k != "files"}
