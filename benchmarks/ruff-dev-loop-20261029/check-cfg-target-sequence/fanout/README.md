# Instructions across five rustc processes after a TextRange API change

Ruff added `TextRange::to_std_range` in `61a49c89eb809a238b01616b0a50a0f785702891`. We removed that hunk from a checkout at `93526150df078af7dd8fc4226cdcc469539d2001` and added it back. The registered command for both directions was `cargo +ohm test -p ruff_text_size --no-run --locked --offline -j4`.

Each edit compiled `ruff_text_size` and its four test harnesses, `auto_traits`, `constructors`, `indexing` and `main`. The four final links ran GCC and the stock Wild. An immediate unchanged command reported all 11 units fresh. All four test executables and ten tests matched the independently empty build. A separate Rust program printed `1..4` when the method was present and failed to compile with E0599 when it was absent. The source hunk, external program and raw file hashes are included here.

| API change | Median instructions across five rustcs, parent | Median instructions across five rustcs, PR | Median paired reduction (95% interval) |
| --- | ---: | ---: | ---: |
| Add `to_std_range` | 1,220,728,360 | 1,212,804,981.5 | 0.656% (0.571% to 0.820%) |
| Remove `to_std_range` | 1,218,868,375.5 | 1,211,065,090.5 | 0.647% (0.603% to 0.711%) |

All 12 pairs improved in each direction. The exact two-sided sign-test p-value was 0.000488 for each, or 0.000977 after Holm correction. The `fill_well_known` calls across the five compilers went from 11,167,137 to 3,481,615 median instructions in both directions. The four native links still ran in every command. There was no timing endpoint in this experiment. The separate single-crate whole-Cargo timing result remains in the [original report](../README.md).

## What was counted

The endpoint is the sum of the guest instructions in the five complete rustc invocations. The Linux tracer recognized the actual compiler executable and recorded the kernel arguments and jobserver pipes. It then ran that compiler under Callgrind in the same PID with the original argument strings, environment and descriptors. The compiler commands agreed with the preceding native acquisition. We verified the mapped compiler driver in each of the five processes. Cargo, GCC, Wild, the immediately unchanged command and test execution run outside that instruction count.

We used Cargo from the published Linux Ohm `1.99.0-19` image. The Rust parent is `722b9412d2386e70d6cffba132544c0e3ae09cd7` and the measured child is `a413698809569f88fe88e07a393c65ba81153c2b`. These are the same optimized stage1 SDKs that ran the single-crate experiment. Each SDK had its own normally trained state, preserving file contents, timestamps and hardlink classes across the target and shared build directories. Twelve adjacent A/D pairs per direction were balanced by order. The estimand and thresholds were frozen before either direction was scored. See [registration.json](registration.json).

## Inputs, archives and independent clean checks

The six registry artifacts for `serde_core`, `serde` and `serde_test` differed between the two original compiler installations. Those six files each had one hardlink. Across the six native edits, kernel traces resolved 6,719 successful file opens by the compiler and linker families. None opened the six registry files. The two installations used their original artifact bytes for every counted run.

A separate native control supplied all six parent files to both installations. Using the parent's timestamps made the child's already-warm `serde_core` stale. Cargo tried to unlink a read-only bind mount and stopped with `EBUSY` before the edit. The separately registered control retained the receiving arm's actual mode, ownership, hardlink count and nanosecond timestamp while giving both installations the same file bytes. All four source switches compiled the original five units, ran four physical links and passed the clean verdicts. After all native commands had succeeded, an isolated Python import failed while comparing the results. Running the identical comparator with its module directory present verified the retained outputs. These controls and the prior raw hashes are in [native.json](native.json).

For each role and source state we compared all sixteen declared output paths with its independently empty build. Fifteen files were byte-identical. The one variable archive held two unchanged `.rcgu.o` bodies, a complete ordered eight-entry GNU symbol-to-member index, Rust metadata and its member-name list. Replacing the one per-session seven-character object identifier at four authenticated positions (two actual members and two `.rmeta-link` entries) reconstructed the complete other archive byte for byte. We retain both raw archive hashes for every comparison. All four complete harness executables, their test inventories, and the successful external program matched without substitutions.

## Recompute or rerun

Run `python3 -IB verify.py` to validate all paired observations, per-compiler sums, raw archive commitments, source-state behavior, balanced order, bootstrap intervals and two-sided Holm-adjusted sign tests. `raw-reader.py` is the original reader for the full per-PID Callgrind outputs and native archives. The small JSON files retain the counts for all 240 compiler processes. The raw archives stay in the original run store. Their digests and sizes are recorded in [products.json](products.json).

For a new native reproduction, supply a local Ruff repository, either SDK, a dedicated hydrated offline registry and a new work directory. The runner clones Ruff, checks both source bodies, creates a private `+ohm` alias, runs both genuine dirty commands, verifies the immediately fresh inventories, runs all four harnesses, and checks both the ordinary and independently empty consumer verdicts.

```sh
python3 -IB replay.py RUFF_REPO SDK PRIVATE_CARGO_HOME NEW_WORK_DIRECTORY
```

To collect the five complete compiler profiles on each edit on Linux x86-64, compile `../replay/trace.c`, install Callgrind at `/usr/bin/valgrind.bin`, then add `--callgrind /absolute/path/check-cfg-trace` to that command. Each compiler gets its own native argument, descriptor and guest profile. Hold source and target path spellings fixed when comparing executable bytes between independent runs.

We exercised this runner on both SDKs after the registered study. The parent ran natively. The child produced ten additional complete compiler profiles and eight stock links across the two edits. Both roles ran all five dirty and empty source phases, agreed on all four complete test executables, and emitted the same external program. Its hash differs from the registered run, which compiled the reader at a different absolute path. The separate data are in [replay-sanity.json](replay-sanity.json). We also reopened the last two originally scored root archives and their independently empty counterparts, confirmed both full-byte reconstructions, and retained their individual hashes and names in [final-archives.json](final-archives.json).
