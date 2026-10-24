# Build and first-transition recipe

The baseline compiler implements published #222 at `9d583ffee18d3cb132ac9b2a0279f449447bf0af`. Its archived measured tree is `792811f296f5c16ba12c942ea916a79cfe83bf26`; the published tree differs only in CI and deletion of an empty test file. Both measured compilers embed `f93a752e3de7404107ea5908d65c8832aee5625a`. Their 74-file runtime manifests differ only in the driver: all 73 other files, including rebuilt std, are byte-identical.

In a fresh owned Rust checkout, preserve one physical source/build path for both builds:

```sh
git checkout --detach f93a752e3de7404107ea5908d65c8832aee5625a
git apply /absolute/evidence/baseline-sdk12.patch
git add -A
git write-tree
# Expected baseline tree: 792811f296f5c16ba12c942ea916a79cfe83bf26
```

Build and archive the complete baseline runtime using `bootstrap.matched.toml` and `matched-build-recipe.json`, with their parameterized build/LLVM paths. The recipe uses stage1, opt3, ThinLTO, one codegen unit, debug1, no compiler incremental compilation, matching host std, and the pinned LLVM/stage0 inputs. No PGO or BOLT is applied. The new producer used CPUs 24–31 with the original 24-job setting; the archived baseline producer used 0–23. Tool bytes, flags, config and physical build paths were otherwise retained. Runtime manifests identify the actual measured pair; a version string alone does not.

Then apply `baseline-measured-to-published.patch` and `compiled-candidate.patch`, stage them and require tree `eb40fac7381f0af86c9d69ab07c10148177a34a2`. Rebuild and archive independently under the same embedded HEAD and recipe. `compiled-to-tested-snapshots.patch` changes only diagnostic-snapshot placement for the forced UI tests, yielding tested tree `eaa760a80c938c6e9d4693bfcc9aa9252cc5383b`. `candidate.patch` is the complete tested change against published #222. The separate `differential.patch` adds old/new answer assertions and must never be a performance arm.

The review/shipping tree is `fc5cb82737be5c7325e6eec097f16b389133483c`. Its only addition to the tested tree is `shipping-ci-only.patch`, selecting the two new precise-capturing UI files in the existing trait-solver CI step. `shipping.patch` is the full review change against published #222; all its non-CI patch bytes equal `candidate.patch`. This workflow overlay does not change the compiled or tested Rust source.

Use authentic uv parent `cd050e4b18dfc4c5354ead52fa2b54756121b091` and child `73337b788c9057f6f3db1035d64be02b61b6ce7b` ([upstream #22049](https://github.com/astral-sh/uv/pull/22049)). This is an actual uv-pep440 implementation fix, selected from a fixed high-fanout shortlist before performance observations. All 83 recorded manifest/lock/config entries match, but runtime behavior intentionally changes. `workload-selection.json` and `uv-upstream.patch` retain the selection and complete edit. Cargo.lock SHA256 is `74b8ae645079c1be4797acce1fd2361ecb15dddf0ba13b2f78d52bee919bd57f`.

Use released SDK12 Cargo, SHA256 `75eff38f65a16abae0a7b9603fc4fa02c846a0b6cb60a72d08b68393849b914b`, with this unchanged command:

```sh
cargo -Zohm-defaults=yes check --locked --offline -j 8 \
  --timings --message-format=json-render-diagnostics
```

Install the selective wrapper before warming. On each VM, independently warm each compiler's parent from source and registry archives only; no compiled state is imported. Confirm a repeated parent command launches zero compilers. Preserve a complete immutable parent template covering source metadata and writable Cargo target/shared/package state. Keep each runtime's identity stable when activating it at the common physical path.

Before collection, run excluded native and instrumented first transitions from fresh copies. Retain all 39 compiler contexts, ordered environments, diagnostics and exact per-arm outputs. Select only `--crate-name uv --crate-type lib`, input `crates/uv/src/lib.rs`, `--emit=dep-info,metadata`, without `--test`. Preserve the live jobserver and eight frontend threads. The accepted compiler-only Callgrind/loader adapter profiles this child, including all its threads, with child tracing disabled. Fresh namespace components are bound explicitly; no fabricated equal identity or old output exception is inherited.

Before each pair, materialize and authenticate both independent pristine writable banks outside the calls. Require zero-compiler parent freshness, apply the exact child edit and run the command once. That selected compiler has never seen the child in its current bank. Keep strict entry/before/after activity checks and the fixed 270-second maintenance admission between complete pairs; missing or transitioning timer information never admits work. At block end restore the parent and fixed Cargo, converge both arms and park both banks.

Host 1 orders are `AB, BA, BA, AB, BA, AB, AB, BA`; host 2 uses the inverse. Seeds are `2026102497` and `2026102498`. Both eight-pair blocks are fixed before either starts. The sole cohort analyzer validates both complete collections, restoration and final/self/stderr count agreement before calculating either effect. It uses 200,000 paired bootstrap resamples per block. A valid miss on either machine ends qualification; an incomplete block leaves both effects masked. This portable packet replays the accepted counts and profiles, not the private collection checks.

The original seeds 2026102495/96 cohort is wholly excluded after the unchanged post-command occupancy guard detected maintenance on one machine. Its completed partner also remains masked. Before the one final replacement cohort, both machines must be restored and freshly admitted, with kube-proxy, Azure-session and Teleport services inactive and each next deadline at least 270 seconds away. The original 1200-second wait limit and all within-pair quiet checks remain unchanged. The replacement uses the already authenticated cold-parent templates; every sample still begins from an independent pristine parent copy. Any failure of the final cohort closes qualification.
