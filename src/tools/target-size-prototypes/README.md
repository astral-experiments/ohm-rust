# Target-size compiler prototypes

This directory retains the measurement harness for the zero-fingerprint and
allocation-payload experiments. Each experiment compares an independent compiler
change against `d37229159e545bbdec917dd7efdf5d26da40d2bc` (Ohm 1.98.0-9).
The result files identify the actual compiler source revision, executable hash,
patch hash, captured command/environment, and fixture revision. Evidence-only
commits after the compiler build do not change the measured Rust implementation.

## Inputs and build

Use a dedicated Linux x86-64 host. The scripts use
`/tmp/ohm-target-size-prototypes` for source, inputs, and receipts, and
`/tmp/codex-cargo-targets/ohm-rust/isolated/target-size-prototypes` for build outputs.
Copy these scripts and manifests into the first directory.

`inputs.json` records checksums for the measured archives and permanent download
locations for the toolchain and LLVM inputs. The source archives are Git archives
of the recorded Rust revision and its `library/backtrace` submodule, without a
directory prefix. Source tarball compression may differ across archive tools;
when regenerating archives, verify the Git revisions and record the new archive
checksums before running `setup.py`.

`rustc-perf-fixtures.json` pins `rust-lang/rustc-perf` at
`c0301bc44d175b9b2c5442b25049475c39d7700c`. Its archive contains only
`collector/compile-benchmarks/syn-2.0.101` and
`collector/compile-benchmarks/regex-automata-0.4.8`, with each fixture directory at
the archive root. These are existing parser and regex workloads, including their
checked-in body-edit patches. Run `python3 prepare_fixtures.py` to download the
pinned source archive, verify its checksum and canonical fixture-content digest,
and generate the selected fixture archive with its recorded local checksum.

Place archives under `inputs/`, run `python3 setup.py`, and place each experiment's
diff against the base in `inputs/<variant>.patch`. Run `python3 build.py baseline
d37229159e545bbdec917dd7efdf5d26da40d2bc`, then `python3 build.py <variant>
<implementation-commit>`. Builds use the same explicit optimized bootstrap
configuration and produce a compiler with its matching standard library.
The build receipts retain the configuration and patch checksum.

Bootstrap sets `__CARGO_OHM_DEFAULTS=no`: the released compiler otherwise failed
to bootstrap the unchanged baseline when an exported macro referenced a private
module. This opt-out applies to building the measurement compilers only. The
measurement driver clears it before preparing or replaying the workloads.
Both bootstrap and direct compiler invocations set `LD_LIBRARY_PATH` to the
pinned custom LLVM library directory; captured environments retain this setting.

## Validation and measurements

Wait until all compiler builds have finished before measuring. Run preparation
and measurements serially on the same host. Commands below select the checking
profile; use `--profile dev` to include code generation. Preparation asserts that
the captured compiler command actually uses the requested emit mode.

```sh
python3 validate_rust.py prepare --variant baseline --fixture syn-2.0.101
python3 validate_rust.py prepare --variant dep-fingerprints --fixture syn-2.0.101
python3 validate_rust.py regression --variant dep-fingerprints
python3 validate_rust.py verify --variant dep-fingerprints --fixture syn-2.0.101
python3 validate_rust.py cycles --variant dep-fingerprints --fixture syn-2.0.101
python3 validate_rust.py cachegrind --variant dep-fingerprints --fixture syn-2.0.101
python3 summarize_rust.py dep-fingerprints --fixture syn-2.0.101 --output results.json
```

Use `allocation-aliases` for the allocation experiment. The regression command
expects its source checkout to be present: it runs both the new allocation
fixture and the existing query-forwarding fixture through seven compilations
and executable runs with four frontend threads and incremental hash verification.
The allocation compiler also reports retained payload-alias counts with
`-Zincremental-info`.

The focused unit tests use each variant source checkout and its existing bootstrap configuration:

```sh
CARGO_HOME=/tmp/codex-cargo-home __CARGO_OHM_DEFAULTS=no LD_LIBRARY_PATH=/tmp/ohm-target-size-prototypes/llvm/lib python3 x.py test --stage 1 -j 8 compiler/rustc_middle --test-args compact_headers_preserve_indices_fingerprints_and_edge_layout
```

Use `aliases_preserve_allocation_identity` for the allocation variant. Stage 1 compiler unit tests build the test harness with the configured stage 0 compiler, reusing the compiler build dependencies.

The real-workload cycle has seven states: original, unchanged, body edit,
unchanged, revert, unchanged, body edit. Each cycle starts with an empty cache;
the sequence includes reuse and the three-generation compaction boundary.
The separate `verify` action adds incremental hash verification to all seven real-workload revisions. Native timings and Cachegrind omit this option. Dependency builds and source/cache setup are outside every measured call.
Native timing uses one unmeasured cycle and counterbalanced ABBA/BAAB cycles.
Cachegrind recreates prior state outside measurement and counts instructions for
revisions 2, 3, and 4 in AB/BA order, with cache and branch simulation disabled.

Storage snapshots sum logical file lengths and count allocated filesystem blocks once per inode within each private target. Physical accounting includes within-target hardlink sharing; it does not claim to measure shared reflink extents. Dependency-graph footers also retain exact node, edge, and omitted-fingerprint counts. Result files preserve
individual counter rows, sample timings, seven-step byte counts, and provenance.
Raw logs remain in the task directory. A smaller file alone does not establish
performance neutrality; each experiment's result must state its measured verdict
and the workloads that were excluded.
