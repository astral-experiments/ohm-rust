# The literal uv #21076 edit on two shipped Ohm stacks

On the original parent of uv #21076, the first `cargo check -p uv-cli --locked --offline -j8` after applying the complete uncommitted edit took **3.099 s on signed −18 and 1.678 s on signed −21** (medians across 12 held-out triples). The median within-triple speedup was **1.851× [1.784, 2.148]**. All 12 comparisons favored −21 and four exceeded 2×. The preregistered 2× gate did not pass (one-sided exact sign-test p = 0.9270 against 2×). [uv #21076](https://github.com/astral-sh/uv/pull/21076)

| Held-out comparison | Median paired ratio | Registered interval | Trials above 1× | Trials above 2× |
| --- | ---: | ---: | ---: | ---: |
| Complete signed −18 / complete signed −21 | 1.851× | [1.784, 2.148] | 12/12 | 4/12 |
| −18 rustc/std / −21 rustc/std, both with signed −18 Cargo | 1.881× | [1.794, 2.138] | 12/12 | 4/12 |
| Signed −18 Cargo / signed −21 Cargo, both with −21 rustc/std | 1.0003× | [0.967, 1.018] | 6/12 | 0/12 |

The six excluded discovery triples produced a whole-stack ratio of 1.840× and opened two independently warmed six-triple cohorts. Every cohort contained all six arm orders. The whole-stack medians were 1.808× and 1.899× by held-out cohort, and 1.848× and 1.948× with −21 scheduled first and second. The intervals use the second and eleventh ordered ratios among 12 (99.3652% central coverage under the registered sign model). These observations share a machine and workload.

Each of the 54 first edits rebuilt just the uv-cli library. Its metadata changed and the 108 surrounding no-ops reported zero dirty units. Each stack's linked parser produced the original error at A and added the expected Usage text at B. Ordinary commands were unwrapped. A separate untimed first edit captured the kernel-observed Cargo and rustc argument vectors for every arm.

This compares complete compiler and standard-library releases, including #278. The third arm measures the Cargo change on −21; there is no −18-rustc/−21-Cargo arm. The separate matched #278 off/on study measures the individual patch. Its clocks are excluded here. [Ohm rustc #278](https://github.com/astral-experiments/ohm-rust/pull/278) · [Ohm −18](https://github.com/astral-experiments/ohm/releases/tag/ohm-1.99.0-18) · [Ohm −21](https://github.com/astral-experiments/ohm/releases/tag/ohm-1.99.0-21)

Extract the archive and run `python3 -ISB verify.py` in `shipped21/` with Git available. Add `--controls` to exercise the tamper, wrong-Cargo and pairing refusals. The reader uses the original Cargo records, elapsed seconds, per-run output stamps, kernel exec records and linked parser observations. Each of the nine arm histories carries full A/final-B metadata digests. Host-specific input paths and the private registry directory are omitted or replaced in the portable copies. The original file hashes are in `provenance.json`.
