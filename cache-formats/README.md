# Incremental-cache compatibility for Ohm Rust #296

The two repaired Linux compilers loaded their own incremental state and rejected every tested incompatible writer. This held for both the ordinary local version string `1.99.0-dev` and an independently seeded `rustc-perf` version. The tests exercised real reads and writes at the same compiler and cache paths. They measured correctness only.

```sh
sha256sum -c SHA256SUMS
python3 -ISB verify.py mandatory.zip
```

The ZIP is 452,816 bytes, SHA-256 `ed26eb4ff05e70577bd6b5516a6a14a94e8b4b69a3fcfaae8991ab238bd55255`. The verifier prints each directed result, full compiler hashes, and the independent source-collision replay. It uses the Python standard library. The archive contains the complete Git-applicable patch series under `source/review-{T1,T3}.patch`, both one-number repairs, source rosters, manifests, native compiler commands and outputs. It also contains selected kernel trace lines from every execution of the separately linked run-make readers, with the original full-trace hashes. Private SDK and working directory prefixes in those trace lines are replaced with `$SDK` and `$WORK`. The migration test itself deliberately used the fixed names `/tmp/sdk` and `/tmp/w`.

## Source and images

The review stack stays on the named base `556a8364f1485a03be31b1a50c250d63198a943d`. T1 becomes `97ad3866` and T3 becomes `26f57475`. Each tree differs from its previously reviewed counterpart only in the format number in `compiler/rustc_incremental/src/persist/file_format.rs`. The legacy SDKs below were built on the compiler-equivalent earlier base. Its rebase to the named base changed the Cargo gitlink. The native experiments compared local Linux stage1 compilers with the same unbranded compiler version. [PR #296](https://github.com/astral-experiments/ohm-rust/pull/296)

| Format | Native build commit | Actual rustc driver SHA-256 prefix |
|---|---|---|
| 8, original base | `625207d2` | `125fdd522d19` |
| 9, previous T1 | `6d997cb8` | `c95f12f07e7a` |
| 10, previous T3 | `3334fcb9` | `9793bfacc271` |
| 11, repaired T1 | `97ad3866` | `d043561dbbec` |
| 12, repaired T3 | `26f57475` | `45e1a53eea14` |

T1 and T3 originally occupied formats 9 and 10. Other Ohm heads then used these numbers for different dep-node rosters. Matching the entire compiler version admits the old headers in both directions. The preserved isolation assertion failed before repair and passed afterward. In a real 2,240-node T1 history, 1,133 nodes have a tag with a different meaning under the intermediate v9 roster. In a 2,247-node T3 history, 1,140 nodes have a tag with a different meaning under the integrated v10 roster. For example, seven recorded `trait_impl_receiver_adt` nodes would be read as `dyn_compatibility_violations`. These counts replay the original graph census against both actual source tables. The original assertion checked header admission, without executing either conflicting compiler on the other's history. The earlier Darwin Diesel crash did not retain its incoming graph or previous writer, so the cause of that crash remains unknown.

## Native installs

Rows identify the writer and columns the reader. Each **L** loaded and each **R** rejected the restored graph in both new trials. An asterisk marks a rejection demonstrated only in the earlier, independently retained unforced 8/9/10 matrix. The new studies each contain 19 directional installs, including five own-image loads and fourteen cross-format rejections. They also include a genuine empty-cache control for each compiler. The forced trial set both `RUSTC_FORCE_RUSTC_VERSION` and `RUSTC_FORCE_INCR_COMP_ARTIFACT_HEADER` to `rustc-perf`; the unforced trial left both unset.

| Writer / reader | 8 | 9 | 10 | 11 | 12 |
|---|---|---|---|---|---|
| 8 | L | R* | R* | R | R |
| 9 | R* | L | R* | R | R |
| 10 | R* | R* | L | R | R |
| 11 | R | R | R | L | R |
| 12 | R | R | R | R | L |

Each cell restored the writer's same original A seed independently for each observation. The unchanged program printed `11`. A changed method returned `17`. Moving the trait implementation to the other type produced the same full E0277 diagnostic and primary span as a clean build by that reader. Complementary `-Zassert-incr-state=loaded` and `not-loaded` invocations establish admission or rejection from the specific rustc diagnostic, compiler exit and successful counterpart. The stored graph, query-cache and work-product headers include each file's hash and actual version.

The migration workers identified the launcher and resolved driver/LLVM with the dynamic loader, hashed them during the actual seed, and checked the same file identity across that reader's installs. Separate run-make traces show the driver and LLVM mappings in real compiler children and the reader binaries' stdout for both solvers. Repaired T1 exercised four revisions per solver; repaired T3 exercised seven.

The initial mandatory controller looked for an SDK manifest hash in a summary that omitted it and stopped before any migration worker ran. The successful controller took it from `available.json` inside each earlier digest-bound native QA ZIP and rechecked all 73 SDK members. The failed lookup and corrected binding remain in the evidence. Seven negative verifier controls separately changed the seed, selected image, compiler flag, version environment, program output, failure status or cache diagnostic. All were rejected.

The independent Linux c517 comparison and its commands are in [INTEGRATED.md](INTEGRATED.md).
