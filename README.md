# First edit in uv-resolver after uv #20988

Twelve held-out triples measured ordinary `cargo check -p uv-resolver --locked --offline -j8` after applying the complete uv #20988 edit. The checkout stayed at parent `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7`; the unstaged working tree matched child `1979349b68dbaa936b98c0f409c1df0598fa72f1`. All three compilers used the same signed Ohm −18 Cargo. Each invocation checked `uv-workspace`, `uv-types`, `uv-distribution`, and `uv-resolver`.

| T3 comparison | Median first-edit wall | Median paired speedup | Exact median interval | One-sided p for >1× |
| --- | ---: | ---: | ---: | ---: |
| Signed −18 vs optimized T3 | 6.577 s vs 4.075 s | 1.6153× | 1.5837–1.6596× | 0.000244 |
| Optimized T1 vs optimized T3 | 5.139 s vs 4.075 s | 1.2548× | 1.2281–1.2926× | 0.000244 |

The T3 result passed the preregistered practical-improvement gate. Its sign-test p for speedup above 2× was 1.0. The nonparametric intervals have 96.14% coverage at this sample size. Speedups use the median of within-triple ratios. The interval and p-values concern this edit, command, compiler construction and machine.

## Verify the recorded study

Run from this directory with Python 3.11 or later. The command works offline without Cargo, rustc or third-party packages.

```sh
python3 -B verify.py
```

The verifier checks the packet and every member, reproduces each execution order from its seed, and cross-checks all Cargo arguments, dirty units, child CPU, raw edit clocks, no-op clocks, loaded compiler identities and worker receipts. It prints both contrasts, their intervals and one-sided sign tests above 1× and 2×, as well as both seed and order-stratum medians. Six exploratory triples are in the separate discovery directory and contribute zero held-out observations. The verifier includes the separate T1 comparison when its completed packet is present.

Within each `records-*.zip`, `times.csv` gives the raw seconds for every arm in the discovery, holdout-1 and holdout-2 cohorts. `pairs.jsonl` preserves the execution order within each triple. Each role's six original `pair-*.zip` files contain Cargo stdout and stderr, exact argv, the measured wall time and child usage. The `setup.zip` files contain the off-clock cold parent and clean parent repeat, and each edit has clean parent and child no-op receipts. Source and SDK identities, native qualification digests and the original scorer output are included. Raw commands refer to the private temporary namespace used by Cargo.

## Compiler source and attribution

The shipped comparator was rustc `a4fa11bcd38280639cb94daacc125472657db6c3`. The optimized arms were separate Linux stage2 builds of field-C `cb06be7edfc264edd87f44f58713e6ae00056c84` and its T1 and T3 derivatives. T1 projects a trait's concrete impl roster by receiver. T3 adds a recoverable ADT receiver witness to T1. Each optimized source had its own rustc-driver PGO profile trained on the same workload suite. The arms shared the external LLVM. The signed comparison therefore includes base and training differences. The T1/T3 contrast compares separately trained builds differing by the ADT witness.

`compiler-source/` contains the patches from C to T1 and T1 to T3, plus the expected Git tree IDs. In a disposable checkout of field-C, applying each patch to the index and comparing `git write-tree` to the corresponding recorded ID reproduces the measured compiler source. The release-22 port starts at `625207d26ce4449c9c95d871c88cfb50e904c09f`. Its candidate commits are `6d997cb8c8ebd3cb0e16c28c09d9855d862238a2` for T1 and `3334fcb9306dadf768e58248c4fefb53852fe02d` for T3. They bump the incremental file format from 8 to 9 and then 10. The earlier candidate commits in the original qualification records shared format 8 and could crash when reusing one another's caches. Every timed arm here started from its own compiler’s parent build. Wall times in this packet apply to the field-C PGO SDKs.

The release-22 branches were subsequently rebased onto `556a8364f1485a03be31b1a50c250d63198a943d`. The T1 head is `8833a44ae11c06f328141f5b4254729c2d7b27cc` and the T3 head is `a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5`. Each retained its predecessor’s entire compiler tree. Only the Cargo gitlink changed across these rebases. These heads have their own qualification history; the wall times above came from the field-C builds.

## Rerun on another host

Use Linux x86-64, quiet fixed CPU affinity, Python 3.11+, Git, ldd and a system C toolchain. Supply a full local uv history containing both commits and tag `0.12.2`, a Cargo home populated by `cargo fetch --locked` at the parent, the signed −18 SDK and two optimized SDKs. The measured SDK images were lost. Fresh builds can use the recorded generation, training and profile-use commands. [Compiler construction recipe](compiler-source/CONSTRUCTION.md)

Before running Cargo, the T3 entry compares the observed launchers, linked drivers and LLVM against the immutable T3 packet. By default, they must match. For independently rebuilt optimized compilers, verify their source trees and build records, then add `--acknowledge-rebuilt` to the first command. The run records the expected and actual hashes in `compiler-identities.json`. The signed −18 baseline, Cargo and external LLVM remain mandatory in either mode. The packet ships source patches, driver/profile digests and raw results; the SDKs and Cargo registry are separate inputs.

```sh
python3 rerun-T3.py --uv "$UV_REPO" --stock "$SIGNED_18" --parent "$T1_SDK" \
  --candidate "$T3_SDK" --cargo "$SIGNED_18/bin/cargo" \
  --cargo-home "$HISTORICAL_CARGO_HOME" --scratch "$FRESH_OUTPUT"
python3 rerun-T3.py --summarize "$FRESH_OUTPUT"
```

For the T1 study use `ohm-uv-resolver-repro.py`, with the optimized C SDK as parent and T1 as candidate. The T3 entry selects the two published T3 seeds and runs the same portable code. Each six-triple cohort covers all six arm orders. The study built **two fresh parent target snapshots per arm** across its two held-out cohorts and restored the relevant snapshot before each edited clock. The portable rerun builds **36 independent cold parent targets** across its 12 triples, one for every arm and trial; cold builds and no-ops are outside the edited clocks. The rerunner was checked against the saved native protocol and a controlled fixture. A second-host native replication has not been performed.

## Fresh confirmation on the cache-corrected −22 sources

A separate 12-triple comparison used the published compiler sources with incremental formats 9 and 10. The two rebuilt compilers had no compiler PGO. The median first check after the same uv edit took 6.557 s on shipped −18, 6.570 s with receiver-specific queries alone and 4.914 s with receiver recovery as well. The paired speedups were 1.336× over −18 (1.317–1.350×) and 1.328× over receiver-specific queries alone (1.310–1.353×). Each comparison improved in all 12 triples. No raw timings are shared with the field PGO study above.

The supplement verifies the six cold parent setups, 36 first edits, 72 unchanged checks, both trait solvers and nine same-path compiler installations. All three same-format installations loaded their cache and all six cross-format installations rejected it. Download the two adjacent files and run the verifier with Python 3.11 or later.

```sh
sha256sum -c records-exact22-T3.sha256
python3 -ISB records-exact22-T3.zip
```
