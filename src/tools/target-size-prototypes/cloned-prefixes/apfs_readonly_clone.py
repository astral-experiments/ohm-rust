"""Check clonefile's read-only-output fallback mechanics, without a candidate compiler."""

import argparse
import ctypes
import errno
import hashlib
import json
import os
import platform
import plistlib
import stat
import subprocess
from pathlib import Path

assert platform.system() == "Darwin"
assert os.geteuid() != 0, "Permission failure requires an unprivileged process"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--root",
    type=Path,
    default=Path(
        "/private/tmp/codex-cargo-targets/ohm-rust/isolated/target-size-prototypes/apfs-readonly-clone-mechanism"
    ),
)
root = parser.parse_args().root
root.mkdir(parents=True, exist_ok=True)
assert not any(root.iterdir()), "Refusing to replace existing mechanism evidence"
assert root.stat().st_dev == Path("/System/Volumes/Data").stat().st_dev
disk = plistlib.loads(
    subprocess.check_output(["diskutil", "info", "-plist", "/System/Volumes/Data"])
)
assert disk["FilesystemType"].lower() == "apfs", disk
source, retained, temporary = (
    root / name for name in ["source.rmeta", "retained.rmeta", "temporary.rmeta"]
)
payload = b"unchanged metadata prefix\n" * 8192
source.write_bytes(payload)
source.chmod(0o444)
os.link(source, retained)
original = source.stat()
libc = ctypes.CDLL(None, use_errno=True)
libc.clonefile.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint32]
libc.clonefile.restype = ctypes.c_int
assert libc.clonefile(os.fsencode(source), os.fsencode(temporary), 0x2) == 0, (
    ctypes.get_errno()
)
cloned = temporary.stat()
assert cloned.st_ino != original.st_ino
assert stat.S_IMODE(cloned.st_mode) == 0o444
try:
    descriptor = os.open(temporary, os.O_WRONLY)
except PermissionError as failure:
    append_errno = failure.errno
    assert append_errno in (errno.EACCES, errno.EPERM)
else:
    os.close(descriptor)
    raise AssertionError(
        "Expected opening the cloned read-only inode for writing to fail"
    )
temporary.unlink()
replacement = payload[:65536] + b"fresh metadata suffix\n"
with temporary.open("wb") as file:
    file.write(replacement)
    file.flush()
    os.fsync(file.fileno())
assert temporary.read_bytes() == replacement
assert temporary.stat().st_ino != original.st_ino
assert temporary.stat().st_mode & stat.S_IWUSR
assert source.read_bytes() == payload and retained.read_bytes() == payload
assert source.stat().st_ino == retained.stat().st_ino == original.st_ino
assert (
    stat.S_IMODE(source.stat().st_mode)
    == stat.S_IMODE(retained.stat().st_mode)
    == 0o444
)
receipt = {
    "scope": "APFS clone/open/unlink/recreate mechanism; not candidate macOS compiler coverage",
    "reviewed_code_commit": "0ee8042ac2422aa67047004ce3758dd4a5410df9",
    "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "platform": platform.platform(),
    "filesystem_type": disk["FilesystemType"],
    "root": str(root),
    "clone_flags": 0x2,
    "source_inode": original.st_ino,
    "clone_inode": cloned.st_ino,
    "source_mode": oct(stat.S_IMODE(original.st_mode)),
    "clone_mode": oct(stat.S_IMODE(cloned.st_mode)),
    "clone_write_open_errno": append_errno,
    "recreated_inode": temporary.stat().st_ino,
    "recreated_mode": oct(stat.S_IMODE(temporary.stat().st_mode)),
    "source_sha256": hashlib.sha256(payload).hexdigest(),
    "recreated_sha256": hashlib.sha256(replacement).hexdigest(),
    "source_and_retained_hardlink_unchanged": True,
    "completed": True,
}
(root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
