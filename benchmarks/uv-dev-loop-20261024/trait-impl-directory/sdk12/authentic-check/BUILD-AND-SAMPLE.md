# Build and sample lifecycle

The original same-host design is incomplete: its inverse stopped on the strict activity guard after 12 observations, all excluded and unread. Before any fresh-host samples, the inverse was moved prospectively to a new dedicated machine of the same AMD EPYC-Milan CPU class and workload placement. Both parent states were independently warmed and all 39 transition contexts, ordered environments, and exact per-arm outputs revalidated. The original inverse order, seed, sample count, and statistical gate remain unchanged. The two accepted blocks are reported separately; this is a prospective cross-host replication, not a completed same-host replication. `cross-host-replication.json` pins the decisions and distinct input records.

The self-contained commands in `README.md` replay the published counts and raw profile records. A new performance experiment also needs its own authenticated toolchain, private fixture, actual compiler contexts, output expectations, and host admission. The following describes the measured lifecycle; copying a previous target directory or repeatedly compiling the child does not reproduce it.

## Matched compiler build

Use a fresh owned Rust checkout and one physical source/build location for both builds. The embedded compiler commit is the SDK12 commit `f93a752e3de7404107ea5908d65c8832aee5625a` in both arms. Preserve that detached HEAD while applying and staging the source patches. `baseline-sdk12.patch` reconstructs the earlier measured #219 baseline tree. Its published source adds only a CI selector, retained separately as `baseline-measured-to-shipping-ci.patch`. `candidate.patch` applies to that published parent. Check the resulting staged tree IDs against `source-identities.json` before each build.

For example, inside a fresh owned checkout:

```sh
git checkout --detach f93a752e3de7404107ea5908d65c8832aee5625a
git apply /absolute/evidence/baseline-sdk12.patch
git add -A
git write-tree
```

The build uses the exact pinned LLVM/stage0 inputs and the settings in `bootstrap.matched.toml` and `matched-build-recipe.json`. Substitute only `<BUILD_ROOT>`, `<LLVM_ROOT>`, and `<WORKSPACE_ROOT>` with the corresponding fresh locations. Retain the config's release channel and the recipe's explicit child environment, including its LLVM loader path. The recipe builds an optimized stage1 compiler and fresh matching host standard library with opt level 3, ThinLTO, one codegen unit, debug level 1, no compiler incremental build, and no PGO or BOLT. Follow the recorded canonical `./x` argv rather than an unrelated developer profile.

Archive the complete baseline stage1 runtime with dereferenced runtime symlinks, excluding only the recipe's declared optional source-tree convenience links. Add the fixed released SDK12 Wild auxiliary unchanged. Preserve this archive before applying `baseline-measured-to-shipping-ci.patch` and then `candidate.patch` at the same source path. Keep the detached embedded HEAD unchanged, stage the changed files and verify the measured candidate tree, rebuild the compiler and matching standard library with the identical recipe, and archive the candidate independently. The baseline CI overlay changes no compiler or test source. The recorded pair has 74 runtime files per arm, of which 73 are byte-identical; the driver is the distinguishing file. Executable name and `rustc -Vv` alone cannot identify an arm.

The metadata wire format is unchanged, but the added query changes the incremental dep-kind layout. Never cross-load incremental state between the compilers. The fixed Cargo binary is released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35`, SHA256 `75eff38f65a16abae0a7b9603fc4fa02c846a0b6cb60a72d08b68393849b914b`, used at one physical path in both arms.

## Independent pristine parent states

Clone uv into a fresh owned fixture from authentic parent `cd050e4b18dfc4c5354ead52fa2b54756121b091`. Its child is `73337b788c9057f6f3db1035d64be02b61b6ce7b`; the exact change is included as `uv-upstream.patch`. The Cargo.lock SHA256 is `74b8ae645079c1be4797acce1fd2361ecb15dddf0ba13b2f78d52bee919bd57f`. Seed only authenticated registry archives and index data. Do not import any prior target, shared-build, package build, or incremental state.

Install the selective compiler recording/profiling wrapper before either parent warm so its presence does not change between warming, preflight, and measurement. Keep the same fixed Cargo, command, environment, working directory, affinity, and umask throughout. The actual command is:

```sh
cargo -Zohm-defaults=yes check --locked --offline -j 8 \
  --timings --message-format=json-render-diagnostics
