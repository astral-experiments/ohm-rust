# Sparse metadata prototype measurements

See the [recorded results](results/sparse-metadata/README.md) for measurements,
their scope, raw evidence, and the prototype's current verdict.

[`sparse_metadata.py`](sparse_metadata.py) measures complete compiler invocations for
syn, Ruff's AST, Ruff's parser, and ty's semantic library. The parser and ty read
dependency metadata as well as producing metadata. Results do not isolate codec
instructions from parsing, type checking, or other compiler work.

## Inputs

- rustc-perf: `c0301bc44d175b9b2c5442b25049475c39d7700c`, including the real
  `collector/compile-benchmarks/syn-2.0.101` source and its lockfile.
- Ruff: `efcffd2178ce62e9951a53d4c50cadc225a0cfec`.
- Cargo: the packaged Ohm 1.98.0-9 Cargo binary, identical for both arms.
- Separately built, matching stage-1 compiler/sysroots for the base and candidate.

The script checks the source archive SHA-256 hashes and hashes the extracted source
trees. It records compiler and driver hashes, compiler build receipts when present,
Cargo version/hash, captured arguments/environment, and dependency metadata sizes.

Place the GitHub source archives for the pinned revisions in
`inputs/rustc-perf-source.tar.gz` and `inputs/ruff-codspeed-codeload.tar.gz` under
the chosen root. Both are available through `https://codeload.github.com/<owner>/<repo>/tar.gz/<revision>`.
The script contains the exact expected archive hashes. Its default paths match the
prototype build harness; `--root`, `--target-root`, and `--cargo` allow other paths.

## Procedure

Run the script's phases in this order: `unpack`, optional `fetch`, `prepare`,
`native`, `instructions`, `report`. `fetch` is a separate source-download step;
`prepare` resolves and builds with `--locked --offline`. Each arm gets separate
dependency artifacts. Preparation and source downloads are excluded from results.

Run measurement phases alone on an otherwise idle Linux host with Valgrind
installed. Each checks for unrelated compiler/build processes and samples CPU idle,
I/O wait, and steal time before every invocation. The host still requires exclusive
scheduling for the duration of a measurement phase.

Each native cycle uses ABBA then BAAB, giving four samples per compiler, after one
excluded warmup per compiler. Instruction cycles default to ABBA (two samples per
compiler); `--instruction-samples 4` selects ABBA then BAAB. `--cycles 2` doubles
these counts, and `--cases` selects the workloads for that phase. Every sample invokes rustc directly
with one frontend thread and `--emit=metadata`; incremental compilation is disabled.
Fresh output files are prepared outside each measurement. Cargo cannot skip a sample.

Results remain under `sparse-metadata-bench/`: exact commands, preparation logs,
ordered samples, raw Cachegrind files, and `summary.json`. Native elapsed time and
Cachegrind `Ir` instructions are reported separately. The size metric is logical
`.rmeta` bytes; it is not whole-target physical allocation or APFS clone accounting.

Preparation can also run in separate calls with `prepare --arms baseline` and
`prepare --arms sparse-metadata`. The manifest must match the same Cargo binary and
source provenance; completed arms and partial arm directories cannot be overwritten.

`prepare` finishes by validating source and command identities. The `finalize` phase
can repeat those checks without rebuilding either captured dependency graph.

After all native measurements finish, `instructions --allow-concurrent-instructions`
permits independent instruction-count workloads to share the host. It records
observed co-runners and CPU counters without enforcing the native quiet guard.
Cachegrind elapsed time is diagnostic only and is never compared as native latency.
