# Rebuild the equal-version pair and rerun the control

This is a separate identity control, not a new version of the primary study. The original candidate archive is reused unchanged; the untreated #219 compiler and matching std were rebuilt with the candidate's exact embedded version. `matched-build-recipe.json` records the actual command/settings with task paths substituted. `runtime-identities.json` includes every file's size, mode and SHA256 in both 74-file runtimes.

Use the same pinned stage0 1.98 and patched LLVM 23.1.1 developer tree, source `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145`, described in the adjacent primary packet's [build inputs](../test-harness/build-inputs.json). Build both variants in one fixed physical source/build path. All commands below are for a new owned clone/build root; do not run the source-selection commands in a worktree containing unrelated changes. Existing HOME remains unchanged.

Prepare the exact candidate tree in the owned private Ohm clone and import the authentic measured commit object:

```sh
CONTROL=/absolute/evidence/fixed-version-control
PRIMARY=/absolute/evidence/test-harness
BUILD_ROOT=/absolute/owned/compiler-build
LLVM_ROOT=/absolute/owned/pinned-llvm
cd "$BUILD_ROOT/source"
git checkout --detach 83dd78b94b407779b669d2f4e6b95f2efea726d3
git apply --index "$PRIMARY/candidate.patch"
test "$(git write-tree)" = 3e01c5f6eb6c6cc05f43320d015b6297440d2da5
test "$(git hash-object -w -t commit "$PRIMARY/candidate-commit.raw")" = 0a72d98143f01fc7db249147df140d2797d7cf73
git checkout --detach 0a72d98143f01fc7db249147df140d2797d7cf73
```

The public source commit has different publication metadata; the supplied raw commit object reconstructs the measured version. Use the exact submodules, task-owned stage0/archive cache and build tools bound by the primary packet. Substitute only the two declared path placeholders in `bootstrap.matched.toml`:

```sh
mkdir -p "$BUILD_ROOT/cargo-home" "$BUILD_ROOT/tmp" "$BUILD_ROOT/bootstrap-cache"
python3 - "$CONTROL/bootstrap.matched.toml" "$BUILD_ROOT/bootstrap.matched.toml" "$BUILD_ROOT" "$LLVM_ROOT" <<'PY'
from pathlib import Path
import sys
source, target, build, llvm = sys.argv[1:]
text = Path(source).read_text().replace('<BUILD_ROOT>', build).replace('<LLVM_ROOT>', llvm)
assert '<BUILD_ROOT>' not in text and '<LLVM_ROOT>' not in text
Path(target).write_text(text)
PY
```

Run this same canonical command for each source selection:

```sh
env -i HOME="$HOME" PATH=/usr/bin:/bin \
  CARGO_HOME="$BUILD_ROOT/cargo-home" TMPDIR="$BUILD_ROOT/tmp" \
  CARGO_BUILD_JOBS=24 CARGO_INCREMENTAL=0 CARGO_TERM_COLOR=never \
  LANG=C.UTF-8 LC_ALL=C.UTF-8 LD_LIBRARY_PATH="$LLVM_ROOT/lib" \
  taskset -c 0-23 ./x build --config "$BUILD_ROOT/bootstrap.matched.toml" \
  --build-dir "$BUILD_ROOT/build" --stage 1 -j 24 library
```

The configuration fixes opt3, ThinLTO, CGU1 for compiler and std, debug1, no PGO, no build incremental/debug assertions and the patched X86 LLVM. Freeze the full candidate stage1 and matching std before selecting the control baseline. Move the live `stage1` and `stage1-std` directories into owned archives before rebuilding so the next arm builds matching std. Preserve all compiled runtime/std bytes. Exclude only the optional `lib/rustlib/src` and `lib/rustlib/rustc-src` convenience source trees, equally in both packages. Include the same fixed LLVM runtime files and shipped SDK12 Wild auxiliary in both, as recorded in the manifests. Keep the build-only LLVM library path out of workload environments.

