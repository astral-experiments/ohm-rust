# uv #20988 on the pre-BOLT #296 compiler

After the complete uv #20988 change, the median time for the first `cargo check -p uv-resolver --locked --offline -j8` was **6.605 s on signed −18** and **3.317 s on the optimized #296 compiler**. Across 12 held-out triples, the paired −18/T3 speedup was **1.982× [1.942, 2.022]**, with a 99.37% exact median interval and one-sided sign-test p=0.927002 against 2. All 12 first edits improved, and 4 exceeded 2×. The preregistered direct 2× gate did not pass.

The optimized base/T3 comparison was **2.019× [1.981, 2.045]** (96.14% interval, p=0.000244141 against 1). Its prespecified positive-effect gate passed. Signed −18/base was 0.981× [0.964, 1.007]. Both optimized compilers used one codegen unit, ThinLTO, embedded bitcode and their own training profile from the same nine non-uv workloads (87 collector entries per source). This contrast measures the combined changes in #296 and source-specific compiler optimization. The intermediate T1 build passed qualification and had no timed arm.

| Compiler | Cold A seconds, one per held-out cohort | First B median (s) | A no-op median (s) | B no-op median (s) |
| --- | ---: | ---: | ---: | ---: |
| Shipped Ohm −18 | 49.12, 48.66 | 6.605 | 0.383 | 0.392 |
| Optimized −22 base | 47.93, 48.15 | 6.722 | 0.382 | 0.389 |
| Optimized −22 with #296 | 48.59, 48.71 | 3.317 | 0.385 | 0.391 |

Each arm used the same signed −18 Cargo. Both holdouts started with fresh parent targets for each SDK. Each used all six SDK orders once. Every first edit restored a parent target, applied the full historical diff with HEAD and index still at the parent, and rebuilt exactly uv-workspace, uv-types, uv-distribution and uv-resolver. Both surrounding no-ops rebuilt zero crates. The edit reuses compiled workspace-exclusion glob patterns, adds a glob-normalization regression and extends the workspace-discovery benchmark. The timed operation checks Rust metadata. The separate compiler qualification ran five suites per optimized source, including seven native dispatch revisions under the default and next solvers. No uv executable was run in the timed check.

The six excluded discovery triples had paired medians of 1.961× over −18 and 2.015× over the optimized base. The direct threshold used one-sided α=.05/3 for three registered endpoints, a distribution-free interval lower bound above 2, both seed and relative-order medians above 2, and a positive matched comparison in each seed. The two secondary contrasts used α=.05. In the ZIP, `times.csv` lists each raw elapsed time in execution order. The cohort records include each child CPU usage receipt, Cargo output, order, and the registered scorer output for this endpoint. Previous studies contributed zero rows. [Earlier #296 evidence](https://github.com/astral-experiments/ohm-rust/tree/1e52cf83ea4481ae8e82a8a3ce47c261fcf7f450)

The timed #296 head was **a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5, format 10**, on base 556a8364f1485a03be31b1a50c250d63198a943d, format 8. Both optimized SDKs were built before BOLT. This packet supplies no measurement of the later format-12 head 26f57475. [Historical uv edit](https://github.com/astral-sh/uv/pull/20988) · [Compiler change](https://github.com/astral-experiments/ohm-rust/pull/296)

## Offline verification

Run `python3 verify.py --negative` beside `records-007.zip`, using Python 3.10 or later. The verifier reconstructs the six permutations per cohort, cross-checks each elapsed time against the matching Cargo record, checks all four dirty units and zero-work no-ops, and recomputes intervals, sign tests, cohort/order medians, and both decisions. It also rejects eight altered controls. `sha256sum -c SHA256SUMS` checks the three publication files.

The packet retains the exact numeric Cargo records and task-local package labels, with the private checkout and executable paths replaced by `$UV` and `cargo`. Digests bind the original raw streams, phase receipts and ELF/profile identities. The hardware-run certificate supplies the worker/loader and suite facts. The portable replay verifies their recorded agreement and statistics without downloading SDKs, rebuilding rustc or re-running the suites.
