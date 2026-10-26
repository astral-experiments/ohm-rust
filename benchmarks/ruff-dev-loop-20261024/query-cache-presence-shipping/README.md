# Query-cache presence index on a Ruff parser edit

With compilers rebuilt using Ohm's stage-2 PGO and BOLT recipe, the presence index reduced instructions in the edited `ruff_python_parser` compilation by **0.603%**. The 99% bootstrap interval was **0.529–0.677%**. All 28 pairs favored the index. The first 14-pair block improved by 0.631% and the second by 0.575%. The earlier stage-1 experiment is recorded separately. [Stage-1 measurements](../query-cache-presence/README.md)

Whole-command wall time was mixed on a separate `ruff_linter` edit. Positive values mean the candidate took less time. Each block contains six pairs per command, split evenly between baseline-first and candidate-first.

| Command | Block A | Block B | All 12 pairs |
| --- | ---: | ---: | ---: |
| `cargo check -p ruff` | −2.07% | −0.63% | −1.35% |
| `cargo build -p ruff` | +5.96% | −4.59% | +0.82% |
| `cargo test -p ruff --no-run` | −0.37% | −2.56% | −1.46% |

The predeclared thresholds were a slowdown of at least 2% overall or 5% in either block for any command. All three commands passed. These 36 pairs do not establish a wall-time improvement or noninferiority.

## Workload

The comparison uses Ruff at `93526150df07`, one fixed PGO Cargo at `e3e879803f99`, and separately trained compiler builds at `d296901b578f` (A, includes #221) and `92181057fa04` (B, adds the presence index). The exact source trees and Cargo, driver, PGO profile, and BOLT profile digests are in the dataset. These are custom builds of the compared commits. [Ruff revision](https://github.com/astral-sh/ruff/tree/93526150df078af7dd8fc4226cdcc469539d2001) · [Compiler change](https://github.com/astral-sh/ohm-rust/compare/d296901b578fb31727267d7358c4aad266625fc9...92181057fa048f4f536c417026ae0fcd5d7ffa09) · [Cargo revision](https://github.com/astral-sh/ohm-cargo/commit/e3e879803f99e1bb0fc5335fe04944316c63aa76)

- Each arm had its own target and shared build directories. The workload used eight build jobs and eight compiler threads, the locked dependencies, offline mode, and `--config profile.dev.build-override.opt-level=0`.
- For the instruction comparison, warm `cargo check -p ruff` on the original sources. Alternate `Invalid Syntax` and `Invalid syntax` in the existing error string in `crates/ruff_python_parser/src/lexer.rs`. For each revision, run both compilers in the order in the dataset. A transparent Cargo wrapper ran Callgrind only on the parser's source compilation. Each sample is the final `Ir` total for that original compiler PID. Cargo and the remaining compilation jobs ran natively.
- For native command time, alternate `"suppression"` and `"suppr3ssion"` in the `UnusedNOQAKind::Suppression` arm of `crates/ruff_linter/src/rules/ruff/rules/unused_noqa.rs`. The endpoint ends when Cargo returns. The test command builds the targets needed to test Ruff. No Ruff tests ran. Each arm retained its own baseline public API hash after the edit.

Both primary blocks used fresh warmups and balanced order, seven AB and seven BA. Across all blocks the confirmation ran 144 Cargo commands, including 16 warmups. The preliminary screen was excluded. The analysis used the geometric mean of the paired instruction ratios and 50,000 bootstrap draws stratified by block and order. The interval is approximate because cache states within a block can be correlated. The exact one-sided randomization test enumerated 11,778,624 balanced assignments and gave `p = 1 / 11,778,624`, below the preallocated `6.103515625e-7`. Both blocks improved and the lower interval bound exceeded the predeclared 0.2% floor.

The candidate compiler build completed. Its packaging check then failed when packaging added a hard link to Wild and each of its two license files. A separate check verified the links and file contents and qualified the built runtime before measurement. The dataset records digests for the failed packaging check, the link verification, and the subsequent runtime check.

## Replay

Run this from this directory with Python 3.12 and NumPy 2.3.5. It recomputes the confidence interval, exact test and native thresholds from the published pairs, then checks every result against the saved values.

```sh
PYTHONPATH=. python3 -B replay.py
```

The dataset includes all 64 pairs, both run orders, and a digest of each raw profile or timing record. The full private Callgrind and process logs were checked by two readers. [Paired data and compiler identities](data.json) · [Expected results](expected.json)
