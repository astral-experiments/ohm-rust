The portable command in README.md replays the accepted evidence. Fresh sampling additionally requires a task-owned fixture and fresh machine/input admission; the following records describe the actual build and sampling lifecycle.

Start from Rust commit `f93a752e3de7404107ea5908d65c8832aee5625a`. Build the baseline at one fixed source/build path using `bootstrap.matched.toml` and `matched-build-recipe.json`, replacing the explicit `<BUILD_ROOT>` and `<LLVM_ROOT>` placeholders. The backend is the pinned Rust LLVM 23.1.1 build. The child-only environment includes its library directory; it must not leak into workload compiler invocations. The canonical build is:

```sh
taskset -c 0-23 ./x build --config /absolute/owned/bootstrap.matched.toml \
  --build-dir /absolute/owned/build --stage 1 -j 24 library
```

Preserve the complete baseline runtime before applying `candidate.patch` at that same source path. Keep the authentic SDK12 HEAD metadata common, record the candidate patch/tree separately, and rebuild matching standard-library outputs for the candidate. Preserve both runtime trees independently. The optional rust-src/rustc-src convenience trees are excluded equally; runtime/std files remain complete. The measured settings are opt3, ThinLTO, one codegen unit, debug1, disabled compiler-build incremental/debug assertions, and no PGO. The only differing runtime file is the driver listed in `source-identities.json`.

Use uv commit `3d82a6ad5a7eab8c68236553b7846517767c4b61` in a fresh private checkout. Keep released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35` fixed. Create independent runtime, target, shared-cache, and package-home banks for each compiler. Seed only immutable dependency archives/index data; import no compiled state. Activate each bank by renaming it to the same physical paths, then warm and prove the original check command fresh. Preserve source mode and the original Cargo bytes.

The native preparation command is exactly:

```sh
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  check --locked --offline -j 8
```

Before every measured sample, the retained dirty fixture writes its equal-length BEFORE comment and runs native Cargo under the selected arm. The exec-only trace must show exactly the captured ordinary uv-library invocation with the same ordered environment, except unusable jobserver descriptors. Then the fixture writes the paired AFTER comment, and that exact rustc invocation is run once under Callgrind 3.22 with `--trace-children=no`, eight frontend threads, and the accepted environment-normalizer adapter. No wrapper or flag is inserted into Cargo's compiler command. The output remains metadata-only for this endpoint.

Each pair uses identical BEFORE/AFTER source bytes across arms. After each sample the original source bytes/mode are restored. At each block end, each touched arm is restored and converged with the original check command, a further traced check proves freshness, and all banks are parked. All source/runtime/Cargo/context/output/diagnostic and final/self/stderr count checks precede effects. Technical failure retains the entire attempt before effects; valid failure stops the fixed plan. There is no pair deletion or sample extension.

Block 1 uses AB, BA, BA, AB, BA, AB, AB, BA. Block 2 uses BA, AB, AB, BA, AB, BA, BA, AB. The exact gate and seeds are in `protocol.json`. The controller uses CPU 8 and measured rustc uses CPUs 0–7; other build/profile work is excluded from that window. Host observations are retained without filtering instruction-count results.

Cross-compiler namespace substitutions apply only to the enumerated comparison positions; actual argv, environments, compiler identities, and cache namespaces are never rewritten. Exact per-arm metadata identities are required. Metadata listings support the narrow comparison, but full serialized metadata equivalence is not claimed across the distinct compiler namespaces.
