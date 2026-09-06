"""Collect compact, auditable benchmark evidence; leave full profiles on the host."""

import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path("/tmp/ohm-target-size-prototypes")
OUTPUT = ROOT / "prefix-export"
OUTPUT.mkdir(exist_ok=False)
files = {}
for folder in ["prefix-correctness", "prefix-measurements"]:
    for path in sorted((ROOT / folder).rglob("*")):
        if not path.is_file():
            continue
        relative = str(path.relative_to(ROOT))
        # Cachegrind instruction totals and profile hashes are in result.json.
        # Keep large per-source-line profiles on the host, outside Git.
        if path.suffix == ".cachegrind":
            files[relative] = {
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "bytes": path.stat().st_size,
                "included": False,
            }
            continue
        if path.suffix not in {".json", ".log", ".stderr", ".stdout"}:
            continue
        raw = path.read_bytes()
        files[relative] = {
            "sha256": hashlib.sha256(raw).hexdigest(),
            "bytes": len(raw),
            "included": True,
        }
        target = OUTPUT / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
for relative in ["measurement-host.json", "logs/cloned-prefixes-status.json"]:
    path = ROOT / relative
    raw = path.read_bytes()
    files[relative] = {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "included": True,
    }
    target = OUTPUT / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(raw)
(OUTPUT / "files.json").write_text(json.dumps(files, indent=2) + "\n")
# A single JSON mapping transports only explicitly selected evidence paths.
payload = {
    str(path.relative_to(OUTPUT)): path.read_text()
    for path in sorted(OUTPUT.rglob("*"))
    if path.is_file()
}
packed = gzip.compress(json.dumps(payload).encode(), mtime=0)
(ROOT / "prefix-results.json.gz").write_bytes(packed)
print(json.dumps({"bytes": len(packed), "sha256": hashlib.sha256(packed).hexdigest()}))
