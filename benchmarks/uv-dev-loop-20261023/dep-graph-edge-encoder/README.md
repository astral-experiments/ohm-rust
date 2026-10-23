# Encoding dependency edges during an incremental uv test build

Selecting the stored index width once per dependency node reduced instructions in the ordinary `uv` library compiler invocation. Each row is a fresh eight-pair confirmation. Every pair improved.

| Block | Instruction reduction | 99% paired bootstrap interval |
| --- | ---: | ---: |
| 1 | 0.241402% | 0.205601% to 0.282939% |
| 2 | 0.219654% | 0.174886% to 0.262463% |

The exploratory four-pair screen measured 0.218153% and missed its 0.5% cutoff. After seeing that result, we recorded a one-candidate exception before collecting either confirmation. The protocol directory contains both decisions. Each confirmation required eight wins and a 99% lower bound above 0.1%. The discovery observations contribute zero pairs to confirmation. The intervals and the per-block two-sided sign tests (p = 0.0078125 each) depend on sampling and scheduling assumptions. Whole-command wall time and the effect on later SDK releases are unmeasured.

## Replay the counts

From this directory, run `python3 -I -B reproduce.py --profiles profiles`. The reader checks the file manifest, both schedules, all 32 confirmation counts, and 200,000 paired bootstrap resamples per block. The separate discovery file preserves the failed screen. All 40 profiles are included. Command headers and file labels have been sanitized. Their metric records, cost rows, calls, and the final counters are preserved. `PROFILE-MANIFEST.json` and `DISCOVERY-MANIFEST.json` in the profiles directory identify the original and published hashes. Private ordered process environments and machine observations stay in the collection archive.

## Collect a fresh run

1. Check out `astral-sh/ohm-rust` at `6d4a7ee3200c5311112d433cb9d1da4a2630a66f`. Build it and the same tree with `candidate.patch` using the included bootstrap configuration. Substitute a private build root for `${BUILD_ROOT}`. Use Rust 1.98.0 stage0, LLVM `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145`, and `./x build --config bootstrap.matched.toml --build-dir <build> --stage 1 -j 24 library`. Build each compiler at common physical paths and save each complete runtime. The comparison used optimized developer builds with opt-level 3, ThinLTO and one codegen unit. The 75 shared runtime files matched across arms. The distinct driver hashes are in `source-identities.json`.
2. Use uv `3d82a6ad5a7eab8c68236553b7846517767c4b61` and fixed Ohm Cargo `77a6a04fbbd4717a5d134662cce99cbbe887a10441cfeb80386ee4c83695bcfe`. Warm independent build and incremental state for each compiler at identical paths. Use `cargo -Zohm-defaults=yes test --no-run --locked --offline -j8` with the workload environment. Capture each arm’s actual ordinary library `rustc` arguments and environment. `workload-context.json` records the baseline reference with path placeholders.
3. Use `dirty_fixture.py` for the equal-length comment edit in `crates/uv/src/lib.rs`. For every arm and pair, warm Cargo on the BEFORE edit. Apply AFTER and profile that ordinary compiler invocation once on CPUs 0–7 with its original eight frontend threads. Use Callgrind final Ir, `--trace-children=no --demangle=no`, and the published [Valgrind environment adapter](https://github.com/astral-sh/ohm-rust/tree/52627489f8f2bacc6204571430f682a8013a72b5/benchmarks/uv-dev-loop-20261023/attribute-proc-macro-cache/collection-tools). The standalone invocation omits stale jobserver descriptors. Restore source bytes and mode after each arm, then check that each compiler’s original Cargo check and test-build converge after the block.
4. Follow the two fixed orders and seeds in the protocol and counts. Each instruction count is the complete compiler process, including its in-process workers and macros. The source patch has five passing focused compiler tests. Its byte-equivalence test exercises the actual encoder at all four widths, extended lengths, nonempty prefixes and successive writes.
