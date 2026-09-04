#!/usr/bin/env python3
"""Prepare pinned rustc-perf fixtures and capture their direct compiler commands."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

from benchmarks import RUSTC_PERF_FIXTURES, SHARDS, WORKSPACES, selected_fixtures

REPOSITORY = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", type=Path, required=True)
    parser.add_argument("--cargo", type=Path, required=True)
    parser.add_argument("--shard", choices=SHARDS)
    parser.add_argument(
        "--suite",
        type=Path,
        default=REPOSITORY / "src/tools/rustc-perf/collector/compile-benchmarks",
    )
    parser.add_argument("--output", type=Path, default=REPOSITORY / "build/codspeed")
    parser.add_argument(
        "--workspaces", type=Path, default=REPOSITORY / "build/codspeed-sources"
    )
    args = parser.parse_args()
    rustc = args.rustc.resolve(strict=True)
    cargo = args.cargo.resolve(strict=True)
    suite = args.suite.resolve(strict=True)
    output = args.output.resolve()
    workspaces = args.workspaces.resolve(strict=True)

    if output.exists():
        parser.error(
            f"output already exists; preparation requires a fresh directory: {output}"
        )
    if output.is_relative_to(suite) or output.is_relative_to(workspaces):
        parser.error("output must be outside the benchmark sources")
    for name in RUSTC_PERF_FIXTURES:
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
            *RUSTC_PERF_FIXTURES,
        ],
        text=True,
    )
    if source_changes:
        parser.error("selected rustc-perf fixtures must have no local changes")

    for workspace in WORKSPACES.values():
        source = workspaces / workspace["directory"]
        revision = subprocess.check_output(
            ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
        ).strip()
        if revision != workspace["revision"]:
            parser.error(f"unexpected workspace revision for {source}: {revision}")
        changes = subprocess.check_output(
            [
                "git",
                "-C",
                str(source),
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
            text=True,
        )
        if changes:
            parser.error(f"workspace must have no local changes: {source}")

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

    fixtures = [
        (
            name if profile == "check" else f"{name}-Debug",
            suite / name,
            crate,
            profile,
            None,
        )
        for name, crate in RUSTC_PERF_FIXTURES.items()
        for profile in ("check", "dev")
    ] + [
        (
            name,
            workspaces / spec["directory"],
            spec["package"].replace("-", "_"),
            spec.get("profile", "check"),
            spec,
        )
        for name, spec in WORKSPACES.items()
    ]
    selected = selected_fixtures(args.shard)
    for name, source, crate_name, profile, workspace in fixtures:
        if name not in selected:
            continue
        cwd = output / "fixtures" / name
        shutil.copytree(source, cwd, ignore=shutil.ignore_patterns(".git"))
        # Force Cargo to invoke the leaf even if an external target directory was reused.
        leaf = cwd / "crates" / workspace["package"] if workspace else cwd
        (leaf / "src/lib.rs").touch()
        patch = cwd / "0-println.patch" if workspace is None else None

        # Otherwise Git discovers the enclosing rust repository and silently skips this patch.
        if patch is not None:
            patch_env = os.environ.copy()
            patch_env.pop("GIT_DIR", None)
            patch_env.pop("GIT_WORK_TREE", None)
            patch_env["GIT_CEILING_DIRECTORIES"] = str(cwd.parent)
            subprocess.run(
                ["git", "apply", "--check", str(patch)],
                cwd=cwd,
                env=patch_env,
                check=True,
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
        extra = ["--package", workspace["package"]] if workspace else []
        if workspace and workspace["features"]:
            extra.extend(["--features", ",".join(workspace["features"])])
        subprocess.run(
            [
                str(cargo),
                "rustc",
                "--locked",
                "--lib",
                "--profile",
                profile,
                *extra,
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
                "profile": profile,
                "source": str(source),
                "source_revision": workspace["revision"]
                if workspace
                else suite_revision,
                "incremental": str(output / "incremental" / name),
                "patch": str(patch) if patch is not None else None,
            }
        )

    manifest_path = output / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Prepared compiler benchmarks: {manifest_path}")


if __name__ == "__main__":
    main()
