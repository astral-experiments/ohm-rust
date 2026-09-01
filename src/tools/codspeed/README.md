# Compiler benchmarks

The `Compiler performance` workflow measures an optimized stage-1 compiler from each
PR and each push to `ohm-1.98.0`. It runs CPU simulation on an x86_64 Depot runner
and Walltime measurements on a dedicated ARM64 CodSpeed Macro runner. Each job
builds the compiler and its matching standard library for that runner.
The workflow verifies the compiler's source revision before benchmarking it.

We reuse `bitmaps-3.2.1` and `syn-2.0.101` from the pinned `src/tools/rustc-perf`
submodule. These provide trait-heavy code and a real parser with a small dependency
graph. For each crate, we measure checking in four scenarios:

| Scenario | State before the measured compiler invocation |
| --- | --- |
| `Full` | Incremental compilation disabled |
| `IncrFull` | Empty incremental cache |
| `IncrUnchanged` | Cache populated by compiling the same sources |
| `IncrPatched` | Populated cache, then the fixture's `0-println.patch` body edit |

Preparation uses bootstrap's pinned Cargo and the candidate compiler to build
dependencies and capture the leaf compiler command and required environment.
Measurements replay that command directly, so Cargo freshness cannot skip a case.
Dependencies are outside the measured work. Source and cache preparation also run
outside measurement, before **every** invocation, including CodSpeed's warmup.

## CPU simulation

CodSpeed 5.2.1's subprocess tracking includes the spawned compiler. The Python
benchmark body launches the compiler, then waits for it in unmeasured teardown.
The child inherits independent instrumentation state, so its instructions continue
to be counted after the parent's measurement ends. Teardown checks the compiler's
exit status before the next case can restore sources or incremental state.
We preserve CodSpeed's live hooks when restoring Cargo's environment and discard
stale jobserver handles.
The compiler runs with one thread and full incremental hash verification, following
rustc-perf's policy for repeatable incremental measurements. The preparation
manifest records the compiler, Cargo, and benchmark-suite revisions.

After measurement, CI audits the raw profiles for all eight cases. Each benchmark
must name a spawned process running the selected compiler, with positive instruction
counts and compiler symbols in its profile. The job summary reports those counts;
the `codspeed-compiler-evidence` artifact preserves the audit and preparation manifest.
The audit also separates parent and compiler syscall counts and observed durations.
Raw profiles are retained for three days in `codspeed-raw-profiles`, including for
successful jobs, so syscall warnings can be investigated.
This checks collection independently of CodSpeed's asynchronous report processing.

Waiting for a simulated child inside the parent's measurement records the child's
slow instrumented execution as time in the parent's wait syscall. Moving the wait
to teardown avoids that misleading contribution. The compiler still performs real
I/O and thread waits, which can produce CodSpeed syscall warnings: those durations
are excluded from CPU simulation, while the compiler's userspace instructions are
counted. This launch/teardown boundary is specific to simulation; a walltime
benchmark must include the wait.

These are simulated CPU costs, including modeled cache behavior, rather than raw
hardware instruction counts or elapsed build times. `./x perf` remains available
for upstream-style hardware-counter measurements and Cachegrind profiling.

## Walltime

The Walltime job uses `test_compiler_walltime.py` to time complete compiler
invocations, including their I/O and system calls. It runs one unmeasured warmup
and ten measured rounds per case, with one compiler invocation per round. The
compiler must exit successfully before the timed call returns. Cases run serially,
and each warmup and measured round gets freshly restored source and incremental
state through the same setup hook as CPU simulation.

The pinned pytest integration also makes one final unmeasured call to return the
result. That call restores state too. Setup and cache seeding are excluded from
the individual timing statistics. The exported `total_time` includes setup and
teardown, so it is not used as the compiler timing metric.

CI checks that all eight Walltime cases have ten measured rounds and valid timing
statistics. The job summary reports their mean, median, standard deviation, minimum,
and maximum. `codspeed-walltime-evidence` retains the audit and compiler manifest;
`codspeed-walltime-statistics` retains the original pytest result documents. Both
artifacts are kept for fourteen days.

Walltime cases have distinct benchmark IDs and start a fresh ARM64 measurement
history. Historical CPU simulation results are not their baseline. The initial
Walltime run records timings; comparisons need a Walltime run for the base revision,
including a mainline run once this workflow is merged.

Both jobs use the fixed compiler configuration in `bootstrap.toml`, without release
PGO or BOLT, and retain the same single-threaded leaf-check workloads. Ohm's existing
development-session replays cover full parallel builds and Cargo cache reuse.

## Run locally

From the repository root, build the candidate compiler and matching sysroot:

```sh
git submodule update --init --depth 1 library/backtrace src/tools/rustc-perf
python3 x.py --ci false --config src/tools/codspeed/bootstrap.toml build --stage 1 library
```

Use uv 0.12.7 and the pinned Python and package versions, then prepare a fresh
`build/codspeed` directory and run the native smoke tests:

```sh
uv venv --python 3.13.15 src/tools/codspeed/.venv
uv pip sync --python src/tools/codspeed/.venv/bin/python --require-hashes \
  src/tools/codspeed/requirements.txt
src/tools/codspeed/.venv/bin/python src/tools/codspeed/prepare.py \
  --rustc build/host/stage1/bin/rustc --cargo build/host/stage0/bin/cargo
src/tools/codspeed/.venv/bin/python -m pytest \
  src/tools/codspeed/test_harness.py src/tools/codspeed/test_profiles.py \
  src/tools/codspeed/test_walltime_results.py
src/tools/codspeed/.venv/bin/python -m pytest src/tools/codspeed/test_compiler.py
src/tools/codspeed/.venv/bin/python -m pytest src/tools/codspeed/test_compiler_walltime.py
```

Without `--codspeed`, each case executes once to check the harness and compiler;
this does not produce performance measurements. Both CI jobs require Linux and run
their respective entry points with `--codspeed`. CPU simulation also enables
subprocess tracking.
The profile coverage check verifies child-process accounting. The pinned pytest
integration can print `0 benchmarked` for the pedantic API even when it emits
measurements; the audit checks the actual per-process instruction counts.

Preparation rejects an existing output directory to prevent mixing candidates.
Use `--output` to select a fresh directory and set `RUSTC_CODSPEED_MANIFEST` to its
`manifest.json` when testing. `--suite` accepts an existing clean rustc-perf suite;
`CARGO_TARGET_DIR` can select the parent directory for the fixtures' Cargo targets.

To update the hash-pinned Python dependencies after changing `pyproject.toml`, run
from this directory:

```sh
uv pip compile --universal --generate-hashes --no-header --no-annotate \
  pyproject.toml --output-file requirements.txt
```
