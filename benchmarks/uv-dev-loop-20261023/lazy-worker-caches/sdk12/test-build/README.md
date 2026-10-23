This packet replays the separately prescribed test-build replication for lazy allocation of three worker-local caches in the SDK12 compiler. It measures the ordinary uv-library rustc invocation captured from a warm dirty `cargo test --no-run --locked --offline -j8`, with Ohm defaults enabled. Each profile includes that process's eight frontend threads and in-process procedural macros; children are uninstrumented. No tests execute.

Run the portable replay with Python 3:

```sh
python3 reproduce.py --profiles profiles
```

The reader verifies core checksums and all 32 compressed profiles, then requires final Callgrind Ir to equal both the sum of self costs and the retained stderr counter. It reproduces the fixed schedules, paired statistics, 200,000 resamples, seeds 2026102383 and 2026102384, and each block's eight-win and 99%-lower-bound-above-0.1% gates. The five reader/statistics functions are unchanged. Per-order results and exact two-sided sign tests remain in the records. The replication was frozen before the primary check effects and was conditional on both check blocks passing; results are not pooled across endpoints.

Only command-header and pathname definitions in profiles are sanitized. Function names, compressed references, call edges, positions, costs, totals and every other record stay exact. Original and derivative hashes are retained. Private environments, host observations, transport records and the reverse path map are omitted. Numerical replay does not reconstruct private collection-validity checks.

The candidate changes allocation for the supertrait, clause-instantiation and function-signature-instantiation caches. The other four worker caches and all lookup/key/replacement bodies stay unchanged. The motivating 56-instance usage probe was a check workload; this packet does not claim that the same usage counts were observed during test-build compilation. Used families still allocate on first access and pass through the lazy-state check; these confirmations do not establish performance for other workloads that use those caches heavily. Correctness covers two compact storage tests, five incremental revisions and a separate 32-case workload exercising all three real paths, with matching positive metadata and three-family negative diagnostics. The 16,385-case generator was not executed or included in the shipping tree.

The two optimized developer compilers use the same build recipe, physical source/build paths, freshly built matching standard library, fixed released SDK12 Cargo and auxiliary bytes. Exactly the driver differs among 74 runtime files. Each compiler has independently warmed state at common active paths. Compiler-specific namespaces are kept intact and compared only at the enumerated positions.

An initial null host binding was rejected before output creation or workload execution. A prospective host-only correction admitted the same observed local host. The first complete block's offline analyzer then stopped before raw gates or effects because an authority-dependent read preceded its assignment. Moving the two existing reads preserved all guards and the complete statistics tail; the same recorded samples were analyzed in a fresh directory. These stops, a refused binding-smoke invocation and the read-only executable-accounting stop remain retained. No measured sample was rerun or excluded.

Preflight retained five executable pairs and 200 ordered native rlib objects. Native object bytes and allocated executable payloads match except build-ID descriptors; namespace/debug differences and one exact `.rodata` section-header entry-size difference are recorded. That read-only accounting creates no general ELF mask or sampled executable exception. Instruction samples retain strict per-arm rmeta identities and the existing rlib rule permitting only identified invocation filenames while preserving native payloads. Full serialized metadata and full DWARF equivalence are not claimed.

The claim is limited to this isolated dirty test-build compiler endpoint. It is not a whole-Cargo, native wall-time, test-execution or cold-build result. Primary check evidence is reported separately.
