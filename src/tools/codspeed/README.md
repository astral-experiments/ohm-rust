# Compiler benchmarks

The `Compiler performance` workflow measures an optimized stage-1 compiler from each
PR and each push to `ohm-1.98.0` using Walltime on a dedicated ARM64 CodSpeed Macro
runner. It builds the compiler and matching standard library once, then shares
that artifact with six compiler benchmark jobs and two development-workflow jobs.
Each job verifies the compiler's source revision before benchmarking one crate or
workspace.

## Compiler cases

We reuse `bitmaps-3.2.1`, `syn-2.0.101`, `serde-1.0.219`, and
`regex-automata-0.4.8` from the pinned `src/tools/rustc-perf` submodule. These cover
trait-heavy code, parsing, serialization, and regular expressions. Each crate has
four checking scenarios and a nonincremental debug build that includes code
generation. We also check Ruff's `ty_python_semantic` and `uv-resolver` with `clap`
enabled, using pinned workspace revisions and nonincremental compilation.

The checking scenarios for the four rustc-perf crates are:

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

The benchmark times each complete compiler invocation, including I/O and system
calls, and checks that it exits successfully before the timed call returns. Cases
run serially with one unmeasured warmup and ten measured rounds, each containing one
compiler invocation. The compiler uses one frontend thread and full incremental hash
verification, following rustc-perf's incremental measurement policy.

The pinned pytest integration also makes one final unmeasured call to return the
result. That call restores state too. Setup and cache seeding are excluded from
the individual timing statistics. The exported `total_time` includes setup and
teardown, so it is not used as the compiler timing metric.

CI checks each job's assigned cases for the expected sampling counts and positive,
finite minimum and mean timings, in nanoseconds. The six compiler jobs cover all 22 cases,
and CodSpeed combines their measurements into one report. Each job retains its
result documents and preparation manifest for fourteen days in a
`codspeed-walltime-evidence-<shard>` artifact. The manifest identifies the compiler,
Cargo, benchmark revisions, and Cargo profiles. Existing cases retain their
benchmark names.
Comparisons require a Walltime run for the base revision, including a mainline run
once this workflow is merged.

The fixed configuration in `bootstrap.toml` measures leaf compilation with one
frontend thread, without release PGO or BOLT. Dependencies are prepared separately
for each crate and profile. Ohm's development-session replays cover full parallel
builds and Cargo cache reuse.

## Development workflows

Eight additional cases run complete Cargo commands on the same pinned Ruff and uv
workspaces. They measure `ty_python_semantic` and `uv-resolver` with the workspace's
normal development profile, incremental compilation, eight Cargo jobs, and Ohm's
default compiler flags, including eight frontend threads.

| Scenario | Prepared state | Measured commands after the edit |
| --- | --- | --- |
| `DependencyEditCheck` | Warm check | Check the target after an implementation-only dependency edit |
| `DirectEditCheck` | Warm check | Check the directly edited target |
| `DirectEditTest` | Warm test build and check | Rebuild the edited target's library test executable |
| `CheckTestCheck` | Warm test build and check | Check, rebuild the library test executable, then check again |

The uv dependency edit changes `uv-normalize`; the Ruff dependency edit changes
`ruff_python_trivia`. Direct edits change private implementations in `uv-resolver`
and `ty_python_semantic`. Each edit preserves behavior and public signatures.
Test builds use `cargo test --lib --no-run`: compilation and linking are measured,
but test execution is excluded.

These cases use the candidate rustc with a fixed Ohm Cargo, built from the
`CARGO_REVISION` pin in `workflows.py` with the `ohm-defaults` feature. CI builds
that Cargo with bootstrap's compiler and packages it alongside the candidate.
This holds Cargo constant across compiler comparisons while allowing its rebuild
decisions to affect the result. It does not benchmark changes to Cargo itself.

The workflow requires `OHM_CARGO_READ_TOKEN` with read-only Contents access to the
private `astral-sh/ohm-cargo` repository. The default workflow token only grants
access to this repository. Checkout does not persist the credential.

Preparation fetches dependencies, warms each command sequence, and snapshots its
target directory. Before every sample, including the unmeasured warmup and final
call, the harness restores the original source and the matching build snapshot,
then applies the edit. Mutable outputs are never hard-linked to the snapshot.
Copies use filesystem cloning where available. Restoration and verification are
outside measurement; Cargo execution, compiler subprocesses, and linking are
inside. Each case uses ten measured rounds with one invocation of the sequence
per round. Warmups and measurements use `--locked --offline`.

