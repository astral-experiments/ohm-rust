"""Build the revised rustc with the same bootstrap configuration as the retained -21 control."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

base = "c79998a5cd143c445a4b2528f20953b176a65c5d"
candidate = "10299ce8f68342985a494b4876f9774a6929128f"
cargo = {"control": "2d44bd4aa68b9ba32cacb5a384ecc18fa5e7553c", "candidate": "3ef3af05460701ced153c6601bc934a10ea0afdd"}
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

for lane, revision in (("candidate", candidate),):
    subprocess.run(["git", "checkout", "--detach", revision], check=True)
    assert output("git", "rev-parse", "HEAD") == revision
    assert output("git", "rev-parse", "HEAD:src/tools/cargo") == cargo[lane]
    record(lane+"-backtrace", ["git","submodule","update","--init","--depth","1","library/backtrace"])
    cmd = [sys.executable, "x.py", "--ci", "false", "--set", "build.submodules=false", "--set", "rust.download-rustc=false", "--set", "rust.omit-git-hash=false", "--set", "rust.debug-assertions=false", "--set", "rust.lto=off", "build", "--build-dir", str(build), "--stage", "1"]
    record(lane+"-host", cmd+["compiler/rustc","library"])
    sdk = products / lane
    def sources(path, names):
        return {"src", "rustc-src"} & set(names) if Path(path).parts[-2:] == ("lib", "rustlib") else set()
    shutil.copytree(build/host/"stage1", sdk, symlinks=False, ignore=sources)
    if linux:
        # Cross bootstrap replaces its host sysroot after the host SDK has been saved.
        record(lane+"-cross", cmd+["--target",cross,"library"])
        cross_tree=Path("lib/rustlib")/cross
        shutil.copytree(build/host/"stage1"/cross_tree,sdk/cross_tree,symlinks=False,dirs_exist_ok=True)
    version = output(sdk/"bin/rustc", "-vV")
    assert "commit-hash: " + revision in version
    for target in ([host,cross] if linux else [host]):
        target_lib=sdk/"lib/rustlib"/target/"lib"
        assert list(target_lib.glob("libstd-*.rlib")), target_lib
    inventory = {str(p.relative_to(sdk)):sha(p) for p in sdk.rglob("*") if p.is_file()}
    (ev/(lane+".json")).write_text(json.dumps({"version":version,"commit":revision,"cargo":cargo[lane],"tree":output("git","rev-parse","HEAD^{tree}"),"files":inventory}, indent=2)+"\n")
record("package", ["tar","-C",str(products),"-czf",str(root/"sdk.tar.gz"),"candidate"])
packed = root/"sdk.tar.gz"
(ev/"archive.json").write_text(json.dumps({"sha256":sha(packed),"bytes":packed.stat().st_size,"host":host})+"\n")
