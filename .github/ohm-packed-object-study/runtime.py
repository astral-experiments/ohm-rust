"""Collect only the Linux rustc, LLVM, linker, and standard-library runtime."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HOST = "x86_64-unknown-linux-gnu"
ROOT = Path(os.environ["RUNNER_TEMP"]) / "ohm-branch-object-qa"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inventory(stage):
    leaf = stage / "lib/rustlib" / HOST
    paths = [stage / "bin/rustc", *(stage / "lib").glob("*.so*")]
    paths.extend(p for p in (leaf / "lib").iterdir() if p.suffix in {".rlib", ".rmeta", ".so", ".o"})
    if (leaf / "bin/rust-lld").is_file():
        paths.append(leaf / "bin/rust-lld")
    gcc = leaf / "bin/gcc-ld"
    if gcc.is_dir():
        paths.extend(p for p in gcc.iterdir() if p.is_file())
    names = [str(p.relative_to(stage)) for p in paths]
    assert len(names) == len(set(names)) and len(names) < 400
    assert any(n.startswith("lib/librustc_driver-") for n in names)
    assert any(n.startswith("lib/libLLVM") for n in names)
    assert any("/lib/libstd-" in n and n.endswith(".rlib") for n in names)
    assert any("/lib/libcore-" in n and n.endswith(".rlib") for n in names)
    assert all(p.is_file() for p in paths)
    return sorted(paths)


def freeze(stage, role, commit):
    dest = ROOT / "capsule"
    objects = dest / "objects"
    objects.mkdir(parents=True, exist_ok=True)
    rows = {}
    for p in inventory(stage):
        key = sha(p)
        obj = objects / key
        if not obj.exists():
            with p.open("rb") as inp, obj.open("xb") as out:
                shutil.copyfileobj(inp, out, 1024 * 1024)
        assert obj.is_file() and sha(obj) == key
        rows[str(p.relative_to(stage))] = [key, p.stat().st_size, p.stat().st_mode & 0o777]
    assert sum(row[1] for row in rows.values()) < 2800 * 2**20
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() == commit
    tree = subprocess.check_output(["git", "rev-parse", "HEAD:compiler"], text=True).strip()
    bootstrap = stage.parent / "stage0/bin"
    with (dest / (role + ".json")).open("x") as out:
        json.dump({"role": role, "commit": commit, "compiler_tree": tree, "files": rows,
                   "stage0_rustc": sha(bootstrap / "rustc"), "stage0_cargo": sha(bootstrap / "cargo"),
                   "stage0_source": sha(Path("src/stage0"))}, out, indent=2, sort_keys=True)


def reopen(role):
    dest = ROOT / "capsule"
    out = ROOT / ("smoke-" + role)
    out.mkdir()
    record = json.loads((dest / (role + ".json")).read_bytes())
    for name, (key, size, mode) in record["files"].items():
        obj = dest / "objects" / key
        assert obj.is_file() and obj.stat().st_size == size and sha(obj) == key
        path = out / name
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(obj, path)
        path.chmod(mode)
    compiler = out / "bin/rustc"
    sysroot = subprocess.check_output([compiler, "--print", "sysroot"], text=True).strip()
    assert Path(sysroot).samefile(out)
    source = out / "probe.rs"
    source.write_text('fn main() { println!("29"); }\n')
    program = out / "run-probe"
    subprocess.run([compiler, source, "-o", program], check=True)
    assert subprocess.check_output([program]) == b"29\n"
    source.write_text("fn main() { OHM_INVALID_SOURCE(); }\n")
    invalid = subprocess.run([compiler, source, "-o", out / "must-not-exist"], capture_output=True)
    assert invalid.returncode and b"E0425" in invalid.stderr and not (out / "must-not-exist").exists()
    return record


if __name__ == "__main__":
    if sys.argv[1] == "freeze":
        freeze(Path(sys.argv[2]).resolve(), sys.argv[3], sys.argv[4])
    else:
        p, c = reopen("P"), reopen("C")
        assert all(p[k] == c[k] for k in ("stage0_rustc", "stage0_cargo", "stage0_source"))
        assert p["compiler_tree"] != c["compiler_tree"]
        print(json.dumps({k: {role: r[k] for role, r in (("P", p), ("C", c))}
                          for k in ("commit", "compiler_tree", "stage0_rustc")}, indent=2))
