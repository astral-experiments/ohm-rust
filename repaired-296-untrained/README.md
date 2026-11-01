# Untrained repaired #296 head replication on uv #20988

The first edited `cargo check -p uv-resolver --locked --offline -j8` took **6.638 s** on shipped signed −18, **6.601 s** on untrained T1 v11, and **4.967 s** on untrained T3 v12 (arm medians).

The paired signed −18 / untrained T3 v12 ratio was **1.332×** (96.14% interval 1.298–1.346×). 12/12 exceeded 1×, with one-sided p=0.000244141.

The paired untrained T1 v11 / untrained T3 v12 ratio was **1.320×** (96.14% interval 1.305–1.350×). 12/12 exceeded 1×, with one-sided p=0.000244141. The preregistered material-replication gate passed. The independent direct 2× gate did not pass.

The registration specified two fresh counterbalanced six-triple cohorts without a discovery round. Positive evidence above 1×, the prespecified material-replication test, and direct 2× use separate decisions. The earlier 6.557→4.914 s result contributed zero rows. All three first-cohort workers exited successfully before a recursion error in the post-clock validator. A separate reader pinned the two original configs, checked all 48 existing files and replayed the 18 first edited checks and product stamps before the second cohort ran. The original nonzero terminal was preserved.

| Compiler | Cold A confirm-1 (s) | Cold A confirm-2 (s) | First B (s) | A no-op (s) | B no-op (s) |
|---|---:|---:|---:|---:|---:|
| Signed −18 | 48.967 | 48.589 | 6.638 | 0.377 | 0.382 |
| untrained T1 v11 | 76.592 | 75.368 | 6.601 | 0.377 | 0.378 |
| untrained T3 v12 | 76.596 | 75.886 | 4.967 | 0.375 | 0.378 |

Cold A is each independently initialized target. Each timing block used all six actual arm orders, an unwrapped check with fixed signed −18 Cargo, a settled zero-dirty A, the full unstaged edit at parent HEAD, and a zero-dirty B repeat. Each edited check rebuilt exactly uv-workspace, uv-types, uv-distribution and uv-resolver. Each table median for first B and the no-ops is over the held-out rows. Each cold A is a single observation.

T1 is 97ad3866dfe788d2ed0bd8f945a9972423da2423, format 11, and T3 is 26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9, format 12. Both are untrained native review compilers. The two prior named heads used formats 9 and 10. The repairs changed one compiler format word per head. The native forced and unforced migration matrices each saw five compatible loads, fourteen format rejections and five empty controls.

## Replay the packet

Run `sha256sum -c SHA256SUMS` and `python3 verify.py --negative` here. The verifier requires only the Python 3.10+ standard library. The packet contains actual orders, all per-trial clocks in `times.csv`, normalized Cargo output, checked source and loaded SDK identities, and native qualification summaries. The worker checked the actual binaries and product data before issuing the saved closures.

## Run Cargo independently

Use Linux x86-64 with Python 3.11+, a quiet fixed CPU set, Git, a system toolchain, and an empty scratch directory. Supply a uv checkout containing tag `0.12.2`, parent `b60da7ad468d5bb8a6b3dee7d7e54a47981f23c7` and direct child `1979349b68dbaa936b98c0f409c1df0598fa72f1`. Fetch that parent's locked dependencies into a separate Cargo home before going offline. Provide the shipped Ohm −18 compiler/std and Cargo, plus complete untrained T1 v11 and untrained T3 v12 SDKs. The observed driver, launcher, source, and LLVM identities are in `qualification.json`. Signed Cargo SHA-256 is `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573`.

```sh
python3 rerun.py --describe
taskset -c 0-15 python3 rerun.py --uv "$UV" --stock "$SDK18" --parent "$PARENT_SDK" --candidate "$CANDIDATE_SDK" --cargo "$SDK18/bin/cargo" --cargo-home "$CARGO_CACHE" --scratch "$EMPTY_RUN"
python3 rerun.py --summarize "$EMPTY_RUN"
```

The adapter reads this packet and imports the evidence branch's `../ohm-uv-resolver-repro.py` after checking its hash. It uses the two registered held-out seeds; its external comparison builds 36 cold parents, compared with two snapshots per SDK in the reported heldouts. Pass `--acknowledge-rebuilt` during execution and summary for changed compiler binaries whose source and build settings you have independently checked. The portable command's configuration and seeds were mechanically verified. An external native rerun was not part of this packet.
