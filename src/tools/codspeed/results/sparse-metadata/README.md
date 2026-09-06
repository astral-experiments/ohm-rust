# Sparse metadata tables

## Conclusion

The prototype reduces the four measured `.rmeta` files by 22–38%. It is worth
pursuing, but the measurements do not establish a performance-neutral change:
instruction counts increase slightly in the measured smaller crates, and ty's
native median increases by 1.7% without an instruction-count measurement. Resolve
that uncertainty before enabling the change broadly.

These are logical sizes of the individual emitted `.rmeta` files. Whole-target
physical allocation, APFS clone sharing, and embedded `.rlib` sizes were not
measured. The percentages must not be applied to total target-directory size.

## Results

Positive performance deltas mean more work or elapsed time. Native numbers are
medians of four samples per arm; Cachegrind `Ir` numbers are medians of two.

| Workload | Baseline `.rmeta` bytes | Sparse `.rmeta` bytes | Size change | Compiler `Ir` change | Native elapsed change |
| --- | ---: | ---: | ---: | ---: | ---: |
| syn 2.0.101 | 1,990,426 | 1,428,551 | -28.23% | +0.083% | +3.82% |
| `ruff_python_ast` | 5,255,775 | 4,041,222 | -23.11% | +0.060% | -0.06% |
| `ruff_python_parser` | 1,271,263 | 997,236 | -21.56% | +0.108% | -7.95% |
| `ty_python_semantic` | 20,604,209 | 12,707,903 | -38.32% | Not measured | +1.71% |

The syn and parser native samples vary substantially; their elapsed deltas should
not be interpreted as reliable regressions or improvements. AST's native medians
are effectively unchanged. Ty remains an unresolved performance concern. See
[results.json](results.json) for every value, median, and range, and
[samples.json](samples.json) for ordered observations and output hashes.

The compiler calls produce metadata and also read dependencies; in particular,
parser reads the candidate AST metadata and ty reads the candidate parser and AST
metadata. Counts include the entire compiler invocation, including parsing and
type checking, but do not aggregate child-process instructions. They do not
isolate encoding or decoding costs. All dependency
graphs were independently built with the corresponding compiler in separate
target directories, and captured `--extern` paths retain those arm-specific
artifacts. Each arm also uses its independently built stage-1 standard-library
metadata. The candidate therefore reads candidate-encoded dependencies and std;
it does not reuse the baseline's prepared metadata. Metadata-only measurements
do not cover dependency decoding performed later during code generation.

## Reproduction and provenance

The measured compiler implementation is commit
`91a57411c676e6c4b6962967c0209716ecd43541`, compared with
`d37229159e545bbdec917dd7efdf5d26da40d2bc` (Ohm release-9 base). Later commits on
this branch change only the measurement driver, tests, and this evidence; they do
not change the compiler implementation. The expanded run-make test is from
`11dd58244cb59b493538cba93113f7320eb5d9ab` and was run with that same built
candidate compiler.

CI at evidence commit `78d7d80f77c` passed formatting but rejected the inline
unit test: compiler unit tests must live in a `tests.rs` file or `tests` directory.
The follow-up moves the unchanged test body to `rmeta/table/tests.rs` behind
`#[cfg(test)] mod tests`. Production table code remains byte-for-byte identical
to the measured implementation. The relocated unit test was rerun; its receipt
and full log are retained in [test-relocation.json](test-relocation.json).

Use [the committed driver](../../sparse_metadata.py) and its
[procedure](../../sparse_metadata.md). The source identities are rustc-perf
`c0301bc44d175b9b2c5442b25049475c39d7700c` and Ruff
`efcffd2178ce62e9951a53d4c50cadc225a0cfec`. Both arms use the same packaged
Ohm 1.98.0-9 Cargo, matched stage-1 sysroots, external LLVM, one frontend thread,
and nonincremental metadata emission. The runtime requires
`LD_LIBRARY_PATH=/tmp/ohm-target-size-prototypes/llvm/lib`.

Native measurements used one warmup per arm followed by ABBA and BAAB, on an
otherwise idle dedicated Linux host. Instruction measurements used one native
warmup per arm followed by ABBA under Cachegrind with cache and branch simulation
disabled. Other independent instruction-count experiments shared the host during
the latter phase using `--allow-concurrent-instructions`; their overlap is
recorded, and instrumented elapsed times are not performance evidence.

[provenance.json](provenance.json) records source/archive hashes, exact compiler
and Cargo identities, the exact captured commands and environments, hardware,
build receipts, and driver hashes. Native measurements used driver commit
`54b5f695dde`; instruction measurements used `1bd018df54a`. Final command
normalization matched both arms for all four cases and source digests remained
unchanged. An earlier path-normalization diagnostic was corrected before any
measurements and remains in the raw logs.

The captured Ohm check commands disable public API hashing for the primary syn
and ty crates, while the AST and parser dependency commands enable it. This
reflects the captured defaults; no additional reuse or public-hash options were
inserted into the benchmark. Cross-toolchain public API hash equality is not asserted:
the independently built standard libraries have different compiler identities.
Hashes for repeated AST/parser emissions are stable within each compiler.

## Correctness

[validation.json](validation.json) and the raw logs record these successful
checks against the built compiler:

- One source unit test covers sparse/dense lookups, widths 1/2/8, empty tables,
  bucket boundaries through 257 entries, and truncated encoded ranges.
- Four run-make tests passed: the new sparse table fixture, `rmeta-preferred`,
  `metadata-only-crate-no-ice`, and `embed-metadata`.
- The expanded sparse run-make fixture passed with public traits, associated
  constants, generics, alignment, standalone metadata, and linked execution. It
  also checks private implementation edits 7→8→8→7 with stable public hashes,
  enabled metadata-prefix reuse, incremental verification, downstream metadata
  reads, and executable output assertions.
- Syn's seven incremental verification revisions passed using the
  [shared validation driver](https://github.com/astral-sh/ohm-rust/blob/4dc2eb14298f959dbde147d84a1ad9b71de4ec39/src/tools/target-size-prototypes/validate_rust.py).
- Preparation of the four real workloads and every measured invocation passed.

These are focused compiler and runtime checks, not a full compiler test suite or
cross-platform validation. The prefix test checks behavior with reuse enabled;
it does not assert an internal debug-log admission message.

## Raw evidence

[raw-evidence.json.gz](raw-evidence.json.gz) is a gzip-compressed JSON object
mapping original paths relative to the experiment root to their original UTF-8
contents. It retains preparation logs, exact invocation manifests, native and
Cachegrind samples and logs, build status receipts, and source/runtime validation
logs. Decode it with Python's `gzip` and `json` standard-library modules.

The original per-source-line Cachegrind profiles remain on the retained experiment
disk and are excluded from this compact bundle. Their SHA-256 hashes and sizes are
in `samples.json`; each compiler's raw `Ir` total is retained in its Valgrind log.
The bundle hash is recorded in `provenance.json`.
