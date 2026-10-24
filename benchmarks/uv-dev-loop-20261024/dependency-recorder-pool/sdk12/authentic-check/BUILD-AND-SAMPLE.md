# Build and first-transition recipe

The archived baseline implements published #222 at `9d583ffee18d3cb132ac9b2a0279f449447bf0af`. Its compiled tree is `792811f296f5c16ba12c942ea916a79cfe83bf26`; publication changes only CI and removal of an empty test file. Both measured compilers embed `f93a752e3de7404107ea5908d65c8832aee5625a`. A version string alone does not identify either runtime; use `source-identities.json` and the two complete 74-file manifests.

In a fresh owned Rust checkout, keep the same physical source/build paths for both arms:

```sh
git checkout --detach f93a752e3de7404107ea5908d65c8832aee5625a
git apply /absolute/evidence/baseline-sdk12.patch
git add -A
git write-tree
# Baseline: 792811f296f5c16ba12c942ea916a79cfe83bf26
```

Build and archive the complete baseline runtime using `bootstrap.matched.toml` and `matched-build-recipe.json`, substituting their build/LLVM paths. The recipe uses stage1, opt3, ThinLTO, one codegen unit, debug1, no compiler incremental compilation, matching host std and the retained LLVM/stage0 inputs. No PGO or BOLT is applied. The study reused the authenticated #222 baseline archive. Its original producer used CPUs 0–23; the new candidate used 24–31 with the same 24-job setting, flags, tool inputs, config and canonical build paths. A fresh reproduction should keep those choices matched within its own pair; the manifests identify the historical measured binaries.

Apply `baseline-measured-to-published.patch`, then `compiled-candidate.patch`, and stage the result. Require tree `b3826635339bd4c9820935353d62d8203c7bd08a`. Rebuild and archive independently while retaining the common embedded HEAD. Only the driver changes; all 73 other archived files must match.

The optional `contract-tests.patch` adds the separate test-only tree `3e4399e87f5df582298bffee1c86cd00e94da46d`. Run its three release contracts with `./x test --stage 1 --keep-stage 1 --force-rerun --set rust.debug-assertions=false compiler/rustc_middle --test-args dep_graph::edges::tests::`, using the same config/build directory and 24-job setting. Run the ten existing incremental selectors in `correctness.json` with the archived production candidate. Preserve the measured runtime and restore the builder after testing.

Use authentic uv parent `cd050e4b18dfc4c5354ead52fa2b54756121b091` and child `73337b788c9057f6f3db1035d64be02b61b6ce7b` ([upstream #22049](https://github.com/astral-sh/uv/pull/22049)). The original selection chose this actual uv-pep440 correctness fix from a predeclared high-fanout shortlist before observations. All 83 recorded manifest/lock/config entries match; runtime behavior intentionally changes. `workload-selection.json` and `uv-upstream.patch` retain the selection and complete edit. Cargo.lock SHA256 is `74b8ae645079c1be4797acce1fd2361ecb15dddf0ba13b2f78d52bee919bd57f`.

Use released SDK12 Cargo, SHA256 `75eff38f65a16abae0a7b9603fc4fa02c846a0b6cb60a72d08b68393849b914b`, with the unchanged command:

```sh
cargo -Zohm-defaults=yes check --locked --offline -j 8 \
  --timings --message-format=json-render-diagnostics
```

`collect/` contains the unchanged selective wrapper, loader-adapter source, exact profiling-tool identities and a parameterized static environment. Its README describes their placement; it does not substitute for the complete collection guards.

Install the selective wrapper before warming. On each VM independently warm both compilers' parent states from source and registry archives only. Confirm a repeated parent command launches zero compilers. Preserve immutable parent templates covering source metadata and writable Cargo target/shared/package state. Activate each runtime at a common physical path while preserving its file identity.

Excluded native and instrumented first transitions establish all 39 unit identities, exact ordered compiler argv/environments and full output bytes within each arm. Concurrent independent units may arrive in different orders; preserve that raw order. Select only `--crate-name uv_client --crate-type lib`, input `crates/uv-client/src/lib.rs`, `--emit=dep-info,metadata`, without `--test`. Preserve the live jobserver and eight frontend threads. The accepted Callgrind/loader adapter profiles this child and all its threads, with child tracing disabled.

Before every pair, prepare and authenticate both independent pristine working copies outside the calls. Require zero-compiler parent freshness, apply the genuine child and execute once. The selected compiler has never seen the child in that bank. Keep strict entry/before/after activity checks. All three declared maintenance services must be inactive with their next deadlines at least 170 seconds away on VM 1 and 180 seconds away on VM 2. These fixed margins come from one blind complete-pair rehearsal per host plus the predeclared formula in `protocol.json`; the wait limit is 1,200 seconds. No service is modified. Restore the parent and fixed Cargo, converge both arms and park both banks at termination.

VM 1 uses `AB, BA, BA, AB, BA, AB, AB, BA`; VM 2 uses the inverse. Seeds are `20261024101` and `20261024102`. Both eight-pair blocks are fixed before either starts. The sole analyzer validates both complete/restored collections and final/self/stderr agreement before calculating either effect, using 200,000 paired bootstrap resamples per block. Either valid miss closes qualification; an incomplete block leaves both effects masked. There is no cap sweep, extension, substitution, pooling or automatic retry. The portable packet replays accepted counts and profiles, not private collection validation.
