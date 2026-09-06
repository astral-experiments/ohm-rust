# Sparse metadata prototype measurements

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

Each cycle uses ABBA then BAAB, giving four samples per compiler, after one excluded
warmup per compiler. `--cycles 2` doubles this; `--cases` selects a subset when a
large Cachegrind run needs a separate window. Every sample invokes rustc directly
with one frontend thread and `--emit=metadata`; incremental compilation is disabled.
Fresh output files are prepared outside each measurement. Cargo cannot skip a sample.

Results remain under `sparse-metadata-bench/`: exact commands, preparation logs,
ordered samples, raw Cachegrind files, and `summary.json`. Native elapsed time and
Cachegrind `Ir` instructions are reported separately. The size metric is logical
`.rmeta` bytes; it is not whole-target physical allocation or APFS clone accounting.
