# What uv-publish had to recompile after uv #22833

The ordinary first edit took **9.996963 s with signed Ohm −18** and **9.094546 s with optimized #306**. To reach twice the −18 speed, the current compiler would have to save **4.096 s** on this command. Each compiler has one ordinary diagnostic clock.

The command prepared the ordinary uv-publish unit-test executable. The real edit moved the public GitUrl fields behind an Arc. The native builds exercised the matching split rmeta/rlib client, its opposite-state failure and two actual uv-publish tests outside the measured build. Each complete visit had 501 Cargo artifacts, 12 dirty local first-edit units and settled no-ops. [Original uv edit](https://github.com/astral-sh/uv/commit/0bbcf757fdf19d43f52c38a5fc2e32a291c262ee)

    cargo test -p uv-publish --lib --features test --no-run --locked --offline -j8

| Native observation | Recompiled, byte-identical objects | IR + LLVM elapsed union | Whole-clock change | All admission checks |
|---|---:|---:|---:|---|
| front-and-leaf | 276 | 2.118 s | +8.26% | fail |
| middle | 144 | 0.769 s | +5.51% | fail |

Both diagnostic whole clocks stayed within 10% of ordinary, but selected individual processes crossed the bound. uv-client was 15.69% slower in the front group and uv-configuration was 16.17% slower in the middle. The exact uv-git-types and uv-pypi-types rlib hashes differed despite matching separate rmeta, every observed constituent incremental object, the test executable and behavior. In the middle group an inherited comparison also treated Python tuples and the JSON representation of exactly the same 12 dirty roles as unequal. The portable verifier checks both the original predicate and the semantic roster. Neither correcting that predicate nor the matching executable removes the independent timing and rlib failures.

Each row matches native CGU names in measureme, object hashes from the A/B incremental work-product files, and the selected Cargo rustc argv. The elapsed unions cover IR translation, LLVM optimization and object emission on each actual observation clock. Adding these across separate fresh builds can double-count overlapping work.

The preceding terminal-only native stream found that 119 of 129 newly compiled objects were identical. Of those, 106 first encountered a comparable changed upstream_monomorphizations_for input, 109 had a changed coarse provider value somewhere, and 105 saw it as the only comparable changed query kind. Some other inputs remained unresolved. The four-role front observation preserved full native events and work-product indexes, but its crash discarded its private per-CGU dependency graphs. The old terminal edges tell us nothing decisive about those missing graphs.

The fresh middle batch retained ordered dependency values. Across its newly compiled, equal-byte objects the first differing comparable saved inputs were upstream_monomorphizations_for 104, none comparable 25, fn_abi_of_instance_no_deduced_attrs 13, layout_of 2. A changed graph input does not prove that it alone forced LLVM or that a narrower input would be identical. The existing Ohm draft includes a per-instance provider lookup and native provider-removal regression. Its three full-debug pairs on a different uv edit reduced elapsed time 6.9%. It reports an effectively flat result under the current line-table defaults. [Ohm #138](https://github.com/astral-experiments/ohm-rust/pull/138)

The registered conservative screen leaves **8.441 s** of optimistic room, compared with the required **4.096 s**. It charges an uncovered or disqualified process its entire ordinary window. The largest qualifying single-batch adverse equal-object union is **0.000 s**. These captures establish no ordinary twofold speedup.

The packet includes the 001 failure and full surviving leaf graphs, the 002 failed attribution, the 003 closed raw streams with their genuine reducer exit 1, the 004 middle native output, and the ordinary baseline. Verification reconstructs both ordinary cases, all source-specific raw compiler events, mapped CGU object names, the first-edge projections actually retained, the observer/product checks and the final registered screen. This is local diagnostic evidence with zero scored benchmark rows.

## Raw trace archive

The shared benchmark workspace retains `uv-22833-publish-backend.zip` (81,816,012 bytes, SHA-256 `98f93393a9f6bdb027cb7c17f56b08c1651410c0e520e93132fa10ece73e90f1`). This directory carries the [archive member hashes](raw-member-sha256.txt), [independently reproduced result](verified.json), [ordinary process windows](ordinary-windows.json) and [registered profile decision](profile-decision.json). The separately warmed, unprofiled test of the existing #138 query has its own self-contained [two-pair packet](../../uv-22833-pr138-stage2/README.md).
