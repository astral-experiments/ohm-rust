# SDK12 validation: the fixed gate was not met

The first valid SDK12 block did not confirm a current-stack instruction improvement for PR216. The study stopped at this valid failure, as prescribed.

| Result | First fixed eight-pair block |
| --- | ---: |
| Geometric instruction reduction, point estimate | 0.232654% |
| Nominal 99% paired bootstrap interval | −0.180406% to +0.542616% |
| Pairs with fewer candidate instructions | 7 of 8 |
| Required gate | 8 of 8 and lower bound strictly above 0.1% |
| Gate | Failed |

The second block was not run. There were no extra pairs, retries of the valid failure, or pooling with SDK6. Whole-command wall time and memory were not measured. The earlier SDK6 results are a separate dataset; this addendum establishes no SDK12 performance qualification.

One earlier collection was invalidated before effect analysis: preparation warmed test builds, while terminal restoration also ran `check`, which compiled previously unwarmed dependencies. Its effects remain unread and are excluded here. Both commands were then traced fresh in both independent states, and the fixed contexts, output guards, diagnostics, and runtime files were revalidated before an entirely new first block. The package guard, order, seed, sample count, and statistical thresholds stayed unchanged.

Run the exact numerical replay from this directory:

```sh
python3 -I -B reproduce.py --profiles profiles
```

The reader verifies the file manifests, fixed order, all 16 final instruction counts, self-cost sums, retained stderr counters, and 200,000 bootstrap resamples with seed `2026102361`. It reproduces the failed gate. `protocol.json` is the exact prospectively frozen protocol, including the unused second order and seed `2026102362`.

All 16 valid profiles are included. Their command headers and pathname definitions use the same sanitation as the original PR216 evidence. Every other profile byte, including instruction records, calls, positions, costs, headers, and final counters, is intact. The profile manifest records original and published hashes plus the unchanged nonpath-record hash. Only each exact `Collected` counter line is retained from stderr. Private ordered environments, machine observations, transport records, and the invalid profiles are excluded. Public replay verifies this derivative and its arithmetic; it does not recreate private collection validation.

The comparison used matched optimized developer compilers from Rust commit `f93a752e3de7404107ea5908d65c8832aee5625a`, fixed released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35`, and unchanged uv `3d82a6ad5a7eab8c68236553b7846517767c4b61`. These are fresh SDK12-source compiler builds, not a comparison against the shipped compiler binary. The complete runtime had 74 files per arm; 73, including matching standard-library artifacts, were byte-identical. The encoder's five canonical byte-equivalence tests passed. Source trees, patch, driver hashes, and backend identity are recorded in `source-identities.json`.

To rebuild, use the [existing PR216 collection recipe](https://github.com/astral-sh/ohm-rust/tree/84f26b4a541bf879f31e4cf024b4eeddac011969/benchmarks/uv-dev-loop-20261023/dep-graph-edge-encoder) with these SDK12 pins and `candidate-sdk12.patch`. Keep the same physical source/build paths and common embedded commit metadata for both arms. Materialize `${BUILD_ROOT}` and `${LLVM_ROOT}` in the included configuration using private task paths. The backend is the pinned source-built X86 LLVM identified in `source-identities.json`; its normal external-LLVM configuration retains Rust patches. With `SDK12_BUILD_ROOT` and `SDK12_LLVM_ROOT` set to those paths, the retained build environment and command are:

```sh
env -i HOME="$HOME" PATH=/usr/bin:/bin \
  CARGO_HOME="$SDK12_BUILD_ROOT/cargo-home" \
  TMPDIR="$SDK12_BUILD_ROOT/tmp" CARGO_BUILD_JOBS=24 \
  CARGO_INCREMENTAL=0 CARGO_TERM_COLOR=never \
  LANG=C.UTF-8 LC_ALL=C.UTF-8 \
  LD_LIBRARY_PATH="$SDK12_LLVM_ROOT/lib" \
  taskset -c 0-23 ./x build \
  --config "$SDK12_BUILD_ROOT/bootstrap.matched.toml" \
  --build-dir "$SDK12_BUILD_ROOT/build" --stage 1 -j 24 library
```

Archive each complete compiler runtime and freshly rebuilt matching standard library before switching source. Supply the same pinned LLVM runtime and SDK12 Wild auxiliary to both archives. Establish new machine-specific artifact identities, namespaces, argv, environments, and output references before collecting a fresh cohort; historical binary/output hashes are not portable admission values.

Warm both `cargo -Zohm-defaults=yes check --locked --offline -j8` and `cargo -Zohm-defaults=yes test --no-run --locked --offline -j8` in each independent compiler state. The primary endpoint remains only the ordinary library compiler captured from the test-build command. For each sample, use the existing equal-length comment fixture, warm Cargo on BEFORE, then profile that exact ordinary compiler once on AFTER with eight compiler threads on CPUs 0–7. The controller uses CPU 8, with the fixed 30-second stage-entry settle and no heavy local builders. Use the existing Callgrind environment adapter, omit only stale jobserver descriptors, and count the whole rustc process including its own worker threads and procedural macros; child processes are excluded. Restore source bytes/mode after each sample. Run original-command check and test-build convergence once per arm at block end, not between samples. Record host observations without filtering instruction results.

Private preflight covered the actual SDK12 compiler context and output classes. Output accounting was scoped to native/layout and typed metadata/diagnostic guards; it does not prove full DWARF or serialized-rmeta equivalence or recompute build IDs. The fixed two-block protocol remains the rule for any fresh performance claim.
