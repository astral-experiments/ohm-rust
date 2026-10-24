The commands in `README.md` replay the retained profiles. To collect fresh measurements, capture Cargo’s invocation in your own uv checkout and record the paths, tools, environment and inputs described below.

The immutable published source is [1bc15b8d](https://github.com/astral-sh/ohm-rust/commit/1bc15b8d7ba87a8a9c73b5b46099dcbacf1a00c7). Its CI-only overlay is separate from the exact measured source.

Build the shipping parent `5e95666f875d0886e541650fa2807ed23ce501c6` and the tree produced by applying `candidate.patch` to it. Use one physical source/build location serially, preserve the complete first runtime, and leave Git HEAD at the parent for both builds so embedded compiler identity stays equal. Stage the candidate patch and verify tree `023f0b79e40bf0601b22e416665eb52b2b355bf0` before building it. Preserve separate writable workload banks even though their captured bytes are initially identical.

Use the pinned stage0 and LLVM inputs required by that Rust source. Replace `<BUILD_ROOT>` and `<LLVM_ROOT>` in `bootstrap.matched.toml` and `matched-build-recipe.json` with owned paths. The actual compiler settings are opt-level 3, ThinLTO, one codegen unit, debug level 1, no incremental compiler build, no debug assertions/logging and no PGO. The exact child environment consists of the recipe's explicit overrides plus the existing HOME. Run the canonical command from the common source checkout with that environment.

```sh
taskset -c 0-23 ./x build --config "$BUILD_ROOT/bootstrap.matched.toml" \
  --build-dir "$BUILD_ROOT/build" --stage 1 -j 24 library
```

Preserve the whole stage1 compiler and freshly built matching host std, dereferencing runtime symlinks. Exclude only the optional rust-src/rustc-src convenience trees. Use the same fixed LLVM runtime and published SDK12 Wild auxiliary in both archives. The measured pair has 74 files per arm, 73 byte-identical, with equal modes. Verify each driver library against its runtime manifest. Rebuilding the final tested tree after the test-only TargetTuple correction reproduced every archived runtime byte.

Use released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35` (SHA256 `75eff38f65a16abae0a7b9603fc4fa02c846a0b6cb60a72d08b68393849b914b`) throughout. Start from genuine uv child `73337b788c9057f6f3db1035d64be02b61b6ce7b`, tree `c77063c04fe7987c70d15b00acca1e44e21ac98d`, in an owned checkout. Seed registry archives/indexes, then warm this exact command under the baseline compiler until a traced repeat reports zero compiler launches.

```sh
cargo -Zohm-defaults=yes test --no-run --locked --offline -j 8 \
  --message-format=json-render-diagnostics
```

Keep the fixture's declared HOME, CARGO_HOME, target/shared directories, runtime, PATH, umask 077 and other environment entries fixed from warming onward. The environment already includes `OHM_BENCH_NORMALIZE_VALGRIND_ENV=1` and the pinned `VALGRIND_LIB`. Leave `LD_PRELOAD` and `LD_LIBRARY_PATH` unset for the original native commands. The supplied normalizer runs its constructor in the directly instrumented rustc process. Its behavior was checked with tool=none and actual native child exec records.

Change byte 1028 in `crates/uv-client/src/retry.rs` from the uppercase L in `// Log on transient errors` to lowercase l. The exact file hashes are `0dd1a5f75cc9f8b2998995c922ecd2925847f588ef0faedb4147c80cb7152a39` before and `2f0508e09ca5cfe500c9607029ced58cbb60777c94cc78033b52155a76bf97ea` after. Trace the original dirty Cargo command and decode each resulting numeric PID file with `env_trace.trace_records` from the included `env_trace.py` (`strace -ff -v -s 131072 -e trace=execve,execveat -o PREFIX`). Retain every numeric PID file and reject undecodable records. The actual capture had 37 compiler launches, 35 cached relinks. Select the ordinary uv library-test cached relink before looking at any comparative counts. This invocation has `--crate-name uv`, `--test`, and the argument pair `-Z link-only`. It used 519 `-L` arguments (514 dependency and five native, 518 distinct values), plus the sysroot directory.

Retain the complete ordered rustc argv and environment, cwd, rlink/object/dependency files and modes/nanosecond mtimes, source bytes/mtime, original linked ELF, toolchain files and live native linker inputs. Keep the eight frontend threads and verify no target-cpu/target-feature overrides. Each sample replays that captured relink in isolation. Give each arm an independent writable copy of the complete state. Activate each bank and matching runtime at the same absolute paths. Reset the entire writable relink unit from the same pristine image before every sample. The real linker must run and resolve external/native libraries each time.

The captured CARGO_MAKEFLAGS names read FD 5 and write FD 7. `jobserver.py` contains the exact tested descriptor/token setup extracted from the collector. Create that pipe before opening command logs, pass both descriptors to rustc with `pass_fds=(5, 7)`, provide seven available tokens plus the implicit job, and require all seven tokens back afterward. This supplies capacity eight with all available tokens. The original Cargo command scheduled sibling compilers concurrently.

Use the pinned Valgrind 3.22 binary and the supplied normalizer, built as `ohm-valgrind-env.so` with the matching headers/toolchain. First run the same original rustc command with tool=none under excluded tracing. Require the original ELF bytes/size/mode, exact stdout/stderr, input preservation and live cc→collect2→Wild execution. Compare native child argv and ordered environments, permitting the observed compiler temporary directory and GCC resolution filename to vary. Execute the original invocation verbatim. Rebind all actual hashes for a fresh fixture. Require the complete original ELF hash.

The measured command adds only the normalizer to LD_PRELOAD and prefixes the captured rustc argv as follows.

```sh
valgrind --tool=callgrind --trace-children=no --demangle=no \
  --extra-debuginfo-path="$VALGRIND_DEBUG" --log-file="$SAMPLE.valgrind.stderr" \
  --separate-threads=no --callgrind-out-file="$SAMPLE.callgrind" \
  <exact captured rustc argv>
```

Have the parent process supply the descriptor setup, original ordered environment and original cwd. Keep the workload on CPUs 0–7 and the controller on CPU 8. All rustc threads contribute to the one profile. Native cc/collect2/Wild children execute outside the counter. Retain complete raw profiles, deterministic compressed copies, stdout/stderr and all operational checks. Require final totals, exclusive self-cost sum and stderr Collected to agree. Use the final `totals:` field for the endpoint.

Block 1 has eight pairs in AB, BA, BA, AB, BA, AB, AB, BA order, seed 2026102491. On a second independent machine of the same CPU class, transfer the exact portable input image and verify every file and actual tool identity at identical absolute paths in the captured Cargo namespace. Repeat excluded native/tool=none output and environment parity first. Block 2 uses BA, AB, AB, BA, AB, BA, BA, AB and seed 2026102492. Each block requires all eight reductions positive, a 99% paired-bootstrap lower reduction above 0.1% using 200,000 log-ratio resamples, and two-sided sign p below .01. Report each fixed block separately.

Keep strict activity admission at entry and immediately before/after each measured command. The fixed maintenance rule waits only outside complete pairs for at least 120 seconds until the next bound heavy-service deadline. Wait for all required deadlines to become available. Unexpected competition invalidates the whole attempt before effects. After the block, restore source, active runtime and all original state, verify both private banks, and on the capture host converge the original Cargo command with zero compiler launches. The replica restores its original source/runtime and parks both banks. Post-restoration activity is recorded observationally. Only after the complete operational and raw-profile gates pass may the numerical reader inspect effects.
