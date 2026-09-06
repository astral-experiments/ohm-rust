#!/usr/bin/env python3
"""Measure sparse metadata on the pinned syn and Ruff sources.

Run `unpack`, optionally `fetch` to populate Cargo's source cache, then `prepare`.
Preparation resolves/builds dependencies with --locked --offline. Run `native` and
`instructions` separately during a quiet window, then `report`. Every measured
command is a direct, nonincremental compiler invocation, never a Cargo cache hit.

The parser and ty cases read real dependency metadata and write their own metadata;
these measure complete compilations, not isolated decoder/encoder microbenchmarks.
"""

import argparse
import hashlib
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

RUSTC_PERF_REVISION = "c0301bc44d175b9b2c5442b25049475c39d7700c"
RUFF_REVISION = "efcffd2178ce62e9951a53d4c50cadc225a0cfec"
ARCHIVES = {
    "rustc-perf": (
        "rustc-perf-source.tar.gz",
        "95f5c094a63bf46dbc052c7f93c9ad603b42a924cb0422026063cc0a2ec5f9bb",
        True,
    ),
    "ruff": (
        "ruff-codspeed-codeload.tar.gz",
        "cef811e0551b63ceb49ea2fb49af0d8d1dffb197c02b62a916f218cd4f7b381d",
        True,
    ),
}
ARMS = ("baseline", "sparse-metadata")
CASES = ("syn", "ruff_python_ast", "ruff_python_parser", "ty_python_semantic")
ORDERS = (ARMS[0], ARMS[1], ARMS[1], ARMS[0], ARMS[1], ARMS[0], ARMS[0], ARMS[1])
COMPILE_ENV = {
    "CARGO_BIN_NAME",
    "CARGO_CRATE_NAME",
    "CARGO_MANIFEST_DIR",
    "CARGO_MANIFEST_PATH",
    "CARGO_PRIMARY_PACKAGE",
    "LD_LIBRARY_PATH",
    "OUT_DIR",
    "RUSTC_BOOTSTRAP",
    "RUSTC_FORCE_RUSTC_VERSION",
}


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def capture():
    """The same program also acts as Cargo's compiler wrapper during preparation."""
    command = sys.argv[1:]
    if "--crate-name" in command:
        name = command[command.index("--crate-name") + 1]
        if name in os.environ["OHM_SPARSE_CAPTURE_CRATES"].split(","):
            env = {
                key: value
                for key, value in os.environ.items()
                if key in COMPILE_ENV
                or key.startswith(("CARGO_PKG_", "CARGO_FEATURE_"))
            }
            path = Path(os.environ["OHM_SPARSE_CAPTURE_DIR"]) / (name + ".json")
            with path.open("x") as stream:
                json.dump(
                    {"command": command, "cwd": os.getcwd(), "env": env},
                    stream,
                    indent=2,
                )
    os.execv(command[0], command)


def command(argv, cwd, env, log):
    started = time.perf_counter_ns()
    with Path(log).open("xb") as stream:
        result = subprocess.run(
            argv, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT, check=False
        )
    elapsed = time.perf_counter_ns() - started
    if result.returncode:
        raise RuntimeError(f"command exited {result.returncode}: {argv}; see {log}")
    return elapsed


def environment(args):
    env = dict(os.environ)
    for key in (
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_BUILD_DIR",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTFLAGS",
        "RUSTC",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTUP_TOOLCHAIN",
        "CARGO_MAKEFLAGS",
        "MAKEFLAGS",
        "MFLAGS",
    ):
        env.pop(key, None)
    env.update(
        CARGO_HOME=str(args.cargo_home),
        CARGO_INCREMENTAL="0",
        CARGO_TERM_COLOR="never",
        RUSTC_BOOTSTRAP="1",
        RUSTC_FORCE_RUSTC_VERSION="rustc-perf",
        LD_LIBRARY_PATH=os.pathsep.join(
            part
            for part in (str(args.root / "llvm/lib"), env.get("LD_LIBRARY_PATH"))
            if part
        ),
    )
    return env


def inventory(root):
    result = []
    for path in sorted(Path(root).rglob("*.rmeta")):
        stat = path.stat()
        result.append(
            {"path": str(path), "bytes": stat.st_size, "sha256": digest(path)}
        )
    return result


