"""Copy the two Cargo output roots as a single graph of independent files and hardlinks."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat

ROOTS = ("products", "shared")


def describe(base):
    files = {}
    for name in ROOTS:
        for path in (base / name).rglob("*"):
            info = path.lstat()
            if stat.S_ISREG(info.st_mode):
                key = (info.st_dev, info.st_ino)
                entry = files.setdefault(key, (info.st_nlink, []))
                assert entry[0] == info.st_nlink
                entry[1].append(path.relative_to(base).as_posix())
            elif stat.S_ISLNK(info.st_mode):
                assert info.st_nlink == 1, path
            else:
                assert stat.S_ISDIR(info.st_mode), path
    groups = []
    for nlink, paths in files.values():
        assert nlink == len(paths), ("hardlink outside tracked output roots", nlink, paths)
        if nlink > 1:
            groups.append(sorted(paths))
    groups.sort()
    return set(files), {
        "regular_paths": sum(len(paths) for _, paths in files.values()),
        "physical_files": len(files),
        "groups": groups,
        "groups_sha256": hashlib.sha256(json.dumps(groups).encode()).hexdigest(),
    }


def copy(base, destination):
    previous, before = describe(base)
    first_copy = {}

    def copy_file(original, new):
        info = os.stat(original, follow_symlinks=False)
        key = (info.st_dev, info.st_ino)
        if key in first_copy:
            os.link(first_copy[key], new)
        else:
            shutil.copy2(original, new, follow_symlinks=False)
            first_copy[key] = new
        return new

    for name in ROOTS:
        assert (base / name).is_dir() and not (destination / name).exists()
        shutil.copytree(base / name, destination / name, symlinks=True, copy_function=copy_file)
    result, after = describe(destination)
    assert before == after and previous.isdisjoint(result)
    return before
