# Ordering reachable nodes in the public API graph

Rustc already interns each node while recording its metadata dependency graph. The change keeps the interned index alongside the stable fingerprint when sorting reachable nodes. One pass builds the final vector and translation table. The previous implementation constructed a second hash map and looked up each node again. Equal fingerprints preserve their insertion order in both implementations.

## Ruff results

On the proposed commit and its exact parent, the dirty compiler used fewer instructions but the user waited longer for this build. Across 20 pairs, the whole command took 2.51% longer (95% interval 0.73–4.20% longer, two-sided balanced randomization p = 0.0128). Four of 20 candidate builds were faster. The whole dirty compiler used 0.4880% fewer instructions over 12 pairs (95% interval 0.4369–0.5343%, all twelve faster by this measure). This case was registered before the proposed compiler was built. The earlier elapsed-time wins have not reproduced on the proposed compiler.

Each row used 12 counterbalanced pairs of the entire dirty rustc process and 20 other pairs of ordinary whole-Cargo wall time. Percentages are mean paired log-ratio savings with a fixed-seed paired bootstrap interval. All 12 compiler pairs improved in each check and in the primary build. Eleven of 12 improved in the independent build. The two predeclared primary instruction tests both pass a Holm adjustment at 0.05. Their two-sided sign p-values were 0.000488 before adjustment. The confirming edit was selected before inspecting the primary counts. [Line-ending edit](https://github.com/astral-sh/ruff/commit/88d851449d2345e11da991794f3e9bfbbd666719) · [Independent comment-indentation edit](https://github.com/astral-sh/ruff/commit/83a3afc3156d73ae4296f3fa74ede28e4c72bfcd)

| Edit and command | Dirty rustc instructions saved | Paired 95% interval | Whole Cargo median, parent to candidate |
| --- | ---: | ---: | ---: |
| **Proposed stack** `ruff_source_file` build | 0.4880% | 0.4369–0.5343% | **383.0 → 395.5 ms** |
| Earlier stack `ruff_source_file` check | 0.3979% | 0.3974–0.3984% | 221.8 → 222.7 ms |
| Earlier stack `ruff_source_file` build | 0.4722% | 0.3936–0.5446% | 403.1 → 392.4 ms |
| Earlier stack `ruff_python_trivia` check | 0.3877% | 0.3870–0.3885% | 212.2 → 216.5 ms |
| Earlier stack `ruff_python_trivia` build | 0.3467% | 0.2288–0.4624% | 398.0 → 389.6 ms |

On the earlier compiler stack, the primary build's paired whole-command saving was 2.00% (95% interval 0.11–3.52%). The independent build also improved, by 2.19% (95% interval 1.27–3.16%) over 20 pairs. Eighteen pairs were faster on that edit. The two cohorts stay separate. Neither check established a wall-time improvement. The second check's paired estimate was 5.20% slower, with an interval from 13.42% slower to 0.43% faster. These were concurrent development hosts with fixed CPU affinity. The instruction measurement is the primary endpoint. The initial cohort covers a small library edit and its immediate development cycle.

Each arm trained Cargo from empty state, applied the historical production edit in place and built it through the ordinary Cargo entry point. Exactly one root compiler ran after the edit, and the first unchanged repeat launched zero compilers and zero linkers. The old source failed its real regression test and the edited source passed. The ordered test inventories and complete test executables agreed between the compilers. Every metadata output was checked against an independent empty target. Paths, contents, timestamps and all hardlink equivalence classes in the warm target and shared build directory were verified on restoration. The check on the second crate included 40 tests in its inventory and the first crate included 13.

For the separate Callgrind counts, both drivers replayed the same literal Cargo-produced parent rustc command with an independently restored parent prestate, environment and working directory. The launcher recreates the jobserver file descriptor numbers and the three idle tokens of the four-job build. It records the complete resulting output and whole-process Ir. Native Cargo runs used Cargo's own live jobserver. Each bare execution, consumer, or state check ran outside the timer.

Library archives can contain fresh rustc-generated member names on two otherwise identical compiles. For these archives the verifier checks the full object contents and the ordered symbol/member tables. The only recognized difference is the nonce in the GNU member-name table and the matching Rust metadata references. The entire ELF or metadata file is compared directly for those respective output types. The archive rule applies only to the benchmark comparison. Native linker scripts can observe these names.

## Compiler and correctness

The earlier optimized stage1 source pair was `722b9412` and `181a2fd0`, without release PGO or BOLT. Both arms have the same compiler version stamp, upstream bootstrap LLVM, 119 compiler-build commands, 53 standard-library files, signed Ohm `-19` Cargo and stock Wild. Only the compiler driver differs among the 55 Rust SDK files. The source patch has the same Git patch ID on this stack and on the proposed stack above the UTF-8 fix. [Measured Rust base](https://github.com/astral-sh/ohm-rust/commit/722b9412d2386e70d6cffba132544c0e3ae09cd7) · [UTF-8 fix](https://github.com/astral-sh/ohm-rust/pull/259)

The exact-stack replication used matched stage1 builds from `569aefbe` and `6ab241bb`, with signed `-19` Cargo and stock Wild. It ran on CPU cores 4–7. The earlier source pair ran on 20–23 of the same machine. Restore and compiler checks were identical on both pairs.

The compiler built from the proposed source commit `6ab241bb` and its corrected parent `569aefbe` also produced nine identical public hash notifications and nine byte-identical metadata files across ASCII, multibyte and partial-character spans. Both compilers passed the downstream proc-macro check, and their real Cargo and run-make executables matched byte for byte. A separate path-dependency fixture exercised a private body change, an invalid public associated-type change, a repair and a clean rebuild, with the same verdicts and outputs on both original arms. The companion Rust program checked 335,923 insertion-order, reachability and equal-fingerprint cases. A source audit covered 229 Ohm Rust PRs and 323 branch tips, with 331 distinct commits. All 15 versions of this particular source file on those tips still used the previous fingerprint-map sort.

## Verify the observations

```sh
sha256sum -c SHA256SUMS
python3 -IB verify.py
rustc +ohm --edition 2024 replay/verify-tie-order.rs -o /tmp/ohm-verify-tie-order
/tmp/ohm-verify-tie-order
```

The JSON files in `cases/` contain every paired count, native verdict, hardlink inventory and raw-profile checksum. With the original recording, add `--raw primary-build=/absolute/path/to/primary-build` to reopen all full compiler profiles and measurement receipts. The portable packet omits the original absolute-path raw bodies. `graph-order.patch` is the measured source change. `registrations/` contains the dated protocols and file manifests. Earlier attempts with flattened hardlinks or incomplete custody were excluded from these rows.

## Rerun on another machine

Use Linux x86-64, Python 3.11 or later, Valgrind, strace, Rustup's Cargo and rustc proxies, a signed Ohm 1.99.0-19 SDK and an executable work filesystem. For the original pair, build optimized stage1 from the public base, save a private complete SDK, apply `graph-order.patch` and rebuild in the same tree with the same version stamp. Set `PACKET`, `STOCK` and `OUT` to absolute paths. Rebuilding and vendoring a compiler requires substantially more space than replaying the Ruff edit.

```sh
set -eu
mkdir -p "$OUT"
cd "$OUT"
git clone --filter=blob:none https://github.com/astral-sh/ohm-rust.git rust
git -C rust fetch origin 722b9412d2386e70d6cffba132544c0e3ae09cd7
git -C rust checkout --detach 722b9412d2386e70d6cffba132544c0e3ae09cd7
git -C rust submodule update --init --recursive
(cd rust && python3 x.py --set rust.download-rustc=false --set llvm.download-ci-llvm=false \
    --set rust.lto=off build --stage 1 compiler/rustc library/std)
stage="$OUT/rust/build/x86_64-unknown-linux-gnu/stage1"
mkdir "$OUT/parent-sdk"
cp -aL "$stage/." "$OUT/parent-sdk/"
git -C rust apply --check "$PACKET/graph-order.patch"
git -C rust apply "$PACKET/graph-order.patch"
(cd rust && python3 x.py --set rust.download-rustc=false --set llvm.download-ci-llvm=false \
    --set rust.lto=off build --stage 1 compiler/rustc library/std)
set -- "$stage"/lib/librustc_driver-*.so
test "$#" = 1
cp -L "$1" "$OUT/child-driver.so"
git clone --filter=blob:none https://github.com/astral-sh/ruff.git ruff
git -C ruff checkout --detach 93526150df078af7dd8fc4226cdcc469539d2001
mkdir "$OUT/registry"
(cd ruff && CARGO_HOME="$OUT/registry" cargo +ohm fetch --locked)
taskset -c 4-7 python3 -IB "$PACKET/replay/run.py" \
    --ruff "$OUT/ruff" --parent-sdk "$OUT/parent-sdk" \
    --candidate-driver "$OUT/child-driver.so" --stock-sdk "$STOCK" \
    --cargo-home "$OUT/registry" --work "$OUT/confirm-build" \
    --crate ruff_python_trivia --mode build
```

Select a CPU set available on the machine. Choose either crate in the table, a new `--work` directory and `--mode check`, `build` or `test`. Add `--native-only` for the genuine dirty, unchanged, clean, regression-verdict and direct native checks without timing. For compilers built from the corrected source pair, choose `--source-pair fixed-stack` and supply their corresponding drivers. The native-only command was exercised on both original compilers with the comment-indentation check and on the two exact proposed-stack compilers with the line-ending test build. The latter compared all four explicit extern operands including both rlib/rmeta pairs, the complete linked ELF, test inventory, failure-to-pass transition and zero-link unchanged retry. Keep the observations from different compilers, machines and edits separate.
