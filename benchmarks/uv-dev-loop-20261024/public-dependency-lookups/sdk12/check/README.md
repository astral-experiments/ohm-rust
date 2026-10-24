This packet replays two fixed instruction-count confirmations of the private-dependency lookup short circuit on SDK12 plus the three lazy worker caches from #218. The endpoint is the ordinary uv-library compiler invocation captured from a warm dirty `cargo check --locked --offline -j8`, with Ohm defaults enabled. Each profile includes that rustc process's eight frontend threads and in-process procedural macros; compiler children are uninstrumented.

Run the portable numerical replay with Python 3:

```sh
python3 reproduce.py --profiles profiles
```

The reader verifies the core checksums and all 32 compressed profiles, then requires final Callgrind Ir to equal both the sum of self costs and the retained stderr counter. It reproduces the fixed schedules, paired statistics, 200,000 resamples, seeds 2026102401 and 2026102402, and each block's eight-win and 99%-lower-bound-above-0.1% gates. The five reader/statistics functions are unchanged from the previously used compiler replay. Per-order results and exact two-sided sign tests remain in the result records.

Only command-header and pathname definitions in the profiles are sanitized. Function names, compressed references, call edges, positions, costs, totals, and every other record stay exact. Original and derivative hashes are retained. Private environments, host observations, transport records, and the reverse path map are omitted. Numerical replay does not independently reconstruct those private collection-validity checks.

The change returns the existing public result immediately when `private_dep == Some(false)`, before consulting the `--extern` map. The original match already gives this case precedence over either command-line value. The other cases, actual crate-location lookup, and the caller's public/private state updates remain unchanged. Both preserved compilers passed nine privacy UI executions, including both direct-first and transitive-first revisions, and one extern-renaming run-make test, with zero failures or ignored cases.

The original source screen found no qualifying opportunity at its 2% threshold. A separate, prospectively bounded exception admitted this small candidate only after an excluded diagnostic found `Some(false)` in 26,362 of 30,593 privacy decisions (86.17%), above the required 75%. The 135 separate crate-locator lookups are outside that bypass. `eligibility.json` preserves the original no-go and the revised one-candidate decision; the earlier function cost was an attribution ceiling, not a wall-time prediction.

Both compilers use the same optimized developer-build recipe, source/build paths, freshly built matching standard library, fixed SDK12 Cargo, and shared auxiliary bytes. Exactly the rustc driver differs among 74 runtime files. Each compiler has its own independently warmed state, activated at common paths. Cargo's intentional compiler-specific namespaces remain distinct and are compared only at the bound path positions. The published source adds only a CI selector beyond the measured tree; the measured Rust and test blobs are exact.

One offline analyzer invocation stopped at output-directory creation because its private parent directory was absent, before reading any evidence. Creating that parent required no sample, input, analyzer, count or math change. The stop remains retained privately.

The claim is limited to this dirty-check compiler endpoint. This packet makes no whole-Cargo or native wall-time claim. A separately frozen test-build replication is reported independently if it passes its own gates.
