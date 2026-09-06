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

Wait until all compiler builds have finished before measuring. Run native timings serially on the same host, with other compiler work idle. Cachegrind counts may overlap independent fixture work after native timings finish; treat its elapsed times as diagnostic only. The shared driver serializes actions that restore the same source fixtures. Commands below select the checking
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

Use `allocation-aliases` for the allocation experiment. Add `--revisions 2` to restrict Cachegrind to the unchanged revision; the default remains revisions 2, 3, and 4. The optional selector was added after the dependency-fingerprint screen: its exact measured driver remains available at commit `4dc2eb14298f959dbde147d84a1ad9b71de4ec39` (SHA-256 `524b9fa9760da4d9e6030db2c29498c04ffb6de2fb88f2a0e9dae1edcff0acfc`). Each counter row identifies its actual driver hash. The regression command
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


## Zero-fingerprint result

**Verdict: do not pursue this encoding as-is for a performance-neutral target-size reduction.** The small storage saving comes with more compiler instructions in all three measured syn check states.

The compiler change was built at `3c0bc75c33c3ba4d3ea63fe431cd500cde48da4a`; the baseline was built at `d37229159e545bbdec917dd7efdf5d26da40d2bc`. Later commits only retain evidence and reproduction scripts.

The measured private targets include dependencies and the leaf crate's artifacts. Allocated target savings across all seven revisions were 0.50–0.59% for syn check, 0.19–0.24% for syn dev, 0.50–0.60% for regex check, and 0.15–0.20% for regex dev. These are whole-target percentages; dependency-graph files themselves shrank by about 1%.

| Workload | Allocated target at first edit, baseline → candidate | Seven-revision compiler time, median baseline → candidate |
| --- | ---: | ---: |
| syn check | 28,688,384 → 28,540,928 bytes | 4.790 → 4.996 s |
| syn dev | 100,036,608 → 99,819,520 bytes | 8.765 → 8.642 s |
| regex check | 38,424,576 → 38,227,968 bytes | 7.498 → 7.414 s |
| regex dev | 174,538,752 → 174,219,264 bytes | 13.793 → 13.642 s |

Native timings have overlapping sample ranges and some bimodal short calls. The result files retain all samples and complete seven-revision totals, rather than treating favorable walltime changes as established speedups.

The instruction-count screen is limited to syn check at the unchanged, first-edit, and post-edit revisions (2, 3, and 4), with two samples per compiler in AB/BA order. The other fixture/profile combinations have native timing and storage data but no instruction-count measurement.

| syn check revision | Baseline median instructions | Candidate median instructions | Change |
| --- | ---: | ---: | ---: |
| 2: unchanged | 1,323,711,625.5 | 1,330,258,748.5 | +0.495% |
| 3: first edit | 1,629,672,451.5 | 1,637,175,875 | +0.460% |
| 4: post-edit | 1,442,205,708.5 | 1,449,529,522.5 | +0.508% |

All twelve counter samples passed. The repeated counts are much tighter than these increases; the prototype did not pass the selected instruction-count screen for a performance-neutral change.

The candidate compiler and its matching standard library built successfully. One compact-header boundary unit test passed, as did 28 real-workload compilations with incremental hash verification and two seven-revision executable regressions (14 further compilations and runs). The baseline passed the same real-workload and executable checks. See [correctness receipts](dep-fingerprints-correctness.json) and the four files under [results](results). These checks do not substitute for the repository's full compiler test suite or CI.

CI tidy requires unit tests in separate test files. The compact-header test was moved unchanged to `compiler/rustc_middle/src/dep_graph/serialized/tests.rs`; the focused unit rerun passed (one test). [The follow-up receipt](dep-fingerprints-relocated-unit.json) records source hashes, identical test-body/production-source checks, and the test result. The measured compiler logic is unchanged.
