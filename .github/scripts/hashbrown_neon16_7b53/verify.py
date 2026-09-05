#!/usr/bin/env python3
"""Run the unchanged vendored package and extra harnesses on native little-endian ARM."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import traceback

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
TOOLCHAIN = "1.97.1"
COMPILER_COMMIT = "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
FEATURES = {
    "default": [],
    "nightly": ["--features", "nightly"],
    "serde-rayon": ["--features", "serde,rayon"],
    "nightly-serde-rayon": ["--features", "nightly,serde,rayon"],
    "no-default": ["--no-default-features"],
    "compiler": ["--no-default-features", "--features", "nightly"],
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_source(path, manifest):
    files = {}
    for entry in sorted(path.rglob("*")):
        if entry.is_symlink():
            raise ValueError(f"source contains a symlink: {entry}")
        if entry.is_file():
            files[str(entry.relative_to(path))] = sha256(entry)
    if files != manifest["files"] or len(files) != 50:
        raise ValueError(f"source manifest mismatch: {path}")
    changed = [name for name, digest in files.items() if manifest["upstream_files"][name] != digest]
    if changed != ["src/control/bitmask.rs", "src/control/group/neon.rs"]:
        raise ValueError(f"unexpected upstream source changes: {changed}")
    return files


def clean_environment():
    env = os.environ.copy()
    for name in list(env):
        if name in {
            "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
            "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC_BOOTSTRAP", "RUSTUP_TOOLCHAIN",
        } or name.startswith(("CARGO_BUILD_", "CARGO_TARGET_", "CARGO_PROFILE_")):
            del env[name]
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--features", choices=list(FEATURES)[:4], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((HERE / "source-manifest.json").read_text())
    shutil.copyfile(HERE / "source-manifest.json", output / "source-manifest.json")
    source = ROOT / "compiler/rustc_hashbrown"
    package = None
    report = {"status": "running", "features": args.features, "commands": [], "libraries": []}
    env = clean_environment()

    def run(label, command, *, cwd=ROOT, bootstrap=False, extra_env=None):
        command = [str(arg) for arg in command]
        log = output / f"{len(report['commands']):02}-{label}.log"
        child_env = env.copy()
        if bootstrap:
            child_env["RUSTC_BOOTSTRAP"] = "1"
        if extra_env:
            child_env.update(extra_env)
        record = {"label": label, "command": command, "cwd": str(cwd),
                  "rustc_bootstrap": bootstrap, "log": log.name}
        report["commands"].append(record)
        print(f"Running {label}: {' '.join(command)}", flush=True)
        with log.open("w") as stream:
            completed = subprocess.run(command, cwd=cwd, env=child_env, stdout=stream,
                                       stderr=subprocess.STDOUT, timeout=900)
        record["returncode"] = completed.returncode
        if completed.returncode:
            print(log.read_text()[-24000:], file=sys.stderr)
            raise RuntimeError(f"{label} exited with {completed.returncode}")
        return log.read_text()

    def cargo(label, subcommand, feature_set, profile="debug", options=()):
        flags = FEATURES[feature_set]
        return run(label, ["cargo", f"+{TOOLCHAIN}", subcommand, "--locked", *flags,
                           *(["--release"] if profile == "release" else []), *options],
                   cwd=package, bootstrap=feature_set in {"nightly", "nightly-serde-rayon", "compiler"})

    def boundaries(feature_set, profile):
        data = cargo(f"{feature_set}-{profile}-build", "build", feature_set, profile,
                     ["--lib", "--message-format=json"])
        artifacts = []
        for line in data.splitlines():
            if line.startswith("{"):
                item = json.loads(line)
                if item.get("reason") == "compiler-artifact" and item["target"]["name"] == "hashbrown":
                    if Path(item["manifest_path"]).resolve() != package / "Cargo.toml":
                        raise ValueError("hashbrown artifact came from a different package")
                    features = set(item["features"])
                    if feature_set in {"compiler", "no-default"}:
                        expected = {"nightly"} if feature_set == "compiler" else set()
                        if features != expected:
                            raise ValueError(f"unexpected compiler/no-default features: {features}")
                    elif (("nightly" in features) != ("nightly" in feature_set)
                          or ("serde" in features) != ("serde" in feature_set)
                          or ("rayon" in features) != ("rayon" in feature_set)
                          or "default" not in features):
                        raise ValueError(f"unexpected features: {features}")
                    artifacts.extend(Path(name) for name in item["filenames"] if name.endswith(".rlib"))
        if len(artifacts) != 1:
            raise ValueError(f"expected one actual hashbrown rlib, got {artifacts}")
        library = artifacts[0]
        report["libraries"].append({"features": feature_set, "profile": profile,
                                    "path": str(library), "sha256": sha256(library), "cargo_features": sorted(features)})
        binary = scratch / f"boundaries-{feature_set}-{profile}"
        optimize = ["-C", "opt-level=3", "-C", "debug-assertions=no"] if profile == "release" else []
        run(f"{feature_set}-{profile}-boundary-compile",
            ["rustc", f"+{TOOLCHAIN}", "--test", "--edition=2024", HERE / "table_boundaries.rs",
             "--extern", f"hashbrown={library}", "-L", f"dependency={library.parent}",
             "-L", f"dependency={library.parent / 'deps'}", *optimize, "-o", binary], cwd=scratch)
        run(f"{feature_set}-{profile}-boundaries", [binary], cwd=scratch)

    try:
        verify_source(source, manifest)
        stage0 = dict(line.split("=", 1) for line in (ROOT / "src/stage0").read_text().splitlines()
                      if line.startswith("compiler_"))
        if stage0["compiler_version"] != TOOLCHAIN or stage0["compiler_git_commit_hash"] != COMPILER_COMMIT:
            raise ValueError("toolchain differs from the baseline src/stage0")
        if platform.machine() not in {"aarch64", "arm64"} or sys.byteorder != "little":
            raise ValueError("validation requires a native little-endian ARM host")
        report["host"] = platform.uname()._asdict()
        report["revision"] = run("revision", ["git", "rev-parse", "HEAD"]).strip()
        version = run("rustc-version", ["rustc", f"+{TOOLCHAIN}", "-vV"])
        if f"release: {TOOLCHAIN}\n" not in version or f"commit-hash: {COMPILER_COMMIT}\n" not in version:
            raise ValueError("unexpected compiler version or commit")
        if "host: aarch64-unknown-linux-gnu\n" not in version:
            raise ValueError("compiler is not the native ARM Linux toolchain")
        cfg = set(run("rustc-cfg", ["rustc", f"+{TOOLCHAIN}", "--print", "cfg"]).splitlines())
        if not {'target_arch="aarch64"', 'target_endian="little"', 'target_feature="neon"'} <= cfg or "miri" in cfg:
            raise ValueError("compiler cfg does not select the unchanged NEON dispatch branch")
        scratch = Path(tempfile.mkdtemp(prefix="hashbrown-neon16-", dir=os.environ["RUNNER_TEMP"])).resolve()
        if scratch.is_relative_to(ROOT):
            raise ValueError("standalone package must be outside the compiler workspace")
        package = scratch / "hashbrown"
        shutil.copytree(source, package)
        verify_source(package, manifest)
        # Keep Cargo configuration and build output outside the source copy.
        env["CARGO_HOME"] = str(scratch / "cargo-home")
        env["CARGO_TARGET_DIR"] = str(scratch / "target")
        env["CARGO_INCREMENTAL"] = "0"
        report["standalone_package"] = str(package)
        report["cargo_toml_rewritten"] = False

        if args.features == "default":
            include_env = {"NEON_CANDIDATE": str(package)}
            for profile in ["debug", "release"]:
                binary = scratch / f"intrinsics-{profile}"
                optimize = ["-O"] if profile == "release" else []
                run(f"intrinsics-{profile}-compile",
                    ["rustc", f"+{TOOLCHAIN}", "--test", "--edition=2024", HERE / "arm_group.rs",
                     *optimize, "-o", binary], cwd=scratch, extra_env=include_env)
                run(f"intrinsics-{profile}", [binary], cwd=scratch)
            run("intrinsics-assembly", ["rustc", f"+{TOOLCHAIN}", "--crate-type=lib", "--edition=2024",
                "--emit=asm", "-O", HERE / "arm_group.rs", "-o", output / "arm-group.s"],
                cwd=scratch, extra_env=include_env)

        for profile in ["debug", "release"]:
            cargo(f"{args.features}-{profile}-upstream", "test", args.features, profile)
            boundaries(args.features, profile)
        # Upstream's tests require default-hasher. Build the exact compiler feature
        # set separately and run the public table harness against that actual rlib.
        extra = {"default": "no-default", "nightly": "compiler"}.get(args.features)
        if extra:
            cargo(f"{extra}-check", "check", extra, options=["--lib"])
            for profile in ["debug", "release"]:
                boundaries(extra, profile)
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        traceback.print_exc()
    finally:
        for label, path in [("checkout", source), ("standalone", package)]:
            if path is None:
                continue
            try:
                verify_source(path, manifest)
                report[f"{label}_source_verified"] = True
            except Exception as error:
                report["status"] = "failed"
                report[f"{label}_source_error"] = str(error)
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"status": report["status"], "features": args.features,
                          "commands": len(report["commands"]), "evidence": str(output)}), flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
