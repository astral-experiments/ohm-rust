#!/usr/bin/env python3
"""Prepare pinned rustc-perf fixtures and capture their direct compiler commands."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess


FIXTURES = {"bitmaps-3.2.1": "bitmaps", "syn-2.0.101": "syn"}
REPOSITORY = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", type=Path, required=True)
    parser.add_argument("--cargo", type=Path, required=True)
    parser.add_argument(
        "--suite",
        type=Path,
        default=REPOSITORY / "src/tools/rustc-perf/collector/compile-benchmarks",
    )
    parser.add_argument("--output", type=Path, default=REPOSITORY / "build/codspeed")
    args = parser.parse_args()
    rustc = args.rustc.resolve(strict=True)
    cargo = args.cargo.resolve(strict=True)
    suite = args.suite.resolve(strict=True)
    output = args.output.resolve()

    if output.exists():
        parser.error(
            f"output already exists; preparation requires a fresh directory: {output}"
        )
    if output.is_relative_to(suite):
        parser.error("output must be outside the rustc-perf source suite")
    for name in FIXTURES:
        for filename in ("Cargo.toml", "Cargo.lock", "0-println.patch"):
            if not (suite / name / filename).is_file():
                parser.error(f"missing fixture file: {suite / name / filename}")

    suite_revision = subprocess.check_output(
        ["git", "-C", str(suite), "rev-parse", "HEAD"], text=True
    ).strip()
    source_changes = subprocess.check_output(
        [
            "git",
            "-C",
            str(suite),
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            *FIXTURES,
        ],
        text=True,
    )
    if source_changes:
        parser.error("selected rustc-perf fixtures must have no local changes")

    manifest = {
        "rustc_version": subprocess.check_output(
            [str(rustc), "-vV"], text=True
        ).strip(),
        "cargo_version": subprocess.check_output(
            [str(cargo), "--version"], text=True
        ).strip(),
        "suite": str(suite),
        "suite_revision": suite_revision,
        "fixtures": [],
    }
    output.mkdir(parents=True)
    targets = Path(os.environ.get("CARGO_TARGET_DIR", output / "targets")).resolve()

    for name, crate_name in FIXTURES.items():
        source = suite / name
        cwd = output / "fixtures" / name
        shutil.copytree(source, cwd)
        # Force Cargo to invoke the leaf even if an external target directory was reused.
        (cwd / "src/lib.rs").touch()
        patch = cwd / "0-println.patch"

        # Otherwise Git discovers the enclosing rust repository and silently skips this patch.
        patch_env = os.environ.copy()
        patch_env.pop("GIT_DIR", None)
        patch_env.pop("GIT_WORK_TREE", None)
        patch_env["GIT_CEILING_DIRECTORIES"] = str(cwd.parent)
        subprocess.run(
            ["git", "apply", "--check", str(patch)], cwd=cwd, env=patch_env, check=True
        )

        capture = output / f"{name}.json"
        environment = os.environ.copy()
        for key in ("CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS", "CARGO_BUILD_BUILD_DIR"):
            environment.pop(key, None)
        environment.update(
            CARGO_ENCODED_RUSTFLAGS="",
            CARGO_INCREMENTAL="0",
            CARGO_TARGET_DIR=str(targets / name),
            CARGO_TERM_COLOR="never",
            OHM_CAPTURE_CRATE=crate_name,
            OHM_CAPTURE_FILE=str(capture),
            RUSTC=str(rustc),
            RUSTC_BOOTSTRAP="1",
            RUSTC_FORCE_RUSTC_VERSION="rustc-perf",
            RUSTC_WORKSPACE_WRAPPER="",
            RUSTC_WRAPPER=str(Path(__file__).with_name("capture.py").resolve()),
            RUSTFLAGS="",
        )
        subprocess.run(
            [
                str(cargo),
                "rustc",
                "--locked",
                "--lib",
                "--profile",
                "check",
                "--",
                "-Zthreads=1",
                "-Zincremental-verify-ich",
            ],
            cwd=cwd,
            env=environment,
            check=True,
        )
        if not capture.is_file():
            raise RuntimeError(f"Cargo did not invoke the compiler for {name}")
        invocation = json.loads(capture.read_text())
        if invocation["command"][0] != str(rustc):
            raise RuntimeError(f"Cargo invoked a different compiler for {name}")
        manifest["fixtures"].append(
            {
                **invocation,
                "name": name,
                "source": str(source),
                "incremental": str(output / "incremental" / name),
                "patch": str(patch),
            }
        )

    manifest_path = output / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Prepared compiler benchmarks: {manifest_path}")


if __name__ == "__main__":
    main()
