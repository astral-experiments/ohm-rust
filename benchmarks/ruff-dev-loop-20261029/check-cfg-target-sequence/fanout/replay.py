"""Reproduce both TextRange API switches in one new local checkout on Linux x86-64."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


here = Path(__file__).resolve().parent
plan = json.loads((here / "registration.json").read_bytes())["input"]
harnesses = {"auto_traits", "constructors", "indexing", "main"}


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("ruff", type=Path, help="local Git clone containing the recorded Ruff revision")
    args.add_argument("sdk", type=Path, help="complete Rust toolchain with the selected Cargo")
    args.add_argument("cargo_home", type=Path, help="dedicated, populated offline registry")
    args.add_argument("output", type=Path, help="new disposable work directory")
    args.add_argument("--callgrind", type=Path, help="optional compiled ../replay/trace.c")
    a = args.parse_args()
    rustup = shutil.which("rustup")
    if rustup is None:
        args.error("rustup is required")
    sdk = a.sdk.resolve(strict=True)
    cargo_home = a.cargo_home.resolve(strict=True)
    out = a.output.resolve()
    trace = a.callgrind.resolve(strict=True) if a.callgrind else None
    wild = sdk / "lib/rustlib/x86_64-unknown-linux-gnu/bin/wild-ld/ld.bfd"
    if trace and (not wild.is_file() or not Path("/usr/bin/valgrind.bin").is_file()):
        args.error("the optional counter requires the SDK's stock Wild and system Valgrind")
    if not (cargo_home / "registry").is_dir() or not (sdk / "bin/cargo").is_file():
        args.error("expected an offline registry and complete SDK")

    out.mkdir()
    src = out / "ruff"
    subprocess.run(["git", "clone", "--quiet", "--shared", "--no-checkout", a.ruff.resolve(), src], check=True)
    subprocess.run(["git", "-C", src, "checkout", "--quiet", "--detach", plan["root_source"]], check=True)
    member = src / "crates/ruff_text_size/src/range.rs"
    with (here / "ruff-change.patch").open("rb") as patch:
        subprocess.run(["git", "-C", src, "apply", "-R", "--check", "-"], stdin=patch, check=True)
    source = {"H": member.read_bytes()}
    with (here / "ruff-change.patch").open("rb") as patch:
        subprocess.run(["git", "-C", src, "apply", "-R", "-"], stdin=patch, check=True)
    source["P"] = member.read_bytes()
    assert {s: hashlib.sha256(b).hexdigest() for s, b in source.items()} == plan["physical_source_sha"]

    alias = out / "rustup/toolchains/ohm"
    alias.parent.mkdir(parents=True)
    alias.symlink_to(sdk, target_is_directory=True)
    bins = out / "bin"
    bins.mkdir()
    for name in ("cargo", "rustc", "rustdoc"):
        (bins / name).symlink_to(rustup)
    env = {k: os.environ[k] for k in ("HOME", "USER", "PATH") if k in os.environ}
    env.update(PATH=str(bins) + os.pathsep + env["PATH"], RUSTUP_HOME=str(out / "rustup"),
               RUSTUP_AUTO_INSTALL="0", CARGO_HOME=str(cargo_home), CARGO_NET_OFFLINE="true",
               CARGO_CACHE_AUTO_CLEAN_FREQUENCY="never", CARGO_TERM_COLOR="never", LC_ALL="C.UTF-8",
               CARGO_TARGET_DIR=str(out / "target"), CARGO_BUILD_BUILD_DIR=str(out / "shared"))

    def run(name, command, status=0):
        result = subprocess.run(command, cwd=src, env=env, capture_output=True, timeout=600)
        (out / (name + ".stdout")).write_bytes(result.stdout)
        (out / (name + ".stderr")).write_bytes(result.stderr)
        assert result.returncode == status, (name, result.returncode, result.stderr[-2000:])
        return result

    phases = {}
    for label, state, cold in [("train", "H", False), ("remove", "P", False), ("add", "H", False),
                               ("empty-removed", "P", True), ("empty-added", "H", True)]:
        if cold:
            for folder in ("shared", "target"):
                (out / folder).rename(out / (label + "-held-" + folder))
        member.write_bytes(source[state])
        assert digest(member) == plan["physical_source_sha"][state]
        command = plan["cli"]
        measured = trace and label in ("remove", "add")
        if measured:
            dest = out / (label + "-profiles")
            dest.mkdir(mode=0o700)
            dirty = run(label, [trace, dest, sdk / "bin/rustc", wild,
                                ",".join(sorted(harnesses | {"ruff_text_size"})), "--", *command])
            lines = dirty.stdout.decode().splitlines()
            assert lines[:2] == ["cargo 0", "links 4"]
            jobs = [line.split() for line in lines[2:]]
            assert len(jobs) == 5 and {j[2] for j in jobs} == harnesses | {"ruff_text_size"}
            for kind, pid, crate, attached, code in jobs:
                assert (kind, attached, code) == ("compiler", "1", "0")
                assert (dest / f"output.{pid}.{pid}").is_file()
        else:
            run(label, command)
        result = run(label + "-repeat", [*command, "--message-format=json"])
        msgs = [json.loads(line) for line in result.stdout.splitlines()]
        assert msgs[-1] == {"reason": "build-finished", "success": True}
        arts = [m for m in msgs if m["reason"] == "compiler-artifact"]
        assert len(arts) == 11 and all(m["fresh"] for m in arts)
        paths = [(row["target"]["name"], Path(f)) for row in arts for f in row["filenames"]]
        assert len(paths) == 16
        root, = [f for name, f in paths if name == "ruff_text_size" and f.suffix == ".rlib"]
        exes = {row["target"]["name"]: Path(row["executable"]) for row in arts if row["executable"]}
        assert set(exes) == harnesses
        counts = {}
        for name, exe in exes.items():
            listing = run(label + "-" + name + "-list", [exe, "--list"]).stdout
            counts[name] = int(re.search(rb"(\d+) tests?, \d+ benchmarks?\s*$", listing)[1])
            assert b"test result: ok" in run(label + "-" + name + "-tests", [exe, "--test-threads=1"]).stdout
        assert sum(counts.values()) == 10
        exe = out / "range-reader"
        external = run(label + "-reader", ["rustc", "+ohm", "--edition=2024", "--crate-name", "range_reader",
                       here / "reader.rs", "--extern", "ruff_text_size=" + str(root), "-L",
                       "dependency=" + str(root.parent), "-o", exe], 0 if state == "H" else 1)
        if state == "H":
            assert run(label + "-reader-run", [exe]).stdout == b"1..4\n"
        else:
            assert b"E0599" in external.stderr and b"to_std_range" in external.stderr
        phases[label] = {"source": state, "root_archive": digest(root),
                         "tests": {name: digest(exe) for name, exe in exes.items()},
                         "counts": counts, "external": digest(exe) if state == "H" else "E0599"}
    for warm, clean in (("add", "empty-added"), ("remove", "empty-removed")):
        for key in ("tests", "counts", "external"):
            assert phases[warm][key] == phases[clean][key], (warm, clean, key)
    print(json.dumps(phases, sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
