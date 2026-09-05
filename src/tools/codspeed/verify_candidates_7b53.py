"""Validate the fixed measured compilers without rebuilding either compiler."""

from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys


CANDIDATES = {
    "folds": ("d9b0fecdbe558ff6a1022982ff68e5faf04c98b0", 33964529181),
    "mir": ("861d55a0be872c0b47ea71e2fac044420e8f0198", 33964702894),
    "instantiation": ("b5e4d14dc9797f71a49561906d0a9cda53ac1842", 33965090737),
    "combined": ("b9bcf51550f7be847c41d9e76547c253c368539b", 33965430528),
    "copy_proofs": ("23ebc5cf12881b1cb27984d06e78a8eb6cea8c39", 33965794444),
    "query_response": ("d8cd3b4fc6298ef1e1cb9091c88fb10fdc09e8e7", 33965881981),
    "trait_proofs": ("9df07251e44f9fba1f4fbfabe04f545bea7138f2", 33966299348),
}


def main():
    kind, source, rustc, output = sys.argv[1:]
    expected_sha, artifact_run = CANDIDATES[kind]
    source, rustc, output = (Path(path).resolve() for path in (source, rustc, output))
    output.mkdir(parents=True, exist_ok=True)
    (output / "bin").mkdir(exist_ok=True)
    env = dict(os.environ, RUSTC_BOOTSTRAP="1")
    report = {
        "kind": kind,
        "expected_sha": expected_sha,
        "artifact_run": artifact_run,
        "cases": [],
    }

    def check(
        name, fixture, *, revision=None, next_solver=False, disable_fast_paths=False, errors=()
    ):
        """Compile one explicit fixture; successful fixtures must also execute."""
        binary = output / "bin" / name
        command = [
            str(rustc),
            "--edition=2021",
            "--error-format=json",
            "-Zvalidate-mir",
            "-Zdeduplicate-diagnostics=no",
            "--crate-name",
            name.replace("-", "_"),
            fixture,
            "-o",
            str(binary),
        ]
        if revision:
            command.extend(["--cfg", revision])
        if next_solver:
            command.append("-Znext-solver=globally")
        if disable_fast_paths:
            command.append("-Zdisable-fast-paths")
        if errors:
            command.append("--emit=metadata")
        record = {
            "name": name,
            "command": command,
            "expected_errors": errors,
            "executed": False,
        }
        report["cases"].append(record)
        try:
            result = subprocess.run(
                command, cwd=source, env=env, capture_output=True, text=True, timeout=120
            )
            (output / f"{name}.jsonl").write_text(result.stderr)
            (output / f"{name}.stdout.txt").write_text(result.stdout)
            record["compiler_exit"] = result.returncode
            assert result.returncode == (1 if errors else 0), result.stderr
            combined = (result.stdout + result.stderr).lower()
            assert not any(
                marker in combined
                for marker in (
                    "internal compiler error",
                    "the compiler unexpectedly panicked",
                    "panicked at",
                )
            ), result.stderr
            diagnostics = [json.loads(line) for line in result.stderr.splitlines()]
            actual = [
                (diagnostic["code"]["code"] if diagnostic.get("code") else None,
                 diagnostic["message"])
                for diagnostic in diagnostics
                if diagnostic.get("level") == "error"
                and not diagnostic["message"].startswith("aborting due to ")
            ]
            record["actual_errors"] = actual
            assert Counter(actual) == Counter(errors), (actual, errors)
            if not errors:
                run = subprocess.run(
                    [str(binary)], cwd=source, env=env, capture_output=True,
                    text=True, timeout=60,
                )
                (output / f"{name}.run.stdout.txt").write_text(run.stdout)
                (output / f"{name}.run.stderr.txt").write_text(run.stderr)
                record["program_exit"] = run.returncode
                record["executed"] = True
                assert run.returncode == 0, run.stderr
            record["status"] = "passed"
        except Exception as error:
            record["status"] = "failed"
            record["failure"] = str(error)
        print(f"{record['status']}: {name}", flush=True)

    try:
        actual_sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=source, text=True
        ).strip()
        report["source_sha"] = actual_sha
        assert actual_sha == expected_sha, (actual_sha, expected_sha)
        architecture = subprocess.check_output(["uname", "-m"], text=True).strip()
        report["runner_architecture"] = architecture
        (output / "runner-architecture.txt").write_text(architecture + "\n")
        version = subprocess.run(
            [str(rustc), "-vV"], capture_output=True, text=True, timeout=30
        )
        (output / "rustc-version.txt").write_text(version.stdout + version.stderr)
        assert version.returncode == 0, version.stderr
        assert architecture == "aarch64", architecture
        assert "host: aarch64-unknown-linux-gnu" in version.stdout.splitlines(), version.stdout
        assert f"commit-hash: {expected_sha}" in version.stdout.splitlines(), version.stdout
        report["compiler_sha_verified"] = True

        if kind in ("folds", "combined"):
            for solver in ("current", "next"):
                check(
                    f"canonical-{solver}", "tests/ui/ohm/repeated-canonical-types.rs",
                    revision=solver, next_solver=solver == "next",
                )
                check(
                    f"wf-bounds-{solver}", "tests/ui/ohm/repeated-wf-bounds.rs",
                    revision=solver, next_solver=solver == "next",
                    errors=(
                        ("E0277", "the trait bound `T: Copy` is not satisfied"),
                        (None, "lifetime may not live long enough"),
                    ),
                )
        if kind in ("mir", "combined"):
            for revision in ("current", "next", "params", "params_next"):
                check(
                    f"sized-places-{revision}", "tests/ui/ohm/mir-typeck-sized-places.rs",
                    revision=revision, next_solver=revision in ("next", "params_next"),
                )
            for solver in ("current", "next"):
                next_solver = solver == "next"
                for suffix in ("", "-proj"):
                    check(
                        f"copy-lifetime{suffix}-{solver}",
                        f"tests/ui/nll/do-not-ignore-lifetime-bounds-in-copy{suffix}.rs",
                        next_solver=next_solver,
                        errors=(("E0597", "`s` does not live long enough"),),
                    )
                check(
                    f"unsized-fail-{solver}", "tests/ui/unsized-locals/unsized-exprs.rs",
                    next_solver=next_solver,
                    errors=(("E0277", "the size for values of type `[u8]` cannot be known at compilation time"),) * 2,
                )
                check(
                    f"unsized-pass-{solver}",
                    "tests/ui/unsized-locals/unsized-exprs-rpass.rs",
                    next_solver=next_solver,
                )
                check(
                    f"unsized-projection-pass-{solver}",
                    "tests/ui/traits/trivial-unsized-projection.rs",
                    revision="good_new" if next_solver else "good",
                    next_solver=next_solver,
                )
                sized_error = (
                    "E0277", "the size for values of type `[()]` cannot be known at compilation time"
                )
                mismatch_error = (
                    "E0271", "type mismatch resolving `<[()] as Bad>::Assert == _`"
                )
                check(
                    f"unsized-projection-fail-{solver}",
                    "tests/ui/traits/trivial-unsized-projection.rs",
                    revision="bad_new" if next_solver else "bad",
                    next_solver=next_solver,
                    errors=(sized_error, mismatch_error, mismatch_error)
                    if next_solver else (sized_error, sized_error),
                )
        if kind in ("instantiation", "combined"):
            for solver in ("current", "next"):
                check(
                    f"instantiation-{solver}", "tests/ui/ohm/type-instantiation-cache.rs",
                    revision=solver, next_solver=solver == "next",
                )
                check(
                    f"instantiation-lifetimes-{solver}",
                    "tests/ui/ohm/type-instantiation-cache-lifetimes.rs",
                    revision=solver, next_solver=solver == "next",
                    errors=((None, "lifetime may not live long enough"),),
                )
        if kind in ("copy_proofs", "trait_proofs"):
            for solver in ("current", "next", "slow"):
                options = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"copy-proofs-{solver}", "tests/ui/ohm/copy-proofs-repeated.rs",
                    **options,
                )
                check(
                    f"copy-region-bound-{solver}", "tests/ui/ohm/copy-proofs-region-bound.rs",
                    errors=(("E0597", "`value` does not live long enough"),), **options,
                )
                check(
                    f"copy-no-core-{solver}", "tests/ui/ohm/copy-proofs-no-core.rs",
                    errors=((None, "lifetime may not live long enough"),), **options,
                )
                for suffix in ("", "-proj"):
                    check(
                        f"copy-lifetime{suffix}-{solver}",
                        f"tests/ui/nll/do-not-ignore-lifetime-bounds-in-copy{suffix}.rs",
                        errors=(("E0597", "`s` does not live long enough"),), **options,
                    )
        if kind == "trait_proofs":
            for solver in ("current", "next", "slow"):
                options = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"trait-proofs-{solver}", "tests/ui/ohm/trait-proofs-repeated.rs",
                    **options,
                )
                check(
                    f"trait-region-bound-{solver}", "tests/ui/ohm/trait-proofs-region-bound.rs",
                    errors=(("E0597", "`value` does not live long enough"),), **options,
                )
        if kind == "query_response":
            for solver in ("current", "next", "slow"):
                options = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"query-response-{solver}", "tests/ui/ohm/query-response-identity.rs",
                    **options,
                )
                check(
                    f"query-response-lifetimes-{solver}",
                    "tests/ui/ohm/query-response-lifetime-effects.rs",
                    errors=(
                        ("E0597", "`value` does not live long enough"),
                        (None, "lifetime may not live long enough"),
                    ), **options,
                )
        failed = [case["name"] for case in report["cases"] if case["status"] != "passed"]
        assert not failed, f"Failed cases: {', '.join(failed)}"
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["failure"] = str(error)
    finally:
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        lines = [
            f"{kind}: {report['status']}",
            f"Expected compiler and source: {expected_sha}",
            f"Artifact run: {artifact_run}",
            *[f"{case['status']}: {case['name']} (executed={case['executed']})"
              for case in report["cases"]],
        ]
        if "failure" in report:
            lines.append(f"Failure: {report['failure']}")
        summary = "\n".join(lines) + "\n"
        (output / "summary.txt").write_text(summary)
        print(summary, flush=True)
        if step_summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(step_summary, "a") as stream:
                stream.write(f"```text\n{summary}```\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
