The portable command in README.md replays the accepted records. Fresh sampling requires a private fixture and fresh input admission; these records describe the actual lifecycle.

Start from Rust `f93a752e3de7404107ea5908d65c8832aee5625a`. Build at one fixed source/build path using `bootstrap.matched.toml` and `matched-build-recipe.json`, replacing `<BUILD_ROOT>` and `<LLVM_ROOT>`. The backend is pinned Rust LLVM 23.1.1; its child-only build library environment must not leak into workload invocations.

```sh
taskset -c 0-23 ./x build --config /absolute/owned/bootstrap.matched.toml \
  --build-dir /absolute/owned/build --stage 1 -j 24 library
```

Preserve the baseline runtime before applying `candidate.patch` at the same source path. Keep authentic SDK12 HEAD metadata common, record the patch/tree separately, and rebuild matching std for the candidate. Preserve both runtimes independently. The optional rust-src/rustc-src convenience trees are excluded equally. Settings are opt3, ThinLTO, one codegen unit, debug1, disabled compiler-build incremental/debug assertions, and no PGO. Only the driver differs; exact identities are in `source-identities.json`.

Use fresh uv commit `3d82a6ad5a7eab8c68236553b7846517767c4b61` and fixed released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35`. Create independent runtime, target, shared-cache and package-home banks, seeded only with immutable source and dependency archives/index data. Import no compiled state. Activate each bank by renaming it to common physical paths. Warm and trace both original commands as fresh in each arm before sampling:

```sh
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  check --locked --offline -j 8
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  test --no-run --locked --offline -j 8
```

Before every sample, the retained fixture writes its equal-length BEFORE comment and runs the original native test-build command under the selected arm. Its exec-only trace must contain the exact captured ordinary uv-library invocation and ordered environment, apart from dead jobserver descriptors. The fixture then writes the paired AFTER comment. Run that exact rustc invocation once under Callgrind 3.22 with `--trace-children=no`, eight frontend threads and the accepted environment-normalizer adapter. No wrapper or flag changes Cargo's compiler command. The ordinary invocation retains `--emit=dep-info,metadata,link`; it is not the library-test or a cached link-only invocation.

Each pair uses identical BEFORE/AFTER source bytes across arms. Restore original source bytes/mode after each sample. At each block end, converge each touched arm with both original check and test-build commands, then park all banks. The runner preserves source/runtime/Cargo/context/output/diagnostic guards. Offline analysis checks the whole collection, every raw record, and final/self/stderr agreement before effects. Technical failure retains the complete attempt before effects; valid failure stops the fixed plan. There is no pair deletion, sample extension or automatic retry.

Block 1 is AB, BA, BA, AB, BA, AB, AB, BA; block 2 is BA, AB, AB, BA, AB, BA, BA, AB. Seeds and gates are in `protocol.json`. The controller uses CPU 8; measured rustc uses CPUs 0–7. Other build/profile work is excluded from that window. Host activity is observed without filtering instruction-count results.

Actual argv, environments, compiler identities and namespaces are never rewritten. Only bound namespace positions are substituted for comparison. Require exact per-arm rmeta bytes and exact ordered rlib native payloads; the established rlib comparison allows only the identified invocation filename changes. Metadata listings support a narrow hash-field comparison, not full serialized metadata equivalence across compiler namespaces. The fresh five-executable preflight is evidence only; it does not broaden measured output admission.
