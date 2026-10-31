"""Test measurement isolation/accounting with fake compiler processes, not Rust workloads."""

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "sparse_metadata", Path(__file__).with_name("sparse_metadata.py")
)
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class MeasurementTests(unittest.TestCase):
    def test_target_hardlinks_and_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "first.rlib"
            artifact.write_bytes(b"x" * 5000)
            os.link(artifact, root / "second.rlib")
            (root / "alias").symlink_to("first.rlib")
            cache = root / "debug/incremental"
            cache.mkdir(parents=True)
            (cache / "query-cache.bin").write_bytes(b"cache")
            deps = root / "debug/deps"
            deps.mkdir()
            metadata = deps / "library.rmeta"
            metadata.write_bytes(b"metadata")
            os.link(metadata, cache / "metadata.rmeta")
            result = driver.target_inventory(root)
            libraries = result["categories"][".rlib"]
            self.assertEqual(libraries["apparent_bytes"], 10000)
            self.assertEqual(libraries["unique_bytes"], 5000)
            self.assertEqual(
                libraries["allocated_bytes"], artifact.stat().st_blocks * 512
            )
            self.assertEqual(result["categories"]["symlinks"]["entries"], 1)
            self.assertEqual(result["incremental_totals"]["unique_bytes"], 13)
            self.assertEqual(
                result["incremental_totals"]["allocated_bytes"],
                sum(path.stat().st_blocks * 512 for path in [cache, *cache.iterdir()]),
            )
            self.assertEqual(result["totals"]["unique_bytes"], 5013 + len("first.rlib"))

    def test_paired_instruction_isolation_and_launch_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence = root / "test"
            evidence.mkdir()
            compiler = (
                f"#!{sys.executable}\n"
                + """
import sys, time
from pathlib import Path
output = Path(sys.argv[sys.argv.index('--out-dir') + 1])
time.sleep(0.2)
(output / 'test.rmeta').write_bytes(b'metadata')
"""
            )
            valgrind = root / "fake-valgrind"
            valgrind.write_text(
                f"#!{sys.executable}\n"
                + """
import subprocess, sys
from pathlib import Path
if sys.argv[1:] == ['--version']:
    print('fake Cachegrind for harness test')
else:
    args = sys.argv[1:]
    profile = next(arg.split('=', 1)[1] for arg in args if arg.startswith('--cachegrind-out-file='))
    first = next(i for i, arg in enumerate(args) if not arg.startswith('--'))
    subprocess.run(args[first:], check=True)
    Path(profile).write_text('events: Ir\\nsummary: 1\\n')
"""
            )
            valgrind.chmod(0o755)
            manifest = {"complete": True, "arms": {}}
            for arm in driver.ARMS:
                sysroot = root / arm
                (sysroot / "bin").mkdir(parents=True)
                rustc = sysroot / "bin/rustc"
                rustc.write_text(compiler)
                rustc.chmod(0o755)
                output = evidence / "outputs" / arm
                output.mkdir(parents=True)
                manifest["arms"][arm] = {
                    "sysroot": str(sysroot),
                    "rustc_sha256": driver.digest(rustc),
                    "driver_sha256": {},
                    "cases": {
                        "syn": {
                            "command": [str(rustc), "--out-dir", str(output)],
                            "env": {},
                            "cwd": str(root),
                            "outputs": str(output),
                        }
                    },
                }
            driver.save(evidence / "manifest.json", manifest)
            args = SimpleNamespace(
                root=root,
                experiment="test",
                operation="instructions",
                paired_instructions=True,
                allow_concurrent_instructions=True,
                cases=["syn"],
                cycles=1,
                instruction_samples=2,
                valgrind=str(valgrind),
                cargo_home=root / "cargo-home",
            )
            read_text = Path.read_text

            def read(path, *args, **kwargs):
                if str(path) == "/proc/cpuinfo":
                    return "test cpu"
                return read_text(path, *args, **kwargs)

            with (
                patch.object(driver, "quiet", return_value={"harness_test": True}),
                patch.object(os, "sched_getaffinity", return_value={0}, create=True),
                patch.object(Path, "read_text", read),
            ):
                driver.measure(args)
            rows = json.loads((evidence / "instructions/samples.json").read_text())
            environment = json.loads(
                (evidence / "instructions/environment.json").read_text()
            )
            driver.validate_samples(rows, environment, "instructions")
            for incomplete in (rows[:-1], rows[:4], rows + [rows[-1]]):
                with self.assertRaisesRegex(ValueError, "Incomplete or duplicated"):
                    driver.validate_samples(incomplete, environment, "instructions")
            invalid_plan = dict(environment, cycles=2)
            with self.assertRaisesRegex(ValueError, "Incomplete or duplicated"):
                driver.validate_samples(rows, invalid_plan, "instructions")
            self.assertTrue(
                all(
                    row["instrumentation"]
                    == ("native" if row["phase"] == "warmup" else "cachegrind")
                    for row in rows
                )
            )
            measured = sorted(
                (row for row in rows if row["phase"] == "measured"),
                key=lambda row: row["order"],
            )
            self.assertEqual([row["arm"] for row in measured], list(driver.ORDERS[:4]))
            self.assertEqual([row["concurrent_pair"] for row in measured], [0, 0, 1, 1])
            self.assertEqual(len({row["log"] for row in rows}), 6)
            for first, second in (measured[:2], measured[2:]):
                self.assertLessEqual(first["started"], second["started"])
                self.assertLess(
                    second["started"], first["started"] + first["elapsed_ns"] / 1e9
                )
            self.assertGreaterEqual(
                measured[2]["started"],
                max(row["started"] + row["elapsed_ns"] / 1e9 for row in measured[:2]),
            )
            for row in measured:
                self.assertEqual(row["metadata"][0]["bytes"], 8)


if __name__ == "__main__":
    unittest.main()
