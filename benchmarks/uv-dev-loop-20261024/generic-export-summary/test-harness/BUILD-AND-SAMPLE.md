# Rebuild and sample

`reproduce.py --profiles profiles` verifies the saved numerical evidence. A new performance run also needs its own private fixture, output/context admission and fixed protocol before candidate effects. The following describes the actual build and sample lifecycle; the historical file hashes are evidence identities, not universal expected outputs.

Use a private clone of `astral-sh/ohm-rust`, one fixed physical source/build path and the pinned submodules. Reconstruct the baseline at HEAD `f93a752e3de7404107ea5908d65c8832aee5625a`, applying `baseline-sdk12.patch` without changing HEAD. `git write-tree` must produce `5fb5081a44fa86a406ce696a94b03dce7a9b9f74`. Published #219 (`83dd78b9`, tree `5628904b`) differs only by a CI selector, with identical compiled sources. This study reused its already-built archived compiler/std; its embedded identity remains f93.

Use Rust stage0 1.98.0 and patched LLVM source `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145` (23.1.1-rust-dev), X86 only, shared Release LLVM with assertions off. Exact archive/backend/tool identities are in `build-inputs.json`. Place the compatible LLVM developer tree at the chosen `<LLVM_ROOT>` and substitute both placeholders in `bootstrap.matched.toml`. Do not substitute an unrelated system LLVM.

Run the canonical build from the fixed source directory with the explicit settings below (the existing HOME is inherited unchanged):

```sh
env -i HOME="$HOME" PATH=/usr/bin:/bin \
  CARGO_HOME=/absolute/owned/build-root/cargo-home \
  TMPDIR=/absolute/owned/build-root/tmp CARGO_BUILD_JOBS=24 \
  CARGO_INCREMENTAL=0 CARGO_TERM_COLOR=never LANG=C.UTF-8 LC_ALL=C.UTF-8 \
  LD_LIBRARY_PATH=/absolute/owned/llvm/lib \
  taskset -c 0-23 ./x build \
  --config /absolute/owned/build-root/bootstrap.matched.toml \
  --build-dir /absolute/owned/build-root/build --stage 1 -j 24 library
```

The config selects opt3, ThinLTO, one codegen unit, debug1, no compiler-build incremental/debug assertions and no PGO. Preserve the complete baseline stage1 runtime and matching std independently before changing source. Optional `lib/rustlib/src` and `lib/rustlib/rustc-src` convenience trees are excluded equally; no compiled runtime/std files are omitted. Preserve the exact shipped SDK12 Wild auxiliary bytes in both runtime copies. Keep the build-only LLVM library path out of workload environments.

After preserving the baseline source/index and runtime, restore the owned source worktree to a clean checkout. For the measured candidate, check out `83dd78b94b407779b669d2f4e6b95f2efea726d3` at the same source path, apply `candidate.patch` to the index and require tree `3e01c5f6eb6c6cc05f43320d015b6297440d2da5`. Import the exact supplied commit object and verify its hash before selecting it:

```sh
git hash-object -w -t commit /absolute/evidence/candidate-commit.raw
# Required output: 0a72d98143f01fc7db249147df140d2797d7cf73
git checkout --detach 0a72d98143f01fc7db249147df140d2797d7cf73
```

This raw object reconstructs the actual measured version string; the public measured-tree commit has different publication metadata. Preserve the old stage1/std trees, rebuild the candidate and fresh matching std with the same recipe, and freeze both 74-file manifests. Do not reuse baseline std or compiled dependency state with the new metadata layout. The actual version difference rejects old metadata before decoding; both cross-arm directions were tested for E0514. Apply the separate test-only strengthening patch after freezing the compiler to run ordinary and explicit `-Zpublic-api-hash` cases with independent incremental directories:

```sh
./x test --config /absolute/owned/build-root/bootstrap.matched.toml \
  --build-dir /absolute/owned/build-root/build --stage 1 -j 24 \
  tests/run-make/c-linker-generic-summary
```

