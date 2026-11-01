# Same-format finite-receiver replication on uv #20988

The first edited `cargo check -p uv-resolver --locked --offline -j8` took **6.539 s** on shipped signed −18, **3.293 s** on independently trained H3 (T3 v12), and **3.090 s** on pre-BOLT T4 v12 (arm medians).

The paired signed −18 / pre-BOLT T4 v12 ratio was **2.098×** (99.37% interval 2.070–2.161×). 12/12 exceeded 2×, with one-sided p=0.000244141.

The paired independently trained H3 (T3 v12) / pre-BOLT T4 v12 ratio was **1.061×** (99.37% interval 1.039–1.089×). 12/12 exceeded 1×, with one-sided p=0.000244141. Both prespecified gates passed.

Six excluded discovery triples gave 2.129× over signed −18 and 1.049× over H3. The nomination rule required a median of at least 1.8× and strictly above 1×, respectively. The two held-out contrasts each use α=.05/7. The older format-10/12 study contributed zero timing rows.

| Compiler | Cold A discovery (s) | Cold A confirm-1 (s) | Cold A confirm-2 (s) | First B (s) | A no-op (s) | B no-op (s) |
|---|---:|---:|---:|---:|---:|---:|
| Signed −18 | 48.414 | 47.608 | 47.159 | 6.539 | 0.372 | 0.378 |
| independently trained H3 (T3 v12) | 49.801 | 48.211 | 47.801 | 3.293 | 0.374 | 0.379 |
| pre-BOLT T4 v12 | 48.212 | 50.313 | 48.658 | 3.090 | 0.378 | 0.377 |

Cold A is each independently initialized target. Each timing block used all six actual arm orders, an unwrapped check with fixed signed −18 Cargo, a settled zero-dirty A, the full unstaged edit at parent HEAD, and a zero-dirty B repeat. Each edited check rebuilt exactly uv-workspace, uv-types, uv-distribution and uv-resolver. Each table median for first B and the no-ops is over the held-out rows. Each cold A is a single observation.

H3 is the actual parent 26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9 and T4 is its child fd96f14140962cc722aa1335c4449c01f3f3a286. Both use format 12, the same 364 query kinds, ThinLTO, one compiler codegen unit, and their own nine-workload PGO profile before BOLT. Their in-tree difference is three compiler files and three run-make files. The independently evaluated finite suite ran both solvers on both trained compilers. After an unrelated edit, H3 reran the two consumers for a primitive and a pair. T4 reused both results. Both reran the unsupported wide-tuple consumer. The five official compiler suites also used the same seven-revision canonical run-make oracle.

## Replay the packet

Run `sha256sum -c SHA256SUMS` and `python3 verify.py --negative` here. The verifier requires only the Python 3.10+ standard library. The packet contains actual orders, all per-trial clocks in `times.csv`, normalized Cargo output, checked source and loaded SDK identities, and native qualification summaries. The worker checked the actual binaries and product data before issuing the saved closures.

## Run Cargo independently

Use Linux x86-64 with Python 3.11+, a quiet fixed CPU set, Git, a system toolchain, and an empty scratch directory. Supply a uv checkout containing tag `0.12.2`, parent `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` and direct child `1979349b68dbaa936b98c0f409c1df0598fa72f1`. Fetch that parent's locked dependencies into a separate Cargo home before going offline. Provide the shipped Ohm −18 compiler/std and Cargo, plus complete independently trained H3 (T3 v12) and pre-BOLT T4 v12 SDKs. The observed driver, launcher, source, profile, and LLVM identities are in `qualification.json`. Signed Cargo SHA-256 is `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573`.

```sh
python3 rerun.py --describe
taskset -c 0-15 python3 rerun.py --uv "$UV" --stock "$SDK18" --parent "$PARENT_SDK" --candidate "$CANDIDATE_SDK" --cargo "$SDK18/bin/cargo" --cargo-home "$CARGO_CACHE" --scratch "$EMPTY_RUN"
python3 rerun.py --summarize "$EMPTY_RUN"
```

The adapter reads this packet and imports the evidence branch's `../ohm-uv-resolver-repro.py` after checking its hash. The external comparison uses the two registered held-out seeds and builds 36 cold parents, compared with two snapshots per SDK in the reported heldouts. Pass `--acknowledge-rebuilt` during execution and summary for changed compiler binaries whose source and build settings you have independently checked. The portable command's configuration and seeds were mechanically verified. An external native rerun was not part of this packet.
