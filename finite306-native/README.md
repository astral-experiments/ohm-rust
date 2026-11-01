# First native builds after uv #20988

The change caches compiled workspace exclusion globs. Each arm applied the complete two-file edit to a warm parent checkout and left HEAD at the parent. Cargo was the same signed −18 binary in both arms. The candidate was the independently trained Linux pre-BOLT compiler from [Ohm Rust #306](https://github.com/astral-experiments/ohm-rust/pull/306), with its associated standard library. [Original uv edit](https://github.com/astral-sh/uv/pull/20988)

| Ordinary first-edit command | Signed −18 | Optimized T4 | Median paired ratio | Paired range | T4 faster | ≥2× | Held-out pairs |
|---|---:|---:|---:|---:|---:|---:|---:|
| `cargo build -p uv-resolver --lib` | 9.565 s | 7.630 s | **1.254×** | 1.214–1.299× | 6/6 | 0/6 | 0 |
| `cargo test -p uv-resolver --lib --no-run` | 8.923 s | 6.886 s | **1.307×** | 1.214–1.328× | 6/6 | 0/6 | 0 |

Both commands used `--locked --offline -j8`. Each discovery row above summarizes six balanced pairs, with three signed-first and three T4-first. The arm times are medians. The ratio is the median of the six within-pair ratios. Each command had to reach 1.8× to earn two fresh six-pair confirmations.

Both discoveries stopped below 1.8×. No held-out pairs or confirmation intervals exist for these native commands. The resolver `cargo check` measurements are a separate endpoint. [Same-format check confirmation](../finite-format12-prebolt/README.md)

| Discovery setup or repeat | Build −18 | Build T4 | Test preparation −18 | Test preparation T4 |
|---|---:|---:|---:|---:|
| Cold A (one observation per cell) | 73.569 s | 72.173 s | 73.158 s | 74.256 s |
| First fresh A repeat (one per cell) | 0.413 s | 0.394 s | 0.389 s | 0.427 s |
| Restored A no-op (six per cell) | 0.370 s | 0.373 s | 0.376 s | 0.385 s |
| Edited B no-op (six per cell) | 0.381 s | 0.379 s | 0.386 s | 0.380 s |

Build cold-started signed first, test preparation cold-started T4 first. Each arm had its own cold A target per cohort. Each paired first edit restored that arm’s A sources, target, home and Cargo state, checked its unchanged A, applied the original patch, then timed the user-facing Cargo command and checked the unchanged B. Every first edit rebuilt exactly `uv-workspace`, `uv-types`, `uv-distribution` and `uv-resolver`. The retained rows include all four before/after compiler invocation stamps.

## Product and compiler checks

For build, the worker hashed the actual resolver rlib, listed its archive members and verified identical output across each immediate no-op. After the timers, the real rustc linked a small external user of `ResolutionMode` against the exact measured A and B archives. All four clients printed `highest|lowest|lowest-direct`.

For test preparation, Cargo identified the concrete ELF. Its full 99-test inventory remained identical on each observation. After timing, each compiler’s A and B executables ran all 99 tests through the same Cargo without recompilation. Both commands also ran the child’s exclusion-pattern regression separately after the timers. Test execution time contributes zero to the first-edit clocks. Whole-file digests, archive members, listings, argv, stdout and stderr are retained for these checks.

The active format-12 T4 came from commit `fd96f14140962cc722aa1335c4449c01f3f3a286`, using nine training workloads and profile `70892579bb848a9eb268d1d74538e53b849f3de4c0df6a05a504f071bdb869ea`. The runtime loader resolved the T4 driver as `f10dcce341a10cd09646bdd778952f6ad32631854d3197f917da8ae47d06d642` and the signed −18 driver as `a7648053cbad025c1bb0bebb7009ce9e1515bdb02e8c30bef29b52a881e5b8de`. The signed −18 Cargo SHA-256 was `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573` in all arms. Each native run verified the active SDK issuer under the exclusive lock and hashed its installed compiler, Cargo and loader-selected driver before its cold build. `qualification.json` retains the five compiler suite outcomes and both solver fixtures. The runtime checks for each compiler appear in `identity.json`. The direct comparison includes the compiler and standard-library differences between these images.

## Replay the recorded evidence

```sh
sha256sum -c SHA256SUMS
python3 -ISB verify.py
python3 -ISB controls.py
# Optional: emit every cold build, first edit and no-op as a flat table.
python3 -ISB verify.py --csv replay.csv
cmp clocks.csv replay.csv
```

`records.zip` includes each original setup and pair ZIP, the exact complete edit, both source trees and lock identities, positive phase closures, the original scorer output and off-clock transcripts. The packet retains the original SHA-256 for each cohort plan and SDK intake record while shortening their storage paths. The CSV orders all cold, first-edit and no-op calls by their registered sequence. The temporary compiler paths printed by Cargo are the original private namespace paths. SDK and product binaries are represented by their measured hashes.

## Repeat the physical study

The original registered runner is `work/uv-workflow-search/phase-costs/trait-rosters/build-test-ordinary/finite306/run.py` in the research checkout. With the original verified historical source and registry bank, both SDKs, and the original four host leases, run `taskset -c 29 /usr/bin/python3 -ISB "$D/run.py" all`, setting `D` to that directory. To run one phase use `"$D/run.py" build discovery` or `"$D/run.py" test discovery`. The packet includes the exact registered implementation, product probes and registration under `measurement/`. Supply the external toolchains, registry and runner dependencies to repeat the native timings. The offline verifier uses Python’s standard library.
