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
    "copy_impl_schema": ("db029ae942064533f669fd3d78caff31d5aa1443", 33970806995),
    "lazy_copy": ("6e7ba93e7fa550912fb96e1b602dd29567edd809", 33971290579),
    "nightly_hasher": ("9bd0045de920bbae10d45b9045945d5a1137321e", 33971229116),
    "dense_interners": ("ea4e9cc5343eb98dc3d42d0099da8b8416145471", 33971439882),
    "thin_stalled": ("fe7c0d0c1d1a0dfae692641b020ed214364142b5", 33971705326),
    "nested_argfolder": ("8a6a1c5dba9e4136f37e53f58634b41dae59adc3", 33971787859),
    "copy_adt": ("284f7a09c013ce6fac94e023861309f4f3798ea1", 33972891567),
    "wf_adt": ("a62dbad7aa17c1f4239e081218090a12f0c33291", 33973221790),
    "trait_impl_schema": ("f4655021ef46126bed6893d0a5de8aa87bbfc9fe", 33973544227),
    "interner_cache": ("c320a2b6ad359b0e8a36d694780a25415b7a0360", 33973762897),
    "wf_nominal": ("33036e29a96f566733bc26bc3ba12ed0c13400f8", 33973833734),
    "hir_trait_proofs": ("7442c4ca9ea86b089bc36616bfbb873908c34f9c", 33974598690),
    "wf_reference": ("928d323cc522b25aa52ed0cb9a23466df18c9f87", 33975415242),
    "local_wf": ("5bcf3b8256639ecfb531de1dc3873721e197a7eb", 33976115645),
    "trivial_wf_batch": ("4354e5759a1b3202ded63c71eff9788675dc10e3", 33976263001),
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
        if kind in ("query_cache", "interner_tables", "packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "nested_argfolder", "interner_cache"):
            thread_counts = (4,) if kind in ("packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "nested_argfolder", "interner_cache") else (1, 4)
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
        if kind in ("packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "nested_argfolder", "interner_cache"):
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
                    errors=(("E0271", "type mismatch resolving `<() as Array<'a>>::Element == ()`"),),
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
                    errors=(("E0271", "type mismatch resolving `<T as TraitWAssocConst>::A == 1`"),),
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
        if kind == "thin_stalled":
            fixtures = (
                ("probe", "tests/ui/inference/issue-36053.rs", True, ()),
                ("infer-types", "tests/ui/traits/multidispatch-infer-convert-target.rs", True, ()),
                ("ambiguous-types", "tests/ui/traits/multidispatch-convert-ambig-dest.rs", False,
                 (("E0283", "type annotations needed"),)),
                ("infer-const", "tests/ui/const-generics/associated-const-bindings/equality_bound_with_infer.rs", False, ()),
                ("ambiguous-const", "tests/ui/const-generics/infer/one-param-uninferred.rs", False,
                 (("E0284", "type annotations needed"),)),
                ("coerce", "tests/ui/inference/iter-ufcs-coercion-type-unification.rs", False, ()),
                ("subtype", "tests/ui/inference/type-infer-generalize-ty-var.rs", False, ()),
                ("projection", "tests/ui/typeck/project-cache-issue-37154.rs", True, ()),
                ("lifetimes", "tests/ui/nll/do-not-ignore-lifetime-bounds-in-copy.rs", False,
                 (("E0597", "`s` does not live long enough"),)),
            )
            for solver in ("current", "next", "slow"):
                for name, fixture, execute, errors in fixtures:
                    check(
                        f"thin-stalled-{name}-{solver}", fixture,
                        next_solver=solver == "next", disable_fast_paths=solver == "slow",
                        execute=execute, errors=errors,
                        extra_flags=() if execute or errors else ("--emit=metadata",),
                    )
        if kind == "nested_argfolder":
            for solver in ("current", "next"):
                check(
                    f"nested-argfolder-{solver}",
                    "tests/ui/ohm/nested-argfolder-cache.rs", revision=solver,
                    next_solver=solver == "next",
                )
                check(
                    f"nested-argfolder-lifetimes-{solver}",
                    "tests/ui/ohm/nested-argfolder-cache-lifetimes.rs", revision=solver,
                    next_solver=solver == "next",
                    errors=((None, "lifetime may not live long enough"),),
                )
        if kind in ("copy_impl_schema", "lazy_copy", "copy_adt", "trait_impl_schema", "hir_trait_proofs"):
            fixtures = Path(__file__).resolve().parent / "cases" / "copy_impl_schema"
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"schema-pointers-{solver}",
                    "tests/ui/ohm/copy-impl-schema-pointers.rs", revision=solver, **controls,
                )
                check(
                    f"schema-param-env-{solver}",
                    "tests/ui/ohm/copy-impl-schema-param-env.rs", revision=solver,
                    errors=((None, "lifetime may not live long enough"),) * 2, **controls,
                )
                custom_errors = {
                    "lifetime": (None, "lifetime may not live long enough"),
                    "pointee": ("E0277", "the trait bound `NoGate: Gate` is not satisfied"
                                if solver == "next" else
                                "the trait bound `&NoGate: Copy` is not satisfied"),
                    "specialized": ("E0277", "the trait bound `&u16: Copy` is not satisfied"),
                    "missing": ("E0277", "the trait bound `&u8: Copy` is not satisfied"),
                }
                for case, error in custom_errors.items():
                    crate = f"copy_impl_schema_{case}_core"
                    rlib = output / "bin" / f"lib{crate}_{solver}.rmeta"
                    check(
                        f"schema-custom-{case}-aux-{solver}",
                        f"tests/ui/ohm/auxiliary/{crate}.rs",
                        revision=solver, crate_name=crate, artifact=rlib, execute=False,
                        extra_flags=("--crate-type=rlib", "--emit=metadata"), **controls,
                    )
                    check(
                        f"schema-custom-{case}-{solver}",
                        f"tests/ui/ohm/copy-impl-schema-custom-{case}.rs",
                        revision=solver, errors=(error,),
                        extra_flags=("--extern", f"{crate}={rlib}"), **controls,
                    )
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
        if kind in ("copy_adt", "trait_impl_schema", "hir_trait_proofs"):
            failures = {
                "lifetimes": ((None, "lifetime may not live long enough"),) * 6,
                "params": (("E0507", "cannot move out of `*value` which is behind a shared reference"),) * 3,
                "invalid": (
                    ("E0204", "the trait `Copy` cannot be implemented for this type"),
                    ("E0184", "the trait `Copy` cannot be implemented for this type; the type has a destructor"),
                ),
                "missing-clone": (("E0277", "the trait bound `MissingClone: Clone` is not satisfied"),),
                "no-core-invalid": (("E0277", "the trait bound `MissingSupertrait: Required` is not satisfied"),),
            }
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"schema-adts-{solver}", "tests/ui/ohm/copy-impl-schema-adts.rs",
                    revision=solver, **controls,
                )
                check(
                    f"schema-adts-no-core-{solver}",
                    "tests/ui/ohm/copy-impl-schema-adts-no-core.rs", revision=solver,
                    execute=False, extra_flags=("--emit=metadata",), **controls,
                )
                for case, errors in failures.items():
                    check(
                        f"schema-adts-{case}-{solver}",
                        f"tests/ui/ohm/copy-impl-schema-adts-{case}.rs", revision=solver,
                        errors=errors, **controls,
                    )
                cache = output / "incremental" / f"schema-local-{solver}"
                for phase, revision, errors in (
                    ("unconditional", "bpass1", ()),
                    ("restricted", "bfail2", ((None, "lifetime may not live long enough"),) * 2),
                    ("removed", "removed", (("E0507", "cannot move out of `*value` which is behind a shared reference"),) * 2),
                    ("restored", "bpass1", ()),
                ):
                    check(
                        f"schema-local-{solver}-{phase}",
                        "tests/incremental/copy-impl-schema-local.rs", revision=revision,
                        incremental=cache, require_loaded=phase != "unconditional",
                        execute=False, extra_flags=("--emit=metadata",), errors=errors,
                        **controls,
                    )
        if kind in ("trait_impl_schema", "hir_trait_proofs"):
            failures = {
                "arguments": (
                    (None, "lifetime may not live long enough"),
                    (None, "lifetime may not live long enough"),
                    ("E0277", "the trait bound `Unit: ConstArg<2>` is not satisfied"),
                ),
                "const-effect": (("E0277", "the trait bound `Ordinary: const Value` is not satisfied"),),
                "lifetimes": (
                    ("E0521", "borrowed data escapes outside of function"),
                    ("E0521", "borrowed data escapes outside of function"),
                    (None, "lifetime may not live long enough"),
                    (None, "lifetime may not live long enough"),
                ),
                "param-env": ((None, "lifetime may not live long enough"),) * 3,
            }
            fixtures = Path(__file__).resolve().parent / "cases" / "trait_impl_schema"
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                for case in ("basics", "fallbacks"):
                    check(
                        f"trait-schema-{case}-{solver}",
                        f"tests/ui/ohm/trait-impl-schema-{case}.rs", revision=solver,
                        **controls,
                    )
                for case, errors in failures.items():
                    check(
                        f"trait-schema-{case}-{solver}",
                        f"tests/ui/ohm/trait-impl-schema-{case}.rs", revision=solver,
                        errors=errors, **controls,
                    )
                rlib = output / "bin" / f"libproof_traits_{solver}.rmeta"
                auxiliary_cache = output / "incremental" / f"trait-schema-aux-{solver}"
                consumer_cache = output / "incremental" / f"trait-schema-consumer-{solver}"
                for phase, revision, errors in (
                    ("unconditional", "unconditional", ()),
                    ("restricted", "restricted", ((None, "lifetime may not live long enough"),)),
                    ("removed", "removed", (("E0277", "the trait bound `Value<'a>: Changing` is not satisfied"),)),
                    ("restored", "unconditional", ()),
                ):
                    check(
                        f"trait-schema-incremental-aux-{solver}-{phase}",
                        str(fixtures / "proof_traits.rs"), revision=revision,
                        crate_name="proof_traits", artifact=rlib, execute=False,
                        incremental=auxiliary_cache, require_loaded=phase != "unconditional",
                        extra_flags=("--crate-type=rlib", "--emit=metadata"), **controls,
                    )
                    check(
                        f"trait-schema-incremental-consumer-{solver}-{phase}",
                        str(fixtures / "consumer.rs"),
                        crate_name="trait_schema_consumer", execute=False,
                        incremental=consumer_cache, require_loaded=phase != "unconditional",
                        extra_flags=("--crate-type=rlib", "--emit=metadata", "--extern", f"proof_traits={rlib}"),
                        errors=errors, **controls,
                    )
        if kind == "hir_trait_proofs":
            for solver in ("current", "next", "slow"):
                controls = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                for case in ("basics", "contexts"):
                    check(
                        f"hir-trait-proofs-{case}-{solver}",
                        f"tests/ui/ohm/hir-trait-proofs-{case}.rs", **controls,
                    )
                check(
                    f"hir-trait-proofs-depth-limit-{solver}",
                    "tests/ui/ohm/hir-trait-proofs-depth-limit.rs", execute=False,
                    extra_flags=("--emit=metadata",), **controls,
                )
                check(
                    f"hir-trait-proofs-depth-overflow-{solver}",
                    "tests/ui/ohm/hir-trait-proofs-depth-overflow.rs",
                    errors=(("E0275", "overflow evaluating the requirement " +
                             ("`Layer<Layer<Layer<Leaf<'a>>>>: Ordinary`" if solver == "next"
                              else "`Leaf<'a>: Ordinary`")),),
                    required_spans=((17, 15, 44, True, None),) if solver == "next"
                    else ((17, 5, 47, True, None),), **controls,
                )
                check(
                    f"hir-trait-proofs-invalid-impls-{solver}",
                    "tests/ui/ohm/hir-trait-proofs-invalid-impls.rs",
                    errors=(
                        ("E0204", "the trait `Copy` cannot be implemented for this type"),
                        ("E0277", "the trait bound `MissingSupertrait: Super` is not satisfied"),
                        ("E0119", "conflicting implementations of trait `Coherent` for type `Duplicate`"),
                    ), **controls,
                )
        if kind in ("wf_adt", "wf_nominal", "wf_reference"):
            for solver in ("current", "next", "slow"):
                controls = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(f"wf-adts-{solver}", "tests/ui/ohm/wf-adt.rs", **controls)
                check(
                    f"wf-adts-lifetimes-{solver}", "tests/ui/ohm/wf-adt-lifetimes.rs",
                    errors=(
                        (None, "lifetime may not live long enough"),
                        (None, "lifetime may not live long enough"),
                        ("E0309", "the parameter type `T` may not live long enough"),
                    ), **controls,
                )
                check(
                    f"wf-adts-higher-ranked-{solver}", "tests/ui/ohm/wf-adt-higher-ranked.rs",
                    errors=(("E0277", "expected an `FnOnce(Local<'a>)` closure, found `fn(Local<'static>) {only_static}`"),)
                    if solver == "next" else ((None, "implementation of `FnOnce` is not general enough"),) * 2,
                    **controls,
                )
                check(
                    f"wf-adts-params-{solver}", "tests/ui/ohm/wf-adt-params.rs",
                    errors=(
                        ("E0277", "the size for values of type `str` cannot be known at compilation time"),
                        (None, "the constant `N` is not of type `usize`"),
                    ) * (2 if solver == "next" else 1), **controls,
                )
                check(
                    f"wf-adts-invalid-{solver}", "tests/ui/ohm/wf-adt-invalid.rs",
                    errors=(("E0072", "recursive type `Invalid` has infinite size"),),
                    **controls,
                )
        if kind in ("wf_nominal", "wf_reference"):
            for solver in ("current", "next", "slow"):
                controls = {
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(
                    f"wf-nominal-{solver}", "tests/ui/ohm/wf-nominal.rs",
                    revision=solver, **controls,
                )
                check(
                    f"wf-nominal-lifetimes-{solver}", "tests/ui/ohm/wf-nominal-lifetimes.rs",
                    revision=solver,
                    errors=(
                        ("E0309", "the parameter type `T` may not live long enough"),
                        (None, "lifetime may not live long enough"),
                    ), **controls,
                )
                check(
                    f"wf-nominal-restrictions-{solver}", "tests/ui/ohm/wf-nominal-restrictions.rs",
                    revision=solver,
                    errors=(
                        ("E0277", "the trait bound `NotClone: Clone` is not satisfied"),
                        (None, "the constant `N` is not of type `usize`"),
                        ("E0277", "the trait bound `<T as Associated>::Output: Clone` is not satisfied"),
                    ) * (2 if solver == "next" else 1), **controls,
                )
                cache = output / "incremental" / f"wf-nominal-plan-{solver}"
                for revision in ("bpass1", "bfail2"):
                    check(
                        f"wf-nominal-plan-{solver}-{revision}",
                        "tests/incremental/wf-nominal-plan.rs", revision=revision,
                        incremental=cache, require_loaded=revision == "bfail2",
                        execute=False, extra_flags=("--emit=metadata",),
                        errors=((None, "lifetime may not live long enough"),) * 2
                        if revision == "bfail2" else (), **controls,
                    )
        if kind in ("wf_reference", "local_wf"):
            for solver in ("current", "next", "slow"):
                controls = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(f"direct-outlives-{solver}", "tests/ui/ohm/direct-outlives.rs", **controls)
                check(
                    f"direct-outlives-lifetimes-{solver}",
                    "tests/ui/ohm/direct-outlives-lifetimes.rs",
                    errors=(
                        (None, "lifetime may not live long enough"),
                        (None, "lifetime may not live long enough"),
                        ("E0521", "borrowed data escapes outside of function"),
                        ("E0521", "borrowed data escapes outside of function"),
                        ("E0310", "the parameter type `T` may not live long enough"),
                        ("E0309", "the parameter type `T` may not live long enough"),
                        ("E0310", "the associated type `<T as Project>::Output` may not live long enough"),
                    ), **controls,
                )
                check(
                    f"direct-outlives-higher-ranked-{solver}",
                    "tests/ui/ohm/direct-outlives-higher-ranked.rs",
                    errors=(
                        ("E0521", "borrowed data escapes outside of function"),
                        ("E0521", "borrowed data escapes outside of function"),
                        (None, "lifetime may not live long enough"),
                    ), **controls,
                )
                check(
                    f"direct-outlives-local-{solver}",
                    "tests/ui/ohm/direct-outlives-local.rs",
                    errors=(("E0597", "`local` does not live long enough"),), **controls,
                )
                if kind == "wf_reference":
                    check(f"wf-reference-{solver}", "tests/ui/ohm/wf-reference.rs", **controls)
                    check(
                        f"wf-reference-lifetimes-{solver}",
                        "tests/ui/ohm/wf-reference-lifetimes.rs",
                        errors=((None, "lifetime may not live long enough"),)
                        + (("E0309", "the parameter type `T` may not live long enough"),) * 4
                        + (("E0597", "`value` does not live long enough"),),
                        required_spans=(
                            (10, 5, 27, True, "requires that `'b` must outlive `'a`"),
                            (15, 5, 21, True, "...so that the type `T` will meet its required lifetime bounds"),
                            (20, 5, 29, True, "...so that the type `T` will meet its required lifetime bounds"),
                            (25, 5, 28, True, "...so that the type `T` will meet its required lifetime bounds"),
                            (30, 5, 36, True, "...so that the type `T` will meet its required lifetime bounds"),
                            (36, 20, 26, True, "borrowed value does not live long enough"),
                        ), **controls,
                    )
                    check(
                        f"wf-reference-fallback-{solver}",
                        "tests/ui/ohm/wf-reference-fallback.rs",
                        errors=(("E0277", "the trait bound `NotClone: Clone` is not satisfied"),)
                        * (2 if solver == "next" else 1) + (
                            ("E0309", "the associated type `<T as Project>::Output` may not live long enough"),
                            (None, "lifetime may not live long enough"),
                            ("E0521", "borrowed data escapes outside of function"),
                        ), **controls,
                    )
                else:
                    check(f"local-wf-{solver}", "tests/ui/ohm/local-wf.rs", **controls)
                    check(
                        f"local-wf-lifetimes-{solver}", "tests/ui/ohm/local-wf-lifetimes.rs",
                        errors=(("E0309", "the parameter type `T` may not live long enough"),)
                        + ((None, "lifetime may not live long enough"),) * 5,
                        required_spans=(
                            (13, 13, 36, True, "...so that the type `T` will meet its required lifetime bounds"),
                            (18, 13, 36, True, "requires that `'b` must outlive `'a`"),
                            (26, 13, 47, True, "requires that `'b` must outlive `'a`"),
                            (38, 13, 34, True, "requires that `'a` must outlive `'static`"),
                            (45, 13, 35, True, "requires that `'a` must outlive `'static`"),
                            (52, 13, 47, True, "requires that `'a` must outlive `'static`"),
                        ), **controls,
                    )
                    check(
                        f"local-wf-bounds-{solver}", "tests/ui/ohm/local-wf-bounds.rs",
                        errors=(
                            ("E0277", "the size for values of type `T` cannot be known at compilation time"),
                            ("E0277", "the trait bound `String: Copy` is not satisfied"),
                        ) * (2 if solver == "next" else 1)
                        + (("E0038", "the trait `NotDyn` is not dyn compatible"),), **controls,
                    )
        if kind == "local_wf":
            fixture = Path(__file__).resolve().parent / "cases" / "local_wf" / "nominal-plan.rs"
            for solver in ("current", "next", "slow"):
                cache = output / "incremental" / f"local-wf-plan-{solver}"
                for revision in ("bpass1", "bfail2"):
                    check(
                        f"local-wf-plan-{solver}-{revision}", str(fixture), revision=revision,
                        next_solver=solver == "next", disable_fast_paths=solver == "slow",
                        incremental=cache, require_loaded=revision == "bfail2",
                        execute=False, extra_flags=("--emit=metadata",),
                        errors=((None, "lifetime may not live long enough"),) * 2
                        if revision == "bfail2" else (),
                    )
        if kind == "trivial_wf_batch":
            borrowed_move = ("E0505", "cannot move out of `x` because it is borrowed")
            for solver in ("current", "next", "slow"):
                controls = {
                    "revision": solver,
                    "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                check(f"trivial-wf-batch-{solver}", "tests/ui/ohm/trivial-wf-batch.rs", **controls)
                check(
                    f"trivial-wf-batch-no-core-{solver}", "tests/ui/ohm/trivial-wf-batch-no-core.rs",
                    execute=False, extra_flags=("--emit=metadata",), **controls,
                )
                check(
                    f"trivial-wf-batch-lifetimes-{solver}", "tests/ui/ohm/trivial-wf-batch-lifetimes.rs",
                    errors=(("E0521", "borrowed data escapes outside of function"),)
                    + ((None, "lifetime may not live long enough"),) * 4, **controls,
                )
                for suffix, errors in (
                    ("", (borrowed_move,)),
                    ("-2", ((None, "lifetime may not live long enough"),)),
                    ("-3", ()),
                    ("-4", (borrowed_move,)),
                    ("-5", (("E0309", "the parameter type `T` may not live long enough"),) * 2
                     + (borrowed_move,)),
                ):
                    check(
                        f"trivial-wf-implied-bounds{suffix}-{solver}",
                        f"tests/ui/fn/implied-bounds-unnorm-associated-type{suffix}.rs",
                        errors=errors, execute=False, extra_flags=("--emit=metadata",), **controls,
                    )
                check(
                    f"trivial-wf-normalized-sig-lifetimes-{solver}",
                    "tests/ui/nll/check-normalized-sig-for-wf.rs",
                    errors=(("E0597", "`s` does not live long enough"),)
                    + (("E0521", "borrowed data escapes outside of function"),) * 2, **controls,
                )
                check(
                    f"trivial-wf-normalized-signature-{solver}",
                    "tests/ui/wf/check-wf-of-normalized-signature.rs",
                    execute=False, extra_flags=("--emit=metadata",), **controls,
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
