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
    "inline_interners": ("d431f7e2d7c2cee45cea4dde567e37846b075969", 33967115972),
    "global_trait_proofs": ("b57fb8b21cfa45a10f51fc4a90f6495518f40458", 33967204643),
    "query_cache": ("c1c0df5ea61aa9567b08597983c3bc66d74f4e3c", 33967420811),
    "interner_tables": ("bff200000e19c2f7f7d9727aceff4b49805217c5", 33968145281),
    "inline_constructors": ("af4c4ee447316d08ed42da16d708286438701f71", 33968335687),
    "early_sized": ("7edf3a10d157b8aec91b643b4ba9153cb3487039", 33968214867),
    "packed_aliases": ("778457af4760eb79cd2b047d5a67a2729411ee4f", 33969983049),
    "interned_arenas": ("00b45135ef5d7be659351ce89c8b63595736380c", 33970652704),
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
        name, fixture, *, revision=None, next_solver=False, disable_fast_paths=False,
        errors=(), incremental=None, require_loaded=False, threads=None,
        crate_name=None, artifact=None, execute=True, extra_flags=(), required_spans=(),
        expected_stdout=None,
    ):
        """Compile one fixture and execute successful binary fixtures."""
        default_name = incremental.name if incremental is not None else name
        crate_name = crate_name or default_name.replace("-", "_")
        binary = artifact or output / "bin" / default_name
        if incremental is not None:
            incremental.mkdir(parents=True, exist_ok=True)
        command = [
            str(rustc),
            "--edition=2021",
            "--error-format=json",
            "-Zvalidate-mir",
            "-Zdeduplicate-diagnostics=no",
            "--crate-name",
            crate_name,
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
        if incremental is not None:
            command.extend([f"-Cincremental={incremental}", "-Zincremental-verify-ich"])
        if require_loaded:
            command.append("-Zassert-incr-state=loaded")
        if threads is not None:
            command.append(f"-Zthreads={threads}")
        command.extend(extra_flags)
        if errors and incremental is None:
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
            if required_spans:
                pending = [d for d in diagnostics if d.get("level") == "error"]
                spans = []
                while pending:
                    diagnostic = pending.pop()
                    pending.extend(diagnostic.get("children", ()))
                    for span in diagnostic.get("spans", ()):
                        if (
                            Path(span["file_name"]).name == Path(fixture).name
                            and span["line_start"] == span["line_end"]
                        ):
                            spans.append((
                                span["line_start"], span["column_start"], span["column_end"],
                                span["is_primary"], span["label"],
                            ))
                record["required_spans"] = required_spans
                record["actual_spans"] = spans
                assert all(span in spans for span in required_spans), (required_spans, spans)
            if not errors and execute:
                run = subprocess.run(
                    [str(binary)], cwd=source, env=env, capture_output=True,
                    text=True, timeout=60,
                )
                (output / f"{name}.run.stdout.txt").write_text(run.stdout)
                (output / f"{name}.run.stderr.txt").write_text(run.stderr)
                record["program_exit"] = run.returncode
                record["executed"] = True
                assert run.returncode == 0, run.stderr
                if expected_stdout is not None:
                    record["expected_stdout"] = expected_stdout
                    assert run.stdout == expected_stdout, (run.stdout, expected_stdout)
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
        if kind in ("copy_proofs", "trait_proofs", "global_trait_proofs"):
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
        if kind in ("trait_proofs", "global_trait_proofs"):
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
        if kind == "global_trait_proofs":
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"global-trait-proofs-{solver}", "tests/ui/ohm/global-trait-proofs.rs",
                    revision=solver, **controls,
                )
                lifetime_errors = (("E0521", "borrowed data escapes outside of function"),) * 2
                check(
                    f"global-trait-lifetimes-{solver}",
                    "tests/ui/ohm/global-trait-proofs-lifetimes.rs",
                    revision=solver, errors=lifetime_errors, **controls,
                )
                incremental = output / "incremental" / f"global-proof-cache-{solver}"
                check(
                    f"global-incremental-{solver}-bpass1",
                    "tests/incremental/mir-typeck-trait-proofs.rs",
                    revision="bpass1", incremental=incremental, **controls,
                )
                check(
                    f"global-incremental-{solver}-bfail2",
                    "tests/incremental/mir-typeck-trait-proofs.rs",
                    revision="bfail2", incremental=incremental, require_loaded=True,
                    errors=lifetime_errors, **controls,
                )
        if kind in ("inline_interners", "inline_constructors", "query_cache", "interner_tables"):
            for solver in ("current", "next"):
                next_solver = solver == "next"
                threads = 4 if kind in ("query_cache", "interner_tables") else None
                check(
                    f"nominal-obligations-{solver}",
                    "tests/ui/wf/nominal-inherited-obligations.rs",
                    revision=solver, next_solver=next_solver, threads=threads,
                )
                check(
                    f"nominal-obligations-fail-{solver}",
                    "tests/ui/wf/nominal-inherited-obligations-fail.rs",
                    revision=solver, next_solver=next_solver, threads=threads,
                    errors=(("E0277", "the trait bound `T: Copy` is not satisfied"),)
                    * (3 if next_solver else 2)
                    + (("E0277", "the trait bound `U: Clone` is not satisfied"),),
                )
                for suffix in ("", "-proj"):
                    check(
                        f"copy-lifetime{suffix}-{solver}",
                        f"tests/ui/nll/do-not-ignore-lifetime-bounds-in-copy{suffix}.rs",
                        next_solver=next_solver, threads=threads,
                        errors=(("E0597", "`s` does not live long enough"),),
                    )
        if kind in ("query_cache", "interner_tables", "packed_aliases", "interned_arenas"):
            thread_counts = (4,) if kind in ("packed_aliases", "interned_arenas") else (1, 4)
            for threads in thread_counts:
                rlib = output / "bin" / f"liba_threads_{threads}.rlib"
                auxiliary_cache = output / "incremental" / f"query-cache-aux-{threads}"
                main_cache = output / "incremental" / f"query-cache-main-{threads}"
                for revision in ("rpass1", "rpass2", "rpass3"):
                    check(
                        f"query-cache-aux-{threads}-{revision}",
                        "tests/incremental/rlib_cross_crate/auxiliary/a.rs",
                        revision=revision, threads=threads, crate_name="a", artifact=rlib,
                        execute=False, incremental=auxiliary_cache,
                        require_loaded=revision != "rpass1",
                        extra_flags=("-Zquery-dep-graph", "--crate-type=rlib"),
                    )
                    check(
                        f"query-cache-main-{threads}-{revision}",
                        "tests/incremental/rlib_cross_crate/b.rs",
                        revision=revision, threads=threads, incremental=main_cache,
                        require_loaded=revision != "rpass1",
                        extra_flags=("-Zquery-dep-graph", "--extern", f"a={rlib}"),
                    )
        if kind in ("packed_aliases", "interned_arenas"):
            for solver in ("current", "next"):
                next_solver = solver == "next"
                check(
                    f"packed-projection-{solver}",
                    "tests/ui/associated-types/projection-as-type-alias.rs",
                    next_solver=next_solver,
                )
                check(
                    f"packed-hrtb-{solver}",
                    "tests/ui/generic-associated-types/rigid-hr-projection-issue-93340.rs",
                    next_solver=next_solver, extra_flags=("--emit=metadata",), execute=False,
                )
                check(
                    f"packed-projection-fail-{solver}",
                    "tests/ui/associated-types/assoc-type-element-mismatch-in-hrtb.rs",
                    next_solver=next_solver,
                    errors=(("E0271", "type mismatch resolving `&() == ()`"
                             if next_solver else
                             "type mismatch resolving `<() as Array<'a>>::Element == ()`"),),
                )
                check(
                    f"packed-lifetimes-{solver}",
                    "tests/ui/generic-associated-types/projection-type-lifetime-mismatch.rs",
                    next_solver=next_solver,
                    errors=((None, "lifetime may not live long enough"),) * 3,
                )
                check(
                    f"packed-opaque-{solver}",
                    "tests/ui/impl-trait/normalize-opaque-with-bound-vars.rs",
                    next_solver=next_solver, extra_flags=("-Cdebuginfo=2",),
                )
                check(
                    f"packed-const-{solver}",
                    "tests/ui/const-generics/mgca/assoc-const-projection-in-bound.rs",
                    next_solver=next_solver,
                )
                check(
                    f"packed-const-fail-{solver}",
                    "tests/ui/const-generics/associated-const-bindings/const-projection-err.rs",
                    next_solver=next_solver,
                    errors=(("E0271", "type mismatch resolving `0 == 1`"
                             if next_solver else
                             "type mismatch resolving `<T as TraitWAssocConst>::A == 1`"),),
                )
                check(
                    f"packed-lazy-alias-{solver}",
                    "tests/ui/const-generics/adt_const_params/alias_const_param_ty-1.rs",
                    next_solver=next_solver, extra_flags=("--emit=metadata",), execute=False,
                )
                rlib = output / "bin" / f"libassociated_types_cc_lib_{solver}.rlib"
                check(
                    f"packed-metadata-aux-{solver}",
                    "tests/ui/associated-types/auxiliary/associated-types-cc-lib.rs",
                    next_solver=next_solver, threads=4, crate_name="associated_types_cc_lib",
                    artifact=rlib, extra_flags=("--crate-type=rlib",), execute=False,
                )
                check(
                    f"packed-metadata-main-{solver}",
                    "tests/ui/associated-types/associated-types-cc.rs",
                    next_solver=next_solver, threads=4,
                    extra_flags=("--extern", f"associated_types_cc_lib={rlib}"),
                    expected_stdout="22\n",
                )
        if kind == "copy_impl_schema":
            fixtures = Path(__file__).resolve().parent / "cases" / "copy_impl_schema"
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                for suffix in ("", "-proj"):
                    check(
                        f"schema-copy-lifetime{suffix}-{solver}",
                        f"tests/ui/nll/do-not-ignore-lifetime-bounds-in-copy{suffix}.rs",
                        errors=(("E0597", "`s` does not live long enough"),), **controls,
                    )
                rlib = output / "bin" / f"libschema_core_{solver}.rmeta"
                auxiliary_cache = output / "incremental" / f"schema-aux-{solver}"
                consumer_cache = output / "incremental" / f"schema-consumer-{solver}"
                for revision in ("unconditional", "restricted", "restored"):
                    check(
                        f"schema-incremental-aux-{solver}-{revision}",
                        str(fixtures / "custom_core.rs"),
                        revision="restricted" if revision == "restricted" else None,
                        crate_name="custom_core", artifact=rlib, execute=False,
                        incremental=auxiliary_cache, require_loaded=revision != "unconditional",
                        extra_flags=("--crate-type=rlib", "--emit=metadata"), **controls,
                    )
                    check(
                        f"schema-incremental-consumer-{solver}-{revision}",
                        str(fixtures / "consumer.rs"),
                        crate_name="schema_consumer", execute=False,
                        incremental=consumer_cache, require_loaded=revision != "unconditional",
                        extra_flags=("--crate-type=rlib", "--emit=metadata", "--extern", f"custom_core={rlib}"),
                        errors=((None, "lifetime may not live long enough"),)
                        if revision == "restricted" else (), **controls,
                    )
        if kind == "early_sized":
            for solver in ("current", "next", "slow"):
                options = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"early-sized-{solver}", "tests/ui/ohm/early-sized-impl-obligations.rs",
                    **options,
                )
                check(
                    f"early-sized-fail-{solver}",
                    "tests/ui/ohm/early-sized-impl-obligations-fail.rs",
                    errors=(("E0277", "the size for values of type `str` cannot be known at compilation time"),),
                    required_spans=(
                        (22, 13, 27, True, "doesn't have a size known at compile-time"),
                        (17, 55, 60, False, "unsatisfied trait bound introduced here"),
                        (19, 15, 20, True, "required by this bound in `require`"),
                    ), **options,
                )
                check(
                    f"early-sized-alias-{solver}", "tests/ui/sized-hierarchy/alias-bounds.rs",
                    extra_flags=("--crate-type=lib",), execute=False, **options,
                )
                check(
                    f"early-sized-tainted-{solver}",
                    "tests/ui/async-await/async-closures/dont-ice-when-body-tainted-by-errors.rs",
                    errors=(("E0277", "the trait bound `(): Impossible` is not satisfied"),),
                    **options,
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
