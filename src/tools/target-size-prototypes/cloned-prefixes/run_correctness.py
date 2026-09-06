"""Run clone admission and fallback checks with the built candidate compiler."""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path[:0] = [
    str(Path(__file__).resolve().parent),
    str(Path(__file__).resolve().parent.parent),
]
import validate_rust as common
from cloned_prefixes import COW

root = common.ROOT
source = root / "sources/cloned-prefixes"
driver = Path(__file__).resolve().parent
version = common.version("cloned-prefixes")
evidence = root / "prefix-correctness"
evidence.mkdir(exist_ok=False)
summary = {"compiler": version, "checks": []}
for filesystem, prefix, expect_cloning in [
    ("btrfs", COW / "correctness", True),
    ("ext4", common.BUILDS / "prefix-correctness", False),
]:
    for kind in ["query", "metadata"]:
        config = {
            "root": str(prefix / kind),
            "rustc": str(common.compiler("cloned-prefixes")),
            "rustc_sha256": version["sha256"],
            "expect_cloning": expect_cloning,
            "fixture_source": str(
                source
                / (
                    "tests/incremental/forward-cached-query-value-cloned.rs"
                    if kind == "query"
                    else "tests/run-make/cloned-metadata-prefix"
                )
            ),
        }
        configuration = evidence / f"{filesystem}-{kind}.json"
        configuration.write_text(json.dumps(config, indent=2) + "\n")
        script = driver / (
            "prefix_runtime.py" if kind == "query" else "metadata_runtime.py"
        )
        with (evidence / f"{filesystem}-{kind}.log").open("w") as log:
            process = subprocess.run(
                ["python3", str(script), str(configuration)],
                stdout=log,
                env=common.clean_env(),
                stderr=subprocess.STDOUT,
                check=False,
            )
        summary["checks"].append(
            {
                "filesystem": filesystem,
                "kind": kind,
                "returncode": process.returncode,
                "config": config,
            }
        )
        (evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        process.check_returncode()
        receipt = Path(config["root"]) / "receipt.json"
        (evidence / f"{filesystem}-{kind}-receipt.json").write_bytes(
            receipt.read_bytes()
        )

# Keep the unit tests' temporary files on the clone-capable filesystem. The
# direct runtime checks above separately require positive clone admission.
unit_temp = COW / "unit-temp"
unit_temp.mkdir(exist_ok=False)
filesystem = json.loads(
    subprocess.check_output(["findmnt", "-T", str(unit_temp), "-J"])
)
assert filesystem["filesystems"][0]["fstype"] == "btrfs", filesystem
bootstrap_env = dict(
    os.environ,
    CARGO_HOME="/tmp/codex-cargo-home",
    __CARGO_OHM_DEFAULTS="no",
    LD_LIBRARY_PATH=str(root / "llvm/lib"),
    TMPDIR=str(unit_temp),
)
for key in [
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_BUILD_DIR",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTUP_TOOLCHAIN",
    "RUSTC",
    "RUSTDOC",
    "RUSTC_FORCE_RUSTC_VERSION",
    "CARGO_MAKEFLAGS",
    "MAKEFLAGS",
    "MFLAGS",
]:
    bootstrap_env.pop(key, None)
command = [
    "python3",
    "x.py",
    "test",
    "--stage",
    "1",
    "-j",
    "8",
    "compiler/rustc_fs_util",
    "compiler/rustc_serialize",
    "tests/incremental/forward-cached-query-value-cloned.rs",
    "tests/run-make/cloned-metadata-prefix",
]
bootstrap = {
    "command": command,
    "cwd": str(source),
    "environment": {
        key: bootstrap_env[key]
        for key in ["CARGO_HOME", "__CARGO_OHM_DEFAULTS", "LD_LIBRARY_PATH", "TMPDIR"]
    },
    "temporary_filesystem": filesystem,
    "started": time.time(),
}
summary["bootstrap"] = bootstrap
(evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
with (evidence / "bootstrap-focused.log").open("w") as log:
    process = subprocess.run(
        command,
        cwd=source,
        env=bootstrap_env,
        stdout=log,
        stderr=subprocess.STDOUT,
        check=False,
    )
bootstrap.update(returncode=process.returncode, finished=time.time())
(evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
process.check_returncode()
print(json.dumps(summary, indent=2))
