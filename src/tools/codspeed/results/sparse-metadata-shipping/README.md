# Sparse metadata: shipping validation

## Decision

Proceed with Ohm integration, with the measured tradeoff stated explicitly.
Complete incremental dev targets use 1.84–2.64% less allocated space, while
individual metadata files shrink 21.56–38.32%. The four real-crate compiler
invocations execute 0.024–0.156% more instructions. This is a small reproducible
cost, not a zero-overhead result; the larger ty workload is now covered by the
same matched instruction-count comparison.

The production change keeps dense tables when a bitmap would not save bytes,
checks table dimensions and arithmetic at construction, validates sparse ranks
and tail bits before prefix relocation, and preserves constant-time lookups.
This changes the metadata wire format and relies on the normal compiler-version
compatibility check; release integration must give the new compiler a distinct
version identity.

## Compiler measurements

| Workload | `.rmeta` bytes | Compiler `Ir` | Native median |
| --- | ---: | ---: | ---: |
| syn 2.0.101 | -28.23% | +0.105% | +3.68% |
| Ruff AST | -23.11% | +0.068% | -0.65% |
| Ruff parser | -21.56% | +0.156% | +3.18% |
| ty semantic | -38.32% | +0.024% | -0.63% |

Each row is a complete metadata-producing compiler invocation, including parsing,
type checking, dependency metadata reads, and metadata encoding. The parser and ty
read candidate-encoded dependencies in the candidate arm; each arm also uses its
own matching standard library. These x86-64 Linux results are not isolated codec measurements. Cachegrind
counts the rustc process and its threads; child-process instructions are excluded.

Native results use four measured samples per arm after one warmup, in ABBA/BAAB
order, on an otherwise idle host. Syn and parser have broad, bimodal timing
spreads; their native medians alone do not establish a slowdown. Instruction
results use two samples per arm. Each baseline/candidate pair runs concurrently, the second pair
reverses launch order, and no output or dependency directory is shared between
arms. Cachegrind elapsed times are diagnostic only. Warmups use the native
compiler; every measured instruction sample starts a fresh Cachegrind process.

The four `.rmeta` reductions exactly reproduce the original prototype's file
sizes. The hardened implementation and all performance measurements here are a
new, matched baseline/candidate run. Original results remain separately available
in [the historical report](../sparse-metadata/README.md).

A diagnostic comparison of the first parser pair attributes 2.07 million of
the 4.71 million additional instructions to `table.rs` source lines. Inlining
changes move work between function names, so those figures are not isolated
codec costs. The exact profile hashes, `cg_diff`/`cg_annotate` commands, and
aggregates are in [profile-diagnostic.json](profile-diagnostic.json); the full
annotation is retained in the raw bundle. This diagnostic did not trigger any
further compiler change.

## Complete incremental dev targets

| Target | Baseline allocated | Candidate allocated | Change |
| --- | ---: | ---: | ---: |
| syn | 72.97 MiB | 71.63 MiB | -1.84% |
| Ruff/ty graph | 1825.13 MiB | 1776.94 MiB | -2.64% |

These are fresh `cargo build --locked --offline --lib` targets for syn and
`ty_python_semantic`, with the repositories' dev profiles and
`CARGO_INCREMENTAL=1`. They include dependency artifacts, standalone metadata,
`.rlib` archives, build-script outputs, and incremental caches. They exclude the
compiler/sysroot and Cargo source cache. They measure retained post-build target
allocation, not incremental-build latency or growth over a sequence of edits.

| Target | Unique `.rmeta` bytes | Unique `.rlib` bytes | Unique incremental-cache bytes |
| --- | ---: | ---: | ---: |
| syn | -23.33% | -2.87% | -1.36% |
| Ruff/ty graph | -18.57% | -3.22% | -1.17% |

Apparent bytes count every pathname. Unique file bytes and whole-target allocated
bytes deduplicate hard-linked inodes. Allocation uses `st_blocks * 512` and includes
directories. The incremental-cache subtotal deduplicates that subtree independently;
a file linked both inside and outside the cache contributes to the cache subtotal
and once to the whole-target total. These ext4 measurements do not measure APFS
clone sharing. `findmnt` confirms ext4 for all four targets in
[filesystem.json](filesystem.json); the original `stat -f` label `ext2/ext3`
is retained because those filesystems share a magic number. Per-extension
allocation assigns an inode to its first visited
pathname if links cross extension groups.

All four targets were flushed with `sync`, then inventoried twice to verify stable
allocation. Every file's content hash was unchanged across the flush. The final ty
archive acquired one additional 4 KiB block after the initial build inventory;
the table uses the settled value. Original, pre-sync, and settled receipts, the
per-file difference, and all content-hash manifests are retained in the raw bundle.

