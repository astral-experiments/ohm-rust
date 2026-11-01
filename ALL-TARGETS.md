# A historical uv parser edit checked 1.855× faster with the packaged GNU −22 compiler

On the first check after uv #21076, the signed Ohm −18 compiler and standard library took a median **3.064 s**, compared with **1.662 s** for the first packaged GNU −22 compiler and standard library. Both arms used the same signed −18 Cargo executable. The median of the 12 paired speedups was **1.8551×**, with an exact distribution-free interval of **[1.7637×, 1.8916×]** under the usual independent-pairs, common-median assumption. GNU −22 won all 12 held-out pairs. None reached 2×. [Original uv change](https://github.com/astral-sh/uv/pull/21076)

```sh
cargo check --locked --offline -j8 -p uv-cli --all-targets --all-features
```

The command checks the ordinary library and its unit-test configuration. Both enable the default feature and the otherwise empty `self-update` marker. The test configuration also uses the `insta` dev-dependency. At this historical revision, uv’s CI uses the two target and feature flags with **workspace Clippy** and `-D warnings`. The measurements here cover **Cargo check for the single uv-cli package**. No tests ran. [Historical lint workflow](https://github.com/astral-sh/uv/blob/9f5808695af96f92daf680ded0ebe939869241ed/.github/workflows/check-lint.yml#L130) · [Contributor instructions](https://github.com/astral-sh/uv/blob/9f5808695af96f92daf680ded0ebe939869241ed/CONTRIBUTING.md#L164)

## What changed

The worker restored the original parent `9f5808695af96f92daf680ded0ebe939869241ed` and applied the complete two-file patch from its immediate child `79ef389e39e3b21a5443b7ab25e890f33b0dadf9`. Its index and HEAD stayed on the parent throughout the edited check. The changed CLI parser reports invalid `uv version --bump foo` input with usage information. The other changed file adds and updates the end-user snapshots in uv’s version tests. Both commits have the same Cargo.lock blob, `c1601688714997cf9380248c63a6bbb32537224d`. The ZIP verifies the Git object IDs for both commits, all paths needed to prove the complete tree difference, the two before/after blobs, and the full patch. The original offline package bank contained 683 locked packages.

A separate native probe compiled and linked the actual all-feature library from fresh targets on both revisions with each compiler. Both compilers produced clap exit code 2 in both cases. The parent emitted only the initial error. The edited parser also emitted the usage and help lines, with its original newline and spacing. The library and linked probe digests changed on each arm. Native builds and probes ran outside every measured check.

```text
error: invalid bump component `foo`

Usage: uv version [OPTIONS] [VALUE]

For more information, try '--help'.
```

## Paired wall times

The six discovery pairs had a median speedup of **1.8763×**, above the prospectively registered 1.8× nomination threshold. Two discovery pairs exceeded 2×. The numbers below use only the two subsequent six-pair blocks. Each block used a new independently built warm parent target per compiler, three pairs in each order, and a separate registered random seed. Each pair restored its own arm’s frozen parent history, ran an unchanged check, applied the previously unseen edit, ran the timed check, then ran a second unchanged check. The two checks around the timed edit were fresh in every pair. The unwrapped monotonic clock enclosed the ordinary Cargo process, including process startup and exit. The originals retained elapsed time to the nanosecond.

| Pair | Block 1, −18 s | −22 s | Speedup | Block 2, −18 s | −22 s | Speedup |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 3.004 | 1.703 | 1.7637× | 3.111 | 1.679 | 1.8522× |
| 2 | 3.033 | 1.618 | 1.8744× | 3.113 | 1.637 | 1.9013× |
| 3 | 3.051 | 1.842 | 1.6559× | 3.274 | 1.731 | 1.8916× |
| 4 | 3.051 | 1.647 | 1.8524× | 3.079 | 1.636 | 1.8820× |
| 5 | 3.044 | 1.677 | 1.8153× | 3.076 | 1.634 | 1.8832× |
| 6 | 3.128 | 1.684 | 1.8578× | 3.015 | 1.634 | 1.8450× |

The held-out paired median was **1.8338×** in block 1 and **1.8826×** in block 2. It was **1.8782×** when −18 ran first and **1.8486×** when −22 ran first. The median unchanged parent checks were 0.3705 s and 0.3684 s for −18 and −22. The unchanged checks after the edit were 0.3684 s and 0.3690 s.

The protocol selected the second and eleventh ordered ratios. With 12 pairs those ranks give 99.3652% coverage for a continuous common median, exceeding the registered 97.5% minimum. The one-sided exact sign test of a speedup above 1× gives 12 wins, zero ties and `p = 1/4096`. Against 2× it gives zero wins, zero ties and `p = 1`. The registered 2× bar also required the interval’s lower bound and the medians for both seeds and both orders to exceed 2×. This result falls below that bar.

## Evidence for checking both configurations

* Every ordinary first edit printed one uv-cli Cargo progress line. Both `lib-uv_cli` and `test-lib-uv_cli` advanced their separate invocation timestamps. Both preceding and following unchanged checks printed zero dirty units and preserved the respective timestamps. The ZIP retains the original time, argument vector, stdout, stderr, unit observations and child CPU usage for each clock.
* After each six-pair block, both workers restored A and repeated the first check with Cargo JSON and verbose command logging. The verifier correlates both dirty Cargo artifact messages with their successful actual rustc invocations. Both check `crates/uv-cli/src/lib.rs`, use incremental compilation, request `dep-info,metadata`, and receive the exact two feature cfgs. One uses `--crate-type lib`, while the other uses `--test` and receives `--extern insta`. Verbose output was collected outside the timed run.
* On the last ordinary B and its independently restored replay, each compiler produced the same SHA-256 for all four files, the library and test `.rmeta` and `.d`. The test metadata marker is correctly empty. Each of two incremental sessions also yielded the same sorted key/result fingerprint vector in both executions for `typeck_root` and `mir_borrowck`. The two sessions had respectively 1,104/923 and 1,139/958 entries. These counts establish equivalent stored results on a replay. They do not measure avoided queries.
* The private execution targets were temporary. The packet retains the measured per-file byte counts and digests and the per-query counts and digests. The included extractor can recompute them from a separately supplied matching final target. The published native probe evidence likewise consists of the build log, exit code, program/library hashes and actual diagnostic receipts.

## Compiler attribution

The baseline is the signed GNU/Linux x86-64 Ohm −18 release. The comparison uses the first successful GNU −22 coordinator artifact, including its compiler, standard library and release optimizations. Both ran Cargo from −18, SHA-256 `02a86ebd2d9e4f237b33f212102b81ed145eb77fae090d67e379cb6f1d70a573`. The original launchers, loaded rustc drivers, loaded LLVM and alternate packaged Cargo hashes are included. The −18 Rust source is `a4fa11bcd38280639cb94daacc125472657db6c3`. GNU −22 was packaged from `556a8364f1485a03be31b1a50c250d63198a943d` by coordinator `007da57ba792129adb5754546e1158e7e6d732b7`. See the release assets to obtain the full SDKs. [Signed −18 release](https://github.com/astral-experiments/ohm/releases/tag/ohm-1.99.0-18) · [GNU −22 job](https://github.com/astral-experiments/ohm/actions/runs/41237680370/job/124644935360) · [Packaged artifact](https://github.com/astral-experiments/ohm/actions/runs/41237680370/artifacts/13057710231)

The **1.8551×** estimates the difference between those two packaged compiler stacks on this command. Isolating the contribution of #278 requires a matched compiler comparison. An earlier library-only six-triple discovery on the same literal edit measured **1.9856×** between matched #278-off and #278-on builds, and **1.6522×** between signed −18 and the #278-on build. Its 1.8× direct-baseline gate closed. The ZIP includes that study’s six rows separately. No rows feed both analyses. [Compiler PR #278](https://github.com/astral-experiments/ohm-rust/pull/278) · [Earlier matched evidence](https://github.com/astral-experiments/ohm-rust/tree/4fa38493b14abb3f2f0dd4797f6897b9b2164930)

## Verify offline

```sh
python3 -ISB ohm-278-uv-21076-all-targets.zip --negative-controls
```

The verifier uses Python 3.11 or later and its standard library. The command checks the entire manifest, independently replays the Git tree and patch proof in memory, checks the two compiler configurations, and prints all 18 paired clocks in nanoseconds and their exact-count sign tests. Its controls reject a dropped arm, wrong seed, fake fresh edit, dirty no-op, inconsistent clock, unchanged test stamp, missing rustc configuration or feature, altered product, same-count altered query-vector digest, incorrect native parser output, and wrong source patch. A separately retained matching final target can also be checked with `--target confirm-1 stock /path/to/target`.

`ORIGINALS.json` lists original relative file locators and SHA-256s, including the original small nested ZIPs. The packet expands those archives and substitutes a fixed public token for the registered dependency-cache directory and the task’s absolute workspace prefix. It records each original token’s digest and each affected member’s replacements and before/after SHA-256. The shared logical paths inside the benchmark and all clock bytes, unit digests, compiler argv flags and native diagnostic bytes survive verbatim. The complete SDKs, dependency archives and temporary targets are available separately only where their original custody and retention permit.