def source_digest(root):
    hasher = hashlib.sha256()
    for path in sorted(Path(root).rglob("*")):
        if path.is_symlink():
            content = "symlink:" + os.readlink(path)
        elif path.is_file():
            content = digest(path)
        else:
            continue
        hasher.update(
            str(path.relative_to(root)).encode() + b"\0" + content.encode() + b"\n"
        )
    return hasher.hexdigest()


def sources(args):
    root = args.root / "bench-sources"
    return {
        "syn": root / "rustc-perf/collector/compile-benchmarks/syn-2.0.101",
        "ruff": root / "ruff",
    }


def unpack(args):
    for name, (filename, expected, strip) in ARCHIVES.items():
        archive = args.root / "inputs" / filename
        assert digest(archive) == expected, archive
        destination = args.root / "bench-sources" / name
        destination.mkdir(parents=True, exist_ok=False)
        subprocess.run(
            ["tar", "-xzf", str(archive), "-C", str(destination)]
            + (["--strip-components=1"] if strip else []),
            check=True,
        )
    save(
        args.root / "bench-sources/provenance.json",
        {
            "archives": ARCHIVES,
            "rustc_perf_revision": RUSTC_PERF_REVISION,
            "ruff_revision": RUFF_REVISION,
            "source_digests": {
                key: source_digest(path) for key, path in sources(args).items()
            },
        },
    )


def fetch(args):
    # This optional, explicitly separate step is the only one permitting network access.
    env = environment(args)
    env["RUSTC"] = str(args.root / "toolchain/bin/rustc")
    for path in sources(args).values():
        subprocess.run(
            [str(args.cargo), "fetch", "--locked"], cwd=path, env=env, check=True
        )


def prepare(args):
    evidence = args.root / "sparse-metadata-bench"
    targets = args.target_root / "sparse-metadata-bench"
    identity = {
        "cargo": str(args.cargo),
        "cargo_sha256": digest(args.cargo),
        "cargo_version": subprocess.check_output(
            [str(args.cargo), "--version"], text=True, env=environment(args)
        ),
        "source_provenance": json.loads(
            (args.root / "bench-sources/provenance.json").read_text()
        ),
        "scope": "Nonincremental metadata-only real-crate compiler invocations",
    }
    if evidence.exists():
        manifest = json.loads((evidence / "manifest.json").read_text())
        assert all(manifest[key] == value for key, value in identity.items())
    else:
        evidence.mkdir()
        manifest = {**identity, "arms": {}, "complete": False}
        save(evidence / "manifest.json", manifest)
    assert len(set(args.arms)) == len(args.arms)
    assert not set(args.arms).intersection(manifest["arms"]), "Arm already prepared"
    for arm in args.arms:
        sysroot = args.target_root / arm / "x86_64-unknown-linux-gnu/stage1"
        rustc = sysroot / "bin/rustc"
        arm_dir = evidence / arm
        arm_dir.mkdir()
        env = environment(args)
        env.update(
            RUSTC=str(rustc),
            RUSTFLAGS="-Zthreads=1",
            CARGO_BUILD_JOBS=str(args.jobs),
            RUSTC_WRAPPER=str(Path(__file__).resolve()),
            OHM_SPARSE_CAPTURE_DIR=str(arm_dir),
        )
        entry = {
            "sysroot": str(sysroot),
            "rustc_sha256": digest(rustc),
            "driver_sha256": {
                str(p): digest(p)
                for p in sorted((sysroot / "lib").glob("librustc_driver-*.so"))
            },
            "rustc_version": subprocess.check_output(
                [str(rustc), "-vV"], text=True, env=env
            ),
            "llvm_runtime_path": str(args.root / "llvm/lib"),
            "preparation": [],
            "cases": {},
        }
        receipt = args.root / "logs" / (arm + "-status.json")
        if receipt.exists():
            entry["compiler_build"] = json.loads(receipt.read_text())
            assert entry["compiler_build"]["returncode"] == 0
        manifest["arms"][arm] = entry
        for name, source in sources(args).items():
            target = targets / arm / name
            assert not target.exists(), target
            env["CARGO_TARGET_DIR"] = str(target)
            env["OHM_SPARSE_CAPTURE_CRATES"] = (
                "syn" if name == "syn" else ",".join(CASES[1:])
            )
            argv = [
                str(args.cargo),
                "rustc",
                "--locked",
                "--offline",
                "--lib",
                "--profile",
                "check",
            ]
            if name == "ruff":
                argv += ["--package", "ty_python_semantic"]
            log = arm_dir / (name + "-prepare.log")
            elapsed = command(argv, source, env, log)
            entry["preparation"].append(
                {
                    "command": argv,
                    "cwd": str(source),
                    "elapsed_ns": elapsed,
                    "log": str(log),
                    "metadata": inventory(target),
                }
            )
        for case in CASES:
            captured = json.loads((arm_dir / (case + ".json")).read_text())
            argv = captured["command"]
            assert Path(argv[0]).resolve() == rustc.resolve()
            assert not any("incremental=" in arg for arg in argv), argv
            argv = [
                "--emit=metadata" if arg.startswith("--emit=") else arg for arg in argv
            ]
            if not any(arg.startswith("--emit=") for arg in argv):
                argv += ["--emit=metadata"]
            outdir = evidence / "outputs" / arm / case
            outdir.mkdir(parents=True)
            argv[argv.index("--out-dir") + 1] = str(outdir)
            entry["cases"][case] = {**captured, "command": argv, "outputs": str(outdir)}
        save(evidence / "manifest.json", manifest)
    finalize(args)