## Correctness and broader coverage

- Matched stage-1 compiler and standard-library builds passed.
- All five metadata-table unit tests passed, covering dense/sparse selection,
  strict size ties, widths and overflow, malformed ranks/tail bits, truncated
  indexes, all 17 fingerprint bytes, and relocation to a different file offset.
- All four focused run-make tests passed. The sparse fixture checks standalone
  `.rmeta` consumers, linked execution, public traits/generics/constants, and
  fixed-width private edits with stable public hashes, incremental verification,
  and downstream executable assertions.
- All real-crate preparations, incremental dev builds, and measured calls passed.
- Final-head Linux and macOS compiler/regression CI and formatting/tidy passed;
  all 11 [current-head checks](github-checks.json) succeeded.
- The final-head ARM64 CodSpeed compiler build and all six benchmark shards
  passed. Their 22 cases include ty, uv, syn, serde, regex, and bitmaps, with
  ten measured rounds per case. Matching fixture manifests and compiler
  configuration were verified across baseline/head artifacts. Median elapsed
  changes range from -1.186% to +0.575%; ty is +0.037% and uv +0.283%. The
  geometric mean is -0.224%, but these separate CI runs do not establish that
  precise speedup. See [CI results](ci-results.json), [provenance](ci-provenance.json),
  and [original artifacts](ci-raw.json.gz). The CI corpus uses matched Cargo
  1.97.1 with `-Zthreads=1` and `-Zincremental-verify-ich`, separately from
  the local four-case matrix using packaged Ohm Cargo. CI artifacts contain
  ten-round summary statistics, not individual-round vectors: median ratios
  and the geometric mean were recomputed from those reported medians.

The native/storage evidence was independently audited: exact sample counts and
counterbalance, no overlapping native invocations, normalized command/environment
equality, frozen compiler identities, within-arm output stability, and all
inode/category/subtree sums. The report rejects missing, duplicated, out-of-order,
or incorrectly instrumented samples before computing a completed comparison.

Where emitted, public API hashes are stable within each compiler arm. Cross-arm
hashes are not compared for equality: compiler-derived crate identities differ
between arms.
These checks do not constitute the entire Rust compiler test suite or every
supported host architecture.

## Revisions and reproduction

- Baseline: `d37229159e545bbdec917dd7efdf5d26da40d2bc` (Ohm release-9 base).
- Measured compiler implementation: `54d6de980917d03fd2a003b6b7f6d3d46ad3d63e`.
- PR source/harness head: `4e23d8dabdd5e6cfd2df667ce9dc36b10cfaeea7`; its compiler
  production source is identical to the measured implementation.
- Preparation/initial-storage driver: `de2e243894f9003fe6a898fd1c242b2bb44811a0`.
- Native/count/final-accounting driver: `4e23d8dabdd5e6cfd2df667ce9dc36b10cfaeea7`.
- Cargo: the same packaged Ohm 1.98.0-9 binary for both arms.
- rustc-perf fixture: `c0301bc44d175b9b2c5442b25049475c39d7700c`, syn 2.0.101.
- Ruff fixture: `efcffd2178ce62e9951a53d4c50cadc225a0cfec`.

See [provenance.json](provenance.json) for exact source/archive/compiler hashes,
build flags, captured arguments, hardware, and environment. Compiler builds use
24 jobs, optimization level 3, no debug assertions/logging, 16 codegen units,
`thin-local` LTO, and the same external LLVM. Measured Rust invocations use one
frontend thread. The initial source/build preparation is excluded from timing.

The checked-in [driver](../../sparse_metadata.py) documents its input archives
and phases. Use `--candidate shipping-sparse --experiment shipping-sparse-metadata`
for every phase: `unpack`, `fetch`, `prepare`, `storage`, `native`,
`instructions --allow-concurrent-instructions --paired-instructions`, and `report`.
Run native phases alone. The [support scripts](support-scripts.json) retain the
exact campaign wrappers and storage stabilization procedure.

The [raw bundle](raw-evidence.json.gz) includes complete commands, sample records,
compiler/Valgrind logs,
source/build/correctness receipts, target inventories, and content-hash manifests.
The compact [samples](samples.json) include full profile sizes and SHA-256 hashes;
full Cachegrind profiles remain on the persistent task VM. Original prototype full
profiles were lost in an earlier `/tmp` reset; its exported receipts/logs remain
available. The shipping campaign's logical `/tmp` paths resolve to persistent
home-backed storage.
