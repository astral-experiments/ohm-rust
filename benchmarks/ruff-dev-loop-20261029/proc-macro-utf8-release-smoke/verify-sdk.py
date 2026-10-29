"""Run the UTF-8 proc-macro regression against the installed Ohm SDK."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(work, expectation):
    work.mkdir(mode=0o700)
    source = work / "source"
    shutil.copytree(Path(__file__).resolve().parent / "fixture", source)
    env = {k: os.environ[k] for k in ("PATH", "HOME", "USER", "LOGNAME", "RUSTUP_HOME", "DEVELOPER_DIR", "SDKROOT") if k in os.environ}
    env.update(CARGO_HOME=str(work / "cargo-home"), CARGO_NET_OFFLINE="true",
               CARGO_TERM_COLOR="never", RUSTUP_AUTO_INSTALL="0", LC_ALL="C")
    (work / "logs").mkdir()
    toolchain = {}
    for tool in ("cargo", "rustc"):
        toolchain[tool] = subprocess.check_output([tool, "+ohm", "-vV"], env=env, text=True)
    sdk = Path(subprocess.check_output(["rustc", "+ohm", "--print", "sysroot"], env=env, text=True).strip())
    for tool in ("cargo", "rustc"):
        toolchain[tool + "_sha256"] = digest(sdk / "bin" / tool)
    drivers = [path for path in (sdk / "lib").glob("*rustc_driver*") if path.is_file()]
    if len(drivers) == 1:
        toolchain["driver_sha256"] = digest(drivers[0])
    (work / "toolchain.json").write_text(json.dumps(toolchain, indent=2) + "\n")

    results = []
    for mode, flags in (("default", []), ("defaults-off", ["-Zohm-defaults=no"])):
        env.update(CARGO_TARGET_DIR=str(work / mode / "target"), CARGO_BUILD_BUILD_DIR=str(work / mode / "shared"))
        expect_panic = expectation == "affected" and mode == "default"
        for command in (["check"], ["build"], ["test", "--no-run"]):
            for attempt in ("first", "repeat"):
                label = f"{mode}-{command[0]}-{attempt}"
                args = ["cargo", "+ohm", *flags, *command, "--locked", "--offline", "-p", "span_probe_app",
                        "-j2", "-vv", "--message-format=json"]
                proc = subprocess.run(args, cwd=source, env=env, capture_output=True, timeout=180)
                for stream in ("stdout", "stderr"):
                    (work / "logs" / f"{label}.{stream}").write_bytes(getattr(proc, stream))
                messages = [json.loads(line) for line in proc.stdout.splitlines()]
                artifacts = [item for item in messages if item.get("reason") == "compiler-artifact"]
                panic = b"char boundary" in proc.stderr and b"public_api_hasher.rs" in proc.stderr
                outcome = {"case": label, "exit": proc.returncode, "utf8_panic": panic,
                           "dirty": [item["target"]["name"] for item in artifacts if not item["fresh"]]}
                results.append(outcome)
                print(label, json.dumps(outcome), flush=True)
                (work / "results.json").write_text(json.dumps(results, indent=2) + "\n")
                assert messages[-1] == {"reason": "build-finished", "success": not expect_panic}, label
                assert proc.returncode == (101 if expect_panic else 0) and panic == expect_panic, label
                if expect_panic:
                    continue
                assert {item["target"]["name"] for item in artifacts} == {"subspan", "span_lib", "span_probe_app"}, label
                if attempt == "repeat" and command[0] in ("check", "build"):
                    assert not outcome["dirty"], label
                if command[0] in ("build", "test"):
                    exe, = {item["executable"] for item in artifacts if item["executable"]}
                    extra = ["--list"] if command[0] == "test" else []
                    output = subprocess.check_output([exe, *extra], cwd=source, env=env, timeout=30)
                    if extra:
                        assert b"0 tests, 0 benchmarks" in output, output
                    else:
                        assert output == b"", output
                    outcome["executable_sha256"] = digest(Path(exe))
    (work / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"Passed {len(results)} Cargo invocations. Raw output is in {work}.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True, type=Path, help="new private output directory")
    parser.add_argument("--expect", choices=("fixed", "affected"), default="fixed")
    options = parser.parse_args()
    if not __debug__:
        parser.error("run without Python optimization so validation assertions execute")
    run(options.work.resolve(), options.expect)
