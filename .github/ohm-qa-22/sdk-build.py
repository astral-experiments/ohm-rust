"""Build the signed -21 source and the two-patch -22 candidate with one bootstrap configuration."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

base = "c79998a5cd143c445a4b2528f20953b176a65c5d"
candidate = "50b3d8afcf6bbb1ecb919159008b0e0ef7e1d2c0"
cargo = {"control": "2d44bd4aa68b9ba32cacb5a384ecc18fa5e7553c", "candidate": "d057942fabfff071335c516f2e2dbb74913f0b36"}
linux = platform.system() == "Linux"
host = "x86_64-unknown-linux-gnu" if linux else "aarch64-apple-darwin"
cross = "aarch64-unknown-linux-gnu"
root = Path(os.environ["RUNNER_TEMP"]) / "ohm22-sdk"
root.mkdir()
ev = root / "evidence"
ev.mkdir()
build = root / "build"
products = root / "products"
products.mkdir()
seen = {}

def output(*args):
    return subprocess.check_output(args, text=True).strip()

def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(2**20), b""):
            h.update(chunk)
    return h.hexdigest()

def record(name, args):
    print(name, flush=True)
    with (ev / (name + ".log")).open("wb") as f:
        sub = subprocess.run(args, stdout=f, stderr=subprocess.STDOUT)
    if sub.returncode:
        with open(f.name, "rb") as out:
            out.seek(max(0, os.fstat(out.fileno()).st_size-7000))
            print(out.read().decode(errors="replace"), flush=True)
    sub.check_returncode()

for lane, revision in (("control", base), ("candidate", candidate)):
    subprocess.run(["git", "checkout", "--detach", revision], check=True)
    assert output("git", "rev-parse", "HEAD") == revision
    assert output("git", "rev-parse", "HEAD:src/tools/cargo") == cargo[lane]
    record(lane+"-backtrace", ["git","submodule","update","--init","--depth","1","library/backtrace"])
    cmd = [sys.executable, "x.py", "--ci", "false", "--set", "build.submodules=false", "--set", "rust.download-rustc=false", "--set", "rust.omit-git-hash=false", "--set", "rust.debug-assertions=false", "--set", "rust.lto=off", "build", "--build-dir", str(build), "--stage", "1"]
    record(lane+"-host", cmd+["compiler/rustc","library"])
    if linux:
        record(lane+"-cross", cmd+["--target",cross,"library"])
    sdk = products / lane
    def admit(source, destination):
        path = Path(source)
        identity = (path.stat().st_mode & 0o777, path.stat().st_size, sha(path))
        if identity in seen:
            os.link(seen[identity], destination)
        else:
            shutil.copy2(source, destination)
            seen[identity] = destination
        return destination
    shutil.copytree(build/host/"stage1", sdk, symlinks=False, copy_function=admit)
    version = output(sdk/"bin/rustc", "-vV")
    assert "commit-hash: " + revision in version
    for target in ([host,cross] if linux else [host]):
        target_lib=sdk/"lib/rustlib"/target/"lib"
        assert list(target_lib.glob("libstd-*.rlib")), target_lib
    inventory = {str(p.relative_to(sdk)):sha(p) for p in sdk.rglob("*") if p.is_file()}
    (ev/(lane+".json")).write_text(json.dumps({"version":version,"commit":revision,"cargo":cargo[lane],"tree":output("git","rev-parse","HEAD^{tree}"),"files":inventory}, indent=2)+"\n")
record("package", ["tar","-C",str(products),"-czf",str(root/"sdk.tar.gz"),"control","candidate"])
packed = root/"sdk.tar.gz"
(ev/"archive.json").write_text(json.dumps({"sha256":sha(packed),"bytes":packed.stat().st_size,"host":host})+"\n")