def finalize(args):
    """Validate source and command identities after both captures are available."""
    evidence = args.root / "sparse-metadata-bench"
    targets = args.target_root / "sparse-metadata-bench"
    manifest = json.loads((evidence / "manifest.json").read_text())
    for name, path in sources(args).items():
        assert (
            source_digest(path) == manifest["source_provenance"]["source_digests"][name]
        )
    if set(manifest["arms"]) != set(ARMS):
        save(evidence / "manifest.json", manifest)
        return
    # The compiler and dependency locations differ. All other options must match.
    for case in CASES:
        normalized = []
        for arm in ARMS:
            argv = manifest["arms"][arm]["cases"][case]["command"]
            normalized.append(
                [
                    arg.replace(str(targets / arm), "<dependencies>")
                    .replace(str(args.target_root / arm), "<sysroot>")
                    .replace(str(evidence / "outputs" / arm), "<outputs>")
                    for arg in argv
                ]
            )
        # Cargo derives disambiguators from the compiler source revision. Normalize those only
        # for comparison; measurements retain each arm's original, captured arguments.
        normalized = [
            [
                re.sub(
                    r"-[0-9a-f]{16}(?=\.(?:rmeta|rlib|so))",
                    "-<hash>",
                    re.sub(r"^(metadata|extra-filename)=.*", r"\1=<hash>", arg),
                )
                for arg in argv
            ]
            for argv in normalized
        ]
        manifest.setdefault("normalized_commands_equal", {})[case] = (
            normalized[0] == normalized[1]
        )
        if normalized[0] != normalized[1]:
            save(evidence / (case + "-command-mismatch.json"), normalized)
            raise RuntimeError(f"Non-equivalent compiler arguments for {case}")
    manifest["complete"] = True
    save(evidence / "manifest.json", manifest)


def cpu_counters():
    fields = Path("/proc/stat").read_text().splitlines()[0].split()[1:9]
    return [int(value) for value in fields]


def quiet(allow_concurrent=False):
    busy = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            name = (entry / "comm").read_text().strip()
            if (entry / "stat").read_text().split(") ", 1)[1].startswith("Z "):
                continue
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue
        if name in {
            "rustc",
            "cargo",
            "clippy-driver",
            "valgrind",
            "cc1",
            "clang",
            "ninja",
            "cmake",
        } or name.startswith(("cachegrind", "callgrind")):
            busy.append({"pid": entry.name, "comm": name})
    if busy and not allow_concurrent:
        raise RuntimeError(f"Unrelated build process exists: {busy}")
    before = cpu_counters()
    time.sleep(0.25)
    after = cpu_counters()
    delta = [b - a for a, b in zip(before, after)]
    total = sum(delta)
    sample = {
        "idle": delta[3] / total,
        "iowait": delta[4] / total,
        "steal": delta[7] / total,
        "other_build_processes": busy,
        "concurrent_instructions_allowed": allow_concurrent,
    }
    if not allow_concurrent and (
        sample["idle"] < 0.90 or sample["iowait"] > 0.02 or sample["steal"] > 0.01
    ):
        raise RuntimeError(f"Host is not quiet: {sample}")
    return sample


