# Compiler benchmarks

The `Compiler performance` workflow measures an optimized stage-1 compiler from each
PR and each push to `ohm-1.98.0` using Walltime on a dedicated ARM64 CodSpeed Macro
runner. It builds the compiler and matching standard library, then verifies the
compiler's source revision before benchmarking it.

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

CI checks that all 22 cases have the expected sampling counts and positive,
finite minimum and mean timings, in nanoseconds. The `codspeed-walltime-evidence`
artifact retains the original result documents and the preparation manifest for
fourteen days. The manifest identifies the compiler, Cargo, benchmark revisions,
and Cargo profiles. Existing checking cases retain their benchmark names.
Comparisons require a Walltime run for the base revision, including a mainline run
once this workflow is merged.

The fixed configuration in `bootstrap.toml` measures leaf compilation with one
frontend thread, without release PGO or BOLT. Dependencies are prepared separately
for each crate and profile. Ohm's development-session replays cover full parallel
builds and Cargo cache reuse.

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
  src/tools/codspeed/test_walltime_results.py
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

To update the hash-pinned Python dependencies after changing `pyproject.toml`, run
from this directory:

```sh
uv pip compile --universal --generate-hashes --no-header --no-annotate \
  pyproject.toml --output-file requirements.txt
```