For fresh workload preparation, use unchanged uv `3d82a6ad5a7eab8c68236553b7846517767c4b61`, fixed released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35` and the runtime identities in this packet. Create independent runtime, target, shared-cache and package-home banks for each compiler. Seed only source/locked dependency archives/index data, with no compiled artifacts. Activate banks by renaming them into identical physical paths, preserving runtime inodes. Warm and trace **both** original check and test-build commands in each arm until the excluded freshness proof shows zero compiler launches:

```sh
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  check --locked --offline -j 8
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  test --no-run --locked --offline -j 8
```

Keep fresh private Cargo/shared-cache configuration, explicit compiler path and unchanged workload environment pinned. Record every ancestor Cargo configuration, tool/runtime/source identity and actual child argv/environment. In the excluded dirty test-build trace, select exactly one successful `--test --crate-name uv crates/uv/src/lib.rs` invocation with `--emit=dep-info,link`, excluding `-Zlink-only`. Retain the six-role compiler multiset and each ordered argument/environment vector. Only the dead Cargo jobserver descriptors are omitted in the standalone replay. Cross-arm comparisons substitute only the freshly observed compiler-context child namespace and its OUT_DIR use; actual command bytes remain unchanged.

Before sampling, native and no-counter Valgrind replays must reproduce each arm's complete linked ELF, typed dep-info/link diagnostic paths and test inventory. Establish new exact per-arm output hashes, including mode and size; do not import the historical hashes or mask arbitrary sections. Cross-arm dynamic dependencies/exports, entrypoint and demangled test discovery must be accounted for. This protocol checks dep-info paths but does not freeze dep-info content hashes.

For each prespecified arm, the supplied `dirty_fixture.py` writes the fixed-length BEFORE comment and native Cargo runs under that arm to establish the immediate pre-state and exact selected compiler context. It then writes the paired AFTER comment. The exact captured test-harness rustc invocation runs once under Callgrind 3.22 with `--trace-children=no`, all its worker threads on CPUs 0–7, and the already-validated environment normalizer. The real link output remains enabled; the child linker executes but contributes no counters. The whole original rustc PID's final Ir is the metric. Native preparation, Cargo, other compiler roles and linker counters are outside this metric.

The unchanged adapter source, package hashes and all 640 profiler-file identities are in `collection-tools/`. Its README gives the build command. After the required BEFORE preparation, fresh context capture and AFTER edit, the actual isolated invocation has this runnable form (the JSON is your fresh context, not a historical private environment):

```python
from pathlib import Path
import json
import os
import signal
import subprocess

context = json.loads(Path("fresh-harness-context.json").read_text())
tools = Path("/absolute/owned/profiling-tools")
argv = context["original_argv"]
environment = {}
for entry in context["observed_environment_rows"]:
    key, value = entry.split("=", 1)
    assert key and key not in environment
    environment[key] = value
omitted = {key: environment.pop(key) for key in ("CARGO_MAKEFLAGS", "MAKEFLAGS", "MFLAGS") if key in environment}
assert set(omitted) == {"CARGO_MAKEFLAGS"}
assert omitted == context["omitted_unusable_jobserver_variables"]
assert environment == context["standalone_environment"]
assert "--test" in argv and "-Zlink-only" not in argv
assert argv[argv.index("--crate-name") + 1] == "uv"
assert "crates/uv/src/lib.rs" in argv
assert "LD_PRELOAD" not in environment and "LD_LIBRARY_PATH" not in environment
assert environment["OHM_BENCH_NORMALIZE_VALGRIND_ENV"] == "1"
assert environment["VALGRIND_LIB"] == str(tools / "valgrind/usr/libexec/valgrind")
profile = Path("/absolute/owned/fresh-sample.callgrind")
assert not profile.exists()
command = ["/usr/bin/taskset", "-c", "0-7",
    str(tools / "valgrind/usr/bin/valgrind.bin"), "--tool=callgrind",
    "--trace-children=no", "--demangle=no",
    "--extra-debuginfo-path=" + str(tools / "valgrind/usr/lib/debug"),
    "--callgrind-out-file=" + str(profile)] + argv