Cargo runs with verbose output retained in a separate log for every sample. The
harness rejects samples without a compiler command for the edited crate and
rejects failed commands. The manifest records the exact compiler and Cargo
versions, source pins, edits, commands, and environment. Credentials and proxy
settings are not included. Workflow result verification requires all four cases
for the assigned workspace; these cases have separate names from compiler cases.

The workflow lane covers warm local edit/build loops. It does not measure release
PGO/BOLT, Clippy, formatting, concurrent worktrees, shared-build caches, disk usage,
or complete recorded sessions. Use Ohm's session and disk studies for those claims.

## Run locally

From the repository root, build the candidate compiler and matching sysroot:

```sh
git submodule update --init --depth 1 library/backtrace src/tools/rustc-perf
python3 x.py --ci false --config src/tools/codspeed/bootstrap.toml build --stage 1 library
```

Use uv 0.12.7 and the pinned Python and package versions, then prepare a fresh
`build/codspeed` directory and run the native smoke tests. First place clean Ruff
and uv checkouts at `build/codspeed-sources/ruff` and `build/codspeed-sources/uv`,
at the revisions in `benchmarks.py`; preparation verifies both pins. The CI
workflow checks out these sources automatically.

```sh
uv venv --python 3.13.15 src/tools/codspeed/.venv
uv pip sync --python src/tools/codspeed/.venv/bin/python --require-hashes \
  src/tools/codspeed/requirements.txt
src/tools/codspeed/.venv/bin/python src/tools/codspeed/prepare.py \
  --rustc build/host/stage1/bin/rustc --cargo build/host/stage0/bin/cargo
src/tools/codspeed/.venv/bin/python -m pytest \
  src/tools/codspeed/test_harness.py \
  src/tools/codspeed/test_walltime_results.py \
  src/tools/codspeed/test_workflows.py
src/tools/codspeed/.venv/bin/python -m pytest src/tools/codspeed/test_compiler_walltime.py
```

Without `--codspeed`, each case executes once to check the harness and compiler;
this does not produce performance measurements. CI runs with `--codspeed` under the
CodSpeed Walltime runner on Linux.

Preparation rejects an existing output directory to prevent mixing candidates.
Use `--output` to select a fresh directory and set `RUSTC_CODSPEED_MANIFEST` to its
`manifest.json` when testing. `--suite` accepts an existing clean rustc-perf suite,
and `--workspaces` selects the directory containing the pinned Ruff and uv checkouts;
`CARGO_TARGET_DIR` can select the parent directory for the fixtures' Cargo targets.

To run one CI shard locally, pass the same `--shard` value to `prepare.py` and
`verify_walltime.py`, and select it with pytest's `-k` option. For example,
`--shard syn-2.0.101` and `-k syn-2.0.101` cover syn's four checking scenarios and
its debug build. Omitting the shard runs the complete suite.

For development workflows, supply Cargo built from the pinned revision with
`ohm-defaults`, or the Cargo executable from Ohm 1.98.0-9. Preparation rejects a
different Cargo revision. Keep `CARGO_TARGET_DIR` stable and private to this
benchmark checkout; the runner creates workflow targets and snapshots beneath it.

```sh
src/tools/codspeed/.venv/bin/python src/tools/codspeed/workflows.py \
  --rustc build/host/stage1/bin/rustc --cargo /path/to/pinned-ohm-cargo \
  --shard uv-resolver
src/tools/codspeed/.venv/bin/python -m pytest \
  src/tools/codspeed/test_workflow_walltime.py -k uv-resolver
```

Omit `--shard` and `-k` to prepare and smoke-test both workspaces. Custom output
directories require `RUSTC_CODSPEED_WORKFLOWS=/path/to/manifest.json`. As with the
compiler cases, native pytest runs validate execution without producing CodSpeed
measurements. CI verifies exported results with
`verify_walltime.py build/codspeed --workflows --shard uv-resolver`.

To update the hash-pinned Python dependencies after changing `pyproject.toml`, run
from this directory:

```sh
uv pip compile --universal --generate-hashes --no-header --no-annotate \
  pyproject.toml --output-file requirements.txt
```
