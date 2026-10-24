# Dependency-read recorder reuse: uv-client check evidence

The compiler change retains up to eight finished dependency-read recorders instead of four. It leaves dependency membership, first-read order, epoch handling and wrapping unchanged.

This packet measures the ordinary `uv_client` library compiler during the **first** authentic uv #22049 parent-to-child `cargo check` transition. The metric is that process's final Callgrind instruction count, including its threads and in-process macros. It excludes Cargo and the other compiler processes. It establishes neither whole-command wall time nor test-build performance.

Replay with Python's standard library:

```sh
python3 -I -B reproduce.py
python3 -I -B reproduce.py --profiles profiles
sha256sum -c CHECKSUMS.sha256
```

`counts.json` retains all 32 counts and both eight-pair schedules. The first command reproduces each block's geometric reduction, paired 99% interval, exact sign test and fixed gate. The second also verifies every compressed profile and rederives final Ir from self costs and the retained profiler counter. Header summaries remain separate.

Both independent VM blocks and inverse schedules were fixed before collection. Both complete raw/restoration gates precede either effect calculation. Each block independently requires eight wins, a 99% lower reduction bound above 0.1%, and two-sided sign p below .01. There is no pooled substitute, cap sweep or sample extension.

Four extra retained buffers cost at most **10 MiB + 224 B for the observed uv-client node ranges**, excluding allocator overhead. This is not a general compiler peak-RSS bound; see `engineering-context.json`. The same-compiler cross-host observation in `namespace-observation.json` documents genuine metadata namespace sensitivity without weakening exact within-arm output checks.

Only command headers and file/object path definitions are sanitized. Function names, references, calls, positions, costs and all other records are unchanged. Private paths, environments and transport records are omitted. The portable replay does not reconstruct private freshness, host or output validation. `BUILD-AND-SAMPLE.md` gives the source/workload recipe; `SEMANTICS.md` explains the narrow change. Four blind headroom profiles and all historical excluded profiles are absent.
