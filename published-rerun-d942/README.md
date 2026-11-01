# Fresh-parent rerun of the published #306 recipe

The unchanged d9422a4c program ran on the original Linux x86-64 devbox using the preserved SDKs, signed −18 Cargo and an offline registry. It rebuilt the parent from an empty target 36 times. The two original six-triple schedules were repeated with new Cargo processes, new parent builds and 12 new first-edit measurements per compiler. No earlier clock enters these results.

| First edited check | Paired median | Exact interval | Sign test | Blocks | Order strata¹ | Decision |
|---|---:|---:|---:|---:|---:|---|
| Signed −18 / T4 (>2×) | 2.1302× | [1.9639, 2.1597] | 10/12 above threshold, p=0.0192871 | 2.1065, 2.1302 | 2.1359, 2.0882 | Missed |
| Optimized T3 / T4 (>1×) | 1.0481× | [0.9631, 1.0861] | 10/12 above threshold, p=0.0192871 | 1.0205, 1.0603 | 1.0529, 1.0437 | Missed |

¹ The first stratum places T4 before the reference. The second places the reference first. Each stratum has six observations. In each of the two blocks, each permutation of the three compilers appears once. The published rule uses α=.01 for each comparison, a 99.3652% nonparametric interval, a one-sided sign test, and requires both block and both order medians to clear the relevant threshold.

| Compiler | Cold A median (s) | A no-op median (s) | First B median (s) | B no-op median (s) |
|---|---:|---:|---:|---:|
| Signed −18 | 48.0812 | 0.3788 | 6.7876 | 0.3741 |
| 007 pre-BOLT T3 (format 10) | 48.9929 | 0.3695 | 3.4059 | 0.3717 |
| Finite pre-BOLT T4 (format 12) | 48.6806 | 0.3848 | 3.2202 | 0.3850 |

All 144 invocations ran `cargo check -p uv-resolver --locked --offline -j8` on cores 0–15. Each first B built uv-workspace, uv-types, uv-distribution and uv-resolver. All 72 A/B no-ops built zero crates. The complete two-file edit was applied at parent HEAD, its tree compared with the literal child, and its index reset before timing. The published program enforced this for every edit; it records the shared patch once. The Cargo logs show successful compilation. This rerun did not run an executable, hash the resulting rmeta or trace per-child mappings; it checked SDK loaders/sysroots and exact compiler bytes during setup. The two previously qualified compilers’ five native suites and solver controls are included separately and verified against their original receipts.

The historical uv parent is `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` and its immediate child is `1979349b68dbaa936b98c0f409c1df0598fa72f1`. The edit reuses compiled glob patterns for workspace exclusions. T3 is `a34f2fffdf6f68cb07e89e0a239d30d0e2af38b5` and T4 is `fd96f14140962cc722aa1335c4449c01f3f3a286`. Each compiler had its own frozen non-uv PGO profile. The comparison includes the source and header differences. These measured SDKs predate BOLT and the later high-format repair.

This rerun uses the same machine, source, binaries and order seeds as the earlier study. Each run here prepared a new parent from scratch; the original held-outs restored two warmed parent snapshots per SDK. Local registry files were shared and host file-cache state was uncontrolled. The rerun is independent of the earlier timing rows. The KVM guest reported an AMD EPYC-Milan, 32 vCPUs and one guest thread per core. Cargo used guest CPUs 0–15 and the controller used 29. The host record does not establish a CPU quota, governor or turbo state. A second-host or rebuilt-SDK run remains unmeasured.

## Replay or run the published program

Use Python 3.11 or later. Run `sha256sum -c SHA256SUMS`, then `python3 -ISB verify.py --negative`. This checks the original native receipts, all 435 portable result files, the independent arithmetic and the unchanged published summarizer. `times.csv` records all 36 fresh parent histories from both blocks, in actual order, with the four monotonic intervals in nanoseconds.

To repeat the native experiment on another Linux x86-64 host, extract `recipe.zip` and follow `published/finite-prebolt/README.md`. Supply an empty scratch destination, a checkout with the literal commits and tag 0.12.2, a Cargo home containing the parent lock, and the three full SDKs. The strict invocation requires the recorded driver and launcher bytes. Rebuilt compilers require explicit acknowledgement and are reported separately. The archive contains source, receipts and output, with no SDK binaries.

The preserved native `records.zip` SHA-256 is `74ddfa9bcddc0115a51f9bc64df226d75842124af8cd2b5236609f774b36f8bf`. The portable source and custody `recipe.zip` SHA-256 is `40f0c764e2d3993f0ccc4e65f23ea86beb68138c288918e5bebf73e4cb3963a6`.
