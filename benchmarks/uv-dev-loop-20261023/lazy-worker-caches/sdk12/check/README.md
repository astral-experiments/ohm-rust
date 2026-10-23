This packet replays two fixed instruction-count confirmations for lazy allocation of three worker-local caches in the SDK12 compiler. The endpoint is the ordinary uv-library compiler invocation captured from a warm dirty `cargo check --locked --offline -j8`, with Ohm defaults enabled. Each profile includes that rustc process's eight frontend threads and in-process procedural macros; compiler children are uninstrumented.

Run the portable replay with Python 3:

```sh
python3 reproduce.py --profiles profiles
```

The reader verifies the core checksums and all 32 compressed profiles, then requires final Callgrind Ir to equal both the sum of self costs and the retained stderr counter. It reproduces the exact fixed schedules, paired statistics, 200,000 resamples, seeds 2026102381 and 2026102382, and each block's eight-win and 99%-lower-bound-above-0.1% gates. The five reader/statistics functions are unchanged from the previously used compiler replay. Per-order results and the exact two-sided sign tests remain in the result records.

Only command-header and pathname definitions in the profiles are sanitized. Function names, compressed references, call edges, positions, costs, totals, and every other record stay exact. Original and derivative hashes are retained. Private environments, host observations, transport records, and the reverse path map are omitted. Numerical replay does not independently reconstruct those private collection-validity checks.

The measured candidate changes allocation for the supertrait, clause-instantiation, and function-signature-instantiation caches. The other four worker caches and every lookup/key/replacement body stay unchanged. A separate descriptive probe observed all 56 worker/family instances and found no accesses to these three families in this check workload. Their combined eager storage was 15 MiB; that observation does not equate storage bytes with instruction or wall-time savings.

Correctness covers two compact storage tests, five incremental revisions, and a separate 32-case workload exercising all three real lookup paths. Positive metadata and the three-family negative diagnostics match between the two preserved compilers. The larger 16,385-case generator was retained externally and was not executed or included in the shipping tree.

Both compilers use the same optimized developer-build recipe, source/build paths, freshly built matching standard library, fixed SDK12 Cargo, and shared auxiliary bytes. Exactly the rustc driver differs among 74 runtime files. Each compiler has its own independently warmed state, activated at common paths; Cargo's intentional compiler-specific namespaces remain distinct and are compared only at the bound path positions.

One offline analysis initially stopped because the already-pinned statistics helper was missing from its declared path. Installing those exact bytes required no sample, input, source, or math change. The original stop is retained, and its raw-gate and metric-integrity records are byte-identical to the successful offline replay.

The claim is limited to this dirty-check compiler endpoint. This packet makes no whole-Cargo, native wall-time, or test-build claim. A separate test-build replication was frozen before the check results and is reported independently.