with Path("fresh-sample.stdout").open("xb") as out, Path("fresh-sample.stderr").open("xb") as err:
    process = subprocess.Popen(command, cwd="/absolute/owned/uv",
        env=dict(environment, LD_PRELOAD=str(tools / "ohm-valgrind-env.so")),
        stdout=out, stderr=err, start_new_session=True)
    try:
        returncode = process.wait(timeout=150)
    except BaseException:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        raise
    assert returncode == 0
```

This single invocation is not a replacement for the state, host, output and restoration guards surrounding every sample. Preserve source restoration in an outer cleanup block even if the command or a validator fails.

The controller is pinned to CPU8, settles once for 30 seconds at stage entry, and uses the inherited strict host checks at entry and before/after every measured call. No heavy builder/profile competes on the measurement host. Original source bytes/mode are restored after every sample; there is **no original-command convergence between arms**. At block end, both original check and test-build commands converge and are traced fresh for each arm, all banks are parked, and source/runtime/Cargo/context/output/diagnostic guards authenticate before effects. The final post-cleanup host observation is retained but observational.

Run the two fixed schedules and seeds in `protocol.json`, collecting each complete restored block before calculating effects. Compare final totals against summed self costs and stderr Collected counters; preserve header offsets. Technical failure retains an opaque attempt; valid failure stops the endpoint without retries, extensions or deleted pairs. Fresh machine/input/output admission and independently warmed state are necessary to rerun this study; the portable numerical replay alone does not establish those conditions.

## Reproducing the independent cross-host replication

The published second block is a fresh-host inverse replication, not completion of the interrupted fixed-host endpoint. Reuse the exact compiler/std archives, Cargo/Wild and profiling tools on a second quiet Linux x86_64 machine. Use the same absolute fixture/runtime/tool paths, but create and warm all uv target, shared-build and Cargo-home state independently from source and package archives. Do not transfer compiled dependency banks.

Before a fresh cohort, recapture all six native compiler roles and the selected library-test argv/environment. Compare each ordered argv/environment as a multiset preserving call multiplicity, allowing only the explicitly observed compiler-context directory substitution and omission of dead CARGO_MAKEFLAGS. Establish new single-hash per-arm ELF and typed-diagnostic guards through native and no-counter replay; do not inherit output hashes or machine identities. Record the complete input manifest before observing counts.

The original first block uses seed 2026102441. The independent inverse uses the original inverse order and seed 2026102442, eight pairs, 200,000 bootstrap resamples and the same all-eight-wins / 99% lower reduction bound above 0.1% gate. Analyze each block separately. There is no pooled gate, valid-failure retry or sample extension. Preserve and exclude invalid cohorts without inspecting instruction totals. The historical five-plus-two incomplete observations are absent from this package.

A passive completion notification can wrap the measured driver without changing it. On the replication host, the launch was held until a known credential-refresh service finished and the unchanged quiet admission passed, with room before the next known heavy timer. Services and guard thresholds were unchanged.

For the maintenance-aware inverse, freeze the host's three known heavy recurring service/timer bindings before sampling. After the original once-per-stage settle and before the unchanged entry check, require successful inactive services and at least 120 seconds until each reported next activation. Otherwise wait outside measurement and observe again every five seconds. Repeat this admission only after both arms of a pair have finished and restored the source, before selecting the next pair's first arm. Do not insert a wait or convergence command between the two arms, and do not disable services or exempt maintenance from any existing occupancy check. Preserve every timer observation. Read typed uint64 next-deadline properties for both realtime and monotonic clocks. Values 0 and UINT64_MAX indicate an unavailable clock; if a timer has no concrete deadline, wait outside pairs and retain that observation. Never admit a missing/ambiguous deadline. Recompute remaining time against the matching clock and read service states after deadlines, so a service beginning between those reads is refused. Wrong/duplicate unit identities, malformed typed properties, failed services or any within-pair guard failure still invalidate the whole cohort. The structured helper was rehearsed through a complete real maintenance transition without a workload before the final cohort. The window does not guarantee safety against unscheduled work. Host-specific timer names and raw process records are private admission inputs, so a fresh host must establish its own bindings before a fresh cohort.
