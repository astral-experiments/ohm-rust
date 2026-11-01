# Finite-receiver recovery on uv #20988

Signed −18/T4 was **2.125×** (99.37% interval 2.061–2.181×). 12/12 pairs exceeded 2×, with one-sided p=0.000244141. The registered gate passed.

Optimized T3/T4 was **1.064×** (99.37% interval 1.045–1.073×). 12/12 pairs exceeded 1×, with one-sided p=0.000244141. The registered gate passed. The joint claim passed. Each held-out block used all six arm orders and three independently warmed targets. The direct and incremental tests each used α=.01 plus the registered interval, seed and order-stratum checks. The direct gate also required a positive incremental median in both seeds.

The discovery medians were 2.100× for signed −18/T4 and 1.052× for optimized T3/T4. The rule required at least 1.8× and strictly above 1×, respectively. Discovery contributed zero rows to confirmation. The earlier untrained comparison measured 1.0917× and missed its 1.10 threshold.

Cold A is each cohort's actual first parent build. First B and no-ops are medians over the held-out rows. One cold observation per arm per cohort supports descriptive comparison.

| Compiler | Cold A discovery (s) | Cold A confirm-1 (s) | Cold A confirm-2 (s) | First B (s) | A no-op (s) | B no-op (s) |
|---|---:|---:|---:|---:|---:|---:|
| Signed Ohm −18 | 47.525 | 48.282 | 47.749 | 6.525 | 0.374 | 0.378 |
| 007 pre-BOLT T3, format 10 | 48.780 | 48.868 | 49.081 | 3.267 | 0.375 | 0.379 |
| Finite-receiver pre-BOLT T4, format 12 | 49.207 | 49.200 | 49.130 | 3.085 | 0.376 | 0.379 |

All arms ran the same signed −18 Cargo: `cargo check -p uv-resolver --locked --offline -j8`. The checkout started at the actual uv parent. The complete uncommitted edit reused compiled workspace-exclusion glob patterns. Each first edited check rebuilt uv-workspace, uv-types, uv-distribution and uv-resolver. The settled A and immediately following B checks rebuilt zero crates. Timing covers the ordinary Cargo command. The separately run native compiler suites validate dispatch and diagnostics under both solvers. [Historical uv edit](https://github.com/astral-sh/uv/pull/20988)

The optimized comparator uses the 007 T3 head a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5 and format 10. T4 uses fd96f14140962cc722aa1335c4449c01f3f3a286 and format 12, based on the repaired T3 source 26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9. Both query rosters have 364 kinds. Both compilers are pre-BOLT release-candidate builds, each with its own profile from nine non-uv workloads. A fresh target per SDK keeps incremental data within its compiler. This comparison includes the format change and source-dependent PGO. Signed −18 is the shipped reference.

The common fifth compiler suite ran the seven-revision 007 T3 run-make fixture under both solvers on T4 (14 native dispatch checks). The separate T4-specific finite suite ran its primitive, tuple and oversized-tuple cases on the optimized stage2 compiler. Each solver matched the clean executable across four stages.

The first compiler build exited 1 during source selection after its shared bridge completed. It produced zero timing rows. The initial discovery launch also returned before it created a plan or any worker. The subsequent launch used the two authenticated helper files from the 007 study. The successor retained the same Rust source, schedule and scoring thresholds, and separately checked its source selector before issuing the new SDK.

## Offline verification

Run `sha256sum -c SHA256SUMS`, then `python3 verify.py --negative`. Python 3.10 or later suffices. The verifier checks compiler and training identity, two-solver semantic controls, each real Cargo outcome, raw order and seed, first-edit clocks, cold starts, unchanged checks, and the registered decisions. `times.csv` in the archive contains every arm and order. Normalized Cargo output and source digests are included. The portable verifier replays the recorded measurements and native receipts without rebuilding the SDKs.

## Run Cargo on another host

From this directory in the evidence branch, use Linux x86-64, Python 3.11+, Git, a system toolchain, and a quiet fixed CPU set. Supply a uv checkout with tag `0.12.2`, parent `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` and direct child `1979349b68dbaa936b98c0f409c1df0598fa72f1`. Fetch the parent's lock into a dedicated Cargo home before going offline. Provide three complete compiler/std SDK directories and an empty scratch path. The signed −18 Cargo must have SHA-256 `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573`.

```sh
python3 rerun.py --describe
taskset -c 0-15 python3 rerun.py --uv "$UV" --stock "$SDK18" --parent "$T3_V10" --candidate "$T4_V12" --cargo "$SDK18/bin/cargo" --cargo-home "$CARGO_CACHE" --scratch "$EMPTY_RUN"
python3 rerun.py --summarize "$EMPTY_RUN"
```

`rerun.py` loads the SHA-checked `../ohm-uv-resolver-repro.py`. The shared runner verifies the full uncommitted edit, cold-A/check/no-op sequences and crate roster. The study-specific adapter checks both launcher and driver identities, uses held-out seeds 415670321 and 367210859, and computes the two α=.01 contrasts. This independent rerun builds 36 cold parents, compared with two restored held-out parent histories per SDK in the reported study. Its rows stay separate.

For rebuilt SDKs use the T3 and T4 source commits above with their matching standard libraries and verified common LLVM. Both measured stage2 compilers used `-Zdylib-lto -Clto=thin -Ccodegen-units=1 -Cembed-bitcode=yes`, their own nine-workload PGO generation and use, and no BOLT rewriting. The profile, launcher and loaded driver hashes are in `qualification.json` inside the ZIP. Adding `--acknowledge-rebuilt` to the run and summary records changed compiler identities explicitly. The adapter's CLI, subjects, schedule and scorer were checked locally. An external rebuilt-SDK run has not been measured here.