def measure(args):
    evidence = args.root / "sparse-metadata-bench"
    manifest = json.loads((evidence / "manifest.json").read_text())
    assert manifest["complete"]
    output = evidence / args.operation
    output.mkdir(exist_ok=False)
    for arm in ARMS:
        prepared = manifest["arms"][arm]
        assert (
            digest(Path(prepared["sysroot"]) / "bin/rustc") == prepared["rustc_sha256"]
        )
        for path, expected in prepared["driver_sha256"].items():
            assert digest(path) == expected
    order = ORDERS
    if args.operation == "instructions":
        order = ORDERS[: args.instruction_samples * 2]
    save(
        output / "environment.json",
        {
            "uname": list(os.uname()),
            "cpuinfo": Path("/proc/cpuinfo").read_text(),
            "affinity": sorted(os.sched_getaffinity(0)),
            "driver_sha256": digest(__file__),
            "valgrind_version": subprocess.check_output(
                [args.valgrind, "--version"], text=True
            )
            if args.operation == "instructions"
            else None,
            "order": order,
            "cycles": args.cycles,
            "cases": args.cases,
            "allow_concurrent_instructions": args.allow_concurrent_instructions,
        },
    )
    records = []
    for case in args.cases:
        # Warm both the compiler and dependency mappings before accepted observations.
        sequence = [(arm, "warmup", 0) for arm in ARMS]
        for cycle in range(args.cycles):
            sequence.extend((arm, "measured", cycle) for arm in order)
        for index, (arm, phase, cycle) in enumerate(sequence):
            prepared = manifest["arms"][arm]["cases"][case]
            argv = prepared["command"]
            env = environment(args)
            env.update(prepared["env"])
            for path in Path(prepared["outputs"]).glob("*.rmeta"):
                path.unlink()
            sample = quiet(args.allow_concurrent_instructions)
            stem = f"{case}-{index:02}-{arm}"
            log = output / (stem + ".log")
            cg = output / (stem + ".cachegrind")
            if args.operation == "instructions" and phase == "measured":
                argv = [
                    args.valgrind,
                    "--tool=cachegrind",
                    "--cache-sim=no",
                    "--branch-sim=no",
                    "--error-exitcode=99",
                    f"--cachegrind-out-file={cg}",
                    *argv,
                ]
            started = time.time()
            elapsed = command(argv, prepared["cwd"], env, log)
            row = {
                "case": case,
                "arm": arm,
                "phase": phase,
                "cycle": cycle,
                "order": index,
                "command": argv,
                "cwd": prepared["cwd"],
                "environment": prepared["env"],
                "started": started,
                "elapsed_ns": elapsed,
                "quiet_sample": sample,
                "log": str(log),
                "metadata": inventory(prepared["outputs"]),
            }
            assert row["metadata"], row
            notifications = []
            for line in log.read_text(errors="replace").splitlines():
                try:
                    item = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(item, dict) and item.get("$message_type") == "artifact":
                    notifications.append(item)
            row["artifact_notifications"] = notifications
            if cg.exists():
                lines = cg.read_text().splitlines()
                events = next(
                    line.split()[1:] for line in lines if line.startswith("events:")
                )
                summary = next(
                    line.split()[1:] for line in lines if line.startswith("summary:")
                )
                row["instructions"] = int(summary[events.index("Ir")])
                row["cachegrind"] = str(cg)
                assert row["instructions"] > 0
            records.append(row)
            save(output / "samples.json", records)
            print(case, arm, phase, elapsed / 1e9, row.get("instructions"), flush=True)


