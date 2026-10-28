"""Read the native results of the same derive and attribute in independent rustc processes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile

arm, sdk, packet = sys.argv[1:]
sdk, packet = Path(sdk).resolve(), Path(packet).resolve() / arm
packet.mkdir()
stem = Path(__file__).resolve().parent
case = Path(tempfile.mkdtemp(prefix="native-macro-", dir=os.environ["RUNNER_TEMP"]))
for filename in ("native.rs", "reader.rs"):
    shutil.copyfile(stem / filename, case / filename)
rows = []
inputs = {}


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def mach(path):
    """Get real Mach-O section payload digests, excluding zero-fill sections."""
    data = path.read_bytes()
    assert struct.unpack_from("<I", data)[0] == 0xFEEDFACF
    ncmd = struct.unpack_from("<I", data, 16)[0]
    off = 32
    sections = {}
    for _ in range(ncmd):
        cmd, size = struct.unpack_from("<II", data, off)
        if cmd == 0x19:
            nsects = struct.unpack_from("<I", data, off + 64)[0]
            for i in range(nsects):
                p = off + 72 + i * 80
                name, seg = (s.rstrip(b"\0").decode("ascii") for s in struct.unpack_from("16s16s", data, p))
                count, start = struct.unpack_from("<QI", data, p + 40)
                if start and count:
                    assert start + count <= len(data)
                    sections[f"{seg},{name}"] = hashlib.sha256(data[start:start + count]).hexdigest()
        assert size >= 8
        off += size
    assert "__TEXT,__text" in sections
    return {"sha256": digest(path), "sections": sections}


env = dict(os.environ, RUSTC_BOOTSTRAP="1")
env.pop("RUST_BACKTRACE", None)
# The rustup shim's exact target is independently verified before any compile.
rustup = shutil.which("rustup")
if rustup is not None:
    home = case / "rustup"
    (home / "toolchains").mkdir(parents=True)
    (home / "toolchains" / "ohm").symlink_to(sdk)
    env["RUSTUP_HOME"] = str(home)
    env["RUSTUP_AUTO_INSTALL"] = "0"
    compiler = [rustup, "run", "ohm", "rustc"]
else:
    compiler = [str(sdk / "bin" / "rustc")]


def run(label, argv):
    argv = [str(a) for a in argv]
    proc = subprocess.run(argv, cwd=case, env=env, capture_output=True)
    (packet / (label + ".stdout")).write_bytes(proc.stdout)
    (packet / (label + ".stderr")).write_bytes(proc.stderr)
    row = {"label": label, "argv": argv, "exit": proc.returncode,
           "stdout_sha256": hashlib.sha256(proc.stdout).hexdigest(),
           "stderr_sha256": hashlib.sha256(proc.stderr).hexdigest()}
    rows.append(row)
    (packet / "commands.json").write_text(json.dumps(rows, indent=2) + "\n")
    print(arm, label, proc.returncode, proc.stdout[:280].decode(errors="replace"),
          proc.stderr[:500].decode(errors="replace"), flush=True)
    proc.check_returncode()
    return proc.stdout.decode().strip()


assert run("version", compiler + ["-vV"]).find("host: aarch64-apple-darwin") >= 0
assert Path(run("sysroot", compiler + ["--print", "sysroot"])).resolve() == sdk
drivers = sorted((sdk / "lib").glob("librustc_driver*.dylib"))
assert len(drivers) == 1
for path in [sdk / "bin/rustc", *drivers, case / "native.rs", case / "reader.rs"]:
    inputs[str(path.name)] = digest(path)
variants = {}
values = {}
warm = case / "incremental"
for phase, wanted, incremental, change in [
    ("original", 17, warm, True),
    ("edited", 29, warm, True),
    ("repeat", 29, warm, False),
    ("edited-empty", 29, case / "empty", False),
    ("restored", 17, warm, True),
    ("restored-empty", 17, case / "empty-restored", False),
]:
    if change:
        (case / "value.c").write_text(f"int value(void) {{ return {wanted}; }}\n")
        run(phase + "-cc", ["cc", "-fPIC", "-c", "value.c", "-o", "value.o"])
        run(phase + "-macro", compiler + ["native.rs", "-Clink-arg=value.o"])
        variants[phase] = {"object": mach(case / "value.o"), "dylib": mach(case / "libnative.dylib")}
    run(phase + "-rustc", compiler + ["reader.rs", "--extern", "native=libnative.dylib",
                                      "-Cincremental=" + str(incremental), "-Zcache-proc-macros",
                                      "-o", "reader"])
    values[phase] = run(phase + "-run", [case / "reader"])
result = {"arm": arm, "inputs": inputs, "native": variants, "values": values}
(packet / "result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
print("DARWIN_NATIVE_VALUES", json.dumps(values, sort_keys=True), flush=True)
assert variants["original"]["dylib"]["sections"]["__TEXT,__text"] != variants["edited"]["dylib"]["sections"]["__TEXT,__text"]
assert values["original"] == values["restored-empty"] == "17,17"
assert values["edited-empty"] == "29,29"
if arm == "fixed":
    assert values["edited"] == values["repeat"] == "29,29" and values["restored"] == "17,17"