```

Independently compile the parent under each runtime and prove a subsequent identical command invokes no compiler or toolchain probes. Before either arm sees the child, freeze that arm's complete parent source/target/shared/package-home state and authenticate every entry plus the recorded parent outputs. Keep the runtime identity stable within each arm. The two arms activate at common physical paths, but retain their intentional distinct Cargo namespace components. Verify actual contexts rather than substituting fabricated matching paths.

Run excluded first-transition preflights from independent copies of those pristine banks. Retain all 39 compiler argv and ordered environment vectors, exact per-arm metadata and dep-info, diagnostics, artifact roles, and restoration. Select exactly the ordinary `uv` library: `--crate-type lib`, input `crates/uv/src/lib.rs`, `--emit=dep-info,metadata`, and no `--test`. Keep all eight compiler frontend threads and the original live jobserver descriptors. The existing compiler-only Callgrind/environment-normalizer adapter instruments that one child, including its threads and in-process macros, with child tracing disabled. Other compiler children remain native. Validate the full profiler/runtime inventory and that instrumentation leaves actual contexts and expected outputs unchanged.

## Fixed first-transition samples

Before each complete pair, independently materialize both full pristine writable banks with new regular-file inodes, authenticate and park them, and check that the arms share no writable regular inode. This preparation occurs outside the measured calls and before the fixed maintenance-window admission. Neither arm may mutate the other bank.

For each arm in the prescribed pair order:

1. Activate its prepared bank and stable runtime; restore the exact parent source.
2. Authenticate the bank and all parent output hashes. Run the original parent check and require zero compiler invocations.
3. Apply the authentic child edit. Observe the unchanged strict activity guard.
4. Run the exact Cargo command once, instrumenting only the selected ordinary library compiler. Record every compiler context, diagnostics, outputs, profile, and matching profiler stderr.
5. Observe the unchanged strict activity guard, validate the exact expected roles/contexts/outputs, and park the arm.

Wait for maintenance only between complete pairs. The prospective fixed margin is 270 seconds with a 1,200-second maximum wait and 5-second observations for the two declared heavy services. A missing or transitioning deadline is a wait state, never a free window. Ordinary short services remain subject to the original occupancy rule. Interference during a pair invalidates the complete attempt; it does not permit deleting or reordering a sample. No service or system policy is changed.

Block 1 orders are `AB, BA, BA, AB, BA, AB, AB, BA`; block 2 orders are `BA, AB, AB, BA, AB, BA, BA, AB`. Seeds are 2026102471 and 2026102472. Each block contains exactly eight pairs. At block end, restore parent source and the fixed Cargo, converge and freshness-check both touched arms, and park both banks. The post-cleanup activity observation is retained. A persistent owner and owned stdin/stdout/stderr keep collection and cleanup alive if the observer disconnects.

Before reading effects, validate the complete fixed schedule, 34 command records, all first-transition and parent-freshness proofs, 39 compiler contexts per transition, 33 strict activity checks, exact per-arm outputs, and successful restoration. Then authenticate each selected profile and require final Ir to equal the sum of all self-cost rows and its matching stderr `Collected` and `I refs` counters. Retain the header summary separately. Only then compute the frozen 200,000-resample paired statistics and the all-wins/lower-bound/sign gates. A valid gate failure ends the candidate; no extension or replacement based on observed effects is permitted.

This endpoint excludes the rest of Cargo's instruction work and does not establish whole-command wall time, native speed, or test-build performance. Cross-arm serialized metadata residuals remain explicitly unclassified, as described in `output-limitations.json`; output admission uses exact per-arm identities throughout.
