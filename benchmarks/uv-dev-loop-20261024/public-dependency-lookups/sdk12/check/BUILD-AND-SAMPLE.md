The command in README.md replays the accepted numerical evidence. Fresh sampling additionally requires a private fixture and fresh machine/input admission; these records describe the actual build and sample lifecycle.

Start from Rust commit `f93a752e3de7404107ea5908d65c8832aee5625a`. Apply `baseline-sdk12.patch` at one fixed source/build path without changing HEAD. This reconstructs shipping parent tree `ceb7e58026e94c7a9ece088856f5f4d322195dc8`. The preserved baseline was built from measured #218 tree `e8f15a7a03e3dffe2a62ff81daab727cd28162d5`; their only difference is the separately supplied CI patch, with identical compiled Rust sources. Common embedded compiler metadata stays at the SDK12 commit.

Build with `bootstrap.matched.toml` and `matched-build-recipe.json`, replacing the explicit `<BUILD_ROOT>`, `<LLVM_ROOT>` and `<WORKSPACE_ROOT>` placeholders. The backend is the pinned Rust LLVM 23.1.1 build. Its library directory is set only in the build subprocess environment and must not leak into workload compiler invocations. The canonical build is:

```sh
taskset -c 0-23 ./x build --config /absolute/owned/bootstrap.matched.toml \
  --build-dir /absolute/owned/build --stage 1 -j 24 library
```

Preserve the complete baseline runtime before applying `candidate.patch` at that same source path. Record candidate tree `5fb5081a44fa86a406ce696a94b03dce7a9b9f74` separately, keep HEAD and the recipe fixed, and rebuild matching standard-library outputs. Preserve both runtime trees independently. Optional rust-src/rustc-src convenience trees are excluded equally; runtime/std files remain complete. Settings are opt3, ThinLTO, one codegen unit, debug1, disabled compiler-build incremental/debug assertions, and no PGO. Only the driver differs among the 74 runtime files.

Use uv commit `3d82a6ad5a7eab8c68236553b7846517767c4b61` in a fresh private checkout. Keep released SDK12 Cargo `fdd26156be98618e2e01a9e02703edac71b79a35` fixed. Create independent runtime, target, shared-cache and package-home banks for each compiler. Seed only immutable dependency archives/index data; import no compiled state. Activate each bank by renaming it to common physical paths, then warm and trace the original check command to prove it fresh. Preserve source mode and the original Cargo bytes.

The native preparation command is exactly:

```sh
taskset -c 0-7 /absolute/active/bin/cargo -Zohm-defaults=yes \
  check --locked --offline -j 8
```

Before every measured sample, the retained dirty fixture writes its equal-length BEFORE comment and runs native Cargo under the selected arm. The exec-only trace must show exactly the captured ordinary uv-library invocation with the same ordered environment, except unusable jobserver descriptors. The fixture then writes the paired AFTER comment. That exact rustc invocation runs once under Callgrind 3.22 with `--trace-children=no`, eight frontend threads and the accepted environment-normalizer adapter. No wrapper or extra flag is inserted into Cargo's compiler command. This endpoint emits dep-info and metadata.

Each pair uses identical BEFORE/AFTER source bytes across arms. Original source bytes and mode are restored after each sample. At block end, each touched arm converges with the original check command, a further traced check proves freshness, and all banks are parked. All source/runtime/Cargo/context/output/diagnostic checks and final/self/stderr count agreements precede effects. Technical failure retains the whole attempt before effects; valid failure stops the fixed plan. There is no pair deletion or sample extension. Private execution and analysis parent directories must exist before the frozen controllers create their fresh output directories.

Block 1 uses AB, BA, BA, AB, BA, AB, AB, BA. Block 2 uses BA, AB, AB, BA, AB, BA, BA, AB. Exact gates and seeds are in `protocol.json`. The controller uses CPU 8 and measured rustc uses CPUs 0–7; other heavy build/profile work is excluded from that window. Host observations are retained without filtering instruction counts.

Cross-compiler namespace substitutions apply only at enumerated comparison positions. Actual argv, environments, compiler identities and cache namespaces remain unchanged. Exact per-arm metadata identities are required. Metadata listings support the narrow comparison, but full serialized metadata equivalence is not claimed across the distinct namespaces.
