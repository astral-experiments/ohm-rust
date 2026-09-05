"""Validate the fixed measured compilers without rebuilding either compiler."""

from collections import Counter
import hashlib
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
    "generalizer": ("3c7b97083e3a18bb624ce88348c0a4ab5670200f", 33977269210),
    "lazy_obligation": ("e78a9f448a18ff0fc37df9d520dadbb50260673c", 33977285310),
    "mimalloc_v2": ("c64f2819b0b7982ad5f7f75b5de19374d6de18c1", 33977300225),
    "resolved_relations": ("fab2b5e8e23f874e40e727ec1cacea533e25a527", 33977945889),
    "root_probe": ("94cabfecfc6fc201868a44007257c8c984644830", 33978363752),
    "dense_lazy": ("696fd87252a253aaa2b34b9bf092804dcbde3585", 33979491861),
    "veccache_mode": ("64c31b0b3c22ada448cfab90be3e9c7fc0339fc3", 33979934693),
    "lazy_engine": ("2ccd07c4470a64f6462191805aef3e6e8a72057b", 33980939158),
    "hashbrown_neon16": ("17a5026d761d5a42572466298ce9732fad9ceb96", 33980542884),
    "canonical_input_cache": ("0250cb57508464a20d2a06c8750883dc9d5739ec", 33981167471),
    "forest_small_storage": ("3670ed7dd0f50ab97916df559b04e95926894d35", 33982186599),
    "resolved_fold_cache": ("f1266b5556879e70ecb33af080c0668108736e8e", 33982201018),
    "stage1_thin_lto": ("198269f763718786fcfbd22b82555d9c6d656ab9", 33982576401),
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

        if kind == "stage1_thin_lto":
            toolchain_root = Path(sys.argv[3]).absolute().parent.parent
            drivers = sorted(toolchain_root.glob("lib/librustc_driver*.so"))
            assert drivers
            report["toolchain_root"] = str(toolchain_root)
            report["toolchain_files"] = []
            for path in (toolchain_root / "bin/rustc", *drivers):
                with path.open("rb") as stream:
                    digest = hashlib.file_digest(stream, "sha256").hexdigest()
                report["toolchain_files"].append({
                    "path": str(path.relative_to(toolchain_root)),
                    "resolved_path": str(path.resolve()),
                    "is_symlink": path.is_symlink(),
                    "symlink_target": os.readlink(path) if path.is_symlink() else None,
                    "size_bytes": path.stat().st_size,
                    "sha256": digest,
                })

        if kind == "mimalloc_v2":
            activation = subprocess.run(
                [str(rustc), "-vV"], capture_output=True, text=True, timeout=30,
                env={**env, "MIMALLOC_VERBOSE": "1"},
            )
            (output / "allocator.stdout.txt").write_text(activation.stdout)
            (output / "allocator.stderr.txt").write_text(activation.stderr)
            report["allocator_activation"] = {
                "expected_version": "2.3.2", "compiler_exit": activation.returncode,
                "environment_delta": {"MIMALLOC_VERBOSE": "1"},
            }
            assert activation.returncode == 0, activation.stderr
            assert f"commit-hash: {expected_sha}" in activation.stdout.splitlines()
            assert any(line.startswith("mimalloc: v2.3.2 (") for line in activation.stderr.splitlines()), activation.stderr
            assert "mimalloc: process init:" in activation.stderr, activation.stderr
            report["allocator_activation"]["status"] = "passed"

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
        if kind in ("query_cache", "interner_tables", "packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "dense_lazy", "hashbrown_neon16", "forest_small_storage", "stage1_thin_lto", "veccache_mode", "nested_argfolder", "interner_cache", "mimalloc_v2"):
            thread_counts = (4,) if kind in ("packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "dense_lazy", "hashbrown_neon16", "forest_small_storage", "stage1_thin_lto", "veccache_mode", "nested_argfolder", "interner_cache", "mimalloc_v2") else (1, 4)
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
        if kind in ("packed_aliases", "interned_arenas", "nightly_hasher", "dense_interners", "dense_lazy", "hashbrown_neon16", "forest_small_storage", "stage1_thin_lto", "veccache_mode", "nested_argfolder", "interner_cache", "mimalloc_v2"):
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
        if kind == "veccache_mode":
            for threads in ("sync", 4):
                check(
                    f"veccache-mode-threads-{threads}",
                    "tests/ui/associated-types/projection-as-type-alias.rs",
                    threads=threads,
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
        if kind == "generalizer":
            # The unsupported next-solver modes are absent from this fixed suite.
            previous_log = env.get("RUSTC_LOG")
            env["RUSTC_LOG"] = "error"
            cases = (
                ('generalizer-pure-current', 'tests/ui/ohm/generalizer-pure.rs', 'current', 'current',
                 (), (), True, False),
                ('generalizer-pure-next', 'tests/ui/ohm/generalizer-pure.rs', 'next', 'next',
                 (), (), True, False),
                ('generalizer-pure-slow', 'tests/ui/ohm/generalizer-pure.rs', 'slow', 'slow',
                 (), (), True, False),
                ('generalizer-pure-fallback-current', 'tests/ui/ohm/generalizer-pure-fallback.rs', 'current', 'current',
                 (('E0425', 'cannot find type `Missing` in this scope'), ('E0308', 'mismatched types'), ('E0277', 'the trait bound `T: Same<_>` is not satisfied')), ((17, 16, 23, True, 'not found in this scope'), (12, 5, 20, True, 'expected `u32`, found associated type'), (28, 15, 16, True, 'the trait `Same<_>` is not implemented for `T`')), False, True),
                ('generalizer-pure-fallback-next', 'tests/ui/ohm/generalizer-pure-fallback.rs', 'next', 'next',
                 (('E0425', 'cannot find type `Missing` in this scope'), ('E0308', 'mismatched types'), ('E0277', 'the trait bound `T: Same<_>` is not satisfied')), ((17, 16, 23, True, 'not found in this scope'), (12, 5, 20, True, 'expected `u32`, found associated type'), (28, 15, 16, True, 'the trait `Same<_>` is not implemented for `T`')), False, True),
                ('generalizer-pure-fallback-slow', 'tests/ui/ohm/generalizer-pure-fallback.rs', 'slow', 'slow',
                 (('E0425', 'cannot find type `Missing` in this scope'), ('E0308', 'mismatched types'), ('E0277', 'the trait bound `T: Same<_>` is not satisfied')), ((17, 16, 23, True, 'not found in this scope'), (12, 5, 20, True, 'expected `u32`, found associated type'), (28, 15, 16, True, 'the trait `Same<_>` is not implemented for `T`')), False, True),
                ('generalizer-pure-lifetimes-current', 'tests/ui/ohm/generalizer-pure-lifetimes.rs', 'current', 'current',
                 (('E0308', 'mismatched types'), ('E0308', 'mismatched types'), (None, 'lifetime may not live long enough'), (None, 'lifetime may not live long enough')), ((24, 5, 11, True, 'types differ in mutability'), (32, 27, 42, True, 'one type is more general than the other'), (10, 5, 28, True, "returning this value requires that `'a` must outlive `'static`"), (16, 19, 25, True, "this usage requires that `'a` must outlive `'static`")), False, True),
                ('generalizer-pure-lifetimes-next', 'tests/ui/ohm/generalizer-pure-lifetimes.rs', 'next', 'next',
                 (('E0308', 'mismatched types'), ('E0308', 'mismatched types'), (None, 'lifetime may not live long enough'), (None, 'lifetime may not live long enough')), ((24, 5, 11, True, 'types differ in mutability'), (32, 27, 42, True, 'one type is more general than the other'), (10, 5, 28, True, "returning this value requires that `'a` must outlive `'static`"), (16, 19, 25, True, "this usage requires that `'a` must outlive `'static`")), False, True),
                ('generalizer-pure-lifetimes-slow', 'tests/ui/ohm/generalizer-pure-lifetimes.rs', 'slow', 'slow',
                 (('E0308', 'mismatched types'), ('E0308', 'mismatched types'), (None, 'lifetime may not live long enough'), (None, 'lifetime may not live long enough')), ((24, 5, 11, True, 'types differ in mutability'), (32, 27, 42, True, 'one type is more general than the other'), (10, 5, 28, True, "returning this value requires that `'a` must outlive `'static`"), (16, 19, 25, True, "this usage requires that `'a` must outlive `'static`")), False, True),
                ('generalizer-legacy-direct-self-reference-occurs-check-current', 'tests/ui/type-inference/direct-self-reference-occurs-check.rs', 'current', 'current',
                 (('E0275', 'overflow assigning `Box<_>` to `_`'),), ((7, 18, 19, True, None),), False, True),
                ('generalizer-legacy-direct-self-reference-occurs-check-slow', 'tests/ui/type-inference/direct-self-reference-occurs-check.rs', 'current', 'slow',
                 (('E0275', 'overflow assigning `Box<_>` to `_`'),), ((7, 18, 19, True, None),), False, True),
                ('generalizer-legacy-enum-self-reference-occurs-check-current', 'tests/ui/type-inference/enum-self-reference-occurs-check.rs', 'current', 'current',
                 (('E0275', 'overflow assigning `Clam<_>` to `_`'),), ((11, 17, 18, True, None),), False, True),
                ('generalizer-legacy-enum-self-reference-occurs-check-slow', 'tests/ui/type-inference/enum-self-reference-occurs-check.rs', 'current', 'slow',
                 (('E0275', 'overflow assigning `Clam<_>` to `_`'),), ((11, 17, 18, True, None),), False, True),
                ('generalizer-legacy-generalize-subtyped-variables-current', 'tests/ui/type-inference/generalize-subtyped-variables.rs', 'current', 'current',
                 (), (), False, True),
                ('generalizer-legacy-generalize-subtyped-variables-slow', 'tests/ui/type-inference/generalize-subtyped-variables.rs', 'current', 'slow',
                 (), (), False, True),
                ('generalizer-legacy-type-infer-generalize-ty-var-current', 'tests/ui/inference/type-infer-generalize-ty-var.rs', 'current', 'current',
                 (), (), False, True),
                ('generalizer-legacy-type-infer-generalize-ty-var-next', 'tests/ui/inference/type-infer-generalize-ty-var.rs', 'next', 'next',
                 (), (), False, True),
                ('generalizer-legacy-type-infer-generalize-ty-var-slow', 'tests/ui/inference/type-infer-generalize-ty-var.rs', 'current', 'slow',
                 (), (), False, True),
                ('generalizer-legacy-hr-alias-non-hr-alias-self-ty-1-current', 'tests/ui/traits/next-solver/generalize/hr-alias-non-hr-alias-self-ty-1.rs', 'old', 'current',
                 (), (), False, True),
                ('generalizer-legacy-hr-alias-non-hr-alias-self-ty-1-next', 'tests/ui/traits/next-solver/generalize/hr-alias-non-hr-alias-self-ty-1.rs', 'next', 'next',
                 (), (), False, True),
                ('generalizer-legacy-hr-alias-non-hr-alias-self-ty-1-slow', 'tests/ui/traits/next-solver/generalize/hr-alias-non-hr-alias-self-ty-1.rs', 'old', 'slow',
                 (), (), False, True),
                ('generalizer-legacy-alias-with-bound-vars-incomplete-generalization-current', 'tests/ui/traits/next-solver/generalize/alias-with-bound-vars-incomplete-generalization.rs', 'old', 'current',
                 (), (), False, True),
                ('generalizer-legacy-alias-with-bound-vars-incomplete-generalization-next', 'tests/ui/traits/next-solver/generalize/alias-with-bound-vars-incomplete-generalization.rs', 'next', 'next',
                 (), (), False, True),
                ('generalizer-legacy-alias-with-bound-vars-incomplete-generalization-slow', 'tests/ui/traits/next-solver/generalize/alias-with-bound-vars-incomplete-generalization.rs', 'old', 'slow',
                 (), (), False, True),
                ('generalizer-legacy-occurs-check-nested-alias-current', 'tests/ui/traits/next-solver/generalize/occurs-check-nested-alias.rs', 'old', 'current',
                 (), (), False, True),
                ('generalizer-legacy-occurs-check-nested-alias-next', 'tests/ui/traits/next-solver/generalize/occurs-check-nested-alias.rs', 'next', 'next',
                 (), (), False, True),
                ('generalizer-legacy-occurs-check-nested-alias-slow', 'tests/ui/traits/next-solver/generalize/occurs-check-nested-alias.rs', 'old', 'slow',
                 (), (), False, True),
                ('generalizer-legacy-generalize-proj-new-universe-index-1-next', 'tests/ui/traits/next-solver/generalize/generalize-proj-new-universe-index-1.rs', 'next', 'next',
                 (), (), False, True),
                ('generalizer-legacy-associated-type-current', 'tests/ui/coherence/occurs-check/associated-type.rs', 'old', 'current',
                 (('E0119', "conflicting implementations of trait `Overlap<for<'a> fn(&'a (), ())>` for type `for<'a> fn(&'a (), ())`"),), (), False, True),
                ('generalizer-legacy-associated-type-next', 'tests/ui/coherence/occurs-check/associated-type.rs', 'next', 'next',
                 (('E0119', "conflicting implementations of trait `Overlap<for<'a> fn(&'a (), ())>` for type `for<'a> fn(&'a (), ())`"),), (), False, True),
                ('generalizer-legacy-associated-type-slow', 'tests/ui/coherence/occurs-check/associated-type.rs', 'old', 'slow',
                 (('E0119', "conflicting implementations of trait `Overlap<for<'a> fn(&'a (), ())>` for type `for<'a> fn(&'a (), ())`"),), (), False, True),
                ('generalizer-legacy-opaques-current', 'tests/ui/coherence/occurs-check/opaques.rs', 'current', 'current',
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'),), ((27, 1, 30, True, 'conflicting implementation'),), False, True),
                ('generalizer-legacy-opaques-next', 'tests/ui/coherence/occurs-check/opaques.rs', 'next', 'next',
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'), ('E0282', 'type annotations needed')), ((27, 1, 30, True, 'conflicting implementation'), (11, 19, 41, True, 'cannot infer type')), False, True),
                ('generalizer-legacy-opaques-slow', 'tests/ui/coherence/occurs-check/opaques.rs', 'current', 'slow',
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'),), ((27, 1, 30, True, 'conflicting implementation'),), False, True),
                ('generalizer-legacy-universe-error1-current', 'tests/ui/traits/non_lifetime_binders/universe-error1.rs', 'current', 'current',
                 (('E0277', 'the trait bound `T: Other<_>` is not satisfied'),), ((16, 11, 12, True, 'the trait `Other<_>` is not implemented for `T`'),), False, True),
                ('generalizer-legacy-universe-error1-slow', 'tests/ui/traits/non_lifetime_binders/universe-error1.rs', 'current', 'slow',
                 (('E0277', 'the trait bound `T: Other<_>` is not satisfied'),), ((16, 11, 12, True, 'the trait `Other<_>` is not implemented for `T`'),), False, True),
                ('generalizer-legacy-bind-param-current', 'tests/ui/const-generics/occurs-check/bind-param.rs', 'current', 'current',
                 (), (), False, False),
                ('generalizer-legacy-bind-param-slow', 'tests/ui/const-generics/occurs-check/bind-param.rs', 'current', 'slow',
                 (), (), False, False),
                ('generalizer-legacy-unify-n-nplusone-current', 'tests/ui/const-generics/occurs-check/unify-n-nplusone.rs', 'current', 'current',
                 (('E0308', 'mismatched types'),), ((14, 11, 20, True, 'encountered a self-referencing constant'),), False, True),
                ('generalizer-legacy-unify-n-nplusone-slow', 'tests/ui/const-generics/occurs-check/unify-n-nplusone.rs', 'current', 'slow',
                 (('E0308', 'mismatched types'),), ((14, 11, 20, True, 'encountered a self-referencing constant'),), False, True),
            )
            for name, fixture, revision, solver, errors, spans, execute, metadata in cases:
                check(
                    name, fixture, revision=revision, next_solver=solver == "next",
                    disable_fast_paths=solver == "slow", errors=errors,
                    required_spans=spans, execute=execute,
                    extra_flags=("-Aunused", "-Aincomplete_features", "-Zui-testing")
                    + (("--emit=metadata",) if metadata else ()),
                )
            if previous_log is None:
                env.pop("RUSTC_LOG")
            else:
                env["RUSTC_LOG"] = previous_log
        if kind in ("lazy_obligation", "dense_lazy", "lazy_engine"):
            for solver in ("current", "next", "slow"):
                controls = {
                    "revision": solver, "next_solver": solver == "next",
                    "disable_fast_paths": solver == "slow",
                }
                for fixture in ("lazy-obligation-context", "lazy-obligation-rpitit"):
                    check(f"{fixture}-{solver}", f"tests/ui/ohm/{fixture}.rs", **controls)
                check(
                    f"lazy-obligation-errors-{solver}", "tests/ui/ohm/lazy-obligation-errors.rs",
                    errors=(('E0276', 'impl has stricter requirements than trait'), ('E0271', 'expected `Once<()>` to be an iterator that yields `T`, but it yields `()`'), ('E0597', '`first` does not live long enough'), ('E0597', '`second` does not live long enough')), required_spans=((13, 28, 32, True, 'impl has extra requirement `T: Copy`'), (22, 34, 65, True, 'expected type parameter `T`, found `()`'), (47, 26, 32, True, 'borrowed value does not live long enough'), (49, 32, 39, True, 'borrowed value does not live long enough')),
                    extra_flags=("-Zui-testing",), **controls,
                )
                for fixture in ("method-normalize-bounds-issue-20604", "method-probe-no-guessing-dyn-trait"):
                    check(
                        f"lazy-obligation-{fixture}-{solver}", f"tests/ui/methods/{fixture}.rs",
                        **controls,
                    )
                check(
                    f"lazy-obligation-normalized-signature-{solver}",
                    "tests/ui/wf/check-wf-of-normalized-signature.rs",
                    execute=False, extra_flags=("--emit=metadata",), **controls,
                )
                check(
                    f"lazy-obligation-normalized-sig-lifetimes-{solver}",
                    "tests/ui/nll/check-normalized-sig-for-wf.rs",
                    errors=(("E0597", "`s` does not live long enough"),)
                    + (("E0521", "borrowed data escapes outside of function"),) * 2, **controls,
                )
            for directory, fixture in (
                ("normalize", "deeply-normalize-env-in-compare-impl-item"),
                ("method", "param-method-from-unnormalized-param-env"),
            ):
                check(
                    f"lazy-obligation-{fixture}-next",
                    f"tests/ui/traits/next-solver/{directory}/{fixture}.rs",
                    next_solver=True, execute=False, extra_flags=("-Aunused", "--emit=metadata"),
                )
        if kind in ("resolved_relations", "root_probe", "resolved_fold_cache"):
            cases = (
                ('tests/ui/inference/lub-glb-with-unbound-infer-var.rs', 'current', True,
                 (), ()),
                ('tests/ui/inference/lub-glb-with-unbound-infer-var.rs', 'next', True,
                 (), ()),
                ('tests/ui/inference/lub-glb-with-unbound-infer-var.rs', 'slow', True,
                 (), ()),
                ('tests/ui/issues/issue-41677.rs', 'current', True,
                 (), ()),
                ('tests/ui/issues/issue-41677.rs', 'next', True,
                 (), ()),
                ('tests/ui/issues/issue-41677.rs', 'slow', True,
                 (), ()),
                ('tests/ui/regions/regions-lub-ref-ref-rc.rs', 'current', True,
                 (), ()),
                ('tests/ui/regions/regions-lub-ref-ref-rc.rs', 'next', True,
                 (), ()),
                ('tests/ui/regions/regions-lub-ref-ref-rc.rs', 'slow', True,
                 (), ()),
                ('tests/ui/inference/type-infer-generalize-ty-var.rs', 'current', False,
                 (), ()),
                ('tests/ui/inference/type-infer-generalize-ty-var.rs', 'next', False,
                 (), ()),
                ('tests/ui/inference/type-infer-generalize-ty-var.rs', 'slow', False,
                 (), ()),
                ('tests/ui/impl-trait/lazy_subtyping_of_opaques.rs', 'current', False,
                 (), ()),
                ('tests/ui/impl-trait/lazy_subtyping_of_opaques.rs', 'next', False,
                 (), ()),
                ('tests/ui/impl-trait/lazy_subtyping_of_opaques.rs', 'slow', False,
                 (), ()),
                ('tests/ui/coherence/occurs-check/opaques.rs', 'current', False,
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'),), ((27, 1, 30, True, 'conflicting implementation'),)),
                ('tests/ui/coherence/occurs-check/opaques.rs', 'next', False,
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'), ('E0282', 'type annotations needed')), ((27, 1, 30, True, 'conflicting implementation'), (11, 19, 41, True, 'cannot infer type'))),
                ('tests/ui/coherence/occurs-check/opaques.rs', 'slow', False,
                 (('E0119', 'conflicting implementations of trait `Trait<_>`'),), ((27, 1, 30, True, 'conflicting implementation'),)),
                ('tests/ui/type-inference/infinite-type-occurs-check.rs', 'current', False,
                 (('E0275', 'overflow assigning `Box<_>` to `_`'),), ((9, 9, 10, True, None),)),
                ('tests/ui/type-inference/infinite-type-occurs-check.rs', 'next', False,
                 (('E0282', 'type annotations needed for `Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<Box<_>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>`'),), ((6, 9, 10, True, None),)),
                ('tests/ui/type-inference/infinite-type-occurs-check.rs', 'slow', False,
                 (('E0275', 'overflow assigning `Box<_>` to `_`'),), ((9, 9, 10, True, None),)),
                ('tests/ui/regions/regions-glb-free-free.rs', 'current', False,
                 (('E0621', 'explicit lifetime required in the type of `s`'),), ()),
                ('tests/ui/regions/regions-glb-free-free.rs', 'next', False,
                 (('E0621', 'explicit lifetime required in the type of `s`'),), ()),
                ('tests/ui/regions/regions-glb-free-free.rs', 'slow', False,
                 (('E0621', 'explicit lifetime required in the type of `s`'),), ()),
                ('tests/ui/ohm/resolved-relation-equality.rs', 'current', True,
                 (), ()),
                ('tests/ui/ohm/resolved-relation-equality.rs', 'next', True,
                 (), ()),
                ('tests/ui/ohm/resolved-relation-equality.rs', 'slow', True,
                 (), ()),
            )
            for fixture, solver, execute, errors, spans in cases:
                if kind in ("root_probe", "resolved_fold_cache") and fixture == "tests/ui/ohm/resolved-relation-equality.rs":
                    fixture = str(Path(__file__).resolve().parent / "cases/root_probe/resolved-relation-equality.rs")
                check(
                    f"resolved-relations-{Path(fixture).stem}-{solver}", fixture,
                    revision=solver, next_solver=solver == "next",
                    disable_fast_paths=solver == "slow", errors=errors,
                    required_spans=spans, execute=execute,
                    extra_flags=(("-Zwrite-long-types-to-disk=no",)
                                 if fixture.endswith("/infinite-type-occurs-check.rs") and solver == "next"
                                 else ()) + (() if execute else ("--emit=metadata",)),
                )
        if kind in ("root_probe", "resolved_fold_cache"):
            cases = (
                ('tests/ui/inference/bitwise-integer-inference-68951.rs', 'current', False,
                 (), ()),
                ('tests/ui/inference/bitwise-integer-inference-68951.rs', 'next', False,
                 (), ()),
                ('tests/ui/inference/bitwise-integer-inference-68951.rs', 'slow', False,
                 (), ()),
                ('tests/ui/inference/ambiguous-numeric-in-closure-ref.rs', 'current', False,
                 (('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`"), ('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`")), ((6, 35, 38, True, None), (11, 37, 40, True, None))),
                ('tests/ui/inference/ambiguous-numeric-in-closure-ref.rs', 'next', False,
                 (('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`"), ('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`")), ((6, 35, 38, True, None), (11, 37, 40, True, None))),
                ('tests/ui/inference/ambiguous-numeric-in-closure-ref.rs', 'slow', False,
                 (('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`"), ('E0689', "can't call method `pow` on ambiguous numeric type `{integer}`")), ((6, 35, 38, True, None), (11, 37, 40, True, None))),
                ('tests/ui/inference/ice-ifer-var-leaked-out-of-rollback-122098.rs', 'current', False,
                 (('E0261', "use of undeclared lifetime name `'a`"), ('E0262', "invalid lifetime parameter name: `'static`"), ('E0261', "use of undeclared lifetime name `'q`"), ('E0392', "lifetime parameter `'q` is never used"), ('E0277', 'the size for values of type `Self` cannot be known at compilation time'), ('E0061', 'this function takes 0 arguments but 1 argument was supplied')), ((4, 20, 22, True, 'undeclared lifetime'), (14, 6, 13, True, "'static is a reserved lifetime name"), (14, 21, 23, True, 'undeclared lifetime'), (11, 14, 16, True, 'unused lifetime parameter'), (7, 21, 25, True, "doesn't have a size known at compile-time"), (21, 31, 41, True, None))),
                ('tests/ui/inference/ice-ifer-var-leaked-out-of-rollback-122098.rs', 'next', False,
                 (('E0261', "use of undeclared lifetime name `'a`"), ('E0262', "invalid lifetime parameter name: `'static`"), ('E0261', "use of undeclared lifetime name `'q`"), ('E0392', "lifetime parameter `'q` is never used"), ('E0277', 'the size for values of type `Self` cannot be known at compilation time'), ('E0061', 'this function takes 0 arguments but 1 argument was supplied')), ((4, 20, 22, True, 'undeclared lifetime'), (14, 6, 13, True, "'static is a reserved lifetime name"), (14, 21, 23, True, 'undeclared lifetime'), (11, 14, 16, True, 'unused lifetime parameter'), (7, 21, 25, True, "doesn't have a size known at compile-time"), (21, 31, 41, True, None))),
                ('tests/ui/inference/ice-ifer-var-leaked-out-of-rollback-122098.rs', 'slow', False,
                 (('E0261', "use of undeclared lifetime name `'a`"), ('E0262', "invalid lifetime parameter name: `'static`"), ('E0261', "use of undeclared lifetime name `'q`"), ('E0392', "lifetime parameter `'q` is never used"), ('E0277', 'the size for values of type `Self` cannot be known at compilation time'), ('E0061', 'this function takes 0 arguments but 1 argument was supplied')), ((4, 20, 22, True, 'undeclared lifetime'), (14, 6, 13, True, "'static is a reserved lifetime name"), (14, 21, 23, True, 'undeclared lifetime'), (11, 14, 16, True, 'unused lifetime parameter'), (7, 21, 25, True, "doesn't have a size known at compile-time"), (21, 31, 41, True, None))),
                ('tests/ui/inference/iter-ufcs-coercion-type-unification.rs', 'current', False,
                 (), ()),
                ('tests/ui/inference/iter-ufcs-coercion-type-unification.rs', 'next', False,
                 (), ()),
                ('tests/ui/inference/iter-ufcs-coercion-type-unification.rs', 'slow', False,
                 (), ()),
                ('tests/ui/type-inference/float-type-inference-unification-14382.rs', 'current', True,
                 (), ()),
                ('tests/ui/type-inference/float-type-inference-unification-14382.rs', 'next', True,
                 (), ()),
                ('tests/ui/type-inference/float-type-inference-unification-14382.rs', 'slow', True,
                 (), ()),
            )
            for fixture, solver, execute, errors, spans in cases:
                check(
                    f"root-probe-{Path(fixture).stem}-{solver}", fixture,
                    revision=solver, next_solver=solver == "next",
                    disable_fast_paths=solver == "slow", errors=errors,
                    required_spans=spans, execute=execute,
                    extra_flags=("-Zui-testing", "-Aunused")
                    + (() if execute else ("--emit=metadata",)),
                )
        if kind == "resolved_fold_cache":
            fixture = "tests/ui/ohm/resolved-fold-probes.rs"
            fixture_hash = hashlib.sha256((source / fixture).read_bytes()).hexdigest()
            assert fixture_hash == "852568d558179ea74e1fd49e00fc3b391a3ee272a3f33a7b3270e5d18b1c634f"
            report["fixture_sha256"] = {fixture: fixture_hash}
            for mode in ("current", "slow", "next", "parallel"):
                check(
                    f"resolved-fold-probes-{mode}", fixture, revision=mode,
                    crate_name="resolved_fold_probes", next_solver=mode == "next",
                    disable_fast_paths=mode == "slow", threads=4 if mode == "parallel" else 1,
                )
            primary_spans = {}
            for case in report["cases"]:
                name = case["name"]
                diagnostics = [
                    json.loads(line)
                    for line in (output / f"{name}.jsonl").read_text().splitlines()
                ]
                if name.startswith("resolved-fold-probes-"):
                    assert not any(d.get("level") == "warning" for d in diagnostics), name
                stem, mode = name.rsplit("-", 1)
                if mode not in ("current", "slow"):
                    continue
                primary_spans[stem, mode] = [
                    (
                        diagnostic["code"]["code"] if diagnostic.get("code") else None,
                        diagnostic["message"],
                        [
                            (
                                span["line_start"], span["column_start"],
                                span["line_end"], span["column_end"], span["label"],
                            )
                            for span in diagnostic["spans"] if span["is_primary"]
                        ],
                    )
                    for diagnostic in diagnostics
                    if diagnostic.get("level") == "error"
                    and not diagnostic["message"].startswith("aborting due to ")
                ]
            pairs = sorted(stem for stem, mode in primary_spans if mode == "current")
            assert len(pairs) == 15
            for stem in pairs:
                assert primary_spans[stem, "current"] == primary_spans[stem, "slow"], stem
            report["current_slow_primary_spans_verified"] = pairs
        if kind == "canonical_input_cache":
            case_directory = Path(__file__).resolve().parent / "cases/canonical_input_cache"
            expected_files = json.loads((case_directory / "manifest.json").read_text())
            actual_files = {
                path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                for path in case_directory.glob("*.rs")
            }
            assert len(actual_files) == 13 and actual_files == expected_files
            report["fixture_sha256"] = actual_files
            errors = {
                'canonical-proof-region-errors': (
                    (None, 'lifetime may not live long enough'),
                    (None, 'lifetime may not live long enough'),
                ),
                'check-normalized-sig-for-wf': (
                    ('E0597', '`s` does not live long enough'),
                    ('E0521', 'borrowed data escapes outside of function'),
                    ('E0521', 'borrowed data escapes outside of function'),
                ),
                'lazy-obligation-errors': (
                    ('E0276', 'impl has stricter requirements than trait'),
                    ('E0271', 'expected `Once<()>` to be an iterator that yields `T`, but it yields `()`'),
                    ('E0597', '`first` does not live long enough'),
                    ('E0597', '`second` does not live long enough'),
                ),
                'repeated-wf-bounds': (
                    ('E0277', 'the trait bound `T: Copy` is not satisfied'),
                    (None, 'lifetime may not live long enough'),
                ),
            }
            spans = {
                'canonical-proof-region-errors': (
                    (28, 5, 19, True, "function was supposed to return data with lifetime `'a` but it is returning data with lifetime `'b`"),
                    (33, 5, 19, True, "function was supposed to return data with lifetime `'b` but it is returning data with lifetime `'a`"),
                ),
                'check-normalized-sig-for-wf': (
                    (11, 7, 9, True, 'borrowed value does not live long enough'),
                    (19, 5, 13, True, '`input` escapes the function body here'),
                    (19, 5, 13, True, "argument requires that `'1` must outlive `'static`"),
                    (27, 5, 13, True, '`input` escapes the function body here'),
                    (27, 5, 13, True, "argument requires that `'a` must outlive `'static`"),
                ),
                'lazy-obligation-errors': (
                    (13, 28, 32, True, 'impl has extra requirement `T: Copy`'),
                    (22, 34, 65, True, 'expected type parameter `T`, found `()`'),
                    (47, 26, 32, True, 'borrowed value does not live long enough'),
                    (49, 32, 39, True, 'borrowed value does not live long enough'),
                ),
                'repeated-wf-bounds': (
                    (12, 33, 43, True, 'the trait `Copy` is not implemented for `T`'),
                    (17, 5, 24, True, "function was supposed to return data with lifetime `'b` but it is returning data with lifetime `'a`"),
                ),
            }
            metadata_only = {
                "check-wf-of-normalized-signature",
                "deeply-normalize-env-in-compare-impl-item",
                "param-method-from-unnormalized-param-env",
            }
            next_only = {
                "deeply-normalize-env-in-compare-impl-item",
                "param-method-from-unnormalized-param-env",
            }
            allow_unused = {
                "method-normalize-bounds-issue-20604", "method-probe-no-guessing-dyn-trait",
                "check-wf-of-normalized-signature", "check-normalized-sig-for-wf",
            } | next_only
            primary_spans = {}
            for filename in sorted(expected_files):
                stem = Path(filename).stem
                modes = ("next",) if stem in next_only else ("current", "slow", "next")
                if stem == "canonical-proof-inputs":
                    modes += ("parallel",)
                for mode in modes:
                    name = f"{stem}-{mode}"
                    check(
                        name, str(case_directory / filename), revision=mode,
                        crate_name=stem.replace("-", "_"), next_solver=mode == "next",
                        disable_fast_paths=mode == "slow", threads=4 if mode == "parallel" else 1,
                        errors=errors.get(stem, ()), required_spans=spans.get(stem, ()),
                        execute=stem not in metadata_only,
                        extra_flags=(("--emit=metadata",) if stem in metadata_only else ())
                        + (("-Aunused",) if stem in allow_unused else ()),
                    )
                    diagnostics = [
                        json.loads(line)
                        for line in (output / f"{name}.jsonl").read_text().splitlines()
                    ]
                    assert not any(d.get("level") == "warning" for d in diagnostics), name
                    primary_spans[name] = [
                        (
                            diagnostic["message"],
                            [
                                (
                                    span["line_start"], span["column_start"],
                                    span["line_end"], span["column_end"], span["label"],
                                )
                                for span in diagnostic["spans"] if span["is_primary"]
                            ],
                        )
                        for diagnostic in diagnostics
                        if diagnostic.get("level") == "error"
                        and not diagnostic["message"].startswith("aborting due to ")
                    ]
            for stem in errors:
                assert primary_spans[f"{stem}-current"] == primary_spans[f"{stem}-slow"], stem
            report["current_slow_primary_spans_verified"] = sorted(errors)
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