def report(args):
    evidence = args.root / "sparse-metadata-bench"
    manifest = json.loads((evidence / "manifest.json").read_text())
    result = {"manifest": str(evidence / "manifest.json"), "cases": {}}
    for operation, metric in (
        ("native", "elapsed_ns"),
        ("instructions", "instructions"),
    ):
        path = evidence / operation / "samples.json"
        if not path.exists():
            continue
        rows = json.loads(path.read_text())
        for case in CASES:
            arms = {}
            for arm in ARMS:
                selected = [
                    r
                    for r in rows
                    if r["case"] == case
                    and r["arm"] == arm
                    and r["phase"] == "measured"
                ]
                if not selected:
                    continue
                values = [r[metric] for r in selected]
                sizes = [sum(p["bytes"] for p in r["metadata"]) for r in selected]
                assert len(set(sizes)) == 1, (case, arm, sizes)
                arms[arm] = {
                    "values": values,
                    "median": statistics.median(values),
                    "min": min(values),
                    "max": max(values),
                    "metadata_bytes": sizes[0],
                    "public_api_hashes": sorted(
                        {
                            item["public_api_hash"]["value"]
                            for row in selected
                            for item in row["artifact_notifications"]
                            if item.get("public_api_hash")
                        }
                    ),
                }
            if len(arms) == 2:
                result["cases"].setdefault(case, {})[operation] = {
                    "arms": arms,
                    "candidate_delta_percent": 100
                    * (arms[ARMS[1]]["median"] / arms[ARMS[0]]["median"] - 1),
                    "metadata_delta_percent": 100
                    * (
                        arms[ARMS[1]]["metadata_bytes"]
                        / arms[ARMS[0]]["metadata_bytes"]
                        - 1
                    ),
                    "public_api_hashes_equal": (
                        arms[ARMS[0]]["public_api_hashes"]
                        == arms[ARMS[1]]["public_api_hashes"]
                        if arms[ARMS[0]]["public_api_hashes"]
                        and arms[ARMS[1]]["public_api_hashes"]
                        else None
                    ),
                }
    result["unchanged_sources"] = {
        name: source_digest(path)
        == manifest["source_provenance"]["source_digests"][name]
        for name, path in sources(args).items()
    }
    assert all(result["unchanged_sources"].values())
    result["limitations"] = [
        "Leaf .rmeta logical bytes, not whole-target physical allocation or APFS clone accounting.",
        "Compiler process instruction counts; no child-process instruction aggregation.",
        "Nonincremental warm-page-cache checking; no clean-build or incremental latency claim.",
        "Parser and ty combine dependency metadata reads with their own analysis and encoding.",
    ]
    save(evidence / "summary.json", result)
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "operation",
        choices=(
            "unpack",
            "fetch",
            "prepare",
            "finalize",
            "native",
            "instructions",
            "report",
        ),
    )
    parser.add_argument(
        "--root", type=Path, default=Path("/tmp/ohm-target-size-prototypes")
    )
    parser.add_argument(
        "--target-root",
        type=Path,
        default=Path(
            "/tmp/codex-cargo-targets/ohm-rust/isolated/target-size-prototypes"
        ),
    )
    parser.add_argument("--cargo", type=Path)
    parser.add_argument(
        "--cargo-home", type=Path, default=Path("/tmp/codex-cargo-home")
    )
    parser.add_argument("--jobs", type=int, default=16)
    parser.add_argument(
        "--arms",
        nargs="+",
        choices=ARMS,
        default=list(ARMS),
        help="Arms to prepare; separate calls may add each arm once",
    )
    parser.add_argument(
        "--cycles",
        type=int,
        default=1,
        help="Each ABBA+BAAB cycle measures four calls per arm",
    )
    parser.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    parser.add_argument(
        "--instruction-samples",
        type=int,
        choices=(2, 4),
        default=2,
        help="Samples per arm per cycle in the instruction phase (ABBA or ABBA+BAAB)",
    )
    parser.add_argument(
        "--allow-concurrent-instructions",
        action="store_true",
        help=(
            "Permit co-running instruction counters; CG elapsed time is never a performance metric"
        ),
    )
    parser.add_argument("--valgrind", default="valgrind")
    args = parser.parse_args()
    args.cargo = args.cargo or args.root / "toolchain/bin/cargo"
    assert args.cycles > 0
    assert not args.allow_concurrent_instructions or args.operation == "instructions"
    if args.operation in ("native", "instructions"):
        measure(args)
    else:
        globals()[args.operation](args)


if __name__ == "__main__":
    if "OHM_SPARSE_CAPTURE_DIR" in os.environ:
        capture()
    else:
        main()