For the untreated control, keep HEAD at the measured candidate commit solely to supply `cfg_version`, but select the exact untreated index/worktree:

```sh
git read-tree --reset -u 83dd78b94b407779b669d2f4e6b95f2efea726d3
test "$(git rev-parse HEAD)" = 0a72d98143f01fc7db249147df140d2797d7cf73
test "$(git write-tree)" = 5628904b977dee390dd085b34bc911c8e9ecb4eb
git diff --quiet
git diff --cached --quiet 83dd78b94b407779b669d2f4e6b95f2efea726d3
```

Run the identical build command again and freeze the untreated runtime/std. `baseline-control-source.patch` records the exact HEAD-to-compiled-source difference. Both full `rustc -vV` outputs must match. This deliberately controlled version is not an authentic claim that the untreated source contains the candidate patch. The metadata layouts differ and equal version labels cannot reject cross-arm artifacts: require physically separate runtime, target, shared-build and Cargo-home banks, seeded only with source and locked package archives. Prove no shared regular-file inodes or imported compiled state before warming.

Use uv `3d82a6ad5a7eab8c68236553b7846517767c4b61`, released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35` (binary SHA `75eff38f65a16abae0a7b9603fc4fa02c846a0b6cb60a72d08b68393849b914b`), fixed Wild and the unchanged primary profiler/adapter. Follow the primary packet's [workload preparation and single-invocation example](../test-harness/BUILD-AND-SAMPLE.md), with this packet's single-block schedule and equal-version runtime pair replacing its primary/cross-host design. The adapter source and all 640 profiler-file identities remain in [collection-tools](../test-harness/collection-tools/README.md).

Warm and trace both original `check --locked --offline -j 8` and `test --no-run --locked --offline -j 8` commands with `-Zohm-defaults=yes` in each independent arm; require zero compiler launches before admission. Capture the actual six-role dirty test-build compiler multiset. Select exactly one `--test --crate-name uv crates/uv/src/lib.rs` compiler with `--emit=dep-info,link`, excluding `-Zlink-only`. Preserve ordered argv/environment; allow only the freshly bound compiler-context directory substitution for comparison and removal of the unusable Cargo jobserver variable for standalone replay. Native and no-counter replays must reproduce the exact per-arm ELF, diagnostics and output mode/size. Establish fresh output hashes, symbol/dynamic/entrypoint inventories and the test listing; historical hashes are not universal admissions.

Pin the controller to CPU8 and all rustc worker threads to CPUs0–7. Settle once for 30 seconds at stage entry. Require the original strict host checks, plus the prospectively bound typed timer observer: all three known heavy services inactive, all three concrete next deadlines at least 120 seconds away, otherwise wait outside pairs. Repeat timer admission only between completed restored pairs; never between the two arms. Every measured before/after host check remains active.

Each arm receives the exact fixed-length BEFORE edit and excluded native preparation/capture, then the paired AFTER edit and the selected rustc invocation under Callgrind with `--trace-children=no`. Real linking remains enabled; only this original rustc PID and its worker threads contribute instructions. Preserve source restoration even on failure. There is no original-command convergence between arms. At block end, both original commands converge in both arms, source/mode/Cargo restore, and all state banks park. Preflight freshness was traced; terminal restoration is independently authenticated. Do not add per-sample convergence or change cache history.

Collect exactly eight pairs in order AB, BA, BA, AB, BA, AB, AB, BA. Analyze only a complete restored valid block: final Ir must equal summed self costs and stderr Collected; retain header offsets. Use 200,000 paired log-ratio bootstrap resamples, seed 2026102461, 99% confidence, all eight wins and a lower reduction bound strictly above 0.1%. Report the exact two-sided sign test. A valid failure stops with no extension or retry. Invalid attempts remain opaque and require a prospective operational decision before any replacement. Neither this control nor the original primary blocks are pooled; there is no native wall-time endpoint in this folder.
