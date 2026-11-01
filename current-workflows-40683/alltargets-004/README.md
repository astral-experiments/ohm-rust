# uv #20988 resolver all-targets and all-features

The 12 new held-out first edits took median 6.670 s on signed −18, 3.357 s on #296 H3 and 3.172 s on #306 T4. The joint gate failed. The independent 36-parent run did not enroll.

| Paired comparison | Median | Registered interval | Pairs above threshold | One-sided p | Gate |
|---|---:|---:|---:|---:|---|
| signed −18 / T4, threshold 2× | 2.09453× | [1.86217, 2.17051] | 10/12 | 0.01928711 | missed |
| H3 / T4, threshold 1× | 1.05348× | [0.96940, 1.20599] | 10/12 | 0.01928711 | missed |

The preregistered one-sided α was .05/16 = .003125. All five first-edit roles compiled across four packages, including both the resolver library and test harness with clap, schemars and tracing-durations-export enabled. Every eligible A, edited B and fresh B produced the original list of 99 tests and passed the full suite. The workspace regression executed zero cases on A and exactly one on both B states. The 003 host-import and terse-list stops are preserved separately.

## Discovery and reproduction

Six counterbalanced discovery trios measured the first complete unstaged edit with parent HEAD using `cargo check -p uv-resolver --all-targets --all-features --locked --offline -j8`. Median first edits took 6.698 s on signed −18, 3.361 s on #296 H3 and 3.150 s on #306 T4.

The median within-trio ratios were **2.13776×** for signed −18 / T4 and **1.07425×** for H3 / T4. The registered enrollment required ≥1.8× and >1× respectively. Twelve separately warmed held-out trios followed. Their independent threshold decisions are in verified.json.

Each arm rebuilt 4 distinct named packages on every first B. All settled parent and child Cargo calls reported zero dirty packages. Each compiler was paired with the same signed −18 Cargo. The two separately PGO-trained research compilers used format 40683.

Run `sha256sum -c SHA256SUMS` and `python3 -ISB verify.py --negative`. The archive contains the actual patch, cold setup, randomized order, raw per-Cargo clocks, stdout, stderr and usage, native subject and QA receipts, and the registered decision. Potentially private host-specific fields in the plan and SDK admission are represented by their original digests and the fields needed to check this case.
