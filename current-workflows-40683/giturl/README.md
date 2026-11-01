# uv #22833 GitUrl first edit

Six counterbalanced discovery trios measured the first complete unstaged edit with parent HEAD using `cargo check -p uv-resolver --locked --offline -j8`. Median first edits took 7.199 s on signed −18, 6.144 s on #296 H3 and 5.962 s on #306 T4.

The median within-trio ratios were **1.20567×** for signed −18 / T4 and **1.02950×** for H3 / T4. The registered enrollment required ≥1.8× and >1× respectively. The discovery missed enrollment. There were no held-out trios and no confirmed speedup.

Each arm rebuilt 19 distinct named packages on every first B. All settled parent and child Cargo calls reported zero dirty packages. Each compiler was paired with the same signed −18 Cargo. The two separately PGO-trained research compilers used format 40683.

The original edit changed `uv-git-types` and two existing parser snapshots. Native A, edited B and clean B were validated on each SDK. At B, GitUrl occupied eight bytes instead of 280 on this Linux target. The native clone shared storage, mutation detached it, the inline-size observer failed with the expected type errors, and both historical whitespace snapshots passed.

Run `sha256sum -c SHA256SUMS` and `python3 -ISB verify.py --negative`. The archive contains the actual patch, cold setup, randomized order, raw per-Cargo clocks, stdout, stderr and usage, native subject and QA receipts, and the registered decision. Potentially private host-specific fields in the plan and SDK admission are represented by their original digests and the fields needed to check this case.
